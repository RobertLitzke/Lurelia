//! A race: laps of a world's circuit (event `circuit`) against bots, all in
//! the same vehicle (world.json `race`). The player drives their own car
//! through `Ride` as anywhere else; the race places it on the grid, holds it
//! through the countdown, keeps everyone's laps, puts a car that is stuck or
//! over back on the track, and drives the rivals with `vehicle::bot`.
//!
//! `Game` hands the race the ride after every fixed step (`tick`), because
//! an activity's own context has no ride in it and the player's car lives
//! there.

mod arms;
pub mod brain;
mod feel;
mod fight;
mod hud;
mod laps;
mod polish;
pub mod records;

use crate::modes::{Activity, ActivityCtx, ModeId};
use crate::player::{MoveInput, Player};
use crate::render::hud::Hud;
use crate::ride::Ride;
use crate::vehicle::bot::{Driver, Place, Rival, Style, Track};
use crate::vehicle::{ground, yaw_along, Controls, Handling, Spec, Vehicle};
use crate::world::fields::{Circuit, RaceDef};
use crate::world::store::Store;
use glam::{Vec2, Vec3};
pub(crate) use hud::clock;
use hud::{ordinal, Map};
use arms::{Arms, Harm, Item, Slot};
use brain::{Brain, Lineup, Tier};
use laps::{put_back, verge, Laps};
use polish::Polish;

/// Seconds a boost pad holds a kart at its boosted top speed.
const PAD_SECS: f32 = 1.2;

/// Seconds of countdown on the grid.
const COUNTDOWN: f32 = 3.5;
/// After the player finishes, how long the rest have to come home.
const RUN_IN: f32 = 30.0;
/// How fast the player's (hidden) kart is brought to a stop after its
/// finish, a share of its speed a second.
const HOME_STOP: f32 = 1.5;
/// Names the rivals go by, in their karts' colours (vehicle/draw.rs
/// `KART_COLOURS`, which the HUD draws them in too).
const NAMES: [&str; 7] = ["BLUE", "GREEN", "YELLOW", "PURPLE", "ORANGE", "CYAN", "PINK"];
/// The most rivals a race takes: eight karts on the grid.
pub const MAX_RIVALS: u32 = 7;
/// Seconds going the wrong way round before the HUD says so.
const WRONG_AFTER: f32 = 1.0;


struct Bot {
    car: Vehicle,
    driver: Driver,
    laps: Laps,
    /// Running on ground that isn't loaded (too far from the player): moved
    /// along the centreline instead of driven, until the ground is there.
    ghost: bool,
    /// Seconds left backing out of where it is stuck.
    unstick: f32,
}

impl Bot {
    /// Past the finish: gone from the road, drawn, heard and touched by
    /// nobody.
    fn home(&self) -> bool {
        self.laps.finish.is_some()
    }
}

enum Phase {
    /// On the grid, counting down.
    Grid(f32),
    Racing,
    /// The player is home; the rest have a while to finish.
    Done(f32),
}

/// What a race is built from, and all of one that outlives a restart: the
/// course and its laps, the karts' own specs (the player's, then each
/// rival's), the level the rivals drive to, where the vehicle models start,
/// the headless lineup, the weapons, how many races have started, and
/// whether a bot drives the player's car. Everything else in a `Race` is
/// made new from these by `Race::fresh`.
struct Setup {
    track: Track,
    checkpoints: Vec<f32>,
    laps: u32,
    spec: Spec,
    rivals: Vec<Spec>,
    level: f32,
    base: usize,
    lineup: Option<Lineup>,
    arms: Arms,
    races: u64,
    autopilot: bool,
}

