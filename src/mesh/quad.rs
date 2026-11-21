//! The packed quad: four u32 words per face, written by the mesher and read
//! by the renderer, the physics ground query and the shaders. The field
//! positions live here and reach the WGSL through the generated prelude
//! (render/shaders.rs), so Rust and the shaders read the same layout.
//!
//! word0: x:5 y:5 z:5 (w-1):5 (h-1):5 dir:3 rounded:1 normal:1 block:1 tube:1
//! word1: material:8 ao:8 (four 2-bit corner values) slot:16 (filled in at upload)
//! word2: corner height offsets:16 (four signed 4-bit quarters of a voxel)
//!        blend material:8 blend weights:8 (four 2-bit quarters)
//! word3: free for flat and sloped quads.
//! A rounded quad instead moves each corner in all three axes: 16 bits per
//! corner (x:5 y:6 z:5, signed sixteenths of a voxel), corner k at bit 16k of
//! the 64-bit number word2 << 32 | word3. Sixteenths rather than eighths so a
//! thin stem's bevel can sit near the equal-sided octagon (mesh.rs BEVEL);
//! y gets the extra bit because a sloped ground corner moves a whole voxel.
//!
//! A rounded quad is never merged, so its size fields are free. A leaf block
//! (block bit) is one face of a 0.5 m cell of leaves: 2x2 voxels, with corner
//! offsets in quarters of a voxel instead of sixteenths. With the normal bit,
//! the size fields hold the normal the face is lit by, smoothed over the
//! crown (octahedral, 5 bits a component: `normal_bits`).

pub type Quad = [u32; 4];

pub const Y_SHIFT: u32 = 5;
pub const Z_SHIFT: u32 = 10;
pub const W_SHIFT: u32 = 15;
pub const H_SHIFT: u32 = 20;
pub const DIR_SHIFT: u32 = 25;
pub const ROUND_BIT: u32 = 28;
pub const NORMAL_BIT: u32 = 29;
pub const BLOCK_BIT: u32 = 30;
/// A rounded quad that is a face of a tube (mesh/lean.rs): it casts its
/// shadow from its real corners, not from the square face it hangs from.
pub const TUBE_BIT: u32 = 31;
pub const AO_SHIFT: u32 = 8;
pub const SLOT_SHIFT: u32 = 16;
pub const BLEND_SHIFT: u32 = 16;

/// word0 from a quad's position within its chunk, size, direction and kind.
#[inline]
pub fn word0(pos: [u32; 3], w: u32, h: u32, dir: usize, rounded: bool) -> u32 {
    pos[0] | pos[1] << Y_SHIFT | pos[2] << Z_SHIFT | (w - 1) << W_SHIFT | (h - 1) << H_SHIFT | (dir as u32) << DIR_SHIFT | (rounded as u32) << ROUND_BIT
}

#[inline]
pub fn pos(q: &Quad) -> [u32; 3] {
    [q[0] & 31, q[0] >> Y_SHIFT & 31, q[0] >> Z_SHIFT & 31]
}

/// Width and height in voxels, along the direction's u and v axes.
#[inline]
pub fn size(q: &Quad) -> (u32, u32) {
    if rounded(q) {
        return if block(q) { (2, 2) } else { (1, 1) };
    }
    ((q[0] >> W_SHIFT & 31) + 1, (q[0] >> H_SHIFT & 31) + 1)
}

/// Face direction: 0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z.
#[inline]
pub fn dir(q: &Quad) -> usize {
    (q[0] >> DIR_SHIFT & 7) as usize
}

#[inline]
pub fn rounded(q: &Quad) -> bool {
    q[0] >> ROUND_BIT & 1 == 1
}

/// A leaf block's face (see the module notes): rounded, 2x2, offsets doubled.
#[inline]
pub fn block(q: &Quad) -> bool {
    q[0] >> BLOCK_BIT & 1 == 1
}

/// Corner offsets of a rounded quad are in sixteenths of a voxel times this
/// (a leaf block's are in quarters).
#[inline]
pub fn round_scale(q: &Quad) -> f32 {
    if block(q) {
        4.0
    } else {
        1.0
    }
}

/// word0 bits for a rounded quad lit by unit normal `n` instead of its facets:
/// octahedral, 5 bits a component, in the size fields, plus the normal bit.
pub fn normal_bits(n: [f32; 3]) -> u32 {
    let s = n[0].abs() + n[1].abs() + n[2].abs();
    let (mut x, mut z) = (n[0] / s, n[2] / s);
    if n[1] < 0.0 {
        (x, z) = ((1.0 - z.abs()) * x.signum(), (1.0 - x.abs()) * z.signum());
    }
    let q = |v: f32| ((v * 0.5 + 0.5) * 31.0).round().clamp(0.0, 31.0) as u32;
    q(x) << W_SHIFT | q(z) << H_SHIFT | 1 << NORMAL_BIT
}

/// The 16-bit sloped-corner offsets of a flat or sloped quad.
#[inline]
pub fn slope_bits(q: &Quad) -> u32 {
    q[2] & 0xffff
}

/// The 64-bit rounded-corner offsets of a rounded quad.
#[inline]
pub fn round_bits(q: &Quad) -> u64 {
    (q[2] as u64) << 32 | q[3] as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_sizes_hold_the_normal() {
        let n = [0.36f32, 0.8, -0.48];
        let q = [word0([3, 4, 5], 1, 1, 2, true) | normal_bits(n) | 1 << BLOCK_BIT, 0, 0, 0];
        assert_eq!(size(&q), (2, 2));
        assert_eq!(pos(&q), [3, 4, 5]);
        assert_eq!(dir(&q), 2);
        assert!(block(&q) && rounded(&q));
        // Decode as the shader does.
        let o = [(q[0] >> W_SHIFT & 31) as f32 / 31.0 * 2.0 - 1.0, (q[0] >> H_SHIFT & 31) as f32 / 31.0 * 2.0 - 1.0];
        let mut d = [o[0], 1.0 - o[0].abs() - o[1].abs(), o[1]];
        if d[1] < 0.0 {
            d = [(1.0 - o[1].abs()) * o[0].signum(), d[1], (1.0 - o[0].abs()) * o[1].signum()];
        }
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let dot = (d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) / l;
        assert!(dot > 0.99, "decoded {d:?} from {n:?}");
    }

    #[test]
    fn word0_round_trips() {
        let q = [word0([3, 31, 17], 32, 5, 4, false), 0, 0, 0];
        assert_eq!(pos(&q), [3, 31, 17]);
        assert_eq!(size(&q), (32, 5));
        assert_eq!(dir(&q), 4);
        assert!(!rounded(&q));
        // A rounded quad is always 1x1 (its size fields hold other things).
        let r = [word0([3, 31, 17], 1, 1, 4, true), 0, 0, 0];
        assert_eq!(size(&r), (1, 1));
        assert!(rounded(&r) && !block(&r));
    }
}
