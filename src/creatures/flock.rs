//! Boids: creatures that move as a group. A library creature opts in with its
//! movement verb and an optional `group` block in its behaviour:
//!
//! - `flock`: a flying flock (starlings). The birds wheel round a slowly
//!   wandering point, now and then come down together on open ground or a
//!   tree, and burst up and away when the player comes within `wary`.
//! - `soar`: a lone bird (an eagle) circling wide and high, that now and
//!   then comes down on the top of a tall tree that affords `perch` (a
//!   spruce, a fir, a redwood, a drowned snag) and sits there a while.
//!   One whose species `hunts` stoops: when it is hungry it picks something
//!   it eats below it in the open (a gull in the air or on the sea, a duck,
//!   a fish within a metre or so of the surface), dives on it at twice its
//!   top speed leading it, and at the strike takes it or misses. A kill is
//!   carried off in the talons (carry.rs), to a tall tree if there is one.
//! - `glide`: a few big seabirds (albatrosses) sweeping wide circles on held
//!   wings, that now and then come down together on the highest ground near
//!   and sit there a while.
//! - any walking verb with `group`: a family group (deer in twos and threes)
//!   that grazes together and bolts together (herd.rs).
//!
//! A display (creatures/display: starlings wheeling, a hawk's sky dance)
//! takes members out of the boids for a while: each carries a `Show` and is
//! put where the display says, and flies on from there when it ends.
//!
//! Each bird steers by the three classic rules over its flockmates within
//! `range`: separation (keep `spacing` apart), alignment (fly the way the
//! neighbours fly) and cohesion (stay with them), plus a pull toward the
//! flock's goal and a spring toward its flying height above the ground.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;
use serde::Deserialize;

/// How a species groups up, from its library behaviour.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct Group {
    /// Smallest and largest group spawned.
    #[serde(default = "one_one")]
    pub size: [u32; 2],
    /// Distance kept between neighbours, metres.
    #[serde(default = "spacing")]
    pub spacing: f32,
    /// How far a member sees its neighbours, metres.
    #[serde(default = "range")]
    pub range: f32,
    #[serde(default = "unit_w")]
    pub separation: f32,
    #[serde(default = "unit_w")]
    pub alignment: f32,
    #[serde(default = "unit_w")]
    pub cohesion: f32,
}

fn one_one() -> [u32; 2] {
    [1, 1]
}
fn spacing() -> f32 {
    1.5
}
fn range() -> f32 {
    8.0
}
fn unit_w() -> f32 {
    1.0
}

impl Default for Group {
    fn default() -> Self {
        Group {
            size: one_one(),
            spacing: spacing(),
            range: range(),
            separation: 1.0,
            alignment: 1.0,
            cohesion: 1.0,
        }
    }
}

/// The boids acceleration for one member at `p` moving at `v`, from its
/// neighbours' positions and velocities (itself excluded).
/// `others` may include the member itself; it is skipped by distance.
pub fn steer(p: Vec3, v: Vec3, others: impl IntoIterator<Item = (Vec3, Vec3)>, g: &Group) -> Vec3 {
    let (mut sep, mut sum_p, mut sum_v, mut n) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0);
    for (q, w) in others {
        let d = p - q;
        let dist = d.length();
        if dist > g.range || dist < 1e-4 {
            continue;
        }
        if dist < g.spacing * 1.5 {
            sep += d / (dist * dist) * g.spacing;
        }
        sum_p += q;
        sum_v += w;
        n += 1.0;
    }
    if n == 0.0 {
        return Vec3::ZERO;
    }
    let coh = (sum_p / n - p) * 0.6;
    let ali = (sum_v / n - v) * 1.2;
    sep * 6.0 * g.separation + ali * g.alignment + coh * g.cohesion
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Fly,
    Land,
    Perch,
    Soar,
    /// A hunter diving on its prey.
    Stoop,
}