pub struct Race {
    track: Track,
    laps: u32,
    spec: Spec,
    /// Each rival's kart as loaded. Its car's own spec changes with its tier,
    /// the polish and damage.
    rivals: Vec<Spec>,
    /// The share of the grip and braking their style trusts that the rivals
    /// drive to.
    level: f32,
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
    /// The player's lap times so far, laps counted, and when the lap under
    /// way began.
    times: Vec<f32>,
    counted: i32,
    lap_start: f32,
    /// Seconds the player has been going the wrong way round, and which way
    /// to turn round (-1 left, 1 right).
    wrong: f32,
    wrong_turn: f32,
    /// The autopilot backing out of where it is stuck, seconds left.
    auto_unstick: f32,
    /// The weapons and items (arms.rs, fight.rs): what is in the air and the
    /// boxes, and by kart (0 the player, then the rivals) what has been done
    /// to it, what it carries, seconds to its next shot of a held weapon,
    /// which kart its rocket is after, and how long it has held its item.
    arms: Arms,
    harm: Vec<Harm>,
    slots: Vec<Slot>,
    reload: Vec<f32>,
    targets: Vec<Option<usize>>,
    held_t: Vec<f32>,
    /// The fire button, held this frame and last step, and the weapon the
    /// digits picked (`KART_ARMS=all`).
    trigger: bool,
    trigger_was: bool,
    pick: usize,
    /// The stick or the drive keys held back (fire behind), or the stick
    /// held forward (throw a dropped item ahead), as the button is pressed;
    /// and the look-behind held (Alt, or the right stick pressed).
    stick_back: bool,
    stick_ahead: bool,
    look_back: bool,
    /// Each kart's brain (0 the player's, for the autopilot), its tier this
    /// race, its own top speed, power and drive force before the tier, and
    /// the brains' names in a headless lineup (`KART_BRAINS`).
    brains: Vec<Brain>,
    tiers: Vec<Tier>,
    raw: Vec<(f32, f32, f32)>,
    lineup: Option<Lineup>,
    /// Races started, which shuffles the tiers between them.
    races: u64,
    /// Each item box's place round the track and off the centreline.
    box_at: Vec<(f32, f32)>,
    /// Seconds each kart has been in another's slipstream (feel.rs).
    draft: Vec<f32>,
    /// The throttle held on the grid: whether now, and the countdown's time
    /// when it was first held without letting go; and seconds left bogged
    /// down after jumping the start.
    gas: bool,
    gas_from: Option<f32>,
    bog: f32,
    /// The racing line wearing in (polish.rs), each kart's own top speed,
    /// power and drive force before polish scales them, and how much faster
    /// the polish under the player makes them, for the HUD.
    polish: Polish,
    bases: Vec<(f32, f32, f32)>,
    my_polish: f32,
    /// Whether the player's finish has gone to the records (`result`).
    reported: bool,
    /// A new race wants the world as built: everything the last one did to
    /// it (polish, goo, blocks) is wiped, as an arena is between matches
    /// (`Activity::wants_world_clean`, `Streamer::clear_edits`).
    clean_world: bool,
    /// The countdown's last second beeped.
    beeped: u32,
    /// The player's splits: how many of the lap's two sector lines they have
    /// crossed this lap, the best time to each from the start of a lap, and
    /// the last split against the best (seconds, negative quicker) with the
    /// race clock when it was taken, for the HUD.
    sectors: u32,
    best_split: Vec<f32>,
    split: Option<(f32, f32)>,
    /// Where the splits are taken round the lap: the track's checkpoints,
    /// or a third and two thirds of the way round.
    split_at: Vec<f32>,
    /// The lap distances a lap must pass in order (the track's checkpoints;
    /// empty for a circuit without).
    checkpoints: Vec<f32>,
    /// Checkpoints, every `check_every` metres round: when each kart passed
    /// each one, counting on through the laps. The standings' gaps are
    /// timed between karts at the last checkpoint both have passed, and
    /// change only as checkpoints are passed.
    checks: Vec<Vec<f32>>,
    check_every: f32,
    /// The player's car on the ground, for the minimap.
    my_pos: Vec2,
    map: Map,
}

impl Race {
    /// A race on `circuit`: the player in `spec`, rival `i` in `rivals[i]`
    /// (a mix of karts in a kart race), driving to `level` of the grip and
    /// braking their style trusts (the launch screen's difficulty), on a road
    /// whose grip the weather has brought down to `grip` of the dry:
    /// every kart's tyres, and what the
    /// rivals trust of them.
    #[allow(clippy::too_many_arguments)]
    pub fn new(def: &RaceDef, circuit: &Circuit, spec: Spec, rivals: Vec<Spec>, level: f32, grip: f32, base: usize, lib: &crate::library::Library) -> Race {
        let wet = |mut s: Spec| {
            s.grip *= grip;
            s
        };
        let spec = wet(spec);
        // A rival with no kart of its own drives the player's.
        let n = def.rivals.clamp(1, MAX_RIVALS) as usize;
        let rivals: Vec<Spec> = (0..n).map(|i| rivals.get(i).cloned().map(wet).unwrap_or_else(|| spec.clone())).collect();
        let track = Track::of(circuit);
        let arms = Arms::new(lib, &track, &circuit.items);
        Race::fresh(Setup {
            track,
            checkpoints: circuit.checkpoints.clone(),
            laps: def.laps.max(1),
            spec,
            rivals,
            level: level * grip,
            base,
            lineup: Lineup::from_env(),
            arms,
            races: 0,
            autopilot: false,
        })
    }

    /// Back to the grid for the race again (the pause menu's RESTART RACE,
    /// Enter on the results): a new race from the same `Setup`, with nothing
    /// of the last one in it.
    pub fn restart(&mut self) {
        *self = Race::fresh(Setup {
            track: self.track.clone(),
            checkpoints: self.checkpoints.clone(),
            laps: self.laps,
            spec: self.spec.clone(),
            rivals: self.rivals.clone(),
            level: self.level,
            base: self.base,
            lineup: self.lineup.clone(),
            arms: self.arms.fresh(),
            races: self.races,
            autopilot: self.autopilot.is_some(),
        });
    }

