# Änderungsprotokoll

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/), Versionen
nach [Semantic Versioning](https://semver.org/lang/de/). Solange die Hauptversion
0 ist, kann sich alles ändern. Jede Zeile nennt, was Nutzer merken; Unfertiges
steht als „unfertig“ da.

## [Unveröffentlicht]

## [0.1.1] – 2026-10-21

Dunkle Darstellung (CSS-Welle dunkel, QD-081). Hell sieht Archiv aus wie
0.1.0.

### Geändert
- Mit „Dunkle Darstellung bevorzugen“ startet Archiv gleich dunkel, sobald
  das Thema eine dunkle Fassung hat (ab Querencia-Klassisch 0.2); vorher
  blieb es unter GTK 4.22 bis zum ersten Umschalten hell. Gilt auch für die
  Fenster der Kommandozeilen-Aufträge (Entpacken aus Files).
- Leisten und Linien folgen weiter allein dem Thema; ein Test sichert, dass
  `style.css` keine festen Farben und keine `@media`-Regel bekommt.

## [0.1.0] – 2026-10-20

Erste Alpha. Öffnen, Ansehen und Entpacken aller Formate, die libarchive
kennt; Erstellen, Hinzufügen und Löschen für zip, tar.gz und tar.xz.

### Hinzugefügt
- Fenster nach File Roller 2.30 / Engrampa: Menüleiste, Werkzeugleiste,
  Ortsleiste, Ordnerbaum, Dateiliste mit Sortierung, Statusleiste
- Lesen über libarchive (compress-tools): zip (auch mit Kennwort), tar mit
  gzip/xz/bzip2/zstd, einzelne komprimierte Dateien, 7z, RAR (nur lesen)
- Vorschau einer Datei (schreibgeschützte Kopie, Standardprogramm)
- Entpacken alles oder Auswahl mit Fortschritt, Abbruch und Rückfrage
  (Ersetzen, Überspringen, für alle); Kennwortabfrage
- Ablehnung von Archiven mit `..` oder absoluten Pfaden; Verknüpfungen aus
  dem Archiv werden nicht angelegt
- Neues Archiv (zip, tar.gz, tar.xz), Dateien und Ordner hinzufügen (auch
  durch Ziehen ins Fenster), Löschen – jeweils als Neuschreiben
- Kommandozeile `--entpacken-hier`, `--entpacken-nach`, `--erstellen`
- Deutsch und Englisch (gettext), eigenes Programmsymbol, Starter für 14
  Archivtypen, MetaInfo

### Bekannte Einschränkungen
- 7z und RAR nur lesen; verschlüsselte Archive nicht änderbar; Kennwort
  beim Erstellen fehlt
- Harte Verknüpfungen in tar-Archiven werden als leere Dateien entpackt
- Kein Ziehen aus dem Fenster nach Files; kein Umbenennen im Archiv
