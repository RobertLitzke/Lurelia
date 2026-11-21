//! The weather: one global state for the whole world, drifting from one kind
//! of day to another on the game clock.
//!
//! Seven kinds (`Kind`): sun, heat, clouds, rain, snow, fog, and the dry
//! storm, which only a world that names it ever sees. Sun is the fair
//! afternoon the game was tuned in, so its row is all ones and all zeroes and
//! nothing about a sunny world has moved. The rest are stated as how far they
//! lean away from it.
//!
//! Nothing here knows about the renderer or the creatures. A kind is a row of
//! numbers (`Sky`), two spells crossfade into one row, and everything that
//! cares reads that:
//!
//! - the light, the sky and the haze: `render/daylight.rs` `weather`
//! - the cloud deck, the wind and the falling rain or snow: the shaders, via
//!   the `weather` field of the frame's globals
//! - how much of each species is out in it: `Sky::abroad` and `Taste`
//!
//! It touches no voxel and no texture: a snowy day leaves bare ground bare.

use crate::world::noise::Rng;
use serde::{Deserialize, Serialize};

/// A kind of day.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Fair weather: the light the styles were tuned in, and the baseline
    /// every other kind is stated against.
    #[default]
    Sun,
    /// A hot, still, glaring day: bleached colour, air that shimmers, no wind.
    Heat,
    /// Broken cloud, flatter light, a fresh breeze.
    Clouds,
    Rain,
    Snow,
    /// A thick, quiet, close day.
    Fog,
    /// A dry storm: a dark deck and lightning out of it, cloud to ground,
    /// with no rain and no thunder (world/lightning.rs). Desert country at
    /// the end of a hot day; no world gets it unless it names it.
    Storm,
}

