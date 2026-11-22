//! Soft bodies: flesh that settles under its own weight and lags the frame.
//!
//! A walrus is three metres of blubber, and drawn as rigid voxel parts it
//! looked like a potato. A bone with `"flesh"` above 0 in its rig is soft:
//! every vertex of it is moved by one field over the whole body, computed in
//! the entity vertex shader (`flesh_field` in common.wgsl, mirrored by
//! `field` here). The field does two things:
//!
//! - Sag: the top settles toward the ground and what it loses bulges out low
//!   on the flanks, more the further the animal is lying down (`sag`).
//! - Jiggle: a damped spring per animal, driven by how its body accelerates
//!   (its travel and the bob of its own pose), shifts the upper flesh against
//!   the frame. It is kept small: the point is that the body looks heavy, not
//!   that it wobbles.
//!
//! One field over the whole body is what keeps the parts joined: two parts
//! that met before the field meet after it. A part softer or stiffer than
//! its parent takes its parent's movement at the joint and its own past it
//! (`flesh_offset` in common.wgsl), so a rigid flipper rides on a sagging
//! flank instead of sinking into it.

use super::{bone_rot, Pose, Rig};
use glam::{Quat, Vec3};

/// How much flatter the top of a fully sagged body is, as a share of its
/// height. Must match the 0.34 in common.wgsl `flesh_field`.
#[cfg_attr(not(test), allow(dead_code))]
pub const SQUASH: f32 = 0.34;

/// The soft part of a rig, in model voxels. Built by the slicer from the
/// bones' `flesh` values and the voxels those bones own.
#[derive(Clone, Debug, PartialEq)]
pub struct Soft {
    /// Per bone, 0 rigid to 1 all flesh.
    pub weight: Vec<f32>,
    /// The field's origin: the bottom centre of the soft bones' voxels.
    pub origin: Vec3,
    /// Height of the soft voxels above the origin.
    pub height: f32,
    /// Half their length front to back.
    pub half_len: f32,
}

impl Soft {
    /// From the bones' flesh values and, per bone, the voxels it owns as
    /// (min, max) corners; None when no bone is soft.
    pub fn new(weight: Vec<f32>, extents: &[Option<(Vec3, Vec3)>]) -> Option<Soft> {
        if weight.iter().all(|w| *w <= 0.0) {
            return None;
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for (w, e) in weight.iter().zip(extents) {
            if let (true, Some((a, b))) = (*w > 0.0, e) {
                lo = lo.min(*a);
                hi = hi.max(*b);
            }
        }
        if lo.x > hi.x {
            return None;
        }
        Some(Soft {
            weight,
            origin: Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5),
            height: (hi.y - lo.y).max(1.0),
            half_len: ((hi.z - lo.z) * 0.5).max(1.0),
        })
    }

    /// The flesh rows of one part's instance (models.rs `Instance::flesh`).
    /// `fk` is the forward kinematics (`anim::forward`) at `voxel` metres a
    /// voxel; `bone` is the part's bone.
    pub fn instance(&self, rig: &Rig, pose: &Pose, fk: &[(Vec3, Quat)], voxel: f32, bone: usize) -> [[f32; 4]; 3] {
        let (root_p, root_q) = fk[0];
        // The origin rides on the root bone.
        let origin = root_p + root_q * ((self.origin - rig.bones[0].joint) * voxel);
        let joint = fk[bone].0 - origin;
        let w = self.weight.get(bone).copied().unwrap_or(0.0);
        let wp = rig.bones[bone].parent.map_or(w, |p| self.weight.get(p).copied().unwrap_or(0.0));
        // The ground is where the model's floor is (the pivot is its bottom
        // centre); a body raised off it settles onto its own underside.
        let floor = (-origin.y).max(0.0);
        let [jx, jy, jz] = pose.jiggle.to_array();
        [
            [joint.x, joint.y, joint.z, w],
            [jx, jy, jz, wp],
            [pose.sag, floor, self.height * voxel, self.half_len * voxel],
        ]
    }
}

/// How far the field moves a point `p` metres from the origin (creature
/// frame): the Rust twin of common.wgsl `flesh_field`, for the tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn field(p: Vec3, jiggle: Vec3, sag: f32, floor: f32, height: f32, half_len: f32) -> Vec3 {
    let up = (p.y - floor).max(0.0);
    let h = (up / height.max(0.001)).min(1.0);
    let s = sag * SQUASH;
    let low = 1.0 - h;
    let t = (up / 0.08).clamp(0.0, 1.0);
    let spread = s * (1.6 * low * low + 0.25) * (t * t * (3.0 - 2.0 * t));
    let along = (p.z / half_len.max(0.001)).clamp(-1.0, 1.0);
    Vec3::new(p.x * spread, -s * up * h, p.z * spread * 0.2) + jiggle * h * h * (1.0 + 0.4 * along)
}

/// How far a soft body sags at posture `level` (0 stand, 1 sit, 2 lie): a
/// standing walrus still carries its weight on its belly.
pub fn sag(level: f32) -> f32 {
    0.6 + 0.4 * (level / 2.0).clamp(0.0, 1.0)
}