    /// A race not yet on its grid (`place_all` puts it there at the next
    /// step): every kart, timer, item and note as a race starts with them.
    fn fresh(setup: Setup) -> Race {
        let Setup { track, checkpoints, laps, spec, rivals, level, base, lineup, arms, races, autopilot } = setup;
        let n = rivals.len();
        let split_at = if checkpoints.is_empty() { vec![track.length / 3.0, track.length * 2.0 / 3.0] } else { checkpoints.clone() };
        // The rivals all race as the style bred for this vehicle (karts have
        // their own, fitted to each kart), with their brain's shares of it.
        // What sets them apart is their tier (`place_all`): a third of them
        // a little faster, a third a little slower.
        let brains: Vec<Brain> = (0..=n).map(|k| lineup.as_ref().and_then(|l| l.brains.get(k).copied()).unwrap_or_default()).collect();
        let bots: Vec<Bot> = rivals
            .iter()
            .enumerate()
            .map(|(i, theirs)| {
                let style = driver_style(theirs, brains[i + 1], level);
                Bot { car: Vehicle::new(theirs.clone(), Vec3::ZERO, 0.0), driver: Driver::new(style), laps: Laps::default(), ghost: false, unstick: 0.0 }
            })
            .collect();
        let map = Map::new(&track);
        let check_every = (track.length / (track.length / CHECK_M).round().max(4.0)).max(1.0);
        let box_at = arms.boxes.iter().map(|b| track.place(b.at, None)).map(|p| (p.s, p.off)).collect();
        let own = |s: &Spec| (s.top_speed, s.power, s.force);
        let bases: Vec<(f32, f32, f32)> = std::iter::once(own(&spec)).chain(rivals.iter().map(own)).collect();
        let autopilot = autopilot.then(|| Driver::new(driver_style(&spec, brains[0], 1.0)));
        Race {
            raw: bases.clone(),
            tiers: vec![Tier::Normal; n + 1],
            brains,
            lineup,
            races,
            rivals,
            level,
            box_at,
            stick_ahead: false,
            look_back: false,
            polish: Polish::fresh(),
            bases,
            my_polish: 1.0,
            reported: false,
            clean_world: false,
            arms,
            harm: Vec::new(),
            slots: Vec::new(),
            reload: Vec::new(),
            targets: Vec::new(),
            held_t: Vec::new(),
            trigger: false,
            trigger_was: false,
            pick: 0,
            stick_back: false,
            draft: Vec::new(),
            gas: false,
            gas_from: None,
            bog: 0.0,
            map,
            times: Vec::new(),
            counted: 0,
            lap_start: 0.0,
            wrong: 0.0,
            wrong_turn: 0.0,
            auto_unstick: 0.0,
            beeped: 0,
            sectors: 0,
            best_split: vec![f32::MAX; split_at.len()],
            split_at,
            checkpoints,
            checks: Vec::new(),
            check_every,
            split: None,
            my_pos: Vec2::ZERO,
            track,
            laps,
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
            autopilot,
        }
    }

    /// Let a bot drive the player's car, as the rivals drive (headless runs).
    pub fn autopilot(&mut self) {
        self.autopilot = Some(Driver::new(driver_style(&self.spec, self.brains[0], 1.0)));
    }

    /// Each kart's finish, for a headless lineup (`KART_BRAINS`): place,
    /// kart, brain, tier, its kart's name and its time (or distance run).
    pub fn result_lines(&self) -> Vec<String> {
        self.standings()
            .iter()
            .enumerate()
            .map(|(place, &(who, p, fin))| {
                let brain = self.lineup.as_ref().and_then(|l| l.names.get(who)).map_or("default", |n| n.as_str());
                let kart = if who == 0 { &self.spec.name } else { &self.bots[who - 1].car.spec.name };
                let time = fin.map_or(format!("dnf {p:.0}m"), |t| format!("{t:.2}"));
                format!("RESULT {} {who} {brain} {} {kart} {time}", place + 1, self.tiers.get(who).map_or('N', |t| t.letter()))
            })
            .collect()
    }

