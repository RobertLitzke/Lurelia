//! Lightning out of a dry storm (weather.rs `Kind::Storm`): strikes from
//! the cloud deck to the ground somewhere round the camera, each a flash of
//! a few return strokes over half a second, and dark between.
//!
//! Like the weather this touches no voxel. A strike is a point on the ground
//! and a seed; the shaders draw the channel from it (common.wgsl `bolt`)
//! into the sky colour, so everything that reflects the sky -- water, a
//! flooded pan, a puddle -- shows it under your feet as well as overhead,
//! and the flash lifts the sky's light over the whole land (render/mod.rs).
//! There is no thunder: the storms this was written for are silent.

use crate::world::noise::Rng;
use crate::world::weather::Sky;
use glam::{Vec2, Vec3};

/// How long a strike lasts, seconds: the strokes and the channel's fade.
const LIFE_S: f32 = 0.75;

#[derive(Clone, Copy, Debug)]
struct Strike {
    /// Where it comes to ground, metres.
    at: Vec2,
    /// Its own shape, 0 to 1000 (common.wgsl `bolt`).
    seed: f32,
    /// Seconds since it began.
    age: f32,
    /// When each return stroke fires, seconds after the first.
    strokes: [f32; 4],
    /// How many of `strokes` it has.
    n: usize,
}

impl Strike {
    /// How bright it is now, 0 to about 1: each stroke a sharp peak that
    /// dies away in a few hundredths of a second.
    fn flash(&self) -> f32 {
        let mut f: f32 = 0.0;
        for &s in &self.strokes[..self.n] {
            let t = self.age - s;
            if t >= 0.0 {
                f = f.max((-t / 0.045).exp());
            }
        }
        // The channel glows faintly for the rest of its life.
        f.max(0.12 * (1.0 - self.age / LIFE_S).max(0.0))
    }
}

#[derive(Clone, Debug)]
pub struct Lightning {
    rng: Rng,
    /// Seconds until the next strike may come.
    wait: f32,
    strike: Option<Strike>,
    /// A strike held for a headless shot (`VG_BOLT=x,z[,flash]`).
    held: Option<[f32; 4]>,
}

impl Lightning {
    pub fn new(seed: u32) -> Lightning {
        let held = std::env::var("VG_BOLT").ok().and_then(|v| {
            let f: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            (f.len() >= 2).then(|| [f[0], f[1], 417.0, f.get(2).copied().unwrap_or(1.0)])
        });
        Lightning { rng: Rng::new(seed ^ 0x6c69_6768), wait: 2.0, strike: None, held }
    }

    /// Step by `dt` real seconds under `sky`, with the camera at `cam`.
    pub fn advance(&mut self, dt: f32, sky: &Sky, cam: Vec3) {
        if let Some(s) = &mut self.strike {
            s.age += dt;
            if s.age > LIFE_S {
                self.strike = None;
            }
        }
        let rate = sky.lightning.clamp(0.0, 1.0);
        if rate < 0.02 {
            self.wait = self.wait.max(2.0);
            return;
        }
        self.wait -= dt * rate;
        if self.wait > 0.0 || self.strike.is_some() {
            return;
        }
        // Several seconds apart in the thick of it, now and then two close
        // together; anywhere from a few hundred metres off to the horizon.
        let r = &mut self.rng;
        self.wait = if r.unit() < 0.2 { 0.6 + r.unit() * 1.2 } else { 2.5 + r.unit() * 7.0 };
        let a = r.unit() * std::f32::consts::TAU;
        let d = 260.0 + r.unit().powf(0.7) * 1700.0;
        let n = 1 + (r.unit() * 3.99) as usize;
        let mut strokes = [0.0; 4];
        for k in 1..n {
            strokes[k] = strokes[k - 1] + 0.06 + r.unit() * 0.12;
        }
        self.strike = Some(Strike {
            at: Vec2::new(cam.x + a.cos() * d, cam.z + a.sin() * d),
            seed: (r.unit() * 1000.0).floor(),
            age: 0.0,
            strokes,
            n,
        });
    }

    /// The frame's `bolt` globals: x, z where it comes to ground, its seed,
    /// and how bright it is now (0: nothing in the sky).
    pub fn uniform(&self) -> [f32; 4] {
        if let Some(h) = self.held {
            return h;
        }
        self.strike.map_or([0.0; 4], |s| [s.at.x, s.at.y, s.seed, s.flash()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::weather::Kind;

    #[test]
    fn a_storm_strikes_and_fair_weather_never_does() {
        let mut l = Lightning::new(3);
        l.held = None;
        let mut lit = 0;
        for _ in 0..(60 * 60) {
            l.advance(1.0 / 60.0, &Kind::Storm.sky(), Vec3::ZERO);
            if l.uniform()[3] > 0.5 {
                lit += 1;
            }
        }
        assert!(lit > 10, "a minute of storm lit {lit} frames");
        let mut l = Lightning::new(3);
        l.held = None;
        for _ in 0..(60 * 60) {
            l.advance(1.0 / 60.0, &Kind::Sun.sky(), Vec3::ZERO);
            assert_eq!(l.uniform()[3], 0.0);
        }
    }
}
