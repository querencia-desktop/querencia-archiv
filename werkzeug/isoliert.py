#!/usr/bin/env python3
"""Querencia Archiv 0.1.0 (nach Querencia Dokumente 0.1.0, Bilder 0.2.0, Rechner G2-H, HMP-04/QE-01): isolierte GUI-Testumgebung ohne Berührung der laufenden Sitzung.

Startet einen eigenen labwc 0.9.8 mit headless-Backend (kein Bildschirm, keine
Eingabegeräte) in einem eigenen XDG_RUNTIME_DIR, mit eigenem D-Bus
(dbus-run-session) und eigenem XDG-Profil (Konfiguration, Daten, Cache, State).
Wie bei Dokumente ist auch HOME umgelenkt (<profil>/home): Entpacken,
Vorschau und Testarchive schreiben so nur ins Profil.
Pfade kommen aus $HOME bzw. Parametern; den labwc
wählt QA_LABWC (Standard $HOME/opt/labwc-0.9.8/bin/labwc). Beendet wird ausschließlich der selbst gestartete Prozessbaum
(eigene Prozessgruppe), nie der labwc der Sitzung.

Aufruf:  isoliert.py <profil> <skript>
<skript> ist ein Shell-Skript, das in der isolierten Umgebung läuft
(WAYLAND_DISPLAY gesetzt) und dessen Ausgabe protokolliert wird.
"""

import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time

REAL_HOME = os.path.expanduser("~")
LABWC = os.environ.get("QA_LABWC", os.path.join(REAL_HOME, "opt/labwc-0.9.8/bin/labwc"))
def _session_labwc(pid: str) -> bool:
    """labwc der echten Sitzung (XDG_RUNTIME_DIR unter /run/user), nicht die
    isolierten Testsitzungen anderer Worker, die jederzeit enden dürfen."""
    try:
        if os.readlink(f"/proc/{pid}/exe") != LABWC:
            return False
        with open(f"/proc/{pid}/environ", "rb") as f:
            env = f.read().split(b"\0")
        return any(e.startswith(b"XDG_RUNTIME_DIR=/run/user/") for e in env)
    except OSError:
        return False


SESSION_LABWC_PIDS = {int(p) for p in os.listdir("/proc") if p.isdigit() and _session_labwc(p)}