/// Every kind, in the order the M key steps through them.
pub const KINDS: [Kind; 7] = [Kind::Sun, Kind::Heat, Kind::Clouds, Kind::Rain, Kind::Snow, Kind::Fog, Kind::Storm];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Sun => "sun",
            Kind::Heat => "heat",
            Kind::Clouds => "clouds",
            Kind::Rain => "rain",
            Kind::Snow => "snow",
            Kind::Fog => "fog",
            Kind::Storm => "storm",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        KINDS.into_iter().find(|k| k.name() == s)
    }

    /// The kind after this one, for the key that steps through them.
    pub fn next(self) -> Kind {
        let i = KINDS.iter().position(|k| *k == self).unwrap_or(0);
        KINDS[(i + 1) % KINDS.len()]
    }

    /// This kind as the numbers everything else reads.
    pub fn sky(self) -> Sky {
        match self {
            // Fair weather is the game as it was: every multiplier 1, every
            // addition 0. Nothing in a sunny world goes through the weather.
            Kind::Sun => Sky {
                cover: 0.0,
                overcast: 0.0,
                haze: 1.0,
                mist: 0.0,
                rain: 0.0,
                snow: 0.0,
                heat: 0.0,
                wind: 1.0,
                warmth: 0.0,
                saturation: 1.0,
                brightness: 1.0,
                lightning: 0.0,
            },
            // Heat: the sky scoured of cloud, the air thick and pale with it,
            // the colour pushed, and not a breath of wind.
            Kind::Heat => Sky {
                cover: -0.3,
                overcast: 0.0,
                haze: 1.45,
                mist: 0.0,
                rain: 0.0,
                snow: 0.0,
                heat: 1.0,
                wind: 0.08,
                warmth: 0.55,
                saturation: 1.14,
                brightness: 1.12,
                lightning: 0.0,
            },
            Kind::Clouds => Sky {
                cover: 0.5,
                overcast: 0.4,
                haze: 1.15,
                mist: 0.0,
                rain: 0.0,
                snow: 0.0,
                heat: 0.0,
                wind: 1.25,
                warmth: -0.1,
                saturation: 0.94,
                brightness: 0.94,
                lightning: 0.0,
            },
            Kind::Rain => Sky {
                cover: 0.9,
                overcast: 0.85,
                haze: 1.8,
                mist: 0.07,
                rain: 1.0,
                snow: 0.0,
                heat: 0.0,
                wind: 1.5,
                warmth: -0.35,
                saturation: 0.8,
                brightness: 0.7,
                lightning: 0.0,
            },
            // Snow: nearly as closed in as rain, but bright rather than dark,
            // and the air still enough for the flakes to come straight down.
            Kind::Snow => Sky {
                cover: 0.85,
                overcast: 0.7,
                haze: 2.0,
                mist: 0.06,
                rain: 0.0,
                snow: 1.0,
                heat: 0.0,
                wind: 0.75,
                warmth: -0.2,
                saturation: 0.7,
                brightness: 0.98,
                lightning: 0.0,
            },
            // Fog: the haze does the work. Very little cloud, because the
            // sky overhead is often clear on the morning this is.
            Kind::Fog => Sky {
                cover: 0.3,
                overcast: 0.55,
                haze: 3.4,
                mist: 0.6,
                rain: 0.0,
                snow: 0.0,
                heat: 0.0,
                wind: 0.2,
                warmth: -0.15,
                saturation: 0.76,
                brightness: 0.88,
                lightning: 0.0,
            },
            // The dry storm: a heavy deck that darkens the land and stirs a
            // wind ahead of it, and lightning, but nothing falls. The light
            // is dim and cool between the strikes, which is what lets them
            // show.
            Kind::Storm => Sky {
                cover: 0.85,
                overcast: 0.78,
                haze: 1.25,
                mist: 0.0,
                rain: 0.0,
                snow: 0.0,
                heat: 0.2,
                wind: 1.35,
                warmth: -0.15,
                saturation: 0.86,
                brightness: 0.66,
                lightning: 1.0,
            },
        }
    }

    /// How long a spell of this kind lasts, in game hours. Fair weather sits;
    /// rain, snow and fog pass.
    fn spell(self) -> (f32, f32) {
        match self {
            Kind::Sun => (4.0, 9.0),
            Kind::Heat => (3.0, 7.0),
            Kind::Clouds => (2.5, 6.0),
            Kind::Rain => (1.0, 3.5),
            Kind::Snow => (1.5, 4.0),
            Kind::Fog => (1.0, 2.5),
            Kind::Storm => (0.8, 2.5),
        }
    }

    /// What may follow this kind, and how readily. Rain never breaks straight
    /// into sun and sun never opens into rain: both go by way of cloud, which
    /// is the hinge of the whole table.
    fn after(self) -> &'static [(Kind, f32)] {
        match self {
            Kind::Sun => &[(Kind::Clouds, 1.0), (Kind::Heat, 0.5), (Kind::Fog, 0.2), (Kind::Storm, 0.2)],
            Kind::Heat => &[(Kind::Sun, 1.0), (Kind::Clouds, 0.6), (Kind::Storm, 0.5)],
            Kind::Clouds => &[(Kind::Sun, 1.0), (Kind::Rain, 0.9), (Kind::Snow, 0.6), (Kind::Fog, 0.4), (Kind::Heat, 0.15), (Kind::Storm, 0.35)],
            Kind::Rain => &[(Kind::Clouds, 1.0), (Kind::Fog, 0.35), (Kind::Snow, 0.2)],
            Kind::Snow => &[(Kind::Clouds, 1.0), (Kind::Fog, 0.3), (Kind::Rain, 0.2)],
            Kind::Fog => &[(Kind::Sun, 1.0), (Kind::Clouds, 0.8), (Kind::Rain, 0.25)],
            // A dry storm blows itself out into broken cloud or a clear
            // evening; it never turns to rain, or it would not be dry.
            Kind::Storm => &[(Kind::Clouds, 1.0), (Kind::Sun, 0.6), (Kind::Heat, 0.15)],
        }
    }

    /// How often this kind turns up in a world that names no climate of its
    /// own. A world sets its own with `"weather"` in `world.json`.
    fn common(self) -> f32 {
        match self {
            Kind::Sun => 1.0,
            Kind::Heat => 0.3,
            Kind::Clouds => 0.8,
            Kind::Rain => 0.5,
            Kind::Snow => 0.15,
            Kind::Fog => 0.25,
            // Only where a world names it.
            Kind::Storm => 0.0,
        }
    }
}

