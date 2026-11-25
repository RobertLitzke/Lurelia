//! `broken_wing`: a plover's distraction. When the player comes near it
//! runs a little way off, then flops along the ground in front of them, one
//! wing dragging and held out as if broken, tail fanned, calling, always
//! staying a few metres ahead and leading them away; once they have
//! followed far enough it gets up and flies off sound.
//!
//! Started by the player coming near (`on_approach`).
//!
//! Params: `near` (metres the player comes within), `lead` (metres it
//! leads them before it gives up), `ahead` (metres it keeps in front).

use super::{face, Cast, Ctx, Drive, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;

pub const KIND: Kind = Kind {
    name: "broken_wing",
    rate: 0.0,
    holds: 0.0,
    cast: Cast::solo,
    params: &["near", "lead", "ahead"],
    words: &[],
    approach: true,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Runner { grounded: true, .. })
}

struct BrokenWing {
    from: Vec3,
    pos: Vec3,
    dir: Vec3,
    lead: f32,
    ahead: f32,
    side: f32,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    // Away from the player, and a little to one side, as off a nest.
    let away = (m.pos - ctx.player).with_y(0.0).normalize_or(m.facing());
    let a = (rng.unit() - 0.5) * 1.2;
    let dir = Vec3::new(away.x * a.cos() - away.z * a.sin(), 0.0, away.x * a.sin() + away.z * a.cos());
    Some(Box::new(BrokenWing { from: m.pos, pos: m.pos, dir, lead: p.num("lead", 30.0, rng), ahead: p.num("ahead", 6.0, rng), side: if rng.unit() < 0.5 { 1.0 } else { -1.0 }, t: 0.0 }))
}

impl Run for BrokenWing {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        let m = &cast[0];
        self.t += ctx.dt;
        let led = (self.pos - self.from).with_y(0.0).length();
        if led > self.lead || self.t > 60.0 {
            return Step::Done;
        }
        // Keep `ahead` in front of the player: flop on when they close in,
        // wait (still flopping) when they hang back.
        let gap = (self.pos - ctx.player).with_y(0.0).length();
        let speed = if gap < self.ahead { m.slow.max(1.0) * 1.2 } else if gap < self.ahead * 1.8 { m.slow.max(1.0) * 0.4 } else { 0.0 };
        let next = self.pos + self.dir * speed * ctx.dt;
        if let Some(g) = ctx.terrain.drawn(next.x, next.z, next.y + 1.0).filter(|g| (g - next.y).abs() < 0.6 && !ctx.terrain.wet(next.x, next.z)) {
            self.pos = Vec3::new(next.x, g, next.z);
        } else {
            // Blocked: veer off to the side and go on.
            self.dir = Vec3::new(-self.dir.z, 0.0, self.dir.x) * self.side;
        }
        let t = self.t;
        let flop = (t * 7.0).sin();
        let pose = Overlay { wings: 0.6 + 0.3 * flop.abs(), roll: self.side * (0.35 + 0.1 * flop), head: -0.2, tail: 0.6, jaw: 0.3 * (t * 5.0).sin().max(0.0), rear: -0.1, ..Default::default() };
        // Faced along the way it goes, or round at the player when it waits.
        let yaw = if speed > 0.0 { face(self.pos, self.pos + self.dir) } else { face(self.pos, ctx.player) };
        Step::On("dragging a wing".into(), vec![Out { drive: Drive::Sit { pos: self.pos, yaw }, pose, ..Default::default() }])
    }
}
