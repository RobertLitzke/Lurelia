import json, os, sys
sys.path.insert(0, os.path.dirname(__file__))
from meshlab import *
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "shots", "creature_lod")
os.makedirs(OUT, exist_ok=True)
FONT = ImageFont.truetype(r"C:\Windows\Fonts\arial.ttf", 17)
SMALL = ImageFont.truetype(r"C:\Windows\Fonts\arial.ttf", 14)
BIG = ImageFont.truetype(r"C:\Windows\Fonts\arialbd.ttf", 22)
V34, SIDE = (55, 15), (90, 4)
counts = {}


def sheet(rows, col_labels, title, path, row_labels=None):
    pw, ph = rows[0][0].width, rows[0][0].height
    lw = 0 if not row_labels else 120
    top = 40 + 46
    W, H = lw + pw * len(col_labels), top + ph * len(rows)
    o = Image.new("RGB", (W, H), (250, 250, 250))
    d = ImageDraw.Draw(o)
    d.text((10, 8), title, fill=(20, 20, 20), font=BIG)
    for c, lab in enumerate(col_labels):
        for k, line in enumerate(lab.split("\n")):
            d.text((lw + c * pw + 8, 44 + k * 20), line, fill=(30, 30, 30), font=FONT if k == 0 else SMALL)
    for r, row in enumerate(rows):
        if row_labels:
            d.text((8, top + r * ph + ph // 2 - 10), row_labels[r], fill=(30, 30, 30), font=FONT)
        for c, im in enumerate(row):
            o.paste(im, (lw + c * pw, top + r * ph))
    o.save(path)
    print("wrote", path)


def head_fit(g, unit):
    pts = np.argwhere(g > 0).astype(float) + 0.5
    zlo, zhi = pts[:, 2].min(), pts[:, 2].max()
    ylo, yhi = pts[:, 1].min(), pts[:, 1].max()
    sel = pts[(pts[:, 2] < zlo + 0.3 * (zhi - zlo)) & (pts[:, 1] > ylo + 0.45 * (yhi - ylo))]
    if len(sel) == 0:
        # No head up front (an octopus, a jelly): look at the top of the body.
        sel = pts[pts[:, 1] > ylo + 0.45 * (yhi - ylo)]
    if len(sel) == 0:
        sel = pts
    c = sel.mean(0) * unit
    return c, 0.32 * np.linalg.norm(pts.max(0) - pts.min(0)) * unit


def part_a(name, engine_square_quads, engine_slope_quads):
    g, pal, vox, d = build(name)
    sq = mesh_square(g)
    V, Q, C, D = mesh_sloped(g)
    T, TC = quads_to_tris(V, Q, C, D)
    base = 2 * engine_square_quads
    mult = [2, 1, 0.5, 0.25] if base < 2000 else [1, 0.5, 0.25, 0.125]
    targets = [int(base * m) for m in mult]
    decs = []
    for t in targets:
        decs.append(decimate(V, T, TC, t))
    scenes = [scene_square(sq, pal), scene_flat(V, T, TC, pal)] + [scene_smooth(*x, pal) for x in decs]
    labels = [f"Square\n{engine_square_quads:,} quads = {2*engine_square_quads:,} tris (engine)",
              f"Sloped\n{engine_slope_quads:,} quads = {2*engine_slope_quads:,} tris (engine)"]
    names = {2: "2x square", 1: "same as square", 0.5: "1/2 of square", 0.25: "1/4 of square", 0.125: "1/8 of square"}
    pct = [names[m] for m in mult]
    for x, p in zip(decs, pct):
        labels.append(f"Decimated\n{len(x[1]):,} tris ({p})")
    counts[name] = {"square_quads": engine_square_quads, "sloped_quads": engine_slope_quads, "decimated_tris": [len(x[1]) for x in decs], "my_greedy_square_quads": len(sq)}
    rows = []
    _, fit34 = render(scenes[0], g, 10, 10, view=V34, ss=1, unit=vox)
    _, fits = render(scenes[0], g, 10, 10, view=SIDE, ss=1, unit=vox)
    hf = head_fit(g, vox)
    for view, fit in ((V34, fit34), (SIDE, fits), (V34, hf)):
        rows.append([Image.fromarray(render(s, g, 400, 360, view=view, fit=fit, ss=3, unit=vox)[0]) for s in scenes])
    sheet(rows, labels, f"{name}: square, sloped and decimated ({d['height']/vox:.0f} voxels per height, {vox*100:.1f} cm voxels)", os.path.join(OUT, f"a_{name}.png"))


def greedy_quads(g):
    return len(mesh_square(g))


def part_b(name):
    g0, pal, vox0, d = build(name)
    g, pal, vox, _ = build(name, vph=25)
    l1, l2 = downsample(g, 2), downsample(g, 4)
    levels = [(g0, vox0, f"Current library\n{d['height']/vox0:.0f} vox/height, {vox0*100:.1f} cm"),
              (g, vox, f"Re-made at 25 vox/height\n{vox*100:.1f} cm voxels"),
              (l1, vox * 2, f"LOD1: 2x2x2 of the 25\n12.5 vox/height"),
              (l2, vox * 4, f"LOD2: 4x4x4 of the 25\n6 vox/height")]
    scenes, labels = [], []
    for gg, u, lab in levels:
        q = mesh_square(gg)
        scenes.append((scene_square(q, pal), gg, u))
        labels.append(lab + f", {len(q):,} quads")
    counts[name + "_lod"] = [len(mesh_square(x[0])) for x in levels]
    _, fit34 = render(scenes[1][0], g, 10, 10, view=V34, ss=1, unit=vox)
    _, fits = render(scenes[1][0], g, 10, 10, view=SIDE, ss=1, unit=vox)
    rows = []
    for view, fit in ((V34, fit34), (SIDE, fits)):
        rows.append([Image.fromarray(render(s, gg, 400, 360, view=view, fit=fit, ss=3, unit=u)[0]) for s, gg, u in scenes])
    # at distance: animal height in pixels, LOD0/1/2, magnified 3x nearest
    ys = np.nonzero((g > 0).any(axis=(0, 2)))[0]
    H = (ys.max() + 1 - ys.min()) * vox
    dist_rows = []
    for px in (96, 48, 24):
        ppm = px / H
        row = []
        for s, gg, u in scenes[1:]:
            im, _ = render(s, gg, 160, 136, view=V34, fit=(fit34[0], 1.0), ss=4, unit=u, ppm=ppm)
            row.append(Image.fromarray(im).resize((400, 340), Image.NEAREST))
        dist_rows.append(row)
    sheet(rows, labels, f"{name}: one resolution rule (25 voxels per body height) and its LOD levels", os.path.join(OUT, f"b_{name}.png"))
    sheet(dist_rows, ["LOD0 (25 vox/height)", "LOD1 (2x2x2)", "LOD2 (4x4x4)"],
          f"{name} at distance: real pixel size, magnified 2.5x (height = whole model, ears and antlers included)", os.path.join(OUT, f"b_{name}_distance.png"),
          row_labels=[f"{p} px tall" for p in (96, 48, 24)])


def part_c(name):
    g, pal, vox, d = build(name, vph=25)
    V, Q, C, D = mesh_sloped(g)
    T, TC = quads_to_tris(V, Q, C, D)
    cols, labels = [], []
    for k in (2, 4):
        lg = downsample(g, k)
        q = mesh_square(lg)
        dec = decimate(V, T, TC, 2 * len(q))
        cols.append((scene_square(q, pal), lg, vox * k))
        labels.append(f"Voxel LOD {k}x{k}x{k}\n{len(q):,} quads = {2*len(q):,} tris")
        cols.append((scene_smooth(*dec, pal), g, vox))
        labels.append(f"Decimated, same budget\n{len(dec[1]):,} tris")
    _, fit34 = render(cols[1][0], g, 10, 10, view=V34, ss=1, unit=vox)
    ys = np.nonzero((g > 0).any(axis=(0, 2)))[0]
    H = (ys.max() + 1 - ys.min()) * vox
    rows = [[Image.fromarray(render(s, gg, 400, 340, view=V34, fit=fit34, ss=3, unit=u)[0]) for s, gg, u in cols]]
    for px in (48, 24):
        rows.append([Image.fromarray(render(s, gg, 160, 136, view=V34, fit=(fit34[0], 1.0), ss=4, unit=u, ppm=px / H)[0]).resize((400, 340), Image.NEAREST) for s, gg, u in cols])
    sheet(rows, labels, f"{name} at 25 vox/height: voxel LOD against a decimated mesh of equal triangle count", os.path.join(OUT, f"c_{name}.png"),
          row_labels=["close up", "48 px tall\n(2.5x)", "24 px tall\n(2.5x)"])


def part_d(name, vph):
    g, pal, vox, d = build(name, vph=vph)
    sq = mesh_square(g)
    l1 = downsample(g, 2)
    sq1 = mesh_square(l1)
    V, Q, C, D = mesh_sloped(g)
    T, TC = quads_to_tris(V, Q, C, D)
    base = 2 * len(sq)
    half, quarter = bake_levels(V, T, TC, base)
    cols = [(scene_square(sq, pal), g, vox), (scene_square(sq1, pal), l1, vox * 2),
            (scene_smooth(*half, pal), g, vox), (scene_smooth(*quarter, pal), g, vox)]
    labels = [f"Voxel, near\n{len(sq):,} quads = {base:,} tris",
              f"Voxel 2x2x2, far\n{len(sq1):,} quads = {2*len(sq1):,} tris",
              f"Decimated 1/2, near\n{len(half[1]):,} tris",
              f"Decimated 1/4, far\n{len(quarter[1]):,} tris"]
    counts[name + f"_{vph}"] = {"voxel": base, "voxel2": 2 * len(sq1), "half": len(half[1]), "quarter": len(quarter[1]), "sloped_tris": len(T), "voxel_m": vox}
    _, fit34 = render(cols[0][0], g, 10, 10, view=V34, ss=1, unit=vox)
    pts = np.argwhere(g > 0)
    H = (pts.max(0) - pts.min(0) + 1).max() * vox
    rows = [[Image.fromarray(render(s, gg, 400, 340, view=V34, fit=fit34, ss=3, unit=u)[0]) for s, gg, u in cols]]
    for px in (96, 48, 24):
        rows.append([Image.fromarray(render(s, gg, 160, 136, view=V34, fit=(fit34[0], 1.0), ss=4, unit=u, ppm=px / H)[0]).resize((400, 340), Image.NEAREST) for s, gg, u in cols])
    sheet(rows, labels, f"{name} at {vph} vox/height ({vox*1000:.1f} mm voxels): voxel mode against decimated mode", os.path.join(OUT, f"d_{name}.png"),
          row_labels=["close up", "96 px long\n(2.5x)", "48 px long\n(2.5x)", "24 px long\n(2.5x)"])


if __name__ == "__main__":
    which = sys.argv[1]
    if which == "d":
        part_d(sys.argv[2], int(sys.argv[3]))
    elif which == "c":
        part_c(sys.argv[2])
    elif which == "a":
        part_a(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]))
    else:
        part_b(sys.argv[2])
    json.dump(counts, open(os.path.join(OUT, f"counts_{which}_{sys.argv[2]}.json"), "w"), indent=1)
