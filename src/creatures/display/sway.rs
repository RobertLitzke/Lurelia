//! `sway`: a colony of garden eels standing out of the sand all turn the
//! same way together, into the current, and lean and sway as one, the swing
//! passing across the colony like wind through grass.
//!
//! Params: `time` (seconds), `swing` (radians either side).

use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "sway",
    rate: 6.0,
    holds: 0.0,
    cast,
    params: &["time", "swing"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 120], range: 15.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Sway {
    yaw: f32,
    time: f32,
    swing: f32,
    /// Each eel's yaw when it started, to turn from.
    from: Vec<f32>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(Sway { yaw: rng.unit() * TAU, time: p.num("time", 15.0 + rng.unit() * 10.0, rng), swing: p.num("swing", 0.6, rng), from: cast.iter().map(|m| m.yaw).collect(), t: 0.0 }))
}

impl Run for Sway {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let ease = (self.t / 2.0).min(1.0).min((self.time - self.t) / 2.0).max(0.0);
        let dir = glam::Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, m)| {
                // The swing reaches each eel later the further along it is.
                let lag = m.pos.dot(dir) * 0.4;
                let want = self.yaw + self.swing * (self.t * 1.1 - lag).sin();
                let d = crate::util::wrap_angle(want - self.from[i]);
                Out { drive: Drive::Stay { face: Some(self.from[i] + d * ease) }, ..Default::default() }
            })
            .collect();
        Step::On("swaying together".into(), outs)
    }
}
