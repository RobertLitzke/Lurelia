"""Writes chronicle.json for Jadewater. The chronicle is data; this script only
saves typing for the repeated pieces (the Old Wall's blocks, the lagoon
towers, the gorge regions, bridges and passages). See the note below.

One-shot, not kept current. This ran once, when the world was made; the
chronicle has moved on since (hand edits, new events and fields), so rerunning
it would overwrite live data with a stale version. It is kept as a record of
how the world was first written. Edit chronicle.json directly instead, and if
you do rerun this, diff the output before keeping any of it.
"""
import json
import math
import os
import random

R = random.Random(400)
# Draws added in the second round (lakes, moss, the extra bridges) come from
# their own generator, so everything R placed stays where it was.
R2 = random.Random(401)
HERE = os.path.dirname(os.path.abspath(__file__))

places = {
    "centre": [430, 420],
    "lantern_well": [250, 215],
    "temple": [300, 560],
    "avenue_foot": [300, 690],
    "choir": [835, 790],
    "river_head": [345, 300],
    "river_mouth": [700, 690],
}

E = []


def ev(year, kind, cause, **kw):
    E.append({"year": year, "event": kind, "cause": cause, **kw})


def near_line(p, line, d):
    for a, b in zip(line, line[1:]):
        ax, az = a
        bx, bz = b
        vx, vz = bx - ax, bz - az
        t = max(0.0, min(1.0, ((p[0] - ax) * vx + (p[1] - az) * vz) / (vx * vx + vz * vz)))
        cx, cz = ax + vx * t, az + vz * t
        if math.hypot(p[0] - cx, p[1] - cz) < d:
            return True
    return False


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


river = [[345, 300], [372, 352], [420, 405], [470, 450], [520, 500], [575, 560], [630, 620], [700, 690], [760, 730]]

# ---- the rock ----------------------------------------------------------------
ev(-30000000, "landmass", "a limestone plateau lifted a little above a warm shallow sea; the sea floor round it is a shelf of lime sand",
   centre=[410, 400], radius=440, roughness=0.55, hills=4, ridges=2, shelf=3)

# The Old Wall: the part of the plateau not yet cut into free towers. Built
# as blocks packed close, so the gaps between them are the gorges.
ev(-8000000, "mesa", "the block round the Lantern Well, the biggest piece of the Old Wall", at="lantern_well",
   radius=62, top=80, warp=0.22, talus=0.08, skirt=4, poi="the Lantern Well block")
ev(-8000000, "erode_rims", "rain rounding the edges of the Well block's top", depth=4, reach=14, gullies=0.6, min_drop=6)

def looks():
    """A tower's rock: most the world's own, some dark, some pale, the odd blue-grey,
    and now and then one wrapped entirely in vines."""
    x = R.random()
    d = {}
    if x < 0.22:
        d["rock"] = "karst_dark"
    elif x < 0.44:
        d["rock"] = "karst_light"
    elif x < 0.50:
        d["rock"] = "karst_blue"
    if R.random() < 0.08:
        d["mantle"] = "vine_mantle"
        d["mantle_share"] = 0.5 if "rock" not in d or d["rock"] != "karst_blue" else 0.35
    return d


FOOT = dict(apron="lime_sand", rubble=["karst_rubble", "karst_rubble", "karst_scree", "karst_scree", "karst_block"])

class Ledge(dict):
    """The ledge plants and skin every tower takes. A fresh draw (from R2) each
    time it is spread into an event: moss over a fifth to half of the walls,
    one tower in five hangs wisteria from its ledges, and some grow a cherry."""

    def keys(self):
        self.clear()
        cherry = R2.random() < 0.25
        self.update(ledge_plants=["cliff_banyan", "cliff_banyan", "small_fig", "small_fig", "karst_pine", "hazel"] + (["flowering_cherry"] if cherry else []),
                    ledge_vine="wisteria" if R2.random() < 0.2 else "karst_vine",
                    moss="karst_moss", moss_share=round(R2.uniform(0.4, 0.75), 2), soil="karst_soil")
        return super().keys()


LEDGE = Ledge()

# The Well block's top is not flat: knobs and domes of the rock still
# standing on it, round the rim of the Well.
for k in range(7):
    a = k * 2 * math.pi / 7 + R.uniform(-0.3, 0.3)
    d = R.uniform(38, 52)
    ev(-7900000, "tower", "a knob of rock still standing on the Well block", at=[round(250 + d * math.cos(a)), round(215 + d * math.sin(a))],
       radius=round(R.uniform(9, 16), 1), height=round(R.uniform(10, 32), 1), bulge=0.6, ledges=12, **LEDGE)

