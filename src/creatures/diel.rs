//! Which hours an animal keeps. Every library creature names a period --
//! `diurnal`, `nocturnal`, `crepuscular` or `any` -- and this turns that plus
//! the clock into two numbers the rest of the code uses:
//!
//! - `abroad`: the share of a biome's quota that is out and about now. The
//!   spawner scales its counts by it and culls the surplus out of sight, so
//!   fireflies come up at dusk and starlings go quiet at dark.
//! - `drowsy`: how far outside its hours the ones you do meet are, which
//!   `walk.rs` turns into bedded-down animals that would rather lie there
//!   than graze.
//!
//! The curves run off the sun's elevation rather than the hour, so they line
//! up with the light the renderer draws and with the clock's own bands
//! (world/clock.rs): the day is long at the top and the night is hurried.
//!
//! Outside its hours a species is not gone, only scarce: `off_hours` is the
//! share still abroad at the wrong end of the clock, 0.08 by default.

use crate::world::clock::sun_elevation;

/// The part of the day an animal keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Period {
    /// Out in daylight. The default, and most of the library.
    #[default]
    Diurnal,
    /// Out after dark: owls, bats, fireflies, the corpse candle.
    Nocturnal,
    /// Out at dawn and dusk, and half out in between: deer, rabbits, foxes.
    Crepuscular,
    /// Keeps no hours at all (cathemeral): fish, whales, the fae.
    Cathemeral,
}

/// Every period, as a creature entry may name it.
pub const PERIODS: [&str; 4] = ["diurnal", "nocturnal", "crepuscular", "any"];

/// The share of a species abroad outside its own hours, where its entry
/// gives no `off_hours` of its own.
pub const OFF_HOURS: f32 = 0.08;

/// How much of a sleeper's quota stays put while the rest is away: a bedded
/// deer is still there to be walked up on, so walkers and snakes thin out far
/// less than birds and insects, which simply are not in the air.
const STAYS: f32 = 0.35;

/// That share, for the spawner: bad weather thins what is abroad, but what
/// sleeps where it stands is still lying there in it.
pub fn stays() -> f32 {
    STAYS
}

impl Period {
    pub fn name(self) -> &'static str {
        match self {
            Period::Diurnal => "diurnal",
            Period::Nocturnal => "nocturnal",
            Period::Crepuscular => "crepuscular",
            Period::Cathemeral => "any",
        }
    }

    pub fn parse(s: &str) -> Option<Period> {
        match s {
            "diurnal" => Some(Period::Diurnal),
            "nocturnal" => Some(Period::Nocturnal),
            "crepuscular" => Some(Period::Crepuscular),
            "any" | "cathemeral" => Some(Period::Cathemeral),
            _ => None,
        }
    }

    /// How active this period is at `hour`, 0 to 1, before `off_hours`: 1
    /// through the hours it keeps, 0 at the far end of the clock. A
    /// crepuscular animal never falls to 0, since it is about by day as well
    /// (at a little over half) and moves a little at night.
    pub fn activity(self, hour: f32) -> f32 {
        let e = sun_elevation(hour);
        match self {
            Period::Diurnal => day(e),
            Period::Nocturnal => night(e),
            Period::Crepuscular => twilight(e).max(0.55 * day(e)).max(0.3 * night(e)),
            Period::Cathemeral => 1.0,
        }
    }
}

