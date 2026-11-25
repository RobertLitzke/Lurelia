//! Baked creature meshes: a creature's voxels turned into a smooth triangle
//! mesh and decimated to a budget, for the smooth creature mode
//! (docs/CREATURES.md "Model resolution and decimation").
//!
//! The steps: surface nets over the whole body (one quad per exposed voxel
//! face, each corner at the mean of the face midpoints around it), three
//! passes of Taubin smoothing to round off the voxel steps without thinning
//! the body, each quad split along its shorter diagonal, then quadric edge
//! collapse (Garland and Heckbert) down to the target triangle count. Face
//! planes are weighted by the square root of their area, so small faces on
//! thin limbs still count. Colour boundaries and the boundaries between bones
//! are held by constraint planes along them, so a patch keeps its outline and
//! a joint stays where it was. Each merged point keeps the volume around its
//! edge (`best`), so legs and necks don't slim; collapses that break the
//! surface's topology (the link condition) or turn a face too far are
//! skipped. Each face corner then gets a normal from the faces around it that
//! bend less than `CREASE`, weighted by their angle at the corner, so a
//! rounded body shades round and a sharp edge stays sharp, and the mesh is
//! split by bone, one per part. docs/CREATURES.md "What the bake replaced"
//! has what else was tried.
//!
//! The body is one surface, so where a part meets its parent the two share a
//! ring of vertices. In the part's mesh that ring follows the parent's
//! rotation and the ring next to it a third of the way (the seam weight), so
//! when the joint bends the surface stretches instead of opening.
//!
//! Vertices are packed four words each, as the renderer reads them
//! (`vs_entity_tri` in scene.wgsl), three to a triangle: word0 x, y as f16;
//! word1 z as f16 and the palette block in the top half (filled in at upload,
//! as a quad's is); word2 the normal, octahedral, two snorm16; word3 the
//! voxel's material in bits 0..8, how open the air in front of the vertex is
//! in 8..16 (0 to 255 for the shader's ambient occlusion of 0 to 3), and the
//! seam weight in 16..24 (0 to 255).
//!
//! `tools/creature_lod/meshlab.py` is the same bake in Python, for sheets.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Faces bending more than this from each other (cosine) shade apart.
const CREASE: f64 = 0.5;
/// How hard a colour or bone boundary holds its line against collapses.
const BOUNDARY_W: f64 = 8.0;
/// A collapse that turns a face more than this (cosine) is refused: 45
/// degrees for the near mesh, 60 for the far.
const NEAR_TURN: f64 = std::f64::consts::FRAC_1_SQRT_2;
const FAR_TURN: f64 = 0.5;
/// How hard the plane that keeps the volume holds a merged point.
const VOL_W: f64 = 2.0;
/// A collapse leaving a triangle with an angle under this (18 degrees) costs
/// three times as much.
const SLIVER: f64 = 0.314;
/// How far the second ring of vertices from a joint follows the parent.
const SECOND_RING: f64 = 1.0 / 3.0;

pub type Vert = [u32; 4];

/// A creature's voxels to bake: its grid (`dims`, x fastest then z then y,
/// palette slot + 1, 0 empty, as `anim::slice::Dims` lays it out), the bone
/// owning each voxel (None: all one part), each bone's parent, and where each
/// part's window starts in the grid, which its vertices are given from.
pub struct Body<'a> {
    pub cells: &'a [u8],
    pub dims: [i32; 3],
    pub owner: Option<&'a [u8]>,
    pub parent: &'a [Option<usize>],
    /// Bones baked as surfaces of their own, with their children (empty:
    /// none).
    pub loose: &'a [bool],
    pub origins: &'a [[i32; 3]],
}

/// Each part's near and far meshes: the whole body decimated to `near`
/// triangles, then on to `far`, and split by bone.
///
/// A loose bone and the bones under it are a piece of their own, meshed
/// apart from the rest, so the faces where it lies against other parts exist
/// on both sides and nothing is open when it swings away. The budget is
/// shared out by each piece's share of the faces.
pub fn bake_body(body: &Body, near: usize, far: usize) -> Vec<(Vec<Vert>, Vec<Vert>)> {
    let parts = body.parent.len().max(1);
    let piece = pieces(body);
    let count = piece.iter().max().map_or(1, |&m| m + 1);
    let surfaces: Vec<(Pad, _)> = (0..count)
        .map(|p| {
            let pad = Pad::new(body, (count > 1).then_some((&piece[..], p)));
            let s = surface(&pad);
            (pad, s)
        })
        .collect();
    let total: usize = surfaces.iter().map(|(_, s)| s.1.len()).sum();
    let mut out = vec![(Vec::new(), Vec::new()); parts];
    for (pad, (v, t, k)) in &surfaces {
        if t.is_empty() {
            continue;
        }
        let share = |budget: usize, min: usize| ((budget as f64 * t.len() as f64 / total as f64).round() as usize).max(min);
        let n = decimate(v, t, k, share(near, 12).min(t.len()), NEAR_TURN);
        let f = decimate(&n.0, &n.1, &n.2, share(far, 8).min(n.1.len()), FAR_TURN);
        let a = pack(&n.0, &n.1, &n.2, pad, body);
        let b = pack(&f.0, &f.1, &f.2, pad, body);
        for (o, (a, b)) in out.iter_mut().zip(a.into_iter().zip(b)) {
            o.0.extend(a);
            o.1.extend(b);
        }
    }
    out
}

