//! `float_play`: an otter afloat plays on its own: lying on its back it
//! spins round slowly, juggles with its fore paws, and now and then rolls
//! right over through the water and comes up on its back again.
//!
//! Only in water: it won't start on land.
//!
//! Params: `time` (seconds), `rolls` (how many roll-overs, at most), `spin`
//! (radians a second it turns while lying back).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "float_play",
    rate: 8.0,
    holds: 0.7,
    cast: Cast::solo,
    params: &["time", "rolls", "spin"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct FloatPlay {
    time: f32,
    rolls: u32,
    done: u32,
    spin: f32,
    yaw: f32,
    t: f32,
    /// Seconds into a roll-over, if in one.
    roll: Option<f32>,
}

/// Seconds a roll-over takes.
const ROLL_S: f32 = 1.2;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    if !ctx.terrain.wet(m.pos.x, m.pos.z) {
        return None;
    }
    Some(Box::new(FloatPlay {
        time: p.num("time", 12.0 + rng.unit() * 10.0, rng),
        rolls: p.count("rolls", [1, 4], rng),
        done: 0,
        spin: p.num("spin", 0.8, rng) * if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        yaw: m.yaw,
        t: 0.0,
        roll: None,
    }))
}

impl Run for FloatPlay {
    fn step(&mut self, _: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time && self.roll.is_none() {
            return Step::Done;
        }
        self.yaw += self.spin * ctx.dt;
        if self.roll.is_none() && self.done < self.rolls && rng.unit() < ctx.dt * 0.3 {
            self.roll = Some(0.0);
            self.done += 1;
        }
        // On its back (half a turn over) unless rolling right round.
        let mut over = TAU * 0.5;
        let mut label = "spinning on its back";
        if let Some(r) = &mut self.roll {
            *r += ctx.dt;
            let u = (*r / ROLL_S).min(1.0);
            over += TAU * u * u * (3.0 - 2.0 * u);
            label = "rolling over";
            if u >= 1.0 {
                self.roll = None;
            }
        }
        let pose = Overlay { roll: over, paw: 0.5 * (self.t * 7.0).sin(), head: 0.3, tail: 0.2, ..Default::default() };
        Step::On(label.into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
    }
}
