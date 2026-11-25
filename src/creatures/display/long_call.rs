//! `long_call`: a gull (or a pelican, a gannet) settles on a post, a roof,
//! the beach or the water and calls: head bowed forward and down, then
//! thrown up and back with the bill wide open, a run of calls, then quiet.
//! Others near it often answer.
//!
//! Params: `calls` (how many), `call_s` (seconds one takes), `on` (where it
//! settles: "perch", "ground" or "water").

use super::perch::{settle_at, Settle};
use super::{Cast, Ctx, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "long_call",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["calls", "call_s"],
    words: &[("on", &["perch", "ground", "water"])],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [1, 3], range: 40.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true })
}

struct LongCall {
    settle: Vec<Settle>,
    calls: u32,
    call_s: f32,
    /// Seconds each starts calling after it sits: the others answer late.
    late: Vec<f32>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let on = p.word("on", "perch");
    let mut settle = Vec::new();
    for m in cast {
        let at = settle_at(m, on, ctx, rng)?;
        settle.push(Settle::new(m, at, m.yaw + (rng.unit() - 0.5) * 1.5));
    }
    Some(Box::new(LongCall {
        settle,
        calls: p.count("calls", [3, 7], rng).max(1),
        call_s: p.num("call_s", 0.9, rng).max(0.3),
        late: (0..cast.len()).map(|i| 0.6 + i as f32 * (0.8 + rng.unit() * 1.5)).collect(),
        t: 0.0,
    }))
}

impl Run for LongCall {
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
            return Step::On("coming down".into(), outs);
        }
        self.t += ctx.dt;
        let last = self.late.iter().fold(0.0f32, |a, &b| a.max(b)) + self.calls as f32 * self.call_s + 1.0;
        if self.t > last {
            return Step::Done;
        }
        for (i, o) in outs.iter_mut().enumerate() {
            let c = (self.t - self.late[i]) / self.call_s;
            if c < 0.0 || c >= self.calls as f32 {
                continue;
            }
            // Bow forward, then the head thrown up with the bill wide.
            let f = c.fract();
            let (head, jaw) = if f < 0.35 { (-0.6 * (PI * f / 0.35).sin(), 0.0) } else { (1.0 * (PI * (f - 0.35) / 0.65).sin(), 0.6 * (PI * (f - 0.35) / 0.65).sin()) };
            o.pose = Overlay { head, jaw, tail: 0.15, wings: 0.15 * jaw, ..Default::default() };
        }
        Step::On("calling".into(), outs)
    }
}
