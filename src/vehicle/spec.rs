//! A vehicle's tune, read from `library/vehicles/<name>.json`. A subtype is a
//! file that says `"like": "sand_rail"` and changes a few numbers
//! (src/inherit.rs). Every field has a default, so a file need only say what
//! makes it different. The keys are flat on purpose: `like` replaces nested
//! objects whole, and a flat file overrides one number at a time.
//!
//! The body frame is the chassis': origin at the centre of mass, -Z forward,
//! +Y up, +X right, metres.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Which handling model drives the tyres (grip.rs).
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Handling {
    /// Grip that peaks and slides: the buggy, an ATV.
    #[default]
    Slip,
    /// The four tyres as one, with grip near total: steering asks for a
    /// yaw rate and gets it up to what the grip allows (kart.rs).
    Kart,
}

/// How the keys and mouse steer.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Steering {
    /// The front wheels turn toward where the view points; A/D turn the view.
    #[default]
    Look,
    /// A/D steer the wheels; the mouse only looks.
    Direct,
}

/// Which wheels the engine turns.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Driven {
    #[default]
    Rear,
    All,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Spec {
    pub name: String,
    /// What the HUD and the notes call it.
    pub title: String,
    pub like: Option<String>,
    #[serde(rename = "version")]
    pub _version: u32,
    #[serde(rename = "introduced_by")]
    pub _introduced_by: String,
    #[serde(rename = "notes")]
    pub _notes: String,
    pub handling: Handling,
    pub steering: Steering,
    /// The drawn model (draw.rs): "sand_rail", "beach_buggy", "kart" or "atv".
    pub model: String,

    // The body.
    /// Kilograms, driver included.
    pub mass: f32,
    /// Moments of inertia about the body's x (pitch), y (yaw) and z (roll)
    /// axes, kg m².
    pub inertia: [f32; 3],
    /// Gravity on the vehicle, m/s². Heavier than real, as the bike's is, so
    /// jumps land with intent.
    pub gravity: f32,
    /// Centre of mass above the axles at rest, metres; negative is below.
    pub com_height: f32,

    // Wheels and suspension.
    pub wheelbase: f32,
    pub track: f32,
    /// Share of the weight on the rear axle.
    pub rear_weight: f32,
    pub wheel_radius: f32,
    pub tyre_width: f32,
    /// Wheel travel, metres.
    pub travel: f32,
    /// The most the ground under a wheel may rise in a step beyond the slope
    /// it was on, metres: more is a kerb or a wall the tyre meets, not ground
    /// it rolls onto. 0 lets a wheel climb voxel steps, as the buggy's do.
    pub climb: f32,
    /// Ride frequency front and rear, Hz.
    pub freq_front: f32,
    pub freq_rear: f32,
    /// Damping ratio in compression and in rebound.
    pub damp_bump: f32,
    pub damp_rebound: f32,
    /// Anti-roll bar, as a share of the corner's spring rate.
    pub anti_roll: f32,
    /// Damper speed, m/s, past which it gives way (a valved damper); 0 for
    /// a linear damper.
    pub damp_knee: f32,
    /// How high the tyres push on the body, as a share of the way from the
    /// ground to the centre of mass: 0 is a real car, 1 never rolls it.
    pub roll_lever: f32,
    /// Damping on pitch and roll with the wheels down, per second.
    pub settle: f32,
    /// The kart's drift: Space hops, and held with the steering over at
    /// speed it drifts, the rear loose (`drift_grip` of its grip) and the
    /// nose held about `drift_angle` degrees into the turn. Every
    /// `drift_charge` seconds of drift is a tier of boost, up to two, each
    /// `drift_boost` m/s on letting go. 0 boost is no drift: Space hops as
    /// on any car. On `handling: kart` the drift turns the kart's path with
    /// up to `drift_grip` of its grip (more than 1 turns tighter than the
    /// steering can), and charges fastest steered into the turn.
    pub drift_charge: f32,
    pub drift_boost: f32,
    pub drift_angle: f32,
    pub drift_grip: f32,
    /// Pull on roll toward the ground under the wheels, rad/s² per radian
    /// of lean: keeps it off its side.
    pub upright: f32,

    // Tyres.
    /// Peak grip on hardpack, as a friction coefficient.
    pub grip: f32,
    /// Grip held past the peak, as a share of it.
    pub slide: f32,
    /// Grip across a wheel the handbrake has locked, as a share of the peak.
    pub handbrake_grip: f32,

    // Drivetrain.
    pub driven: Driven,
    /// Peak drive force, newtons, and power, kW: the force falls as P / v.
    pub force: f32,
    pub power: f32,
    /// m/s, forward and in reverse.
    pub top_speed: f32,
    pub reverse_speed: f32,
    /// Braking, in g.
    pub brake: f32,
    /// Air drag, N per (m/s)².
    pub drag: f32,
    /// Extra force while boosting, newtons, and how much higher the top speed goes.
    pub boost_force: f32,
    pub boost_speed: f32,

    // Steering.
    /// Lock at rest and at top speed, degrees.
    pub lock: f32,
    pub lock_fast: f32,
    /// How fast the wheels turn toward the asked-for angle, degrees a second.
    pub steer_rate: f32,

    // Assists, 0 for off.
    /// Counter-torque on yaw past what the steering asks for, per second.
    pub yaw_damp: f32,
    /// Down-force with three or more wheels down, N per (m/s)².
    pub downforce: f32,
    /// Extra gravity, m/s², for the first half-second off the ground while
    /// the ground is close under the wheels: a kerb or a crest is a hop, not
    /// a flight, as kart games keep their karts on the road.
    pub stick: f32,
    /// Off the throttle, the rolling and air drag times this: above 1, the
    /// car loses its speed faster when let roll, as an engine braking does.
    pub coast: f32,
    /// Air control: pitch and roll acceleration from the keys, rad/s², and
    /// the damping on both, per second.
    pub air_pitch: f32,
    pub air_roll: f32,
    pub air_damp: f32,
    /// How hard the body is turned toward the ground it will land on.
    pub air_level: f32,
    /// Degrees nose-up that the air pull aims for over the ground below,
    /// so the rear wheels land a moment before the front (the ATV). When
    /// it is set the pull holds through the W/S keys, which add to it, and
    /// with no ground in reach the car is turned toward level plus this.
    pub air_tilt: f32,
    /// The hop: upward speed for a quick tap and for a fully loaded one, m/s.
    pub hop: f32,
    pub hop_loaded: f32,
    /// Landing slower than this (m/s into the springs) is a clean landing.
    pub clean_impact: f32,
    /// Speed given back by a clean landing, m/s.
    pub clean_kick: f32,
    /// How far the drawn chassis rolls and pitches with the acceleration,
    /// radians per g, on top of the body's own pose.
    pub visual_roll: f32,
    pub visual_pitch: f32,

    /// Chassis collision spheres, [x, y, z, radius] in the body frame.
    pub collide: Vec<[f32; 4]>,
    /// The footprint other karts meet (vehicle/touch.rs), width and length,
    /// metres, where the body is bigger than its wheels; None takes it from
    /// the wheelbase, track and tyres.
    pub bump_box: Option<[f32; 2]>,
    /// The driver's eye in the body frame.
    pub eye: [f32; 3],
    /// Chase camera distance and height.
    pub cam_dist: f32,
    pub cam_height: f32,
    /// Engine note at idle and at the top of a gear, Hz.
    pub idle_hz: f32,
    pub top_hz: f32,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            name: "sand_rail".into(),
            title: "SAND RAIL".into(),
            like: None,
            _version: 1,
            _introduced_by: String::new(),
            _notes: String::new(),
            handling: Handling::Slip,
            steering: Steering::Look,
            model: "sand_rail".into(),
            mass: 680.0,
            inertia: [620.0, 800.0, 260.0],
            gravity: 14.0,
            com_height: -0.05,
            wheelbase: 2.6,
            track: 1.7,
            rear_weight: 0.58,
            wheel_radius: 0.38,
            tyre_width: 0.3,
            travel: 0.45,
            climb: 0.0,
            freq_front: 1.6,
            freq_rear: 1.8,
            damp_bump: 0.3,
            damp_rebound: 0.6,
            anti_roll: 0.4,
            damp_knee: 0.0,
            roll_lever: 0.3,
            settle: 0.0,
            upright: 0.0,
            drift_charge: 0.8,
            drift_boost: 0.0,
            drift_angle: 25.0,
            drift_grip: 1.0,
            grip: 1.15,
            slide: 0.85,
            handbrake_grip: 0.33,
            driven: Driven::Rear,
            force: 5200.0,
            power: 110.0,
            top_speed: 30.0,
            reverse_speed: 8.0,
            brake: 1.1,
            drag: 0.8,
            boost_force: 3500.0,
            boost_speed: 6.0,
            lock: 35.0,
            lock_fast: 8.0,
            steer_rate: 160.0,
            yaw_damp: 1.5,
            downforce: 0.6,
            stick: 0.0,
            coast: 1.0,
            air_pitch: 4.0,
            air_roll: 4.0,
            air_damp: 1.5,
            air_level: 1.2,
            air_tilt: 0.0,
            hop: 2.6,
            hop_loaded: 5.0,
            clean_impact: 4.5,
            clean_kick: 1.5,
            visual_roll: 0.06,
            visual_pitch: 0.04,
            bump_box: None,
            collide: vec![
                // The ground sits 0.33 m under the centre of mass at rest,
                // so the belly is above it: the mass is carried lower than
                // the frame shows, which is what keeps the car on its wheels.
                // Nose, belly front and back, tail, the cage's four top
                // corners, and its shoulders.
                [0.0, 0.25, -1.7, 0.22],
                [0.0, 0.2, -0.6, 0.26],
                [0.0, 0.2, 0.5, 0.26],
                [0.0, 0.42, 1.4, 0.26],
                [-0.5, 1.45, -0.35, 0.16],
                [0.5, 1.45, -0.35, 0.16],
                [-0.5, 1.45, 0.55, 0.16],
                [0.5, 1.45, 0.55, 0.16],
                [-0.62, 0.75, 0.1, 0.22],
                [0.62, 0.75, 0.1, 0.22],
            ],
            eye: [0.0, 1.05, 0.15],
            cam_dist: 6.4,
            cam_height: 1.7,
            idle_hz: 38.0,
            top_hz: 120.0,
        }
    }
}