/// Natural frequency of the flesh, Hz, and its damping ratio: slow and soft,
/// settling in a couple of wobbles.
const HZ: f32 = 2.6;
const DAMPING: f32 = 0.3;
/// How hard the flesh is thrown, as a share of the acceleration that throws
/// it, and how far it may go, metres per metre of body height.
const GAIN: f32 = 0.5;
const REACH: f32 = 0.05;
/// Accelerations past this (a step up a ledge, a teleport) count as this.
const MAX_ACCEL: f32 = 25.0;

/// The jiggle spring for one animal.
#[derive(Clone, Debug, Default)]
pub struct Jiggle {
    /// Last step's driving point (creature frame, from its own pose) and
    /// position in the world, and the velocity each had.
    last: Option<(Vec3, Vec3)>,
    vel: (Vec3, Vec3),
    x: Vec3,
    v: Vec3,
}

impl Jiggle {
    /// Advance one step. `pos` and `yaw` place the animal in the world,
    /// `pose` is the pose just composed, `height` the soft body's height in
    /// metres. Returns the flesh offset, creature frame, metres.
    pub fn step(&mut self, pose: &Pose, pos: Vec3, yaw: f32, height: f32, dt: f32) -> Vec3 {
        if dt <= 0.0 {
            return self.x;
        }
        // The upper flesh hangs off the body's frame: where the frame puts a
        // point half the body up, and where the world puts the animal.
        let q0 = pose.bones.first().map_or(Quat::IDENTITY, |e| bone_rot(*e));
        let local = pose.root + q0 * Vec3::Y * (height * 0.5);
        let mut accel = Vec3::ZERO;
        if let Some((lp, wp)) = self.last {
            let vl = (local - lp) / dt;
            let vw = (pos - wp) / dt;
            let aw = Quat::from_rotation_y(-yaw) * ((vw - self.vel.1) / dt);
            accel = ((vl - self.vel.0) / dt + aw).clamp_length_max(MAX_ACCEL);
            self.vel = (vl, vw);
        }
        self.last = Some((local, pos));
        let w = std::f32::consts::TAU * HZ;
        // Semi-implicit Euler: stable at 60 Hz for a spring this slow.
        let a = -w * w * self.x - 2.0 * DAMPING * w * self.v - accel * GAIN;
        self.v += a * dt;
        self.x += self.v * dt;
        let reach = REACH * height;
        if self.x.length() > reach {
            self.x = self.x.clamp_length_max(reach);
            self.v *= 0.5;
        }
        self.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lying down flattens the top and spreads the base; the belly on the
    /// ground stays where it is.
    #[test]
    fn a_sagging_body_settles_and_spreads() {
        let (h, len) = (1.0, 1.5);
        let top = field(Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, 1.0, 0.0, h, len);
        assert!(top.y < -0.2, "the top should drop, got {top:?}");
        let flank = field(Vec3::new(0.5, 0.2, 0.0), Vec3::ZERO, 1.0, 0.0, h, len);
        assert!(flank.x > 0.05, "the low flank should bulge out, got {flank:?}");
        let belly = field(Vec3::new(0.2, 0.0, 0.3), Vec3::ZERO, 1.0, 0.0, h, len);
        assert!(belly.length() < 1e-4, "the belly on the ground should stay, got {belly:?}");
        let rigid = field(Vec3::new(0.5, 0.7, 0.4), Vec3::ZERO, 0.0, 0.0, h, len);
        assert!(rigid.length() < 1e-6);
    }

    /// The field is continuous: neighbouring points move almost alike, so
    /// faces do not tear apart.
    #[test]
    fn the_field_does_not_tear() {
        let j = Vec3::new(0.02, -0.03, 0.01);
        for i in 0..200 {
            let p = Vec3::new((i % 7) as f32 * 0.17 - 0.5, (i % 11) as f32 * 0.11 - 0.1, (i % 13) as f32 * 0.23 - 1.4);
            let e = Vec3::splat(1e-3);
            let a = field(p, j, 1.0, 0.0, 1.0, 1.5);
            let b = field(p + e, j, 1.0, 0.0, 1.0, 1.5);
            assert!((a - b).length() < 0.01, "a jump at {p:?}");
        }
    }

    /// A bump sets the flesh wobbling, it stays within its reach, and it
    /// comes to rest.
    #[test]
    fn the_jiggle_rings_down() {
        let mut j = Jiggle::default();
        let pose = Pose::rest(1);
        let mut peak: f32 = 0.0;
        let mut pos = Vec3::ZERO;
        for i in 0..600 {
            // Moving at 1 m/s, stopping dead after a second.
            if i < 60 {
                pos.z -= 1.0 / 60.0;
            }
            let x = j.step(&pose, pos, 0.0, 1.0, 1.0 / 60.0);
            if i > 60 {
                peak = peak.max(x.length());
            }
            assert!(x.length() <= REACH + 1e-5);
        }
        assert!(peak > 0.005, "stopping should throw the flesh, peak {peak}");
        assert!(j.x.length() < 1e-3, "it should settle, at {:?}", j.x);
    }
}
