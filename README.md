# Querencia Archiv

Klassische Archivverwaltung für alle, die Archive öffnen, entpacken und
packen wollen. Teil von
[Querencia Desktop](https://querencialinux.org) – einer
klassischen Desktopumgebung im GNOME-2-Stil für Wayland (labwc, Rust, GTK 4).
Installiert wird es zusammen mit Querencia Desktop aus der APT-Quelle;
Anleitung auf <https://querencialinux.org>.

*English summary:* A classic archive manager modelled on File Roller 2.30 /
Engrampa, reading every format libarchive knows and writing zip, tar.gz and
tar.xz. Part of Querencia Desktop, a classic GNOME-2-style desktop for
Wayland. Alpha software; documentation is in German.

> **Alpha.** 7z und RAR nur lesen; verschlüsselte Archive nur lesen und
> entpacken; kein Ziehen aus dem Fenster heraus. Keine Sicherheits- oder
> Datenverlustgarantie.

## Zweck

- Archive öffnen (Dialog, Kommandozeile, Ziehen und Ablegen) und als
  Ordnerbaum mit Dateiliste (Name, Größe, Typ, Änderungsdatum) zeigen;
  gelesen wird über libarchive: zip (auch mit Kennwort, ZipCrypto und
  AES), tar, tar.gz/.xz/.bz2/.zst, einzelne `.gz`/`.xz`/`.bz2`/`.zst`, 7z,
  RAR 4/5 (nur lesen, ohne unRAR), ISO, cpio u. a.
- Vorschau: Doppelklick entpackt eine Datei schreibgeschützt in einen
  eigenen Vorschauordner und öffnet sie mit dem Standardprogramm
- Entpacken aller oder der ausgewählten Dateien mit Fortschritt und
  Abbruch; vorhandene Dateien: Ersetzen, Überspringen, für alle
- **Sichere Pfade:** Ein Archiv mit `..` oder absoluten Pfaden wird nicht
  entpackt (Auftrag abgelehnt, nichts geschrieben); Verknüpfungen aus dem
  Archiv werden nie angelegt; beim Schreiben wird keiner vorhandenen
  Verknüpfung gefolgt
- Neue Archive als zip, tar.gz oder tar.xz; Dateien und Ordner
  hinzufügen (auch durch Ziehen ins Fenster) und löschen – als Neuschreiben
  in einen Hilfsnamen, das alte Archiv bleibt bis zum Schluss unverändert
- Kommandozeile für Files und Skripte (siehe unten)
- Bewusst **nicht** Teil dieses Programms (0.1.0): 7z und RAR erstellen,
  Kennwörter beim Erstellen, Ändern verschlüsselter Archive, Ziehen aus dem
  Fenster, Umbenennen im Archiv, Prüfen der Integrität

Anwendungskennung (App-ID): `org.querencia.Archiv` · Binary:
`querencia-archiv` · Debian-Paket: `querencia-archiv`

| Taste | Wirkung |
|---|---|
| Strg+N, Strg+O, Strg+E | Neues Archiv, Öffnen, Entpacken |
| Strg+W, Strg+Q | Fenster schließen, Programm beenden |
| Entf, Strg+A, Umschalt+Strg+A | Löschen, alles auswählen, Auswahl aufheben (in der Liste) |
| Eingabe / Doppelklick | Ordner öffnen bzw. Datei ansehen |
| Rücktaste, Alt+↑, Alt+←, Alt+→, Alt+Pos1 | eine Ebene hoch, zurück, vor, Anfang |
| F9, Strg+R / F5 | Ordnerleiste, neu laden |

## Kommandozeile

```text
querencia-archiv [DATEI…]                       Archive öffnen
querencia-archiv --entpacken-hier DATEI…        neben dem Archiv entpacken
querencia-archiv --entpacken-nach ORDNER DATEI… in ORDNER entpacken
querencia-archiv --erstellen ARCHIV DATEI…      neues .zip, .tar.gz oder .tar.xz
```

„Hier entpacken“ legt einen Ordner mit dem Namen des Archivs an, außer alles
liegt schon in einem einzigen Ordner. Mit Bildschirm zeigen die Aufträge
Fortschritt-, Konflikt-, Kennwort- und Fehlerfenster; ohne Bildschirm
laufen sie still und ersetzen nie. Exitcode 0 = erledigt, 1 = Fehler oder
abgebrochen, 2 = Aufruf falsch. Für Files: [docs/integration.md](docs/integration.md).

## Stand

| | |
|---|---|
| Version | 0.1.3 (Tag `archiv/0.1.3`; Italienisch, Spanisch, Französisch); seit 0.1.2 Info-Dialog mit Alpha-Hinweis, Website, Copyright und Lizenz, Hilfe-Menü mit Online-Hilfe und „Fehler melden“); seit 0.1.1 startet es unter „Dunkle Darstellung bevorzugen“ dunkel |
| Letzte Prüfung | 26.10.2026 (0.1.3): 34 Unit- und Kerntests, Clippy sauber, `i18n.py pruefen` und `msgfmt --check` für de/es/fr/it; isoliert 1024×768: Szenario `szenario.sh` de und en ohne FAIL, dazu de/en/it/es/fr alle Menüs, Statusleiste, Entpacken, Neues Archiv, Online-Hilfe, „Fehler melden“, Info (je 14 PASS). 22.10.2026 (0.1.2): 34 Unit- und Kerntests, Clippy sauber, `i18n.py pruefen` und `msgfmt -c` ohne Warnungen; isoliert de/en 1024×768: Online-Hilfe, „Fehler melden“ (je über die Zugriffstaste) und Info-Dialog. 21.10.2026 (0.1.1): 33 Unit- und Kerntests; isoliert hell gegen 0.1.0 und dunkel beim Start und beim Wechsel, je de/en × 1024×768/1920×1080, Kontrast WCAG AA |
| Bekannte Lücken | siehe [docs/testing.md](docs/testing.md), Abschnitt „Nicht getestet“ |
| Getestet auf | Ubuntu 26.04, labwc 0.9.8, GTK 4.22, libarchive 3.8.5, Rust 1.93 |
| Nicht getestet | andere Distributionen, HiDPI, Mehrmonitor, echte RAR-Archive, Ziehen aus Files |

