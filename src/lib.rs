//! Querencia Archiv: Kern ohne Oberfläche – Formaterkennung, Pfadprüfung
//! (kein `..`, keine absoluten Pfade, keinen Verknüpfungen folgen),
//! Archivinhalt mit Ordnerbaum, Lesen und Entpacken über libarchive
//! (compress-tools), Schreiben über eine eigene schmale libarchive-Anbindung.
//! Die Oberfläche (main.rs und ihre Module) baut darauf auf; das
//! Testwerkzeug `examples/testarchive.rs` ebenso.

pub mod anzeige;
pub mod entpacken;
pub mod format;
pub mod i18n;
pub mod lesen;
pub mod modell;
pub mod pfade;
pub mod schreiben;

pub const APP_ID: &str = "org.querencia.Archiv";
pub const APP_NAME: &str = "Querencia Archiv";
