//! Nebenfenster als `async`-Funktionen: Meldung, Frage, Kennwort,
//! Konflikt, Fortschritt, Entpacken, Neues Archiv, Info.
//!
//! Jedes Fenster wird je Hauptfenster (bzw. je Kommandozeilenlauf) einmal
//! angelegt und beim Schließen nur verborgen (wie in Bilder und Dokumente):
//! GTK 4.22.4 stürzt beim Zerstören eines Fensters ab, das aus einem
//! Tastenkürzel heraus geöffnet wurde (QI-01). Escape schließt jedes
//! Fenster und gilt als „Abbrechen“. Zugriffstasten sind je Fenster und
//! Sprache eindeutig.

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures_channel::oneshot;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use archiv::entpacken::Konflikt;
use archiv::format::{ERSTELLBAR, Format};
use archiv::i18n::{gettext, gettext_f};
use archiv::lesen::Stand;
use archiv::{APP_NAME, anzeige};

use crate::resource;

type Antwort<T> = Rc<RefCell<Option<oneshot::Sender<T>>>>;

/// Website von Querencia Desktop (Info-Dialog).
const WEBSITE: &str = "https://querencialinux.org";
/// Neue Fehlermeldung im gemeinsamen Repo (Hilfe → Fehler melden).
pub const FEHLER_MELDEN: &str = "https://github.com/querencia-desktop/querencia/issues/new/choose";

/// Adresse der Online-Hilfe in der Sprache der Oberfläche: Die Adresse steht
/// selbst im Katalog, gettext wählt sie wie jeden Text nach der Locale
/// (ohne Katalog Englisch).
pub fn online_hilfe() -> String {
    // TRANSLATORS: Adresse der Online-Hilfe; nur das Sprachkürzel anpassen
    // (deutsch …/de/docs/).
    gettext("https://querencialinux.org/en/docs/")
}

/// Öffnet `uri` mit dem Standardprogramm der Sitzung (wie xdg-open).
pub fn uri_oeffnen(eltern: &impl IsA<gtk::Window>, uri: &str) {
    let kontext = WidgetExt::display(eltern.as_ref()).app_launch_context();
    if let Err(e) = gio::AppInfo::launch_default_for_uri(uri, Some(&kontext)) {
        eprintln!("querencia-archiv: {uri} nicht geöffnet: {e}");
    }
}

fn antworten<T>(a: &Antwort<T>, wert: T) {
    if let Some(tx) = a.borrow_mut().take() {
        let _ = tx.send(wert);
    }
}

