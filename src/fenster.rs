//! Das Archivfenster: Menüleiste, Werkzeugleiste, Ortsleiste, Ordnerbaum
//! und Dateiliste, Statusleiste.
//!
//! Aufbau wie Engrampa 1.28 / File Roller 2.30: Archive | Edit | View |
//! Help (deutsch Archiv | Bearbeiten | Ansicht | Hilfe aus `po/de.po`),
//! Werkzeugleiste Neu, Öffnen, Entpacken, Hinzufügen, darunter die
//! Ortsleiste (Zurück, Vor, Hoch, Anfang, „Ort:“). Jedes Archiv hat ein
//! eigenes Fenster; ein leeres Fenster wird wiederverwendet. Keine eigene
//! Titelleiste: labwc zeichnet die Server-Dekoration.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use archiv::entpacken::{self, Auftrag, Auswahl};
use archiv::format::Format;
use archiv::i18n::{gettext, gettext_f, ngettext_f, pgettext};
use archiv::lesen::{self, Archiv, Fehler as LeseFehler, Stand};
use archiv::modell::Inhalt;
use archiv::schreiben::{self, Sammlung};
use archiv::{APP_NAME, anzeige};

use crate::aufgabe::{self, Anzeige};
use crate::dialoge::Dialoge;
use crate::zeile::{OrdnerZeile, Zeile};

const CSS: &str = include_str!("style.css");

/// Diagnose auf stderr (Zeiten, Zahl der Einträge), nie Archivinhalte.
fn diagnose() -> bool {
    std::env::var_os("QUERENCIA_ARCHIV_DEBUG").is_some()
}

