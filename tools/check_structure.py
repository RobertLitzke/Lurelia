"""Check structure files for what the loader cannot see: the things a reviewer
would otherwise have to find by reading the ops (library/schema/structure.md).

    python tools/check_structure.py library/structures/chapel.json
    python tools/check_structure.py ../new_chapel.json   # a replacement

A file outside library/structures/ is treated as a replacement for the
library entry of the same name and compared with it. Checks:

- the schema (library/schema/structure.schema.json), if `jsonschema` is
  installed; `lurelia --validate` covers most of it otherwise
- every material named exists; every `mat` is a slot, a material, air or carve
- slots no op uses; anything drawn outside `size`; the anchor inside `size`
- doors: a `door_*` material in a slot that is not a door (every box of it
  becomes a sliding door on a structure placed at age 0), a door leaf under
  2 m, a door slot whose material is not `door_*` (it never opens)
- sizes the notes give in metres, beside what the file measures
- `_generated_by` (edit the script, not the file), canonical form, version

Exit status 1 if anything is an error.
"""
import json
import math
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

LIB = os.path.join(ROOT, "library")
VOXEL = 0.25


def extent(op, off):
    """Bounding box [lo, hi] of what an op draws, in voxels."""
    o = op["op"]

    def a(p):
        return [p[i] + off[i] for i in range(3)]

    if o in ("box", "walls"):
        return a(op["from"]), [t + off[i] for i, t in enumerate(op["to"])]
    if o == "gable":
        f, t = op["from"], op["to"]
        across = (t[2] - f[2]) if op["axis"] == "x" else (t[0] - f[0])
        top = f[1] + math.ceil(across / 2) * op.get("rise", 1)
        return a(f), [t[0] + off[0], top + off[1], t[2] + off[2]]
    if o == "ellipsoid":
        c, r = a(op["centre"]), op["radius"]
        return [c[i] - r[i] for i in range(3)], [c[i] + r[i] for i in range(3)]
    if o == "cylinder":
        b, r = a(op["base"]), op["radius"]
        return [b[0] - r, b[1], b[2] - r], [b[0] + r, b[1] + op["height"], b[2] + r]
    if o in ("segment",):
        ps, r = [a(p) for p in op["points"]], op["radius"]
        return [min(p[i] for p in ps) - r for i in range(3)], [max(p[i] for p in ps) + r for i in range(3)]
    if o == "log":
        b, r, yaw = a(op["base"]), op["radius"], math.radians(op.get("yaw", 0))
        dx, dz = (1.0, 0.0) if op["axis"] == "x" else (0.0, 1.0)
        dx, dz = dx * math.cos(yaw) - dz * math.sin(yaw), dx * math.sin(yaw) + dz * math.cos(yaw)
        e = [b[0] + dx * op["length"], b[1], b[2] + dz * op["length"]]
        return [min(b[i], e[i]) - r for i in range(3)], [max(b[i], e[i]) + r for i in range(3)]
    if o == "disc":
        # Upright: `radius` up and across, `thick` along its axis.
        c, r, t, yaw = a(op["centre"]), op["radius"], op["thick"] / 2, math.radians(op.get("yaw", 0))
        ax, az = (1.0, 0.0) if op["axis"] == "x" else (0.0, 1.0)
        ax, az = ax * math.cos(yaw) - az * math.sin(yaw), ax * math.sin(yaw) + az * math.cos(yaw)
        hx, hz = abs(ax) * t + abs(az) * r, abs(az) * t + abs(ax) * r
        return [c[0] - hx, c[1] - r, c[2] - hz], [c[0] + hx, c[1] + r, c[2] + hz]
    if o == "dish":
        v, r = a(op["vertex"]), op["radius"] + op.get("depth", 0)
        return [v[0] - r, v[1] - r, v[2] - r], [v[0] + r, v[1] + r, v[2] + r]
    if o == "set":
        p = a(op["at"])
        return p, [p[0] + 1, p[1] + 1, p[2] + 1]
    return None


