//! Leaning stems. A thin stem (`tube` material: canes, spurs, bamboo, vines,
//! coral limbs) that runs along an axis is bevelled face by face into an
//! octagon (`tube_face` in mesh.rs). One that leans is a staircase of voxels,
//! and bevelling its faces leaves a staircase of octagons. Here each voxel of
//! a leaning stem is drawn instead as a short octagonal tube along the line
//! the stem's voxels around it lie on, so the voxels overlap into one smooth
//! slanted stem.
//!
//! Threads. Where the generator knows the stem's own line (a plant whose
//! bark is a tube: canes, spurs), it hands the mesher that line as a
//! `Thread` (`Generator::threads`, built per chunk from `tube_trees`, for
//! new columns and remeshes alike), and the tube is drawn along it exactly:
//! stems that touch, as at the root of a clump, stay apart. A thread draws
//! only where its voxels still stand, so a dug stem breaks. Tube bark is
//! rasterized as one corner-joined line of voxels (stamp.rs), which the
//! thread covers. Straight-up stems are left to the axis bevel.
//!
//! Sticks are threads too: a limb thinner than its plant's `thin_limbs`,
//! whatever the bark, is drawn as a line and handed over with `mat` = the
//! bark and `lone` set, so the trunk's voxels where the limb leaves it keep
//! their faces. A deciduous tree in leaf hands over none (`Generator::threads`).
//!
//! Route rails are threads too (`Generator::rail_threads`): a thin rail of
//! a kit (wire, rope, handrail) along its centreline, sampled every half
//! metre where it follows the ground. A thread with a `mat` covers voxels of
//! that material rather than tube materials, so a rail needs no flag.
//!
//! Stem voxels no thread covers (structures, other tube materials) get a
//! fitted line instead. The line is fitted to the stem's voxels within two voxels (those joined to
//! this one, 26-connected), so two chunks that share a voxel through their
//! margins fit it the same way and the tube carries on across the border.
//! Where the voxels around are not one thin line (a fork, two stems
//! touching, a thick limb) or the line runs within about 6 degrees of an
//! axis, the voxel is left to the ordinary faces. The voxels themselves are
//! untouched: collision and digging still see squares. The shadow pass draws
//! tube faces where they are (`quad::TUBE_BIT`), not at their anchors.
//!
//! The tube's faces are rounded quads (corners moved off a plain voxel face),
//! so each is kept within a voxel or so of the face it hangs from: a fitted
//! stem's segments are cut a third of a voxel long, and a thread's faces two
//! voxels long, then cut again by `hang` wherever the face they hang from
//! cannot reach their corners. Thin stems take six sides, and joints that
//! barely turn are not capped: a cane field is thousands of threads.

use super::{pack_round, quad, Quad, DIRS, OFF};
use crate::world::gen::{PAD, PAD3, PADY};
use crate::world::material::tubes;
use glam::Vec3;

/// Tube radius in voxels: about as thick as the one-voxel stem it replaces.
const R: f32 = 0.46;
/// How far round a voxel the line is fitted, in voxels.
const WIN: isize = 2;
/// A line within this cosine of an axis (about 6 degrees) is left to the
/// axis-aligned bevel.
const STRAIGHT: f32 = 0.995;
/// Longest piece of tube one quad spans, in voxels, for a fitted stem.
const PIECE: f32 = 0.35;
/// Longest piece of a thread's tube, voxels. `hang` cuts a face its anchor
/// cannot reach, so this is only a first cut: most stems need no more.
const THREAD_PIECE: f32 = 2.0;
/// How far past a rounded quad's reach a corner may be and the quad still
/// hang whole: a sixteenth of a voxel, the packing's own step. A small face
/// whose middle lies within a sixteenth of a voxel boundary cannot reach both
/// sides of its anchor (offsets run -16..15 sixteenths), and no cut makes it
/// fit; refining for it only multiplied the quads (a 30-voxel rod at 48 sides
/// by 150 pieces). Clamping it moves one corner by at most that sixteenth.
const SLACK: f32 = 1.07;
/// A joint whose two pieces turn by less than this (cosine, about 15
/// degrees) is closed by their overlap and needs no caps.
const JOIN_STRAIGHT: f32 = 0.966;

