//! Hummingbirds (the `hum` movement): a bird that hovers at flowers. It
//! sits on a bare twig at the top of a shrub or small tree, darts out to
//! work a run of flowers -- hovering a second or so at each, facing in, and
//! backing out before the next -- and goes back to a perch. Flowers are
//! plants that afford `nectar` and are in bloom now (an ocotillo only in
//! spring, `flower_seasons`), and the drifts of wildflowers drawn in a
//! meadow (mesh/sprites.rs `bloom_at`).
//!
//! Now and then, leaving its perch, it climbs twenty metres or more
//! straight up and dives: a narrow U down past the perch at speed and up
//! the far side, then again the other way, a few times over, the way a male
//! Anna's does over a female.
//!
//! No physics. Wings beat in a blur (`hover`) and the body hangs tilted up
//! while it hovers (birds.rs sets `Flight::upright`).

use super::affordance::Affordance;
use super::plants::Flora;
use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub enum State {
    /// On a twig.
    Perch { timer: f32 },
    /// Hovering at a flower at `at`, facing `face` (a yaw).
    Hover { at: Vec3, face: f32, timer: f32 },
    /// A quick flight from `from` to `to`; `then` says what it does there.
    Dart { from: Vec3, to: Vec3, t: f32, dur: f32, then: Then },
    /// The dive display: `dir` is the way across the U, `dives` how many
    /// are left, `s` how far through this one (0 at the top of the climb on
    /// one side, 1 at the top on the other).
    Dive { bottom: Vec3, dir: Vec3, s: f32, dives: u32, climb: f32 },
}

/// What a dart ends in.
#[derive(Clone, Copy, Debug)]
pub enum Then {
    Hover { face: f32 },
    Perch,
    /// At the top of the climb, ready to dive.
    Dive { bottom: Vec3, dir: Vec3, dives: u32 },
}

pub struct Hummer {
    pub state: State,
    home: Vec3,
    /// The twig it goes back to.
    perch: Vec3,
    /// Flowers left in this run.
    visits: u32,
    /// The last flower: the next is often on the same plant.
    last: Option<Vec3>,
    wob: f32,
    rng: Rng,
}

impl Hummer {
    pub fn perched(&self) -> bool {
        matches!(self.state, State::Perch { .. })
    }
    /// Coming in to the twig.
    pub fn landing(&self) -> bool {
        matches!(self.state, State::Dart { t, then: Then::Perch, .. } if t > 0.75)
    }
    pub fn doing(&self) -> &'static str {
        match self.state {
            State::Perch { .. } => "perched",
            State::Hover { .. } => "at a flower",
            State::Dart { then: Then::Dive { .. }, .. } | State::Dive { .. } => "diving",
            State::Dart { .. } => "darting",
        }
    }
    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }
}

/// How far from home it goes for flowers, metres.
const RANGE: f32 = 22.0;
/// Height of the display's climb above the bottom of the dive, metres, and
/// half the width of the U.
const CLIMB: (f32, f32) = (18.0, 10.0);
const HALF_U: (f32, f32) = (5.0, 3.0);
/// Seconds through one dive.
const DIVE_S: f32 = 1.3;

/// The flower to go to next and the yaw that faces it: a blooming plant
/// near home (often the one it is at), else a drift of meadow flowers.
fn next_flower(h: &mut Hummer, flora: &Flora, blooming: &[bool], t: &Terrain) -> Option<(Vec3, f32)> {
    let open = |p: u16| blooming.get(p as usize).copied().unwrap_or(true);
    // Another flower on the same plant, close by.
    if let Some(last) = h.last.filter(|_| h.rng.unit() < 0.6) {
        if let Some(p) = flora.near(last.x, last.z, 2.5, Some(Affordance::Nectar)).find(|p| open(p.plant)) {
            return Some(on_plant(h, p));
        }
    }
    let plants: Vec<_> = flora.near(h.home.x, h.home.z, RANGE, Some(Affordance::Nectar)).filter(|p| open(p.plant)).copied().collect();
    if !plants.is_empty() && h.rng.unit() < 0.8 {
        let p = plants[(h.rand() * plants.len() as f32) as usize % plants.len()];
        return Some(on_plant(h, &p));
    }
    // Meadow flowers: the thickest drift of a dozen looks.
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..12 {
        let a = h.rand() * TAU;
        let r = h.rand() * RANGE;
        let (x, z) = (h.home.x + a.sin() * r, h.home.z + a.cos() * r);
        let Some((gy, m)) = t.ground(x, z, t.store.top_m()) else { continue };
        let bloom = crate::mesh::sprites::bloom_at(m, x, z) + h.rand() * 0.02;
        if bloom > 0.05 && best.is_none_or(|b| bloom > b.0) {
            let g = t.drawn(x, z, gy + 0.5).unwrap_or(gy);
            best = Some((bloom, Vec3::new(x, g + 0.3 + h.rand() * 0.15, z)));
        }
    }
    if let Some((_, f)) = best {
        let face = h.rand() * TAU;
        // Hover just off it, facing in.
        let off = Vec3::new(face.sin(), 0.0, face.cos()) * 0.12;
        h.last = None;
        return Some((f + off, face));
    }
    plants.first().map(|p| on_plant(h, p))
}

