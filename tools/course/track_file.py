"""Build a kart track from a .track layout (tools/course/FORMAT.md).

The track is everything a kart touches: the road's centreline and height at
every metre, its cross-section, walls, decks, tunnels, jumps, and the race
data (grid, pickups, pads, checkpoints). This resolves the layout once into
`worlds/<name>/track.json`, which the engine stamps into the world after the
world's own chronicle, without asking the ground for anything, so the track
comes out the same whatever the world and the engine do round it
(FORMAT.md).

The world round the track is an ordinary chronicle, written by hand or by an
agent. The first build writes a starter world (world.json, chronicle.json,
story.md, look.json) from the style; later builds leave those alone except
for world.json's `race`, `spawn` and `spawn_bearing`, which follow the track.
`ground.bin`, the land fitted to the road, is rewritten every build.

    python -I tools/course/track_file.py tools/course/maps/peregrine.track            # build
    python -I tools/course/track_file.py tools/course/maps/peregrine.track --check    # report only
    python -I tools/course/track_file.py tools/course/maps/peregrine.track --fresh    # rewrite the starter world too
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))

import shapes  # noqa: E402

# Where the road stands more than this over the land it is a deck on pillars
# (the engine's DECK_ABOVE).
DECK_ABOVE = 2.5
# Two stretches of road that cross want this much between their surfaces.
CROSS_CLEAR = 4.5
# Past the corridor the fitted ground eases from the road back to the land
# over this many metres.
FIT = 20.0
# Land lower than the road is filled up level with it this far past the
# run-off (`shoulder`), so there is never a ditch beside the road.
SHOULDER = 10.0
# Run-off either side of an open road (no walls), metres, unless a `verge`
# line says otherwise.
RUNOFF = 8.0
# A wall stands this far out past the kerb: 1 m of kerb, then 1.2 m of wall.
WALL_THICK = 1.2
# The kart, for estimates: top speed (m/s) and gravity (library/vehicles/kart.json).
KART_V = 30.0
KART_G = 14.0
# A pad's kick at full speed, for flying a boost jump (race/mod.rs
# `boost_kick_by`): the kart leaves the lip at about this.
KART_BOOST_V = 38.0
# The props a kart smashes (`smash`): metres along the road, across it and
# high, the material, and the share of its speed a kart keeps.
SMASH = {
    "bale": (2.0, 1.25, 1.0, "straw_bound", 0.9),
    "stones": (2.0, 1.0, 0.75, None, 0.86),
    "gate": (3.5, 0.25, 1.25, "weathered_planks", 0.92),
    "cane": (6.0, 3.0, 3.0, "cane_stalk", 0.93),
    "logs": (3.0, 1.5, 0.75, "weathered_planks", 0.9),
    "crates": (1.5, 1.5, 1.5, "weathered_planks", 0.9),
}
KINDS = ("gun", "rocket", "goo", "block", "disc", "boost", "shield", "overdrive", "repair")
SIDES = {"left": -1.0, "centre": 0.0, "center": 0.0, "right": 1.0}


class LayoutError(Exception):
    pass


def style(name):
    p = HERE / "styles" / f"{name}.json"
    if not p.exists():
        have = ", ".join(sorted(q.stem for q in (HERE / "styles").glob("*.json")))
        raise LayoutError(f"style {name}: no tools/course/styles/{name}.json (there are {have})")
    return json.loads(p.read_text(encoding="utf-8"))


# ---- reading the layout -----------------------------------------------------------------


def parse(path):
    """The layout as a dict."""
    c = {"laps": 3, "rivals": 3, "vehicle": "kart", "width": 12.0, "walls": 0.0, "bank": 1.0,
         "land": 30.0, "rolling": 4.0, "hour": 14.0, "lights": 0.0, "grid": 8, "title": None, "blurb": "",
         "mats": {}, "hills": [], "points": [], "surface": [], "widths": [], "wall_runs": [], "verge": [],
         "banks": [], "items": [], "pads": [], "jumps": [], "checkpoints": [], "shapes": [], "structures": [],
         "plateaus": [], "ramps": [], "tunnels": [], "fords": [], "floods": [], "zones": [], "smash": [],
         "boost_jumps": set(), "landings": []}

    def opt(w, key, cast=float, default=None):
        if key in w:
            i = w.index(key)
            v = cast(w[i + 1])
            del w[i:i + 2]
            return v
        return default

    for n, raw in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        w = line.split()
        k = w[0]
        try:
            if k == "track":
                c["name"] = w[1]
                c["size"] = float(w[w.index("size") + 1])
                c["style"] = w[w.index("style") + 1] if "style" in w else "downs"
            elif k in ("title", "blurb"):
                c[k] = line[len(k):].strip()
            elif k == "race":
                c["laps"] = opt(w, "laps", int, c["laps"])
                c["rivals"] = opt(w, "rivals", int, c["rivals"])
                c["vehicle"] = opt(w, "vehicle", str, c["vehicle"])
            elif k == "road":
                c["width"] = float(w[1])
                c["walls"] = opt(w, "walls", float, c["walls"])
                c["bank"] = opt(w, "bank", float, c["bank"])
            elif k == "materials":
                for key in ("road", "kerb", "wall", "deck", "verge", "post", "pad", "portal", "light"):
                    v = opt(w, key, str)
                    if v:
                        c["mats"][key] = v
                if len(w) > 1:
                    raise LayoutError(f"`materials` takes road, kerb, wall, deck, verge, post, pad, portal and light, not {w[1]}")
            elif k == "land":
                c["land"] = float(w[1])
                if len(w) > 2:
                    c["rolling"] = float(w[2])
            elif k == "shoulder":
                c["shoulder"] = float(w[1])
            elif k == "arch":
                c.setdefault("arches", []).append({"s": float(w[1]), "span": float(w[2]) if len(w) > 2 and w[2] != "mat" else None,
                                                   "height": float(w[3]) if len(w) > 3 and w[3] != "mat" else 8.0,
                                                   "thick": float(w[4]) if len(w) > 4 and w[4] != "mat" else 5.0,
                                                   "mat": opt(w, "mat", str), "line": n})
            elif k == "hour":
                c["hour"] = float(w[1])
            elif k == "lights":
                c["lights"] = float(w[1])
            elif k == "grid":
                c["grid"] = int(w[1])
            elif k == "hill":
                c["hills"].append(tuple(map(float, w[1:5])))
            elif k == "plateau":
                ring = [tuple(float(v) for v in p_.split(",")) for p_ in w[3:]]
                if len(ring) < 3:
                    raise LayoutError("`plateau TOP EDGE x,z x,z x,z ...` needs three corners or more")
                c["plateaus"].append((float(w[1]), float(w[2]), ring))
            elif k == "ramp":
                c["ramps"].append((float(w[1]), float(w[2]), float(w[3]) if len(w) > 3 else 30.0))
            elif k == "tunnel":
                mat = opt(w, "mat", str)
                portal = opt(w, "portal", str)
                c["tunnels"].append((float(w[1]), float(w[2]), float(w[3]) if len(w) > 3 else 6.0, mat, portal))
            elif k == "point":
                level = opt(w, "at")
                rise = float(w[3]) if len(w) > 3 else 0.0
                c["points"].append((float(w[1]), float(w[2]), rise, level))
            elif k == "surface":
                c["surface"].append((float(w[1]), float(w[2]), w[3]))
            elif k == "width":
                c["widths"].append((float(w[1]), float(w[2]), float(w[3])))
            elif k == "walls":
                c["wall_runs"].append((float(w[1]), float(w[2]), float(w[3])))
            elif k == "verge":
                mat = None if w[3] == "-" else w[3]
                c["verge"].append((float(w[1]), float(w[2]), mat, float(w[4]) if len(w) > 4 else None))
            elif k == "bank":
                if len(w) > 4 and w[3] == "deg":
                    # A fixed angle, the outside of the turn up: a bowl's wall.
                    deg = float(w[4])
                    if not 0 < deg <= 45:
                        raise LayoutError("`bank FROM TO deg A`: A from 0 to 45 (a voxel slope steeper than 1 in 1 is a step)")
                    c.setdefault("bank_deg", []).append((float(w[1]), float(w[2]), deg))
                else:
                    c["banks"].append((float(w[1]), float(w[2]), float(w[3])))
            elif k == "items":
                s = float(w[1])
                rest = w[2:]
                across = int(rest.pop(0)) if rest and rest[0].isdigit() else None
                for kind in rest:
                    if kind not in KINDS:
                        raise LayoutError(f"no item called {kind} ({', '.join(KINDS)})")
                if across is None:
                    across = len(rest) or 4
                c["items"].append((s, across, rest))
            elif k == "pad":
                side = w[2] if len(w) > 2 else "centre"
                if side not in SIDES:
                    raise LayoutError(f"a pad's side is left, centre or right, not {side}")
                c["pads"].append((float(w[1]), side, float(w[3]) if len(w) > 3 else 6.0))
            elif k == "jump":
                if "boost" in w:
                    w.remove("boost")
                    c["boost_jumps"].add(len(c["jumps"]))
                c["jumps"].append((float(w[1]), float(w[2]), float(w[3]) if len(w) > 3 else 8.0))
                c.setdefault("skews", {})[len(c["jumps"]) - 1] = float(w[4]) if len(w) > 4 else 0.0
            elif k == "landing":
                c["landings"].append((float(w[1]), float(w[2]), float(w[3]) if len(w) > 3 else KART_V,
                                      float(w[4]) if len(w) > 4 else 3.0))
            elif k == "ford":
                c["fords"].append((float(w[1]), float(w[2]), float(w[3]), True))
            elif k == "flood":
                dip = opt(w, "dip", float)
                c["floods"].append((float(w[1]), float(w[2]), [float(v) for v in w[3:]]))
                if dip:
                    c["fords"].append((float(w[1]), float(w[2]), dip, False))
                if len(w) < 4:
                    raise LayoutError("`flood FROM TO DEPTH [DEPTH ...]` needs a depth for lap 1 at least")
            elif k == "wind":
                c["zones"].append({"from": float(w[1]), "to": float(w[2]), "kind": "wind", "value": float(w[3]),
                                   "bearing": float(w[4]), **({"gust": float(w[5])} if len(w) > 5 else {})})
            elif k == "herd":
                c["zones"].append({"from": float(w[1]), "to": float(w[2]), "kind": "herd", "species": w[3],
                                   "value": float(w[4]), "laps": [float(v) for v in w[5:]]})
            elif k == "glide":
                c["zones"].append({"from": float(w[1]), "to": float(w[2]), "kind": "glide", "value": float(w[3]) if len(w) > 3 else 0.3})
            elif k == "thermal":
                c["zones"].append({"from": 0.0, "to": 0.0, "kind": "thermal", "value": float(w[4]),
                                   "at": [float(w[1]), float(w[2]), float(w[3])] + ([float(w[5])] if len(w) > 5 else [])})
            elif k == "gravity":
                c["zones"].append({"from": float(w[1]), "to": float(w[2]), "kind": "gravity", "value": float(w[3])})
            elif k == "smash":
                mat = opt(w, "mat", str)
                slow = opt(w, "slow", float)
                if w[2] not in SIDES:
                    raise LayoutError("`smash S left|centre|right OFFSET KIND [LEN] [mat M] [slow K]`")
                if w[4] not in SMASH:
                    raise LayoutError(f"no smashable called {w[4]} ({', '.join(SMASH)})")
                c["smash"].append({"s": float(w[1]), "off": float(w[3]) * SIDES[w[2]], "kind": w[4],
                                   "len": float(w[5]) if len(w) > 5 else None, "mat": mat, "slow": slow, "line": n})
            elif k == "checkpoint":
                c["checkpoints"].append(float(w[1]))
            elif k in ("add", "carve"):
                if len(w) < 3 or w[1] not in shapes.SHAPES:
                    raise LayoutError(f"`{k}` needs a shape ({', '.join(shapes.SHAPES)})")
                mat = opt(w, "mat", str)
                # `drive`: a shape karts are meant to drive over (a kicker,
                # a shoal), which the check leaves alone.
                if "drive" in w:
                    w.remove("drive")
                    c.setdefault("drive", set()).add(n)
                c["shapes"].append((k, w[1:], mat, n))
            elif k == "structure":
                st = {"name": w[1], "line": n}
                rest = w[2:]
                y = opt(rest, "at")
                if y is not None:
                    st["y"] = y
                if rest and rest[0] == "along":
                    st["along"] = float(rest[1])
                    if rest[2] not in ("left", "right"):
                        raise LayoutError("`structure NAME along S left|right OFFSET [BEARING]`")
                    st["off"] = float(rest[3]) * SIDES[rest[2]]
                    rest = rest[4:]
                    if rest:
                        st["turn"] = float(rest[0])
                else:
                    st["at"] = [float(rest[0]), float(rest[1])]
                    if len(rest) > 2:
                        st["bearing"] = float(rest[2])
                c["structures"].append(st)
            else:
                raise LayoutError(f"unknown line `{k}`")
        except (IndexError, ValueError) as e:
            raise LayoutError(f"line {n}: {raw.strip()!r}: {e}") from None
        except LayoutError as e:
            raise LayoutError(f"line {n}: {e}") from None
    if "name" not in c:
        raise LayoutError("no `track NAME size S [style S]` line")
    if len(c["points"]) < 4:
        raise LayoutError("a track needs at least four `point` lines")
    try:
        c["source"] = Path(path).resolve().relative_to(ROOT).as_posix()
    except ValueError:
        c["source"] = str(path)
    st = style(c["style"])
    c["mats"] = {"road": st["road"], "kerb": st["kerb"], "wall": st["wall"], "deck": st.get("deck", st["wall"]),
                 "post": st.get("gate_post", "bridge_steel"), "pad": st.get("pad", "boost_pad"),
                 "portal": st.get("portal", "concrete"), "light": st.get("light", "works_lamp"), **c["mats"]}
    return c


# ---- the road -------------------------------------------------------------------------------


# A jump's shape: the ramp curves up from level over LEN metres to the lip
# (height H at S, a parabola, so there is no kink where it starts and its
# slope at the lip is 2H/LEN), then a slip face falls back to the road at
# JUMP_BACK (a 1 in 2) over at least JUMP_BACK_MIN metres, so a kart too slow
# to fly rolls down it instead of dropping off a step. The engine slants it
# across the road the same way (race_track.rs `jump_profile`).
JUMP_BACK = 0.5
# The steepest a landing hill falls, as a slope (tan): where the flight
# path has steepened to this, it meets the hill.
LANDING_SLOPE = 0.7
# Over how far a fixed-angle bank's road rises to it either side, metres.
LIFT_EASE = 50.0
# The curve at the foot of a landing hill, metres.
OUTRUN = 40.0
# The hardest a kart may meet the ground after a jump, m/s square on.
JUMP_HIT = 11.0
JUMP_BACK_MIN = 3.0


def jump_back(H):
    return max(H / JUMP_BACK, JUMP_BACK_MIN)


def jump_profile(d, H, LEN):
    """The jump's height over the road at d metres past the ramp's start
    (S - LEN); d may be an array. 0 outside the ramp and the slip face."""
    d = np.asarray(d, dtype=np.float64)
    B = jump_back(H)
    up = np.clip(d / LEN, 0.0, 1.0) ** 2
    down = 1.0 - np.clip((d - LEN) / B, 0.0, 1.0)
    return np.where((d >= 0) & (d < LEN), H * up, np.where((d >= LEN) & (d < LEN + B), H * down, 0.0))


def land_at(c, x, z):
    """The track's own land under (x, z): `land` and the hills, shaped as the
    terrain event shapes its relief (a bell, a third of its height left at
    RADIUS), then each plateau in turn, and no noise (the world adds that)."""
    x, z = np.broadcast_arrays(np.asarray(x, dtype=np.float64), np.asarray(z, dtype=np.float64))
    h = np.full(x.shape, c["land"], dtype=np.float64)
    for hx, hz, r, hh in c["hills"]:
        h = h + hh * np.exp(-((x - hx) ** 2 + (z - hz) ** 2) / (r * r))
    for top, edge, ring in c["plateaus"]:
        d = outside(x, z, ring)
        w = 1.0 - np.clip(d / max(edge, 0.01), 0.0, 1.0)
        w = w * w * (3 - 2 * w)
        h = h + (top - h) * w
    return h


def outside(x, z, ring):
    """How far (x, z) is outside the polygon `ring`, metres; 0 inside."""
    d = np.full(x.shape, np.inf)
    hit = np.zeros(x.shape, dtype=bool)
    n = len(ring)
    for e in range(n):
        (xa, za), (xb, zb) = ring[e], ring[(e + 1) % n]
        ex, ez = xb - xa, zb - za
        t = np.clip(((x - xa) * ex + (z - za) * ez) / max(ex * ex + ez * ez, 1e-9), 0.0, 1.0)
        d = np.minimum(d, np.hypot(x - (xa + ex * t), z - (za + ez * t)))
        hit ^= ((za > z) != (zb > z)) & (x < ex * (z - za) / (zb - za + 1e-12) + xa)
    return np.where(hit, 0.0, d)


def pchip(xs, ys, period):
    """Slopes for a monotone cubic through (xs, ys) on a loop `period` long
    (Fritsch and Carlson): flat at a point that is a peak, a dip, or level
    with a neighbour, so the curve never overshoots, and a level run between
    equal heights stays level."""
    k = len(xs)
    h = np.array([((xs[(j + 1) % k] - xs[j]) % period) or 1e-6 for j in range(k)])
    d = np.array([(ys[(j + 1) % k] - ys[j]) / h[j] for j in range(k)])
    m = np.zeros(k)
    for j in range(k):
        a, b = d[j - 1], d[j]
        if a * b <= 0:
            continue
        w1, w2 = 2 * h[j] + h[j - 1], h[j] + 2 * h[j - 1]
        m[j] = (w1 + w2) / (w1 / a + w2 / b)
    return h, m


def hermite(s, xs, ys, h, m, period):
    """The cubic through (xs, ys) with slopes m, at each of `s`."""
    k = len(xs)
    out = np.zeros(len(s))
    for j in range(k):
        x0 = xs[j]
        u = (s - x0) % period
        on = u < h[j]
        t = u[on] / h[j]
        y0, y1 = ys[j], ys[(j + 1) % k]
        out[on] = ((2 * t ** 3 - 3 * t ** 2 + 1) * y0 + (t ** 3 - 2 * t ** 2 + t) * h[j] * m[j]
                   + (-2 * t ** 3 + 3 * t ** 2) * y1 + (t ** 3 - t ** 2) * h[j] * m[(j + 1) % k])
    return out


def catmull(pts, step):
    """The closed Catmull-Rom curve through `pts`, a point every `step` m or so."""
    n = len(pts)
    out = []
    for i in range(n):
        p0, p1, p2, p3 = (np.array(pts[(i + d) % n][:2]) for d in (-1, 0, 1, 2))
        k = max(2, int(math.dist(p1, p2) / step))
        t = np.arange(k)[:, None] / k
        out.append(0.5 * (2 * p1 + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t ** 2 + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3))
    return np.concatenate(out)


def ranges(L, runs):
    """Each `(from, to, ...)` stretch with its ends wrapped onto the line:
    `(run, a, b)`, where a > b for one that wraps past the start."""
    for run in runs:
        a, b = run[0] % L, run[1] % L
        yield run, a, b


def in_run(s, a, b):
    return (s >= a) & (s < b) if a <= b else (s >= a) | (s < b)


def smooth_loop(v, window):
    """A circular moving average of `v` over `window` samples, twice."""
    k = np.ones(window) / window
    for _ in range(2):
        pad = np.concatenate([v[-window:], v, v[:window]])
        v = np.convolve(pad, k, mode="same")[window:-window]
    return v


def resolve(c):
    """The frozen road: samples a metre apart round the lap from the start
    line, with everything the engine and the race read from them."""
    plan = [(x, z) for x, z, _, _ in c["points"]]
    dense = catmull(plan, 0.25)
    seg = np.hypot(*np.diff(np.vstack([dense, dense[:1]]), axis=0).T)
    cum = np.concatenate([[0.0], np.cumsum(seg)])
    L = float(cum[-1])
    n = int(round(L))
    s = np.arange(n) * (L / n)
    loop = np.vstack([dense, dense[:1]])
    x = np.interp(s, cum, loop[:, 0])
    z = np.interp(s, cum, loop[:, 1])
    P = np.stack([x, z], axis=1)

    # Each layout point found on the line in order, a little way on from the
    # last, so a point by a crossing stays on its own stretch.
    at = []
    for j, (px, pz) in enumerate(plan):
        if j == 0:
            at.append(0)
            continue
        start = at[-1]
        reach = int(math.dist(plan[j - 1], (px, pz)) * 2) + 10
        idx = np.arange(start, start + reach) % n
        at.append(int(idx[np.argmin(np.hypot(x[idx] - px, z[idx] - pz))]))
    ground = land_at(c, x, z)
    pts = c["points"]
    k = len(pts)
    # The road's height: one monotone cubic through every point's height (its
    # `at`, or the land there and its RISE), so a climb pinned at several
    # points is one even grade and a run between equal heights is level.
    # Between two points without `at` the road also follows the land's shape
    # between them; next to a point with `at` it is the curve alone, so a
    # cliff between a free point and a pinned one can't pull the road up it.
    xs = s[at]
    hs = np.array([lv if lv is not None else ground[at[j]] + rise for j, (_, _, rise, lv) in enumerate(pts)])
    hk, mk = pchip(xs, hs, L)
    y = hermite(s, xs, hs, hk, mk, L)
    gk, gm = pchip(xs, ground[at], L)
    land_curve = hermite(s, xs, ground[at], gk, gm, L)
    follow = np.zeros(n)
    flat = np.zeros(n, dtype=bool)
    for j in range(k):
        i0, i1 = at[j], at[(j + 1) % k]
        span = (i1 - i0) % n
        free0, free1 = pts[j][3] is None, pts[(j + 1) % k][3] is None
        for q in range(span + 1):
            i = (i0 + q) % n
            if q < span or not span:
                follow[i] = 1.0 if free0 and free1 else 0.0
            flat[i] = not free0 and not free1
    y = y + follow * (ground - land_curve)

    # Jumps: the road curves up to a lip at S and falls back down a slip
    # face (`jump_profile`), unbanked either side.
    nobank = flat.copy()
    for S, H, LEN in c["jumps"]:
        y = y + jump_profile((s - (S - LEN)) % L, H, LEN)
        nobank |= ((s - (S - LEN - 10)) % L) < LEN + jump_back(H) + 30

    # Landing hills: from A (the take-off) to B the road is shaped to the
    # flight of a kart leaving A at V m/s along the road's slope there: GAP m
    # under its path at most, meeting it where the path has fallen to the
    # steepest a landing slope may be (LANDING_SLOPE), then running on down
    # that slope and easing out to the road at B. A kart flies down the hill
    # and touches down along it, as a ski jumper does.
    for A, B, V, GAP in c["landings"]:
        a, b = A % L, B % L
        on = in_run(s, a, b)
        d = (s - a) % L
        span = (b - a) % L
        ia = int(np.argmin(np.abs(s - a)))
        ib = int(np.argmin(np.abs(s - b)))
        # Off a jump's lip at A, the kart leaves along the lip's slope.
        lip = [(S, H, LEN) for S, H, LEN in c["jumps"] if abs((S - A + L / 2) % L - L / 2) < 1.5]
        if lip:
            S_, H_, LEN_ = lip[0]
            i_lip = int(np.argmin(np.abs(s - S_ % L)))
            y0 = float(y[i_lip])
            base = i_lip - int(LEN_) - 2
            k0 = float((y[base] - y[base - 10]) / 10.0) + 2 * H_ / LEN_
        else:
            y0 = float(y[ia])
            k0 = float((y[ia] - y[ia - 3]) / max(s[ia] - s[ia - 3], 1e-3))
        # Where the path is falling at LANDING_SLOPE: there it meets the hill.
        dl = max((k0 + LANDING_SLOPE) * V * V / KART_G, 10.0)
        fly = y0 + k0 * d - KART_G * d * d / (2 * V * V)
        hill = fly - GAP * np.sin(np.pi * np.clip(d / dl, 0.0, 1.0))
        # Past touchdown: on down the hill at LANDING_SLOPE, then a curve
        # OUTRUN m long eases it level onto the road at B's height, and it
        # runs on level to B.
        y1 = y0 + k0 * dl - KART_G * dl * dl / (2 * V * V)
        yb = float(y[ib])
        bend = LANDING_SLOPE * OUTRUN / 2
        d2 = dl + max(y1 - (yb + bend), 0.0) / LANDING_SLOPE
        q = np.clip(d - d2, 0.0, OUTRUN)
        out = np.where(d < d2, y1 - LANDING_SLOPE * (d - dl), yb + bend - (LANDING_SLOPE * q - LANDING_SLOPE * q * q / (2 * OUTRUN)))
        if d2 + OUTRUN > span:
            raise LayoutError(f"the landing hill {A:g} to {B:g} needs {d2 + OUTRUN:.0f} m to come down level; it has {span:.0f}")
        y = np.where(on, np.where(d < dl, hill, out), y)
        nobank |= on
        c.setdefault("landing_runs", []).append((a, b, dl))

    # Fords: the road dips DEPTH under the water's level over the stretch,
    # eased in and out over its first and last quarter, unbanked.
    for (A, B, D, _), a, b in ranges(L, c["fords"]):
        span = (b - a) % L
        u = ((s - a) % L) / max(span, 1.0)
        on = in_run(s, a, b)
        ease = np.clip(np.minimum(u, 1.0 - u) / 0.25, 0.0, 1.0)
        y = y - np.where(on, D * (0.5 - 0.5 * np.cos(np.pi * ease)), 0.0)
        nobank |= on

    width = np.full(n, c["width"])
    for (_, _, w), a, b in ranges(L, c["widths"]):
        width[in_run(s, a, b)] = w
    if c["widths"]:
        width = smooth_loop(width, 10)
    wall = np.full(n, c["walls"])
    for (_, _, h), a, b in ranges(L, c["wall_runs"]):
        wall[in_run(s, a, b)] = h
    runoff = np.where(wall > 0, 0.0, RUNOFF)
    for (_, _, _, r), a, b in ranges(L, c["verge"]):
        if r is not None:
            runoff[in_run(s, a, b)] = r
    bank_k = np.full(n, c["bank"])
    for (_, _, kk), a, b in ranges(L, c["banks"]):
        bank_k[in_run(s, a, b)] = kk
    # Ramps: the land is built up or cut down to the road, never a deck, and
    # eased out over WIDTH. Tunnels: bored through the land, no walls or
    # run-off (the bore has its own).
    ramp = np.zeros(n, dtype=bool)
    fit = np.full(n, FIT)
    for (_, _, wd), a, b in ranges(L, c["ramps"]):
        on = in_run(s, a, b)
        ramp |= on
        fit[on] = wd
    # Jumps are ramps too: the land is built up under a jump and its slip
    # face, eased out over 24 m either side, so a jump standing high over
    # the land never turns into a deck (whose slab and parapets a kart would
    # hit) and the dunes either side rise to meet it.
    for S, H, LEN in c["jumps"]:
        on = ((s - (S - LEN - 4)) % L) < LEN + jump_back(H) + 8
        ramp |= on
        fit[on] = np.maximum(fit[on], 24.0)
    tunnel = np.zeros(n, dtype=bool)
    bore = np.zeros(n)
    for (_, _, th, _, _), a, b in ranges(L, c["tunnels"]):
        on = in_run(s, a, b)
        tunnel |= on
        bore[on] = th
    wall[tunnel] = 0.0
    runoff[tunnel] = 0.0

    # Banking, as the circuit event works it out: the outside of each turn
    # lifted by how hard it turns, eased out over 10 m round a flat stretch.
    a_, b_, c_ = np.roll(P, 4, axis=0), P, np.roll(P, -4, axis=0)
    d1 = b_ - a_
    d2 = c_ - b_
    l1, l2 = np.hypot(*d1.T), np.hypot(*d2.T)
    u1, u2 = d1 / np.maximum(l1, 1e-6)[:, None], d2 / np.maximum(l2, 1e-6)[:, None]
    cross = u1[:, 0] * u2[:, 1] - u1[:, 1] * u2[:, 0]
    dot = (u1 * u2).sum(axis=1)
    turn = -np.arctan2(cross, dot) / np.maximum((l1 + l2) * 0.5, 0.5)
    t13 = sum(np.roll(turn, j) for j in range(-6, 7)) / 13.0
    near_flat = sum(np.roll(nobank, j).astype(float) for j in range(-10, 11)) / 21.0
    unflat = 1.0 - np.minimum(near_flat * 2.0, 1.0)
    bank = -np.clip(t13 * 4.0, -0.18, 0.18) * bank_k * np.where(nobank, 0.0, unflat)
    # Fixed-angle banks: the outside of the turn lifted to a set slope,
    # eased in and out over 15 m.
    for (_, _, deg), a, b in ranges(L, c.get("bank_deg", [])):
        on = in_run(s, a, b)
        span = (b - a) % L
        u = ((s - a) % L) / max(span, 1.0)
        ease = np.clip(np.minimum(u, 1.0 - u) * span / 15.0, 0.0, 1.0)
        k_ = math.tan(math.radians(deg)) * (0.5 - 0.5 * np.cos(np.pi * ease))
        fixed = -np.sign(t13) * k_
        bank = np.where(on, fixed, bank)
        # Tipped about its inner edge, not its middle: the inside stays at
        # the road's own height and the outside climbs the wall, so the bowl
        # never sinks into a trench. The road rises to it over LIFT_EASE m
        # either side.
        top = math.tan(math.radians(deg)) * (float(width[(int(a) + int(span / 2)) % n]) / 2 + 1.0)
        into = np.minimum((s - (a - LIFT_EASE)) % L, (b + LIFT_EASE - s) % L)
        near = in_run(s, (a - LIFT_EASE) % L, (b + LIFT_EASE) % L)
        lift_u = np.clip(into / LIFT_EASE, 0.0, 1.0)
        y = y + np.where(near, top * (0.5 - 0.5 * np.cos(np.pi * lift_u)), 0.0)
        nobank |= near & ~on


    # Decks: the road more than DECK_ABOVE over the land, in runs of three or
    # more, each reaching two samples past its ends.
    high = (y - ground > DECK_ABOVE) & ~ramp & ~tunnel
    deck = np.zeros(n, dtype=bool)
    i = 0
    while i < n:
        if not high[i]:
            i += 1
            continue
        a = i
        while i < n and high[i]:
            i += 1
        if i - a >= 3:
            deck[np.arange(a - 2, i + 2) % n] = True
    deck &= ~tunnel

    # Covered: inside an `add` shape over the road (a tunnel's block). The
    # engine eases no ground there, and the fitted land isn't pulled down to
    # the road, so the hill the tunnel goes through stays a hill.
    covered = np.zeros(n, dtype=bool)
    for op, words, _, _ in c["shapes"]:
        if op != "add":
            continue
        (x0, y0, z0), (x1, y1, z1) = shapes.bounds(words)
        covered |= (x >= x0) & (x <= x1) & (z >= z0) & (z <= z1) & (y0 <= y + 1.0) & (y1 >= y + 2.5)

    # Checkpoints: the layout's, or a third and two thirds of the way round.
    checks = sorted(cp % L for cp in c["checkpoints"]) or [L / 3, 2 * L / 3]

    return {"n": n, "L": L, "s": s, "x": x, "z": z, "y": y, "bank": bank, "width": width, "wall": wall,
            "runoff": runoff, "deck": deck, "covered": covered, "ground": ground, "at": at, "checks": checks,
            "tunnel": tunnel, "bore": bore, "fit": fit}


def along(r, S, off):
    """The point `off` metres right of the centreline (negative: left) at lap
    distance S, and the road's bearing there (degrees clockwise from north)."""
    L, n = r["L"], r["n"]
    i = int(round((S % L) / L * n)) % n
    dx, dz = r["x"][(i + 1) % n] - r["x"][i - 1], r["z"][(i + 1) % n] - r["z"][i - 1]
    d = np.hypot(dx, dz)
    dx, dz = dx / d, dz / d
    # Right of the way round, in x east and z south: (-dz, dx).
    return (r["x"][i] - dz * off, r["z"][i] + dx * off), math.degrees(math.atan2(dx, -dz)) % 360


