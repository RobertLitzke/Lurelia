//! Where a town reaches.
//!
//! Not a circle round its centre but the ground that most wants building on:
//! near the centre, near the water (people want the view, and the quay is
//! where the work is), and along the roads in from the country, where houses
//! string out and thin away. Each patch of ground gets a score, distance from
//! the centre counting against it and the water and the roads for it; the
//! town is the patches scoring best, enough of them to make up the area its
//! radius asks for, joined to the centre. The rest of the generator asks how
//! far inside or outside the town a point is (`sd`), so the districts, the
//! core, the plots thinning out and the farmland round about all follow the
//! same shape.

use super::graph::seg_foot;
use super::Ground;
use crate::world::noise::perlin2;
use glam::Vec2;

/// Raster step, metres.
const STEP: f32 = 4.0;

#[derive(Clone, Debug, Default)]
pub struct Extent {
    pub lo: Vec2,
    pub w: usize,
    pub h: usize,
    /// Signed distance to the town's edge: negative inside, metres.
    pub sd: Vec<f32>,
    /// The score (below 1 is town), and infinity on water.
    pub score: Vec<f32>,
    /// Distance to the nearest road in from the country, metres (to 200).
    pub road: Vec<f32>,
    /// Distance to open water, metres.
    pub water: Vec<f32>,
    /// The farthest the town reaches from its centre.
    pub reach: f32,
}

impl Extent {
    fn cell(&self, p: Vec2) -> Option<usize> {
        let q = (p - self.lo) / STEP;
        (q.x >= 0.0 && q.y >= 0.0 && (q.x as usize) < self.w && (q.y as usize) < self.h).then(|| q.y as usize * self.w + q.x as usize)
    }
    pub fn sd(&self, p: Vec2) -> f32 {
        self.cell(p).map_or(1e6, |i| self.sd[i])
    }
    pub fn score(&self, p: Vec2) -> f32 {
        self.cell(p).map_or(f32::INFINITY, |i| self.score[i])
    }
    pub fn road(&self, p: Vec2) -> f32 {
        self.cell(p).map_or(200.0, |i| self.road[i])
    }
    pub fn water(&self, p: Vec2) -> f32 {
        self.cell(p).map_or(f32::MAX, |i| self.water[i])
    }
}

/// Is p inside the polygon (even-odd)?
pub fn in_poly(poly: &[Vec2], p: Vec2) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut odd = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Two-pass chamfer distance from the cells `seed` marks, metres, on a grid
/// of `w` by `h` cells `step` m apart.
pub(super) fn chamfer(w: usize, h: usize, step: f32, seed: impl Fn(usize) -> bool) -> Vec<f32> {
    let mut d: Vec<f32> = (0..w * h).map(|i| if seed(i) { 0.0 } else { f32::MAX }).collect();
    let (s, g) = (step, step * std::f32::consts::SQRT_2);
    for z in 0..h {
        for x in 0..w {
            let i = z * w + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + s);
            }
            if z > 0 {
                v = v.min(d[i - w] + s);
                if x > 0 {
                    v = v.min(d[i - w - 1] + g);
                }
                if x + 1 < w {
                    v = v.min(d[i - w + 1] + g);
                }
            }
            d[i] = v;
        }
    }
    for z in (0..h).rev() {
        for x in (0..w).rev() {
            let i = z * w + x;
            let mut v = d[i];
            if x + 1 < w {
                v = v.min(d[i + 1] + s);
            }
            if z + 1 < h {
                v = v.min(d[i + w] + s);
                if x + 1 < w {
                    v = v.min(d[i + w + 1] + g);
                }
                if x > 0 {
                    v = v.min(d[i + w - 1] + g);
                }
            }
            d[i] = v;
        }
    }
    d
}

