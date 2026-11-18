//! The game's own events turned into particles: a landing, going into the
//! water, a wheel on dirt, a paddle stroke, breath on a cold morning, snow
//! kicked up underfoot. Each is found by comparing the body before and after
//! a step, so nothing in player.rs or ride.rs has to know particles exist.

use super::Fx;
use crate::ride::Mode;
use crate::world::material::AIR;
use crate::world::matlib::{self, flag};
use crate::world::store::Store;
use glam::Vec3;

/// Falling faster than this, m/s, throws dust on landing.
const LAND_V: f32 = 4.0;
/// Going into the water faster than this, m/s, splashes.
const SPLASH_V: f32 = 1.5;
/// A wheel throws dust above this speed over the ground, m/s.
const WHEEL_V: f32 = 2.5;
/// Feet kick snow up above this speed, m/s.
const KICK_V: f32 = 1.2;

/// What the hooks remember between steps.
#[derive(Default, Clone, Debug)]
pub struct State {
    strokes: f32,
    breath_t: f32,
}

/// The body as one step left it, and what it was before.
pub struct Body {
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub vel: Vec3,
    /// The velocity going into the step: a landing has already zeroed `vel.y`.
    pub vel_before: Vec3,
    pub on_ground: bool,
    pub was_ground: bool,
    pub swimming: bool,
    pub fly: bool,
    /// The water's surface over the body, metres.
    pub water: f32,
    pub mode: Mode,
    pub forward: Vec3,
    pub eye: Vec3,
    /// Strokes the kayak has taken (`Ride::strokes`) and which blade.
    pub strokes: f32,
    pub stroke_side: f32,
    /// Cold enough to see your breath.
    pub cold: bool,
    /// Snow lying on the ground.
    pub snow: bool,
}

/// The material a body at `pos` stands on: the voxel under the feet, or
/// the one under that where the feet are in a slope's lip. AIR for none.
pub fn ground_under(store: &Store, pos: Vec3) -> u8 {
    let (x, z) = ((pos.x * 4.0).floor() as i32, (pos.z * 4.0).floor() as i32);
    let y = ((pos.y + 0.01) * 4.0).floor() as i32;
    for dy in 1..=3 {
        match store.get(x, y - dy, z) {
            Some(m) if m != AIR && !matlib::has(m, flag::FOLIAGE) => return m,
            _ => {}
        }
    }
    AIR
}

/// Hard enough to strike sparks: anything that isn't wood, earth, snow
/// or growing, going by its name.
pub fn hard(m: u8) -> bool {
    if matlib::has(m, flag::FOLIAGE) || matlib::has(m, flag::GROWS) || matlib::has(m, flag::WATER) {
        return false;
    }
    let soft = ["wood", "log", "plank", "bark", "dirt", "soil", "mud", "sand", "grass", "moss", "snow", "leaf", "peat", "loam", "straw", "thatch", "cloth", "clay", "ash"];
    !matlib::name(m).is_some_and(|n| soft.iter().any(|s| n.contains(s)))
}

fn snowy(m: u8) -> bool {
    m == crate::world::material::SNOW || matlib::name(m).is_some_and(|n| n.contains("snow"))
}

