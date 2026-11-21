//! Locomotion verbs: the names a library creature can give as its
//! `behaviour.movement`, and what each means to the game. This is the one
//! list of verbs. The library validates against `MOVEMENTS`, the spawner
//! hands each creature to its verb's mover, and the animation reads the
//! verb's default activity table and wingbeat.
//!
//! A new verb that moves like an existing one is a new row here. A verb that
//! needs new motion is a new `Mover` variant, a mover module beside walk.rs
//! and circle.rs (a spawn function and an update function), and an arm in
//! spawn.rs and in `Entities::update`.

use crate::anim::flight::Pattern;
use crate::anim::life::Routine;

/// Where a snake lives, which is the only thing that tells the three snake
/// verbs apart (creatures/snake.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Habit {
    /// On the ground: crawling, coiled, basking.
    Ground,
    /// Up trees: climbing trunks and lying along branches.
    Tree,
    /// In rivers, lakes and the sea, at the surface.
    Water,
}

/// Which code moves a creature, with the settings that tell verbs sharing a
/// mover apart.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mover {
    /// On the ground with a physics body (walk.rs).
    Walk {
        /// Bounces along as it goes (rabbits).
        hops: bool,
        /// Never moves off the spot.
        still: bool,
        /// Keeps to the water's edge and may step into water.
        wades: bool,
        /// Stops and draws into its shell when the player comes close,
        /// instead of running (tortoises and turtles).
        hides: bool,
        /// Lies out by the sea and, when startled, humps to the water and
        /// dives in, gone with a splash (seals).
        dives: bool,
    },
    /// Circling a fixed centre in the air (circle.rs). Radius and speed are
    /// each a low value plus a random share of a spread.
    Circle { radius: (f32, f32), speed: (f32, f32) },
    /// A flock of birds (flock.rs). `soar`: one bird alone, circling wide
    /// and high, never landing. `glide`: a few big seabirds sweeping wide
    /// and low, that come down together on the highest ground near.
    Flock { soar: bool, glide: bool },
    /// Small birds hopping between shrub tops (flit.rs).
    Flit,
    /// A school of fish (swim.rs).
    School,
    /// A whale offshore that spouts and breaches (swim.rs).
    Breach,
    /// Waterfowl floating near the shore that fly off together (fowl.rs).
    Paddle,
    /// Woodpeckers on tree trunks (woodpecker.rs).
    Cling,
    /// Insects hovering at flowers, or over water if `water` (insects.rs).
    Buzz { water: bool },
    /// A snake, posed along a path rather than walked (snake.rs).
    Slither { habit: Habit },
    /// An octopus on the sea floor, its arms let down over what is under
    /// them (octopus.rs).
    Creep,
}

pub struct Verb {
    pub name: &'static str,
    pub mover: Mover,
    /// Default activity table for rigged walkers (anim/life.rs).
    pub routine: Routine,
    /// Default wingbeat for rigged birds when the creature names no `flight`.
    pub flight: Pattern,
}

const fn walk(name: &'static str, routine: Routine, hops: bool, still: bool, wades: bool) -> Verb {
    Verb {
        name,
        mover: Mover::Walk {
            hops,
            still,
            wades,
            hides: false,
            dives: false,
        },
        routine,
        flight: Pattern::Flap,
    }
}

const fn other(name: &'static str, mover: Mover, flight: Pattern) -> Verb {
    Verb {
        name,
        mover,
        routine: Routine::Graze,
        flight,
    }
}

