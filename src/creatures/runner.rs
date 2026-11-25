//! Running birds: small birds that live on their feet and fly only when they
//! have to, low over the ground. Four verbs share this mover
//! (locomotion.rs `Mover::Run`):
//!
//! - `scurry` (sanderlings): a flock on a beach that keeps to the edge of
//!   the surf, running up the sand ahead of each wave as it comes in and
//!   chasing the water back down as it drains, probing the wet sand it
//!   leaves. Each bird reads where the swash will be a moment from now
//!   (`Terrain::water_edge`, the same sheet of water the terrain shader
//!   draws) and runs for the point just above it, drifting along the beach
//!   as it goes. Flushed, the whole flock swings out low over the surf and
//!   comes back in to the water's edge farther along the beach; now and then
//!   it does so unprompted.
//! - `patter` (plovers): run a few metres, stop dead, peck, run again, on
//!   open sand or bare ground, out of the water; a loose flock that flushes
//!   together and lands a little way off.
//! - `dash` (roadrunners): alone, long runs across open ground at a trot or
//!   a sprint, now and then a hop up onto a rock or a bush top to look
//!   round. With `hunts` it runs down what it eats (lizards) and carries it
//!   off in its bill (`carry::Beak`). Startled, it mostly sprints off on
//!   foot, and sometimes flies a short way low.
//! - `covey` (quail): a band feeding together in run-stop-peck, now and
//!   then one up on a bush top. Pressed, the whole covey runs off on foot,
//!   heads down; only when you are on top of it does it burst up all at once
//!   on whirring wings, scatter low over the ground and come down 20 to 45 m
//!   off.
//!
//! A display (creatures/display: a killdeer's broken wing, a roadrunner
//! courting) takes a bird over for a while: it carries a `Show` and is put
//! where the display says (`Sit` on its feet, `Place` in the air), and picks
//! up on the ground where it was left.
//!
//! No physics body: a runner stands on the ground as it is drawn
//! (`Terrain::drawn`) and refuses a step up a rock, into a trunk or into
//! water. Wings and legs are animated by birds.rs like any other bird: on the
//! ground the wings are folded and the legs stride with the pace.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::util::FxHashMap as HashMap;
use crate::world::noise::{hash32, Rng};
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// What sort of running bird (the verb).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Keeps to the edge of the surf.
    Surf,
    /// Run, stop, peck.
    Patter,
    /// Long runs alone, hunting.
    Dash,
    /// A covey that runs from you, and bursts up only when you are close.
    Covey,
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    /// On its feet: running to `goal`, or standing and pecking.
    Ground,
    /// On a rock or bush top, looking round.
    Perch { timer: f32 },
    /// In the air, on a curve from `from` through `via` to `to`.
    Fly { from: Vec3, via: Vec3, to: Vec3, t: f32, dur: f32, perch: bool },
}

pub struct Runner {
    pub state: State,
    pub style: Style,
    /// The flock it belongs to: one band flies together.
    pub band: u32,
    home: Vec3,
    goal: Vec3,
    /// Surf: seconds until it reads the water's edge again. Otherwise:
    /// seconds it stands before the next run.
    timer: f32,
    /// Running flat out (out of the water's way, after prey, from you).
    hurry: bool,
    /// Surf: how far up the sand from the water's edge it likes to be, metres.
    lane: f32,
    /// Surf: which way along the beach it drifts (+1 or -1), and for how
    /// much longer.
    along: f32,
    along_timer: f32,
    /// Surf: the way down the beach to the sea, smoothed (unit, flat).
    down: Vec3,
    /// Seconds before it hunts again (dash).
    pub fed: f32,
    /// In a display last step.
    shown: bool,
    /// Seconds it has been blocked on the way to its goal.
    stuck: f32,
    rng: Rng,
}

impl Runner {
    /// Standing or running on the ground or sitting on a top: wings folded.
    pub fn grounded(&self) -> bool {
        !matches!(self.state, State::Fly { .. })
    }
    /// The last part of a flight: wings flared, feet forward.
    pub fn landing(&self) -> bool {
        matches!(self.state, State::Fly { t, .. } if t > 0.8)
    }
    /// For the census.
    pub fn doing(&self, moving: bool) -> &'static str {
        match self.state {
            State::Fly { .. } => "flying",
            State::Perch { .. } => "on a top",
            State::Ground if moving && self.hurry => "sprinting",
            State::Ground if moving => "running",
            State::Ground => "standing",
        }
    }
    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }
}

