//! What feet leave behind, and what the ground underfoot is like to walk on.
//!
//! `tread` reads a ground material for the two things a foot cares about:
//! how loud a step on it is (startle.rs hears it) and whether it keeps a
//! mark. Snow, mud and sand always take a print; bare earth takes one once
//! the rain has softened it (world/damp.rs `soft`); grass and low plants do
//! not hold a print but are trampled flat by anything big enough, and rock,
//! gravel and leaf litter take nothing.
//!
//! The marks (`Marks`) are laid by walkers as they go (walk.rs reports each
//! footfall as a `Step`) and by the player, and worn away by the weather:
//! rain blurs a print in mud, new snow fills one in snow, wind smooths sand,
//! and trampled grass springs back over a few hours. How worn a print is is
//! how old it reads, which is most of what tracking is. render/signs.rs
//! draws the prints; the trampled patches bend the grass (foliage.wgsl).

use crate::world::material::AIR;
use crate::world::matlib::{self, flag};
use crate::world::store::Store;
use glam::Vec3;

/// How a ground keeps a mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Takes {
    /// Rock, gravel, leaf litter, water: nothing.
    Nothing,
    /// Snow, mud, sand: a print every time.
    Print,
    /// Bare earth: a print once it is wet enough to be soft.
    WhenSoft,
    /// Grass and low plants: no print, but bent and trampled flat.
    Bends,
}

/// What a print is pressed into, for how it looks and how fast it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Bed {
    Snow,
    Mud,
    Sand,
    #[default]
    Earth,
}

/// The ground underfoot as a foot meets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tread {
    /// How loud a step is, relative to ordinary ground (1): dry leaf litter
    /// about 1.8, moss 0.35.
    pub loud: f32,
    /// How much the wet quiets it, 0 to 1: wet litter and grass make
    /// little noise, wet rock is no quieter.
    pub damps: f32,
    pub takes: Takes,
    pub bed: Bed,
}

const fn tread(loud: f32, damps: f32, takes: Takes, bed: Bed) -> Tread {
    Tread { loud, damps, takes, bed }
}

/// The tread of a material by what it is called and what it is flagged as.
/// Names are the library's keys (`library/materials`), so a new material is
/// read the way its name says without an entry of its own.
pub fn tread_of_name(name: &str, water: bool, grass: bool, hard: bool) -> Tread {
    use Bed::*;
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|k| n.contains(k));
    if water {
        tread(1.5, 0.0, Takes::Nothing, Mud)
    } else if has(&["snow"]) {
        tread(0.5, 0.0, Takes::Print, Snow)
    } else if has(&["mud", "bog", "marsh", "silt", "clay", "peat"]) {
        tread(0.7, 0.0, Takes::Print, Mud)
    } else if has(&["sand", "dune"]) && !has(&["sandstone"]) {
        tread(0.6, 0.0, Takes::Print, Sand)
    } else if has(&["litter", "forest_floor", "duff", "leaf_mould", "leaves"]) {
        tread(1.8, 0.6, Takes::Nothing, Earth)
    } else if has(&["needles"]) {
        tread(1.2, 0.5, Takes::Nothing, Earth)
    } else if has(&["moss"]) {
        tread(0.35, 0.0, Takes::Nothing, Earth)
    } else if has(&["plank", "timber", "board", "deck", "wood"]) {
        tread(1.2, 0.0, Takes::Nothing, Earth)
    } else if has(&["gravel", "scree", "shingle", "pebble", "cinder", "talus"]) {
        tread(1.6, 0.1, Takes::Nothing, Earth)
    } else if grass || has(&["grass", "turf", "meadow", "lawn", "heath", "fern"]) {
        tread(0.8, 0.4, Takes::Bends, Earth)
    } else if hard {
        tread(1.0, 0.0, Takes::Nothing, Earth)
    } else {
        tread(0.8, 0.3, Takes::WhenSoft, Earth)
    }
}

/// The tread of the voxel material `m`.
pub fn tread_of(m: u8) -> Tread {
    if m == AIR {
        return tread(0.8, 0.0, Takes::Nothing, Bed::Earth);
    }
    let water = matlib::has(m, flag::WATER);
    let grass = matlib::has(m, flag::UNDERSTORY) || matlib::grass(m).is_some();
    let hard = crate::sound::watch::ground_of(m) == crate::sound::Ground::Stone;
    // The stable key first: built-in materials (SNOW, GRAVEL) have no
    // library name.
    tread_of_name(matlib::key(m).or(matlib::name(m)).unwrap_or(""), water, grass, hard)
}

