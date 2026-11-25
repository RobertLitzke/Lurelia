//! `tandem`: two dragonflies joined, the male in front holding the female
//! behind his head, flying together low over the water; every few metres
//! they dip and she touches her tail to the surface to lay.
//!
//! Params: `time` (seconds), `dips` (seconds between dips).

use super::{heading, Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "tandem",
    rate: 8.0,
    holds: 0.3,
    cast,
    params: &["time", "dips"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 15.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Buzzer
}

struct Tandem {
    pos: Vec3,
    dir: f32,
    surface: f32,
    time: f32,
    dips: f32,
    gap: f32,
    t: f32,
    trail: Vec3,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let a = &cast[0];
    let surface = ctx.terrain.inland_water(a.pos.x, a.pos.z).map(|w| w.0).or_else(|| ctx.terrain.wet(a.pos.x, a.pos.z).then_some(ctx.terrain.sea))?;
    Some(Box::new(Tandem {
        pos: a.pos,
        dir: rng.unit() * TAU,
        surface,
        time: p.num("time", 10.0 + rng.unit() * 8.0, rng),
        dips: p.num("dips", 1.5, rng).max(0.4),
        gap: (a.snout * 2.0).max(0.04),
        t: 0.0,
        trail: cast[1].pos,
    }))
}

impl Run for Tandem {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        // Wandering low over the water, turning back from the bank.
        self.dir += 0.8 * (self.t * 0.7).sin() * ctx.dt;
        let ahead = Vec3::new(self.dir.sin(), 0.0, self.dir.cos());
        let next = self.pos + ahead * 1.2 * ctx.dt;
        if ctx.terrain.wet(next.x, next.z) {
            self.pos = next;
        } else {
            self.dir += PI * ctx.dt * 2.0;
        }
        let dip = ((self.t / self.dips).fract() * PI * 2.0).sin().max(0.0).powi(4);
        let y = self.surface + 0.5 - 0.42 * dip;
        let lead = Vec3::new(self.pos.x, y, self.pos.z);
        // She follows a body behind him, joined.
        let back = (self.trail - lead).with_y(0.0).normalize_or(-ahead);
        self.trail = lead + back * self.gap - Vec3::Y * 0.02;
        let a = Out { drive: Drive::Place { pos: lead, yaw: heading(cast[0].pos, lead, cast[0].yaw) }, ..Default::default() };
        let b = Out { drive: Drive::Place { pos: self.trail, yaw: heading(self.trail, lead, cast[1].yaw) }, ..Default::default() };
        Step::On(if dip > 0.5 { "dipping to lay" } else { "in tandem" }.into(), vec![a, b])
    }
}