/// How long a stoop may last before the hunter gives it up, seconds.
const STOOP_S: f32 = 14.0;
/// How far round and below itself a hunter looks for prey, metres.
const STOOP_REACH_M: f32 = 110.0;
/// How close the talons must come, metres.
const STRIKE_M: f32 = 1.6;
/// Fish deeper than this under the surface are out of reach, metres.
const FISH_DEPTH_M: f32 = 1.3;
/// The chance a strike takes its prey: a fish, a bird sitting on the
/// water, the ground or a perch, a bird in the air (which jinks).
const TAKE_FISH: f32 = 0.5;
const TAKE_SITTING: f32 = 0.25;
const TAKE_FLYING: f32 = 0.3;

/// One flock, kept as its own entity (no Look, so nothing is drawn for it).
pub struct Flock {
    pub species: usize,
    pub mode: Mode,
    /// Where the flock is heading; it wanders round `home`.
    pub goal: Vec3,
    pub home: Vec3,
    pub timer: f32,
    phase: f32,
    /// Sweeps wide and low and roosts high (`glide`).
    glide: bool,
    /// Circles high and alone, and perches only on tall trees (`soar`).
    soar: bool,
    /// A hunter's prey while it stoops.
    prey: Option<hecs::Entity>,
    /// Seconds until a hunter is hungry again, and until it next looks.
    full: f32,
    look_t: f32,
    /// Seconds it has left carrying a kill.
    hold: f32,
    rng: Rng,
}

/// The shortest tree a soaring bird comes down on, metres.
const PERCH_TREE_M: f32 = 14.0;
/// How far round a soaring bird looks for one, metres.
const PERCH_REACH_M: f32 = 160.0;

/// A member of a flock.
pub struct Flyer {
    pub flock: hecs::Entity,
    pub vel: Vec3,
    pub perched: bool,
    /// In a display last step.
    pub shown: bool,
}

impl Flock {
    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }
}

/// Spawn a flock of `n` (or a soaring bird, n = 1) of species `s` above `at`.
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, at: Vec3, n: u32, seed: u32) -> u32 {
    let (soar, glide) = match sp.verb().mover {
        super::locomotion::Mover::Flock { soar, glide } => (soar, glide),
        _ => (false, false),
    };
    let mut f = Flock {
        species: s,
        mode: if soar { Mode::Soar } else { Mode::Fly },
        goal: at,
        home: at,
        timer: 0.0,
        phase: 0.0,
        glide,
        soar,
        prey: None,
        full: 0.0,
        look_t: 0.0,
        hold: 0.0,
        rng: Rng::new(seed | 1),
    };
    f.timer = 20.0 + f.rand() * 30.0;
    f.phase = f.rand() * std::f32::consts::TAU;
    f.full = 20.0 + f.rand() * 60.0;
    let height = sp.beh.fly_height.max(3.0);
    let mut members = Vec::new();
    for _ in 0..n.max(1) {
        let off = Vec3::new(f.rand() - 0.5, f.rand() * 0.5, f.rand() - 0.5) * (2.0 + n as f32 * 0.25);
        let a = f.rand() * std::f32::consts::TAU;
        let pos = at + Vec3::Y * height + off;
        let vel = Vec3::new(a.sin(), 0.0, a.cos()) * sp.beh.walk_speed.max(2.0);
        members.push((pos, vel));
    }
    let fe = world.spawn((f,));
    for (pos, vel) in members {
        let yaw = vel.x.atan2(vel.z) + std::f32::consts::PI;
        world.spawn((Flyer { flock: fe, vel, perched: false, shown: false }, Creature::of(s), Look::new(sp.model, pos, yaw)));
    }
    n.max(1)
}