/// The ride frequency's spring and its corner mass, for axle `rear`.
pub struct Corner {
    pub mass: f32,
    pub k: f32,
    /// Compression at rest, metres.
    pub sag: f32,
}

impl Spec {
    pub fn corner(&self, rear: bool) -> Corner {
        let share = if rear { self.rear_weight } else { 1.0 - self.rear_weight };
        let mass = self.mass * share * 0.5;
        let f = if rear { self.freq_rear } else { self.freq_front };
        let w = std::f32::consts::TAU * f;
        let k = mass * w * w;
        let sag = (mass * self.gravity / k).min(self.travel * 0.6);
        Corner { mass, k, sag }
    }

    /// Where wheel `i` hangs from (front left, front right, rear left, rear
    /// right): the hub at full droop is `travel` below this, along the
    /// body's down.
    pub fn mount(&self, i: usize) -> glam::Vec3 {
        let rear = i >= 2;
        let left = i.is_multiple_of(2);
        let z = if rear { self.wheelbase * (1.0 - self.rear_weight) } else { -self.wheelbase * self.rear_weight };
        let x = self.track * 0.5 * if left { -1.0 } else { 1.0 };
        let c = self.corner(rear);
        // At rest the hub is `travel - sag` below the mount and the centre
        // of mass `com_height` above the hub.
        let y = -self.com_height + self.travel - c.sag;
        glam::Vec3::new(x, y, z)
    }
}

