//! Vorschauordner: Einzelne Dateien aus dem Archiv werden zum Ansehen nach
//! `$XDG_CACHE_HOME/querencia-archiv/vorschau-<pid>/<n>/` entpackt (Ordner
//! 0700, Datei schreibgeschützt). Beim Beenden räumt das Programm seinen
//! eigenen Ordner weg, beim Start die Reste beendeter Läufe.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use gtk::glib;

fn basis() -> PathBuf {
    glib::user_cache_dir().join("querencia-archiv")
}

fn eigener() -> PathBuf {
    basis().join(format!("vorschau-{}", std::process::id()))
}

/// Ein neuer, leerer Ordner für eine Vorschau.
pub fn neuer_ordner() -> Option<PathBuf> {
    let e = eigener();
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&e)
        .ok()?;
    (1..10_000).find_map(|n| {
        let p = e.join(n.to_string());
        fs::DirBuilder::new().mode(0o700).create(&p).ok().map(|_| p)
    })
}

pub fn schreibschutz(datei: &Path) {
    let _ = fs::set_permissions(datei, fs::Permissions::from_mode(0o444));
}

/// Eigenen Vorschauordner entfernen (beim Beenden).
pub fn aufraeumen() {
    let _ = fs::remove_dir_all(eigener());
}

/// Vorschauordner beendeter Läufe entfernen (beim Start).
pub fn alte_aufraeumen() {
    let Ok(liste) = fs::read_dir(basis()) else {
        return;
    };
    for e in liste.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix("vorschau-")
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if !Path::new(&format!("/proc/{pid}")).exists() {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}
