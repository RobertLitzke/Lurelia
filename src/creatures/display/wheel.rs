//! `wheel`: the whole flock turns as one. Starlings in the air swing round
//! together, the turn rippling through the flock from one side to the other
//! so it bends and folds, the cloud stretching and pulling in, for a few
//! passes before they go on as they were. Shorebirds on the sand
//! (sanderlings, lapwings) get up together first, wheel low over the beach
//! and come back down.
//!
//! Params: `time` (seconds), `turns` (whole turns the flock goes round),
//! `ripple` (seconds the turn takes to cross the flock).

use super::{heading, Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::{Quat, Vec3};
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "wheel",
    rate: 6.0,
    holds: 0.4,
    cast,
    params: &["time", "turns", "ripple"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [6, 120], range: 60.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    use super::super::runner::Style;
    matches!(m.mover, Mover::Flocker { perched: false, soars: false } | Mover::Runner { grounded: true, style: Style::Surf | Style::Patter })
}

struct Wheel {
    centre: Vec3,
    vel: Vec3,
    /// Each bird's place in the flock at the start.
    offs: Vec<Vec3>,
    /// How far across the flock each bird is, 0 to 1, for the ripple.
    lag: Vec<f32>,
    time: f32,
    spin: f32,
    ripple: f32,
    /// Metres a flock on the ground rises before it wheels.
    lift: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len() as f32;
    let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / n;
    let offs: Vec<Vec3> = cast.iter().map(|m| m.pos - centre).collect();
    let a = rng.unit() * TAU;
    let across = Vec3::new(a.cos(), 0.0, a.sin());
    let d: Vec<f32> = offs.iter().map(|o| o.dot(across)).collect();
    let (lo, hi) = d.iter().fold((f32::MAX, f32::MIN), |(l, h), &x| (l.min(x), h.max(x)));
    let lag = d.iter().map(|x| (x - lo) / (hi - lo).max(0.1)).collect();
    let time = p.num("time", 10.0 + rng.unit() * 8.0, rng);
    let turns = p.num("turns", 1.0 + rng.unit(), rng) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
    let speed = cast[0].slow.max(4.0);
    let lift = if matches!(cast[0].mover, Mover::Runner { .. }) { 5.0 } else { 0.0 };
    Some(Box::new(Wheel { centre, vel: cast[0].facing() * speed, offs, lag, time, spin: turns * TAU / time, ripple: p.num("ripple", 1.2, rng), lift, t: 0.0 }))
}

impl Run for Wheel {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time + self.ripple {
            return Step::Done;
        }
        // The flock's heading turns with the wheel, so it sweeps round in a
        // wide curve as it goes.
        self.vel = Quat::from_rotation_y(self.spin * ctx.dt) * self.vel;
        self.centre += self.vel * ctx.dt;
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                // Each bird turns a beat after the one on the near side.
                let t = (self.t - self.lag[i] * self.ripple).clamp(0.0, self.time);
                let angle = self.spin * t;
                // The cloud stretches out along the turn and pulls in again.
                let squeeze = 1.0 + 0.35 * (TAU * t / self.time * 2.0).sin();
                let o = Quat::from_rotation_y(angle) * self.offs[i];
                let o = Vec3::new(o.x * squeeze, o.y + 2.0 * (t * 1.3 + self.lag[i] * 3.0).sin(), o.z / squeeze);
                // Up off the ground at the start and back down at the end.
                let end = self.time + self.ripple;
                let up = (self.t / 1.5).min(1.0).min((end - self.t) / 1.5).max(0.0);
                let o = if self.lift > 0.0 { Vec3::new(o.x, o.y.max(-0.5) * up, o.z) } else { o };
                let pos = self.centre + o + Vec3::Y * self.lift * up;
                Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, ..Default::default() }
            })
            .collect();
        Step::On("wheeling".into(), outs)
    }
}
