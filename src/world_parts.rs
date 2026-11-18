//! The parts of a world that move on their own: its platforms (lifts, doors)
//! and its portals (teleporters, jump pads), and the step that runs them
//! and carries whoever is on one.
//!
//! One copy of that step, for the game and for the tools that run a match
//! without a window (`tools::tourney`). The tournament used to carry its own
//! copy, written because it has no `Game`, and the two had drifted: it
//! placed the lifts against the first nav node rather than against the
//! floor under the spawn, and it built them from the world's stated
//! platforms without the generated doors, so a world with doors would have
//! paired the wrong plate with the wrong box.

use crate::modes::Activity;
use crate::platforms::Platforms;
use crate::player::Player;
use crate::portals::Portals;
use crate::world::store::Store;
use crate::Vec3;

/// How many steps the floor under the spawn is looked for while its chunk
/// streams in, before the world's own stated spawn height is used instead.
const SETTLE_TRIES: u32 = 240;

#[derive(Default)]
pub struct WorldParts {
    pub platforms: Platforms,
    pub portals: Portals,
    /// The world's own spawn point: the floor under it is the datum for
    /// everything a world states a height for, from a nav node to a lift.
    spawn: Vec3,
    /// That floor once found, and the attempts left at finding it.
    floor_base: Option<f32>,
    floor_tries: u32,
    /// A platform was placed on the last step: the grass under it has to
    /// go (`Game` clears it, since the streamer is the game's).
    pub placed: bool,
}

impl WorldParts {
    pub fn new(platforms: Platforms, portals: Portals, spawn: Vec3) -> WorldParts {
        WorldParts {
            platforms,
            portals,
            spawn,
            floor_base: None,
            floor_tries: SETTLE_TRIES,
            placed: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.platforms.is_empty() && self.portals.is_empty()
    }

    /// The floor the spawn point stands on. It wants the chunk under the
    /// spawn, which streams in over the first frames, so it is asked for
    /// until it is found and then kept; after a few seconds it settles for
    /// the world's stated spawn height, the way `Shooter::settle_nav` gives
    /// up rather than retrying forever.
    fn floor(&mut self, store: &Store) -> Option<f32> {
        if self.floor_base.is_none() && self.floor_tries > 0 {
            self.floor_tries -= 1;
            let p = self.spawn;
            let found = crate::creatures::terrain::voxel_ground(store, p.x, p.z, p.y + 1.0).map(|(y, _)| y);
            self.floor_base = found.or_else(|| (self.floor_tries == 0).then_some(p.y));
        }
        self.floor_base
    }

    /// Move the platforms and portals one step and carry whoever is on one:
    /// `player` (None until they have spawned, or when a tool leaves them
    /// out) and every body `activity` moves.
    ///
    /// Before the activity's tick and before the player's step, so that a
    /// rider steps off a floor that has already moved this frame rather than
    /// one that moves after it: the shooter unstacks its fighters right
    /// after it moves them, and a platform writing positions after that
    /// would undo the shove.
    pub fn step<A: Activity + ?Sized>(&mut self, dt: f32, store: &mut Store, mut player: Option<&mut Player>, mut activity: Option<&mut A>) {
        if self.is_empty() {
            return;
        }
        if let Some(base) = self.floor(store) {
            let st = &*store;
            let ground = |x: f32, z: f32, from: f32| crate::creatures::terrain::voxel_ground(st, x, z, from).map(|(y, _)| y);
            self.placed |= self.platforms.settle(base, ground);
            self.portals.settle(base, ground);
        }

        // Portals: send or throw whoever has stepped into one.
        if !self.portals.is_empty() {
            self.portals.step(dt);
            if let Some(p) = player.as_deref_mut() {
                self.portals.carry(p);
            }
            if let Some(a) = activity.as_deref_mut() {
                let portals = &mut self.portals;
                a.bodies(&mut |b| portals.carry(b));
            }
        }

        if self.platforms.is_empty() {
            return;
        }
        // Who is asking for one. A platform is called by the bodies on it,
        // so this runs before they move and before the platforms do.
        let mut sense: Vec<(Vec3, f32)> = Vec::new();
        if let Some(p) = player.as_deref() {
            sense.push((p.pos, p.body.half_w));
        }
        if let Some(a) = activity.as_deref_mut() {
            a.bodies(&mut |b| sense.push((b.pos, b.body.half_w)));
        }
        for (p, hw) in sense {
            self.platforms.sense(p, hw);
        }
        self.platforms.step(dt, store);
        // And carry the riders. A body moved sideways is nudged rather than
        // placed, so a platform cannot post one into a wall.
        let plats = &self.platforms;
        let store = &*store;
        let mut carry = move |b: &mut Player| {
            let by = plats.carry(b.pos, b.body.half_w, b.body.vel.y <= 0.0);
            if by == Vec3::ZERO {
                return;
            }
            // Up only as far as the body fits, and aside onto the landing
            // rather than into it (`platforms::Platforms::clear_lift`).
            b.pos = plats.clear_lift(store, b.pos, by.y, b.body.half_w, b.body.height);
            if by.x != 0.0 || by.z != 0.0 {
                crate::physics::nudge(store, &mut b.pos, Vec3::new(by.x, 0.0, by.z), b.body.half_w, b.body.height);
            }
            // Where it was is left where it was, so the body is drawn
            // between the two the way the plate under it now is: both lerp
            // by the same `alpha`, so feet and deck move together. Bringing
            // the old position along as well, which is what this did, froze
            // the rider to the step instead and was half of why a lift rode
            // like a staircase.
        };
        if let Some(p) = player {
            carry(p);
        }
        if let Some(a) = activity {
            a.bodies(&mut carry);
        }
    }
}
