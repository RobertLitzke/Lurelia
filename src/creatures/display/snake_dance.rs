//! `snake_dance`: two male snakes lying near each other raise their heads
//! and the front of their bodies high and sway together, necks twining,
//! each trying to push the other's head down, until one gives up.
//!
//! Snakes are posed from their chain (snake.rs): the display holds them
//! where they lie and lifts and sways the front of the body by the overlay.
//!
//! Params: `time` (seconds), `rise` (radians the front lifts).

use super::{Cast, Ctx, Drive, From, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::library::MemberDef;
use crate::world::noise::Rng;

pub const KIND: Kind = Kind {
    name: "snake_dance",
    rate: 6.0,
    holds: 0.4,
    cast,
    params: &["time", "rise"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    let male = MemberDef { age: Some("adult".into()), sex: Some("male".into()) };
    Cast { from: From::Same, count: [2, 2], range: 6.0, members: vec![male] }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Serpent { resting: true }
}

struct SnakeDance {
    time: f32,
    rise: f32,
    t: f32,
}

fn start(_: &[Member], p: &Params, rng: &mut Rng, _: &Ctx) -> Option<Box<dyn Run>> {
    Some(Box::new(SnakeDance { time: p.num("time", 15.0 + rng.unit() * 15.0, rng), rise: p.num("rise", 1.0, rng), t: 0.0 }))
}

impl Run for SnakeDance {
    fn step(&mut self, _: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        if self.t > self.time {
            return Step::Done;
        }
        let t = self.t;
        let up = (t / 3.0).min(1.0).min((self.time - t) / 3.0).max(0.0);
        // Swaying together, out of step a little, and pressing down in turns.
        let outs = (0..2)
            .map(|i| {
                let s = if i == 0 { 1.0 } else { -1.0 };
                let press = (t * 0.6 + i as f32 * 1.5).sin().max(0.0);
                let pose = Overlay { head: self.rise * up * (1.0 - 0.3 * press), turn: s * 0.5 * up * (t * 1.3 + i as f32 * 0.6).sin(), ..Default::default() };
                Out { drive: Drive::Stay { face: None }, pose, ..Default::default() }
            })
            .collect();
        Step::On("rising and twining".into(), outs)
    }
}
