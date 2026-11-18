"""Writes chronicle.json for Lowmouth. The chronicle is data; this script only
saves typing for the repeated pieces (the stilt village's boardwalks and
houses, the regions). See the note below.

Coordinates in this script are in the first 1536 m frame. The world was later
trimmed to 1384 m by cutting 152 m off the west and the south; `X0` is
subtracted from every x on writing, and the south edge is now z = 1384.

One-shot, not kept current. This ran once, when the world was made; the
chronicle has moved on since (hand edits, new events and fields), so rerunning
it would overwrite live data with a stale version. It is kept as a record of
how the world was first written. Edit chronicle.json directly instead, and if
you do rerun this, diff the output before keeping any of it.
"""
import json
import os

X0 = 152
SIZE = 1384

places = {
    "centre": [768, 760],
    "headwater": [1010, 170],
    "breach": [905, 360],
    "old_lobe": [400, 1110],
    "swamp": [380, 560],
    "swamp_west": [270, 760],
    "tremblante": [830, 930],
    "sawgrass": [1210, 560],
    "mouth": [1150, 1190],
    "lowmouth": [640, 700],
    "parish": [440, 1030],
    "crossroads": [600, 250],
    "landing": [884, 300],
    "light": [700, 1298],
}

E = []
FORT = [200, 930]


def ev(year, kind, cause, **kw):
    E.append({"year": year, "event": kind, "cause": cause, **kw})


# ---- the land -------------------------------------------------------------------
ev(-6000, "landmass", "the delta plain the river built out into the Gulf, mud on mud",
   centre="centre", radius=960, roughness=0.35, hills=2.5, ridges=0, shelf=1.4, edge_fall=60)
ev(-4000, "lake", "Sorrow Lake, the old oxbow the Marrow drains out of on its way to the sea",
   name="sorrow", at="headwater", radius=70, depth=2.4, bed="river_mud", bank="river_mud", ragged=0.35, stretch=1.6, bearing=60)
ev(-3000, "river", "the Marrow on its old course, south-west to the sea past the parish",
   name="marrow", path=[[1010, 170], [960, 260], [905, 360], [760, 560], [640, 700], [520, 880], [430, 1060], [350, 1280], [325, 1382]],
   width=18, depth=3.2, bed="river_mud", bank="river_mud", meander=16, floor=2.6)
ev(-110, "avulsion", "the flood of the old century: the Marrow broke its bank above the parish and took the short way south-east",
   river="marrow", breach="breach", path=[[905, 360], [1000, 520], [1090, 640], [1100, 800], [1180, 960], [1210, 1150], [1260, 1330], [1272, 1382]],
   old_bed="river_mud", silt="silt", silt_thick=0.8)
ev(-90, "subside", "the old lobe, starved of silt, settling into the sea: the parish drowned with it",
   at="old_lobe", radius=280, stretch=1.35, bearing=25, level=[1.1, 2.0], islands=0.05, island_size=40, ragged=0.35)
ev(-85, "subside", "the back swamp behind the old channel sinking into black water among the cypress",
   at="swamp", radius=230, stretch=1.5, bearing=110, level=[0.5, 1.4], islands=0.4, island_size=34, ragged=0.4)
ev(-85, "subside", "the west swamp, the same sinking reaching out toward the sound",
   at="swamp_west", radius=150, stretch=1.3, bearing=170, level=[0.6, 1.4], islands=0.38, island_size=30, ragged=0.4)
ev(-75, "subside", "the trembling prairie between the rivers, a floating marsh over water a hand deep",
   at="tremblante", radius=120, stretch=1.4, bearing=60, level=[0.1, 0.45], islands=0.3, island_size=26, ragged=0.45)
ev(-70, "subside", "the flats east of the new river letting down into a sheet of sawgrass and shallow water",
   at="sawgrass", radius=210, stretch=1.5, bearing=10, level=[0.35, 0.7], islands=0.36, island_size=50, grain=90, ragged=0.4)
ev(-40, "subside", "the new mouth, where the river's silt meets the Gulf and the mangroves hold it",
   at="mouth", radius=250, stretch=1.2, bearing=150, level=[0.4, 1.5], islands=0.42, island_size=30, ragged=0.4)
