//! Swimmers: fish that school and whales that spout and breach. The spray
//! they throw up is particles (src/fx): they push a `fx::Cue` and `Game`
//! throws it.
//!
//! - `school`: a school of fish steering by the same boids rules as a starling
//!   flock (flock.rs `steer`), in three dimensions under the surface. The
//!   school wanders round a home in water at least two metres deep, keeps out
//!   of the shallows, scatters away from the player, and now and then a fish
//!   leaps clear of the water and drops back with a small splash.
//! - `breach`: a whale alone on a long slow course offshore. It dives for half
//!   a minute or so, comes up and lies at the surface spouting every few
//!   seconds, and sometimes ends a dive by going deep and driving up out of
//!   the water, twisting onto its side before it falls back in a big splash.
//!
//! Neither has a physics body: water has no voxels (the sea is a plane at
//! sea level), so they are kept between the bed and the surface by hand.
//! `Terrain::bed` is the height of the sea or lake bed.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::anim::{Plan, Role};
use crate::fx::Cue;
use crate::world::noise::{hash32, Rng};
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// What the swimmers did, for the headless stats.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct Stats {
    pub fish: u32,
    pub whales: u32,
    pub leaps: u32,
    /// Hunters rushing a school this step, and fish caught.
    pub chases: u32,
    pub caught: u32,
    pub spouts: u32,
    pub breaches: u32,
    /// Fish-steps spent in water shallower than a metre.
    pub shallow: u32,
}

/// How a school lives (library `habit`). See `Behaviour::habit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Habit {
    #[default]
    Open,
    Cloud,
    Hover,
    Patrol,
    Rest,
    Bait,
    Pulse,
    Burrow,
    Cruise,
}

pub const HABITS: [&str; 9] = ["open", "cloud", "hover", "patrol", "rest", "bait", "pulse", "burrow", "cruise"];

impl Habit {
    pub fn of(sp: &Species) -> Habit {
        match sp.beh.habit.as_deref() {
            Some("cloud") => Habit::Cloud,
            Some("hover") => Habit::Hover,
            Some("patrol") => Habit::Patrol,
            Some("rest") => Habit::Rest,
            Some("bait") => Habit::Bait,
            Some("pulse") => Habit::Pulse,
            Some("burrow") => Habit::Burrow,
            Some("cruise") => Habit::Cruise,
            _ => Habit::Open,
        }
    }

    /// Minds the diver at all: these go about their business whoever comes.
    fn startles(self) -> bool {
        !matches!(self, Habit::Patrol | Habit::Cruise | Habit::Pulse | Habit::Burrow)
    }

    /// How far a new home may be from the old (or, for those tied to one
    /// spot, from where it was born), metres: (least, most).
    fn roam(self) -> (f32, f32) {
        match self {
            Habit::Cloud => (0.5, 3.0),
            Habit::Hover => (3.0, 9.0),
            Habit::Rest => (5.0, 12.0),
            Habit::Patrol => (25.0, 55.0),
            Habit::Cruise => (40.0, 80.0),
            Habit::Bait => (10.0, 25.0),
            _ => (8.0, 28.0),
        }
    }
}

pub struct School {
    pub species: usize,
    habit: Habit,
    /// Where it was born: a cloud keeps to its coral head, a hover to its
    /// patch of reef.
    anchor: Vec3,
    home: Vec3,
    goal: Vec3,
    timer: f32,
    phase: f32,
    flee: f32,
    /// Hover: seconds left hanging still, and to the next pause.
    pause: f32,
    /// Cloud: seconds left down in the coral after a fright.
    hide: f32,
    /// A hunter: the school it is rushing and for how much longer, and
    /// seconds until it looks for another.
    chase: Option<hecs::Entity>,
    chase_t: f32,
    rest_t: f32,
    rng: Rng,
}

pub struct Fish {
    pub school: hecs::Entity,
    vel: Vec3,
    wag: f32,
    air: bool,
    rng: Rng,
    /// Where in its depth band this one swims, 0 to 1, so a school is
    /// spread up and down a little, not all at one level.
    lane: f32,
    /// Burrow: its hole in the sand, and how far out of it it stands (0 to 1).
    anchor: Vec3,
    ext: f32,
    /// A `back_float` swimmer: how far over on its back it lies, 0 to 1.
    back: f32,
}

/// The height a fish of species `sp` living as `habit` swims at, over bed `bed`
/// under surface `top`, at `lane` (0 to 1) of its band.
/// A swimmer that floats low and swims level: anything as tall as a seal
/// or more, and the water form of any land animal whatever its size. (The
/// harbour seal is 0.55 m: by height alone it pitched like a fish, and one
/// left on a bar too shallow to sink in stood on its nose in the sand.)
fn big_swimmer(sp: &Species, m: &crate::models::Model) -> bool {
    sp.is_water_form || m.height > 0.6
}

/// How fast a big swimmer's heading can swing, radians a second: a seal or
/// a walrus turns in arcs, so the shoal and rock checks below steer it
/// round rather than flick it about on the spot.
const BIG_TURN: f32 = 1.6;

fn band_y(sp: &Species, habit: Habit, bed: f32, top: f32, lane: f32) -> f32 {
    let room = (top - 0.3).max(bed + 0.1);
    if habit == Habit::Rest {
        return bed + 0.12;
    }
    if let Some([a, b]) = sp.beh.above_bed {
        return (bed + a + (b - a) * lane).clamp(bed + 0.1, room);
    }
    if let Some([a, b]) = sp.beh.depth {
        // In water too shallow for the band, the floor of it wins over the
        // room (min must not pass max).
        return (top - a - (b - a) * lane).clamp((bed + 0.5).min(room), room);
    }
    top - swim_depth(sp, top - bed)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    /// Under, cruising; `timer` until it comes up.
    Dive,
    /// At the surface, back awash, spouting.
    Surface,
    /// Going deep before a breach.
    Sink,
    /// Driving up toward the surface.
    Rise,
    /// Out of the water.
    Air,
}

