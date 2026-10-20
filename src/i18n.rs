//! Übersetzungen über GNU gettext (Textdomäne `querencia-archiv`).
//!
//! Quellsprache im Code ist Englisch (msgid englisch, wie bei GNOME und
//! Debian); Deutsch kommt aus `po/de.po`. Ohne passenden Katalog zeigt das
//! Programm die englischen Texte. Die Sprache wählt gettext wie üblich über
//! `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` und `LANG`.
//!
//! Die Kataloge (`<lang>/LC_MESSAGES/querencia-archiv.mo`) sucht das Programm
//! zuerst neben der Binary (`<ablage>/share/locale`, unveränderliche Ablage
//! unter `~/opt/querencia/archiv/<sha16>/`), dann im Präfix der Binary
//! (`/usr/bin/../share/locale`), zuletzt in `/usr/share/locale`.
//! `QUERENCIA_ARCHIV_LOCALEDIR` setzt das Verzeichnis ausdrücklich (Tests).
//!
//! Texte mit Platzhaltern stehen als ein Satz im Katalog („Could not open “{name}”.“),
//! nie zusammengesetzt; [`gettext_f`] und [`ngettext_f`] setzen die Werte erst
//! nach der Übersetzung ein. Gleiche englische Wörter mit verschiedenen
//! Zugriffstasten im Deutschen („_Schließen“ im Dialog, „S_chließen“ im Menü)
//! trennt ein Kontext (`pgettext`).

use std::path::{Path, PathBuf};

use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain};
pub use gettextrs::{gettext, ngettext, pgettext};

/// Textdomäne = Name der .mo-Dateien.
pub const DOMAIN: &str = "querencia-archiv";

/// Textdomäne binden; vor `Application::run` aufrufen. Die Locale aus der
/// Umgebung setzt GTK selbst beim Start (`gtk_init` ruft `setlocale(LC_ALL,
/// "")`); alle Texte entstehen erst danach (startup/activate). Ein eigenes
/// `setlocale` wäre in gettext-rs 0.8 `unsafe` und ist hier verboten
/// (`unsafe_code = "forbid"`).
pub fn init() {
    let dir = locale_dir();
    if let Err(err) = bindtextdomain(DOMAIN, dir.as_path()) {
        eprintln!(
            "querencia-archiv: Übersetzungen nicht gebunden ({}): {err}",
            dir.display()
        );
    }
    let _ = bind_textdomain_codeset(DOMAIN, "UTF-8");
    let _ = textdomain(DOMAIN);
}

/// Verzeichnis der Kataloge (siehe Modulbeschreibung).
pub fn locale_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("QUERENCIA_ARCHIV_LOCALEDIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    // current_exe() löst den Link ~/opt/querencia/bin/querencia-archiv auf.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    locale_dir_for(exe_dir.as_deref())
}

fn locale_dir_for(exe_dir: Option<&Path>) -> PathBuf {
    if let Some(exe_dir) = exe_dir {
        let beside = exe_dir.join("share/locale");
        if beside.is_dir() {
            return beside;
        }
        if let Some(prefix) = exe_dir.parent() {
            let prefixed = prefix.join("share/locale");
            if prefixed.is_dir() {
                return prefixed;
            }
        }
    }
    PathBuf::from("/usr/share/locale")
}

/// Dezimalzeichen der Oberflächensprache („,“ im Deutschen) aus dem Katalog.
/// Bewusst nicht aus `LC_NUMERIC`: Die Sitzung läuft mit `LANG=en_US.UTF-8`
/// und `LANGUAGE=de`, das Zahlenformat folgte sonst nicht der Oberfläche.
pub fn decimal_separator() -> String {
    // TRANSLATORS: Dezimalzeichen in Dateigrößen („2.9 MB“)
    pgettext("decimal separator", ".")
}

/// `value` mit `digits` Nachkommastellen und dem Dezimalzeichen `sep`.
pub fn decimal(value: f64, digits: usize, sep: &str) -> String {
    format!("{value:.digits$}").replace('.', sep)
}

/// Übersetzt `msgid` und setzt danach benannte Platzhalter `{name}` ein.
pub fn gettext_f(msgid: &str, args: &[(&str, &str)]) -> String {
    fill(&gettext(msgid), args)
}

