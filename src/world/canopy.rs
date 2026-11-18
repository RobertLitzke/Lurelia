//! The shapes a crown is built from, and the rules that build one over a set
//! of limbs.
//!
//! Until 2026-11-10 every leaf clump was a sphere, so a branching tree was a
//! tiny ball on the end of each twig under a shell of more balls: fragmented
//! up close, one smooth mushroom cap from a hundred metres. A crown is now
//! made of four shapes — a ball, a filled dome, a flat plate and a sleeve of
//! leaves along a limb — and is assembled as lobes over the big limbs, with
//! partial infill between them, so it reads as a lobed canopy with notches
//! instead of a dome.
//!
//! Every shape measures itself in one normalised distance: 0 at its middle, 1
//! at its nominal surface. `gen.rs` draws the band round 1 ragged and dapples
//! the whole thing with broad lumps, so one rasteriser serves all four.

use glam::Vec3;

/// One clump of foliage: a shape of radius `r` at `c`.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub c: Vec3,
    pub r: f32,
    pub shape: Shape,
    /// Material, or 0 to take the tree's leaf (flower heads over green leaves).
    pub mat: u8,
}

/// What a clump of foliage is shaped like.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    /// A round clump: the only shape there used to be.
    Ball,
    /// A filled dome, `rise` as tall as it is wide above its middle and
    /// half that below: a crown lobe, flatter underneath than over.
    Dome { rise: f32 },
    /// A flat plate, `thick` as deep as it is wide through `axis`: the infill
    /// between two lobes, and an elder's flower heads.
    Plate { axis: Vec3, thick: f32 },
    /// A sleeve of leaves along a limb, from `c` to `c + span`: foliage
    /// clothing a twig instead of a ball stuck on its end.
    Sleeve { span: Vec3 },
}

impl Blob {
    pub fn ball(c: Vec3, r: f32) -> Blob {
        Blob { c, r, shape: Shape::Ball, mat: 0 }
    }
    pub fn dome(c: Vec3, r: f32, rise: f32) -> Blob {
        Blob {
            c,
            r,
            shape: Shape::Dome { rise },
            mat: 0,
        }
    }
    pub fn plate(c: Vec3, r: f32, axis: Vec3, thick: f32) -> Blob {
        let axis = axis.normalize_or_zero();
        let axis = if axis == Vec3::ZERO { Vec3::Y } else { axis };
        Blob {
            c,
            r,
            shape: Shape::Plate { axis, thick },
            mat: 0,
        }
    }
    pub fn sleeve(c: Vec3, r: f32, span: Vec3) -> Blob {
        Blob {
            c,
            r,
            shape: Shape::Sleeve { span },
            mat: 0,
        }
    }
    pub fn of(self, mat: u8) -> Blob {
        Blob { mat, ..self }
    }

    /// The box the shape reaches, as offsets from `c`, with `squash` on the
    /// vertical and `outer` the ragged edge's overshoot.
    pub fn bounds(&self, squash: f32, outer: f32) -> (Vec3, Vec3) {
        let e = self.r * outer;
        match self.shape {
            Shape::Ball => (Vec3::new(-e, -e * squash, -e), Vec3::new(e, e * squash, e)),
            Shape::Dome { rise } => {
                let up = e * rise * squash;
                (Vec3::new(-e, -up * BELOW, -e), Vec3::new(e, up, e))
            }
            // A plate on any axis still fits its own sphere.
            Shape::Plate { .. } => (Vec3::new(-e, -e * squash, -e), Vec3::new(e, e * squash, e)),
            Shape::Sleeve { span } => {
                let pad = Vec3::new(e, e * squash, e);
                (span.min(Vec3::ZERO) - pad, span.max(Vec3::ZERO) + pad)
            }
        }
    }

    /// Distance of `p` from the middle, 1 at the nominal surface. Above about
    /// 4 the point is nowhere near and the caller can stop.
    pub fn dist(&self, p: Vec3, squash: f32) -> f32 {
        let mut q = p - self.c;
        q.y /= squash.max(0.05);
        let r = self.r.max(1e-3);
        match self.shape {
            Shape::Ball => q.length() / r,
            Shape::Dome { rise } => {
                let ry = (r * rise).max(1e-3);
                // Domed over, flatter underneath: below the middle the same
                // height counts for about twice as much.
                let y = if q.y >= 0.0 { q.y / ry } else { q.y / (ry * BELOW) };
                ((q.x * q.x + q.z * q.z) / (r * r) + y * y).sqrt()
            }
            Shape::Plate { axis, thick } => {
                let a = q.dot(axis);
                let across = (q - axis * a).length();
                let t = (r * thick).max(1e-3);
                ((a / t).powi(2) + (across / r).powi(2)).sqrt()
            }
            Shape::Sleeve { mut span } => {
                span.y /= squash.max(0.05);
                let l2 = span.length_squared().max(1e-6);
                let t = (q.dot(span) / l2).clamp(0.0, 1.0);
                (q - span * t).length() / r
            }
        }
    }

    /// How far the clump reaches out from `base` on the flat, and how high.
    pub fn reach(&self, base: Vec3, squash: f32, outer: f32) -> (f32, f32) {
        let (lo, hi) = self.bounds(squash, outer);
        let far = |x: f32, z: f32| Vec3::new(x, 0.0, z).length();
        let c = self.c - base;
        let out = far(c.x + hi.x, c.z + hi.z)
            .max(far(c.x + lo.x, c.z + lo.z))
            .max(far(c.x + hi.x, c.z + lo.z))
            .max(far(c.x + lo.x, c.z + hi.z));
        (out, c.y + hi.y)
    }
}

/// How far a dome reaches below its middle, as a share of its rise.
const BELOW: f32 = 0.52;

/// Broad lumps on a clump's surface: its radius swells and shrinks by a few
/// tenths in a handful of directions fixed per clump, so a crown keeps lobes
/// and notches at any distance, after the leaf sprites have faded out. Two dot
/// products on a direction we already have — no extra noise lookup, which
/// would cost a lookup on every voxel inside the clump as well as the skin.
pub struct Lumps {
    a: Vec3,
    b: Vec3,
    amp: f32,
}

impl Lumps {
    /// `seed` picks the directions, `amp` how deep the notches cut (about a
    /// fifth of the radius at 0.2).
    pub fn new(seed: u32, amp: f32) -> Lumps {
        let unit = super::noise::unit;
        let dir = |s: u32| {
            let z = unit(s) * 2.0 - 1.0;
            let a = unit(super::noise::hash32(s)) * std::f32::consts::TAU;
            let r = (1.0 - z * z).max(0.0).sqrt();
            Vec3::new(r * a.cos(), z, r * a.sin())
        };
        let s2 = super::noise::hash32(seed ^ 0x9e37_79b9);
        Lumps { a: dir(seed), b: dir(s2), amp }
    }

    /// The surface's distance in the direction of `q` (whose length is
    /// `d * r`), as a multiple of the nominal 1.
    pub fn at(&self, q: Vec3, len: f32) -> f32 {
        if len < 1e-4 {
            return 1.0;
        }
        let n = q / len;
        1.0 + self.amp * n.dot(self.a) * n.dot(self.b) * 2.0
    }
}
