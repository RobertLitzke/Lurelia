"""Writes Jadewater's library entries (materials, plants, structures, creatures,
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

W = "jadewater"
written = []


def save(kind, d):
    d = {"name": d.pop("name"), "version": d.pop("version", 1), "introduced_by": d.pop("introduced_by", W), **d}
    path = os.path.join(ROOT, "library", kind, d["name"] + ".json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(libfmt.render(d, kind))
    written.append(f"{kind}/{d['name']}")


# ---- materials (ids 230-249) -------------------------------------------------

def mat(id, name, notes, pattern, base, alt, calm_base, calm_alt, scale=8, contrast=1.0, **kw):
    tex = {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast,
           "calm": {"base": calm_base, "alt": calm_alt}}
    d = {"name": name, "notes": notes, **({"id": id} if id else {}), "smooth": kw.pop("smooth", False), **kw, "texture": tex}
    save("materials", d)


mat(230, "karst_limestone", "Pale blue-grey layered limestone: the towers and the Old Wall. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "strata",
    [132, 146, 156], [150, 162, 170], [128, 142, 154], [144, 156, 166], scale=2, contrast=0.6, facet=True, version=2)
mat(231, "karst_streaked", "A darker band of the limestone, streaked grey-black where water runs down the faces. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "strata",
    [100, 110, 120], [122, 132, 140], [104, 114, 124], [122, 132, 142], scale=3, contrast=0.8, facet=True, version=2)
mat(232, "karst_pale", "The palest bed of the limestone, almost white, near the tops of the towers. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "cracks",
    [170, 180, 186], [150, 162, 170], [168, 178, 186], [150, 162, 172], scale=4, contrast=0.6, facet=True, version=2)
mat(233, "lime_sand", "White lime sand of the lagoon floor; under shallow water it turns the sea turquoise.", "ripples",
    [214, 210, 192], [200, 196, 178], [220, 216, 198], [206, 202, 186], scale=6, contrast=0.6, smooth=True)
mat(234, "bamboo_culm", "Golden bamboo culms with green nodes. Passable: you push between the stalks.", "strata",
    [172, 150, 72], [134, 136, 62], [170, 150, 78], [138, 138, 66], scale=3, contrast=1.0, passable=True)
mat(235, "giant_culm", "Giant bamboo: thick green-gold culms banded at the nodes.", "strata",
    [116, 136, 62], [150, 150, 70], [118, 136, 64], [146, 146, 72], scale=2, contrast=1.0)
mat(236, "bamboo_leaves", "Narrow yellow-green bamboo leaves in feathery sprays.", "blotch",
    [88, 124, 52], [112, 142, 60], [92, 124, 56], [110, 138, 62], scale=9, contrast=1.0)
mat(237, "banyan_bark", "Smooth grey banyan bark, fluted where the aerial roots have fused into the trunk.", "grain",
    [128, 122, 110], [104, 100, 92], [140, 134, 122], [118, 114, 104], scale=6, contrast=0.8)
mat(238, "banyan_leaves", "Banyan foliage: thick, glossy, dark green.", "blotch",
    [56, 92, 46], [72, 110, 54], [66, 102, 52], [80, 116, 60], scale=6, contrast=1.0, tint="leaves")
mat(240, "lantern_film", "The living film on cave ceilings and cenote overhangs: damp karst rock pricked with points of cold blue light, threads of it hanging down. v2: was bright cyan all over; now plain damp rock whose points of light the shader adds (glow_specks).", "noise",
    [112, 122, 128], [98, 108, 116], [116, 126, 132], [102, 112, 120], scale=5, contrast=1.0, glow=1.6, glow_specks=True, facet=True, version=2)
mat(241, "lumen_moss", "Moss on the cenote rims, faintly lit by the film beneath it.", "speckle",
    [62, 128, 96], [96, 196, 176], [70, 134, 102], [100, 190, 172], scale=6, contrast=1.0, glow=0.35, smooth=True)
mat(242, "lily_pad", "Giant lily pad: thick, waxy green with pale radial veins. Firm enough to stand on.", "rings",
    [78, 128, 58], [104, 152, 72], [86, 132, 64], [108, 150, 78], scale=3, contrast=0.8)
mat(243, "lily_flower", "Pale pink-white lily petals that glow a little at the base.", "blotch",
    [236, 206, 214], [250, 236, 236], [236, 210, 216], [248, 234, 234], scale=6, contrast=0.8, glow=0.25)
mat(244, "coral_lacquer", "Coral-red lacquer on the Order's bridges and gates; it has not faded.", "grain",
    [206, 84, 62], [190, 72, 54], [200, 92, 72], [188, 82, 64], scale=8, contrast=0.5)
mat(245, "temple_wood", "Dark cedar of the temple's posts and beams, gone grey at the edges.", "grain",
    [96, 70, 52], [80, 58, 44], [104, 78, 60], [90, 68, 52], scale=6, contrast=0.8)
mat(246, "temple_roof", "Dark grey-green roof tiles in long rows.", "planks",
    [74, 86, 84], [60, 70, 70], [80, 92, 90], [68, 78, 78], scale=8, contrast=0.8)
mat(247, "shoji_paper", "Paper screens on a fine wooden lattice, yellowed.", "coursed",
    [226, 216, 192], [196, 184, 160], [226, 218, 196], [204, 194, 172], scale=8, contrast=0.5)
mat(248, "temple_stone", "Grey granite of the lanterns, statues and steps, lichened.", "speckle",
    [148, 148, 138], [124, 126, 118], [156, 156, 146], [136, 138, 130], scale=8, contrast=0.8)
mat(250, "tower_grass", "Short wind-cropped grass and moss on the tower tops and the top of the Old Wall.", "noise",
    [92, 112, 58], [82, 102, 52], [96, 116, 62], [86, 106, 56], scale=4, contrast=1.0, smooth=True)
mat(251, "jungle_floor", "Dark jungle floor: leaf litter, roots and moss under the banyans.", "blotch",
    [78, 84, 50], [96, 104, 58], [84, 90, 56], [100, 108, 62], scale=5, contrast=1.0, smooth=True)
mat(226, "karst_dark", "A dark grey limestone, almost slate: some towers are cut from it. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "strata",
    [84, 90, 96], [100, 106, 112], [88, 94, 100], [104, 110, 116], scale=3, contrast=0.7, facet=True, version=2)
mat(227, "karst_light", "A very pale limestone, nearly white with a cream cast: the palest towers. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "strata",
    [204, 206, 200], [188, 192, 188], [206, 208, 202], [192, 196, 192], scale=2, contrast=0.5, facet=True, version=2)
mat(228, "karst_blue", "A blue-grey limestone, cold in colour: a few towers stand out in it. v2: drawn in flat facets (surface nets lit like sloped ground) instead of smoothed into slopes, so tower walls and the Old Wall's cliffs read as smooth rock, not one-voxel steps.", "strata",
    [104, 124, 146], [124, 142, 160], [108, 128, 150], [126, 144, 162], scale=3, contrast=0.6, facet=True, version=2)
mat(229, "vine_mantle", "A tower wrapped in vines and moss so thickly no rock shows: solid, so it can be climbed. v2: faceted with the rock.", "blotch",
    [60, 104, 48], [84, 130, 60], [66, 108, 52], [88, 132, 64], scale=5, contrast=1.0, tint="leaves", facet=True, version=2)
mat(None, "karst_moss", "Thick green moss streaking down the karst walls where water runs, and on the shady sides of ledges. Faceted with the rock.", "blotch",
    [62, 100, 50], [86, 122, 58], [70, 106, 54], [92, 126, 62], scale=5, contrast=1.0, facet=True)
mat(None, "karst_soil", "Dark leaf mould and red-brown earth caught on the tops of the karst's ledges and bulges. Faceted with the rock.", "noise",
    [92, 72, 52], [112, 88, 60], [98, 78, 56], [116, 92, 64], scale=5, contrast=1.0, facet=True)
mat(249, "bamboo_litter", "Pale gold drift of fallen bamboo leaves on the forest floor.", "noise",
    [122, 112, 70], [104, 98, 62], [124, 114, 74], [108, 102, 66], scale=6, contrast=1.0, smooth=True)

# ---- plants ------------------------------------------------------------------

save("plants", {"name": "giant_bamboo", "notes": "bamboo: Giant bamboo: clumps of thick green-gold culms 14-22 m tall, nodding at the tips, leaves on the upper half. Solid.",
                "form": "bamboo", "height": [14, 22], "trunk_radius": [0.18, 0.24], "crown_radius": [1.8, 3.2],
                "bark": "giant_culm", "leaf": "bamboo_leaves", "stems": [7, 13], "crown_base": 0.5, "max_slope": 0.8})
save("plants", {"name": "golden_bamboo", "notes": "bamboo: Golden bamboo of the gorges: dense clumps of thin culms 6-11 m tall that you push through (the culms are passable).",
                "form": "bamboo", "height": [6, 11], "trunk_radius": [0.17, 0.19], "crown_radius": [1.6, 2.6],
                "bark": "bamboo_culm", "leaf": "bamboo_leaves", "stems": [10, 18], "crown_base": 0.55, "max_slope": 1.0})
save("plants", {"name": "banyan", "notes": "branching: Banyan: a huge spreading fig on a buttressed, fluted grey trunk; grows out of cliffs as cliff_banyan.",
                "form": "branching", "height": [15, 21], "trunk_radius": [0.75, 1.0], "buttress": 0.8,
                "crown_radius": [3.6, 4.8], "bark": "banyan_bark", "leaf": "banyan_leaves", "spread": 1.0, "limb": 0.5,
                "max_slope": 1.0})
save("plants", {"name": "cliff_banyan", "notes": "branching: A banyan that started in a crack in a cliff: smaller, grown out of the rock face on steep ground.",
                "like": "banyan", "height": [8, 12], "trunk_radius": [0.4, 0.55], "buttress": 0.5,
                "crown_radius": [2.6, 3.4], "max_slope": 9})
save("plants", {"name": "small_fig", "notes": "broad: A small fig tree, 3-5 m, that grows anywhere it gets a root in: tower tops, ledges, the backs of beaches.",
                "like": "oak", "height": [3, 5.5], "trunk_radius": [0.14, 0.22], "crown_radius": [1.2, 2.0], "bark": "banyan_bark",
                "leaf": "banyan_leaves", "max_slope": 2.0})
save("plants", {"name": "karst_vine", "notes": "vine: The long vine curtains of the karst: strands 5-16 m down every tower lip and ledge.",
                "like": "hanging_vine", "height": [5, 16], "crown_radius": [2.0, 5.0]})
save("plants", {"name": "wisteria", "version": 2, "introduced_by": "faerie",
                "notes": "vine: Wisteria: a woody vine hung with long racemes, violet at the top fading to lilac and white, over trees and down cliffs. v2: was a weeping tree; worlds that had it plant flowering_cherry instead.",
                "form": "vine", "height": [4, 11], "trunk_radius": [0.0, 0.0], "crown_radius": [1.5, 3.5],
                "bark": "wisteria_leaves", "leaf": "wisteria_leaves", "hosts": ["tree", "cliff"], "max_slope": 9, "affords": ["nectar"]})
save("plants", {"name": "karst_pine", "notes": "umbrella: The small wind-twisted pine of the tower tops: flat-crowned, 3-6 m.",
                "like": "umbrella_pine", "height": [3, 6], "trunk_radius": [0.14, 0.22], "crown_radius": [1.6, 2.6],
                "squash": 0.45, "max_slope": 2.0})

# ---- structures ---------------------------------------------------------------

def box(a, b, m, part=None):
    o = {"op": "box", "from": a, "to": b, "mat": m}
    if part:
        o["part"] = part
    return o


def rope_bridge(length_m):
    """A rope-and-plank suspension bridge along local X, deck sagging to the
    middle. Base rim: the anchor voxel (each end's deck) sits on the highest
    ground under it, so both towers' tops must be at the same height."""
    L = length_m * 4
    sag = max(4, round(L / 40))  # voxels of sag at mid-span
    A = sag + 2                   # deck height at the ends
    Wd = 10
    ops = []
    seg = 4
    for x0 in range(0, L, seg):
        xm = (x0 + seg / 2) / L
        y = A - round(sag * (1 - (2 * xm - 1) ** 2))
        ops.append(box([x0, y - 1, 2], [x0 + seg, y, 8], "deck", "timber"))
        # Hand ropes a metre up, posts every two metres.
        ops.append(box([x0, y + 4, 1], [x0 + seg, y + 5, 2], "rope", "cloth"))
        ops.append(box([x0, y + 4, 8], [x0 + seg, y + 5, 9], "rope", "cloth"))
        if x0 % 8 == 0:
            ops.append(box([x0, y, 1], [x0 + 1, y + 4, 2], "rail", "timber"))
            ops.append(box([x0, y, 8], [x0 + 1, y + 4, 9], "rail", "timber"))
    # A small gate at each end: two lacquered posts and a lintel.
    for x in (0, L - 2):
        ops.append(box([x, A, 0], [x + 2, A + 12, 2], "rail", "timber"))
        ops.append(box([x, A, 8], [x + 2, A + 12, 10], "rail", "timber"))
        ops.append(box([x, A + 12, 0], [x + 2, A + 13, 10], "rail", "timber"))
    return {
        "name": f"rope_bridge_{length_m}",
        "notes": f"The Order's lacquered rope bridge, {length_m} m along local X, deck sagging {sag * 0.25:.2f} m at mid-span, a small red gate at each end. Base rim: place at the gap's middle between two towers whose tops are the same height (mesa `top`), bearing 0 for east-west, 90 for north-south; each end should rest 3-6 m onto a tower.",
        "size": [L, A + 14, Wd], "anchor": [L // 2, A, Wd // 2], "foundation": 0,
        "level": False, "decays": False, "base": "rim", "tags": ["order", "bridge"],
        "slots": {"deck": "temple_wood", "rope": "coral_lacquer", "rail": "coral_lacquer"},
        "ops": ops,
    }


for n in (24, 32, 40, 48, 56, 64, 72, 80):
    save("structures", rope_bridge(n))

# Torii: 5 m wide, 4.5 m tall, front local -Z.
torii_ops = [
    box([3, 0, 1], [5, 16, 3], "post", "timber"),
    box([15, 0, 1], [17, 16, 3], "post", "timber"),
    box([1, 12, 1], [19, 13, 3], "post", "timber"),            # nuki
    box([9, 13, 1], [11, 16, 3], "post", "timber"),            # gakuzuka
    box([0, 16, 0], [20, 17, 4], "post", "timber"),            # shimaki
    box([-1, 17, 0], [21, 18, 4], "top", "timber"),            # kasagi
    box([-2, 18, 0], [0, 19, 4], "top", "timber"),
    box([20, 18, 0], [22, 19, 4], "top", "timber"),
    box([2, 0, 0], [6, 1, 4], "stone", "stone"),
    box([14, 0, 0], [18, 1, 4], "stone", "stone"),
]
save("structures", {"name": "torii", "notes": "A lacquered torii gate, 5 m wide and 4.5 m tall, its dark top beam turned up at the ends. Front local -Z: face it along the avenue.",
                    "size": [24, 20, 4], "anchor": [12, 0, 2], "foundation": 2, "decays": True, "tags": ["order", "gate"],
                    "slots": {"post": "coral_lacquer", "top": "trim_dark", "stone": "temple_stone"},
                    "ops": [dict(o, **{"from": [o["from"][0] + 2, *o["from"][1:]], "to": [o["to"][0] + 2, *o["to"][1:]]}) for o in torii_ops]})

save("structures", {"name": "stone_lantern", "notes": "A granite toro lantern, 2 m tall; the film has grown inside its fire box and it still glows. Feature.",
                    "size": [6, 9, 6], "anchor": [3, 0, 3], "foundation": 1, "decays": False, "level": False, "tags": ["order"],
                    "ops": [
                        box([1, 0, 1], [5, 1, 5], "temple_stone", "stone"),
                        box([2, 1, 2], [4, 4, 4], "temple_stone", "stone"),
                        box([1, 4, 1], [5, 5, 5], "temple_stone", "stone"),
                        box([1, 5, 1], [5, 7, 5], "temple_stone", "stone"),
                        box([2, 5, 1], [4, 7, 5], "lantern_film"),
                        box([1, 5, 2], [5, 7, 4], "lantern_film"),
                        box([0, 7, 0], [6, 8, 6], "temple_stone", "stone"),
                        box([2, 8, 2], [4, 9, 4], "temple_stone", "stone"),
                    ]})

save("structures", {"name": "jizo_statue", "notes": "A small stone figure, knee high, with a faded red cloth bib. In rows by the temple paths. Feature.",
                    "size": [4, 5, 4], "anchor": [2, 0, 2], "foundation": 1, "decays": False, "level": False, "tags": ["order"],
                    "ops": [
                        box([0, 0, 0], [4, 1, 4], "temple_stone", "stone"),
                        box([1, 1, 1], [3, 4, 3], "temple_stone", "stone"),
                        box([1, 2, 0], [3, 3, 1], "coral_lacquer", "cloth"),
                        box([1, 4, 1], [3, 5, 3], "temple_stone", "stone"),
                    ]})


def hall():
    ops = [
        box([0, 0, 0], [48, 2, 40], "base", "stone"),               # stone plinth
        box([2, 2, 2], [46, 4, 38], "floor", "floor"),               # raised floor
        box([16, 0, -6], [32, 1, 0], "base", "stone"),              # steps
        box([18, 1, -4], [30, 2, 0], "base", "stone"),
        box([20, 2, -2], [28, 3, 2], "base", "stone"),
        {"op": "walls", "from": [6, 4, 6], "to": [42, 18, 34], "mat": "screen", "thick": 1, "part": "cloth"},
        box([20, 4, 6], [28, 15, 7], "air"),                          # open front
    ]
    for x in (2, 6, 14, 22, 26, 34, 42, 45):
        for z in (2, 6, 34, 37):
            ops.append(box([x, 4, z], [x + 1, 19, z + 1], "post", "timber"))
    for z in (14, 22, 26):
        for x in (2, 6, 42, 45):
            ops.append(box([x, 4, z], [x + 1, 19, z + 1], "post", "timber"))
    ops += [
        box([1, 18, 1], [47, 20, 39], "post", "timber"),            # tie beams
        {"op": "gable", "from": [-4, 20, -4], "to": [52, 20, 44], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
        box([-4, 19, -4], [52, 20, -2], "roof", "roof"),              # eaves
        box([-4, 19, 42], [52, 20, 44], "roof", "roof"),
        box([20, 21, 18], [28, 23, 22], "lantern_film"),            # the lantern inside the roof void
    ]
    return {"name": "temple_hall", "notes": "The main hall of the Order's temple: a stone plinth with steps, a raised floor on cedar posts, paper screens and a big tiled gable roof. Front (steps) local -Z.",
            "size": [56, 50, 50], "anchor": [24, 0, 20], "foundation": 6, "decays": True, "tags": ["order", "temple"],
            "slots": {"base": "temple_stone", "floor": "temple_wood", "post": "temple_wood", "screen": "shoji_paper", "roof": "temple_roof"},
            "ops": [dict(o, **{"from": [o["from"][0] + 4, o["from"][1], o["from"][2] + 6], "to": [o["to"][0] + 4, o["to"][1], o["to"][2] + 6]}) for o in ops]}


save("structures", hall())


def pagoda():
    ops = [box([4, 0, 4], [28, 2, 28], "base", "stone")]
    y = 2
    w = 16
    for tier in range(3):
        lo = 16 - w // 2
        hi = 16 + w // 2
        ops.append({"op": "walls", "from": [lo, y, lo], "to": [hi, y + 9, hi], "mat": "wall", "thick": 1, "part": "wall"})
        ops.append(box([15, y, lo], [17, y + 6, lo + 1], "air"))
        r0, r1 = lo - 5, hi + 5
        ops.append(box([r0, y + 9, r0], [r1, y + 10, r1], "roof", "roof"))
        ops.append(box([r0 + 2, y + 10, r0 + 2], [r1 - 2, y + 11, r1 - 2], "roof", "roof"))
        ops.append(box([r0 - 1, y + 10, r0 - 1], [r0 + 1, y + 11, r0 + 1], "roof", "roof"))
        ops.append(box([r1 - 1, y + 10, r0 - 1], [r1 + 1, y + 11, r0 + 1], "roof", "roof"))
        ops.append(box([r0 - 1, y + 10, r1 - 1], [r0 + 1, y + 11, r1 + 1], "roof", "roof"))
        ops.append(box([r1 - 1, y + 10, r1 - 1], [r1 + 1, y + 11, r1 + 1], "roof", "roof"))
        y += 11
        w -= 3
    ops.append(box([15, y, 15], [17, y + 12, 17], "spire", "metal"))
    for k in range(4):
        ops.append(box([14, y + 2 + k * 2, 14], [18, y + 3 + k * 2, 18], "spire", "metal"))
    return {"name": "pagoda", "notes": "A three-storey pagoda, 6 m square at the foot and 13 m to the top of its lacquered finial; each storey a little narrower under wide tiled eaves. Door on local -Z.",
            "size": [40, y + 13, 40], "anchor": [20, 0, 20], "foundation": 6, "decays": True, "tags": ["order", "temple"],
            "slots": {"base": "temple_stone", "wall": "temple_wood", "roof": "temple_roof", "spire": "coral_lacquer"},
            "ops": [dict(o, **{"from": [o["from"][0] + 4, o["from"][1], o["from"][2] + 4], "to": [o["to"][0] + 4, o["to"][1], o["to"][2] + 4]}) for o in ops]}


save("structures", pagoda())

save("structures", {"name": "bell_house", "notes": "An open bell house: four posts, a small tiled roof, and a big green bronze bell hanging inside.",
                    "size": [24, 28, 24], "anchor": [12, 0, 12], "foundation": 4, "decays": True, "tags": ["order", "temple"],
                    "slots": {"base": "temple_stone", "post": "temple_wood", "roof": "temple_roof", "bell": "temple_roof"},
                    "ops": [
                        box([3, 0, 3], [21, 2, 21], "base", "stone"),
                        box([5, 2, 5], [7, 18, 7], "post", "timber"),
                        box([17, 2, 5], [19, 18, 7], "post", "timber"),
                        box([5, 2, 17], [7, 18, 19], "post", "timber"),
                        box([17, 2, 17], [19, 18, 19], "post", "timber"),
                        box([4, 17, 4], [20, 18, 20], "post", "timber"),
                        {"op": "gable", "from": [0, 18, 0], "to": [24, 18, 24], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
                        {"op": "cylinder", "base": [12, 8, 12], "radius": 3, "height": 9, "mat": "bell", "part": "metal"},
                        {"op": "ellipsoid", "centre": [12, 17, 12], "radius": [3, 1.5, 3], "mat": "bell", "part": "metal"},
                    ]})

def rubble(name, blobs, size, notes, k=1.7):
    ops = [{"op": "ellipsoid", "centre": [round(v * k, 1) for v in c], "radius": [round(v * k, 1) for v in r], "mat": "stone", "part": "stone"}
           for c, r in blobs]
    size = [round(v * k) for v in size]
    return {"name": name, "notes": notes, "size": size, "anchor": [size[0] // 2, 0, size[2] // 2], "foundation": 0,
            "level": False, "decays": False, "slots": {"stone": "karst_limestone"}, "tags": ["karst", "rubble"], "ops": ops}


save("structures", rubble("karst_rubble", [([6, 3, 6], [4.5, 3.5, 4]), ([12, 2, 9], [3, 2.5, 3]), ([4, 1.5, 12], [2.5, 2, 2])], [18, 8, 16],
                          "A few blocks of limestone fallen from a tower, half sunk in the sand. Slot stone: the tower's rock."))
save("structures", rubble("karst_block", [([9, 5, 9], [7, 6, 6.5]), ([15, 3, 4], [3, 3, 3])], [20, 12, 18],
                          "A big fallen block of limestone, 3-4 m across, with a smaller one beside it. Slot stone: the tower's rock."))
save("structures", rubble("karst_scree", [([3, 1, 3], [2, 1.5, 2]), ([8, 1, 5], [2.5, 1.5, 2]), ([13, 1, 3], [1.8, 1.2, 1.8]),
                                          ([6, 1, 10], [2.2, 1.5, 2.5]), ([12, 1, 11], [1.5, 1, 1.5])], [16, 4, 14],
                          "Scattered small blocks and stones at a tower's foot. Slot stone: the tower's rock."))

def shifted(ops, dx, dz):
    return [dict(o, **{k: [o[k][0] + dx, o[k][1], o[k][2] + dz] for k in ("from", "to", "centre", "base") if k in o}) for o in ops]


save("structures", {"name": "pavilion", "notes": "An open pavilion of the Order: four posts on a stone footing under a small tiled roof, 4 m square, for looking out from a tower top. Settles on its lowest corner; the footing fills down.",
                    "size": [20, 24, 20], "anchor": [10, 0, 10], "foundation": 12, "level": False, "decays": True, "tags": ["order"],
                    "slots": {"base": "temple_stone", "post": "coral_lacquer", "roof": "temple_roof", "beam": "temple_wood"},
                    "ops": [
                        box([1, 0, 1], [19, 2, 19], "base", "stone"),
                        box([3, 2, 3], [5, 13, 5], "post", "timber"), box([15, 2, 3], [17, 13, 5], "post", "timber"),
                        box([3, 2, 15], [5, 13, 17], "post", "timber"), box([15, 2, 15], [17, 13, 17], "post", "timber"),
                        box([2, 12, 2], [18, 13, 18], "beam", "timber"),
                        {"op": "gable", "from": [-1, 13, -1], "to": [21, 13, 21], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
                    ]})
save("structures", {"name": "shrine_ruin", "notes": "A ruined shrine of the Order in the jungle: a stone platform, broken pillars of different heights, a fallen one, a lantern. Level ground.",
                    "size": [32, 22, 32], "anchor": [16, 0, 16], "foundation": 4, "decays": True, "tags": ["order", "ruin"],
                    "slots": {"stone": "temple_stone"},
                    "ops": [
                        box([0, 0, 0], [32, 2, 32], "stone", "floor"),
                        box([4, 2, 4], [28, 3, 28], "stone", "floor"),
                        box([6, 3, 6], [9, 20, 9], "stone", "stone"), box([23, 3, 6], [26, 12, 9], "stone", "stone"),
                        box([6, 3, 23], [9, 9, 26], "stone", "stone"), box([23, 3, 23], [26, 17, 26], "stone", "stone"),
                        box([5, 20, 5], [27, 22, 10], "stone", "stone"),
                        {"op": "log", "base": [11, 4, 24], "radius": 1.6, "length": 13, "axis": "x", "mat": "stone", "part": "stone"},
                        box([14, 3, 14], [18, 5, 18], "stone", "stone"), box([15, 5, 15], [17, 8, 17], "stone", "stone"),
                        box([14, 8, 14], [18, 10, 18], "stone", "stone"), box([15, 8, 15], [17, 9, 17], "lantern_film"),
                    ]})
save("structures", {"name": "stone_stupa", "notes": "A small stepped stone stupa, 3 m tall, mossy: the Order left them along paths and on beaches. Feature.",
                    "size": [12, 13, 12], "anchor": [6, 0, 6], "foundation": 2, "level": False, "decays": False, "tags": ["order"],
                    "slots": {"stone": "temple_stone"},
                    "ops": [
                        box([0, 0, 0], [12, 2, 12], "stone", "stone"), box([1, 2, 1], [11, 4, 11], "stone", "stone"),
                        box([2, 4, 2], [10, 6, 10], "stone", "stone"),
                        {"op": "ellipsoid", "centre": [6, 7.5, 6], "radius": [3.5, 2.5, 3.5], "mat": "stone", "part": "stone"},
                        box([5, 9, 5], [7, 13, 7], "stone", "stone"),
                    ]})
sea_torii_ops = [
    box([3, 0, 1], [5, 22, 3], "post", "timber"), box([15, 0, 1], [17, 22, 3], "post", "timber"),
    box([1, 17, 1], [19, 18, 3], "post", "timber"), box([9, 18, 1], [11, 21, 3], "post", "timber"),
    box([0, 21, 0], [20, 22, 4], "post", "timber"), box([-1, 22, 0], [21, 23, 4], "top", "timber"),
    box([-2, 23, 0], [0, 24, 4], "top", "timber"), box([20, 23, 0], [22, 24, 4], "top", "timber"),
]
save("structures", {"name": "sea_torii", "notes": "A great torii standing in the sea, its feet in the water, 5.5 m tall. Base sea; the posts fill down to the sea floor. Front local -Z.",
                    "size": [24, 24, 4], "anchor": [12, 0, 2], "foundation": 20, "level": False, "decays": False, "base": "sea", "tags": ["order", "gate"],
                    "slots": {"post": "coral_lacquer", "top": "trim_dark"}, "ops": shifted(sea_torii_ops, 2, 0)})

save("structures", {"name": "tower_shrine", "notes": "A tiny wooden shrine on a stone footing, a metre and a half tall, that the Order set on the tops of towers. Front local -Z.",
                    "size": [8, 9, 8], "anchor": [4, 0, 4], "foundation": 2, "decays": True, "tags": ["order"],
                    "slots": {"wood": "temple_wood", "roof": "temple_roof"},
                    "ops": [
                        box([1, 0, 1], [7, 1, 7], "temple_stone", "stone"),
                        box([2, 1, 2], [6, 5, 6], "wood", "timber"),
                        box([3, 2, 2], [5, 4, 3], "lantern_film"),
                        {"op": "gable", "from": [0, 5, 0], "to": [8, 5, 8], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"},
                    ]})


def lily_raft(name, pads, size, notes):
    ops = []
    for (x, z, r) in pads:
        ops.append({"op": "cylinder", "base": [x, 4, z], "radius": r, "height": 1, "mat": "lily_pad"})
        # The notch every lily pad has, and a rim a voxel high.
        ops.append(box([x, 4, z], [x + r + 1, 5, z + 1], "air"))
    for (x, z, r) in pads[::2]:
        fx, fz = x - r // 3, z + r // 3
        ops.append({"op": "ellipsoid", "centre": [fx, 5.5, fz], "radius": [1.2, 1.0, 1.2], "mat": "lily_flower"})
    return {"name": name, "notes": notes, "size": [size, 8, size], "anchor": [size // 2, 0, size // 2],
            "foundation": 0, "level": False, "decays": False, "base": "sea", "tags": ["cenote"], "ops": ops}


save("structures", lily_raft("lily_raft", [(14, 14, 9), (31, 11, 7), (44, 20, 10), (22, 32, 11), (42, 41, 8), (12, 46, 7), (30, 51, 9)], 60,
                             "A raft of giant water lilies on the surface of a cenote: seven pads 3.5-5.5 m across with pale flowers, close enough to step between. Base sea; the pads float at sea level."))
save("structures", lily_raft("lily_raft_small", [(10, 10, 8), (25, 13, 6), (14, 26, 7)], 36,
                             "Three giant lily pads and a flower on a cenote's surface. Base sea."))

# ---- creatures ------------------------------------------------------------------

DARK = [36, 28, 24]
WHITE = [240, 236, 228]
save("creatures", {"name": "panda", "notes": "graze: Giant panda: white with black legs, shoulders, ears and eye patches. Sits in the bamboo eating; slow, and not much bothered by people.",
                   "like": "bear", "scale": 0.95,
                   "colors": {"bear": WHITE, "muzzle": WHITE, "dark": DARK},
                   "behaviour": {"walk_speed": 0.5, "flee_speed": 2.5, "wary": 4, "spawn_chance": 1},
                   "extra_ops": [
                       {"op": "recolour", "at": [0, 0, 0], "until": [32, 9, 32], "from": "bear", "to": "dark"},
                       {"op": "recolour", "at": [10, 6, 8], "until": [23, 17, 13], "from": "bear", "to": "dark"},
                       {"op": "recolour", "at": [12, 15, 4], "until": [21, 18, 7], "from": "bear", "to": "dark"},
                       box([14, 13, 2], [16, 15, 3], "dark", "head"),
                       box([17, 13, 2], [19, 15, 3], "dark", "head"),
                   ]})
save("creatures", {"name": "kite_ray", "notes": "undulate: Kite ray: a jade-backed ray the size of a door, spotted with cyan light, that flies. Rises from the cenotes and rides the updrafts round the towers.",
                   "like": "sky_ray", "scale": 1.5,
                   "colors": {"back": [64, 118, 108], "belly": [214, 226, 216], "glow": [90, 230, 230, 1.6]},
                   "behaviour": {"fly_height": 14, "walk_speed": 2.4, "flee_speed": 4.0}})
save("creatures", {"name": "glossy_starling", "notes": "flock: Glossy starling: a starling in metallic blue-green with a bright orange belly, in quick flocks round the towers and over the beaches.",
                   "like": "starling",
                   "colors": {"body": [34, 96, 150], "head": [28, 70, 140], "beak": [24, 24, 30], "wing": [26, 126, 140], "tail": [30, 64, 120],
                              "legs": [40, 32, 28], "belly": [226, 120, 40]},
                   "behaviour": {"group": {"size": [6, 60], "spacing": 1.4, "range": 8, "separation": 1.0, "alignment": 1.4, "cohesion": 1.0}},
                   "extra_ops": [{"op": "recolour_below", "y": 8, "from": "body", "to": "belly"}]})
save("creatures", {"name": "swimming_turtle", "notes": "school: A green sea turtle swimming: alone or in twos over the lime sand of the lagoon, turning away when you come close.",
                   "like": "sea_turtle",
                   "behaviour": {"movement": "school", "walk_speed": 0.9, "flee_speed": 2.2, "wary": 5, "leap": 0, "spawn_chance": 1,
                                 "group": {"size": [1, 2], "spacing": 3, "range": 10, "separation": 1.0, "alignment": 1.0, "cohesion": 0.6}}})
save("creatures", {"name": "serow", "notes": "graze: Serow: a small dark goat-antelope of the tower ledges and the top of the Old Wall, with a pale throat.",
                   "like": "bighorn", "scale": 0.85,
                   "colors": {"rabbit": [70, 64, 58], "rabbit_light": [150, 140, 126]}})

# ---- collectibles -------------------------------------------------------------

save("collectibles", {"name": "bell_charm", "display": "bell charm", "notes": "A little bronze bell on a red cord, left at the temple by the Order's visitors.",
                      "voxel": 0.0125, "spin": False,
                      "colors": {"bronze": [120, 150, 110, 0.4], "cord": [200, 92, 72]},
                      "ops": [
                          {"op": "ellipsoid", "centre": [16, 10, 16], "radius": [7, 8, 7], "mat": "bronze"},
                          box([9, 2, 9], [23, 4, 23], "bronze"),
                          box([15, 18, 15], [17, 30, 17], "cord"),
                      ]})

# ---- biomes -------------------------------------------------------------------

def P(plant, density, verb="scatter", **params):
    d = {"plant": plant, "density": density, "placement": {"verb": verb, "params": params}}
    if "field" in params:
        d["placement"]["field"] = params.pop("field")
    return d


save("biomes", {"name": "jade_shallows", "notes": "The lagoon floor: white lime sand under one to four metres of green water, turquoise from above.",
                "priority": 0, "rules": {"underwater": True}, "surface": "lime_sand", "subsurface": "lime_sand", "map_color": [96, 186, 176],
                "creatures": [{"creature": "herring", "count": 40}, {"creature": "manta_ray", "count": 2}, {"creature": "swimming_turtle", "count": 14},
                              {"creature": "cormorant", "count": 6}, {"creature": "gull", "count": 8}]})
save("biomes", {"name": "lime_beach", "notes": "White lime-sand beaches at the foot of the jungle and round the towers.",
                "priority": 6, "rules": {"coast": [0, 22], "elev": [-1, 3.0], "slope": [0, 0.6]}, "surface": "lime_sand", "subsurface": "lime_sand",
                "map_color": [226, 222, 200],
                "plants": [P("hazel", 0.002, "edge", width=8, inner=0.004), P("small_fig", 0.004, "edge", width=10, inner=0.006)],
                "features": [{"structure": "stone_stupa", "density": 0.0004, "placement": {"verb": "lone", "params": {"spacing": 60}}},
                             {"structure": "karst_scree", "density": 0.002, "placement": {"verb": "scatter", "params": {}}}],
                "version": 2, "creatures": [{"creature": "heron", "count": 5}, {"creature": "sea_turtle", "count": 2}, {"creature": "gull", "count": 4},
                              {"creature": "glossy_starling", "count": 20}],
                "collectibles": [{"item": "seashell", "density": 0.002, "placement": {"verb": "scatter", "params": {}}}]})
save("biomes", {"name": "karst_jungle", "notes": "The lowland between the Old Wall and the lagoon: banyans over oak and alder, ferns, vines on everything.",
                "version": 2, "priority": 1, "rules": {}, "surface": "jungle_floor", "subsurface": "jungle_floor", "map_color": [70, 120, 60], "mist": 0.25,
                "plants": [P("banyan", 0.0016, "clustered", size=60, min=0.3, max=0.6), P("oak", 0.006, "clustered", size=40, min=0.35, max=0.6),
                           P("alder", 0.003), P("hazel", 0.008), P("fern", 0.06, "clustered", size=18, min=0.3, max=0.55),
                           P("cliff_banyan", 0.004, "gradient", field="slope", min=1.2, max=4.0),
                           P("hanging_vine", 0.25), P("wisteria", 0.08), P("small_fig", 0.004),
                           P("flowering_cherry", 0.002, "clustered", size=40, min=0.4, max=0.6)],
                "features": [{"structure": "shrine_ruin", "density": 0.0002, "placement": {"verb": "lone", "params": {"spacing": 110}}},
                             {"structure": "stone_stupa", "density": 0.0004, "placement": {"verb": "lone", "params": {"spacing": 70}}},
                             {"structure": "stone_lantern", "density": 0.0003, "placement": {"verb": "scatter", "params": {}}}],
                "creatures": [{"creature": "boar", "count": 8}, {"creature": "deer", "count": 8}, {"creature": "sparrow", "count": 14},
                              {"creature": "pileated_woodpecker", "count": 3}, {"creature": "kite_ray", "count": 6},
                              {"creature": "glossy_starling", "count": 20}]})
save("biomes", {"name": "karst_pinnacle", "notes": "The tops and faces of the towers and the Old Wall: thin turf, wind-twisted pines, vine curtains over every lip, banyans out of the cliffs.",
                "version": 2, "priority": 7, "rules": {"elev": [20, 200]}, "surface": "tower_grass", "subsurface": "karst_soil", "map_color": [168, 176, 180],
                "plants": [P("karst_pine", 0.02, "clustered", size=16, min=0.3, max=0.55),
                           P("banyan", 0.003, "clustered", size=30, min=0.35, max=0.6), P("oak", 0.008, "clustered", size=24, min=0.4, max=0.6),
                           P("hazel", 0.008), P("small_fig", 0.02, "clustered", size=14, min=0.3, max=0.55),
                           P("fern", 0.03, "clustered", size=12, min=0.3, max=0.5),
                           P("cliff_banyan", 0.008, "gradient", field="slope", min=0.9, max=4.0),
                           P("karst_vine", 0.65), P("wisteria", 0.12),
                           P("flowering_cherry", 0.004, "clustered", size=20, min=0.4, max=0.6)],
                "features": [{"structure": "tower_shrine", "density": 0.0002, "placement": {"verb": "lone", "params": {"spacing": 60}}},
                             {"structure": "pavilion", "density": 0.00015, "placement": {"verb": "lone", "params": {"spacing": 90}}},
                             {"structure": "stone_stupa", "density": 0.0002, "placement": {"verb": "lone", "params": {"spacing": 70}}}],
                "creatures": [{"creature": "serow", "count": 10}, {"creature": "eagle", "count": 1}, {"creature": "kite_ray", "count": 15},
                              {"creature": "starling", "count": 50}, {"creature": "glossy_starling", "count": 40}]})
save("biomes", {"name": "bamboo_gorge", "notes": "The narrow misty gorges of the Old Wall: golden bamboo you push through, a few giant clumps, ferns, pandas.",
                "priority": 9, "rules": {"region": "gorges", "elev": [-1, 16]}, "surface": "bamboo_litter", "subsurface": "dirt",
                "map_color": [196, 176, 80], "mist": 0.4,
                "plants": [P("golden_bamboo", 0.07, "clustered", size=10, min=0.25, max=0.5), P("giant_bamboo", 0.006),
                           P("fern", 0.08), P("karst_vine", 0.7), P("cliff_banyan", 0.002, "gradient", field="slope", min=1.2, max=4.0)],
                "version": 2, "creatures": [{"creature": "panda", "count": 6}, {"creature": "kite_ray", "count": 6}, {"creature": "dragonfly", "count": 6}]})
save("biomes", {"name": "bamboo_forest", "notes": "Wetter ground east of the Old Wall: giant bamboo in dense patches among small broadleaf trees and ferns, misty.",
                "priority": 4, "rules": {"moist": [0.62, 1.0], "elev": [2.5, 20]}, "surface": "bamboo_litter", "subsurface": "dirt",
                "map_color": [150, 168, 70], "mist": 0.5,
                "plants": [P("giant_bamboo", 0.03, "clustered", size=30, min=0.3, max=0.55), P("golden_bamboo", 0.012, "clustered", size=20, min=0.4, max=0.6),
                           P("alder", 0.003), P("oak", 0.002), P("hazel", 0.008), P("fern", 0.09, "clustered", size=16, min=0.25, max=0.5)],
                "creatures": [{"creature": "panda", "count": 8}, {"creature": "deer", "count": 4}, {"creature": "sparrow", "count": 8}]})
save("biomes", {"name": "jungle_temple", "notes": "The Order's temple grounds at the foot of the Old Wall: moss over everything, banyans in the walls, stone lanterns and rows of little statues.",
                "priority": 10, "rules": {"region": "temple"}, "surface": "moss_grass", "subsurface": "dirt", "map_color": [190, 96, 80], "mist": 0.35,
                "plants": [P("banyan", 0.001, "lone", spacing=26), P("fern", 0.05), P("hazel", 0.003)],
                "features": [{"structure": "stone_lantern", "density": 0.003, "placement": {"verb": "scatter", "params": {}}},
                             {"structure": "jizo_statue", "density": 0.004, "placement": {"verb": "clustered", "params": {"size": 8, "min": 0.4, "max": 0.6}}}],
                "creatures": [{"creature": "deer", "count": 3}, {"creature": "sparrow", "count": 6}],
                "collectibles": [{"item": "bell_charm", "density": 0.0015, "placement": {"verb": "scatter", "params": {}}}]})
save("biomes", {"name": "glow_cenote", "notes": "The rims and walls of the flooded sinkholes: moss lit by the film beneath, vine curtains, banyans on the walls, kite rays rising.",
                "priority": 11, "rules": {"region": "cenotes"}, "surface": "lumen_moss", "subsurface": "dirt", "rock": "karst_streaked",
                "map_color": [80, 210, 210], "mist": 0.15, "water": {"shallow": [46, 150, 104], "deep": [10, 52, 40]},
                "plants": [P("karst_vine", 0.95), P("fern", 0.08), P("cliff_banyan", 0.006, "gradient", field="slope", min=1.0, max=4.0),
                           P("golden_bamboo", 0.004)],
                "version": 2, "creatures": [{"creature": "kite_ray", "count": 12}, {"creature": "dragonfly", "count": 8}, {"creature": "lantern_moth", "count": 10}]})

print("\n".join(written))
