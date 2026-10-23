# Prüfen

## Unit- und Kerntests

```bash
bash werkzeug/bauen.sh pruefen      # fmt, Clippy, dann cargo test ohne Bau-Platz
```

- `src/pfade.rs`: Normalisieren (`./a//b`, Umlaute, Punkte im Namen),
  Ausbrüche (`../`, `a/../..`, `/etc/passwd`, `//x`), Ordner und Dateien
  folgen keiner Verknüpfung, freie Namen „(2)“.
- `src/modell.rs`: Baumaufbau mit impliziten Ordnern, Präfix ≠ Ordner
  (`ab/` gegen `a/`), doppelte Einträge, unsichere Einträge, Arten,
  einziger oberster Ordner.
- `src/format.rs`: Formaterkennung aus Kennbytes und Namen (zip, 7z, RAR 4/5,
  tar, gz/xz/bz2/zst mit und ohne tar, Inhalt vor Name), schreibbare
  Formate, Endungen.
- `src/i18n.rs`, `src/anzeige.rs`: Platzhalter, Katalogsuche, deutscher
  Katalog vollständig, Größen und Datum.
- `src/kommandozeile.rs`: alle Aufrufe und Fehlaufrufe.
- `tests/kern.rs` (echtes libarchive, Archive entstehen im Test):
  zip/tar.gz/tar.xz/7z hin und zurück mit Umlauten und leerem Ordner;
  Auswahl ohne angezeigten Ordner; Konflikte (Überspringen, Ersetzen,
  Abbrechen); Abbruch ohne halbe Dateien; zip mit ZipCrypto und AES-256
  (ohne, falsches, richtiges Kennwort; Neuschreiben verweigert); bösartiges
  tar (`../`, absolut, Verknüpfung nach außen, vorhandene Verknüpfung im
  Ziel); Hinzufügen/Ersetzen/Entfernen in zip, tar.gz, tar.xz; 7z nicht
  änderbar; einzelne `.gz`; kein Archiv; „Hier entpacken“; Rechte.

Ohne Bildschirm nur über `werkzeug/ohne-bildschirm.sh` (eigenes, leeres
`XDG_RUNTIME_DIR`): Nur `WAYLAND_DISPLAY` zu entfernen genügt nicht, GTK
verbindet sich dann mit `$XDG_RUNTIME_DIR/wayland-0`, also mit der laufenden
Sitzung.

## Isolierte GUI-Prüfung

```bash
bash werkzeug/werkzeug-bauen.sh                 # qeingabe/qausgabe (einmal)
cargo build --example testarchive && cp target/debug/examples/testarchive werkzeug/bin/
bash werkzeug/lauf.sh de 1024x768               # ebenso en, 1920x1080
```

`lauf.sh` legt mit `profil.sh` ein Wegwerfprofil unter `gui-test/<name>/` an
(labwc-Konfiguration der Sitzung, Thema Querencia-Klassisch, Symbole,
Ablage-Kopie mit Katalog unter `opt/archiv/<sha16>/`, Testarchive in
`home/Archive`, ein Vorschau-Prüfstarter, der nur protokolliert) und
startet `isoliert.py`: eigener labwc ohne Bildschirm, eigenes `HOME`,
`XDG_*`, D-Bus, keine Portale, kein gvfs. Das Szenario
`werkzeug/szenario.sh` bedient nur die Tastatur (de-Layout, qeingabe) und
macht Bildschirmfotos (grim). Belege und Protokolle: `nachweis/gui/<name>/`
(`ablauf.log` mit PASS/FAIL je Prüfpunkt, `status.txt`).