/// A thread: one straight piece of a thin stem whose line is known (a plant's
/// limb, from `Generator::threads`), in the padded chunk's voxel
/// coordinates. `hull` is the radius its voxels were drawn to (stamp.rs),
/// `r` the stem's own radius, both in voxels. `mat` is the material its
/// voxels are made of, or 0 for any tube material (a plant's stems).
#[derive(Clone, Copy, Debug)]
pub struct Thread {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
    pub hull: f32,
    pub mat: u8,
    /// Cover only voxels standing alone in a line (at most three solid
    /// face neighbours): a tree's sticks are its bark, like its trunk, and
    /// the trunk's own voxels where a limb leaves it must keep their faces.
    pub lone: bool,
    /// Cover only voxels marked in the chunk's ownership mask (`leaning`'s
    /// `own`): a structure's segment, whose own voxels the shape records
    /// (`Shape::thread_vox`), so it never hides a post or a shell of the
    /// same material that it touches.
    pub owned: bool,
}

/// Thicker stems than this (voxels) keep their voxel faces. Big faces are
/// cut into pieces a rounded quad can reach (`hang`), but past a few voxels
/// a staircase reads as masonry rather than as a badly drawn round.
pub const THREAD_MAX_R: f32 = 3.0;

/// Leaning stem voxels of a padded chunk, and the tube quads that draw them.
pub(super) struct Lean {
    /// True for every padded voxel drawn as a tube (the chunk's own and its
    /// margin's), so faces beside it are not hidden by it.
    pub flags: Vec<bool>,
    pub quads: Vec<Quad>,
}

fn coord(i: usize) -> [isize; 3] {
    [(i % PAD) as isize, (i / (PAD * PAD)) as isize, ((i / PAD) % PAD) as isize]
}

fn index(c: [isize; 3]) -> Option<usize> {
    let inside = c[0] >= 0 && c[1] >= 0 && c[2] >= 0 && (c[0] as usize) < PAD && (c[1] as usize) < PADY && (c[2] as usize) < PAD;
    inside.then(|| c[0] as usize + c[1] as usize * PAD * PAD + c[2] as usize * PAD)
}

fn centre(c: [isize; 3]) -> Vec3 {
    Vec3::new(c[0] as f32 + 0.5, c[1] as f32 + 0.5, c[2] as f32 + 0.5)
}

/// The line a stem voxel lies on.
struct Fit {
    /// The voxel's centre moved onto the line.
    at: Vec3,
    t: Vec3,
    /// Whether the stem carries on past this voxel each way.
    ahead: bool,
    behind: bool,
    /// Spacing of the stem's voxels along the line.
    step: f32,
    /// The tube's radius, voxels.
    radius: f32,
}