/// How long a change takes, in game hours: about half a minute of real time
/// at the clock's afternoon rate, which is long enough that the light slides
/// rather than snaps and short enough that a downpour arrives.
const CHANGE_H: f32 = 0.55;

/// One kind of day as the numbers everything else reads. Between two spells
/// this is the two rows mixed, so every value is continuous.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Added to the cloud deck's cover, -1 (scoured clear) to 1 (solid).
    pub cover: f32,
    /// How flat the light is, 0 to 1: the sun loses its edge and the sky
    /// fills in for it.
    pub overcast: f32,
    /// Multiplier on the distance haze.
    pub haze: f32,
    /// Mist lying everywhere, added to the hour's own (daylight.rs `Air`).
    pub mist: f32,
    /// Falling rain, 0 to 1 (post.wgsl).
    pub rain: f32,
    /// Falling snow, 0 to 1.
    pub snow: f32,
    /// How hot it is, 0 to 1. Nothing physical yet: creatures read it.
    pub heat: f32,
    /// Multiplier on the wind that bends grass and leaves, 0 (still) to about
    /// 1.5 (gusty).
    pub wind: f32,
    /// Added to the grade's warm/cool shift: heat warms, rain and snow cool.
    pub warmth: f32,
    pub saturation: f32,
    pub brightness: f32,
    /// How often lightning strikes, 0 (never) to 1 (the thick of a dry
    /// storm): world/lightning.rs.
    pub lightning: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Kind::Sun.sky()
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Sky {
    /// Two kinds of day, `t` of the way from the first to the second.
    pub fn blend(a: Sky, b: Sky, t: f32) -> Sky {
        let t = t.clamp(0.0, 1.0);
        Sky {
            cover: mix(a.cover, b.cover, t),
            overcast: mix(a.overcast, b.overcast, t),
            haze: mix(a.haze, b.haze, t),
            mist: mix(a.mist, b.mist, t),
            rain: mix(a.rain, b.rain, t),
            snow: mix(a.snow, b.snow, t),
            heat: mix(a.heat, b.heat, t),
            wind: mix(a.wind, b.wind, t),
            warmth: mix(a.warmth, b.warmth, t),
            saturation: mix(a.saturation, b.saturation, t),
            brightness: mix(a.brightness, b.brightness, t),
            lightning: mix(a.lightning, b.lightning, t),
        }
    }

    /// How windy it is beyond a fair day's breeze, 0 to 1: what grounds the
    /// birds. Below a fair day (heat, fog) this is 0, not negative.
    pub fn gustiness(&self) -> f32 {
        ((self.wind - 1.0) / 0.5).clamp(0.0, 1.0)
    }

    /// The share of a species that is out in this weather, 0 to 1 (`Taste`
    /// says what it minds). Fair weather is 1 for everything.
    pub fn abroad(&self, t: Taste) -> f32 {
        let f = mix(1.0, t.rain, self.rain) * mix(1.0, t.snow, self.snow) * mix(1.0, t.heat, self.heat) * mix(1.0, t.wind, self.gustiness());
        f.clamp(0.0, 1.0)
    }

    /// How far this weather alone would put a grazing animal down in the
    /// grass, 0 to 1, whatever the hour says (creatures/walk.rs beds it down
    /// the way the night does). Cattle stand out a shower; a downpour or
    /// driving snow puts them under the trees.
    pub fn sheltering(&self) -> f32 {
        (self.rain * 0.5 + self.snow * 0.35).clamp(0.0, 1.0)
    }

    /// The `weather` field of the frame's globals: cover, rain, snow, wind.
    pub fn uniform(&self) -> [f32; 4] {
        [self.cover, self.rain, self.snow, self.wind]
    }
}

/// What a species minds about the weather: the share of it still abroad in
/// the thick of each, 1 for one it doesn't notice. `Taste::of` gives the
/// default for a way of moving and a library entry overrides any of them
/// (`library::Behaviour` `rain`, `snow`, `heat`, `wind`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taste {
    pub rain: f32,
    pub snow: f32,
    /// In the full heat of the day. Reptiles come out in it; mammals don't.
    pub heat: f32,
    /// In a gale.
    pub wind: f32,
}

