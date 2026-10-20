#!/bin/bash
# Archiv 0.1.0: Wegwerfprofil für einen isolierten Lauf (headless labwc).
# Kopiert lesend: labwc rc.xml/environment der Sitzung (Runtime-Repo), hicolor
# des Benutzers (ohne 48x48) plus das Programmsymbol aus data/icons (wie es
# das Integrationsskript ablegt); Thema Querencia-Klassisch und Symbolthema
# Querencia als Link. Legt eine Ablage-Kopie unter
# <profil>/opt/archiv/<sha16>/ an (Binary und share/locale wie die spätere
# Live-Ablage) und den Link <profil>/opt/bin/querencia-archiv darauf.
# Testarchive (examples/testarchive.rs, vorab gebaut: werkzeug/bin/testarchive)
# liegen unter <profil>/home/Archive; „Vorschau“ öffnet Textdateien mit
# einem Prüfstarter, der nur protokolliert (kein fremdes Programm im Lauf).
# gvfs ist im Wegwerf-Bus nicht aktivierbar (wsdd-Falle).
#   profil.sh <name> [ablage-quelle]   → gibt den Profilpfad aus
set -euo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
P=$R/gui-test/${1:?name}
Q=${2:-$R/integration/vorlage}
case $P in "$R"/gui-test/?*) ;; *) echo "Profil nur unter gui-test/" >&2; exit 2 ;; esac
[ -e "$P" ] && { chmod -R u+rwX "$P"; rm -rf -- "$P"; }
mkdir -p "$P"/{config/labwc,data/themes,data/icons,data/applications,state,cache,logs,opt/bin,opt/archiv,home/Archive}
L=${QA_LABWC_CONFIG:-$HOME/projects/querencia-runtime/session/querencia/labwc}
cp "$L/rc.xml" "$L/environment" "$P/config/labwc/"
ln -s "$HOME/.local/share/themes/Querencia-Klassisch" "$P/data/themes/Querencia-Klassisch"
cp -a "$HOME/.local/share/icons/hicolor" "$P/data/icons/"
[ -d "$HOME/.local/share/icons/Querencia" ] && ln -s "$HOME/.local/share/icons/Querencia" "$P/data/icons/Querencia"
for d in "$R"/data/icons/hicolor/*/apps; do
    mkdir -p "$P/data/icons/hicolor/${d#"$R"/data/icons/hicolor/}"
    cp "$d"/org.querencia.Archiv.* "$P/data/icons/hicolor/${d#"$R"/data/icons/hicolor/}/"
done
if find "$P/data/icons/" -path '*48x48*' -name 'org.querencia.*' | grep -q .; then
    echo "48x48-Programmsymbol im Profil – Abbruch (GTK-4.22-Symbolabsturz)" >&2; exit 2
fi
S=$(sha256sum "$Q/querencia-archiv" | cut -c1-16)
A=$P/opt/archiv/$S
mkdir -p "$A/share/locale/de/LC_MESSAGES"
cp "$Q/querencia-archiv" "$A/"
cp "$Q/share/locale/de/LC_MESSAGES/querencia-archiv.mo" "$A/share/locale/de/LC_MESSAGES/"
chmod -R a-w "$A"
ln -s "$A/querencia-archiv" "$P/opt/bin/querencia-archiv"
"$R/werkzeug/bin/testarchive" "$P/home/Archive" ${QA_GROSS:+--gross "$QA_GROSS"} >/dev/null
# Vorschau-Prüfstarter: schreibt nur auf, was geöffnet werden sollte
printf '#!/bin/sh\necho "vorschau: $1" >>"%s/logs/vorschau.log"\nls -l "$1" >>"%s/logs/vorschau.log"\n' "$P" "$P" >"$P/vorschau-pruefung.sh"
cat >"$P/data/applications/qa-vorschau-pruefung.desktop" <<X
[Desktop Entry]
Type=Application
Name=Vorschau-Prüfung
Exec=/bin/sh $P/vorschau-pruefung.sh %f
MimeType=text/plain;text/markdown;
NoDisplay=true
X
printf '[Default Applications]\ntext/plain=qa-vorschau-pruefung.desktop\ntext/markdown=qa-vorschau-pruefung.desktop\n' >"$P/config/mimeapps.list"
mkdir -p "$P/data/dbus-1/services"
for n in org.gtk.vfs.Daemon org.gtk.vfs.Metadata org.gtk.vfs.UDisks2VolumeMonitor org.gtk.vfs.GoaVolumeMonitor org.gtk.vfs.GPhoto2VolumeMonitor org.gtk.vfs.MTPVolumeMonitor org.gtk.vfs.AfcVolumeMonitor; do
    printf '[D-BUS Service]\nName=%s\nExec=/bin/false\n' "$n" >"$P/data/dbus-1/services/$n.service"
done
echo "$P"
