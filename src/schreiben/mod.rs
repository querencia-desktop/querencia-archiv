//! Archive schreiben: neu erstellen und vorhandene neu schreiben
//! (Hinzufügen, Entfernen).
//!
//! libarchive kann an ein Archiv nichts anhängen; Hinzufügen und Entfernen
//! lesen deshalb das alte Archiv Eintrag für Eintrag und schreiben ein
//! neues. Geschrieben wird immer unter einem Hilfsnamen im Ordner des
//! Ziels; erst das vollständige, geschlossene Archiv ersetzt das alte in
//! einem Schritt (`rename`). Ein Abbruch oder Fehler lässt das alte Archiv
//! unverändert.
//!
//! Nicht neu geschrieben werden Archive mit Verknüpfungen, Gerätedateien
//! oder unsicheren Namen (sie gingen verloren bzw. würden weitergereicht)
//! und verschlüsselte Archive (das neue wäre unverschlüsselt). Beim
//! Hinzufügen von der Platte werden Verknüpfungen übersprungen und gezählt.

pub mod ffi;

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use compress_tools::ArchiveContents;

pub use ffi::{AE_IFDIR, AE_IFLNK, AE_IFREG};

use crate::format::{Container, Filter, Format};
use crate::lesen::{self, Archiv, Fehler as LeseFehler, Stand};
use crate::modell::Art;
use crate::pfade::RelPfad;

/// Fehler beim Schreiben.
#[derive(Debug)]
pub enum Fehler {
    Lesen(LeseFehler),
    /// Meldung von libarchive beim Schreiben.
    Schreiben(String),
    /// Eine hinzuzufügende Datei ist nicht lesbar.
    Quelle(io::Error, PathBuf),
    /// Hilfsdatei oder Umbenennen im Zielordner.
    Ziel(io::Error, PathBuf),
    /// Format oder Inhalt erlauben kein Neuschreiben.
    NichtAenderbar,
    /// Verschlüsselt: Neuschreiben würde die Verschlüsselung entfernen.
    Verschluesselt,
    Abgebrochen,
}

impl From<LeseFehler> for Fehler {
    fn from(e: LeseFehler) -> Fehler {
        match e {
            LeseFehler::Abgebrochen => Fehler::Abgebrochen,
            LeseFehler::PasswortNoetig | LeseFehler::PasswortFalsch => Fehler::Verschluesselt,
            e => Fehler::Lesen(e),
        }
    }
}

/// Fortschritt beim Schreiben.
pub trait Fortschritt {
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, name: &str);
}

impl<F: FnMut(u64, u64, &str)> Fortschritt for F {
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, name: &str) {
        self(fertig, gesamt, name)
    }
}

/// Ein Eintrag von der Platte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Posten {
    Datei {
        quelle: PathBuf,
        ziel: RelPfad,
        groesse: u64,
    },
    Ordner {
        quelle: PathBuf,
        ziel: RelPfad,
    },
}

impl Posten {
    pub fn ziel(&self) -> &RelPfad {
        match self {
            Posten::Datei { ziel, .. } | Posten::Ordner { ziel, .. } => ziel,
        }
    }
}

/// Was von der Platte ins Archiv kommt.
#[derive(Clone, Debug, Default)]
pub struct Sammlung {
    pub posten: Vec<Posten>,
    /// übersprungene Verknüpfungen und Sonderdateien
    pub uebersprungen: usize,
    pub groesse: u64,
}

impl Sammlung {
    /// Sammelt `pfade` (Dateien oder Ordner, rekursiv, ohne Verknüpfungen
    /// zu folgen) unter dem Archivordner `ordner` („“ = Wurzel).
    pub fn von(pfade: &[PathBuf], ordner: &str) -> Result<Sammlung, Fehler> {
        let mut s = Sammlung::default();
        for p in pfade {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Some(ziel) = RelPfad::in_ordner(ordner, &name) else {
                s.uebersprungen += 1;
                continue;
            };
            s.sammeln(p, ziel)?;
        }
        Ok(s)
    }