impl Default for Taste {
    fn default() -> Self {
        Taste {
            rain: 1.0,
            snow: 1.0,
            heat: 1.0,
            wind: 1.0,
        }
    }
}

/// The broad kinds of animal the defaults are written for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Build {
    /// Anything on the ground: walkers, snakes.
    Ground,
    /// Anything whose living is made in the air.
    Flier,
    /// Anything small and flying: it is simply not out in bad weather.
    Insect,
    /// Anything in the water, which neither rain nor wind reaches.
    Swimmer,
}

impl Taste {
    /// What this build of animal minds, before a library entry says otherwise.
    pub fn of(b: Build) -> Taste {
        match b {
            Build::Ground => Taste {
                rain: 0.6,
                snow: 0.55,
                heat: 0.65,
                wind: 0.9,
            },
            Build::Flier => Taste {
                rain: 0.15,
                snow: 0.25,
                heat: 0.8,
                wind: 0.3,
            },
            Build::Insect => Taste {
                rain: 0.04,
                snow: 0.0,
                heat: 1.0,
                wind: 0.15,
            },
            Build::Swimmer => Taste::default(),
        }
    }

    /// The same with whatever the library entry sets of its own.
    pub fn with(mut self, rain: Option<f32>, snow: Option<f32>, heat: Option<f32>, wind: Option<f32>) -> Taste {
        if let Some(v) = rain {
            self.rain = v;
        }
        if let Some(v) = snow {
            self.snow = v;
        }
        if let Some(v) = heat {
            self.heat = v;
        }
        if let Some(v) = wind {
            self.wind = v;
        }
        self
    }
}

/// Which kinds of day a world gets, and how often. A world with no `weather`
/// block of its own gets every kind at `Kind::common`.
#[derive(Clone, Debug)]
pub struct Climate {
    weights: [f32; KINDS.len()],
}

impl Default for Climate {
    fn default() -> Self {
        Climate { weights: KINDS.map(Kind::common) }
    }
}

impl Climate {
    /// A world's own climate: kind name to weight. A kind left out of a
    /// non-empty block never happens there, so a desert that lists everything
    /// but snow never sees one.
    pub fn from_weights(w: &std::collections::BTreeMap<String, f32>) -> Climate {
        if w.is_empty() {
            return Climate::default();
        }
        Climate {
            weights: KINDS.map(|k| w.get(k.name()).copied().unwrap_or(0.0).max(0.0)),
        }
    }

    fn weight(&self, k: Kind) -> f32 {
        self.weights[KINDS.iter().position(|x| *x == k).unwrap_or(0)]
    }

    /// Whether this world ever sees this kind of day.
    pub fn has(&self, k: Kind) -> bool {
        self.weight(k) > 0.0
    }

    /// The kind a world opens on: the likeliest it has.
    pub fn commonest(&self) -> Kind {
        KINDS.into_iter().max_by(|a, b| self.weight(*a).total_cmp(&self.weight(*b))).unwrap_or(Kind::Sun)
    }

    /// Which kinds this world names that nothing is called, for `--validate`.
    pub fn unknown(w: &std::collections::BTreeMap<String, f32>) -> Vec<String> {
        w.keys().filter(|k| Kind::parse(k).is_none()).cloned().collect()
    }

    /// What follows `from`, drawn from that kind's own table and weighted by
    /// how common each is here. Where nothing in that kind's table is allowed
    /// here, the weather stays as it is rather than leaving the climate: a
    /// world of nothing but sun (the duel bench) holds its sun.
    fn next(&self, from: Kind, rng: &mut Rng, lean: &[f32; KINDS.len()]) -> Kind {
        let at = |k: Kind| KINDS.iter().position(|x| *x == k).unwrap_or(0);
        let choices: Vec<(Kind, f32)> = from
            .after()
            .iter()
            .filter(|(k, _)| self.has(*k))
            .map(|&(k, w)| (k, w * self.weight(k) * lean[at(k)].max(0.0)))
            .filter(|(_, w)| *w > 0.0)
            .collect();
        let total: f32 = choices.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            return from;
        }
        let mut pick = rng.unit() * total;
        for (k, w) in &choices {
            pick -= w;
            if pick <= 0.0 {
                return *k;
            }
        }
        choices[choices.len() - 1].0
    }
}

