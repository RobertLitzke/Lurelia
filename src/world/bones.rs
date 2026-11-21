//! Bones drawn as volumes: the skeletons the `skeleton` event lays on the land.
//!
//! A skeleton is far too big for a structure (a structure is a dense voxel
//! grid, and one rib of Ribshore's titan would be twenty million voxels), and
//! it lies at any angle, where structures turn in quarter turns. So each bone
//! is a short list of tapered capsules, flattened or cut where the bone is,
//! and the chunk generator asks, per voxel, whether a point lies inside one.
//! Carves (negative capsules) hollow a skull and open its sockets.

use super::noise::{hash3, perlin3, unit};
use glam::Vec3;

/// Grid cell of the bounds index, metres.
const CELL: f32 = 16.0;

/// One tapered capsule from `a` to `b`, radius `ra` at `a` and `rb` at `b`.
#[derive(Clone, Copy, Debug)]
pub struct Bone {
    pub a: Vec3,
    pub b: Vec3,
    pub ra: f32,
    pub rb: f32,
    /// A direction across the bone squashed by `squash` (a rib is flatter
    /// than it is wide, a scapula is a plate); zero for a round bone.
    pub flat: Vec3,
    pub squash: f32,
    /// A cut end is flat instead of rounded: a break, showing the marrow.
    pub cut_a: bool,
    pub cut_b: bool,
    /// Worn flat on top at this height (m; infinity for none). The worn face
    /// is skinned in bone, worn through to the marrow in patches.
    pub top: f32,
    /// The skeleton this bone belongs to (index into `Bones::sets`).
    pub set: u16,
    /// Takes bone away instead of adding it (the inside of a skull).
    pub carve: bool,
}

/// What a skeleton's bones are made of.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoneSet {
    pub bone: u8,
    pub marrow: u8,
    /// Darker pits in the marrow (0: none).
    pub pits: u8,
    /// Lichen on bone open to the sky, over `lichen_share` of it (0: none).
    pub lichen: u8,
    pub lichen_share: f32,
    /// Barnacles from a little over the sea down to `wet_depth` under it.
    pub wet: u8,
    pub wet_depth: f32,
    /// Bone deeper under the sea than that (0: plain bone).
    pub deep: u8,
    /// Teeth and baleen (`Bone::set` points at a set carrying these as `bone`).
    /// Cortex thickness, metres: deeper than this inside a bone is marrow.
    pub shell: f32,
}

#[derive(Default)]
pub struct Bones {
    pub list: Vec<Bone>,
    pub sets: Vec<BoneSet>,
    /// Bounds of each bone (min, max).
    bounds: Vec<(Vec3, Vec3)>,
    /// The bones whose bounds reach each CELL of the world.
    grid: Vec<Vec<u32>>,
    side: usize,
}

impl Bone {
    pub fn capsule(a: Vec3, b: Vec3, ra: f32, rb: f32, set: u16) -> Self {
        Bone { a, b, ra, rb, flat: Vec3::ZERO, squash: 1.0, cut_a: false, cut_b: false, top: f32::INFINITY, set, carve: false }
    }

    fn bounds(&self) -> (Vec3, Vec3) {
        let r = self.ra.max(self.rb) + 0.5;
        let lo = self.a.min(self.b) - Vec3::splat(r);
        let mut hi = self.a.max(self.b) + Vec3::splat(r);
        hi.y = hi.y.min(self.top + 0.5);
        (lo, hi)
    }

