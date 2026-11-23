#!/usr/bin/env python3
"""Measure what one library entry costs to draw, before and after a rework.

    python tools/bench_entry.py plants oak
    python tools/bench_entry.py creatures fox --count 60
    python tools/bench_entry.py structures farmhouse --before notes/replaced_structures/farmhouse_v1.json

Builds a flat test world with about --count copies of the entry (plants grow
from a biome that holds only it, structures stand in a grid, creatures are
spawned round the camera with VG_SPAWN), then renders the same views with the
before and the after version and prints a table of the two.

Both versions go in under one name, `zz_bench_subject`, so every placement
seed, position and view is the same and only the entry differs. The world and
a copy of the library live in a sandbox folder (the engine reads `library/`
and `worlds/` from its working directory), so nothing in the checkout is
touched and other sessions never see the test entry.

The quad and card counts are exact: the same run gives the same numbers, and
they are what decides the GPU cost (docs/PERFORMANCE.md). Frame times are
noisy, so each view runs --reps times, before and after alternating, and the
table shows the median.

Before: --before FILE, else the newest notes/replaced_<kind>/NAME_v*.json,
else the committed file (git HEAD). After: --after FILE, else the library file.
"""

import argparse
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KINDS = ("plants", "creatures", "structures")
SUBJECT = "zz_bench_subject"
BIOME = "zz_bench_ground"
WORLD = "zz_bench"
VOXEL = 0.25  # metres per structure voxel

# What the table shows: (label, path into the stats JSON, exact?)
ROWS = [
    ("quads drawn", ("quads_drawn",), True),
    ("shadow quads", ("shadow_quads",), True),
    ("cards drawn", ("sprites_drawn",), True),
    ("models drawn", ("entities_drawn",), True),
    ("model quads", ("entity_quads",), True),
    ("draw calls", ("draws",), True),
    ("quads resident", ("quads_resident",), True),
    ("frame ms p50", ("frame_ms_p50",), False),
    ("gpu wait ms", ("split_avg_ms", "gpu_wait"), False),
    ("tick ms", ("split_avg_ms", "tick"), False),
    ("bench p50 ms", ("bench", "p50_ms"), False),
    ("bench p95 ms", ("bench", "p95_ms"), False),
    ("bench p99 ms", ("bench", "p99_ms"), False),
]


def load_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def find_before(kind, name):
    notes = ROOT / "notes" / f"replaced_{kind}"
    olds = sorted(notes.glob(f"{name}_v*.json"), key=lambda p: int(re.sub(r"\D", "", p.stem.rsplit("_v", 1)[1]) or 0))
    if olds:
        return load_json(olds[-1]), str(olds[-1].relative_to(ROOT))
    try:
        text = subprocess.run(["git", "show", f"HEAD:library/{kind}/{name}.json"], cwd=ROOT, capture_output=True, check=True, text=True, encoding="utf-8").stdout
        return json.loads(text), f"git HEAD:library/{kind}/{name}.json"
    except subprocess.CalledProcessError:
        sys.exit(f"no before version of {name}: pass --before FILE")


def plant_spacing(entries):
    # The whole crown comes out 1.5-2x crown_radius (worldgen reference), so
    # leave about four lobe radii between trunks, from the wider version.
    widest = max(max(e.get("crown_radius", [1.0, 1.0])) for e in entries)
    return max(3.0, 4.0 * widest)


def structure_pitch(entries):
    foot = max(max(e["size"][0], e["size"][2]) * VOXEL for e in entries)
    return foot + 4.0


