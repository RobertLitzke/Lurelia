//! The zombie round (notes/ZOMBIES.md): the player and up to three bots
//! cross an explore world in three legs, two to refuges and the last to an
//! extraction, through zombies a director sends.
//!
//! It is the shooter's rules played on an open world, so everything a match
//! has is here for nothing: real guns, health, pads, corpses, the HUD. What
//! this file adds:
//!
//! - the leg: a start near where the player stands and a refuge 400-750 m
//!   away by a walking route, both taken from the world survey's anchors,
//!   with caches of guns, rounds and medical kits beside the route;
//! - the party: bots on the player's side with the `Companion` brain, which
//!   follow or hold (X), and never run dry;
//! - the zombies: a pool of bots with the `Zombie` brain, woken and put to
//!   sleep by the director, each sent along a flow field over the survey's
//!   person-passable grid toward the nearest survivor;
//! - the director: an intensity cycle (build-up, peak, relief) with a floor
//!   under it, and noise (rounds fired) that buys extra hordes.
//!
//! The bots are fighters 1.. as in any match: the party first, then the
//! zombie slots. A slot that is not in play is a dead fighter whose respawn
//! never comes; the director revives it somewhere and lets it die again.

use super::{new_fighter, stand, Shooter};
use crate::brain::{self, companion::Companion, person, zombie, Bot, Goal, Order, Role};
use crate::creatures::terrain::voxel_ground;
use crate::library::Library;
use crate::player::Player;
use crate::weapon_kit::KitDef;
use crate::world::fields::{PickupDef, ShooterDef, World};
use crate::world::noise::Rng;
use crate::world::store::Store;
use crate::world::survey::graph::Kind;
use crate::world::survey::{Chan, Survey, CELL_M};
use glam::Vec3;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Zombie slots: the most that can be up at once, and a few spare so a slot
/// whose corpse is still lying there is not the only one free.
const SLOTS: usize = 40;
const MOST_UP: usize = 34;
/// A zombie's health. Five rifle rounds, one shotgun blast at a few metres,
/// two swings of the shovel.
const ZOMBIE_HP: f32 = 82.5;
/// How fast a zombie goes, metres a second: one after the party a little
/// faster than you walk (5) and well under your sprint (8.3), a runner
/// nearer the sprint, one milling about at a shuffle.
const CHASE_MPS: f32 = 6.0;
const RUNNER_MPS: f32 = 7.2;
const IDLE_MPS: f32 = 1.4;
/// A crawler, down on its front dragging itself along: its pace after the
/// party and milling about, its body's height, and the share of the ambient
/// zombies that crawl. It climbs nothing.
const CRAWL_MPS: f32 = 1.9;
const CRAWL_IDLE_MPS: f32 = 0.6;
pub(super) const CRAWL_H: f32 = 0.7;
const CRAWLER_SHARE: f32 = 0.1;
/// Seconds between a chasing zombie's looks for the party: each look it
/// takes where they are as the place to make for, and between looks it
/// makes for that, not for where they have gone since.
const LOOK_S: (f32, f32) = (1.0, 4.0);
/// The most it goes off the straight line to that place, radians (20 deg),
/// and the distance inside which it goes straight.
const OFF_LINE: f32 = 0.35;
const STRAIGHT_M: f32 = 8.0;
/// Seconds it looks round the place it last saw them before it gives up:
/// at least, and up to this more. And its pace while it looks.
const SEARCH_S: (f32, f32) = (10.0, 5.0);
const SEARCH_MPS: f32 = 2.4;
/// What a blow to a zombie's head comes to, from any weapon: more than it has.
pub(super) const HEADSHOT: f32 = 1000.0;
/// Metres inside which a cache with its gun still in it is marked on screen.
const CACHE_SHOWN_M: f32 = 250.0;
/// How many zombies about at average difficulty, against the first cut's
/// numbers; each level either side adds or takes this much again.
const CROWD: f32 = 0.85;
const CROWD_STEP: f32 = 0.12;
/// How many zombies there are by the size of the party, against a party of
/// three bots: each bot is a gun, so fewer of them face fewer zombies. One
/// bot, the launch screen's default, has 0.6 of three bots' share.
const PARTY_SHARE: [f32; 4] = [0.4, 0.6, 0.8, 1.0];
/// Where each of the party walks round the player, metres to the side
/// (right positive) and ahead (negative is behind): one at the back, one
/// out to each flank, so between them they see every way.
const FORMATION: [(f32, f32); 3] = [(0.0, -4.5), (-3.4, -1.0), (3.4, -1.0)];
/// How near a zombie has to be to land a blow, metres, and the seconds
/// between its blows.
const REACH_M: f32 = 1.45;
const BLOW_S: f32 = 1.15;
/// A blow at average difficulty, and how much more or less for each level
/// either side.
const BLOW: f32 = 15.0;
const BLOW_STEP: f32 = 2.0;
/// After a blow the zombie stands for this long, seconds, and whoever it
/// hit is held: they keep `HELD_PACE` less of their pace for each zombie
/// that has hold of them, down to `HELD_LEAST`. One can be got away from at
/// a sprint while it stands; two or three together hold you there.
pub(super) const GRAB_S: f32 = 1.0;
const HELD_PACE: f32 = 0.35;
const HELD_LEAST: f32 = 0.25;
/// Where the director puts zombies, metres from the player: out of sight and
/// out of the way, but not so far they never arrive.
const SPAWN_NEAR: f32 = 42.0;
const SPAWN_FAR: f32 = 85.0;
/// Further than this from every survivor, a zombie is put back in the pool.
const LOSE_M: f32 = 115.0;
/// Metres behind the party past which a zombie out of the player's sight
/// is put back in the pool, so the party is not followed by a long tail of
/// them while the way ahead is empty.
const BEHIND_M: f32 = 50.0;
/// The share of zombies, ambient and hordes alike, that come in from the
/// way ahead (the route about `AHEAD_M` on); the rest come from anywhere.
const AHEAD_SHARE: f32 = 0.8;
const AHEAD_M: f32 = 70.0;
/// How far a shot carries to the zombies, metres: every idle zombie inside
/// it turns toward the party.
const SHOT_WAKES_M: f32 = 70.0;
/// Cells a side of the flow field, centred on the player: 100 cells of 4 m.
const FLOW_W: i32 = 100;
/// Cost of a step to a side and on the diagonal.
const ORTHO: u32 = 10;
const DIAG: u32 = 14;
/// How long a leg is by the walking route, metres: what is wanted, and what
/// is taken when nothing fits.
const LEG: (f32, f32) = (400.0, 750.0);
const LEG_WIDE: (f32, f32) = (250.0, 1100.0);
/// Legs in a round: two to refuges, the last to the extraction.
const LEGS: usize = 3;
/// How much further each leg presses than the one before: leg two has a
/// quarter more zombies than leg one, leg three half as many again.
const LEG_PRESS: f32 = 0.25;
/// Metres from a refuge the player goes before the next leg starts.
const REFUGE_LEAVE_M: f32 = 25.0;
/// The holdout at the extraction, seconds: at least, and up to this more.
const FINALE_S: (f32, f32) = (240.0, 120.0);
/// Seconds between the finale's hordes.
const FINALE_HORDE_S: f32 = 20.0;
/// The director's cycle, seconds: each phase lasts at least the first number
/// and up to the second more. The round's first build-up and the one that
/// starts each later leg have lengths of their own.
const FIRST_BUILD_S: (f32, f32) = (60.0, 30.0);
const LEG_BUILD_S: (f32, f32) = (45.0, 25.0);
const BUILD_S: (f32, f32) = (55.0, 35.0);
const PEAK_S: (f32, f32) = (25.0, 15.0);
const RELAX_S: (f32, f32) = (25.0, 15.0);
/// The party is pressed under this share of the player's health, or with
/// this many zombies within `PRESSED_M`; a build-up then waits up to
/// `PRESSED_WAIT_S` past its end before the peak.
const PRESSED_HEALTH: f32 = 0.35;
const PRESSED_CROWD: usize = 7;
const PRESSED_M: f32 = 12.0;
const PRESSED_WAIT_S: f32 = 30.0;
/// A peak ends early under this share of health, once it is this old.
const PEAK_BAIL: (f32, f32) = (0.25, 8.0);
/// A peak's hordes, seconds between them, and how many come in each: at
/// least, up to this more, and this more again once the first minute is
/// out, before the leg's press.
const PEAK_WAVES: u32 = 3;
const WAVE_GAP_S: f32 = 5.0;
const WAVE_SIZE: (f32, f32, f32) = (5.0, 4.0, 2.0);
/// Noise: heat (rounds fired, weighted) fades over `HEAT_FADE_S`; over
/// `NOISE_HORDE` it buys a horde of `NOISE_SIZE`, at most every `NOISE_GAP_S`,
/// which spends `NOISE_COST` of it.
const HEAT_FADE_S: f32 = 40.0;
const NOISE_HORDE: f32 = 45.0;
const NOISE_COST: f32 = 30.0;
const NOISE_GAP_S: f32 = 40.0;
const NOISE_SIZE: (f32, f32) = (8.0, 6.0);
/// Ambient zombies wanted within `AMBIENT_M` of the player, before the
/// press. A build-up starts at the first number, and adds the second once
/// the first minute is out and the third over the build-up; a peak has the
/// first and the second after the first minute; a breather the floor.
const AMBIENT_M: f32 = 70.0;
const AMBIENT_BUILD: (f32, f32, f32) = (3.0, 4.0, 7.0);
const AMBIENT_PEAK: (f32, f32) = (6.0, 4.0);
const AMBIENT_FLOOR: f32 = 2.0;
/// Seconds between tries at bringing up an ambient zombie, in a breather,
/// and in the finale.
const SPAWN_GAP_S: f32 = 0.7;
const RELAX_SPAWN_GAP_S: f32 = 4.0;
const FINALE_SPAWN_GAP_S: f32 = 0.8;
/// The finale's hordes (at least, up to this more), and the crowd about
/// between them, before the press.
const FINALE_SIZE: (f32, f32) = (6.0, 5.0);
const FINALE_CROWD: f32 = 8.0;
/// The share of a horde that runs.
const RUNNER_SHARE: f32 = 0.3;
/// Metres from the extraction that count as getting out once it is open.
const OUT_M: f32 = 5.0;
/// Metres from the refuge that count as arriving.
const ARRIVE_M: f32 = 6.0;

/// Where the director is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Pressure rising: ambient zombies, more of them as it goes on.
    Build,
    /// A horde, and the ambient ones besides.
    Peak,
    /// A breather: nothing new comes.
    Relax,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Build => "build",
            Phase::Peak => "peak",
            Phase::Relax => "relax",
        }
    }
}

/// How the round ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Made,
    Lost,
}

/// Where the round stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// On the way to the leg's goal.
    Leg,
    /// At a refuge between legs: nothing new comes until the player leaves.
    Refuge,
    /// At the extraction, holding out until the way out opens.
    Finale,
    /// The way out is open: get to it.
    Out,
}

/// One leg of the round: where it ends and the way there.
#[derive(Clone)]
pub struct Leg {
    pub goal: Vec3,
    /// The zone the goal is in.
    pub name: String,
    /// Metres by the walking route, and the route itself, a point every cell.
    pub route_m: f32,
    pub route: Vec<Vec3>,
}

/// The round's condition (notes/ZOMBIES.md "Round conditions"): the weather
/// and hour it is played in, and what that does to sight, sound and scent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Clear,
    Night,
    Rain,
    Fog,
    Wind,
}

impl Condition {
    pub const ALL: [Condition; 5] = [Condition::Clear, Condition::Night, Condition::Rain, Condition::Fog, Condition::Wind];

    /// As the launch screen and the end screen show it.
    pub fn title(self) -> &'static str {
        match self {
            Condition::Clear => "Clear day",
            Condition::Night => "Night",
            Condition::Rain => "Rain",
            Condition::Fog => "Heavy fog",
            Condition::Wind => "Dry windstorm",
        }
    }

    pub fn parse(s: &str) -> Option<Condition> {
        Condition::ALL.into_iter().find(|c| c.title().eq_ignore_ascii_case(s) || format!("{c:?}").eq_ignore_ascii_case(s))
    }

    /// The weather (world/weather.rs `Kind`, by name) and hour it is held at.
    pub fn sky(self) -> (&'static str, f32) {
        match self {
            Condition::Clear => ("sun", 11.0),
            Condition::Night => ("clouds", 23.0),
            Condition::Rain => ("rain", 14.0),
            Condition::Fog => ("fog", 8.0),
            // The breeziest the weather has; the scent is the rules'.
            Condition::Wind => ("clouds", 15.0),
        }
    }

    /// Metres a chasing zombie keeps track of somebody it can see.
    fn track_m(self) -> f32 {
        match self {
            Condition::Night => 28.0,
            Condition::Fog => 16.0,
            _ => 70.0,
        }
    }

    /// Metres a zombie sees somebody at, and wakes for anybody at whatever
    /// it can see.
    fn sight_m(self) -> f32 {
        match self {
            Condition::Night => 18.0,
            Condition::Fog => 12.0,
            _ => 35.0,
        }
    }

    fn close_m(self) -> f32 {
        match self {
            Condition::Night => 12.0,
            Condition::Fog => 10.0,
            _ => 16.0,
        }
    }

    /// The most a party bot sees, metres.
    fn party_sight(self) -> f32 {
        match self {
            Condition::Night => 35.0,
            Condition::Fog => 20.0,
            _ => f32::INFINITY,
        }
    }

    /// How far a shot carries and how much it counts, against a clear day.
    fn noise(self) -> f32 {
        if self == Condition::Rain {
            0.5
        } else {
            1.0
        }
    }

    /// Metres downwind of the party an idle zombie smells it.
    fn scent_m(self) -> f32 {
        match self {
            Condition::Wind => 40.0,
            Condition::Rain => 10.0,
            _ => 20.0,
        }
    }

    /// The nearest a zombie is brought up: closer where nobody sees far.
    fn spawn_near(self) -> f32 {
        match self {
            Condition::Night => 32.0,
            Condition::Fog => 26.0,
            _ => SPAWN_NEAR,
        }
    }
}

/// A body climbing over something: up the near face, across the top, down
/// to `to`. `t` and `len` are seconds.
#[derive(Clone, Copy)]
struct Vault {
    from: Vec3,
    top: f32,
    to: Vec3,
    t: f32,
    len: f32,
}

