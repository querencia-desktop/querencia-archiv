//! Inhalt eines Archivs: die Einträge in Archivreihenfolge und daraus der
//! Ordnerbaum für Ordnerleiste und Dateiliste.
//!
//! Viele zip-Archive nennen Ordner nicht eigens („a/b/c.txt“ ohne „a/“ und
//! „a/b/“); solche Ordner entstehen im Baum von selbst. Kommt ein Pfad
//! mehrfach vor (an ein tar angehängt), gilt im Baum der letzte Eintrag –
//! wie beim Entpacken, wo der spätere den früheren überschreibt. Unsichere
//! Einträge (pfade.rs) hängen nicht im Baum, sondern stehen eigens in
//! [`Inhalt::unsicher`] und werden in der Wurzel mit Warnung gezeigt.

use std::collections::{BTreeMap, HashMap};

use crate::pfade::{self, Name, RelPfad, Unsicher};

/// Art eines Eintrags laut Archivkopf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Datei,
    Ordner,
    /// Symbolische Verknüpfung: wird angezeigt, aber nie angelegt.
    Verknuepfung,
    /// Gerätedateien, FIFOs u. Ä.: nur angezeigt.
    Sonstiges,
}

impl Art {
    /// Aus `st_mode` (S_IFMT-Bits).
    pub fn aus_modus(modus: u32) -> Art {
        match modus & 0o170000 {
            0o100000 => Art::Datei,
            0o040000 => Art::Ordner,
            0o120000 => Art::Verknuepfung,
            // manche zip-Archive tragen keinen Typ ein
            0 => Art::Datei,
            _ => Art::Sonstiges,
        }
    }
}

/// Ein Eintrag, wie libarchive ihn meldet.
#[derive(Clone, Debug)]
pub struct Eintrag {
    /// Name genau wie im Archiv (Schlüssel beim Entpacken einer Auswahl).
    pub roh: String,
    pub name: Name,
    pub art: Art,
    pub groesse: u64,
    /// Änderungszeit, Unix-Sekunden (0: unbekannt).
    pub geaendert: i64,
    /// Rechte (untere 12 Bit von `st_mode`).
    pub rechte: u32,
}

impl Eintrag {
    pub fn neu(roh: &str, modus: u32, groesse: u64, geaendert: i64) -> Eintrag {
        let mut art = Art::aus_modus(modus);
        // zip ohne Typbits: Ordner enden auf „/“
        if art == Art::Datei && roh.ends_with('/') {
            art = Art::Ordner;
        }
        Eintrag {
            roh: roh.to_string(),
            name: pfade::pruefen(roh),
            art,
            groesse,
            geaendert,
            rechte: modus & 0o7777,
        }
    }

    pub fn pfad(&self) -> Option<&RelPfad> {
        match &self.name {
            Name::Sicher(p) => Some(p),
            _ => None,
        }
    }

    pub fn unsicher(&self) -> Option<Unsicher> {
        match self.name {
            Name::Unsicher(u) => Some(u),
            _ => None,
        }
    }
}

/// Ein Ordner im Baum („“ = Wurzel).
#[derive(Clone, Debug, Default)]
pub struct Ordner {
    /// Namen der Unterordner, sortiert.
    pub ordner: Vec<String>,
    /// Dateien (Indizes in [`Inhalt::eintraege`]), in Archivreihenfolge.
    pub dateien: Vec<usize>,
    /// Eigener Ordnereintrag im Archiv, falls vorhanden (für das Datum).
    pub eintrag: Option<usize>,
}

/// Archivinhalt mit Baum.
#[derive(Clone, Debug, Default)]
pub struct Inhalt {
    pub eintraege: Vec<Eintrag>,
    /// Schlüssel ohne führendes oder abschließendes „/“, Wurzel = „“; sortiert,
    /// damit Unterordner direkt hinter ihrem Elternordner folgen.
    pub baum: BTreeMap<String, Ordner>,
    /// Indizes unsicherer Einträge.
    pub unsicher: Vec<usize>,
    /// Zahl der Dateien (ohne doppelte, ohne unsichere) und ihre Größe.
    pub dateien: usize,
    pub groesse: u64,
}