pub fn css_laden() {
    let p = gtk::CssProvider::new();
    p.load_from_string(CSS);
    if let Some(d) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &d,
            &p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// QD-081: GTK 4.22 lädt bei „prefer-dark“ das dunkle Thema-CSS nur bei einem
/// Wechsel im laufenden Programm; ein Start bliebe hell. Die Meldung stößt das
/// Nachladen einmal an (hell bleibt unverändert). Die eigene CSS braucht keine
/// Bindung an das Farbschema: Sie hat keine `@media`-Regel und nimmt Linien
/// aus `currentColor`, das dem Thema folgt.
pub fn farbschema_anstossen() {
    if let Some(s) = gtk::Settings::default() {
        s.notify("gtk-interface-color-scheme");
    }
}

/// GNOME Human statt Adwaita, wenn der Benutzer GTKs Standard behalten hat
/// (dieselbe Regel wie Panel, Files, Bilder und Dokumente).
pub fn klassische_symbole() {
    let Some(s) = gtk::Settings::default() else {
        return;
    };
    let jetzt = s
        .gtk_icon_theme_name()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let da = glib::system_data_dirs()
        .iter()
        .any(|d| d.join("icons/gnome-human/index.theme").is_file());
    if (jetzt.is_empty() || jetzt == "Adwaita") && da {
        s.set_gtk_icon_theme_name(Some("gnome-human"));
    }
}

/// Kürzel. Löschen, Alles auswählen und Rücktaste gelten nur in der Liste
/// (liste_tasten), damit das Ortsfeld sie behält.
pub fn kuerzel(app: &gtk::Application) {
    for (aktion, tasten) in [
        ("win.neu", &["<Control>n"][..]),
        ("win.oeffnen", &["<Control>o"]),
        ("win.entpacken", &["<Control>e"]),
        ("win.schliessen", &["<Control>w"]),
        ("win.beenden", &["<Control>q"]),
        ("win.ordnerleiste", &["F9"]),
        ("win.neu-laden", &["<Control>r", "F5"]),
        ("win.zurueck", &["<Alt>Left"]),
        ("win.vor", &["<Alt>Right"]),
        ("win.hoch", &["<Alt>Up"]),
        ("win.anfang", &["<Alt>Home"]),
    ] {
        app.set_accels_for_action(aktion, tasten);
    }
}

fn eintrag(text: &str, aktion: &str, kuerzel: Option<&str>) -> gio::MenuItem {
    let i = gio::MenuItem::new(Some(text), Some(aktion));
    if let Some(k) = kuerzel {
        i.set_attribute_value("accel", Some(&k.to_variant()));
    }
    i
}

fn abschnitt(eintraege: &[gio::MenuItem]) -> gio::Menu {
    let m = gio::Menu::new();
    for e in eintraege {
        m.append_item(e);
    }
    m
}

fn menue(abschnitte: &[gio::Menu]) -> gio::Menu {
    let m = gio::Menu::new();
    for a in abschnitte {
        m.append_section(None, a);
    }
    m
}

/// Menüleiste; Zugriffstasten (_) je Menü eindeutig, je Sprache für sich.
pub fn menueleiste() -> gio::Menu {
    let leiste = gio::Menu::new();
    leiste.append_submenu(
        Some(&gettext("_Archive")),
        &menue(&[
            abschnitt(&[
                eintrag(&gettext("_New…"), "win.neu", None),
                eintrag(&gettext("_Open…"), "win.oeffnen", None),
            ]),
            abschnitt(&[eintrag(&gettext("_Extract…"), "win.entpacken", None)]),
            abschnitt(&[
                // TRANSLATORS: Menü „Archiv“; Knöpfe in Dialogen nehmen „_Close“ ohne Kontext
                eintrag(&pgettext("Archive menu", "_Close"), "win.schliessen", None),
                eintrag(&gettext("_Quit"), "win.beenden", None),
            ]),
        ]),
    );
    leiste.append_submenu(
        Some(&gettext("_Edit")),
        &menue(&[
            abschnitt(&[
                eintrag(&gettext("_Add Files…"), "win.dateien-hinzufuegen", None),
                eintrag(&gettext("Add a _Folder…"), "win.ordner-hinzufuegen", None),
                eintrag(&gettext("_Delete"), "win.entfernen", Some("Delete")),
            ]),
            abschnitt(&[
                eintrag(
                    &gettext("_Select All"),
                    "win.alles-auswaehlen",
                    Some("<Control>a"),
                ),
                eintrag(
                    &gettext("D_eselect All"),
                    "win.auswahl-aufheben",
                    Some("<Shift><Control>a"),
                ),
            ]),
        ]),
    );
    leiste.append_submenu(
        Some(&gettext("_View")),
        &menue(&[
            abschnitt(&[
                eintrag(&gettext("_Toolbar"), "win.werkzeugleiste", None),
                eintrag(&gettext("_Statusbar"), "win.statusleiste", None),
                eintrag(&gettext("_Folders"), "win.ordnerleiste", None),
            ]),
            abschnitt(&[eintrag(&gettext("_Reload"), "win.neu-laden", None)]),
        ]),
    );
    leiste.append_submenu(
        Some(&gettext("_Help")),
        &menue(&[
            abschnitt(&[
                // TRANSLATORS: Hilfe-Menü; Online-Hilfe und Fehlermeldung öffnen
                // den Browser. Zugriffstasten im Menü verschieden halten.
                eintrag(&gettext("Online _Help"), "win.online-hilfe", None),
                eintrag(&gettext("_Report a Bug"), "win.fehler-melden", None),
            ]),
            abschnitt(&[eintrag(&gettext("_About"), "win.info", None)]),
        ]),
    );
    leiste
}

/// Werkzeugknopf mit Ersatzsymbolen (das Querencia-Thema kennt nicht jedes
/// freedesktop-Symbol, z. B. document-new).
fn werkzeug(symbole: &[&str], text: &str, hinweis: &str, aktion: &str) -> gtk::Button {
    let bild = gtk::Image::from_gicon(&gio::ThemedIcon::from_names(symbole));
    bild.set_icon_size(gtk::IconSize::Large);
    let beschriftung = gtk::Label::new(Some(text));
    let inhalt = gtk::Box::new(gtk::Orientation::Vertical, 2);
    inhalt.append(&bild);
    inhalt.append(&beschriftung);
    let b = gtk::Button::new();
    b.set_child(Some(&inhalt));
    b.set_tooltip_text(Some(hinweis));
    b.set_action_name(Some(aktion));
    b.add_css_class("flat");
    b.set_focus_on_click(false);
    // Die Tastatur gehört der Liste und dem Ortsfeld (wie in Dokumente).
    b.set_can_focus(false);
    b
}

fn ortsknopf(symbol: &str, hinweis: &str, aktion: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(symbol);
    b.set_tooltip_text(Some(hinweis));
    b.set_action_name(Some(aktion));
    b.add_css_class("flat");
    b.set_focus_on_click(false);
    // Die Tastatur gehört der Liste und dem Ortsfeld (wie in Dokumente).
    b.set_can_focus(false);
    b
}

/// Das geöffnete Archiv.
struct Offen {
    pfad: PathBuf,
    archiv: Archiv,
    /// Noch nicht auf der Platte („Neu“ ohne Dateien).
    existiert: bool,
    passwort: Option<String>,
}

/// Ohne offenes Archiv (leeres Fenster, während des Ladens) gesperrt.
const ARCHIV_AKTIONEN: [&str; 7] = [
    "entpacken",
    "entfernen",
    "alles-auswaehlen",
    "auswahl-aufheben",
    "neu-laden",
    "anfang",
    "hoch",
];

thread_local! {
    static FENSTER: RefCell<Vec<Rc<Fenster>>> = const { RefCell::new(Vec::new()) };
}

pub struct Fenster {
    pub window: gtk::ApplicationWindow,
    dialoge: Dialoge,
    werkzeugleiste: gtk::Box,
    ortsleiste: gtk::Box,
    ort: gtk::Entry,
    links: gtk::ScrolledWindow,
    baum_auswahl: gtk::SingleSelection,
    liste: gtk::ColumnView,
    liste_daten: gio::ListStore,
    liste_auswahl: gtk::MultiSelection,
    stapel: gtk::Stack,
    statusleiste: gtk::Box,
    status: gtk::Label,
    offen: Rc<RefCell<Option<Offen>>>,
    ordner: RefCell<String>,
    zurueck: RefCell<Vec<String>>,
    vor: RefCell<Vec<String>>,
    /// Baumauswahl wird gerade aus dem Programm gesetzt
    baum_sync: Cell<bool>,
    beschaeftigt: Cell<bool>,
    hinweis: RefCell<String>,
    /// Das Fenster ist gezeichnet (siehe oeffnen).
    gezeichnet: Cell<bool>,
}

type FensterAktion = fn(&Rc<Fenster>);

impl Fenster {
    /// Ein leeres Fenster (wiederverwenden) oder ein neues.
    pub fn leer_oder_neu(app: &gtk::Application) -> Rc<Fenster> {
        let leer = FENSTER.with(|f| {
            f.borrow()
                .iter()
                .find(|w| w.offen.borrow().is_none() && !w.beschaeftigt.get())
                .cloned()
        });
        leer.unwrap_or_else(|| Fenster::neu(app))
    }

    pub fn irgendeins(app: &gtk::Application) -> Rc<Fenster> {
        FENSTER
            .with(|f| f.borrow().first().cloned())
            .unwrap_or_else(|| Fenster::neu(app))
    }

    fn neu(app: &gtk::Application) -> Rc<Fenster> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(APP_NAME)
            .default_width(760)
            .default_height(520)
            .show_menubar(true)
            .build();
        window.add_css_class("qa-fenster");
        let dialoge = Dialoge::neu(Some(window.upcast_ref()));

        // Werkzeugleiste wie Engrampa
        let werkzeugleiste = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        werkzeugleiste.add_css_class("qa-werkzeugleiste");
        werkzeugleiste.append(&werkzeug(
            &["document-new", "package-x-generic"],
            // TRANSLATORS: Werkzeugleiste, kurz
            &gettext("New"),
            &gettext("Create a new archive"),
            "win.neu",
        ));
        werkzeugleiste.append(&werkzeug(
            &["document-open"],
            &gettext("Open"),
            &gettext("Open archive"),
            "win.oeffnen",
        ));
        werkzeugleiste.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        werkzeugleiste.append(&werkzeug(
            &["extract-archive", "archive-extract", "document-save"],
            &gettext("Extract"),
            &gettext("Extract files from the archive"),
            "win.entpacken",
        ));
        werkzeugleiste.append(&werkzeug(
            &["add-files-to-archive", "archive-insert", "list-add"],
            &gettext("Add"),
            &gettext("Add files to the archive"),
            "win.dateien-hinzufuegen",
        ));

        // Ortsleiste
        let ortsleiste = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        ortsleiste.add_css_class("qa-ortsleiste");
        ortsleiste.append(&ortsknopf(
            "go-previous",
            &gettext("Go to the previous visited location"),
            "win.zurueck",
        ));
        ortsleiste.append(&ortsknopf(
            "go-next",
            &gettext("Go to the next visited location"),
            "win.vor",
        ));
        ortsleiste.append(&ortsknopf("go-up", &gettext("Go up one level"), "win.hoch"));
        ortsleiste.append(&ortsknopf(
            "go-home",
            &gettext("Go to the home location"),
            "win.anfang",
        ));
        let ort = gtk::Entry::new();
        ort.set_hexpand(true);
        let ort_text = gtk::Label::with_mnemonic(&gettext("_Location:"));
        ort_text.set_mnemonic_widget(Some(&ort));
        ort_text.set_margin_start(6);
        ort_text.set_margin_end(4);
        ortsleiste.append(&ort_text);
        ortsleiste.append(&ort);

        // Ordnerbaum
        let baum_auswahl = gtk::SingleSelection::new(None::<gio::ListModel>);
        baum_auswahl.set_autoselect(false);
        baum_auswahl.set_can_unselect(true);
        let baum_fabrik = gtk::SignalListItemFactory::new();
        baum_fabrik.connect_setup(|_, obj| {
            let Some(item) = obj.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let bild = gtk::Image::from_icon_name("folder");
            let text = gtk::Label::new(None);
            text.set_xalign(0.0);
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            zeile.append(&bild);
            zeile.append(&text);
            let aufklapp = gtk::TreeExpander::new();
            aufklapp.set_child(Some(&zeile));
            item.set_child(Some(&aufklapp));
        });
        baum_fabrik.connect_bind(|_, obj| {
            let Some(item) = obj.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(reihe) = item.item().and_downcast::<gtk::TreeListRow>() else {
                return;
            };
            let Some(aufklapp) = item.child().and_downcast::<gtk::TreeExpander>() else {
                return;
            };
            aufklapp.set_list_row(Some(&reihe));
            let Some(o) = reihe.item().and_downcast::<OrdnerZeile>() else {
                return;
            };
            if let Some(text) = aufklapp
                .child()
                .and_then(|z| z.last_child())
                .and_downcast::<gtk::Label>()
            {
                text.set_text(&o.name());
            }
        });
        let baum = gtk::ListView::new(Some(baum_auswahl.clone()), Some(baum_fabrik));
        baum.add_css_class("qa-baum");
        let links = gtk::ScrolledWindow::builder().child(&baum).build();
        links.set_size_request(160, -1);

        // Dateiliste
        let liste_daten = gio::ListStore::new::<Zeile>();
        let liste = gtk::ColumnView::new(None::<gtk::MultiSelection>);
        liste.set_show_column_separators(true);
        liste.add_css_class("qa-liste");
        let ordner_zuerst = gtk::CustomSorter::new(|a, b| {
            let o = |x: &glib::Object| x.downcast_ref::<Zeile>().is_some_and(|z| z.ist_ordner());
            o(b).cmp(&o(a)).into()
        });
        let sortierer = gtk::MultiSorter::new();
        sortierer.append(ordner_zuerst);
        if let Some(s) = liste.sorter() {
            sortierer.append(s);
        }
        let sortiert = gtk::SortListModel::new(Some(liste_daten.clone()), Some(sortierer));
        let liste_auswahl = gtk::MultiSelection::new(Some(sortiert));
        liste.set_model(Some(&liste_auswahl));
        let rechts = gtk::ScrolledWindow::builder().child(&liste).build();
        rechts.set_hexpand(true);

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&links));
        paned.set_end_child(Some(&rechts));
        paned.set_resize_start_child(false);
        paned.set_shrink_start_child(false);
        paned.set_position(190);

        // TRANSLATORS: leeres Fenster; Menüpfad und Kürzel wie in der Oberfläche
        let leer = gtk::Label::new(Some(&gettext(
            "No archive open.\n\nArchive → Open… (Ctrl+O), Archive → New… (Ctrl+N)\nor drop files here.",
        )));
        leer.set_justify(gtk::Justification::Center);
        leer.add_css_class("dim-label");
        let stapel = gtk::Stack::new();
        stapel.add_named(&leer, Some("leer"));
        stapel.add_named(&paned, Some("archiv"));
        stapel.set_vexpand(true);

        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_hexpand(true);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let statusleiste = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        statusleiste.add_css_class("qa-statusleiste");
        statusleiste.append(&status);

        let spalte = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spalte.append(&werkzeugleiste);
        spalte.append(&ortsleiste);
        spalte.append(&stapel);
        spalte.append(&statusleiste);
        window.set_child(Some(&spalte));

        let f = Rc::new(Fenster {
            window,
            dialoge,
            werkzeugleiste,
            ortsleiste,
            ort,
            links,
            baum_auswahl,
            liste,
            liste_daten,
            liste_auswahl,
            stapel,
            statusleiste,
            status,
            offen: Rc::new(RefCell::new(None)),
            ordner: RefCell::new(String::new()),
            zurueck: RefCell::new(Vec::new()),
            vor: RefCell::new(Vec::new()),
            baum_sync: Cell::new(false),
            beschaeftigt: Cell::new(false),
            hinweis: RefCell::new(String::new()),
            gezeichnet: Cell::new(false),
        });
        f.spalten_anlegen();
        f.aktionen_anlegen();
        f.signale_verbinden();
        f.zustand_setzen();
        FENSTER.with(|l| l.borrow_mut().push(f.clone()));
        f
    }

    pub fn present(&self) {
        self.window.present();
    }

    // ------------------------------------------------------------ Aufbau

    fn spalten_anlegen(self: &Rc<Self>) {
        fn fabrik(
            mit_symbol: bool,
            rechts: bool,
            text: fn(&Zeile) -> String,
        ) -> gtk::SignalListItemFactory {
            let fabrik = gtk::SignalListItemFactory::new();
            fabrik.connect_setup(move |_, obj| {
                let Some(item) = obj.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                if mit_symbol {
                    let bild = gtk::Image::new();
                    bild.set_pixel_size(16);
                    zeile.append(&bild);
                }
                let label = gtk::Label::new(None);
                label.set_xalign(if rechts { 1.0 } else { 0.0 });
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                zeile.append(&label);
                item.set_child(Some(&zeile));
            });
            fabrik.connect_bind(move |_, obj| {
                let Some(item) = obj.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let Some(z) = item.item().and_downcast::<Zeile>() else {
                    return;
                };
                let Some(zeile) = item.child().and_downcast::<gtk::Box>() else {
                    return;
                };
                let mut kind = zeile.first_child();
                if mit_symbol && let Some(bild) = kind.clone().and_downcast::<gtk::Image>() {
                    bild.set_from_gicon(&z.symbol());
                    kind = bild.next_sibling();
                }
                if let Some(label) = kind.and_downcast::<gtk::Label>() {
                    label.set_text(&text(&z));
                }
            });
            fabrik
        }
        let ausdruck = |e: &str| {
            gtk::PropertyExpression::new(Zeile::static_type(), None::<gtk::Expression>, e)
        };

        // TRANSLATORS: Spaltenköpfe der Dateiliste
        let name = gtk::ColumnViewColumn::new(
            Some(&gettext("Name")),
            Some(fabrik(true, false, |z| z.name())),
        );
        let s = gtk::StringSorter::new(Some(ausdruck("name")));
        s.set_ignore_case(true);
        name.set_sorter(Some(&s));
        name.set_expand(true);
        name.set_resizable(true);

        let groesse = gtk::ColumnViewColumn::new(
            Some(&gettext("Size")),
            Some(fabrik(false, true, |z| {
                if z.ist_ordner() {
                    String::new()
                } else {
                    anzeige::groesse(z.groesse())
                }
            })),
        );
        groesse.set_sorter(Some(&gtk::NumericSorter::new(Some(ausdruck("groesse")))));
        groesse.set_fixed_width(90);
        groesse.set_resizable(true);

        let typ = gtk::ColumnViewColumn::new(
            Some(&gettext("Type")),
            Some(fabrik(false, false, |z| z.typ())),
        );
        let s = gtk::StringSorter::new(Some(ausdruck("typ")));
        s.set_ignore_case(true);
        typ.set_sorter(Some(&s));
        typ.set_fixed_width(150);
        typ.set_resizable(true);

        let datum = gtk::ColumnViewColumn::new(
            Some(&gettext("Date Modified")),
            Some(fabrik(false, false, |z| anzeige::datum(z.geaendert()))),
        );
        datum.set_sorter(Some(&gtk::NumericSorter::new(Some(ausdruck("geaendert")))));
        datum.set_fixed_width(130);
        datum.set_resizable(true);

        for c in [&name, &groesse, &typ, &datum] {
            self.liste.append_column(c);
        }
        self.liste
            .sort_by_column(Some(&name), gtk::SortType::Ascending);
    }

    fn aktionen_anlegen(self: &Rc<Self>) {
        let aktionen: [(&str, FensterAktion); 16] = [
            ("neu", |f| f.neu_fragen(Vec::new())),
            ("oeffnen", |f| f.oeffnen_fragen()),
            ("entpacken", |f| f.entpacken_fragen()),
            ("dateien-hinzufuegen", |f| f.hinzufuegen_fragen(false)),
            ("ordner-hinzufuegen", |f| f.hinzufuegen_fragen(true)),
            ("entfernen", |f| f.entfernen_fragen()),
            ("alles-auswaehlen", |f| {
                f.liste_auswahl.select_all();
            }),
            ("auswahl-aufheben", |f| {
                f.liste_auswahl.unselect_all();
            }),
            ("neu-laden", |f| f.neu_laden()),
            ("zurueck", |f| f.verlauf(true)),
            ("vor", |f| f.verlauf(false)),
            ("hoch", |f| {
                let o = f.ordner.borrow().clone();
                if !o.is_empty() {
                    let eltern = o
                        .rsplit_once('/')
                        .map(|(e, _)| e.to_string())
                        .unwrap_or_default();
                    f.gehe_zu(&eltern, true);
                }
            }),
            ("anfang", |f| f.gehe_zu("", true)),
            ("online-hilfe", |f| {
                crate::dialoge::uri_oeffnen(&f.window, &crate::dialoge::online_hilfe())
            }),
            ("fehler-melden", |f| {
                crate::dialoge::uri_oeffnen(&f.window, crate::dialoge::FEHLER_MELDEN)
            }),
            ("info", |f| f.dialoge.info()),
            ("schliessen", |f| f.window.close()),
            ("beenden", |f| {
                if let Some(app) = f.window.application() {
                    for w in app.windows() {
                        w.close();
                    }
                }
            }),
        ];
        for (name, tun) in aktionen {
            let a = gio::SimpleAction::new(name, None);
            let w = Rc::downgrade(self);
            a.connect_activate(move |_, _| {
                if let Some(f) = w.upgrade() {
                    tun(&f);
                }
            });
            self.window.add_action(&a);
        }
        // Umschalter im Menü „Ansicht“
        for (name, widget) in [
            (
                "werkzeugleiste",
                self.werkzeugleiste.clone().upcast::<gtk::Widget>(),
            ),
            ("statusleiste", self.statusleiste.clone().upcast()),
            ("ordnerleiste", self.links.clone().upcast()),
        ] {
            let a = gio::SimpleAction::new_stateful(name, None, &true.to_variant());
            a.connect_activate(move |a, _| {
                let an = !a.state().and_then(|s| s.get::<bool>()).unwrap_or(true);
                a.set_state(&an.to_variant());
                widget.set_visible(an);
            });
            self.window.add_action(&a);
        }
    }

    fn aktion(&self, name: &str) -> Option<gio::SimpleAction> {
        self.window
            .lookup_action(name)
            .and_downcast::<gio::SimpleAction>()
    }

    fn signale_verbinden(self: &Rc<Self>) {
        // Baum → Ordner
        let w = Rc::downgrade(self);
        self.baum_auswahl.connect_selection_changed(move |s, _, _| {
            let Some(f) = w.upgrade() else { return };
            if f.baum_sync.get() {
                return;
            }
            if let Some(o) = s
                .selected_item()
                .and_downcast::<gtk::TreeListRow>()
                .and_then(|r| r.item())
                .and_downcast::<OrdnerZeile>()
            {
                f.gehe_zu(&o.pfad(), true);
            }
        });
        // Doppelklick/Eingabe in der Liste
        let w = Rc::downgrade(self);
        self.liste.connect_activate(move |_, pos| {
            let Some(f) = w.upgrade() else { return };
            if let Some(z) = f.liste_auswahl.item(pos).and_downcast::<Zeile>() {
                if z.ist_ordner() {
                    f.gehe_zu(&z.ordnerpfad(), true);
                } else if !z.unsicher() {
                    f.vorschau(z.index() as usize);
                }
            }
        });
        let w = Rc::downgrade(self);
        self.liste_auswahl
            .connect_selection_changed(move |_, _, _| {
                if let Some(f) = w.upgrade() {
                    f.status_setzen();
                }
            });
        let tasten = gtk::EventControllerKey::new();
        let w = Rc::downgrade(self);
        tasten.connect_key_pressed(move |_, taste, _, mods| {
            let Some(f) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let strg = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let a =
                |n: &str| WidgetExt::activate_action(&f.window, &format!("win.{n}"), None).is_ok();
            let erledigt = match taste {
                gdk::Key::Delete | gdk::Key::KP_Delete if !strg => a("entfernen"),
                gdk::Key::a | gdk::Key::A if strg && shift => a("auswahl-aufheben"),
                gdk::Key::a | gdk::Key::A if strg => a("alles-auswaehlen"),
                gdk::Key::BackSpace => a("hoch"),
                _ => false,
            };
            if erledigt {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.liste.add_controller(tasten);
        // GTK 4.22 lässt den Fokus nach Escape auf der Menüleiste stehen
        // (danach liefen Pos1/Eingabe in die Menüs): Escape auf der
        // Menüleiste selbst gibt ihn an die Liste zurück; ein offenes Menü
        // schließt Escape wie gewohnt.
        let menue_esc = gtk::EventControllerKey::new();
        menue_esc.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = Rc::downgrade(self);
        menue_esc.connect_key_pressed(move |_, taste, _, _| {
            let Some(f) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if taste != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            let fokus = GtkWindowExt::focus(&f.window);
            let in_leiste = fokus.as_ref().is_some_and(|w| {
                w.ancestor(gtk::PopoverMenuBar::static_type()).is_some()
                    && w.ancestor(gtk::Popover::static_type()).is_none()
            });
            if in_leiste {
                f.liste.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(menue_esc);
        // Ortsfeld: Eingabe springt in den Ordner
        let w = Rc::downgrade(self);
        self.ort.connect_activate(move |e| {
            let Some(f) = w.upgrade() else { return };
            let t = e.text().trim().trim_matches('/').to_string();
            let da = f
                .offen
                .borrow()
                .as_ref()
                .is_some_and(|o| o.archiv.inhalt.ordner(&t).is_some());
            if da {
                f.gehe_zu(&t, true);
                f.liste.grab_focus();
            } else {
                f.ort_setzen();
                f.window.error_bell();
            }
        });
        // Dateien ins Fenster ziehen
        let ablage = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let w = Rc::downgrade(self);
        ablage.connect_drop(move |_, wert, _, _| {
            let (Some(f), Ok(liste)) = (w.upgrade(), wert.get::<gdk::FileList>()) else {
                return false;
            };
            let pfade: Vec<PathBuf> = liste.files().iter().filter_map(|d| d.path()).collect();
            if pfade.is_empty() || f.beschaeftigt.get() {
                return false;
            }
            f.abgelegt(pfade);
            true
        });
        self.window.add_controller(ablage);
        let w = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(f) = w.upgrade() {
                // Läuft noch eine Arbeit, bricht Schließen sie zuerst ab; das
                // Fenster bleibt, bis der Faden fertig ist (sonst liefen
                // Rückfragen und Ergebnisse in ein geschlossenes Fenster).
                if f.beschaeftigt.get() {
                    f.dialoge.fortschritt().abbrechen();
                    return glib::Propagation::Stop;
                }
                f.dialoge.abbruch.cancel();
                FENSTER.with(|l| l.borrow_mut().retain(|x| !Rc::ptr_eq(x, &f)));
            }
            glib::Propagation::Proceed
        });
    }

    // ------------------------------------------------------- Zustand

    fn zustand_setzen(&self) {
        let offen = self.offen.borrow();
        let da = offen.is_some();
        for n in ARCHIV_AKTIONEN {
            if let Some(a) = self.aktion(n) {
                a.set_enabled(da && !self.beschaeftigt.get());
            }
        }
        let aenderbar = offen.as_ref().is_none_or(|o| o.archiv.format.aenderbar());
        for n in [
            "dateien-hinzufuegen",
            "ordner-hinzufuegen",
            "neu",
            "oeffnen",
        ] {
            if let Some(a) = self.aktion(n) {
                a.set_enabled(
                    !self.beschaeftigt.get() && (n == "neu" || n == "oeffnen" || aenderbar),
                );
            }
        }
        let nichts_auf_platte = offen.as_ref().is_some_and(|o| !o.existiert);
        if let Some(a) = self.aktion("entpacken") {
            a.set_enabled(da && !nichts_auf_platte && !self.beschaeftigt.get());
        }
        if let Some(a) = self.aktion("entfernen") {
            a.set_enabled(da && aenderbar && !nichts_auf_platte && !self.beschaeftigt.get());
        }
        drop(offen);
        if let Some(a) = self.aktion("zurueck") {
            a.set_enabled(da && !self.zurueck.borrow().is_empty());
        }
        if let Some(a) = self.aktion("vor") {
            a.set_enabled(da && !self.vor.borrow().is_empty());
        }
        if let Some(a) = self.aktion("hoch") {
            a.set_enabled(da && !self.ordner.borrow().is_empty());
        }
        self.ortsleiste.set_sensitive(da);
        self.stapel
            .set_visible_child_name(if da { "archiv" } else { "leer" });
    }

    fn titel_setzen(&self) {
        match self.offen.borrow().as_ref() {
            Some(o) => {
                let name = o
                    .pfad
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.window.set_title(Some(&format!("{name} – {APP_NAME}")));
            }
            None => self.window.set_title(Some(APP_NAME)),
        }
    }

    fn ort_setzen(&self) {
        let o = self.ordner.borrow();
        self.ort.set_text(&if o.is_empty() {
            "/".to_string()
        } else {
            format!("/{o}/")
        });
    }

    fn status_setzen(&self) {
        let offen = self.offen.borrow();
        let Some(o) = offen.as_ref() else {
            self.status.set_text("");
            return;
        };
        let inhalt = &o.archiv.inhalt;
        let zeilen = self.ausgewaehlt();
        let text = if zeilen.is_empty() {
            let ordner = self.ordner.borrow();
            let n = inhalt
                .ordner(&ordner)
                .map(|x| x.ordner.len() + x.dateien.len())
                .unwrap_or(0)
                + if ordner.is_empty() {
                    inhalt.unsicher.len()
                } else {
                    0
                };
            ngettext_f(
                "{n} object ({size})",
                "{n} objects ({size})",
                n as u32,
                &[("size", &anzeige::groesse(inhalt.groesse_unter(&ordner)))],
            )
        } else {
            let mut groesse = 0;
            for z in &zeilen {
                groesse += if z.ist_ordner() {
                    inhalt.groesse_unter(&z.ordnerpfad())
                } else {
                    z.groesse()
                };
            }
            ngettext_f(
                "{n} object selected ({size})",
                "{n} objects selected ({size})",
                zeilen.len() as u32,
                &[("size", &anzeige::groesse(groesse))],
            )
        };
        let mut teile = vec![text];
        if !inhalt.unsicher.is_empty() {
            teile.push(ngettext_f(
                "Warning: {n} entry with an unsafe path is not extracted",
                "Warning: {n} entries with unsafe paths are not extracted",
                inhalt.unsicher.len() as u32,
                &[],
            ));
        }
        if !o.archiv.format.aenderbar() && !o.archiv.format.kurzname().is_empty() {
            teile.push(gettext_f(
                "{format} (read only)",
                &[("format", o.archiv.format.kurzname())],
            ));
        }
        let hinweis = self.hinweis.borrow();
        if !hinweis.is_empty() {
            teile.push(hinweis.clone());
        }
        self.status.set_text(&teile.join(" – "));
    }

    fn ausgewaehlt(&self) -> Vec<Zeile> {
        let auswahl = self.liste_auswahl.selection();
        let mut aus = Vec::new();
        if let Some((mut iter, erste)) = gtk::BitsetIter::init_first(&auswahl) {
            let mut pos = Some(erste);
            while let Some(p) = pos {
                if let Some(z) = self.liste_auswahl.item(p).and_downcast::<Zeile>() {
                    aus.push(z);
                }
                pos = iter.next();
            }
        }
        aus
    }

    // ------------------------------------------------------- Navigation

    fn gehe_zu(self: &Rc<Self>, ordner: &str, merken: bool) {
        let alt = self.ordner.borrow().clone();
        {
            let offen = self.offen.borrow();
            let Some(o) = offen.as_ref() else { return };
            if o.archiv.inhalt.ordner(ordner).is_none() {
                return;
            }
        }
        if merken && alt != ordner {
            self.zurueck.borrow_mut().push(alt);
            self.vor.borrow_mut().clear();
        }
        self.ordner.replace(ordner.to_string());
        self.liste_fuellen();
        self.ort_setzen();
        self.baum_waehlen(ordner);
        self.zustand_setzen();
        self.status_setzen();
    }

    fn verlauf(self: &Rc<Self>, zurueck: bool) {
        let (von, nach) = if zurueck {
            (&self.zurueck, &self.vor)
        } else {
            (&self.vor, &self.zurueck)
        };
        let Some(ziel) = von.borrow_mut().pop() else {
            return;
        };
        nach.borrow_mut().push(self.ordner.borrow().clone());
        self.gehe_zu(&ziel, false);
    }

    fn liste_fuellen(&self) {
        let offen = self.offen.borrow();
        let Some(o) = offen.as_ref() else {
            self.liste_daten.remove_all();
            return;
        };
        let inhalt = &o.archiv.inhalt;
        let ordner = self.ordner.borrow();
        let mut zeilen: Vec<Zeile> = Vec::new();
        if let Some(x) = inhalt.ordner(&ordner) {
            for name in &x.ordner {
                let pfad = if ordner.is_empty() {
                    name.clone()
                } else {
                    format!("{ordner}/{name}")
                };
                let eintrag = inhalt.ordner(&pfad).and_then(|u| u.eintrag);
                zeilen.push(Zeile::ordner(
                    name,
                    &pfad,
                    eintrag.map(|i| &inhalt.eintraege[i]),
                    eintrag,
                ));
            }
            for &i in &x.dateien {
                zeilen.push(Zeile::datei(&inhalt.eintraege[i], i));
            }
        }
        if ordner.is_empty() {
            for &i in &inhalt.unsicher {
                zeilen.push(Zeile::datei(&inhalt.eintraege[i], i));
            }
        }
        self.liste_daten
            .splice(0, self.liste_daten.n_items(), &zeilen);
        self.liste_auswahl.unselect_all();
    }

    fn baum_aufbauen(&self, name: &str) {
        let offen = self.offen.clone();
        let wurzel = gio::ListStore::new::<OrdnerZeile>();
        wurzel.append(&OrdnerZeile::neu(name, ""));
        let modell = gtk::TreeListModel::new(wurzel, false, false, move |obj| {
            let o = obj.downcast_ref::<OrdnerZeile>()?;
            let offen = offen.borrow();
            let inhalt = &offen.as_ref()?.archiv.inhalt;
            let pfad = o.pfad();
            let x = inhalt.ordner(&pfad)?;
            if x.ordner.is_empty() {
                return None;
            }
            let kinder = gio::ListStore::new::<OrdnerZeile>();
            for n in &x.ordner {
                let p = if pfad.is_empty() {
                    n.clone()
                } else {
                    format!("{pfad}/{n}")
                };
                kinder.append(&OrdnerZeile::neu(n, &p));
            }
            Some(kinder.upcast())
        });
        self.baum_sync.set(true);
        self.baum_auswahl.set_model(Some(&modell));
        if let Some(r) = modell.row(0) {
            r.set_expanded(true);
        }
        self.baum_sync.set(false);
    }

    /// Wählt `ordner` im Baum und klappt die Eltern auf.
    fn baum_waehlen(&self, ordner: &str) {
        let Some(modell) = self
            .baum_auswahl
            .model()
            .and_downcast::<gtk::TreeListModel>()
        else {
            return;
        };
        let finden = |pfad: &str| -> Option<(u32, gtk::TreeListRow)> {
            (0..modell.n_items()).find_map(|i| {
                let r = modell.row(i)?;
                let o = r.item().and_downcast::<OrdnerZeile>()?;
                (o.pfad() == pfad).then_some((i, r))
            })
        };
        self.baum_sync.set(true);
        let mut weg = String::new();
        for teil in ordner.split('/').filter(|t| !t.is_empty()) {
            if let Some((_, r)) = finden(&weg) {
                r.set_expanded(true);
            }
            if !weg.is_empty() {
                weg.push('/');
            }
            weg.push_str(teil);
        }
        if let Some((_, r)) = finden(&weg) {
            r.set_expanded(true);
        }
        match finden(ordner) {
            Some((i, _)) => self.baum_auswahl.set_selected(i),
            None => self.baum_auswahl.set_selected(gtk::INVALID_LIST_POSITION),
        }
        self.baum_sync.set(false);
    }

    // ------------------------------------------------------- Öffnen

    fn oeffnen_fragen(self: &Rc<Self>) {
        let d = gtk::FileDialog::builder()
            .title(gettext("Open"))
            .modal(true)
            .build();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("All archives")));
        for m in crate::MIME_TYPEN {
            filter.add_mime_type(m);
        }
        let alle = gtk::FileFilter::new();
        alle.set_name(Some(&gettext("All files")));
        alle.add_pattern("*");
        let filter_liste = gio::ListStore::new::<gtk::FileFilter>();
        filter_liste.append(&filter);
        filter_liste.append(&alle);
        d.set_filters(Some(&filter_liste));
        d.set_default_filter(Some(&filter));
        if let Some(o) = self.offen.borrow().as_ref()
            && let Some(p) = o.pfad.parent()
        {
            d.set_initial_folder(Some(&gio::File::for_path(p)));
        }
        let w = Rc::downgrade(self);
        d.open_multiple(Some(&self.window), Some(&self.dialoge.abbruch), move |r| {
            let (Some(f), Ok(liste)) = (w.upgrade(), r) else {
                return;
            };
            let Some(app) = f.window.application() else {
                return;
            };
            for (i, datei) in liste.iter::<gio::File>().flatten().enumerate() {
                let Some(p) = datei.path() else { continue };
                let ziel = if i == 0 && f.offen.borrow().is_none() {
                    f.clone()
                } else {
                    Fenster::leer_oder_neu(&app)
                };
                ziel.present();
                ziel.oeffnen(p);
            }
        });
    }

    /// Öffnet `pfad` in diesem Fenster (Hintergrund, Kennwort bei Bedarf).
    /// Erst nach dem dritten gezeichneten Bild: Kennwort-, Fehler- und
    /// Fortschrittsfenster brauchen ein Elternfenster, das labwc schon kennt
    /// (sonst erscheinen sie ohne Titelleiste, Befund Dokumente 0.1.0).
    pub fn oeffnen(self: &Rc<Self>, pfad: PathBuf) {
        let f = self.clone();
        self.nach_dem_zeichnen(move || {
            glib::spawn_future_local(async move { f.laden(pfad, None, "").await });
        });
    }

    fn nach_dem_zeichnen(&self, tun: impl FnOnce() + 'static) {
        if self.gezeichnet.get() {
            tun();
            return;
        }
        let tun = RefCell::new(Some(tun));
        let bilder = Cell::new(0);
        let w = self.window.downgrade();
        self.window.add_tick_callback(move |_, _| {
            bilder.set(bilder.get() + 1);
            if bilder.get() < 3 {
                return glib::ControlFlow::Continue;
            }
            if let Some(f) = w
                .upgrade()
                .and_then(|w| FENSTER.with(|l| l.borrow().iter().find(|x| x.window == w).cloned()))
            {
                f.gezeichnet.set(true);
            }
            if let Some(t) = tun.take() {
                glib::idle_add_local_once(t);
            }
            glib::ControlFlow::Break
        });
    }

    async fn laden(self: &Rc<Self>, pfad: PathBuf, passwort: Option<String>, ordner: &str) {
        let name = pfad
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut pw = passwort;
        let mut falsch = false;
        loop {
            self.beschaeftigt.set(true);
            self.zustand_setzen();
            let stand = Stand::default();
            let (p, s, k) = (pfad.clone(), stand.clone(), pw.clone());
            let beginn = std::time::Instant::now();
            let r = aufgabe::laufen(
                &self.dialoge,
                Anzeige {
                    fenstertitel: &gettext("Opening Archive"),
                    titel: &gettext_f("Reading “{name}”", &[("name", &name)]),
                    lesestand_von: Some(&pfad),
                },
                &stand,
                move |_| lesen::auflisten(&p, k.as_deref(), &s),
            )
            .await;
            self.beschaeftigt.set(false);
            match r {
                Some(Ok(archiv)) => {
                    if diagnose() {
                        eprintln!(
                            "querencia-archiv: geöffnet in {} ms, {} Einträge, {} Dateien, {} Byte",
                            beginn.elapsed().as_millis(),
                            archiv.inhalt.eintraege.len(),
                            archiv.inhalt.dateien,
                            archiv.inhalt.groesse
                        );
                    }
                    self.offen.replace(Some(Offen {
                        pfad: pfad.clone(),
                        archiv,
                        existiert: true,
                        passwort: pw,
                    }));
                    self.angezeigt(&name, ordner);
                    return;
                }
                Some(Err(LeseFehler::PasswortNoetig)) | Some(Err(LeseFehler::PasswortFalsch)) => {
                    match self.dialoge.passwort(&name, falsch).await {
                        Some(k) => {
                            pw = Some(k);
                            falsch = true;
                        }
                        None => break,
                    }
                }
                Some(Err(LeseFehler::Abgebrochen)) => break,
                Some(Err(e)) => {
                    self.lesefehler_zeigen(&name, &e).await;
                    break;
                }
                None => {
                    self.dialoge
                        .fehler(
                            &gettext_f("Could not open “{name}”.", &[("name", &name)]),
                            &gettext("Internal error."),
                        )
                        .await;
                    break;
                }
            }
        }
        self.zustand_setzen();
    }

    fn angezeigt(self: &Rc<Self>, name: &str, ordner: &str) {
        self.zurueck.borrow_mut().clear();
        self.vor.borrow_mut().clear();
        self.baum_aufbauen(name);
        self.titel_setzen();
        let ziel = if self
            .offen
            .borrow()
            .as_ref()
            .is_some_and(|o| o.archiv.inhalt.ordner(ordner).is_some())
        {
            ordner.to_string()
        } else {
            String::new()
        };
        self.ordner.replace(ziel.clone());
        self.liste_fuellen();
        self.ort_setzen();
        self.baum_waehlen(&ziel);
        self.zustand_setzen();
        self.status_setzen();
        self.liste.grab_focus();
    }

    async fn lesefehler_zeigen(&self, name: &str, e: &LeseFehler) {
        let primaer = gettext_f("Could not open “{name}”.", &[("name", name)]);
        let sekundaer = match e {
            LeseFehler::Format => gettext("Archive type not supported."),
            LeseFehler::Io(err) => gettext_f(
                "The file could not be read.\n\nDetails: {detail}",
                &[("detail", &err.to_string())],
            ),
            LeseFehler::Archiv(m) => gettext_f(
                "The archive is damaged or cannot be read.\n\nDetails: {detail}",
                &[("detail", m)],
            ),
            _ => gettext("The archive is encrypted."),
        };
        self.dialoge.fehler(&primaer, &sekundaer).await;
    }

    fn neu_laden(self: &Rc<Self>) {
        let (pfad, pw, existiert) = match self.offen.borrow().as_ref() {
            Some(o) => (o.pfad.clone(), o.passwort.clone(), o.existiert),
            None => return,
        };
        if !existiert {
            return;
        }
        let ordner = self.ordner.borrow().clone();
        let f = self.clone();
        glib::spawn_future_local(async move { f.laden(pfad, pw, &ordner).await });
    }

    // ------------------------------------------------------- Neu

    fn neu_fragen(self: &Rc<Self>, danach: Vec<PathBuf>) {
        let f = self.clone();
        glib::spawn_future_local(async move {
            let ort = danach
                .first()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .or_else(|| {
                    f.offen
                        .borrow()
                        .as_ref()
                        .and_then(|o| o.pfad.parent().map(Path::to_path_buf))
                })
                .unwrap_or_else(glib::home_dir);
            let vorschlag = match danach.as_slice() {
                [eins] => eins
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                // TRANSLATORS: Vorschlag für den Namen eines neuen Archivs
                _ => gettext("New Archive"),
            };
            let Some((pfad, format)) = f.dialoge.neues_archiv(&ort, &vorschlag).await else {
                return;
            };
            if archiv::pfade::vorhanden(&pfad) != archiv::pfade::Vorhanden::Nichts {
                let name = pfad
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                f.dialoge
                    .fehler(
                        &gettext_f("Could not create “{name}”.", &[("name", &name)]),
                        &gettext("A file with this name already exists. Choose another name."),
                    )
                    .await;
                return;
            }
            if !pfad.parent().is_some_and(|p| p.is_dir()) {
                f.dialoge
                    .fehler(
                        &gettext("Could not create the archive."),
                        &gettext("The folder does not exist."),
                    )
                    .await;
                return;
            }
            let ziel = if f.offen.borrow().is_some() {
                match f.window.application() {
                    Some(app) => Fenster::leer_oder_neu(&app),
                    None => return,
                }
            } else {
                f.clone()
            };
            ziel.present();
            ziel.offen.replace(Some(Offen {
                pfad: pfad.clone(),
                archiv: Archiv {
                    format,
                    roh: false,
                    inhalt: Inhalt::aus(Vec::new()),
                },
                existiert: false,
                passwort: None,
            }));
            let name = pfad
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ziel.angezeigt(&name, "");
            if !danach.is_empty() {
                ziel.hinzufuegen(danach).await;
            }
        });
    }

    // ------------------------------------------------------- Hinzufügen

    fn hinzufuegen_fragen(self: &Rc<Self>, ordner: bool) {
        let d = gtk::FileDialog::builder()
            .title(if ordner {
                gettext("Add a Folder")
            } else {
                gettext("Add Files")
            })
            .modal(true)
            .build();
        let w = Rc::downgrade(self);
        let fertig = move |r: Result<gio::ListModel, glib::Error>| {
            let (Some(f), Ok(liste)) = (w.upgrade(), r) else {
                return;
            };
            let pfade: Vec<PathBuf> = liste
                .iter::<gio::File>()
                .flatten()
                .filter_map(|d| d.path())
                .collect();
            if pfade.is_empty() {
                return;
            }
            if f.offen.borrow().is_none() {
                f.neu_fragen(pfade);
            } else {
                let f2 = f.clone();
                glib::spawn_future_local(async move { f2.hinzufuegen(pfade).await });
            }
        };
        if ordner {
            d.select_multiple_folders(Some(&self.window), Some(&self.dialoge.abbruch), fertig);
        } else {
            d.open_multiple(Some(&self.window), Some(&self.dialoge.abbruch), fertig);
        }
    }

    fn abgelegt(self: &Rc<Self>, pfade: Vec<PathBuf>) {
        if self.offen.borrow().is_some() {
            let f = self.clone();
            glib::spawn_future_local(async move { f.hinzufuegen(pfade).await });
            return;
        }
        // ohne Archiv: ein einzelnes Archiv öffnen, sonst ein neues anlegen
        if let [eins] = pfade.as_slice()
            && eins.is_file()
            && lesen::kopf(eins)
                .is_ok_and(|k| Format::erkennen(&k, &eins.to_string_lossy()) != Format::Sonstiges)
        {
            self.oeffnen(eins.clone());
            return;
        }
        self.neu_fragen(pfade);
    }

    /// Kann das offene Archiv neu geschrieben werden? Sonst Meldung.
    async fn aenderbar_pruefen(&self) -> bool {
        let grund = {
            let offen = self.offen.borrow();
            let Some(o) = offen.as_ref() else {
                return false;
            };
            if !o.archiv.format.aenderbar() || o.archiv.roh {
                Some(gettext_f(
                    "Querencia Archiv can only read {format} archives.",
                    &[("format", o.archiv.format.kurzname())],
                ))
            } else if o.archiv.inhalt.nicht_neu_schreibbar() {
                Some(gettext(
                    "The archive contains links, special files or unsafe paths that would be lost when it is rewritten.",
                ))
            } else if o.passwort.is_some() {
                Some(gettext("Encrypted archives cannot be changed yet."))
            } else {
                None
            }
        };
        match grund {
            Some(g) => {
                self.dialoge
                    .fehler(&gettext("The archive cannot be changed."), &g)
                    .await;
                false
            }
            None => true,
        }
    }

    async fn hinzufuegen(self: &Rc<Self>, pfade: Vec<PathBuf>) {
        if self.beschaeftigt.get() || !self.aenderbar_pruefen().await {
            return;
        }
        let (archivpfad, archiv, existiert) = match self.offen.borrow().as_ref() {
            Some(o) => (o.pfad.clone(), o.archiv.clone(), o.existiert),
            None => return,
        };
        // das Archiv selbst nie in sich aufnehmen
        let pfade: Vec<PathBuf> = pfade
            .into_iter()
            .filter(|p| std::fs::canonicalize(p).ok() != std::fs::canonicalize(&archivpfad).ok())
            .collect();
        if pfade.is_empty() {
            return;
        }
        let ordner = self.ordner.borrow().clone();
        let name = archivpfad
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.beschaeftigt.set(true);
        self.zustand_setzen();
        let stand = Stand::default();
        let s = stand.clone();
        let ap = archivpfad.clone();
        let r = aufgabe::laufen(
            &self.dialoge,
            Anzeige {
                fenstertitel: &gettext("Adding Files"),
                titel: &gettext_f("Adding files to “{name}”", &[("name", &name)]),
                lesestand_von: None,
            },
            &stand,
            move |bote| {
                let sammlung = Sammlung::von(&pfade, &ordner)?;
                if existiert {
                    schreiben::neu_schreiben(&ap, &archiv, &HashSet::new(), &sammlung, &s, bote)
                } else {
                    schreiben::erstellen(&ap, archiv.format, &sammlung, None, &s, bote)
                }
            },
        )
        .await;
        self.beschaeftigt.set(false);
        self.nach_schreiben(r, &name, &archivpfad).await;
    }

    async fn nach_schreiben(
        self: &Rc<Self>,
        r: Option<Result<schreiben::Ergebnis, schreiben::Fehler>>,
        name: &str,
        archivpfad: &Path,
    ) {
        match r {
            Some(Ok(e)) => {
                if let Some(o) = self.offen.borrow_mut().as_mut() {
                    o.existiert = true;
                }
                self.hinweis.replace(if e.uebersprungen > 0 {
                    ngettext_f(
                        "{n} link or special file was skipped",
                        "{n} links or special files were skipped",
                        e.uebersprungen as u32,
                        &[],
                    )
                } else {
                    String::new()
                });
                let ordner = self.ordner.borrow().clone();
                self.laden(archivpfad.to_path_buf(), None, &ordner).await;
            }
            Some(Err(schreiben::Fehler::Abgebrochen)) => self.zustand_setzen(),
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
                    schreiben::Fehler::Schreiben(m) => {
                        gettext_f("Details: {detail}", &[("detail", m)])
                    }
                    schreiben::Fehler::Verschluesselt => {
                        gettext("Encrypted archives cannot be changed yet.")
                    }
                    schreiben::Fehler::Lesen(l) => {
                        gettext_f("Details: {detail}", &[("detail", &l.to_string())])
                    }
                    _ => gettext("The archive cannot be changed."),
                };
                self.dialoge
                    .fehler(
                        &gettext_f("Could not update “{name}”.", &[("name", name)]),
                        &detail,
                    )
                    .await;
                self.zustand_setzen();
            }
            None => self.zustand_setzen(),
        }
    }

    // ------------------------------------------------------- Entfernen

    fn entfernen_fragen(self: &Rc<Self>) {
        let zeilen = self.ausgewaehlt();
        if zeilen.is_empty() || self.beschaeftigt.get() {
            return;
        }
        let f = self.clone();
        glib::spawn_future_local(async move {
            if !f.aenderbar_pruefen().await {
                return;
            }
            let ja = f
                .dialoge
                .frage(
                    &ngettext_f(
                        "Delete the selected object from the archive?",
                        "Delete the {n} selected objects from the archive?",
                        zeilen.len() as u32,
                        &[],
                    ),
                    &gettext("The archive is rewritten without them. This cannot be undone."),
                    &gettext("_Delete"),
                )
                .await;
            if !ja {
                return;
            }
            let (archivpfad, archiv) = match f.offen.borrow().as_ref() {
                Some(o) => (o.pfad.clone(), o.archiv.clone()),
                None => return,
            };
            let (dateien, ordner) = Self::zeilen_teilen(&zeilen);
            let weg: HashSet<usize> =
                match Auswahl::ordner_aus(&archiv.inhalt, &dateien, &ordner, "") {
                    Ok(a) => a.ziele.keys().copied().collect(),
                    Err(_) => return,
                };
            let name = archivpfad
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            f.beschaeftigt.set(true);
            f.zustand_setzen();
            let stand = Stand::default();
            let s = stand.clone();
            let ap = archivpfad.clone();
            let r = aufgabe::laufen(
                &f.dialoge,
                Anzeige {
                    fenstertitel: &gettext("Deleting Files"),
                    titel: &gettext_f("Deleting files from “{name}”", &[("name", &name)]),
                    lesestand_von: None,
                },
                &stand,
                move |bote| {
                    schreiben::neu_schreiben(&ap, &archiv, &weg, &Sammlung::default(), &s, bote)
                },
            )
            .await;
            f.beschaeftigt.set(false);
            f.nach_schreiben(r, &name, &archivpfad).await;
        });
    }

    fn zeilen_teilen(zeilen: &[Zeile]) -> (Vec<usize>, Vec<String>) {
        let mut dateien = Vec::new();
        let mut ordner = Vec::new();
        for z in zeilen {
            if z.ist_ordner() {
                ordner.push(z.ordnerpfad());
            } else {
                dateien.push(z.index() as usize);
            }
        }
        (dateien, ordner)
    }

    // ------------------------------------------------------- Entpacken

    fn entpacken_fragen(self: &Rc<Self>) {
        if self.beschaeftigt.get() {
            return;
        }
        let f = self.clone();
        glib::spawn_future_local(async move {
            let vorschlag = match f.offen.borrow().as_ref() {
                Some(o) => o
                    .pfad
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(glib::home_dir),
                None => return,
            };
            let zeilen = f.ausgewaehlt();
            let Some((ziel, alle)) = f.dialoge.entpacken(&vorschlag, !zeilen.is_empty()).await
            else {
                return;
            };
            let auswahl = {
                let offen = f.offen.borrow();
                let Some(o) = offen.as_ref() else { return };
                if alle {
                    Auswahl::alles(&o.archiv.inhalt)
                } else {
                    let (dateien, ordner) = Self::zeilen_teilen(&zeilen);
                    Auswahl::ordner_aus(&o.archiv.inhalt, &dateien, &ordner, &f.ordner.borrow())
                }
            };
            let auswahl = match auswahl {
                Ok(a) => a,
                Err(abgelehnt) => {
                    f.abgelehnt_zeigen(&abgelehnt.0).await;
                    return;
                }
            };
            if let Err(e) = std::fs::create_dir_all(&ziel) {
                f.dialoge
                    .fehler(
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
                return;
            }
            if let Some(e) = f.entpacken(auswahl, ziel.clone()).await {
                let mut teile = vec![ngettext_f(
                    "{n} file extracted to “{folder}”",
                    "{n} files extracted to “{folder}”",
                    e.dateien as u32,
                    &[("folder", &ziel.display().to_string())],
                )];
                if e.uebersprungen > 0 {
                    teile.push(ngettext_f(
                        "{n} skipped",
                        "{n} skipped",
                        e.uebersprungen as u32,
                        &[],
                    ));
                }
                if e.nicht_angelegt > 0 {
                    teile.push(ngettext_f(
                        "{n} link or special file not created",
                        "{n} links or special files not created",
                        e.nicht_angelegt as u32,
                        &[],
                    ));
                }
                f.hinweis.replace(teile.join(", "));
                f.status_setzen();
            }
        });
    }

    async fn abgelehnt_zeigen(&self, namen: &[String]) {
        let mut liste: Vec<String> = namen.iter().take(8).cloned().collect();
        if namen.len() > 8 {
            liste.push("…".into());
        }
        self.dialoge
            .fehler(
                &gettext("Extraction refused"),
                &gettext_f(
                    "The archive contains entries with unsafe paths (“..” or absolute). Nothing was extracted.\n\n{names}",
                    &[("names", &liste.join("\n"))],
                ),
            )
            .await;
    }

    /// Entpackt `auswahl` nach `ziel`; fragt bei Bedarf nach dem Kennwort.
    async fn entpacken(
        self: &Rc<Self>,
        auswahl: Auswahl,
        ziel: PathBuf,
    ) -> Option<entpacken::Ergebnis> {
        let (archivpfad, roh, mut pw) = match self.offen.borrow().as_ref() {
            Some(o) => (o.pfad.clone(), o.archiv.roh, o.passwort.clone()),
            None => return None,
        };
        let name = archivpfad
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let auswahl = Rc::new(auswahl);
        let mut erledigt = HashSet::new();
        let mut gesamt = entpacken::Ergebnis::default();
        let mut falsch = false;
        loop {
            self.beschaeftigt.set(true);
            self.zustand_setzen();
            let stand = Stand::default();
            let (s, a, z, k, ap) = (
                stand.clone(),
                (*auswahl).clone(),
                ziel.clone(),
                pw.clone(),
                archivpfad.clone(),
            );
            let mut fertig = std::mem::take(&mut erledigt);
            let beginn = std::time::Instant::now();
            let r = aufgabe::laufen(
                &self.dialoge,
                Anzeige {
                    fenstertitel: &gettext("Extracting Files"),
                    titel: &gettext_f("Extracting files from “{name}”", &[("name", &name)]),
                    lesestand_von: None,
                },
                &stand,
                move |bote| {
                    let r = entpacken::entpacken(
                        &Auftrag {
                            archiv: &ap,
                            roh,
                            ziel: &z,
                            auswahl: &a,
                            passwort: k.as_deref(),
                            stand: &s,
                        },
                        &mut fertig,
                        bote,
                    );
                    (r, fertig)
                },
            )
            .await;
            self.beschaeftigt.set(false);
            self.zustand_setzen();
            let Some((r, fertig)) = r else {
                self.dialoge
                    .fehler(
                        &gettext("An error occurred while extracting files."),
                        &gettext("Internal error."),
                    )
                    .await;
                return None;
            };
            erledigt = fertig;
            match r {
                Ok(e) => {
                    if diagnose() {
                        eprintln!(
                            "querencia-archiv: entpackt in {} ms, {} Dateien",
                            beginn.elapsed().as_millis(),
                            e.dateien
                        );
                    }
                    gesamt.dateien += e.dateien;
                    gesamt.ordner += e.ordner;
                    gesamt.uebersprungen += e.uebersprungen;
                    gesamt.nicht_angelegt += e.nicht_angelegt;
                    if pw.is_some()
                        && let Some(o) = self.offen.borrow_mut().as_mut()
                    {
                        o.passwort = pw.clone();
                    }
                    return Some(gesamt);
                }
                Err(entpacken::Fehler::Lesen(
                    LeseFehler::PasswortNoetig | LeseFehler::PasswortFalsch,
                )) => match self.dialoge.passwort(&name, falsch).await {
                    Some(k) => {
                        pw = Some(k);
                        falsch = true;
                    }
                    None => return None,
                },
                Err(entpacken::Fehler::Abgebrochen) => {
                    if diagnose() {
                        eprintln!(
                            "querencia-archiv: Entpacken abgebrochen nach {} ms",
                            beginn.elapsed().as_millis()
                        );
                    }
                    return None;
                }
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
                    self.dialoge
                        .fehler(
                            &gettext("An error occurred while extracting files."),
                            &detail,
                        )
                        .await;
                    return None;
                }
            }
        }
    }

    // ------------------------------------------------------- Vorschau

    /// Entpackt eine Datei in einen eigenen Vorschauordner und öffnet sie
    /// mit dem Standardprogramm. Die Kopie ist schreibgeschützt; Änderungen
    /// daran kommen nicht ins Archiv zurück.
    fn vorschau(self: &Rc<Self>, index: usize) {
        if self.beschaeftigt.get() {
            return;
        }
        let f = self.clone();
        glib::spawn_future_local(async move {
            let (auswahl, name) = {
                let offen = f.offen.borrow();
                let Some(o) = offen.as_ref() else { return };
                if !o.existiert {
                    return;
                }
                let Some(e) = o.archiv.inhalt.eintraege.get(index) else {
                    return;
                };
                let Some(p) = e.pfad() else { return };
                let name = p.name().to_string();
                let Some(rel) = archiv::pfade::RelPfad::einzeln(&name) else {
                    return;
                };
                let mut a = Auswahl::default();
                a.ziele.insert(index, rel);
                a.gesamt = e.groesse;
                (a, name)
            };
            let Some(ordner) = crate::vorschau::neuer_ordner() else {
                f.dialoge
                    .fehler(
                        &gettext("Could not display the file."),
                        &gettext("No temporary folder."),
                    )
                    .await;
                return;
            };
            if f.entpacken(auswahl, ordner.clone()).await.is_none() {
                return;
            }
            let datei = ordner.join(&name);
            crate::vorschau::schreibschutz(&datei);
            let kontext = gtk::prelude::WidgetExt::display(&f.window).app_launch_context();
            let uri = gio::File::for_path(&datei).uri();
            if let Err(e) = gio::AppInfo::launch_default_for_uri(&uri, Some(&kontext)) {
                f.dialoge
                    .fehler(
                        &gettext_f("Could not display “{name}”.", &[("name", &name)]),
                        &gettext_f("Details: {detail}", &[("detail", e.message())]),
                    )
                    .await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::CSS;

    #[test]
    fn css_ohne_feste_farben_und_ohne_media() {
        // QD-081: Grund, Schrift und Linien kommen aus dem Thema
        // (currentColor), damit „Dunkle Darstellung bevorzugen“ alles
        // mitnimmt. Eine @media-Regel bräuchte die Bindung des Providers an
        // gtk-interface-color-scheme (gemessen in Bilder, Dokumente, Software).
        let ohne_kommentare: String = CSS
            .split("/*")
            .map(|t| t.split_once("*/").map_or(t, |(_, r)| r))
            .collect();
        for muster in ["#", "rgb(", "rgba(", "@media"] {
            assert!(
                !ohne_kommentare.contains(muster),
                "feste Farbe oder @media in style.css: {muster}"
            );
        }
        assert_eq!(CSS.matches("alpha(currentColor, 0.18)").count(), 3);
    }
}
