//! Testarchive für die GUI-Prüfung und die Messung (werkzeug/).
//!
//!   cargo run --example testarchive -- <ordner> [--gross <MB>]
//!
//! Legt in <ordner> an: normal.zip, normal.tar.gz, normal.tar.xz, normal.7z
//! (gleicher Inhalt: Ordner mit Unterordnern, Umlauten, einem leeren
//! Ordner), geschuetzt.zip (AES-256, Kennwort „querencia“), boese.tar
//! (../, absoluter Pfad, Verknüpfung nach außen), notiz.txt.gz (einzelne
//! Datei), kaputt.zip (abgeschnitten), quelle/ (Dateien zum Hinzufügen).
//! Mit --gross zusätzlich gross.zip und gross.tar.gz aus <MB> MB schlecht
//! komprimierbaren Daten in 64 Dateien (für die 1-GB-Messung).

use std::fs;
use std::path::{Path, PathBuf};

use archiv::format::Format;
use archiv::lesen::Stand;
use archiv::schreiben::{self, AE_IFREG, Sammlung, ffi};

fn baum(q: &Path) {
    let p = q.join("Projekt Ä");
    fs::create_dir_all(p.join("bilder/klein")).unwrap();
    fs::create_dir_all(p.join("leer")).unwrap();
    fs::create_dir_all(p.join("texte")).unwrap();
    fs::write(
        p.join("liesmich.txt"),
        "Querencia Archiv – Testinhalt\n".repeat(40),
    )
    .unwrap();
    fs::write(
        p.join("texte/brief.txt"),
        "Sehr geehrte Damen und Herren,\n".repeat(200),
    )
    .unwrap();
    fs::write(
        p.join("texte/übersicht.md"),
        "# Übersicht\n\n- eins\n- zwei\n",
    )
    .unwrap();
    let daten: Vec<u8> = (0..180_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    fs::write(p.join("bilder/foto.raw"), &daten).unwrap();
    fs::write(
        p.join("bilder/klein/symbol.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
    )
    .unwrap();
    fs::write(q.join("lose datei.txt"), "liegt oben\n").unwrap();
}

fn erstellen(ziel: &Path, f: Format, quellen: &[PathBuf], pw: Option<(&str, &[&str])>) {
    let s = Sammlung::von(quellen, "").unwrap();
    schreiben::erstellen(ziel, f, &s, pw, &Stand::default(), &mut |_, _, _: &str| {}).unwrap();
    println!("{}", ziel.display());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(ziel) = args.first().map(PathBuf::from) else {
        eprintln!("Aufruf: testarchive <ordner> [--gross <MB>]");
        std::process::exit(2);
    };
    let gross: Option<u64> = args
        .iter()
        .position(|a| a == "--gross")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());
    fs::create_dir_all(&ziel).unwrap();
    let q = ziel.join(".baum");
    let _ = fs::remove_dir_all(&q);
    fs::create_dir_all(&q).unwrap();
    baum(&q);
    let quellen = vec![q.join("Projekt Ä"), q.join("lose datei.txt")];
    for f in [Format::Zip, Format::TarGz, Format::TarXz, Format::SiebenZip] {
        erstellen(
            &ziel.join(format!("normal{}", f.endung())),
            f,
            &quellen,
            None,
        );
    }
    erstellen(
        &ziel.join("geschuetzt.zip"),
        Format::Zip,
        &quellen,
        Some(("querencia", &["zip:encryption=aes256"])),
    );
    // bösartig: ../, absolut, Verknüpfung nach außen und Datei dahinter
    let b = ziel.join("boese.tar");
    let mut s = ffi::Schreiber::neu(&b, ffi::Container::Pax, ffi::Filter::Ohne, &[], None).unwrap();
    for name in [
        "harmlos.txt",
        "../ausbruch.txt",
        "/tmp/querencia-archiv-absolut.txt",
    ] {
        s.kopf(name, AE_IFREG, 0o644, 6, 1_760_000_000).unwrap();
        s.daten(b"boese\n").unwrap();
        s.ende().unwrap();
    }
    s.verknuepfung("weg", "/tmp").unwrap();
    s.kopf(
        "weg/hinter-verknuepfung.txt",
        AE_IFREG,
        0o644,
        6,
        1_760_000_000,
    )
    .unwrap();
    s.daten(b"boese\n").unwrap();
    s.ende().unwrap();
    s.schliessen().unwrap();
    println!("{}", b.display());
    // einzelne komprimierte Datei (libarchive „raw“): gzip-Filter ohne tar
    let n = ziel.join("notiz.txt.gz");
    let mut s = ffi::Schreiber::neu_roh(&n, ffi::Filter::Gzip).unwrap();
    s.kopf("data", AE_IFREG, 0o644, 22, 1_760_000_000).unwrap();
    s.daten(b"eine einzelne Notiz.\n\n").unwrap();
    s.ende().unwrap();
    s.schliessen().unwrap();
    println!("{}", n.display());
    // kaputt: zip in der Mitte abgeschnitten
    let voll = fs::read(ziel.join("normal.zip")).unwrap();
    fs::write(ziel.join("kaputt.zip"), &voll[..voll.len() / 2]).unwrap();
    println!("{}", ziel.join("kaputt.zip").display());
    // Dateien zum Hinzufügen
    let neu = ziel.join("quelle");
    fs::create_dir_all(neu.join("ordner/innen")).unwrap();
    fs::write(neu.join("neu.txt"), "neu hinzugefügt\n").unwrap();
    fs::write(neu.join("ordner/innen/tief.txt"), "tief\n").unwrap();
    if let Some(mb) = gross {
        let g = ziel.join(".gross");
        let _ = fs::remove_dir_all(&g);
        fs::create_dir_all(&g).unwrap();
        // xorshift: schlecht komprimierbar, aber reproduzierbar
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let je = mb * 1024 * 1024 / 64;
        let mut buf = vec![0u8; 1 << 20];
        for i in 0..64 {
            let mut f = fs::File::create(g.join(format!("teil-{i:02}.bin"))).unwrap();
            let mut rest = je;
            while rest > 0 {
                for c in buf.chunks_mut(8) {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    c.copy_from_slice(&x.to_le_bytes()[..c.len()]);
                }
                let n = rest.min(buf.len() as u64) as usize;
                std::io::Write::write_all(&mut f, &buf[..n]).unwrap();
                rest -= n as u64;
            }
        }
        erstellen(
            &ziel.join("gross.zip"),
            Format::Zip,
            std::slice::from_ref(&g),
            None,
        );
        erstellen(
            &ziel.join("gross.tar.gz"),
            Format::TarGz,
            std::slice::from_ref(&g),
            None,
        );
        fs::remove_dir_all(&g).unwrap();
    }
    fs::remove_dir_all(&q).unwrap();
}
