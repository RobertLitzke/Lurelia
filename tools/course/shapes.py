"""The arena format's solid shapes, rasterised to quarter-metre voxels and cut
into box ops (FORMAT.md, "Shapes and structures").

Taken from the arena importer's `layout.raster` and `boxes.decompose`
(git-ignored), neither of which reads any imported data, so a
fresh checkout can build a track without third_party/. Only what a track
uses is here: no `terrain` grids and no `pool`.
"""

from __future__ import annotations

import numpy as np

V = 0.25  # metres to the voxel
SHAPES = ("box", "prism", "section", "pyramid", "stairs", "ramp")


class ShapeError(Exception):
    pass


def bounds(words):
    """The box (x0, y0, z0), (x1, y1, z1) in metres that a shape's words
    cover, from its numbers."""
    kind, a = words[0], words[1:]

    def pairs(ws):
        return [tuple(float(v) for v in p.split(",")) for p in ws]

    if kind == "box":
        x0, z0, x1, z1, y0, y1 = map(float, a[:6])
        return (min(x0, x1), min(y0, y1), min(z0, z1)), (max(x0, x1), max(y0, y1), max(z0, z1))
    if kind in ("stairs", "ramp"):
        x0, z0, x1, z1 = map(float, a[:4])
        y0, y1 = float(a[5]), float(a[6])
        return (min(x0, x1), min(y0, y1), min(z0, z1)), (max(x0, x1), max(y0, y1), max(z0, z1))
    if kind == "prism":
        y0, y1 = float(a[0]), float(a[1])
        ps = pairs(a[2:])
        xs, zs = [p[0] for p in ps], [p[1] for p in ps]
        return (min(xs), y0, min(zs)), (max(xs), y1, max(zs))
    if kind == "section":
        axis, l0, l1 = a[0], float(a[1]), float(a[2])
        ps = pairs(a[3:])
        across, ys = [p[0] for p in ps], [p[1] for p in ps]
        if axis == "z":
            return (min(across), min(ys), l0), (max(across), max(ys), l1)
        return (l0, min(ys), min(across)), (l1, max(ys), max(across))
    if kind == "pyramid":
        x0, z0, x1, z1, y = map(float, a[:5])
        ax, az, ay = (float(v) for v in a[5].split(","))
        return (min(x0, ax), min(y, ay), min(z0, az)), (max(x1, ax), max(y, ay), max(z1, az))
    raise ShapeError(f"no such shape as {kind} ({', '.join(SHAPES)})")


