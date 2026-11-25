//! `nurse`: a young one comes to its mother, stands at her flank facing her
//! tail, and nurses, butting up under her belly with its tail going, while
//! she stands and looks round at it. A family display: the two must be of
//! one family group (creatures/herd.rs `Kin`).
//!
//! Params: `time` (seconds it nurses).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "nurse",
    rate: 10.0,
    holds: 1.0,
    cast,
    params: &["time"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    let mother = MemberDef { age: Some("adult".into()), sex: Some("female".into()) };
    let young = MemberDef { age: Some("juvenile".into()), sex: None };
    Cast { from: From::Family, count: [2, 2], range: 30.0, members: vec![mother, young] }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Nurse {
    /// Which side of her it comes to: +1 her left, -1 her right.
    side: f32,
    time: f32,
    nursing: bool,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let (mum, young) = (&cast[0], &cast[1]);
    let f = mum.facing();
    let left = Vec3::new(f.z, 0.0, -f.x);
    let side = if (young.pos - mum.pos).dot(left) >= 0.0 { 1.0 } else { -1.0 };
    Some(Box::new(Nurse { side, time: p.num("time", 12.0 + rng.unit() * 13.0, rng), nursing: false, t: 0.0 }))
}

impl Nurse {
    /// Where the young one stands: beside her, a little behind her middle,
    /// facing her tail, its muzzle under her belly by the hind legs.
    fn spot(&self, mum: &Member, young: &Member) -> Vec3 {
        let f = mum.facing();
        let left = Vec3::new(f.z, 0.0, -f.x);
        let udder = mum.pos - f * (mum.snout * 0.35);
        // Just clear of her side: any closer and the two bodies push each
        // other apart (creatures/crowd.rs) and she is shoved along.
        udder + f * young.snout * 0.8 + left * self.side * (mum.half_w + young.half_w + 0.1)
    }
}

impl Run for Nurse {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let (mum, young) = (&cast[0], &cast[1]);
        let spot = self.spot(mum, young);
        let toward_tail = mum.yaw + PI;
        if !self.nursing {
            if young.at(spot) {
                self.nursing = true;
                self.t = 0.0;
            } else if self.t > 15.0 {
                return Step::Done;
            }
            let outs = vec![
                Out { drive: Drive::Stay { face: None }, pose: Overlay { turn: 0.5 * self.side, ..Default::default() }, ..Default::default() },
                Out { drive: Drive::Go { to: spot, speed: young.slow * 1.3 }, ..Default::default() },
            ];
            return Step::On("coming to nurse".into(), outs);
        }
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        // Short butts up under her belly, and the tail going all the while.
        let butt = 0.15 * (t * 5.0).sin().max(0.0);
        let outs = vec![
            Out {
                drive: Drive::Stay { face: None },
                pose: Overlay { turn: 0.8 * self.side, head: -0.1, wag: 0.15 * (t * 1.3).sin(), ..Default::default() },
                ..Default::default()
            },
            Out {
                // Held in place: it shuffles to keep at her side if she
                // shifts.
                drive: if !young.at(spot) { Drive::Go { to: spot, speed: young.slow } } else { Drive::Stay { face: Some(toward_tail - 0.25 * self.side) } },
                pose: Overlay { head: 0.2 + butt, turn: 0.3 * self.side, tail: 0.3, wag: 0.6 * (t * 9.0).sin(), ..Default::default() },
                ..Default::default()
            },
        ];
        Step::On("nursing".into(), outs)
    }
}