pub struct Whale {
    pub phase: Phase,
    heading: f32,
    speed: f32,
    vy: f32,
    timer: f32,
    spout: f32,
    goal: Vec3,
    pitch: f32,
    roll: f32,
    wag: f32,
    rng: Rng,
}

fn rest_anim(models: &[crate::models::Model], model: usize) -> Option<crate::anim::Anim> {
    models[model].rig.as_ref().map(|r| crate::anim::Anim::hold(crate::anim::Pose::rest(r.bones.len())))
}

/// Blend `pose` toward an otter lying on its back at the surface, `t` of the
/// way (0 belly down, 1 on its back): rolled over about the body's middle,
/// head raised to look along its chest, forepaws folded on it and hind feet
/// up. Rotations are in the body's frame, which is upside down once rolled,
/// so a raised head is a negative pitch.
pub fn back_pose(rig: &crate::anim::Rig, pose: &mut crate::anim::Pose, t: f32, wag: f32) {
    if t <= 0.0 {
        return;
    }
    let t = t.min(1.0);
    let ease = t * t * (3.0 - 2.0 * t);
    let mix = |v: &mut Vec3, to: Vec3| *v = v.lerp(to, ease);
    let n = pose.bones.len();
    for (i, b) in rig.bones.iter().enumerate().take(n) {
        let upper = crate::anim::motors::upper_leg(rig, i);
        let v = &mut pose.bones[i];
        match b.role {
            // A slow sway on the swell.
            _ if i == 0 => mix(v, Vec3::new(0.0, 0.0, PI + (wag * 0.13).sin() * 0.12)),
            Role::Neck => mix(v, Vec3::new(-0.45, 0.0, 0.0)),
            Role::Head => mix(v, Vec3::new(-0.25, 0.0, 0.0)),
            Role::Leg if upper && b.pair == 0 => mix(v, Vec3::new(1.0, 0.0, b.side * 0.35)),
            Role::Leg if upper => mix(v, Vec3::new(-0.35, 0.0, 0.0)),
            Role::Tail => mix(v, Vec3::new(0.15, (wag * 0.2).sin() * 0.1, 0.0)),
            _ => {}
        }
    }
}

/// How deep a fish swims in water `depth` deep: near the surface, deeper in
/// deeper water, never close to the bed.
fn fish_depth(depth: f32) -> f32 {
    (depth * 0.35).clamp(0.45, 2.5).min(depth * 0.5)
}

/// How deep species `sp` swims in water `depth` deep: an `awash` swimmer
/// (a crocodile) lies with its back at the surface.
fn swim_depth(sp: &Species, depth: f32) -> f32 {
    // The pivot is at the feet: a crocodile's back and eyes stand a hand
    // out of the water with its feet this far down.
    if sp.beh.awash {
        0.3
    } else {
        fish_depth(depth)
    }
}

/// A school of `n` fish of species `s` at surface point `at`. Now and then
/// a rare morph of the species swims among them.
#[allow(clippy::too_many_arguments)]
pub fn spawn_school(world: &mut hecs::World, s: usize, species: &[Species], models: &[crate::models::Model], at: Vec3, depth: f32, n: u32, seed: u32) {
    let sp = &species[s];
    let mut rng = Rng::new(seed | 1);
    let habit = Habit::of(sp);
    let sc = School {
        species: s,
        habit,
        anchor: at,
        home: at,
        goal: at,
        timer: 8.0 + rng.unit() * 12.0,
        phase: rng.unit() * TAU,
        flee: 0.0,
        pause: 4.0 + rng.unit() * 6.0,
        hide: 0.0,
        chase: None,
        chase_t: 0.0,
        rest_t: 4.0 + rng.unit() * 10.0,
        rng: Rng::new(hash32(seed ^ 0x5c4) | 1),
    };
    let se = world.spawn((sc,));
    let a = rng.unit() * TAU;
    let v = Vec3::new(a.sin(), 0.0, a.cos()) * sp.beh.walk_speed.max(0.5);
    for k in 0..n.max(1) {
        let spread = match habit {
            // Garden eels stand a metre or so apart in their colony.
            Habit::Burrow => 1.2 * (n as f32).sqrt(),
            Habit::Cloud => 0.6 + (n as f32).sqrt() * 0.2,
            _ => 1.0 + (n as f32).sqrt() * 0.6,
        };
        let off = Vec3::new(rng.unit() - 0.5, (rng.unit() - 0.5) * 0.4, rng.unit() - 0.5) * spread;
        let pos = at - Vec3::Y * swim_depth(&species[s], depth) + off;
        let yaw = v.x.atan2(v.z) + PI;
        let ms = crate::variety::pick(&sp.morphs, rng.unit()).unwrap_or(s);
        let model = species[ms].model;
        let e = world.spawn((
            Fish {
                school: se,
                vel: if habit == Habit::Burrow { Vec3::ZERO } else { v },
                wag: rng.unit() * TAU,
                air: false,
                rng: Rng::new(hash32(seed ^ k.wrapping_mul(0x85eb_ca6b)) | 1),
                lane: rng.unit(),
                anchor: pos,
                ext: 1.0,
                back: if sp.beh.back_float { 1.0 } else { 0.0 },
            },
            Creature {
                species: ms,
                spawned_for: species[s].quota,
            },
            Look::new(model, pos, yaw),
        ));
        if let Some(a) = rest_anim(models, model) {
            let _ = world.insert_one(e, a);
        }
    }
}

/// A whale of species `s` (or now and then one of its rare morphs) at
/// surface point `at`, starting under.
pub fn spawn_whale(world: &mut hecs::World, s: usize, species: &[Species], models: &[crate::models::Model], at: Vec3, seed: u32) {
    let mut rng = Rng::new(seed | 1);
    let ms = crate::variety::pick(&species[s].morphs, rng.unit()).unwrap_or(s);
    let sp = &species[ms];
    let heading = rng.unit() * TAU;
    let w = Whale {
        phase: Phase::Dive,
        heading,
        speed: sp.beh.walk_speed,
        vy: 0.0,
        timer: 4.0 + rng.unit() * 10.0,
        spout: 0.0,
        goal: at,
        pitch: 0.0,
        roll: 0.0,
        wag: 0.0,
        rng: Rng::new(hash32(seed ^ 0xb1ea) | 1),
    };
    let pos = at - Vec3::Y * 5.0;
    let e = world.spawn((
        w,
        Creature {
            species: ms,
            spawned_for: species[s].quota,
        },
        Look::new(sp.model, pos, heading + PI),
    ));
    if let Some(a) = rest_anim(models, sp.model) {
        let _ = world.insert_one(e, a);
    }
}

