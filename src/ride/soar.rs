//! Scoring a glide: how close it is flown, and what it threads.
//!
//! Points come almost entirely from proximity. A fan of rays round the flyer
//! finds the nearest surface each step; distance flown earns in proportion to
//! how close that is, so cruising the open sky earns nothing however long the
//! boost keeps it up. A multiplier charges while something is within
//! `CLOSE_D` and bleeds away out in the open, and lump sums land for the
//! things that take nerve: a near miss, a gap taken between two walls, a pass
//! under an arch, a skim along the ground, a graze survived, a barrel roll.
//!
//! A run opens when the wings open and banks on touchdown. Crashing banks
//! part of it. The best run is kept per world in `progress.json`.

use super::{smoothstep, Mode};
use crate::world::store::Store;
use glam::Vec3;

/// How far the probes look, metres.
const REACH: f32 = 14.0;
/// Nearest surface at or inside this earns at full rate...
const FULL_D: f32 = 1.0;
/// ...and nothing at or beyond this.
const FADE_D: f32 = 12.0;
/// Something this near charges the multiplier.
const CLOSE_D: f32 = 8.0;
/// Nothing within this, for `RESET_T` seconds, resets the multiplier.
const RESET_D: f32 = 25.0;
const RESET_T: f32 = 3.0;
/// Seconds of close flying to charge the multiplier from 1x to `MAX_MULT`.
const CHARGE: f32 = 6.0;
const MAX_MULT: f32 = 8.0;
/// Seconds for the multiplier to fall halfway back to 1x out in the open.
const BLEED_HALF: f32 = 2.0;
/// Points per metre flown at full proximity and 1x.
const RATE: f32 = 8.0;
/// How long a call-out stays on screen.
const CALL_SECS: f32 = 1.6;

/// The lump sums, worst to best. A bonus fires at most once per `cool`
/// seconds, and firing one holds every lesser one off for `SHADE` seconds so
/// a threaded gap doesn't also pay out as a near miss.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bonus {
    Close,
    Roll,
    Graze,
    Thread,
    Skim,
    Under,
    Needle,
}

const KINDS: usize = 7;
/// Lesser bonuses are held off this long after a better one fires.
const SHADE: f32 = 1.5;

impl Bonus {
    /// Name, points, how much it adds to the multiplier, and its own cooldown.
    const fn spec(self) -> (&'static str, u32, f32, f32) {
        match self {
            Bonus::Close => ("CLOSE", 200, 0.25, 2.0),
            Bonus::Roll => ("ROLL", 300, 0.25, 2.0),
            Bonus::Graze => ("GRAZE", 500, 0.25, 1.5),
            Bonus::Thread => ("THREAD", 1_000, 0.5, 2.5),
            Bonus::Skim => ("SKIM", 1_500, 0.5, 3.5),
            Bonus::Under => ("UNDER", 2_000, 0.5, 3.0),
            Bonus::Needle => ("NEEDLE", 4_000, 1.0, 3.0),
        }
    }

    fn idx(self) -> usize {
        self as usize
    }

    /// Worst to best, which is also the order `spec` ranks them in.
    const ALL: [Bonus; KINDS] = [Bonus::Close, Bonus::Roll, Bonus::Graze, Bonus::Thread, Bonus::Skim, Bonus::Under, Bonus::Needle];
}

/// One probe direction, in the flyer's own frame (forward, right, up).
struct Dir {
    f: f32,
    r: f32,
    u: f32,
}

const fn d(f: f32, r: f32, u: f32) -> Dir {
    Dir { f, r, u }
}

/// Twelve rays: ahead and the four quarters round it, out to each side, up
/// and down, and back. The side pair finds gaps, the up/down pair arches.
const DIRS: [Dir; 12] = [
    d(1.0, 0.0, 0.0),
    d(1.0, 0.6, 0.0),
    d(1.0, -0.6, 0.0),
    d(1.0, 0.0, 0.6),
    d(1.0, 0.0, -0.6),
    d(1.0, 0.6, -0.6),
    d(1.0, -0.6, -0.6),
    d(0.0, 1.0, 0.0),
    d(0.0, -1.0, 0.0),
    d(0.0, 0.0, 1.0),
    d(0.0, 0.0, -1.0),
    d(-1.0, 0.0, 0.0),
];
const RIGHT: usize = 7;
const LEFT: usize = 8;
const UP: usize = 9;
const DOWN: usize = 10;