/// How far ahead a sanderling reads the swash, seconds: it starts up the
/// beach as the wave breaks, not when the water reaches its feet.
const LOOKAHEAD: f32 = 0.5;
/// How far up and down the beach from where it stands a surf bird looks for
/// the water's edge, and how finely, metres. A wide flat beach has a run-up
/// of twenty metres and more.
const SEARCH: f32 = 18.0;
const SEARCH_STEP: f32 = 0.5;

/// The way down the beach toward the sea at (x, z) (`Terrain::seaward`).
fn seaward(t: &Terrain, x: f32, z: f32) -> Option<Vec3> {
    t.seaward(x, z)
}

/// Where it can put its feet at (x, z) near height `y`: the drawn ground,
/// unless that is a step up onto a rock, into a trunk, or over water
/// (`dry`: also not under the swash, `wade`: a little water is fine).
fn footing(t: &Terrain, x: f32, z: f32, y: f32, wade: bool) -> Option<f32> {
    let (top, _) = t.ground(x, z, y + 0.6)?;
    let g = top + crate::physics::drawn_ground_lift(t.store, Vec3::new(x, top, z));
    if g - y > 0.35 || g - y < -1.2 || t.height(x, z).is_some_and(|h| g > h + 1.2) {
        return None;
    }
    // Something standing on the ground there: a rock, a log, a trunk. (The
    // drawn ground can lie below the voxel top, so ask above the voxel.)
    if t.in_rock(Vec3::new(x, top + 0.12, z)) || t.inland_water(x, z).is_some() {
        return None;
    }
    let edge = t.water_edge(x, z, 0.0);
    let floor = if wade { t.sea_at(x, z) - 0.03 } else { edge + 0.03 };
    (g > floor).then_some(g)
}

/// Is the straight run from `a` to `b` clear, sampled every half metre?
fn clear_run(t: &Terrain, a: Vec3, b: Vec3, wade: bool) -> Option<f32> {
    let d = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
    let n = (d.length() / 0.5).ceil().max(1.0) as i32;
    let mut y = a.y;
    for i in 1..=n {
        let p = a + d * (i as f32 / n as f32);
        y = footing(t, p.x, p.z, y, wade)?;
    }
    Some(y)
}

/// The point `lane` metres up the sand from the water's edge on the line
/// down the beach through `at`: the edge is where the ground first dips
/// under the swash (read `LOOKAHEAD` ahead), searched from the land side.
fn edge_spot(t: &Terrain, at: Vec3, down: Vec3, lane: f32) -> Option<Vec3> {
    let n = (SEARCH / SEARCH_STEP) as i32;
    let mut last_dry: Option<(Vec3, f32)> = None;
    let mut y = at.y + 2.0;
    for i in -n..=n {
        let p = at + down * (i as f32 * SEARCH_STEP);
        let Some(g) = t.drawn(p.x, p.z, y + 0.6) else {
            continue;
        };
        // Something standing on the beach (a log, a rock, a giant's
        // tooth): not the beach. Reading under the last height inside a
        // solid column would climb it a step at a time.
        if g - y > 0.5 || t.height(p.x, p.z).is_some_and(|h| g > h + 0.8) {
            continue;
        }
        y = g;
        let over = g - t.water_edge(p.x, p.z, LOOKAHEAD);
        if over < 0.0 {
            // The first sample in the water: the edge lies between it and the
            // last dry one, and the bird stands a little up from it.
            let edge = match last_dry {
                Some((q, o)) => q.lerp(Vec3::new(p.x, g, p.z), (o / (o - over)).clamp(0.0, 1.0)),
                None => Vec3::new(p.x, g, p.z),
            };
            let back = edge - down * lane;
            let y = t.drawn(back.x, back.z, edge.y + 1.0).unwrap_or(edge.y);
            return Some(Vec3::new(back.x, y, back.z));
        }
        last_dry = Some((Vec3::new(p.x, g, p.z), over));
    }
    // Dry all the way down: go down to the water.
    last_dry.map(|(p, _)| p)
}

