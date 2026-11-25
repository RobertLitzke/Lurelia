//! `billing`: a pair of auks (puffins, razorbills, murrelets) side by side
//! on the water, facing, rattling their bills together side to side, then
//! bobbing their heads and floating apart.
//!
//! Params: `time` (seconds), `on` (where they settle: "water", or "perch" or
//! "ground" for a cliff ledge).

use super::perch::{settle_at, Settle};
use super::{face, Cast, Ctx, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "billing",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["time"],
    words: &[("on", &["water", "perch", "ground"])],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 50.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true })
}

struct Billing {
    settle: [Settle; 2],
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let at = settle_at(a, p.word("on", "water"), ctx, rng)?;
    // Bills touching: two bodies' reach apart on a line through the spot.
    let gap = (a.snout + b.snout) * 0.5 + 0.02;
    let dir = (b.pos - a.pos).with_y(0.0).normalize_or(Vec3::X);
    let (pa, pb) = (at - dir * gap * 0.5, at + dir * gap * 0.5);
    let pb = Vec3::new(pb.x, at.y, pb.z);
    Some(Box::new(Billing { settle: [Settle::new(a, pa, face(pa, pb)), Settle::new(b, pb, face(pb, pa))], time: p.num("time", 8.0 + rng.unit() * 8.0, rng), t: 0.0 }))
}

impl Run for Billing {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let mut outs = Vec::new();
        let mut flying = false;
        for (i, s) in self.settle.iter_mut().enumerate() {
            match s.flying(&cast[i], ctx.dt) {
                Some(o) => {
                    flying = true;
                    outs.push(o);
                }
                None => outs.push(Out { drive: s.sit(), ..Default::default() }),
            }
        }
        if flying {
            return Step::On("coming together".into(), outs);
        }
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        // Bills rattled side to side together, in bursts, with bobs between.
        let burst = (t % 3.0) < 2.0;
        for (i, o) in outs.iter_mut().enumerate() {
            let s = if i == 0 { 1.0 } else { -1.0 };
            o.pose = if burst { Overlay { turn: s * 0.35 * (t * 14.0).sin(), head: 0.1, ..Default::default() } } else { Overlay { head: -0.3 * (t * 4.0 + i as f32).sin().abs(), ..Default::default() } };
        }
        Step::On(if burst { "billing" } else { "bobbing" }.into(), outs)
    }
}