/// Which piece each bone is meshed in: the root's (0) unless it or a bone
/// above it is loose. Parents come before their children.
fn pieces(body: &Body) -> Vec<usize> {
    let mut piece = vec![0; body.parent.len().max(1)];
    let mut next = 1;
    for b in 1..piece.len() {
        piece[b] = if body.loose.get(b).copied().unwrap_or(false) {
            next += 1;
            next - 1
        } else {
            body.parent[b].map_or(0, |p| piece[p])
        };
    }
    piece
}

/// The near and far meshes of one 32^3 grid that is a single part.
pub fn bake(cells: &[u8], near: usize, far: usize) -> (Vec<Vert>, Vec<Vert>) {
    let body = Body { cells, dims: [32; 3], owner: None, parent: &[None], loose: &[], origins: &[[0; 3]] };
    bake_body(&body, near, far).pop().unwrap_or_default()
}

// ---------------------------------------------------------------- surface

/// The grid with a layer of air round it, and each voxel's bone.
struct Pad {
    n: [i32; 3],
    colour: Vec<u8>,
    bone: Vec<u8>,
}

impl Pad {
    /// The body's voxels, or with a piece given (each bone's piece, and
    /// which one) only that piece's.
    fn new(b: &Body, only: Option<(&[usize], usize)>) -> Pad {
        let n = [b.dims[0] + 2, b.dims[1] + 2, b.dims[2] + 2];
        let len = (n[0] * n[1] * n[2]) as usize;
        let mut p = Pad { n, colour: vec![0; len], bone: vec![0; len] };
        let [dx, dy, dz] = b.dims;
        for y in 0..dy {
            for z in 0..dz {
                for x in 0..dx {
                    let i = ((y * dz + z) * dx + x) as usize;
                    let c = b.cells[i];
                    if c != 0 {
                        let bone = b.owner.map_or(0, |o| if o[i] == u8::MAX { 0 } else { o[i] });
                        if only.is_some_and(|(piece, p)| piece.get(bone as usize).copied().unwrap_or(0) != p) {
                            continue;
                        }
                        let k = p.idx(x + 1, y + 1, z + 1);
                        p.colour[k] = c;
                        p.bone[k] = bone;
                    }
                }
            }
        }
        p
    }
    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((x * self.n[1] + y) * self.n[2] + z) as usize
    }
    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        (0..self.n[0]).contains(&x) && (0..self.n[1]).contains(&y) && (0..self.n[2]).contains(&z) && self.colour[self.idx(x, y, z)] != 0
    }
}

/// Taubin smoothing of the surface nets: passes, the shrinking step, the
/// inflating step, and how far (per axis, in voxels) a vertex may end up from
/// where surface nets put it.
const TAUBIN: (usize, f64, f64, f64) = (3, 0.5, -0.53, 0.5);

