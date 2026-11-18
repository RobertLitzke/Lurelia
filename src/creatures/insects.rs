//! Insects (the `buzz` and `dart` movements): bees that go from flower to
//! flower, hovering at each, and dragonflies that hold still in the air over
//! water and dart between spots. No physics and no animation: the models are
//! a few centimetres long and only their path shows.
//!
//! - `buzz`: flowers are plants that afford `nectar` (affordance.rs) within
//!   14 m of the bee's home; it hovers 1 to 4 s at each and drifts back
//!   toward home. With no flowers it hovers low over the ground.
//! - `dart`: spots 0.4 to 1.6 m over water or the bank within 9 m of home;
//!   hover 0.5 to 3 s, then a quick straight dart.
//!
//! Both move off a few metres when the player comes within `wary`.

use super::affordance::Affordance;
use super::plants::Flora;
use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub enum State {
    Hover { at: Vec3, timer: f32 },
    Dart { from: Vec3, to: Vec3, t: f32, dur: f32 },
}

pub struct Buzzer {
    pub state: State,
    home: Vec3,
    /// Dragonfly: keeps to water.
    water: bool,
    wob: f32,
    rng: Rng,
}

/// Is (x, z) water or its bank, for a dragonfly.
fn by_water(t: &Terrain, x: f32, z: f32) -> bool {
    t.wet(x, z) || t.shore_dist(x, z).is_some_and(|d| d < 3.0)
}

/// Height of the top of the ground or water at (x, z).
fn floor(t: &Terrain, x: f32, z: f32, from: f32) -> f32 {
    t.ground(x, z, from).map(|g| g.0).or_else(|| t.height(x, z)).unwrap_or(t.sea).max(t.sea)
}

/// The next place to hover.
fn next(b: &mut Buzzer, flora: &Flora, t: &Terrain, at: Vec3) -> Vec3 {
    if b.water {
        for _ in 0..10 {
            let a = b.rng.unit() * TAU;
            let r = 1.5 + b.rng.unit() * 7.5;
            let (x, z) = (b.home.x + a.sin() * r, b.home.z + a.cos() * r);
            if by_water(t, x, z) {
                let y = floor(t, x, z, at.y + 4.0) + 0.4 + b.rng.unit() * 1.2;
                return Vec3::new(x, y, z);
            }
        }
    } else {
        let flowers: Vec<_> = flora.near(b.home.x, b.home.z, 14.0, Some(Affordance::Nectar)).collect();
        if !flowers.is_empty() {
            let p = flowers[(b.rng.unit() * flowers.len() as f32) as usize % flowers.len()];
            let a = b.rng.unit() * TAU;
            let r = p.crown.min(1.5) * 0.6 * b.rng.unit();
            let top = p.height.max(0.6);
            let y = p.pos.y + top * (0.55 + 0.45 * b.rng.unit()) + 0.1;
            return Vec3::new(p.pos.x + a.sin() * r, y, p.pos.z + a.cos() * r);
        }
    }
    // Nothing found: low over the ground near home.
    let a = b.rng.unit() * TAU;
    let r = b.rng.unit() * 4.0;
    let (x, z) = (b.home.x + a.sin() * r, b.home.z + a.cos() * r);
    Vec3::new(x, floor(t, x, z, at.y + 4.0) + 0.3 + b.rng.unit() * 0.8, z)
}

/// `n` insects of species `s` round `at`; false if the place won't do (no
/// flowers for a bee, no water for a dragonfly).
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, flora: &Flora, t: &Terrain, at: Vec3, n: u32, water: bool, seed: u32) -> bool {
    let ok = if water {
        by_water(t, at.x, at.z)
    } else {
        flora.near(at.x, at.z, 15.0, Some(Affordance::Nectar)).next().is_some()
    };
    if !ok {
        return false;
    }
    let mut rng = Rng::new(seed | 1);
    for _ in 0..n.max(1) {
        let mut b = Buzzer {
            state: State::Hover { at, timer: 0.0 },
            home: at,
            water,
            wob: rng.unit() * TAU,
            rng: Rng::new(rng.state | 1),
        };
        let first = next(&mut b, flora, t, at + Vec3::Y);
        b.state = State::Hover { at: first, timer: rng.unit() * 2.0 };
        world.spawn((b, Creature::of(s), Look::new(sp.model, first, rng.unit() * TAU)));
    }
    true
}

/// Step every insect.
pub fn update(world: &mut hecs::World, species: &[Species], flora: &Flora, dt: f32, threat: &super::startle::Threat, t: &Terrain) {
    let player = threat.pos;
    for (b, look, me) in world.query_mut::<(&mut Buzzer, &mut Look, &Creature)>() {
        look.begin_step();
        let sp = &species[me.species];
        b.wob += dt * if b.water { 2.0 } else { 7.0 };
        let nerve = super::startle::Nerve::of(sp, None, super::age::Stage::Adult);
        let near = (look.pos - player).length() < super::startle::reach(&nerve, threat, look.pos, 1.0);
        match b.state {
            State::Hover { at, timer } => {
                let timer = timer - dt;
                // Bees jitter at a flower; dragonflies hold nearly still.
                let j = if b.water { 0.02 } else { 0.07 };
                look.pos = at + Vec3::new(b.wob.sin() * j, (b.wob * 1.3).sin() * j, (b.wob * 0.7).cos() * j);
                if near || timer <= 0.0 {
                    let to = if near {
                        let away = (look.pos - player).with_y(0.0).normalize_or_zero();
                        look.pos + away * 4.0 + Vec3::Y * 1.0
                    } else {
                        next(b, flora, t, look.pos)
                    };
                    let speed = if near { sp.beh.flee_speed } else { sp.beh.walk_speed }.max(0.5);
                    let dur = ((to - look.pos).length() / speed).clamp(0.15, 6.0);
                    b.state = State::Dart { from: look.pos, to, t: 0.0, dur };
                    if !b.water && near {
                        b.home = Vec3::new(to.x, b.home.y, to.z);
                    }
                } else {
                    b.state = State::Hover { at, timer };
                    if b.rng.unit() < dt * 0.8 {
                        look.yaw += (b.rng.unit() - 0.5) * 2.0;
                    }
                }
            }
            State::Dart { from, to, t: tt, dur } => {
                let tt = (tt + dt / dur).min(1.0);
                let e = tt * tt * (3.0 - 2.0 * tt);
                // Bees wander a little on the way; dragonflies go straight.
                let sway = if b.water { 0.0 } else { 0.15 * (PI * tt).sin() * (b.wob * 0.5).sin() };
                look.pos = from.lerp(to, e) + Vec3::new(sway, 0.0, -sway);
                let d = to - from;
                if d.with_y(0.0).length() > 0.05 {
                    look.yaw = d.x.atan2(d.z) + PI;
                }
                b.state = if tt >= 1.0 {
                    let (lo, hi) = if b.water { (0.5, 3.0) } else { (1.0, 4.0) };
                    State::Hover {
                        at: to,
                        timer: lo + b.rng.unit() * (hi - lo),
                    }
                } else {
                    State::Dart { from, to, t: tt, dur }
                };
            }
        }
    }
}
