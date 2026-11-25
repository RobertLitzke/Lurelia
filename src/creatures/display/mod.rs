//! Displays: the rare things a species now and then does, beyond getting
//! about -- a hummingbird's dive, rams butting heads, a calf nursing, cranes
//! dancing, wolves howling. Each is written once here, in a file of its own,
//! and any species can list it in its library entry (`behaviour.displays`,
//! library/creature.rs `DisplayDef`) with how often it does it, in which
//! seasons, who takes part and numbers of its own.
//!
//! A display borrows its cast from their movers for a while and hands them
//! back. While one runs, each member carries a `Show`: the walker (walk.rs),
//! the hummingbird (hummer.rs), the circlers (circle.rs) and the flitting
//! birds (flit.rs) read it and do what it says -- stand, walk to a spot, be
//! put at a point, sit there -- in place of what they would have chosen, and
//! the pose overlay (`apply`) is laid on top of the pose they compose
//! (birds.rs does it for the fliers).
//! Removing the `Show` hands a member back.
//!
//! One scheduler (`Shows`) decides what plays, so displays come as glimpses
//! rather than constant business: it only starts one the player could see,
//! keeps a few running at most, waits between them, and scales every rate by
//! one dial (`VG_DISPLAY_RATE`). A member that is startled ends the display
//! for all of it.
//!
//! Dev hooks: `VG_DISPLAY=<kind>` makes that kind start whenever it can, and
//! the game's debug key starts one on the creature under the crosshair.

pub mod bait_ball;
pub mod barrel_roll;
pub mod bask_pose;
pub mod billing;
pub mod bob;
pub mod boxing;
pub mod broken_wing;
pub mod bury;
pub mod cache;
pub mod call;
pub mod chase;
pub mod colour_flash;
pub mod courtship;
pub mod covey_call;
pub mod curl;
pub mod dance;
pub mod dig;
pub mod drop;
pub mod drum;
pub mod flash_flight;
pub mod float_play;
pub mod gallop;
pub mod gesture;
pub mod greet;
pub mod groom;
pub mod head_butt;
pub mod leap;
pub mod long_call;
pub mod march;
pub mod necking;
pub mod nurse;
pub mod orientation;
pub mod pair_bow;
pub mod patrol;
pub mod perch;
pub mod pipe;
pub mod play;
pub mod plunge;
pub mod pronk;
pub mod pulse_bloom;
pub mod push_up;
pub mod raft;
pub mod rear_strike;
pub mod roll;
pub mod rub;
pub mod sentinel;
pub mod shuttle;
pub mod sky_dance;
pub mod sky_dive;
pub mod sky_point;
pub mod snake_dance;
pub mod song_flight;
pub mod spar;
pub mod spyhop;
pub mod stalk;
pub mod streamer_flight;
pub mod surface_roll;
pub mod swarm;
pub mod sway;
pub mod sweep;
pub mod sync_flash;
pub mod tandem;
pub mod threat;
pub mod tumble;
pub mod fish;
pub mod wash;
pub mod wheel;
pub mod wing_clap;
pub mod wing_dry;
pub mod wing_stretch;

use super::age::{Age, Stage};
use super::herd::Kin;
use super::circle::Gull;
use super::flit::Flit;
use super::hummer::Hummer;
use super::sex::Sex;
use super::terrain::Terrain;
use super::walk::{Critter, Mood};
use super::{Creature, Look, Species};
use crate::anim::life::Act;
use crate::anim::{Pose, Rig, Role};
use crate::library::{CastDef, DisplayDef, MemberDef};
use crate::models::Model;
use crate::world::noise::Rng;
use glam::Vec3;
use std::collections::BTreeMap;

/// Every display, by the name an entry gives it.
pub static KINDS: [Kind; 69] = [
    sky_dive::KIND,
    head_butt::KIND,
    nurse::KIND,
    dance::KIND,
    call::KIND,
    spar::KIND,
    roll::KIND,
    sentinel::KIND,
    stalk::KIND,
    threat::KIND,
    tumble::KIND,
    cache::KIND,
    bob::KIND,
    song_flight::KIND,
    rear_strike::KIND,
    groom::KIND,
    necking::KIND,
    greet::KIND,
    rub::KIND,
    pronk::KIND,
    play::KIND,
    boxing::KIND,
    drum::KIND,
    chase::KIND,
    push_up::KIND,
    fish::KIND,
    wash::KIND,
    dig::KIND,
    march::KIND,
    pipe::KIND,
    sweep::KIND,
    wing_stretch::KIND,
    bask_pose::KIND,
    float_play::KIND,
    gallop::KIND,
    gesture::KIND,
    shuttle::KIND,
    sky_dance::KIND,
    wing_dry::KIND,
    long_call::KIND,
    billing::KIND,
    wing_clap::KIND,
    streamer_flight::KIND,
    plunge::KIND,
    wheel::KIND,
    sky_point::KIND,
    broken_wing::KIND,
    courtship::KIND,
    covey_call::KIND,
    pair_bow::KIND,
    orientation::KIND,
    tandem::KIND,
    swarm::KIND,
    sync_flash::KIND,
    flash_flight::KIND,
    bait_ball::KIND,
    drop::KIND,
    leap::KIND,
    surface_roll::KIND,
    patrol::KIND,
    bury::KIND,
    sway::KIND,
    pulse_bloom::KIND,
    raft::KIND,
    barrel_roll::KIND,
    spyhop::KIND,
    snake_dance::KIND,
    curl::KIND,
    colour_flash::KIND,
];

/// The global dial on every display's rate, and on the gap between them:
/// above 1 they come thick and fast for judging them, below 1 they become
/// rare. `VG_DISPLAY_RATE` overrides it.
pub const DIAL: f32 = 1.0;
/// At most this many displays run at once.
const MAX_RUNNING: usize = 2;
/// Seconds between one display ending and the next starting, at a dial of 1.
const GAP_S: f32 = 12.0;
/// The furthest off a display is started, metres: well inside what can be
/// watched, so it is not missed for being a speck.
const VIEW_M: f32 = 90.0;
/// How far either side of where the player faces a display may start: the
/// cosine of the half-angle, about 60 degrees.
const VIEW_COS: f32 = 0.5;
/// The drowsiest a creature may be (creatures/diel.rs) and still start a
/// display.
const AWAKE: f32 = 0.6;
/// No display runs longer than this; it is ended and the cast handed back.
const LONGEST_S: f32 = 150.0;
/// Seconds before a creature answers the player's approach with a display
/// again.
const APPROACH_AGAIN_S: f32 = 45.0;

