//! Greedy mesher. Turns a padded voxel array (see gen.rs for the margins) into
//! packed quads, grouped by face direction so the renderer can skip directions
//! that face away from the camera.
//!
//! The packed quad layout is in mesh/quad.rs.
//!
//! Face directions: 0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z. For each, the (u, v) axes are
//! chosen so u x v points along the face normal and corners (0,0),(1,0),(1,1),(0,1)
//! wind counter-clockwise seen from outside. The shader uses the same table.
//!
//! Slopes. With `smooth` on, small steps in the ground become even slopes. Each
//! corner of a ground surface moves up or down, by at most a voxel, to the mean
//! height of the ground surfaces in the 4x4 block of columns around it, counting
//! only surfaces within four levels. A staircase rising one voxel every four
//! columns, every column, or two every column comes out as a straight incline.
//! Side faces move their edges to match: the top edge to the ground above, the
//! bottom edge to the ground below, and the middle edge of a two-voxel riser
//! between the two, so risers close up. The answer depends only on the corner
//! and the surface height, so every face that shares a corner moves it to the
//! same place. Cliffs, trees, planks and brick keep their square edges.
//!
//! Blends. Also with `smooth` on, a ground top face beside other ground carries
//! that material and, per corner, the share of it among the four columns at
//! that corner. The shader draws whichever material wins against a noisy
//! threshold, so a grass and sand border runs smooth and wavy instead of in
//! voxel steps. Two neighbouring faces see the same shares from opposite sides.
//!
//! Trees. Corners of tree faces (bark and leaves) whose eight surrounding
//! voxels are all tree or air are placed by surface nets: at the mean of the
//! points where the tree's surface crosses the edges between those eight voxel
//! centres. Canopies become rounded faceted blobs and trunks octagons. The
//! position depends only on the eight voxels, so the surface stays closed. A
//! corner with ground within a voxel stays square, so a trunk flares back to a
//! square foot that meets the sloped ground exactly.
//!
//! Leaf blocks. In full-detail chunks, foliage (`block_leaf`) is not meshed
//! voxel by voxel: `leaf_blocks` groups the voxels into 0.5 m cells, takes a
//! cell with at least `BLOCK_MIN` leaf voxels of its eight as leaves, and
//! meshes that grid the way trees are rounded (surface nets on the cell
//! grid), one 2x2 quad a cell face. Crowns were 85-90% of all drawn quads;
//! this draws about a quarter as many for the same crown. Each face is lit by
//! the crown's smoothed shape rather than its facets (`quad::normal_bits`),
//! as rounded foliage is at coarser detail too. Bark is meshed as if the
//! leaves weren't there, so bark and leaves are two closed surfaces that
//! cross, and no gap can open where a limb enters a crown. The voxels
//! themselves stay 0.25 m: collision, digging and the leaf sprites use them.
//! Chunks of the next level (0.5 m voxels, 128-256 m away at the default
//! detail radius) draw 1 m blocks the same way. Leaf sprites that would sit
//! inside a block are left out (`in_leaf_block`).
//! `VOXEL_LEAF_BLOCKS=0` meshes leaves voxel by voxel, for comparisons.
//!
//! Leaning stems. A thin stem that leans off an axis is drawn as a slanted
//! tube fitted through its voxels instead of their faces (mesh/lean.rs).
//!
//! A sloped, blended or rounded quad is always 1x1 (a leaf block's 2x2). Each direction's group
//! keeps its tilted quads (sloped or rounded) at the end; `tilted` counts them,
//! so the renderer can draw just those where a tilted face might show.

mod lean;
pub use lean::{Thread, THREAD_MAX_R};
pub mod decimate;
pub mod quad;
pub mod sprites;

use crate::world::gen::{M, MY, PAD, PAD3, PADY};
use crate::world::material::{rounds, smooths, tubes, AIR};

/// (normal axis, sign, u axis, v axis); axes 0 = x, 1 = y, 2 = z.
pub const DIRS: [(usize, i32, usize, usize); 6] = [(0, 1, 1, 2), (0, -1, 2, 1), (1, 1, 2, 0), (1, -1, 0, 2), (2, 1, 0, 1), (2, -1, 1, 0)];

pub use quad::Quad;

/// Unit normal of face direction `d` (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z).
pub fn dir_normal(d: usize) -> glam::Vec3 {
    use glam::Vec3;
    match d {
        0 => Vec3::X,
        1 => Vec3::NEG_X,
        2 => Vec3::Y,
        3 => Vec3::NEG_Y,
        4 => Vec3::Z,
        _ => Vec3::NEG_Z,
    }
}

#[derive(Default, Clone)]
pub struct Mesh {
    pub quads: Vec<Quad>,
    /// Number of quads for each direction, in order; quads are stored grouped.
    pub dir_counts: [u32; 6],
    /// Each group is [flat quads | rounded tree quads | sloped ground quads].
    /// Rounded quads stay in their own direction's group, so the shadow pass
    /// can draw trees as their square voxel faces (a performance compromise:
    /// see GUIDE.md). Sloped quads go to the group of the axis their
    /// triangles face most. Both tilted sections are sorted by how far their
    /// quads lean from the group's axis (see LEANS): `tails[c][g]` counts the
    /// sloped quads at the end of group g leaning more than LEANS[c - 1] (all
    /// of them for c = 0), and `rtails[c][g]` the same for the rounded section,
    /// which ends where the sloped one starts. Each quad decodes by its own
    /// direction.
    pub tails: [[u32; 6]; LEANS.len() + 1],
    pub rtails: [[u32; 6]; LEANS.len() + 1],
    /// Grass, flowers and leaf clusters (see sprites.rs), uploaded after the
    /// face groups. Only full-detail chunks have them.
    pub sprites: Vec<Quad>,
    /// The first `sprite_fill` sprites are fill tufts drawn only near the
    /// camera, the next `sprite_grass` the rest of the grass (sprites::scatter).
    pub sprite_fill: u32,
    pub sprite_grass: u32,
    /// The last `sprite_patch` are the far grass patches; just before them,
    /// `sprite_stalk` flower stalks that stay full size out to
    /// foliage::STALK_FADE.
    pub sprite_patch: u32,
    pub sprite_stalk: u32,
}

/// Lean classes of tilted quads: (cos, tan) of the largest lean in each. A
/// quad leaning past the last is "wild" and always drawn.
pub const LEANS: [(f32, f32); 4] = [(0.966, 0.27), (0.9, 0.49), (0.766, 0.84), (0.578, 1.42)];

/// Corners of a quad in voxel units within its chunk, winding order.
fn quad_corners(q: &Quad) -> [[f32; 3]; 4] {
    let p = quad::pos(q).map(|c| c as f32);
    let d = quad::dir(q);
    let (w, h) = quad::size(q);
    let (w, h) = (w as f32, h as f32);
    let rounded = quad::rounded(q);
    let packed = quad::round_bits(q);
    let sx = |v: u64, n: u32| ((v << (64 - n)) as i64 >> (64 - n)) as f32;
    let (na, sign, ua, va) = DIRS[d];
    [0usize, 1, 2, 3].map(|k| {
        let mut c = p;
        if sign > 0 {
            c[na] += 1.0;
        }
        c[ua] += if k == 1 || k == 2 { w } else { 0.0 };
        c[va] += if k >= 2 { h } else { 0.0 };
        if rounded {
            let b = packed >> (16 * k);
            let s = quad::round_scale(q) / 16.0;
            [c[0] + sx(b & 31, 5) * s, c[1] + sx(b >> 5 & 63, 6) * s, c[2] + sx(b >> 11 & 31, 5) * s]
        } else {
            [c[0], c[1] + sx((q[2] >> (4 * k)) as u64 & 15, 4) / 4.0, c[2]]
        }
    })
}

/// Group, section (0 flat, 1 rounded, 2 sloped) and lean class (0 for flat,
/// then 1 + the index into LEANS, or LEANS.len() + 1 for wild) of a quad.
fn classify(q: &Quad) -> (usize, u8, u8) {
    let d = quad::dir(q);
    if !tilted(q) {
        return (d, 0, 0);
    }
    let rounded = quad::rounded(q);
    let c = quad_corners(q);
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let unit = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 1e-6 {
            Some(v.map(|x| x / l))
        } else {
            None
        }
    };
    // The shader splits sloped and rounded quads into (0, 1, 2) and (0, 2, 3).
    let t1 = unit(cross(sub(c[1], c[0]), sub(c[2], c[0])));
    let t2 = unit(cross(sub(c[2], c[0]), sub(c[3], c[0])));
    let both = [t1, t2];
    let tris = || both.iter().flatten();
    if tris().next().is_none() {
        return (d, 1 + (!rounded) as u8, LEANS.len() as u8 + 1);
    }
    let mean = tris().fold([0.0; 3], |a, n| [a[0] + n[0], a[1] + n[1], a[2] + n[2]]);
    // Dominant axis of the mean normal, keeping the quad's own on a tie.
    let along = |g: usize| {
        let (na, sign, _, _) = DIRS[g];
        mean[na] * sign as f32
    };
    let g = if rounded {
        d
    } else {
        (0..6).fold(d, |best, g| if along(g) > along(best) + 1e-4 { g } else { best })
    };
    let (na, sign, _, _) = DIRS[g];
    let cos = tris().map(|n| n[na] * sign as f32).fold(1.0f32, f32::min);
    let class = 1 + LEANS.iter().take_while(|(c, _)| cos < *c).count() as u8;
    (g, 1 + (!rounded) as u8, class)
}

#[inline]
fn vertex_ao(s1: bool, s2: bool, c: bool) -> u32 {
    if s1 && s2 {
        0
    } else {
        3 - (s1 as u32 + s2 as u32 + c as u32)
    }
}

/// Index step for one voxel along an axis in the padded array (x, y, z).
const STRIDE: [isize; 3] = [1, (PAD * PAD) as isize, PAD as isize];
/// Padded coordinates of chunk voxel (0, 0, 0).
const OFF: [isize; 3] = [M as isize, MY as isize, M as isize];
const DY: isize = STRIDE[1];

#[inline]
fn idx(x: isize, y: isize, z: isize) -> usize {
    (y * DY + z * PAD as isize + x) as usize
}

/// How many levels up or down a column's ground may be and still count toward
/// a corner. Four reaches a staircase of two voxels per column; the padding in
/// gen.rs (`MY`) is sized for it.
const REACH: isize = 4;

/// Height of the ground surface of column (x, z) nearest level t, within
/// `REACH` (t first, then t+1, t-1, t+2, ...): a ground voxel with air above.
fn ground_near(at: &impl Fn(isize, isize, isize) -> u8, x: isize, z: isize, t: isize, reach: isize) -> Option<isize> {
    for r in 0..=reach {
        for s in [t + r, t - r] {
            if smooths(at(x, s - 1, z)) && at(x, s, z) == AIR {
                return Some(s);
            }
            if r == 0 {
                break;
            }
        }
    }
    None
}

/// Offset in quarter voxels (-4 to 4) of the ground corner at lattice point
/// (x, z) on the surface at height t. Each column of the 4x4 block around the
/// corner gives its ground surface nearest t; columns with none are left out.
/// A surface of another material at t right beside the corner (a plank floor,
/// say), or a block hanging with its underside at t, pins it.
fn corner_offset(
    at: &impl Fn(isize, isize, isize) -> u8,
    near: &impl Fn(isize, isize, isize, isize) -> Option<isize>,
    colh: &impl Fn(isize, isize) -> Option<f32>,
    x: isize,
    z: isize,
    t: isize,
) -> i32 {
    corner_raw(at, near, colh, x, z, t).0
}

