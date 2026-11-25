//! The live grid (notes/WORLD_SURVEY_PLAN.md, S2; beside the static survey
//! in world/survey, kept out of its module so editing this file leaves the
//! shipped survey files current): what is happening on the
//! land now, on an 8 m grid that changes every few seconds. Threat, noise
//! and scent are stamped where things happen and fade; carrion and who is
//! where are counted afresh each tick. It is what the world knows, not what
//! any one animal knows: a director (a herd, a horde) may read it, a single
//! body reads its own senses.
//!
//! Stamps queue as they come and are laid on the next tick, every `TICK_S`
//! seconds, so a stamp costs a push. A tick decays every channel by its own
//! half-life, carries the scent down the wind and spreads it a little, then
//! lays the queued stamps and the counts. Nothing is saved: a loaded game
//! starts with a quiet land.

use glam::{Vec2, Vec3};

/// Edge of one live cell, metres.
pub const CELL_M: f32 = 8.0;

/// Seconds between ticks.
pub const TICK_S: f32 = 5.0;

/// The most stamps held between ticks; past this, new ones are dropped (a
/// tick lays them all, so this is only a guard against a runaway source).
const MAX_STAMPS: usize = 20_000;

/// How a channel is kept from tick to tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Keep {
    /// Stamped as things happen; fades by its half-life.
    Fades,
    /// Counted afresh every tick (`Count`): what is there now.
    Counted,
    /// Counted every tick, and what was there fades by its half-life rather
    /// than vanishing (a carcass's smell outlasts the carcass).
    Lingers,
}

macro_rules! live_chans {
    ($($v:ident = $name:literal, $keep:ident, $half:expr, $full:expr, $about:literal;)*) => {
        /// One channel of the live grid.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum LiveChan { $($v),* }

        impl LiveChan {
            pub const ALL: &'static [LiveChan] = &[$(LiveChan::$v),*];

            pub fn name(self) -> &'static str {
                match self { $(LiveChan::$v => $name),* }
            }

            pub fn keep(self) -> Keep {
                match self { $(LiveChan::$v => Keep::$keep),* }
            }

            /// Seconds for a stamp to fade to half (0 for counted channels).
            pub fn half_life(self) -> f32 {
                match self { $(LiveChan::$v => $half),* }
            }

            /// The level the map draws at full colour.
            pub fn full(self) -> f32 {
                match self { $(LiveChan::$v => $full),* }
            }

            /// One line for the map's legend.
            pub fn about(self) -> &'static str {
                match self { $(LiveChan::$v => $about),* }
            }
        }
    };
}

live_chans! {
    Threat = "threat", Fades, 120.0, 1.0, "DANGER: A PREDATOR HUNTING, ANIMALS BOLTING, A PERSON, SHOTS";
    Noise = "noise", Fades, 15.0, 1.0, "NOISE: FOOTSTEPS, ENGINES, GUNFIRE";
    Scent = "scent", Fades, 60.0, 1.0, "SCENT OF THE PLAYER AND PREDATORS, CARRIED DOWN THE WIND";
    Carrion = "carrion", Lingers, 1800.0, 1.0, "CARCASSES, AND THE SMELL THEY LEAVE";
    Grazers = "grazers", Counted, 0.0, 4.0, "HOW MANY ANIMALS THAT HUNT NOTHING ARE HERE";
    Predators = "predators", Counted, 0.0, 2.0, "HOW MANY ANIMALS THAT HUNT ARE HERE";
    Disturbed = "disturbed", Fades, 10800.0, 1.0, "GROUND TRAMPLED OR BROKEN";
}

impl LiveChan {
    pub fn parse(s: &str) -> Option<LiveChan> {
        LiveChan::ALL.iter().copied().find(|c| c.name() == s)
    }
}

/// Something laid on the grid: `amount` at the middle, falling off to
/// nothing at `radius` metres.
#[derive(Clone, Copy, Debug)]
pub struct Stamp {
    pub chan: LiveChan,
    pub at: Vec2,
    pub amount: f32,
    pub radius: f32,
}

#[derive(Default)]
pub struct Live {
    /// Cells along each side; 0 before a world is set.
    pub n: usize,
    grids: Vec<Vec<f32>>,
    stamps: Vec<Stamp>,
    /// What each counted channel holds this tick, filled by the game before
    /// `tick` (`count`).
    counts: Vec<Stamp>,
    /// Seconds since the last tick.
    since: f32,
    /// Ticks run, and how long the last took, for the map's legend.
    pub ticks: u64,
    pub tick_ms: f32,
    /// Stamps dropped for being past `MAX_STAMPS` since the world was set.
    pub dropped: usize,
    /// The caller's own clock of the last shot laid (game/live.rs), so each
    /// shot is laid once.
    pub heard_to: f32,
}

