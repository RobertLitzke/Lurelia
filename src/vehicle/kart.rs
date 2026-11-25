//! The kart's tyres (`handling: kart`): the four of them as one, with grip
//! near total. The buggy's slip model lets the steering ask for more than the
//! tyres hold, and past the peak a tyre lets go, so a kart on it washed wide
//! with any real input. Here the steering asks for a yaw rate and the tyres
//! give it up to what their grip allows, so full lock at speed is the
//! tightest line the kart can hold rather than a slide.
//!
//! - Along the kart: drive, brakes and rolling drag, as on the buggy.
//! - Across it: the sideways speed is taken out, up to the grip that the
//!   push along leaves (one friction circle for the whole kart).
//! - Yaw: steered to the bicycle model's rate, v tan(steer) / wheelbase,
//!   capped at the rate the grip can turn the kart's path at this speed.
//! - A drift turns the path by a set share of the grip (tighter steered in,
//!   wider steered out), its push across the way the kart is going rather
//!   than across its nose, so it does not scrub the speed off; the nose is
//!   held at the drift's angle into the turn.
//!
//! All of it pushes through the centre of mass: the tyres never lever the
//! kart over, and its lean is only drawn (`visual_roll`).

use super::body::Body;
use super::spec::Spec;
use glam::Vec3;

/// The ground the kart stands on this substep.
pub struct Ground {
    /// The ground's up, the average under the wheels that are down.
    pub n: Vec3,
    /// Load on all the tyres together, newtons.
    pub load: f32,
    /// Friction coefficient, surface and water taken in.
    pub mu: f32,
    /// Rolling drag, m/s².
    pub drag: f32,
}

/// What the driver asks of the tyres.
pub struct Ask {
    /// The front wheels' angle, radians, positive to the left.
    pub steer: f32,
    /// Drive force, newtons, and brake force, newtons.
    pub drive: f32,
    pub brake: f32,
    pub hold: bool,
    /// A drift: its way (1 right, -1 left) and how hard the keys hold it
    /// into the turn, -1 (out) to 1 (in).
    pub drift: Option<(f32, f32)>,
}

/// What the tyres did.
#[derive(Default)]
pub struct Out {
    pub force: Vec3,
    pub torque: Vec3,
    /// The kart's slip angle, radians: between its nose and its path.
    pub slip: f32,
    /// Sideways sliding speed the grip could not take out, m/s.
    pub slide: f32,
    /// Share of the asked-for drive not delivered.
    pub spin: f32,
    pub v_long: f32,
}

/// The kart's turning limit: the sideways acceleration its grip gives on
/// flat ground at `v` m/s, with the down-force, m/s². The steering lock
/// (`Vehicle::lock`) is cut to what this allows.
pub fn reach(s: &Spec, v: f32) -> f32 {
    s.grip * (s.gravity + s.downforce * v * v / s.mass)
}

/// The plain kart, `library/vehicles/kart.json`, read once: the kart the
/// bots' kart style was tuned on (`bot::Style::kart`), which every other
/// kart's driver is fitted against.
pub fn plain() -> &'static Spec {
    static PLAIN: std::sync::OnceLock<Spec> = std::sync::OnceLock::new();
    PLAIN.get_or_init(|| super::Catalogue::load(&crate::library::data_root()).get("kart"))
}

/// The tightest bend, as a curvature (1/m), that kart `s` holds at its top
/// speed with the grip it has at `v` m/s.
pub fn flat_out_bend(s: &Spec, v: f32) -> f32 {
    reach(s, v) / s.top_speed.max(1.0).powi(2)
}

/// How much of the grip the steering may ask for: a margin, so the kart
/// turns as asked and never at the very edge.
const MARGIN: f32 = 0.95;

/// How quickly the yaw rate follows what is asked, seconds.
const YAW_TAU: f32 = 0.06;