Änderungen: [CHANGELOG.md](CHANGELOG.md).

## Bauen und Prüfen

`cargo build --release --locked`, danach `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` und `python3 werkzeug/i18n.py pruefen` (laufen ohne Bildschirm; die ersten drei sind die CI `.github/workflows/ci.yml`).
Voraussetzungen (`libarchive-dev`), Schreib-Anbindung, Übersetzungen, Paket: [docs/building.md](docs/building.md); isolierte GUI-Prüfung (`bash werkzeug/lauf.sh de 1024x768`, nie in der laufenden Sitzung – Kommandozeilentests nur über `werkzeug/ohne-bildschirm.sh` mit eigenem `XDG_RUNTIME_DIR`) und Messung: [docs/testing.md](docs/testing.md); Integration: [docs/integration.md](docs/integration.md).

## Vorbild und Abgrenzung

Vorbild für Bedienung und Aussehen ist **File Roller 2.30 aus GNOME 2** bzw.
sein Nachfolger **Engrampa** (MATE): Menüs Archiv/Bearbeiten/Ansicht/Hilfe,
Werkzeugleiste, Ortsleiste mit Zurück/Vor/Hoch/Anfang, Ordnerleiste neben
der Liste, Statusleiste. Querencia übernimmt davon die Bedienideen,
**keinen Quellcode, keine Texte, keine Grafiken**. Das Projekt ist mit File
Roller, Engrampa und deren Autoren weder verbunden noch von ihnen
unterstützt.

Abweichungen vom Vorbild und Dinge, die bewusst fehlen:

- keine externen Werkzeuge (`unzip`, `7z`, `unrar`): alles über libarchive
- „Entpacken“ fragt Ziel und Umfang in einem eigenen kleinen Fenster statt
  im erweiterten Dateidialog; „Ordner wiederherstellen“ ist immer an,
  „ältere nicht überschreiben“ fehlt
- Hinzufügen ersetzt gleichnamige Dateien ohne Rückfrage (wie File Roller
  mit „Aktualisieren“)
- Einträge mit unsicheren Pfaden werden angezeigt (Warnsymbol), aber ein
  Archiv mit solchen Einträgen wird nie ganz entpackt

## Lizenz

- Programm: GPL-3.0-or-later, siehe [LICENSE](LICENSE). Copyright 2026
  Querencia Desktop contributors; Urheberhinweis in
  [COPYRIGHT](COPYRIGHT).
- **libarchive:** Querencia Archiv linkt dynamisch gegen `libarchive`
  (BSD-2-Clause; Ubuntu-Paket `libarchive13t64`), über das Crate
  compress-tools (MIT oder Apache-2.0) und eine eigene schmale
  Schreib-Anbindung. RAR liest libarchive mit eigenem, freiem Code; unRAR
  (nicht frei) wird nicht benutzt, RAR erstellen kann das Programm nicht.
  Weitere Bibliotheken: GTK 4, GLib, Cairo, Pango (LGPL), gtk-rs, libc,
  gettext-rs (MIT bzw. MIT oder Apache-2.0).
- `werkzeug/virtual-keyboard-unstable-v1.xml` (nur Testwerkzeug) behält den
  MIT-Vermerk im Dateikopf.
- Übersetzungen (`po/`) stehen unter derselben Lizenz wie das Programm.

Name und Logo „Querencia“ sind keine freien Inhalte: [MARKE.md](MARKE.md).

## Fehler melden

Fehler, Wünsche und Fragen zu Querencia gehen an die gemeinsame Meldestelle:
<https://github.com/querencia-desktop/querencia/issues/new/choose>.
Sicherheitslücken bitte nicht öffentlich melden, siehe [SECURITY.md](SECURITY.md).
