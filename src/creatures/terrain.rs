//! The ground and water as creatures see them. Every mover asks these
//! instead of reading the voxel store or the world's fields itself, so a
//! question like "is this wet" has one answer per meaning.
//!
//! Two sources: the voxel store (full detail, only where loaded; trees and
//! shrubs included) and, in a chronicle world, the world's fields (bare
//! ground height, rivers, coast). The legacy island has only the store.

use crate::world::fields::World as Land;
use crate::world::matlib::{self, flag};
use crate::world::store::Store;
use glam::Vec3;

pub struct Terrain<'a> {
    pub store: &'a Store,
    pub land: Option<&'a Land>,
    pub sea: f32,
}

/// Top of the voxel ground at (x, z) below `from_y`, metres, and its
/// material, if that column is loaded at full detail.
pub fn voxel_ground(store: &Store, x: f32, z: f32, from_y: f32) -> Option<(f32, u8)> {
    let vx = (x * 4.0).floor() as i32;
    let vz = (z * 4.0).floor() as i32;
    let start = ((from_y * 4.0) as i32).min(store.top_vox() - 1);
    if !store.loaded(vx, start.max(0), vz) {
        return None;
    }
    store.top_below(vx, vz, start).map(|(vy, m)| ((vy + 1) as f32 / 4.0, m))
}

/// A voxel of a material flagged as water (`river_water`, which the old
/// painted rivers used; nothing draws it any more). Walkers keep out.
pub fn water_voxel(m: u8) -> bool {
    matlib::has(m, flag::WATER)
}

/// Damage a second for a body whose feet are at `foot`: the worst of the
/// voxels it is standing in or on.
///
/// Molten slag is solid, so a body stands on top of a pool rather than in
/// it, and the voxel under the feet is the one that matters. The ankles are
/// checked too, for a pool a body has waded into or a ledge it is half off,
/// and the four corners of the collision box as well as the middle, so
/// standing with one foot over the edge of a pit still counts.
pub fn hurt_at(store: &Store, foot: Vec3, half_w: f32) -> f32 {
    worst_under(store, foot, half_w, matlib::hurt)
}

/// The same, with the damage of a material looked up by `of`. Split out so
/// the shape of the search can be tested without installing a material
/// table, which is global and which other tests assume is bare.
fn worst_under(store: &Store, foot: Vec3, half_w: f32, of: impl Fn(u8) -> f32) -> f32 {
    let mut worst = 0.0f32;
    let r = half_w * 0.8;
    for (dx, dz) in [(0.0, 0.0), (-r, -r), (r, -r), (-r, r), (r, r)] {
        let (vx, vz) = (((foot.x + dx) * 4.0).floor() as i32, ((foot.z + dz) * 4.0).floor() as i32);
        // The voxel the feet are in, and the one just under them.
        for dy in [0.05f32, -0.1] {
            let vy = ((foot.y + dy) * 4.0).floor() as i32;
            if let Some(m) = store.get(vx, vy, vz) {
                worst = worst.max(of(m));
            }
        }
    }
    worst
}