/// `corner_offset`, plus the exact offset in voxels where it came from the
/// columns' true heights. `colh(x, z)` is the generated ground height of a
/// column in the same voxel units as t, when known. Where the four columns
/// round the corner are unedited ground (each voxel surface is its true height
/// rounded) and within a voxel of each other, the corner sits at their mean
/// true height: gentle slopes come out as even ramps instead of 0.25 m steps
/// eased over a metre. Clamped to a voxel of every one of the four surfaces, so
/// the corner's absolute height is the same from each of their levels and the
/// surface stays closed.
fn corner_raw(
    at: &impl Fn(isize, isize, isize) -> u8,
    near: &impl Fn(isize, isize, isize, isize) -> Option<isize>,
    colh: &impl Fn(isize, isize) -> Option<f32>,
    x: isize,
    z: isize,
    t: isize,
) -> (i32, Option<f32>) {
    let (mut sum, mut n) = (0i32, 0i32);
    let (mut hsum, mut natural, mut smin, mut smax) = (0.0f32, 0, isize::MAX, isize::MIN);
    for cz in z - 2..z + 2 {
        for cx in x - 2..x + 2 {
            let inner = (cx == x - 1 || cx == x) && (cz == z - 1 || cz == z);
            let (b, h) = (at(cx, t - 1, cz), at(cx, t, cz));
            // Another material's surface at t, or the underside of anything
            // hanging at t (a canopy just over the ground), right beside the
            // corner: it must not move, or the two would part.
            let floor_of_other = h == AIR && b != AIR && !smooths(b);
            let ceiling = h != AIR && b == AIR;
            if inner && (floor_of_other || ceiling) {
                return (0, None);
            }
            // Further out, only ground counts, whatever else stands there: a
            // column that counted at one level and not the next could make the
            // corner of the higher ground come out lower than the lower one's.
            if let Some(s) = near(cx, cz, t, REACH) {
                sum += (s - t) as i32;
                n += 1;
                if inner {
                    if let Some(h) = colh(cx, cz) {
                        if (s as f32 - h).abs() <= 0.55 {
                            hsum += h;
                            natural += 1;
                            smin = smin.min(s);
                            smax = smax.max(s);
                        }
                    }
                }
            }
        }
    }
    if natural == 4 && smax - smin <= 1 {
        let a = (hsum * 0.25).clamp(smax as f32 - 1.0, smin as f32 + 1.0);
        let q = ((4.0 * a + 0.5).floor() as i32 - 4 * t as i32).clamp(-4, 4);
        return (q, Some(a - t as f32));
    }
    if n == 0 {
        return (0, None);
    }
    // Round half up, never half away from zero: the same corner seen from the
    // level above is this value plus whole voxels, and must round the same way.
    // At most one voxel either way: the unmoved middle edges of a taller riser
    // are a voxel apart, and a corner that passed one would fold the surface
    // over itself and open a hole.
    ((8 * sum + n).div_euclid(2 * n).clamp(-4, 4), None)
}

/// Large buffers the mesher reuses from one chunk to the next on the same
/// worker thread. Allocating them fresh for every chunk on all workers at once
/// is the pattern that stalled the main thread before (see gen.rs GenScratch).
#[derive(Default)]
struct Pool {
    memo: Vec<i8>,
    rmemo: Vec<u16>,
    surf: Vec<[u8; SURF]>,
    surf_n: Vec<u8>,
    dens: Vec<u8>,
    ground: Vec<bool>,
    dens_tmp: Vec<u8>,
    ground_tmp: Vec<bool>,
    keys: Vec<u16>,
    quads: Vec<Quad>,
    marked: Vec<u8>,
}

thread_local! {
    static POOL: std::cell::RefCell<Pool> = std::cell::RefCell::new(Pool::default());
}

/// A pooled buffer cleared and filled with `fill` to length `n`.
fn pooled<T: Clone>(slot: impl FnOnce(&mut Pool) -> &mut Vec<T>, n: usize, fill: T) -> Vec<T> {
    let mut v = POOL.with(|p| std::mem::take(slot(&mut p.borrow_mut())));
    v.clear();
    v.resize(n, fill);
    v
}

/// Ground corner heights, memoised per (corner, surface height).
struct Corners<'a> {
    v: &'a [u8; PAD3],
    memo: Vec<i8>,
    /// Rounded tree corners per lattice point: 0 unknown, 1 none, else packed.
    rmemo: Vec<u16>,
    trees: Option<TreeFields>,
    /// Ground surface heights of each padded column, lowest first, so a
    /// corner's 16 columns are a few compares each instead of voxel scans.
    /// A column with more than `SURF` keeps `SURF + 1` as its count and is
    /// scanned instead.
    surf: Vec<[u8; SURF]>,
    surf_n: Vec<u8>,
    /// True ground height of each padded column in voxels (see `corner_raw`).
    colh: Option<&'a [f32]>,
}

const UNKNOWN: i8 = i8::MIN;
const SURF: usize = 6;

impl<'a> Corners<'a> {
    fn new(v: &'a [u8; PAD3], colh: Option<&'a [f32]>) -> Self {
        let mut surf = pooled(|p| &mut p.surf, PAD * PAD, [0u8; SURF]);
        let mut surf_n = pooled(|p| &mut p.surf_n, PAD * PAD, 0u8);
        for y in 1..PADY {
            let (below, here) = (&v[(y - 1) * PAD * PAD..y * PAD * PAD], &v[y * PAD * PAD..(y + 1) * PAD * PAD]);
            for c in 0..PAD * PAD {
                if here[c] == AIR && smooths(below[c]) {
                    let n = surf_n[c] as usize;
                    if n < SURF {
                        surf[c][n] = y as u8;
                    }
                    surf_n[c] = (n + 1).min(SURF + 1) as u8;
                }
            }
        }
        let memo = pooled(|p| &mut p.memo, PAD3, UNKNOWN);
        let rmemo = pooled(|p| &mut p.rmemo, PAD3, 0u16);
        Self {
            v,
            memo,
            rmemo,
            trees: None,
            surf,
            surf_n,
            colh,
        }
    }

    /// `ground_near` from the lists: the nearest surface within reach, the
    /// upper one on a tie.
    fn near(&self, x: isize, z: isize, t: isize, reach: isize) -> Option<isize> {
        let c = (z * PAD as isize + x) as usize;
        let n = self.surf_n[c] as usize;
        if n > SURF {
            return ground_near(&|x, y, z| self.at(x, y, z), x, z, t, reach);
        }
        let mut best: Option<isize> = None;
        for &s in &self.surf[c][..n] {
            let s = s as isize;
            let d = (s - t).abs();
            if d <= reach && best.map_or(true, |b| d <= (b - t).abs()) {
                best = Some(s);
            }
        }
        best
    }

    #[inline]
    fn at(&self, x: isize, y: isize, z: isize) -> u8 {
        self.v[idx(x, y, z)]
    }

    /// `round_corner`, memoised; the memo holds the three 4-bit fields plus 1.
    fn round(&mut self, x: isize, y: isize, z: isize) -> Option<[i32; 3]> {
        let key = idx(x, y, z);
        let m = self.rmemo[key];
        if m == 0 {
            let r = round_corner(self.v, &mut self.trees, x, y, z);
            self.rmemo[key] = r.map_or(1, |o| 2 | ((o[0] + 8) as u16) << 2 | ((o[1] + 8) as u16) << 6 | ((o[2] + 8) as u16) << 10);
            return r;
        }
        if m == 1 {
            return None;
        }
        Some([(m >> 2 & 15) as i32 - 8, (m >> 6 & 15) as i32 - 8, (m >> 10 & 15) as i32 - 8])
    }

    fn get(&mut self, x: isize, z: isize, t: isize) -> i32 {
        let key = idx(x, t, z);
        if self.memo[key] == UNKNOWN {
            let v = self.v;
            let ch = self.colh;
            self.memo[key] = corner_offset(&|x, y, z| v[idx(x, y, z)], &|x, z, t, r| self.near(x, z, t, r), &|x, z| padded_h(ch, x, z), x, z, t) as i8;
        }
        self.memo[key] as i32
    }

    /// Offset in quarter voxels of the corner at lattice point (x, z), height h,
    /// on a side face of column X looking into the air of column Y. The edge
    /// follows Y's ground if it lies on it, X's ground top if it lies on that,
    /// and between the two if it is the middle of a two-voxel riser.
    fn edge(&mut self, x: isize, z: isize, h: isize, xc: (isize, isize), yc: (isize, isize)) -> i32 {
        let ya = |s: &Self, k: isize| s.at(yc.0, k, yc.1);
        let xa = |s: &Self, k: isize| s.at(xc.0, k, xc.1);
        let floor = if smooths(ya(self, h - 1)) {
            Some(h)
        } else if ya(self, h - 1) == AIR && smooths(ya(self, h - 2)) {
            Some(h - 1)
        } else {
            None
        };
        let top = if smooths(xa(self, h - 1)) && xa(self, h) == AIR {
            Some(h)
        } else if smooths(xa(self, h)) && xa(self, h + 1) == AIR {
            Some(h + 1)
        } else {
            None
        };
        // Where something overhangs the edge (a canopy just above the ground, a
        // ledge), the corner may only move away from it: a floating block's edge
        // can rise to meet the ground but not hang below the block, and ground
        // cannot push its edge up into a block sitting over the air beside it.
        let x_under = xa(self, h - 1) != AIR;
        let y_open = ya(self, h) == AIR;
        match (floor, top) {
            (Some(s), _) if s == h => {
                let o = self.get(x, z, h);
                if x_under {
                    o
                } else {
                    o.max(0)
                }
            }
            (_, Some(s)) if s == h => {
                let o = self.get(x, z, h);
                if y_open {
                    o
                } else {
                    o.min(0)
                }
            }
            (Some(lo), Some(hi)) if y_open => {
                let a = 4 * lo as i32 + self.get(x, z, lo);
                let b = 4 * hi as i32 + self.get(x, z, hi);
                let (num, den) = (a * (hi - h) as i32 + b * (h - lo) as i32, (hi - lo) as i32);
                (2 * num + den).div_euclid(2 * den) - 4 * h as i32
            }
            _ => 0,
        }
    }

    /// Blend for the ground top face of column (x, z) at height t, material m:
    /// the most common other ground material among the eight columns around it
    /// (surfaces within a level), and per corner how many of the four columns
    /// meeting there have it. Returns word2's upper half, or 0.
    fn blend(&self, x: isize, z: isize, t: isize, m: u8) -> u32 {
        let mut mats = [0u8; 9];
        for dz in 0..3 {
            for dx in 0..3 {
                let (cx, cz) = (x + dx - 1, z + dz - 1);
                mats[(dz * 3 + dx) as usize] = self.near(cx, cz, t, 1).map_or(0, |s| self.at(cx, s - 1, cz));
            }
        }
        let mut best = (0, 0u8);
        for &c in &mats {
            if c != 0 && c != m {
                let n = mats.iter().filter(|&&o| o == c).count();
                if n > best.0 || (n == best.0 && c < best.1) {
                    best = (n, c);
                }
            }
        }
        if best.0 == 0 {
            return 0;
        }
        let other = best.1;
        // Paving and road surfaces keep a sharp edge.
        if crate::world::matlib::crisp(m) || crate::world::matlib::crisp(other) {
            return 0;
        }
        // +Y corners: k = 0 (x, z), 1 (x, z+1), 2 (x+1, z+1), 3 (x+1, z); the
        // four columns at a corner are the 2x2 of the 3x3 block nearest it.
        let mut w = 0u32;
        for (k, (cx, cz)) in [(0, 0), (0, 1), (1, 1), (1, 0)].into_iter().enumerate() {
            let n = (0..2).flat_map(|j| (0..2).map(move |i| (cx + i, cz + j))).filter(|&(i, j)| mats[j * 3 + i] == other).count();
            w |= (n.min(3) as u32) << (2 * k);
        }
        (other as u32) << 16 | w << 24
    }
}

