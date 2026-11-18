"""Bake the HUD's distance-field font atlas.

    python library/fonts/bake_atlas.py

Writes atlas.png (one channel: 0.5 is the glyph edge, brighter is inside) and
atlas.json (metrics) next to this file; src/render/font.rs includes both at
compile time. Rerun after changing a face, weight or the character set.

Faces, all SIL Open Font License (licence texts beside the fonts):
  0 caps        Cinzel, weight 600: menus, headings, every in-game line
  1 caps_bold   Cinzel, weight 700: the selected menu item, headings
  2 italic      Cormorant Garamond Italic, weight 500: notes and sentences
Glyph 0x2726 in face 0 is the logo's four-point star.

Needs Pillow, numpy, scipy and fontTools.
"""
import io
import json
import os

import numpy as np
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage as nd

HERE = os.path.dirname(os.path.abspath(__file__))
EM = 64          # atlas pixels per em
HI = 4           # rasterise this many times larger, then shrink
SPREAD = 8       # distance range each side of the edge, in atlas pixels
PAD = SPREAD + 1
ATLAS_W = 1024
FACES = [
    ('caps', 'Cinzel-VariableFont_wght.ttf', 600),
    ('caps_bold', 'Cinzel-VariableFont_wght.ttf', 700),
    ('italic', 'CormorantGaramond-Italic-VariableFont_wght.ttf', 500),
]
CHARS = [chr(c) for c in range(32, 127)] + list('·‹›–—’×°')
STAR = 0x2726


def instance(path, wght):
    f = TTFont(os.path.join(HERE, path))
    inst = instancer.instantiateVariableFont(f, {'wght': wght})
    buf = io.BytesIO()
    inst.save(buf)
    buf.seek(0)
    return ImageFont.truetype(buf, EM * HI)


def sdf(mask):
    """Hi-res boolean mask -> distance field shrunk by HI, as 0..255."""
    inside = nd.distance_transform_edt(mask)
    outside = nd.distance_transform_edt(~mask)
    d = (inside - outside) / HI  # atlas pixels, positive inside
    h, w = d.shape
    d = d.reshape(h // HI, HI, w // HI, HI).mean(axis=(1, 3))
    return np.clip(0.5 + d / (2 * SPREAD), 0, 1)


def glyph_cell(font, ch):
    """Distance field cell for one character, and its placement."""
    l, t, r, b = font.getbbox(ch, anchor='ls')
    adv = font.getlength(ch) / HI
    if r <= l or b <= t:
        return None, adv, 0, 0
    # Snap the box out to whole atlas pixels, then pad by the spread.
    l, t = (l // HI) * HI, (t // HI) * HI
    r, b = -(-r // HI) * HI, -(-b // HI) * HI
    p = PAD * HI
    w, h = r - l + 2 * p, b - t + 2 * p
    im = Image.new('L', (w, h), 0)
    ImageDraw.Draw(im).text((p - l, p - t), ch, font=font, fill=255, anchor='ls')
    cell = sdf(np.asarray(im) > 127)
    # Offset of the cell's top-left from the pen at the baseline, atlas pixels.
    return cell, adv, (l - p) / HI, (t - p) / HI


def star_cell():
    """The logo's four-point star: concave sides, one em across."""
    n = EM * HI
    pts = []
    def bez(p0, p1, p2, p3):
        for i in range(24):
            t = i / 24
            q = [(1 - t) ** 3 * a + 3 * (1 - t) ** 2 * t * b + 3 * (1 - t) * t * t * c + t ** 3 * d
                 for a, b, c, d in zip(p0, p1, p2, p3)]
            pts.append(tuple(v / 24 * n for v in q))
    bez((12, 0), (12.8, 7.6), (16.4, 11.2), (24, 12))
    bez((24, 12), (16.4, 12.8), (12.8, 16.4), (12, 24))
    bez((12, 24), (11.2, 16.4), (7.6, 12.8), (0, 12))
    bez((0, 12), (7.6, 11.2), (11.2, 7.6), (12, 0))
    p = PAD * HI
    im = Image.new('L', (n + 2 * p, n + 2 * p), 0)
    ImageDraw.Draw(im).polygon([(x + p, y + p) for x, y in pts], fill=255)
    cell = sdf(np.asarray(im) > 127)
    # Centred on the pen: offset is half a cell.
    return cell, EM, -cell.shape[1] / 2, -cell.shape[0] / 2


def main():
    cells = []  # (face, code, cell, adv, ox, oy)
    faces = []
    for fi, (name, path, wght) in enumerate(FACES):
        font = instance(path, wght)
        cap = -font.getbbox('H', anchor='ls')[1] / HI
        xh = -font.getbbox('x', anchor='ls')[1] / HI
        asc, desc = font.getmetrics()
        faces.append({'name': name, 'cap': cap, 'x_height': xh, 'ascent': asc / HI, 'descent': desc / HI, 'glyphs': {}})
        for ch in CHARS:
            cell, adv, ox, oy = glyph_cell(font, ch)
            cells.append((fi, ord(ch), cell, adv, ox, oy))
        if fi == 0:
            cells.append((0, STAR) + star_cell())
    # Shelf-pack the tallest first.
    order = sorted(range(len(cells)), key=lambda i: -(cells[i][2].shape[0] if cells[i][2] is not None else 0))
    x = y = shelf = 0
    place = {}
    for i in order:
        c = cells[i][2]
        if c is None:
            continue
        h, w = c.shape
        if x + w > ATLAS_W:
            x, y, shelf = 0, y + shelf + 1, 0
        place[i] = (x, y)
        x += w + 1
        shelf = max(shelf, h)
    atlas_h = y + shelf + 1
    atlas_h = -(-atlas_h // 64) * 64
    atlas = np.zeros((atlas_h, ATLAS_W), np.float32)
    for i, (fi, code, c, adv, ox, oy) in enumerate(cells):
        g = {'adv': round(adv, 3)}
        if c is not None:
            px, py = place[i]
            h, w = c.shape
            atlas[py:py + h, px:px + w] = c
            g.update({'x': px, 'y': py, 'w': w, 'h': h, 'ox': round(ox, 3), 'oy': round(oy, 3)})
        faces[fi]['glyphs'][str(code)] = g
    Image.fromarray((atlas * 255 + 0.5).astype(np.uint8), 'L').save(os.path.join(HERE, 'atlas.png'), optimize=True)
    with open(os.path.join(HERE, 'atlas.json'), 'w', encoding='utf-8') as f:
        json.dump({'em': EM, 'spread': SPREAD, 'width': ATLAS_W, 'height': atlas_h, 'faces': faces}, f, separators=(',', ':'))
    print(f'atlas {ATLAS_W}x{atlas_h}, {len(cells)} glyphs')


if __name__ == '__main__':
    main()
