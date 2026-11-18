//! `cargo test --release export_texture_options -- --ignored`: renders a set of
//! candidate textures for one material to PNGs, so the look can be judged as a
//! field of blocks rather than one tile. `TEXOPTS_OUT=DIR` says where.
//!
//! Each option is written three ways: the 64x64 tile; an 8x8 field of blocks as
//! the game lays them now (every block the same tile, which is what makes a
//! strong pattern read as a grid); and the same field with the tile turned and
//! flipped per block, which costs one hash in the shader and breaks the grid.

#![cfg(test)]

use super::texsets;
use crate::library::Recipe;

type Rgb = [f32; 3];
const N: usize = 64;

fn recipe(pattern: &str, base: Rgb, alt: Rgb, scale: f32, contrast: f32) -> Recipe {
    Recipe {
        pattern: pattern.to_string(),
        base,
        alt: Some(alt),
        scale,
        contrast,
        calm: None,
    }
}

fn tile_of(r: &Recipe) -> Vec<Rgb> {
    let classic = |_cl: u32, x: usize, y: usize| super::recipes::texel(r, false, x, y);
    texsets::tile(crate::world::material::layer::COUNT, Some(r), &classic)
}

/// An 8x8 field of 1 m blocks. `vary` turns and flips each block by a hash of
/// its position, the way a shader could from the block's own coordinates.
fn field(t: &[Rgb], vary: bool) -> Vec<Rgb> {
    let blocks = 8;
    let w = blocks * N;
    let mut out = vec![[0.0; 3]; w * w];
    for by in 0..blocks {
        for bx in 0..blocks {
            let h = crate::world::noise::hash2(bx as i32, by as i32, 7717);
            let (turn, flip) = if vary { ((h >> 3) & 3, (h >> 9) & 1) } else { (0, 0) };
            for y in 0..N {
                for x in 0..N {
                    let (mut sx, mut sy) = (x, y);
                    for _ in 0..turn {
                        let (px, py) = (sx, sy);
                        sx = py;
                        sy = N - 1 - px;
                    }
                    if flip == 1 {
                        sx = N - 1 - sx;
                    }
                    out[(by * N + y) * w + bx * N + x] = t[sy * N + sx];
                }
            }
        }
    }
    out
}

fn write_png(path: &std::path::Path, w: u32, h: u32, px: &[Rgb]) -> std::io::Result<()> {
    let mut bytes = vec![0u8; px.len() * 3];
    for (i, c) in px.iter().enumerate() {
        for k in 0..3 {
            bytes[i * 3 + k] = c[k].clamp(0.0, 255.0).round() as u8;
        }
    }
    crate::util::save_png(path, w, h, &bytes)
}

/// The options, in the order they are written. Chalk rubble's own colours
/// throughout, so only the pattern is being judged.
fn options() -> Vec<(&'static str, Recipe)> {
    let base = [170.0, 162.0, 146.0];
    let alt = [150.0, 144.0, 132.0];
    // A paler second colour: most of what makes the current one shout is the
    // distance between the two tones, not the shapes.
    let near = [162.0, 155.0, 141.0];
    vec![
        ("1_now_gravel_11_c50", recipe("gravel", base, alt, 11.0, 0.5)),
        ("2_gravel_11_c22", recipe("gravel", base, alt, 11.0, 0.22)),
        ("3_gravel_6_c35", recipe("gravel", base, alt, 6.0, 0.35)),
        ("4_gravel_4_c30", recipe("gravel", base, alt, 4.0, 0.3)),
        ("5_gravel_6_c20_near", recipe("gravel", base, near, 6.0, 0.2)),
        ("6_speckle_8_c25", recipe("speckle", base, alt, 8.0, 0.25)),
        ("7_blotch_3_c30", recipe("blotch", base, alt, 3.0, 0.3)),
        ("8_noise_5_c25", recipe("noise", base, alt, 5.0, 0.25)),
        ("9_cracks_4_c25", recipe("cracks", base, alt, 4.0, 0.25)),
        ("10_grain_6_c20", recipe("grain", base, near, 6.0, 0.2)),
    ]
}

#[test]
#[ignore]
fn export_texture_options() {
    let dir = std::path::PathBuf::from(std::env::var("TEXOPTS_OUT").unwrap_or_else(|_| "texopts".into()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, r) in options() {
        let t = tile_of(&r);
        write_png(&dir.join(format!("{name}_tile.png")), N as u32, N as u32, &t).unwrap();
        let w = (8 * N) as u32;
        write_png(&dir.join(format!("{name}_field.png")), w, w, &field(&t, false)).unwrap();
        write_png(&dir.join(format!("{name}_varied.png")), w, w, &field(&t, true)).unwrap();
        println!("wrote {name}");
    }
}