impl Terrain<'_> {
    /// See `voxel_ground`.
    pub fn ground(&self, x: f32, z: f32, from_y: f32) -> Option<(f32, u8)> {
        voxel_ground(self.store, x, z, from_y)
    }

    /// The bare ground height from the world's fields, if there is a world.
    pub fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.land.map(|l| l.f.height(x, z))
    }

    /// What a flock keeps above and lands on: the highest voxel (trees
    /// included) or else the world's ground, never below the sea.
    pub fn surface(&self, x: f32, z: f32) -> Option<f32> {
        self.ground(x, z, self.store.top_m())
            .map(|g| g.0.max(self.sea))
            .or_else(|| self.land.map(|l| l.f.height(x, z).max(self.sea)))
    }

    /// Where a small bird perches: the highest voxel below `from`, standing
    /// on the leaf-cluster sprites (about 0.3 m above the voxels) on foliage.
    pub fn perch(&self, x: f32, z: f32, from: f32) -> Option<f32> {
        self.ground(x, z, from).map(|(y, m)| y + if crate::mesh::sprites::leafy(m) { 0.3 } else { 0.0 })
    }

    /// River or sea, where birds never come down. Half a metre of margin:
    /// the smoothed ground mesh is drawn up to 0.3 m below the voxel tops,
    /// so a bird just above the waterline looks wet.
    pub fn wet(&self, x: f32, z: f32) -> bool {
        self.land.is_some_and(|l| {
            let i = l.f.idx(x, z);
            l.f.water[i] == 1 || l.f.river_dist[i] < 1.5 || l.f.height(x, z) < self.sea + 0.5
        })
    }

    /// River water at (x, z): its surface and its depth over the bare
    /// ground (None off rivers and where the bank rises above it).
    pub fn inland_water(&self, x: f32, z: f32) -> Option<(f32, f32)> {
        let l = self.land?;
        let w = l.f.wet(x, z)?;
        let d = w.level - l.f.height(x, z);
        (d > 0.0).then_some((w.level, d))
    }

    /// The river's current at (x, z), metres a second (zero off rivers).
    pub fn current(&self, x: f32, z: f32) -> glam::Vec2 {
        self.land.map_or(glam::Vec2::ZERO, |l| l.f.current(x, z))
    }

    /// Open sea deep enough for a seabird to sit on.
    pub fn deep(&self, x: f32, z: f32) -> bool {
        self.land.is_some_and(|l| l.f.height(x, z) < self.sea - 0.6)
    }

    /// Height of the sea or lake bed (ten metres down without a world).
    pub fn bed(&self, x: f32, z: f32) -> f32 {
        self.land.map_or(self.sea - 10.0, |l| l.f.height(x, z))
    }

    /// Width of the world, metres (unbounded without one).
    pub fn size(&self) -> f32 {
        self.land.map_or(f32::MAX, |l| l.f.size)
    }

    /// Metres to the coast, negative out at sea, if there is a world.
    pub fn coast(&self, x: f32, z: f32) -> Option<f32> {
        self.land.map(|l| l.f.coast_dist[l.f.idx(x, z)])
    }

    /// Metres to the nearest river channel or shore, if there is a world.
    pub fn shore_dist(&self, x: f32, z: f32) -> Option<f32> {
        self.land.map(|l| shore_dist(l, x, z))
    }
}

/// Metres to the nearest river channel or shore.
pub fn shore_dist(land: &Land, x: f32, z: f32) -> f32 {
    let f = &land.f;
    let i = f.idx(x, z);
    f.river_dist[i].min(f.coast_dist[i].abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    /// Stone with a four-metre square of `hot` let into it, surface at 2 m.
    fn pit(hot: u8) -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -24..24 {
            for z in -24..24 {
                for y in 0..8 {
                    let inside = (-8..8).contains(&x) && (-8..8).contains(&z);
                    s.set(x, y, z, if inside { hot } else { STONE });
                }
            }
        }
        s
    }

    /// Standing on something that burns costs its rate, standing on stone
    /// costs nothing, and a body with one foot over the rim still burns --
    /// which is the case that matters, because nobody walks into a pit
    /// squarely and a pit you can stand on the edge of is not a hazard.
    #[test]
    fn a_foot_in_the_fire_is_enough() {
        let hot = 200u8;
        let burns = |m: u8| if m == hot { 12.0 } else { 0.0 };
        let store = pit(hot);
        assert_eq!(worst_under(&store, Vec3::new(0.0, 2.0, 0.0), 0.3, burns), 12.0, "standing on it did not hurt");
        assert_eq!(worst_under(&store, Vec3::new(5.0, 2.0, 5.0), 0.3, burns), 0.0, "stone hurt");
        assert_eq!(worst_under(&store, Vec3::new(2.1, 2.0, 0.0), 0.3, burns), 12.0, "a foot over the edge did not count");
        // Well clear of it, nothing: the search must not reach a body away.
        assert_eq!(worst_under(&store, Vec3::new(3.0, 2.0, 3.0), 0.3, burns), 0.0, "it reached too far");
    }

    /// Nothing in a bare world burns, so the ordinary case costs nothing and
    /// cannot be turned on by accident.
    #[test]
    fn plain_ground_never_burns() {
        let store = pit(STONE);
        assert_eq!(hurt_at(&store, Vec3::new(0.0, 2.0, 0.0), 0.3), 0.0);
    }
}
