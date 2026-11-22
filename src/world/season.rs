//! The seasons: which part of the year the whole world is in, turning over on
//! the game clock about every two days.
//!
//! Four seasons (`Season`), of which a world has one or more: a tropical world
//! names summer alone and never turns, a temperate one names all four. Summer
//! is the look the game was tuned in, so its row of numbers is all ones and
//! zeroes and a summer world is the game as it was.
//!
//! Two kinds of thing read a season, and they read it differently.
//!
//! The numbers that are read every frame -- the palette, the weather it
//! favours, how much of a species is abroad -- slide from one season's row
//! (`Phase`) to the next over the first half of the new season, so nothing
//! about the light or the sky ever snaps.
//!
//! The things baked into the mesh -- leaf colour, bare branches, blossom --
//! cannot slide, because a tree's leaves are voxels. Instead each tree turns
//! on its own: `Turn` holds the season being left, the one arriving and how
//! far through the change the world is, and every tree compares its own seed
//! against that (`Turn::wears`). The world is regenerated a few times over the
//! change, so autumn walks through a forest tree by tree instead of arriving
//! all at once. `flora::dress` is what puts a tree in the clothes `Turn` gives
//! it.
//!
//! Lying snow belongs here too, because what makes it lie is the season and
//! what makes it fall is the weather: it builds while snow falls and melts at
//! a rate the season sets, so a shower in autumn is gone by evening and the
//! same shower in winter stays and deepens.

use crate::world::noise::Rng;
use crate::world::weather::{Build, Kind, KINDS};
use serde::{Deserialize, Serialize};

/// A part of the year.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Season {
    Spring,
    /// The long green afternoon the game was tuned in, and the baseline every
    /// other season is stated against.
    #[default]
    Summer,
    Autumn,
    Winter,
}

/// Every season, in the order the year goes round.
pub const SEASONS: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

/// How long a season lasts, in game hours: two days, which is about fifty
/// minutes of real time at the clock's own pace.
pub const SEASON_H: f32 = 48.0;

/// The hour a turn of the year prefers to land on, and how far either side it
/// wanders: the small hours, when the leaves can change without being watched.
const NIGHT: (f32, f32) = (2.0, 1.0);

/// How often a turn is pulled to the night rather than left where it falls.
const PULL: f32 = 0.8;

/// How far a turn left to itself wanders from the two-day mark, game hours.
const DRIFT_H: f32 = 10.0;

/// How long the leaves take to turn once a season has, game hours: half the
/// season, so autumn is still arriving a day into it.
pub const TURN_H: f32 = 24.0;

/// How many steps the turn is cut into. Every tree in a step changes at once
/// and the world is regenerated once for it, so this is the number of sweeps
/// a turn of the year costs.
pub const STEPS: u32 = 4;

/// What the trees are wearing, which is baked into the mesh.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Leaf {
    /// Full green leaf.
    Green,
    /// Green leaf and whatever blossom the plant has.
    Blossom,
    /// Its autumn colour, where it has one.
    Autumn,
    /// No leaf at all, where it has an autumn colour to lose. Anything
    /// evergreen keeps its own.
    Bare,
}

