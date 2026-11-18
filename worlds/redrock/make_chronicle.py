"""Writes chronicle.json for Redrock. The chronicle is data; this script only
saves typing for the repeated pieces (the hoodoo ring, the fins). See the note below.

One-shot, not kept current. This ran once, when the world was made; the
chronicle has moved on since (hand edits, new events and fields), so rerunning
it would overwrite live data with a stale version. It is kept as a record of
how the world was first written. Edit chronicle.json directly instead, and if
you do rerun this, diff the output before keeping any of it.
"""
import json
import math
import os

places = {
    "centre": [768, 740],
    "cone": [1080, 190],
    "station": [640, 556],
    "harlow": [440, 1284],
    "harlow_mesa": [430, 1160],
    "bitterwater": [800, 1390],
    "gathering": [420, 840],
    "fins": [1035, 655],
}

E = []
def ev(year, kind, cause, **kw):
    if kind == "canyon":
        # Bench heights, their level and ledge widths wander (and follow folded beds).
        kw.setdefault("irregular", 1.0)
    E.append({"year": year, "event": kind, "cause": cause, **kw})

# ---- the rock -------------------------------------------------------------
ev(-6500000, "dip", "the west of the country, tipped down toward the old river by a slow monocline: the bands of the Red and White Cliffs run down at a slant toward the amphitheatre",
   at=[300, 400], radius=560, bearing=245, grade=0.04, warp=6)
ev(-6500000, "dip", "the east, bowed up over the fin country the other way, so the beds climb eastward through the side canyons",
   at=[1180, 380], radius=520, bearing=70, grade=0.035, warp=5)
ev(-10000000, "landmass", "the flat-lying sediments, lifted and worn into a high desert; the reservoir now surrounds it",
   centre="centre", radius=900, roughness=0.8, hills=7, ridges=3)
ev(-6000000, "escarpment", "the lowest step: the desert bench stands above the old river flats in the south",
   **{"from": [1700, 1330], "to": [-160, 1320]}, height=32, width=14, reach=2000)
ev(-5000000, "escarpment", "the Red Cliffs, the dark red wall that crosses the whole country",
   **{"from": [1700, 505], "to": [-160, 490]}, height=36, width=10, reach=2000)
ev(-4000000, "escarpment", "the White Cliffs below the plateau forest",
   **{"from": [1700, 300], "to": [-160, 285]}, height=20, width=9, reach=2000)

# The reefs: the beds bent up along two folds and stripped back to tilted ridges,
# a long dip slope on one side and a ledgy scarp on the other, with a few cracks
# worn through the crest where a walker can get up.
ev(-3000000, "reef", "the Comb, a fold in the south-east where the beds bend up and the eroded edge stands as a tilted ridge",
   **{"from": [1000, 1060], "to": [1235, 1150]}, height=34, width=58, crest=0.55, scarp=9, cracks=3, crack_width=4.5,
   floor="wash_sand", poi="the Comb")
ev(-3000000, "reef", "a smaller fold west of the Gathering, its beds tilted up into a low ridge",
   **{"from": [140, 690], "to": [335, 805]}, height=22, width=44, crest=0.55, scarp=7, cracks=2, crack_width=4,
   floor="wash_sand", poi="the Little Reef")

sorrel = [[900, 110], [880, 250], [868, 420], [882, 540], [860, 700], [900, 900], [872, 1100], [920, 1300], [950, 1500]]
ev(-900000, "canyon", "the Sorrel River cutting south from the plateau springs to the big river",
   path=sorrel, width=18, depth=34, depth_end=22, wall=0.6, bench=8, meander=10, floor="wash_sand", poi="head of Sorrel Canyon")
ev(-899000, "river", "the Sorrel itself, a small river on the canyon floor",
   name="sorrel", path=[[900, 120]] + sorrel[1:], width=5, depth=1.2, bed="river_cobbles", water="river_water", meander=10)
ev(-880000, "canyon", "the frost amphitheatre cut back into the White Cliffs, where the hoodoos stand",
   path=[[420, 230], [425, 300], [430, 360]], width=70, depth=14, depth_end=4, wall=1.2, bench=6, meander=6, poi="the amphitheatre")

# Mesas, buttes and spires: the pieces of cliff the water hasn't taken yet.
ev(-600000, "mesa", "Harlow Mesa, a long remnant of the Red Cliffs in the south-west", at="harlow_mesa",
   radius=56, height=32, stretch=[90, 2.2], warp=0.25, tiers=3, dip=[205, 0.06], poi="Harlow Mesa")