/// Wie [`gettext_f`] mit Pluralform für `n`; `{n}` steht automatisch bereit.
pub fn ngettext_f(msgid: &str, msgid_plural: &str, n: u32, args: &[(&str, &str)]) -> String {
    let count = n.to_string();
    let mut all = vec![("n", count.as_str())];
    all.extend_from_slice(args);
    fill(&ngettext(msgid, msgid_plural, n), &all)
}

/// Ersetzt `{schlüssel}` in einem Durchgang; eingesetzte Werte werden nicht
/// erneut durchsucht, unbekannte Platzhalter bleiben stehen.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let key = &after[..close];
            args.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v, close))
        });
        match value {
            Some((v, close)) => {
                out.push_str(v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_once() {
        assert_eq!(
            fill("Saved “{name}”.", &[("name", "a.txt")]),
            "Saved “a.txt”."
        );
        // Ein Wert mit Klammern wird nicht noch einmal ersetzt.
        assert_eq!(
            fill("{a} und {b}", &[("a", "{b}"), ("b", "x")]),
            "{b} und x"
        );
        assert_eq!(fill("{unbekannt} {", &[("a", "x")]), "{unbekannt} {");
    }

    #[test]
    fn decimals_use_the_given_separator() {
        assert_eq!(decimal(12.5, 1, ","), "12,5");
        assert_eq!(decimal(2.94, 1, "."), "2.9");
        assert_eq!(decimal(100.0, 0, ","), "100");
        // ohne Katalog: englischer Punkt
        assert_eq!(decimal_separator(), ".");
    }

    #[test]
    fn untranslated_text_falls_back_to_the_english_msgid() {
        // Ohne gebundenen Katalog liefert gettext die msgid.
        assert_eq!(ngettext_f("{n} byte", "{n} bytes", 3, &[]), "3 bytes");
        assert_eq!(ngettext_f("{n} byte", "{n} bytes", 1, &[]), "1 byte");
    }

    /// Deutsch hängt an `po/de.po`: jede msgid der Vorlage muss dort
    /// übersetzt sein (sonst sähe die deutsche Sitzung Englisch).
    #[test]
    fn german_catalog_is_complete() {
        let pot = include_str!("../po/querencia-archiv.pot");
        let de = include_str!("../po/de.po");
        assert!(!de.contains("#, fuzzy"), "de.po enthält fuzzy-Einträge");
        let ids = |text: &str| -> Vec<String> {
            text.split("\n\n")
                .filter_map(|block| {
                    let id: Vec<&str> = block
                        .lines()
                        .skip_while(|l| !l.starts_with("msgid "))
                        .take_while(|l| !l.starts_with("msgstr") && !l.starts_with("msgid_plural"))
                        .collect();
                    let ctxt = block.lines().find(|l| l.starts_with("msgctxt "));
                    (!id.is_empty()).then(|| format!("{ctxt:?}{}", id.join("")))
                })
                .collect()
        };
        let pot_ids = ids(pot);
        let de_ids = ids(de);
        assert!(pot_ids.len() > 50, "Vorlage zu klein: {}", pot_ids.len());
        for id in &pot_ids {
            assert!(de_ids.contains(id), "fehlt in de.po: {id}");
        }
        for block in de.split("\n\n").skip(1) {
            let empty = block
                .lines()
                .any(|l| l.starts_with("msgstr") && l.ends_with(" \"\""))
                && !block.lines().any(|l| l.starts_with('"'));
            assert!(!empty, "unübersetzt in de.po:\n{block}");
        }
    }

    #[test]
    fn catalogs_are_searched_beside_the_binary_then_in_the_prefix() {
        let tmp = std::env::temp_dir().join(format!("qa-i18n-{}", std::process::id()));
        let ablage = tmp.join("ablage");
        std::fs::create_dir_all(ablage.join("share/locale")).unwrap();
        assert_eq!(locale_dir_for(Some(&ablage)), ablage.join("share/locale"));
        let bin = tmp.join("usr/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(tmp.join("usr/share/locale")).unwrap();
        assert_eq!(locale_dir_for(Some(&bin)), tmp.join("usr/share/locale"));
        let leer = tmp.join("leer/bin");
        std::fs::create_dir_all(&leer).unwrap();
        assert_eq!(
            locale_dir_for(Some(&leer)),
            PathBuf::from("/usr/share/locale")
        );
        assert_eq!(locale_dir_for(None), PathBuf::from("/usr/share/locale"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
