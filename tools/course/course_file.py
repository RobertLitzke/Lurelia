"""Build a kart-racing world from a .course layout (tools/course/FORMAT.md).

    python -I tools/course/course_file.py tools/course/maps/kestrel.course

writes worlds/<name>/: world.json, chronicle.json, story.md, report.txt and,
for a style that has one, look.json. The course is a terrain event (the land
and its hills) and a circuit event (the road, kerbs and walls), plus a world
.json `race` block; the game builds the rest. report.txt is the verdict on
the layout: length, climb, the tightest bends, and any place two stretches of
road come too close.
"""
import argparse
import json
import math
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# What each style dresses a course in. `look` is a world whose look.json the
# course borrows (render grading); None keeps the default.
STYLES = {
    "ice": {
        # Ice all the way down, so the walls' faces are ice and not bedrock.
        "rock": [["glacier_ice", 0.7], ["fall_ice_blue", 0.3]],
        "strata": None,
        "biomes": ["ice_deck", "arena_seabed"],
        "default_biome": "ice_deck",
        "seabed_biome": "arena_seabed",
        "road": "ice_track",
        "kerb": "ice_kerb",
        "wall": "fall_ice_blue",
        # Summer, as the ice arenas are: the ice is in the materials, and a
        # winter would snow over the road.
        "seasons": ["summer"],
        "weather": {"clouds": 1.0, "sun": 0.6},
        "start_weather": "clouds",
        "look": "kaldvik",
        "round_edges": ["fall_ice_blue", "packed_snow"],
        "story": "ice and packed snow under a pale sky",
    },
    "downs": {
        "rock": [["chalk_white", 0.75], ["chalk_grey", 0.25]],
        "strata": None,
        "biomes": ["chalk_down", "feathergrass_sea", "seabed"],
        "default_biome": "chalk_down",
        "seabed_biome": "seabed",
        "road": "road_asphalt",
        "kerb": "track_kerb",
        "wall": "chalk_white",
        "seasons": ["summer"],
        "weather": {"sun": 1.0, "clouds": 0.6},
        "start_weather": "sun",
        "look": None,
        "round_edges": None,
        "story": "chalk downs and short grass",
    },
}


class LayoutError(Exception):
    pass


def parse(path):
    """The course as a dict, from the .course text."""
    c = {"hills": [], "points": [], "laps": 3, "rivals": 3, "vehicle": "kart", "width": 12.0, "walls": 0.0,
         "land": 30.0, "rolling": 4.0, "bank": 1.0, "hour": 14.0, "title": None, "blurb": ""}
    for n, raw in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        w = line.split()
        try:
            k = w[0]
            if k == "course":
                c["name"] = w[1]
                c["size"] = float(w[w.index("size") + 1])
                c["style"] = w[w.index("style") + 1] if "style" in w else "downs"
            elif k in ("title", "blurb"):
                c[k] = line[len(k):].strip()
            elif k == "race":
                for key in ("laps", "rivals"):
                    if key in w:
                        c[key] = int(w[w.index(key) + 1])
                if "vehicle" in w:
                    c["vehicle"] = w[w.index("vehicle") + 1]
            elif k == "road":
                c["width"] = float(w[1])
                if "walls" in w:
                    c["walls"] = float(w[w.index("walls") + 1])
                if "bank" in w:
                    c["bank"] = float(w[w.index("bank") + 1])
            elif k == "land":
                c["land"] = float(w[1])
                if len(w) > 2:
                    c["rolling"] = float(w[2])
            elif k == "hour":
                c["hour"] = float(w[1])
            elif k == "hill":
                x, z, r, h = map(float, w[1:5])
                c["hills"].append((x, z, r, h))
            elif k == "point":
                x, z = float(w[1]), float(w[2])
                rise = float(w[3]) if len(w) > 3 else 0.0
                c["points"].append((x, z, rise))
            else:
                raise LayoutError(f"unknown line `{k}`")
        except (IndexError, ValueError) as e:
            raise LayoutError(f"line {n}: {raw.strip()!r}: {e}") from None
        except LayoutError as e:
            raise LayoutError(f"line {n}: {e}") from None
    if "name" not in c:
        raise LayoutError("no `course NAME size S [style S]` line")
    if c["style"] not in STYLES:
        raise LayoutError(f"style {c['style']}: one of {', '.join(STYLES)}")
    if len(c["points"]) < 4:
        raise LayoutError("a course needs at least four `point` lines")
    return c


