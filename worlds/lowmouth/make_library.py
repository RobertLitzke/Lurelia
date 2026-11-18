"""Writes Lowmouth's library entries (materials, plants, structures, creatures,
biomes) through libfmt.render so they come out canonical. The JSON files in
library/ are what the game reads. See the note below.

Materials leave out "id": the game gives them free ids at load, per world
(Library::assign_ids), since the old 17..254 range is nearly used up.

One-shot, not kept current. This ran once, when the world was made; the JSON
in library/ has moved on since (entries renamed or removed, new fields, hand
edits), so rerunning it would overwrite live data with a stale version. It is
kept as a record of how the world was first written. Edit the JSON directly
instead, and if you do rerun this, diff the output before keeping any of it.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

W = "lowmouth"
written = []


def save(kind, d):
    d = {"name": d.pop("name"), "version": 1, "introduced_by": W, **d}
    path = os.path.join(ROOT, "library", kind, d["name"] + ".json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(libfmt.render(d, kind))
    written.append(f"{kind}/{d['name']}")


# ---- materials ----------------------------------------------------------------

def mat(name, notes, pattern, base, alt, calm_base=None, calm_alt=None, scale=8, contrast=1.0, **kw):
    tex = {"pattern": pattern, "base": base, "alt": alt, "scale": scale, "contrast": contrast,
           "calm": {"base": calm_base or base, "alt": calm_alt or alt}}
    d = {"name": name, "notes": notes, "smooth": kw.pop("smooth", False), **kw, "texture": tex}
    save("materials", d)


mat("swamp_muck", "Black-brown swamp mud under the cypress: wet, with a litter of rusty needles.", "blotch",
    [62, 54, 42], [78, 66, 48], [66, 58, 46], [80, 70, 52], scale=6, contrast=0.7, smooth=True)
mat("marsh_grass", "Coarse tawny-green marsh grass on the levee toes, the chenier and the hammock edges (grass sprites grow on it).", "noise",
    [116, 126, 66], [132, 134, 74], [112, 128, 64], [128, 134, 72], scale=6, contrast=0.6, smooth=True, tint="meadow")
mat("oyster_shell", "Old oyster shell, grey-white and broken: the chenier's beaches and the shell road.", "gravel",
    [196, 190, 178], [158, 152, 142], [192, 186, 174], [164, 158, 148], scale=7, contrast=0.8, smooth=True)
mat("cane_char", "A burned cane field: black stubble and grey ash in rows.", "speckle",
    [54, 50, 46], [86, 80, 72], [60, 56, 52], [90, 84, 76], scale=8, contrast=0.8, smooth=True)
mat("cypress_bark", "Bald cypress bark: fibrous, peeling in long grey-brown strips with red-brown beneath.", "grain",
    [118, 100, 84], [92, 76, 64], [122, 104, 88], [98, 82, 70], scale=5, contrast=0.7)
mat("cypress_needles", "Bald cypress foliage: soft feathery sprays of needles, a light olive green.", "blotch",
    [92, 114, 58], [108, 128, 66], [90, 112, 56], [106, 126, 64], scale=8, contrast=0.9)
mat("spanish_moss", "Spanish moss: grey-green beards hanging from the oaks and cypress. Passable.", "grain",
    [128, 134, 116], [106, 114, 98], [126, 132, 114], [108, 116, 100], scale=9, contrast=0.8, passable=True)
mat("mangrove_bark", "Mangrove bark: smooth grey-brown prop roots and stems, mud-stained near the water.", "noise",
    [104, 94, 80], [84, 76, 64], [108, 98, 84], [88, 80, 68], scale=6, contrast=0.6)
mat("mangrove_leaves", "Red mangrove leaves: thick, glossy and dark green.", "blotch",
    [54, 90, 48], [70, 106, 56], [58, 94, 50], [72, 108, 58], scale=8, contrast=0.9, tint="leaves")
mat("live_oak_bark", "Live oak bark: near-black, deeply furrowed into blocks.", "cracks",
    [72, 64, 56], [52, 46, 40], [76, 68, 60], [56, 50, 44], scale=5, contrast=0.8)
mat("live_oak_leaves", "Live oak leaves: small, leathery and dark olive, evergreen.", "blotch",
    [58, 78, 44], [72, 90, 52], [56, 76, 42], [70, 88, 50], scale=9, contrast=0.9)
mat("tupelo_leaves", "Water tupelo leaves: broad, a dull mid green.", "blotch",
    [74, 100, 52], [88, 112, 60], [72, 98, 50], [86, 110, 58], scale=8, contrast=0.9)
mat("palm_frond", "Palmetto and cabbage palm fronds: stiff fans of grey-green blades.", "grain",
    [96, 124, 66], [114, 138, 74], [98, 124, 68], [114, 138, 76], scale=7, contrast=0.8)
mat("palm_trunk", "Cabbage palm trunk, ringed with old frond scars.", "rings",
    [124, 112, 94], [102, 92, 76], [126, 114, 96], [106, 96, 80], scale=6, contrast=0.6)
mat("silver_deadwood", "Dead cypress and cedar weathered silver-grey, split along the grain: the ghost forest's snags, the bottle trees, the figure's cane. Drawn as thin stems (bevelled, not rounded), so slim branches stay joined.", "grain",
    [134, 130, 122], [100, 96, 90], [164, 156, 144], [142, 134, 124], scale=4, contrast=0.9, tube=True)
mat("cane_leaves", "Sugar cane: tall jointed stalks and long ribbon leaves, fresh green.", "grain",
    [112, 148, 72], [134, 158, 86], [110, 144, 70], [130, 154, 84], scale=7, contrast=0.7)
mat("sawgrass_blades", "Sawgrass: tough tawny-green blades with toothed edges, in tussocks out of the water.", "grain",
    [132, 132, 76], [150, 140, 86], [128, 132, 74], [146, 140, 84], scale=7, contrast=0.7)
mat("hyacinth_leaves", "Water hyacinth: glossy round leaves on swollen floating stalks, a mat over the water.", "blotch",
    [68, 118, 58], [86, 136, 66], [70, 120, 60], [88, 136, 68], scale=8, contrast=0.9)
mat("hyacinth_flower", "Water hyacinth flowers: pale lilac spikes with a yellow eye.", "speckle",
    [164, 136, 204], [200, 176, 228], [164, 140, 204], [198, 176, 226], scale=9, contrast=0.8)
mat("tomb_lime", "Whitewashed plaster over brick: the raised tombs, stained grey where the water stood.", "blotch",
    [222, 218, 206], [194, 190, 178], [220, 216, 204], [196, 192, 182], scale=5, contrast=0.6)
mat("delta_mud", "Soft grey-brown delta mud: the bed of the open water and the mudflats, smooth and wet.", "noise",
    [104, 96, 78], [114, 106, 86], [108, 100, 82], [116, 108, 88], scale=3, contrast=0.35, smooth=True)
mat("cattail_leaves", "Cattail leaves: tall dark green straps standing out of the shallows (drawn as sprites).", "grain",
    [70, 104, 50], [86, 116, 58], scale=7, contrast=0.6)
mat("cattail_head", "A cattail's stem and brown velvet head (drawn as a stalk sprite).", "noise",
    [92, 60, 36], [110, 74, 46], scale=6, contrast=0.6)
mat("palmetto_fronds", "Saw palmetto fans: stiff grey-green blades (drawn as sprites).", "grain",
    [96, 124, 66], [114, 138, 74], scale=7, contrast=0.8)
mat("stick_wood", "Peeled sticks lashed into lattices: grey, dry, a little twisted.", "grain",
    [96, 88, 76], [72, 66, 58], scale=6, contrast=0.7)
mat("cane_soil", "Dark worked soil between the cane rows.", "noise",
    [92, 76, 58], [104, 88, 66], [94, 78, 60], [106, 90, 68], scale=4, contrast=0.5, smooth=True)
mat("chalk", "Chalk or cornmeal drawn on the ground at the crossroads: thin pale lines.", "flat",
    [228, 224, 214], [228, 224, 214], scale=4, contrast=0.2)
mat("candle_flame", "Candle flames and lamp wicks: a small warm light.", "flat",
    [255, 204, 120], [255, 230, 170], scale=4, contrast=0.3, glow=1.4)
mat("bottle_blue", "Cobalt-blue bottle glass hung on the bottle trees, faintly lit from inside.", "glass",
    [44, 84, 168], [86, 132, 220], scale=6, contrast=0.7, glow=1.0)
mat("bottle_violet", "Violet bottle glass on the bottle trees.", "glass",
    [104, 62, 152], [146, 104, 196], scale=6, contrast=0.7, glow=0.9)

# ---- plants -------------------------------------------------------------------

def plant(name, notes, **kw):
    save("plants", {"name": name, "notes": notes, **kw})


plant("bald_cypress", "Bald cypress: a tall flat-crowned tree standing in the black water on a great flared, buttressed foot.",
      form="branching", height=[14, 22], trunk_radius=[0.45, 0.7], buttress=0.9, crown_radius=[3.5, 5.0], spread=0.35,
      bark="cypress_bark", leaf="cypress_needles", max_slope=0.8, standing_water=[-1.5, 1.6], affords=["perch", "nest"])
plant("cypress_snag", "A dead cypress, grey and barkless: the ghost forest where salt water reached the old swamp.",
      form="snag", height=[6, 14], trunk_radius=[0.35, 0.55], crown_radius=[1.5, 2.5], bark="silver_deadwood",
      max_slope=0.8, standing_water=[-1.0, 2.2], affords=["perch"])
plant("water_tupelo", "Water tupelo: a swollen-footed swamp tree with a narrow crown of light green, among the cypress.",
      form="broad", height=[10, 15], trunk_radius=[0.3, 0.45], buttress=0.6, crown_radius=[2.4, 3.4],
      bark="mangrove_bark", leaf="tupelo_leaves", max_slope=0.8, standing_water=[-1.0, 1.3])
plant("red_mangrove", "Red mangrove: a low dark crown on a tangle of arching prop roots, standing in the shallows of the river mouth.",
      form="mangrove", height=[3, 11], trunk_radius=[0.3, 0.45], crown_radius=[2.4, 4.6], stems=[5, 8],
      bark="mangrove_bark", leaf="mangrove_leaves", max_slope=0.8, standing_water=[0.05, 1.7], affords=["perch", "cover"])
plant("black_mangrove", "Black mangrove: a bushy tree on the mud banks just above the tide.",
      form="broad", height=[3, 5], trunk_radius=[0.15, 0.22], crown_radius=[1.8, 2.6],
      bark="mangrove_bark", leaf="mangrove_leaves", max_slope=0.8, standing_water=[-0.8, 0.5], affords=["cover"])
plant("live_oak", "Southern live oak: a vast low dome on a short black trunk, limbs reaching out sideways, hung with moss.",
      form="branching", height=[9, 13], trunk_radius=[0.7, 0.95], buttress=0.6, crown_radius=[7.5, 9.5], spread=1.0, dome=0.35,
      limb=0.6, bark="live_oak_bark", leaf="live_oak_leaves", max_slope=0.6, affords=["perch", "nest"])
plant("cabbage_palm", "Cabbage palm: a ringed grey trunk with a round head of stiff fan fronds (sprite fans round the head).",
      form="umbrella", height=[7, 11], trunk_radius=[0.18, 0.24], crown_radius=[1.4, 1.9], squash=0.5,
      bark="palm_trunk", leaf="palm_frond", max_slope=0.6)
plant("saw_palmetto", "Saw palmetto: low clumps of grey-green fans under the oaks and on the hammocks, drawn as sprites.",
      form="herb", height=[0.8, 1.2], crown_radius=[0.8, 1.2], bark="palm_trunk", leaf="palmetto_fronds", max_slope=0.7,
      standing_water=[-5, -0.1])
plant("sugar_cane", "Sugar cane: dense clumps of straight jointed stalks taller than a man, ribbon leaves arching over; drawn as sprites.",
      form="herb", height=[2.8, 3.8], crown_radius=[0.4, 0.6], bark="dead_wood", leaf="cane_leaves", max_slope=0.4,
      standing_water=[-5, -0.1], affords=["cover"])
plant("sawgrass", "Sawgrass: low tussocks of toothed tawny blades out of the shallow water, drawn as sprites (a kayak slides through them).",
      form="herb", height=[0.8, 1.1], crown_radius=[0.6, 0.9], bark="dead_wood", leaf="sawgrass_blades",
      max_slope=0.6, standing_water=[-0.5, 0.8])
plant("cattail", "Cattails: tall strap leaves in clumps in the shallows, most with a stem carrying a brown velvet head; drawn as sprites, so a kayak slides through them.",
      form="herb", height=[1.6, 2.2], crown_radius=[0.4, 0.6], bark="dead_wood", leaf="cattail_leaves",
      max_slope=0.6, standing_water=[-0.4, 0.9], stalk=["cattail_head", 0.65])
plant("spanish_moss", "Spanish moss: grey beards hanging from the crowns of the live oaks and cypress.",
      form="vine", height=[1.0, 2.8], trunk_radius=[0.0, 0.0], crown_radius=[1.0, 2.0],
      bark="spanish_moss", leaf="spanish_moss", max_slope=9, hosts=["tree"])

# ---- structures -----------------------------------------------------------------
# Voxels are 0.25 m. Front faces local -Z. With base "sea", y = 0 is 1 m under
# the sea and the water's surface is y = 4.

def box(a, b, mat, part="timber"):
    return {"op": "box", "from": list(a), "to": list(b), "mat": mat, "part": part}


def posts(x0, x1, z0, z1, y0, y1, step, mat="post"):
    """Posts round a rectangle's edge (inclusive corners), one voxel square."""
    out = []
    xs = list(range(x0, x1, step)) + [x1 - 1]
    zs = list(range(z0, z1, step)) + [z1 - 1]
    for x in xs:
        for z in (z0, z1 - 1):
            out.append(box([x, y0, z], [x + 1, y1, z + 1], mat))
    for z in zs:
        for x in (x0, x1 - 1):
            out.append(box([x, y0, z], [x + 1, y1, z + 1], mat))
    return out


