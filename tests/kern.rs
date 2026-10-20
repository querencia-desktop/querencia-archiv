//! Kern über das echte libarchive: erstellen, auflisten, entpacken (alles,
//! Auswahl, Konflikte, Abbruch, Kennwort), bösartige Archive, Neuschreiben.
//! Alle Archive entstehen im Test selbst (eigener Schreiber).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use archiv::entpacken::{self, Auftrag, Auswahl, Konflikt, Rueckmeldung};
use archiv::format::Format;
use archiv::lesen::{self, Fehler as LeseFehler, Stand};
use archiv::schreiben::{self, Sammlung, ffi};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qa-kern-{name}-{}", std::process::id()));
    if d.exists() {
        fs::remove_dir_all(&d).unwrap();
    }
    fs::create_dir_all(&d).unwrap();
    d
}

/// Quellbaum: projekt/a.txt, projekt/unter/b.bin, projekt/leer/, „Ä ö.txt“.
fn quelle(d: &Path) -> Vec<PathBuf> {
    let p = d.join("projekt");
    fs::create_dir_all(p.join("unter")).unwrap();
    fs::create_dir_all(p.join("leer")).unwrap();
    fs::write(p.join("a.txt"), b"Hallo Archiv\n").unwrap();
    let b: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    fs::write(p.join("unter/b.bin"), &b).unwrap();
    fs::write(d.join("Ä ö.txt"), "Umlaute äöü ß\n").unwrap();
    vec![p, d.join("Ä ö.txt")]
}

struct Still {
    antwort: Konflikt,
    konflikte: usize,
}

impl Rueckmeldung for Still {
    fn fortschritt(&mut self, fertig: u64, gesamt: u64, _: &str) {
        assert!(fertig <= gesamt, "{fertig} > {gesamt}");
    }
    fn konflikt(&mut self, _: &Path, _: &archiv::pfade::RelPfad) -> Konflikt {
        self.konflikte += 1;
        self.antwort
    }
}

fn still() -> Still {
    Still {
        antwort: Konflikt::Abbrechen,
        konflikte: 0,
    }
}

fn erstellen(ziel: &Path, format: Format, pfade: &[PathBuf]) {
    let s = Sammlung::von(pfade, "").unwrap();
    schreiben::erstellen(
        ziel,
        format,
        &s,
        None,
        &Stand::default(),
        &mut |_, _, _: &str| {},
    )
    .unwrap();
}

fn alles_entpacken(
    archiv: &Path,
    ziel: &Path,
    pw: Option<&str>,
    r: &mut Still,
) -> Result<entpacken::Ergebnis, entpacken::Fehler> {
    let a = lesen::auflisten(archiv, None, &Stand::default()).unwrap();
    let w = Auswahl::alles(&a.inhalt).unwrap();
    fs::create_dir_all(ziel).unwrap();
    entpacken::entpacken(
        &Auftrag {
            archiv,
            roh: a.roh,
            ziel,
            auswahl: &w,
            passwort: pw,
            stand: &Stand::default(),
        },
        &mut HashSet::new(),
        r,
    )
}

fn reste(d: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for e in fs::read_dir(d).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.contains(".qa-") {
            v.push(n);
        }
        if e.file_type().unwrap().is_dir() {
            v.extend(reste(&e.path()));
        }
    }
    v
}

