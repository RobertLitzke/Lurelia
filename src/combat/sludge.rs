//! Globs of sludge as real matter: the goo gun's shells land as voxels
//! rather than as models stuck to a wall, and then behave like the stuff
//! they are made of.
//!
//! A blob clings to whatever face it touches, slumps a voxel at a time when
//! nothing holds it up, rests on other blobs, and sets them off when it
//! goes. That is all there is to the physics, and it is enough for the three
//! things the gun is for: mining a doorway, blocking one, and leaving a
//! stack somewhere that costs whoever walks past it next.
//!
//! Nothing here writes to the world. Every change goes onto `Combat`'s edit
//! queue and `Game` applies it at the end of the tick, which means nothing
//! laid down this tick can be read back this tick -- so each blob keeps its
//! own list of the voxels it owns, and `Sludge` keeps the set of every voxel
//! any blob has claimed. The store answers for the world and `claimed`
//! answers for the sludge; neither is asked about the other.

use super::Scene;
use crate::library::SplashDef;
use crate::world::material::{AIR, STONE};
use crate::world::store::Store;
use glam::Vec3;
use std::collections::HashSet;

/// How near a body has to come to a blob before it goes off, metres. The
/// same reach a shell stuck on a wall had (`shot::STUCK_TOUCH`).
const TOUCH: f32 = 1.1;
/// Seconds a blob takes to slump one voxel: 0.25 m at five metres a second.
/// It is a fall, not a fall with gravity -- sludge does not accelerate, it
/// oozes.
const FALL_S: f32 = 0.05;
/// The fuse a blob is given when a neighbour's burst reaches it. Short
/// enough to read as one chain going off and long enough that each link is
/// a separate bang rather than all of them inside one step -- which also
/// keeps a chain out of the call stack.
const CHAIN_S: f32 = 0.08;
/// The most blobs on the map at once. Past this the oldest is set off, so a
/// held trigger cannot bury the mesher in work or the floor in sludge.
const MOST: usize = 40;
/// The six face neighbours of a voxel. Sludge touching a face clings;
/// touching only a corner does not.
const FACES: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// What a glob is made of, by this world's own id for it. Every world
/// reaches the material (`worldload::ENGINE_MATERIALS`), so the fallback
/// applies only before a material table is installed, which is a unit test:
/// there it has to be solid and nothing more.
fn mat() -> u8 {
    crate::world::matlib::id_of("sludge").unwrap_or(STONE)
}

/// Is there something at `v` for sludge to hold on to? The store answers for
/// the world; `claimed` answers for the sludge, because a blob laid down
/// this tick is not in the store yet. A voxel outside the world counts as
/// solid: there is nothing out there to fall into.
fn solid(store: &Store, claimed: &HashSet<[i32; 3]>, v: [i32; 3]) -> bool {
    claimed.contains(&v) || store.get(v[0], v[1], v[2]).is_none_or(|m| m != AIR)
}

/// One glob on the map.
pub struct Blob {
    /// The voxels this blob owns. Nothing else owns them, and they are the
    /// only record of where it is until `Game` has applied the edits.
    own: Vec<[i32; 3]>,
    /// The middle of it, for the proximity rule and the burst.
    at: Vec3,
    /// Seconds before it goes off where it is.
    fuse: f32,
    splash: SplashDef,
    /// Who fired it, so the burst knows whose self-damage it is.
    by: usize,
    /// Seconds banked towards the next voxel of slump.
    fell: f32,
}

impl Blob {
    fn mine(&self, v: [i32; 3]) -> bool {
        self.own.contains(&v)
    }

    /// Is anything holding it up -- a wall, the floor, another blob? A blob
    /// resting against any face of anything solid clings there, which is
    /// what makes the gun stick to walls and ceilings and lets globs stack.
    /// Its own voxels do not hold it up.
    fn held(&self, store: &Store, claimed: &HashSet<[i32; 3]>) -> bool {
        self.own.iter().any(|&[x, y, z]| {
            FACES.iter().any(|&[dx, dy, dz]| {
                let v = [x + dx, y + dy, z + dz];
                !self.mine(v) && solid(store, claimed, v)
            })
        })
    }

