//! `call`: an animal stops, lifts its head and calls, a few times over, and
//! others of its kind near it join in, each a little after the last. Wolves
//! sit back and howl, a lion stands and roars with its jaw wide and its
//! flanks pumping, a bullfrog squats and pumps its throat. A display that will
//! want a sound when the game has one.
//!
//! Params: `calls` (how many each), `call_s` (seconds a call lasts),
//! `posture` ("stand", "sit" or "lie"), `pitch` (radians the head lifts),
//! `jaw` (radians the jaw opens), `pump` (radians the body pitches in time
//! with the call, a few beats a second).

use super::{Cast, Ctx, Drive, From, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::anim::Role;
use crate::world::noise::Rng;
use std::f32::consts::TAU;

pub const KIND: Kind = Kind {
    name: "call",
    rate: 6.0,
    holds: 0.8,
    cast,
    params: &["calls", "call_s", "pitch", "jaw", "pump"],
    words: &[("posture", &["stand", "sit", "lie"])],
    approach: false,
    roles: &[Role::Head],
    ready,
    start,
};

fn cast() -> Cast {
    Cast { from: From::Same, count: [1, 4], range: 30.0, members: Vec::new() }
}

fn ready(m: &Member) -> bool {
    m.walker_free()
}

struct Voice {
    /// Seconds before its first call.
    wait: f32,
    left: u32,
    /// Into this call or this breath, and how long it lasts.
    t: f32,
    len: f32,
    calling: bool,
}

struct Call {
    voices: Vec<Voice>,
    call_s: f32,
    act: Option<Act>,
    pitch: f32,
    jaw: f32,
    pump: f32,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, _ctx: &Ctx) -> Option<Box<dyn Run>> {
    let call_s = p.num("call_s", 3.0, rng).max(0.3);
    let voices = (0..cast.len())
        .map(|i| Voice {
            wait: 1.0 + i as f32 * (0.6 + rng.unit() * 1.4),
            left: p.count("calls", [2, 4], rng).max(1),
            t: 0.0,
            len: 0.0,
            calling: false,
        })
        .collect();
    let act = match p.word("posture", "stand") {
        "sit" => Some(Act::Sit),
        "lie" => Some(Act::Lie),
        _ => None,
    };
    Some(Box::new(Call {
        voices,
        call_s,
        act,
        pitch: p.num("pitch", 0.8, rng),
        jaw: p.num("jaw", 0.25, rng),
        pump: p.num("pump", 0.0, rng),
    }))
}

impl Run for Call {
    fn step(&mut self, _cast: &[Member], ctx: &Ctx, rng: &mut Rng) -> Step {
        let mut outs = Vec::with_capacity(self.voices.len());
        let mut any = false;
        let mut chorus = 0;
        for v in &mut self.voices {
            let base = Out { drive: Drive::Stay { face: None }, act: self.act, ..Default::default() };
            if v.wait > 0.0 {
                v.wait -= ctx.dt;
                any = true;
                outs.push(Out { pose: Overlay { head: 0.15, ..Default::default() }, ..base });
                continue;
            }
            v.t += ctx.dt;
            if v.t >= v.len {
                if v.calling {
                    v.left = v.left.saturating_sub(1);
                }
                v.calling = !v.calling && v.left > 0;
                v.t = 0.0;
                v.len = if v.calling { self.call_s * (0.8 + rng.unit() * 0.5) } else { 1.0 + rng.unit() * 2.0 };
            }
            if v.left == 0 {
                outs.push(Out { act: None, ..base });
                continue;
            }
            any = true;
            let (up, pump) = if v.calling {
                chorus += 1;
                let k = v.t / v.len;
                // Up into it fast, held, and down at the end.
                ((k / 0.15).min(1.0) * ((1.0 - k) / 0.15).min(1.0), (v.t * 3.0 * TAU).sin())
            } else {
                (0.0, 0.0)
            };
            let pose = Overlay { head: 0.15 + self.pitch * up, jaw: self.jaw * up, rear: self.pump * pump * up, tail: -0.2 * up, ..Default::default() };
            outs.push(Out { pose, ..base });
        }
        if !any {
            return Step::Done;
        }
        let phase = if chorus > 1 { format!("calling, {chorus} together") } else if chorus == 1 { "calling".into() } else { "between calls".into() };
        Step::On(phase, outs)
    }
}