    /// Whether every kart is home, or the time after the player's finish is up.
    pub fn all_home(&self) -> bool {
        matches!(self.phase, Phase::Done(t) if t > RUN_IN) || (self.me.finish.is_some() && self.bots.iter().all(|b| b.home()))
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
                let name = fight::name(*who);
                match fin {
                    Some(t) => format!("{name} {}", clock(*t, 1)),
                    None if *who > 0 && self.bots[who - 1].ghost => format!("{name} {:.0}m~", p),
                    None => format!("{name} {:.0}m", p),
                }
            })
            .collect();
        let off: Vec<String> = std::iter::once(&self.me).chain(self.bots.iter().map(|b| &b.laps)).map(|l| format!("{:.0}", l.place.map_or(0.0, |p| p.off))).collect();
        // What each kart carries, and * for one spun out.
        let items: Vec<String> = (0..self.bots.len() + 1)
            .map(|k| {
                let item = match self.slots.get(k) {
                    Some(Slot::Have(i, n)) => format!("{}{n}", &i.name()[..1]),
                    _ => "-".into(),
                };
                let spun = if self.harm.get(k).is_some_and(|h| h.spin > 0.0) { "*" } else { "" };
                let hurt = self.harm.get(k).map_or(0.0, |h| h.damage * 100.0);
                let hurt = if hurt > 0.05 { format!("d{hurt:.1}") } else { String::new() };
                let shield = if self.harm.get(k).is_some_and(|h| h.shield > 0.0) { "S" } else { "" };
                format!("{item}{spun}{shield}{hurt}")
            })
            .collect();
        format!("{phase}  {}  lap {}/{}  {}  off {}  items {}  {:.0} km/h", clock(self.clock, 1), self.me.lap(self.track.length, self.laps), self.laps, order.join(", "), off.join("/"), items.join("/"), self.speed * 3.6)
    }

    /// Grid slot `i` (0 at the front): behind the line, two abreast.
    fn slot(&self, i: usize) -> (Vec2, Vec2, f32, f32) {
        let s = -6.0 * (i as f32 + 1.0);
        let lane = if i.is_multiple_of(2) { -2.2 } else { 2.2 };
        let (p, d) = self.track.at(s);
        (p + Vec2::new(-d.y, d.x) * lane, d, lane, s)
    }

    /// Kart `k`'s laps: 0 the player's, then the rivals'.
    fn laps_of(&self, k: usize) -> &Laps {
        if k == 0 {
            &self.me
        } else {
            &self.bots[k - 1].laps
        }
    }

    /// The player's best lap so far.
    fn best_lap(&self) -> Option<f32> {
        self.times.iter().copied().reduce(f32::min)
    }

    /// A new race's karts onto the grid: the rivals in front, the player at
    /// the back, and the tiers dealt.
    fn place_all(&mut self, ride: &mut Ride, player: &mut Player, store: &Store) {
        let length = self.track.length;
        let n = self.bots.len();
        // The tiers for this race: a lineup's own, or a third of the rivals
        // good and a third bad, shuffled each race. The player's kart is as
        // it is.
        self.races += 1;
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64) ^ self.races.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.tiers = match self.lineup.as_ref().and_then(|l| l.tiers.clone()) {
            Some(t) => (0..=n).map(|k| t.get(k).copied().unwrap_or(Tier::Normal)).collect(),
            None => std::iter::once(Tier::Normal).chain(Tier::deal(n, seed)).collect(),
        };
        self.bases = self.raw.iter().zip(&self.tiers).map(|(&(top, power, force), t)| (top * t.share(), power * t.share(), force * t.share())).collect();
        for i in 0..n {
            let (p, d, lane, s) = self.slot(i);
            let at = Vec3::new(p.x, ground(store, p, player.pos.y), p.y);
            let b = &mut self.bots[i];
            b.car = Vehicle::new(b.car.spec.clone(), at, yaw_along(d));
            b.driver.lane = lane;
            b.laps = Laps::start(s, length);
        }
        let (p, d, _, s) = self.slot(n);
        let at = Vec3::new(p.x, ground(store, p, player.pos.y), p.y);
        ride.clear_race();
        ride.race_car(player, self.spec.clone(), at, yaw_along(d));
        self.me = Laps::start(s, length);
        self.clean_world = true;
        self.placed = true;
    }

    /// One fixed step, after the player's own (Game::fixed_step).
    #[allow(clippy::needless_range_loop)] // each bot reads the others by index
    pub fn tick(&mut self, ride: &mut Ride, player: &mut Player, store: &Store, sea: f32, slopes: bool, dt: f32) {
        // A player found out of the car starts the race again.
        if self.placed && ride.car().is_none() {
            self.restart();
        }
        if !self.placed {
            self.place_all(ride, player, store);
        }
        if let Phase::Grid(t) = &mut self.phase {
            *t -= dt;
            ride.hold = true;
            let go = *t <= 0.0;
            // A beep at 3, 2 and 1, and a higher one for GO.
            let left = t.max(0.0).ceil() as u32;
            if left <= 3 && left != self.beeped {
                self.beeped = left;
                crate::sound::emit(crate::sound::Noise::ear(crate::sound::Kind::Beep(left == 0), 0.8));
            }
            for b in &mut self.bots {
                b.car.step(&Controls::default(), store, sea, slopes, dt);
            }
            // The rocket start: when the throttle went down and stayed down.
            let t_now = *t;
            if self.gas {
                self.gas_from.get_or_insert(t_now);
            } else {
                self.gas_from = None;
            }
            if go {
                self.phase = Phase::Racing;
                self.note = Some("GO".into());
                ride.hold = false;
                self.rocket_start(ride);
            }
            self.watch(ride);
            return;
        }
        ride.look_back = self.look_back && self.me.finish.is_none();
        self.clock += dt;
        // Bogged down after jumping the start: held a moment longer.
        if self.bog > 0.0 {
            self.bog -= dt;
            ride.hold = self.bog > 0.0;
        }
        let length = self.track.length;
        let total = length * self.laps as f32;

        // Where everyone is, then the rivals' moves.
        let my_place = ride.car().map(|c| self.track.place(c.pos(), self.me.place));
        let places: Vec<Place> = self.bots.iter().map(|b| self.track.place(b.car.pos(), b.laps.place)).collect();
        let my_speed = ride.car().map_or(0.0, |c| c.v_fwd());
        let spun: Vec<bool> = self.harm.iter().map(|h| h.spin > 0.0).collect();
        // Goo and blocks on the road, seen by the drivers as stopped cars, so
        // they move over to pass them as they would a kart.
        let hazards: Vec<Rival> = self.arms.hazards().into_iter().map(|h| Rival { place: self.track.place(h, None), speed: 0.0 }).collect();
        for i in 0..self.bots.len() {
            if self.bots[i].home() {
                continue;
            }
            let mut rivals: Vec<Rival> = (0..self.bots.len()).filter(|&j| j != i && !self.bots[j].home()).map(|j| Rival { place: places[j], speed: self.bots[j].car.v_fwd() }).collect();
            if let (Some(p), None) = (my_place, self.me.finish) {
                rivals.push(Rival { place: p, speed: my_speed });
            }
            rivals.extend(hazards.iter().copied());
            let goal = self.box_goal(i + 1, places[i].s);
            let b = &mut self.bots[i];
            b.driver.goal = goal;
            let mut c = b.driver.race(&b.car, &self.track, &rivals);
            if let Some(back) = unstick(&mut b.unstick, b.laps.slow, places[i].off, dt) {
                c = back;
            }
            // Spun out: no controls until it is over.
            if spun.get(i + 1).copied().unwrap_or(false) {
                c = Controls::default();
            }
            b.car.step(&c, store, sea, slopes, dt);
            if b.car.waiting {
                // Out where the world isn't loaded: carried round the
                // centreline at the speed its driver wants, out of sight.
                let v = b.driver.want.min(b.car.spec.top_speed).max(8.0);
                let s = places[i].s + v * dt;
                let (p, d) = self.track.at(s);
                // At the road's height, where known, so it is placed on its
                // own stretch at a crossing and up a climb.
                let y = self.track.height_at(s).map_or(b.car.pos().y, |h| h + 0.4);
                b.car.b.pos = Vec3::new(p.x, y, p.y);
                b.car.b.rot = glam::Quat::from_rotation_y(yaw_along(d));
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
        let goal = my_place.and_then(|p| self.box_goal(0, p.s));
        if let (Some(d), Some(car), None) = (self.autopilot.as_mut(), ride.car(), self.me.finish) {
            d.goal = goal;
            let mut rivals: Vec<Rival> = (0..self.bots.len()).filter(|&j| !self.bots[j].home()).map(|j| Rival { place: places[j], speed: self.bots[j].car.v_fwd() }).collect();
            rivals.extend(hazards.iter().copied());
            let mut c = d.race(car, &self.track, &rivals);
            if let (Some(back), Some(p)) = (unstick(&mut self.auto_unstick, self.me.slow, 0.0, dt), my_place) {
                c = Controls { steer: p.off.signum(), ..back };
            }
            ride.auto = Some(c);
        }

        // Cars that touch push each other. A rival carried round out of the
        // loaded world (`ghost`) touches nobody, nor does a kart home.
        let mut hardest = 0.0f32;
        if let (Some(mine), None) = (ride.car_mut(), self.me.finish) {
            for b in self.bots.iter_mut().filter(|b| !b.ghost && !b.home() && b.laps.ghost <= 0.0 && self.me.ghost <= 0.0) {
                let (hard, at) = meet(mine, &mut b.car, store);
                if hard > 0.0 {
                    hardest = hardest.max(hard);
                    knock_sound(at, hard);
                }
            }
        }
        for i in 0..self.bots.len() {
            for j in i + 1..self.bots.len() {
                let (a, b) = self.bots.split_at_mut(j);
                if a[i].ghost || b[0].ghost || a[i].home() || b[0].home() || a[i].laps.ghost > 0.0 || b[0].laps.ghost > 0.0 {
                    continue;
                }
                let (hard, at) = meet(&mut a[i].car, &mut b[0].car, store);
                if hard > 2.0 && (at - player.pos).length() < 60.0 {
                    knock_sound(at, hard);
                }
            }
        }
        if hardest > 3.0 {
            ride.jolt(hardest);
        }

        // Boost pads under anyone on the ground.
        self.pads(ride, my_place, &places);

        // The weapons: where everyone is, the boxes and what is used, and
        // what the hits did.
        self.poses(ride);
        self.fight(ride, player, dt);
        self.harmed(ride, dt);
        self.slipstream(ride, dt);
        self.polished(ride);

        // Laps, finishes, and cars put back on the track.
        for i in 0..self.bots.len() {
            let b = &mut self.bots[i];
            if b.laps.finish.is_some() {
                continue;
            }
            let width = self.track.width_at(places[i].s);
            if self.harm.get(i + 1).is_none_or(|h| h.over <= 0.0) {
                verge(&mut b.car, places[i].off, width, dt);
            }
            let road_y = self.track.height_at(places[i].s);
            let lost = b.laps.follow(places[i], &b.car, length, width, road_y, &self.checkpoints, dt);
            if b.laps.progress >= total {
                b.laps.finish = Some(self.clock);
            } else if lost || b.laps.slow > BOT_GIVE_UP {
                // Far out of bounds, or (a bot only) stuck past backing out.
                put_back(&mut b.car, &self.track, b.laps.road_s, store);
                b.laps.clear();
                b.unstick = 0.0;
            }
        }
        if let (Some(p), None) = (my_place, self.me.finish) {
            let width = self.track.width_at(p.s);
            if let (Some(car), true) = (ride.car_mut(), self.harm.first().is_none_or(|h| h.over <= 0.0)) {
                verge(car, p.off, width, dt);
            }
            let road_y = self.track.height_at(p.s);
            let lost = ride.car().is_some_and(|c| self.me.follow(p, c, length, width, road_y, &self.checkpoints, dt));
            // A wheel on the kerb at speed: the controller buzzes.
            if ride.car().is_some_and(|c| c.grounded > 0 && c.b.vel.length() > 5.0) && (p.off.abs() - width * 0.5).abs() < 0.6 {
                ride.rumble = ride.rumble.max(0.12);
            }
            // A lap done: each counted once, however often the line is
            // crossed back and forth. The first runs from the start.
            let done = (self.me.progress / length).floor() as i32;
            if done > self.counted {
                self.counted = done;
                let t = self.clock - self.lap_start;
                if let Some(best) = self.best_lap() {
                    self.split = Some((t - best, self.clock));
                }
                self.times.push(t);
                self.lap_start = self.clock;
                self.sectors = 0;
                let next = done as u32 + 1;
                if next <= self.laps {
                    crate::sound::emit(crate::sound::Noise::ear(crate::sound::Kind::Lap(next == self.laps), 0.7));
                }
                if next < self.laps {
                    self.note = Some(format!("LAP {next}/{}   {}", self.laps, clock(t, 1)));
                } else if next == self.laps {
                    self.note = Some(format!("FINAL LAP   {}", clock(t, 1)));
                }
            }
            // The sector lines (`split_at`): the time to each against the
            // best so far.
            let in_lap = self.me.progress - self.counted.max(0) as f32 * length;
            if self.counted >= 0 && (self.sectors as usize) < self.split_at.len() && in_lap >= self.split_at[self.sectors as usize] {
                let k = self.sectors as usize;
                let t = self.clock - self.lap_start;
                if self.best_split[k] < f32::MAX {
                    self.split = Some((t - self.best_split[k], self.clock));
                }
                self.best_split[k] = self.best_split[k].min(t);
                self.sectors += 1;
            }
            // Going the wrong way round: back along the track, or facing
            // back and not getting anywhere forward.
            if let Some(car) = ride.car() {
                let (_, d) = self.track.at(p.s);
                let v = Vec2::new(car.b.vel.x, car.b.vel.z).dot(d);
                let f = car.forward();
                let f2 = Vec2::new(f.x, f.z).normalize_or_zero();
                let facing = f2.dot(d) < -0.3;
                self.wrong = if v < -2.0 || (facing && v < 1.0) { self.wrong + dt } else { 0.0 };
                // The shorter way round to face along the track: in (x, z),
                // z south, a negative perp_dot is a turn to the left.
                self.wrong_turn = if f2.perp_dot(d) < 0.0 { -1.0 } else { 1.0 };
            }
            if self.me.progress >= total {
                self.me.finish = Some(self.clock);
                let pos = self.position();
                self.note = Some(format!("FINISHED {}   {}", ordinal(pos), clock(self.clock, 1)));
                crate::sound::emit(crate::sound::Noise::ear(crate::sound::Kind::Finish, 0.8));
                self.phase = Phase::Done(0.0);
                // Home: the kart is gone from the road and the player
                // watches the rest come in over the results.
                ride.hold = true;
                ride.hide = true;
                ride.auto = None;
            } else if lost {
                let at = self.me.road_s;
                if let Some(car) = ride.car_mut() {
                    put_back(car, &self.track, at, store);
                }
                self.me.clear();
                self.note = Some("BACK ON THE TRACK".into());
            }
        }
        if self.me.finish.is_some() {
            // The camera rolls to a stop past the line, where the rest come
            // home.
            if let Some(car) = ride.car_mut() {
                car.b.vel *= (1.0 - HOME_STOP * dt).max(0.0);
            }
        }
        if let Phase::Done(t) = &mut self.phase {
            *t += dt;
            if self.again {
                self.restart();
                return;
            }
        }
        self.again = false;
        self.checkpoints();
        self.watch(ride);
        self.engines(player);
    }

    /// Where kart `k`, `s` metres round, should drive for an item box: with
    /// nothing carried and a brain that looks for them, the lane of the
    /// nearest box standing within its reach ahead.
    fn box_goal(&self, k: usize, s: f32) -> Option<f32> {
        let reach = self.brains.get(k)?.boxes;
        if reach <= 0.0 || self.slots.get(k).is_some_and(|sl| *sl != Slot::Empty) {
            return None;
        }
        let len = self.track.length;
        self.arms
            .boxes
            .iter()
            .zip(&self.box_at)
            .filter(|(b, _)| b.back <= 0.5)
            .map(|(_, &(bs, off))| ((bs - s).rem_euclid(len), off))
            .filter(|&(d, _)| (4.0..reach).contains(&d))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, off)| off)
    }

    /// A kart on the ground on a boost pad gets the kick a full drift gives,
    /// once for each pad it drives over.
    fn pads(&mut self, ride: &mut Ride, my_place: Option<Place>, places: &[Place]) {
        if self.track.pads.is_empty() {
            return;
        }
        let kick = |car: &mut Vehicle| {
            if car.grounded > 0 && car.kick_t < 0.2 {
                boost_kick(car, PAD_SECS);
            }
        };
        if let (Some(p), Some(car)) = (my_place, ride.car_mut()) {
            if self.track.on_pad(p) {
                kick(car);
            }
        }
        for (b, p) in self.bots.iter_mut().zip(places) {
            if !b.ghost && self.track.on_pad(*p) {
                kick(&mut b.car);
            }
        }
    }

    /// The player's car, copied for the HUD.
    fn watch(&mut self, ride: &Ride) {
        if let Some(c) = ride.car() {
            self.my_pos = Vec2::new(c.pos().x, c.pos().z);
            self.speed = c.v_fwd();
            self.tier = c.drift_tier();
            self.boosting = c.kick_t > 0.0;
        }
    }

    /// The rivals' engines, each a held voice at its car.
    fn engines(&self, player: &Player) {
        for (i, b) in self.bots.iter().enumerate().filter(|(_, b)| !b.home()) {
            if (b.car.pos() - player.pos).length() < 120.0 {
                crate::sound::emit(crate::sound::Noise::engine(b.car.pos(), b.car.rpm, i as u32));
            }
        }
    }

    /// The player's finish, once: laps, the race time, the best lap and the
    /// kart, for the records.
    pub fn result(&mut self) -> Option<(u32, f32, Option<f32>, String)> {
        let race = self.me.finish?;
        // A bot driving the player's kart (headless runs) sets no records.
        if std::mem::replace(&mut self.reported, true) || self.autopilot.is_some() {
            return None;
        }
        Some((self.laps, race, self.best_lap(), self.spec.name.clone()))
    }

    /// The finish line on the HUD says what the race beat.
    pub fn beat(&mut self, b: records::Beaten) {
        let what = match (b.race, b.lap) {
            (true, true) => "COURSE AND LAP RECORD",
            (true, false) => "COURSE RECORD",
            (false, true) => "LAP RECORD",
            (false, false) => return,
        };
        let was = self.note.take().unwrap_or_default();
        self.note = Some(if was.is_empty() { what.to_string() } else { format!("{was}   {what}") });
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

impl Race {
    /// Every kart polishes the tiles under its wheels, and goes as fast as
    /// the polish under them lets it: its own top speed, power and drive
    /// force scaled by it (polish.rs). A rival carried round out of sight
    /// polishes along where it is carried, its wheels on the road.
    fn polished(&mut self, ride: &mut Ride) {
        let n = self.bots.len() + 1;
        for k in 0..n {
            let ghost = k > 0 && self.bots[k - 1].ghost;
            let Some(&(top, power, force)) = self.bases.get(k) else { continue };
            // Damage takes its share off whatever the polish gives.
            let hurt = 1.0 - self.harm.get(k).map_or(0.0, |h| h.damage);
            let Some(car) = fight::kart(&mut self.bots, ride, k) else { continue };
            let wheels = polish_wheels(car, ghost);
            self.polish.roll(k, &wheels);
            let f = self.polish.factor(&wheels) * hurt;
            car.spec.top_speed = top * f;
            car.spec.power = power * f;
            car.spec.force = force * f;
            if k == 0 {
                self.my_polish = f / hurt;
            }
        }
    }

    /// The polish marks laid since last asked, for `Game`'s renderer.
    pub fn polish_marks(&mut self) -> (bool, Vec<crate::render::polish::Mark>) {
        self.polish.take_marks()
    }

    /// Each kart's checkpoints passed since last step, timed.
    fn checkpoints(&mut self) {
        if !matches!(self.phase, Phase::Racing | Phase::Done(_)) {
            return;
        }
        let n = self.bots.len() + 1;
        self.checks.resize(n, Vec::new());
        for k in 0..n {
            let l = self.laps_of(k);
            if l.finish.is_some() {
                continue;
            }
            let passed = (l.progress / self.check_every).floor().max(0.0) as usize;
            while self.checks[k].len() < passed {
                self.checks[k].push(self.clock);
            }
        }
    }

    /// Seconds kart `b` passed the last checkpoint both have passed after
    /// kart `a` did: how far `b` trails `a`, as of then.
    pub(super) fn check_gap(&self, a: usize, b: usize) -> Option<f32> {
        let (ca, cb) = (self.checks.get(a)?, self.checks.get(b)?);
        let c = ca.len().min(cb.len()).checked_sub(1)?;
        Some(cb[c] - ca[c])
    }

}

/// Where `car`'s wheels meet the road: each wheel that is down, or all four
/// for a rival carried out of sight (`ghost`), which is held at the road's
/// height and has no wheels touching anything.
fn polish_wheels(car: &Vehicle, ghost: bool) -> Vec<Vec3> {
    let down = car.up() * -1.0;
    (0..4)
        .filter(|&i| ghost || car.wheels[i].down)
        .map(|i| {
            let w = &car.wheels[i];
            let mount = car.b.pos + car.b.rot * car.spec.mount(i);
            if ghost {
                Vec3::new(mount.x, car.b.pos.y - 0.4, mount.z)
            } else {
                mount + down * (w.t + car.spec.wheel_radius)
            }
        })
        .collect()
}

/// Checkpoints about this many metres apart round the course (`checks`).
const CHECK_M: f32 = 100.0;

/// A bot stuck this long (under 1 m/s) backs out...
const STUCK_AFTER: f32 = 1.5;
/// ...for this long...
const BACK_SECS: f32 = 1.2;
/// ...and one still stuck after this long is put back on the track: a bot
/// only, so a race always finishes. The player's car only ever goes back
/// when far out of bounds.
const BOT_GIVE_UP: f32 = 10.0;

/// The driver for a car of `spec` with `brain`, at `level` of the grip and
/// braking it trusts: a kart's fitted to its kart, any other car's the
/// style bred for racing.
fn driver_style(spec: &Spec, brain: Brain, level: f32) -> Style {
    let base = if spec.handling == Handling::Kart { Style::for_kart(spec) } else { Style::racer() };
    let st = brain.style(base);
    Style { corner_g: st.corner_g * level, brake_g: st.brake_g * level, ..st }
}

/// A driver backing out of where it is stuck: reversing, steered so the
/// nose swings back toward the road (a car `off` metres right of the
/// centreline reverses steering right, which turns its nose left). None
/// while it isn't. `left` is the time left backing out.
fn unstick(left: &mut f32, slow: f32, off: f32, dt: f32) -> Option<Controls> {
    if *left <= 0.0 && slow > STUCK_AFTER {
        *left = BACK_SECS;
    }
    if *left <= 0.0 {
        return None;
    }
    *left -= dt;
    Some(Controls { brake: 1.0, steer: off.signum(), ..Default::default() })
}

/// Two cars touching (`vehicle::meet`): how hard they met, m/s, and where.
fn meet(a: &mut Vehicle, b: &mut Vehicle, store: &Store) -> (f32, Vec3) {
    let at = (a.pos() + b.pos()) * 0.5;
    (crate::vehicle::meet(a, b, Some(store)), at)
}

/// The kick a full drift gives, and `secs` held at the boosted top speed: a
/// boost pad's, and the BOOST item's.
fn boost_kick(car: &mut Vehicle, secs: f32) {
    let fwd = car.forward();
    car.b.vel += fwd * car.spec.drift_boost.max(3.0) * 2.0;
    car.kick_t = secs;
}

/// The knock of two karts meeting, louder the harder.
fn knock_sound(at: Vec3, hard: f32) {
    if hard > 1.5 {
        crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Clank, at, (0.2 + hard / 12.0).min(1.0)));
    }
}

