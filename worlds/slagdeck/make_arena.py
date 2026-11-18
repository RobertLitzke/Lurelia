#!/usr/bin/env python3
"""Write library/structures/slagdeck_arena.json: the whole arena as one
structure.

The map is a classic arena read as a foundry deck -- a hall round a slag pit, two
roofed side rooms with galleries over them, a walkway ring at six metres, a
raised platform over the north walkway, and corridors behind the hall joining
the rooms. Writing it by hand in quarter-metre voxels would be unreadable, so
this says it in metres and the script does the arithmetic.

Local frame: x east, z south, y up from the yard. The deck -- the floor you
play on -- is DECK metres up, on a solid plinth, which is how the pit gets to
be a pit without carving the terrain.

    python worlds/slagdeck/make_arena.py
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

V = 4  # voxels per metre

# Heights, metres above the yard.
DECK = 5.0  # the main floor
PIT = 1.5  # the pit floor
SLAG = 1.0  # the slag lies between PIT and here
WALK = 9.0  # the walkway ring and the galleries
LEDGE = 12.0  # the platform over the north walkway
TOP = 17.0  # the top of the outer wall

# The mesher draws a staircase as an even slope when it rises a quarter-metre
# per metre of run (mesh.rs, "Slopes"), so every ramp here is laid out from
# its rise at that grade rather than from a length picked by eye.
RUN = 4.0  # metres of run per metre of rise

# The footprint, metres: the outer wall runs round x 2..86, z 2..70.
X0, X1 = 2.0, 86.0
Z0, Z1 = 2.0, 70.0
WALL = 1.0

# Interior edges.
IX0, IX1 = X0 + WALL, X1 - WALL
IZ0, IZ1 = Z0 + WALL, Z1 - WALL

# The hall, and the two side rooms either side of it.
HX0, HX1 = 25.0, 63.0
HZ0, HZ1 = 11.0, 61.0
WROOM = (IX0, HX0, HZ0, 45.0)  # west room: roofed, gallery over it
EROOM = (HX1, IX1, 25.0, HZ1)  # east room: the same, the other way up
# The slag pit, in the middle of the hall.
PX0, PX1 = 35.0, 53.0
PZ0, PZ1 = 29.0, 47.0
# How wide a walkway is.
WIDE = 4.0

ops = []


def box(x0, x1, y0, y1, z0, z1, mat, part=None):
    """A box in metres; `to` is exclusive, as the shape ops want it."""
    op = {
        "op": "box",
        "from": [round(x0 * V), round(y0 * V), round(z0 * V)],
        "to": [round(x1 * V), round(y1 * V), round(z1 * V)],
        "mat": mat,
    }
    if part:
        op["part"] = part
    ops.append(op)


def slab(x0, x1, z0, z1, top, body="works_beam", face="works_grate", thick=1.0):
    """A floor whose top face is at `top`: a metre of structure under a
    quarter-metre of walking surface."""
    box(x0, x1, top - thick, top - 0.25, z0, z1, body, "floor")
    box(x0, x1, top - 0.25, top, z0, z1, face, "floor")


def ramp(x0, x1, z0, z1, lo, hi, towards, mat="deck_ramp"):
    """A ramp in quarter-metre steps, filled solid underneath. `towards` is
    the way it climbs: "+x", "-x", "+z" or "-z". The material is smoothed and
    the run is RUN metres per metre of rise, which is the grade the mesher
    draws as one clean slope."""
    steps = max(1, round((hi - lo) / 0.25))
    along = (x1 - x0) if towards in ("+x", "-x") else (z1 - z0)
    want = (hi - lo) * RUN
    assert abs(along - want) < 0.01, f"ramp of {hi - lo} m rise wants {want} m of run, got {along}"
    for i in range(steps):
        a = along * i / steps
        b = along * (i + 1) / steps
        y = lo + 0.25 * (i + 1)
        if towards == "+x":
            box(x0 + a, x0 + b, lo - 0.5, y, z0, z1, mat, "floor")
        elif towards == "-x":
            box(x1 - b, x1 - a, lo - 0.5, y, z0, z1, mat, "floor")
        elif towards == "+z":
            box(x0, x1, lo - 0.5, y, z0 + a, z0 + b, mat, "floor")
        else:
            box(x0, x1, lo - 0.5, y, z1 - b, z1 - a, mat, "floor")


def stripe(x0, x1, z0, z1, top):
    """Warning paint on the top face of an edge."""
    box(x0, x1, top - 0.25, top, z0, z1, "hazard_stripe", "floor")


def air(x0, x1, y0, y1, z0, z1):
    box(x0, x1, y0, y1, z0, z1, "air")


LAMPS = []


def lamp(x, z, hang, under, reach=16.0, strength=2.0, span=1.5):
    """A fitting hung from `under` down to `hang`, and the light it gives.
    The fitting is voxels in the structure; the light is a point in the
    chronicle, because light is not a thing you can build out of blocks."""
    box(x - 0.125, x + 0.125, hang + 0.25, under, z - 0.125, z + 0.125, "works_beam", "metal")
    box(x - span * 0.5, x + span * 0.5, hang, hang + 0.25, z - 0.25, z + 0.25, "works_lamp", "glass")
    LAMPS.append((round(x, 2), round(z, 2), round(hang - 0.4, 2), reach, strength))


# ---- the plinth and the deck ------------------------------------------------
box(X0, X1, 0.0, DECK - 0.25, Z0, Z1, "concrete", "stone")
box(X0, X1, DECK - 0.25, DECK, Z0, Z1, "deck_floor", "floor")

# ---- the outer wall ---------------------------------------------------------
for a, b, c, d in [
    (X0, X1, Z0, Z0 + WALL),
    (X0, X1, Z1 - WALL, Z1),
    (X0, X0 + WALL, Z0, Z1),
    (X1 - WALL, X1, Z0, Z1),
]:
    box(a, b, DECK, TOP - 2.0, c, d, "works_panel_pale", "wall")
    # Galvanised sheet along the top two metres, as the works would have it.
    box(a, b, TOP - 2.0, TOP, c, d, "corrugated_tin", "wall")

# ---- partitions -------------------------------------------------------------
# Hall against the two side rooms, full height.
box(HX0 - WALL, HX0, DECK, TOP, HZ0, HZ1, "works_panel", "wall")
box(HX1, HX1 + WALL, DECK, TOP, HZ0, HZ1, "works_panel", "wall")
# Hall against the corridors north and south, full height.
box(HX0 - WALL, HX1 + WALL, DECK, TOP, HZ0 - WALL, HZ0, "works_panel", "wall")
box(HX0 - WALL, HX1 + WALL, DECK, TOP, HZ1, HZ1 + WALL, "works_panel", "wall")
# The side rooms' own end walls, only as high as their roofs.
box(IX0, HX0, DECK, WALK, WROOM[3], WROOM[3] + WALL, "works_panel_pale", "wall")
box(HX1 + WALL, IX1, DECK, WALK, EROOM[2] - WALL, EROOM[2], "works_panel_pale", "wall")
# The north and south corridors are divided from the open alcoves beside them
# only by the hall's wall, so they run the width of the deck.

# ---- galleries round the side rooms -----------------------------------------
# A gallery is a ring four metres wide, not a roof: all the light in this
# world comes from the sky, and a room with a lid on it is pitch black and
# unplayable. The well in the middle also lets the two floors shoot at each
# other, which is the whole point of having two.
def gallery(x0, x1, z0, z1, top):
    slab(x0, x1, z0, z0 + WIDE, top)
    slab(x0, x1, z1 - WIDE, z1, top)
    slab(x0, x0 + WIDE, z0 + WIDE, z1 - WIDE, top)
    slab(x1 - WIDE, x1, z0 + WIDE, z1 - WIDE, top)
    for a, b, c, d2 in [
        (x0, x1, z0 + WIDE - 0.5, z0 + WIDE),
        (x0, x1, z1 - WIDE, z1 - WIDE + 0.5),
        (x0 + WIDE - 0.5, x0 + WIDE, z0 + WIDE, z1 - WIDE),
        (x1 - WIDE, x1 - WIDE + 0.5, z0 + WIDE, z1 - WIDE),
    ]:
        stripe(a, b, c, d2, top)


gallery(WROOM[0], WROOM[1], WROOM[2], WROOM[3], WALK)
gallery(EROOM[0] + WALL, EROOM[1], EROOM[2], EROOM[3], WALK)

# ---- the walkway ring inside the hall ---------------------------------------
slab(HX0, HX1, HZ0, HZ0 + WIDE, WALK)  # north
slab(HX0, HX1, HZ1 - WIDE, HZ1, WALK)  # south
slab(HX0, HX0 + WIDE, HZ0 + WIDE, HZ1 - WIDE, WALK)  # west
slab(HX1 - WIDE, HX1, HZ0 + WIDE, HZ1 - WIDE, WALK)  # east

# ---- the platform over the north walkway, and the ramps up to it ------------
LX0, LX1 = 37.0, 51.0
ramp(LX0 - 12.0, LX0, HZ0, HZ0 + WIDE, WALK, LEDGE, "+x")
slab(LX0, LX1, HZ0, HZ0 + WIDE, LEDGE, face="deck_floor")
ramp(LX1, LX1 + 12.0, HZ0, HZ0 + WIDE, WALK, LEDGE, "-x")
# Two columns under it, so it is held up by something.
for x in (LX0 + 1.0, LX1 - 2.0):
    box(x, x + 1.0, WALK, LEDGE - 1.0, HZ0 + 1.5, HZ0 + 2.5, "works_beam", "metal")

# ---- ramps from the hall floor up to the ring -------------------------------
ramp(HX0 + WIDE, HX0 + WIDE + 16.0, 50.0, 54.0, DECK, WALK, "-x")
ramp(HX1 - WIDE - 16.0, HX1 - WIDE, 22.0, 26.0, DECK, WALK, "+x")
# Up to the west gallery out of the open sump south of the west room, and up
# to the east one out of the alcove north of the east room: a ramp inside a
# roofed room could never reach the roof.
ramp(8.0, 12.0, WROOM[3], WROOM[3] + 16.0, DECK, WALK, "-z")
ramp(74.0, 78.0, EROOM[2] - 16.0, EROOM[2], DECK, WALK, "+z")

# ---- the pit ----------------------------------------------------------------
air(PX0, PX1, PIT, DECK, PZ0, PZ1)
# Line its walls, so the pit is a tapping floor and not a hole in a slab.
box(PX0, PX1, SLAG, DECK, PZ0, PZ0 + 0.25, "works_beam", "wall")
box(PX0, PX1, SLAG, DECK, PZ1 - 0.25, PZ1, "works_beam", "wall")
box(PX0, PX0 + 0.25, SLAG, DECK, PZ0, PZ1, "works_beam", "wall")
box(PX1 - 0.25, PX1, SLAG, DECK, PZ0, PZ1, "works_beam", "wall")
box(PX0, PX1, SLAG, PIT, PZ0, PZ1, "slag_pool", "floor")
# A ramp out of it, up its west side to the south lip.
ramp(PX0, PX0 + WIDE, PZ1 - 14.0, PZ1, PIT, DECK, "+z")
stripe(PX0 - 0.5, PX1 + 0.5, PZ0 - 0.5, PZ0, DECK)
stripe(PX0 - 0.5, PX1 + 0.5, PZ1, PZ1 + 0.5, DECK)
stripe(PX0 - 0.5, PX0, PZ0, PZ1, DECK)
stripe(PX1, PX1 + 0.5, PZ0, PZ1, DECK)

# ---- pillars in the hall ----------------------------------------------------
for px in (31.0, 57.0):
    for pz in (20.0, 56.0):
        box(px, px + 1.0, DECK, TOP, pz, pz + 1.0, "works_beam", "metal")

# ---- pilasters and a rail down the hall's walls -----------------------------
# A flat grey wall forty metres long is nothing to read a room by; these give
# the eye something to measure the hall against without taking cover away.
for px in [HX0 + 2.0 + 8.0 * i for i in range(5)]:
    for pz, pz1 in [(HZ0 - WALL, HZ0), (HZ1, HZ1 + WALL)]:
        box(px, px + 0.5, DECK, TOP - 1.0, pz - 0.25, pz1 + 0.25, "works_beam", "metal")
for pz in [HZ0 + 3.0 + 8.0 * i for i in range(6)]:
    for px, px1 in [(HX0 - WALL, HX0), (HX1, HX1 + WALL)]:
        box(px - 0.25, px1 + 0.25, DECK, TOP - 1.0, pz, pz + 0.5, "works_beam", "metal")
for a, b, c, d2 in [
    (HX0 - WALL - 0.25, HX1 + WALL + 0.25, HZ0 - WALL - 0.25, HZ0),
    (HX0 - WALL - 0.25, HX1 + WALL + 0.25, HZ1, HZ1 + WALL + 0.25),
    (HX0 - WALL - 0.25, HX0, HZ0, HZ1),
    (HX1, HX1 + WALL + 0.25, HZ0, HZ1),
]:
    box(a, b, TOP - 4.5, TOP - 4.0, c, d2, "rust_iron", "metal")

# ---- roof trusses over the open hall ----------------------------------------
for tz in (14.0, 22.0, 30.0, 38.0, 46.0, 54.0):
    box(HX0, HX1, TOP - 1.0, TOP - 0.5, tz, tz + 0.5, "works_beam", "frame")
    box(HX0, HX1, TOP - 0.5, TOP, tz + 0.125, tz + 0.375, "rust_iron", "frame")

# ---- openings ---------------------------------------------------------------
# Hall to the west room: two doorways, and a long gallery window above.
air(HX0 - WALL, HX0, DECK, DECK + 4.0, 20.0, 28.0)
air(HX0 - WALL, HX0, DECK, DECK + 4.0, 36.0, 44.0)
air(HX0 - WALL, HX0, WALK, WALK + 4.0, 14.0, 42.0)
# Hall to the east room.
air(HX1, HX1 + WALL, DECK, DECK + 4.0, 30.0, 38.0)
air(HX1, HX1 + WALL, DECK, DECK + 4.0, 46.0, 54.0)
air(HX1, HX1 + WALL, WALK, WALK + 4.0, 28.0, 58.0)
# Hall to the north corridor: two doorways and a slot at walkway height.
air(28.0, 34.0, DECK, DECK + 4.0, HZ0 - WALL, HZ0)
air(54.0, 60.0, DECK, DECK + 4.0, HZ0 - WALL, HZ0)
air(HX0 + 8.0, HX1 - 8.0, WALK, WALK + 2.0, HZ0 - WALL, HZ0)
# Hall to the south corridor, the same.
air(28.0, 34.0, DECK, DECK + 4.0, HZ1, HZ1 + WALL)
air(54.0, 60.0, DECK, DECK + 4.0, HZ1, HZ1 + WALL)
air(HX0 + 8.0, HX1 - 8.0, WALK, WALK + 2.0, HZ1, HZ1 + WALL)
# West room to the sump south of it, and east room to the alcove north of it.
air(IX0 + 4.0, IX0 + 12.0, DECK, DECK + 4.0, WROOM[3], WROOM[3] + WALL)
air(IX1 - 12.0, IX1 - 4.0, DECK, DECK + 4.0, EROOM[2] - WALL, EROOM[2])
# Louvred vents where the windows were. An arena is a closed room: from
# inside it you should never see the yard, the sea or the horizon, because
# the moment you can, the map stops being a building and starts being a
# diorama standing in a landscape. So these are grating, not air -- they read
# as a works' wall vents, they let no light in and they let no view out.
# What the rooms lost in daylight the lamps below make up.
for wx0, wx1 in [(14.0, 26.0), (38.0, 50.0), (62.0, 74.0)]:
    box(wx0, wx1, TOP - 5.0, TOP - 2.0, Z0, Z0 + WALL, "works_grate", "wall")
    box(wx0, wx1, TOP - 5.0, TOP - 2.0, Z1 - WALL, Z1, "works_grate", "wall")
for wz in (16.0, 26.0, 36.0):
    box(X0, X0 + WALL, DECK + 1.0, WALK - 1.0, wz, wz + 6.0, "works_grate", "wall")
for wz in (30.0, 40.0, 50.0):
    box(X1 - WALL, X1, DECK + 1.0, WALK - 1.0, wz, wz + 6.0, "works_grate", "wall")

# ---- the roof ---------------------------------------------------------------
# Over the whole deck. Under it the hall is a room rather than a yard, and
# nothing outside the building can see in or shoot in.
box(X0, X1, TOP, TOP + 0.25, Z0, Z1, "works_beam", "roof")
box(X0, X1, TOP + 0.25, TOP + 0.5, Z0, Z1, "corrugated_tin", "roof")
# Monitor slots between the trusses, the way a works roof lets its fume out.
# Barred over, for the same reason the windows are: standing in the hall you
# should see the roof, not the sky through it.
for sz in (18.0, 26.0, 34.0, 42.0, 50.0):
    box(HX0 + 4.0, HX1 - 4.0, TOP, TOP + 1.5, sz, sz + 1.5, "works_grate", "roof")
    for kz in (sz - 0.5, sz + 1.5):
        box(HX0 + 4.0, HX1 - 4.0, TOP + 0.5, TOP + 1.25, kz, kz + 0.5, "works_panel", "wall")
    box(HX0 + 4.0, HX1 - 4.0, TOP + 1.25, TOP + 1.5, sz - 0.5, sz + 2.0, "corrugated_tin", "roof")

# ---- lamps ------------------------------------------------------------------
# High over the hall, between the trusses.
for lx in (31.0, 44.0, 57.0):
    for lz in (18.0, 30.0, 42.0, 54.0):
        lamp(lx, lz, TOP - 2.5, TOP, reach=24.0, strength=0.85, span=2.5)
# Under the galleries, which is where the two shops get all their light.
for lz in (16.0, 26.0, 36.0):
    lamp(WROOM[0] + 2.0, lz, WALK - 1.4, WALK - 1.0, reach=14.0, strength=1.15)
    lamp(WROOM[1] - 2.0, lz, WALK - 1.4, WALK - 1.0, reach=14.0, strength=1.15)
for lz in (30.0, 42.0, 54.0):
    lamp(EROOM[0] + WALL + 2.0, lz, WALK - 1.4, WALK - 1.0, reach=14.0, strength=1.15)
    lamp(EROOM[1] - 2.0, lz, WALK - 1.4, WALK - 1.0, reach=14.0, strength=1.15)
# Over the gallery rings themselves.
for lz in (16.0, 28.0, 40.0):
    lamp(WROOM[0] + 2.0, lz, WALK + 3.5, TOP, reach=14.0, strength=0.8)
for lz in (30.0, 42.0, 54.0):
    lamp(EROOM[1] - 2.0, lz, WALK + 3.5, TOP, reach=14.0, strength=0.8)
# Down both corridors.
for lx in (10.0, 26.0, 44.0, 62.0, 78.0):
    lamp(lx, HZ0 - WALL - 4.0, DECK + 4.0, TOP, reach=13.0, strength=0.95)
    lamp(lx, HZ1 + WALL + 4.0, DECK + 4.0, TOP, reach=13.0, strength=0.95)
# Over the pit, so the tapping floor is the brightest thing in the hall.
for lz in (PZ0 + 4.0, PZ1 - 4.0):
    lamp(PX0 - 2.0, lz, DECK + 4.5, TOP, reach=16.0, strength=1.05)
    lamp(PX1 + 2.0, lz, DECK + 4.5, TOP, reach=16.0, strength=1.05)

# ---- paint on every edge you can fall off -----------------------------------
for x0, x1, z0, z1 in [
    (HX0, HX1, HZ0 + WIDE - 0.5, HZ0 + WIDE),
    (HX0, HX1, HZ1 - WIDE, HZ1 - WIDE + 0.5),
    (HX0 + WIDE - 0.5, HX0 + WIDE, HZ0 + WIDE, HZ1 - WIDE),
    (HX1 - WIDE, HX1 - WIDE + 0.5, HZ0 + WIDE, HZ1 - WIDE),
]:
    stripe(x0, x1, z0, z1, WALK)

# ---- out --------------------------------------------------------------------
entry = {
    "name": "slagdeck_arena",
    "version": 1,
    "introduced_by": "slagdeck",
    "notes": (
        "The whole arena deck as one structure, written by "
        "worlds/slagdeck/make_arena.py: a hall round a slag pit, roofed side "
        "rooms with galleries over them, a walkway ring six metres up, a "
        "platform over the north walkway, and corridors behind the hall. "
        "Place it level with bearing 0; its anchor is the middle of the "
        "footprint at yard level. Edit the script, not this file."
    ),
    "size": [round(X1 * V) + 1, round(TOP * V) + 1, round(Z1 * V) + 1],
    "anchor": [round((X0 + X1) * 0.5 * V), 0, round((Z0 + Z1) * 0.5 * V)],
    "foundation": 4,
    "level": True,
    "decays": False,
    "tags": ["arena", "works"],
    "ops": ops,
}
path = os.path.join(ROOT, "library", "structures", "slagdeck_arena.json")
with open(path, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(libfmt.render(entry, "structures"))
print(f"{len(ops)} ops -> {path}")

# The lamps go into the chronicle, as offsets from the deck's own placement
# point. The game reads chronicle.json, so that stays the source; this only
# keeps it level with the fittings the script hangs them on.
AX, AZ = (X0 + X1) * 0.5, (Z0 + Z1) * 0.5
here = os.path.dirname(os.path.abspath(__file__))
cpath = os.path.join(here, "chronicle.json")
with open(cpath, encoding="utf-8") as fh:
    chron = json.load(fh)
groups = {}
for lx, lz, ly, reach, strength in LAMPS:
    groups.setdefault((reach, strength), []).append([round(lx - AX, 2), round(lz - AZ, 2), ly])
events = [e for e in chron["events"] if e.get("event") != "lamp"]
for (reach, strength), pts in sorted(groups.items()):
    events.append(
        {
            "year": -1,
            "event": "lamp",
            "cause": "The works lighting, still on, which is the other thing nobody here can explain.",
            "at": "deck",
            "points": pts,
            "reach": reach,
            "strength": strength,
            "colour": [255, 234, 196],
        }
    )
chron["events"] = events
with open(cpath, "w", encoding="utf-8", newline="\n") as fh:
    json.dump(chron, fh, indent=2)
    fh.write("\n")
print(f"{len(LAMPS)} lamps -> {cpath}")

# ---- the bot navigation graph -----------------------------------------------
# Where a bot may stand and how it gets between those places (src/brain/nav.rs).
# Hand-placed, because this is one small arena and the walkway ring, the ramps
# and the pit exit are nearly the whole graph; generating them is still to do.
#
# `up` is metres above the deck floor, not an absolute height: the world does
# not know what height the plinth ended up at, but it does know the walkway is
# four metres over the floor. The engine finds the real floor under each one.
#
# Keep them sparse: nav points closer than about 1.5 m make bots stick on
# them, and nothing here is within six.

UP_DECK = 0.0
UP_WALK = WALK - DECK
UP_LEDGE = LEDGE - DECK
UP_PIT = PIT - DECK

NAV = {}
NAV_ORDER = []


def node(name, x, z, up, cover=0.0, poi=None):
    assert name not in NAV, name
    for px in (31.0, 57.0):
        for pz in (20.0, 56.0):
            if abs(x - px - 0.5) < 2.0 and abs(z - pz - 0.5) < 2.0 and up == UP_DECK:
                raise AssertionError(f"{name} is standing in the pillar at {px},{pz}")
    for other, n in NAV.items():
        if abs(n[0] - x) < 1.5 and abs(n[1] - z) < 1.5 and abs(n[2] - up) < 1.0:
            raise AssertionError(f"{name} is on top of {other}")
    NAV[name] = (x, z, up, cover, poi)
    NAV_ORDER.append(name)


LINKS = []


def link(a, b, reach="walk"):
    assert a in NAV and b in NAV, (a, b)
    LINKS.append((a, b, reach))


def chain(*names, reach="walk"):
    for a, b in zip(names, names[1:]):
        link(a, b, reach)


# The hall floor, round the pit.
node("hall_nw", 33.0, 23.0, UP_DECK)
node("hall_ne", 54.0, 23.0, UP_DECK)
node("hall_sw", 33.0, 51.0, UP_DECK)
node("hall_se", 54.0, 51.0, UP_DECK)
node("hall_n", 38.0, 22.0, UP_DECK)
node("hall_s", 40.0, 52.0, UP_DECK, poi="pit")
node("hall_w", 31.0, 38.0, UP_DECK)
node("hall_e", 56.0, 38.0, UP_DECK)
chain("hall_nw", "hall_n", "hall_ne")
chain("hall_sw", "hall_s", "hall_se")
chain("hall_nw", "hall_w", "hall_sw")
chain("hall_ne", "hall_e", "hall_se")

# The feet of the two ramps from the hall floor up to the walkway ring.
node("ramp_w_foot", 46.0, 52.0, UP_DECK)
node("ramp_e_foot", 46.0, 24.0, UP_DECK)
link("hall_s", "ramp_w_foot")
link("hall_se", "ramp_w_foot")
link("hall_n", "ramp_e_foot")
link("hall_nw", "ramp_e_foot")

# The corridors behind the hall, north and south, running the whole width.
for tag, z in (("n", 6.0), ("s", 65.0)):
    for i, x in enumerate((10.0, 31.0, 44.0, 57.0, 78.0)):
        node(f"cor_{tag}{i}", x, z, UP_DECK, cover=0.35)
    chain(*[f"cor_{tag}{i}" for i in range(5)])
# Hall to corridor, through the doorways at x 28..34 and 54..60.
link("hall_nw", "cor_n1")
link("hall_ne", "cor_n3")
link("hall_sw", "cor_s1")
link("hall_se", "cor_s3")

# The west shop floor, and the open sump south of it.
node("west_n", 10.0, 16.0, UP_DECK, cover=0.3)
node("west_m", 10.0, 30.0, UP_DECK, cover=0.3, poi="west shop")
node("west_s", 10.0, 40.0, UP_DECK, cover=0.3)
node("west_in_n", 20.0, 24.0, UP_DECK)
node("west_in_s", 20.0, 40.0, UP_DECK)
chain("west_n", "west_m", "west_s")
chain("west_in_n", "west_in_s")
link("west_n", "west_in_n")
link("west_s", "west_in_s")
link("west_m", "west_in_n")
link("west_m", "west_in_s")
node("sump_ramp", 10.0, 52.0, UP_DECK)
node("sump_e", 18.0, 55.0, UP_DECK, cover=0.25)
chain("west_s", "sump_ramp", "sump_e")
link("sump_e", "cor_s0")
link("sump_ramp", "cor_s0")
# Hall to the west shop, through the two doorways at z 20..28 and 36..44.
link("hall_nw", "west_in_n")
link("hall_w", "west_in_s")
link("hall_sw", "west_in_s")
# The east shop floor, and the alcove north of it.
node("east_s", 76.0, 56.0, UP_DECK, cover=0.3)
node("east_m", 76.0, 42.0, UP_DECK, cover=0.3, poi="east shop")
node("east_n", 76.0, 32.0, UP_DECK, cover=0.3)
node("east_in_s", 68.0, 50.0, UP_DECK)
node("east_in_n", 68.0, 34.0, UP_DECK)
chain("east_s", "east_m", "east_n")
chain("east_in_s", "east_in_n")
link("east_s", "east_in_s")
link("east_n", "east_in_n")
link("east_m", "east_in_s")
link("east_m", "east_in_n")
node("alcove_ramp", 76.0, 14.0, UP_DECK)
node("alcove_w", 68.0, 16.0, UP_DECK, cover=0.25)
chain("east_n", "alcove_ramp", "alcove_w")
link("alcove_w", "cor_n4")
link("alcove_ramp", "cor_n4")
# Hall to the east shop, through the doorways at z 30..38 and 46..54.
link("hall_e", "east_in_n")
link("hall_ne", "east_in_n")
link("hall_se", "east_in_s")

# The pit. A bot goes down the ramp on its west side and out to the south lip,
# or steps off the edge and cannot climb back out that way.
node("pit_floor", 44.0, 36.0, UP_PIT, cover=0.5)
node("pit_ramp", 37.0, 42.0, UP_PIT)
link("pit_floor", "pit_ramp")
link("pit_ramp", "hall_s")
# Falling in: one way, from each lip.
for lip in ("hall_n", "hall_s", "hall_w", "hall_e"):
    link(lip, "pit_floor", "drop")

# The walkway ring inside the hall, six metres up. Its north arm is the two
# ramps to the gantry and the gantry itself, so the corners are the ramp feet.
node("ring_nw", 27.0, 13.0, UP_WALK)
node("ring_ne", 61.0, 13.0, UP_WALK)
node("ring_w", 27.0, 36.0, UP_WALK)
node("ring_e", 61.0, 36.0, UP_WALK)
node("ring_sw", 27.0, 59.0, UP_WALK)
node("ring_se", 61.0, 59.0, UP_WALK)
node("ring_s", 44.0, 59.0, UP_WALK, cover=0.15)
chain("ring_nw", "ring_w", "ring_sw")
chain("ring_ne", "ring_e", "ring_se")
chain("ring_sw", "ring_s", "ring_se")
# The tops of the two ramps up from the hall floor.
node("ramp_w_top", 28.0, 52.0, UP_WALK)
node("ramp_e_top", 60.0, 24.0, UP_WALK)
link("ramp_w_foot", "ramp_w_top")
link("ramp_w_top", "ring_w")
link("ramp_w_top", "ring_sw")
link("ramp_e_foot", "ramp_e_top")
link("ramp_e_top", "ring_e")
link("ramp_e_top", "ring_ne")
# Off the ring into the hall: a four-metre step down, one way.
link("ring_w", "hall_w", "drop")
link("ring_e", "hall_e", "drop")
link("ring_s", "hall_s", "drop")
link("ring_nw", "hall_nw", "drop")
link("ring_ne", "hall_ne", "drop")

# The gantry over the north walk, up the ramps from the ring's north corners.
node("gantry", 44.0, 13.0, UP_LEDGE, cover=0.2, poi="gantry")
link("ring_nw", "gantry")
link("ring_ne", "gantry")

# The galleries over the two shops: a four-metre ring round a light well, so
# the two floors can shoot at each other.
node("wgal_n", 12.0, 13.0, UP_WALK, cover=0.2)
node("wgal_w", 5.0, 28.0, UP_WALK, cover=0.4)
node("wgal_s", 12.0, 43.0, UP_WALK, cover=0.2)
node("wgal_e", 23.0, 28.0, UP_WALK)
chain("wgal_n", "wgal_w", "wgal_s")
link("wgal_n", "wgal_e")
link("wgal_s", "wgal_e")
# Up out of the sump, and the drops back into the shop.
node("wgal_ramp", 10.0, 47.0, UP_WALK)
link("sump_ramp", "wgal_ramp")
link("wgal_ramp", "wgal_s")
for a, b in (("wgal_n", "west_n"), ("wgal_w", "west_m"), ("wgal_s", "west_s"), ("wgal_e", "west_in_n")):
    link(a, b, "drop")
# The gallery window into the hall, at walkway height.
link("wgal_e", "ring_w", "drop")

node("egal_n", 76.0, 27.0, UP_WALK, cover=0.2)
node("egal_e", 83.0, 43.0, UP_WALK, cover=0.4)
node("egal_s", 76.0, 59.0, UP_WALK, cover=0.2)
node("egal_w", 66.0, 43.0, UP_WALK)
chain("egal_n", "egal_e", "egal_s")
link("egal_n", "egal_w")
link("egal_s", "egal_w")
node("egal_ramp", 76.0, 22.0, UP_WALK)
link("alcove_ramp", "egal_ramp")
link("egal_ramp", "egal_n")
for a, b in (("egal_n", "east_n"), ("egal_e", "east_m"), ("egal_s", "east_s"), ("egal_w", "east_in_n")):
    link(a, b, "drop")
link("egal_w", "ring_e", "drop")

# ---- what is lying about to be picked up -------------------------------------
# Health, armour, rounds and the guns themselves (src/shooter/pickup.rs).
#
# Placed on nav nodes rather than at coordinates of their own, which is not
# laziness: a node is already known to be standable, not inside a pillar and
# not on top of another node, and those are exactly the three ways a pad goes
# wrong. It also means a bot can always route to one, since the thing it is
# routing to IS a node.
#
# The layout is the whole of the map design, so it is worth saying what it is
# for. Health is cheap and in the open on the hall floor, where fights happen,
# so a hurt fighter has somewhere to go that is also dangerous. Armour is up on
# the gantry, which is the most exposed place in the arena -- you can be seen
# from everywhere up there, and that is the price. The rocket launcher is on
# the pit ramp, down in the open and a long way from anywhere, so fetching it
# costs you position. It was at the bottom of the pit, which was a better idea
# on paper -- the pit is slag and burns, so the best gun cost health as well --
# but steering does not know that ground can hurt (BACKLOG 364), and what it
# actually bought was bots wading into the slag after it. A pad is no place to
# express a risk the other fighters cannot perceive.
# The scrap gun is in the west sump, close quarters, where it
# is the right gun anyway. Rounds are spread round the ring so a fight on the
# walkway does not simply run dry.
#
# The seven guns added later are placed by the range each one is for, which is
# the only principle a weapon pad has: the sniper rifle at the blind end of the
# west gallery, which is the one place in the arena you can see a long way from
# and be hard to reach; the bore on the east walkway, which looks across
# the whole hall; the pulse gun in the west shop and the skipjack in the east,
# both rooms of corners at the range those two guns want; and the goo gun down
# the sump ramp, where sludge on a wall is in somebody's way. The Digger
# and the sidearm are on nobody's floor, because a fighter respawns holding
# both: the hammer needs no ammunition at all, and a man with nothing to shoot
# with is not playing.

PADS = [
    ("health", "hall_nw", None, None, None),
    ("health", "hall_se", None, None, None),
    ("health", "west_in_s", None, None, None),
    ("health", "east_in_n", None, None, None),
    ("armour", "gantry", None, None, 35.0),
    ("armour", "alcove_w", 25.0, None, 25.0),
    ("weapon", "pit_ramp", None, "rocket_launcher", 30.0),
    ("weapon", "sump_e", None, "scrap_gun", 20.0),
    ("weapon", "ring_nw", None, "minigun", 20.0),
    # The seven guns a fighter does not respawn with. The hammer and the
    # sidearm are never on the floor: you always have both.
    ("weapon", "wgal_w", None, "sniper_rifle", 25.0),
    ("weapon", "ring_e", None, "bore", 25.0),
    ("weapon", "west_m", None, "pulse_gun", 20.0),
    ("weapon", "east_m", None, "skipjack", 20.0),
    ("weapon", "sump_ramp", None, "goo_gun", 20.0),
    ("ammo", "ring_se", None, "rocket_launcher", None),
    ("ammo", "wgal_s", None, "minigun", None),
    ("ammo", "egal_s", None, "scrap_gun", None),
    ("ammo", "ramp_w_foot", None, "minigun", None),
    ("ammo", "ramp_e_foot", None, "scrap_gun", None),
    ("ammo", "wgal_n", None, "sniper_rifle", None),
    ("ammo", "egal_w", None, "bore", None),
    ("ammo", "east_in_s", None, "pulse_gun", None),
    ("ammo", "west_in_n", None, "goo_gun", None),
    ("ammo", "hall_w", None, "skipjack", None),
    ("ammo", "hall_e", None, "sidearm", None),
]

for kind, where, _amt, _gun, _resp in PADS:
    assert where in NAV, f"a {kind} pad stands on {where}, which is not a nav node"
# Two pads on one node would be two blobs in the same place.
_seen = [w for _, w, _, _, _ in PADS]
assert len(set(_seen)) == len(_seen), f"two pads share a node: {sorted(set(x for x in _seen if _seen.count(x) > 1))}"

# ---- out: the nav graph into world.json -------------------------------------
# The engine reads world.json, so that stays the source; this only keeps the
# graph level with the geometry the script above built.
# World coordinates, taken from where the chronicle actually puts the deck, so
# the graph follows the building if it ever moves.
DECK_AT = chron["places"]["deck"]
OFF_X = DECK_AT[0] - AX
OFF_Z = DECK_AT[1] - AZ

ix = {name: i for i, name in enumerate(NAV_ORDER)}
nodes = []
for name in NAV_ORDER:
    x, z, up, cover, poi = NAV[name]
    n = {"at": [round(x + OFF_X, 2), round(z + OFF_Z, 2)], "up": round(up, 2)}
    if cover:
        n["cover"] = cover
    if poi:
        n["poi"] = poi
    nodes.append(n)
links = []
for a, b, reach in LINKS:
    l = {"a": ix[a], "b": ix[b]}
    if reach != "walk":
        l["reach"] = reach
    links.append(l)

wpath = os.path.join(here, "world.json")
with open(wpath, encoding="utf-8") as fh:
    world = json.load(fh)
world["shooter"]["nav"] = {"nodes": nodes, "links": links}
pads = []
for kind, where, amount, gun, respawn in PADS:
    x, z, up, _cover, _poi = NAV[where]
    pad = {"kind": kind, "at": [round(x + OFF_X, 2), round(z + OFF_Z, 2)], "up": round(up, 2)}
    if gun:
        pad["weapon"] = gun
    if amount is not None:
        pad["amount"] = amount
    if respawn is not None:
        pad["respawn_s"] = respawn
    pads.append(pad)
world["shooter"]["pickups"] = pads
# Indented JSON puts every number of every coordinate on its own line, which
# for a graph this size is a thousand lines nobody can read. Collapse the
# arrays that hold only numbers back onto one line.
text = json.dumps(world, indent=2)
text = re.sub(
    r"\[\s+(-?[\d.]+(?:,\s+-?[\d.]+)*)\s+\]",
    lambda m: "[" + ", ".join(x.strip().rstrip(",") for x in m.group(1).split()) + "]",
    text,
)
with open(wpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(text + "\n")
print(f"{len(nodes)} nav nodes, {len(links)} links, {len(pads)} pickups -> {wpath}")
