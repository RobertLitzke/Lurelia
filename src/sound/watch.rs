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
use crate::world::material::AIR;
use crate::world::matlib::{self, flag};
use glam::Vec3;

/// What a material sounds like underfoot, from what the engine already
/// knows about it: the driving surface it declares (world/surface.rs), the
/// same table the wheels and their dust read, else a guess from its name.
pub fn ground_of(m: u8) -> Ground {
    use crate::world::surface::Surface;
    if m == AIR {
        Ground::Soft
    } else if matlib::has(m, flag::WATER) {
        Ground::Water
    } else if matlib::has(m, flag::FOLIAGE) {
        Ground::Leaves
    } else if matlib::has(m, flag::UNDERSTORY) {
        Ground::Grass
    } else if let Some(s) = matlib::surface(m) {
        match s {
            Surface::Rock | Surface::Ice => Ground::Stone,
            Surface::Grass => Ground::Grass,
            Surface::Hardpack | Surface::SandWet | Surface::SandSoft | Surface::Mud | Surface::Snow => Ground::Soft,
        }
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

/// Zombie groans sounding at once in the zombie round, at most, as for one
/// species of animal (creatures/voice.rs `PER_SPECIES`), and how far off a
/// zombie is still listened to.
const GROANS: usize = 4;
const GROAN_HEARD_M: f32 = 150.0;

/// A zombie's voice: when its next idle call is due, whether it was after
/// the party last frame, and the call it has sounding, posted again each
/// frame from where it now is (as creatures/voice.rs does for an animal).
#[derive(Clone, Default)]
struct Groan {
    /// Seconds until each idle call is due, by call index; empty until the
    /// zombie is first heard.
    due: Vec<f32>,
    chasing: bool,
    made: u32,
    live: Option<LiveCall>,
}

#[derive(Clone, Copy)]
struct LiveCall {
    cry: crate::sound::call::Cry,
    size: f32,
    seed: u32,
    boost: f32,
    left: f32,
}

#[derive(Default)]
pub struct Watch {
    me: Option<Me>,
    gait: Gait,
    bots: Vec<Bot>,
    groans: Vec<Groan>,
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
            Some(sh) => {
                self.match_(sh, &g.store, dt, ear);
                // The zombie round's zombies are bots, which the creature
                // voices never see: their calls are the library zombie's.
                if sh.zed.is_some() {
                    let lib = g.library();
                    let zombie = lib.and_then(|l| l.creature("zombie").map(|i| (i, &l.creatures[i])));
                    if let Some((who, v)) = zombie.and_then(|(i, c)| c.behaviour.voice.as_ref().map(|v| (i as u16, v))) {
                        self.zombie_voices(sh, v, who, dt, ear);
                    }
                }
            }
            None => {
                self.me = None;
                self.bots.clear();
                self.groans.clear();
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
            self.bots = sh
                .bots
                .iter()
                .map(|b| Bot {
                    health: b.fighter.health,
                    alive: b.fighter.alive(),
                    picked: b.fighter.picked,
                    gait: Gait::default(),
                    hurt_cd: 0.0,
                })
                .collect();
        }
        for (k, (b, was)) in sh.bots.iter().zip(self.bots.iter_mut()).enumerate() {
            let chest = b.body.pos + Vec3::Y * 1.2;
            let alive = b.fighter.alive();
            // A zombie is no robot: it neither clanks when hit nor powers
            // down. Its voice is `zombie_voices`.
            let robot = !sh.is_zombie(k + 1);
            if was.alive && !alive && robot {
                emit(Noise::at(Kind::Shutdown, chest, 1.2));
            } else if alive && robot && b.fighter.health < was.health - 0.5 && was.hurt_cd <= 0.0 {
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

    /// The zombie round's groans, from the library zombie's `voice`: its
    /// "idle" calls now and then while a zombie mills about, its "chase"
    /// calls as it turns after the party. `who` is the zombie's creature
    /// number, for the list of calls heard.
    fn zombie_voices(&mut self, sh: &crate::shooter::Shooter, voice: &crate::library::VoiceDef, who: u16, dt: f32, ear: Vec3) {
        use crate::sound::call::Cry;
        let unit = |s: u32| (s >> 8) as f32 / (1u32 << 24) as f32;
        self.groans.resize(sh.bots.len(), Groan::default());
        let mut sounding = self.groans.iter().filter(|g| g.live.is_some()).count();
        for (k, b) in sh.bots.iter().enumerate() {
            let g = &mut self.groans[k];
            let Some(chasing) = sh.zombie_chasing(k) else {
                // Down or not in play: whatever it was saying stops with it.
                if g.live.take().is_some() {
                    sounding -= 1;
                }
                g.due.clear();
                g.chasing = false;
                continue;
            };
            let mouth = b.body.pos + Vec3::Y * b.height() * 0.9;
            let key = 0x4000_0000 | k as u32;
            if let Some(l) = g.live.as_mut() {
                l.left -= dt;
                if l.left > 0.0 {
                    emit(Noise::call(mouth, l.cry, l.size, l.seed, who, key, l.boost));
                } else {
                    g.live = None;
                    sounding -= 1;
                }
            }
            let woke = chasing && !g.chasing;
            g.chasing = chasing;
            // A new seed for each call it makes.
            let seed = crate::world::noise::hash32(key ^ g.made.wrapping_mul(0x9e37_79b9));
            if g.due.is_empty() {
                // Spread the first calls over the whole wait, so a horde
                // that has just come up doesn't call as one.
                g.due = voice.calls.iter().enumerate().map(|(i, c)| c.every_s.unwrap_or([60.0, 240.0])[1] * unit(seed ^ i as u32 ^ 0x1d1e)).collect();
            }
            let mut fire = None;
            for (i, c) in voice.calls.iter().enumerate() {
                let ready = match c.on.as_str() {
                    "idle" => {
                        let [lo, hi] = c.every_s.unwrap_or([60.0, 240.0]);
                        let due = &mut g.due[i];
                        *due -= dt;
                        if *due > 0.0 {
                            false
                        } else {
                            *due = lo + (hi - lo).max(0.0) * unit(seed ^ i as u32 ^ 0xd0e5);
                            !chasing
                        }
                    }
                    "chase" => woke,
                    _ => false,
                };
                if ready && fire.is_none() && unit(seed ^ i as u32 ^ 0xc4a7) <= c.chance.unwrap_or(1.0) {
                    fire = Some(c);
                }
            }
            let Some(c) = fire else { continue };
            let Some(cry) = Cry::parse(&c.cry) else { continue };
            let far = (mouth - ear).length();
            if g.live.is_some() || sounding >= GROANS || far > GROAN_HEARD_M {
                continue;
            }
            // Each zombie's voice a little its own.
            let size = voice.size.unwrap_or(1.0) * c.size.unwrap_or(1.0) * (0.92 + 0.16 * unit(crate::world::noise::hash32(key)));
            g.made += 1;
            // Stop posting a little before the engine lets it go, so a late
            // post never starts it over.
            let left = (cry.pattern(size, seed).len + far / 343.0 - 0.2).max(0.1);
            emit(Noise::call(mouth, cry, size, seed, who, key, c.boost));
            g.live = Some(LiveCall { cry, size, seed, boost: c.boost, left });
            sounding += 1;
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

    /// The zombie round voices its zombies from the library zombie, which
    /// must have calls this watcher plays: cries the engine knows, set off
    /// by "idle" or "chase", one of each.
    #[test]
    fn the_zombie_has_a_voice_the_round_plays() {
        let (lib, errs) = crate::library::Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        let z = &lib.creatures[lib.creature("zombie").expect("a zombie creature")];
        let v = z.behaviour.voice.as_ref().expect("the zombie has a voice");
        for c in &v.calls {
            assert!(crate::sound::call::Cry::parse(&c.cry).is_some(), "no cry called {}", c.cry);
            assert!(matches!(c.on.as_str(), "idle" | "chase"), "the round plays no {} call", c.on);
        }
        for on in ["idle", "chase"] {
            assert!(v.calls.iter().any(|c| c.on == on), "no {on} call");
        }
    }
}
