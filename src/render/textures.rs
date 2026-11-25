//! Synthesised block textures. Each layer covers one metre. The calm set and the
//! old speckled set here are drawn at 32x32 (8x8 texels on a 0.25 m voxel face);
//! calm is doubled into the 64x64 GPU tile, and the classic style's tiles are
//! painted from the old set by texsets.rs. Built from tileable noise at startup.
//!
//! Two sets: `Classic` has speckle, cracks and grain like a block game; `Calm` is
//! near-flat colour with soft, low-frequency variation and a warmer, painted
//! palette, for a flatter, illustrated look.

use crate::world::material::layer;
use crate::world::noise::{hash2, unit};

/// Texels across the GPU tile. The classic and calm sets are drawn at `TILE`
/// and doubled; the experimental sets (texsets.rs) use the full resolution.
pub const SIZE: usize = 64;
pub const MIPS: u32 = 7;
const TILE: usize = 32;

fn vnoise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let h = |x: i32, y: i32| unit(hash2(x.rem_euclid(period), y.rem_euclid(period), seed));
    let a = h(ix, iy) + (h(ix + 1, iy) - h(ix, iy)) * sx;
    let b = h(ix, iy + 1) + (h(ix + 1, iy + 1) - h(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

/// Tileable fractal noise in [0, 1] over a 32-texel tile; `px`/`py` are lattice
/// cells across the tile on each axis at the first octave.
fn tfbm(x: f32, y: f32, px: i32, py: i32, oct: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    for o in 0..oct {
        let (fx, fy) = (px << o, py << o);
        sum += vnoise(x / TILE as f32 * fx as f32, y / TILE as f32 * fy as f32, fx.max(fy), seed + o * 31) * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

fn th(x: usize, y: usize, seed: u32) -> f32 {
    unit(hash2(x as i32, y as i32, seed))
}

type Rgb = [f32; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn grass_col(x: usize, y: usize) -> Rgb {
    let n = tfbm(x as f32, y as f32, 4, 4, 3, 11);
    let base = mix([78.0, 116.0, 50.0], [104.0, 136.0, 62.0], n);
    let h = th(x, y, 12);
    if h < 0.13 {
        scale(base, 0.78)
    } else if h > 0.95 {
        mix(base, [168.0, 186.0, 92.0], 0.5)
    } else {
        base
    }
}

fn dirt_col(x: usize, y: usize) -> Rgb {
    let n = tfbm(x as f32, y as f32, 4, 4, 3, 21);
    let base = mix([104.0, 72.0, 48.0], [134.0, 96.0, 64.0], n);
    let h = th(x, y, 22);
    if h < 0.06 {
        [86.0, 62.0, 44.0]
    } else if h > 0.96 {
        [158.0, 128.0, 96.0]
    } else {
        base
    }
}

fn stone_col(x: usize, y: usize, seed: u32, dark: Rgb, light: Rgb) -> Rgb {
    let n = tfbm(x as f32, y as f32, 4, 4, 3, seed);
    let mut c = mix(dark, light, n);
    let r = tfbm(x as f32, y as f32, 4, 4, 2, seed + 7);
    let ridge = 1.0 - (2.0 * r - 1.0).abs();
    if ridge > 0.94 {
        c = scale(c, 0.72);
    }
    let h = th(x, y, seed + 9);
    if h > 0.97 {
        c = scale(c, 1.15);
    }
    c
}

fn leaves_col(x: usize, y: usize, seed: u32, dark: Rgb, mid: Rgb, hi: Rgb) -> Rgb {
    let n = tfbm(x as f32, y as f32, 8, 8, 2, seed);
    let h = th(x, y, seed + 1);
    if n < 0.33 {
        dark
    } else if h > 0.88 {
        hi
    } else {
        mix(mid, hi, (n - 0.33) * 0.6)
    }
}

fn texel(l: u32, x: usize, y: usize) -> Rgb {
    let (vx, vy) = (x % 8, y % 8);
    match l {
        layer::GRASS_TOP => grass_col(x, y),
        layer::GRASS_SIDE => {
            let row = y / 8;
            let fringe = 2 + (unit(hash2(x as i32, row as i32, 5)) * 2.6) as usize;
            if vy < fringe {
                scale(grass_col(x, y), 0.92)
            } else {
                dirt_col(x, y)
            }
        }
        layer::DIRT => dirt_col(x, y),
        layer::SAND => {
            let n = tfbm(x as f32, y as f32, 4, 4, 2, 31);
            let ripple = ((y as f32 + n * 6.0) * 0.785).sin() * 5.0;
            let g = (th(x, y, 32) - 0.5) * 14.0;
            [218.0 + ripple + g, 200.0 + ripple + g, 150.0 + ripple * 0.6 + g]
        }
        layer::STONE => stone_col(x, y, 41, [104.0, 106.0, 108.0], [150.0, 150.0, 146.0]),
        layer::STONE_DARK => stone_col(x, y, 51, [70.0, 74.0, 86.0], [104.0, 108.0, 120.0]),
        layer::STONE_LIGHT => {
            let mut c = stone_col(x, y, 61, [160.0, 148.0, 128.0], [196.0, 186.0, 166.0]);
            if vy == 3 && th(x / 3, y, 62) > 0.3 {
                c = scale(c, 0.86);
            }
            c
        }
        layer::BARK => {
            let n = vnoise(x as f32 / 32.0 * 16.0, y as f32 / 32.0 * 2.0, 16, 71);
            let c = mix([76.0, 54.0, 36.0], [116.0, 86.0, 58.0], n);
            if th(x, y / 4, 72) < 0.12 {
                scale(c, 0.7)
            } else {
                c
            }
        }
        layer::BARK_TOP => {
            let (cx, cy) = (vx as f32 + 0.5 - 4.0, vy as f32 + 0.5 - 4.0);
            let r = (cx * cx + cy * cy).sqrt();
            if r > 3.3 {
                [96.0, 70.0, 46.0]
            } else if (r * 2.1).sin() > 0.2 {
                [176.0, 140.0, 96.0]
            } else {
                [146.0, 110.0, 72.0]
            }
        }
        layer::LEAVES => leaves_col(x, y, 81, [30.0, 58.0, 30.0], [56.0, 98.0, 44.0], [92.0, 138.0, 62.0]),
        layer::PINE => {
            let streak = vnoise(x as f32 / 32.0 * 16.0, y as f32 / 32.0 * 4.0, 16, 91);
            let c = mix([26.0, 56.0, 42.0], [52.0, 92.0, 62.0], streak);
            if th(x, y, 92) > 0.9 {
                mix(c, [90.0, 130.0, 86.0], 0.5)
            } else {
                c
            }
        }
        layer::PLANKS => {
            let row = y / 4;
            let joint = (unit(hash2(0, row as i32, 101)) * 32.0) as usize;
            let grain = vnoise(x as f32 / 32.0 * 4.0, y as f32 / 32.0 * 32.0, 32, 102);
            let mut c = mix([150.0, 108.0, 66.0], [186.0, 142.0, 92.0], grain);
            c = scale(c, 0.92 + 0.12 * unit(hash2(row as i32, 3, 103)));
            if y % 4 == 3 || x == joint {
                c = scale(c, 0.68);
            }
            c
        }
        layer::GRAVEL => {
            let cell = hash2((x / 3) as i32, (y / 3) as i32, 111);
            let t = unit(cell);
            let c = mix([96.0, 92.0, 88.0], [158.0, 150.0, 140.0], t);
            if x % 3 == 2 || y % 3 == 2 {
                scale(c, 0.8)
            } else {
                c
            }
        }
        layer::BIRCH_BARK => {
            let c = mix([214.0, 210.0, 198.0], [236.0, 234.0, 226.0], th(x, y, 121));
            let dash = unit(hash2((x / 3) as i32, y as i32, 122));
            if dash < 0.1 && y % 2 == 0 {
                [40.0, 38.0, 36.0]
            } else {
                c
            }
        }
        layer::LEAVES_AUTUMN => {
            let t = tfbm(x as f32, y as f32, 4, 4, 2, 131);
            let mid = mix([196.0, 92.0, 30.0], [222.0, 160.0, 46.0], t);
            leaves_col(x, y, 132, [120.0, 50.0, 22.0], mid, [240.0, 190.0, 80.0])
        }
        layer::SNOW => {
            let n = tfbm(x as f32, y as f32, 4, 4, 2, 141);
            mix([222.0, 230.0, 240.0], [246.0, 248.0, 252.0], n)
        }
        layer::BIRCH_LEAVES => leaves_col(x, y, 151, [70.0, 104.0, 36.0], [118.0, 158.0, 58.0], [170.0, 196.0, 90.0]),
        layer::BRICK => {
            let row = y / 4;
            let off = if row % 2 == 0 { 0 } else { 4 };
            if y % 4 == 3 || (x + off) % 8 == 7 {
                [188.0, 180.0, 166.0]
            } else {
                let t = unit(hash2(((x + off) / 8) as i32, row as i32, 161));
                let c = mix([136.0, 60.0, 44.0], [170.0, 84.0, 60.0], t);
                scale(c, 0.92 + 0.12 * th(x, y, 162))
            }
        }
        _ => [255.0, 0.0, 255.0],
    }
}

/// Soft variation over the tile: `amp` is the +- fraction of brightness.
fn soft(c: Rgb, x: usize, y: usize, seed: u32, amp: f32) -> Rgb {
    let n = tfbm(x as f32, y as f32, 2, 2, 2, seed) * 2.0 - 1.0;
    scale(c, 1.0 + n * amp)
}

/// Two-tone blotches with a soft edge between them, for foliage.
fn blotch(x: usize, y: usize, seed: u32, a: Rgb, b: Rgb) -> Rgb {
    let n = tfbm(x as f32, y as f32, 4, 4, 2, seed);
    let t = ((n - 0.42) / 0.16).clamp(0.0, 1.0);
    mix(a, b, t * t * (3.0 - 2.0 * t))
}

fn calm_texel(l: u32, x: usize, y: usize) -> Rgb {
    let (vx, vy) = (x % 8, y % 8);
    let grass = |x, y| soft([102.0, 130.0, 60.0], x, y, 201, 0.035);
    let dirt = |x, y| soft([160.0, 98.0, 50.0], x, y, 202, 0.04);
    match l {
        layer::GRASS_TOP => grass(x, y),
        layer::GRASS_SIDE => {
            // All grass, a little darker toward the bottom: the surface voxel's
            // sides are one-voxel steps on a slope, and soil stripes on every
            // step make meadows busy. Deeper voxels are dirt and still show it.
            scale(grass(x, y), 1.0 - vy as f32 * 0.018)
        }
        layer::DIRT => dirt(x, y),
        layer::SAND => soft([214.0, 182.0, 124.0], x, y, 204, 0.025),
        layer::STONE => {
            // Faint horizontal strata, as if the cliff were painted in flat planes.
            let band = vnoise(x as f32 / 32.0 * 2.0, y as f32 / 32.0 * 8.0, 8, 205);
            soft(mix([150.0, 128.0, 112.0], [168.0, 146.0, 126.0], band), x, y, 206, 0.02)
        }
        layer::STONE_DARK => {
            let band = vnoise(x as f32 / 32.0 * 2.0, y as f32 / 32.0 * 8.0, 8, 207);
            soft(mix([78.0, 84.0, 104.0], [92.0, 98.0, 116.0], band), x, y, 208, 0.02)
        }
        layer::STONE_LIGHT => {
            let band = vnoise(x as f32 / 32.0 * 2.0, y as f32 / 32.0 * 8.0, 8, 209);
            let c = mix([206.0, 172.0, 128.0], [220.0, 188.0, 142.0], band);
            if vy == 3 {
                scale(c, 0.95)
            } else {
                c
            }
        }
        layer::BARK => {
            let n = vnoise(x as f32 / 32.0 * 8.0, y as f32 / 32.0 * 2.0, 8, 210);
            mix([104.0, 64.0, 40.0], [122.0, 78.0, 48.0], n)
        }
        layer::BARK_TOP => {
            let (cx, cy) = (vx as f32 + 0.5 - 4.0, vy as f32 + 0.5 - 4.0);
            let r = (cx * cx + cy * cy).sqrt();
            if r > 3.3 {
                [104.0, 78.0, 56.0]
            } else {
                mix([170.0, 136.0, 96.0], [156.0, 122.0, 84.0], (r * 1.4).sin() * 0.5 + 0.5)
            }
        }
        layer::LEAVES => blotch(x, y, 211, [48.0, 92.0, 48.0], [64.0, 110.0, 56.0]),
        layer::PINE => blotch(x, y, 212, [26.0, 76.0, 58.0], [36.0, 90.0, 64.0]),
        layer::PLANKS => {
            let row = y / 4;
            let joint = (unit(hash2(0, row as i32, 213)) * 32.0) as usize;
            let c = scale([178.0, 132.0, 88.0], 0.97 + 0.06 * unit(hash2(row as i32, 3, 214)));
            if y % 4 == 3 || x == joint {
                scale(c, 0.86)
            } else {
                c
            }
        }
        layer::GRAVEL => {
            let t = unit(hash2((x / 4) as i32, (y / 4) as i32, 215));
            mix([144.0, 136.0, 128.0], [160.0, 152.0, 142.0], t)
        }
        layer::BIRCH_BARK => {
            let c = soft([228.0, 222.0, 208.0], x, y, 216, 0.02);
            let dash = unit(hash2((x / 4) as i32, (y / 2) as i32, 217));
            if dash < 0.08 && y % 2 == 0 {
                [96.0, 90.0, 84.0]
            } else {
                c
            }
        }
        layer::LEAVES_AUTUMN => blotch(x, y, 218, [214.0, 98.0, 30.0], [238.0, 146.0, 40.0]),
        layer::SNOW => soft([238.0, 242.0, 248.0], x, y, 219, 0.01),
        layer::BIRCH_LEAVES => blotch(x, y, 220, [124.0, 150.0, 60.0], [144.0, 166.0, 70.0]),
        layer::BRICK => {
            let row = y / 4;
            let off = if row % 2 == 0 { 0 } else { 4 };
            if y % 4 == 3 || (x + off) % 8 == 7 {
                [196.0, 182.0, 164.0]
            } else {
                let t = unit(hash2(((x + off) / 8) as i32, row as i32, 221));
                mix([166.0, 92.0, 70.0], [178.0, 104.0, 78.0], t)
            }
        }
        _ => [255.0, 0.0, 255.0],
    }
}

fn to_lin(c: f32) -> f32 {
    (c / 255.0).powf(2.2)
}

fn to_srgb(c: f32) -> u8 {
    (c.max(0.0).powf(1.0 / 2.2) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Mips built ahead (`prebuild`): the set, the material table's generation
/// they were painted from, and the mips.
type Built = (bool, u32, Vec<Vec<Vec<u8>>>);
static PREBUILT: std::sync::Mutex<Option<Built>> = std::sync::Mutex::new(None);

/// Paint the blocks for `calm` now, off the main thread (a world's loader,
/// once its materials are installed), for the renderer to take (`take_or_build`)
/// rather than paint them itself in the first frame: about 300 ms.
pub fn prebuild(calm: bool) {
    let gen = crate::world::matlib::generation();
    let mips = build(calm);
    *PREBUILT.lock().unwrap_or_else(|e| e.into_inner()) = Some((calm, gen, mips));
}

/// `build(calm)`, from what `prebuild` left if it is that set painted from
/// the material table now installed.
pub fn take_or_build(calm: bool) -> Vec<Vec<Vec<u8>>> {
    let gen = crate::world::matlib::generation();
    let mut slot = PREBUILT.lock().unwrap_or_else(|e| e.into_inner());
    // Another set (the renderer's start-up blocks) leaves it for its turn.
    if slot.as_ref().is_some_and(|(c, g, _)| *c == calm && *g == gen) {
        return slot.take().unwrap().2;
    }
    drop(slot);
    build(calm)
}

/// RGBA8 sRGB data for every mip of every layer: `out[mip][layer]`.
/// `calm` is the saturated style's flat set; otherwise the classic style's
/// tiles (texsets.rs).
pub fn build(calm: bool) -> Vec<Vec<Vec<u8>>> {
    let mut mips = vec![Vec::new(); MIPS as usize];
    let mut sheet: Vec<Vec<u8>> = Vec::new();
    let extra = crate::world::matlib::table().map(|t| t.recipes.as_slice()).unwrap_or(&[]);
    // The classic style paints whole tiles (render/texsets.rs) from the old
    // per-texel set below; saturated uses its own set directly.
    for l in 0..layer::COUNT + extra.len() as u32 {
        let mut lin = vec![[0f32; 3]; SIZE * SIZE];
        let mut base = vec![0u8; SIZE * SIZE * 4];
        let recipe = (l >= layer::COUNT).then(|| &extra[(l - layer::COUNT) as usize]);
        // Classic tiles are painted whole (they blur and mix).
        let classic = |cl: u32, x: usize, y: usize| match recipe {
            Some(r) => super::recipes::texel(r, false, x, y),
            None => texel(cl, x, y),
        };
        let tile = (!calm).then(|| super::texsets::tile(l, recipe, &classic));
        for y in 0..SIZE {
            for x in 0..SIZE {
                let c = if let Some(t) = &tile {
                    t[y * SIZE + x]
                } else if let Some(r) = recipe {
                    super::recipes::texel(r, calm, x / 2, y / 2)
                } else if calm {
                    calm_texel(l, x / 2, y / 2)
                } else {
                    texel(l, x / 2, y / 2)
                };
                let i = y * SIZE + x;
                for k in 0..3 {
                    let v = c[k].clamp(0.0, 255.0);
                    base[i * 4 + k] = v.round() as u8;
                    lin[i][k] = to_lin(v);
                }
                base[i * 4 + 3] = 255;
            }
        }
        sheet.push(base.clone());
        mips[0].push(base);
        let mut size = SIZE;
        for m in 1..MIPS as usize {
            let ns = size / 2;
            let mut next = vec![[0f32; 3]; ns * ns];
            let mut bytes = vec![0u8; ns * ns * 4];
            for y in 0..ns {
                for x in 0..ns {
                    let mut acc = [0f32; 3];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let s = lin[(y * 2 + dy) * size + x * 2 + dx];
                        for k in 0..3 {
                            acc[k] += s[k] * 0.25;
                        }
                    }
                    next[y * ns + x] = acc;
                    for k in 0..3 {
                        bytes[(y * ns + x) * 4 + k] = to_srgb(acc[k]);
                    }
                    bytes[(y * ns + x) * 4 + 3] = 255;
                }
            }
            mips[m].push(bytes);
            lin = next;
            size = ns;
        }
    }
    if let Ok(path) = std::env::var("VOXEL_TEX_SHEET") {
        write_sheet(&path, &sheet);
    }
    mips
}

/// Every layer's base tile in a row-major grid, 8 across, for comparing sets.
fn write_sheet(path: &str, tiles: &[Vec<u8>]) {
    let cols = 8;
    let rows = tiles.len().div_ceil(cols);
    let (w, h) = (cols * SIZE, rows * SIZE);
    let mut img = vec![0u8; w * h * 4];
    for (i, t) in tiles.iter().enumerate() {
        let (ox, oy) = ((i % cols) * SIZE, (i / cols) * SIZE);
        for y in 0..SIZE {
            let d = ((oy + y) * w + ox) * 4;
            img[d..d + SIZE * 4].copy_from_slice(&t[y * SIZE * 4..(y + 1) * SIZE * 4]);
        }
    }
    let _ = crate::util::save_png(path, w as u32, h as u32, &img);
}
