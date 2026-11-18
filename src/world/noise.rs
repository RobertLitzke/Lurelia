//! Hashing and gradient noise. Everything is a pure function of its inputs and a
//! seed, so world generation is reproducible on any thread.

#[inline]
pub fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Lattice hash with the seed already mixed (see `seed_mix`), so noise functions
/// hash the seed once per call rather than once per corner.
#[inline]
fn lattice2(x: i32, y: i32, sm: u32) -> u32 {
    hash32((x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ sm)
}

#[inline]
fn lattice3(x: i32, y: i32, z: i32, sm: u32) -> u32 {
    hash32((x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (z as u32).wrapping_mul(0xcb1a_b31f) ^ sm)
}

#[inline]
fn seed_mix(seed: u32) -> u32 {
    hash32(seed.wrapping_add(0x9e37_79b9))
}

#[inline]
pub fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    hash32((x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ hash32(seed.wrapping_add(0x9e37_79b9)))
}

#[inline]
pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    hash32((x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (z as u32).wrapping_mul(0xcb1a_b31f) ^ hash32(seed.wrapping_add(0x9e37_79b9)))
}

/// Uniform float in [0, 1) from a hash.
#[inline]
pub fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// A seeded random stream: each draw adds a fixed odd step to the state and
/// hashes it. Creatures, flocks, schools and individual variation draw from
/// these. Two steps are in use; `walker` keeps the walkers' and the activity
/// scheduler's sequences as they were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng {
    pub state: u32,
    step: u32,
}

impl Rng {
    pub const fn new(state: u32) -> Self {
        Rng { state, step: 0x9e37_79b9 }
    }
    pub const fn walker(state: u32) -> Self {
        Rng { state, step: 0x6d2b_79f5 }
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.state = hash32(self.state.wrapping_add(self.step));
        unit(self.state)
    }
    /// Uniform in [-1, 1).
    #[inline]
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// The diagonals are written out as 0.707_107 rather than FRAC_1_SQRT_2 so the
// table reads as eight gradients of the same shape; clippy flags each one.
#[allow(clippy::approx_constant)]
const G2: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.707_107, 0.707_107),
    (-0.707_107, 0.707_107),
    (0.707_107, -0.707_107),
    (-0.707_107, -0.707_107),
];

/// 2D gradient noise, roughly in [-1, 1].
pub fn perlin2(x: f32, y: f32, seed: u32) -> f32 {
    let xf0 = x.floor();
    let yf0 = y.floor();
    let ix = xf0 as i32;
    let iy = yf0 as i32;
    let fx = x - xf0;
    let fy = y - yf0;
    let sm = seed_mix(seed);
    let g = |i: i32, j: i32, dx: f32, dy: f32| {
        let (gx, gy) = G2[(lattice2(ix + i, iy + j, sm) & 7) as usize];
        gx * dx + gy * dy
    };
    let u = fade(fx);
    let v = fade(fy);
    let a = lerp(g(0, 0, fx, fy), g(1, 0, fx - 1.0, fy), u);
    let b = lerp(g(0, 1, fx, fy - 1.0), g(1, 1, fx - 1.0, fy - 1.0), u);
    lerp(a, b, v) * 1.414
}

const G3: [(f32, f32, f32); 16] = [
    (1.0, 1.0, 0.0),
    (-1.0, 1.0, 0.0),
    (1.0, -1.0, 0.0),
    (-1.0, -1.0, 0.0),
    (1.0, 0.0, 1.0),
    (-1.0, 0.0, 1.0),
    (1.0, 0.0, -1.0),
    (-1.0, 0.0, -1.0),
    (0.0, 1.0, 1.0),
    (0.0, -1.0, 1.0),
    (0.0, 1.0, -1.0),
    (0.0, -1.0, -1.0),
    (1.0, 1.0, 0.0),
    (-1.0, 1.0, 0.0),
    (0.0, -1.0, 1.0),
    (0.0, -1.0, -1.0),
];

/// 3D gradient noise, roughly in [-1, 1].
pub fn perlin3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let (fx, fy, fz) = (x - x0, y - y0, z - z0);
    let sm = seed_mix(seed);
    let g = |i: i32, j: i32, k: i32| {
        let (gx, gy, gz) = G3[(lattice3(ix + i, iy + j, iz + k, sm) & 15) as usize];
        gx * (fx - i as f32) + gy * (fy - j as f32) + gz * (fz - k as f32)
    };
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let x00 = lerp(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = lerp(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = lerp(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = lerp(g(0, 1, 1), g(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Fractal sum of 2D noise, normalised to roughly [-1, 1].
pub fn fbm2(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let (mut fx, mut fy) = (x, y);
    for o in 0..octaves {
        sum += perlin2(fx, fy, seed.wrapping_add(o * 1013)) * amp;
        norm += amp;
        amp *= 0.5;
        // Rotate a little between octaves so grid artefacts don't line up.
        let (rx, ry) = (fx * 1.6 - fy * 1.2, fx * 1.2 + fy * 1.6);
        fx = rx + 17.3;
        fy = ry - 9.1;
    }
    sum / norm
}

/// Ridged multifractal in [0, 1]: sharp crests where the noise crosses zero.
pub fn ridged2(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut weight = 1.0f32;
    let (mut fx, mut fy) = (x, y);
    for o in 0..octaves {
        let n = 1.0 - perlin2(fx, fy, seed.wrapping_add(o * 7919)).abs();
        let n = n * n * weight;
        weight = n.clamp(0.0, 1.0);
        sum += n * amp;
        norm += amp;
        amp *= 0.5;
        let (rx, ry) = (fx * 1.6 - fy * 1.2, fx * 1.2 + fy * 1.6);
        fx = rx + 31.7;
        fy = ry + 5.3;
    }
    sum / norm
}

#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_deterministic() {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for i in 0..20_000 {
            let x = i as f32 * 0.137;
            let y = i as f32 * 0.071 - 40.0;
            let a = perlin2(x, y, 7);
            assert_eq!(a, perlin2(x, y, 7));
            let b = perlin3(x, y, x * 0.3, 7);
            lo = lo.min(a).min(b);
            hi = hi.max(a).max(b);
        }
        assert!(lo > -1.6 && hi < 1.6, "range {lo}..{hi}");
        assert!(lo < -0.5 && hi > 0.5, "range too narrow {lo}..{hi}");
    }
}
