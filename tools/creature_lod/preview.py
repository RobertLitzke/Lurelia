"""Decimation preview of one library creature, at its own voxel size.

python -I tools/creature_lod/preview.py NAME [OUT.png]

Columns: the voxel model, the baked 1/2 decimated mesh (near), the baked 1/4
(far). Rows: three-quarter, side, head close-up, then 64 px and 32 px long at
real size (magnified 2.5x). Prints the triangle counts. Reads the library
from the checkout, so it shows the entry as it is on disk now.
"""
import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from meshlab import *
from sheets import sheet, head_fit, V34, SIDE, OUT
from PIL import Image


def bake(name, vph=None):
    g, pal, vox, d = build(name, vph=vph)
    sq = mesh_square(g)
    V, Q, C, D = mesh_sloped(g)
    T, TC = quads_to_tris(V, Q, C, D)
    base = 2 * len(sq)
    half, quarter = bake_levels(V, T, TC, base)
    return g, pal, vox, d, sq, half, quarter


def main(name, out):
    t0 = time.time()
    g, pal, vox, d, sq, half, quarter = bake(name)
    cols = [(scene_square(sq, pal), g), (scene_smooth(*half, pal), g), (scene_smooth(*quarter, pal), g)]
    labels = [f"Voxel\n{len(sq):,} quads = {2*len(sq):,} tris",
              f"Decimated 1/2 (near)\n{len(half[1]):,} tris",
              f"Decimated 1/4 (far)\n{len(quarter[1]):,} tris"]
    _, fit34 = render(cols[0][0], g, 10, 10, view=V34, ss=1, unit=vox)
    _, fits = render(cols[0][0], g, 10, 10, view=SIDE, ss=1, unit=vox)
    hf = head_fit(g, vox)
    rows = []
    for view, fit in ((V34, fit34), (SIDE, fits), (V34, hf)):
        rows.append([Image.fromarray(render(s, gg, 360, 300, view=view, fit=fit, ss=3, unit=vox)[0]) for s, gg in cols])
    pts = np.argwhere(g > 0)
    L = (pts.max(0) - pts.min(0) + 1).max() * vox
    for px in (64, 32):
        rows.append([Image.fromarray(render(s, gg, 144, 120, view=V34, fit=(fit34[0], 1.0), ss=4, unit=vox, ppm=px / L)[0]).resize((360, 300), Image.NEAREST) for s, gg in cols])
    sheet(rows, labels, f"{name}: {vox*1000:.1f} mm voxels, {d.get('height', 0)/vox:.0f} per height, {L/vox:.0f} along its longest side",
          out, row_labels=["3/4", "side", "head", "64 px\n(2.5x)", "32 px\n(2.5x)"])
    print(f"{name}: voxel {2*len(sq)} tris, half {len(half[1])}, quarter {len(quarter[1])}, {time.time()-t0:.0f} s")


if __name__ == "__main__":
    n = sys.argv[1]
    main(n, sys.argv[2] if len(sys.argv) > 2 else os.path.join(OUT, f"p_{n}.png"))