impl Vault {
    /// Where the body's feet are `t` seconds in.
    fn at(&self) -> Vec3 {
        let s = (self.t / self.len).clamp(0.0, 1.0);
        let lift = self.top + 0.05;
        let ease = |u: f32| u * u * (3.0 - 2.0 * u);
        if s < 0.45 {
            let u = ease(s / 0.45);
            Vec3::new(self.from.x, self.from.y + (lift - self.from.y) * u, self.from.z)
        } else if s < 0.7 {
            let u = ease((s - 0.45) / 0.25);
            let h = self.from.lerp(self.to, u);
            Vec3::new(h.x, lift, h.z)
        } else {
            let u = (s - 0.7) / 0.3;
            Vec3::new(self.to.x, lift + (self.to.y - lift) * u * u, self.to.z)
        }
    }
}

/// The ground a zombie can be put on at (x, z) under `from`: down through
/// leaves, needles and branches, so none comes in up a tree, and with room
/// for a body over it.
fn spawn_ground(store: &Store, x: f32, z: f32, from: f32) -> Option<f32> {
    use crate::world::matlib;
    let mut top = from;
    for _ in 0..40 {
        let (y, m) = voxel_ground(store, x, z, top)?;
        let tree = matlib::has(m, matlib::flag::FOLIAGE) || matlib::key(m).is_some_and(|k| k.contains("bark") || k.contains("branch"));
        if !tree {
            return (!crate::physics::hits(store, Vec3::new(x, y + 0.1, z), 0.3, 1.7)).then_some(y);
        }
        top = y - 0.3;
    }
    None
}

/// Seconds a body has to be held up against something before it climbs.
const VAULT_AFTER_S: f32 = 0.35;

/// Whether a body at `at` going `dir` (level, unit) has something in front
/// of it that it can climb over: the top it goes over at and where it comes
/// down. Something it could step over is not this; a wall that goes on up
/// past `VAULT_UP`, or that it would come down inside, is not either.
fn vault_over(store: &Store, at: Vec3, dir: Vec3, half_w: f32, height: f32) -> Option<(f32, Vec3)> {
    let hits = |p: Vec3| crate::physics::hits(store, p, half_w, height);
    let face = at + dir * 0.55;
    if !crate::physics::hits(store, face + Vec3::Y * 0.5, half_w, 0.3) {
        return None;
    }
    let mut h = 0.5;
    let top = loop {
        if h > VAULT_UP + 0.01 {
            return None;
        }
        if !hits(face + Vec3::Y * h) && !hits(at + Vec3::Y * h) {
            break at.y + h;
        }
        h += 0.25;
    };
    for d in [1.0, 1.3, 1.6, 2.0, 2.5, 3.0] {
        let p = at + dir * d;
        if hits(Vec3::new(p.x, top, p.z)) {
            return None;
        }
        let Some((y, _)) = voxel_ground(store, p.x, p.z, top) else { continue };
        let land = Vec3::new(p.x, y + 0.05, p.z);
        if y < top - 0.4 && top - y < 4.5 && !hits(land) {
            return Some((top, land));
        }
    }
    None
}

/// What the director keeps about one zombie slot.
#[derive(Clone, Copy, Default)]
struct Slot {
    /// After the party rather than milling about.
    chasing: bool,
    /// A runner keeps its legs going; a crawler has none to speak of.
    runner: bool,
    crawler: bool,
    /// Seconds until it can land another blow.
    cool: f32,
    /// Who it is after (fighter number), refreshed with the orders.
    mark: Option<usize>,
    /// Where it was when it last got anywhere, and seconds since.
    last: Vec3,
    still: f32,
    /// Seconds it still stands after landing a blow (`GRAB_S`).
    pause: f32,
    /// Where it last knew the party to be (`LOOK_S`), whether it could see
    /// them at its last look, seconds to its next look, and the angle off
    /// the straight line it is taking to get there.
    known: Option<Vec3>,
    sighted: bool,
    look: f32,
    angle: f32,
    /// At the place it last knew them, with nobody there: seconds spent
    /// looking, how long it will look, and where it is poking about.
    search: f32,
    search_len: f32,
    poke: Vec3,
    poke_t: f32,
}

/// A grid of step costs from the survivors outward over the survey's
/// person-passable cells: downhill on it is the way to the nearest of them.
struct Flow {
    x0: i32,
    z0: i32,
    w: i32,
    dist: Vec<u32>,
}

impl Flow {
    fn empty() -> Flow {
        Flow { x0: 0, z0: 0, w: 0, dist: Vec::new() }
    }

    fn cell(p: Vec3) -> (i32, i32) {
        ((p.x / CELL_M).floor() as i32, (p.z / CELL_M).floor() as i32)
    }

    fn at(&self, cx: i32, cz: i32) -> u32 {
        let (x, z) = (cx - self.x0, cz - self.z0);
        if x < 0 || z < 0 || x >= self.w || z >= self.w {
            return u32::MAX;
        }
        self.dist[(z * self.w + x) as usize]
    }

    fn of(&self, p: Vec3) -> u32 {
        let (x, z) = Flow::cell(p);
        self.at(x, z)
    }

    /// Dijkstra out from `sources` over a `w`-cell window with its corner at
    /// `(x0, z0)`.
    fn build(s: &Survey, x0: i32, z0: i32, w: i32, sources: &[Vec3]) -> Flow {
        let n = s.n as i32;
        let pass = s.grid(Chan::PassPerson);
        let open = |x: i32, z: i32| x >= 0 && z >= 0 && x < n && z < n && pass[(z * n + x) as usize] >= 128;
        let mut f = Flow { x0, z0, w, dist: vec![u32::MAX; (w * w) as usize] };
        let mut heap = BinaryHeap::new();
        for p in sources {
            let (x, z) = Flow::cell(*p);
            let (lx, lz) = (x - x0, z - z0);
            if lx < 0 || lz < 0 || lx >= w || lz >= w {
                continue;
            }
            let i = (lz * w + lx) as usize;
            f.dist[i] = 0;
            heap.push(Reverse((0u32, x, z)));
        }
        while let Some(Reverse((d, x, z))) = heap.pop() {
            if d > f.at(x, z) {
                continue;
            }
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                let (lx, lz) = (nx - x0, nz - z0);
                if lx < 0 || lz < 0 || lx >= w || lz >= w || !open(nx, nz) {
                    continue;
                }
                let diag = dx != 0 && dz != 0;
                // No cutting a corner between two blocked cells.
                if diag && !(open(x + dx, z) && open(x, z + dz)) {
                    continue;
                }
                let nd = d + if diag { DIAG } else { ORTHO };
                let i = (lz * w + lx) as usize;
                if nd < f.dist[i] {
                    f.dist[i] = nd;
                    heap.push(Reverse((nd, nx, nz)));
                }
            }
        }
        f
    }

    /// The cell `steps` downhill of `from`, as a point at its middle on
    /// `from`'s height. None where `from` is off the field or nothing round
    /// it is lower.
    fn ahead(&self, from: Vec3, steps: usize) -> Option<Vec3> {
        let (mut x, mut z) = Flow::cell(from);
        let mut d = self.at(x, z);
        if d == u32::MAX {
            return None;
        }
        let mut moved = false;
        for _ in 0..steps {
            let mut best = (d, x, z);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let v = self.at(x + dx, z + dz);
                if v < best.0 {
                    best = (v, x + dx, z + dz);
                }
            }
            if best.0 >= d {
                break;
            }
            (d, x, z) = best;
            moved = true;
        }
        moved.then(|| Vec3::new((x as f32 + 0.5) * CELL_M, from.y, (z as f32 + 0.5) * CELL_M))
    }

    /// Metres along the field from `p` to the nearest source.
    fn metres(&self, p: Vec3) -> Option<f32> {
        let d = self.of(p);
        (d != u32::MAX).then(|| d as f32 * CELL_M / ORTHO as f32)
    }
}

/// Metres a side of a cell of the fine field, and cells a side of it: a
/// 96 m square round the player, which holds every place a zombie is put.
const NEAR_RES: f32 = 0.5;
const NEAR_N: i32 = 192;
/// The most a step between two cells of the fine field may climb, metres:
/// a stair or a kerb, which a zombie takes with a hop.
const NEAR_CLIMB: f32 = 1.0;
/// The tallest thing a zombie climbs over, metres over its feet: a hedge, a
/// garden wall, a fence. A house wall goes on up past it, and a window has
/// wall over it, so neither is a way in. Going over costs the field this
/// much more than walking a cell, so a gap close by is still taken first.
pub(super) const VAULT_UP: f32 = 2.1;
const VAULT_COST: u32 = 60;

/// The same as `Flow` at half a metre, measured on the voxels themselves:
/// where the floor is in each cell and whether a body fits over it. The
/// survey's 4 m cells cannot see a doorway, so close in, this is what takes
/// a zombie round a wall and in at the door rather than into the wall.
/// The way between two cells of the fine field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Open,
    /// Only by climbing over.
    Over,
    Shut,
}

struct Near {
    x0: f32,
    z0: f32,
    /// Each cell's floor, NaN where nobody can stand.
    floor: Vec<f32>,
    dist: Vec<u32>,
}

/// How high a step a body gets up, with a hop where it must: the fine field's
/// headroom test starts this far over the floor, so a kerb or a doorstep in
/// the next voxel is not a wall.
const STEP_UP: f32 = 0.55;

/// Solid for the fine field: what `Store::solid` says, less doors. A door is
/// a plate that stands in its doorway as solid until a body comes near it
/// (platforms.rs), and a zombie at the door is one; read as wall, it sealed
/// every house in the field.
fn walled(store: &Store, x: i32, y: i32, z: i32) -> bool {
    // Material names are display names here ("DOOR GREEN").
    store.solid(x, y, z) && !store.get(x, y, z).and_then(crate::world::matlib::name).is_some_and(|n| n.len() >= 4 && n[..4].eq_ignore_ascii_case("door"))
}

/// Does a box with its feet at `p` meet anything `walled`?
fn boxed(store: &Store, p: Vec3, half_w: f32, height: f32) -> bool {
    let v = |a: f32| (a * 4.0).floor() as i32;
    for y in v(p.y)..=v(p.y + height) {
        for z in v(p.z - half_w)..=v(p.z + half_w) {
            for x in v(p.x - half_w)..=v(p.x + half_w) {
                if walled(store, x, y, z) {
                    return true;
                }
            }
        }
    }
    false
}

impl Near {
    fn empty() -> Near {
        Near { x0: 0.0, z0: 0.0, floor: Vec::new(), dist: Vec::new() }
    }

    fn cell(&self, p: Vec3) -> Option<(i32, i32)> {
        let (x, z) = (((p.x - self.x0) / NEAR_RES).floor() as i32, ((p.z - self.z0) / NEAR_RES).floor() as i32);
        (x >= 0 && z >= 0 && x < NEAR_N && z < NEAR_N && !self.dist.is_empty()).then_some((x, z))
    }

    /// Metres of walking from the nearest survivor to `p`, None off the
    /// field or where nobody can get.
    fn walk_m(&self, p: Vec3) -> Option<f32> {
        let (x, z) = self.cell(p)?;
        let d = self.at(x, z);
        (d != u32::MAX).then(|| d as f32 * NEAR_RES / ORTHO as f32)
    }

    fn at(&self, x: i32, z: i32) -> u32 {
        if x < 0 || z < 0 || x >= NEAR_N || z >= NEAR_N || self.dist.is_empty() {
            return u32::MAX;
        }
        self.dist[(z * NEAR_N + x) as usize]
    }

    /// The floor a body can stand on in fine cell `(gx, gz)`: NaN for none,
    /// None while its voxels are not loaded. Looked for from two metres over
    /// the land, which is under a ground floor's ceiling and over a rise in
    /// the ground, so a house is walked through rather than over. Kept in
    /// `cache` once known: the field is rebuilt every second and the ground
    /// under it mostly is not.
    fn floor_of(store: &Store, land: &World, cache: &mut std::collections::HashMap<(i32, i32), f32>, gx: i32, gz: i32) -> Option<f32> {
        if let Some(y) = cache.get(&(gx, gz)) {
            return Some(*y);
        }
        let (x, z) = ((gx as f32 + 0.5) * NEAR_RES, (gz as f32 + 0.5) * NEAR_RES);
        // The first voxel down that a foot stands on. Not `voxel_ground`,
        // which stops at anything that is not air: a door leaf is a voxel a
        // body walks through, and read as a floor it put the doorway two
        // metres up and sealed every house.
        let (vx, vz) = ((x * 4.0).floor() as i32, (z * 4.0).floor() as i32);
        let top = ((land.f.height(x, z) + 2.0) * 4.0) as i32;
        if !store.loaded(vx, top.max(0), vz) {
            return None;
        }
        let floor = (top - 40..=top).rev().find(|&vy| vy >= 0 && walled(store, vx, vy, vz));
        let y = floor.map_or(f32::NAN, |vy| (vy as f32 + if store.slab(vx, vy, vz) { 0.5 } else { 1.0 }) / 4.0);
        // Room over it for a body: a window with a sill at knee height is an
        // opening nobody walks through, and a door is. Only the voxels down
        // the middle of the cell: a box as wide as a body fits no cell of a
        // metre-wide door, and `edge_open` asks about the voxels between.
        let fits = !y.is_nan() && !boxed(store, Vec3::new(x, y + STEP_UP, z), 0.05, 1.4);
        let y = if fits { y } else { f32::NAN };
        cache.insert((gx, gz), y);
        Some(y)
    }

    /// Whether the voxels between fine cell `(gx, gz)` and the next one east
    /// (`dir` 0) or south (1) leave a body room, at the higher of the two
    /// floors. A fence one voxel thick stands between two cell middles and
    /// is seen only here.
    ///
    /// `Edge::Over` is one a body cannot walk through but can climb over:
    /// clear from `VAULT_UP` up.
    fn edge_open(store: &Store, edges: &mut std::collections::HashMap<(i32, i32, u8), Edge>, gx: i32, gz: i32, dir: u8, y: f32) -> Edge {
        if let Some(o) = edges.get(&(gx, gz, dir)) {
            return *o;
        }
        let (x, z) = if dir == 0 { ((gx + 1) as f32 * NEAR_RES, (gz as f32 + 0.5) * NEAR_RES) } else { ((gx as f32 + 0.5) * NEAR_RES, (gz + 1) as f32 * NEAR_RES) };
        let e = if !boxed(store, Vec3::new(x, y + STEP_UP, z), 0.05, 1.4) {
            Edge::Open
        } else if !boxed(store, Vec3::new(x, y + VAULT_UP, z), 0.05, 1.2) {
            Edge::Over
        } else {
            Edge::Shut
        };
        edges.insert((gx, gz, dir), e);
        e
    }

