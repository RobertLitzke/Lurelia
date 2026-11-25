//! `gesture`: a person or a robot standing about does something with its
//! hands: waves (at the player, if near), stretches both arms overhead and
//! leans back, or lifts its hands and looks down at them.
//!
//! Params: `time` (seconds), `gesture` (one of the three; left out, any).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "gesture",
    rate: 6.0,
    holds: 0.9,
    cast: Cast::solo,
    params: &["time"],
    words: &[("gesture", &["wave", "stretch", "hands"])],
    approach: false,
    roles: &[Role::Arm],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum What {
    Wave,
    Stretch,
    Hands,
}

struct Gesture {
    what: What,
    time: f32,
    yaw: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let what = match p.word("gesture", "") {
        "wave" => What::Wave,
        "stretch" => What::Stretch,
        "hands" => What::Hands,
        _ => [What::Wave, What::Stretch, What::Hands][(rng.unit() * 3.0) as usize % 3],
    };
    let yaw = if what == What::Wave && (ctx.player - m.pos).length() < 40.0 { face(m.pos, ctx.player) } else { m.yaw };
    Some(Box::new(Gesture { what, time: p.num("time", 3.0 + rng.unit() * 2.0, rng), yaw, t: 0.0 }))
}

impl Run for Gesture {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let e = (t / 0.6).min(1.0).min((self.time - t) / 0.6).max(0.0);
        let (pose, label) = match self.what {
            What::Wave => (Overlay { wave: 2.6 * e, arms: 0.15 * e * (t * 9.0).sin(), head: 0.1 * e, ..Default::default() }, "waving"),
            What::Stretch => (Overlay { raise: 2.8 * e, arms: 0.3 * e, rear: 0.12 * e, head: 0.35 * e, ..Default::default() }, "stretching"),
            What::Hands => (Overlay { raise: 1.0 * e, arms: -0.2 * e, head: -0.6 * e, turn: 0.1 * (t * 1.5).sin(), ..Default::default() }, "looking at its hands"),
        };
        Step::On(label.into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
    }
}