ev(-30, "subside", "the Gulf eating at the south shore: the chenier's old point gone under, open water to the edge",
   at=[760, 1412], radius=82, stretch=7, bearing=90, level=[1.4, 2.6], edge=40, ragged=0.2)
ev(-60, "bayou", "Bayou Lune, an old crevasse channel from the trembling prairie down to the bay",
   path=[[830, 930], [720, 960], [610, 1010], [520, 1040]], width=9, depth=1.6, bank=6, meander=10)
ev(-60, "bayou", "Bayou Grue, a slough draining the sawgrass south into the new mouth",
   path=[[1200, 760], [1240, 880], [1230, 1000]], width=8, depth=1.5, bank=6, meander=10)
ev(-60, "bayou", "Bayou Noir, the swamp's back door to the sound in the north-west",
   path=[[300, 470], [240, 400], [190, 330], [160, 300]], width=8, depth=1.6, bank=6, meander=12)

# ---- regions ----------------------------------------------------------------------
def region(name, at, radius, cause):
    ev(-30, "region", cause, name=name, at=at, radius=radius)


for at, r in (([1100, 300], 160), ([860, 290], 150), ([650, 240], 140), ([420, 230], 140), ([250, 250], 100)):
    region("levee", at, r, "the natural levee along the Marrow and the old high ground of the north, where the road runs")
region("cane", [300, 165], 135, "Mire's cane, planted on the high ground behind the levee")
region("cane", [480, 140], 85, "Mire's cane, the east block")
region("burnt", [660, 140], 75, "the cane field burned off this autumn, the stubble left black")
region("swamp", [380, 560], 270, "the back swamp")
region("swamp", [270, 760], 170, "the west swamp")
region("swamp", [560, 600], 110, "the swamp's edge along the old channel")
region("parish", [420, 1090], 270, "the bay over the drowned parish")
region("ghost", [300, 880], 95, "the ghost forest, where the bay's salt reached the swamp")
region("ghost", [450, 900], 60, "the ghost forest's east end")
region("lowmouth", [625, 690], 130, "Lowmouth, the stilt village on the old channel")
region("sawgrass", [1220, 560], 260, "the sawgrass flats")
region("moss", [830, 930], 140, "the trembling prairie, a floating marsh like the old country's mosses, with the marsh lights over it")
region("mouth", [1150, 1200], 300, "the new mouth and its mangrove keys")
region("chenier", [720, 1290], 240, "the chenier, an old shell ridge on the south point")
region("chenier", [650, 1300], 90, "the chenier's point")

# ---- roads ------------------------------------------------------------------------
def path(cause, pts, width=4, mat="oyster_shell"):
    for a, b in zip(pts, pts[1:]):
        ev(-60, "path", cause, **{"from": a, "to": b}, width=width, mat=mat)


path("the levee road, shell over mud, from the west fields to the landing", [[170, 236], [360, 246], [600, 250], [760, 268], [870, 292]])
path("the parish road, south from the cane to Lowmouth", [[600, 60], [600, 250], [618, 420], [640, 560], [640, 640]], width=3, mat="packed_dirt")

# ---- the levee and the crossroads -------------------------------------------------
ev(-120, "place", "the sugar house, run until the cane went to the big mill up the river", structure="sugar_mill_ruin",
   at=[790, 220], bearing=180, age=70, poi="the old sugar house")
ev(-120, "place", "the quarters behind the sugar house, roofless since the mill closed", structure="brick_quarters",
   at=[796, 262], bearing=180, age=70)
ev(-120, "place", "the sugar house's rain cistern", structure="brick_cistern", at=[760, 206], bearing=0, age=70)
ev(-120, "place", "the last wall of the boiling shed", structure="brick_wall_ruin", at=[822, 214], bearing=270, age=80)
ev(-120, "place", "the last wall of the cooper's shop", structure="brick_wall_ruin", at=[766, 244], bearing=90, age=80)
ev(-50, "place", "the pump house the parish built to drain the cane after the flood", structure="pump_station",
   at=[862, 352], bearing=270, age=20, poi="the pump house")
