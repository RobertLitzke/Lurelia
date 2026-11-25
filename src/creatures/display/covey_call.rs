//! `covey_call`: one of a covey gets up on a high point (a rock, a bush
//! top) and calls over and over, head up; the others come in to the foot
//! of it from wherever they were feeding, and it drops down among them.
//! Quail on foot; ptarmigan fly in.
//!
//! Params: `calls` (how many), `call_s` (seconds between them), `reach`
//! (metres it looks for a high point).

use super::{face, heading, spot, Cast, Ctx, Drive, From, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "covey_call",
    rate: 6.0,
    holds: 0.5,
    cast,
    params: &["calls", "call_s", "reach"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

use crate::anim::Role;

fn cast() -> Cast {
    Cast { from: From::Same, count: [3, 20], range: 40.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Runner { grounded: true, .. } | Mover::Flocker { perched: true, soars: false })
}

struct CoveyCall {
    post: Vec3,
    up: Hop,
    calls: u32,
    call_s: f32,
    /// Where each of the rest comes to, round the foot of the post, and
    /// how it gets there.
    marks: Vec<Vec3>,
    hops: Vec<Hop>,
    t: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let caller = &cast[0];
    let post = spot(ctx.terrain, caller.pos, 1.0, p.num("reach", 8.0, rng), None, true, rng)?;
    let foot = Vec3::new(post.x, ctx.terrain.height(post.x, post.z).unwrap_or(post.y), post.z);
    let n = cast.len().max(2) - 1;
    let marks: Vec<Vec3> = (0..cast.len())
        .map(|i| {
            let a = i as f32 / n as f32 * TAU + rng.unit() * 0.5;
            let r = 1.0 + rng.unit() * 1.5;
            let q = foot + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            Vec3::new(q.x, ctx.terrain.drawn(q.x, q.z, q.y + 2.0).unwrap_or(q.y), q.z)
        })
        .collect();
    // The rest come in slowly, on foot or low over the ground.
    let hops = cast.iter().zip(&marks).map(|(m, &to)| Hop::new(m.pos, to, m.slow.max(0.8))).collect();
    Some(Box::new(CoveyCall {
        post,
        up: Hop::new(caller.pos, post, 2.0),
        calls: p.count("calls", [5, 10], rng).max(1),
        call_s: p.num("call_s", 1.6, rng).max(0.5),
        marks,
        hops,
        t: 0.0,
    }))
}

impl Run for CoveyCall {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _: &mut Rng) -> Step {
        self.t += ctx.dt;
        let len = self.calls as f32 * self.call_s;
        if self.t > len + 2.0 {
            return Step::Done;
        }
        let mut outs = Vec::with_capacity(cast.len());
        // The caller: up onto the post, calling, then down to the others.
        let (pos, up_done) = self.up.step(ctx.dt);
        let caller = &cast[0];
        let out = if !up_done {
            Out { drive: Drive::Place { pos, yaw: heading(caller.pos, pos, caller.yaw) }, ..Default::default() }
        } else if self.t > len {
            let to = self.marks[0];
            let u = ((self.t - len) / 1.0).min(1.0);
            let at = self.post.lerp(to, u) + Vec3::Y * 0.4 * (PI * u).sin();
            Out { drive: Drive::Sit { pos: at, yaw: caller.yaw }, ..Default::default() }
        } else {
            let c = (self.t / self.call_s).fract();
            let call = (PI * (c / 0.4).min(1.0)).sin() * (c < 0.4) as u32 as f32;
            Out { drive: Drive::Sit { pos: self.post, yaw: caller.yaw + 0.3 * (self.t * 0.4).sin() }, pose: Overlay { head: 0.5 * call + 0.2, jaw: 0.4 * call, tail: 0.2, ..Default::default() }, ..Default::default() }
        };
        outs.push(out);
        // The others: a moment's pause, heads up, then in to the foot of it.
        for i in 1..cast.len() {
            let m = &cast[i];
            let out = if self.t < 1.0 + i as f32 * 0.3 {
                Out { drive: Drive::Sit { pos: m.pos, yaw: face(m.pos, self.post) }, pose: Overlay { head: 0.4, ..Default::default() }, ..Default::default() }
            } else {
                let (pos, done) = self.hops[i].step(ctx.dt);
                let pos = if done { self.marks[i] } else { Vec3::new(pos.x, ctx.terrain.drawn(pos.x, pos.z, pos.y + 2.0).unwrap_or(pos.y).max(pos.y - 0.3), pos.z) };
                Out { drive: Drive::Sit { pos, yaw: if done { face(pos, self.post) } else { heading(m.pos, pos, m.yaw) } }, ..Default::default() }
            };
            outs.push(out);
        }
        Step::On("calling the covey in".into(), outs)
    }
}