/// Every vehicle in the library, resolved, with the files they came from so
/// a changed file can be read again while driving.
#[derive(Default)]
pub struct Catalogue {
    pub specs: HashMap<String, Spec>,
    /// The file each vehicle was read from.
    pub files: HashMap<String, PathBuf>,
    dir: PathBuf,
    stamp: Option<SystemTime>,
    pub errors: Vec<String>,
}

fn newest(dir: &Path) -> Option<SystemTime> {
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).max()
}

impl Catalogue {
    /// The vehicles under `library/vehicles` of the data root.
    pub fn load(root: &Path) -> Catalogue {
        let dir = root.join("library").join("vehicles");
        let mut c = Catalogue { dir, ..Default::default() };
        c.read();
        c
    }

    fn read(&mut self) {
        self.stamp = newest(&self.dir);
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
            files.sort();
            for p in files {
                let file = p.display().to_string();
                match std::fs::read_to_string(&p)
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()))
                {
                    Ok(v) => entries.push((file, v)),
                    Err(e) => errors.push(format!("{file}: {e}")),
                }
            }
        }
        let mut specs = HashMap::new();
        let mut files = HashMap::new();
        for (file, v) in crate::inherit::resolve(entries, &mut errors) {
            match serde_json::from_value::<Spec>(v) {
                Ok(s) => {
                    files.insert(s.name.clone(), PathBuf::from(&file));
                    specs.insert(s.name.clone(), s);
                }
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
        for e in &errors {
            eprintln!("vehicles: {e}");
        }
        // A file that will not parse while it is being edited keeps the
        // last good tune rather than taking the vehicle away.
        if !specs.is_empty() || self.specs.is_empty() {
            self.specs = specs;
            self.files = files;
        }
        self.errors = errors;
    }

    /// Read the files again if any changed since the last read; true if so.
    pub fn reload_if_changed(&mut self) -> bool {
        let now = newest(&self.dir);
        if now.is_some() && now != self.stamp {
            self.read();
            return true;
        }
        false
    }

    /// The tune called `name`, or the built-in sand rail when the library
    /// has none by that name.
    pub fn get(&self, name: &str) -> Spec {
        self.specs.get(name).cloned().unwrap_or_else(|| Spec {
            name: name.to_string(),
            ..Spec::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The library's files parse, and a subtype inherits what it doesn't say.
    #[test]
    fn library_vehicles_resolve() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let c = Catalogue::load(&root);
        assert!(c.errors.is_empty(), "{:?}", c.errors);
        let rail = c.specs.get("sand_rail").expect("sand_rail");
        let beach = c.specs.get("beach_buggy").expect("beach_buggy");
        assert!(beach.mass < rail.mass && beach.driven == Driven::All && rail.driven == Driven::Rear);
        assert_eq!(beach.wheelbase, rail.wheelbase, "unsaid numbers come from the parent");
        assert_ne!(beach.title, rail.title);
    }

    /// At rest the hub hangs where the sag puts it, inside its travel.
    #[test]
    fn sag_sits_inside_the_travel() {
        let s = Spec::default();
        for rear in [false, true] {
            let c = s.corner(rear);
            assert!(c.sag > s.travel * 0.15 && c.sag < s.travel * 0.5, "sag {}", c.sag);
        }
    }
}
