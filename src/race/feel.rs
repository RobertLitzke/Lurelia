//! What makes a kart race feel like one, items or none: the
//! slipstream behind another kart, the rocket start, and the countdown
//! lights on the gantry.

use super::fight::kart;
use super::{Phase, Race};
use crate::ride::Ride;
use crate::vehicle::Vehicle;
use glam::{Quat, Vec2, Vec3};

/// The slipstream: within this many metres behind another kart...
const DRAFT_BACK: f32 = 10.0;
/// ...and this close to its line...
const DRAFT_SIDE: f32 = 2.0;
/// ...for this long, a kart may run this share over its top speed, pushed
/// toward it this hard (m/s²).
const DRAFT_AFTER: f32 = 1.0;
const DRAFT_TOP: f32 = 1.06;
const DRAFT_PUSH: f32 = 3.0;

/// The rocket start: the throttle held from when "1" shows gives this much
/// at GO, m/s; held from before "2" the engine bogs for this long.
pub(super) const ROCKET_KICK: f32 = 4.0;
pub(super) const BOG_SECS: f32 = 0.5;

impl Race {
    /// Who is in whose slipstream this step, and the push it gives.
    pub(super) fn slipstream(&mut self, ride: &mut Ride, dt: f32) {
        let n = self.bots.len() + 1;
        self.draft.resize(n, 0.0);
        let at = |r: &Race, k: usize| {
            let l = r.laps_of(k);
            (l.progress, l.place.map_or(0.0, |p| p.off), l.finish.is_some())
        };
        for k in 0..n {
            let (p, off, done) = at(self, k);
            let behind = !done
                && (0..n).any(|j| {
                    let (q, o, _) = at(self, j);
                    j != k && (1.5..DRAFT_BACK).contains(&(q - p)) && (o - off).abs() < DRAFT_SIDE
                });
            self.draft[k] = if behind { self.draft[k] + dt } else { 0.0 };
            if self.draft[k] < DRAFT_AFTER {
                continue;
            }
            if let Some(car) = kart(&mut self.bots, ride, k) {
                let top = car.spec.top_speed * DRAFT_TOP;
                let fwd = car.forward();
                let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
                if car.grounded > 0 && car.b.vel.dot(flat) < top && car.b.vel.dot(flat) > car.spec.top_speed * 0.7 {
                    car.b.vel += flat * DRAFT_PUSH * dt;
                }
            }
        }
    }

    /// Whether the player is in a slipstream now, for the HUD.
    pub(super) fn drafting(&self) -> bool {
        self.draft.first().is_some_and(|&d| d >= DRAFT_AFTER)
    }
}

impl Race {
    /// At GO: a kick for the player who held the throttle from "1", a
    /// bogged engine for one who held it from before "2", and a rocket start
    /// for about half the rivals.
    pub(super) fn rocket_start(&mut self, ride: &mut Ride) {
        if let Some(from) = self.gas_from.take() {
            if from > 2.0 {
                self.bog = BOG_SECS;
                ride.hold = true;
                self.note = Some("BOGGED DOWN".into());
            } else if from <= 1.0 {
                if let Some(car) = ride.car_mut() {
                    rocket_kick(car);
                }
                self.note = Some("ROCKET START".into());
            }
        }
        let n = self.bots.len();
        for i in 0..n {
            if (i + self.laps as usize) % 2 == 0 {
                rocket_kick(&mut self.bots[i].car);
            }
        }
    }
}

/// The rocket start's kick, and half a second at the boosted top speed.
fn rocket_kick(car: &mut Vehicle) {
    let fwd = car.forward();
    car.b.vel += fwd * ROCKET_KICK;
    car.kick_t = 0.5;
}

/// The countdown lights: this many lamps across the middle of the gantry's
/// beam, on its face toward the grid, this far apart. Each is a square of
/// the shooter's lamp bars (`avatar::LAMP_M`, 0.22 m long), `LAMP_BARS` of
/// them stacked at `LAMP_SCALE` times their size: 0.9 m across.
const LAMPS: usize = 4;
const LAMP_GAP: f32 = 1.2;
const LAMP_BARS: usize = 4;
const LAMP_SCALE: f32 = 4.0;
/// Seconds the lamps stay green after GO.
const GREEN_FOR: f32 = 3.0;
const RED: [f32; 3] = [0.9, 0.0, 0.0];
const GREEN: [f32; 3] = [0.15, 1.0, 0.2];
const UNLIT: [f32; 3] = [0.05, 0.05, 0.06];

impl Race {
    /// Each countdown lamp: its middle, the way across the road, and its
    /// colour if lit. The first three go red at 3, 2 and 1, and at GO all
    /// four go green for a few seconds. None on a course without a gantry.
    fn gantry_lamps(&self) -> Vec<(Vec3, Vec2, Option<[f32; 3]>)> {
        let Some(clear) = self.gate else { return Vec::new() };
        let (reds, green) = match self.phase {
            // `left` is the number the HUD shows: 3, 2, 1.
            Phase::Grid(t) => (4usize.saturating_sub(t.max(0.0).ceil() as usize).min(3), false),
            Phase::Racing | Phase::Done(_) => (0, self.clock < GREEN_FOR),
        };
        let (p, d) = self.track.at(0.0);
        let road = self.track.height_at(0.0).unwrap_or(0.0);
        let side = Vec2::new(-d.y, d.x);
        // The middle of the beam's metre of depth, just off its face toward
        // the grid (the beam is half a metre thick on the line).
        let y = road + clear + 0.5;
        (0..LAMPS)
            .map(|i| {
                let q = p + side * ((i as f32 - (LAMPS - 1) as f32 * 0.5) * LAMP_GAP) - d * 0.37;
                let lit = if green {
                    Some(GREEN)
                } else if i < reds {
                    Some(RED)
                } else {
                    None
                };
                (Vec3::new(q.x, y, q.y), side, lit)
            })
            .collect()
    }

    /// The lamps drawn: lit ones glowing, the rest dark.
    pub(super) fn gantry_instances(&self, weapons: usize, models: &[crate::models::Model], out: &mut Vec<(usize, crate::models::Instance)>) {
        use crate::brain::avatar::{LAMP_M, LAMP_VOX};
        let lamp = weapons + crate::combat::viewmodel::COUNT + LAMP_M;
        let bar = LAMP_VOX * LAMP_SCALE;
        for (at, side, lit) in self.gantry_lamps() {
            // The bar's length (model x) across the road.
            let yaw = (-side.y).atan2(side.x);
            for j in 0..LAMP_BARS {
                let y = (j as f32 - (LAMP_BARS - 1) as f32 * 0.5) * bar;
                let tint = lit.unwrap_or(UNLIT);
                if let Some(inst) = crate::shooter::hold::sized(lamp, models, at + Vec3::Y * y, yaw, Quat::IDENTITY, tint, LAMP_SCALE) {
                    out.push(inst);
                }
            }
        }
    }

    /// The lit lamps' light on the grid and the line, for `Game`'s lamps.
    pub(super) fn gantry_glow(&self) -> impl Iterator<Item = (Vec3, f32, [f32; 3], f32)> {
        let (_, d) = self.track.at(0.0);
        let back = Vec3::new(-d.x, -0.6, -d.y);
        self.gantry_lamps().into_iter().filter_map(move |(at, _, lit)| lit.map(|c| (at + back, 9.0, c, 1.2)))
    }
}