/// Surface nets over the padded grid, smoothed: vertices in grid coordinates,
/// triangles facing out, and each triangle's key: its material (slot + 1) and
/// its voxel's bone above it.
#[allow(clippy::type_complexity)]
fn surface(p: &Pad) -> (Vec<[f64; 3]>, Vec<[u32; 3]>, Vec<u16>) {
    let solid = |x: i32, y: i32, z: i32| p.solid(x, y, z);
    let [px, py, pz] = p.n;
    // The surface point of lattice point l (a voxel corner): the mean of the
    // midpoints of the voxel faces around it, and whether the voxels there
    // are one thick along some axis.
    let corner = |l: [i32; 3]| -> ([f64; 3], bool) {
        let at = |i: i32, j: i32, k: i32| (l[0] - 1 + i, l[1] - 1 + j, l[2] - 1 + k);
        let mut s = [[[false; 2]; 2]; 2];
        let mut cnt = 0;
        for i in 0..2 {
            for j in 0..2 {
                for k in 0..2 {
                    let (x, y, z) = at(i, j, k);
                    s[i as usize][j as usize][k as usize] = solid(x, y, z);
                    cnt += s[i as usize][j as usize][k as usize] as i32;
                }
            }
        }
        let mut o = [0f64; 3];
        let mut thin_any = false;
        if cnt != 0 && cnt != 8 {
            let (mut sm, mut n) = ([0f64; 3], 0);
            for a in 0..2 {
                for b in 0..2 {
                    let (ha, hb) = (a as f64 - 0.5, b as f64 - 0.5);
                    if s[0][a][b] != s[1][a][b] {
                        sm[1] += ha;
                        sm[2] += hb;
                        n += 1;
                    }
                    if s[a][0][b] != s[a][1][b] {
                        sm[0] += ha;
                        sm[2] += hb;
                        n += 1;
                    }
                    if s[a][b][0] != s[a][b][1] {
                        sm[0] += ha;
                        sm[1] += hb;
                        n += 1;
                    }
                }
            }
            for k in 0..3 {
                o[k] = sm[k] / n as f64;
            }
            // A layer one voxel thin along an axis keeps its corners there
            // half as free, so it does not pinch to nothing.
            for (ax, ok) in o.iter_mut().enumerate() {
                let mut thin = true;
                'v: for i in 0..8 {
                    let (x, y, z) = at(i & 1, i >> 1 & 1, i >> 2 & 1);
                    if !solid(x, y, z) {
                        continue;
                    }
                    let mut lo = [x, y, z];
                    let mut hi = [x, y, z];
                    lo[ax] -= 1;
                    hi[ax] += 1;
                    if solid(lo[0], lo[1], lo[2]) || solid(hi[0], hi[1], hi[2]) {
                        thin = false;
                        break 'v;
                    }
                }
                if thin {
                    *ok /= 2.0;
                    thin_any = true;
                }
            }
        }
        // Lattice point in padded coordinates, less the pad.
        ([l[0] as f64 + o[0] - 1.0, l[1] as f64 + o[1] - 1.0, l[2] as f64 + o[2] - 1.0], thin_any)
    };
    let mut ids: std::collections::HashMap<[i32; 3], u32> = std::collections::HashMap::new();
    let (mut verts, mut thin) = (Vec::new(), Vec::new());
    // Each exposed face: its corners, its key, and the way it faces.
    let mut quads: Vec<([u32; 4], u16, [f64; 3])> = Vec::new();
    for na in 0..3 {
        let (ua, va) = match na {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        for sign in [1i32, -1] {
            for x in 0..px {
                for y in 0..py {
                    for z in 0..pz {
                        if !solid(x, y, z) {
                            continue;
                        }
                        let mut nb = [x, y, z];
                        nb[na] += sign;
                        if solid(nb[0], nb[1], nb[2]) {
                            continue;
                        }
                        let mut q = [0u32; 4];
                        for (k, qk) in q.iter_mut().enumerate() {
                            let mut l = [x, y, z];
                            if sign > 0 {
                                l[na] += 1;
                            }
                            if k == 1 || k == 2 {
                                l[ua] += 1;
                            }
                            if k >= 2 {
                                l[va] += 1;
                            }
                            *qk = *ids.entry(l).or_insert_with(|| {
                                let (pos, th) = corner(l);
                                verts.push(pos);
                                thin.push(th);
                                (verts.len() - 1) as u32
                            });
                        }
                        let mut dirn = [0f64; 3];
                        dirn[na] = sign as f64;
                        let at = p.idx(x, y, z);
                        quads.push((q, p.colour[at] as u16 | (p.bone[at] as u16) << 8, dirn));
                    }
                }
            }
        }
    }
    taubin(&mut verts, &quads, &thin);
    let (mut tris, mut cols) = (Vec::with_capacity(2 * quads.len()), Vec::with_capacity(2 * quads.len()));
    for (q, mat, dirn) in quads {
        let [a, b, c, d] = q;
        let len = |i: u32, j: u32| dist2(verts[i as usize], verts[j as usize]);
        let pair = if len(a, c) <= len(b, d) { [[a, b, c], [a, c, d]] } else { [[a, b, d], [b, c, d]] };
        for t in pair {
            let n = tri_normal(&verts, t);
            let t = if dot(n, dirn) < 0.0 { [t[0], t[2], t[1]] } else { t };
            tris.push(t);
            cols.push(mat);
        }
    }
    (verts, tris, cols)
}

