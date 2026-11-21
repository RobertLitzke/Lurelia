//! The sounds that come from watching the game rather than from being told:
//! footsteps, jumps and landings, pads and teleporters, and in a match
//! everything that changes about the player and the bots from one frame to
//! the next -- hurt, killed, a pickup, a reload, a new weapon in hand, a
//! hit marker going up, a flag changing hands.
//!
//! Reading the state rather than posting from inside the shooter keeps the
//! shooter's rules free of sound. Gunfire is the exception: which fire mode
//! went off is only known at the trigger, so `Combat::fire` and `hold_beam`
//! post it themselves.

use super::{emit, Ground, Kind, Noise, Pick};
use crate::fx::hooks::{ground_under, hard};
use crate::game::Game;
use crate::world::matlib::{self, flag};
use crate::world::material::AIR;
use glam::Vec3;

/// What a material sounds like underfoot, from what the engine already
/// knows about it.
pub fn ground_of(m: u8) -> Ground {
    if m == AIR {
        Ground::Soft
    } else if matlib::has(m, flag::WATER) {
        Ground::Water
    } else if matlib::has(m, flag::FOLIAGE) {
        Ground::Leaves
    } else if matlib::has(m, flag::UNDERSTORY) {
        Ground::Grass
    } else if hard(m) {
        Ground::Stone
    } else {
        Ground::Soft
    }
}

/// Seconds between hurt sounds for one fighter: a beam's damage lands every
/// step, and sixty grunts a second is not being hurt, it is a buzz.
const HURT_GAP: f32 = 0.3;

/// Metres walked between footsteps.
const STRIDE: f32 = 1.45;
/// Bots further than this make no footsteps worth a voice.
const BOT_STEPS_M: f32 = 40.0;

/// A body's walking, for its footsteps and its jumps and landings.
#[derive(Default, Clone)]
struct Gait {
    walked: f32,
    grounded: bool,
    fall: f32,
    at: Option<Vec3>,
}

impl Gait {
    /// Step a body forward a frame, posting what it did. `gain` is how loud
    /// its feet are; `own` is the player, whose jumps and landings are at
    /// the ear.
    #[allow(clippy::too_many_arguments)] // a body's readings, each its own thing
    fn step(&mut self, store: &crate::world::store::Store, pos: Vec3, vel: Vec3, on_ground: bool, swimming: bool, dt: f32, gain: f32, own: bool) {
        let was = self.at.replace(pos);
        let place = |k: Kind, g: f32| emit(if own { Noise::ear(k, g) } else { Noise::at(k, pos, g) });
        // Carried a long way in one frame: a teleporter (or a respawn, which
        // the match watch posts for itself).
        if let Some(w) = was {
            if (pos - w).length() > 8.0 && dt < 0.2 {
                place(Kind::Teleport, 0.9);
                self.walked = 0.0;
            }
        }
        let flat = Vec3::new(vel.x, 0.0, vel.z).length();
        if on_ground && !self.grounded {
            if self.fall < -4.0 {
                place(Kind::Land(-self.fall), gain * (0.4 + (-self.fall / 12.0).min(1.2)));
            }
            self.walked = STRIDE * 0.5;
        } else if !on_ground && self.grounded && vel.y > 12.0 {
            // Thrown upward far harder than legs do it: a jump pad.
            place(Kind::Pad, 1.0);
        } else if !on_ground && self.grounded && vel.y > 2.0 {
            place(Kind::Jump, gain * 0.8);
        }
        if on_ground && flat > 0.6 {
            self.walked += flat * dt;
            if self.walked >= STRIDE {
                self.walked -= STRIDE;
                let g = if swimming { Ground::Water } else { ground_of(ground_under(store, pos + Vec3::Y * 0.05)) };
                // Running is heavier on the feet than walking.
                emit(Noise::at(Kind::Step(g), pos, gain * (0.5 + (flat / 8.0).min(0.8))));
            }
        } else if swimming && flat > 0.4 {
            self.walked += flat * dt;
            if self.walked >= STRIDE * 1.5 {
                self.walked = 0.0;
                emit(Noise::at(Kind::Step(Ground::Water), pos, gain * 0.7));
            }
        }
        if !on_ground {
            self.fall = vel.y;
        }
        self.grounded = on_ground;
    }
}

/// The player's fighter as it was last frame.
#[derive(Clone, Copy)]
struct Me {
    health: f32,
    armour: f32,
    alive: bool,
    held: usize,
    guns: usize,
    reloading: bool,
    picked: u32,
    mark: f32,
    kill: f32,
}

/// A bot as it was last frame.
#[derive(Clone)]
struct Bot {
    health: f32,
    alive: bool,
    picked: u32,
    gait: Gait,
    hurt_cd: f32,
}

#[derive(Default)]
pub struct Watch {
    me: Option<Me>,
    gait: Gait,
    bots: Vec<Bot>,
    flags: usize,
    /// The player's hurt: seconds before the next sound may play, and the
    /// damage gathered while waiting.
    hurt_cd: f32,
    hurt: f32,
}

impl Watch {
    /// Look at the world as it is this frame and post whatever changed.
    pub fn look(&mut self, g: &Game, dt: f32, ear: Vec3) {
        // The match first: a respawn is heard as one, and the feet must not
        // hear the same jump across the map as a teleporter.
        match g.activity.as_ref().and_then(|a| a.shooter()) {
            Some(sh) => self.match_(sh, &g.store, dt, ear),
            None => {
                self.me = None;
                self.bots.clear();
                self.flags = 0;
            }
        }
        let p = &g.player;
        // Riding, the ride makes the noise; flying free, nothing does.
        if g.ride.mode == crate::ride::Mode::Foot && !p.fly {
            self.gait.step(&g.store, p.pos, p.body.vel, p.body.on_ground, p.swimming, dt, 0.3, true);
        } else {
            self.gait = Gait::default();
        }
    }

