//! `sky_dance`: a raptor's roller-coaster flight. It climbs steeply on
//! beating wings, tips over at the top and stoops, sweeps up out of the dive
//! into the next climb, a string of them across the sky, then goes back to
//! its circle. Circlers (a peregrine) and soaring birds (hawks, eagles,
//! vultures) both do it.
//!
//! When a second of its kind is near, the two dance together, one chasing
//! the other along the same course, and at the end they close, lock talons
//! and fall cartwheeling round each other before they let go.
//!
//! Params: `waves` (how many climbs and stoops), `height` (metres from
//! bottom to top), `wave_m` (metres across one), `cartwheel` (seconds the
//! pair falls locked; 0 for none).

use super::{heading, Cast, Ctx, Drive, From, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "sky_dance",
    rate: 6.0,
    holds: 0.0,
    cast,
    params: &["waves", "height", "wave_m", "cartwheel"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [1, 2], range: 150.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true } | Mover::Flocker { perched: false, soars: true })
}

enum Phase {
    /// Flying in to the start of the course.
    Join(Vec<Hop>),
    Waves,
    /// The pair flying together to lock.
    Close(Vec<Hop>),
    /// Locked, wheeling round each other and falling; seconds in.
    Wheel(f32),
}

struct SkyDance {
    centre: Vec3,
    r: f32,
    ang: f32,
    dir: f32,
    base: f32,
    height: f32,
    wave_m: f32,
    waves: u32,
    /// Metres flown along the course by the leader.
    s: f32,
    speed: f32,
    /// How far behind the leader the second flies along the course, metres.
    lag: f32,
    cartwheel: f32,
    /// Where the locked pair wheel round, and the axis they wheel about.
    hub: Vec3,
    axle: Vec3,
    phase: Phase,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let ground = ctx.terrain.height(m.pos.x, m.pos.z).unwrap_or(m.pos.y - 20.0).max(ctx.terrain.sea);
    let height = p.num("height", 15.0, rng);
    let base = m.pos.y.max(ground + 10.0 + if cast.len() > 1 { 25.0 } else { 0.0 });
    let r = 35.0 + rng.unit() * 15.0;
    let ang = rng.unit() * TAU;
    let centre = m.pos - Vec3::new(ang.cos(), 0.0, ang.sin()) * r;
    let speed = m.fast.max(m.slow).clamp(8.0, 18.0);
    let mut d = SkyDance {
        centre,
        r,
        ang,
        dir: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        base,
        height,
        wave_m: p.num("wave_m", 30.0, rng).max(8.0),
        waves: p.count("waves", [3, 6], rng).max(1),
        s: 0.0,
        speed,
        lag: 12.0,
        cartwheel: p.num("cartwheel", 4.0 + rng.unit() * 2.0, rng),
        hub: Vec3::ZERO,
        axle: Vec3::X,
        phase: Phase::Waves,
    };
    let hops = cast.iter().enumerate().map(|(i, c)| Hop::new(c.pos, d.on_course(-(i as f32) * d.lag), speed)).collect();
    d.phase = Phase::Join(hops);
    Some(Box::new(d))
}

/// Height over the bottom, 0 to 1, `f` of the way through a wave: a long
/// climb, then a short steep stoop.
fn wave(f: f32) -> f32 {
    if f < 0.65 {
        let u = f / 0.65;
        u * u * (3.0 - 2.0 * u)
    } else {
        let u = (f - 0.65) / 0.35;
        (1.0 - u) * (1.0 - u)
    }
}

impl SkyDance {
    /// The point `back` metres along the course from the leader (negative
    /// is behind).
    fn on_course(&self, back: f32) -> Vec3 {
        let s = (self.s + back).max(0.0);
        let f = (s / self.wave_m).fract();
        let a = self.ang + self.dir * back / self.r;
        let y = self.base + self.height * wave(f);
        Vec3::new(self.centre.x + a.cos() * self.r, y, self.centre.z + a.sin() * self.r)
    }

    fn stooping(&self, back: f32) -> bool {
        ((self.s + back).max(0.0) / self.wave_m).fract() >= 0.65
    }
}

