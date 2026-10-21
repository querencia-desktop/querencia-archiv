//! Kommandozeile für Files, Desktop und Skripte (ohne D-Bus):
//!
//! ```text
//! querencia-archiv [DATEI…]                      Archive in Fenstern öffnen
//! querencia-archiv --entpacken-hier DATEI…       jedes Archiv neben sich entpacken
//! querencia-archiv --entpacken-nach ORDNER DATEI…
//! querencia-archiv --erstellen ARCHIV DATEI…     neues zip/tar.gz/tar.xz
//! ```
//!
//! Mit Bildschirm zeigen die Aufträge Fortschritt, Konflikt-, Kennwort- und
//! Fehlerfenster (eigener Prozess, `NON_UNIQUE`); ohne Bildschirm laufen sie
//! still: vorhandene Dateien werden übersprungen, nie ersetzt, verschlüsselte
//! Archive scheitern, Meldungen gehen nach stderr. Exitcode 0 = alles
//! erledigt, 1 = ein Auftrag scheiterte oder wurde abgebrochen, 2 = Aufruf
//! falsch. Die Hilfe ist deutsch und unübersetzt (Log-Text, wie die
//! Optionsnamen).

use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use archiv::APP_ID;
use archiv::entpacken::{self, Auftrag as EntpackAuftrag, Auswahl, Konflikt, Rueckmeldung};
use archiv::format::Format;
use archiv::i18n::{gettext, gettext_f, ngettext_f};
use archiv::lesen::{self, Fehler as LeseFehler, Stand};
use archiv::pfade::RelPfad;
use archiv::schreiben::{self, Sammlung};

use crate::aufgabe::{self, Anzeige};
use crate::dialoge::Dialoge;

pub const HILFE: &str = "Aufruf:
  querencia-archiv [DATEI…]                       Archive öffnen
  querencia-archiv --entpacken-hier DATEI…        neben dem Archiv entpacken
  querencia-archiv --entpacken-nach ORDNER DATEI… in ORDNER entpacken
  querencia-archiv --erstellen ARCHIV DATEI…      neues Archiv (.zip, .tar.gz, .tar.xz)
  querencia-archiv --hilfe | --version

