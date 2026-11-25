//! `wing_stretch`: a big bird or a pterosaur standing on the ground opens
//! its wings wide, holds them up a few seconds (a heron sunning, a
//! quetzalcoatlus showing its span), gives them a shake and folds.
//!
//! Params: `hold` (seconds open), `times` (how often it opens them).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "wing_stretch",
    rate: 6.0,
    holds: 0.7,
    cast: Cast::solo,
    params: &["hold", "times"],
    words: &[],
    approach: false,
    roles: &[Role::Wing],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct WingStretch {
    hold: f32,
    times: u32,
    done: u32,
    t: f32,
    yaw: f32,
}

/// Seconds opening and folding.
const OPEN_S: f32 = 1.0;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(WingStretch { hold: p.num("hold", 3.0 + rng.unit() * 3.0, rng), times: p.count("times", [1, 2], rng).max(1), done: 0, t: 0.0, yaw: cast[0].yaw }))
}

impl Run for WingStretch {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let len = self.hold + 2.0 * OPEN_S;
        if self.t > len + 0.8 {
            self.done += 1;
            self.t = 0.0;
            if self.done >= self.times {
                return Step::Done;
            }
        }
        let t = self.t;
        let open = (t / OPEN_S).min(1.0).min(((len - t) / OPEN_S).max(0.0));
        let open = open * open * (3.0 - 2.0 * open);
        // A shake just before folding.
        let shake = if t > len - OPEN_S - 0.5 && t < len - OPEN_S { 0.12 * (t * 30.0).sin() } else { 0.0 };
        let pose = Overlay { wings: (open + shake).clamp(0.0, 1.0), head: 0.2 * open, tail: 0.2 * open, rear: 0.05 * (PI * open).sin(), ..Default::default() };
        let label = if open >= 0.99 { "wings open" } else { "opening" };
        Step::On(label.into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
    }
}