/// A padded column's true height from a mesher height array, if any.
fn padded_h(colh: Option<&[f32]>, x: isize, z: isize) -> Option<f32> {
    let c = colh?;
    if x < 0 || z < 0 || x >= PAD as isize || z >= PAD as isize {
        return None;
    }
    Some(c[z as usize * PAD + x as usize])
}

/// Height in voxels of the drawn ground under the point (x, z), for the ground
/// surface of that column nearest y (within two voxels), or None. Coordinates
/// are in voxels; `at` reads any voxel. It splits each cell along the same
/// diagonal as the shader, so feet stand on the slope that is drawn.
#[cfg(test)]
pub fn ground_height(at: impl Fn(isize, isize, isize) -> u8, x: f32, z: f32, y: f32) -> Option<f32> {
    ground_height_h(at, |_, _| None, x, z, y)
}

/// `ground_height` with the columns' true heights (`colh`, voxels, in the same
/// coordinates as `at`): on gentle unedited ground the corners are exact, not
/// rounded to quarter voxels as drawn, so a wheel rolls a smooth grade.
pub fn ground_height_h(at: impl Fn(isize, isize, isize) -> u8, colh: impl Fn(isize, isize) -> Option<f32>, x: f32, z: f32, y: f32) -> Option<f32> {
    ground_height_with(&at, |px, pz, t| ground_corner(&at, &colh, px, pz, t), x, z, y)
}

/// The height offset in voxels of the drawn ground's corner at lattice point
/// (px, pz) on the surface at height t (`ground_height_h`'s corners).
pub fn ground_corner(at: &impl Fn(isize, isize, isize) -> u8, colh: &impl Fn(isize, isize) -> Option<f32>, px: isize, pz: isize, t: isize) -> f32 {
    let near = |x, z, t, r| ground_near(at, x, z, t, r);
    let (q, exact) = corner_raw(at, &near, colh, px, pz, t);
    exact.unwrap_or(q as f32 / 4.0)
}

/// `ground_height_h` with its corners from `corner(px, pz, t)`, which a
/// caller sampling the same ground many times can remember.
pub fn ground_height_with(at: &impl Fn(isize, isize, isize) -> u8, corner: impl Fn(isize, isize, isize) -> f32, x: f32, z: f32, y: f32) -> Option<f32> {
    let (cx, cz) = (x.floor() as isize, z.floor() as isize);
    let t = ground_near(at, cx, cz, y.round() as isize, 2)?;
    let o = |px, pz| corner(px, pz, t);
    let (h0, h1, h2, h3) = (o(cx, cz), o(cx, cz + 1), o(cx + 1, cz + 1), o(cx + 1, cz));
    let (fx, fz) = (x - cx as f32, z - cz as f32);
    // Triangles (0, 1, 2) and (0, 2, 3), split along (x, z) - (x+1, z+1).
    let h = if fz >= fx { h0 + (h2 - h1) * fx + (h1 - h0) * fz } else { h0 + (h3 - h0) * fx + (h2 - h3) * fz };
    Some(t as f32 + h)
}

/// A sloped or rounded quad: its plane may face away from its direction.
pub fn tilted(q: &Quad) -> bool {
    quad::slope_bits(q) != 0 || quad::rounded(q)
}

/// Per-chunk fields for rounding trees, built only for chunks that have them.
struct TreeFields {
    /// Tree voxels among the 3x3x3 around each voxel (0 to 27).
    dens: Vec<u8>,
    /// Ground anywhere in the 4x4x4 voxels around each lattice point.
    ground: Vec<bool>,
}

impl TreeFields {
    fn new(v: &[u8; PAD3]) -> Self {
        let strides = [1usize, PAD * PAD, PAD];
        let dims = [PAD, PADY, PAD];
        let coord = |i: usize, a: usize| match a {
            0 => i % PAD,
            1 => i / (PAD * PAD),
            _ => (i / PAD) % PAD,
        };
        // Separable 3-wide box sums along x, y, z.
        let mut dens = pooled(|p| &mut p.dens, 0, 0u8);
        dens.extend(v.iter().map(|&m| rounds(m) as u8));
        let mut src = pooled(|p| &mut p.dens_tmp, PAD3, 0u8);
        for a in 0..3 {
            src.copy_from_slice(&dens);
            for (i, d) in dens.iter_mut().enumerate() {
                let c = coord(i, a);
                *d = if c >= 1 && c + 1 < dims[a] { src[i - strides[a]] + src[i] + src[i + strides[a]] } else { 0 };
            }
        }
        // Separable 4-wide ORs: lattice point c covers voxels c-2 ..= c+1.
        let mut ground = pooled(|p| &mut p.ground, 0, false);
        ground.extend(v.iter().map(|&m| smooths(m)));
        let mut gsrc = pooled(|p| &mut p.ground_tmp, PAD3, false);
        for a in 0..3 {
            gsrc.copy_from_slice(&ground);
            for (i, g) in ground.iter_mut().enumerate() {
                let c = coord(i, a);
                *g = c >= 2 && c + 1 < dims[a] && (0..4).any(|o| gsrc[i + o * strides[a] - 2 * strides[a]]);
            }
        }
        POOL.with(|p| {
            let mut p = p.borrow_mut();
            p.dens_tmp = src;
            p.ground_tmp = gsrc;
        });
        Self { dens, ground }
    }
}

impl Drop for Corners<'_> {
    /// Hand the buffers back for the next chunk.
    fn drop(&mut self) {
        POOL.with(|p| {
            let mut p = p.borrow_mut();
            p.memo = std::mem::take(&mut self.memo);
            p.rmemo = std::mem::take(&mut self.rmemo);
            p.surf = std::mem::take(&mut self.surf);
            p.surf_n = std::mem::take(&mut self.surf_n);
            if let Some(t) = self.trees.take() {
                p.dens = t.dens;
                p.ground = t.ground;
            }
        });
    }
}

/// Corner offset (x, y, z) in sixteenths of a voxel of the tree corner at padded
/// lattice point (x, y, z), or None when the eight voxels around it are not all
/// tree or air (or are all one of them), or ground lies within a voxel.
fn round_corner(v: &[u8; PAD3], f: &mut Option<TreeFields>, x: isize, y: isize, z: isize) -> Option<[i32; 3]> {
    let mut solid = [[[false; 2]; 2]; 2];
    let mut count = 0;
    let mut facet = true;
    // Bevelled where any of the eight is a built thing: where a faceted dome
    // sits on a bevelled ring, the ring's straight edge wins, not a zigzag.
    let mut steps = false;
    for (i, si) in solid.iter_mut().enumerate() {
        for (j, sj) in si.iter_mut().enumerate() {
            for (k, s) in sj.iter_mut().enumerate() {
                let m = v[idx(x - 1 + i as isize, y - 1 + j as isize, z - 1 + k as isize)];
                if m != AIR {
                    if !rounds(m) {
                        return None;
                    }
                    facet &= crate::world::matlib::facets(m);
                    steps |= crate::world::matlib::steps_only(m);
                    *s = true;
                    count += 1;
                }
            }
        }
    }
    if count == 0 || count == 8 {
        return None;
    }
    // Built things (`bevel_steps`) move a corner only along the axes where
    // the shape does not run straight on: none at the edge of a box, only
    // across at the foot of a stepped pillar, both ways at a stair's lip.
    let keep = if steps { clean_axes(v, x, y, z) } else { [false; 3] };
    if keep == [true; 3] {
        return Some([0; 3]);
    }
    let f = f.get_or_insert_with(|| TreeFields::new(v));
    // Ground in the 4x4x4 voxels around: stay square. The ground rules move a
    // corner by which face asks, which is only safe beside other vertical
    // moves; this keeps every corner next to a rounded one unmoved.
    // Built things skip it: ground among their own eight voxels already kept
    // the corner square (it doesn't round), and nearer than that no ground
    // face shares the corner. Without this a wall on a slab one voxel thick
    // kept its foot square and bevelled the row above, a row of teeth.
    if !steps && f.ground[idx(x, y, z)] {
        return None;
    }
    let dens = |i: usize, j: usize, k: usize| f.dens[idx(x - 1 + i as isize, y - 1 + j as isize, z - 1 + k as isize)] as i32;
    // Each crossing edge contributes the point where the blurred density
    // (tree share of 27) passes the half-way level, kept on the edge's middle
    // half so the vertex stays well inside its cell; the vertex is their mean.
    // Positions are in 64ths of a voxel from the lattice point.
    // Faceted rock takes each crossing at the edge's middle (plain surface
    // nets): a one-voxel step becomes one flat slanted face, not a rounded lump.
    let cross = |da: i32, db: i32| -> i32 {
        if facet {
            return 0;
        }
        let t = if da == db { 32 } else { ((27 - 2 * da) * 64 / (2 * (db - da))).clamp(16, 48) };
        t - 32
    };
    let (mut sum, mut n) = ([0i32; 3], 0i32);
    for a in 0..2 {
        for b in 0..2 {
            let (ha, hb) = (64 * a as i32 - 32, 64 * b as i32 - 32);
            if solid[0][a][b] != solid[1][a][b] {
                sum[0] += cross(dens(0, a, b), dens(1, a, b));
                sum[1] += ha;
                sum[2] += hb;
                n += 1;
            }
            if solid[a][0][b] != solid[a][1][b] {
                sum[0] += ha;
                sum[1] += cross(dens(a, 0, b), dens(a, 1, b));
                sum[2] += hb;
                n += 1;
            }
            if solid[a][b][0] != solid[a][b][1] {
                sum[0] += ha;
                sum[1] += hb;
                sum[2] += cross(dens(a, b, 0), dens(a, b, 1));
                n += 1;
            }
        }
    }
    // Mean in eighths (8 64ths each), rounded half up, then doubled into the
    // sixteenths the packing holds: tree corners keep the eighth-voxel grid
    // they have always rounded to.
    let mut o = sum.map(|s| 2 * (2 * s + 8 * n).div_euclid(16 * n).clamp(-4, 4));
    for a in 0..3 {
        if keep[a] {
            o[a] = 0;
        }
    }
    Some(o)
}

/// For each axis, whether the shape at lattice point (x, y, z) runs straight
/// on along it: the 2x2 slab of voxels one further out on either side matches
/// the slab next to the point. All three hold at the edge or corner of a box
/// two voxels or more across, a wall corner, the foot of a wall and the middle
/// of a flat face, so every face meeting there runs flat for two voxels; none
/// at the lip of a staircase of one-voxel steps; only the vertical one along
/// the stepped outline of an upright pillar.
fn clean_axes(v: &[u8; PAD3], x: isize, y: isize, z: isize) -> [bool; 3] {
    let solid = |p: [isize; 3]| {
        if p[0] < 0 || p[1] < 0 || p[2] < 0 || p[0] as usize >= PAD || p[1] as usize >= PADY || p[2] as usize >= PAD {
            return None;
        }
        Some(v[idx(p[0], p[1], p[2])] != AIR)
    };
    let c = [x, y, z];
    let mut out = [true; 3];
    for (a, keep) in out.iter_mut().enumerate() {
        let (b, d) = ((a + 1) % 3, (a + 2) % 3);
        for i in -1..=0isize {
            for j in -1..=0isize {
                let at = |k: isize| {
                    let mut p = c;
                    p[a] += k;
                    p[b] += i;
                    p[d] += j;
                    solid(p)
                };
                // Outside the padded array: count it as running on.
                let (o, n, n2, o2) = (at(-2), at(-1), at(0), at(1));
                if o.is_some() && o != n || o2.is_some() && o2 != n2 {
                    *keep = false;
                }
            }
        }
    }
    out
}

/// Bevel of a tube's edges, sixteenths of a voxel. An equal-sided octagon
/// inside a one-voxel square wants 16 / (2 + sqrt 2) = 4.69; 5 is the nearest
/// the packing holds, which leaves the chamfers a touch wider than the flats
/// and the stem a little rounder than a regular octagon.
const BEVEL: i32 = 5;

