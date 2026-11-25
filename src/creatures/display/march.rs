//! `march`: flamingos packed tight, necks up, walking together one way and
//! then all wheeling and going another, heads flagging side to side in
//! time.
//!
//! Params: `time` (seconds), `legs` (how many times the group turns).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "march",
    rate: 6.0,
    holds: 0.6,
    cast,
    params: &["time", "legs"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [4, 14], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct March {
    centre: Vec3,
    dir: Vec3,
    /// Each member's place in the pack, as offsets across and along.
    slots: Vec<(f32, f32)>,
    leg: f32,
    legs: u32,
    done: u32,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len();
    let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / n as f32;
    // A tight block, rows of three or four, a body's width apart.
    let w = cast[0].half_w * 2.4;
    let row = (n as f32).sqrt().ceil() as usize;
    let slots = (0..n).map(|i| (((i % row) as f32 - (row - 1) as f32 * 0.5) * w, -((i / row) as f32) * w * 1.2)).collect();
    let legs = p.count("legs", [3, 6], rng).max(1);
    let time = p.num("time", 20.0 + rng.unit() * 10.0, rng);
    Some(Box::new(March { centre, dir: cast[0].facing(), slots, leg: time / legs as f32, legs, done: 0, time, t: 0.0 }))
}

impl Run for March {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        if self.t > self.leg * (self.done + 1) as f32 {
            self.done += 1;
            // All wheel together, a sharp turn one way or the other.
            let a = (0.8 + rng.unit() * 1.4) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
            let y = self.dir.x.atan2(self.dir.z) + a;
            self.dir = Vec3::new(y.sin(), 0.0, y.cos());
        }
        let slow = cast.iter().map(|m| m.slow).fold(f32::MAX, f32::min);
        self.centre += self.dir * slow * 0.8 * ctx.dt;
        let side = Vec3::new(-self.dir.z, 0.0, self.dir.x);
        let flag = 0.45 * (self.t * 2.4).sin();
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let (x, z) = self.slots[i];
                let to = self.centre + side * x + self.dir * (z + 1.5);
                Out { drive: Drive::Go { to, speed: m.slow }, pose: Overlay { head: 0.35, turn: flag, ..Default::default() }, ..Default::default() }
            })
            .collect();
        Step::On(format!("marching, leg {} of {}", self.done + 1, self.legs), outs)
    }
}