def curve(r):
    """Curvature at each sample, 1/m."""
    P = np.stack([r["x"], r["z"]], axis=1)
    a, b, cc = np.roll(P, 4, axis=0), P, np.roll(P, -4, axis=0)
    d1, d2 = b - a, cc - b
    ang = np.arctan2(d1[:, 0] * d2[:, 1] - d1[:, 1] * d2[:, 0], (d1 * d2).sum(axis=1))
    return np.abs(ang) / np.maximum((np.hypot(*d1.T) + np.hypot(*d2.T)) * 0.5, 0.01)


# ---- the report -----------------------------------------------------------------------------


def report(c, r):
    """Lines saying what the track is, and the problems with it."""
    n, L, s, x, z, y = r["n"], r["L"], r["s"], r["x"], r["z"], r["y"]
    lines, problems, notes = [], [], []
    walls = r["wall"].max()
    lines.append(f"{c['name']}: {L:.0f} m round, {len(c['points'])} points, road {r['width'].min():g}"
                 + (f" to {r['width'].max():g}" if r["width"].max() > r["width"].min() + 0.01 else "") + " m"
                 + (f", walls up to {walls:g} m" if walls else ", open verges"))
    lines.append("points round the lap: " + ", ".join(f"{s[i]:.0f}" for i in r["at"]))
    k = curve(r)
    radius = 1.0 / np.maximum(k, 1e-6)
    tight = []
    for i in np.argsort(radius):
        if all(min(abs(i - j), n - abs(i - j)) > 30 for j in tight):
            tight.append(int(i))
        if len(tight) == 4:
            break
    lines.append("tightest bends: " + ", ".join(f"{radius[i]:.0f} m at {s[i]:.0f} m ({x[i]:.0f},{z[i]:.0f})" for i in tight))
    if radius[tight[0]] < 12.0:
        problems.append(f"a {radius[tight[0]]:.0f} m bend {s[tight[0]]:.0f} m round: karts want 15 m or more")

    # Grades over 10 m, either way.
    g = (np.roll(y, -10) - y) / 10.0
    jumps = [(S % L, H, LEN) for S, H, LEN in c["jumps"]]
    lips = np.zeros(n, dtype=bool)
    for S, H, LEN in jumps:
        lips |= ((s - (S - LEN)) % L) < LEN + jump_back(H) + 2
    # A jump's ramp and slip face are steep on purpose.
    # So are the banks of a ford or a flooded channel's dip.
    for A, B, D, _ in c["fords"]:
        lips |= in_run(s, A % L, B % L)
    for a_, b_, _ in c.get("landing_runs", []):
        lips |= in_run(s, a_, b_)
    for A, B, _ in c.get("bank_deg", []):
        lips |= in_run(s, (A - LIFT_EASE) % L, (B + LIFT_EASE) % L)
    near_lip = sum(np.roll(lips, j) for j in range(-10, 1)).astype(bool)
    worst = int(np.argmax(np.where(near_lip, 0.0, np.abs(g))))
    lines.append(f"climb {y.max() - y.min():.1f} m, lowest {y.min():.1f}, highest {y.max():.1f}; "
                 f"steepest grade {abs(g[worst]) * 100:.0f}% {'up' if g[worst] > 0 else 'down'} at {s[worst]:.0f} m"
                 + (" (away from the jumps)" if jumps else ""))
    steep = (np.abs(g) > 0.15) & ~near_lip
    if steep.any():
        i = int(np.argmax(np.where(steep, np.abs(g), 0)))
        problems.append(f"a {abs(g[i]) * 100:.0f}% grade {s[i]:.0f} m round ({x[i]:.0f},{z[i]:.0f}): keep within 15%")

    # Two stretches close on the plan: a crossing if one is well over the
    # other, a problem if not.
    clear = 2 * r["width"].max() / 2 + 2.0 * (2.2 if walls else 1.0) + 4.0
    P = np.stack([x, z], axis=1)
    crossings, close = [], None
    for i in range(0, n, 2):
        j = np.arange(i + 40, n, 2)
        j = j[n - (j - i) >= 40]
        if not len(j):
            continue
        d = np.hypot(*(P[j] - P[i]).T)
        for jj in j[d < clear]:
            gap = abs(y[i] - y[jj])
            low = i if y[i] < y[jj] else jj
            need = r["bore"][low] + 1.5 if r["tunnel"][low] else CROSS_CLEAR
            if gap >= need:
                if all(math.dist(P[i], q) > 50 for q, _ in crossings):
                    crossings.append((P[i], gap))
            else:
                dd = math.dist(P[i], P[jj])
                if close is None or dd < close[0]:
                    close = (dd, i, gap, need)
    for q, gap in crossings:
        lines.append(f"crossing at {q[0]:.0f},{q[1]:.0f}: {gap:.1f} m between the roads")
    if close:
        i = close[1]
        problems.append(f"two stretches of road {close[0]:.0f} m apart near {x[i]:.0f},{z[i]:.0f} ({s[i]:.0f} m round) with "
                        f"{close[2]:.1f} m between their heights: they need {clear:.0f} m apart, or {close[3]:g} m one over the other")
    decks = int(r["deck"].sum())
    if decks:
        lines.append(f"{decks} m on decks")

    edge = 20.0
    if ((x < edge) | (z < edge) | (x > c["size"] - edge) | (z > c["size"] - edge)).any():
        problems.append(f"the road comes within {edge:g} m of the world's edge")
    for j, (px, pz, rise, lv) in enumerate(c["points"]):
        if lv is None and abs(rise) > 2.8:
            problems.append(f"point {px:g},{pz:g}: a rise of {rise:g} m leaves a gap under the road (give it `at Y` for a deck)")

    # A carve across the road whose floor is below it digs a hole in it.
    for op, words, _, line_no in c["shapes"]:
        if op != "carve":
            continue
        (x0, y0, z0), (x1, y1, z1) = shapes.bounds(words)
        hit = (x >= x0) & (x <= x1) & (z >= z0) & (z <= z1) & (y0 < y - 0.13) & (y1 > y)
        if hit.any():
            i = int(np.argmax(hit))
            problems.append(f"line {line_no}: the carve's floor ({y0:g}) is under the road ({y[i]:.2f}) {s[i]:.0f} m round: "
                            f"start it at the road's height")

    # Tunnels: how long, and where the track's own land over one is too low
    # to cover it (the world may raise it; else the lining shows, a gallery).
    fitted = fitted_ground(c, r) if c["tunnels"] else None
    for a0, b0, th, _, _ in c["tunnels"]:
        on = np.flatnonzero(in_run(s, a0 % L, b0 % L))
        if not len(on):
            continue
        N = fitted.shape[0]
        g = fitted[np.clip(z[on].astype(int), 0, N - 1), np.clip(x[on].astype(int), 0, N - 1)]
        covered = g >= y[on] + th + 1.0
        if covered.any():
            first, last = s[on][np.argmax(covered)], s[on][len(on) - 1 - np.argmax(covered[::-1])]
            cover = f"the track's land covers {int(covered.sum())} m of it, {first:.0f} to {last:.0f}"
        else:
            cover = "the track's land covers none of it"
        lines.append(f"tunnel {a0 % L:.0f} to {b0 % L:.0f} m, {len(on)} m long, {th:g} m high; {cover}"
                     + ("; the rest stands out of the land as a lined gallery" if not covered.all() else ""))

    # An `add` that stands into the road's band and isn't cut away by a later
    # carve is a step no kart climbs.
    grids = []
    for op, words, _, line_no in c["shapes"]:
        (x0, y0, z0), (x1, y1, z1) = shapes.bounds(words)
        lo = [int(np.floor(v / shapes.V)) - 1 for v in (x0, y0, z0)]
        hi = [int(np.ceil(v / shapes.V)) + 1 for v in (x1, y1, z1)]
        sx, sy, sz = (max(1, h_ - l_) for l_, h_ in zip(lo, hi))
        if sx * sy * sz > 30_000_000:
            grids.append((op, line_no, None, lo, (sx, sy, sz)))
            continue
        grids.append((op, line_no, shapes.inside(words, (sy, sz, sx), [v * shapes.V for v in lo]), lo, (sx, sy, sz)))

    def solid(g, px, py, pz):
        _, _, grid, lo, (sx, sy, sz) = g
        ix, iy, iz = int(np.floor(px / shapes.V)) - lo[0], int(np.floor(py / shapes.V)) - lo[1], int(np.floor(pz / shapes.V)) - lo[2]
        if grid is None or not (0 <= ix < sx and 0 <= iy < sy and 0 <= iz < sz):
            return False
        return bool(grid[iy, iz, ix])

    for gi, g in enumerate(grids):
        op, line_no, grid, lo, (sx, sy, sz) = g
        if op != "add" or grid is None or line_no in c.get("drive", ()):
            continue
        x0, z0 = lo[0] * shapes.V, lo[2] * shapes.V
        x1, z1 = x0 + sx * shapes.V, z0 + sz * shapes.V
        near = np.flatnonzero((x > x0 - 10) & (x < x1 + 10) & (z > z0 - 10) & (z < z1 + 10))
        hit = None
        for i in near:
            dx, dz = x[(i + 1) % n] - x[i - 1], z[(i + 1) % n] - z[i - 1]
            dl = math.hypot(dx, dz) or 1.0
            rx, rz = -dz / dl, dx / dl
            half = r["width"][i] / 2
            for off in np.arange(-half, half + 0.01, 1.0):
                px, pz = x[i] + rx * off, z[i] + rz * off
                for up in (0.15, 0.6, 1.2):
                    py = y[i] + r["bank"][i] * (-off) + up
                    if solid(g, px, py, pz) and not any(h[0] == "carve" and solid(h, px, py, pz) for h in grids[gi + 1:]):
                        hit = (i, up)
                        break
                if hit:
                    break
            if hit:
                break
        if hit:
            i, up = hit
            problems.append(f"line {line_no}: the shape stands into the road {s[i]:.0f} m round ({x[i]:.0f},{z[i]:.0f}), "
                            f"{up:g} m over its surface: a kart can't get past it")

    # Jumps: where a kart at top speed lands, and whether a kart leaving the
    # lip anywhere across the road, at any speed from a crawl to flat out,
    # clears every part of the jump with both sides of the kart: a slanted
    # lip is further along on one side, so a kart taking off from one lane
    # flies over the rest of the ramp on the other.
    skews = c.get("skews", {})

    def surface(sq, across, S, H, LEN, tan):
        """The road's height `across` m left of the line at lap distance sq,
        with this jump slanted (the centreline's y has it square)."""
        d0 = (sq - (S - LEN)) % L
        return float(np.interp(sq % L, s, y, period=L)) - float(jump_profile(d0, H, LEN)) + float(jump_profile((d0 + across * tan) % L, H, LEN))

    for k_, (S, H, LEN) in enumerate(jumps):
        tan = math.tan(math.radians(skews.get(k_, 0.0)))
        half = float(r["width"][int(round(S / L * n)) % n]) / 2
        lip = 2 * H / LEN
        worst = (math.inf, 0.0, 0.0, 0.0)
        # The hardest touchdown: how fast the kart meets the ground square
        # on, m/s (its speed times the sine of the angle between its path and
        # the ground). Coming down on a level road from full speed is about
        # 9; flying into the face of a rise is far more.
        hard = (-math.inf, 0.0, 0.0, 0.0)
        land_full = None
        boosted = k_ in c["boost_jumps"]
        top_v = KART_BOOST_V if boosted else KART_V
        for v in ((KART_BOOST_V,) if boosted else ()) + (KART_V, KART_V * 0.75, KART_V * 0.5, KART_V * 0.3):
            vx, vy = v / math.hypot(1, lip), v * lip / math.hypot(1, lip)
            for lane in np.arange(-half + 1.0, half - 0.99, 0.5):
                # Leaving this lane's lip, `lane * tan` before S (lanes are
                # metres left of the line; the right side's lip is further on).
                s0 = S - lane * tan
                top = surface(s0 - 0.01, lane, S, H, LEN, tan)
                t = 0.0
                while t < 4.0:
                    t += 0.01
                    sq = s0 + vx * t
                    hk = top + vy * t - KART_G * t * t / 2
                    centre = surface(sq, lane, S, H, LEN, tan)
                    if hk <= centre:
                        if v == top_v and abs(lane) < 0.25:
                            land_full = sq - s0
                        if t > 0.05:
                            ground = (surface(sq + 0.5, lane, S, H, LEN, tan) - surface(sq - 0.5, lane, S, H, LEN, tan))
                            fall = (vy - KART_G * t) / vx
                            angle = math.atan(ground) - math.atan(fall)
                            hit = math.hypot(vx, vy - KART_G * t) * math.sin(angle)
                            # A boost jump is meant to be flown from the pad: a
                            # kart without it falls short, by design.
                            if hit > hard[0] and (not boosted or v >= KART_BOOST_V):
                                hard = (hit, v, lane, sq - s0)
                        break
                    # Both sides of the kart, 0.7 m either side of its line,
                    # each once it is past its own side's lip (on a slanted
                    # lip one side is still rolling up the ramp as the
                    # other leaves it).
                    for w in (-0.7, 0.7):
                        if sq < s0 - w * tan + 0.05:
                            continue
                        gap = hk - surface(sq, lane + w, S, H, LEN, tan)
                        if gap < worst[0]:
                            worst = (gap, v, lane, sq - s0)
        if land_full is None:
            problems.append(f"the jump at {S:.0f} m: no landing within 4 s")
            continue
        d = land_full
        i = int(round(S / L * n)) % n
        lines.append(f"jump at {S:.0f} m, {H:g} m over {LEN:g} m" + (f", slanted {skews[k_]:g} deg" if skews.get(k_) else "")
                     + f": lands about {d:.0f} m on at {'pad' if boosted else 'full'} speed; closest a wheel comes to the jump in flight {worst[0]:+.2f} m; "
                     f"hardest touchdown {hard[0]:.1f} m/s")
        # A kart off a pad is going faster than a kart can on its own, and
        # comes down harder in proportion.
        if hard[0] > JUMP_HIT * max(1.0, hard[1] / KART_V):
            problems.append(f"the jump at {S:.0f} m: a kart at {hard[1] * 3.6:.0f} km/h leaving it {abs(hard[2]):.0f} m "
                            f"{'left' if hard[2] > 0 else 'right'} of the line comes down {hard[3]:.0f} m on at "
                            f"{hard[0]:.1f} m/s square on (keep within {JUMP_HIT:g}): it flies into a rise")
        if worst[0] < -0.15:
            problems.append(f"the jump at {S:.0f} m: a kart at {worst[1] * 3.6:.0f} km/h leaving it {abs(worst[2]):.0f} m "
                            f"{'left' if worst[2] > 0 else 'right'} of the line hits it {worst[3]:.1f} m on ({worst[0]:+.2f} m)")
        span = np.arange(i, i + int(d) + 15) % n
        if radius[span].min() < 40:
            problems.append(f"the jump at {S:.0f} m lands on a bend ({radius[span].min():.0f} m) {s[span[np.argmin(radius[span])]]:.0f} m round")

    for name, at in (("items", [t[0] for t in c["items"]]), ("pad", [t[0] for t in c["pads"]]), ("checkpoint", c["checkpoints"])):
        for S in at:
            if not 0 <= S < L:
                notes.append(f"{name} at {S:g}: past the lap ({L:.0f} m), taken round to {S % L:.0f}")
    if c["items"]:
        lines.append("pickups at " + ", ".join(f"{S:.0f}" for S, _, _ in c["items"]))
    if c["pads"]:
        lines.append("pads at " + ", ".join(f"{S:.0f} {side}" for S, side, _ in c["pads"]))
    for A, B, D, water in c["fords"]:
        lines.append(f"{'ford' if water else 'dip'} {A % L:.0f} to {B % L:.0f} m, {D:g} m deep")
    for A, B, ds in c["floods"]:
        lines.append(f"flood {A % L:.0f} to {B % L:.0f} m, by lap " + ", ".join(f"{d:+g}" for d in ds) + " m on the road at its ends")
    for z_ in c["zones"]:
        if z_["kind"] == "thermal":
            lines.append(f"thermal at {z_['at'][0]:g},{z_['at'][1]:g}, {z_['at'][2]:g} m across, lift {z_['value']:g}")
            continue
        lines.append(f"{z_['kind']} {z_['from'] % L:.0f} to {z_['to'] % L:.0f} m: {z_.get('species', '')} {z_['value']:g}"
                     + (f" toward {z_['bearing']:g} deg" if z_["kind"] == "wind" else "") + (f", gusts of {z_['gust']:g} s" if z_.get("gust") else ""))
    if c["smash"]:
        lines.append("smashables: " + ", ".join(f"{p['kind']} at {p['s'] % L:.0f} {p['off']:+g}" for p in c["smash"]))
    for a_ in c.get("arches", []):
        made = arch_shape(c, r, a_)
        clear = made[1] if made else 0.0
        lines.append(f"arch at {a_['s'] % L:.0f} m: {clear:.1f} m clear over the kerbs")
        if clear < 5.0:
            problems.append(f"line {a_['line']}: the arch at {a_['s'] % L:.0f} m is only {clear:.1f} m over the kerbs: make it higher or wider")
    lines.append("checkpoints at " + ", ".join(f"{cp:.0f}" for cp in r["checks"]))
    lines.append(f"shapes {len(c['shapes'])}, structures {len(c['structures'])}")
    lines.append(f"laps {c['laps']}, rivals {c['rivals']}, vehicle {c['vehicle']}; a race is about {L * c['laps'] / 25:.0f} s")
    lines += notes
    lines += [f"PROBLEM: {p}" for p in problems] or ["no problems"]
    return lines, problems


