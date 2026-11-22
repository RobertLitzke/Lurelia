//! Surf (world.json `surf`, GUIDE.md "Surf"): waves that come in off the open
//! sea, steepen as the water shoals, break into white water and run up the
//! beach. It is all drawn in the water and terrain shaders from a grid baked
//! once per world: how far each point lies seaward of the high-water coast,
//! which the wave crests follow, and how open the sea in front of it is, so a
//! lagoon or a harbour behind a mole stays calm.
//!
//! The swell is the open sea's surface itself rising and falling: a few long
//! waves rolling in from the side of the map the coast faces, dying away in
//! shallow water (where the surf takes over) and in sheltered water. The
//! renderer moves the sea's mesh by it (`vs_water`) and gameplay reads the
//! same numbers (`Shore::swell_at`), so what floats rides the waves it shows.

use super::fields::Fields;
use glam::Vec2;
use serde::Deserialize;
use std::sync::atomic::{AtomicU32, Ordering};

/// world.json `surf`: `{"height": 1.2}`, the size of the breakers on a fair
/// day, with the wave length and period if the defaults do not suit.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SurfDef {
    /// Height of a breaking wave on a fair day, metres. The wind scales it.
    pub height: f32,
    /// Metres from one crest to the next.
    #[serde(default = "default_length")]
    pub length_m: f32,
    /// Seconds between one wave and the next.
    #[serde(default = "default_period")]
    pub period_s: f32,
    /// The open sea's swell, crest to trough on a fair day, metres: half
    /// the breaker height if left out; 0 keeps the sea's surface flat.
    #[serde(default)]
    pub swell: Option<f32>,
    /// Metres from one crest of the longest swell to the next.
    #[serde(default = "default_swell_length")]
    pub swell_length_m: f32,
}

fn default_length() -> f32 {
    16.0
}
fn default_period() -> f32 {
    7.0
}
fn default_swell_length() -> f32 {
    40.0
}

impl SurfDef {
    /// The renderer's surf uniform: height, length, period, on.
    pub fn uniform(&self) -> [f32; 4] {
        [self.height.max(0.0), self.length_m.max(2.0), self.period_s.max(1.0), if self.height > 0.0 { 1.0 } else { 0.0 }]
    }

    /// Crest to trough of the swell on a fair day, metres.
    pub fn swell_height(&self) -> f32 {
        self.swell.unwrap_or(self.height * 0.5).max(0.0)
    }
}

/// The swell's waves: (length as a share of the longest, turn off the main
/// heading in radians, share of the height). `swell` in scene.wgsl has the
/// same table.
const SWELL: [(f32, f32, f32); 3] = [(1.0, 0.0, 0.5), (0.63, 0.47, 0.3), (0.41, -0.66, 0.2)];

/// Gravity for the waves' speed: in deep water a wave of length L runs at
/// sqrt(g L / 2 pi).
const G: f32 = 9.81;

/// What a built world knows of its surf and swell (`Fields::shore`): the
/// shore grid the renderer also reads, the swell's heading, height and
/// length, and the clock and wind the game sets every tick (in atomics, as
/// the world is shared).
#[derive(Default, Debug)]
pub struct Shore {
    /// Side of the grids, texels over the whole world.
    pub m: usize,
    /// Metres seaward of the coast, and how open the sea is (`shore_grid`).
    pub seaward: Vec<f32>,
    pub open: Vec<f32>,
    /// Which way the swell runs (toward the land), unit (x, z).
    pub heading: Vec2,
    /// Crest to trough on a fair day, metres; 0 for none.
    pub height: f32,
    /// Metres between crests of the longest wave.
    pub length: f32,
    /// The breakers' spacing (m) and period (s), `SurfDef` `length_m` and
    /// `period_s`.
    pub surf_length: f32,
    pub surf_period: f32,
    /// Height of a breaker on a fair day, metres (`SurfDef` `height`).
    pub surf_height: f32,
    /// Game seconds and the wind's multiplier (f32 bits).
    clock: AtomicU32,
    wind: AtomicU32,
}