def build_world(sandbox, kind, entries, count):
    """Write the bench world and biome. Returns the world's centre, the half
    width of the occupied patch, and the creature spawn list if any."""
    n = math.ceil(math.sqrt(count))
    if kind == "plants":
        pitch = plant_spacing(entries)
    elif kind == "structures":
        pitch = structure_pitch(entries)
    else:
        pitch = 4.0
    half = n * pitch / 2
    size = max(256, int(math.ceil((2 * half + 96) / 64)) * 64)
    c = size / 2

    world = load_json(ROOT / "worlds" / "duelbox" / "world.json")
    for k in ("shooter", "games", "round_edges", "pickups", "seeds"):
        world.pop(k, None)
    world.update({
        "name": WORLD,
        "blurb": "A flat bench for measuring one library entry (tools/bench_entry.py).",
        "listed": False,
        "size_m": size,
        "height_m": 64,
        "sea_level_m": 2.0,
        "seed": 7,
        "spawn": [c, c + half + 8],
        "spawn_bearing": 0,
        "spawn_up": 5.0,
        "default_biome": BIOME,
        "seabed_biome": "arena_seabed",
        "biomes": [BIOME, "arena_seabed"],
        "modes": ["walk", "screenshot"],
    })

    # Plants grow only where the land is: size the island to the patch so the
    # biome's density puts about `count` trees on it.
    radius = half + 12 if kind == "plants" else size / 2 - 16
    events = [{
        "year": -4000, "event": "landmass", "cause": "A flat bench.", "centre": "middle",
        "radius": radius, "roughness": 0.0, "hills": 0, "ridges": 0, "shelf": 4, "edge_fall": 50,
    }]
    # A plant of the shallows (mangroves, cypress: `standing_water`) only
    # grows under water, so sink the patch to the middle of its range and let
    # the bench biome run on into the water.
    wet = kind == "plants" and all("standing_water" in e for e in entries)
    if wet:
        lo, hi = entries[0]["standing_water"]
        mid = round((lo + hi) / 2, 2)
        events.append({"year": -100, "event": "subside", "cause": "Flooded for a plant of the shallows.", "at": "middle",
                       "radius": half + 10, "level": [mid, mid], "ragged": 0.0, "edge": 6, "grain": 400})
        world["seabed_biome"] = BIOME
        world["biomes"] = [BIOME]
    plants = []
    if kind == "plants":
        # Scatter with a spacing a little under the pitch; calibrate() then
        # scales the density until about `count` trees grow.
        area = math.pi * (half + 4) ** 2
        plants = [{"plant": SUBJECT, "density": round(1.5 * count / area, 6), "placement": {"verb": "scatter", "params": {"spacing": round(pitch * 0.75, 2)}}}]
    elif kind == "structures":
        k = 0
        for i in range(n):
            for j in range(n):
                if k >= count:
                    break
                x = c - half + (i + 0.5) * pitch
                z = c - half + (j + 0.5) * pitch
                events.append({"year": -1, "event": "place", "cause": "A bench copy.", "structure": SUBJECT, "at": [round(x, 2), round(z, 2)], "bearing": 0})
                k += 1
    chronicle = {"places": {"middle": [c, c]}, "events": events}

    biome = {
        "name": BIOME, "version": 1, "introduced_by": WORLD, "notes": "Bench ground: lawn and nothing else.",
        "priority": 1, "rules": {}, "surface": "lawn_grass", "subsurface": "dirt", "fringe": 0,
        "map_color": [110, 170, 90], "plants": plants, "creatures": [], "collectibles": [],
    }
    if wet:
        biome.update({"rules": {"amphibious": True}, "surface": "delta_mud", "subsurface": "silt"})
    wdir = sandbox / "worlds" / WORLD
    wdir.mkdir(parents=True, exist_ok=True)
    (wdir / "world.json").write_text(json.dumps(world, indent=2), encoding="utf-8")
    (wdir / "chronicle.json").write_text(json.dumps(chronicle, indent=2), encoding="utf-8")
    (wdir / "story.md").write_text("A bench for tools/bench_entry.py.\n", encoding="utf-8")
    (sandbox / "library" / "biomes" / f"{BIOME}.json").write_text(json.dumps(biome, indent=2), encoding="utf-8")
    return c, half


def gazetteer(binary, sandbox):
    """(trees, structures) the bench world grew, from --gazetteer."""
    p = subprocess.run([str(binary), "--world", WORLD, "--gazetteer"], cwd=sandbox, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600)
    m = re.search(r"(\d+) structures placed, (\d+) trees", p.stdout)
    if not m:
        sys.exit(f"gazetteer failed:\n{p.stdout[-2000:]}\n{p.stderr[-2000:]}")
    return int(m.group(2)), int(m.group(1))


