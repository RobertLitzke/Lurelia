//! `wing_clap`: a nightjar's display flight at dusk. It flies slow loose
//! loops low over its patch, and every few seconds claps its wings together
//! over its back with a crack, then glides with them held up in a V, tail
//! spread, before flying on.
//!
//! Params: `time` (seconds), `every` (seconds between claps), `radius`
//! (metres round a loop).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "wing_clap",
    rate: 8.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["time", "every", "radius"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true })
}

struct WingClap {
    centre: Vec3,
    r: f32,
    ang: f32,
    dir: f32,
    speed: f32,
    time: f32,
    every: f32,
    next: f32,
    /// Seconds into a clap and glide.
    clap: Option<f32>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let r = p.num("radius", 7.0, rng);
    let ang = rng.unit() * TAU;
    Some(Box::new(WingClap {
        centre: m.pos - Vec3::new(ang.cos(), 0.0, ang.sin()) * r,
        r,
        ang,
        dir: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        speed: m.slow.clamp(3.0, 7.0),
        time: p.num("time", 12.0 + rng.unit() * 10.0, rng),
        every: p.num("every", 3.5, rng).max(1.0),
        next: 1.5,
        clap: None,
        t: 0.0,
    }))
}

/// Seconds a clap and the glide after it last.
const CLAP_S: f32 = 1.4;

impl Run for WingClap {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        if self.t > self.time && self.clap.is_none() {
            return Step::Done;
        }
        self.next -= ctx.dt;
        if self.next <= 0.0 && self.clap.is_none() {
            self.clap = Some(0.0);
            self.next = self.every * (0.7 + rng.unit() * 0.6);
        }
        let mut pose = Overlay::default();
        let mut pace = 1.0;
        let mut label = "looping";
        if let Some(c) = &mut self.clap {
            *c += ctx.dt;
            // Wings snapped up together, then held in a V for the glide.
            pose.wings = if *c < 0.12 { 1.0 } else { 0.7 };
            pose.tail = 0.4;
            pace = 0.7;
            label = if *c < 0.12 { "clap" } else { "gliding, wings up" };
            if *c > CLAP_S {
                self.clap = None;
            }
        }
        self.ang += self.dir * self.speed * pace * ctx.dt / self.r;
        let r = self.r * (1.0 + 0.25 * (self.ang * 1.7).sin());
        let pos = self.centre + Vec3::new(self.ang.cos() * r, 0.6 * (self.ang * 2.0).sin(), self.ang.sin() * r);
        Step::On(label.into(), vec![Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, pose, ..Default::default() }])
    }
}
