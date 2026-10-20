#!/usr/bin/env python3
"""Programmsymbol: rendert data/icons/hicolor/scalable/apps/org.querencia.Archiv.svg
in die PNG-Stufen 16, 22, 24, 32, 64, 128, 256 (NIE 48x48, GTK-4.22-Symbolabsturz)
nach data/icons/hicolor/<N>x<N>/apps/ – Verfahren wie querencia-symbole
(werkzeug/rendern.py, G2-V): offline über GdkPixbuf (SVG-Lader librsvg),
leere tEXt-Felder, damit die PNGs reproduzierbar sind.
Aufruf (im Repo-Wurzelverzeichnis): python3 -I werkzeug/symbol-rendern.py"""
import os
import sys

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

NAME = "org.querencia.Archiv"
STUFEN = (16, 22, 24, 32, 64, 128, 256)


def main() -> int:
    basis = os.path.join("data", "icons", "hicolor")
    svg = os.path.join(basis, "scalable", "apps", NAME + ".svg")
    for n in STUFEN:
        assert n != 48
        ordner = os.path.join(basis, f"{n}x{n}", "apps")
        os.makedirs(ordner, exist_ok=True)
        bild = GdkPixbuf.Pixbuf.new_from_file_at_scale(svg, n, n, True)
        if (bild.get_width(), bild.get_height()) != (n, n):
            print(f"Stufe {n}: {bild.get_width()}x{bild.get_height()}", file=sys.stderr)
            return 1
        bild.savev(os.path.join(ordner, NAME + ".png"), "png", [], [])
    print(NAME, "Stufen", " ".join(map(str, STUFEN)), "+ scalable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
