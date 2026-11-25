//! `fish`: a bear wades out into a shallow riffle, stands facing upstream
//! with its head low over the water, and every few seconds lunges and pins
//! with its fore paws. Now and then one comes up with a fish -- its entry's
//! `food` (a salmon) -- and carries it to the nearest dry bank to eat it.
//!
//! Params: `time` (seconds it watches the water at most), `reach` (metres it
//! looks for shallow water), `catch` (the chance a lunge brings up a fish),
//! `deep` (the deepest water it wades into, metres).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "fish",
    rate: 8.0,
    holds: 0.5,
    cast: Cast::solo,
    params: &["time", "reach", "catch", "deep"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(PartialEq)]
enum Phase {
    Wading,
    Watching,
    Lunging,
    Carrying,
    Eating,
}

struct Fish {
    spot: Vec3,
    bank: Vec3,
    yaw: f32,
    time: f32,
    catch: f32,
    phase: Phase,
    /// Seconds into the current phase, and into the watching altogether.
    t: f32,
    watched: f32,
    /// Seconds to the next lunge.
    next: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let reach = p.num("reach", 30.0, rng);
    let deep = p.num("deep", 0.8, rng);
    // The nearest river water shallow enough to stand in, looking out in a
    // ring, and the last dry ground before it on the same line: the bank.
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for k in 0..32 {
        let a = k as f32 / 32.0 * std::f32::consts::TAU;
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let mut r = 0.0;
        let mut dry = m.pos;
        while r < reach {
            let q = m.pos + dir * r;
            match ctx.terrain.inland_water(q.x, q.z) {
                Some((_, d)) if d > 0.15 && d < deep => {
                    // A couple of metres in from the edge, where the fish run.
                    let inn = m.pos + dir * (r + 1.5 + m.half_w);
                    let ok = ctx.terrain.inland_water(inn.x, inn.z).is_some_and(|(_, d)| d < deep);
                    let spot = if ok { inn } else { q };
                    if best.is_none_or(|b| r < b.0) {
                        best = Some((r, spot, dry));
                    }
                    break;
                }
                Some(_) => break,
                None => dry = q,
            }
            r += 0.8;
        }
    }
    let (_, spot, bank) = best?;
    // Face upstream, into the current, where the fish come from; across
    // the water if there is none to speak of.
    let c = ctx.terrain.current(spot.x, spot.z);
    let yaw = if c.length() > 0.05 { face(spot, spot - Vec3::new(c.x, 0.0, c.y)) } else { face(bank, spot) };
    let time = p.num("time", 25.0 + rng.unit() * 35.0, rng);
    let catch = p.num("catch", 0.3, rng);
    Some(Box::new(Fish { spot, bank, yaw, time, catch, phase: Phase::Wading, t: 0.0, watched: 0.0, next: 2.0 + rng.unit() * 4.0 }))
}

impl Run for Fish {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        match self.phase {
            Phase::Wading => {
                if m.at(self.spot) {
                    self.phase = Phase::Watching;
                    self.t = 0.0;
                } else if self.t > 40.0 {
                    return Step::Done;
                }
                Step::On("wading out".into(), vec![Out { drive: Drive::Go { to: self.spot, speed: m.slow }, ..Default::default() }])
            }
            Phase::Watching => {
                self.watched += ctx.dt;
                if self.watched > self.time {
                    return Step::Done;
                }
                if self.t > self.next {
                    self.phase = Phase::Lunging;
                    self.t = 0.0;
                }
                // Head low over the water, weaving a little as it follows a fish.
                let pose = Overlay { head: -0.55 + 0.08 * (self.watched * 1.3).sin(), turn: 0.25 * (self.watched * 0.7).sin(), ..Default::default() };
                Step::On("watching the water".into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
            }
            Phase::Lunging => {
                let t = self.t;
                if t > 1.2 {
                    if rng.unit() < self.catch {
                        self.phase = Phase::Carrying;
                    } else {
                        self.phase = Phase::Watching;
                        self.next = 3.0 + rng.unit() * 6.0;
                    }
                    self.t = 0.0;
                }
                // Up on the hind legs, then down hard with both fore paws.
                let up = (t / 0.45).min(1.0) * (1.0 - ((t - 0.45) / 0.35).clamp(0.0, 1.0));
                let pose = Overlay { rear: 0.45 * up, plant: 1.0, raise: 0.9 * up, head: -0.2 - 0.5 * (1.0 - up), jaw: 0.3 * up, ..Default::default() };
                Step::On("lunging".into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
            }
            Phase::Carrying => {
                if m.at(self.bank) {
                    self.phase = Phase::Eating;
                    self.t = 0.0;
                } else if self.t > 40.0 {
                    return Step::Done;
                }
                let pose = Overlay { head: 0.15, ..Default::default() };
                Step::On("carrying a fish".into(), vec![Out { drive: Drive::Go { to: self.bank, speed: m.slow }, pose, held: true, ..Default::default() }])
            }
            Phase::Eating => {
                if self.t > 14.0 {
                    return Step::Done;
                }
                let pose = Overlay { head: -0.6, jaw: 0.25 * (self.t * 4.0).sin().abs(), paw: 0.15 * (self.t * 2.0).sin(), ..Default::default() };
                let face = face(self.bank, self.spot) + std::f32::consts::PI;
                Step::On("eating a fish".into(), vec![Out { drive: Drive::Stay { face: Some(face) }, pose, act: Some(Act::Sit), held: true, ..Default::default() }])
            }
        }
    }
}
