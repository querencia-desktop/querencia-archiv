//! Pfadprüfung beim Entpacken: kein `..`, keine absoluten Pfade, keinen
//! Verknüpfungen folgen.
//!
//! Ein Archiv bestimmt die Namen seiner Einträge selbst. Querencia Archiv
//! schreibt nur in den gewählten Zielordner:
//!
//! - Namen mit `..` als Bestandteil und absolute Namen (`/etc/passwd`) gelten
//!   als **unsicher**. Ein Auftrag, der einen unsicheren Eintrag enthält,
//!   wird vor dem ersten Schreiben abgelehnt (entpacken.rs).
//! - `.`-Bestandteile und doppelte `/` fallen weg (`./a//b` → `a/b`).
//! - Symbolische Verknüpfungen aus dem Archiv werden nie angelegt; beim
//!   Anlegen der Ordner wird keiner bestehenden Verknüpfung gefolgt
//!   ([`ordner_anlegen`]), Dateien entstehen mit `O_NOFOLLOW` und
//!   `O_EXCL` ([`datei_anlegen`]). So kann auch eine Verknüpfung, die schon im
//!   Zielordner liegt, nichts aus ihm herausleiten.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Ein geprüfter relativer Pfad im Archiv: Bestandteile ohne `.`, `..` und
/// leere Teile, getrennt durch `/`, nie leer.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPfad(String);

/// Warum ein Name nicht entpackt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsicher {
    /// Beginnt mit `/`.
    Absolut,
    /// Enthält `..` als Bestandteil.
    Elternordner,
}

/// Ergebnis der Prüfung eines Namens aus dem Archiv.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    Sicher(RelPfad),
    Unsicher(Unsicher),
    /// Nur `.` oder `/`-Folgen ohne Inhalt (`./` in tar-Archiven): der
    /// Zielordner selbst, nichts zu tun.
    Leer,
}

impl RelPfad {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Letzter Bestandteil.
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Übergeordneter Ordner als Text („“ = Wurzel).
    pub fn eltern(&self) -> &str {
        self.0.rsplit_once('/').map(|(e, _)| e).unwrap_or("")
    }