def spline(pts, step=1.0):
    """The closed Catmull-Rom line through `pts`, a point every `step` m or so:
    near enough the game's own (world/chronicle/geom.rs) to judge a layout."""
    n = len(pts)
    out = []
    for i in range(n):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[(i + 1) % n], pts[(i + 2) % n]
        k = max(2, int(math.dist(p1, p2) / step))
        for j in range(k):
            t = j / k
            t2, t3 = t * t, t * t * t
            out.append(tuple(0.5 * (2 * p1[c] + (-p0[c] + p2[c]) * t + (2 * p0[c] - 5 * p1[c] + 4 * p2[c] - p3[c]) * t2
                                    + (-p0[c] + 3 * p1[c] - 3 * p2[c] + p3[c]) * t3) for c in (0, 1)))
    return out


def report(c):
    """Lines saying what the layout is, and what is wrong with it."""
    line = spline([(x, z) for x, z, _ in c["points"]])
    n = len(line)
    length = sum(math.dist(line[i], line[(i + 1) % n]) for i in range(n))

    def radius(i, k=4):
        a, b, d = line[i - k], line[i], line[(i + k) % n]
        d1, d2 = (b[0] - a[0], b[1] - a[1]), (d[0] - b[0], d[1] - b[1])
        turn = math.atan2(d1[0] * d2[1] - d1[1] * d2[0], d1[0] * d2[0] + d1[1] * d2[1])
        return ((math.hypot(*d1) + math.hypot(*d2)) / 2) / abs(turn) if turn else 1e9

    radii = [radius(i) for i in range(n)]
    tight = []
    for i in sorted(range(n), key=lambda i: radii[i]):
        if all(abs(i - j) > 30 and n - abs(i - j) > 30 for j in tight):
            tight.append(i)
        if len(tight) == 4:
            break
    out = [f"{c['name']}: {length:.0f} m round, {len(c['points'])} points, road {c['width']:g} m"
           + (f", walls {c['walls']:g} m" if c["walls"] else ", open verges")]
    out.append("tightest bends: " + ", ".join(f"{radii[i]:.0f} m at {line[i][0]:.0f},{line[i][1]:.0f}" for i in tight))
    problems = []
    # Two stretches of road closer than this would share a kerb or a wall.
    clear = c["width"] + 2.0 * (2.2 if c["walls"] else 1.0) + 4.0
    worst = None
    for i in range(0, n, 2):
        for j in range(i + 40, n, 2):
            if n - (j - i) < 40:
                continue
            d = math.dist(line[i], line[j])
            if worst is None or d < worst[0]:
                worst = (d, line[i], line[j])
    if worst:
        out.append(f"closest approach of two stretches: {worst[0]:.0f} m (needs {clear:.0f})")
        if worst[0] < clear:
            problems.append(f"two stretches of road {worst[0]:.0f} m apart near {worst[1][0]:.0f},{worst[1][1]:.0f}: they need {clear:.0f}")
    if radii[tight[0]] < 12.0:
        problems.append(f"a {radii[tight[0]]:.0f} m bend at {line[tight[0]][0]:.0f},{line[tight[0]][1]:.0f}: karts want 15 m or more")
    edge = 20.0
    for x, z, rise in c["points"]:
        if not (edge <= x <= c["size"] - edge and edge <= z <= c["size"] - edge):
            problems.append(f"point {x:g},{z:g} is within {edge:g} m of the world's edge")
        if abs(rise) > 2.8:
            problems.append(f"point {x:g},{z:g}: a rise of {rise:g} m leaves a gap under the road (keep it within 2.8)")
    out.append(f"laps {c['laps']}, rivals {c['rivals']}, vehicle {c['vehicle']}; a race is about {length * c['laps'] / 25:.0f} s at 90 km/h")
    out += [f"PROBLEM: {p}" for p in problems] or ["no problems"]
    return out, problems


