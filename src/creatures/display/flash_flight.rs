//! `flash_flight`: a morpho flying slow and bouncing along a path, each
//! deep wingbeat showing the bright blue upper side and then the brown
//! under side, so it flashes on and off as it goes; it settles at the end.
//!
//! Params: `time` (seconds), `beat_s` (seconds a full wingbeat takes).

use super::{heading, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "flash_flight",
    rate: 8.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["time", "beat_s"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Circler { free: true }
}

struct FlashFlight {
    pos: Vec3,
    dir: f32,
    floor: f32,
    time: f32,
    beat_s: f32,
    speed: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let g = ctx.terrain.height(m.pos.x, m.pos.z).unwrap_or(m.pos.y - 2.0);
    Some(Box::new(FlashFlight {
        pos: m.pos,
        dir: rng.unit() * TAU,
        floor: g,
        time: p.num("time", 8.0 + rng.unit() * 6.0, rng),
        beat_s: p.num("beat_s", 0.7, rng).max(0.2),
        speed: m.slow.clamp(0.8, 2.5),
        t: 0.0,
    }))
}

impl Run for FlashFlight {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        self.dir += 0.4 * (self.t * 0.6).sin() * ctx.dt;
        self.pos += Vec3::new(self.dir.sin(), 0.0, self.dir.cos()) * self.speed * ctx.dt;
        if let Some(g) = ctx.terrain.height(self.pos.x, self.pos.z) {
            self.floor += (g - self.floor) * (ctx.dt * 2.0).min(1.0);
        }
        // Slow, deep beats: up showing the blue, down showing the brown; it
        // lifts on each downstroke and sinks between.
        let ph = (self.t / self.beat_s).fract();
        let open = (PI * ph).sin();
        let pos = Vec3::new(self.pos.x, self.floor + 1.5 + 0.35 * (TAU * ph).sin(), self.pos.z);
        let pose = Overlay { wings: open, ..Default::default() };
        Step::On("flashing".into(), vec![Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, pose, ..Default::default() }])
    }
}
