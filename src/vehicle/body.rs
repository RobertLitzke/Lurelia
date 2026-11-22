//! The rigid body: integration, and the chassis kept out of solid voxels.
//!
//! The chassis is a handful of spheres (`Spec::collide`). Each one that is
//! inside rock is pushed out along the way out, and an impulse at the
//! contact takes away the speed into the rock, with friction along it. The
//! impulse acts at the contact, not the centre, so a clipped corner turns the
//! car instead of stopping it dead.

use crate::world::store::{Cell, Store};
use glam::{Quat, Vec3};

/// Position, orientation and their rates. `rot` takes the body frame to
/// the world; `ang` is the angular velocity in the world frame.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub ang: Vec3,
    pub mass: f32,
    /// Diagonal inertia in the body frame.
    pub inertia: Vec3,
}

impl Body {
    /// The inverse inertia applied to a world-frame vector.
    pub fn inv_i(&self, v: Vec3) -> Vec3 {
        let b = self.rot.inverse() * v;
        self.rot * (b / self.inertia)
    }

    /// Velocity of the body point at world position `p`.
    pub fn point_vel(&self, p: Vec3) -> Vec3 {
        self.vel + self.ang.cross(p - self.pos)
    }

    /// Semi-implicit Euler over `h` with force `f` and torque `t` (world).
    pub fn integrate(&mut self, f: Vec3, t: Vec3, h: f32) {
        self.vel += f / self.mass * h;
        // Torque in the body frame, with the gyroscopic term.
        let tb = self.rot.inverse() * t;
        let wb = self.rot.inverse() * self.ang;
        let iw = wb * self.inertia;
        let wb = wb + (tb - wb.cross(iw)) / self.inertia * h;
        self.ang = self.rot * wb;
        // A runaway spin is a bug somewhere, not a car: cap it.
        self.ang = self.ang.clamp_length_max(25.0);
        self.vel = self.vel.clamp_length_max(80.0);
        self.pos += self.vel * h;
        let w = self.ang;
        let dq = Quat::from_xyzw(w.x, w.y, w.z, 0.0) * self.rot;
        self.rot = Quat::from_xyzw(self.rot.x + 0.5 * h * dq.x, self.rot.y + 0.5 * h * dq.y, self.rot.z + 0.5 * h * dq.z, self.rot.w + 0.5 * h * dq.w).normalize();
    }

    /// An impulse `j` at world point `p`.
    pub fn impulse(&mut self, p: Vec3, j: Vec3) {
        self.vel += j / self.mass;
        self.ang += self.inv_i((p - self.pos).cross(j));
    }

    /// Push the chassis spheres out of solid voxels. Returns the hardest
    /// speed into the rock among them, m/s (0 for none).
    pub fn collide(&mut self, store: &Store, spheres: &[[f32; 4]], slopes: bool) -> f32 {
        let mut hardest = 0.0f32;
        for s in spheres {
            let c = self.pos + self.rot * Vec3::new(s[0], s[1], s[2]);
            let Some((n, pen)) = push_out(store, c, s[3], slopes) else { continue };
            self.pos += n * pen * 0.8;
            let p = c - n * s[3];
            let r = p - self.pos;
            let v = self.point_vel(p);
            let vn = v.dot(n);
            if vn >= 0.0 {
                continue;
            }
            hardest = hardest.max(-vn);
            let k = 1.0 / self.mass + n.dot(self.inv_i(r.cross(n)).cross(r));
            let e = if -vn > 3.0 { 0.15 } else { 0.0 };
            let jn = -(1.0 + e) * vn / k;
            self.impulse(p, n * jn);
            // Friction along the rock.
            let v = self.point_vel(p);
            let vt = v - n * v.dot(n);
            let len = vt.length();
            if len > 1e-4 {
                let t = vt / len;
                let kt = 1.0 / self.mass + t.dot(self.inv_i(r.cross(t)).cross(r));
                let jt = (len / kt).min(0.45 * jn);
                self.impulse(p, -t * jt);
            }
        }
        hardest
    }
}

/// The way out of solid voxels for a sphere at `c` of radius `r`, and how
/// deep it is in: the push from every voxel it overlaps, summed, so a sphere
/// on a flat floor of many voxels is pushed straight up. With `slopes`, the
/// ground is the drawn slope the wheels roll on: the open tops of the voxel
/// steps under it, which stand up to a voxel proud of it, are not met.
pub fn push_out(store: &Store, c: Vec3, r: f32, slopes: bool) -> Option<(Vec3, f32)> {
    let lo = ((c - Vec3::splat(r)) * 4.0).floor();
    let hi = ((c + Vec3::splat(r)) * 4.0).floor();
    let mut sum = Vec3::ZERO;
    let mut deepest = 0.0f32;
    for y in lo.y as i32..=hi.y as i32 {
        for z in lo.z as i32..=hi.z as i32 {
            for x in lo.x as i32..=hi.x as i32 {
                if !matches!(store.cell(x, y, z), Cell::Solid | Cell::Water) {
                    continue;
                }
                let bmin = Vec3::new(x as f32, y as f32, z as f32) * 0.25;
                let q = c.clamp(bmin, bmin + Vec3::splat(0.25));
                let d = c - q;
                let dist = d.length();
                if dist >= r {
                    continue;
                }
                // The centre inside the voxel: out through the top, which is
                // nearly always the way out of ground.
                let mut n = if dist > 1e-4 { d / dist } else { Vec3::Y };
                // An open voxel top below the centre is a step in the ground,
                // which the drawn slope smooths over: ride up it rather than
                // meet its edge as a wall, which kicks the car over.
                let top = bmin.y + 0.25;
                if c.y >= top && !matches!(store.cell(x, y + 1, z), Cell::Solid | Cell::Water) {
                    if slopes {
                        continue;
                    }
                    n = Vec3::Y;
                }
                let pen = r - dist;
                sum += n * pen;
                deepest = deepest.max(pen);
            }
        }
    }
    if slopes {
        // The drawn ground under the sphere, met as a floor.
        if let Some((h, _)) = super::wheel::height(store, c.x, c.z, c.y + 0.25, r + 0.75, true) {
            let pen = h - (c.y - r);
            if pen > 0.0 {
                sum += Vec3::Y * pen;
                deepest = deepest.max(pen);
            }
        }
    }
    if deepest <= 0.0 {
        return None;
    }
    Some((sum.normalize_or(Vec3::Y), deepest))
}