/// One kind of display: its defaults and how to start it.
pub struct Kind {
    pub name: &'static str,
    /// Times an hour one individual starts it, when its entry gives none.
    pub rate: f32,
    /// Its flightiness multiplier while it runs (`DisplayDef::holds`).
    pub holds: f32,
    /// Who takes part, when the entry gives no `cast`.
    pub cast: fn() -> Cast,
    /// The `params` it reads, for the library check.
    pub params: &'static [&'static str],
    /// Params that take a word, and the words each takes.
    pub words: &'static [(&'static str, &'static [&'static str])],
    /// Started by the player coming near rather than at a rate, when the
    /// entry doesn't say (`DisplayDef::on_approach`).
    pub approach: bool,
    /// Bone roles the rig must have for this display to show at all.
    pub roles: &'static [Role],
    /// Whether this one can take part now: its mover is one the display
    /// knows how to drive, and it is free to.
    pub ready: fn(&Member) -> bool,
    /// Begin, with the cast found. None if it can't after all.
    pub start: fn(&[Member], &Params, &mut Rng, &Ctx) -> Option<Box<dyn Run>>,
}

/// A display under way.
pub trait Run: Send + Sync {
    /// One step: what each member (in cast order) does now, or Done.
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step;
}

pub enum Step {
    /// Carry on: the phase, for the crosshair readout, and one `Out` per
    /// member in cast order.
    On(String, Vec<Out>),
    Done,
}

/// What one member does this step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Out {
    pub drive: Drive,
    pub pose: Overlay,
    /// Walkers: the posture to hold (stand, sit, lie). Stand when None.
    pub act: Option<Act>,
    /// Walkers: an upward kick this step, m/s, if it is on the ground.
    pub hop: f32,
    /// Glowing creatures that blink (fireflies): how bright its glow is now,
    /// 0 to 1, in place of its own rhythm (entities.rs).
    pub flash: Option<f32>,
    /// Blanched pale, 0 to 1, the way a startled creature flashes (an
    /// octopus's colour ripples).
    pub pale: f32,
    /// Its food (`DisplayDef::food`) is in its bill or mouth this step.
    pub held: bool,
}

/// Where a member goes.
#[derive(Clone, Copy, Debug)]
pub enum Drive {
    /// Stand where it is, turning to `face` (a yaw) if given.
    Stay { face: Option<f32> },
    /// Walk toward `to` at `speed` m/s, stopping on it.
    Go { to: Vec3, speed: f32 },
    /// Walkers: step backward at `speed` m/s without turning.
    Back { speed: f32 },
    /// Free movers (a hummingbird): be at `pos`, facing `yaw`, flying.
    Place { pos: Vec3, yaw: f32 },
    /// Fliers: be at `pos` (its feet), facing `yaw`, perched with its wings
    /// folded. Walkers treat it as `Stay`.
    Sit { pos: Vec3, yaw: f32 },
}

impl Default for Drive {
    fn default() -> Self {
        Drive::Stay { face: None }
    }
}

/// Laid over the pose the mover composes, by bone role, so one display fits
/// every rig with the roles it uses. Angles in radians.
#[derive(Clone, Copy, Debug, Default)]
pub struct Overlay {
    /// Head and neck pitch, + up (motors::look).
    pub head: f32,
    /// Head and neck turn, + to its left.
    pub turn: f32,
    /// Wings: 0 as the mover has them, 1 spread and raised.
    pub wings: f32,
    /// The whole body pitched, + nose up (rearing).
    pub rear: f32,
    /// Tail, + up.
    pub tail: f32,
    /// Tail swung side to side, radians.
    pub wag: f32,
    /// The whole body rolled, + onto its right side (an animal rolling on
    /// its back).
    pub roll: f32,
    /// Legs pedalling: each upper leg swung by this, alternate legs the
    /// other way.
    pub kick: f32,
    /// Arms (and a crab's claws): `arms` spreads them out to the sides,
    /// `raise` lifts them forward and up.
    pub arms: f32,
    pub raise: f32,
    /// The jaw opened, radians.
    pub jaw: f32,
    /// Drawn in, 0 to 1 (`Pose::tuck`): a hedgehog curling up.
    pub tuck: f32,
    /// With `rear`: keep the low end's legs planted, 0 to 1. Rearing (rear
    /// > 0) plants the hind legs and lifts the body so the hips stay put; a
    /// skunk's handstand (rear < 0) plants the fore legs.
    pub plant: f32,
    /// The hind feet (`stamp`) or the fore feet (`paw`) swung forward, left
    /// and right alternately, radians: + is one side forward and the other
    /// back, so an oscillating value stamps or digs.
    pub stamp: f32,
    pub paw: f32,
    /// One arm (its right) raised, radians, for a wave.
    pub wave: f32,
}

/// On each member of a running display. The walker and the hummingbird read
/// it; the crosshair readout prints `kind` and `phase`.
pub struct Show {
    pub kind: &'static str,
    pub phase: String,
    pub out: Out,
    /// Multiplier on the player's pressure on it (`DisplayDef::holds`).
    pub holds: f32,
    /// The model of the food it carries (`DisplayDef::food`), drawn at its
    /// mouth while `out.held`.
    pub food: Option<usize>,
}

