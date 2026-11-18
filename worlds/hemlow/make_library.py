"""Writes Hemlow's library entries (materials, plants, structures, creatures,
collectibles, biomes) through libfmt.render so they come out canonical. The
JSON files in library/ are what the game reads. See the note below.

One-shot, not kept current. This ran once, when the world was made; the JSON
in library/ has moved on since (entries renamed or removed, new fields, hand
edits), so rerunning it would overwrite live data with a stale version. It is
kept as a record of how the world was first written. Edit the JSON directly
instead, and if you do rerun this, diff the output before keeping any of it.
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

W = "hemlow"
written = []


def save(kind, d):
    d = {"name": d.pop("name"), "version": 1, "introduced_by": W, **d}
    path = os.path.join(ROOT, "library", kind, d["name"] + ".json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(libfmt.render(d, kind))
    written.append(f"{kind}/{d['name']}")


# ---- materials (ids 60-79) ---------------------------------------------------

def mat(id, name, notes, pattern, base, alt, calm_base, calm_alt, scale=8, contrast=1.0, **kw):
    tex = {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast,
           "calm": {"base": calm_base, "alt": calm_alt}}
    d = {"name": name, "notes": notes, "id": id, "smooth": kw.pop("smooth", False), **kw, "texture": tex}
    save("materials", d)


mat(60, "oolite", "Honey-coloured oolitic limestone in thick beds: the face of Hemlow Edge and the bones of the upland.", "strata",
    [176, 150, 104], [196, 170, 120], [184, 160, 116], [200, 176, 128], scale=2, contrast=0.6, smooth=True)
mat(61, "oolite_cream", "A paler, creamier bed of the oolite that makes a level band along the Edge.", "strata",
    [206, 192, 156], [220, 206, 170], [210, 196, 162], [222, 210, 176], scale=3, contrast=0.5, smooth=True)
mat(62, "cotswold_stone", "Coursed honey limestone walling, dressed at the corners: every building in Hemlow.", "coursed",
    [186, 156, 106], [204, 176, 124], [196, 168, 118], [210, 184, 134], scale=5, contrast=0.8)
mat(63, "stone_tiles", "Stone slate roofing: small grey-brown limestone slates in diminishing courses, crusted with lichen.", "coursed",
    [84, 80, 74], [104, 98, 88], [92, 88, 80], [110, 104, 94], scale=9, contrast=0.9)
mat(64, "wildflower_grass", "Hay-meadow turf thick with flowers: buttercup, scabious, knapweed, ox-eye daisy, poppies (sprites as on prairie grass).", "noise",
    [96, 132, 58], [112, 146, 64], [100, 134, 60], [114, 146, 66], scale=6, contrast=0.6, smooth=True, tint="meadow")
mat(65, "bog_moss", "Sphagnum lawn of the Wisp Moss: wet green moss with rusty-red and ochre patches.", "blotch",
    [104, 126, 60], [146, 104, 62], [108, 128, 64], [146, 110, 70], scale=5, contrast=0.8, smooth=True)
mat(66, "peat", "Black-brown peat under the mere and the bog pools.", "noise",
    [52, 40, 30], [66, 52, 38], [58, 46, 34], [70, 56, 42], scale=6, contrast=0.6, smooth=True)
mat(67, "glowworm_ceiling", "The roof of Lantern Hole: wet stone with scattered blue-green glow-worm lights. v2: grey stone between the lights, and only the lights glow (glow_specks), with threads hanging down.", "noise",
    [92, 96, 98], [80, 84, 86], [96, 100, 102], [84, 88, 90], scale=8, contrast=1.0, glow=1.2, glow_specks=True, version=2)
mat(76, "cave_rock", "Dark wet limestone of Lantern Hole's walls and floor, stained grey-brown and slick.", "strata",
    [52, 50, 48], [64, 60, 56], [54, 52, 50], [66, 62, 58], scale=3, contrast=0.7, smooth=True)
mat(77, "flowstone", "Pale cream calcite of the stalactites and stalagmites, wet and faintly translucent.", "strata",
    [196, 186, 162], [214, 206, 186], [200, 190, 166], [216, 208, 190], scale=6, contrast=0.6)
mat(69, "lamplit_glass", "Leaded window panes with lamplight behind them: Hemlow leaves a lamp in every window on Midsummer Eve.", "glass",
    [236, 180, 96], [250, 210, 130], [236, 182, 100], [250, 212, 134], scale=4, contrast=0.6, glow=0.8)
mat(70, "beech_bark", "Smooth pale grey beech bark with darker mottling.", "blotch",
    [128, 126, 118], [108, 106, 100], [138, 136, 128], [116, 114, 108], scale=5, contrast=0.6)
mat(71, "beech_leaves", "Fresh bright-green beech leaves in flat sprays.", "blotch",
    [92, 136, 52], [110, 150, 60], [96, 136, 56], [112, 148, 62], scale=7, contrast=1.0, tint="leaves")
mat(72, "elder_flower", "Elder in flower: dark green leaves under flat cream plates of blossom.", "blotch",
    [80, 112, 52], [228, 222, 186], [84, 114, 56], [226, 220, 188], scale=7, contrast=1.0)
mat(73, "rose_bush", "Cottage-garden and dog roses: green leaves thick with pink blooms.", "speckle",
    [70, 108, 50], [222, 128, 150], [76, 112, 54], [220, 136, 156], scale=8, contrast=1.0)
mat(74, "reed_green", "Long grey-green leaves and stems of reedmace.", "grain",
    [96, 124, 64], [112, 140, 74], [100, 126, 68], [114, 140, 76], scale=6, contrast=0.6)
mat(75, "reedmace_head", "The velvet brown seed heads of reedmace.", "noise",
    [92, 60, 38], [110, 74, 46], [96, 64, 42], [112, 76, 50], scale=6, contrast=0.6)

# ---- plants -------------------------------------------------------------------

save("plants", {"name": "beech", "notes": "A tall beech of the Hollins and the Edge hangers: smooth grey trunk, a high dome of fresh green.",
                "form": "branching", "height": [17, 24], "trunk_radius": [0.6, 0.85], "buttress": 0.45,
                "crown_radius": [5.0, 6.5], "bark": "beech_bark", "leaf": "beech_leaves", "max_slope": 1.2,
                "spread": 1.0, "autumn": "leaves_autumn"})
save("plants", {"name": "elder", "notes": "Elder in hedges and at the wood's edge, in full cream flower at midsummer.",
                "form": "broad", "height": [3.5, 5.5], "trunk_radius": [0.12, 0.18], "crown_radius": [1.8, 2.6],
                "bark": "bark", "leaf": "elder_flower", "leaf_alt": ["leaves", 0.2], "max_slope": 0.9, "affords": ["nectar"]})
save("plants", {"name": "cottage_rose", "notes": "Roses in the cottage gardens and wild dog rose in the hedges: a pink-flowered shrub.",
                "form": "shrub", "height": [1.2, 2.0], "crown_radius": [0.8, 1.3], "bark": "bark", "leaf": "rose_bush",
                "affords": ["nectar"]})
save("plants", {"name": "reedmace", "notes": "canes: Reedmace (bulrush) in the bog pools and along the Hem: stiff stems topped with brown velvet heads.",
                "form": "canes", "height": [1.6, 2.4], "trunk_radius": [0.08, 0.1], "crown_radius": [0.4, 0.7],
                "bark": "reed_green", "leaf": "reed_green", "stems": [7, 13], "flower": "reedmace_head", "max_slope": 0.6})

# ---- structures ---------------------------------------------------------------
# Voxels are 0.25 m. Front faces local -Z. Buildings stand back from z = 0 by
# a couple of voxels so porches and bays fit inside the size.


def window(x0, y0, w, h, z0, z1, mullion=True):
    """A lit window through a wall with a stone mullion."""
    ops = [{"op": "box", "from": [x0, y0, z0], "to": [x0 + w, y0 + h, z1], "mat": "glass", "part": "glass"}]
    if mullion and w >= 4:
        m = x0 + w // 2
        ops.append({"op": "box", "from": [m, y0, z0], "to": [m + 1, y0 + h, z1], "mat": "wall", "part": "wall"})
    return ops


def side_window(x0, x1, y0, z0, w, h):
    return [{"op": "box", "from": [x0, y0, z0], "to": [x1, y0 + h, z0 + w], "mat": "glass", "part": "glass"}]


def door(x0, w, h, z0, slot="door"):
    return [
        {"op": "box", "from": [x0, 1, z0], "to": [x0 + w, 1 + h, z0 + 2], "mat": "air"},
        {"op": "box", "from": [x0, 1, z0 + 1], "to": [x0 + w, 1 + h, z0 + 2], "mat": slot, "part": "timber"},
        {"op": "box", "from": [x0 - 1, 1 + h, z0], "to": [x0 + w + 1, 2 + h, z0 + 2], "mat": "wall", "part": "wall"},
    ]


def chimney(x0, z0, y0, top, w=3, d=4):
    return [
        {"op": "box", "from": [x0, y0, z0], "to": [x0 + w, top, z0 + d], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [x0, top, z0 + 1], "to": [x0 + 1, top + 2, z0 + 2], "mat": "pot", "part": "stone"},
        {"op": "box", "from": [x0 + w - 1, top, z0 + d - 2], "to": [x0 + w, top + 2, z0 + d - 1], "mat": "pot", "part": "stone"},
    ]


def house_body(x0, x1, z0, z1, wall_h, rise, roof="roof", stone_gables=True):
    """Floor, walls, roof and stone gable ends of a rectangular building with
    its ridge along x."""
    ops = [
        {"op": "box", "from": [x0, 0, z0], "to": [x1, 1, z1], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [x0, 1, z0], "to": [x1, wall_h + 1, z1], "mat": "wall", "thick": 2, "part": "wall"},
        {"op": "gable", "from": [x0 - 1, wall_h + 1, z0 - 1], "to": [x1 + 1, wall_h + 1, z1 + 1], "axis": "x",
         "mat": roof, "rise": rise, "part": "roof"},
    ]
    if stone_gables:
        for gx0, gx1 in ((x0, x0 + 2), (x1 - 2, x1)):
            ops.append({"op": "gable", "from": [gx0, wall_h + 1, z0], "to": [gx1, wall_h + 1, z1], "axis": "x",
                        "mat": "wall", "rise": rise, "part": "wall"})
    return ops


def ridge_y(z0, z1, wall_h, rise):
    return wall_h + 1 + (z1 - z0 + 2) / 2 * rise


HOUSE_SLOTS = {"wall": "cotswold_stone", "floor": "moss_flagstone", "roof": "stone_tiles", "glass": "lamplit_glass",
               "door": "door_green", "pot": "brick_brown"}


def cottage():
    x0, x1, z0, z1, wh, rise = 1, 33, 3, 25, 18, 2
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = house_body(x0, x1, z0, z1, wh, rise)
    ops += door(15, 4, 8, z0)
    ops += window(5, 5, 5, 4, z0, z0 + 2) + window(24, 5, 5, 4, z0, z0 + 2)
    ops += window(6, 12, 4, 4, z0, z0 + 2) + window(24, 12, 4, 4, z0, z0 + 2)
    ops += window(8, 6, 5, 4, z1 - 2, z1) + window(20, 6, 5, 4, z1 - 2, z1)
    ops += side_window(x0, x0 + 2, 12, 12, 4, 4) + side_window(x1 - 2, x1, 12, 12, 4, 4)
    # A stone hood over the door on two brackets.
    ops += [
        {"op": "box", "from": [13, 10, 1], "to": [21, 11, 3], "mat": "wall", "part": "stone"},
        {"op": "gable", "from": [13, 11, 0], "to": [21, 11, 3], "axis": "z", "mat": "roof", "rise": 1, "part": "roof"},
    ]
    ops += chimney(x0, 12, wh, top) + chimney(x1 - 3, 12, wh, top)
    save("structures", {"name": "cotswold_cottage", "notes": "A two-up two-down Cotswold cottage of honey stone under a steep stone-slate roof, stone gable ends, a chimney at each end, a hood over the door and lamplit mullioned windows. Door on local -Z.",
                        "size": [34, top + 3, 27], "anchor": [17, 0, 13], "foundation": 8, "tags": ["hemlow", "house"],
                        "slots": dict(HOUSE_SLOTS), "ops": ops})


def thatch_cottage():
    x0, x1, z0, z1, wh, rise = 1, 31, 3, 23, 14, 2
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = [
        {"op": "box", "from": [x0, 0, z0], "to": [x1, 1, z1], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [x0, 1, z0], "to": [x1, wh + 1, z1], "mat": "wall", "thick": 2, "part": "wall"},
        # Deep thatch carried over the ends as well (a hipped look from the ridge).
        {"op": "gable", "from": [x0 - 2, wh + 1, z0 - 2], "to": [x1 + 2, wh + 1, z1 + 2], "axis": "x", "mat": "roof",
         "rise": rise, "part": "roof"},
        {"op": "gable", "from": [x0 - 1, wh - 1, z0 + 2], "to": [x0 + 3, wh - 1, z1 - 2], "axis": "z", "mat": "roof",
         "rise": 1, "part": "roof"},
        {"op": "gable", "from": [x1 - 3, wh - 1, z0 + 2], "to": [x1 + 1, wh - 1, z1 - 2], "axis": "z", "mat": "roof",
         "rise": 1, "part": "roof"},
    ]
    ops += door(9, 4, 8, z0, "door")
    ops += window(17, 5, 6, 4, z0, z0 + 2) + window(3, 5, 4, 4, z0, z0 + 2)
    ops += window(12, 5, 5, 4, z1 - 2, z1)
    ops += side_window(x1 - 2, x1, 5, 10, 4, 4)
    ops += chimney(14, 11, wh, top, w=4, d=3)
    slots = dict(HOUSE_SLOTS, roof="thatch", door="door_blue")
    save("structures", {"name": "cotswold_thatch", "notes": "A low thatched cottage of honey stone, the thatch carried deep over the eaves and round the ends, one chimney through the ridge. Door on local -Z.",
                        "size": [33, top + 3, 26], "anchor": [16, 0, 13], "foundation": 8, "tags": ["hemlow", "house"],
                        "slots": slots, "ops": ops})


def big_house():
    x0, x1, z0, z1, wh, rise = 1, 45, 3, 29, 26, 2
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = house_body(x0, x1, z0, z1, wh, rise)
    # Two gabled dormers flush with the front wall, each with a window.
    for gx in (6, 28):
        ops.append({"op": "box", "from": [gx, wh + 1, z0], "to": [gx + 12, wh + 7, z0 + 4], "mat": "wall", "part": "wall"})
        ops.append({"op": "gable", "from": [gx - 1, wh + 7, z0 - 1], "to": [gx + 13, wh + 7, z0 + 10], "axis": "z",
                    "mat": "roof", "rise": 2, "part": "roof"})
        ops.append({"op": "gable", "from": [gx, wh + 7, z0], "to": [gx + 12, wh + 7, z0 + 2], "axis": "z",
                    "mat": "wall", "rise": 2, "part": "wall"})
        ops += window(gx + 4, wh + 2, 4, 5, z0, z0 + 2)
    ops += door(21, 4, 9, z0)
    ops += window(6, 5, 6, 5, z0, z0 + 2) + window(32, 5, 6, 5, z0, z0 + 2)
    ops += window(7, 15, 5, 5, z0, z0 + 2) + window(33, 15, 5, 5, z0, z0 + 2) + window(21, 15, 4, 5, z0, z0 + 2)
    ops += window(10, 6, 6, 5, z1 - 2, z1) + window(28, 6, 6, 5, z1 - 2, z1) + window(19, 16, 6, 5, z1 - 2, z1)
    ops += side_window(x0, x0 + 2, 16, 14, 4, 5) + side_window(x1 - 2, x1, 16, 14, 4, 5)
    ops += [{"op": "box", "from": [19, 11, 1], "to": [27, 12, 3], "mat": "wall", "part": "stone"}]
    ops += chimney(x0, 14, wh, top, d=5) + chimney(x1 - 3, 14, wh, top, d=5)
    slots = dict(HOUSE_SLOTS, door="door_red")
    save("structures", {"name": "cotswold_house", "notes": "A two-storey Cotswold house: honey stone, stone-slate roof, two gabled dormers in the front wall, end chimneys, lamplit mullioned windows. Door on local -Z.",
                        "size": [46, top + 3, 31], "anchor": [23, 0, 15], "foundation": 8, "tags": ["hemlow", "house"],
                        "slots": slots, "ops": ops})


def row():
    x0, x1, z0, z1, wh, rise = 1, 85, 3, 25, 18, 2
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = house_body(x0, x1, z0, z1, wh, rise)
    for i, (dx, slot) in enumerate(((0, "door"), (28, "door2"), (56, "door3"))):
        ops += door(x0 + dx + 4, 4, 8, z0, slot)
        ops += window(x0 + dx + 14, 5, 6, 4, z0, z0 + 2)
        ops += window(x0 + dx + 5, 12, 4, 4, z0, z0 + 2) + window(x0 + dx + 16, 12, 4, 4, z0, z0 + 2)
        ops += window(x0 + dx + 10, 6, 5, 4, z1 - 2, z1)
    ops += chimney(x0, 12, wh, top) + chimney(x0 + 27, 11, wh, top, w=3, d=5) + chimney(x0 + 55, 11, wh, top, w=3, d=5) + chimney(x1 - 3, 12, wh, top)
    slots = dict(HOUSE_SLOTS, door="door_green", door2="door_yellow", door3="door_blue")
    save("structures", {"name": "cotswold_row", "notes": "A terrace of three small Cotswold cottages under one stone-slate roof, each with its own painted door and chimney. Doors on local -Z.",
                        "size": [86, top + 3, 27], "anchor": [43, 0, 13], "foundation": 8, "tags": ["hemlow", "house"],
                        "slots": slots, "ops": ops})


def church():
    # Tower x 0-24, nave x 24-84, chancel x 84-108; south porch on the front.
    nz0, nz1, nh = 12, 44, 28
    ops = [
        {"op": "box", "from": [24, 0, nz0], "to": [84, 1, nz1], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [24, 1, nz0], "to": [84, nh + 1, nz1], "mat": "wall", "thick": 2, "part": "wall"},
        {"op": "gable", "from": [23, nh + 1, nz0 - 1], "to": [85, nh + 1, nz1 + 1], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
        {"op": "gable", "from": [82, nh + 1, nz0], "to": [84, nh + 1, nz1], "axis": "x", "mat": "wall", "rise": 1, "part": "wall"},
        # Chancel
        {"op": "box", "from": [84, 0, 18], "to": [108, 1, 38], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [84, 1, 18], "to": [108, 23, 38], "mat": "wall", "thick": 2, "part": "wall"},
        {"op": "gable", "from": [84, 23, 17], "to": [109, 23, 39], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
        {"op": "gable", "from": [106, 23, 18], "to": [108, 23, 38], "axis": "x", "mat": "wall", "rise": 1, "part": "wall"},
        {"op": "box", "from": [107, 6, 24], "to": [108, 20, 32], "mat": "glass", "part": "glass"},
        {"op": "box", "from": [107, 6, 27], "to": [108, 20, 29], "mat": "wall", "part": "wall"},
        # Tower: hollow, a west door, belfry openings, battlements and pinnacles.
        {"op": "walls", "from": [0, 0, 16], "to": [24, 84, 40], "mat": "wall", "thick": 3, "part": "wall"},
        {"op": "box", "from": [0, 0, 16], "to": [24, 1, 40], "mat": "floor", "part": "floor"},
        {"op": "box", "from": [0, 1, 25], "to": [3, 13, 31], "mat": "air"},
        {"op": "box", "from": [1, 1, 25], "to": [2, 12, 31], "mat": "door", "part": "timber"},
        {"op": "box", "from": [0, 20, 26], "to": [3, 32, 30], "mat": "glass", "part": "glass"},
        {"op": "box", "from": [-1, 0, 15], "to": [25, 4, 41], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [-1, 40, 15], "to": [25, 41, 41], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [-1, 84, 15], "to": [25, 86, 41], "mat": "wall", "part": "stone"},
    ]
    # Louvred belfry openings on all four faces.
    for a, b in (((8, 66, 16), (16, 78, 19)), ((8, 66, 37), (16, 78, 40)), ((0, 66, 24), (3, 78, 32)), ((21, 66, 24), (24, 78, 32))):
        ops.append({"op": "box", "from": list(a), "to": list(b), "mat": "air"})
        for y in range(68, 78, 3):
            ops.append({"op": "box", "from": [a[0], y, a[2]], "to": [b[0], y + 1, b[2]], "mat": "louvre", "part": "timber"})
    # Battlements: merlons round the top.
    for i in range(0, 26, 4):
        for z in (15, 39):
            ops.append({"op": "box", "from": [i - 1, 86, z], "to": [i + 1, 89, z + 2], "mat": "wall", "part": "stone"})
        for x in (-1, 23):
            ops.append({"op": "box", "from": [x, 86, 15 + i], "to": [x + 2, 89, 17 + i], "mat": "wall", "part": "stone"})
    for x, z in ((-1, 15), (22, 15), (-1, 38), (22, 38)):
        ops.append({"op": "box", "from": [x, 86, z], "to": [x + 3, 95, z + 3], "mat": "wall", "part": "stone"})
        ops.append({"op": "box", "from": [x + 1, 95, z + 1], "to": [x + 2, 98, z + 2], "mat": "wall", "part": "stone"})
    # Tall nave windows on both sides.
    for x in (32, 46, 66, 76):
        for z0, z1 in ((nz0, nz0 + 2), (nz1 - 2, nz1)):
            ops.append({"op": "box", "from": [x, 7, z0], "to": [x + 4, 23, z1], "mat": "glass", "part": "glass"})
            ops.append({"op": "box", "from": [x + 2, 7, z0], "to": [x + 3, 23, z1], "mat": "wall", "part": "wall"})
    # Buttresses between them.
    for x in (42, 62, 82):
        ops.append({"op": "box", "from": [x, 0, nz0 - 3], "to": [x + 2, 20, nz0], "mat": "wall", "part": "stone"})
        ops.append({"op": "box", "from": [x, 0, nz1], "to": [x + 2, 20, nz1 + 3], "mat": "wall", "part": "stone"})
    # South porch with its door into the nave.
    ops += [
        {"op": "walls", "from": [52, 0, 0], "to": [68, 17, nz0 + 1], "mat": "wall", "thick": 2, "part": "wall"},
        {"op": "box", "from": [52, 0, 0], "to": [68, 1, nz0], "mat": "floor", "part": "floor"},
        {"op": "gable", "from": [51, 17, -1], "to": [69, 17, nz0 + 1], "axis": "z", "mat": "roof", "rise": 1, "part": "roof"},
        {"op": "gable", "from": [52, 17, 0], "to": [68, 17, 2], "axis": "z", "mat": "wall", "rise": 1, "part": "wall"},
        {"op": "box", "from": [56, 1, 0], "to": [64, 13, 2], "mat": "air"},
        {"op": "box", "from": [57, 1, nz0], "to": [63, 13, nz0 + 2], "mat": "air"},
        {"op": "box", "from": [57, 1, nz0 + 1], "to": [63, 13, nz0 + 2], "mat": "door", "part": "timber"},
    ]
    slots = {"wall": "cotswold_stone", "floor": "moss_flagstone", "roof": "stone_tiles", "glass": "lamplit_glass",
             "door": "timber_frame", "louvre": "weathered_planks"}
    save("structures", {"name": "wool_church", "notes": "Hemlow's wool church: a square west tower 22 m to its pinnacles with battlements and louvred belfry, a long nave with tall lit windows and buttresses, a lower chancel to the east and a south porch. Tower at local -X; porch on local -Z.",
                        "size": [110, 99, 48], "anchor": [56, 0, 28], "foundation": 10, "tags": ["hemlow", "church", "landmark"],
                        "slots": slots, "ops": [shift(o, 1, 0, 1) for o in ops]})


def shift(op, dx, dy, dz):
    """Move an op by whole voxels (so negative overhangs land inside the size)."""
    o = dict(op)
    for k in ("from", "to", "at", "until", "base", "centre"):
        if k in o:
            v = o[k]
            o[k] = [v[0] + dx, v[1] + dy, v[2] + dz]
    return o


def tithe_barn():
    x0, x1, z0, z1, wh, rise = 1, 101, 12, 48, 20, 1
    top = int(ridge_y(z0, z1, wh, rise))
    ops = house_body(x0, x1, z0, z1, wh, rise)
    ops = [o for o in ops if o["op"] != "box"]  # earth floor
    for px in (22, 62):
        ops += [
            {"op": "walls", "from": [px, 1, 1], "to": [px + 20, wh + 3, z0 + 1], "mat": "wall", "thick": 2, "part": "wall"},
            {"op": "gable", "from": [px - 1, wh + 3, 0], "to": [px + 21, wh + 3, z0 + 6], "axis": "z", "mat": "roof", "rise": 2, "part": "roof"},
            {"op": "gable", "from": [px, wh + 3, 1], "to": [px + 20, wh + 3, 3], "axis": "z", "mat": "wall", "rise": 2, "part": "wall"},
            {"op": "box", "from": [px + 3, 1, 1], "to": [px + 17, 19, 3], "mat": "air"},
            {"op": "box", "from": [px + 3, 1, z0], "to": [px + 17, 19, z0 + 2], "mat": "air"},
            {"op": "box", "from": [px + 3, 1, z1 - 2], "to": [px + 17, 17, z1 - 1], "mat": "air"},
            {"op": "box", "from": [px + 3, 1, z1 - 1], "to": [px + 17, 17, z1], "mat": "door", "part": "timber"},
        ]
    # Ventilation slits and buttresses down both long sides.
    for x in range(8, 100, 12):
        if not (20 <= x <= 44 or 60 <= x <= 84):
            ops.append({"op": "box", "from": [x, 7, z0], "to": [x + 1, 15, z0 + 2], "mat": "air"})
        ops.append({"op": "box", "from": [x, 7, z1 - 2], "to": [x + 1, 15, z1], "mat": "air"})
        ops.append({"op": "box", "from": [x + 4, 0, z1], "to": [x + 7, 16, z1 + 3], "mat": "wall", "part": "stone"})
    ops.append({"op": "box", "from": [x0, 26, 26], "to": [x0 + 2, 32, 34], "mat": "air"})
    ops.append({"op": "box", "from": [x1 - 2, 26, 26], "to": [x1, 32, 34], "mat": "air"})
    slots = {"wall": "cotswold_stone", "roof": "stone_tiles", "door": "weathered_planks", "floor": "packed_dirt"}
    save("structures", {"name": "tithe_barn", "notes": "The abbey's tithe barn: 25 m long under a huge stone-slate roof, two gabled cart porches on the front (local -Z) with wide doorways right through, slit vents and buttresses.",
                        "size": [102, top + 3, 52], "anchor": [51, 0, 30], "foundation": 10, "tags": ["hemlow", "barn"],
                        "slots": slots, "ops": ops})


def inn():
    x0, x1, z0, z1, wh, rise = 1, 53, 5, 35, 26, 2
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = house_body(x0, x1, z0, z1, wh, rise)
    ops += door(24, 5, 9, z0)
    # Two lit bay windows either side of the door.
    for bx in (6, 36):
        ops += [
            {"op": "box", "from": [bx, 1, z0 - 3], "to": [bx + 11, 4, z0], "mat": "wall", "part": "stone"},
            {"op": "box", "from": [bx, 4, z0 - 3], "to": [bx + 11, 11, z0 + 2], "mat": "glass", "part": "glass"},
            {"op": "box", "from": [bx + 5, 4, z0 - 3], "to": [bx + 6, 11, z0], "mat": "wall", "part": "wall"},
            {"op": "box", "from": [bx - 1, 11, z0 - 4], "to": [bx + 12, 12, z0], "mat": "roof", "part": "roof"},
        ]
    ops += window(8, 16, 5, 5, z0, z0 + 2) + window(24, 16, 5, 5, z0, z0 + 2) + window(40, 16, 5, 5, z0, z0 + 2)
    ops += window(10, 6, 6, 5, z1 - 2, z1) + window(36, 6, 6, 5, z1 - 2, z1) + window(23, 16, 6, 5, z1 - 2, z1)
    ops += side_window(x0, x0 + 2, 6, 17, 5, 5) + side_window(x1 - 2, x1, 16, 17, 5, 5)
    # Lanterns by the door and the hanging sign on its bracket.
    ops += [
        {"op": "box", "from": [21, 9, z0 - 1], "to": [22, 11, z0], "mat": "glass", "part": "glass"},
        {"op": "box", "from": [31, 9, z0 - 1], "to": [32, 11, z0], "mat": "glass", "part": "glass"},
        {"op": "box", "from": [50, 1, 0], "to": [52, 22, 2], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [42, 20, 0], "to": [52, 21, 2], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [44, 12, 1], "to": [50, 19, 2], "mat": "sign", "part": "timber"},
    ]
    ops += chimney(x0, 15, wh, top, d=5) + chimney(x1 - 3, 15, wh, top, d=5)
    slots = dict(HOUSE_SLOTS, door="door_black", timber="timber_frame", sign="door_green")
    save("structures", {"name": "village_inn", "notes": "The Hob and Lantern, Hemlow's inn: two storeys of honey stone, lit bay windows either side of the door, lanterns, a painted sign hanging from a post on its bracket. Door on local -Z.",
                        "size": [54, top + 3, 37], "anchor": [27, 0, 20], "foundation": 8, "tags": ["hemlow", "inn"],
                        "slots": slots, "ops": ops})


def dovecote():
    ops = [
        {"op": "cylinder", "base": [12, 0, 12], "radius": 10, "height": 24, "mat": "wall", "part": "wall"},
        {"op": "cylinder", "base": [12, 1, 12], "radius": 8, "height": 23, "mat": "air"},
        {"op": "box", "from": [10, 1, 1], "to": [14, 9, 4], "mat": "air"},
        {"op": "box", "from": [10, 1, 2], "to": [14, 9, 3], "mat": "door", "part": "timber"},
        {"op": "cylinder", "base": [12, 12, 12], "radius": 10.5, "height": 1, "mat": "wall", "part": "stone"},
    ]
    for i, r in enumerate((11, 9.5, 8, 6.5, 5, 3.5, 2)):
        ops.append({"op": "cylinder", "base": [12, 24 + i * 2, 12], "radius": r, "height": 2, "mat": "roof", "part": "roof"})
    ops += [
        {"op": "box", "from": [10, 38, 10], "to": [14, 42, 14], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [11, 39, 10], "to": [13, 41, 14], "mat": "air"},
        {"op": "box", "from": [10, 39, 11], "to": [14, 41, 13], "mat": "air"},
        {"op": "box", "from": [9, 42, 9], "to": [15, 43, 15], "mat": "roof", "part": "roof"},
        {"op": "box", "from": [11, 43, 11], "to": [13, 44, 13], "mat": "roof", "part": "roof"},
    ]
    save("structures", {"name": "dovecote", "notes": "A round stone dovecote with a conical stone-slate roof and a little timber lantern on top where the doves go in and out. Door on local -Z.",
                        "size": [24, 45, 24], "anchor": [12, 0, 12], "foundation": 6, "tags": ["hemlow"],
                        "slots": {"wall": "cotswold_stone", "roof": "stone_tiles", "timber": "weathered_planks", "door": "door_wood"},
                        "ops": ops})


def market_cross():
    ops = [
        {"op": "box", "from": [0, 0, 0], "to": [16, 2, 16], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [2, 2, 2], "to": [14, 4, 14], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [4, 4, 4], "to": [12, 6, 12], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [7, 6, 7], "to": [9, 26, 9], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [4, 21, 7], "to": [12, 23, 9], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [7, 26, 7], "to": [9, 28, 9], "mat": "wall", "part": "stone"},
    ]
    save("structures", {"name": "market_cross", "notes": "The market cross on Hemlow green: three worn steps, a tall shaft and a weathered cross head.",
                        "size": [16, 28, 16], "anchor": [8, 0, 8], "foundation": 4, "tags": ["hemlow"],
                        "slots": {"wall": "lichen_rock"}, "ops": ops})


def packhorse_bridge():
    # 16 m long along local X, 3 m wide; the deck's top at y 20 (anchor).
    ops = [{"op": "box", "from": [0, 0, 1], "to": [64, 20, 11], "mat": "wall", "part": "stone"}]
    for cx in (14, 32, 50):
        ops.append({"op": "ellipsoid", "centre": [cx, 0, 6], "radius": [7.5, 14, 8], "mat": "air"})
    ops += [
        {"op": "box", "from": [0, 19, 2], "to": [64, 20, 10], "mat": "deck", "part": "floor"},
        {"op": "box", "from": [0, 20, 1], "to": [64, 23, 2], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [0, 20, 10], "to": [64, 23, 11], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [16, 20, 2], "to": [48, 21, 10], "mat": "deck", "part": "floor"},
        {"op": "box", "from": [16, 23, 1], "to": [48, 24, 2], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [16, 23, 10], "to": [48, 24, 11], "mat": "wall", "part": "stone"},
        # Cutwaters on the piers.
        {"op": "box", "from": [21, 0, 0], "to": [25, 12, 1], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [39, 0, 0], "to": [43, 12, 1], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [21, 0, 11], "to": [25, 12, 12], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [39, 0, 11], "to": [43, 12, 12], "mat": "wall", "part": "stone"},
    ]
    save("structures", {"name": "packhorse_bridge", "notes": "A medieval packhorse bridge: three low arches, a narrow cobbled deck humped over the middle, low parapets. 16 m along local X; base rim hangs it from the banks, so place it at the river's middle with local X across the stream.",
                        "size": [64, 24, 12], "anchor": [32, 20, 6], "level": False, "base": "rim", "decays": False,
                        "tags": ["hemlow", "bridge"], "slots": {"wall": "cotswold_stone", "deck": "fae_cobbles"}, "ops": ops})


def hob_stones():
    ops = []
    n = 11
    hs = [11, 15, 9, 17, 12, 8, 14, 16, 10, 13, 11]
    ws = [6, 5, 7, 5, 6, 4, 6, 5, 6, 5, 7]
    for i in range(n):
        a = i / n * math.tau + 0.2
        cx, cz = 36 + math.cos(a) * 27, 36 + math.sin(a) * 27
        h, w = hs[i], ws[i]
        # Each stone a rough slab set along the ring: a body, a knobbly top
        # leaning one way, and a weathered shoulder.
        tx, tz = -math.sin(a), math.cos(a)
        ops.append({"op": "ellipsoid", "centre": [round(cx, 1), h * 0.45, round(cz, 1)],
                    "radius": [round(abs(tx) * w / 2 + abs(tz) * 1.8 + 0.4, 1), h * 0.5 + 0.5, round(abs(tz) * w / 2 + abs(tx) * 1.8 + 0.4, 1)],
                    "mat": "stone", "part": "stone"})
        lean = 1.0 if i % 2 else -1.0
        ops.append({"op": "ellipsoid", "centre": [round(cx + tx * lean, 1), h * 0.85, round(cz + tz * lean, 1)],
                    "radius": [round(abs(tx) * w / 3 + 1.4, 1), h * 0.22 + 0.5, round(abs(tz) * w / 3 + 1.4, 1)],
                    "mat": "stone", "part": "stone"})
        if i % 3 == 0:
            ops.append({"op": "ellipsoid", "centre": [round(cx, 1), h * 0.6, round(cz, 1)], "radius": [1.6, 1.4, 1.6], "mat": "moss", "part": "stone"})
    # The King Stone, tall and alone outside the ring.
    ops.append({"op": "ellipsoid", "centre": [6, 10, 6], "radius": [3.0, 10.5, 2.2], "mat": "stone", "part": "stone"})
    ops.append({"op": "ellipsoid", "centre": [7, 17, 6], "radius": [2.2, 3.5, 1.8], "mat": "stone", "part": "stone"})
    ops.append({"op": "ellipsoid", "centre": [6, 13, 7], "radius": [1.4, 1.4, 1.6], "mat": "moss", "part": "stone"})
    save("structures", {"name": "hob_stones", "notes": "The Hob Stones: a ring of eleven knobbly weathered standing stones, 2-4.5 m tall and 14 m across, glowing moss on some, and a taller King Stone outside the ring to the north-west. Settles on the ground; centre on the clearing.",
                        "size": [72, 22, 72], "anchor": [36, 0, 36], "level": False, "decays": False, "tags": ["hemlow", "landmark"],
                        "slots": {"stone": "lichen_rock", "moss": "glowmoss"}, "ops": ops})


def dry_wall():
    ops = [
        {"op": "box", "from": [0, 0, 0], "to": [40, 4, 3], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [0, 4, 1], "to": [40, 5, 2], "mat": "wall", "part": "stone"},
        {"op": "box", "from": [17, 0, 0], "to": [22, 5, 3], "mat": "air"},
        {"op": "box", "from": [17, 0, 1], "to": [18, 5, 2], "mat": "gate", "part": "timber"},
        {"op": "box", "from": [18, 2, 1], "to": [22, 3, 2], "mat": "gate", "part": "timber"},
        {"op": "box", "from": [18, 4, 1], "to": [22, 5, 2], "mat": "gate", "part": "timber"},
    ]
    save("structures", {"name": "cotswold_wall", "notes": "A 10 m length of honey dry-stone field wall with a coping of stones on edge and a field gate in it. Long axis local X.",
                        "size": [40, 5, 3], "anchor": [20, 0, 1], "level": False, "decays": False, "tags": ["hemlow"],
                        "slots": {"wall": "cotswold_stone", "gate": "weathered_planks"}, "ops": ops})


def cone(x, z, y0, length, r0, down, mat="flow"):
    """A tapering spike of stacked cylinders: up from y0, or down from it."""
    ops = []
    steps = max(2, int(length // 2))
    for k in range(steps):
        r = max(0.6, r0 * (1 - k / steps))
        y = y0 - (k + 1) * 2 if down else y0 + k * 2
        ops.append({"op": "cylinder", "base": [x, y, z], "radius": round(r, 2), "height": 2, "mat": mat, "part": "stone"})
    return ops


def formations(name, ceiling, notes, column=True):
    """Stalactites as long ellipsoids hanging from above the ceiling, so only
    their tapering lower halves show; stalagmites the same, up from below the
    floor; in the chambers one column where the two have met."""
    import random
    rnd = random.Random(name)
    ops = []
    for _ in range(rnd.randint(4, 7)):
        x, z = rnd.randint(4, 20), rnd.randint(4, 20)
        r, l = rnd.uniform(1.6, 3.2), rnd.uniform(7, 16)
        ops.append({"op": "ellipsoid", "centre": [x, ceiling + 24, z], "radius": [round(r, 1), round(l + 14, 1), round(r * rnd.uniform(0.8, 1.2), 1)], "mat": "flow", "part": "stone"})
    for _ in range(rnd.randint(2, 4)):
        x, z = rnd.randint(4, 20), rnd.randint(4, 20)
        r, l = rnd.uniform(2.0, 3.6), rnd.uniform(5, 12)
        ops.append({"op": "ellipsoid", "centre": [x, 6, z], "radius": [round(r, 1), round(l + 6, 1), round(r * rnd.uniform(0.8, 1.2), 1)], "mat": "flow", "part": "stone"})
    if column:
        x, z = rnd.randint(7, 17), rnd.randint(7, 17)
        ops.append({"op": "cylinder", "base": [x, 0, z], "radius": 2.2, "height": ceiling + 30, "mat": "flow", "part": "stone"})
        ops.append({"op": "ellipsoid", "centre": [x, 10, z], "radius": [4.0, 6, 4.0], "mat": "flow", "part": "stone"})
        ops.append({"op": "ellipsoid", "centre": [x, ceiling + 12, z], "radius": [3.6, 7, 3.6], "mat": "flow", "part": "stone"})
    save("structures", {"name": name, "notes": notes, "size": [24, ceiling + 34, 24], "anchor": [12, 12, 12], "level": False,
                        "decays": False, "tags": ["hemlow", "cave"], "slots": {"flow": "flowstone"}, "ops": ops})


formations("cave_formations", 21, "A few stalactites, stalagmites and one column for a cave passage about 5 m high: place with `height` at the passage floor (sea + passage height - radius); the stalactites start above the ceiling and hang down.", column=False)
formations("cave_formations_tall", 34, "Stalactites, stalagmites and a column for a cave chamber about 8 m high: place with `height` at the chamber floor.")


def log_pile():
    ops = []
    for row, (n, y) in enumerate(((5, 1.6), (4, 4.6), (3, 7.6))):
        for i in range(n):
            ops.append({"op": "log", "base": [1, y, 3.2 + i * 3.1 + row * 1.5], "radius": 1.5, "length": 22, "axis": "x", "mat": "log", "part": "timber"})
    for x, z in ((0, 1), (23, 1), (0, 18), (23, 18)):
        ops.append({"op": "box", "from": [x, 0, z], "to": [x + 1, 10, z + 1], "mat": "post", "part": "timber"})
    save("structures", {"name": "log_pile", "notes": "A woodcutter's stack of cordwood between posts, 6 m long. Long axis local X.",
                        "size": [24, 10, 20], "anchor": [12, 0, 10], "level": False, "decays": False, "tags": ["hemlow", "prop"],
                        "slots": {"log": "bark", "post": "weathered_planks"}, "ops": ops})


def chopping_block():
    ops = [
        {"op": "cylinder", "base": [6, 0, 6], "radius": 2.6, "height": 3, "mat": "log", "part": "timber"},
        {"op": "box", "from": [5, 3, 6], "to": [6, 4, 7], "mat": "blade", "part": "metal"},
        {"op": "box", "from": [5, 4, 6], "to": [6, 9, 7], "mat": "handle", "part": "timber"},
        {"op": "log", "base": [9, 0.8, 1], "radius": 0.9, "length": 4, "axis": "z", "mat": "log", "part": "timber"},
        {"op": "log", "base": [1, 0.8, 9], "radius": 0.9, "length": 4, "axis": "x", "mat": "log", "part": "timber"},
        {"op": "log", "base": [2, 0.8, 2], "radius": 0.8, "length": 3, "axis": "x", "mat": "log", "part": "timber"},
    ]
    save("structures", {"name": "chopping_block", "notes": "A chopping block with an axe left in it and split logs round it.",
                        "size": [12, 10, 12], "anchor": [6, 0, 6], "level": False, "decays": False, "tags": ["hemlow", "prop"],
                        "slots": {"log": "bark", "blade": "rust_iron", "handle": "weathered_planks"}, "ops": ops})


def skep_bench():
    ops = [
        {"op": "box", "from": [0, 3, 1], "to": [28, 4, 7], "mat": "plank", "part": "timber"},
        {"op": "box", "from": [1, 0, 2], "to": [3, 3, 6], "mat": "plank", "part": "timber"},
        {"op": "box", "from": [25, 0, 2], "to": [27, 3, 6], "mat": "plank", "part": "timber"},
        {"op": "box", "from": [0, 4, 7], "to": [28, 14, 8], "mat": "plank", "part": "timber"},
        {"op": "box", "from": [0, 14, 0], "to": [28, 15, 8], "mat": "roof", "part": "roof"},
    ]
    for x in (5, 14, 23):
        ops.append({"op": "ellipsoid", "centre": [x, 4, 4], "radius": [3.2, 5.5, 3.0], "mat": "skep", "part": "roof"})
        ops.append({"op": "box", "from": [x, 4, 1], "to": [x + 1, 5, 2], "mat": "air"})
    save("structures", {"name": "skep_bench", "notes": "Three straw bee skeps on a plank bench under a little lean-to roof. Front local -Z.",
                        "size": [28, 15, 8], "anchor": [14, 0, 4], "level": False, "decays": False, "tags": ["hemlow", "prop"],
                        "slots": {"plank": "weathered_planks", "roof": "thatch", "skep": "thatch"}, "ops": ops})


def herb_beds():
    ops = []
    for i, z in enumerate((0, 8, 16)):
        ops.append({"op": "walls", "from": [0, 0, z], "to": [20, 2, z + 6], "mat": "edge", "thick": 1, "part": "timber"})
        ops.append({"op": "box", "from": [1, 0, z + 1], "to": [19, 2, z + 5], "mat": "soil", "part": "floor"})
        for x in range(2, 18, 3):
            ops.append({"op": "ellipsoid", "centre": [x + 0.5, 2.5, z + 3], "radius": [1.3, 1.2, 1.3], "mat": ["herb", "herb", "bloom"][(x + i) % 3], "part": "roof"})
    save("structures", {"name": "herb_beds", "notes": "Three raised herb beds edged with boards: green herbs among pink-flowered roses.",
                        "size": [20, 4, 22], "anchor": [10, 0, 11], "level": False, "decays": False, "tags": ["hemlow", "prop"],
                        "slots": {"edge": "weathered_planks", "soil": "packed_dirt", "herb": "salal_leaves", "bloom": "rose_bush"}, "ops": ops})


def charcoal_clamp():
    ops = [
        {"op": "ellipsoid", "centre": [14, 0, 14], "radius": [12, 8, 12], "mat": "earth", "part": "stone"},
        {"op": "cylinder", "base": [14, 6, 14], "radius": 1.2, "height": 3, "mat": "char", "part": "stone"},
        {"op": "cylinder", "base": [14, 8, 14], "radius": 0.6, "height": 1, "mat": "ember", "part": "stone"},
        {"op": "cylinder", "base": [34, 0, 8], "radius": 3.6, "height": 12, "mat": "pole", "part": "timber"},
        {"op": "cylinder", "base": [34, 0, 8], "radius": 2.6, "height": 11, "mat": "air"},
        {"op": "box", "from": [33, 0, 4], "to": [36, 7, 6], "mat": "air"},
    ]
    for k in range(9):
        a = k / 9 * math.tau
        x, z = 34 + math.cos(a) * 3, 8 + math.sin(a) * 3
        ops.append({"op": "box", "from": [round(x), 10, round(z)], "to": [round(x) + 1, 17 - k % 3, round(z) + 1], "mat": "pole", "part": "timber"})
    save("structures", {"name": "charcoal_clamp", "notes": "A charcoal burner's clamp: a low earth-covered mound with a glowing vent on top, and beside it the burner's round hut of poles.",
                        "size": [40, 18, 28], "anchor": [20, 0, 14], "level": False, "decays": False, "tags": ["hemlow", "prop"],
                        "slots": {"earth": "packed_dirt", "char": "charcoal", "ember": "lamplit_glass", "pole": "dead_wood"}, "ops": ops})


def woodcutter_hut():
    x0, x1, z0, z1, wh, rise = 1, 25, 3, 21, 13, 1
    top = int(ridge_y(z0, z1, wh, rise)) + 4
    ops = [
        {"op": "box", "from": [x0, 0, z0], "to": [x1, 1, z1], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [x0, 1, z0], "to": [x1, wh + 1, z1], "mat": "wall", "thick": 2, "part": "wall"},
        {"op": "box", "from": [x0, 1, z0], "to": [x0 + 2, wh + 1, z0 + 2], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [x1 - 2, 1, z0], "to": [x1, wh + 1, z0 + 2], "mat": "timber", "part": "timber"},
        {"op": "gable", "from": [x0 - 1, wh + 1, z0 - 2], "to": [x1 + 1, wh + 1, z1 + 2], "axis": "x", "mat": "roof", "rise": rise, "part": "roof"},
    ]
    ops += door(11, 4, 8, z0)
    ops += window(4, 5, 4, 4, z0, z0 + 2) + window(18, 5, 4, 4, z0, z0 + 2)
    ops += chimney(x1 - 4, 9, wh, top, w=3, d=3)
    ops += [
        {"op": "box", "from": [x1, 0, z0 + 2], "to": [x1 + 9, 1, z1 - 2], "mat": "floor", "part": "floor"},
        {"op": "box", "from": [x1 + 8, 0, z0 + 2], "to": [x1 + 9, 9, z0 + 3], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [x1 + 8, 0, z1 - 3], "to": [x1 + 9, 9, z1 - 2], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [x1, 9, z0 + 1], "to": [x1 + 10, 10, z1 - 1], "mat": "roof", "part": "roof"},
    ]
    for i in range(3):
        ops.append({"op": "log", "base": [x1 + 1, 1.5 + i * 2.6, z0 + 3], "radius": 1.2, "length": z1 - z0 - 6, "axis": "z", "mat": "log", "part": "timber"})
    slots = {"wall": "weathered_planks", "timber": "timber_frame", "floor": "packed_dirt", "roof": "shingle_moss", "glass": "lamplit_glass",
             "door": "door_wood", "pot": "brick_brown", "log": "bark"}
    save("structures", {"name": "woodcutter_hut", "notes": "A woodcutter's plank hut in the wood: mossy shingle roof, a chimney, lamplit windows, and a lean-to woodshed on its +X side stacked with logs. Door on local -Z.",
                        "size": [36, top + 3, 24], "anchor": [14, 0, 12], "foundation": 6, "tags": ["hemlow", "house"],
                        "slots": slots, "ops": ops})


log_pile()
chopping_block()
skep_bench()
herb_beds()
charcoal_clamp()
woodcutter_hut()

cottage()
thatch_cottage()
big_house()
row()
church()
tithe_barn()
inn()
dovecote()
market_cross()
packhorse_bridge()
hob_stones()
dry_wall()

# ---- creatures ----------------------------------------------------------------

save("creatures", {"name": "firefly", "notes": "Firefly: a small dark beetle whose tail lights green-gold in slow flashes, each on its own rhythm, drifting low over the long grass and the wood's edge at dusk. Drawn well over life size so it shows.",
                   "voxel": 0.03, "half_w": 0.08, "height": 0.08,
                   "colors": {"mothbody": [36, 28, 24], "wingcase": [74, 58, 40], "lamp": [196, 236, 96, 3.2]},
                   "behaviour": {"movement": "flutter", "fly_height": 0.9, "blink": 0.45},
                   "ops": [
                       {"op": "ellipsoid", "centre": [16, 16, 14], "radius": [1.3, 1.1, 2.4], "mat": "mothbody"},
                       {"op": "ellipsoid", "centre": [16, 15.8, 18.4], "radius": [1.5, 1.3, 2.0], "mat": "lamp"},
                       {"op": "ellipsoid", "centre": [14.6, 17.0, 15.4], "radius": [1.0, 0.4, 2.8], "mat": "wingcase"},
                       {"op": "ellipsoid", "centre": [17.4, 17.0, 15.4], "radius": [1.0, 0.4, 2.8], "mat": "wingcase"},
                       {"op": "set", "at": [15, 16, 11], "mat": "mothbody"},
                       {"op": "set", "at": [17, 16, 11], "mat": "mothbody"}]})
save("creatures", {"name": "marsh_light", "notes": "A will-o'-the-wisp of the Wisp Moss: a pale blue-white ball of light the size of a fist with a fainter halo, drifting low over the bog pools and slowly pulsing. Nobody knows what it is.",
                   "voxel": 0.03, "half_w": 0.1, "height": 0.1,
                   "colors": {"wisp": [220, 236, 255, 3.0], "halo": [150, 190, 255, 1.4]},
                   "behaviour": {"movement": "flutter", "fly_height": 1.1, "blink": 0.12},
                   "ops": [
                       {"op": "ellipsoid", "centre": [16, 16, 16], "radius": [3.4, 3.4, 3.4], "mat": "halo"},
                       {"op": "ellipsoid", "centre": [16, 16, 16], "radius": [2.2, 2.2, 2.2], "mat": "wisp"}]})
save("creatures", {"name": "hedgehog", "notes": "Hedgehog: a small brown ball of spines that trundles along the hedge bottoms at dusk and curls up tight when you come close.",
                   "colors": {"shell": [104, 84, 62], "scute": [74, 58, 40], "rim": [96, 74, 48], "belly": [150, 98, 60],
                              "skin": [128, 96, 60], "dark": [36, 28, 24]},
                   "behaviour": {"walk_speed": 0.25, "flee_speed": 0.25, "wary": 4}, "like": "tortoise", "scale": 0.6})
save("creatures", {"name": "barn_owl", "notes": "Barn owl: pale gold and white, quartering low over the meadows at dusk on soft slow wingbeats.",
                   "colors": {"body": [240, 236, 228], "head": [240, 236, 228], "beak": [210, 180, 140], "wing": [214, 176, 112],
                              "tail": [214, 176, 112], "legs": [210, 180, 140]},
                   "behaviour": {"movement": "circle", "fly_height": 4, "walk_speed": 2.5, "flee_speed": 6.0, "wary": 10.0},
                   "like": "starling", "scale": 1.9})
save("creatures", {"name": "kingfisher", "notes": "Kingfisher: a flash of electric blue with an orange breast, flitting between the willows and reeds along the Hem.",
                   "colors": {"body": [40, 130, 190], "head": [40, 110, 170], "wing": [40, 130, 190], "tail": [40, 110, 170],
                              "breast": [222, 120, 52], "head_cap": [60, 170, 210]},
                   "like": "sparrow", "scale": 0.85})

save("creatures", {"name": "tawny_owl", "notes": "Tawny owl: a round, rusty-brown wood owl with dark eyes, floating low between the trunks of the Hollins at dusk.",
                   "colors": {"body": [150, 98, 60], "head": [150, 98, 60], "beak": [210, 180, 140], "wing": [128, 96, 60],
                              "tail": [128, 96, 60], "legs": [210, 180, 140]},
                   "behaviour": {"movement": "circle", "fly_height": 5, "walk_speed": 2.2, "flee_speed": 5.5, "wary": 9.0},
                   "like": "starling", "scale": 1.7})
save("creatures", {"name": "great_horned_owl", "notes": "Great horned owl: big, mottled grey-brown with a pale throat, quartering slowly over old forest at dusk. Rare: seen now and then, never for long.",
                   "colors": {"body": [110, 90, 70], "head": [96, 78, 60], "beak": [36, 28, 24], "wing": [88, 72, 56],
                              "tail": [88, 72, 56], "legs": [210, 180, 140]},
                   "behaviour": {"movement": "circle", "fly_height": 8, "walk_speed": 2.5, "flee_speed": 6.0, "wary": 14.0, "spawn_chance": 0.08},
                   "like": "starling", "scale": 2.3})
save("creatures", {"name": "jewel_kingfisher", "notes": "Jewel kingfisher: a small kingfisher of the tropics, electric turquoise above and burnt orange below, with a violet-blue crown; flits along the cenote rims and jungle streams.",
                   "colors": {"body": [24, 196, 206], "head": [70, 90, 220], "wing": [24, 196, 206], "tail": [40, 150, 220],
                              "breast": [240, 128, 36], "head_cap": [110, 120, 255]},
                   "like": "sparrow", "scale": 0.8})

# ---- collectibles -------------------------------------------------------------

save("collectibles", {"name": "hag_stone", "display": "hag stone",
                      "notes": "A pebble with a hole worn right through it. The village says if you look through one you see the Hollins as they really are.",
                      "voxel": 0.022, "colors": {"pebble": [168, 160, 146], "dark": [36, 28, 24]},
                      "ops": [{"op": "ellipsoid", "centre": [16, 5, 16], "radius": [9, 5, 8], "mat": "pebble"},
                              {"op": "cylinder", "base": [16, 0, 16], "radius": 2.6, "height": 11, "mat": "dark"}]})

# ---- biomes -------------------------------------------------------------------

def p(plant, density, verb="scatter", **params):
    d = {"plant": plant, "density": density, "placement": {"verb": verb, "params": params}}
    if "field" in params:
        d["field"] = params.pop("field")
    return d


def grad(plant, density, field, lo, hi):
    return {"plant": plant, "density": density, "placement": {"verb": "gradient", "params": {"min": lo, "max": hi}, "field": field}}


def c(name, count):
    return {"creature": name, "count": count}


save("biomes", {"name": "hollins", "notes": "The Hollins: old oak and beech wood with hazel and fern beneath, turning strange near the Hob Stones (gold maples, cherry trees, blossom vines, pale birch, glassbloom). The default of Hemlow.",
                "priority": 1, "rules": {}, "surface": "moss_grass", "subsurface": "dirt", "map_color": [62, 120, 64],
                "plants": [
                    p("beech", 0.0045, "clustered", size=45, min=0.3, max=1.0),
                    p("great_oak", 0.0011, "lone", spacing=22),
                    p("oak", 0.0025, "clustered", size=30, min=0.25, max=1.0),
                    p("hazel", 0.01, "clustered", size=14, min=0.2, max=1.0),
                    grad("gold_maple", 0.004, "strange", 0.12, 1.0),
                    grad("flowering_cherry", 0.002, "strange", 0.3, 1.0),
                    grad("pale_birch", 0.003, "strange", 0.45, 1.0),
                    grad("blossom_vine", 0.25, "strange", 0.08, 1.0),
                    p("hanging_vine", 0.08),
                    p("fern", 0.14, "clustered", size=12, min=0.0, max=1.0),
                    grad("giant_fir", 0.003, "strange", 0.25, 1.0),
                    p("bigleaf_maple", 0.0015, "lone", spacing=24),
                    p("elder", 0.0015, "edge", width=14, inner=0.002),
                    grad("glassbloom", 0.02, "strange", 0.55, 1.0),
                ],
                "features": [{"structure": "mossy_log", "density": 0.0004, "placement": {"verb": "scatter", "params": {}}},
                             {"structure": "fallen_trunk", "density": 0.00015, "placement": {"verb": "scatter", "params": {}}}],
                "creatures": [c("deer", 8), c("mossback_deer", 7), c("fae_horse", 3), c("moss_bear", 1), c("moss_tortoise", 3),
                              c("boar", 4), c("fox", 3), c("sky_ray", 7), c("lantern_moth", 50), c("firefly", 90),
                              c("dusk_wren", 14), c("woodpecker", 2), c("glow_bee", 8), c("tawny_owl", 2), c("great_horned_owl", 1)],
                "collectibles": [{"item": "mushroom", "density": 0.0008, "placement": {"verb": "scatter", "params": {}}}]})

save("biomes", {"name": "hemlow_meadow", "notes": "The hay meadows and pasture on the Knap and down the valley: flower-thick turf, a few great oaks and elders, dry-stone walls, fireflies at dusk.",
                "priority": 8, "rules": {"region": "meadow"}, "surface": "wildflower_grass", "subsurface": "dirt", "map_color": [150, 186, 80],
                "plants": [
                    p("great_oak", 0.00025, "lone", spacing=60),
                    p("elder", 0.0008, "clustered", size=20, min=0.3, max=1.0),
                    p("cottage_rose", 0.0008, "clustered", size=16, min=0.4, max=1.0),
                    p("hazel", 0.004, "edge", width=12, inner=0.0),
                ],
                "features": [{"structure": "cotswold_wall", "density": 0.0005, "placement": {"verb": "scatter", "params": {"seat": 1.0}}}],
                "creatures": [c("rabbit", 14), c("hedgehog", 4), c("starling", 40), c("sparrow", 10), c("barn_owl", 2), c("firefly", 160),
                              c("lantern_moth", 20), c("bumblebee", 8), c("bee", 8), c("glow_bee", 6), c("fae_horse", 2), c("horse", 4)]})

save("biomes", {"name": "hemlow_green", "notes": "The village on the top of the Knap: lawns, cottage-garden roses and cherries, the great oak on the green.",
                "priority": 9, "rules": {"region": "hemlow"}, "surface": "lawn_grass", "subsurface": "dirt", "map_color": [180, 200, 110],
                "plants": [
                    p("great_oak", 0.0004, "lone", spacing=45),
                    p("cottage_rose", 0.006, "clustered", size=10, min=0.2, max=1.0),
                    p("flowering_cherry", 0.0006, "lone", spacing=26),
                    p("elder", 0.0006),
                ],
                "creatures": [c("sparrow", 12), c("dusk_wren", 6), c("firefly", 40), c("lantern_moth", 12), c("bee", 6), c("hedgehog", 2)]})

save("biomes", {"name": "hem_banks", "notes": "The Hem's banks: willows and alders, reedmace in the slack water, kingfishers, herons and dragonflies.",
                "priority": 10, "rules": {"river": [0, 9]}, "surface": "meadow_grass", "subsurface": "dirt", "map_color": [90, 160, 120],
                "plants": [
                    p("willow", 0.004, "riparian", width=8),
                    p("alder", 0.003, "riparian", width=9),
                    p("reedmace", 0.05, "riparian", width=3),
                    p("cottage_rose", 0.0015),
                ],
                "creatures": [c("heron", 2), c("faerie_heron", 1), c("kingfisher", 5), c("dragonfly", 10), c("glass_dragonfly", 6),
                              c("mallard", 8), c("firefly", 40)],
                "collectibles": [{"item": "hag_stone", "density": 0.0006, "placement": {"verb": "scatter", "params": {}}}]})

save("biomes", {"name": "wisp_moss", "notes": "The Wisp Moss round Hemmere: sphagnum and peat pools, reedmace, willow scrub, dead alders standing in the wet; marsh lights over it after sunset.",
                "priority": 11, "rules": {"region": "moss"}, "surface": "bog_moss", "subsurface": "peat", "map_color": [130, 120, 70],
                "plants": [
                    p("reedmace", 0.14, "clustered", size=7, min=0.45, max=1.0),
                    p("willow", 0.0018, "clustered", size=25, min=0.3, max=1.0),
                    p("alder", 0.001),
                    p("snag", 0.0012),
                    p("broken_snag", 0.0008),
                    p("heather", 0.012, "clustered", size=14, min=0.3, max=1.0),
                    p("birch", 0.0008, "edge", width=18, inner=0.0),
                ],
                "creatures": [c("marsh_light", 16), c("firefly", 60), c("heron", 2), c("faerie_heron", 2), c("goose", 10), c("swan", 2),
                              c("mallard", 10), c("dragonfly", 10), c("glass_dragonfly", 8), c("meadowhawk", 6)],
                "mist": 0.2, "water": {"shallow": [74, 100, 72], "deep": [26, 42, 32]}})

save("biomes", {"name": "edge_hanger", "notes": "The beech hanger on the face of Hemlow Edge and the combes: bare oolite between beeches clinging to the slope, ferns, vines over the lip.",
                "priority": 9, "rules": {"slope": [0.9, 50]}, "surface": "forest_floor", "subsurface": "dirt", "rock": "oolite",
                "map_color": [120, 130, 80],
                "plants": [
                    grad("beech", 0.006, "slope", 0.8, 3.0),
                    p("hanging_vine", 0.5),
                    p("fern", 0.12, "clustered", size=8, min=0.0, max=1.0),
                    p("hazel", 0.004),
                ],
                "creatures": [c("woodpecker", 1), c("firefly", 20), c("sky_ray", 2)]})

save("biomes", {"name": "lantern_spout", "notes": "Round Lantern Spout and its pool: moss lit faintly by the glow-worms, ferns in the spray, vines down the cliff.",
                "priority": 12, "rules": {"region": "spout"}, "surface": "moss_grass", "subsurface": "dirt", "rock": "oolite",
                "map_color": [80, 200, 190],
                "plants": [
                    p("hanging_vine", 0.8),
                    p("fern", 0.1, "clustered", size=6, min=0.0, max=1.0),
                    p("willow", 0.001),
                ],
                "creatures": [c("firefly", 40), c("glass_dragonfly", 6), c("dragonfly", 4), c("lantern_moth", 10)],
                "mist": 0.35})

save("biomes", {"name": "hob_clearing", "notes": "The clearing round the Hob Stones: short grass where no tree grows, a ring of glassbloom round it, sky rays turning overhead.",
                "priority": 12, "rules": {"region": "hob"}, "surface": "moss_grass", "subsurface": "dirt", "map_color": [170, 120, 220],
                "plants": [
                    {"plant": "glassbloom", "density": 0.25, "placement": {"verb": "ring", "params": {"x": 470, "z": 210, "radius": 30, "width": 5}}},
                ],
                "creatures": [c("sky_ray", 4), c("glow_bee", 4), c("lantern_moth", 16), c("firefly", 30)],
                "collectibles": [{"item": "hag_stone", "density": 0.002, "placement": {"verb": "scatter", "params": {}}}]})

print(f"wrote {len(written)} entries")