impl Season {
    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }

    pub fn parse(s: &str) -> Option<Season> {
        SEASONS.into_iter().find(|x| x.name() == s)
    }

    /// The next season round the year, whether or not a world has it.
    pub fn next(self) -> Season {
        let i = SEASONS.iter().position(|x| *x == self).unwrap_or(0);
        SEASONS[(i + 1) % SEASONS.len()]
    }

    /// What the trees wear in it.
    pub fn leaf(self) -> Leaf {
        match self {
            Season::Spring => Leaf::Blossom,
            Season::Summer => Leaf::Green,
            Season::Autumn => Leaf::Autumn,
            Season::Winter => Leaf::Bare,
        }
    }

    /// This season as the numbers everything else reads.
    pub fn phase(self) -> Phase {
        match self {
            // New growth: a yellower green, the colour pushed a little, and
            // the wettest, windiest weather of the year.
            Season::Spring => Phase {
                green: [1.02, 1.07, 0.93],
                straw: 0.0,
                saturation: 1.04,
                brightness: 1.02,
                warmth: 0.02,
                melt: 0.25,
                weather: bias(&[(Kind::Rain, 1.4), (Kind::Clouds, 1.2), (Kind::Fog, 1.1), (Kind::Snow, 0.3), (Kind::Heat, 0.2)]),
                abroad: [1.0, 1.0, 0.9, 1.0],
                declination: 4.0,
            },
            // Summer is the game as it was: every multiplier 1, every
            // addition 0, and the world's own climate untouched.
            Season::Summer => Phase {
                green: [1.0, 1.0, 1.0],
                straw: 0.0,
                saturation: 1.0,
                brightness: 1.0,
                warmth: 0.0,
                melt: 0.8,
                weather: [1.0; KINDS.len()],
                abroad: [1.0; 4],
                declination: 14.0,
            },
            // The turn: the grass goes over, the light goes gold, and the
            // first fogs of the year sit in the hollows.
            Season::Autumn => Phase {
                green: [1.0, 0.93, 0.82],
                straw: 0.45,
                saturation: 0.98,
                brightness: 0.96,
                warmth: 0.12,
                melt: 0.3,
                weather: bias(&[(Kind::Fog, 1.6), (Kind::Rain, 1.3), (Kind::Clouds, 1.2), (Kind::Sun, 0.9), (Kind::Snow, 0.6), (Kind::Heat, 0.3)]),
                abroad: [0.95, 0.85, 0.6, 1.0],
                declination: -4.0,
            },
            // Winter: the colour out of everything, the snow lying where it
            // falls, and most of the small life gone.
            Season::Winter => Phase {
                green: [0.84, 0.86, 0.88],
                straw: 0.8,
                saturation: 0.82,
                brightness: 0.97,
                warmth: -0.2,
                melt: 0.02,
                weather: bias(&[
                    (Kind::Snow, 4.0),
                    (Kind::Clouds, 1.3),
                    (Kind::Fog, 1.4),
                    (Kind::Sun, 0.8),
                    (Kind::Rain, 0.5),
                    (Kind::Heat, 0.0),
                    (Kind::Storm, 0.15),
                ]),
                abroad: [0.7, 0.5, 0.1, 0.9],
                declination: -14.0,
            },
        }
    }
}

/// A row of weather multipliers from the kinds that are not 1.
fn bias(set: &[(Kind, f32)]) -> [f32; KINDS.len()] {
    let mut out = [1.0; KINDS.len()];
    for (k, v) in set {
        out[KINDS.iter().position(|x| x == k).unwrap_or(0)] = *v;
    }
    out
}

/// One season as the numbers read every frame. Through a turn of the year
/// this is the two rows mixed, so every value is continuous.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phase {
    /// Multiply on the colour of leaves, grass and plant sprites.
    pub green: [f32; 3],
    /// How far the grass has gone over to straw, 0 to 1.
    pub straw: f32,
    pub saturation: f32,
    pub brightness: f32,
    /// Added to the grade's warm/cool shift.
    pub warmth: f32,
    /// How fast lying snow goes, in depth a game hour.
    pub melt: f32,
    /// Multiplies the world's own weather weights, so a climate leans with
    /// the year without being replaced by it.
    pub weather: [f32; KINDS.len()],
    /// The share of each build of animal abroad (world/weather.rs `Build`,
    /// in its order): ground, flier, insect, swimmer.
    pub abroad: [f32; 4],
    /// The sun's declination, degrees (render/daylight.rs `SunPath`): high in
    /// summer, low in winter. The clock keeps every season's day the same
    /// real length, so this moves the sun's height and nothing else.
    pub declination: f32,
}