blocks = []
# Bridged pairs on the Wall (the Red Way's high crossings): set first, low warp.
wall_pairs = [((150, 330), (222, 330)), ((390, 160), (390, 232)), ((120, 150), (120, 222))]
for a, b in wall_pairs:
    for c in (a, b):
        blocks.append((c, 26))
        ev(-8000000, "tower", "a block of the Old Wall the Order chose for a crossing of the Red Way, its top cut level", at=list(c),
           radius=26, top=72, bulge=0.3, crown="flat", lean=2, taper=0.12, ledges=10, **LEDGE)

mountains = 0
wall_towers = []
wall_top = {}
for gz in range(9):
    for gx in range(10):
        cx = 70 + gx * 50 + R.uniform(-14, 14) + (gz % 2) * 25
        cz = 70 + gz * 48 + R.uniform(-14, 14)
        c = (round(cx), round(cz))
        # The Wall's ragged south-east edge, the river's gorge and the well block.
        if cx + cz > 600 + R.uniform(-50, 50) or near_line(c, river, 30) or dist(c, places["lantern_well"]) < 62 + 14:
            continue
        r = R.uniform(17, 28)
        # Blocks may run together into longer walls, but not swallow each other.
        if any(dist(c, q) < max(r, rq) + 4 for q, rq in blocks):
            continue
        blocks.append((c, r))
        wall_towers.append((c, r))
        high = mountains < 6 and R.random() < 0.3
        top = R.uniform(98, 122) if high else R.choice([72, 72, 72, 64, 56])
        mountains += high
        wall_top[c] = top
        ev(-8000000, "tower", "a summit tower of the Old Wall, higher than the rest" if high else "a tower of the Old Wall between the gorges",
           at=list(c), radius=round(r, 1), top=round(top + R.uniform(-8, 14), 1), bulge=round(R.uniform(0.4, 0.9), 2),
           stretch=[round(R.uniform(0, 180)), round(R.uniform(1.0, 1.6), 2)], ledges=10, **LEDGE, **looks(),
           **({"poi": f"summit tower {mountains}"} if high else {}))

# Gorge regions: points in the gaps between blocks.
gorge_pts = []
for z in range(40, 470, 16):
    for x in range(30, 520, 16):
        p = (x, z)
        if x + z > 700:
            continue
        ds = sorted(dist(p, q) - rq for q, rq in blocks + [(tuple(places["lantern_well"]), 62)])
        if ds[0] > 2 and ds[1] < 26:
            gorge_pts.append(p)
for p in gorge_pts:
    ev(-7000000, "region", "a gorge between blocks of the Old Wall", name="gorges", at=list(p), radius=14)

# The lagoon: towers standing free in the shallows, and a few in the jungle.
redway = [(560, 760), (600, 760), (640, 760), (640, 800), (680, 800), (720, 800)]
choir = [(places["choir"][0] + 72 * math.sin(k * 2 * math.pi / 7), places["choir"][1] - 72 * math.cos(k * 2 * math.pi / 7)) for k in range(7)]
towers = []
tries = 0
while len(towers) < 95 and tries < 20000:
    tries += 1
    c = (R.uniform(440, 1000), R.uniform(380, 1000))
    if c[0] + c[1] < 900 or near_line(c, river, 26):
        continue
    if any(dist(c, q) < 34 for q in redway) or dist(c, places["choir"]) < 150 or dist(c, places["temple"]) < 120:
        continue
    r = R.choice([7, 8, 10, 12, 14, 16, 19, 22, 26])
    if any(dist(c, q) < r + rq + 10 for q, rq in towers):
        continue
    towers.append((c, r))
lagoon = []
lakes = []
for c, r in towers:
    top = R.uniform(40, 115) if r > 11 else R.uniform(26, 75)
    lagoon.append((c, r, top))
    lake = r >= 14 and R2.random() < 0.4
    if lake:
        lakes.append((c, r))
    ev(-6000000, "tower", "a free tower in the lagoon, the plateau cut down to its last pillars" + (", a small lake on its top" if lake else ""),
       at=[round(c[0]), round(c[1])],
       radius=r, top=round(top, 1), bulge=round(R.uniform(0.35, 1.0), 2), flute=round(R.uniform(0.06, 0.16), 2),
       stretch=[round(R.uniform(0, 180)), round(R.uniform(1.0, 1.5), 2)], ledges=10, **LEDGE, **looks(), **FOOT, rubble_count=R.randint(6, 16),
       **({"crown": "flat", "taper": 0.18} if lake else {}))
for c in redway:
    ev(-6000000, "tower", "a lagoon tower whose top the Order cut down level with its neighbours for the Red Way", at=list(c),
       radius=10, top=46, bulge=0.3, crown="flat", lean=1, taper=0.1, ledges=10, **LEDGE, **FOOT, rubble_count=4)