fn fit(v: &[u8; PAD3], c: [isize; 3]) -> Option<Fit> {
    const W: isize = 2 * WIN + 1;
    let slot = |p: [isize; 3]| ((p[0] - c[0] + WIN) + (p[1] - c[1] + WIN) * W + (p[2] - c[2] + WIN) * W * W) as usize;
    let mut seen = [false; (W * W * W) as usize];
    let mut pts = vec![c];
    seen[slot(c)] = true;
    let mut k = 0;
    while k < pts.len() {
        let p = pts[k];
        k += 1;
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let q = [p[0] + dx, p[1] + dy, p[2] + dz];
                    if (q[0] - c[0]).abs() > WIN || (q[1] - c[1]).abs() > WIN || (q[2] - c[2]).abs() > WIN || seen[slot(q)] {
                        continue;
                    }
                    seen[slot(q)] = true;
                    if index(q).is_some_and(|i| tubes(v[i])) {
                        pts.push(q);
                    }
                }
            }
        }
    }
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len() as f32;
    let mean = pts.iter().map(|&p| centre(p)).sum::<Vec3>() / n;
    let mut cov = [[0f32; 3]; 3];
    for &p in &pts {
        let d = centre(p) - mean;
        let d = [d.x, d.y, d.z];
        for (a, row) in cov.iter_mut().enumerate() {
            for (b, e) in row.iter_mut().enumerate() {
                *e += d[a] * d[b];
            }
        }
    }
    let mul = |x: Vec3| {
        let x = [x.x, x.y, x.z];
        Vec3::new(
            cov[0][0] * x[0] + cov[0][1] * x[1] + cov[0][2] * x[2],
            cov[1][0] * x[0] + cov[1][1] * x[1] + cov[1][2] * x[2],
            cov[2][0] * x[0] + cov[2][1] * x[1] + cov[2][2] * x[2],
        )
    };
    // Principal axis by power iteration, from the widest axis.
    let wide = (0..3).max_by(|&a, &b| cov[a][a].total_cmp(&cov[b][b])).unwrap_or(1);
    let mut t = Vec3::ZERO;
    t[wide] = 1.0;
    t = (t + Vec3::splat(0.05)).normalize();
    for _ in 0..24 {
        let m = mul(t);
        if m.length_squared() < 1e-9 {
            return None;
        }
        t = m.normalize();
    }
    // One thin line: every voxel near it, and not many more voxels than the
    // line is long.
    let (mut perp, mut lo, mut hi) = (0.0f32, f32::MAX, f32::MIN);
    for &p in &pts {
        let d = centre(p) - mean;
        let a = d.dot(t);
        perp += (d - t * a).length_squared();
        lo = lo.min(a);
        hi = hi.max(a);
    }
    // Mean squared distance from the axis of a solid round stem of radius
    // rho is rho^2 / 2, so this is the stem's radius; up to about two voxels
    // across counts as a stem.
    let rho = (2.0 * perp / n).sqrt();
    if rho > 1.1 {
        return None;
    }
    let area = std::f32::consts::PI * (rho + 0.5) * (rho + 0.5) + 1.0;
    if n > (hi - lo + 1.0) * area {
        return None;
    }
    let lean = t.abs().max_element();
    if lean > STRAIGHT {
        return None;
    }
    let x = centre(c);
    let along = |p: [isize; 3]| (centre(p) - x).dot(t);
    Some(Fit {
        at: mean + t * (x - mean).dot(t),
        t,
        ahead: pts.iter().any(|&p| along(p) > 0.5),
        behind: pts.iter().any(|&p| along(p) < -0.5),
        step: (1.0 / lean).min(1.8),
        radius: (rho * 0.8).clamp(R, 0.7),
    })
}

/// The leaning stems of padded chunk `v`, or None when it has none.
/// `threads` are the stems whose line is known: each is drawn along that
/// line, and covers the stem voxels it passes through. Stem voxels no thread
/// covers have a line fitted to them instead (`fit`), which works for a lone
/// stem but not where two touch, as at the root of a clump.
///
/// `model`: an entity model (palette colours, mesh::mesh_model_t): a thread
/// covers voxels of exactly its colour, and no lines are fitted, since palette
/// ids are not world materials and `tubes` means nothing for them.
///
/// `own`: one flag per padded voxel, the voxels `owned` threads may cover
/// (empty when no thread is `owned`).
pub(super) fn leaning(v: &[u8; PAD3], threads: &[Thread], own: &[bool], model: bool) -> Option<Lean> {
    if threads.is_empty() && (model || !v.iter().any(|&m| tubes(m))) {
        return None;
    }
    let mut flags = vec![false; PAD3];
    let mut quads = Vec::new();
    let ends = Ends::new(threads);
    for (k, th) in threads.iter().enumerate() {
        thread(v, th, threads, &ends, k, model, own, &mut flags, &mut quads);
    }
    for (i, &m) in v.iter().enumerate() {
        if model || !tubes(m) || flags[i] {
            continue;
        }
        let c = coord(i);
        let Some(f) = fit(v, c) else { continue };
        flags[i] = true;
        // Only the chunk's own voxels are drawn; the margin's are only marked.
        let own = (0..3).all(|a| c[a] >= OFF[a] && c[a] < OFF[a] + 32);
        if own {
            tube(&f, m, &mut quads);
        }
    }
    flags.iter().any(|&f| f).then_some(Lean { flags, quads })
}

