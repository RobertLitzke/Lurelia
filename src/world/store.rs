//! Full-detail voxel data near the player, for physics, ray casts and edits.
//! Coarser levels of detail are never stored: they are generated, meshed and dropped.

use super::gen::{cidx, pidx, CHUNK3, M, MY, PAD, PAD3, PADY};
use super::material::{AIR, STONE};
use crate::util::FxHashMap as HashMap;

/// Materials bodies move through: thin growth such as vines, and herb
/// markers, which are drawn only as sprites.
#[inline]
pub fn passable(m: u8) -> bool {
    super::matlib::passable(m) || crate::mesh::sprites::marker(m)
}

/// How a voxel behaves for something moving through it. Every body, ride
/// and creature asks this rather than reading materials itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cell {
    /// Air, or unloaded space above the world.
    Open,
    /// Vines and herb markers: nothing collides with them.
    Passable,
    /// Leaves and needles: walkers stand on them, the bike brushes through.
    Foliage,
    /// Water drawn as voxels: a filled pool in an imported map. A swimmer
    /// passes through it (`physics::hits_wet`); everything else -- walkers,
    /// the bike, a lift -- stands on it as it always did.
    Water,
    /// Everything else, and unloaded space inside the world (so nothing
    /// falls out while it streams in).
    Solid,
}

/// A ray's first hit: the voxel, the open voxel before it, the distance in
/// metres, the face it entered through and its material. Part of the query
/// API for modes; digging uses only the voxels so far.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Hit {
    pub voxel: [i32; 3],
    pub prev: [i32; 3],
    pub t: f32,
    pub normal: glam::Vec3,
    pub mat: u8,
}

pub enum Voxels {
    Uniform(u8),
    Data(Box<[u8; CHUNK3]>),
}

impl Voxels {
    pub fn from_padded(p: &[u8; PAD3]) -> Voxels {
        let first = p[pidx(M, MY, M)];
        let uniform = (0..32).all(|y| {
            (0..32).all(|z| {
                let s = pidx(M, y + MY, z + M);
                p[s..s + 32].iter().all(|&m| m == first)
            })
        });
        if uniform {
            return Voxels::Uniform(first);
        }
        let mut data: Box<[u8; CHUNK3]> = vec![0u8; CHUNK3].into_boxed_slice().try_into().unwrap();
        for y in 0..32 {
            for z in 0..32 {
                let src = pidx(M, y + MY, z + M);
                let dst = cidx(0, y, z);
                data[dst..dst + 32].copy_from_slice(&p[src..src + 32]);
            }
        }
        Voxels::Data(data)
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        match self {
            Voxels::Uniform(m) => *m,
            Voxels::Data(d) => d[cidx(x, y, z)],
        }
    }
}

/// The generated ground height of a full-detail voxel column, in voxels.
pub type GroundFn = std::sync::Arc<dyn Fn(i32, i32) -> f32 + Send + Sync>;

#[derive(Default)]
pub struct Store {
    pub chunks: HashMap<(i32, i32, i32), Voxels>,
    /// The generator's ground heights, so feet and wheels follow gentle slopes
    /// exactly (see `mesh::ground_height_h`). Set by the streamer.
    pub ground: Option<GroundFn>,
    /// The world's height in voxels; unloaded space below it counts as solid
    /// and space above it as open. 0 means 512 (128 m), which tests use.
    pub height_vox: i32,
    /// Moving platforms, as the boxes they stand in right now
    /// (src/platforms.rs). Not in the chunks, because a chunk that changed
    /// has to be remeshed and a lift changes every frame; so this is a short
    /// list that every voxel read consults, and it is empty in a world
    /// without them. The mesher reads the chunks directly and never sees
    /// these: a platform is drawn as a model, not as terrain.
    pub platforms: Vec<Slab>,
    /// How hard this world pulls and which way (world/gravity.rs). It is
    /// here because everything that falls is already handed the store.
    pub gravity: crate::world::gravity::Gravity,
}

/// A moving platform's box, in voxel coordinates (`hi` is past the end), and
/// the material it is solid as. `src/platforms.rs` owns these and rewrites
/// the list every step.
///
/// Two boxes, because two kinds of thing ask. A voxel read (a shot, the nav
/// graph, a creature's feet) gets the cells wholly inside the plate. A body
/// box (`physics::hits`) gets the plate exactly where it is drawn, `lo_m` to
/// `hi_m` in metres: a plate in flight is between voxels nearly all the
/// time, and a rider colliding with the voxel copy of it lands up to a
/// quarter metre below the deck it can see, is set back on the deck, falls
/// again, and rides the lift as a judder.
#[derive(Clone, Copy, Debug)]
pub struct Slab {
    pub lo: [i32; 3],
    pub hi: [i32; 3],
    pub mat: u8,
    pub lo_m: glam::Vec3,
    pub hi_m: glam::Vec3,
}

