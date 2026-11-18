//! The northern lights: how strong they are tonight and this hour. The
//! curtains themselves are drawn by the sky shader (scene.wgsl `aurora`),
//! which reads the `aurora` vec4 in Globals; the land and water take a faint
//! green from them through the sky fill (render/mod.rs).
//!
//! A world sets the strength of its best night (world.json `aurora`); most
//! worlds leave it 0 and never see them. Each night draws its own share of
//! that, seeded by the world and the count of nights, so some nights are
//! quiet and a few are a storm with arcs right overhead. Within a night the
//! display builds after dark, peaks in a substorm an hour either side of
//! midnight and slackens toward morning.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

static STRENGTH: AtomicU32 = AtomicU32::new(0);
static SEED: AtomicU32 = AtomicU32::new(0);

/// Noons seen since the world loaded, to tell one night from the next.
struct Nights {
    last: f32,
    n: u32,
}

static NIGHTS: Mutex<Nights> = Mutex::new(Nights { last: -1.0, n: 0 });

/// This world's best-night strength (0 = none) and its seed.
pub fn set_strength(s: f32, seed: u32) {
    STRENGTH.store(s.max(0.0).to_bits(), Ordering::Relaxed);
    SEED.store(seed, Ordering::Relaxed);
    *NIGHTS.lock().unwrap() = Nights { last: -1.0, n: 0 };
}

pub fn strength() -> f32 {
    f32::from_bits(STRENGTH.load(Ordering::Relaxed))
}

fn hash(n: u32, seed: u32) -> f32 {
    let mut h = n.wrapping_mul(0x9E37_79B9) ^ seed.wrapping_mul(0x85EB_CA6B) ^ 0xA0A1;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// How much of the world's strength night `n` gets: one night in five is
/// quiet, most are middling, a few come near the whole of it.
pub fn night_share(n: u32, seed: u32) -> f32 {
    let h = hash(n, seed);
    if h < 0.2 {
        0.12
    } else {
        0.3 + 0.7 * ((h - 0.2) / 0.8).powf(1.6)
    }
}

/// Within a night, by the hour: a build after dark to a substorm around
/// midnight, slackening toward morning but never quite gone.
pub fn activity(hour: f32) -> f32 {
    // Hours from midnight, -12..12.
    let x = (hour + 12.0).rem_euclid(24.0) - 12.0;
    let peak = (-((x - 0.3) / 1.6).powi(2)).exp();
    0.45 + 0.55 * peak
}

fn forced() -> Option<f32> {
    static F: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *F.get_or_init(|| std::env::var("VG_AURORA").ok().and_then(|v| v.parse().ok()))
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The Globals `aurora` vec4 for `hour`: x = brightness now (0 by day and
/// on worlds without them), y = how far the arcs reach overhead (0 low in
/// the north, 1 across the zenith), z = a number fixed for the night that
/// lays the curtains out differently each time, w = unused.
pub fn uniform(hour: f32) -> [f32; 4] {
    let s = strength();
    if s <= 0.0 {
        return [0.0; 4];
    }
    let n = {
        let mut nights = NIGHTS.lock().unwrap();
        if nights.last >= 0.0 && nights.last < 12.0 && hour >= 12.0 && hour - nights.last < 6.0 {
            nights.n += 1;
        }
        nights.last = hour;
        nights.n
    };
    let seed = SEED.load(Ordering::Relaxed);
    // VG_AURORA=0..1 holds tonight's share, for screenshots.
    let share = forced().unwrap_or_else(|| night_share(n, seed));
    let dark = smooth(-6.0, -13.0, crate::world::clock::sun_elevation(hour));
    let now = share * activity(hour);
    [s * now * dark, (now * now * 1.4).min(1.0), hash(n, seed ^ 0x5bd1) * 97.0, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nights_differ_and_some_are_quiet() {
        let shares: Vec<f32> = (0..200).map(|n| night_share(n, 7)).collect();
        let quiet = shares.iter().filter(|s| **s < 0.15).count();
        let big = shares.iter().filter(|s| **s > 0.8).count();
        assert!((20..60).contains(&quiet), "{quiet}");
        assert!(big > 5 && big < 60, "{big}");
    }

    #[test]
    fn the_display_peaks_near_midnight() {
        assert!(activity(0.3) > 0.99);
        assert!(activity(20.0) < 0.5 && activity(20.0) > 0.44);
        assert!(activity(12.0) >= 0.45);
    }
}
