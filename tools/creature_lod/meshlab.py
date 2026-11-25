"""Creature mesh lab: square (greedy), sloped (surface nets and Taubin smoothing),
decimated (quadric edge collapse of the sloped mesh), voxel LOD, and a small
software renderer that draws them all the same way."""
import heapq, json, math, os, sys
import numpy as np
from scipy.ndimage import gaussian_filter

LIB = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "library", "creatures")
SCALED = ["voxel", "half_w", "height"]
MERGED = ["colors", "behaviour", "in_water", "vary"]


# ---------------------------------------------------------------- entries
def load(name):
    d = json.load(open(os.path.join(LIB, name + ".json"), encoding="utf-8"))
    if "like" not in d:
        return d
    p = load(d["like"])
    out = dict(p)
    s = d.get("scale", 1.0)
    for k in SCALED:
        if k in p and k not in d:
            out[k] = p[k] * s
    for k, v in d.items():
        if k in ("like", "scale", "extra_ops"):
            continue
        if k in MERGED and isinstance(v, dict):
            m = dict(p.get(k, {}))
            m.update(v)
            out[k] = m
        else:
            out[k] = v
    # A child's own `ops` replace the parent's, as every other key does
    # (src/inherit.rs); `extra_ops` are appended to whichever ops it has.
    out["ops"] = list(d["ops"] if "ops" in d else p.get("ops", [])) + list(d.get("extra_ops", []))
    return out


def grid_of(d):
    g = d.get("grid", 32)
    return [g, g, g] if isinstance(g, int) else list(g)