/// The world's weather: the spell it is in, the one it is coming from, and
/// how far through the change it is.
#[derive(Clone, Debug)]
pub struct Weather {
    /// The kind it is settling into.
    pub kind: Kind,
    /// The kind it is coming out of.
    from: Kind,
    /// 0 at the moment of a change, 1 once it has arrived.
    blend: f32,
    /// Game hours left in this spell before the next change begins.
    left: f32,
    /// False once the weather is pinned (`--weather`, or the M key): it stays
    /// where it is put.
    pub drifting: bool,
    climate: Climate,
    /// What the season makes of this world's climate: a multiplier per kind,
    /// all 1 in summer (world/season.rs `Phase::weather`). The climate says
    /// what the world gets; this says what the year makes of it.
    lean: [f32; KINDS.len()],
    rng: Rng,
}

impl Default for Weather {
    fn default() -> Self {
        Weather::new(Kind::Sun, Climate::default(), 0, true)
    }
}

impl Weather {
    pub fn new(kind: Kind, climate: Climate, seed: u32, drifting: bool) -> Weather {
        let mut rng = Rng::new(seed ^ 0x57ea_7e4du32);
        let (lo, hi) = kind.spell();
        let left = lo + rng.unit() * (hi - lo);
        Weather {
            kind,
            from: kind,
            blend: 1.0,
            left,
            drifting,
            climate,
            lean: [1.0; KINDS.len()],
            rng,
        }
    }

    /// How the season leans on this world's climate (world/season.rs).
    pub fn lean(&mut self, w: [f32; KINDS.len()]) {
        self.lean = w;
    }

    /// The numbers this moment of the world looks like.
    /// What this world's weather may be (world.json `weather`).
    pub fn climate(&self) -> &Climate {
        &self.climate
    }

    pub fn sky(&self) -> Sky {
        if self.blend >= 1.0 {
            return self.kind.sky();
        }
        let t = self.blend;
        Sky::blend(self.from.sky(), self.kind.sky(), t * t * (3.0 - 2.0 * t))
    }

