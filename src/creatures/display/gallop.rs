//! `gallop`: the whole group bursts into a run together for no reason that
//! can be seen, tails up, swings round in a wide curve, and pulls up a
//! little way off to stand and graze again.
//!
//! Params: `time` (seconds running), `curve` (radians the run bends
//! through).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "gallop",
    rate: 5.0,
    holds: 0.6,
    cast,
    params: &["time", "curve"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 14], range: 35.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Gallop {
    yaw: f32,
    turn: f32,
    time: f32,
    t: f32,
    /// Each member's offset from the lead point, fixed at the start.
    offs: Vec<Vec3>,
    lead: Vec3,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len() as f32;
    let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / n;
    let time = p.num("time", 5.0 + rng.unit() * 4.0, rng);
    let curve = p.num("curve", 1.5, rng) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
    Some(Box::new(Gallop { yaw: cast[0].yaw, turn: curve / time, time, t: 0.0, offs: cast.iter().map(|m| (m.pos - centre).with_y(0.0) * 0.8).collect(), lead: centre }))
}

impl Run for Gallop {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        // A second to pull up after the run.
        if self.t > self.time + 1.5 {
            return Step::Done;
        }
        let running = self.t < self.time;
        let fast = cast.iter().map(|m| m.fast.max(m.slow)).fold(f32::MAX, f32::min);
        if running {
            self.yaw += self.turn * ctx.dt;
            self.lead += Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos()) * fast * 0.85 * ctx.dt;
        }
        let ahead = Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                if !running {
                    return Out { drive: Drive::Stay { face: None }, pose: Overlay { head: 0.3, ..Default::default() }, ..Default::default() };
                }
                let to = self.lead + self.offs[i] + ahead * 3.0;
                Out { drive: Drive::Go { to, speed: m.fast.max(m.slow) }, pose: Overlay { tail: 0.7, head: 0.15, ..Default::default() }, ..Default::default() }
            })
            .collect();
        Step::On(if running { "galloping" } else { "pulling up" }.into(), outs)
    }
}
