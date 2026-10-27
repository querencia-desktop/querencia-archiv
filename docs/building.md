# Bauen

## Voraussetzungen

Ubuntu 26.04 mit `build-essential pkg-config rustc cargo libgtk-4-dev
libarchive-dev gettext` (Rust 1.93 aus dem Archiv reicht; libarchive 3.8.5).
Crates: gtk4 0.11, compress-tools 0.16 (MIT oder Apache-2.0, Lesen über
libarchive), libc 0.2, gettext-rs 0.8 (`gettext-system`),
futures-channel/-util 0.3; alle Versionen in `Cargo.lock`. Die Bauskripte
arbeiten offline (`--offline --locked`).

Das Crate besteht aus einer Bibliothek `archiv` (`src/lib.rs`: Format,
Pfade, Modell, Lesen, Entpacken, Schreiben, i18n, Anzeigeformate) und dem
Programm `querencia-archiv` (`src/main.rs`: Fenster, Dialoge,
Hintergrundarbeit, Kommandozeile). `examples/testarchive.rs` erzeugt die
Testarchive der GUI-Prüfung.

## Schreiben über libarchive

compress-tools bindet nur das Lesen an. Das Schreiben (zip, tar.gz, tar.xz;
fürs Neuschreiben auch tar, tar.bz2, tar.zst; 7z und „raw“ nur für das
Testwerkzeug) geht über `src/schreiben/ffi.rs`: etwa zwanzig Funktionen aus
`archive.h`/`archive_entry.h`, mit Typen wie in libarchive 3.8 für Linux, und
ein sicherer Typ `Schreiber`. Das ist das einzige Modul mit `unsafe`
(`#![allow(unsafe_code)]`, Lint sonst `deny`). Während des Schreibens gilt
fadenlokal die Locale C.UTF-8 (Namen mit Umlauten auch ohne `setlocale`);
zip schreibt die Namen als UTF-8 mit gesetztem Kennbit (`zip:hdrcharset=UTF-8`),
tar als pax.

## Release mit Prüfungen

```bash
bash werkzeug/bauen.sh            # fmt, Clippy, Tests, dann Release
bash werkzeug/bauen.sh pruefen    # nur fmt, Clippy, Tests
bash werkzeug/bauen.sh release    # nur Release
```

- Übersetzt wird auf einem der zwei gemeinsamen Bau-Plätze
  (`~/.local/state/querencia/bau-platz.sh`, `nice -n 10`,
  `CARGO_BUILD_JOBS=4`) mit eigenem `TMPDIR` (`tmp/`); `cargo test` läuft
  danach **ohne** Bau-Platz und über `werkzeug/ohne-bildschirm.sh`.
- Der Release-Build läuft mit
  `--remap-path-prefix=<repo>=/usr/src/querencia-archiv` und
  `--remap-path-prefix=<CARGO_HOME>=/usr/share/cargo` nach
  `target/remap/release/`. Das Skript zählt am Ende die Home-Pfade im
  Binary (`strings -a … | grep -c $HOME`, Soll 0).
- Protokolle: `nachweis/bau/<art>.log`, Prüfsumme `nachweis/bau/release-sha256.txt`.

## Übersetzungen

Englisch steht im Code (msgid), Deutsch, Italienisch, Spanisch und
Französisch in `po/de.po`, `po/it.po`, `po/es.po`, `po/fr.po`. Werkzeug ist
`werkzeug/i18n.py` aus dem Editor (nur Python-Standardbibliothek):

```bash
python3 werkzeug/i18n.py pot      # po/querencia-archiv.pot aus src/ und data/*.desktop.in
python3 werkzeug/i18n.py update   # po/*.po nachziehen, dann übersetzen
python3 werkzeug/i18n.py desktop data/org.querencia.Archiv.desktop.in data/org.querencia.Archiv.desktop
python3 werkzeug/i18n.py pruefen  # muss PASS melden
python3 werkzeug/i18n.py mo <ziel>
msgfmt -c -o /dev/null po/de.po   # Gegenprobe mit GNU gettext
```

Die Binary sucht `querencia-archiv.mo` in dieser Reihenfolge:
`QUERENCIA_ARCHIV_LOCALEDIR`, `<Ordner der Binary>/share/locale`,
`<Präfix>/share/locale`, `/usr/share/locale`. Ohne Katalog ist die
Oberfläche englisch; ohne Bildschirm (Kommandozeile still) ebenfalls, weil
dann niemand `setlocale` aufruft. Die Spalte „Typ“ übersetzt
shared-mime-info selbst.

## Symbol und Ressource

Quelle ist `data/icons/hicolor/scalable/apps/org.querencia.Archiv.svg`
(Entwurf im Stil der Querencia-Symbole: Karton mit Reißverschluss, graue
Linien #5a5a5a, Zipper #F07746; das Symbole-Repo übernimmt ihn später). Nach
einer Änderung:

```bash
python3 -I werkzeug/symbol-rendern.py     # PNG 16–256, nie 48x48
glib-compile-resources --sourcedir=data/icons/hicolor/scalable/apps \
  --target=data/resource/archiv.gresource data/resource/archiv.gresource.xml
```

Der Unit-Test `resource::tests` schlägt fehl, solange die Ressource nicht
zum SVG passt. Das Info-Fenster nimmt das Symbol unter eigenem Namen
(`org.querencia.Archiv-info`) aus der Ressource.

## Paket (später)

- `/usr/bin/querencia-archiv`
- `/usr/share/locale/<lang>/LC_MESSAGES/querencia-archiv.mo` für de, es, fr, it (aus `po/<lang>.po`
  mit `msgfmt`; Build-Abhängigkeit `gettext`)
- `/usr/share/applications/org.querencia.Archiv.desktop`
  (`data/org.querencia.Archiv.desktop`)
- `/usr/share/icons/hicolor/{16…256,scalable}/apps/org.querencia.Archiv.*`
- `/usr/share/metainfo/org.querencia.Archiv.metainfo.xml`
- Abhängigkeit `libarchive13t64`; Build-Abhängigkeit `libarchive-dev`;
  `debian/copyright` mit dem Hinweis „gelinkt gegen libarchive (BSD-2-Clause)“.
