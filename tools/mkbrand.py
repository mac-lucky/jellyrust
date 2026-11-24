#!/usr/bin/env python3
"""Render the JellyRust brand masters, `assets/logo-master.png` and `assets/splash-master.png`,
from the vector source `assets/logo.svg`.

    python3 tools/mkbrand.py
    python3 tools/mkicons.py assets/logo-master.png --splash=assets/splash-master.png

then the badged DEV and NIGHTLY sets (`docs/distribution.md`). The mark is a jellyfish whose bell
carries a rust play triangle, in the accent-to-lavender gradient; `assets/icons/brand-mark.svg` is
its one-colour twin, tinted by the sign-in pane, so change the two together. The wordmark is set in
the app's own Poppins SemiBold, so the splash and the UI share one typeface. Needs `rsvg-convert`
(librsvg) on PATH.
"""
import io
import pathlib
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
SVG = ROOT / "assets/logo.svg"

# Flat panel, and appinfo.json's iconColor: mkicons.py samples the master's corner for that field
# and measures "ink" as anything off-panel, so a glow here would read as logo.
BG = (9, 8, 16)
INK = (244, 242, 250)
RUST = (200, 100, 59)  # the play triangle's colour in assets/logo.svg


def mark(side):
    """The colour mark on transparency, `side` px square, rasterized by librsvg."""
    png = subprocess.run(["rsvg-convert", "-w", str(side), "-h", str(side), str(SVG)],
                         check=True, capture_output=True).stdout
    return Image.open(io.BytesIO(png)).convert("RGBA")


def logo_master(path, side=1254):
    img = Image.new("RGBA", (side, side), BG + (255,))
    img.alpha_composite(mark(side))
    img.convert("RGB").save(path)


def splash_master(path, w=1920, h=1080):
    img = Image.new("RGBA", (w, h), BG + (255,))
    m = mark(400)
    m = m.crop(m.getbbox())
    font = ImageFont.truetype(str(ROOT / "pkg/appfont-bold.ttf"), 104)
    d = ImageDraw.Draw(img)
    # "Jelly" in the UI's ink, "Rust" in the mark's one warm colour.
    a, b = "Jelly", "Rust"
    wa, wb = d.textlength(a, font=font), d.textlength(b, font=font)
    x0, y0, x1, y1 = d.textbbox((0, 0), a + b, font=font)
    gap = 64
    top = round((h - (m.height + gap + (y1 - y0))) / 2)
    img.alpha_composite(m, ((w - m.width) // 2, top))
    tx = round((w - (wa + wb)) / 2)
    ty = top + m.height + gap - y0
    d.text((tx, ty), a, font=font, fill=INK)
    d.text((tx + wa, ty), b, font=font, fill=RUST)
    img.convert("RGB").save(path)


def main():
    out = ROOT / "assets"
    logo_master(out / "logo-master.png")
    splash_master(out / "splash-master.png")
    print("wrote", out / "logo-master.png", "and", out / "splash-master.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
