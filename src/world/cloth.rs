//! Cloth in the wind: lines of prayer flags and kites on their strings.
//!
//! Neither is in the voxels. The chronicle's `flags` and `kites` events
//! leave a list here, and each frame the game draws the ones near the camera
//! as plates (`render/plates.rs`), swung by the clock and the day's wind, so
//! a flag line flutters and a kite rides and rocks on its string without any
//! of it touching the mesher. Kites come down when the wind drops and at
//! night; flags only hang slacker.

use crate::render::plates::Plates;
use glam::{Vec2, Vec3};

/// A string of flags between two points, sagging under its own weight.
#[derive(Clone, Debug)]
pub struct FlagLine {
    pub a: Vec3,
    pub b: Vec3,
    /// How far the middle hangs below the straight line, as a share of the
    /// length.
    pub sag: f32,
    /// One flag's width and height, metres.
    pub size: [f32; 2],
    /// Metres from one flag to the next along the line.
    pub spacing: f32,
    /// Linear RGBA, taken in turn along the line.
    pub colours: Vec<[f32; 4]>,
    pub seed: u32,
}

/// A kite flown from a point on the ground.
#[derive(Clone, Debug)]
pub struct Kite {
    pub anchor: Vec3,
    /// How high it flies over its anchor on a fair day, metres.
    pub height: f32,
    /// Half the diamond's diagonal, metres.
    pub size: f32,
    /// Body, stripe and tail, linear RGBA.
    pub body: [f32; 4],
    pub stripe: [f32; 4],
    pub tail: [f32; 4],
    pub seed: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Cloth {
    pub flags: Vec<FlagLine>,
    pub kites: Vec<Kite>,
}

/// Past this far a flag line is not drawn.
pub const FLAG_RANGE: f32 = 170.0;
/// Past this far a kite is not drawn: they are a skyline thing.
pub const KITE_RANGE: f32 = 520.0;

/// sRGB bytes to the linear RGBA the plates take.
pub fn linear(c: [u8; 3]) -> [f32; 4] {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), 1.0]
}

fn hash01(seed: u32, k: u32) -> f32 {
    crate::world::noise::hash32(seed ^ k.wrapping_mul(0x9e37_79b9)) as f32 / u32::MAX as f32
}

/// The wind as the cloth feels it: the world's one wind direction (the one
/// the grass bends to) and the weather's strength, 0 still to about 1.5.
pub struct Air {
    pub dir: Vec3,
    pub strength: f32,
    /// Seconds on the game's clock.
    pub t: f32,
    /// True while the sun is up: kites are flown by day.
    pub day: bool,
}

impl Air {
    pub fn new(strength: f32, t: f32, day: bool) -> Self {
        let w: Vec2 = crate::world::WIND;
        Air { dir: Vec3::new(w.x, 0.0, w.y).normalize(), strength, t, day }
    }
}

/// A thin cord from `a` to `b`, drawn as one plate turned to face the eye.
fn cord(out: &mut Plates, a: Vec3, b: Vec3, eye: Vec3, width: f32, colour: [f32; 4]) {
    let mid = (a + b) * 0.5;
    let along = (b - a) * 0.5;
    let to_eye = (eye - mid).normalize_or_zero();
    let side = along.cross(to_eye).normalize_or_zero() * width * 0.5;
    if side == Vec3::ZERO {
        return;
    }
    out.flat(mid, along, side, colour);
}

/// The point `s` (0 to 1) along a line from `a` to `b` that sags `sag` of
/// its length at the middle.
fn sagged(a: Vec3, b: Vec3, sag: f32, s: f32) -> Vec3 {
    let len = a.distance(b);
    a.lerp(b, s) - Vec3::Y * (4.0 * s * (1.0 - s) * sag * len)
}

impl FlagLine {
    pub fn near(&self, eye: Vec3, range: f32) -> bool {
        let mid = (self.a + self.b) * 0.5;
        mid.distance(eye) - self.a.distance(self.b) * 0.5 <= range
    }