# ---- the files ------------------------------------------------------------------------------


def fitted_ground(c, r):
    """The land as the track sees it, with the road's surroundings eased to
    meet the road: 1 m cells, rows north to south. Decks and covered
    stretches don't pull (the land stays under a bridge and round a tunnel)."""
    N = int(round(c["size"]))
    cc = np.arange(N) + 0.5
    X, Z = np.meshgrid(cc, cc)
    land = land_at(c, X, Z)
    pull = ~r["deck"] & ~r["covered"] & ~r["tunnel"]
    inner = r["width"] / 2 + 1.0 + np.where(r["wall"] > 0, WALL_THICK, 0.0) + r["runoff"]
    # Shoulder: where the land is lower than the road it is filled up level
    # with it for this far past the run-off before it eases away, so a road
    # never has a ditch or a drop beside it (kart tracks don't); land higher
    # than the road eases down to it as before, a bank or a cliff.
    shoulder = c.get("shoulder", SHOULDER)
    reach = inner + r["fit"] + shoulder

    def nearest(which):
        """Each cell's nearest point among `which` within its reach: (distance, index)."""
        best = np.full((N, N), np.inf)
        idx = np.full((N, N), -1)
        for i in np.flatnonzero(which):
            px, pz, rr = r["x"][i], r["z"][i], reach[i]
            x0, x1 = max(0, int(px - rr)), min(N, int(px + rr) + 2)
            z0, z1 = max(0, int(pz - rr)), min(N, int(pz + rr) + 2)
            if x0 >= x1 or z0 >= z1:
                continue
            d = np.hypot(X[z0:z1, x0:x1] - px, Z[z0:z1, x0:x1] - pz)
            win, wi = best[z0:z1, x0:x1], idx[z0:z1, x0:x1]
            m = (d < rr) & (d < win)
            win[m] = d[m]
            wi[m] = i
        return best, idx

    # As the engine lays the road: a cell nearest a deck, a tunnel or a
    # covered point is left to it, unless a stretch of road more than 3 m
    # above or below is in reach, which pulls it.
    d_all, i_all = nearest(np.ones(r["n"], dtype=bool))
    d_pull, i_pull = nearest(pull)
    have = i_all >= 0
    own = have & pull[np.maximum(i_all, 0)]
    other = have & ~own & (i_pull >= 0) & (np.abs(r["y"][np.maximum(i_pull, 0)] - r["y"][np.maximum(i_all, 0)]) > 3.0)
    use = own | other
    k = np.maximum(i_pull, 0)
    fit = np.maximum(r["fit"][k], 1e-3)
    lower = land < r["y"][k]
    t_up = np.clip((inner[k] + shoulder + fit - d_pull) / fit, 0.0, 1.0)
    t_dn = np.clip((inner[k] + fit - d_pull) / fit, 0.0, 1.0)
    t = np.where(lower, t_up, t_dn)
    w = np.where(use, t * t * (3 - 2 * t), 0.0)
    return (land + (r["y"][k] - land) * w).astype("<f4")