/// A point `dist` metres along the beach from `p` (`sign` picks the way),
/// keeping the same distance from the coast; None off the beach.
fn along_shore(t: &Terrain, p: Vec3, dist: f32, sign: f32) -> Option<Vec3> {
    let c0 = t.coast(p.x, p.z)?;
    let mut q = p;
    for _ in 0..(dist / 4.0).ceil() as i32 {
        let down = seaward(t, q.x, q.z)?;
        q += Vec3::new(down.z, 0.0, -down.x) * sign * 4.0;
        let c = t.coast(q.x, q.z)?;
        q += down * (c - c0).clamp(-4.0, 4.0);
    }
    let g = t.drawn(q.x, q.z, t.height(q.x, q.z).unwrap_or(q.y) + 3.0)?;
    let c = t.coast(q.x, q.z)?;
    // Still on the beach: about as far from the coast as it started (the
    // tide can put the water's edge well seaward of the high-water coast),
    // low, and dry.
    let ok = (c - c0).abs() < 10.0 && g < t.sea + 4.0 && g > t.sea_at(q.x, q.z) - 0.05;
    ok.then_some(Vec3::new(q.x, g, q.z))
}

/// A spot for a plover or a roadrunner to run to, `rmin..rmax` away,
/// toward `toward` if given: a clear run over dry ground.
fn run_spot(r: &mut Runner, t: &Terrain, at: Vec3, toward: Option<Vec3>, rmin: f32, rmax: f32) -> Option<Vec3> {
    for _ in 0..8 {
        let a = match toward {
            Some(d) => d.x.atan2(d.z) + (r.rand() - 0.5) * 1.4,
            None => r.rand() * TAU,
        };
        let d = rmin + r.rand() * (rmax - rmin);
        let to = Vec3::new(at.x + a.sin() * d, at.y, at.z + a.cos() * d);
        if let Some(y) = clear_run(t, at, to, false) {
            return Some(Vec3::new(to.x, y, to.z));
        }
    }
    None
}

/// A rock or bush top a roadrunner can hop up onto, within `r` m: standing
/// 0.5 to 2.5 m over the ground round it.
fn top_spot(rn: &mut Runner, t: &Terrain, at: Vec3, r: f32) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..10 {
        let a = rn.rand() * TAU;
        let d = 2.0 + rn.rand() * (r - 2.0);
        let (x, z) = (at.x + a.sin() * d, at.z + a.cos() * d);
        let Some(y) = t.perch(x, z, at.y + 5.0) else { continue };
        let rise = y - t.height(x, z).unwrap_or(y);
        if !(0.5..2.5).contains(&rise) || t.wet(x, z) {
            continue;
        }
        let score = rise + rn.rand();
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, Vec3::new(x, y, z)));
        }
    }
    best.map(|b| b.1)
}

/// `n` birds of species `s` at ground point `at`, as one band. False if the
/// spot won't do: a surf bird wants a beach with the sea's edge on it.
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, style: Style, t: &Terrain, at: Vec3, n: u32, seed: u32) -> bool {
    let mut rng = Rng::new(seed | 1);
    let spacing = sp.beh.group.as_ref().map_or(1.0, |g| g.spacing);
    let (centre, down) = match style {
        Style::Surf => {
            let Some(c) = t.coast(at.x, at.z) else { return false };
            // From well up the beach to the low-water mark.
            if !(-80.0..45.0).contains(&c) {
                return false;
            }
            let Some(down) = seaward(t, at.x, at.z) else { return false };
            // A beach, not a cliff: the ground falls gently to the sea.
            let (Some(h0), Some(h1)) = (t.height(at.x, at.z), t.height(at.x + down.x * 4.0, at.z + down.z * 4.0)) else {
                return false;
            };
            if (h0 - h1).abs() > 1.2 || h0 > t.sea + 3.5 {
                return false;
            }
            let start = Vec3::new(at.x, t.drawn(at.x, at.z, h0 + 3.0).unwrap_or(h0), at.z);
            // A wide beach at low water can put the edge farther off than
            // one search reaches: search again from where the last one ended.
            let mut p = start;
            for _ in 0..4 {
                let Some(q) = edge_spot(t, p, down, 0.5) else { return false };
                let moved = (q - p).with_y(0.0).length();
                p = q;
                if moved < SEARCH - 1.0 {
                    break;
                }
            }
            (p, down)
        }
        _ => {
            let Some(g) = t.drawn(at.x, at.z, t.store.top_m()) else { return false };
            if g < t.sea + 0.3 || t.wet(at.x, at.z) && style == Style::Dash {
                return false;
            }
            let p = Vec3::new(at.x, g, at.z);
            if footing(t, p.x, p.z, g, false).is_none() {
                return false;
            }
            (p, Vec3::X)
        }
    };
    let band = hash32(seed ^ 0xb4d);
    // The whole band drifts the same way along the beach.
    let along = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
    let along_timer = 10.0 + rng.unit() * 30.0;
    for k in 0..n.max(1) {
        let mut r = Runner {
            state: State::Ground,
            style,
            band,
            home: centre,
            goal: centre,
            timer: rng.unit() * 2.0,
            hurry: false,
            lane: 0.15 + rng.unit() * 0.9,
            along,
            along_timer,
            down,
            fed: 0.0,
            shown: false,
            stuck: 0.0,
            rng: Rng::new(hash32(seed ^ k.wrapping_mul(0x9e37_79b9)) | 1),
        };
        // Strung out along the beach, or scattered round the spot.
        let off = match style {
            Style::Surf => Vec3::new(down.z, 0.0, -down.x) * (r.rand() - 0.5) * spacing * (n as f32).sqrt() * 2.5 + down * (r.rand() - 0.5) * 1.5,
            _ => Vec3::new(r.rand() - 0.5, 0.0, r.rand() - 0.5) * spacing * (n as f32).sqrt() * 2.0,
        };
        let p = centre + off;
        let y = t.drawn(p.x, p.z, centre.y + 1.5).unwrap_or(centre.y);
        let pos = Vec3::new(p.x, y, p.z);
        r.goal = pos;
        let yaw = r.rand() * TAU;
        world.spawn((r, Creature::of(s), Look::new(sp.model, pos, yaw)));
    }
    true
}