fn ramp(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Daylight: full once the sun is up, gone by the end of civil twilight.
fn day(e: f32) -> f32 {
    ramp(-7.0, 0.0, e)
}

/// Dark: full once the sun is well down.
fn night(e: f32) -> f32 {
    1.0 - ramp(-9.0, -2.0, e)
}

/// The two edges of the day: from before first light (`clock::dawn()`, the sun
/// 10.6 degrees down) through the hour after sunrise, and the same in
/// reverse at the other end.
fn twilight(e: f32) -> f32 {
    ramp(-16.0, -10.0, e).min(1.0 - ramp(-1.0, 6.0, e))
}

/// The share of a species' biome quota that is abroad at `hour`: all of it
/// through its own hours, `off_hours` of it outside them.
pub fn abroad(p: Period, off_hours: f32, hour: f32) -> f32 {
    let off = off_hours.clamp(0.0, 1.0);
    off + (1.0 - off) * p.activity(hour)
}

/// The same, for an animal that sleeps where it stands rather than leaving:
/// part of the quota stays put, bedded down, however wrong the hour.
pub fn abroad_sleeping(p: Period, off_hours: f32, hour: f32) -> f32 {
    let a = abroad(p, off_hours, hour);
    STAYS + (1.0 - STAYS) * a
}

/// How far outside its hours a creature is now, 0 wide awake to 1 fast
/// asleep. `walk.rs` reads it when the activity scheduler picks what next,
/// and squares it, so an animal at the wrong end of the clock is down nearly
/// always while a deer half-drowsy at midday only now and then beds.
pub fn drowsy(p: Period, hour: f32) -> f32 {
    1.0 - p.activity(hour)
}

/// How many of `want` are abroad at `hour`, for the spawner. Never rounds a
/// species the biome wants away entirely while it is in its own hours.
pub fn quota(want: u32, share: f32) -> u32 {
    let n = (want as f32 * share.clamp(0.0, 1.0)).round() as u32;
    if n == 0 && share > 0.5 {
        1
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::clock::{dawn, dusk};

    const NIGHT: f32 = 1.0;
    const MIDDAY: f32 = 12.0;

    #[test]
    fn each_period_keeps_its_hours() {
        // Midday: the diurnal are all out, the nocturnal are not.
        assert!(Period::Diurnal.activity(MIDDAY) > 0.99);
        assert!(Period::Nocturnal.activity(MIDDAY) < 0.01);
        assert!(Period::Cathemeral.activity(MIDDAY) > 0.99);
        // Night: the other way about.
        assert!(Period::Diurnal.activity(NIGHT) < 0.01);
        assert!(Period::Nocturnal.activity(NIGHT) > 0.99);
        // The crepuscular peak at the ends of the day, and are half out at
        // midday rather than gone (deer bed down in the heat).
        let first = Period::Crepuscular.activity(dawn());
        assert!(first > 0.9, "crepuscular at dawn {first}");
        assert!(Period::Crepuscular.activity(dusk()) > 0.9);
        let noon = Period::Crepuscular.activity(MIDDAY);
        assert!((0.5..0.7).contains(&noon), "crepuscular at midday {noon}");
        assert!(Period::Crepuscular.activity(NIGHT) < 0.4);
    }

    #[test]
    fn off_hours_leaves_a_few_out() {
        let n = abroad(Period::Diurnal, OFF_HOURS, NIGHT);
        assert!((0.05..0.11).contains(&n), "{n}");
        assert!(abroad(Period::Diurnal, 0.0, NIGHT) < 0.01);
        assert!(abroad(Period::Diurnal, OFF_HOURS, MIDDAY) > 0.99);
        // A sleeper keeps a good part of its quota lying about.
        assert!(abroad_sleeping(Period::Diurnal, OFF_HOURS, NIGHT) > 0.35);
        assert!(abroad_sleeping(Period::Diurnal, OFF_HOURS, MIDDAY) > 0.99);
    }

    #[test]
    fn drowsiness_is_the_other_side_of_activity() {
        assert!(drowsy(Period::Diurnal, NIGHT) > 0.99);
        assert!(drowsy(Period::Diurnal, MIDDAY) < 0.01);
        assert!(drowsy(Period::Nocturnal, MIDDAY) > 0.99);
        assert_eq!(drowsy(Period::Cathemeral, NIGHT), 0.0);
    }

    #[test]
    fn quotas_round_but_dont_wipe_out_a_species_in_its_hours() {
        assert_eq!(quota(50, 1.0), 50);
        assert_eq!(quota(50, 0.0), 0);
        assert_eq!(quota(10, 0.08), 1);
        assert_eq!(quota(1, 0.08), 0);
        // One animal the biome wants, in its own hours, is still one animal.
        assert_eq!(quota(1, 0.6), 1);
    }

    #[test]
    fn names_round_trip() {
        for n in PERIODS {
            assert_eq!(Period::parse(n).unwrap().name(), n);
        }
        assert_eq!(Period::parse("cathemeral"), Some(Period::Cathemeral));
        assert_eq!(Period::parse("twilight"), None);
    }
}
