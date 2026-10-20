//! Formaterkennung und welche Formate Querencia Archiv schreiben kann.
//!
//! Gelesen wird jedes Format, das libarchive erkennt (Inhalt, nicht Name).
//! Die Erkennung hier dient der Anzeige, der Wahl des Schreibformats beim
//! Neuschreiben (Hinzufügen, Entfernen) und der Kommandozeile `--erstellen`:
//! zuerst die Kennbytes am Dateianfang, dann bei komprimierten Strömen der
//! Name (`.tar.gz` gegenüber `.gz`), zuletzt das Ergebnis von libarchive
//! (`roh`: ein einzelner komprimierter Strom ohne tar darin).

/// Archivformate (Container und Filter zusammen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    SiebenZip,
    Rar,
    /// Einzelne komprimierte Datei (`text.gz`), kein tar darin.
    Gz,
    Xz,
    Bz2,
    Zst,
    /// libarchive kennt mehr (cpio, ISO, CAB, LHA, ar …): nur lesen.
    Sonstiges,
}

/// Kompressionsfilter beim Schreiben.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Ohne,
    Gzip,
    Xz,
    Bzip2,
    Zstd,
}

/// Container beim Schreiben.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Zip,
    /// pax (POSIX.1-2001) mit UTF-8-Namen, ohne unnötige Erweiterungen.
    Tar,
    SiebenZip,
}

/// Formate, die „Neu“ und `--erstellen` anbieten (Umfang 0.1.0).
pub const ERSTELLBAR: [Format; 3] = [Format::Zip, Format::TarGz, Format::TarXz];

