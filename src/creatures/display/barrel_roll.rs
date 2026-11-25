//! `barrel_roll`: a ray flying over the land rolls right over round its
//! long axis as it goes, once or twice, wings rippling, and levels out.
//!
//! Params: `rolls` (how many), `roll_s` (seconds one takes), `time`
//! (seconds flying straight on, rolls spread through it).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "barrel_roll",
    rate: 8.0,
    holds: 0.2,
    cast: Cast::solo,
    params: &["rolls", "roll_s", "time"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Circler { free: true }
}

struct BarrelRoll {
    pos: Vec3,
    dir: Vec3,
    speed: f32,
    rolls: u32,
    roll_s: f32,
    time: f32,
    t: f32,
    side: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let rolls = p.count("rolls", [1, 2], rng).max(1);
    let roll_s = p.num("roll_s", 1.6, rng).max(0.5);
    Some(Box::new(BarrelRoll { pos: m.pos, dir: m.facing(), speed: m.slow.max(1.0), rolls, roll_s, time: p.num("time", rolls as f32 * roll_s + 3.0, rng), t: 0.0, side: if rng.unit() < 0.5 { 1.0 } else { -1.0 } }))
}

impl Run for BarrelRoll {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let next = self.pos + self.dir * self.speed * ctx.dt;
        let floor = ctx.terrain.height(next.x, next.z).unwrap_or(next.y - 3.0).max(ctx.terrain.sea);
        self.pos = Vec3::new(next.x, next.y.max(floor + 1.5), next.z);
        // Rolls through the middle of the run.
        let lead = (self.time - self.rolls as f32 * self.roll_s) * 0.5;
        let u = ((self.t - lead) / (self.rolls as f32 * self.roll_s)).clamp(0.0, 1.0);
        let e = u * u * (3.0 - 2.0 * u);
        let pose = Overlay { roll: self.side * TAU * self.rolls as f32 * e, ..Default::default() };
        let label = if u > 0.0 && u < 1.0 { "rolling" } else { "gliding" };
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos: self.pos, yaw: heading(m.pos, self.pos, m.yaw) }, pose, ..Default::default() }])
    }
}