impl Slab {
    #[inline]
    pub fn holds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.lo[0] && x < self.hi[0] && y >= self.lo[1] && y < self.hi[1] && z >= self.lo[2] && z < self.hi[2]
    }

    /// Does the box `lo`..`hi`, metres, overlap the plate by more than `tol`
    /// on every axis? Touching is not overlapping: a rider's feet sit
    /// exactly on the deck.
    #[inline]
    pub fn meets(&self, lo: glam::Vec3, hi: glam::Vec3, tol: f32) -> bool {
        hi.x > self.lo_m.x + tol && lo.x < self.hi_m.x - tol && hi.y > self.lo_m.y + tol && lo.y < self.hi_m.y - tol && hi.z > self.lo_m.z + tol && lo.z < self.hi_m.z - tol
    }
}

impl Store {
    /// The world's height in voxels.
    #[inline]
    pub fn top_vox(&self) -> i32 {
        if self.height_vox > 0 {
            self.height_vox
        } else {
            512
        }
    }

    /// The world's height in metres.
    pub fn top_m(&self) -> f32 {
        self.top_vox() as f32 * 0.25
    }

    /// The highest non-air voxel at or below `from_vy` in column (x, z), and its
    /// material. None if nothing is there or a chunk on the way isn't loaded.
    /// Chunks of uniform air are skipped whole.
    pub fn top_below(&self, x: i32, z: i32, from_vy: i32) -> Option<(i32, u8)> {
        let mut vy = from_vy.min(self.top_vox() - 1);
        while vy >= 0 {
            let c = self.chunks.get(&(x >> 5, vy >> 5, z >> 5))?;
            if let Voxels::Uniform(AIR) = c {
                vy = (vy & !31) - 1;
                continue;
            }
            let m = c.get((x & 31) as usize, (vy & 31) as usize, (z & 31) as usize);
            if m != AIR {
                return Some((vy, m));
            }
            vy -= 1;
        }
        None
    }

    /// Generated ground height of voxel column (x, z) in voxels, when known.
    pub fn ground_vox(&self, x: i32, z: i32) -> Option<f32> {
        self.ground.as_ref().map(|g| g(x, z))
    }

    /// The material of the platform standing over a voxel, if one is.
    #[inline]
    fn platform_at(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        self.platforms.iter().find(|s| s.holds(x, y, z)).map(|s| s.mat)
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        if !self.platforms.is_empty() {
            if let Some(m) = self.platform_at(x, y, z) {
                return Some(m);
            }
        }
        let k = (x >> 5, y >> 5, z >> 5);
        self.chunks.get(&k).map(|c| c.get((x & 31) as usize, (y & 31) as usize, (z & 31) as usize))
    }

    /// How voxel (x, y, z) behaves for movement (see `Cell`).
    #[inline]
    pub fn cell(&self, x: i32, y: i32, z: i32) -> Cell {
        if y < 0 {
            return Cell::Solid;
        }
        match self.get(x, y, z) {
            None if y < self.top_vox() => Cell::Solid,
            None | Some(AIR) => Cell::Open,
            Some(m) if passable(m) => Cell::Passable,
            Some(m) if super::matlib::has(m, super::matlib::flag::WATER) => Cell::Water,
            Some(m) if super::matlib::has(m, super::matlib::flag::FOLIAGE) => Cell::Foliage,
            Some(_) => Cell::Solid,
        }
    }

