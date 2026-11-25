//! `threat`: the player comes too close and the animal, instead of running,
//! warns them off. Its `style` says how:
//!
//! - "side": turns broadside on and swings its tail (an ankylosaurus's
//!   club, a stegosaurus's spikes).
//! - "face": squares up to the player, head low, jaw open, arms spread and
//!   tail up (a raptor).
//! - "handstand": faces the player and stands on its fore legs with its
//!   rump and tail high (a spotted skunk).
//! - "curl": lies down and draws its head and legs in (a hedgehog).
//! - "claws": faces the player with its claws raised and open (a crab).
//!
//! It holds while the player stays within reach, and settles once they back
//! off or after `time`. Come right up to it and it runs after all (its
//! `holds` is low, not 0). Started by the player's approach, within `near`
//! metres (by default a little outside its flee distance).
//!
//! Params: `style`, `near`, `time` (most seconds it keeps it up), `swing`
//! (radians the tail swings, or the body rears in a handstand).

use super::{face, Cast, Ctx, Drive, Kind, Member, Out, Overlay, Params, Run, Step};
use crate::anim::life::Act;
use crate::world::noise::Rng;
use std::f32::consts::{FRAC_PI_2, TAU};

pub const KIND: Kind = Kind {
    name: "threat",
    rate: 0.0,
    holds: 0.3,
    cast: Cast::solo,
    params: &["near", "time", "swing"],
    words: &[("style", &["side", "face", "handstand", "curl", "claws"])],
    approach: true,
    roles: &[],
    ready,
    start,
};

fn ready(m: &Member) -> bool {
    m.walker_free()
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Side,
    Face,
    Handstand,
    Curl,
    Claws,
}

struct Threat {
    style: Style,
    near: f32,
    time: f32,
    swing: f32,
    /// Which side it turns to the player, for "side".
    flank: f32,
    t: f32,
    /// Seconds the player has been out of reach, and into settling.
    away: f32,
    settling: Option<f32>,
}

fn start(cast: &[Member], p: &Params, rng: &mut Rng, ctx: &Ctx) -> Option<Box<dyn Run>> {
    let m = &cast[0];
    let style = match p.word("style", "face") {
        "side" => Style::Side,
        "handstand" => Style::Handstand,
        "curl" => Style::Curl,
        "claws" => Style::Claws,
        _ => Style::Face,
    };
    // Broadside the way it is already more nearly turned.
    let to_player = face(m.pos, ctx.player);
    let rel = (m.yaw - to_player + TAU * 2.5) % TAU - std::f32::consts::PI;
    Some(Box::new(Threat {
        style,
        near: p.num("near", (m.wary * 1.3).max(6.0), rng),
        time: p.num("time", 25.0, rng),
        swing: p.num("swing", if style == Style::Handstand { 1.1 } else { 0.7 }, rng),
        flank: if rel >= 0.0 { 1.0 } else { -1.0 },
        t: 0.0,
        away: 0.0,
        settling: None,
    }))
}

impl Run for Threat {
    fn step(&mut self, cast: &[Member], ctx: &Ctx, _rng: &mut Rng) -> Step {
        self.t += ctx.dt;
        let m = &cast[0];
        let dist = (ctx.player - m.pos).with_y(0.0).length();
        if dist > self.near * 1.4 {
            self.away += ctx.dt;
        } else {
            self.away = 0.0;
        }
        if self.settling.is_none() && (self.away > 2.0 || self.t > self.time) {
            self.settling = Some(0.0);
        }
        // Easing in at the start and out at the end.
        let w = match &mut self.settling {
            Some(s) => {
                *s += ctx.dt;
                if *s > 1.5 {
                    return Step::Done;
                }
                1.0 - *s / 1.5
            }
            None => (self.t / 0.6).min(1.0),
        };
        let at_player = face(m.pos, ctx.player);
        let beat = (self.t * 1.3 * TAU).sin();
        let mut out = Out { drive: Drive::Stay { face: Some(at_player) }, ..Default::default() };
        out.pose = match self.style {
            Style::Side => {
                out.drive = Drive::Stay { face: Some(at_player + self.flank * FRAC_PI_2) };
                // Head turned to keep an eye on the player, tail swinging.
                Overlay { turn: self.flank * 0.6 * w, head: -0.15 * w, wag: self.swing * beat * w, tail: 0.15 * w, ..Default::default() }
            }
            Style::Face => Overlay { head: -0.35 * w, jaw: 0.5 * w, arms: 0.7 * w, raise: 0.3 * w, tail: 0.35 * w, wag: 0.1 * beat * w, ..Default::default() },
            Style::Handstand => Overlay { rear: -self.swing * w, plant: 1.0, head: self.swing * 0.7 * w, tail: 1.2 * w, ..Default::default() },
            Style::Curl => {
                out.act = (w > 0.5).then_some(Act::Lie);
                Overlay { head: -0.9 * w, tuck: w, ..Default::default() }
            }
            Style::Claws => Overlay { raise: 0.9 * w, arms: 0.5 * w + 0.1 * beat * w, ..Default::default() },
        };
        let label = if self.settling.is_some() { "settling" } else { "warning you off" };
        Step::On(label.into(), vec![out])
    }
}
