//! Plants as creatures see them: every placed library plant in a world as a
//! light reference (where it stands, which plant, how tall, how wide, what
//! it affords), bucketed on a grid so a creature can ask what is near.
//! Built once when a world's flora is loaded (`Entities::set_world`), from
//! the trees the world placed; empty on the legacy island.
//!
//! A consumer asks `Entities::plants` with `near` or `nearest`, for example
//! a browser looking for `Affordance::Browse` within 20 m, or a small bird
//! for the nearest `Perch`, and steers toward the answer.

#![allow(dead_code)]

use super::affordance::{Affordance, Affords};
use crate::util::FxHashMap as HashMap;
use crate::world::fields::World as Land;
use glam::Vec3;

/// One placed plant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlantRef {
    /// Base of the trunk, metres.
    pub pos: Vec3,
    /// Index into the library's plants.
    pub plant: u16,
    /// Top of the crown above the base, metres.
    pub height: f32,
    /// Crown radius, metres.
    pub crown: f32,
    /// Trunk radius, metres (0 for herbs).
    pub trunk: f32,
    pub affords: Affords,
}

/// Grid cell edge, metres.
const CELL: f32 = 16.0;

#[derive(Default)]
pub struct Flora {
    /// Per library plant, whether it is in flower this season (a plant that
    /// names no `flower_seasons` always is). Empty means all are: set by
    /// `set_season`.
    pub blooming: Vec<bool>,
    refs: Vec<PlantRef>,
    /// Cell -> indices into `refs`, by trunk position.
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl Flora {
    /// Every library plant the world placed, vines included.
    pub fn build(land: &Land) -> Self {
        let affords: Vec<Affords> = land.lib.plants.iter().map(|p| Affords::from_names(&p.affords)).collect();
        let refs = land
            .trees
            .iter()
            .filter(|t| (t.plant as usize) < affords.len())
            .map(|t| PlantRef {
                pos: t.pos,
                plant: t.plant,
                height: t.top,
                crown: t.radius,
                trunk: t.trunk_r,
                affords: affords[t.plant as usize],
            })
            .collect();
        Self::from_refs(refs)
    }

    pub fn from_refs(refs: Vec<PlantRef>) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::default();
        for (k, r) in refs.iter().enumerate() {
            grid.entry((cell(r.pos.x), cell(r.pos.z))).or_default().push(k as u32);
        }
        Flora { blooming: Vec::new(), refs, grid }
    }

    /// Which plants are in flower in `season` (world/season.rs `Season::name`).
    pub fn set_season(&mut self, lib: &crate::library::Library, season: &str) {
        self.blooming = lib.plants.iter().map(|p| p.flower_seasons.is_empty() || p.flower_seasons.iter().any(|s| s == season)).collect();
    }

    /// Plants whose trunk stands within `radius` of (x, z) and that afford
    /// `wants` (any plant when None), in no particular order.
    pub fn near(&self, x: f32, z: f32, radius: f32, wants: Option<Affordance>) -> impl Iterator<Item = &PlantRef> {
        let (x0, x1, z0, z1) = (cell(x - radius), cell(x + radius), cell(z - radius), cell(z + radius));
        (z0..=z1)
            .flat_map(move |cz| (x0..=x1).map(move |cx| (cx, cz)))
            .filter_map(|c| self.grid.get(&c))
            .flatten()
            .map(|&k| &self.refs[k as usize])
            .filter(move |r| wants.is_none_or(|a| r.affords.has(a)) && (r.pos.x - x).powi(2) + (r.pos.z - z).powi(2) <= radius * radius)
    }

    /// The nearest plant within `radius` of (x, z) that affords `wants`.
    pub fn nearest(&self, x: f32, z: f32, radius: f32, wants: Affordance) -> Option<&PlantRef> {
        self.near(x, z, radius, Some(wants)).min_by(|a, b| {
            let da = (a.pos.x - x).powi(2) + (a.pos.z - z).powi(2);
            let db = (b.pos.x - x).powi(2) + (b.pos.z - z).powi(2);
            da.total_cmp(&db)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_plants_by_distance_and_affordance() {
        let r = |x: f32, z: f32, a: &[&str]| PlantRef {
            pos: Vec3::new(x, 0.0, z),
            plant: 0,
            height: 3.0,
            crown: 1.0,
            trunk: 0.2,
            affords: Affords::from_names(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        };
        let f = Flora::from_refs(vec![r(5.0, 5.0, &["browse"]), r(30.0, 5.0, &["browse", "perch"]), r(-20.0, -1.0, &["perch"]), r(100.0, 100.0, &[])]);
        assert_eq!(f.near(0.0, 0.0, 10.0, None).count(), 1);
        assert_eq!(f.near(0.0, 0.0, 40.0, None).count(), 3);
        assert_eq!(f.near(0.0, 0.0, 40.0, Some(Affordance::Perch)).count(), 2);
        assert_eq!(f.nearest(0.0, 0.0, 40.0, Affordance::Perch).map(|p| p.pos.x), Some(-20.0));
        assert_eq!(f.nearest(0.0, 0.0, 40.0, Affordance::Browse).map(|p| p.pos.x), Some(5.0));
        assert!(f.nearest(0.0, 0.0, 40.0, Affordance::Nest).is_none());
    }
}