/// Step every flock. Birds keep above `terrain.surface` (trees included, so
/// they can perch in them) and never come down where `terrain.wet`.
/// Returns the kills made this step, (hunter, prey), for carry::take.
pub fn update(
    world: &mut hecs::World,
    species: &[Species],
    groups: &[Group],
    flora: &super::plants::Flora,
    dt: f32,
    threat: &super::startle::Threat,
    terrain: &Terrain,
) -> Vec<(hecs::Entity, hecs::Entity)> {
    let player = threat.pos;
    let ground = |x: f32, z: f32| terrain.surface(x, z);
    let wet = |x: f32, z: f32| terrain.wet(x, z);
    // Members by flock.
    let mut by_flock: crate::util::FxHashMap<hecs::Entity, Vec<(Vec3, Vec3, bool)>> = Default::default();
    for (fl, look) in world.query::<(&Flyer, &Look)>().iter() {
        by_flock.entry(fl.flock).or_default().push((look.pos, fl.vel, fl.perched));
    }
    // Flocks with no members left are gone.
    let empty: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Flock)>().iter().map(|(e, _)| e).filter(|e| !by_flock.contains_key(e)).collect();
    for e in empty {
        let _ = world.despawn(e);
    }
    // What the hunters could take: every live animal of a kind some soaring
    // hunter eats, where it is and how it is moving, if it is in the open.
    let hunters: Vec<bool> = species.iter().map(|s| !s.hunts.is_empty()).collect();
    let mut quarry: Vec<(hecs::Entity, Vec3, Vec3, usize, Reach)> = Vec::new();
    if world.query::<&Flock>().iter().any(|f| f.soar && hunters[f.species]) {
        let eaten: Vec<bool> = (0..species.len()).map(|k| species.iter().any(|s| s.hunts.contains(&k))).collect();
        for (e, c, l, age, held) in world.query::<(hecs::Entity, &Creature, &Look, Option<&super::age::Age>, Option<&super::carry::Carried>)>().iter() {
            if !eaten[c.species] || held.is_some() || age.is_some_and(|a| a.dead()) {
                continue;
            }
            if let Some(r) = reach(l.pos, terrain) {
                quarry.push((e, l.pos, (l.pos - l.prev_pos) / dt.max(1e-3), c.species, r));
            }
        }
    }
    let quarry_at = |e: hecs::Entity| quarry.iter().find(|q| q.0 == e);
    // Flock decisions: wander, land, flush, hunt.
    let mut flushed: Vec<hecs::Entity> = Vec::new();
    for (fe, f) in world.query_mut::<(hecs::Entity, &mut Flock)>() {
        let Some(m) = by_flock.get(&fe) else { continue };
        let sp = &species[f.species];
        let n = m.len() as f32;
        let centre = m.iter().fold(Vec3::ZERO, |a, x| a + x.0) / n;
        // The whole flock bursts together, so its flightiness moves the
        // radius rather than building alarm in each bird.
        let nerve = super::startle::Nerve::of(sp, None, super::age::Stage::Adult);
        let near_player = m.iter().any(|x| (x.0 - player).length() < super::startle::reach(&nerve, threat, x.0, 6.0));
        f.timer -= dt;
        f.phase += dt * if f.mode == Mode::Soar || f.glide { 0.12 } else { 0.18 };
        f.full -= dt;
        f.hold = (f.hold - dt).max(0.0);
        f.look_t -= dt;
        match f.mode {
            Mode::Soar => {
                // Wide, slow circles round a home that drifts.
                let r = 45.0;
                f.goal = f.home + Vec3::new(f.phase.cos() * r, 0.0, f.phase.sin() * r);
                if hunters[f.species] && f.full <= 0.0 && f.hold <= 0.0 && f.look_t <= 0.0 && !near_player {
                    f.look_t = 3.0 + f.rand() * 3.0;
                    // The nearest thing it eats in reach and below it.
                    let mut best: Option<(f32, hecs::Entity)> = None;
                    for q in &quarry {
                        let flat = Vec3::new(q.1.x - centre.x, 0.0, q.1.z - centre.z).length();
                        if !sp.hunts.contains(&q.3) || flat > STOOP_REACH_M || q.1.y > centre.y - 4.0 {
                            continue;
                        }
                        if best.is_none_or(|b| flat < b.0) {
                            best = Some((flat, q.0));
                        }
                    }
                    if let Some((_, e)) = best {
                        f.prey = Some(e);
                        f.mode = Mode::Stoop;
                        f.timer = STOOP_S;
                    }
                }
                if f.mode == Mode::Soar && f.timer <= 0.0 {
                    // Now and then down onto the top of the tallest of a few
                    // tall trees in reach, never with the player close.
                    // With a kill it always looks for a tree to eat it in.
                    let mut best: Option<(f32, Vec3)> = None;
                    if (f.hold > 0.0 || f.rand() < 0.4) && !near_player {
                        for t in flora.near(centre.x, centre.z, PERCH_REACH_M, Some(super::affordance::Affordance::Perch)) {
                            if t.height < PERCH_TREE_M {
                                continue;
                            }
                            let score = t.height + f.rng.unit() * 8.0;
                            if best.is_none_or(|b| score > b.0) {
                                // The tree's own top as the voxels stand, or
                                // its grown height where they are not built.
                                let top = ground(t.pos.x, t.pos.z).filter(|y| *y > t.pos.y + t.height * 0.5).unwrap_or(t.pos.y + t.height);
                                best = Some((score, Vec3::new(t.pos.x, top, t.pos.z)));
                            }
                        }
                    }
                    if let Some((_, top)) = best {
                        if std::env::var_os("VG_CATCH_M").is_some() {
                            eprintln!("VG_CATCH: to a tree top at {:.0},{:.0},{:.0}{}", top.x, top.y, top.z, if f.hold > 0.0 { " with a kill" } else { "" });
                        }
                        f.goal = top;
                        f.mode = Mode::Land;
                        f.timer = 50.0;
                    } else {
                        let (dx, dz) = (f.rand() - 0.5, f.rand() - 0.5);
                        f.home += Vec3::new(dx, 0.0, dz) * 60.0;
                        f.timer = 40.0 + f.rand() * 40.0;
                    }
                }
            }
            Mode::Stoop => {
                // Dive at where the prey will be by the time it gets there.
                match f.prey.and_then(quarry_at) {
                    Some(q) if f.timer > 0.0 => {
                        let eta = (q.1 - centre).length() / (sp.beh.flee_speed.max(4.0) * 2.0);
                        let lead = q.2 * eta.min(2.0);
                        f.goal = q.1 + Vec3::new(lead.x, lead.y.max(0.0), lead.z);
                        if q.4 == Reach::Fish {
                            // It takes a fish at the surface, not under it.
                            f.goal.y = terrain.sea;
                        }
                    }
                    _ => {
                        // Gone, out of the open, or taken too long.
                        f.prey = None;
                        f.mode = Mode::Soar;
                        f.full = 10.0 + f.rand() * 20.0;
                        f.home = centre;
                        f.timer = 20.0 + f.rand() * 20.0;
                    }
                }
            }
            Mode::Fly => {
                let r = if f.glide { 60.0 } else { 12.0 + n.sqrt() * 2.0 };
                f.goal = f.home + Vec3::new(f.phase.cos() * r, 0.0, (f.phase * 1.3).sin() * r * 0.7);
                if f.timer <= 0.0 {
                    // Gliders keep the air like vultures and come down seldom.
                    let lands = if f.glide { 0.1 } else { 0.5 };
                    if f.rand() < lands && !near_player {
                        // Come down somewhere near; gliders on the highest
                        // dry ground of a few looks round within sight of the
                        // sea, a ridge, a rock or a bone, but never a tree.
                        let tries = if f.glide { 10 } else { 1 };
                        let mut best: Option<Vec3> = None;
                        for _ in 0..tries {
                            let a = f.rand() * std::f32::consts::TAU;
                            let reach = if f.glide { 15.0 + f.rand() * 45.0 } else { 10.0 + f.rand() * 20.0 };
                            let p = centre + Vec3::new(a.cos(), 0.0, a.sin()) * reach;
                            let leafy = f.glide && terrain.ground(p.x, p.z, terrain.store.top_m()).is_some_and(|g| crate::mesh::sprites::leafy(g.1));
                            let inland = f.glide && terrain.coast(p.x, p.z).is_some_and(|d| d > 40.0);
                            if leafy || inland {
                                continue;
                            }
                            if let (Some(gy), false) = (ground(p.x, p.z), wet(p.x, p.z)) {
                                if best.is_none_or(|b| gy > b.y) {
                                    best = Some(Vec3::new(p.x, gy, p.z));
                                }
                            }
                        }
                        if let Some(goal) = best {
                            f.goal = goal;
                            f.mode = Mode::Land;
                            f.timer = 20.0;
                        }
                    } else {
                        let (dx, dz) = (f.rand() - 0.5, f.rand() - 0.5);
                        f.home = centre + Vec3::new(dx, 0.0, dz) * if f.glide { 160.0 } else { 50.0 };
                        f.timer = 15.0 + f.rand() * 25.0;
                    }
                }
            }
            Mode::Land => {
                if m.iter().all(|x| x.2) {
                    if f.soar && std::env::var_os("VG_CATCH_M").is_some() {
                        eprintln!("VG_CATCH: perched at {:.0},{:.0},{:.0}", m[0].0.x, m[0].0.y, m[0].0.z);
                    }
                    f.mode = Mode::Perch;
                    f.timer = if f.soar {
                        45.0 + f.rand() * 90.0
                    } else if f.glide {
                        30.0 + f.rand() * 70.0
                    } else {
                        12.0 + f.rand() * 30.0
                    };
                } else if f.timer <= 0.0 || near_player {
                    flushed.push(fe);
                }
            }
            Mode::Perch => {
                if f.timer <= 0.0 || near_player {
                    flushed.push(fe);
                }
            }
        }
        if flushed.last() == Some(&fe) {
            // Off and away from the player, then wheel round somewhere new.
            let away = Vec3::new(centre.x - player.x, 0.0, centre.z - player.z).normalize_or_zero();
            f.home = centre + away * 40.0;
            f.goal = f.home;
            f.mode = if f.soar { Mode::Soar } else { Mode::Fly };
            f.timer = 20.0 + f.rand() * 25.0;
        }
    }
    let flocks: std::collections::HashMap<hecs::Entity, (Mode, Vec3, usize, bool, Option<hecs::Entity>)> =
        world.query::<(hecs::Entity, &Flock)>().iter().map(|(e, f)| (e, (f.mode, f.goal, f.species, f.soar, f.prey))).collect();
    // Strikes this step: (flock, hunter, prey, how the prey was).
    let mut strikes: Vec<(hecs::Entity, hecs::Entity, hecs::Entity, Reach)> = Vec::new();
    // Members.
    for (me, fl, look, morph, show) in world.query_mut::<(hecs::Entity, &mut Flyer, &mut Look, Option<&crate::variety::Morph>, Option<&super::display::Show>)>() {
        look.begin_step();
        let (pace, _) = crate::variety::pace(morph);
        let Some(&(mode, goal, s, soarer, prey)) = flocks.get(&fl.flock) else {
            continue;
        };
        let sp = &species[s];
        let g = &groups[s];
        let cruise = sp.beh.walk_speed.max(2.0) * pace;
        // In a display: where it says, its velocity kept from the move so
        // it flies on smoothly when handed back.
        if let Some(sh) = show {
            match sh.out.drive {
                super::display::Drive::Place { pos, yaw } => {
                    fl.vel = (pos - look.pos) / dt.max(1e-4);
                    (look.pos, look.yaw, fl.perched) = (pos, yaw, false);
                }
                super::display::Drive::Sit { pos, yaw } => {
                    fl.vel = Vec3::ZERO;
                    (look.pos, look.yaw, fl.perched) = (pos, yaw, true);
                }
                _ => {}
            }
            fl.shown = true;
            continue;
        }
        if std::mem::take(&mut fl.shown) {
            // Up and off with the rest, at its cruising pace.
            fl.perched = false;
            let ahead = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
            if fl.vel.length() < cruise * 0.6 {
                fl.vel = ahead * cruise + Vec3::Y * 1.5;
            }
        }
        let top = sp.beh.flee_speed.max(cruise * 1.2) * pace * if mode == Mode::Stoop { 2.0 } else { 1.0 };
        let p = look.pos;
        if flushed.contains(&fl.flock) && fl.perched {
            fl.perched = false;
            let away = Vec3::new(p.x - player.x, 0.0, p.z - player.z).normalize_or_zero();
            let h = crate::world::noise::hash3(p.x as i32 * 13, p.z as i32 * 7, (p.y * 10.0) as i32, 5);
            let jitter = Vec3::new(crate::world::noise::unit(h) - 0.5, 0.0, crate::world::noise::unit(h >> 8) - 0.5);
            fl.vel = (away + jitter + Vec3::Y * 1.2).normalize_or_zero() * top;
        }
        if fl.perched {
            continue;
        }
        let members = by_flock.get(&fl.flock).map_or(&[][..], |v| v.as_slice());
        let others = members.len().saturating_sub(1);
        let floor = ground(p.x, p.z);
        let mut acc = steer(p, fl.vel, members.iter().map(|x| (x.0, x.1)), g);
        let to_goal = goal - p;
        match mode {
            Mode::Stoop => {
                // Straight at it, as fast as it can go, and the talons out at
                // the end. A fish is lifted from just under the surface.
                acc += (to_goal.normalize_or_zero() * top - fl.vel) * 3.0;
                if let Some(q) = prey.and_then(quarry_at) {
                    let at = if q.4 == Reach::Fish { Vec3::new(q.1.x, q.1.y.max(terrain.sea - 0.2), q.1.z) } else { q.1 };
                    if (at - p).length() < STRIKE_M {
                        strikes.push((fl.flock, me, q.0, q.4));
                    }
                }
            }
            Mode::Land => {
                // Glide down onto the spot, slowing as it nears.
                let want = to_goal.normalize_or_zero() * (cruise * (to_goal.length() / 12.0).clamp(0.3, 1.0));
                acc += (want - fl.vel) * 1.5;
                if soarer {
                    // A tree top is a small mark. Over the last stretch it
                    // flies straight in, slowing all the way, and settles on
                    // the top itself rather than the flank of the crown.
                    let d = to_goal.length();
                    if d < 1.0 {
                        fl.perched = true;
                        fl.vel = Vec3::ZERO;
                        look.pos = goal;
                        continue;
                    }
                    if d < 20.0 {
                        // From above: over the top first, then down onto it.
                        let flat = Vec3::new(to_goal.x, 0.0, to_goal.z).length();
                        let aim = if flat > 1.5 { to_goal + Vec3::Y * 3.0 } else { to_goal };
                        let da = aim.length().max(1e-3);
                        fl.vel = aim / da * (d * 0.7).clamp(1.2, cruise);
                        acc = Vec3::ZERO;
                    }
                }
                let reach = if soarer { 0.0 } else { 6.0 + others as f32 * 0.15 };
                if let Some(gy) = floor {
                    if p.y - gy < 0.35 && Vec3::new(to_goal.x, 0.0, to_goal.z).length() < reach && (soarer || !wet(p.x, p.z)) {
                        fl.perched = true;
                        fl.vel = Vec3::ZERO;
                        look.pos.y = gy;
                        continue;
                    }
                }
            }
            _ => {
                let flat = Vec3::new(to_goal.x, 0.0, to_goal.z);
                acc += (flat.normalize_or_zero() * cruise - Vec3::new(fl.vel.x, 0.0, fl.vel.z)) * 0.5;
                if let Some(gy) = floor {
                    let want_y = gy + sp.beh.fly_height.max(3.0);
                    acc.y += (want_y - p.y) * 0.6 - fl.vel.y * 0.8;
                }
            }
        }
        if let Some(gy) = floor {
            if p.y < gy + 1.5 && mode != Mode::Land && mode != Mode::Stoop {
                acc.y += 25.0;
            }
        }
        fl.vel += acc * dt;
        let sp_len = fl.vel.length();
        let lo = if mode == Mode::Land { 0.5 } else { cruise * 0.6 };
        if sp_len > top {
            fl.vel *= top / sp_len;
        } else if sp_len < lo && sp_len > 1e-4 {
            fl.vel *= lo / sp_len;
        }
        look.pos += fl.vel * dt;
        if let Some(gy) = floor {
            look.pos.y = look.pos.y.max(gy + 0.05);
        }
        let flat = Vec3::new(fl.vel.x, 0.0, fl.vel.z);
        if flat.length() > 0.3 {
            let want = flat.x.atan2(flat.z) + std::f32::consts::PI;
            look.yaw = crate::util::turn_toward(look.yaw, want, (dt * 6.0).min(1.0));
        }
    }
    // Each strike takes its prey or misses; either way the stoop is over.
    let mut kills = Vec::new();
    for (fe, hunter, victim, how) in strikes {
        let Ok(mut f) = world.get::<&mut Flock>(fe) else {
            continue;
        };
        if f.mode != Mode::Stoop {
            continue;
        }
        let chance = match how {
            Reach::Fish => TAKE_FISH,
            Reach::Sitting => TAKE_SITTING,
            Reach::Flying => TAKE_FLYING,
        };
        let took = f.rand() < chance;
        f.prey = None;
        f.mode = Mode::Soar;
        f.home = goal_after_strike(&f);
        if took {
            kills.push((hunter, victim));
            f.full = super::carry::FULL_S;
            f.hold = super::carry::CARRY_S * 2.0;
            // Off to a tree with it soon.
            f.timer = 2.0;
        } else {
            f.full = 8.0 + f.rand() * 20.0;
            f.timer = 20.0 + f.rand() * 20.0;
        }
        if std::env::var_os("VG_CATCH_M").is_some() {
            let at = world.get::<&Look>(hunter).map(|l| l.pos).unwrap_or_default();
            eprintln!("VG_CATCH: stoop on a {how:?} {} at {:.0},{:.0},{:.0}", if took { "took it" } else { "missed" }, at.x, at.y, at.z);
        }
    }
    kills
}

