#!/bin/bash
# Archiv 0.1.0: Ablage-Quelle für GUI-Läufe vor der Vorlage – Binary und
# share/locale/de/LC_MESSAGES/querencia-archiv.mo (aus po/de.po) in einem
# Ordner, genau wie später die Live-Ablage.
#   quelle.sh <binary> <ziel>
set -euo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
B=${1:?binary}; Z=${2:?ziel}
case $Z in "$R"/gui-test/?*) ;; *) echo "Ziel nur unter gui-test/" >&2; exit 2 ;; esac
[ -e "$Z" ] && { chmod -R u+rwX "$Z"; rm -rf -- "$Z"; }
mkdir -p "$Z"
cp "$B" "$Z/querencia-archiv"
(cd "$R" && python3 -I werkzeug/i18n.py mo "$Z/share/locale" >/dev/null)
ls "$Z" "$Z/share/locale/de/LC_MESSAGES"
