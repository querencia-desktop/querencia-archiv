#!/bin/bash
# Archiv 0.1.0: Speicher und Zeit mit 1-GB-Archiven (QA_GROSS=1024 legt
# gross.zip und gross.tar.gz aus 64 schlecht komprimierbaren Dateien an),
# isoliert wie szenario.sh. Zeiten aus dem Diagnoseprotokoll
# (QUERENCIA_ARCHIV_DEBUG), Speicher aus /proc/<pid>/status (VmRSS, VmHWM).
# Dazu Abbruch mitten im Entpacken (keine halben Dateien) und die
# Kommandozeile unter /usr/bin/time -v. Ergebnis: $OUT/messung.txt.
set -u
. "$QA_PROFILE/env.sh"
Q=$W/bin/qeingabe; QA=$W/bin/qausgabe
L=$QA_PROFILE/logs
M=$OUT/messung.txt
mkdir -p "$OUT"
"$QA" "$RES" >>"$L/szenario.log" 2>&1; sleep 1
B=${RES%x*}; H=${RES#*x}
export GSK_RENDERER=cairo RUST_BACKTRACE=1 QUERENCIA_ARCHIV_DEBUG=1
. "$W/eingabe.sh"
n=0
shot() { n=$((n + 1)); grim "$(printf '%s/%02d-%s-%s-%s.png' "$OUT" "$n" "$1" "$SPR" "$RES")"; }
mem() { echo "$1: $(grep -E '^(VmRSS|VmHWM)' "/proc/$PID/status" | tr -s ' \t' ' ' | tr '\n' ' ')" >>"$M"; }
warte_auf() { local i; for i in $(seq 1 $(($2 * 10))); do grep -q "$1" "$L/app.log" && return 0; sleep 0.1; done; return 1; }
since() { echo "$(echo "$(date +%s.%N) - $1" | bc | cut -c1-6) s"; }
echo "Dateien: $(stat -c '%n %s' "$T/gross.zip" "$T/gross.tar.gz" | tr '\n' ' ')" >"$M"
echo "Auflösung $RES, Sprache $SPR" >>"$M"
q "size $B $H" "wait 300"
for a in gross.zip gross.tar.gz; do
    : >"$L/app.log"
    T0=$(date +%s.%N)
    "$BIN" "$T/$a" >>"$L/app.log" 2>&1 &
    PID=$!
    warte_auf "geöffnet in" 300
    echo "== $a: Start bis Liste (Wanduhr): $(since "$T0"); $(grep 'geöffnet in' "$L/app.log")" >>"$M"
    q "wait 800"; mem "$a nach dem Öffnen"; shot "offen-${a%%.*}-${a#*.}"
    rm -rf "$T/aus-gross"
    T1=$(date +%s.%N)
    q "chord ctrl e" "wait 800" "chord ctrl a" "type $T/aus-gross" "key Return" "wait 3000"
    shot "entpacken-laeuft-${a#*.}"
    warte_auf "entpackt in" 600
    echo "$a: Entpacken (Wanduhr ab Strg+E): $(since "$T1"); $(grep 'entpackt in' "$L/app.log")" >>"$M"
    mem "$a nach dem Entpacken"
    echo "$a: entpackt $(du -sb "$T/aus-gross" | cut -f1) Byte, $(find "$T/aus-gross" -type f | wc -l) Dateien" >>"$M"
    # Abbruch mitten im Entpacken: keine halben Dateien
    rm -rf "$T/aus-abbruch"
    q "chord ctrl e" "wait 800" "chord ctrl a" "type $T/aus-abbruch" "key Return" "wait 4000"
    shot "abbruch-vorher-${a#*.}"
    q "key Escape"
    warte_auf "abgebrochen nach" 60
    q "wait 1000"; shot "abbruch-nachher-${a#*.}"
    echo "$a: Abbruch: $(grep 'abgebrochen nach' "$L/app.log"); Reste: $(find "$T/aus-abbruch" -name '*.qa-teil-*' | wc -l), vollständige Dateien: $(find "$T/aus-abbruch" -type f | wc -l)" >>"$M"
    mem "$a nach dem Abbruch"
    echo "$a Höchststand: $(grep VmHWM "/proc/$PID/status" | tr -s ' \t' ' ')" >>"$M"
    q "chord ctrl q" "wait 2000"
    kill -0 "$PID" 2>/dev/null && echo "$a: ENDE läuft noch" >>"$M" || echo "$a: ENDE beendet" >>"$M"
    rm -rf "$T/aus-gross" "$T/aus-abbruch"
done
# Kommandozeile (eigener Prozess, Fenster nur für Fortschritt)
for a in gross.zip gross.tar.gz; do
    rm -rf "$T/cli-gross"
    /usr/bin/time -v "$BIN" --entpacken-nach "$T/cli-gross" "$T/$a" 2>"$L/time-${a#*.}.txt"
    echo "CLI $a: rc=$? $(grep -E 'Elapsed|Maximum resident' "$L/time-${a#*.}.txt" | tr -s ' \t' ' ' | tr '\n' ';')" >>"$M"
    rm -rf "$T/cli-gross"
done
echo "fertig" >"$L/status.txt"
touch "$L/FERTIG"
