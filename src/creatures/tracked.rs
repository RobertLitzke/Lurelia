//! Large animals remembered while nobody is near them.
//!
//! Everything else is made up again as the player goes: walk away from a
//! hare and come back, and another hare has been set down. That is fine
//! for a hare and wrong for a herd of brachiosaurs, which the player has
//! watched from a ridge and expects to find again. So when a large animal
//! (`Species::large`) is dropped for being far away (spawn.rs `drop_far`),
//! a record of it is kept: where it stood and which way it faced, its
//! species, age, coat and herd. Come back within `RESTORE_M` and it is put
//! back where it was, before the biomes are asked for anything new, so the
//! same herd is standing there and nothing extra turns up beside it.
//!
//! Records do not move while nobody watches. One the player has gone more
//! than `FORGET_M` from is forgotten: by then whatever is met there is new.

use super::age::{Age, Stage};
use super::herd::Kin;
use super::walk::Critter;
use super::{Coat, Creature, Look};
use crate::physics::Body;
use crate::world::store::Store;
use glam::Vec3;

/// A record is forgotten once the player is this far from it, metres.
pub const FORGET_M: f32 = 600.0;
/// A record is put back once the player is this close, metres: inside the
/// distance a large animal is dropped at (spawn.rs, 260 m out of view), so
/// one dropped is not put straight back.
pub const RESTORE_M: f32 = 220.0;
/// The most records kept; past this the oldest go.
const MOST: usize = 160;

/// One large animal as it was when it was dropped.
#[derive(Clone, Debug)]
pub struct Record {
    pub pos: Vec3,
    pub yaw: f32,
    pub heading: f32,
    pub species: usize,
    pub spawned_for: usize,
    pub model: usize,
    pub half_w: f32,
    pub height: f32,
    pub stage: Stage,
    pub seed: u32,
    pub side: f32,
    pub group: Option<u32>,
    pub morph: Option<crate::variety::Morph>,
    pub coat: Option<Coat>,
}

#[derive(Default)]
pub struct Tracked {
    pub records: Vec<Record>,
    /// Large animals put back where they were left, for the headless stats.
    pub restored: u32,
}

impl Tracked {
    /// Remember the large animal `e` before it is despawned. Anything that
    /// is not a living walker is not kept.
    pub fn remember(&mut self, world: &hecs::World, e: hecs::Entity) {
        let mut q = world.query_one::<(&Critter, &Creature, &Look, &Body, Option<&Age>, Option<&Kin>, Option<&crate::variety::Morph>, Option<&Coat>)>(e);
        let Ok((crit, c, look, body, age, kin, morph, coat)) = q.get() else {
            return;
        };
        let stage = age.map_or(Stage::Adult, |a| a.stage);
        if stage == Stage::Dead || crit.air.is_some() {
            return;
        }
        if self.records.len() >= MOST {
            self.records.remove(0);
        }
        self.records.push(Record {
            pos: look.pos,
            yaw: look.yaw,
            heading: crit.heading,
            species: c.species,
            spawned_for: c.spawned_for,
            model: look.model,
            half_w: body.half_w,
            height: body.height,
            stage,
            seed: crit.rng.state,
            side: crit.side,
            group: kin.map(|k| k.group),
            morph: morph.cloned(),
            coat: coat.copied(),
        });
    }

    /// Forget what the player has gone too far from, and put back what they
    /// have come back to: set down on the ground where it stood, which
    /// must be there to stand on (loaded voxels, or the generator's ground
    /// past full detail). Returns how many were put back.
    pub fn restore(&mut self, world: &mut hecs::World, store: &Store, player: Vec3) -> u32 {
        let flat = |p: Vec3| Vec3::new(p.x - player.x, 0.0, p.z - player.z).length();
        self.records.retain(|r| flat(r.pos) <= FORGET_M);
        let mut put = 0;
        let mut k = 0;
        while k < self.records.len() {
            let r = &self.records[k];
            if flat(r.pos) > RESTORE_M {
                k += 1;
                continue;
            }
            let ground = super::terrain::voxel_ground(store, r.pos.x, r.pos.z, r.pos.y + 3.0)
                .map(|g| g.0)
                .or_else(|| super::terrain::far_ground(store, r.pos.x, r.pos.z));
            let Some(gy) = ground else {
                // Its ground has not streamed in yet: next time.
                k += 1;
                continue;
            };
            let r = self.records.remove(k);
            let pos = Vec3::new(r.pos.x, gy + 0.01, r.pos.z);
            let look = Look::new(r.model, pos, r.yaw);
            let e = world.spawn((
                Critter::new(r.heading, 1.0, r.seed, r.side),
                Creature {
                    species: r.species,
                    spawned_for: r.spawned_for,
                },
                Body::new(r.half_w, r.height),
                look,
                Age::new(r.stage),
            ));
            if let Some(g) = r.group {
                let _ = world.insert_one(e, Kin { group: g });
            }
            // Its own coat and build, so the same animal comes back rather
            // than a fresh roll of the dice (spawn.rs `roll_morphs` leaves
            // one that has a Morph alone).
            if let Some(m) = r.morph {
                let _ = world.insert_one(e, m);
            }
            if let Some(c) = r.coat {
                let _ = world.insert_one(e, c);
            }
            put += 1;
        }
        self.restored += put;
        put
    }

    /// How many are remembered now.
    pub fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(at: Vec3) -> Record {
        Record {
            pos: at,
            yaw: 0.0,
            heading: 0.0,
            species: 0,
            spawned_for: 0,
            model: 0,
            half_w: 1.0,
            height: 3.0,
            stage: Stage::Adult,
            seed: 1,
            side: 1.0,
            group: Some(7),
            morph: None,
            coat: None,
        }
    }

    /// A store with nothing loaded and no generator: no ground anywhere.
    #[test]
    fn far_records_are_forgotten_and_near_ones_wait_for_their_ground() {
        let store = Store::default();
        let mut world = hecs::World::new();
        let mut t = Tracked::default();
        t.records.push(record(Vec3::new(700.0, 0.0, 0.0)));
        t.records.push(record(Vec3::new(400.0, 0.0, 0.0)));
        t.records.push(record(Vec3::new(100.0, 0.0, 0.0)));
        assert_eq!(t.restore(&mut world, &store, Vec3::ZERO), 0);
        // The one 700 m off is forgotten; the one in reach is kept until
        // there is ground under it, and the one between waits.
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn a_near_record_on_loaded_ground_comes_back_as_itself() {
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
        let mut world = hecs::World::new();
        let mut t = Tracked::default();
        t.records.push(record(Vec3::new(0.5, 0.0, 0.5)));
        assert_eq!(t.restore(&mut world, &store, Vec3::new(150.0, 0.0, 0.0)), 1);
        assert_eq!(t.len(), 0);
        let (kin, look) = world.query::<(&Kin, &Look)>().iter().map(|(k, l)| (k.group, l.pos)).next().expect("put back");
        assert_eq!(kin, 7, "came back without its herd");
        assert!((look.y - 0.26).abs() < 0.02, "set down at {look}");
    }
}
