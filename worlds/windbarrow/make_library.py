"""Writes Windbarrow's library entries (materials, plants, structures,
creatures, collectibles, biomes) through libfmt.render so they come out
canonical. The JSON files in library/ are what the game reads. See the note below.

One-shot, not kept current. This ran once, when the world was made; the JSON
in library/ has moved on since (entries renamed or removed, new fields, hand
edits), so rerunning it would overwrite live data with a stale version. It is
kept as a record of how the world was first written. Edit the JSON directly
instead, and if you do rerun this, diff the output before keeping any of it.
"""
import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

W = "windbarrow"
written = []


def save(kind, d):
    d = {"name": d.pop("name"), "version": 1, "introduced_by": W, **d}
    path = os.path.join(ROOT, "library", kind, d["name"] + ".json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(libfmt.render(d, kind))
    written.append(f"{kind}/{d['name']}")


# ---- materials ---------------------------------------------------------------
# New materials leave `id` out: the game gives each world its own ids at load.

def mat(name, notes, pattern, base, alt, calm_base, calm_alt, scale=8, contrast=1.0, **kw):
    tex = {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast,
           "calm": {"base": calm_base, "alt": calm_alt}}
    d = {"name": name, "notes": notes, "smooth": kw.pop("smooth", False), **kw, "texture": tex}
    save("materials", d)


# The country's cover: a tall bunchgrass gone gold, tinted like a meadow so
# the grass sprites on it grow long in patches and bow together in the gusts.
# Meadow-tinted ground swings between sage and ochre across tens of metres
# (calm_tint in scene.wgsl multiplies by up to 1.3, 1.04, 0.58), so these bases
# are kept low in chroma or the ochre patches come out pure yellow.
mat("feathergrass", "Feathergrass: the steppe's cover, a tall bunchgrass with a silver-gold summer awn, tawny over bare chalk between the tussocks.",
    "noise", [126, 116, 86], [114, 104, 78], [150, 138, 108], [138, 126, 98], scale=6, contrast=0.7,
    smooth=True, grass={"sparse": 0.12, "dense": 0.9, "short": 3.2},
    top={"pattern": "noise", "base": [140, 128, 96], "alt": [128, 118, 88], "scale": 6, "contrast": 0.7,
         "calm": {"base": [164, 150, 118], "alt": [152, 140, 110]}})
mat("downland_grass", "Short chalk turf: close-cropped grass gone khaki with thyme and small flowers in it, the chalk showing through on the crowns.",
    "noise", [112, 114, 88], [122, 124, 96], [132, 134, 106], [142, 144, 114], scale=7, contrast=0.6,
    smooth=True, grass={"sparse": 0.1, "dense": 0.8, "short": 3.0},
    top={"pattern": "noise", "base": [124, 126, 98], "alt": [134, 136, 106], "scale": 7, "contrast": 0.6,
         "calm": {"base": [146, 148, 116], "alt": [156, 158, 124]}})
mat("barley_grass", "A small walled barley plot: thin ranks of pale ears over worked chalk soil.",
    "noise", [142, 132, 94], [156, 146, 106], [162, 152, 114], [174, 164, 126], scale=5, contrast=0.5,
    smooth=True, grass={"sparse": 0.5, "dense": 0.95, "short": 2.6})
mat("chalk_white", "White chalk: soft, thickly bedded limestone, the bones of the whole country. Shows on every scarp and hoof-cut.",
    "strata", [196, 192, 182], [210, 206, 196], [198, 194, 184], [210, 208, 198], scale=2, contrast=0.45,
    smooth=True)
mat("chalk_grey", "A greyer, harder bed of the chalk low in the stack, stained along its joints.",
    "strata", [152, 150, 144], [168, 166, 158], [156, 154, 148], [170, 168, 160], scale=3, contrast=0.5,
    smooth=True)
mat("flint_band", "A band of flint nodules in the chalk: hard blue-black lumps in a level line along every face that reaches it.",
    "strata", [62, 64, 72], [46, 48, 54], [68, 70, 78], [50, 52, 58], scale=4, contrast=0.9, smooth=True)
mat("chalk_rubble", "Broken chalk and flint underfoot: the trails, the pit floors and the bared ground of the Tepe.",
    "gravel", [170, 162, 146], [150, 144, 132], [172, 164, 150], [154, 148, 138], scale=11, contrast=0.5,
    smooth=True)
mat("flint_grass", "The cover of the Flint Down: thin tawny-khaki grass with chalk and black flint showing through everywhere.",
    "speckle", [120, 116, 94], [94, 92, 84], [138, 134, 110], [106, 104, 94], scale=6, contrast=0.8,
    smooth=True, grass={"sparse": 0.04, "dense": 0.3, "short": 1.8})
mat("flint", "Knapped flint: the working floors and the spoil, glassy blue-grey under a chalk-white cortex.",
    "speckle", [58, 60, 66], [176, 172, 160], [64, 66, 72], [178, 174, 164], scale=7, contrast=1.0)
mat("chalk_block", "Chalk cut in blocks: the barrow kerbs, the Tepe's bench wall and the lining of the passage.",
    "coursed", [196, 190, 176], [208, 204, 190], [198, 192, 180], [210, 206, 194], scale=5, contrast=0.7)
mat("carved_limestone", "The Tepe's pillars: a hard cream limestone from somewhere else, worked smooth and carved in low relief.",
    "blotch", [180, 176, 164], [164, 160, 148], [192, 188, 176], [176, 172, 160], scale=6, contrast=0.5)
mat("ochre", "Red ochre, still in the grooves of the carving and on the posts.", "noise",
    [146, 78, 56], [128, 66, 46], [150, 84, 62], [132, 72, 52], scale=6, contrast=0.6)
mat("daub", "Wattle and daub, limewashed: the walls of the houses, cream going grey where the rain drives on them.",
    "blotch", [212, 198, 168], [192, 178, 148], [216, 202, 174], [196, 182, 154], scale=5, contrast=0.6)
mat("oak_post", "Split oak, adze-dressed and weathered silver: the house posts, the barrow posts and the granary legs.",
    "grain", [138, 122, 98], [118, 104, 84], [144, 128, 106], [122, 108, 90], scale=6, contrast=0.7)
mat("cave_chalk", "The walls of the caves: dry chalk, grey-brown where water once ran down the joints, and pale enough to hold what light comes in at the mouths and down the shafts.",
    "strata", [158, 146, 124], [138, 126, 106], [162, 150, 128], [142, 130, 110], scale=3, contrast=0.5,
    smooth=True, glow=0.45)
mat("bone_floor", "The floor of a bear chamber: earth trodden black by generations of bears, with old bone broken through it.",
    "speckle", [66, 56, 46], [206, 198, 176], [70, 60, 50], [208, 200, 180], scale=6, contrast=0.9,
    smooth=True)
mat("old_bone", "Long bones and skulls dry on a cave floor: ivory going brown where the earth has stained them.",
    "noise", [206, 196, 170], [184, 172, 146], [212, 202, 178], [190, 178, 154], scale=5, contrast=0.6)
mat("rawhide", "Hides scraped, smoked brown and lashed over a frame: the cover of the bone house and the windbreak.",
    "noise", [122, 88, 58], [104, 74, 48], [128, 94, 64], [110, 80, 54], scale=5, contrast=0.7)
mat("frost_grass", "The grass of a frost hollow: every blade rimed white and glittering, whatever the hour and whatever the season.",
    "noise", [196, 202, 202], [176, 184, 186], [206, 212, 212], [186, 194, 196], scale=6, contrast=0.5,
    smooth=True, glints=True, grass={"sparse": 0.08, "dense": 0.5, "short": 1.6})
mat("bone_rubble", "The ground of the Bone Downs: chalk rubble with splinters and knuckles of old ivory broken through it.",
    "speckle", [176, 164, 136], [228, 220, 194], [180, 168, 142], [230, 222, 198], scale=8, contrast=1.1,
    smooth=True)
mat("salt_crust", "The white crust round Saltmere: salt dried in plates over grey mud, cracked into polygons.",
    "cracks", [210, 206, 192], [186, 180, 164], [212, 208, 196], [190, 184, 170], scale=4, contrast=0.8,
    smooth=True)
mat("salt_mud", "Grey-brown salt mud: the bed of Saltmere and the flats that come out of it in summer.",
    "noise", [120, 112, 98], [136, 128, 112], [124, 116, 104], [138, 130, 116], scale=6, contrast=0.5,
    smooth=True)

# ---- plants -------------------------------------------------------------------
# Everything else the world plants (hawthorn, juniper, willow, alder, birch,
# sagebrush, heather, snags, lichen) is reused from the library as it stands.

save("plants", {"name": "mother_oak",
                "notes": "branching: The great oak below Durn: a vast domed crown on a short, hugely buttressed black trunk, wider than it is tall and old enough that the village has always called it that. One tree only, in its own region.",
                "form": "branching", "height": [26, 30], "trunk_radius": [1.0, 1.2], "buttress": 0.9,
                "crown_radius": [9.0, 11.0], "bark": "live_oak_bark", "leaf": "leaves",
                "curve": "arch", "canopy": "cloud", "fill": 0.85, "leader": 0.05, "spread": 1.0,
                "dome": 0.4, "max_slope": 0.7, "autumn": "leaves_autumn"})
save("plants", {"name": "feather_tussock",
                "notes": "herb: A big feathergrass tussock standing out of the sward, its awns silver-gold and bent downwind; drawn as sprites, so you walk through it.",
                "form": "herb", "height": [1.3, 2.1], "crown_radius": [0.5, 0.9],
                "bark": "sawgrass_blades", "leaf": "sawgrass_blades", "max_slope": 0.9})

# ---- structures ---------------------------------------------------------------
# Voxels are 0.25 m. Front faces local -Z. Neolithic Windbarrow: posts of split
# oak, walls of limewashed daub, roofs of straw, fences of woven hazel.

HOUSE_SLOTS = {"wall": "daub", "roof": "thatch", "timber": "oak_post", "floor": "packed_dirt",
               "hurdle": "stick_wood"}


def cone_roof(cx, cz, y0, r, steps, step_h=1, mat="roof", part="roof"):
    """A conical straw roof: stacked cylinders narrowing to a point, then the
    inside carved out again so the house is hollow under it."""
    ops = []
    for k in range(steps):
        rr = max(0.9, r * (1 - k / steps))
        ops.append({"op": "cylinder", "base": [cx, y0 + k * step_h, cz], "radius": round(rr, 2),
                    "height": step_h, "mat": mat, "part": part})
    for k in range(steps - 2):
        rr = r * (1 - k / steps) - 2.2
        if rr > 1.0:
            ops.append({"op": "cylinder", "base": [cx, y0 + k * step_h, cz], "radius": round(rr, 2),
                        "height": step_h, "mat": "air"})
    return ops


def round_house(name, r, wall_h, roof_steps, notes):
    roof_steps = roof_steps * 2
    """A round house: a daub wall ring on a dirt floor under a deep conical
    straw roof, its door through the wall on local -Z."""
    over = r + 3                      # the eaves stand out past the wall
    size = 2 * over + 4
    c = size // 2
    ops = [
        {"op": "cylinder", "base": [c, 0, c], "radius": r + 1, "height": 1, "mat": "floor", "part": "floor"},
        {"op": "cylinder", "base": [c, 1, c], "radius": r, "height": wall_h, "mat": "wall", "part": "wall"},
        {"op": "cylinder", "base": [c, 1, c], "radius": r - 2, "height": wall_h, "mat": "air"},
        # posts set into the wall ring, the roof's feet
        {"op": "cylinder", "base": [c, 1, c], "radius": r - 2, "height": 1, "mat": "floor", "part": "floor"},
    ]
    for k in range(8):
        a = k * math.tau / 8 + 0.4
        px, pz = c + (r - 1) * math.cos(a), c + (r - 1) * math.sin(a)
        ops.append({"op": "cylinder", "base": [round(px, 1), 1, round(pz, 1)], "radius": 1.2,
                    "height": wall_h + 1, "mat": "timber", "part": "timber"})
    ops += cone_roof(c, c, 1 + wall_h, over, roof_steps)
    # the door, cut through the wall and the eaves on -Z, with a lintel
    dz0 = c - over - 1
    ops += [
        {"op": "box", "from": [c - 2, 1, dz0], "to": [c + 2, 8, c - r + 3], "mat": "air"},
        {"op": "box", "from": [c - 3, 8, c - r - 1], "to": [c + 3, 9, c - r + 3], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [c - 2, 0, dz0], "to": [c + 2, 1, c - r + 1], "mat": "floor", "part": "floor"},
    ]
    save("structures", {"name": name, "notes": notes, "size": [size, 1 + wall_h + roof_steps + 2, size],
                        "anchor": [c, 0, c], "foundation": 6, "tags": ["windbarrow", "house"],
                        "slots": dict(HOUSE_SLOTS), "ops": ops})


round_house("round_house", 12, 8, 11,
            "A Windbarrow round house: a limewashed daub wall ring 6 m across on oak posts, under a deep conical straw roof. Door on local -Z.")
round_house("round_house_small", 9, 7, 9,
            "A small Windbarrow round house, 4.5 m across: daub on oak posts under a straw cone. Door on local -Z.")


def longhouse():
    """The gathering house: 14 m along local X, daub between oak posts, a long
    straw roof, a door at each end of the front wall (local -Z)."""
    x1, z1, wh = 56, 28, 10
    ops = [
        {"op": "box", "from": [0, 0, 0], "to": [x1, 1, z1], "mat": "floor", "part": "floor"},
        {"op": "walls", "from": [0, 1, 0], "to": [x1, wh + 1, z1], "mat": "wall", "thick": 2, "part": "wall"},
    ]
    # oak posts up the long walls and at the corners
    for x in (0, 10, 20, 27, 34, 44, 54):
        for z in (0, z1 - 2):
            ops.append({"op": "box", "from": [x, 1, z], "to": [x + 2, wh + 1, z + 2], "mat": "timber", "part": "timber"})
    ops += [
        {"op": "box", "from": [0, wh + 1, 0], "to": [x1, wh + 2, z1], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [2, wh + 1, 2], "to": [x1 - 2, wh + 2, z1 - 2], "mat": "air"},
        {"op": "gable", "from": [0, wh + 2, 0], "to": [x1, wh + 2, z1], "axis": "x",
         "mat": "roof", "rise": 2, "part": "roof"},
    ]
    for dx in (8, 40):
        ops += [
            {"op": "box", "from": [dx, 1, 0], "to": [dx + 6, 9, 2], "mat": "air"},
            {"op": "box", "from": [dx - 1, 9, 0], "to": [dx + 7, 10, 2], "mat": "timber", "part": "timber"},
        ]
    # smoke gap at the ridge
    ops.append({"op": "box", "from": [24, wh + 8, 12], "to": [32, wh + 14, 16], "mat": "air"})
    save("structures", {"name": "longhouse",
                        "notes": "The longhouse where a Windbarrow village gathers: 14 m of daub between split-oak posts under a long straw roof, a smoke gap at the ridge, a door at each end of the front wall (local -Z).",
                        "size": [x1, wh + 32, z1], "anchor": [x1 // 2, 0, 0], "foundation": 6,
                        "tags": ["windbarrow", "house"], "slots": dict(HOUSE_SLOTS), "ops": ops})


longhouse()


def granary():
    """A grain store lifted clear of the ground on four oak legs."""
    ops = []
    for x, z in ((2, 2), (12, 2), (2, 12), (12, 12)):
        ops.append({"op": "box", "from": [x, 0, z], "to": [x + 2, 11, z + 2], "mat": "timber", "part": "timber"})
    ops += [
        {"op": "box", "from": [1, 11, 1], "to": [15, 13, 15], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [2, 13, 2], "to": [14, 21, 14], "mat": "wall", "part": "wall"},
        {"op": "box", "from": [4, 13, 2], "to": [8, 18, 3], "mat": "timber", "part": "timber"},
    ]
    ops += cone_roof(8, 8, 21, 9, 14)
    save("structures", {"name": "granary",
                        "notes": "A Windbarrow granary: a small daub box on four oak legs, out of reach of mice, under a straw cone. Its hatch is on local -Z.",
                        "size": [16, 40, 16], "anchor": [8, 0, 8], "foundation": 4,
                        "tags": ["windbarrow", "house"], "slots": dict(HOUSE_SLOTS), "ops": ops})


granary()


def stock_pen():
    """A ring of woven hazel hurdles with a gap for a gate."""
    r, h = 26, 5
    size = 2 * r + 6
    c = size // 2
    ops = [
        {"op": "cylinder", "base": [c, 0, c], "radius": r + 1, "height": h, "mat": "hurdle", "part": "frame"},
        {"op": "cylinder", "base": [c, 0, c], "radius": r - 1, "height": h, "mat": "air"},
        {"op": "box", "from": [c - 5, 0, c - r - 2], "to": [c + 5, h, c - r + 2], "mat": "air"},
        {"op": "box", "from": [c - 5, 0, c - r - 1], "to": [c - 4, h + 2, c - r + 1], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [c + 4, 0, c - r - 1], "to": [c + 5, h + 2, c - r + 1], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [c - 5, 2, c - r - 1], "to": [c + 5, 3, c - r], "mat": "hurdle", "part": "frame"},
    ]
    for k in range(10):
        a = k * math.tau / 10 + 0.2
        px, pz = c + r * math.cos(a), c + r * math.sin(a)
        ops.append({"op": "cylinder", "base": [round(px, 1), 0, round(pz, 1)], "radius": 1.1,
                    "height": h + 2, "mat": "timber", "part": "timber"})
    save("structures", {"name": "stock_pen",
                        "notes": "A Windbarrow stock pen: a ring of woven hazel hurdles 13 m across on oak stakes, its gate on local -Z.",
                        "size": [size, h + 4, size], "anchor": [c, 0, c], "foundation": 3,
                        "tags": ["windbarrow"], "slots": dict(HOUSE_SLOTS), "ops": ops})


stock_pen()


def hurdle_fence():
    ops = [
        {"op": "box", "from": [0, 0, 1], "to": [40, 5, 3], "mat": "hurdle", "part": "frame"},
    ]
    for x in (0, 9, 19, 29, 38):
        ops.append({"op": "box", "from": [x, 0, 0], "to": [x + 2, 7, 4], "mat": "timber", "part": "timber"})
    save("structures", {"name": "hurdle_fence",
                        "notes": "A 10 m run of woven hazel hurdling between oak stakes: the plot and yard fences of Windbarrow. Long axis local X.",
                        "size": [40, 7, 4], "anchor": [20, 0, 2], "foundation": 3, "level": False,
                        "tags": ["windbarrow"], "slots": dict(HOUSE_SLOTS), "ops": ops})


hurdle_fence()


def drying_rack():
    """Hides stretched on a frame, flapping in the wind."""
    ops = [
        {"op": "box", "from": [0, 0, 2], "to": [2, 16, 4], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [26, 0, 2], "to": [28, 16, 4], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [0, 15, 2], "to": [28, 17, 4], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [3, 4, 2], "to": [11, 15, 3], "mat": "hide", "part": "cloth"},
        {"op": "box", "from": [13, 6, 2], "to": [19, 15, 3], "mat": "hide", "part": "cloth"},
        {"op": "box", "from": [21, 3, 2], "to": [26, 15, 3], "mat": "hide", "part": "cloth"},
    ]
    save("structures", {"name": "drying_rack",
                        "notes": "Three hides stretched to dry on an oak frame 7 m long, in the yards of Durn. Long axis local X, hides facing local -Z.",
                        "size": [28, 18, 6], "anchor": [14, 0, 3], "foundation": 3, "level": False,
                        "tags": ["windbarrow"], "slots": {"timber": "oak_post", "hide": "canvas"}, "ops": ops})


drying_rack()


def windbreak():
    """A long hurdle screen with a hide lean-to behind it: the summer camp."""
    ops = [
        {"op": "box", "from": [0, 0, 2], "to": [48, 9, 4], "mat": "hurdle", "part": "frame"},
    ]
    for x in (0, 11, 23, 35, 46):
        ops.append({"op": "box", "from": [x, 0, 1], "to": [x + 2, 11, 5], "mat": "timber", "part": "timber"})
    ops += [
        {"op": "box", "from": [12, 0, 12], "to": [14, 9, 14], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [34, 0, 12], "to": [36, 9, 14], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [12, 8, 4], "to": [36, 9, 14], "mat": "hide", "part": "cloth"},
        {"op": "box", "from": [12, 9, 3], "to": [36, 10, 6], "mat": "hide", "part": "cloth"},
    ]
    save("structures", {"name": "windbreak",
                        "notes": "The flint diggers' windbreak: 12 m of hazel hurdling on oak stakes with a hide lean-to behind it. The screen faces the wind on local -Z; the shelter is behind it.",
                        "size": [48, 12, 16], "anchor": [24, 0, 3], "foundation": 3, "level": False,
                        "tags": ["windbarrow"], "slots": {"timber": "oak_post", "hurdle": "stick_wood", "hide": "canvas"},
                        "ops": ops})


windbreak()


def camp_hearth():
    """An open hearth: a kerb of chalk blocks, ash and charcoal in it, and a
    tripod of poles over it."""
    ops = [
        {"op": "cylinder", "base": [10, 0, 10], "radius": 7, "height": 2, "mat": "stone", "part": "stone"},
        {"op": "cylinder", "base": [10, 0, 10], "radius": 5.4, "height": 2, "mat": "ash", "part": "floor"},
        {"op": "cylinder", "base": [10, 0, 10], "radius": 3.0, "height": 1, "mat": "char", "part": "floor"},
        {"op": "box", "from": [4, 2, 10], "to": [5, 16, 11], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [15, 2, 10], "to": [16, 16, 11], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [10, 2, 4], "to": [11, 16, 5], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [5, 15, 5], "to": [15, 16, 15], "mat": "timber", "part": "timber"},
    ]
    save("structures", {"name": "camp_hearth",
                        "notes": "An open hearth in a village yard: a chalk-block kerb 3.5 m across with ash and charcoal in it and a tripod of poles over it.",
                        "size": [20, 18, 20], "anchor": [10, 0, 10], "foundation": 2, "level": False,
                        "tags": ["windbarrow"],
                        "slots": {"stone": "chalk_block", "ash": "ash", "char": "charcoal", "timber": "oak_post"},
                        "ops": ops})


camp_hearth()


def tee_pillar(ops, x, z, h, broad_x, thick=2, wide=5, carve_marks=True):
    """One T-shaped pillar: a tall slab with a crossbar laid over its head.
    `broad_x` turns the broad face to face along x."""
    if broad_x:
        ops.append({"op": "box", "from": [x - wide, 0, z - thick], "to": [x + wide, h, z + thick],
                    "mat": "pillar", "part": "stone"})
        ops.append({"op": "box", "from": [x - wide - 2, h, z - thick - 1], "to": [x + wide + 2, h + 4, z + thick + 1],
                    "mat": "pillar", "part": "stone"})
        if carve_marks:
            ops.append({"op": "box", "from": [x - wide + 1, h - 9, z - thick], "to": [x - wide + 3, h - 4, z - thick + 1],
                        "mat": "paint", "part": "stone"})
            ops.append({"op": "box", "from": [x + 1, h - 13, z - thick], "to": [x + wide - 1, h - 12, z - thick + 1],
                        "mat": "paint", "part": "stone"})
    else:
        ops.append({"op": "box", "from": [x - thick, 0, z - wide], "to": [x + thick, h, z + wide],
                    "mat": "pillar", "part": "stone"})
        ops.append({"op": "box", "from": [x - thick - 1, h, z - wide - 2], "to": [x + thick + 1, h + 4, z + wide + 2],
                    "mat": "pillar", "part": "stone"})
        if carve_marks:
            ops.append({"op": "box", "from": [x - thick, h - 9, z - wide + 1], "to": [x - thick + 1, h - 4, z - wide + 3],
                        "mat": "paint", "part": "stone"})
            ops.append({"op": "box", "from": [x - thick, h - 13, z + 1], "to": [x - thick + 1, h - 12, z + wide - 1],
                        "mat": "paint", "part": "stone"})


def tepe_enclosure():
    """The Tepe: a ring of twelve carved T-pillars inside a chalk bench wall,
    two taller pillars at the centre facing out east. 30 m across."""
    size = 124
    c = size // 2
    wall_r, ring_r = 54, 40
    ops = [
        {"op": "cylinder", "base": [c, 0, c], "radius": wall_r, "height": 7, "mat": "bench", "part": "stone"},
        {"op": "cylinder", "base": [c, 1, c], "radius": wall_r - 7, "height": 8, "mat": "air"},
        {"op": "cylinder", "base": [c, 0, c], "radius": wall_r - 7, "height": 1, "mat": "floor", "part": "floor"},
        {"op": "cylinder", "base": [c, 7, c], "radius": wall_r, "height": 1, "mat": "bench", "part": "stone"},
        {"op": "cylinder", "base": [c, 7, c], "radius": wall_r - 7, "height": 1, "mat": "air"},
    ]
    # the way in, a gap in the bench wall on local -Z between two uprights
    ops += [
        {"op": "box", "from": [c - 5, 0, c - wall_r - 2], "to": [c + 5, 9, c - wall_r + 8], "mat": "air"},
        {"op": "box", "from": [c - 5, 0, c - wall_r - 2], "to": [c + 5, 1, c - wall_r + 8], "mat": "floor", "part": "floor"},
    ]
    tee_pillar(ops, c - 8, c - wall_r + 1, 14, True, carve_marks=False)
    tee_pillar(ops, c + 8, c - wall_r + 1, 14, True, carve_marks=False)
    # the ring of twelve
    for k in range(12):
        a = k * math.tau / 12 + math.tau / 24
        px, pz = c + ring_r * math.cos(a), c + ring_r * math.sin(a)
        broad_x = abs(math.sin(a)) >= abs(math.cos(a))
        tee_pillar(ops, int(round(px)), int(round(pz)), 13 + (k % 3), broad_x)
    # the two at the centre, taller, their broad faces north and south
    tee_pillar(ops, c - 7, c, 17, False, thick=3, wide=6)
    tee_pillar(ops, c + 7, c, 17, False, thick=3, wide=6)
    save("structures", {"name": "tepe_enclosure",
                        "notes": "The Tepe: a ring of twelve carved T-shaped limestone pillars 3-4 m tall inside a chalk-block bench wall 27 m across, with two taller pillars at the centre; red ochre still in the carving. The way in is on local -Z; stands on a levelled hilltop.",
                        "size": [size, 26, size], "anchor": [c, 0, c], "foundation": 8, "decays": False,
                        "tags": ["windbarrow", "temple"],
                        "slots": {"bench": "chalk_block", "pillar": "carved_limestone", "paint": "ochre",
                                  "floor": "chalk_rubble"},
                        "ops": ops})


tepe_enclosure()


def tepe_pillar():
    ops = []
    tee_pillar(ops, 10, 10, 15, True, thick=3, wide=6)
    save("structures", {"name": "tepe_pillar",
                        "notes": "A single carved T-pillar of the Tepe's cream limestone, standing where it was left: 4 m tall, its broad faces along local X, ochre in the grooves.",
                        "size": [20, 21, 20], "anchor": [10, 0, 10], "foundation": 6, "decays": False,
                        "level": False, "tags": ["windbarrow", "temple"],
                        "slots": {"pillar": "carved_limestone", "paint": "ochre"}, "ops": ops})


tepe_pillar()


def kurgan_kerb():
    """A ring of chalk kerb blocks round the foot of a mound, half sunk in the
    turf, with a split-oak post standing at one side of it."""
    r = 28
    size = 2 * r + 8
    c = size // 2
    ops = []
    for k in range(26):
        a = k * math.tau / 26
        px, pz = c + r * math.cos(a), c + r * math.sin(a)
        w = 2 + (k % 3)
        ops.append({"op": "box", "from": [int(px) - w, 0, int(pz) - w], "to": [int(px) + w, 4 + (k % 2), int(pz) + w],
                    "mat": "stone", "part": "stone"})
    save("structures", {"name": "kurgan_kerb",
                        "notes": "A kerb of chalk blocks round the foot of a kurgan, 14 m across and half sunk in the turf. Settles on the ground; centre it on a mound of about that radius.",
                        "size": [size, 8, size], "anchor": [c, 0, c], "foundation": 6, "level": False,
                        "decays": False, "tags": ["windbarrow", "barrow"],
                        "slots": {"stone": "chalk_block"}, "ops": ops})


kurgan_kerb()


def barrow_post():
    """The post on a kurgan's crown: a split oak trunk, its top cut to a point
    and ringed with ochre, with chalk blocks packed round its foot."""
    ops = [
        {"op": "cylinder", "base": [8, 0, 8], "radius": 4.2, "height": 2, "mat": "stone", "part": "stone"},
        {"op": "box", "from": [6, 1, 7], "to": [10, 13, 10], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [6, 13, 7], "to": [10, 14, 10], "mat": "paint", "part": "timber"},
        {"op": "box", "from": [7, 14, 8], "to": [9, 16, 9], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [6, 9, 7], "to": [10, 10, 10], "mat": "paint", "part": "timber"},
    ]
    save("structures", {"name": "barrow_post",
                        "notes": "A split-oak post 3.5 m tall standing on a kurgan's crown, ringed with red ochre, chalk blocks packed round its foot. Broad faces along local X.",
                        "size": [16, 18, 16], "anchor": [8, 0, 8], "foundation": 8, "level": False,
                        "decays": False, "tags": ["windbarrow", "barrow"],
                        "slots": {"stone": "chalk_block", "timber": "oak_post", "paint": "ochre"}, "ops": ops})


barrow_post()


def long_barrow_mouth():
    """The open east end of the Long Barrow: a kerbed forecourt, two uprights,
    the closing slab pushed aside, and a chalk-lined passage carved back into
    the mound."""
    size_z = 56
    c = 16
    ops = [
        # the passage, cut back into the mound (local +Z) and lined
        {"op": "box", "from": [c - 5, 1, 8], "to": [c + 5, 10, size_z], "mat": "carve"},
        {"op": "box", "from": [c - 5, 0, 8], "to": [c + 5, 1, size_z], "mat": "floor", "part": "floor"},
        {"op": "box", "from": [c - 5, 1, 8], "to": [c - 3, 9, size_z], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c + 3, 1, 8], "to": [c + 5, 9, size_z], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 5, 9, 8], "to": [c + 5, 10, size_z], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 3, 1, 9], "to": [c + 3, 9, size_z - 2], "mat": "air"},
        # the forecourt: a floor of chalk rubble and a kerb round it
        {"op": "box", "from": [2, 0, 0], "to": [30, 1, 10], "mat": "floor", "part": "floor"},
        {"op": "box", "from": [2, 0, 0], "to": [4, 3, 10], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [28, 0, 0], "to": [30, 3, 10], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [4, 0, 0], "to": [28, 2, 2], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 5, 0, 6], "to": [c + 5, 2, 8], "mat": "air"},
        # the two uprights either side of the mouth, and the lintel over it
        {"op": "box", "from": [c - 8, 0, 6], "to": [c - 5, 13, 9], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c + 5, 0, 6], "to": [c + 8, 13, 9], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 8, 10, 6], "to": [c + 8, 13, 9], "mat": "stone", "part": "stone"},
        # the closing slab, pushed aside and left lying in the forecourt
        {"op": "box", "from": [4, 0, 2], "to": [6, 11, 5], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [6, 0, 2], "to": [13, 2, 5], "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 4, 1, 3], "to": [c + 4, 2, 4], "mat": "paint", "part": "floor"},
    ]
    save("structures", {"name": "long_barrow_mouth",
                        "notes": "The open east end of the Long Barrow: a kerbed forecourt with ochre on its floor, two uprights and a lintel at the mouth, the closing slab pushed aside and lying, and a chalk-lined passage carved 12 m back into the mound. Place at the mound's east end facing out (local -Z); it carves the passage into the earth behind it.",
                        "size": [32, 15, size_z], "anchor": [c, 0, 2], "foundation": 4, "level": False,
                        "decays": False, "tags": ["windbarrow", "barrow"],
                        "slots": {"stone": "chalk_block", "floor": "chalk_rubble", "paint": "ochre"}, "ops": ops})