for k, c in enumerate(choir):
    ev(-5000000, "tower", "one of the seven towers of the Choir: the same height and the same distance apart, which rain does not do",
       at=[round(c[0], 1), round(c[1], 1)], radius=10, top=88, bulge=0.25, crown="dome", lean=0, flute=0.05, **FOOT, rubble_count=5,
       **({"poi": "the Choir"} if k == 0 else {}))

# Rain caught in hollows on the flat tops: small turquoise lakes over white lime sand.
for k, (c, r) in enumerate(lakes):
    ev(-5900000, "lake", "rain caught in a hollow on a tower's top, turquoise over white lime", name=f"tower_lake_{k}",
       at=[round(c[0]), round(c[1])], radius=round(r * R2.uniform(0.2, 0.3), 1), depth=round(R2.uniform(0.9, 1.6), 1),
       bed="lime_sand", bank="karst_soil", ragged=0.35)

# Three of the lakes have cut a notch in their tower's rim and spill over it
# into the lagoon, on the side facing the jungle shore. The river starts in
# the lake and the drop makes the waterfall.
for k in (2, 4, 12):
    (cx, cz), r = lakes[k]
    a = math.atan2(705 - cz, 300 - cx)
    out = [[round(cx + math.cos(a) * d, 1), round(cz + math.sin(a) * d, 1)] for d in (0, r * 0.7, r + 12)]
    ev(-5800000, "river", "a tower lake spilling over a notch in the rim into the lagoon", name=f"tower_spill_{k}",
       path=out, width=3.5, depth=0.8, bed="lime_sand", meander=0)

# ---- water inside the rock -----------------------------------------------------
ev(-3000000, "canyon", "the Lantern Well: the roof of the Wall's biggest cave fell in, and the sea came up into it",
   path=[[244, 210], [258, 222]], width=36, depth=300, wall=0.06, bench=14, meander=0, floor="lime_sand", poi="the Lantern Well")
cenotes = [((470, 650), 24), ((560, 420), 20), ((395, 470), 16), ((610, 300), 18)]
for (x, z), w in cenotes:
    ev(-2500000, "mesa", "a low hump of limestone in the jungle, hollow underneath", at=[x, z], radius=w * 0.5 + 14, height=9,
       warp=0.3, talus=0.55, skirt=12)
    ev(-2000000, "canyon", "a cenote in the jungle where a cave roof fell in", path=[[x - 5, z - 3], [x + 5, z + 3]],
       width=w, depth=300, wall=0.05, bench=20, meander=0, floor="lime_sand", poi="a cenote")
for (x, z), w in cenotes + [(tuple(places["lantern_well"]), 40)]:
    ev(-2000000, "region", "the cenote's rim and walls, where the film grows", name="cenotes", at=[x, z], radius=w * 0.5 + 12)

# Undercut grottoes round the cenotes at the waterline: the overhangs.
def ring(c, r, n=10):
    return [[round(c[0] + r * math.cos(a * 2 * math.pi / n), 1), round(c[1] + r * math.sin(a * 2 * math.pi / n), 1)] for a in range(n + 1)]


ev(-1500000, "passage", "the overhang round the Lantern Well, undercut at the waterline and lit by the film",
   path=ring((248, 217), 27, 16), height=2.2, radius=3.2, lining="lantern_film", poi="the lit overhang")
for (x, z), w in cenotes:
    ev(-1500000, "passage", "a cenote's overhang, undercut at the waterline and lit by the film", path=ring((x, z), w * 0.5 + 4, 12),
       height=1.8, radius=2.4, lining="lantern_film")

# The blue hole in the Choir: a cold shaft into a chamber.
cx, cz = places["choir"]
ev(-1500000, "passage", "the blue hole in the middle of the Choir: a shaft in the sea floor into a cold chamber",
   path=[[cx - 4, cz], [cx + 4, cz]], height=-26, radius=11, shafts=1, poi="the blue hole")

# Caves through the towers and the Wall, where the water table stood.
ev(-1200000, "passage", "the water passage from the Lantern Well to the gorges, its ceiling thick with the film",
   path=[[250, 236], [256, 280], [262, 320]], height=3.6, radius=2.6, lining="lantern_film", shafts=1, poi="the Well passage")
for (c, r) in [(c, r) for c, r in blocks if r > 24][:6]:
    lined = dist(c, places["lantern_well"]) < 230
    ev(-1200000, "passage", "an old water passage straight through a block of the Wall" + (", lit by the film" if lined else ""),
       path=[[c[0] - r - 8, c[1]], [c[0] + r + 8, c[1]]], height=R.choice([4, 18, 30]), radius=2.2,
       lining="lantern_film" if lined else None, shafts=1 if r > 28 else 0)