    fn sammeln(&mut self, quelle: &Path, ziel: RelPfad) -> Result<(), Fehler> {
        let m = fs::symlink_metadata(quelle).map_err(|e| Fehler::Quelle(e, quelle.into()))?;
        if m.is_file() {
            self.groesse += m.len();
            self.posten.push(Posten::Datei {
                quelle: quelle.into(),
                ziel,
                groesse: m.len(),
            });
        } else if m.is_dir() {
            self.posten.push(Posten::Ordner {
                quelle: quelle.into(),
                ziel: ziel.clone(),
            });
            let mut kinder: Vec<PathBuf> = fs::read_dir(quelle)
                .map_err(|e| Fehler::Quelle(e, quelle.into()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            kinder.sort();
            for k in kinder {
                let name = k
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match ziel.mit(&name) {
                    Some(z) => self.sammeln(&k, z)?,
                    None => self.uebersprungen += 1,
                }
            }
        } else {
            self.uebersprungen += 1;
        }
        Ok(())
    }
}

/// Ergebnis eines Schreiblaufs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ergebnis {
    pub eintraege: usize,
    pub entfernt: usize,
    pub hinzugefuegt: usize,
    pub ersetzt: usize,
    pub uebersprungen: usize,
}

fn hilfsname(ziel: &Path) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = ziel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n = N.fetch_add(1, Ordering::Relaxed);
    ziel.with_file_name(format!(".{name}.qa-neu-{}-{n}", std::process::id()))
}

pub fn c_art(
    container: Container,
    filter: Filter,
) -> (ffi::Container, ffi::Filter, &'static [&'static str]) {
    let c = match container {
        Container::Zip => ffi::Container::Zip,
        Container::Tar => ffi::Container::Pax,
        Container::SiebenZip => ffi::Container::SiebenZip,
    };
    let f = match filter {
        Filter::Ohne => ffi::Filter::Ohne,
        Filter::Gzip => ffi::Filter::Gzip,
        Filter::Xz => ffi::Filter::Xz,
        Filter::Bzip2 => ffi::Filter::Bzip2,
        Filter::Zstd => ffi::Filter::Zstd,
    };
    // zip: Namen als UTF-8 mit gesetztem Kennbit (Bit 11), wie Info-ZIP
    let o: &[&str] = match container {
        Container::Zip => &["zip:hdrcharset=UTF-8"],
        _ => &[],
    };
    (c, f, o)
}

/// Offener Schreibvorgang unter einem Hilfsnamen.
struct Ziel {
    s: Option<ffi::Schreiber>,
    hilf: PathBuf,
    ziel: PathBuf,
}

impl Ziel {
    fn neu(
        ziel: &Path,
        container: Container,
        filter: Filter,
        extra: &[&str],
        passwort: Option<&str>,
    ) -> Result<Ziel, Fehler> {
        let hilf = hilfsname(ziel);
        // Hilfsname exklusiv anlegen (libarchive öffnet dann mit O_TRUNC)
        File::options()
            .write(true)
            .create_new(true)
            .open(&hilf)
            .map_err(|e| Fehler::Ziel(e, hilf.clone()))?;
        let (c, f, o) = c_art(container, filter);
        let mut opt: Vec<&str> = o.to_vec();
        opt.extend_from_slice(extra);
        match ffi::Schreiber::neu(&hilf, c, f, &opt, passwort) {
            Ok(s) => Ok(Ziel {
                s: Some(s),
                hilf,
                ziel: ziel.into(),
            }),
            Err(m) => {
                let _ = fs::remove_file(&hilf);
                Err(Fehler::Schreiben(m))
            }
        }
    }

    fn s(&mut self) -> &mut ffi::Schreiber {
        self.s.as_mut().expect("Schreiber offen")
    }