# ---------------------------------------------------------------- compile
def compile_ops(ops, size, colors):
    """Port of world/shape.rs run() for the creature ops. Grid [x, y, z],
    0 empty, else index+1 into the returned material list."""
    names = list(colors.keys())
    unknown = set()

    def res(m):
        if m == "air":
            return 0
        if m in colors:
            return names.index(m) + 1
        unknown.add(m)
        return None

    g = np.zeros(size, np.uint8)
    X, Y, Z = size

    def box(a, b, m):
        a = [max(0, int(a[i])) for i in range(3)]
        b = [min(size[i], int(b[i])) for i in range(3)]
        if all(b[i] > a[i] for i in range(3)):
            g[a[0]:b[0], a[1]:b[1], a[2]:b[2]] = m

    def add(p, off):
        return [p[i] + off[i] for i in range(3)]

    def put(pts, m):
        for x, y, z in pts:
            if 0 <= x < X and 0 <= y < Y and 0 <= z < Z:
                g[x, y, z] = m

    def fill_shape(op, b, m):
        t, r = op["op"], op["radius"]
        out = []
        if t == "cylinder":
            h = op["height"]
            for y in range(math.floor(b[1]), math.ceil(b[1] + h)):
                for z in range(math.floor(b[2] - r), math.ceil(b[2] + r) + 1):
                    for x in range(math.floor(b[0] - r), math.ceil(b[0] + r) + 1):
                        if (x + 0.5 - b[0]) ** 2 + (z + 0.5 - b[2]) ** 2 <= r * r:
                            out.append((x, y, z))
                if 0 < r < 0.75:
                    out.append((math.floor(b[0]), y, math.floor(b[2])))
        elif t == "log" and op.get("yaw", 0):
            a = math.radians(op["yaw"]) + (0 if op.get("axis", "z") == "x" else math.pi / 2)
            d = (math.cos(a), math.sin(a)); L = op["length"]
            e = (b[0] + d[0] * L, b[2] + d[1] * L)
            for y in range(math.floor(b[1] - r), math.ceil(b[1] + r) + 1):
                dy = y + 0.5 - b[1]
                for z in range(math.floor(min(b[2], e[1]) - r) - 1, math.ceil(max(b[2], e[1]) + r) + 2):
                    for x in range(math.floor(min(b[0], e[0]) - r) - 1, math.ceil(max(b[0], e[0]) + r) + 2):
                        u, v = x + 0.5 - b[0], z + 0.5 - b[2]
                        tt = u * d[0] + v * d[1]; ac = -u * d[1] + v * d[0]
                        if 0 <= tt < L and ac * ac + dy * dy <= r * r:
                            out.append((x, y, z))
        elif t == "log":
            ax = op.get("axis", "z") == "x"; L = op["length"]
            for tt in range(math.ceil(L)):
                for y in range(math.floor(b[1] - r), math.ceil(b[1] + r) + 1):
                    for w in range(-math.ceil(r) - 1, math.ceil(r) + 2):
                        x, z, cw = (math.floor(b[0]) + tt, math.floor(b[2]) + w, b[2]) if ax else (math.floor(b[0]) + w, math.floor(b[2]) + tt, b[0])
                        acr = (z + 0.5 - cw) if ax else (x + 0.5 - cw)
                        if acr ** 2 + (y + 0.5 - b[1]) ** 2 <= r * r:
                            out.append((x, y, z))
        elif t == "disc" and op.get("yaw", 0):
            a = math.radians(op["yaw"]) + (0 if op.get("axis", "z") == "x" else math.pi / 2)
            n = (math.cos(a), math.sin(a)); ri = math.ceil(r) + 2; half = max(op["thick"] * 0.5, 0.5)
            for y in range(int(b[1]) - ri, int(b[1]) + ri + 1):
                dy = y + 0.5 - b[1]
                for z in range(int(b[2]) - ri, int(b[2]) + ri + 1):
                    for x in range(int(b[0]) - ri, int(b[0]) + ri + 1):
                        u, v = x + 0.5 - b[0], z + 0.5 - b[2]
                        w = u * n[0] + v * n[1]; sd = -u * n[1] + v * n[0]
                        if abs(w) <= half and sd * sd + dy * dy <= r * r:
                            out.append((x, y, z))
        else:  # disc
            ax = op.get("axis", "z") == "x"; ri = math.ceil(r) + 1; ti = math.ceil(op["thick"] * 0.5)
            ci = 2 if ax else 0
            for y in range(int(b[1]) - ri, int(b[1]) + ri + 1):
                for u in range(int(b[ci]) - ri, int(b[ci]) + ri + 1):
                    for w in range(-ti, max(ti, 1)):
                        x, z = (int(b[0]) + w, u) if ax else (u, int(b[2]) + w)
                        if (u + 0.5 - b[ci]) ** 2 + (y + 0.5 - b[1]) ** 2 <= r * r:
                            out.append((x, y, z))
        put(out, m)

    def run(ops, off):
        for op in ops:
            t = op["op"]
            if t == "box":
                m = res(op["mat"])
                if m is not None:
                    box(add(op["from"], off), add(op["to"], off), m)
            elif t == "set":
                m = res(op["mat"])
                if m is not None:
                    a = add(op["at"], off)
                    box(a, [a[0] + 1, a[1] + 1, a[2] + 1], m)
            elif t == "ellipsoid":
                m = res(op["mat"])
                if m is None:
                    continue
                c, r = add(op["centre"], off), op["radius"]
                lo = [max(0, math.floor(c[i] - r[i])) for i in range(3)]
                hi = [min(size[i] - 1, math.ceil(c[i] + r[i])) for i in range(3)]
                if any(hi[i] < lo[i] for i in range(3)):
                    continue
                xs, ys, zs = [np.arange(lo[i], hi[i] + 1) + 0.5 for i in range(3)]
                dd = ((xs[:, None, None] - c[0]) / r[0]) ** 2 + ((ys[None, :, None] - c[1]) / r[1]) ** 2 + ((zs[None, None, :] - c[2]) / r[2]) ** 2
                sub = g[lo[0]:hi[0] + 1, lo[1]:hi[1] + 1, lo[2]:hi[2] + 1]
                sub[dd <= 1.0] = m
            elif t == "segment":
                m = res(op["mat"])
                if m is None:
                    continue
                hull = max(op["radius"], 0.62)
                pts = [add(p, off) for p in op["points"]]
                for a, b in zip(pts, pts[1:]):
                    a, b = np.array(a, float), np.array(b, float)
                    lo = [max(0, math.floor(min(a[i], b[i]) - hull)) for i in range(3)]
                    hi = [min(size[i] - 1, math.ceil(max(a[i], b[i]) + hull)) for i in range(3)]
                    if any(hi[i] < lo[i] for i in range(3)):
                        continue
                    xs, ys, zs = np.meshgrid(*[np.arange(lo[i], hi[i] + 1) + 0.5 for i in range(3)], indexing="ij")
                    c = np.stack([xs, ys, zs], -1)
                    dv = b - a
                    l2 = max(dv @ dv, 1e-6)
                    tt = np.clip(((c - a) @ dv) / l2, 0, 1)
                    q = a + tt[..., None] * dv - c
                    inside = (q * q).sum(-1) < hull * hull
                    sub = g[lo[0]:hi[0] + 1, lo[1]:hi[1] + 1, lo[2]:hi[2] + 1]
                    if m == 0:
                        sub[inside] = 0
                    else:
                        sub[inside & (sub == 0)] = m
            elif t == "recolour_below":
                f, to = res(op["from"]), res(op["to"])
                if f is None or to is None:
                    continue
                y = max(0, min(Y, int(op["y"] + off[1])))
                sub = g[:, :y, :]
                sub[sub == f] = to
            elif t == "recolour":
                f, to = res(op["from"]), res(op["to"])
                if f is None or to is None:
                    continue
                a, b = add(op["at"], off), add(op["until"], off)
                a = [max(0, int(a[i])) for i in range(3)]
                b = [min(size[i], int(b[i])) for i in range(3)]
                sub = g[a[0]:b[0], a[1]:b[1], a[2]:b[2]]
                sub[sub == f] = to
            elif t == "repeat":
                for k in range(op["count"]):
                    run(op["ops"], [off[i] + op["step"][i] * k for i in range(3)])
            elif t in ("cylinder", "disc", "log"):
                m = res(op["mat"])
                if m is None:
                    continue
                fill_shape(op, add(op.get("base", op.get("centre")), off), m)
            else:
                print("unhandled op", t, file=sys.stderr)
    run(ops, [0, 0, 0])
    if unknown:
        print("unknown materials", unknown, file=sys.stderr)
    pal = np.array([[128, 128, 128]] + [list(colors[n])[:3] if not isinstance(colors[n], dict) else list(colors[n].get("rgb", colors[n].get("color", [128,128,128])))[:3] for n in names], float)
    return g, pal


