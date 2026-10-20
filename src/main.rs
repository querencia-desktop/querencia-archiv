//! Querencia Archiv – klassische Archivverwaltung für Querencia Desktop.
//!
//! Vorbild ist Engrampa 1.28 bzw. File Roller 2.30 (GNOME 2): Menüleiste,
//! Werkzeugleiste, Ortsleiste, Ordnerbaum neben der Dateiliste,
//! Statusleiste. GTK 4 ohne libadwaita und ohne eigene Titelleiste. Lesen
//! über libarchive (alle Formate, RAR nur lesen), Schreiben von zip, tar.gz
//! und tar.xz; der Kern liegt in der Bibliothek (`src/lib.rs`).
//! Oberflächentexte englisch über gettext, Deutsch aus `po/de.po`.

mod aufgabe;
mod dialoge;
mod fenster;
mod kommandozeile;
mod resource;
mod vorschau;
mod zeile;

use std::ffi::OsString;

use gtk::prelude::*;
use gtk::{gio, glib};

use archiv::{APP_ID, APP_NAME, i18n};

use kommandozeile::Modus;

/// Inhaltstypen, die Querencia Archiv öffnet (Starter, Öffnen-Dialog,
/// Standardprogramm). Die tar-Untertypen nennt shared-mime-info eigens
/// (`.tar.gz` ist application/x-compressed-tar, nicht application/gzip).
pub const MIME_TYPEN: [&str; 14] = [
    "application/zip",
    "application/x-tar",
    "application/gzip",
    "application/x-xz",
    "application/x-bzip2",
    "application/zstd",
    "application/x-7z-compressed",
    "application/vnd.rar",
    "application/x-compressed-tar",
    "application/x-xz-compressed-tar",
    "application/x-bzip2-compressed-tar",
    "application/x-bzip-compressed-tar",
    "application/x-zstd-compressed-tar",
    "application/x-zip-compressed",
];

fn main() -> glib::ExitCode {
    i18n::init();
    glib::set_application_name(APP_NAME);
    let mut args: Vec<OsString> = std::env::args_os().collect();
    let argv0 = if args.is_empty() {
        OsString::from("querencia-archiv")
    } else {
        args.remove(0)
    };
    match kommandozeile::auswerten(&args) {
        Ok(Modus::Hilfe) => {
            println!("{}", kommandozeile::HILFE);
            glib::ExitCode::SUCCESS
        }
        Ok(Modus::Version) => {
            println!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
            glib::ExitCode::SUCCESS
        }
        Ok(Modus::Auftrag(a)) => kommandozeile::ausfuehren(a, argv0),
        Ok(Modus::Fenster(dateien)) => fenster_starten(argv0, dateien),
        Err(m) => {
            eprintln!("querencia-archiv: {m}\n\n{}", kommandozeile::HILFE);
            glib::ExitCode::from(2)
        }
    }
}

fn fenster_starten(argv0: OsString, dateien: Vec<OsString>) -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|app| {
        gtk::Window::set_default_icon_name(APP_ID);
        fenster::klassische_symbole();
        resource::register();
        fenster::css_laden();
        fenster::kuerzel(app);
        app.set_menubar(Some(&fenster::menueleiste()));
        vorschau::alte_aufraeumen();
    });
    app.connect_shutdown(|_| vorschau::aufraeumen());
    app.connect_activate(|app| fenster::Fenster::irgendeins(app).present());
    app.connect_open(|app, dateien, _| {
        for d in dateien {
            let Some(p) = d.path() else { continue };
            let f = fenster::Fenster::leer_oder_neu(app);
            f.present();
            f.oeffnen(p);
        }
    });
    // Dateien als file://-URIs: prozentkodiert, also auch Namen ohne
    // gültiges UTF-8; GApplication reicht sie an ein laufendes Archiv weiter.
    let mut argv = vec![argv0.to_string_lossy().into_owned()];
    argv.extend(
        dateien
            .iter()
            .map(|d| gio::File::for_commandline_arg(d).uri().to_string()),
    );
    app.run_with_args(&argv)
}