def rail(x0, x1, z0, z1, y, mat="post"):
    """A plank rail round three sides of a deck (open at the front, -Z)."""
    return [box([x0, y, z0], [x0 + 1, y + 1, z1], mat), box([x1 - 1, y, z0], [x1, y + 1, z1], mat),
            box([x0, y, z1 - 1], [x1, y + 1, z1], mat)]


def window(x, y, w, h, z0, z1, mat="window"):
    return [{"op": "box", "from": [x, y, z0], "to": [x + w, y + h, z1], "mat": mat, "part": "glass"}]


def stilt_house(name, notes, w, d, floor, wall_h, porch, slots, tags, lit=False):
    """A board house on posts over the water: floor `floor` voxels over y=0,
    a front porch `porch` deep, tin gable roof along Z."""
    x0, x1 = 1, 1 + w
    z0, z1 = 1 + porch, 1 + porch + d
    fy = floor
    ops = posts(x0, x1, 1, z1, 0, fy, 6)
    ops += [box([x0, fy, 1], [x1, fy + 1, z1], "deck", "floor")]
    ops += [{"op": "walls", "from": [x0, fy + 1, z0], "to": [x1, fy + 1 + wall_h, z1], "mat": "wall", "thick": 1, "part": "wall"}]
    # Porch posts, rail and a shed roof over it.
    for x in (x0, x1 - 1):
        ops.append(box([x, fy + 1, 1], [x + 1, fy + 1 + wall_h, 2], "post"))
    ops += [box([x0, fy + 4, 1], [x0 + 1, fy + 5, z0], "post"), box([x1 - 1, fy + 4, 1], [x1, fy + 5, z0], "post")]
    ops.append({"op": "box", "from": [x0 - 1, fy + 1 + wall_h, 0], "to": [x1 + 1, fy + 2 + wall_h, z0], "mat": "roof", "part": "roof"})
    # The house roof: tin over a ridge along Z.
    ops.append({"op": "gable", "from": [x0 - 1, fy + 1 + wall_h, z0 - 1], "to": [x1 + 1, fy + 1 + wall_h, z1 + 1],
                "axis": "z", "mat": "roof", "rise": 1, "part": "roof"})
    # Door and windows: front door centred, windows each side and along the flanks.
    mid = (x0 + x1) // 2
    ops.append({"op": "box", "from": [mid - 2, fy + 1, z0], "to": [mid + 2, fy + 9, z0 + 1], "mat": "door", "part": "timber"})
    win = "lamp" if lit else "window"
    ops += window(x0 + 2, fy + 4, 3, 4, z0, z0 + 1, win) if w >= 14 else []
    ops += window(x1 - 5, fy + 4, 3, 4, z0, z0 + 1, win) if w >= 14 else []
    for z in range(z0 + 5, z1 - 4, 10):
        ops += [{"op": "box", "from": [x0, fy + 4, z], "to": [x0 + 1, fy + 8, z + 3], "mat": win, "part": "glass"},
                {"op": "box", "from": [x1 - 1, fy + 4, z], "to": [x1, fy + 8, z + 3], "mat": win, "part": "glass"}]
    top = fy + 2 + wall_h + (w + 2) // 2 + 2
    save("structures", {"name": name, "notes": notes, "size": [w + 2, top, z1 + 1], "anchor": [(w + 2) // 2, 0, (z1 + 1) // 2],
                        "foundation": 24, "level": False, "base": "sea", "tags": tags, "slots": slots, "ops": ops})


HOUSE = {"wall": "siding_teal", "roof": "corrugated_tin", "deck": "weathered_planks", "post": "timber_frame",
         "door": "door_blue", "window": "window_glass", "lamp": "lamplit_glass"}

stilt_house("stilt_house", "A shotgun house on cypress posts over the water: board walls, a tin roof, a front porch facing the channel (local -Z). Floor 2 m over the water; base sea, the posts run down to the mud.",
            18, 36, 12, 11, 8, dict(HOUSE), ["lowmouth", "house"])
stilt_house("stilt_camp", "A small fishing camp on posts: one room, a deep porch facing the water (local -Z), tin roof. Base sea.",
            14, 20, 11, 10, 10, dict(HOUSE, wall="siding_rust", door="door_green"), ["lowmouth", "house"])


def walkway(name, notes, length, deck_y, width=6):
    ops = [box([0, deck_y, 0], [width, deck_y + 1, length], "deck", "floor")]
    for z in range(1, length, 10):
        for x in (0, width - 1):
            ops.append(box([x, 0, z], [x + 1, deck_y, z + 1], "post"))
    # A hand rail on posts along one side.
    for z in range(1, length, 10):
        ops.append(box([width - 1, deck_y + 1, z], [width, deck_y + 4, z + 1], "post"))
    ops.append(box([width - 1, deck_y + 4, 0], [width, deck_y + 5, length], "post"))
    save("structures", {"name": name, "notes": notes, "size": [width, deck_y + 6, length], "anchor": [width // 2, 0, 0],
                        "foundation": 24, "level": False, "base": "sea", "tags": ["lowmouth", "walkway"],
                        "slots": {"deck": "weathered_planks", "post": "timber_frame"}, "ops": ops})


walkway("stilt_walkway", "A plank boardwalk on posts 1.5 m over the water, 8 m long, starting at the anchor and running toward local +Z; posts every 2.5 m, a rail on one side. A kayak passes under it between the posts. Base sea.",
        32, 10)


def footbridge():
    """A humped plank footbridge 16 m long over a channel: posts only near the
    ends, the middle 2.5 m clear of the water so a kayak passes under."""
    L, W = 64, 6
    ops = []
    for z in range(L):
        t = abs(z - L / 2) / (L / 2)
        y = int(round(14 - 4 * t * t))
        ops.append(box([0, y, z], [W, y + 1, z + 1], "deck", "floor"))
        if z % 4 == 0:
            ops.append(box([W - 1, y + 1, z], [W, y + 4, z + 1], "post"))
            ops.append(box([0, y + 1, z], [1, y + 4, z + 1], "post"))
        ops.append(box([0, y + 4, z], [1, y + 5, z + 1], "post"))
        ops.append(box([W - 1, y + 4, z], [W, y + 5, z + 1], "post"))
    for z in (1, 8, L - 9, L - 2):
        t = abs(z - L / 2) / (L / 2)
        y = int(round(14 - 4 * t * t))
        for x in (0, W - 1):
            ops.append(box([x, 0, z], [x + 1, y, z + 1], "post"))
    save("structures", {"name": "stilt_footbridge", "notes": "A humped plank footbridge 16 m long on posts, running from the anchor toward local +Z: its middle stands 2.5 m over the water with no posts under it, so a kayak passes beneath. Base sea.",
                        "size": [W, 20, L], "anchor": [W // 2, 0, 0], "foundation": 24, "level": False, "base": "sea",
                        "tags": ["lowmouth", "walkway"], "slots": {"deck": "weathered_planks", "post": "timber_frame"}, "ops": ops})


footbridge()


def net_shed():
    ops = posts(0, 16, 0, 20, 0, 10, 7)
    ops += [box([0, 10, 0], [16, 11, 20], "deck", "floor")]
    ops += [{"op": "walls", "from": [0, 11, 0], "to": [16, 20, 20], "mat": "wall", "thick": 1, "part": "wall"},
            {"op": "box", "from": [3, 11, 0], "to": [13, 19, 1], "mat": "air", "part": "wall"},
            {"op": "gable", "from": [-1, 20, -1], "to": [17, 20, 21], "axis": "x", "mat": "roof", "rise": 1, "part": "roof"}]
    # Nets drying on a rail in front.
    ops += [box([0, 11, -3], [1, 17, -2], "post"), box([15, 11, -3], [16, 17, -2], "post"),
            box([0, 16, -3], [16, 17, -2], "post")]
    for x in range(1, 15, 2):
        ops.append({"op": "box", "from": [x, 12, -3], "to": [x + 1, 16, -2], "mat": "net", "part": "cloth"})
    save("structures", {"name": "net_shed", "notes": "An open-fronted net shed on posts over the water, nets drying on a rail along its front (local -Z). Base sea.",
                        "size": [18, 26, 24], "anchor": [8, 0, 10], "foundation": 24, "level": False, "base": "sea",
                        "tags": ["lowmouth"], "slots": {"wall": "weathered_planks", "roof": "corrugated_tin", "deck": "weathered_planks",
                                                         "post": "timber_frame", "net": "canvas"}, "ops": ops})


net_shed()


def houseboat():
    ops = [box([0, 3, 0], [16, 6, 36], "hull", "timber"),
           {"op": "walls", "from": [2, 6, 6], "to": [14, 16, 32], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "box", "from": [1, 16, 4], "to": [15, 17, 34], "mat": "roof", "part": "roof"},
           {"op": "box", "from": [6, 6, 6], "to": [10, 14, 7], "mat": "door", "part": "timber"}]
    ops += window(3, 10, 2, 3, 6, 7, "lamp") + window(11, 10, 2, 3, 6, 7, "lamp")
    for z in (12, 20):
        ops += [{"op": "box", "from": [2, 10, z], "to": [3, 13, z + 3], "mat": "window", "part": "glass"},
                {"op": "box", "from": [13, 10, z], "to": [14, 13, z + 3], "mat": "window", "part": "glass"}]
    ops += [box([0, 6, 0], [1, 9, 6], "post"), box([15, 6, 0], [16, 9, 6], "post"), box([0, 8, 0], [16, 9, 1], "post")]
    save("structures", {"name": "houseboat", "notes": "A houseboat moored in the channel: a flat hull awash, a board cabin with a flat tin roof, a lamp in the windows, a railed foredeck (local -Z). Base sea.",
                        "size": [16, 18, 36], "anchor": [8, 0, 18], "foundation": 0, "level": False, "base": "sea",
                        "tags": ["lowmouth"], "slots": {"hull": "weathered_planks", "wall": "siding_sage", "roof": "corrugated_tin",
                                                         "post": "timber_frame", "door": "door_red", "window": "window_glass", "lamp": "lamplit_glass"},
                        "ops": ops})


houseboat()


def land_shack(name, notes, w, d, wall_h, slots, tags, lit, extra=None, piers=3):
    """A board shack on short piers on dry ground, porch and tin gable roof."""
    x0, x1, porch = 1, 1 + w, 7
    z0, z1 = 1 + porch, 1 + porch + d
    fy = piers
    ops = posts(x0, x1, 1, z1, 0, fy, 6)
    ops += [box([x0, fy, 1], [x1, fy + 1, z1], "deck", "floor"),
            {"op": "walls", "from": [x0, fy + 1, z0], "to": [x1, fy + 1 + wall_h, z1], "mat": "wall", "thick": 1, "part": "wall"}]
    for x in range(x0, x1, max(4, w // 4)):
        ops.append(box([x, fy + 1, 1], [x + 1, fy + 1 + wall_h, 2], "post"))
    ops.append(box([x1 - 1, fy + 1, 1], [x1, fy + 1 + wall_h, 2], "post"))
    ops.append({"op": "box", "from": [x0 - 1, fy + 1 + wall_h, 0], "to": [x1 + 1, fy + 2 + wall_h, z0], "mat": "roof", "part": "roof"})
    ops.append({"op": "gable", "from": [x0 - 1, fy + 1 + wall_h, z0 - 1], "to": [x1 + 1, fy + 1 + wall_h, z1 + 1],
                "axis": "z", "mat": "roof", "rise": 1, "part": "roof"})
    mid = (x0 + x1) // 2
    ops.append({"op": "box", "from": [mid - 2, fy + 1, z0], "to": [mid + 2, fy + 9, z0 + 1], "mat": "door", "part": "timber"})
    win = "lamp" if lit else "window"
    ops += window(x0 + 2, fy + 4, 4, 4, z0, z0 + 1, win) + window(x1 - 6, fy + 4, 4, 4, z0, z0 + 1, win)
    for z in range(z0 + 5, z1 - 4, 9):
        ops += [{"op": "box", "from": [x0, fy + 4, z], "to": [x0 + 1, fy + 8, z + 3], "mat": win, "part": "glass"},
                {"op": "box", "from": [x1 - 1, fy + 4, z], "to": [x1, fy + 8, z + 3], "mat": win, "part": "glass"}]
    ops += extra(x0, x1, z0, z1, fy, wall_h) if extra else []
    top = fy + 2 + wall_h + (w + 2) // 2 + 4
    save("structures", {"name": name, "notes": notes, "size": [w + 2, top, z1 + 1], "anchor": [(w + 2) // 2, 0, (z1 + 1) // 2],
                        "foundation": 6, "level": True, "tags": tags, "slots": slots, "ops": ops})


def bait_sign(x0, x1, z0, z1, fy, wh):
    return [{"op": "box", "from": [x0 + 2, fy + wh + 2, 0], "to": [x1 - 2, fy + wh + 6, 1], "mat": "sign", "part": "timber"}]


land_shack("bait_shop", "Theriot's bait and tackle at the river landing: a board shack on short piers with a porch, a painted sign over it and a tin roof. Door on local -Z.",
           16, 22, 11, dict(HOUSE, wall="siding_cream", door="door_red", sign="sign_white"), ["lowmouth"], False, bait_sign)


def juke_extra(x0, x1, z0, z1, fy, wh):
    ops = [{"op": "box", "from": [x0 + 1, fy + wh + 2, 0], "to": [x1 - 1, fy + wh + 7, 1], "mat": "sign", "part": "timber"}]
    # A string of lights along the porch eaves, and the door stood open.
    for x in range(x0, x1, 3):
        ops.append({"op": "box", "from": [x, fy + wh, 1], "to": [x + 1, fy + wh + 1, 2], "mat": "lamp", "part": "glass"})
    mid = (x0 + x1) // 2
    ops.append({"op": "box", "from": [mid - 2, fy + 1, z0], "to": [mid + 2, fy + 9, z0 + 1], "mat": "air", "part": "timber"})
    ops.append({"op": "box", "from": [mid - 6, fy + 1, z0 + 6], "to": [mid + 6, fy + 2, z0 + 12], "mat": "lamp", "part": "floor"})
    return ops


land_shack("juke_joint", "The juke joint at the crossroads: a tin-roofed board hall with a hand-painted sign, a string of lights along the porch, lamplit windows and the door stood open. Door on local -Z.",
           24, 30, 12, dict(HOUSE, wall="siding_slate", door="door_red", sign="sign_yellow"), ["lowmouth", "crossroads"], True, juke_extra, piers=2)


def crossroads_post():
    ops = [box([7, 0, 7], [8, 16, 8], "post"),
           box([0, 13, 7], [7, 14, 8], "board"), box([8, 11, 7], [15, 12, 8], "board"),
           box([7, 9, 0], [8, 10, 7], "board"), box([7, 14, 8], [8, 15, 15], "board"),
           # A lantern hung from a crook on its own post.
           box([12, 0, 12], [13, 14, 13], "post"), box([10, 13, 12], [12, 14, 13], "post"),
           {"op": "box", "from": [10, 11, 12], "to": [11, 13, 13], "mat": "lamp", "part": "glass"}]
    save("structures", {"name": "crossroads_post", "notes": "The finger post where the levee road crosses the parish road: four boards pointing four ways, and a lantern on a crook beside it. Stand it in the middle of the crossing.",
                        "size": [15, 17, 15], "anchor": [7, 0, 7], "foundation": 2, "level": False, "decays": False,
                        "tags": ["crossroads"], "slots": {"post": "timber_frame", "board": "weathered_planks", "lamp": "lamplit_glass"}, "ops": ops})


crossroads_post()


def offering():
    ops = []
    # A chalk figure drawn on the road: a cross in a ring with four arms ending in hooks.
    ops += [{"op": "box", "from": [4, 0, 15], "to": [27, 1, 16], "mat": "chalk", "part": "floor"},
            {"op": "box", "from": [15, 0, 4], "to": [16, 1, 27], "mat": "chalk", "part": "floor"}]
    import math
    for k in range(48):
        a = k / 48 * 2 * math.pi
        x, z = 15.5 + 9 * math.cos(a), 15.5 + 9 * math.sin(a)
        ops.append({"op": "set", "at": [int(x), 0, int(z)], "mat": "chalk", "part": "floor"})
    for (x, z) in ((4, 14), (4, 16), (26, 14), (26, 16), (14, 4), (16, 4), (14, 26), (16, 26)):
        ops.append({"op": "set", "at": [x, 0, z], "mat": "chalk", "part": "floor"})
    # Candles round it, a bottle and a glass, a little pile at the centre.
    for k in range(10):
        a = k / 10 * 2 * math.pi + 0.3
        x, z = int(15.5 + 12 * math.cos(a)), int(15.5 + 12 * math.sin(a))
        ops += [{"op": "box", "from": [x, 1, z], "to": [x + 1, 2, z + 1], "mat": "wax", "part": "floor"},
                {"op": "set", "at": [x, 2, z], "mat": "candle_flame", "part": "floor"}]
    ops += [{"op": "box", "from": [17, 1, 13], "to": [18, 4, 14], "mat": "bottle_violet", "part": "glass"},
            {"op": "box", "from": [13, 1, 17], "to": [14, 3, 18], "mat": "bottle_blue", "part": "glass"},
            {"op": "box", "from": [15, 1, 15], "to": [16, 2, 16], "mat": "candle_flame", "part": "floor"}]
    save("structures", {"name": "crossroads_offering", "notes": "Left at the crossroads in the night: a figure drawn in chalk on the road, candles burning round it, a bottle and a glass. Flat on the ground; put it in the middle of a crossing.",
                        "size": [32, 5, 32], "anchor": [16, 0, 16], "foundation": 0, "level": False, "decays": False,
                        "tags": ["crossroads"], "slots": {"wax": "tomb_lime"}, "ops": ops})


offering()


def figure():
    """A tall thin man in a long coat and a wide hat, leaning on a cane."""
    ops = [{"op": "box", "from": [6, 0, 7], "to": [8, 9, 9], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [9, 0, 7], "to": [11, 9, 9], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [5, 9, 6], "to": [12, 20, 10], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [4, 13, 7], "to": [5, 19, 9], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [12, 13, 7], "to": [13, 19, 9], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [7, 20, 7], "to": [10, 23, 10], "mat": "face", "part": "cloth"},
           {"op": "box", "from": [4, 23, 4], "to": [13, 24, 13], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [6, 24, 6], "to": [11, 26, 11], "mat": "coat", "part": "cloth"},
           {"op": "box", "from": [13, 0, 8], "to": [14, 14, 9], "mat": "cane", "part": "timber"}]
    save("structures", {"name": "crossroads_figure", "notes": "A tall thin man standing at the crossroads in a long dark coat and a wide hat, leaning on a cane. Faces local -Z.",
                        "size": [17, 27, 17], "anchor": [8, 0, 8], "foundation": 0, "level": False, "decays": False,
                        "tags": ["crossroads"], "slots": {"coat": "door_black", "face": "trim_dark", "cane": "silver_deadwood"}, "ops": ops})


figure()


def bottle_tree():
    import math
    ops = [{"op": "cylinder", "base": [16, 0, 16], "radius": 1.6, "height": 15, "mat": "trunk", "part": "timber"}]
    tips = []
    for k in range(7):
        a = k / 7 * 2 * math.pi + 0.4
        y0 = 7 + (k % 3) * 2
        length = 7 + (k % 2) * 3
        for s in range(length):
            x = 16 + math.cos(a) * s
            z = 16 + math.sin(a) * s
            y = y0 + s * 0.6
            ops.append({"op": "set", "at": [int(x), int(y), int(z)], "mat": "trunk", "part": "timber"})
        tips.append((16 + math.cos(a) * (length - 1), y0 + (length - 1) * 0.6, 16 + math.sin(a) * (length - 1)))
        # Bottles slid over the twig ends, neck down.
        mid = (16 + math.cos(a) * (length // 2), y0 + (length // 2) * 0.6, 16 + math.sin(a) * (length // 2))
        for (x, y, z), m in ((tips[-1], "bottle_blue" if k % 2 == 0 else "bottle_violet"), (mid, "bottle_blue")):
            ops.append({"op": "box", "from": [int(x), int(y) - 2, int(z)], "to": [int(x) + 1, int(y), int(z) + 1], "mat": m, "part": "glass"})
    save("structures", {"name": "bottle_tree", "notes": "A bottle tree: a dead cypress trunk with its limbs cut short, a blue or violet bottle pushed over the end of each to catch what wanders by at night. Faces anywhere.",
                        "size": [32, 24, 32], "anchor": [16, 0, 16], "foundation": 2, "level": False, "decays": False,
                        "tags": ["crossroads"], "slots": {"trunk": "silver_deadwood"}, "ops": ops})


bottle_tree()


def line3(a, b, mat="stick"):
    """Voxels along a line, stepping one axis at a time so each joins the
    last face to face (no gaps where it runs diagonally)."""
    (x0, y0), (x1, y1) = a, b
    out, x, y = [], x0, y0
    out.append((x, y))
    while (x, y) != (x1, y1):
        dx, dy = x1 - x, y1 - y
        if abs(dx) * 1.0 / max(1, abs(x1 - x0)) >= abs(dy) * 1.0 / max(1, abs(y1 - y0)) and dx != 0:
            x += 1 if dx > 0 else -1
        else:
            y += 1 if dy > 0 else -1
        out.append((x, y))
    return out


def lattice(name, scale, notes):
    """A stake with a woven frame of sticks: crossed diagonals, a bar, a
    hoop of sticks round them and antler-like tips."""
    k = scale
    W = 9 * k
    cx = W // 2
    pts = set()
    lines = [((1 * k, 6 * k), (8 * k - 1, 13 * k)), ((8 * k - 1, 6 * k), (1 * k, 13 * k)), ((1 * k, 9 * k + k // 2), (8 * k - 1, 9 * k + k // 2)),
             ((cx, 5 * k), (cx, 14 * k)),
             ((cx, 14 * k), (cx - 2 * k, 15 * k + k)), ((cx, 14 * k), (cx + 2 * k, 15 * k + k)),
             ((cx - 2 * k, 15 * k + k), (cx - 3 * k, 15 * k + k + 2)), ((cx + 2 * k, 15 * k + k), (cx + 3 * k, 15 * k + k + 2))]
    import math
    r = 3.6 * k
    ring = [(round(cx + r * math.cos(t / 24 * 2 * math.pi)), round(9.5 * k + r * math.sin(t / 24 * 2 * math.pi))) for t in range(25)]
    for p0, p1 in zip(ring, ring[1:]):
        lines.append((p0, p1))
    for a0, b0 in lines:
        for q in line3(a0, b0):
            pts.add(q)
    ops = [{"op": "box", "from": [cx, 0, 2], "to": [cx + 1, 6 * k, 3], "mat": "stick", "part": "timber"}]
    ops += [{"op": "set", "at": [x, y, 2], "mat": "stick", "part": "timber"} for (x, y) in sorted(pts) if x >= 0 and y >= 0]
    top = max(y for _, y in pts) + 1
    save("structures", {"name": name, "notes": notes, "size": [W, top + 1, 5], "anchor": [cx, 0, 2], "foundation": 2, "level": False, "decays": False,
                        "tags": ["crossroads"], "slots": {"stick": "stick_wood"}, "ops": ops})


lattice("stick_lattice", 1, "A woven lattice of sticks lashed to a stake, the size of a man's chest: crossed sticks in a hoop, antler-like tips at the top. Someone's mark along the paths through the cane. Flat face to local -Z.")
lattice("stick_lattice_tall", 2, "A big woven lattice of sticks on a stake, taller than a man: crossed sticks in a hoop, antler-like tips at the top. Flat face to local -Z.")


def cane_houses():
    land_shack("cane_cottage", "A cane-worker's cottage in Mire: a board shotgun house on short brick piers, a deep front porch, a tin roof. Door on local -Z.",
               14, 26, 11, dict(HOUSE, wall="white_siding", door="door_green"), ["mire", "house"], False)
    land_shack("cane_cottage_b", "A wider cottage in Mire: board walls, a porch across the front, a tin roof. Door on local -Z.",
               20, 22, 11, dict(HOUSE, wall="siding_sage", door="door_red"), ["mire", "house"], False)
    land_shack("cane_store", "Mire's general store: a board store on piers with a sign across the porch roof and a tin roof. Door on local -Z.",
               22, 28, 13, dict(HOUSE, wall="siding_cream", door="door_black", sign="sign_white"), ["mire"], False, bait_sign)


cane_houses()


def drowned_church():
    """A white board church with a square tower, its floor 1 m under the bay."""
    ops = [{"op": "walls", "from": [6, 0, 16], "to": [38, 26, 76], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "gable", "from": [5, 26, 15], "to": [39, 26, 77], "axis": "z", "mat": "roof", "rise": 1, "part": "roof"},
           # Tower at the front with a belfry and a short spire.
           {"op": "walls", "from": [12, 0, 2], "to": [32, 52, 18], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "box", "from": [12, 52, 2], "to": [32, 53, 18], "mat": "trim", "part": "wall"},
           {"op": "gable", "from": [11, 53, 1], "to": [33, 53, 19], "axis": "x", "mat": "roof", "rise": 2, "part": "roof"}]
    for x in (16, 26):
        ops.append({"op": "box", "from": [x, 42, 2], "to": [x + 3, 50, 3], "mat": "air", "part": "wall"})
    ops.append({"op": "box", "from": [19, 4, 2], "to": [25, 18, 3], "mat": "door", "part": "timber"})
    for z in range(24, 72, 10):
        ops += [{"op": "box", "from": [6, 8, z], "to": [7, 20, z + 4], "mat": "window", "part": "glass"},
                {"op": "box", "from": [37, 8, z], "to": [38, 20, z + 4], "mat": "window", "part": "glass"}]
    # A cross on the gable end.
    ops += [box([21, 34, 76], [23, 42, 77], "trim", "wall"), box([19, 38, 76], [25, 40, 77], "trim", "wall")]
    save("structures", {"name": "drowned_church", "notes": "St. Anne of the Marrow, the parish church that went down with the old lobe: a white board church with a square front tower (local -Z), its floor a metre under the bay. Base sea; give it an age so the roof is going.",
                        "size": [44, 60, 78], "anchor": [22, 0, 40], "foundation": 8, "level": False, "base": "sea",
                        "tags": ["parish"], "slots": {"wall": "white_siding", "roof": "shingle_dark", "trim": "trim_white",
                                                      "door": "door_wood", "window": "pane_glass"}, "ops": ops})


drowned_church()


def sunken_house():
    """A house sunk to its eaves: walls 3.25 m, its floor 2.5 m under the sea
    when hung with `height` -2.5."""
    ops = [{"op": "walls", "from": [1, 0, 1], "to": [21, 13, 33], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "gable", "from": [0, 13, 0], "to": [22, 13, 34], "axis": "z", "mat": "roof", "rise": 1, "part": "roof"},
           {"op": "box", "from": [9, 0, 1], "to": [13, 9, 2], "mat": "door", "part": "timber"},
           {"op": "box", "from": [9, 13, 33], "to": [13, 26, 36], "mat": "chimney", "part": "wall"}]
    ops += window(3, 5, 3, 4, 1, 2) + window(16, 5, 3, 4, 1, 2)
    save("structures", {"name": "sunken_house", "notes": "A parish house sunk to its eaves in the bay: board walls under the water, a shingle roof and a brick chimney standing out of it. Place with height -2.5 (its floor that far under the sea). Door on local -Z.",
                        "size": [22, 27, 36], "anchor": [11, 0, 17], "foundation": 0, "level": False,
                        "tags": ["parish"], "slots": {"wall": "siding_grey", "roof": "shingle_moss", "door": "door_wood",
                                                      "chimney": "brick_brown", "window": "pane_glass"}, "ops": ops})


sunken_house()


def drowned_lights():
    """Lamplight in a sunken house after dark: a lit attic window in its front
    gable, just over the water, and a glow in the front windows under it."""
    ops = [{"op": "box", "from": [9, 14, 0], "to": [13, 17, 1], "mat": "lamp", "part": "glass"},
           {"op": "box", "from": [8, 13, 0], "to": [14, 14, 1], "mat": "sill", "part": "timber"},
           {"op": "box", "from": [3, 5, 2], "to": [6, 9, 3], "mat": "lamp", "part": "glass"},
           {"op": "box", "from": [16, 5, 2], "to": [19, 9, 3], "mat": "lamp", "part": "glass"}]
    save("structures", {"name": "drowned_lights", "notes": "Lamplight in a sunken_house: a lit attic window in its front gable just over the water, and the front windows lit under it. Place exactly over one (same spot, bearing and height).",
                        "size": [22, 18, 36], "anchor": [11, 0, 17], "foundation": 0, "level": False, "decays": False,
                        "tags": ["parish"], "slots": {"lamp": "lamplit_glass", "sill": "weathered_planks"}, "ops": ops})


drowned_lights()


def raised_tombs():
    ops = []
    for k, x in enumerate(range(0, 48, 12)):
        h = 7 + (k % 2)
        ops += [{"op": "box", "from": [x + 1, 0, 2], "to": [x + 10, h, 12], "mat": "tomb", "part": "stone"},
                {"op": "gable", "from": [x, h, 1], "to": [x + 11, h, 13], "axis": "z", "mat": "tomb", "rise": 1, "part": "stone"},
                {"op": "box", "from": [x + 4, 2, 1], "to": [x + 7, 6, 2], "mat": "slab", "part": "stone"},
                box([x + 5, h + 6, 6], [x + 6, h + 11, 7], "iron", "metal"),
                box([x + 4, h + 9, 6], [x + 7, h + 10, 7], "iron", "metal")]
    save("structures", {"name": "raised_tombs", "notes": "A row of four whitewashed tombs built above the ground (the dead can't be buried in ground this wet), each with a gabled top and an iron cross. Fronts face local -Z.",
                        "size": [48, 20, 14], "anchor": [24, 0, 7], "foundation": 4, "level": True,
                        "tags": ["parish", "grave"], "slots": {"tomb": "tomb_lime", "slab": "fieldstone", "iron": "rust_iron"}, "ops": ops})
    sunk = [dict(o) for o in ops]
    save("structures", {"name": "sunken_tombs", "notes": "The parish tombs where the bay came over them: the same whitewashed row, standing in a metre of water. Base sea.",
                        "size": [48, 20, 14], "anchor": [24, 0, 7], "foundation": 6, "level": False, "base": "sea",
                        "tags": ["parish", "grave"], "slots": {"tomb": "tomb_lime", "slab": "fieldstone", "iron": "rust_iron"}, "ops": sunk})


raised_tombs()


def sugar_mill():
    ops = [{"op": "walls", "from": [0, 0, 0], "to": [48, 18, 72], "mat": "wall", "thick": 2, "part": "wall"},
           {"op": "box", "from": [18, 0, 0], "to": [30, 14, 2], "mat": "air", "part": "wall"},
           {"op": "box", "from": [0, 6, 20], "to": [2, 13, 26], "mat": "air", "part": "wall"},
           {"op": "box", "from": [46, 6, 40], "to": [48, 13, 46], "mat": "air", "part": "wall"},
           # The chimney at the back corner.
           {"op": "box", "from": [36, 0, 58], "to": [48, 64, 70], "mat": "wall", "part": "stone"},
           {"op": "box", "from": [39, 2, 61], "to": [45, 64, 67], "mat": "air", "part": "stone"},
           {"op": "box", "from": [35, 62, 57], "to": [49, 64, 71], "mat": "wall", "part": "stone"},
           # Rusted kettles inside.
           {"op": "cylinder", "base": [12, 0, 30], "radius": 5, "height": 5, "mat": "iron", "part": "metal"},
           {"op": "cylinder", "base": [26, 0, 30], "radius": 5, "height": 5, "mat": "iron", "part": "metal"}]
    save("structures", {"name": "sugar_mill_ruin", "notes": "The old sugar house: roofless brick walls 4.5 m high with an arched cart door (local -Z), a tall square brick chimney at the back corner and the rusted sugar kettles still inside. Give it an age.",
                        "size": [50, 66, 72], "anchor": [24, 0, 36], "foundation": 6, "level": True,
                        "tags": ["levee"], "slots": {"wall": "brick_brown", "iron": "rust_iron"}, "ops": ops})


sugar_mill()


def brick_ruins():
    """Outbuildings of the sugar house, gone to ruin with it."""
    # The quarters: a row of three brick rooms, roofless, a chimney between each pair.
    ops = [{"op": "walls", "from": [0, 0, 0], "to": [64, 12, 20], "mat": "wall", "thick": 2, "part": "wall"}]
    for x in (20, 42):
        ops.append({"op": "box", "from": [x, 0, 2], "to": [x + 2, 12, 18], "mat": "wall", "part": "wall"})
        ops.append({"op": "box", "from": [x - 1, 0, 14], "to": [x + 3, 22, 20], "mat": "wall", "part": "stone"})
    for x in (7, 29, 51):
        ops += [{"op": "box", "from": [x, 0, 0], "to": [x + 4, 9, 2], "mat": "air", "part": "wall"},
                {"op": "box", "from": [x + 7, 4, 0], "to": [x + 10, 8, 2], "mat": "air", "part": "wall"}]
    save("structures", {"name": "brick_quarters", "notes": "The sugar house's quarters: a roofless row of three brick rooms, a door and a window each on local -Z, a chimney stack between each pair. Give it an age.",
                        "size": [64, 24, 20], "anchor": [32, 0, 10], "foundation": 4, "level": True,
                        "tags": ["levee"], "slots": {"wall": "brick_brown"}, "ops": ops})
    # A round brick cistern, its iron hoops rusted.
    ops = [{"op": "cylinder", "base": [12, 0, 12], "radius": 10, "height": 14, "mat": "wall", "part": "wall"},
           {"op": "cylinder", "base": [12, 1, 12], "radius": 8, "height": 14, "mat": "air", "part": "wall"},
           {"op": "cylinder", "base": [12, 4, 12], "radius": 10.4, "height": 1, "mat": "iron", "part": "metal"},
           {"op": "cylinder", "base": [12, 10, 12], "radius": 10.4, "height": 1, "mat": "iron", "part": "metal"}]
    save("structures", {"name": "brick_cistern", "notes": "A round brick cistern 5 m across that caught rainwater for the sugar house, open at the top, two rusted iron hoops round it.",
                        "size": [25, 15, 25], "anchor": [12, 0, 12], "foundation": 4, "level": True,
                        "tags": ["levee"], "slots": {"wall": "brick_brown", "iron": "rust_iron"}, "ops": ops})
    # A wall left standing on its own, with an arched opening.
    ops = [{"op": "box", "from": [0, 0, 0], "to": [36, 18, 2], "mat": "wall", "part": "wall"},
           {"op": "box", "from": [13, 0, 0], "to": [23, 10, 2], "mat": "air", "part": "wall"},
           {"op": "ellipsoid", "centre": [18, 10, 1], "radius": [5, 3.5, 2], "mat": "air", "part": "wall"},
           {"op": "box", "from": [0, 0, 2], "to": [3, 8, 6], "mat": "wall", "part": "stone"},
           {"op": "box", "from": [33, 0, 2], "to": [36, 8, 6], "mat": "wall", "part": "stone"}]
    save("structures", {"name": "brick_wall_ruin", "notes": "The last wall of a brick building, standing alone with an arched doorway through it and buttresses at the ends; faces local -Z.",
                        "size": [36, 18, 6], "anchor": [18, 0, 3], "foundation": 4, "level": True,
                        "tags": ["levee"], "slots": {"wall": "brick_brown"}, "ops": ops})


brick_ruins()


def pump_station():
    ops = [{"op": "box", "from": [0, 0, 0], "to": [20, 16, 20], "mat": "wall", "part": "wall"},
           {"op": "box", "from": [1, 16, 1], "to": [19, 17, 19], "mat": "roof", "part": "roof"},
           {"op": "box", "from": [8, 0, 0], "to": [12, 9, 1], "mat": "door", "part": "metal"}]
    for x in (4, 14):
        ops += [{"op": "log", "base": [x, 12, 20], "radius": 1.5, "length": 30, "axis": "z", "mat": "pipe", "part": "metal"}]
    save("structures", {"name": "pump_station", "notes": "A concrete pump house on the levee with two rusted pipes running out its back (local +Z) over the bank to the water. Door on local -Z.",
                        "size": [20, 18, 50], "anchor": [10, 0, 10], "foundation": 6, "level": True,
                        "tags": ["levee"], "slots": {"wall": "concrete", "roof": "roof_tar", "door": "rust_iron", "pipe": "rust_iron"}, "ops": ops})


pump_station()


def lighthouse():
    ops = [{"op": "cylinder", "base": [16, 0, 16], "radius": 7, "height": 4, "mat": "base", "part": "stone"},
           {"op": "cylinder", "base": [16, 4, 16], "radius": 5, "height": 54, "mat": "wall", "part": "wall"},
           {"op": "cylinder", "base": [16, 58, 16], "radius": 6.5, "height": 1, "mat": "dark", "part": "metal"},
           {"op": "cylinder", "base": [16, 59, 16], "radius": 4, "height": 6, "mat": "lamp", "part": "glass"},
           {"op": "cylinder", "base": [16, 65, 16], "radius": 4.5, "height": 2, "mat": "dark", "part": "metal"},
           {"op": "box", "from": [14, 4, 10], "to": [18, 12, 11], "mat": "door", "part": "timber"}]
    for y in (20, 34, 46):
        ops.append({"op": "box", "from": [15, y, 10], "to": [17, y + 3, 11], "mat": "window", "part": "glass"})
    ops.append({"op": "repeat", "count": 3, "step": [0, 16, 0], "ops": [
        {"op": "cylinder", "base": [16, 12, 16], "radius": 5, "height": 3, "mat": "dark", "part": "wall"}]})
    save("structures", {"name": "lighthouse", "notes": "The light on the chenier's point: a white tower 15 m tall banded in black, the lamp room on top lit day and night. Door on local -Z.",
                        "size": [32, 68, 32], "anchor": [16, 0, 16], "foundation": 6, "level": True,
                        "tags": ["chenier"], "slots": {"base": "concrete", "wall": "white_siding", "dark": "door_black",
                                                       "lamp": "lamplit_glass", "door": "door_black", "window": "pane_glass"}, "ops": ops})


lighthouse()


def shrimp_boat():
    import math
    ops = []
    for z in range(4, 64):
        t = (z - 34) / 30
        half = 8 * math.sqrt(max(0.0, 1 - t ** 4)) * (0.55 if z < 12 else 1.0)
        if half < 1:
            continue
        x0, x1 = int(16 - half), int(16 + half)
        ops.append({"op": "box", "from": [x0, 0, z], "to": [x1, 8 + (2 if z < 14 else 0), z + 1], "mat": "hull", "part": "timber"})
    ops += [{"op": "box", "from": [10, 8, 30], "to": [22, 16, 44], "mat": "cabin", "part": "wall"},
            {"op": "box", "from": [9, 16, 29], "to": [23, 17, 45], "mat": "roof", "part": "roof"},
            {"op": "box", "from": [11, 11, 30], "to": [21, 14, 31], "mat": "window", "part": "glass"},
            {"op": "box", "from": [15, 8, 48], "to": [17, 34, 50], "mat": "mast", "part": "timber"},
            {"op": "box", "from": [0, 30, 48], "to": [32, 31, 50], "mat": "mast", "part": "timber"}]
    save("structures", {"name": "shrimp_boat_wreck", "notes": "A shrimp trawler sunk at its mooring: the hull settled in the mud to the gunwales, the wheelhouse and the outrigger mast standing out of the water. Bow to local -Z. Base sea.",
                        "size": [32, 36, 66], "anchor": [16, 0, 33], "foundation": 0, "level": False, "base": "sea",
                        "tags": ["mouth"], "slots": {"hull": "siding_cream", "cabin": "white_siding", "roof": "corrugated_tin",
                                                    "window": "window_glass", "mast": "rust_iron"}, "ops": ops})


shrimp_boat()


def steamboat():
    """A paddle steamer, two white decks lit at every window, two tall stacks."""
    ops = [{"op": "box", "from": [2, 3, 0], "to": [30, 7, 120], "mat": "hull", "part": "timber"},
           {"op": "box", "from": [0, 7, 6], "to": [32, 8, 116], "mat": "deck", "part": "floor"},
           {"op": "walls", "from": [3, 8, 14], "to": [29, 16, 108], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "box", "from": [1, 16, 10], "to": [31, 17, 112], "mat": "deck", "part": "floor"},
           {"op": "walls", "from": [6, 17, 22], "to": [26, 24, 96], "mat": "wall", "thick": 1, "part": "wall"},
           {"op": "box", "from": [5, 24, 20], "to": [27, 25, 98], "mat": "roof", "part": "roof"},
           {"op": "box", "from": [11, 25, 30], "to": [21, 31, 40], "mat": "wall", "part": "wall"},
           {"op": "box", "from": [10, 31, 29], "to": [22, 32, 41], "mat": "roof", "part": "roof"},
           {"op": "cylinder", "base": [11, 25, 50], "radius": 1.6, "height": 26, "mat": "stack", "part": "metal"},
           {"op": "cylinder", "base": [21, 25, 50], "radius": 1.6, "height": 26, "mat": "stack", "part": "metal"},
           # The stern wheel.
           {"op": "disc", "centre": [16, 10, 114], "radius": 8, "thick": 22, "axis": "x", "mat": "wheel", "part": "timber"}]
    for z in range(18, 104, 5):
        ops += [{"op": "box", "from": [3, 10, z], "to": [4, 14, z + 2], "mat": "lamp", "part": "glass"},
                {"op": "box", "from": [28, 10, z], "to": [29, 14, z + 2], "mat": "lamp", "part": "glass"}]
    for z in range(26, 94, 6):
        ops += [{"op": "box", "from": [6, 19, z], "to": [7, 22, z + 2], "mat": "lamp", "part": "glass"},
                {"op": "box", "from": [25, 19, z], "to": [26, 22, z + 2], "mat": "lamp", "part": "glass"}]
    save("structures", {"name": "ghost_steamboat", "notes": "A sternwheel paddle steamer, two white decks with lamplight at every window and two tall black stacks: the Belle Marrow, lost in the flood of the old century. Bow to local -Z. Base sea.",
                        "size": [32, 52, 124], "anchor": [16, 0, 60], "foundation": 0, "level": False, "decays": False, "base": "sea",
                        "tags": ["parish"], "slots": {"hull": "trim_dark", "deck": "weathered_planks", "wall": "white_siding",
                                                      "roof": "trim_white", "stack": "door_black", "wheel": "station_red", "lamp": "lamplit_glass"},
                        "ops": ops})


steamboat()


def oil_platform():
    """A small oil well platform on steel piles out in the Gulf: a deck, a
    wellhead and a short derrick, a shed, a ladder."""
    ops = []
    for x in (2, 29):
        for z in (2, 29):
            ops.append({"op": "box", "from": [x, 0, z], "to": [x + 2, 14, z + 2], "mat": "steel", "part": "metal"})
    ops += [{"op": "box", "from": [0, 14, 0], "to": [33, 15, 33], "mat": "deck", "part": "floor"},
            {"op": "box", "from": [0, 15, 0], "to": [33, 17, 1], "mat": "rail", "part": "metal"},
            {"op": "box", "from": [0, 15, 32], "to": [33, 17, 33], "mat": "rail", "part": "metal"},
            {"op": "box", "from": [0, 15, 0], "to": [1, 17, 33], "mat": "rail", "part": "metal"},
            {"op": "box", "from": [32, 15, 0], "to": [33, 17, 33], "mat": "rail", "part": "metal"},
            {"op": "box", "from": [20, 15, 18], "to": [31, 24, 31], "mat": "shed", "part": "wall"},
            {"op": "box", "from": [19, 24, 17], "to": [32, 25, 32], "mat": "rail", "part": "roof"},
            {"op": "box", "from": [8, 15, 8], "to": [12, 19, 12], "mat": "steel", "part": "metal"}]
    # The derrick: four legs leaning in to a crown, braced.
    for (x0, z0, dx, dz) in ((3, 3, 1, 1), (16, 3, -1, 1), (3, 16, 1, -1), (16, 16, -1, -1)):
        for y in range(15, 55):
            k = (y - 15) // 6
            ops.append({"op": "set", "at": [x0 + dx * k, y, z0 + dz * k], "mat": "steel", "part": "metal"})
    for y in range(20, 52, 8):
        k = (y - 15) // 6
        ops.append({"op": "box", "from": [3 + k, y, 3 + k], "to": [17 - k, y + 1, 4 + k], "mat": "steel", "part": "metal"})
        ops.append({"op": "box", "from": [3 + k, y, 16 - k], "to": [17 - k, y + 1, 17 - k], "mat": "steel", "part": "metal"})
    ops.append({"op": "box", "from": [8, 55, 8], "to": [12, 56, 12], "mat": "lamp", "part": "glass"})
    save("structures", {"name": "oil_platform", "notes": "A small oil well platform out in the Gulf: a steel deck on four piles 2.5 m over the water, a wellhead, a shed and a short derrick with a light on top. Base sea; the piles run down to the bed.",
                        "size": [33, 57, 33], "anchor": [16, 0, 16], "foundation": 60, "level": False, "base": "sea", "decays": True,
                        "tags": ["gulf"], "slots": {"steel": "bridge_steel", "rail": "rust_iron", "deck": "concrete", "shed": "siding_grey", "lamp": "lamplit_glass"},
                        "ops": ops})


oil_platform()


def fort_ruin():
    """A brick coastal fort: a half-moon wall with gun embrasures facing the
    Gulf, a sally port, a powder magazine, half of it gone."""
    import math
    ops = []
    R = 34
    for k in range(0, 181, 3):
        a = math.radians(k)
        x, z = int(40 + R * math.cos(a)), int(4 + R * math.sin(a))
        h = 18 if k % 18 else 12
        ops.append({"op": "box", "from": [x - 1, 0, z - 1], "to": [x + 2, h, z + 2], "mat": "wall", "part": "wall"})
    ops += [{"op": "box", "from": [6, 0, 0], "to": [74, 14, 3], "mat": "wall", "part": "wall"},
            {"op": "box", "from": [36, 0, 0], "to": [44, 10, 3], "mat": "air", "part": "wall"},
            {"op": "box", "from": [32, 0, 18], "to": [48, 10, 28], "mat": "wall", "part": "stone"},
            {"op": "box", "from": [34, 0, 20], "to": [46, 8, 26], "mat": "air", "part": "stone"},
            {"op": "box", "from": [38, 0, 18], "to": [42, 7, 20], "mat": "air", "part": "stone"},
            {"op": "cylinder", "base": [22, 0, 22], "radius": 2, "height": 2, "mat": "iron", "part": "metal"},
            {"op": "log", "base": [22, 2, 22], "radius": 1.2, "length": 9, "axis": "z", "mat": "iron", "part": "metal"}]
    save("structures", {"name": "fort_ruin", "notes": "Fort St. Jean, a brick half-moon fort that guarded the old pass: a curved wall with embrasures facing the Gulf (local +Z), a straight back wall with a sally port (local -Z), a vaulted magazine and one gun left. Give it an age.",
                        "size": [80, 18, 40], "anchor": [40, 0, 14], "foundation": 6, "level": True,
                        "tags": ["parish"], "slots": {"wall": "brick_brown", "iron": "rust_iron"}, "ops": ops})


fort_ruin()


def duck_blind():
    ops = [{"op": "box", "from": [0, 0, 0], "to": [1, 10, 1], "mat": "post", "part": "timber"},
           {"op": "box", "from": [11, 0, 0], "to": [12, 10, 1], "mat": "post", "part": "timber"},
           {"op": "box", "from": [0, 0, 9], "to": [1, 10, 10], "mat": "post", "part": "timber"},
           {"op": "box", "from": [11, 0, 9], "to": [12, 10, 10], "mat": "post", "part": "timber"},
           {"op": "box", "from": [0, 6, 0], "to": [12, 7, 10], "mat": "deck", "part": "floor"},
           {"op": "walls", "from": [0, 7, 0], "to": [12, 11, 10], "mat": "reed", "thick": 1, "part": "roof"},
           {"op": "box", "from": [3, 10, 0], "to": [9, 11, 1], "mat": "air", "part": "roof"}]
    save("structures", {"name": "duck_blind", "notes": "A duck blind out in the flats: a plank box on four posts walled with bundled reeds, a slot to look out of on local -Z. Base sea.",
                        "size": [12, 12, 10], "anchor": [6, 0, 5], "foundation": 10, "level": False, "base": "sea",
                        "tags": ["sawgrass"], "slots": {"post": "timber_frame", "deck": "weathered_planks", "reed": "thatch"}, "ops": ops})


duck_blind()


def barge_wreck():
    ops = [{"op": "walls", "from": [0, 0, 0], "to": [20, 9, 72], "mat": "hull", "thick": 1, "part": "metal"},
           {"op": "box", "from": [1, 0, 1], "to": [19, 2, 71], "mat": "hull", "part": "metal"},
           {"op": "box", "from": [0, 4, 30], "to": [20, 9, 40], "mat": "air", "part": "metal"},
           {"op": "box", "from": [2, 9, 60], "to": [18, 16, 70], "mat": "cabin", "part": "wall"},
           {"op": "box", "from": [1, 16, 59], "to": [19, 17, 71], "mat": "hull", "part": "roof"}]
    save("structures", {"name": "barge_wreck", "notes": "A rusted cane barge sunk on the river bank, broken across the middle, its little deckhouse still at the stern (local +Z). Base sea.",
                        "size": [20, 17, 72], "anchor": [10, 0, 36], "foundation": 0, "level": False, "base": "sea",
                        "tags": ["river"], "slots": {"hull": "rust_iron", "cabin": "siding_rust"}, "ops": ops})


barge_wreck()


def hyacinth_raft():
    import math
    ops = []
    pads = [(10, 12, 7), (22, 9, 6), (30, 18, 8), (16, 24, 7), (28, 30, 6), (8, 30, 5)]
    for (x, z, r) in pads:
        ops.append({"op": "cylinder", "base": [x, 4, z], "radius": r, "height": 1, "mat": "hyacinth_leaves"})
    for k in range(14):
        a = k * 2.3
        x, z, r = pads[k % len(pads)]
        ops.append({"op": "box", "from": [int(x + math.cos(a) * r * 0.5), 5, int(z + math.sin(a) * r * 0.5)],
                    "to": [int(x + math.cos(a) * r * 0.5) + 1, 7, int(z + math.sin(a) * r * 0.5) + 1], "mat": "hyacinth_flower"})
    save("structures", {"name": "hyacinth_raft", "notes": "A floating mat of water hyacinth on still water, glossy leaves and lilac flower spikes, 9 m across. Base sea; it floats at the surface.",
                        "size": [40, 8, 40], "anchor": [20, 0, 20], "foundation": 0, "level": False, "decays": False, "base": "sea",
                        "tags": ["swamp"], "ops": ops})


hyacinth_raft()


def lily_patch(name, flowers, seed):
    """Lily pads floating in a patch, a few in flower. Base sea: y = 4 is the
    water's surface."""
    import random
    rnd = random.Random(seed)
    ops = []
    pads = []
    for _ in range(26):
        for _try in range(20):
            x, z, r = rnd.uniform(3, 29), rnd.uniform(3, 29), rnd.choice([1.5, 2, 2, 2.5])
            if all((x - a) ** 2 + (z - b) ** 2 > (r + c + 0.5) ** 2 for a, b, c in pads):
                pads.append((x, z, r))
                break
    for (x, z, r) in pads:
        ops.append({"op": "cylinder", "base": [int(x), 4, int(z)], "radius": r, "height": 1, "mat": "lily_pad"})
        # The notch to the middle.
        ops.append({"op": "set", "at": [int(x) + 1, 4, int(z)], "mat": "air"})
    for (x, z, r) in pads[:flowers]:
        ops.append({"op": "box", "from": [int(x), 5, int(z)], "to": [int(x) + 1, 6, int(z) + 1], "mat": "lily_flower"})
    save("structures", {"name": name, "notes": "A patch of water lilies 8 m across: round pads flat on still water" + (", a few white flowers among them." if flowers else ", none in flower.") + " Base sea; it floats at the surface.",
                        "size": [32, 7, 32], "anchor": [16, 0, 16], "foundation": 0, "level": False, "decays": False, "base": "sea",
                        "tags": ["swamp"], "ops": ops})


lily_patch("lily_pads", 0, 11)
lily_patch("lily_pads_flowering", 5, 23)


def cypress_knees():
    import math
    ops = []
    spots = [(6, 6, 4), (11, 4, 6), (14, 10, 5), (5, 13, 7), (10, 15, 4), (15, 3, 3), (2, 9, 5)]
    for (x, z, h) in spots:
        ops.append({"op": "cylinder", "base": [x, 0, z], "radius": 1.2, "height": 3 + h // 2, "mat": "cypress_bark"})
        ops.append({"op": "cylinder", "base": [x, 3 + h // 2, z], "radius": 0.7, "height": h, "mat": "cypress_bark"})
    save("structures", {"name": "cypress_knees", "notes": "A cluster of cypress knees: knobbly cones of root standing out of the black water round a cypress. Base sea; they rise from the mud to half a metre over the surface.",
                        "size": [18, 14, 18], "anchor": [9, 0, 9], "foundation": 10, "level": False, "decays": False, "base": "sea",
                        "tags": ["swamp"], "ops": ops})


cypress_knees()

# ---- creatures ----------------------------------------------------------------

def croc_ops():
    """A crocodilian 3 m long on a 32 grid (0.1 m voxels): long low body, a long
    flat snout, raised eyes, a row of scutes, a heavy tapering tail, four short
    splayed legs."""
    ops = [
        {"op": "ellipsoid", "centre": [16, 2.6, 15.5], "radius": [3.4, 1.9, 6.5], "mat": "hide"},
        {"op": "ellipsoid", "centre": [16, 2.7, 8.2], "radius": [2.5, 1.6, 2.6], "mat": "hide", "part": "head"},
        {"op": "box", "from": [14, 1, 3], "to": [18, 3, 8], "mat": "hide", "part": "head"},
        {"op": "box", "from": [15, 1, 0], "to": [17, 3, 3], "mat": "hide", "part": "head"},
        {"op": "box", "from": [13, 3, 8], "to": [15, 5, 10], "mat": "hide", "part": "head"},
        {"op": "box", "from": [17, 3, 8], "to": [19, 5, 10], "mat": "hide", "part": "head"},
        {"op": "set", "at": [13, 4, 8], "mat": "eye", "part": "head"},
        {"op": "set", "at": [18, 4, 8], "mat": "eye", "part": "head"},
        {"op": "box", "from": [14, 1, 21], "to": [18, 4, 26], "mat": "hide", "part": "tail"},
        {"op": "box", "from": [15, 1, 26], "to": [17, 3, 31], "mat": "hide", "part": "tail_tip"},
        {"op": "box", "from": [15, 3, 26], "to": [17, 4, 30], "mat": "ridge", "part": "tail_tip"},
        {"op": "box", "from": [15, 4, 21], "to": [17, 5, 26], "mat": "ridge", "part": "tail"},
        {"op": "box", "from": [10, 0, 10], "to": [13, 2, 13], "mat": "hide", "part": "leg_fl"},
        {"op": "box", "from": [19, 0, 10], "to": [22, 2, 13], "mat": "hide", "part": "leg_fr"},
        {"op": "box", "from": [10, 0, 18], "to": [13, 2, 21], "mat": "hide", "part": "leg_hl"},
        {"op": "box", "from": [19, 0, 18], "to": [22, 2, 21], "mat": "hide", "part": "leg_hr"},
    ]
    for z in range(10, 21, 2):
        for x in (14, 17):
            ops.append({"op": "set", "at": [x, 4, z], "mat": "ridge"})
    ops.append({"op": "recolour_below", "y": 1, "from": "hide", "to": "belly"})
    return ops


CROC_RIG = {"plan": "sprawler", "bones": [
    {"name": "body", "role": "root"},
    {"name": "head", "role": "head", "parent": "body", "joint": [16, 2.6, 10.5]},
    {"name": "tail", "role": "tail", "parent": "body", "joint": [16, 2.5, 20]},
    {"name": "tail_tip", "role": "tail", "parent": "tail", "joint": [16, 2, 25]},
    {"name": "leg_fl", "role": "leg", "parent": "body", "joint": [13, 1.5, 11], "side": "l", "pair": 0},
    {"name": "leg_fr", "role": "leg", "parent": "body", "joint": [19, 1.5, 11], "side": "r", "pair": 0},
    {"name": "leg_hl", "role": "leg", "parent": "body", "joint": [13, 1.5, 19.5], "side": "l", "pair": 1},
    {"name": "leg_hr", "role": "leg", "parent": "body", "joint": [19, 1.5, 19.5], "side": "r", "pair": 1}]}

save("creatures", {"name": "crocodile", "notes": "haulout: American crocodile, grey-olive and narrow-snouted, in the brackish water of the river mouth: hauled out on the mud banks in the sun, it watches you come and slides into the water before you are close. The base of the delta's crocodilians.",
                   "voxel": 0.125, "half_w": 0.45, "height": 0.55,
                   "colors": {"hide": [96, 98, 72], "belly": [176, 168, 128], "ridge": [70, 72, 54], "eye": [36, 28, 24]},
                   "behaviour": {"movement": "haulout", "walk_speed": 0.17, "flee_speed": 1.15, "wary": 12, "flees_from": ["player"]},
                   "vary": {"size": 0.15, "shade": 0.08}, "ops": croc_ops(), "rig": CROC_RIG})
save("creatures", {"name": "swimming_crocodile", "like": "crocodile", "notes": "A crocodile in the water: lying awash with only its back, eyes and snout out, drifting, and sinking away when you come close.",
                   "behaviour": {"movement": "school", "walk_speed": 0.35, "flee_speed": 1.3, "wary": 9, "leap": 0, "awash": True, "shallows": 0.6,
                                 "group": {"size": [1, 1], "spacing": 4, "range": 10}}})
save("creatures", {"name": "alligator", "like": "crocodile", "notes": "haulout: American alligator: broad-snouted and near black, the fresh-water crocodilian of the swamp and the sawgrass, basking on the banks.",
                   "colors": {"hide": [52, 56, 44], "belly": [196, 188, 156], "ridge": [36, 40, 32], "eye": [36, 28, 24]}})
save("creatures", {"name": "swimming_alligator", "like": "alligator", "notes": "An alligator in the water: awash among the cypress knees, eyes and nostrils out, sinking away when you come close.",
                   "behaviour": {"movement": "school", "walk_speed": 0.35, "flee_speed": 1.3, "wary": 9, "leap": 0, "awash": True, "shallows": 0.6,
                                 "group": {"size": [1, 1], "spacing": 4, "range": 10}}})
save("creatures", {"name": "old_pale", "like": "swimming_alligator", "rarity": 0.03, "scale": 1.6,
                   "notes": "Old Pale: a white alligator half as long again as any other, seen now and then in the swamp and never twice in the same place.",
                   "colors": {"hide": [226, 222, 206], "belly": [240, 236, 226], "ridge": [204, 198, 182], "eye": [190, 60, 50, 0.4]}})
save("creatures", {"name": "great_egret", "like": "heron", "notes": "wade: Great egret: tall and all white with a yellow bill, stalking the shallows.",
                   "colors": {"heron": [240, 236, 228], "white": [240, 236, 228], "beak": [240, 190, 40], "dark": [36, 28, 24]}})
import json as _json
_heron = _json.load(open(os.path.join(ROOT, "library", "creatures", "heron.json"), encoding="utf-8"))
_spoon_ops = [o for o in _heron["ops"] if not (o.get("mat") == "beak")]
# A spoonbill's bill: a straight shaft ending in a flat, wide spoon.
_spoon_ops += [{"op": "box", "from": [16, 17, 7], "to": [17, 18, 9], "mat": "beak", "part": "head"},
               {"op": "box", "from": [15, 17, 4], "to": [18, 18, 7], "mat": "beak", "part": "head"},
               {"op": "box", "from": [16, 17, 3], "to": [17, 18, 4], "mat": "beak", "part": "head"}]
save("creatures", {"name": "roseate_spoonbill", "scale": 0.8,
                   "notes": "wade: Roseate spoonbill: a pink wader sweeping its long spoon-ended bill through the shallows of the mouth and the flats.",
                   "voxel": _heron["voxel"], "half_w": _heron["half_w"], "height": _heron["height"],
                   "colors": {"heron": [226, 132, 150], "white": [244, 196, 204], "beak": [196, 180, 140], "dark": [36, 28, 24]},
                   "behaviour": _heron["behaviour"], "ops": _spoon_ops, "rig": _heron["rig"]})
save("creatures", {"name": "brown_pelican", "like": "gull", "scale": 2.2, "notes": "circle: Brown pelican: big and grey-brown, flying low over the bay and the sound and floating on it.",
                   "colors": {"white": [210, 196, 160], "beak": [196, 160, 96], "dark": [36, 28, 24], "grey": [112, 104, 96]}})
_cormorant = _json.load(open(os.path.join(ROOT, "library", "creatures", "cormorant.json"), encoding="utf-8"))
# The anhinga: a slim black body, a long fan tail and a very long thin neck
# kinked like a snake's, with a small head and a dagger bill.
_anhinga_ops = [
    {"op": "ellipsoid", "centre": [16, 8, 20], "radius": [1.9, 1.9, 5.5], "mat": "body"},
    {"op": "box", "from": [16, 9, 11], "to": [17, 10, 16], "mat": "body"},
    {"op": "box", "from": [16, 10, 8], "to": [17, 12, 11], "mat": "body", "part": "head"},
    {"op": "box", "from": [16, 12, 5], "to": [17, 13, 8], "mat": "body", "part": "head"},
    {"op": "ellipsoid", "centre": [16.5, 12.6, 4.2], "radius": [1.0, 0.9, 1.4], "mat": "head", "part": "head"},
    {"op": "box", "from": [16, 12, 0], "to": [17, 13, 3], "mat": "beak", "part": "head"},
    {"op": "box", "from": [2, 9, 17], "to": [8, 10, 20], "mat": "tail", "part": "wing_l_tip"},
    {"op": "box", "from": [8, 9, 17], "to": [11, 10, 20], "mat": "wing", "part": "wing_l"},
    {"op": "box", "from": [11, 9, 16], "to": [15, 10, 21], "mat": "wing", "part": "wing_l"},
    {"op": "box", "from": [18, 9, 16], "to": [22, 10, 21], "mat": "wing", "part": "wing_r"},
    {"op": "box", "from": [22, 9, 17], "to": [25, 10, 20], "mat": "wing", "part": "wing_r"},
    {"op": "box", "from": [25, 9, 17], "to": [31, 10, 20], "mat": "tail", "part": "wing_r_tip"},
    {"op": "box", "from": [14, 8, 25], "to": [19, 9, 31], "mat": "tail", "part": "tail"},
    {"op": "box", "from": [15, 4, 20], "to": [16, 6, 21], "mat": "legs", "part": "leg_l"},
    {"op": "box", "from": [17, 4, 20], "to": [18, 6, 21], "mat": "legs", "part": "leg_r"},
]
_anhinga_rig = {"plan": "bird", "bones": [
    {"name": "body", "role": "root"},
    {"name": "head", "role": "head", "parent": "body", "joint": [16.5, 10, 11]},
    {"name": "wing_l", "role": "wing", "parent": "body", "joint": [15, 9.5, 18.5], "side": "l"},
    {"name": "wing_l_tip", "role": "wing", "parent": "wing_l", "joint": [8, 9.5, 18.5], "side": "l"},
    {"name": "wing_r", "role": "wing", "parent": "body", "joint": [18, 9.5, 18.5], "side": "r"},
    {"name": "wing_r_tip", "role": "wing", "parent": "wing_r", "joint": [25, 9.5, 18.5], "side": "r"},
    {"name": "tail", "role": "tail", "parent": "body", "joint": [16.5, 8.5, 25]},
    {"name": "leg_l", "role": "leg", "parent": "body", "joint": [15.5, 6, 20.5], "side": "l", "pair": 0},
    {"name": "leg_r", "role": "leg", "parent": "body", "joint": [17.5, 6, 20.5], "side": "r", "pair": 0}]}
save("creatures", {"name": "anhinga", "scale": 2.4, "voxel": 0.021, "half_w": 0.12, "height": 0.15,
                   "notes": "circle: Anhinga, the snakebird: near-black with silver-streaked wings and a very long thin neck, swimming with only the neck out and drying its wings on the snags.",
                   "colors": {"body": [12, 12, 15], "head": [26, 20, 16], "beak": [150, 122, 72], "wing": [84, 88, 92], "tail": [12, 12, 15], "legs": [18, 18, 20]},
                   "behaviour": _cormorant["behaviour"], "ops": _anhinga_ops, "rig": _anhinga_rig})
save("creatures", {"name": "channel_catfish", "like": "trout", "scale": 1.3, "notes": "Channel catfish: grey-olive, whiskered, in loose groups along the bayous and the channels.",
                   "colors": {"herring": [104, 104, 92], "silver": [196, 190, 176], "fin": [84, 84, 74]},
                   "behaviour": {"walk_speed": 0.8, "flee_speed": 3.0, "leap": 0.2, "shallows": 0.8, "group": {"size": [2, 5], "spacing": 1.2, "range": 5}}})
save("creatures", {"name": "alligator_gar", "like": "trout", "scale": 2.8, "notes": "Alligator gar: a long armoured fish with a toothy snout, cruising the channels and rolling at the surface.",
                   "colors": {"herring": [96, 96, 66], "silver": [176, 170, 130], "fin": [80, 74, 56]},
                   "behaviour": {"walk_speed": 0.7, "flee_speed": 2.6, "leap": 0.4, "shallows": 0.9, "group": {"size": [1, 2], "spacing": 2.0, "range": 6}}})
save("creatures", {"name": "nutria", "like": "capybara", "scale": 0.5, "notes": "wade: Nutria: a big brown water rat with orange teeth, grazing the banks and swimming off when you come.",
                   "colors": {"coat": [110, 82, 56], "belly": [128, 96, 60], "mane": [110, 82, 56], "nose": [36, 28, 24], "tusk": [214, 140, 40], "dark": [36, 28, 24]}})

# ---- biomes -------------------------------------------------------------------

def biome(name, notes, priority, rules, surface, sub, color, plants=(), features=(), creatures=(), collectibles=(), **kw):
    d = {"name": name, "notes": notes, "priority": priority, "rules": rules, "surface": surface, "subsurface": sub,
         "map_color": color}
    if plants:
        d["plants"] = [dict(plant=p, density=dn, placement=pl) for (p, dn, pl) in plants]
    if features:
        d["features"] = [dict(structure=s, density=dn, placement=pl) for (s, dn, pl) in features]
    if creatures:
        d["creatures"] = [dict(creature=c, count=n) for (c, n) in creatures]
    if collectibles:
        d["collectibles"] = [dict(item=c, density=dn, placement=pl) for (c, dn, pl) in collectibles]
    d.update(kw)
    save("biomes", d)


def S(**p):
    return {"verb": "scatter", "params": p}


def C(size, lo, hi):
    return {"verb": "clustered", "params": {"size": size, "min": lo, "max": hi}}


def CT(lo):
    """Cattails: dense stands a few metres across (20 to 100 plants), where a
    small-scale noise tops `lo`; the higher `lo`, the fewer stands."""
    return C(22, lo, lo + 0.015)


def E(width, inner):
    return {"verb": "edge", "params": {"width": width, "inner": inner}}


def R(width):
    return {"verb": "riparian", "params": {"width": width}}


def L(spacing):
    return {"verb": "lone", "params": {"spacing": spacing}}


TEA = {"shallow": [92, 84, 52], "deep": [34, 32, 22]}

biome("delta_bed", "The sound round the delta and the floor of every open water: soft grey mud under a metre or so of brown water.",
      0, {"underwater": True}, "delta_mud", "silt", [86, 120, 132],
      creatures=[("brown_pelican", 8), ("gull", 10), ("cormorant", 4), ("herring", 30), ("swimming_turtle", 4)])
biome("bottomland", "The delta's ordinary ground: low wet woods of tupelo and oak along the old levees, palmettos under them, moss hanging everywhere.",
      4, {"elev": [-0.5, 20]}, "marsh_grass", "silt", [104, 140, 80],
      plants=[("live_oak", 0.0006, L(34)), ("water_tupelo", 0.004, C(60, 0.45, 0.7)), ("bald_cypress", 0.002, C(70, 0.55, 0.8)), ("willow", 0.002, R(16)),
              ("cabbage_palm", 0.0015, C(50, 0.5, 0.75)), ("saw_palmetto", 0.02, C(30, 0.45, 0.7)), ("spanish_moss", 0.6, S()),
              ("cattail", 0.7, CT(0.7))],
      creatures=[("deer", 6), ("boar", 4), ("coyote", 2), ("bear", 1), ("crow", 6), ("great_horned_owl", 1), ("pileated_woodpecker", 2),
                 ("firefly", 50), ("dragonfly", 8), ("nutria", 4)],
      collectibles=[("pottery_shard", 0.00006, S())])
biome("levee", "The levee and the high ground along the Marrow: mown grass, a few great live oaks, cottonwoods and willows down the bank.",
      7, {"region": "levee"}, "meadow_grass", "silt", [140, 176, 96],
      plants=[("live_oak", 0.0003, L(60)), ("cottonwood", 0.004, R(18)), ("willow", 0.004, R(10)), ("spanish_moss", 0.9, S())],
      creatures=[("deer", 3), ("coyote", 2), ("crow", 6), ("starling", 30), ("sandhill_crane", 3), ("great_horned_owl", 1)])
biome("cane_field", "Sugar cane in blocks taller than a man, dirt rows between.",
      8, {"region": "cane"}, "cane_soil", "silt", [170, 190, 90],
      plants=[("sugar_cane", 0.12, C(10, 0.2, 0.35))],
      creatures=[("boar", 3), ("crow", 8), ("jackrabbit", 3)])
biome("cane_town", "Mire, the cane town: mown yards between the cottages, a few live oaks hung with moss, dirt lanes.",
      9, {"region": "mire"}, "meadow_grass", "silt", [200, 180, 140],
      plants=[("live_oak", 0.0008, L(30)), ("spanish_moss", 0.8, S()), ("cabbage_palm", 0.0006, S())],
      creatures=[("crow", 4), ("starling", 20)],
      collectibles=[("settler_coin", 0.0002, S())])
biome("mound_grounds", "The mound grounds: short grass kept down by deer on the great bird mound and the low half rings east of it, a few old oaks.",
      10, {"region": "mounds"}, "meadow_grass", "silt", [196, 200, 140],
      plants=[("live_oak", 0.0004, L(50)), ("spanish_moss", 0.6, S())],
      creatures=[("deer", 5), ("sandhill_crane", 3), ("great_horned_owl", 1), ("coyote", 2)],
      collectibles=[("arrowhead", 0.0005, S()), ("pottery_shard", 0.0004, S())])
biome("burned_cane", "A cane field burned off: black stubble and ash, a few charred stalks standing, and the woven stick lattices along the path through it.",
      9, {"region": "burnt"}, "cane_char", "silt", [70, 66, 60],
      plants=[("sugar_cane", 0.004, S()), ("snag", 0.0003, S())],
      creatures=[("crow", 10)])
biome("cypress_swamp", "The back swamp: bald cypress and tupelo standing in black tea-coloured water, knees round their feet, moss in long beards, hyacinth on the still water.",
      10, {"region": "swamp", "amphibious": True}, "swamp_muck", "peat", [70, 110, 80],
      plants=[("bald_cypress", 0.012, C(40, 0.3, 0.6)), ("water_tupelo", 0.004, S()), ("cypress_snag", 0.0006, S()),
              ("spanish_moss", 0.85, S()), ("cattail", 0.7, CT(0.64)), ("fern", 0.02, S()),
              ("saw_palmetto", 0.008, S())],
      features=[("cypress_knees", 0.0025, S()), ("hyacinth_raft", 0.0003, S()), ("mossy_log", 0.0004, S()),
                ("lily_pads", 0.0012, C(60, 0.5, 0.75)), ("lily_pads_flowering", 0.0008, C(60, 0.55, 0.8))],
      creatures=[("heron", 3), ("great_egret", 4), ("anhinga", 4), ("kingfisher", 3), ("mallard", 10), ("pileated_woodpecker", 2),
                 ("barn_owl", 1), ("firefly", 70), ("dragonfly", 12), ("alligator", 6), ("swimming_alligator", 7), ("nutria", 4),
                 ("channel_catfish", 10)],
      water=TEA, mist=0.15)
biome("ghost_forest", "The ghost forest on the bay's north shore: grey dead cypress standing in the water where the salt came in, cormorants and anhingas on every snag.",
      11, {"region": "ghost", "amphibious": True}, "delta_mud", "peat", [150, 150, 140],
      plants=[("cypress_snag", 0.006, C(30, 0.25, 0.6)), ("cattail", 0.7, CT(0.66))],
      creatures=[("anhinga", 6), ("cormorant", 6), ("heron", 2), ("crow", 4), ("swimming_alligator", 2)], mist=0.35)
biome("drowned_parish", "The bay over the old parish: open brown water a metre or two deep, a church tower and the tops of tombs and roofs standing out of it.",
      11, {"region": "parish", "amphibious": True}, "delta_mud", "silt", [110, 130, 140],
      features=[("hyacinth_raft", 0.0002, S())],
      creatures=[("brown_pelican", 4), ("anhinga", 3), ("swimming_alligator", 2), ("alligator_gar", 4)],
      collectibles=[("settler_coin", 0.00004, S())])
biome("lowmouth_village", "Lowmouth, the stilt village on the old channel: houses and camps on posts, boardwalks and footbridges over the water, boats tied under them.",
      12, {"region": "lowmouth", "amphibious": True}, "swamp_muck", "silt", [190, 150, 110],
      plants=[("bald_cypress", 0.002, S()), ("spanish_moss", 0.6, S()), ("cattail", 0.7, CT(0.7))],
      creatures=[("mallard", 8), ("great_egret", 2), ("crow", 4), ("channel_catfish", 6)],
      collectibles=[("settler_coin", 0.0002, S())])
biome("sawgrass", "The sawgrass flats: a sheet of grass standing in a hand or two of clear water, running to the sky, with hammocks of palm and oak like islands.",
      9, {"region": "sawgrass", "elev": [-5, 0.25], "amphibious": True}, "marsh_grass", "peat", [176, 168, 96],
      plants=[("sawgrass", 0.07, C(10, 0.35, 0.6)), ("cattail", 0.7, CT(0.68))],
      creatures=[("great_egret", 6), ("roseate_spoonbill", 6), ("sandhill_crane", 4), ("heron", 2), ("alligator", 4),
                 ("swimming_alligator", 4), ("dragonfly", 12), ("meadowhawk", 8)])
biome("hammock", "A hammock: a tree island in the sawgrass, a little higher and dry, thick with cabbage palm, oak, strangler fig and palmetto.",
      10, {"region": "sawgrass", "elev": [0.25, 20]}, "marsh_grass", "dirt", [80, 120, 60],
      plants=[("cabbage_palm", 0.008, S()), ("live_oak", 0.0015, S()), ("small_fig", 0.001, S()), ("saw_palmetto", 0.04, S()),
              ("spanish_moss", 0.6, S())],
      features=[("bottle_tree", 0.00008, L(120))],
      creatures=[("deer", 4), ("boar", 2), ("firefly", 40)],
      collectibles=[("purple_bottle", 0.0002, S())])
biome("mangrove_keys", "The new mouth: red mangroves on their prop roots in the shallows, black mangroves on the mud keys, channels winding between, the river's silt settling.",
      10, {"region": "mouth", "amphibious": True}, "delta_mud", "silt", [60, 120, 90],
      plants=[("red_mangrove", 0.02, C(24, 0.3, 0.6)), ("black_mangrove", 0.01, S()), ("cattail", 0.7, CT(0.72))],
      creatures=[("brown_pelican", 6), ("roseate_spoonbill", 6), ("great_egret", 4), ("cormorant", 4), ("crocodile", 5),
                 ("swimming_crocodile", 5), ("swimming_turtle", 3), ("alligator_gar", 3), ("heron", 4),
                 ("herring", 24)],
      collectibles=[("seashell", 0.0003, S())])
biome("chenier", "The chenier: an old shell ridge on the south point, live oaks bent by the Gulf wind, palms, oyster shell underfoot, the tombs of the parish and the light at its tip.",
      9, {"region": "chenier"}, "marsh_grass", "oyster_shell", [140, 160, 110],
      plants=[("live_oak", 0.003, C(40, 0.4, 0.7)), ("cabbage_palm", 0.003, S()), ("saw_palmetto", 0.02, S()), ("spanish_moss", 0.6, S())],
      creatures=[("deer", 3), ("boar", 2), ("coyote", 3), ("bear", 1), ("crow", 4), ("brown_pelican", 4)],
      collectibles=[("arrowhead", 0.0001, S()), ("seashell", 0.0002, S())])
biome("gulf_beach", "The Gulf shore of the chenier: oyster shell and pale sand, wrack along the tide line.",
      11, {"region": "chenier", "elev": [-0.4, 0.6]}, "oyster_shell", "sand", [214, 206, 180],
      creatures=[("gull", 8), ("brown_pelican", 4), ("sea_turtle", 1)],
      collectibles=[("seashell", 0.0008, S())])
biome("mudflat", "Mud where the river's silt comes out at the mouth's edge: a few wet bars, drift logs and dead snags, wading birds walking it, mist lying on it at dawn and dusk.",
      11, {"region": "mouth", "elev": [0.0, 0.3]}, "delta_mud", "silt", [140, 124, 96],
      plants=[("cattail", 0.7, CT(0.68)), ("cypress_snag", 0.0006, S())],
      features=[("fallen_log", 0.0006, S()), ("lily_pads", 0.0004, S())],
      creatures=[("heron", 2), ("sandhill_crane", 2), ("great_egret", 2), ("roseate_spoonbill", 4), ("crocodile", 2)],
      collectibles=[("seashell", 0.0004, S())], mist=0.4)

print("\n".join(written))
print(len(written), "entries")
