//! Entpacken: alles oder eine Auswahl, mit Fortschritt, Abbruch und
//! Rückfrage bei vorhandenen Dateien.
//!
//! Querencia Archiv schreibt selbst (nicht `archive_write_disk`), damit jede
//! Regel aus pfade.rs gilt: Ein Auftrag mit einem unsicheren Eintrag wird
//! vor dem ersten Schreiben abgelehnt; Verknüpfungen und Gerätedateien aus
//! dem Archiv werden übersprungen und gezählt. Jede Datei entsteht zuerst
//! unter einem Hilfsnamen im Zielordner und wird erst vollständig an ihren
//! Platz umbenannt; ein Abbruch oder Fehler hinterlässt keine halbe Datei.
//! Rechte: höchstens `rwxr-xr-x`, setuid/setgid/sticky fallen weg, der
//! Besitzer darf immer lesen und schreiben.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use compress_tools::ArchiveContents;

use crate::lesen::{self, Fehler as LeseFehler, Stand};
use crate::modell::{Art, Inhalt};
use crate::pfade::{self, Name, RelPfad, Vorhanden};

/// Antwort auf „Datei existiert bereits“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Konflikt {
    Ersetzen,
    Ueberspringen,
    Abbrechen,
}

/// Rückmeldungen während des Entpackens (Hintergrundfaden → Oberfläche).
pub trait Rueckmeldung {
    /// `fertig` von `gesamt` Bytes, gerade bei `name`.
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, name: &str);
    /// `ziel` existiert schon als Datei; `neu` ist der Archivpfad.
    fn konflikt(&mut self, ziel: &Path, neu: &RelPfad) -> Konflikt;
}

/// Was entpackt wird: Index im Archiv → Pfad unter dem Zielordner.
#[derive(Clone, Debug, Default)]
pub struct Auswahl {
    pub ziele: HashMap<usize, RelPfad>,
    /// Summe der Dateigrößen (für den Fortschritt).
    pub gesamt: u64,
}

/// Abgelehnt: unsichere Einträge (rohe Namen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abgelehnt(pub Vec<String>);

impl Auswahl {
    /// Alles. Abgelehnt, sobald das Archiv einen unsicheren Eintrag hat.
    pub fn alles(inhalt: &Inhalt) -> Result<Auswahl, Abgelehnt> {
        let alle: Vec<usize> = (0..inhalt.eintraege.len()).collect();
        Auswahl::aus(inhalt, &alle, "")
    }

    /// Die Einträge `indizes` (Dateien und Ordnereinträge), mit allem darunter
    /// für Ordner, ohne das Präfix `ordner` (der in der Liste gezeigte
    /// Ordner: wer dort zwei Dateien wählt, bekommt genau diese zwei).
    pub fn aus(inhalt: &Inhalt, indizes: &[usize], ordner: &str) -> Result<Auswahl, Abgelehnt> {
        let unsicher: Vec<String> = indizes
            .iter()
            .filter(|&&i| inhalt.eintraege[i].unsicher().is_some())
            .map(|&i| inhalt.eintraege[i].roh.clone())
            .collect();
        if !unsicher.is_empty() {
            return Err(Abgelehnt(unsicher));
        }
        let mut a = Auswahl::default();
        let mut nimm = |i: usize| {
            let e = &inhalt.eintraege[i];
            if let Some(p) = e.pfad()
                && let Some(z) = p.ohne_praefix(ordner)
                && a.ziele.insert(i, z).is_none()
                && e.art == Art::Datei
            {
                a.gesamt += e.groesse;
            }
        };
        for &i in indizes {
            nimm(i);
            if inhalt.eintraege[i].art == Art::Ordner
                && let Some(p) = inhalt.eintraege[i].pfad()
            {
                for j in inhalt.dateien_unter(p.as_str()) {
                    nimm(j);
                }
                for (pfad, o) in &inhalt.baum {
                    if pfad
                        .strip_prefix(p.as_str())
                        .is_some_and(|r| r.starts_with('/'))
                        && let Some(j) = o.eintrag
                    {
                        nimm(j);
                    }
                }
            }
        }
        Ok(a)
    }

    /// Ganze Ordner aus dem Baum (auch ohne eigenen Eintrag im Archiv).
    pub fn ordner_aus(
        inhalt: &Inhalt,
        dateien: &[usize],
        ordnerpfade: &[String],
        anzeige: &str,
    ) -> Result<Auswahl, Abgelehnt> {
        let mut alle: Vec<usize> = dateien.to_vec();
        for o in ordnerpfade {
            alle.extend(inhalt.dateien_unter(o));
            for (pfad, x) in &inhalt.baum {
                let darin = pfad == o
                    || pfad
                        .strip_prefix(o.as_str())
                        .is_some_and(|r| r.starts_with('/'));
                if darin && let Some(j) = x.eintrag {
                    alle.push(j);
                }
            }
        }
        Auswahl::aus(inhalt, &alle, anzeige)
    }
}