    fn posten(
        &mut self,
        p: &Posten,
        fertig: &mut u64,
        gesamt: u64,
        stand: &Stand,
        rueck: &mut dyn Fortschritt,
    ) -> Result<(), Fehler> {
        match p {
            Posten::Ordner { quelle, ziel } => {
                let m = fs::metadata(quelle).map_err(|e| Fehler::Quelle(e, quelle.clone()))?;
                self.s()
                    .kopf(
                        &format!("{ziel}/"),
                        AE_IFDIR,
                        m.permissions().mode() & 0o777,
                        0,
                        m.mtime(),
                    )
                    .and_then(|_| self.s().ende())
                    .map_err(Fehler::Schreiben)
            }
            Posten::Datei { quelle, ziel, .. } => {
                let mut f = File::open(quelle).map_err(|e| Fehler::Quelle(e, quelle.clone()))?;
                let m = f
                    .metadata()
                    .map_err(|e| Fehler::Quelle(e, quelle.clone()))?;
                // Größe zum Zeitpunkt des Kopfs; wächst die Datei inzwischen,
                // wird nur so viel gelesen
                let laenge = m.len();
                rueck.fortschritt(*fertig, gesamt, ziel.as_str());
                self.s()
                    .kopf(
                        ziel.as_str(),
                        AE_IFREG,
                        m.permissions().mode() & 0o777,
                        laenge,
                        m.mtime(),
                    )
                    .map_err(Fehler::Schreiben)?;
                let mut rest = laenge;
                let mut buf = vec![0u8; 256 * 1024];
                while rest > 0 {
                    if stand.abgebrochen() {
                        return Err(Fehler::Abgebrochen);
                    }
                    let stueck = rest.min(buf.len() as u64) as usize;
                    let n = f
                        .read(&mut buf[..stueck])
                        .map_err(|e| Fehler::Quelle(e, quelle.clone()))?;
                    if n == 0 {
                        // kürzer geworden: mit Nullen auffüllen wäre falsch
                        return Err(Fehler::Quelle(
                            io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "Datei wurde während des Lesens kürzer",
                            ),
                            quelle.clone(),
                        ));
                    }
                    self.s().daten(&buf[..n]).map_err(Fehler::Schreiben)?;
                    rest -= n as u64;
                    *fertig += n as u64;
                    rueck.fortschritt(*fertig, gesamt, "");
                }
                self.s().ende().map_err(Fehler::Schreiben)
            }
        }
    }

    /// Schließt das Archiv und ersetzt das Ziel.
    fn fertig(mut self, rechte_von: Option<&Path>) -> Result<(), Fehler> {
        let s = self.s.take().expect("Schreiber offen");
        s.schliessen().map_err(Fehler::Schreiben)?;
        let rechte = rechte_von
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o644);
        fs::set_permissions(&self.hilf, fs::Permissions::from_mode(rechte))
            .map_err(|e| Fehler::Ziel(e, self.hilf.clone()))?;
        File::open(&self.hilf)
            .and_then(|f| f.sync_all())
            .map_err(|e| Fehler::Ziel(e, self.hilf.clone()))?;
        fs::rename(&self.hilf, &self.ziel).map_err(|e| Fehler::Ziel(e, self.ziel.clone()))
    }
}

impl Drop for Ziel {
    fn drop(&mut self) {
        if self.s.take().is_some() {
            let _ = fs::remove_file(&self.hilf);
        }
    }
}

/// Erstellt `ziel` neu im Format `format` aus `sammlung`. `extra` sind
/// zusätzliche libarchive-Optionen (nur das Testwerkzeug: Verschlüsselung).
pub fn erstellen(
    ziel: &Path,
    format: Format,
    sammlung: &Sammlung,
    passwort: Option<(&str, &[&str])>,
    stand: &Stand,
    rueck: &mut dyn Fortschritt,
) -> Result<Ergebnis, Fehler> {
    let (c, f) = match format {
        Format::SiebenZip => (Container::SiebenZip, Filter::Ohne),
        f => f.schreibweise().ok_or(Fehler::NichtAenderbar)?,
    };
    let (pw, extra) = match passwort {
        Some((p, o)) => (Some(p), o),
        None => (None, &[][..]),
    };
    let mut z = Ziel::neu(ziel, c, f, extra, pw)?;
    let mut fertig = 0;
    for p in &sammlung.posten {
        z.posten(p, &mut fertig, sammlung.groesse, stand, rueck)?;
    }
    z.fertig(None)?;
    Ok(Ergebnis {
        eintraege: sammlung.posten.len(),
        hinzugefuegt: sammlung.posten.len(),
        uebersprungen: sammlung.uebersprungen,
        ..Ergebnis::default()
    })
}