impl Tread {
    /// How loud a step on it is with the ground `wet` (0 to 1), relative to
    /// ordinary dry ground.
    pub fn loudness(&self, wet: f32) -> f32 {
        self.loud * (1.0 - self.damps * wet.clamp(0.0, 1.0))
    }

    /// Whether a foot leaves a print here, with the ground `soft` (0 to 1,
    /// world/damp.rs).
    pub fn prints(&self, soft: f32) -> bool {
        match self.takes {
            Takes::Print => true,
            Takes::WhenSoft => soft > 0.45,
            _ => false,
        }
    }
}

/// The shape of a foot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Foot {
    /// Two toes: deer, cattle, pigs, goats.
    Hoof,
    /// Four toes round a pad: dogs, cats, foxes, most small mammals.
    Paw,
    /// Five toes over a broad pad, the whole foot down.
    Bear,
    /// Three toes forward and one back.
    Bird,
    /// The player's boot.
    Boot,
    /// A stretch of a car's tyre track, laid end to end (ride/drive.rs).
    Tyre,
}

const HOOVED: [&str; 35] = [
    "deer",
    "elk",
    "moose",
    "cattle",
    "cow",
    "bull",
    "ox",
    "bison",
    "buffalo",
    "goat",
    "sheep",
    "ram",
    "pig",
    "boar",
    "hog",
    "horse",
    "pony",
    "zebra",
    "donkey",
    "antelope",
    "gazelle",
    "impala",
    "kudu",
    "oryx",
    "ibex",
    "chamois",
    "wildebeest",
    "reindeer",
    "caribou",
    "yak",
    "aurochs",
    "camel",
    "giraffe",
    "tapir",
    "bighorn",
];
const BIRDS: [&str; 22] = [
    "duck",
    "goose",
    "swan",
    "heron",
    "egret",
    "crane",
    "stork",
    "ibis",
    "turkey",
    "grouse",
    "pheasant",
    "quail",
    "partridge",
    "ostrich",
    "emu",
    "rhea",
    "chicken",
    "hen",
    "peafowl",
    "bustard",
    "plover",
    "gull",
];

impl Foot {
    /// A creature's foot by its library name: hooved and bird names read
    /// as they say, bears as bears, anything that flies as a bird, and the
    /// rest as a paw.
    pub fn of(name: &str, flies: bool) -> Foot {
        // Whole words, or the end of one for the longer keys (a reindeer is
        // a deer, a warthog is not a hedgehog's cousin, a fox is no ox).
        let n = name.to_lowercase();
        let words: Vec<&str> = n.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()).collect();
        let is = |keys: &[&str]| keys.iter().any(|k| words.iter().any(|w| *w == *k || (k.len() >= 4 && w.ends_with(k))));
        if is(&["bear"]) {
            Foot::Bear
        } else if is(&HOOVED) || is(&["warthog"]) {
            Foot::Hoof
        } else if flies || is(&BIRDS) {
            Foot::Bird
        } else {
            Foot::Paw
        }
    }

    /// The print's length for a body `height` metres tall.
    pub fn size(self, height: f32) -> f32 {
        let k = match self {
            Foot::Hoof => 0.06,
            Foot::Paw => 0.07,
            Foot::Bear => 0.15,
            Foot::Bird => 0.1,
            Foot::Boot => return 0.29,
            Foot::Tyre => return 0.6,
        };
        (height * k).clamp(0.035, 0.32)
    }

    pub fn index(self) -> u32 {
        match self {
            Foot::Hoof => 0,
            Foot::Paw => 1,
            Foot::Bear => 2,
            Foot::Bird => 3,
            Foot::Boot => 4,
            Foot::Tyre => 5,
        }
    }
}

/// One footfall, as a walker or the player reports it.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    /// Where the foot came down, on the drawn ground.
    pub pos: Vec3,
    /// Which way it was heading (radians, the creature's yaw convention:
    /// forward is (-sin, -cos)).
    pub yaw: f32,
    pub foot: Foot,
    /// The body's height, metres: how big a print and how wide a trampled
    /// patch.
    pub height: f32,
}

