//! `head_butt`: two males square up a few metres apart, heads low, charge,
//! and meet head to head; they back off, shaking it off, and go again, a few
//! times. Bighorn rams rear onto their hind legs as they set off (`rear`);
//! bison and triceratops shove from all fours.
//!
//! Params: `clashes` (how many), `backoff` (metres between their snouts when
//! they square up), `charge` (share of its running speed it charges at),
//! `rear` (radians it rears up at the start of a charge).

use super::{face, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "head_butt",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["clashes", "backoff", "charge", "rear"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    let male = MemberDef { age: Some("adult".into()), sex: Some("male".into()) };
    Cast { from: From::Same, count: [2, 2], range: 25.0, members: vec![male] }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy)]
enum Phase {
    /// Walking to their marks either side of the middle.
    Approach,
    /// Facing each other, heads low, for `hold` seconds.
    Square { hold: f32 },
    Charge,
    Clash,
    /// Backing away, still facing each other.
    Recoil,
    Finish,
}

struct HeadButt {
    mid: Vec3,
    /// From the first toward the second.
    axis: Vec3,
    backoff: f32,
    clashes: u32,
    done: u32,
    charge: f32,
    rear: f32,
    phase: Phase,
    /// Seconds in this phase.
    t: f32,
    /// Seconds allowed to walk to the marks: longer for a slow animal far
    /// off (tortoises).
    approach_s: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let d = (b.pos - a.pos).with_y(0.0);
    let axis = if d.length() > 0.1 { d.normalize() } else { -a.facing() };
    let tall = a.height.max(b.height);
    let slow = a.slow.min(b.slow).max(0.05);
    let approach_s = (d.length() * 0.5 / slow * 1.5).clamp(14.0, 90.0);
    Some(Box::new(HeadButt {
        approach_s,
        mid: (a.pos + b.pos) * 0.5,
        axis,
        backoff: p.num("backoff", (tall * 1.6).clamp(2.5, 7.0), rng),
        clashes: p.count("clashes", [2, 4], rng).max(1),
        done: 0,
        charge: p.num("charge", 0.6, rng).clamp(0.1, 1.0),
        rear: p.num("rear", 0.0, rng),
        phase: Phase::Approach,
        t: 0.0,
    }))
}

impl HeadButt {
    /// Where member `i` stands to square up: its snout half the backoff
    /// short of the middle.
    fn mark(&self, m: &Member, i: usize) -> Vec3 {
        let side = if i == 0 { -1.0 } else { 1.0 };
        self.mid + self.axis * side * (m.snout + self.backoff * 0.5)
    }

    fn next(&mut self, p: Phase) {
        self.phase = p;
        self.t = 0.0;
    }
}

impl Run for HeadButt {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = |i: usize| face(cast[i].pos, cast[1 - i].pos);
        let gap = (cast[0].pos - cast[1].pos).with_y(0.0).length();
        // Centre to centre when their heads meet.
        let meet = cast[0].snout + cast[1].snout;
        let both = |o: Out| vec![o, o];
        let stay = |pose: Overlay| (0..2).map(|i| Out { drive: Drive::Stay { face: Some(faces(i)) }, pose, ..Default::default() }).collect::<Vec<_>>();
        let (phase, outs): (String, Vec<Out>) = match self.phase {
            Phase::Approach => {
                let marks = [self.mark(&cast[0], 0), self.mark(&cast[1], 1)];
                let there = [cast[0].at(marks[0]), cast[1].at(marks[1])];
                if self.t > self.approach_s && gap > meet + self.backoff * 3.0 {
                    // Kept apart by something between them: give it up.
                    return Step::Done;
                }
                if there[0] && there[1] || self.t > self.approach_s {
                    let hold = 1.0 + rng.unit() * 1.5;
                    self.next(Phase::Square { hold });
                }
                let outs = (0..2)
                    .map(|i| {
                        let drive = if there[i] { Drive::Stay { face: Some(faces(i)) } } else { Drive::Go { to: marks[i], speed: cast[i].slow } };
                        Out { drive, ..Default::default() }
                    })
                    .collect();
                ("squaring up".into(), outs)
            }
            Phase::Square { hold } => {
                if self.t > hold {
                    // Too close to get up any speed: back off first.
                    self.next(if gap < meet + self.backoff * 0.5 { Phase::Recoil } else { Phase::Charge });
                }
                ("squared up".into(), stay(Overlay { head: -0.45, tail: 0.1, ..Default::default() }))
            }
            Phase::Charge => {
                let label = format!("charging, {} of {}", self.done + 1, self.clashes);
                if gap <= meet + 0.15 || self.t > 4.0 {
                    self.done += 1;
                    self.next(Phase::Clash);
                }
                // Up on the hind legs as it sets off, and down onto the
                // blow.
                let k = (self.t / 0.9).min(1.0);
                let rear = self.rear * (PI * k).sin().max(0.0) - 0.08 * k;
                let outs = (0..2)
                    .map(|i| {
                        let m = &cast[i];
                        let toward = (cast[1 - i].pos - m.pos).with_y(0.0).normalize_or_zero();
                        // Its share of the gap, to where the heads meet.
                        let to = m.pos + toward * ((gap - meet) * 0.5 + 0.3);
                        Out {
                            drive: Drive::Go { to, speed: m.fast * self.charge },
                            pose: Overlay { head: -0.8, rear, tail: 0.3, ..Default::default() },
                            ..Default::default()
                        }
                    })
                    .collect();
                (label, outs)
            }
            Phase::Clash => {
                if self.t > 0.45 {
                    self.next(Phase::Recoil);
                }
                // The heads snap up and the bodies rock back.
                let jolt = (self.t / 0.45 * PI).sin();
                (format!("clash {} of {}", self.done, self.clashes), stay(Overlay { head: -0.8 + 0.7 * jolt, rear: 0.12 * jolt, tail: 0.3, ..Default::default() }))
            }
            Phase::Recoil => {
                if gap >= meet + self.backoff || self.t > 3.5 {
                    if self.done >= self.clashes {
                        self.next(Phase::Finish);
                    } else {
                        let hold = 0.8 + rng.unit() * 1.5;
                        self.next(Phase::Square { hold });
                    }
                }
                let shake = 0.35 * (self.t * 14.0).sin() * (1.0 - self.t / 1.2).max(0.0);
                let outs = (0..2)
                    .map(|i| Out { drive: Drive::Back { speed: cast[i].slow * 0.9 }, pose: Overlay { head: -0.2, turn: shake, ..Default::default() }, ..Default::default() })
                    .collect();
                ("backing off".into(), outs)
            }
            Phase::Finish => {
                if self.t > 2.5 {
                    return Step::Done;
                }
                let shake = 0.3 * (self.t * 12.0).sin() * (1.0 - self.t / 2.5);
                ("shaking it off".into(), both(Out { pose: Overlay { turn: shake, ..Default::default() }, ..Default::default() }))
            }
        };
        Step::On(phase, outs)
    }
}