def calibrate(binary, sandbox, count):
    """Scale the bench biome's plant density until about `count` trees grow."""
    path = sandbox / "library" / "biomes" / f"{BIOME}.json"
    biome = load_json(path)
    trees = 0
    for _ in range(5):
        trees, _ = gazetteer(binary, sandbox)
        if abs(trees - count) <= max(3, count * 0.08) or trees == 0:
            break
        biome["plants"][0]["density"] *= count / trees
        path.write_text(json.dumps(biome, indent=2), encoding="utf-8")
    return trees


def views(kind, c, half):
    """Each view: name, extra arguments. The bench route walks a 50 m loop."""
    if kind == "creatures":
        stand = ["--ground", f"{c},{c}"]
        return [
            ("eye", stand + ["--look", "0,-8", "--frames", "300"]),
            ("aerial", ["--pos", f"{c},{30},{c + 30}", "--look", "0,-40", "--frames", "300"]),
            ("walk", stand + ["--bench-route", "field", "--frames", "120"]),
        ]
    return [
        ("eye", ["--ground", f"{c},{c + half}", "--look", "0,-2", "--frames", "240"]),
        ("aerial", ["--pos", f"{c},{12 + half * 0.4},{c + half + 8}", "--look", "0,-28", "--frames", "240"]),
        ("walk", ["--bench-route", "field", "--ground", f"{c},{c}", "--frames", "120"]),
    ]


def pick(d, path):
    for k in path:
        if not isinstance(d, dict) or k not in d:
            return None
        d = d[k]
    return d


def run(binary, sandbox, args, env, out_json, shot):
    cmd = [str(binary), "--headless", "--world", WORLD, "--fresh", "--save", str(sandbox / "saves"), "--no-hud",
           "--size", ARGS.size, "--hour", "13", "--weather", "sun", "--season", ARGS.season, "--wait", "60",
           "--stats", str(out_json)] + args
    if shot:
        cmd += ["--screenshot", str(shot)]
    p = subprocess.run(cmd, cwd=sandbox, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600)
    if p.returncode != 0 or not out_json.exists():
        sys.exit(f"run failed ({p.returncode}): {' '.join(cmd)}\n{p.stdout[-2000:]}\n{p.stderr[-2000:]}")
    for line in p.stdout.splitlines():
        if "problem" in line.lower() or "unknown" in line.lower():
            print("  engine:", line.strip())
    return load_json(out_json)


def fmt(v):
    if v is None:
        return "-"
    return f"{v:,.0f}" if abs(v) >= 100 else f"{v:.2f}"