/// A flower on plant `p`: somewhere on the outside of its crown, upper half
/// (an ocotillo's flowers are at the tips of its canes), and the yaw that
/// faces in to it from just outside.
fn on_plant(h: &mut Hummer, p: &super::plants::PlantRef) -> (Vec3, f32) {
    h.last = Some(p.pos);
    let a = h.rand() * TAU;
    let out = Vec3::new(a.sin(), 0.0, a.cos());
    let r = p.crown.clamp(0.2, 2.0) * (0.6 + 0.4 * h.rand());
    let top = p.height.max(0.4);
    let y = p.pos.y + top * (0.55 + 0.42 * h.rand());
    let flower = p.pos + out * r + Vec3::Y * (y - p.pos.y);
    // The bird hangs a beak's length outside, facing the plant: a model
    // faces -Z, so facing inward (-out) is yaw atan2(out).
    (flower + out * 0.12, out.x.atan2(out.z))
}

/// A bare top to sit on: the highest point of a shrub or small tree, 1 to
/// 5 m over the ground, of a dozen looks round `at`.
fn perch_spot(h: &mut Hummer, t: &Terrain, at: Vec3, rmin: f32, rmax: f32, toward: Option<Vec3>) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..12 {
        let a = match toward {
            Some(d) => d.x.atan2(d.z) + (h.rand() - 0.5) * 1.6,
            None => h.rand() * TAU,
        };
        let r = rmin + h.rand() * (rmax - rmin);
        let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
        let Some(y) = t.perch(x, z, at.y + 25.0) else { continue };
        let rise = y - t.height(x, z).unwrap_or(y);
        if !(0.8..6.0).contains(&rise) || y < t.sea + 0.5 {
            continue;
        }
        let score = 4.0 - (rise - 2.5).abs() + h.rand();
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, Vec3::new(x, y, z)));
        }
    }
    best.map(|b| b.1)
}

/// `n` hummingbirds of species `s` round `at`; false where there is
/// nothing in flower.
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, flora: &Flora, blooming: &[bool], t: &Terrain, at: Vec3, n: u32, seed: u32) -> bool {
    let mut rng = Rng::new(seed | 1);
    for k in 0..n.max(1) {
        let mut h = Hummer {
            state: State::Perch { timer: 0.0 },
            home: at,
            perch: at,
            visits: 0,
            last: None,
            wob: rng.unit() * TAU,
            rng: Rng::new(crate::world::noise::hash32(seed ^ k.wrapping_mul(0x27d4_eb2d)) | 1),
        };
        let Some((flower, face)) = next_flower(&mut h, flora, blooming, t) else {
            return k > 0;
        };
        if k == 0 && (flower - at).with_y(0.0).length() > RANGE + 3.0 {
            return false;
        }
        h.perch = perch_spot(&mut h, t, flower, 2.0, 14.0, None).unwrap_or(flower + Vec3::Y * 1.5);
        h.home = flower;
        // Half start on a twig, half at a flower.
        let pos = if h.rand() < 0.5 {
            h.state = State::Perch { timer: 2.0 + h.rand() * 8.0 };
            h.perch
        } else {
            h.visits = 2 + (h.rand() * 4.0) as u32;
            h.state = State::Hover { at: flower, face, timer: 0.5 + h.rand() };
            flower
        };
        world.spawn((h, Creature::of(s), Look::new(sp.model, pos, face)));
    }
    true
}

