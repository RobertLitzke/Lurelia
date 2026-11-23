//! Big animals walking through shrubs instead of round them.
//!
//! An elephant, a mammoth or a triceratops does not pick its way round a
//! knee-high bush. A walker big enough (`big`) asks for any low shrub on
//! its path to be flattened (walk.rs pushes the shrub's base to
//! `Entities::crush`) and keeps its line instead of steering away; here the
//! shrub's stems and leaves above the bottom layer are taken out of the
//! store, as the car does with what it runs over (game.rs `crush_shrubs`).
//!
//! Each flattened shrub is remembered with what it was made of, and grows
//! back after `REGROW_S` once nothing big is standing in it, so a herd
//! leaves a trail of broken brush behind it for a few minutes rather than
//! slowly clearing the map. None of it is saved (`Streamer::edit_passing`):
//! a shrub whose ground streams out and back is whole again anyway.

use crate::util::FxHashSet as HashSet;
use crate::world::material::AIR;
use crate::world::matlib;
use crate::world::store::Store;
use glam::{Vec2, Vec3};

/// The smallest walker that tramples: as tall as an elephant and as broad.
/// Bison (2.6 m) and giraffes (half a metre wide) still go round.
pub const BIG_M: f32 = 2.8;
pub const BIG_HALF_W: f32 = 0.75;
/// The tallest shrub anything tramples, metres; a big animal flattens only
/// what reaches below its back (`shoulder`). An elephant shoves through
/// brush its own height, so this is well above the car's 1.8 m.
pub const TALL_M: f32 = 2.6;
/// A plant with a trunk thicker than this is a tree, however low.
pub const TRUNK_M: f32 = 0.12;
/// Only this close to the player: further out nobody sees the brush.
pub const NEAR_M: f32 = 120.0;
/// How long a flattened shrub stays down, seconds.
pub const REGROW_S: f32 = 240.0;
/// The most flattened shrubs kept at once. Past this the oldest stand up
/// early, so a long day with a herd never strips a whole plain.
const MOST: usize = 600;
/// The most shrubs grown back in one step, so a queue of them coming due
/// together does not remesh a dozen chunks at once.
const REGROW_PER_STEP: usize = 4;

/// Whether a body this size tramples brush.
pub fn big(half_w: f32, height: f32) -> bool {
    height >= BIG_M && half_w >= BIG_HALF_W
}

/// The tallest shrub a body `height` tall walks through.
pub fn shoulder(height: f32) -> f32 {
    (height * 0.85).min(TALL_M)
}

/// The material library plant `plant`'s stems are made of.
pub fn bark_of(land: Option<&crate::world::fields::World>, plant: u16) -> Option<u8> {
    land.and_then(|w| w.lib.plants.get(plant as usize)).and_then(|d| matlib::id_of(&d.bark))
}

/// Is a voxel of `m` part of a shrub whose stems are `bark`: a stem or a
/// leaf. Rock, a wall or soil inside its crown is not.
fn own(bark: Option<u8>, m: u8) -> bool {
    m != AIR && (Some(matlib::base(m)) == bark || matlib::has(m, matlib::flag::FOLIAGE | matlib::flag::GROWS))
}

/// Is the shrub based at `base`, `height` tall with a crown `crown` across
/// and stems of `bark`, still standing? Any of its own voxels above its
/// bottom layer in a cross of columns over the middle of the crown (a shrub
/// of many stems has nothing in its very middle). True only where
/// `Trample::flatten` has something to take out.
pub fn standing(store: &Store, base: Vec3, height: f32, crown: f32, bark: Option<u8>) -> bool {
    let y0 = (base.y * 4.0).floor() as i32 + 2;
    let y1 = ((base.y + height) * 4.0).floor() as i32;
    let r = crown * 0.4;
    [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].iter().any(|&(dx, dz)| {
        let (x, z) = (((base.x + dx) * 4.0).floor() as i32, ((base.z + dz) * 4.0).floor() as i32);
        (y0..=y1).any(|y| store.get(x, y, z).is_some_and(|m| own(bark, m)))
    })
}

/// One shrub lying flattened.
struct Flat {
    key: (i32, i32, i32),
    at: Vec2,
    r: f32,
    since: f32,
    /// What was taken out of it, to put back.
    voxels: Vec<([i32; 3], u8)>,
}

#[derive(Default)]
pub struct Trample {
    flat: Vec<Flat>,
    keys: HashSet<(i32, i32, i32)>,
    /// Shrubs flattened in all, for the headless stats.
    pub count: u32,
}

