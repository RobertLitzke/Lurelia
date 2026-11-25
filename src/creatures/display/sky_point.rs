//! `sky_point`: an albatross pair's dance on the ground. They come down
//! side by side facing, and go through it together: bills pointed straight
//! at the sky with a call, wings half spread, bowing, then fencing with
//! their bills, clacking them against each other side to side. A pteranodon
//! pair does the same with their crests.
//!
//! Params: `time` (seconds), `on` (where they come down: "ground" or
//! "perch").

use super::perch::{settle_at, Settle};
use super::{face, Cast, Ctx, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "sky_point",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["time"],
    words: &[("on", &["ground", "perch"])],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 80.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Flocker { .. })
}

struct SkyPoint {
    settle: [Settle; 2],
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    // A circler-style landing: dry ground near the first.
    let lead = Member { mover: Mover::Circler { free: true }, ..*a };
    let at = settle_at(&lead, p.word("on", "ground"), ctx, rng)?;
    let gap = (a.snout + b.snout) * 0.55;
    let dir = (b.pos - a.pos).with_y(0.0).normalize_or(Vec3::X);
    let (pa, pb) = (at - dir * gap * 0.5, at + dir * gap * 0.5);
    let g = |q: Vec3| Vec3::new(q.x, ctx.terrain.height(q.x, q.z).unwrap_or(at.y), q.z);
    let (pa, pb) = (g(pa), g(pb));
    Some(Box::new(SkyPoint { settle: [Settle::new(a, pa, face(pa, pb)), Settle::new(b, pb, face(pb, pa))], time: p.num("time", 12.0 + rng.unit() * 10.0, rng), t: 0.0 }))
}

impl Run for SkyPoint {
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
        if self.t > self.time {
            return Step::Done;
        }
        // A loop of three moves, both in step: sky point, bow, fence.
        let t = self.t;
        let beat = (t / 2.5) as u32 % 3;
        let u = (t / 2.5).fract();
        let label = ["sky pointing", "bowing", "fencing"][beat as usize];
        for (i, o) in outs.iter_mut().enumerate() {
            let s = if i == 0 { 1.0 } else { -1.0 };
            o.pose = match beat {
                0 => Overlay { head: 1.3 * (u * std::f32::consts::PI).sin(), jaw: 0.4 * (u * 18.0).sin().max(0.0), wings: 0.5, tail: 0.3, ..Default::default() },
                1 => Overlay { head: -0.7 * (u * std::f32::consts::PI).sin(), rear: -0.15 * (u * std::f32::consts::PI).sin(), wings: 0.2, ..Default::default() },
                _ => Overlay { turn: s * 0.45 * (t * 9.0).sin(), head: 0.15, jaw: 0.3 * (t * 12.0).sin().max(0.0), ..Default::default() },
            };
        }
        Step::On(label.into(), outs)
    }
}
