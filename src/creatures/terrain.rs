//! The ground and water as creatures see them. Every mover asks these
//! instead of reading the voxel store or the world's fields itself, so a
//! question like "is this wet" has one answer per meaning.
//!
//! Two sources: the voxel store (full detail, only where loaded; trees and
//! shrubs included) and, in a chronicle world, the world's fields (bare
//! ground height, rivers, coast). The legacy island has only the store.

use crate::world::fields::World as Land;
use crate::world::matlib::{self, flag};
use crate::world::store::{Cell, Store};
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
    // A slab's top is the middle of its voxel.
    store.top_below(vx, vz, start).map(|(vy, m)| ((vy as f32 + if matlib::slab(m) { 0.5 } else { 1.0 }) / 4.0, m))
}

/// How far above a body's feet `roof_over` looks for a roof, metres.
pub const ROOF_LOOK_M: f32 = 40.0;

/// The underside of the roof over `at`, metres, if there is one: the first
/// solid voxel above head height that is not foliage.
///
/// A canopy is not a roof, so a wood stays out of doors. What this tells
/// apart is a room the land has been laid over -- an arena sunk under
/// the plain, a cave, Hemlow's Lantern Hole -- where a ray dropped from the
/// top of the world lands on the plain overhead rather than on the floor
/// the room is standing on.
pub fn roof_over(store: &Store, at: Vec3) -> Option<f32> {
    let vx = (at.x * 4.0).floor() as i32;
    let vz = (at.z * 4.0).floor() as i32;
    let lo = ((at.y + 2.0) * 4.0).floor() as i32;
    let hi = (((at.y + ROOF_LOOK_M) * 4.0) as i32).min(store.top_vox() - 1);
    (lo.max(0)..=hi).find(|&vy| store.cell(vx, vy, vz) == Cell::Solid).map(|vy| vy as f32 / 4.0)
}