#[test]
fn drei_formate_hin_und_zurueck() {
    let d = tmp("formate");
    let q = quelle(&d.join("q"));
    for f in [Format::Zip, Format::TarGz, Format::TarXz, Format::SiebenZip] {
        let a = d.join(format!("t{}", f.endung()));
        erstellen(&a, f, &q);
        let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
        let kopf = lesen::kopf(&a).unwrap();
        assert_eq!(Format::erkennen(&kopf, &a.to_string_lossy()), f, "{f:?}");
        assert_eq!(l.format, f);
        assert!(!l.roh);
        assert_eq!(l.inhalt.dateien, 3, "{f:?}");
        assert_eq!(
            l.inhalt.groesse,
            13 + 300_000 + "Umlaute äöü ß\n".len() as u64
        );
        assert!(
            l.inhalt.ordner("projekt/leer").is_some(),
            "{f:?}: leerer Ordner"
        );
        assert!(l.inhalt.unsicher.is_empty());
        let wurzel = l.inhalt.ordner("").unwrap();
        let namen: Vec<_> = wurzel
            .dateien
            .iter()
            .map(|&i| l.inhalt.eintraege[i].pfad().unwrap().to_string())
            .collect();
        assert_eq!(namen, ["Ä ö.txt"], "{f:?}");
        let z = d.join(format!("aus-{}", f.kurzname()));
        let e = alles_entpacken(&a, &z, None, &mut still()).unwrap();
        assert_eq!(e.dateien, 3);
        assert_eq!(
            fs::read(z.join("projekt/a.txt")).unwrap(),
            b"Hallo Archiv\n"
        );
        assert_eq!(
            fs::read(z.join("projekt/unter/b.bin")).unwrap(),
            fs::read(d.join("q/projekt/unter/b.bin")).unwrap()
        );
        assert!(z.join("projekt/leer").is_dir(), "{f:?}");
        assert_eq!(
            fs::read_to_string(z.join("Ä ö.txt")).unwrap(),
            "Umlaute äöü ß\n"
        );
        assert!(reste(&z).is_empty() && reste(&d).is_empty());
    }
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn auswahl_ohne_den_angezeigten_ordner() {
    let d = tmp("auswahl");
    let q = quelle(&d.join("q"));
    let a = d.join("t.zip");
    erstellen(&a, Format::Zip, &q);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    // in der Liste steht man in „projekt“ und wählt „unter“ (Ordner ohne
    // eigenen Eintrag? zip von libarchive hat Ordnereinträge) und a.txt
    let a_txt = l.inhalt.ordner("projekt").unwrap().dateien[0];
    let w = Auswahl::ordner_aus(
        &l.inhalt,
        &[a_txt],
        &["projekt/unter".to_string()],
        "projekt",
    )
    .unwrap();
    let z = d.join("aus");
    fs::create_dir(&z).unwrap();
    let e = entpacken::entpacken(
        &Auftrag {
            archiv: &a,
            roh: false,
            ziel: &z,
            auswahl: &w,
            passwort: None,
            stand: &Stand::default(),
        },
        &mut HashSet::new(),
        &mut still(),
    )
    .unwrap();
    assert_eq!(e.dateien, 2);
    assert!(z.join("a.txt").is_file());
    assert!(z.join("unter/b.bin").is_file());
    assert!(!z.join("projekt").exists());
    assert_eq!(w.gesamt, 13 + 300_000);
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn konflikte_ueberspringen_und_ersetzen() {
    let d = tmp("konflikt");
    let q = quelle(&d.join("q"));
    let a = d.join("t.tar.gz");
    erstellen(&a, Format::TarGz, &q);
    let z = d.join("aus");
    alles_entpacken(&a, &z, None, &mut still()).unwrap();
    fs::write(z.join("projekt/a.txt"), b"geaendert").unwrap();
    let mut r = Still {
        antwort: Konflikt::Ueberspringen,
        konflikte: 0,
    };
    let e = alles_entpacken(&a, &z, None, &mut r).unwrap();
    assert_eq!((r.konflikte, e.uebersprungen, e.dateien), (3, 3, 0));
    assert_eq!(fs::read(z.join("projekt/a.txt")).unwrap(), b"geaendert");
    let mut r = Still {
        antwort: Konflikt::Ersetzen,
        konflikte: 0,
    };
    let e = alles_entpacken(&a, &z, None, &mut r).unwrap();
    assert_eq!((r.konflikte, e.dateien), (3, 3));
    assert_eq!(
        fs::read(z.join("projekt/a.txt")).unwrap(),
        b"Hallo Archiv\n"
    );
    // Abbrechen in der Rückfrage: nichts weiter geschrieben, keine Reste
    fs::write(z.join("projekt/a.txt"), b"x").unwrap();
    let r = alles_entpacken(&a, &z, None, &mut still());
    assert!(matches!(r, Err(entpacken::Fehler::Abgebrochen)));
    assert_eq!(fs::read(z.join("projekt/a.txt")).unwrap(), b"x");
    assert!(reste(&z).is_empty());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn abbruch_hinterlaesst_keine_halben_dateien() {
    let d = tmp("abbruch");
    let q = quelle(&d.join("q"));
    let a = d.join("t.tar.xz");
    erstellen(&a, Format::TarXz, &q);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    let w = Auswahl::alles(&l.inhalt).unwrap();
    let z = d.join("aus");
    fs::create_dir(&z).unwrap();
    let stand = Stand::default();
    struct Bricht(Stand);
    impl Rueckmeldung for Bricht {
        fn fortschritt(&mut self, fertig: u64, _: u64, _: &str) {
            if fertig > 1000 {
                self.0
                    .abbruch
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        fn konflikt(&mut self, _: &Path, _: &archiv::pfade::RelPfad) -> Konflikt {
            Konflikt::Abbrechen
        }
    }
    let r = entpacken::entpacken(
        &Auftrag {
            archiv: &a,
            roh: false,
            ziel: &z,
            auswahl: &w,
            passwort: None,
            stand: &stand,
        },
        &mut HashSet::new(),
        &mut Bricht(stand.clone()),
    );
    assert!(matches!(r, Err(entpacken::Fehler::Abgebrochen)), "{r:?}");
    assert!(reste(&z).is_empty(), "{:?}", reste(&z));
    assert!(!z.join("projekt/unter/b.bin").exists());
    fs::remove_dir_all(&d).unwrap();
}

/// zip mit Kennwort (ZipCrypto und AES-256), Lesen ohne, mit falschem und
/// mit richtigem Kennwort.
#[test]
fn passwortgeschuetzte_zip() {
    let d = tmp("kennwort");
    let q = quelle(&d.join("q"));
    for verfahren in ["zip:encryption=zipcrypt", "zip:encryption=aes256"] {
        let a = d.join("geschuetzt.zip");
        let s = Sammlung::von(&q, "").unwrap();
        schreiben::erstellen(
            &a,
            Format::Zip,
            &s,
            Some(("querencia", &[verfahren])),
            &Stand::default(),
            &mut |_, _, _: &str| {},
        )
        .unwrap();
        // Auflisten geht ohne Kennwort (zip verschlüsselt nur die Daten)
        let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
        assert_eq!(l.inhalt.dateien, 3);
        let z = d.join("aus");
        let r = alles_entpacken(&a, &z, None, &mut still());
        assert!(
            matches!(r, Err(entpacken::Fehler::Lesen(LeseFehler::PasswortNoetig))),
            "{verfahren}: {r:?}"
        );
        assert!(reste(&z).is_empty());
        let r = alles_entpacken(&a, &z, Some("falsch"), &mut still());
        assert!(
            matches!(r, Err(entpacken::Fehler::Lesen(LeseFehler::PasswortFalsch))),
            "{verfahren}: {r:?}"
        );
        let e = alles_entpacken(
            &a,
            &z,
            Some("querencia"),
            &mut Still {
                antwort: Konflikt::Ersetzen,
                konflikte: 0,
            },
        )
        .unwrap();
        assert_eq!(e.dateien, 3, "{verfahren}");
        assert_eq!(
            fs::read(z.join("projekt/a.txt")).unwrap(),
            b"Hallo Archiv\n"
        );
        // Neuschreiben würde die Verschlüsselung entfernen: verweigert
        let r = schreiben::neu_schreiben(
            &a,
            &l,
            &HashSet::from([0]),
            &Sammlung::default(),
            &Stand::default(),
            &mut |_, _, _: &str| {},
        );
        assert!(matches!(r, Err(schreiben::Fehler::Verschluesselt)), "{r:?}");
        fs::remove_dir_all(&z).unwrap();
        fs::remove_file(&a).unwrap();
        assert!(reste(&d).is_empty(), "{:?}", reste(&d));
    }
    fs::remove_dir_all(&d).unwrap();
}

/// tar mit ../, absolutem Pfad und einer Verknüpfung nach außen.
fn boeses_tar(a: &Path, aussen: &Path) {
    let mut s = ffi::Schreiber::new_pax(a);
    for name in [
        "ok.txt",
        "../boese.txt",
        &format!("{}/absolut.txt", aussen.display()),
    ] {
        s.kopf(name, schreiben::AE_IFREG, 0o644, 3, 1).unwrap();
        s.daten(b"xyz").unwrap();
        s.ende().unwrap();
    }
    s.schliessen().unwrap();
}

trait PaxNeu {
    fn new_pax(a: &Path) -> ffi::Schreiber;
}
impl PaxNeu for ffi::Schreiber {
    fn new_pax(a: &Path) -> ffi::Schreiber {
        ffi::Schreiber::neu(a, ffi::Container::Pax, ffi::Filter::Ohne, &[], None).unwrap()
    }
}

#[test]
fn boesartige_archive_werden_abgelehnt() {
    let d = tmp("boese");
    let aussen = tmp("boese-aussen");
    let a = d.join("boese.tar");
    boeses_tar(&a, &aussen);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    assert_eq!(l.inhalt.unsicher.len(), 2);
    assert_eq!(l.inhalt.dateien, 1);
    let r = Auswahl::alles(&l.inhalt);
    let abgelehnt = r.unwrap_err();
    assert_eq!(abgelehnt.0.len(), 2);
    assert!(abgelehnt.0.iter().any(|n| n == "../boese.txt"));
    // nur „ok.txt“ gewählt: erlaubt
    let ok = l.inhalt.ordner("").unwrap().dateien.clone();
    assert!(Auswahl::aus(&l.inhalt, &ok, "").is_ok());
    // eine Auswahl mit einem unsicheren Eintrag: abgelehnt
    assert!(Auswahl::aus(&l.inhalt, &[ok[0], l.inhalt.unsicher[0]], "").is_err());
    assert!(!d.join("boese.txt").exists() && !aussen.join("absolut.txt").exists());
    // Neuschreiben eines solchen Archivs: verweigert
    let r = schreiben::neu_schreiben(
        &a,
        &l,
        &HashSet::new(),
        &Sammlung::default(),
        &Stand::default(),
        &mut |_, _, _: &str| {},
    );
    assert!(matches!(r, Err(schreiben::Fehler::NichtAenderbar)));

    // Verknüpfung „weg“ → außen, danach „weg/x.txt“: die Verknüpfung wird
    // nicht angelegt, x.txt landet in einem echten Ordner „weg“
    let b = d.join("verknuepfung.tar");
    let mut s = ffi::Schreiber::new_pax(&b);
    s.verknuepfung("weg", &aussen.display().to_string())
        .unwrap();
    s.kopf("weg/x.txt", schreiben::AE_IFREG, 0o644, 1, 1)
        .unwrap();
    s.daten(b"!").unwrap();
    s.ende().unwrap();
    s.schliessen().unwrap();
    let z = d.join("aus");
    let e = alles_entpacken(&b, &z, None, &mut still()).unwrap();
    assert_eq!((e.dateien, e.nicht_angelegt), (1, 1));
    assert!(fs::symlink_metadata(z.join("weg")).unwrap().is_dir());
    assert!(!aussen.join("x.txt").exists());
    // eine Verknüpfung, die schon im Zielordner liegt, wird nicht verfolgt
    let z2 = d.join("aus2");
    fs::create_dir(&z2).unwrap();
    std::os::unix::fs::symlink(&aussen, z2.join("weg")).unwrap();
    let r = alles_entpacken(&b, &z2, None, &mut still());
    assert!(matches!(r, Err(entpacken::Fehler::Ziel(..))), "{r:?}");
    assert!(!aussen.join("x.txt").exists());
    fs::remove_dir_all(&d).unwrap();
    fs::remove_dir_all(&aussen).unwrap();
}

#[test]
fn hinzufuegen_und_entfernen_schreibt_neu() {
    let d = tmp("neu");
    let q = quelle(&d.join("q"));
    fs::write(d.join("neu.txt"), b"neu").unwrap();
    fs::write(d.join("a.txt"), b"ersetzt").unwrap();
    for f in [Format::Zip, Format::TarGz, Format::TarXz] {
        let a = d.join(format!("t{}", f.endung()));
        erstellen(&a, f, &q);
        let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
        // „Ä ö.txt“ entfernen; neu.txt in die Wurzel, a.txt nach projekt (ersetzt)
        let weg: HashSet<usize> = l
            .inhalt
            .ordner("")
            .unwrap()
            .dateien
            .iter()
            .copied()
            .collect();
        let mut s = Sammlung::von(&[d.join("neu.txt")], "").unwrap();
        s.posten
            .extend(Sammlung::von(&[d.join("a.txt")], "projekt").unwrap().posten);
        let e =
            schreiben::neu_schreiben(&a, &l, &weg, &s, &Stand::default(), &mut |_, _, _: &str| {})
                .unwrap();
        assert_eq!((e.entfernt, e.ersetzt, e.hinzugefuegt), (1, 1, 2), "{f:?}");
        let n = lesen::auflisten(&a, None, &Stand::default()).unwrap();
        assert_eq!(n.format, f);
        assert_eq!(n.inhalt.dateien, 3, "{f:?}");
        let z = d.join(format!("aus-{}", f.kurzname()));
        alles_entpacken(&a, &z, None, &mut still()).unwrap();
        assert!(!z.join("Ä ö.txt").exists());
        assert_eq!(fs::read(z.join("neu.txt")).unwrap(), b"neu");
        assert_eq!(fs::read(z.join("projekt/a.txt")).unwrap(), b"ersetzt");
        assert!(z.join("projekt/unter/b.bin").is_file());
        assert!(reste(&d).is_empty(), "{:?}", reste(&d));
    }
    // 7z und RAR: nicht änderbar
    let a = d.join("t.7z");
    erstellen(&a, Format::SiebenZip, &q);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    let r = schreiben::neu_schreiben(
        &a,
        &l,
        &HashSet::new(),
        &Sammlung::default(),
        &Stand::default(),
        &mut |_, _, _: &str| {},
    );
    assert!(matches!(r, Err(schreiben::Fehler::NichtAenderbar)));
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn einzelne_komprimierte_datei_und_hier_ziel() {
    let d = tmp("roh");
    fs::write(d.join("notiz.txt"), b"nur eine Datei\n").unwrap();
    let ok = std::process::Command::new("gzip")
        .arg("-k")
        .arg(d.join("notiz.txt"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        let a = d.join("notiz.txt.gz");
        let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
        assert!(l.roh);
        assert_eq!(l.format, Format::Gz);
        let w = l.inhalt.ordner("").unwrap();
        assert_eq!(
            l.inhalt.eintraege[w.dateien[0]].pfad().unwrap().as_str(),
            "notiz.txt"
        );
        let (ziel, neu) = entpacken::hier_ziel(&a, &l.inhalt);
        assert_eq!((ziel.as_path(), neu), (d.as_path(), false));
        let z = d.join("aus");
        alles_entpacken(&a, &z, None, &mut still()).unwrap();
        assert_eq!(fs::read(z.join("notiz.txt")).unwrap(), b"nur eine Datei\n");
    }
    // kein Archiv
    fs::write(d.join("text.zip"), b"kein zip, nur Text\n").unwrap();
    let r = lesen::auflisten(&d.join("text.zip"), None, &Stand::default());
    assert!(matches!(r, Err(LeseFehler::Format)), "{r:?}");
    // Hier-Ziel: ein oberster Ordner → direkt; mehrere → eigener Ordner
    let q = quelle(&d.join("q"));
    let a = d.join("eins.zip");
    erstellen(&a, Format::Zip, &q[..1]);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    assert_eq!(entpacken::hier_ziel(&a, &l.inhalt), (d.clone(), false));
    let a = d.join("zwei.tar.gz");
    erstellen(&a, Format::TarGz, &q);
    let l = lesen::auflisten(&a, None, &Stand::default()).unwrap();
    assert_eq!(entpacken::hier_ziel(&a, &l.inhalt), (d.join("zwei"), true));
    fs::create_dir(d.join("zwei")).unwrap();
    assert_eq!(
        entpacken::hier_ziel(&a, &l.inhalt),
        (d.join("zwei (2)"), true)
    );
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn rechte_beim_entpacken() {
    assert_eq!(entpacken::dateirechte(0o100755), 0o755);
    assert_eq!(entpacken::dateirechte(0o104755), 0o755);
    assert_eq!(entpacken::dateirechte(0o100644), 0o644);
    assert_eq!(entpacken::dateirechte(0o100400), 0o600);
    assert_eq!(entpacken::dateirechte(0o100777), 0o755);
    assert_eq!(entpacken::dateirechte(0), 0o644);
    assert_eq!(entpacken::dateirechte(0o100700), 0o700);
}