impl Default for Phase {
    fn default() -> Self {
        Season::Summer.phase()
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Phase {
    /// Two seasons, `t` of the way from the first to the second.
    pub fn blend(a: Phase, b: Phase, t: f32) -> Phase {
        let t = t.clamp(0.0, 1.0);
        let mut out = Phase {
            green: [mix(a.green[0], b.green[0], t), mix(a.green[1], b.green[1], t), mix(a.green[2], b.green[2], t)],
            straw: mix(a.straw, b.straw, t),
            saturation: mix(a.saturation, b.saturation, t),
            brightness: mix(a.brightness, b.brightness, t),
            warmth: mix(a.warmth, b.warmth, t),
            melt: mix(a.melt, b.melt, t),
            weather: a.weather,
            abroad: a.abroad,
            declination: mix(a.declination, b.declination, t),
        };
        for i in 0..out.weather.len() {
            out.weather[i] = mix(a.weather[i], b.weather[i], t);
        }
        for i in 0..out.abroad.len() {
            out.abroad[i] = mix(a.abroad[i], b.abroad[i], t);
        }
        out
    }

    /// The share of a build of animal abroad in this part of the year, before
    /// the hour and the weather have their say. A library entry overrides it
    /// for the one season it matters in (`library::Behaviour` `winter`).
    pub fn abroad(&self, b: Build, winter: Option<f32>) -> f32 {
        let i = match b {
            Build::Ground => 0,
            Build::Flier => 1,
            Build::Insect => 2,
            Build::Swimmer => 3,
        };
        match winter {
            // The entry states what is left of it at the depth of winter, and
            // the year walks between that and all of it.
            Some(w) => {
                let deep = (1.0 - self.abroad[i]) / (1.0 - Season::Winter.phase().abroad[i]).max(1e-3);
                mix(1.0, w, deep.clamp(0.0, 1.0))
            }
            None => self.abroad[i],
        }
    }

    /// The `season` field of the frame's globals: the green multiply and the straw.
    pub fn uniform(&self) -> [f32; 4] {
        [self.green[0], self.green[1], self.green[2], self.straw]
    }
}

/// Where the world is between two seasons, as the generator reads it: every
/// tree compares its own seed against `at` and wears one season or the other,
/// so the turn walks through a wood rather than crossing it in a line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Turn {
    pub from: Season,
    pub to: Season,
    /// Which step of the turn it is, 0 (nothing has changed) to `STEPS`
    /// (everything has).
    pub step: u32,
    /// How deep the lying snow is: 0 none, 1 a covering, 2 deep.
    pub snow: u32,
}

impl Default for Turn {
    /// Summer, settled, no snow: the world as it was before there were
    /// seasons.
    fn default() -> Self {
        Turn::settled(Season::Summer, 0)
    }
}

impl Turn {
    /// A world sitting squarely in one season.
    pub fn settled(s: Season, snow: u32) -> Turn {
        Turn { from: s, to: s, step: STEPS, snow }
    }

    /// Which season this tree is dressed for. Its own seed decides where in
    /// the turn it goes, so the same tree always turns at the same moment.
    pub fn wears(&self, seed: u32) -> Season {
        if self.step >= STEPS {
            return self.to;
        }
        if self.step == 0 {
            return self.from;
        }
        let u = crate::world::noise::unit(crate::world::noise::hash32(seed ^ 0x5ea5_0f11));
        if u < self.step as f32 / STEPS as f32 {
            self.to
        } else {
            self.from
        }
    }

    /// Whether anything baked differs between the two ends of this turn. When
    /// nothing does -- a world of one season, or two whose trees look the
    /// same -- a turn of the year costs no regeneration at all.
    pub fn moves(&self) -> bool {
        self.from.leaf() != self.to.leaf()
    }

    /// Packed small enough to hand the worker threads in one atomic.
    pub fn pack(&self) -> u32 {
        let idx = |s: Season| SEASONS.iter().position(|x| *x == s).unwrap_or(0) as u32;
        idx(self.from) | idx(self.to) << 4 | self.step.min(STEPS) << 8 | self.snow.min(3) << 16
    }