/// The face of tube voxel `i` (padded index) toward direction `d`, bevelled:
/// each edge of the face that runs along the stem (the voxel continues past
/// it one way or the other) with open air beyond it and round the corner is
/// pulled in by BEVEL, and the slanted strip that closes the corner is
/// returned too, made by whichever of the two faces meeting there has the
/// lower axis. Corner offsets as for rounded quads. None when no edge bevels.
///
/// A face the stem runs into (the voxel carries on straight through the
/// opposite face) is an end cap, and is cut to the same octagon as the
/// cross-section. One quad can't be an octagon, so the cap comes back as
/// three: the middle band (the face itself, pulled in along `ua`) and a
/// trapezoid closing each end, in the first two chamfer slots. The sides'
/// own chamfers already run down to its slanted edges, so it needs none.
/// Left square, a cap stuck out past the octagon and the open ring at its
/// rim showed daylight through the stem: the white specks at the foot of a
/// bamboo culm.
#[allow(clippy::type_complexity)]
fn tube_face(v: &[u8; PAD3], i: isize, d: usize) -> Option<([[i32; 3]; 4], [Option<[[i32; 3]; 4]>; 4])> {
    let (na, sign, ua, va) = DIRS[d];
    let toward = sign as isize * STRIDE[na];
    let air = |j: isize| v[j as usize] == AIR;
    let stem = |j: isize| tubes(v[j as usize]);
    // Ground beside or under the stem: keep this voxel square, as a tree's
    // foot stays square where ground is within a voxel. A bevelled face is
    // not pulled down to the sloped ground the way a plain one is, so a
    // bevelled foot stands clear of it and the gaps show light through the
    // hollow stem.
    let hard = |j: isize| {
        let m = v[j as usize];
        m != AIR && !tubes(m)
    };
    for dy in [-1isize, 0] {
        for dz in -1isize..=1 {
            for dx in -1isize..=1 {
                if (dy, dx, dz) != (0, 0, 0) && hard(i + dy * STRIDE[1] + dz * STRIDE[2] + dx * STRIDE[0]) {
                    return None;
                }
            }
        }
    }
    let mut face = [[0i32; 3]; 4];
    let mut chamfers = [None; 4];
    let mut any = false;
    // (edge axis, side, the face corners on that edge)
    let edges = [(ua, -1, [0usize, 3]), (ua, 1, [1, 2]), (va, -1, [0, 1]), (va, 1, [2, 3])];
    let open = |axis: usize, side: i32| {
        let step = side as isize * STRIDE[axis];
        air(i + step) && air(i + step + toward)
    };
    // The stem runs into this face: cut the cap to the octagon, but only
    // where every voxel around the stem's cross-section is air, which is when
    // all four side faces are bevelled and all four corner chamfers run down
    // to the cap. Hemmed in, it stays the square it has always been. The
    // voxel beyond the cap itself may be anything: a culm's foot stands on
    // the ground.
    let clear = |axis: usize, side: i32| {
        let step = side as isize * STRIDE[axis];
        let other = if axis == ua { va } else { ua };
        air(i + step) && air(i + step + STRIDE[other]) && air(i + step - STRIDE[other])
    };
    if stem(i - toward) && edges.iter().all(|&(axis, side, _)| clear(axis, side)) {
        let b = BEVEL;
        let (full, inner) = (16 - b, b - 16);
        // Middle band: the square pulled in by the bevel at each ua edge.
        for (k, c) in face.iter_mut().enumerate() {
            c[ua] = if k == 1 || k == 2 { -b } else { b };
        }
        // The end trapezoids, each from a flat side of the octagon out to the
        // two slanted ones. Corner order follows the face's, so they wind the
        // same way and face out with it.
        let mut lo = [[0i32; 3]; 4];
        lo[0][va] = b;
        lo[1][ua] = inner;
        lo[2][ua] = inner;
        lo[3][va] = -b;
        let mut hi = [[0i32; 3]; 4];
        hi[0][ua] = full;
        hi[1][va] = b;
        hi[2][va] = -b;
        hi[3][ua] = full;
        return Some((face, [Some(lo), Some(hi), None, None]));
    }
    for (n, &(axis, side, on)) in edges.iter().enumerate() {
        let along = if axis == ua { va } else { ua };
        if !open(axis, side) {
            continue;
        }
        if !stem(i + STRIDE[along]) && !stem(i - STRIDE[along]) {
            continue;
        }
        any = true;
        for &k in &on {
            face[k][axis] -= side * BEVEL;
        }
        if na < axis {
            let mut c = [[0i32; 3]; 4];
            for (k, ck) in c.iter_mut().enumerate() {
                if on.contains(&k) {
                    ck[na] = -sign * BEVEL;
                } else {
                    ck[axis] = side * (16 - BEVEL);
                }
            }
            chamfers[n] = Some(c);
        }
    }
    any.then_some((face, chamfers))
}

/// Pack four corner offsets (sixteenths, quarters for a leaf block) into
/// word2 and word3 of a rounded quad: x:5 y:6 z:5 each, corner k at bit 16k.
pub(crate) fn pack_round(c: &[[i32; 3]; 4]) -> (u32, u32) {
    let mut v = 0u64;
    for (k, o) in c.iter().enumerate() {
        let bits = (o[0] as u64 & 31) | (o[1] as u64 & 63) << 5 | (o[2] as u64 & 31) << 11;
        v |= bits << (16 * k);
    }
    ((v >> 32) as u32, v as u32)
}

#[cfg(test)]
pub fn mesh_chunk(v: &[u8; PAD3], smooth: bool, out: &mut Mesh) {
    mesh_chunk_h(v, smooth, None, false, out)
}

/// Mesh a model's voxels (palette materials): square faces, and none of the
/// terrain's content rules, such as herb markers meshing as air. Rods whose
/// line is known (`models::Grid::rod`) are drawn as round tubes along them
/// (mesh/lean.rs), the voxels under them hidden.
pub fn mesh_model_t(v: &[u8; PAD3], threads: &[Thread], out: &mut Mesh) {
    mesh_impl_t(v, false, true, None, None, false, threads, &[], out);
}

/// `mesh_chunk` with each padded column's true ground height in voxels
/// (PAD x PAD, z-major, in the padded array's coordinates), for even slopes.
/// `blocks`: mesh foliage as leaf blocks of 2x2x2 voxels (the two finest
/// levels; see the module notes), when slopes are on.
pub fn mesh_chunk_h(v: &[u8; PAD3], smooth: bool, colh: Option<&[f32]>, blocks: bool, out: &mut Mesh) {
    mesh_chunk_t(v, smooth, colh, blocks, &[], &[], out)
}

/// `mesh_chunk_h` with the threads (thin stems whose line is known, from
/// `Generator::threads`) that pass through the chunk: they are drawn as
/// tubes along their lines (mesh/lean.rs). `own` marks the padded voxels
/// that `owned` threads may hide (`Generator::chunk_threads`).
#[allow(clippy::too_many_arguments)]
pub fn mesh_chunk_t(v: &[u8; PAD3], smooth: bool, colh: Option<&[f32]>, blocks: bool, threads: &[Thread], own: &[bool], out: &mut Mesh) {
    let blocks = blocks && smooth && leaf_blocks_on() && v.iter().any(|&m| block_leaf(m));
    // Herb markers (one voxel at the foot of a fern or fireweed) are drawn
    // only as sprites: mesh as if they were air. So are leaves drawn as blocks.
    if blocks || v.iter().any(|&m| sprites::marker(m)) {
        let mut c = pooled(|p| &mut p.marked, 0, 0u8);
        c.extend(v.iter().map(|&m| if sprites::marker(m) || (blocks && block_leaf(m)) { AIR } else { m }));
        mesh_impl_t(c.as_slice().try_into().unwrap(), smooth, true, colh, blocks.then_some(v), true, threads, own, out);
        POOL.with(|p| p.borrow_mut().marked = c);
    } else {
        mesh_impl_t(v, smooth, true, colh, None, true, threads, own, out);
    }
}

/// Stem bevelling is on unless VOXEL_TUBES=0 (for before/after comparisons).
fn tubes_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_TUBES").map(|v| v != "0").unwrap_or(true))
}

/// Leaf blocks are on unless VOXEL_LEAF_BLOCKS=0.
fn leaf_blocks_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_LEAF_BLOCKS").map(|v| v != "0").unwrap_or(true))
}

/// Foliage drawn as leaf blocks: rounded leaves, not faceted rock or stems.
fn block_leaf(m: u8) -> bool {
    m != AIR && rounds(m) && crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE) && !crate::world::matlib::facets(m) && !tubes(m)
}

/// The outward normal of the tree surface at voxel `i` (padded index), from
/// the slope of the blurred tree density two voxels either way over a 3x3
/// cross-section: smooth across a crown, so its faces light as one volume.
/// None where the density is flat.
fn smoothed(dens: &[u8], i: isize) -> Option<[f32; 3]> {
    let at = |o: isize| -> i32 {
        let k = i + o;
        if k >= 0 && (k as usize) < dens.len() {
            dens[k as usize] as i32
        } else {
            0
        }
    };
    let mut g = [0f32; 3];
    for a in 0..3 {
        let (b, c) = (STRIDE[(a + 1) % 3], STRIDE[(a + 2) % 3]);
        let mut s = 0;
        for j in -1..=1isize {
            for k in -1..=1isize {
                let side = j * b + k * c;
                s += at(2 * STRIDE[a] + side) - at(-2 * STRIDE[a] + side);
            }
        }
        g[a] = -(s as f32);
    }
    unit3(g)
}

fn unit3(g: [f32; 3]) -> Option<[f32; 3]> {
    let l = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    (l > 1.0).then(|| g.map(|x| x / l))
}

/// Whether padded voxel (x, y, z) of `v`, meshed with slopes on (`smooth`),
/// lies in a leaf block: inside a crown's drawn surface, where a leaf sprite
/// would never be seen.
pub fn in_leaf_block(v: &[u8; PAD3], smooth: bool, x: isize, y: isize, z: isize) -> bool {
    if !smooth || !leaf_blocks_on() || x < 0 || y < 0 || z < 0 || x as usize >= PAD || y as usize >= PADY || z as usize >= PAD {
        return false;
    }
    let (bx, by, bz) = (x & !1, y & !1, z & !1);
    let mut n = 0;
    for i in 0..8 {
        n += block_leaf(v[idx(bx + (i & 1), by + (i >> 1 & 1), bz + (i >> 2 & 1))]) as u8;
    }
    n >= BLOCK_MIN
}

/// Leaf block cells across and up the padded array.
const BX: usize = PAD / 2;
const BY: usize = PADY / 2;
const _: () = assert!(M % 2 == 0 && MY % 2 == 0, "leaf block cells must line up with chunks");

/// A cell with at least this many leaf voxels of its eight is leaves. Over
/// half, so the block surface sits a little inside the crown's voxels and the
/// leaf sprites on their faces make the silhouette; at 3 the surface bulged
/// past the sprites as a pale low-poly shell. (Bark is its own surface, so a
/// smaller crown opens no holes where the limbs go in.)
const BLOCK_MIN: u8 = 5;

#[inline]
fn bidx(x: usize, y: usize, z: usize) -> usize {
    (y * BX + z) * BX + x
}

