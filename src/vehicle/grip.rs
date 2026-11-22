//! The tyres: how much of the push along and across a wheel the ground
//! gives back. One model so far, `Slip`: grip rises with the slip angle to a
//! peak and then holds most of it, and driving and cornering share one
//! friction circle, so a rear-driven car steps out under power and comes
//! back when the throttle is lifted.

use super::spec::Spec;
use glam::Vec3;

/// What a wheel is asked to do along its rolling direction this substep.
#[derive(Clone, Copy, Default, Debug)]
pub struct Ask {
    /// Drive force, newtons (negative in reverse).
    pub drive: f32,
    /// Brake force, newtons, always against the motion.
    pub brake: f32,
    /// Locked by the handbrake.
    pub locked: bool,
    /// Hold still: nothing asked of it and nearly stopped.
    pub hold: bool,
}

/// What the tyre did.
#[derive(Clone, Copy, Default, Debug)]
pub struct Grip {
    pub force: Vec3,
    /// Slip angle, radians.
    pub slip: f32,
    /// Speed the tyre is sliding across the ground, m/s.
    pub slide: f32,
    /// Share of the asked-for drive the tyre could not deliver.
    pub spin: f32,
    /// Speed along the wheel, m/s.
    pub v_long: f32,
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Grip across the wheel at slip angle `a`, as a share of the peak: a
/// smooth rise to the peak at `peak`, then easing down to `slide` of it.
pub fn curve(a: f32, peak: f32, slide: f32) -> f32 {
    if a < peak {
        let t = a / peak;
        t * (2.0 - t)
    } else {
        1.0 + (slide - 1.0) * smoothstep(peak, peak * 2.5, a)
    }
}

/// The tyre force for one wheel with `load` newtons on it, contact velocity
/// `v`, rolling along `f` and across `s` (both in the ground plane), on
/// ground with grip `mu` and peak slip `peak`. `m` is the share of the
/// car's mass this wheel can stop in a substep of `h` seconds; `drag` is
/// the surface's rolling drag, m/s².
#[allow(clippy::too_many_arguments)]
pub fn slip(spec: &Spec, ask: Ask, load: f32, v: Vec3, f: Vec3, s: Vec3, mu: f32, peak: f32, drag: f32, m: f32, h: f32) -> Grip {
    let v_long = v.dot(f);
    let v_lat = v.dot(s);
    let cap = mu * load;
    // The force that would stop this wheel's motion along a direction in
    // this substep. Half of it, since the four wheels share one body and
    // each pulling the whole of it overshoots.
    let stop = |vel: f32| -vel * m / h * 0.5;

    // Along the wheel.
    let mut long = 0.0;
    let mut asked = 0.0f32;
    if ask.locked {
        long = stop(v_long).clamp(-cap * spec.slide, cap * spec.slide);
    } else if ask.hold {
        long = stop(v_long).clamp(-cap, cap);
    } else {
        asked = ask.drive;
        long += ask.drive;
        if ask.brake > 0.0 {
            long += stop(v_long).clamp(-ask.brake, ask.brake);
        }
        // Rolling drag, from the load the wheel carries.
        let roll = load / spec.gravity.max(1.0) * drag;
        long += stop(v_long).clamp(-roll, roll);
    }
    let long_cap = long.clamp(-cap, cap);
    let spin = if asked.abs() > 1.0 { ((long.abs() - long_cap.abs()) / asked.abs()).clamp(0.0, 1.0) } else { 0.0 };
    let long = long_cap;

    // Across it.
    let a = v_lat.abs().atan2(v_long.abs().max(0.5));
    let speed = v.length();
    // Slow, the tyre holds as static friction does: a parked car on a slope
    // stays parked. Moving, the slip angle sets the grip.
    let w = smoothstep(0.5, 3.0, speed);
    let mut lat_mu = mu * (1.0 + (curve(a, peak, spec.slide) - 1.0) * w);
    if ask.locked {
        lat_mu *= spec.handbrake_grip;
    }
    // One friction circle: what the push along the wheel uses is not there
    // for cornering.
    let room = (cap * cap - long * long).max(0.0).sqrt();
    let lat_cap = (lat_mu * load).min(room);
    let want = stop(v_lat);
    let lat = want.clamp(-lat_cap, lat_cap);
    let slide = if want.abs() > lat_cap + 1.0 { v_lat.abs() } else { 0.0 };
    Grip {
        force: f * long + s * lat,
        slip: a,
        slide,
        spin,
        v_long,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_peaks_and_holds() {
        let p = 0.16;
        assert!(curve(0.0, p, 0.85).abs() < 1e-6);
        assert!((curve(p, p, 0.85) - 1.0).abs() < 1e-4);
        assert!(curve(p * 0.5, p, 0.85) > 0.7);
        let far = curve(p * 4.0, p, 0.85);
        assert!((far - 0.85).abs() < 1e-3, "{far}");
    }

    /// Full throttle leaves less grip for cornering than none.
    #[test]
    fn power_uses_up_the_grip_across() {
        let spec = Spec::default();
        let v = Vec3::new(0.6, 0.0, -10.0);
        let (f, s) = (Vec3::NEG_Z, Vec3::X);
        let coast = slip(&spec, Ask::default(), 3000.0, v, f, s, 1.0, 0.16, 0.0, 170.0, 1.0 / 240.0);
        let power = slip(&spec, Ask { drive: 2800.0, ..Default::default() }, 3000.0, v, f, s, 1.0, 0.16, 0.0, 170.0, 1.0 / 240.0);
        assert!(power.force.x.abs() < coast.force.x.abs(), "{} vs {}", power.force.x, coast.force.x);
    }
}