    fn build(store: &Store, land: &World, cache: &mut std::collections::HashMap<(i32, i32), f32>, edges: &mut std::collections::HashMap<(i32, i32, u8), Edge>, centre: Vec3, sources: &[Vec3], snap: i32, vault: bool) -> Near {
        let half = NEAR_N as f32 * NEAR_RES * 0.5;
        let gx0 = ((centre.x - half) / NEAR_RES).floor() as i32;
        let gz0 = ((centre.z - half) / NEAR_RES).floor() as i32;
        let (x0, z0) = (gx0 as f32 * NEAR_RES, gz0 as f32 * NEAR_RES);
        let cells = (NEAR_N * NEAR_N) as usize;
        let mut floor = vec![f32::NAN; cells];
        for cz in 0..NEAR_N {
            for cx in 0..NEAR_N {
                if let Some(y) = Near::floor_of(store, land, cache, gx0 + cx, gz0 + cz) {
                    floor[(cz * NEAR_N + cx) as usize] = y;
                }
            }
        }
        let mut n = Near { x0, z0, floor, dist: vec![u32::MAX; cells] };
        let mut heap = BinaryHeap::new();
        for p in sources {
            let Some((x, z)) = n.cell(*p) else { continue };
            // The nearest ground within `snap` cells: a body standing with
            // its middle against a wall is in a cell the field has as wall
            // (one cell over will do, no further, or it could be the far
            // side of the wall), and the test walker's aim can be anywhere.
            let mut best = None;
            for r in 0..=snap {
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs().max(dz.abs()) != r || best.is_some() {
                            continue;
                        }
                        let (cx, cz) = (x + dx, z + dz);
                        if cx >= 0 && cz >= 0 && cx < NEAR_N && cz < NEAR_N && !n.floor[(cz * NEAR_N + cx) as usize].is_nan() {
                            best = Some((cx, cz));
                        }
                    }
                }
                if best.is_some() {
                    break;
                }
            }
            let (x, z) = best.unwrap_or((x, z));
            let i = (z * NEAR_N + x) as usize;
            if n.floor[i].is_nan() {
                n.floor[i] = p.y;
            }
            n.dist[i] = 0;
            heap.push(Reverse((0u32, x, z)));
        }
        let ok = |n: &Near, x: i32, z: i32| x >= 0 && z >= 0 && x < NEAR_N && z < NEAR_N && !n.floor[(z * NEAR_N + x) as usize].is_nan();
        let (gx0, gz0) = ((x0 / NEAR_RES).round() as i32, (z0 / NEAR_RES).round() as i32);
        // The way between two side by side cells, either way round.
        // With `vault`, a way over costs `VAULT_COST` and is otherwise open.
        let mut extra = 0u32;
        let mut pass = |n: &Near, ax: i32, az: i32, bx: i32, bz: i32, extra: &mut u32| -> bool {
            let y = n.floor[(az * NEAR_N + ax) as usize].max(n.floor[(bz * NEAR_N + bx) as usize]);
            let (lx, lz, dir) = if ax != bx { (ax.min(bx), az, 0) } else { (ax, az.min(bz), 1) };
            match Near::edge_open(store, edges, gx0 + lx, gz0 + lz, dir, y) {
                Edge::Open => true,
                Edge::Over if vault => {
                    *extra = VAULT_COST;
                    true
                }
                _ => false,
            }
        };
        while let Some(Reverse((d, x, z))) = heap.pop() {
            if d > n.at(x, z) {
                continue;
            }
            let here = n.floor[(z * NEAR_N + x) as usize];
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if !ok(&n, nx, nz) {
                    continue;
                }
                let diag = dx != 0 && dz != 0;
                if diag {
                    // Round the corner by both sides: the two cells beside
                    // it and the four ways between.
                    if !(ok(&n, x + dx, z) && ok(&n, x, z + dz)) {
                        continue;
                    }
                    // Never over anything on a diagonal: straight at it.
                    let mut e = 0;
                    if !(pass(&n, x, z, x + dx, z, &mut e) && pass(&n, x, z, x, z + dz, &mut e) && pass(&n, x + dx, z, nx, nz, &mut e) && pass(&n, x, z + dz, nx, nz, &mut e)) || e > 0 {
                        continue;
                    }
                } else if !pass(&n, x, z, nx, nz, &mut extra) {
                    continue;
                }
                // The field runs from the party outward, so a zombie walks
                // it the other way: from the neighbour into this cell. It
                // may climb a stair's height and drop any way down.
                let there = n.floor[(nz * NEAR_N + nx) as usize];
                let climb = here - there;
                if climb > if vault && !diag { VAULT_UP } else { NEAR_CLIMB } {
                    extra = 0;
                    continue;
                }
                if climb > NEAR_CLIMB {
                    extra = VAULT_COST;
                }
                let nd = d + if diag { DIAG } else { ORTHO } + std::mem::take(&mut extra);
                let i = (nz * NEAR_N + nx) as usize;
                if nd < n.dist[i] {
                    n.dist[i] = nd;
                    heap.push(Reverse((nd, nx, nz)));
                }
            }
        }
        n
    }

    /// `steps` cells downhill of `from`, at that cell's floor; None where
    /// `from` is not on the field. A body standing against a wall may be in
    /// a cell that is all wall, so the best of its neighbours will do.
    fn ahead(&self, from: Vec3, steps: usize) -> Option<Vec3> {
        let (mut x, mut z) = self.cell(from)?;
        let mut d = self.at(x, z);
        if d == u32::MAX {
            // Up against a wall, a body's middle can be in a cell the field
            // has as wall: the nearest cell within a metre and a half that
            // is on it, and from there.
            let mut best = (u32::MAX, i32::MAX, x, z);
            for dz in -3..=3 {
                for dx in -3..=3 {
                    let v = self.at(x + dx, z + dz);
                    let r = dx * dx + dz * dz;
                    if v != u32::MAX && r < best.1 {
                        best = (v, r, x + dx, z + dz);
                    }
                }
            }
            if best.0 == u32::MAX {
                return None;
            }
            (d, x, z) = (best.0, best.2, best.3);
            if steps == 0 || best.1 > 0 {
                let y = self.floor[(z * NEAR_N + x) as usize];
                return Some(Vec3::new(self.x0 + (x as f32 + 0.5) * NEAR_RES, if y.is_nan() { from.y } else { y }, self.z0 + (z as f32 + 0.5) * NEAR_RES));
            }
        }
        for _ in 0..steps {
            let mut best = (d, x, z);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let v = self.at(x + dx, z + dz);
                if v < best.0 {
                    best = (v, x + dx, z + dz);
                }
            }
            if best.0 >= d {
                break;
            }
            (d, x, z) = best;
        }
        let y = self.floor[(z * NEAR_N + x) as usize];
        Some(Vec3::new(self.x0 + (x as f32 + 0.5) * NEAR_RES, if y.is_nan() { from.y } else { y }, self.z0 + (z as f32 + 0.5) * NEAR_RES))
    }
}

/// A round planned on a world: the map it is played on and its legs.
pub struct Plan {
    /// The map the match is played on, in the shape a world.json would give
    /// it: guns, spawn, pads.
    pub rules: ShooterDef,
    pub start: Vec3,
    /// Each leg in turn; the last ends at the extraction.
    pub legs: Vec<Leg>,
}

/// The roles of the guns a run is played with (`weapon_kit::ROLES`). The
/// melee and the sidearm are what a survivor starts with; the rest are found.
/// In the realistic kit: a shotgun, an assault rifle, a bolt rifle, a
/// machine gun and a shoulder rocket.
const FOUND: [&str; 5] = ["spread", "midrange", "marksman", "heavy", "explosive"];

/// Plan a round on survey `s` starting near `near` (where the player
/// stands): up to three legs, two to refuges and the last to an extraction
/// site, each leading on from the one before rather than back. The guns are
/// `kit`'s, by role; a role the kit has no gun for is left out, caches and all.
pub fn plan(s: &Survey, near: Vec3, home: Vec3, seed: u32, kit: &KitDef) -> Result<Plan, String> {
    let mut rng = Rng::new(crate::world::noise::hash32(seed ^ 0x2b0b) | 1);
    let of_role = |role: &str| kit.roles.get(role).and_then(|g| g.first()).map(String::as_str);
    let found = FOUND.map(of_role);
    let anchors = &s.graph.anchors;
    let flat = |a: &[f32; 3]| Vec3::new(a[0], a[1], a[2]);
    // The start: a refuge near the player, else where they stand. The world
    // has only streamed in round the player, so anywhere further is ground
    // nobody could be stood on yet.
    let start = anchors
        .iter()
        .filter(|a| a.kind == Kind::Refuge)
        .map(|a| flat(&a.pos))
        .filter(|p| (p.x - near.x).hypot(p.z - near.z) < 90.0)
        .min_by(|a, b| a.distance(near).total_cmp(&b.distance(near)))
        .unwrap_or(near);
    let n = s.n as i32;
    let level = |p: Vec3, q: Vec3| (p.x - q.x).hypot(p.z - q.z);
    let mut legs: Vec<Leg> = Vec::new();
    let mut from = start;
    let mut used = vec![start];
    for i in 0..LEGS {
        let last = i + 1 == LEGS;
        let flow = Flow::build(s, 0, 0, n, &[from]);
        // Somewhere `lo..hi` m on by the walking route, 250 m from every
        // earlier stop, and (with `onward`) further from the start than this
        // leg begins, so the round goes somewhere rather than round about.
        let pick = |lo: f32, hi: f32, kinds: &[Kind], onward: bool, rng: &mut Rng| -> Option<(Vec3, f32, u16)> {
            let fits: Vec<(Vec3, f32, u16)> = anchors
                .iter()
                .filter(|a| kinds.contains(&a.kind))
                .filter_map(|a| {
                    let p = flat(&a.pos);
                    let fresh = used.iter().all(|u| level(*u, p) > 250.0);
                    let on = !onward || i == 0 || level(p, start) > level(from, start) + 150.0;
                    flow.metres(p).filter(|m| (lo..=hi).contains(m) && fresh && on).map(|m| (p, m, a.zone))
                })
                .collect();
            (!fits.is_empty()).then(|| fits[((rng.unit() * fits.len() as f32) as usize).min(fits.len() - 1)])
        };
        // The extraction wants room to hold out in: a focal place first.
        let (first, wide): (&[Kind], &[Kind]) = if last { (&[Kind::Focal], &[Kind::Focal, Kind::Refuge, Kind::Nook]) } else { (&[Kind::Refuge], &[Kind::Refuge, Kind::Focal, Kind::Nook]) };
        let Some((goal, route_m, zone)) = pick(LEG.0, LEG.1, first, true, &mut rng)
            .or_else(|| pick(LEG_WIDE.0, LEG_WIDE.1, first, true, &mut rng))
            .or_else(|| pick(LEG_WIDE.0, LEG_WIDE.1, wide, true, &mut rng))
            .or_else(|| pick(LEG_WIDE.0, LEG_WIDE.1, wide, false, &mut rng))
        else {
            break;
        };
        let name = s.graph.zones.get(zone as usize).map(|z| z.name.clone()).unwrap_or_default();
        // The route, walked back from the goal downhill to where the leg starts.
        let mut route = vec![goal];
        let mut at = goal;
        for _ in 0..2000 {
            match flow.ahead(at, 1) {
                Some(p) => {
                    at = p;
                    route.push(p);
                }
                None => break,
            }
        }
        route.push(from);
        route.reverse();
        legs.push(Leg { goal, name, route_m, route });
        used.push(goal);
        from = goal;
    }
    if legs.is_empty() {
        return Err("no refuge 250-1100 m from the start by a walking route".into());
    }

    let base = home.y;
    let mut pickups = Vec::new();
    let pad = |kind: &str, at: Vec3, dx: f32, dz: f32, weapon: Option<&str>| -> PickupDef {
        PickupDef {
            kind: kind.to_string(),
            at: [at.x + dx, at.z + dz],
            up: Some(at.y - base + 1.0),
            amount: None,
            weapon: weapon.map(str::to_string),
            respawn_s: Some(100_000.0),
            pad_of: None,
        }
    };
    // The guns each leg's caches hold, in the order they turn up: ordinary
    // ones first, then the heavy ones, more of them the further on.
    let heavy = if rng.unit() < 0.5 { [found[3], found[4]] } else { [found[4], found[3]] };
    let orders: [Vec<&str>; LEGS] = [[found[2], found[0], found[1]], [heavy[0], found[2], found[1]], [heavy[1], heavy[0], found[0]]].map(|o| o.into_iter().flatten().collect());
    let mut caches: Vec<Vec3> = Vec::new();
    for (i, leg) in legs.iter().enumerate() {
        let route = &leg.route;
        let leg_start = route.first().copied().unwrap_or(start);
        // Caches beside the route: a nook or a room within 45 m of it,
        // between an eighth and nine tenths of the way, 70 m apart or more.
        let along = |p: Vec3| -> Option<(f32, f32)> {
            let (k, d) = route.iter().enumerate().map(|(k, r)| (k, (r.x - p.x).hypot(r.z - p.z))).min_by(|a, b| a.1.total_cmp(&b.1))?;
            Some((k as f32 / route.len().max(1) as f32, d))
        };
        let near_route = |kinds: &[Kind]| -> Vec<(f32, Vec3)> {
            let mut v: Vec<(f32, Vec3)> = anchors
                .iter()
                .filter(|a| kinds.contains(&a.kind))
                .map(|a| flat(&a.pos))
                .filter(|p| p.distance(leg_start) > 25.0 && p.distance(leg.goal) > 25.0)
                .filter_map(|p| along(p).filter(|(f, d)| *d < 45.0 && (0.12..0.9).contains(f)).map(|(f, _)| (f, p)))
                .collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v
        };
        let mut mine: Vec<Vec3> = Vec::new();
        let take = |spots: Vec<(f32, Vec3)>, mine: &mut Vec<Vec3>, caches: &[Vec3]| {
            for (_, p) in spots {
                if caches.iter().chain(mine.iter()).all(|c| c.distance(p) > 70.0) && mine.len() < 3 {
                    mine.push(p);
                }
            }
        };
        take(near_route(&[Kind::Nook, Kind::Refuge]), &mut mine, &caches);
        if mine.len() < 2 {
            take(near_route(&[Kind::Focal, Kind::Vantage, Kind::Entry, Kind::Pinch, Kind::Drink, Kind::Feed]), &mut mine, &caches);
        }
        if mine.len() < 2 {
            let on: Vec<(f32, Vec3)> = [0.3, 0.65].iter().map(|f| (*f, route[((route.len() as f32 * f) as usize).min(route.len() - 1)])).collect();
            take(on, &mut mine, &caches);
        }
        mine.sort_by(|a, b| along(*a).map_or(0.0, |x| x.0).total_cmp(&along(*b).map_or(0.0, |x| x.0)));
        for (k, c) in mine.iter().enumerate() {
            if let Some(gun) = orders[i].get(k % orders[i].len().max(1)) {
                pickups.push(pad("weapon", *c, 0.0, 0.0, Some(gun)));
                pickups.push(pad("ammo", *c, 1.6, 0.0, Some(gun)));
            }
            pickups.push(pad("health", *c, -1.6, 0.0, None));
        }
        caches.extend(mine);
        // A refuge between legs holds what the brief says it always does:
        // rounds for the pistol and the two guns you start with, two medical
        // kits, and one long gun with its rounds.
        if i + 1 < legs.len() {
            let g = leg.goal;
            pickups.push(pad("health", g, 1.2, 0.0, None));
            pickups.push(pad("health", g, -1.2, 0.0, None));
            for (gun, dx) in [(of_role("sidearm"), 0.0), (found[0], 1.2), (found[1], -1.2)] {
                if let Some(gun) = gun {
                    pickups.push(pad("ammo", g, dx, 1.2, Some(gun)));
                }
            }
            if let Some(gun) = if i == 0 { found[2] } else { heavy[0] } {
                pickups.push(pad("weapon", g, 0.0, -1.2, Some(gun)));
                pickups.push(pad("ammo", g, 1.2, -1.2, Some(gun)));
            }
        }
    }
    // A role with a choice of guns (the realistic melee: a shovel or a
    // knife) is drawn here, and the kit keeps whatever it lists.
    let melee = kit.roles.get("melee").filter(|g| !g.is_empty()).map(|g| g[((rng.unit() * g.len() as f32) as usize).min(g.len() - 1)].as_str());
    let weapons: Vec<String> = [melee, of_role("sidearm")].into_iter().chain(found).flatten().map(str::to_string).collect();
    let to = legs[0].goal - start;
    let bearing = -(-to.x).atan2(-to.z).to_degrees();
    let rules: ShooterDef = serde_json::from_value(serde_json::json!({
        "weapons": weapons,
        "health": 100.0,
        "respawn_s": 1.0e6,
        // The floor is looked for from a metre over the anchor's own, so a
        // house's start is its ground floor and not the storey above.
        "spawns": [{"at": [start.x, start.z], "bearing": bearing, "ceiling": start.y + 1.0}],
    }))
    .map_err(|e| e.to_string())?;
    let mut rules = rules;
    rules.pickups = pickups;
    Ok(Plan { rules, start, legs })
}