/// A run's score, and what the flyer has near it.
#[derive(Default)]
pub struct Soar {
    /// Points in the run under way.
    pub score: f64,
    /// 1x to `MAX_MULT`.
    pub mult: f32,
    /// Distance to the nearest surface, metres (None: nothing within `REACH`).
    pub near: Option<f32>,
    /// Metres flown in this run.
    pub dist: f32,
    /// Seconds flown in this run.
    pub secs: f32,
    /// Best run banked in this world.
    pub best: f64,
    /// The last call-out: its words, its points, and seconds left on screen.
    pub call: Option<(&'static str, u32, f32)>,
    /// A run is open (the wings are out).
    pub flying: bool,
    /// Seconds with nothing inside `RESET_D`.
    away: f32,
    /// Seconds left before each bonus may fire again.
    cool: [f32; KINDS],
    /// How long the ground has been within skimming reach, seconds.
    skim_t: f32,
    /// Distances the probes found this step.
    hits: [Option<f32>; DIRS.len()],
}

impl Soar {
    /// Open a run. Keeps `best`.
    pub fn start(&mut self) {
        self.score = 0.0;
        self.mult = 1.0;
        self.dist = 0.0;
        self.secs = 0.0;
        self.away = 0.0;
        self.skim_t = 0.0;
        self.cool = [0.0; KINDS];
        self.near = None;
        self.hits = [None; DIRS.len()];
        self.call = None;
        self.flying = true;
    }

    /// Close the run and keep it if it is the best. `kept` is the fraction
    /// banked: all of a landing, part of a crash. Returns the line to show,
    /// or None when there was nothing worth saying.
    pub fn bank(&mut self, kept: f32) -> Option<String> {
        if !self.flying {
            return None;
        }
        self.flying = false;
        self.call = None;
        let run = self.score * kept as f64;
        self.score = run;
        if run < 500.0 {
            return None;
        }
        let record = run > self.best;
        if record {
            self.best = run;
        }
        Some(if record {
            format!("FLIGHT {}   BEST YET", thousands(run))
        } else {
            format!("FLIGHT {}   BEST {}", thousands(run), thousands(self.best))
        })
    }

    /// Probe round the flyer and score the step. `eye` is where the flyer
    /// sees from, `vel` its velocity, `clearance` the air under it.
    pub fn step(&mut self, store: &Store, eye: Vec3, vel: Vec3, clearance: Option<f32>, dt: f32) {
        let s = vel.length();
        let fwd = vel.normalize_or(Vec3::NEG_Z);
        let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(fwd);
        for (i, k) in DIRS.iter().enumerate() {
            let dir = fwd * k.f + right * k.r + up * k.u;
            self.hits[i] = store.ray(eye, dir, REACH, |m| m != crate::world::material::AIR && !super::foliage(m)).map(|h| h.t);
        }
        let nearest = self.hits.iter().flatten().copied().fold(f32::INFINITY, f32::min);
        self.near = nearest.is_finite().then_some(nearest);

        for c in &mut self.cool {
            *c = (*c - dt).max(0.0);
        }
        self.call = match self.call.take() {
            Some((n, p, t)) if t > dt => Some((n, p, t - dt)),
            _ => None,
        };

        // Earn by the metre, in proportion to how close it is flown.
        let step = s * dt;
        self.dist += step;
        self.secs += dt;
        let near = 1.0 - smoothstep(FULL_D, FADE_D, nearest);
        self.score += (step * near * self.mult * RATE) as f64;

        // The multiplier charges close in and bleeds away out in the open.
        if nearest < CLOSE_D {
            self.mult = (self.mult + dt / CHARGE * (MAX_MULT - 1.0)).min(MAX_MULT);
        } else {
            let k = (-dt * std::f32::consts::LN_2 / BLEED_HALF).exp();
            self.mult = 1.0 + (self.mult - 1.0) * k;
        }
        if nearest > RESET_D {
            self.away += dt;
            if self.away > RESET_T {
                self.mult = 1.0;
            }
        } else {
            self.away = 0.0;
        }

        // Skimming: the ground held close while going fast.
        let low = clearance.is_some_and(|c| c < 1.5) && s > 19.0;
        self.skim_t = if low { self.skim_t + dt } else { 0.0 };

        let at = |i: usize| self.hits[i].unwrap_or(f32::INFINITY);
        let (l, r) = (at(LEFT), at(RIGHT));
        let gap = l.max(r);
        if gap < 2.0 {
            self.award(Bonus::Needle);
        } else if at(UP) < 3.0 && at(DOWN) < 3.0 {
            self.award(Bonus::Under);
        } else if self.skim_t > 1.0 {
            self.award(Bonus::Skim);
            self.skim_t = 0.0;
        } else if gap < 4.0 {
            self.award(Bonus::Thread);
        } else if nearest < 2.5 {
            self.award(Bonus::Close);
        }
    }

