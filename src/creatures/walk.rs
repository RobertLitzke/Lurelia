//! Walkers: animals on the ground with a physics body (the `Walk` mover:
//! graze, hop, skulk, wade, still). They wander, notice the player and
//! flee, run from predators and chase prey, hop over small ledges, and steer
//! round trunks, walls, drops and water. Rigged ones choose their own
//! activities (anim/life.rs).

use super::affordance::Affordance;
use super::terrain::{voxel_ground, water_voxel, Terrain};
use super::{Creature, Look, Pathing, Species};
use crate::anim::life::Act;
use crate::anim::Role;
use crate::models::Model;
use crate::physics::{drawn_ground_lift, move_body, Body};
use crate::world::noise::Rng;
use glam::Vec3;

/// How far a plant-eater will walk to reach leaves it can browse, metres.
/// About as far as one can see through a wood, so an animal in a meadow is
/// not pulled across it by a tree on the far side.
const BROWSE_M: f32 = 22.0;
/// How near a browse plant counts as being at it.
const AT_PLANT_M: f32 = 1.4;
/// How often a plant-eater setting off makes for browse rather than
/// wandering. Not every time: a meadow should not drain into its hedges.
const BROWSE_CHANCE: f32 = 0.55;

/// How near the water's edge a drinker stops, metres.
const DRINK_AT: f32 = 2.5;
/// How much of what an animal plans to do becomes a rest instead when it is
/// at its drowsiest (creatures/diel.rs). Not all of it: something is always
/// stirring. Drowsiness is squared on the way in, so a deer half asleep at
/// midday beds down now and then rather than half the time.
const BEDS_DOWN: f32 = 0.85;

#[derive(Clone, Copy, PartialEq)]
pub enum Mood {
    Idle,
    Walk,
    Flee,
    Chase,
    /// Carrying a kill off, at a walk (creatures/carry.rs).
    Feed,
}

pub struct Critter {
    pub mood: Mood,
    pub heading: f32,
    pub timer: f32,
    pub rng: Rng,
    /// Which way it last stepped round an obstacle (+1 left, -1 right), so it
    /// keeps following a wall instead of dithering at it.
    pub side: f32,
    /// Seconds spent pushing without getting anywhere.
    pub stuck: f32,
    /// A walker that can fly (a heron), in the air on a short flight.
    pub air: Option<Air>,
    /// A basking sea turtle on its way back to the sea, to swim off.
    pub seaward: bool,
    /// Seconds before this one will hunt again: counting down while it
    /// carries a kill, then a long wait with a full belly.
    pub fed: f32,
    /// Alarm at the player, built up at `startle::pressure` per second and
    /// spent when it bolts (creatures/startle.rs).
    pub alarm: f32,
    /// Metres walked since its last footfall was reported (creatures/sign.rs),
    /// and which foot that was.
    pub walked: f32,
    pub left: bool,
    /// A `back_float` swimmer idling afloat: how far over on its back it
    /// lies, 0 to 1.
    pub back: f32,
    /// Whether it stood on the generator's ground, past full detail, when
    /// it was last stepped (terrain.rs `far_ground`).
    pub far: bool,
}

/// A short flight from one spot to another: up, across, down.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    pub from: Vec3,
    pub to: Vec3,
    pub t: f32,
    pub dur: f32,
    pub height: f32,
}

impl Critter {
    pub fn new(heading: f32, timer: f32, rng: u32, side: f32) -> Self {
        Critter {
            mood: Mood::Idle,
            heading,
            timer,
            rng: Rng::walker(rng),
            side,
            stuck: 0.0,
            air: None,
            seaward: false,
            fed: 0.0,
            alarm: 0.0,
            walked: 0.0,
            left: false,
            back: 0.0,
            far: false,
        }
    }
}

/// How fast a walker small enough not to have turning inertia swings round
/// to its heading: a share of the way each second, which reads as instant.
const FREE_TURN: f32 = 8.0;
/// The height at which turning starts to take time, metres.
const TURN_FROM_M: f32 = 1.5;
/// The slowest a big walker goes while turning hard: it creeps forward
/// round the arc instead of standing to spin.
const TURN_CRAWL: f32 = 0.2;

/// How fast a walker `height` metres tall can turn, radians a second, or
/// `FREE_TURN` for one that turns at once. Falls off a little faster than
/// height: a 3 m elephant or triceratops turns about 1.6 rad/s at a walk, a
/// 6 m brachiosaur 0.6. Bolting, it throws itself round half again as hard.
fn turn_rate(height: f32, mood: Mood) -> f32 {
    if height <= TURN_FROM_M {
        return FREE_TURN;
    }
    let base = 4.5 * (TURN_FROM_M / height).powf(1.5);
    let urge = match mood {
        Mood::Flee | Mood::Chase => 1.6,
        _ => 1.0,
    };
    (base * urge).clamp(0.4, FREE_TURN - 0.01)
}

/// Footfalls are only reported this close to the player: further out nobody
/// will find them before they wear away, and the walkers out there are few.
const MARKS_M: f32 = 90.0;

/// How far off a hunter notices what it eats: 14 m for a fox or a raptor,
/// further for a big animal whose head is high off the ground (a
/// tyrannosaur sees a hadrosaur 30 m off).
fn hunt_sight(height: f32) -> f32 {
    (height * 8.0).max(14.0)
}

/// Can a body this size walk `dist` metres along `dir` from feet at `pos`:
/// nothing taller than a step in the way (trunks, walls, cliffs, low
/// branches at head height), no shrub crown to climb onto, no drop deeper
/// than 1.6 m, no river deeper than `max_depth`?
#[allow(clippy::too_many_arguments)]
fn walkable(terrain: &Terrain, pos: Vec3, dir: Vec3, dist: f32, half_w: f32, height: f32, sea: f32, max_depth: f32) -> bool {
    let store = terrain.store;
    let mut y = pos.y;
    let n = (dist / 0.4).ceil().max(1.0) as i32;
    for k in 1..=n {
        let p = pos + dir * (dist * k as f32 / n as f32);
        if crate::physics::hits(store, Vec3::new(p.x, y + 0.5, p.z), half_w, height) {
            return false;
        }
        let Some((gy, m)) = voxel_ground(store, p.x, p.z, y + 0.5) else {
            return false;
        };
        // Walk round shrub and tree crowns, not over them.
        if gy < sea + 0.2 || gy < y - 1.6 || crate::mesh::sprites::leafy(m) || water_voxel(m) {
            return false;
        }
        if terrain.inland_water(p.x, p.z).is_some_and(|w| w.1 > max_depth) {
            return false;
        }
        y = gy;
    }
    true
}

