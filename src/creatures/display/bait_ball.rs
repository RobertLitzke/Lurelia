//! `bait_ball`: a school pulls in tight into a ball and mills round and
//! round in it, every fish on its own lap, the ball turning as a whole and
//! drifting; then it loosens and swims on.
//!
//! Params: `time` (seconds), `radius` (metres across the ball, at most),
//! `spin` (laps a second at the ball's edge).

use super::perch::water_top;
use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::{Quat, Vec3};
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "bait_ball",
    rate: 6.0,
    holds: 0.3,
    cast,
    params: &["time", "radius", "spin"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [8, 200], range: 30.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct BaitBall {
    centre: Vec3,
    radius: f32,
    spin: f32,
    time: f32,
    /// Each fish's place on the ball: a direction from the centre, and how
    /// far out (0 to 1).
    slots: Vec<(Vec3, f32)>,
    from: Vec<Vec3>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len();
    let mid = cast.iter().map(|m| m.pos).sum::<Vec3>() / n as f32;
    let top = water_top(ctx, mid.x, mid.z);
    let bed = ctx.terrain.bed(mid.x, mid.z);
    let want = (n as f32).cbrt() * cast[0].height.max(0.1) * 1.2;
    let radius = p.num("radius", 3.0, rng).min(want).min((top - bed) * 0.4).max(0.4);
    let centre = Vec3::new(mid.x, ((top + bed) * 0.5).clamp(bed + radius + 0.3, top - radius - 0.3), mid.z);
    // Spread evenly over a sphere (a golden-angle spiral), some deeper in.
    let slots = (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - y * y).sqrt();
            let a = i as f32 * PI * (3.0 - 5f32.sqrt());
            (Vec3::new(a.cos() * r, y * 0.8, a.sin() * r), 0.6 + 0.4 * rng.unit())
        })
        .collect();
    Some(Box::new(BaitBall { centre, radius, spin: p.num("spin", 0.25, rng), time: p.num("time", 15.0 + rng.unit() * 10.0, rng), slots, from: cast.iter().map(|m| m.pos).collect(), t: 0.0 }))
}

impl Run for BaitBall {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let gather = (self.t / 3.0).min(1.0).min((self.time - self.t) / 3.0).max(0.0);
        let g = gather * gather * (3.0 - 2.0 * gather);
        let turn = Quat::from_rotation_y(self.t * self.spin * TAU);
        self.centre += Vec3::new((self.t * 0.2).sin(), 0.0, (self.t * 0.15).cos()) * 0.2 * ctx.dt;
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let (dir, out) = self.slots[i];
                // Inner fish lap faster than the outer ones.
                let lap = Quat::from_rotation_y(self.t * self.spin * TAU * (1.6 - out));
                let on_ball = self.centre + turn * lap * dir * self.radius * out;
                let pos = self.from[i].lerp(on_ball, g);
                // Facing along its lap.
                let along = Vec3::Y.cross(pos - self.centre);
                let yaw = if along.length() > 1e-3 { super::face(pos, pos + along) } else { m.yaw };
                Out { drive: Drive::Place { pos, yaw }, ..Default::default() }
            })
            .collect();
        Step::On(if gather >= 1.0 { "milling in a ball" } else { "balling up" }.into(), outs)
    }
}
