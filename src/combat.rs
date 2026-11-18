//! Guns and what they hit, with nothing in it about why anyone is shooting.
//!
//! Three layers, from the data up (docs/SHOOTER.md):
//!
//! - `library/weapons/*.json` is one gun: its two fire modes, damage, ammo.
//! - `weapon.rs` holds a gun in hand: the cadence, the spin-up, the rounds
//!   left, which one is out.
//! - this file and `shot.rs` fire it: rays resolved on the spot, shells that
//!   fly and bounce and burst, damage to creatures, and a push on whoever a
//!   burst catches, the shooter included.
//!
//! The rules that decide health, respawning and score are not here; they are
//! in `src/shooter.rs`, which owns a `Combat` and calls it.
//!
//! Nothing here knows which fighter is the human. A shot is fired `by` an
//! index into the scene's `fighters` and the damage it does comes back as a
//! list of `Hit`s for the rules to apply, so a bot's rocket reaches the
//! player down the same path the player's reaches the bot.

pub mod sheet;
pub mod shot;
pub mod sludge;
pub mod viewmodel;
pub mod weapon;

use crate::creatures::Look;
use crate::entities::Entities;
use crate::library::{FireDef, FireKind, SplashDef};
use crate::models::Model;
use crate::render::overlay::Overlay;
use crate::world::material::AIR;
use crate::world::store::Store;
use glam::Vec3;
use shot::{Burst, Shell, Tracer};

/// One body a shot can find: the player, or a bot. The rules build the list
/// fresh each step and keep the order, so an index is a fighter.
#[derive(Clone, Copy, Debug)]
pub struct Hitbox {
    /// The feet.
    pub foot: Vec3,
    pub height: f32,
    pub half_w: f32,
    /// A dead body is not shot at again.
    pub alive: bool,
}

impl Hitbox {
    /// The middle of the body: what splash measures its distance to.
    pub fn chest(&self) -> Vec3 {
        self.foot + Vec3::Y * self.height * 0.55
    }
}

/// Everything a shot is resolved against. Short-lived: the caller builds one
/// per step out of what it has borrowed.
pub struct Scene<'a> {
    pub store: &'a Store,
    pub ents: &'a mut Entities,
    pub models: &'a [Model],
    /// Everyone who can be shot, in the rules' own order.
    pub fighters: &'a [Hitbox],
}

/// Damage and push landing on one fighter, for the rules to apply. `by` is
/// who fired it, so a kill can be credited; `by == who` is self-damage.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub who: usize,
    pub by: usize,
    pub damage: f32,
    /// Metres per second to add to that fighter's velocity.
    pub push: Vec3,
    /// What the same blow does to a body with nothing holding it up, metres
    /// a second. Not `push`, which is a living fighter's footing and is kept
    /// small enough to play against: a man who is still standing leans into
    /// a blast and a corpse goes over the railing, and the rocket jump would
    /// be unflyable at the speed a body should be thrown at.
    pub fling: Vec3,
    /// A ray through the top of the body (`FireDef::head_mult`): the HUD's
    /// hit marker says so.
    pub head: bool,
    /// From a burst, or from a shell that bursts: a body this kills comes
    /// apart rather than falling whole (`shooter::gibs`).
    pub blast: bool,
}

/// Everything in the air: the shells flying, the tracers still fading and the
/// bursts still expanding. One belongs to a match, not to a shooter: every
/// round in the air is in here whoever fired it.
#[derive(Default)]
pub struct Combat {
    pub shells: Vec<Shell>,
    tracers: Vec<Tracer>,
    bursts: Vec<Burst>,
    rng: u32,
    /// What landed on the fighters since the rules last took it.
    taken: Vec<Hit>,
    /// Damage dealt to animals, for the score line.
    pub dealt: f32,
    /// Animals killed. Fighters are the rules' business, not this file's.
    pub kills: u32,
    /// Voxel changes waiting for the rules to hand them to `Game`: what a
    /// bore has eaten, and what a glob of sludge has laid down. Combat may
    /// read the world and never write it.
    edits: Vec<([i32; 3], u8)>,
    /// The globs of sludge lying on the map: real voxels with a fuse.
    sludge: sludge::Sludge,
    /// Seconds a cutting beam has been held since its last bite, and how far
    /// round its twist of light has turned (`twist`).
    chew: f32,
    spun: f32,
    /// What there was to see, for the particles (src/fx): a shot leaving a
    /// muzzle, a round striking a wall, a burst. Nothing in the rules reads
    /// these; `Game` drains them each frame and the bench never does, so
    /// the list is capped (`cue`).
    cues: Vec<Cue>,
    /// Every round and beam resolved since `take_strikes`: where it started,
    /// where it stopped and what it carried. The rules test these against
    /// the dead, which are not hitboxes, so a corpse can be shot apart a
    /// piece at a time (`shooter::Shooter::chip`). Capped like `cues`,
    /// because the bench never drains it.
    strikes: Vec<(Vec3, Vec3, f32)>,
    /// The material a shell with a `spoil` lays down: the map's own, set by
    /// the rules (`ShooterDef::spoil`). 0 until they set it, which is stone.
    pub spoil_mat: u8,
}

/// Lay a cube of `mat`, `size` metres across, where a shell flying along
/// `vel` stopped at `at` (a finger short of the face it hit). Only into
/// open air, and never into a body: a block that would bury somebody leaves
/// that voxel out, which is a notch in the block rather than a fighter
/// stuck inside it.
fn lay_block(scene: &Scene, at: Vec3, vel: Vec3, size: f32, mat: u8, edits: &mut Vec<([i32; 3], u8)>) {
    let mat = if mat == 0 { crate::world::material::STONE } else { mat };
    let n = (size * 4.0).round().max(1.0) as i32;
    // Back off the face by half the block, so it sits against what it hit
    // rather than half inside it.
    let back = at - vel.normalize_or(Vec3::NEG_Y) * (size * 0.5);
    let lo = ((back * 4.0) - Vec3::splat((n - 1) as f32 * 0.5)).round();
    for dy in 0..n {
        for dz in 0..n {
            for dx in 0..n {
                let v = [lo.x as i32 + dx, lo.y as i32 + dy, lo.z as i32 + dz];
                if scene.store.get(v[0], v[1], v[2]) != Some(AIR) {
                    continue;
                }
                let (a, b) = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 4.0, Vec3::new(v[0] as f32 + 1.0, v[1] as f32 + 1.0, v[2] as f32 + 1.0) / 4.0);
                let buries = scene.fighters.iter().any(|m| m.alive && b.x > m.foot.x - m.half_w && a.x < m.foot.x + m.half_w && b.z > m.foot.z - m.half_w && a.z < m.foot.z + m.half_w && b.y > m.foot.y && a.y < m.foot.y + m.height);
                if !buries {
                    edits.push((v, mat));
                }
            }
        }
    }
}

/// Metres a second a block of spoil (the bore's right button) knocks
/// somebody back along its flight, and lifts them by: about a rocket's push,
/// which is what half a metre of rock arriving at speed should be.
const BLOCK_KNOCK: f32 = 12.0;
const BLOCK_LIFT: f32 = 3.0;

/// How many strikes are kept waiting for `take_strikes`.
const STRIKES_KEPT: usize = 256;

/// Something a fight did that shows: the particles take it from here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A round fired from `at`, along `dir`.
    Muzzle { at: Vec3, dir: Vec3 },
    /// A round struck the world at `at`, travelling along `dir`.
    Impact { at: Vec3, dir: Vec3 },
    /// A burst at `at`, `radius` metres across.
    Burst { at: Vec3, radius: f32 },
    /// Sparks thrown off something breaking, such as a dead bot's chest
    /// lamp.
    Sparks { at: Vec3 },
}

/// Cues kept waiting for somebody to take them; a fight nobody draws (the
/// bench, the tournament) keeps only the latest.
const CUES_KEPT: usize = 256;

/// The longest a shell is simulated before it is dropped, metres of flight.
const SHELL_RANGE: f32 = 400.0;
/// Seconds a burst lights the room round it.
const BURST_S: f32 = 0.32;
/// How often a beam that cuts takes a ball out of what it is held on. The
/// pick digs a 1.8 m ball every 0.14 s (`Game::dig`); this is the same thing
/// slower, so a bore held on an arena partition is through it in a second
/// and a half and a thick wall takes a steady hand.
const BITE_S: f32 = 0.26;
/// How far out of the muzzle the thread round a beam takes to open to its
/// full width.
const TWIST_OPEN_M: f32 = 2.0;

/// Metres of air kept between the camera and anything drawn in the world, so
/// a tracer leaving the muzzle doesn't fill the screen. The gun is held at
/// about half a metre, so this is as near as a ribbon may start and still be
/// a line rather than a wall of colour.
const NEAR_CLEAR: f32 = 0.9;

/// A burst scaled by a charge: the damage straight, the radius
/// by less, because a burst twice as fierce is not twice as wide.
fn scaled(sp: SplashDef, k: f32) -> SplashDef {
    SplashDef {
        damage: sp.damage * k,
        radius: sp.radius * grown(k, 0.45),
        push: sp.push * grown(k, 0.7),
        ..sp
    }
}

/// How much harder a blast throws a body than it shoves a fighter who is
/// still on his feet.
const BLAST_FLING: f32 = 2.4;
/// Metres a second a direct hit throws a body, per point of damage. A shot
/// does not move a man who is braced against it, but it does tip over one
/// who has stopped bracing: a bolt at 35 knocks the body a couple of
/// metres along the line it came in on.
const SHOT_FLING: f32 = 0.055;
/// How much of a shot's fling goes upward: enough to take the feet off the
/// floor, so the body falls rather than sliding.
const FLING_LIFT: f32 = 0.45;
/// The most a shot alone may throw a body, metres a second. A sniper's head
/// shot is 160 damage and would otherwise launch a man the length of the
/// room on a bullet, which is a blast's job.
const SHOT_MOST: f32 = 5.0;

/// What a hit of `damage` along `dir` does to a body that is no longer
/// holding itself up.
fn fling(dir: Vec3, damage: f32) -> Vec3 {
    ((dir.normalize_or_zero() + Vec3::Y * FLING_LIFT) * damage * SHOT_FLING).clamp_length_max(SHOT_MOST)
}

/// `k` taken only `share` of the way: 1 stays 1, and a multiplier of 3 at a
/// share of 0.5 is 2.
fn grown(k: f32, share: f32) -> f32 {
    1.0 + (k - 1.0) * share
}

impl Combat {
    pub fn new(seed: u32) -> Self {
        Combat { rng: seed | 1, ..Default::default() }
    }

    fn roll(&mut self) -> f32 {
        self.rng = crate::world::noise::hash32(self.rng);
        crate::world::noise::unit(self.rng)
    }