def inside(words, shape_vox, origin_m):
    """Which voxels of a grid [y, z, x] of `shape_vox`, its corner at
    `origin_m`, the shape fills."""
    sy, sz, sx = shape_vox
    solid = np.zeros(shape_vox, dtype=bool)
    cx = origin_m[0] + (np.arange(sx) + 0.5) * V
    cy = origin_m[1] + (np.arange(sy) + 0.5) * V
    cz = origin_m[2] + (np.arange(sz) + 0.5) * V

    def sl(c, a, b):
        return slice(int(np.searchsorted(c, min(a, b))), int(np.searchsorted(c, max(a, b))))

    kind, a = words[0], words[1:]
    if kind == "box":
        x0, z0, x1, z1, y0, y1 = map(float, a[:6])
        solid[sl(cy, y0, y1), sl(cz, z0, z1), sl(cx, x0, x1)] = True
    elif kind == "prism":
        y0, y1 = float(a[0]), float(a[1])
        ring = np.array([[float(v) for v in p.split(",")] for p in a[2:]])
        xs, zs = sl(cx, ring[:, 0].min(), ring[:, 0].max()), sl(cz, ring[:, 1].min(), ring[:, 1].max())
        X, Z = np.meshgrid(cx[xs], cz[zs])
        hit = np.zeros(X.shape, dtype=bool)
        n = len(ring)
        for e in range(n):
            (xa, za), (xb, zb) = ring[e], ring[(e + 1) % n]
            hit ^= ((za > Z) != (zb > Z)) & (X < (xb - xa) * (Z - za) / (zb - za + 1e-12) + xa)
        ysl = sl(cy, y0, y1)
        block = solid[ysl, zs, xs]
        block[:, hit] = True
        solid[ysl, zs, xs] = block
    elif kind == "pyramid":
        x0, z0, x1, z1, yb = map(float, a[:5])
        ax, az, ay = (float(v) for v in a[5].split(","))
        ysl = sl(cy, min(yb, ay), max(yb, ay))
        for j, yv in zip(range(ysl.start, ysl.stop), cy[ysl]):
            f = (yv - yb) / (ay - yb)
            solid[j, sl(cz, z0 + (az - z0) * f, z1 + (az - z1) * f), sl(cx, x0 + (ax - x0) * f, x1 + (ax - x1) * f)] = True
    elif kind == "section":
        ax, a0, a1 = a[0], float(a[1]), float(a[2])
        ring = np.array([[float(v) for v in p.split(",")] for p in a[3:]])
        u_c = cz if ax == "x" else cx
        us = sl(u_c, ring[:, 0].min(), ring[:, 0].max())
        ysl = sl(cy, ring[:, 1].min(), ring[:, 1].max())
        U, Y = np.meshgrid(u_c[us], cy[ysl])  # [y, u]
        hit = np.zeros(U.shape, dtype=bool)
        n = len(ring)
        for e in range(n):
            (ua, ya), (ub, yb) = ring[e], ring[(e + 1) % n]
            hit ^= ((ya > Y) != (yb > Y)) & (U < (ub - ua) * (Y - ya) / (yb - ya + 1e-12) + ua)
        if ax == "x":
            block = solid[ysl, us, sl(cx, a0, a1)]
            block[hit] = True
            solid[ysl, us, sl(cx, a0, a1)] = block
        else:
            block = solid[ysl, sl(cz, a0, a1), us]
            block[np.broadcast_to(hit[:, None, :], block.shape)] = True
            solid[ysl, sl(cz, a0, a1), us] = block
    elif kind in ("stairs", "ramp"):
        x0, z0, x1, z1 = map(float, a[:4])
        toward, y0, y1 = a[4], float(a[5]), float(a[6])
        xs, zs = sl(cx, x0, x1), sl(cz, z0, z1)
        X, Z = np.meshgrid(cx[xs], cz[zs])
        t = {"e": (X - x0) / (x1 - x0), "w": (x1 - X) / (x1 - x0), "s": (Z - z0) / (z1 - z0), "n": (z1 - Z) / (z1 - z0)}[toward]
        t = np.clip(t, 0, 1)
        if kind == "stairs":
            n = max(1, int(round((y1 - y0) / 0.25)))
            top = y0 + (np.minimum(np.floor(t * n), n - 1) + 1) * (y1 - y0) / n
        else:
            # At least one voxel thick, or the low end's last column comes
            # out empty and leaves a slot under the ramp.
            top = np.maximum(y0 + (y1 - y0) * t, min(y0, y1) + 0.25)
        ysl = sl(cy, y0, max(y0, y1))
        m = cy[ysl][:, None, None] < top[None]
        block = solid[ysl, zs, xs]
        block[m] = True
        solid[ysl, zs, xs] = block
    else:
        raise ShapeError(f"no such shape as {kind} ({', '.join(SHAPES)})")
    return solid


def decompose(grid):
    """Greedy boxes over a [y, z, x] grid of labels (0 for nothing): along x,
    then z, then y, so floors and walls come out as slabs. Yields
    (label, (x0, y0, z0), (x1, y1, z1)) with the high corner exclusive."""
    grid = np.ascontiguousarray(grid.copy())
    sy, sz, sx = grid.shape
    out = []
    for y in range(sy):
        layer = grid[y]
        flat = layer.reshape(-1)
        for pos in np.flatnonzero(flat):
            label = flat[pos]
            if label == 0:
                continue  # taken by an earlier box
            z0, x0 = divmod(int(pos), sx)
            row = layer[z0]
            x1 = x0 + 1
            while x1 < sx and row[x1] == label:
                x1 += 1
            z1 = z0 + 1
            while z1 < sz and bool((layer[z1, x0:x1] == label).all()):
                z1 += 1
            y1 = y + 1
            while y1 < sy and bool((grid[y1, z0:z1, x0:x1] == label).all()):
                y1 += 1
            grid[y:y1, z0:z1, x0:x1] = 0
            out.append((int(label), (x0, y, z0), (x1, y1, z1)))
    return out


def boxes(words):
    """A shape as box ops in world voxels: (origin [x, y, z], size [x, y, z],
    boxes [[x0, y0, z0, x1, y1, z1], ...] from the origin). Empty boxes
    when it covers no voxel."""
    a, b = bounds(words)
    lo = [int(np.floor(v / V)) - 1 for v in a]
    hi = [int(np.ceil(v / V)) + 1 for v in b]
    sx, sy, sz = (max(1, h - l) for l, h in zip(lo, hi))
    if sx * sy * sz > 60_000_000:
        raise ShapeError(f"{sx * sy * sz / 1e6:.0f} M voxels is too big for one shape; cut it up")
    grid = inside(words, (sy, sz, sx), [v * V for v in lo])
    out = [[*p, *q] for _, p, q in decompose(grid.astype(np.uint8))]
    return lo, [sx, sy, sz], out
