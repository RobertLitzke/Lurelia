//! The tide (world.json `tide`, GUIDE.md "Tides"): the open sea falls and
//! rises with the clock, and the rivers running into it fall with it near
//! their mouths. Lakes, pools below sea level that the sea cannot reach, and
//! rivers above a waterfall or well above the sea keep their level.
//!
//! A world's `sea_level_m` is high water: everything the generator placed
//! against the sea (beaches, shore plants, jetties, buildings) was placed
//! against that level, so the tide only ever goes out from it and nothing on
//! dry land is flooded. Low water is `range` metres below it.

use serde::Deserialize;
use std::sync::atomic::{AtomicU32, Ordering};

/// world.json `tide`: `{"range": 2.5}`, with the hour of the first high
/// water and the length of a tide if the defaults do not suit.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TideDef {
    /// Metres between high and low water. 0 is no tide.
    pub range: f32,
    /// The hour of high water on the first day (0 to 24).
    #[serde(default = "default_high")]
    pub high: f32,
    /// Game hours from one high water to the next: 18, three quarters of a
    /// day, unless the world says otherwise. The sea is out at its lowest
    /// half that after each high.
    #[serde(default = "default_period")]
    pub period_h: f32,
}

fn default_high() -> f32 {
    6.0
}
fn default_period() -> f32 {
    18.0
}

impl TideDef {
    /// How far below high water the sea stands `hours` of game clock after
    /// the first midnight (the day count times 24, plus the hour), metres: 0
    /// at high water, `range` at low. The period need not divide the day, so
    /// each day's tides come later than the last's, and a screenshot at a
    /// given hour on the first day always shows the same tide.
    pub fn drop_at(&self, hours: f32) -> f32 {
        let period = self.period_h.max(1.0);
        let phase = ((hours - self.high) / period).rem_euclid(1.0) * std::f32::consts::TAU;
        self.range.max(0.0) * 0.5 * (1.0 - phase.cos())
    }
}

/// What a built world knows of its tide (`Fields::tide`): which cells are
/// open sea, the range, and how far the sea has fallen now. The game sets
/// the fall every tick; the world is shared, so it is held in an atomic.
#[derive(Default, Debug)]
pub struct Tide {
    /// One bit per field cell: open sea, below `Fields::sea` and joined to the
    /// edge of the map through cells below it. Empty when the world has no
    /// tide, or no sea that reaches the edge.
    ocean: Vec<u64>,
    /// Metres between high and low water; 0 with no tide.
    pub range: f32,
    /// Metres below high water the sea stands now (f32 bits).
    drop: AtomicU32,
}

impl Tide {
    /// A tide of `range` over the cells `ocean` marks (`n` x `n`, row by row).
    /// None of it holds when no cell is open sea.
    pub fn new(range: f32, ocean: &[bool]) -> Self {
        let mut bits = vec![0u64; ocean.len().div_ceil(64)];
        let mut any = false;
        for (i, _) in ocean.iter().enumerate().filter(|(_, o)| **o) {
            bits[i / 64] |= 1 << (i % 64);
            any = true;
        }
        if !any || range <= 0.0 {
            return Self::default();
        }
        Self { ocean: bits, range, drop: AtomicU32::new(0) }
    }

    /// True when the sea here rises and falls.
    pub fn on(&self) -> bool {
        self.range > 0.0 && !self.ocean.is_empty()
    }