/// What a display sees of one member.
#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub e: hecs::Entity,
    pub pos: Vec3,
    pub yaw: f32,
    pub species: usize,
    /// The species whose quota it fills, so morphs count as their parents.
    pub kind_of: usize,
    pub stage: Stage,
    pub sex: Option<Sex>,
    pub kin: Option<u32>,
    pub height: f32,
    pub half_w: f32,
    /// How far its snout is in front of its feet, metres (Model::reach).
    pub snout: f32,
    pub mover: Mover,
    /// Its walk and run speeds, m/s, with its morph applied.
    pub slow: f32,
    pub fast: f32,
    pub wary: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mover {
    /// A walker: whether it is fleeing (or hunting, or carrying a kill), in
    /// the air, or underground.
    Walker { busy: bool },
    Hummer { perched: bool },
    /// A circler (circle.rs): `free` when it is going round its circle, not
    /// on the water or coming down to it.
    Circler { free: bool },
    /// A flitting bird (flit.rs): whether it sits on a perch or the ground.
    Flitter { perched: bool },
    /// A woodpecker (woodpecker.rs): whether it is on a trunk.
    Clinger { clinging: bool },
    /// A duck, goose or swan (fowl.rs): whether it is on the water or ashore.
    Paddler { floating: bool },
    /// An insect (insects.rs): a bee or a dragonfly.
    Buzzer,
    /// A fish of a school (swim.rs): whether it is out of the water.
    Fish { air: bool },
    /// A whale (swim.rs): whether it lies at the surface.
    Whale { surface: bool },
    /// A snake, a millipede or a slug (snake.rs): whether it is at rest.
    Serpent { resting: bool },
    /// An octopus or a lobster (octopus.rs).
    Creeper,
    /// A running bird (runner.rs): whether it is on its feet, and its style.
    Runner { grounded: bool, style: super::runner::Style },
    /// A member of a flock, or a soaring or gliding bird (flock.rs):
    /// whether it is perched, and whether it soars or glides alone.
    Flocker { perched: bool, soars: bool },
    Other,
}

impl Member {
    pub fn walker_free(&self) -> bool {
        self.mover == Mover::Walker { busy: false }
    }
    /// The way it faces, on the flat.
    pub fn facing(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
    }
    /// Whether it has got to `to`: within a body's width for a big animal,
    /// which turns too wide to stop on a point.
    pub fn at(&self, to: Vec3) -> bool {
        (to - self.pos).with_y(0.0).length() < 0.5 + self.half_w
    }
}

/// The yaw that faces from `from` toward `to` (a model faces -Z).
pub fn face(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    d.x.atan2(d.z) + std::f32::consts::PI
}

/// Moving from `a` to `b`, the yaw it faces; `or` if it isn't moving across.
pub fn heading(a: Vec3, b: Vec3, or: f32) -> f32 {
    if (b - a).with_y(0.0).length() > 1e-3 {
        face(a, b)
    } else {
        or
    }
}

/// Walkers taking up marks: each walks to `marks[i]` at its walking pace,
/// and once there stands turned to `faces[i]` wearing `pose`. Whether all
/// are there.
pub fn gather(cast: &[Member], marks: &[Vec3], faces: &[f32], pose: Overlay) -> (Vec<Out>, bool) {
    let mut all = true;
    let outs = cast
        .iter()
        .enumerate()
        .map(|(i, m)| {
            if m.at(marks[i]) {
                Out { drive: Drive::Stay { face: Some(faces[i]) }, pose, ..Default::default() }
            } else {
                all = false;
                Out { drive: Drive::Go { to: marks[i], speed: m.slow }, ..Default::default() }
            }
        })
        .collect();
    (outs, all)
}

/// A flier's flight from one point to another: eased at both ends, in a
/// shallow arc when it isn't coming down.
#[derive(Clone, Copy, Debug)]
pub struct Hop {
    pub from: Vec3,
    pub to: Vec3,
    t: f32,
    dur: f32,
    lift: f32,
}

impl Hop {
    pub fn new(from: Vec3, to: Vec3, speed: f32) -> Hop {
        let d = (to - from).length();
        let lift = if to.y < from.y - 2.0 { 0.0 } else { (0.3 + 0.12 * d).min(4.0) };
        Hop { from, to, t: 0.0, dur: (d / speed.max(0.5)).clamp(0.4, 12.0), lift }
    }

    /// Advance: where it is now, and whether it has got there.
    pub fn step(&mut self, dt: f32) -> (Vec3, bool) {
        self.t = (self.t + dt / self.dur).min(1.0);
        let e = self.t * self.t * (3.0 - 2.0 * self.t);
        (self.from.lerp(self.to, e) + Vec3::Y * self.lift * (std::f32::consts::PI * self.t).sin(), self.t >= 1.0)
    }
}

/// Somewhere for a flier to come down, `rmin..rmax` from `around` (toward
/// `toward` if given): dry ground above the sea, or with `raised` the best
/// raised perch (a post, a shrub, a tree), else the ground. Its feet's point.
pub fn spot(terrain: &Terrain, around: Vec3, rmin: f32, rmax: f32, toward: Option<Vec3>, raised: bool, rng: &mut Rng) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..14 {
        let a = match toward {
            Some(d) if d.length() > 1e-3 => d.x.atan2(d.z) + (rng.unit() - 0.5) * 1.6,
            _ => rng.unit() * std::f32::consts::TAU,
        };
        let r = rmin + rng.unit() * (rmax - rmin);
        let (x, z) = (around.x + a.sin() * r, around.z + a.cos() * r);
        let Some(g) = terrain.height(x, z) else { continue };
        if g < terrain.sea + 0.3 || terrain.wet(x, z) && !raised {
            continue;
        }
        let top = if raised { terrain.perch(x, z, g.max(around.y) + 30.0).unwrap_or(g) } else { g };
        let rise = top - g;
        if rise < 0.5 && terrain.wet(x, z) {
            continue;
        }
        // Raised: a post or a small tree (2 to 8 m) first, the ground last.
        let score = if raised && rise > 0.5 { 4.0 - (rise - 5.0).abs().max(3.0) * 0.3 } else { 0.0 } + rng.unit();
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, Vec3::new(x, top, z)));
        }
    }
    best.map(|b| b.1)
}

/// What a display steps with.
pub struct Ctx<'a> {
    pub terrain: &'a Terrain<'a>,
    /// The world's placed plants (trunks to rub on).
    pub flora: &'a super::plants::Flora,
    pub dt: f32,
    pub player: Vec3,
}

/// Who a display needs, resolved from a `CastDef` over the display's default.
#[derive(Clone, Debug)]
pub struct Cast {
    pub from: From,
    pub count: [u32; 2],
    pub range: f32,
    pub members: Vec<MemberDef>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    Solo,
    Same,
    Family,
}

