//! A race: laps of a world's circuit (event `circuit`) against bots, all in
//! the same vehicle (world.json `race`). The player drives their own car
//! through `Ride` as anywhere else; the race places it on the grid, holds it
//! through the countdown, keeps everyone's laps, puts a car that is stuck or
//! over back on the track, and drives the rivals with `vehicle::bot`.
//!
//! `Game` hands the race the ride after every fixed step (`tick`), because
//! an activity's own context has no ride in it and the player's car lives
//! there.

use crate::modes::{Activity, ActivityCtx, ModeId};
use crate::player::{MoveInput, Player};
use crate::render::hud::Hud;
use crate::ride::Ride;
use crate::vehicle::bot::{Driver, Place, Rival, Style, Track};
use crate::vehicle::{Controls, Spec, Vehicle};
use crate::world::fields::{Circuit, RaceDef};
use crate::world::store::Store;
use glam::{Vec2, Vec3};

/// Seconds of countdown on the grid.
const COUNTDOWN: f32 = 3.5;
/// After the player finishes, how long the rest have to come home.
const RUN_IN: f32 = 30.0;
/// Names the rivals go by, in their karts' colours (vehicle/draw.rs).
const NAMES: [&str; 3] = ["BLUE", "GREEN", "YELLOW"];

/// One car's progress round the circuit.
#[derive(Clone, Copy, Default)]
struct Laps {
    /// Metres driven along the circuit since the start line, counting laps.
    progress: f32,
    last_s: f32,
    finish: Option<f32>,
    place: Option<Place>,
    /// Seconds stuck, seconds over on its side or roof, and seconds well off
    /// the track.
    slow: f32,
    upset: f32,
    away: f32,
}

impl Laps {
    fn start(s: f32, length: f32) -> Laps {
        Laps { progress: s, last_s: s.rem_euclid(length), ..Default::default() }
    }

    /// Follow the car to `place`, counting a lap each time the line is
    /// crossed going forward; true when it is stuck, over, or well off the
    /// track for a while, and wants putting back on it.
    fn follow(&mut self, place: Place, car: &Vehicle, length: f32, width: f32, dt: f32) -> bool {
        let mut ds = place.s - self.last_s;
        if ds < -length * 0.5 {
            ds += length;
        } else if ds > length * 0.5 {
            ds -= length;
        }
        self.progress += ds;
        self.last_s = place.s;
        self.place = Some(place);
        self.slow = if car.b.vel.length() < 1.0 { self.slow + dt } else { 0.0 };
        self.upset = if car.up().y < 0.5 { self.upset + dt } else { 0.0 };
        self.away = if place.off.abs() > width * 0.5 + 8.0 { self.away + dt } else { 0.0 };
        self.slow > 3.0 || self.upset > 2.0 || self.away > 3.0
    }

    fn clear(&mut self) {
        self.slow = 0.0;
        self.upset = 0.0;
        self.away = 0.0;
    }

    fn lap(&self, length: f32, laps: u32) -> u32 {
        ((self.progress / length).floor().max(0.0) as u32 + 1).min(laps)
    }
}

struct Bot {
    car: Vehicle,
    driver: Driver,
    laps: Laps,
    /// Running on ground that isn't loaded (too far from the player): moved
    /// along the centreline instead of driven, until the ground is there.
    ghost: bool,
}

enum Phase {
    /// On the grid, counting down.
    Grid(f32),
    Racing,
    /// The player is home; the rest have a while to finish.
    Done(f32),
}

pub struct Race {
    track: Track,
    laps: u32,
    spec: Spec,
    /// Where the vehicle models start in `Game::models`.
    base: usize,
    bots: Vec<Bot>,
    me: Laps,
    phase: Phase,
    clock: f32,
    placed: bool,
    again: bool,
    note: Option<String>,
    /// The player's car as the HUD shows it.
    speed: f32,
    tier: u32,
    boosting: bool,
    /// A bot driving the player's car (headless runs).
    autopilot: Option<Driver>,
}

