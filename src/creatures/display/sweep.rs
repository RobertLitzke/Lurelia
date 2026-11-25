//! `sweep`: a spoonbill feeding. It wades slowly forward through the
//! shallows with its bill in the water, swinging its head side to side in
//! wide arcs, and now and then jerks its head up to swallow.
//!
//! Params: `time` (seconds), `swing` (radians either side), `reach` (metres
//! it looks for water to wade).

use super::{Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "sweep",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["time", "swing", "reach"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Sweep {
    to: Vec3,
    time: f32,
    swing: f32,
    t: f32,
    gulp: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let reach = p.num("reach", 15.0, rng);
    // A line of wet ground to wade along, the first way round that has one.
    let to = (0..16).find_map(|k| {
        let a = m.yaw + k as f32 * 0.4 * if k % 2 == 0 { 1.0 } else { -1.0 };
        let dir = Vec3::new(-a.sin(), 0.0, -a.cos());
        let end = m.pos + dir * reach;
        let wet = (1..=4).all(|s| {
            let q = m.pos + dir * reach * s as f32 / 4.0;
            ctx.terrain.wet(q.x, q.z)
        });
        wet.then_some(end)
    })?;
    Some(Box::new(Sweep { to, time: p.num("time", 12.0 + rng.unit() * 10.0, rng), swing: p.num("swing", 0.7, rng), t: 0.0, gulp: 0.0 }))
}

impl Run for Sweep {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        if self.t > self.time || m.at(self.to) {
            return Step::Done;
        }
        if self.gulp <= 0.0 && rng.unit() < ctx.dt * 0.25 {
            self.gulp = 0.8;
        }
        let pose = if self.gulp > 0.0 {
            self.gulp -= ctx.dt;
            Overlay { head: 0.5 * (self.gulp / 0.8 * std::f32::consts::PI).sin(), ..Default::default() }
        } else {
            Overlay { head: -0.75, turn: self.swing * (self.t * 2.6).sin(), ..Default::default() }
        };
        let label = if self.gulp > 0.0 { "swallowing" } else { "sweeping" };
        Step::On(label.into(), vec![Out { drive: Drive::Go { to: self.to, speed: m.slow * 0.35 }, pose, ..Default::default() }])
    }
}