impl Cast {
    pub fn solo() -> Cast {
        Cast { from: From::Solo, count: [1, 1], range: 0.0, members: Vec::new() }
    }

    /// The entry's `cast` laid over this default, field by field.
    fn over(mut self, d: Option<&CastDef>) -> Cast {
        let Some(d) = d else { return self };
        if let Some(f) = d.from.as_deref().and_then(parse_from) {
            self.from = f;
        }
        if let Some(c) = d.count {
            self.count = c;
        }
        if let Some(r) = d.range {
            self.range = r;
        }
        if !d.members.is_empty() {
            self.members = d.members.clone();
        }
        if self.from == From::Solo {
            self.count = [1, 1];
        }
        self
    }

    fn filter(&self, i: usize) -> Option<&MemberDef> {
        self.members.get(i).or(self.members.last())
    }
}

fn parse_from(s: &str) -> Option<From> {
    match s {
        "solo" => Some(From::Solo),
        "same" => Some(From::Same),
        "family" => Some(From::Family),
        _ => None,
    }
}

/// The cast the zoo (creatures/zoo.rs) puts on a stage for display `d`:
/// the most it takes, each as an (age, sex) filter, the lead first. None
/// for a kind this build doesn't know.
pub fn zoo_cast(d: &DisplayDef) -> Option<Vec<(Option<String>, Option<String>)>> {
    let k = kind(&d.kind)?;
    let cast = (k.cast)().over(d.cast.as_ref());
    let n = cast.count[1].clamp(1, 8) as usize;
    Some((0..n).map(|i| cast.filter(i).map_or((None, None), |f| (f.age.clone(), f.sex.clone()))).collect())
}

/// Does `m` meet a member filter? "adult" takes in the aged.
fn fits(m: &Member, f: Option<&MemberDef>) -> bool {
    if m.stage == Stage::Dead {
        return false;
    }
    let Some(f) = f else { return true };
    let age = match f.age.as_deref() {
        None => true,
        Some("adult") => matches!(m.stage, Stage::Adult | Stage::Aged),
        Some(a) => m.stage.name() == a,
    };
    let sex = match f.sex.as_deref() {
        None => true,
        Some(s) => m.sex.map(|x| x.name()) == Some(s),
    };
    age && sex
}

/// A display's `params` for one species, read as numbers or picked from
/// [low, high] ranges.
pub struct Params<'a>(pub &'a BTreeMap<String, serde_json::Value>);

impl Params<'_> {
    /// The number `name`, a random pick if it is a range, else `default`.
    pub fn num(&self, name: &str, default: f32, rng: &mut Rng) -> f32 {
        match self.0.get(name) {
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(default as f64) as f32,
            Some(serde_json::Value::Array(a)) if a.len() == 2 => {
                let lo = a[0].as_f64().unwrap_or(default as f64) as f32;
                let hi = a[1].as_f64().unwrap_or(lo as f64) as f32;
                lo + (hi - lo) * rng.unit()
            }
            _ => default,
        }
    }

    /// A word param, or `default`.
    pub fn word<'b>(&'b self, name: &str, default: &'b str) -> &'b str {
        self.0.get(name).and_then(|v| v.as_str()).unwrap_or(default)
    }

    /// A whole-number pick, for counts: [2, 5] gives 2 to 5.
    pub fn count(&self, name: &str, default: [u32; 2], rng: &mut Rng) -> u32 {
        let (lo, hi) = match self.0.get(name) {
            Some(serde_json::Value::Number(n)) => {
                let v = n.as_f64().unwrap_or(default[0] as f64) as u32;
                (v, v)
            }
            Some(serde_json::Value::Array(a)) if a.len() == 2 => (a[0].as_f64().unwrap_or(0.0) as u32, a[1].as_f64().unwrap_or(0.0) as u32),
            _ => (default[0], default[1]),
        };
        lo + ((hi.saturating_sub(lo) + 1) as f32 * rng.unit()) as u32 % (hi.saturating_sub(lo) + 1)
    }
}

pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

/// The global dial, with the dev override.
fn dial() -> f32 {
    static D: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var("VG_DISPLAY_RATE").ok().and_then(|s| s.parse().ok()).unwrap_or(DIAL).max(0.0))
}

/// `VG_DISPLAY=<kind>`: that kind starts whenever it can.
fn pushed() -> Option<&'static str> {
    static P: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    P.get_or_init(|| std::env::var("VG_DISPLAY").ok().filter(|s| !s.is_empty())).as_deref()
}

struct Running {
    kind: &'static Kind,
    run: Box<dyn Run>,
    cast: Vec<hecs::Entity>,
    holds: f32,
    t: f32,
}

/// The scheduler: what is running, and when the next may start.
pub struct Shows {
    running: Vec<Running>,
    gap: f32,
    rng: Rng,
    /// Start a display on this creature as soon as one can, whatever the
    /// odds (the game's debug key). Cleared once tried.
    pub forced: Option<hecs::Entity>,
    /// Which of a forced creature's displays to try first, so pressing the
    /// key again goes on to the next.
    turn: usize,
    /// Displays started and ended, for the headless stats.
    pub started: u32,
    pub ended: u32,
    /// Kind name -> times started.
    pub by_kind: BTreeMap<&'static str, u32>,
    /// Creatures that answered the player's approach lately, and seconds
    /// until they may again.
    answered: Vec<(hecs::Entity, f32)>,
    /// Seconds since the world started, for the debug log.
    clock: f32,
}

impl Default for Shows {
    fn default() -> Self {
        Shows {
            running: Vec::new(),
            gap: 4.0,
            rng: Rng::new(0x5d1_5b1a),
            forced: None,
            turn: 0,
            started: 0,
            ended: 0,
            by_kind: BTreeMap::new(),
            answered: Vec::new(),
            clock: 0.0,
        }
    }
}

/// What the scheduler is told of the world each step.
pub struct World<'a> {
    pub species: &'a [Species],
    pub models: &'a [Model],
    pub terrain: &'a Terrain<'a>,
    pub flora: &'a super::plants::Flora,
    /// The world's collectibles' models, by name, for food (`Show::food`).
    pub items: &'a crate::util::FxHashMap<String, crate::worldload::ItemModel>,
    pub player: Vec3,
    /// Where the player looks, on the flat.
    pub facing: Vec3,
    pub season: &'a str,
    pub hour: f32,
    pub sky: &'a crate::world::weather::Sky,
}

