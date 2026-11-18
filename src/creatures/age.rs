//! The four ages of an animal: juvenile, adult, aged, deceased.
//!
//! A stage is rolled once when the individual spawns and never changes. It is
//! held in an `Age` component and shows up in three places: the individual's
//! `Morph` (a juvenile is smaller with a bigger head, an old one is greyer and
//! slower, src/variety.rs), the model it is drawn with (the deceased are drawn
//! with the skeleton derived in anim/bones.rs), and the crosshair label
//! (census.rs).
//!
//! Which ages a creature has follows from its size unless its library entry
//! says otherwise: anything knee-high or taller has young and old, and
//! anything half a metre or more leaves bones. Small animals leave nothing,
//! because a sparrow's skeleton is four voxels and reads as litter.

use crate::library::Ages;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stage {
    Juvenile,
    #[default]
    Adult,
    Aged,
    Dead,
}

impl Stage {
    /// The suffix on the crosshair label; adults get none.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Juvenile => " (juvenile)",
            Stage::Adult => "",
            Stage::Aged => " (aged)",
            Stage::Dead => " (deceased)",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stage::Juvenile => "juvenile",
            Stage::Adult => "adult",
            Stage::Aged => "aged",
            Stage::Dead => "deceased",
        }
    }
}

/// What this creature's individuals carry, resolved from the library entry and
/// its size.
#[derive(Clone, Copy, Debug)]
pub struct Stages {
    /// Body scale of a juvenile; 0 means the creature has no juvenile stage.
    pub juvenile: f32,
    pub aged: bool,
    /// Leaves a skeleton when it dies.
    pub bones: bool,
}

impl Default for Stages {
    fn default() -> Self {
        Self {
            juvenile: 0.0,
            aged: false,
            bones: false,
        }
    }
}

/// Knee-high and up has young and old; half a metre and up leaves bones.
const YOUNG_FROM_M: f32 = 0.4;
const BONES_FROM_M: f32 = 0.5;
pub const JUVENILE_SCALE: f32 = 0.62;

impl Stages {
    pub fn resolve(a: &Ages, height: f32) -> Self {
        let big = height >= YOUNG_FROM_M;
        Self {
            juvenile: a.juvenile.unwrap_or(if big { JUVENILE_SCALE } else { 0.0 }).clamp(0.0, 1.0),
            aged: a.aged.unwrap_or(big),
            bones: match a.bones.as_deref() {
                Some("full") => true,
                Some(_) => false,
                None => height >= BONES_FROM_M,
            },
        }
    }

    pub fn has_juvenile(&self) -> bool {
        self.juvenile > 0.0
    }
}

/// How much of a population is in each stage. The chosen numbers: 5% of animals die
/// on a given day and 5% are juvenile. Bones last half a day, so the standing
/// stock of skeletons is half of that 5%.
pub const JUVENILE_SHARE: f32 = 0.05;
pub const AGED_SHARE: f32 = 0.05;
pub const DEAD_SHARE: f32 = 0.025;

/// How long bones lie before they are gone: half a day, and the day is 25 real
/// minutes (world/clock.rs), so 750 seconds of `Entities::clock`.
pub const BONES_LAST_S: f32 = 750.0;
/// How long a fresh kill stays a body before it is bones.
pub const CARCASS_S: f32 = 60.0;

fn share(var: &str, default: f32) -> f32 {
    std::env::var(var).ok().and_then(|s| s.parse().ok()).map_or(default, |v: f32| v.clamp(0.0, 1.0))
}

/// The shares in use, with the dev overrides applied. `VG_DEAD_SHARE=0.3`
/// fills a field with skeletons to look at them.
pub fn shares() -> (f32, f32, f32) {
    (share("VG_JUVENILE_SHARE", JUVENILE_SHARE), share("VG_AGED_SHARE", AGED_SHARE), share("VG_DEAD_SHARE", DEAD_SHARE))
}

/// One individual's stage from a roll in 0..1. The bands are fixed, so turning
/// one stage off for a species does not change how often the others turn up.
pub fn roll(st: &Stages, roll: f32) -> Stage {
    let (juv, aged, dead) = shares();
    if st.bones && roll < dead {
        Stage::Dead
    } else if st.has_juvenile() && (0.25..0.25 + juv).contains(&roll) {
        Stage::Juvenile
    } else if st.aged && (0.5..0.5 + aged).contains(&roll) {
        Stage::Aged
    } else {
        Stage::Adult
    }
}