def r2(v, k=2):
    v = round(float(v), k)
    return int(v) if v == int(v) else v


VOX = 4  # voxels a metre


def merge_columns(cols, y0, y1):
    """Voxel columns {(x, z)} from y0 to y1 as boxes [x0, y0, z0, x1, y1, z1],
    runs along z merged."""
    out = []
    by_x = {}
    for x, z in cols:
        by_x.setdefault(x, []).append(z)
    for x in sorted(by_x):
        zs = sorted(by_x[x])
        start = prev = zs[0]
        for z in zs[1:] + [None]:
            if z is not None and z == prev + 1:
                prev = z
                continue
            out.append([x, y0, start, x + 1, y1, prev + 1])
            if z is not None:
                start = prev = z
    return out


def water_zone(c, r, A, B, depths):
    """A flood zone over the stretch A to B: the water's level (the road at
    the stretch's ends), its depth over that each lap, and the voxel boxes
    it may fill (the corridor, from the lowest road up), which the race
    fills only where there is air (race/course.rs)."""
    L, n, s = r["L"], r["n"], r["s"]
    a, b = A % L, B % L
    on = np.flatnonzero(in_run(s, a, b))
    if not len(on):
        raise LayoutError(f"the stretch {A:g} to {B:g} has no road on it")
    ends = [on[0], on[-1]] if a <= b else [int(np.argmin(np.abs(s - a))), int(np.argmin(np.abs(s - b)))]
    level = float(min(r["y"][ends[0]], r["y"][ends[-1]]))
    cols = set()
    for i in on:
        dx, dz = r["x"][(i + 1) % n] - r["x"][i - 1], r["z"][(i + 1) % n] - r["z"][i - 1]
        dl = math.hypot(dx, dz) or 1.0
        rx, rz = -dz / dl, dx / dl
        reach = r["width"][i] / 2 + 1.0 + max(r["runoff"][i], 2.0) + 4.0
        for off in np.arange(-reach, reach + 0.01, 0.2):
            for along_ in (-0.5, 0.0, 0.5):
                px = r["x"][i] + rx * off + dx / dl * along_
                pz = r["z"][i] + rz * off + dz / dl * along_
                cols.add((int(math.floor(px * VOX)), int(math.floor(pz * VOX))))
    floor = int(math.floor((r["y"][on].min() - 1.0) * VOX))
    top = int(math.ceil((level + max(depths)) * VOX)) + 1
    return {"from": r2(a, 1), "to": r2(b, 1), "kind": "flood", "mat": "river_water", "laps": [r2(d) for d in depths], "level": r2(level),
            "area": merge_columns(cols, floor, max(top, floor + 1))}


