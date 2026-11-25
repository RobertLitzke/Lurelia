//! `tumble`: two crows or ravens at play in the air. They climb together
//! over where they met and chase round a loose circle, one tight behind the
//! other, and every few seconds one flips over onto its back and rolls,
//! dropping a few metres before it rights itself. Now and then the two close
//! up and roll together. Then each flies back up to its own circle
//! (circle.rs takes it back).
//!
//! Params: `time` (seconds of play), `climb` (metres over the ground it plays
//! at, at least), `every` (seconds between rolls), `drop` (metres a roll
//! loses).

use super::{heading, Cast, Ctx, Drive, From, Hop, Kind, Member, Mover, Out, Overlay, Params, Run, Step};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

pub const KIND: Kind = Kind {
    name: "tumble",
    rate: 8.0,
    // High over the trees it pays the player no mind.
    holds: 0.0,
    cast,
    params: &["time", "climb", "every", "drop"],
    words: &[],
    approach: false,
    roles: &[],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [2, 2], range: 60.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.mover == Mover::Circler { free: true }
}

/// One roll under way: how far through, how long it takes, how many turns
/// over and which way.
#[derive(Clone, Copy)]
struct Roll {
    u: f32,
    dur: f32,
    turns: f32,
    side: f32,
}

struct Tumble {
    centre: Vec3,
    /// Height of the play.
    y: f32,
    r: f32,
    speed: f32,
    /// The leader's angle round the centre, and which way round.
    ang: f32,
    dir: f32,
    /// How far round behind the leader the follower is, radians.
    lag: f32,
    left: f32,
    every: f32,
    next: f32,
    drop: f32,
    rolls: [Option<Roll>; 2],
    /// Both rolling together.
    locked: bool,
    join: Option<[Hop; 2]>,
}

/// The follower's gap behind the leader, radians, and when locked.
const LAG: f32 = 0.35;
const LOCK_LAG: f32 = 0.05;

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let mid = (cast[0].pos + cast[1].pos) * 0.5;
    let ground = ctx.terrain.height(mid.x, mid.z).unwrap_or(mid.y - 10.0).max(ctx.terrain.sea);
    let y = mid.y.max(ground + p.num("climb", 12.0, rng));
    let speed = cast[0].fast.max(cast[0].slow).clamp(6.0, 12.0);
    let mut t = Tumble {
        centre: Vec3::new(mid.x, y, mid.z),
        y,
        r: 9.0 + rng.unit() * 4.0,
        speed,
        ang: rng.unit() * TAU,
        dir: if rng.unit() < 0.5 { 1.0 } else { -1.0 },
        lag: LAG,
        left: p.num("time", 15.0 + rng.unit() * 15.0, rng),
        every: p.num("every", 2.5, rng).max(0.8),
        next: 2.0,
        drop: p.num("drop", 3.0, rng),
        rolls: [None; 2],
        locked: false,
        join: None,
    };
    let weave = (t.ang * 2.3).sin() * 1.2;
    t.join = Some([Hop::new(cast[0].pos, t.slot(0, weave), speed), Hop::new(cast[1].pos, t.slot(1, weave), speed)]);
    Some(Box::new(t))
}

impl Tumble {
    /// Where member `k` flies, before any roll: the leader on a loose
    /// circle, the follower behind it and weaving above and below.
    fn slot(&self, k: usize, weave: f32) -> Vec3 {
        let a = self.ang - self.dir * self.lag * k as f32;
        let r = self.r * (1.0 + 0.2 * (a * 1.3).sin());
        let bob = (a * 0.8).sin() * 1.5 + if k == 1 { weave } else { 0.0 };
        self.centre + Vec3::new(a.cos() * r, self.y - self.centre.y + bob, a.sin() * r)
    }
}

impl Run for Tumble {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let dt = ctx.dt;
        if let Some(hops) = &mut self.join {
            let mut outs = Vec::new();
            let mut all = true;
            for (k, h) in hops.iter_mut().enumerate() {
                let (pos, done) = h.step(dt);
                all &= done;
                outs.push(Out { drive: Drive::Place { pos, yaw: heading(cast[k].pos, pos, cast[k].yaw) }, ..Default::default() });
            }
            if all {
                self.join = None;
            }
            return Step::On("joining".into(), outs);
        }
        self.left -= dt;
        if self.left <= 0.0 && self.rolls.iter().all(Option::is_none) {
            return Step::Done;
        }
        self.ang += self.dir * self.speed * dt / self.r;
        // Now and then a roll: one of them, or the two together.
        self.next -= dt;
        if self.next <= 0.0 && self.left > 2.0 && self.rolls.iter().all(Option::is_none) {
            self.next = self.every * (0.6 + rng.unit() * 0.8);
            let side = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
            if rng.unit() < 0.3 {
                self.locked = true;
                self.rolls = [Some(Roll { u: 0.0, dur: 1.6, turns: 2.0, side }), Some(Roll { u: 0.0, dur: 1.6, turns: 2.0, side: -side })];
            } else {
                let k = (rng.unit() < 0.5) as usize;
                let turns = if rng.unit() < 0.4 { 2.0 } else { 1.0 };
                self.rolls[k] = Some(Roll { u: 0.0, dur: 0.6 + 0.4 * turns, turns, side });
            }
        }
        let want = if self.locked { LOCK_LAG } else { LAG };
        self.lag += (want - self.lag) * (dt * 3.0).min(1.0);
        let weave = (self.ang * 2.3).sin() * 1.2;
        let mut outs = Vec::new();
        for k in 0..2 {
            let mut pos = self.slot(k, weave);
            let mut pose = Overlay::default();
            if let Some(r) = &mut self.rolls[k] {
                r.u = (r.u + dt / r.dur).min(1.0);
                // Over onto its back and round, falling, and level again.
                let e = r.u * r.u * (3.0 - 2.0 * r.u);
                pose.roll = r.side * TAU * r.turns * e;
                pose.wings = 0.4 * (PI * r.u).sin();
                let mult = if self.locked { 2.0 } else { 1.0 };
                pos.y -= self.drop * mult * (PI * r.u).sin();
                if r.u >= 1.0 {
                    self.rolls[k] = None;
                }
            }
            let floor = ctx.terrain.height(pos.x, pos.z).unwrap_or(pos.y - 3.0) + 3.0;
            pos.y = pos.y.max(floor);
            outs.push(Out { drive: Drive::Place { pos, yaw: heading(cast[k].pos, pos, cast[k].yaw) }, pose, ..Default::default() });
        }
        if self.rolls.iter().all(Option::is_none) {
            self.locked = false;
        }
        let phase = match (self.locked, self.rolls.iter().any(Option::is_some)) {
            (true, _) => "rolling together",
            (_, true) => "rolling",
            _ => "chasing",
        };
        Step::On(phase.into(), outs)
    }
}
