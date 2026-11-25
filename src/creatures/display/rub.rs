//! `rub`: walks to a tree and scratches on it. A bear stands up with its
//! back to the trunk and works up and down it (`style` "back"); a bison or
//! an elephant leans its flank or forehead on it and shoves back and forth
//! (`style` "flank").
//!
//! Params: `time` (seconds rubbing), `reach` (metres it looks for a trunk),
//! `style`.

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "rub",
    rate: 5.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["time", "reach"],
    words: &[("style", &["back", "flank"])],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Rub {
    mark: Vec3,
    yaw: f32,
    back: bool,
    time: f32,
    there: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let reach = p.num("reach", 30.0, rng);
    let trunk = ctx
        .flora
        .near(m.pos.x, m.pos.z, reach, None)
        .filter(|r| r.trunk > 0.05 && r.height > m.height * 1.5)
        .min_by(|a, b| (a.pos - m.pos).length().total_cmp(&(b.pos - m.pos).length()))?
        .pos;
    let back = p.word("style", "flank") == "back";
    let from = (m.pos - trunk).with_y(0.0).normalize_or(Vec3::X);
    let (mark, yaw) = if back {
        // Back to the trunk: standing up, its hindquarters at the bark.
        let mark = trunk + from * (m.half_w + 0.35);
        (mark, face(trunk, mark))
    } else {
        // Side on, the flank against the bark.
        let side = Vec3::new(-from.z, 0.0, from.x);
        let mark = trunk + from * (m.half_w + 0.3);
        (mark, face(mark, mark + side))
    };
    Some(Box::new(Rub { mark, yaw, back, time: p.num("time", 8.0 + rng.unit() * 8.0, rng), there: false, t: 0.0 }))
}

impl Run for Rub {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        // Trunks are steered round, so close by the mark will do.
        let near = (self.mark - m.pos).with_y(0.0).length() < 1.0 + m.half_w * 1.5;
        if !self.there {
            if near || self.t > 25.0 {
                self.there = near;
                if !self.there {
                    return Step::Done;
                }
                self.t = 0.0;
            }
            let out = Out { drive: Drive::Go { to: self.mark, speed: m.slow }, ..Default::default() };
            return Step::On("going to a tree".into(), vec![out]);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let ease = (t / 1.2).min(1.0) * ((self.time - t) / 1.2).min(1.0);
        let pose = if self.back {
            // Up on its hind legs, shuffling up and down the bark.
            Overlay { rear: 1.1 * ease, plant: 1.0, roll: 0.12 * (t * 2.2).sin() * ease, head: 0.3 * ease, turn: 0.25 * (t * 0.8).sin(), raise: 0.3 * ease, ..Default::default() }
        } else {
            Overlay { roll: -0.12 * ease * (0.6 + 0.4 * (t * 2.5).sin()), head: -0.2 + 0.15 * (t * 1.1).sin(), turn: -0.3 * ease, ..Default::default() }
        };
        Step::On("rubbing".into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, ..Default::default() }])
    }
}
