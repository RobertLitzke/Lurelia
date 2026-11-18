//! Ribbons: a 3D polyline plus a kit, drawn per voxel column.
//!
//! Nothing is compiled to a voxel array, so a kilometre of fence costs only
//! its segments. Because each column asks the kit what belongs at its own
//! offset across the line, a ribbon runs at any angle and any height — the
//! quarter-turn limit on placed structures does not apply. This is the same
//! bargain `world/city.rs` makes for streets and bridge decks.

use super::kit::{Ground, Kit};
use crate::util::FxHashMap as HashMap;
use crate::world::fields::BUCKET;
use crate::world::material::AIR;
use glam::{Vec2, Vec3};

/// One straight piece of a route. `s0` is how far along the whole route its
/// start lies, so posts keep their spacing across a corner.
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: Vec3,
    pub b: Vec3,
    pub kit: u16,
    pub s0: f32,
    /// The surface is a deck in the air, not graded land.
    pub decked: bool,
}

/// A support under a deck.
#[derive(Clone, Copy, Debug)]
pub struct Pier {
    pub at: Vec2,
    pub r: f32,
    pub top: f32,
    pub mat: u8,
}

#[derive(Default)]
pub struct Ribbons {
    pub kits: Vec<Kit>,
    pub segs: Vec<Seg>,
    pub piers: Vec<Pier>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

const PIER_BIT: u32 = 1 << 31;
const ID_MASK: u32 = PIER_BIT - 1;

fn bucket(v: f32) -> i32 {
    (v / BUCKET).floor() as i32
}

fn seg_dist(a: Vec2, b: Vec2, p: Vec2) -> (f32, f32) {
    let ab = b - a;
    let l2 = ab.length_squared().max(1e-6);
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p.distance(a + ab * t), t)
}