    /// Fire one round of `f`. The shot is worked out from `eye` along `dir`,
    /// because that is where the crosshair is pointing, but it leaves from
    /// `muzzle`, which is off to one side: a tracer drawn straight down the
    /// line of sight is end-on and invisible. `by` is which fighter pulled
    /// the trigger, which is how a burst knows whose self-damage it is.
    #[allow(clippy::too_many_arguments)] // eye, muzzle and aim are three separate points, and none follows from the others
    pub fn fire(&mut self, scene: &mut Scene, eye: Vec3, muzzle: Vec3, dir: Vec3, f: &FireDef, by: usize, strength: f32) {
        let dir = dir.normalize_or(Vec3::NEG_Z);
        let strength = strength.max(0.01);
        if f.kind != FireKind::Beam {
            self.cue(Cue::Muzzle { at: muzzle, dir });
        }
        match f.kind {
            // A beam is not fired a round at a time; `beam` holds it.
            FireKind::Hitscan | FireKind::Beam => {
                for _ in 0..f.pellets.max(1) {
                    let (a, b) = (self.roll(), self.roll());
                    let aim = shot::spread(dir, f.spread_deg.to_radians(), a, b);
                    // A round that bores takes the whole channel at once:
                    // that is a shot, not a cut held on a wall.
                    if f.bore_r > 0.0 {
                        self.bore(scene.store, eye, aim, f.bore_r, f.range);
                    }
                    let end = self.ray(scene, eye, aim, f, by, strength, f.bore_r > 0.0);
                    self.tracers.push(Tracer {
                        a: muzzle,
                        b: end,
                        left: f.tracer_s,
                        of: f.tracer_s,
                        tint: f.tracer,
                        width: 0.03,
                    });
                }
            }
            FireKind::Shell => {
                // Aim the shell at what the crosshair is on, not parallel to
                // the eye, or everything lands a hand to the right of it.
                let far = scene.store.ray(eye, dir, 240.0, |m| m != AIR).map_or(240.0, |h| h.t.max(2.0));
                let at = (eye + dir * far - muzzle).normalize_or(dir);
                let look = viewmodel::shell_look(f);
                // `pellets` of them, which is how the scrap gun throws a
                // cone of scrap you can watch fly rather than nine rays that
                // are over before the frame is drawn.
                for _ in 0..f.pellets.max(1) {
                    let jitter = if f.spread_deg > 0.0 {
                        let (a, b) = (self.roll(), self.roll());
                        shot::spread(at, f.spread_deg.to_radians(), a, b)
                    } else {
                        at
                    };
                    // A cone of scrap is not all thrown at one speed.
                    let speed = f.speed * if f.pellets > 1 { 0.82 + 0.36 * self.roll() } else { 1.0 };
                    let spin = self.roll() * std::f32::consts::TAU;
                    self.shells.push(Shell {
                        pos: muzzle,
                        prev: muzzle,
                        // A charged shot flies harder as well as hitting
                        // harder: a full glob of sludge is thrown, a tap
                        // dribbles out of the muzzle.
                        vel: jitter * speed * grown(strength, 0.4),
                        gravity: f.gravity,
                        bounces: f.bounces,
                        keep: f.bounce_keep,
                        self_hit: f.self_hit,
                        bounced: false,
                        fuse: if f.fuse_s > 0.0 { f.fuse_s } else { f32::MAX },
                        flown: 0.0,
                        damage: f.damage * strength,
                        splash: f.splash.map(|sp| scaled(sp, strength)),
                        tint: f.tracer,
                        look,
                        spin,
                        stick_s: f.stick_s * grown(strength, 0.6),
                        stuck: false,
                        // A charged glob is a bigger glob: more of the tank
                        // went into it.
                        goo_r: f.goo_r * grown(strength, 0.5),
                        spoil: f.spoil_r,
                        cluster: f.cluster.as_ref().map(|c| shot::Cluster {
                            count: c.count,
                            damage: c.damage * strength,
                            speed: c.speed,
                            gravity: c.gravity,
                            cone_deg: c.cone_deg,
                            bounces: c.bounces,
                            fuse_s: c.fuse_s,
                            splash: c.splash.map(|sp| scaled(sp, strength)),
                            look: viewmodel::shell_of(&c.look).unwrap_or(look),
                        }),
                        by,
                    });
                }
            }
        }
    }

    /// A beam held on a target for one step: the same ray as a round, but
    /// its `damage` is per second and the line is drawn for this step only.
    /// Nothing accumulates between steps, so a beam on a wall costs nothing
    /// and a beam on a body is a steady drain.
    ///
    /// A beam with a `bore_r` eats the wall it is held on rather than
    /// stopping at it, a bite at a time: see `gnaw`.
    #[allow(clippy::too_many_arguments)] // eye, muzzle and aim are three separate points, and none follows from the others
    pub fn hold_beam(&mut self, scene: &mut Scene, eye: Vec3, muzzle: Vec3, dir: Vec3, f: &FireDef, by: usize, dt: f32) {
        let dir = dir.normalize_or(Vec3::NEG_Z);
        if f.bore_r > 0.0 {
            self.gnaw(scene.store, eye, dir, f, dt);
        }
        let before = self.taken.len();
        let end = self.ray(scene, eye, dir, f, by, dt, false);
        // It has to outlive this step: the tracers are faded in the same
        // tick that fires them, so a beam drawn for exactly one step is a
        // beam nobody ever sees.
        let life = f.tracer_s.max(dt * 1.5);
        if f.arc_jumps > 0 {
            // Who the beam itself landed on, if anybody: where the arc
            // starts from.
            let first = self.taken[before..].iter().find(|h| h.by == by && h.who != by).map(|h| (h.who, h.damage));
            if let Some(first) = first {
                self.arc(scene, first, f, by, dir, life);
            }
        }
        self.tracers.push(Tracer {
            a: muzzle,
            b: end,
            left: life,
            of: life,
            tint: f.tracer,
            // Wider than a tracer and drawn at full brightness for its whole
            // life: a beam is a rope of light, not a streak left behind one.
            width: 0.09,
        });
        if f.twist > 0.0 {
            self.twist(muzzle, end, f, life);
        }
    }

    /// The beam jumping on from body to body (`FireDef::arc_jumps`): from
    /// `first`, the body it landed on and what it did to it, to the nearest
    /// other living body within `arc_m` that the last one can see, keeping
    /// `arc_keep` of its damage at each jump. Nobody is jumped to twice,
    /// and never back into whoever is holding it.
    fn arc(&mut self, scene: &Scene, first: (usize, f32), f: &FireDef, by: usize, dir: Vec3, life: f32) {
        let mut done = vec![by, first.0];
        let (mut at, mut damage) = first;
        for _ in 0..f.arc_jumps {
            damage *= f.arc_keep;
            let Some(here) = scene.fighters.get(at) else { break };
            let from = here.chest();
            let next = scene
                .fighters
                .iter()
                .enumerate()
                .filter(|(i, m)| m.alive && !done.contains(i))
                .map(|(i, m)| (i, m.chest(), m.chest().distance(from)))
                .filter(|&(_, to, d)| d <= f.arc_m && shot::sees(scene.store, from, to))
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let Some((n, to, _)) = next else { break };
            self.taken.push(Hit {
                who: n,
                by,
                damage,
                push: Vec3::ZERO,
                fling: fling((to - from).normalize_or(dir), damage),
                head: false,
                blast: false,
            });
            self.tracers.push(Tracer {
                a: from,
                b: to,
                left: life,
                of: life,
                tint: f.tracer,
                width: 0.06,
            });
            if f.twist > 0.0 {
                self.twist(from, to, f, life);
            }
            done.push(n);
            at = n;
        }
    }

    /// A cutting beam gnawing at whatever it is held on: every `BITE_S` it
    /// takes a ball of `bore_r` out of the face it has reached, which is the
    /// same bite the pick takes out of the open world and slower.
    ///
    /// The whole channel at once is what a round does (`fire`). Held, the
    /// shot is not an event but a rate, and taking sixty metres of wall
    /// sixty times a second is both the wrong feel and a hundred thousand
    /// voxels a frame. A bite is a few hundred, and what you watch is a hole
    /// deepening in front of you.
    fn gnaw(&mut self, store: &crate::world::store::Store, from: Vec3, dir: Vec3, f: &FireDef, dt: f32) {
        self.chew += dt;
        if self.chew < BITE_S {
            return;
        }
        self.chew = 0.0;
        let Some(hit) = store.ray(from, dir, f.range, |m| m != AIR) else { return };
        // A little in past the face, so the ball is sure to take the voxels
        // the ray stopped on and not the air just short of them.
        let at = from + dir * (hit.t + f.bore_r * 0.5);
        self.pit(store, at, f.bore_r);
    }