/// A mark on the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Print(Foot),
    /// Grass and low plants pressed flat, leaning the way it went.
    Trampled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sign {
    pub pos: Vec3,
    pub yaw: f32,
    pub kind: Kind,
    pub bed: Bed,
    /// Print length, or a trampled patch's radius, metres.
    pub size: f32,
    /// 0 fresh to 1 gone.
    pub wear: f32,
}

/// How many marks are kept; the oldest go first.
pub const MAX_MARKS: usize = 4096;
/// Only bodies at least this tall trample grass enough to show.
const TRAMPLES: f32 = 0.5;

/// Every mark on the ground.
#[derive(Default)]
pub struct Marks {
    pub signs: Vec<Sign>,
    next: usize,
    /// Bumped whenever a mark is laid or goes, so the renderer can tell when
    /// to send them again.
    pub version: u64,
}

/// The material under a foot at `pos` (the same probe as the footstep
/// sounds, fx/hooks.rs).
pub fn ground_at(store: &Store, pos: Vec3) -> u8 {
    crate::fx::hooks::ground_under(store, pos)
}

impl Marks {
    fn push(&mut self, s: Sign) {
        if self.signs.len() < MAX_MARKS {
            self.signs.push(s);
        } else {
            self.signs[self.next] = s;
            self.next = (self.next + 1) % MAX_MARKS;
        }
        self.version += 1;
    }

    /// Lay what `step` leaves on ground of material `ground`, with the
    /// ground `soft` (world/damp.rs).
    pub fn lay(&mut self, step: &Step, ground: u8, soft: f32) {
        let t = tread_of(ground);
        if t.prints(soft) {
            self.push(Sign {
                pos: step.pos,
                yaw: step.yaw,
                kind: Kind::Print(step.foot),
                bed: t.bed,
                size: step.foot.size(step.height),
                wear: 0.0,
            });
        } else if t.takes == Takes::Bends && step.height >= TRAMPLES {
            self.trample(step);
        }
    }

    /// Press the grass flat where `step` came down. A fresh patch close to
    /// another is folded into it rather than laid again, so a herd walking
    /// one line through a meadow leaves a trail rather than a thousand
    /// overlapping patches.
    fn trample(&mut self, step: &Step) {
        let r = 0.25 + 0.3 * step.height.min(2.5);
        let close = r * 0.8;
        let start = self.signs.len().saturating_sub(256);
        for s in self.signs[start..].iter_mut().rev() {
            if s.kind == Kind::Trampled && (s.pos - step.pos).length_squared() < close * close {
                s.wear = 0.0;
                s.size = s.size.max(r);
                return;
            }
        }
        self.push(Sign {
            pos: step.pos,
            yaw: step.yaw,
            kind: Kind::Trampled,
            bed: Bed::Earth,
            size: r,
            wear: 0.0,
        });
    }