    fn match_(&mut self, sh: &crate::shooter::Shooter, store: &crate::world::store::Store, dt: f32, ear: Vec3) {
        let f = &sh.me;
        let held = f.arsenal.guns.get(f.arsenal.held);
        let now = Me {
            health: f.health,
            armour: f.armour,
            alive: f.alive(),
            held: f.arsenal.held,
            guns: f.arsenal.guns.len(),
            reloading: held.is_some_and(|h| h.reloading()),
            picked: f.picked,
            mark: sh.mark,
            kill: sh.mark_kill,
        };
        if let Some(was) = self.me {
            if was.alive && !now.alive {
                emit(Noise::ear(Kind::Death, 1.0));
                self.hurt = 0.0;
            } else if !was.alive && now.alive {
                emit(Noise::ear(Kind::Respawn, 0.8));
                // The respawn is not a teleporter.
                self.gait.at = None;
            } else if now.alive {
                self.hurt += (was.health - now.health + (was.armour - now.armour).max(0.0)).max(0.0);
                self.hurt_cd -= dt;
                if self.hurt > 0.5 && self.hurt_cd <= 0.0 {
                    let share = self.hurt / f.full_health.max(1.0);
                    emit(Noise::ear(Kind::Hurt(share), 0.5 + share.min(1.0)));
                    self.hurt = 0.0;
                    self.hurt_cd = HURT_GAP;
                }
                if now.picked > was.picked {
                    let what = if now.guns > was.guns {
                        Pick::Weapon
                    } else if now.health > was.health + 0.5 {
                        Pick::Health
                    } else if now.armour > was.armour + 0.5 {
                        Pick::Armour
                    } else {
                        Pick::Ammo
                    };
                    emit(Noise::ear(Kind::Pickup(what), 0.8));
                }
                if now.held != was.held {
                    emit(Noise::ear(Kind::Switch, 0.6));
                } else if now.reloading && !was.reloading {
                    emit(Noise::ear(Kind::ReloadOut, 0.6));
                } else if !now.reloading && was.reloading {
                    emit(Noise::ear(Kind::ReloadIn, 0.6));
                }
            }
            // A hit marker that went up rather than down is a round landed.
            if now.mark > was.mark + 1e-4 {
                emit(Noise::ear(Kind::Hit { head: sh.mark_head }, 0.7));
            }
            if now.kill > was.kill + 1e-4 {
                emit(Noise::ear(Kind::Kill, 0.8));
            }
        }
        self.me = Some(now);

        if self.bots.len() != sh.bots.len() {
            self.bots = sh.bots.iter().map(|b| Bot { health: b.fighter.health, alive: b.fighter.alive(), picked: b.fighter.picked, gait: Gait::default(), hurt_cd: 0.0 }).collect();
        }
        for (b, was) in sh.bots.iter().zip(self.bots.iter_mut()) {
            let chest = b.body.pos + Vec3::Y * 1.2;
            let alive = b.fighter.alive();
            if was.alive && !alive {
                emit(Noise::at(Kind::Shutdown, chest, 1.2));
            } else if alive && b.fighter.health < was.health - 0.5 && was.hurt_cd <= 0.0 {
                emit(Noise::at(Kind::Clank, chest, 0.7));
                was.hurt_cd = HURT_GAP;
            }
            was.hurt_cd -= dt;
            if alive && b.fighter.picked > was.picked {
                emit(Noise::at(Kind::Grab, chest, 0.5));
            }
            if alive && (ear - b.body.pos).length() < BOT_STEPS_M {
                // Bots are robots: heavier on their feet than the player,
                // but feet all the same, under the guns.
                was.gait.step(store, b.body.pos, b.body.body.vel, b.body.body.on_ground, b.body.swimming, dt, 0.5, false);
            } else {
                was.gait = Gait::default();
            }
            was.health = b.fighter.health;
            was.alive = alive;
            was.picked = b.fighter.picked;
        }

        // Flags: whatever the log gained since last frame.
        let log = &sh.flag_log;
        if log.len() < self.flags {
            self.flags = 0;
        }
        let mine = sh.me.team;
        for (_, e) in &log[self.flags..] {
            use crate::shooter::flag::Event;
            let ours = Some(e.team()) == mine;
            let k = match *e {
                Event::Taken { .. } | Event::PickedUp { .. } => Kind::FlagTaken(ours),
                Event::Dropped { .. } => Kind::FlagDropped,
                Event::Returned { .. } => Kind::FlagReturned,
                // The flag of `team` was brought home by the other side.
                Event::Captured { .. } => Kind::FlagCaptured(!ours),
            };
            emit(Noise::ear(k, 0.8));
        }
        self.flags = log.len();

        // Rockets in the air: a held roar that goes where they go. The same
        // test as `Report::of`'s for a rocket, read off the shell: a big
        // burst, flying flat, not sludge.
        let mut nth = [0u32; 64];
        for s in &sh.combat.shells {
            let rocket = s.splash.as_ref().is_some_and(|x| x.radius >= 4.0) && s.goo_r == 0.0 && s.gravity < 8.0 && !s.stuck;
            if !rocket {
                continue;
            }
            let by = s.by.min(63);
            emit(Noise::rush(s.pos, s.vel, ear, by as u32 * 16 + nth[by]));
            nth[by] = (nth[by] + 1).min(15);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_is_soft_ground() {
        assert_eq!(ground_of(AIR), Ground::Soft);
    }
}