/// Step every hummingbird. `blooming[plant]` says whether a library plant
/// is in flower this season.
pub fn update(world: &mut hecs::World, species: &[Species], flora: &Flora, blooming: &[bool], dt: f32, threat: &super::startle::Threat, t: &Terrain) {
    let player = threat.pos;
    for (h, look, me, morph) in world.query_mut::<(&mut Hummer, &mut Look, &Creature, Option<&crate::variety::Morph>)>() {
        look.begin_step();
        let sp = &species[me.species];
        let (pace, _) = crate::variety::pace(morph);
        let speed = sp.beh.walk_speed.max(2.0) * pace;
        let fast = sp.beh.flee_speed.max(speed) * pace;
        h.wob += dt * 9.0;
        let nerve = super::startle::Nerve::of(sp, morph, super::age::Stage::Adult);
        let near = (look.pos - player).length() < super::startle::reach(&nerve, threat, look.pos, 1.5);
        let diving = matches!(h.state, State::Dive { .. } | State::Dart { then: Then::Dive { .. }, .. });
        if near && !diving && !matches!(h.state, State::Dart { .. }) {
            // Off to a twig farther away, and a new patch round it.
            let away = (look.pos - player).with_y(0.0).normalize_or_zero();
            let to = perch_spot(h, t, look.pos, 10.0, 22.0, Some(away)).unwrap_or(look.pos + away * 10.0 + Vec3::Y * 3.0);
            h.home = to;
            h.perch = to;
            h.visits = 0;
            dart(h, look.pos, to, fast, Then::Perch);
        }
        match h.state {
            State::Perch { timer } => {
                look.pos = h.perch;
                let timer = timer - dt;
                if timer > 0.0 {
                    h.state = State::Perch { timer };
                    // Quick turns of the head are the animation's; now and
                    // then it shuffles round on the twig.
                    if h.rand() < dt * 0.4 {
                        look.yaw += (h.rand() - 0.5) * 2.0;
                    }
                    continue;
                }
                // Off the twig: a display now and then, else a run of flowers.
                if h.rand() < 0.2 {
                    let a = h.rand() * TAU;
                    let dir = Vec3::new(a.sin(), 0.0, a.cos());
                    let g = t.drawn(h.perch.x, h.perch.z, h.perch.y + 0.5).unwrap_or(h.perch.y - 2.0);
                    let bottom = Vec3::new(h.perch.x, g + 1.0, h.perch.z) + dir * (h.rand() - 0.5) * 2.0;
                    let climb = CLIMB.0 + h.rand() * CLIMB.1;
                    let half = HALF_U.0 + h.rand() * HALF_U.1;
                    let top = bottom - dir * half + Vec3::Y * climb;
                    let dives = 2 + (h.rand() * 4.0) as u32;
                    // Straight up: a slow climb, wings whirring.
                    let from = look.pos;
                    h.state = State::Dart {
                        from,
                        to: top,
                        t: 0.0,
                        dur: ((top - from).length() / (speed * 0.6)).clamp(1.5, 8.0),
                        then: Then::Dive { bottom, dir: dir * half, dives },
                    };
                } else {
                    h.visits = 3 + (h.rand() * 6.0) as u32;
                    go_to_flower(h, flora, blooming, t, look.pos, speed);
                }
            }
            State::Hover { at, face, timer } => {
                let timer = timer - dt;
                // Holding still in the air, a little bob, and backing out of
                // the flower at the end.
                let facing = Vec3::new(-face.sin(), 0.0, -face.cos());
                let back = if timer < 0.25 { (0.25 - timer.max(0.0)) * 0.5 } else { 0.0 };
                look.pos = at + Vec3::new(h.wob.sin() * 0.008, (h.wob * 0.7).sin() * 0.015, (h.wob * 1.3).cos() * 0.008) - facing * back;
                look.yaw = face;
                if timer > 0.0 {
                    h.state = State::Hover { at, face, timer };
                    continue;
                }
                h.visits = h.visits.saturating_sub(1);
                if h.visits == 0 {
                    // Back to a twig: its own, or one nearer if it has wandered.
                    if (h.perch - look.pos).length() > 18.0 {
                        h.perch = perch_spot(h, t, look.pos, 2.0, 12.0, None).unwrap_or(h.perch);
                    }
                    let perch = h.perch;
                    dart(h, look.pos, perch, speed, Then::Perch);
                } else {
                    go_to_flower(h, flora, blooming, t, look.pos, speed);
                }
            }
            State::Dart { from, to, t: tt, dur, then } => {
                let tt = (tt + dt / dur).min(1.0);
                let e = tt * tt * (3.0 - 2.0 * tt);
                let d = to - from;
                // Long darts lift over what is between.
                let lift = (0.15 * d.with_y(0.0).length()).min(1.5) * (PI * tt).sin();
                let p = from.lerp(to, e) + Vec3::Y * lift;
                let floor = t.drawn(p.x, p.z, p.y + 1.0).map_or(p.y, |g| g + 0.15);
                look.pos = Vec3::new(p.x, p.y.max(floor.min(to.y.max(from.y))), p.z);
                let flat = d.with_y(0.0);
                if flat.length() > 0.3 {
                    look.yaw = flat.x.atan2(flat.z) + PI;
                }
                if tt < 1.0 {
                    h.state = State::Dart { from, to, t: tt, dur, then };
                    continue;
                }
                look.pos = to;
                // Dev hook: VG_HUM=1 says what each dart ended in.
                if debug() {
                    let ground = t.drawn(to.x, to.z, to.y + 0.5).unwrap_or(f32::NAN);
                    eprintln!("VG_HUM: {} at {:.1},{:.1},{:.1} ({:.1} m up)", match then { Then::Hover { .. } => "flower", Then::Perch => "perch", Then::Dive { .. } => "dive" }, to.x, to.y, to.z, to.y - ground);
                }
                h.state = match then {
                    Then::Hover { face } => State::Hover {
                        at: to,
                        face,
                        timer: 0.5 + h.rand() * 1.2,
                    },
                    Then::Perch => State::Perch { timer: 5.0 + h.rand() * 20.0 },
                    Then::Dive { bottom, dir, dives } => State::Dive {
                        bottom,
                        dir,
                        s: 0.0,
                        dives,
                        climb: to.y - bottom.y,
                    },
                };
            }
            State::Dive { bottom, dir, s, dives, climb } => {
                let s = s + dt / DIVE_S;
                if s < 1.0 {
                    // Down the U and up the far side, fastest at the bottom.
                    let u = (PI * (s - 0.5)).sin();
                    let p = bottom + dir * u + Vec3::Y * climb * u * u;
                    let v = p - look.pos;
                    look.pos = p;
                    if v.with_y(0.0).length() > 1e-3 {
                        look.yaw = v.x.atan2(v.z) + PI;
                    }
                    h.state = State::Dive { bottom, dir, s, dives, climb };
                    continue;
                }
                if dives > 1 {
                    // Hang at the top a moment, turned round, and back down.
                    let top = bottom + dir + Vec3::Y * climb;
                    let from = look.pos;
                    h.state = State::Dart {
                        from,
                        to: top,
                        t: 0.0,
                        dur: 0.6 + h.rand() * 0.6,
                        then: Then::Dive { bottom, dir: -dir, dives: dives - 1 },
                    };
                } else {
                    let perch = h.perch;
                    dart(h, look.pos, perch, fast, Then::Perch);
                }
            }
        }
    }
}

/// Dev hook VG_HUM, read once.
fn debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VG_HUM").is_some())
}

fn dart(h: &mut Hummer, from: Vec3, to: Vec3, speed: f32, then: Then) {
    let dur = ((to - from).length() / speed).clamp(0.2, 5.0);
    h.state = State::Dart { from, to, t: 0.0, dur, then };
}

fn go_to_flower(h: &mut Hummer, flora: &Flora, blooming: &[bool], t: &Terrain, from: Vec3, speed: f32) {
    match next_flower(h, flora, blooming, t) {
        Some((to, face)) => {
            // Never into the ground on a slope beside the plant.
            let floor = t.drawn(to.x, to.z, to.y + 1.0).map_or(to.y, |g| g + 0.25);
            dart(h, from, Vec3::new(to.x, to.y.max(floor), to.z), speed, Then::Hover { face })
        }
        None => {
            h.visits = 0;
            let perch = h.perch;
            dart(h, from, perch, speed, Then::Perch);
        }
    }
}