/// The town round `c` of about the area a circle of `radius` would cover,
/// pulled toward water by `water` (0 none, 1 the usual) and out along the
/// `roads` by `ribbon`.
/// `bounds`, when given, is an outline the town stays inside.
#[allow(clippy::too_many_arguments)]
pub fn build(c: Vec2, radius: f32, roads: &[Vec<Vec2>], ground: &dyn Ground, seed: u32, water: f32, ribbon: f32, bounds: &[Vec2]) -> Extent {
    let ext = radius * 2.4 + 60.0;
    let lo = (c - Vec2::splat(ext)).max(Vec2::ZERO);
    let hi = (c + Vec2::splat(ext)).min(Vec2::splat(ground.size()));
    let w = ((hi.x - lo.x) / STEP) as usize + 1;
    let h = ((hi.y - lo.y) / STEP) as usize + 1;
    let at = |i: usize| lo + Vec2::new((i % w) as f32 + 0.5, (i / w) as f32 + 0.5) * STEP;
    let wet: Vec<bool> = (0..w * h).map(|i| ground.wet(at(i))).collect();
    let out_of_bounds: Vec<bool> = (0..w * h).map(|i| !bounds.is_empty() && !in_poly(bounds, at(i))).collect();
    let dw = chamfer(w, h, STEP, |i| wet[i]);
    // Distance to the roads in.
    let mut road = vec![200.0f32; w * h];
    for line in roads {
        for s in line.windows(2) {
            let (a, b) = (s[0], s[1]);
            let blo = ((a.min(b) - Vec2::splat(200.0) - lo) / STEP).max(Vec2::ZERO);
            let bhi = (a.max(b) + Vec2::splat(200.0) - lo) / STEP;
            for z in blo.y as usize..(bhi.y as usize).min(h) {
                for x in blo.x as usize..(bhi.x as usize).min(w) {
                    let i = z * w + x;
                    let d = seg_foot(a, b, at(i)).0;
                    if d < road[i] {
                        road[i] = d;
                    }
                }
            }
        }
    }
    // The score before scaling: (distance term, pull and noise).
    let raw: Vec<(f32, f32)> = (0..w * h)
        .map(|i| {
            let p = at(i);
            let v = p - c;
            let r = v.length();
            let d = v / r.max(1e-3);
            let pull = water * 0.5 * (1.0 - dw[i] / (radius * 0.8)).max(0.0) + ribbon * 0.35 * (1.0 - road[i] / 55.0).max(0.0);
            let noise = 0.13 * perlin2(d.x * 1.3 + 7.0, d.y * 1.3, seed ^ 0x51) + 0.08 * perlin2(p.x * 0.02, p.y * 0.02, seed ^ 0x52);
            (r / radius, noise - pull)
        })
        .collect();
    // Scale the distance term until the town covers a circle's worth of
    // dry ground.
    let target = (std::f32::consts::PI * radius * radius / (STEP * STEP)) as usize;
    let count = |k: f32| (0..w * h).filter(|&i| !wet[i] && !out_of_bounds[i] && raw[i].0 / k + raw[i].1 < 1.0).count();
    let (mut a, mut b) = (0.3f32, 2.0f32);
    for _ in 0..16 {
        let m = (a + b) * 0.5;
        if count(m) < target {
            a = m;
        } else {
            b = m;
        }
    }
    let k = (a + b) * 0.5;
    let score: Vec<f32> = (0..w * h).map(|i| if wet[i] || out_of_bounds[i] { f32::INFINITY } else { raw[i].0 / k + raw[i].1 }).collect();
    // The piece joined to the centre. A river through the town doesn't cut
    // it in two: water scoring as town carries the join, though it is never
    // town itself.
    let passable: Vec<bool> = (0..w * h).map(|i| !out_of_bounds[i] && raw[i].0 / k + raw[i].1 < 1.0).collect();
    let mut reached = vec![false; w * h];
    let q0 = (c - lo) / STEP;
    let start = (q0.y as usize).min(h - 1) * w + (q0.x as usize).min(w - 1);
    let mut stack = vec![start];
    while let Some(i) = stack.pop() {
        if reached[i] || !passable[i] && i != start {
            continue;
        }
        reached[i] = true;
        let (x, z) = (i % w, i / w);
        if x > 0 {
            stack.push(i - 1);
        }
        if x + 1 < w {
            stack.push(i + 1);
        }
        if z > 0 {
            stack.push(i - w);
        }
        if z + 1 < h {
            stack.push(i + w);
        }
    }
    let inside: Vec<bool> = (0..w * h).map(|i| reached[i] && !wet[i]).collect();
    // How deep inside: measured to the dry land outside, so a town on the
    // shore is not all "edge" along its waterfront.
    let din = chamfer(w, h, STEP, |i| !inside[i] && !wet[i]);
    let dout = chamfer(w, h, STEP, |i| inside[i]);
    let sd: Vec<f32> = (0..w * h).map(|i| if inside[i] { -din[i] } else { dout[i] }).collect();
    let reach = (0..w * h).filter(|&i| inside[i]).map(|i| at(i).distance(c)).fold(0.0, f32::max) + STEP;
    Extent { lo, w, h, sd, score, road, water: dw, reach }
}

/// Where the market goes on a town by the water: as near the water as a
/// square and a row of buildings allow, on the side the water lies. `None`
/// when no wide water is within reach.
pub fn waterfront_centre(c: Vec2, radius: f32, plaza: f32, ground: &dyn Ground) -> Option<Vec2> {
    let mut best: Option<(f32, Vec2)> = None;
    for k in 0..72 {
        let a = k as f32 / 72.0 * std::f32::consts::TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let mut s = 0.0;
        while s < radius * 1.2 && !ground.wet(c + d * s) {
            s += 4.0;
        }
        if s >= radius * 1.2 {
            continue;
        }
        // Wide water (a sea, a lake, an estuary), not a river: most of a
        // 30 m circle round where the ray met it is wet.
        let q = c + d * (s + 30.0);
        let wet = (0..24)
            .filter(|&j| {
                let b = j as f32 / 24.0 * std::f32::consts::TAU;
                ground.wet(q + Vec2::new(b.cos(), b.sin()) * 30.0)
            })
            .count();
        if wet < 17 {
            continue;
        }
        if best.is_none_or(|b| s < b.0) {
            best = Some((s, d));
        }
    }
    let (s, d) = best?;
    let keep = plaza + 24.0;
    (s > keep).then(|| c + d * (s - keep))
}
