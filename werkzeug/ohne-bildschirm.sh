#!/bin/bash
# Führt einen Befehl garantiert ohne Bildschirm aus: kein WAYLAND_DISPLAY,
# kein DISPLAY und ein eigenes, leeres XDG_RUNTIME_DIR. Nur WAYLAND_DISPLAY
# zu entfernen genügt NICHT – GTK verbindet sich dann mit
# $XDG_RUNTIME_DIR/wayland-0, also mit der laufenden Sitzung (Vorfall
# Archiv 0.1.0, 20.10.2026 15:06).
#   ohne-bildschirm.sh <befehl> …
set -euo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$R/laufzeit"
RT=$(mktemp -d "$R/laufzeit/qa-ohne-XXXXXX")
trap 'rm -rf -- "$RT"' EXIT
chmod 700 "$RT"
env -u WAYLAND_DISPLAY -u DISPLAY -u WAYLAND_SOCKET XDG_RUNTIME_DIR="$RT" \
    DBUS_SESSION_BUS_ADDRESS=disabled: GDK_BACKEND=wayland "$@"
