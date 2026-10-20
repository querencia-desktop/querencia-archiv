#!/bin/bash
# Querencia Archiv: fmt, Clippy, Tests und Release-Build. Übersetzt wird auf
# einem der gemeinsamen Bau-Plätze (~/.local/state/querencia/bau-platz.sh:
# nice 10, 4 Jobs; Regel der Startsession seit 20.10.2026) – offline, eigener
# TMPDIR; die Tests selbst laufen danach außerhalb des Bau-Platzes (Regel des
# Auftrags Archiv 0.1.0: Bau-Plätze nicht mit Testläufen belegen).
# Der Release-Build läuft mit --remap-path-prefix (Repo → /usr/src/…,
# Cargo-Home → /usr/share/cargo, wie querencia-packaging), damit keine
# Home-Pfade im Binary stehen; geprüft wird das am Ende mit strings.
#   bauen.sh            alles
#   bauen.sh pruefen    nur fmt, Clippy, Tests
#   bauen.sh release    nur Release-Build
set -uo pipefail
ART=${1:-alles}
R=$(cd "$(dirname "$0")/.." && pwd)
L=$R/nachweis/bau; mkdir -p "$L"; LOG=$L/$ART.log
PLATZ=$HOME/.local/state/querencia/bau-platz.sh
CH=${CARGO_HOME:-$HOME/.cargo}
mkdir -p "$R/tmp"
export TMPDIR=$R/tmp ART CH R
: >"$LOG"
df -h / | tail -1 | tee -a "$LOG"
echo "== $ART $(git -C "$R" log -1 --format='%h %s' 2>/dev/null) (Arbeitsbaum: $(git -C "$R" status --porcelain | wc -l) Änderungen): warte auf einen Bau-Platz $(date +%T)" | tee -a "$LOG"
(cd "$R" && bash "$PLATZ" bash -c '
    echo "Bau-Platz erhalten $(date +%T)"
    set -x
    if [ "$ART" != release ]; then
      cargo fmt --all --check &&
      env -u WAYLAND_DISPLAY -u DISPLAY DBUS_SESSION_BUS_ADDRESS=disabled: cargo clippy --all-targets --offline --locked -- -D warnings &&
      env -u WAYLAND_DISPLAY -u DISPLAY DBUS_SESSION_BUS_ADDRESS=disabled: cargo test --offline --locked --no-run || exit 1
    fi
    if [ "$ART" != pruefen ]; then
      env -u WAYLAND_DISPLAY -u DISPLAY \
        RUSTFLAGS="--remap-path-prefix=$R=/usr/src/querencia-archiv --remap-path-prefix=$CH=/usr/share/cargo" \
        CARGO_TARGET_DIR="$R/target/remap" \
        cargo build --release --offline --locked || exit 1
    fi
') >>"$LOG" 2>&1
RC=$?
if [ "$RC" = 0 ] && [ "$ART" != release ]; then
    echo "== Tests (ohne Bau-Platz) $(date +%T)" | tee -a "$LOG"
    (cd "$R" && bash werkzeug/ohne-bildschirm.sh cargo test --offline --locked) >>"$LOG" 2>&1
    RC=$?
fi
echo "== $ART: fertig rc=$RC $(date +%T)" | tee -a "$LOG"
B=$R/target/remap/release/querencia-archiv
if [ "$ART" != pruefen ] && [ -f "$B" ]; then
    sha256sum "$B" | tee "$L/release-sha256.txt"
    echo "Home-Pfade im Binary: $(strings -a "$B" | grep -c "$HOME")" | tee -a "$LOG"
fi
df -h / | tail -1 | tee -a "$LOG"
exit $RC