long_barrow_mouth()


def cave_mouth():
    """A cave entrance: a low arched opening carved back into a slope, meeting
    the passage behind it, with fallen chalk round the threshold."""
    ops = [
        {"op": "box", "from": [4, 1, 6], "to": [20, 13, 64], "mat": "carve"},
        {"op": "box", "from": [6, 1, 6], "to": [18, 14, 64], "mat": "carve"},
        {"op": "box", "from": [4, 0, 6], "to": [20, 1, 64], "mat": "floor", "part": "floor"},
        {"op": "box", "from": [2, 0, 0], "to": [22, 1, 10], "mat": "floor", "part": "floor"},
        {"op": "ellipsoid", "centre": [4, 1, 7], "radius": [3, 2, 3], "mat": "stone", "part": "stone"},
        {"op": "ellipsoid", "centre": [21, 1, 9], "radius": [2.4, 2, 2.4], "mat": "stone", "part": "stone"},
        {"op": "ellipsoid", "centre": [10, 0, 2], "radius": [3.4, 1.6, 2.6], "mat": "stone", "part": "stone"},
    ]
    save("structures", {"name": "cave_mouth",
                        "notes": "A cave mouth in a chalk slope: carves an opening 4 m wide and 3 m high 16 m straight back into the hill, with fallen chalk blocks round the threshold. Place at the foot of a slope facing out (local -Z), on the line of a `passage` so the two meet.",
                        "size": [24, 16, 64], "anchor": [12, 0, 2], "foundation": 2, "level": False,
                        "decays": False, "tags": ["windbarrow", "cave"],
                        "slots": {"floor": "chalk_rubble", "stone": "chalk_white"}, "ops": ops})