impl Race {
    pub fn new(def: &RaceDef, circuit: &Circuit, spec: Spec, base: usize) -> Race {
        let track = Track::new(circuit.points.clone(), circuit.width).with_heights(circuit.heights.clone());
        // The rivals all race as the style bred for this vehicle (karts have
        // their own), a touch apart so they don't drive as one.
        let base_style = if spec.drift_boost > 0.0 { Style::kart() } else { Style::racer() };
        let n = def.rivals.clamp(1, 3) as usize;
        let bots = (0..n)
            .map(|i| {
                let k = i as f32;
                let style = Style { corner_g: base_style.corner_g - 0.6 * k, brake_g: base_style.brake_g - 0.4 * k, ..base_style };
                Bot { car: Vehicle::new(spec.clone(), Vec3::ZERO, 0.0), driver: Driver::new(style), laps: Laps::default(), ghost: false }
            })
            .collect();
        Race {
            track,
            laps: def.laps.max(1),
            spec,
            base,
            bots,
            me: Laps::default(),
            phase: Phase::Grid(COUNTDOWN),
            clock: 0.0,
            placed: false,
            again: false,
            note: None,
            speed: 0.0,
            tier: 0,
            boosting: false,
            autopilot: None,
        }
    }

    /// Let a bot drive the player's car, as the rivals drive (headless runs).
    pub fn autopilot(&mut self) {
        self.autopilot = Some(Driver::new(if self.spec.drift_boost > 0.0 { Style::kart() } else { Style::racer() }));
    }

    /// A line on how the race stands, for headless runs.
    pub fn status(&self) -> String {
        let phase = match self.phase {
            Phase::Grid(t) => format!("GRID {t:.1}"),
            Phase::Racing => "RACING".into(),
            Phase::Done(t) => format!("DONE {t:.1}"),
        };
        let order: Vec<String> = self
            .standings()
            .iter()
            .map(|(who, p, fin)| {
                let name = if *who == 0 { "YOU" } else { NAMES[(who - 1) % NAMES.len()] };
                match fin {
                    Some(t) => format!("{name} {}", clock(*t)),
                    None if *who > 0 && self.bots[who - 1].ghost => format!("{name} {:.0}m~", p),
                    None => format!("{name} {:.0}m", p),
                }
            })
            .collect();
        let off: Vec<String> = std::iter::once(&self.me).chain(self.bots.iter().map(|b| &b.laps)).map(|l| format!("{:.0}", l.place.map_or(0.0, |p| p.off))).collect();
        format!("{phase}  {}  lap {}/{}  {}  off {}  {:.0} km/h", clock(self.clock), self.me.lap(self.track.length, self.laps), self.laps, order.join(", "), off.join("/"), self.speed * 3.6)
    }

    /// Grid slot `i` (0 at the front): behind the line, two abreast.
    fn slot(&self, i: usize) -> (Vec2, Vec2, f32, f32) {
        let s = -6.0 * (i as f32 + 1.0);
        let lane = if i.is_multiple_of(2) { -2.2 } else { 2.2 };
        let (p, d) = self.track.at(s);
        (p + Vec2::new(-d.y, d.x) * lane, d, lane, s)
    }

    /// Everyone onto the grid: the rivals in front, the player at the back.
    fn place_all(&mut self, ride: &mut Ride, player: &mut Player, store: &Store) {
        let length = self.track.length;
        let n = self.bots.len();
        for i in 0..n {
            let (p, d, lane, s) = self.slot(i);
            let at = Vec3::new(p.x, ground(store, p, player.pos.y), p.y);
            let b = &mut self.bots[i];
            b.car = Vehicle::new(self.spec.clone(), at, yaw_of(d));
            b.driver = Driver::new(b.driver.style);
            b.driver.lane = lane;
            b.laps = Laps::start(s, length);
            b.ghost = false;
        }
        let (p, d, _, s) = self.slot(n);
        let at = Vec3::new(p.x, ground(store, p, player.pos.y), p.y);
        ride.race_car(player, self.spec.clone(), at, yaw_of(d));
        self.me = Laps::start(s, length);
        self.phase = Phase::Grid(COUNTDOWN);
        self.clock = 0.0;
        self.placed = true;
    }

