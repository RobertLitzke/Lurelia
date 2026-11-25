//! `roll`: an animal lies down, rolls onto its back with its legs pedalling
//! in the air, back down and over again a time or two, then gets up and
//! shakes itself off. A horse in a dust patch, a bear on a meadow, a boar in
//! a wallow.
//!
//! Params: `rolls` (how many times it goes over), `over` (radians it rolls
//! at the top, about 3 for right onto its back), `kick` (radians its legs
//! pedal).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "roll",
    rate: 6.0,
    holds: 0.8,
    cast: Cast::solo,
    params: &["rolls", "over", "kick"],
    words: &[],
    approach: false,
    roles: &[Role::Leg],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    LieDown,
    Rolling,
    GetUp,
    Shake,
}

struct Roll {
    rolls: u32,
    done: u32,
    over: f32,
    kick: f32,
    /// Which side it goes over on, 1 or -1.
    side: f32,
    /// Seconds for one roll, up and back.
    len: f32,
    phase: Phase,
    t: f32,
}

fn start(_cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(Roll {
        rolls: p.count("rolls", [1, 3], rng).max(1),
        done: 0,
        over: p.num("over", 2.6, rng),
        kick: p.num("kick", 0.6, rng),
        side: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        len: 2.2 + rng.unit() * 0.8,
        phase: Phase::LieDown,
        t: 0.0,
    }))
}

impl Roll {
    fn next(&mut self, p: Phase) {
        self.phase = p;
        self.t = 0.0;
    }
}

impl Run for Roll {
    fn step(&mut self, _cast: &[Member], ctx: &Ctx, _rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let lie = Out { drive: Drive::Stay { face: None }, act: Some(Act::Lie), ..Default::default() };
        let (label, out) = match self.phase {
            Phase::LieDown => {
                // Long enough for the posture to settle (anim/life.rs).
                if self.t > 1.8 {
                    self.next(Phase::Rolling);
                }
                ("lying down", lie)
            }
            Phase::Rolling => {
                let k = (self.t / self.len).min(1.0);
                // Over onto its back and down again, with a pause at the
                // top where the legs pedal hardest.
                let up = (PI * k).sin().powf(0.6);
                if k >= 1.0 {
                    self.done += 1;
                    if self.done >= self.rolls {
                        self.next(Phase::GetUp);
                    } else {
                        self.t = 0.0;
                    }
                }
                let kick = self.kick * up * (self.t * 2.5 * TAU).sin();
                let pose = Overlay { roll: self.side * self.over * up, kick, head: -0.2 * up, turn: self.side * 0.4 * up, ..Default::default() };
                ("rolling", Out { pose, ..lie })
            }
            Phase::GetUp => {
                if self.t > 1.5 {
                    self.next(Phase::Shake);
                }
                ("getting up", Out { act: None, ..lie })
            }
            Phase::Shake => {
                if self.t > 1.4 {
                    return Step::Done;
                }
                // A shiver from the head back along the body.
                let fade = 1.0 - self.t / 1.4;
                let s = (self.t * 18.0).sin() * fade;
                let pose = Overlay { roll: 0.12 * s, turn: 0.3 * s, wag: 0.4 * s, ..Default::default() };
                ("shaking off", Out { drive: Drive::Stay { face: None }, pose, ..Default::default() })
            }
        };
        Step::On(label.into(), vec![out])
    }
}