for (c, r) in [(c, r) for c, r in towers if r >= 12][:10]:
    ns = R.random() < 0.5
    a, b = ([c[0], c[1] - r - 6], [c[0], c[1] + r + 6]) if ns else ([c[0] - r - 6, c[1]], [c[0] + r + 6, c[1]])
    ev(-1200000, "passage", "a water passage through a lagoon tower, dark this far from the Well",
       path=[[round(a[0]), round(a[1])], [round(b[0]), round(b[1])]], height=R.choice([3, 6, 12]), radius=2.0)
for c in choir[::2]:
    ev(-1200000, "passage", "a passage through a tower of the Choir, lit like the Well's",
       path=[[round(c[0] - 16), round(c[1])], [round(c[0] + 16), round(c[1])]], height=8, radius=2.4, lining="lantern_film")

ev(-1000000, "moisture", "the wet ground east of the Wall where the bamboo forest grows", at=[600, 230], radius=190, amount=0.35)
ev(-900000, "river", "the small river that runs out of the Wall's main gorge to the lagoon",
   name="jade_river", path=river, width=5, depth=1.0, bed="lime_sand", water="river_water", meander=14)

# ---- caverns ---------------------------------------------------------------------
# Sea caves: the waterline cut straight through the bigger lagoon towers.
seacaves = 0
for c, r, top in lagoon:
    if r < 12 or R.random() > 0.5:
        continue
    a = R.uniform(0, math.pi)
    d = (math.cos(a), math.sin(a))
    L = r * 1.8 + 8
    ev(-1100000, "passage", "a sea cave the waves cut right through a tower", path=[[round(c[0] - d[0] * L), round(c[1] - d[1] * L)],
       [round(c[0] + d[0] * L), round(c[1] + d[1] * L)]], height=1.6, radius=round(R.uniform(3.2, 5.0), 1),
       **({"lining": "lantern_film"} if R.random() < 0.2 else {}), **({"poi": "a sea cave"} if seacaves == 0 else {}))
    seacaves += 1

# The Old Wall's caverns: big chambers inside the towers, chimneys up to the
# tops, and passages joining them through the rock (and across the gorges).
halls = sorted([t for t in wall_towers if t[1] > 21], key=lambda t: t[0][0] + t[0][1] * 0.3)[:9]
chambers = []
chamber_r = {}
for k, (c, r) in enumerate(halls):
    h = round(R.uniform(10, 26), 1)
    lit = dist(c, places["lantern_well"]) < 240
    r_ch = round(R.uniform(6, 9), 1)
    ev(-1150000, "passage", "a cavern hollowed out inside a tower of the Old Wall, a chimney climbing from its roof to the sky" + (", its roof lit by the film" if lit else ""),
       path=[[c[0] - 3, c[1]], [c[0] + 3, c[1] + 1]], height=h + 3, radius=r_ch, shafts=1,
       **({"lining": "lantern_film"} if lit else {}), **({"poi": "a cavern in the Old Wall"} if k == 0 else {}))
    chambers.append((c, h))
    chamber_r[c] = r_ch
for (a, ha), (b, hb) in zip(chambers, chambers[1:]):
    if dist(a, b) > 170:
        continue
    h = (ha + hb) / 2
    ev(-1140000, "passage", "an old water passage joining two caverns through the rock", path=[list(a), list(b)],
       height=round(h + 1.4, 1), radius=2.3, wander=8, lining="lantern_film" if R.random() < 0.5 else None)

# ---- the Order's rope bridges --------------------------------------------------------
# Strung between lagoon towers at any height, each end landing in a tunnel the
# Order cut through the tower, crossing it to a doorway on the far side.
LENGTHS = (24, 32, 40, 48, 56, 64, 72, 80)
cand = []
for i, (a, ra, ta) in enumerate(lagoon):
    for j, (b, rb, tb) in enumerate(lagoon):
        if j <= i:
            continue
        dx, dz = b[0] - a[0], b[1] - a[1]
        ew = abs(dx) >= abs(dz)
        along, perp = (abs(dx), abs(dz)) if ew else (abs(dz), abs(dx))
        rmin = min(ra, rb)
        if perp > 0.3 * rmin or not (26 <= along <= 90) or min(ta, tb) < 34:
            continue
        fits = [n for n in LENGTHS if along - rmin <= n <= along + 0.7 * rmin]
        if fits:
            cand.append((i, j, ew, fits[0]))
