# Integration in die laufende Sitzung

Skript: `integration/werkzeug/archiv-010-integrieren.sh` (erzeugt aus
`archiv-010-integrieren.sh.in` durch `vorlage-010-erzeugen.sh <commit>`, das
die Prüfsummen einträgt). Vorbild: Dokumente 0.1.0.

```bash
bash integration/werkzeug/archiv-010-integrieren.sh pruefen
ARCH_FREIGABE=ja bash integration/werkzeug/archiv-010-integrieren.sh aktivieren   # nur nach Freigabe
ARCH_FREIGABE=ja bash integration/werkzeug/archiv-010-integrieren.sh zurueck
```

Ist `HOME` das echte Heimatverzeichnis, verlangen `aktivieren` und
`zurueck` `ARCH_FREIGABE=ja`; mit umgelenktem `HOME` (Probe) arbeitet das
Skript nur dort. `aktivieren` startet nichts.

| Teil | Was | alt | neu |
|---|---|---|---|
| A | Ablage `~/opt/querencia/archiv/<sha16>/` mit `querencia-archiv` (0555), `share/locale/de/LC_MESSAGES/querencia-archiv.mo` und `SHA256SUMS` (0444), Ordner 0555, `.herkunft` daneben; entsteht unter einem Hilfsnamen und wird in einem Schritt umbenannt | – | bleibt bei `zurueck` liegen |
| P | Link `~/opt/querencia/bin/querencia-archiv` | fehlt | → Ablage |
| S | Symbol `~/.local/share/icons/hicolor/{16,22,24,32,64,128,256}x…/apps/org.querencia.Archiv.png` und `scalable/…svg`, **nie 48×48**; danach `touch hicolor` | keine der acht | alle acht wie Vorlage |
| D | Starter `~/.local/share/applications/org.querencia.Archiv.desktop` (Exec/TryExec auf die Ablage, `desktop-file-validate`), danach `mimeinfo.cache` neu | fehlt | wie Vorlage |
| M | `~/.config/mimeapps.list`, `[Default Applications]`: je eine Zeile `<typ>=org.querencia.Archiv.desktop` für 14 Typen | sonst | alle 14 |

Die 14 Typen: `application/zip`, `application/x-tar`, `application/gzip`,
`application/x-xz`, `application/x-bzip2`, `application/zstd`,
`application/x-7z-compressed`, `application/vnd.rar` (nur lesen) sowie
`application/x-compressed-tar`, `application/x-xz-compressed-tar`,
`application/x-bzip2-compressed-tar`, `application/x-bzip-compressed-tar`,
`application/x-zstd-compressed-tar` und `application/x-zip-compressed`.
Die tar-Untertypen sind nötig, weil shared-mime-info `.tar.gz` als
`application/x-compressed-tar` führt, nicht als `application/gzip`.

`aktivieren` merkt die Werte der 14 Zeilen vorher in
`~/.local/state/querencia/archiv-010-mime-vorher.txt`; `zurueck` stellt
jede Zeile, die noch auf Archiv zeigt, auf ihren Vorwert zurück (oder
entfernt sie) und lässt eine Zeile stehen, die der Benutzer inzwischen auf
ein anderes Programm gestellt hat. Fehlte `mimeapps.list` vorher und bleibt
nur die leere Überschrift, wird sie wieder entfernt.

Ablauf je Lauf: Sperre (`archiv-integrieren.lock`), Invarianten (Prüfsummen
der Vorlage, Katalog vorhanden, kein 48×48, Starter mit genau zwei
`@ABLAGE@`, MimeType = die 14 Typen), Zustand jedes Teils (`fremd` →
Exit 2, nichts geändert), Sicherung
(`~/.local/state/querencia/sicherung-archiv-010-<befehl>-<zeit>-<pid>/`),
Einzelschritte atomar über Hilfsnamen, Kontrolle nach jedem Schritt,
Rückrollen bei Fehler oder Signal, Schlusskontrolle mit GIO (alle 14 Typen
nennen Archiv), Journal `~/opt/querencia/protokoll.txt` mit Rückfallbefehl.

Exit-Codes: 0 erledigt, 1 zurückgerollt (Zustand wie vorher), 2 verweigert
(nichts geändert), 3 Rückrollen unvollständig (Sicherung liegt vor).

## Probe

```bash
bash integration/werkzeug/probe-010.sh "$PWD/gui-test/probe"
```

