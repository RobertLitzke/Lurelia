//! Small helpers shared by every layer, with no knowledge of the game.

use std::hash::{BuildHasherDefault, Hasher};

/// The "Fx" hash used inside rustc: one rotate, xor and multiply per word.
/// Several times faster than the standard SipHash for the small integer keys
/// of the voxel store, the streamer and the world grids, and deterministic,
/// so iterating a map gives the same order on every run. It is not resistant
/// to crafted keys, which doesn't matter here.
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

const K: u64 = 0x517c_c1b7_2722_0a95;

impl FxHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            self.add(u64::from_le_bytes(b));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// Compass bearing of a ground-plane vector (x east, y = world +Z, south),
/// radians clockwise from north.
pub fn bearing(v: glam::Vec2) -> f32 {
    v.x.atan2(-v.y).rem_euclid(std::f32::consts::TAU)
}

/// An angle wrapped into [-pi, pi).
#[inline]
pub fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// `from` turned toward `to` by fraction `k` of the shorter way round.
#[inline]
pub fn turn_toward(from: f32, to: f32, k: f32) -> f32 {
    from + wrap_angle(to - from) * k
}

pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuild>;
pub type FxHashSet<K> = std::collections::HashSet<K, FxBuild>;

/// Smooth a square grid in place: a box blur of radius `r` cells applied
/// twice (close to a tent filter), edges clamped. Each pass keeps a running
/// sum along rows and columns, so the cost doesn't grow with `r`.
pub fn blur(g: &mut [f32], n: usize, r: i32) {
    if r <= 0 || n == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; n * n];
    for _ in 0..2 {
        box_pass(g, &mut tmp, n, r);
    }
}

fn box_pass(g: &mut [f32], tmp: &mut [f32], n: usize, r: i32) {
    let w = (2 * r + 1) as f32;
    let at = |i: i32| i.clamp(0, n as i32 - 1) as usize;
    // Rows into tmp.
    for z in 0..n {
        let row = &g[z * n..z * n + n];
        let mut s: f32 = (-r..=r).map(|k| row[at(k)]).sum();
        for x in 0..n {
            tmp[z * n + x] = s / w;
            s += row[at(x as i32 + r + 1)] - row[at(x as i32 - r)];
        }
    }
    // Columns back into g.
    for x in 0..n {
        let mut s: f32 = (-r..=r).map(|k| tmp[at(k) * n + x]).sum();
        for z in 0..n {
            g[z * n + x] = s / w;
            s += tmp[at(z as i32 + r + 1) * n + x] - tmp[at(z as i32 - r) * n + x];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One running-sum pass matches a direct average with clamped edges.
    #[test]
    fn blur_matches_direct_average() {
        let n = 9;
        let g: Vec<f32> = (0..n * n).map(|i| ((i * 37) % 11) as f32).collect();
        let r = 2;
        let mut fast = g.clone();
        let mut tmp = vec![0.0; n * n];
        box_pass(&mut fast, &mut tmp, n, r);
        let at = |i: i32| i.clamp(0, n as i32 - 1) as usize;
        for z in 0..n as i32 {
            for x in 0..n as i32 {
                let mut s = 0.0;
                for dz in -r..=r {
                    for dx in -r..=r {
                        s += g[at(z + dz) * n + at(x + dx)];
                    }
                }
                let want = s / ((2 * r + 1) * (2 * r + 1)) as f32;
                assert!((fast[z as usize * n + x as usize] - want).abs() < 1e-3);
            }
        }
    }
}

/// Copy a save file that would not load to `<name>.bad` and say why, so the
/// next save -- which will be written from whatever did load -- replaces the
/// file but not the only copy of what was in it. Returns where the copy went.
pub fn set_aside(path: &std::path::Path, why: impl std::fmt::Display) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bad");
    let bad = path.with_file_name(name);
    match std::fs::copy(path, &bad) {
        Ok(_) => eprintln!("{}: {why}; a copy is kept at {}", path.display(), bad.display()),
        Err(e) => eprintln!("{}: {why}; and it could not be copied aside ({e})", path.display()),
    }
    bad
}

/// For one field of a save file: a value this build cannot read (a style or a
/// game mode whose name has changed) becomes the default rather than failing
/// the whole file and taking every other field with it.
/// `#[serde(deserialize_with = "crate::util::or_default")]`.
pub fn or_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::de::DeserializeOwned,
{
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// Write 8-bit pixels to `path` as a PNG: RGBA when there are four bytes a
/// pixel, RGB when there are three.
pub fn save_png(path: impl AsRef<std::path::Path>, w: u32, h: u32, px: &[u8]) -> std::io::Result<()> {
    let colour = match px.len() / (w as usize * h as usize).max(1) {
        3 => png::ColorType::Rgb,
        _ => png::ColorType::Rgba,
    };
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(colour);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(px)?;
    Ok(())
}

/// An enum whose values are named in the library's JSON, and only those
/// names. A misspelt name fails to parse where it is written, and a value
/// added here is a compile error in every `match` that has not been told
/// what it means -- where a string with a `_ =>` arm ran the default.
///
/// Each variant states its JSON name. The enum gets `NAMES` (the editor's
/// pick lists), `name()` and `parse()`.
macro_rules! named {
    ($(#[$m:meta])* $vis:vis enum $T:ident { $($(#[$vm:meta])* $V:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        $vis enum $T {
            $($(#[$vm])* #[serde(rename = $s)] $V),+
        }

        #[allow(dead_code)]
        impl $T {
            pub const ALL: &'static [$T] = &[$($T::$V),+];
            pub const NAMES: &'static [&'static str] = &[$($s),+];

            pub fn name(self) -> &'static str {
                match self {
                    $($T::$V => $s),+
                }
            }

            pub fn parse(s: &str) -> Option<$T> {
                match s {
                    $($s => Some($T::$V),)+
                    _ => None,
                }
            }
        }

        impl std::fmt::Display for $T {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}
pub(crate) use named;
