//! `sky_dive`: a male hummingbird's courtship dive. Off its twig it climbs
//! twenty metres or more straight up, then dives in a narrow U down past the
//! perch at speed and up the far side, turns, and does it again the other
//! way, a few times over -- the way an Anna's does over a female. Then the
//! hummingbird darts back to its twig (hummer.rs takes it back).
//!
//! Params: `climb` (metres above the bottom of the U), `half_width` (half
//! the U across), `dives` (how many), `dive_s` (seconds through one).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "sky_dive",
    rate: 8.0,
    // High over the shrubs it pays the player no mind.
    holds: 0.0,
    cast: Cast::solo,
    params: &["climb", "half_width", "dives", "dive_s"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Hummer { perched: true }
}

enum Phase {
    /// Straight up from `from` to the top of the first dive.
    Climb { from: Vec3, t: f32, dur: f32 },
    /// Down the U and up the far side; `s` 0 to 1.
    Dive { s: f32 },
    /// Hanging at the top a moment, turned round.
    Turn { from: Vec3, t: f32, dur: f32 },
}

struct SkyDive {
    bottom: Vec3,
    /// Across the U, half its width: the dive runs from -dir to +dir.
    dir: Vec3,
    climb: f32,
    dives: u32,
    done: u32,
    dive_s: f32,
    phase: Phase,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn super::Run>> {
    let m = &cast[0];
    let a = rng.unit() * TAU;
    let half = p.num("half_width", 5.0 + rng.unit() * 3.0, rng);
    let dir = Vec3::new(a.sin(), 0.0, a.cos()) * half;
    let g = ctx.terrain.drawn(m.pos.x, m.pos.z, m.pos.y + 0.5).unwrap_or(m.pos.y - 2.0);
    let bottom = Vec3::new(m.pos.x, g + 1.0, m.pos.z) + dir.normalize_or_zero() * (rng.unit() - 0.5) * 2.0;
    let climb = p.num("climb", 18.0 + rng.unit() * 10.0, rng);
    let top = bottom - dir + Vec3::Y * climb;
    // A slow climb, wings whirring.
    let dur = ((top - m.pos).length() / (m.slow.max(2.0) * 0.6)).clamp(1.5, 8.0);
    Some(Box::new(SkyDive {
        bottom,
        dir,
        climb,
        dives: p.count("dives", [2, 5], rng).max(1),
        done: 0,
        dive_s: p.num("dive_s", 1.3, rng).max(0.4),
        phase: Phase::Climb { from: m.pos, t: 0.0, dur },
    }))
}

/// The point `s` of the way through a dive from -dir to +dir.
fn on_u(bottom: Vec3, dir: Vec3, climb: f32, s: f32) -> Vec3 {
    let u = (PI * (s - 0.5)).sin();
    bottom + dir * u + Vec3::Y * climb * u * u
}


impl Run for SkyDive {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        let (bottom, climb) = (self.bottom, self.climb);
        let top = move |dir: Vec3| bottom - dir + Vec3::Y * climb;
        let (pos, phase) = match &mut self.phase {
            Phase::Climb { from, t, dur } => {
                *t = (*t + ctx.dt / *dur).min(1.0);
                let e = *t * *t * (3.0 - 2.0 * *t);
                let p = from.lerp(top(self.dir), e);
                if *t >= 1.0 {
                    self.phase = Phase::Dive { s: 0.0 };
                }
                (p, "climbing".to_string())
            }
            Phase::Dive { s } => {
                *s += ctx.dt / self.dive_s;
                if *s >= 1.0 {
                    self.done += 1;
                    if self.done >= self.dives {
                        return Step::Done;
                    }
                    // The far top is the start of the next, the other way.
                    self.dir = -self.dir;
                    self.phase = Phase::Turn { from: m.pos, t: 0.0, dur: 0.6 + rng.unit() * 0.6 };
                    (m.pos, format!("dive {} of {}", self.done, self.dives))
                } else {
                    (on_u(self.bottom, self.dir, self.climb, *s), format!("dive {} of {}", self.done + 1, self.dives))
                }
            }
            Phase::Turn { from, t, dur } => {
                *t = (*t + ctx.dt / *dur).min(1.0);
                let p = from.lerp(top(self.dir), *t * *t * (3.0 - 2.0 * *t));
                if *t >= 1.0 {
                    self.phase = Phase::Dive { s: 0.0 };
                }
                (p, format!("turning, {} of {} done", self.done, self.dives))
            }
        };
        let yaw = heading(m.pos, pos, m.yaw);
        Step::On(phase, vec![Out { drive: Drive::Place { pos, yaw }, ..Default::default() }])
    }
}
