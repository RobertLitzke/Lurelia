//! A car's lights: two headlights as spots
//! (`render/lamps.rs`, `Lamps::push_spot`) and two red tail lights, lit
//! when it is dark enough or the weather is heavy (`lit`, 0 to 1, as
//! `Game::prepare_lamps` works it out from the hour and `gloom`), and the
//! tail lights brighter while the car brakes (`Vehicle::braking`), by day
//! as well. The tail lights matter more than they sound: they are how a
//! kart ahead is seen in the dark.
//!
//! The lamps on the kart model glow to match (`draw.rs`, `KART_HEADLAMP`
//! and `KART_TAILLAMP`): `head_glow` and `tail_glow` are their gains.

use super::Vehicle;
use crate::world::weather::Sky;
use glam::Vec3;
use std::sync::atomic::{AtomicU32, Ordering};

/// How far the headlights reach, metres, and their cone's half-angle.
pub const REACH: f32 = 30.0;
const HALF: f32 = 0.45;
/// How bright the headlights are at full dark, and the tail lights.
const HEAD: f32 = 2.6;
const TAIL: f32 = 0.8;
const TAIL_REACH: f32 = 3.5;
/// The tail lights while braking: this bright, at least `BRAKE_FLOOR` of
/// it by day, and a little further.
const BRAKE: f32 = 2.2;
const BRAKE_FLOOR: f32 = 0.35;
const BRAKE_REACH: f32 = 5.0;
/// The tail lamps' glow on the model with the lights on and not braking,
/// against 1 braking.
const TAIL_GLOW: f32 = 0.6;

/// How much the weather alone calls for lights, 0 to 1: rain, snow, fog
/// and a storm's dark sky turn them on by day. A drizzle or a thin mist
/// does not.
pub fn gloom(sky: &Sky) -> f32 {
    let fall = smooth(0.3, 0.9, sky.rain.max(sky.snow));
    let fog = smooth(0.2, 0.55, sky.mist);
    let storm = smooth(0.3, 0.9, sky.lightning);
    fall.max(fog).max(storm) * 0.8
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// This frame's `lit`, set once a frame by the game for the drawing to read
/// (stored as the f32's bits).
static LIT: AtomicU32 = AtomicU32::new(0);

pub fn set_lit(lit: f32) {
    LIT.store(lit.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn lit() -> f32 {
    f32::from_bits(LIT.load(Ordering::Relaxed))
}

/// The gain on the model's headlamp glow at `lit` (0 is dark glass).
pub fn head_glow(lit: f32) -> f32 {
    lit
}

/// The gain on the model's tail lamp glow at `lit`, braking or not.
pub fn tail_glow(lit: f32, braking: bool) -> f32 {
    if braking {
        1.0
    } else {
        lit * TAIL_GLOW
    }
}

/// One light: where, which way and how wide for a spot (None all round),
/// how far, its colour and strength.
pub struct Light {
    pub pos: Vec3,
    pub spot: Option<(Vec3, f32)>,
    pub reach: f32,
    pub colour: [f32; 3],
    pub strength: f32,
}

/// Whether the car shows any light at `lit`.
pub fn any(car: &Vehicle, lit: f32) -> bool {
    lit > 0.05 || car.braking
}

/// The car's lights at `lit`.
pub fn of(car: &Vehicle, lit: f32) -> Vec<Light> {
    let (fwd, up) = (car.forward(), car.up());
    let right = fwd.cross(up).normalize_or(Vec3::X);
    let half_w = car.spec.track * 0.4;
    let nose = car.spec.wheelbase * 0.5 + car.spec.wheel_radius;
    let at = car.pos() + up * 0.3;
    // Aimed a little down, so they light the road ahead rather than the sky.
    let beam = (fwd - up * 0.08).normalize_or(fwd);
    let (tail, tail_reach) = if car.braking { (BRAKE * lit.max(BRAKE_FLOOR), BRAKE_REACH) } else { (TAIL * lit, TAIL_REACH) };
    let mut out = Vec::with_capacity(4);
    for side in [-1.0f32, 1.0] {
        if lit > 0.05 {
            out.push(Light {
                pos: at + fwd * nose + right * half_w * side,
                spot: Some((beam, HALF)),
                reach: REACH,
                colour: [1.0, 0.95, 0.85],
                strength: HEAD * lit,
            });
        }
        if tail > 0.01 {
            out.push(Light { pos: at - fwd * nose + right * half_w * side, spot: None, reach: tail_reach, colour: [1.0, 0.12, 0.08], strength: tail });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heavy_weather_turns_lights_on_and_a_drizzle_does_not() {
        let mut sky = crate::world::weather::Kind::Sun.sky();
        assert_eq!(gloom(&sky), 0.0);
        sky.rain = 1.0;
        assert!(gloom(&sky) > 0.5);
        sky.rain = 0.15;
        assert!(gloom(&sky) < 0.05);
    }

    #[test]
    fn brake_lamps_burn_brighter_than_tail_lamps() {
        assert!(tail_glow(1.0, true) > tail_glow(1.0, false));
        assert!(tail_glow(0.0, true) > 0.5, "brake lamps show by day");
        assert_eq!(tail_glow(0.0, false), 0.0);
        assert_eq!(head_glow(0.0), 0.0);
    }
}