def rescale_ops(ops, k):
    """Same shapes at k times the voxels per metre."""
    out = []
    R = lambda v: int(round(v * k))
    for op in ops:
        o = dict(op)
        t = op["op"]
        if t == "ellipsoid":
            o["centre"] = [c * k for c in op["centre"]]
            o["radius"] = [r * k for r in op["radius"]]
        elif t == "segment":
            o["points"] = [[c * k for c in p] for p in op["points"]]
            o["radius"] = op["radius"] * k
        elif t in ("box", "recolour"):
            a, b = ("from", "to") if t == "box" else ("at", "until")
            fa = [R(v) for v in op[a]]
            fb = [max(R(op[b][i]), fa[i] + 1) for i in range(3)]
            o[a], o[b] = fa, fb
        elif t == "set":
            fa = [R(v) for v in op["at"]]
            fb = [max(R(v + 1), fa[i] + 1) for i, v in enumerate(op["at"])]
            o = {"op": "box", "from": fa, "to": fb, "mat": op["mat"]}
        elif t == "recolour_below":
            o["y"] = R(op["y"])
        out.append(o)
    return out


def build(name, vph=None):
    """Voxel grid for a creature, optionally re-rasterized at `vph` voxels
    per body height. Returns grid, palette, voxel size (m)."""
    d = load(name)
    size, ops, voxel = grid_of(d), d["ops"], d["voxel"]
    if vph:
        k = vph / (d["height"] / voxel)
        ops = rescale_ops(ops, k)
        size = [int(math.ceil(s * k)) + 2 for s in size]
        voxel = voxel / k
    g, pal = compile_ops(ops, size, d["colors"])
    return g, pal, voxel, d


