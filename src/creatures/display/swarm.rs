//! `swarm`: bats at dusk pouring into a dense spiral over one spot, going
//! round and round together, the column rising and tightening, before they
//! spread out to feed.
//!
//! Params: `time` (seconds), `rise` (metres the column climbs), `radius`
//! (metres across the spiral).

use super::{heading, Cast, Ctx, Drive, From, Hop, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "swarm",
    rate: 8.0,
    holds: 0.0,
    cast,
    params: &["time", "rise", "radius"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 60], range: 60.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Circler { free: true }
}

struct Swarm {
    centre: Vec3,
    radius: f32,
    rise: f32,
    time: f32,
    /// Each one's angle, height share and speed round.
    birds: Vec<(f32, f32, f32)>,
    join: Vec<Option<Hop>>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len() as f32;
    let mid = cast.iter().map(|m| m.pos).sum::<Vec3>() / n;
    let g = ctx.terrain.height(mid.x, mid.z).unwrap_or(mid.y - 5.0).max(ctx.terrain.sea);
    let centre = Vec3::new(mid.x, g + 4.0, mid.z);
    let radius = p.num("radius", 5.0, rng);
    let birds: Vec<(f32, f32, f32)> = cast.iter().map(|_| (rng.unit() * TAU, rng.unit(), 1.2 + rng.unit() * 0.8)).collect();
    let mut s = Swarm { centre, radius, rise: p.num("rise", 12.0, rng), time: p.num("time", 15.0 + rng.unit() * 10.0, rng), birds, join: Vec::new(), t: 0.0 };
    s.join = cast.iter().enumerate().map(|(i, m)| Some(Hop::new(m.pos, s.at(i), m.slow.max(4.0)))).collect();
    Some(Box::new(s))
}

impl Swarm {
    fn at(&self, i: usize) -> Vec3 {
        let (a, h, _) = self.birds[i];
        let u = (self.t / self.time).min(1.0);
        // The column narrows and climbs as it goes on.
        let r = self.radius * (1.0 - 0.5 * u) * (0.5 + 0.5 * h);
        self.centre + Vec3::new(a.cos() * r, self.rise * u * (0.3 + 0.7 * h) + h * 3.0, a.sin() * r)
    }
}

impl Run for Swarm {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let mut outs = Vec::with_capacity(cast.len());
        for (i, m) in cast.iter().enumerate() {
            let (a, _, w) = self.birds[i];
            self.birds[i].0 = a + w * ctx.dt * (1.0 + (self.t / self.time));
            let to = self.at(i);
            let pos = match &mut self.join[i] {
                Some(hop) => {
                    hop.to = to;
                    let (p, done) = hop.step(ctx.dt);
                    if done {
                        self.join[i] = None;
                    }
                    p
                }
                None => to,
            };
            outs.push(Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, ..Default::default() });
        }
        Step::On("swarming".into(), outs)
    }
}
