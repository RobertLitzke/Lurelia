//! `chase`: two squirrels tear round after each other in tight spirals,
//! the one behind never quite catching up, switching direction, and
//! finishing at the foot of a tree (walkers can't climb, so they stop
//! there).
//!
//! Params: `time` (seconds), `radius` (metres round the spiral).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "chase",
    rate: 6.0,
    holds: 0.6,
    cast,
    params: &["time", "radius"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 20.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Chase {
    centre: Vec3,
    radius: f32,
    ang: f32,
    dir: f32,
    time: f32,
    flip: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let mid = (cast[0].pos + cast[1].pos) * 0.5;
    // Round a trunk if there is one near.
    let centre = ctx.flora.near(mid.x, mid.z, 12.0, None).find(|r| r.trunk > 0.05).map_or(mid, |r| r.pos);
    let d = cast[0].pos - centre;
    Some(Box::new(Chase {
        centre,
        radius: p.num("radius", 2.5, rng),
        ang: d.z.atan2(d.x),
        dir: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        time: p.num("time", 8.0 + rng.unit() * 8.0, rng),
        flip: 2.0 + rng.unit() * 2.0,
        t: 0.0,
    }))
}

impl Run for Chase {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        self.flip -= ctx.dt;
        if self.flip <= 0.0 {
            self.dir = -self.dir;
            self.flip = 1.5 + rng.unit() * 2.5;
        }
        let speed = cast[0].fast.max(cast[0].slow);
        self.ang += self.dir * speed * 0.8 * ctx.dt / self.radius;
        // The radius breathes in and out, so the spiral tightens and opens.
        let r = self.radius * (0.7 + 0.3 * (self.t * 0.9).sin());
        let at = |a: f32| self.centre + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let lead = at(self.ang + self.dir * 0.9);
        let outs = vec![
            Out { drive: Drive::Go { to: lead, speed }, pose: Overlay { tail: 0.7, ..Default::default() }, ..Default::default() },
            Out { drive: Drive::Go { to: cast[0].pos, speed }, pose: Overlay { tail: 0.7, ..Default::default() }, ..Default::default() },
        ];
        Step::On("chasing round".into(), outs)
    }
}