def smash_boxes(c, r, p):
    """A smashable prop's voxel boxes, world voxels: a block LEN along the
    road and its width across, standing on the road (or its verge) at its
    place, of its parts (a gate is two posts and three rails)."""
    L, n = r["L"], r["n"]
    ln, wd, h, mat, slow = SMASH[p["kind"]]
    ln = p["len"] or ln
    (px, pz), bearing = along(r, p["s"], p["off"])
    i = int(round((p["s"] % L) / L * n)) % n
    half = r["width"][i] / 2 + 1.0
    base = r["y"][i] - r["bank"][i] * float(np.clip(p["off"], -half, half))
    y0 = int(round(base * VOX))
    br = math.radians(bearing)
    fx, fz = math.sin(br), -math.cos(br)      # along the road
    rx, rz = -fz, fx                          # across it, to the right
    if p["kind"] == "gate":
        parts = [(-ln / 2, -ln / 2 + 0.25, 0.0, h), (ln / 2 - 0.25, ln / 2, 0.0, h),
                 (-ln / 2, ln / 2, 0.25, 0.5), (-ln / 2, ln / 2, 0.625, 0.875), (-ln / 2, ln / 2, 1.0, 1.25)]
    else:
        parts = [(-ln / 2, ln / 2, 0.0, h)]
    boxes = []
    for u0, u1, h0, h1 in parts:
        cols = set()
        reach = max(abs(u0), abs(u1)) + wd
        for vx in range(int(math.floor((px - reach) * VOX)), int(math.ceil((px + reach) * VOX)) + 1):
            for vz in range(int(math.floor((pz - reach) * VOX)), int(math.ceil((pz + reach) * VOX)) + 1):
                qx, qz = (vx + 0.5) / VOX - px, (vz + 0.5) / VOX - pz
                u, v = qx * fx + qz * fz, qx * rx + qz * rz
                if u0 <= u <= u1 and abs(v) <= wd / 2:
                    cols.add((vx, vz))
        if cols:
            boxes += merge_columns(cols, y0 + int(round(h0 * VOX)), y0 + int(round(h1 * VOX)))
    return {"boxes": boxes, "mat": p["mat"] or mat or c["mats"]["wall"], "slow": p["slow"] or slow}


