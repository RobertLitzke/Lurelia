"""Photograph a structure where its worlds placed it, for review.

    python tools/shoot_structure.py wool_church
    python tools/shoot_structure.py oil_platform --world lowmouth --max 2 --views 4

Finds the worlds whose chronicle names the structure (or pass --world for a
feature a biome scatters), reads each world's gazetteer for where it stands, probes the ground there (VG_PROBE) and takes
`--views` shots from round it (the first from in front, where the gazetteer
gives a facing), looking down at about 25 degrees from far enough out to frame
it, so the land and trees round it are in the picture and nearby buildings
rarely block it, plus one from overhead, and tiles them into one contact
sheet (<world>_<n>_sheet.png, 1200 px wide): look at that, not the single
views. Shots go to screenshots/structures/<name>/ (git-ignored).

It drives the release binary (target/release/lurelia, or --bin PATH), which
reads the library at load, so an edited structure file needs no rebuild.
"""
import argparse
import glob
import json
import math
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VOXEL = 0.25


def worlds_using(name):
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, "worlds", "*", "chronicle.json")) + glob.glob(os.path.join(ROOT, "local", "worlds", "*", "chronicle.json"))):
        if f'"{name}"' in open(path, encoding="utf-8").read():
            out.append(os.path.basename(os.path.dirname(path)))
    return out


def run(binary, args, env=None):
    e = dict(os.environ, **(env or {}))
    r = subprocess.run([binary] + args, cwd=ROOT, capture_output=True, text=True, env=e)
    return r.stdout + r.stderr


def placements(binary, world, name):
    """(x, z, facing or None) for each placement, from the gazetteer."""
    text = run(binary, ["--world", world, "--gazetteer"])
    found = []
    for m in re.finditer(rf"\b{re.escape(name)} at (-?\d+),(-?\d+) facing (-?\d+) deg", text):
        found.append((float(m.group(1)), float(m.group(2)), float(m.group(3))))
    # Built structures are also listed by centre; natural ones only by event.
    for m in re.finditer(rf"^  {re.escape(name)} +(-?\d+),(-?\d+) ", text, re.M):
        x, z = float(m.group(1)), float(m.group(2))
        if not any(abs(x - f[0]) < 12 and abs(z - f[1]) < 12 for f in found):
            found.append((x, z, None))
    # Natural features a biome scatters are listed by name with a few centres.
    if not found:
        m = re.search(rf"^  {re.escape(name)} +\d+  (.+)$", text[text.find("Natural features"):], re.M)
        if m:
            for xz in m.group(1).split():
                x, z = xz.split(",")
                found.append((float(x), float(z), None))
    return found