def downsample(g, k, frac=0.25):
    X, Y, Z = [-(-s // k) * k for s in g.shape]
    p = np.zeros((X, Y, Z), np.uint8)
    p[: g.shape[0], : g.shape[1], : g.shape[2]] = g
    blocks = p.reshape(X // k, k, Y // k, k, Z // k, k).transpose(0, 2, 4, 1, 3, 5).reshape(X // k, Y // k, Z // k, k ** 3)
    solid = (blocks > 0).sum(-1)
    out = np.zeros(blocks.shape[:3], np.uint8)
    keep = solid >= frac * k ** 3
    for idx in zip(*np.nonzero(keep)):
        v = blocks[idx]
        v = v[v > 0]
        out[idx] = np.bincount(v).argmax()
    return out


# ---------------------------------------------------------------- meshing
DIRS = []
for na in range(3):
    ua, va = [a for a in range(3) if a != na]
    for sign in (1, -1):
        DIRS.append((na, sign, ua, va))


def padded(g):
    p = np.zeros([s + 2 for s in g.shape], g.dtype)
    p[1:-1, 1:-1, 1:-1] = g
    return p


def mesh_square(g):
    """Greedy-merged faces by colour. Returns list of (corners 4x3, normal, colour)."""
    p = padded(g)
    quads = []
    for na, sign, ua, va in DIRS:
        nb = np.roll(p, -sign, axis=na)
        exposed = np.where((p > 0) & (nb == 0), p, 0)
        for s in range(p.shape[na]):
            sl = [slice(None)] * 3
            sl[na] = s
            m = exposed[tuple(sl)]  # 2D in remaining axes order (ua, va) since ua<va
            if not m.any():
                continue
            m = m.copy()
            U, V = m.shape
            for u in range(U):
                v = 0
                while v < V:
                    c = m[u, v]
                    if c == 0:
                        v += 1
                        continue
                    w = 1
                    while v + w < V and m[u, v + w] == c:
                        w += 1
                    h = 1
                    while u + h < U and (m[u + h, v : v + w] == c).all():
                        h += 1
                    m[u : u + h, v : v + w] = 0
                    plane = s + (1 if sign > 0 else 0)
                    cs = []
                    for du, dv in ((0, 0), (h, 0), (h, w), (0, w)):
                        q = [0, 0, 0]
                        q[na] = plane
                        q[ua] = u + du
                        q[va] = v + dv
                        cs.append(q)
                    n = [0, 0, 0]
                    n[na] = sign
                    quads.append((np.array(cs, float) - 1, np.array(n, float), c))
                    v += w
    return quads


TAUBIN = (3, 0.5, -0.53, 0.5)  # passes, lambda, mu, how far a vertex may move


def mesh_sloped(g):
    """Surface nets as src/mesh/decimate.rs `surface`: one quad per exposed
    voxel face, each corner at the mean of the face crossings around it
    (halved along an axis where the voxels are one thick), then Taubin
    smoothing. Returns vertex array, faces (quads of vertex ids), face colour,
    face dir."""
    pg = padded(g)
    p = pg > 0
    S = p.shape

    def solid(v):
        return 0 <= v[0] < S[0] and 0 <= v[1] < S[1] and 0 <= v[2] < S[2] and p[v[0], v[1], v[2]]

    def corner(l):
        at = lambda i, j, k: (l[0] - 1 + i, l[1] - 1 + j, l[2] - 1 + k)
        s = [[[solid(at(i, j, k)) for k in range(2)] for j in range(2)] for i in range(2)]
        sm, n = np.zeros(3), 0
        for a in range(2):
            for b in range(2):
                ha, hb = a - 0.5, b - 0.5
                if s[0][a][b] != s[1][a][b]:
                    sm += (0, ha, hb); n += 1
                if s[a][0][b] != s[a][1][b]:
                    sm += (ha, 0, hb); n += 1
                if s[a][b][0] != s[a][b][1]:
                    sm += (ha, hb, 0); n += 1
        o = sm / n if n else np.zeros(3)
        thin_any = False
        for ax in range(3):
            thin = True
            for i in range(8):
                v = at(i & 1, i >> 1 & 1, i >> 2 & 1)
                if not solid(v):
                    continue
                lo, hi = list(v), list(v)
                lo[ax] -= 1
                hi[ax] += 1
                if solid(lo) or solid(hi):
                    thin = False
                    break
            if thin:
                o[ax] /= 2
                thin_any = True
        return np.array(l, float) + o - 1, thin_any

    vid, verts, thin, quads, cols, dirs = {}, [], [], [], [], []
    for na, sign, ua, va in DIRS:
        nb = np.roll(pg, -sign, axis=na)
        for gx in np.argwhere((pg > 0) & (nb == 0)):
            ids = []
            for k in range(4):
                l = list(gx)
                if sign > 0:
                    l[na] += 1
                l[ua] += 1 if k in (1, 2) else 0
                l[va] += 1 if k >= 2 else 0
                key = tuple(l)
                if key not in vid:
                    vid[key] = len(verts)
                    pos, th = corner(l)
                    verts.append(pos)
                    thin.append(th)
                ids.append(vid[key])
            quads.append(ids)
            cols.append(pg[tuple(gx)])
            n = [0, 0, 0]
            n[na] = sign
            dirs.append(n)
    V, Q, C = np.array(verts), np.array(quads), np.array(cols)
    return taubin(V, Q, C, np.array(thin)), Q, C, np.array(dirs, float)


def taubin(V, Q, C, thin):
    """Shrink and inflate steps over the quad edges. Each vertex stays within
    half a voxel of where surface nets put it, and moves half as far where
    the voxels are one thick or on a colour boundary."""
    passes, lam, mu, clamp = TAUBIN
    n = len(V)
    nb = [set() for _ in range(n)]
    cols = [set() for _ in range(n)]
    for q, c in zip(Q, C):
        for k in range(4):
            a, b = q[k], q[(k + 1) % 4]
            nb[a].add(b); nb[b].add(a)
            cols[a].add(c)
    nb = [np.array(sorted(x)) for x in nb]
    half = thin | np.array([len(c) > 1 for c in cols])
    f = np.where(half, 0.5, 1.0)[:, None]
    X = V.copy()
    for _ in range(passes):
        for w in (lam, mu):
            X = X + w * f * np.array([X[nb[i]].mean(0) - X[i] for i in range(n)])
        X = V + np.clip(X - V, -clamp, clamp)
    return X


def quads_to_tris(V, Q, C, D):
    tris, tc = [], []
    for q, c, dn in zip(Q, C, D):
        a, b, cc, d = q
        # shorter diagonal
        if np.linalg.norm(V[a] - V[cc]) <= np.linalg.norm(V[b] - V[d]):
            t1, t2 = (a, b, cc), (a, cc, d)
        else:
            t1, t2 = (a, b, d), (b, cc, d)
        for t in (t1, t2):
            n = np.cross(V[t[1]] - V[t[0]], V[t[2]] - V[t[0]])
            if n @ dn < 0:
                t = (t[0], t[2], t[1])
            tris.append(t)
            tc.append(c)
    return np.array(tris), np.array(tc)


# ---------------------------------------------------------------- decimation
BOUNDARY_W = 8.0
VOL_W = 2.0
SLIVER = math.radians(18)


def plane_q(n, p0, w):
    p = np.append(n, -n @ p0)
    return np.outer(p, p) * w


def tri_min_angle(p0, p1, p2):
    m = math.pi
    for a, b, c in ((p0, p1, p2), (p1, p2, p0), (p2, p0, p1)):
        u, w = b - a, c - a
        lu, lw = np.linalg.norm(u), np.linalg.norm(w)
        if lu < 1e-9 or lw < 1e-9:
            return 0.0
        m = min(m, math.acos(max(-1, min(1, (u @ w) / (lu * lw)))))
    return m


def decimate(V, T, C, target, max_turn_deg=45.0):
    """Garland-Heckbert edge collapse as src/mesh/decimate.rs: face planes
    weighted by the square root of their area, colour boundaries held by
    planes along them, the merged point solved with a plane that keeps the
    volume and a slight pull to the middle of the edge, collapses that leave
    slivers put back, and the link condition and a turn limit. Returns V, T, C
    of the result."""
    V = [np.array(v, float) for v in V]
    T = [list(t) for t in T]
    C = list(C)
    alive = [True] * len(T)
    nV = len(V)
    vf = [set() for _ in range(nV)]
    for i, t in enumerate(T):
        for v in t:
            vf[v].add(i)
    Q = [np.zeros((4, 4)) for _ in range(nV)]
    fnormal = lambda t: np.cross(V[t[1]] - V[t[0]], V[t[2]] - V[t[0]])
    for t in T:
        n = fnormal(t)
        a = np.linalg.norm(n)
        if a < 1e-12:
            continue
        K = plane_q(n / a, V[t[0]], math.sqrt(a / 2))
        for v in t:
            Q[v] += K
    edge_faces = {}
    for i, t in enumerate(T):
        for k in range(3):
            edge_faces.setdefault(tuple(sorted((t[k], t[(k + 1) % 3]))), []).append(i)
    for e, fs in edge_faces.items():
        if len(fs) == 2 and C[fs[0]] == C[fs[1]]:
            continue
        ed = V[e[1]] - V[e[0]]
        L = np.linalg.norm(ed)
        if L < 1e-9:
            continue
        for f in fs:
            n = fnormal(T[f])
            a = np.linalg.norm(n)
            if a < 1e-12:
                continue
            m = np.cross(ed, n / a)
            lm = np.linalg.norm(m)
            if lm < 1e-12:
                continue
            K = plane_q(m / lm, V[e[0]], BOUNDARY_W * L * L)
            Q[e[0]] += K
            Q[e[1]] += K

    stamp = [0] * nV
    vdead = [False] * nV
    heap = []
    cos_turn = math.cos(math.radians(max_turn_deg))

    def best(a, b):
        Qs = Q[a] + Q[b]
        shared = vf[a] & vf[b]
        g, d = np.zeros(3), 0.0
        rem = []
        for f in vf[a] | vf[b]:
            t = T[f]
            x = a if a in t else b
            k = t.index(x)
            i1, i2 = t[(k + 1) % 3], t[(k + 2) % 3]
            d += V[x] @ np.cross(V[i1], V[i2])
            if f not in shared:
                g += np.cross(V[i1], V[i2])
                rem.append((i1, i2))
        mid = (V[a] + V[b]) / 2
        A = Qs[:3, :3].copy()
        rhs = -Qs[:3, 3].copy()
        gl = np.linalg.norm(g)
        if gl > 1e-9:
            nv = g / gl
            A += VOL_W * np.outer(nv, nv)
            rhs += VOL_W * nv * (d / gl)
        eps = 1e-3 * max(np.trace(A), 1e-9)
        x = np.linalg.solve(A + eps * np.eye(3), rhs + eps * mid)
        cands = [V[a], V[b]]
        if np.linalg.norm(x - mid) < 0.75 + np.linalg.norm(V[a] - V[b]) / 2:
            cands.append(x)
        bc, bp = None, None
        for c in cands:
            h = np.append(c, 1)
            cost = h @ Qs @ h
            if bc is None or cost < bc:
                bc, bp = cost, c
        bc = max(bc, 0.0)
        if any(tri_min_angle(bp, V[i1], V[i2]) < SLIVER for i1, i2 in rem):
            bc *= 3
        return bc, bp

    def push(a, b):
        if a > b:
            a, b = b, a
        c, _ = best(a, b)
        heapq.heappush(heap, (c, a, b, stamp[a], stamp[b]))

    def nbrs(v):
        s = set()
        for f in vf[v]:
            s.update(T[f])
        s.discard(v)
        return s

    for e in edge_faces:
        push(*e)
    count = len(T)
    while count > target and heap:
        c, a, b, sa, sb = heapq.heappop(heap)
        if vdead[a] or vdead[b] or stamp[a] != sa or stamp[b] != sb:
            continue
        shared = vf[a] & vf[b]
        if not shared:
            continue
        opp = set()
        for f in shared:
            opp.update(T[f])
        opp -= {a, b}
        if (nbrs(a) & nbrs(b)) != opp:
            continue
        _, pnew = best(a, b)
        ok = True
        for v in (a, b):
            for f in vf[v] - shared:
                t = T[f]
                n0 = fnormal(t)
                old = V[v]
                V[v] = pnew
                n1 = fnormal(t)
                V[v] = old
                l0, l1 = np.linalg.norm(n0), np.linalg.norm(n1)
                if l1 < 1e-12 or (l0 > 1e-12 and (n0 @ n1) / (l0 * l1) < cos_turn):
                    ok = False
                    break
            if not ok:
                break
        if not ok:
            continue
        V[a] = pnew
        Q[a] = Q[a] + Q[b]
        for f in shared:
            alive[f] = False
            for v in T[f]:
                vf[v].discard(f)
            count -= 1
        for f in list(vf[b]):
            T[f] = [a if v == b else v for v in T[f]]
            vf[a].add(f)
        vf[b] = set()
        vdead[b] = True
        stamp[a] += 1
        for n in nbrs(a):
            push(a, n)
    keep = [i for i in range(len(T)) if alive[i]]
    used = sorted({v for i in keep for v in T[i]})
    remap = {v: k for k, v in enumerate(used)}
    return (np.array([V[v] for v in used]), np.array([[remap[v] for v in T[i]] for i in keep]),
            np.array([C[i] for i in keep]))


def bake_levels(V, T, C, base):
    """The near and far meshes, as models.rs bakes them: the near to half the
    square model's triangles, the far from the near to a quarter."""
    near = decimate(V, T, C, base // 2, max_turn_deg=45)
    return near, decimate(*near, base // 4, max_turn_deg=60)


# ---------------------------------------------------------------- render
class Scene:
    """Triangles in voxel coords with per-corner normals and per-face colour."""

    def __init__(self, P, N, C):
        self.P, self.N, self.C = P, N, C  # (n,3,3), (n,3,3), (n,3)


def scene_square(quads, pal):
    P, N, C = [], [], []
    for cs, n, c in quads:
        for t in ((0, 1, 2), (0, 2, 3)):
            P.append(cs[list(t)])
            N.append([n] * 3)
            C.append(pal[c])
    return Scene(np.array(P), np.array(N, float), np.array(C))


def scene_flat(V, T, C, pal):
    P = V[T]
    n = np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0])
    n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)
    return Scene(P, np.repeat(n[:, None], 3, 1), pal[C])


def scene_smooth(V, T, C, pal, crease=0.5):
    """Corner normals from the faces round the vertex that bend less than 60
    degrees from this one, each weighted by its angle at the vertex."""
    P = V[T]
    fn = np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0])
    fu = fn / np.maximum(np.linalg.norm(fn, axis=1, keepdims=True), 1e-12)
    ang = np.zeros((len(T), 3))
    for k in range(3):
        u = P[:, (k + 1) % 3] - P[:, k]
        w = P[:, (k + 2) % 3] - P[:, k]
        cs = (u * w).sum(1) / np.maximum(np.linalg.norm(u, axis=1) * np.linalg.norm(w, axis=1), 1e-12)
        ang[:, k] = np.arccos(np.clip(cs, -1, 1))
    vf = [[] for _ in range(len(V))]
    for i, t in enumerate(T):
        for k, v in enumerate(t):
            vf[v].append((i, k))
    N = np.zeros(P.shape)
    for i, t in enumerate(T):
        for k, v in enumerate(t):
            s = np.zeros(3)
            for f, kk in vf[v]:
                if fu[f] @ fu[i] > crease:
                    s += fu[f] * ang[f, kk]
            N[i, k] = s / max(np.linalg.norm(s), 1e-12)
    return Scene(P, N, pal[C])