cave_mouth()


def bear_den():
    """A bear's bed in a cave chamber: a floor of bone and trodden chalk with
    long bones lying in it and a scored block at the back."""
    rnd = random.Random("bear_den")
    ops = [{"op": "cylinder", "base": [16, 0, 16], "radius": 11, "height": 1, "mat": "floor", "part": "floor"}]
    for _ in range(9):
        x, z = rnd.randint(8, 24), rnd.randint(8, 24)
        ln = rnd.randint(3, 7)
        if rnd.random() < 0.5:
            ops.append({"op": "box", "from": [x, 1, z], "to": [x + ln, 2, z + 1], "mat": "bone", "part": "floor"})
        else:
            ops.append({"op": "box", "from": [x, 1, z], "to": [x + 1, 2, z + ln], "mat": "bone", "part": "floor"})
    ops += [
        {"op": "ellipsoid", "centre": [16, 1, 6], "radius": [5, 3, 3], "mat": "bone", "part": "floor"},
        {"op": "ellipsoid", "centre": [8, 1, 26], "radius": [3, 2, 2.4], "mat": "stone", "part": "stone"},
    ]
    save("structures", {"name": "bear_den",
                        "notes": "A cave bear's bed: a trodden floor of chalk rubble and bone 5 m across with long bones and a skull lying in it. Place inside a cave chamber with `height` at the chamber floor; level false, so it beds into the rubble.",
                        "size": [32, 6, 32], "anchor": [16, 0, 16], "foundation": 3, "level": False,
                        "decays": False, "tags": ["windbarrow", "cave"],
                        "slots": {"floor": "bone_floor", "bone": "old_bone", "stone": "chalk_grey"}, "ops": ops})