R.shuffle(cand)
uses = {}
bridges = []
for i, j, ew, n in cand:
    if uses.get(i, 0) >= 2 or uses.get(j, 0) >= 2 or len(bridges) >= 32:
        continue
    (a, ra, ta), (b, rb, tb) = lagoon[i], lagoon[j]
    hi = min(ta, tb) - 12
    if hi < 9:
        continue
    h = round(R.uniform(8, hi), 1)
    uses[i] = uses.get(i, 0) + 1
    uses[j] = uses.get(j, 0) + 1
    bridges.append((i, j, ew, n, h))
# The second round: looser pairing, up to five bridges a tower, a second
# bridge between the same pair only at a clearly different height.
cand2 = []
for i, (a, ra, ta) in enumerate(lagoon):
    for j, (b, rb, tb) in enumerate(lagoon):
        if j <= i:
            continue
        dx, dz = b[0] - a[0], b[1] - a[1]
        ew = abs(dx) >= abs(dz)
        along, perp = (abs(dx), abs(dz)) if ew else (abs(dz), abs(dx))
        rmin = min(ra, rb)
        if perp > 0.6 * rmin or not (20 <= along <= 110) or min(ta, tb) < 26:
            continue
        fits = [n for n in LENGTHS if along - 1.2 * rmin <= n <= along + 0.8 * rmin]
        if fits:
            cand2.append((i, j, ew, fits[0]))
R2.shuffle(cand2)
for i, j, ew, n in cand2:
    if uses.get(i, 0) >= 6 or uses.get(j, 0) >= 6 or len(bridges) >= 100:
        continue
    (a, ra, ta), (b, rb, tb) = lagoon[i], lagoon[j]
    hi = min(ta, tb) - 12
    if hi < 7:
        continue
    h = round(R2.uniform(6, hi), 1)
    # Tunnels through the same tower must not run into each other.
    if any((x in (i, j) or y in (i, j)) and abs(hh - h) < 5 for x, y, _, _, hh in bridges):
        continue
    uses[i] = uses.get(i, 0) + 1
    uses[j] = uses.get(j, 0) + 1
    bridges.append((i, j, ew, n, h))
lagoon_bridges = len(bridges)
for k, (i, j, ew, n, h) in enumerate(bridges):
    (a, ra, ta), (b, rb, tb) = lagoon[i], lagoon[j]
    mid = [round((a[0] + b[0]) / 2, 1), round((a[1] + b[1]) / 2, 1)]
    ev(-370, "place", "one of the Order's rope bridges, strung from a tunnel in one tower to a tunnel in the next",
       structure=f"rope_bridge_{n}", at=mid, bearing=0 if ew else 90, height=h, age=0,
       **({"poi": "the high bridges"} if k == 0 else {}))
    # The tunnel the bridge runs through, and a cross tunnel out the far side of each tower.
    if ew:
        line = [[round(a[0]), mid[1]], [round(b[0]), mid[1]]]
    else:
        line = [[mid[0], round(a[1])], [mid[0], round(b[1])]]
    ev(-371, "passage", "the tunnel each end of a bridge runs into", path=line, height=round(h + 1.05, 2), radius=1.7)
    for (c, r) in ((a, ra), (b, rb)):
        L = r * 1.6 + 6
        cross = [[round(c[0]), round(c[1] - L)], [round(c[0]), round(c[1] + L)]] if ew else [[round(c[0] - L), round(c[1])], [round(c[0] + L), round(c[1])]]
        ev(-371, "passage", "a tunnel crossing the tower to a doorway on its far side", path=cross, height=round(h + 1.05, 2), radius=1.7)

# From the caverns inside the Wall: a doorway in the cavern wall, a bridge out
# across the gorge at the cavern's floor, and a tunnel into the next block.
cave_bridges = 0
for c, hch in chambers:
    rc = chamber_r[c]
    deck = round(hch + 3 - 0.55 * rc + 0.3, 1)
    best = None
    for b, rb in wall_towers:
        if b == c:
            continue
        dx, dz = b[0] - c[0], b[1] - c[1]
        ew = abs(dx) >= abs(dz)
        along, perp = (abs(dx), abs(dz)) if ew else (abs(dz), abs(dx))
        if perp > 0.5 * min(rb, 24) or not (28 <= along <= 110) or wall_top[b] < deck + 10:
            continue
        rt = next(r for q, r in wall_towers if q == c)
        gap = along - rt - rb
        fits = [n for n in LENGTHS if gap + 6 <= n <= gap + 30]
        if fits and (best is None or along < best[0]):
            best = (along, b, ew, fits[0])
    if best is None:
        continue
    _, b, ew, n = best
    # The bridge's middle: over the gorge, between the two walls.
    rt = next(r for q, r in wall_towers if q == c)
    rb = next(r for q, r in wall_towers if q == b)
    d = dist(c, b)
    u = (rt + (d - rt - rb) / 2) / d
    mid = [round(c[0] + (b[0] - c[0]) * u, 1), round(c[1] + (b[1] - c[1]) * u, 1)]
    if ew:
        mid[1] = round((c[1] + b[1]) / 2, 1)
    else:
        mid[0] = round((c[0] + b[0]) / 2, 1)
    ev(-372, "place", "a rope bridge the Order strung from a doorway in a cavern of the Old Wall, out across a gorge to the next block",
       structure=f"rope_bridge_{n}", at=mid, bearing=0 if ew else 90, height=deck, age=0,
       **({"poi": "the cavern bridge"} if cave_bridges == 0 else {}))
    line = [[round(c[0]), mid[1]], [round(b[0]), mid[1]]] if ew else [[mid[0], round(c[1])], [mid[0], round(b[1])]]
    ev(-373, "passage", "the doorway out of the cavern and the tunnel into the far block", path=line, height=round(deck + 1.05, 2), radius=1.7)
    cave_bridges += 1

