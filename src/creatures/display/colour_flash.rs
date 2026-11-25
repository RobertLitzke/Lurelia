//! `colour_flash`: an octopus ripples pale and dark in quick waves, arms
//! spread and raised; a lobster instead rears its front up and holds its
//! claws high and open (`style` "claws").
//!
//! Params: `time` (seconds), `style` ("ripple" or "claws").

use super::{face, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "colour_flash",
    rate: 8.0,
    holds: 0.4,
    cast: Cast::solo,
    params: &["time"],
    words: &[("style", &["ripple", "claws"])],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.mover == Mover::Creeper
}

struct ColourFlash {
    claws: bool,
    time: f32,
    yaw: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let claws = p.word("style", "ripple") == "claws";
    let yaw = if claws && (ctx.player - m.pos).length() < 15.0 { face(m.pos, ctx.player) } else { m.yaw };
    Some(Box::new(ColourFlash { claws, time: p.num("time", 6.0 + rng.unit() * 6.0, rng), yaw, t: 0.0 }))
}

impl Run for ColourFlash {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let ease = (t / 0.8).min(1.0).min((self.time - t) / 0.8).max(0.0);
        let out = if self.claws {
            Out { drive: Drive::Stay { face: Some(self.yaw) }, pose: Overlay { raise: 0.8 * ease, arms: 0.5 * ease, rear: 0.25 * ease, ..Default::default() }, ..Default::default() }
        } else {
            // Waves of pale, a couple a second, the body swelling with them.
            let wave = (t * 7.0).sin().max(0.0);
            Out { drive: Drive::Stay { face: None }, pose: Overlay { arms: 0.4 * ease, raise: 0.2 * ease * wave, ..Default::default() }, pale: 0.55 * ease * wave, ..Default::default() }
        };
        Step::On(if self.claws { "claws up" } else { "flashing pale and dark" }.into(), vec![out])
    }
}