    /// Fire a bonus, if it is off cooldown. Holds off every lesser one.
    pub fn award(&mut self, b: Bonus) {
        if !self.flying || self.cool[b.idx()] > 0.0 {
            return;
        }
        let (name, points, bump, cool) = b.spec();
        // Paid at the multiplier it was earned at; the bump counts from here on.
        let paid = (points as f32 * self.mult) as u32;
        self.score += paid as f64;
        self.mult = (self.mult + bump).min(MAX_MULT);
        self.cool[b.idx()] = cool;
        for lesser in Bonus::ALL.iter().take(b.idx()) {
            self.cool[lesser.idx()] = self.cool[lesser.idx()].max(SHADE);
        }
        self.call = Some((name, paid, CALL_SECS));
    }

    /// Crashing costs the multiplier.
    pub fn crashed(&mut self) {
        self.mult = (self.mult * 0.5).max(1.0);
    }

    /// The score line for the HUD, and the multiplier as a fraction of full.
    pub fn hud(&self, mode: Mode) -> Option<(String, f32)> {
        if mode != Mode::Wing || !self.flying {
            return None;
        }
        let line = format!("{}   x{:.1}", thousands(self.score), self.mult);
        Some((line, (self.mult - 1.0) / (MAX_MULT - 1.0)))
    }
}

/// 18420 -> "18,420".
pub fn thousands(v: f64) -> String {
    let n = v.max(0.0).round() as u64;
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testutil::*;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0.0), "0");
        assert_eq!(thousands(18_420.4), "18,420");
        assert_eq!(thousands(1_234_567.0), "1,234,567");
    }

    /// Open sky earns next to nothing; the same distance flown close to a
    /// wall earns a great deal more.
    #[test]
    fn proximity_is_the_score() {
        let s = land(|_| 2.0);
        let mut open = Soar::default();
        open.start();
        let mut low = Soar::default();
        low.start();
        let v = Vec3::new(25.0, 0.0, 0.0);
        for _ in 0..120 {
            open.step(&s, Vec3::new(0.0, 120.0, 0.0), v, None, 1.0 / 60.0);
            low.step(&s, Vec3::new(0.0, 3.2, 0.0), v, Some(1.2), 1.0 / 60.0);
        }
        assert!(open.score < 10.0, "open sky earned {}", open.score);
        assert!(low.score > 1_000.0, "close in earned only {}", low.score);
        assert!(low.mult > 3.0, "multiplier {}", low.mult);
    }

    /// The multiplier bleeds away out in the open and resets after a while.
    #[test]
    fn the_multiplier_bleeds_and_resets() {
        let s = land(|_| 2.0);
        let mut so = Soar::default();
        so.start();
        let v = Vec3::new(25.0, 0.0, 0.0);
        for _ in 0..360 {
            so.step(&s, Vec3::new(0.0, 3.2, 0.0), v, Some(1.2), 1.0 / 60.0);
        }
        let charged = so.mult;
        assert!(charged > 6.0, "charged to {charged}");
        // Up into clear air: it falls back, then resets.
        for _ in 0..120 {
            so.step(&s, Vec3::new(0.0, 140.0, 0.0), v, None, 1.0 / 60.0);
        }
        assert!(so.mult < charged * 0.6, "bled to {}", so.mult);
        for _ in 0..240 {
            so.step(&s, Vec3::new(0.0, 140.0, 0.0), v, None, 1.0 / 60.0);
        }
        assert!((so.mult - 1.0).abs() < 0.01, "reset to {}", so.mult);
    }

    /// A better bonus holds the lesser ones off, so one tight gap pays out
    /// once rather than three times over.
    #[test]
    fn a_better_bonus_shades_the_lesser_ones() {
        let mut so = Soar::default();
        so.start();
        so.award(Bonus::Needle);
        let after = so.score;
        so.award(Bonus::Close);
        so.award(Bonus::Thread);
        assert_eq!(so.score, after, "a lesser bonus paid out under a needle");
    }

    /// A run banks, and a crash banks less than a landing.
    #[test]
    fn banking_keeps_the_best() {
        let mut so = Soar::default();
        so.start();
        so.score = 10_000.0;
        assert!(so.bank(1.0).unwrap().contains("BEST YET"));
        assert_eq!(so.best, 10_000.0);
        so.start();
        so.score = 10_000.0;
        let line = so.bank(0.6).unwrap();
        assert!(line.contains("6,000") && line.contains("10,000"), "{line}");
        assert_eq!(so.best, 10_000.0);
    }
}