def arch_shape(c, r, a):
    """An arch over the road at lap distance a["s"]: two legs standing just
    outside the run-off and a span over the road, a["thick"] m deep along
    it, the opening a["span"] m across and a["height"] m clear at its
    middle, its underside a half ellipse and its top a squared one. Voxel
    boxes, as a shape for track.json."""
    L, n = r["L"], r["n"]
    i = int(round((a["s"] % L) / L * n)) % n
    (px, pz), bearing = along(r, a["s"], 0.0)
    base = float(r["y"][i])
    half_road = float(r["width"][i]) / 2 + 1.0
    hi = (a["span"] / 2) if a["span"] else half_road + 2.0
    ho = hi + 3.0
    H = a["height"]
    top_h = H + 3.0
    br = math.radians(bearing)
    fx, fz = math.sin(br), -math.cos(br)
    rx, rz = -fz, fx
    reach = ho + a["thick"]
    lo_y = int(math.floor((base - 3.0) * VOX))
    cols = []
    for vx in range(int(math.floor((px - reach) * VOX)), int(math.ceil((px + reach) * VOX)) + 1):
        for vz in range(int(math.floor((pz - reach) * VOX)), int(math.ceil((pz + reach) * VOX)) + 1):
            qx, qz = (vx + 0.5) / VOX - px, (vz + 0.5) / VOX - pz
            u, v = qx * fx + qz * fz, abs(qx * rx + qz * rz)
            if abs(u) > a["thick"] / 2 or v > ho:
                continue
            top = base + top_h * max(1.0 - (v / ho) ** 4, 0.0) ** 0.25
            if v >= hi:
                y0 = base - 3.0
            else:
                y0 = base + H * max(1.0 - (v / hi) ** 2, 0.0) ** 0.5
            if top > y0 + 0.2:
                cols.append((vx, vz, int(math.floor(y0 * VOX)), int(math.ceil(top * VOX))))
    if not cols:
        return None
    x0 = min(cc[0] for cc in cols)
    z0 = min(cc[1] for cc in cols)
    y_lo = min(cc[2] for cc in cols)
    boxes = [[cx - x0, cy0 - y_lo, cz - z0, cx - x0 + 1, cy1 - y_lo, cz - z0 + 1] for cx, cz, cy0, cy1 in cols]
    size = [max(bb[3] for bb in boxes), max(bb[4] for bb in boxes), max(bb[5] for bb in boxes)]
    clear = H * max(1.0 - (half_road / hi) ** 2, 0.0) ** 0.5
    return {"mat": a["mat"] or c["mats"]["wall"], "at": [x0, y_lo, z0], "size": size, "boxes": boxes, "line": a["line"]}, clear


