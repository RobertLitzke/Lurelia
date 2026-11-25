"""The two baked levels of many creatures on one sheet.

    python -I tools/creature_lod/lod_sheet.py cell NAME     # bake and render one cell
    python -I tools/creature_lod/lod_sheet.py compose OUT.png TITLE NAME...
    python -I tools/creature_lod/lod_sheet.py voxel NAME    # raw voxel cell
    python -I tools/creature_lod/lod_sheet.py vcompose OUT.png TITLE NAME...

A cell is the 1/2 mesh (near) and the 1/4 mesh (far), three-quarter view, the
same scale, with the triangle counts and the distance the level switches at
(2 px per model voxel at 1080p with a 60 degree view). Cells are kept under
shots/creature_lod/cells/ so the compose step is quick and cells can be
rendered in parallel.
"""
import json, math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from meshlab import *
from sheets import V34, SIDE, OUT
from preview import bake
from PIL import Image, ImageDraw, ImageFont

CELLS = os.path.join(OUT, "cells")
FONT = ImageFont.truetype(r"C:\Windows\Fonts\arial.ttf", 15)
BOLD = ImageFont.truetype(r"C:\Windows\Fonts\arialbd.ttf", 16)
BIG = ImageFont.truetype(r"C:\Windows\Fonts\arialbd.ttf", 24)
PW, PH = 260, 210
VW, VH = 380, 300
PX_PER_M_AT_1M = 1080 / (2 * math.tan(math.radians(30)))


def cell(name):
    os.makedirs(CELLS, exist_ok=True)
    g, pal, vox, d, sq, half, quarter = bake(name)
    s1, s2 = scene_smooth(*half, pal), scene_smooth(*quarter, pal)
    _, fit = render(s1, g, 10, 10, view=V34, ss=1, unit=vox)
    a = Image.fromarray(render(s1, g, PW, PH, view=V34, fit=fit, ss=3, unit=vox)[0])
    b = Image.fromarray(render(s2, g, PW, PH, view=V34, fit=fit, ss=3, unit=vox)[0])
    a.save(os.path.join(CELLS, f"{name}_half.png"))
    b.save(os.path.join(CELLS, f"{name}_quarter.png"))
    switch = PX_PER_M_AT_1M * vox / 2
    meta = {"name": name, "voxel": vox, "square_tris": 2 * len(sq), "half": len(half[1]), "quarter": len(quarter[1]), "switch_m": switch}
    json.dump(meta, open(os.path.join(CELLS, f"{name}.json"), "w"))
    print(meta)


def compose(out, title, names, per_row=4):
    rows = math.ceil(len(names) / per_row)
    lab = 44
    W = per_row * 2 * PW + (per_row - 1) * 12
    H = 50 + rows * (PH + lab + 10)
    o = Image.new("RGB", (W, H), (250, 250, 250))
    d = ImageDraw.Draw(o)
    d.text((10, 10), title, fill=(20, 20, 20), font=BIG)
    for i, n in enumerate(names):
        r, c = divmod(i, per_row)
        x, y = c * (2 * PW + 12), 50 + r * (PH + lab + 10)
        m = json.load(open(os.path.join(CELLS, f"{n}.json")))
        d.text((x + 6, y + 2), n.replace("_", " "), fill=(20, 20, 20), font=BOLD)
        d.text((x + 6, y + 22), f"{m['voxel']*1000:.1f} mm voxels; near {m['half']:,} tris, far {m['quarter']:,} beyond {m['switch_m']:.0f} m", fill=(60, 60, 60), font=FONT)
        o.paste(Image.open(os.path.join(CELLS, f"{n}_half.png")), (x, y + lab))
        o.paste(Image.open(os.path.join(CELLS, f"{n}_quarter.png")), (x + PW, y + lab))
        d.line([(x + PW, y + lab), (x + PW, y + lab + PH)], fill=(225, 225, 225))
    o.save(out)
    print("wrote", out)


def voxel_cell(name):
    """The unsmoothed voxel model, three-quarter and side, for review."""
    os.makedirs(CELLS, exist_ok=True)
    g, pal, vox, d = build(name)
    sq = mesh_square(g)
    s = scene_square(sq, pal)
    for tag, view in (("v34", V34), ("vside", SIDE)):
        _, fit = render(s, g, 10, 10, view=view, ss=1, unit=vox)
        Image.fromarray(render(s, g, VW, VH, view=view, fit=fit, ss=3, unit=vox)[0]).save(os.path.join(CELLS, f"{name}_{tag}.png"))
    pts = np.argwhere(g > 0)
    ext = (pts.max(0) - pts.min(0) + 1).tolist()
    meta = {"name": name, "voxel": vox, "quads": len(sq), "per_height": d.get("height", 0) / vox, "extent": ext}
    json.dump(meta, open(os.path.join(CELLS, f"{name}_vox.json"), "w"))
    print(meta)


def voxel_compose(out, title, names, per_row=3):
    rows = math.ceil(len(names) / per_row)
    lab = 44
    W = per_row * 2 * VW + (per_row - 1) * 12
    H = 50 + rows * (VH + lab + 10)
    o = Image.new("RGB", (W, H), (250, 250, 250))
    d = ImageDraw.Draw(o)
    d.text((10, 10), title, fill=(20, 20, 20), font=BIG)
    for i, n in enumerate(names):
        r, c = divmod(i, per_row)
        x, y = c * (2 * VW + 12), 50 + r * (VH + lab + 10)
        m = json.load(open(os.path.join(CELLS, f"{n}_vox.json")))
        d.text((x + 6, y + 2), n.replace("_", " "), fill=(20, 20, 20), font=BOLD)
        d.text((x + 6, y + 22), f"{m['voxel']*1000:.1f} mm voxels, {m['per_height']:.0f} per height; {m['quads']:,} quads", fill=(60, 60, 60), font=FONT)
        o.paste(Image.open(os.path.join(CELLS, f"{n}_v34.png")), (x, y + lab))
        o.paste(Image.open(os.path.join(CELLS, f"{n}_vside.png")), (x + VW, y + lab))
        d.line([(x + VW, y + lab), (x + VW, y + lab + VH)], fill=(225, 225, 225))
    o.save(out)
    print("wrote", out)


if __name__ == "__main__":
    if sys.argv[1] == "cell":
        cell(sys.argv[2])
    elif sys.argv[1] == "voxel":
        voxel_cell(sys.argv[2])
    elif sys.argv[1] == "vcompose":
        voxel_compose(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        compose(sys.argv[2], sys.argv[3], sys.argv[4:])