impl Ribbons {
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty() && self.piers.is_empty()
    }

    /// Build the bucket grid. Called once the chronicle has run.
    pub fn index(&mut self) {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::default();
        let mut add = |lo: Vec2, hi: Vec2, id: u32| {
            for bz in bucket(lo.y)..=bucket(hi.y) {
                for bx in bucket(lo.x)..=bucket(hi.x) {
                    grid.entry((bx, bz)).or_default().push(id);
                }
            }
        };
        for (k, s) in self.segs.iter().enumerate() {
            let r = Vec2::splat(self.kits[s.kit as usize].reach);
            let (a, b) = (Vec2::new(s.a.x, s.a.z), Vec2::new(s.b.x, s.b.z));
            add(a.min(b) - r, a.max(b) + r, k as u32);
        }
        for (k, p) in self.piers.iter().enumerate() {
            add(p.at - Vec2::splat(p.r), p.at + Vec2::splat(p.r), PIER_BIT | k as u32);
        }
        self.grid = grid;
    }

    /// Segments and piers whose bounds may touch the box, metres.
    pub fn near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<u32> {
        let mut out = Vec::new();
        if self.grid.is_empty() {
            return out;
        }
        for bz in bucket(z0)..=bucket(z1) {
            for bx in bucket(x0)..=bucket(x1) {
                if let Some(list) = self.grid.get(&(bx, bz)) {
                    out.extend(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// How far the listed items reach: the lowest and highest in metres, and
    /// how far a `follow` kit stands above whatever ground is under it (that
    /// height is not known here, so the caller adds it to the land's own top).
    pub fn y_range(&self, items: &[u32]) -> (f32, f32, f32) {
        let (mut lo, mut hi, mut follow) = (f32::MAX, f32::MIN, 0.0f32);
        for &id in items {
            let k = (id & ID_MASK) as usize;
            if id & PIER_BIT != 0 {
                let p = &self.piers[k];
                lo = lo.min(p.top - 40.0);
                hi = hi.max(p.top + 0.5);
                continue;
            }
            let s = &self.segs[k];
            let kit = &self.kits[s.kit as usize];
            if kit.ground == Ground::Follow {
                follow = follow.max(kit.rise + 1.0);
                continue;
            }
            lo = lo.min(s.a.y.min(s.b.y) - kit.drop - 1.0);
            hi = hi.max(s.a.y.max(s.b.y) + kit.rise + kit.clear + 1.0);
        }
        (lo, hi, follow)
    }

    /// Is the point on or beside a ribbon? Plants and features keep clear.
    pub fn covers(&self, p: Vec2, margin: f32) -> bool {
        let Some(list) = self.grid.get(&(bucket(p.x), bucket(p.y))) else {
            return false;
        };
        list.iter().filter(|&&id| id & PIER_BIT == 0).any(|&id| {
            let s = &self.segs[(id & ID_MASK) as usize];
            let kit = &self.kits[s.kit as usize];
            seg_dist(Vec2::new(s.a.x, s.a.z), Vec2::new(s.b.x, s.b.z), p).0 <= kit.reach + margin
        })
    }

    /// Is the point under a deck that stands clear of the ground? Nothing
    /// grows there: a crown would come up through the planks.
    pub fn under_deck(&self, p: Vec2, margin: f32) -> bool {
        let Some(list) = self.grid.get(&(bucket(p.x), bucket(p.y))) else {
            return false;
        };
        list.iter().filter(|&&id| id & PIER_BIT == 0).any(|&id| {
            let s = &self.segs[(id & ID_MASK) as usize];
            s.decked && seg_dist(Vec2::new(s.a.x, s.a.z), Vec2::new(s.b.x, s.b.z), p).0 <= self.kits[s.kit as usize].half + margin
        })
    }
}

/// Draw the listed ribbons into a padded chunk. `ground` gives the generated
/// land height of a padded column, for the kits that follow it.
#[allow(clippy::too_many_arguments)]
pub fn draw(r: &Ribbons, list: &[u32], v: &mut [u8], o: Vec3, vsize: f32, pad: usize, pady: usize, idx: impl Fn(usize, usize, usize) -> usize, ground: impl Fn(usize, usize) -> f32) -> bool {
    let mut any = false;
    let (y0, y1) = (o.y, o.y + pady as f32 * vsize);
    // Only the items whose height range meets this chunk.
    let segs: Vec<&Seg> = list
        .iter()
        .filter(|&&id| id & PIER_BIT == 0)
        .map(|&id| &r.segs[(id & ID_MASK) as usize])
        .filter(|s| {
            let kit = &r.kits[s.kit as usize];
            kit.ground == Ground::Follow || (s.a.y.min(s.b.y) - kit.drop - 1.0 < y1 && s.a.y.max(s.b.y) + kit.rise + kit.clear + 1.0 > y0)
        })
        .collect();
    let piers: Vec<&Pier> = list
        .iter()
        .filter(|&&id| id & PIER_BIT != 0)
        .map(|&id| &r.piers[(id & ID_MASK) as usize])
        .filter(|p| p.top + 0.5 > y0)
        .collect();
    if segs.is_empty() && piers.is_empty() {
        return false;
    }
    for pz in 0..pad {
        let zm = o.z + (pz as f32 + 0.5) * vsize;
        for px in 0..pad {
            let xm = o.x + (px as f32 + 0.5) * vsize;
            let p = Vec2::new(xm, zm);
            let g = ground(px, pz);
            for s in &segs {
                let kit = &r.kits[s.kit as usize];
                let (a, b) = (Vec2::new(s.a.x, s.a.z), Vec2::new(s.b.x, s.b.z));
                let (d, t) = seg_dist(a, b, p);
                if d > kit.reach {
                    continue;
                }
                // Signed offset across the line, positive to its left.
                let dir = (b - a).normalize_or_zero();
                let across = (p - (a + (b - a) * t)).dot(Vec2::new(dir.y, -dir.x));
                let s_m = s.s0 + t * a.distance(b);
                let base = if kit.ground == Ground::Follow { g } else { s.a.y + (s.b.y - s.a.y) * t };
                let (lo, hi) = (base - kit.drop - vsize, base + kit.rise + kit.clear + vsize);
                let py0 = (((lo - o.y) / vsize).floor().max(0.0)) as usize;
                let py1 = (((hi - o.y) / vsize).ceil().clamp(0.0, pady as f32)) as usize;
                // A bored passage only exists inside rock: outside it, where
                // the line leaves the hill, it writes nothing at all.
                let bore = kit.ground == Ground::Bore;
                for py in py0..py1 {
                    let up = o.y + (py as f32 + 0.5) * vsize - base;
                    match kit.voxel(across, up, s_m, s.decked) {
                        Some(m) => {
                            let i = idx(px, py, pz);
                            if bore && v[i] == AIR {
                                continue;
                            }
                            v[i] = m;
                            any = true;
                        }
                        // Headroom: a tunnel or an underpass keeps its air.
                        None if kit.clear > 0.0 && across.abs() <= kit.half && up > 0.0 && up < kit.clear => {
                            v[idx(px, py, pz)] = AIR;
                        }
                        None => {}
                    }
                }
            }
            for pr in &piers {
                if p.distance(pr.at) > pr.r {
                    continue;
                }
                for py in 0..pady {
                    let ym = o.y + (py as f32 + 0.5) * vsize;
                    if ym < pr.top && ym > g - 1.5 {
                        v[idx(px, py, pz)] = pr.mat;
                        any = true;
                    }
                }
            }
        }
    }
    any
}