def track_json(c, r):
    L, n = r["L"], r["n"]
    rows = [[r2(r["x"][i]), r2(r["z"][i]), r2(r["y"][i]), r2(r["bank"][i], 4), r2(r["width"][i]), r2(r["wall"][i]),
             r2(r["runoff"][i], 1), int(r["deck"][i]) | (2 * int(r["covered"][i])) | (4 * int(r["tunnel"][i]))] for i in range(n)]
    out = {
        "_generated_by": "tools/course/track_file.py",
        "notes": "The track as the engine stamps it (tools/course/FORMAT.md). Generated: edit the .track file, not this.",
        "name": f"{c['name']}_ring",
        "source": c["source"],
        "length": r2(L),
        "line": rows,
        "mats": {k: v for k, v in c["mats"].items() if k not in ("post", "portal", "light") and (k != "pad" or c["pads"])},
        "gate": {"light": "line_white", "dark": "line_black", "post": c["mats"]["post"], "grid": c["grid"]},
        "checkpoints": [r2(cp, 1) for cp in r["checks"]],
    }
    if c["lights"]:
        out["lights"] = c["lights"]
    zones = list(c["zones"])
    for A, B, D, water in c["fords"]:
        if water:
            zones.append(water_zone(c, r, A, B, [0.0]))
    for A, B, ds in c["floods"]:
        zones.append(water_zone(c, r, A, B, ds))
    # The rise to a fixed-angle bank is steep on purpose too.
    for (A, B, _), a_, b_ in ranges(r["L"], c.get("bank_deg", [])):
        zones.append({"from": (a_ - LIFT_EASE) % r["L"], "to": (b_ + LIFT_EASE) % r["L"], "kind": "landing"})
    # A landing hill is steep on purpose: the engine's grade check leaves it be.
    for a_, b_, _ in c.get("landing_runs", []):
        zones.append({"from": a_, "to": b_, "kind": "landing"})
    for z_ in zones:
        z_["from"], z_["to"] = r2(z_["from"] % r["L"], 1), r2(z_["to"] % r["L"], 1)
    if zones:
        out["zones"] = zones
    if c["smash"]:
        out["smash"] = [smash_boxes(c, r, p) for p in c["smash"]]
    if c["surface"]:
        out["surfaces"] = [[r2(a % L, 1), r2(b % L, 1), m] for a, b, m in c["surface"]]
    verges = [[r2(a % L, 1), r2(b % L, 1), m] for a, b, m, _ in c["verge"] if m]
    if verges:
        out["verges"] = verges
    if c["items"]:
        out["items"] = [{"s": r2(S % L, 1), "across": a, "kinds": kinds} for S, a, kinds in c["items"]]
    if c["pads"]:
        out["pads"] = [{"s": r2(S % L, 1), "side": SIDES[side], "len": LEN} for S, side, LEN in c["pads"]]
    if c["jumps"]:
        skews = c.get("skews", {})
        out["jumps"] = [{"s": r2(S % L, 1), "height": H, "len": LEN, **({"skew": skews[k]} if skews.get(k) else {})}
                        for k, (S, H, LEN) in enumerate(c["jumps"])]
    if c["tunnels"]:
        out["tunnels"] = [{"from": r2(a % L, 1), "to": r2(b % L, 1), "height": h, "mat": m or c["mats"]["wall"],
                           "portal": pm or c["mats"]["portal"], "light": c["mats"]["light"]} for a, b, h, m, pm in c["tunnels"]]
    shp = []
    for op, words, mat, line_no in c["shapes"]:
        lo, size, boxes = shapes.boxes(words)
        if not boxes:
            print(f"line {line_no}: {op} {words[0]} covers no voxel")
            continue
        shp.append({"mat": "carve" if op == "carve" else (mat or c["mats"]["wall"]), "at": lo, "size": size, "boxes": boxes,
                    "line": line_no})
    for a_ in c.get("arches", []):
        made = arch_shape(c, r, a_)
        if made:
            shp.append(made[0])
    if shp:
        out["shapes"] = shp
    sts = []
    for s_ in c["structures"]:
        e = {"name": s_["name"]}
        if "along" in s_:
            (px, pz), b = along(r, s_["along"], s_["off"])
            e["at"] = [r2(px), r2(pz)]
            # Facing the road unless turned: the front looks back across it.
            facing = (b + (90.0 if s_["off"] < 0 else -90.0) + s_.get("turn", 0.0)) % 360
            e["bearing"] = r2(facing, 1)
        else:
            e["at"] = s_["at"]
            if "bearing" in s_:
                e["bearing"] = s_["bearing"]
        if "y" in s_:
            e["y"] = s_["y"]
        sts.append(e)
    if sts:
        out["structures"] = sts
    return out


