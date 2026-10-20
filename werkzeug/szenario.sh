#!/bin/bash
# Archiv 0.1.0: GUI-Prüfung in der isolierten Sitzung (isoliert.py), je Sprache.
# Umgebung aus $QA_PROFILE/env.sh: BIN, RES, OUT, SPR (de|en), T (Testarchive), W.
# Nur Tastatur (de-Layout) und Bildschirmfotos; beendet mit Strg+Q.
# Belege: alle vier Menüs, Ordner öffnen/zurück/hoch, Ordnerleiste aus/ein,
# Auswahl und Statusleiste, Vorschau (Prüfstarter im Profil), Entpacken alles
# mit Fortschritt, Konfliktfenster („für alle“ + Überspringen), Kennwort
# (falsch, richtig), bösartiges Archiv (abgelehnt), 7z (nur lesen), kaputtes
# zip (Fehler), Neues Archiv + Dateien hinzufügen (Dateidialog) + Löschen,
# Info, Kommandozeile (--entpacken-hier, --entpacken-nach mit Konflikt,
# --erstellen).
set -u
. "$QA_PROFILE/env.sh"
Q=$W/bin/qeingabe; QA=$W/bin/qausgabe
L=$QA_PROFILE/logs
mkdir -p "$OUT"
"$QA" "$RES" >>"$L/szenario.log" 2>&1; sleep 1
B=${RES%x*}; H=${RES#*x}
# Software-Renderer zeichnet 1920×1080 langsamer: längere Wartezeiten
if [ "$B" -gt 1500 ]; then WT=1200; else WT=700; fi
# Zugriffstasten je Sprache
case $SPR in
de) ALLE=d; FUERALLE=f; SKIP=b; ERSETZEN=e; LOESCHEN=l; INFO=i; ORT=o ;;
*) ALLE=a; FUERALLE=a; SKIP=s; ERSETZEN=r; LOESCHEN=d; INFO=a; ORT=l ;;
esac
export GSK_RENDERER=cairo RUST_BACKTRACE=1
echo "LANG=$LANG LANGUAGE=$LANGUAGE BIN=$BIN" >>"$L/szenario.log"
"$BIN" "$T/normal.zip" >"$L/app.log" 2>&1 &
PID=$!
echo "app pid $PID $(readlink /proc/$PID/exe)" >>"$L/szenario.log"
sleep $((WT / 200 + 1))
. "$W/eingabe.sh"
n=0
shot() { n=$((n + 1)); local f; f=$(printf '%s/%02d-%s-%s-%s.png' "$OUT" "$n" "$1" "$SPR" "$RES"); grim "$f"; echo "$(date +%T) $f" >>"$L/szenario.log"; }
lebt() { kill -0 "$PID" 2>/dev/null && echo lebt || echo beendet; }
schritt() { local name=$1; shift; q "$@" "wait $WT"; shot "$name"; echo "$(date +%T) $name: $(lebt)" >>"$L/ablauf.log"; }
pruefe() { if eval "$2"; then echo "PASS $1" >>"$L/ablauf.log"; else echo "FAIL $1 ($2)" >>"$L/ablauf.log"; fi; }