ev(-40, "place", "Theriot's bait shop at the landing, where everyone puts in", structure="bait_shop",
   at=[868, 284], face=[906, 296], age=4, poi="Theriot's landing")
ev(-200, "place", "the finger post where the levee road crosses the parish road", structure="crossroads_post",
   at="crossroads", bearing=0, poi="the crossroads")
ev(-1, "place", "what is left at the crossroads in the night", structure="crossroads_offering",
   at="crossroads", bearing=0, when={"hours": [21, 5]})
ev(-1, "place", "the man at the crossroads, seen from the road after dark and never there when you arrive",
   structure="crossroads_figure", at=[588, 238], face=[620, 266], when={"hours": [19, 5.5], "far": 70})
ev(-60, "place", "the juke joint that stands on the corner lot between midnight and three, and is an empty lot by day",
   structure="juke_joint", at=[634, 222], face="crossroads", when={"hours": [0, 3]}, poi="the empty lot")
for k, (x, z) in enumerate([(592, 205), (609, 178), (591, 150), (609, 122), (592, 94), (608, 70)]):
    ev(-1, "place", "a woven stick lattice staked by the path through the burned cane", structure="stick_lattice_tall" if k % 3 == 1 else "stick_lattice",
       at=[x, z], face=[600, z])
for (x, z, f) in [(588, 264, (600, 250)), (614, 266, (600, 250)), (452, 104, (400, 170)), (250, 96, (250, 140)), (330, 270, (330, 240))]:
    ev(-1, "place", "a woven stick lattice where a cane trail meets another", structure="stick_lattice", at=[x, z], face=list(f))

# ---- Mire, the cane town --------------------------------------------------------
ev(-100, "settlement", "Mire, the cane town that grew round old Mire's fields after the flood spared them",
   name="mire", at=[400, 170], radius=95, houses=24,
   buildings=["cane_store", "cane_cottage", "cane_cottage_b", "cane_cottage", "cane_cottage", "cane_cottage_b", "cane_cottage", "cane_cottage_b", "cane_cottage",
              "cane_cottage_b", "cane_cottage", "cane_cottage", "cane_cottage_b", "cane_cottage", "cane_cottage_b", "cane_cottage", "cane_cottage",
              "cane_cottage_b", "cane_cottage", "cane_cottage_b", "cane_cottage", "cane_cottage", "cane_cottage_b", "cane_cottage"],
   path="packed_dirt")
ev(-60, "place", "the filling station where the town lane meets the levee road", structure="filling_station",
   at=[404, 254], face=[404, 240], age=6, poi="Mire filling station")
path("the lane from Mire down to the levee road", [[400, 170], [402, 215], [404, 246]], width=4, mat="packed_dirt")
path("the cane trails, out from Mire through the fields and round them",
     [[400, 170], [330, 150], [260, 120], [200, 150], [190, 210], [250, 240], [330, 220], [400, 170]], width=2, mat="packed_dirt")
path("the north trail into the cane", [[400, 170], [420, 110], [470, 80], [540, 110], [560, 160], [520, 200], [460, 190], [400, 170]], width=2, mat="packed_dirt")
path("the cross trail", [[260, 120], [300, 70], [380, 80], [420, 110]], width=2, mat="packed_dirt")
path("the cut through the cane to the crossroads", [[540, 110], [600, 120]], width=2, mat="packed_dirt")
path("the west loop through the cane", [[190, 210], [168, 170], [190, 110], [260, 120]], width=2, mat="packed_dirt")
path("the back trail along the north of the fields", [[190, 110], [240, 60], [330, 40], [420, 50], [470, 80]], width=2, mat="packed_dirt")
path("the middle trail, Mire to the levee through the cane", [[330, 150], [320, 200], [300, 236]], width=2, mat="packed_dirt")
path("the trail through the burned field to the sugar house", [[600, 120], [640, 100], [700, 120], [720, 180], [760, 206]], width=2, mat="packed_dirt")
path("the burned field's cross trail", [[620, 180], [660, 150], [690, 96]], width=2, mat="packed_dirt")

