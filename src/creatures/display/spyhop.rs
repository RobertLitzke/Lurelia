//! `spyhop`: a whale lying at the surface rises straight up, head out of
//! the water to its flippers, hangs there turning slowly to look round, and
//! slides back down. With `slaps` it then rolls onto its side and slaps the
//! water with its flukes, again and again (humpbacks).
//!
//! Params: `hold` (seconds up), `slaps` (tail slaps after; 0 for none).

use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

pub const KIND: Kind = Kind {
    name: "spyhop",
    rate: 8.0,
    holds: 0.0,
    cast: Cast::solo,
    params: &["hold", "slaps"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Whale { surface: true }
}

struct Spyhop {
    at: Vec3,
    yaw: f32,
    length: f32,
    hold: f32,
    slaps: u32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    Some(Box::new(Spyhop { at: m.pos, yaw: m.yaw, length: (m.snout * 2.0).max(m.height * 3.0), hold: p.num("hold", 4.0 + rng.unit() * 4.0, rng), slaps: p.count("slaps", [0, 0], rng), t: 0.0 }))
}

impl Run for Spyhop {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let (rise, sink) = (3.0, 3.0);
        let up_end = rise + self.hold + sink;
        let slap_s = 1.6;
        let end = up_end + self.slaps as f32 * slap_s + if self.slaps > 0 { 1.5 } else { 0.0 };
        if self.t > end {
            return Step::Done;
        }
        let t = self.t;
        let (up, label) = if t < rise {
            let u = t / rise;
            (u * u * (3.0 - 2.0 * u), "rising")
        } else if t < rise + self.hold {
            (1.0, "looking round")
        } else if t < up_end {
            let u = (up_end - t) / sink;
            (u * u * (3.0 - 2.0 * u), "sliding back")
        } else {
            (0.0, "slapping its tail")
        };
        // Nose straight up, a third of its length clear of the water; the
        // body's pivot is at its middle, so that much higher.
        let pos = self.at + Vec3::Y * self.length * 0.2 * up;
        let turn = if (rise..rise + self.hold).contains(&t) { 0.5 * ((t - rise) / self.hold * PI).sin() } else { 0.0 };
        let mut pose = Overlay { rear: FRAC_PI_2 * 0.95 * up, ..Default::default() };
        if t >= up_end && self.slaps > 0 {
            // On its side, flukes lifted and brought down hard each slap.
            let k = (t - up_end) / slap_s;
            pose.roll = 1.3 * (k / 0.5).min(1.0);
            pose.tail = if k < self.slaps as f32 { 1.2 * (PI * k.fract()).sin().powf(0.5) } else { 0.0 };
        }
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: self.yaw + turn }, pose, ..Default::default() }])
    }
}