/// Schreibt `archiv` neu: ohne die Einträge `entfernen` (Indizes), mit
/// `sammlung` dazu (gleichnamige Dateien im Archiv werden ersetzt).
pub fn neu_schreiben(
    pfad: &Path,
    archiv: &Archiv,
    entfernen: &HashSet<usize>,
    sammlung: &Sammlung,
    stand: &Stand,
    rueck: &mut dyn Fortschritt,
) -> Result<Ergebnis, Fehler> {
    let (c, f) = archiv.format.schreibweise().ok_or(Fehler::NichtAenderbar)?;
    if archiv.roh || archiv.inhalt.nicht_neu_schreibbar() {
        return Err(Fehler::NichtAenderbar);
    }
    let neu: HashSet<&str> = sammlung.posten.iter().map(|p| p.ziel().as_str()).collect();
    // Index → behalten? (entfernt oder durch Neues ersetzt fällt weg)
    let mut behalten = HashSet::new();
    let mut erg = Ergebnis::default();
    let mut gesamt = sammlung.groesse;
    for (i, e) in archiv.inhalt.eintraege.iter().enumerate() {
        let ersetzt = e.pfad().is_some_and(|p| neu.contains(p.as_str()));
        if entfernen.contains(&i) {
            erg.entfernt += 1;
        } else if ersetzt {
            if e.art == Art::Datei {
                erg.ersetzt += 1;
            }
        } else {
            behalten.insert(i);
            if e.art == Art::Datei {
                gesamt += e.groesse;
            }
        }
    }
    let mut z = Ziel::neu(pfad, c, f, &[], None)?;
    let mut it = lesen::durchlaufen(pfad, false, None, stand, move |i, _, _| {
        behalten.contains(&i)
    })?;
    let mut fertig = 0u64;
    for teil in &mut it {
        if stand.abgebrochen() {
            return Err(Fehler::Abgebrochen);
        }
        match teil {
            ArchiveContents::StartOfEntry(name, st) => {
                let typ = if Art::aus_modus(st.st_mode) == Art::Ordner || name.ends_with('/') {
                    AE_IFDIR
                } else {
                    AE_IFREG
                };
                rueck.fortschritt(fertig, gesamt, &name);
                z.s()
                    .kopf(
                        &name,
                        typ,
                        st.st_mode & 0o7777,
                        st.st_size.max(0) as u64,
                        st.st_mtime,
                    )
                    .map_err(Fehler::Schreiben)?;
                erg.eintraege += 1;
            }
            ArchiveContents::DataChunk(d) => {
                z.s().daten(&d).map_err(Fehler::Schreiben)?;
                fertig += d.len() as u64;
                rueck.fortschritt(fertig, gesamt, "");
            }
            ArchiveContents::EndOfEntry => z.s().ende().map_err(Fehler::Schreiben)?,
            ArchiveContents::Err(e) => return Err(LeseFehler::from(e).into()),
        }
    }
    it.close().map_err(|e| Fehler::from(LeseFehler::from(e)))?;
    for p in &sammlung.posten {
        z.posten(p, &mut fertig, gesamt, stand, rueck)?;
        erg.eintraege += 1;
        erg.hinzugefuegt += 1;
    }
    erg.uebersprungen = sammlung.uebersprungen;
    z.fertig(Some(pfad))?;
    Ok(erg)
}
