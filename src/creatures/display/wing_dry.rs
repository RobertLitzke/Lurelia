//! `wing_dry`: a cormorant or an anhinga after fishing comes to a post or a
//! rock and stands with its wings held open and still to dry, now and then
//! flapping them, turning its head to watch.
//!
//! Params: `time` (seconds), `on` (where it settles: "perch", "ground" or
//! "water").

use super::perch::{settle_at, Settle};
use super::{Cast, Ctx, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "wing_dry",
    rate: 6.0,
    holds: 0.6,
    cast: Cast::solo,
    params: &["time"],
    words: &[("on", &["perch", "ground", "water"])],
    approach: false,
    roles: &[Role::Wing],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true })
}

struct WingDry {
    settle: Settle,
    time: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let at = settle_at(m, p.word("on", "perch"), ctx, rng)?;
    let yaw = m.yaw + (rng.unit() - 0.5) * 2.0;
    Some(Box::new(WingDry { settle: Settle::new(m, at, yaw), time: p.num("time", 20.0 + rng.unit() * 20.0, rng), t: 0.0 }))
}

impl Run for WingDry {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        if let Some(out) = self.settle.flying(&cast[0], ctx.dt) {
            return Step::On("coming down".into(), vec![out]);
        }
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let open = (t / 1.0).min(1.0).min((self.time - t) / 1.0);
        // A few slow flaps every so often.
        let flap = if (t % 8.0) > 6.5 { 0.25 * (t * 6.0).sin() } else { 0.0 };
        let pose = Overlay { wings: (open * 0.95 + flap).clamp(0.0, 1.0), head: 0.2, turn: 0.6 * (t * 0.35).sin(), ..Default::default() };
        Step::On("drying its wings".into(), vec![Out { drive: self.settle.sit(), pose, ..Default::default() }])
    }
}