    fn middle(&self) -> Vec3 {
        let n = self.own.len().max(1) as f32;
        let sum = self.own.iter().fold(Vec3::ZERO, |a, v| a + Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5));
        sum / n / 4.0
    }

    /// Has somebody walked into it?
    fn touched(&self, scene: &Scene) -> bool {
        scene.fighters.iter().filter(|m| m.alive).any(|man| {
            let flat = Vec3::new(man.foot.x - self.at.x, 0.0, man.foot.z - self.at.z).length();
            let within = self.at.y > man.foot.y - 0.4 && self.at.y < man.foot.y + man.height + 0.4;
            flat < TOUCH + man.half_w && within
        })
    }
}

/// Every blob on the map.
#[derive(Default)]
pub struct Sludge {
    blobs: Vec<Blob>,
    /// Blobs whose voxels went back to air last step and whose burst is
    /// owed this one. A burst has to wait a step because the voxels it is
    /// standing in are still in the store until `Game` has applied the
    /// edits, and a burst inside solid matter can see nothing: it would go
    /// off in the middle of its own glob and touch nobody. One step is
    /// 1/60 s and reads as the same bang.
    owed: Vec<(Vec3, SplashDef, usize)>,
    /// Every voxel any blob owns, so one blob can rest on another and no two
    /// claim the same voxel.
    claimed: HashSet<[i32; 3]>,
}

