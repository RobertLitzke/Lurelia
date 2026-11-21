#!/usr/bin/env python3
"""Write Sluice: a capture-the-flag map, two pump yards facing each other
across a drained spillway.

The map is mirror-symmetric about x = 48 (its own metres), so swapping sides
is a fair test of anything else. Three lanes join the two yards and meet
nowhere between them:

- the gantry, along the north wall: a walkway three metres up, reached by a
  flight of steps at each end, with the long view down the whole spillway.
  The sniper's lane.
- the channel, down the middle: the spillway floor two metres below the
  yards, open to the sky and to the gantry above it, with three pillars of
  the old sluice gate standing in it for cover. The quick lane and the
  exposed one.
- the culvert, along the south wall: a roofed concrete passage at yard
  level with a dog-leg halfway, so it has no sightline end to end. The slow
  and safe lane, and the one a carrier with company takes home.

Each yard has a wall across it with a door onto each lane, two crates for a
defender to hold behind, and the flag stand at the back.

    python worlds/sluice/make_map.py

Writes library/structures/sluice_yards.json, and the nav graph, spawns,
pickups and flags into this folder's world.json. Edit this, not those.
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

V = 4  # voxels per metre
ORG = 2.0  # the map's own metres start this far into the structure
DECK = 5.0  # yard floor
LOW = DECK - 2.0  # channel floor
HIGH = DECK + 3.0  # gantry top
WALL_TOP = DECK + 6.0
L, W = 96.0, 44.0  # length (x) and width (z)
MID = L / 2

ops = []


def box(x0, x1, y0, y1, z0, z1, mat, part=None):
    """Both halves at once: every box is laid where it says and mirrored
    about the middle, which is what keeps the map fair by construction. A
    box that reaches the middle line is one box across it."""
    spans = [(x0, L - x0)] if x1 >= MID else [(x0, x1), (L - x1, L - x0)]
    for a, b in spans:
        op = {
            "op": "box",
            "from": [round((a + ORG) * V), round(y0 * V), round((z0 + ORG) * V)],
            "to": [round((b + ORG) * V), round(y1 * V), round((z1 + ORG) * V)],
            "mat": mat,
        }
        if part:
            op["part"] = part
        ops.append(op)


# ---- ground --------------------------------------------------------------
box(0, MID, 0.0, DECK - 0.25, 0, W, "concrete", "stone")
box(0, MID, DECK - 0.25, DECK, 0, W, "deck_floor", "floor")
# The spillway: carved out of the plinth, stepping down from each yard over
# six metres (eight steps of a quarter metre, a body walks them).
box(30, MID, LOW, DECK, 14, 30, "air")
for i in range(8):
    x = 24 + 0.75 * i
    box(x, 30, DECK - 0.25 * (i + 1), DECK, 14, 30, "air")
box(24, MID, LOW - 0.25, LOW, 14, 30, "concrete", "floor")
for i in range(8):
    x = 24 + 0.75 * i
    box(x, x + 0.75, DECK - 0.25 * (i + 1) - 0.25, DECK - 0.25 * (i + 1), 14, 30, "deck_floor", "floor")

# ---- the outer wall ------------------------------------------------------
box(0, MID, DECK, WALL_TOP, 0, 1, "works_panel_pale", "wall")
box(0, MID, DECK, WALL_TOP, W - 1, W, "works_panel_pale", "wall")
box(0, 1, DECK, WALL_TOP, 0, W, "works_panel_pale", "wall")

# ---- each yard: a wall across it with three doors, two crates ------------
YARD = 20.0
for z0, z1 in ((1, 4), (11, 18), (26, 34), (40, W - 1)):
    box(YARD, YARD + 1, DECK, DECK + 4.5, z0, z1, "works_panel_pale", "wall")
for cx, cz in ((13, 13), (13, 31)):
    box(cx - 1, cx + 1, DECK, DECK + 1.5, cz - 1, cz + 1, "corrugated_tin", "crate")
# The stand's own floor, painted, so the base reads from the far end.
box(6, 10, DECK - 0.25, DECK, 20, 24, "hazard_stripe", "floor")

# ---- the gantry ----------------------------------------------------------
box(28, MID, DECK, HIGH, 3, 10, "works_panel_pale", "stone")
box(28, MID, HIGH - 0.25, HIGH, 3, 10, "deck_floor", "floor")
for i in range(12):
    x = 22 + 0.5 * i
    box(x, 28, DECK, DECK + 0.25 * (i + 1), 3, 10, "deck_floor", "floor")
# A rail along the channel side, waist high: it keeps nobody in, it only
# says where the edge is.
box(28, MID, HIGH, HIGH + 1.0, 9.75, 10, "hazard_stripe", "rail")

# ---- the old gate's pillars, in the channel ------------------------------
box(39, 41, LOW, DECK + 1.0, 17, 19, "concrete", "wall")
box(47, MID, LOW, DECK + 1.0, 25, 27, "concrete", "wall")

# ---- the culvert ---------------------------------------------------------
box(24, MID, DECK, DECK + 4.0, 32, 33, "concrete", "wall")
box(30, MID, DECK + 4.0, DECK + 4.5, 32, W - 1, "concrete", "roof")
# The dog-leg: a wall out from the south side, so the passage has no line
# end to end.
box(47, MID, DECK, DECK + 4.0, 37, W - 1, "concrete", "wall")

# ---- out: the structure --------------------------------------------------
here = os.path.dirname(os.path.abspath(__file__))
SIZE = [round((L + 2 * ORG) * V) + 8, round((WALL_TOP + 1) * V) + 8, round((W + 2 * ORG) * V) + 8]
struct = {
    "name": "sluice_yards",
    "version": 1,
    "introduced_by": "sluice",
    "_generated_by": "worlds/sluice/make_map.py",
    "notes": (
        "Sluice's whole map, written by worlds/sluice/make_map.py: two pump yards across a "
        "drained spillway, joined by a gantry, the channel and a culvert. Mirror-symmetric "
        "about its middle. Edit the script, not this file."
    ),
    "size": SIZE,
    "anchor": [round((MID + ORG) * V), 0, round((W / 2 + ORG) * V)],
    "foundation": 4,
    "level": True,
    "decays": False,
    "tags": ["arena", "works"],
    "ops": ops,
}
spath = os.path.join(ROOT, "library", "structures", "sluice_yards.json")
with open(spath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(libfmt.render(struct, "structures"))
print(f"{len(ops)} ops -> {spath}")

# ---- the nav graph -------------------------------------------------------
# The west half by hand, then mirrored; nodes on the middle line are shared.
# Heights are metres above the yard floor.
WEST = {
    "stand": (8, 22, 0),
    "yard_n": (8, 8, 0),
    "yard_s": (8, 36, 0),
    "crate_n": (16, 15, 0),
    "crate_s": (16, 29, 0),
    "yard_mid": (15, 22, 0),
    "door_n": (21.5, 7, 0),
    "door_m": (21.5, 22, 0),
    "door_s": (21.5, 37, 0),
    "steps": (24, 6.5, 1.25),
    "gantry_a": (30, 6.5, 3),
    "gantry_b": (39, 6.5, 3),
    "chan_a": (31, 22, -2),
    "chan_b": (40, 23, -2),
    "culv_a": (26, 37.5, 0),
    "culv_b": (34, 37.5, 0),
    "culv_c": (42, 35, 0),
}
CENTRE = {"gantry_mid": (48, 6.5, 3), "chan_mid": (48, 20, -2), "culv_mid": (48, 34.5, 0)}
WEST_LINKS = [
    ("stand", "yard_mid"),
    ("stand", "yard_n"),
    ("stand", "yard_s"),
    ("yard_n", "crate_n"),
    ("yard_s", "crate_s"),
    ("crate_n", "yard_mid"),
    ("crate_s", "yard_mid"),
    ("yard_n", "door_n"),
    ("crate_n", "door_n"),
    ("yard_mid", "door_m"),
    ("crate_n", "door_m"),
    ("crate_s", "door_m"),
    ("yard_s", "door_s"),
    ("crate_s", "door_s"),
    ("door_n", "steps"),
    ("steps", "gantry_a"),
    ("gantry_a", "gantry_b"),
    ("gantry_b", "gantry_mid"),
    ("door_m", "chan_a"),
    ("chan_a", "chan_b"),
    ("chan_b", "chan_mid"),
    ("door_s", "culv_a"),
    ("culv_a", "culv_b"),
    ("culv_b", "culv_c"),
    ("culv_c", "culv_mid"),
]

with open(os.path.join(here, "chronicle.json"), encoding="utf-8") as fh:
    chron = json.load(fh)
AT = chron["places"]["yards"]
OX, OZ = AT[0] - MID, AT[1] - W / 2


def world_xz(x, z):
    return [round(x + OX, 2), round(z + OZ, 2)]


nodes, ix = [], {}
for side in ("w", "e"):
    for name, (x, z, up) in WEST.items():
        xx = x if side == "w" else L - x
        ix[(side, name)] = len(nodes)
        nodes.append({"at": world_xz(xx, z), "up": up})
for name, (x, z, up) in CENTRE.items():
    ix[("c", name)] = len(nodes)
    nodes.append({"at": world_xz(x, z), "up": up, "poi": name.replace("_", " ")})


def key(side, name):
    return ("c", name) if name in CENTRE else (side, name)


links = []
for side in ("w", "e"):
    for a, b in WEST_LINKS:
        links.append({"a": ix[key(side, a)], "b": ix[key(side, b)]})

# ---- spawns, flags, pickups ----------------------------------------------
spawns = []
for team, side in ((0, "w"), (1, "e")):
    for x, z, bearing in ((4, 12, 90), (4, 32, 90), (11, 22, 90)):
        xx = x if side == "w" else L - x
        spawns.append({"at": world_xz(xx, z), "bearing": bearing if side == "w" else 270, "team": team})
flags = [
    {"team": 0, "at": world_xz(*WEST["stand"][:2])},
    {"team": 1, "at": world_xz(L - WEST["stand"][0], WEST["stand"][1])},
]
pickups = [
    {"kind": "health", "at": world_xz(48, 22), "up": -2.0, "amount": 50.0},
    {"kind": "armour", "at": world_xz(48, 36), "amount": 50.0},
    {"kind": "health", "at": world_xz(15, 22), "amount": 10.0},
    {"kind": "health", "at": world_xz(L - 15, 22), "amount": 10.0},
    {"kind": "health", "at": world_xz(39, 6.5), "up": 3.0, "amount": 25.0},
    {"kind": "health", "at": world_xz(L - 39, 6.5), "up": 3.0, "amount": 25.0},
]

wpath = os.path.join(here, "world.json")
with open(wpath, encoding="utf-8") as fh:
    world = json.load(fh)
sh = world["shooter"]
sh["nav"] = {"nodes": nodes, "links": links}
sh["spawns"] = spawns
sh["ctf"]["flags"] = flags
sh["pickups"] = pickups
world["spawn"] = world_xz(MID, 13)
text = json.dumps(world, indent=2)
text = re.sub(
    r"\[\s+(-?[\d.]+(?:,\s+-?[\d.]+)*)\s+\]",
    lambda m: "[" + ", ".join(x.strip().rstrip(",") for x in m.group(1).split()) + "]",
    text,
)
with open(wpath, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(text + "\n")
print(f"{len(nodes)} nav nodes, {len(links)} links -> {wpath}")
