//! `necking`: two males stand side by side, facing the same way, and swing
//! their long necks at each other, one and then the other, the blows landing
//! on the flank. Giraffes and the long-necked dinosaurs; vicuna in small.
//!
//! Params: `swings` (how many blows between them), `swing` (radians the neck
//! goes round), `swing_s` (seconds one swing takes).

use super::{gather, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "necking",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["swings", "swing", "swing_s"],
    words: &[],
    approach: false,
    roles: &[Role::Neck],
    ready,
    start,
};

fn cast() -> Cast {
    let male = MemberDef { age: Some("adult".into()), sex: Some("male".into()) };
    Cast { from: From::Same, count: [2, 2], range: 30.0, members: vec![male] }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Necking {
    marks: [Vec3; 2],
    yaw: f32,
    swings: u32,
    done: u32,
    swing: f32,
    swing_s: f32,
    met: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mid = (a.pos + b.pos) * 0.5;
    let along = a.facing();
    let side = Vec3::new(-along.z, 0.0, along.x);
    // Shoulder to shoulder, a neck's swing apart.
    let off = side * ((a.half_w + b.half_w) * 0.5 + a.height * 0.12);
    Some(Box::new(Necking {
        marks: [mid - off, mid + off],
        yaw: a.yaw,
        swings: p.count("swings", [4, 9], rng).max(1),
        done: 0,
        swing: p.num("swing", 1.0, rng),
        swing_s: p.num("swing_s", 1.8, rng).max(0.5),
        met: false,
        t: 0.0,
    }))
}

impl Run for Necking {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if !self.met {
            let (outs, all) = gather(cast, &self.marks, &[self.yaw; 2], Overlay::default());
            if all || self.t > 20.0 {
                self.met = true;
                self.t = 0.0;
            }
            return Step::On("side by side".into(), outs);
        }
        let u = self.t / self.swing_s;
        if u >= 1.0 {
            self.done += 1;
            self.t = 0.0;
            if self.done >= self.swings {
                return Step::Done;
            }
        }
        let who = (self.done % 2) as usize;
        // The swinger's neck goes back and up, away, then round hard into
        // the other's flank; the other leans away a little.
        let u = u.min(1.0);
        let wind = (PI * (u / 0.4).min(1.0)).sin() * (u < 0.4) as u32 as f32;
        let blow = if u >= 0.4 { (PI * ((u - 0.4) / 0.6)).sin() } else { 0.0 };
        let outs = (0..2)
            .map(|i| {
                // Toward the other: member 0 stands on the left.
                let toward = if i == 0 { -1.0 } else { 1.0 };
                let pose = if i == who {
                    Overlay { turn: toward * (self.swing * blow - 0.4 * wind), head: 0.4 * wind - 0.5 * blow, ..Default::default() }
                } else {
                    Overlay { turn: -toward * 0.15 * blow, head: 0.1, ..Default::default() }
                };
                Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }
            })
            .collect();
        Step::On(format!("swinging, {} of {}", self.done + 1, self.swings), outs)
    }
}
