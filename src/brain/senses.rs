//! What a body may know: what it can see, and what it remembers of what it
//! saw.
//!
//! This is the layer every difficulty knob actually lives on, so it is also
//! the layer a stealth game would be built out of. A contact decays rather
//! than vanishing, which is what makes a bot push toward where you *were* —
//! and what makes a guard believable for the same reason.
//!
//! The voxel raycast is the expensive call here, so a bot looks on its own
//! schedule rather than every step (`LOOK_EVERY`).

use super::{person::Personality, Foe};
use crate::combat::shot::sees;
use crate::world::store::Store;
use glam::Vec3;

/// Seconds between sight checks. Staggered per bot by the caller, so eight
/// bots do not all raycast on the same step.
pub const LOOK_EVERY: f32 = 0.1;

/// How long a contact is worth chasing after it was last seen.
pub const REMEMBER_S: f32 = 6.0;

/// What one body knows about another.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub who: usize,
    /// Where it was when last seen, and how fast it was going.
    pub pos: Vec3,
    pub vel: Vec3,
    /// When it was last seen, and when this run of seeing it began.
    pub seen: f32,
    pub acquired: f32,
    /// True while it is in sight right now.
    pub in_sight: bool,
}

impl Contact {
    /// Seconds since it was last seen.
    pub fn stale(&self, now: f32) -> f32 {
        now - self.seen
    }

    /// Seconds this run of seeing it has lasted: what the aim settles over.
    pub fn held(&self, now: f32) -> f32 {
        now - self.acquired
    }

    /// Where it probably is now, carried on from where it was going.
    pub fn guess(&self, now: f32) -> Vec3 {
        self.pos + self.vel * (now - self.seen).min(0.6)
    }
}

/// Can a body at `from`, facing `facing`, see the point `at`?
///
/// Three tests in the order they are cheap: range, then the cone, then the
/// raycast. `peripheral` is a cosine, so a negative one lets it see behind
/// itself, which only the top difficulty does.
pub fn can_see(store: &Store, from: Vec3, facing: Vec3, p: &Personality, at: Vec3) -> bool {
    let v = at - from;
    let d = v.length();
    if d > p.sight {
        return false;
    }
    if d > 0.5 && v.normalize_or(facing).dot(facing) < p.peripheral {
        return false;
    }
    sees(store, from, at)
}

/// What a bot knows about everyone else. One contact per fighter it has met.
#[derive(Default)]
pub struct Memory {
    pub of: Vec<Contact>,
    /// Seconds until the next sight check.
    due: f32,
}

impl Memory {
    /// Stagger the first look, so bots do not raycast in lockstep.
    pub fn new(offset: f32) -> Memory {
        Memory { of: Vec::new(), due: offset }
    }

    pub fn get(&self, who: usize) -> Option<&Contact> {
        self.of.iter().find(|c| c.who == who)
    }

    /// Look around, at most every `LOOK_EVERY` seconds. Returns true if it
    /// actually looked this time.
    pub fn look(&mut self, store: &Store, from: Vec3, facing: Vec3, p: &Personality, who: usize, fighters: &[Foe], now: f32, dt: f32) -> bool {
        self.due -= dt;
        if self.due > 0.0 {
            return false;
        }
        self.due = LOOK_EVERY;
        // A teammate is somebody it sees and does not fight: not a contact.
        let mine = fighters.iter().find(|f| f.who == who).and_then(|f| f.team);
        for f in fighters {
            if f.who == who || f.friend_of(mine) {
                continue;
            }
            let seen = f.alive && can_see(store, from, facing, p, f.chest());
            match self.of.iter_mut().find(|c| c.who == f.who) {
                Some(c) => {
                    if seen {
                        // A run of seeing it is unbroken while it stays in
                        // sight; losing it and finding it again is a fresh
                        // acquisition, and so a fresh second of bad aim.
                        if !c.in_sight {
                            c.acquired = now;
                        }
                        c.pos = f.chest();
                        c.vel = f.vel;
                        c.seen = now;
                    }
                    c.in_sight = seen;
                }
                None if seen => self.of.push(Contact {
                    who: f.who,
                    pos: f.chest(),
                    vel: f.vel,
                    seen: now,
                    acquired: now,
                    in_sight: true,
                }),
                None => {}
            }
        }
        self.of.retain(|c| now - c.seen < REMEMBER_S);
        true
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    fn wall() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        // A floor, and a wall across x = 5 m.
        for x in -40..40 {
            for z in -40..40 {
                for y in 0..8 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        for z in -40..40 {
            for y in 8..40 {
                s.set(20, y, z, STONE);
            }
        }
        s
    }

    /// Range, the cone and the wall each stop it, in that order.
    #[test]
    fn three_things_stop_it_seeing_you() {
        let store = wall();
        let mut p = super::super::person::level("average").unwrap();
        p.sight = 30.0;
        let from = Vec3::new(0.0, 3.5, 0.0);
        let ahead = Vec3::NEG_Z;
        assert!(can_see(&store, from, ahead, &p, from + Vec3::new(0.0, 0.0, -10.0)), "straight ahead and clear");
        assert!(!can_see(&store, from, ahead, &p, from + Vec3::new(0.0, 0.0, -40.0)), "past its sight");
        assert!(!can_see(&store, from, ahead, &p, from + Vec3::new(0.0, 0.0, 10.0)), "behind it");
        assert!(!can_see(&store, from, Vec3::X, &p, from + Vec3::new(10.0, 0.0, 0.0)), "through the wall");
    }

    /// Losing sight and finding you again restarts the settling, and a
    /// contact is dropped once it is stale.
    #[test]
    fn finding_you_again_is_a_fresh_acquisition() {
        let store = wall();
        let p = super::super::person::level("average").unwrap();
        let mut m = Memory::new(0.0);
        let me = Vec3::new(0.0, 3.5, 0.0);
        let there = |x: f32| Foe {
            who: 1,
            foot: Vec3::new(x, 2.0, -6.0),
            vel: Vec3::ZERO,
            height: 1.75,
            alive: true,
            team: None,
        };
        assert!(m.look(&store, me, Vec3::NEG_Z, &p, 0, &[there(0.0)], 1.0, 1.0));
        assert_eq!(m.get(1).map(|c| c.acquired), Some(1.0));
        // Still in sight a second later: the same run, so it has been held
        // for a second.
        m.look(&store, me, Vec3::NEG_Z, &p, 0, &[there(0.0)], 2.0, 1.0);
        assert_eq!(m.get(1).map(|c| c.held(2.0)), Some(1.0));
        // Out of sight behind the wall, then back: a new run.
        m.look(&store, me, Vec3::NEG_Z, &p, 0, &[there(30.0)], 3.0, 1.0);
        assert_eq!(m.get(1).map(|c| c.in_sight), Some(false));
        m.look(&store, me, Vec3::NEG_Z, &p, 0, &[there(0.0)], 4.0, 1.0);
        assert_eq!(m.get(1).map(|c| c.held(4.0)), Some(0.0), "it kept its old lock");
        // Long enough out of sight and it is forgotten.
        m.look(&store, me, Vec3::NEG_Z, &p, 0, &[there(30.0)], 4.0 + REMEMBER_S + 1.0, 1.0);
        assert!(m.get(1).is_none(), "it is still remembering a contact from last week");
    }
}