impl Sludge {
    /// Where each blob is and how far its burst reaches: what a body could
    /// see lying in its way (`brain::Threat`).
    pub fn mines(&self) -> impl Iterator<Item = (Vec3, f32)> + '_ {
        self.blobs.iter().map(|b| (b.at, b.splash.radius))
    }

    /// Nothing of ours is on the map.
    pub fn idle(&self) -> bool {
        self.blobs.is_empty() && self.owed.is_empty()
    }

    /// A glob has landed at `at`: lay a ball of sludge `r` metres across
    /// wherever there is air for it, and keep the list.
    ///
    /// Returns false if there was nowhere to put any of it -- a glob fired
    /// into a corner it cannot fill is a glob that simply goes off, which is
    /// what every other shell does.
    #[allow(clippy::too_many_arguments)] // a blob is where, how big, how long, how hard and whose
    pub fn land(&mut self, store: &Store, at: Vec3, r: f32, fuse: f32, splash: SplashDef, by: usize, edits: &mut Vec<([i32; 3], u8)>) -> bool {
        let m = mat();
        let c = [(at.x * 4.0).floor() as i32, (at.y * 4.0).floor() as i32, (at.z * 4.0).floor() as i32];
        let ri = (r * 4.0).ceil() as i32;
        let mut own = Vec::new();
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let v = [c[0] + dx, c[1] + dy, c[2] + dz];
                    let mid = Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5) / 4.0;
                    if (mid - at).length() > r {
                        continue;
                    }
                    // Only air, and only air nobody else has claimed: a
                    // glob fills the space it finds and never eats the wall
                    // it hit. Taking the wall out is the bore's job.
                    if self.claimed.contains(&v) || store.get(v[0], v[1], v[2]) != Some(AIR) {
                        continue;
                    }
                    own.push(v);
                }
            }
        }
        if own.is_empty() {
            return false;
        }
        for &v in &own {
            self.claimed.insert(v);
            edits.push((v, m));
        }
        let mut blob = Blob { own, at, fuse, splash, by, fell: 0.0 };
        blob.at = blob.middle();
        self.blobs.push(blob);
        // Too many: the oldest goes next step rather than this one, so the
        // cap reads as a chain going off and not as a glob vanishing.
        if self.blobs.len() > MOST {
            self.blobs[0].fuse = 0.0;
        }
        true
    }

    /// Slump what is unsupported, go off where the fuse has run out or
    /// somebody has walked in, and pass the burst on to whatever is near it.
    ///
    /// Returns one entry per burst for the caller to resolve, because the
    /// damage rules are `Combat`'s and the blobs are only matter.
    pub fn step(&mut self, scene: &Scene, dt: f32, edits: &mut Vec<([i32; 3], u8)>) -> Vec<(Vec3, SplashDef, usize)> {
        let m = mat();
        // The bursts owed from last step, now that the sludge they were
        // standing in is out of the world.
        let went_off = std::mem::take(&mut self.owed);
        // --- slump ---------------------------------------------------------
        // One blob at a time against the claims of all the others, so a
        // stack settles from the bottom up over a few steps rather than
        // sliding through itself in one.
        let mut blobs = std::mem::take(&mut self.blobs);
        for b in &mut blobs {
            b.fuse -= dt;
            // Out of the claims while it is measured, or it would find
            // itself holding itself up.
            for &v in &b.own {
                self.claimed.remove(&v);
            }
            if !b.held(scene.store, &self.claimed) {
                b.fell += dt;
                if b.fell >= FALL_S {
                    b.fell = 0.0;
                    let down: Vec<[i32; 3]> = b.own.iter().map(|&[x, y, z]| [x, y - 1, z]).collect();
                    for &v in &b.own {
                        if !down.contains(&v) {
                            edits.push((v, AIR));
                        }
                    }
                    for &v in &down {
                        if !b.own.contains(&v) {
                            edits.push((v, m));
                        }
                    }
                    b.own = down;
                    b.at = b.middle();
                }
            } else {
                b.fell = 0.0;
            }
            for &v in &b.own {
                self.claimed.insert(v);
            }
        }
        // --- what goes off -------------------------------------------------
        let mut lit: Vec<(Vec3, SplashDef)> = Vec::new();
        blobs.retain(|b| {
            if b.fuse > 0.0 && !b.touched(scene) {
                return true;
            }
            for &v in &b.own {
                self.claimed.remove(&v);
                edits.push((v, AIR));
            }
            lit.push((b.at, b.splash));
            self.owed.push((b.at, b.splash, b.by));
            false
        });
        self.blobs = blobs;
        // --- the chain -----------------------------------------------------
        // A burst that reaches a blob lights its fuse instead of setting it
        // off here, so each link is its own bang and a long chain cannot
        // run away down the stack.
        for (at, sp) in &lit {
            for b in &mut self.blobs {
                if b.fuse > CHAIN_S
                    && b.own.iter().any(|&[x, y, z]| {
                        let mid = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) / 4.0;
                        (mid - *at).length() < sp.radius
                    })
                {
                    b.fuse = CHAIN_S;
                }
            }
        }
        went_off
    }

    /// Take the lot off the map without anybody being hurt: a round is
    /// over. The voxels go back to air through the queue like any other
    /// change. Returns where each blob was and its splash, so the caller can
    /// show it popping rather than vanishing.
    pub fn clear(&mut self, edits: &mut Vec<([i32; 3], u8)>) -> Vec<(Vec3, SplashDef)> {
        self.owed.clear();
        self.claimed.clear();
        self.blobs
            .drain(..)
            .map(|b| {
                for v in b.own {
                    edits.push((v, AIR));
                }
                (b.at, b.splash)
            })
            .collect()
    }

    /// Take one fighter's blobs off the map, quietly: the man who laid them
    /// has died. Everybody else's stay where they are.
    pub fn clear_by(&mut self, by: usize, edits: &mut Vec<([i32; 3], u8)>) {
        let claimed = &mut self.claimed;
        self.blobs.retain(|b| {
            if b.by != by {
                return true;
            }
            for &v in &b.own {
                claimed.remove(&v);
                edits.push((v, AIR));
            }
            false
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::store::Voxels;

    fn open_air() -> Store {
        let mut s = Store::default();
        s.chunks.insert((0, 0, 0), Voxels::Uniform(AIR));
        s
    }

    fn splash() -> SplashDef {
        serde_json::from_value(serde_json::json!({ "damage": 50.0, "radius": 2.0 })).unwrap()
    }

    /// A death takes that fighter's goo off the map and leaves everyone
    /// else's, and the round's end takes the rest and says where it was.
    #[test]
    fn a_death_clears_only_its_own_blobs() {
        let s = open_air();
        let mut g = Sludge::default();
        let mut edits = Vec::new();
        assert!(g.land(&s, Vec3::new(2.0, 2.0, 2.0), 0.3, 9.0, splash(), 1, &mut edits));
        assert!(g.land(&s, Vec3::new(5.0, 2.0, 5.0), 0.3, 9.0, splash(), 2, &mut edits));
        edits.clear();
        g.clear_by(1, &mut edits);
        assert_eq!(g.blobs.len(), 1);
        assert_eq!(g.blobs[0].by, 2);
        assert!(!edits.is_empty() && edits.iter().all(|e| e.1 == AIR));
        assert!(edits.iter().all(|e| !g.claimed.contains(&e.0)));
        let gone = g.clear(&mut edits);
        assert_eq!(gone.len(), 1);
        assert!(g.idle());
    }
}