/// How much the steering tightens or widens a drift: the path turns with
/// 0.6 of the drift's grip, this share more steered in and less steered out.
const DRIFT_STEER: f32 = 0.5;
/// How quickly a drift's nose is brought to its angle, seconds.
const ANGLE_TAU: f32 = 0.2;

pub fn tyres(s: &Spec, b: &Body, g: &Ground, ask: &Ask, h: f32) -> Out {
    let n = g.n;
    let m = s.mass;
    let fwd = b.rot * Vec3::NEG_Z;
    let fl = (fwd - n * fwd.dot(n)).normalize_or(fwd);
    // Across the kart, to its left.
    let sd = n.cross(fl).normalize_or(b.rot * Vec3::NEG_X);
    let v = b.vel;
    let v_long = v.dot(fl);
    let v_lat = v.dot(sd);
    let cap = g.mu * g.load;
    // The force that would take out half the speed along a direction in a
    // substep: the rest of the body's forces still act on it.
    let stop = |vel: f32| -vel * m / h * 0.5;

    // Along the kart.
    let mut long = 0.0f32;
    if ask.hold {
        long = stop(v_long).clamp(-cap, cap);
    } else {
        long += ask.drive;
        if ask.brake > 0.0 {
            long += stop(v_long).clamp(-ask.brake, ask.brake);
        }
        let roll = g.load / s.gravity.max(1.0) * g.drag;
        long += stop(v_long).clamp(-roll, roll);
    }
    let held = long.clamp(-cap, cap);
    let spin = if ask.drive.abs() > 1.0 { ((long.abs() - held.abs()) / ask.drive.abs()).clamp(0.0, 1.0) } else { 0.0 };
    let long = held;
    let room = (cap * cap - long * long).max(0.0).sqrt();
    let a_max = cap / m;
    let speed = (v - n * v.dot(n)).length();

    let mut out = Out {
        spin,
        v_long,
        ..Default::default()
    };
    out.force = fl * long;
    let r = b.ang.dot(n);
    let r_want = match ask.drift.filter(|_| speed > 2.0) {
        None => {
            let want = stop(v_lat);
            let lat = want.clamp(-room, room);
            out.force += sd * lat;
            out.slide = if want.abs() > room + 1.0 { v_lat.abs() } else { 0.0 };
            out.slip = v_lat.atan2(v_long.abs().max(0.5)).abs();
            // No more than the grip the push along leaves can turn the
            // path, so the nose stays on it under power too.
            let geo = v_long * ask.steer.tan() / s.wheelbase;
            let most = MARGIN * room / m / speed.max(1.0);
            geo.clamp(-most, most)
        }
        Some((dir, keys)) => {
            let path = (v - n * v.dot(n)) / speed;
            // Toward the inside of the turn, across the path.
            let inside = path.cross(n) * dir;
            let k = keys.clamp(-1.0, 1.0);
            let a = s.drift_grip * a_max * (0.6 + DRIFT_STEER * k);
            // What the drive already pushes inward is taken off what the
            // tyres must, and the drift may use `drift_grip` of the grip.
            let want = m * a - long * fl.dot(inside);
            let most = cap * s.drift_grip.max(1.0);
            out.force += inside * want.clamp(0.0, (most * most - long * long).max(0.0).sqrt());
            let beta = path.cross(fl).dot(n).atan2(path.dot(fl));
            let angle = s.drift_angle.to_radians() * (0.75 + 0.45 * k);
            // Right is a turn the negative way about up.
            out.slip = beta.abs();
            -dir * a / speed + (-dir * angle - beta) / ANGLE_TAU
        }
    };
    // The yaw the tyres can bring about: no more than their grip levered
    // over half the wheelbase.
    let yaw = (s.inertia[1] * (r_want - r) / YAW_TAU).clamp(-cap * s.wheelbase * 0.5, cap * s.wheelbase * 0.5);
    out.torque = n * yaw;
    out
}
