//! `bury`: a ray settles on the bottom, shuffles its wing edges to throw
//! sand over its back until only its eyes show, lies there, then lifts off
//! in a little cloud and swims on.
//!
//! Params: `time` (seconds buried).

use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "bury",
    rate: 6.0,
    holds: 0.5,
    cast: Cast::solo,
    params: &["time"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Bury {
    from: Vec3,
    down: Vec3,
    yaw: f32,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let bed = ctx.terrain.bed(m.pos.x, m.pos.z);
    Some(Box::new(Bury { from: m.pos, down: Vec3::new(m.pos.x, bed + m.height * 0.15, m.pos.z) + m.facing() * 0.5, yaw: m.yaw, time: p.num("time", 8.0 + rng.unit() * 8.0, rng), t: 0.0 }))
}

impl Run for Bury {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let (settle, shuffle) = (2.5, 2.0);
        let end = settle + shuffle + self.time + 1.5;
        if self.t > end {
            return Step::Done;
        }
        let t = self.t;
        let (pos, pose, label) = if t < settle {
            let u = t / settle;
            (self.from.lerp(self.down, u * u * (3.0 - 2.0 * u)), Overlay::default(), "settling")
        } else if t < settle + shuffle {
            // Wing edges flicking, the body working itself down.
            let s = (t * 14.0).sin();
            (self.down - Vec3::Y * 0.03 * (t - settle) / shuffle, Overlay { roll: 0.08 * s, wings: 0.2 * s.abs(), ..Default::default() }, "shuffling in")
        } else if t < settle + shuffle + self.time {
            (self.down - Vec3::Y * 0.03, Overlay::default(), "buried")
        } else {
            let u = (t - settle - shuffle - self.time) / 1.5;
            (self.down.lerp(self.from, u), Overlay::default(), "lifting off")
        };
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: self.yaw }, pose, ..Default::default() }])
    }
}
