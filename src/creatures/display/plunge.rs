//! `plunge`: a kingfisher fishing. It flies out over the water, hovers a
//! moment a few metres up, folds and drops straight in, comes out with a
//! fish (its entry's `food`) and flies back to its perch, where it beats the fish
//! against the branch with hard swings of its head before swallowing.
//!
//! Params: `hover` (metres above the water it hangs), `reach` (metres it
//! looks for water), `bash` (head swings on the perch).

use super::{heading, Cast, Ctx, Drive, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "plunge",
    rate: 8.0,
    holds: 0.4,
    cast: Cast::solo,
    params: &["hover", "reach", "bash"],
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
    Out(Hop),
    Hover,
    Drop,
    Under,
    Back(Hop),
    Bash,
}

struct Plunge {
    perch: Vec3,
    yaw: f32,
    over: Vec3,
    surface: f32,
    bash: u32,
    phase: Phase,
    t: f32,
}

/// The water's surface at (x, z): a river's, or the sea.
fn surface(ctx: &Ctx, x: f32, z: f32) -> Option<f32> {
    match ctx.terrain.inland_water(x, z) {
        Some((level, depth)) if depth > 0.4 => Some(level),
        _ => ctx.terrain.deep(x, z).then_some(ctx.terrain.sea),
    }
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let reach = p.num("reach", 30.0, rng);
    let (x, z, s) = (0..20).find_map(|_| {
        let a = rng.unit() * std::f32::consts::TAU;
        let r = 2.0 + rng.unit() * (reach - 2.0);
        let (x, z) = (m.pos.x + a.sin() * r, m.pos.z + a.cos() * r);
        surface(ctx, x, z).map(|s| (x, z, s))
    })?;
    let over = Vec3::new(x, s + p.num("hover", 3.0, rng), z);
    Some(Box::new(Plunge { perch: m.pos, yaw: m.yaw, over, surface: s, bash: p.count("bash", [3, 6], rng), phase: Phase::Out(Hop::new(m.pos, over, m.slow.max(4.0))), t: 0.0 }))
}

impl Run for Plunge {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        let place = |pos: Vec3, yaw: f32, pose: Overlay| Out { drive: Drive::Place { pos, yaw }, pose, ..Default::default() };
        let (out, label) = match &mut self.phase {
            Phase::Out(h) => {
                let (pos, done) = h.step(ctx.dt);
                if done {
                    self.phase = Phase::Hover;
                    self.t = 0.0;
                }
                (place(pos, heading(m.pos, pos, m.yaw), Overlay::default()), "flying out")
            }
            Phase::Hover => {
                if self.t > 1.5 {
                    self.phase = Phase::Drop;
                    self.t = 0.0;
                }
                (place(self.over + Vec3::Y * 0.05 * (self.t * 20.0).sin(), m.yaw, Overlay { head: -0.6, ..Default::default() }), "hovering")
            }
            Phase::Drop => {
                // Bill first, falling fast.
                let y = self.over.y - 0.5 * 18.0 * self.t * self.t;
                if y <= self.surface - 0.2 {
                    self.phase = Phase::Under;
                    self.t = 0.0;
                }
                (place(Vec3::new(self.over.x, y.max(self.surface - 0.2), self.over.z), m.yaw, Overlay { head: -1.0, ..Default::default() }), "plunging")
            }
            Phase::Under => {
                if self.t > 0.6 {
                    let from = Vec3::new(self.over.x, self.surface + 0.1, self.over.z);
                    self.phase = Phase::Back(Hop::new(from, self.perch, m.slow.max(4.0)));
                    self.t = 0.0;
                }
                (place(Vec3::new(self.over.x, self.surface - 0.2, self.over.z), m.yaw, Overlay::default()), "under")
            }
            Phase::Back(h) => {
                let (pos, done) = h.step(ctx.dt);
                if done {
                    self.phase = Phase::Bash;
                    self.t = 0.0;
                }
                (Out { held: true, ..place(pos, heading(m.pos, pos, m.yaw), Overlay::default()) }, "back with a fish")
            }
            Phase::Bash => {
                let swings = self.t / 0.5;
                if swings > self.bash as f32 + 1.0 {
                    return Step::Done;
                }
                let turn = if swings < self.bash as f32 { 1.1 * (swings * std::f32::consts::PI).sin() } else { 0.0 };
                (Out { drive: Drive::Sit { pos: self.perch, yaw: self.yaw }, pose: Overlay { turn, head: -0.2, ..Default::default() }, held: true, ..Default::default() }, "beating the fish")
            }
        };
        Step::On(label.into(), vec![out])
    }
}