    /// One fixed step, after the player's own (Game::fixed_step).
    #[allow(clippy::needless_range_loop)] // each bot reads the others by index
    pub fn tick(&mut self, ride: &mut Ride, player: &mut Player, store: &Store, sea: f32, slopes: bool, dt: f32) {
        if !self.placed || ride.car().is_none() {
            self.place_all(ride, player, store);
        }
        if let Phase::Grid(t) = &mut self.phase {
            *t -= dt;
            ride.hold = true;
            let go = *t <= 0.0;
            for b in &mut self.bots {
                b.car.step(&Controls::default(), store, sea, slopes, dt);
            }
            if go {
                self.phase = Phase::Racing;
                self.note = Some("GO".into());
                ride.hold = false;
            }
            self.watch(ride);
            return;
        }
        self.clock += dt;
        let length = self.track.length;
        let total = length * self.laps as f32;

        // Where everyone is, then the rivals' moves.
        let my_place = ride.car().map(|c| self.track.place(c.pos(), self.me.place));
        let places: Vec<Place> = self.bots.iter().map(|b| self.track.place(b.car.pos(), b.laps.place)).collect();
        let my_speed = ride.car().map_or(0.0, |c| c.v_fwd());
        for i in 0..self.bots.len() {
            if self.bots[i].laps.finish.is_some() {
                self.bots[i].car.step(&Controls { brake: 0.4, ..Default::default() }, store, sea, slopes, dt);
                continue;
            }
            let mut rivals: Vec<Rival> = (0..self.bots.len()).filter(|&j| j != i).map(|j| Rival { place: places[j], speed: self.bots[j].car.v_fwd() }).collect();
            if let Some(p) = my_place {
                rivals.push(Rival { place: p, speed: my_speed });
            }
            let b = &mut self.bots[i];
            let c = b.driver.race(&b.car, &self.track, &rivals);
            b.car.step(&c, store, sea, slopes, dt);
            if b.car.waiting {
                // Out where the world isn't loaded: carried round the
                // centreline at the speed its driver wants, out of sight.
                let v = b.driver.want.min(b.car.spec.top_speed).max(8.0);
                let (p, d) = self.track.at(places[i].s + v * dt);
                let y = b.car.pos().y;
                b.car.b.pos = Vec3::new(p.x, y, p.y);
                b.car.b.rot = glam::Quat::from_rotation_y(yaw_of(d));
                b.car.prev_pos = b.car.b.pos;
                b.car.prev_rot = b.car.b.rot;
                b.ghost = true;
            } else if b.ghost {
                // Back on loaded ground: set down on it where it has got to.
                put_back(&mut b.car, &self.track, places[i].s, store);
                b.car.b.vel = b.car.forward() * b.driver.want.min(b.car.spec.top_speed) * 0.8;
                b.ghost = false;
            }
        }
        // The autopilot chooses the player's controls for the next step.
        if let (Some(d), Some(car), None) = (self.autopilot.as_mut(), ride.car(), self.me.finish) {
            let rivals: Vec<Rival> = (0..self.bots.len()).map(|j| Rival { place: places[j], speed: self.bots[j].car.v_fwd() }).collect();
            let c = d.race(car, &self.track, &rivals);
            ride.auto = Some(c);
        }

        // Cars that touch push each other.
        if let Some(mine) = ride.car_mut() {
            for b in &mut self.bots {
                crate::vehicle::bump(mine, &mut b.car);
            }
        }
        for i in 0..self.bots.len() {
            for j in i + 1..self.bots.len() {
                let (a, b) = self.bots.split_at_mut(j);
                crate::vehicle::bump(&mut a[i].car, &mut b[0].car);
            }
        }

        // Laps, finishes, and cars put back on the track.
        for i in 0..self.bots.len() {
            let b = &mut self.bots[i];
            if b.laps.finish.is_some() {
                continue;
            }
            let lost = b.laps.follow(places[i], &b.car, length, self.track.width, dt);
            if b.laps.progress >= total {
                b.laps.finish = Some(self.clock);
            } else if lost {
                put_back(&mut b.car, &self.track, places[i].s, store);
                b.laps.clear();
            }
        }
        if let (Some(p), None) = (my_place, self.me.finish) {
            let width = self.track.width;
            let lost = ride.car().is_some_and(|c| self.me.follow(p, c, length, width, dt));
            if self.me.progress >= total {
                self.me.finish = Some(self.clock);
                let pos = self.position();
                self.note = Some(format!("FINISHED {}   {}", ordinal(pos), clock(self.clock)));
                self.phase = Phase::Done(0.0);
            } else if lost {
                if let Some(car) = ride.car_mut() {
                    put_back(car, &self.track, p.s, store);
                }
                self.me.clear();
                self.note = Some("BACK ON THE TRACK".into());
            }
        }
        if let Phase::Done(t) = &mut self.phase {
            // Home: the player drives on as they like while the rest finish.
            *t += dt;
            if std::mem::take(&mut self.again) {
                self.placed = false;
            }
        }
        self.again = false;
        self.watch(ride);
        self.engines(player);
    }

