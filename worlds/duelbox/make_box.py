#!/usr/bin/env python3
"""Write library/structures/duelbox.json: one empty room, for measuring guns.

Slagdeck is a map -- a pit, galleries, corridors, nine spawns -- and that is
exactly what makes it the wrong instrument for a question about a weapon. Two
bots on Slagdeck spend most of a match out of each other's sight, and what
comes out of a round is partly the map.

So: one rectangular room with nothing in it. No cover, no height, no pickups,
two spawns at opposite ends of the long axis. Whatever separates two guns in
here is the gun.

    python worlds/duelbox/make_box.py
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

V = 4  # voxels per metre

DECK = 5.0  # the floor, metres above the yard, on a solid plinth
CEIL = 17.0  # the underside of the roof
TOP = 18.0  # the top of the structure

# The footprint, metres. Long enough that a sniper rifle and a scrap gun are
# both at home somewhere in it: fifty metres end to end.
X0, X1 = 2.0, 60.0
Z0, Z1 = 2.0, 36.0
WALL = 1.0
IX0, IX1 = X0 + WALL, X1 - WALL
IZ0, IZ1 = Z0 + WALL, Z1 - WALL

ops = []
LAMPS = []


def box(x0, x1, y0, y1, z0, z1, mat, part=None):
    op = {
        "op": "box",
        "from": [round(x0 * V), round(y0 * V), round(z0 * V)],
        "to": [round(x1 * V), round(y1 * V), round(z1 * V)],
        "mat": mat,
    }
    if part:
        op["part"] = part
    ops.append(op)


def lamp(x, z, reach=20.0, strength=1.6):
    box(x - 0.75, x + 0.75, CEIL - 0.25, CEIL, z - 0.25, z + 0.25, "works_lamp", "glass")
    LAMPS.append((round(x - (X0 + X1) * 0.5, 2), round(z - (Z0 + Z1) * 0.5, 2), round(CEIL - 0.4, 2), reach, strength))


# The plinth and the floor.
box(X0, X1, 0.0, DECK - 0.25, Z0, Z1, "concrete", "stone")
box(X0, X1, DECK - 0.25, DECK, Z0, Z1, "deck_floor", "floor")

# Four walls and a roof.
for a, b, c, d in [
    (X0, X1, Z0, Z0 + WALL),
    (X0, X1, Z1 - WALL, Z1),
    (X0, X0 + WALL, Z0, Z1),
    (X1 - WALL, X1, Z0, Z1),
]:
    box(a, b, DECK, CEIL, c, d, "works_panel_pale", "wall")
box(X0, X1, CEIL, TOP, Z0, Z1, "corrugated_tin", "roof")

# A painted line down the middle of the floor, so a screenshot of the room
# says which way it runs and how far across it you are.
box((X0 + X1) * 0.5 - 0.25, (X0 + X1) * 0.5 + 0.25, DECK - 0.25, DECK, IZ0, IZ1, "hazard_stripe", "floor")

for lx in (14.0, 31.0, 48.0):
    for lz in (12.0, 26.0):
        lamp(lx, lz)

# ---- out: the structure -----------------------------------------------------
here = os.path.dirname(os.path.abspath(__file__))
SIZE = [round((X1 - X0) * V) + 16, round(TOP * V) + 8, round((Z1 - Z0) * V) + 16]
struct = {
    "name": "duelbox",
    "version": 1,
    "introduced_by": "duelbox",
    "notes": (
        "One empty room, written by worlds/duelbox/make_box.py: fifty-eight metres "
        "by thirty-four, a flat floor on a plinth, four walls, a roof and six lamps. "
        "Nothing to hide behind, which is the point -- it is a bench for weapons, "
        "not a map. Edit the script, not this file."
    ),
    "size": SIZE,
    "anchor": [round((X0 + X1) * 0.5 * V), 0, round((Z0 + Z1) * 0.5 * V)],
    "foundation": 4,
    "level": True,
    "decays": False,
    "tags": ["arena", "works"],
    "ops": ops,
}
spath = os.path.join(ROOT, "library", "structures", "duelbox.json")
with open(spath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(libfmt.render(struct, "structures"))
print(f"{len(ops)} ops -> {spath}")

# ---- out: the lamps into the chronicle --------------------------------------
cpath = os.path.join(here, "chronicle.json")
with open(cpath, encoding="utf-8") as fh:
    chron = json.load(fh)
chron["events"] = [e for e in chron["events"] if e.get("event") != "lamp"]
chron["events"].append(
    {
        "year": -1,
        "event": "lamp",
        "cause": "Six strip lights, because a room nobody can see across measures nothing.",
        "at": "box",
        "points": [[x, z, y] for x, z, y, _r, _s in LAMPS],
        "reach": LAMPS[0][3],
        "strength": LAMPS[0][4],
        "colour": [255, 240, 214],
    }
)
with open(cpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(json.dumps(chron, indent=2) + "\n")

# ---- out: the nav graph -----------------------------------------------------
# A plain grid over the floor. Nothing in the room, so every node sees every
# other one and the links are only about walking.
BOX_AT = chron["places"]["box"]
OFF_X = BOX_AT[0] - (X0 + X1) * 0.5
OFF_Z = BOX_AT[1] - (Z0 + Z1) * 0.5
XS = [8.0, 18.0, 28.0, 38.0, 48.0, 54.0]
ZS = [8.0, 19.0, 30.0]
nodes, ix = [], {}
for iz, z in enumerate(ZS):
    for jx, x in enumerate(XS):
        ix[(jx, iz)] = len(nodes)
        nodes.append({"at": [round(x + OFF_X, 2), round(z + OFF_Z, 2)], "up": 0.0})
links = []
for iz in range(len(ZS)):
    for jx in range(len(XS)):
        if jx + 1 < len(XS):
            links.append({"a": ix[(jx, iz)], "b": ix[(jx + 1, iz)]})
        if iz + 1 < len(ZS):
            links.append({"a": ix[(jx, iz)], "b": ix[(jx, iz + 1)]})

wpath = os.path.join(here, "world.json")
with open(wpath, encoding="utf-8") as fh:
    world = json.load(fh)
world["shooter"]["nav"] = {"nodes": nodes, "links": links}
world["shooter"]["spawns"] = [
    {"at": [round(10.0 + OFF_X, 2), round(19.0 + OFF_Z, 2)], "bearing": 90},
    {"at": [round(52.0 + OFF_X, 2), round(19.0 + OFF_Z, 2)], "bearing": 270},
]
world["spawn"] = [round(31.0 + OFF_X, 2), round(8.0 + OFF_Z, 2)]
text = json.dumps(world, indent=2)
text = re.sub(
    r"\[\s+(-?[\d.]+(?:,\s+-?[\d.]+)*)\s+\]",
    lambda m: "[" + ", ".join(x.strip().rstrip(",") for x in m.group(1).split()) + "]",
    text,
)
with open(wpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(text + "\n")
print(f"{len(nodes)} nav nodes, {len(links)} links -> {wpath}")
