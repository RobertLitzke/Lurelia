//! `curl`: a millipede or a slug, touched or nearly, curls up tight where
//! it is and stays curled until the player has gone, then slowly unrolls.
//!
//! Started by the player coming near (`on_approach`).
//!
//! Params: `near` (metres the player comes within), `time` (seconds it
//! stays curled once the player is gone).

use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "curl",
    rate: 0.0,
    holds: 0.0,
    cast: Cast::solo,
    params: &["near", "time"],
    words: &[],
    approach: true,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Serpent { .. })
}

struct Curl {
    at: Vec3,
    near: f32,
    time: f32,
    /// Seconds since the player was last near.
    clear: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(Curl { at: cast[0].pos, near: p.num("near", 2.5, rng), time: p.num("time", 4.0, rng), clear: 0.0, t: 0.0 }))
}

impl Run for Curl {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let close = (ctx.player - self.at).length() < self.near * 1.6;
        self.clear = if close { 0.0 } else { self.clear + ctx.dt };
        if self.clear > self.time || self.t > 90.0 {
            return Step::Done;
        }
        let pose = Overlay { tuck: 1.0, head: -0.4, ..Default::default() };
        Step::On(if close { "curled up" } else { "curled, waiting" }.into(), vec![Out { drive: Drive::Stay { face: None }, pose, ..Default::default() }])
    }
}
