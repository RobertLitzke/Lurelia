//! `rear_strike`: two stallions face each other, rear up on their hind legs
//! and strike out with their fore hooves, mouths open, come down and go up
//! again, a few times, then break off.
//!
//! Params: `rears` (how many times they go up), `height` (radians the body
//! tips up), `strike` (radians the fore legs paw).

use super::{face, gather, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "rear_strike",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["rears", "height", "strike"],
    words: &[],
    approach: false,
    roles: &[Role::Leg],
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

struct RearStrike {
    marks: [Vec3; 2],
    rears: u32,
    done: u32,
    height: f32,
    strike: f32,
    /// 0 meeting, 1 rearing, 2 breaking off.
    phase: u8,
    t: f32,
    /// Seconds one rear takes.
    up_s: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mid = (a.pos + b.pos) * 0.5;
    let axis = (b.pos - a.pos).with_y(0.0).normalize_or(-a.facing());
    let gap = (a.snout + b.snout) * 0.5 + 0.4;
    Some(Box::new(RearStrike {
        marks: [mid - axis * gap, mid + axis * gap],
        rears: p.count("rears", [2, 4], rng).max(1),
        done: 0,
        height: p.num("height", 0.75, rng),
        strike: p.num("strike", 0.6, rng),
        phase: 0,
        t: 0.0,
        up_s: 1.6 + rng.unit() * 0.6,
    }))
}

impl Run for RearStrike {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = [face(cast[0].pos, cast[1].pos), face(cast[1].pos, cast[0].pos)];
        match self.phase {
            0 => {
                let (outs, all) = gather(cast, &self.marks, &faces, Overlay { head: 0.2, ..Default::default() });
                if all || self.t > 15.0 {
                    self.phase = 1;
                    self.t = 0.0;
                }
                Step::On("squaring up".into(), outs)
            }
            1 => {
                let u = (self.t / self.up_s).min(1.0);
                if u >= 1.0 {
                    self.done += 1;
                    self.t = 0.0;
                    if self.done >= self.rears {
                        self.phase = 2;
                    }
                }
                // Up, a held moment striking, and down; the second a beat late.
                let outs = (0..2)
                    .map(|i| {
                        let v = (u - 0.08 * i as f32).clamp(0.0, 1.0);
                        let up = (PI * v).sin().powf(0.6);
                        let pose = Overlay {
                            rear: self.height * up,
                            plant: 1.0,
                            kick: self.strike * up * (self.t * 14.0 + i as f32).sin(),
                            head: 0.3 * up,
                            jaw: 0.3 * up,
                            tail: 0.4 * up,
                            ..Default::default()
                        };
                        Out { drive: Drive::Stay { face: Some(faces[i]) }, pose, ..Default::default() }
                    })
                    .collect();
                Step::On(format!("rearing, {} of {}", (self.done + 1).min(self.rears), self.rears), outs)
            }
            _ => {
                if self.t > 2.0 {
                    return Step::Done;
                }
                let outs = (0..2).map(|i| Out { drive: Drive::Back { speed: cast[i].slow * 0.7 }, pose: Overlay { head: 0.3, ..Default::default() }, ..Default::default() }).collect();
                Step::On("breaking off".into(), outs)
            }
        }
    }
}