fn key(p: Vec3) -> (i32, i32, i32) {
    ((p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32)
}

impl Trample {
    /// Flatten the shrubs based at `bases` (walk.rs found them on a big
    /// animal's path). `bark_of` is the material a library plant's stems
    /// are made of. Returns the store edits.
    ///
    /// A shrub recorded as down whose voxels are in the store again (its
    /// ground streamed out and back) is flattened afresh, so whatever
    /// `standing` reports standing does come down.
    pub fn flatten(&mut self, plants: &super::plants::Flora, store: &Store, bark_of: impl Fn(u16) -> Option<u8>, now: f32, bases: &[Vec3]) -> Vec<([i32; 3], u8)> {
        let mut changes = Vec::new();
        // Two animals can ask for the same shrub in one step.
        let mut asked: Vec<(i32, i32, i32)> = Vec::new();
        for &base in bases {
            let k = key(base);
            if asked.contains(&k) {
                continue;
            }
            asked.push(k);
            let Some(p) = plants.near(base.x, base.z, 0.05, None).find(|p| key(p.pos) == k) else {
                continue;
            };
            if p.height > TALL_M || p.trunk > TRUNK_M {
                continue;
            }
            let bark = bark_of(p.plant);
            let c = p.crown + 0.25;
            let lo = ((p.pos - Vec3::new(c, 0.0, c)) * 4.0).floor();
            let hi = ((p.pos + Vec3::new(c, p.height + 0.25, c)) * 4.0).floor();
            let floor = (p.pos.y * 4.0).floor() as i32;
            let mut voxels = Vec::new();
            for y in lo.y as i32..=hi.y as i32 {
                for z in lo.z as i32..=hi.z as i32 {
                    for x in lo.x as i32..=hi.x as i32 {
                        let Some(m) = store.get(x, y, z) else { continue };
                        if !own(bark, m) {
                            continue;
                        }
                        // The bottom layer of stems stays, broken off at the
                        // ground; leaves anywhere go, since a big animal
                        // walks on the broken brush, not round it.
                        if y <= floor && !crate::mesh::sprites::leafy(m) {
                            continue;
                        }
                        voxels.push(([x, y, z], m));
                    }
                }
            }
            if voxels.is_empty() {
                continue;
            }
            changes.extend(voxels.iter().map(|&(v, _)| (v, AIR)));
            if !self.keys.insert(k) {
                // Whole again in the store: this record replaces the old.
                self.flat.retain(|f| f.key != k);
            }
            self.flat.push(Flat {
                key: k,
                at: Vec2::new(p.pos.x, p.pos.z),
                r: p.crown + 0.5,
                since: now,
                voxels,
            });
            self.count += 1;
        }
        changes
    }

    /// Stand up the shrubs that have been down long enough and that nothing
    /// big is standing in (`occupied` says whether a point on the flat is
    /// inside a big walker, within a radius). Returns the store edits.
    pub fn regrow(&mut self, store: &Store, now: f32, occupied: impl Fn(Vec2, f32) -> bool) -> Vec<([i32; 3], u8)> {
        let mut changes = Vec::new();
        let crowded = self.flat.len().saturating_sub(MOST);
        let mut grown = 0;
        let mut k = 0;
        while k < self.flat.len() && grown < REGROW_PER_STEP {
            let f = &self.flat[k];
            // The oldest, past the most kept, are due whatever their age.
            let due = now - f.since >= REGROW_S || k < crowded;
            if !due {
                k += 1;
                continue;
            }
            if occupied(f.at, f.r) {
                // Something is still standing in it: try again in a while.
                self.flat[k].since = now - REGROW_S + 15.0;
                k += 1;
                continue;
            }
            let f = self.flat.remove(k);
            self.keys.remove(&f.key);
            // Only into air: whatever has been put there since (or the
            // shrub itself, streamed back in whole) stays.
            changes.extend(f.voxels.into_iter().filter(|(v, _)| store.get(v[0], v[1], v[2]) == Some(AIR)));
            grown += 1;
        }
        changes
    }

    /// Whether `regrow` has a shrub to look at now: one down long enough, or
    /// more down than are kept.
    pub fn due(&self, now: f32) -> bool {
        self.flat.len() > MOST || self.flat.iter().any(|f| now - f.since >= REGROW_S)
    }

    /// How many shrubs are lying flattened now.
    #[cfg(test)]
    pub fn lying(&self) -> usize {
        self.flat.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_big_trample() {
        assert!(big(0.8, 3.0), "an elephant");
        assert!(big(1.1, 3.0), "a triceratops");
        assert!(!big(0.63, 2.6), "a bison goes round");
        assert!(!big(0.5, 4.9), "so does a giraffe");
        assert!(!big(0.28, 1.4), "and a deer");
    }

    /// A 1.5 m bush of bark voxels on a loaded patch of ground.
    fn bush() -> (Store, super::super::plants::Flora, Vec3) {
        let mut store = Store::default();
        for cx in -1..=1 {
            for cz in -1..=1 {
                store.insert_column(cx, cz, 2, 0, Vec::new());
            }
        }
        for x in -8..8 {
            for z in -8..8 {
                store.set(x, 0, z, crate::world::material::DIRT);
            }
        }
        let base = Vec3::new(0.0, 0.25, 0.0);
        for y in 1..7 {
            for z in -2..2 {
                for x in -2..2 {
                    store.set(x, y, z, crate::world::material::BARK);
                }
            }
        }
        let flora = super::super::plants::Flora::from_refs(vec![super::super::plants::PlantRef {
            pos: base,
            plant: 0,
            height: 1.5,
            crown: 0.5,
            trunk: 0.0,
            affords: Default::default(),
        }]);
        (store, flora, base)
    }

    #[test]
    fn a_flattened_bush_keeps_its_stubs_and_grows_back_once_left_alone() {
        let (mut store, flora, base) = bush();
        let mut t = Trample::default();
        let bark = |_| Some(crate::world::material::BARK);
        let changes = t.flatten(&flora, &store, bark, 0.0, &[base]);
        assert!(!changes.is_empty());
        for &([x, y, z], m) in &changes {
            store.set(x, y, z, m);
        }
        assert!(!standing(&store, base, 1.5, 0.5, Some(crate::world::material::BARK)), "still standing after it was flattened");
        assert_eq!(store.get(0, 1, 0), Some(crate::world::material::BARK), "the stubs at the ground went too");
        // Asking again does nothing: it is down already.
        assert!(t.flatten(&flora, &store, bark, 1.0, &[base]).is_empty());
        // Not yet due, then due but stood in, then due and clear.
        assert!(t.regrow(&store, REGROW_S * 0.5, |_, _| false).is_empty());
        assert!(t.regrow(&store, REGROW_S + 1.0, |_, _| true).is_empty(), "grew up under an animal");
        assert_eq!(t.lying(), 1);
        let back = t.regrow(&store, REGROW_S + 30.0, |_, _| false);
        assert_eq!(back.len(), changes.len(), "not all of it grew back");
        assert_eq!(t.lying(), 0);
    }

    #[test]
    fn rock_in_the_crown_is_not_the_shrub_standing() {
        use crate::world::material::{BARK, STONE};
        let (mut store, flora, base) = bush();
        // A boulder up through the middle of the crown.
        for y in 1..7 {
            store.set(0, y, 0, STONE);
        }
        assert!(standing(&store, base, 1.5, 0.5, Some(BARK)));
        let mut t = Trample::default();
        for ([x, y, z], m) in t.flatten(&flora, &store, |_| Some(BARK), 0.0, &[base]) {
            store.set(x, y, z, m);
        }
        assert_eq!(store.get(0, 3, 0), Some(STONE), "the boulder went with the shrub");
        assert!(!standing(&store, base, 1.5, 0.5, Some(BARK)), "the boulder counts as the shrub");
        // Nothing of a plant with other stems is here at all.
        assert!(!standing(&store, base, 1.5, 0.5, None));
    }

    #[test]
    fn a_shrub_whole_again_in_the_store_is_flattened_again() {
        let (store, flora, base) = bush();
        let mut t = Trample::default();
        let bark = |_| Some(crate::world::material::BARK);
        // Asked for twice in one step, it is flattened once.
        let first = t.flatten(&flora, &store, bark, 0.0, &[base, base]);
        assert!(!first.is_empty());
        assert_eq!(t.count, 1);
        // The edits never reach the store, as when its ground streams out
        // and back: it is standing, so it comes down again.
        assert!(standing(&store, base, 1.5, 0.5, Some(crate::world::material::BARK)));
        let again = t.flatten(&flora, &store, bark, 1.0, &[base]);
        assert_eq!(again.len(), first.len());
        assert_eq!(t.lying(), 1);
    }

    #[test]
    fn a_big_animal_tramples_below_its_shoulder() {
        assert!((shoulder(3.0) - 2.55).abs() < 1e-4);
        assert_eq!(shoulder(6.0), TALL_M);
    }
}
