//! `cache`: a crow, raven or jay hides food. It comes down to a patch of
//! ground a little way off, jabs a hole with its bill, pushes something into
//! it, sweeps the litter back over with sideways flicks of its head, and
//! looks round to see who was watching before it flies off. An acorn
//! woodpecker on its granary trunk hammers an acorn into a hole in the bark
//! instead, and doesn't cover it. Its entry's `food` (an acorn) is in its bill
//! from the flight down until it is tucked in.
//!
//! Params: `dig` (seconds digging), `cover` (seconds covering), `range`
//! (metres away it comes down, at most).

use super::{heading, spot, Cast, Ctx, Drive, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "cache",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["dig", "cover", "range"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true } | Mover::Flitter { perched: true } | Mover::Clinger { clinging: true })
}

enum Phase {
    Fly(Hop),
    /// A moment on the ground before it starts.
    Land,
    Dig,
    /// Pushing it in.
    Tuck,
    Cover,
    LookRound,
}

struct Cache {
    at: Vec3,
    yaw: f32,
    phase: Phase,
    t: f32,
    dig: f32,
    cover: f32,
    /// Where it looks while looking round, radians left of ahead.
    look: f32,
    to: f32,
    /// Into bark on a trunk rather than the ground.
    bark: bool,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    if m.mover == (Mover::Clinger { clinging: true }) {
        let dig = p.num("dig", 2.0 + rng.unit() * 2.0, rng);
        return Some(Box::new(Cache { at: m.pos, yaw: m.yaw, phase: Phase::Land, t: 0.0, dig, cover: 0.0, look: 0.0, to: 0.8, bark: true }));
    }
    let flying = matches!(m.mover, Mover::Circler { .. });
    let (lo, hi) = if flying { (8.0, 25.0) } else { (2.0, 8.0) };
    let hi = p.num("range", hi, rng).max(lo + 1.0);
    let at = spot(ctx.terrain, m.pos, lo, hi, Some(m.facing()), false, rng)?;
    let speed = if flying { m.slow.max(4.0) } else { m.slow.max(2.0) };
    Some(Box::new(Cache {
        at,
        yaw: heading(m.pos, at, m.yaw),
        phase: Phase::Fly(Hop::new(m.pos, at, speed)),
        t: 0.0,
        dig: p.num("dig", 2.0 + rng.unit() * 2.0, rng),
        cover: p.num("cover", 1.5 + rng.unit(), rng),
        look: 0.0,
        to: 0.8,
        bark: false,
    }))
}

impl Run for Cache {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        let dt = ctx.dt;
        self.t += dt;
        let t = self.t;
        let sit = Drive::Sit { pos: self.at, yaw: self.yaw };
        let mut pose = Overlay::default();
        let held = matches!(self.phase, Phase::Land | Phase::Dig) || matches!(self.phase, Phase::Tuck) && t < 0.5;
        let phase = match &mut self.phase {
            Phase::Fly(h) => {
                let (pos, done) = h.step(dt);
                if done {
                    self.phase = Phase::Land;
                    self.t = 0.0;
                }
                let yaw = heading(m.pos, pos, m.yaw);
                return Step::On("flying down".into(), vec![Out { drive: Drive::Place { pos, yaw }, held: true, ..Default::default() }]);
            }
            Phase::Land => {
                if t > 0.6 {
                    self.phase = Phase::Dig;
                    self.t = 0.0;
                }
                "landed"
            }
            Phase::Dig => {
                // Quick jabs, bill to the ground (or to the bark ahead).
                let jab = (t * 11.0).sin().abs();
                pose.head = if self.bark { 0.1 - 0.4 * jab } else { -0.7 - 0.35 * jab };
                if t > self.dig {
                    self.phase = Phase::Tuck;
                    self.t = 0.0;
                }
                "digging"
            }
            Phase::Tuck => {
                pose.head = if self.bark { -0.4 } else { -1.05 };
                pose.rear = -0.15 * (t * 3.0).sin().max(0.0);
                if t > 0.9 {
                    self.phase = if self.bark { Phase::LookRound } else { Phase::Cover };
                    self.t = 0.0;
                }
                "tucking it in"
            }
            Phase::Cover => {
                // Sideways flicks, drawing the litter back over.
                pose.head = -0.75;
                pose.turn = 0.5 * (t * 7.0).sin();
                if t > self.cover {
                    self.phase = Phase::LookRound;
                    self.t = 0.0;
                }
                "covering"
            }
            Phase::LookRound => {
                if (self.look - self.to).abs() < 0.05 {
                    self.to = (rng.unit() - 0.5) * 2.4;
                }
                self.look += (self.to - self.look).clamp(-4.0 * dt, 4.0 * dt);
                pose.turn = self.look;
                pose.head = 0.15;
                if t > 2.0 {
                    return Step::Done;
                }
                "looking round"
            }
        };
        Step::On(phase.into(), vec![Out { drive: sit, pose, held, ..Default::default() }])
    }
}