impl Live {
    /// A quiet grid over a world `size` metres across.
    pub fn new(size: f32) -> Live {
        let n = (size / CELL_M).ceil().max(1.0) as usize;
        Live { n, grids: vec![vec![0.0; n * n]; LiveChan::ALL.len()], ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Cell index of a world point, clamped to the grid.
    pub fn idx(&self, x: f32, z: f32) -> usize {
        let c = |v: f32| ((v / CELL_M) as i32).clamp(0, self.n as i32 - 1) as usize;
        c(z) * self.n + c(x)
    }

    pub fn grid(&self, c: LiveChan) -> &[f32] {
        &self.grids[c as usize]
    }

    pub fn value(&self, c: LiveChan, x: f32, z: f32) -> f32 {
        if self.n == 0 {
            return 0.0;
        }
        self.grids[c as usize][self.idx(x, z)]
    }

    /// Queue a stamp for the next tick.
    pub fn stamp(&mut self, chan: LiveChan, at: Vec3, amount: f32, radius: f32) {
        if self.n == 0 || amount <= 0.0 {
            return;
        }
        if self.stamps.len() >= MAX_STAMPS {
            self.dropped += 1;
            return;
        }
        self.stamps.push(Stamp { chan, at: Vec2::new(at.x, at.z), amount, radius: radius.max(CELL_M * 0.5) });
    }

    /// Something that goes on (a person standing about, an animal running),
    /// laid every tick: scaled so that, laid every tick, it settles at
    /// `level` rather than piling up. A one-off (a shot) is a `stamp`.
    pub fn hold(&mut self, chan: LiveChan, at: Vec3, level: f32, radius: f32) {
        let fade = 0.5f32.powf(TICK_S / chan.half_life().max(TICK_S));
        self.stamp(chan, at, level * (1.0 - fade), radius);
    }

    /// Something a counted channel holds this tick (an animal, a carcass).
    pub fn count(&mut self, chan: LiveChan, at: Vec3, amount: f32) {
        if self.n > 0 {
            self.counts.push(Stamp { chan, at: Vec2::new(at.x, at.z), amount, radius: 0.0 });
        }
    }

    /// Run the clock on `dt` seconds. True when a tick is due: the caller
    /// then fills the counts (`count`) and calls `tick`.
    pub fn due(&mut self, dt: f32) -> bool {
        if self.n == 0 {
            return false;
        }
        self.since += dt;
        self.since >= TICK_S
    }

    /// One tick. `wind` is how far the scent drifts per second, metres, on
    /// the ground plane (x, z).
    pub fn tick(&mut self, wind: Vec2) {
        let t = std::time::Instant::now();
        let dt = self.since.max(TICK_S);
        self.since = 0.0;
        let n = self.n;
        for &c in LiveChan::ALL {
            let g = &mut self.grids[c as usize];
            match c.keep() {
                Keep::Counted => g.iter_mut().for_each(|v| *v = 0.0),
                Keep::Fades | Keep::Lingers => {
                    let k = 0.5f32.powf(dt / c.half_life());
                    g.iter_mut().for_each(|v| *v = if *v * k < 1e-3 { 0.0 } else { *v * k });
                }
            }
        }
        drift(&mut self.grids[LiveChan::Scent as usize], n, wind * dt / CELL_M);
        for s in std::mem::take(&mut self.stamps) {
            lay(&mut self.grids[s.chan as usize], n, s);
        }
        let mut held = vec![Vec::new(); LiveChan::ALL.len()];
        for s in std::mem::take(&mut self.counts) {
            held[s.chan as usize].push(s);
        }
        for &c in LiveChan::ALL {
            let list = &held[c as usize];
            let g = &mut self.grids[c as usize];
            match c.keep() {
                Keep::Counted => {
                    for s in list {
                        let k = cell(n, s.at);
                        g[k] += s.amount;
                    }
                }
                Keep::Lingers => {
                    let mut now = vec![0f32; n * n];
                    for s in list {
                        now[cell(n, s.at)] += s.amount;
                    }
                    for (v, m) in g.iter_mut().zip(now) {
                        *v = v.max(m);
                    }
                }
                Keep::Fades => {}
            }
        }
        self.ticks += 1;
        self.tick_ms = t.elapsed().as_secs_f32() * 1000.0;
    }

    /// Each channel's total and highest cell, for `--eco-dump`.
    pub fn stats(&self) -> Vec<(&'static str, f32, f32)> {
        LiveChan::ALL.iter().map(|&c| {
            let g = self.grid(c);
            (c.name(), g.iter().sum(), g.iter().copied().fold(0.0, f32::max))
        }).collect()
    }
}

fn cell(n: usize, at: Vec2) -> usize {
    let c = |v: f32| ((v / CELL_M) as i32).clamp(0, n as i32 - 1) as usize;
    c(at.y) * n + c(at.x)
}

/// Lay one stamp: its amount at the middle, falling off linearly to its
/// radius, added to what is there. No cell goes past 10.
fn lay(g: &mut [f32], n: usize, s: Stamp) {
    let r = s.radius;
    let lo = |v: f32| (((v - r) / CELL_M).floor().max(0.0) as usize).min(n);
    let hi = |v: f32| (((v + r) / CELL_M).ceil().max(0.0) as usize).min(n);
    for z in lo(s.at.y)..hi(s.at.y) {
        for x in lo(s.at.x)..hi(s.at.x) {
            let c = Vec2::new((x as f32 + 0.5) * CELL_M, (z as f32 + 0.5) * CELL_M);
            let d = c.distance(s.at);
            if d < r {
                let v = &mut g[z * n + x];
                *v = (*v + s.amount * (1.0 - d / r)).min(10.0);
            }
        }
    }
}

/// Carry a grid `by` cells down the wind (sampling upwind, so nothing is
/// lost between cells), then spread it a little: each cell keeps 60% and
/// shares the rest with its four neighbours. What drifts off the edge is
/// gone.
fn drift(g: &mut [f32], n: usize, by: Vec2) {
    if g.iter().all(|&v| v == 0.0) {
        return;
    }
    let src = g.to_vec();
    let at = |x: i32, z: i32| if x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n { src[z as usize * n + x as usize] } else { 0.0 };
    for z in 0..n {
        for x in 0..n {
            let (sx, sz) = (x as f32 - by.x, z as f32 - by.y);
            let (x0, z0) = (sx.floor(), sz.floor());
            let (fx, fz) = (sx - x0, sz - z0);
            let (x0, z0) = (x0 as i32, z0 as i32);
            let a = at(x0, z0) * (1.0 - fx) + at(x0 + 1, z0) * fx;
            let b = at(x0, z0 + 1) * (1.0 - fx) + at(x0 + 1, z0 + 1) * fx;
            g[z * n + x] = a * (1.0 - fz) + b * fz;
        }
    }
    let src = g.to_vec();
    let at = |x: i32, z: i32| if x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n { src[z as usize * n + x as usize] } else { 0.0 };
    for z in 0..n as i32 {
        for x in 0..n as i32 {
            let near = at(x - 1, z) + at(x + 1, z) + at(x, z - 1) + at(x, z + 1);
            g[z as usize * n + x as usize] = at(x, z) * 0.6 + near * 0.1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_quiet(l: &mut Live, wind: Vec2) {
        assert!(l.due(TICK_S));
        l.tick(wind);
    }

    #[test]
    fn a_stamp_lands_on_the_next_tick_and_fades_by_its_half_life() {
        let mut l = Live::new(160.0);
        let at = Vec3::new(84.0, 0.0, 84.0);
        l.stamp(LiveChan::Threat, at, 1.0, 20.0);
        assert_eq!(l.value(LiveChan::Threat, 84.0, 84.0), 0.0);
        tick_quiet(&mut l, Vec2::ZERO);
        let first = l.value(LiveChan::Threat, 84.0, 84.0);
        assert!(first > 0.8, "{first}");
        // 120 s is 24 ticks: half left.
        for _ in 0..24 {
            tick_quiet(&mut l, Vec2::ZERO);
        }
        let later = l.value(LiveChan::Threat, 84.0, 84.0);
        assert!((later / first - 0.5).abs() < 0.02, "{later} of {first}");
        // Nothing past the radius.
        assert_eq!(l.value(LiveChan::Threat, 84.0 + 40.0, 84.0), 0.0);
    }

    #[test]
    fn a_held_source_settles_at_its_level() {
        let mut l = Live::new(80.0);
        let at = Vec3::new(20.0, 0.0, 20.0);
        for _ in 0..400 {
            l.hold(LiveChan::Threat, at, 0.5, 16.0);
            tick_quiet(&mut l, Vec2::ZERO);
        }
        let v = l.value(LiveChan::Threat, 20.0, 20.0);
        assert!((v - 0.5).abs() < 0.03, "{v}");
    }

    #[test]
    fn scent_drifts_down_the_wind() {
        let mut l = Live::new(400.0);
        let at = Vec3::new(100.0, 0.0, 200.0);
        for _ in 0..6 {
            l.stamp(LiveChan::Scent, at, 1.0, 8.0);
            // 2 m/s toward +x: 10 m a tick.
            tick_quiet(&mut l, Vec2::new(2.0, 0.0));
        }
        let down = l.value(LiveChan::Scent, 140.0, 200.0);
        let up = l.value(LiveChan::Scent, 60.0, 200.0);
        assert!(down > 0.05 && down > up * 4.0, "down {down}, up {up}");
    }

    #[test]
    fn counts_are_what_is_there_now_and_carrion_lingers() {
        let mut l = Live::new(80.0);
        let at = Vec3::new(20.0, 0.0, 20.0);
        l.count(LiveChan::Grazers, at, 1.0);
        l.count(LiveChan::Grazers, at, 1.0);
        l.count(LiveChan::Carrion, at, 1.0);
        tick_quiet(&mut l, Vec2::ZERO);
        assert_eq!(l.value(LiveChan::Grazers, 20.0, 20.0), 2.0);
        assert_eq!(l.value(LiveChan::Carrion, 20.0, 20.0), 1.0);
        // Gone: the grazers are gone, the carrion's smell stays a while.
        tick_quiet(&mut l, Vec2::ZERO);
        assert_eq!(l.value(LiveChan::Grazers, 20.0, 20.0), 0.0);
        assert!(l.value(LiveChan::Carrion, 20.0, 20.0) > 0.99);
    }
}