    /// Whether field cell `i` is open sea.
    pub fn ocean(&self, i: usize) -> bool {
        self.ocean.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    pub fn drop(&self) -> f32 {
        f32::from_bits(self.drop.load(Ordering::Relaxed))
    }

    pub fn set_drop(&self, d: f32) {
        if self.on() {
            self.drop.store(d.clamp(0.0, self.range).to_bits(), Ordering::Relaxed);
        }
    }

    /// How far river water standing at `level` falls now, metres, where it
    /// may fall at most `ebb` (`Wet::ebb`: 0 for water the tide never
    /// reaches). `high` is high water. Water at the sea's own level falls
    /// with it; the fall shrinks to nothing a full range above the sea, and
    /// it is linear in between so the river never runs uphill (`river_fall`
    /// in scene.wgsl is the same).
    pub fn river_fall(&self, level: f32, high: f32, ebb: f32) -> f32 {
        if ebb <= 0.0 || !self.on() {
            return 0.0;
        }
        let taper = (1.0 - (level - high).max(0.0) / self.range).max(0.0);
        (self.drop() * taper).min(ebb)
    }
}

/// Open sea: every cell below `sea` that is joined to the edge of the map
/// through cells below it (four ways round). A pool in a hollow below sea
/// level that the sea cannot reach is not open sea.
pub fn ocean_cells(n: usize, h: &[f32], sea: f32) -> Vec<bool> {
    let mut seen = vec![false; n * n];
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !seen[i] && h[i] < sea {
            seen[i] = true;
            stack.push(i);
        }
    };
    for k in 0..n {
        push(k, &mut seen, &mut stack);
        push((n - 1) * n + k, &mut seen, &mut stack);
        push(k * n, &mut seen, &mut stack);
        push(k * n + n - 1, &mut seen, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, z) = (i % n, i / n);
        if x > 0 {
            push(i - 1, &mut seen, &mut stack);
        }
        if x + 1 < n {
            push(i + 1, &mut seen, &mut stack);
        }
        if z > 0 {
            push(i - n, &mut seen, &mut stack);
        }
        if z + 1 < n {
            push(i + n, &mut seen, &mut stack);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_water_at_the_set_hour_and_low_between() {
        let t = TideDef { range: 3.0, high: 6.0, period_h: 18.0 };
        assert!(t.drop_at(6.0).abs() < 1e-4);
        assert!((t.drop_at(15.0) - 3.0).abs() < 1e-4);
        // The next high is on the second day, at midnight.
        assert!(t.drop_at(24.0).abs() < 1e-4);
        assert!((t.drop_at(33.0) - 3.0).abs() < 1e-4);
        // Before the first high, going back in time is the same curve.
        assert!((t.drop_at(0.0) - t.drop_at(18.0)).abs() < 1e-4);
        // Smooth across midnight: the clock is counted in days, not wrapped.
        assert!((t.drop_at(23.999) - t.drop_at(24.0)).abs() < 1e-3);
    }

    #[test]
    fn a_hollow_the_sea_cannot_reach_is_not_ocean() {
        // 6 x 6: sea along the west edge, a hollow in the middle walled off.
        let n = 6;
        let mut h = vec![10.0f32; n * n];
        for z in 0..n {
            h[z * n] = 0.0;
        }
        h[2 * n + 3] = 0.0;
        let o = ocean_cells(n, &h, 5.0);
        assert!(o[0] && o[3 * n]);
        assert!(!o[2 * n + 3]);
        assert!(!o[1]);
    }

    #[test]
    fn rivers_fall_with_the_sea_near_it_and_not_far_above() {
        let ocean = vec![true; 4];
        let t = Tide::new(2.0, &ocean);
        t.set_drop(2.0);
        // At the sea's own level: the whole fall, up to what the river may lose.
        assert!((t.river_fall(10.0, 10.0, 5.0) - 2.0).abs() < 1e-5);
        assert!((t.river_fall(10.0, 10.0, 0.8) - 0.8).abs() < 1e-5);
        // Half a range up, half; a range up, none; never for a lake.
        assert!((t.river_fall(11.0, 10.0, 5.0) - 1.0).abs() < 1e-5);
        assert_eq!(t.river_fall(12.5, 10.0, 5.0), 0.0);
        assert_eq!(t.river_fall(10.0, 10.0, 0.0), 0.0);
        // Downstream never stands higher than upstream.
        let a = 11.0 - t.river_fall(11.0, 10.0, 5.0);
        let b = 10.5 - t.river_fall(10.5, 10.0, 5.0);
        assert!(b <= a);
        // No sea, no tide.
        let dry = Tide::new(2.0, &[false; 4]);
        dry.set_drop(2.0);
        assert!(!dry.on() && dry.drop() == 0.0);
    }
}