# ---- Lowmouth: boardwalks over the water, houses on posts ----------------------
WALK = 8.0


def walk_line(cause, a, b):
    """Boardwalk segments from a to b along x or z (8 m each)."""
    (x0, z0), (x1, z1) = a, b
    if z0 == z1:
        n = int(round(abs(x1 - x0) / WALK))
        step = WALK if x1 > x0 else -WALK
        bearing = 270 if x1 > x0 else 90
        for i in range(n):
            ev(-70, "place", cause, structure="stilt_walkway", at=[x0 + i * step, z0], bearing=bearing)
    else:
        n = int(round(abs(z1 - z0) / WALK))
        step = WALK if z1 > z0 else -WALK
        bearing = 0 if z1 > z0 else 180
        for i in range(n):
            ev(-70, "place", cause, structure="stilt_walkway", at=[x0, z0 + i * step], bearing=bearing)


# The old channel's line through the village, so houses keep out of it.
CHANNEL = [(730, 612), (700, 638), (668, 646), (654, 660), (652, 690), (640, 708), (626, 722), (620, 742), (598, 758), (576, 776), (576, 812)]


def seg_dist(p, a, b):
    (px, pz), (ax, az), (bx, bz) = p, a, b
    dx, dz = bx - ax, bz - az
    t = max(0.0, min(1.0, ((px - ax) * dx + (pz - az) * dz) / max(dx * dx + dz * dz, 1e-6)))
    return ((px - ax - dx * t) ** 2 + (pz - az - dz * t) ** 2) ** 0.5


WALKS = [
    ("Front Street, the boardwalk across Lowmouth, west side", (516, 700), (628, 700)),
    ("Front Street, east side", (644, 700), (716, 700)),
    ("North Walk, along the top of the village", (524, 644), (652, 644)),
    ("Top Walk, out to the north bridge", (640, 616), (712, 616)),
    ("Top Walk, east of the north bridge", (728, 616), (752, 616)),
    ("the steps between North Walk and Top Walk", (640, 616), (640, 644)),
    ("West Walk, down the swamp side of the village", (540, 620), (540, 764)),
    ("Mid Walk, between North Walk and Front Street", (572, 644), (572, 700)),
    ("Back Walk, down the west side to the camps", (596, 704), (596, 760)),
    ("South Walk, along the bottom of the village", (540, 760), (596, 760)),
    ("Back Walk, east of the little bridge", (616, 744), (640, 744)),
    ("Church Walk, north to the old landing", (676, 696), (676, 648)),
    ("Bank Walk, along the east bank", (700, 704), (700, 760)),
]
BRIDGES = [
    ("the Big Bridge over the old channel, high enough to paddle under", (628, 700), "the Big Bridge"),
    ("the little bridge on the back walk", (600, 744), None),
    ("the north bridge, over the channel at the top of the village", (712, 616), "the north bridge"),
]
for cause, a, b in WALKS:
    walk_line(cause, a, b)
for cause, at, poi in BRIDGES:
    kw = {"poi": poi} if poi else {}
    ev(-70, "place", cause, structure="stilt_footbridge", at=list(at), bearing=270, **kw)

