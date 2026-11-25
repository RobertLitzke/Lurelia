//! `bob`: a bird on a perch, bobbing. An owl comes down to a post or a
//! tree top and works its head: swivels it far round and holds, then sways
//! and bobs it to judge how far off something is. A dipper on its rock dips
//! its whole body up and down. Then it goes back to what it was doing.
//!
//! A circler (an owl) first flies to a raised perch near by; a flitting bird
//! (a dipper) bobs where it sits.
//!
//! Params: `time` (seconds on the perch), `bob` (metres the body dips),
//! `bob_s` (seconds a dip takes), `swivel` (radians the head turns, at
//! most), `sway` (radians of the side-to-side head bob).

use super::{face, heading, spot, Cast, Ctx, Drive, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::anim::Role;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::PI;

pub const KIND: Kind = Kind {
    name: "bob",
    rate: 6.0,
    holds: 0.5,
    cast: Cast::solo,
    params: &["time", "bob", "bob_s", "swivel", "sway"],
    words: &[],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    matches!(m.mover, Mover::Circler { free: true } | Mover::Flitter { perched: true })
}

/// What it does for a few seconds on the perch.
#[derive(Clone, Copy)]
enum Bout {
    /// Body dips, this many.
    Dip(u32),
    /// Head turned to an angle and held.
    Swivel(f32),
    /// Head swayed and bobbed, judging distance.
    Sway,
}

struct Bob {
    at: Vec3,
    yaw: f32,
    fly: Option<Hop>,
    left: f32,
    bout: Bout,
    t: f32,
    len: f32,
    bob: f32,
    bob_s: f32,
    swivel: f32,
    sway: f32,
    /// The head turn it holds between bouts, eased toward.
    turn: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let (at, fly) = match m.mover {
        Mover::Circler { .. } => {
            let at = spot(ctx.terrain, m.pos, 6.0, 30.0, Some(m.facing()), true, rng)?;
            (at, Some(Hop::new(m.pos, at, m.slow.max(4.0))))
        }
        _ => (m.pos, None),
    };
    // Settled facing about the player's way: owls watch.
    let yaw = face(at, ctx.player) + (rng.unit() - 0.5) * 1.2;
    Some(Box::new(Bob {
        at,
        yaw,
        fly,
        left: p.num("time", 12.0 + rng.unit() * 14.0, rng),
        bout: Bout::Swivel(0.0),
        t: 0.0,
        len: 1.0,
        bob: p.num("bob", 0.03, rng),
        bob_s: p.num("bob_s", 0.6, rng).max(0.15),
        swivel: p.num("swivel", 1.2, rng),
        sway: p.num("sway", 0.25, rng),
        turn: 0.0,
    }))
}

impl Bob {
    fn next_bout(&mut self, rng: &mut Rng) {
        self.t = 0.0;
        let r = rng.unit();
        // A bird that barely dips (an owl) swivels and sways instead; one
        // that turns its head little (a dipper) mostly dips.
        let dips = if self.bob >= 0.02 { 0.5 } else { 0.0 };
        let sways = if self.sway > 0.05 { 0.25 } else { 0.0 };
        self.bout = if r < dips {
            let n = 2 + (rng.unit() * 5.0) as u32;
            self.len = n as f32 * self.bob_s;
            Bout::Dip(n)
        } else if r < dips + sways {
            self.len = 1.5 + rng.unit() * 1.5;
            Bout::Sway
        } else {
            self.len = 1.0 + rng.unit() * 2.0;
            Bout::Swivel((rng.unit() * 2.0 - 1.0) * self.swivel)
        };
    }
}

impl Run for Bob {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let m = &cast[0];
        let dt = ctx.dt;
        if let Some(h) = &mut self.fly {
            let (pos, done) = h.step(dt);
            if done {
                self.fly = None;
            }
            let yaw = heading(m.pos, pos, m.yaw);
            return Step::On("flying to a perch".into(), vec![Out { drive: Drive::Place { pos, yaw }, ..Default::default() }]);
        }
        self.left -= dt;
        if self.left <= 0.0 {
            return Step::Done;
        }
        self.t += dt;
        if self.t > self.len {
            self.next_bout(rng);
        }
        let t = self.t;
        let mut pos = self.at;
        let mut pose = Overlay::default();
        let (phase, want) = match self.bout {
            Bout::Dip(n) => {
                // Down and up, a little forward lean at the bottom.
                let d = ((t / self.bob_s) * PI).sin().abs();
                pos.y -= self.bob * d;
                pose.rear = -0.12 * d;
                (format!("dipping, {n}"), self.turn)
            }
            Bout::Swivel(a) => ("head round".to_string(), a),
            Bout::Sway => {
                pose.head = self.sway * 0.6 * (t * 7.0).sin();
                pose.rear = 0.0;
                ("swaying its head".to_string(), self.turn + self.sway * (t * 3.5).sin())
            }
        };
        // Owls snap their heads round, then hold.
        self.turn += (want - self.turn).clamp(-6.0 * dt, 6.0 * dt);
        pose.turn = self.turn;
        Step::On(phase, vec![Out { drive: Drive::Sit { pos, yaw: self.yaw }, pose, ..Default::default() }])
    }
}