bear_den()


def digger_gear():
    """What the flint diggers leave at a pit's edge: antler picks, a hide, a
    basket and a heap of knapped flakes."""
    ops = [
        {"op": "box", "from": [2, 0, 4], "to": [10, 1, 10], "mat": "hide", "part": "cloth"},
        {"op": "ellipsoid", "centre": [15, 1, 6], "radius": [3.2, 1.6, 3.2], "mat": "chips", "part": "floor"},
        {"op": "box", "from": [12, 0, 12], "to": [13, 9, 13], "mat": "timber", "part": "timber"},
        {"op": "box", "from": [11, 8, 11], "to": [15, 10, 15], "mat": "bone", "part": "timber"},
        {"op": "box", "from": [4, 0, 12], "to": [10, 1, 15], "mat": "bone", "part": "floor"},
        {"op": "box", "from": [5, 1, 13], "to": [6, 4, 14], "mat": "bone", "part": "timber"},
    ]
    save("structures", {"name": "digger_gear",
                        "notes": "Flint diggers' gear left at a pit edge: a hide spread on the chalk, a heap of knapped flakes, an antler pick leaning on a stake and another lying. Faces local -Z.",
                        "size": [20, 12, 20], "anchor": [10, 0, 10], "foundation": 0, "level": False,
                        "tags": ["windbarrow"],
                        "slots": {"hide": "canvas", "chips": "flint", "timber": "oak_post", "bone": "old_bone"},
                        "ops": ops})


digger_gear()


def offering_post():
    """A post beside a trail, with a horse skull on it and strips of hide tied
    below: the mark at a parting of the ways."""
    ops = [
        {"op": "cylinder", "base": [8, 0, 8], "radius": 3.4, "height": 2, "mat": "stone", "part": "stone"},
        {"op": "cylinder", "base": [8, 1, 8], "radius": 1.4, "height": 14, "mat": "timber", "part": "timber"},
        {"op": "ellipsoid", "centre": [8, 16, 8], "radius": [2.2, 2.2, 3.6], "mat": "bone", "part": "stone"},
        {"op": "box", "from": [7, 14, 3], "to": [9, 17, 6], "mat": "bone", "part": "stone"},
        {"op": "box", "from": [6, 9, 8], "to": [7, 13, 9], "mat": "hide", "part": "cloth"},
        {"op": "box", "from": [9, 8, 7], "to": [10, 12, 8], "mat": "hide", "part": "cloth"},
        {"op": "box", "from": [8, 10, 10], "to": [9, 13, 11], "mat": "paint", "part": "cloth"},
    ]
    save("structures", {"name": "offering_post",
                        "notes": "An oak post 4 m tall at a parting of the trails, a horse skull set on its top facing local -Z and strips of hide and ochred cloth tied below it.",
                        "size": [16, 20, 16], "anchor": [8, 0, 8], "foundation": 6, "level": False,
                        "tags": ["windbarrow"],
                        "slots": {"stone": "chalk_block", "timber": "oak_post", "bone": "old_bone",
                                  "hide": "canvas", "paint": "ochre"}, "ops": ops})


offering_post()


def bone_house():
    """A house of mammoth, as the hunters build where there is no timber: a
    footing ring of skulls with their sockets showing, ribs arched over them,
    hides over the ribs, and five pairs of tusks standing clear of the whole so
    that what it is built of can be seen from across the camp."""
    r, c = 12, 18
    ops = [{"op": "cylinder", "base": [c, 0, c], "radius": r + 2, "height": 1, "mat": "floor", "part": "floor"}]

    def arc(p0, p1, p2, p3, n, r0, r1, mat, part):
        """A line of spheres down a cubic curve, and its mirror in x."""
        out = []
        for k in range(n + 1):
            t = k / n
            u = 1 - t
            x, y, z = (u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * cc + t * t * t * d
                       for a, b, cc, d in zip(p0, p1, p2, p3))
            rr = r0 + (r1 - r0) * t
            for sx in (x, 2 * c - x):
                out.append({"op": "ellipsoid", "centre": [round(sx, 1), round(y, 1), round(z, 1)],
                            "radius": [round(rr, 2)] * 3, "mat": mat, "part": part})
        return out

    # the footing ring: eight skulls, each a dome with a dark socket in its face
    for k in range(8):
        a = k * math.tau / 8 + 0.4
        px, pz = c + r * math.cos(a), c + r * math.sin(a)
        ox, oz = math.cos(a) * 2.6, math.sin(a) * 2.6
        ops.append({"op": "ellipsoid", "centre": [round(px, 1), 3.0, round(pz, 1)],
                    "radius": [3.6, 3.2, 3.6], "mat": "bone", "part": "stone"})
        ops.append({"op": "ellipsoid", "centre": [round(px + ox, 1), 3.2, round(pz + oz, 1)],
                    "radius": [1.3, 1.5, 1.3], "mat": "socket", "part": "stone"})

    # ribs from the ring to the crown
    for k in range(10):
        a = k * math.tau / 10 + 0.15
        for t in range(9):
            u = t / 8
            rr = r * (1 - u * u * 0.9)
            y = 6 + 13 * math.sin(u * math.pi * 0.52)
            px, pz = c + rr * math.cos(a), c + rr * math.sin(a)
            ops.append({"op": "ellipsoid", "centre": [round(px, 1), round(y, 1), round(pz, 1)],
                        "radius": [1.1, 1.1, 1.1], "mat": "bone", "part": "frame"})

    # hides over the ribs, but only the upper courses: the bone shows below
    for t in range(6):
        u = (t + 2) / 9
        rr = r * (1 - u * u * 0.9) + 0.7
        y = 6 + 13 * math.sin(u * math.pi * 0.52)
        ops.append({"op": "cylinder", "base": [c, round(y), c], "radius": round(rr, 1), "height": 2,
                    "mat": "hide", "part": "cloth"})
        ops.append({"op": "cylinder", "base": [c, round(y), c], "radius": round(rr - 1.6, 1), "height": 2,
                    "mat": "air"})

    # five pairs of tusks arching over the dome, clear of the hides
    for k, a in enumerate([0.0, 1.05, 2.1, 3.15, 4.2]):
        bx, bz = c + (r - 1) * math.cos(a), c + (r - 1) * math.sin(a)
        tx, tz = c + 2.5 * math.cos(a + 0.5), c + 2.5 * math.sin(a + 0.5)
        ops += arc((bx, 5.0, bz),
                   (bx - (bx - c) * 0.1, 15.0, bz - (bz - c) * 0.1),
                   (tx + (bx - c) * 0.25, 25.0, tz + (bz - c) * 0.25),
                   (tx, 27.0, tz), 12, 1.6, 0.6, "bone", "frame")

    # the doorway on local -Z, with two great tusks crossing over it
    ops.append({"op": "box", "from": [c - 5, 0, c - r - 6], "to": [c + 5, 15, c - r + 5], "mat": "air"})
    ops += arc((c - 6.5, 1.0, c - r - 1), (c - 7.5, 10.0, c - r - 4),
               (c - 3.0, 19.0, c - r - 5), (c + 2.5, 20.0, c - r - 3), 14, 1.8, 0.7, "bone", "frame")
    ops.append({"op": "box", "from": [c - 5, 0, c - r - 6], "to": [c + 5, 1, c - r + 2],
                "mat": "floor", "part": "floor"})
    save("structures", {"name": "bone_house",
                        "notes": "A house of mammoth where there is no timber: a footing ring of eight skulls with their sockets showing, ribs arched over them, hides lashed over the upper ribs only, five pairs of tusks arching clear over the roof and two more crossing over the doorway. 6 m across; door on local -Z.",
                        "size": [2 * c, 32, 2 * c], "anchor": [c, 0, c], "foundation": 5,
                        "tags": ["windbarrow", "house"],
                        "slots": {"bone": "old_bone", "socket": "bone_floor", "hide": "rawhide",
                                  "floor": "packed_dirt"}, "ops": ops})


