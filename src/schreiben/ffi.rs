//! Schmale Anbindung an den Schreibteil von libarchive (3.8, `archive.h`,
//! `archive_entry.h`). compress-tools bindet nur das Lesen an; die wenigen
//! Funktionen zum Schreiben stehen hier, mit Typen wie in den Kopfdateien
//! für Linux (`la_int64_t` = i64, `la_ssize_t` = isize, `mode_t` = u32,
//! `time_t` = i64). Einziges Modul mit `unsafe`; nach außen gibt es nur den
//! Typ [`Schreiber`], der beide Zeiger besitzt und in `Drop` genau einmal
//! freigibt.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_long, c_uint, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[repr(C)]
struct Archive {
    _p: [u8; 0],
}
#[repr(C)]
struct Entry {
    _p: [u8; 0],
}

const ARCHIVE_OK: c_int = 0;
const ARCHIVE_WARN: c_int = -20;
pub const AE_IFREG: c_uint = 0o100000;
pub const AE_IFDIR: c_uint = 0o040000;
pub const AE_IFLNK: c_uint = 0o120000;

#[link(name = "archive")]
unsafe extern "C" {
    fn archive_write_new() -> *mut Archive;
    fn archive_write_free(a: *mut Archive) -> c_int;
    fn archive_write_close(a: *mut Archive) -> c_int;
    fn archive_write_set_format_zip(a: *mut Archive) -> c_int;
    fn archive_write_set_format_pax_restricted(a: *mut Archive) -> c_int;
    fn archive_write_set_format_7zip(a: *mut Archive) -> c_int;
    fn archive_write_set_format_raw(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_none(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_gzip(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_xz(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_bzip2(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_zstd(a: *mut Archive) -> c_int;
    fn archive_write_set_options(a: *mut Archive, opts: *const c_char) -> c_int;
    fn archive_write_set_passphrase(a: *mut Archive, p: *const c_char) -> c_int;
    fn archive_write_open_filename(a: *mut Archive, file: *const c_char) -> c_int;
    fn archive_write_header(a: *mut Archive, e: *mut Entry) -> c_int;
    fn archive_write_data(a: *mut Archive, buf: *const c_void, len: usize) -> isize;
    fn archive_write_finish_entry(a: *mut Archive) -> c_int;
    fn archive_error_string(a: *mut Archive) -> *const c_char;

    fn archive_entry_new() -> *mut Entry;
    fn archive_entry_free(e: *mut Entry);
    fn archive_entry_set_pathname_utf8(e: *mut Entry, name: *const c_char);
    fn archive_entry_set_size(e: *mut Entry, s: i64);
    fn archive_entry_set_filetype(e: *mut Entry, t: c_uint);
    fn archive_entry_set_perm(e: *mut Entry, p: u32);
    fn archive_entry_set_mtime(e: *mut Entry, sec: i64, nsec: c_long);
    fn archive_entry_set_symlink_utf8(e: *mut Entry, ziel: *const c_char);
}

/// Container-Auswahl auf C-Ebene.
#[derive(Clone, Copy, Debug)]
pub enum Container {
    Zip,
    Pax,
    SiebenZip,
    /// Ein einzelner Strom ohne Container (nur das Testwerkzeug: `.gz`).
    Roh,
}

/// Filter auf C-Ebene.
#[derive(Clone, Copy, Debug)]
pub enum Filter {
    Ohne,
    Gzip,
    Xz,
    Bzip2,
    Zstd,
}

fn cstring(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| format!("NUL-Zeichen in „{s}“"))
}

/// Ein offenes Archiv zum Schreiben. Wird es ohne [`Schreiber::schliessen`]
/// fallen gelassen, gibt `Drop` libarchive frei (die Datei bleibt dann
/// unvollständig; der Aufrufer schreibt immer in einen Hilfsnamen).
pub struct Schreiber {
    a: *mut Archive,
    e: *mut Entry,
    // nach `a` freigegeben (Felder fallen nach Drop::drop)
    _locale: Option<Utf8Locale>,
}

/// Fadenlokale Locale C.UTF-8 für die Lebensdauer des Schreibers: libarchive
/// rechnet Namen über die Locale um, und ein Programm ohne `setlocale`
/// (Tests) stünde sonst auf „C“ (ASCII). Wie `UTF8LocaleGuard` in
/// compress-tools beim Lesen. Der Schreiber ist deshalb nicht `Send`.
struct Utf8Locale {
    alt: libc::locale_t,
    neu: libc::locale_t,
}

impl Utf8Locale {
    fn setzen() -> Option<Utf8Locale> {
        // SAFETY: newlocale mit gültiger, statischer Zeichenkette; NULL wird
        // geprüft. uselocale wirkt nur auf diesen Faden.
        unsafe {
            let neu = libc::newlocale(
                libc::LC_CTYPE_MASK,
                c"C.UTF-8".as_ptr(),
                std::ptr::null_mut(),
            );
            if neu.is_null() {
                return None;
            }
            let alt = libc::uselocale(neu);
            Some(Utf8Locale { alt, neu })
        }
    }
}

impl Drop for Utf8Locale {
    fn drop(&mut self) {
        // SAFETY: alt stammt aus uselocale, neu aus newlocale (genau einmal).
        unsafe {
            libc::uselocale(self.alt);
            libc::freelocale(self.neu);
        }
    }
}

impl Schreiber {
    fn fehler(&self, was: &str) -> String {
        // SAFETY: self.a ist gültig, solange self lebt; die Zeichenkette
        // gehört libarchive und wird sofort kopiert.
        let m = unsafe {
            let p = archive_error_string(self.a);
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        };
        if m.is_empty() {
            was.to_string()
        } else {
            format!("{was}: {m}")
        }
    }

    fn pruefe(&self, rc: c_int, was: &str) -> Result<(), String> {
        if rc == ARCHIVE_OK {
            Ok(())
        } else {
            Err(self.fehler(was))
        }
    }

    /// Legt `pfad` neu an (eine vorhandene Datei wird überschrieben).
    /// `optionen` wie bei `archive_write_set_options` („zip:encryption=…“).
    pub fn neu(
        pfad: &Path,
        container: Container,
        filter: Filter,
        optionen: &[&str],
        passwort: Option<&str>,
    ) -> Result<Schreiber, String> {
        // SAFETY: Rückgabe wird auf NULL geprüft; beide Zeiger gehören ab hier
        // dem Schreiber und werden in Drop freigegeben.
        let (a, e) = unsafe { (archive_write_new(), archive_entry_new()) };
        if a.is_null() || e.is_null() {
            // SAFETY: nur Nicht-NULL freigeben
            unsafe {
                if !a.is_null() {
                    archive_write_free(a);
                }
                if !e.is_null() {
                    archive_entry_free(e);
                }
            }
            return Err("libarchive: kein Speicher".into());
        }
        let s = Schreiber {
            a,
            e,
            _locale: Utf8Locale::setzen(),
        };
        // SAFETY: s.a ist gültig.
        let rc = unsafe {
            match container {
                Container::Zip => archive_write_set_format_zip(s.a),
                Container::Pax => archive_write_set_format_pax_restricted(s.a),
                Container::SiebenZip => archive_write_set_format_7zip(s.a),
                Container::Roh => archive_write_set_format_raw(s.a),
            }
        };
        s.pruefe(rc, "Format")?;
        // SAFETY: s.a ist gültig.
        let rc = unsafe {
            match filter {
                Filter::Ohne => archive_write_add_filter_none(s.a),
                Filter::Gzip => archive_write_add_filter_gzip(s.a),
                Filter::Xz => archive_write_add_filter_xz(s.a),
                Filter::Bzip2 => archive_write_add_filter_bzip2(s.a),
                Filter::Zstd => archive_write_add_filter_zstd(s.a),
            }
        };
        s.pruefe(rc, "Filter")?;
        for o in optionen {
            let c = cstring(o)?;
            // SAFETY: c lebt bis nach dem Aufruf; libarchive kopiert.
            let rc = unsafe { archive_write_set_options(s.a, c.as_ptr()) };
            s.pruefe(rc, o)?;
        }
        if let Some(pw) = passwort {
            let c = cstring(pw)?;
            // SAFETY: wie oben.
            let rc = unsafe { archive_write_set_passphrase(s.a, c.as_ptr()) };
            s.pruefe(rc, "Kennwort")?;
        }
        let c = CString::new(pfad.as_os_str().as_bytes()).map_err(|_| "NUL im Pfad".to_string())?;
        // SAFETY: wie oben.
        let rc = unsafe { archive_write_open_filename(s.a, c.as_ptr()) };
        s.pruefe(rc, "Öffnen")?;
        Ok(s)
    }

    /// Einzelne komprimierte Datei (genau ein Eintrag).
    pub fn neu_roh(pfad: &Path, filter: Filter) -> Result<Schreiber, String> {
        Schreiber::neu(pfad, Container::Roh, filter, &[], None)
    }

    /// Kopf eines Eintrags; danach bei Dateien genau `groesse` Bytes über
    /// [`Schreiber::daten`], dann [`Schreiber::ende`].
    pub fn kopf(
        &mut self,
        name: &str,
        typ: c_uint,
        rechte: u32,
        groesse: u64,
        geaendert: i64,
    ) -> Result<(), String> {
        let c = cstring(name)?;
        // SAFETY: self.e ist gültig; archive_entry_clear ist nicht nötig,
        // weil jeder Aufruf alle benutzten Felder neu setzt.
        let rc = unsafe {
            archive_entry_set_pathname_utf8(self.e, c.as_ptr());
            archive_entry_set_filetype(self.e, typ);
            archive_entry_set_perm(self.e, rechte & 0o7777);
            archive_entry_set_size(self.e, if typ == AE_IFREG { groesse as i64 } else { 0 });
            archive_entry_set_mtime(self.e, geaendert, 0);
            archive_write_header(self.a, self.e)
        };
        // ARCHIVE_WARN (Namen, die das Format nicht ganz fasst) zählt hier als
        // Fehler: Querencia Archiv soll nichts stillschweigend verändern.
        if rc == ARCHIVE_WARN {
            return Err(self.fehler(name));
        }
        self.pruefe(rc, name)
    }

    /// Symbolische Verknüpfung (nur das Testwerkzeug legt solche an).
    pub fn verknuepfung(&mut self, name: &str, ziel: &str) -> Result<(), String> {
        let c = cstring(ziel)?;
        // SAFETY: self.e ist gültig; c lebt bis nach dem Aufruf von kopf,
        // der den Eintrag schreibt (libarchive kopiert die Zeichenkette).
        unsafe { archive_entry_set_symlink_utf8(self.e, c.as_ptr()) };
        let r = self.kopf(name, AE_IFLNK, 0o777, 0, 0);
        // SAFETY: wie oben; zurücksetzen für die nächsten Einträge
        unsafe { archive_entry_set_symlink_utf8(self.e, std::ptr::null()) };
        r?;
        self.ende()
    }

    pub fn daten(&mut self, mut buf: &[u8]) -> Result<(), String> {
        while !buf.is_empty() {
            // SAFETY: buf ist gültig für buf.len() Bytes.
            let n = unsafe { archive_write_data(self.a, buf.as_ptr().cast(), buf.len()) };
            if n <= 0 {
                return Err(self.fehler("Schreiben"));
            }
            buf = &buf[n as usize..];
        }
        Ok(())
    }

    pub fn ende(&mut self) -> Result<(), String> {
        // SAFETY: self.a ist gültig.
        let rc = unsafe { archive_write_finish_entry(self.a) };
        self.pruefe(rc, "Eintrag")
    }

    /// Schreibt den Schluss des Archivs; erst danach ist die Datei gültig.
    pub fn schliessen(self) -> Result<(), String> {
        // SAFETY: self.a ist gültig; Drop gibt danach frei.
        let rc = unsafe { archive_write_close(self.a) };
        self.pruefe(rc, "Abschließen")
    }
}

impl Drop for Schreiber {
    fn drop(&mut self) {
        // SAFETY: a und e stammen aus archive_write_new/archive_entry_new
        // und werden genau einmal freigegeben. archive_write_free schließt
        // ein noch offenes Archiv selbst.
        unsafe {
            archive_entry_free(self.e);
            archive_write_free(self.a);
        }
    }
}