/// `walkable` for a large animal out past full detail, on the generator's
/// ground (terrain.rs `far_ground`): no climb or drop of more than 1.6 m a
/// sample, nothing under the water line or in a river deeper than
/// `max_depth`, and no tree trunk within `half_w` of the line. Voxels that
/// are loaded along the way are read as voxels.
#[allow(clippy::too_many_arguments)]
fn far_walkable(terrain: &Terrain, plants: &super::plants::Flora, pos: Vec3, dir: Vec3, dist: f32, half_w: f32, sea: f32, max_depth: f32) -> bool {
    let store = terrain.store;
    let mut y = pos.y;
    let n = (dist / 1.0).ceil().max(1.0) as i32;
    for k in 1..=n {
        let p = pos + dir * (dist * k as f32 / n as f32);
        if p.x < 8.0 || p.z < 8.0 || p.x > terrain.size() - 8.0 || p.z > terrain.size() - 8.0 {
            return false;
        }
        let gy = match super::terrain::far_ground(store, p.x, p.z) {
            Some(g) => g,
            None => match voxel_ground(store, p.x, p.z, y + 2.0) {
                Some((g, m)) if !crate::mesh::sprites::leafy(m) && !water_voxel(m) => g,
                _ => return false,
            },
        };
        if gy < sea + 0.2 || (gy - y).abs() > 1.6 {
            return false;
        }
        if terrain.inland_water(p.x, p.z).is_some_and(|w| w.1 > max_depth) {
            return false;
        }
        if plants.near(p.x, p.z, half_w + 1.5, None).any(|t| t.trunk > super::trample::TRUNK_M && Vec3::new(t.pos.x - p.x, 0.0, t.pos.z - p.z).length() < half_w + t.trunk + 0.3) {
            return false;
        }
        y = gy;
    }
    true
}

/// What a walker needs to be given its activity scheduler.
struct Waiting {
    e: hecs::Entity,
    model: usize,
    seed: u32,
    species: Option<usize>,
    habits: crate::anim::life::Habits,
    supple: f32,
    side: f32,
}

/// Give each rigged walker without one its activity scheduler.
pub fn attach_life(world: &mut hecs::World, species: &[Species], models: &[Model]) {
    let unanimated: Vec<Waiting> = world
        .query::<(hecs::Entity, &Critter, &Look, Option<&Creature>, Option<&crate::anim::Anim>, Option<&crate::variety::Morph>)>()
        .iter()
        .filter(|(_, _, l, _, a, _)| a.is_none() && models[l.model].rig.is_some())
        .map(|(e, c, l, cr, _, m)| Waiting {
            e,
            model: l.model,
            seed: c.rng.state,
            species: cr.map(|c| c.species),
            habits: m.map_or([1.0; crate::anim::life::ACTS.len()], |m| m.habits),
            supple: m.map_or(1.0, |m| m.supple),
            side: c.side,
        })
        .collect();
    for Waiting {
        e,
        model,
        seed,
        species: s,
        habits,
        supple,
        side,
    } in unanimated
    {
        let rig = models[model].rig.as_ref().unwrap();
        let routine = s.map_or(super::locomotion::VERBS[0].routine, |s| species[s].verb().routine);
        let mut anim = crate::anim::Anim::alive(rig, rig.table(routine), seed);
        if let Some(l) = anim.life.as_mut() {
            l.habits = habits;
            l.supple = supple;
            l.on_side = s.is_some_and(|s| species[s].beh.sleeps_on_side);
            l.side = side;
        }
        let _ = world.insert_one(e, anim);
    }
}

