//! `wash`: a raccoon goes to the water's edge, sits up and works its fore
//! paws in the shallows, rubbing and turning something over, head down
//! watching its hands, and now and then lifts them to its mouth, where its
//! entry's `food` (a crayfish) shows.
//!
//! Params: `time` (seconds), `reach` (metres it looks for water).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "wash",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["time", "reach"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Wash {
    edge: Vec3,
    yaw: f32,
    time: f32,
    there: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let reach = p.num("reach", 25.0, rng);
    // The nearest water in a ring of looks, and the dry ground just short of it.
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let mut r = 1.0;
        while r < reach {
            let q = m.pos + dir * r;
            if ctx.terrain.wet(q.x, q.z) {
                let edge = m.pos + dir * (r - m.half_w - 0.3).max(0.0);
                if best.is_none_or(|b| r < b.0) {
                    best = Some((r, edge, q));
                }
                break;
            }
            r += 0.7;
        }
    }
    let (_, edge, water) = best?;
    Some(Box::new(Wash { edge, yaw: face(edge, water), time: p.num("time", 10.0 + rng.unit() * 10.0, rng), there: false, t: 0.0 }))
}

impl Run for Wash {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        if !self.there {
            if m.at(self.edge) {
                self.there = true;
                self.t = 0.0;
            } else if self.t > 30.0 {
                return Step::Done;
            }
            return Step::On("going to the water".into(), vec![Out { drive: Drive::Go { to: self.edge, speed: m.slow }, ..Default::default() }]);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        // Every few seconds the paws come up to the mouth.
        let lift = ((t * 0.5).sin() - 0.8).max(0.0) * 5.0;
        let pose = Overlay { rear: 0.35, plant: 1.0, head: -0.5 + 0.4 * lift, paw: 0.5 * (t * 9.0).sin() * (1.0 - lift), raise: 0.3 + 0.6 * lift, ..Default::default() };
        Step::On(if lift > 0.1 { "tasting" } else { "washing" }.into(), vec![Out { drive: Drive::Stay { face: Some(self.yaw) }, pose, act: Some(Act::Sit), held: lift > 0.1, ..Default::default() }])
    }
}
