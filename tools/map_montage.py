"""Lay the explore worlds' overhead maps side by side at one scale.

Each map in shots/world-maps/NAME.png is drawn at 1 px per metre, so the tiles
are resized by the same factor and keep their relative sizes. Usage:

    python -I tools/map_montage.py OUT.png NAME [NAME ...]
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
SCALE = 0.5  # output px per metre
GAP = 40
LABEL = 44
MAX_ROW_M = 5200  # wrap a row after this many metres


def font(size):
    for f in ("arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def main():
    out, names = sys.argv[1], sys.argv[2:]
    tiles = []
    for n in names:
        size = json.loads((ROOT / "worlds" / n / "world.json").read_text(encoding="utf-8"))["size_m"]
        px = round(size * SCALE)
        im = Image.open(ROOT / "shots" / "world-maps" / f"{n}.png").convert("RGB").resize((px, px), Image.LANCZOS)
        tiles.append((n, size, im))
    tiles.sort(key=lambda t: -t[1])

    rows, row, w = [], [], 0
    for t in tiles:
        if row and w + t[1] > MAX_ROW_M:
            rows.append(row)
            row, w = [], 0
        row.append(t)
        w += t[1]
    rows.append(row)

    width = max(sum(t[2].width for t in r) + GAP * (len(r) + 1) for r in rows)
    bar_h = 90
    height = sum(max(t[2].height for t in r) + LABEL + GAP for r in rows) + GAP + bar_h
    canvas = Image.new("RGB", (width, height), (24, 26, 30))
    d = ImageDraw.Draw(canvas)
    f, fs = font(30), font(22)
    y = GAP
    for r in rows:
        x = GAP
        for n, size, im in r:
            d.text((x, y), n, fill=(235, 235, 235), font=f)
            d.text((x + d.textlength(n, font=f) + 14, y + 7), f"{size:g} m", fill=(150, 155, 165), font=fs)
            canvas.paste(im, (x, y + LABEL))
            x += im.width + GAP
        y += max(t[2].height for t in r) + LABEL + GAP

    # Scale bar: 1 km.
    km = round(1000 * SCALE)
    d.rectangle((GAP, y + 20, GAP + km, y + 32), fill=(235, 235, 235))
    d.text((GAP + km + 14, y + 12), "1 km", fill=(235, 235, 235), font=f)
    canvas.save(out)
    print(out, canvas.size)


if __name__ == "__main__":
    main()