    pub fn unpack(v: u32) -> Turn {
        let at = |i: u32| SEASONS[(i as usize) % SEASONS.len()];
        Turn {
            from: at(v & 15),
            to: at((v >> 4) & 15),
            step: (v >> 8) & 255,
            snow: (v >> 16) & 3,
        }
    }
}

/// Which seasons a world has. A world that names none has summer alone, which
/// is every world the engine had before there were seasons.
#[derive(Clone, Copy, Debug)]
pub struct Calendar {
    has: [bool; SEASONS.len()],
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar::of(&[Season::Summer])
    }
}

impl Calendar {
    pub fn of(list: &[Season]) -> Calendar {
        let mut has = [false; SEASONS.len()];
        for s in list {
            has[SEASONS.iter().position(|x| x == s).unwrap_or(0)] = true;
        }
        if has.iter().all(|h| !h) {
            return Calendar::default();
        }
        Calendar { has }
    }

    /// A world's own list by name, ignoring anything nothing is called.
    pub fn from_names(names: &[String]) -> Calendar {
        Calendar::of(&names.iter().filter_map(|n| Season::parse(n)).collect::<Vec<_>>())
    }

    /// Which names a world states that nothing is called, for `--validate`.
    pub fn unknown(names: &[String]) -> Vec<String> {
        names.iter().filter(|n| Season::parse(n).is_none()).cloned().collect()
    }

    pub fn has(&self, s: Season) -> bool {
        self.has[SEASONS.iter().position(|x| *x == s).unwrap_or(0)]
    }

    /// Every season this world has, in the order the year goes round.
    pub fn list(&self) -> Vec<Season> {
        SEASONS.into_iter().filter(|s| self.has(*s)).collect()
    }

    /// The season a new game opens in: the first the world has, going round
    /// from summer, which is the one a world with no calendar is in.
    pub fn first(&self) -> Season {
        let mut s = Season::Summer;
        for _ in 0..SEASONS.len() {
            if self.has(s) {
                return s;
            }
            s = s.next();
        }
        Season::Summer
    }

    /// The season after this one here: round the year to the next the world
    /// has, which in a world of one season is that season again.
    pub fn after(&self, s: Season) -> Season {
        let mut n = s.next();
        for _ in 0..SEASONS.len() {
            if self.has(n) {
                return n;
            }
            n = n.next();
        }
        s
    }
}

/// The world's year: the season it is in, the one it is leaving, how far
/// through the change it is, what day it is, and how much snow is lying.
#[derive(Clone, Debug)]
pub struct Year {
    /// The season it is in now. The name changes at the turn; the trees
    /// follow over the next half season.
    pub season: Season,
    /// The season it is coming out of.
    from: Season,
    /// Game hours since the turn.
    since: f32,
    /// Game hours until the next turn.
    left: f32,
    /// Days since the world opened, counted from the first midnight.
    pub day: u32,
    /// The hour the clock was at last time it was asked, to catch midnight.
    was: f32,
    /// Lying snow, 0 to about 1.5. The generator sees it in steps.
    lying: f32,
    /// Which step of lying snow the world is built for, so a flurry either
    /// side of the line does not rebuild it twice.
    snow_step: u32,
    /// Whether the year has had its first fall yet, which winter sees to.
    fell: bool,
    /// The hour of winter by which it ought to have snowed.
    first_fall: f32,
    calendar: Calendar,
    /// False once the season is pinned (the launch screen, `--season`): it
    /// stays where it is put.
    pub drifting: bool,
    /// False in a world whose climate never snows (Thornpan, Ribshore): its
    /// winter opens with bare ground. Set by `set_snows`.
    snows: bool,
    rng: Rng,
}

impl Default for Year {
    fn default() -> Self {
        Year::new(Season::Summer, Calendar::default(), 0, crate::world::clock::START, false)
    }
}