/// A member as the display sees it, or None if it is gone.
fn member(world: &hecs::World, e: hecs::Entity, w: &World) -> Option<Member> {
    let ent = world.entity(e).ok()?;
    let look = ent.get::<&Look>()?;
    let me = ent.get::<&Creature>()?;
    let sp = w.species.get(me.species)?;
    let md = w.models.get(look.model)?;
    let morph = ent.get::<&crate::variety::Morph>();
    let scale = morph.as_ref().map_or(1.0, |m| m.scale);
    let speed = morph.as_ref().map_or(1.0, |m| m.speed);
    let mover = if let Some(c) = ent.get::<&Critter>() {
        Mover::Walker { busy: c.mood != Mood::Idle && c.mood != Mood::Walk || c.air.is_some() || c.under > 0.0 }
    } else if let Some(h) = ent.get::<&Hummer>() {
        Mover::Hummer { perched: h.perched() }
    } else if let Some(g) = ent.get::<&Gull>() {
        Mover::Circler { free: g.circling() }
    } else if let Some(f) = ent.get::<&Flit>() {
        Mover::Flitter { perched: f.perched() }
    } else if let Some(c) = ent.get::<&super::woodpecker::Clinger>() {
        Mover::Clinger { clinging: c.clinging() }
    } else if let Some(f) = ent.get::<&super::swim::Fish>() {
        Mover::Fish { air: f.air }
    } else if let Some(w) = ent.get::<&super::swim::Whale>() {
        Mover::Whale { surface: w.phase == super::swim::Phase::Surface }
    } else if let Some(s) = ent.get::<&super::snake::Serpent>() {
        Mover::Serpent { resting: s.resting() }
    } else if ent.get::<&super::octopus::Octopus>().is_some() {
        Mover::Creeper
    } else if let Some(p) = ent.get::<&super::fowl::Paddler>() {
        Mover::Paddler { floating: p.floating() }
    } else if ent.get::<&super::insects::Buzzer>().is_some() {
        Mover::Buzzer
    } else if let Some(r) = ent.get::<&super::runner::Runner>() {
        Mover::Runner { grounded: r.grounded(), style: r.style }
    } else if let Some(f) = ent.get::<&super::flock::Flyer>() {
        let soars = matches!(super::locomotion::verb(&sp.beh.movement).mover, super::locomotion::Mover::Flock { soar: true, .. } | super::locomotion::Mover::Flock { glide: true, .. });
        Mover::Flocker { perched: f.perched, soars }
    } else {
        Mover::Other
    };
    Some(Member {
        e,
        pos: look.pos,
        yaw: look.yaw,
        species: me.species,
        kind_of: sp.quota,
        stage: ent.get::<&Age>().map_or(Stage::Adult, |a| a.stage),
        sex: ent.get::<&Sex>().map(|s| *s),
        kin: ent.get::<&Kin>().map(|k| k.group),
        height: md.height * scale,
        half_w: md.half_w * scale,
        snout: (-md.reach[0]).max(md.half_w) * scale,
        mover,
        slow: sp.beh.walk_speed * speed,
        fast: sp.beh.flee_speed * speed,
        wary: sp.beh.wary,
    })
}

/// Does this rig have what the display needs?
fn has_roles(md: &Model, k: &Kind) -> bool {
    k.roles.iter().all(|r| md.rig.as_ref().is_some_and(|rig| rig.bones.iter().any(|b| b.role == *r)))
}

impl Shows {
    /// Step what is running, end what is over, and perhaps start one.
    pub fn update(&mut self, world: &mut hecs::World, w: &World, dt: f32) {
        self.clock += dt;
        self.step_running(world, w, dt);
        self.gap -= dt;
        self.answered.retain_mut(|(_, t)| {
            *t -= dt;
            *t > 0.0
        });
        self.approach(world, w);
        let forced = self.forced.take();
        if let Some(e) = forced {
            if let Some(m) = member(world, e, w) {
                self.force(world, w, m);
            }
            return;
        }
        if self.running.len() >= MAX_RUNNING || (self.gap > 0.0 && pushed().is_none()) {
            return;
        }
        self.maybe_start(world, w, dt);
    }

