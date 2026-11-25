//! `pronk`: stiff-legged bounds, all four feet leaving the ground together,
//! back arched and head down, a few in a row; often taken up by the ones
//! near it. Springbok-style, on pronghorn and saiga; mule deer stot.
//!
//! Params: `bounds` (how many), `hop` (upward speed of each, m/s), `gap`
//! (seconds between bounds).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "pronk",
    rate: 6.0,
    holds: 0.6,
    cast,
    params: &["bounds", "hop", "gap"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [1, 4], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Pronk {
    dir: Vec3,
    bounds: u32,
    hop: f32,
    gap: f32,
    /// Per member: seconds to its next bound, bounds done.
    next: Vec<f32>,
    done: Vec<u32>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let a = rng.unit() * 1.2 - 0.6;
    let f = cast[0].facing();
    let dir = Vec3::new(f.x * a.cos() - f.z * a.sin(), 0.0, f.x * a.sin() + f.z * a.cos());
    Some(Box::new(Pronk {
        dir,
        bounds: p.count("bounds", [3, 7], rng).max(1),
        hop: p.num("hop", 3.2, rng),
        gap: p.num("gap", 0.75, rng).max(0.35),
        // The others take it up a moment later each.
        next: (0..cast.len()).map(|i| 0.2 + i as f32 * (0.4 + rng.unit() * 0.4)).collect(),
        done: vec![0; cast.len()],
        t: 0.0,
    }))
}

impl Run for Pronk {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.done.iter().all(|&d| d >= self.bounds) && self.t > 1.0 {
            return Step::Done;
        }
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                if self.done[i] >= self.bounds {
                    return Out { drive: Drive::Stay { face: None }, pose: Overlay { head: 0.2, ..Default::default() }, ..Default::default() };
                }
                self.next[i] -= ctx.dt;
                let mut hop = 0.0;
                if self.next[i] <= 0.0 {
                    hop = self.hop;
                    self.next[i] = self.gap;
                    self.done[i] += 1;
                }
                // Arched, head down, tail up, legs held straight under it.
                let pose = Overlay { head: -0.45, rear: -0.06, tail: 0.6, ..Default::default() };
                let to = m.pos + self.dir * 3.0;
                Out { drive: Drive::Go { to, speed: m.slow * 1.5 }, pose, hop, ..Default::default() }
            })
            .collect();
        Step::On("pronking".into(), outs)
    }
}