impl Year {
    /// A world opening in `season` at `hour`. It opens settled: every tree is
    /// already dressed for it, and the first turn is about three quarters of a
    /// season away, so a world opened in the afternoon is a quarter of the way
    /// through its season.
    pub fn new(season: Season, calendar: Calendar, seed: u32, hour: f32, drifting: bool) -> Year {
        let mut y = Year {
            season,
            from: season,
            since: TURN_H,
            left: 0.0,
            day: 0,
            was: hour,
            lying: 0.0,
            snow_step: 0,
            fell: false,
            first_fall: 0.0,
            calendar,
            drifting,
            snows: true,
            rng: Rng::new(seed ^ 0x5ea5_02e4u32),
        };
        y.left = y.draw(SEASON_H * 0.75, hour);
        y.first_fall = y.rng.unit() * SEASON_H * 0.4;
        y.settle_snow();
        y
    }

    /// Snow to match the season it has just been put in: a world that opens
    /// in winter opens in the middle of one, with the snow already down.
    fn settle_snow(&mut self) {
        let deep = self.snows && self.season == Season::Winter;
        self.lying = if deep { 1.0 } else { 0.0 };
        self.snow_step = if deep { 2 } else { 0 };
        self.fell = deep;
    }

    /// How long until the next turn, from a target of `target` hours away.
    /// Most turns are pulled to the small hours; the rest are left to wander
    /// either side of the mark, so a season can end at any time of day. The
    /// pull picks the night before or the night after in the proportion that
    /// leaves the average length exactly `target`.
    fn draw(&mut self, target: f32, hour: f32) -> f32 {
        if self.rng.unit() < PULL {
            let land = NIGHT.0 + self.rng.signed() * NIGHT.1;
            let at = (hour + target).rem_euclid(24.0);
            let after = (land - at).rem_euclid(24.0);
            let before = 24.0 - after;
            if self.rng.unit() < before / 24.0 {
                target + after
            } else {
                target - before
            }
        } else {
            target + self.rng.signed() * DRIFT_H
        }
        .max(6.0)
    }

    /// The numbers this moment of the year looks like: the two seasons mixed
    /// over the first half of the new one.
    pub fn phase(&self) -> Phase {
        if self.from == self.season || self.since >= TURN_H {
            return self.season.phase();
        }
        let t = (self.since / TURN_H).clamp(0.0, 1.0);
        Phase::blend(self.from.phase(), self.season.phase(), t * t * (3.0 - 2.0 * t))
    }

    /// What the generator needs: which seasons the trees are between, how far
    /// through, and how deep the snow is.
    pub fn turn(&self) -> Turn {
        Turn {
            from: self.from,
            to: self.season,
            step: if self.from == self.season {
                STEPS
            } else {
                (((self.since / TURN_H).clamp(0.0, 1.0) * STEPS as f32).floor() as u32).min(STEPS)
            },
            snow: self.snow_step,
        }
    }