/// Dev hook VG_RUNNER, read once: 0 off, 1 a surf bird's reading of the
/// beach now and then, 2 (`track`) one surf bird's every step.
fn debug() -> u8 {
    static ON: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *ON.get_or_init(|| match std::env::var("VG_RUNNER").as_deref() {
        Ok("track") => 2,
        Ok(_) => 1,
        Err(_) => 0,
    })
}

/// For VG_RUNNER=track: the first surf bird asked is the one followed.
fn tracked(id: u32) -> bool {
    use std::sync::atomic::{AtomicU32, Ordering};
    static ONE: AtomicU32 = AtomicU32::new(u32::MAX);
    let _ = ONE.compare_exchange(u32::MAX, id, Ordering::Relaxed, Ordering::Relaxed);
    ONE.load(Ordering::Relaxed) == id
}

/// A band's flight, decided once for all of it.
struct Plan {
    /// Where the band's middle lands.
    to: Vec3,
    /// The band's middle now.
    centre: Vec3,
    /// Out over the water (surf), or straight (others).
    out: Vec3,
    /// Hops up onto a top (a roadrunner alone).
    perch: bool,
    /// How much of its place in the band each bird keeps: 0.8 for a flock
    /// that holds together, more for a covey that scatters as it goes up.
    spread: f32,
}

