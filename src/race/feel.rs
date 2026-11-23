//! What makes a kart race feel like one, items or none: the
//! slipstream behind another kart, and the rocket start.

use super::fight::kart;
use super::Race;
use crate::ride::Ride;
use crate::vehicle::Vehicle;
use glam::Vec3;

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
