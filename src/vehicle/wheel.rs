//! A wheel: the ground under it and the spring and damper it sits on.
//!
//! The ground comes from the height field the terrain is drawn from, not
//! from marching a ray through voxels, so the tyre rolls on what you see.
//! Three samples along the rolling direction, each lowered by the tyre's own
//! circle, let a wheel climb a 0.25 m voxel step over its radius instead of
//! meeting it as a wall.

use crate::world::material::AIR;
use crate::world::store::{Cell, VoxelRead};
use crate::world::surface::Surface;
use glam::Vec3;

/// The ground a wheel is over, as a plane: refreshed once a tick, used by
/// each substep.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    /// A point on the plane, under the hub.
    pub point: Vec3,
    pub n: Vec3,
    pub mat: u8,
    pub surface: Surface,
    /// Water over the ground here, metres.
    pub depth: f32,
}

/// The drawn ground under (x, z), searching down from `y` for `reach`
/// metres; water voxels count as ground, as they do for every ride.
pub fn height(store: &impl VoxelRead, x: f32, z: f32, y: f32, reach: f32, slopes: bool) -> Option<(f32, u8)> {
    height_under(store, x, z, y, reach, slopes, false)
}

/// `height`, or with `through_water` the bed under any water voxels: a kart
/// made to wade sinks through a ford to the road under it, and feels the
/// water's depth (`probe`'s `level`) instead of driving on its top.
pub fn height_under(store: &impl VoxelRead, x: f32, z: f32, y: f32, reach: f32, slopes: bool, through_water: bool) -> Option<(f32, u8)> {
    let (vx, vz) = ((x * 4.0).floor() as i32, (z * 4.0).floor() as i32);
    let top = (y * 4.0).floor() as i32;
    let bottom = ((y - reach) * 4.0).floor() as i32;
    for vy in (bottom.max(0)..=top).rev() {
        let cell = store.cell(vx, vy, vz);
        if cell == Cell::Solid || (cell == Cell::Water && !through_water) {
            let t = vy + 1;
            let mut h = t as f32 / 4.0;
            if slopes {
                if let Some(d) = store.ground_height(x * 4.0, z * 4.0, t as f32) {
                    h = (d / 4.0).clamp(h - 0.3, h + 0.3);
                }
            }
            return Some((h, store.get(vx, vy, vz).unwrap_or(AIR)));
        }
    }
    None
}

/// The ground plane under a wheel at `hub` rolling along `fwd`, looked for
/// from `from` metres down to `reach` below it. `level` is the water's
/// surface. Ground above `limit` (a height) is an obstacle for the chassis,
/// not ground under the tyre: a small wheel meets a voxel step as a kerb.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn probe(store: &impl VoxelRead, hub: Vec3, fwd: Vec3, r: f32, from: f32, reach: f32, level: f32, slopes: bool, limit: Option<f32>, through_water: bool) -> Option<Contact> {
    let f = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
    // A column that is still solid where the search starts is a wall or a
    // trunk beside the tyre, not ground under it: the chassis meets it.
    let col = |x: f32, z: f32| {
        let inside = matches!(store.cell((x * 4.0).floor() as i32, (from * 4.0).floor() as i32, (z * 4.0).floor() as i32), Cell::Solid);
        if inside {
            None
        } else {
            height_under(store, x, z, from, reach, slopes, through_water).filter(|c| limit.is_none_or(|l| c.0 <= l))
        }
    };
    // The highest the ground may rise round the hub and still be rolled
    // onto: anything taller (a shrub's stem, a ledge) is an obstacle, and
    // read as ground it would tilt the plane toward the car and turn the
    // car's own speed into a slam on that spring.
    let step = (0.75 * r).max(0.3);
    // The hub's own ground, outvoted by the voxels round it, so a stem one
    // voxel wide under the hub does not lift the wheel onto it.
    let v = 0.25;
    let mut round: Vec<(f32, u8)> = [(0.0, 0.0), (v, 0.0), (-v, 0.0), (0.0, v), (0.0, -v)].iter().filter_map(|&(dx, dz)| col(hub.x + dx, hub.z + dz)).collect();
    if round.is_empty() {
        return None;
    }
    round.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mid = round[(round.len() - 1) / 2];
    let (h0, mat) = match col(hub.x, hub.z) {
        Some(c) if c.0 <= mid.0 + step => c,
        _ => mid,
    };
    let h = |x: f32, z: f32| col(x, z).filter(|c| c.0 <= h0 + step);
    // The tyre's circle over the ground ahead of and behind the hub: a step
    // under the front of the tyre lifts the wheel before the hub is over it.
    let mut top = h0;
    for d in [-0.6 * r, 0.6 * r] {
        if let Some((hd, _)) = h(hub.x + f.x * d, hub.z + f.z * d) {
            top = top.max(hd - (r - (r * r - d * d).sqrt()));
        }
    }
    // The plane's slope from the ground a little round the hub.
    let e = 0.4;
    let s = |dx: f32, dz: f32| h(hub.x + dx, hub.z + dz).map(|(y, _)| y);
    let gx = match (s(e, 0.0), s(-e, 0.0)) {
        (Some(a), Some(b)) => (a - b) / (2.0 * e),
        (Some(a), None) => (a - h0) / e,
        (None, Some(b)) => (h0 - b) / e,
        _ => 0.0,
    };
    let gz = match (s(0.0, e), s(0.0, -e)) {
        (Some(a), Some(b)) => (a - b) / (2.0 * e),
        (Some(a), None) => (a - h0) / e,
        (None, Some(b)) => (h0 - b) / e,
        _ => 0.0,
    };
    let mut n = Vec3::new(-gx.clamp(-2.5, 2.5), 1.0, -gz.clamp(-2.5, 2.5)).normalize();
    if n.y < 0.35 {
        n = (n + Vec3::Y * 0.5).normalize();
    }
    Some(Contact {
        point: Vec3::new(hub.x, top, hub.z),
        n,
        mat,
        surface: Surface::of(mat),
        depth: (level - h0).max(0.0),
    })
}