/// The headless switches, read once when the round starts.
#[derive(Clone, Copy, Default)]
struct Headless {
    /// `VG_ZOMBIE_WALK`: walk the player down the route (`walk_route`).
    walk: bool,
    /// `VG_ZOMBIE_JUMP`: stand the player at each leg's goal after ten seconds.
    jump: bool,
    /// `VG_ZOMBIE_FINALE_S`: the holdout's length, seconds.
    finale_s: Option<f32>,
}

impl Headless {
    fn read() -> Headless {
        Headless {
            walk: std::env::var_os("VG_ZOMBIE_WALK").is_some(),
            jump: std::env::var_os("VG_ZOMBIE_JUMP").is_some(),
            finale_s: std::env::var("VG_ZOMBIE_FINALE_S").ok().and_then(|v| v.parse().ok()),
        }
    }
}

/// The round as it goes.
pub struct Run {
    pub start: Vec3,
    /// Every leg, and which one the party is on.
    pub legs: Vec<Leg>,
    pub leg: usize,
    pub stage: Stage,
    /// Seconds into the stage; the finale's length.
    pub stage_t: f32,
    pub finale_len: f32,
    /// The goal of the leg the party is on, and the way there (the leg's own,
    /// copied out so the director reads one place).
    pub goal: Vec3,
    pub goal_name: String,
    pub route_m: f32,
    route: Vec<Vec3>,
    pub condition: Condition,
    /// Which way the wind blows, level: an idle zombie downwind of the party
    /// smells it (`Condition::scent_m`).
    wind: Vec3,
    /// For the end screen: health the party lost, rounds the player fired
    /// from the explosive guns, seconds the player swam, the clock when the
    /// way out was reached.
    pub hurt: f32,
    pub blasts: u32,
    pub swim_s: f32,
    /// The party bots are fighters 1..=party; the zombie slots come after.
    pub(crate) party: usize,
    slots: Vec<Slot>,
    flow: Flow,
    flow_due: f32,
    near: Near,
    /// Floors of the fine cells seen so far (`Near::floor_of`), and the ways
    /// between them (`Near::edge_open`).
    floors: std::collections::HashMap<(i32, i32), f32>,
    edges: std::collections::HashMap<(i32, i32, u8), Edge>,
    phase: Phase,
    /// Seconds into the phase, and how long it lasts.
    phase_t: f32,
    phase_len: f32,
    /// Peak hordes still to come in this peak: how many, and seconds to the next.
    waves: u32,
    wave_due: f32,
    /// Where the current horde comes from.
    wave_from: Option<Vec3>,
    /// Rounds fired lately, weighted: what buys a noise horde.
    heat: f32,
    since_horde: f32,
    /// Rounds fired, counted from the fighters' scores.
    shots_seen: Vec<u32>,
    spawn_due: f32,
    rng: Rng,
    /// Seconds since the party was put down at the start.
    pub clock: f32,
    placed: bool,
    /// The party holds where it is (X), or follows.
    pub hold: Option<Vec3>,
    /// Where the player has walked, newest last, a point every 0.75 m: the
    /// party follows along it, which is ground somebody has just crossed,
    /// rather than to a spot behind the player that may be through a wall.
    trail: Vec<Vec3>,
    /// The test walker's own field (`walk_route`).
    walk: (Near, f32),
    headless: Headless,
    pub over: Option<(Outcome, f32)>,
    /// Zombies killed, by anybody, and how loud the party was.
    pub kills: u32,
    pub noise: f32,
    pub hordes: u32,
    /// How hard a blow lands, by the difficulty.
    blow: f32,
    /// What the director has done, for the headless report.
    pub spawned: u32,
    pub culled: u32,
    /// The difficulty's share of the zombies (`CROWD`).
    crowd: f32,
    /// Who a zombie has hold of (fighter number), and for how much longer.
    grabs: Vec<(usize, f32)>,
    /// Per bot, party and zombies alike: a climb under way, and where it
    /// was last step and for how long it has been held up there.
    vaults: Vec<Option<Vault>>,
    held_up: Vec<(Vec3, f32)>,
}

impl Run {
    /// A round from the top: on leg one, nothing up, nothing counted.
    #[allow(clippy::too_many_arguments)]
    fn begin(start: Vec3, legs: Vec<Leg>, party: usize, blow: f32, crowd: f32, condition: Condition, mut rng: Rng, placed: bool) -> Run {
        let a = rng.unit() * std::f32::consts::TAU;
        let first = legs[0].clone();
        let mut run = Run {
            start,
            legs,
            leg: 0,
            stage: Stage::Leg,
            stage_t: 0.0,
            finale_len: 0.0,
            goal: first.goal,
            goal_name: first.name,
            route_m: first.route_m,
            route: first.route,
            condition,
            wind: Vec3::new(a.cos(), 0.0, a.sin()),
            hurt: 0.0,
            blasts: 0,
            swim_s: 0.0,
            party,
            slots: vec![Slot::default(); SLOTS],
            flow: Flow::empty(),
            flow_due: 0.0,
            near: Near::empty(),
            floors: std::collections::HashMap::new(),
            edges: std::collections::HashMap::new(),
            phase: Phase::Build,
            phase_t: 0.0,
            phase_len: 75.0,
            waves: 0,
            wave_due: 0.0,
            wave_from: None,
            heat: 0.0,
            since_horde: 0.0,
            shots_seen: Vec::new(),
            spawn_due: 2.0,
            rng,
            clock: 0.0,
            placed,
            hold: None,
            trail: Vec::new(),
            walk: (Near::empty(), 0.0),
            headless: Headless::read(),
            over: None,
            kills: 0,
            noise: 0.0,
            hordes: 0,
            blow,
            spawned: 0,
            culled: 0,
            crowd,
            vaults: Vec::new(),
            held_up: Vec::new(),
            grabs: Vec::new(),
        };
        run.phase_len = run.draw(FIRST_BUILD_S);
        run
    }

    /// On to leg `i`: its goal and route, and the director from a build-up.
    fn enter_leg(&mut self, i: usize) {
        let l = self.legs[i].clone();
        self.leg = i;
        self.goal = l.goal;
        self.goal_name = l.name;
        self.route_m = l.route_m;
        self.route = l.route;
        self.stage = Stage::Leg;
        self.stage_t = 0.0;
        self.phase = Phase::Build;
        self.phase_t = 0.0;
        self.phase_len = self.draw(LEG_BUILD_S);
        self.waves = 0;
        self.wave_from = None;
        self.hold = None;
    }

    /// A length or a count from a (least, up to this more) pair.
    fn draw(&mut self, (lo, more): (f32, f32)) -> f32 {
        lo + self.rng.unit() * more
    }

    /// Whether this is the last leg, the one to the extraction.
    pub fn last_leg(&self) -> bool {
        self.leg + 1 >= self.legs.len()
    }

    /// The share of zombies now: the difficulty's, and more each leg.
    fn press(&self) -> f32 {
        self.crowd * (1.0 + LEG_PRESS * self.leg as f32)
    }

    /// What the run earned on the end screen, from its numbers.
    pub fn tags(&self, sh: &Shooter) -> Vec<&'static str> {
        let mut out = Vec::new();
        let made = matches!(self.over, Some((Outcome::Made, _)));
        let minutes = (self.clock / 60.0).max(1.0);
        if self.noise / minutes < 6.0 {
            out.push("GHOST RUN");
        }
        if self.blasts >= 6 {
            out.push("DEMOLITION CREW");
        }
        if self.swim_s >= 40.0 {
            out.push("SWAM FOR IT");
        }
        let lost = sh.bots.iter().take(self.party).filter(|b| !b.fighter.alive()).count();
        if made && self.party > 0 && lost == self.party {
            out.push("LAST ONE STANDING");
        } else if made && lost > 0 {
            out.push("LEFT BEHIND");
        }
        out
    }

    pub fn zombies_up(&self, sh: &Shooter) -> usize {
        sh.bots.iter().skip(self.party).filter(|b| b.fighter.alive()).count()
    }

    pub fn phase_name(&self) -> &'static str {
        self.phase.name()
    }

    /// The party still standing, the player included, and how many it started as.
    pub fn standing(&self, sh: &Shooter) -> (usize, usize) {
        let up = usize::from(sh.me.alive()) + sh.bots.iter().take(self.party).filter(|b| b.fighter.alive()).count();
        (up, self.party + 1)
    }

    /// Metres to the refuge as the crow flies.
    /// The point on the route about `m` metres on from where it passes
    /// nearest `at`; the refuge near the end.
    pub fn route_ahead(&self, at: Vec3, m: f32) -> Vec3 {
        let Some(i) = (0..self.route.len()).min_by(|&a, &b| self.route[a].distance(at).total_cmp(&self.route[b].distance(at))) else { return self.goal };
        let mut gone = 0.0;
        for j in i + 1..self.route.len() {
            gone += self.route[j].distance(self.route[j - 1]);
            if gone >= m {
                return self.route[j];
            }
        }
        self.goal
    }

    pub fn to_goal(&self, at: Vec3) -> f32 {
        (self.goal.x - at.x).hypot(self.goal.z - at.z)
    }
}

impl Shooter {
    /// A zombie round: the plan's map, the party out of `rules.bots`, and the
    /// zombie slots behind them.
    #[allow(clippy::too_many_arguments)]
    pub fn zombies(lib: &Library, rules: &ShooterDef, plan: Plan, home: (Vec3, f32), store: &Store, seed: u32, difficulty: &str, repeat: bool, condition: Condition) -> Shooter {
        let mut rules = rules.clone();
        let roster = std::mem::take(&mut rules.bots);
        rules.target_score = 0;
        rules.teams = false;
        let mut sh = if repeat { Shooter::repeatable(lib, &rules, home, store, seed) } else { Shooter::new(lib, &rules, home, store, seed) };
        sh.me.team = Some(0);
        let base = person::level(difficulty).or_else(|| person::level("average")).expect("average is a level");
        // The party: each with a long gun and the pistol, full.
        // By role, so whatever kit the match is played with fills them.
        let of_role = |role: &str| rules.weapons.iter().find(|n| lib.weapon(n).is_some_and(|i| lib.weapons[i].role == role)).cloned().unwrap_or_default();
        let party_guns = [of_role("midrange"), of_role("spread"), of_role("midrange")];
        let sidearm = of_role("sidearm");
        for (i, def) in roster.iter().take(3).enumerate() {
            let mut body = Player::new(plan.start + Vec3::new(1.5 * (i as f32 - 1.0), 0.5, 1.5), 0.0);
            body.pitch = 0.0;
            body.arena = false;
            body.air_jumps = 1;
            let mut fighter = new_fighter(lib, &rules);
            let guns = [party_guns[i % 3].clone(), sidearm.clone()];
            fighter.arsenal = crate::combat::weapon::Arsenal::tuned(lib, &guns, true, &rules.weapon_tune, |n| eprintln!("zombies: no weapon called {n}"));
            fighter.revive(true);
            fighter.team = Some(0);
            let mut person = person::level(&def.skill).unwrap_or(base).flavour(def.bold, def.steady);
            // Night and fog shorten what the party sees as well.
            person.sight = person.sight.min(condition.party_sight());
            let s = seed ^ (i as u32 + 1).wrapping_mul(0x27d4);
            sh.bots.push(Bot::new(&def.name, body, fighter, person, Box::new(Companion::new(s, i)), super::teams::tint(0, i), &sh.avatar, s));
        }
        let party = sh.bots.len();
        // The zombie slots, all down to begin with.
        for k in 0..SLOTS {
            let s = seed ^ 0x2077 ^ (k as u32 + 7).wrapping_mul(0x85eb_ca6b);
            let mut body = Player::new(plan.start - Vec3::Y * 50.0, 0.0);
            body.arena = false;
            body.air_jumps = 1;
            let mut fighter = brain::Fighter::new(crate::combat::weapon::Arsenal { guns: Vec::new(), held: 0 }, ZOMBIE_HP, 0.0);
            fighter.health = 0.0;
            fighter.respawn = f32::INFINITY;
            fighter.fell = body.pos;
            let shade = 0.8 + (s % 5) as f32 * 0.07;
            let tint = [0.52 * shade, 0.6 * shade, 0.42 * shade];
            let mut bot = Bot::new("zombie", body, fighter, base, Box::new(zombie::Zombie::new(s)), tint, &sh.avatar, s);
            // A zombie's own build where the library has them, else a bot's.
            bot.build = sh.avatar.zombie_build(s);
            bot.figure = sh.avatar.body(bot.build).figure(s);
            sh.bots.push(bot);
        }
        let level = person::NAMES.iter().position(|n| n.eq_ignore_ascii_case(difficulty)).unwrap_or(2) as f32;
        let run = Run::begin(
            plan.start,
            plan.legs,
            party,
            (BLOW + BLOW_STEP * (level - 2.0)).max(8.0),
            (CROWD + CROWD_STEP * (level - 2.0)).max(0.4) * PARTY_SHARE[party.min(3)],
            condition,
            Rng::new(crate::world::noise::hash32(seed ^ 0xdead) | 1),
            false,
        );
        sh.zed = Some(Box::new(run));
        sh
    }