houses = [
    ("stilt_house", [588, 692], 180), ("stilt_house", [612, 692], 180), ("stilt_camp", [656, 693], 180),
    ("stilt_house", [688, 692], 180), ("stilt_house", [604, 712], 0), ("stilt_camp", [660, 709], 0),
    ("stilt_house", [690, 711], 0), ("stilt_camp", [586, 730], 90), ("stilt_house", [586, 752], 90),
    ("stilt_camp", [668, 662], 90), ("stilt_house", [688, 664], 270), ("stilt_camp", [630, 754], 0),
]
# More houses down both sides of every walk, every 16 m or so, clear of the
# channel, the bridges, the other walks and each other.
taken = [tuple(h[1]) for h in houses] + [(650, 676), (636, 724), (650, 650)]
k = 0
for cause, a, b in WALKS:
    (x0, z0), (x1, z1) = a, b
    length = abs(x1 - x0) + abs(z1 - z0)
    along_x = z0 == z1
    t = 7.0
    while t < length - 3:
        side = 1 if k % 2 == 0 else -1
        k += 1
        if along_x:
            x = x0 + t * (1 if x1 > x0 else -1)
            pos = (x, z0 + 9 * side)
            bearing = 180 if side < 0 else 0
        else:
            z = z0 + t * (1 if z1 > z0 else -1)
            pos = (x0 + 9 * side, z)
            bearing = 90 if side < 0 else 270
        t += 16.0
        if min(seg_dist(pos, c, d) for c, d in zip(CHANNEL, CHANNEL[1:])) < 13:
            continue
        if any(seg_dist(pos, at, (at[0] + 16, at[1])) < 12 for _, at, _ in BRIDGES):
            continue
        if any(seg_dist(pos, c, d) < 6.5 for _, c, d in WALKS):
            continue
        if any(((pos[0] - q[0]) ** 2 + (pos[1] - q[1]) ** 2) ** 0.5 < 12 for q in taken):
            continue
        taken.append(pos)
        houses.append(("stilt_camp" if k % 3 == 0 else "stilt_house", [round(pos[0]), round(pos[1])], bearing))

walls = ["siding_teal", "siding_cream", "siding_sage", "siding_rust", "siding_slate", "siding_grey"]
doors = ["door_blue", "door_red", "door_green", "door_yellow", "door_blue", "door_black"]
for k, (st, at, b) in enumerate(houses):
    ev(-70, "place", "a house in Lowmouth, built on posts when the ground went under", structure=st, at=at, bearing=b,
       slots={"wall": walls[k % len(walls)], "door": doors[(k * 2) % len(doors)]}, age=6 + (k * 7) % 30)
ev(-50, "place", "the net shed at the channel's bend", structure="net_shed", at=[650, 676], bearing=270, age=12)
ev(-50, "place", "the net shed on West Walk", structure="net_shed", at=[530, 700], bearing=90, age=20)
ev(-20, "place", "the Guidrys' houseboat, tied up in the channel", structure="houseboat", at=[636, 724], bearing=180, age=5)
ev(-20, "place", "a houseboat at the top of the village", structure="houseboat", at=[650, 650], bearing=0, age=15)
ev(-20, "place", "a houseboat below the village", structure="houseboat", at=[590, 778], bearing=200, age=9)
ev(-20, "place", "a houseboat by the north bridge", structure="houseboat", at=[704, 630], bearing=330, age=22)
path("the path from Lowmouth's east bank up to the mound grounds", [[700, 700], [760, 704], [790, 760]], width=2, mat="packed_dirt")

# ---- the drowned parish -----------------------------------------------------------
ev(-180, "place", "St. Anne of the Marrow, the parish church, its floor now a metre under the bay", structure="drowned_church",
   at=[430, 1010], bearing=0, age=60, poi="St. Anne's")
for k, (x, z, b) in enumerate([(380, 980, 90), (470, 975, 270), (395, 1060, 0), (480, 1050, 180), (440, 1110, 0),
                               (350, 1022, 90), (362, 1100, 0), (410, 948, 180), (502, 1002, 270), (520, 1062, 270),
                               (472, 1140, 0), (402, 1150, 0), (330, 962, 90), (512, 950, 180), (420, 1190, 0)]):
    ev(-180, "place", "a parish house that went down with the old lobe", structure="sunken_house", at=[x, z], bearing=b,
       height=-2.5, age=90)
    ev(-1, "place", "lamplight in the windows of a drowned house, after dark", structure="drowned_lights", at=[x, z], bearing=b,
       height=-2.5, when={"hours": [19.5, 5]})
ev(-150, "place", "the parish tombs, the bay over them now", structure="sunken_tombs", at=[455, 1015], bearing=90, age=90)
ev(-110, "place", "the Belle Marrow, the steamer lost in the flood of the old century, seen on the bay at night from far off",
   structure="ghost_steamboat", at=[300, 1180], bearing=20, when={"hours": [20, 5], "far": 150})

