//! Hintergrundarbeit mit Fortschrittsfenster.
//!
//! [`laufen`] führt die Arbeit in einem eigenen Faden aus und wartet
//! (`async`) auf ihr Ergebnis, ohne die GTK-Hauptschleife anzuhalten. Der
//! Faden meldet Fortschritt über einen [`Bote`]n; eine Rückfrage bei
//! vorhandenen Dateien wartet im Faden auf die Antwort aus dem
//! Konfliktfenster („für alle“ merkt sich die Antwort für den Rest des
//! Laufs). Das Fortschrittsfenster erscheint erst nach 400 ms, damit kurze
//! Arbeiten nicht flackern; „Abbrechen“ setzt das Abbruchkennzeichen.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;
use gtk::glib;

use archiv::entpacken::{Konflikt, Rueckmeldung};
use archiv::lesen::Stand;
use archiv::pfade::RelPfad;
use archiv::schreiben;

use crate::dialoge::Dialoge;

enum Nachricht<T> {
    Fortschritt(u64, u64, String),
    Konflikt(PathBuf, mpsc::Sender<Konflikt>),
    Fertig(T),
}

/// Verbindung des Arbeitsfadens zur Oberfläche.
pub struct Bote<T> {
    tx: UnboundedSender<Nachricht<T>>,
    zuletzt: Option<Instant>,
}

impl<T> Bote<T> {
    fn senden(&mut self, fertig: u64, gesamt: u64, name: &str) {
        // höchstens 20 Meldungen je Sekunde, Namenswechsel immer
        let jetzt = Instant::now();
        if name.is_empty()
            && fertig < gesamt
            && self
                .zuletzt
                .is_some_and(|z| jetzt - z < Duration::from_millis(50))
        {
            return;
        }
        self.zuletzt = Some(jetzt);
        let _ = self
            .tx
            .unbounded_send(Nachricht::Fortschritt(fertig, gesamt, name.to_string()));
    }
}

impl<T> Rueckmeldung for Bote<T> {
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, name: &str) {
        self.senden(fertig, gesamt, name);
    }

    fn konflikt(&mut self, ziel: &Path, _neu: &RelPfad) -> Konflikt {
        let (tx, rx) = mpsc::channel();
        if self
            .tx
            .unbounded_send(Nachricht::Konflikt(ziel.into(), tx))
            .is_err()
        {
            return Konflikt::Abbrechen;
        }
        rx.recv().unwrap_or(Konflikt::Abbrechen)
    }
}

impl<T> schreiben::Fortschritt for Bote<T> {
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, name: &str) {
        self.senden(fertig, gesamt, name);
    }
}

/// Was das Fortschrittsfenster zeigt.
pub struct Anzeige<'a> {
    pub fenstertitel: &'a str,
    pub titel: &'a str,
    /// Datei, deren Lesestand (`Stand::gelesen`) den Balken treibt, solange
    /// die Arbeit selbst keinen Fortschritt meldet (Auflisten).
    pub lesestand_von: Option<&'a Path>,
}

/// Führt `arbeit` im Hintergrund aus und liefert ihr Ergebnis (`None`, wenn
/// der Arbeitsfaden ohne Ergebnis endete, also abgestürzt ist).
pub async fn laufen<T: Send + 'static>(
    dialoge: &Dialoge,
    anzeige: Anzeige<'_>,
    stand: &Stand,
    arbeit: impl FnOnce(&mut Bote<T>) -> T + Send + 'static,
) -> Option<T> {
    let (tx, mut rx) = unbounded::<Nachricht<T>>();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        let mut bote = Bote { tx, zuletzt: None };
        let t = arbeit(&mut bote);
        let _ = tx2.unbounded_send(Nachricht::Fertig(t));
    });
    let f = dialoge.fortschritt();
    f.beginnen(anzeige.fenstertitel, anzeige.titel, stand);
    let fertig = Rc::new(Cell::new(false));
    let gemeldet = Rc::new(Cell::new(false));
    // Während einer Rückfrage bleibt das Fortschrittsfenster verborgen: es
    // läge sonst über dem Konfliktfenster und nähme ihm den Fokus.
    let frage_offen = Rc::new(Cell::new(false));
    let sichtbar = Rc::new(Cell::new(false));
    // verzögert zeigen
    {
        let (f, fertig, frage_offen, sichtbar) = (
            f.clone(),
            fertig.clone(),
            frage_offen.clone(),
            sichtbar.clone(),
        );
        glib::timeout_add_local_once(Duration::from_millis(400), move || {
            if !fertig.get() {
                sichtbar.set(true);
                if !frage_offen.get() {
                    f.zeigen();
                }
            }
        });
    }
    // Lesestand der Archivdatei, bis die Arbeit selbst meldet
    if let Some(datei) = anzeige.lesestand_von {
        let gesamt = std::fs::metadata(datei).map(|m| m.len()).unwrap_or(0);
        let (f, fertig, gemeldet, gelesen) = (
            f.clone(),
            fertig.clone(),
            gemeldet.clone(),
            stand.gelesen.clone(),
        );
        glib::timeout_add_local(Duration::from_millis(150), move || {
            if fertig.get() {
                return glib::ControlFlow::Break;
            }
            if !gemeldet.get() {
                f.setzen(gelesen.load(Ordering::Relaxed).min(gesamt), gesamt, "");
            }
            glib::ControlFlow::Continue
        });
    }
    let mut fuer_alle: Option<Konflikt> = None;
    let mut ergebnis = None;
    while let Some(n) = rx.next().await {
        match n {
            Nachricht::Fortschritt(a, b, name) => {
                gemeldet.set(true);
                f.setzen(a, b, &name);
            }
            Nachricht::Konflikt(ziel, antwort) => {
                let k = match fuer_alle {
                    Some(k) => k,
                    None => {
                        frage_offen.set(true);
                        f.verbergen();
                        let (k, alle) = dialoge.konflikt(&ziel).await;
                        frage_offen.set(false);
                        if sichtbar.get() && k != Konflikt::Abbrechen {
                            f.zeigen();
                        }
                        if alle && k != Konflikt::Abbrechen {
                            fuer_alle = Some(k);
                        }
                        k
                    }
                };
                let _ = antwort.send(k);
            }
            Nachricht::Fertig(t) => {
                ergebnis = Some(t);
                break;
            }
        }
    }
    fertig.set(true);
    f.ende();
    ergebnis
}
