//! `box`: two hares rear up on their hind legs face to face and swat at
//! each other with their fore paws in quick flurries, drop down, chase a few
//! metres, and go up again.
//!
//! Params: `bouts` (how many times they go up), `bout_s` (seconds a bout
//! lasts).

use super::{face, gather, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "box",
    rate: 6.0,
    holds: 0.6,
    cast,
    params: &["bouts", "bout_s"],
    words: &[],
    approach: false,
    roles: &[Role::Leg],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 20.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Box_ {
    marks: [Vec3; 2],
    bouts: u32,
    done: u32,
    bout_s: f32,
    /// 0 meeting, 1 up and boxing, 2 a short chase.
    phase: u8,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mid = (a.pos + b.pos) * 0.5;
    let axis = (b.pos - a.pos).with_y(0.0).normalize_or(-a.facing());
    let gap = (a.snout + b.snout) * 0.45 + 0.05;
    Some(Box::new(Box_ { marks: [mid - axis * gap, mid + axis * gap], bouts: p.count("bouts", [2, 4], rng).max(1), done: 0, bout_s: p.num("bout_s", 2.5, rng).max(0.8), phase: 0, t: 0.0 }))
}

impl Run for Box_ {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = [face(cast[0].pos, cast[1].pos), face(cast[1].pos, cast[0].pos)];
        match self.phase {
            0 => {
                let (outs, all) = gather(cast, &self.marks, &faces, Overlay::default());
                if all || self.t > 12.0 {
                    self.phase = 1;
                    self.t = 0.0;
                }
                Step::On("facing up".into(), outs)
            }
            1 => {
                if self.t > self.bout_s {
                    self.done += 1;
                    self.t = 0.0;
                    if self.done >= self.bouts {
                        return Step::Done;
                    }
                    self.phase = 2;
                }
                let t = self.t;
                let up = (t / 0.3).min(1.0) * ((self.bout_s - t) / 0.3).clamp(0.0, 1.0);
                let outs = (0..2)
                    .map(|i| {
                        let pose = Overlay { rear: 1.2 * up, plant: 1.0, paw: 0.9 * up * (t * 16.0 + i as f32 * 1.3).sin(), raise: 0.5 * up, head: 0.2 * up, ..Default::default() };
                        Out { drive: Drive::Stay { face: Some(faces[i]) }, pose, ..Default::default() }
                    })
                    .collect();
                Step::On(format!("boxing, {} of {}", self.done + 1, self.bouts), outs)
            }
            _ => {
                // One breaks away a few metres, the other after it, and they
                // turn to face again.
                if self.t > 1.5 {
                    let d = (cast[1].pos - cast[0].pos).with_y(0.0).normalize_or(Vec3::X);
                    let mid = (cast[0].pos + cast[1].pos) * 0.5;
                    let gap = (cast[0].snout + cast[1].snout) * 0.45 + 0.05;
                    self.marks = [mid - d * gap, mid + d * gap];
                    self.phase = 0;
                    self.t = 0.0;
                }
                let run = cast[0].pos - cast[0].facing() * 4.0;
                let outs = vec![
                    Out { drive: Drive::Go { to: run, speed: cast[0].fast }, ..Default::default() },
                    Out { drive: Drive::Go { to: cast[0].pos, speed: cast[1].fast }, ..Default::default() },
                ];
                Step::On("chasing".into(), outs)
            }
        }
    }
}
