//! `courtship`: a roadrunner courting. It runs up to another one, stops
//! close in front of it with its tail raised and wagging slowly side to
//! side, wings drooped, bows and offers something in its bill (its entry's `food`, a lizard),
//! and circles round it with little jumps; then they go their ways.
//!
//! Params: `time` (seconds of display), `circles` (times it goes round).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "courtship",
    rate: 6.0,
    holds: 0.4,
    cast,
    params: &["time", "circles"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 40.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Runner { grounded: true, .. })
}

struct Courtship {
    pos: Vec3,
    /// 0 running up, 1 in front, 2 circling.
    phase: u8,
    time: f32,
    circles: f32,
    ang: f32,
    r: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let r = (a.snout + b.snout) * 1.2 + 0.2;
    Some(Box::new(Courtship { pos: a.pos, phase: 0, time: p.num("time", 4.0 + rng.unit() * 3.0, rng), circles: p.num("circles", 1.0 + rng.unit(), rng), ang: 0.0, r, t: 0.0 }))
}

impl Courtship {
    /// Walk `pos` toward `to` at `speed` over the ground; whether it is there.
    fn walk(&mut self, to: Vec3, speed: f32, ctx: &Ctx) -> bool {
        let d = (to - self.pos).with_y(0.0);
        let step = speed * ctx.dt;
        if d.length() <= step {
            self.pos = to;
            return true;
        }
        let next = self.pos + d.normalize() * step;
        let g = ctx.terrain.drawn(next.x, next.z, next.y + 1.0).unwrap_or(next.y);
        self.pos = Vec3::new(next.x, g, next.z);
        false
    }
}

impl Run for Courtship {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let (a, b) = (&cast[0], &cast[1]);
        self.t += ctx.dt;
        let other = Out { drive: Drive::Sit { pos: b.pos, yaw: face(b.pos, a.pos) }, pose: Overlay { head: 0.1, ..Default::default() }, ..Default::default() };
        let (pose, label) = match self.phase {
            0 => {
                let to = b.pos + (a.pos - b.pos).with_y(0.0).normalize_or(Vec3::X) * self.r;
                if self.walk(to, a.fast.max(a.slow), ctx) || self.t > 15.0 {
                    self.phase = 1;
                    self.t = 0.0;
                    let d = self.pos - b.pos;
                    self.ang = d.z.atan2(d.x);
                }
                (Overlay { tail: 0.4, ..Default::default() }, "running up")
            }
            1 => {
                if self.t > self.time {
                    self.phase = 2;
                    self.t = 0.0;
                }
                let bow = (self.t * 1.5).sin().max(0.0);
                (Overlay { tail: 1.0, wag: 0.5 * (self.t * 2.0).sin(), wings: 0.25, head: -0.5 * bow, rear: -0.2 * bow, jaw: 0.15, ..Default::default() }, "tail up, bowing")
            }
            _ => {
                self.ang += a.slow.max(1.0) * ctx.dt / self.r;
                if self.t * a.slow.max(1.0) / self.r > self.circles * TAU {
                    return Step::Done;
                }
                let to = b.pos + Vec3::new(self.ang.cos(), 0.0, self.ang.sin()) * self.r;
                self.walk(to, a.slow.max(1.0) * 2.0, ctx);
                // Little jumps as it goes round.
                self.pos.y += 0.08 * (self.t * 9.0).sin().max(0.0);
                (Overlay { tail: 1.0, wag: 0.5 * (self.t * 2.0).sin(), wings: 0.25, ..Default::default() }, "circling")
            }
        };
        let yaw = if self.phase == 2 { face(self.pos, self.pos + Vec3::new(-self.ang.sin(), 0.0, self.ang.cos())) } else { face(self.pos, b.pos) };
        Step::On(label.into(), vec![Out { drive: Drive::Sit { pos: self.pos, yaw }, pose, held: true, ..Default::default() }, other])
    }
}
