"""Redraw a library creature family at a new voxel size, shapes unchanged.

    python tools/creature_rescale.py NAME VOXEL [--dry] [--from VOXEL]

Sets the entry's `voxel` to VOXEL metres and maps every voxel coordinate by
k = old voxel / new voxel about the pivot (bottom centre of the grid): ops,
the rig's joints and claims, bone overlaps, and the `extra_ops` of every
entry that inherits from it (at any depth, since a child's ops are drawn in
its parent's grid). `grid` grows or shrinks with k, kept within 32..96 and
even; half_w, height and everything else in metres stays as it is.

It is the mechanical first step of the creature process in docs/CREATURES.md
("Model resolution and decimation"): the shapes come out the same size in
metres, with no new detail. The modelling pass comes after it. The original
of each file it changes is kept under notes/replaced_creatures/ unless one
of that version is already there.
"""
import glob, json, math, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import libfmt  # noqa: E402

LIB = os.path.join(ROOT, "library", "creatures")


def load(n):
    return json.load(open(os.path.join(LIB, n + ".json"), encoding="utf-8"))


def grid3(g):
    g = 32 if g is None else g
    return [g, g, g] if isinstance(g, int) else list(g)


class Map:
    def __init__(self, k, old, new):
        self.k = k
        self.p0 = [old[0] / 2, 0.0, old[2] / 2]
        self.p1 = [new[0] / 2, 0.0, new[2] / 2]

    def f(self, p):
        return [round(self.p1[i] + (p[i] - self.p0[i]) * self.k, 3) for i in range(3)]

    def i(self, p):
        return [int(round(v)) for v in self.f(p)]

    def s(self, v):
        return round(v * self.k, 3)

    def box(self, a, b):
        fa, fb = self.i(a), self.i(b)
        return fa, [max(fb[j], fa[j] + 1) for j in range(3)]

    def ops(self, ops, inner=False):
        out = []
        for op in ops:
            o = dict(op)
            t = op["op"]
            if t == "ellipsoid":
                o["centre"] = self.f(op["centre"])
                o["radius"] = [self.s(r) for r in op["radius"]]
            elif t == "segment":
                o["points"] = [self.f(p) for p in op["points"]]
                o["radius"] = self.s(op["radius"])
            elif t == "box":
                o["from"], o["to"] = self.box(op["from"], op["to"])
            elif t == "recolour":
                o["at"], o["until"] = self.box(op["at"], op["until"])
            elif t == "set":
                a = op["at"]
                o = {"op": "box", "from": None, "to": None, "mat": op["mat"]}
                o["from"], o["to"] = self.box(a, [a[0] + 1, a[1] + 1, a[2] + 1])
                if "part" in op:
                    o["part"] = op["part"]
            elif t == "recolour_below":
                o["y"] = int(round(op["y"] * self.k))
            elif t == "cylinder":
                o["base"] = self.f(op["base"])
                o["radius"], o["height"] = self.s(op["radius"]), self.s(op["height"])
            elif t == "log":
                o["base"] = self.f(op["base"])
                o["radius"], o["length"] = self.s(op["radius"]), self.s(op["length"])
            elif t == "disc":
                o["centre"] = self.f(op["centre"])
                o["radius"], o["thick"] = self.s(op["radius"]), max(1.0, self.s(op["thick"]))
            elif t == "repeat":
                o["step"] = [int(round(v * self.k)) for v in op["step"]]
                o["ops"] = self.ops(op["ops"], True)
            else:
                raise SystemExit(f"creature_rescale: no rule for op {t}")
            out.append(o)
        return out

    def rig(self, rig):
        rig = json.loads(json.dumps(rig))
        for b in rig.get("bones", []):
            if "joint" in b:
                b["joint"] = self.f(b["joint"])
            if "claims" in b:
                b["claims"] = [list(self.box(a, c)) for a, c in b["claims"]]
            if "overlap" in b:
                b["overlap"] = max(1, int(round(b["overlap"] * self.k)))
        return rig


def children_of():
    kids = {}
    for f in glob.glob(os.path.join(LIB, "*.json")):
        d = json.load(open(f, encoding="utf-8"))
        if d.get("like"):
            kids.setdefault(d["like"], []).append(d["name"])
    return kids


def bounds(ops, lo, hi, off=(0, 0, 0)):
    """Grow lo/hi (grid coordinates) to hold every op's shape, roughly."""
    def grow(a, b):
        for i in range(3):
            lo[i] = min(lo[i], a[i] + off[i])
            hi[i] = max(hi[i], b[i] + off[i])
    for op in ops:
        t = op["op"]
        if op.get("mat") == "air" or t in ("recolour", "recolour_below"):
            continue
        if t == "ellipsoid":
            c, r = op["centre"], op["radius"]
            grow([c[i] - r[i] for i in range(3)], [c[i] + r[i] for i in range(3)])
        elif t == "segment":
            for q in op["points"]:
                grow([v - op["radius"] for v in q], [v + op["radius"] for v in q])
        elif t == "box":
            grow(op["from"], op["to"])
        elif t == "set":
            grow(op["at"], [v + 1 for v in op["at"]])
        elif t in ("cylinder", "log", "disc"):
            b = op.get("base", op.get("centre"))
            e = op["radius"] + op.get("length", op.get("height", 0)) + op.get("thick", 0)
            grow([v - e for v in b], [v + e for v in b])
        elif t == "repeat":
            for k in range(op["count"]):
                bounds(op["ops"], lo, hi, [off[i] + op["step"][i] * k for i in range(3)])


