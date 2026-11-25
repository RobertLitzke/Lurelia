//! `streamer_flight`: a male quetzal leaves its perch, flies steeply up
//! above the canopy calling, its long tail streamers rippling behind, tips
//! over at the top and swoops back down in a wide curve to the perch.
//!
//! Params: `climb` (metres it goes up), `flights` (how many), `out` (metres
//! the loop goes out from the perch).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "streamer_flight",
    rate: 6.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["climb", "flights", "out"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Flitter { perched: true }
}

struct Streamer {
    perch: Vec3,
    yaw: f32,
    climb: f32,
    out: f32,
    dir: Vec3,
    flights: u32,
    done: u32,
    /// 0 to 1 through one flight.
    u: f32,
    dur: f32,
    rest: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let a = rng.unit() * TAU;
    let climb = p.num("climb", 8.0 + rng.unit() * 6.0, rng);
    Some(Box::new(Streamer {
        perch: m.pos,
        yaw: m.yaw,
        climb,
        out: p.num("out", 8.0, rng),
        dir: Vec3::new(a.sin(), 0.0, a.cos()),
        flights: p.count("flights", [1, 3], rng).max(1),
        done: 0,
        u: 0.0,
        dur: (climb / 2.5).clamp(3.0, 8.0),
        rest: 0.0,
    }))
}

impl Run for Streamer {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        let sit = Out { drive: Drive::Sit { pos: self.perch, yaw: self.yaw }, ..Default::default() };
        if self.rest > 0.0 {
            self.rest -= ctx.dt;
            return Step::On("back on the perch".into(), vec![sit]);
        }
        self.u += ctx.dt / self.dur;
        if self.u >= 1.0 {
            self.done += 1;
            self.u = 0.0;
            if self.done >= self.flights {
                return Step::Done;
            }
            let a = rng.unit() * TAU;
            self.dir = Vec3::new(a.sin(), 0.0, a.cos());
            self.rest = 2.0 + rng.unit() * 3.0;
            return Step::On("back on the perch".into(), vec![sit]);
        }
        // Straight up most of the way, out over the top, and a long swoop
        // back in: a lopsided loop in the vertical plane along `dir`.
        let u = self.u;
        let up = if u < 0.45 { (u / 0.45).powf(0.8) } else { 1.0 - ((u - 0.45) / 0.55).powi(2) };
        let out = (PI * u).sin() * self.out * (0.3 + 0.7 * u);
        let pos = self.perch + Vec3::Y * self.climb * up + self.dir * out;
        let pose = Overlay { wag: 0.35 * (u * 40.0).sin(), tail: if u < 0.45 { -0.3 } else { 0.2 }, jaw: 0.2 * (u * 30.0).sin().max(0.0), ..Default::default() };
        let label = if u < 0.45 { "climbing" } else { "swooping back" };
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, pose, ..Default::default() }])
    }
}