# Across the gorges of the Old Wall, block to block, at any height under the tops.
wall_bridges = 0
wuses = {}
wcand = []
for i, (a, ra) in enumerate(wall_towers):
    for j, (b, rb) in enumerate(wall_towers):
        if j <= i:
            continue
        dx, dz = b[0] - a[0], b[1] - a[1]
        ew = abs(dx) >= abs(dz)
        along, perp = (abs(dx), abs(dz)) if ew else (abs(dz), abs(dx))
        gap = along - ra - rb
        if perp > 0.4 * min(ra, rb) or not (4 <= gap <= 60):
            continue
        fits = [n for n in LENGTHS if gap + 6 <= n <= gap + 30]
        if fits:
            wcand.append((i, j, ew, fits[0]))
R2.shuffle(wcand)
for i, j, ew, n in wcand:
    if wuses.get(i, 0) >= 3 or wuses.get(j, 0) >= 3 or wall_bridges >= 30:
        continue
    (a, ra), (b, rb) = wall_towers[i], wall_towers[j]
    hi = min(wall_top[a], wall_top[b]) - 10
    if hi < 20:
        continue
    h = round(R2.uniform(16, hi), 1)
    wuses[i] = wuses.get(i, 0) + 1
    wuses[j] = wuses.get(j, 0) + 1
    d = dist(a, b)
    u = (ra + (d - ra - rb) / 2) / d
    mid = [round(a[0] + (b[0] - a[0]) * u, 1), round(a[1] + (b[1] - a[1]) * u, 1)]
    if ew:
        mid[1] = round((a[1] + b[1]) / 2, 1)
    else:
        mid[0] = round((a[0] + b[0]) / 2, 1)
    ev(-372, "place", "a rope bridge of the Order across a gorge of the Old Wall, from a tunnel in one block to a tunnel in the next",
       structure=f"rope_bridge_{n}", at=mid, bearing=0 if ew else 90, height=h, age=0)
    line = [[round(a[0]), mid[1]], [round(b[0]), mid[1]]] if ew else [[mid[0], round(a[1])], [mid[0], round(b[1])]]
    ev(-373, "passage", "the tunnels either end of a gorge bridge", path=line, height=round(h + 1.05, 2), radius=1.7)
    wall_bridges += 1

# ---- the Order of the Lantern ---------------------------------------------------
ev(-400, "region", "the Order's temple grounds", name="temple", at="temple", radius=70)
ev(-400, "place", "the main hall of the Order's temple, facing down the torii avenue", structure="temple_hall", at=[300, 545],
   bearing=180, age=40, poi="the Order's temple")
ev(-400, "place", "the temple's pagoda", structure="pagoda", at=[255, 560], bearing=90, age=40, poi="the pagoda")
ev(-400, "place", "the temple's bell house", structure="bell_house", at=[345, 565], bearing=270, age=40)
for k in range(9):
    ev(-400, "place", "a gate on the torii avenue up to the temple", structure="torii", at=[300, 680 - k * 10], bearing=180, age=40,
       **({"poi": "the torii avenue"} if k == 0 else {}))
ev(-400, "path", "the avenue through the gates to the hall", **{"from": [300, 695], "to": [300, 575]}, width=3, mat="temple_stone")

bridges = 0


def bridge(a, b, cause):
    global bridges
    d = dist(a, b)
    ew = abs(a[0] - b[0]) > abs(a[1] - b[1])
    mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]
    return mid, ew, d


for a, b in wall_pairs:
    mid, ew, d = bridge(a, b, "")
    gap = d - 44
    length = min([n for n in (24, 32, 40, 48) if n >= gap + 10], default=48)
    ev(-380, "place", "a crossing of the Red Way, rope and plank lacquered red, over a gorge of the Old Wall",
       structure=f"rope_bridge_{length}", at=mid, bearing=0 if ew else 90, age=0, poi="the Red Way")
