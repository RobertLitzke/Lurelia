//! `bask_pose`: a seal hauled out lies on its side and lifts its head and
//! its hind flippers clear of the ground together, the body bent like a
//! banana, holds it, and slowly lets them down; now and then it scratches
//! with a fore flipper.
//!
//! Params: `time` (seconds), `lift` (radians the head and tail come up).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "bask_pose",
    rate: 6.0,
    holds: 0.8,
    cast: Cast::solo,
    params: &["time", "lift"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct BaskPose {
    time: f32,
    lift: f32,
    t: f32,
    side: f32,
    yaw: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(BaskPose { time: p.num("time", 15.0 + rng.unit() * 15.0, rng), lift: p.num("lift", 0.5, rng), t: 0.0, side: if rng.unit() < 0.5 { 1.0 } else { -1.0 }, yaw: cast[0].yaw }))
}

impl Run for BaskPose {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let up = (t / 2.0).min(1.0).min((self.time - t) / 3.0);
        // A slow sag and lift while it holds.
        let up = up * (0.85 + 0.15 * (t * 0.5).sin());
        let scratch = if (t % 9.0) > 7.5 { 0.6 * (t * 12.0).sin() } else { 0.0 };
        let pose = Overlay { roll: self.side * 0.5, head: self.lift * up, tail: self.lift * 1.2 * up, paw: scratch, ..Default::default() };
        Step::On("basking, head and tail up".into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, act: Some(Act::Lie), ..Default::default() }])
    }
}