def main() -> int:
    profile = os.path.abspath(sys.argv[1])
    script = os.path.abspath(sys.argv[2])
    timeout = int(sys.argv[3]) if len(sys.argv) > 3 else 120
    for sub in ("config/labwc", "data", "cache", "state", "logs"):
        os.makedirs(os.path.join(profile, sub), exist_ok=True)
    rc = os.path.join(profile, "config/labwc/rc.xml")
    if not os.path.exists(rc):
        with open(rc, "w") as f:
            # Wie live: Serverdekoration, kein eigenes Theme (labwc-Standard).
            f.write('<?xml version="1.0"?>\n<labwc_config>\n'
                    '  <core><decoration>server</decoration></core>\n'
                    '</labwc_config>\n')
    # PT-1 (nach SW-6, QD-037): keine Portale im Wegwerf-Bus. Sonst startet
    # xdg-document-portal und sein FUSE-Mount <laufzeit>/doc erscheint im echten
    # Querencia Files unter „Geräte“. $XDG_DATA_HOME/dbus-1/services hat beim
    # Sitzungsbus Vorrang vor /usr/share; die Einträge hier machen die Portale
    # nicht aktivierbar.
    masken = os.path.join(profile, "data", "dbus-1", "services")
    os.makedirs(masken, exist_ok=True)
    for name in ("org.freedesktop.portal.Documents", "org.freedesktop.portal.Desktop",
                 "org.freedesktop.impl.portal.PermissionStore"):
        with open(os.path.join(masken, name + ".service"), "w") as f:
            f.write(f"[D-BUS Service]\nName={name}\nExec=/bin/false\n")
    # PT-1: Ohne Settings-Portal nimmt GTK seine eingebaute Schrift (kleiner
    # als live, verschiebt Layouts und Klickziele). Die Schrift der echten
    # Sitzung (nur gelesen) kommt deshalb ins Profil, außer es hat eine eigene.
    # Wie Dokumente: ebenso GTK-Thema und Symbolthema der Sitzung (nur gelesen),
    # damit die Belegbilder Querencia-Klassisch zeigen.
    gtk4 = os.path.join(profile, "config", "gtk-4.0", "settings.ini")
    if not os.path.exists(gtk4):
        werte = {}
        for schluessel, name in (("font-name", "gtk-font-name"), ("gtk-theme", "gtk-theme-name"),
                                 ("icon-theme", "gtk-icon-theme-name")):
            try:
                v = subprocess.run(["gsettings", "get", "org.gnome.desktop.interface", schluessel],
                                   capture_output=True, text=True, timeout=5).stdout.strip().strip("'")
            except (OSError, subprocess.TimeoutExpired):
                v = ""
            if v:
                werte[name] = v
        if werte:
            os.makedirs(os.path.dirname(gtk4), exist_ok=True)
            with open(gtk4, "w") as f:
                f.write("[Settings]\n" + "".join(f"{k}={v}\n" for k, v in werte.items()))
    laufzeit = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "laufzeit")
    os.makedirs(laufzeit, exist_ok=True)
    runtime = tempfile.mkdtemp(prefix="qd-runtime-", dir=laufzeit)
    os.chmod(runtime, 0o700)

    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("WAYLAND", "DBUS", "DISPLAY", "XDG_", "LABWC", "SWAYSOCK"))}
    env.update({
        "HOME": os.path.join(profile, "home"),
        "XDG_RUNTIME_DIR": runtime,
        "XDG_CONFIG_HOME": os.path.join(profile, "config"),
        "XDG_DATA_HOME": os.path.join(profile, "data"),
        "XDG_CACHE_HOME": os.path.join(profile, "cache"),
        "XDG_STATE_HOME": os.path.join(profile, "state"),
        "XDG_DATA_DIRS": "/usr/share",
        "XDG_CURRENT_DESKTOP": "X-Querencia",
        "XDG_SESSION_TYPE": "wayland",
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "pixman",
        "WLR_HEADLESS_OUTPUTS": "1",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "XKB_DEFAULT_LAYOUT": "de",
        "GDK_DEBUG": "no-portals",
        "GTK_USE_PORTAL": "0",
        "QA_PROFILE": profile,
    })
    # Benutzerordner wie auf der VM, aber unter dem umgelenkten HOME.
    real_dirs = os.path.join(REAL_HOME, ".config/user-dirs.dirs")
    own_dirs = os.path.join(profile, "config/user-dirs.dirs")
    os.makedirs(os.path.join(profile, "home"), exist_ok=True)
    if os.path.exists(real_dirs) and not os.path.exists(own_dirs):
        shutil.copy2(real_dirs, own_dirs)
    if os.path.exists(own_dirs):
        with open(own_dirs) as f:
            for z in f:
                if z.startswith("XDG_") and '"$HOME/' in z:
                    os.makedirs(os.path.join(profile, "home", z.split('"$HOME/', 1)[1].split('"')[0]), exist_ok=True)

    log = open(os.path.join(profile, "logs", f"lauf-{time.strftime('%Y%m%d-%H%M%S')}.log"), "w")
    cmd = ["dbus-run-session", "--", LABWC, "-C", os.path.join(profile, "config/labwc"),
           "-s", f"/bin/bash {script}"]
    if os.environ.get("QA_LABWC_DEBUG") == "1":
        cmd.insert(3, "-d")
    print("Start:", " ".join(cmd), file=log, flush=True)
    proc = subprocess.Popen(cmd, env=env, stdin=subprocess.DEVNULL, stdout=log,
                            stderr=subprocess.STDOUT, start_new_session=True, cwd=profile)
    marker = os.path.join(profile, "logs", "FERTIG")
    if os.path.exists(marker):
        os.remove(marker)
    deadline = time.time() + timeout
    while time.time() < deadline and proc.poll() is None and not os.path.exists(marker):
        time.sleep(0.5)
    status = "fertig" if os.path.exists(marker) else ("beendet" if proc.poll() is not None else "Zeitlimit")
    # Nur die eigene Prozessgruppe; der labwc der Sitzung bleibt unberührt.
    pgid = os.getpgid(proc.pid) if proc.poll() is None else None
    if pgid is not None:
        assert pgid != os.getpgid(0)
        for pid in SESSION_LABWC_PIDS:
            try:
                assert os.getpgid(pid) != pgid, "Sitzungs-labwc in eigener Gruppe?!"
            except ProcessLookupError:
                pass
        os.killpg(pgid, signal.SIGTERM)
        try:
            proc.wait(10)
        except subprocess.TimeoutExpired:
            os.killpg(pgid, signal.SIGKILL)
            proc.wait(5)
    # Kinder, die die Gruppe verlassen haben (setsid durch GApplication-
    # Starter u. Ä.), werden über ihren Laufzeitordner gefunden und beendet.
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            with open(f"/proc/{p}/environ", "rb") as f:
                if f"XDG_RUNTIME_DIR={runtime}".encode() in f.read().split(b"\0"):
                    os.kill(int(p), signal.SIGTERM)
        except (OSError, ProcessLookupError):
            pass
    time.sleep(1)
    # PT-1: falls doch ein Portal lief, nur den eigenen doc-Mount aushängen.
    # Über /proc/self/mounts, weil os.path.ismount() einen toten FUSE-Mount
    # („Transport endpoint is not connected“) nicht erkennt.
    doc = os.path.join(os.path.realpath(runtime), "doc")
    with open("/proc/self/mounts") as f:
        eigen = any(z.split()[1] == doc for z in f)
    if eigen and subprocess.run(["fusermount3", "-u", doc], stderr=subprocess.DEVNULL).returncode:
        subprocess.run(["fusermount3", "-u", "-z", doc], check=False)
    shutil.rmtree(runtime, ignore_errors=True)
    for pid in SESSION_LABWC_PIDS:
        assert os.path.exists(f"/proc/{pid}"), "Sitzungs-labwc verschwunden!"
    print(f"Status: {status}; Log: {log.name}; Sitzungs-labwc {sorted(SESSION_LABWC_PIDS)} läuft weiter")
    log.close()
    return 0 if status == "fertig" else 1


if __name__ == "__main__":
    sys.exit(main())