impl Shore {
    /// The shore of a world with surf: its grids (`m` square) and swell.
    pub fn build(f: &Fields, def: &SurfDef, m: usize) -> Self {
        let (seaward, open) = shore_grid(f, m);
        let heading = swell_heading(&seaward, &open, m);
        Self {
            m,
            seaward,
            open,
            heading,
            height: def.swell_height(),
            length: def.swell_length_m.max(4.0),
            surf_length: def.length_m.max(2.0),
            surf_period: def.period_s.max(1.0),
            surf_height: def.height.max(0.0),
            clock: AtomicU32::new(0),
            wind: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    /// The renderer's swell uniform: heading (x, z), height, length.
    pub fn uniform(&self) -> [f32; 4] {
        [self.heading.x, self.heading.y, self.height, self.length]
    }

    /// Set the game clock (seconds, the time the renderer draws with) and the
    /// wind's multiplier (`Sky::wind`).
    pub fn set_clock(&self, secs: f32, wind: f32) {
        self.clock.store(secs.to_bits(), Ordering::Relaxed);
        self.wind.store(wind.to_bits(), Ordering::Relaxed);
    }

    /// How open the sea is at (x, z) in a world `size` metres across,
    /// bilinear as `shore_at` reads it in scene.wgsl.
    pub fn open_at(&self, x: f32, z: f32, size: f32) -> f32 {
        self.sample(&self.open, x, z, size)
    }

    /// Metres seaward of the coast at (x, z), as `shore_at` reads it.
    pub fn seaward_at(&self, x: f32, z: f32, size: f32) -> f32 {
        self.sample(&self.seaward, x, z, size)
    }

    fn sample(&self, grid: &[f32], x: f32, z: f32, size: f32) -> f32 {
        if grid.is_empty() {
            return 0.0;
        }
        let m = self.m;
        let p = |v: f32| (v / size * m as f32 - 0.5).clamp(0.0, m as f32 - 1.001);
        let (px, pz) = (p(x), p(z));
        let (ix, iz) = (px as usize, pz as usize);
        let (fx, fz) = (px - ix as f32, pz - iz as f32);
        let at = |x: usize, z: usize| grid[z.min(m - 1) * m + x.min(m - 1)];
        let a = at(ix, iz) + (at(ix + 1, iz) - at(ix, iz)) * fx;
        let b = at(ix, iz + 1) + (at(ix + 1, iz + 1) - at(ix, iz + 1)) * fx;
        a + (b - a) * fz
    }

    /// The breaker at (x, z) now, as `surf_at` in scene.wgsl draws it: where
    /// in its cycle the wave is (0 as its crest passes) and how big this one
    /// is against the rest of its set (0.2 to 1). None without surf.
    pub fn breaker(&self, x: f32, z: f32, size: f32) -> Option<(f32, f32)> {
        if self.seaward.is_empty() {
            return None;
        }
        let t = f32::from_bits(self.clock.load(Ordering::Relaxed));
        let v = self.seaward_at(x, z, size) / self.surf_length + t / self.surf_period;
        Some((v.rem_euclid(1.0), 0.6 + 0.4 * (v.floor() * 0.9).sin()))
    }

    /// How far above the sea's level the swash of the last wave reaches up
    /// the beach at (x, z) now, metres: the water runs up fast after a wave
    /// breaks and slides back slowly. `swash` in scene.wgsl draws the same
    /// sheet of water, so a bird that keeps just above this keeps just
    /// ahead of the water on screen. `ahead` reads it that many seconds
    /// from now. 0 without surf.
    pub fn swash_at(&self, x: f32, z: f32, size: f32, ahead: f32) -> f32 {
        if self.seaward.is_empty() || self.surf_height <= 0.0 {
            return 0.0;
        }
        let t = f32::from_bits(self.clock.load(Ordering::Relaxed)) + ahead;
        let wind = f32::from_bits(self.wind.load(Ordering::Relaxed)).clamp(0.5, 2.0);
        let s = self.seaward_at(x, z, size);
        let v = s / self.surf_length + t / self.surf_period;
        let (wave, f) = (v.floor(), v.rem_euclid(1.0));
        let big = 0.6 + 0.4 * (wave * 0.9).sin();
        let section = 0.55 + 0.75 * vnoise2(x * 0.018 + wave * 1.7, z * 0.018 + wave * 0.6);
        let h = self.surf_height * wind * big * section * self.open_at(x, z, size);
        if h < 0.02 {
            return 0.0;
        }
        let up = smooth(0.0, 0.16, f) * (1.0 - smooth(0.22, 0.8, f));
        h * 0.4 * up * (0.75 + 0.5 * vnoise2(x * 0.12, z * 0.12))
    }

    /// How far the swell lifts the sea at (x, z) now, metres above or below
    /// its level, where the water is `depth` deep and the world `size`
    /// across. 0 with no swell.
    pub fn swell_at(&self, x: f32, z: f32, depth: f32, size: f32) -> f32 {
        if self.height <= 0.0 {
            return 0.0;
        }
        let t = f32::from_bits(self.clock.load(Ordering::Relaxed));
        let wind = f32::from_bits(self.wind.load(Ordering::Relaxed));
        swell(Vec2::new(x, z), t, self.heading, self.height, self.length, wind, self.open_at(x, z, size), depth, 0.0).0
    }
}

/// The swell at `p`: height above the sea's level and its slope (d/dx,
/// d/dz). `spacing` is how coarsely it is sampled there (the mesh, or a
/// pixel): a wave shorter than a few samples is left out rather than
/// aliased. scene.wgsl `swell` is the same.
#[allow(clippy::too_many_arguments)]
pub fn swell(p: Vec2, t: f32, heading: Vec2, height: f32, length: f32, wind: f32, open: f32, depth: f32, spacing: f32) -> (f32, Vec2) {
    let a = height * 0.5 * wind.clamp(0.5, 2.0) * open * smooth(0.4, 4.0, depth);
    if a <= 0.0 {
        return (0.0, Vec2::ZERO);
    }
    let (mut y, mut slope) = (0.0, Vec2::ZERO);
    for (i, &(ls, turn, share)) in SWELL.iter().enumerate() {
        let len = length * ls;
        let keep = 1.0 - smooth(len * 0.12, len * 0.25, spacing);
        if keep <= 0.0 {
            continue;
        }
        let k = std::f32::consts::TAU / len;
        let w = (G * k).sqrt();
        let (s, c) = turn.sin_cos();
        let d = Vec2::new(heading.x * c - heading.y * s, heading.x * s + heading.y * c);
        let phase = k * d.dot(p) - w * t + i as f32 * 1.7;
        let amp = a * share * keep;
        y += amp * phase.cos();
        slope -= d * (amp * k * phase.sin());
    }
    (y, slope)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Which way the swell runs: in from the open sea, the opposite of the way
/// the open coast faces on average. The engine's wind where there is no
/// open coast.
fn swell_heading(seaward: &[f32], open: &[f32], m: usize) -> Vec2 {
    let mut sum = Vec2::ZERO;
    for z in 1..m - 1 {
        for x in 1..m - 1 {
            let i = z * m + x;
            if open[i] < 0.6 || !(5.0..150.0).contains(&seaward[i]) {
                continue;
            }
            let g = Vec2::new(seaward[i + 1] - seaward[i - 1], seaward[i + m] - seaward[i - m]);
            sum += g.normalize_or_zero();
        }
    }
    let toward_land = -sum;
    if toward_land.length() < 1e-3 {
        return super::WIND;
    }
    toward_land.normalize()
}

/// Metres seaward of the high-water coast over an `m` x `m` grid (negative
/// on land), smoothed so the crests that follow it bend round the coast
/// rather than copying every notch in it; and how open the sea is there, 0
/// in an enclosed lagoon to 1 off an open coast.
pub fn shore_grid(f: &Fields, m: usize) -> (Vec<f32>, Vec<f32>) {
    let texel = f.size / m as f32;
    // Seaward distance at each texel's centre, from the 1 m coast field.
    let mut s: Vec<f32> = (0..m * m)
        .map(|k| {
            let (x, z) = ((k % m) as f32 + 0.5, (k / m) as f32 + 0.5);
            -f.coast_dist[f.idx(x * texel, z * texel)]
        })
        .collect();
    // Clamp far inland and far out: only the band near the coast matters, and
    // a 1e9 from an empty distance transform must not smear in.
    for v in s.iter_mut() {
        *v = v.clamp(-60.0, 400.0);
    }
    let smooth_r = ((10.0 / texel).round() as usize).max(1);
    blur(&mut s, m, smooth_r);
    blur(&mut s, m, smooth_r);
    // Openness: a wide blur of how far out the sea reaches. Off an open coast
    // the water a hundred metres out is a hundred metres from land; in a
    // lagoon it never gets far from any shore.
    let mut open: Vec<f32> = s.iter().map(|v| v.clamp(0.0, 120.0)).collect();
    // The crests bend a little, not ruled lines: a slow wobble in the
    // distance they follow. Baked here rather than added in the shader so
    // the sound (`Shore::breaker`) times the same waves.
    for (k, v) in s.iter_mut().enumerate() {
        let (x, z) = (((k % m) as f32 + 0.5) * texel, ((k / m) as f32 + 0.5) * texel);
        *v += super::noise::perlin2(x * 0.025, z * 0.025, 0x57f) * 3.5;
    }
    let open_r = ((60.0 / texel).round() as usize).max(1);
    blur(&mut open, m, open_r);
    blur(&mut open, m, open_r);
    for v in open.iter_mut() {
        let t = ((*v - 12.0) / 30.0).clamp(0.0, 1.0);
        *v = t * t * (3.0 - 2.0 * t);
    }
    (s, open)
}

/// Per texel of an `m` x `m` grid, what the renderer's sea reads beside
/// the ground (heightmap g, b, a): the tide's hollows (`Fields::basin_grid`),
/// and with surf, the seaward distance and openness (`Fields::shore`, built
/// at the same size).
pub fn shore_texels(f: &Fields, m: usize) -> Vec<[f32; 3]> {
    let basin = f.basin_grid(m);
    let sh = &f.shore;
    let has = sh.m == m && sh.seaward.len() == m * m;
    (0..m * m).map(|i| if has { [basin[i], sh.seaward[i], sh.open[i]] } else { [basin[i], 0.0, 0.0] }).collect()
}

/// scene.wgsl `hash3u`, the same bits.
fn hash3u(x: i32, y: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// scene.wgsl `vnoise2`, the same: value noise 0 to 1 on a unit lattice.
fn vnoise2(x: f32, z: f32) -> f32 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let (i, j) = (ix as i32, iz as i32);
    let at = |a: i32, b: i32| (hash3u(a, b, 7) & 1023) as f32 / 1023.0;
    let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
    let top = a + (b - a) * ux;
    let bot = c + (d - c) * ux;
    top + (bot - top) * uz
}

/// Box blur of radius `r` texels, rows then columns, in place.
fn blur(v: &mut [f32], m: usize, r: usize) {
    let mut tmp = vec![0.0f32; m * m];
    let w = (2 * r + 1) as f32;
    for z in 0..m {
        let row = &v[z * m..(z + 1) * m];
        let at = |x: isize| row[x.clamp(0, m as isize - 1) as usize];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for x in 0..m {
            tmp[z * m + x] = sum / w;
            sum += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
        }
    }
    for x in 0..m {
        let at = |z: isize| tmp[z.clamp(0, m as isize - 1) as usize * m + x];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for z in 0..m {
            v[z * m + x] = sum / w;
            sum += at(z as isize + r as isize + 1) - at(z as isize - r as isize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swell_rises_and_falls_and_dies_in_the_shallows() {
        let h = Vec2::new(1.0, 0.0);
        let at = |x: f32, t: f32, depth: f32| swell(Vec2::new(x, 0.0), t, h, 1.0, 40.0, 1.0, 1.0, depth, 0.0);
        // Somewhere along a line of deep water it is up, somewhere down,
        // never more than half the height.
        let ys: Vec<f32> = (0..200).map(|i| at(i as f32, 0.0, 20.0).0).collect();
        assert!(ys.iter().cloned().fold(f32::MIN, f32::max) > 0.2);
        assert!(ys.iter().cloned().fold(f32::MAX, f32::min) < -0.2);
        assert!(ys.iter().all(|y| y.abs() <= 0.5 + 1e-4));
        // It moves.
        assert!((at(10.0, 0.0, 20.0).0 - at(10.0, 1.5, 20.0).0).abs() > 1e-3);
        // Nothing in a few centimetres of water, or in a closed lagoon.
        assert_eq!(at(10.0, 0.0, 0.2).0, 0.0);
        assert_eq!(swell(Vec2::new(10.0, 0.0), 0.0, h, 1.0, 40.0, 1.0, 0.0, 20.0, 0.0).0, 0.0);
        // The slope matches the height's own change.
        let e = 0.01;
        let (y0, s0) = at(12.0, 0.7, 20.0);
        let y1 = at(12.0 + e, 0.7, 20.0).0;
        assert!(((y1 - y0) / e - s0.x).abs() < 0.02);
    }

    /// The swash on an open beach runs up and drains back once a wave, is
    /// never more than the 0.4 of a wave's height the shader lets it climb,
    /// and is not there at all without surf.
    #[test]
    fn swash_runs_up_and_drains_once_a_wave() {
        let m = 16;
        let shore = Shore {
            m,
            seaward: vec![-5.0; m * m],
            open: vec![1.0; m * m],
            surf_length: 16.0,
            surf_period: 7.0,
            surf_height: 1.2,
            ..Default::default()
        };
        let reach: Vec<f32> = (0..70)
            .map(|i| {
                shore.set_clock(i as f32 * 0.1, 1.0);
                shore.swash_at(40.0, 40.0, 160.0, 0.0)
            })
            .collect();
        let top = reach.iter().cloned().fold(0.0, f32::max);
        assert!(top > 0.05 && top <= 1.2 * 2.0 * 0.4 * 1.25, "{top}");
        // Dry for part of every wave: it drains away, then the next one comes.
        assert!(reach.iter().any(|&r| r == 0.0));
        // Reading ahead is reading later.
        shore.set_clock(1.0, 1.0);
        let ahead = shore.swash_at(40.0, 40.0, 160.0, 0.5);
        shore.set_clock(1.5, 1.0);
        assert!((shore.swash_at(40.0, 40.0, 160.0, 0.0) - ahead).abs() < 1e-5);
        let calm = Shore { surf_height: 0.0, ..Shore::default() };
        assert_eq!(calm.swash_at(40.0, 40.0, 160.0, 0.0), 0.0);
    }

    #[test]
    fn vnoise_stays_in_range_and_matches_its_lattice() {
        for i in 0..200 {
            let (x, z) = (i as f32 * 0.37 - 20.0, i as f32 * 0.61 + 3.0);
            let v = vnoise2(x, z);
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
        assert_eq!(vnoise2(3.0, 5.0), (hash3u(3, 5, 7) & 1023) as f32 / 1023.0);
    }

    #[test]
    fn blur_keeps_a_flat_field_flat_and_smooths_a_step() {
        let m = 8;
        let mut v = vec![3.0f32; m * m];
        blur(&mut v, m, 2);
        assert!(v.iter().all(|x| (x - 3.0).abs() < 1e-5));
        let mut step: Vec<f32> = (0..m * m).map(|k| if k % m < 4 { 0.0 } else { 10.0 }).collect();
        blur(&mut step, m, 1);
        assert!(step[3] > 0.0 && step[3] < 10.0 && step[0] == 0.0);
    }

    #[test]
    fn open_coast_is_open_and_a_lagoon_is_not() {
        // 256 m: sea on the west half, land on the east with a small pond.
        let mut f = Fields::new(256.0, 10.0);
        let n = f.n;
        for i in 0..n * n {
            let (x, z) = (i % n, i / n);
            let pond = (x as i32 - 200).pow(2) + (z as i32 - 128).pow(2) < 64;
            f.h[i] = if x < 128 || pond { 0.0 } else { 20.0 };
        }
        f.derive();
        let m = 64;
        let (s, open) = shore_grid(&f, m);
        // The swell comes in from the west, where the sea is.
        let heading = swell_heading(&s, &open, m);
        assert!(heading.x > 0.9, "{heading}");
        let at = |x: f32, z: f32| ((z / 4.0) as usize) * m + (x / 4.0) as usize;
        // Out at sea, seaward and open; inland, negative.
        assert!(s[at(40.0, 128.0)] > 50.0 && open[at(40.0, 128.0)] > 0.9);
        assert!(s[at(240.0, 20.0)] < 0.0);
        // The pond is water but neither far from shore nor open.
        assert!(open[at(200.0, 128.0)] < 0.2);
    }
}
