"""Writes worlds/thornpan/chronicle.json. The JSON is what the game reads.
See the note below.

Order matters twice over: events run oldest year first, and events in the same
year run in file order. The years below are set so the list reads as the story
does -- granite, rift, the lake and its waterline, the river, the fan, the
pans, the people.

One-shot, not kept current. This ran once, when the world was made; the
chronicle has moved on since (hand edits, new events and fields), so rerunning
it would overwrite live data with a stale version. It is kept as a record of
how the world was first written. Edit chronicle.json directly instead, and if
you do rerun this, diff the output before keeping any of it.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))

PLACES = {
    "centre": [880, 560],
    # The upper river: in at the north-east corner, down the rift trough.
    "source": [1500, 90],
    "gate": [1240, 330],
    "narrows": [980, 580],
    "narrows_rim": [1012, 520],
    # The fan.
    "spread": [520, 900],
    "head": [880, 700],
    "breach": [760, 790],
    "sedge": [560, 860],
    "longwater": [1140, 450],
    "camp": [612, 986],
    # The dead end.
    "pans": [250, 1200],
    "tomb": [470, 1210],
    # The hills.
    "lamp_kopje": [900, 1120],
    "bee_bend": [1014, 558],
}

# Seven kopjes: (x, z, radius, height). Lamp Kopje is the tallest and carries
# the tower; the rest break up the grass and give the long view something to
# stack behind.
KOPJES = [
    (900, 1120, 34, 26),
    (1180, 880, 26, 18),
    (1340, 640, 22, 15),
    (1120, 1360, 24, 16),
    (1060, 300, 20, 13),
    (1420, 1100, 28, 20),
    (700, 480, 18, 12),
]

E = []
LATE = []


def ev(year, event, cause, **kw):
    E.append({"year": year, "event": event, "cause": cause, **kw})


# ---- the rock ----------------------------------------------------------------

ev(-40000000, "landmass",
   "granite basement under its own sand, rolling sandveld standing above the flood in the north and east and letting down into it in the south-west",
   centre="centre", radius=1480, roughness=0.5, hills=44, ridges=12, shelf=2, edge_fall=150)

# The rift: two faults forty metres apart in height, the block between them
# dropped. The raised side of an escarpment is to the right of from -> to.
ev(-2000000, "escarpment", "the north-west fault of the rift: the block beyond it stood up and the river took the trough",
   **{"from": [1330, 230], "to": [930, 660]}, height=13, width=14, reach=300)
ev(-2000000, "escarpment", "the south-east fault, facing the first across the trough",
   **{"from": [1010, 760], "to": [1420, 320]}, height=11, width=13, reach=300)

for i, (x, z, r, h) in enumerate(KOPJES):
    # A deposit dome, not a mesa: a kopje is a whaleback of granite, not a
    # butte, and a dome of the rock itself gives the curved shells.
    ev(-1800000, "deposit", "a granite dome standing out of its own sand, weathering off in curved shells: one of the Stacks",
       at=[x, z], radius=r + 16, thickness=h, mat="granite", shape="dome")
    ev(-1790000, "region", "the bare rock of the Stacks", name="stacks", at=[x, z], radius=r + 6)
    # The boulders go in after the water has finished cutting (year -13000),
    # or half of them are dropped onto ground a later event takes away.
    LATE.append((x, z, r, i))

ev(-1600000, "erode_rims", "a hundred thousand dry seasons rounding the top of every rock face in the country",
   depth=3, reach=12, gullies=0.45, min_drop=4)

# ---- the river and the fan ---------------------------------------------------

UPPER = [[1500, 90], [1380, 240], [1240, 330], [1120, 430], [1020, 520], [980, 580]]
LOWER = [[880, 700], [760, 790], [660, 860], [560, 930], [470, 1000], [390, 1070]]

ev(-60000, "vale", "the trough floor worn into a soft valley, grassy to the water on the open bank",
   path=UPPER + [[880, 700]], width=90, depth=5, depth_end=4, side=70, meander=7)

ev(-14000, "subside",
   "the lake that filled this basin to thirty-four metres gone, and the ground it stood on settling: the Spread, flat and a hand under the water for two months of the year",
   at="spread", radius=440, level=[0.0, 0.5], grain=80, edge=80, ragged=0.42,
   islands=0.5, island_size=34, island_rise=1.3)

ev(-13800, "subside", "the deepest part of the old lake floor, swept bare and left dry: the White Pans",
   at="pans", radius=300, level=[0.3, 1.0], grain=60, edge=60, ragged=0.3, stretch=1.4, bearing=120)
ev(-13700, "deposit", "salt dried out of the last of the lake, plate on plate, until the pan floor stood clear of the water again",
   at="pans", radius=290, thickness=1.6, mat="shore_sand", shape="dome", depth=2)

ev(-13600, "deposit", "the island Sedge stands on: a sand knoll the fan built and kept",
   at="sedge", radius=78, thickness=4.4, mat="shore_sand", shape="dome", depth=4)
ev(-13600, "deposit", "the bank Barnacle Camp stands on",
   at="camp", radius=46, thickness=4.0, mat="shore_sand", shape="dome", depth=4)
ev(-13600, "deposit", "the rise the Tomb Tree stands on, off the east rim of the pans",
   at="tomb", radius=42, thickness=3.6, mat="shore_sand", shape="dome", depth=4)

ev(-3000, "river", "the Thorn, down the trough and out into the Spread, which is as far as any water in this country goes",
   name="thorn", path=UPPER + LOWER, width=26, depth=3.2, bed="shore_sand", bank="shore_sand",
   meander=12, floor=2.4)

ev(-1200, "lake", "a pool the hippo hold at the head of the fan, too deep to dry",
   name="head_pool", at=[800, 760], radius=52, depth=3.4, bed="shore_sand", bank="shore_sand",
   ragged=0.35, stretch=1.5, bearing=70)
ev(-1100, "lake", "a second pool down the fan, kept open by the elephants crossing it",
   name="low_pool", at=[470, 1060], radius=40, depth=2.8, bed="shore_sand", bank="shore_sand",
   ragged=0.4, stretch=1.3, bearing=130)

ev(-120, "avulsion",
   "the Thorn broke out at the head of the fan and took the west way: the old channel below dried to a grass lane in a season",
   river="thorn", breach="breach", old_bed="shore_sand", silt="shore_sand", silt_thick=0.5,
   path=[[760, 790], [640, 830], [540, 890], [450, 950], [370, 1020]])

for i, (path, w) in enumerate([
    ([[700, 840], [620, 900], [560, 970], [500, 1050]], 11),
    ([[520, 900], [430, 960], [340, 1040], [250, 1130]], 10),
    ([[840, 760], [820, 860], [760, 950], [700, 1040]], 9),
]):
    ev(-100, "bayou", "a channel the elephants keep open across the fan, filled end to end by the flood",
       path=path, width=w, depth=1.6, bank=6, meander=9)

for x, z, r, i in LATE:
    ev(-13000, "spires", "boulders split off the dome and stacked where they fell",
       structures=["granite_stack"], at=[x, z], radius=r + 20, count=6 + (i == 0) * 3, spacing=18)

# ---- names on the ground -----------------------------------------------------

ev(-90, "region", "the Spread: the fan of channels and sand islands the river ends in", name="spread", at="spread", radius=470)
ev(-90, "region", "the White Pans, the old lake floor", name="pans", at="pans", radius=310)
ev(-90, "region", "the Green Bank: closed forest on the shaded side of the upper river", name="green_bank", at=[1062, 286], radius=250)

ev(-60, "fire", "a dry-season grass fire south of the Spread, run out by the afternoon wind; the bee-eaters followed it",
   at=[760, 1060], radius=180, wind_bearing=120, stretch=1.8, char="charcoal")

# ---- the people --------------------------------------------------------------

ev(-300, "place", "the Ringstone, built by people who were gone before the present ones came",
   structure="ring_wall", poi="the Ringstone", at=[930, 1150], face="lamp_kopje", age=300)
ev(-900, "place", "the Lamp: a tower built to carry a light, facing out over the deepest part of a lake that was already gone",
   structure="lamp_tower", poi="the Lamp", at="lamp_kopje", bearing=225, age=900)

ev(-40, "settlement",
   "Sedge, on the biggest island in the Spread: round houses, drying racks, a stock pen for the dry season and a landing",
   name="sedge", at="sedge", radius=74, houses=9,
   buildings=["round_house", "round_house_small", "granary"],
   extras=[{"structure": "drying_rack", "count": 1, "near": "edge"}],
   slots={"wall": "daub", "roof": "thatch"}, path="path_dirt")
ev(-40, "settlement",
   "Longwater, upstream on the open bank where the cattle are",
   name="longwater", at="longwater", radius=80, houses=7,
   buildings=["round_house", "round_house_small", "granary"],
   extras=[{"structure": "stock_pen", "count": 2, "near": "edge"},
           {"structure": "hurdle_fence", "count": 4, "near": "edge"}],
   slots={"wall": "daub", "roof": "thatch"}, path="path_dirt")
ev(-35, "settlement",
   "Barnacle Camp, a fishing camp on a lower channel, built against the hull of the broken ship and partly out of it",
   name="camp", at="camp", radius=40, houses=3,
   buildings=["net_shed", "round_house_small"],
   extras=[{"structure": "drying_rack", "count": 2, "near": "centre"}],
   slots={"wall": "daub", "roof": "thatch"}, path="path_dirt")

# The baobabs that are buildings.
ev(-30, "place", "the Council Tree: the hollow baobab Sedge meets in", structure="hollow_baobab",
   at=[592, 884], face="sedge")
ev(-30, "place", "the Cistern Tree, on the rise above Longwater, kept full against the end of the dry",
   structure="baobab_cistern", poi="the Cistern Tree", at=[1108, 492], face="longwater")
ev(-180, "place", "the Tomb Tree, east of the pans: two generations of one family, closed with a slab",
   structure="baobab_tomb", poi="the Tomb Tree", at="tomb", bearing=270, age=180)

# The ships.
# Placed late in the list, after the land is final, so no later event cuts
# through them; `age` is what weathers them, not the year.
ev(-22, "place", "a sea-going hull on her beam ends on the salt, masts still up, in a country with no sea",
   structure="careened_hull", at=[300, 1230], bearing=40, age=400, poi="the Careen")
ev(-22, "place", "a hull broken in the reeds, barnacled to the rail; the fishing camp is built against it",
   structure="wreck_ribs", at=[648, 1004], bearing=110, poi="the Barnacle")
ev(-22, "place", "a third hull on her side in the flood meadows, the cattle rub on her",
   structure="wreck_ribs", at=[700, 806], bearing=20, poi="the Ribs")

# The bee-eater bank, on the outside of the second bend below the Narrows.
ev(-20, "place", "fifteen metres of cut bank with a thousand nest holes in it: the carmine bee-eater colony",
   structure="bee_bank", poi="the bee-eater bank", at="bee_bend", bearing=270)

# Landings.
for i, (x, z, b) in enumerate([(566, 898, 200), (574, 902, 210), (582, 906, 220),
                               (1126, 478, 160), (636, 996, 140)]):
    ev(-20, "place", "a mokoro drawn up at a landing", structure="mokoro", at=[x, z], bearing=b)
ev(-20, "place", "the flood post at Sedge's landing, the years cut into it", structure="flood_post",
   at=[570, 892], bearing=200)
ev(-20, "place", "the flood post at Longwater's landing", structure="flood_post", at=[1130, 470], bearing=160)

# The lake coming back: open water on the pans at the hour before sunset, with
# a hull in it, gone by the time you are close enough to see it properly.
ev(-5, "place", "the lake coming back: a hull standing in open water on the pan, seen only from a distance and only before sunset",
   structure="careened_hull", at=[196, 1150], bearing=70, when={"far": 320, "hours": [16.4, 19.0]}, poi="the lake coming back")
ev(-5, "place", "another of them, further out on the salt", structure="wreck_ribs",
   at=[330, 1300], bearing=150, when={"far": 280, "hours": [16.4, 19.0]})

# ---- areas -------------------------------------------------------------------

AREAS = [
    ("The Narrows", "valley", [1000, 600], 200),
    ("The Green Bank", "forest", [1080, 300], 210),
    ("The Dry Bank", "grassland", [1290, 560], 240),
    ("The Spread", "delta", [520, 900], 430),
    ("The White Pans", "pan", [250, 1200], 300),
    ("Lamp Kopje", "hill", [900, 1120], 90),
    ("Sedge", "village", [560, 860], 80),
    ("Longwater", "village", [1140, 450], 86),
    ("Barnacle Camp", "camp", [612, 986], 46),
    ("The Stacks", "hills", [1250, 880], 280),
]
for name, kind, at, r in AREAS:
    ev(-1, "area", "the name this ground goes by", name=name, kind=kind, at=at, radius=r)

E.sort(key=lambda e: e["year"])
out = {"places": PLACES, "events": E}
with open(os.path.join(HERE, "chronicle.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump(out, f, indent=2)
    f.write("\n")
print(f"{len(E)} events")
