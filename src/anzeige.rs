//! Anzeigeformate: Größen wie Nautilus 2 (Basis 1024, eine Nachkommastelle,
//! Dezimalzeichen aus dem Katalog) und Datum je Sprache.

use gtk::glib;

use crate::i18n::{decimal, decimal_separator, gettext, ngettext_f};

/// „0 bytes“, „1023 bytes“, „1.5 KB“, „250 MB“ (deutsch „1,5 KB“).
pub fn groesse(bytes: u64) -> String {
    groesse_mit(bytes, &decimal_separator())
}

pub fn groesse_mit(bytes: u64, sep: &str) -> String {
    const STUFEN: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        // TRANSLATORS: Dateigröße unter 1 KB
        return ngettext_f(
            "{n} byte",
            "{n} bytes",
            bytes.min(u32::MAX as u64) as u32,
            &[],
        );
    }
    let mut wert = bytes as f64 / 1024.0;
    let mut stufe = 0;
    while wert >= 1024.0 && stufe + 1 < STUFEN.len() {
        wert /= 1024.0;
        stufe += 1;
    }
    let ziffern = if wert >= 100.0 { 0 } else { 1 };
    format!("{} {}", decimal(wert, ziffern, sep), STUFEN[stufe])
}

/// Änderungszeit (Unix-Sekunden, Ortszeit); 0 oder unlesbar: leer.
pub fn datum(sekunden: i64) -> String {
    if sekunden <= 0 {
        return String::new();
    }
    glib::DateTime::from_unix_local(sekunden)
        .ok()
        // TRANSLATORS: Spalte „Date Modified“, Format von g_date_time_format
        .and_then(|z| z.format(&gettext("%Y-%m-%d %H:%M")).ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groessen() {
        assert_eq!(groesse_mit(0, "."), "0 bytes");
        assert_eq!(groesse_mit(1, "."), "1 byte");
        assert_eq!(groesse_mit(1023, "."), "1023 bytes");
        assert_eq!(groesse_mit(1536, ","), "1,5 KB");
        assert_eq!(groesse_mit(250 * 1024 * 1024, "."), "250 MB");
        assert_eq!(
            groesse_mit(3 * 1024 * 1024 * 1024 + 400 * 1024 * 1024, "."),
            "3.4 GB"
        );
        assert_eq!(groesse_mit(5 << 40, "."), "5.0 TB");
    }

    #[test]
    fn datumsformat() {
        assert_eq!(datum(0), "");
        assert_eq!(datum(-5), "");
        assert_eq!(datum(1_800_000_000).len(), "2027-01-15 08:00".len());
    }
}
