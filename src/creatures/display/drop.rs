//! `drop`: a school over the reef drops into the coral all at once, every
//! fish diving for the bottom in the same instant, hides there a few
//! seconds, and comes back up to where it was.
//!
//! Params: `hide` (seconds down), `down_s` (seconds the drop takes).

use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "drop",
    rate: 6.0,
    holds: 0.0,
    cast,
    params: &["hide", "down_s"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [4, 150], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Drop {
    from: Vec<Vec3>,
    down: Vec<Vec3>,
    yaws: Vec<f32>,
    hide: f32,
    down_s: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let down = cast.iter().map(|m| Vec3::new(m.pos.x, ctx.terrain.bed(m.pos.x, m.pos.z) + m.height.max(0.05) * 0.8, m.pos.z)).collect();
    Some(Box::new(Drop {
        from: cast.iter().map(|m| m.pos).collect(),
        down,
        yaws: cast.iter().map(|m| m.yaw).collect(),
        hide: p.num("hide", 3.0 + rng.unit() * 4.0, rng),
        down_s: p.num("down_s", 0.6, rng).max(0.2),
        t: 0.0,
    }))
}

impl Run for Drop {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let up_s = self.down_s * 5.0;
        let end = self.down_s + self.hide + up_s;
        if self.t > end {
            return Step::Done;
        }
        let u = if self.t < self.down_s {
            let u = self.t / self.down_s;
            u * u
        } else if self.t < self.down_s + self.hide {
            1.0
        } else {
            let u = (end - self.t) / up_s;
            u * u * (3.0 - 2.0 * u)
        };
        let outs = (0..cast.len())
            .map(|i| {
                // A little jitter while it hides in the coral.
                let pos = self.from[i].lerp(self.down[i], u);
                Out { drive: Drive::Place { pos, yaw: self.yaws[i] + if u >= 1.0 { 0.3 * (self.t * 2.0 + i as f32).sin() } else { 0.0 } }, ..Default::default() }
            })
            .collect();
        let label = if self.t < self.down_s { "dropping" } else if u >= 1.0 { "hiding in the coral" } else { "coming back up" };
        Step::On(label.into(), outs)
    }
}
