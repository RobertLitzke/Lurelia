#!/usr/bin/env python3
"""Write ctfbox: one long room with a flag at each end, for measuring how bots
play capture the flag.

Duelbox is the bench for guns; this is the bench for the team brain. The map
is kept out of the answer: a flat, roofed room ninety metres by thirty, a
stand at each end, two pillars in each half so a carrier has something to
break a line of sight with, and three spawns a side behind their own stand.
Whatever separates two team brains in here is the team brain.

    python worlds/ctfbox/make_box.py

Writes library/structures/ctfbox.json and the nav graph, spawns, flags and
lamps into this folder's world.json and chronicle.json.
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

V = 4  # voxels per metre
DECK = 5.0
CEIL = 17.0
TOP = 18.0
X0, X1 = 2.0, 94.0
Z0, Z1 = 2.0, 34.0
WALL = 1.0
MID_X = (X0 + X1) * 0.5
MID_Z = (Z0 + Z1) * 0.5

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


def lamp(x, z):
    box(x - 0.75, x + 0.75, CEIL - 0.25, CEIL, z - 0.25, z + 0.25, "works_lamp", "glass")
    LAMPS.append((round(x - MID_X, 2), round(z - MID_Z, 2), round(CEIL - 0.4, 2)))


box(X0, X1, 0.0, DECK - 0.25, Z0, Z1, "concrete", "stone")
box(X0, X1, DECK - 0.25, DECK, Z0, Z1, "deck_floor", "floor")
for a, b, c, d in [(X0, X1, Z0, Z0 + WALL), (X0, X1, Z1 - WALL, Z1), (X0, X0 + WALL, Z0, Z1), (X1 - WALL, X1, Z0, Z1)]:
    box(a, b, DECK, CEIL, c, d, "works_panel_pale", "wall")
box(X0, X1, CEIL, TOP, Z0, Z1, "corrugated_tin", "roof")
box(MID_X - 0.25, MID_X + 0.25, DECK - 0.25, DECK, Z0 + WALL, Z1 - WALL, "hazard_stripe", "floor")

# Two pillars a half, mirrored about the middle line: something to break a
# line of sight with on the way home, and nothing else.
PILLARS = [(30.0, 11.0), (36.0, 25.0)]
for px, pz in PILLARS:
    for x in (px, X1 + X0 - px):
        box(x - 1.5, x + 1.5, DECK, CEIL, pz - 1.5, pz + 1.5, "works_panel_pale", "wall")

for lx in (14.0, 33.0, 48.0, 63.0, 82.0):
    for lz in (10.0, 26.0):
        lamp(lx, lz)

here = os.path.dirname(os.path.abspath(__file__))
SIZE = [round((X1 - X0) * V) + 16, round(TOP * V) + 8, round((Z1 - Z0) * V) + 16]
struct = {
    "name": "ctfbox",
    "version": 1,
    "introduced_by": "ctfbox",
    "_generated_by": "worlds/ctfbox/make_box.py",
    "notes": (
        "One long room for capture the flag, written by worlds/ctfbox/make_box.py: ninety-two "
        "metres by thirty-two, a flag stand at each end, two pillars in each half, a roof and "
        "ten lamps. A bench for the team brain, not a map. Edit the script, not this file."
    ),
    "size": SIZE,
    "anchor": [round(MID_X * V), 0, round(MID_Z * V)],
    "foundation": 4,
    "level": True,
    "decays": False,
    "tags": ["arena", "works"],
    "ops": ops,
}
spath = os.path.join(ROOT, "library", "structures", "ctfbox.json")
with open(spath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(libfmt.render(struct, "structures"))
print(f"{len(ops)} ops -> {spath}")

cpath = os.path.join(here, "chronicle.json")
with open(cpath, encoding="utf-8") as fh:
    chron = json.load(fh)
chron["events"] = [e for e in chron["events"] if e.get("event") != "lamp"]
chron["events"].append(
    {
        "year": -1,
        "event": "lamp",
        "cause": "Ten strip lights down the length of the room.",
        "at": "box",
        "points": [[x, z, y] for x, z, y in LAMPS],
        "reach": 20.0,
        "strength": 1.6,
        "colour": [255, 240, 214],
    }
)
with open(cpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(json.dumps(chron, indent=2) + "\n")

# The nav graph: a grid over the floor with the pillars' cells left out.
AT = chron["places"]["box"]
OX, OZ = AT[0] - MID_X, AT[1] - MID_Z
XS = [8.0, 16.0, 24.0, 32.0, 40.0, 48.0, 56.0, 64.0, 72.0, 80.0, 88.0]
ZS = [7.0, 18.0, 29.0]


def blocked(x, z):
    for px, pz in PILLARS:
        for cx in (px, X1 + X0 - px):
            if abs(x - cx) < 2.6 and abs(z - pz) < 2.6:
                return True
    return False


nodes, ix = [], {}
for iz, z in enumerate(ZS):
    for jx, x in enumerate(XS):
        if blocked(x, z):
            continue
        ix[(jx, iz)] = len(nodes)
        nodes.append({"at": [round(x + OX, 2), round(z + OZ, 2)], "up": 0.0})
links = []
for (jx, iz), a in ix.items():
    for dj, di in ((1, 0), (0, 1)):
        b = ix.get((jx + dj, iz + di))
        if b is not None:
            links.append({"a": a, "b": b})

# Stand on the middle row's end nodes, so a bot can route to it as a node.
FLAG_W = (XS[0], ZS[1])
FLAG_E = (XS[-1], ZS[1])
spawns = []
for team, x, bearing in ((0, 4.5, 90), (1, X1 + X0 - 4.5, 270)):
    for z in (9.0, 18.0, 27.0):
        spawns.append({"at": [round(x + OX, 2), round(z + OZ, 2)], "bearing": bearing, "team": team})

wpath = os.path.join(here, "world.json")
with open(wpath, encoding="utf-8") as fh:
    world = json.load(fh)
sh = world["shooter"]
sh["nav"] = {"nodes": nodes, "links": links}
sh["spawns"] = spawns
sh["ctf"]["flags"] = [
    {"team": 0, "at": [round(FLAG_W[0] + OX, 2), round(FLAG_W[1] + OZ, 2)]},
    {"team": 1, "at": [round(FLAG_E[0] + OX, 2), round(FLAG_E[1] + OZ, 2)]},
]
world["spawn"] = [round(MID_X + OX, 2), round(Z0 + 4.0 + OZ, 2)]
text = json.dumps(world, indent=2)
text = re.sub(
    r"\[\s+(-?[\d.]+(?:,\s+-?[\d.]+)*)\s+\]",
    lambda m: "[" + ", ".join(x.strip().rstrip(",") for x in m.group(1).split()) + "]",
    text,
)
with open(wpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(text + "\n")
print(f"{len(nodes)} nav nodes, {len(links)} links -> {wpath}")