def dump(obj):
    """JSON with the line's rows one to a line, so the file is readable and
    diffs well."""
    text = json.dumps({k: v for k, v in obj.items() if k not in ("line", "shapes", "zones", "smash")}, indent=1)[:-2]
    rows = ",\n".join("  " + json.dumps(row, separators=(",", ":")) for row in obj["line"])
    text += ',\n "line": [\n' + rows + "\n ]"
    for key in ("shapes", "zones", "smash"):
        if key in obj:
            sh = ",\n".join("  " + json.dumps(s_, separators=(",", ":")) for s_ in obj[key])
            text += f',\n "{key}": [\n' + sh + "\n ]"
    return text + "\n}\n"


def starter_world(c, r, out):
    """world.json, chronicle.json, story.md and look.json for a new world."""
    st = style(c["style"])
    title = c["title"] or c["name"].replace("_", " ").upper()
    world = {
        "name": c["name"],
        "title": title,
        "blurb": c["blurb"] or f"A kart circuit in {st['story']}.",
        "size_m": c["size"],
        "height_m": 96,
        "sea_level_m": 4,
        "seed": sum(map(ord, c["name"])) * 7 + 11,
        "sun": {"latitude": 50},
        "modes": ["walk", "drive", "race", "screenshot", "camera"],
        "games": ["race"],
        "vehicles": [c["vehicle"]],
        "start_hour": c["hour"],
        **st["world"],
    }
    world["start_season"] = world["seasons"][0]
    chron = {"events": [
        {"year": -1000000, "event": "heightmap", "file": "ground.bin",
         "cause": "the land the track is laid over, fitted to the road by tools/course/track_file.py"},
        {"year": -999999, "event": "terrain", "mode": "add", "base": 0, "hills": c["rolling"], "ridges": 0, "rough": 1,
         "vary": 0.4, "warp": 30, "cause": "rolling over the fitted land"},
    ]}
    (out / "world.json").write_text(json.dumps(world, indent=2) + "\n", encoding="utf-8")
    (out / "chronicle.json").write_text(json.dumps(chron, indent=2) + "\n", encoding="utf-8")
    (out / "story.md").write_text(f"# {title.title()}\n\n{world['blurb']}\n\nThe track is built from {c['source']} "
                                  f"by tools/course/track_file.py; the world round it is this folder's chronicle.\n", encoding="utf-8")
    if st.get("look") and not (out / "look.json").exists():
        shutil.copyfile(ROOT / "worlds" / st["look"] / "look.json", out / "look.json")


def follow_track(c, r, out):
    """The parts of world.json that follow the track: the race, and the
    spawn on the start line facing along the road."""
    path = out / "world.json"
    world = json.loads(path.read_text(encoding="utf-8"))
    bearing = round(math.degrees(math.atan2(r["x"][1] - r["x"][0], -(r["z"][1] - r["z"][0])))) % 360
    world["spawn"] = [r2(r["x"][0]), r2(r["z"][0])]
    world["spawn_bearing"] = bearing
    world["race"] = {"circuit": f"{c['name']}_ring", "track": "track.json", "laps": c["laps"], "rivals": c["rivals"], "vehicle": c["vehicle"]}
    for key in ("modes", "games"):
        if "race" not in world.setdefault(key, []):
            world[key].append("race")
    path.write_text(json.dumps(world, indent=2) + "\n", encoding="utf-8")


def build(c, r, fresh):
    out = ROOT / "worlds" / c["name"]
    new = not (out / "chronicle.json").exists()
    out.mkdir(parents=True, exist_ok=True)
    if new or fresh:
        starter_world(c, r, out)
    follow_track(c, r, out)
    (out / "track.json").write_text(dump(track_json(c, r)), encoding="utf-8")
    (out / "ground.bin").write_bytes(fitted_ground(c, r).tobytes())
    return out, new or fresh


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("layout")
    ap.add_argument("--check", action="store_true", help="report on the layout without writing anything")
    ap.add_argument("--fresh", action="store_true", help="rewrite the starter world (world.json, chronicle.json, story.md) too")
    args = ap.parse_args()
    try:
        c = parse(args.layout)
        r = resolve(c)
        lines, problems = report(c, r)
    except (LayoutError, shapes.ShapeError) as e:
        sys.exit(f"{args.layout}: {e}")
    print("\n".join(lines))
    if args.check:
        sys.exit(1 if problems else 0)
    try:
        out, started = build(c, r, args.fresh)
    except (LayoutError, shapes.ShapeError) as e:
        sys.exit(f"{args.layout}: {e}")
    (out / "report.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)}/track.json and ground.bin" + (" and a starter world" if started else ""))


if __name__ == "__main__":
    main()
