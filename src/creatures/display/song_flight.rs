//! `song_flight`: a small bird sings from the air. Off its perch it climbs
//! in a slow spiral, fluttering, hangs at the top singing with its tail
//! spread, then drops with its wings held up in a V and glides the last bit
//! back down to where it started. A skylark goes up out of sight almost; a
//! sparrow only a few metres.
//!
//! Params: `climb` (metres it goes up), `rise` (metres a second it climbs),
//! `hang` (seconds at the top), `spiral` (radius of the climb, metres).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "song_flight",
    rate: 6.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["climb", "rise", "hang", "spiral"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Flitter { perched: true }
}

enum Phase {
    Climb,
    Hang,
    /// Down from `from` to the perch; `t` 0 to 1 over `dur`.
    Drop { from: Vec3, t: f32, dur: f32 },
    Land,
}

struct SongFlight {
    perch: Vec3,
    /// The foot of the spiral's axis, so it starts on the perch.
    centre: Vec3,
    yaw: f32,
    climb: f32,
    rise: f32,
    hang: f32,
    spiral: f32,
    /// Height above the perch, and the angle round the spiral.
    up: f32,
    ang: f32,
    turn: f32,
    t: f32,
    phase: Phase,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let ang = rng.unit() * TAU;
    let spiral = p.num("spiral", 2.5, rng);
    Some(Box::new(SongFlight {
        perch: m.pos,
        centre: m.pos - Vec3::new(ang.cos(), 0.0, ang.sin()) * spiral * 0.3,
        yaw: m.yaw,
        climb: p.num("climb", 6.0 + rng.unit() * 6.0, rng),
        rise: p.num("rise", 1.5, rng).max(0.3),
        hang: p.num("hang", 4.0 + rng.unit() * 6.0, rng),
        spiral,
        up: 0.0,
        ang,
        turn: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        t: 0.0,
        phase: Phase::Climb,
    }))
}

impl SongFlight {
    /// On the spiral at height `up`: it widens as it climbs.
    fn on_spiral(&self) -> Vec3 {
        let r = self.spiral * (0.3 + 0.7 * (self.up / self.climb.max(0.1)).min(1.0));
        self.centre + Vec3::new(self.ang.cos() * r, self.up, self.ang.sin() * r)
    }
}

impl Run for SongFlight {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        let dt = ctx.dt;
        self.t += dt;
        let t = self.t;
        let mut pose = Overlay::default();
        let (drive, phase) = match &mut self.phase {
            Phase::Climb => {
                self.up = (self.up + self.rise * dt).min(self.climb);
                self.ang += self.turn * dt * 1.2;
                pose.head = 0.3;
                pose.jaw = 0.15 * (t * 9.0).sin().max(0.0);
                if self.up >= self.climb {
                    self.phase = Phase::Hang;
                    self.t = 0.0;
                }
                let pos = self.on_spiral();
                (Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, "climbing, singing")
            }
            Phase::Hang => {
                // Hanging into the wind, a slow drift round, tail fanned.
                self.ang += self.turn * dt * 0.25;
                pose.head = 0.35;
                pose.tail = 0.35;
                pose.jaw = 0.2 * (t * 11.0).sin().max(0.0);
                let pos = self.on_spiral() + Vec3::Y * 0.2 * (t * 1.7).sin();
                if t > self.hang {
                    let dur = (self.climb / 6.0).clamp(1.0, 8.0);
                    self.phase = Phase::Drop { from: pos, t: 0.0, dur };
                }
                (Drive::Place { pos, yaw: m.yaw }, "singing at the top")
            }
            Phase::Drop { from, t, dur } => {
                *t = (*t + dt / *dur).min(1.0);
                // Falling fast, wings up in a V, then a glide onto the perch.
                let e = *t * *t * (3.0 - 2.0 * *t);
                let pos = from.lerp(self.perch, e);
                pose.wings = if *t < 0.8 { 0.8 } else { 0.8 * (1.0 - *t) * 5.0 };
                if *t >= 1.0 {
                    self.phase = Phase::Land;
                    self.t = 0.0;
                }
                (Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, "dropping")
            }
            Phase::Land => {
                if t > 0.6 {
                    return Step::Done;
                }
                (Drive::Sit { pos: self.perch, yaw: self.yaw }, "landed")
            }
        };
        Step::On(phase.into(), vec![Out { drive, pose, ..Default::default() }])
    }
}