    pub fn draw(&self, air: &Air, eye: Vec3, out: &mut Plates) {
        let len = self.a.distance(self.b);
        if len < 0.5 {
            return;
        }
        let d = ((self.a + self.b) * 0.5).distance(eye);
        let far = d > FLAG_RANGE * 0.6;
        let rope = linear([96, 84, 66]);
        let n_cord = if far { 3 } else { ((len / 2.5) as usize).clamp(3, 14) };
        for k in 0..n_cord {
            let s0 = k as f32 / n_cord as f32;
            let s1 = (k + 1) as f32 / n_cord as f32;
            cord(out, sagged(self.a, self.b, self.sag, s0), sagged(self.a, self.b, self.sag, s1), eye, 0.03, rope);
        }
        let n = ((len / self.spacing) as usize).max(1);
        let tangent = (self.b - self.a).normalize();
        // The wind across the line is what lifts the flags off it.
        let mut across = air.dir - tangent * air.dir.dot(tangent);
        let lift = across.length();
        if lift < 0.15 {
            across = tangent.cross(Vec3::Y).normalize_or_zero();
            if across.dot(air.dir) < 0.0 {
                across = -across;
            }
        } else {
            across /= lift;
        }
        let blow = (air.strength * 0.75).clamp(0.05, 1.1) * lift.max(0.3);
        let [w, h] = self.size;
        for i in 0..n {
            let s = (i as f32 + 0.5) / n as f32;
            let top = sagged(self.a, self.b, self.sag, s);
            let local = (sagged(self.a, self.b, self.sag, (s + 0.01).min(1.0)) - sagged(self.a, self.b, self.sag, (s - 0.01).max(0.0))).normalize_or_zero();
            let phase = i as f32 * 0.83 + hash01(self.seed, i as u32) * 6.0;
            let colour = self.colours[i % self.colours.len()];
            // Two halves, the lower one lagging, so the cloth ripples.
            let mut at = top;
            let halves = if far { 1 } else { 2 };
            for half in 0..halves {
                let flutter = (air.t * (5.5 + half as f32 * 1.7) + phase + half as f32 * 1.1).sin() * (0.12 + 0.22 * blow);
                let swing = (0.35 + 0.9 * blow + flutter).clamp(0.0, 1.45);
                let down = -Vec3::Y * swing.cos() + across * swing.sin();
                let hh = h / halves as f32;
                let centre = at + down * (hh * 0.5);
                out.flat(centre, local * (w * 0.5), down * (hh * 0.5), colour);
                at += down * hh;
            }
        }
    }
}

impl Kite {
    pub fn draw(&self, air: &Air, eye: Vec3, out: &mut Plates) {
        if !air.day || air.strength < 0.35 {
            return;
        }
        let ph = hash01(self.seed, 1) * 6.28;
        let rise = (air.strength.min(1.4) / 1.0).clamp(0.5, 1.25);
        let h = self.height * rise;
        let side = Vec3::Y.cross(air.dir).normalize();
        let t = air.t;
        let kite = self.anchor
            + air.dir * (h * 0.6)
            + Vec3::Y * h
            + side * ((t * 0.37 + ph).sin() * h * 0.14 + (t * 0.91 + ph * 2.0).sin() * h * 0.03)
            + Vec3::Y * ((t * 0.53 + ph).sin() * h * 0.05);
        if kite.distance(eye) > KITE_RANGE {
            return;
        }
        // The string: hangs a little under the straight line, drawn in a few
        // pieces near the eye and as one far off.
        let string = linear([60, 56, 50]);
        let near = kite.distance(eye) < 120.0;
        let pieces = if near { 10 } else { 3 };
        for k in 0..pieces {
            let s0 = k as f32 / pieces as f32;
            let s1 = (k + 1) as f32 / pieces as f32;
            cord(out, sagged(self.anchor, kite, 0.06, s0), sagged(self.anchor, kite, 0.06, s1), eye, if near { 0.025 } else { 0.045 }, string);
        }
        // The face looks back down the string, tipped toward the sky.
        let n = ((self.anchor - kite).normalize() * 0.7 + Vec3::Y * 0.3).normalize();
        let right = Vec3::Y.cross(n).normalize_or_zero();
        let up = n.cross(right).normalize();
        let rock = (t * 1.3 + ph).sin() * 0.22 + (t * 3.1 + ph).sin() * 0.06;
        let (rs, rc) = rock.sin_cos();
        let (r2, u2) = (right * rc + up * rs, up * rc - right * rs);
        // A diamond: the square turned on its point.
        let s = self.size * std::f32::consts::FRAC_1_SQRT_2;
        let dr = (r2 + u2) * (s * 0.5_f32.sqrt());
        let du = (u2 - r2) * (s * 0.5_f32.sqrt());
        out.flat(kite, dr, du, self.body);
        out.flat(kite + n * 0.02, dr * 0.5, du * 0.5, self.stripe);
        // The spars.
        let spar = linear([70, 52, 36]);
        cord(out, kite + u2 * self.size, kite - u2 * self.size, eye, 0.04, spar);
        cord(out, kite + r2 * self.size * 0.8, kite - r2 * self.size * 0.8, eye, 0.04, spar);
        // The tail streams downwind from the bottom point, waving.
        let mut at = kite - u2 * self.size;
        let bows = if near { 9 } else { 5 };
        for j in 1..=bows {
            let wave = (t * 3.4 - j as f32 * 0.9 + ph).sin() * 0.22 * (j as f32 / bows as f32 + 0.3);
            let next = at + (air.dir * 0.55 - Vec3::Y * 0.35 + side * wave).normalize() * (self.size * 0.42);
            cord(out, at, next, eye, 0.03, string);
            let c = if j % 2 == 0 { self.tail } else { self.stripe };
            out.flat(next, side * (self.size * 0.13), Vec3::Y * (self.size * 0.07), c);
            at = next;
        }
    }
}

impl Cloth {
    pub fn draw(&self, air: &Air, eye: Vec3, out: &mut Plates) {
        for f in &self.flags {
            if f.near(eye, FLAG_RANGE) {
                f.draw(air, eye, out);
            }
        }
        for k in &self.kites {
            k.draw(air, eye, out);
        }
    }
}