    /// (how far inside the round surface p is, how far under the worn top,
    /// how far from a cut end), metres, or None outside. The first makes the
    /// cortex; a worn top is skinned over, a cut end shows the marrow.
    pub fn margin(&self, p: Vec3) -> Option<(f32, f32, f32)> {
        if p.y > self.top {
            return None;
        }
        let ab = self.b - self.a;
        let len2 = ab.length_squared().max(1e-6);
        let len = len2.sqrt();
        let raw = (p - self.a).dot(ab) / len2;
        if (raw < 0.0 && self.cut_a) || (raw > 1.0 && self.cut_b) {
            return None;
        }
        let t = raw.clamp(0.0, 1.0);
        let r = self.ra + (self.rb - self.ra) * t;
        let mut d = p - (self.a + ab * t);
        if self.squash != 1.0 && self.flat != Vec3::ZERO {
            let k = d.dot(self.flat);
            d += self.flat * k * (1.0 / self.squash - 1.0);
        }
        let m = r - d.length();
        if m < 0.0 {
            return None;
        }
        let mut cut = f32::INFINITY;
        if self.cut_a {
            cut = cut.min(raw * len);
        }
        if self.cut_b {
            cut = cut.min((1.0 - raw) * len);
        }
        Some((m, self.top - p.y, cut))
    }
}

impl Bones {
    pub fn add_set(&mut self, s: BoneSet) -> u16 {
        self.sets.push(s);
        (self.sets.len() - 1) as u16
    }

