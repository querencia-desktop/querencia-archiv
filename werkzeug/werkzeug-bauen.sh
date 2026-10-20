#!/bin/bash
# Baut qeingabe und qausgabe (nur Testwerkzeuge, nichts wird installiert) nach
# werkzeug/bin/. Protokoll-XML: virtual-keyboard aus diesem Ordner, die
# wlr-Protokolle aus dem Cargo-Quellcache (wayland-protocols-wlr).
set -euo pipefail
HIER="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-$HIER/bin}"
mkdir -p "$OUT"
VP=$(ls -d ~/.cargo/registry/src/*/wayland-protocols-wlr-0.3.*/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml | head -1)
OM=$(dirname "$VP")/wlr-output-management-unstable-v1.xml
for x in "$VP" "$HIER/virtual-keyboard-unstable-v1.xml" "$OM"; do
  b=$(basename "$x" .xml)
  wayland-scanner client-header "$x" "$OUT/$b-client-protocol.h"
  wayland-scanner private-code "$x" "$OUT/$b-protocol.c"
done
gcc -O2 -Wall -Wextra -I"$OUT" -o "$OUT/qeingabe" "$HIER/qeingabe.c" "$OUT"/virtual-*-protocol.c "$OUT"/wlr-virtual-*-protocol.c \
  $(pkg-config --cflags --libs wayland-client xkbcommon)
gcc -O2 -Wall -Wextra -I"$OUT" -o "$OUT/qausgabe" "$HIER/qausgabe.c" "$OUT/wlr-output-management-unstable-v1-protocol.c" \
  $(pkg-config --cflags --libs wayland-client)
echo "gebaut: $OUT/qeingabe $OUT/qausgabe"