    fn step_running(&mut self, world: &mut hecs::World, w: &World, dt: f32) {
        let mut i = 0;
        while i < self.running.len() {
            let r = &mut self.running[i];
            r.t += dt;
            let cast: Option<Vec<Member>> = r.cast.iter().map(|&e| member(world, e, w)).collect();
            let over = match &cast {
                None => true,
                Some(c) => {
                    // A startled walker has bolted (walk.rs scales its alarm
                    // by `holds`); anything else leaves when the player is
                    // closer than its nerve, `holds` of the way.
                    r.t > LONGEST_S
                        || c.iter().any(|m| match m.mover {
                            Mover::Walker { busy } => busy,
                            _ => r.holds > 0.0 && (m.pos - w.player).length() < m.wary * r.holds,
                        })
                }
            };
            let step = if over {
                Step::Done
            } else {
                let ctx = Ctx { terrain: w.terrain, flora: w.flora, dt, player: w.player };
                r.run.step(cast.as_deref().unwrap(), &ctx, &mut self.rng)
            };
            match step {
                Step::Done => {
                    let r = self.running.swap_remove(i);
                    if debug() {
                        eprintln!("VG_DISPLAY: {} ended after {:.1} s{}", r.kind.name, r.t, if over { " (cut short)" } else { "" });
                    }
                    for e in r.cast {
                        let _ = world.remove_one::<Show>(e);
                    }
                    self.ended += 1;
                    self.gap = GAP_S / dial().max(0.01);
                    continue;
                }
                Step::On(phase, outs) => {
                    for (k, (e, out)) in r.cast.iter().zip(outs).enumerate() {
                        if let Ok(mut s) = world.get::<&mut Show>(*e) {
                            if k == 0 && debug() && s.phase != phase {
                                let at = cast.as_deref().map_or(Vec3::ZERO, |c| c[0].pos);
                                eprintln!("VG_DISPLAY: {} at {:.1} s: {phase} ({:.1},{:.1},{:.1})", r.kind.name, r.t, at.x, at.y, at.z);
                            }
                            s.phase.clone_from(&phase);
                            s.out = out;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Everyone a display could involve: creatures of a species with
    /// displays, within reach of the player, and not already in one.
    /// The zoo's animals (creatures/zoo.rs) are left out, except that a
    /// zoo stage (`stage`) draws on its own cast and nothing else.
    fn pool(world: &hecs::World, w: &World, stage: Option<u32>) -> Vec<Member> {
        let reach = VIEW_M + 40.0;
        let ids: Vec<hecs::Entity> = world
            .query::<(hecs::Entity, &Creature, &Look, Option<&Show>, Option<&super::zoo::Kept>)>()
            .iter()
            .filter(|(_, c, l, s, k)| {
                s.is_none()
                    && k.map(|k| k.group) == stage
                    && w.species.get(c.species).is_some_and(|sp| !sp.beh.displays.is_empty())
                    && (l.pos - w.player).length() < reach
            })
            .map(|(e, ..)| e)
            .collect();
        ids.into_iter().filter_map(|e| member(world, e, w)).collect()
    }

    fn maybe_start(&mut self, world: &mut hecs::World, w: &World, dt: f32) {
        let pool = Self::pool(world, w, None);
        let dial = dial();

        for m in &pool {
            let to = m.pos - w.player;
            let flat = to.with_y(0.0);
            let d = flat.length();
            // In front of the player, near enough to watch, and further than
            // it would mind. A kind pushed with VG_DISPLAY needs only to be
            // near, so a headless run can find it whichever way it looks.
            let seen = d >= m.wary * 0.8 && flat.normalize_or_zero().dot(w.facing) >= VIEW_COS;
            if d > VIEW_M || !seen && pushed().is_none() {
                continue;
            }
            let sp = &w.species[m.species];
            // Awake enough: a deer at midday nurses its fawn, one bedded
            // at midnight doesn't.
            if super::diel::drowsy(sp.beh.period(), w.hour).max(w.sky.sheltering()) > AWAKE {
                continue;
            }
            for def in &sp.beh.displays {
                let Some(k) = kind(&def.kind) else { continue };
                if def.on_approach.unwrap_or(k.approach) {
                    continue;
                }
                let push = pushed() == Some(k.name);
                if !def.season.is_empty() && !def.season.iter().any(|s| s == w.season) && !push {
                    continue;
                }
                let p = if push { 1.0 } else { def.rate.unwrap_or(k.rate) * dial / 3600.0 * dt };
                if self.rng.unit() >= p {
                    continue;
                }
                if self.begin(world, w, k, def, m, &pool) {
                    return;
                }
            }
        }
    }

    /// Displays that answer the player coming near: started on anything
    /// free within the display's `near` (by default a little outside where
    /// it would run), whatever is already running.
    fn approach(&mut self, world: &mut hecs::World, w: &World) {
        let close: Vec<hecs::Entity> = world
            .query::<(hecs::Entity, &Creature, &Look, Option<&Show>)>()
            .without::<&super::zoo::Kept>()
            .iter()
            .filter(|(e, c, l, s)| {
                s.is_none()
                    && (l.pos - w.player).length() < 40.0
                    && !self.answered.iter().any(|(a, _)| a == e)
                    && w.species.get(c.species).is_some_and(|sp| sp.beh.displays.iter().any(|d| kind(&d.kind).is_some_and(|k| d.on_approach.unwrap_or(k.approach))))
            })
            .map(|(e, ..)| e)
            .collect();
        if close.is_empty() {
            return;
        }
        let pool = Self::pool(world, w, None);
        for e in close {
            let Some(m) = pool.iter().find(|m| m.e == e).copied() else { continue };
            let sp = &w.species[m.species];
            for def in &sp.beh.displays {
                let Some(k) = kind(&def.kind) else { continue };
                if !def.on_approach.unwrap_or(k.approach) {
                    continue;
                }
                let near = Params(&def.params).num("near", (m.wary * 1.3).max(6.0), &mut self.rng);
                if (m.pos - w.player).length() > near {
                    continue;
                }
                self.answered.push((e, APPROACH_AGAIN_S));
                if self.begin(world, w, k, def, &m, &pool) {
                    break;
                }
            }
        }
    }

    /// The debug key: the creature's displays, from the one after the last
    /// tried, until one starts.
    fn force(&mut self, world: &mut hecs::World, w: &World, m: Member) {
        let defs = &w.species[m.species].beh.displays;
        let pool = Self::pool(world, w, None);
        for k in 0..defs.len() {
            let i = (self.turn + k) % defs.len();
            if let Some(kd) = kind(&defs[i].kind) {
                if self.begin(world, w, kd, &defs[i], &m, &pool) {
                    self.turn = i + 1;
                    return;
                }
            }
        }
    }

    /// Find the cast round `first` and start. False if it couldn't.
    fn begin(&mut self, world: &mut hecs::World, w: &World, k: &'static Kind, def: &DisplayDef, first: &Member, pool: &[Member]) -> bool {
        if !has_roles(&w.models[w.species[first.species].model], k) && !k.roles.is_empty() {
            return false;
        }
        let cast = (k.cast)().over(def.cast.as_ref());
        if !(k.ready)(first) || !fits(first, cast.filter(0)) {
            return false;
        }
        let mut members = vec![*first];
        let most = cast.count[1].max(1) as usize;
        while members.len() < most {
            let slot = members.len();
            let next = pool
                .iter()
                .filter(|o| !members.iter().any(|x| x.e == o.e))
                .filter(|o| match cast.from {
                    From::Solo => false,
                    From::Same => o.kind_of == first.kind_of,
                    From::Family => o.kind_of == first.kind_of && o.kin.is_some() && o.kin == first.kin,
                })
                .filter(|o| (o.pos - first.pos).length() < cast.range && (k.ready)(o) && fits(o, cast.filter(slot)))
                .min_by(|a, b| (a.pos - first.pos).length().total_cmp(&(b.pos - first.pos).length()));
            match next {
                Some(o) => members.push(*o),
                None => break,
            }
        }
        if members.len() < cast.count[0].max(1) as usize {
            return false;
        }
        let ctx = Ctx { terrain: w.terrain, flora: w.flora, dt: 0.0, player: w.player };
        let Some(run) = (k.start)(&members, &Params(&def.params), &mut self.rng, &ctx) else {
            return false;
        };
        let holds = def.holds.unwrap_or(k.holds);
        let food = def.food.as_ref().and_then(|n| w.items.get(n).map(|i| i.model).or_else(|| w.species.iter().find(|s| s.name == *n).map(|s| s.model)));
        for m in &members {
            let food = if m.e == first.e { food } else { None };
            let _ = world.insert_one(m.e, Show { kind: k.name, phase: "starting".into(), out: Out::default(), holds, food });
        }
        if debug() {
            eprintln!("VG_DISPLAY: {} by {} {} at {:.0},{:.0}, clock {:.1} s", k.name, members.len(), w.species[first.species].name, first.pos.x, first.pos.z, self.clock);
        }
        self.running.push(Running { kind: k, run, cast: members.iter().map(|m| m.e).collect(), holds, t: 0.0 });
        self.started += 1;
        *self.by_kind.entry(k.name).or_insert(0) += 1;
        true
    }

    /// Start display `def` of `lead`'s species on its zoo stage
    /// (creatures/zoo.rs), with the cast the zoo put there. False if it
    /// couldn't.
    pub fn stage(&mut self, world: &mut hecs::World, w: &World, lead: hecs::Entity, def: usize) -> bool {
        let Some(group) = world.get::<&super::zoo::Kept>(lead).ok().map(|k| k.group) else { return false };
        let Some(m) = member(world, lead, w) else { return false };
        let Some(d) = w.species[m.species].beh.displays.get(def) else { return false };
        let Some(k) = kind(&d.kind) else { return false };
        let pool = Self::pool(world, w, Some(group));
        self.begin(world, w, k, d, &m, &pool)
    }

    /// The kinds running now, for the headless stats.
    pub fn now(&self) -> Vec<&'static str> {
        self.running.iter().map(|r| r.kind.name).collect()
    }
}

fn debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VG_DISPLAY").is_some() || std::env::var_os("VG_DISPLAY_LOG").is_some())
}

/// Lay a display's overlay on a composed pose. `voxel` is the model's voxel
/// size in metres, scaled to the individual.
pub fn apply(rig: &Rig, pose: &mut Pose, o: &Overlay, voxel: f32) {
    use crate::anim::motors::upper_leg;
    crate::anim::motors::look(rig, pose, o.turn, o.head);
    if let Some(b) = pose.bones.first_mut() {
        b.x += o.rear;
        b.z += o.roll;
    }
    // The legs at the low end stay on the ground: the body is lifted (or
    // dropped) so their hips keep their height, and they are turned back
    // against the tip so they stand upright under it.
    let last = rig.bones.iter().filter(|b| b.role == Role::Leg).map(|b| b.pair).max().unwrap_or(0);
    let planted = |i: usize| upper_leg(rig, i) && last > 0 && if o.rear > 0.0 { rig.bones[i].pair == last } else { rig.bones[i].pair == 0 };
    if o.plant > 0.0 && o.rear != 0.0 {
        let hips: Vec<Vec3> = (0..rig.bones.len()).filter(|&i| planted(i)).map(|i| rig.bones[i].joint - rig.bones[0].joint).collect();
        if !hips.is_empty() {
            let h = hips.iter().sum::<Vec3>() / hips.len() as f32;
            let drop = h.y * (o.rear.cos() - 1.0) - h.z * o.rear.sin();
            pose.root.y -= drop * voxel * o.plant;
        }
    }
    let tails = rig.bones.iter().filter(|b| b.role == Role::Tail).count().max(1) as f32;
    for (i, b) in rig.bones.iter().enumerate() {
        if o.plant > 0.0 && planted(i) {
            pose.bones[i].x -= o.rear * o.plant;
        }
        if o.kick != 0.0 && upper_leg(rig, i) && !(o.plant > 0.0 && planted(i)) {
            let odd = (b.pair + (b.side > 0.0) as u32) % 2 == 1;
            pose.bones[i].x += if odd { -o.kick } else { o.kick };
        }
        if (o.stamp != 0.0 || o.paw != 0.0) && upper_leg(rig, i) && last > 0 {
            let s = if b.side > 0.0 { 1.0 } else { -1.0 };
            if b.pair == last {
                pose.bones[i].x += o.stamp * s;
            } else if b.pair == 0 {
                pose.bones[i].x += o.paw * s;
            }
        }
        // Curling up: the head and the legs drawn in to the body.
        let base_of_head = matches!(b.role, Role::Head | Role::Neck) && b.parent.is_some_and(|p| !matches!(rig.bones[p].role, Role::Head | Role::Neck));
        if o.tuck > 0.0 && (base_of_head || upper_leg(rig, i)) {
            if pose.tuck.len() < rig.bones.len() {
                pose.tuck.resize(rig.bones.len(), 0.0);
            }
            pose.tuck[i] = pose.tuck[i].max(o.tuck.min(1.0));
        }
        match b.role {
            Role::Tail => {
                pose.bones[i].x -= o.tail / tails;
                pose.bones[i].y += o.wag / tails.sqrt();
            }
            Role::Jaw => pose.bones[i].x -= o.jaw,
            Role::Arm if b.parent.is_none_or(|q| rig.bones[q].role != Role::Arm) => {
                pose.bones[i].z += b.side * o.arms;
                pose.bones[i].x -= o.raise + if b.side > 0.0 { o.wave } else { 0.0 };
            }
            Role::Wing if o.wings > 0.0 => {
                // Inner bones raised into a V, tips held out flat: the open
                // wings of a crane's dance. The mover's fold is eased out.
                let inner = b.parent.is_none_or(|q| rig.bones[q].role != Role::Wing);
                let open = if inner { Vec3::new(0.0, b.side * 0.15, b.side * 0.55) } else { Vec3::new(0.0, 0.0, -b.side * 0.25) };
                pose.bones[i] = pose.bones[i].lerp(open, o.wings.clamp(0.0, 1.0));
            }
            _ => {}
        }
    }
}

/// The lowest point of a body in `pose`, frame metres: its bones' hull
/// voxels, or their joints for a rig made in code.
fn lowest(rig: &Rig, pose: &Pose, voxel: f32) -> f32 {
    let mut at = Vec::new();
    crate::anim::forward(rig, pose, voxel, Vec3::ZERO, &mut at);
    let mut lo = f32::MAX;
    for (i, b) in rig.bones.iter().enumerate() {
        let (p, q) = at[i];
        if b.hull.is_empty() {
            lo = lo.min(p.y);
        }
        for h in &b.hull {
            lo = lo.min((p + q * ((*h - b.joint) * voxel)).y);
        }
    }
    lo
}

/// Walkers: after the overlay `o` rolled, tucked or tipped the body (a
/// horse on its back, a seal basking, a skunk's handstand), lift it so
/// nothing goes lower than it was in the pose the mover made (`before`):
/// the body stays on the ground, not in it. Rearing with `plant` is lifted
/// by `apply` already.
pub fn keep_up(rig: &Rig, before: &Pose, pose: &mut Pose, o: &Overlay, voxel: f32) {
    let tips = o.roll.abs() > 0.05 || o.tuck > 0.0 || (o.rear.abs() > 0.05 && o.plant <= 0.0);
    if !tips || rig.bones.is_empty() {
        return;
    }
    let sink = lowest(rig, before, voxel) - lowest(rig, pose, voxel);
    if sink > 0.0 {
        pose.root.y += sink;
    }
}

/// The library check: each display names a kind, reads only that kind's
/// params, names a cast it can use, and the creature has the bone roles and
/// the mover the display needs.
pub fn validate(name: &str, defs: &[DisplayDef], movement: &str, roles: &[Role], out: &mut Vec<String>) {
    for d in defs {
        let Some(k) = kind(&d.kind) else {
            out.push(format!("creature {name}: unknown display {}, not one of {:?}", d.kind, KINDS.iter().map(|k| k.name).collect::<Vec<_>>()));
            continue;
        };
        for (p, v) in &d.params {
            if !k.params.contains(&p.as_str()) && !k.words.iter().any(|(w, _)| w == p) {
                out.push(format!("creature {name}: display {} reads no param {p} (it reads {:?})", k.name, k.params));
            }
            if let Some((_, ok)) = k.words.iter().find(|(w, _)| w == p) {
                if !v.as_str().is_some_and(|s| ok.contains(&s)) {
                    out.push(format!("creature {name}: display {} param {p} must be one of {ok:?}", k.name));
                }
            }
        }
        for r in k.roles {
            if !roles.contains(r) {
                out.push(format!("creature {name}: display {} needs a rig with a {r:?} bone", k.name));
            }
        }
        let mover = super::locomotion::verb(movement).mover;
        let walker = matches!(mover, super::locomotion::Mover::Walk { .. });
        let hummer = matches!(mover, super::locomotion::Mover::Hum);
        let circle = matches!(mover, super::locomotion::Mover::Circle { .. });
        let flit = matches!(mover, super::locomotion::Mover::Flit);
        let cling = matches!(mover, super::locomotion::Mover::Cling);
        let run = matches!(mover, super::locomotion::Mover::Run { .. });
        let paddle = matches!(mover, super::locomotion::Mover::Paddle);
        let school = matches!(mover, super::locomotion::Mover::School { .. });
        let breach = matches!(mover, super::locomotion::Mover::Breach { .. });
        let slither = matches!(mover, super::locomotion::Mover::Slither { .. });
        let creep = matches!(mover, super::locomotion::Mover::Creep { .. });
        let buzz = matches!(mover, super::locomotion::Mover::Buzz { .. });
        let flock = matches!(mover, super::locomotion::Mover::Flock { soar: false, glide: false });
        let glide = matches!(mover, super::locomotion::Mover::Flock { glide: true, .. });
        let fits_mover = match k.name {
            "sky_dive" | "shuttle" => hummer,
            "sky_dance" => circle || matches!(mover, super::locomotion::Mover::Flock { soar: true, .. }),
            "tumble" | "wing_dry" | "long_call" | "billing" | "wing_clap" => circle,
            "cache" => circle || flit || cling,
            "bob" => circle || flit,
            "song_flight" | "streamer_flight" | "plunge" => flit,
            "drum" => walker || cling,
            "wheel" => flock || run,
            "sky_point" => glide,
            "covey_call" => run || flock,
            "broken_wing" | "courtship" => run,
            "pair_bow" => paddle,
            "orientation" | "tandem" => buzz,
            "swarm" | "flash_flight" => circle,
            "sync_flash" => circle || buzz,
            "bait_ball" | "drop" | "leap" | "surface_roll" | "patrol" | "bury" | "sway" | "pulse_bloom" | "raft" => school,
            "barrel_roll" => circle,
            "spyhop" => breach,
            "snake_dance" | "curl" => slither,
            "colour_flash" => creep,
            _ => walker,
        };
        if !fits_mover {
            out.push(format!("creature {name}: display {} can't drive the {movement} movement", k.name));
        }
        if let Some(c) = &d.cast {
            if c.from.as_deref().is_some_and(|f| parse_from(f).is_none()) {
                out.push(format!("creature {name}: display {}: cast from must be solo, same or family", k.name));
            }
            if c.count.is_some_and(|[lo, hi]| lo == 0 || lo > hi) {
                out.push(format!("creature {name}: display {}: cast count must be [least, most] with 1 <= least <= most", k.name));
            }
            for m in &c.members {
                if m.age.as_deref().is_some_and(|a| !["juvenile", "adult", "aged"].contains(&a)) {
                    out.push(format!("creature {name}: display {}: member age must be juvenile, adult or aged", k.name));
                }
                if m.sex.as_deref().is_some_and(|s| Sex::parse(s).is_none()) {
                    out.push(format!("creature {name}: display {}: member sex must be female or male", k.name));
                }
            }
        }
        for s in &d.season {
            if crate::world::season::Season::parse(s).is_none() {
                out.push(format!("creature {name}: display {}: unknown season {s}", k.name));
            }
        }
        if d.rate.is_some_and(|r| r < 0.0) || d.holds.is_some_and(|h| h < 0.0) {
            out.push(format!("creature {name}: display {}: rate and holds can't be negative", k.name));
        }
    }
}
