//! `pulse_bloom`: jellies near each other all start rising together, beat
//! by beat, toward the light, gathering just under the surface in a loose
//! bloom; after a while they let themselves sink back down.
//!
//! Params: `time` (seconds at the top), `rise` (metres a second they climb
//! at best).

use super::perch::water_top;
use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "pulse_bloom",
    rate: 6.0,
    holds: 0.0,
    cast,
    params: &["time", "rise"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 80], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Fish { air: false }
}

struct Bloom {
    pos: Vec<Vec3>,
    home: Vec<f32>,
    top: Vec<f32>,
    beat: Vec<f32>,
    rise: f32,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(Bloom {
        pos: cast.iter().map(|m| m.pos).collect(),
        home: cast.iter().map(|m| m.pos.y).collect(),
        top: cast.iter().map(|m| water_top(ctx, m.pos.x, m.pos.z) - 0.4 - m.height - rng.unit() * 0.8).collect(),
        beat: cast.iter().map(|_| rng.unit() * TAU).collect(),
        rise: p.num("rise", 0.15, rng),
        time: p.num("time", 15.0 + rng.unit() * 10.0, rng),
        t: 0.0,
    }))
}

impl Run for Bloom {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let mut all_up = true;
        let mut all_down = true;
        let sinking = self.t > self.time;
        for i in 0..self.pos.len() {
            self.beat[i] += ctx.dt * 1.4;
            let push = self.beat[i].sin().max(0.0);
            let y = &mut self.pos[i].y;
            if !sinking {
                if *y < self.top[i] {
                    *y = (*y + self.rise * push * 2.0 * ctx.dt).min(self.top[i]);
                    all_up = false;
                }
            } else if *y > self.home[i] {
                *y = (*y - self.rise * 0.6 * ctx.dt).max(self.home[i]);
                all_down = false;
            }
            self.pos[i] += Vec3::new((self.beat[i] * 0.1).cos(), 0.0, (self.beat[i] * 0.08).sin()) * 0.05 * ctx.dt;
        }
        if sinking && all_down {
            return Step::Done;
        }
        let outs = (0..self.pos.len()).map(|i| Out { drive: Drive::Place { pos: self.pos[i], yaw: self.beat[i] * 0.02 }, ..Default::default() }).collect();
        let label = if sinking { "sinking back" } else if all_up { "blooming at the top" } else { "rising" };
        Step::On(label.into(), outs)
    }
}
