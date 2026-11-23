//! Polish: the racing line wears in over a race.
//! The road is cut into tiles a metre square, and every kart
//! whose wheels roll over a tile polishes it: each pass makes the tile 0.5%
//! faster for whoever drives on it afterwards, for the rest of the race.
//! Nothing about it is random. Seven karts on one line make the seventh 3.5%
//! quicker there than the first, and three laps of it make that line 10.5%
//! quicker by the end; a kart that takes its own line gets none of it.
//!
//! A kart's speed on polish: its top speed, power and drive force are its
//! own times one plus `PER_PASS` per pass, averaged over the tiles under its
//! wheels that are down. A pass is counted once per kart, however many of
//! its wheels cross the tile and however long it lingers on it, until it
//! has moved well on. A rival carried round out of sight, where the world
//! isn't loaded, polishes the line it is carried along like any other.
//!
//! Polished tiles darken in `STEPS`. The world is never changed for it: each
//! step a tile reaches is a `Mark` handed to the renderer, which keeps them
//! in a map of the world the terrain shader darkens the ground by
//! (`render::polish`), so polishing costs no voxel edits and no remeshing.

use crate::render::polish::Mark;
use crate::util::FxHashMap as HashMap;
use glam::Vec3;

/// How much faster each pass makes a tile.
pub const PER_PASS: f32 = 0.005;
/// The side of a tile, metres.
const TILE: f32 = 1.0;
/// Passes at which a tile darkens a step: once driven over, then more. A
/// field of eight on one line reaches the darkest by the end of the first lap.
const STEPS: [u16; 4] = [1, 3, 5, 8];
/// How much each step darkens the ground, as a share of its colour.
const SHADE: [f32; 4] = [0.1, 0.2, 0.3, 0.4];
/// Tiles a kart remembers having counted: about three metres of road under
/// both wheel tracks, so its front and back wheels count a tile once.
const RECENT: usize = 12;

type Tile = (i32, i32);

fn tile_of(p: Vec3) -> Tile {
    ((p.x / TILE).floor() as i32, (p.z / TILE).floor() as i32)
}

#[derive(Default)]
pub struct Polish {
    count: HashMap<Tile, u16>,
    /// By kart, the tiles it counted lately (a ring), and where it writes next.
    recent: Vec<[Option<Tile>; RECENT]>,
    next: Vec<usize>,
    /// Steps reached since the renderer last took them, and whether the
    /// race started over since (every mark before it is gone).
    marks: Vec<Mark>,
    cleared: bool,
}

impl Polish {
    /// Passes over the tile at `p`.
    pub fn passes(&self, p: Vec3) -> u16 {
        self.count.get(&tile_of(p)).copied().unwrap_or(0)
    }

    /// Kart `k` rolls on with its wheels on the road at `wheels`: each tile
    /// under one it hasn't counted lately gets a pass from it.
    pub fn roll(&mut self, k: usize, wheels: &[Vec3]) {
        if self.recent.len() <= k {
            self.recent.resize(k + 1, [None; RECENT]);
            self.next.resize(k + 1, 0);
        }
        for &w in wheels {
            let t = tile_of(w);
            if self.recent[k].contains(&Some(t)) {
                continue;
            }
            self.recent[k][self.next[k]] = Some(t);
            self.next[k] = (self.next[k] + 1) % RECENT;
            let n = self.count.entry(t).or_insert(0);
            *n = n.saturating_add(1);
            if let Some(step) = STEPS.iter().position(|&s| s == *n) {
                self.marks.push(Mark { at: [t.0 as f32 * TILE, t.1 as f32 * TILE], size: TILE, y: w.y, dark: SHADE[step] });
            }
        }
    }

    /// How much faster a kart goes on the polish under its wheels at
    /// `wheels`: 1 plus `PER_PASS` a pass, averaged over them.
    pub fn factor(&self, wheels: &[Vec3]) -> f32 {
        if wheels.is_empty() {
            return 1.0;
        }
        let sum: f32 = wheels.iter().map(|&w| self.passes(w) as f32).sum();
        1.0 + PER_PASS * sum / wheels.len() as f32
    }

    /// The marks laid since last asked, and whether the race started over
    /// first, for `Game` to hand the renderer.
    pub fn take_marks(&mut self) -> (bool, Vec<Mark>) {
        (std::mem::take(&mut self.cleared), std::mem::take(&mut self.marks))
    }

    /// A new race's: no tile polished, and the renderer told to drop every
    /// mark it holds.
    pub fn fresh() -> Polish {
        Polish { cleared: true, ..Polish::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arithmetic in the design: seven karts on one line make the seventh
    /// 3.5% faster there than the first; three laps of it, 10.5%.
    #[test]
    fn seven_karts_three_laps_is_ten_and_a_half_percent() {
        let mut p = Polish::default();
        let w = [Vec3::new(5.5, 0.0, 5.5)];
        for k in 0..7 {
            p.roll(k, &w);
        }
        assert!((p.factor(&w) - 1.035).abs() < 1e-6);
        // The same seven over it twice more: a fresh pass each lap, once
        // each has moved well on.
        for _ in 0..2 {
            for k in 0..7 {
                p.recent[k] = [None; RECENT];
                p.roll(k, &w);
            }
        }
        assert!((p.factor(&w) - 1.105).abs() < 1e-6);
    }

    /// A kart counts a tile once, whichever of its wheels cross it and
    /// however long it sits there; each step reached is marked once.
    #[test]
    fn a_kart_counts_a_tile_once_a_pass() {
        let mut p = Polish::default();
        let wheels = [Vec3::new(10.2, 1.0, 10.2), Vec3::new(10.8, 1.0, 10.8), Vec3::new(11.2, 1.0, 10.2), Vec3::new(11.8, 1.0, 10.8)];
        for _ in 0..30 {
            p.roll(0, &wheels);
        }
        assert!(p.count.values().all(|&n| n == 1), "{:?}", p.count);
        // A second and third kart over the same tiles: the second step.
        p.roll(1, &wheels);
        p.roll(2, &wheels);
        assert!(p.count.values().all(|&n| n == 3));
        assert!(p.factor(&wheels) > 1.0);
        let (cleared, marks) = p.take_marks();
        assert!(!cleared);
        // Two tiles, each marked at 1 pass and at 3.
        assert_eq!(marks.len(), 4, "{marks:?}");
        assert!(Polish::fresh().take_marks().0);
    }
}