impl Inhalt {
    pub fn aus(eintraege: Vec<Eintrag>) -> Inhalt {
        let mut baum: BTreeMap<String, Ordner> = BTreeMap::new();
        baum.insert(String::new(), Ordner::default());
        let mut unsicher = Vec::new();
        // Pfad einer Datei → Index (der letzte gewinnt)
        let mut dateien: HashMap<String, usize> = HashMap::new();
        fn ordner_mit_eltern(baum: &mut BTreeMap<String, Ordner>, pfad: &str) {
            if baum.contains_key(pfad) {
                return;
            }
            baum.insert(pfad.to_string(), Ordner::default());
            let (eltern, name) = pfad.rsplit_once('/').unwrap_or(("", pfad));
            ordner_mit_eltern(baum, eltern);
            if let Some(e) = baum.get_mut(eltern) {
                e.ordner.push(name.to_string());
            }
        }
        for (i, e) in eintraege.iter().enumerate() {
            match &e.name {
                Name::Unsicher(_) => unsicher.push(i),
                Name::Leer => {}
                Name::Sicher(p) if e.art == Art::Ordner => {
                    ordner_mit_eltern(&mut baum, p.as_str());
                    if let Some(o) = baum.get_mut(p.as_str()) {
                        o.eintrag = Some(i);
                    }
                }
                Name::Sicher(p) => {
                    ordner_mit_eltern(&mut baum, p.eltern());
                    dateien.insert(p.as_str().to_string(), i);
                }
            }
        }
        // Dateien in Archivreihenfolge einsortieren, Doppelte nur einmal
        let mut index: Vec<usize> = dateien.into_values().collect();
        index.sort_unstable();
        let mut groesse = 0;
        for &i in &index {
            let e = &eintraege[i];
            if let Some(p) = e.pfad()
                && let Some(o) = baum.get_mut(p.eltern())
            {
                o.dateien.push(i);
                groesse += e.groesse;
            }
        }
        for o in baum.values_mut() {
            o.ordner.sort_by_key(|a| a.to_lowercase());
        }
        Inhalt {
            dateien: index.len(),
            groesse,
            eintraege,
            baum,
            unsicher,
        }
    }

    pub fn ordner(&self, pfad: &str) -> Option<&Ordner> {
        self.baum.get(pfad)
    }

    /// Alle Dateieinträge unter `ordner` (rekursiv), für Auswahl und
    /// Entfernen eines Ordners.
    pub fn dateien_unter(&self, ordner: &str) -> Vec<usize> {
        self.baum
            .iter()
            .filter(|(pfad, _)| {
                ordner.is_empty()
                    || pfad.as_str() == ordner
                    || pfad
                        .strip_prefix(ordner)
                        .is_some_and(|r| r.starts_with('/'))
            })
            .flat_map(|(_, o)| o.dateien.iter().copied())
            .collect()
    }

    /// Größe aller Dateien unter `ordner`.
    pub fn groesse_unter(&self, ordner: &str) -> u64 {
        self.dateien_unter(ordner)
            .iter()
            .map(|&i| self.eintraege[i].groesse)
            .sum()
    }

    /// Ein einziger oberster Ordner, in dem alles liegt („Hier entpacken“
    /// legt dann keinen weiteren Ordner an).
    pub fn ein_oberster_ordner(&self) -> Option<&str> {
        let w = self.baum.get("")?;
        (w.dateien.is_empty() && w.ordner.len() == 1 && self.unsicher.is_empty())
            .then(|| w.ordner[0].as_str())
    }