impl Run for SkyDance {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let place = |m: &Member, pos: Vec3, pose: Overlay| Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, pose, ..Default::default() };
        let n = cast.len();
        match &mut self.phase {
            Phase::Join(hops) => {
                let mut all = true;
                let outs = hops
                    .iter_mut()
                    .zip(cast)
                    .map(|(h, m)| {
                        let (pos, done) = h.step(ctx.dt);
                        all &= done;
                        place(m, pos, Overlay::default())
                    })
                    .collect();
                if all {
                    self.phase = Phase::Waves;
                }
                Step::On("setting off".into(), outs)
            }
            Phase::Waves => {
                let pace = if self.stooping(0.0) { 1.6 } else { 0.75 };
                let ds = self.speed * pace * ctx.dt;
                self.s += ds;
                self.ang += self.dir * ds / self.r;
                if self.s > self.wave_m * self.waves as f32 {
                    if n < 2 || self.cartwheel <= 0.0 {
                        return Step::Done;
                    }
                    // Close: the second comes up under the first.
                    let meet = cast[0].pos + Vec3::Y * 2.0;
                    self.hub = meet;
                    let along = (cast[0].pos - cast[1].pos).with_y(0.0).normalize_or(Vec3::X);
                    self.axle = Vec3::new(-along.z, 0.0, along.x);
                    let hops = vec![Hop::new(cast[0].pos, meet + along * 1.0, self.speed * 0.6), Hop::new(cast[1].pos, meet - along * 1.0, self.speed)];
                    self.phase = Phase::Close(hops);
                    return Step::On("closing".into(), (0..n).map(|i| place(&cast[i], cast[i].pos, Overlay::default())).collect());
                }
                let k = (self.s / self.wave_m) as u32 + 1;
                let outs = (0..n)
                    .map(|i| {
                        let back = -(i as f32) * self.lag;
                        let stoop = self.stooping(back);
                        let pose = Overlay { head: if stoop { -0.4 } else { 0.2 }, tail: if stoop { -0.2 } else { 0.3 }, ..Default::default() };
                        place(&cast[i], self.on_course(back), pose)
                    })
                    .collect();
                let label = if self.stooping(0.0) { format!("stooping, {k} of {}", self.waves) } else { format!("climbing, {k} of {}", self.waves) };
                Step::On(label, outs)
            }
            Phase::Close(hops) => {
                let mut all = true;
                let outs: Vec<Out> = hops
                    .iter_mut()
                    .zip(cast)
                    .map(|(h, m)| {
                        let (pos, done) = h.step(ctx.dt);
                        all &= done;
                        place(m, pos, Overlay { wings: 0.5, ..Default::default() })
                    })
                    .collect();
                if all {
                    self.phase = Phase::Wheel(0.0);
                }
                Step::On("closing".into(), outs)
            }
            Phase::Wheel(t) => {
                *t += ctx.dt;
                let t = *t;
                if t > self.cartwheel {
                    return Step::Done;
                }
                // Feet locked at the hub, bodies opposite each other, the
                // pair turning over and over about a level axle as it falls.
                let fall = 4.0 * t;
                let floor = ctx.terrain.height(self.hub.x, self.hub.z).unwrap_or(self.hub.y - 40.0).max(ctx.terrain.sea) + 6.0;
                let hub = Vec3::new(self.hub.x, (self.hub.y - fall).max(floor), self.hub.z);
                let spin = t * TAU * 0.8;
                let across = self.axle.cross(Vec3::Y);
                let outs = (0..2)
                    .map(|i| {
                        let a = spin + i as f32 * std::f32::consts::PI;
                        let arm = (across * a.cos() + Vec3::Y * a.sin()) * 0.6;
                        let pos = hub + arm;
                        let yaw = super::face(pos, hub);
                        let pose = Overlay { roll: a, wings: 0.9, tail: 0.3, ..Default::default() };
                        Out { drive: Drive::Place { pos, yaw }, pose, ..Default::default() }
                    })
                    .collect();
                Step::On("locked, cartwheeling".into(), outs)
            }
        }
    }
}
