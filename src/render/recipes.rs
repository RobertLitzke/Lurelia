//! Texture patterns for library materials: each recipe names a pattern (a verb
//! written here) with two colours, a scale and a contrast. One 32x32 tile covers a
//! metre, so a 0.25 m voxel face shows 8x8 texels. The calm style uses the
//! recipe's calm colours when given and softens every pattern.

use crate::library::Recipe;
use crate::world::noise::{hash2, unit};

pub const PATTERNS: [&str; 16] = [
    "flat", "noise", "speckle", "strata", "columns", "cracks", "blotch", "planks", "coursed", "glass", "ripples", "grain", "rings", "thatch", "gravel", "stars",
];

const SIZE: usize = 32;
type Rgb = [f32; 3];

fn vnoise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let p = period.max(1);
    let h = |x: i32, y: i32| unit(hash2(x.rem_euclid(p), y.rem_euclid(p), seed));
    let a = h(ix, iy) + (h(ix + 1, iy) - h(ix, iy)) * sx;
    let b = h(ix, iy + 1) + (h(ix + 1, iy + 1) - h(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

/// Tileable fractal noise in [0, 1]; `cells` lattice cells across the tile.
fn tfbm(x: f32, y: f32, cells: i32, oct: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
    for o in 0..oct {
        let f = cells.max(1) << o;
        sum += vnoise(x / SIZE as f32 * f as f32, y / SIZE as f32 * f as f32, f, seed + o * 31) * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

fn th(x: i32, y: i32, seed: u32) -> f32 {
    unit(hash2(x, y, seed))
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn seed_of(r: &Recipe) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for b in r.pattern.bytes().chain(r.base.iter().flat_map(|v| (*v as u32).to_le_bytes())) {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

/// One texel of a recipe, sRGB 0..255.
pub fn texel(r: &Recipe, calm: bool, x: usize, y: usize) -> Rgb {
    let seed = seed_of(r);
    let (base, alt) = match (&r.calm, calm) {
        (Some(c), true) => (c.base, c.alt.or(r.alt).unwrap_or(c.base)),
        _ => (r.base, r.alt.unwrap_or(r.base)),
    };
    // The calm block may also swap the pattern and scale its contrast.
    let calm_block = if calm { r.calm.as_ref() } else { None };
    let pattern = calm_block.and_then(|c| c.pattern.as_deref()).unwrap_or(r.pattern.as_str());
    let calm_k = calm_block.and_then(|c| c.contrast).unwrap_or(1.0);
    // The calm style keeps patterns but pulls the two colours together.
    let k = if calm { 0.4 * r.contrast * calm_k } else { r.contrast };
    let alt = mix(base, alt, k.min(1.0));
    let cells = (calm_block.and_then(|c| c.scale).unwrap_or(r.scale).round() as i32).clamp(1, 32);
    let (xi, yi) = (x as i32, y as i32);
    let (xf, yf) = (x as f32, y as f32);
    let grain = if calm { 0.0 } else { (th(xi, yi, seed ^ 7) - 0.5) * 10.0 * k.min(1.0) };
    let c = match pattern {
        "flat" => base,
        "noise" => mix(base, alt, tfbm(xf, yf, cells, 3, seed)),
        // Sparse single points of `alt` (about 3% of texels) on dark, noisy
        // `base`: glow-worms on a cave roof.
        "stars" => {
            let n = tfbm(xf, yf, cells, 2, seed);
            if th(xi, yi, seed + 1) < 0.03 {
                alt
            } else {
                mix(base, scale(base, 1.4), n)
            }
        }
        "speckle" => {
            let n = tfbm(xf, yf, cells, 2, seed);
            let c = mix(base, alt, n * 0.4);
            if th(xi, yi, seed + 1) < 0.12 {
                alt
            } else {
                c
            }
        }
        "strata" => {
            let band = vnoise(xf / SIZE as f32 * 2.0, yf / SIZE as f32 * cells as f32 * 2.0, (cells * 2).max(1), seed);
            let c = mix(base, alt, band);
            if (yi * cells / 8) % 4 == 3 && th(xi / 4, yi, seed + 2) > 0.3 {
                scale(c, 0.9)
            } else {
                c
            }
        }
        "columns" => {
            // Vertical joints like columnar basalt, each column a slightly different tone.
            let w = (SIZE as i32 / cells).max(2);
            let col = xi.div_euclid(w);
            let t = th(col, 0, seed);
            let c = mix(base, alt, t * 0.6);
            let edge = xi.rem_euclid(w) == 0;
            let joint = yi.rem_euclid(SIZE as i32) == (th(col, 1, seed) * SIZE as f32) as i32;
            if edge || joint {
                scale(c, 0.7)
            } else {
                c
            }
        }
        "cracks" => {
            // Cell edges of a jittered grid read as cracks.
            let w = (SIZE as f32 / cells as f32).max(2.0);
            let (gx, gy) = ((xf / w).floor() as i32, (yf / w).floor() as i32);
            let mut d1 = f32::MAX;
            let mut d2 = f32::MAX;
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let (cx, cy) = (gx + ox, gy + oy);
                    // Points kept to the middle 70% of their cell: two points
                    // nearly meeting fill a patch with crack, which repeats
                    // every metre and gives the tile away.
                    let px = (cx as f32 + 0.15 + 0.7 * th(cx.rem_euclid(cells), cy.rem_euclid(cells), seed)) * w;
                    let py = (cy as f32 + 0.15 + 0.7 * th(cy.rem_euclid(cells), cx.rem_euclid(cells), seed + 9)) * w;
                    let d = ((px - xf).powi(2) + (py - yf).powi(2)).sqrt();
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
            let c = mix(base, alt, tfbm(xf, yf, cells, 2, seed + 3) * 0.5);
            if d2 - d1 < 1.2 {
                scale(alt, if calm { 1.0 - 0.25 * calm_k } else { 0.75 })
            } else {
                c
            }
        }
        "blotch" => {
            let n = tfbm(xf, yf, cells, 2, seed);
            let t = ((n - 0.42) / 0.16).clamp(0.0, 1.0);
            let c = mix(base, alt, t * t * (3.0 - 2.0 * t));
            if !calm && th(xi, yi, seed + 4) > 0.93 {
                scale(c, 1.15)
            } else {
                c
            }
        }
        "planks" => {
            let row = yi / 4;
            let joint = (th(0, row, seed) * SIZE as f32) as i32;
            let g = vnoise(xf / SIZE as f32 * 4.0, yf / SIZE as f32 * 32.0, 32, seed + 5);
            let c = scale(mix(base, alt, g), 0.94 + 0.1 * th(row, 3, seed));
            if yi % 4 == 3 || xi == joint {
                scale(c, 0.72)
            } else {
                c
            }
        }
        "coursed" => {
            // Courses of dressed or rough stone with paler joints.
            let row = yi / 8;
            let off = if row % 2 == 0 { 0 } else { 5 };
            let len = 10 + (th(row, 7, seed) * 6.0) as i32;
            let block = (xi + off) / len;
            let joint = yi % 8 == 7 || (xi + off) % len == len - 1;
            if joint {
                mix(base, [200.0, 190.0, 170.0], 0.4)
            } else {
                scale(mix(base, alt, th(block, row, seed)), 0.92 + 0.12 * tfbm(xf, yf, 4, 2, seed + 6))
            }
        }
        "glass" => {
            // Dark, nearly uniform, with bright diagonal streaks.
            let streak = ((xf + yf * 0.6) / SIZE as f32 * cells as f32 * std::f32::consts::TAU).sin();
            let n = tfbm(xf, yf, 2, 2, seed);
            let c = mix(base, alt, (streak * 0.5 + 0.5).powf(8.0) * 0.9 + n * 0.1);
            if th(xi, yi, seed + 8) > 0.985 {
                [240.0, 240.0, 250.0]
            } else {
                c
            }
        }
        "ripples" => {
            let n = tfbm(xf, yf, 4, 2, seed);
            let r = ((yf + n * 6.0) / SIZE as f32 * cells as f32 * std::f32::consts::TAU).sin();
            mix(base, alt, r * 0.5 + 0.5)
        }
        "grain" => {
            let n = vnoise(xf / SIZE as f32 * 16.0, yf / SIZE as f32 * 2.0, 16, seed);
            let c = mix(base, alt, n);
            if th(xi, yi / 4, seed + 1) < 0.1 {
                scale(c, 0.72)
            } else {
                c
            }
        }
        "rings" => {
            let (vx, vy) = ((xi % 8) as f32 + 0.5 - 4.0, (yi % 8) as f32 + 0.5 - 4.0);
            let d = (vx * vx + vy * vy).sqrt();
            if d > 3.3 {
                scale(base, 0.8)
            } else {
                mix(base, alt, (d * 2.1).sin() * 0.5 + 0.5)
            }
        }
        "thatch" => {
            let s = ((xf * 0.7 + yf) / SIZE as f32 * 24.0 + th(xi / 2, 0, seed) * 3.0).sin();
            let row = yi / 6;
            let c = mix(base, alt, s * 0.5 + 0.5);
            if yi % 6 == 5 {
                scale(c, 0.8)
            } else {
                scale(c, 0.95 + 0.1 * th(row, xi / 6, seed))
            }
        }
        "gravel" => super::texsets::gravel(base, alt, cells, seed, xf + 0.5, yf + 0.5),
        _ => [255.0, 0.0, 255.0],
    };
    [c[0] + grain, c[1] + grain, c[2] + grain]
}