    /// Whether a blow to fighter `who` draws blood: a zombie's does, a
    /// robot's and your own (which you would only see on the screen) do not.
    pub fn bleeds(&self, who: usize) -> bool {
        self.is_zombie(who)
    }

    /// Whether bot fighter `who` is a zombie slot.
    pub(crate) fn is_zombie(&self, who: usize) -> bool {
        self.zed.as_ref().is_some_and(|z| who > z.party)
    }

    /// For bot `k` (an index into `bots`), if it is a zombie that is up:
    /// whether it is after the party. For the sound (sound/watch.rs).
    pub(crate) fn zombie_chasing(&self, k: usize) -> Option<bool> {
        let z = self.zed.as_ref()?;
        let slot = z.slots.get(k.checked_sub(z.party)?)?;
        self.bots[k].fighter.alive().then_some(slot.chasing)
    }

    /// The round's step, before the brains think: place the party, refresh
    /// the flow field, run the director, and hand every bot its order.
    pub(super) fn direct_zombies(&mut self, player: &mut Player, store: &Store, land: Option<&World>, dt: f32) {
        let Some(mut z) = self.zed.take() else { return };
        let survey = land.and_then(|w| w.survey.as_ref());
        // Enter after the round is over: another go at the same leg.
        let again = std::mem::take(&mut self.again);
        if z.over.is_some_and(|(_, t)| t > 1.5) && again {
            self.restart_zombies(&mut z, player, store);
        }
        if !z.placed {
            // Wait until the match has stood the player on the start.
            if self.me_to_the_field == 0 {
                self.place_party(&z, player, store);
                self.arm_me();
                z.placed = true;
            }
            self.zed = Some(z);
            return;
        }
        if let Some((_, t)) = &mut z.over {
            *t += dt;
        } else {
            z.clock += dt;
        }
        let Some(s) = survey else {
            self.zed = Some(z);
            return;
        };
        // Headless only: walk the player down the planned route, so a whole
        // leg can be run and measured without anybody at the keys.
        if let (true, Some(w)) = (z.headless.walk && self.me.alive() && z.over.is_none(), land) {
            // It cannot fight, so it is kept whole: what is measured is
            // the route, the director and the arrival.
            self.me.health = self.me.full_health;
            let mut walk = std::mem::replace(&mut z.walk, (Near::empty(), 0.0));
            walk_route(&mut z, &mut walk, player, store, w, dt);
            z.walk = walk;
        }
        // Headless only: after ten seconds on a leg, stand the player at its
        // goal, so the refuges and the finale can be run through without a
        // whole round's walking (`VG_ZOMBIE_FINALE_S` shortens the holdout).
        if z.headless.jump && z.over.is_none() && z.stage == Stage::Leg && z.stage_t > 10.0 {
            stand(player, z.goal + Vec3::Y * 0.6, player.yaw);
            z.stage_t = 0.0;
        }
        let survivors = self.survivors(player, &z);
        if self.me.alive() && z.trail.last().is_none_or(|t| t.distance(player.pos) > 0.75) {
            z.trail.push(player.pos);
            if z.trail.len() > 120 {
                z.trail.remove(0);
            }
        }
        // The flow field, from wherever the party is.
        z.flow_due -= dt;
        if z.flow_due <= 0.0 && !survivors.is_empty() {
            z.flow_due = 0.75;
            let (cx, cz) = Flow::cell(player.pos);
            let feet: Vec<Vec3> = survivors.iter().map(|(_, p)| *p).collect();
            z.flow = Flow::build(s, cx - FLOW_W / 2, cz - FLOW_W / 2, FLOW_W, &feet);
            if let Some(w) = land {
                z.near = Near::build(store, w, &mut z.floors, &mut z.edges, player.pos, &feet, 1, true);
            }
        }
        if z.over.is_none() {
            if player.swimming {
                z.swim_s += dt;
            }
            self.hear_shots(&mut z, dt);
            match z.stage {
                Stage::Leg => self.run_director(&mut z, s, player, store, &survivors, dt),
                // Nothing new comes to a refuge.
                Stage::Refuge => {}
                Stage::Finale | Stage::Out => self.run_finale(&mut z, s, player, store, &survivors, dt),
            }
            self.end_of_round(&mut z, player, store, dt);
        }
        self.orders_for(&mut z, player, store, &survivors, dt);
        self.party_care(&z, player, store, dt);
        self.zed = Some(z);
    }

    /// Everyone on the party's side who is up, as (fighter, foot).
    fn survivors(&self, player: &Player, z: &Run) -> Vec<(usize, Vec3)> {
        let mut out = Vec::with_capacity(z.party + 1);
        if self.me.alive() {
            out.push((0, player.pos));
        }
        for (i, b) in self.bots.iter().enumerate().take(z.party) {
            if b.fighter.alive() {
                out.push((i + 1, b.pos()));
            }
        }
        out
    }

    /// The player's start: the melee weapon, and the pistol, shotgun and
    /// assault rifle with as much as each can carry; the rifle out.
    fn arm_me(&mut self) {
        let a = &mut self.me.arsenal;
        for g in &mut a.guns {
            if matches!(g.def.role.as_str(), "sidearm" | "spread" | "midrange") {
                g.have = true;
                g.ammo = g.def.ammo_max;
                g.loaded = g.def.mag.min(g.ammo);
            }
        }
        if let Some(i) = a.guns.iter().position(|g| g.have && g.def.role == "midrange") {
            a.held = i;
        }
    }

    /// Stand the party bots round the player.
    fn place_party(&mut self, z: &Run, player: &Player, store: &Store) {
        let back = -player.forward();
        let side = Vec3::new(-back.z, 0.0, back.x);
        for i in 0..z.party {
            // Beside or behind the player where there is room, else on the
            // player's own spot (the shove apart sorts that out): never on
            // the far side of a wall.
            let off = back * 1.2 + side * (1.1 * (i as f32 - 1.0));
            let mut at = player.pos + Vec3::new(off.x, 0.0, off.z);
            let clear = |p: Vec3| !crate::physics::hits(store, p + Vec3::Y * 0.1, 0.3, 1.7) && crate::combat::shot::sees(store, player.pos + Vec3::Y * 1.0, p + Vec3::Y * 1.0);
            match voxel_ground(store, at.x, at.z, player.pos.y + 1.0) {
                Some((y, _)) if (y - player.pos.y).abs() < 0.6 && clear(Vec3::new(at.x, y, at.z)) => at.y = y + 0.05,
                _ => at = player.pos,
            }
            let b = &mut self.bots[i];
            stand(&mut b.body, at, player.yaw);
            b.pilot = brain::Pilot { yaw: player.yaw, ..Default::default() };
        }
    }

    /// Count the rounds fired since last time: the player's at full weight,
    /// the party's at half. Every idle zombie within earshot wakes.
    fn hear_shots(&mut self, z: &mut Run, dt: f32) {
        z.shots_seen.resize(z.party + 1, 0);
        let mut loud = Vec::new();
        for who in 0..=z.party {
            let (shots, at) = if who == 0 { (self.me.score.shots, None) } else { (self.bots[who - 1].fighter.score.shots, Some(self.bots[who - 1].pos())) };
            let new = shots.saturating_sub(z.shots_seen[who]);
            z.shots_seen[who] = shots;
            if new > 0 {
                let w = if who == 0 { 1.0 } else { 0.5 } * z.condition.noise();
                if who == 0 && self.me.arsenal.current().is_some_and(|g| matches!(g.def.role.as_str(), "explosive" | "lobber" | "breacher")) {
                    z.blasts += new;
                }
                z.heat += new as f32 * w;
                z.noise += new as f32 * w;
                loud.push(at);
            }
        }
        z.heat *= (-dt / HEAT_FADE_S).exp();
        if loud.is_empty() {
            return;
        }
        let me_at = self.noises.iter().find(|n| n.by == 0).map(|n| n.at);
        for at in loud {
            let Some(at) = at.or(me_at) else { continue };
            for (k, b) in self.bots.iter().enumerate().skip(z.party) {
                if b.fighter.alive() && b.pos().distance(at) < SHOT_WAKES_M * z.condition.noise() {
                    let slot = &mut z.slots[k - z.party];
                    slot.chasing = true;
                    if !slot.sighted {
                        slot.known = Some(at);
                        slot.search = 0.0;
                    }
                }
            }
        }
    }

    /// The director: what comes, when, and from where.
    fn run_director(&mut self, z: &mut Run, s: &Survey, player: &Player, store: &Store, survivors: &[(usize, Vec3)], dt: f32) {
        z.phase_t += dt;
        z.since_horde += dt;
        let up = z.zombies_up(self);
        // How hard the party is pressed: the player's health, and what is
        // already on top of them.
        let close = self.bots.iter().skip(z.party).filter(|b| b.fighter.alive() && b.pos().distance(player.pos) < PRESSED_M).count();
        let whole = self.me.health / self.me.full_health.max(1.0);
        let pressed = whole < PRESSED_HEALTH || close >= PRESSED_CROWD;
        // The first minute is gentle, whatever the phase.
        let early = (z.clock / 60.0).min(1.0);
        match z.phase {
            Phase::Build => {
                // Held back while the party is pressed, but not for long.
                if z.phase_t > z.phase_len && (!pressed || z.phase_t > z.phase_len + PRESSED_WAIT_S) {
                    z.phase = Phase::Peak;
                    z.phase_t = 0.0;
                    z.phase_len = z.draw(PEAK_S);
                    z.waves = PEAK_WAVES;
                    z.wave_due = 0.0;
                    z.wave_from = None;
                    z.hordes += 1;
                }
            }
            Phase::Peak => {
                if z.phase_t > z.phase_len || (whole < PEAK_BAIL.0 && z.phase_t > PEAK_BAIL.1) {
                    z.phase = Phase::Relax;
                    z.phase_t = 0.0;
                    z.phase_len = z.draw(RELAX_S);
                    z.waves = 0;
                }
            }
            Phase::Relax => {
                if z.phase_t > z.phase_len {
                    z.phase = Phase::Build;
                    z.phase_t = 0.0;
                    z.phase_len = z.draw(BUILD_S);
                }
            }
        }
        // A peak's hordes, its waves all from one direction.
        if z.waves > 0 {
            z.wave_due -= dt;
            if z.wave_due <= 0.0 {
                z.waves -= 1;
                z.wave_due = WAVE_GAP_S;
                let size = ((z.draw((WAVE_SIZE.0, WAVE_SIZE.1)) + WAVE_SIZE.2 * early) * z.press()).round() as usize;
                self.horde(z, s, player, store, survivors, size);
                z.since_horde = 0.0;
            }
        }
        // Noise buys a horde of its own, out of the cycle.
        if z.heat > NOISE_HORDE && z.since_horde > NOISE_GAP_S && z.phase != Phase::Relax {
            z.heat -= NOISE_COST;
            z.since_horde = 0.0;
            z.wave_from = None;
            z.hordes += 1;
            let size = (z.draw(NOISE_SIZE) * z.press()).round() as usize;
            self.horde(z, s, player, store, survivors, size);
        }
        // The ambient ones: how many there should be about now.
        let want = match z.phase {
            Phase::Build => AMBIENT_BUILD.0 + early * (AMBIENT_BUILD.1 + AMBIENT_BUILD.2 * (z.phase_t / z.phase_len).min(1.0)),
            Phase::Peak => AMBIENT_PEAK.0 + AMBIENT_PEAK.1 * early,
            // The floor: even in a breather there are a couple about.
            Phase::Relax => AMBIENT_FLOOR,
        };
        let want = (want * z.press()).round() as usize;
        z.spawn_due -= dt;
        if z.spawn_due <= 0.0 {
            z.spawn_due = if z.phase == Phase::Relax { RELAX_SPAWN_GAP_S } else { SPAWN_GAP_S };
            let near = self.bots.iter().skip(z.party).filter(|b| b.fighter.alive() && b.pos().distance(player.pos) < AMBIENT_M).count();
            if near < want && up < MOST_UP {
                // Some milling ahead along the way to the refuge, the rest
                // already after the party.
                let chasing = z.rng.unit() < 0.35 + 0.3 * early;
                let toward = (z.rng.unit() < AHEAD_SHARE).then(|| z.route_ahead(player.pos, AHEAD_M));
                if let Some(at) = self.find_spot(z, s, player, store, survivors, toward) {
                    let crawler = z.rng.unit() < CRAWLER_SHARE;
                    self.wake(z, at, chasing, false, crawler);
                }
            }
        }
        // Put back the ones left far behind.
        let next = z.route_ahead(player.pos, 30.0) - player.pos;
        let ahead = Vec3::new(next.x, 0.0, next.z).normalize_or(Vec3::Z);
        for k in z.party..self.bots.len() {
            let b = &self.bots[k];
            if !b.fighter.alive() {
                continue;
            }
            let far = survivors.iter().map(|(_, p)| p.distance(b.pos())).fold(f32::INFINITY, f32::min);
            let off = b.pos() - player.pos;
            let behind = far > BEHIND_M && Vec3::new(off.x, 0.0, off.z).dot(ahead) < -0.5 * far && !crate::combat::shot::sees(store, player.eye(1.0), b.pos() + Vec3::Y * 1.2);
            if far > LOSE_M || behind || b.pos().y < -20.0 {
                self.sleep(z, k);
                z.culled += 1;
            }
        }
    }

