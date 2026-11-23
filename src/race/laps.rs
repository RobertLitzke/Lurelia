//! Each car's laps round the circuit, and putting a car back on the track.

use crate::vehicle::bot::{Place, Track};
use crate::vehicle::{ground, yaw_along, Vehicle};
use crate::world::store::Store;
use glam::Vec3;

/// Seconds far out of bounds before a car is put back on the track: the
/// countdown the HUD shows.
pub(super) const AWAY_SECS: f32 = 3.0;
/// Far out of bounds: more than this many metres past the kerb...
pub(super) const FAR_OFF: f32 = 15.0;
/// ...or this many below the road.
pub(super) const FAR_DOWN: f32 = 6.0;
/// Seconds a car put back on the track passes through the others.
pub(super) const GHOST_SECS: f32 = 1.0;

/// One car's progress round the circuit.
#[derive(Clone, Copy, Default)]
pub(super) struct Laps {
    /// Metres driven along the circuit since the start line, counting laps.
    pub(super) progress: f32,
    pub(super) last_s: f32,
    pub(super) finish: Option<f32>,
    pub(super) place: Option<Place>,
    /// Seconds far out of bounds (`far`), and seconds stuck (a bot backs out).
    pub(super) away: f32,
    pub(super) slow: f32,
    /// The last place round the circuit it was on the road or the kerb:
    /// where a put-back sets it down, so leaving the road never gains it
    /// anything.
    pub(super) road_s: f32,
    /// Seconds left passing through other cars after a put-back.
    pub(super) ghost: f32,
    /// Checkpoints passed since the start, counting every lap's.
    pub(super) passed: u32,
}

impl Laps {
    pub(super) fn start(s: f32, length: f32) -> Laps {
        let s0 = s.rem_euclid(length);
        Laps { progress: s, last_s: s0, road_s: s0, ..Default::default() }
    }

    /// Follow the car to `place`, counting a lap each time the line is
    /// crossed going forward; true when it has been far out of bounds for
    /// `AWAY_SECS` and wants putting back on the track. Nothing else puts a
    /// car back: one on its side rights itself where it is (`Vehicle`'s
    /// `righting`), and one that is stuck backs out. `road_y` is the road's
    /// height here, where known.
    ///
    /// A jump forward round the track further than the car could have
    /// driven in the step (across a gap to a later stretch of road) is not
    /// counted, so a cut gains nothing. Nor does going past a checkpoint
    /// (`checks`, lap distances in order) far out of bounds: the car's
    /// progress waits at the checkpoint until it comes back to the road,
    /// and whatever it skipped it drives again.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn follow(&mut self, place: Place, car: &Vehicle, length: f32, width: f32, road_y: Option<f32>, checks: &[f32], dt: f32) -> bool {
        let mut ds = place.s - self.last_s;
        if ds < -length * 0.5 {
            ds += length;
        } else if ds > length * 0.5 {
            ds -= length;
        }
        if ds <= car.b.vel.length() * dt + 4.0 {
            let mut to = self.progress + ds;
            if !checks.is_empty() {
                let next = self.next_check(checks, length);
                if to >= next {
                    if far(place.off, width, car.pos().y, road_y) {
                        to = to.min(next - 0.01).max(self.progress);
                    } else {
                        self.passed += 1;
                    }
                }
            }
            self.progress = to;
        }
        self.last_s = place.s;
        self.place = Some(place);
        if place.off.abs() <= width * 0.5 + 1.0 {
            self.road_s = place.s;
        }
        self.slow = if car.b.vel.length() < 1.0 { self.slow + dt } else { 0.0 };
        self.away = if far(place.off, width, car.pos().y, road_y) { self.away + dt } else { 0.0 };
        self.ghost = (self.ghost - dt).max(0.0);
        self.away > AWAY_SECS
    }

    /// Where the next checkpoint not yet passed is, in progress.
    pub(super) fn next_check(&self, checks: &[f32], length: f32) -> f32 {
        let m = checks.len() as u32;
        (self.passed / m) as f32 * length + checks[(self.passed % m) as usize]
    }

    /// After a put-back: the timers cleared, and a moment passing through
    /// the other cars.
    pub(super) fn clear(&mut self) {
        self.slow = 0.0;
        self.away = 0.0;
        self.ghost = GHOST_SECS;
    }

    pub(super) fn lap(&self, length: f32, laps: u32) -> u32 {
        ((self.progress / length).floor().max(0.0) as u32 + 1).min(laps)
    }
}