    pub fn teile(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// Pfad ohne das Präfix `ordner` („a/b“ ohne „a“ → „b“); `None`, wenn
    /// der Pfad nicht in `ordner` liegt oder `ordner` selbst ist.
    pub fn ohne_praefix(&self, ordner: &str) -> Option<RelPfad> {
        if ordner.is_empty() {
            return Some(self.clone());
        }
        self.0
            .strip_prefix(ordner)
            .and_then(|r| r.strip_prefix('/'))
            .filter(|r| !r.is_empty())
            .map(|r| RelPfad(r.to_string()))
    }

    /// Hängt einen einzelnen, schon geprüften Namen an (für Ordnerinhalte
    /// beim Hinzufügen); `None` bei Namen mit `/`, `.` oder `..`.
    pub fn mit(&self, name: &str) -> Option<RelPfad> {
        let n = RelPfad::einzeln(name)?;
        Some(RelPfad(format!("{}/{}", self.0, n.0)))
    }

    /// Ein einzelner Dateiname als Pfad (`None` bei `/`, `.`, `..`, leer).
    pub fn einzeln(name: &str) -> Option<RelPfad> {
        (!name.is_empty() && name != "." && name != ".." && !name.contains('/'))
            .then(|| RelPfad(name.to_string()))
    }

    /// Ordnerpfad `ordner` (Anzeige-Ordner, „“ = Wurzel) plus `name`.
    pub fn in_ordner(ordner: &str, name: &str) -> Option<RelPfad> {
        let n = RelPfad::einzeln(name)?;
        if ordner.is_empty() {
            return Some(n);
        }
        match pruefen(ordner) {
            Name::Sicher(o) => o.mit(&n.0),
            _ => None,
        }
    }
}

impl std::fmt::Display for RelPfad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Prüft und normalisiert einen Namen, wie er im Archiv steht.
pub fn pruefen(roh: &str) -> Name {
    if roh.starts_with('/') {
        return Name::Unsicher(Unsicher::Absolut);
    }
    let mut teile = Vec::new();
    for t in roh.split('/') {
        match t {
            "" | "." => {}
            ".." => return Name::Unsicher(Unsicher::Elternordner),
            t => teile.push(t),
        }
    }
    if teile.is_empty() {
        Name::Leer
    } else {
        Name::Sicher(RelPfad(teile.join("/")))
    }
}

/// Legt `rel` (einen Ordner) unter `wurzel` an, Bestandteil für Bestandteil.
/// Ein vorhandener Bestandteil muss ein echter Ordner sein (keine
/// Verknüpfung); sonst Fehler `NotADirectory`. Gibt den Ordnerpfad zurück.
pub fn ordner_anlegen(wurzel: &Path, rel: &RelPfad) -> io::Result<PathBuf> {
    let mut p = wurzel.to_path_buf();
    for teil in rel.teile() {
        p.push(teil);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("{} ist kein Ordner", p.display()),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                match fs::DirBuilder::new().mode(0o755).create(&p) {
                    Ok(()) => {}
                    // Gleichzeitig angelegt: nur ein echter Ordner zählt.
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                        if !fs::symlink_metadata(&p)?.is_dir() {
                            return Err(e);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(p)
}

/// Übergeordneter Ordner einer Datei `rel` unter `wurzel`, angelegt wie bei
/// [`ordner_anlegen`] (keine Verknüpfung als Zwischenordner).
pub fn eltern_anlegen(wurzel: &Path, rel: &RelPfad) -> io::Result<PathBuf> {
    match pruefen(rel.eltern()) {
        Name::Sicher(e) => ordner_anlegen(wurzel, &e),
        _ => Ok(wurzel.to_path_buf()),
    }
}

/// Neue Datei, nie über eine Verknüpfung und nie über Vorhandenes.
pub fn datei_anlegen(pfad: &Path, modus: u32) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(modus)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(pfad)
}

/// Was am Zielort einer Datei schon liegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vorhanden {
    Nichts,
    /// Datei oder Verknüpfung: darf nach Rückfrage ersetzt werden (das
    /// Umbenennen ersetzt eine Verknüpfung selbst, folgt ihr nicht).
    Datei,
    /// Ein Ordner wird nie durch eine Datei ersetzt.
    Ordner,
}

pub fn vorhanden(pfad: &Path) -> Vorhanden {
    match fs::symlink_metadata(pfad) {
        Ok(m) if m.is_dir() => Vorhanden::Ordner,
        Ok(_) => Vorhanden::Datei,
        Err(_) => Vorhanden::Nichts,
    }
}

/// Freier Name „name“, „name (2)“, „name (3)“ … in `ordner`.
pub fn freier_name(ordner: &Path, name: &str) -> PathBuf {
    let p = ordner.join(name);
    if vorhanden(&p) == Vorhanden::Nichts {
        return p;
    }
    (2..)
        .map(|i| ordner.join(format!("{name} ({i})")))
        .find(|p| vorhanden(p) == Vorhanden::Nichts)
        .unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sicher(s: &str) -> String {
        match pruefen(s) {
            Name::Sicher(p) => p.0,
            n => panic!("{s}: {n:?}"),
        }
    }

    #[test]
    fn normale_namen_werden_normalisiert() {
        assert_eq!(sicher("a/b/c.txt"), "a/b/c.txt");
        assert_eq!(sicher("./a//b/./c"), "a/b/c");
        assert_eq!(sicher("ordner/"), "ordner");
        assert_eq!(sicher("Ä ö/ü ß.txt"), "Ä ö/ü ß.txt");
        // Punkte im Namen sind keine Elternordner
        assert_eq!(sicher("..versteckt/a..b/...txt"), "..versteckt/a..b/...txt");
        // Gegenschrägstriche sind unter Linux Teil des Namens
        assert_eq!(sicher("..\\..\\x"), "..\\..\\x");
        assert_eq!(pruefen("./"), Name::Leer);
        assert_eq!(pruefen(""), Name::Leer);
        assert_eq!(pruefen("."), Name::Leer);
    }

    #[test]
    fn ausbrueche_sind_unsicher() {
        for s in ["../x", "a/../../x", "a/..", "..", "./../x", "a/b/../c"] {
            assert_eq!(pruefen(s), Name::Unsicher(Unsicher::Elternordner), "{s}");
        }
        for s in ["/etc/passwd", "//x", "/", "/./a"] {
            assert_eq!(pruefen(s), Name::Unsicher(Unsicher::Absolut), "{s}");
        }
    }

    #[test]
    fn hilfsfunktionen() {
        let p = match pruefen("a/b/c.txt") {
            Name::Sicher(p) => p,
            _ => unreachable!(),
        };
        assert_eq!(p.name(), "c.txt");
        assert_eq!(p.eltern(), "a/b");
        assert_eq!(p.ohne_praefix("a").unwrap().as_str(), "b/c.txt");
        assert_eq!(p.ohne_praefix("").unwrap().as_str(), "a/b/c.txt");
        assert!(p.ohne_praefix("x").is_none());
        assert!(p.ohne_praefix("a/b/c.txt").is_none());
        // „a/bc“ liegt nicht im Ordner „a/b“
        assert!(sicher_p("a/bc").ohne_praefix("a/b").is_none());
        assert_eq!(RelPfad::in_ordner("a/b", "x").unwrap().as_str(), "a/b/x");
        assert_eq!(RelPfad::in_ordner("", "x").unwrap().as_str(), "x");
        assert!(RelPfad::in_ordner("", "..").is_none());
        assert!(RelPfad::in_ordner("", "a/b").is_none());
        assert!(RelPfad::in_ordner("../a", "x").is_none());
    }

    fn sicher_p(s: &str) -> RelPfad {
        match pruefen(s) {
            Name::Sicher(p) => p,
            _ => unreachable!(),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qa-pfade-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ordner_folgen_keiner_verknuepfung() {
        let w = tmp("link");
        let aussen = tmp("aussen");
        std::os::unix::fs::symlink(&aussen, w.join("weg")).unwrap();
        // „weg/x“ würde über die Verknüpfung nach außen führen
        let e = ordner_anlegen(&w, &sicher_p("weg/x")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotADirectory);
        assert!(!aussen.join("x").exists());
        // eine Datei als Zwischenordner ebenso
        fs::write(w.join("datei"), b"").unwrap();
        assert!(ordner_anlegen(&w, &sicher_p("datei/x")).is_err());
        // normaler Fall, auch wiederholt
        let o = ordner_anlegen(&w, &sicher_p("a/b")).unwrap();
        assert!(o.is_dir());
        assert_eq!(ordner_anlegen(&w, &sicher_p("a/b")).unwrap(), o);
        fs::remove_dir_all(&w).unwrap();
        fs::remove_dir_all(&aussen).unwrap();
    }

    #[test]
    fn dateien_nie_ueber_verknuepfungen() {
        let w = tmp("datei");
        let aussen = tmp("datei-aussen");
        fs::write(aussen.join("ziel"), b"alt").unwrap();
        std::os::unix::fs::symlink(aussen.join("ziel"), w.join("f")).unwrap();
        assert!(datei_anlegen(&w.join("f"), 0o644).is_err());
        assert_eq!(fs::read(aussen.join("ziel")).unwrap(), b"alt");
        assert_eq!(vorhanden(&w.join("f")), Vorhanden::Datei);
        assert_eq!(vorhanden(&w), Vorhanden::Ordner);
        assert_eq!(vorhanden(&w.join("nichts")), Vorhanden::Nichts);
        assert!(datei_anlegen(&w.join("neu"), 0o644).is_ok());
        assert!(datei_anlegen(&w.join("neu"), 0o644).is_err());
        fs::remove_dir_all(&w).unwrap();
        fs::remove_dir_all(&aussen).unwrap();
    }

    #[test]
    fn freie_namen() {
        let w = tmp("frei");
        assert_eq!(freier_name(&w, "a"), w.join("a"));
        fs::create_dir(w.join("a")).unwrap();
        fs::write(w.join("a (2)"), b"").unwrap();
        assert_eq!(freier_name(&w, "a"), w.join("a (3)"));
        fs::remove_dir_all(&w).unwrap();
    }
}
