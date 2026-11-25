//! `greet`: two of a family walk up to each other and touch heads, lean in
//! and rub, heads going up and round each other, tails going. Elephants and
//! mammoths entwine trunks; wolves come in low with tails wagging.
//!
//! Params: `time` (seconds touching), `low` (radians the head is carried
//! low: wolves), `wag` (tail wag, radians).

use super::{face, gather, Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "greet",
    rate: 6.0,
    holds: 0.7,
    cast,
    params: &["time", "low", "wag"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Family, count: [2, 2], range: 30.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Greet {
    marks: [Vec3; 2],
    time: f32,
    low: f32,
    wag: f32,
    met: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    let (a, b) = (&cast[0], &cast[1]);
    let mid = (a.pos + b.pos) * 0.5;
    let axis = (b.pos - a.pos).with_y(0.0).normalize_or(-a.facing());
    // Snouts just meeting, a little offset so the heads pass alongside.
    let side = Vec3::new(-axis.z, 0.0, axis.x) * 0.15 * (a.half_w + b.half_w);
    Some(Box::new(Greet {
        marks: [mid - axis * a.snout * 0.9 - side, mid + axis * b.snout * 0.9 + side],
        time: p.num("time", 5.0 + rng.unit() * 5.0, rng),
        low: p.num("low", 0.0, rng),
        wag: p.num("wag", 0.0, rng),
        met: false,
        t: 0.0,
    }))
}

impl Run for Greet {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let faces = [face(cast[0].pos, cast[1].pos), face(cast[1].pos, cast[0].pos)];
        if !self.met {
            let pose = Overlay { head: -self.low, wag: self.wag * (self.t * 9.0).sin(), ..Default::default() };
            let (mut outs, all) = gather(cast, &self.marks, &faces, pose);
            for o in &mut outs {
                o.pose = pose;
            }
            if all || self.t > 20.0 {
                self.met = true;
                self.t = 0.0;
            }
            return Step::On("coming up".into(), outs);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let outs = (0..2)
            .map(|i| {
                let s = if i == 0 { 1.0 } else { -1.0 };
                let pose = Overlay {
                    head: -self.low + 0.25 * (t * 1.3 + i as f32).sin(),
                    turn: s * 0.35 * (t * 0.9).sin(),
                    wag: self.wag * (t * 9.0).sin(),
                    tail: 0.2,
                    ..Default::default()
                };
                Out { drive: Drive::Stay { face: Some(faces[i]) }, pose, ..Default::default() }
            })
            .collect();
        Step::On("greeting".into(), outs)
    }
}