/// Whether a car `off` metres from the centreline of a road `width` wide, at
/// height `y`, is far out of bounds: well past the kerb, or fallen well
/// below the road.
pub(super) fn far(off: f32, width: f32, y: f32, road_y: Option<f32>) -> bool {
    off.abs() > width * 0.5 + 1.0 + FAR_OFF || road_y.is_some_and(|r| y < r - FAR_DOWN)
}

/// Put `car` back on the centreline `s` metres round, facing the way round,
/// stopped.
pub(super) fn put_back(car: &mut Vehicle, track: &Track, s: f32, store: &Store) {
    let (p, d) = track.at(s);
    // On the road where its height is known, so a car put back in a
    // tunnel lands on the tunnel's floor and not on its roof.
    let y = track.height_at(s).unwrap_or_else(|| ground(store, p, car.pos().y)) + 0.3;
    *car = Vehicle::new(car.spec.clone(), Vec3::new(p.x, y, p.y), yaw_along(d));
}

/// Off the road past its kerb, a car is dragged down toward this share of
/// its top speed...
const VERGE_SPEED: f32 = 0.6;
/// ...this hard for each m/s over it, m/s² per m/s, on top of a steady
/// VERGE_DRAG m/s². The ground's own feel is too close to the road's on
/// some circuits (ice road beside snow) to be any penalty by itself.
const VERGE_BITE: f32 = 0.8;
const VERGE_DRAG: f32 = 1.0;

