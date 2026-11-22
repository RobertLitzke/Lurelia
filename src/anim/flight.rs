//! Bird motion: wings, tail, legs, head and body attitude from the flight a
//! bird is making. Flight paths stay in flock.rs, flit.rs and the circling
//! code in entities.rs; this only reads the bird's velocity, whether it is
//! perched and whether it is coming in to land.
//!
//! Wings are two-bone chains (`wing` role: inner, then tip). Signs: a wing's
//! "up" is roll * side, a wing folded back is yaw -side * 1.45; root roll +a
//! banks left, root pitch +a lifts the head.

use super::{Pose, Rig};
use crate::anim::{Plan, Role};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pattern {
    /// Steady beats (pigeons, ducks).
    Flap,
    /// A few beats, then a glide (crows, gulls, starlings).
    FlapGlide,
    /// A burst of beats, then wings folded and a dip (sparrows, finches).
    Bound,
    /// Wings held in a shallow V, a rare beat (eagles).
    Soar,
    /// Wings held out flat, a beat or two in half a minute (albatrosses).
    Sail,
    /// Fast shallow beats (moths, kestrels).
    Hover,
}

pub const PATTERNS: [&str; 6] = ["flap", "flap_glide", "bound", "soar", "sail", "hover"];

impl Pattern {
    pub fn parse(s: &str) -> Option<Pattern> {
        Some(match s {
            "flap" => Pattern::Flap,
            "flap_glide" => Pattern::FlapGlide,
            "bound" => Pattern::Bound,
            "soar" => Pattern::Soar,
            "sail" => Pattern::Sail,
            "hover" => Pattern::Hover,
            _ => return None,
        })
    }
}

/// Wingbeats per second from the wingspan: big wings beat slowly.
pub fn beat_for(rig: &Rig, voxel: f32) -> f32 {
    let mut lo = 16.0f32;
    let mut hi = 16.0f32;
    for b in &rig.bones {
        if b.role == Role::Wing {
            lo = lo.min(b.joint.x);
            hi = hi.max(b.joint.x);
        }
    }
    // Tips reach about as far past their joint as the inner wing does.
    let span = ((hi - lo) * 1.9).max(8.0) * voxel;
    (4.5 / span.powf(0.6)).clamp(1.5, 12.0)
}

pub struct Input {
    pub dt: f32,
    pub vel: Vec3,
    pub yaw: f32,
    pub perched: bool,
    /// Coming in to land: flare the wings and drop the legs.
    pub landing: bool,
    /// Clinging upright to a trunk (woodpeckers): body vertical, quick pecks.
    pub cling: bool,
}

#[derive(Clone, Debug)]
pub struct Flight {
    pub pattern: Pattern,
    pub beat: f32,
    pub phase: f32,
    /// 0 wings held, 1 beating.
    pub flapping: f32,
    /// 0 spread, 1 folded against the body.
    pub fold: f32,
    pub bank: f32,
    pub pitch: f32,
    pub flare: f32,
    /// Seconds of take-off beating left.
    pub burst: f32,
    /// In a flap-glide or bound cycle: true while gliding or folded.
    pub resting: bool,
    cycle: f32,
    pub look: (f32, f32),
    look_want: (f32, f32),
    look_timer: f32,
    pub peck: f32,
    peck_timer: f32,
    pub t: f32,
    was_perched: bool,
    /// Walking on the ground or paddling: stride phase in cycles, and how
    /// much (0 still, 1 walking).
    pub stride: f32,
    pub walk: f32,
    /// Metres covered in one stride cycle (two steps): 0.3 for a goose, a
    /// few centimetres for a sandpiper, whose legs go in a blur.
    pub stride_m: f32,
    /// Seconds between pecks while on its feet, least and spread: a
    /// sandpiper probes the sand all the time, a sparrow now and then.
    pub peck_every: (f32, f32),
    /// Hangs with its body tilted up while it hovers (hummingbirds).
    pub upright: bool,
    prev_yaw: f32,
    rng: Rng,
}