def ground(binary, world, x, z, reach):
    """A function giving the probed ground height near (x, z)."""
    step = max(2, int(math.ceil(reach / 8)))
    text = run(binary, ["--world", world, "--gazetteer"], {"VG_PROBE": f"{int(x)},{int(z)},{step}"})
    i = text.find("Probe round")
    rows = text[i:].splitlines()[1:]
    xs = [int(v) for v in rows[0].split()]
    grid = {}
    for line in rows[1:]:
        parts = line.split()
        if not parts or not re.fullmatch(r"-?\d+", parts[0]):
            break
        zz = int(parts[0])
        # Wide heights can run together ("122.8*124.8"): take every number.
        values = re.findall(r"-?\d+(?:\.\d+)?", " ".join(parts[1:]))
        for xx, v in zip(xs, values):
            grid[(xx, zz)] = float(v)

    def at(px, pz):
        k = min(grid, key=lambda g: (g[0] - px) ** 2 + (g[1] - pz) ** 2)
        return grid[k]
    return at


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("name")
    ap.add_argument("--world", action="append", help="only these worlds (repeatable)")
    ap.add_argument("--max", type=int, default=2, help="placements per world (default 2)")
    ap.add_argument("--views", type=int, default=3, help="shots round each placement (default 3)")
    ap.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "lurelia"))
    ap.add_argument("--out", default=None)
    ap.add_argument("--size", default="960x540", help="render size of each view (default 960x540)")
    ap.add_argument("--no-sheet", action="store_true", help="skip the contact sheet")
    ap.add_argument("--dist", type=float, default=None,
                    help="camera distance in metres (default from the size); fix it to compare two versions from the same spot")
    a = ap.parse_args()
    a.bin = os.path.abspath(os.path.join(ROOT, a.bin))
    if os.name == "nt" and not a.bin.endswith(".exe") and os.path.exists(a.bin + ".exe"):
        a.bin += ".exe"

    d = json.load(open(os.path.join(ROOT, "library", "structures", a.name + ".json"), encoding="utf-8"))
    sx, sy, sz = (v * VOXEL for v in d["size"])
    reach = max(math.hypot(sx, sz), sy)
    dist = a.dist or max(7.0, 1.15 * reach + 4.0)
    out = a.out or os.path.join(ROOT, "screenshots", "structures", a.name)
    os.makedirs(out, exist_ok=True)
    worlds = a.world or worlds_using(a.name)
    if not worlds:
        sys.exit(f"no world's chronicle names {a.name}; pass --world")
    shots = []
    for world in worlds:
        spec = json.load(open(os.path.join(ROOT, "worlds" if os.path.isdir(os.path.join(ROOT, "worlds", world)) else "local/worlds", world, "world.json"), encoding="utf-8"))
        sea = float(spec.get("sea_level_m", 0))
        found = placements(a.bin, world, a.name)
        if not found:
            print(f"{world}: {a.name} not found in the gazetteer")
            continue
        for k, (x, z, facing) in enumerate(found[: a.max]):
            g = ground(a.bin, world, x, z, dist + 4)
            base = max(g(x, z), sea) if d.get("base") == "sea" else g(x, z)
            look_at = base + sy * 0.4
            views = []
            for v in range(a.views + (0 if a.no_sheet else 1)):
                if v < a.views:
                    bearing = ((facing if facing is not None else 200.0) + v * 360.0 / a.views) % 360
                    # Bearings run clockwise from north (-z).
                    cx = x + dist * math.sin(math.radians(bearing))
                    cz = z - dist * math.cos(math.radians(bearing))
                    cy = max(g(cx, cz), sea) + 2.0 + sy * 0.4 + dist * 0.45
                    name = f"{int(bearing):03d}"
                else:
                    # The sheet's last view: from overhead, a little to the south.
                    cx, cz = x, z + dist * 0.25
                    cy = base + sy + dist * 1.1
                    name = "top"
                yaw = math.degrees(math.atan2(-(x - cx), -(z - cz))) % 360
                pitch = -math.degrees(math.atan2(cy - look_at, math.hypot(x - cx, z - cz)))
                path = os.path.join(out, f"{world}_{k + 1}_{name}.png")
                run(a.bin, ["--headless", "--world", world, "--size", a.size, "--pos", f"{cx:.1f},{cy:.1f},{cz:.1f}",
                            "--look", f"{yaw:.0f},{pitch:.0f}", "--no-hud", "--screenshot", path])
                views.append(path)
            shots += views
            if a.no_sheet:
                for path in views:
                    print(f"{world} #{k + 1} at {x:.0f},{z:.0f}: {os.path.relpath(path, ROOT)}")
            else:
                sheet = os.path.join(out, f"{world}_{k + 1}_sheet.png")
                contact_sheet(views, sheet)
                print(f"{world} #{k + 1} at {x:.0f},{z:.0f}: {os.path.relpath(sheet, ROOT)}")
    if not shots:
        sys.exit(1)


def contact_sheet(paths, out, width=1200):
    """The views tiled two across into one image `width` pixels wide, so a
    reviewer looks once at a small picture instead of four large ones."""
    from PIL import Image
    tiles = [Image.open(p).convert("RGB") for p in paths if os.path.exists(p)]
    if not tiles:
        return
    tw = width // 2
    th = round(tiles[0].height * tw / tiles[0].width)
    rows = (len(tiles) + 1) // 2
    sheet = Image.new("RGB", (width, th * rows), (0, 0, 0))
    for i, t in enumerate(tiles):
        sheet.paste(t.resize((tw, th)), ((i % 2) * tw, (i // 2) * th))
    sheet.save(out)


if __name__ == "__main__":
    main()
