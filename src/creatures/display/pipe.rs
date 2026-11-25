//! `pipe`: two oystercatchers run side by side, heads down and bills
//! pointed at the ground, calling, along the shore and back, sometimes a
//! third joining.
//!
//! Params: `runs` (how many runs), `run_m` (metres one run goes).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "pipe",
    rate: 6.0,
    holds: 0.6,
    cast,
    params: &["runs", "run_m"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 3], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Pipe {
    from: Vec3,
    dir: Vec3,
    run_m: f32,
    runs: u32,
    done: u32,
    /// How far along the current run the pair is, metres.
    s: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / cast.len() as f32;
    Some(Box::new(Pipe { from: centre, dir: cast[0].facing(), run_m: p.num("run_m", 10.0, rng), runs: p.count("runs", [2, 4], rng).max(1), done: 0, s: 0.0, t: 0.0 }))
}

impl Run for Pipe {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let speed = cast.iter().map(|m| m.slow).fold(f32::MAX, f32::min) * 1.4;
        self.s += speed * ctx.dt;
        if self.s > self.run_m {
            self.done += 1;
            if self.done >= self.runs {
                return Step::Done;
            }
            self.from += self.dir * self.run_m;
            self.dir = -self.dir;
            self.s = 0.0;
        }
        let side = Vec3::new(-self.dir.z, 0.0, self.dir.x);
        let w = cast[0].half_w * 2.5;
        let n = cast.len() as f32;
        let call = (self.t * 8.0).sin().max(0.0);
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let to = self.from + self.dir * (self.s + 1.0) + side * (i as f32 - (n - 1.0) * 0.5) * w;
                Out { drive: Drive::Go { to, speed: m.slow * 1.6 }, pose: Overlay { head: -0.6, jaw: 0.3 * call, ..Default::default() }, ..Default::default() }
            })
            .collect();
        Step::On(format!("piping, run {} of {}", self.done + 1, self.runs), outs)
    }
}