def view_basis(az_deg, el_deg):
    az, el = math.radians(az_deg), math.radians(el_deg)
    fwd = np.array([math.sin(az) * math.cos(el), -math.sin(el), math.cos(az) * math.cos(el)])  # camera looks along fwd
    right = np.cross(fwd, [0, 1, 0])
    right /= np.linalg.norm(right)
    up = np.cross(right, fwd)
    return right, up, fwd


def render(scene, solid_grid, W, H, view=(35, 18), fit=None, ss=3, sun=(-0.45, 0.8, -0.4), unit=1.0, ppm=None):
    """Orthographic, deferred: lambert sun + sky, AO from the blurred voxel
    field. `fit` = (centre, half_extent) in units of `unit` (metres when unit
    is the voxel size), shared across panels. `ppm` fixes pixels per unit."""
    right, up, fwd = view_basis(*view)
    P = scene.P
    if fit is None:
        flat = P.reshape(-1, 3) * unit
        lo, hi = flat.min(0), flat.max(0)
        fit = ((lo + hi) / 2, np.linalg.norm(hi - lo) / 2)
    centre, half = fit
    w, h = W * ss, H * ss
    scale = ppm * ss if ppm else min(w, h) / (2 * half * 1.02)
    rel = P * unit - centre
    sx = rel @ right * scale + w / 2
    sy = -(rel @ up) * scale + h / 2
    sz = rel @ fwd
    zbuf = np.full((h, w), np.inf)
    nbuf = np.zeros((h, w, 3))
    cbuf = np.zeros((h, w, 3))
    pbuf = np.zeros((h, w, 3))
    hit = np.zeros((h, w), bool)
    for i in range(len(P)):
        x, y, z = sx[i], sy[i], sz[i]
        x0, x1 = max(int(math.floor(x.min())), 0), min(int(math.ceil(x.max())), w - 1)
        y0, y1 = max(int(math.floor(y.min())), 0), min(int(math.ceil(y.max())), h - 1)
        if x1 < x0 or y1 < y0:
            continue
        area = (x[1] - x[0]) * (y[2] - y[0]) - (x[2] - x[0]) * (y[1] - y[0])
        if abs(area) < 1e-9:
            continue
        px, py = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
        w0 = ((x[1] - px) * (y[2] - py) - (x[2] - px) * (y[1] - py)) / area
        w1 = ((x[2] - px) * (y[0] - py) - (x[0] - px) * (y[2] - py)) / area
        w2 = 1 - w0 - w1
        inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (w2 >= -1e-6)
        if not inside.any():
            continue
        zz = w0 * z[0] + w1 * z[1] + w2 * z[2]
        sub = zbuf[y0 : y1 + 1, x0 : x1 + 1]
        m = inside & (zz < sub)
        if not m.any():
            continue
        sub[m] = zz[m]
        n = scene.N[i]
        nn = w0[..., None] * n[0] + w1[..., None] * n[1] + w2[..., None] * n[2]
        pp = w0[..., None] * P[i, 0] + w1[..., None] * P[i, 1] + w2[..., None] * P[i, 2]
        nbuf[y0 : y1 + 1, x0 : x1 + 1][m] = nn[m]
        pbuf[y0 : y1 + 1, x0 : x1 + 1][m] = pp[m]
        cbuf[y0 : y1 + 1, x0 : x1 + 1][m] = scene.C[i]
        hit[y0 : y1 + 1, x0 : x1 + 1][m] = True
    # a colour looked up per pixel from where it is in the model, if the scene has one
    if getattr(scene, "lookup", None) is not None and hit.any():
        cbuf[hit] = scene.lookup(pbuf[hit])
    # shade
    n = nbuf / np.maximum(np.linalg.norm(nbuf, axis=-1, keepdims=True), 1e-9)
    L = np.array(sun, float)
    L /= np.linalg.norm(L)
    diff = np.clip(n @ L, 0, 1)
    sky = 0.55 + 0.2 * n[..., 1]
    field = gaussian_filter((padded(solid_grid) > 0).astype(float), 1.2)
    q = pbuf + 1 + n * 0.8
    from scipy.ndimage import map_coordinates
    occ = map_coordinates(field, [q[..., 0] - 0.5, q[..., 1] - 0.5, q[..., 2] - 0.5], order=1, mode="constant")
    ao = np.clip(1.25 - 1.3 * occ, 0.45, 1.0)
    lin = (cbuf / 255.0) ** 2.2
    light = sky * ao + 0.85 * diff * np.sqrt(ao)
    col = lin * light[..., None]
    col = np.clip(col, 0, 1) ** (1 / 2.2)
    # background
    gy = np.linspace(0, 1, h)[:, None, None]
    bg = (1 - gy) * np.array([0.86, 0.89, 0.93]) + gy * np.array([0.74, 0.77, 0.80])
    img = np.where(hit[..., None], col, bg)
    img = img.reshape(H, ss, W, ss, 3).mean((1, 3))
    return (img * 255).astype(np.uint8), fit


def tri_count_square(quads):
    return 2 * len(quads)