    /// A pack of `size` from one place, after the party.
    fn horde(&mut self, z: &mut Run, s: &Survey, player: &Player, store: &Store, survivors: &[(usize, Vec3)], size: usize) {
        let from = match z.wave_from {
            Some(p) => Some(p),
            None => {
                let toward = (z.stage == Stage::Leg && z.rng.unit() < AHEAD_SHARE).then(|| z.route_ahead(player.pos, AHEAD_M));
                self.find_spot(z, s, player, store, survivors, toward)
            }
        };
        let Some(from) = from else { return };
        z.wave_from = Some(from);
        let mut placed = 0;
        for _ in 0..size * 4 {
            if placed >= size || z.zombies_up(self) >= MOST_UP {
                break;
            }
            let a = z.rng.unit() * std::f32::consts::TAU;
            let r = 1.0 + z.rng.unit() * 7.0;
            let (x, zz) = (from.x + a.cos() * r, from.z + a.sin() * r);
            let Some(y) = spawn_ground(store, x, zz, from.y + 4.0) else { continue };
            if (y - from.y).abs() > 4.0 || s.byte(Chan::PassPerson, x, zz) < 128 {
                continue;
            }
            if z.near.cell(Vec3::new(x, y, zz)).is_some() && z.near.ahead(Vec3::new(x, y, zz), 1).is_none() {
                continue;
            }
            let runner = z.rng.unit() < RUNNER_SHARE;
            self.wake(z, Vec3::new(x, y + 0.05, zz), true, runner, false);
            placed += 1;
        }
    }

    /// Somewhere to put a zombie: on ground a person can walk, connected to
    /// the party, unlit, out of the player's sight, and 42-85 m off. Biased
    /// toward `toward` when given: within about 45 degrees of it.
    fn find_spot(&mut self, z: &mut Run, s: &Survey, player: &Player, store: &Store, survivors: &[(usize, Vec3)], toward: Option<Vec3>) -> Option<Vec3> {
        let eye = player.eye(1.0);
        let lean = toward.map(|t| (t.z - player.pos.z).atan2(t.x - player.pos.x));
        for _ in 0..30 {
            let a = match lean {
                Some(l) => l + (z.rng.unit() - 0.5) * 1.6,
                None => z.rng.unit() * std::f32::consts::TAU,
            };
            // Ahead, mostly just out of sight rather than at the far edge.
            let u = z.rng.unit();
            let lo = z.condition.spawn_near();
            let r = lo + if lean.is_some() { u * u } else { u } * (SPAWN_FAR - lo);
            let (x, zz) = (player.pos.x + a.cos() * r, player.pos.z + a.sin() * r);
            if x < 2.0 || zz < 2.0 || x > s.size - 2.0 || zz > s.size - 2.0 {
                continue;
            }
            if s.byte(Chan::PassPerson, x, zz) < 128 || s.value(Chan::Light, x, zz) > 0.3 {
                continue;
            }
            let p = Vec3::new(x, player.pos.y, zz);
            if z.flow.of(p) == u32::MAX {
                continue;
            }
            let Some(y) = spawn_ground(store, x, zz, player.pos.y + 30.0) else { continue };
            if (y - player.pos.y).abs() > 30.0 {
                continue;
            }
            let at = Vec3::new(x, y + 0.05, zz);
            if survivors.iter().any(|(_, q)| q.distance(at) < lo - 8.0) {
                continue;
            }
            // Ground the fine field reaches: a zombie put behind a hedge
            // with no gap in it would stand there all round.
            if z.near.cell(at).is_some() && z.near.ahead(at, 1).is_none() {
                continue;
            }
            if crate::combat::shot::sees(store, eye, at + Vec3::Y * 1.4) {
                continue;
            }
            return Some(at);
        }
        None
    }

    /// Bring a zombie slot up at `at`. A crawler is never a runner.
    fn wake(&mut self, z: &mut Run, at: Vec3, chasing: bool, runner: bool, crawler: bool) {
        let Some(k) = (z.party..self.bots.len()).find(|&k| !self.bots[k].fighter.alive() && !self.corpses.iter().any(|c| c.bot == k)) else { return };
        let runner = runner && !crawler;
        let yaw = z.rng.unit() * std::f32::consts::TAU;
        let b = &mut self.bots[k];
        stand(&mut b.body, at, yaw);
        b.body.pitch = 0.0;
        b.body.stand_h = if crawler { CRAWL_H } else { crate::player::STAND_H };
        b.body.body.height = b.body.stand_h;
        b.pilot = brain::Pilot { yaw, ..Default::default() };
        b.fighter.revive(false);
        b.fighter.health = ZOMBIE_HP;
        b.figure.revive();
        // Its gait: a runner runs, a crawler crawls, the rest shamble or
        // lurch (anim/undead.rs).
        b.figure.walk = crate::anim::figure::Walk::for_zombie((k as u32).wrapping_mul(0x9e37_79b9) ^ z.spawned, runner, crawler);
        z.slots[k - z.party] = Slot { chasing, runner, crawler, cool: 0.5, last: at, ..Slot::default() };
        z.spawned += 1;
    }

    /// Put a zombie slot back in the pool, with no body left behind.
    fn sleep(&mut self, z: &mut Run, k: usize) {
        let b = &mut self.bots[k];
        b.fighter.health = 0.0;
        b.fighter.respawn = f32::INFINITY;
        b.fighter.fell = b.body.pos;
        z.slots[k - z.party] = Slot::default();
    }

    /// Where the round goes next: a refuge reached, left, the extraction
    /// reached, held, got out of; or the player down.
    fn end_of_round(&mut self, z: &mut Run, player: &Player, store: &Store, dt: f32) {
        if !self.me.alive() {
            z.over = Some((Outcome::Lost, 0.0));
            self.note = Some("THE DEAD GOT YOU".into());
            return;
        }
        z.stage_t += dt;
        let at_goal = |z: &Run, m: f32| z.to_goal(player.pos) < m && (player.pos.y - z.goal.y).abs() < 4.0;
        match z.stage {
            Stage::Leg if at_goal(z, ARRIVE_M) => {
                z.stage_t = 0.0;
                if z.last_leg() {
                    z.stage = Stage::Finale;
                    z.finale_len = FINALE_S.0 + z.rng.unit() * FINALE_S.1;
                    if let Some(t) = z.headless.finale_s {
                        z.finale_len = t;
                    }
                    // The first horde soon after the call.
                    z.since_horde = FINALE_HORDE_S - 6.0;
                    let m = z.finale_len as u32;
                    self.note = Some(format!("EXTRACTION CALLED: HOLD OUT {}:{:02}", m / 60, m % 60));
                } else {
                    z.stage = Stage::Refuge;
                    self.at_refuge(z, player, store);
                    self.note = Some("REFUGE: HEAL AND RESUPPLY, LEAVE WHEN READY".into());
                }
            }
            Stage::Refuge => {
                // Out of the door and on. The test walker stands at the goal,
                // so it is sent on after a few seconds.
                let walker = z.headless.walk && z.stage_t > 8.0;
                if z.to_goal(player.pos) > REFUGE_LEAVE_M || walker {
                    z.enter_leg(z.leg + 1);
                    let to = if z.last_leg() { "THE EXTRACTION" } else { "THE NEXT REFUGE" };
                    self.note = Some(format!("LEG {} OF {}: ON TO {to}", z.leg + 1, z.legs.len()));
                }
            }
            Stage::Finale if z.stage_t >= z.finale_len => {
                z.stage = Stage::Out;
                z.stage_t = 0.0;
                self.note = Some("THE WAY OUT IS OPEN: GET TO THE MARK".into());
            }
            Stage::Out if at_goal(z, OUT_M) => {
                z.over = Some((Outcome::Made, 0.0));
                self.note = Some("YOU MADE IT".into());
            }
            _ => {}
        }
    }

    /// A refuge reached: whoever of the party is down comes back at half
    /// health beside the player, and the zombies nobody can see are let go.
    fn at_refuge(&mut self, z: &mut Run, player: &Player, store: &Store) {
        let eye = player.eye(1.0);
        for k in z.party..self.bots.len() {
            if self.bots[k].fighter.alive() && !crate::combat::shot::sees(store, eye, self.bots[k].pos() + Vec3::Y * 1.2) {
                self.sleep(z, k);
            }
        }
        let back = -player.forward();
        for i in 0..z.party {
            if self.bots[i].fighter.alive() {
                continue;
            }
            let side = Vec3::new(-back.z, 0.0, back.x) * (1.1 * (i as f32 - 1.0));
            let mut at = player.pos + Vec3::new(back.x, 0.0, back.z) * 1.2 + side;
            match voxel_ground(store, at.x, at.z, player.pos.y + 1.0) {
                Some((y, _)) if (y - player.pos.y).abs() < 0.6 && crate::combat::shot::sees(store, player.pos + Vec3::Y, Vec3::new(at.x, y + 1.0, at.z)) => at.y = y + 0.05,
                _ => at = player.pos,
            }
            let b = &mut self.bots[i];
            b.fighter.revive(true);
            b.fighter.health = b.fighter.full_health * 0.5;
            b.figure.revive();
            stand(&mut b.body, at, player.yaw);
            self.corpses.retain(|c| c.bot != i);
        }
    }

    /// The finale: hordes from every side at a steady rate, and a crowd about
    /// between them, until the way out opens and after.
    fn run_finale(&mut self, z: &mut Run, s: &Survey, player: &Player, store: &Store, survivors: &[(usize, Vec3)], dt: f32) {
        z.since_horde += dt;
        if z.since_horde > FINALE_HORDE_S {
            z.since_horde = 0.0;
            z.wave_from = None;
            z.hordes += 1;
            let size = (z.draw(FINALE_SIZE) * z.press()).round() as usize;
            self.horde(z, s, player, store, survivors, size);
        }
        z.spawn_due -= dt;
        if z.spawn_due <= 0.0 {
            z.spawn_due = FINALE_SPAWN_GAP_S;
            let near = self.bots.iter().skip(z.party).filter(|b| b.fighter.alive() && b.pos().distance(player.pos) < AMBIENT_M).count();
            if near < (FINALE_CROWD * z.press()).round() as usize && z.zombies_up(self) < MOST_UP {
                if let Some(at) = self.find_spot(z, s, player, store, survivors, None) {
                    let runner = z.rng.unit() < RUNNER_SHARE;
                    self.wake(z, at, true, runner, false);
                }
            }
        }
        for k in z.party..self.bots.len() {
            let b = &self.bots[k];
            if b.fighter.alive() && (b.pos().distance(player.pos) > LOSE_M || b.pos().y < -20.0) {
                self.sleep(z, k);
                z.culled += 1;
            }
        }
    }