impl Format {
    /// Erkennt das Format aus den ersten Bytes (mindestens 262 für tar)
    /// und dem Dateinamen.
    pub fn erkennen(kopf: &[u8], name: &str) -> Format {
        let n = name.to_lowercase();
        let endet = |e: &[&str]| e.iter().any(|e| n.ends_with(e));
        let tar_name = |e: &[&str]| e.iter().any(|e| n.ends_with(e));
        if kopf.starts_with(b"PK\x03\x04") || kopf.starts_with(b"PK\x05\x06") {
            return Format::Zip;
        }
        if kopf.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
            return Format::SiebenZip;
        }
        if kopf.starts_with(b"Rar!\x1a\x07") {
            return Format::Rar;
        }
        if kopf.len() >= 262 && &kopf[257..262] == b"ustar" {
            return Format::Tar;
        }
        if kopf.starts_with(&[0x1F, 0x8B]) {
            return if tar_name(&[".tar.gz", ".tgz", ".taz"]) {
                Format::TarGz
            } else {
                Format::Gz
            };
        }
        if kopf.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
            return if tar_name(&[".tar.xz", ".txz"]) {
                Format::TarXz
            } else {
                Format::Xz
            };
        }
        if kopf.starts_with(b"BZh") {
            return if tar_name(&[".tar.bz2", ".tbz2", ".tbz", ".tb2"]) {
                Format::TarBz2
            } else {
                Format::Bz2
            };
        }
        if kopf.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            return if tar_name(&[".tar.zst", ".tzst"]) {
                Format::TarZst
            } else {
                Format::Zst
            };
        }
        // alte tar-Archive (v7) haben keine Kennung: nur nach Name
        if endet(&[".tar"]) {
            return Format::Tar;
        }
        Format::Sonstiges
    }

    /// Nach dem Lesen durch libarchive berichtigt: Ein komprimierter Strom
    /// mit tar darin ist `Tar…` (auch ohne passenden Namen), einer ohne tar
    /// (`roh`) die einzelne Datei.
    pub fn nach_lesen(self, roh: bool) -> Format {
        use Format::*;
        match (self, roh) {
            (Gz | TarGz, false) => TarGz,
            (Xz | TarXz, false) => TarXz,
            (Bz2 | TarBz2, false) => TarBz2,
            (Zst | TarZst, false) => TarZst,
            (Gz | TarGz, true) => Gz,
            (Xz | TarXz, true) => Xz,
            (Bz2 | TarBz2, true) => Bz2,
            (Zst | TarZst, true) => Zst,
            (f, _) => f,
        }
    }

    /// Format zu einem Archivnamen (für „Neu“ und `--erstellen`).
    pub fn aus_namen(name: &str) -> Option<Format> {
        let n = name.to_lowercase();
        [
            (".zip", Format::Zip),
            (".tar.gz", Format::TarGz),
            (".tgz", Format::TarGz),
            (".tar.xz", Format::TarXz),
            (".txz", Format::TarXz),
            (".tar.bz2", Format::TarBz2),
            (".tar.zst", Format::TarZst),
            (".tar", Format::Tar),
            (".7z", Format::SiebenZip),
            (".rar", Format::Rar),
        ]
        .into_iter()
        .find(|(e, _)| n.ends_with(e))
        .map(|(_, f)| f)
    }

    /// Übliche Endung.
    pub fn endung(self) -> &'static str {
        match self {
            Format::Zip => ".zip",
            Format::Tar => ".tar",
            Format::TarGz => ".tar.gz",
            Format::TarXz => ".tar.xz",
            Format::TarBz2 => ".tar.bz2",
            Format::TarZst => ".tar.zst",
            Format::SiebenZip => ".7z",
            Format::Rar => ".rar",
            Format::Gz => ".gz",
            Format::Xz => ".xz",
            Format::Bz2 => ".bz2",
            Format::Zst => ".zst",
            Format::Sonstiges => "",
        }
    }

    /// Kurzname für Statusleiste und Fehlermeldungen (keine Übersetzung).
    pub fn kurzname(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarXz => "tar.xz",
            Format::TarBz2 => "tar.bz2",
            Format::TarZst => "tar.zst",
            Format::SiebenZip => "7z",
            Format::Rar => "RAR",
            Format::Gz => "gzip",
            Format::Xz => "xz",
            Format::Bz2 => "bzip2",
            Format::Zst => "zstd",
            Format::Sonstiges => "",
        }
    }

    /// Container und Filter, wenn Querencia Archiv dieses Format schreiben
    /// kann (Neuschreiben beim Hinzufügen und Entfernen). 7z schreibt nur
    /// das Testwerkzeug; RAR kann keine freie Software erzeugen.
    pub fn schreibweise(self) -> Option<(Container, Filter)> {
        match self {
            Format::Zip => Some((Container::Zip, Filter::Ohne)),
            Format::Tar => Some((Container::Tar, Filter::Ohne)),
            Format::TarGz => Some((Container::Tar, Filter::Gzip)),
            Format::TarXz => Some((Container::Tar, Filter::Xz)),
            Format::TarBz2 => Some((Container::Tar, Filter::Bzip2)),
            Format::TarZst => Some((Container::Tar, Filter::Zstd)),
            _ => None,
        }
    }

    /// Hinzufügen und Entfernen erlaubt (Neuschreiben im selben Format).
    pub fn aenderbar(self) -> bool {
        self.schreibweise().is_some()
    }

    /// „Neu“ und `--erstellen` bieten das Format an.
    pub fn erstellbar(self) -> bool {
        ERSTELLBAR.contains(&self)
    }

    /// Archivname ohne Formatendung („bericht.tar.gz“ → „bericht“), für
    /// „Hier entpacken“ und die Vorschlagsnamen.
    pub fn ohne_endung(name: &str) -> String {
        let n = name.to_lowercase();
        let endungen = [
            ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.lz", ".tar.z", ".tgz", ".txz",
            ".tbz2", ".tbz", ".tzst", ".zip", ".tar", ".7z", ".rar", ".gz", ".xz", ".bz2", ".zst",
            ".iso", ".cpio", ".cab", ".lzh", ".lha", ".ar", ".deb", ".jar",
        ];
        for e in endungen {
            if n.ends_with(e) && n.len() > e.len() {
                return name[..name.len() - e.len()].to_string();
            }
        }
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_kopf() -> Vec<u8> {
        let mut k = vec![0u8; 512];
        k[..5].copy_from_slice(b"a.txt");
        k[257..263].copy_from_slice(b"ustar\0");
        k
    }

    #[test]
    fn kennbytes() {
        assert_eq!(Format::erkennen(b"PK\x03\x04rest", "x.bin"), Format::Zip);
        assert_eq!(Format::erkennen(b"PK\x05\x06", "leer.zip"), Format::Zip);
        assert_eq!(
            Format::erkennen(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4], "a"),
            Format::SiebenZip
        );
        assert_eq!(
            Format::erkennen(b"Rar!\x1a\x07\x01\x00", "a.rar"),
            Format::Rar
        );
        assert_eq!(
            Format::erkennen(b"Rar!\x1a\x07\x00", "alt.rar"),
            Format::Rar
        );
        assert_eq!(Format::erkennen(&tar_kopf(), "ohne-endung"), Format::Tar);
        assert_eq!(Format::erkennen(b"\x1f\x8b\x08", "a.tar.gz"), Format::TarGz);
        assert_eq!(Format::erkennen(b"\x1f\x8b\x08", "a.TGZ"), Format::TarGz);
        assert_eq!(
            Format::erkennen(b"\x1f\x8b\x08", "notiz.txt.gz"),
            Format::Gz
        );
        assert_eq!(
            Format::erkennen(b"\xfd7zXZ\x00\x00", "a.tar.xz"),
            Format::TarXz
        );
        assert_eq!(Format::erkennen(b"\xfd7zXZ\x00\x00", "a.xz"), Format::Xz);
        assert_eq!(Format::erkennen(b"BZh91AY", "a.tar.bz2"), Format::TarBz2);
        assert_eq!(Format::erkennen(b"BZh91AY", "a.bz2"), Format::Bz2);
        assert_eq!(
            Format::erkennen(&[0x28, 0xB5, 0x2F, 0xFD, 0], "a.tar.zst"),
            Format::TarZst
        );
        assert_eq!(
            Format::erkennen(&[0x28, 0xB5, 0x2F, 0xFD], "a.zst"),
            Format::Zst
        );
        // Inhalt vor Name: eine zip-Datei namens .tar.gz bleibt zip
        assert_eq!(
            Format::erkennen(b"PK\x03\x04", "falsch.tar.gz"),
            Format::Zip
        );
        assert_eq!(Format::erkennen(b"", "alt.tar"), Format::Tar);
        assert_eq!(Format::erkennen(b"\x00\x01", "bild.iso"), Format::Sonstiges);
    }

    #[test]
    fn nach_dem_lesen() {
        // gz ohne passenden Namen, aber mit tar darin
        assert_eq!(Format::Gz.nach_lesen(false), Format::TarGz);
        assert_eq!(Format::TarGz.nach_lesen(true), Format::Gz);
        assert_eq!(Format::Zip.nach_lesen(false), Format::Zip);
    }

    #[test]
    fn schreibbare_formate() {
        for f in ERSTELLBAR {
            assert!(f.erstellbar() && f.aenderbar(), "{f:?}");
        }
        assert!(Format::TarBz2.aenderbar() && !Format::TarBz2.erstellbar());
        for f in [
            Format::SiebenZip,
            Format::Rar,
            Format::Gz,
            Format::Sonstiges,
        ] {
            assert!(!f.aenderbar() && !f.erstellbar(), "{f:?}");
        }
        assert_eq!(Format::aus_namen("x.TAR.GZ"), Some(Format::TarGz));
        assert_eq!(Format::aus_namen("x.zip"), Some(Format::Zip));
        assert_eq!(Format::aus_namen("x.txz"), Some(Format::TarXz));
        assert_eq!(Format::aus_namen("x.txt"), None);
    }

    #[test]
    fn namen_ohne_endung() {
        assert_eq!(Format::ohne_endung("bericht.tar.gz"), "bericht");
        assert_eq!(Format::ohne_endung("Fotos 2026.ZIP"), "Fotos 2026");
        assert_eq!(Format::ohne_endung("notiz.txt.gz"), "notiz.txt");
        assert_eq!(Format::ohne_endung(".zip"), ".zip");
        assert_eq!(Format::ohne_endung("ohne"), "ohne");
    }
}
