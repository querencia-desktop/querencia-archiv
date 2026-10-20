//! Zeilen als GObjects: [`Zeile`] für die Dateiliste (`GtkColumnView`, die
//! Spaltensortierer lesen die Eigenschaften), [`OrdnerZeile`] für den
//! Ordnerbaum (`GtkTreeListModel`).

use std::cell::{Cell, RefCell};

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;

use archiv::i18n::gettext;
use archiv::modell::{Art, Eintrag};

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Zeile)]
    pub struct Zeile {
        #[property(get, set)]
        pub name: RefCell<String>,
        /// Ordner: Pfad im Archiv; Datei: leer
        #[property(get, set)]
        pub ordnerpfad: RefCell<String>,
        /// Datei: Index im Archiv; Ordner: Index des Ordnereintrags oder -1
        #[property(get, set)]
        pub index: Cell<i64>,
        #[property(get, set)]
        pub groesse: Cell<u64>,
        #[property(get, set)]
        pub typ: RefCell<String>,
        #[property(get, set)]
        pub inhaltstyp: RefCell<String>,
        #[property(get, set)]
        pub geaendert: Cell<i64>,
        #[property(get, set)]
        pub ist_ordner: Cell<bool>,
        #[property(get, set)]
        pub unsicher: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Zeile {
        const NAME: &'static str = "QuerenciaArchivZeile";
        type Type = super::Zeile;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Zeile {}
}

mod imp_ordner {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::OrdnerZeile)]
    pub struct OrdnerZeile {
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub pfad: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OrdnerZeile {
        const NAME: &'static str = "QuerenciaArchivOrdnerZeile";
        type Type = super::OrdnerZeile;
    }

    #[glib::derived_properties]
    impl ObjectImpl for OrdnerZeile {}
}

glib::wrapper! {
    pub struct Zeile(ObjectSubclass<imp::Zeile>);
}

glib::wrapper! {
    pub struct OrdnerZeile(ObjectSubclass<imp_ordner::OrdnerZeile>);
}

impl Zeile {
    /// Ein Unterordner (`pfad` im Archiv, `eintrag`: eigener Ordnereintrag).
    pub fn ordner(
        name: &str,
        pfad: &str,
        eintrag: Option<&Eintrag>,
        index: Option<usize>,
    ) -> Zeile {
        glib::Object::builder()
            .property("name", name)
            .property("ordnerpfad", pfad)
            .property("index", index.map(|i| i as i64).unwrap_or(-1))
            .property("groesse", 0u64)
            .property("typ", gettext("Folder"))
            .property("inhaltstyp", "inode/directory")
            .property("geaendert", eintrag.map(|e| e.geaendert).unwrap_or(0))
            .property("ist-ordner", true)
            .property("unsicher", false)
            .build()
    }

    /// Eine Datei (auch Verknüpfung, Sonstiges) oder ein unsicherer Eintrag.
    pub fn datei(e: &Eintrag, index: usize) -> Zeile {
        let unsicher = e.unsicher().is_some();
        let name = match e.pfad() {
            Some(p) => p.name().to_string(),
            None => e.roh.clone(),
        };
        let (inhaltstyp, typ) = if unsicher {
            // TRANSLATORS: Spalte „Typ“ für Einträge mit ../ oder absolutem Pfad
            (
                "application/octet-stream".to_string(),
                gettext("Unsafe path – not extracted"),
            )
        } else {
            match e.art {
                Art::Verknuepfung => (
                    "inode/symlink".to_string(),
                    // TRANSLATORS: Spalte „Typ“; Querencia Archiv legt Verknüpfungen nicht an
                    gettext("Link – not extracted"),
                ),
                Art::Sonstiges => (
                    "application/octet-stream".to_string(),
                    // TRANSLATORS: Spalte „Typ“ für Geräte- und Sonderdateien
                    gettext("Special file – not extracted"),
                ),
                _ => {
                    let (ct, _) = gio::content_type_guess(Some(name.as_str()), None);
                    let typ = gio::content_type_get_description(&ct).to_string();
                    (ct.to_string(), typ)
                }
            }
        };
        glib::Object::builder()
            .property("name", name)
            .property("ordnerpfad", "")
            .property("index", index as i64)
            .property("groesse", e.groesse)
            .property("typ", typ)
            .property("inhaltstyp", inhaltstyp)
            .property("geaendert", e.geaendert)
            .property("ist-ordner", false)
            .property("unsicher", unsicher)
            .build()
    }

    pub fn symbol(&self) -> gio::Icon {
        if self.unsicher() {
            return gio::ThemedIcon::new("dialog-warning").upcast();
        }
        if self.ist_ordner() {
            return gio::ThemedIcon::new("folder").upcast();
        }
        gio::content_type_get_icon(&self.inhaltstyp())
    }
}

impl OrdnerZeile {
    pub fn neu(name: &str, pfad: &str) -> OrdnerZeile {
        glib::Object::builder()
            .property("name", name)
            .property("pfad", pfad)
            .build()
    }
}
