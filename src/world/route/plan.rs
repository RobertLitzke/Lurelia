//! Settlement layouts: the shape of a village's streets, and the plots along
//! them.
//!
//! A layout is only a list of street lines. Everything after that — routing
//! them over the land, grading them, building to a kit — is the same
//! machinery any other route uses. What a layout adds is frontage: given a
//! street, where the plots are, which way their buildings face, and where the
//! door path meets the kerb.

use crate::world::fields::Fields;
use crate::world::noise::{hash32, perlin2, unit};
use glam::Vec2;

/// How a settlement's streets are arranged.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    /// Blocks on a square grid: a surveyed town.
    Grid,
    /// A ring with spokes into the middle: a green, a market cross.
    Radial,
    /// Lanes wandering out from a centre and branching: a village that grew.
    Organic,
    /// One street through, with short lanes off it: a roadside settlement.
    Linear,
}

impl Layout {
    pub fn read(s: &str) -> Option<Layout> {
        Some(match s {
            "grid" => Layout::Grid,
            "radial" => Layout::Radial,
            "organic" => Layout::Organic,
            "linear" => Layout::Linear,
            _ => return None,
        })
    }
}

/// One street of a layout: where it runs and how important it is (0 is the
/// main street, 1 a lane).
pub struct Street {
    pub pts: Vec<Vec2>,
    pub rank: u8,
}

/// Lay out `radius` m of streets around `c`. `bearing` turns the whole plan;
/// a grid or a linear plan reads best on a quarter turn, where the buildings
/// sit square to their street.
pub fn streets(layout: Layout, c: Vec2, radius: f32, bearing: f32, block: f32, seed: u32) -> Vec<Street> {
    let a = bearing.to_radians();
    let (u, v) = (Vec2::new(a.cos(), a.sin()), Vec2::new(-a.sin(), a.cos()));
    let mut out = Vec::new();
    match layout {
        Layout::Grid => {
            let n = ((radius / block).floor() as i32).max(1);
            for i in -n..=n {
                let off = i as f32 * block;
                // A street is cut back to the circle it crosses.
                let half = (radius * radius - off * off).max(0.0).sqrt();
                if half < block * 0.6 {
                    continue;
                }
                out.push(Street {
                    pts: vec![c + u * off - v * half, c + u * off + v * half],
                    rank: if i == 0 { 0 } else { 1 },
                });
                out.push(Street {
                    pts: vec![c + v * off - u * half, c + v * off + u * half],
                    rank: if i == 0 { 0 } else { 1 },
                });
            }
        }
        Layout::Radial => {
            let ring = radius * 0.6;
            let steps = 24;
            let pts: Vec<Vec2> = (0..=steps)
                .map(|k| {
                    let t = k as f32 / steps as f32 * std::f32::consts::TAU + a;
                    let r = ring * (1.0 + 0.08 * perlin2(t.cos() * 2.0, t.sin() * 2.0, seed));
                    c + Vec2::new(t.cos(), t.sin()) * r
                })
                .collect();
            out.push(Street { pts, rank: 0 });
            let spokes = 5 + (hash32(seed ^ 11) % 3) as usize;
            for k in 0..spokes {
                let t = k as f32 / spokes as f32 * std::f32::consts::TAU + a;
                let d = Vec2::new(t.cos(), t.sin());
                out.push(Street {
                    pts: vec![c + d * (radius * 0.08), c + d * radius],
                    rank: if k == 0 { 0 } else { 1 },
                });
            }
        }
        Layout::Linear => {
            out.push(Street {
                pts: vec![c - v * radius, c + v * radius],
                rank: 0,
            });
            let n = ((radius * 2.0 / block).floor() as i32).max(1);
            for i in 0..n {
                let t = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                let h = hash32(seed ^ (i as u32).wrapping_mul(0x9e37_79b9));
                let side = if h & 1 == 0 { 1.0 } else { -1.0 };
                let len = block * (0.5 + unit(h >> 1));
                out.push(Street {
                    pts: vec![c + v * (t * radius), c + v * (t * radius) + u * side * len],
                    rank: 1,
                });
            }
        }
        Layout::Organic => {
            // A lane wanders right through, and the rest branch off it and
            // bend away. A village that grew has a street, not a star.
            let legs = 7;
            let mut p = c - v * radius;
            let mut dir = v;
            let mut main = vec![p];
            for j in 0..legs {
                let turn = perlin2(p.x * 0.012, p.y * 0.012, seed ^ 3) * 0.7;
                let (s, co) = turn.sin_cos();
                dir = Vec2::new(dir.x * co - dir.y * s, dir.x * s + dir.y * co).normalize_or_zero();
                p += dir * (radius * 2.0 / legs as f32);
                main.push(p);
                let _ = j;
            }
            let branches = 2 + (hash32(seed ^ 5) % 3) as usize;
            for k in 0..branches {
                let h = hash32(seed ^ (k as u32 + 1).wrapping_mul(0x85eb_ca6b));
                let j = 1 + (h as usize % (legs - 1));
                let base = main[j];
                let along = (main[j + 1] - main[j - 1]).normalize_or_zero();
                let side = if h & 0x100 == 0 { 1.0 } else { -1.0 };
                let mut d = Vec2::new(-along.y, along.x) * side;
                let mut q = base;
                let mut pts = vec![q];
                for i in 0..3 {
                    let turn = (unit(hash32(h ^ i as u32)) - 0.5) * 1.1;
                    let (s, co) = turn.sin_cos();
                    d = Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co).normalize_or_zero();
                    q += d * (radius * 0.3);
                    pts.push(q);
                }
                out.push(Street { pts, rank: 1 });
            }
            out.push(Street { pts: main, rank: 0 });
        }
    }
    out
}

