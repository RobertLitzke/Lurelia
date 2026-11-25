//! `pair_bow`: a pair on the water swim together, turn face to face and
//! bow: heads dipped together, necks curved, again and again, now and then
//! both raising their heads high; swans make a heart of their necks. Then
//! they drift apart.
//!
//! Params: `bows` (how many), `bow_s` (seconds one takes).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "pair_bow",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["bows", "bow_s"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Paddler { floating: true }
}

struct PairBow {
    /// Where each comes to, face to face, and where it is now.
    marks: [Vec3; 2],
    pos: [Vec3; 2],
    bows: u32,
    bow_s: f32,
    met: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mid = (a.pos + b.pos) * 0.5;
    if !ctx.terrain.wet(mid.x, mid.z) {
        return None;
    }
    let dir = (b.pos - a.pos).with_y(0.0).normalize_or(Vec3::X);
    let gap = (a.snout + b.snout) * 0.6;
    let y = a.pos.y;
    let at = |q: Vec3| Vec3::new(q.x, y, q.z);
    Some(Box::new(PairBow { marks: [at(mid - dir * gap * 0.5), at(mid + dir * gap * 0.5)], pos: [a.pos, b.pos], bows: p.count("bows", [3, 7], rng).max(1), bow_s: p.num("bow_s", 1.6, rng).max(0.5), met: false, t: 0.0 }))
}

impl Run for PairBow {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = [face(self.pos[0], self.pos[1]), face(self.pos[1], self.pos[0])];
        if !self.met {
            let mut there = true;
            let outs = (0..2)
                .map(|i| {
                    let d = (self.marks[i] - self.pos[i]).with_y(0.0);
                    let step = cast[i].slow.max(0.5) * 0.6 * ctx.dt;
                    if d.length() > step {
                        there = false;
                        self.pos[i] += d.normalize() * step;
                    } else {
                        self.pos[i] = self.marks[i];
                    }
                    let yaw = if d.length() > 0.05 { face(self.pos[i], self.marks[i]) } else { faces[i] };
                    Out { drive: Drive::Sit { pos: self.pos[i], yaw }, ..Default::default() }
                })
                .collect();
            if there || self.t > 20.0 {
                self.met = true;
                self.t = 0.0;
            }
            return Step::On("swimming together".into(), outs);
        }
        let n = self.t / self.bow_s;
        if n > self.bows as f32 {
            return Step::Done;
        }
        // Every third a head-up instead of a bow.
        let f = n.fract();
        let high = (n as u32) % 3 == 2;
        let outs = (0..2)
            .map(|i| {
                let s = (PI * f).sin();
                let pose = if high { Overlay { head: 0.6 * s, ..Default::default() } } else { Overlay { head: -0.7 * s, rear: -0.08 * s, turn: if i == 0 { 0.15 } else { -0.15 } * s, ..Default::default() } };
                Out { drive: Drive::Sit { pos: self.pos[i], yaw: faces[i] }, pose, ..Default::default() }
            })
            .collect();
        Step::On(if high { "heads up" } else { "bowing" }.into(), outs)
    }
}