impl Activity for Race {
    fn id(&self) -> ModeId {
        ModeId::Race
    }

    fn frame(&mut self, _cx: &mut ActivityCtx, input: &MoveInput, actions: &mut crate::game::Actions, _dt: f32) {
        self.gas = input.forward || input.throttle > 0.5;
        // The left stick or the drive keys held back fire behind (S also
        // brakes, so a tap of it with the button is enough); the stick held
        // forward throws goo or a block ahead. The keys can't throw ahead:
        // W is held to drive. Alt or the right stick held looks behind, and
        // what is fired then goes behind too.
        self.stick_back = input.stick.y < -0.5 || (!input.pad && input.back);
        self.stick_ahead = input.pad && input.stick.y > 0.5;
        self.look_back = input.look;
        self.again |= std::mem::take(&mut actions.accept);
        // The left button uses what the kart carries; with KART_ARMS=all the
        // digits pick the weapon.
        self.trigger = std::mem::take(&mut actions.dig);
        actions.place = false;
        if let Some(k) = actions.select.take() {
            self.pick = k.saturating_sub(1) % Item::WEAPONS.len();
        }
        // In a race you stay in the car.
        actions.vehicle_toggle = false;
        actions.bike_toggle = false;
        actions.kayak_toggle = false;
        actions.glide = false;
    }

