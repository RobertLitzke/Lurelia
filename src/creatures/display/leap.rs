//! `leap`: a fish comes up fast from below and clears the water, arcing
//! through the air and falling back in with a splash, sometimes two or
//! three in a row. A manta turns a somersault in the air; a sunfish
//! flops out sideways.
//!
//! Params: `leaps` (how many), `height` (metres it clears the surface),
//! `flip` (turns it rolls over in the air: mantas).

use super::perch::water_top;
use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "leap",
    rate: 8.0,
    holds: 0.0,
    cast: Cast::solo,
    params: &["leaps", "height", "flip"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

enum Phase {
    /// Up from where it was to just under the surface, `t` of `dur`.
    Rise { from: Vec3, t: f32, dur: f32 },
    /// In the air: launched from `at` with velocity `v`, `t` seconds ago.
    Air { at: Vec3, v: Vec3, t: f32 },
    /// Back down to where it started.
    Back { from: Vec3, t: f32 },
}

struct Leap {
    home: Vec3,
    dir: Vec3,
    leaps: u32,
    done: u32,
    height: f32,
    flip: f32,
    phase: Phase,
}

const G: f32 = 9.8;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    Some(Box::new(Leap {
        home: m.pos,
        dir: m.facing(),
        leaps: p.count("leaps", [1, 3], rng).max(1),
        done: 0,
        height: p.num("height", 0.4 + m.height * 1.5, rng),
        flip: p.num("flip", 0.0, rng),
        phase: Phase::Rise { from: m.pos, t: 0.0, dur: 1.2 },
    }))
}

impl Run for Leap {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        let top = water_top(ctx, m.pos.x, m.pos.z);
        let mut pose = Overlay::default();
        let (pos, label) = match &mut self.phase {
            Phase::Rise { from, t, dur } => {
                *t = (*t + ctx.dt / *dur).min(1.0);
                let under = Vec3::new(from.x, top - 0.3, from.z) + self.dir * 2.0 * *t;
                let pos = from.lerp(under, *t * *t);
                if *t >= 1.0 {
                    let up = (2.0 * G * (self.height + 0.3)).sqrt();
                    self.phase = Phase::Air { at: pos, v: self.dir * 2.5 + Vec3::Y * up, t: 0.0 };
                }
                (pos, "rising")
            }
            Phase::Air { at, v, t } => {
                *t += ctx.dt;
                let pos = *at + *v * *t - Vec3::Y * 0.5 * G * *t * *t;
                let air = 2.0 * v.y / G;
                pose.roll = self.flip * TAU * (*t / air).min(1.0);
                // Nose up going out, down coming in.
                pose.rear = 0.8 * (1.0 - 2.0 * (*t / air).min(1.0));
                if pos.y < top - 0.3 && *t > air * 0.5 {
                    self.done += 1;
                    if self.done < self.leaps {
                        self.phase = Phase::Rise { from: pos, t: 0.3, dur: 1.0 };
                    } else {
                        self.phase = Phase::Back { from: pos, t: 0.0 };
                    }
                }
                (pos, "leaping")
            }
            Phase::Back { from, t } => {
                *t = (*t + ctx.dt / 2.0).min(1.0);
                if *t >= 1.0 {
                    return Step::Done;
                }
                (from.lerp(self.home + self.dir * 3.0, *t), "diving back")
            }
        };
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, pose, ..Default::default() }])
    }
}
