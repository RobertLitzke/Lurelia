//! `sentinel`: a marmot or a prairie dog sits up tall on its haunches and
//! looks slowly round, and one or two near it do the same. A prairie dog
//! now and then throws itself up and back with its head to the sky and its
//! fore paws out (the jump-yip), and the ones near it answer.
//!
//! Params: `time` (seconds it keeps watch), `rear` (radians it tips up past
//! its own sit), `yip` (chance of a jump-yip each few seconds, 0 to 1),
//! `hop` (upward speed of the jump, m/s).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "sentinel",
    rate: 8.0,
    holds: 0.7,
    cast,
    params: &["time", "rear", "yip", "hop"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [1, 3], range: 20.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Watcher {
    /// Seconds before it sits up.
    wait: f32,
    /// Where it is looking, radians left of ahead, and where it turns to next.
    look: f32,
    to: f32,
    /// Seconds into a jump-yip, if it is in one.
    yip: Option<f32>,
}

struct Sentinel {
    who: Vec<Watcher>,
    time: f32,
    rear: f32,
    yip: f32,
    hop: f32,
    t: f32,
    /// Seconds to the next chance of a yip.
    next_yip: f32,
}

/// How long a jump-yip lasts, seconds.
const YIP_S: f32 = 0.9;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let who = (0..cast.len()).map(|i| Watcher { wait: i as f32 * (0.4 + rng.unit() * 0.8), look: 0.0, to: 0.0, yip: None }).collect();
    Some(Box::new(Sentinel {
        who,
        time: p.num("time", 8.0 + rng.unit() * 10.0, rng),
        rear: p.num("rear", 0.3, rng),
        yip: p.num("yip", 0.0, rng).clamp(0.0, 1.0),
        hop: p.num("hop", 1.6, rng),
        t: 0.0,
        next_yip: 2.0,
    }))
}

impl Run for Sentinel {
    fn step(&mut self, _cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        // One yip sets the rest off, a beat apart.
        self.next_yip -= ctx.dt;
        if self.next_yip <= 0.0 {
            self.next_yip = 2.5 + rng.unit() * 2.0;
            if rng.unit() < self.yip {
                for (i, w) in self.who.iter_mut().enumerate() {
                    if w.wait <= 0.0 {
                        w.yip = Some(-(i as f32) * 0.5);
                    }
                }
            }
        }
        let mut yipping = false;
        let outs = self
            .who
            .iter_mut()
            .map(|w| {
                if w.wait > 0.0 {
                    w.wait -= ctx.dt;
                    return Out { drive: Drive::Stay { face: None }, ..Default::default() };
                }
                // Turns its head to a new bearing every second or two.
                if (w.look - w.to).abs() < 0.05 {
                    w.to = (rng.unit() - 0.5) * 2.4;
                }
                w.look += (w.to - w.look).clamp(-1.2 * ctx.dt, 1.2 * ctx.dt);
                let mut pose = Overlay { rear: self.rear, plant: 1.0, head: -self.rear * 0.8, turn: w.look, ..Default::default() };
                let mut hop = 0.0;
                if let Some(t) = &mut w.yip {
                    *t += ctx.dt;
                    if *t >= 0.0 {
                        yipping = true;
                        let k = (*t / YIP_S).min(1.0);
                        let up = (PI * k).sin();
                        pose.rear += 0.5 * up;
                        pose.head += 1.0 * up;
                        pose.arms = 0.8 * up;
                        pose.raise = 0.6 * up;
                        pose.turn *= 1.0 - up;
                        if *t < ctx.dt * 1.5 {
                            hop = self.hop;
                        }
                    }
                    if *t >= YIP_S {
                        w.yip = None;
                    }
                }
                Out { drive: Drive::Stay { face: None }, act: Some(Act::Sit), pose, hop, flash: None, pale: 0.0, held: false }
            })
            .collect();
        Step::On(if yipping { "jump-yip".into() } else { "keeping watch".into() }, outs)
    }
}
