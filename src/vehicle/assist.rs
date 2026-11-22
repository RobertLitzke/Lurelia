//! Help the driver doesn't see: yaw damping past what the steering asks for,
//! down-force on crests, air control, and turning the car back onto its
//! wheels. Each is a number in the spec, and 0 turns it off.

use super::body::Body;
use super::spec::Spec;
use super::Controls;
use glam::Vec3;

/// A counter-torque on yaw once the car turns faster than its steering and
/// speed call for: it takes the snap out of a slide without taking the slide.
pub fn yaw_damp(spec: &Spec, b: &Body, steer: f32, v_fwd: f32) -> Vec3 {
    if spec.yaw_damp <= 0.0 {
        return Vec3::ZERO;
    }
    let up = b.rot * Vec3::Y;
    let r = b.ang.dot(up);
    let want = v_fwd * steer.tan() / spec.wheelbase;
    let margin = 0.25 + 0.15 * want.abs();
    let excess = if r.abs() > want.abs() + margin && r * want >= 0.0 {
        r - want.signum() * (want.abs() + margin)
    } else if r * want < 0.0 && r.abs() > margin {
        // Turning against the steering: a spin, or a slide caught too late.
        r - r.signum() * margin
    } else {
        0.0
    };
    -up * excess * spec.yaw_damp * spec.inertia[1]
}

/// With the wheels down, damping on the body's pitch and roll rates, so a
/// bump under one wheel rocks the car less and for less long.
pub fn settle(spec: &Spec, b: &Body) -> Vec3 {
    if spec.settle <= 0.0 {
        return Vec3::ZERO;
    }
    let right = b.rot * Vec3::X;
    let back = b.rot * Vec3::Z;
    let i = Vec3::from(spec.inertia);
    -(right * (b.ang.dot(right) * i.x) + back * (b.ang.dot(back) * i.z)) * spec.settle
}

/// A pull on roll alone toward the ground under the wheels (`n`), so a car
/// that lands on two wheels or is tipped by a bump comes back down onto four
/// rather than going over. Pitch is left alone: jumps keep their nose.
pub fn upright(spec: &Spec, b: &Body, n: Vec3) -> Vec3 {
    if spec.upright <= 0.0 {
        return Vec3::ZERO;
    }
    let up = b.rot * Vec3::Y;
    let back = b.rot * Vec3::Z;
    let axis = up.cross(n);
    back * (axis.dot(back) * spec.upright * spec.inertia[2])
}

/// In the air: W/S pitch and A/D roll, both damped, and a weak pull toward
/// the slope the car is coming down on.
pub fn air(spec: &Spec, b: &Body, c: &Controls, ground_n: Option<Vec3>) -> Vec3 {
    let right = b.rot * Vec3::X;
    let back = b.rot * Vec3::Z;
    let up = b.rot * Vec3::Y;
    let i = Vec3::from(spec.inertia);
    // W noses down (a negative turn about the right axis), D rolls right
    // (a negative turn about the backward axis).
    let mut t = right * (-c.pitch * spec.air_pitch * i.x) + back * (-c.roll * spec.air_roll * i.z);
    // Damp pitch and roll, never yaw: the car keeps the heading it left on.
    let w_r = b.ang.dot(right);
    let w_b = b.ang.dot(back);
    t -= right * (w_r * spec.air_damp * i.x) + back * (w_b * spec.air_damp * i.z);
    if let Some(n) = ground_n {
        if spec.air_level > 0.0 && c.pitch == 0.0 && c.roll == 0.0 {
            let axis = up.cross(n);
            t += right * (axis.dot(right) * spec.air_level * i.x) + back * (axis.dot(back) * spec.air_level * i.z);
        }
    }
    t
}

/// Turning a car on its side or roof back onto its wheels: a torque toward
/// upright, damped, and a lift so it does not grind round on its roof.
pub fn right_up(spec: &Spec, b: &Body) -> (Vec3, Vec3) {
    let up = b.rot * Vec3::Y;
    let mut axis = up.cross(Vec3::Y);
    if axis.length() < 0.2 {
        // On its roof: roll about the long axis.
        axis = b.rot * Vec3::NEG_Z * if axis.dot(b.rot * Vec3::NEG_Z) >= 0.0 { 1.0 } else { -1.0 };
    }
    let i = Vec3::from(spec.inertia).max_element();
    let torque = (axis.normalize() * 9.0 - b.ang * 4.0) * i;
    let lift = if up.y < 0.6 { Vec3::Y * b.mass * spec.gravity * 1.15 } else { Vec3::ZERO };
    (torque, lift)
}