fn tube(f: &Fit, m: u8, out: &mut Vec<Quad>) {
    let t = f.t;
    let hp = if f.ahead { 0.55 * f.step } else { 0.3 };
    let hm = if f.behind { 0.55 * f.step } else { 0.3 };
    let a = f.at - t * hm;
    let len = hp + hm;
    let pieces = (len / PIECE).ceil().max(1.0) as usize;
    let rf = if t.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = t.cross(rf).normalize();
    let e2 = t.cross(e1);
    let ring: [Vec3; 8] = std::array::from_fn(|k| {
        let th = (k as f32 + 0.5) * std::f32::consts::FRAC_PI_4;
        (e1 * th.cos() + e2 * th.sin()) * f.radius
    });
    // Chunk voxel coordinates from padded ones.
    let off = Vec3::new(OFF[0] as f32, OFF[1] as f32, OFF[2] as f32);
    for j in 0..pieces {
        let c0 = a + t * (len * j as f32 / pieces as f32) - off;
        let c1 = a + t * (len * (j + 1) as f32 / pieces as f32) - off;
        for k in 0..8 {
            let (r0, r1) = (ring[k], ring[(k + 1) % 8]);
            face([c0 + r0, c0 + r1, c1 + r1, c1 + r0], r0 + r1, m, out);
        }
    }
    // Caps on the ends the stem stops at.
    for (end, n, open) in [(a + t * len, t, !f.ahead), (a, -t, !f.behind)] {
        if !open {
            continue;
        }
        let o: [Vec3; 8] = std::array::from_fn(|k| end - off + ring[k]);
        face([o[0], o[1], o[2], o[3]], n, m, out);
        face([o[3], o[4], o[7], o[0]], n, m, out);
        face([o[4], o[5], o[6], o[7]], n, m, out);
    }
}

/// One quad with corners `q` (chunk voxel coordinates), facing `n`: wound to
/// face that way and hung from the plain voxel face nearest to it.
fn face(mut q: [Vec3; 4], n: Vec3, m: u8, out: &mut Vec<Quad>) {
    if (q[1] - q[0]).cross(q[2] - q[0]).dot(n) < 0.0 {
        q.swap(1, 3);
    }
    hang(q, n, m, 0, out);
}

/// `face` once wound. A quad whose corners reach further from the face it
/// hangs from than a rounded quad can move them is cut in two across its
/// longer sides and each half hung on its own: clamped, its edges would pull
/// away from its neighbours' and show daylight through the tube.
fn hang(q: [Vec3; 4], n: Vec3, m: u8, depth: u32, out: &mut Vec<Quad>) {
    let (d, p, r, cost) = anchor(q, n, true);
    let (na, sign, ua, va) = DIRS[d];
    let plain = |k: usize| plain(p, na, sign, ua, va, k);
    if cost > SLACK && depth < 5 {
        let mid = |a: Vec3, b: Vec3| (a + b) * 0.5;
        if (q[1] - q[0]).length_squared() + (q[3] - q[2]).length_squared() > (q[2] - q[1]).length_squared() + (q[0] - q[3]).length_squared() {
            let (e, f) = (mid(q[0], q[1]), mid(q[3], q[2]));
            hang([q[0], e, f, q[3]], n, m, depth + 1, out);
            hang([e, q[1], q[2], f], n, m, depth + 1, out);
        } else {
            let (e, f) = (mid(q[1], q[2]), mid(q[0], q[3]));
            hang([q[0], q[1], e, f], n, m, depth + 1, out);
            hang([f, e, q[2], q[3]], n, m, depth + 1, out);
        }
        return;
    }
    let offs: [[i32; 3]; 4] = std::array::from_fn(|k| {
        let o = (q[(k + r) % 4] - plain(k)) * 16.0;
        [(o.x.round() as i32).clamp(-16, 15), (o.y.round() as i32).clamp(-32, 31), (o.z.round() as i32).clamp(-16, 15)]
    });
    let (w2, w3) = pack_round(&offs);
    out.push([quad::word0(p.map(|x| x as u32), 1, 1, d, true) | 1 << quad::TUBE_BIT, m as u32 | 0xff << 8, w2, w3]);
}

/// Corner `k` of the plain voxel face at `p` facing (na, sign).
fn plain(p: [i32; 3], na: usize, sign: i32, ua: usize, va: usize, k: usize) -> Vec3 {
    let mut c = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
    if sign > 0 {
        c[na] += 1.0;
    }
    c[ua] += (k == 1 || k == 2) as i32 as f32;
    c[va] += (k >= 2) as i32 as f32;
    c
}

/// Whether a wound quad hangs whole from one plain face (`hang` would not
/// cut it), wherever it lies: the anchor is not clamped to a chunk.
fn fits(mut q: [Vec3; 4], n: Vec3) -> bool {
    if (q[1] - q[0]).cross(q[2] - q[0]).dot(n) < 0.0 {
        q.swap(1, 3);
    }
    anchor(q, n, false).3 <= SLACK
}

