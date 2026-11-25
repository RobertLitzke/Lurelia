//! `dance`: cranes in a loose ring, each to its own steps -- bowing low,
//! leaping with wings open, standing tall with wings spread, stepping round
//! the ring -- for half a minute or so, then back to feeding.
//!
//! Params: `time` (seconds it lasts), `ring` (radius of the ring, metres),
//! `leap` (upward speed of a leap, m/s).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "dance",
    rate: 6.0,
    holds: 0.7,
    cast,
    params: &["time", "ring", "leap"],
    words: &[],
    approach: false,
    roles: &[Role::Wing],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 6], range: 25.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum Move {
    Stand,
    Bow,
    Leap,
    Spread,
    Step,
}

struct Dancer {
    /// Its place round the ring, radians.
    angle: f32,
    step: Move,
    /// Seconds into this step, and how long it lasts.
    t: f32,
    len: f32,
}

struct Dance {
    centre: Vec3,
    ring: f32,
    time: f32,
    leap: f32,
    dancing: bool,
    t: f32,
    who: Vec<Dancer>,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / cast.len() as f32;
    let tall = cast.iter().map(|m| m.height).fold(0.0, f32::max);
    // Spread evenly round the ring, in the order they stand round it now.
    let mut order: Vec<(usize, f32)> = cast
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let d = m.pos - centre;
            (i, d.x.atan2(d.z) + if d.length() < 0.1 { i as f32 } else { 0.0 })
        })
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    let base = order.first().map_or(0.0, |o| o.1);
    let mut who: Vec<Dancer> = (0..cast.len()).map(|_| Dancer { angle: 0.0, step: Move::Stand, t: 0.0, len: 0.0 }).collect();
    for (k, (i, _)) in order.iter().enumerate() {
        who[*i].angle = base + k as f32 * TAU / cast.len() as f32;
        who[*i].len = 0.3 + rng.unit() * 1.2;
    }
    Some(Box::new(Dance {
        centre,
        ring: p.num("ring", (tall * 1.4).max(1.2), rng),
        time: p.num("time", 20.0 + rng.unit() * 15.0, rng),
        leap: p.num("leap", 4.5, rng),
        dancing: false,
        t: 0.0,
        who,
    }))
}

impl Dance {
    fn place(&self, angle: f32) -> Vec3 {
        self.centre + Vec3::new(angle.sin(), 0.0, angle.cos()) * self.ring
    }
}

fn pick(rng: &mut Rng) -> (Move, f32) {
    match (rng.unit() * 10.0) as u32 {
        0..=1 => (Move::Bow, 0.8 + rng.unit() * 0.4),
        2..=3 => (Move::Leap, 1.0),
        4..=5 => (Move::Spread, 1.0 + rng.unit() * 0.8),
        6..=7 => (Move::Step, 1.2 + rng.unit() * 0.6),
        _ => (Move::Stand, 0.4 + rng.unit() * 1.0),
    }
}

impl Run for Dance {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if !self.dancing {
            let there = cast.iter().zip(&self.who).all(|(m, d)| m.at(self.place(d.angle)));
            if there || self.t > 12.0 {
                self.dancing = true;
                self.t = 0.0;
            }
            let outs = cast
                .iter()
                .zip(&self.who)
                .map(|(m, d)| {
                    let to = self.place(d.angle);
                    let drive = if m.at(to) { Drive::Stay { face: Some(face(m.pos, self.centre)) } } else { Drive::Go { to, speed: m.slow } };
                    Out { drive, ..Default::default() }
                })
                .collect();
            return Step::On("gathering".into(), outs);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let mut outs = Vec::with_capacity(cast.len());
        for i in 0..cast.len() {
            let m = &cast[i];
            let d = &mut self.who[i];
            d.t += ctx.dt;
            let first = d.t <= ctx.dt;
            if d.t >= d.len {
                let (s, len) = pick(rng);
                d.step = s;
                d.len = len;
                d.t = 0.0;
                if s == Move::Step {
                    d.angle += (if rng.unit() < 0.5 { 1.0 } else { -1.0 }) * (0.4 + rng.unit() * 0.4);
                }
            }
            let k = (d.t / d.len).clamp(0.0, 1.0);
            let arc = (PI * k).sin();
            let to_centre = face(m.pos, self.centre);
            let (angle, step) = (d.angle, d.step);
            let mut out = Out { drive: Drive::Stay { face: Some(to_centre) }, ..Default::default() };
            match step {
                Move::Stand => out.pose = Overlay { head: 0.2, ..Default::default() },
                Move::Bow => out.pose = Overlay { head: -0.9 * arc, rear: -0.2 * arc, wings: 0.35 * arc, tail: 0.2 * arc, ..Default::default() },
                Move::Leap => {
                    out.pose = Overlay { head: 0.4 * arc, wings: arc, rear: 0.15 * arc, ..Default::default() };
                    if first || d.t < ctx.dt * 1.5 {
                        out.hop = self.leap;
                    }
                }
                Move::Spread => out.pose = Overlay { head: 0.5 * arc, wings: arc, ..Default::default() },
                Move::Step => {
                    let to = self.centre + Vec3::new(angle.sin(), 0.0, angle.cos()) * self.ring;
                    out.drive = if (to - m.pos).with_y(0.0).length() > 0.4 { Drive::Go { to, speed: m.slow } } else { Drive::Stay { face: Some(to_centre) } };
                    out.pose = Overlay { wings: 0.4 * arc, head: 0.1, ..Default::default() };
                }
            }
            outs.push(out);
        }
        Step::On("dancing".into(), outs)
    }
}