/// Alternate shrinking and inflating Laplacian steps over the quad edges,
/// which round off voxel steps without thinning the body. A vertex moves
/// half as far where the voxels are one thick or on a colour boundary, and
/// stays within `TAUBIN.3` of where it started.
fn taubin(v: &mut [[f64; 3]], quads: &[([u32; 4], u16, [f64; 3])], thin: &[bool]) {
    let (passes, lam, mu, clamp) = TAUBIN;
    let n = v.len();
    let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut colour: Vec<Option<u16>> = vec![None; n];
    let mut edge = vec![false; n];
    for (q, mat, _) in quads {
        let c = mat & 255;
        for k in 0..4 {
            let (a, b) = (q[k], q[(k + 1) % 4]);
            if !nb[a as usize].contains(&b) {
                nb[a as usize].push(b);
            }
            if !nb[b as usize].contains(&a) {
                nb[b as usize].push(a);
            }
            match colour[a as usize] {
                None => colour[a as usize] = Some(c),
                Some(o) if o != c => edge[a as usize] = true,
                _ => {}
            }
        }
    }
    let f: Vec<f64> = (0..n).map(|i| if thin[i] || edge[i] { 0.5 } else { 1.0 }).collect();
    let v0 = v.to_vec();
    let mut lap = vec![[0f64; 3]; n];
    for _ in 0..passes {
        for w in [lam, mu] {
            for (i, l) in lap.iter_mut().enumerate() {
                let mut m = [0f64; 3];
                for &j in &nb[i] {
                    m = add(m, v[j as usize]);
                }
                *l = if nb[i].is_empty() { [0.0; 3] } else { sub(scale(m, 1.0 / nb[i].len() as f64), v[i]) };
            }
            for i in 0..n {
                v[i] = add(v[i], scale(lap[i], w * f[i]));
            }
        }
        for i in 0..n {
            for k in 0..3 {
                v[i][k] = v[i][k].clamp(v0[i][k] - clamp, v0[i][k] + clamp);
            }
        }
    }
}

// ---------------------------------------------------------------- vectors

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn crs(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}
fn tri_normal(v: &[[f64; 3]], t: [u32; 3]) -> [f64; 3] {
    let (a, b, c) = (v[t[0] as usize], v[t[1] as usize], v[t[2] as usize]);
    crs(sub(b, a), sub(c, a))
}

// ---------------------------------------------------------------- quadrics

/// A symmetric 4x4 quadric, upper triangle: a b c d / e f g / h i / j.
#[derive(Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn plane(n: [f64; 3], d: f64, w: f64) -> Quadric {
        let [a, b, c] = n;
        Quadric([a * a * w, a * b * w, a * c * w, a * d * w, b * b * w, b * c * w, b * d * w, c * c * w, c * d * w, d * d * w])
    }
    fn add(&mut self, o: &Quadric) {
        for k in 0..10 {
            self.0[k] += o.0[k];
        }
    }
    fn sum(a: &Quadric, b: &Quadric) -> Quadric {
        let mut q = *a;
        q.add(b);
        q
    }
    fn cost(&self, p: [f64; 3]) -> f64 {
        let q = &self.0;
        let [x, y, z] = p;
        q[0] * x * x + 2.0 * q[1] * x * y + 2.0 * q[2] * x * z + 2.0 * q[3] * x + q[4] * y * y + 2.0 * q[5] * y * z + 2.0 * q[6] * y + q[7] * z * z + 2.0 * q[8] * z + q[9]
    }
    /// The system its minimum solves: the 3x3 part and the right-hand side.
    fn system(&self) -> ([[f64; 3]; 3], [f64; 3]) {
        let q = &self.0;
        ([[q[0], q[1], q[2]], [q[1], q[4], q[5]], [q[2], q[5], q[7]]], [-q[3], -q[6], -q[8]])
    }
}

fn det3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// m x = r by Cramer's rule, if m is well conditioned.
fn solve3(m: [[f64; 3]; 3], r: [f64; 3]) -> Option<[f64; 3]> {
    let det = det3(&m);
    if det.abs() < 1e-12 {
        return None;
    }
    let col = |c: usize| {
        let mut a = m;
        for (row, rv) in a.iter_mut().zip(r) {
            row[c] = rv;
        }
        det3(&a) / det
    };
    Some([col(0), col(1), col(2)])
}

/// The smallest angle of a triangle, radians.
fn min_angle(p: [[f64; 3]; 3]) -> f64 {
    let mut m = std::f64::consts::PI;
    for k in 0..3 {
        let (u, w) = (sub(p[(k + 1) % 3], p[k]), sub(p[(k + 2) % 3], p[k]));
        let (lu, lw) = (len(u), len(w));
        if lu < 1e-9 || lw < 1e-9 {
            return 0.0;
        }
        m = m.min((dot(u, w) / (lu * lw)).clamp(-1.0, 1.0).acos());
    }
    m
}