/// Every verb. The first is what an unknown name falls back to.
pub const VERBS: [Verb; 27] = [
    walk("graze", Routine::Graze, false, false, false),
    walk("hop", Routine::Hop, true, false, false),
    // Hops, and keeps to the water edge: frogs and toads.
    walk("splash", Routine::Hop, true, false, true),
    walk("skulk", Routine::Skulk, false, false, false),
    walk("wade", Routine::Graze, false, false, true),
    other(
        "circle",
        Mover::Circle {
            radius: (8.0, 14.0),
            speed: (6.0, 3.0),
        },
        Pattern::FlapGlide,
    ),
    other(
        "flutter",
        Mover::Circle {
            radius: (1.5, 3.0),
            speed: (2.0, 1.5),
        },
        Pattern::Hover,
    ),
    // Flutter's slow cousin: a light that drifts rather than flickers, on a
    // wider and much lazier circle (corpse candles, will-o'-the-wisps).
    other(
        "drift",
        Mover::Circle {
            radius: (4.0, 6.0),
            speed: (0.45, 0.35),
        },
        Pattern::Hover,
    ),
    walk("still", Routine::Still, false, true, false),
    other("flock", Mover::Flock { soar: false, glide: false }, Pattern::FlapGlide),
    other("soar", Mover::Flock { soar: true, glide: false }, Pattern::Soar),
    // Albatrosses: wide low sweeps over the sea, roosting on high ground.
    other("glide", Mover::Flock { soar: false, glide: true }, Pattern::Soar),
    other("flit", Mover::Flit, Pattern::Bound),
    other("school", Mover::School, Pattern::Flap),
    other("breach", Mover::Breach, Pattern::Flap),
    // A slow plodder that hides in its shell rather than run.
    Verb {
        name: "plod",
        mover: Mover::Walk {
            hops: false,
            still: false,
            wades: false,
            hides: true,
            dives: false,
        },
        routine: Routine::Still,
        flight: Pattern::Flap,
    },
    // A sea turtle on the beach: hides in its shell when you come close,
    // and now and then crawls back to the sea and swims off (its `school`
    // child); swimmers haul out again onto the sand.
    Verb {
        name: "bask",
        mover: Mover::Walk {
            hops: false,
            still: false,
            wades: true,
            hides: true,
            dives: true,
        },
        routine: Routine::Still,
        flight: Pattern::Flap,
    },
    // Hauled out on the shore; startled, it makes for the sea and dives.
    Verb {
        name: "haulout",
        mover: Mover::Walk {
            hops: false,
            still: false,
            wades: true,
            hides: false,
            dives: true,
        },
        routine: Routine::Still,
        flight: Pattern::Flap,
    },
    // Wide, slow circles a few metres up with rippling wings (rays that
    // fly; rigged with plan "ray").
    other(
        "undulate",
        Mover::Circle {
            radius: (7.0, 9.0),
            speed: (1.6, 1.0),
        },
        Pattern::Hover,
    ),
    other("paddle", Mover::Paddle, Pattern::Flap),
    other("cling", Mover::Cling, Pattern::Bound),
    other("buzz", Mover::Buzz { water: false }, Pattern::Hover),
    other("dart", Mover::Buzz { water: true }, Pattern::Hover),
    // Snakes (snake.rs), by where they live.
    other("slither", Mover::Slither { habit: Habit::Ground }, Pattern::Flap),
    other("climb", Mover::Slither { habit: Habit::Tree }, Pattern::Flap),
    other("swim", Mover::Slither { habit: Habit::Water }, Pattern::Flap),
    other("creep", Mover::Creep, Pattern::Flap),
];

/// The verb names, for validation.
pub const MOVEMENTS: [&str; VERBS.len()] = {
    let mut out = [""; VERBS.len()];
    let mut i = 0;
    while i < VERBS.len() {
        out[i] = VERBS[i].name;
        i += 1;
    }
    out
};

/// The verb named `name`; an unknown name walks and grazes (validation
/// reports it).
pub fn verb(name: &str) -> &'static Verb {
    VERBS.iter().find(|v| v.name == name).unwrap_or(&VERBS[0])
}

impl Verb {
    pub fn hops(&self) -> bool {
        matches!(self.mover, Mover::Walk { hops: true, .. })
    }
    pub fn still(&self) -> bool {
        matches!(self.mover, Mover::Walk { still: true, .. })
    }
    pub fn wades(&self) -> bool {
        matches!(self.mover, Mover::Walk { wades: true, .. })
    }
    pub fn dives(&self) -> bool {
        matches!(self.mover, Mover::Walk { dives: true, .. })
    }
    pub fn hides(&self) -> bool {
        matches!(self.mover, Mover::Walk { hides: true, .. })
    }
    /// A snake, whatever its habit.
    pub fn slithers(&self) -> bool {
        matches!(self.mover, Mover::Slither { .. })
    }
    /// Moved by flock.rs, circle.rs or flit.rs: animated as a bird if rigged.
    pub fn flies(&self) -> bool {
        matches!(self.mover, Mover::Flock { .. } | Mover::Circle { .. } | Mover::Flit | Mover::Paddle | Mover::Cling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_unknown_names_graze() {
        for (i, a) in MOVEMENTS.iter().enumerate() {
            assert!(!MOVEMENTS[i + 1..].contains(a), "{a} twice");
            assert_eq!(verb(a).name, *a);
        }
        assert_eq!(verb("teleport").name, "graze");
    }
}