    /// Take a ball of `r` metres out of the world at `at`. The pick's own
    /// shape (`Game::dig`), because a hole a drill made and a hole a pick
    /// made should not be different kinds of hole.
    fn pit(&mut self, store: &crate::world::store::Store, at: Vec3, r: f32) {
        let c = (at * 4.0).floor();
        let ri = (r * 4.0).ceil() as i32;
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let v = [c.x as i32 + dx, c.y as i32 + dy, c.z as i32 + dz];
                    let mid = Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5) / 4.0;
                    if mid.distance(at) > r {
                        continue;
                    }
                    if store.get(v[0], v[1], v[2]).is_some_and(|m| m != AIR) {
                        self.edits.push((v, AIR));
                    }
                }
            }
        }
    }

    /// The thread of light round a beam: a few chords spiralling about the
    /// line, turning at the weapon file's own rate. A straight rope reads as
    /// a laser; the same rope with a thread round it reads as something
    /// turning, which is what a drill is, and what a stream of plasma is.
    fn twist(&mut self, a: Vec3, b: Vec3, f: &FireDef, life: f32) {
        let axis = b - a;
        let len = axis.length();
        if len < 0.6 {
            return;
        }
        let dir = axis / len;
        // Any two directions across the beam: the twist is drawn in these.
        let up = if dir.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let u = dir.cross(up).normalize_or(Vec3::X);
        let w = dir.cross(u);
        // It opens out of the barrel rather than starting at its full
        // width: the thread is drawn from the muzzle, with its radius
        // swelling to full over the first couple of metres, so what you see
        // leaves the bore as a point and is a rope by the time it is across
        // the room. Drawn at full width from the muzzle it is a hand's width
        // from the camera and fills the screen.
        let from = (len * 0.08).min(0.35);
        let span = len - from;
        let r = f.bore_r.clamp(0.4, 0.6) * 0.18;
        let turns = (span * 0.6).clamp(1.0, 9.0);
        let segs = (turns * 8.0).round().max(6.0) as usize;
        let phase = self.spun * f.twist * std::f32::consts::TAU;
        let point = |k: usize| {
            let t = k as f32 / segs as f32;
            let out = from + t * span;
            let ang = phase + t * turns * std::f32::consts::TAU;
            a + dir * out + (u * ang.cos() + w * ang.sin()) * r * (out / TWIST_OPEN_M).min(1.0)
        };
        for k in 0..segs {
            self.tracers.push(Tracer {
                a: point(k),
                b: point(k + 1),
                left: life,
                of: life,
                tint: f.tracer,
                width: 0.035,
            });
        }
    }

    /// Voxel changes made and not yet handed over, each a position and the
    /// material that belongs there: `AIR` where a bore ate the wall, sludge
    /// where a glob landed. The rules drain them into the activity's edit
    /// queue (`modes::ActivityCtx::edits`), because the store belongs to
    /// `Game` and combat is only allowed to ask.
    ///
    /// Nothing written this way can be read back this step, so anything
    /// placing voxels keeps its own account of what it put where
    /// (`sludge::Blob`).
    pub fn take_edits(&mut self) -> Vec<([i32; 3], u8)> {
        std::mem::take(&mut self.edits)
    }

    /// Take one bite out of the first wall along the ray, out to `far`: a
    /// ball of radius `r` metres about the voxel it lands on, which is the
    /// shape and the size a shovel cuts out in the world (`Game::dig`).
    ///
    /// Nothing here checks what it is cutting. Whether a floor is allowed to
    /// go is the rules' and `Game`'s business, as it is for a bore. True
    /// when there was something there to bite.
    pub fn bite(&mut self, store: &crate::world::store::Store, from: Vec3, dir: Vec3, r: f32, far: f32) -> bool {
        let Some((hit, _)) = store.raycast(from, dir, far) else {
            return false;
        };
        let c = Vec3::new(hit[0] as f32 + 0.5, hit[1] as f32 + 0.5, hit[2] as f32 + 0.5);
        // The radius is in metres and the grid is quarter-metre voxels.
        let rv = r * 4.0;
        let ri = rv.ceil() as i32;
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let v = [hit[0] + dx, hit[1] + dy, hit[2] + dz];
                    let d = Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5) - c;
                    if d.length() <= rv && store.get(v[0], v[1], v[2]).is_some_and(|m| m != AIR) {
                        self.edits.push((v, AIR));
                    }
                }
            }
        }
        true
    }

    /// Take a ball of radius `r` metres out of the world round `at`, where a
    /// burst went off (`SplashDef::crater`). Measured from the burst point
    /// itself, which sits a few centimetres off the face it struck, so a
    /// small radius takes a shallow bite out of a floor or a wall.
    fn crater(&mut self, store: &crate::world::store::Store, at: Vec3, r: f32) {
        let c = at * 4.0;
        let rv = r * 4.0;
        let lo = (c - Vec3::splat(rv)).floor();
        let hi = (c + Vec3::splat(rv)).ceil();
        for y in lo.y as i32..=hi.y as i32 {
            for z in lo.z as i32..=hi.z as i32 {
                for x in lo.x as i32..=hi.x as i32 {
                    let mid = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    if (mid - c).length() <= rv && store.get(x, y, z).is_some_and(|m| m != AIR) {
                        self.edits.push(([x, y, z], AIR));
                    }
                }
            }
        }
    }

    /// Eat a tunnel of radius `r` metres along the ray, out to `far`.
    ///
    /// Walked in quarter-metre steps, which is the voxel edge, and every
    /// voxel within `r` of the line goes. Nothing here checks what it is
    /// cutting: a bore through a wall and a bore through the floor are the
    /// same shot, and whether the floor is allowed to go is the rules' and
    /// `Game`'s business, not the gun's.
    fn bore(&mut self, store: &crate::world::store::Store, from: Vec3, dir: Vec3, r: f32, far: f32) {
        let step = 0.25;
        let ri = (r * 4.0).ceil() as i32;
        let mut seen = std::collections::HashSet::new();
        let mut t = 0.0f32;
        while t <= far {
            let at = from + dir * t;
            let c = [(at.x * 4.0).floor() as i32, (at.y * 4.0).floor() as i32, (at.z * 4.0).floor() as i32];
            for dy in -ri..=ri {
                for dz in -ri..=ri {
                    for dx in -ri..=ri {
                        let v = [c[0] + dx, c[1] + dy, c[2] + dz];
                        let mid = Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5) / 4.0;
                        // Distance from the line, not from this step along
                        // it, or the tunnel comes out as a string of beads.
                        let w = mid - from;
                        let along = w.dot(dir).clamp(0.0, far);
                        if (w - dir * along).length() > r {
                            continue;
                        }
                        if !seen.insert(v) {
                            continue;
                        }
                        if store.get(v[0], v[1], v[2]).is_some_and(|m| m != AIR) {
                            self.edits.push((v, AIR));
                        }
                    }
                }
            }
            t += step;
        }
    }

    /// One pellet: the nearest of the world, the fighters, the animals and
    /// anybody's plasma along the ray. `scale` multiplies the damage, which
    /// is a charge for a round and the step's own seconds for a beam.
    /// Returns where the tracer should end.
    fn ray(&mut self, scene: &mut Scene, from: Vec3, dir: Vec3, f: &FireDef, by: usize, scale: f32, through: bool) -> Vec3 {
        // `through` is a round that does not stop on a wall, because the
        // wall is going with it: the channel is cut by whoever called this.
        let mut reach = match through {
            true => f.range,
            false => scene.store.ray(from, dir, f.range, |m| m != AIR).map_or(f.range, |h| h.t),
        };
        // A fighter in front of the wall stops the round.
        let man = shot::where_fighter(scene.fighters, from, dir, reach, by);
        if let Some((t, _, _)) = man {
            reach = t;
        }
        let beast = shot::nearest_creature(scene, from, dir, reach);
        if let Some((t, _)) = beast {
            reach = t;
        }
        let at = match (beast, man) {
            // An animal in front of the fighter: the animal takes it.
            (Some((t, e)), _) => {
                let dmg = shot::falloff(f, t) * scale;
                if shot::hurt(scene, e, dmg, fling(dir, dmg)) {
                    self.kills += 1;
                }
                self.dealt += dmg;
                from + dir * t
            }
            (None, Some((t, who, head))) => {
                // A head shot, where the weapon cares: the sniper rifle's
                // whole character, and read off the capsule rather than
                // from a second hitbox.
                let mult = if head { f.head_mult } else { 1.0 };
                let damage = shot::falloff(f, t) * scale * mult;
                self.taken.push(Hit {
                    who,
                    by,
                    damage,
                    push: Vec3::ZERO,
                    fling: fling(dir, damage),
                    head,
                    blast: false,
                });
                from + dir * t
            }
            (None, None) => {
                // Stopped by the world rather than running out of range.
                if !through && reach < f.range {
                    self.cue(Cue::Impact { at: from + dir * reach, dir });
                }
                from + dir * reach
            }
        };
        if self.strikes.len() < STRIKES_KEPT {
            self.strikes.push((from, at, f.damage * scale));
        }
        if let Some(sp) = &f.splash {
            let sp = scaled(*sp, scale);
            self.burst(scene, at, &sp, by);
        }
        at
    }

    /// Set a burst off at `at`: damage and push everything inside it that it
    /// can see.
    fn burst(&mut self, scene: &mut Scene, at: Vec3, sp: &SplashDef, by: usize) {
        // Creatures.
        let caught: Vec<(hecs::Entity, f32, Vec3)> = scene
            .ents
            .world
            .query::<(hecs::Entity, &Look)>()
            .iter()
            .filter_map(|(e, l)| {
                let v = l.pos + Vec3::Y * 0.3 - at;
                let d = v.length();
                // Which way the burst would throw it: out of the blast and
                // up, as for a fighter.
                let away = if d > 0.3 { v / d } else { Vec3::Y };
                (d < sp.radius).then(|| (e, 1.0 - d / sp.radius, (away + Vec3::Y * 0.6).normalize_or(Vec3::Y)))
            })
            .collect();
        for (e, share, away) in caught {
            let dmg = sp.damage * share.powf(sp.curve);
            if shot::hurt(scene, e, dmg, away * sp.push * share * BLAST_FLING) {
                self.kills += 1;
            }
            self.dealt += dmg;
        }
        // The fighters, the one who fired among them: a burst does not care
        // whose it was, only how far away each body is and whether it can
        // see out of the corner it went off in.
        for (who, man) in scene.fighters.iter().enumerate() {
            if !man.alive {
                continue;
            }
            let chest = man.chest();
            let v = chest - at;
            let d = v.length();
            if d >= sp.radius || !shot::sees(scene.store, at, chest) {
                continue;
            }
            let share = 1.0 - d / sp.radius;
            let own = who == by;
            // Standing right on top of it there is no direction left, so the
            // push goes up: that is the rocket jump. A burst below the feet
            // always lifts, which is what makes it worth aiming at the floor.
            let away = if d > 0.3 { v / d } else { Vec3::Y };
            let up = (away + Vec3::Y * 0.6).normalize_or(Vec3::Y);
            self.taken.push(Hit {
                who,
                by,
                damage: sp.damage * share.powf(sp.curve) * if own { sp.self_damage } else { 1.0 },
                push: up * sp.push * share * if own { sp.self_push } else { 1.0 },
                // A body is thrown by the whole burst, its own included:
                // `self_push` is there so a rocket jump is survivable, and
                // the man it kills is past caring whose rocket it was.
                fling: up * sp.push * share * BLAST_FLING,
                head: false,
                blast: true,
            });
        }
        if sp.crater > 0.0 {
            self.crater(scene.store, at, sp.crater);
        }
        self.cue(Cue::Burst { at, radius: sp.radius });
        self.bursts.push(Burst {
            at,
            radius: sp.radius,
            left: BURST_S,
            tint: [1.0, 0.72, 0.36],
        });
    }

    /// Fly the shells, tick the fuses, fade the tracers and the bursts.
    /// What the fighters took is waiting in `take_hits`.
    pub fn step(&mut self, scene: &mut Scene, dt: f32) {
        // The clock the thread round a beam turns on. It runs whether or
        // not a beam is held, so one never starts from the same place
        // twice, and it is seconds rather than turns because each weapon
        // file states its own rate (`FireDef::twist`).
        self.spun = (self.spun + dt) % 600.0;
        let mut went_off: Vec<(Vec3, SplashDef, usize)> = Vec::new();
        // Globs that have landed: where, how big, how long a fuse, how
        // hard and whose. They are laid after the shells have been flown,
        // because laying one touches both the blobs and the edit queue.
        let mut landed: Vec<(Vec3, f32, f32, SplashDef, usize)> = Vec::new();
        let mut shells = std::mem::take(&mut self.shells);
        // What a cluster shell broke into. Collected rather than pushed,
        // because the shells are out of `self` while they are being flown.
        let mut thrown: Vec<shot::Shell> = Vec::new();
        shells.retain_mut(|s| match {
            let flight = s.step(scene, dt);
            // A plain round in flight (a scrap shard, a disc) is a strike
            // like a bullet is, for whatever the rules shoot at that is not
            // a hitbox: the stretch it flew this step, and what it carries.
            if s.splash.is_none() && s.goo_r <= 0.0 && s.spoil <= 0.0 && !s.stuck && self.strikes.len() < STRIKES_KEPT {
                self.strikes.push((s.prev, s.pos, s.damage));
            }
            flight
        } {
            shot::Flight::Flying => true,
            shot::Flight::Hit { at, beast, man } => {
                if let Some(e) = beast {
                    if shot::hurt(scene, e, s.damage, fling(s.vel, s.damage)) {
                        self.kills += 1;
                    }
                    self.dealt += s.damage;
                }
                // Struck a fighter: the shell's own damage, on top of the
                // burst below. Without this a shell with no burst -- a
                // scrap shard, a plasma bolt, a skipjack disc -- flies
                // through a body and does nothing at all.
                if let Some(who) = man {
                    // Its owner takes their own share of it (`self_hit`).
                    let damage = if who == s.by { s.damage * s.self_hit } else { s.damage };
                    self.taken.push(Hit {
                        who,
                        by: s.by,
                        damage,
                        // A body stops a shell dead rather than being shoved
                        // by it; what moves anybody is the burst. Except a
                        // block of stone, which is what it is for.
                        push: if s.spoil > 0.0 { s.vel.normalize_or_zero() * BLOCK_KNOCK + Vec3::Y * BLOCK_LIFT } else { Vec3::ZERO },
                        fling: fling(s.vel, s.damage),
                        head: false,
                        blast: s.splash.is_some(),
                    });
                }
                // A glob of sludge that found a wall or the floor, or
                // that ran its fuse out in the air, becomes matter rather
                // than bursting: what this gun does is what it leaves
                // behind. One that found a body bursts on it, which is what
                // its file says and what the damage above has paid for.
                // A shell with no burst that found the world: chips off it.
                if beast.is_none() && man.is_none() && s.splash.is_none() && s.goo_r <= 0.0 {
                    self.cues.push(Cue::Impact { at, dir: s.vel });
                    if self.cues.len() > CUES_KEPT {
                        self.cues.remove(0);
                    }
                }
                // A block of spoil that found the world becomes part of it.
                if s.spoil > 0.0 && beast.is_none() && man.is_none() {
                    lay_block(scene, at, s.vel, s.spoil, self.spoil_mat, &mut self.edits);
                }
                match (s.goo_r > 0.0 && beast.is_none() && man.is_none(), s.splash) {
                    (true, Some(sp)) => {
                        let fuse = if s.stick_s > 0.0 { s.stick_s } else { 1.0 };
                        landed.push((at, s.goo_r, fuse, sp, s.by));
                    }
                    (_, Some(sp)) => went_off.push((at, sp, s.by)),
                    (_, None) => {}
                }
                if let Some(c) = s.cluster {
                    let back = -s.vel.normalize_or(Vec3::Y);
                    for _ in 0..c.count {
                        let (a, b) = (self.roll(), self.roll());
                        // Thrown back out of the impact rather than on into
                        // it, or half of them are inside the wall the shell
                        // just hit and go off there.
                        let dir = shot::spread(back, c.cone_deg.to_radians(), a, b);
                        let speed = c.speed * (0.7 + 0.6 * self.roll());
                        thrown.push(shot::Shell {
                            pos: at + dir * 0.2,
                            prev: at + dir * 0.2,
                            vel: dir * speed,
                            gravity: c.gravity,
                            bounces: c.bounces,
                            keep: 0.45,
                            self_hit: 0.0,
                            bounced: false,
                            fuse: c.fuse_s,
                            flown: 0.0,
                            damage: c.damage,
                            splash: c.splash,
                            tint: s.tint,
                            look: c.look,
                            spin: self.roll() * std::f32::consts::TAU,
                            stick_s: 0.0,
                            stuck: false,
                            goo_r: 0.0,
                            spoil: 0.0,
                            cluster: None,
                            by: s.by,
                        });
                    }
                }
                false
            }
            shot::Flight::Spent => false,
        });
        shells.extend(thrown);
        self.shells = shells;
        // The sludge: globs that have just landed, then what slumps, what
        // goes off, and what its neighbours catch from it. Every voxel it
        // moves goes on the queue, because the store is `Game`'s.
        let mut edits = std::mem::take(&mut self.edits);
        for (at, r, fuse, sp, by) in landed {
            // Nowhere to put it -- fired into a corner it cannot fill --
            // and it simply goes off, as any other shell would.
            if !self.sludge.land(scene.store, at, r, fuse, sp, by, &mut edits) {
                went_off.push((at, sp, by));
            }
        }
        went_off.extend(self.sludge.step(scene, dt, &mut edits));
        self.edits = edits;
        for (at, sp, by) in went_off {
            self.burst(scene, at, &sp, by);
        }
        self.tracers.retain_mut(|t| {
            t.left -= dt;
            t.left > 0.0
        });
        self.bursts.retain_mut(|b| {
            b.left -= dt;
            b.left > 0.0
        });
    }

    /// The light each burst still throws: where, how far it reaches, its
    /// colour and how bright, for `Game` to light the room with. It flares
    /// white-gold and dies away orange over `BURST_S`.
    pub fn flashes(&self) -> impl Iterator<Item = (Vec3, f32, [f32; 3], f32)> + '_ {
        self.bursts.iter().map(|b| {
            let k = (b.left / BURST_S).clamp(0.0, 1.0);
            let colour = [1.0, 0.45 + 0.35 * k, 0.12 + 0.3 * k];
            (b.at, b.radius * (1.6 + 0.8 * k), colour, 3.0 * k * k)
        })
    }

    /// Everything that has landed on a fighter since this was last called.
    pub fn take_hits(&mut self) -> Vec<Hit> {
        std::mem::take(&mut self.taken)
    }

    /// Record a round as if one had been fired, for tests of what the
    /// rules do with `strikes`.
    #[cfg(test)]
    pub fn strike(&mut self, from: Vec3, to: Vec3, damage: f32) {
        self.strikes.push((from, to, damage));
    }

    /// Every round and beam since this was last called (`strikes`).
    pub fn take_strikes(&mut self) -> Vec<(Vec3, Vec3, f32)> {
        std::mem::take(&mut self.strikes)
    }

    /// Nothing is in the air: the rules can let a round end.
    #[allow(dead_code)] // the rules will want it when a round can end
    pub fn quiet(&self) -> bool {
        self.shells.is_empty() && self.sludge.idle()
    }

    /// The cues since the last call (see `Cue`).
    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Sparks at `at`, for whatever the rules want to see break.
    pub fn sparks(&mut self, at: Vec3) {
        self.cue(Cue::Sparks { at });
    }

    fn cue(&mut self, c: Cue) {
        if self.cues.len() >= CUES_KEPT {
            self.cues.remove(0);
        }
        self.cues.push(c);
    }

    pub fn clear(&mut self) {
        self.taken.clear();
        self.cues.clear();
        self.strikes.clear();
        self.shells.clear();
        // The sludge on the map goes too, and goes the only way anything
        // can: on the queue, for `Game` to apply.
        self.sludge.clear(&mut self.edits);
        self.tracers.clear();
        self.bursts.clear();
    }

    /// Draw it all as overlay lines: tracers fading, shells as a bright
    /// streak along their flight.
    /// Overlay lines are ribbons a fixed width in metres, so anything within
    /// arm's reach of `eye` fills the screen; everything here starts clear of
    /// the camera rather than at the muzzle.
    pub fn draw(&self, eye: Vec3, out: &mut Overlay) {
        for t in &self.tracers {
            let a = (t.left / t.of.max(1e-3)).clamp(0.0, 1.0);
            let v = t.b - t.a;
            let len = v.length();
            if len < 0.1 {
                continue;
            }
            let start = t.a + v / len * (NEAR_CLEAR - (t.a - eye).length()).clamp(0.0, len);
            out.line(start, t.b, t.width, [t.tint[0], t.tint[1], t.tint[2], a * 0.95]);
        }
        // The shell itself is a model in the world (`viewmodel::SHELLS`,
        // drawn by the rules' `instances`); what is left here is the trail
        // behind it, which a model cannot do.
        for s in &self.shells {
            if (s.pos - eye).length() < NEAR_CLEAR {
                continue;
            }
            // Sludge on the wall has no trail: a short bright smear where it
            // landed, so you can see what you have mined a doorway with.
            if s.stuck {
                let r = s.splash.map_or(0.4, |sp| sp.radius * 0.18);
                out.line(s.pos - Vec3::Y * r, s.pos + Vec3::Y * r, r, [s.tint[0], s.tint[1], s.tint[2], 0.45]);
                continue;
            }
            let back = s.vel.normalize_or(Vec3::NEG_Z);
            if s.splash.is_some() {
                // A motor leaves smoke. Scrap does not.
                out.line(s.pos - back * 0.3, s.pos - back * 3.2, 0.34, [0.62, 0.6, 0.58, 0.2]);
                out.line(s.pos - back * 0.2, s.pos - back * 1.1, 0.14, [s.tint[0], s.tint[1], s.tint[2], 0.5]);
            } else {
                out.line(s.pos, s.pos - back * 0.9, 0.05, [s.tint[0], s.tint[1], s.tint[2], 0.55]);
            }
        }
        // A burst is drawn by the particles (`Cue::Burst`) and lit by
        // `flashes`; nothing of it is a line.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::weapon::Trigger;
    use crate::library::FireKind;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    fn room() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        // A 12 m square of floor inside walls, all in voxels.
        for x in -24..24 {
            for z in -24..24 {
                for y in 0..8 {
                    s.set(x, y, z, STONE);
                }
                if x.abs() >= 22 || z.abs() >= 22 {
                    for y in 8..40 {
                        s.set(x, y, z, STONE);
                    }
                }
            }
        }
        s
    }

    fn rocket() -> FireDef {
        FireDef {
            bore_r: 0.0,
            goo_r: 0.0,
            spoil_r: 0.0,
            burst: 0,
            burst_gap_s: 0.0,
            twist: 0.0,
            arc_jumps: 0,
            arc_m: 0.0,
            arc_keep: 0.5,
            cluster: None,
            kind: FireKind::Shell,
            rpm: 60.0,
            damage: 40.0,
            pellets: 1,
            spread_deg: 0.0,
            range: 240.0,
            falloff: 1.0,
            spin_up: 0.0,
            speed: 30.0,
            gravity: 0.0,
            bounces: 0,
            bounce_keep: 0.45,
            self_hit: 0.0,
            fuse_s: 8.0,
            splash: Some(SplashDef {
                damage: 80.0,
                radius: 5.0,
                push: 13.0,
                self_damage: 0.34,
                self_push: 1.0,
                crater: 0.0,
                curve: 1.0,
            }),
            per_shot: 1,
            kick: 1.0,
            charge_s: 0.0,
            charge_mult: 1.0,
            stick_s: 0.0,
            head_mult: 1.0,
            tracer: [1.0, 0.5, 0.2],
            tracer_s: 0.08,
            look: String::new(),
        }
    }

    fn me(foot: Vec3) -> Hitbox {
        Hitbox {
            foot,
            height: 1.75,
            half_w: 0.3,
            alive: true,
        }
    }

    /// Everything that landed on fighter `who`.
    fn on(hits: &[Hit], who: usize) -> (f32, Vec3) {
        hits.iter().filter(|h| h.who == who).fold((0.0, Vec3::ZERO), |(d, p), h| (d + h.damage, p + h.push))
    }

    /// A rocket fired at the floor under the player's feet throws them
    /// upward and takes a third of its damage: the rocket jump.
    #[test]
    fn a_rocket_at_your_feet_lifts_you() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        c.fire(&mut scene, eye, eye, Vec3::NEG_Y, &rocket(), 0, 1.0);
        for _ in 0..30 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        let (damage, push) = on(&c.take_hits(), 0);
        assert!(c.quiet(), "the rocket never went off");
        assert!(push.y > 6.0, "it only lifted {} m/s", push.y);
        assert!((damage - 80.0 * 0.34 * 0.8).abs() < 12.0, "took {damage} damage");
    }

    /// A blast throws a carcass. An animal that is shot stops being an
    /// animal and becomes a thing with mass, and what killed it is still in
    /// it: it is thrown, it comes down, and it stops. Before this a carcass
    /// stayed exactly where the animal was standing, in the air if that is
    /// where it was.
    #[test]
    fn a_blast_throws_a_carcass() {
        use crate::creatures::{flung::Flung, Creature, Look};
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let stood = Vec3::new(0.0, 2.0, 0.0);
        let beast = ents.world.spawn((Look::new(0, stood, 0.0), Creature::of(0)));
        let fighters = [me(Vec3::new(8.0, 2.0, 0.0))];
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            // Into the floor beside it, so what reaches the animal is the
            // burst and not the shell.
            let at = stood + Vec3::new(1.6, 3.0, 0.0);
            c.fire(&mut scene, at, at, Vec3::NEG_Y, &rocket(), 0, 1.0);
            for _ in 0..40 {
                c.step(&mut scene, 1.0 / 60.0);
            }
        }
        let thrown = ents.world.get::<&Flung>(beast).map(|f| f.body.vel).expect("the carcass was not thrown");
        assert!(thrown.x < -4.0 && thrown.y > 4.0, "the blast left it going {thrown:?}");
        for _ in 0..150 {
            crate::creatures::flung::update(&mut ents.world, &[], &store, 1.0 / 30.0);
        }
        let at = ents.world.get::<&Look>(beast).map(|l| l.pos).unwrap();
        // How far it ended up, not which way: the room is twelve metres and
        // a blast throws a body harder than that, so it comes off a wall.
        assert!(at.distance(stood) > 2.0, "it ended up {:.1} m from where it died", at.distance(stood));
        assert!((at.y - 2.0).abs() < 0.2, "it came to rest at y {:.2}, not on the floor", at.y);
        assert!(ents.world.get::<&Flung>(beast).is_err(), "it is still being stepped after it stopped");
    }

    /// A rocket fired across the room goes off on the far wall, far enough
    /// away that the player takes nothing.
    /// A rocket already in the air is no reason to wait: fire, reload, fire
    /// again, and both are flying at once. Nothing caps how many bodies may
    /// be in the air, and this is what says so.
    #[test]
    fn two_rockets_can_be_in_the_air_at_once() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        // Along the room, which is 12 m of floor, so at 30 m/s neither has
        // reached the far wall when the second one leaves.
        let down = Vec3::X;
        c.fire(&mut scene, eye, eye, down, &rocket(), 0, 1.0);
        for _ in 0..6 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        c.fire(&mut scene, eye, eye, down, &rocket(), 0, 1.0);
        assert_eq!(c.shells.len(), 2, "the second rocket did not leave while the first was flying");
        let apart = (c.shells[0].pos - c.shells[1].pos).length();
        assert!(apart > 2.0, "the two rockets are {apart} m apart, so the first one never moved");
    }

    /// A shell mode with pellets throws one body per pellet, each on its own
    /// line: the scrap gun's cone of scrap, which used to be nine rays
    /// that were over before the frame was drawn.
    #[test]
    fn buckshot_throws_one_body_per_pellet() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(11);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let mut f = rocket();
        f.pellets = 9;
        f.spread_deg = 7.0;
        f.splash = None;
        f.bounces = 1;
        let eye = foot + Vec3::Y * 1.5;
        c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
        assert_eq!(c.shells.len(), 9, "nine pellets did not make nine bodies");
        // They are a cone, not a column: no two on the same heading.
        let a = c.shells[0].vel.normalize();
        let b = c.shells[8].vel.normalize();
        assert!(a.dot(b) < 0.9999, "every shard went down the same line");
        // And every one of them is drawn as something.
        assert!(c.shells.iter().all(|s| s.look >= viewmodel::SHELL), "a shell has no body to draw");
    }

    /// A skipjack disc, as its file has it, fired across the room at a
    /// shallow angle: it comes off the walls all four times at a pace still
    /// worth dodging, and ends on a wall, never hanging in the air. It used
    /// to keep under half its speed per bounce, crawl after the second, and
    /// run its fuse out in mid-air.
    #[test]
    fn a_skipjack_disc_bounces_and_ends_on_a_wall() {
        let text = std::fs::read_to_string("library/weapons/skipjack.json").unwrap();
        let w: crate::library::WeaponDef = serde_json::from_str(&text).unwrap();
        let f = w.fire;
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(5);
        // The firer is out of the way, in a corner, so the disc only meets
        // walls.
        let fighters = [me(Vec3::new(-4.5, 2.0, -4.5))];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = Vec3::new(0.0, 3.5, 0.0);
        c.fire(&mut scene, eye, eye, Vec3::new(1.0, 0.0, 0.37).normalize(), &f, 0, 1.0);
        assert_eq!(c.shells.len(), 1);
        let mut last = (Vec3::ZERO, 0, 0.0);
        for _ in 0..(20.0 / (1.0 / 60.0)) as usize {
            if c.shells.is_empty() {
                break;
            }
            let s = &c.shells[0];
            last = (s.pos, s.bounces, s.vel.length());
            c.step(&mut scene, 1.0 / 60.0);
        }
        assert!(c.shells.is_empty(), "the disc was still flying after twenty seconds");
        let (_, bounces, speed) = last;
        assert_eq!(bounces, 0, "it ended with {bounces} bounces left");
        assert!(speed > 15.0, "it was down to {speed:.1} m/s by its last wall");
        // It chipped the wall it ended on, and the room's walls are at
        // +-5.5 m: it ended against one of them, not in the air.
        let at = c.cues.iter().rev().find_map(|q| if let Cue::Impact { at, .. } = q { Some(*at) } else { None }).expect("it never struck anything");
        assert!(at.x.abs() > 5.0 || at.z.abs() > 5.0, "it ended in mid-air at {at:?}");
    }

    /// The bore's right button throws a block of the map's spoil, which
    /// lands against the wall it is thrown at as half a metre of real
    /// voxels in that material, in the air in front of the wall and not in
    /// it.
    #[test]
    fn a_bore_block_lands_as_voxels() {
        let text = std::fs::read_to_string("library/weapons/bore.json").unwrap();
        let w: crate::library::WeaponDef = serde_json::from_str(&text).unwrap();
        let f = w.alt.expect("the bore has a second mode");
        assert!(f.spoil_r > 0.0, "the bore's right button throws nothing");
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(5);
        c.spoil_mat = 9;
        let fighters = [me(Vec3::new(-4.5, 2.0, -4.5))];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = Vec3::new(0.0, 3.5, 0.0);
        c.fire(&mut scene, eye, eye, Vec3::new(1.0, 0.15, 0.0).normalize(), &f, 0, 1.0);
        for _ in 0..120 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        let laid = c.take_edits();
        assert_eq!(laid.len(), 8, "it laid {} voxels, not a 2x2x2 block", laid.len());
        for (v, m) in &laid {
            assert_eq!(*m, 9, "laid the wrong material");
            assert!(store.get(v[0], v[1], v[2]) == Some(AIR), "laid a voxel inside the wall at {v:?}");
            assert!(v[0] >= 18 && v[0] < 22, "the block is not against the east wall: {v:?}");
        }
    }

    /// A bore block that passes close beside somebody hits them: it is
    /// half a metre wide, not a point, and a block that went through a
    /// shoulder because its middle missed read as a block that went through
    /// a body. It hurts, it knocks them back along its flight, and it does
    /// not lay itself down inside them.
    #[test]
    fn a_bore_block_hits_with_its_width_and_knocks_back() {
        let text = std::fs::read_to_string("library/weapons/bore.json").unwrap();
        let w: crate::library::WeaponDef = serde_json::from_str(&text).unwrap();
        let f = w.alt.expect("the bore has a second mode");
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(5);
        let them = Vec3::new(3.0, 2.0, 0.45);
        let fighters = [me(Vec3::new(-4.5, 2.0, -4.5)), me(them)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        // Level and straight down x at chest height, its middle 0.45 m to
        // the side of theirs: a bullet on this line misses.
        let eye = Vec3::new(-3.0, 3.0, 0.0);
        let mut bullet = f.clone();
        bullet.spoil_r = 0.0;
        bullet.gravity = 0.0;
        c.fire(&mut scene, eye, eye, Vec3::X, &bullet, 0, 1.0);
        for _ in 0..30 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        assert!(c.take_hits().is_empty(), "a point on this line hit them, so the test proves nothing");
        c.take_edits();
        let mut block = f.clone();
        block.gravity = 0.0;
        c.fire(&mut scene, eye, eye, Vec3::X, &block, 0, 1.0);
        for _ in 0..30 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        let hits = c.take_hits();
        assert_eq!(hits.len(), 1, "the block went through them");
        assert_eq!(hits[0].who, 1);
        assert!((hits[0].damage - f.damage).abs() < 1e-3);
        assert!(hits[0].push.x > 8.0 && hits[0].push.y > 0.0, "it did not knock them back: {:?}", hits[0].push);
        assert!(c.take_edits().is_empty(), "a block that hit a body laid itself down anyway");
    }

    /// The pulse gun's right button, held on one of a crowd: it jumps on to
    /// the nearest body in sight within reach, and on from that one, at
    /// half the damage each time. It does not jump through a wall, to
    /// somebody out of reach, back to anybody it has already been in, or
    /// into whoever is holding it.
    #[test]
    fn the_arc_jumps_body_to_body_at_half_each_time() {
        let text = std::fs::read_to_string("library/weapons/pulse_gun.json").unwrap();
        let w: crate::library::WeaponDef = serde_json::from_str(&text).unwrap();
        let f = w.alt.expect("the pulse gun has a second mode");
        assert!(f.arc_jumps >= 2 && (f.arc_keep - 0.5).abs() < 1e-6);
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(5);
        let fighters = [
            me(Vec3::new(-2.0, 2.0, 0.0)),
            // The one the beam is held on.
            me(Vec3::new(4.5, 2.0, 0.0)),
            // Nearest to it of all, but through the east wall.
            me(Vec3::new(7.5, 2.0, 0.0)),
            // A chain of two across the room from it.
            me(Vec3::new(4.5, 2.0, -3.5)),
            me(Vec3::new(1.0, 2.0, -3.5)),
            // And one too far from any of them.
            me(Vec3::new(-4.5, 2.0, 4.5)),
        ];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = fighters[0].foot + Vec3::Y * 1.5;
        let dir = (fighters[1].chest() - eye).normalize();
        c.hold_beam(&mut scene, eye, eye, dir, &f, 0, 1.0 / 60.0);
        let hits = c.take_hits();
        let on = |who: usize| hits.iter().filter(|h| h.who == who).map(|h| h.damage).sum::<f32>();
        assert!(on(1) > 0.0, "the beam missed the one it was held on");
        assert!((on(3) - on(1) * 0.5).abs() < 1e-4, "first jump: {} against {}", on(3), on(1));
        assert!((on(4) - on(1) * 0.25).abs() < 1e-4, "second jump: {} against {}", on(4), on(1));
        assert_eq!(on(2), 0.0, "it jumped through the wall");
        assert_eq!(on(5), 0.0, "it jumped further than it reaches");
        assert_eq!(on(0), 0.0, "it jumped back into whoever was holding it");
        assert_eq!(hits.len(), 3, "somebody was hit twice: {hits:?}");
    }

    /// A skipjack disc fired square at a wall comes straight back at whoever
    /// fired it, and cuts them for half what it would cut anybody else. On
    /// its way out it does not touch them.
    #[test]
    fn a_skipjack_disc_comes_back_at_you_for_half() {
        let text = std::fs::read_to_string("library/weapons/skipjack.json").unwrap();
        let w: crate::library::WeaponDef = serde_json::from_str(&text).unwrap();
        let f = w.fire;
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(5);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
        for _ in 0..120 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        let hits = c.take_hits();
        assert_eq!(hits.len(), 1, "the disc hit {} times", hits.len());
        assert_eq!((hits[0].who, hits[0].by), (0, 0));
        assert!((hits[0].damage - f.damage * 0.5).abs() < 1e-3, "it cut its owner for {}", hits[0].damage);
    }

    #[test]
    fn a_rocket_down_the_room_does_not_touch_you() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        c.fire(&mut scene, eye, eye, Vec3::X, &rocket(), 0, 1.0);
        for _ in 0..60 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        assert!(c.quiet(), "the rocket never reached the wall");
        assert_eq!(on(&c.take_hits(), 0).0, 0.0, "it hurt the shooter from across the room");
    }

    /// A rocket fired at somebody else goes off on them rather than flying
    /// past, hurts them far more than it hurts the man who fired it (which
    /// is `self_damage`), and is credited to him.
    #[test]
    fn a_rocket_at_somebody_else_hurts_them_and_barely_touches_you() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        // Four metres: the test room is only twelve across, so this is
        // inside the burst and the self-damage share is the point.
        let fighters = [me(foot), me(foot + Vec3::X * 4.0)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        c.fire(&mut scene, eye, eye, Vec3::X, &rocket(), 0, 1.0);
        for _ in 0..60 {
            c.step(&mut scene, 1.0 / 60.0);
        }
        let hits = c.take_hits();
        let (mine, _) = on(&hits, 0);
        let (theirs, shove) = on(&hits, 1);
        assert!(theirs > 40.0, "the man in the way only took {theirs}");
        assert!(theirs > mine * 5.0, "it hurt the shooter nearly as much ({mine}) as the target ({theirs})");
        assert!(shove.length() > 4.0, "a direct rocket barely moved him: {shove:?}");
        assert!(hits.iter().any(|h| h.who == 1 && h.by == 0), "nobody was credited with it");
    }

    /// A weapon that cares where it lands does more damage through the top
    /// of a body than through the middle of one.
    #[test]
    fn a_head_shot_is_worth_more() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot), me(foot + Vec3::X * 4.0)];
        let mut f = rocket();
        f.kind = FireKind::Hitscan;
        f.splash = None;
        f.damage = 50.0;
        f.falloff = 1.0;
        f.range = 100.0;
        f.head_mult = 2.0;
        // The same shot at the chest and at the head of a man four metres
        // off: the aim is the only difference between them.
        let mut at = |aim: Vec3, f: &FireDef| {
            let mut c = Combat::new(5);
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            let eye = foot + Vec3::Y * 1.5;
            c.fire(&mut scene, eye, eye, aim, f, 0, 1.0);
            on(&c.take_hits(), 1).0
        };
        let chest = at(Vec3::new(4.0, -0.6, 0.0).normalize(), &f);
        let head = at(Vec3::new(4.0, 0.12, 0.0).normalize(), &f);
        assert!(chest > 0.0 && head > 0.0, "one of the two shots missed: {chest} and {head}");
        assert!((head - chest * 2.0).abs() < 0.01, "the head shot was worth {head} against {chest}");
    }

    /// Sludge clings to the wall it lands on, waits there, and goes off on
    /// whoever walks up to it.
    #[test]
    fn sludge_waits_on_the_wall() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(3);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        // Nobody else in the room: the glob has to stay put.
        let fighters = [me(foot)];
        let mut f = rocket();
        f.speed = 20.0;
        f.gravity = 10.0;
        f.stick_s = 5.0;
        f.fuse_s = 9.0;
        f.splash = Some(SplashDef {
            damage: 40.0,
            radius: 3.0,
            push: 2.0,
            self_damage: 1.0,
            self_push: 1.0,
            crater: 0.0,
            curve: 1.0,
        });
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            let eye = foot + Vec3::Y * 1.5;
            c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
            for _ in 0..120 {
                c.step(&mut scene, 1.0 / 60.0);
            }
        }
        assert_eq!(c.shells.len(), 1, "the glob did not stay on the wall");
        assert!(c.shells[0].stuck, "it is still flying after two seconds in a twelve metre room");
        let where_it_is = c.shells[0].pos;
        assert_eq!(c.take_hits().len(), 0, "it went off with nobody near it");
        // Somebody walks into it.
        let near = [me(foot), me(where_it_is - Vec3::Y * 1.0)];
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &near,
            };
            c.step(&mut scene, 1.0 / 60.0);
        }
        assert!(c.shells.is_empty(), "walking into it did not set it off");
        let (took, _) = on(&c.take_hits(), 1);
        assert!(took > 10.0, "it went off and the man beside it took {took}");
    }

    /// A beam does its damage by the second, so a second of it on a body is
    /// the same damage however long the frames are. The step length is
    /// whatever `dt` carries in, and this is the test that catches somebody
    /// reaching for `game::STEP` inside `hold_beam` instead: that passes at
    /// 1/60 and is wrong at every other frame rate. It is also drawn for
    /// the step it was held and leaves nothing in the air.
    #[test]
    fn a_beam_pays_by_the_second() {
        for dt in [1.0f32 / 60.0, 1.0 / 30.0, 1.0 / 144.0] {
            let store = room();
            let mut ents = Entities::new(Default::default(), 1);
            let foot = Vec3::new(0.0, 2.0, 0.0);
            let fighters = [me(foot), me(foot + Vec3::X * 5.0)];
            let mut f = rocket();
            f.kind = crate::library::FireKind::Beam;
            f.splash = None;
            f.damage = 60.0;
            f.falloff = 1.0;
            f.range = 60.0;
            let mut c = Combat::new(4);
            let eye = foot + Vec3::Y * 1.5;
            let aim = Vec3::new(5.0, -0.5, 0.0).normalize();
            {
                let mut scene = Scene {
                    store: &store,
                    ents: &mut ents,
                    models: &[],
                    fighters: &fighters,
                };
                for _ in 0..(1.0 / dt).round() as u32 {
                    c.hold_beam(&mut scene, eye, eye, aim, &f, 0, dt);
                }
            }
            let (took, _) = on(&c.take_hits(), 1);
            assert!((took - 60.0).abs() < 1.0, "a second of a 60-a-second beam at dt {dt} did {took}");
            assert!(c.shells.is_empty(), "a beam left something in the air");
            assert!(!c.tracers.is_empty(), "a beam drew no line");
        }
    }

    /// A ray stops on the first body in its way, and the shooter is not in
    /// its own line of fire.
    #[test]
    fn a_ray_stops_on_the_first_body() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(7);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot), me(foot + Vec3::X * 2.0), me(foot + Vec3::X * 4.0)];
        let mut scene = Scene {
            store: &store,
            ents: &mut ents,
            models: &[],
            fighters: &fighters,
        };
        let eye = foot + Vec3::Y * 1.5;
        let mut f = rocket();
        f.kind = FireKind::Hitscan;
        f.splash = None;
        f.damage = 20.0;
        f.falloff = 1.0;
        f.range = 100.0;
        c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
        let hits = c.take_hits();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].who, 1, "it went through the near man");
        assert!((hits[0].damage - 20.0).abs() < 0.01);
    }

    /// Every weapon in the library, every mode of it, held on a man four
    /// metres off for three seconds: each one has to put damage on him.
    /// This is the net under the arsenal -- a mode that fires nothing, or
    /// fires something that passes through a body without touching it, shows
    /// up here as a zero and nowhere else.
    #[test]
    fn every_mode_of_every_weapon_lands_something() {
        let (lib, errs) = crate::library::Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = room();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [me(foot), me(foot + Vec3::X * 4.0)];
        let eye = foot + Vec3::Y * 1.5;
        let aim = Vec3::new(4.0, -0.6, 0.0).normalize();
        let mut zeroes: Vec<String> = Vec::new();
        for def in &lib.weapons {
            for t in [Trigger::Fire, Trigger::Alt] {
                let mut g = crate::combat::weapon::Held::new(std::sync::Arc::new(def.clone()), true);
                if g.mode(t).is_none() {
                    continue;
                }
                let mut ents = Entities::new(Default::default(), 1);
                let mut c = Combat::new(7);
                let mut took = 0.0f32;
                let dt = 1.0 / 60.0;
                for i in 0..190 {
                    // The last half second the finger comes off, which is
                    // what fires a charging mode at all.
                    let held = if i < 160 { t } else { Trigger::Off };
                    let shot = g.pull(held, dt);
                    let mut scene = Scene {
                        store: &store,
                        ents: &mut ents,
                        models: &[],
                        fighters: &fighters,
                    };
                    if let Some(f) = g.fired_mode(&shot, held).cloned() {
                        if shot.beam {
                            c.hold_beam(&mut scene, eye, eye, aim, &f, 0, dt);
                        }
                        for _ in 0..shot.shots {
                            c.fire(&mut scene, eye, eye, aim, &f, 0, shot.strength);
                        }
                    }
                    c.step(&mut scene, dt);
                    took += on(&c.take_hits(), 1).0;
                }
                let what = format!("{} {t:?}", def.name);
                println!("{what:34} {took:7.1}");
                if took <= 0.0 {
                    zeroes.push(what);
                }
            }
        }
        assert!(zeroes.is_empty(), "these modes put nothing on a man four metres away: {zeroes:?}");
    }

    /// The flight predictor finds a wall that sends a shell back onto a man
    /// standing where no straight line reaches: the bank shot. The room's
    /// east wall is at x = 5.5, so a disc thrown at it from the west comes
    /// off at the mirrored angle, and a target put where the mirror says it
    /// will land has to be found -- and one put anywhere else has not.
    #[test]
    fn a_shell_can_be_banked_off_a_wall() {
        let store = room();
        let mut f = rocket();
        f.speed = 60.0;
        f.gravity = 0.0;
        f.bounces = 2;
        f.fuse_s = 3.0;
        f.splash = None;
        let from = Vec3::new(-3.0, 3.5, 0.0);
        // Out to the wall at +x, two metres up the room as it goes.
        let out = Vec3::new(8.5, 0.0, 2.0).normalize();
        // Mirrored in the wall's normal: it comes back the way it went, with
        // the same drift along z, so it crosses x = -3 two metres further on.
        let lands = Vec3::new(-3.0, 3.5, 4.0);
        let near = shot::bank(&store, from, out, &f, lands, Vec3::ZERO);
        assert!(near < 0.5, "the banked disc passed {near:.2} m from where the mirror says it lands");
        // And it does not claim to reach somebody it flies nowhere near.
        let elsewhere = Vec3::new(-3.0, 3.5, -4.0);
        let miss = shot::bank(&store, from, out, &f, elsewhere, Vec3::ZERO);
        assert!(miss > 2.0, "it claimed to land {miss:.2} m from a man it never flew at");
        // A shell with no bounces in it stops on the wall and never comes
        // back, which is why `bounces` is what the bot reads.
        f.bounces = 0;
        let dead = shot::bank(&store, from, out, &f, lands, Vec3::ZERO);
        assert!(dead > 2.0, "a shell that cannot bounce came back off the wall anyway");
    }

    /// The predictor carries the target forward on its own clock, so the
    /// answer has the lead in it: a man walking out of the way is a miss,
    /// and a man walking into it is a hit.
    #[test]
    fn the_predictor_leads_a_walking_target() {
        let store = room();
        let mut f = rocket();
        f.speed = 30.0;
        f.gravity = 0.0;
        f.bounces = 0;
        f.fuse_s = 3.0;
        f.splash = None;
        let from = Vec3::new(-4.0, 3.5, 0.0);
        let dir = Vec3::X;
        // Four metres out at 30 m/s is about an eighth of a second.
        let standing = Vec3::new(0.0, 3.5, 0.0);
        let still = shot::bank(&store, from, dir, &f, standing, Vec3::ZERO);
        assert!(still < 0.3, "a shell down a straight line missed a man on it by {still:.2} m");
        // Walking across it fast enough to be gone by the time it arrives.
        let walking = shot::bank(&store, from, dir, &f, standing, Vec3::Z * 9.0);
        assert!(walking > 0.6, "it still called it a hit on a man who had walked off: {walking:.2} m");
        // And starting aside, walking onto the line, is the shot to take.
        let onto = shot::bank(&store, from, dir, &f, standing - Vec3::Z * 1.2, Vec3::Z * 9.0);
        assert!(onto < 0.3, "it would not lead a man walking onto the line: {onto:.2} m");
    }

    /// The spade takes one ball out of the first wall it reaches and stops
    /// there, rather than tunnelling on as a bore does. Nothing within reach
    /// is nothing taken.
    #[test]
    fn the_spade_bites_the_wall_in_front_of_it() {
        let store = room();
        let mut c = Combat::new(3);
        let eye = Vec3::new(0.0, 3.5, 0.0);
        assert!(!c.bite(&store, eye, Vec3::Y, 0.45, 0.2), "it bit something 20 cm away in open air");
        assert!(c.take_edits().is_empty());
        // The floor is under the eye. A bite of 0.45 m is a ball 1.8 voxels
        // in radius, about two dozen quarter-metre voxels where the ground
        // is solid and fewer through a slab, all of them near where it
        // landed. The same ball `Game::dig` cuts out in the world.
        assert!(c.bite(&store, eye, -Vec3::Y, 0.45, 6.0), "it found no floor to bite");
        let edits = c.take_edits();
        assert!((8..40).contains(&edits.len()), "one bite took {} voxels", edits.len());
        let mid = Vec3::new(
            edits.iter().map(|(v, _)| v[0] as f32).sum::<f32>() / edits.len() as f32,
            edits.iter().map(|(v, _)| v[1] as f32).sum::<f32>() / edits.len() as f32,
            edits.iter().map(|(v, _)| v[2] as f32).sum::<f32>() / edits.len() as f32,
        ) / 4.0;
        assert!((mid.x - eye.x).abs() < 0.6 && (mid.z - eye.z).abs() < 0.6, "the hole came out at {mid:?}, not under the eye");
        assert!(mid.y < eye.y, "it bit upward: {mid:?}");
        assert!(edits.iter().all(|(_, m)| *m == AIR), "a bite put something back");
    }

    /// A burst with a `crater` takes a few voxels out of the floor it went
    /// off on: the rocket's 0.33 m is about four, wherever on a voxel it
    /// lands and whether it came straight down or skimmed in.
    #[test]
    fn a_rocket_crater_takes_a_few_voxels_out_of_the_floor() {
        let store = room();
        let mut counts = Vec::new();
        // The floor's top face is y = 2.0 m. A shell stops 6 cm short of the
        // face along its flight: straight down that is 6 cm above it, and
        // skimming in it is close to touching.
        for (x, z) in [(0.0, 0.0), (0.125, 0.125), (0.1, 0.03), (0.2, 0.17)] {
            for lift in [0.06, 0.01] {
                let mut c = Combat::new(3);
                c.crater(&store, Vec3::new(x, 2.0 + lift, z), 0.33);
                let e = c.take_edits();
                assert!(e.iter().all(|([_, y, _], m)| *y == 7 && *m == AIR), "it cut deeper than the top layer: {e:?}");
                counts.push(e.len());
            }
        }
        let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
        assert!(counts.iter().all(|n| (2..=6).contains(n)), "voxels per crater: {counts:?}");
        assert!((3.0..=5.0).contains(&mean), "a crater took {mean} voxels on average: {counts:?}");
    }

    /// A bore does not stop on a wall: it takes the wall with it, hands the
    /// voxels to the rules, and carries on to its full range. A ray without
    /// a `bore_r` leaves the room exactly as it found it.
    #[test]
    fn a_bore_eats_the_wall_and_keeps_going() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let eye = foot + Vec3::Y * 1.5;
        let fighters = [me(foot)];
        let mut f = rocket();
        f.kind = FireKind::Hitscan;
        f.splash = None;
        f.damage = 10.0;
        f.falloff = 1.0;
        f.range = 40.0;

        // Without a bore: nothing is taken out of the world at all.
        let mut c = Combat::new(3);
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
        }
        assert!(c.take_edits().is_empty(), "an ordinary ray dug a hole");

        // With one: the east wall goes, and so does what is behind it.
        f.bore_r = 0.5;
        let mut c = Combat::new(3);
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            c.fire(&mut scene, eye, eye, Vec3::X, &f, 0, 1.0);
        }
        let cuts = c.take_edits();
        assert!(!cuts.is_empty(), "the bore left the wall standing");
        assert!(cuts.iter().all(|(_, m)| *m == AIR), "a bore filled something in");
        // The room's east wall is voxels 22 and 23. A ray that stopped on
        // it could never take the far face out, and this does: the wall is
        // gone right through, which is the whole of what a bore is.
        let near = cuts.iter().filter(|([x, _, _], _)| *x == 22).count();
        let far = cuts.iter().filter(|([x, _, _], _)| *x == 23).count();
        assert!(near > 0 && far > 0, "the bore stopped in the wall: {near} voxels of the near face, {far} of the far");
        assert_eq!(near, far, "it took more of one face of the wall than the other");
        // And it is a tunnel, not a scattering: every voxel is within the
        // bore's own radius of the line it was fired down.
        for ([x, y, z], _) in &cuts {
            let mid = Vec3::new(*x as f32 + 0.5, *y as f32 + 0.5, *z as f32 + 0.5) / 4.0;
            let w = mid - eye;
            let off = (w - Vec3::X * w.x).length();
            assert!(off <= f.bore_r + 0.01, "a voxel {off:.2} m off the line went with a {} m bore", f.bore_r);
        }
    }

    /// A beam that bores does not take the whole channel the instant it is
    /// switched on: it eats the face it is held on a bite at a time, and
    /// each bite is at the face the one before it left behind. Held long
    /// enough it is through the wall, which is the bore's whole point.
    #[test]
    fn a_held_beam_eats_its_way_in() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let eye = foot + Vec3::Y * 1.5;
        let fighters = [me(foot)];
        let mut f = rocket();
        f.kind = FireKind::Beam;
        f.splash = None;
        f.damage = 10.0;
        f.falloff = 1.0;
        f.range = 40.0;
        f.bore_r = 0.5;
        let mut c = Combat::new(3);
        let dt = 1.0 / 60.0;
        let mut bites = 0;
        let mut taken = 0;
        // A second of it, in steps, with the store never actually changing:
        // the test is the rate, not the hole.
        for _ in 0..60 {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            c.hold_beam(&mut scene, eye, eye, Vec3::X, &f, 0, dt);
            let cut = c.take_edits();
            if !cut.is_empty() {
                bites += 1;
                taken += cut.len();
            }
        }
        // Three or four bites a second, and every one of them a ball of
        // voxels rather than forty metres of wall.
        assert!((3..=4).contains(&bites), "{bites} bites in a second");
        assert!(taken > 0 && taken < 2000, "a bite took {} voxels", taken / bites.max(1));
    }

    /// A shell with a `cluster` breaks into little ones where it lands, and
    /// they are real shells: they fly, they fall, and what they are worth is
    /// what they do to somebody standing where they land.
    #[test]
    fn a_cluster_shell_breaks_into_little_ones() {
        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let eye = foot + Vec3::Y * 1.5;
        // Somebody standing where it is going to land: fighter 1.
        let fighters = [me(foot), me(Vec3::new(4.0, 2.0, 0.0))];
        let mut f = rocket();
        f.speed = 20.0;
        f.gravity = 10.0;
        f.damage = 10.0;
        f.cluster = Some(crate::library::ClusterDef {
            count: 10,
            damage: 8.0,
            speed: 9.0,
            gravity: 9.0,
            cone_deg: 80.0,
            bounces: 3,
            fuse_s: 1.5,
            splash: f.splash,
            look: String::new(),
        });
        let mut c = Combat::new(5);
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            // Lobbed at the floor by his feet.
            c.fire(&mut scene, eye, eye, Vec3::new(4.0, -1.5, 0.0).normalize(), &f, 0, 1.0);
        }
        let mut most = 0;
        for _ in 0..120 {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            c.step(&mut scene, 1.0 / 60.0);
            most = most.max(c.shells.len());
        }
        assert!(most >= 10, "the shell left {most} little ones behind it");
        let on_him: f32 = c.take_hits().iter().filter(|h| h.who == 1).map(|h| h.damage).sum();
        assert!(on_him > 20.0, "the little ones did him {on_him:.0} damage");
    }

    /// The scrap gun's right button throws the hopper's own scrap out of
    /// the burst: the same shards its left button fires, and they do not
    /// bounce. Read off the weapon file, because that is where it is said.
    #[test]
    fn the_scrap_gun_lob_throws_the_same_shards() {
        let (lib, errs) = crate::library::Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let def = lib.weapons.iter().find(|w| w.name == "scrap_gun").expect("no scrap gun");
        let alt = def.alt.as_ref().expect("the scrap gun has no right button");
        let c = alt.cluster.as_ref().expect("the lob throws nothing");
        assert_eq!(c.look, def.fire.look, "the lob throws something other than scrap");
        assert_eq!(c.bounces, 0, "the little ones bounce");

        let store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let eye = foot + Vec3::Y * 1.5;
        let fighters = [me(foot)];
        let mut cb = Combat::new(11);
        {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            // Lobbed at the floor a few metres off.
            cb.fire(&mut scene, eye, eye, Vec3::new(4.0, -1.0, 0.0).normalize(), alt, 0, 1.0);
        }
        let mut most = 0;
        for _ in 0..180 {
            let mut scene = Scene {
                store: &store,
                ents: &mut ents,
                models: &[],
                fighters: &fighters,
            };
            cb.step(&mut scene, 1.0 / 60.0);
            most = most.max(cb.shells.len());
        }
        assert!(most >= c.count as usize, "the burst left {most} shards behind it");
    }

    /// A glob of sludge: the shell lands and the rules are handed voxels
    /// to lay, not a burst. Nothing is written to the world here -- the
    /// edits are all that come out of it.
    fn goo() -> FireDef {
        let mut f = rocket();
        f.speed = 24.0;
        f.gravity = 0.0;
        f.damage = 16.0;
        f.stick_s = 6.0;
        f.goo_r = 0.5;
        f.splash = Some(SplashDef {
            damage: 40.0,
            radius: 3.0,
            push: 4.0,
            self_damage: 0.5,
            self_push: 1.0,
            crater: 0.0,
            curve: 1.0,
        });
        f
    }

    /// What `Game` does with the edit queue at the end of a tick: a test
    /// that does not do this is a test where nothing combat lays down is
    /// ever in the world.
    fn apply(store: &mut Store, edits: &[([i32; 3], u8)]) {
        for &([x, y, z], m) in edits {
            store.set(x, y, z, m);
        }
    }

    /// Fire a glob and step until it has landed and settled, applying what
    /// it lays as `Game` would. The fighter stands well clear, so nothing
    /// goes off by walking into it. Returns every voxel change it made.
    fn lob(c: &mut Combat, store: &mut Store, ents: &mut Entities, from: Vec3, dir: Vec3, f: &FireDef) -> Vec<([i32; 3], u8)> {
        let fighters = [me(Vec3::new(4.6, 2.0, 4.6))];
        {
            let mut scene = Scene {
                store,
                ents,
                models: &[],
                fighters: &fighters,
            };
            c.fire(&mut scene, from, from, dir, f, 0, 1.0);
        }
        let mut all = Vec::new();
        // Ten quiet steps before it is called settled: a slump is banked up
        // over several (`sludge::FALL_S`), so a blob that has just landed
        // has nothing to say for two or three.
        let mut still = 0;
        for _ in 0..240 {
            {
                let mut scene = Scene {
                    store,
                    ents,
                    models: &[],
                    fighters: &fighters,
                };
                c.step(&mut scene, 1.0 / 60.0);
            }
            let edits = c.take_edits();
            apply(store, &edits);
            still = match c.shells.is_empty() && edits.is_empty() {
                true => still + 1,
                false => 0,
            };
            all.extend(edits);
            if still >= 10 {
                break;
            }
        }
        all
    }

    /// A glob leaves matter behind: real voxels of sludge against the face
    /// it struck, and no burst at all.
    #[test]
    fn a_glob_lands_as_voxels() {
        let mut store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(3);
        let eye = Vec3::new(0.0, 3.5, 0.0);
        let laid = lob(&mut c, &mut store, &mut ents, eye, Vec3::X, &goo());
        assert!(!laid.is_empty(), "the glob left nothing on the wall");
        assert!(laid.iter().all(|(_, m)| *m != AIR), "a glob dug a hole instead of filling one");
        // All of it is air in the room, never the wall itself: the east
        // wall starts at voxel 22, and filling a hole is not boring one.
        assert!(laid.iter().all(|([x, _, _], _)| *x < 22), "the glob wrote into the wall");
        // It is a ball about as wide as the file says, in quarter-metre
        // voxels: 0.5 m across is a handful of them, not one and not a
        // hundred.
        assert!((8..=90).contains(&laid.len()), "{} voxels for a 0.5 m glob", laid.len());
        assert!(!c.quiet(), "the map is not quiet with sludge lying on it");
    }

    /// A glob with nothing to cling to slumps: the same voxels one step
    /// lower, and the ones it has left behind go back to air. Boring away
    /// what a stack stands on drops it the same way.
    #[test]
    fn unsupported_sludge_slumps() {
        // The room with the middle of its floor taken out: nothing but air
        // under the glob.
        let mut store = room();
        for x in -20..20 {
            for z in -20..20 {
                for y in 0..8 {
                    store.set(x, y, z, AIR);
                }
            }
        }
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(3);
        let mut f = goo();
        // Nothing in its way and a fuse that runs out at once, so it goes
        // off in mid-air, where there is no face to hold on to.
        f.fuse_s = 0.1;
        f.speed = 4.0;
        f.gravity = 0.0;
        let eye = Vec3::new(0.0, 3.5, 0.0);
        let laid = lob(&mut c, &mut store, &mut ents, eye, Vec3::X, &f);
        assert!(!laid.is_empty(), "nothing was laid in the air");
        assert!(laid.iter().any(|(_, m)| *m == AIR), "it hung in the air: nothing was ever vacated");
        // It is lower than it was laid, and the voxels it passed through
        // are air again.
        let put = laid.iter().filter(|(_, m)| *m != AIR).map(|([_, y, _], _)| *y).max().expect("nothing laid");
        let rest: Vec<i32> = laid.iter().filter(|(v, m)| *m != AIR && store.get(v[0], v[1], v[2]) != Some(AIR)).map(|([_, y, _], _)| *y).collect();
        let low = rest.iter().copied().max().expect("it is nowhere at all now");
        assert!(low < put, "sludge did not slump: laid at {put} and resting at {low}");
    }

    /// A glob laid on the floor rests there, and a second lands on the
    /// first rather than in it: sludge stacks, so a doorway can be filled.
    #[test]
    fn sludge_stacks_on_sludge() {
        let mut store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(3);
        let mut f = goo();
        f.gravity = 30.0;
        // Both lobbed at the same patch of floor, one after the other, so
        // the second finds the first already lying there.
        let eye = Vec3::new(0.0, 3.5, 0.0);
        let down = Vec3::new(1.0, -1.0, 0.0).normalize();
        let one = lob(&mut c, &mut store, &mut ents, eye, down, &f);
        let two = lob(&mut c, &mut store, &mut ents, eye, down, &f);
        let top = |v: &Vec<([i32; 3], u8)>| v.iter().filter(|(_, m)| *m != AIR).map(|([_, y, _], _)| *y).max().unwrap_or(i32::MIN);
        assert!(!one.is_empty() && !two.is_empty(), "one of the globs laid nothing");
        assert!(top(&two) > top(&one), "the second glob did not stack: {} then {}", top(&one), top(&two));
        // Neither has gone off, so neither has taken anything away.
        assert!(one.iter().chain(two.iter()).all(|(_, m)| *m != AIR), "a glob burst instead of settling");
    }

    /// One blob going off sets off its neighbours, and all of it is back to
    /// air when the chain has finished.
    #[test]
    fn a_burst_sets_off_the_sludge_next_to_it() {
        let mut store = room();
        let mut ents = Entities::new(Default::default(), 1);
        let mut c = Combat::new(3);
        let mut f = goo();
        f.gravity = 30.0;
        let eye = Vec3::new(0.0, 3.5, 0.0);
        let mut put: Vec<[i32; 3]> = Vec::new();
        for k in 0..3 {
            let dir = Vec3::new(1.0, -1.0, -0.2 + 0.2 * k as f32).normalize();
            for (v, m) in lob(&mut c, &mut store, &mut ents, eye, dir, &f) {
                match m == AIR {
                    true => put.retain(|o| *o != v),
                    false => put.push(v),
                }
            }
        }
        assert!(put.len() > 20, "only {} voxels of sludge went down", put.len());
        assert!(put.iter().all(|v| store.get(v[0], v[1], v[2]) != Some(AIR)), "the sludge is not in the world");
        // Walk into the nearest one. It goes off, and its burst lights the
        // fuses of the others, which go a moment later.
        let fighters = [me(Vec3::new(1.5, 2.0, 0.0))];
        for _ in 0..120 {
            {
                let mut scene = Scene {
                    store: &store,
                    ents: &mut ents,
                    models: &[],
                    fighters: &fighters,
                };
                c.step(&mut scene, 1.0 / 60.0);
            }
            let edits = c.take_edits();
            apply(&mut store, &edits);
        }
        assert!(c.quiet(), "sludge was left on the map after the chain had gone off");
        let left: Vec<[i32; 3]> = put.iter().copied().filter(|v| store.get(v[0], v[1], v[2]) != Some(AIR)).collect();
        assert!(left.is_empty(), "{} voxels of sludge were left in the world", left.len());
        // And the man who walked into it paid for all three.
        let took: f32 = c.take_hits().iter().filter(|h| h.who == 0).map(|h| h.damage).sum();
        assert!(took > 40.0, "walking into a chain of three globs cost {took}");
    }
}
