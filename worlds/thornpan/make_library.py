"""Writes Thornpan's library entries (materials, plants, creatures,
structures, biomes) through libfmt.render so they come out canonical. The JSON
files in library/ are what the game reads. See the note below.

Thornpan reuses as much of the library as it can: salt_crust and salt_mud for
the pans, shore_sand for the channel beds, marsh_grass for the flood meadow,
cabbage_palm for the fan palms, round_house / granary / stock_pen /
drying_rack / net_shed for the villages, and elephant, lion, zebra,
water_buffalo, crocodile, heron, egret, eagle, macaque and the rest of the
fauna unchanged.

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

W = "thornpan"
written = []


def save(kind, d):
    d = {"name": d.pop("name"), "version": 1, "introduced_by": W, **d}
    path = os.path.join(ROOT, "library", kind, d["name"] + ".json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(libfmt.render(d, kind))
    written.append(f"{kind}/{d['name']}")


# ---- materials ---------------------------------------------------------------
# Four new ones. Everything else the world needs is already in the library.

def mat(name, notes, pattern, base, alt, calm_base, calm_alt, scale=8, contrast=1.0, **kw):
    tex = {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast,
           "calm": {"base": calm_base, "alt": calm_alt}}
    d = {"name": name, "notes": notes, "smooth": kw.pop("smooth", False), **kw, "texture": tex}
    save("materials", d)


# The country's cover. Meadow-tinted ground swings between sage and ochre over
# tens of metres (calm_tint in scene.wgsl multiplies by up to 1.3, 1.04, 0.58),
# so the base is kept low in chroma or the ochre patches come out pure yellow.
mat("savanna_grass",
    "Savanna grass: tall tussock grass gone gold in the dry, standing over pale sand that shows through between the tufts.",
    "noise", [150, 132, 84], [138, 120, 76], [170, 150, 102], [158, 138, 94], scale=6, contrast=0.7,
    smooth=True, grass={"sparse": 0.14, "dense": 0.9, "short": 3.4},
    top={"pattern": "noise", "base": [162, 142, 92], "alt": [150, 132, 84], "scale": 6, "contrast": 0.7,
         "calm": {"base": [182, 160, 110], "alt": [170, 150, 102]}})
mat("granite",
    "Granite: the country's floor where it stands up in domes, grey speckled light and dark, weathering off in curved shells. Drawn in facets so a whaleback reads as rock, not steps.",
    "speckle", [142, 140, 138], [110, 110, 112], [146, 145, 144], [116, 116, 118], scale=5, contrast=0.9,
    facet=True)
mat("baobab_bark",
    "Baobab bark: smooth, thick and pewter grey with a pink cast, pleated in soft vertical folds on a trunk far too wide for its height.",
    "noise", [146, 138, 128], [132, 124, 116], [156, 148, 138], [142, 134, 126], scale=7, contrast=0.5,
    smooth=True)
mat("acacia_leaf",
    "Acacia leaf: fine grey-green pinnate leaf, thin enough to see sky through, massed into a flat top.",
    "speckle", [116, 130, 98], [100, 114, 86], [126, 140, 108], [110, 124, 96], scale=6, contrast=0.6,
    foliage=True)
mat("barnacle_crust",
    "Barnacle crust: a metre-thick band of cemented shell plates on every rock face at thirty-four metres, the old lake's waterline. Grey-white, pitted, and wrong in a country with no sea.",
    "blotch", [192, 188, 176], [166, 160, 148], [196, 192, 182], [172, 166, 154], scale=4, contrast=0.8)

# ---- plants ------------------------------------------------------------------

save("plants", {
    "name": "baobab",
    "notes": "Baobab: a grey barrel of a trunk, three to five metres through, under a sparse crown of thick limbs held up like roots. Bare for much of the year; the crown is open enough to see sky through.",
    "form": "branching",
    "height": [13, 19],
    "trunk_radius": [1.7, 2.5],
    "buttress": 1.0,
    "crown_radius": [3.6, 5.0],
    "bark": "baobab_bark",
    "leaf": "live_oak_leaves",
    "curve": "straight",
    "canopy": "tufts",
    "leader": 0.1,
    "fill": 0.15,
    "spread": 0.9,
    "dome": 0.15,
    "limb": 0.55,
    "max_slope": 0.5,
    "affords": ["perch", "nest"],
})
save("plants", {
    "name": "dead_baobab",
    "notes": "A baobab that died standing: the same grey barrel with its limbs bare and broken short. On the pan rim they stand in groups, holding nothing.",
    "form": "snag",
    "height": [6, 10],
    "trunk_radius": [0.85, 1.35],
    "crown_radius": [3.2, 4.6],
    "bark": "baobab_bark",
    "max_slope": 0.5,
})
save("plants", {
    "name": "acacia",
    "notes": "Thorn acacia: a flat-topped umbrella of fine grey-green leaf on a short trunk, the crown wider than the tree is tall.",
    "form": "umbrella",
    "height": [5, 8.5],
    "trunk_radius": [0.22, 0.38],
    "crown_radius": [2.6, 3.8],
    "bark": "pale_bark",
    "leaf": "acacia_leaf",
    "squash": 0.5,
    "max_slope": 0.7,
    "affords": ["perch", "nest", "browse"],
})
save("plants", {
    "name": "mopane",
    "notes": "Mopane: a scrubby thicket of butterfly leaf on thin stems, head high, on the thinnest sand. Turns coppery in the dry.",
    "form": "shrub",
    "height": [2.2, 4.0],
    "trunk_radius": [0.1, 0.16],
    "crown_radius": [1.2, 1.9],
    "bark": "pale_bark",
    "leaf": "live_oak_leaves",
    "max_slope": 0.8,
})

# ---- creatures ---------------------------------------------------------------
# Five of the seven inherit a rig with `like`; the hippo and the giraffe are too
# far off any existing shape and carry their own ops and bones.

DARK = [36, 28, 24]
WHITE = [240, 236, 228]
HORN = [214, 196, 160]


def quadruped_rig(leg_y, low_y, legs, neck, head, tail, ears=None):
    """A quadruped rig from its joint heights. `legs` is
    {name: (x, z, claim_box)}, one per leg."""
    bones = [{"name": "body", "role": "root"},
             {"name": "neck", "role": "neck", "parent": "body", "joint": neck},
             {"name": "head", "role": "head", "parent": "neck", "joint": head}]
    for name, (x, z) in ears or {}:
        bones.append({"name": name, "role": "ear", "parent": "head", "joint": [x, head[1] + 2, z],
                      "side": name[-1]})
    for i, (name, (x, z, claim)) in enumerate(legs.items()):
        side = name[-1]
        pair = 0 if name[-2] == "f" else 1
        bones.append({"name": name, "role": "leg", "parent": "body", "joint": [x, leg_y, z], "side": side, "pair": pair})
        bones.append({"name": name + "_low", "role": "leg", "parent": name, "joint": [x, low_y, z],
                      "claims": [claim], "side": side, "pair": pair})
    bones.append({"name": "tail", "role": "tail", "parent": "body", "joint": tail})
    return {"plan": "quadruped", "bones": bones}


def leg(ops, name, x0, x1, z0, z1, y_top, hoof, mat="coat"):
    """A square leg from the ground to y_top, with a dark foot."""
    ops.append({"op": "box", "from": [x0, hoof, z0], "to": [x1, y_top, z1], "mat": mat, "part": name})
    ops.append({"op": "box", "from": [x0, 0, z0], "to": [x1, hoof, z1], "mat": "dark", "part": name})


# Hippo: a barrel on stumps, eyes and ears on top of the head so it can lie
# awash in a pool with the rest of it under.
ops = [
    {"op": "ellipsoid", "centre": [16, 9, 17], "radius": [6.4, 4.6, 8.8], "mat": "coat"},
    {"op": "recolour_below", "y": 7, "from": "coat", "to": "belly"},
]
leg(ops, "leg_fl", 11, 15, 8, 12, 8, 2)
leg(ops, "leg_fr", 17, 21, 8, 12, 8, 2)
leg(ops, "leg_hl", 11, 15, 21, 25, 8, 2)
leg(ops, "leg_hr", 17, 21, 21, 25, 8, 2)
ops += [
    {"op": "box", "from": [12, 6, 7], "to": [20, 13, 10], "mat": "coat", "part": "neck"},
    {"op": "ellipsoid", "centre": [16, 9.5, 5], "radius": [5.4, 3.6, 4.4], "mat": "coat", "part": "head"},
    {"op": "box", "from": [10, 6, 1], "to": [22, 11, 5], "mat": "coat", "part": "head"},
    {"op": "box", "from": [11, 6, 1], "to": [21, 8, 3], "mat": "nose", "part": "head"},
    {"op": "set", "at": [12, 11, 2], "mat": "dark", "part": "head"},
    {"op": "set", "at": [19, 11, 2], "mat": "dark", "part": "head"},
    {"op": "box", "from": [11, 12, 6], "to": [13, 14, 8], "mat": "coat", "part": "ear_l"},
    {"op": "box", "from": [19, 12, 6], "to": [21, 14, 8], "mat": "coat", "part": "ear_r"},
    {"op": "set", "at": [12, 12, 5], "mat": "dark", "part": "head"},
    {"op": "set", "at": [19, 12, 5], "mat": "dark", "part": "head"},
    {"op": "box", "from": [15, 8, 25], "to": [17, 11, 27], "mat": "coat", "part": "tail"},
]
save("creatures", {
    "name": "hippo",
    "notes": "Hippo: three tonnes of grey barrel on stumps, eyes, ears and nostrils set on the top of the head so it can lie in a pool with everything else under the water. Gives way to nothing and runs faster than it looks.",
    "voxel": 0.14,
    "half_w": 0.91,
    "height": 1.81,
    "colors": {"coat": [122, 106, 102], "belly": [160, 126, 118], "nose": [150, 110, 100], "dark": DARK},
    "behaviour": {
        "movement": "wade",
        "walk_speed": 0.5,
        "flee_speed": 7.0,
        "wary": 16,
        "flightiness": 0.25,
        "juvenile_flightiness": 0.5,
        "flees_from": ["player"],
        "swims": True,
        "group": {"size": [2, 6], "spacing": 3.0, "range": 14, "alignment": 0.8, "cohesion": 1.3},
    },
    "ops": ops,
    "vary": {"size": 0.1, "shade": 0.06, "hue": 0.02},
    "rig": quadruped_rig(
        8, 4,
        {"leg_fl": (13, 10, [[11, 0, 8], [15, 4, 12]]),
         "leg_fr": (19, 10, [[17, 0, 8], [21, 4, 12]]),
         "leg_hl": (13, 23, [[11, 0, 21], [15, 4, 25]]),
         "leg_hr": (19, 23, [[17, 0, 21], [21, 4, 25]])},
        [16, 9, 10], [16, 9.5, 7.5], [16, 10, 24.5],
        ears=[("ear_l", (12, 7)), ("ear_r", (20, 7))],
    ),
})

# Giraffe: all of its height is leg and neck, so the body is small and sits
# high. 32 voxels at 0.155 m is 4.96 m to the top of the ossicones.
ops = [
    {"op": "ellipsoid", "centre": [16, 18, 18], "radius": [3.4, 3.2, 5.4], "mat": "coat"},
    {"op": "ellipsoid", "centre": [16, 19.5, 13], "radius": [3.3, 3.4, 3.4], "mat": "coat"},
    {"op": "recolour_below", "y": 16, "from": "coat", "to": "belly"},
]
leg(ops, "leg_fl", 13, 15, 12, 14, 17, 2)
leg(ops, "leg_fr", 18, 20, 12, 14, 17, 2)
leg(ops, "leg_hl", 13, 15, 21, 23, 16, 2)
leg(ops, "leg_hr", 18, 20, 21, 23, 16, 2)
# The neck steps forward as it rises; the dapple is the coat and belly split,
# so the pattern comes from `vary` rather than per-plate colours.
neck = [(21, 11), (22, 11), (23, 10), (24, 10), (25, 9), (26, 9), (27, 8)]
for y, z in neck:
    ops.append({"op": "box", "from": [14, y, z], "to": [18, y + 1, z + 4], "mat": "coat", "part": "neck"})
ops += [
    {"op": "box", "from": [14, 28, 4], "to": [18, 31, 10], "mat": "coat", "part": "head"},
    {"op": "box", "from": [15, 28, 2], "to": [18, 30, 4], "mat": "nose", "part": "head"},
    {"op": "set", "at": [14, 30, 6], "mat": "dark", "part": "head"},
    {"op": "set", "at": [17, 30, 6], "mat": "dark", "part": "head"},
    {"op": "box", "from": [13, 30, 7], "to": [15, 32, 9], "mat": "coat", "part": "ear_l"},
    {"op": "box", "from": [17, 30, 7], "to": [19, 32, 9], "mat": "coat", "part": "ear_r"},
    {"op": "box", "from": [15, 31, 5], "to": [16, 33, 6], "mat": "horn", "part": "head"},
    {"op": "box", "from": [16, 31, 5], "to": [17, 33, 6], "mat": "horn", "part": "head"},
    {"op": "box", "from": [15, 16, 23], "to": [17, 20, 25], "mat": "coat", "part": "tail"},
    {"op": "box", "from": [15, 13, 24], "to": [17, 16, 26], "mat": "dark", "part": "tail"},
]
save("creatures", {
    "name": "giraffe",
    "notes": "Giraffe: five metres of leg and neck with a small body slung between them, browsing the tops of the thorn trees. Walks both legs of a side together and has no interest in people at all.",
    "voxel": 0.155,
    "half_w": 0.5,
    "height": 4.9,
    "colors": {"coat": [176, 128, 74], "belly": [226, 210, 180], "nose": [150, 110, 100], "horn": HORN, "dark": DARK},
    "behaviour": {
        "movement": "graze",
        "walk_speed": 0.8,
        "flee_speed": 9.0,
        "wary": 20,
        "flightiness": 0.5,
        "juvenile_flightiness": 0.8,
        "flees_from": ["player"],
        "group": {"size": [2, 5], "spacing": 6.0, "range": 26, "alignment": 0.6, "cohesion": 0.8},
    },
    "ops": ops,
    "vary": {"size": 0.12, "shade": 0.1, "hue": 0.03},
    "rig": quadruped_rig(
        17, 9,
        {"leg_fl": (14, 13, [[13, 0, 12], [15, 9, 14]]),
         "leg_fr": (19, 13, [[18, 0, 12], [20, 9, 14]]),
         "leg_hl": (14, 22, [[13, 0, 21], [15, 9, 23]]),
         "leg_hr": (19, 22, [[18, 0, 21], [20, 9, 23]])},
        [16, 20, 11.5], [16, 28, 9], [16, 18, 23],
        ears=[("ear_l", (14, 8)), ("ear_r", (18, 8))],
    ),
})

# Lechwe: a deer-sized antelope of the flooded grass, chestnut with white
# stockings, and the horns swept back instead of antlers.
save("creatures", {
    "name": "lechwe",
    "notes": "Red lechwe: a chestnut antelope of the flood meadows, hocks always wet, running in splashing herds through water a foot deep. The males carry long horns swept back in a shallow lyre.",
    "like": "deer",
    "scale": 0.95,
    "colors": {"coat": [172, 108, 52], "belly": WHITE, "antler": HORN},
    "behaviour": {
        "movement": "wade",
        "walk_speed": 1.2,
        "flee_speed": 9.0,
        "wary": 22,
        "flightiness": 1.6,
        "swims": True,
        "group": {"size": [8, 30], "spacing": 2.4, "range": 48, "alignment": 1.3, "cohesion": 1.5},
    },
    "extra_ops": [
        # Off with the antlers, on with a pair of back-swept horns. The carve
        # takes the ears with it, so they go back on after it.
        {"op": "box", "from": [12, 25, 4], "to": [22, 32, 9], "mat": "air", "part": "head"},
        {"op": "box", "from": [12, 25, 7], "to": [14, 27, 8], "mat": "coat", "part": "ear_l"},
        {"op": "box", "from": [19, 25, 7], "to": [21, 27, 8], "mat": "coat", "part": "ear_r"},
        {"op": "box", "from": [15, 26, 6], "to": [16, 29, 7], "mat": "antler", "part": "head"},
        {"op": "box", "from": [17, 26, 6], "to": [18, 29, 7], "mat": "antler", "part": "head"},
        {"op": "set", "at": [15, 29, 7], "mat": "antler", "part": "head"},
        {"op": "set", "at": [17, 29, 7], "mat": "antler", "part": "head"},
        {"op": "set", "at": [15, 30, 8], "mat": "antler", "part": "head"},
        {"op": "set", "at": [17, 30, 8], "mat": "antler", "part": "head"},
        {"op": "set", "at": [15, 31, 7], "mat": "antler", "part": "head"},
        {"op": "set", "at": [17, 31, 7], "mat": "antler", "part": "head"},
    ],
})

# Wildebeest: the bison rig is already humped and heavy-shouldered, which is
# most of a wildebeest; it wants the colour, a beard and cow horns.
save("creatures", {
    "name": "wildebeest",
    "notes": "Blue wildebeest: slate grey, high in the shoulder and low in the head, with a black mane and beard and horns that hook out and up. Grazes the dry grass in long strung-out herds.",
    "like": "bison",
    "scale": 0.8,
    "colors": {"coat": [96, 94, 98], "wool": [78, 76, 82], "horn": [150, 142, 128], "dark": DARK},
    "behaviour": {
        "walk_speed": 1.0,
        "flee_speed": 11.0,
        "wary": 24,
        "flightiness": 1.1,
        "group": {"size": [10, 30], "spacing": 3.0, "range": 55, "alignment": 1.4, "cohesion": 1.6},
    },
    "extra_ops": [
        {"op": "box", "from": [10, 14, 3], "to": [22, 18, 7], "mat": "air", "part": "head"},
        # Horns out sideways from the crown, then up.
        {"op": "box", "from": [11, 14, 4], "to": [14, 15, 6], "mat": "horn", "part": "head"},
        {"op": "box", "from": [18, 14, 4], "to": [21, 15, 6], "mat": "horn", "part": "head"},
        {"op": "box", "from": [11, 15, 4], "to": [12, 17, 6], "mat": "horn", "part": "head"},
        {"op": "box", "from": [20, 15, 4], "to": [21, 17, 6], "mat": "horn", "part": "head"},
        # Beard under the jaw and a mane down the neck.
        {"op": "box", "from": [15, 7, 3], "to": [17, 10, 6], "mat": "dark", "part": "head"},
        {"op": "box", "from": [15, 16, 6], "to": [17, 18, 11], "mat": "dark", "part": "neck"},
    ],
})

# Warthog: the boar with a bigger head, tusks and a brush tail.
save("creatures", {
    "name": "warthog",
    "notes": "Warthog: grey, bristled and comic, head down in the grass with its tail straight up when it trots off. Everywhere in the country and the first thing most people see.",
    "like": "boar",
    "scale": 1.05,
    "colors": {"coat": [110, 100, 92], "belly": [128, 118, 108], "mane": [52, 44, 38], "tusk": WHITE, "dark": DARK},
    "behaviour": {
        "walk_speed": 1.1,
        "flee_speed": 8.0,
        "wary": 14,
        "flightiness": 1.5,
        "group": {"size": [2, 6], "spacing": 1.8, "range": 12, "alignment": 1.0, "cohesion": 1.1},
    },
})

# Carmine bee-eater: the kingfisher reworked. Carmine body, teal head, a long
# tail streamer, and it nests in thousands in a cut bank.
save("creatures", {
    "name": "bee_eater",
    "notes": "Carmine bee-eater: carmine body, blue-green head and a long tail, hunting insects out over the water all morning. Nests in holes in a cut bank, in colonies of hundreds, and follows any grass fire for what it drives up.",
    "like": "sparrow",
    "scale": 1.1,
    "colors": {
        "body": [196, 62, 72],
        "head": [64, 150, 160],
        "wing": [176, 56, 66],
        "tail": [150, 48, 58],
        "breast": [206, 86, 86],
        "head_cap": [72, 164, 172],
        "beak": DARK,
    },
    "behaviour": {
        "movement": "flock",
        "walk_speed": 7.0,
        "flee_speed": 13.0,
        "wary": 10,
        "flightiness": 1.4,
        "group": {"size": [14, 40], "spacing": 1.4, "range": 12, "separation": 1.0, "alignment": 0.9, "cohesion": 1.0},
    },
})

# Vulture: the condor, browner and smaller, over whatever the lions left.
save("creatures", {
    "name": "vulture",
    "notes": "White-backed vulture: a brown soaring plank with a bald head, circling anything dead. Where there are three in the air there is a kill under them.",
    "like": "condor",
    "scale": 0.82,
    "colors": {"body": [78, 68, 58], "wing": [62, 54, 48], "tail": [54, 48, 44], "head": [196, 188, 176], "white": WHITE},
    "behaviour": {"movement": "soar", "walk_speed": 7.0, "flee_speed": 14.0, "wary": 30, "flightiness": 0.8},
})

# ---- structures --------------------------------------------------------------


def peg_ladder(ops, x, z, y0, y1, step=4, mat="wood"):
    """Pegs hammered into a trunk, alternating sides of the climb line."""
    for i, y in enumerate(range(y0, y1, step)):
        dx = 1 if i % 2 else -1
        ops.append({"op": "box", "from": [x + dx, y, z], "to": [x + dx + 1, y + 1, z + 3],
                    "mat": mat, "part": "timber"})


def baobab_shell(r_out, r_in, h, door=None, slab=False):
    """A hollow baobab: a barrel that flares at the foot and tapers to the
    crown, a room inside it, and five limbs leaving the top, each a chain of
    blocks stepping up and out like a root held in the air. Local centre is
    (r_out, *, r_out) and the door faces -Z (the structure's front)."""
    c = r_out
    ops = []
    # The barrel: a flare at the foot, a near-straight shaft, and a taper into
    # a rounded crown. Cylinders of a few fixed radii, so the silhouette is
    # clean instead of a staircase of one-voxel steps.
    flare = max(2, int(h * 0.12))
    shaft = int(h * 0.74)
    ops.append({"op": "cylinder", "base": [c, 0, c], "radius": round(r_out * 1.1, 2), "height": flare,
                "mat": "bark", "part": "wall"})
    ops.append({"op": "cylinder", "base": [c, flare - 1, c], "radius": r_out, "height": shaft - flare + 1,
                "mat": "bark", "part": "wall"})
    ops.append({"op": "cylinder", "base": [c, shaft - 1, c], "radius": round(r_out * 0.84, 2),
                "height": h - shaft + 1, "mat": "bark", "part": "wall"})
    ops.append({"op": "ellipsoid", "centre": [c, h, c], "radius": [round(r_out * 0.84, 2), round(h * 0.1, 1), round(r_out * 0.84, 2)],
                "mat": "bark", "part": "wall"})
    # The room inside, and a floor of beaten earth.
    ops.append({"op": "cylinder", "base": [c, 1, c], "radius": r_in, "height": h - 6, "mat": "air"})
    ops.append({"op": "ellipsoid", "centre": [c, h - 6, c], "radius": [r_in, round(h * 0.12, 1), r_in], "mat": "air"})
    ops.append({"op": "cylinder", "base": [c, 0, c], "radius": r_in, "height": 1, "mat": "floor", "part": "floor"})
    # Six limbs leaving the crown, out and up, thinning as they go: a
    # baobab's limbs are stubby and root-like, not long.
    for k in range(6):
        a = k * 2.0 * math.pi / 6 + 0.4
        x, y, z = c, h, c
        rad = r_out * 0.24
        for step in range(8):
            x += math.cos(a) * rad * 1.2
            z += math.sin(a) * rad * 1.2
            y += rad * 1.25
            rad *= 0.80
            if rad < 0.8:
                break
            ops.append({"op": "ellipsoid", "centre": [round(x, 1), round(y, 1), round(z, 1)],
                        "radius": [round(rad, 2)] * 3, "mat": "bark", "part": "timber"})
    if door:
        w, dh = door
        ops.append({"op": "box", "from": [c - w // 2, 1, 0], "to": [c + w // 2, 1 + dh, c], "mat": "air"})
    if slab:
        ops.append({"op": "box", "from": [c - 3, 1, 0], "to": [c + 3, 11, 3], "mat": "air"})
        ops.append({"op": "box", "from": [c - 3, 1, 1], "to": [c + 3, 10, 3], "mat": "stone", "part": "stone"})
    return ops


R = 20  # trunk radius in voxels: a 10 m barrel
H = 54  # 13.5 m to the crown, before the limbs

ops = baobab_shell(R, 10, H, door=(6, 11))
# Benches round the inside wall and a hearth scar in the middle.
ops.append({"op": "cylinder", "base": [R, 1, R], "radius": 9, "height": 2, "mat": "wood", "part": "timber"})
ops.append({"op": "cylinder", "base": [R, 1, R], "radius": 6.5, "height": 2, "mat": "air"})
ops.append({"op": "cylinder", "base": [R, 1, R], "radius": 3, "height": 1, "mat": "ash", "part": "floor"})
save("structures", {
    "name": "hollow_baobab",
    "notes": "The council tree: a hollow baobab open at the bottom on one side, with benches round the inside wall and a hearth scar in the middle. Big enough for a dozen people. Face the doorway at a village centre; it carves nothing.",
    "size": [2 * R + 1, H + 26, 2 * R + 1],
    "anchor": [R, 0, R],
    "level": False,
    "decays": False,
    "tags": ["natural", "baobab", "shelter"],
    "slots": {"bark": "baobab_bark", "wood": "weathered_planks", "floor": "packed_dirt", "ash": "charcoal"},
    "ops": ops,
})

ops = baobab_shell(17, 9, 46, door=(5, 9))
# A sealed basin of water in the bottom, and pegs up the outside to the hole
# the water goes in by.
ops.append({"op": "cylinder", "base": [17, 1, 17], "radius": 8, "height": 5, "mat": "clay", "part": "floor"})
ops.append({"op": "cylinder", "base": [17, 2, 17], "radius": 6.5, "height": 4, "mat": "water"})
peg_ladder(ops, 17, 1, 6, 42)
save("structures", {
    "name": "baobab_cistern",
    "notes": "The cistern tree: a hollow baobab sealed with clay at the bottom and kept full of water against the end of the dry, with pegs hammered up the outside and a low door to dip from. Face the door downhill toward the path.",
    "size": [35, 70, 35],
    "anchor": [17, 0, 17],
    "level": False,
    "decays": False,
    "tags": ["natural", "baobab", "water"],
    "slots": {"bark": "baobab_bark", "wood": "oak_post", "floor": "packed_dirt", "clay": "daub", "water": "river_water", "ash": "charcoal"},
    "ops": ops,
})

ops = baobab_shell(16, 8, 42, slab=True)
ops.append({"op": "box", "from": [12, 1, 12], "to": [20, 3, 20], "mat": "floor", "part": "floor"})
save("structures", {
    "name": "baobab_tomb",
    "notes": "The tomb tree: a hollow baobab closed with a dressed slab, two generations of one family inside. Face the slab toward open ground; nothing opens it.",
    "size": [33, 66, 33],
    "anchor": [16, 0, 16],
    "level": False,
    "decays": False,
    "tags": ["natural", "baobab", "tomb"],
    "slots": {"bark": "baobab_bark", "wood": "oak_post", "floor": "packed_dirt", "stone": "fieldstone", "ash": "charcoal"},
    "ops": ops,
})

save("structures", {
    "name": "granite_stack",
    "notes": "Granite boulders stacked three and four high as the dome they split off weathered away, the top one overhanging. Scatter on and round a kopje; it levels nothing.",
    "size": [26, 34, 26],
    "anchor": [13, 0, 13],
    "level": False,
    "decays": False,
    "tags": ["natural", "granite"],
    "slots": {"rock": "granite", "lichen": "rock_lichen"},
    "ops": [
        {"op": "ellipsoid", "centre": [13, 6, 13], "radius": [11, 7, 10], "mat": "rock"},
        {"op": "ellipsoid", "centre": [12, 15, 14], "radius": [8, 6, 7.5], "mat": "rock"},
        {"op": "ellipsoid", "centre": [15, 23, 12], "radius": [6, 5, 5.5], "mat": "rock"},
        {"op": "ellipsoid", "centre": [11, 28, 13], "radius": [3.5, 3, 3.5], "mat": "lichen"},
    ],
})

ops = [{"op": "cylinder", "base": [8, 0, 8], "radius": 7.5, "height": 2, "mat": "clay", "part": "wall"}]
for i, r in enumerate([6.5, 5.5, 4.5, 3.5, 2.5, 1.8]):
    ops.append({"op": "cylinder", "base": [8, 2 + i * 2, 8], "radius": r, "height": 2, "mat": "clay", "part": "wall"})
ops += [
    {"op": "box", "from": [8, 13, 7], "to": [9, 17, 8], "mat": "clay", "part": "wall"},
    {"op": "box", "from": [6, 12, 9], "to": [7, 15, 10], "mat": "clay", "part": "wall"},
]
save("structures", {
    "name": "termite_mound",
    "notes": "A termite mound: a hard clay cone chest to head high with a chimney or two off the crown, standing in the grass. Scatter on dry ground; it levels nothing.",
    "size": [17, 18, 17],
    "anchor": [8, 0, 8],
    "level": False,
    "decays": False,
    "tags": ["natural"],
    "slots": {"clay": "packed_dirt"},
    "ops": ops,
})

ops = [
    {"op": "log", "base": [3, 1, 2], "radius": 2.6, "length": 24, "axis": "z", "mat": "wood", "part": "timber"},
    {"op": "log", "base": [3, 3, 3], "radius": 1.9, "length": 22, "axis": "z", "mat": "air"},
    {"op": "box", "from": [1, 0, 1], "to": [6, 2, 4], "mat": "wood", "part": "timber"},
    {"op": "box", "from": [1, 0, 24], "to": [6, 2, 27], "mat": "wood", "part": "timber"},
    {"op": "box", "from": [0, 1, 12], "to": [7, 2, 14], "mat": "wood", "part": "timber"},
    {"op": "box", "from": [5, 2, 6], "to": [6, 3, 22], "mat": "pole", "part": "timber"},
]
save("structures", {
    "name": "mokoro",
    "notes": "A mokoro: a dugout canoe six metres long, drawn up on the sand with its pole laid along the thwart. Place in twos and threes at a landing, bow to the water.",
    "size": [8, 5, 29],
    "anchor": [3, 0, 14],
    "level": False,
    "decays": True,
    "tags": ["boat"],
    "slots": {"wood": "dead_wood", "pole": "oak_post"},
    "ops": ops,
})

ops = [{"op": "box", "from": [2, 0, 2], "to": [5, 11, 5], "mat": "post", "part": "timber"}]
for y in range(2, 11, 2):
    ops.append({"op": "box", "from": [2, y, 1], "to": [5, y + 1, 2], "mat": "post", "part": "timber"})
ops.append({"op": "box", "from": [1, 7, 1], "to": [6, 8, 2], "mat": "post", "part": "timber"})
save("structures", {
    "name": "flood_post",
    "notes": "The flood post at a landing: a squared post with the year's high water cut into it as notches, the two that overtopped it marked by a crosspiece. Face the notched side at the water.",
    "size": [7, 12, 7],
    "anchor": [3, 0, 3],
    "level": True,
    "decays": True,
    "tags": ["marker"],
    "slots": {"post": "oak_post"},
    "ops": ops,
})

# The bee-eater bank: a cut earth face with a thousand holes in it, carved back
# into the river bank. The front faces the water.
ops = [{"op": "box", "from": [0, 0, 0], "to": [60, 16, 8], "mat": "earth", "part": "wall"}]
for i in range(14):
    for j in range(5):
        x = 3 + i * 4 + (2 if j % 2 else 0)
        y = 3 + j * 2
        ops.append({"op": "box", "from": [x, y, 0], "to": [x + 2, y + 2, 5], "mat": "air"})
ops.append({"op": "box", "from": [0, 0, 0], "to": [60, 2, 6], "mat": "sand", "part": "floor"})
save("structures", {
    "name": "bee_bank",
    "notes": "A carmine bee-eater colony: fifteen metres of cut earth bank honeycombed with nest holes, a sand apron at its foot. Face it at the water on the outside of a bend; place bee_eaters with it.",
    "size": [61, 17, 9],
    "anchor": [30, 0, 8],
    "foundation": 8,
    "level": False,
    "decays": False,
    "tags": ["natural", "nest"],
    "slots": {"earth": "packed_dirt", "sand": "shore_sand"},
    "ops": ops,
})

# Wreck one: frames out of the reeds, barnacled to the rail. The ribs are
# drawn as rings, the ring's lower half cut away, and the keel laid in last so
# it survives the cutting.
ops = []
for i in range(11):
    z = 5 + i * 4
    r = 7 - abs(i - 5) * 0.7
    ops += [
        {"op": "disc", "centre": [10, 3, z], "radius": r, "thick": 1.4, "axis": "z", "mat": "plank", "part": "timber"},
        {"op": "disc", "centre": [10, 3, z], "radius": r - 1.3, "thick": 1.8, "axis": "z", "mat": "air"},
        {"op": "box", "from": [0, 0, z - 2], "to": [21, 3, z + 2], "mat": "air"},
    ]
ops += [
    {"op": "box", "from": [8, 0, 2], "to": [12, 4, 46], "mat": "plank", "part": "timber"},
    {"op": "box", "from": [8, 0, 2], "to": [12, 2, 46], "mat": "shell", "part": "timber"},
]
# Barnacle crust up to the turn of the bilge on every frame.
for i in range(11):
    z = 5 + i * 4
    r = 7 - abs(i - 5) * 0.7
    ops += [
        {"op": "disc", "centre": [10, 3, z], "radius": r, "thick": 1.4, "axis": "z", "mat": "shell", "part": "timber"},
        {"op": "disc", "centre": [10, 3, z], "radius": r - 1.3, "thick": 1.8, "axis": "z", "mat": "air"},
        {"op": "box", "from": [0, 0, z - 2], "to": [21, 3, z + 2], "mat": "air"},
        {"op": "box", "from": [0, 6, z - 2], "to": [21, 14, z + 2], "mat": "plank", "part": "timber"},
        {"op": "disc", "centre": [10, 3, z], "radius": r - 1.3, "thick": 1.8, "axis": "z", "mat": "air"},
        {"op": "box", "from": [0, 0, z - 2], "to": [21, 3, z + 2], "mat": "air"},
    ]
ops += [
    {"op": "box", "from": [8, 0, 2], "to": [12, 4, 46], "mat": "plank", "part": "timber"},
    {"op": "box", "from": [8, 0, 2], "to": [12, 2, 46], "mat": "shell", "part": "timber"},
]
save("structures", {
    "name": "wreck_ribs",
    "notes": "A sea-going hull broken in the reeds: the keel and eleven frames standing out of the grass, barnacle crust to the turn of the bilge. Twelve metres; lay it along the ground anywhere in the flood meadows.",
    "size": [21, 15, 49],
    "anchor": [10, 0, 24],
    "level": False,
    "decays": False,
    "tags": ["wreck"],
    "slots": {"plank": "weathered_planks", "shell": "barnacle_crust"},
    "ops": ops,
})

# Wreck two: careened on the pan, masts still up. A hull lying on her side:
# the shell, hollowed, then everything above the deck line cut away.
ops = [
    {"op": "ellipsoid", "centre": [16, 9, 36], "radius": [9, 8, 32], "mat": "plank"},
    {"op": "ellipsoid", "centre": [16, 9, 36], "radius": [7.2, 6.4, 30], "mat": "air"},
    {"op": "ellipsoid", "centre": [16, 4.5, 36], "radius": [9, 3.5, 32], "mat": "shell"},
    {"op": "ellipsoid", "centre": [16, 4.5, 36], "radius": [7.4, 2.6, 30], "mat": "air"},
    # Open to the sky: the deck is gone and she lists, so the cut is a wedge.
    {"op": "box", "from": [16, 10, 0], "to": [33, 20, 73], "mat": "air"},
    {"op": "box", "from": [12, 14, 0], "to": [33, 20, 73], "mat": "air"},
    {"op": "box", "from": [8, 1, 14], "to": [10, 3, 58], "mat": "plank", "part": "timber"},
    # Two masts, leaning the way she lies.
    {"op": "cylinder", "base": [12, 9, 26], "radius": 1.6, "height": 24, "mat": "mast", "part": "timber"},
    {"op": "cylinder", "base": [13, 9, 44], "radius": 1.3, "height": 18, "mat": "mast", "part": "timber"},
    {"op": "box", "from": [5, 30, 25], "to": [19, 31, 27], "mat": "mast", "part": "timber"},
]
save("structures", {
    "name": "careened_hull",
    "notes": "A ship on her beam ends on the salt: eighteen metres of hull lying open to the sky with her masts still up and barnacles along the bilge. One of these only; lay it on the pan.",
    "size": [34, 34, 73],
    "anchor": [16, 0, 36],
    "level": False,
    "decays": True,
    "tags": ["wreck"],
    "slots": {"plank": "weathered_planks", "shell": "barnacle_crust", "mast": "oak_post"},
    "ops": ops,
})

ops = [
    {"op": "cylinder", "base": [14, 0, 14], "radius": 13, "height": 40, "mat": "stone", "part": "stone"},
    {"op": "cylinder", "base": [14, 1, 14], "radius": 9, "height": 40, "mat": "air"},
    {"op": "box", "from": [11, 1, 0], "to": [17, 12, 14], "mat": "air"},
    {"op": "cylinder", "base": [14, 34, 14], "radius": 13, "height": 2, "mat": "sill", "part": "stone"},
    {"op": "cylinder", "base": [14, 34, 14], "radius": 10, "height": 2, "mat": "air"},
]
for i in range(6):
    a = i * math.pi / 3
    ops.append({"op": "box", "from": [round(14 + 10 * math.cos(a)) - 1, 20, round(14 + 10 * math.sin(a)) - 1],
                "to": [round(14 + 10 * math.cos(a)) + 2, 25, round(14 + 10 * math.sin(a)) + 2], "mat": "air"})
save("structures", {
    "name": "lamp_tower",
    "notes": "The Lamp: a round stone tower on the highest kopje, built to carry a light, twelve metres of it still standing with the lantern floor gone and six window slots round the head. Its door faces the way you climb; the tower looks over the pans.",
    "size": [29, 40, 29],
    "anchor": [14, 0, 14],
    "foundation": 10,
    "level": True,
    "decays": True,
    "tags": ["ruin", "tower"],
    "slots": {"stone": "fieldstone", "sill": "carved_limestone"},
    "ops": ops,
})

ops = [
    {"op": "cylinder", "base": [30, 0, 30], "radius": 29, "height": 9, "mat": "stone", "part": "stone"},
    {"op": "cylinder", "base": [30, 0, 30], "radius": 25, "height": 9, "mat": "air"},
    {"op": "box", "from": [26, 0, 0], "to": [34, 9, 30], "mat": "air"},
    {"op": "cylinder", "base": [30, 0, 30], "radius": 12, "height": 2, "mat": "paving", "part": "floor"},
]
save("structures", {
    "name": "ring_wall",
    "notes": "The Ringstone: a dry-stone enclosure fifteen metres across, coursed and fitted without mortar, with one gap and a paved middle. Built by people who were gone before the present ones came. Place it on a kopje shoulder, gap downhill.",
    "size": [61, 10, 61],
    "anchor": [30, 0, 30],
    "foundation": 6,
    "level": True,
    "decays": True,
    "tags": ["ruin", "enclosure"],
    "slots": {"stone": "cotswold_stone", "paving": "moss_flagstone"},
    "ops": ops,
})

# ---- biomes ------------------------------------------------------------------


def biome(name, notes, priority, rules, surface, subsurface, map_color, plants=(), features=(), creatures=(), collectibles=(), **kw):
    def pl(spec):
        plant, density, verb, params = spec
        return {"plant": plant, "density": density, "placement": {"verb": verb, "params": params}}

    def ft(spec):
        s, density, verb, params = spec
        d = {"structure": s, "density": density}
        if verb:
            d["placement"] = {"verb": verb, "params": params}
        return d

    d = {"name": name, "notes": notes, "priority": priority, "rules": rules, "surface": surface,
         "subsurface": subsurface, "map_color": map_color, **kw}
    if plants:
        d["plants"] = [pl(p) for p in plants]
    if features:
        d["features"] = [ft(f) for f in features]
    if creatures:
        d["creatures"] = [{"creature": c, "count": n} for c, n in creatures]
    if collectibles:
        d["collectibles"] = [{"item": i, "density": n, "placement": {"verb": "scatter", "params": {}}} for i, n in collectibles]
    save("biomes", d)


biome("golden_savanna",
      "Open savanna: gold tussock grass on rolling sand, flat-topped acacias in loose stands, baobabs standing alone a hundred metres apart, and termite mounds. The country between the water and the pans.",
      6, {"elev": [2.0, 60], "slope": [0, 0.5]}, "savanna_grass", "packed_dirt", [172, 150, 96],
      plants=[("acacia", 0.004, "clustered", {"size": 40, "min": 0.0, "max": 0.012}),
              ("baobab", 0.00022, "lone", {"spacing": 70}),
              ("mopane", 0.012, "clustered", {"size": 30, "min": 0.0, "max": 0.05}),
              ("rabbitbrush", 0.03, "scatter", {})],
      features=[("termite_mound", 0.0004, "lone", {"spacing": 40}),
                ("granite_stack", 6e-05, "lone", {"spacing": 90})],
      creatures=[("zebra", 14), ("wildebeest", 30), ("giraffe", 5), ("warthog", 10), ("vulture", 4),
                 ("glossy_starling", 12), ("jackrabbit", 6), ("lizard", 8), ("bee", 10)],
      collectibles=[("ivory_shard", 8e-05)])

biome("flood_meadow",
      "The flooded grass: short green sward on sand a hand or two under the water for two months of the year, standing in water you can see your feet through. The lechwe herds and everything that follows them.",
      11, {"elev": [-1.2, 1.9], "slope": [0, 0.35], "amphibious": True}, "marsh_grass", "shore_sand", [122, 154, 96],
      plants=[("sawgrass", 0.05, "scatter", {}),
              ("cattail", 0.03, "riparian", {"width": 24}),
              ("reedmace", 0.02, "scatter", {}),
              ("cabbage_palm", 0.0012, "clustered", {"size": 26, "min": 0.0, "max": 0.006})],
      creatures=[("lechwe", 30), ("elephant", 6), ("water_buffalo", 12), ("hippo", 6), ("crocodile", 3),
                 ("great_egret", 8), ("heron", 5), ("bee_eater", 24), ("dragonfly", 14)],
      collectibles=[("seashell", 0.0002)])

biome("palm_island",
      "A palm island in the delta: a sand knoll a metre and a half up, ringed with fan palms grown from nuts that went through an elephant, with grass on its crown and usually one baobab.",
      14, {"region": "spread", "elev": [1.6, 8], "slope": [0, 0.6]}, "savanna_grass", "shore_sand", [164, 168, 102],
      plants=[("cabbage_palm", 0.02, "edge", {"width": 10, "inner": 0.004}),
              ("baobab", 0.0012, "lone", {"spacing": 40}),
              ("acacia", 0.003, "scatter", {}),
              ("sawgrass", 0.02, "scatter", {})],
      features=[("termite_mound", 0.0006, "lone", {"spacing": 30})],
      creatures=[("warthog", 6), ("macaque", 4), ("glossy_starling", 8), ("firefly", 20)],
      collectibles=[("seashell", 0.0004)])

biome("kopje_rock",
      "A kopje: bare granite dome and the boulders split off it, lichen in the shade, a rock fig rooted in a crack, and lions asleep on the warm stone.",
      16, {"region": "stacks", "slope": [0.12, 4.0], "elev": [4, 80]}, "granite", "granite", [140, 139, 138], rock="granite",
      plants=[("rock_lichen", 0.5, "scatter", {}),
              ("rock_moss", 0.18, "scatter", {}),
              ("cliff_banyan", 0.004, "lone", {"spacing": 30}),
              ("mopane", 0.006, "scatter", {})],
      features=[("granite_stack", 0.0012, "lone", {"spacing": 26})],
      creatures=[("lion", 5), ("marmot", 6), ("vulture", 3), ("bat", 10), ("lizard", 10)])

biome("gallery_forest",
      "Gallery forest: closed high forest along the shaded bank of the upper river, figs rooted into the scarp, vines, ferns, and no view out of it at all.",
      18, {"region": "green_bank", "river": [0, 70], "elev": [0.5, 40]}, "jungle_floor", "dirt", [56, 92, 54],
      plants=[("banyan", 0.012, "scatter", {}),
              ("small_fig", 0.01, "scatter", {}),
              ("fern", 0.14, "scatter", {}),
              ("hanging_vine", 0.3, "scatter", {}),
              ("shelf_fungus", 0.25, "scatter", {}),
              ("tree_moss", 0.3, "scatter", {})],
      creatures=[("macaque", 10), ("fruit_bat", 12), ("pileated_woodpecker", 4), ("kingfisher", 4),
                 ("bee_eater", 8), ("frog", 10), ("firefly", 30)])

biome("pan_floor",
      "A salt pan: the floor of a lake that has not existed for fourteen thousand years, swept bare and cracked into plates, with dead baobabs on its rim and shells lying on it in drifts.",
      15, {"region": "pans", "elev": [0.1, 6]}, "salt_crust", "salt_mud", [214, 210, 198],
      plants=[("dead_baobab", 0.0004, "lone", {"spacing": 60}),
              ("sagebrush", 0.004, "edge", {"width": 40, "inner": 0.0})],
      creatures=[("vulture", 5), ("jackrabbit", 3)],
      collectibles=[("seashell", 0.0012)])

biome("delta_shallows",
      "The channel beds: clean white sand under water a metre or two deep, clear enough to count the fish in it. The sand is why the flood runs clear by the time it is past the first islands.",
      3, {"underwater": True}, "shore_sand", "shore_sand", [196, 196, 168],
      plants=[("water_lily", 0.05, "scatter", {})],
      creatures=[("channel_catfish", 14), ("golden_trout", 16), ("swimming_crocodile", 4), ("otter", 4)])

print("\n".join(written))
print(f"{len(written)} entries")