/// Slow `car` if it is down on the ground `off` metres from the centreline
/// of a road `width` wide, past the kerb.
pub(super) fn verge(car: &mut Vehicle, off: f32, width: f32, dt: f32) {
    if off.abs() <= width * 0.5 + 1.0 || car.grounded == 0 {
        return;
    }
    let flat = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
    let v = flat.length();
    if v < 0.5 {
        return;
    }
    let cap = car.spec.top_speed * VERGE_SPEED;
    let slow = (VERGE_DRAG + VERGE_BITE * (v - cap).max(0.0)) * dt;
    car.b.vel -= flat / v * slow.min(v);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::Spec;

    /// Crossing the line going forward counts a lap; rolling back over it
    /// takes the distance away again.
    #[test]
    fn laps_count_across_the_line() {
        let length = 100.0;
        // Steps of up to 30 m in 0.1 s: a car fast enough to make them, or
        // the shortcut guard would refuse them.
        let mut car = Vehicle::new(Spec::default(), Vec3::ZERO, 0.0);
        car.b.vel = Vec3::new(0.0, 0.0, -400.0);
        let mut l = Laps::start(-6.0, length);
        let at = |s: f32| Place { seg: 0, s, off: 0.0 };
        for s in [96.0, 99.0, 2.0, 30.0, 60.0, 90.0, 5.0] {
            l.follow(at(s), &car, length, 12.0, None, &[], 0.1);
        }
        assert!((l.progress - 105.0).abs() < 1e-3, "{}", l.progress);
        assert_eq!(l.lap(length, 3), 2);
        l.follow(at(98.0), &car, length, 12.0, None, &[], 0.1);
        assert!((l.progress - 98.0).abs() < 1e-3, "{}", l.progress);
    }

    /// Past the kerb a car at full speed loses a good part of it within a
    /// second, and settles toward VERGE_SPEED of its top; on the road it
    /// keeps it all.
    #[test]
    fn the_verge_slows_a_car() {
        let spec = Spec { top_speed: 29.0, ..Spec::default() };
        let run = |off: f32| {
            let mut car = Vehicle::new(spec.clone(), Vec3::ZERO, 0.0);
            car.grounded = 4;
            car.b.vel = Vec3::new(0.0, 0.0, -29.0);
            for _ in 0..60 {
                verge(&mut car, off, 12.0, 1.0 / 60.0);
            }
            car.b.vel.length()
        };
        assert_eq!(run(6.5), 29.0);
        let v = run(9.0);
        assert!(v > 17.0 && v < 23.0, "{v}");
    }

    /// A car that cuts across a gap to a later stretch gains nothing: the
    /// jump round the track isn't counted.
    #[test]
    fn a_cut_across_a_gap_gains_nothing() {
        let length = 1000.0;
        let mut car = Vehicle::new(Spec::default(), Vec3::ZERO, 0.0);
        car.b.vel = Vec3::new(0.0, 0.0, -25.0);
        let mut l = Laps::start(100.0, length);
        let dt = 1.0 / 60.0;
        l.follow(Place { seg: 0, s: 100.4, off: 9.0 }, &car, length, 12.0, None, &[], dt);
        // Across the verge, the nearest road is suddenly 300 m further on.
        l.follow(Place { seg: 0, s: 400.0, off: -5.0 }, &car, length, 12.0, None, &[], dt);
        l.follow(Place { seg: 0, s: 400.4, off: -4.0 }, &car, length, 12.0, None, &[], dt);
        assert!((l.progress - 100.8).abs() < 1e-3, "{}", l.progress);
    }

    /// Only far out of bounds puts a car back: not upside down, not stuck,
    /// not 10 m past the kerb.
    #[test]
    fn only_far_out_of_bounds_puts_a_car_back() {
        let length = 1000.0;
        let dt = 0.1;
        let run = |off: f32, y: f32, upset: bool| {
            let mut car = Vehicle::new(Spec::default(), Vec3::new(0.0, y, 0.0), 0.0);
            if upset {
                car.b.rot = glam::Quat::from_rotation_z(std::f32::consts::PI);
            }
            let mut l = Laps::start(50.0, length);
            (0..100).any(|_| l.follow(Place { seg: 0, s: 50.0, off }, &car, length, 12.0, Some(30.0), &[], dt))
        };
        assert!(!run(0.0, 30.0, true), "upside down on the road");
        assert!(!run(0.0, 30.0, false), "stuck on the road");
        assert!(!run(17.0, 30.0, false), "10 m past the kerb");
        assert!(run(25.0, 30.0, false), "18 m past the kerb");
        assert!(run(0.0, 20.0, false), "fallen 10 m below the road");
    }

    /// Past a checkpoint far out of bounds a car's progress waits at the
    /// checkpoint; on the road it passes, and a lap needs them all.
    #[test]
    fn a_checkpoint_skipped_far_off_holds_progress() {
        let length = 300.0;
        let checks = [100.0, 200.0];
        let mut car = Vehicle::new(Spec::default(), Vec3::ZERO, 0.0);
        car.b.vel = Vec3::new(0.0, 0.0, -25.0);
        let dt = 0.1;
        let drive = |l: &mut Laps, from: f32, to: f32, off: f32| {
            let mut s = from;
            while s < to {
                s += 2.0;
                l.follow(Place { seg: 0, s: s.rem_euclid(length), off }, &car, length, 12.0, None, &checks, dt);
            }
        };
        // On the road all the way: both passed, a lap counted.
        let mut l = Laps::start(0.0, length);
        drive(&mut l, 0.0, 302.0, 0.0);
        assert_eq!(l.passed, 2);
        assert_eq!(l.lap(length, 3), 2);
        // Round the second checkpoint 30 m off the road: held there, and the
        // 40 m it went on off the road don't count.
        let mut l = Laps::start(0.0, length);
        drive(&mut l, 0.0, 190.0, 0.0);
        drive(&mut l, 190.0, 230.0, 30.0);
        assert!((l.progress - 200.0).abs() < 0.1, "{}", l.progress);
        assert_eq!(l.passed, 1);
        // Back on the road it passes, and carries on from where it is held.
        drive(&mut l, 230.0, 240.0, 0.0);
        assert_eq!(l.passed, 2);
        assert!(l.progress < 212.0, "{}", l.progress);
    }
}