Wegwerf-HOME aus einer Kopie des Live-Stands (Starter, mimeinfo.cache,
hicolor, mimeapps.list, leere `opt/querencia/bin`): Grundlauf,
Idempotenz, Fehler und Signale an jeder Stelle (`ARCH_PROBE=1
ARCH_FEHLER=<stelle>|signal-<stelle>`), fremde Vorwerte, fehlende
`mimeapps.list`, später geänderter Standard, halber M-Stand, fremde Teile,
veränderte Ablage/Vorlage, belegte Sperre. Live wird nur ohne Freigabe
aufgerufen (Exit 2) und am Ende auf Unverändertheit verglichen.

## Versionswechsel 0.1.0 → 0.1.1

Archiv 0.1.1 (CSS-Welle dunkel) wird nicht mit dem Skript oben, sondern mit
einem eigenen Wechselskript im Arbeitsordner aktiviert (Muster Dokumente
0.1.2); Skript und Vorlage von 0.1.0 bleiben als Rückfall unverändert.

```bash
bash ~/projects/querencia-archiv-011/integration/archiv-011-integrieren.sh pruefen
ARC011_FREIGABE=ja bash ~/projects/querencia-archiv-011/integration/archiv-011-integrieren.sh aktivieren   # nur nach Freigabe, mit der Welle
ARC011_FREIGABE=ja bash ~/projects/querencia-archiv-011/integration/archiv-011-integrieren.sh zurueck      # → 0.1.0
```

Es ändert nur den Link `~/opt/querencia/bin/querencia-archiv` (P) und im
Starter Exec/TryExec (D) und legt die neue Ablage an; Symbole,
`mimeapps.list` und `mimeinfo.cache` bleiben. „alt“ heißt hier Live-Stand
0.1.0 (Binary und Starter mit festen Prüfsummen), sonst gilt dasselbe
Verfahren wie oben (Sperre `archiv-integrieren.lock`, Sicherung,
Rückrollen, Journal). Probe mit umgelenkten Pfaden `ARC011_OPT`,
`ARC011_APPS`, `ARC011_STATE` (alle oder keiner):
`~/projects/querencia-archiv-011/werkzeug/probe.sh <leerer-ordner>`.
Rückbau: erst dieses Skript `zurueck`, danach bei Bedarf das 0.1.0-Skript.

## Kommandozeile für Files (Auftrag A3)

Querencia Archiv braucht für Files kein D-Bus; die Kontextmenü-Aktionen
rufen die Kommandozeile auf (eigener Prozess je Aufruf, mit Fortschritts-,
Konflikt-, Kennwort- und Fehlerfenster):

| Aktion im Kontextmenü | Aufruf | Für |
|---|---|---|
| Hier entpacken | `querencia-archiv --entpacken-hier %F` | Archive (die 14 Typen) |
| Entpacken nach … | Ordner wählen, dann `querencia-archiv --entpacken-nach <ordner> %F` | Archive |
| Komprimieren … | Name und Typ erfragen, dann `querencia-archiv --erstellen <ordner>/<name>.zip %F` (oder `.tar.gz`, `.tar.xz`) | beliebige Dateien und Ordner |
| Mit Archivverwaltung öffnen | `querencia-archiv %F` (Starter) | Archive |

- „Hier entpacken“ legt einen Ordner mit dem Archivnamen ohne Endung an
  (`bericht.tar.gz` → `bericht/`, belegt: `bericht (2)/`), außer alles
  liegt schon in genau einem obersten Ordner oder das Archiv enthält nur
  eine Datei; dann direkt neben das Archiv.
- `--erstellen` verweigert ein vorhandenes Ziel (Exit 1) und andere Endungen
  als `.zip`, `.tar.gz`/`.tgz`, `.tar.xz`/`.txz`.
- Exitcode 0 = erledigt, 1 = Fehler, Abbruch oder abgelehnt (unsichere
  Pfade), 2 = Aufruf falsch. Ohne Bildschirm laufen die Aufträge still
  (vorhandene Dateien werden übersprungen, nie ersetzt).
- Ziehen aus dem Archivfenster nach Files gibt es in 0.1.0 noch nicht;
  Ziehen von Dateien **in** das Fenster fügt sie dem offenen Archiv hinzu
  (ohne offenes Archiv: ein einzelnes Archiv wird geöffnet, sonst fragt
  „Neues Archiv“).