bone_house()


def bone_fence():
    """Ribs and jaws set upright in the chalk: a stock fence where the hazel
    does not grow."""
    ops = []
    x = 0
    k = 0
    while x < 38:
        h = 7 + (k % 3)
        lean = (k % 2) * 2 - 1
        for t in range(h):
            u = t / max(h - 1, 1)
            ops.append({"op": "ellipsoid", "centre": [round(x + 1 + lean * u * 1.2, 1), round(1 + t * 1.1, 1), 2.0],
                        "radius": [1.0 - 0.3 * u, 1.0, 1.0 - 0.3 * u], "mat": "bone", "part": "frame"})
        x += 3 + (k % 2)
        k += 1
    ops.append({"op": "box", "from": [0, 4, 1], "to": [38, 5, 3], "mat": "bone", "part": "frame"})
    save("structures", {"name": "bone_fence",
                        "notes": "A 10 m run of stock fence built of mammoth ribs set upright in the chalk with a rail lashed along them, for ground where no hazel grows. Long axis local X.",
                        "size": [40, 14, 5], "anchor": [19, 0, 2], "foundation": 4, "level": False,
                        "tags": ["windbarrow"], "slots": {"bone": "old_bone"}, "ops": ops})


bone_fence()


def bone_outcrop():
    """A mammoth coming out of the chalk: the dome of a skull, a tusk, and
    three ribs standing out of the ground where the hill has worn back."""
    ops = [
        {"op": "ellipsoid", "centre": [12, 2, 12], "radius": [5.5, 4.0, 5.0], "mat": "bone", "part": "stone"},
        {"op": "ellipsoid", "centre": [12, 1, 17], "radius": [3.0, 2.4, 3.2], "mat": "bone", "part": "stone"},
    ]
    for t in range(12):
        u = t / 11
        ops.append({"op": "ellipsoid",
                    "centre": [round(12 - 7 * u - 2 * u * u, 1), round(3 + 2.6 * math.sin(u * 2.2), 1),
                               round(9 - 5 * u + 3 * u * u, 1)],
                    "radius": [1.5 - 0.7 * u] * 3, "mat": "bone", "part": "stone"})
    for k, (bx, bz) in enumerate([(19, 10), (22, 14), (24, 19)]):
        for t in range(7):
            u = t / 6
            ops.append({"op": "ellipsoid",
                        "centre": [round(bx + 2.5 * u, 1), round(1 + 7 * math.sin(u * 1.5), 1),
                                   round(bz + 1.5 * u, 1)],
                        "radius": [1.1 - 0.3 * u] * 3, "mat": "bone", "part": "stone"})
    save("structures", {"name": "bone_outcrop",
                        "notes": "A mammoth weathering out of the chalk: the dome of a skull, one tusk curving out of the ground and three ribs standing clear of it. Settles on the ground; faces local -Z.",
                        "size": [30, 14, 26], "anchor": [14, 0, 13], "foundation": 3, "level": False,
                        "decays": False, "tags": ["windbarrow"], "slots": {"bone": "old_bone"}, "ops": ops})


bone_outcrop()


def skate_pillar():
    """The thirteenth pillar: the same stone as the Tepe, standing alone far
    out on the grass, and carved with a skate instead of a beast."""
    ops = []
    tee_pillar(ops, 10, 10, 26, True, thick=3, wide=7, carve_marks=False)
    # the skate: a diamond body across the broad face, with a tail
    for x0, x1, y in [(7, 13, 17), (5, 15, 18), (3, 17, 19), (5, 15, 20), (7, 13, 21)]:
        ops.append({"op": "box", "from": [x0, y, 7], "to": [x1, y + 1, 8], "mat": "paint", "part": "stone"})
    ops.append({"op": "box", "from": [9, 13, 7], "to": [11, 17, 8], "mat": "paint", "part": "stone"})
    ops.append({"op": "box", "from": [8, 12, 7], "to": [12, 13, 8], "mat": "paint", "part": "stone"})
    save("structures", {"name": "skate_pillar",
                        "notes": "The thirteenth pillar: a carved T-pillar of the Tepe stone, 7.5 m and the tallest of them, standing alone on the open grass of the empty north with a wind skate cut into its broad south face and ochred. Broad faces along local X, the carving on local -Z.",
                        "size": [22, 34, 22], "anchor": [10, 0, 10], "foundation": 10, "decays": False,
                        "level": False, "tags": ["windbarrow", "temple"],
                        "slots": {"pillar": "carved_limestone", "paint": "ochre"}, "ops": ops})


skate_pillar()