for a, b in zip(redway, redway[1:]):
    mid, ew, d = bridge(a, b, "")
    ev(-380, "place", "the Red Way out over the lagoon, from one levelled tower to the next", structure="rope_bridge_32",
       at=mid, bearing=0 if ew else 90, age=0)

# Temples on the levelled tops of the Wall, where the Red Way lands.
ev(-390, "place", "the Order's high hall on the levelled top of a Wall tower, facing the bridge", structure="temple_hall",
   at=[146, 334], bearing=90, age=150, poi="the high hall")
ev(-390, "place", "a stone lantern by the high hall", structure="stone_lantern", at=[160, 322], bearing=90, age=150)
ev(-390, "place", "a stone lantern by the high hall", structure="stone_lantern", at=[160, 346], bearing=90, age=150)
ev(-390, "place", "the pagoda on the north crossing of the Red Way", structure="pagoda", at=[390, 150], bearing=180, age=150, poi="the high pagoda")
ev(-390, "place", "a bell house on the Wall", structure="bell_house", at=[120, 140], bearing=180, age=150)
ev(-390, "place", "a shrine on the Wall's far crossing", structure="shrine_ruin", at=[222, 330], bearing=270, age=150)
# A great torii standing in the sea in the middle of the Choir, facing the blue hole.
ev(-380, "place", "a great torii the Order set up in the sea inside the Choir, facing the blue hole", structure="sea_torii",
   at=[places["choir"][0], places["choir"][1] + 34], bearing=0, age=40, poi="the sea torii")
ev(-380, "place", "a second torii, further out", structure="sea_torii", at=[places["choir"][0], places["choir"][1] + 50], bearing=0, age=40)

# Lily rafts on the cenotes.
for (x, z), w in cenotes:
    ev(-200, "place", "giant lilies on a cenote, in the light of its overhang", structure="lily_raft_small", at=[x, z], age=0)
ev(-200, "place", "giant lilies in the Lantern Well", structure="lily_raft", at=[250, 214], age=0, poi="the lily rafts")


# ---- the bay ---------------------------------------------------------------------
# East and south of the jungle the lagoon was a field of islands in open water.
# Between the towers the lime mud never stopped piling up, and where it reached
# the surface it grew over: low green hills with the towers standing out of them,
# so the water between is a bay with headlands and inlets instead of open sea.
#
# These come after everything above, so every event already written keeps the
# noise it had: an event is seeded by its place in the year-sorted list unless
# it says otherwise, and each existing one is pinned to the place it has now.
for k, e in enumerate(sorted(E, key=lambda e: e["year"])):
    e.setdefault("seed", k + 1)


def free(p, r, placed):
    """Room for a bank of radius r at p: in the lagoon, clear of the Choir, the
    river's course, the Red Way's line of towers and the open sea beyond."""
    x, z = p
    if not (470 <= x <= 900 and 420 <= z <= 900) or not (980 <= x + z <= 1640):
        return False
    if dist(p, places["choir"]) < 175 or near_line(p, river, 50):
        return False
    if any(dist(p, q) < 55 for q in redway):
        return False
    return all(dist(p, q) > (r + rq) * 0.58 for q, rq in placed)


banks = []
tries = 0
while len(banks) < 15 and tries < 20000:
    tries += 1
    c = (R2.uniform(470, 900), R2.uniform(420, 900))
    r = R2.uniform(44, 86)
    if free(c, r, banks):
        banks.append((c, r))
for c, r in banks:
    ev(-4000000, "deposit", "lime mud banked up between the towers until it stood out of the water and grew over: one of the low hills of the bay",
       at=[round(c[0]), round(c[1])], radius=round(r, 1), thickness=round(R2.uniform(7.5, 12.5), 1),
       mat="karst_soil", shape="dome", depth=7)
# Smaller knolls on and beside the banks, so the hills roll instead of reading
# as one smooth dome each.
knolls = 0
for _ in range(2600):
    if knolls >= 26:
        break
    c = (R2.uniform(470, 900), R2.uniform(420, 900))
    if not any(dist(c, q) < rq * 0.85 for q, rq in banks):
        continue
    r = R2.uniform(16, 32)
    ev(-3990000, "deposit", "a knoll on one of the bay's hills", at=[round(c[0]), round(c[1])],
       radius=round(r, 1), thickness=round(R2.uniform(2.2, 4.6), 1), mat="karst_soil", shape="dome", depth=7)
    knolls += 1