„Hier entpacken“ legt einen Ordner mit dem Namen des Archivs an, außer alles
liegt schon in einem einzigen Ordner. Einträge mit „..“ oder absolutem Pfad
werden nie entpackt (Auftrag abgelehnt). Exitcode 0 = erledigt, 1 = Fehler
oder abgebrochen, 2 = Aufruf falsch.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auftrag {
    EntpackenHier(Vec<PathBuf>),
    EntpackenNach(PathBuf, Vec<PathBuf>),
    Erstellen(PathBuf, Vec<PathBuf>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modus {
    Fenster(Vec<OsString>),
    Auftrag(Auftrag),
    Hilfe,
    Version,
}

pub fn auswerten(args: &[OsString]) -> Result<Modus, String> {
    let Some(erstes) = args.first() else {
        return Ok(Modus::Fenster(Vec::new()));
    };
    let pfade = |a: &[OsString]| -> Vec<PathBuf> { a.iter().map(PathBuf::from).collect() };
    match erstes.to_str() {
        Some("--hilfe" | "--help" | "-h") => Ok(Modus::Hilfe),
        Some("--version") => Ok(Modus::Version),
        Some("--entpacken-hier") => match &args[1..] {
            [] => Err("--entpacken-hier: keine Datei angegeben".into()),
            r => Ok(Modus::Auftrag(Auftrag::EntpackenHier(pfade(r)))),
        },
        Some("--entpacken-nach") => match &args[1..] {
            [] | [_] => Err("--entpacken-nach: ORDNER und mindestens eine Datei angeben".into()),
            [ordner, r @ ..] => Ok(Modus::Auftrag(Auftrag::EntpackenNach(
                ordner.into(),
                pfade(r),
            ))),
        },
        Some("--erstellen") => match &args[1..] {
            [] | [_] => Err("--erstellen: ARCHIV und mindestens eine Datei angeben".into()),
            [archiv, r @ ..] => Ok(Modus::Auftrag(Auftrag::Erstellen(archiv.into(), pfade(r)))),
        },
        Some("--") => Ok(Modus::Fenster(args[1..].to_vec())),
        Some(s) if s.starts_with('-') => Err(format!("unbekannte Option: {s}")),
        _ => {
            if let Some(o) = args
                .iter()
                .find(|a| a.to_str().is_some_and(|s| s.starts_with("--")))
            {
                return Err(format!("Optionen nur am Anfang: {}", o.to_string_lossy()));
            }
            Ok(Modus::Fenster(args.to_vec()))
        }
    }
}

/// Fortschritt und Rückfragen für beide Arten (mit und ohne Bildschirm).
trait Melder: Rueckmeldung + schreiben::Fortschritt {}
impl<T: Rueckmeldung + schreiben::Fortschritt> Melder for T {}

/// Oberfläche eines Auftrags.
trait Oberflaeche {
    async fn arbeit<T: Send + 'static>(
        &self,
        anzeige: Anzeige<'_>,
        stand: &Stand,
        f: impl FnOnce(&mut dyn Melder) -> T + Send + 'static,
    ) -> Option<T>;
    async fn passwort(&self, name: &str, falsch: bool) -> Option<String>;
    async fn fehler(&self, primaer: &str, sekundaer: &str);
}

struct MitFenstern(Dialoge);

impl Oberflaeche for MitFenstern {
    async fn arbeit<T: Send + 'static>(
        &self,
        anzeige: Anzeige<'_>,
        stand: &Stand,
        f: impl FnOnce(&mut dyn Melder) -> T + Send + 'static,
    ) -> Option<T> {
        aufgabe::laufen(&self.0, anzeige, stand, move |b| f(b)).await
    }
    async fn passwort(&self, name: &str, falsch: bool) -> Option<String> {
        self.0.passwort(name, falsch).await
    }
    async fn fehler(&self, primaer: &str, sekundaer: &str) {
        self.0.fehler(primaer, sekundaer).await
    }
}

/// Ohne Bildschirm: nichts ersetzen, kein Kennwort, Meldungen nach stderr.
struct Still;

struct StillMelder;

impl Rueckmeldung for StillMelder {
    fn fortschritt(&mut self, _: u64, _: u64, _: &str) {}
    fn konflikt(&mut self, ziel: &Path, _: &RelPfad) -> Konflikt {
        eprintln!(
            "querencia-archiv: übersprungen (existiert): {}",
            ziel.display()
        );
        Konflikt::Ueberspringen
    }
}

impl schreiben::Fortschritt for StillMelder {
    fn fortschritt(&mut self, _: u64, _: u64, _: &str) {}
}

impl Oberflaeche for Still {
    async fn arbeit<T: Send + 'static>(
        &self,
        _: Anzeige<'_>,
        _: &Stand,
        f: impl FnOnce(&mut dyn Melder) -> T + Send + 'static,
    ) -> Option<T> {
        Some(f(&mut StillMelder))
    }
    async fn passwort(&self, name: &str, _: bool) -> Option<String> {
        eprintln!("querencia-archiv: {name}: verschlüsselt, ein Kennwort geht nur mit Bildschirm");
        None
    }
    async fn fehler(&self, primaer: &str, sekundaer: &str) {
        eprintln!(
            "querencia-archiv: {primaer} {}",
            sekundaer.replace('\n', " ")
        );
    }
}

