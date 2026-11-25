//! `orientation`: a bee learning where it is. Facing back the way it came,
//! it flies widening arcs side to side in front of the flowers, rising as
//! it goes, before it turns and leaves.
//!
//! Params: `arcs` (how many), `width` (metres across the last arc).

use super::{face, Cast, Ctx, Drive, Kind, Member, Mover, Out, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "orientation",
    rate: 8.0,
    holds: 0.4,
    cast: Cast::solo,
    params: &["arcs", "width"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Buzzer
}

struct Orientation {
    at: Vec3,
    side: Vec3,
    back: Vec3,
    arcs: u32,
    width: f32,
    t: f32,
}

/// Seconds one arc takes.
const ARC_S: f32 = 0.9;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let f = m.facing();
    Some(Box::new(Orientation { at: m.pos, side: Vec3::new(-f.z, 0.0, f.x), back: -f, arcs: p.count("arcs", [5, 9], rng).max(1), width: p.num("width", 1.5, rng), t: 0.0 }))
}

impl Run for Orientation {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let n = self.t / ARC_S;
        if n > self.arcs as f32 {
            return Step::Done;
        }
        // Each arc wider and a little further back and higher than the last.
        let grow = (n + 1.0) / self.arcs as f32;
        let x = (PI * n).sin() * self.width * 0.5 * grow;
        let pos = self.at + self.side * x + self.back * (0.3 + 0.8 * grow) * (1.0 - (PI * n).sin().abs() * 0.3) + Vec3::Y * 0.6 * grow;
        Step::On("orientation flight".into(), vec![Out { drive: Drive::Place { pos, yaw: face(pos, self.at) }, ..Default::default() }])
    }
}
