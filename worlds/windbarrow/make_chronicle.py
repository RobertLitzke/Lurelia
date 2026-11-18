"""Writes worlds/windbarrow/chronicle.json. The JSON is what the game reads.
See the note below.

Windbarrow is chalk downland with no coast: the landmass radius is far larger
than the world, so every cell is land and the only water is the Narrow and
Saltmere. Order of the history: the chalk and its flint bands, the Edge, the
combes cut into it, the caves under the Flint Down, the valley of the Narrow
and the salt hollow, then six hundred years of people.

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

HERE = os.path.dirname(os.path.abspath(__file__))

SIZE = 1536

PLACES = {
    "durn": [430, 790],
    "durn_centre": [430, 790],
    "crest": [832, 424],
    "tepe": [592, 622],
    "barrow_north": [782, 506],
    "barrow_south": [706, 786],
    "long_barrow": [742, 566],
    "green_kurgan": [778, 498],
    "mares_barrow": [736, 648],
    "saltmere": [320, 1230],
    "gape": [1146, 752],
    "gape_south": [1134, 918],
    "swallow": [1278, 702],
    "flint_camp": [1266, 700],
    "spring": [560, 200],
    "ford": [404, 712],
}

events = []


def ev(year, event, cause, **kw):
    events.append({"year": year, "event": event, "cause": cause, **kw})


# ---- the chalk ----------------------------------------------------------------

ev(-90000000, "landmass",
   "lime mud laid down in a warm sea and lifted without folding: chalk downland "
   "running past every horizon, so the world has no coast",
   centre=[768, 768], radius=2600, roughness=0.5, hills=45, ridges=11, shelf=14)

# The chalk block stands well clear of the water: nothing in the country floods,
# and the only standing water is the Narrow and Saltmere.
ev(-80000000, "deposit",
   "the whole chalk block standing high and dry, thickest in the middle of the country",
   at=[768, 768], radius=3400, thickness=18, mat="chalk_white", shape="dome")

# Harder cores in the chalk that weathered slower: knolls standing over the
# general swell, so from a valley the next hill shuts off the view.
KNOLLS = [
    (236, 352, 150, 13), (430, 236, 120, 11), (650, 320, 96, 9), (886, 232, 134, 12),
    (160, 690, 118, 10), (470, 470, 104, 9), (560, 880, 142, 13), (300, 980, 110, 10),
    (700, 1100, 150, 14), (900, 760, 126, 11), (920, 1060, 108, 9), (1010, 300, 96, 8),
    (170, 1360, 132, 12), (520, 1300, 120, 11), (860, 1380, 104, 9),
]
for kx, kz, kr, kt in KNOLLS:
    ev(-1800000, "deposit", "a harder core in the chalk that weathered slower than the rest",
       at=[kx, kz], radius=kr, thickness=kt, shape="dome", mat="chalk_white")

ev(-2000000, "escarpment",
   "the chalk snapped along a line and the eastern block stood up: the Barrow Edge",
   **{"from": [1070, 1500], "to": [1160, 60]}, height=20, width=15, reach=2000)

# Combes cut back into the high side of the Edge, so the face is a run of bays
# and noses instead of one wall.
COMBES = [
    ([[1248, 268], [1196, 258], [1140, 254], [1086, 252]], 9, 8),
    ([[1232, 362], [1186, 358], [1140, 356], [1096, 354]], 7, 6),
    ([[1262, 452], [1210, 446], [1156, 442], [1104, 440]], 10, 9),
    ([[1218, 604], [1172, 600], [1128, 598], [1090, 596]], 8, 7),
    ([[1258, 866], [1204, 860], [1150, 856], [1098, 854]], 10, 9),
    ([[1240, 1066], [1188, 1058], [1132, 1052], [1080, 1050]], 9, 8),
    ([[1226, 1174], [1180, 1168], [1136, 1164], [1096, 1162]], 7, 6),
    ([[1252, 1288], [1200, 1282], [1148, 1278], [1098, 1276]], 10, 7),
    ([[1236, 150], [1188, 146], [1142, 144], [1100, 142]], 8, 7),
]
for path, width, depth in COMBES:
    ev(-200000, "vale", "a dry combe worn back into the face of the Edge",
       path=path, width=width, depth=depth, side=30, fall=5, meander=6)

# The Gape: a deeper combe, cut to the level the caves open at.
ev(-200000, "canyon",
   "the Gape: the deepest of the combes, cut down to the level the caves open at",
   path=[[1310, 772], [1246, 764], [1192, 758], [1142, 752]], width=11, depth=17,
   depth_end=14, wall=0.7, bench=7, meander=5, floor="chalk_rubble", poi="the Gape")
ev(-200000, "canyon",
   "a second combe in the next bay south, with a lower cave mouth in its head",
   path=[[1282, 940], [1224, 932], [1174, 926], [1130, 920]], width=10, depth=22,
   depth_end=17, wall=0.7, bench=7, meander=5, floor="chalk_rubble")

ev(-150000, "erode_rims",
   "frost and rain rounding the lip of the Edge and gullying its tops",
   depth=3, reach=11, gullies=0.5, min_drop=4)

# ---- water --------------------------------------------------------------------

ev(-60000, "subside",
   "a shallow basin in the south-west chalk with no way out of it",
   at="saltmere", radius=260, drop=[7, 11], grain=80, edge=70, ragged=0.3, stretch=1.3,
   bearing=120)

# The valley swings from side to side down the country instead of running
# straight for the lake.
NARROW = [[560, 200], [538, 268], [498, 326], [506, 398], [470, 452], [424, 496],
          [438, 566], [466, 624], [430, 678], [376, 712], [352, 772], [378, 830],
          [396, 892], [360, 944], [320, 992], [336, 1058], [350, 1120], [322, 1172],
          [318, 1206]]

ev(-40000, "vale",
   "the valley of the Narrow, worn across the whole country from the northern springs "
   "to the salt hollow",
   path=NARROW, width=34, depth=7, depth_end=5, side=92, meander=16)

ev(-30000, "lake",
   "Saltmere: the Narrow ends here and goes no further, so the water stands and turns salt",
   name="saltmere", at=[322, 1226], radius=152, depth=3.2, bed="salt_mud", bank="salt_crust",
   ragged=0.35, stretch=1.5, bearing=115)

ev(-28000, "river",
   "the Narrow, rising in springs at the foot of the northern downs and running south-west "
   "into Saltmere",
   name="narrow", path=NARROW, width=7, depth=2.6, bed="river_gravel", bank="river_mud",
   meander=22)

ev(-20000, "moisture", "springs and seepage all down the valley of the Narrow",
   at=[430, 700], radius=420, amount=0.2)
ev(-20000, "moisture", "the dry high ground east of the Edge", at=[1270, 760], radius=340,
   amount=-0.3)

# ---- regions ------------------------------------------------------------------

ev(-19000, "region", "the Flint Down: thin ground over rubble east of the Edge",
   name="flint", at=[1272, 700], radius=330)
ev(-19000, "region", "the Flint Down, carried south along the top of the Edge",
   name="flint", at=[1210, 1000], radius=230)
ev(-19000, "region", "Saltmere and its white shore", name="saltmere", at=[322, 1226], radius=215)
ev(-19000, "region",
   "a hollow in the northern downs the cold sits in and does not leave, whatever the season",
   name="frost", at=[560, 190], radius=36)
ev(-19000, "region",
   "the Bone Downs: the stretch of the Flint Down where it is mammoth bone that comes out of the "
   "chalk and not flint",
   name="bone", at=[1352, 940], radius=95)

# ---- the caves ----------------------------------------------------------------
# A dry system in the chalk under the Flint Down, running east from the head of
# the Gape, with a second mouth in the head of the combe south of it and a
# swallow hole over the first chamber. A `passage` is a tube about its axis, so
# each one's axis is CAVE_FLOOR + its radius and the floors run on the level.

# Heights in events are metres above sea, and the two combe heads are cut to 47.
CAVE_FLOOR = 47.0

CAVE = [
    # east from the head of the Gape, in under the down
    ([[1310, 769], [1346, 762], [1382, 752], [1414, 742]], 2.8, 1, "the Gape passage"),
    # the first chamber, with the swallow hole up out of it
    ([[1418, 740], [1430, 732]], 4.2, 1, "the First Chamber"),
    # north, then back west, deeper in
    ([[1430, 732], [1452, 702], [1460, 668], [1452, 634]], 2.6, 0, None),
    ([[1452, 634], [1420, 606], [1380, 580], [1340, 560]], 2.4, 1, None),
    # the bone chamber at the far end, under the highest ground
    ([[1336, 556], [1322, 548]], 4.4, 0, "the Bone Chamber"),
    # the south branch, round toward the second mouth
    ([[1414, 742], [1404, 780], [1388, 818], [1362, 852], [1330, 884]], 2.4, 2, None),
    ([[1330, 884], [1304, 906], [1286, 936]], 2.6, 0, None),
    # a blind side passage: the one with the cat scratched on its wall
    ([[1452, 702], [1476, 728], [1482, 756]], 2.0, 0, "the Scratched Cat"),
]
for path, radius, shafts, poi in CAVE:
    kw = {"path": path, "height": CAVE_FLOOR + radius, "radius": radius,
          "wall": "cave_chalk", "wander": 3}
    if shafts:
        kw["shafts"] = shafts
    if poi:
        kw["poi"] = poi
    ev(-18000, "passage",
       "water running down the joints of the chalk hollowed out the Flint Down; the passages "
       "are dry now", **kw)

# ---- the Tepe -----------------------------------------------------------------

ev(-4200, "place",
   "the Tepe: twelve carved pillars in a bench wall, raised by people the villages do not "
   "account for, and older than any of them",
   structure="tepe_enclosure", at="tepe", bearing=160, poi="the Tepe")
ev(-4200, "place",
   "a thirteenth pillar, dressed and dragged part of the way up the hill, and left",
   structure="tepe_pillar", at=[640, 676], bearing=70, poi="the left pillar")
ev(-4190, "place",
   "a pillar of the same stone standing alone in the empty north, a day from the Tepe and carved "
   "with a skate instead of a beast",
   structure="skate_pillar", at=[228, 112], bearing=180, poi="the Skate Pillar")

# A spiral cut through the turf to the chalk, out on the empty southern downs.
# Laid as paths, so it follows the ground instead of sitting on it.
LAB = [742, 1096]
turns, r0, r1, step = 3.0, 4.0, 42.0, 0.42
pts = []
a = 0.0
while a <= turns * 2 * math.pi:
    r = r0 + (r1 - r0) * a / (turns * 2 * math.pi)
    pts.append([round(LAB[0] + r * math.cos(a), 1), round(LAB[1] + r * math.sin(a), 1)])
    a += step
for u, v in zip(pts, pts[1:]):
    ev(-4180, "path", "the turf maze on the southern downs, cut through the grass to the chalk",
       **{"from": u, "to": v}, width=1.3, mat="chalk_white")
ev(-4179, "place", "the post at the mouth of the turf maze", structure="barrow_post",
   at=[round(LAB[0] + 45.0, 1), LAB[1]], bearing=270, poi="the turf maze")
ev(-4178, "place",
   "what the maze is walked to: a shaft cut twenty metres into the chalk at its heart, collared and "
   "ochred, with air coming steadily up out of it as it does at the Long Barrow",
   structure="wind_shaft", at=LAB, bearing=0, poi="the shaft at the heart of the maze")

# ---- the barrow field ---------------------------------------------------------

ev(-3400, "region", "the barrow ridge, where the dead go", name="barrows",
   at=[742, 640], radius=170)

# Families bury their own together, so the mounds stand in loose knots over the
# ridge rather than in a line: three clusters, each a big barrow with its own
# scattered round the it, at least 14 m apart.
rnd = random.Random(31)
FIXED = [(736, 648, 17.0), (712, 764, 10.0), (778, 498, 5.0)]
CLUSTERS = [(742, 560, 62, 6), (738, 654, 74, 7), (718, 760, 66, 6)]
KURGANS = list(FIXED)
for cx, cz, spread, count in CLUSTERS:
    placed = 0
    for _ in range(400):
        if placed >= count:
            break
        a = rnd.uniform(0, 2 * math.pi)
        d = spread * math.sqrt(rnd.uniform(0.08, 1.0))
        x, z = round(cx + d * math.cos(a)), round(cz + d * math.sin(a) * 0.8)
        r = rnd.choice([3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 9.0])
        if all((x - ox) ** 2 + (z - oz) ** 2 > (r + orr + 14) ** 2 for ox, oz, orr in KURGANS):
            KURGANS.append((x, z, r))
            placed += 1
# oldest first, so the big ones are not always the newest
rnd.shuffle(KURGANS)

for k, (x, z, r) in enumerate(KURGANS):
    thick = round(1.0 + r * (0.42 if r >= 8.0 else 0.5), 1)
    kw = {"at": [x, z], "radius": r, "thickness": thick, "mat": "chalk_white", "shape": "dome"}
    ev(-3200 + k * 40, "deposit",
       "a kurgan heaped over the dead, the chalk out of its own ring ditch", **kw)

# Kerbs on the mounds the kerb ring fits, posts on some of the crowns.
for k, (x, z, r) in enumerate(KURGANS):
    if 6.0 <= r <= 7.0:
        ev(-3100 + k * 40, "place", "a kerb of chalk blocks set round the foot of the mound",
           structure="kurgan_kerb", at=[x, z], bearing=rnd.choice([0, 30, 60, 90]))
    if r >= 8.0 or k % 3 == 0:
        kw = {"structure": "barrow_post", "at": [x, z], "bearing": rnd.choice([0, 45, 90, 135])}
        if r == 17.0:
            kw["poi"] = "the Mare's Barrow"
        ev(-3080 + k * 40, "place", "a split oak post standing on the crown of the mound", **kw)

# The Long Barrow: the oldest thing built here, and open.
ev(-3600, "earthwork",
   "the Long Barrow, heaped before any of the round mounds and kerbed at its east end",
   parts=[{"from": [708, 566], "to": [748, 566], "width": 9, "top": 3.6}],
   slope=2.0, relative=True, poi="the Long Barrow")
ev(-3590, "place",
   "the mouth of the Long Barrow, its closing slab pushed aside long ago and never put back",
   structure="long_barrow_mouth", at=[754, 566], bearing=90, poi="the mouth of the Long Barrow")
ev(-3580, "path",
   "the grass round the forecourt, combed flat and away by the air that comes steadily out "
   "of the mouth, in all weathers",
   **{"from": [757, 566], "to": [765, 566]}, width=9, mat="chalk_rubble")

ev(-3300, "region", "the hilltop round the Tepe, which the villages still keep clear of grass",
   name="tepe", at="tepe", radius=26)
ev(-3000, "region", "the one mound in the field that is green when the rest are gold",
   name="green_kurgan", at="green_kurgan", radius=11)
ev(-2990, "place", "the post on the green kurgan, which nobody renews and which does not rot",
   structure="barrow_post", at=[778, 498], bearing=20, poi="the green kurgan")

# ---- Durn ---------------------------------------------------------------------

HOUSE_SLOTS = {"wall": "daub", "roof": "thatch", "timber": "oak_post", "floor": "packed_dirt",
               "hurdle": "stick_wood"}

ev(-600, "settlement",
   "Durn, in a hollow on the south bank of the Narrow: eleven round houses, the longhouse, "
   "granaries and pens",
   name="durn", at="durn", radius=78, houses=11,
   buildings=["round_house", "round_house", "round_house_small", "round_house"],
   extras=[{"structure": "longhouse", "count": 1, "near": "centre"},
           {"structure": "granary", "count": 2, "near": "centre"},
           {"structure": "stock_pen", "count": 2, "near": "edge"},
           {"structure": "hurdle_fence", "count": 6, "near": "edge"},
           {"structure": "drying_rack", "count": 3, "near": "edge"},
           {"structure": "camp_hearth", "count": 1, "near": "edge"}],
   slots=HOUSE_SLOTS, path="chalk_rubble")
ev(-595, "region", "the trodden ground of Durn", name="steading", at="durn", radius=48)
for k, (x, z, r) in enumerate([(362, 806, 26), (430, 870, 22)]):
    ev(-590 + k, "region", "a walled barley plot below Durn", name="barley", at=[x, z], radius=r)
ev(-588, "region",
   "the ground under the great oak below Durn, older than the village and left alone",
   name="greatoak", at=[470, 814], radius=9)

# ---- Crest --------------------------------------------------------------------

ev(-520, "settlement",
   "Crest, in the lee of a ridge north of the barrow field: seven houses and a longhouse, "
   "the diggers' village",
   name="crest", at="crest", radius=58, houses=7,
   buildings=["round_house", "round_house_small", "round_house"],
   extras=[{"structure": "longhouse", "count": 1, "near": "centre"},
           {"structure": "granary", "count": 2, "near": "centre"},
           {"structure": "stock_pen", "count": 1, "near": "edge"},
           {"structure": "hurdle_fence", "count": 4, "near": "edge"},
           {"structure": "drying_rack", "count": 2, "near": "edge"},
           {"structure": "camp_hearth", "count": 1, "near": "edge"}],
   slots=HOUSE_SLOTS, path="chalk_rubble")
ev(-515, "region", "the trodden ground of Crest", name="steading", at="crest", radius=44)
ev(-510, "region", "Crest's one barley plot, in the lee of the ridge", name="barley",
   at=[882, 452], radius=20)

# ---- the flint pits and the summer camp ---------------------------------------

PITS = [([[1240, 668], [1262, 664]], 7, 5),
        ([[1282, 676], [1304, 684]], 6, 4.5),
        ([[1252, 724], [1274, 732]], 7, 5),
        ([[1300, 716], [1318, 706]], 6, 4)]
for k, (path, width, depth) in enumerate(PITS):
    ev(-300 + k, "canyon", "an open flint pit, dug down to the upper nodule band and left",
       path=path, width=width, depth=depth, wall=0.5, bench=4, meander=2, floor="chalk_rubble")
    ev(-299 + k, "deposit", "the spoil heaped beside a flint pit",
       at=[path[0][0] - 14, path[0][1] + 12], radius=11, thickness=2.2, mat="chalk_rubble",
       shape="dome")

ev(-290, "place", "the diggers' windbreak at the pits, used in the dry months only",
   structure="windbreak", at=[1266, 696], bearing=250, poi="the summer camp")
for k, (x, z, b) in enumerate([(1246, 702, 250), (1278, 712, 250), (1256, 682, 250)]):
    ev(-289 + k, "place", "a hut at the summer camp", structure="round_house_small", at=[x, z],
       bearing=b)
ev(-286, "place", "the camp's hearth", structure="camp_hearth", at=[1264, 708], bearing=0)
ev(-285, "place",
   "the bone house at the summer camp: skulls for a footing, ribs for rafters and hides lashed over "
   "them, because nothing grows up here to build with",
   structure="bone_house", at=[1254, 688], bearing=240, poi="the bone house")
for q, (bx, bz, bb) in enumerate([(1280, 690, 160), (1274, 720, 70)]):
    ev(-284 + q, "place", "a stock fence of mammoth ribs at the summer camp",
       structure="bone_fence", at=[bx, bz], bearing=bb)
for k, (x, z) in enumerate([(1246, 672), (1288, 680), (1258, 728)]):
    ev(-285 + k, "place", "picks and a knapping floor left at a pit's edge",
       structure="digger_gear", at=[x, z], bearing=200)

# ---- the trails ---------------------------------------------------------------
# A foot and a half wide, kept open by use, and along the ridges rather than the
# valleys because the ridges are dry and you can see along them.

TRAILS = [
    # Durn up to the Tepe and on along the barrow ridge to Crest
    ([[452, 778], [500, 740], [548, 690], [580, 650], [592, 622]], 1.6),
    ([[600, 606], [648, 592], [700, 578], [742, 572], [756, 566]], 1.6),
    ([[742, 572], [760, 540], [776, 510], [800, 476], [824, 440]], 1.5),
    # the ridge south through the barrow field
    ([[742, 578], [736, 618], [730, 668], [718, 720], [708, 768]], 1.4),
    # Crest east to the Gape and up to the flint camp
    ([[840, 420], [910, 428], [986, 450], [1058, 490], [1096, 536]], 1.5),
    ([[1096, 536], [1118, 596], [1130, 664], [1140, 724], [1146, 748]], 1.4),
    ([[1096, 536], [1150, 560], [1206, 600], [1244, 648], [1262, 688]], 1.4),
    # north up the valley head, past the frost hollow, on to the Skate Pillar
    ([[520, 300], [536, 266], [550, 240], [558, 226]], 1.4),
    ([[552, 214], [524, 192], [486, 176], [440, 162], [392, 146], [340, 130], [292, 118], [242, 112]], 1.3),
    # south off the barrow ridge to the turf maze
    ([[708, 772], [714, 822], [722, 876], [728, 932], [734, 992], [740, 1046], [742, 1082]], 1.4),
    # east off the flint camp onto the Bone Downs
    ([[1272, 712], [1288, 760], [1306, 812], [1324, 866], [1340, 910], [1348, 934]], 1.3),
    # Durn down the valley to the ford, and on toward Saltmere
    ([[424, 800], [412, 760], [404, 724], [400, 706]], 1.6),
    ([[396, 716], [380, 800], [364, 890], [346, 990], [334, 1090], [328, 1166]], 1.4),
]
for path, width in TRAILS:
    for a, b in zip(path, path[1:]):
        ev(-250, "path", "a trail trodden through the grass and kept open by use",
           **{"from": a, "to": b}, width=width, mat="chalk_rubble")

ev(-240, "place", "a post at the parting of the trails below the Tepe",
   structure="offering_post", at=[598, 612], bearing=200, poi="the offering post")
ev(-239, "place", "a post where the trail leaves the barrow ridge for the Gape",
   structure="offering_post", at=[1098, 534], bearing=300)
ev(-238, "place", "a post at the ford on the Narrow",
   structure="offering_post", at=[408, 708], bearing=90)
ev(-237, "place",
   "the post on the rim of the frost hollow, set facing in: the trail goes round it and the sheep "
   "are not let down",
   structure="offering_post", at=[558, 222], bearing=180, poi="the frost hollow")
ev(-236, "place",
   "the post where the diggers stopped working east: beyond it the chalk gives bone, not flint",
   structure="offering_post", at=[1346, 928], bearing=120, poi="the Bone Downs")

# ---- the cave mouths and what is in them --------------------------------------
# Placed last so they carve into ground that is finished: each sits in the head
# of a combe cut down to the cave floor, facing out down its combe (west).

ev(-200, "place", "the Gape: the mouth of the cave system in the head of the deepest combe",
   structure="cave_mouth", at=[1304, 770], bearing=270, poi="the mouth of the Gape")
ev(-199, "place", "the lower mouth, in the head of the combe south of the Gape",
   structure="cave_mouth", at=[1278, 938], bearing=270)
# The bone and the claw-scoring start at the mouths: beds just outside them, on
# the combe floor, where the bears lie up in the sun.
for k, (x, z, b) in enumerate([(1292, 772, 90), (1266, 936, 90)]):
    ev(-198 + k, "place", "a cave bear's bed at the mouth, bone trodden into the chalk",
       structure="bear_den", at=[x, z], bearing=b)
ev(-196, "place",
   "a long-toothed cat scratched on a block at the mouth of the Gape, by someone who went in "
   "far enough to see one",
   structure="cat_scratch", at=[1298, 764], bearing=250, poi="the scratched cat")

out = {"places": PLACES, "events": sorted(events, key=lambda e: e["year"])}
path = os.path.join(HERE, "chronicle.json")
with open(path, "w", encoding="utf-8", newline=chr(10)) as f:
    json.dump(out, f, indent=2)
    f.write(chr(10))
print(f"wrote {len(events)} events to {path}")
