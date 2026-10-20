//! Lesen über libarchive (Crate compress-tools): Inhalt auflisten und
//! Einträge in Archivreihenfolge durchlaufen.
//!
//! Beim Auflisten überspringt libarchive die Daten (Filter liefert
//! `false`); bei zip springt es über das zentrale Verzeichnis, bei
//! komprimierten tar-Archiven muss es den ganzen Strom entpacken. Eine
//! einzelne komprimierte Datei (`notiz.txt.gz`) erkennt libarchive nur im
//! „raw“-Modus; der zweite Versuch läuft deshalb nur, wenn der erste mit
//! „unbekanntes Format“ endet und die Kennbytes einen komprimierten Strom
//! zeigen. Das mtree-Format bleibt aus (libarchive hielte sonst manche
//! Textdatei für ein Archiv).

use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use compress_tools::{ArchiveContents, ArchiveIterator, ArchiveIteratorBuilder, ArchivePassword};

use crate::format::Format;
use crate::modell::{Art, Eintrag, Inhalt};
use crate::pfade::{Name, RelPfad};

/// Fehler beim Lesen.
#[derive(Debug)]
pub enum Fehler {
    /// Die Datei selbst ist nicht lesbar.
    Io(io::Error),
    /// Kein Archiv, das libarchive kennt.
    Format,
    /// Eintrag oder Archiv ist verschlüsselt, kein Kennwort angegeben.
    PasswortNoetig,
    /// Das angegebene Kennwort passt nicht.
    PasswortFalsch,
    /// Meldung von libarchive (beschädigt, nicht unterstützt …).
    Archiv(String),
    Abgebrochen,
}

impl std::fmt::Display for Fehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fehler::Io(e) => write!(f, "{e}"),
            Fehler::Format => f.write_str("Unrecognized archive format"),
            Fehler::PasswortNoetig => f.write_str("Passphrase required"),
            Fehler::PasswortFalsch => f.write_str("Incorrect passphrase"),
            Fehler::Archiv(m) => f.write_str(m),
            Fehler::Abgebrochen => f.write_str("Cancelled"),
        }
    }
}

impl From<compress_tools::Error> for Fehler {
    fn from(e: compress_tools::Error) -> Fehler {
        match e {
            compress_tools::Error::Io(e) => Fehler::Io(e),
            compress_tools::Error::Extraction { details, .. } => meldung(&details),
            other => Fehler::Archiv(other.to_string()),
        }
    }
}

/// Ordnet die Meldungen von libarchive 3.8 (zip, 7z, rar) zu.
fn meldung(m: &str) -> Fehler {
    let k = m.to_lowercase();
    if k.contains("incorrect passphrase") {
        Fehler::PasswortFalsch
    } else if k.contains("passphrase") || k.contains("encrypted") && k.contains("password") {
        Fehler::PasswortNoetig
    } else if k.contains("unrecognized archive format") {
        Fehler::Format
    } else {
        Fehler::Archiv(m.trim().to_string())
    }
}

/// Lesestand für Fortschrittsanzeigen: gelesene Bytes der Archivdatei.
#[derive(Clone, Default)]
pub struct Stand {
    pub gelesen: Arc<AtomicU64>,
    pub abbruch: Arc<AtomicBool>,
}

impl Stand {
    pub fn abgebrochen(&self) -> bool {
        self.abbruch.load(Ordering::Relaxed)
    }
}

/// Datei, die beim Lesen ihren Stand meldet.
struct Zaehler {
    datei: File,
    gelesen: Arc<AtomicU64>,
}

impl Read for Zaehler {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.datei.read(buf)?;
        if let Ok(p) = self.datei.stream_position() {
            self.gelesen.store(p, Ordering::Relaxed);
        }
        Ok(n)
    }
}

impl Seek for Zaehler {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let p = self.datei.seek(pos)?;
        self.gelesen.store(p, Ordering::Relaxed);
        Ok(p)
    }
}

/// Namen ohne gültiges UTF-8 (alte Archive) verlustbehaftet statt Abbruch.
fn namen_lesen(b: &[u8]) -> compress_tools::Result<String> {
    Ok(String::from_utf8_lossy(b).into_owned())
}