    /// The player's car, copied for the HUD.
    fn watch(&mut self, ride: &Ride) {
        if let Some(c) = ride.car() {
            self.speed = c.v_fwd();
            self.tier = c.drift_tier();
            self.boosting = c.kick_t > 0.0;
        }
    }

    /// The rivals' engines, each a held voice at its car.
    fn engines(&self, player: &Player) {
        for (i, b) in self.bots.iter().enumerate() {
            if (b.car.pos() - player.pos).length() < 120.0 {
                crate::sound::emit(crate::sound::Noise::engine(b.car.pos(), b.car.rpm, i as u32));
            }
        }
    }

    /// The player's place in the race, 1 for leading.
    fn position(&self) -> usize {
        self.standings().iter().position(|(who, _, _)| *who == 0).map_or(1, |p| p + 1)
    }

    /// Everyone in race order: (0 for the player or 1.. for a rival,
    /// progress, finish time). Finishers by time, then by distance run.
    fn standings(&self) -> Vec<(usize, f32, Option<f32>)> {
        let mut out = vec![(0, self.me.progress, self.me.finish)];
        out.extend(self.bots.iter().enumerate().map(|(i, b)| (i + 1, b.laps.progress, b.laps.finish)));
        out.sort_by(|a, b| match (a.2, b.2) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.1.total_cmp(&a.1),
        });
        out
    }
}

/// The ground under `p`, searching down from a little above `near` and then
/// from high up: where a car is set down on the grid or back on the track.
fn ground(store: &Store, p: Vec2, near: f32) -> f32 {
    let at = |y: f32| crate::vehicle::draw_ground(store, Vec3::new(p.x, y, p.y));
    at(near + 6.0).or_else(|| (0..40).rev().find_map(|k| at(k as f32 * 8.0))).unwrap_or(near)
}

fn yaw_of(d: Vec2) -> f32 {
    (-d.x).atan2(-d.y)
}

/// Put `car` back on the centreline `s` metres round, facing the way round,
/// stopped.
fn put_back(car: &mut Vehicle, track: &Track, s: f32, store: &Store) {
    let (p, d) = track.at(s);
    let y = ground(store, p, car.pos().y) + 0.3;
    *car = Vehicle::new(car.spec.clone(), Vec3::new(p.x, y, p.y), yaw_of(d));
}

fn ordinal(n: usize) -> String {
    let tail = match n {
        1 => "ST",
        2 => "ND",
        3 => "RD",
        _ => "TH",
    };
    format!("{n}{tail}")
}

/// Seconds as minutes:seconds.tenths.
fn clock(t: f32) -> String {
    format!("{}:{:04.1}", (t / 60.0).floor() as u32, t % 60.0)
}

impl Activity for Race {
    fn id(&self) -> ModeId {
        ModeId::Race
    }

    fn frame(&mut self, _cx: &mut ActivityCtx, _input: &MoveInput, actions: &mut crate::game::Actions, _dt: f32) {
        self.again |= std::mem::take(&mut actions.accept);
        // In a race you stay in the car.
        actions.vehicle_toggle = false;
        actions.bike_toggle = false;
        actions.kayak_toggle = false;
        actions.glide = false;
    }

    fn step(&mut self, _cx: ActivityCtx, _input: &MoveInput) {}

    fn race_mut(&mut self) -> Option<&mut Race> {
        Some(self)
    }

    fn note(&mut self) -> Option<String> {
        self.note.take()
    }

    fn instances(&self, _player: &Player, alpha: f32, _base: usize, models: &[crate::models::Model], out: &mut Vec<(usize, crate::models::Instance)>) {
        for (i, b) in self.bots.iter().enumerate() {
            crate::vehicle::draw::instances_in(&b.car, alpha, self.base, models, true, false, i + 1, out);
        }
    }