    /// Solid for a body on foot: everything but open and passable voxels.
    /// Water is in it -- a body that cannot swim stands on a pool rather
    /// than sinking into it. `physics::hits_wet` is the swimmer's answer.
    #[inline]
    pub fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        matches!(self.cell(x, y, z), Cell::Solid | Cell::Foliage | Cell::Water)
    }

    /// Is voxel (x, y, z) a slab, solid in its lower half only
    /// (`matlib::slab`)? Physics stands a body on its middle.
    #[inline]
    pub fn slab(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z).is_some_and(super::matlib::slab)
    }

    pub fn loaded(&self, x: i32, y: i32, z: i32) -> bool {
        self.chunks.contains_key(&(x >> 5, y >> 5, z >> 5))
    }

    /// Set one voxel; returns false if its chunk isn't loaded.
    pub fn set(&mut self, x: i32, y: i32, z: i32, m: u8) -> bool {
        let k = (x >> 5, y >> 5, z >> 5);
        let Some(c) = self.chunks.get_mut(&k) else {
            return false;
        };
        let (lx, ly, lz) = ((x & 31) as usize, (y & 31) as usize, (z & 31) as usize);
        if let Voxels::Uniform(u) = c {
            if *u == m {
                return true;
            }
            let d: Box<[u8; CHUNK3]> = vec![*u; CHUNK3].into_boxed_slice().try_into().unwrap();
            *c = Voxels::Data(d);
        }
        if let Voxels::Data(d) = c {
            d[cidx(lx, ly, lz)] = m;
        }
        true
    }

    /// Insert a full-detail column: generated chunks plus uniform fill above and below.
    pub fn insert_column(&mut self, cx: i32, cz: i32, count: i32, solid_below: i32, chunks: Vec<(i32, Voxels)>) {
        for cy in 0..count {
            let fill = if cy < solid_below { STONE } else { AIR };
            self.chunks.insert((cx, cy, cz), Voxels::Uniform(fill));
        }
        for (cy, v) in chunks {
            self.chunks.insert((cx, cy, cz), v);
        }
    }

    /// Chunks held as full voxel arrays and as one material (uniform).
    pub fn census(&self) -> (usize, usize) {
        let data = self.chunks.values().filter(|v| matches!(v, Voxels::Data(_))).count();
        (data, self.chunks.len() - data)
    }

    /// Remove a column's chunks, handing their voxels to `out` rather than
    /// freeing them here (see stream.rs `free_later`).
    pub fn take_column(&mut self, cx: i32, cz: i32, count: i32, out: &mut Vec<Voxels>) {
        for cy in 0..count {
            if let Some(v) = self.chunks.remove(&(cx, cy, cz)) {
                out.push(v);
            }
        }
    }

    /// Padded copy of a chunk and its neighbours' borders, for remeshing after an edit.
    /// Missing neighbours read as air.
    /// The 27 chunks around this one are looked up once and copied a row at a time.
    pub fn padded(&self, cx: i32, cy: i32, cz: i32) -> Box<[u8; PAD3]> {
        const _: () = assert!(M <= 32 && MY <= 32, "padding reaches past the next chunk");
        let mut out: Box<[u8; PAD3]> = vec![AIR; PAD3].into_boxed_slice().try_into().unwrap();
        let mut near: [[[Option<&Voxels>; 3]; 3]; 3] = [[[None; 3]; 3]; 3];
        for (dy, plane) in near.iter_mut().enumerate() {
            for (dz, row) in plane.iter_mut().enumerate() {
                for (dx, c) in row.iter_mut().enumerate() {
                    *c = self.chunks.get(&(cx + dx as i32 - 1, cy + dy as i32 - 1, cz + dz as i32 - 1));
                }
            }
        }
        // Padded x runs split into the parts that fall in the chunks at dx = -1, 0, +1.
        let runs = [(0usize, M, 32 - M), (M, 32, 0), (M + 32, M, 0)];
        for py in 0..PADY {
            let y = py as i32 - MY as i32;
            let (dy, ly) = ((y.div_euclid(32) + 1) as usize, y.rem_euclid(32) as usize);
            for pz in 0..PAD {
                let z = pz as i32 - M as i32;
                let (dz, lz) = ((z.div_euclid(32) + 1) as usize, z.rem_euclid(32) as usize);
                for (dx, &(p0, len, lx0)) in runs.iter().enumerate() {
                    let dst = pidx(p0, py, pz);
                    match near[dy][dz][dx] {
                        None => {}
                        Some(Voxels::Uniform(m)) => out[dst..dst + len].fill(*m),
                        Some(Voxels::Data(d)) => {
                            let src = cidx(lx0, ly, lz);
                            out[dst..dst + len].copy_from_slice(&d[src..src + len]);
                        }
                    }
                }
            }
        }
        out
    }

    /// Step through voxels along a ray (metres in, voxel coordinates out). Returns the
    /// first solid voxel and the empty voxel just before it.
    pub fn raycast(&self, origin: glam::Vec3, dir: glam::Vec3, max_m: f32) -> Option<([i32; 3], [i32; 3])> {
        self.ray(origin, dir, max_m, |m| m != AIR).map(|h| (h.voxel, h.prev))
    }

    /// Step along a ray (metres) to the first loaded voxel whose material
    /// `stop` accepts, within `max_m`.
    pub fn ray(&self, origin: glam::Vec3, dir: glam::Vec3, max_m: f32, stop: impl Fn(u8) -> bool) -> Option<Hit> {
        let o = origin * 4.0;
        let d = dir.normalize_or_zero();
        if d == glam::Vec3::ZERO {
            return None;
        }
        let mut cell = [o.x.floor() as i32, o.y.floor() as i32, o.z.floor() as i32];
        let step = [d.x.signum() as i32, d.y.signum() as i32, d.z.signum() as i32];
        let inv = [1.0 / d.x.abs().max(1e-9), 1.0 / d.y.abs().max(1e-9), 1.0 / d.z.abs().max(1e-9)];
        let oa = [o.x, o.y, o.z];
        let da = [d.x, d.y, d.z];
        let mut tmax = [0.0f32; 3];
        for a in 0..3 {
            let next = if da[a] > 0.0 { cell[a] as f32 + 1.0 - oa[a] } else { oa[a] - cell[a] as f32 };
            tmax[a] = next * inv[a];
        }
        let mut prev = cell;
        let max_t = max_m * 4.0;
        // The axis of the last step and the distance to it, for the hit face.
        let (mut axis, mut t) = (usize::MAX, 0.0);
        loop {
            if let Some(m) = self.get(cell[0], cell[1], cell[2]) {
                if stop(m) {
                    let mut normal = glam::Vec3::ZERO;
                    if axis < 3 {
                        normal[axis] = -step[axis] as f32;
                    }
                    return Some(Hit {
                        voxel: cell,
                        prev,
                        t: t / 4.0,
                        normal,
                        mat: m,
                    });
                }
            }
            let a = if tmax[0] < tmax[1] {
                if tmax[0] < tmax[2] {
                    0
                } else {
                    2
                }
            } else if tmax[1] < tmax[2] {
                1
            } else {
                2
            };
            if tmax[a] > max_t {
                return None;
            }
            prev = cell;
            cell[a] += step[a];
            (axis, t) = (a, tmax[a]);
            tmax[a] += inv[a];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ray reports the voxel it hit, how far away, and the face it entered.
    #[test]
    fn ray_reports_distance_and_face() {
        let mut s = Store::default();
        s.chunks.insert((0, 0, 0), Voxels::Uniform(AIR));
        s.set(10, 4, 3, STONE);
        // From x = 1 m (voxel 4) toward +x: enters voxel 10 through its -x face, 1.5 m on.
        let h = s.ray(glam::Vec3::new(1.0, 1.1, 0.8), glam::Vec3::X, 10.0, |m| m != AIR).unwrap();
        assert_eq!(h.voxel, [10, 4, 3]);
        assert_eq!(h.prev, [9, 4, 3]);
        assert!((h.t - 1.5).abs() < 1e-4, "t = {}", h.t);
        assert_eq!(h.normal, glam::Vec3::NEG_X);
        assert_eq!(h.mat, STONE);
        assert_eq!(s.cell(10, 4, 3), Cell::Solid);
        assert_eq!(s.cell(11, 4, 3), Cell::Open);
        assert_eq!(s.cell(40, 4, 3), Cell::Solid, "unloaded inside the world");
    }

    /// The row-copying padded() matches reading every voxel with get().
    #[test]
    fn padded_matches_voxel_reads() {
        let mut s = Store::default();
        let mut seed = 12345u32;
        for cy in -1..=2 {
            for cz in 2..=4 {
                for cx in -2..=0 {
                    if (cx + cy + cz) % 3 == 0 {
                        s.chunks.insert((cx, cy, cz), Voxels::Uniform(7));
                        continue;
                    }
                    let mut d: Box<[u8; CHUNK3]> = vec![0u8; CHUNK3].into_boxed_slice().try_into().unwrap();
                    for v in d.iter_mut() {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        *v = (seed >> 27) as u8;
                    }
                    s.chunks.insert((cx, cy, cz), Voxels::Data(d));
                }
            }
        }
        s.chunks.remove(&(0, 2, 4));
        let p = s.padded(-1, 1, 3);
        let (bx, by, bz) = (-32 - M as i32, 32 - MY as i32, 96 - M as i32);
        for py in 0..PADY {
            for pz in 0..PAD {
                for px in 0..PAD {
                    let want = s.get(bx + px as i32, by + py as i32, bz + pz as i32).unwrap_or(AIR);
                    assert_eq!(p[pidx(px, py, pz)], want, "at {px},{py},{pz}");
                }
            }
        }
    }

    /// top_below finds the ground under uniform air chunks, stops at an
    /// unloaded chunk, and respects the world height.
    #[test]
    fn top_below_skips_air_and_respects_height() {
        let mut s = Store { height_vox: 640, ..Store::default() };
        for cy in 0..20 {
            s.chunks.insert((0, cy, 0), Voxels::Uniform(if cy < 2 { STONE } else { AIR }));
        }
        assert_eq!(s.top_below(3, 3, 10_000), Some((63, STONE)));
        s.set(3, 300, 3, STONE);
        assert_eq!(s.top_below(3, 3, 639), Some((300, STONE)));
        assert_eq!(s.top_below(4, 3, 639), Some((63, STONE)));
        // Above 128 m but inside a 160 m world, unloaded space is solid.
        assert!(s.solid(40, 600, 40));
        assert!(!s.solid(40, 640, 40));
        s.chunks.remove(&(0, 1, 0));
        assert_eq!(s.top_below(4, 3, 639), None);
    }
}