    /// Everything in the air, with the context's store and creatures, which
    /// `tick` doesn't have.
    fn step(&mut self, cx: ActivityCtx, _input: &MoveInput) {
        // The step the world is wiped in lays nothing: the wipe comes after
        // this step's edits and would take them with it.
        if self.clean_world {
            return;
        }
        self.arms.step(cx.store, cx.ents, cx.models, &self.track, cx.edits, crate::game::STEP);
    }

    fn wants_world_clean(&mut self) -> bool {
        std::mem::take(&mut self.clean_world)
    }

    fn overlay(&self, player: &Player, out: &mut crate::render::overlay::Overlay) {
        self.arms.combat.draw(player.eye(1.0), out);
    }

    fn race_mut(&mut self) -> Option<&mut Race> {
        Some(self)
    }

    fn each_race_kart(&self, f: &mut dyn FnMut(&Vehicle)) {
        for b in self.bots.iter().filter(|b| !b.ghost && !b.home()) {
            f(&b.car);
        }
    }

    fn note(&mut self) -> Option<String> {
        self.note.take()
    }

    fn instances(&self, player: &Player, alpha: f32, weapons: usize, models: &[crate::models::Model], out: &mut Vec<(usize, crate::models::Instance)>) {
        self.arms_instances(player, alpha, weapons, models, out);
        for (i, b) in self.bots.iter().enumerate() {
            if b.home() {
                continue;
            }
            let from = out.len();
            crate::vehicle::draw::instances_in(&b.car, alpha, self.base, models, true, false, i + 1, out);
            // Just put back on the track: drawn faint while it passes
            // through the others.
            if b.laps.ghost > 0.0 {
                for (_, inst) in &mut out[from..] {
                    *inst = inst.glass(0.45);
                }
            }
            if self.harm.get(i + 1).is_some_and(|h| h.flashing()) {
                crate::vehicle::draw::flash(&mut out[from..]);
            }
        }
    }

    fn hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32, _fov: f32) {
        self.draw_hud(hud, w, h, s);
    }
}