/// Where edge a-b collapses to and what that costs. The point minimises the
/// two ends' quadrics, held to a plane that keeps the volume enclosed by the
/// faces round the edge (so a convex limb doesn't thin) and pulled very
/// slightly toward the edge's middle (so on a flat or cylindrical stretch it
/// stays central); an end wins if it costs less. A collapse that would leave
/// a sliver triangle costs three times as much.
fn best(q: &[Quadric], v: &[[f64; 3]], vf: &[Vec<u32>], t: &[[u32; 3]], a: u32, b: u32) -> (f64, [f64; 3]) {
    let qs = Quadric::sum(&q[a as usize], &q[b as usize]);
    let (pa, pb) = (v[a as usize], v[b as usize]);
    let mid = scale(add(pa, pb), 0.5);
    let (mut g, mut d) = ([0f64; 3], 0f64);
    let mut rem: Vec<(u32, u32)> = Vec::new();
    let others = vf[b as usize].iter().filter(|f| !vf[a as usize].contains(f));
    for &f in vf[a as usize].iter().chain(others) {
        let tr = t[f as usize];
        let has_a = tr.contains(&a);
        let x = if has_a { a } else { b };
        let k = tr.iter().position(|&y| y == x).unwrap_or(0);
        let (i1, i2) = (tr[(k + 1) % 3], tr[(k + 2) % 3]);
        let c = crs(v[i1 as usize], v[i2 as usize]);
        d += dot(v[x as usize], c);
        if !(has_a && tr.contains(&b)) {
            g = add(g, c);
            rem.push((i1, i2));
        }
    }
    let (mut m, mut r) = qs.system();
    let gl = len(g);
    if gl > 1e-9 {
        let nv = scale(g, 1.0 / gl);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += VOL_W * nv[i] * nv[j];
            }
            r[i] += VOL_W * nv[i] * d / gl;
        }
    }
    let eps = 1e-3 * (m[0][0] + m[1][1] + m[2][2]).max(1e-9);
    for i in 0..3 {
        m[i][i] += eps;
        r[i] += eps * mid[i];
    }
    let mut cands = vec![pa, pb];
    if let Some(x) = solve3(m, r) {
        if len(sub(x, mid)) < 0.75 + len(sub(pa, pb)) / 2.0 {
            cands.push(x);
        }
    }
    let mut out = (f64::INFINITY, mid);
    for c in cands {
        let k = qs.cost(c);
        if k < out.0 {
            out = (k, c);
        }
    }
    out.0 = out.0.max(0.0);
    if rem.iter().any(|&(i1, i2)| min_angle([out.1, v[i1 as usize], v[i2 as usize]]) < SLIVER) {
        out.0 *= 3.0;
    }
    out
}