/// A plot facing a street: where the building stands, the bearing its front
/// faces (the street), and the point on the kerb its path meets.
#[derive(Clone, Copy, Debug)]
pub struct Plot {
    pub at: Vec2,
    pub face: f32,
    pub kerb: Vec2,
}

/// Plots along a street line: one every `spacing` m, alternating sides,
/// `setback` m back from the edge of a street `half` m wide. Only the sites
/// `ok` accepts are returned, so a plot never lands in water or on a slope.
pub fn plots(line: &[Vec2], half: f32, setback: f32, spacing: f32, depth: f32, seed: u32, mut ok: impl FnMut(Vec2, Vec2) -> bool) -> Vec<Plot> {
    let mut out = Vec::new();
    let mut s = spacing * 0.5;
    let mut walked = 0.0f32;
    let mut k = 0usize;
    while k + 1 < line.len() {
        let (a, b) = (line[k], line[k + 1]);
        let len = a.distance(b);
        if walked + len < s {
            walked += len;
            k += 1;
            continue;
        }
        let t = (s - walked) / len.max(1e-3);
        let p = a.lerp(b, t);
        let dir = (b - a).normalize_or_zero();
        let perp = Vec2::new(-dir.y, dir.x);
        let h = hash32(seed ^ (out.len() as u32).wrapping_mul(0x27d4_eb2d));
        // Sides alternate, with the odd gap where the ground says no.
        let side = if (out.len() & 1) == 0 { 1.0 } else { -1.0 };
        let jitter = (unit(h) - 0.5) * spacing * 0.25;
        let kerb = p + perp * side * half + dir * jitter;
        let at = kerb + perp * side * (setback + depth * 0.5);
        if ok(at, kerb) {
            out.push(Plot {
                at,
                // The front faces back across the street.
                face: crate::util::bearing(kerb - at).to_degrees(),
                kerb,
            });
        }
        s += spacing * 0.5;
    }
    out
}

/// Flat, dry, free ground in a square `half` m either side of `p`.
pub fn site_ok(f: &Fields, p: Vec2, half: f32) -> bool {
    if p.x < half || p.y < half || p.x > f.size - half || p.y > f.size - half {
        return false;
    }
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for dz in [-half, 0.0, half] {
        for dx in [-half, 0.0, half] {
            let i = f.idx(p.x + dx, p.y + dz);
            if f.water[i] != 0 || f.occupied[i] {
                return false;
            }
            lo = lo.min(f.h[i]);
            hi = hi.max(f.h[i]);
        }
    }
    lo > f.sea + 1.2 && hi - lo < half * 0.45
}

/// Is a plot buildable? The same test as `site_ok`, except on the street
/// side: a building that fronts a street necessarily stands on the ground
/// that street's grading feathered out over, so taken ground between the plot
/// and the kerb is what a frontage is, not a reason to refuse it. Water and
/// slope still are, everywhere.
pub fn plot_ok(f: &Fields, at: Vec2, kerb: Vec2, half: f32) -> bool {
    if at.x < half || at.y < half || at.x > f.size - half || at.y > f.size - half {
        return false;
    }
    let front = (kerb - at).normalize_or_zero();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for dz in [-half, 0.0, half] {
        for dx in [-half, 0.0, half] {
            let q = at + Vec2::new(dx, dz);
            let i = f.idx(q.x, q.y);
            // Open water and an old channel refuse a plot wherever they are.
            if f.water[i] == 1 || f.water[i] == 2 {
                return false;
            }
            // Behind the frontage the ground must be free.
            if Vec2::new(dx, dz).dot(front) <= 0.0 && (f.water[i] != 0 || f.occupied[i]) {
                return false;
            }
            lo = lo.min(f.h[i]);
            hi = hi.max(f.h[i]);
        }
    }
    lo > f.sea + 0.8 && hi - lo < half * 0.5
}