    /// Wear every mark by `hours` of game clock under `sky`. Each bed has
    /// its own pace -- a print in snow keeps a day, in sand a few hours --
    /// and the weather adds to it: rain blurs mud and earth, falling snow
    /// fills snow, wind smooths sand, warmth melts snow. Grass springs back.
    pub fn advance(&mut self, hours: f32, sky: &crate::world::weather::Sky) {
        if hours <= 0.0 || self.signs.is_empty() {
            return;
        }
        let windy = (sky.wind - 1.0).max(0.0);
        let before = self.signs.len();
        for s in &mut self.signs {
            let rate = match (s.kind, s.bed) {
                (Kind::Trampled, _) => 0.16,
                (_, Bed::Snow) => 0.04 + 1.5 * sky.snow + 0.25 * sky.heat + 0.3 * sky.rain,
                (_, Bed::Mud) => 0.03 + 0.5 * sky.rain,
                (_, Bed::Sand) => 0.08 + 0.6 * windy + 0.7 * sky.rain,
                (_, Bed::Earth) => 0.05 + 0.8 * sky.rain,
            };
            s.wear += rate * hours;
        }
        let gone = self.signs.iter().any(|s| s.wear >= 1.0);
        if gone {
            // Dropping marks breaks the ring's order; it is only an order of
            // which goes first when full, so start the ring again.
            self.signs.retain(|s| s.wear < 1.0);
            self.next = 0;
        }
        if gone || self.signs.len() != before {
            self.version += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::weather::Kind as Sky;

    #[test]
    fn names_read_as_their_ground() {
        assert_eq!(tread_of_name("packed_snow", false, false, false).takes, Takes::Print);
        assert_eq!(tread_of_name("river_mud", false, false, false).takes, Takes::Print);
        assert_eq!(tread_of_name("red_sand", false, false, false).takes, Takes::Print);
        assert_eq!(tread_of_name("navajo_sandstone", false, false, true).takes, Takes::Nothing);
        assert_eq!(tread_of_name("meadow_turf", false, true, false).takes, Takes::Bends);
        assert_eq!(tread_of_name("loam", false, false, false).takes, Takes::WhenSoft);
        assert_eq!(tread_of_name("cobbles", false, false, true).takes, Takes::Nothing);
    }

    #[test]
    fn dry_litter_is_loud_and_wet_litter_quiet() {
        let litter = tread_of_name("forest_floor", false, false, false);
        assert!(litter.loudness(0.0) > 1.5);
        assert!(litter.loudness(1.0) < litter.loudness(0.0) * 0.5);
        // Bog moss is a bog: it takes a print rather than hushing like a
        // moss bank.
        assert_eq!(tread_of_name("bog_moss", false, false, false).takes, Takes::Print);
        assert!(tread_of_name("karst_moss", false, false, false).loudness(0.0) < 0.5);
    }

    #[test]
    fn bare_earth_prints_only_when_soft() {
        let earth = tread_of_name("loam", false, false, false);
        assert!(!earth.prints(0.1));
        assert!(earth.prints(0.8));
        assert!(tread_of_name("packed_snow", false, false, false).prints(0.0));
    }

    #[test]
    fn feet_by_name() {
        assert_eq!(Foot::of("red_deer", false), Foot::Hoof);
        assert_eq!(Foot::of("brown_bear", false), Foot::Bear);
        assert_eq!(Foot::of("grey_heron", true), Foot::Bird);
        assert_eq!(Foot::of("red_fox", false), Foot::Paw);
        assert_eq!(Foot::of("hedgehog", false), Foot::Paw);
        assert_eq!(Foot::of("reindeer", false), Foot::Hoof);
        assert_eq!(Foot::of("musk_ox", false), Foot::Hoof);
        assert!(Foot::Bear.size(1.2) > Foot::Paw.size(0.5) * 2.0);
    }

    fn sign(bed: Bed, kind: Kind) -> Sign {
        Sign {
            pos: Vec3::ZERO,
            yaw: 0.0,
            kind,
            bed,
            size: 0.1,
            wear: 0.0,
        }
    }

    #[test]
    fn rain_blurs_mud_faster_than_a_dry_day_and_snow_fills_snow() {
        let mut dry = Marks::default();
        dry.signs.push(sign(Bed::Mud, Kind::Print(Foot::Hoof)));
        let mut wet = Marks::default();
        wet.signs.push(sign(Bed::Mud, Kind::Print(Foot::Hoof)));
        dry.advance(1.0, &Sky::Sun.sky());
        wet.advance(1.0, &Sky::Rain.sky());
        assert!(wet.signs[0].wear > dry.signs[0].wear * 5.0);
        let mut snow = Marks::default();
        snow.signs.push(sign(Bed::Snow, Kind::Print(Foot::Paw)));
        snow.advance(0.5, &Sky::Sun.sky());
        assert_eq!(snow.signs.len(), 1, "a snow print keeps on a fair day");
        snow.advance(1.0, &Sky::Snow.sky());
        assert!(snow.signs.is_empty(), "an hour of snowfall fills it");
    }

    #[test]
    fn a_herd_through_a_meadow_leaves_one_trail_not_a_heap() {
        let mut m = Marks::default();
        for i in 0..40 {
            // Ten deer each crossing the same four metres of line.
            let x = (i % 4) as f32;
            m.trample(&Step {
                pos: Vec3::new(x, 0.0, 0.0),
                yaw: 0.0,
                foot: Foot::Hoof,
                height: 1.4,
            });
        }
        assert!(m.signs.len() <= 5, "{} patches", m.signs.len());
    }

    #[test]
    fn the_oldest_go_first_when_full() {
        let mut m = Marks::default();
        for i in 0..MAX_MARKS + 10 {
            m.push(Sign {
                pos: Vec3::new(i as f32, 0.0, 0.0),
                ..sign(Bed::Snow, Kind::Print(Foot::Boot))
            });
        }
        assert_eq!(m.signs.len(), MAX_MARKS);
        assert!(m.signs.iter().all(|s| s.pos.x >= 10.0));
    }
}
