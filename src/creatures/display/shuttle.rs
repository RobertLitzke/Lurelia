//! `shuttle`: a male hummingbird's close display. In front of its perch it
//! swings side to side on a short arc like a pendulum, low in the middle and
//! up at each end, fast, a dozen times or more, tail flaring at the turns;
//! then it darts back to its twig (hummer.rs takes it back).
//!
//! Params: `swings` (how many), `width` (metres end to end), `swing_s`
//! (seconds one swing takes).

use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "shuttle",
    rate: 8.0,
    holds: 0.4,
    cast: Cast::solo,
    params: &["swings", "width", "swing_s"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Hummer { perched: true }
}

struct Shuttle {
    centre: Vec3,
    side: Vec3,
    yaw: f32,
    width: f32,
    swings: u32,
    swing_s: f32,
    t: f32,
    /// Seconds getting out to the arc, before the first swing.
    lead: f32,
    from: Vec3,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let f = m.facing();
    // Out in front of the twig, facing back at it (where a female would sit).
    let centre = m.pos + f * 1.2 - Vec3::Y * 0.3;
    Some(Box::new(Shuttle {
        centre,
        side: Vec3::new(-f.z, 0.0, f.x),
        yaw: m.yaw + PI,
        width: p.num("width", 1.0, rng),
        swings: p.count("swings", [8, 16], rng).max(1),
        swing_s: p.num("swing_s", 0.35, rng).max(0.12),
        t: 0.0,
        lead: 0.5,
        from: m.pos,
    }))
}

impl Run for Shuttle {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let half = self.width * 0.5;
        if self.t < self.lead {
            let u = self.t / self.lead;
            let pos = self.from.lerp(self.centre - self.side * half + Vec3::Y * half * 0.6, u);
            return Step::On("out in front".into(), vec![Out { drive: Drive::Place { pos, yaw: self.yaw }, ..Default::default() }]);
        }
        let s = (self.t - self.lead) / self.swing_s;
        if s >= self.swings as f32 {
            return Step::Done;
        }
        // Each swing goes end to end, cos over half a turn: -1 to 1 and back.
        let x = -(PI * s).cos();
        let pos = self.centre + self.side * half * x + Vec3::Y * half * 0.6 * x * x;
        let pose = Overlay { tail: 0.6 * x.abs().powi(4), ..Default::default() };
        Step::On(format!("shuttling, {} of {}", s as u32 + 1, self.swings), vec![Out { drive: Drive::Place { pos, yaw: self.yaw }, pose, ..Default::default() }])
    }
}