q "size $B $H" "move $((B / 2)) $((H / 2))" "wait 300"
shot start
MO=$(grep -o '/[^ ]*querencia-archiv\.mo' "/proc/$PID/maps" 2>/dev/null | head -1)
echo "katalog: ${MO:-keiner}" >>"$L/ablauf.log"
# Menüs: F10 öffnet „Archiv“, → wechselt zum nächsten Menü
schritt menue-1-archiv "key F10" "wait 400"
schritt menue-2-bearbeiten "key Right"
schritt menue-3-ansicht "key Right"
schritt menue-4-hilfe "key Right"
schritt menue-zu "key Escape" "wait 300" "key Escape"
# Ordner: Home wählt die erste Zeile („Projekt Ä“), Eingabe öffnet ihn
schritt ordner-auf "key Home" "wait 300" "key Return"
schritt ordner-tiefer "key Home" "wait 300" "key Return"
schritt hoch "chord alt Up"
schritt zurueck "chord alt Left"
schritt vor "chord alt Right"
schritt anfang "chord alt Home"
schritt ordnerleiste-aus "key F9"
schritt ordnerleiste-ein "key F9"
# Auswahl und Statusleiste
schritt alles-auswaehlen "key Home" "wait 300" "chord ctrl a"
schritt auswahl-aufheben "chord ctrl+shift a"
# Vorschau: liesmich.txt ist in „Projekt Ä“ die letzte Zeile (Ordner zuerst)
schritt vorschau "key Home" "wait 300" "key Return" "wait 500" "key End" "wait 300" "key Return" "wait 1500"
pruefe vorschau "grep -q 'liesmich.txt' '$L/vorschau.log' 2>/dev/null && grep -q '^-r--r--r--' '$L/vorschau.log'"
q "chord alt Home" "wait 400"
# Entpacken alles nach $T/aus
schritt entpacken-dialog "chord ctrl e" "wait 600"
schritt entpacken-ziel "chord ctrl a" "type $T/aus" "chord alt $ALLE"
schritt entpacken-fertig "key Return" "wait 2000"
pruefe entpacken-alles "[ -f '$T/aus/Projekt Ä/texte/brief.txt' ] && [ -d '$T/aus/Projekt Ä/leer' ] && [ -f '$T/aus/lose datei.txt' ]"
# noch einmal: Konfliktfenster, „für alle“, Überspringen
echo geaendert >"$T/aus/lose datei.txt"
schritt konflikt-dialog "chord ctrl e" "wait 600" "chord ctrl a" "type $T/aus" "chord alt $ALLE" "key Return" "wait 1500"
schritt konflikt-fuer-alle "chord alt $FUERALLE"
schritt konflikt-uebersprungen "chord alt $SKIP" "wait 1500"
pruefe konflikt-uebersprungen "[ \"\$(cat '$T/aus/lose datei.txt')\" = geaendert ]"
# Ersetzen für eine Datei (Auswahl: lose datei.txt ist die letzte Zeile der Wurzel)
schritt ersetzen-auswahl "key End" "wait 300" "chord ctrl e" "wait 600" "chord ctrl a" "type $T/aus" "key Return" "wait 1500"
schritt ersetzen-fertig "chord alt $ERSETZEN" "wait 1500"
pruefe konflikt-ersetzt "[ \"\$(cat '$T/aus/lose datei.txt')\" = 'liegt oben' ]"
# Kennwort: geschuetzt.zip in einem zweiten Fenster
"$BIN" "$T/geschuetzt.zip" >>"$L/app2.log" 2>&1
schritt geschuetzt-offen "wait 1500"
schritt kennwort-frage "chord ctrl e" "wait 600" "chord ctrl a" "type $T/aus-pw" "chord alt $ALLE" "key Return" "wait 2000"
schritt kennwort-falsch "type falsch" "key Return" "wait 2000"
schritt kennwort-richtig "type querencia" "key Return" "wait 2500"
pruefe kennwort "[ -f '$T/aus-pw/Projekt Ä/liesmich.txt' ]"
q "chord ctrl w" "wait 800"
# bösartiges Archiv: Warnung in Liste und Statusleiste, Entpacken abgelehnt
"$BIN" "$T/boese.tar" >>"$L/app2.log" 2>&1
schritt boese-offen "wait 1500"
schritt boese-abgelehnt "chord ctrl e" "wait 600" "chord ctrl a" "type $T/aus-boese" "chord alt $ALLE" "key Return" "wait 1500"
pruefe boese-abgelehnt "[ ! -e '$T/ausbruch.txt' ] && [ ! -e /tmp/querencia-archiv-absolut.txt ] && [ -z \"\$(ls -A '$T/aus-boese' 2>/dev/null)\" ]"
q "key Escape" "wait 500" "chord ctrl w" "wait 800"
# 7z: nur lesen; kaputtes zip: Fehlermeldung
"$BIN" "$T/normal.7z" >>"$L/app2.log" 2>&1
schritt siebenzip "wait 1500" "key Home" "wait 300" "key Return"
q "chord ctrl w" "wait 800"
"$BIN" "$T/kaputt.zip" >>"$L/app2.log" 2>&1
schritt kaputt "wait 2000"
q "key Escape" "wait 500" "chord ctrl w" "wait 800"
# Neues Archiv, Datei über den Dateidialog hinzufügen, wieder löschen
"$BIN" >>"$L/app2.log" 2>&1
schritt neu-leer "wait 1200"
schritt neu-dialog "chord ctrl n" "wait 800"
schritt neu-name "chord ctrl a" "type neu-gui" "key Tab" "chord ctrl a" "type $T" "key Return" "wait 1000"
# F10 → „Archiv“, → „Bearbeiten“ (erster Eintrag „Dateien hinzufügen“ ist markiert)
schritt hinzufuegen-dialog "key F10" "wait 400" "key Right" "wait 300" "key Return" "wait 1800"
schritt hinzufuegen-pfad "type $T/quelle/neu.txt" "wait 800"
schritt hinzugefuegt "key Return" "wait 2500"
pruefe neu-erstellt "[ -f '$T/neu-gui.zip' ]"
# Ordner hinzufügen (zweiter Eintrag in „Bearbeiten“)
schritt ordner-dialog "key F10" "wait 400" "key Right" "wait 300" "key Down" "wait 300" "key Return" "wait 1800"
schritt ordner-pfad "type $T/quelle/ordner" "wait 800" "key Return" "wait 1200"
schritt ordner-hinzugefuegt "key Return" "wait 2500"
schritt loeschen-frage "key Home" "wait 300" "key Delete" "wait 800"
schritt geloescht "chord alt $LOESCHEN" "wait 2500"
# Info über das Menü (F10 → ← = Hilfe → ↓ → Zugriffstaste)
schritt info "key F10" "wait 400" "key Left" "wait 400" "key Down" "wait 300" "key $INFO" "wait 800"
schritt info-zu "key Escape"
schritt ortsfeld "chord alt $ORT" "wait 300"
q "key Escape" "wait 300" "chord ctrl w" "wait 800"
# Kommandozeile in derselben isolierten Sitzung
"$BIN" --entpacken-hier "$T/normal.tar.xz" >>"$L/cli.log" 2>&1; echo "cli hier rc=$?" >>"$L/ablauf.log"
pruefe cli-hier "[ -f '$T/normal/Projekt Ä/liesmich.txt' ]"
"$BIN" --entpacken-hier "$T/notiz.txt.gz" >>"$L/cli.log" 2>&1; echo "cli gz rc=$?" >>"$L/ablauf.log"
pruefe cli-gz "grep -q Notiz '$T/notiz.txt'"
"$BIN" --erstellen "$T/cli.tar.gz" "$T/quelle" >>"$L/cli.log" 2>&1; echo "cli erstellen rc=$?" >>"$L/ablauf.log"
pruefe cli-erstellen "[ -s '$T/cli.tar.gz' ]"
"$BIN" --entpacken-nach "$T/aus" "$T/normal.zip" >>"$L/cli.log" 2>&1 &
CLI=$!
schritt cli-konflikt "wait 2500"
q "key Escape" "wait 1500"
wait $CLI; echo "cli nach (Konflikt abgebrochen) rc=$?" >>"$L/ablauf.log"
# Ende: Archivfenster nach vorn (Aktivierung ohne Datei), Strg+Q beendet
"$BIN" >>"$L/app2.log" 2>&1
q "wait 1000"
shot ende
q "chord ctrl q"
T1=$(date +%s%N)
for i in $(seq 1 100); do kill -0 "$PID" 2>/dev/null || break; sleep 0.1; done
echo "Strg+Q bis Prozessende: $(( ($(date +%s%N) - T1) / 1000000 )) ms" >>"$L/ablauf.log"
if kill -0 "$PID" 2>/dev/null; then
    echo "ENDE Strg+Q: läuft noch" >"$L/status.txt"
else
    wait "$PID"; echo "ENDE Strg+Q: beendet, Exitcode $?" >"$L/status.txt"
fi
echo "katalog: ${MO:-keiner}" >>"$L/status.txt"
echo "vorschau-reste: $(find "$QA_PROFILE/cache/querencia-archiv" -mindepth 1 2>/dev/null | wc -l)" >>"$L/status.txt"
grep -c '^FAIL' "$L/ablauf.log" | sed 's/^/FAIL: /' >>"$L/status.txt"
cat "$L/status.txt" >>"$L/ablauf.log"
touch "$L/FERTIG"