    /// Every bot's order this step: the party follows or holds, and every
    /// zombie that is up heads for the nearest survivor along the flow.
    fn orders_for(&mut self, z: &mut Run, player: &Player, store: &Store, survivors: &[(usize, Vec3)], dt: f32) {
        self.orders.resize(self.bots.len(), None);
        // Which way the party is going: along the last few metres of the
        // player's trail, else the way the player faces.
        let ahead = match (z.trail.len() >= 4).then(|| player.pos - z.trail[z.trail.len() - 4]) {
            Some(d) if d.x.hypot(d.z) > 0.5 => Vec3::new(d.x, 0.0, d.z).normalize(),
            _ => {
                let f = player.forward();
                Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z)
            }
        };
        let right = Vec3::new(-ahead.z, 0.0, ahead.x);
        for i in 0..z.party {
            // Its place in the formation where it can walk there and see
            // the player from it; else a little back along the trail, which
            // is somewhere the player has just been.
            let (sx, sz) = FORMATION[i % FORMATION.len()];
            let want = player.pos + right * sx + ahead * sz;
            let spot = voxel_ground(store, want.x, want.z, player.pos.y + 1.0).map(|(y, _)| Vec3::new(want.x, y, want.z)).filter(|p| {
                (p.y - player.pos.y).abs() < 0.8
                    && z.near.walk_m(*p).is_some_and(|m| m < sx.hypot(sz) * 1.6 + 2.0)
                    && crate::combat::shot::sees(store, player.pos + Vec3::Y * 1.2, *p + Vec3::Y * 1.2)
            });
            let lag = 2.2 + 1.3 * i as f32;
            let mut back = player.pos;
            let mut gone = 0.0;
            for t in z.trail.iter().rev() {
                gone += t.distance(back);
                back = *t;
                if gone >= lag {
                    break;
                }
            }
            self.orders[i] = Some(match z.hold {
                Some(post) => Order::new(Role::Defend, Goal::Hold { post, leash: 8.0, watch: post + player.forward() }, 0.5),
                None if self.me.alive() => Order::new(Role::Escort, Goal::Point(spot.unwrap_or(back)), 0.5),
                None => Order::new(Role::Escort, Goal::Follow(0), 0.5),
            });
        }
        let won = matches!(z.over, Some((Outcome::Made, _)));
        let mut stuck = Vec::new();
        for k in z.party..self.bots.len() {
            let slot = &mut z.slots[k - z.party];
            slot.cool = (slot.cool - dt).max(0.0);
            let b = &mut self.bots[k];
            if !b.fighter.alive() || won {
                self.orders[k] = None;
                continue;
            }
            let at = b.pos();
            let Some(&(who, foot)) = survivors.iter().min_by(|a, c| a.1.distance(at).total_cmp(&c.1.distance(at))) else {
                self.orders[k] = None;
                continue;
            };
            let d = foot.distance(at);
            let eye = b.body.eye(1.0);
            let see = |p: Vec3| crate::combat::shot::sees(store, eye, p + Vec3::Y * 1.2);
            // A blow wakes it, and so does anybody close or in plain view.
            // Downwind of the party, close enough, it smells them.
            let off = Vec3::new(at.x - foot.x, 0.0, at.z - foot.z);
            let smelt = off.length() < z.condition.scent_m() && off.dot(z.wind) > 0.7 * off.length();
            let (close, sight) = (z.condition.close_m(), z.condition.sight_m());
            if !slot.chasing && (b.fighter.struck.is_some() || d < close || smelt || (d < sight && see(foot))) {
                slot.chasing = true;
                slot.known = Some(foot);
                slot.sighted = true;
                slot.look = 0.0;
            }
            let idle = if slot.crawler { CRAWL_IDLE_MPS } else { IDLE_MPS };
            if !slot.chasing {
                b.body.walk_speed = Some(idle);
                self.orders[k] = None;
                continue;
            }
            slot.mark = Some(who);
            // Just landed a blow: it stands there, holding on, facing them.
            if slot.pause > 0.0 {
                slot.pause -= dt;
                slot.last = at;
                slot.still = 0.0;
                b.body.walk_speed = Some(0.0);
                let mut o = Order::new(Role::Attack, Goal::Point(at), 0.5);
                o.mark = Some(who);
                self.orders[k] = Some(o);
                continue;
            }
            // Looking for them, every second or few. Close in it hears them
            // and knows where they are all the time.
            slot.look -= dt;
            if d < close {
                slot.known = Some(foot);
                slot.sighted = true;
            } else if slot.look <= 0.0 || slot.known.is_none() {
                slot.look = LOOK_S.0 + z.rng.unit() * LOOK_S.1;
                slot.sighted = d < z.condition.track_m() && see(foot);
                if slot.sighted || slot.known.is_none() {
                    slot.known = Some(foot);
                    slot.angle = (z.rng.unit() * 2.0 - 1.0) * OFF_LINE;
                    slot.search = 0.0;
                }
            }
            let known = slot.known.unwrap_or(foot);
            let to_known = Vec3::new(known.x - at.x, 0.0, known.z - at.z).length();
            // Got there and nobody is: it pokes about the place for a while,
            // then gives up and stands about where it is.
            let searching = !slot.sighted && to_known < 2.5 || slot.search > 0.0;
            if searching {
                if slot.search == 0.0 {
                    slot.search_len = SEARCH_S.0 + z.rng.unit() * SEARCH_S.1;
                    slot.poke_t = 0.0;
                }
                slot.search += dt;
                slot.poke_t -= dt;
                if slot.poke_t <= 0.0 {
                    let a = z.rng.unit() * std::f32::consts::TAU;
                    let r = 2.0 + z.rng.unit() * 5.0;
                    slot.poke = known + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                    slot.poke_t = 2.0 + z.rng.unit() * 1.5;
                }
                if slot.search > slot.search_len {
                    *slot = Slot { runner: slot.runner, crawler: slot.crawler, ..Slot::default() };
                    b.body.walk_speed = Some(idle);
                    self.orders[k] = None;
                    continue;
                }
            }
            b.body.walk_speed = Some(if slot.crawler {
                CRAWL_MPS
            } else if searching {
                SEARCH_MPS
            } else if slot.runner {
                RUNNER_MPS
            } else {
                CHASE_MPS
            });
            // Not getting anywhere: against a wall the fields do not know
            // about, or in a crowd at a door. Out of the player's sight it
            // is put back in the pool, and another comes from somewhere else.
            if at.distance(slot.last) > 0.75 || d < REACH_M + 0.5 {
                slot.last = at;
                slot.still = 0.0;
            } else {
                slot.still += dt;
            }
            if slot.still > 6.0 && !crate::combat::shot::sees(store, player.eye(1.0), at + Vec3::Y * 1.2) {
                stuck.push(k);
                continue;
            }
            // Where to make for. At arm's length, straight at them. Where it
            // last saw them is about where they are, the fields' way there
            // (the fine one round walls and in at doors, the survey's further
            // out). Anywhere else, straight for the place, and on a slant.
            let goal = if d < 1.2 {
                foot
            } else if searching {
                slot.poke
            } else if known.distance(foot) < 4.0 {
                z.near.ahead(at, 5).or_else(|| z.flow.ahead(at, 3)).unwrap_or(known)
            } else {
                known
            };
            let goal = if d >= 1.2 && !searching && to_known > STRAIGHT_M {
                let v = goal - at;
                let (sa, ca) = slot.angle.sin_cos();
                at + Vec3::new(v.x * ca - v.z * sa, v.y, v.x * sa + v.z * ca)
            } else {
                goal
            };
            let urgency = if searching { 0.45 } else if slot.runner { zombie::RUN } else { 0.5 };
            let mut o = Order::new(Role::Attack, Goal::Point(goal), urgency);
            o.mark = (!searching).then_some(who);
            self.orders[k] = Some(o);
        }
        for k in stuck {
            self.orders[k] = None;
            self.sleep(z, k);
            z.culled += 1;
        }
    }

    /// The party bots: rounds to spare, a reload in a lull, and one left far
    /// behind brought up to the player.
    fn party_care(&mut self, z: &Run, player: &Player, store: &Store, _dt: f32) {
        for i in 0..z.party {
            let b = &mut self.bots[i];
            if !b.fighter.alive() {
                continue;
            }
            let low = b.fighter.arsenal.current().is_some_and(|g| g.def.mag > 0 && g.ammo < g.def.mag * 2);
            if low {
                let held = b.fighter.arsenal.held;
                b.fighter.arsenal.refill();
                b.fighter.arsenal.held = held;
            }
            if b.fighter.arsenal.current().is_some_and(|g| g.loaded == 0 && g.reload <= 0.0) {
                b.fighter.arsenal.reload();
            }
            if z.hold.is_none() && b.pos().distance(player.pos) > 60.0 && self.me.alive() {
                let back = -player.forward();
                let at = player.pos + Vec3::new(back.x, 0.0, back.z) * 3.0;
                if let Some((y, _)) = voxel_ground(store, at.x, at.z, player.pos.y + 1.0) {
                    stand(&mut b.body, Vec3::new(at.x, y + 0.05, at.z), player.yaw);
                }
            }
        }
    }

    /// After the bodies have moved: a body held up against something low
    /// enough climbs over it, and one already climbing is moved along its
    /// climb, whatever its keys did this step. Zombies and the party alike:
    /// a hedge stops neither.
    pub(super) fn zombie_vaults(&mut self, player: &Player, store: &Store, dt: f32) {
        let Some(mut z) = self.zed.take() else { return };
        let n = self.bots.len();
        z.vaults.resize(n, None);
        z.held_up.resize(n, (Vec3::ZERO, 0.0));
        for k in 0..n {
            let b = &mut self.bots[k];
            if !b.fighter.alive() {
                z.vaults[k] = None;
                b.figure.vault = None;
                continue;
            }
            if let Some(v) = z.vaults[k].as_mut() {
                v.t += dt;
                b.body.pos = v.at();
                b.body.body.vel = Vec3::ZERO;
                if v.t >= v.len {
                    z.vaults[k] = None;
                    b.figure.vault = None;
                    b.body.body.on_ground = true;
                } else {
                    b.figure.vault = Some(v.t / v.len);
                }
                continue;
            }
            // Where it is trying to get to.
            let goal = match self.orders.get(k).copied().flatten().map(|o| o.goal) {
                Some(Goal::Point(p)) => Some(p),
                Some(Goal::Hold { post, .. }) => Some(post),
                Some(Goal::Follow(0)) => Some(player.pos),
                _ => None,
            };
            let at = b.body.pos;
            let (last, held) = z.held_up[k];
            let want = goal.map(|g| Vec3::new(g.x - at.x, 0.0, g.z - at.z)).filter(|w| w.length() > 1.0);
            let moved = Vec3::new(at.x - last.x, 0.0, at.z - last.z).length();
            let held = if want.is_some() && moved < 0.5 * dt { held + dt } else { 0.0 };
            z.held_up[k] = (at, held);
            let crawler = k >= z.party && z.slots.get(k - z.party).is_some_and(|s| s.crawler);
            let (Some(w), true) = (want, held > VAULT_AFTER_S && b.body.body.on_ground && !crawler) else { continue };
            let dir = w.normalize();
            if let Some((top, to)) = vault_over(store, at, dir, b.body.body.half_w, b.body.body.height) {
                // Quicker for a runner and for the party; longer the higher.
                let quick = if k < z.party || z.slots.get(k - z.party).is_some_and(|s| s.runner) { 0.75 } else { 1.0 };
                z.vaults[k] = Some(Vault { from: at, top, to, t: 0.0, len: (0.7 + 0.35 * (top - at.y)) * quick });
                z.held_up[k] = (to, 0.0);
                b.figure.vault = Some(0.0);
            }
        }
        self.zed = Some(z);
    }

    /// After the bodies have moved: every zombie in reach of a survivor and
    /// past its cooldown lands a blow.
    pub(super) fn zombie_strikes(&mut self, player: &mut Player) {
        let Some(mut z) = self.zed.take() else { return };
        if z.over.is_none() {
            let survivors = self.survivors(player, &z);
            for k in z.party..self.bots.len() {
                if !self.bots[k].fighter.alive() || z.slots[k - z.party].cool > 0.0 || z.vaults.get(k).is_some_and(|v| v.is_some()) {
                    continue;
                }
                let at = self.bots[k].pos();
                let hit = survivors.iter().copied().find(|(_, p)| (p.x - at.x).hypot(p.z - at.z) < REACH_M && (p.y - at.y).abs() < 1.6);
                let Some((who, p)) = hit else { continue };
                z.slots[k - z.party].cool = BLOW_S * (0.85 + 0.3 * z.rng.unit());
                z.slots[k - z.party].pause = GRAB_S;
                z.grabs.push((who, GRAB_S));
                let push = Vec3::new(p.x - at.x, 0.0, p.z - at.z).normalize_or_zero() * 0.8;
                let (f, body) = if who == 0 {
                    (&mut self.me, &mut player.body)
                } else {
                    let b = &mut self.bots[who - 1];
                    (&mut b.fighter, &mut b.body.body)
                };
                body.vel += push;
                f.blame = Some((k + 1, brain::BLAME_S));
                f.struck = Some(k + 1);
                z.hurt += z.blow.min(f.health.max(0.0));
                if f.take(z.blow) {
                    self.down(who, k + 1, player, push, false);
                }
            }
        }
        // Whoever is held is slowed, more for each zombie holding them.
        let dt = crate::game::STEP;
        for g in &mut z.grabs {
            g.1 -= dt;
        }
        z.grabs.retain(|g| g.1 > 0.0 && z.over.is_none());
        let held = |who: usize| z.grabs.iter().filter(|g| g.0 == who).count() as f32;
        player.slowed = (1.0 - HELD_PACE * held(0)).max(HELD_LEAST);
        for i in 0..z.party {
            self.bots[i].body.slowed = (1.0 - HELD_PACE * held(i + 1)).max(HELD_LEAST);
        }
        self.zed = Some(z);
    }

    /// A zombie went down: counted, and its slot forgets what it was doing.
    pub(super) fn zombie_down(&mut self, who: usize) {
        if let Some(z) = self.zed.as_mut() {
            if who > z.party {
                z.kills += 1;
                z.slots[who - 1 - z.party] = Slot::default();
            }
        }
    }

    /// X: the party holds where the player stands, or follows again.
    pub(super) fn toggle_hold(&mut self, at: Vec3) {
        if let Some(z) = self.zed.as_mut() {
            z.hold = match z.hold {
                Some(_) => None,
                None => Some(at),
            };
            self.note = Some(if z.hold.is_some() { "PARTY: HOLD HERE".into() } else { "PARTY: FOLLOW ME".into() });
        }
    }

    /// Another go at the same round: everybody back at the start, whole, the
    /// zombies gone, the caches full again, the world as it was.
    fn restart_zombies(&mut self, z: &mut Run, player: &mut Player, store: &Store) {
        for k in z.party..self.bots.len() {
            self.sleep(z, k);
        }
        let floor = voxel_ground(store, z.start.x, z.start.z, z.start.y + 1.0).map_or(z.start, |(y, _)| Vec3::new(z.start.x, y + 0.1, z.start.z));
        let to = z.legs[0].goal - z.start;
        stand(player, floor, (-to.x).atan2(-to.z));
        player.pitch = 0.0;
        self.me.revive(false);
        self.arm_me();
        self.me.score = brain::Score::default();
        for i in 0..z.party {
            let b = &mut self.bots[i];
            b.fighter.revive(true);
            b.fighter.score = brain::Score::default();
            b.figure.revive();
        }
        self.place_party(z, player, store);
        self.corpses.clear();
        self.gibs.clear();
        self.combat.clear();
        self.feed.clear();
        self.clean_world = true;
        // The pads go down again on the nav clock (`settle_nav`).
        self.pickups = super::pickup::Pickups::default();
        self.nav_tries = super::NAV_TRIES;
        self.nav_due = 0;
        let legs = std::mem::take(&mut z.legs);
        *z = Run::begin(z.start, legs, z.party, z.blow, z.crowd, z.condition, Rng::new(z.rng.state), true);
        self.note = Some("ANOTHER GO".into());
    }

    /// The round for the headless report.
    pub fn zombie_report(&self) -> Option<serde_json::Value> {
        let z = self.zed.as_ref()?;
        let (up, of) = z.standing(self);
        Some(serde_json::json!({
            "start": [z.start.x, z.start.y, z.start.z],
            "legs": z.legs.iter().map(|l| serde_json::json!({"goal": [l.goal.x, l.goal.y, l.goal.z], "zone": l.name, "route_m": l.route_m})).collect::<Vec<_>>(),
            "leg": z.leg + 1,
            "stage": format!("{:?}", z.stage),
            "stage_t": z.stage_t,
            "finale_s": z.finale_len,
            "condition": z.condition.title(),
            "my_ground": self.me_ground,
            "hurt": z.hurt,
            "blasts": z.blasts,
            "swim_s": z.swim_s,
            "tags": z.tags(self),
            "goal": [z.goal.x, z.goal.y, z.goal.z],
            "goal_zone": z.goal_name,
            "route_m": z.route_m,
            "route_points": z.route.len(),
            "clock_s": z.clock,
            "phase": z.phase_name(),
            "heat": z.heat,
            "noise": z.noise,
            "hordes": z.hordes,
            "zombies_up": z.zombies_up(self),
            "spawned": z.spawned,
            "culled": z.culled,
            "cache_pads": self.pickups.pads.iter().filter(|p| matches!(p.kind, super::pickup::Kind::Weapon)).map(|p| serde_json::json!({"gun": p.weapon, "at": [p.at.x, p.at.y, p.at.z], "placed": p.placed, "ready": p.ready()})).collect::<Vec<_>>(),
            "kills": z.kills,
            "party_standing": [up, of],
            "player_health": self.me.health,
            "over": z.over.map(|(o, _)| format!("{o:?}")),
            "my_guns": self.me.arsenal.guns.iter().filter(|g| g.have).map(|g| format!("{} {}+{}", g.def.name, g.loaded, g.ammo)).collect::<Vec<_>>(),
            "party": self.bots.iter().take(z.party).map(|b| serde_json::json!({
                "name": b.name, "alive": b.fighter.alive(), "health": b.fighter.health, "pos": [b.pos().x, b.pos().y, b.pos().z],
                "doing": b.brain.doing(), "kills": b.fighter.score.kills, "shots": b.fighter.score.shots,
            })).collect::<Vec<_>>(),
            "zombies": self.bots.iter().enumerate().skip(z.party).filter(|(_, b)| b.fighter.alive()).map(|(k, b)| serde_json::json!({
                "pos": [b.pos().x, b.pos().y, b.pos().z], "doing": b.brain.doing(), "chasing": z.slots[k - z.party].chasing,
            })).collect::<Vec<_>>(),
        }))
    }

    /// The refuge, marked through walls, while the round is on.
    pub(super) fn zombie_marks(&self, player: &Player) -> Vec<crate::modes::Mark> {
        let Some(z) = self.zed.as_ref() else { return Vec::new() };
        if z.over.is_some() {
            return Vec::new();
        }
        let d = z.to_goal(player.pos);
        let green = [0.55, 1.0, 0.6, 0.95];
        let mut out = Vec::new();
        let label = match z.stage {
            Stage::Leg if z.last_leg() => format!("EXTRACTION {d:.0} M"),
            Stage::Leg => format!("REFUGE {d:.0} M"),
            Stage::Refuge => "REFUGE".to_string(),
            Stage::Finale => {
                let left = (z.finale_len - z.stage_t).max(0.0) as u32;
                format!("HOLD OUT {}:{:02}", left / 60, left % 60)
            }
            Stage::Out => format!("WAY OUT {d:.0} M"),
        };
        out.push(crate::modes::Mark { at: z.goal + Vec3::Y * 1.5, ink: green, label });
        // At a refuge, where the next leg goes; and the extraction is shown
        // from the start, so the party can scout it on the way.
        if z.stage == Stage::Refuge {
            if let Some(next) = z.legs.get(z.leg + 1) {
                let m = (next.goal - player.pos).length();
                let what = if z.leg + 2 >= z.legs.len() { "EXTRACTION" } else { "NEXT REFUGE" };
                out.push(crate::modes::Mark { at: next.goal + Vec3::Y * 1.5, ink: green, label: format!("{what} {m:.0} M") });
            }
        }
        if !z.last_leg() {
            if let Some(end) = z.legs.last() {
                let m = (end.goal - player.pos).length();
                out.push(crate::modes::Mark { at: end.goal + Vec3::Y * 1.5, ink: [0.55, 1.0, 0.6, 0.5], label: format!("EXTRACTION {m:.0} M") });
            }
        }
        // Each cache whose gun is still there, within reach of a detour.
        for p in self.pickups.pads.iter().filter(|p| p.ready() && matches!(p.kind, super::pickup::Kind::Weapon)) {
            let m = (p.at - player.pos).length();
            if m < CACHE_SHOWN_M {
                let gun = p.weapon.as_deref().unwrap_or("gun").replace('_', " ").to_uppercase();
                out.push(crate::modes::Mark { at: p.at + Vec3::Y * 1.2, ink: [1.0, 0.85, 0.35, 0.9], label: format!("{gun} {m:.0} M") });
            }
        }
        out
    }
}

