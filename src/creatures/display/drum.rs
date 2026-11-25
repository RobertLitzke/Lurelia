//! `drum`: a burst of thumps. A kangaroo rat or a rabbit stands still and
//! stamps a hind foot on the ground in quick runs, a warning that carries
//! through the soil. A woodpecker on a trunk hammers its bill on it in
//! bursts too fast to follow, the sound carrying across the wood.
//!
//! Params: `bursts` (how many runs of stamps), `beat` (stamps a second),
//! `burst_s` (seconds a run lasts).

use super::{Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "drum",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["bursts", "beat", "burst_s"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free() || m.mover == Mover::Clinger { clinging: true }
}

struct Drum {
    bursts: u32,
    done: u32,
    beat: f32,
    burst_s: f32,
    rest: f32,
    t: f32,
    yaw: f32,
    /// On a trunk (a woodpecker), where it clings.
    cling: Option<glam::Vec3>,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let cling = (cast[0].mover == Mover::Clinger { clinging: true }).then_some(cast[0].pos);
    Some(Box::new(Drum {
        cling,
        bursts: p.count("bursts", [2, 5], rng).max(1),
        done: 0,
        beat: p.num("beat", if cling.is_some() { 16.0 } else { 7.0 }, rng).max(1.0),
        burst_s: p.num("burst_s", 1.2, rng).max(0.3),
        rest: 0.0,
        t: 0.0,
        yaw: cast[0].yaw,
    }))
}

impl Run for Drum {
    fn step(&mut self, _: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let still = match self.cling {
            Some(pos) => Drive::Sit { pos, yaw: self.yaw },
            None => Drive::Stay { face: Some(self.yaw) },
        };
        if self.rest > 0.0 {
            self.rest -= ctx.dt;
            let pose = Overlay { head: 0.25, turn: 0.4 * (self.t * 0.9).sin(), ..Default::default() };
            return Step::On("listening".into(), vec![Out { drive: still, pose, ..Default::default() }]);
        }
        if self.t > self.burst_s {
            self.done += 1;
            self.t = 0.0;
            if self.done >= self.bursts {
                return Step::Done;
            }
            self.rest = 1.0 + rng.unit() * 2.0;
        }
        // A sharp lift and slam each beat: one hind foot, or the bill.
        let ph = (self.t * self.beat).fract();
        let hit = if ph < 0.35 { (ph / 0.35 * TAU * 0.5).sin() } else { 0.0 };
        let pose = if self.cling.is_some() { Overlay { head: 0.15 - 0.45 * hit, rear: -0.05 * hit, ..Default::default() } } else { Overlay { stamp: 0.5 * hit, head: 0.2, rear: 0.05, ..Default::default() } };
        Step::On(format!("drumming, {} of {}", self.done + 1, self.bursts), vec![Out { drive: still, pose, ..Default::default() }])
    }
}