/// Particles for one step of the player's body.
pub fn step(fx: &mut Fx, b: &Body, store: &Store, dt: f32) {
    if !fx.on || b.fly {
        return;
    }
    let fwd = Vec3::new(b.forward.x, 0.0, b.forward.z).normalize_or(Vec3::NEG_Z);
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let flat = Vec3::new(b.vel.x, 0.0, b.vel.z).length();

    // Into the water: a splash where the body broke the surface.
    let in_water = b.pos.y < b.water;
    if in_water && b.prev_pos.y >= b.water - 0.05 && b.vel_before.y < -SPLASH_V && b.mode != Mode::Kayak {
        let at = Vec3::new(b.pos.x, b.water + 0.02, b.pos.z);
        let amount = (-b.vel_before.y / 6.0).clamp(0.4, 2.0);
        fx.burst("splash", at, Vec3::Y, 0, amount);
        fx.burst("splash_mist", at, Vec3::Y, 0, amount);
    }

    let under = if b.on_ground { ground_under(store, b.pos) } else { AIR };
    let wet = under != AIR && matlib::has(under, flag::WATER);

    // A landing: dust off the ground it landed on, and grit with it.
    if b.on_ground && !b.was_ground && !in_water && b.vel_before.y < -LAND_V && under != AIR && !wet {
        let amount = ((-b.vel_before.y - LAND_V) / 6.0 + 0.6).min(2.0);
        let at = Vec3::new(b.pos.x, b.pos.y + 0.05, b.pos.z);
        if snowy(under) {
            fx.burst("snow_kick", at, Vec3::Y, under, amount * 2.0);
        } else {
            fx.burst("landing_dust", at, Vec3::Y, under, amount);
            fx.burst("landing_grit", at, Vec3::Y, under, amount);
        }
    }

    // The bike's back wheel on loose ground. Grass holds its dirt (and the
    // tufts would hide a puff that low anyway).
    if b.mode == Mode::Bike && b.on_ground && flat > WHEEL_V && under != AIR && !wet && !matlib::has(under, flag::UNDERSTORY) {
        let rate = 14.0 * (flat / 8.0).min(1.5);
        let at = b.pos - fwd * 0.55 + Vec3::Y * 0.06;
        let back = -fwd * 0.8 + Vec3::Y * 0.6;
        let kind = if snowy(under) { "snow_kick" } else { "wheel_dust" };
        fx.trickle(kind, at, back, under, rate, dt);
    }

    // Feet in lying snow.
    if b.mode == Mode::Foot && b.on_ground && flat > KICK_V && snowy(under) && (b.snow || under != AIR) {
        let rate = 6.0 * (flat / 5.0).min(1.6);
        fx.trickle("snow_kick", b.pos + fwd * 0.15 + Vec3::Y * 0.05, -fwd * 0.4 + Vec3::Y, under, rate, dt);
    }

    // A paddle stroke: spray off the blade as it comes out of the water.
    if b.mode == Mode::Kayak {
        if b.strokes.floor() > fx.hooks.strokes.floor() || b.strokes.floor() < fx.hooks.strokes.floor() - 1.0 {
            let side = if b.stroke_side >= 0.0 { 1.0 } else { -1.0 };
            let at = Vec3::new(b.pos.x, b.water + 0.05, b.pos.z) + right * side * 0.95 - fwd * 0.3;
            fx.burst("paddle_spray", at, Vec3::Y * 1.2 - fwd * 0.4 + right * side * 0.3, 0, 1.0);
        }
    }
    fx.hooks.strokes = b.strokes;

    // Breath: a puff every few seconds while it is cold, in front of the face.
    if b.cold && !(b.swimming && b.eye.y < b.water) {
        fx.hooks.breath_t -= dt;
        if fx.hooks.breath_t <= 0.0 {
            fx.hooks.breath_t = 3.0 + fx.roll() * 1.5;
            let look = b.forward.normalize_or(Vec3::NEG_Z);
            let at = b.eye + look * 0.32 - Vec3::Y * 0.14 + b.vel * 0.05;
            fx.burst("breath", at, look + Vec3::Y * 0.15, 0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxDef;
    use crate::world::store::Voxels;

    fn defs() -> Vec<FxDef> {
        ["landing_dust", "landing_grit", "splash", "splash_mist", "breath"]
            .iter()
            .map(|n| serde_json::from_value(serde_json::json!({"name": n, "count": [5, 5], "life": [1.0, 1.0], "size": [0.1, 0.1]})).unwrap())
            .collect()
    }

    /// Stone floor at y 1 m (voxels 0..4), air above, 16 m square.
    fn floor() -> Store {
        let mut s = Store::default();
        for cx in 0..2 {
            for cz in 0..2 {
                s.chunks.insert((cx, 0, cz), Voxels::Uniform(AIR));
            }
        }
        for x in 0..64 {
            for z in 0..64 {
                for y in 0..4 {
                    s.set(x, y, z, crate::world::material::STONE);
                }
            }
        }
        s
    }

    fn body() -> Body {
        Body {
            pos: Vec3::new(8.0, 1.0, 8.0),
            prev_pos: Vec3::new(8.0, 1.1, 8.0),
            vel: Vec3::ZERO,
            vel_before: Vec3::new(0.0, -7.0, 0.0),
            on_ground: true,
            was_ground: false,
            swimming: false,
            fly: false,
            water: -10.0,
            mode: Mode::Foot,
            forward: Vec3::NEG_Z,
            eye: Vec3::new(8.0, 2.6, 8.0),
            strokes: 0.0,
            stroke_side: 1.0,
            cold: false,
            snow: false,
        }
    }

    #[test]
    fn a_hard_landing_throws_dust_and_a_soft_one_none() {
        let store = floor();
        let mut fx = Fx::with_defs(defs());
        step(&mut fx, &body(), &store, 1.0 / 60.0);
        assert!(fx.len() >= 10, "a 7 m/s landing threw {}", fx.len());
        let mut fx = Fx::with_defs(defs());
        step(&mut fx, &Body { vel_before: Vec3::new(0.0, -2.0, 0.0), ..body() }, &store, 1.0 / 60.0);
        assert_eq!(fx.len(), 0, "a step down threw dust");
        let mut fx = Fx::with_defs(defs());
        step(&mut fx, &Body { fly: true, ..body() }, &store, 1.0 / 60.0);
        assert_eq!(fx.len(), 0, "the free camera threw dust");
    }

    #[test]
    fn going_into_the_water_splashes() {
        let store = floor();
        let mut fx = Fx::with_defs(defs());
        let b = Body {
            pos: Vec3::new(8.0, 4.9, 8.0),
            prev_pos: Vec3::new(8.0, 5.1, 8.0),
            on_ground: false,
            water: 5.0,
            ..body()
        };
        step(&mut fx, &b, &store, 1.0 / 60.0);
        assert!(fx.len() >= 5, "no splash: {}", fx.len());
    }

    #[test]
    fn breath_comes_every_few_seconds_while_cold() {
        let store = floor();
        let mut fx = Fx::with_defs(defs());
        let b = Body { cold: true, was_ground: true, vel_before: Vec3::ZERO, ..body() };
        for _ in 0..600 {
            step(&mut fx, &b, &store, 1.0 / 60.0);
            fx.step(1.0 / 60.0, &store);
        }
        // Ten seconds: two or three breaths of five puffs, a second each.
        assert!(fx.len() <= 5, "breaths pile up: {}", fx.len());
        let mut n = 0;
        for _ in 0..600 {
            let before = fx.len();
            step(&mut fx, &b, &store, 1.0 / 60.0);
            n += (fx.len() > before) as usize;
        }
        assert!((2..=4).contains(&n), "{n} breaths in ten seconds");
    }
}