/// Leeres Nebenfenster, verborgen statt zerstört; Escape schließt.
fn fenster(eltern: Option<&gtk::Window>, modal: bool) -> gtk::Window {
    let w = gtk::Window::builder()
        .modal(modal)
        .hide_on_close(true)
        .resizable(false)
        .build();
    w.set_transient_for(eltern);
    w.add_css_class("qa-dialog");
    let keys = gtk::EventControllerKey::new();
    let ww = w.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            if let Some(w) = ww.upgrade() {
                w.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    w.add_controller(keys);
    w
}

/// Zeigt ein (wieder)verwendetes Nebenfenster. Ein verborgenes Fenster,
/// das GTK 4.22 erneut abbildet, bekäme von labwc sonst keine Titelleiste
/// mehr (Befund Archiv 0.1.0 und Dokumente 0.1.0). Ein verborgenes Fenster
/// wird deshalb vorher „unrealisiert“: GTK legt beim Zeigen eine neue
/// Oberfläche an und fordert die Dekoration neu an. (`set_decorated` hin
/// und her führte zu einem Wayland-Protokollfehler.)
fn zeigen(w: &gtk::Window) {
    if !w.is_visible() && w.is_realized() {
        WidgetExt::unrealize(w);
    }
    w.present();
}

/// Kehrt zurück, sobald der Benutzer das Fenster schließt (Knopf, Escape,
/// Titelleiste); die Fenster werden nur verborgen, nie zerstört.
async fn bis_verborgen(w: &gtk::Window) {
    if !w.is_visible() {
        return;
    }
    let (tx, rx) = oneshot::channel::<()>();
    let tx = RefCell::new(Some(tx));
    let id = w.connect_hide(move |_| {
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    });
    let _ = rx.await;
    w.disconnect(id);
}

/// Symbol links, fette Hauptzeile und Erklärung daneben (GNOME-2-Meldung).
fn meldung_inhalt(symbol: &str, primaer: &str, sekundaer: &str) -> gtk::Box {
    let img = gtk::Image::from_icon_name(symbol);
    img.set_pixel_size(48);
    img.set_valign(gtk::Align::Start);
    let titel = gtk::Label::new(None);
    titel.set_markup(&format!(
        "<b><big>{}</big></b>",
        glib::markup_escape_text(primaer)
    ));
    titel.set_xalign(0.0);
    titel.set_wrap(true);
    titel.set_max_width_chars(48);
    let text = gtk::Label::new(Some(sekundaer));
    text.set_xalign(0.0);
    text.set_wrap(true);
    text.set_max_width_chars(56);
    text.set_selectable(true);
    text.set_visible(!sekundaer.is_empty());
    let spalte = gtk::Box::new(gtk::Orientation::Vertical, 8);
    spalte.append(&titel);
    spalte.append(&text);
    let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    zeile.append(&img);
    zeile.append(&spalte);
    zeile
}

/// Inhalt mit Rand und einer Knopfreihe rechts unten.
fn mit_knoepfen(inhalt: &impl IsA<gtk::Widget>, knoepfe: &[&gtk::Button]) -> gtk::Box {
    let reihe = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    reihe.set_halign(gtk::Align::End);
    reihe.set_margin_top(18);
    for k in knoepfe {
        reihe.append(*k);
    }
    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.set_margin_top(12);
    aussen.set_margin_bottom(12);
    aussen.set_margin_start(12);
    aussen.set_margin_end(12);
    aussen.append(inhalt);
    aussen.append(&reihe);
    aussen
}

fn knopf(text: &str) -> gtk::Button {
    gtk::Button::with_mnemonic(text)
}

/// Fortschrittsfenster mit „Abbrechen“.
pub struct Fortschritt {
    pub fenster: gtk::Window,
    titel: gtk::Label,
    detail: gtk::Label,
    balken: gtk::ProgressBar,
    stand: RefCell<Option<Stand>>,
}

impl Fortschritt {
    fn neu(eltern: Option<&gtk::Window>) -> Rc<Fortschritt> {
        let fenster = fenster(eltern, eltern.is_some());
        let titel = gtk::Label::new(None);
        titel.set_xalign(0.0);
        titel.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        titel.set_max_width_chars(50);
        let detail = gtk::Label::new(None);
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        detail.set_max_width_chars(50);
        detail.add_css_class("dim-label");
        let balken = gtk::ProgressBar::new();
        balken.set_show_text(true);
        balken.set_size_request(380, -1);
        let spalte = gtk::Box::new(gtk::Orientation::Vertical, 8);
        spalte.append(&titel);
        spalte.append(&balken);
        spalte.append(&detail);
        let abbrechen = knopf(&gettext("_Cancel"));
        // Kein Fokus auf „Abbrechen“: ein verirrter Eingabe- oder Leertastendruck
        // (z. B. aus dem vorigen Fenster) bräche sonst die Arbeit ab. Escape,
        // Alt+A und Klick brechen ab.
        abbrechen.set_can_focus(false);
        fenster.set_child(Some(&mit_knoepfen(&spalte, &[&abbrechen])));
        let f = Rc::new(Fortschritt {
            fenster,
            titel,
            detail,
            balken,
            stand: RefCell::new(None),
        });
        let w = Rc::downgrade(&f);
        abbrechen.connect_clicked(move |_| {
            if let Some(f) = w.upgrade() {
                f.abbrechen();
            }
        });
        let w = Rc::downgrade(&f);
        f.fenster.connect_close_request(move |_| {
            if let Some(f) = w.upgrade() {
                f.abbrechen();
            }
            glib::Propagation::Stop
        });
        f
    }

    pub fn abbrechen(&self) {
        if let Some(s) = self.stand.borrow().as_ref() {
            s.abbruch.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.detail.set_text(&gettext("Cancelling…"));
    }

    pub fn beginnen(&self, fenstertitel: &str, titel: &str, stand: &Stand) {
        self.fenster.set_title(Some(fenstertitel));
        self.titel
            .set_markup(&format!("<b>{}</b>", glib::markup_escape_text(titel)));
        self.detail.set_text("");
        self.balken.set_fraction(0.0);
        self.balken.set_text(None);
        self.stand.replace(Some(stand.clone()));
    }

    pub fn setzen(&self, fertig: u64, gesamt: u64, name: &str) {
        let anteil = if gesamt > 0 {
            (fertig as f64 / gesamt as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.balken.set_fraction(anteil);
        // TRANSLATORS: Fortschrittsbalken, z. B. „12.5 MB of 1.0 GB“
        self.balken.set_text(Some(&gettext_f(
            "{done} of {total}",
            &[
                ("done", &anzeige::groesse(fertig)),
                ("total", &anzeige::groesse(gesamt)),
            ],
        )));
        if !name.is_empty() {
            self.detail.set_text(name);
        }
    }

    pub fn zeigen(&self) {
        zeigen(&self.fenster);
    }

    pub fn verbergen(&self) {
        self.fenster.set_visible(false);
    }

    pub fn ende(&self) {
        self.stand.replace(None);
        self.fenster.set_visible(false);
    }
}

/// Kennwortabfrage.
struct Passwort {
    fenster: gtk::Window,
    text: gtk::Label,
    hinweis: gtk::Label,
    feld: gtk::PasswordEntry,
    antwort: Antwort<Option<String>>,
}

/// Rückfrage bei vorhandenen Dateien.
struct KonfliktFenster {
    fenster: gtk::Window,
    titel: gtk::Label,
    text: gtk::Label,
    alle: gtk::CheckButton,
    antwort: Antwort<(Konflikt, bool)>,
}

/// Ziel und Umfang beim Entpacken.
struct EntpackFenster {
    fenster: gtk::Window,
    ziel: gtk::Entry,
    alle: gtk::CheckButton,
    auswahl: gtk::CheckButton,
    antwort: Antwort<Option<(PathBuf, bool)>>,
}

/// Name, Ort und Format eines neuen Archivs.
struct NeuFenster {
    fenster: gtk::Window,
    name: gtk::Entry,
    ort: gtk::Entry,
    antwort: Antwort<Option<(PathBuf, Format)>>,
}

/// Alle Nebenfenster eines Hauptfensters.
pub struct Dialoge {
    eltern: Option<gtk::Window>,
    /// Bricht offene Dateidialoge ab, wenn das Hauptfenster schließt (ein
    /// offener GTK-Dateidialog hielte das Programm sonst nach Strg+Q am Leben).
    pub abbruch: gio::Cancellable,
    meldung: OnceCell<gtk::Window>,
    frage: OnceCell<(gtk::Window, Antwort<bool>)>,
    passwort: OnceCell<Passwort>,
    konflikt: OnceCell<KonfliktFenster>,
    fortschritt: OnceCell<Rc<Fortschritt>>,
    entpacken: OnceCell<EntpackFenster>,
    neu: OnceCell<NeuFenster>,
    info: OnceCell<gtk::Window>,
}

fn ordner_waehlen(
    eltern: &gtk::Window,
    feld: &gtk::Entry,
    titel: &str,
    abbruch: &gio::Cancellable,
) {
    let d = gtk::FileDialog::builder().title(titel).modal(true).build();
    let start = gio::File::for_path(feld.text().as_str());
    if start.query_exists(None::<&gio::Cancellable>) {
        d.set_initial_folder(Some(&start));
    }
    let feld = feld.clone();
    d.select_folder(Some(eltern), Some(abbruch), move |r| {
        if let Ok(f) = r
            && let Some(p) = f.path()
        {
            feld.set_text(&p.to_string_lossy());
        }
    });
}

fn beschriftung(text: &str, feld: &impl IsA<gtk::Widget>) -> gtk::Label {
    let l = gtk::Label::with_mnemonic(text);
    l.set_xalign(0.0);
    l.set_mnemonic_widget(Some(feld));
    l
}

impl Dialoge {
    pub fn neu(eltern: Option<&gtk::Window>) -> Dialoge {
        Dialoge {
            eltern: eltern.cloned(),
            abbruch: gio::Cancellable::new(),
            meldung: OnceCell::new(),
            frage: OnceCell::new(),
            passwort: OnceCell::new(),
            konflikt: OnceCell::new(),
            fortschritt: OnceCell::new(),
            entpacken: OnceCell::new(),
            neu: OnceCell::new(),
            info: OnceCell::new(),
        }
    }

    fn eltern(&self) -> Option<&gtk::Window> {
        self.eltern.as_ref()
    }

    /// Meldung mit „Schließen“; kehrt zurück, wenn sie geschlossen ist.
    pub async fn meldung(&self, symbol: &str, titel: &str, primaer: &str, sekundaer: &str) {
        let w = self.meldung.get_or_init(|| fenster(self.eltern(), true));
        w.set_title(Some(titel));
        let schliessen = knopf(&gettext("_Close"));
        let ww = w.downgrade();
        schliessen.connect_clicked(move |_| {
            if let Some(w) = ww.upgrade() {
                w.close();
            }
        });
        w.set_child(Some(&mit_knoepfen(
            &meldung_inhalt(symbol, primaer, sekundaer),
            &[&schliessen],
        )));
        w.set_default_widget(Some(&schliessen));
        GtkWindowExt::set_focus(w, Some(&schliessen));
        zeigen(w);
        bis_verborgen(w).await;
    }

    pub async fn fehler(&self, primaer: &str, sekundaer: &str) {
        self.meldung("dialog-error", &gettext("Error"), primaer, sekundaer)
            .await;
    }

    /// Ja/Nein; `ja` ist die Beschriftung des bestätigenden Knopfs.
    pub async fn frage(&self, primaer: &str, sekundaer: &str, ja: &str) -> bool {
        let (w, antwort) = self.frage.get_or_init(|| {
            let w = fenster(self.eltern(), true);
            let a: Antwort<bool> = Rc::new(RefCell::new(None));
            let aa = a.clone();
            w.connect_close_request(move |_| {
                antworten(&aa, false);
                glib::Propagation::Proceed
            });
            (w, a)
        });
        w.set_title(Some(APP_NAME));
        let nein = knopf(&gettext("_Cancel"));
        let jaknopf = knopf(ja);
        jaknopf.add_css_class("destructive-action");
        for (k, wert) in [(&nein, false), (&jaknopf, true)] {
            let aa = antwort.clone();
            let ww = w.downgrade();
            k.connect_clicked(move |_| {
                antworten(&aa, wert);
                if let Some(w) = ww.upgrade() {
                    w.set_visible(false);
                }
            });
        }
        w.set_child(Some(&mit_knoepfen(
            &meldung_inhalt("dialog-question", primaer, sekundaer),
            &[&nein, &jaknopf],
        )));
        GtkWindowExt::set_focus(w, Some(&nein));
        let (tx, rx) = oneshot::channel();
        antwort.replace(Some(tx));
        zeigen(w);
        rx.await.unwrap_or(false)
    }

    fn passwort_fenster(&self) -> &Passwort {
        self.passwort.get_or_init(|| {
            let fenster = fenster(self.eltern(), true);
            fenster.set_title(Some(&gettext("Password Required")));
            let titel = gtk::Label::new(None);
            titel.set_markup(&format!(
                "<b><big>{}</big></b>",
                glib::markup_escape_text(&gettext("This archive is encrypted"))
            ));
            titel.set_xalign(0.0);
            let text = gtk::Label::new(None);
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_max_width_chars(52);
            let feld = gtk::PasswordEntry::new();
            feld.set_show_peek_icon(true);
            feld.set_hexpand(true);
            feld.set_activates_default(true);
            let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            zeile.append(&beschriftung(&gettext("_Password:"), &feld));
            zeile.append(&feld);
            let hinweis = gtk::Label::new(None);
            hinweis.set_xalign(0.0);
            hinweis.add_css_class("error");
            let spalte = gtk::Box::new(gtk::Orientation::Vertical, 8);
            for w in [
                titel.upcast_ref::<gtk::Widget>(),
                text.upcast_ref(),
                zeile.upcast_ref(),
                hinweis.upcast_ref(),
            ] {
                spalte.append(w);
            }
            let img = gtk::Image::from_icon_name("dialog-password");
            img.set_pixel_size(48);
            img.set_valign(gtk::Align::Start);
            let oben = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            oben.append(&img);
            oben.append(&spalte);
            let abbrechen = knopf(&gettext("_Cancel"));
            let ok = knopf(&gettext("_OK"));
            ok.add_css_class("suggested-action");
            fenster.set_child(Some(&mit_knoepfen(&oben, &[&abbrechen, &ok])));
            fenster.set_default_widget(Some(&ok));
            let antwort: Antwort<Option<String>> = Rc::new(RefCell::new(None));
            let (a, w) = (antwort.clone(), fenster.downgrade());
            abbrechen.connect_clicked(move |_| {
                antworten(&a, None);
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let (a, w, f) = (antwort.clone(), fenster.downgrade(), feld.clone());
            ok.connect_clicked(move |_| {
                antworten(&a, Some(f.text().to_string()));
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let a = antwort.clone();
            fenster.connect_close_request(move |_| {
                antworten(&a, None);
                glib::Propagation::Proceed
            });
            Passwort {
                fenster,
                text,
                hinweis,
                feld,
                antwort,
            }
        })
    }

    /// Fragt nach dem Kennwort für `name`; `None` = abgebrochen.
    pub async fn passwort(&self, name: &str, falsch: bool) -> Option<String> {
        let p = self.passwort_fenster();
        p.text.set_text(&gettext_f(
            "Enter the password for “{name}”.",
            &[("name", name)],
        ));
        p.hinweis.set_text(&if falsch {
            gettext("Incorrect password. Please try again.")
        } else {
            String::new()
        });
        p.hinweis.set_visible(falsch);
        p.feld.set_text("");
        let (tx, rx) = oneshot::channel();
        p.antwort.replace(Some(tx));
        zeigen(&p.fenster);
        p.feld.grab_focus();
        rx.await.ok().flatten()
    }

    fn konflikt_fenster(&self) -> &KonfliktFenster {
        self.konflikt.get_or_init(|| {
            let fenster = fenster(self.eltern(), true);
            fenster.set_title(Some(&gettext("File Already Exists")));
            let titel = gtk::Label::new(None);
            titel.set_xalign(0.0);
            titel.set_wrap(true);
            titel.set_max_width_chars(48);
            let text = gtk::Label::new(None);
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_max_width_chars(56);
            let alle = gtk::CheckButton::with_mnemonic(&gettext("_Apply this action to all files"));
            let spalte = gtk::Box::new(gtk::Orientation::Vertical, 8);
            spalte.append(&titel);
            spalte.append(&text);
            spalte.append(&alle);
            let img = gtk::Image::from_icon_name("dialog-warning");
            img.set_pixel_size(48);
            img.set_valign(gtk::Align::Start);
            let oben = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            oben.append(&img);
            oben.append(&spalte);
            let abbrechen = knopf(&gettext("_Cancel"));
            let ueberspringen = knopf(&gettext("_Skip"));
            let ersetzen = knopf(&gettext("_Replace"));
            fenster.set_child(Some(&mit_knoepfen(
                &oben,
                &[&abbrechen, &ueberspringen, &ersetzen],
            )));
            fenster.set_default_widget(Some(&ersetzen));
            let antwort: Antwort<(Konflikt, bool)> = Rc::new(RefCell::new(None));
            for (k, wert) in [
                (&abbrechen, Konflikt::Abbrechen),
                (&ueberspringen, Konflikt::Ueberspringen),
                (&ersetzen, Konflikt::Ersetzen),
            ] {
                let (a, w, al) = (antwort.clone(), fenster.downgrade(), alle.clone());
                k.connect_clicked(move |_| {
                    antworten(&a, (wert, al.is_active()));
                    if let Some(w) = w.upgrade() {
                        w.set_visible(false);
                    }
                });
            }
            let a = antwort.clone();
            fenster.connect_close_request(move |_| {
                antworten(&a, (Konflikt::Abbrechen, false));
                glib::Propagation::Proceed
            });
            KonfliktFenster {
                fenster,
                titel,
                text,
                alle,
                antwort,
            }
        })
    }

    /// „Datei existiert bereits“: Antwort und ob sie für alle gilt.
    pub async fn konflikt(&self, ziel: &Path) -> (Konflikt, bool) {
        let k = self.konflikt_fenster();
        let name = ziel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ordner = ziel
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        k.titel.set_markup(&format!(
            "<b><big>{}</big></b>",
            glib::markup_escape_text(&gettext_f(
                "A file named “{name}” already exists.",
                &[("name", &name)]
            ))
        ));
        k.text.set_text(&gettext_f(
            "Replacing it will overwrite its contents in “{folder}”.",
            &[("folder", &ordner)],
        ));
        k.alle.set_active(false);
        let (tx, rx) = oneshot::channel();
        k.antwort.replace(Some(tx));
        zeigen(&k.fenster);
        rx.await.unwrap_or((Konflikt::Abbrechen, false))
    }

    pub fn fortschritt(&self) -> Rc<Fortschritt> {
        self.fortschritt
            .get_or_init(|| Fortschritt::neu(self.eltern()))
            .clone()
    }

    fn entpack_fenster(&self) -> &EntpackFenster {
        self.entpacken.get_or_init(|| {
            let fenster = fenster(self.eltern(), true);
            fenster.set_title(Some(&gettext("Extract")));
            let ziel = gtk::Entry::new();
            ziel.set_hexpand(true);
            ziel.set_width_chars(40);
            ziel.set_activates_default(true);
            let waehlen = knopf(&gettext("_Browse…"));
            let raster = gtk::Grid::new();
            raster.set_row_spacing(8);
            raster.set_column_spacing(8);
            raster.attach(&beschriftung(&gettext("Extract _to:"), &ziel), 0, 0, 1, 1);
            raster.attach(&ziel, 1, 0, 1, 1);
            raster.attach(&waehlen, 2, 0, 1, 1);
            let kopf = gtk::Label::new(None);
            kopf.set_markup(&format!(
                "<b>{}</b>",
                glib::markup_escape_text(&gettext("Files"))
            ));
            kopf.set_xalign(0.0);
            kopf.set_margin_top(8);
            let alle = gtk::CheckButton::with_mnemonic(&gettext("_All files"));
            let auswahl = gtk::CheckButton::with_mnemonic(&gettext("_Selected files"));
            auswahl.set_group(Some(&alle));
            for k in [&alle, &auswahl] {
                k.set_margin_start(12);
            }
            raster.attach(&kopf, 0, 1, 3, 1);
            raster.attach(&alle, 0, 2, 3, 1);
            raster.attach(&auswahl, 0, 3, 3, 1);
            let abbrechen = knopf(&gettext("_Cancel"));
            let los = knopf(&gettext("_Extract"));
            los.add_css_class("suggested-action");
            fenster.set_child(Some(&mit_knoepfen(&raster, &[&abbrechen, &los])));
            fenster.set_default_widget(Some(&los));
            let antwort: Antwort<Option<(PathBuf, bool)>> = Rc::new(RefCell::new(None));
            let (w, z, ab) = (fenster.downgrade(), ziel.clone(), self.abbruch.clone());
            waehlen.connect_clicked(move |_| {
                if let Some(w) = w.upgrade() {
                    ordner_waehlen(&w, &z, &gettext("Extract"), &ab);
                }
            });
            let (a, w) = (antwort.clone(), fenster.downgrade());
            abbrechen.connect_clicked(move |_| {
                antworten(&a, None);
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let (a, w, z, al) = (
                antwort.clone(),
                fenster.downgrade(),
                ziel.clone(),
                alle.clone(),
            );
            los.connect_clicked(move |_| {
                let text = z.text().to_string();
                if text.trim().is_empty() {
                    return;
                }
                antworten(&a, Some((PathBuf::from(text.trim()), al.is_active())));
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let a = antwort.clone();
            fenster.connect_close_request(move |_| {
                antworten(&a, None);
                glib::Propagation::Proceed
            });
            EntpackFenster {
                fenster,
                ziel,
                alle,
                auswahl,
                antwort,
            }
        })
    }

    /// Fragt Ziel und Umfang; `Some((ziel, alle))`.
    pub async fn entpacken(&self, vorschlag: &Path, auswahl_da: bool) -> Option<(PathBuf, bool)> {
        let e = self.entpack_fenster();
        e.ziel.set_text(&vorschlag.to_string_lossy());
        e.auswahl.set_sensitive(auswahl_da);
        if auswahl_da {
            e.auswahl.set_active(true);
        } else {
            e.alle.set_active(true);
        }
        let (tx, rx) = oneshot::channel();
        e.antwort.replace(Some(tx));
        zeigen(&e.fenster);
        e.ziel.grab_focus();
        rx.await.ok().flatten()
    }

    fn neu_fenster(&self) -> &NeuFenster {
        self.neu.get_or_init(|| {
            let fenster = fenster(self.eltern(), true);
            fenster.set_title(Some(&gettext("New Archive")));
            let name = gtk::Entry::new();
            name.set_width_chars(32);
            name.set_activates_default(true);
            let ort = gtk::Entry::new();
            ort.set_hexpand(true);
            ort.set_activates_default(true);
            let waehlen = knopf(&gettext("_Browse…"));
            let endungen: Vec<&str> = ERSTELLBAR.iter().map(|f| f.endung()).collect();
            let format = gtk::DropDown::from_strings(&endungen);
            format.set_halign(gtk::Align::Start);
            let raster = gtk::Grid::new();
            raster.set_row_spacing(8);
            raster.set_column_spacing(8);
            raster.attach(&beschriftung(&gettext("_Name:"), &name), 0, 0, 1, 1);
            raster.attach(&name, 1, 0, 2, 1);
            raster.attach(&beschriftung(&gettext("_Location:"), &ort), 0, 1, 1, 1);
            raster.attach(&ort, 1, 1, 1, 1);
            raster.attach(&waehlen, 2, 1, 1, 1);
            raster.attach(
                &beschriftung(&gettext("Archive _type:"), &format),
                0,
                2,
                1,
                1,
            );
            raster.attach(&format, 1, 2, 2, 1);
            let abbrechen = knopf(&gettext("_Cancel"));
            let los = knopf(&gettext("C_reate"));
            los.add_css_class("suggested-action");
            fenster.set_child(Some(&mit_knoepfen(&raster, &[&abbrechen, &los])));
            fenster.set_default_widget(Some(&los));
            let antwort: Antwort<Option<(PathBuf, Format)>> = Rc::new(RefCell::new(None));
            let (w, o, ab) = (fenster.downgrade(), ort.clone(), self.abbruch.clone());
            waehlen.connect_clicked(move |_| {
                if let Some(w) = w.upgrade() {
                    ordner_waehlen(&w, &o, &gettext("New Archive"), &ab);
                }
            });
            let (a, w) = (antwort.clone(), fenster.downgrade());
            abbrechen.connect_clicked(move |_| {
                antworten(&a, None);
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let (a, w, n, o, f) = (
                antwort.clone(),
                fenster.downgrade(),
                name.clone(),
                ort.clone(),
                format.clone(),
            );
            los.connect_clicked(move |_| {
                let format = ERSTELLBAR[(f.selected() as usize).min(ERSTELLBAR.len() - 1)];
                let mut name = n.text().trim().to_string();
                if name.is_empty() || name.contains('/') {
                    return;
                }
                if Format::aus_namen(&name) != Some(format) {
                    name.push_str(format.endung());
                }
                antworten(&a, Some((Path::new(o.text().trim()).join(name), format)));
                if let Some(w) = w.upgrade() {
                    w.set_visible(false);
                }
            });
            let a = antwort.clone();
            fenster.connect_close_request(move |_| {
                antworten(&a, None);
                glib::Propagation::Proceed
            });
            NeuFenster {
                fenster,
                name,
                ort,
                antwort,
            }
        })
    }

    /// Fragt Name, Ort und Format eines neuen Archivs.
    pub async fn neues_archiv(&self, ort: &Path, name: &str) -> Option<(PathBuf, Format)> {
        let n = self.neu_fenster();
        n.name.set_text(name);
        n.ort.set_text(&ort.to_string_lossy());
        let (tx, rx) = oneshot::channel();
        n.antwort.replace(Some(tx));
        zeigen(&n.fenster);
        n.name.grab_focus();
        rx.await.ok().flatten()
    }

    pub fn info(&self) {
        let w = self.info.get_or_init(|| {
            let w = fenster(self.eltern(), false);
            // TRANSLATORS: Fenstertitel des Info-Dialogs; deutsch mit dem
            // Eigennamen „Querencia Archiv“, englisch mit dem Namen aus dem
            // Anwendungsmenü.
            w.set_title(Some(&gettext("About Archive Manager")));
            let inhalt = gtk::Box::new(gtk::Orientation::Vertical, 8);
            inhalt.set_margin_top(6);
            inhalt.set_margin_start(12);
            inhalt.set_margin_end(12);
            let img = gtk::Image::from_icon_name(resource::INFO_ICON);
            img.set_pixel_size(64);
            inhalt.append(&img);
            let name = gtk::Label::new(None);
            // TRANSLATORS: Programmname und Version im Info-Dialog; deutsch
            // „Querencia Archiv {version}“.
            let titel = gettext_f(
                "Archive Manager {version}",
                &[("version", env!("CARGO_PKG_VERSION"))],
            );
            name.set_markup(&format!(
                "<b><big>{}</big></b>",
                glib::markup_escape_text(&titel)
            ));
            let text = gtk::Label::new(Some(&gettext(
                "Classic archive manager for Querencia Desktop\nmodelled on File Roller 2.30.\n\n\
                 Reads every format libarchive knows, writes zip, tar.gz and tar.xz.",
            )));
            text.set_justify(gtk::Justification::Center);
            let alpha = gtk::Label::new(Some(&gettext("Alpha – not released for everyday use.")));
            let website = gtk::Label::new(None);
            website.set_markup(&format!("<a href=\"{WEBSITE}\">querencialinux.org</a>"));
            // Wie die Einträge im Hilfe-Menü über GIO statt über das Portal öffnen.
            let ww = w.downgrade();
            website.connect_activate_link(move |_, uri| {
                if let Some(w) = ww.upgrade() {
                    uri_oeffnen(&w, uri);
                }
                glib::Propagation::Stop
            });
            let copyright = gtk::Label::new(Some("© 2026 Querencia Desktop contributors"));
            copyright.add_css_class("dim-label");
            let lizenz = gtk::Label::new(Some(&gettext("License: GPL-3.0-or-later")));
            lizenz.add_css_class("dim-label");
            let formate = gtk::Label::new(Some(&gettext("Archive formats: libarchive (BSD)")));
            formate.add_css_class("dim-label");
            for x in [
                name.upcast_ref::<gtk::Widget>(),
                text.upcast_ref(),
                alpha.upcast_ref(),
                website.upcast_ref(),
                copyright.upcast_ref(),
                lizenz.upcast_ref(),
                formate.upcast_ref(),
            ] {
                inhalt.append(x);
            }
            let schliessen = knopf(&gettext("_Close"));
            let ww = w.downgrade();
            schliessen.connect_clicked(move |_| {
                if let Some(w) = ww.upgrade() {
                    w.close();
                }
            });
            w.set_child(Some(&mit_knoepfen(&inhalt, &[&schliessen])));
            w.set_default_widget(Some(&schliessen));
            w
        });
        if let Some(k) = w.default_widget() {
            GtkWindowExt::set_focus(w, Some(&k));
        }
        zeigen(w);
    }
}
