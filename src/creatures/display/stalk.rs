//! `stalk`: a hunter freezes on something only it can see a few metres off,
//! crouches, creeps in a step at a time with pauses, and then pounces: a fox
//! leaps high and comes down nose first (`leap`), a cat rushes the last
//! metres, a heron stands frozen and then stabs its neck down (`strike`).
//! Then it noses about where it landed.
//!
//! Params: `leap` (upward speed of the pounce, m/s, 0 for none), `strike`
//! (1 to stab with the neck from where it stands), `range` (metres to the
//! spot it stalks), `creep` (share of its walking speed).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "stalk",
    rate: 6.0,
    holds: 0.8,
    cast: Cast::solo,
    params: &["leap", "strike", "range", "creep"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// Stopped dead, head up and pointed.
    Fix,
    Creep,
    Freeze,
    Pounce,
    Nose,
}

struct Stalk {
    spot: Vec3,
    leap: f32,
    strike: bool,
    creep: f32,
    phase: Phase,
    t: f32,
    hold: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let strike = p.num("strike", 0.0, rng) > 0.5;
    let range = p.num("range", if strike { m.snout + 0.6 } else { 4.0 + rng.unit() * 4.0 }, rng);
    // Somewhere ahead and a little to one side.
    let a = (rng.unit() - 0.5) * 1.2;
    let f = m.facing();
    let dir = Vec3::new(f.x * a.cos() - f.z * a.sin(), 0.0, f.x * a.sin() + f.z * a.cos());
    let mut spot = m.pos + dir * range;
    spot.y = ctx.terrain.height(spot.x, spot.z).unwrap_or(m.pos.y);
    if (spot.y - m.pos.y).abs() > 2.0 {
        return None;
    }
    Some(Box::new(Stalk {
        spot,
        leap: p.num("leap", 0.0, rng),
        strike,
        creep: p.num("creep", 0.3, rng).clamp(0.05, 1.0),
        phase: Phase::Fix,
        t: 0.0,
        hold: 1.2 + rng.unit(),
    }))
}

impl Stalk {
    fn next(&mut self, p: Phase, hold: f32) {
        self.phase = p;
        self.t = 0.0;
        self.hold = hold;
    }
}

impl Run for Stalk {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let m = &cast[0];
        let to = self.spot - m.pos;
        let dist = to.with_y(0.0).length();
        let facing = face(m.pos, self.spot);
        // Close enough to spring from: the leap covers about a body and a
        // half, a rush a few metres, a stab only the neck's reach.
        let spring = if self.strike { m.snout + 0.8 } else if self.leap > 0.0 { m.snout + 1.5 } else { 3.0 };
        let crouch = Overlay { head: -0.35, rear: -0.06, tail: -0.1, ..Default::default() };
        let (label, out) = match self.phase {
            Phase::Fix => {
                if self.t > self.hold {
                    self.next(if dist <= spring { Phase::Freeze } else { Phase::Creep }, 1.2 + rng.unit() * 1.5);
                }
                ("fixed on something", Out { drive: Drive::Stay { face: Some(facing) }, pose: Overlay { head: 0.25, ..Default::default() }, ..Default::default() })
            }
            Phase::Creep => {
                if dist <= spring || self.t > 15.0 {
                    self.next(Phase::Freeze, 0.8 + rng.unit() * 1.5);
                } else if self.t > self.hold {
                    // A pause mid-creep, one foot up.
                    self.next(Phase::Freeze, 0.5 + rng.unit());
                }
                ("creeping", Out { drive: Drive::Go { to: self.spot, speed: m.slow * self.creep }, pose: crouch, ..Default::default() })
            }
            Phase::Freeze => {
                if self.t > self.hold {
                    self.next(if dist <= spring { Phase::Pounce } else { Phase::Creep }, 1.2 + rng.unit() * 1.5);
                }
                ("frozen", Out { drive: Drive::Stay { face: Some(facing) }, pose: Overlay { head: -0.45, ..crouch }, ..Default::default() })
            }
            Phase::Pounce => {
                let len = if self.strike { 0.6 } else { 1.0 };
                if self.t > len {
                    self.next(Phase::Nose, 1.5 + rng.unit() * 1.5);
                }
                let k = (self.t / len).min(1.0);
                let arc = (PI * k).sin();
                if self.strike {
                    // The neck shoots out and down and back.
                    let pose = Overlay { head: -1.3 * arc, tail: 0.2 * arc, ..Default::default() };
                    ("strike", Out { drive: Drive::Stay { face: Some(facing) }, pose, ..Default::default() })
                } else {
                    // Up nose first, and down nose first onto the spot.
                    let pose = Overlay { head: -0.2 - 0.6 * k, rear: 0.5 * (1.0 - 2.0 * k) * arc.max(0.2), tail: 0.4 * arc, ..Default::default() };
                    let hop = if self.t <= ctx.dt * 1.5 { self.leap } else { 0.0 };
                    ("pounce", Out { drive: Drive::Go { to: self.spot, speed: m.fast }, pose, hop, ..Default::default() })
                }
            }
            Phase::Nose => {
                if self.t > self.hold {
                    return Step::Done;
                }
                let pose = Overlay { head: -0.9, turn: 0.4 * (self.t * 3.0).sin(), ..Default::default() };
                ("nosing about", Out { drive: Drive::Stay { face: None }, pose, ..Default::default() })
            }
        };
        Step::On(label.into(), vec![out])
    }
}