impl Flight {
    pub fn new(pattern: Pattern, beat: f32, seed: u32) -> Self {
        let mut f = Self {
            pattern,
            beat,
            phase: 0.0,
            flapping: 1.0,
            fold: 0.0,
            bank: 0.0,
            pitch: 0.0,
            flare: 0.0,
            burst: 0.0,
            resting: false,
            cycle: 0.0,
            look: (0.0, 0.0),
            look_want: (0.0, 0.0),
            look_timer: 0.0,
            peck: 0.0,
            peck_timer: 2.0,
            t: 0.0,
            was_perched: false,
            stride: 0.0,
            walk: 0.0,
            stride_m: 0.3,
            peck_every: (1.5, 4.0),
            upright: false,
            prev_yaw: f32::NAN,
            rng: Rng::walker(seed | 1),
        };
        f.phase = f.rand();
        f.cycle = f.rand() * 2.0;
        f
    }

    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }

    pub fn step(&mut self, inp: &Input) {
        let dt = inp.dt;
        let k = |rate: f32| (dt * rate).min(1.0);
        self.t += dt;
        if self.was_perched && !inp.perched {
            self.burst = 0.8;
        }
        self.was_perched = inp.perched;
        self.burst = (self.burst - dt).max(0.0);
        let fold_want = if inp.perched { 1.0 } else { 0.0 };
        self.fold += (fold_want - self.fold) * k(if inp.perched { 7.0 } else { 14.0 });

        // Turn rate from the heading, for banking.
        let dyaw = crate::util::wrap_angle(if self.prev_yaw.is_nan() { 0.0 } else { inp.yaw - self.prev_yaw });
        self.prev_yaw = inp.yaw;
        let turn = dyaw / dt.max(1e-4);
        let flat = Vec3::new(inp.vel.x, 0.0, inp.vel.z).length();
        let speed = inp.vel.length();

        // Beating or holding the wings.
        self.cycle -= dt;
        if self.cycle <= 0.0 {
            self.resting = !self.resting;
            let r = self.rand();
            self.cycle = match (self.pattern, self.resting) {
                (Pattern::FlapGlide, false) => (3.0 + r * 4.0) / self.beat,
                (Pattern::FlapGlide, true) => 0.6 + r * 1.2,
                (Pattern::Bound, false) => (2.0 + r * 2.0) / self.beat,
                (Pattern::Bound, true) => 0.2 + r * 0.25,
                (Pattern::Soar, false) => 2.0 / self.beat,
                (Pattern::Soar, true) => 5.0 + r * 8.0,
                (Pattern::Sail, false) => 1.5 / self.beat,
                (Pattern::Sail, true) => 20.0 + r * 20.0,
                _ => 1.0,
            };
        }
        let mut want: f32 = match self.pattern {
            Pattern::Flap | Pattern::Hover => 1.0,
            _ if self.resting => 0.0,
            _ => 1.0,
        };
        let vy = inp.vel.y;
        // Climbing hard always takes beats; flock jostling up and down doesn't.
        if vy > 2.0 && !matches!(self.pattern, Pattern::Soar | Pattern::Sail) {
            want = 1.0;
        }
        if vy < -1.5 && self.pattern != Pattern::Hover && self.pattern != Pattern::Bound {
            want = 0.0;
        }
        if self.burst > 0.0 {
            want = 1.0;
        }
        if inp.perched {
            want = 0.0;
        }
        self.flapping += (want - self.flapping) * k(10.0);
        let rate = self.beat * if self.pattern == Pattern::Hover { 1.6 } else { 1.0 } * (1.0 + 0.5 * (self.burst > 0.0) as u8 as f32 + 0.25 * (vy / 3.0).clamp(0.0, 1.0));
        self.phase = (self.phase + rate * dt).fract();

        // Attitude.
        let bank_want = if inp.perched { 0.0 } else { (turn * speed * 0.06).clamp(-0.9, 0.9) };
        self.bank += (bank_want - self.bank) * k(4.0);
        let climb = if flat > 0.3 || vy.abs() > 0.3 { vy.atan2(flat.max(0.3)) * 0.6 } else { 0.0 };
        let flare_want = if inp.landing && !inp.perched { 1.0 } else { 0.0 };
        self.flare += (flare_want - self.flare) * k(5.0);
        let pitch_want = if inp.perched && inp.cling {
            1.35
        } else if inp.perched {
            0.0
        } else if self.upright && flat < 2.0 {
            // Hovering: hung tilted up, beating forward and back. Climbing
            // straight up it stands nearly upright.
            0.75 * (1.0 - flat / 2.0) + 0.3 * (vy / 3.0).clamp(0.0, 1.0)
        } else {
            climb.clamp(-0.6, 0.5) + 0.5 * self.flare
        };
        self.pitch += (pitch_want - self.pitch) * k(5.0);

        // Perched: quick head turns and the odd peck.
        self.look_timer -= dt;
        if self.look_timer <= 0.0 {
            self.look_timer = if inp.perched { 0.4 + self.rand() * 1.6 } else { 1.0 + self.rand() * 2.0 };
            let (a, b) = (self.rand(), self.rand());
            self.look_want = if inp.perched { ((a - 0.5) * 2.2, (b - 0.5) * 0.6) } else { ((a - 0.5) * 0.3, 0.0) };
        }
        self.look.0 += (self.look_want.0 - self.look.0) * k(18.0);
        self.look.1 += (self.look_want.1 - self.look.1) * k(18.0);
        self.peck_timer -= dt;
        if inp.perched && self.peck_timer <= 0.0 {
            // A clinging woodpecker drums: bursts of quick blows.
            self.peck_timer = if inp.cling {
                0.12 + if self.rand() < 0.15 { 1.5 } else { 0.0 }
            } else {
                self.peck_every.0 + self.rand() * self.peck_every.1
            };
            self.peck = 1.0;
        }
        self.peck = (self.peck - dt * if inp.cling { 9.0 } else { 3.0 }).max(0.0);
        // Walking (a goose ashore) or paddling: legs stride with the pace,
        // about two steps per 0.3 m.
        let walk_want = if inp.perched && !inp.cling { (flat / 0.12).clamp(0.0, 1.0) } else { 0.0 };
        self.walk += (walk_want - self.walk) * k(8.0);
        // Short legs stride faster, up to eight cycles a second (a blur at 60
        // frames a second, not an alias).
        let cap = (1.2 / self.stride_m.max(0.05)).clamp(4.0, 8.0);
        self.stride = (self.stride + dt * (flat / self.stride_m.max(0.05)).clamp(0.6, cap) * (self.walk > 0.02) as u8 as f32).fract();
    }

    pub fn compose(&self, rig: &Rig, voxel: f32) -> Pose {
        let n = rig.bones.len();
        let mut p = Pose::rest(n);
        if rig.plan == Some(Plan::Ray) {
            // Rays that fly ripple their wings instead of beating them.
            super::motors::undulate(rig, &mut p, self.phase, 0.3, 1.0);
            p.bones[0].x += self.pitch * 0.5;
            p.bones[0].z += self.bank;
            return p;
        }
        let f = self.fold;
        let open = 1.0 - f;
        let (amp, tip_amp, dihedral) = match self.pattern {
            Pattern::Hover => (0.5, 0.3, 0.1),
            Pattern::Soar => (0.6, 0.4, 0.22),
            Pattern::Sail => (0.5, 0.35, 0.04),
            _ => (0.85, 0.6, 0.08),
        };
        // Big, slow wings beat shallower (a heron's, an eagle's).
        let deep = (self.beat / 3.5).clamp(0.5, 1.0);
        let (amp, tip_amp) = (amp * deep, tip_amp * deep);
        // Downstroke quicker than upstroke: a sine bent toward the bottom.
        let s = (TAU * self.phase).sin();
        let wave = if s < 0.0 { s * 1.15 } else { s * 0.9 };
        let tip_wave = (TAU * (self.phase - 0.12)).sin();
        let bounding_fold = if self.pattern == Pattern::Bound && self.resting { (1.0 - self.flapping) * 0.85 } else { 0.0 };
        for i in 0..n {
            let b = &rig.bones[i];
            let side = b.side;
            match b.role {
                Role::Wing => {
                    let inner = b.parent.is_none_or(|q| rig.bones[q].role != Role::Wing);
                    let tuck = (f + bounding_fold).min(1.0);
                    let spread_up = if inner {
                        amp * wave * self.flapping + dihedral + 0.5 * self.flare
                    } else {
                        tip_amp * tip_wave * self.flapping + 0.05
                    };
                    let spread_yaw = if inner { side * 0.4 * self.flare } else { 0.0 };
                    let (fold_yaw, fold_up) = if inner { (-side * 1.45, -0.25) } else { (-side * 2.8, 0.0) };
                    let up = spread_up * (1.0 - tuck) + fold_up * tuck;
                    let yaw = spread_yaw * (1.0 - tuck) + fold_yaw * tuck;
                    p.bones[i] = Vec3::new(0.0, yaw, side * up);
                }
                Role::Tail => {
                    p.bones[i].x = 0.5 * self.flare * open + 0.12 * (self.t * 3.0).sin() * f;
                    p.bones[i].y = -self.bank * 0.3;
                }
                Role::Leg => {
                    // Tucked back in flight, forward to land, straight when perched.
                    p.bones[i].x = -1.4 * open * (1.0 - self.flare) + 0.5 * self.flare * open;
                    // Striding: the two legs swing opposite ways.
                    let lag = if side < 0.0 { 0.0 } else { 0.5 };
                    p.bones[i].x += 0.75 * self.walk * f * (TAU * (self.stride + lag)).sin();
                }
                Role::Head => {
                    p.bones[i].y = self.look.0;
                    p.bones[i].x = self.look.1 - 0.9 * (self.peck * std::f32::consts::PI).sin() - 0.3 * self.pitch;
                    p.bones[i].z = -self.bank * 0.5;
                }
                Role::Root | Role::Spine | Role::Neck | Role::Jaw | Role::Arm | Role::Ear => {}
            }
        }
        p.bones[0].x = self.pitch;
        // A waddle: the body rolls onto each foot in turn.
        p.bones[0].z = self.bank + 0.07 * self.walk * f * (TAU * self.stride).sin();
        // Perched: stand on the feet, which hang below the model's floor.
        p.root.y = -rig.floor * voxel * f;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(vel: Vec3, perched: bool) -> Input {
        Input {
            dt: 1.0 / 60.0,
            vel,
            yaw: 0.0,
            perched,
            landing: false,
            cling: false,
        }
    }

    /// A hummingbird hangs tilted up while it hovers and flies level once it
    /// gets going; another bird hovering in place does not tilt.
    #[test]
    fn a_hummingbird_hovers_tilted_up() {
        let mut hum = Flight::new(Pattern::Hover, 12.0, 7);
        hum.upright = true;
        let mut moth = Flight::new(Pattern::Hover, 12.0, 7);
        for _ in 0..120 {
            hum.step(&input(Vec3::ZERO, false));
            moth.step(&input(Vec3::ZERO, false));
        }
        assert!(hum.pitch > 0.6, "{}", hum.pitch);
        assert!(moth.pitch.abs() < 0.05, "{}", moth.pitch);
        for _ in 0..120 {
            hum.step(&input(Vec3::new(0.0, 0.0, -6.0), false));
        }
        assert!(hum.pitch.abs() < 0.1, "{}", hum.pitch);
    }

    /// Short legs stride faster at the same pace, up to the cap.
    #[test]
    fn short_legs_stride_faster() {
        let run = |stride_m: f32| {
            let mut f = Flight::new(Pattern::Flap, 8.0, 3);
            f.stride_m = stride_m;
            let mut turns = 0.0;
            let mut last = f.stride;
            for _ in 0..600 {
                f.step(&input(Vec3::new(0.0, 0.0, -1.0), true));
                let d = f.stride - last;
                turns += if d < 0.0 { d + 1.0 } else { d };
                last = f.stride;
            }
            turns / 10.0
        };
        let (goose, piper) = (run(0.3), run(0.1));
        assert!((goose - 1.0 / 0.3).abs() < 0.3, "{goose}");
        assert!(piper > goose * 2.0 && piper <= 8.0 + 1e-3, "{piper}");
    }
}
