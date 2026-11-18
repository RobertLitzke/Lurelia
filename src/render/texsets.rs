//! The classic style's block textures: the old speckled set (textures.rs
//! `texel`, recipes.rs) softened toward its own blur, drawn at 64 texels with
//! smooth filtering, grass sides that are all grass, and colours moved off the
//! block-game palette (redder soils and wood, warm greys; greens left alone).
//! It replaced the speckled set on 2026-11-08. Gravel recipes are drawn at full
//! resolution. The saturated style keeps its own flat set (textures.rs).

use crate::library::Recipe;
use crate::world::material::layer;
use crate::world::noise::{hash2, unit};

const SF: f32 = 32.0;
type Rgb = [f32; 3];

/// Scale on scene.wgsl's per-voxel brightness and hue jitter for this set.
pub const JITTER: f32 = 0.3;

/// How much of each texel's difference from its neighbourhood the set keeps.
const DETAIL: f32 = 0.45;

// ---- noise and colour ------------------------------------------------------

fn h(x: i32, y: i32, seed: u32) -> f32 {
    unit(hash2(x, y, seed))
}

/// Tileable Voronoi with `n` cells across the tile: (nearest distance, second
/// distance, nearest cell's hash in [0, 1], offset from that cell's point).
fn voronoi(x: f32, y: f32, n: i32, seed: u32) -> (f32, f32, f32, [f32; 2]) {
    let w = SF / n as f32;
    let (gx, gy) = ((x / w).floor() as i32, (y / w).floor() as i32);
    let (mut d1, mut d2, mut id, mut off) = (f32::MAX, f32::MAX, 0.0, [0.0, 0.0]);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let (cx, cy) = (gx + ox, gy + oy);
            let (wx, wy) = (cx.rem_euclid(n), cy.rem_euclid(n));
            let px = (cx as f32 + 0.15 + 0.7 * h(wx, wy, seed)) * w;
            let py = (cy as f32 + 0.15 + 0.7 * h(wy, wx, seed + 9)) * w;
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = h(wx, wy, seed + 17);
                off = [x - px, y - py];
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1, d2, id, off)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn mul(a: Rgb, b: Rgb) -> Rgb {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn colours(r: &Recipe) -> (Rgb, Rgb) {
    (r.base, r.alt.unwrap_or(r.base))
}

// ---- whole tiles -------------------------------------------------------------

const N: usize = 64;

/// A whole 64x64 tile of layer `l`. `classic(layer, x, y)` gives the old
/// speckled texel at 32-texel resolution, which the tile is painted from.
pub fn tile(l: u32, recipe: Option<&Recipe>, classic: &dyn Fn(u32, usize, usize) -> Rgb) -> Vec<Rgb> {
    // Classic at 32 texels, sampled bilinearly (wrapping) up to 64.
    let src = if l == layer::GRASS_SIDE { layer::GRASS_TOP } else { l };
    let small: Vec<Rgb> = (0..32 * 32).map(|i| classic(src, i % 32, i / 32)).collect();
    let bil = |x: usize, y: usize| -> Rgb {
        let (u, v) = ((x as f32 + 0.5) * 0.5 - 0.5, (y as f32 + 0.5) * 0.5 - 0.5);
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |x: f32, y: f32| small[(y as i32).rem_euclid(32) as usize * 32 + (x as i32).rem_euclid(32) as usize];
        mix(mix(at(x0, y0), at(x0 + 1.0, y0), fx), mix(at(x0, y0 + 1.0), at(x0 + 1.0, y0 + 1.0), fx), fy)
    };
    // Gravel is drawn at full resolution and barely softened: blurred stones
    // read as mush.
    let gravel_rec = recipe.filter(|r| r.pattern == "gravel");
    let mut t: Vec<Rgb> = (0..N * N)
        .map(|i| {
            let (x, y) = (i % N, i / N);
            if let Some(r) = gravel_rec {
                let (b, a) = colours(r);
                let cells = (r.scale.round() as i32).clamp(1, 16);
                return gravel(b, mix(b, a, r.contrast.min(1.0)), cells, recipe_seed(r), (x as f32 + 0.5) * 0.5, (y as f32 + 0.5) * 0.5);
            }
            bil(x, y)
        })
        .collect();
    let k = if gravel_rec.is_some() { 0.75 } else { DETAIL };
    if k < 1.0 {
        let b = blur(&blur(&t, 3), 3);
        for (c, m) in t.iter_mut().zip(&b) {
            *c = mix(*m, *c, k);
        }
    }
    if l == layer::GRASS_SIDE {
        for (i, c) in t.iter_mut().enumerate() {
            *c = scale(*c, 1.0 - ((i / N) % 16) as f32 * 0.01);
        }
    }
    for c in t.iter_mut() {
        *c = repaint(*c);
    }
    t
}

/// Separable box blur of radius `r`, wrapping at the tile edges.
fn blur(t: &[Rgb], r: i32) -> Vec<Rgb> {
    let n = N as i32;
    let pass = |src: &[Rgb], horiz: bool| -> Vec<Rgb> {
        (0..N * N)
            .map(|i| {
                let (x, y) = ((i % N) as i32, (i / N) as i32);
                let mut acc = [0.0; 3];
                for d in -r..=r {
                    let (sx, sy) = if horiz { ((x + d).rem_euclid(n), y) } else { (x, (y + d).rem_euclid(n)) };
                    let c = src[(sy * n + sx) as usize];
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
                scale(acc, 1.0 / (2 * r + 1) as f32)
            })
            .collect()
    };
    pass(&pass(t, true), false)
}

/// Moves a colour off the familiar block-game palette: soils and wood toward
/// red umber, near-greys a little warm. Greens stay as they are.
fn repaint(c: Rgb) -> Rgb {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let v = mx;
    let s = if mx > 0.0 { d / mx } else { 0.0 };
    if s < 0.12 {
        return mul(c, [1.03, 1.0, 0.95]);
    }
    let mut h = if mx == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let mut s2 = s;
    if (15.0..55.0).contains(&h) {
        h -= 8.0;
        s2 *= 0.92;
    }
    // Back to RGB.
    let cc = v * s2;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = cc * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (cc, x, 0.0),
        1 => (x, cc, 0.0),
        2 => (0.0, cc, x),
        3 => (0.0, x, cc),
        4 => (x, 0.0, cc),
        _ => (cc, 0.0, x),
    };
    let m = v - cc;
    [(r1 + m) * 255.0, (g1 + m) * 255.0, (b1 + m) * 255.0]
}

fn recipe_seed(r: &Recipe) -> u32 {
    let mut s = 0x811c_9dc5u32;
    for b in r.pattern.bytes().chain(r.base.iter().flat_map(|v| (*v as u32).to_le_bytes())) {
        s = (s ^ b as u32).wrapping_mul(0x0100_0193);
    }
    s
}

/// Packed gravel: rounded stones of two sizes, each its own tone between `base`
/// and `alt`, lit from the upper left with a darker rim, dark gaps between.
/// `cells` larger stones across the metre tile; (xf, yf) in 32-texel units, so
/// the same pattern draws at 32 texels (recipes.rs) or 64 (repaint).
pub fn gravel(base: Rgb, alt: Rgb, cells: i32, seed: u32, xf: f32, yf: f32) -> Rgb {
    let gap = scale(mix(base, alt, 0.5), 0.8);
    let stone = |n: i32, s: u32, size: f32| -> Option<Rgb> {
        let (d1, d2, id, off) = voronoi(xf, yf, n, s);
        let w = SF / n as f32;
        let r = w * (0.42 + 0.2 * h((id * 997.0) as i32, 3, s)) * size;
        let edge = (d2 - d1) * 0.5;
        // A stone fills its cell up to its radius, and never crosses into the next.
        if d1 > r || edge < 0.35 {
            return None;
        }
        let tone = mix(base, alt, id * 0.55);
        let tone = mul(tone, if h((id * 991.0) as i32, 5, s) > 0.6 { [1.05, 1.0, 0.95] } else { [0.97, 0.99, 1.03] });
        let lit = 1.0 - ((off[0] + off[1]) / r) * 0.1;
        let rim = 1.0 - smooth(0.55, 1.0, d1 / r) * 0.14;
        Some(scale(tone, lit * rim))
    };
    stone(cells.max(2), seed, 1.25).or_else(|| stone(cells.max(2) * 2, seed + 7, 1.4)).unwrap_or(gap)
}