# ---- the mounds -----------------------------------------------------------------
BX, BZ = 760, 640
ev(-3400, "earthwork", "the Bird Mound, heaped up basket by basket three thousand years ago by the people who lived on the old levee",
   parts=[{"from": [BX + 26, BZ], "to": [BX - 22, BZ], "width": 30, "top": 12},
          {"from": [BX - 22, BZ], "to": [BX - 44, BZ - 2], "width": 14, "top": 9.5},
          {"from": [BX + 2, BZ - 6], "to": [BX + 22, BZ - 70], "width": 20, "top": 8},
          {"from": [BX + 2, BZ + 6], "to": [BX + 22, BZ + 70], "width": 20, "top": 8},
          {"from": [BX + 22, BZ - 70], "to": [BX + 40, BZ - 84], "width": 12, "top": 6},
          {"from": [BX + 22, BZ + 70], "to": [BX + 40, BZ + 84], "width": 12, "top": 6},
          {"from": [BX + 26, BZ], "to": [BX + 58, BZ - 14], "width": 18, "top": 6.5},
          {"from": [BX + 26, BZ], "to": [BX + 58, BZ + 14], "width": 18, "top": 6.5},
          {"from": [BX + 26, BZ], "to": [BX + 62, BZ], "width": 18, "top": 7}],
   poi="the Bird Mound")
ev(-3300, "earthwork", "the half rings, low ridges the same people raised east of the mound, opening on a plaza toward it",
   arcs=[{"centre": [BX + 150, BZ], "radius": r, "from": 15, "to": 165, "width": 9, "top": 1.8} for r in (48, 66, 84, 102)], relative=True,
   poi="the half rings")
region("mounds", [BX + 60, BZ], 180, "the mound grounds")

# ---- trails ----------------------------------------------------------------------
path("the path from the sugar house to the pump house", [[790, 240], [820, 300], [862, 340]], width=2, mat="packed_dirt")
path("the levee path east of the landing", [[884, 300], [960, 280], [1040, 300], [1120, 300], [1200, 320]], width=2, mat="packed_dirt")
path("the old trail south from the landing through the woods to the mound", [[884, 310], [900, 400], [880, 480], [840, 560], [870, 560]],
     width=2, mat="packed_dirt")
path("the walk round the mound grounds", [[870, 560], [910, 640], [870, 730], [790, 760], [690, 720], [680, 640], [700, 560], [790, 530], [870, 560]],
     width=2, mat="packed_dirt")
path("the trail from the half rings to the plaza", [[910, 640], [1010, 640]], width=2, mat="packed_dirt")
path("the long trail south from the mounds to the chenier", [[790, 760], [800, 860], [790, 960], [760, 1060], [740, 1150], [720, 1230], [680, 1240]],
     width=2, mat="packed_dirt")
path("the chenier path, from the tombs to the light", [[640, 1240], [700, 1262], [800, 1282], [760, 1300], [700, 1298]], width=2, mat="oyster_shell")
path("a hog trail through the bottomland east of the river", [[960, 450], [1000, 560], [1040, 700], [1000, 820], [940, 900]], width=2, mat="packed_dirt")
path("the trail along the east hammocks", [[1400, 380], [1420, 520], [1410, 700], [1400, 880], [1390, 1040]], width=2, mat="packed_dirt")

# ---- more along the water --------------------------------------------------------
ev(-60, "place", "Mother Ruth's camp, deep in the swamp, where people bring what they can't take to the priest", structure="stilt_camp",
   at=[232, 640], bearing=90, age=20, slots={"wall": "siding_slate", "door": "door_blue"}, poi="Mother Ruth's camp")
for (x, z) in ((218, 626), (246, 628), (222, 660), (250, 656)):
    ev(-40, "place", "a bottle tree round Mother Ruth's camp", structure="bottle_tree", at=[x, z], bearing=(x * 7) % 360)
ev(-5, "place", "a lattice on the water path to Mother Ruth's", structure="stick_lattice_tall", at=[270, 640], face=[300, 640])
ev(-25, "place", "the Bon Temps well, a small platform out in the Gulf, pumping still", structure="oil_platform",
   at=[1090, 1356], bearing=0, age=8, poi="the Bon Temps well")
