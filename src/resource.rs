//! Eingebettete Ressourcen (GResource): das Programmsymbol als SVG für das
//! Info-Fenster. Quelle ist dasselbe SVG wie das installierte Programmsymbol
//! (`data/icons/hicolor/scalable/apps/org.querencia.Archiv.svg`), eingebettet
//! unter eigenem Namen, damit es nie mit dem PNG-Satz der App-ID im
//! Symbolthema zusammentrifft (GTK-4.22-Symbolabsturz, siehe Bericht G2-V).

use gtk::{gdk, gio, glib};

/// Name des Symbols im Symbolthema (nur aus der Ressource, nur SVG).
pub const INFO_ICON: &str = "org.querencia.Archiv-info";
const ICON_PATH: &str = "/org/querencia/Archiv/icons";
const DATA: &[u8] = include_bytes!("../data/resource/archiv.gresource");

fn resource() -> Result<gio::Resource, glib::Error> {
    gio::Resource::from_data(&glib::Bytes::from_static(DATA))
}

/// Registriert die Ressource und macht ihre Symbole dem Symbolthema bekannt;
/// läuft nach dem Start von GTK.
pub fn register() {
    match resource() {
        Ok(r) => {
            gio::resources_register(&r);
            if let Some(display) = gdk::Display::default() {
                gtk::IconTheme::for_display(&display).add_resource_path(ICON_PATH);
            }
        }
        Err(err) => eprintln!("querencia-archiv: Ressourcen nicht lesbar: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_icon_is_the_program_svg() {
        let r = resource().expect("Ressource");
        let path = format!("{ICON_PATH}/scalable/apps/{INFO_ICON}.svg");
        let data = r
            .lookup_data(&path, gio::ResourceLookupFlags::NONE)
            .expect("Symbol in der Ressource");
        let svg = include_bytes!("../data/icons/hicolor/scalable/apps/org.querencia.Archiv.svg");
        assert_eq!(
            &data[..],
            &svg[..],
            "Ressource veraltet: neu erzeugen (data/resource/archiv.gresource.xml)"
        );
    }
}
