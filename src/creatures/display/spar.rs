//! `spar`: two males walk up to each other, lower their heads until antlers
//! or horns meet, and push and twist, each giving ground and taking it back,
//! for a few seconds to a quarter of a minute. No run-up: that is
//! `head_butt`. Then they back apart and shake their heads.
//!
//! Params: `time` (seconds locked together), `twist` (radians their heads
//! twist either way), `push` (share of its walking speed each shoves at).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "spar",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["time", "twist", "push"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    let male = MemberDef { age: Some("adult".into()), sex: Some("male".into()) };
    Cast { from: From::Same, count: [2, 2], range: 25.0, members: vec![male] }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Approach,
    Locked,
    Part,
    Shake,
}

struct Spar {
    mid: Vec3,
    axis: Vec3,
    time: f32,
    twist: f32,
    push: f32,
    phase: Phase,
    t: f32,
    /// Which of the two is pushing, and for how long yet.
    pusher: usize,
    shove: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let d = (b.pos - a.pos).with_y(0.0);
    let axis = if d.length() > 0.1 { d.normalize() } else { -a.facing() };
    Some(Box::new(Spar {
        mid: (a.pos + b.pos) * 0.5,
        axis,
        time: p.num("time", 6.0 + rng.unit() * 8.0, rng),
        twist: p.num("twist", 0.35, rng),
        push: p.num("push", 0.35, rng).clamp(0.05, 1.0),
        phase: Phase::Approach,
        t: 0.0,
        pusher: 0,
        shove: 0.0,
    }))
}

impl Run for Spar {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = |i: usize| face(cast[i].pos, cast[1 - i].pos);
        let gap = (cast[0].pos - cast[1].pos).with_y(0.0).length();
        let meet = cast[0].snout + cast[1].snout;
        let (label, outs): (&str, Vec<Out>) = match self.phase {
            Phase::Approach => {
                // Marks with their snouts a hand apart either side of the middle.
                let marks = [self.mid - self.axis * (cast[0].snout + 0.1), self.mid + self.axis * (cast[1].snout + 0.1)];
                let there = [cast[0].at(marks[0]), cast[1].at(marks[1])];
                if self.t > 14.0 && gap > meet * 3.0 + 3.0 {
                    return Step::Done;
                }
                if there[0] && there[1] || self.t > 14.0 {
                    self.phase = Phase::Locked;
                    self.t = 0.0;
                }
                let outs = (0..2)
                    .map(|i| {
                        let drive = if there[i] { Drive::Stay { face: Some(faces(i)) } } else { Drive::Go { to: marks[i], speed: cast[i].slow } };
                        Out { drive, pose: Overlay { head: -0.3 * there[i] as u32 as f32, ..Default::default() }, ..Default::default() }
                    })
                    .collect();
                ("squaring up", outs)
            }
            Phase::Locked => {
                if self.t > self.time {
                    self.phase = Phase::Part;
                    self.t = 0.0;
                }
                self.shove -= ctx.dt;
                if self.shove <= 0.0 {
                    self.pusher = (rng.unit() < 0.5) as usize;
                    self.shove = 0.8 + rng.unit() * 1.6;
                }
                // Heads twisting against each other, slow and uneven.
                let twist = self.twist * ((self.t * 0.9 * TAU).sin() * 0.7 + (self.t * 2.3).sin() * 0.3);
                let outs = (0..2)
                    .map(|i| {
                        let m = &cast[i];
                        let toward = (cast[1 - i].pos - m.pos).with_y(0.0).normalize_or_zero();
                        let pushing = i == self.pusher;
                        let drive = if pushing && gap > meet * 0.85 {
                            Drive::Go { to: m.pos + toward * 0.6, speed: m.slow * self.push }
                        } else if !pushing && gap < meet * 1.15 {
                            Drive::Back { speed: m.slow * self.push * 0.8 }
                        } else {
                            Drive::Stay { face: Some(faces(i)) }
                        };
                        let side = if i == 0 { 1.0 } else { -1.0 };
                        Out { drive, pose: Overlay { head: -0.7, turn: twist * side, rear: -0.05, tail: 0.2, ..Default::default() }, ..Default::default() }
                    })
                    .collect();
                ("locked, pushing", outs)
            }
            Phase::Part => {
                if gap > meet + 1.5 || self.t > 2.5 {
                    self.phase = Phase::Shake;
                    self.t = 0.0;
                }
                let outs = (0..2).map(|i| Out { drive: Drive::Back { speed: cast[i].slow * 0.8 }, pose: Overlay { head: -0.3, ..Default::default() }, ..Default::default() }).collect();
                ("breaking off", outs)
            }
            Phase::Shake => {
                if self.t > 2.0 {
                    return Step::Done;
                }
                let shake = 0.3 * (self.t * 12.0).sin() * (1.0 - self.t / 2.0);
                ("shaking it off", vec![Out { pose: Overlay { turn: shake, ..Default::default() }, ..Default::default() }; 2])
            }
        };
        Step::On(label.into(), outs)
    }
}