/// Ergebnis eines Laufs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ergebnis {
    pub dateien: usize,
    pub ordner: usize,
    pub uebersprungen: usize,
    /// Verknüpfungen und Gerätedateien, die bewusst nicht angelegt wurden.
    pub nicht_angelegt: usize,
}

/// Fehler beim Entpacken.
#[derive(Debug)]
pub enum Fehler {
    Lesen(LeseFehler),
    /// Schreiben im Zielordner fehlgeschlagen.
    Ziel(io::Error, PathBuf),
    Abgebrochen,
}

impl From<LeseFehler> for Fehler {
    fn from(e: LeseFehler) -> Fehler {
        match e {
            LeseFehler::Abgebrochen => Fehler::Abgebrochen,
            e => Fehler::Lesen(e),
        }
    }
}

/// Ein Entpackauftrag.
pub struct Auftrag<'a> {
    pub archiv: &'a Path,
    pub roh: bool,
    pub ziel: &'a Path,
    pub auswahl: &'a Auswahl,
    pub passwort: Option<&'a str>,
    pub stand: &'a Stand,
}

/// Hilfsname für eine entstehende Datei im selben Ordner.
fn hilfsname(ziel: &Path) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = ziel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n = N.fetch_add(1, Ordering::Relaxed);
    ziel.with_file_name(format!(".{name}.qa-teil-{}-{n}", std::process::id()))
}

/// Rechte einer entpackten Datei aus den Rechten im Archiv.
pub fn dateirechte(rechte: u32) -> u32 {
    if rechte & 0o777 == 0 {
        return 0o644;
    }
    (rechte & 0o755) | 0o600
}