/// Where a hunter wheels round after a strike: about where it struck.
fn goal_after_strike(f: &Flock) -> Vec3 {
    Vec3::new(f.goal.x, f.home.y, f.goal.z)
}

/// How a hunter would take an animal, or None if it cannot: under trees or a
/// roof, or a fish too deep.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Reach {
    Flying,
    Sitting,
    Fish,
}

fn reach(p: Vec3, t: &Terrain) -> Option<Reach> {
    // Anything solid over it (leaves, a roof) hides it from above.
    let over = t.ground(p.x, p.z, t.store.top_m()).map(|g| g.0);
    if over.is_some_and(|y| y > p.y + 0.8 && y > t.sea + 0.3) {
        return None;
    }
    if p.y < t.sea - 0.15 {
        return (p.y > t.sea - FISH_DEPTH_M && t.wet(p.x, p.z)).then_some(Reach::Fish);
    }
    let under = over.unwrap_or(p.y).max(t.sea);
    Some(if p.y > under + 1.5 { Reach::Flying } else { Reach::Sitting })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_too_close_push_apart_and_far_ones_pull_in() {
        let g = Group {
            spacing: 1.0,
            range: 10.0,
            ..Default::default()
        };
        let close = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(0.5, 0.0, 0.0), Vec3::ZERO)], &g);
        assert!(close.x < 0.0, "{close}");
        let far = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(6.0, 0.0, 0.0), Vec3::ZERO)], &g);
        assert!(far.x > 0.0, "{far}");
        let out_of_range = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(20.0, 0.0, 0.0), Vec3::ZERO)], &g);
        assert_eq!(out_of_range, Vec3::ZERO);
    }

    #[test]
    fn members_turn_to_fly_with_their_neighbours() {
        let g = Group::default();
        let v = Vec3::new(5.0, 0.0, 0.0);
        let others = [(Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 5.0)), (Vec3::new(-2.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 5.0))];
        let a = steer(Vec3::ZERO, v, others, &g);
        assert!(a.z > 0.0 && a.x < 0.0, "{a}");
    }
}