ev(-600000, "mesa", "Sentinel Butte, east of the canyon", at=[1060, 930], radius=42, height=38, warp=0.2, tiers=2,
   dip=[125, 0.08], poi="Sentinel Butte")
ev(-600000, "mesa", "a smaller butte west of the canyon", at=[560, 1000], radius=30, height=30, warp=0.3, tiers=2,
   dip=[300, 0.07])
ev(-500000, "mesa", "the Totem, a spire left where a butte wore away", at=[1190, 760], radius=6, height=40,
   warp=0.12, talus=0.2, skirt=14, poi="the Totem")
ev(-500000, "mesa", "the Totem's smaller twin", at=[1222, 790], radius=4.5, height=28, warp=0.12, talus=0.2, skirt=10)
# Side canyons cutting back into the two cliff lines, so neither runs straight.
for x, head, length in [(150, 430, 70), (330, 405, 95), (520, 440, 60), (745, 420, 80), (1010, 410, 90), (1240, 435, 65), (1400, 420, 80)]:
    ev(-700000, "canyon", "a side canyon cut back into the Red Cliffs", path=[[x, head], [x + 8, head + length * 0.6], [x + 4, 503]],
       width=10, depth=8, depth_end=1, wall=0.5, bench=8, meander=5)
for x, head in [(250, 215), (610, 205), (1230, 225)]:
    ev(-700000, "canyon", "a side canyon cut back into the White Cliffs", path=[[x, head], [x - 6, 255], [x, 298]],
       width=9, depth=6, depth_end=1, wall=0.5, bench=6, meander=4)
fins = [(975, 640, 3.4, 24, 8), (996, 655, 3.8, 28, 9), (1020, 660, 4.6, 34, 9), (1043, 648, 3.6, 30, 8),
        (1064, 664, 3.4, 26, 7), (1085, 670, 4.0, 28, 7), (1106, 655, 3.2, 22, 6)]
for x, z, r, h, k in fins:
    ev(-400000, "mesa", "a fin of pale sandstone, one of a row east of the canyon", at=[x, z], radius=r, height=h,
       stretch=[0, k], warp=0.2, talus=0.12, skirt=5, dip=[90, 0.05])
ev(-400000, "mesa", "a low bench of bare slickrock", at=[1150, 640], radius=30, height=3, warp=0.35, talus=0.7, skirt=10, cap="slickrock")
ev(-300000, "erode_rims", "frost and flash floods rounding off the lip of every cliff and cutting notches back into the tops along the joints",
   depth=3.5, reach=12, gullies=0.5, min_drop=5, joints=1.0)
for i, (x, z) in enumerate([(430, 1388), (462, 1372), (500, 1392), (404, 1366), (530, 1378), (470, 1405)]):
    ev(-300000, "deposit", "a mound of banded clay in the badlands", at=[x, z], radius=12 + 3 * (i % 3), thickness=4 + (i % 2) * 2,
       mat="chinle_mudstone")
ev(-300000, "region", "the banded clay badlands on the low flats", name="painted", at=[465, 1388], radius=62)
ev(-8000, "dunes", "sand blown off the old lake beaches and heaped into dunes in the south-west",
   at=[230, 1000], radius=130, spacing=46, height=9, wind_bearing=60, mat="coral_sand", name="dunes", poi="the Coral Dunes")

# ---- the volcano --------------------------------------------------------------
ev(-940, "volcano", "the cinder cone that opened on the plateau nine hundred years ago",
   at="cone", radius=110, height=26, crater_radius=14, crater_depth=6, flows=2)
ev(-939, "region", "the cinders the cone threw out", name="cinder", at="cone", radius=150)
ev(-938, "lava_tube", "lava that crusted over on the cone's west flank and drained, leaving the ice cave",
   path=[[990, 150], [962, 205], [985, 262]], radius=2.6, depth=5, skylights=2)

# ---- the canyon people ----------------------------------------------------------
ev(-800, "place", "the canyon people's granaries in an alcove high in the west wall", structure="cliff_granaries",
   at=[875, 790], bearing=90, poi="the granaries")

# ---- hoodoos ---------------------------------------------------------------------
ev(-50000, "spires", "frost and rain cutting the soft pink rock under the White Cliffs into hoodoos",
   structures=["hoodoo", "hoodoo_squat", "hoodoo_tall", "hoodoo_squat"], at=[428, 335], radius=55, count=28, spacing=8,
   poi="the hoodoos")
