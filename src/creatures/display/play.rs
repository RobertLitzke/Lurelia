//! `play`: the young of one family chase each other round, now one in
//! front and now another, with sudden bounds and pounces, a roll in the
//! grass, and a stop to face off before they go again.
//!
//! Params: `time` (seconds), `pounce` (upward speed of a pounce, m/s).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "play",
    rate: 8.0,
    holds: 0.7,
    cast,
    params: &["time", "pounce"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    let young = MemberDef { age: Some("juvenile".into()), sex: None };
    Cast { from: From::Family, count: [2, 4], range: 30.0, members: vec![young] }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy)]
enum Doing {
    /// Running after member `k`.
    Chase(usize),
    /// Running off from where it is, toward `to`.
    Flee(Vec3),
    /// Down on its side, a roll.
    Roll,
    /// Facing member `k`, front end low.
    FaceOff(usize),
}

struct Play {
    time: f32,
    pounce: f32,
    t: f32,
    doing: Vec<(Doing, f32)>,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let n = cast.len();
    let doing = (0..n).map(|i| (if i == 0 { Doing::Flee(cast[0].pos + cast[0].facing() * 8.0) } else { Doing::Chase(0) }, 2.0 + rng.unit() * 2.0)).collect();
    Some(Box::new(Play { time: p.num("time", 15.0 + rng.unit() * 15.0, rng), pounce: p.num("pounce", 2.5, rng), t: 0.0, doing }))
}

impl Run for Play {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let n = cast.len();
        let centre = cast.iter().map(|m| m.pos).sum::<Vec3>() / n as f32;
        let mut outs = Vec::new();
        for (i, m) in cast.iter().enumerate() {
            let (doing, left) = &mut self.doing[i];
            *left -= ctx.dt;
            let mut hop = 0.0;
            if *left <= 0.0 {
                // Something else: chase another, run off, roll, or face off.
                let other = (i + 1 + (rng.unit() * (n - 1) as f32) as usize) % n;
                let r = rng.unit();
                *doing = if r < 0.4 {
                    hop = self.pounce * 0.6;
                    Doing::Chase(other)
                } else if r < 0.7 {
                    let a = rng.unit() * std::f32::consts::TAU;
                    Doing::Flee(centre + Vec3::new(a.sin(), 0.0, a.cos()) * (4.0 + rng.unit() * 6.0))
                } else if r < 0.85 {
                    Doing::Roll
                } else {
                    Doing::FaceOff(other)
                };
                *left = match doing {
                    Doing::Roll => 1.6,
                    Doing::FaceOff(_) => 1.0 + rng.unit(),
                    _ => 1.5 + rng.unit() * 2.5,
                };
            }
            let out = match *doing {
                Doing::Chase(k) => {
                    let to = cast[k].pos;
                    if (to - m.pos).length() < m.snout + 0.6 && rng.unit() < ctx.dt * 2.0 {
                        hop = self.pounce;
                    }
                    Out { drive: Drive::Go { to, speed: m.fast.max(m.slow) * 0.8 }, pose: Overlay { head: -0.15, tail: 0.4, ..Default::default() }, hop, ..Default::default() }
                }
                Doing::Flee(to) => Out { drive: Drive::Go { to, speed: m.fast.max(m.slow) * 0.8 }, pose: Overlay { tail: 0.5, ..Default::default() }, hop, ..Default::default() },
                Doing::Roll => {
                    let u = 1.0 - *left / 1.6;
                    let roll = 2.2 * (u * std::f32::consts::PI).sin();
                    Out { drive: Drive::Stay { face: None }, pose: Overlay { roll, kick: 0.5 * (u * 20.0).sin(), ..Default::default() }, act: Some(Act::Lie), ..Default::default() }
                }
                Doing::FaceOff(k) => Out {
                    drive: Drive::Stay { face: Some(face(m.pos, cast[k].pos)) },
                    pose: Overlay { rear: -0.2, plant: 1.0, head: 0.1, tail: 0.6, wag: 0.4 * (self.t * 10.0).sin(), ..Default::default() },
                    ..Default::default()
                },
            };
            outs.push(out);
        }
        Step::On("playing".into(), outs)
    }
}
