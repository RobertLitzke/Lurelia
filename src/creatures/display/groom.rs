//! `groom`: two of a group grooming. Horses stand side by side head to
//! tail and nibble along each other's withers (`style` "mutual"). Monkeys
//! sit, one working its hands and face through the other's fur while the
//! other sits slumped and still, and now and then they swap (`style` "one").
//!
//! Params: `time` (seconds), `style`.

use super::{face, gather, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "groom",
    rate: 6.0,
    holds: 0.8,
    cast,
    params: &["time"],
    words: &[("style", &["mutual", "one"])],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 20.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Groom {
    marks: [Vec3; 2],
    faces: [f32; 2],
    mutual: bool,
    time: f32,
    t: f32,
    met: bool,
    /// Which one grooms, for "one"; seconds until they swap.
    groomer: usize,
    swap: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mutual = p.word("style", "mutual") == "mutual";
    let mid = (a.pos + b.pos) * 0.5;
    let along = a.facing();
    let side = Vec3::new(-along.z, 0.0, along.x);
    let (marks, faces) = if mutual {
        // Side by side, head to tail, shoulders level with the other's withers.
        let off = side * (a.half_w + b.half_w) * 0.55;
        let fwd = along * a.snout * 0.35;
        ([mid - off + fwd, mid + off - fwd], [a.yaw, a.yaw + std::f32::consts::PI])
    } else {
        // The groomer sits behind and to one side of the other.
        let at_b = mid + along * 0.3;
        let at_a = at_b - along * (b.half_w + a.snout * 0.6) + side * b.half_w * 0.5;
        ([at_a, at_b], [face(at_a, at_b), a.yaw])
    };
    Some(Box::new(Groom { marks, faces, mutual, time: p.num("time", 15.0 + rng.unit() * 15.0, rng), t: 0.0, met: false, groomer: 0, swap: 8.0 + rng.unit() * 6.0 }))
}

impl Run for Groom {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if !self.met {
            let (outs, all) = gather(cast, &self.marks, &self.faces, Overlay::default());
            if all || self.t > 15.0 {
                self.met = true;
                self.t = 0.0;
            }
            return Step::On("coming together".into(), outs);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        if self.mutual {
            // Both nibbling, heads turned in to the other's shoulder.
            let outs = (0..2)
                .map(|i| {
                    let s = if i == 0 { 1.0 } else { -1.0 };
                    let nib = (t * 5.0 + i as f32 * 1.7).sin();
                    let pose = Overlay { turn: s * (0.9 + 0.15 * nib), head: -0.15 + 0.1 * nib, jaw: 0.15 * nib.max(0.0), ..Default::default() };
                    Out { drive: Drive::Stay { face: Some(self.faces[i]) }, pose, ..Default::default() }
                })
                .collect();
            return Step::On("grooming each other".into(), outs);
        }
        self.swap -= ctx.dt;
        if self.swap <= 0.0 {
            self.groomer = 1 - self.groomer;
            self.swap = 8.0 + rng.unit() * 6.0;
        }
        let g = self.groomer;
        let outs = (0..2)
            .map(|i| {
                let pose = if i == g {
                    let pick = (t * 6.0).sin();
                    Overlay { head: -0.5 + 0.1 * pick, turn: 0.2 * (t * 0.7).sin(), raise: 0.7 + 0.25 * pick, ..Default::default() }
                } else {
                    Overlay { head: -0.35, turn: 0.3 * (t * 0.2).sin(), ..Default::default() }
                };
                let face = face(cast[i].pos, cast[1 - i].pos);
                Out { drive: Drive::Stay { face: Some(if i == g { face } else { self.faces[i] }) }, pose, act: Some(Act::Sit), ..Default::default() }
            })
            .collect();
        Step::On("grooming".into(), outs)
    }
}