ev(-220, "place", "Fort St. Jean, the brick fort that guarded the old pass to the parish", structure="fort_ruin",
   at=[FORT[0], FORT[1]], bearing=90, age=150, poi="Fort St. Jean")
for k, (x, z, b) in enumerate([(1150, 520, 0), (1270, 640, 90), (1300, 470, 180), (1180, 720, 270)]):
    ev(-10, "place", "a duck blind out in the sawgrass", structure="duck_blind", at=[x, z], bearing=b, age=5)
ev(-40, "place", "a cane barge sunk on the bank of the new river", structure="barge_wreck", at=[1172, 930], bearing=10, age=35,
   poi="the sunk barge")
ev(-30, "place", "a fishing camp on the mangrove keys", structure="stilt_camp", at=[1060, 1120], bearing=90, age=10)
ev(-30, "place", "a fishing camp at the mouth", structure="stilt_camp", at=[1330, 1250], bearing=270, age=14,
   slots={"wall": "siding_teal", "door": "door_yellow"})
ev(-30, "place", "the net shed at the mouth camp", structure="net_shed", at=[1314, 1272], bearing=270, age=14)
ev(-70, "place", "the Broussard place on the chenier, empty since the storm", structure="cane_cottage_b", at=[800, 1282], bearing=180, age=60,
   poi="the Broussard place")

# ---- the chenier and the mouth ---------------------------------------------------
ev(-120, "place", "the tombs on the chenier, where the parish buried its dead after the flood", structure="raised_tombs",
   at=[640, 1240], bearing=90, age=40, poi="the chenier tombs")
ev(-100, "place", "the light on the chenier's point", structure="lighthouse", at="light", bearing=0, age=10, poi="the light")
ev(-15, "place", "the Miss Odile, a shrimp boat that sank at her mooring in the mouth", structure="shrimp_boat_wreck",
   at=[1236, 1330], bearing=200, age=15, poi="the wreck")
ev(-80, "place", "the grandmother tree, the oldest bottle tree on the hammocks", structure="bottle_tree",
   at=[1260, 520], bearing=0, poi="the bottle tree")

POINT_KEYS = {"at", "face", "from", "to", "centre", "breach"}


def shift(v):
    return [v[0] - X0, v[1]]


def walk(d):
    for k, v in d.items():
        if k in POINT_KEYS and isinstance(v, list) and len(v) == 2 and all(isinstance(c, (int, float)) for c in v):
            d[k] = shift(v)
        elif k == "path" and isinstance(v, list):
            d[k] = [shift(q) for q in v]
        elif isinstance(v, dict):
            walk(v)
        elif isinstance(v, list):
            for q in v:
                if isinstance(q, dict):
                    walk(q)


# The land's events keep the seeds they had before events were added ahead of
# them (each event is seeded by its place in the year-sorted list).
SEEDS = {"the delta plain": 1, "Sorrow Lake": 2, "the Bird Mound": 3, "the half rings": 4, "the Marrow on its old": 5,
         "the flood of the old century": 21, "the old lobe": 25, "the back swamp": 26, "the west swamp, the same": 27,
         "the trembling prairie between": 29, "the flats east": 30, "Bayou Lune": 75, "Bayou Grue": 76, "Bayou Noir": 77,
         "the new mouth, where": 111}
for e in E:
    for start, k in SEEDS.items():
        if e["cause"].startswith(start) and e["event"] != "region":
            e["seed"] = k
for start in SEEDS:
    assert sum(e["cause"].startswith(start) and e["event"] != "region" for e in E) == 1, start
for e in E:
    walk(e)
    for k in ("at", "from", "to", "centre", "face"):
        v = e.get(k)
        if isinstance(v, list) and not (0 <= v[0] <= SIZE and 0 <= v[1] <= SIZE):
            print("off the map:", e["cause"][:60], v)
places = {k: shift(v) for k, v in places.items()}
out = {"places": places, "events": E}
here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "chronicle.json"), "w") as f:
    json.dump(out, f, indent=2)
print(len(E), "events")
