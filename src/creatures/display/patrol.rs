//! `patrol`: a big fish (a reef shark, a barracuda, a grouper) cruising a
//! slow beat: a long oval along the reef edge at its depth, round and round,
//! steady and unhurried, then off.
//!
//! Params: `laps` (how many), `length` (metres along the oval), `width`
//! (metres across).

use super::{face, Cast, Ctx, Drive, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "patrol",
    rate: 6.0,
    holds: 0.2,
    cast: Cast::solo,
    params: &["laps", "length", "width"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Patrol {
    centre: Vec3,
    along: Vec3,
    across: Vec3,
    a: f32,
    b: f32,
    ang: f32,
    speed: f32,
    laps: f32,
    gone: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let along = m.facing();
    let across = Vec3::new(-along.z, 0.0, along.x);
    let a = p.num("length", 20.0, rng) * 0.5;
    let b = p.num("width", 6.0, rng) * 0.5;
    // It starts at the end of the oval it is at, heading along it.
    let centre = m.pos - across * b;
    // Keep the course over water at least as deep as the fish is long.
    for k in 0..8 {
        let q = centre + along * a * ((k as f32 / 8.0) * TAU).cos() + across * b * ((k as f32 / 8.0) * TAU).sin();
        if ctx.terrain.bed(q.x, q.z) > m.pos.y - m.height * 0.5 {
            return None;
        }
    }
    Some(Box::new(Patrol { centre, along, across, a, b, ang: std::f32::consts::FRAC_PI_2, speed: m.slow.max(0.5) * 0.7, laps: p.num("laps", 1.0 + rng.unit() * 2.0, rng), gone: 0.0 }))
}

impl Run for Patrol {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let r = (self.a + self.b) * 0.5;
        let d = self.speed * ctx.dt / r;
        self.ang -= d;
        self.gone += d;
        if self.gone > self.laps * TAU {
            return Step::Done;
        }
        let at = |t: f32| self.centre + self.along * self.a * t.cos() + self.across * self.b * t.sin();
        let pos = at(self.ang);
        let next = at(self.ang - 0.05);
        Step::On("patrolling".into(), vec![Out { drive: Drive::Place { pos, yaw: face(pos, next) }, ..Default::default() }])
    }
}