    /// What to call it on screen.
    pub fn name(&self) -> &'static str {
        self.season.name()
    }

    /// How much snow is lying, 0 to about 1.5, for anything that wants it
    /// smooth rather than in steps (the hush of a snowy world, sound/bed.rs).
    pub fn lying(&self) -> f32 {
        self.lying
    }

    /// Carry the year forward by `hours` of game clock, with the clock now at
    /// `hour` and the weather doing `snow`. Returns the turn the world should
    /// be built for if that has changed, and nothing while it has not.
    pub fn advance(&mut self, hours: f32, hour: f32, snow: f32) -> Option<Turn> {
        if hours <= 0.0 {
            return None;
        }
        let was = self.turn();
        if hour < self.was {
            self.day += 1;
        }
        self.was = hour;
        self.since += hours;
        // Snow builds while it falls and goes at the season's own rate. The
        // steps the world is built for have a little hysteresis, so a shower
        // either side of a line does not rebuild it twice.
        let phase = self.phase();
        self.lying = (self.lying + snow * 0.4 * hours - phase.melt * hours).clamp(0.0, 1.5);
        if snow > 0.05 {
            self.fell = true;
        }
        self.snow_step = step_of(self.lying, self.snow_step);
        if self.drifting {
            self.left -= hours;
            if self.left <= 0.0 {
                let next = self.calendar.after(self.season);
                // A turn that lands mid-turn still starts from where the trees
                // actually are, so a run of short seasons never jumps.
                self.from = if self.since < TURN_H { self.from } else { self.season };
                self.season = next;
                self.since = 0.0;
                self.fell = false;
                self.first_fall = self.rng.unit() * SEASON_H * 0.4;
                self.left = self.draw(SEASON_H, hour);
            }
        }
        let now = self.turn();
        (now != was).then_some(now)
    }

    /// The weather the year wants next, if it wants one: winter sees to it
    /// that the snow starts somewhere in its first day, rather than leaving a
    /// whole winter to the dice.
    pub fn wants_weather(&mut self) -> Option<Kind> {
        if self.season == Season::Winter && !self.fell && self.since > self.first_fall {
            self.fell = true;
            return Some(Kind::Snow);
        }
        None
    }

    /// Put the year to a season by hand (the launch screen, `--season`): it
    /// arrives at once, fully dressed, and stays.
    /// Whether this world's climate has snow in it at all. Without it a
    /// winter is opened, or put by hand, with no snow lying.
    pub fn set_snows(&mut self, on: bool) {
        self.snows = on;
        self.settle_snow();
    }

    pub fn set(&mut self, s: Season) {
        self.season = s;
        self.from = s;
        self.since = TURN_H;
        self.drifting = false;
        // A winter put there by hand is the depth of it, snow and all: a
        // world asked for winter and shown bare ground is not winter.
        self.settle_snow();
    }

    /// Which seasons this world has, for the launch screen.
    pub fn calendar(&self) -> Calendar {
        self.calendar
    }
}

