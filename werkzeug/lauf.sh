#!/bin/bash
# Archiv 0.1.0: ein isolierter Lauf.
#   lauf.sh <de|en> <BxH> [name] [ablage-quelle]
# de = Sitzung wie live (LANG=en_US.UTF-8, LANGUAGE=de), en = LANGUAGE=en.
# Gestartet wird der Link <profil>/opt/bin/querencia-archiv (Ablage-Kopie
# mit share/locale). SZENARIO=<datei> wählt das Szenario (Standard
# szenario.sh), ZEIT=<s> das Zeitlimit, QA_GROSS=<MB> legt zusätzlich die
# großen Testarchive an (messung.sh).
set -euo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
SPR=$1; RES=$2; NAME=${3:-$SPR-$RES}
P=$(bash "$R/werkzeug/profil.sh" "$NAME" "${4:-$R/integration/vorlage}")
OUT=$R/nachweis/gui/$NAME
rm -rf -- "$OUT"; mkdir -p "$OUT"
cat >"$P/env.sh" <<X
BIN=${BIN:-$P/opt/bin/querencia-archiv}
RES=$RES
OUT=$OUT
SPR=$SPR
W=$R/werkzeug
T=$P/home/Archive
export GIO_USE_VFS=local
export LANG=en_US.UTF-8
export LANGUAGE=$SPR
unset LC_ALL LC_MESSAGES
X
python3 -I "$R/werkzeug/isoliert.py" "$P" "$R/werkzeug/${SZENARIO:-szenario.sh}" "${ZEIT:-600}" || true
cp "$P"/logs/*.log "$P"/logs/*.txt "$OUT/" 2>/dev/null || true
cat "$OUT/status.txt" 2>/dev/null || echo "kein status.txt"