    /// Enthält Einträge, die beim Neuschreiben verloren gingen
    /// (Verknüpfungen, Sonstiges) oder unsicher sind.
    pub fn nicht_neu_schreibbar(&self) -> bool {
        !self.unsicher.is_empty()
            || self
                .eintraege
                .iter()
                .any(|e| matches!(e.art, Art::Verknuepfung | Art::Sonstiges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(roh: &str, g: u64) -> Eintrag {
        Eintrag::neu(roh, 0o100644, g, 1)
    }
    fn o(roh: &str) -> Eintrag {
        Eintrag::neu(roh, 0o040755, 0, 2)
    }

    #[test]
    fn baum_mit_impliziten_ordnern() {
        let i = Inhalt::aus(vec![
            d("a/b/c.txt", 10),
            o("a/d/"),
            d("e.txt", 5),
            d("./a/f.txt", 1),
            o("./"),
        ]);
        let namen: Vec<_> = i.baum.keys().cloned().collect();
        assert_eq!(namen, ["", "a", "a/b", "a/d"]);
        assert_eq!(i.ordner("").unwrap().ordner, ["a"]);
        assert_eq!(i.ordner("a").unwrap().ordner, ["b", "d"]);
        assert_eq!(i.ordner("a").unwrap().dateien, [3]);
        assert_eq!(i.ordner("a/b").unwrap().dateien, [0]);
        assert_eq!(i.ordner("").unwrap().dateien, [2]);
        // „a/d/“ hat einen eigenen Eintrag, „a/b“ nicht
        assert_eq!(i.ordner("a/d").unwrap().eintrag, Some(1));
        assert_eq!(i.ordner("a/b").unwrap().eintrag, None);
        assert_eq!(i.dateien, 3);
        assert_eq!(i.groesse, 16);
        assert_eq!(i.groesse_unter("a"), 11);
        let mut unter = i.dateien_unter("a");
        unter.sort();
        assert_eq!(unter, [0, 3]);
        assert_eq!(i.dateien_unter("").len(), 3);
        assert!(i.unsicher.is_empty());
        assert!(!i.nicht_neu_schreibbar());
    }

    #[test]
    fn praefix_ist_kein_ordner() {
        let i = Inhalt::aus(vec![d("ab/x", 1), d("a/y", 2), d("a b/z", 3)]);
        assert_eq!(i.dateien_unter("a"), [1]);
        assert_eq!(i.dateien_unter("ab"), [0]);
        assert_eq!(i.ordner("").unwrap().ordner, ["a", "a b", "ab"]);
    }

    #[test]
    fn doppelte_und_unsichere() {
        let i = Inhalt::aus(vec![
            d("x.txt", 1),
            d("../boese", 1),
            d("/etc/passwd", 1),
            d("x.txt", 7),
        ]);
        assert_eq!(i.ordner("").unwrap().dateien, [3]);
        assert_eq!(i.dateien, 1);
        assert_eq!(i.groesse, 7);
        assert_eq!(i.unsicher, [1, 2]);
        assert_eq!(i.eintraege[1].unsicher(), Some(Unsicher::Elternordner));
        assert_eq!(i.eintraege[2].unsicher(), Some(Unsicher::Absolut));
        assert!(i.nicht_neu_schreibbar());
        assert_eq!(i.ein_oberster_ordner(), None);
    }

    #[test]
    fn arten_und_oberster_ordner() {
        assert_eq!(Art::aus_modus(0o120777), Art::Verknuepfung);
        assert_eq!(Art::aus_modus(0o020644), Art::Sonstiges);
        assert_eq!(Art::aus_modus(0o644), Art::Datei);
        assert_eq!(Eintrag::neu("z/", 0, 0, 0).art, Art::Ordner);
        let i = Inhalt::aus(vec![d("projekt/a", 1), d("projekt/b/c", 1)]);
        assert_eq!(i.ein_oberster_ordner(), Some("projekt"));
        let i = Inhalt::aus(vec![d("projekt/a", 1), d("lose.txt", 1)]);
        assert_eq!(i.ein_oberster_ordner(), None);
        let v = Inhalt::aus(vec![d("a", 1), Eintrag::neu("l", 0o120777, 0, 0)]);
        assert!(v.nicht_neu_schreibbar());
    }
}