    /// Build the bounds index once every skeleton is in.
    pub fn index(&mut self, world: f32) {
        self.side = (world / CELL).ceil().max(1.0) as usize;
        self.grid = vec![Vec::new(); self.side * self.side];
        self.bounds = self.list.iter().map(|b| b.bounds()).collect();
        let cl = |v: f32| ((v / CELL).floor().max(0.0) as usize).min(self.side - 1);
        for (k, (lo, hi)) in self.bounds.iter().enumerate() {
            for z in cl(lo.z)..=cl(hi.z) {
                for x in cl(lo.x)..=cl(hi.x) {
                    self.grid[z * self.side + x].push(k as u32);
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Bones whose bounds reach the box x0..x1, z0..z1 (metres), and the
    /// heights they span.
    pub fn near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> (Vec<u32>, f32, f32) {
        let mut out = Vec::new();
        if self.grid.is_empty() {
            return (out, f32::MAX, f32::MIN);
        }
        let cl = |v: f32| ((v / CELL).floor().max(0.0) as usize).min(self.side - 1);
        for z in cl(z0)..=cl(z1) {
            for x in cl(x0)..=cl(x1) {
                for &k in &self.grid[z * self.side + x] {
                    let (lo, hi) = self.bounds[k as usize];
                    if hi.x >= x0 && lo.x <= x1 && hi.z >= z0 && lo.z <= z1 {
                        out.push(k);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &k in &out {
            lo = lo.min(self.bounds[k as usize].0.y);
            hi = hi.max(self.bounds[k as usize].1.y);
        }
        (out, lo, hi)
    }

    /// Inside any bone of `list` and outside every carve: (set, round margin,
    /// depth under a worn top, distance from a cut end). A carve near p keeps
    /// the cortex on its face.
    fn inside(&self, list: &[u32], p: Vec3) -> Option<(u16, f32, f32, f32)> {
        let mut best: Option<(u16, f32, f32, f32)> = None;
        let mut carve_gap = f32::MAX;
        for &k in list {
            let (lo, hi) = self.bounds[k as usize];
            if p.x < lo.x || p.y < lo.y || p.z < lo.z || p.x > hi.x || p.y > hi.y || p.z > hi.z {
                continue;
            }
            let b = &self.list[k as usize];
            if b.carve {
                match b.margin(p) {
                    Some(_) => return None,
                    None => {
                        // Distance outside the carve, roughly: grow it.
                        let mut g = *b;
                        g.ra += 1.5;
                        g.rb += 1.5;
                        if let Some((m, _, _)) = g.margin(p) {
                            carve_gap = carve_gap.min(1.5 - m);
                        }
                    }
                }
                continue;
            }
            if let Some((m, top, cut)) = b.margin(p) {
                if best.is_none_or(|(_, bm, _, _)| m > bm) {
                    best = Some((b.set, m, top, cut));
                }
            }
        }
        best.map(|(s, m, top, cut)| (s, m.min(carve_gap.max(0.0)), top, cut))
    }

    /// Draw the bones of `list` into a padded voxel block: `v` indexed by
    /// `idx(px, py, pz)`, voxel centres at `o + (p + 0.5) * vsize`. Bone
    /// replaces whatever is there, so a buried bone is found by digging.
    /// Returns whether anything was drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, list: &[u32], v: &mut [u8], o: Vec3, vsize: f32, pad: usize, pady: usize, idx: fn(usize, usize, usize) -> usize, sea: f32) -> bool {
        if list.is_empty() {
            return false;
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &k in list {
            lo = lo.min(self.bounds[k as usize].0);
            hi = hi.max(self.bounds[k as usize].1);
        }
        let span = |l: f32, h: f32, o: f32, n: usize| {
            let a = ((l - o) / vsize - 1.0).floor().max(0.0) as usize;
            let b = (((h - o) / vsize) + 1.0).ceil().clamp(0.0, n as f32) as usize;
            a..b.max(a)
        };
        let mut any = false;
        for pz in span(lo.z, hi.z, o.z, pad) {
            let z = o.z + (pz as f32 + 0.5) * vsize;
            for px in span(lo.x, hi.x, o.x, pad) {
                let x = o.x + (px as f32 + 0.5) * vsize;
                // Only the bones over this column.
                let col: Vec<u32> = list
                    .iter()
                    .copied()
                    .filter(|&k| {
                        let (l, h) = self.bounds[k as usize];
                        x >= l.x && x <= h.x && z >= l.z && z <= h.z
                    })
                    .collect();
                if !col.iter().any(|&k| !self.list[k as usize].carve) {
                    continue;
                }
                let (cl, ch) = col.iter().fold((f32::MAX, f32::MIN), |(a, b), &k| (a.min(self.bounds[k as usize].0.y), b.max(self.bounds[k as usize].1.y)));
                for py in span(cl, ch, o.y, pady) {
                    let y = o.y + (py as f32 + 0.5) * vsize;
                    let p = Vec3::new(x, y, z);
                    let Some((set, m, top, cut)) = self.inside(&col, p) else {
                        continue;
                    };
                    let s = &self.sets[set as usize];
                    let mut mat = s.bone;
                    let shell = s.shell.max(vsize * 1.2);
                    // Marrow: deep inside, and showing on a clean break; a
                    // worn top is skinned except where it is worn through.
                    let worn_through = top < shell && perlin3(x * 0.09, y * 0.2, z * 0.09, 77) > 0.3;
                    if m > shell && (top > shell || worn_through) {
                        mat = s.marrow;
                        // Honeycomb: pits in the face of a break.
                        if s.pits != 0 && cut < 1.0 && m > shell + 0.4 {
                            let c = Vec3::new(x, y, z) / 0.8;
                            let (cx, cy, cz) = (c.x.floor() as i32, c.y.floor() as i32, c.z.floor() as i32);
                            let h = hash3(cx, cy, cz, 913);
                            let j = Vec3::new(unit(h), unit(h >> 8), unit(h >> 16)) * 0.5 + 0.25;
                            if (c - Vec3::new(cx as f32, cy as f32, cz as f32) - j).length() < 0.33 {
                                mat = s.pits;
                            }
                        }
                    } else if y < sea + 0.8 && y > sea - s.wet_depth && s.wet != 0 {
                        mat = s.wet;
                    } else if y <= sea - s.wet_depth && s.deep != 0 {
                        mat = s.deep;
                    } else if s.lichen != 0 && y > sea + 2.0 {
                        // Open to the sky: nothing of this skeleton just over it.
                        let up = self.inside(&col, p + Vec3::Y * (vsize + 0.3)).is_none();
                        let n = perlin3(x * 0.07, y * 0.05, z * 0.07, 41) * 0.6 + perlin3(x * 0.4, y * 0.4, z * 0.4, 42) * 0.4;
                        if up && n > 0.45 - s.lichen_share {
                            mat = s.lichen;
                        }
                    }
                    if mat != 0 {
                        v[idx(px, py, pz)] = mat;
                        any = true;
                    }
                }
            }
        }
        any
    }
}