Ablauf: Menüs Archiv/Bearbeiten/Ansicht/Hilfe; Ordner öffnen, hoch, zurück,
vor, Anfang; Ordnerleiste aus/ein; alles auswählen/Auswahl aufheben;
Vorschau (liesmich.txt, schreibgeschützt); Entpacken alles; Konflikt mit
„für alle“ + Überspringen; Ersetzen einer ausgewählten Datei; zip mit
Kennwort (falsch, richtig); bösartiges tar (Warnung in Liste und
Statusleiste, Entpacken abgelehnt, nichts geschrieben); 7z (nur lesen);
kaputtes zip (Fehlermeldung); Neues Archiv, Datei und Ordner über den
Dateidialog hinzufügen, Löschen mit Rückfrage; Info; Ortsfeld;
Kommandozeile `--entpacken-hier` (tar.xz, einzelne .gz), `--erstellen`,
`--entpacken-nach` mit Konfliktfenster (Escape → Exit 1); Strg+Q.

Prüfpunkte (alle vier Läufe): vorschau, entpacken-alles,
konflikt-uebersprungen, konflikt-ersetzt, kennwort, boese-abgelehnt,
neu-erstellt, cli-hier, cli-gz, cli-erstellen; dazu Katalog in
`/proc/<pid>/maps` (de: aus der Ablage, en: keiner), keine Reste im
Vorschauordner, Prozessende nach Strg+Q.

## Dunkle Darstellung (0.1.1, CSS-Welle dunkel)

Die Prüfwerkzeuge für 0.1.1 liegen im Arbeitsordner
der Fassung (`<arbeitsordner-011>/werkzeug/`, nicht im Repo): `lauf-011.sh
alt|neu hell|start|wechsel|kontrast de|en <BxH>` startet einen isolierten
Lauf wie oben, aber immer im selben Profilpfad, mit einmal erzeugten
Testarchiven (feste Zeiten) und mit Thema, Symbolen und Schriftwerten aus
einem Schnappschuss, damit Läufe von 0.1.0 und 0.1.1 pixelgleich
vergleichbar sind. `hell` ist das Szenario oben plus Dateidialog „Öffnen“
(ohne Portal, Thema 0.1); `start` und `wechsel` laufen mit einem
Settings-Portal-Ersatz (`color-scheme` 0/1, im Lauf umschaltbar) und Thema
0.2. `vergleich-011.py` vergleicht zwei Ergebnisordner Pixel für Pixel,
`farbanteil.py` ordnet Fotos hell/dunkel zu, `linien-011.py` misst die drei
eigenen Linien aus `style.css`, `kontrast-archiv.py` den Kontrast dunkel
(WCAG AA).

`style.css` hat keine festen Farben und keine `@media`-Regel (Test
`fenster::tests::css_ohne_feste_farben_und_ohne_media`). Die Linien nehmen
`alpha(currentColor, 0.18)` und folgen dem Thema ohne Bindung des
CSS-Providers an `gtk-interface-color-scheme` (gemessen: hell `#d0d0cf`,
dunkel `#595752`, beim Start wie beim Wechsel). Eine `@media`-Regel bräuchte
diese Bindung (gemessen in Bilder 0.2.4, Dokumente 0.1.2, Software 0.1.4).

## Messung (1-GB-Archive)

```bash
QA_GROSS=1024 SZENARIO=messung.sh ZEIT=1800 bash werkzeug/lauf.sh de 1024x768 messung-de-1024x768
```

`gross.zip` und `gross.tar.gz` aus 64 schlecht komprimierbaren Dateien
(zusammen 1 GiB): Öffnen bis Liste, Entpacken alles, Abbruch mitten im
Entpacken (keine halben Dateien), Speicher (VmRSS/VmHWM), Kommandozeile
unter `/usr/bin/time -v`. Ergebnis `nachweis/gui/messung-*/messung.txt`.

## Nicht getestet

- echte Sitzung (Panel-Menü, Doppelklick in Files, Ziehen aus Files)
- RAR-Archive (kein freies Werkzeug zum Erzeugen auf der VM), 7z mit
  Kennwort, ISO/cpio/CAB
- harte Verknüpfungen in tar (werden als leere Dateien entpackt)
- Dateinamen ohne gültiges UTF-8, sehr viele Einträge (> 100 000)
- HiDPI, Mehrmonitor, GL-Renderer, `LANG=C`, CI auf GitHub