    /// What to call it on screen: the kind it is becoming, once it is more
    /// than halfway there.
    pub fn name(&self) -> &'static str {
        if self.blend < 0.5 {
            self.from.name()
        } else {
            self.kind.name()
        }
    }

    /// Carry the weather forward by `hours` of game clock. A spell runs down,
    /// then the next kind is drawn and crossfades in over `CHANGE_H`.
    pub fn advance(&mut self, hours: f32) {
        if hours <= 0.0 {
            return;
        }
        if self.blend < 1.0 {
            self.blend = (self.blend + hours / CHANGE_H).min(1.0);
        }
        if !self.drifting {
            return;
        }
        self.left -= hours;
        if self.left > 0.0 {
            return;
        }
        // A change that lands mid-change still starts from where the sky
        // actually is, so a run of quick spells never jumps.
        let next = self.climate.next(self.kind, &mut self.rng, &self.lean);
        self.from = if self.blend < 1.0 { self.from } else { self.kind };
        self.kind = next;
        self.blend = 0.0;
        let (lo, hi) = next.spell();
        self.left = lo + self.rng.unit() * (hi - lo) + CHANGE_H;
    }

    /// Put the weather to a kind by hand (the M key, `--weather`): it arrives
    /// over the usual change and then stays, since a hand-set sky that wanders
    /// off on its own is worse than no key at all.
    pub fn set(&mut self, kind: Kind) {
        self.from = if self.blend < 1.0 { self.from } else { self.kind };
        self.kind = kind;
        self.blend = 0.0;
        self.drifting = false;
    }

    /// Ask for a kind of day without pinning the sky: it arrives over the
    /// usual change, sits for its usual spell and then drifts on as before.
    /// Winter uses it to make sure the snow starts (world/season.rs).
    pub fn nudge(&mut self, kind: Kind) {
        // Never out of the world's own climate: a winter in a world that
        // never sees snow is a cold dry one, not a snowy one.
        if self.kind == kind || !self.climate.has(kind) {
            return;
        }
        self.from = if self.blend < 1.0 { self.from } else { self.kind };
        self.kind = kind;
        self.blend = 0.0;
        let (lo, hi) = kind.spell();
        self.left = lo + self.rng.unit() * (hi - lo) + CHANGE_H;
    }

    /// The kind after the one it is heading for, for the key that steps
    /// through them.
    pub fn next_kind(&self) -> Kind {
        self.kind.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fair weather is the game as it was: nothing it hands anything else
    /// moves a value.
    #[test]
    fn sun_changes_nothing() {
        let s = Kind::Sun.sky();
        assert_eq!(s.cover, 0.0);
        assert_eq!(s.haze, 1.0);
        assert_eq!(s.wind, 1.0);
        assert_eq!(s.mist, 0.0);
        assert_eq!(s.rain, 0.0);
        assert_eq!(s.snow, 0.0);
        assert_eq!(s.overcast, 0.0);
        assert_eq!(s.warmth, 0.0);
        assert_eq!(s.saturation, 1.0);
        assert_eq!(s.brightness, 1.0);
        assert_eq!(s.uniform(), [0.0, 0.0, 0.0, 1.0]);
        // And everything is out in it.
        for b in [Build::Ground, Build::Flier, Build::Insect, Build::Swimmer] {
            assert_eq!(s.abroad(Taste::of(b)), 1.0);
        }
        assert_eq!(s.sheltering(), 0.0);
        assert_eq!(s.gustiness(), 0.0);
    }

    #[test]
    fn heat_is_a_still_bright_day() {
        let s = Kind::Heat.sky();
        assert!(s.wind < 0.15, "no wind in the heat: {}", s.wind);
        assert!(s.cover < 0.0, "the sky is scoured clear");
        assert!(s.saturation > 1.0 && s.brightness > 1.0 && s.warmth > 0.0);
        assert_eq!(s.rain, 0.0);
    }

    #[test]
    fn names_round_trip() {
        for k in KINDS {
            assert_eq!(Kind::parse(k.name()), Some(k));
        }
        assert_eq!(Kind::parse("hail"), None);
        // The key steps through every kind and comes back.
        let mut k = Kind::Sun;
        for _ in 0..KINDS.len() {
            k = k.next();
        }
        assert_eq!(k, Kind::Sun);
    }

    #[test]
    fn a_change_slides_rather_than_snaps() {
        let mut w = Weather::new(Kind::Sun, Climate::default(), 7, true);
        assert_eq!(w.sky(), Kind::Sun.sky());
        w.set(Kind::Rain);
        assert_eq!(w.sky(), Kind::Sun.sky(), "the change starts where it was");
        assert_eq!(w.name(), "sun", "and is still called the old kind");
        w.advance(CHANGE_H * 0.5);
        let half = w.sky();
        assert!(half.rain > 0.1 && half.rain < 0.9, "halfway: {}", half.rain);
        assert_eq!(w.name(), "rain");
        w.advance(CHANGE_H);
        assert_eq!(w.sky(), Kind::Rain.sky());
        assert!(!w.drifting, "a hand-set sky stays put");
        w.advance(100.0);
        assert_eq!(w.kind, Kind::Rain);
    }

    /// Left alone it keeps changing, and every kind a world with no climate
    /// of its own allows turns up: all but the dry storm, which only a world
    /// that names it gets.
    #[test]
    fn it_drifts_through_the_kinds() {
        let mut w = Weather::new(Kind::Sun, Climate::default(), 3, true);
        let mut seen = std::collections::BTreeSet::new();
        let mut changes = 0;
        let mut was = w.kind;
        for _ in 0..40_000 {
            w.advance(0.01);
            seen.insert(w.kind.name());
            if w.kind != was {
                changes += 1;
                was = w.kind;
            }
            let s = w.sky();
            assert!(s.haze >= 1.0 && s.rain <= 1.0 && s.snow <= 1.0 && s.wind >= 0.0);
        }
        assert!(changes > 40, "400 game hours brought {changes} changes");
        assert_eq!(seen.len(), KINDS.len() - 1, "saw {seen:?}");
        assert!(!seen.contains("storm"));
    }

    /// A world's climate rules kinds out, and the drift never reaches one.
    #[test]
    fn a_climate_rules_kinds_out() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("sun".to_string(), 2.0);
        m.insert("heat".to_string(), 1.0);
        m.insert("clouds".to_string(), 0.5);
        let c = Climate::from_weights(&m);
        assert!(c.has(Kind::Sun) && !c.has(Kind::Snow) && !c.has(Kind::Rain));
        assert_eq!(c.commonest(), Kind::Sun);
        let mut w = Weather::new(Kind::Sun, c, 11, true);
        for _ in 0..40_000 {
            w.advance(0.01);
            assert!(matches!(w.kind, Kind::Sun | Kind::Heat | Kind::Clouds), "a desert got {}", w.kind.name());
        }
        // An empty block is no block: every kind again.
        assert!(Climate::from_weights(&std::collections::BTreeMap::new()).has(Kind::Snow));
    }

    /// A world of one kind holds it, and never leaves its own climate for a
    /// kind it does not have (the duel bench, which must not vary at all).
    #[test]
    fn a_world_of_one_kind_holds_it() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("sun".to_string(), 1.0);
        let mut w = Weather::new(Kind::Sun, Climate::from_weights(&m), 5, true);
        for _ in 0..20_000 {
            w.advance(0.01);
            assert_eq!(w.kind, Kind::Sun);
            assert_eq!(w.sky(), Kind::Sun.sky());
        }
        // And one of nothing but rain stays in the rain rather than clearing
        // to a kind the world hasn't got.
        let mut m = std::collections::BTreeMap::new();
        m.insert("rain".to_string(), 1.0);
        let mut w = Weather::new(Kind::Rain, Climate::from_weights(&m), 5, true);
        for _ in 0..20_000 {
            w.advance(0.01);
            assert_eq!(w.kind, Kind::Rain);
        }
    }

    #[test]
    fn a_climate_names_only_real_kinds() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("rain".to_string(), 1.0);
        m.insert("hail".to_string(), 1.0);
        assert_eq!(Climate::unknown(&m), vec!["hail".to_string()]);
    }

    /// Rain and sun are never next to each other: the table goes by way of
    /// cloud both ways.
    #[test]
    fn rain_and_sun_are_not_neighbours() {
        assert!(!Kind::Sun.after().iter().any(|(k, _)| *k == Kind::Rain));
        assert!(!Kind::Rain.after().iter().any(|(k, _)| *k == Kind::Sun));
        assert!(!Kind::Sun.after().iter().any(|(k, _)| *k == Kind::Snow));
    }

    /// Birds and insects go quiet in the rain; fish never notice it.
    #[test]
    fn the_rain_takes_the_birds_out_of_the_air() {
        let rain = Kind::Rain.sky();
        let fliers = rain.abroad(Taste::of(Build::Flier));
        let ground = rain.abroad(Taste::of(Build::Ground));
        let bugs = rain.abroad(Taste::of(Build::Insect));
        let fish = rain.abroad(Taste::of(Build::Swimmer));
        assert!(bugs < fliers, "insects {bugs} vs birds {fliers}");
        assert!(fliers < ground, "birds {fliers} vs walkers {ground}");
        assert!(ground < fish, "walkers {ground} vs fish {fish}");
        assert_eq!(fish, 1.0);
        assert!(rain.sheltering() > 0.4, "a downpour puts grazers down");
        // And a gale alone does it too, with no rain falling.
        let gusty = Sky { wind: 1.5, ..Kind::Sun.sky() };
        assert!(gusty.abroad(Taste::of(Build::Flier)) < 0.4);
        assert_eq!(gusty.abroad(Taste::of(Build::Swimmer)), 1.0);
        // A library entry can say it doesn't mind.
        let tough = Taste::of(Build::Flier).with(Some(1.0), None, None, Some(1.0));
        assert_eq!(rain.abroad(tough), 1.0);
    }

    /// Lizards are out in the heat and mammals are not.
    #[test]
    fn the_heat_suits_a_reptile() {
        let heat = Kind::Heat.sky();
        let basker = Taste::of(Build::Ground).with(None, None, Some(1.0), None);
        assert_eq!(heat.abroad(basker), 1.0);
        assert!(heat.abroad(Taste::of(Build::Ground)) < 0.7);
    }
}
