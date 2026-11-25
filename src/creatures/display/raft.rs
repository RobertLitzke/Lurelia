//! `raft`: two sea otters afloat on their backs side by side, holding paws
//! so they don't drift apart, turning slowly together on the swell, now and
//! then grooming their chests, until one lets go.
//!
//! Params: `time` (seconds).

use super::perch::water_top;
use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "raft",
    rate: 6.0,
    holds: 0.4,
    cast,
    params: &["time"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 20.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Raft {
    centre: Vec3,
    from: [Vec3; 2],
    gap: f32,
    yaw: f32,
    spin: f32,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let mid = (cast[0].pos + cast[1].pos) * 0.5;
    let top = water_top(ctx, mid.x, mid.z);
    if !ctx.terrain.wet(mid.x, mid.z) {
        return None;
    }
    Some(Box::new(Raft {
        centre: Vec3::new(mid.x, top - cast[0].height * 0.4, mid.z),
        from: [cast[0].pos, cast[1].pos],
        gap: (cast[0].half_w + cast[1].half_w) * 1.1,
        yaw: cast[0].yaw,
        spin: (rng.unit() - 0.5) * 0.3,
        time: p.num("time", 20.0 + rng.unit() * 20.0, rng),
        t: 0.0,
    }))
}

impl Run for Raft {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        self.yaw += self.spin * ctx.dt;
        let side = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
        let come = (self.t / 4.0).min(1.0);
        let swell = Vec3::Y * 0.05 * (self.t * 0.8).sin();
        let groom = (self.t % 10.0) > 7.0;
        let outs = (0..2)
            .map(|i| {
                let s = if i == 0 { -0.5 } else { 0.5 };
                let pos = self.from[i].lerp(self.centre + side * self.gap * s, come) + swell;
                let pose = if groom { Overlay { paw: 0.4 * (self.t * 8.0 + i as f32).sin(), head: 0.2, ..Default::default() } } else { Overlay::default() };
                Out { drive: Drive::Place { pos, yaw: self.yaw }, pose, ..Default::default() }
            })
            .collect();
        Step::On(if groom { "grooming, holding paws" } else { "rafting, holding paws" }.into(), outs)
    }
}