/// The plain face a quad hangs from: its direction, its voxel, the turn of
/// the corner order that needs the least move, and that move as a share of
/// what a rounded quad can reach (over 1 does not fit). `clamp` keeps the
/// voxel inside the chunk, as packing needs.
fn anchor(q: [Vec3; 4], n: Vec3, clamp: bool) -> (usize, [i32; 3], usize, f32) {
    let d = (0..6)
        .max_by(|&a, &b| {
            let s = |g: usize| n[DIRS[g].0] * DIRS[g].1 as f32;
            s(a).total_cmp(&s(b))
        })
        .unwrap_or(2);
    let (na, sign, ua, va) = DIRS[d];
    let mean = (q[0] + q[1] + q[2] + q[3]) / 4.0;
    // The plain face centred on the quad, as near as the grid allows.
    let mut p = [0i32; 3];
    p[na] = mean[na].round() as i32 - (sign > 0) as i32;
    p[ua] = (mean[ua] - 0.5).round() as i32;
    p[va] = (mean[va] - 0.5).round() as i32;
    let p = if clamp { p.map(|x| x.clamp(0, 31)) } else { p };
    // The turn of the corner order (winding kept) that needs the least move.
    let reach = |o: Vec3| (o.x.abs() / 0.94).max(o.y.abs() / 1.94).max(o.z.abs() / 0.94);
    let cost = |r: usize| (0..4).map(|k| reach(q[(k + r) % 4] - plain(p, na, sign, ua, va, k))).fold(0.0f32, f32::max);
    let r = (0..4).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap_or(0);
    (d, p, r, cost(r))
}

/// How near two ends have to be to join, voxels.
const JOIN: f32 = 0.05;

/// The chunk's thread ends by position, in cells twice `JOIN` across, so
/// finding the pieces that meet at a point looks in the cells round it
/// rather than at every thread: a cane field hands a chunk hundreds.
struct Ends(crate::util::FxHashMap<[i32; 3], Vec<usize>>);

impl Ends {
    const CELL: f32 = 2.0 * JOIN;

    fn cell(p: Vec3) -> [i32; 3] {
        (p / Self::CELL).floor().as_ivec3().to_array()
    }

    fn new(all: &[Thread]) -> Self {
        let mut m = crate::util::FxHashMap::<[i32; 3], Vec<usize>>::default();
        for (j, th) in all.iter().enumerate() {
            m.entry(Self::cell(th.a)).or_default().push(j);
            if Self::cell(th.b) != Self::cell(th.a) {
                m.entry(Self::cell(th.b)).or_default().push(j);
            }
        }
        Ends(m)
    }

    /// The threads with an end in the cells round `p`: every thread with an
    /// end within `JOIN` of it, and some further.
    fn around(&self, p: Vec3) -> impl Iterator<Item = usize> + '_ {
        let c = Self::cell(p);
        (0..27).flat_map(move |n| {
            let k = [c[0] + n % 3 - 1, c[1] + n / 3 % 3 - 1, c[2] + n / 9 - 1];
            self.0.get(&k).into_iter().flatten().copied()
        })
    }
}