def main():
    global ARGS
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("kind", choices=KINDS)
    ap.add_argument("name")
    ap.add_argument("--before", help="JSON file of the old version")
    ap.add_argument("--after", help="JSON file of the new version (default: the library file)")
    ap.add_argument("--before-also", action="append", default=[], metavar="FILE",
                    help="another entry's old version to swap in for the before runs, such as the parent of an entry with `like` (repeatable)")
    ap.add_argument("--count", type=int, default=100, help="how many copies (default 100)")
    ap.add_argument("--reps", type=int, default=3, help="runs of each view per version (default 3)")
    ap.add_argument("--bin", default=str(ROOT / "target" / "release" / "lurelia.exe"), help="engine binary")
    ap.add_argument("--size", default="1920x1080")
    ap.add_argument("--season", default="summer")
    ap.add_argument("--out", help="folder for the report and shots (default shots/bench/<kind>_<name>)")
    ap.add_argument("--keep", action="store_true", help="keep the sandbox folder")
    ARGS = ap.parse_args()

    kind, name = ARGS.kind, ARGS.name
    binary = Path(ARGS.bin).resolve()
    if not binary.exists() and binary.with_suffix("").exists():
        binary = binary.with_suffix("")
    if not binary.exists():
        sys.exit(f"no engine binary at {binary}: build it or pass --bin")
    after_path = Path(ARGS.after) if ARGS.after else ROOT / "library" / kind / f"{name}.json"
    after, after_src = load_json(after_path), str(after_path)
    if ARGS.before:
        before, before_src = load_json(ARGS.before), ARGS.before
    else:
        before, before_src = find_before(kind, name)
    if before == after:
        print("note: before and after are the same file; the two columns measure noise")
    out = Path(ARGS.out) if ARGS.out else ROOT / "shots" / "bench" / f"{kind}_{name}"
    out.mkdir(parents=True, exist_ok=True)

    sandbox = Path(tempfile.mkdtemp(prefix="lurelia_bench_"))
    try:
        shutil.copytree(ROOT / "library", sandbox / "library")
        c, half = build_world(sandbox, kind, [before, after], ARGS.count)
        env = dict(os.environ)
        if kind == "creatures":
            env["VG_SPAWN"] = ",".join([SUBJECT] * ARGS.count)
            env["VG_SPAWN_DIST"] = "6"
        subject = sandbox / "library" / kind / f"{SUBJECT}.json"
        # Other entries that change with the version: (path in the sandbox, old text, new text).
        also = []
        for f in ARGS.before_also:
            old = load_json(f)
            path = sandbox / "library" / kind / f"{old['name']}.json"
            also.append((path, json.dumps(old, indent=2), path.read_text(encoding="utf-8")))

        def use(which, entry):
            subject.write_text(json.dumps(dict(entry, name=SUBJECT), indent=2), encoding="utf-8")
            for path, old, new in also:
                path.write_text(old if which == "before" else new, encoding="utf-8")

        counts = {}
        for which, entry in (("before", before), ("after", after)):
            use(which, entry)
            if kind == "plants" and which == "before":
                calibrate(binary, sandbox, ARGS.count)
            if kind != "creatures":
                trees, placed = gazetteer(binary, sandbox)
                counts[which] = trees if kind == "plants" else placed
        if counts:
            print(f"copies grown: before {counts['before']}, after {counts['after']}")
        vs = views(kind, c, half)
        results = {v: {"before": [], "after": []} for v, _ in vs}
        print(f"bench {kind}/{name}: {ARGS.count} copies, {ARGS.reps} reps, before {before_src}, after {after_src}")
        for rep in range(ARGS.reps):
            for which, entry in (("before", before), ("after", after)) if rep % 2 == 0 else (("after", after), ("before", before)):
                use(which, entry)
                for vname, vargs in vs:
                    shot = out / f"{which}_{vname}.png" if rep == 0 and vname != "walk" else None
                    d = run(binary, sandbox, vargs, env, sandbox / "stats.json", shot)
                    results[vname][which].append(d)
                    print(f"  rep {rep + 1} {which:6} {vname:6} quads {fmt(d.get('quads_drawn'))}  frame p50 {fmt(d.get('frame_ms_p50'))} ms")
    finally:
        if ARGS.keep:
            print("sandbox kept at", sandbox)
        else:
            shutil.rmtree(sandbox, ignore_errors=True)

    lines = [f"# {kind}/{name}: before vs after, {ARGS.count} copies, median of {ARGS.reps}", "",
             f"before: {before_src}  ", f"after: {after_src}  "]
    if counts:
        lines.append(f"copies in the world: before {counts['before']}, after {counts['after']}")
    else:
        lines.append(f"copies asked for: {ARGS.count} (creatures wander, so fewer may be in view)")
    lines.append("")
    summary = {}
    for vname, _ in vs:
        lines += [f"## {vname}", "", "| | before | after | change |", "|---|---:|---:|---:|"]
        for label, path, _exact in ROWS:
            b = [pick(d, path) for d in results[vname]["before"]]
            a = [pick(d, path) for d in results[vname]["after"]]
            b = [x for x in b if isinstance(x, (int, float))]
            a = [x for x in a if isinstance(x, (int, float))]
            if not b or not a:
                continue
            mb, ma = statistics.median(b), statistics.median(a)
            ch = f"{(ma - mb) / mb * 100:+.0f}%" if mb else "-"
            lines.append(f"| {label} | {fmt(mb)} | {fmt(ma)} | {ch} |")
            summary.setdefault(vname, {})[label] = {"before": mb, "after": ma}
        lines.append("")
    lines.append("Counts are exact; ms rows are medians and vary a few percent run to run.")
    report = "\n".join(lines)
    (out / "report.md").write_text(report + "\n", encoding="utf-8")
    (out / "report.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print()
    print(report)
    print(f"\nreport and shots in {out}")


if __name__ == "__main__":
    main()