/// Ein geöffnetes Archiv: Format und Inhalt.
#[derive(Clone, Debug)]
pub struct Archiv {
    pub format: Format,
    /// Einzelne komprimierte Datei (libarchive „raw“).
    pub roh: bool,
    pub inhalt: Inhalt,
}

/// Kennbytes am Dateianfang.
pub fn kopf(pfad: &Path) -> io::Result<Vec<u8>> {
    let mut k = vec![0; 512];
    let mut f = File::open(pfad)?;
    let mut n = 0;
    while n < k.len() {
        match f.read(&mut k[n..])? {
            0 => break,
            m => n += m,
        }
    }
    k.truncate(n);
    Ok(k)
}

/// Iterator über `pfad`; `filter` entscheidet je Eintrag, ob seine Daten
/// gelesen werden (Index in Archivreihenfolge, Name, `st_mode`).
pub fn durchlaufen(
    pfad: &Path,
    roh: bool,
    passwort: Option<&str>,
    stand: &Stand,
    filter: impl Fn(usize, &str, &compress_tools::stat) -> bool + 'static,
) -> Result<ArchiveIterator<impl Read + Seek>, Fehler> {
    let datei = File::open(pfad).map_err(Fehler::Io)?;
    let quelle = Zaehler {
        datei,
        gelesen: stand.gelesen.clone(),
    };
    let zaehler = Rc::new(Cell::new(0usize));
    let z = zaehler.clone();
    let mut b = ArchiveIteratorBuilder::new(quelle)
        .decoder(namen_lesen)
        .mtree_format(false)
        .raw_format(roh)
        .filter(move |name, st| {
            let i = z.get();
            z.set(i + 1);
            filter(i, name, st)
        });
    if let Some(pw) = passwort {
        b = b.with_password(ArchivePassword::new(pw)?);
    }
    Ok(b.build()?)
}

/// Listet den Inhalt (ohne Daten zu entpacken).
pub fn auflisten(pfad: &Path, passwort: Option<&str>, stand: &Stand) -> Result<Archiv, Fehler> {
    let k = kopf(pfad).map_err(Fehler::Io)?;
    let name = pfad
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = Format::erkennen(&k, &name);
    match liste(pfad, false, passwort, stand) {
        Ok(e) => Ok(Archiv {
            format: format.nach_lesen(false),
            roh: false,
            inhalt: Inhalt::aus(e),
        }),
        Err(Fehler::Format) if format.nach_lesen(true) != format.nach_lesen(false) => {
            let mut e = liste(pfad, true, passwort, stand)?;
            // libarchive nennt den einzigen Eintrag „data“; angezeigt und
            // entpackt wird er unter dem Archivnamen ohne Endung
            let geaendert = std::fs::metadata(pfad)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            for x in &mut e {
                if let Some(p) = RelPfad::einzeln(&Format::ohne_endung(&name)) {
                    x.name = Name::Sicher(p);
                }
                x.art = Art::Datei;
                x.rechte = 0o644;
                if x.geaendert == 0 {
                    x.geaendert = geaendert;
                }
            }
            Ok(Archiv {
                format: format.nach_lesen(true),
                roh: true,
                inhalt: Inhalt::aus(e),
            })
        }
        Err(e) => Err(e),
    }
}

fn liste(
    pfad: &Path,
    roh: bool,
    passwort: Option<&str>,
    stand: &Stand,
) -> Result<Vec<Eintrag>, Fehler> {
    let gesammelt = Rc::new(std::cell::RefCell::new(Vec::new()));
    let g = gesammelt.clone();
    let abbruch = stand.abbruch.clone();
    // Der Filter sammelt nur die Köpfe; `true` (Daten lesen) nur, um bei
    // einem Abbruch aus der Schleife zu kommen.
    let mut it = durchlaufen(pfad, roh, passwort, stand, move |_, name, st| {
        g.borrow_mut().push(Eintrag::neu(
            name,
            st.st_mode,
            st.st_size.max(0) as u64,
            st.st_mtime,
        ));
        abbruch.load(Ordering::Relaxed)
    })?;
    if let Some(ArchiveContents::Err(e)) = it.next() {
        return Err(e.into());
    }
    if stand.abgebrochen() {
        return Err(Fehler::Abgebrochen);
    }
    it.close()?;
    Ok(gesammelt.take())
}