/// Draw thread `k` of `all` and mark the stem voxels it covers.
#[allow(clippy::too_many_arguments)]
fn thread(v: &[u8; PAD3], th: &Thread, all: &[Thread], ends: &Ends, k: usize, model: bool, own: &[bool], flags: &mut [bool], out: &mut Vec<Quad>) {
    let full = th.b - th.a;
    let span = full.length();
    if span < 1e-3 {
        return;
    }
    let t = full / span;
    // A thread carries on where another starts at its end (or ends at its
    // start): no cap there, and the two overlap a little so the bend closes.
    // The direction of the piece joined at `p`, if any.
    let near = |p: Vec3| ends.around(p).any(|j| j != k && (all[j].a.distance(p) < JOIN || all[j].b.distance(p) < JOIN));
    let (join_a, join_b) = (near(th.a), near(th.b));
    // A joint that barely turns is closed by the overlap: no caps there. It
    // has to be one rod carrying on into the next: a piece ending where this
    // one starts (or starting where it ends), running on the same way, of
    // about the same radius and material. Two rods that merely start at one
    // point -- a gun's muzzle brake round its barrel, a scope's bell round
    // its tube, a fork -- each keep their caps, or the fat one is left open.
    let carries = |o: &Thread, from: Vec3, to: Vec3| {
        let d = (o.b - o.a).normalize_or_zero();
        from.distance(to) < JOIN && d.dot(t) > JOIN_STRAIGHT && o.mat == th.mat && (o.r - th.r).abs() <= 0.3 * o.r.max(th.r)
    };
    let smooth_a = ends.around(th.a).any(|j| j != k && carries(&all[j], all[j].b, th.a));
    let smooth_b = ends.around(th.b).any(|j| j != k && carries(&all[j], all[j].a, th.b));
    // A thread knows its own radius, so it can be thinner than the voxel
    // line under it (the fitted stems cannot: `R`).
    let radius = th.r.max(0.15);
    let base = crate::world::matlib::base;
    let ours = |m: u8| {
        if model {
            m == th.mat
        } else if th.mat != 0 {
            m != 0 && base(m) == base(th.mat)
        } else {
            tubes(m)
        }
    };
    let grow = (radius * 0.6).min(0.35);
    let a = th.a - t * if join_a { grow } else { 0.0 };
    let len = span + if join_a { grow } else { 0.0 } + if join_b { grow } else { 0.0 };
    // The stem voxels it passes through: those within its hull.
    let hull = th.hull + 0.1;
    let (lo, hi) = (th.a.min(th.b) - Vec3::splat(hull), th.a.max(th.b) + Vec3::splat(hull));
    for y in (lo.y.floor() as isize).max(0)..=(hi.y.floor() as isize).min(PADY as isize - 1) {
        for z in (lo.z.floor() as isize).max(0)..=(hi.z.floor() as isize).min(PAD as isize - 1) {
            for x in (lo.x.floor() as isize).max(0)..=(hi.x.floor() as isize).min(PAD as isize - 1) {
                let c = [x, y, z];
                let Some(i) = index(c) else { continue };
                if !ours(v[i]) || (th.owned && !own.get(i).copied().unwrap_or(false)) {
                    continue;
                }
                if th.lone {
                    let solid = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
                        .iter()
                        .filter(|o| index([x + o[0], y + o[1], z + o[2]]).is_some_and(|j| v[j] != 0))
                        .count();
                    if solid > 3 {
                        continue;
                    }
                }
                let p = centre(c);
                let s = (p - th.a).dot(t).clamp(0.0, span);
                if (th.a + t * s).distance(p) < hull {
                    flags[i] = true;
                }
            }
        }
    }
    let rf = if t.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = t.cross(rf).normalize();
    let e2 = t.cross(e1);
    let ring_of = |sides: usize| -> Vec<Vec3> {
        (0..sides)
            .map(|k| {
                let th = (k as f32 + 0.5) * std::f32::consts::TAU / sides as f32;
                (e1 * th.cos() + e2 * th.sin()) * radius
            })
            .collect()
    };
    // Six sides for a twig, eight for a stem, more for a spar, so a mast
    // stays round.
    let mut sides = if radius > 1.2 {
        16
    } else if radius > 0.7 {
        12
    } else if radius > 0.35 {
        8
    } else {
        6
    };
    // Long pieces, cut finer for the whole tube until every face hangs
    // whole from one plain face. Cutting single faces instead (`hang`)
    // leaves a corner in the middle of a neighbour's edge, and a crack.
    // Judged on the whole thread and unclamped, so every chunk cuts it alike.
    let mut pieces = (len / THREAD_PIECE).ceil().max(1.0) as usize;
    let most = (len / 0.5).ceil().max(1.0) as usize;
    let mut ring = ring_of(sides);
    for _ in 0..8 {
        let mut worst: Option<(f32, f32)> = None;
        'check: for j in 0..pieces {
            let c0 = a + t * (len * j as f32 / pieces as f32);
            let c1 = a + t * (len * (j + 1) as f32 / pieces as f32);
            for k in 0..sides {
                let (r0, r1) = (ring[k], ring[(k + 1) % sides]);
                if !fits([c0 + r0, c0 + r1, c1 + r1, c1 + r0], r0 + r1) {
                    worst = Some((r0.distance(r1), len / pieces as f32));
                    break 'check;
                }
            }
        }
        match worst {
            None => break,
            Some((around, along)) if around > along && sides < 32 => {
                sides *= 2;
                ring = ring_of(sides);
            }
            Some(_) if pieces < most => pieces = (pieces * 2).min(most),
            Some(_) => break,
        }
    }
    let off = Vec3::new(OFF[0] as f32, OFF[1] as f32, OFF[2] as f32);
    // The stem's material where it still stands: a piece with none of its
    // voxels left near it (dug away) is not drawn. Near, not under: a thin
    // rail drawn on sloping ground is a dashed line of voxels.
    let stem = |p: Vec3| {
        let c = [p.x.floor() as isize, p.y.floor() as isize, p.z.floor() as isize];
        let reach = th.hull.ceil().max(1.0) as isize;
        let mut found = None;
        for dy in -reach..=reach {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    if let Some(i) = index([c[0] + dx, c[1] + dy, c[2] + dz]) {
                        if ours(v[i]) && flags[i] {
                            found = Some(v[i]);
                        }
                    }
                }
            }
        }
        found
    };
    // Each face is drawn by the chunk its anchor voxel is in, not the one
    // its piece's middle is in: on a thick tube the far side of the ring
    // lies in the next chunk, and pinned inside this one it tears.
    let inside = |p: Vec3| p.x >= 0.0 && p.y >= 0.0 && p.z >= 0.0 && p.x < PAD as f32 && p.y < PADY as f32 && p.z < PAD as f32;
    // A face is this chunk's only if its middle is within half a voxel of the
    // chunk's own 32 (`owned`), and a face's middle is within the radius of
    // its piece's line. So a piece whose line, widened by the radius (and a
    // voxel to spare), misses the chunk has no face here and is skipped
    // before any is worked out: a long thread crosses many chunks.
    let away = |lo: Vec3, hi: Vec3| (hi - off).cmplt(Vec3::splat(-1.5 - radius)).any() || (lo - off).cmpgt(Vec3::splat(33.5 + radius)).any();
    for j in 0..pieces {
        let c0 = a + t * (len * j as f32 / pieces as f32);
        let c1 = a + t * (len * (j + 1) as f32 / pieces as f32);
        if away(c0.min(c1), c0.max(c1)) {
            continue;
        }
        let mid = (c0 + c1) * 0.5;
        let here = inside(mid).then(|| stem(mid));
        if here == Some(None) {
            continue;
        }
        for k in 0..sides {
            let (r0, r1) = (ring[k], ring[(k + 1) % sides]);
            let q = [c0 - off + r0, c0 - off + r1, c1 - off + r1, c1 - off + r0];
            if !owned(q, r0 + r1) {
                continue;
            }
            let m = match here {
                Some(m) => m,
                None => stem((c0 + c1) * 0.5 + (r0 + r1) * 0.5 * (1.0 - 0.5 / radius.max(0.5))),
            };
            let Some(m) = m else { continue };
            face(q, r0 + r1, m, out);
        }
    }
    // Every end is capped but a joint that barely turns: at a sharper bend
    // the outside of the corner otherwise looks into the hollow tube.
    for (end, n, skip) in [(a + t * len, t, smooth_b), (a, -t, smooth_a)] {
        if skip || away(end, end) {
            continue;
        }
        let m = if inside(end - n * 0.2) { stem(end - n * 0.2) } else { None };
        // The polygon as a fan of quads from its first corner.
        let o: Vec<Vec3> = ring.iter().map(|&q| end - off + q).collect();
        for k in (1..sides - 2).step_by(2) {
            let q = [o[0], o[k], o[k + 1], o[k + 2]];
            if !owned(q, n) {
                continue;
            }
            let m = m.or_else(|| stem((q[0] + q[1] + q[2] + q[3]) / 4.0 + off - n * 0.3));
            let Some(m) = m else { continue };
            face(q, n, m, out);
        }
    }
}

/// Whether this chunk draws face `q` (chunk voxel coordinates, facing `n`):
/// its anchor voxel, unclamped, is one of the chunk's own.
fn owned(mut q: [Vec3; 4], n: Vec3) -> bool {
    if (q[1] - q[0]).cross(q[2] - q[0]).dot(n) < 0.0 {
        q.swap(1, 3);
    }
    anchor(q, n, false).1.iter().all(|&x| (0..32).contains(&x))
}