#[derive(PartialEq)]
struct Cand {
    cost: f64,
    a: u32,
    b: u32,
    sa: u32,
    sb: u32,
}
impl Eq for Cand {}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> Ordering {
        // Cheapest first out of a max-heap.
        o.cost.total_cmp(&self.cost).then(o.a.cmp(&self.a)).then(o.b.cmp(&self.b))
    }
}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Collapse edges until `target` triangles are left (or no collapse is
/// allowed). Returns the compacted vertices, triangles and materials.
#[allow(clippy::type_complexity)]
fn decimate(v0: &[[f64; 3]], t0: &[[u32; 3]], c0: &[u16], target: usize, max_turn: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>, Vec<u16>) {
    let mut v: Vec<[f64; 3]> = v0.to_vec();
    let mut t: Vec<[u32; 3]> = t0.to_vec();
    let nv = v.len();
    let mut alive = vec![true; t.len()];
    let mut vf: Vec<Vec<u32>> = vec![Vec::new(); nv];
    for (i, tr) in t.iter().enumerate() {
        for &k in tr {
            vf[k as usize].push(i as u32);
        }
    }
    let mut q = vec![Quadric::default(); nv];
    for tr in &t {
        let n = tri_normal(&v, *tr);
        let a = len(n);
        if a < 1e-12 {
            continue;
        }
        let n = scale(n, 1.0 / a);
        let k = Quadric::plane(n, -dot(n, v[tr[0] as usize]), (a / 2.0).sqrt());
        for &x in tr {
            q[x as usize].add(&k);
        }
    }
    // Edges and the faces on them; a boundary between colours or bones (or
    // an open edge) gets planes along it that hold it in place.
    let mut edges: std::collections::HashMap<(u32, u32), Vec<u32>> = std::collections::HashMap::new();
    for (i, tr) in t.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tr[k], tr[(k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(i as u32);
        }
    }
    for (&(a, b), fs) in &edges {
        if fs.len() == 2 && c0[fs[0] as usize] == c0[fs[1] as usize] {
            continue;
        }
        for &f in fs {
            let n = tri_normal(&v, t[f as usize]);
            let an = len(n);
            let ed = sub(v[b as usize], v[a as usize]);
            let l = len(ed);
            if an < 1e-12 || l < 1e-9 {
                continue;
            }
            let m = crs(ed, scale(n, 1.0 / an));
            let lm = len(m);
            if lm < 1e-12 {
                continue;
            }
            let m = scale(m, 1.0 / lm);
            let k = Quadric::plane(m, -dot(m, v[a as usize]), BOUNDARY_W * l * l);
            q[a as usize].add(&k);
            q[b as usize].add(&k);
        }
    }

    let mut stamp = vec![0u32; nv];
    let mut dead = vec![false; nv];
    let mut heap = BinaryHeap::new();
    for &(a, b) in edges.keys() {
        let (cost, _) = best(&q, &v, &vf, &t, a, b);
        heap.push(Cand { cost, a, b, sa: 0, sb: 0 });
    }
    let nbrs = |vf: &[Vec<u32>], t: &[[u32; 3]], x: u32, out: &mut Vec<u32>| {
        out.clear();
        for &f in &vf[x as usize] {
            for &y in &t[f as usize] {
                if y != x && !out.contains(&y) {
                    out.push(y);
                }
            }
        }
    };
    let (mut na, mut nb, mut opp, mut shared) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count = t.len();
    while count > target {
        let Some(c) = heap.pop() else { break };
        let (a, b) = (c.a, c.b);
        if dead[a as usize] || dead[b as usize] || stamp[a as usize] != c.sa || stamp[b as usize] != c.sb {
            continue;
        }
        shared.clear();
        shared.extend(vf[a as usize].iter().filter(|f| vf[b as usize].contains(f)).copied());
        if shared.is_empty() {
            continue;
        }
        // Link condition: the only vertices next to both are the ones across
        // the shared faces, or the collapse would pinch the surface.
        opp.clear();
        for &f in &shared {
            for &y in &t[f as usize] {
                if y != a && y != b && !opp.contains(&y) {
                    opp.push(y);
                }
            }
        }
        nbrs(&vf, &t, a, &mut na);
        nbrs(&vf, &t, b, &mut nb);
        let common = na.iter().filter(|x| nb.contains(x)).count();
        if common != opp.len() || !opp.iter().all(|x| na.contains(x) && nb.contains(x)) {
            continue;
        }
        let (_, pnew) = best(&q, &v, &vf, &t, a, b);
        // No face may flip or turn far.
        let mut ok = true;
        'check: for x in [a, b] {
            for &f in &vf[x as usize] {
                if shared.contains(&f) {
                    continue;
                }
                let tr = t[f as usize];
                let n0 = tri_normal(&v, tr);
                let old = v[x as usize];
                v[x as usize] = pnew;
                let n1 = tri_normal(&v, tr);
                v[x as usize] = old;
                let (l0, l1) = (len(n0), len(n1));
                if l1 < 1e-12 || (l0 > 1e-12 && dot(n0, n1) / (l0 * l1) < max_turn) {
                    ok = false;
                    break 'check;
                }
            }
        }
        if !ok {
            continue;
        }
        // Collapse b into a.
        v[a as usize] = pnew;
        let qb = q[b as usize];
        q[a as usize].add(&qb);
        for &f in &shared {
            alive[f as usize] = false;
            for &y in &t[f as usize] {
                vf[y as usize].retain(|&g| g != f);
            }
            count -= 1;
        }
        let moved = std::mem::take(&mut vf[b as usize]);
        for f in moved {
            for y in t[f as usize].iter_mut() {
                if *y == b {
                    *y = a;
                }
            }
            if !vf[a as usize].contains(&f) {
                vf[a as usize].push(f);
            }
        }
        dead[b as usize] = true;
        stamp[a as usize] += 1;
        nbrs(&vf, &t, a, &mut na);
        for &y in &na {
            let (lo, hi) = (a.min(y), a.max(y));
            let (cost, _) = best(&q, &v, &vf, &t, lo, hi);
            heap.push(Cand { cost, a: lo, b: hi, sa: stamp[lo as usize], sb: stamp[hi as usize] });
        }
    }
    // Compact.
    let mut remap = vec![u32::MAX; nv];
    let (mut vo, mut to, mut co) = (Vec::new(), Vec::new(), Vec::new());
    for (i, tr) in t.iter().enumerate() {
        if !alive[i] {
            continue;
        }
        let mut nt = [0u32; 3];
        for k in 0..3 {
            let x = tr[k] as usize;
            if remap[x] == u32::MAX {
                remap[x] = vo.len() as u32;
                vo.push(v[x]);
            }
            nt[k] = remap[x];
        }
        to.push(nt);
        co.push(c0[i]);
    }
    (vo, to, co)
}

// ---------------------------------------------------------------- packing

/// The angle of triangle `tr` at its vertex `x`, which weights that face in
/// the vertex's normal.
fn corner_angle(v: &[[f64; 3]], tr: [u32; 3], x: u32) -> f64 {
    let k = tr.iter().position(|&y| y == x).unwrap_or(0);
    let p = v[tr[k] as usize];
    let (u, w) = (sub(v[tr[(k + 1) % 3] as usize], p), sub(v[tr[(k + 2) % 3] as usize], p));
    (dot(u, w) / (len(u) * len(w)).max(1e-12)).clamp(-1.0, 1.0).acos()
}

/// IEEE half from f32 (round to nearest; the values here are small grid
/// coordinates, so no infinities or subnormals worth keeping).
fn f16(x: f32) -> u32 {
    let b = x.to_bits();
    let sign = (b >> 16) & 0x8000;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        return sign;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    let mut h = sign | (exp as u32) << 10 | man >> 13;
    if man & 0x1000 != 0 {
        h += 1;
    }
    h & 0xffff
}

fn snorm16(x: f64) -> u32 {
    ((x.clamp(-1.0, 1.0) * 32767.0).round() as i32 as u32) & 0xffff
}