cx, cz = places["gathering"]
for k in range(11):
    a = k * 2 * math.pi / 11 + 0.3
    r = 22 + (3 if k % 3 == 0 else 0)
    ev(-50000, "place", "one of the eleven hoodoos standing in a ring on the open desert, far from any cliff",
       structure="hoodoo_squat" if k % 4 else "hoodoo", at=[round(cx + r * math.cos(a), 1), round(cz + r * math.sin(a), 1)],
       bearing=(k * 97) % 360, poi="the Gathering" if k == 0 else None)

# ---- arches ------------------------------------------------------------------------
ev(-20000, "place", "a great window worn through the tallest fin", structure="great_window", at=[1020, 652], bearing=90,
   poi="the Great Window")
ev(-20000, "place", "a smaller window in the last fin", structure="small_window", at=[1078, 672], bearing=90)
# More fins across arch country, each with an opening worn through it. A window
# is placed across its fin (bearing + 90), settled on the lowest ground under it.
arch_fins = [
    # x, z, fin bearing, radius, stretch, height, window, poi. Windows turn in
    # quarter turns only, so fins run within a few degrees of the compass points.
    (1150, 640, 95, 4.2, 4.6, 20, "window_lone", "the Lone Arch"),
    (1212, 702, 8, 3.8, 4.0, 18, "window_span", None),
    (1262, 612, 84, 3.5, 3.6, 22, "window_tall", "Needle Window"),
    (1100, 762, 176, 4.5, 4.4, 20, "window_twin", "the Spectacles"),
    (1178, 822, 4, 3.6, 3.8, 24, "window_high", None),
    (1008, 792, 172, 4.0, 4.0, 17, "window_span", None),
    (1292, 762, 96, 3.4, 3.4, 16, "window_tall", None),
    (958, 722, 2, 3.6, 4.2, 19, "window_high", None),
    # A few out on the open desert bench, far from arch country.
    (690, 880, 92, 3.8, 3.6, 16, "window_span", None),
    (360, 640, 3, 3.4, 3.4, 15, "window_tall", None),
    (760, 1150, 176, 4.0, 3.8, 18, "window_high", None),
    (1290, 900, 88, 3.6, 3.8, 17, "window_twin", None),
]
for x, z, b, r, k, h, win, name in arch_fins:
    ev(-400000, "mesa", "a sandstone fin standing out of the slickrock, the joints either side of it worn away", at=[x, z],
       radius=r, height=h, stretch=[b, k], warp=0.22, talus=0.22, skirt=7, dip=[(b + 70) % 360, 0.05])
    ev(-20000, "place", "an opening worn through the fin, frost wedging grains loose until the light came through",
       structure=win, at=[x, z], bearing=(b + 90) % 360, poi=name)
ev(-20000, "region", "arch country: the bench round the fins, where the sandstone weathers into arches, windows and potholes",
   name="arches", at=[1120, 680], radius=210)

# ---- Bitterwater ------------------------------------------------------------------
ev(-146, "place", "Bitterwater's stone chapel, now standing in the shallows", structure="chapel", at="bitterwater",
   bearing=200, age=63, slots={"wall": "red_stone", "floor": "red_stone", "roof": "corrugated_tin"}, poi="Bitterwater chapel")
ev(-146, "place", "a Bitterwater farmhouse", structure="settler_house", at=[770, 1398], bearing=180, age=63,
   slots={"wall": "red_stone", "roof": "corrugated_tin"})
ev(-146, "place", "a Bitterwater farmhouse", structure="settler_house", at=[828, 1404], bearing=170, age=63,
   slots={"wall": "red_stone", "roof": "corrugated_tin"})

# ---- Harlow -------------------------------------------------------------------
ev(-144, "place", "the stamp mill on the slope below the mesa", structure="stamp_mill", at=[515, 1240], bearing=180,
   poi="Harlow stamp mill")
ev(-144, "deposit", "the mill's tailings, spread in a fan below it", at=[532, 1278], radius=22, thickness=1.6, mat="tailings")
ev(-144, "place", "the hoisting works over Harlow's main shaft", structure="headframe", at=[380, 1246], bearing=180,
   poi="Harlow headframe")
ev(-144, "place", "the haulage adit into the mesa", structure="mine_portal", at=[330, 1232], bearing=180, poi="the Harlow adit")
ev(-144, "place", "Harlow's cemetery on the rise east of town", structure="cemetery", at=[570, 1300], bearing=270,
   poi="Harlow cemetery")