def grid_for(ops_lists, old, k):
    """The grid that holds the shapes at the new size: 32..96 a side, even,
    centred on the pivot in x and z."""
    lo, hi = [1e9] * 3, [-1e9] * 3
    for ops in ops_lists:
        bounds(ops, lo, hi)
    lo = [max(lo[i], 0) for i in range(3)]
    hi = [min(hi[i], old[i]) for i in range(3)]
    px, pz = old[0] / 2, old[2] / 2
    need = [2 * max(px - lo[0], hi[0] - px) * k + 4, hi[1] * k + 3, 2 * max(pz - lo[2], hi[2] - pz) * k + 4]
    return [max(32, min(96, 2 * math.ceil(v / 2))) for v in need], need

def keep_original(d):
    dst = os.path.join(ROOT, "notes", "replaced_creatures", f"{d['name']}_v{d.get('version', 1)}.json")
    if not os.path.exists(dst):
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        open(dst, "w", encoding="utf-8", newline="\n").write(libfmt.render(d, "creatures"))


def write(d):
    open(os.path.join(LIB, d["name"] + ".json"), "w", encoding="utf-8", newline="\n").write(libfmt.render(d, "creatures"))


def main(name, voxel, dry=False, from_voxel=None):
    d = load(name)
    if d.get("like") and not d.get("ops"):
        raise SystemExit(f"{name} inherits its shapes from {d['like']}: rescale the family's root")
    # An entry with `like` and ops of its own is the root of its shapes; it
    # may still take its voxel (scaled) and grid from the parent, which
    # `--from` and the parent's grid stand in for.
    cur = d.get("voxel") or from_voxel
    if cur is None:
        raise SystemExit(f"{name} takes its voxel from {d['like']}: give its current size with --from VOXEL")
    k = cur / voxel
    old = grid3(d.get("grid") if "grid" in d or not d.get("like") else load(d["like"]).get("grid"))
    kids = children_of()
    # Entries drawn in this grid: the root, then every descendant down to
    # one that gives both its own voxel and its own ops (a frame of its own).
    # One with a voxel of its own but the parent's ops is drawn in this grid
    # at its own size, so its voxel shrinks by k with the root's.
    mapped, pinned, todo = [], [], [name]
    while todo:
        n = todo.pop()
        for c in kids.get(n, []):
            cd = load(c)
            if "voxel" in cd and cd.get("ops"):
                pinned.append(cd)
            else:
                mapped.append(cd)
                todo.append(c)
    lists = [d["ops"]] + [x.get("ops", []) + x.get("extra_ops", []) for x in mapped]
    new, need = grid_for(lists, old, k)
    if any(v > 96 for v in need):
        print(f"warning: {name} needs {[round(v) for v in need]} voxels at this size, past the 96 limit; shapes past the grid are cut")
    if new != [32, 32, 32] and not d.get("rig"):
        print(f"warning: {name} has no rig but needs a {new} grid; only rigged models can be bigger than 32^3")
    m = Map(k, old, new)
    nd = dict(d)
    nd["voxel"] = round(voxel, 5)
    if new == [32, 32, 32]:
        nd.pop("grid", None)
    else:
        nd["grid"] = new
    nd["ops"] = m.ops(d["ops"])
    if d.get("rig"):
        nd["rig"] = m.rig(d["rig"])
    changed = [(d, nd)]
    for cd in mapped:
        nc = dict(cd)
        if "voxel" in cd:
            nc["voxel"] = round(cd["voxel"] / k, 5)
        for key in ("ops", "extra_ops"):
            if cd.get(key):
                nc[key] = m.ops(cd[key])
        if cd.get("rig"):
            nc["rig"] = m.rig(cd["rig"])
        if cd.get("grid"):
            print(f"warning: {cd['name']} sets its own grid {cd['grid']}; check it")
        if nc != cd:
            changed.append((cd, nc))
    # A child with ops and a voxel of its own keeps its frame: give it the
    # grid and rig it was using, if it took them from the root.
    for cd in pinned:
        nc = dict(cd)
        # The grid too, even the default one: a child without a grid of its
        # own takes the root's, which has just changed size.
        if "grid" not in cd and new != old:
            nc["grid"] = old
        if "rig" not in cd and d.get("rig"):
            nc["rig"] = d["rig"]
        if nc != cd:
            changed.append((cd, nc))
    print(f"{name}: voxel {cur} -> {voxel} (k {k:.3f}), grid {old} -> {new}; mapped {', '.join(x['name'] for x in mapped) or '-'}; own frame {', '.join(x['name'] for x in pinned) or '-'}")
    if dry:
        return
    for old_d, new_d in changed:
        keep_original(old_d)
        write(new_d)


if __name__ == "__main__":
    fv = float(sys.argv[sys.argv.index("--from") + 1]) if "--from" in sys.argv else None
    main(sys.argv[1], float(sys.argv[2]), "--dry" in sys.argv, fv)