/// Step every walker.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn update(
    world: &mut hecs::World,
    species: &[Species],
    plants: &super::plants::Flora,
    terrain: &Terrain,
    dt: f32,
    threat: &super::startle::Threat,
    slopes: bool,
    models: &[Model],
    hour: f32,
    sky: &crate::world::weather::Sky,
    now: f32,
    stats: &mut Pathing,
    cues: &mut Vec<crate::fx::Cue>,
    steps: &mut Vec<super::sign::Step>,
    crush: &mut Vec<Vec3>,
) {
    let player = threat.pos;
    let thirst = thirst(hour);
    let store = terrain.store;
    let sea = terrain.sea;
    // Where everyone is and how fast, for fleeing and hunting.
    let mut snap: Vec<(Option<usize>, Vec3, Vec3, hecs::Entity)> = world
        .query::<(hecs::Entity, &Critter, &Look, &Body, Option<&Creature>)>()
        .iter()
        .map(|(e, _, l, b, c)| (c.map(|c| c.species), l.pos, b.vel, e))
        .collect();
    // Running birds are hunters too (a roadrunner after a lizard), though
    // they have no body of their own.
    snap.extend(
        world
            .query::<(hecs::Entity, &super::runner::Runner, &Look, &Creature)>()
            .iter()
            .map(|(e, _, l, c)| (Some(c.species), l.pos, (l.pos - l.prev_pos) / dt.max(1e-3), e)),
    );
    // Who is bolting this step: the alarm spreads from them to anything near
    // enough to hear the crash of it (creatures/startle.rs `Threat::alarm_of`).
    let bolting: Vec<(Vec3, Vec3, hecs::Entity)> = world
        .query::<(hecs::Entity, &Critter, &Look, &Body)>()
        .iter()
        .filter(|(_, c, _, _)| c.mood == Mood::Flee && c.air.is_none())
        .map(|(e, _, l, b)| (l.pos, b.vel, e))
        .collect();
    let mut dived: Vec<(hecs::Entity, Vec3, usize)> = Vec::new();
    // Prey caught this step: (hunter, caught).
    let mut kills: Vec<(hecs::Entity, hecs::Entity)> = Vec::new();
    for (ent, crit, body, look, mut anim, morph, me, age) in world.query_mut::<(
        hecs::Entity,
        &mut Critter,
        &mut Body,
        &mut Look,
        Option<&mut crate::anim::Anim>,
        Option<&crate::variety::Morph>,
        Option<&Creature>,
        Option<&super::age::Age>,
    )>() {
        // A footfall each stride on the ground, for the marks it leaves
        // (creatures/sign.rs): one print per stride, the hind foot coming
        // down in the front one's print the way a walking deer's does,
        // alternating left and right of the line.
        if body.on_ground && crit.air.is_none() {
            let d = look.pos - look.prev_pos;
            let moved = Vec3::new(d.x, 0.0, d.z).length();
            if moved < 1.0 {
                crit.walked += moved;
                let stride = (body.height * 0.75).clamp(0.25, 1.6);
                if crit.walked >= stride {
                    crit.walked = 0.0;
                    if (look.pos - player).length() < MARKS_M {
                        crit.left = !crit.left;
                        let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
                        let across = Vec3::new(-fwd.z, 0.0, fwd.x) * body.half_w * 0.7 * if crit.left { 1.0 } else { -1.0 };
                        let sp = me.map(|c| &species[c.species]);
                        let foot = sp.map_or(super::sign::Foot::Paw, |s| super::sign::Foot::of(&s.name, s.beh.fly_height > 0.0));
                        steps.push(super::sign::Step {
                            pos: look.pos + across,
                            yaw: look.yaw,
                            foot,
                            height: body.height,
                            // Only the big ones shake the ground: the
                            // square root of height times breadth, 1 for
                            // an elephant.
                            weight: if super::trample::big(body.half_w, body.height) {
                                (body.height * body.half_w / 2.4).sqrt() * morph.map_or(1.0, |m| m.scale)
                            } else {
                                0.0
                            },
                        });
                    }
                }
            }
        }
        look.begin_step();
        look.prev_lift = look.lift;
        look.flash = (look.flash - dt * 3.0).max(0.0);
        let to_player = look.pos - player;
        let dist = Vec3::new(to_player.x, 0.0, to_player.z).length();
        let sp = me.map(|c| &species[c.species]);
        // The random island's three animals have no species, so their
        // numbers come from the table they are spawned from.
        let old_island = if sp.is_none() { super::spawn::legacy(look.model) } else { None };
        let (wary, fast, slow) = match (sp, old_island) {
            (Some(s), _) => (s.beh.wary, s.beh.flee_speed, s.beh.walk_speed),
            (None, Some(l)) => (l.wary, l.fast, l.slow),
            (None, None) => (8.0, 6.5, 1.3),
        };
        let (fast, slow) = match morph {
            Some(m) => (fast * m.speed, slow * m.speed),
            None => (fast, slow),
        };
        // How far outside its own hours it is, 0 awake to 1 fast asleep
        // (creatures/diel.rs).
        // Weather puts an animal down the way the wrong hour does: cattle
        // stand out a shower, but a downpour or driving snow has them lying
        // under the trees in the middle of the afternoon (world/weather.rs
        // `sheltering`).
        let drowsy = sp.map_or(0.0, |s| super::diel::drowsy(s.beh.period(), hour)).max(sky.sheltering());
        // How far out it minds the player, and how readily it runs. Asleep,
        // both drop: you can walk up on bedded deer at night.
        let stage = age.map_or(super::age::Stage::Adult, |a| a.stage);
        let nerve = match sp {
            Some(s) => super::startle::Nerve::of(s, morph, stage),
            None => super::startle::Nerve::plain(wary * morph.map_or(1.0, |m| m.wary)),
        }
        .dozing(drowsy);
        let wary = nerve.dist;
        let verb = sp.map(|s| s.verb());
        let hop = verb.map_or(old_island.is_some_and(|l| l.hops), |v| v.hops());
        let still = verb.is_some_and(|v| v.still());
        // Something that eats plants: a walker with nothing on its list of
        // prey. The random island's three have no species, so none of them
        // browses -- there are no library plants there to browse.
        let browses = !still && sp.is_some_and(|s| s.hunts.is_empty());
        let wades = verb.is_some_and(|v| v.wades());
        let swims = sp.is_some_and(|s| s.beh.swims);
        // Swimming: in river water deeper than its legs, floating head up.
        let float_at = body.height * 0.8;
        let afloat = swims && terrain.inland_water(look.pos.x, look.pos.z).is_some_and(|w| w.1 > float_at);
        let shy = sp.is_none_or(|s| s.flees_player);
        let hides = verb.is_some_and(|v| v.hides());
        // Fliers that walk (herons) take wing when startled.
        let wings = sp.is_some_and(|s| s.beh.fly_height > 0.0) && models[look.model].rig.as_ref().is_some_and(|r| r.bones.iter().any(|b| b.role == Role::Wing));
        if let Some(mut a) = crit.air {
            a.t = (a.t + dt / a.dur).min(1.0);
            let t = a.t;
            let flat = a.from.lerp(a.to, t * t * (3.0 - 2.0 * t));
            // Up over the ground between, then down onto the spot.
            let under = terrain.surface(flat.x, flat.z).unwrap_or(flat.y);
            let arc = (std::f32::consts::PI * t).sin().powf(0.5);
            let y = (a.from.y + (a.to.y - a.from.y) * t).max(under * arc + flat.y * (1.0 - arc)) + a.height * arc;
            look.pos = Vec3::new(flat.x, y, flat.z);
            let d = a.to - a.from;
            look.yaw = crate::util::turn_toward(look.yaw, d.x.atan2(d.z) + std::f32::consts::PI, (dt * 4.0).min(1.0));
            body.vel = Vec3::ZERO;
            stats.flying_steps += 1;
            if t >= 1.0 {
                crit.air = None;
                crit.mood = Mood::Idle;
                crit.timer = 3.0 + crit.rng.unit() * 3.0;
            } else {
                crit.air = Some(a);
            }
            if let (Some(an), Some(rig)) = (anim, models[look.model].rig.as_ref()) {
                let vel = (look.pos - look.prev_pos) / dt.max(1e-4);
                an.step_flight(
                    rig,
                    models[look.model].voxel,
                    &crate::anim::flight::Input {
                        dt,
                        vel,
                        yaw: look.yaw,
                        perched: t > 0.97,
                        landing: t > 0.8,
                        cling: false,
                    },
                );
            }
            continue;
        }
        // The nearest predator it runs from, and the nearest prey it hunts.
        let mut hunter: Option<(f32, Vec3, Vec3)> = None;
        let mut prey: Option<(f32, Vec3, hecs::Entity)> = None;
        if let Some(s) = sp {
            for (os, op, ov, oe) in &snap {
                let Some(os) = os else { continue };
                let d = Vec3::new(op.x - look.pos.x, 0.0, op.z - look.pos.z).length();
                if d < 0.05 {
                    continue;
                }
                if s.flees.contains(os) && d < wary && hunter.is_none_or(|t| d < t.0) {
                    hunter = Some((d, *op, *ov));
                }
                if s.hunts.contains(os) && d < hunt_sight(body.height) && prey.is_none_or(|t| d < t.0) && crit.fed <= 0.0 {
                    prey = Some((d, *op, *oe));
                }
            }
        }
        let hiding = hides && dist < wary;
        stats.hiding_steps += hiding as u32;
        stats.startled += (crit.mood == Mood::Flee) as u32;
        // Alarm builds while the player (or a predator) presses on it, and it
        // bolts once a second's worth has gathered (creatures/startle.rs).
        // Cover is only looked for at close range, where one short ray is
        // cheap, and only against the player: a deer under a tree still hears
        // the wolf.
        let facing = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
        // Head down in the grass or at the water, it sees little of what
        // moves (anim/life.rs `graze_w`).
        let head_down = anim.as_deref().and_then(|a| a.life.as_deref()).map_or(0.0, |l| l.graze_w);
        let from_player = if shy && !hiding {
            let cover = || {
                let from = look.pos + Vec3::Y * (body.height * 0.6).max(0.2);
                let to = player + Vec3::Y * 1.2;
                let d = to - from;
                let len = d.length();
                len > 0.1 && store.raycast(from, d / len, len).is_some()
            };
            stats.cover_rays += (nerve.minds_people() && dist < super::startle::COVER_M) as u32;
            super::startle::pressure_on(&nerve, look.pos, facing, head_down, threat, cover)
        } else {
            0.0
        };
        // A neighbour bolting: the nearest one in reach, and which way it ran.
        let mut herd: Option<(f32, Vec3, Vec3)> = None;
        if crit.mood != Mood::Flee && !hiding {
            for (bp, bv, be) in &bolting {
                if *be == ent {
                    continue;
                }
                let d = (*bp - look.pos).length();
                if d < nerve.dist && herd.is_none_or(|h| d < h.0) {
                    herd = Some((d, *bp, *bv));
                }
            }
        }
        let from_herd = herd.map_or(0.0, |(_, bp, bv)| {
            super::startle::pressure_on(&nerve, look.pos, facing, head_down, &super::startle::Threat::alarm_of(bp, bv), || false)
        });
        let from_hunter = match hunter {
            Some((_, hp, hv)) if !hiding => {
                let t = super::startle::Threat::animal(hp, hv);
                super::startle::pressure(&nerve.hunted(), look.pos, facing, &t, || false)
            }
            _ => 0.0,
        };
        // Whichever is pressing harder is the one it runs from.
        // A herd that bolts runs the way the first one went.
        let away = if from_herd > from_player.max(from_hunter) {
            let (_, bp, bv) = herd.unwrap();
            if bv.length_squared() > 0.25 {
                Vec3::new(bv.x, 0.0, bv.z)
            } else {
                look.pos - bp
            }
        } else if from_hunter > from_player {
            let hp = hunter.map_or(player, |h| h.1);
            look.pos - hp
        } else {
            to_player
        };
        stats.alarmed_by_herd += (from_herd > from_player.max(from_hunter)) as u32;
        let bolts = super::startle::bolts(&mut crit.alarm, from_player.max(from_hunter).max(from_herd), dt);
        if hiding {
            crit.mood = Mood::Idle;
            crit.timer = crit.timer.max(2.0 + crit.rng.unit() * 3.0);
        } else if bolts {
            crit.mood = Mood::Flee;
            crit.heading = away.x.atan2(away.z) + (crit.rng.unit() - 0.5) * 0.6;
            crit.timer = 1.5 + crit.rng.unit();
            if wings && body.on_ground {
                if let Some(a) = take_wing(terrain, look.pos, crit.heading, wades, sp.map_or(6.0, |s| s.beh.fly_height), &mut crit.rng) {
                    crit.air = Some(a);
                    stats.flights += 1;
                    if let (Some(an), Some(rig)) = (anim, models[look.model].rig.as_ref()) {
                        if an.flight.is_none() {
                            let beh = &sp.unwrap().beh;
                            let pattern = beh.flight.as_deref().and_then(crate::anim::flight::Pattern::parse).unwrap_or(crate::anim::flight::Pattern::Flap);
                            let beat = beh.wingbeat.unwrap_or_else(|| crate::anim::flight::beat_for(rig, models[look.model].voxel));
                            let mut f = crate::anim::flight::Flight::new(pattern, beat, crit.rng.state);
                            f.fold = 1.0;
                            an.flight = Some(Box::new(f));
                        }
                        if let Some(f) = an.flight.as_mut() {
                            // Wings start folded, as they were on the ground.
                            f.fold = 1.0;
                        }
                    }
                    continue;
                }
            }
        }
        if sp.is_some() {
            // Chase prey, unless something is being run from.
            if hides || crit.mood == Mood::Flee {
                // Hiding or running, not hunting.
            } else if let Some((d, pp, pe)) = prey {
                if crit.mood != Mood::Flee {
                    let to = pp - look.pos;
                    crit.mood = Mood::Chase;
                    crit.heading = to.x.atan2(to.z);
                    crit.timer = crit.timer.max(0.6);
                    // Close enough to take it. No model for the pounce: it is
                    // in the predator's mouth from here (creatures/carry.rs).
                    if d < super::carry::catch_m().max(body.height * 0.6) && body.on_ground {
                        kills.push((ent, pe));
                        crit.mood = Mood::Feed;
                        crit.fed = super::carry::CARRY_S;
                        crit.timer = super::carry::CARRY_S;
                    }
                }
            }
            if crit.mood == Mood::Feed && crit.fed <= 0.0 {
                // Done with it: it is dropped where the predator stands, and
                // the predator is not hungry again for a while.
                crit.mood = Mood::Walk;
                crit.fed = super::carry::FULL_S;
            }
            // Waders keep to the water's edge.
            if wades {
                if let Some(here) = terrain.shore_dist(look.pos.x, look.pos.z) {
                    stats.wader_steps += 1;
                    stats.wader_by_water += (here <= 4.0) as u32;
                    // Drawn back to the bank: a wader that strays
                    // more than a few metres heads for the water, and
                    // one standing idle well inland sets off for it.
                    if here > 8.0 && crit.mood == Mood::Idle {
                        crit.mood = Mood::Walk;
                        crit.timer = crit.timer.max(4.0);
                    }
                    if here > 3.0 && crit.mood == Mood::Walk {
                        let mut best = (here, crit.heading);
                        for k in 0..8 {
                            let a = k as f32 * std::f32::consts::TAU / 8.0;
                            let q = look.pos + Vec3::new(a.sin(), 0.0, a.cos()) * 4.0;
                            let v = terrain.shore_dist(q.x, q.z).unwrap_or(f32::MAX);
                            if v < best.0 {
                                best = (v, a);
                            }
                        }
                        crit.heading = best.1;
                    }
                }
            }
        }
        crit.timer -= dt;
        crit.fed = (crit.fed - dt).max(0.0);
        let life = anim.as_mut().and_then(|a| a.life.as_mut());
        let water = terrain.shore_dist(look.pos.x, look.pos.z);
        if crit.timer <= 0.0 && life.is_some() {
            let l = life.unwrap();
            let was_moving = crit.mood != Mood::Idle;
            let (walk, dur, turn) = l.choose(was_moving);
            crit.mood = if walk { Mood::Walk } else { Mood::Idle };
            crit.timer = dur;
            crit.heading += turn;
            // Drinking keeps to dawn and dusk: midday drinks are mostly
            // skipped, and at the ends of the day a rest or a graze is now
            // and then a walk to the water instead.
            if l.act == Act::Drink && crit.rng.unit() > thirst {
                l.act = Act::Stand;
            } else if matches!(l.act, Act::Stand | Act::Graze) && crit.rng.unit() < (thirst - 0.3) * 0.35 {
                l.act = Act::Drink;
            }
            if l.act == Act::Drink {
                // Off to the water if there is some within 35 m, else not.
                match water {
                    Some(d) if d < 35.0 => {
                        crit.mood = if d > DRINK_AT { Mood::Walk } else { Mood::Idle };
                        crit.timer = dur + d / slow.max(0.3);
                    }
                    _ => l.act = Act::Stand,
                }
            } else if browses && crit.mood == Mood::Walk && crit.rng.unit() < BROWSE_CHANCE {
                // What a plant's `affords` list is for: a plant-eater setting
                // off makes for leaves and twigs it can reach rather than a
                // heading out of the hat, and stands and eats once it is
                // there. Bees look for `nectar` the same way (insects.rs).
                if let Some(near) = plants.nearest(look.pos.x, look.pos.z, BROWSE_M, Affordance::Browse) {
                    let to = near.pos - look.pos;
                    let d = Vec3::new(to.x, 0.0, to.z).length();
                    if d > AT_PLANT_M {
                        crit.heading = to.x.atan2(to.z);
                        crit.timer = dur + d / slow.max(0.3);
                        stats.browse_walks += 1;
                    } else {
                        l.act = Act::Graze;
                        crit.mood = Mood::Idle;
                        stats.browse_meals += 1;
                    }
                }
            }
            // Outside its hours it would rather be lying down: most of what
            // it plans to do becomes a long rest instead, so a meadow at
            // midnight is deer bedded in the grass rather than deer grazing.
            // Nothing beds down while it is running, hunting or already
            // alarmed; one that is woken gets to its feet quickly
            // (anim/life.rs raises the posture fast as the alarm rises).
            if drowsy > 0.0 && crit.alarm < 0.2 && matches!(crit.mood, Mood::Idle | Mood::Walk) && crit.rng.unit() < drowsy * drowsy * BEDS_DOWN {
                l.act = Act::Lie;
                crit.mood = Mood::Idle;
                crit.timer = 25.0 + crit.rng.unit() * 70.0;
            }
        } else if crit.timer <= 0.0 {
            if crit.mood == Mood::Walk || crit.mood == Mood::Flee || crit.rng.unit() < 0.4 {
                crit.mood = Mood::Idle;
                crit.timer = 1.0 + crit.rng.unit() * 3.5;
            } else {
                crit.mood = Mood::Walk;
                crit.heading += (crit.rng.unit() - 0.5) * 2.5;
                crit.timer = 2.0 + crit.rng.unit() * 4.0;
            }
        }
        // Sea turtles: now and then, unless hiding, crawl back to the sea.
        if hides && verb.is_some_and(|v| v.dives()) && !hiding && !crit.seaward && crit.rng.unit() < dt / 150.0 {
            crit.seaward = true;
        }
        if crit.seaward && !hiding {
            crit.mood = Mood::Walk;
        }
        // Seals: make for the sea when startled and dive in.
        if verb.is_some_and(|v| v.dives()) && (crit.mood == Mood::Flee || (crit.seaward && !hiding)) {
            let mut best = (terrain.coast(look.pos.x, look.pos.z).unwrap_or(0.0), crit.heading);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let q = look.pos + Vec3::new(a.sin(), 0.0, a.cos()) * 3.0;
                if let Some(c) = terrain.coast(q.x, q.z) {
                    if c < best.0 {
                        best = (c, a);
                    }
                }
            }
            crit.heading = best.1;
            crit.timer = crit.timer.max(1.0);
            let ahead = look.pos + Vec3::new(crit.heading.sin(), 0.0, crit.heading.cos()) * 1.5;
            if terrain.height(ahead.x, ahead.z).is_some_and(|h| h < sea + 0.2) {
                dived.push((ent, Vec3::new(ahead.x, sea, ahead.z), me.map_or(0, |c| c.species)));
                stats.dives += 1;
                continue;
            }
        }
        // Walking to drink: head for the nearest river or shore, stop at it.
        let drinking = anim.as_ref().and_then(|a| a.life.as_ref()).is_some_and(|l| l.act == Act::Drink);
        if drinking {
            if let Some(here) = water {
                if crit.mood == Mood::Walk && here <= DRINK_AT {
                    crit.mood = Mood::Idle;
                } else if crit.mood == Mood::Walk {
                    let mut best = (here, crit.heading);
                    for k in 0..8 {
                        let a = k as f32 * std::f32::consts::TAU / 8.0;
                        let q = look.pos + Vec3::new(a.sin(), 0.0, a.cos()) * 4.0;
                        let v = terrain.shore_dist(q.x, q.z).unwrap_or(f32::MAX);
                        if v < best.0 {
                            best = (v, a);
                        }
                    }
                    crit.heading = best.1;
                }
                if crit.mood == Mood::Idle && here <= DRINK_AT + 1.0 {
                    stats.drinking_steps += 1;
                }
            }
        }
        let speed = match crit.mood {
            _ if still => 0.0,
            Mood::Idle => 0.0,
            Mood::Walk if crit.seaward => fast.max(slow),
            Mood::Walk => slow,
            Mood::Flee => fast,
            Mood::Chase => fast * 0.9,
            // Carrying a kill off: a steady walk, no hurry.
            Mood::Feed => slow * 0.8,
        };
        // A lying or sitting creature gets up before it moves.
        let speed = speed * anim.as_ref().map_or(1.0, |a| a.mobility());
        // Steer round trunks, walls, drops and water: if the way ahead is
        // blocked, take the nearest clear heading, trying first the side
        // it last went round.
        // A big animal walks through low brush: the shrubs on its line are
        // flattened (creatures/trample.rs) and it does not steer round them.
        // Past full detail (terrain.rs `far_ground`) there are no voxels to
        // walk on. Any walker standing out there walks the generator's
        // ground instead, keeping off drops, deep water and tree trunks.
        let far = super::terrain::far_ground(store, look.pos.x, look.pos.z).is_some();
        let was_far = std::mem::replace(&mut crit.far, far);
        // True only while a shrub ahead is one `Trample::flatten` will take
        // down (`trample::standing`), so it never holds its line into
        // something that stays.
        let mut crushing = false;
        if speed > 0.0 && !far && sp.is_some() && super::trample::big(body.half_w, body.height) && dist < super::trample::NEAR_M && body.on_ground {
            let reach = (0.8 + speed * 0.45).clamp(1.2, 3.5);
            let tall = super::trample::shoulder(body.height);
            let sh = super::crowd::Shape::of_walker(look, body, models, morph);
            let way = glam::Vec2::new(crit.heading.sin(), crit.heading.cos());
            // The end of its spine toward where it is going, run on by the
            // distance it looks ahead.
            let (tip, tail) = if (sh.a - sh.mid()).dot(way) >= 0.0 { (sh.a, sh.b) } else { (sh.b, sh.a) };
            let path = super::crowd::Shape {
                a: tail,
                b: tip + way * reach,
                ..sh
            };
            let mid = path.mid();
            for p in plants.near(mid.x, mid.y, path.bound() + super::trample::TALL_M, None) {
                if p.height > tall || p.trunk > super::trample::TRUNK_M || (p.pos.y - look.pos.y).abs() > 1.0 {
                    continue;
                }
                let q = glam::Vec2::new(p.pos.x, p.pos.z);
                if path.spine_dist(q) > sh.r + p.crown * 0.6 {
                    continue;
                }
                let bark = super::trample::bark_of(terrain.land, p.plant);
                if !super::trample::standing(store, p.pos, p.height, p.crown, bark) {
                    continue;
                }
                crush.push(p.pos);
                // Only a shrub ahead of the body is a reason to keep the
                // line: one already under it is being walked off.
                crushing |= (q - tip).dot(way) > -sh.r;
            }
        }
        if speed > 0.0 {
            let reach = (0.8 + speed * 0.45).clamp(1.2, 3.5);
            // The collision box is a square the width of the body; a long
            // animal's head and rump reach past it, so look wider -- but only
            // half as wide again, or a giraffe treats itself as a three-metre
            // blob and finds every village and wood impassable.
            let wide = body.half_w.max((body.height * 0.35).min(body.half_w * 1.5));
            // Anything will wade to about a third of its own height, so a
            // rabbit keeps to the edge and an elephant crosses a channel;
            // waders go deeper still, and swimmers over their heads.
            let wade_m = (body.height * 0.35).max(0.3).max(if wades { 0.6 } else { 0.0 });
            let water_line = sea - (wade_m - 0.2).max(0.0);
            let max_depth = if swims { f32::INFINITY } else { wade_m };
            let clear = |a: f32| {
                let d = Vec3::new(a.sin(), 0.0, a.cos());
                if far {
                    far_walkable(terrain, plants, look.pos, d, reach + body.half_w, body.half_w, water_line, max_depth)
                } else {
                    walkable(terrain, look.pos, d, reach, wide, body.height, water_line, max_depth)
                }
            };
            if !crushing && !clear(crit.heading) {
                stats.turned += 1;
                let mut found = None;
                'fan: for k in 1..=7 {
                    for s in [crit.side, -crit.side] {
                        let off = s * k as f32 * 0.4;
                        if clear(crit.heading + off) {
                            found = Some(off);
                            break 'fan;
                        }
                    }
                }
                if found.is_none() {
                    // Nothing clear at arm's length. Try again at half the
                    // reach and the body's own width: an animal that has
                    // walked into a gap between two trees can creep out of
                    // it instead of spinning on the spot.
                    let tight = |a: f32| {
                        let d = Vec3::new(a.sin(), 0.0, a.cos());
                        if far {
                            far_walkable(terrain, plants, look.pos, d, reach * 0.5, body.half_w, water_line, max_depth)
                        } else {
                            walkable(terrain, look.pos, d, reach * 0.5, body.half_w, body.height, water_line, max_depth)
                        }
                    };
                    'tight: for k in 1..=9 {
                        for s in [crit.side, -crit.side] {
                            let off = s * k as f32 * 0.35;
                            if tight(crit.heading + off) {
                                found = Some(off);
                                break 'tight;
                            }
                        }
                    }
                }
                match found {
                    Some(off) => {
                        crit.heading += off;
                        crit.side = off.signum();
                    }
                    // Boxed in: stop, face back the way it came, think
                    // again. It keeps turning the same way while it is
                    // stuck, so it works round whatever has it rather than
                    // flipping end for end every frame.
                    _ => {
                        crit.heading += crit.side * (2.0 + crit.rng.unit() * 0.6);
                        if crit.mood == Mood::Walk {
                            crit.mood = Mood::Idle;
                            crit.timer = 0.8 + crit.rng.unit() * 1.5;
                        }
                    }
                }
            }
        }
        let speed = match crit.mood {
            Mood::Idle => 0.0,
            _ if afloat => speed.min(slow.max(0.8) * 1.2),
            _ => speed,
        };
        let dir = Vec3::new(crit.heading.sin(), 0.0, crit.heading.cos());
        // Turning has inertia, the more so the bigger the body: a deer
        // swings round in a stride, a triceratops leans into a wide arc. A
        // big walker turns toward its heading at its own rate and goes the
        // way it faces, slowing into a sharp turn rather than pivoting on
        // the spot or sliding sideways while its body catches up.
        let rate = turn_rate(body.height, crit.mood);
        let heavy = rate < FREE_TURN && !afloat && crit.air.is_none();
        let (dir, speed) = if heavy && speed > 0.0 {
            let want = crit.heading + std::f32::consts::PI;
            let off = crate::util::wrap_angle(want - look.yaw);
            look.yaw += off.clamp(-rate * dt, rate * dt);
            let facing = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
            let lined_up = (facing.dot(dir) + 1.0) * 0.5;
            (facing, speed * (lined_up * lined_up).max(TURN_CRAWL))
        } else {
            (dir, speed)
        };
        let target = dir * speed;
        let k = (dt * if body.on_ground { 10.0 } else { 2.0 }).min(1.0);
        body.vel.x += (target.x - body.vel.x) * k;
        body.vel.z += (target.z - body.vel.z) * k;
        body.vel.y = (body.vel.y - 24.0 * dt).max(-40.0);
        if hop && speed > 0.0 && body.on_ground {
            body.vel.y = 3.6;
        }
        let before = look.pos;
        let res = if let Some(gy) = far.then(|| super::terrain::far_ground(store, look.pos.x + body.vel.x * dt, look.pos.z + body.vel.z * dt)).flatten() {
            // Out in the distance: straight along the ground, no voxels to
            // meet. The steering above has kept it off drops and trunks.
            look.pos = Vec3::new(look.pos.x + body.vel.x * dt, gy, look.pos.z + body.vel.z * dt);
            body.vel.y = 0.0;
            body.on_ground = true;
            crate::physics::MoveResult { stepped: 0.0, blocked: false }
        } else {
            // The first step on voxels after generator ground: the two can
            // differ by a step, so come up out of them rather than start
            // inside.
            if was_far && crate::physics::hits(store, look.pos, body.half_w, body.height) {
                if let Some((gy, _)) = voxel_ground(store, look.pos.x, look.pos.z, look.pos.y + 2.0) {
                    if gy > look.pos.y && gy - look.pos.y < 2.0 {
                        look.pos.y = gy + 0.01;
                    }
                }
            }
            move_body(store, &mut look.pos, body, body.vel * dt, 0.5)
        };
        // Afloat: held at the surface and carried by the current.
        if swims {
            if let Some((level, depth)) = terrain.inland_water(look.pos.x, look.pos.z) {
                if depth > float_at && look.pos.y < level - float_at {
                    let c = terrain.current(look.pos.x, look.pos.z);
                    look.pos.x += c.x * dt;
                    look.pos.z += c.y * dt;
                    look.pos.y = level - float_at;
                    body.vel.y = 0.0;
                    body.on_ground = true;
                    stats.swimming_steps += 1;
                }
            }
        }
        if speed > 0.0 {
            stats.moving += 1;
            stats.blocked += res.blocked as u32;
        }
        if res.blocked && body.on_ground && speed > 0.0 && !crushing {
            // Jump only onto a low ledge with room above it, never up a
            // trunk or a wall.
            let up = look.pos + Vec3::Y * 1.0;
            if !crate::physics::hits(store, up, body.half_w, body.height) && !crate::physics::hits(store, up + dir * 0.3, body.half_w, body.height) {
                body.vel.y = 5.5;
                stats.jumps += 1;
            }
        }
        // Pushing without getting anywhere: turn well away.
        let moved = Vec3::new(look.pos.x - before.x, 0.0, look.pos.z - before.z).length();
        if speed > 0.3 && moved < speed * dt * 0.25 {
            crit.stuck += dt;
        } else {
            crit.stuck = (crit.stuck - dt).max(0.0);
        }
        if crit.stuck > 0.6 {
            crit.stuck = 0.0;
            crit.heading += std::f32::consts::PI * (0.5 + crit.rng.unit());
            stats.stuck += 1;
        }
        if let Some((_, m)) = voxel_ground(store, look.pos.x, look.pos.z, look.pos.y + 0.1) {
            stats.on_foliage += crate::mesh::sprites::leafy(m) as u32;
        }
        if speed > 0.1 && !heavy {
            let want = dir.x.atan2(dir.z) + std::f32::consts::PI;
            look.yaw = crate::util::turn_toward(look.yaw, want, (dt * FREE_TURN).min(1.0));
        }
        if !slopes || afloat || far {
            look.lift = 0.0;
        } else if body.on_ground {
            let target = drawn_ground_lift(store, look.pos);
            look.lift += (target - look.lift) * (dt * 20.0).min(1.0);
        }
        let moving = Vec3::new(body.vel.x, 0.0, body.vel.z).length();
        look.bob = if !hop && body.on_ground { (moving * 0.02) * (look.pos.x + look.pos.z).sin().abs() } else { 0.0 };
        if let (Some(a), Some(rig)) = (anim, models[look.model].rig.as_ref()) {
            look.bob = 0.0;
            let from_y = look.pos.y + 1.5;
            let ground = |x: f32, z: f32| voxel_ground(store, x, z, from_y).map(|g| g.0).or_else(|| super::terrain::far_ground(store, x, z));
            a.step(
                rig,
                models[look.model].voxel,
                &crate::anim::life::Input {
                    dt,
                    speed: moving,
                    moving: crit.mood != Mood::Idle,
                    alarm: crit.mood == Mood::Flee,
                    on_ground: body.on_ground,
                    pos: look.pos,
                    yaw: look.yaw,
                    player,
                    wary: if shy { wary } else { 0.0 },
                    ground: &ground,
                    water: water_surface(terrain, look.pos),
                    hide: hiding,
                    pivot: models[look.model].pivot,
                    scale: morph.map_or(1.0, |m| m.scale),
                },
            );
            // An otter idling or asleep in the river drifts on its back with
            // the current, and rolls over as soon as it moves off.
            if sp.is_some_and(|s| s.beh.back_float) {
                let want = if afloat && crit.mood == Mood::Idle { 1.0 } else { 0.0 };
                crit.back += (want - crit.back).clamp(-dt * 1.5, dt * 0.5);
                super::swim::back_pose(rig, &mut a.pose, crit.back, look.pos.x * 3.0 + look.pos.z * 2.0);
            }
        }
    }
    super::carry::take(world, species, models, &kills, now);
    for (e, at, s) in dived {
        let _ = world.despawn(e);
        super::swim::splash_at(cues, at, 0.7);
        // It swims on as its species' water form (its entry's `in_water`), or
        // as itself where it has none.
        let swimmer = species[s].water.unwrap_or(s);
        // It surfaces a few metres out, where the water is a metre deep.
        let out = (0..8)
            .flat_map(|r| (0..8).map(move |k| (r, k)))
            .map(|(r, k)| {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                at + Vec3::new(a.sin(), 0.0, a.cos()) * (1.0 + r as f32 * 1.5)
            })
            .find(|q| sea - terrain.bed(q.x, q.z) >= 1.0);
        if let Some(q) = out {
            super::swim::spawn_school(world, swimmer, species, models, q, sea - terrain.bed(q.x, q.z), 1, e.id() ^ 0x5ea1);
        }
    }
}