/// The step of lying snow `d` is, holding the step it is already at until it
/// is clear of the line (0.2 and 0.65 going up, 0.14 and 0.55 coming down).
fn step_of(d: f32, at: u32) -> u32 {
    let up = [0.2, 0.65];
    let down = [0.14, 0.55];
    let mut out = at.min(2);
    while out < 2 && d >= up[out as usize] {
        out += 1;
    }
    while out > 0 && d < down[(out - 1) as usize] {
        out -= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Summer is the game as it was: nothing it hands anything else moves a
    /// value, and a summer world never turns.
    #[test]
    fn summer_changes_nothing() {
        let p = Season::Summer.phase();
        assert_eq!(p.green, [1.0, 1.0, 1.0]);
        assert_eq!(p.straw, 0.0);
        assert_eq!(p.saturation, 1.0);
        assert_eq!(p.brightness, 1.0);
        assert_eq!(p.warmth, 0.0);
        assert_eq!(p.weather, [1.0; KINDS.len()]);
        assert_eq!(p.uniform(), [1.0, 1.0, 1.0, 0.0]);
        for b in [Build::Ground, Build::Flier, Build::Insect, Build::Swimmer] {
            assert_eq!(p.abroad(b, None), 1.0);
        }
        let mut y = Year::new(Season::Summer, Calendar::default(), 4, 15.0, true);
        for _ in 0..40_000 {
            y.advance(0.01, 15.0, 0.0);
            assert_eq!(y.season, Season::Summer);
            assert_eq!(y.phase(), Season::Summer.phase());
            assert!(!y.turn().moves());
        }
    }

    #[test]
    fn names_round_trip() {
        for s in SEASONS {
            assert_eq!(Season::parse(s.name()), Some(s));
        }
        assert_eq!(Season::parse("monsoon"), None);
        let mut s = Season::Spring;
        for _ in 0..SEASONS.len() {
            s = s.next();
        }
        assert_eq!(s, Season::Spring);
    }

    /// A world states the seasons it has and the year only visits those.
    #[test]
    fn a_calendar_rules_seasons_out() {
        let c = Calendar::from_names(&["summer".to_string(), "winter".to_string(), "monsoon".to_string()]);
        assert!(c.has(Season::Summer) && c.has(Season::Winter));
        assert!(!c.has(Season::Spring) && !c.has(Season::Autumn));
        assert_eq!(c.first(), Season::Summer);
        assert_eq!(c.after(Season::Summer), Season::Winter);
        assert_eq!(c.after(Season::Winter), Season::Summer);
        assert_eq!(Calendar::unknown(&["monsoon".to_string()]), vec!["monsoon".to_string()]);
        let mut y = Year::new(Season::Summer, c, 9, 15.0, true);
        for _ in 0..40_000 {
            y.advance(0.01, 15.0, 0.0);
            assert!(matches!(y.season, Season::Summer | Season::Winter), "got {}", y.name());
        }
    }

    /// Left alone the year goes round, every season turns up, and a season
    /// lasts about two days.
    #[test]
    fn the_year_goes_round() {
        let c = Calendar::of(&SEASONS);
        let mut y = Year::new(Season::Spring, c, 3, 15.0, true);
        let mut seen = std::collections::BTreeSet::new();
        let mut turns = 0;
        let mut was = y.season;
        let mut hour = 15.0f32;
        let mut lengths = Vec::new();
        let mut since = 0.0f32;
        for _ in 0..200_000 {
            hour = (hour + 0.01).rem_euclid(24.0);
            y.advance(0.01, hour, 0.0);
            since += 0.01;
            seen.insert(y.name());
            if y.season != was {
                turns += 1;
                was = y.season;
                lengths.push((since, hour));
                since = 0.0;
            }
        }
        assert_eq!(seen.len(), SEASONS.len(), "saw {seen:?}");
        assert!(turns > 30, "2000 game hours brought {turns} turns");
        // The first is short (a world opens part of the way in), so judge the rest.
        let rest: Vec<f32> = lengths.iter().skip(1).map(|(h, _)| *h).collect();
        let mean = rest.iter().sum::<f32>() / rest.len() as f32;
        assert!((mean - SEASON_H).abs() < 3.0, "a season averaged {mean} hours");
        // Most turns land in the small hours, but not all of them.
        let night = rest.len();
        let at_night = lengths.iter().skip(1).filter(|(_, h)| *h < 4.0 || *h > 22.0).count();
        assert!(at_night * 2 > night, "{at_night} of {night} turns landed at night");
        assert!(at_night < night, "every single turn landed at night");
    }

    /// The trees turn over the first half of the season, tree by tree, and
    /// the world is asked to rebuild once a step.
    #[test]
    fn the_leaves_turn_tree_by_tree() {
        let c = Calendar::of(&SEASONS);
        let mut y = Year::new(Season::Summer, c, 5, 15.0, true);
        // Settled: every tree wears the season.
        let t = y.turn();
        assert_eq!(t.step, STEPS);
        for seed in 0..200u32 {
            assert_eq!(t.wears(seed), Season::Summer);
        }
        // Run to the turn of the year.
        let mut hour = 15.0f32;
        while y.season == Season::Summer {
            hour = (hour + 0.05).rem_euclid(24.0);
            y.advance(0.05, hour, 0.0);
        }
        assert_eq!(y.turn().step, 0, "nothing has turned at the moment it turns");
        for seed in 0..200u32 {
            assert_eq!(y.turn().wears(seed), Season::Summer, "still in summer clothes");
        }
        // Halfway through the turn, some trees have changed and some have not.
        let mut rebuilds = 0;
        for _ in 0..(TURN_H / 0.05 * 0.55) as u32 {
            hour = (hour + 0.05).rem_euclid(24.0);
            if y.advance(0.05, hour, 0.0).is_some() {
                rebuilds += 1;
            }
        }
        let t = y.turn();
        let turned = (0..400u32).filter(|s| t.wears(*s) == Season::Autumn).count();
        assert!(turned > 50 && turned < 350, "{turned} of 400 trees had turned");
        assert!(rebuilds >= 2 && rebuilds <= STEPS as usize + 1, "{rebuilds} rebuilds halfway through");
        // And by the end of the turn every tree has.
        for _ in 0..(TURN_H / 0.05) as u32 {
            hour = (hour + 0.05).rem_euclid(24.0);
            y.advance(0.05, hour, 0.0);
        }
        let t = y.turn();
        assert_eq!(t.step, STEPS);
        for seed in 0..200u32 {
            assert_eq!(t.wears(seed), Season::Autumn);
        }
    }

    /// Snow lies where it falls in winter and is gone by evening in summer.
    #[test]
    fn snow_lies_in_winter_and_melts_in_summer() {
        let c = Calendar::of(&SEASONS);
        let mut winter = Year::new(Season::Winter, c, 7, 15.0, false);
        for _ in 0..600 {
            winter.advance(0.01, 15.0, 1.0);
        }
        assert!(winter.lying() > 0.65, "six hours of snow lay {}", winter.lying());
        assert_eq!(winter.turn().snow, 2);
        // It stays through a long clear spell.
        for _ in 0..2_400 {
            winter.advance(0.01, 15.0, 0.0);
        }
        assert!(winter.lying() > 0.5, "a day later: {}", winter.lying());
        let mut summer = Year::new(Season::Summer, c, 7, 15.0, false);
        for _ in 0..600 {
            summer.advance(0.01, 15.0, 1.0);
        }
        // Even while it falls, summer ground barely holds it.
        assert!(summer.lying() < 0.5, "summer held {}", summer.lying());
        for _ in 0..600 {
            summer.advance(0.01, 15.0, 0.0);
        }
        assert_eq!(summer.turn().snow, 0, "it was gone by evening");
    }

    /// Winter makes sure it snows, rather than leaving it to the dice.
    #[test]
    fn winter_sees_to_the_first_fall() {
        // A world that opens in winter opens with the snow already down, so
        // the one to watch is a winter the year turned into.
        let c = Calendar::of(&[Season::Summer, Season::Winter]);
        let mut y = Year::new(Season::Summer, c, 13, 15.0, true);
        while y.season == Season::Summer {
            y.advance(0.05, 15.0, 0.0);
            assert_eq!(y.wants_weather(), None, "summer asked for snow");
        }
        let mut asked = false;
        for _ in 0..(SEASON_H / 0.05) as u32 {
            y.advance(0.05, 15.0, 0.0);
            if y.wants_weather() == Some(Kind::Snow) {
                asked = true;
                break;
            }
        }
        assert!(asked, "winter never asked for snow");
        // And having asked once, it does not keep asking.
        assert_eq!(y.wants_weather(), None);
    }

    /// A year put by hand stays where it is put, fully dressed.
    #[test]
    fn a_pinned_season_holds() {
        let mut y = Year::new(Season::Summer, Calendar::of(&SEASONS), 2, 15.0, true);
        y.set(Season::Winter);
        assert_eq!(y.season, Season::Winter);
        assert_eq!(y.turn(), Turn::settled(Season::Winter, 2), "a winter asked for is the depth of it");
        for _ in 0..40_000 {
            y.advance(0.01, 15.0, 0.0);
            assert_eq!(y.season, Season::Winter);
        }
    }

    #[test]
    fn a_turn_packs_into_a_word() {
        for from in SEASONS {
            for to in SEASONS {
                for step in 0..=STEPS {
                    for snow in 0..3 {
                        let t = Turn { from, to, step, snow };
                        assert_eq!(Turn::unpack(t.pack()), t);
                    }
                }
            }
        }
    }

    /// The library's own word on a species beats the season's default.
    #[test]
    fn a_species_can_state_its_winter() {
        let w = Season::Winter.phase();
        assert!(w.abroad(Build::Insect, None) < 0.2);
        assert_eq!(w.abroad(Build::Insect, Some(1.0)), 1.0, "one that does not mind it");
        assert!(w.abroad(Build::Ground, Some(0.0)) < 0.05, "one that sleeps through it");
        // Out of winter the override does nothing.
        let s = Season::Summer.phase();
        assert_eq!(s.abroad(Build::Ground, Some(0.0)), 1.0);
    }
}