/// Step every running bird. Prey caught this step goes on `kills`
/// (hunter, prey), and a hunter that caught something on `fed`.
pub fn update(world: &mut hecs::World, species: &[Species], dt: f32, threat: &super::startle::Threat, t: &Terrain, kills: &mut Vec<(hecs::Entity, hecs::Entity)>) {
    let player = threat.pos;
    // Where each band is and whether it is going up, before anyone moves.
    let mut bands: HashMap<u32, (Vec3, u32, bool, Style, f32)> = HashMap::default();
    // The way each band drifts along the beach: its first member's.
    let mut drift: HashMap<u32, f32> = HashMap::default();
    let mut members: Vec<(u32, Vec3)> = Vec::new();
    // Coveys being pressed, and the way away from you: they run first.
    let mut pressed: HashMap<u32, Vec3> = HashMap::default();
    for (r, look, me, morph) in world.query_mut::<(&mut Runner, &Look, &Creature, Option<&crate::variety::Morph>)>() {
        let sp = &species[me.species];
        let nerve = super::startle::Nerve::of(sp, morph, super::age::Stage::Adult);
        let reach = super::startle::reach(&nerve, threat, look.pos, 2.0);
        let flat = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z);
        let mut near = r.grounded() && flat.length() < reach;
        // A roadrunner already sprinting off doesn't flush again.
        near &= !(r.style == Style::Dash && r.hurry);
        if r.style == Style::Covey && near {
            // A covey runs while you are at its flee distance and goes up
            // only inside two fifths of it.
            pressed.insert(r.band, flat.normalize_or_zero());
            near = flat.length() < reach * 0.4;
        }
        // Now and then a shore flock gets up for no reason you can see: one
        // roll a band, by its first member.
        let leader = !drift.contains_key(&r.band);
        let restless = leader && r.grounded() && matches!(r.style, Style::Surf | Style::Patter) && r.rand() < dt / 150.0;
        drift.entry(r.band).or_insert(r.along);
        let e = bands.entry(r.band).or_insert((Vec3::ZERO, 0, false, r.style, 0.0));
        e.0 += look.pos;
        e.1 += 1;
        if near || restless {
            e.2 = true;
            e.4 = e.4.max(if near { 1.0 } else { 0.5 });
        }
        members.push((r.band, look.pos));
    }
    let mut plans: HashMap<u32, Plan> = HashMap::default();
    for (&band, &(sum, n, up, style, why)) in &bands {
        if !up {
            continue;
        }
        let centre = sum / n.max(1) as f32;
        let away = Vec3::new(centre.x - player.x, 0.0, centre.z - player.z).normalize_or_zero();
        let mut rng = Rng::new(hash32(band ^ (centre.x * 7.0) as u32 ^ (centre.z * 13.0) as u32) | 1);
        let flushed = why >= 1.0;
        let plan = match style {
            Style::Surf => {
                let Some(down) = seaward(t, centre.x, centre.z) else { continue };
                let tan = Vec3::new(down.z, 0.0, -down.x);
                // Away from you along the beach; either way if nothing pressed it.
                let sign = if flushed && tan.dot(away).abs() > 0.1 {
                    tan.dot(away).signum()
                } else if rng.unit() < 0.5 {
                    1.0
                } else {
                    -1.0
                };
                let dist = 25.0 + rng.unit() * 45.0;
                let to = along_shore(t, centre, dist, sign).or_else(|| along_shore(t, centre, dist * 0.5, -sign)).unwrap_or(centre);
                Plan {
                    to,
                    centre,
                    out: down * (6.0 + rng.unit() * 10.0),
                    perch: false,
                    spread: 0.8,
                }
            }
            Style::Patter | Style::Dash | Style::Covey => {
                // A roadrunner runs for it more often than it flies.
                if style == Style::Dash && rng.unit() < 0.7 {
                    continue;
                }
                let dir = if flushed { away } else { Vec3::new(rng.unit() - 0.5, 0.0, rng.unit() - 0.5).normalize_or_zero() };
                let mut to = None;
                let (lo, span) = if style == Style::Covey { (20.0, 25.0) } else { (14.0, 26.0) };
                for _ in 0..8 {
                    let a = dir.x.atan2(dir.z) + (rng.unit() - 0.5) * 1.2;
                    let d = lo + rng.unit() * span;
                    let (x, z) = (centre.x + a.sin() * d, centre.z + a.cos() * d);
                    if let Some(g) = t.drawn(x, z, t.height(x, z).unwrap_or(centre.y) + 3.0) {
                        if footing(t, x, z, g, false).is_some() {
                            to = Some(Vec3::new(x, g, z));
                            break;
                        }
                    }
                }
                let Some(to) = to else { continue };
                let spread = if style == Style::Covey { 1.8 } else { 0.8 };
                Plan { to, centre, out: Vec3::ZERO, perch: false, spread }
            }
        };
        plans.insert(band, plan);
    }
    // Who it can run down: walkers of what it hunts (lizards).
    let prey: Vec<(hecs::Entity, usize, Vec3)> = world
        .query::<(hecs::Entity, &Look, &Creature, &super::walk::Critter)>()
        .iter()
        .map(|(e, l, c, _)| (e, c.species, l.pos))
        .collect();

    for (ent, r, look, me, morph, show) in world.query_mut::<(hecs::Entity, &mut Runner, &mut Look, &Creature, Option<&crate::variety::Morph>, Option<&super::display::Show>)>() {
        look.begin_step();
        if let Some(s) = show {
            if let super::display::Drive::Place { pos, yaw } | super::display::Drive::Sit { pos, yaw } = s.out.drive {
                (look.pos, look.yaw) = (pos, yaw);
            }
            r.shown = true;
            continue;
        }
        if std::mem::take(&mut r.shown) {
            // On its feet where it was left, a moment before it carries on.
            if let Some(g) = t.drawn(look.pos.x, look.pos.z, look.pos.y + 2.0) {
                look.pos.y = g;
            }
            r.state = State::Ground;
            r.home = look.pos;
            r.goal = look.pos;
            r.timer = 1.0;
        }
        let sp = &species[me.species];
        let (pace, _) = crate::variety::pace(morph);
        let walk = sp.beh.walk_speed.max(0.3) * pace;
        let fly_speed = sp.beh.flee_speed.max(walk * 2.0) * pace;
        r.fed = (r.fed - dt).max(0.0);
        if r.grounded() {
            if let Some(plan) = plans.get(&r.band) {
                // Each keeps its place in the band, a little closer up.
                let to = plan.to + (look.pos - plan.centre).with_y(0.0) * plan.spread;
                let to = Vec3::new(to.x, t.drawn(to.x, to.z, plan.to.y + 2.0).unwrap_or(plan.to.y), to.z);
                let high = sp.beh.fly_height.max(0.8) * (0.8 + 0.4 * r.rand());
                let via = (look.pos + to) * 0.5 + plan.out + Vec3::Y * high;
                let dist = (to - look.pos).length() + plan.out.length();
                r.state = State::Fly {
                    from: look.pos,
                    via,
                    to,
                    t: 0.0,
                    dur: (dist / fly_speed).clamp(0.8, 12.0),
                    perch: plan.perch,
                };
                r.home = to;
            }
        }
        match r.state {
            State::Fly { from, via, to, t: tt, dur, perch } => {
                let tt = (tt + dt / dur).min(1.0);
                let e = tt * tt * (3.0 - 2.0 * tt) * 0.5 + tt * 0.5;
                let a = from.lerp(via, e);
                let b = via.lerp(to, e);
                let p = a.lerp(b, e);
                let d = b - a;
                if d.with_y(0.0).length() > 0.01 {
                    look.yaw = d.x.atan2(d.z) + PI;
                }
                // Never through the ground on the way.
                let floor = t.drawn(p.x, p.z, p.y + 1.0).unwrap_or(p.y);
                look.pos = Vec3::new(p.x, p.y.max(floor + 0.15 * (PI * tt).sin()), p.z);
                if tt >= 1.0 {
                    look.pos = to;
                    r.goal = to;
                    r.hurry = false;
                    r.timer = 0.5 + r.rand();
                    r.state = if perch { State::Perch { timer: 4.0 + r.rand() * 8.0 } } else { State::Ground };
                } else {
                    r.state = State::Fly { from, via, to, t: tt, dur, perch };
                }
                continue;
            }
            State::Perch { timer } => {
                let timer = timer - dt;
                let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < sp.beh.wary;
                if timer <= 0.0 || near {
                    // Down again, a hop off the far side.
                    let away = if near { Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero() } else { Vec3::ZERO };
                    let to = run_spot(r, t, look.pos, near.then_some(away), 2.0, 4.0).unwrap_or(look.pos);
                    let to = Vec3::new(to.x, t.height(to.x, to.z).map_or(to.y, |h| t.drawn(to.x, to.z, h + 0.5).unwrap_or(h)), to.z);
                    r.state = State::Fly {
                        from: look.pos,
                        via: (look.pos + to) * 0.5 + Vec3::Y * 0.6,
                        to,
                        t: 0.0,
                        dur: 0.7,
                        perch: false,
                    };
                } else {
                    r.state = State::Perch { timer };
                    if r.rand() < dt * 0.5 {
                        look.yaw += (r.rand() - 0.5) * 2.0;
                    }
                }
                continue;
            }
            State::Ground => {}
        }

        // On the ground: where to run.
        let mut speed = walk;
        match r.style {
            Style::Surf => {
                r.timer -= dt;
                r.along_timer -= dt;
                let band_way = drift.get(&r.band).copied().unwrap_or(r.along);
                if band_way != r.along && r.along_timer > 0.0 {
                    r.along = band_way;
                } else if r.along_timer <= 0.0 {
                    r.along = -r.along;
                    r.along_timer = 15.0 + r.rand() * 40.0;
                }
                if r.timer <= 0.0 {
                    r.timer = 0.08 + r.rand() * 0.08;
                    if let Some(d) = seaward(t, look.pos.x, look.pos.z) {
                        r.down = (r.down * 0.7 + d * 0.3).normalize_or(d);
                    }
                    let tan = Vec3::new(r.down.z, 0.0, -r.down.x);
                    // Drift along the beach, back toward the band, and out of
                    // a neighbour's way.
                    let mut push = tan * r.along * 0.35;
                    let (mut sum, mut n) = (Vec3::ZERO, 0);
                    let spacing = sp.beh.group.as_ref().map_or(0.8, |g| g.spacing);
                    for &(b, p) in &members {
                        if b != r.band {
                            continue;
                        }
                        sum += p;
                        n += 1;
                        let d = (look.pos - p).with_y(0.0);
                        let l = d.length();
                        if l > 0.01 && l < spacing {
                            push += d / l * (spacing - l) * 2.0;
                        }
                    }
                    if n > 1 {
                        let mid = sum / n as f32;
                        let off = (mid - look.pos).dot(tan);
                        if off.abs() > 10.0 {
                            push += tan * off.signum() * 0.6;
                        }
                    }
                    let base = look.pos + push.dot(tan) * tan;
                    if let Some(p) = edge_spot(t, base, r.down, r.lane) {
                        r.goal = p;
                    }
                    // Water at its feet, or the wave coming: run for it.
                    let wet = look.pos.y < t.water_edge(look.pos.x, look.pos.z, LOOKAHEAD) + 0.01;
                    r.hurry = wet || (r.goal - look.pos).with_y(0.0).length() > 2.5;
                }
                if r.hurry {
                    speed = walk * 2.6;
                }
            }
            Style::Patter | Style::Dash | Style::Covey => {
                let dash = r.style == Style::Dash;
                let covey = r.style == Style::Covey;
                // A covey being pressed: off on foot together, heads down.
                if let Some(&away) = pressed.get(&r.band).filter(|_| covey && !r.hurry) {
                    if let Some(to) = run_spot(r, t, look.pos, Some(away), 8.0, 16.0) {
                        r.goal = to;
                        r.hurry = true;
                        r.home = to;
                    }
                }
                let at_goal = (r.goal - look.pos).with_y(0.0).length() < 0.15;
                // A roadrunner after something it eats.
                let mut chasing = false;
                if dash && r.fed <= 0.0 && !sp.hunts.is_empty() {
                    let mut best: Option<(f32, hecs::Entity, Vec3)> = None;
                    for &(pe, ps, pp) in &prey {
                        let d = (pp - look.pos).with_y(0.0).length();
                        if sp.hunts.contains(&ps) && d < 15.0 && best.is_none_or(|b| d < b.0) {
                            best = Some((d, pe, pp));
                        }
                    }
                    if let Some((d, pe, pp)) = best {
                        chasing = true;
                        r.goal = pp;
                        r.hurry = true;
                        if d < super::carry::catch_m().max(0.35) {
                            kills.push((ent, pe));
                            r.fed = super::carry::FULL_S;
                            r.hurry = false;
                            r.goal = look.pos;
                            r.timer = 3.0 + r.rand() * 3.0;
                        }
                    }
                }
                let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < sp.beh.wary;
                if dash && near && !r.hurry && !chasing {
                    // Off on foot, flat out, away from you.
                    let away = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero();
                    if let Some(to) = run_spot(r, t, look.pos, Some(away), 14.0, 26.0) {
                        r.goal = to;
                        r.hurry = true;
                    }
                }
                if at_goal && !chasing {
                    r.hurry = false;
                    r.timer -= dt;
                    if r.timer <= 0.0 {
                        // A roadrunner looking round; a quail keeping watch
                        // over its covey from a bush top.
                        if (dash && r.rand() < 0.12) || (covey && r.rand() < 0.03) {
                            if let Some(top) = top_spot(r, t, look.pos, 8.0) {
                                r.state = State::Fly {
                                    from: look.pos,
                                    via: (look.pos + top) * 0.5 + Vec3::Y * 0.8,
                                    to: top,
                                    t: 0.0,
                                    dur: 0.8,
                                    perch: true,
                                };
                                continue;
                            }
                        }
                        let back = (r.home - look.pos).with_y(0.0);
                        // A covey keeps close; a plover wanders; a roadrunner ranges.
                        let tether = match r.style {
                            Style::Dash => 60.0,
                            Style::Covey => 8.0,
                            _ => 25.0,
                        };
                        let bias = (back.length() > tether).then(|| back.normalize_or_zero());
                        let (lo, hi) = if dash { (4.0, 20.0) } else { (1.0, 5.0) };
                        if let Some(to) = run_spot(r, t, look.pos, bias, lo, hi) {
                            r.goal = to;
                            // A roadrunner sometimes goes at a sprint for nothing.
                            r.hurry = dash && r.rand() < 0.25;
                        }
                        r.timer = if dash { 1.0 + r.rand() * 4.0 } else { 0.6 + r.rand() * 2.5 };
                    }
                }
                if r.hurry {
                    speed = match r.style {
                        Style::Dash => sp.beh.flee_speed * pace,
                        Style::Covey => walk * 3.5,
                        _ => walk * 1.8,
                    };
                }
            }
        }

        // Dev hook: VG_RUNNER=track prints one surf bird's every step: how
        // far seaward of it the water's edge is (negative: water past its
        // feet) and whether it is hurrying.
        if r.style == Style::Surf && debug() == 2 && tracked(ent.id()) {
            let down = r.down;
            let at = look.pos.dot(down);
            let lo = (0..60).map(|i| look.pos + down * (i as f32 * 0.25 - 7.5)).find(|p| {
                t.drawn(p.x, p.z, look.pos.y + 1.0).is_some_and(|g| g < t.water_edge(p.x, p.z, 0.0))
            });
            eprintln!(
                "VG_TRACK gap {:.2} {}",
                lo.map_or(f32::NAN, |p| p.dot(down) - at),
                match r.state {
                    State::Fly { .. } => "fly",
                    _ if r.hurry => "hurry",
                    _ => "walk",
                }
            );
        }
        // Dev hook: VG_RUNNER=1 prints a surf bird's reading of the beach
        // now and then.
        if r.style == Style::Surf && debug() == 1 && r.rand() < dt * 0.05 {
            eprintln!(
                "VG_RUNNER: at {:.1},{:.2},{:.1} goal {:.1},{:.2},{:.1} edge {:.2} sea {:.2} coast {:?} hurry {}",
                look.pos.x,
                look.pos.y,
                look.pos.z,
                r.goal.x,
                r.goal.y,
                r.goal.z,
                t.water_edge(look.pos.x, look.pos.z, 0.0),
                t.sea_at(look.pos.x, look.pos.z),
                t.coast(look.pos.x, look.pos.z),
                r.hurry
            );
        }
        // Run.
        let d = (r.goal - look.pos).with_y(0.0);
        let len = d.length();
        if len > 0.12 {
            let step = (speed * dt).min(len);
            let dir = d / len;
            let to = look.pos + dir * step;
            let wade = r.style == Style::Surf;
            match footing(t, to.x, to.z, look.pos.y, wade) {
                Some(y) => {
                    look.pos = Vec3::new(to.x, y, to.z);
                    look.yaw = dir.x.atan2(dir.z) + PI;
                    r.stuck = 0.0;
                }
                None => {
                    // Blocked (a bank, a log, a rock). A moment of that and it
                    // flutters over to where it was going, if it can stand
                    // there; else it stops and picks again.
                    r.stuck += dt;
                    let goal = r.goal;
                    let lands = t.drawn(goal.x, goal.z, goal.y + 1.0).filter(|&g| footing(t, goal.x, goal.z, g, r.style == Style::Surf).is_some());
                    match lands {
                        Some(g) if r.stuck > 0.8 && len < 25.0 => {
                            let to = Vec3::new(goal.x, g, goal.z);
                            r.state = State::Fly {
                                from: look.pos,
                                via: (look.pos + to) * 0.5 + Vec3::Y * (0.6 + 0.04 * len),
                                to,
                                t: 0.0,
                                dur: (len / (fly_speed * 0.6)).clamp(0.5, 3.0),
                                perch: false,
                            };
                            r.stuck = 0.0;
                        }
                        Some(_) => {}
                        None => {
                            r.goal = look.pos;
                            r.timer = r.timer.min(0.3);
                        }
                    }
                }
            }
        } else if let Some(y) = t.drawn(look.pos.x, look.pos.z, look.pos.y + 0.6) {
            // Standing: keep the feet on the ground as the tide goes.
            look.pos.y = y;
        }
    }
}
