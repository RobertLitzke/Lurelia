//! `dig`: head down and front end low, scraping fast with the fore feet,
//! throwing the earth back; a pause with its nose in the hole, then more.
//!
//! Params: `time` (seconds), `beat` (strokes a second).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "dig",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["time", "beat"],
    words: &[],
    approach: false,
    roles: &[Role::Leg],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Dig {
    time: f32,
    beat: f32,
    t: f32,
    yaw: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(Dig { time: p.num("time", 8.0 + rng.unit() * 8.0, rng), beat: p.num("beat", 4.0, rng), t: 0.0, yaw: cast[0].yaw }))
}

impl Run for Dig {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        // Two seconds on, one with the nose in, round again.
        let sniff = (t % 3.0) > 2.0;
        let pose = if sniff {
            Overlay { head: -0.9, rear: -0.15, plant: 1.0, ..Default::default() }
        } else {
            Overlay { head: -0.6, rear: -0.15, plant: 1.0, paw: 0.7 * (t * self.beat * TAU).sin(), tail: 0.3, ..Default::default() }
        };
        Step::On(if sniff { "nose in the hole" } else { "digging" }.into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
    }
}