/// Octahedral encoding of a unit vector, as WGSL's decode in `oct_normal`.
fn oct(n: [f64; 3]) -> [f64; 2] {
    let s = n[0].abs() + n[1].abs() + n[2].abs();
    let (x, y, z) = (n[0] / s, n[1] / s, n[2] / s);
    if z >= 0.0 {
        [x, y]
    } else {
        [(1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum()]
    }
}

/// Triangles to each part's packed vertices: smoothed normals and occlusion
/// over the whole body, positions from the part's window, and the seam
/// weights where it meets its parent.
fn pack(v: &[[f64; 3]], t: &[[u32; 3]], k: &[u16], pad: &Pad, body: &Body) -> Vec<Vec<Vert>> {
    // Ambient occlusion: how much of the 3x3x3 block a voxel out along the
    // normal is solid, as the square faces' corner AO reads it.
    let open = |p: [f64; 3], n: [f64; 3]| -> u32 {
        let q = add(add(p, n), [1.0, 1.0, 1.0]);
        let (cx, cy, cz) = (q[0].floor() as i32, q[1].floor() as i32, q[2].floor() as i32);
        let mut solid = 0;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    solid += pad.solid(cx + dx, cy + dy, cz + dz) as i32;
                }
            }
        }
        // A flat open face has about 9 of 27 solid; twice that is a crease.
        let a = (1.0 - (solid as f64 - 6.0).max(0.0) / 12.0).clamp(0.0, 1.0);
        (a * 255.0).round() as u32
    };
    let funit: Vec<[f64; 3]> = t.iter().map(|tr| tri_normal(v, *tr)).map(|n| scale(n, 1.0 / len(n).max(1e-12))).collect();
    let mut vf: Vec<Vec<u32>> = vec![Vec::new(); v.len()];
    for (i, tr) in t.iter().enumerate() {
        for &x in tr {
            vf[x as usize].push(i as u32);
        }
    }
    let parts = body.parent.len().max(1);
    let bone_of = |f: usize| ((k[f] >> 8) as usize).min(parts - 1);
    // The bones whose faces touch each vertex.
    let touches = |x: usize, b: usize| vf[x].iter().any(|&f| bone_of(f as usize) == b);
    let base = crate::world::material::PALETTE_BASE as u32;
    let mut out: Vec<Vec<Vert>> = vec![Vec::new(); parts];
    // Seam weight of each vertex in each part's mesh, worked out per part.
    let mut weight = vec![0f64; v.len()];
    for (b, mesh) in out.iter_mut().enumerate() {
        weight.iter_mut().for_each(|w| *w = 0.0);
        if let Some(pb) = body.parent.get(b).copied().flatten() {
            // The ring this part shares with its parent follows the parent;
            // the vertices one face further in go a third of the way.
            let mut ring = Vec::new();
            for x in 0..v.len() {
                if touches(x, b) && touches(x, pb) {
                    weight[x] = 1.0;
                    ring.push(x);
                }
            }
            for &x in &ring {
                for &f in &vf[x] {
                    if bone_of(f as usize) != b {
                        continue;
                    }
                    for &y in &t[f as usize] {
                        if weight[y as usize] == 0.0 {
                            weight[y as usize] = SECOND_RING;
                        }
                    }
                }
            }
        }
        let o = body.origins.get(b).copied().unwrap_or([0; 3]);
        for (i, tr) in t.iter().enumerate() {
            if bone_of(i) != b {
                continue;
            }
            for &x in tr {
                let mut sm = [0f64; 3];
                for &f in &vf[x as usize] {
                    if dot(funit[f as usize], funit[i]) > CREASE {
                        sm = add(sm, scale(funit[f as usize], corner_angle(v, t[f as usize], x)));
                    }
                }
                let n = scale(sm, 1.0 / len(sm).max(1e-12));
                let on = oct(n);
                let p = v[x as usize];
                let w = (weight[x as usize] * 255.0).round() as u32;
                let local = [p[0] - o[0] as f64, p[1] - o[1] as f64, p[2] - o[2] as f64];
                mesh.push([
                    f16(local[0] as f32) | f16(local[1] as f32) << 16,
                    f16(local[2] as f32),
                    snorm16(on[0]) | snorm16(on[1]) << 16,
                    (base + (k[i] & 255) as u32 - 1) | open(p, n) << 8 | w << 16,
                ]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 32;

    fn ball(r: f32) -> Vec<u8> {
        let mut c = vec![0u8; N * N * N];
        for y in 0..N {
            for z in 0..N {
                for x in 0..N {
                    let d = ((x as f32 + 0.5 - 16.0).powi(2) + (y as f32 + 0.5 - 16.0).powi(2) + (z as f32 + 0.5 - 16.0).powi(2)).sqrt();
                    if d < r {
                        c[(y * N + z) * N + x] = if y < 16 { 1 } else { 2 };
                    }
                }
            }
        }
        c
    }

    fn whole(cells: &[u8]) -> Pad {
        Pad::new(&Body { cells, dims: [32; 3], owner: None, parent: &[None], loose: &[], origins: &[[0; 3]] }, None)
    }

    #[test]
    fn a_ball_decimates_to_its_budget_and_stays_closed() {
        let (v, t, c) = surface(&whole(&ball(10.0)));
        assert!(t.len() > 2000, "{} faces", t.len());
        let (v2, t2, _) = decimate(&v, &t, &c, 400, NEAR_TURN);
        assert!(t2.len() <= 400 && t2.len() > 300, "{} left", t2.len());
        // Closed: every edge has two faces.
        let mut e: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
        for tr in &t2 {
            for k in 0..3 {
                let (a, b) = (tr[k], tr[(k + 1) % 3]);
                *e.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(e.values().all(|&n| n == 2));
        // Still about ten voxels round.
        let r: f64 = v2.iter().map(|p| len(sub(*p, [16.0, 16.0, 16.0]))).sum::<f64>() / v2.len() as f64;
        assert!((9.0..11.0).contains(&r), "mean radius {r}");
    }

    #[test]
    fn faces_point_out() {
        let (v, t, _) = surface(&whole(&ball(6.0)));
        for tr in &t {
            let n = tri_normal(&v, *tr);
            let c = scale(add(add(v[tr[0] as usize], v[tr[1] as usize]), v[tr[2] as usize]), 1.0 / 3.0);
            assert!(dot(n, sub(c, [16.0, 16.0, 16.0])) > 0.0);
        }
    }

    /// A ball split in two bones at y 16: the upper part's seam ring sits on
    /// the boundary, weighted to its parent, and the parts share no faces.
    #[test]
    fn a_split_body_has_a_weighted_seam() {
        let cells = ball(10.0);
        let owner: Vec<u8> = (0..N * N * N).map(|i| if cells[i] == 0 { u8::MAX } else if i / (N * N) >= 16 { 1 } else { 0 }).collect();
        let body = Body { cells: &cells, dims: [32; 3], owner: Some(&owner), parent: &[None, Some(0)], loose: &[], origins: &[[0; 3], [0; 3]] };
        let parts = bake_body(&body, 600, 300);
        assert_eq!(parts.len(), 2);
        let (root, top) = (&parts[0].0, &parts[1].0);
        assert!(!root.is_empty() && !top.is_empty());
        let y = |v: &Vert| {
            let h = v[0] >> 16;
            let e = ((h >> 10) & 31) as i32 - 15;
            (1.0 + (h & 1023) as f32 / 1024.0) * 2f32.powi(e)
        };
        let full: Vec<f32> = top.iter().filter(|v| v[3] >> 16 & 255 == 255).map(y).collect();
        assert!(!full.is_empty(), "no seam ring");
        assert!(full.iter().all(|&h| (15.0..17.0).contains(&h)), "seam ring off the joint: {full:?}");
        assert!(root.iter().all(|v| v[3] >> 16 & 255 == 0), "the root follows no parent");
    }

    /// The same ball with the top loose: each half is closed on its own, so
    /// both have faces on the cut, and the top has no seam ring.
    #[test]
    fn a_loose_part_is_closed_on_its_own() {
        let cells = ball(10.0);
        let owner: Vec<u8> = (0..N * N * N).map(|i| if cells[i] == 0 { u8::MAX } else if i / (N * N) >= 16 { 1 } else { 0 }).collect();
        let body = Body { cells: &cells, dims: [32; 3], owner: Some(&owner), parent: &[None, Some(0)], loose: &[false, true], origins: &[[0; 3], [0; 3]] };
        let parts = bake_body(&body, 600, 300);
        let (root, top) = (&parts[0].0, &parts[1].0);
        assert!(!root.is_empty() && !top.is_empty());
        assert!(top.iter().all(|v| v[3] >> 16 & 255 == 0), "a loose part follows no parent");
        // Undecimated, the two closed halves have the cut's faces twice over
        // and the joined body none.
        let all = |loose: &[bool]| bake_body(&Body { loose, ..body }, 1 << 20, 1 << 20).iter().map(|m| m.0.len()).sum::<usize>();
        let (split, joined) = (all(&[false, true]), all(&[]));
        assert!(split > joined + 3 * 2 * 100, "the cut adds faces: {split} vs {joined}");
    }

    #[test]
    fn half_floats_round_trip() {
        for x in [0.0f32, 0.5, 1.0, 15.25, 31.9375, 33.0] {
            let h = f16(x);
            let e = ((h >> 10) & 31) as i32 - 15;
            let m = (h & 1023) as f32 / 1024.0;
            let back = if h == 0 { 0.0 } else { (1.0 + m) * 2f32.powi(e) };
            assert!((back - x).abs() < 0.02, "{x} -> {back}");
        }
    }
}