/// Entpackt `auftrag`. `erledigt` merkt sich fertige Einträge über Läufe
/// hinweg: Nach „Kennwort nötig“ fragt die Oberfläche und ruft erneut auf,
/// ohne schon Entpacktes ein zweites Mal anzufassen.
pub fn entpacken(
    auftrag: &Auftrag,
    erledigt: &mut HashSet<usize>,
    rueck: &mut dyn Rueckmeldung,
) -> Result<Ergebnis, Fehler> {
    let wurzel =
        fs::canonicalize(auftrag.ziel).map_err(|e| Fehler::Ziel(e, auftrag.ziel.into()))?;
    let wahl: HashSet<usize> = auftrag
        .auswahl
        .ziele
        .keys()
        .copied()
        .filter(|i| !erledigt.contains(i))
        .collect();
    let aktuell = Arc::new(AtomicUsize::new(usize::MAX));
    let a = aktuell.clone();
    let mut it = lesen::durchlaufen(
        auftrag.archiv,
        auftrag.roh,
        auftrag.passwort,
        auftrag.stand,
        move |i, _, _| {
            let ja = wahl.contains(&i);
            if ja {
                a.store(i, Ordering::Relaxed);
            }
            ja
        },
    )?;
    let mut erg = Ergebnis::default();
    let mut fertig: u64 = 0;
    let gesamt = auftrag.auswahl.gesamt;
    // Datei, deren Daten gerade kommen (`datei` leer: übersprungen)
    struct Offen {
        hilf: PathBuf,
        datei: Option<fs::File>,
        ziel: PathBuf,
        index: usize,
        rechte: u32,
        zeit: i64,
    }
    let mut offen: Option<Offen> = None;
    let aufraeumen = |o: &mut Option<Offen>| {
        if let Some(o) = o.take()
            && o.datei.is_some()
        {
            let _ = fs::remove_file(&o.hilf);
        }
    };
    let ergebnis = (|| -> Result<(), Fehler> {
        for teil in &mut it {
            if auftrag.stand.abgebrochen() {
                return Err(Fehler::Abgebrochen);
            }
            match teil {
                ArchiveContents::StartOfEntry(name, st) => {
                    let i = aktuell.load(Ordering::Relaxed);
                    let Some(rel) = auftrag.auswahl.ziele.get(&i) else {
                        continue;
                    };
                    let art = if auftrag.roh {
                        Art::Datei
                    } else {
                        crate::modell::Eintrag::neu(&name, st.st_mode, 0, 0).art
                    };
                    rueck.fortschritt(fertig, gesamt, rel.as_str());
                    match art {
                        Art::Ordner => {
                            pfade::ordner_anlegen(&wurzel, rel)
                                .map_err(|e| Fehler::Ziel(e, wurzel.join(rel.as_str())))?;
                            erg.ordner += 1;
                            erledigt.insert(i);
                        }
                        Art::Verknuepfung | Art::Sonstiges => {
                            erg.nicht_angelegt += 1;
                            erledigt.insert(i);
                        }
                        Art::Datei => {
                            let eltern = pfade::eltern_anlegen(&wurzel, rel)
                                .map_err(|e| Fehler::Ziel(e, wurzel.join(rel.as_str())))?;
                            let ziel = eltern.join(rel.name());
                            let mut weiter = true;
                            match pfade::vorhanden(&ziel) {
                                Vorhanden::Nichts => {}
                                Vorhanden::Ordner => {
                                    erg.uebersprungen += 1;
                                    weiter = false;
                                }
                                Vorhanden::Datei => match rueck.konflikt(&ziel, rel) {
                                    Konflikt::Ersetzen => {}
                                    Konflikt::Ueberspringen => {
                                        erg.uebersprungen += 1;
                                        weiter = false;
                                    }
                                    Konflikt::Abbrechen => return Err(Fehler::Abgebrochen),
                                },
                            }
                            let hilf = hilfsname(&ziel);
                            let datei = if weiter {
                                Some(
                                    pfade::datei_anlegen(&hilf, 0o600)
                                        .map_err(|e| Fehler::Ziel(e, ziel.clone()))?,
                                )
                            } else {
                                // übersprungen: Daten lesen und verwerfen
                                erledigt.insert(i);
                                None
                            };
                            offen = Some(Offen {
                                hilf,
                                datei,
                                ziel,
                                index: i,
                                rechte: st.st_mode,
                                zeit: st.st_mtime,
                            });
                        }
                    }
                }
                ArchiveContents::DataChunk(daten) => {
                    fertig += daten.len() as u64;
                    if let Some(o) = offen.as_mut()
                        && let Some(d) = o.datei.as_mut()
                    {
                        d.write_all(&daten)
                            .map_err(|e| Fehler::Ziel(e, o.ziel.clone()))?;
                        rueck.fortschritt(fertig.min(gesamt), gesamt, "");
                    }
                }
                ArchiveContents::EndOfEntry => {
                    if let Some(mut o) = offen.take() {
                        if let Some(d) = o.datei.take() {
                            let schliessen = (|| {
                                d.set_permissions(fs::Permissions::from_mode(dateirechte(
                                    o.rechte,
                                )))?;
                                if o.zeit > 0 {
                                    d.set_modified(
                                        UNIX_EPOCH + Duration::from_secs(o.zeit as u64),
                                    )?;
                                } else {
                                    d.set_modified(SystemTime::now())?;
                                }
                                d.sync_all()?;
                                drop(d);
                                fs::rename(&o.hilf, &o.ziel)
                            })();
                            if let Err(e) = schliessen {
                                let _ = fs::remove_file(&o.hilf);
                                return Err(Fehler::Ziel(e, o.ziel));
                            }
                            erg.dateien += 1;
                        }
                        erledigt.insert(o.index);
                    }
                }
                ArchiveContents::Err(e) => return Err(LeseFehler::from(e).into()),
            }
        }
        Ok(())
    })();
    aufraeumen(&mut offen);
    ergebnis?;
    it.close().map_err(|e| Fehler::from(LeseFehler::from(e)))?;
    rueck.fortschritt(gesamt, gesamt, "");
    Ok(erg)
}

/// Ziel von „Hier entpacken“: der Ordner des Archivs, wenn alles in einem
/// einzigen obersten Ordner liegt (und der noch nicht existiert), sonst ein
/// neuer Ordner „<Archivname ohne Endung>“ (bzw. „… (2)“).
pub fn hier_ziel(archiv: &Path, inhalt: &Inhalt) -> (PathBuf, bool) {
    let ordner = archiv
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if let Some(oben) = inhalt.ein_oberster_ordner()
        && pfade::vorhanden(&ordner.join(oben)) == Vorhanden::Nichts
    {
        return (ordner, false);
    }
    // eine einzelne Datei (auch gz ohne tar) braucht keinen eigenen Ordner
    let w = inhalt.ordner("");
    if inhalt.dateien == 1
        && inhalt.unsicher.is_empty()
        && w.is_some_and(|w| w.ordner.is_empty() && w.dateien.len() == 1)
    {
        return (ordner, false);
    }
    let name = archiv
        .file_name()
        .map(|n| crate::format::Format::ohne_endung(&n.to_string_lossy()))
        .unwrap_or_else(|| "Archiv".into());
    let name = match pfade::pruefen(&name) {
        Name::Sicher(p) if !p.as_str().contains('/') => name,
        _ => "Archiv".into(),
    };
    (pfade::freier_name(&ordner, &name), true)
}
