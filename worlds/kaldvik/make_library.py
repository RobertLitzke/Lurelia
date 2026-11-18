"""Kaldvik's hamlet and Christmas market: materials, structures, and the
chronicle events that place them (with their lamps and chimney smoke).

Run from the 3D_game folder: python worlds/kaldvik/make_library.py
It writes library/materials/*.json and library/structures/*.json for the
entries below, and replaces the events in worlds/kaldvik/chronicle.json whose
cause starts with HAMLET (so it can be run again).

Structures are in voxels, four to a metre; the front (door) is local -Z and
faces the bearing a `place` gives.
"""
import json
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
LIB = os.path.join(ROOT, "library")
TAG = "HAMLET"


def write(kind, d):
    d = {"name": d["name"], "version": 1, "introduced_by": "kaldvik", **{k: v for k, v in d.items() if k != "name"}}
    with open(os.path.join(LIB, kind, d["name"] + ".json"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps(d, indent=2) + "\n")


def mat(name, notes, pattern, base, alt, scale=5, contrast=0.6, **kw):
    calm = {"base": [min(255, c + 8) for c in base], "alt": [min(255, c + 8) for c in alt]}
    write("materials", {"name": name, "notes": notes, "smooth": False, **kw,
                        "texture": {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast, "calm": calm}})


# ---- materials --------------------------------------------------------------
mat("falu_red", "Falu red board cladding: the iron-ochre paint of every rorbu and barn on the coast, weathered toward brick.", "planks", [142, 40, 32], [122, 34, 28])
mat("ochre_board", "Board cladding painted yellow ochre, the other colour of a northern hamlet.", "planks", [196, 150, 72], [176, 132, 62])
mat("window_warm", "A window with a lamp behind it: dark glass by day, warm amber from dusk.", "glass", [238, 176, 92], [252, 206, 128], 4, 0.6, glow=1.2, night_glow=True)
mat("fairy_light", "A string of small warm bulbs, lit from dusk.", "flat", [255, 226, 164], [255, 236, 190], 4, 0.2, glow=2.6, night_glow=True)
mat("bauble_red", "Red glass baubles and the lights in them.", "flat", [220, 40, 40], [240, 70, 60], 4, 0.2, glow=1.6, night_glow=True)
mat("star_gold", "The star on the market spruce.", "flat", [255, 214, 96], [255, 230, 140], 4, 0.2, glow=3.5, night_glow=True)
mat("lantern_glass", "A street lantern's glass, lit from dusk.", "glass", [246, 212, 150], [255, 228, 176], 4, 0.4, glow=2.2, night_glow=True)
mat("turf_roof", "A turf roof: grass sods over birch bark, brown and sere in winter.", "blotch", [92, 86, 58], [72, 70, 46], 6, 0.7)
mat("turf_wall", "Turf walls: blocks of sod laid in herringbone courses on a stone footing.", "coursed", [98, 84, 62], [80, 70, 52], 5, 0.7)
mat("stockfish", "Cod split and hung in pairs to dry in the frost: grey-gold, stiff as boards.", "speckle", [176, 160, 128], [146, 132, 104], 6, 0.6)
mat("xmas_spruce", "The market spruce's boughs, dense and dark.", "speckle", [34, 64, 44], [24, 50, 34], 6, 0.6)
mat("stall_red", "Red-and-white striped canvas over a market stall.", "strata", [196, 46, 40], [232, 222, 210], 3, 0.9)
mat("straw_bound", "Straw bound tight in bundles, golden and bristling.", "speckle", [214, 172, 92], [184, 142, 70], 6, 0.7)
mat("ribbon_red", "Red ribbon wound round a straw goat.", "flat", [196, 30, 36], [176, 24, 30], 4, 0.3)
mat("ship_hull", "A wooden ship's hull, black with tar and greenheart sheathing, rimed white where the spray froze on it.", "planks", [34, 34, 38], [48, 50, 56], 5, 0.6)
mat("ship_trim", "The ship's rail and deckhouse trim, once white, now the colour of old ice.", "planks", [196, 196, 188], [176, 178, 172], 5, 0.5)
mat("sail_frozen", "Canvas furled on the yards and frozen stiff, grey-white and crusted.", "blotch", [214, 216, 214], [190, 194, 194], 5, 0.6)
mat("ship_rigging", "Tarred spars and standing rigging, furred with hoar frost.", "grain", [52, 46, 40], [70, 64, 56], 5, 0.5)
mat("stall_green", "Green-and-white striped canvas over a market stall.", "strata", [40, 112, 70], [232, 228, 214], 3, 0.9)


# ---- structure helpers -------------------------------------------------------
def box(a, b, m, part=None):
    o = {"op": "box", "from": list(a), "to": list(b), "mat": m}
    if part:
        o["part"] = part
    return o


def gable(a, b, axis, m, rise=1, part="roof"):
    return {"op": "gable", "from": list(a), "to": list(b), "axis": axis, "mat": m, "rise": rise, "part": part}


def cyl(base, r, h, m, part=None):
    o = {"op": "cylinder", "base": list(base), "radius": r, "height": h, "mat": m}
    if part:
        o["part"] = part
    return o


def ell(c, r, m, part=None):
    o = {"op": "ellipsoid", "centre": list(c), "radius": list(r), "mat": m}
    if part:
        o["part"] = part
    return o


def house(name, notes, L, W, wall_h, wall, roof, trim="trim_white", stilts=0, chimney=0.25, door="door_red", tags=("kaldvik", "house"), rise=1):
    """A gabled house, ridge along x, door in the middle of the -z side.
    Returns the structure and its chimney top (local voxels)."""
    y0 = stilts
    ops = []
    if stilts:
        for x in range(1, L + 1, 10):
            for z in (1, W - 1):
                ops.append(box((x, 0, z), (x + 2, y0, z + 2), "weathered_planks", "timber"))
        ops.append(box((L - 1, 0, 1), (L + 1, y0, 3), "weathered_planks", "timber"))
        ops.append(box((L - 1, 0, W - 1), (L + 1, y0, W + 1), "weathered_planks", "timber"))
    eave = y0 + 1 + wall_h
    ridge = eave + (W + 2) // 2 * rise
    ops += [
        box((1, y0, 1), (L + 1, y0 + 1, W + 1), "weathered_planks", "floor"),
        {"op": "walls", "from": [1, y0 + 1, 1], "to": [L + 1, eave, W + 1], "mat": wall, "thick": 1, "part": "wall"},
        gable((0, eave, 0), (L + 2, eave, W + 2), "x", roof, rise),
        gable((1, eave, 1), (2, eave, W + 1), "x", wall, rise, "wall"),
        gable((L, eave, 1), (L + 1, eave, W + 1), "x", wall, rise, "wall"),
    ]
    for x, z in ((1, 1), (L, 1), (1, W), (L, W)):
        ops.append(box((x, y0 + 1, z), (x + 1, eave, z + 1), trim, "timber"))
    # Door and its step.
    dx = (L + 2) // 2
    ops += [box((dx - 2, y0 + 1, 1), (dx + 2, y0 + 10, 2), door, "timber"),
            box((dx - 3, y0 + 10, 1), (dx + 3, y0 + 11, 2), trim, "timber")]
    # Windows, two storeys if the walls are tall enough.
    rows = [y0 + 4] + ([y0 + 13] if wall_h >= 18 else [])
    for wy in rows:
        for x in range(5, L - 3, 9):
            if abs(x + 2 - dx) < 5 and wy == y0 + 4:
                continue
            for z in (1, W):
                ops.append(box((x, wy, z), (x + 4, wy + 4, z + 1), "window_warm", "glass"))
                ops.append(box((x - 1, wy - 1, z), (x + 5, wy, z + 1), trim, "timber"))
    for x in (1, L):
        ops.append(box((x, eave + 2, W // 2), (x + 1, eave + 6, W // 2 + 3), "window_warm", "glass"))
    cx = max(3, int(L * chimney))
    top = ridge + 4
    ops += [box((cx, eave - 4, W // 2), (cx + 3, top, W // 2 + 3), "fieldstone", "stone")]
    size = [L + 2, top + 1, W + 2]
    s = {"name": name, "notes": notes + " Door on local -Z.", "size": size, "anchor": [size[0] // 2, 0, size[2] // 2],
         "foundation": 6 if not stilts else 0, "tags": list(tags), "ops": ops}
    if stilts:
        s["base"] = "sea"
    else:
        s["level"] = True
    return s, (cx + 1.5, top, W // 2 + 1.5)


STRUCT = {}
CHIMNEY = {}


def add(s, chim=None):
    STRUCT[s["name"]] = s
    if chim:
        CHIMNEY[s["name"]] = chim


add(*house("rorbu", "A fisherman's rorbu: a Falu-red cabin on stilts at the water's edge, half over the fjord, white trim, a tarred roof.",
           32, 24, 14, "falu_red", "roof_tar", stilts=10, chimney=0.75))
add(*house("red_house", "A two-storey house in Falu red with white trim and a slate roof.", 36, 26, 20, "falu_red", "slate"))
add(*house("white_house", "A white-painted two-storey house, the merchant's, with a tarred roof.", 38, 26, 20, "white_siding", "roof_tar", trim="trim_white"))
add(*house("ochre_house", "A small ochre house with a turf roof.", 30, 24, 14, "ochre_board", "turf_roof"))

# Boathouse (naust): stone walls, turf roof, the big door on the gable end
# toward the water (local -X; place with the bearing to the shore + 90).
L, W = 40, 22
ops = [
    {"op": "walls", "from": [1, 0, 1], "to": [L + 1, 11, W + 1], "mat": "fieldstone", "thick": 2, "part": "stone"},
    gable((0, 11, 0), (L + 2, 11, W + 2), "x", "turf_roof"),
    gable((1, 11, 1), (2, 11, W + 1), "x", "weathered_planks", 1, "timber"),
    gable((L, 11, 1), (L + 1, 11, W + 1), "x", "weathered_planks", 1, "timber"),
    box((1, 0, 5), (3, 10, W - 3), "weathered_planks", "timber"),
]
add({"name": "naust", "notes": "A boathouse: low dry-stone walls under a turf roof, the boat door in the gable end that faces the water (local -X).",
     "size": [L + 2, 11 + 13, W + 2], "anchor": [(L + 2) // 2, 0, (W + 2) // 2], "foundation": 6, "level": True, "tags": ["kaldvik", "house"], "ops": ops})

# Icelandic turf house: long turf walls and roof, two white timber gables on
# the front, one with the door.
L, W = 44, 26
ops = [
    {"op": "walls", "from": [1, 0, 1], "to": [L + 1, 10, W + 1], "mat": "turf_wall", "thick": 3, "part": "wall"},
    gable((0, 10, 0), (L + 2, 10, W + 2), "x", "turf_roof"),
    gable((1, 10, 1), (2, 10, W + 1), "x", "turf_wall", 1, "wall"),
    gable((L, 10, 1), (L + 1, 10, W + 1), "x", "turf_wall", 1, "wall"),
]
for gx in (8, 26):
    ops += [box((gx, 0, 0), (gx + 12, 14, 4), "white_siding", "wall"),
            gable((gx - 1, 14, 0), (gx + 13, 14, 4), "z", "roof_tar", 1),
            gable((gx, 14, 0), (gx + 12, 14, 1), "z", "white_siding", 1, "wall"),
            box((gx + 4, 15, 0), (gx + 8, 18, 1), "window_warm", "glass"),
            box((gx + 2, 4, 0), (gx + 5, 8, 1), "window_warm", "glass")]
ops += [box((12, 0, 0), (16, 9, 1), "door_red", "timber")]
ops += [box((32, 9, 12), (35, 26, 15), "fieldstone", "stone")]
add({"name": "turf_house", "notes": "An Icelandic turf farmhouse: long sod walls and a grass roof, two white timber gables on the front, one with the door.",
     "size": [L + 2, 27, W + 2], "anchor": [(L + 2) // 2, 0, (W + 2) // 2], "foundation": 6, "level": True, "tags": ["kaldvik", "house"], "ops": ops},
    (33.5, 26, 13.5))

# White church: nave along z, tower and stepped spire at the front (-z).
NX, NZ0, NZ1, NW = 30, 14, 64, 30
ops = [
    box((0, 0, NZ0), (NW, 1, NZ1), "weathered_planks", "floor"),
    {"op": "walls", "from": [0, 1, NZ0], "to": [NW, 26, NZ1], "mat": "white_siding", "thick": 1, "part": "wall"},
    gable((-1, 26, NZ0 - 1), (NW + 1, 26, NZ1 + 1), "z", "roof_tar", 1),
    gable((0, 26, NZ1 - 1), (NW, 26, NZ1), "z", "white_siding", 1, "wall"),
]
for z in range(NZ0 + 5, NZ1 - 4, 9):
    for x in (0, NW - 1):
        ops.append(box((x, 8, z), (x + 1, 20, z + 3), "window_warm", "glass"))
ops += [
    box((8, 0, 2), (22, 50, 16), "white_siding", "wall"),
    box((12, 1, 2), (18, 13, 3), "door_red", "timber"),
    box((13, 36, 2), (17, 42, 3), "window_warm", "glass"),
    box((7, 50, 1), (23, 52, 17), "roof_tar", "roof"),
]
for k, (a, b) in enumerate([(8, 22), (10, 20), (12, 18), (13, 17), (14, 16)]):
    ops.append(box((a, 52 + k * 7, a - 6), (b, 59 + k * 7, b - 6), "roof_tar", "roof"))
ops.append(box((14, 87, 8), (16, 92, 10), "star_gold", "metal"))
add({"name": "white_church", "notes": "The hamlet's white wooden church: a plain nave with tall lamplit windows, a square tower with a tarred stepped spire, the door in the tower (local -Z).",
     "size": [NW + 2, 93, NZ1 + 2], "anchor": [NW // 2, 0, 9], "foundation": 6, "level": True, "tags": ["kaldvik", "house"], "ops": ops})

# Stockfish rack (hjell): A-frame posts and rails hung with split cod.
L = 44
ops = []
for x in range(0, L, 12):
    ops += [box((x, 0, 1), (x + 2, 22, 3), "weathered_planks", "timber"), box((x, 0, 9), (x + 2, 22, 11), "weathered_planks", "timber")]
for y in (10, 20):
    for z in (2, 10):
        ops.append(box((0, y, z), (L - 10, y + 1, z + 1), "weathered_planks", "timber"))
        for x in range(2, L - 12, 3):
            ops.append(box((x, y - 6 + (x % 2), z), (x + 1, y, z + 1), "stockfish", "cloth"))
add({"name": "stockfish_rack", "notes": "A hjell: a timber rack hung with split cod drying in the frost, in two tiers.",
     "size": [L, 23, 12], "anchor": [L // 2, 0, 6], "foundation": 3, "level": False, "decays": False, "tags": ["kaldvik"], "ops": ops})


def stall(name, canvas):
    ops = [
        box((0, 0, 0), (14, 1, 12), "weathered_planks", "floor"),
        box((0, 1, 0), (14, 5, 2), "weathered_planks", "timber"),
        box((0, 5, 0), (14, 6, 3), "trim_white", "timber"),
    ]
    for x, z in ((0, 0), (13, 0), (0, 11), (13, 11)):
        ops.append(box((x, 1, z), (x + 1, 11, z + 1), "weathered_planks", "timber"))
    ops += [box((0, 1, 11), (14, 10, 12), "weathered_planks", "wall"),
            gable((-1, 11, -1), (15, 11, 13), "x", canvas, 1, "cloth")]
    for x in range(0, 14, 2):
        ops.append(box((x, 10, -1), (x + 1, 11, 0), "fairy_light", "metal"))
    goods = ["stall_red", "ochre_board", "trim_white", "stockfish", "falu_red", "bauble_red"]
    for k, x in enumerate(range(1, 13, 3)):
        ops.append(box((x, 6, 0), (x + 2, 7 + k % 2, 2), goods[k % len(goods)], "cloth"))
    return {"name": name, "notes": "A Christmas-market stall: a counter under a striped canvas roof hung with a string of lights, goods on the counter. Counter on local -Z.",
            "size": [16, 19, 14], "anchor": [7, 0, 6], "foundation": 2, "level": True, "tags": ["kaldvik", "market"], "ops": ops}


add(stall("stall_red", "stall_red"))
add(stall("stall_green", "stall_green"))

# The market spruce: a cone of boughs in whorls, lights and baubles, a star.
ops = [cyl((12, 0, 12), 1.5, 8, "bark", "timber")]
tiers = 9
H = 40
for y in range(5, 5 + H, 2):
    t = (y - 5) / H
    whorl = ((y - 5) % 8) / 8.0
    r = (11.5 * (1.0 - t) + 1.0) * (0.82 + 0.18 * whorl)
    ops.append(cyl((12, y, 12), r, 2, "xmas_spruce", "cloth"))
for k in range(tiers):
    t = k / tiers
    r = 11.5 * (1.0 - t) + 0.5
    y = 6 + k * 4.2
    for j in range(int(r * 1.6)):
        a = j * 2.399 + k * 0.7
        m = "bauble_red" if (j + k) % 3 == 0 else "fairy_light"
        px, pz = int(12 + math.cos(a) * r), int(12 + math.sin(a) * r)
        ops.append(box((px, int(y + j % 3), pz), (px + 1, int(y + 1 + j % 3), pz + 1), m, "metal"))
top = int(6 + tiers * 4.2) + 2
ops += [box((11, top, 11), (14, top + 3, 14), "star_gold", "metal"), box((12, top - 1, 10), (13, top + 4, 15), "star_gold", "metal"),
        box((10, top + 1, 12), (15, top + 2, 13), "star_gold", "metal")]
add({"name": "market_spruce", "notes": "The market's Christmas tree: a tall spruce hung with warm lights and red baubles, a gold star on top.",
     "size": [25, top + 5, 25], "anchor": [12, 0, 12], "foundation": 2, "level": False, "decays": False, "tags": ["kaldvik", "market"], "ops": ops})

# Yule goat: a straw goat bound with red ribbon, three metres high.
ops = [
    ell((15, 12, 7), (10, 4.5, 4.2), "straw_bound", "cloth"),
    ell((24, 17, 7), (3.0, 5.5, 3.0), "straw_bound", "cloth"),
    ell((27, 22, 7), (4.0, 2.8, 2.6), "straw_bound", "cloth"),
    ell((30, 21, 7), (1.6, 1.4, 1.4), "straw_bound", "cloth"),
    ell((5, 15, 7), (2.4, 1.2, 1.2), "straw_bound", "cloth"),
]
# Horns: arcs of small blobs sweeping back and down.
for side in (4.5, 9.5):
    for k in range(7):
        a = k * 0.45
        ops.append(ell((25 - 3.5 * math.sin(a), 24 + 3.5 * math.cos(a) - 1, side), (1.1, 1.1, 1.1), "straw_bound", "cloth"))
for x in (8, 21):
    for z in (4, 10):
        ops.append(cyl((x, 0, z), 1.5, 10, "straw_bound", "cloth"))
# Ribbons: bands round the body and neck, a voxel wide.
for x in (10, 15, 20):
    ops.append(ell((x + 0.5, 12, 7), (1.0, 4.8, 4.5), "ribbon_red", "cloth"))
ops.append(ell((24, 15.5, 7), (3.3, 1.0, 3.3), "ribbon_red", "cloth"))
add({"name": "yule_goat", "notes": "A julbock: a straw goat three metres high bound in red ribbon, the market's mascot. Faces local +X.",
     "size": [34, 32, 15], "anchor": [17, 0, 7], "foundation": 1, "level": False, "decays": False, "tags": ["kaldvik", "market"], "ops": ops})

# The post is a box: a cylinder under a voxel in radius draws nothing, and
# the lantern was left floating.
ops = [box((2, 0, 2), (3, 12, 3), "roof_tar", "metal"), box((1, 0, 1), (4, 1, 4), "roof_tar", "metal"),
       box((1, 11, 1), (4, 12, 4), "roof_tar", "metal"), box((1, 12, 1), (4, 15, 4), "lantern_glass", "glass"),
       box((0, 15, 0), (5, 16, 5), "roof_tar", "metal"), box((2, 16, 2), (3, 17, 3), "roof_tar", "metal")]
add({"name": "lamp_post", "notes": "A street lantern on a black iron post, lit from dusk.", "size": [5, 18, 5], "anchor": [2, 0, 2], "foundation": 1,
     "level": False, "decays": False, "tags": ["kaldvik"], "ops": ops})


# ---- the beset ship -----------------------------------------------------------
# A three-masted barquentine of an old polar expedition, 44 m long, caught in
# the cove ice and left there: hull rising out of the floe, pressure ridges
# shoved up along her side, masts leaning with the list, sails frozen on the
# yards, icicles off every rail and spar. Long axis local X, bow at +X; the
# shape's y 0 is a metre under the sea (`base: sea`), the ice at about y 5.
import random
rng = random.Random(1914)
SL, SW, DECK = 176, 32, 22          # length, beam, deck height (voxels)
CX, CZ = 98, 44                     # hull centre in the shape
ops = [
    ell((CX, DECK, CZ), (SL / 2, DECK, SW / 2), "ship_hull", "wall"),
    box((0, DECK + 1, 0), (220, 170, 90), "air"),
    # Bulwarks: a ring of hull a metre high round the deck.
    ell((CX, DECK + 2.5, CZ), (SL / 2 - 1, 2.5, SW / 2 - 0.5), "ship_hull", "wall"),
    ell((CX, DECK + 3.4, CZ), (SL / 2 - 3, 2.6, SW / 2 - 2), "air"),
    ell((CX, DECK + 0.5, CZ), (SL / 2 - 3, 1.0, SW / 2 - 2), "weathered_planks", "floor"),
    # The rail.
    ell((CX, DECK + 5.2, CZ), (SL / 2 - 1, 0.8, SW / 2 - 0.5), "ship_trim", "timber"),
    ell((CX, DECK + 5.6, CZ), (SL / 2 - 2.5, 1.2, SW / 2 - 2), "air"),
]
# Deckhouses: the after cabin and the galley forward of the mainmast.
ops += [box((32, DECK + 1, CZ - 8), (58, DECK + 11, CZ + 8), "ship_trim", "wall"),
        box((31, DECK + 11, CZ - 9), (59, DECK + 12, CZ + 9), "ship_hull", "roof"),
        box((106, DECK + 1, CZ - 6), (122, DECK + 9, CZ + 6), "ship_trim", "wall"),
        box((105, DECK + 9, CZ - 7), (123, DECK + 10, CZ + 7), "ship_hull", "roof"),
        box((112, DECK + 10, CZ - 1), (114, DECK + 16, CZ + 1), "ship_hull", "stone")]
for x in (36, 42, 48, 54):
    ops.append(box((x, DECK + 5, CZ - 8), (x + 2, DECK + 8, CZ - 7), "window_glass", "glass"))
# The wheel aft.
ops.append(box((18, DECK + 1, CZ - 1), (20, DECK + 6, CZ + 1), "ship_rigging", "timber"))
ops.append({"op": "disc", "centre": [19, DECK + 7, CZ + 0.5], "radius": 3, "thick": 1, "axis": "x", "mat": "ship_rigging", "part": "timber"})
# Masts, leaning to starboard (+Z) with the list: stacked short lengths,
# each a little further over. (x, height, square-rigged)
LEAN = 0.11
masts = [(140, 116, True), (96, 126, False), (52, 102, False)]
tops = []
for mx, mh, square in masts:
    for y0 in range(DECK, DECK + mh, 6):
        dz = (y0 - DECK) * LEAN
        r = 2.2 if y0 < DECK + mh * 0.6 else 1.5
        ops.append(cyl((mx + 0.5, y0, CZ + 0.5 + dz), r, 6, "ship_rigging", "timber"))
    tops.append((mx, DECK + mh, CZ + mh * LEAN))
    if square:
        # Yards across the ship, sails furled and frozen on them.
        for k, (yy, half) in enumerate([(0.42, 30), (0.6, 26), (0.76, 21), (0.9, 15)]):
            y = DECK + mh * yy
            z0 = CZ + (y - DECK) * LEAN
            ops.append({"op": "log", "base": [mx + 0.5, y, z0 - half], "radius": 1.2, "length": half * 2, "axis": "z", "mat": "ship_rigging", "part": "timber"})
            ops.append(ell((mx + 0.5, y - 1.6, z0), (2.2, 2.0, half - 2), "sail_frozen", "cloth"))
            for j in range(int(half * 0.5)):
                iz = z0 - half + 2 + rng.random() * (half * 2 - 4)
                ops.append(cyl((mx + 0.5, y - 3.5 - rng.random() * 4, iz), 0.9, 4 + rng.random() * 5, "fall_ice", "cloth"))
    else:
        # Fore-and-aft: a boom and a gaff, the sail furled along the boom.
        for yy, ln in ((0.12, 34), (0.55, 26)):
            y = DECK + mh * yy
            z0 = CZ + (y - DECK) * LEAN
            ops.append({"op": "log", "base": [mx - ln, y, z0 + 0.5], "radius": 1.2, "length": ln, "axis": "x", "mat": "ship_rigging", "part": "timber"})
        y = DECK + mh * 0.12
        ops.append(ell((mx - 17, y + 2.2, CZ + (y - DECK) * LEAN + 0.5), (16, 2.0, 2.2), "sail_frozen", "cloth"))
# Standing rigging: shrouds from each masthead down to the rails either
# side, stays between the mastheads and down to the bowsprit and the stern,
# as runs of single voxels.
def line(a, b, m="ship_rigging"):
    n = int(max(abs(b[0] - a[0]), abs(b[1] - a[1]), abs(b[2] - a[2]))) + 1
    out, seen = [], set()
    for i in range(n + 1):
        t = i / n
        v = tuple(int(math.floor(a[k] + (b[k] - a[k]) * t)) for k in range(3))
        if v not in seen:
            seen.add(v)
            out.append(box(v, (v[0] + 1, v[1] + 1, v[2] + 1), m, "timber"))
    return out
for mx, top_y, top_z in tops:
    head = (mx + 0.5, top_y - 8, top_z)
    for dx in (-8, 0, 8):
        for side in (-1, 1):
            ops += line(head, (mx + dx, DECK + 5, CZ + side * (SW / 2 - 1)))
(fx, fy, fz), (mx_, my, mz), (ax, ay, az) = tops
ops += line((fx, fy - 4, fz), (CX + SL / 2 + 28, DECK + 15, CZ + 0.5))
ops += line((mx_, my - 6, mz), (fx, fy - 10, fz))
ops += line((ax, ay - 6, az), (mx_, my - 14, mz))
ops += line((ax, ay - 4, az), (CX - SL / 2 + 6, DECK + 5, CZ + 0.5))
# Bowsprit and jib-boom, rising over the bow.
for t in range(0, 36, 3):
    ops.append(cyl((CX + SL / 2 - 6 + t, DECK + 3 + t * 0.35, CZ + 0.5), 1.4, 3, "ship_rigging", "timber"))
# Icicles all along the rail and from the bowsprit.
for k in range(90):
    a = rng.random() * 2 * math.pi
    x, z = CX + math.cos(a) * (SL / 2 - 1.5), CZ + math.sin(a) * (SW / 2 - 0.5)
    ops.append(cyl((x, DECK + 6 - (2 + rng.random() * 6), z), 0.9 + rng.random() * 0.5, 2 + rng.random() * 6, "fall_ice", "cloth"))
# Pressure ridges: slabs of floe shoved up against her, heaviest on the port
# side that took the squeeze.
for side, count, height in ((-1, 26, 16), (1, 12, 9)):
    for k in range(count):
        x = 20 + rng.random() * (SL - 20)
        off = (SW / 2 + 2 + rng.random() * 8)
        z = CZ + side * off
        w, h, d = 6 + rng.random() * 10, 3 + rng.random() * height, 3 + rng.random() * 4
        lean = rng.choice([-1, 1])
        for j in range(3):
            ops.append(box((int(x + lean * j * 2), 4 + j * int(h / 3), int(z - d / 2 + side * j)),
                           (int(x + w + lean * j * 2), 4 + (j + 1) * int(h / 3) + 1, int(z + d / 2 + side * j)), "sea_ice", "stone"))
add({"name": "beset_ship", "notes": "An old polar expedition's barquentine, 44 m long, caught in the cove ice and abandoned: hull standing out of the floe, pressure ridges along her port side, masts leaning to starboard, sails frozen on the yards. Long axis local X, bow at +X; `base: sea`.",
     "size": [220, 160, 90], "anchor": [CX, 0, CZ], "base": "sea", "decays": False, "tags": ["kaldvik", "wreck"], "ops": ops})

for s in STRUCT.values():
    write("structures", s)

# ---- placement ----------------------------------------------------------------
# (structure, x, z, bearing)
PLACES = [
    ("rorbu", 858, 446, 270), ("rorbu", 859, 461, 270), ("rorbu", 860, 476, 270),
    ("naust", 860, 496, 0),
    ("stockfish_rack", 876, 432, 0), ("stockfish_rack", 878, 444, 0),
    ("red_house", 882, 503, 270), ("white_house", 905, 452, 270), ("ochre_house", 899, 514, 270),
    ("red_house", 916, 486, 270), ("turf_house", 924, 432, 270),
    ("white_church", 934, 506, 270),
    ("market_spruce", 890, 474, 0),
    ("stall_red", 879, 466, 90), ("stall_green", 879, 482, 90),
    ("stall_green", 901, 466, 270), ("stall_red", 901, 482, 270),
    ("stall_red", 890, 461, 180), ("stall_green", 890, 488, 0),
    ("yule_goat", 873, 475, 270),
    ("lamp_post", 877, 474, 0), ("lamp_post", 903, 474, 0), ("lamp_post", 890, 457, 0), ("lamp_post", 890, 493, 0),
    ("lamp_post", 868, 468, 0), ("lamp_post", 912, 496, 0),
    ("beset_ship", 378, 684, 90),
]


def rot_of(bearing):
    return int(((bearing % 360) + 45) // 90) % 4


def rotate(x, z, size, r):
    return [(x, z), (size[2] - 1 - z, x), (size[0] - 1 - x, size[2] - 1 - z), (z, size[0] - 1 - x)][r]


events = []
smoke = []
lamps = []
for name, x, z, b in PLACES:
    ev = {"year": 1700, "event": "place", "cause": f"{TAG}: the {name.replace('_', ' ')}", "structure": name, "at": [x, z], "bearing": b}
    if name == "beset_ship":
        ev["cause"] = f"{TAG}: the Nordstjerne, an expedition ship that wintered in Isvika in 1915 and never got out: her crew walked to the hamlet over the ice in the spring"
        ev["poi"] = "the Nordstjerne, beset"
    events.append(ev)
    s = STRUCT[name]
    if name in CHIMNEY:
        r = rot_of(b)
        size = s["size"]
        ax, az = rotate(s["anchor"][0], s["anchor"][2], size, r)
        cx, cy, cz = CHIMNEY[name]
        rx, rz = rotate(int(cx), int(cz), size, r)
        wx = (math.floor(x * 4) - ax + rx + 0.5) / 4
        wz = (math.floor(z * 4) - az + rz + 0.5) / 4
        smoke.append((wx, wz, cy / 4 + 0.2, s.get("base") == "sea"))
    if name == "lamp_post":
        lamps.append([x, z, 3.6])
    if name.startswith("stall"):
        lamps.append([x, z, 2.6])
lamps.append([890, 474, 6.0])

land = [[round(wx - 890, 2), round(wz - 474, 2), round(up, 2)] for wx, wz, up, sea in smoke if not sea]
sea = [[round(wx - 890, 2), round(wz - 474, 2), round(up - 1.0, 2)] for wx, wz, up, sea in smoke if sea]
events.append({"year": 1700, "event": "vent", "cause": f"{TAG}: smoke from every chimney in the hamlet", "at": [890, 474], "points": land, "fx": "chimney_smoke", "rate": 2.5, "spread": 0.3})
if sea:
    events.append({"year": 1700, "event": "vent", "cause": f"{TAG}: smoke from the rorbu stoves", "at": [890, 474], "points": sea, "fx": "chimney_smoke", "rate": 2.0, "spread": 0.3, "on_sea": True})
events.append({"year": 1700, "event": "lamp", "cause": f"{TAG}: the market's lights and the street lanterns, lit at dusk", "at": [890, 474],
               "points": [[round(px - 890, 1), round(pz - 474, 1), up] for px, pz, up in lamps], "reach": 13, "colour": [255, 206, 140], "strength": 1.7, "after_dark": True})
events.insert(0, {"year": 1600, "event": "region", "cause": f"{TAG}: the cleared ground the hamlet stands on, cut back from the spruce", "name": "hamlet", "at": [892, 476], "radius": 58})
events.append({"year": 1700, "event": "area", "cause": f"{TAG}: the hamlet", "name": "Kaldvik", "kind": "village", "at": [890, 474], "radius": 60})

write("fx", {"name": "chimney_smoke", "notes": "Woodsmoke from a stove in frost: grey-white, rising slowly and leaning with the wind.",
             "shape": "puff", "count": [1, 1], "speed": [0.5, 0.9], "cone_deg": 10, "life": [4.0, 7.0], "size": [0.2, 1.8], "size_jitter": 0.3,
             "gravity": -0.25, "drag": 0.5, "wind": 0.9, "color": [206, 206, 210], "alpha": 0.28, "near_fade": 1.0})

path = os.path.join(HERE, "chronicle.json")
c = json.load(open(path, encoding="utf-8"))
c["events"] = [e for e in c["events"] if not str(e.get("cause", "")).startswith(TAG)] + events
c["events"].sort(key=lambda e: e["year"])
c["places"]["market"] = [890, 474]
with open(path, "w", encoding="utf-8", newline="\n") as fh:
    json.dump(c, fh, indent=2)
print(f"{len(STRUCT)} structures, {len(events)} events, {len(land) + len(sea)} chimneys, {len(lamps)} lamps")
