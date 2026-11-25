//! `sync_flash`: glowing ones near each other fall into step. Each starts
//! on its own rhythm, and flash by flash they pull together until the whole
//! patch lights up at once, then dark, then all at once again; after a
//! while they drift out of step.
//!
//! Only for species that glow (fireflies: `blink`); the brightness goes
//! through `Out::flash` (entities.rs).
//!
//! Params: `time` (seconds), `rate` (flashes a second when in step), `pull`
//! (how fast they fall into step, 0 to 1 a flash).

use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "sync_flash",
    rate: 8.0,
    holds: 0.0,
    cast,
    params: &["time", "rate", "pull"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 80], range: 30.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true } | Mover::Buzzer)
}

struct SyncFlash {
    /// Each one's phase, 0 to 1 through its cycle.
    phase: Vec<f32>,
    rate: f32,
    pull: f32,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(SyncFlash {
        phase: cast.iter().map(|_| rng.unit()).collect(),
        rate: p.num("rate", 0.8, rng).max(0.1),
        pull: p.num("pull", 0.25, rng).clamp(0.0, 1.0),
        time: p.num("time", 20.0 + rng.unit() * 15.0, rng),
        t: 0.0,
    }))
}

impl Run for SyncFlash {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        // Coupled clocks: each is nudged toward the mean phase every step,
        // hard in the middle of the display and letting go at the end.
        let n = self.phase.len() as f32;
        let (s, c) = self.phase.iter().fold((0.0f32, 0.0f32), |(s, c), p| {
            let a = p * std::f32::consts::TAU;
            (s + a.sin(), c + a.cos())
        });
        let mean = (s / n).atan2(c / n) / std::f32::consts::TAU;
        let hold = (self.t / 4.0).min(1.0) * ((self.time - self.t) / 6.0).clamp(0.0, 1.0);
        let k = self.pull * self.rate * hold * ctx.dt * 4.0;
        let mut together = 0.0;
        let outs = cast
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let p = &mut self.phase[i];
                let off = ((mean - *p + 1.5).rem_euclid(1.0)) - 0.5;
                *p = (*p + self.rate * ctx.dt + off * k).rem_euclid(1.0);
                together += off.abs();
                let on = if *p < 0.2 { (*p / 0.2 * std::f32::consts::PI).sin() } else { 0.0 };
                // No `Place`: the fliers carry on moving as they were.
                Out { drive: Drive::Stay { face: None }, flash: Some(on), ..Default::default() }
            })
            .collect();
        let label = if together / n < 0.05 { "flashing together" } else { "falling into step" };
        Step::On(label.into(), outs)
    }
}