/// A splash at surface point `at` (a seal diving in): `size` 0.5 small, 1 big.
pub fn splash_at(cues: &mut Vec<Cue>, at: Vec3, size: f32) {
    splash(cues, "splash", at, 0.6 * size, size * 1.2);
    splash(cues, "splash_mist", at, 0.6 * size, size);
}

/// A burst of `name` thrown up from the surface at `at`.
fn splash(cues: &mut Vec<Cue>, name: &'static str, at: Vec3, radius: f32, amount: f32) {
    crate::fx::cue(
        cues,
        Cue {
            name,
            at,
            radius,
            dir: Vec3::Y,
            amount,
        },
    );
}

/// Step every swimmer and every blob of spray.
#[allow(clippy::too_many_arguments)]
pub fn update(
    world: &mut hecs::World,
    species: &[Species],
    groups: &[super::flock::Group],
    models: &[crate::models::Model],
    dt: f32,
    threat: &super::startle::Threat,
    terrain: &Terrain,
    stats: &mut Stats,
    sprays: &mut Vec<Cue>,
) {
    let player = threat.pos;
    let (sea, size) = (terrain.sea, terrain.size());
    let bed = |x: f32, z: f32| terrain.bed(x, z);
    // The water's surface: a river's where there is one, else the sea's,
    // with the tide and the swell on it (`Terrain::sea_at`).
    let surf = |x: f32, z: f32| terrain.inland_water(x, z).map_or_else(|| terrain.sea_at(x, z), |w| w.0.max(sea));
    let in_river = |x: f32, z: f32| terrain.inland_water(x, z).is_some_and(|w| w.0 > sea);
    // Beyond the world's edge counts as dry, so nothing swims off it. Under
    // a roof (a sea cave) the height field holds the land overhead, so the
    // depth is down to the floor under the surface there, and rock at the
    // surface is dry.
    let depth = |x: f32, z: f32| {
        if x < 20.0 || z < 20.0 || x > size - 20.0 || z > size - 20.0 {
            0.0
        } else {
            let (s, b) = (surf(x, z), bed(x, z));
            if b > s {
                let p = Vec3::new(x, s - 0.3, z);
                if terrain.in_rock(p) {
                    0.0
                } else {
                    s - terrain.bed_under(p)
                }
            } else {
                s - b
            }
        }
    };
    // The player only startles fish from the water or the water's edge.
    let near = |p: Vec3, wary: f32| Vec3::new(p.x - player.x, 0.0, p.z - player.z).length() < wary && player.y < surf(p.x, p.z) + 2.5;

    // Schools: wander, scatter.
    let mut by_school: crate::util::FxHashMap<hecs::Entity, Vec<(Vec3, Vec3)>> = Default::default();
    for (f, l) in world.query::<(&Fish, &Look)>().iter() {
        by_school.entry(f.school).or_default().push((l.pos, f.vel));
    }
    let empty: Vec<hecs::Entity> = world.query::<(hecs::Entity, &School)>().iter().map(|(e, _)| e).filter(|e| !by_school.contains_key(e)).collect();
    for e in empty {
        let _ = world.despawn(e);
    }
    // The centre of each school, and what it is: for a bait ball's swirl and
    // for the hunters, who rush a school of something they eat.
    let centres: crate::util::FxHashMap<hecs::Entity, Vec3> = by_school.iter().map(|(e, m)| (*e, m.iter().fold(Vec3::ZERO, |a, x| a + x.0) / m.len().max(1) as f32)).collect();
    let kinds: crate::util::FxHashMap<hecs::Entity, usize> = world.query::<(hecs::Entity, &School)>().iter().map(|(e, s)| (e, s.species)).collect();
    // Every fish whose species hunts, where it is: what the prey flees.
    let hunters: Vec<(Vec3, usize)> = world
        .query::<(&Fish, &Look, &Creature)>()
        .iter()
        .filter(|(_, _, c)| !species[c.species].hunts.is_empty())
        .map(|(_, l, c)| (l.pos, c.species))
        .collect();
    let hunted_near = |prey: usize, p: Vec3, r: f32| hunters.iter().filter(|(_, hs)| species[*hs].hunts.contains(&prey)).map(|(q, _)| *q).find(|q| q.distance(p) < r);
    for (se, sc) in world.query_mut::<(hecs::Entity, &mut School)>() {
        let Some(m) = by_school.get(&se) else {
            continue;
        };
        let sp = &species[sc.species];
        let n = m.len() as f32;
        let centre = m.iter().fold(Vec3::ZERO, |a, x| a + x.0) / n;
        sc.timer -= dt;
        sc.flee = (sc.flee - dt).max(0.0);
        sc.phase += dt * 0.25;
        let nerve = super::startle::Nerve::of(sp, None, super::age::Stage::Adult);
        let habit = sc.habit;
        sc.hide = (sc.hide - dt).max(0.0);
        if habit.startles() && m.iter().any(|x| near(x.0, super::startle::reach(&nerve, threat, x.0, 3.0))) && sc.flee <= 0.0 {
            if habit == Habit::Cloud {
                // A cloud drops into its coral rather than leaving it.
                sc.hide = 3.0 + sc.rng.unit() * 2.0;
            } else {
                let away = Vec3::new(centre.x - player.x, 0.0, centre.z - player.z).normalize_or_zero();
                let run = if matches!(habit, Habit::Hover | Habit::Rest) { 9.0 } else { 18.0 };
                sc.home = centre + away * run;
            }
            sc.flee = 3.0;
        }
        // A hunter close by: the same fright as the diver gives.
        if habit.startles() && sc.flee <= 0.0 {
            if let Some(h) = hunted_near(sc.species, centre, 7.0 + n.sqrt()) {
                if habit == Habit::Cloud {
                    sc.hide = 3.0 + sc.rng.unit() * 2.0;
                } else {
                    let away = Vec3::new(centre.x - h.x, 0.0, centre.z - h.z).normalize_or_zero();
                    sc.home = centre + away * 14.0;
                }
                sc.flee = 2.5;
            }
        }
        // A hunter now and then picks a school of what it eats within 35 m
        // and rushes it for a few seconds, then drifts off and rests.
        if !sp.hunts.is_empty() {
            sc.rest_t -= dt;
            if let Some(target) = sc.chase {
                sc.chase_t -= dt;
                match centres.get(&target) {
                    Some(c) if sc.chase_t > 0.0 => sc.home = *c,
                    _ => {
                        sc.chase = None;
                        sc.rest_t = 12.0 + sc.rng.unit() * 18.0;
                    }
                }
            } else if sc.rest_t <= 0.0 {
                let prey = centres
                    .iter()
                    .filter(|(e, _)| kinds.get(*e).is_some_and(|k| sp.hunts.contains(k)))
                    .map(|(e, c)| (*e, c.distance(centre)))
                    .filter(|(_, d)| *d < 35.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match prey {
                    Some((e, _)) => {
                        sc.chase = Some(e);
                        sc.chase_t = 6.0 + sc.rng.unit() * 5.0;
                    }
                    None => sc.rest_t = 4.0 + sc.rng.unit() * 4.0,
                }
            }
        }
        if habit == Habit::Hover {
            // Hang still a few seconds, then cruise a few.
            sc.pause -= dt;
            if sc.pause < -(4.0 + sc.rng.unit() * 6.0) {
                sc.pause = 3.0 + sc.rng.unit() * 5.0;
            }
        }
        if sc.timer <= 0.0 {
            // A new home somewhere near in water deep enough (in a river,
            // its deeper middle). A cloud or a hover keeps near where it was
            // born; the others wander from where they are.
            let river = in_river(centre.x, centre.z);
            let (lo, hi) = if river { (3.0, 15.0) } else { habit.roam() };
            let from = if matches!(habit, Habit::Cloud | Habit::Hover) { sc.anchor } else { centre };
            let want = sp.beh.shallows.unwrap_or(if river { 1.2 } else { 2.0 });
            for _ in 0..8 {
                let a = sc.rng.unit() * TAU;
                let r = lo + sc.rng.unit() * (hi - lo);
                let p = Vec3::new(from.x + a.sin() * r, sea, from.z + a.cos() * r);
                let d = depth(p.x, p.z);
                // A patrol keeps to its depth band, along the wall.
                let fits = sp.beh.depth.is_none_or(|[a, b]| habit != Habit::Patrol || (d >= a && d <= b + 6.0));
                if d >= want && fits {
                    sc.home = p;
                    break;
                }
            }
            sc.timer = match habit {
                Habit::Rest => 40.0 + sc.rng.unit() * 50.0,
                Habit::Patrol | Habit::Cruise => 25.0 + sc.rng.unit() * 20.0,
                _ => 10.0 + sc.rng.unit() * 15.0,
            };
        }
        let r = match habit {
            // Rushing a school: straight at it, no circling.
            _ if sc.chase.is_some() => 0.0,
            Habit::Cloud => 0.6 + n.sqrt() * 0.2,
            Habit::Hover => 1.2,
            Habit::Rest | Habit::Burrow | Habit::Pulse => 0.0,
            Habit::Bait => 2.0 + n.sqrt() * 0.25,
            _ => 3.0 + n.sqrt(),
        };
        sc.goal = sc.home + Vec3::new(sc.phase.cos() * r, 0.0, (sc.phase * 1.3).sin() * r * 0.7);
    }
    // A chasing school counts as fleeing for its speed: a rush is as fast
    // as a bolt.
    let schools: std::collections::HashMap<hecs::Entity, (Vec3, bool, Habit, bool, bool)> = world
        .query::<(hecs::Entity, &School)>()
        .iter()
        .map(|(e, s)| (e, (s.goal, s.flee > 0.0 || s.chase.is_some(), s.habit, s.pause > 0.0, s.hide > 0.0)))
        .collect();
    let chasing: crate::util::FxHashSet<hecs::Entity> = world.query::<(hecs::Entity, &School)>().iter().filter(|(_, s)| s.chase.is_some()).map(|(e, _)| e).collect();
    stats.chases += chasing.len() as u32;
    // Prey within reach of a rushing hunter's jaws, to be caught.
    let prey_fish: Vec<(hecs::Entity, Vec3, usize)> = world
        .query::<(hecs::Entity, &Fish, &Look, &Creature)>()
        .iter()
        .filter(|(_, f, _, _)| !chasing.contains(&f.school))
        .map(|(e, _, l, c)| (e, l.pos, c.species))
        .collect();
    let mut eaten: Vec<hecs::Entity> = Vec::new();

    // Swimmers that have a land form which hauls out (seals) climb out now and
    // then. `water` points both ways, so on a water form it is its land form.
    let land_form: Vec<Option<usize>> = species.iter().map(|s| s.water.filter(|_| s.is_water_form).filter(|&p| species[p].verb().dives())).collect();
    let mut hauled: Vec<(hecs::Entity, Vec3, f32, usize)> = Vec::new();
    // Fish.
    for (fe, f, l, me, anim, morph) in world.query_mut::<(hecs::Entity, &mut Fish, &mut Look, &Creature, Option<&mut crate::anim::Anim>, Option<&crate::variety::Morph>)>() {
        l.begin_step();
        stats.fish += 1;
        let sp = &species[me.species];
        let big = big_swimmer(sp, &models[l.model]);
        let g = &groups[me.species];
        let (goal, fleeing, habit, paused, hiding) = schools.get(&f.school).copied().unwrap_or((l.pos, false, Habit::Open, false, false));
        let (pace, _) = crate::variety::pace(morph);
        let p0 = l.pos;
        if habit == Habit::Burrow {
            // A garden eel: risen out of its hole in the sand, swaying in
            // the current, sinking back when anyone comes near.
            let b = bed(f.anchor.x, f.anchor.z);
            let close = Vec3::new(f.anchor.x - player.x, 0.0, f.anchor.z - player.z).length() < 3.5 && player.y < surf(p0.x, p0.z) + 1.0;
            f.ext = if close { (f.ext - dt * 2.0).max(0.0) } else { (f.ext + dt * 0.2).min(1.0) };
            let len = models[l.model].height;
            l.pos = Vec3::new(f.anchor.x, b - (1.0 - f.ext) * len * 0.95, f.anchor.z);
            f.wag += dt;
            l.yaw = (f.wag * 0.4 + f.lane * 6.0).sin() * 0.6 + f.lane * TAU;
            continue;
        }
        if habit == Habit::Pulse {
            // A jellyfish: drifting, a slow beat lifting it a hand at a
            // time, sinking a little between.
            f.wag += dt;
            let beat = (f.wag * 1.3 + f.lane * TAU).sin();
            let top_here = surf(p0.x, p0.z);
            let want = band_y(sp, habit, bed(p0.x, p0.z), top_here, f.lane);
            let drift = Vec3::new((f.wag * 0.07 + f.lane * 9.0).cos(), 0.0, (f.wag * 0.05 + f.lane * 5.0).sin()) * 0.12;
            f.vel = drift + Vec3::Y * ((want - p0.y) * 0.2 + beat.max(0.0) * 0.12 - 0.03);
            l.pos += f.vel * dt;
            let b = bed(l.pos.x, l.pos.z);
            l.pos.y = l.pos.y.clamp(b + 0.3, (surf(l.pos.x, l.pos.z) - 0.25).max(b + 0.3));
            l.yaw += dt * 0.05;
            continue;
        }
        let cruise = sp.beh.walk_speed.max(0.4)
            * pace
            * if fleeing || hiding {
                2.5
            } else if paused {
                0.12
            } else {
                1.0
            };
        let top = sp.beh.flee_speed.max(cruise) * pace;
        let p = l.pos;
        let top_here = surf(p.x, p.z);
        let river = in_river(p.x, p.z);
        if f.air {
            f.vel.y -= 9.8 * dt;
            l.pos += f.vel * dt;
            if l.pos.y < top_here && f.vel.y < 0.0 {
                f.air = false;
                f.vel.y *= 0.3;
                splash(sprays, "fish_splash", Vec3::new(l.pos.x, top_here, l.pos.z), 0.15, 1.0);
                splash(sprays, "splash_mist", Vec3::new(l.pos.x, top_here, l.pos.z), 0.15, 0.4);
            }
        } else {
            let members = by_school.get(&f.school).map_or(&[][..], |v| v.as_slice());
            let mut acc = super::flock::steer(p, f.vel, members.iter().copied(), g);
            let to_goal = Vec3::new(goal.x - p.x, 0.0, goal.z - p.z);
            // A resting ray settles once it is over its spot.
            let settle = if habit == Habit::Rest { (to_goal.length() / 2.0).min(1.0) } else { 1.0 };
            acc += (to_goal.normalize_or_zero() * cruise * settle - Vec3::new(f.vel.x, 0.0, f.vel.z)) * if fleeing { 2.0 } else { 0.6 };
            let d = depth(p.x, p.z);
            let b_here = terrain.bed_under(p);
            let mut want_y = band_y(sp, habit, b_here, top_here, f.lane);
            if habit == Habit::Rest && settle > 0.3 {
                // Gliding to a new spot: lifted a little off the sand.
                want_y = b_here + 0.6;
            }
            if hiding {
                // Down into the coral.
                want_y = b_here + 0.2;
            }
            // Darting away from a hunter close by, one fish at a time: a bait
            // ball splits round the jack driving through it.
            if let Some(h) = hunted_near(me.species, p, 4.0) {
                let away = p - h;
                acc += away.normalize_or_zero() * (4.0 - away.length()) * 7.0;
                if habit == Habit::Cloud {
                    want_y = b_here + 0.2;
                }
            }
            // A rushing hunter takes a fish that comes within a hand of it.
            if chasing.contains(&f.school) && eaten.is_empty() {
                if let Some((e, _, _)) = prey_fish.iter().find(|(e, q, k)| *e != fe && sp.hunts.contains(k) && q.distance(p) < 0.45 + models[l.model].half_w) {
                    eaten.push(*e);
                }
            }
            if habit == Habit::Bait {
                // The ball turns round its own middle, and opens round the
                // diver: anything within a few metres is pushed out of the way.
                if let Some(c) = centres.get(&f.school) {
                    let r = Vec3::new(p.x - c.x, 0.0, p.z - c.z);
                    acc += Vec3::Y.cross(r).normalize_or_zero() * 1.6;
                }
                let me = p - player;
                if me.length() < 3.5 && player.y < top_here {
                    acc += me.normalize_or_zero() * (3.5 - me.length()) * 6.0;
                }
            }
            acc.y += (want_y - p.y) * 1.5 - f.vel.y * 1.2;
            // Keep out of the shallows: turn toward deeper water.
            let ahead = p + Vec3::new(f.vel.x, 0.0, f.vel.z) * 0.8;
            let shoal = sp.beh.shallows.map_or(if river { 0.7 } else { 1.2 }, |s| s * 0.75);
            if depth(ahead.x, ahead.z) < shoal || d < shoal - 0.2 {
                let gx = depth(p.x + 1.0, p.z) - depth(p.x - 1.0, p.z);
                let gz = depth(p.x, p.z + 1.0) - depth(p.x, p.z - 1.0);
                let deeper = Vec3::new(gx, 0.0, gz).normalize_or_zero();
                let deeper = if deeper == Vec3::ZERO { -Vec3::new(f.vel.x, 0.0, f.vel.z).normalize_or_zero() } else { deeper };
                acc += deeper * 10.0;
            }
            if d < shoal - 0.2 {
                stats.shallow += 1;
            }
            // A seal near the shore, about once a minute, climbs out onto a
            // low bank or beach close by.
            if let Some(parent) = land_form[me.species] {
                if f.rng.unit() < dt / 60.0 && !near(p, 12.0) {
                    for k in 0..12 {
                        let a = k as f32 * TAU / 12.0;
                        let q = p + Vec3::new(a.sin(), 0.0, a.cos()) * 4.0;
                        if let Some((gy, m)) = terrain.ground(q.x, q.z, sea + 4.0) {
                            if gy > sea + 0.3 && gy < sea + 2.5 && !crate::mesh::sprites::leafy(m) && !super::terrain::water_voxel(m) {
                                hauled.push((fe, Vec3::new(q.x, gy + 0.01, q.z), a + PI, parent));
                                break;
                            }
                        }
                    }
                }
            }
            let before = Vec3::new(f.vel.x, 0.0, f.vel.z);
            f.vel += acc * dt;
            if big {
                let after = Vec3::new(f.vel.x, 0.0, f.vel.z);
                if before.length() > 0.05 && after.length() > 0.05 {
                    let a0 = before.x.atan2(before.z);
                    let d = crate::util::wrap_angle(after.x.atan2(after.z) - a0);
                    let most = BIG_TURN * dt * if fleeing { 1.6 } else { 1.0 };
                    if d.abs() > most {
                        let a = a0 + d.clamp(-most, most);
                        let s = after.length();
                        f.vel.x = a.sin() * s;
                        f.vel.z = a.cos() * s;
                    }
                }
            }
            let flat = Vec3::new(f.vel.x, 0.0, f.vel.z).length();
            if flat > top {
                f.vel.x *= top / flat;
                f.vel.z *= top / flat;
            } else if flat < cruise * 0.4 && flat > 1e-4 {
                f.vel.x *= cruise * 0.4 / flat;
                f.vel.z *= cruise * 0.4 / flat;
            }
            f.vel.y = f.vel.y.clamp(-1.0, 1.0);
            // Anything swimming in one of the sea's streams rides it.
            let stream = crate::player::dive::sea_current(p, top_here, |x, z| depth(x, z) >= crate::player::dive::CURRENT_DEPTH);
            l.pos += (f.vel + stream * 0.85) * dt;
            let b = terrain.bed_under(l.pos);
            // (max: a fish over a bar that is barely awash stays on it.)
            let top = surf(l.pos.x, l.pos.z);
            // The model stands on its pivot, its belly. Anything as big as a
            // seal keeps most of itself under, back awash at most: kept only
            // under the surface by its belly, a walrus rode on top of the
            // water like a log.
            let under = models[l.model].height * if big { 0.75 } else { 0.0 };
            l.pos.y = l.pos.y.clamp(b + 0.1, (top - 0.12 - under).max(b + 0.1));
            // The bed is the floor under the fish (`bed_under`), but the
            // rock beside it is not in that: a step into a solid voxel is
            // refused and the fish turns back the way it came. (One already
            // in rock, set down there by the spawner, may swim out.)
            if terrain.in_rock(l.pos) && !terrain.in_rock(p) {
                l.pos = p;
                if big {
                    // A big body swings aside toward the deeper side and
                    // slows, rather than flicking straight back.
                    let side = Vec3::new(-f.vel.z, 0.0, f.vel.x).normalize_or_zero();
                    let k = if depth(p.x + side.x * 2.0, p.z + side.z * 2.0) >= depth(p.x - side.x * 2.0, p.z - side.z * 2.0) { 1.0 } else { -1.0 };
                    let s = Vec3::new(f.vel.x, 0.0, f.vel.z).length() * 0.6;
                    f.vel.x = side.x * k * s;
                    f.vel.z = side.z * k * s;
                } else {
                    f.vel.x = -f.vel.x;
                    f.vel.z = -f.vel.z;
                }
            }
            // A leap now and then, more often when scattering.
            let rate = sp.beh.leap.max(0.0) / 60.0 * if fleeing { 4.0 } else { 1.0 };
            if l.pos.y > top - 2.6 && d > shoal && f.rng.unit() < rate * dt {
                let dir = Vec3::new(f.vel.x, 0.0, f.vel.z).normalize_or_zero();
                f.vel = dir * (1.5 + f.rng.unit() * 1.5) + Vec3::Y * (2.8 + f.rng.unit() * 1.8);
                l.pos.y = top;
                f.air = true;
                stats.leaps += 1;
                splash(sprays, "fish_splash", l.pos, 0.1, 0.7);
                splash(sprays, "splash_mist", l.pos, 0.1, 0.3);
            }
        }
        let flat = Vec3::new(f.vel.x, 0.0, f.vel.z);
        let to_me = Vec3::new(player.x - l.pos.x, 0.0, player.z - l.pos.z);
        if habit == Habit::Hover && paused && to_me.length() < 7.0 && player.y < surf(l.pos.x, l.pos.z) {
            // Hanging still with a diver about: turn to look at them.
            let want = to_me.x.atan2(to_me.z) + PI;
            l.yaw = crate::util::turn_toward(l.yaw, want, (dt * 2.0).min(1.0));
        } else if flat.length() > 0.05 {
            let want = flat.x.atan2(flat.z) + PI;
            let rate = if big { 4.0 } else { 8.0 };
            l.yaw = crate::util::turn_toward(l.yaw, want, (dt * rate).min(1.0));
        }
        if let (Some(a), Some(rig)) = (anim, models[l.model].rig.as_ref()) {
            let speed = f.vel.length();
            f.wag += dt * (5.0 + speed * 6.0);
            a.prev = a.pose.clone();
            // A fish hanging nearly still keeps level: a slow drift up or
            // down must not stand it on its nose. A big body (a seal, a
            // walrus, a shark) tips only a little: it pivots on its belly, so
            // a fish's tilt stood a walrus up on its tail.
            let lim = if big { 0.3 } else { 1.0 };
            let pitch = f.vel.y.atan2(flat.length().max(0.6)).clamp(-lim, lim);
            a.pose.bones[0] = Vec3::new(pitch - sp.beh.swim_pitch, 0.0, 0.0);
            if sp.beh.back_float {
                // An otter drifts and dozes on its back, and rolls over onto
                // its belly to swim off when something comes too close.
                let want = if fleeing || f.air { 0.0 } else { 1.0 };
                f.back += (want - f.back).clamp(-dt * 1.5, dt * 0.5);
                // Its pose is made afresh each step: the swim below, then
                // the back float blended over it by `f.back`.
                a.pose.bones.iter_mut().skip(1).for_each(|v| *v = Vec3::ZERO);
            }
            if rig.plan == Some(Plan::Ray) {
                // Mantas: a slow ripple out along the wings, quicker when fast.
                let mut p = crate::anim::Pose::rest(rig.bones.len());
                crate::anim::motors::undulate(rig, &mut p, f.wag / std::f32::consts::TAU * 0.2, 0.32, 1.0);
                p.bones[0].x += pitch;
                a.pose = p;
            } else {
                for (i, b) in rig.bones.iter().enumerate() {
                    if b.role == Role::Tail {
                        let amp = if f.air { 0.15 } else { 0.35 + speed * 0.1 };
                        a.pose.bones[i] = Vec3::new(0.0, f.wag.sin() * amp.min(0.6), 0.0);
                    } else if b.role == Role::Leg && b.side != 0.0 && crate::anim::motors::upper_leg(rig, i) {
                        // Flippers: the front pair beat slowly up and down
                        // and sweep back (a turtle's flight through water);
                        // the hind pair trail and steer.
                        let ph = f.wag * 0.3;
                        a.pose.bones[i] = if b.pair == 0 {
                            Vec3::new(0.0, -b.side * 0.35 * (ph + 1.2).sin(), -b.side * 0.6 * ph.sin())
                        } else {
                            Vec3::new(0.0, -b.side * 0.5, -b.side * 0.15 * (ph * 0.5).sin())
                        };
                    }
                }
            }
            if sp.beh.back_float {
                back_pose(rig, &mut a.pose, f.back, f.wag);
            }
        }
    }

    for e in eaten {
        if world.despawn(e).is_ok() {
            stats.caught += 1;
        }
    }
    for (e, at, yaw, s) in hauled {
        let _ = world.despawn(e);
        let m = &models[species[s].model];
        let seed = e.id().wrapping_mul(0x9e37_79b9) | 1;
        world.spawn((
            super::walk::Critter::new(yaw, 3.0, seed, 1.0),
            Creature::of(s),
            crate::physics::Body::new(m.half_w, m.height),
            Look::new(species[s].model, at, yaw),
        ));
    }

    // Whales.
    for (w, l, me, anim, morph) in world.query_mut::<(&mut Whale, &mut Look, &Creature, Option<&mut crate::anim::Anim>, Option<&crate::variety::Morph>)>() {
        l.begin_step();
        stats.whales += 1;
        let sp = &species[me.species];
        let m = &models[l.model];
        let p = l.pos;
        let d = depth(p.x, p.z);
        let cruise = sp.beh.walk_speed.max(0.5) * crate::variety::pace(morph).0;
        let fwd = Vec3::new(w.heading.sin(), 0.0, w.heading.cos());
        // A bigger individual sits deeper and its blowhole is higher.
        let scale = morph.map_or(1.0, |x| x.scale);
        let length = m.voxel * scale * 28.0;
        let height = m.height * scale;
        // Steer: toward a goal offshore, away from shallows ahead.
        if w.phase != Phase::Air {
            w.timer -= dt;
            if (w.goal - p).with_y(0.0).length() < 15.0 || depth(w.goal.x, w.goal.z) < 10.0 {
                for _ in 0..8 {
                    let a = w.rng.unit() * TAU;
                    let r = 40.0 + w.rng.unit() * 60.0;
                    let q = Vec3::new(p.x + a.sin() * r, sea, p.z + a.cos() * r);
                    if depth(q.x, q.z) >= 12.0 {
                        w.goal = q;
                        break;
                    }
                }
            }
            let ahead = p + fwd * (length * 1.5);
            let turn;
            if depth(ahead.x, ahead.z) < 8.0 {
                // Turn toward whichever side is deeper.
                let l_ = p + Vec3::new((w.heading + 0.8).sin(), 0.0, (w.heading + 0.8).cos()) * length * 1.5;
                let r_ = p + Vec3::new((w.heading - 0.8).sin(), 0.0, (w.heading - 0.8).cos()) * length * 1.5;
                turn = if depth(l_.x, l_.z) > depth(r_.x, r_.z) { 0.35 } else { -0.35 };
            } else {
                let to = w.goal - p;
                let want = to.x.atan2(to.z);
                turn = crate::util::wrap_angle(want - w.heading).clamp(-0.08, 0.08);
            }
            w.heading += turn * dt;
        }
        // The sea's surface over the whale, with the swell on it.
        let sea = terrain.sea_at(p.x, p.z);
        // The water line for the back awash, from the model's height.
        let awash = sea - height * 0.75;
        // Pivot height for swimming `below` metres under the surface, kept a
        // metre off the bed.
        let bed_here = sea - d;
        let under = |below: f32| (sea - below - height).max(bed_here + 1.0);
        match w.phase {
            Phase::Dive => {
                w.speed += (cruise - w.speed) * dt * 0.5;
                w.vy += ((under(5.0) - p.y) * 0.3 - w.vy) * dt;
                if w.timer <= 0.0 {
                    if w.rng.unit() < sp.beh.leap.clamp(0.0, 1.0) && d > 12.0 {
                        w.phase = Phase::Sink;
                        w.timer = 8.0;
                    } else {
                        w.phase = Phase::Surface;
                        w.timer = 22.0 + w.rng.unit() * 18.0;
                        w.spout = 2.5;
                    }
                }
            }
            Phase::Surface => {
                w.speed += (cruise * 0.7 - w.speed) * dt * 0.5;
                w.vy += ((awash - p.y) * 0.8 - w.vy * 1.5) * dt;
                w.spout -= dt;
                if w.spout <= 0.0 && p.y > awash - 0.6 {
                    let hole = p + fwd * (length * 0.2) + Vec3::Y * height * 0.9;
                    // The blow: a bushy column of mist out of the blowhole.
                    splash(sprays, "whale_blow", Vec3::new(hole.x, sea + 0.1, hole.z), 0.0, 1.0);
                    stats.spouts += 1;
                    w.spout = 6.0 + w.rng.unit() * 5.0;
                }
                if w.timer <= 0.0 {
                    w.phase = Phase::Dive;
                    w.timer = 25.0 + w.rng.unit() * 25.0;
                }
            }
            Phase::Sink => {
                w.speed += (cruise * 0.6 - w.speed) * dt;
                w.vy += ((under(9.0) - p.y) * 0.5 - w.vy) * dt;
                if p.y < sea - 7.0 || w.timer <= 0.0 {
                    w.phase = Phase::Rise;
                }
            }
            Phase::Rise => {
                w.speed += (4.5 - w.speed) * dt * 2.0;
                w.vy = (w.vy + 9.0 * dt).min(8.5);
                if p.y + height * 0.5 > sea {
                    w.phase = Phase::Air;
                    stats.breaches += 1;
                    splash(sprays, "big_splash", Vec3::new(p.x, sea, p.z), 2.0, 1.0);
                    splash(sprays, "big_splash_mist", Vec3::new(p.x, sea, p.z), 2.0, 1.0);
                }
            }
            Phase::Air => {
                w.vy -= 9.8 * dt;
                // Twist onto its side on the way up.
                w.roll += (1.4 - w.roll) * dt * 1.8;
                if w.vy < 0.0 && p.y + height * 0.5 < sea {
                    w.phase = Phase::Surface;
                    w.timer = 10.0 + w.rng.unit() * 8.0;
                    w.spout = 4.0;
                    let at = p + fwd * (length * 0.1);
                    splash(sprays, "big_splash", Vec3::new(at.x, sea, at.z), length * 0.35, 1.8);
                    splash(sprays, "big_splash_mist", Vec3::new(at.x, sea, at.z), length * 0.35, 1.8);
                }
            }
        }
        if w.phase != Phase::Air {
            w.roll += (0.0 - w.roll) * dt * 0.8;
            w.vy = w.vy.clamp(-1.5, if w.phase == Phase::Rise { 8.5 } else { 1.0 });
        }
        l.pos += fwd * w.speed * dt + Vec3::Y * w.vy * dt;
        if w.phase != Phase::Air {
            let b = bed(l.pos.x, l.pos.z);
            l.pos.y = l.pos.y.max(b + 0.3).min(if w.phase == Phase::Rise { sea + 2.0 } else { awash });
        }
        l.yaw = w.heading + PI;
        let pitch_want = w.vy.atan2(w.speed.max(0.3)).clamp(-0.6, 1.25);
        let rate = if w.phase == Phase::Air { 1.4 } else { 2.5 };
        w.pitch += (pitch_want - w.pitch) * (dt * rate).min(1.0);
        if let (Some(a), Some(rig)) = (anim, m.rig.as_ref()) {
            w.wag += dt * (0.8 + w.speed * 0.35);
            a.prev = a.pose.clone();
            a.pose.bones[0] = Vec3::new(w.pitch, 0.0, w.roll);
            for (i, b) in rig.bones.iter().enumerate() {
                match b.role {
                    Role::Tail => a.pose.bones[i] = Vec3::new(w.wag.sin() * if w.phase == Phase::Air { 0.1 } else { 0.3 }, 0.0, 0.0),
                    Role::Arm => {
                        let flap = if w.phase == Phase::Air { 0.5 } else { 0.12 * (w.wag * 0.5).sin() };
                        a.pose.bones[i] = Vec3::new(0.0, 0.0, flap * b.side);
                    }
                    Role::Root | Role::Spine | Role::Neck | Role::Head | Role::Jaw | Role::Leg | Role::Wing | Role::Ear => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reef_fish_keep_to_their_band_over_the_bed() {
        let beh: crate::library::Behaviour = serde_json::from_value(serde_json::json!({ "movement": "school", "above_bed": [0.5, 2.0] })).unwrap();
        let mut sp = Species {
            name: "reef_fish".into(),
            model: 0,
            beh,
            flees: vec![],
            hunts: vec![],
            flees_player: true,
            like: None,
            quota: 0,
            water: None,
            is_water_form: false,
            vary: Default::default(),
            coats: vec![],
            morphs: vec![],
            ages: Default::default(),
            aged_tint: [1.0; 3],
        };
        // Over a bed at 10 m under a surface at 24: between 10.5 and 12.
        for lane in [0.0, 0.5, 1.0] {
            let y = band_y(&sp, Habit::Cloud, 10.0, 24.0, lane);
            assert!((10.5..=12.0).contains(&y), "lane {lane}: {y}");
        }
        // Over a flat a metre under: never out of the water.
        assert!(band_y(&sp, Habit::Cloud, 23.0, 24.0, 1.0) <= 23.7);
        // A ray rests on the sand.
        assert!((band_y(&sp, Habit::Rest, 10.0, 24.0, 0.5) - 10.12).abs() < 1e-4);
        sp.beh.above_bed = None;
        sp.beh.depth = Some([2.0, 6.0]);
        let y = band_y(&sp, Habit::Patrol, 0.0, 24.0, 0.5);
        assert!((y - 20.0).abs() < 1e-4, "{y}");
        assert_eq!(HABITS.len(), 9);
    }

    #[test]
    fn fish_swim_shallow_in_shallow_water_and_never_near_the_bed() {
        assert!(fish_depth(1.0) <= 0.5);
        assert!(fish_depth(3.0) >= 0.45 && fish_depth(3.0) <= 1.5);
        assert!(fish_depth(40.0) <= 2.5);
    }

    #[test]
    fn a_seal_diving_in_asks_for_a_splash() {
        let mut cues = Vec::new();
        splash_at(&mut cues, Vec3::new(5.0, 24.0, 5.0), 0.7);
        let names: Vec<&str> = cues.iter().map(|c| c.name).collect();
        assert_eq!(names, ["splash", "splash_mist"]);
        assert!(cues.iter().all(|c| c.at.y == 24.0 && c.radius > 0.0));
    }
}
