"""Shoot a world's audit views and summarise its animal soak.

    python tools/world_audit.py WORLD --views views.json --out shots/audit/WORLD
    python tools/world_audit.py WORLD --soak 480

`views.json` is a list of views, each one of:
    {"name": "harbour", "pos": [x, y, z], "look": [yaw, pitch]}
    {"name": "street", "ground": [x, z], "look": [yaw, pitch]}
    {"name": "spawn"}                      (the world's own start view)
Optional per view: "hour" (default 15), "view" (draw distance, m).
Yaw 0 looks north, -90 east. Writes one PNG per view, named NN_name.png.

`--soak HOURS` runs the herd and pack records alone (`--eco-soak`) and prints
each species' count at the start, the lowest, the highest and the end, with
kills, births and starvations, and flags a species that crashes or explodes,
a world with predators and no kills, and herd animals that nothing hunts.
"""

import argparse
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def binary(path):
    if not os.path.isabs(path):
        path = os.path.join(ROOT, path)
    if os.name == "nt" and not path.endswith(".exe") and os.path.exists(path + ".exe"):
        return path + ".exe"
    return path


def shoot(a):
    views = json.load(open(a.views, encoding="utf-8"))
    os.makedirs(a.out, exist_ok=True)
    save = os.path.join(os.environ.get("TEMP", "/tmp"), "vg_audit_" + a.world)
    for k, v in enumerate(views, 1):
        name = "%02d_%s.png" % (k, v.get("name", "view"))
        cmd = [a.bin, "--headless", "--world", a.world, "--save", save, "--style", "calm",
               "--frames", "10", "--no-hud", "--size", a.size, "--screenshot", os.path.join(a.out, name)]
        if "pos" in v:
            cmd += ["--pos", ",".join(str(x) for x in v["pos"])]
        if "ground" in v:
            cmd += ["--ground", ",".join(str(x) for x in v["ground"])]
        if "look" in v:
            cmd += ["--look", ",".join(str(x) for x in v["look"])]
        if "view" in v:
            cmd += ["--view", str(v["view"])]
        cmd += ["--hour", str(v.get("hour", 15))]
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        print(("ok   " if r.returncode == 0 else "FAIL ") + name)


def soak(a):
    cmd = [a.bin, "--headless", "--world", a.world, "--eco-soak", str(a.soak)]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    rows = [json.loads(l) for l in r.stdout.splitlines() if l.startswith("{")]
    if not rows:
        print("no soak output; stderr tail:\n" + r.stderr[-800:])
        return
    lib = os.path.join(ROOT, "library", "creatures")
    hunted, hunters = set(), set()
    for name in rows[0]["animals"]:
        p = os.path.join(lib, name + ".json")
        if os.path.exists(p):
            b = json.load(open(p, encoding="utf-8")).get("behaviour", {})
            if b.get("hunts"):
                hunters.add(name)
                hunted.update(b["hunts"])
    last = rows[-1]
    print("%-22s %6s %6s %6s %6s" % ("species", "start", "low", "high", "end"))
    for name, start in sorted(rows[0]["animals"].items()):
        series = [r["animals"].get(name, 0) for r in rows]
        lo, hi, end = min(series), max(series), series[-1]
        flag = ""
        if start and end < start * 0.6:
            flag = "  CRASH"
        elif start and end > start * 1.6:
            flag = "  EXPLODES"
        if name not in hunters and name not in hunted and start >= 20:
            flag += "  nothing hunts it"
        print("%-22s %6d %6d %6d %6d%s" % (name, start, lo, hi, end, flag))
    print("over %.0f game hours: killed %d, born %d, starved %d, lost herds %d, replaced %d"
          % (last["hours"], last["killed"], last["born"], last["starved"], last["lost"], last["replaced"]))
    if hunters and last["killed"] == 0:
        print("WARNING: predators (%s) made no kills" % ", ".join(sorted(hunters)))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("world")
    ap.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "lurelia"))
    ap.add_argument("--views")
    ap.add_argument("--out")
    ap.add_argument("--size", default="1280x720")
    ap.add_argument("--soak", type=float)
    a = ap.parse_args()
    a.bin = binary(a.bin)
    if a.views:
        if not a.out:
            a.out = os.path.join(ROOT, "shots", "audit", a.world)
        shoot(a)
    if a.soak:
        soak(a)
    if not a.views and not a.soak:
        ap.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