fn dateiname(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Listet `archiv` mit Kennwortabfrage.
async fn auflisten(
    ui: &impl Oberflaeche,
    archiv: &Path,
) -> Option<(lesen::Archiv, Option<String>)> {
    let name = dateiname(archiv);
    let mut pw: Option<String> = None;
    let mut falsch = false;
    loop {
        let stand = Stand::default();
        let (p, s, k) = (archiv.to_path_buf(), stand.clone(), pw.clone());
        let r = ui
            .arbeit(
                Anzeige {
                    fenstertitel: &gettext("Opening Archive"),
                    titel: &gettext_f("Reading “{name}”", &[("name", &name)]),
                    lesestand_von: Some(archiv),
                },
                &stand,
                move |_| lesen::auflisten(&p, k.as_deref(), &s),
            )
            .await?;
        match r {
            Ok(a) => return Some((a, pw)),
            Err(LeseFehler::PasswortNoetig | LeseFehler::PasswortFalsch) => {
                pw = Some(ui.passwort(&name, falsch).await?);
                falsch = true;
            }
            Err(LeseFehler::Abgebrochen) => return None,
            Err(e) => {
                let detail = match &e {
                    LeseFehler::Format => gettext("Archive type not supported."),
                    LeseFehler::Io(err) => gettext_f(
                        "The file could not be read.\n\nDetails: {detail}",
                        &[("detail", &err.to_string())],
                    ),
                    e => gettext_f(
                        "The archive is damaged or cannot be read.\n\nDetails: {detail}",
                        &[("detail", &e.to_string())],
                    ),
                };
                ui.fehler(
                    &gettext_f("Could not open “{name}”.", &[("name", &name)]),
                    &detail,
                )
                .await;
                return None;
            }
        }
    }
}

/// Entpackt ein Archiv; `ziel` None = „Hier entpacken“.
async fn entpacken_eins(ui: &impl Oberflaeche, archiv: &Path, ziel: Option<&Path>) -> bool {
    let name = dateiname(archiv);
    let Some((liste, mut pw)) = auflisten(ui, archiv).await else {
        return false;
    };
    let auswahl = match Auswahl::alles(&liste.inhalt) {
        Ok(a) => a,
        Err(abgelehnt) => {
            ui.fehler(
                &gettext("Extraction refused"),
                &gettext_f(
                    "The archive contains entries with unsafe paths (“..” or absolute). Nothing was extracted.\n\n{names}",
                    &[("names", &abgelehnt.0.iter().take(8).cloned().collect::<Vec<_>>().join("\n"))],
                ),
            )
            .await;
            return false;
        }
    };
    let ziel = match ziel {
        Some(z) => z.to_path_buf(),
        None => entpacken::hier_ziel(archiv, &liste.inhalt).0,
    };
    if let Err(e) = std::fs::create_dir_all(&ziel) {
        ui.fehler(
            &gettext("Extraction not performed"),
            &gettext_f(
                "The folder “{folder}” could not be created.\n\nDetails: {detail}",
                &[
                    ("folder", &ziel.display().to_string()),
                    ("detail", &e.to_string()),
                ],
            ),
        )
        .await;
        return false;
    }
    let mut erledigt = HashSet::new();
    let mut falsch = false;
    loop {
        let stand = Stand::default();
        let (s, a, z, k, ap, roh) = (
            stand.clone(),
            auswahl.clone(),
            ziel.clone(),
            pw.clone(),
            archiv.to_path_buf(),
            liste.roh,
        );
        let mut fertig = std::mem::take(&mut erledigt);
        let (r, fertig) = match ui
            .arbeit(
                Anzeige {
                    fenstertitel: &gettext("Extracting Files"),
                    titel: &gettext_f("Extracting files from “{name}”", &[("name", &name)]),
                    lesestand_von: None,
                },
                &stand,
                move |m| {
                    let r = entpacken::entpacken(
                        &EntpackAuftrag {
                            archiv: &ap,
                            roh,
                            ziel: &z,
                            auswahl: &a,
                            passwort: k.as_deref(),
                            stand: &s,
                        },
                        &mut fertig,
                        m,
                    );
                    (r, fertig)
                },
            )
            .await
        {
            Some(x) => x,
            None => return false,
        };
        erledigt = fertig;
        match r {
            Ok(e) => {
                if e.nicht_angelegt > 0 {
                    eprintln!(
                        "querencia-archiv: {name}: {}",
                        ngettext_f(
                            "{n} link or special file not created",
                            "{n} links or special files not created",
                            e.nicht_angelegt as u32,
                            &[],
                        )
                    );
                }
                return true;
            }
            Err(entpacken::Fehler::Lesen(
                LeseFehler::PasswortNoetig | LeseFehler::PasswortFalsch,
            )) => match ui.passwort(&name, falsch).await {
                Some(k) => {
                    pw = Some(k);
                    falsch = true;
                }
                None => return false,
            },
            Err(entpacken::Fehler::Abgebrochen) => return false,
            Err(e) => {
                let detail = match &e {
                    entpacken::Fehler::Ziel(err, p) => gettext_f(
                        "“{file}” could not be written.\n\nDetails: {detail}",
                        &[
                            ("file", &p.display().to_string()),
                            ("detail", &err.to_string()),
                        ],
                    ),
                    entpacken::Fehler::Lesen(l) => gettext_f(
                        "The archive is damaged or cannot be read.\n\nDetails: {detail}",
                        &[("detail", &l.to_string())],
                    ),
                    entpacken::Fehler::Abgebrochen => String::new(),
                };
                ui.fehler(
                    &gettext("An error occurred while extracting files."),
                    &detail,
                )
                .await;
                return false;
            }
        }
    }
}

async fn erstellen(ui: &impl Oberflaeche, archiv: &Path, dateien: &[PathBuf]) -> bool {
    let name = dateiname(archiv);
    let titel = gettext_f("Could not create “{name}”.", &[("name", &name)]);
    let Some(format) = Format::aus_namen(&name).filter(|f| f.erstellbar()) else {
        ui.fehler(
            &titel,
            &gettext("Supported archive types for new archives: .zip, .tar.gz, .tar.xz."),
        )
        .await;
        return false;
    };
    if archiv::pfade::vorhanden(archiv) != archiv::pfade::Vorhanden::Nichts {
        ui.fehler(
            &titel,
            &gettext("A file with this name already exists. Choose another name."),
        )
        .await;
        return false;
    }
    let ordner = archiv
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !ordner.is_dir() {
        ui.fehler(&titel, &gettext("The folder does not exist."))
            .await;
        return false;
    }
    let archiv = ordner.join(&name);
    let stand = Stand::default();
    let (s, ap, d) = (stand.clone(), archiv.clone(), dateien.to_vec());
    let r = ui
        .arbeit(
            Anzeige {
                fenstertitel: &gettext("Adding Files"),
                titel: &gettext_f("Adding files to “{name}”", &[("name", &name)]),
                lesestand_von: None,
            },
            &stand,
            move |m| {
                let sammlung = Sammlung::von(&d, "")?;
                schreiben::erstellen(&ap, format, &sammlung, None, &s, m)
            },
        )
        .await;
    match r {
        Some(Ok(_)) => true,
        Some(Err(schreiben::Fehler::Abgebrochen)) | None => false,
        Some(Err(e)) => {
            let detail = match &e {
                schreiben::Fehler::Quelle(err, p) => gettext_f(
                    "“{file}” could not be read.\n\nDetails: {detail}",
                    &[
                        ("file", &p.display().to_string()),
                        ("detail", &err.to_string()),
                    ],
                ),
                schreiben::Fehler::Ziel(err, p) => gettext_f(
                    "“{file}” could not be written.\n\nDetails: {detail}",
                    &[
                        ("file", &p.display().to_string()),
                        ("detail", &err.to_string()),
                    ],
                ),
                e => gettext_f("Details: {detail}", &[("detail", &format!("{e:?}"))]),
            };
            ui.fehler(&titel, &detail).await;
            false
        }
    }
}

async fn ausfuehren_mit(ui: &impl Oberflaeche, auftrag: &Auftrag) -> u8 {
    let mut ok = true;
    match auftrag {
        Auftrag::EntpackenHier(dateien) => {
            for d in dateien {
                ok &= entpacken_eins(ui, d, None).await;
            }
        }
        Auftrag::EntpackenNach(ordner, dateien) => {
            for d in dateien {
                ok &= entpacken_eins(ui, d, Some(ordner)).await;
            }
        }
        Auftrag::Erstellen(archiv, dateien) => ok = erstellen(ui, archiv, dateien).await,
    }
    if ok { 0 } else { 1 }
}

/// Führt einen Auftrag aus; mit Bildschirm in einer eigenen
/// GTK-Anwendung, sonst still.
pub fn ausfuehren(auftrag: Auftrag, argv0: OsString) -> glib::ExitCode {
    if gtk::init().is_err() {
        let code = glib::MainContext::default().block_on(ausfuehren_mit(&Still, &auftrag));
        return glib::ExitCode::from(code);
    }
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let code = Rc::new(Cell::new(1u8));
    let c = code.clone();
    app.connect_startup(|_| {
        gtk::Window::set_default_icon_name(APP_ID);
        crate::fenster::klassische_symbole();
        crate::resource::register();
        crate::fenster::css_laden();
        crate::fenster::farbschema_anstossen();
    });
    app.connect_activate(move |app| {
        let halt = app.hold();
        let (auftrag, c) = (auftrag.clone(), c.clone());
        glib::spawn_future_local(async move {
            let ui = MitFenstern(Dialoge::neu(None));
            c.set(ausfuehren_mit(&ui, &auftrag).await);
            drop(halt);
        });
    });
    app.run_with_args(&[argv0.to_string_lossy()]);
    glib::ExitCode::from(code.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn aufrufe() {
        assert_eq!(auswerten(&a(&[])), Ok(Modus::Fenster(vec![])));
        assert_eq!(
            auswerten(&a(&["x.zip", "y.7z"])),
            Ok(Modus::Fenster(a(&["x.zip", "y.7z"])))
        );
        assert_eq!(
            auswerten(&a(&["--", "--seltsam.zip"])),
            Ok(Modus::Fenster(a(&["--seltsam.zip"])))
        );
        assert_eq!(
            auswerten(&a(&["--entpacken-hier", "a.zip", "b.tar.gz"])),
            Ok(Modus::Auftrag(Auftrag::EntpackenHier(vec![
                "a.zip".into(),
                "b.tar.gz".into()
            ])))
        );
        assert_eq!(
            auswerten(&a(&["--entpacken-nach", "/tmp/z", "a.zip"])),
            Ok(Modus::Auftrag(Auftrag::EntpackenNach(
                "/tmp/z".into(),
                vec!["a.zip".into()]
            )))
        );
        assert_eq!(
            auswerten(&a(&["--erstellen", "n.tar.xz", "a", "b"])),
            Ok(Modus::Auftrag(Auftrag::Erstellen(
                "n.tar.xz".into(),
                vec!["a".into(), "b".into()]
            )))
        );
        assert_eq!(auswerten(&a(&["--hilfe"])), Ok(Modus::Hilfe));
        assert_eq!(auswerten(&a(&["--version"])), Ok(Modus::Version));
        assert!(auswerten(&a(&["--entpacken-hier"])).is_err());
        assert!(auswerten(&a(&["--entpacken-nach", "z"])).is_err());
        assert!(auswerten(&a(&["--erstellen", "x.zip"])).is_err());
        assert!(auswerten(&a(&["--unbekannt"])).is_err());
        assert!(auswerten(&a(&["x.zip", "--entpacken-hier"])).is_err());
    }
}