/// `VG_ZOMBIE_WALK`: walk the player down the planned route, at a walk,
/// through the fine field toward a point on the route 35 m on, so doors and
/// walls are gone round as a zombie goes round them. A test aid, not a way
/// to play: `walk` is the field it keeps and the seconds until it is rebuilt.
fn walk_route(z: &mut Run, walk: &mut (Near, f32), player: &mut Player, store: &Store, land: &World, dt: f32) {
    walk.1 -= dt;
    if walk.1 <= 0.0 {
        walk.1 = 1.0;
        let near = z.route.iter().enumerate().min_by(|a, b| a.1.distance(player.pos).total_cmp(&b.1.distance(player.pos))).map_or(0, |(k, _)| k);
        let mut k = near;
        let mut gone = 0.0;
        while k + 1 < z.route.len() && gone < 35.0 {
            gone += z.route[k].distance(z.route[k + 1]);
            k += 1;
        }
        let aim = if k + 1 >= z.route.len() { z.goal } else { z.route[k] };
        walk.0 = Near::build(store, land, &mut z.floors, &mut z.edges, player.pos, &[aim], 6, false);
    }
    let Some(next) = walk.0.ahead(player.pos, 3) else { return };
    // Pressed keys, as a bot's are: the body collides, climbs and opens
    // doors exactly as anybody's does.
    let to = Vec3::new(next.x - player.pos.x, 0.0, next.z - player.pos.z);
    if to.length() > 0.05 {
        player.yaw = (-to.x).atan2(-to.z);
    }
    let level = to.normalize_or_zero();
    let ahead = player.pos + level * 0.7;
    let jump = crate::physics::hits(store, ahead + Vec3::Y * 0.45, 0.3, 0.05) && !crate::physics::hits(store, ahead + Vec3::Y * 1.3, 0.3, 0.05);
    let keys = crate::brain::steer::drive(player.yaw, &crate::brain::steer::Want { go: to, jump, ..Default::default() });
    player.walk(&keys, store, (-1.0e9, glam::Vec2::ZERO), true, dt);
}

/// How long the corpse of a zombie lies, seconds: long enough to read what
/// happened, short enough that a long fight is not ankle deep.
pub const CORPSE_S: f32 = 20.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::survey::graph::Anchor;
    use std::path::Path;

    fn anchor(kind: Kind, x: f32, z: f32) -> Anchor {
        Anchor {
            kind,
            pos: [x, 0.0, z],
            facing: [0.0, 1.0],
            zone: u16::MAX,
            above_floor: 0.0,
            open: 1.0,
            sightline: 50.0,
            ways_in: 8,
            flat: 10.0,
            cover: 0.0,
            roofed: false,
            drink: 100.0,
            graze: 0.0,
            browse: 0.0,
        }
    }

    /// A 1.6 km square anybody can walk anywhere on, with a refuge at the
    /// start, two more 600 m on each, an extraction 600 m past those, and a
    /// nook beside each of the first two legs for a cache.
    fn open_land() -> (Survey, Vec3) {
        let mut s = Survey::blank(400);
        s.grid_mut(Chan::PassPerson).fill(255);
        s.graph.anchors = vec![
            anchor(Kind::Refuge, 200.0, 200.0),
            anchor(Kind::Refuge, 800.0, 200.0),
            anchor(Kind::Refuge, 800.0, 800.0),
            anchor(Kind::Focal, 1400.0, 800.0),
            anchor(Kind::Nook, 500.0, 220.0),
            anchor(Kind::Nook, 780.0, 500.0),
        ];
        (s, Vec3::new(201.0, 0.0, 199.0))
    }

    fn kit(name: &str) -> &'static KitDef {
        crate::weapon_kit::get(name).unwrap_or_else(|| panic!("a {name} kit"))
    }

    #[test]
    fn a_round_is_planned_on_from_refuge_to_extraction() {
        let (s, near) = open_land();
        let p = plan(&s, near, Vec3::ZERO, 7, kit("realistic")).expect("a plan");
        assert_eq!(p.start, Vec3::new(200.0, 0.0, 200.0), "the start is the refuge by the player");
        let goals: Vec<Vec3> = p.legs.iter().map(|l| l.goal).collect();
        assert_eq!(goals, [Vec3::new(800.0, 0.0, 200.0), Vec3::new(800.0, 0.0, 800.0), Vec3::new(1400.0, 0.0, 800.0)]);
        for l in &p.legs {
            assert!((LEG.0..=LEG.1).contains(&l.route_m), "a leg of {} m", l.route_m);
            assert!(l.route.first().unwrap().distance(*l.route.last().unwrap()) > 500.0);
            assert_eq!(*l.route.last().unwrap(), l.goal);
        }
    }

    /// Every pad holds a gun the round hands out, and they are the kit's:
    /// played with the kit the plan was made for, nothing is swapped.
    #[test]
    fn the_caches_hold_the_kits_guns() {
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        let (s, near) = open_land();
        for name in ["realistic", "arena"] {
            let k = kit(name);
            for seed in 0..6 {
                let mut p = plan(&s, near, Vec3::ZERO, seed, k).unwrap();
                let w = &p.rules.weapons;
                for role in FOUND.iter().chain(&["melee", "sidearm"]) {
                    assert!(w.iter().any(|g| k.roles[*role].contains(g)), "{name}: no {role} gun in {w:?}");
                }
                assert!(p.rules.pickups.iter().any(|x| x.kind == "weapon"), "{name}: no caches");
                for g in p.rules.pickups.iter().filter_map(|x| x.weapon.as_deref()) {
                    assert!(w.iter().any(|x| x == g), "{name}: a pad for {g}, which the round does not hand out");
                }
                assert_eq!(crate::weapon_kit::apply(&mut p.rules, &lib, k, seed), crate::weapon_kit::Applied::default(), "{name}");
            }
        }
    }

    /// A kit with no explosive or heavy gun leaves those caches out rather
    /// than filling them with a gun it has not got.
    #[test]
    fn a_role_the_kit_lacks_has_no_caches() {
        let mut k = kit("realistic").clone();
        k.roles.remove("explosive");
        k.roles.remove("heavy");
        let (s, near) = open_land();
        let p = plan(&s, near, Vec3::ZERO, 3, &k).unwrap();
        for gone in ["rpg", "machine_gun"] {
            assert!(!p.rules.weapons.iter().any(|g| g == gone));
            assert!(!p.rules.pickups.iter().any(|x| x.weapon.as_deref() == Some(gone)));
        }
        assert!(p.rules.pickups.iter().any(|x| x.kind == "weapon"));
    }

    #[test]
    fn no_refuge_in_reach_is_no_round() {
        let (mut s, near) = open_land();
        s.graph.anchors.retain(|a| a.pos[0] < 300.0);
        assert!(plan(&s, near, Vec3::ZERO, 1, kit("realistic")).is_err());
    }

    /// The survey's field goes round a wall through its gap, never through
    /// it or across a corner, and downhill on it is the way to the source.
    #[test]
    fn the_flow_goes_round_a_wall() {
        let mut s = Survey::blank(50);
        let n = 50;
        let pass = s.grid_mut(Chan::PassPerson);
        pass.fill(255);
        for z in 0..40 {
            pass[z * n + 25] = 0;
        }
        let cell = |x: i32, z: i32| Vec3::new((x as f32 + 0.5) * CELL_M, 0.0, (z as f32 + 0.5) * CELL_M);
        let f = Flow::build(&s, 0, 0, 50, &[cell(10, 10)]);
        assert_eq!(f.metres(cell(20, 10)), Some(40.0));
        assert_eq!(f.metres(cell(25, 10)), None, "the wall is no way");
        let round = f.metres(cell(40, 10)).expect("the far side is reached through the gap");
        assert!(round > 2.0 * 30.0 * CELL_M, "{round} m: through the wall rather than round it");
        // Walked downhill from the far side, it comes back through the gap.
        let mut at = cell(40, 10);
        let mut through = false;
        for _ in 0..200 {
            let Some(next) = f.ahead(at, 1) else { break };
            assert!(f.of(next) < f.of(at));
            let (x, z) = Flow::cell(next);
            if x == 25 {
                assert!(z >= 40, "crossed the wall at z {z}");
                through = true;
            }
            at = next;
        }
        assert!(through);
        assert_eq!(Flow::cell(at), (10, 10));
    }

    #[test]
    fn a_climb_goes_up_over_and_down() {
        let v = |t| Vault { from: Vec3::ZERO, top: 1.0, to: Vec3::new(2.0, 0.2, 0.0), t, len: 1.0 };
        assert_eq!(v(0.0).at(), Vec3::ZERO);
        assert!((v(0.55).at().y - 1.05).abs() < 1e-5, "over the top");
        assert!(v(1.0).at().distance(Vec3::new(2.0, 0.2, 0.0)) < 1e-5);
        // It never dips below the top while it is over the wall.
        for i in 45..=70 {
            assert!(v(i as f32 / 100.0).at().y >= 1.0);
        }
    }

    #[test]
    fn conditions_parse_by_title_or_name() {
        for c in Condition::ALL {
            assert_eq!(Condition::parse(c.title()), Some(c));
            assert_eq!(Condition::parse(&format!("{c:?}").to_lowercase()), Some(c));
        }
        assert_eq!(Condition::parse("hail"), None);
        // Nobody sees further at night or in fog than on a clear day.
        for c in [Condition::Night, Condition::Fog] {
            assert!(c.sight_m() < Condition::Clear.sight_m() && c.track_m() < Condition::Clear.track_m());
        }
    }

    /// Each leg presses harder than the one before, and a new leg starts
    /// the director from a build-up.
    #[test]
    fn each_leg_presses_harder() {
        let (s, near) = open_land();
        let p = plan(&s, near, Vec3::ZERO, 7, kit("realistic")).unwrap();
        let mut run = Run::begin(p.start, p.legs, 1, BLOW, 1.0, Condition::Clear, Rng::new(9), true);
        assert!((FIRST_BUILD_S.0..=FIRST_BUILD_S.0 + FIRST_BUILD_S.1).contains(&run.phase_len));
        let mut last = run.press();
        for leg in 1..run.legs.len() {
            run.phase = Phase::Relax;
            run.enter_leg(leg);
            assert_eq!(run.phase, Phase::Build);
            assert!((LEG_BUILD_S.0..=LEG_BUILD_S.0 + LEG_BUILD_S.1).contains(&run.phase_len));
            assert!(run.press() > last);
            last = run.press();
        }
        assert!(run.last_leg());
        // The way ahead is along the route, about as far on as asked.
        let on = run.route_ahead(run.route[0], 40.0);
        let gone = (on - run.route[0]).length();
        assert!((40.0..48.0).contains(&gone), "{gone} m on");
    }
}