/// How thirsty animals are at `hour`, 0.3 at midday to 1 at dawn and dusk:
/// the share of planned drinks they keep (world/clock.rs runs 5.2 to 18.8).
/// The water surface standing at `pos`: a river or lake if there is one over
/// the ground here, else the sea where the ground is below it. None on dry
/// land. A drinking animal lowers its mouth to this rather than to the bed.
fn water_surface(terrain: &Terrain, pos: Vec3) -> Option<f32> {
    if let Some((level, _)) = terrain.inland_water(pos.x, pos.z) {
        return Some(level);
    }
    (pos.y < terrain.sea).then_some(terrain.sea)
}

fn thirst(hour: f32) -> f32 {
    let from_end = (hour - crate::world::clock::dawn()).min(crate::world::clock::dusk() - hour).max(0.0);
    1.0 - 0.7 * (from_end / 2.5).clamp(0.0, 1.0)
}

/// Where a startled flier goes: 25-50 m away, roughly away from the
/// threat, onto dry open ground (by the water for a wader).
fn take_wing(terrain: &Terrain, from: Vec3, heading: f32, wades: bool, height: f32, rng: &mut Rng) -> Option<Air> {
    // Under a roof the flight would arc over the land laid on top of it, so
    // it goes on foot (#377).
    if terrain.roof(from).is_some() {
        return None;
    }
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..12 {
        let a = heading + (rng.unit() - 0.5) * 1.6;
        let r = 25.0 + rng.unit() * 25.0;
        let (x, z) = (from.x + a.sin() * r, from.z + a.cos() * r);
        if x < 8.0 || z < 8.0 || x > terrain.size() - 8.0 || z > terrain.size() - 8.0 {
            continue;
        }
        let Some((gy, m)) = terrain.ground(x, z, terrain.store.top_m()) else {
            continue;
        };
        if gy < terrain.sea + 0.4 || crate::mesh::sprites::leafy(m) || water_voxel(m) || terrain.height(x, z).is_some_and(|h| (h - gy).abs() > 1.0) {
            continue;
        }
        let score = if wades { terrain.shore_dist(x, z).unwrap_or(0.0) } else { 0.0 };
        if wades && score > 5.0 {
            continue;
        }
        if best.is_none_or(|b| score < b.0) {
            best = Some((score, Vec3::new(x, gy + 0.01, z)));
        }
    }
    let (_, to) = best?;
    let dist = (to - from).with_y(0.0).length();
    Some(Air {
        from,
        to,
        t: 0.0,
        dur: (dist / 4.5).max(4.0),
        height: height.max(3.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::startle::Threat;
    use crate::ride::Mode;

    /// A flat world of solid ground 1 m thick, 40 m on a side. Its chunks
    /// are loaded first: `Store::set` drops a voxel in a chunk that is not,
    /// and an unloaded chunk reads as solid.
    fn flat_store() -> crate::world::store::Store {
        let mut store = crate::world::store::Store::default();
        for cx in -1..=5 {
            for cz in -1..=5 {
                store.insert_column(cx, cz, 2, 0, Vec::new());
            }
        }
        for vx in -8..168 {
            for vz in -8..168 {
                for vy in 0..4 {
                    store.set(vx, vy, vz, 1);
                }
            }
        }
        store
    }

    fn one_model() -> Model {
        Model {
            mesh: Default::default(),
            voxel: 0.0625,
            pivot: Vec3::ZERO,
            half_w: 0.28,
            height: 1.4,
            reach: [-0.28, 0.28],
            rig: None,
            skeleton: None,
            block: 0,
        }
    }

    fn one_species(flightiness: f32, wary: f32) -> Species {
        let beh: crate::library::Behaviour = serde_json::from_value(serde_json::json!({ "movement": "graze", "wary": wary, "flightiness": flightiness, "flees_from": ["player"] })).unwrap();
        Species {
            name: "test".into(),
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
        }
    }

    /// Seconds before the animal bolts, stepping the real mover at 1/30 s,
    /// with the player held at `from` and travelling at `vel` in `mode`.
    fn until_flight(flightiness: f32, at: Vec3, from: Vec3, vel: Vec3, mode: Mode) -> Option<f32> {
        let store = flat_store();
        let species = vec![one_species(flightiness, 10.0)];
        let models = vec![one_model()];
        let mut world = hecs::World::new();
        // Facing +Z, away from a player who stands at the origin.
        let e = world.spawn((Critter::new(0.0, 99.0, 7, 1.0), Body::new(0.28, 1.4), Look::new(0, at, std::f32::consts::PI), Creature::of(0)));
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let threat = Threat::new(from, vel, mode);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        for step in 0..600 {
            let flora = crate::creatures::plants::Flora::default();
            update(
                &mut world,
                &species,
                &flora,
                &terrain,
                dt,
                &threat,
                false,
                &models,
                12.0,
                &Default::default(),
                step as f32 * dt,
                &mut stats,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
            );
            if world.get::<&Critter>(e).unwrap().mood == Mood::Flee {
                return Some(step as f32 * dt);
            }
        }
        None
    }

    #[test]
    fn a_walker_bolts_from_a_bike_and_ignores_a_wingsuit_overhead() {
        let at = Vec3::new(0.0, 1.0, 6.0);
        let ground = Vec3::new(0.0, 1.0, 0.0);
        let walk = until_flight(1.2, at, ground, Vec3::new(0.0, 0.0, 1.4), Mode::Foot).expect("walker");
        let bike = until_flight(1.2, at, ground, Vec3::new(0.0, 0.0, 8.0), Mode::Bike).expect("bike");
        assert!(bike < walk * 0.6, "bike {bike}s, on foot {walk}s");
        // A glider eight metres up and a long way out, closing fast: it is
        // outside the flee distance because height counts, and quiet enough
        // not to matter once it is inside.
        let wing = until_flight(1.2, at, Vec3::new(0.0, 9.0, 0.0), Vec3::new(0.0, 0.0, 14.0), Mode::Wing);
        assert!(wing.is_none_or(|w| w > walk * 2.0), "wingsuit {wing:?}, on foot {walk}s");
    }

    #[test]
    fn a_walker_holds_its_ground_against_a_still_player_and_an_indifferent_one_never_moves() {
        let at = Vec3::new(0.0, 1.0, 6.0);
        // Standing still at six metres of a ten-metre flee distance presses
        // so little that the alarm settles (startle.rs `QUIET`): it may watch,
        // and need never run.
        let still = until_flight(1.2, at, Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, Mode::Foot);
        let walk = until_flight(1.2, at, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.4), Mode::Foot).expect("walker");
        assert!(still.is_none_or(|s| s > walk * 1.5), "standing {still:?}s, walking {walk}s");
        assert!(until_flight(0.0, at, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.4), Mode::Foot).is_none());
    }

    #[test]
    fn a_walker_runs_from_a_predator_that_closes_on_it() {
        let store = flat_store();
        // 0 rabbit, which flees 1 fox; the player is far away and standing.
        let mut rabbit = one_species(1.8, 10.0);
        rabbit.flees = vec![1];
        let fox = one_species(0.9, 10.0);
        let species = vec![rabbit, fox];
        let models = vec![one_model()];
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let threat = super::super::startle::Threat::new(Vec3::new(200.0, 1.0, 200.0), Vec3::ZERO, Mode::Foot);
        let dt = 1.0 / 30.0;
        let run = |fox_at: Option<Vec3>| {
            let mut world = hecs::World::new();
            let at = Vec3::new(0.0, 1.0, 0.0);
            let e = world.spawn((Critter::new(0.0, 99.0, 3, 1.0), Body::new(0.2, 0.35), Look::new(0, at, 0.0), Creature::of(0)));
            if let Some(f) = fox_at {
                let mut c = Critter::new(0.0, 99.0, 5, 1.0);
                c.mood = Mood::Idle;
                world.spawn((c, Body::new(0.2, 0.7), Look::new(0, f, 0.0), Creature::of(1)));
            }
            let mut stats = Pathing::default();
            for step in 0..300 {
                let flora = crate::creatures::plants::Flora::default();
                update(
                    &mut world,
                    &species,
                    &flora,
                    &terrain,
                    dt,
                    &threat,
                    false,
                    &models,
                    12.0,
                    &Default::default(),
                    step as f32 * dt,
                    &mut stats,
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
                if world.get::<&Critter>(e).unwrap().mood == Mood::Flee {
                    return Some(step as f32 * dt);
                }
            }
            None
        };
        assert!(run(None).is_none(), "nothing near it and it ran anyway");
        let near = run(Some(Vec3::new(0.0, 1.0, 4.0))).expect("fox four metres off");
        let far = run(Some(Vec3::new(0.0, 1.0, 9.5))).unwrap_or(f32::MAX);
        assert!(near < 2.0, "fox at 4 m took {near}s");
        assert!(far > near, "fox at 9.5 m {far}s, at 4 m {near}s");
    }

    /// A walker `half_w` by `height` set walking +Z for five seconds at a
    /// shrub 7 m ahead: a 1.5 m bush of solid leaf voxels 1.6 m across. How
    /// far it got, whether it asked for the shrub to be flattened, and how
    /// many times it turned aside.
    fn walk_at_a_shrub(half_w: f32, height: f32, length: f32) -> (f32, usize, u32) {
        let mut store = flat_store();
        let shrub = Vec3::new(0.0, 1.0, 5.0);
        for vy in 4..10 {
            for vz in 17..23 {
                for vx in -3..3 {
                    store.set(vx, vy, vz, crate::world::material::LEAVES);
                }
            }
        }
        let flora = crate::creatures::plants::Flora::from_refs(vec![crate::creatures::plants::PlantRef {
            pos: shrub,
            plant: 0,
            height: 1.5,
            crown: 0.8,
            trunk: 0.0,
            affords: Default::default(),
        }]);
        let species = vec![one_species(0.0, 0.0)];
        let mut model = one_model();
        (model.half_w, model.height, model.reach) = (half_w, height, [-length * 0.5, length * 0.5]);
        let models = vec![model];
        let mut world = hecs::World::new();
        let mut c = Critter::new(0.0, 99.0, 7, 1.0);
        c.mood = Mood::Walk;
        let e = world.spawn((c, Body::new(half_w, height), Look::new(0, Vec3::new(0.0, 1.0, -2.0), std::f32::consts::PI), Creature::of(0)));
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let threat = Threat::new(Vec3::new(0.0, 1.0, -30.0), Vec3::ZERO, Mode::Foot);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        let mut crush = Vec::new();
        for step in 0..150 {
            update(
                &mut world,
                &species,
                &flora,
                &terrain,
                dt,
                &threat,
                false,
                &models,
                12.0,
                &Default::default(),
                step as f32 * dt,
                &mut stats,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut crush,
            );
        }
        let z = world.get::<&Look>(e).unwrap().pos.z;
        (z, crush.len(), stats.turned)
    }

    #[test]
    fn a_big_walker_keeps_its_line_through_a_shrub_and_a_deer_goes_round() {
        let (_, crushed, turned) = walk_at_a_shrub(1.1, 3.0, 8.0);
        assert!(crushed > 0, "a triceratops asked for nothing to be flattened");
        assert_eq!(turned, 0, "a triceratops steered round a bush");
        let (z, crushed, turned) = walk_at_a_shrub(0.28, 1.4, 1.6);
        assert_eq!(crushed, 0, "a deer flattened a bush");
        assert!(turned > 0 && z < 4.0, "a deer walked into a bush: at z {z}, turned {turned} times");
    }

    /// Heading the opposite way to where it faces, a big walker swings
    /// round over a second or more and keeps creeping forward as it does;
    /// a small one is round at once.
    #[test]
    fn a_big_walker_turns_in_an_arc_and_a_small_one_on_the_spot() {
        let store = flat_store();
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let species = vec![one_species(0.0, 0.0)];
        let threat = Threat::new(Vec3::new(0.0, 1.0, -30.0), Vec3::ZERO, Mode::Foot);
        let dt = 1.0 / 30.0;
        let turn = |half_w: f32, height: f32| {
            let mut model = one_model();
            (model.half_w, model.height) = (half_w, height);
            let models = vec![model];
            let mut world = hecs::World::new();
            // Facing +Z (yaw PI), told to walk -Z.
            let mut c = Critter::new(std::f32::consts::PI, 99.0, 7, 1.0);
            c.mood = Mood::Walk;
            let e = world.spawn((c, Body::new(half_w, height), Look::new(0, Vec3::new(10.0, 1.0, 10.0), std::f32::consts::PI), Creature::of(0)));
            let mut stats = Pathing::default();
            let mut quickest = 0.0f32;
            for step in 0..30 {
                let before = world.get::<&Look>(e).unwrap().yaw;
                update(&mut world, &species, &Default::default(), &terrain, dt, &threat, false, &models, 12.0, &Default::default(), step as f32 * dt, &mut stats, &mut Vec::new(), &mut Vec::new(), &mut Vec::new());
                let after = world.get::<&Look>(e).unwrap().yaw;
                quickest = quickest.max(crate::util::wrap_angle(after - before).abs() / dt);
            }
            let yaw = world.get::<&Look>(e).unwrap().yaw;
            (quickest, crate::util::wrap_angle(yaw).abs())
        };
        let (big_rate, big_yaw) = turn(1.1, 3.0);
        assert!(big_rate < 2.0, "a triceratops turned at {big_rate} rad/s");
        assert!(big_yaw > 1.0, "a triceratops came round in a second");
        let (_, small_yaw) = turn(0.28, 1.4);
        assert!(small_yaw < 0.2, "a deer was still {small_yaw} rad off after a second");
    }

    #[test]
    fn a_walker_is_slower_to_bolt_from_someone_behind_it() {
        // It faces +Z. Standing it six metres ahead of the player puts the
        // player behind it; six metres behind puts the player in front.
        let behind = until_flight(1.2, Vec3::new(0.0, 1.0, 6.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.4), Mode::Foot).expect("behind");
        let front = until_flight(1.2, Vec3::new(0.0, 1.0, -6.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.4), Mode::Foot).expect("front");
        assert!(behind > front, "from behind {behind}s, from in front {front}s");
    }
}