# Events added to chronicle.json by hand after the generator last ran
# (areas and signs), then the underwater work of 2026-11-17 (the Deep
# Eye cenote and the atoll, docs/UNDERWATER.md). Kept here verbatim so
# a rerun writes the same file; each carries its own seed or none.
TAIL = json.loads(r"""[
 {
  "year": 0,
  "event": "area",
  "cause": "the well the cave light is strongest in",
  "name": "The Lantern Well",
  "kind": "sinkhole",
  "at": "lantern_well",
  "radius": 130
 },
 {
  "year": 0,
  "event": "area",
  "cause": "what the Order of the Lantern built on the shelf",
  "name": "The Temple",
  "kind": "monastery",
  "at": "temple",
  "radius": 110
 },
 {
  "year": 0,
  "event": "area",
  "cause": "the line of towers the monks cut a way between",
  "name": "The Avenue",
  "kind": "causeway",
  "at": "avenue_foot",
  "radius": 90
 },
 {
  "year": 0,
  "event": "area",
  "cause": "the ring of towers that sings when the wind is east",
  "name": "The Choir",
  "kind": "ring",
  "at": "choir",
  "radius": 120
 },
 {
  "year": 0,
  "event": "area",
  "cause": "the green shallow sea the towers stand in",
  "name": "The Jade Water",
  "kind": "lagoon",
  "at": "centre",
  "radius": 240
 },
 {
  "year": -400,
  "event": "sign",
  "cause": "the Order cut the name of each way into the rock at its foot",
  "at": "avenue_foot",
  "text": [
   "THE",
   "AVENUE"
  ],
  "style": "carved_stone",
  "bearing": 0
 },
 {
  "year": -400,
  "event": "sign",
  "cause": "and another at the temple stair",
  "at": [
   306,
   572
  ],
  "text": [
   "TEMPLE"
  ],
  "style": "carved_stone",
  "bearing": 180
 },
 {
  "year": -390,
  "event": "sign",
  "cause": "a stone at the mouth of the passage into the well",
  "at": [
   262,
   228
  ],
  "text": [
   "THE",
   "LANTERN",
   "WELL"
  ],
  "style": "carved_stone",
  "bearing": 220
 },
 {
  "year": -2000000,
  "event": "cenote",
  "cause": "the Deep Eye: a cave under a karst hill in the bamboo country whose roof fell in; spring water filled it nearly to the rim, clear as air, and coral the colour of the jade water grew in bands down its walls",
  "name": "the_deep_eye",
  "at": [
   560,
   190
  ],
  "radius": 15,
  "hill": 30,
  "hill_radius": 80,
  "depth": 32,
  "bell": 7,
  "bed": "lime_sand",
  "poi": "the Deep Eye",
  "seed": 9101
 },
 {
  "year": -500000,
  "event": "atoll",
  "cause": "a ring of coral grown up round the rim of a drowned karst crown out in the open water to the south-west, white sand islets on it and a lagoon inside",
  "at": [
   175,
   900
  ],
  "radius": 72,
  "ring": 20,
  "lagoon_depth": 7,
  "patches": 7,
  "motu": 4,
  "motu_height": 1.5,
  "passes": 1,
  "pass_width": 12,
  "blue_hole": 0,
  "fore": {
   "crest": 0.5,
   "width": 26,
   "wall_top": 4.5,
   "wall_foot": 12,
   "wall_run": 2.0,
   "spur_spacing": 8,
   "spur_height": 1.2,
   "apron": 40,
   "undercut": 0.4
  },
  "rock": "reef_rock",
  "sand": "lime_sand",
  "poi": "the atoll",
  "seed": 9102
 },
 {
  "year": -300,
  "event": "place",
  "cause": "a trading junk of the Order's time that ran onto the atoll in a typhoon and slid down its outer wall",
  "structure": "sunken_hull",
  "at": [
   64,
   902
  ],
  "bearing": 10,
  "age": 300,
  "poi": "the junk wreck"
 },
 {
  "year": -300,
  "event": "place",
  "cause": "the junk's cargo of celadon and earthenware jars, spilled down the sand below the wall",
  "structure": "jar_spill",
  "at": [
   70,
   922
  ],
  "bearing": 0,
  "age": 300,
  "poi": "the jar field"
 }
]""")
E.extend(TAIL)

out = {"places": places, "events": E}
with open(os.path.join(HERE, "chronicle.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(out, f, indent=1)
print(len(E), "events;", len(blocks), "wall blocks;", len(towers), "lagoon towers;", len(gorge_pts), "gorge points;",
      lagoon_bridges, "lagoon bridges;", cave_bridges, "cavern bridges;", wall_bridges, "gorge bridges;", len(lakes), "tower lakes;",
      len(banks), "bay hills;", knolls, "knolls")