/// On every individual. `until` and `bones_at` are times on
/// `Entities::clock`; 0 means never.
#[derive(Clone, Copy, Debug)]
pub struct Age {
    pub stage: Stage,
    /// When this entity is removed (the deceased only).
    pub until: f32,
    /// When a body becomes a skeleton (a fresh kill only).
    pub bones_at: f32,
}

impl Age {
    pub fn new(stage: Stage) -> Self {
        Age { stage, until: 0.0, bones_at: 0.0 }
    }

    /// Bones already picked clean, lying for the rest of their half day.
    pub fn old_bones(now: f32, jitter: f32) -> Self {
        Age {
            stage: Stage::Dead,
            until: now + BONES_LAST_S * (0.4 + jitter * 0.6),
            bones_at: 0.0,
        }
    }

    /// A fresh kill: a body for a minute, then bones for half a day.
    pub fn fresh_kill(now: f32) -> Self {
        Age {
            stage: Stage::Dead,
            until: now + CARCASS_S + BONES_LAST_S,
            bones_at: now + CARCASS_S,
        }
    }

    pub fn dead(&self) -> bool {
        self.stage == Stage::Dead
    }

    /// Whether it is drawn as a skeleton yet, rather than as a body.
    pub fn boned(&self, now: f32) -> bool {
        self.stage == Stage::Dead && now >= self.bones_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::noise::{hash32, unit};

    fn ages(j: Option<f32>, a: Option<bool>, b: Option<&str>) -> Ages {
        Ages {
            juvenile: j,
            aged: a,
            bones: b.map(String::from),
        }
    }

    #[test]
    fn size_decides_which_ages_a_creature_has() {
        // A deer: young, old and bones.
        let deer = Stages::resolve(&ages(None, None, None), 1.4);
        assert!(deer.has_juvenile() && deer.aged && deer.bones);
        // A lizard: none of them.
        let lizard = Stages::resolve(&ages(None, None, None), 0.1);
        assert!(!lizard.has_juvenile() && !lizard.aged && !lizard.bones);
        // A starling-sized bird the entry has turned juveniles off for.
        let starling = Stages::resolve(&ages(Some(0.0), Some(false), Some("none")), 0.6);
        assert!(!starling.has_juvenile() && !starling.aged && !starling.bones);
    }

    #[test]
    fn the_stages_turn_up_about_as_often_as_their_shares() {
        let st = Stages {
            juvenile: JUVENILE_SCALE,
            aged: true,
            bones: true,
        };
        let n = 100_000;
        let mut seen = [0usize; 4];
        for k in 0..n {
            let r = unit(hash32(k));
            seen[match roll(&st, r) {
                Stage::Juvenile => 0,
                Stage::Aged => 1,
                Stage::Dead => 2,
                Stage::Adult => 3,
            }] += 1;
        }
        let f = |i: usize| seen[i] as f32 / n as f32;
        assert!((f(0) - JUVENILE_SHARE).abs() < 0.004, "{seen:?}");
        assert!((f(1) - AGED_SHARE).abs() < 0.004, "{seen:?}");
        assert!((f(2) - DEAD_SHARE).abs() < 0.003, "{seen:?}");
        assert!((f(3) - 0.875).abs() < 0.01, "{seen:?}");
    }

    #[test]
    fn turning_one_stage_off_leaves_the_others_alone() {
        // No bones: those individuals become adults, not juveniles.
        let no_bones = Stages {
            juvenile: JUVENILE_SCALE,
            aged: true,
            bones: false,
        };
        let n = 40_000;
        let mut juv = 0;
        for k in 0..n {
            if roll(&no_bones, unit(hash32(k))) == Stage::Juvenile {
                juv += 1;
            }
            assert_ne!(roll(&no_bones, unit(hash32(k))), Stage::Dead);
        }
        assert!((juv as f32 / n as f32 - JUVENILE_SHARE).abs() < 0.006);
    }

    #[test]
    fn a_kill_is_a_body_before_it_is_bones() {
        let a = Age::fresh_kill(100.0);
        assert!(a.dead() && !a.boned(100.0));
        assert!(a.boned(100.0 + CARCASS_S + 1.0));
        assert!(a.until > 100.0 + CARCASS_S);
        // Bones found lying in the field are bones from the start.
        assert!(Age::old_bones(100.0, 0.5).boned(100.0));
    }
}