/// The ground at (x, z) in the room `near` stands in: searched from just
/// under the roof over `near` when it has one, else from the top of the
/// world. None where the search starts inside solid, which under a roof is
/// a wall or the rock round the room, not somewhere to stand.
pub fn ground_near(store: &Store, x: f32, z: f32, near: Vec3) -> Option<(f32, u8)> {
    match roof_over(store, near) {
        Some(roof) => {
            let from = roof - 0.05;
            if store.cell((x * 4.0).floor() as i32, (from * 4.0).floor() as i32, (z * 4.0).floor() as i32) != Cell::Open {
                return None;
            }
            voxel_ground(store, x, z, from)
        }
        None => voxel_ground(store, x, z, store.top_m()),
    }
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
    /// See `roof_over`.
    pub fn roof(&self, at: Vec3) -> Option<f32> {
        roof_over(self.store, at)
    }

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

    /// The sea's surface at (x, z) now, with the swell on it
    /// (`Fields::sea_at`); the flat sea level without a world.
    pub fn sea_at(&self, x: f32, z: f32) -> f32 {
        self.land.map_or(self.sea, |l| l.f.sea_at(x, z))
    }

    /// Where the sea's edge stands on a beach at (x, z), `ahead` seconds
    /// from now: the sea's level with the tide, and the swash of the surf
    /// running up the sand over it (world/surf.rs `swash_at`). The flat sea
    /// without a world.
    pub fn water_edge(&self, x: f32, z: f32, ahead: f32) -> f32 {
        self.land.map_or(self.sea, |l| l.f.sea_now() + l.f.shore.swash_at(x, z, l.f.size, ahead))
    }

    /// The way down the beach to the sea at (x, z), flat and unit: across
    /// the surf's smoothed shore grid where the world has surf (the line
    /// the wave crests follow), else down the distance to the coast over a
    /// few metres. None far from any coast.
    pub fn seaward(&self, x: f32, z: f32) -> Option<Vec3> {
        let l = self.land?;
        let sh = &l.f.shore;
        let d = 3.0;
        let g = if !sh.seaward.is_empty() {
            let s = |dx: f32, dz: f32| sh.seaward_at(x + dx, z + dz, l.f.size);
            Vec3::new(s(d, 0.0) - s(-d, 0.0), 0.0, s(0.0, d) - s(0.0, -d))
        } else {
            let c = |dx: f32, dz: f32| l.f.coast_dist[l.f.idx(x + dx, z + dz)];
            Vec3::new(c(-d, 0.0) - c(d, 0.0), 0.0, c(0.0, -d) - c(0.0, d))
        };
        (g.length() > 0.5).then(|| g.normalize())
    }

    /// Height of the ground as it is drawn at (x, z): the voxel top under
    /// `from` with the smoothed mesh's slope on it (physics::drawn_ground_lift),
    /// which is what the swash is drawn over and what feet should meet.
    pub fn drawn(&self, x: f32, z: f32, from: f32) -> Option<f32> {
        let (g, _) = self.ground(x, z, from)?;
        Some(g + crate::physics::drawn_ground_lift(self.store, Vec3::new(x, g, z)))
    }

    /// River water at (x, z): its surface and its depth over the bare
    /// ground (None off rivers and where the bank rises above it).
    pub fn inland_water(&self, x: f32, z: f32) -> Option<(f32, f32)> {
        let l = self.land?;
        let w = l.f.wet_now(x, z)?;
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

    /// The bed under a swimmer at `p`: the height field's, unless `p` is
    /// under an overhang -- an undercut reef, a sea cave -- where it is the
    /// solid floor below `p` itself. The height field knows only the top of
    /// the rock, so a fish in the hollow under it kept to the band over it.
    pub fn bed_under(&self, p: Vec3) -> f32 {
        let field = self.bed(p.x, p.z);
        if p.y >= field {
            return field;
        }
        let (vx, vz) = ((p.x * 4.0).floor() as i32, (p.z * 4.0).floor() as i32);
        let top = (p.y * 4.0).floor() as i32;
        // A swimmer's bed is at most this far below it; past that, or where
        // the voxels are not loaded, the height field answers.
        let lowest = top - 160;
        for vy in (lowest.max(0)..=top).rev() {
            if !self.store.loaded(vx, vy, vz) {
                return field;
            }
            if self.store.cell(vx, vy, vz) == Cell::Solid {
                return (vy + 1) as f32 / 4.0;
            }
        }
        field
    }

    /// Is `p` inside loaded rock? Water, air and passable voxels are not,
    /// and nor is anywhere not loaded, which the height field answers for.
    pub fn in_rock(&self, p: glam::Vec3) -> bool {
        let (x, y, z) = ((p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32);
        self.store.loaded(x, y, z) && !matches!(self.store.cell(x, y, z), Cell::Open | Cell::Water | Cell::Passable)
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

    /// A fish under an overhang keeps to the floor under it, not to the
    /// band over the rock the height field remembers.
    #[test]
    fn a_swimmer_under_an_overhang_has_the_floor_under_it() {
        let mut s = pit(STONE);
        for x in -8..8 {
            for z in -8..8 {
                for y in 40..48 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        // No world, so the height field is ten metres under the sea: 10 m.
        let t = Terrain { store: &s, land: None, sea: 20.0 };
        assert_eq!(t.bed_under(Vec3::new(0.6, 5.0, 0.6)), 2.0, "under the overhang");
        assert_eq!(t.bed_under(Vec3::new(0.6, 12.5, 0.6)), 10.0, "over it");
    }

    /// A room under a roof is searched from under the roof, and a wall is
    /// not somewhere to stand; out from under it the search is from the sky.
    #[test]
    fn a_room_under_a_roof_finds_its_own_floor() {
        let mut s = pit(STONE);
        // The loaded chunks are the whole world, so the sky is their top.
        s.height_vox = 64;
        // A slab roof 10-12 m up over a 4 m square, and one wall under it.
        for x in -8..8 {
            for z in -8..8 {
                for y in 40..48 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        for z in -8..8 {
            for y in 8..40 {
                s.set(4, y, z, STONE);
            }
        }
        let inside = Vec3::new(0.1, 2.0, 0.1);
        assert_eq!(roof_over(&s, inside), Some(10.0));
        assert_eq!(voxel_ground(&s, 0.6, 0.6, s.top_m()).map(|g| g.0), Some(12.0), "from the sky it lands on the roof");
        assert_eq!(ground_near(&s, 0.6, 0.6, inside).map(|g| g.0), Some(2.0));
        assert_eq!(ground_near(&s, 1.1, 0.6, inside), None, "inside the wall");
        let outside = Vec3::new(5.0, 2.0, 5.0);
        assert_eq!(roof_over(&s, outside), None);
        assert_eq!(ground_near(&s, 5.0, 5.0, outside).map(|g| g.0), Some(2.0));
    }

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
    /// Rock is rock only where it is loaded: above it is water to a fish,
    /// and past the loaded chunks the height field has the say.
    #[test]
    fn in_rock_is_loaded_solid_only() {
        let s = pit(STONE);
        let t = Terrain { store: &s, land: None, sea: 20.0 };
        assert!(t.in_rock(Vec3::new(1.0, 1.0, 1.0)));
        assert!(!t.in_rock(Vec3::new(1.0, 3.0, 1.0)));
        assert!(!t.in_rock(Vec3::new(500.0, 1.0, 500.0)));
    }

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
