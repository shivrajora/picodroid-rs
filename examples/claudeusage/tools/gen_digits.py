#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Render the big-numeral sprites in ../assets.

The SDK renders one font size, so the large percentages are composed from
pre-rendered glyph images (see ui/BigNumber.java). PAPK assets are RGB565 with
no alpha, so each glyph is drawn straight onto the card colour it will sit on:
keep CARD in step with Palette.CARD.

    python3 tools/gen_digits.py            # needs Pillow and the Ubuntu font
"""
import os
import sys

from PIL import Image, ImageDraw, ImageFont

CARD = (0x1C, 0x1A, 0x18)
INK = (0xF0, 0xEE, 0xE6)
FONT = "/usr/share/fonts/truetype/ubuntu/Ubuntu[wdth,wght].ttf"
HEIGHT = 44
PX = 46
GLYPHS = {**{str(d): "d%d" % d for d in range(10)}, "%": "pct", "-": "dash", "+": "plus"}
SCALE = 4  # supersample, then downscale: smoother edges than FreeType at size


def main():
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
    font = ImageFont.truetype(FONT, PX * SCALE)
    try:
        font.set_variation_by_axes([100, 700])
    except OSError:
        print("warning: font variations unavailable, using the default weight", file=sys.stderr)
    digit_w = max(font.getbbox(str(d))[2] - font.getbbox(str(d))[0] for d in range(10))
    top = min(font.getbbox(str(d))[1] for d in range(10))
    bottom = max(font.getbbox(str(d))[3] for d in range(10))
    for ch, name in GLYPHS.items():
        box = font.getbbox(ch)
        w = digit_w if ch.isdigit() else box[2] - box[0]
        cell_w = w + 2 * SCALE
        img = Image.new("RGB", (cell_w, HEIGHT * SCALE), CARD)
        x = (cell_w - (box[2] - box[0])) // 2 - box[0]
        y = (HEIGHT * SCALE - (bottom - top)) // 2 - top
        ImageDraw.Draw(img).text((x, y), ch, font=font, fill=INK)
        img = img.resize((max(1, cell_w // SCALE), HEIGHT), Image.LANCZOS)
        img.save(os.path.join(out, name + ".png"))
        print("%-9s %dx%d" % (name + ".png", img.width, img.height))


if __name__ == "__main__":
    main()