    fn hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32, _fov: f32) {
        let white = [1.0, 1.0, 1.0, 0.95];
        let dim = [0.85, 0.9, 0.95, 0.85];
        let warm = [1.0, 0.82, 0.35, 1.0];
        let centre = |hud: &mut Hud, y: f32, size: f32, c: [f32; 4], t: &str| {
            let tw = Hud::text_width(t, size);
            hud.label((w * 0.5 - tw * 0.5).floor(), y, size, c, t);
        };
        let n = self.bots.len() + 1;
        // Position and lap, top right; the race clock under them.
        let right = |hud: &mut Hud, y: f32, size: f32, c: [f32; 4], t: &str| {
            let tw = Hud::text_width(t, size);
            hud.label((w - tw - 24.0).floor(), y, size, c, t);
        };
        right(hud, 20.0, s * 2.4, white, &format!("{}/{}", self.position(), n));
        right(hud, 20.0 + 26.0 * s, s * 1.2, dim, &format!("LAP {}/{}", self.me.lap(self.track.length, self.laps), self.laps));
        right(hud, 20.0 + 40.0 * s, s * 1.2, dim, &clock(self.me.finish.unwrap_or(self.clock)));
        // Speed and the drift's charge, bottom centre.
        centre(hud, h - 34.0 * s, s * 1.6, white, &format!("{:.0} KM/H", self.speed.abs() * 3.6));
        let pips = match self.tier {
            0 if self.boosting => "BOOST".to_string(),
            0 => String::new(),
            t => format!("DRIFT {}", "*".repeat(t as usize)),
        };
        if !pips.is_empty() {
            centre(hud, h - 50.0 * s, s * 1.2, warm, &pips);
        }
        match self.phase {
            Phase::Grid(t) => {
                let left = t.ceil() as u32;
                let say = if left > 3 { "READY".to_string() } else { left.to_string() };
                centre(hud, h * 0.3, s * 4.0, warm, &say);
                centre(hud, h * 0.3 + 44.0 * s, s, dim, "W DRIVE  A D STEER  SPACE HOP, HOLD TO DRIFT, LET GO TO BOOST");
            }
            Phase::Racing => {}
            Phase::Done(since) => {
                // The results, as they come in.
                let y0 = h * 0.26;
                let (bw, bh) = (200.0 * s, (52.0 + 14.0 * n as f32) * s);
                hud.rect((w * 0.5 - bw * 0.5).floor(), y0 - 8.0 * s, bw, bh, [0.0, 0.0, 0.0, 0.5]);
                centre(hud, y0, s * 1.8, white, "RESULTS");
                for (k, (who, _, fin)) in self.standings().iter().enumerate() {
                    let name = if *who == 0 { "YOU" } else { NAMES[(who - 1) % NAMES.len()] };
                    let time = fin.map_or_else(|| if since > RUN_IN { "DNF".to_string() } else { "RACING".to_string() }, clock);
                    let c = if *who == 0 { warm } else { white };
                    centre(hud, y0 + (22.0 + 14.0 * k as f32) * s, s * 1.2, c, &format!("{}  {:<6}  {}", ordinal(k + 1), name, time));
                }
                centre(hud, y0 + (30.0 + 14.0 * n as f32) * s, s, dim, "ENTER TO RACE AGAIN");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crossing the line going forward counts a lap; rolling back over it
    /// takes the distance away again.
    #[test]
    fn laps_count_across_the_line() {
        let length = 100.0;
        let car = Vehicle::new(Spec::default(), Vec3::ZERO, 0.0);
        let mut l = Laps::start(-6.0, length);
        let at = |s: f32| Place { seg: 0, s, off: 0.0 };
        for s in [96.0, 99.0, 2.0, 30.0, 60.0, 90.0, 5.0] {
            l.follow(at(s), &car, length, 12.0, 0.1);
        }
        assert!((l.progress - 105.0).abs() < 1e-3, "{}", l.progress);
        assert_eq!(l.lap(length, 3), 2);
        l.follow(at(98.0), &car, length, 12.0, 0.1);
        assert!((l.progress - 98.0).abs() < 1e-3, "{}", l.progress);
    }

    #[test]
    fn clock_reads_minutes_and_tenths() {
        assert_eq!(clock(83.45), "1:23.4");
        assert_eq!(ordinal(2), "2ND");
    }
}