def build(c):
    s = STYLES[c["style"]]
    name = c["name"]
    pts = c["points"]
    x0, z0 = pts[0][0], pts[0][1]
    x1, z1 = pts[1][0], pts[1][1]
    bearing = round(math.degrees(math.atan2(x1 - x0, -(z1 - z0)))) % 360
    title = c["title"] or name.replace("_", " ").upper()
    circuit = f"{name}_ring"
    world = {
        "name": name,
        "title": title,
        "blurb": c["blurb"] or f"A kart circuit in {s['story']}.",
        "size_m": c["size"],
        "height_m": 96,
        "sea_level_m": 4,
        "seed": sum(map(ord, name)) * 7 + 11,
        "sun": {"latitude": 50},
        "rock": s["rock"],
        "spawn": [x0, z0],
        "spawn_bearing": bearing,
        "modes": ["walk", "drive", "race", "screenshot", "camera"],
        "games": ["race"],
        "vehicles": [c["vehicle"]],
        "race": {"circuit": circuit, "laps": c["laps"], "rivals": c["rivals"], "vehicle": c["vehicle"]},
        "seasons": s["seasons"],
        "start_season": s["seasons"][0],
        "weather": s["weather"],
        "start_weather": s["start_weather"],
        "start_hour": c["hour"],
        "default_biome": s["default_biome"],
        "seabed_biome": s["seabed_biome"],
        "weathering": 0.0 if c["style"] == "ice" else 0.4,
        "moss": 0.0 if c["style"] == "ice" else 0.2,
        "biomes": s["biomes"],
    }
    if s["strata"]:
        world["strata"] = s["strata"]
    if s["round_edges"]:
        world["round_edges"] = s["round_edges"]
        world["snow_line_m"] = 400
    terrain = {
        "year": -1000000,
        "event": "terrain",
        "cause": "the land the course is laid over",
        "base": c["land"], "hills": c["rolling"], "ridges": 0, "rough": 1, "vary": 0.4, "warp": 30, "scale": 1.0,
        "relief": [{"at": [x, z], "radius": r, "height": h} for x, z, r, h in c["hills"]],
    }
    road = {
        "year": 2000,
        "event": "circuit",
        "cause": f"the {title.title()} circuit laid over it",
        "name": circuit,
        "path": [[x, z] for x, z, _ in pts],
        "rise": [r for _, _, r in pts],
        "width": c["width"],
        "mat": s["road"],
        "kerb": s["kerb"],
        "bank": c["bank"],
    }
    if c["walls"]:
        road["wall"] = c["walls"]
        road["wall_mat"] = s["wall"]
    out = ROOT / "worlds" / name
    out.mkdir(parents=True, exist_ok=True)
    (out / "world.json").write_text(json.dumps(world, indent=2) + "\n", encoding="utf-8")
    chron = {"events": [terrain, road]}
    (out / "chronicle.json").write_text(json.dumps(chron, indent=2) + "\n", encoding="utf-8")
    (out / "story.md").write_text(f"# {title.title()}\n\n{world['blurb']}\n\nBuilt from a course layout by tools/course/course_file.py.\n", encoding="utf-8")
    if s["look"]:
        shutil.copyfile(ROOT / "worlds" / s["look"] / "look.json", out / "look.json")
    elif (out / "look.json").exists():
        (out / "look.json").unlink()
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("layout")
    ap.add_argument("--check", action="store_true", help="report on the layout without writing the world")
    args = ap.parse_args()
    try:
        c = parse(args.layout)
    except LayoutError as e:
        sys.exit(f"{args.layout}: {e}")
    lines, problems = report(c)
    print("\n".join(lines))
    if args.check:
        sys.exit(1 if problems else 0)
    out = build(c)
    (out / "report.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
