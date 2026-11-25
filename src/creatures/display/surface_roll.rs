//! `surface_roll`: a gar or a catfish comes slowly up, breaks the surface
//! with its back, rolls over taking a gulp of air, and sinks away again.
//!
//! Params: `rolls` (how many), `gap` (seconds under between them).

use super::perch::water_top;
use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "surface_roll",
    rate: 8.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["rolls", "gap"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct SurfaceRoll {
    home: Vec3,
    dir: Vec3,
    yaw: f32,
    rolls: u32,
    gap: f32,
    t: f32,
}

/// Seconds one roll takes, up and down.
const ROLL_S: f32 = 4.0;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    Some(Box::new(SurfaceRoll { home: m.pos, dir: m.facing(), yaw: m.yaw, rolls: p.count("rolls", [1, 3], rng).max(1), gap: p.num("gap", 3.0, rng).max(0.0), t: 0.0 }))
}

impl Run for SurfaceRoll {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        let cycle = ROLL_S + self.gap;
        let k = (self.t / cycle) as u32;
        if k >= self.rolls {
            return Step::Done;
        }
        let u = ((self.t % cycle) / ROLL_S).min(1.0);
        let top = water_top(ctx, m.pos.x, m.pos.z);
        // Up in a slow curve, the back just breaking the surface, and down.
        let up = (PI * u).sin();
        let along = self.home + self.dir * (self.t * 0.5);
        let y = self.home.y + (top - m.height * 0.3 - self.home.y) * up;
        let pos = Vec3::new(along.x, y, along.z);
        let rolling = (u - 0.35) / 0.3;
        let pose = Overlay { roll: if (0.0..1.0).contains(&rolling) { TAU * 0.5 * (PI * rolling).sin() } else { 0.0 }, jaw: if (0.0..1.0).contains(&rolling) { 0.4 } else { 0.0 }, ..Default::default() };
        let label = if u >= 1.0 { "under" } else if (0.0..1.0).contains(&rolling) { "rolling at the top" } else { "coming up" };
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: self.yaw }, pose, ..Default::default() }])
    }
}
