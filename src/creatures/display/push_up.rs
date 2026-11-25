//! `push_up`: a lizard's challenge. Facing whatever it is telling off, it
//! pumps its front end up and down on straightened fore legs in quick runs,
//! head bobbing, then holds still and watches.
//!
//! Params: `runs` (how many runs), `pumps` (push-ups in a run), `height`
//! (radians the front comes up).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "push_up",
    rate: 8.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["runs", "pumps", "height"],
    words: &[],
    approach: false,
    roles: &[Role::Leg],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct PushUp {
    runs: u32,
    done: u32,
    pumps: u32,
    height: f32,
    t: f32,
    rest: f32,
    yaw: f32,
}

/// Seconds one push-up takes.
const PUMP_S: f32 = 0.45;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    // Facing the player if it is near, else whichever way it was.
    let m = &cast[0];
    let yaw = if (ctx.player - m.pos).length() < 15.0 { face(m.pos, ctx.player) } else { m.yaw };
    Some(Box::new(PushUp {
        runs: p.count("runs", [2, 4], rng).max(1),
        done: 0,
        pumps: p.count("pumps", [3, 6], rng).max(1),
        height: p.num("height", 0.35, rng),
        t: 0.0,
        rest: 0.5,
        yaw,
    }))
}

impl Run for PushUp {
    fn step(&mut self, _: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let still = Drive::Stay { face: Some(self.yaw) };
        if self.rest > 0.0 {
            self.rest -= ctx.dt;
            return Step::On("watching".into(), vec![Out { drive: still, pose: Overlay { head: 0.15, ..Default::default() }, ..Default::default() }]);
        }
        self.t += ctx.dt;
        if self.t > self.pumps as f32 * PUMP_S {
            self.done += 1;
            self.t = 0.0;
            if self.done >= self.runs {
                return Step::Done;
            }
            self.rest = 1.5 + rng.unit() * 2.5;
        }
        let up = (PI * (self.t / PUMP_S).fract()).sin();
        let pose = Overlay { rear: self.height * up, plant: 1.0, head: 0.25 * up, tail: 0.1, ..Default::default() };
        Step::On(format!("push-ups, {} of {}", self.done + 1, self.runs), vec![Out { drive: still, pose, ..Default::default() }])
    }
}