/// What the suspension does in one substep.
pub struct Spring {
    /// Hub below the mount, metres along the body's down.
    pub t: f32,
    /// Compression, 0 at full droop.
    pub x: f32,
    /// Rate of compression, m/s.
    pub x_dot: f32,
    /// Force pushing the body up along its own up, newtons.
    pub force: f32,
}

/// The spring and damper for a wheel hanging from `mount` (world) on a body
/// whose up is `u`, over plane `c`. None when the tyre does not reach.
/// `dead` makes the bump stop dead rather than springy: past full travel it
/// stops the body falling onto the wheel and stores nothing to throw it back
/// up with (the kart, whose travel is a few centimetres).
#[allow(clippy::too_many_arguments)]
pub fn suspend(c: &Contact, mount: Vec3, u: Vec3, v_mount: Vec3, r: f32, travel: f32, k: f32, mass: f32, damp: (f32, f32, f32), dead: bool, h: f32) -> Option<Spring> {
    let un = u.dot(c.n);
    if un < 0.25 {
        return None;
    }
    let t = ((mount - c.point).dot(c.n) - r) / un;
    if t >= travel {
        return None;
    }
    let x = travel - t;
    let x_dot = -v_mount.dot(c.n) / un;
    // Linear over the first 60% of the travel, then stiffening to the stop.
    let knee = 0.6 * travel;
    let mut f = k * x;
    if x > knee {
        let e = (x - knee) / (travel - knee);
        f += k * (x - knee) * 3.0 * e;
    }
    // The bump stop: very stiff, and the force is capped so a wheel that
    // has gone into the ground pushes out over a few steps rather than in one.
    if x > travel && !dead {
        f += 30.0 * k * (x - travel).min(0.15);
    }
    let zeta = if x_dot > 0.0 { damp.0 } else { damp.1 };
    let c_d = 2.0 * zeta * (k * mass).sqrt() * if x > travel && !dead { 3.0 } else { 1.0 };
    // A damper can stop the wheel's motion in a substep but never reverse
    // it: that is what keeps a car at rest from buzzing.
    let cap = mass * x_dot.abs() / h;
    // Past the knee speed the damper gives way, as a valved one does: a
    // sharp bump at speed is not passed whole to the body, and a wheel
    // dropping into a dip keeps its load. damp.2 is the knee, 0 for linear.
    let knee = damp.2;
    let v = if knee > 0.0 && x_dot.abs() > knee {
        x_dot.signum() * (knee + (x_dot.abs() - knee) * 0.2)
    } else {
        x_dot
    };
    let mut fd = (c_d * v).clamp(-cap, cap);
    if x > travel && dead {
        // Out of travel: the closing speed that comes of falling (a landing)
        // is stopped in this substep. What comes of running into a slope is
        // not: the caller lifts the body out of the ground instead
        // (`Vehicle::climb_out`), so a kerb is stepped up, never launched off.
        let fall = -v_mount.y * c.n.y / un;
        if fall > 0.0 {
            fd = fd.max(mass * fall / h);
        }
    }
    Some(Spring {
        t: t.max(0.0),
        x,
        x_dot,
        force: (f + fd).max(0.0),
    })
}