/// Mesh the leaves of `v` as 0.5 m blocks (see the module notes) into `out`.
/// Every input is a count over cells this chunk and its neighbour both hold
/// in their padding, so corners on a chunk's side come out the same in both.
fn leaf_blocks(v: &[u8; PAD3], out: &mut Mesh) {
    const N: usize = BX * BY * BX;
    let mut count = [0u8; N];
    let mut mat = [AIR; N];
    // Something in the cell nobody draws: air, a sprite marker (meshed as
    // air), or leaf voxels too few to make a block. A leaf face toward such a
    // cell must be drawn. Hiding it beside leaves that fell short of a block
    // opened holes through a crown wherever another tree's leaves of a
    // non-foliage material shared its cells (a conifer beside an autumn
    // maple, 2026-11-17): neither surface was drawn there.
    let mut air = [false; N];
    // Something solid that isn't tree (ground, a wall): corners there stay square.
    let mut hard = [false; N];
    for y in 0..PADY {
        for z in 0..PAD {
            for x in 0..PAD {
                let m = v[idx(x as isize, y as isize, z as isize)];
                let b = bidx(x / 2, y / 2, z / 2);
                if m == AIR || sprites::marker(m) {
                    air[b] = true;
                } else if block_leaf(m) {
                    count[b] += 1;
                    if mat[b] == AIR {
                        mat[b] = m;
                    }
                } else if !rounds(m) {
                    hard[b] = true;
                }
            }
        }
    }
    let leaf = |b: usize| count[b] >= BLOCK_MIN;
    // Offset of the cell grid's lattice point l (its cells l-1 and l along
    // each axis) in quarter voxels: surface nets, each crossing edge giving
    // the point where the count passes the threshold, kept to the edge's
    // middle half; cell centres are 4 quarters from the lattice point.
    let corner = |l: [usize; 3]| -> [i32; 3] {
        let cell = |i: usize, j: usize, k: usize| bidx(l[0] - 1 + i, l[1] - 1 + j, l[2] - 1 + k);
        let mut any_hard = false;
        for i in 0..8 {
            any_hard |= hard[cell(i & 1, i >> 1 & 1, i >> 2 & 1)];
        }
        if any_hard {
            return [0; 3];
        }
        let cross = |a: usize, b: usize| -> f32 {
            let (ca, cb) = (count[a] as f32, count[b] as f32);
            let t = (BLOCK_MIN as f32 - 0.5 - ca) / (cb - ca);
            (-4.0 + 8.0 * t).clamp(-2.0, 2.0)
        };
        let (mut sum, mut n) = ([0f32; 3], 0.0);
        for a in 0..2 {
            for b in 0..2 {
                let (ha, hb) = (8.0 * a as f32 - 4.0, 8.0 * b as f32 - 4.0);
                if leaf(cell(0, a, b)) != leaf(cell(1, a, b)) {
                    sum[0] += cross(cell(0, a, b), cell(1, a, b));
                    sum[1] += ha;
                    sum[2] += hb;
                    n += 1.0;
                }
                if leaf(cell(a, 0, b)) != leaf(cell(a, 1, b)) {
                    sum[0] += ha;
                    sum[1] += cross(cell(a, 0, b), cell(a, 1, b));
                    sum[2] += hb;
                    n += 1.0;
                }
                if leaf(cell(a, b, 0)) != leaf(cell(a, b, 1)) {
                    sum[0] += ha;
                    sum[1] += hb;
                    sum[2] += cross(cell(a, b, 0), cell(a, b, 1));
                    n += 1.0;
                }
            }
        }
        if n == 0.0 {
            return [0; 3];
        }
        sum.map(|s| ((s / n).round() as i32).clamp(-7, 7))
    };
    // The cell's outward normal from the counts one cell either way.
    let normal = |c: [usize; 3]| -> Option<[f32; 3]> {
        let mut g = [0f32; 3];
        for a in 0..3 {
            let (b, d) = ((a + 1) % 3, (a + 2) % 3);
            let mut s = 0i32;
            for j in 0..3 {
                for k in 0..3 {
                    let mut hi = c;
                    let mut lo = c;
                    hi[a] += 1;
                    lo[a] -= 1;
                    hi[b] = c[b] + j - 1;
                    lo[b] = c[b] + j - 1;
                    hi[d] = c[d] + k - 1;
                    lo[d] = c[d] + k - 1;
                    s += count[bidx(hi[0], hi[1], hi[2])] as i32 - count[bidx(lo[0], lo[1], lo[2])] as i32;
                }
            }
            g[a] = -(s as f32);
        }
        unit3(g)
    };
    let (m0, my0) = (M / 2, MY / 2);
    for cy in my0..my0 + 16 {
        for cz in m0..m0 + 16 {
            for cx in m0..m0 + 16 {
                let b = bidx(cx, cy, cz);
                if !leaf(b) {
                    continue;
                }
                let c = [cx, cy, cz];
                let mut lit = None;
                for (d, &(na, sign, ua, va)) in DIRS.iter().enumerate() {
                    let mut o = c;
                    o[na] = (c[na] as isize + sign as isize) as usize;
                    let nb = bidx(o[0], o[1], o[2]);
                    // Beside more leaves, or a cell the voxel mesher draws all
                    // of (no air, and no stray leaves of its own): hidden.
                    if leaf(nb) || !(air[nb] || count[nb] > 0) {
                        continue;
                    }
                    let mut pos = [0u32; 3];
                    for a in 0..3 {
                        let off = if a == 1 { MY } else { M };
                        pos[a] = (2 * c[a] - off) as u32;
                    }
                    if sign > 0 {
                        pos[na] += 1;
                    }
                    let mut offs = [[0i32; 3]; 4];
                    for (k, ok) in offs.iter_mut().enumerate() {
                        let mut l = c;
                        if sign > 0 {
                            l[na] += 1;
                        }
                        l[ua] += (k == 1 || k == 2) as usize;
                        l[va] += (k >= 2) as usize;
                        *ok = corner(l);
                    }
                    let (w2, w3) = pack_round(&offs);
                    let n = *lit.get_or_insert_with(|| normal(c));
                    let w0 = quad::word0(pos, 1, 1, d, true) | 1 << quad::BLOCK_BIT | n.map_or(0, quad::normal_bits);
                    out.quads.push([w0, mat[b] as u32 | 0xff << 8, w2, w3]);
                }
            }
        }
    }
}

#[cfg(test)]
fn mesh_impl(v: &[u8; PAD3], smooth: bool, merge: bool, out: &mut Mesh) {
    mesh_impl_h(v, smooth, merge, None, None, true, out)
}

/// Mesh `v`; with `leaves`, also the leaf blocks of that array (whose leaves
/// `v` holds as air). `slabs`: draw slab materials half height, which is
/// terrain only -- a model's voxels are palette slots, not materials.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn mesh_impl_h(v: &[u8; PAD3], smooth: bool, merge: bool, colh: Option<&[f32]>, leaves: Option<&[u8; PAD3]>, slabs: bool, out: &mut Mesh) {
    mesh_impl_t(v, smooth, merge, colh, leaves, slabs, &[], &[], out)
}

/// Nanoseconds in each part of `mesh_impl_t`, summed over all threads: lean
/// stems, corner setup, the six face sweeps, leaf blocks, regrouping. For the
/// headless stats (`gen_stage_ms`).
pub static MESH_NANOS: [std::sync::atomic::AtomicU64; 5] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