ev(-144, "settlement", "the silver camp at the foot of Harlow Mesa, 1882", name="harlow", at="harlow", radius=50, houses=11,
   buildings=["false_front", "miner_shack", "false_front_wide", "miner_shack", "stone_office", "miner_shack", "false_front"],
   extras=[{"structure": "water_tank", "near": "edge"}], path="path_dirt")
ev(-133, "abandon", "silver collapsed in 1893 and the town emptied in a year", settlement="harlow")
ev(-74, "place", "the uranium men's steel headframe over an old shaft, 1952", structure="steel_headframe", at=[296, 1262],
   bearing=180, age=62)
ev(-74, "place", "an ore cart left on its side", structure="ore_cart", at=[348, 1252], bearing=30, age=62)

# ---- the highway, the station and the course --------------------------------------

ev(-98, "canyon", "the side wash at the east end that the road takes down to the ferry",
   path=[[1380, 555], [1450, 562], [1530, 590]], width=18, depth=1, depth_end=34, wall=1.6, bench=4, meander=3, floor="wash_sand")
ev(-97, "road", "the highway along the foot of the Red Cliffs, finished in 1930 (the bridge went in first)",
   path=[[22, 578], [100, 560], [300, 547], [600, 545], [882, 540], [1150, 548], [1380, 555], [1455, 563], [1515, 586]],
   width=8, mat="asphalt", shoulder="road_gravel", smooth=40)
ev(-97, "place", "the steel arch bridge over Sorrel Canyon, 1929", structure="arch_bridge", at=[880, 540], bearing=0,
   poi="Sorrel Canyon bridge")
ev(-44, "canyon", "the 1983 flash flood that cut the west wash back 200 m in a night and took the highway's last stretch to the lake with it",
   path=[[222, 549], [150, 556], [90, 562], [20, 580]], width=18, depth=11, depth_end=34, wall=0.9, bench=5, meander=4,
   floor="wash_sand", poi="the washout")
ev(-43, "place", "the barrier the highway department put up where the road now ends in air", structure="road_barrier",
   at=[236, 548], bearing=270, age=42, poi="the end of the road")
ev(-63, "place", "the ferry landing at the east end of the highway", structure="ferry_landing", at=[1478, 578], bearing=110,
   age=4, poi="the ferry landing")
ev(-72, "place", "Pruitt's filling station, 1954", structure="filling_station", at="station", bearing=0, age=2,
   poi="Pruitt's filling station")
ev(-72, "place", "the station's sign", structure="station_sign", at=[612, 553], bearing=0, age=2)
ev(-72, "place", "the station's water tank", structure="water_tank", at=[672, 582], bearing=0, age=2)
course = [[560, 615], [640, 610], [700, 613], [732, 628], [736, 655], [712, 668], [650, 664], [612, 668], [592, 690],
          [560, 698], [534, 680], [532, 644]]
ev(-34, "track", "the Pruitts' son bulldozed a dirt bike course behind the station, 1992",
   path=course, width=6, mat="packed_dirt", poi="Pruitt's bike course")
ev(-34, "place", "the course's start gate", structure="start_gate", at=[600, 606], bearing=0, age=6)
for x, z in [(742, 640), (720, 676), (528, 690), (524, 636), (600, 682)]:
    ev(-34, "place", "tyres stacked on the outside of a turn", structure="tyre_stack", at=[x, z], bearing=0, age=6)

# ---- fire -------------------------------------------------------------------------
for x, z, r in [(520, 120, 60), (760, 220, 50), (330, 230, 45), (980, 80, 40)]:
    ev(-5000, "moisture", "a spring-wet park on the plateau where the pines give way to grass", at=[x, z], radius=r, amount=0.5)
ev(-28, "fire", "the 1998 fire that burned a long strip of plateau forest west of the cone",
   at=[640, 170], radius=110, wind_bearing=255, stretch=0.7, char="charcoal")

E = [{k: v for k, v in e.items() if v is not None} for e in E]
out = {"places": places, "events": E}
here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "chronicle.json"), "w", encoding="utf-8", newline="\n") as fh:
    fh.write("{\n  \"places\": " + json.dumps(places) + ",\n  \"events\": [\n")
    fh.write(",\n".join("    " + json.dumps(e) for e in E))
    fh.write("\n  ]\n}\n")
print(len(E), "events")