def flatten(ops, off=(0, 0, 0)):
    for op in ops:
        if op["op"] == "repeat":
            for k in range(op["count"]):
                yield from flatten(op["ops"], [off[j] + op["step"][j] * k for j in range(3)])
        else:
            yield op, off


def check(path):
    errors, warnings, info = [], [], []
    text = open(path, encoding="utf-8").read()
    d = json.loads(text)
    name = d.get("name", "?")
    stem = os.path.splitext(os.path.basename(path))[0]
    in_lib = os.path.normpath(os.path.dirname(os.path.abspath(path))) == os.path.normpath(os.path.join(LIB, "structures"))
    slots = d.get("slots", {})
    mats = {f[:-5] for f in os.listdir(os.path.join(LIB, "materials")) if f.endswith(".json")}
    # The engine's own materials (planks, brick, stone...) have no library file.
    src = open(os.path.join(ROOT, "src", "world", "matlib.rs"), encoding="utf-8").read()
    block = src[src.find("BUILTIN"):]
    mats |= set(re.findall(r'\("([a-z_]+)",', block[: block.find("];")]))

    if d.get("_generated_by"):
        warnings.append(f"written by {d['_generated_by']}: change the script, not this file")
    if in_lib and stem != name:
        errors.append(f"name {name!r} but the file is {stem}.json")

    try:
        from jsonschema import Draft202012Validator
        schema = json.load(open(os.path.join(LIB, "schema", "structure.schema.json"), encoding="utf-8"))
        for e in Draft202012Validator(schema).iter_errors(d):
            where = "/".join(str(p) for p in e.absolute_path) or "(top)"
            errors.append(f"schema at {where}: {e.message[:140]}")
    except ImportError:
        info.append("jsonschema not installed: schema not checked (lurelia --validate covers most of it)")

    for slot, m in slots.items():
        if m not in mats:
            errors.append(f"slot {slot}: no material {m}")
    used, outside = set(), []
    lo_all, hi_all = [math.inf] * 3, [-math.inf] * 3
    size = d.get("size", [0, 0, 0])
    door_leaves, not_leaves, below = [], 0, False
    for op, off in flatten(d.get("ops", [])):
        m = op.get("mat")
        if m is None:
            continue
        used.add(m)
        if m not in slots and m not in mats and m not in ("air", "carve"):
            errors.append(f"{op['op']}: mat {m!r} is neither a slot nor a material")
        real = slots.get(m, m)
        e = extent(op, off)
        if e is None or m in ("air", "carve"):
            continue
        lo, hi = e
        for i in range(3):
            lo_all[i], hi_all[i] = min(lo_all[i], lo[i]), max(hi_all[i], hi[i])
        if any(lo[i] < -0.6 or hi[i] > size[i] + 0.6 for i in (0, 2)) or hi[1] > size[1] + 0.6:
            outside.append(f"{op['op']} {[round(v, 1) for v in lo]}..{[round(v, 1) for v in hi]}")
        elif lo[1] < -0.6:
            below = True
        if real.startswith("door_"):
            if op["op"] != "box":
                not_leaves += 1
            else:
                door_leaves.append(([op["from"][i] + off[i] for i in range(3)], [op["to"][i] + off[i] for i in range(3)]))
    if not_leaves:
        warnings.append(f"{not_leaves} ops other than boxes are drawn in a door_* material: use a non-door material for them")
    for slot in sorted(set(slots) - used):
        warnings.append(f"slot {slot} is never used")
    for o in outside[:5]:
        warnings.append(f"drawn outside size {size}: {o} (clipped)")
    if len(outside) > 5:
        warnings.append(f"... and {len(outside) - 5} more ops outside size")
    if below:
        info.append("some ops reach below y = 0 and are cut off at ground level (fine for rocks sunk into the ground)")
    anchor = d.get("anchor", [0, 0, 0])
    if not all(0 <= anchor[i] < size[i] for i in (0, 2)):
        errors.append(f"anchor {anchor} is outside the footprint of size {size}")
    elif not 0 <= anchor[1] < size[1]:
        info.append(f"anchor y {anchor[1]} is outside the box: the structure is hung or buried by that much")

    for slot, m in slots.items():
        if m.startswith("door_") and "door" not in slot:
            warnings.append(f"slot {slot} uses door material {m}: on a structure placed at age 0 every box of it becomes a sliding door")
        if "door" in slot and not m.startswith("door_") and slot in used:
            info.append(f"slot {slot} is {m}, not a door_* material: its doors never open")
    # A leaf is often several boxes stacked (an arched door): join boxes that
    # overlap across and touch up and down, and measure each group.
    groups = []
    for lo, hi in door_leaves:
        joined = [g for g in groups if lo[0] < g[1][0] and g[0][0] < hi[0] and lo[2] < g[1][2] and g[0][2] < hi[2]
                  and lo[1] <= g[1][1] and g[0][1] <= hi[1]]
        for g in joined:
            groups.remove(g)
            lo = [min(lo[i], g[0][i]) for i in range(3)]
            hi = [max(hi[i], g[1][i]) for i in range(3)]
        groups.append((lo, hi))
    for h in sorted({g[1][1] - g[0][1] for g in groups}):
        if h < 8:
            warnings.append(f"a door leaf is {h * VOXEL:.2f} m tall; the player is 1.75 m")

    if lo_all[0] < math.inf:
        w, t, dep = [(hi_all[i] - lo_all[i]) * VOXEL for i in range(3)]
        top = hi_all[1] * VOXEL
        info.append(f"measures {w:.1f} x {dep:.1f} m (x by z), {top:.1f} m to its top")
        # The notes may give several sizes (a tower's height and its spire's);
        # flag a kind only when none of them is near what the file measures.
        heights, spans = [], []
        for m in re.finditer(r"(\d+(?:\.\d+)?)[ -](?:m|metres?)\b([^.,;:\d]{0,30})", d.get("notes", "")):
            v, rest = float(m.group(1)), m.group(2)
            if re.search(r"\b(tall|high|to (its|the) (top|tip|pinnacles|cross))", rest):
                heights.append((v, m.group(0).strip()))
            elif re.search(r"\b(long|across|wide|square)\b", rest):
                spans.append((v, m.group(0).strip()))
        if heights and all(abs(v - top) > max(1.0, 0.15 * top) for v, _ in heights):
            warnings.append(f"notes give the height as {', '.join(repr(s) for _, s in heights)}, but it is {top:.1f} m to its top")
        if spans and all(min(abs(v - w), abs(v - dep)) > max(1.0, 0.2 * v) for v, _ in spans):
            warnings.append(f"notes give the size as {', '.join(repr(s) for _, s in spans)}, but it measures {w:.1f} x {dep:.1f} m")

    if text.replace("\r\n", "\n") != libfmt.render(d, "structures"):
        info.append("not in canonical form: run python tools/libfmt.py")

    if not in_lib:
        lib_path = os.path.join(LIB, "structures", name + ".json")
        if os.path.exists(lib_path):
            old = json.load(open(lib_path, encoding="utf-8"))
            for k in ("size", "anchor", "foundation", "slots", "base", "level", "decays", "tags", "vines"):
                if old.get(k) != d.get(k):
                    warnings.append(f"replacement changes {k}: {old.get(k)} -> {d.get(k)}")
            if d.get("version", 1) <= old.get("version", 1):
                info.append(f"version is {d.get('version', 1)}, same as the library's: bump it when it replaces the entry")
        else:
            info.append(f"no library entry called {name}: this would be a new structure")
    return name, errors, warnings, info


def main():
    paths = [p for p in sys.argv[1:] if not p.startswith("-")]
    if not paths:
        print(__doc__)
        sys.exit(2)
    bad = False
    for p in paths:
        name, errors, warnings, info = check(p)
        print(f"{p} ({name}): {len(errors)} errors, {len(warnings)} warnings")
        for tag, lines in (("error", errors), ("warning", warnings), ("note", info)):
            for line in lines:
                print(f"  {tag}: {line}")
        bad |= bool(errors)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