fn mesh_nanos(stage: usize, since: std::time::Instant) -> std::time::Instant {
    MESH_NANOS[stage].fetch_add(since.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    std::time::Instant::now()
}

#[allow(clippy::too_many_arguments)]
fn mesh_impl_t(v: &[u8; PAD3], smooth: bool, merge: bool, colh: Option<&[f32]>, leaves: Option<&[u8; PAD3]>, slabs: bool, threads: &[Thread], own: &[bool], out: &mut Mesh) {
    let tm = std::time::Instant::now();
    out.quads.clear();
    out.sprites.clear();
    out.sprite_fill = 0;
    out.sprite_grass = 0;
    out.sprite_patch = 0;
    out.sprite_stalk = 0;
    out.dir_counts = [0; 6];
    out.tails = [[0; 6]; LEANS.len() + 1];
    out.rtails = [[0; 6]; LEANS.len() + 1];
    // Leaning stems are drawn as slanted tubes (mesh/lean.rs): their voxels
    // make no faces of their own and hide none beside them.
    // A model's threads (mesh_model_t) are drawn though it is not smoothed.
    let lean = if tubes_on() && (smooth || !threads.is_empty()) {
        lean::leaning(v, threads, own, !smooth)
    } else {
        None
    };
    let tm = mesh_nanos(0, tm);
    let leaning = |i: isize| lean.as_ref().is_some_and(|l| l.flags[i as usize]);
    let solid = |i: isize| v[i as usize] != AIR && !leaning(i);
    let mut corners = if smooth { Some(Corners::new(v, colh)) } else { None };
    let tm = mesh_nanos(1, tm);
    // Mask entries: 0 = no face; otherwise material | ao << 8 | 1 << 16 (present)
    // | 1 << 17 when the corners differ or the face is sloped or blended, which
    // blocks merging. `extra` holds word2.
    let mut mask = [0u32; 32 * 32];
    let mut extra = [0u32; 32 * 32];
    let mut extra3 = [0u32; 32 * 32];
    // word0 bits of a rounded quad's smoothed normal (quad::normal_bits).
    let mut extran = [0u32; 32 * 32];
    let origin = OFF[0] * STRIDE[0] + OFF[1] * STRIDE[1] + OFF[2] * STRIDE[2];
    for (d, &(na, sign, ua, va)) in DIRS.iter().enumerate() {
        let start = out.quads.len();
        let (sn, su, sv) = (STRIDE[na], STRIDE[ua], STRIDE[va]);
        let toward = sign as isize * sn;
        for slice in 0..32isize {
            let mut any = false;
            let base = origin + slice * sn;
            for vv in 0..32isize {
                let row = base + vv * sv;
                for uu in 0..32isize {
                    let i = row + uu * su;
                    let m = v[i as usize];
                    let mi = (vv * 32 + uu) as usize;
                    if m == AIR || leaning(i) {
                        mask[mi] = 0;
                        continue;
                    }
                    let q = i + toward;
                    // A slab fills only the lower half of its voxel, so it
                    // hides a face only where it reaches the whole of it: the
                    // top of what is under it, and the side of a slab beside
                    // it. Whatever stands on a slab, and the side of a full
                    // voxel next to one, still shows.
                    let half = slabs && crate::world::matlib::slab(m);
                    if solid(q) && !(slabs && crate::world::matlib::slab(v[q as usize]) && (d == 3 || (d != 2 && !half))) {
                        mask[mi] = 0;
                        continue;
                    }
                    // Ambient occlusion from the eight voxels around q in the face plane.
                    let (um, up, vm, vp) = (solid(q - su), solid(q + su), solid(q - sv), solid(q + sv));
                    let mut a = [
                        vertex_ao(um, vm, solid(q - su - sv)),
                        vertex_ao(up, vm, solid(q + su - sv)),
                        vertex_ao(up, vp, solid(q + su + sv)),
                        vertex_ao(um, vp, solid(q - su + sv)),
                    ];
                    let mut word2 = 0u32;
                    let mut word3 = 0u32;
                    let mut rounded = false;
                    let mut gone = false;
                    // A thin stem: bevel the edges that run along it.
                    let tube = if !half && corners.is_some() && tubes(m) && tubes_on() { tube_face(v, i, d) } else { None };
                    if half {
                        // A slab: the top face and the top edge of each side
                        // face come down half a voxel (two quarters). Its
                        // quads are 1x1, so the corners at the top are the
                        // ones the v (or u) axis moves up.
                        if d != 3 {
                            for k in 0..4 {
                                let up = d == 2 || (ua == 1 && (k == 1 || k == 2)) || (va == 1 && k >= 2);
                                if up {
                                    word2 |= ((-2i32 & 15) as u32) << (4 * k);
                                }
                            }
                        }
                    } else if let Some((face, chamfers)) = tube {
                        (word2, word3) = pack_round(&face);
                        rounded = true;
                        let mut pos = [0u32; 3];
                        pos[na] = slice as u32;
                        pos[ua] = uu as u32;
                        pos[va] = vv as u32;
                        for c in chamfers.iter().flatten() {
                            let (w2, w3) = pack_round(c);
                            out.quads.push([quad::word0(pos, 1, 1, d, true), m as u32 | 0xff << 8, w2, w3]);
                        }
                    } else if let Some(cs) = corners.as_mut() {
                        let mut p = [0isize; 3];
                        p[na] = slice + OFF[na];
                        p[ua] = uu + OFF[ua];
                        p[va] = vv + OFF[va];
                        let xc = (p[0], p[2]);
                        let mut yc = xc;
                        if na == 0 {
                            yc.0 += sign as isize;
                        } else if na == 2 {
                            yc.1 += sign as isize;
                        }
                        let ground_top = na == 1 && smooths(m);
                        // Bottom and top height (quarters) of the two vertical edges
                        // of a side face; a riser whose edges both close up is dropped.
                        let along = if ua == 1 { va } else { ua };
                        let mut edge = [[0i32; 2]; 2];
                        let mut round = [[0i32; 3]; 4];
                        let mut any_round = false;
                        for k in 0..4 {
                            let mut c = p;
                            if sign > 0 {
                                c[na] += 1;
                            }
                            c[ua] += (k == 1 || k == 2) as isize;
                            c[va] += (k >= 2) as isize;
                            if let Some(r) = cs.round(c[0], c[1], c[2]) {
                                round[k] = r;
                                any_round = true;
                                continue;
                            }
                            let o = if d == 3 {
                                0
                            } else if na == 1 {
                                if ground_top {
                                    cs.get(c[0], c[2], c[1])
                                } else {
                                    0
                                }
                            } else {
                                cs.edge(c[0], c[2], c[1], xc, yc)
                            };
                            round[k] = [0, 4 * o, 0];
                            word2 |= ((o & 15) as u32) << (4 * k);
                            if na != 1 {
                                edge[(c[along] - p[along]) as usize][(c[1] > p[1]) as usize] = c[1] as i32 * 4 + o;
                            }
                        }
                        if any_round && crate::world::matlib::steps_only(m) && round.iter().all(|r| *r == [0; 3]) {
                            // Bevelled material in the middle of a flat face:
                            // no corner moved, so it stays a plain quad and can
                            // merge (a bevelled arena wall would otherwise be
                            // one quad a voxel). Not for material faceted all
                            // over (a dome, a granite whaleback): a flat
                            // stretch there drawn plain is lit unlike the
                            // facets round it and shows as a stepped patch.
                        } else if any_round {
                            (word2, word3) = pack_round(&round);
                            rounded = true;
                            extran[mi] = 0;
                            if crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE) && !crate::world::matlib::facets(m) {
                                if let Some(t) = cs.trees.as_ref() {
                                    extran[mi] = smoothed(&t.dens, i).map_or(0, quad::normal_bits);
                                }
                            }
                            if crate::world::matlib::facets(m) {
                                // As on sloped ground: the steps that darkened
                                // these corners are facets now.
                                a = [3; 4];
                            }
                        } else {
                            gone = na != 1 && edge.iter().all(|e| e[1] <= e[0]);
                            if word2 != 0 {
                                // The steps that darkened these corners are slopes now.
                                a = [3; 4];
                            }
                            if ground_top {
                                word2 |= cs.blend(p[0], p[2], p[1] + 1, m);
                            }
                        }
                    }
                    if gone {
                        mask[mi] = 0;
                        continue;
                    }
                    let ao = a[0] | a[1] << 2 | a[2] << 4 | a[3] << 6;
                    let uniform = a[0] == a[1] && a[1] == a[2] && a[2] == a[3];
                    mask[mi] = m as u32 | ao << 8 | 1 << 16 | ((!uniform || word2 != 0 || rounded) as u32) << 17 | (rounded as u32) << 18;
                    extra[mi] = word2;
                    extra3[mi] = word3;
                    any = true;
                }
            }
            if !any {
                continue;
            }
            // Greedy merge of equal, mergeable cells.
            for vv in 0..32usize {
                let mut uu = 0usize;
                while uu < 32 {
                    let k = mask[vv * 32 + uu];
                    if k == 0 {
                        uu += 1;
                        continue;
                    }
                    let mut w = 1;
                    let mut h = 1;
                    // Sloped and blended faces stay 1x1: the shader grows merged
                    // (flat) quads slightly to seal T-junctions, which only works
                    // for quads that lie in one plane.
                    if merge && k & (1 << 17) == 0 {
                        while uu + w < 32 && mask[vv * 32 + uu + w] == k {
                            w += 1;
                        }
                        'grow: while vv + h < 32 {
                            for i in 0..w {
                                if mask[(vv + h) * 32 + uu + i] != k {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }
                    }
                    for j in 0..h {
                        for i in 0..w {
                            mask[(vv + j) * 32 + uu + i] = 0;
                        }
                    }
                    let mut pos = [0u32; 3];
                    pos[na] = slice as u32;
                    pos[ua] = uu as u32;
                    pos[va] = vv as u32;
                    let mut w0 = quad::word0(pos, w as u32, h as u32, d, k >> 18 & 1 == 1);
                    if k >> 18 & 1 == 1 {
                        w0 |= extran[vv * 32 + uu];
                    }
                    out.quads.push([w0, k & 0xffff, extra[vv * 32 + uu], extra3[vv * 32 + uu]]);
                    uu += w;
                }
            }
        }
        let _ = start;
    }
    let tm = mesh_nanos(2, tm);
    if let Some(l) = leaves {
        leaf_blocks(l, out);
    }
    if let Some(l) = &lean {
        out.quads.extend_from_slice(&l.quads);
    }
    let tm = mesh_nanos(3, tm);
    // Regroup (see Mesh::tails), so the renderer can draw just the part of a
    // group that might show.
    // A stable counting sort by (group, kind, lean class).
    const NC: usize = LEANS.len() + 2;
    let mut keys = pooled(|p| &mut p.keys, 0, 0u16);
    let mut start = [0u32; 6 * 3 * NC];
    for q in &out.quads {
        let (g, s, c) = classify(q);
        let k = (g * 3 + s as usize) * NC + c as usize;
        keys.push(k as u16);
        start[k] += 1;
        out.dir_counts[g] += 1;
        let tails = if s == 1 { &mut out.rtails } else { &mut out.tails };
        for (i, t) in tails.iter_mut().enumerate() {
            t[g] += (s != 0 && c as usize > i) as u32;
        }
    }
    let mut at = 0;
    for s in start.iter_mut() {
        (*s, at) = (at, at + *s);
    }
    let src = std::mem::replace(&mut out.quads, pooled(|p| &mut p.quads, keys.len(), [0; 4]));
    for (q, &k) in src.iter().zip(&keys) {
        out.quads[start[k as usize] as usize] = *q;
        start[k as usize] += 1;
    }
    POOL.with(|p| {
        let mut p = p.borrow_mut();
        p.keys = keys;
        p.quads = src;
    });
    mesh_nanos(4, tm);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::gen::pidx;
    use crate::world::material::{GRASS, LEAVES, PLANKS, SAND};

    fn empty() -> Box<[u8; PAD3]> {
        vec![0u8; PAD3].into_boxed_slice().try_into().unwrap()
    }

    /// `bevel_steps`: the edges and corners of a box, and the foot of a wall,
    /// stay square; the lips of a one-voxel staircase are bevelled.
    #[test]
    fn clean_corners_of_boxes_stay_square() {
        let clean = |v: &[u8; PAD3], x, y, z| clean_axes(v, x, y, z) == [true; 3];
        let mut v = empty();
        let fill = |v: &mut [u8; PAD3], x: std::ops::Range<isize>, y: std::ops::Range<isize>, z: std::ops::Range<isize>| {
            for yy in y.clone() {
                for zz in z.clone() {
                    for xx in x.clone() {
                        v[idx(xx, yy, zz)] = PLANKS;
                    }
                }
            }
        };
        // Floor up to y = 10, a 4x4x4 box on it at x, z 6..10, and a wall
        // at x 20..24 running the whole width.
        fill(&mut v, 0..PAD as isize, 0..10, 0..PAD as isize);
        fill(&mut v, 6..10, 10..14, 6..10);
        fill(&mut v, 20..24, 10..20, 0..PAD as isize);
        assert!(clean(&v, 10, 14, 10), "top corner of the box");
        assert!(clean(&v, 10, 12, 10), "vertical edge of the box");
        assert!(clean(&v, 8, 14, 6), "top edge of the box");
        assert!(clean(&v, 6, 10, 8), "foot of the box");
        assert!(clean(&v, 20, 10, 16), "foot of the wall");
        assert!(clean(&v, 14, 10, 16), "middle of the floor");
        // A staircase down in +z at x 12..16: step k is solid up to 14 - k
        // over z 12 + k.
        let mut s = empty();
        fill(&mut s, 0..PAD as isize, 0..10, 0..PAD as isize);
        for k in 0..4 {
            fill(&mut s, 12..16, 10..14 - k, 12 + k..13 + k);
        }
        assert!(!clean(&s, 14, 14, 13), "lip of the top step");
        assert!(!clean(&s, 14, 13, 13), "inside corner of a step");
        assert!(!clean(&s, 14, 13, 14), "lip of the next step");
        // An upright pillar whose outline steps one voxel at a time in plan:
        // its corners move across, never up or down.
        let mut p = empty();
        for (x, z) in [(6, 6), (7, 6), (6, 7)] {
            fill(&mut p, x..x + 1, 10..20, z..z + 1);
        }
        assert_eq!(clean_axes(&p, 7, 15, 7), [false, true, false], "stepped outline of a pillar");
    }

    /// Padded index of chunk voxel (x, y, z); the margins can be reached with
    /// negative or >31 coordinates.
    fn c(x: i32, y: i32, z: i32) -> usize {
        pidx((x + M as i32) as usize, (y + MY as i32) as usize, (z + M as i32) as usize)
    }

    /// Quads facing direction d (by their own direction, not their group).
    fn of_dir(m: &Mesh, d: u32) -> Vec<Quad> {
        m.quads.iter().copied().filter(|q| quad::dir(q) as u32 == d).collect()
    }

    fn top_quads(m: &Mesh) -> Vec<Quad> {
        of_dir(m, 2)
    }

    fn offsets(q: Quad) -> [i32; 4] {
        [0, 1, 2, 3].map(|k| (((q[2] >> (4 * k)) & 15) << 28) as i32 >> 28)
    }

    fn sloped(q: Quad) -> bool {
        q[2] & 0xffff != 0
    }

    /// Ground whose height in voxels is `h(x)` for chunk column x, the same along z.
    fn ground(h: impl Fn(i32) -> i32) -> Box<[u8; PAD3]> {
        let mut v = empty();
        for z in -(M as i32)..32 + M as i32 {
            for x in -(M as i32)..32 + M as i32 {
                for y in -(MY as i32)..h(x).min(32 + MY as i32) {
                    v[c(x, y, z)] = GRASS;
                }
            }
        }
        v
    }

    /// Height in quarter voxels of the +Y surface at corner lattice x (any z).
    fn profile(m: &Mesh) -> std::collections::BTreeMap<u32, i32> {
        let mut out = std::collections::BTreeMap::new();
        for q in top_quads(m) {
            let [x, y, _] = quad::pos(&q);
            let y = y as i32;
            let o = offsets(q);
            // +Y: u = z, v = x, so corners 0 and 1 sit at x, corners 2 and 3 at x + 1.
            out.insert(x, (y + 1) * 4 + o[0]);
            out.insert(x + 1, (y + 1) * 4 + o[3]);
        }
        out
    }

    /// A slab is drawn half a voxel high: its top comes down two quarters,
    /// the top edge of each side with it, and the bottom stays put. A full
    /// voxel beside it keeps the side face the slab only half covers, and
    /// one on top of it keeps its underside.
    #[test]
    fn a_slab_is_drawn_half_height() {
        let (slab, _table) = crate::physics::tests::slab_material();
        let mut v = empty();
        v[c(5, 5, 5)] = slab;
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        assert_eq!(m.quads.len(), 6);
        for q in &m.quads {
            let d = quad::dir(q);
            let low = quad_corners(q).iter().map(|c| c[1]).fold(f32::MAX, f32::min);
            let high = quad_corners(q).iter().map(|c| c[1]).fold(f32::MIN, f32::max);
            let want = match d {
                2 => (5.5, 5.5),
                3 => (5.0, 5.0),
                _ => (5.0, 5.5),
            };
            assert_eq!((low, high), want, "face {d} spans {low} to {high}");
        }
        // A full voxel to the east and one on top.
        v[c(6, 5, 5)] = 1;
        v[c(5, 6, 5)] = 1;
        mesh_chunk(&v, false, &mut m);
        let has = |at: [u32; 3], d: usize| m.quads.iter().any(|q| quad::dir(q) == d && quad::pos(q) == at);
        assert!(has([6, 5, 5], 1), "the full voxel's west face is hidden by half a voxel of slab");
        assert!(has([5, 6, 5], 3), "the voxel on the slab lost its underside");
        assert!(!has([5, 5, 5], 0), "the slab's east face shows against a full voxel");
        assert!(!has([5, 5, 5], 2), "the slab's top shows under a full voxel");
        // Two slabs side by side hide each other.
        v[c(6, 5, 5)] = slab;
        v[c(5, 6, 5)] = 0;
        mesh_chunk(&v, false, &mut m);
        assert!(!m.quads.iter().any(|q| quad::pos(q) == [5, 5, 5] && quad::dir(q) == 0));
    }

    #[test]
    fn single_voxel_has_six_faces() {
        let mut v = empty();
        v[c(5, 5, 5)] = 1;
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        assert_eq!(m.quads.len(), 6);
        assert_eq!(m.dir_counts, [1; 6]);
    }

    #[test]
    fn flat_floor_merges_into_one_quad_per_side() {
        let mut v = empty();
        for z in 0..32 {
            for x in 0..32 {
                v[c(x, 3, z)] = 1;
            }
        }
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        // Top and bottom each one quad; the slab's rim touches the padding (air),
        // so each of the four sides is one 32x1 strip.
        assert_eq!((of_dir(&m, 2).len() as u32), 1);
        assert_eq!((of_dir(&m, 3).len() as u32), 1);
        assert_eq!(m.quads.len(), 6);
    }

    #[test]
    fn padding_hides_faces_against_neighbours() {
        let v = ground(|_| 4);
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        assert_eq!(m.quads.len(), 1, "only the top should show");
        mesh_chunk(&v, true, &mut m);
        assert_eq!(m.quads.len(), 1, "flat ground stays one quad");
    }

    #[test]
    fn gentle_staircase_becomes_an_even_slope() {
        // One voxel up every four columns.
        let v = ground(|x| 4 + x.div_euclid(4));
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        assert!((of_dir(&m, 1).len() as u32) > 0, "blocky: risers");
        mesh_chunk(&v, true, &mut m);
        assert_eq!((of_dir(&m, 1).len() as u32), 0, "smooth: every riser closes up");
        let p = profile(&m);
        for x in 2..30 {
            assert_eq!(p[&(x + 1)] - p[&x], 1, "a quarter voxel per column at x = {x}: {p:?}");
        }
    }

    #[test]
    fn steep_staircase_becomes_an_even_slope() {
        // One voxel up every column.
        let v = ground(|x| 4 + x);
        let mut m = Mesh::default();
        mesh_chunk(&v, true, &mut m);
        assert_eq!((of_dir(&m, 1).len() as u32), 0);
        let p = profile(&m);
        for x in 3..28 {
            assert_eq!(p[&(x + 1)] - p[&x], 4, "x = {x}: {p:?}");
        }
    }

    #[test]
    fn two_voxels_a_column_becomes_an_even_slope() {
        let v = ground(|x| 4 + 2 * x);
        let mut m = Mesh::default();
        mesh_chunk(&v, true, &mut m);
        assert_eq!((of_dir(&m, 1).len() as u32), 0, "{:?}", m.dir_counts);
        let p = profile(&m);
        for x in 3..12 {
            assert_eq!(p[&(x + 1)] - p[&x], 8, "x = {x}: {p:?}");
        }
    }

    #[test]
    fn diagonal_staircase_closes_up() {
        let mut v = empty();
        for z in -(M as i32)..32 + M as i32 {
            for x in -(M as i32)..32 + M as i32 {
                for y in -(MY as i32)..(4 + x + z).min(32 + MY as i32) {
                    v[c(x, y, z)] = GRASS;
                }
            }
        }
        let mut m = Mesh::default();
        mesh_chunk(&v, true, &mut m);
        assert_eq!(((of_dir(&m, 1).len() as u32), (of_dir(&m, 5).len() as u32)), (0, 0), "{:?}", m.dir_counts);
    }

    #[test]
    fn grass_and_sand_blend_across_their_border() {
        let mut v = ground(|_| 4);
        for z in -(M as i32)..32 + M as i32 {
            for x in 16..32 + M as i32 {
                v[c(x, 3, z)] = SAND;
            }
        }
        let mut m = Mesh::default();
        mesh_chunk(&v, false, &mut m);
        assert!(m.quads.iter().all(|q| q[2] == 0), "blocky: no blends");
        mesh_chunk(&v, true, &mut m);
        for q in top_quads(&m) {
            let [x, _, z] = quad::pos(&q);
            if z != 10 || (x != 15 && x != 16) {
                continue;
            }
            let (own, other, w) = (q[1] & 255, q[2] >> 16 & 255, q[2] >> 24);
            // Corners k = 0, 1 sit at x, corners 2, 3 at x + 1; the border is at x = 16.
            if x == 15 {
                assert_eq!((own, other), (GRASS as u32, SAND as u32));
                assert_eq!(w, 2 << 4 | 2 << 6, "half sand at the border corners, none behind");
            } else {
                assert_eq!((own, other), (SAND as u32, GRASS as u32));
                assert_eq!(w, 2 | 2 << 2);
            }
        }
    }

    #[test]
    fn feet_stand_on_the_drawn_slope() {
        // The gentle staircase: the drawn ground rises a quarter voxel per column.
        let v = ground(|x| 4 + x.div_euclid(4));
        let at = |x: isize, y: isize, z: isize| v[idx(x, y, z)];
        let base = ground_height(at, (10 + M) as f32, (10 + M) as f32, (4 + 2 + MY) as f32).unwrap();
        for i in 1..12 {
            let x = 10.0 + i as f32 * 0.5;
            let h = ground_height(at, x + M as f32, (10 + M) as f32, base + i as f32 * 0.125).unwrap();
            assert!((h - base - i as f32 * 0.125).abs() < 1e-4, "x = {x}: {h} vs {base}");
        }
    }

    #[test]
    fn two_voxel_risers_close_up() {
        // Two voxels up every four columns.
        let v = ground(|x| 4 + 2 * x.div_euclid(4));
        let mut m = Mesh::default();
        mesh_chunk(&v, true, &mut m);
        assert_eq!((of_dir(&m, 1).len() as u32), 0, "{:?}", m.dir_counts);
    }

    /// Corners in sixteenths of a voxel, chunk units, in winding order.
    fn corners(q: Quad) -> [[i32; 3]; 4] {
        let p = quad::pos(&q).map(|c| c as i32);
        let d = quad::dir(&q);
        let (w, h) = quad::size(&q);
        let (w, h) = (w as i32, h as i32);
        let rounded = quad::rounded(&q);
        let o = offsets(q);
        let packed = quad::round_bits(&q);
        let sx = |v: u64, n: u32| ((v << (64 - n)) as i64 >> (64 - n)) as i32;
        let (na, sign, ua, va) = DIRS[d];
        [0, 1, 2, 3].map(|k| {
            let mut c = p;
            if sign > 0 {
                c[na] += 1;
            }
            c[ua] += if k == 1 || k == 2 { w } else { 0 };
            c[va] += if k >= 2 { h } else { 0 };
            if rounded {
                let b = packed >> (16 * k);
                let s = quad::round_scale(&q) as i32;
                [c[0] * 16 + sx(b & 31, 5) * s, c[1] * 16 + sx(b >> 5 & 63, 6) * s, c[2] * 16 + sx(b >> 11 & 31, 5) * s]
            } else {
                [c[0] * 16, c[1] * 16 + 4 * o[k], c[2] * 16]
            }
        })
    }

    /// Edges of the surface that nothing closes, away from the chunk's sides.
    /// Non-vertical edges must meet a reversed twin exactly; along each vertical
    /// lattice line the upward and downward edges must cover the same heights.
    fn open_edges(m: &Mesh) -> Vec<String> {
        use std::collections::HashMap;
        let mut flat: HashMap<([i32; 3], [i32; 3]), i32> = HashMap::new();
        let mut lines: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
        for &q in &m.quads {
            let c = corners(q);
            for k in 0..4 {
                let (a, b) = (c[k], c[(k + 1) % 4]);
                if a == b {
                    continue;
                }
                // Keep away from the chunk's sides, where the surface carries on
                // in the neighbour; vertical edges are clipped rather than
                // dropped, since the two sides of one may be split differently.
                let (lo, hi) = (24, 232);
                let across = |p: [i32; 3]| (12..252).contains(&p[0]) && (12..252).contains(&p[2]);
                if !across(a) || !across(b) {
                    continue;
                }
                if a[0] == b[0] && a[2] == b[2] {
                    let (ya, yb) = (a[1].clamp(lo, hi), b[1].clamp(lo, hi));
                    if ya != yb {
                        lines.entry((a[0], a[2])).or_default().push((ya, yb));
                    }
                } else if (lo..=hi).contains(&a[1]) && (lo..=hi).contains(&b[1]) {
                    *flat.entry((a, b)).or_default() += 1;
                    *flat.entry((b, a)).or_default() -= 1;
                }
            }
        }
        let mut out = Vec::new();
        for ((a, b), n) in &flat {
            if *n > 0 {
                out.push(format!("edge {a:?} -> {b:?} x{n}"));
            }
        }
        for ((x, z), segs) in &lines {
            let mut ev: Vec<(i32, i32)> = Vec::new();
            for &(a, b) in segs {
                let s = if b > a { 1 } else { -1 };
                ev.push((a.min(b), s));
                ev.push((a.max(b), -s));
            }
            ev.sort();
            let mut sum = 0;
            let mut i = 0;
            while i < ev.len() {
                let y = ev[i].0;
                while i < ev.len() && ev[i].0 == y {
                    sum += ev[i].1;
                    i += 1;
                }
                if sum != 0 && i < ev.len() {
                    out.push(format!("line ({x}, {z}) at y {}..{} net {sum}", y, ev[i].0));
                }
            }
        }
        out.sort();
        out
    }

    /// Ground meshed with the generator's true column heights (even gentle
    /// slopes) must stay closed too, and gentle ground must come out smoother:
    /// fewer distinct corner steps along a slope than with the 4x4 mean.
    #[test]
    fn generated_ground_with_heights_is_closed() {
        use crate::world::gen::{ColKey, GenScratch, Generator};
        use crate::world::spec::WorldSpec;
        let g = Generator::new(WorldSpec::default());
        let mut m = Mesh::default();
        let mut bad = Vec::new();
        let mut checked = 0;
        let mut scratch = GenScratch::default();
        let mut keys: Vec<ColKey> = Vec::new();
        for x in [5, 12, 20, 30, 38, 46, 55] {
            for z in [12, 30, 50] {
                keys.push(ColKey { lod: 0, x, z });
            }
        }
        keys.push(ColKey { lod: 1, x: 10, z: 20 });
        keys.push(ColKey { lod: 2, x: 6, z: 9 });
        let mut exact = 0;
        for key in keys {
            let (x, z) = (key.x, key.z);
            g.gen_column_with(key, None, &mut scratch, |cy, v, colh| {
                mesh_impl_h(v, true, false, Some(colh), None, true, &mut m);
                checked += m.quads.len();
                for e in open_edges(&m) {
                    bad.push(format!("chunk ({x}, {cy}, {z}): {e}"));
                }
                // The generator's heights agree with the full-detail lookup.
                if key.lod == 0 {
                    for (pz, px) in [(5usize, 7usize), (20, 13), (30, 31)] {
                        let wx = x * 32 - M as i32 + px as i32;
                        let wz = z * 32 - M as i32 + pz as i32;
                        let want = g.ground_vox(wx, wz) - (cy * 32 - MY as i32) as f32;
                        assert!((colh[pz * PAD + px] - want).abs() < 1e-3, "height mismatch {} vs {want}", colh[pz * PAD + px]);
                        exact += 1;
                    }
                }
            });
        }
        assert!(exact > 0);
        assert!(checked > 10000, "too little ground checked: {checked}");
        assert!(bad.is_empty(), "{} open edges, first: {:#?}", bad.len(), &bad[..bad.len().min(12)]);
    }

    /// A long gentle ramp (1 in 16) with its true heights: the ground height
    /// along it rises steadily instead of in quarter-voxel jumps.
    #[test]
    fn gentle_ramp_with_heights_is_even() {
        let truth = |x: f32| 6.0 + x / 16.0;
        let v = ground(|x| truth(x as f32).round() as i32);
        let mut colh = vec![0f32; PAD * PAD];
        for z in 0..PAD {
            for x in 0..PAD {
                colh[z * PAD + x] = truth(x as f32 - M as f32) + MY as f32;
            }
        }
        let at = |x: isize, y: isize, z: isize| {
            if x < 0 || y < 0 || z < 0 || x >= PAD as isize || y >= PADY as isize || z >= PAD as isize {
                AIR
            } else {
                v[idx(x, y, z)]
            }
        };
        let ch = |x: isize, z: isize| {
            if x < 0 || z < 0 || x >= PAD as isize || z >= PAD as isize {
                None
            } else {
                Some(colh[z as usize * PAD + x as usize])
            }
        };
        let zp = (PAD / 2) as f32;
        let mut prev: Option<f32> = None;
        let mut worst: f32 = 0.0;
        for k in 0..100 {
            let xp = (M + 3) as f32 + k as f32 * 0.25;
            let y = truth(xp - M as f32).round() + MY as f32;
            let h = ground_height_h(at, ch, xp, zp, y).unwrap();
            if let Some(p) = prev {
                assert!(h >= p - 1e-4, "ramp goes down at {xp}: {p} -> {h}");
                worst = worst.max(h - p);
            }
            prev = Some(h);
        }
        // 1 in 16 over a quarter voxel is 1/64 voxel a step.
        assert!(worst < 0.03, "steepest step along the ramp {worst} voxels");
    }

    #[test]
    fn generated_ground_is_closed() {
        use crate::world::gen::{ColKey, Generator};
        use crate::world::spec::WorldSpec;
        let g = Generator::new(WorldSpec::default());
        let mut m = Mesh::default();
        let mut bad = Vec::new();
        let mut checked = 0;
        let mut keys: Vec<ColKey> = Vec::new();
        for x in [5, 12, 20, 30, 38, 46, 55] {
            for z in [12, 30, 50] {
                keys.push(ColKey { lod: 0, x, z });
            }
        }
        keys.push(ColKey { lod: 1, x: 10, z: 20 });
        keys.push(ColKey { lod: 2, x: 6, z: 9 });
        for key in keys {
            let (x, z) = (key.x, key.z);
            let col = g.gen_column(key, None);
            for (cy, v) in &col.chunks {
                mesh_impl(v, true, false, &mut m);
                checked += m.quads.len();
                for e in open_edges(&m) {
                    bad.push(format!("chunk ({x}, {cy}, {z}): {e}"));
                }
            }
        }
        assert!(checked > 10000, "too little ground checked: {checked}");
        assert!(bad.is_empty(), "{} open edges, first: {:#?}", bad.len(), &bad[..bad.len().min(12)]);
    }

    /// A ball of leaves in the air meshes as leaf blocks into a closed
    /// surface with about a quarter of the quads.
    #[test]
    fn leaf_blocks_close_and_cost_less() {
        let mut v = [AIR; PAD3];
        let centre = [17.3f32, 19.6, 15.8];
        for y in 0..PADY {
            for z in 0..PAD {
                for x in 0..PAD {
                    let d = [x as f32 + 0.5 - centre[0], y as f32 + 0.5 - centre[1], z as f32 + 0.5 - centre[2]];
                    if d[0] * d[0] + d[1] * d[1] * 1.6 + d[2] * d[2] < 64.0 {
                        v[idx(x as isize, y as isize, z as isize)] = LEAVES;
                    }
                }
            }
        }
        let mut fine = Mesh::default();
        mesh_chunk_h(&v, true, None, false, &mut fine);
        let mut blocks = Mesh::default();
        mesh_chunk_h(&v, true, None, true, &mut blocks);
        assert!(blocks.quads.iter().all(quad::block), "only leaf blocks expected");
        assert!(blocks.quads.iter().all(|q| q[0] >> quad::NORMAL_BIT & 1 == 1), "every leaf block face lit smooth");
        assert!(blocks.quads.len() * 3 < fine.quads.len(), "{} blocks vs {} fine", blocks.quads.len(), fine.quads.len());
        let open = open_edges(&blocks);
        assert!(open.is_empty(), "{} open edges, first: {:?}", open.len(), &open[..open.len().min(6)]);
    }

    /// Two trees' crowns sharing cells: where a leaf cell meets cells that
    /// hold some of its leaves and some other rounded voxels (an autumn
    /// maple's non-foliage leaves, bark), the leaf surface stays closed. It
    /// used to be hidden toward any cell with no air, which left the needles
    /// in those cells drawn by nobody: a hole through the crown.
    #[test]
    fn a_crown_stays_closed_where_another_plant_shares_its_cells() {
        let mut v = [AIR; PAD3];
        let centre = [17.3f32, 19.6, 15.8];
        for y in 0..PADY {
            for z in 0..PAD {
                for x in 0..PAD {
                    let d = [x as f32 + 0.5 - centre[0], y as f32 + 0.5 - centre[1], z as f32 + 0.5 - centre[2]];
                    if d[0] * d[0] + d[1] * d[1] * 1.6 + d[2] * d[2] < 64.0 {
                        // A slab of cells through the middle, half leaves and
                        // half bark: short of a leaf block, and no air.
                        let slab = (18..20).contains(&y);
                        v[idx(x as isize, y as isize, z as isize)] = if slab && x % 2 == 0 { crate::world::material::BARK } else { LEAVES };
                    }
                }
            }
        }
        let mut m = Mesh::default();
        mesh_chunk_h(&v, true, None, true, &mut m);
        let blocks = Mesh {
            quads: m.quads.iter().copied().filter(quad::block).collect(),
            ..Default::default()
        };
        let open = open_edges(&blocks);
        assert!(open.is_empty(), "{} open leaf-block edges round the shared cells, first: {:?}", open.len(), &open[..open.len().min(6)]);
    }

    #[test]
    fn plank_floor_pins_the_ground_beside_it() {
        // A one-voxel step at x = 16, with one plank voxel replacing the top of
        // the low ground right against the step.
        let mut v = ground(|x| if x < 16 { 4 } else { 5 });
        v[c(15, 3, 10)] = PLANKS;
        let mut m = Mesh::default();
        mesh_chunk(&v, true, &mut m);
        for q in top_quads(&m) {
            let [x, _, z] = quad::pos(&q);
            if q[1] & 255 == PLANKS as u32 {
                assert!(!sloped(q), "plank tops stay flat");
            }
            // Grass at (15, 9) shares the plank's corner (16, 10), which the step
            // would otherwise pull up; +Y corner 2 is (x + 1, z + 1).
            if (x, z) == (15, 9) {
                assert!(sloped(q), "the grass still slopes");
                assert_eq!(offsets(q)[2], 0, "but not at the plank's corner");
            }
        }
    }
}

#[cfg(test)]
mod mesh_bench {
    use super::*;
    use crate::world::gen::{ColKey, Generator};
    use crate::world::spec::WorldSpec;
    /// Meshing time with and without slopes, single-threaded:
    /// `cargo test --release mesh_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench() {
        let g = Generator::new(WorldSpec::default());
        let mut chunks = Vec::new();
        for x in (0..64).step_by(8) {
            for z in (0..64).step_by(8) {
                chunks.extend(g.gen_column(ColKey { lod: 0, x, z }, None).chunks.into_iter().map(|c| c.1));
            }
        }
        let mut m = Mesh::default();
        for smooth in [false, true, false, true] {
            let mut best = f64::MAX;
            for _ in 0..5 {
                let t = std::time::Instant::now();
                for v in &chunks {
                    mesh_chunk(v, smooth, &mut m);
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e3 / chunks.len() as f64);
            }
            println!("smooth {smooth}: {best:.3} ms/chunk over {} chunks", chunks.len());
        }
    }

    /// Meshing a real world's columns with their leaning stems, one thread,
    /// and a hash of every quad, so a faster lean.rs can be checked to draw
    /// the same stems:
    /// `WORLD=saurischia AT=330,600 cargo test --release lean_world -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn lean_world() {
        use crate::world::gen::GenScratch;
        let name = std::env::var("WORLD").unwrap_or_else(|_| "saurischia".into());
        let at: Vec<f32> = std::env::var("AT").unwrap_or_else(|_| "330,600".into()).split(',').filter_map(|s| s.parse().ok()).collect();
        let _lock = crate::world::matlib::table_lock();
        crate::worldload::select(&name);
        let loaded = crate::worldload::load(&name).expect("world loads");
        let g = Generator::with_world(loaded.world.clone());
        let (cx, cz) = ((at[0] * 4.0 / 32.0).floor() as i32, (at[1] * 4.0 / 32.0).floor() as i32);
        let mut scratch = GenScratch::default();
        let mut chunks = Vec::new();
        for dz in -3..=3 {
            for dx in -3..=3 {
                let key = ColKey { lod: 0, x: cx + dx, z: cz + dz };
                let trees = g.tube_trees(key);
                g.gen_column_with(key, None, &mut scratch, |cy, padded, colh| {
                    let (threads, own) = g.chunk_threads(&trees, key, cy);
                    chunks.push((Box::new(*padded), colh.to_vec(), threads, own));
                });
            }
        }
        let mut m = Mesh::default();
        let (mut best, mut lean, mut hash, mut quads) = (f64::MAX, 0u64, 0u64, 0usize);
        for _ in 0..3 {
            MESH_NANOS[0].store(0, std::sync::atomic::Ordering::Relaxed);
            let t = std::time::Instant::now();
            (hash, quads) = (0xcbf2_9ce4_8422_2325, 0);
            for (v, colh, threads, own) in &chunks {
                mesh_chunk_t(v, true, Some(colh), true, threads, own, &mut m);
                quads += m.quads.len();
                for q in &m.quads {
                    for &w in q {
                        hash = (hash ^ w as u64).wrapping_mul(0x100_0000_01b3);
                    }
                }
            }
            let ms = t.elapsed().as_secs_f64() * 1e3;
            if ms < best {
                best = ms;
                lean = MESH_NANOS[0].load(std::sync::atomic::Ordering::Relaxed);
            }
        }
        let cols = 49.0;
        println!(
            "{name} at {at:?}: meshing {:.2} ms a column, lean {:.2} ms; {quads} quads, hash {hash:016x}",
            best / cols,
            lean as f64 / 1e6 / cols
        );
    }
}