def wind_shaft():
    """What the maze is walked to: a square shaft dropping twenty metres into
    the chalk, collared in blocks, with ochre on the collar and air coming
    steadily up out of it. No flint was ever dug here."""
    c, top = 14, 80          # the anchor sits at ground level, 20 m above the floor
    ops = [
        # the shaft itself, cut from the floor to a little over the ground
        {"op": "box", "from": [c - 4, 2, c - 4], "to": [c + 4, top + 6, c + 4], "mat": "carve"},
        {"op": "box", "from": [c - 4, 1, c - 4], "to": [c + 4, 2, c + 4], "mat": "dark", "part": "floor"},
        # a chalk lining down the first few metres, so it reads as built
        {"op": "walls", "from": [c - 5, 2, c - 5], "to": [c + 5, top - 8, c + 5],
         "mat": "deep", "thick": 1, "part": "stone"},
        {"op": "walls", "from": [c - 5, top - 8, c - 5], "to": [c + 5, top + 1, c + 5],
         "mat": "stone", "thick": 1, "part": "stone"},
        # the collar: three courses of blocks round the mouth, ochre on the top
        {"op": "cylinder", "base": [c, top, c], "radius": 9.5, "height": 3, "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 4, top, c - 4], "to": [c + 4, top + 4, c + 4], "mat": "air"},
        {"op": "cylinder", "base": [c, top + 3, c], "radius": 9.5, "height": 1, "mat": "paint", "part": "stone"},
        {"op": "cylinder", "base": [c, top + 3, c], "radius": 7.5, "height": 1, "mat": "stone", "part": "stone"},
        {"op": "box", "from": [c - 4, top + 3, c - 4], "to": [c + 4, top + 4, c + 4], "mat": "air"},
    ]
    # two stones standing at the lip, one each side of the way in
    for sx in (-8, 6):
        ops.append({"op": "box", "from": [c + sx, top + 3, c - 10], "to": [c + sx + 2, top + 13, c - 8],
                    "mat": "pillar", "part": "stone"})
    ops.append({"op": "box", "from": [c - 9, top + 11, c - 10], "to": [c + 9, top + 13, c - 8],
                "mat": "pillar", "part": "stone"})
    save("structures", {"name": "wind_shaft",
                        "notes": "A shaft 2 m square cut twenty metres down into the chalk at the heart of the turf maze, collared in three courses of blocks with ochre laid along the top, and a lintel on two uprights over the way in (local -Z). Its anchor is at ground level, so it digs below where it is placed.",
                        "size": [2 * c, top + 16, 2 * c], "anchor": [c, top, c], "foundation": 4,
                        "decays": False, "tags": ["windbarrow", "barrow"],
                        "slots": {"stone": "chalk_block", "pillar": "carved_limestone", "paint": "ochre",
                                  "deep": "chalk_grey", "dark": "bone_floor"}, "ops": ops})


wind_shaft()


def cat_scratch():
    """A block of chalk with a long-toothed cat scratched and ochred on its
    face: a low body, a heavy head, two teeth hanging past the jaw. Much
    fresher work than the Tepe's."""
    ops = [
        {"op": "box", "from": [1, 0, 3], "to": [19, 13, 9], "mat": "stone", "part": "stone"},
        # body and back line
        {"op": "box", "from": [4, 6, 2], "to": [15, 7, 3], "mat": "paint", "part": "stone"},
        {"op": "box", "from": [4, 5, 2], "to": [5, 7, 3], "mat": "paint", "part": "stone"},
        # head, over the shoulders at the left
        {"op": "box", "from": [3, 7, 2], "to": [6, 10, 3], "mat": "paint", "part": "stone"},
        # the two teeth, hanging below the jaw
        {"op": "box", "from": [3, 4, 2], "to": [4, 7, 3], "mat": "paint", "part": "stone"},
        {"op": "box", "from": [5, 3, 2], "to": [6, 7, 3], "mat": "paint", "part": "stone"},
        # legs
        {"op": "box", "from": [6, 3, 2], "to": [7, 6, 3], "mat": "paint", "part": "stone"},
        {"op": "box", "from": [9, 3, 2], "to": [10, 6, 3], "mat": "paint", "part": "stone"},
        {"op": "box", "from": [12, 3, 2], "to": [13, 6, 3], "mat": "paint", "part": "stone"},
        # the tail, up over the rump
        {"op": "box", "from": [14, 7, 2], "to": [15, 10, 3], "mat": "paint", "part": "stone"},
        {"op": "box", "from": [15, 9, 2], "to": [17, 10, 3], "mat": "paint", "part": "stone"},
    ]
    save("structures", {"name": "cat_scratch",
                        "notes": "A chalk block with a long-toothed cat scratched into its face and ochred: a low body, a heavy head, two teeth past the jaw, the tail up. The carved face is on local -Z.",
                        "size": [20, 14, 12], "anchor": [10, 0, 6], "foundation": 4, "level": False,
                        "decays": False, "tags": ["windbarrow", "cave"],
                        "slots": {"stone": "chalk_grey", "paint": "ochre"}, "ops": ops})


cat_scratch()

# ---- creatures ----------------------------------------------------------------
# All built on library rigs with `like`, so they cost no new shape programs and
# reuse colour values where they can (dark [36,28,24], white [240,236,228]).

save("creatures", {"name": "przewalski_horse",
                   "notes": "graze: Przewalski's horse: dun, pale round the muzzle and belly, with a short upright black mane and a dark stripe down the back. Bands of mares graze the open grass and wheel away together.",
                   "colors": {"coat": [196, 162, 104], "belly": [228, 212, 178], "mane": [52, 42, 32],
                              "muzzle": [240, 236, 228], "dark": [36, 28, 24]},
                   "behaviour": {"movement": "graze", "walk_speed": 1.2, "flee_speed": 9.5, "wary": 20,
                                 "flees_from": ["player", "wolf", "sabretooth"],
                                 "group": {"size": [3, 8], "spacing": 3.4, "range": 22, "alignment": 1.0, "cohesion": 1.1}},
                   "like": "horse", "scale": 0.88,
                   "extra_ops": [
                       {"op": "recolour", "at": [15, 18, 11], "until": [17, 21, 25], "from": "coat", "to": "mane"},
                       {"op": "box", "from": [15, 26, 6], "to": [17, 28, 10], "mat": "mane", "part": "neck"},
                       {"op": "box", "from": [15, 23, 9], "to": [17, 25, 12], "mat": "mane", "part": "neck"}],
                   "vary": {"size": 0.07, "shade": 0.1, "hue": 0.04}})

save("creatures", {"name": "saiga",
                   "notes": "graze: Saiga: a pale sandy steppe antelope with a swollen down-turned nose and short ridged amber horns. Small groups out on the open grass; the quickest thing here to run.",
                   "colors": {"pronghorn": [206, 192, 158], "white": [240, 236, 228], "dark": [36, 28, 24],
                              "black": [202, 172, 104]},
                   "behaviour": {"movement": "graze", "walk_speed": 1.04, "flee_speed": 9.6, "wary": 28,
                                 "flees_from": ["player", "wolf", "sabretooth"],
                                 "group": {"size": [3, 9], "spacing": 2.8, "range": 20, "alignment": 1.0, "cohesion": 1.1}},
                   "like": "pronghorn", "scale": 0.9,
                   "extra_ops": [
                       {"op": "ellipsoid", "centre": [16.5, 22.5, 2.5], "radius": [2.4, 2.6, 2.2],
                        "mat": "pronghorn", "part": "head"},
                       {"op": "box", "from": [15, 20, 1], "to": [18, 21, 3], "mat": "dark", "part": "head"}],
                   "vary": {"size": 0.08, "shade": 0.08, "hue": 0.03}})

save("creatures", {"name": "sabretooth",
                   "notes": "skulk: Sabretoothed cat: heavier in the shoulder than a cougar and tawny with faint flank spots, a pair of long pale canines hanging past its jaw. Hunts horses and saiga, alone, and is rarely seen.",
                   "colors": {"coat": [198, 154, 98], "belly": [232, 222, 198], "muzzle": [210, 180, 140],
                              "dark": [36, 28, 24], "tusk": [232, 222, 194], "spot": [146, 110, 62]},
                   "behaviour": {"movement": "skulk", "walk_speed": 1.2, "flee_speed": 9.0, "wary": 24,
                                 "flees_from": ["player"],
                                 "hunts": ["przewalski_horse", "saiga", "deer", "rabbit"], "spawn_chance": 0.05},
                   "like": "cougar", "scale": 1.3,
                   "extra_ops": [
                       {"op": "box", "from": [15, 7, 1], "to": [16, 11, 2], "mat": "tusk", "part": "head"},
                       {"op": "box", "from": [17, 7, 1], "to": [18, 11, 2], "mat": "tusk", "part": "head"},
                       {"op": "set", "at": [15, 6, 2], "mat": "tusk", "part": "head"},
                       {"op": "set", "at": [17, 6, 2], "mat": "tusk", "part": "head"},
                       {"op": "ellipsoid", "centre": [16, 13, 12], "radius": [4.0, 3.0, 3.4], "mat": "coat"},
                       {"op": "set", "at": [13, 11, 15], "mat": "spot"},
                       {"op": "set", "at": [19, 11, 17], "mat": "spot"},
                       {"op": "set", "at": [13, 10, 20], "mat": "spot"},
                       {"op": "set", "at": [19, 12, 13], "mat": "spot"}],
                   "vary": {"size": 0.08, "shade": 0.1, "hue": 0.03}})

save("creatures", {"name": "cave_bear",
                   "notes": "graze: Cave bear: half again the bulk of a black bear, dark brown with a high shoulder hump and a pale muzzle, denning in the chalk caves and out at the mouths at dawn and dusk.",
                   "colors": {"bear": [86, 64, 44], "muzzle": [196, 170, 132], "dark": [36, 28, 24]},
                   "behaviour": {"movement": "graze", "walk_speed": 0.8, "flee_speed": 7.0, "wary": 15,
                                 "flees_from": ["player"], "spawn_chance": 0.3, "swims": True},
                   "like": "bear", "scale": 1.8,
                   "extra_ops": [
                       {"op": "ellipsoid", "centre": [16.5, 14.5, 13.5], "radius": [4.8, 3.2, 4.0], "mat": "bear"},
                       {"op": "box", "from": [13, 0, 9], "to": [16, 2, 14], "mat": "dark", "part": "leg_fl"},
                       {"op": "box", "from": [17, 0, 9], "to": [20, 2, 14], "mat": "dark", "part": "leg_fr"}],
                   "vary": {"size": 0.1, "shade": 0.1, "hue": 0.04}})


def horn_ops():
    """One pair of aurochs horns: out of the skull, forward and out, then up
    and in, as a line of small spheres so the sweep reads instead of stepping.
    Mirrored about the model centre."""
    P = [(12.6, 16.2, 5.2), (9.4, 16.4, 2.0), (6.6, 17.6, 0.8), (7.6, 21.6, 2.0)]

    def bez(t):
        u = 1 - t
        return tuple(u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * e
                     for a, b, c, e in zip(*P))

    ops = []
    N = 30
    for k in range(N + 1):
        t = k / N
        x, y, z = bez(t)
        r = 1.55 - 0.6 * t ** 0.7
        for sx in (x, 32.0 - x):
            ops.append({"op": "ellipsoid", "centre": [round(sx, 2), round(y, 2), round(z, 2)],
                        "radius": [round(r, 2)] * 3, "mat": "horn", "part": "head"})
    return ops


save("creatures", {"name": "corpse_candle",
                   "notes": "drift: A corpse candle over the barrow field: a cold yellow flame the size of a fist with a dim halo, drifting between the mounds after dark and pulsing slowly. The villages say it is the breath going by.",
                   "voxel": 0.03, "half_w": 0.1, "height": 0.1,
                   "colors": {"wisp": [252, 240, 168, 3.0], "halo": [214, 186, 96, 1.4]},
                   "behaviour": {"movement": "drift", "fly_height": 1.3, "blink": 0.09},
                   "like": "marsh_light", "scale": 1.0})

save("creatures", {"name": "sky_manta",
                   "notes": "undulate: One skate in twenty grown to three metres across, riding well above the rest where they skim the awns. A rare morph of wind_skate, so it stands in for one of them and is never spawned on its own.",
                   "colors": {"back": [146, 134, 98], "belly": [218, 210, 186], "glow": [198, 184, 124]},
                   "behaviour": {"movement": "undulate", "walk_speed": 3.0, "flee_speed": 5.0, "wary": 0,
                                 "fly_height": 7.5, "flight": "flap", "wingbeat": 0.32, "group": None},
                   "like": "wind_skate", "rarity": 0.05, "scale": 2.4,
                   "vary": {"size": 0.08, "shade": 0.08, "hue": 0.02}})

save("creatures", {"name": "aurochs",
                   "notes": "graze: The wild ox the barrow people hunt and have not tamed: black-brown, smooth-backed and taller at the shoulder than a man, with long horns sweeping forward and up. Two herds hold the whole country, so finding one is a day's walk and a sight when it happens.",
                   "colors": {"coat": [52, 42, 34], "wool": [64, 52, 42], "horn": [224, 212, 182],
                              "dark": [36, 28, 24]},
                   "behaviour": {"movement": "graze", "walk_speed": 0.75, "flee_speed": 8.0, "wary": 24,
                                 "flees_from": ["player"], "spawn_chance": 0.09,
                                 "group": {"size": [10, 18], "spacing": 2.6, "range": 30,
                                           "alignment": 1.0, "cohesion": 1.5}},
                   "like": "bison", "scale": 1.12,
                   "extra_ops": [
                       {"op": "recolour", "at": [10, 14, 6], "until": [22, 24, 18], "from": "wool", "to": "coat"},
                       *horn_ops()],
                   "vary": {"size": 0.08, "shade": 0.1, "hue": 0.03}})

save("creatures", {"name": "steppe_wolf",
                   "notes": "Grey wolf of the steppe: the same animal as the library wolf, but travelling in families of three to six, so a pack is met rarely and a lone wolf almost never. Hunts horses and saiga.",
                   "colors": {"coyote": [124, 118, 112], "rabbit_light": [210, 196, 176], "dark": [36, 28, 24]},
                   "behaviour": {"walk_speed": 1.5, "flee_speed": 8.5, "wary": 20,
                                 "hunts": ["przewalski_horse", "saiga", "rabbit"], "spawn_chance": 0.1,
                                 "swims": True,
                                 "group": {"size": [3, 6], "spacing": 2.6, "range": 18,
                                           "alignment": 1.0, "cohesion": 1.3}},
                   "like": "wolf", "scale": 1.0,
                   "vary": {"size": 0.08, "shade": 0.12, "hue": 0.02}})

save("creatures", {"name": "steppe_sheep",
                   "notes": "The sheep of the villages, kept in one big flock apiece rather than in knots of a few: twelve to thirty-six together, so a flock is a sight when you come on it and the downs are empty between.",
                   "colors": {"rabbit": [240, 236, 228], "rabbit_light": [240, 236, 228], "white": [240, 236, 228],
                              "fleece": [238, 232, 220], "face": [60, 56, 54], "dark": [36, 28, 24],
                              "antler": [214, 196, 160]},
                   "behaviour": {"movement": "graze", "walk_speed": 0.7, "flee_speed": 4.5, "wary": 9,
                                 "group": {"size": [12, 36], "spacing": 1.8, "range": 26,
                                           "alignment": 1.0, "cohesion": 1.6}},
                   "like": "sheep", "scale": 1.0})

save("creatures", {"name": "wind_skate",
                   "notes": "undulate: Wind skate: a straw-pale ray a metre across that rides the wind over the feathergrass, a hand above the awns, wings rippling. Nobody knows what it eats.",
                   "colors": {"back": [186, 170, 112], "belly": [230, 222, 196], "glow": [214, 198, 136]},
                   "behaviour": {"movement": "undulate", "walk_speed": 2.4, "flee_speed": 4.6, "wary": 6,
                                 "fly_height": 2.2, "flight": "flap", "wingbeat": 0.6, "group": None},
                   "like": "sky_ray", "scale": 0.82,
                   "vary": {"size": 0.1, "shade": 0.1, "hue": 0.03}})

# ---- collectibles -------------------------------------------------------------

save("collectibles", {"name": "flint_blade", "display": "flint blade",
                      "notes": "A long blade struck from a flint core, blue-black with a white heel: on the working floors and all along the trails.",
                      "voxel": 0.02,
                      "colors": {"flint": [58, 60, 66], "cortex": [196, 190, 176]},
                      "ops": [{"op": "ellipsoid", "centre": [16, 2, 16], "radius": [2.4, 1.6, 9], "mat": "flint"},
                              {"op": "ellipsoid", "centre": [16, 2, 24], "radius": [2.6, 1.6, 2.4], "mat": "cortex"}]})

save("collectibles", {"name": "antler_pick", "display": "antler pick",
                      "notes": "A red-deer antler with its tines cut back to one, worn smooth at the grip: the tool the flint comes out with.",
                      "voxel": 0.022,
                      "colors": {"antler": [206, 186, 148], "antler_dark": [160, 138, 102]},
                      "ops": [{"op": "ellipsoid", "centre": [16, 2, 16], "radius": [1.8, 1.6, 9], "mat": "antler"},
                              {"op": "box", "from": [15, 1, 7], "to": [17, 3, 10], "mat": "antler_dark"},
                              {"op": "box", "from": [12, 1, 8], "to": [15, 3, 10], "mat": "antler"},
                              {"op": "box", "from": [16, 1, 22], "to": [20, 3, 24], "mat": "antler"}]})

save("collectibles", {"name": "ivory_shard", "display": "splinter of ivory",
                      "notes": "A curved splinter off a mammoth tusk, weathered out of the chalk: cream going brown along the cracks.",
                      "voxel": 0.02,
                      "colors": {"tooth": [226, 216, 190], "tooth_dark": [168, 150, 118]},
                      "ops": [{"op": "ellipsoid", "centre": [16, 2, 12], "radius": [1.8, 1.5, 5], "mat": "tooth"},
                              {"op": "ellipsoid", "centre": [17.5, 2, 19], "radius": [1.5, 1.3, 4], "mat": "tooth"},
                              {"op": "ellipsoid", "centre": [19.5, 2, 24], "radius": [1.2, 1.1, 2.4], "mat": "tooth_dark"}]})

save("collectibles", {"name": "bear_tooth", "display": "bear's tooth",
                      "notes": "A cave bear's canine, as long as a finger, with a hole bored through the root: picked up round the cave mouths and worn.",
                      "voxel": 0.018,
                      "colors": {"tooth": [226, 216, 190], "tooth_dark": [168, 150, 118]},
                      "ops": [{"op": "ellipsoid", "centre": [16, 3, 16], "radius": [2.2, 2.4, 7], "mat": "tooth"},
                              {"op": "ellipsoid", "centre": [16, 3, 22], "radius": [2.8, 2.8, 3.0], "mat": "tooth_dark"},
                              {"op": "cylinder", "base": [16, 0, 22], "radius": 1.0, "height": 7, "mat": "tooth_dark"}]})

# ---- biomes -------------------------------------------------------------------


def p(plant, density, verb="scatter", **params):
    d = {"plant": plant, "density": density, "placement": {"verb": verb, "params": params}}
    if "field" in params:
        d["field"] = params.pop("field")
    return d


def grad(plant, density, field, lo, hi):
    return {"plant": plant, "density": density,
            "placement": {"verb": "gradient", "params": {"min": lo, "max": hi}, "field": field}}


def c(name, count):
    return {"creature": name, "count": count}


def f(structure, density, **params):
    d = {"structure": structure, "density": density}
    if params:
        d["placement"] = {"verb": "scatter", "params": params}
    return d


def item(name, density, verb="scatter", **params):
    return {"item": name, "density": density, "placement": {"verb": verb, "params": params}}


# The herds of the open grass: horses, saiga, mammoth, and what hunts them.
STEPPE_LIFE = [c("przewalski_horse", 4), c("saiga", 5), c("mammoth", 4), c("steppe_wolf", 4), c("eagle", 2),
               c("sabretooth", 1), c("wind_skate", 5), c("marmot", 7), c("rabbit", 3),
               c("starling", 20),
               c("sparrow", 4), c("lizard", 4), c("bee", 4), c("crow", 3)]

save("biomes", {"name": "feathergrass_sea",
                "notes": "The open steppe: waist-high feathergrass gone gold over chalk, big tussocks standing out of it, a hawthorn or a juniper every few hundred metres. The default of Windbarrow.",
                "priority": 1, "rules": {}, "surface": "feathergrass", "subsurface": "chalk_white",
                "rock": None, "map_color": [196, 176, 104],
                "plants": [
                    p("feather_tussock", 0.022, "clustered", size=26, min=0.3, max=1.0),
                    p("hawthorn", 0.0004, "lone", spacing=40),
                    p("juniper", 0.0005, "lone", spacing=34),
                    p("birch", 0.0005, "clustered", size=15, min=0.56, max=1.0),
                    p("alder", 0.0003, "clustered", size=13, min=0.62, max=1.0),
                    p("sagebrush", 0.007, "clustered", size=18, min=0.0, max=0.8),
                    p("heather", 0.003, "clustered", size=12, min=0.2, max=1.0),
                ],
                "features": [f("boulder", 0.00008), f("flat_rock", 0.00012)],
                "creatures": STEPPE_LIFE,
                "collectibles": [item("flint_blade", 0.0002)]})

save("biomes", {"name": "chalk_down",
                "notes": "The crowns and steeper flanks of the downs, grazed short: thin green-gold turf with the chalk coming through, thyme and juniper, no tall grass.",
                "priority": 6, "rules": {"slope": [0.34, 1.5]}, "surface": "downland_grass",
                "subsurface": "chalk_white", "rock": None, "map_color": [176, 178, 120],
                "plants": [
                    p("juniper", 0.0016, "clustered", size=20, min=0.0, max=0.9),
                    p("hawthorn", 0.0009, "lone", spacing=26),
                    p("heather", 0.01, "clustered", size=14, min=0.2, max=1.0),
                    p("feather_tussock", 0.012, "clustered", size=16, min=0.0, max=0.7),
                    p("rock_lichen", 0.3),
                ],
                "features": [f("boulder", 0.0002), f("rock_outcrop", 0.00008)],
                "creatures": [c("przewalski_horse", 2), c("saiga", 2), c("steppe_sheep", 7), c("eagle", 2),
                              c("steppe_wolf", 2),
                              c("marmot", 6), c("rabbit", 3), c("wind_skate", 2), c("starling", 14), c("sparrow", 6),
                              c("lizard", 5), c("fox", 2)],
                "collectibles": [item("flint_blade", 0.0003)]})

save("biomes", {"name": "scarp_face",
                "notes": "The face of the Barrow Edge and the walls of its combes: bare white chalk with the flint band running level along it, junipers out of the cracks, lichen.",
                "priority": 10, "rules": {"slope": [1.5, 50]}, "surface": "chalk_rubble",
                "subsurface": "chalk_white", "rock": "chalk_white", "map_color": [214, 210, 196],
                "plants": [
                    p("juniper", 0.004, "clustered", size=12, min=0.0, max=1.0),
                    p("rock_lichen", 0.5),
                    p("hanging_vine", 0.08),
                ],
                "features": [f("boulder", 0.0006)],
                "creatures": [c("eagle", 3), c("great_horned_owl", 1), c("lizard", 6), c("marmot", 5),
                              c("bat", 8), c("sparrow", 6)],
                "collectibles": [item("flint_blade", 0.0006)]})

save("biomes", {"name": "flint_down",
                "notes": "The high ground east of the Edge: short sparse grass over chalk rubble, more flint than soil, the diggers' pits and spoil, and the cave mouths under it.",
                "priority": 12, "rules": {"region": "flint"}, "surface": "flint_grass",
                "subsurface": "chalk_white", "rock": "chalk_white", "map_color": [176, 170, 128],
                "plants": [
                    p("feather_tussock", 0.01, "clustered", size=14, min=0.0, max=0.6),
                    p("sagebrush", 0.006, "clustered", size=16, min=0.0, max=0.9),
                    p("juniper", 0.0012, "lone", spacing=24),
                    p("heather", 0.005, "clustered", size=10, min=0.2, max=1.0),
                ],
                "features": [f("boulder", 0.0004), f("flat_rock", 0.0003)],
                "creatures": [c("cave_bear", 3), c("steppe_wolf", 2), c("eagle", 2), c("marmot", 8), c("rabbit", 2),
                              c("lizard", 6), c("bat", 10), c("crow", 4), c("saiga", 2), c("sparrow", 3)],
                "collectibles": [item("flint_blade", 0.0012), item("antler_pick", 0.0003),
                                 item("bear_tooth", 0.0002)]})

save("biomes", {"name": "swale",
                "notes": "The damp bottoms of the dry valleys, where the soil is deep: feathergrass shoulder-high and greener, big tussocks, a birch or a willow in the wettest heads.",
                "priority": 8, "rules": {"moist": [0.62, 1.0], "slope": [0, 0.2]}, "surface": "feathergrass",
                "subsurface": "dirt", "rock": None, "map_color": [168, 170, 92],
                "plants": [
                    p("feather_tussock", 0.1, "clustered", size=30, min=0.3, max=1.0),
                    p("birch", 0.0022, "clustered", size=20, min=0.35, max=1.0),
                    p("alder", 0.0012, "clustered", size=16, min=0.45, max=1.0),
                    p("hawthorn", 0.0012, "lone", spacing=24),
                    p("sagebrush", 0.002),
                    p("broken_snag", 0.0003),
                ],
                "creatures": [c("przewalski_horse", 3), c("saiga", 3), c("mammoth", 3), c("deer", 5), c("aurochs", 14),
                              c("steppe_wolf", 3),
                              c("wind_skate", 6), c("rabbit", 3), c("badger", 2), c("starling", 20),
                              c("sparrow", 12), c("dragonfly", 4), c("bumblebee", 4)],
                "collectibles": [item("flint_blade", 0.0002)]})

save("biomes", {"name": "narrow_banks",
                "notes": "The green line of the Narrow: willow and alder leaning downwind, reedmace in the slack water, long meadow grass, and everything in the country coming down to drink.",
                "priority": 14, "rules": {"river": [0, 26]}, "surface": "meadow_grass", "subsurface": "dirt",
                "rock": None, "map_color": [96, 150, 76],
                "plants": [
                    p("willow", 0.012, "riparian", width=16),
                    p("alder", 0.01, "riparian", width=20),
                    p("birch", 0.004, "riparian", width=24),
                    p("reedmace", 0.05, "riparian", width=7),
                    p("feather_tussock", 0.02, "clustered", size=14, min=0.2, max=1.0),
                    p("tree_moss", 0.2),
                ],
                "features": [f("mossy_log", 0.0004)],
                "creatures": [c("przewalski_horse", 2), c("mammoth", 2), c("deer", 5), c("boar", 3), c("otter", 2),
                              c("heron", 3), c("goose", 8), c("mallard", 8), c("swan", 2), c("kingfisher", 2),
                              c("dragonfly", 10), c("meadowhawk", 6), c("frog", 6), c("steppe_wolf", 2), c("corpse_candle", 2),
                              c("starling", 14), c("sparrow", 12), c("bumblebee", 5), c("bat", 6)],
                "collectibles": [item("flint_blade", 0.0003), item("pottery_shard", 0.0002)]})

save("biomes", {"name": "barrow_field",
                "notes": "The barrow ridge: turf kept short by the herds over nineteen mounds, chalk kerbs showing through it, oak posts on the crowns, and nothing growing taller than a thistle.",
                "priority": 16, "rules": {"region": "barrows"}, "surface": "downland_grass",
                "subsurface": "chalk_white", "rock": None, "map_color": [186, 180, 132],
                "plants": [
                    p("heather", 0.008, "clustered", size=10, min=0.3, max=1.0),
                    p("feather_tussock", 0.008, "clustered", size=12, min=0.0, max=0.5),
                    p("hawthorn", 0.0005, "lone", spacing=34),
                ],
                "creatures": [c("saiga", 2), c("przewalski_horse", 1), c("steppe_sheep", 5), c("eagle", 2),
                              c("corpse_candle", 9),
                              c("rabbit", 3), c("marmot", 4), c("wind_skate", 3), c("starling", 12),
                              c("sparrow", 6), c("crow", 3), c("fox", 1)],
                "collectibles": [item("flint_blade", 0.0005), item("pottery_shard", 0.0004)]})

save("biomes", {"name": "green_kurgan",
                "notes": "The one mound in the field that is green when the rest are gold, and the ring of ground round it: thick wet-looking turf, and no grazing marks on it.",
                "priority": 20, "rules": {"region": "green_kurgan"}, "surface": "moss_grass",
                "subsurface": "dirt", "rock": None, "map_color": [80, 170, 90],
                "plants": [
                    p("heather", 0.004, "clustered", size=6, min=0.4, max=1.0),
                ],
                "creatures": [c("wind_skate", 3), c("sparrow", 4), c("bumblebee", 4)],
                "collectibles": [item("pottery_shard", 0.001)]})

save("biomes", {"name": "tepe_ground",
                "notes": "The hilltop round the Tepe: turf grazed and trodden down to the chalk, thyme and thistles at the wall's foot, and the pillars standing in it. The bare rubble is the enclosure's own floor, inside the bench wall.",
                "priority": 18, "rules": {"region": "tepe"}, "surface": "downland_grass",
                "subsurface": "chalk_white", "rock": None, "map_color": [200, 196, 150],
                "plants": [
                    p("heather", 0.03, "clustered", size=9, min=0.2, max=1.0),
                    p("feather_tussock", 0.02, "clustered", size=11, min=0.35, max=1.0),
                    p("sagebrush", 0.006, "clustered", size=12, min=0.3, max=1.0),
                    p("hawthorn", 0.0006, "lone", spacing=26),
                    p("rock_lichen", 0.3),
                ],
                "features": [f("boulder", 0.0004), f("flat_rock", 0.0005)],
                "creatures": [c("eagle", 2), c("crow", 4), c("starling", 10), c("sparrow", 6), c("lizard", 4)],
                "collectibles": [item("flint_blade", 0.0008), item("pottery_shard", 0.0008)]})

save("biomes", {"name": "steading",
                "notes": "The ground of a Windbarrow village: yards trodden to bare chalk between the round houses, barley plots behind hurdles, hearths, racks and pens, cattle and sheep standing about.",
                "priority": 22, "rules": {"region": "steading"}, "surface": "downland_grass",
                "subsurface": "chalk_white", "rock": None, "map_color": [172, 146, 110],
                "plants": [
                    p("feather_tussock", 0.012, "clustered", size=8, min=0.0, max=0.6),
                    p("hawthorn", 0.0008, "lone", spacing=22),
                ],
                "creatures": [c("cow", 6), c("steppe_sheep", 10), c("przewalski_horse", 2), c("goose", 6), c("rabbit", 2),
                              c("sparrow", 12), c("starling", 16), c("crow", 4), c("bee", 6)],
                "collectibles": [item("pottery_shard", 0.002), item("flint_blade", 0.0012),
                                 item("antler_pick", 0.0004)]})

save("biomes", {"name": "barley_plot",
                "notes": "A barley plot behind its hurdles: thin ranks of pale ears on worked chalk soil, a few thistles at the fence line.",
                "priority": 24, "rules": {"region": "barley"}, "surface": "barley_grass",
                "subsurface": "dirt", "rock": None, "map_color": [198, 184, 124],
                "plants": [], "features": [],
                "creatures": [c("sparrow", 14), c("starling", 10), c("crow", 3), c("rabbit", 2),
                              c("bumblebee", 4)],
                "collectibles": [item("flint_blade", 0.0008)]})

save("biomes", {"name": "oak_ground",
                "notes": "The few metres round the great oak below Durn: the same turf as the village keeps, and one tree in it. A small region plus `lone` spacing wider than the region is how a world plants exactly one tree.",
                "priority": 26, "rules": {"region": "greatoak"}, "surface": "downland_grass",
                "subsurface": "dirt", "rock": None, "map_color": [120, 150, 80],
                "plants": [p("mother_oak", 0.02, "lone", spacing=40)],
                "creatures": [c("sparrow", 8), c("starling", 10), c("crow", 2), c("bee", 4)],
                "collectibles": [item("pottery_shard", 0.001)]})

save("biomes", {"name": "frost_hollow",
                "notes": "A hollow in the downs where the grass is rimed white and glittering at any hour of a high-summer afternoon. Nothing is wrong with it but that; the sheep will not go down into it.",
                "priority": 17, "rules": {"region": "frost"}, "surface": "frost_grass",
                "subsurface": "chalk_white", "rock": None, "map_color": [216, 224, 224],
                "plants": [
                    p("feather_tussock", 0.012, "clustered", size=9, min=0.4, max=1.0),
                    p("sagebrush", 0.004, "clustered", size=10, min=0.4, max=1.0),
                    p("hawthorn", 0.0004, "lone", spacing=30),
                ],
                "features": [f("boulder", 0.0005), f("flat_rock", 0.0004)],
                "creatures": [c("corpse_candle", 5), c("crow", 3), c("wind_skate", 2), c("sparrow", 3),
                              c("marmot", 2)],
                "collectibles": [item("flint_blade", 0.0004)],
                "mist": 0.22})

save("biomes", {"name": "bone_down",
                "notes": "The Bone Downs: a stretch of the Flint Down where it is mammoth bone that weathers out of the chalk and not flint. Skulls and ribs stand out of the rubble, ivory splinters lie about, and the diggers will not work it.",
                "priority": 13, "rules": {"region": "bone"}, "surface": "bone_rubble",
                "subsurface": "chalk_white", "rock": "chalk_white", "map_color": [204, 196, 168],
                "plants": [
                    p("feather_tussock", 0.006, "clustered", size=10, min=0.3, max=0.8),
                    p("sagebrush", 0.004, "clustered", size=12, min=0.2, max=0.9),
                    p("juniper", 0.0008, "lone", spacing=28),
                ],
                "features": [f("bone_outcrop", 0.0009, seat=2.0), f("boulder", 0.0004)],
                "creatures": [c("crow", 6), c("eagle", 2), c("cave_bear", 1), c("marmot", 4),
                              c("steppe_wolf", 1), c("lizard", 4), c("wind_skate", 2)],
                "collectibles": [item("ivory_shard", 0.0016), item("bear_tooth", 0.0005),
                                 item("flint_blade", 0.0004)]})

save("biomes", {"name": "salt_shore",
                "notes": "Round Saltmere: a white crust of salt in plates over grey mud, low grey scrub back from it, and nothing else growing.",
                "priority": 15, "rules": {"region": "saltmere"}, "surface": "salt_crust",
                "subsurface": "salt_mud", "rock": None, "map_color": [222, 216, 200],
                "plants": [
                    p("sagebrush", 0.012, "clustered", size=22, min=0.0, max=0.9),
                    p("feather_tussock", 0.004, "clustered", size=10, min=0.0, max=0.4),
                ],
                "creatures": [c("goose", 10), c("heron", 3), c("swan", 2), c("saiga", 2),
                              c("przewalski_horse", 1), c("crow", 4), c("starling", 10), c("wind_skate", 2),
                              c("meadowhawk", 4)],
                "water": {"shallow": [152, 168, 156], "deep": [86, 104, 100]},
                "collectibles": [item("flint_blade", 0.0002)]})

save("biomes", {"name": "salt_bed",
                "notes": "The bed of Saltmere: grey salt mud under a metre or two of still, milky water.",
                "priority": 4, "rules": {"underwater": True}, "surface": "salt_mud", "subsurface": "salt_mud",
                "rock": "chalk_grey", "map_color": [150, 160, 158],
                "plants": [], "creatures": []})

print(f"wrote {len(written)} entries")
for w in written:
    print("  " + w)
