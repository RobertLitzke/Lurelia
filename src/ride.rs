//! Movement beyond walking: a mountain bike, gliding, climbing and a rope.
//!
//! The bike follows the drawn ground as a height field and leaves it wherever
//! the ground falls away faster than gravity can follow, so any crest or lip
//! taken at speed is a jump. It rolls on its own; Space held loads the
//! suspension and letting go pops, and popping at a lip adds to the jump.
//! Ctrl tucks: lower drag and a harder pull downhill, and a descent taken
//! tucked stores a charge that the next jump spends as a lighter, floatier
//! arc.
//!
//! Gliding (glide.rs) is the one way to fly: an energy glider with real-ish
//! lift and drag, thermals and ridge lift, rolled rather than steered flat,
//! scored on how close it is flown (soar.rs). F launches it from the ground
//! or opens it in the air. Climbing and the rope (climb.rs) get the player up
//! cliffs and spires to glide from them.
//!
//! All modes feed a shared camera feel (wider view with speed, lean into turns,
//! a sprung eye that takes landings, a radial blur) and a chase camera whose
//! follow point trails the rider a little, so jumps and swings read as arcs.

mod climb;
mod drive;
mod glide;
pub use glide::GlideAir;
mod kayak;
pub mod soar;

use crate::mesh::ground_height_h;
use crate::models::Instance;
use crate::models::{Grid, Model};
use crate::modes::{Allowed, ModeId};
use crate::physics::{hits, move_body, move_body_with};
use crate::player::{smooth_damp, MoveInput, Player};
use crate::rig::{Camera, Chase, ChaseSpec};
use crate::world::material::{self, AIR};
use crate::world::store::{Cell, Store};
use glam::{Mat3, Quat, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Foot,
    Bike,
    /// Gliding (the suit with wings).
    Wing,
    /// Hands on a wall.
    Climb,
    /// Hanging from the rope.
    Swing,
    /// Sitting in the kayak (kayak.rs).
    Kayak,
    /// Driving a four-wheeler (drive.rs, src/vehicle).
    Drive,
}

/// Gravity for the bike in the air (rising; falling is a little stronger and
/// the top of the arc a little lighter, see `air_g`).
const BIKE_G: f32 = 16.0;
/// Gravity along a slope for the rolling bike. Lower than real, so hills
/// don't run away with it.
const SLOPE_G: f32 = 7.4;
/// Air drag on the bike, per metre: holds a steep descent near 70 km/h.
const BIKE_DRAG: f32 = 0.0058;
/// The bike rolls on its own up to this speed on the flat...
const CRUISE: f32 = 9.4;
/// ...and W pedals it up to this. Both are about a tenth below where they
/// used to sit: the bike picks up as fast as before but tops out lower, so
/// the speed worth having comes from the hills or a tuck.
const PEDAL: f32 = 12.2;
/// Seconds to fold into a full tuck, and to come out of one.
const TUCK_IN: f32 = 0.28;
/// A full tuck on a steep descent fills the charge in this long.
const TUCK_FILL: f32 = 2.2;
/// Out of the tuck on the ground, the charge bleeds away in this long.
const TUCK_BLEED: f32 = 5.0;
/// Eye above the bike's ground contact.
const BIKE_EYE: f32 = 1.42;
/// Eye above the flyer's feet (the body box's floor).
const WING_EYE: f32 = 0.9;
/// A full boost lasts this long held down...
const BOOST_SECS: f32 = 1.4;
/// ...and refills in this long; the wings refill a tenth slower.
const BOOST_REFILL: f32 = 4.0;
const BOOST_REFILL_WING: f32 = BOOST_REFILL / 0.9;
/// Air under the player needed to open the wings with Space (F opens them anywhere).
pub const WING_CLEARANCE: f32 = 6.0;
/// The prevailing wind blows toward +X, a little south.
const WIND: Vec3 = Vec3::new(crate::world::WIND.x, 0.0, crate::world::WIND.y);

/// Whether the chase camera starts on, per mode (V flips it). Every mode
/// starts in first person but driving, where the car is the thing to see.
const CHASE_DEFAULT: [bool; 7] = [false, false, false, false, false, false, true];

/// A climb over a top edge, played out over a fraction of a second.
#[derive(Clone, Copy)]
struct Mantle {
    from: Vec3,
    to: Vec3,
    t: f32,
}

impl Mode {
    /// The name the HUD and the stats show.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Foot => "WALK",
            Mode::Bike => "BIKE",
            Mode::Wing => "GLIDE",
            Mode::Climb => "CLIMB",
            Mode::Swing => "ROPE",
            Mode::Kayak => "KAYAK",
            Mode::Drive => "DRIVE",
        }
    }

    /// Exhaustive on purpose: a new mode must say what its camera does.
    fn cam(self, air: bool) -> ChaseSpec {
        let (height, across, upwards, dist) = match self {
            Mode::Foot => (1.4, 0.05, 0.05, 4.6),
            Mode::Bike => (1.5, 0.05, if air { 0.2 } else { 0.08 }, 3.6),
            Mode::Wing => (0.9, 0.08, 0.14, 4.8),
            Mode::Climb => (1.2, 0.1, 0.1, 4.0),
            Mode::Swing => (1.2, 0.1, 0.16, 4.6),
            Mode::Kayak => (0.9, 0.1, 0.1, 4.4),
            Mode::Drive => (1.7, 0.06, if air { 0.18 } else { 0.1 }, 6.4),
        };
        ChaseSpec { height, across, upwards, dist }
    }
}

#[derive(Default)]
pub struct Ride {
    /// Held on a race's grid (or after its finish): the car takes no keys.
    pub hold: bool,
    /// Controls a bot chose for the player's car this step, in place of the
    /// keys (a race's autopilot, for headless runs). Taken each step.
    pub auto: Option<crate::vehicle::Controls>,
    /// How hard the controller should shake, 0..1, from this step's knocks.
    /// main.rs takes it each frame (src/pad.rs).
    pub rumble: f32,
    pub mode: Mode,
    /// The modes this world allows (world.json `modes`).
    pub allowed: Allowed,
    /// Chase camera flipped from its default, per mode.
    chase_flip: [bool; 7],
    from_bike: bool,
    // Bike state.
    heading: f32,
    prev_heading: f32,
    speed: f32,
    vy: f32,
    pub air: bool,
    air_time: f32,
    slope: f32,
    turn_rate: f32,
    /// Heading change the bars are asking for, rad/s; eased so bursty mouse
    /// input doesn't make the lean flicker.
    steer: f32,
    /// Suspension loaded by holding Space, 0 to 1.
    preload: f32,
    /// Folded into a tuck (Ctrl), 0 to 1.
    tuck: f32,
    /// Charge stored by tucking down a descent, 0 to 1. The next jump spends
    /// it all.
    charge: f32,
    /// The charge the jump under way was launched with: how much lighter the
    /// air is, 0 to 1.
    float: f32,
    /// Streaks at the rim of the view, eased, 0 to 1.
    streaks: f32,
    /// Boost meter used up, 0 (full) to 1 (empty).
    spent: f32,
    /// Emptied the meter: no boost until Shift is let go.
    boost_lock: bool,
    /// A race's rules for the meter, set by the race every step: seconds of
    /// boost when full, seconds to refill, and whether it may be used now
    /// (only with no item carried; while it may not, it doesn't refill
    /// either). None is the ride's own meter.
    pub race_boost: Option<(f32, f32, bool)>,
    /// The car drawn lit up this frame: a race sets it while the player's
    /// kart blinks after taking damage.
    pub flash: bool,
    /// The car not drawn, and showing no lights: a race's kart past the
    /// finish.
    pub hide: bool,
    /// The view turned to look behind (a race's look-behind, held).
    pub look_back: bool,
    pub boosting: bool,
    boost_glow: f32,
    // Gliding.
    /// Seconds left of a launch's climb, before the wings take the air.
    launch_t: f32,
    /// Seconds left of the legs pushing at the start of a launch.
    launch_push: f32,
    /// Seconds left of the climb swinging round into forward flight.
    launch_swing: f32,
    aim_yaw: f32,
    aim_pitch: f32,
    look_was: bool,
    /// How fast the mouse is swinging the aim across, rad/s, smoothed: it
    /// banks the glider as well as aiming it.
    aim_rate: f32,
    /// Vertical speed, m/s, for the HUD.
    pub vario: f32,
    /// Rising air under the wings now, m/s, and what it is.
    pub lift: f32,
    lift_kind: glide::LiftKind,
    /// No thermals (tests that measure the glide itself).
    pub calm_air: bool,
    /// This world's thermals and ridge wind (world.json `glide`).
    pub glide_air: glide::GlideAir,
    /// How the water's surface slopes under the boat (rise per metre in x
    /// and z): the swell (world/surf.rs), set by the game each step. The
    /// kayak pitches and rolls with it.
    pub water_slope: glam::Vec2,
    /// Bank the body is flying at, radians; positive is right wing down.
    /// It has mass (`ROLL_ACCEL`, `ROLL_W`, `ROLL_ZETA` in glide.rs), so the
    /// keys push it rather than setting it.
    roll: f32,
    /// How fast the roll is turning, rad/s.
    roll_v: f32,
    /// A barrel roll under way: how far round it is, 0 to TAU, 0 when not
    /// rolling. Carried apart from `lean` so the view takes all of it.
    spin: f32,
    prev_spin: f32,
    /// Seconds before another barrel roll may start.
    spin_cd: f32,
    /// The run under way and the best one in this world (soar.rs).
    pub soar: soar::Soar,
    // Climbing.
    wall_n: Vec3,
    mantle: Option<Mantle>,
    lunge: f32,
    lunge_cd: f32,
    push_t: f32,
    // The rope.
    anchor: Vec3,
    anchor_vox: [i32; 3],
    rope_len: f32,
    /// Where the rope would catch if thrown now (None: nothing in reach, or
    /// the rope is put away).
    pub hook_target: Option<Vec3>,
    /// The target came from the aim assist rather than the crosshair's own ray.
    pub hook_assisted: bool,
    /// The rope is in hand (Z gets it out). Climbing and swinging have it out
    /// regardless.
    pub rope_out: bool,
    /// Seconds on the ground on foot since the rope was last used; it is put
    /// away after a moment.
    rope_idle: f32,
    /// The rope has been thrown since it came out (so landing puts it away).
    rope_used: bool,
    hook_target_vox: [i32; 3],
    hook_was: bool,
    /// Hints already shown, one bit per mode.
    told: u8,
    /// Climbed distance driving the hands' reach in first person, radians.
    climb_phase: f32,
    /// Draw the rider model even in first person (screenshot mode).
    pub show_body: bool,
    // Body pose for the model and the camera.
    lean: f32,
    prev_lean: f32,
    body_pitch: f32,
    prev_body_pitch: f32,
    // Camera feel.
    fov_k: f32,
    prev_fov_k: f32,
    fx: f32,
    eye_y: f32,
    prev_eye_y: f32,
    eye_v: f32,
    /// A landing bob on top of the eye spring: it overshoots once.
    bob: f32,
    bob_v: f32,
    shake: f32,
    t: f32,
    jump_was: bool,
    /// The chase camera's follow point, which trails the body (src/rig.rs).
    chase: Chase,
    // The kayak.
    /// The current under the player (set before each step), m/s in x and z.
    pub current: glam::Vec2,
    /// Strokes taken: the whole part counts strokes, the fraction is how far
    /// through this one.
    paddle_t: f32,
    /// This stroke's push, -1 (left blade) to 1 (right blade).
    stroke: f32,
    aground: bool,
    /// When the aground note was last shown (`t`), so it comes back while
    /// you keep paddling at a bar you can't see.
    aground_said: f32,
    /// Height of the air under the player, metres (None: more than 80 m).
    pub clearance: Option<f32>,
    /// A message for the HUD.
    pub note: Option<String>,
    /// The kayak left floating where its paddler rolled over the side
    /// (where, heading), to climb back into from the water (kayak.rs).
    pub moored: Option<(Vec3, f32)>,
    // Driving (drive.rs).
    /// The car being driven, and the one left parked.
    car: Option<crate::vehicle::Vehicle>,
    pub parked: Option<crate::vehicle::Vehicle>,
    /// The vehicles this world has (world.json `vehicles`), and which of
    /// them U brings.
    pub vehicles: Vec<String>,
    pub vehicle_pick: usize,
    /// Every vehicle tune in the library.
    pub catalogue: crate::vehicle::Catalogue,
    /// U held since it was pressed in the car: a tap gets out, a hold resets.
    out_t: Option<f32>,
    /// What the car did this step, for the sound and the dust.
    pub drive_events: crate::vehicle::Events,
    /// Seconds to the next look at the tune files.
    retune_t: f32,
    /// The tuning panel's line (vehicle/tune.rs).
    tune_pick: usize,
}

fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
}

fn dir_of(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// The yaw whose `dir_of` is the horizontal part of `d`.
fn yaw_of(d: Vec3) -> f32 {
    (-d.x).atan2(-d.z)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One step of a damped spring toward 0 (`freq` in Hz, `zeta` below 1
/// overshoots).
fn spring(x: f32, v: f32, freq: f32, zeta: f32, dt: f32) -> (f32, f32) {
    let w = std::f32::consts::TAU * freq;
    let v = v + (-w * w * x - 2.0 * zeta * w * v) * dt;
    (x + v * dt, v)
}

enum Ground {
    /// Solid at waist height: a wall, a trunk, a cliff.
    Wall,
    At(f32, u8),
    /// Nothing within reach below.
    Drop,
}

/// The ground under (x, z) below `y + 0.9` m and within `reach` m under `y`.
/// Where slopes are drawn this is the drawn surface, not the voxel top.
fn ground_at(store: &Store, x: f32, z: f32, y: f32, reach: f32, slopes: bool) -> Ground {
    let (vx, vz) = ((x * 4.0).floor() as i32, (z * 4.0).floor() as i32);
    let top = ((y + 0.9) * 4.0).floor() as i32;
    // Foliage is neither wall nor ground: the bike goes through bushes.
    // Water is ground to a ride: the bike crosses a filled pool rather
    // than sinking into it, as it did when water was simply solid.
    let hard = |vy: i32| matches!(store.cell(vx, vy, vz), Cell::Solid | Cell::Water);
    if hard(top) {
        return Ground::Wall;
    }
    let bottom = ((y - reach) * 4.0).floor() as i32;
    for vy in (bottom.max(0)..top).rev() {
        if hard(vy) {
            let t = vy + 1;
            let mut h = t as f32 / 4.0;
            if slopes {
                let at = |x: isize, y: isize, z: isize| store.get(x as i32, y as i32, z as i32).unwrap_or(AIR);
                if let Some(d) = ground_height_h(at, |x, z| store.ground_vox(x as i32, z as i32), x * 4.0, z * 4.0, t as f32) {
                    h = (d / 4.0).clamp(h - 0.3, h + 0.3);
                }
            }
            return Ground::At(h, store.get(vx, vy, vz).unwrap_or(AIR));
        }
    }
    Ground::Drop
}

/// Height of the first solid voxel top under (x, y, z), within `max` metres.
/// None when there is none, or the column hasn't loaded.
fn top_below(store: &Store, x: f32, z: f32, y: f32, max: f32) -> Option<f32> {
    let (vx, vz) = ((x * 4.0).floor() as i32, (z * 4.0).floor() as i32);
    let y0 = (y * 4.0).floor() as i32;
    let y1 = ((y - max) * 4.0).floor() as i32;
    for vy in (y1.max(0)..=y0).rev() {
        match store.get(vx, vy, vz) {
            None => return None,
            Some(m) if m != AIR => return Some((vy + 1) as f32 / 4.0),
            _ => {}
        }
    }
    None
}

/// Leaves and needles: the bike brushes through them, and they hold no rope.
fn foliage(m: u8) -> bool {
    crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE)
}

/// Like `physics::hits`, but foliage doesn't count; returns (hard hit,
/// foliage touched).
fn hits_soft(store: &Store, p: Vec3, hw: f32, h: f32) -> (bool, bool) {
    let v = |a: f32| (a * 4.0 + 1e-4).floor() as i32;
    let w = |a: f32| (a * 4.0 - 1e-4).floor() as i32;
    let mut soft = false;
    for y in v(p.y)..=w(p.y + h) {
        for z in v(p.z - hw)..=w(p.z + hw) {
            for x in v(p.x - hw)..=w(p.x + hw) {
                match store.cell(x, y, z) {
                    Cell::Solid | Cell::Water => return (true, soft),
                    Cell::Foliage => soft = true,
                    Cell::Open | Cell::Passable => {}
                }
            }
        }
    }
    (false, soft)
}

fn hits_hard(store: &Store, p: Vec3, hw: f32, h: f32) -> bool {
    hits_soft(store, p, hw, h).0
}

/// Every chunk a standing body box at `p` touches has loaded.
fn box_loaded(store: &Store, p: Vec3, hw: f32, h: f32) -> bool {
    let v = |a: f32| (a * 4.0).floor() as i32;
    [p.x - hw, p.x + hw].iter().all(|&x| {
        [p.z - hw, p.z + hw]
            .iter()
            .all(|&z| [p.y, p.y + h].iter().all(|&y| y >= store.top_m() || store.loaded(v(x), v(y), v(z))))
    })
}

/// Rolling resistance by surface, m/s².
fn rolling(m: u8) -> f32 {
    match m {
        material::SAND | material::SNOW => 1.6,
        material::GRASS | material::DIRT => 0.35,
        material::LEAVES | material::PINE | material::BIRCH_LEAVES | material::LEAVES_AUTUMN => 1.0,
        _ => 0.2,
    }
}

/// Bike gravity in the air: lighter over the top of the arc so jumps hang a
/// moment, heavier coming down so they still land with intent. `float` is the
/// charge the jump went up with (see `take_off`), which lightens all of it.
fn air_g(vy: f32, float: f32) -> f32 {
    let hang = 1.0 - 0.38 * (1.0 - smoothstep(0.0, 3.0, vy.abs()));
    let light = 1.0 - 0.3 * float.clamp(0.0, 1.0);
    if vy > 0.0 {
        BIKE_G * hang * light
    } else {
        (BIKE_G + 2.0) * hang * light
    }
}

impl Ride {
    /// Paddle strokes taken in the kayak: the whole part counts strokes
    /// (src/fx/hooks.rs throws spray as each one ends).
    pub fn strokes(&self) -> f32 {
        self.paddle_t
    }

    /// The blade in the water, -1 left to 1 right.
    pub fn stroke_side(&self) -> f32 {
        self.stroke
    }

    fn idx(m: Mode) -> usize {
        m as usize
    }

    /// Back on foot holding nothing: no rope out, no launch under way, no
    /// wall in hand. For leaving every mode at once (the free camera).
    pub fn drop_everything(&mut self) {
        self.soar.bank(0.0);
        self.mode = Mode::Foot;
        self.rope_out = false;
        self.rope_used = false;
        self.hook_target = None;
        self.launch_t = 0.0;
        self.from_bike = false;
        self.mantle = None;
        if let Some(car) = self.car.take() {
            self.parked = Some(car);
        }
    }

    /// Whether this world allows mode `m`; when it doesn't, say so on screen.
    pub(crate) fn can(&mut self, m: ModeId) -> bool {
        let ok = self.allowed.has(m);
        if !ok {
            self.note = Some(format!("NO {} ON THIS MAP", m.name().to_uppercase()));
        }
        ok
    }

    /// How fast A/D turn the view in this mode, radians a second; 0 where
    /// the keys do something else. This is view input, like the mouse, so
    /// `Player::turn_by_keys` applies it once a frame with the frame's own
    /// dt rather than inside the fixed step: inside the step a key turn
    /// moved the view in 60 Hz jumps, which above 60 fps you can see as a
    /// stutter the mouse doesn't have.
    pub fn key_turn(&self) -> f32 {
        match self.mode {
            Mode::Bike => 2.0,
            Mode::Kayak => 1.5,
            // A/D steer: with a direct tune, and always in the driver's seat.
            Mode::Drive if !self.chase_on() || self.car.as_ref().is_some_and(|c| c.spec.steering == crate::vehicle::Steering::Direct) => 0.0,
            Mode::Drive => 2.0,
            Mode::Foot | Mode::Wing | Mode::Climb | Mode::Swing => 0.0,
        }
    }

    pub fn chase_on(&self) -> bool {
        let i = Self::idx(self.mode);
        CHASE_DEFAULT[i] ^ self.chase_flip[i]
    }

    /// Chase camera on or off in every mode (headless checks).
    pub fn set_chase(&mut self, on: bool) {
        for (i, d) in CHASE_DEFAULT.iter().enumerate() {
            self.chase_flip[i] = *d != on;
        }
    }

    /// Speed in m/s for the HUD and the camera.
    pub fn speed(&self, p: &Player) -> f32 {
        match self.mode {
            Mode::Bike => (self.speed * self.speed + self.vy * self.vy).sqrt(),
            Mode::Wing | Mode::Swing | Mode::Climb => p.body.vel.length(),
            Mode::Kayak => Vec3::new(p.body.vel.x, 0.0, p.body.vel.z).length(),
            Mode::Foot => Vec3::new(p.body.vel.x, 0.0, p.body.vel.z).length(),
            Mode::Drive => p.body.vel.length(),
        }
    }

    /// Show a mode's key hint the first time only.
    fn tell(&mut self, m: Mode, text: &str) {
        let bit = 1u8 << Self::idx(m);
        if self.told & bit == 0 {
            self.told |= bit;
            self.note = Some(text.into());
        }
    }

    /// X: get on or off the bike.
    pub fn toggle_bike(&mut self, p: &mut Player) {
        match self.mode {
            Mode::Foot => {
                if p.swimming {
                    self.note = Some("NOT IN THE WATER".into());
                    return;
                }
                if !self.can(ModeId::Bike) {
                    return;
                }
                self.start_bike(p, Vec3::new(p.body.vel.x, 0.0, p.body.vel.z).length());
                self.rope_out = false;
                self.air = !p.body.on_ground;
                self.vy = p.body.vel.y;
                self.note = Some("BIKE   W PEDAL  SHIFT SPRINT  CTRL TUCK DOWNHILL  S BRAKE  HOLD SPACE, LET GO TO JUMP  V CAMERA".into());
            }
            Mode::Bike => {
                self.mode = Mode::Foot;
                p.body.vel = dir_of(self.heading) * self.speed * 0.5 + Vec3::Y * self.vy;
                p.body.on_ground = !self.air;
                self.note = Some("ON FOOT".into());
            }
            Mode::Wing | Mode::Swing | Mode::Climb => self.note = Some("LAND FIRST".into()),
            Mode::Kayak => self.note = Some("IN THE KAYAK: K TO GET OUT".into()),
            Mode::Drive => self.note = Some("DRIVING: U TO GET OUT".into()),
        }
    }

    /// Chase camera on or off while driving.
    pub fn set_drive_chase(&mut self, on: bool) {
        let i = Self::idx(Mode::Drive);
        self.chase_flip[i] = CHASE_DEFAULT[i] != on;
    }

    pub fn toggle_chase(&mut self) {
        if self.mode != Mode::Foot {
            let i = Self::idx(self.mode);
            self.chase_flip[i] = !self.chase_flip[i];
        }
    }

    fn start_bike(&mut self, p: &mut Player, speed: f32) {
        self.mode = Mode::Bike;
        self.heading = p.yaw;
        self.prev_heading = p.yaw;
        self.speed = speed;
        self.air_time = 0.0;
        self.preload = 0.0;
        if self.eye_y == 0.0 {
            self.eye_y = p.eye(1.0).y;
            self.prev_eye_y = self.eye_y;
        }
    }

    /// One 60 Hz step. Returns false when the player is on foot, so the
    /// walking code should run.
    pub fn step(&mut self, p: &mut Player, input: &MoveInput, store: &Store, sea: f32, slopes: bool, dt: f32) -> bool {
        self.prev_heading = self.heading;
        self.prev_lean = self.lean;
        self.prev_spin = self.spin;
        self.prev_body_pitch = self.body_pitch;
        self.prev_fov_k = self.fov_k;
        self.prev_eye_y = self.eye_y;
        self.chase.begin_step();
        self.t += dt;
        let jump_edge = input.jump && !self.jump_was;
        let jump_release = !input.jump && self.jump_was;
        self.jump_was = input.jump;
        let hook_edge = input.hook && !self.hook_was;
        self.hook_was = input.hook;
        self.clearance = top_below(store, p.pos.x, p.pos.z, p.pos.y - 0.01, 80.0).map(|h| p.pos.y - h);

        // Boost (Shift): a sprint on the bike with the wheels down, a push while
        // gliding. Drains while held, refills after.
        if !input.sprint {
            self.boost_lock = false;
        }
        let traction = match self.mode {
            Mode::Bike => !self.air,
            Mode::Drive => self.car.as_ref().is_some_and(|c| c.grounded > 0),
            Mode::Wing => self.launch_t <= 0.0,
            Mode::Kayak => input.forward,
            _ => false,
        };
        let (secs, race_refill, may) = self.race_boost.unwrap_or((BOOST_SECS, 0.0, true));
        let start_ok = self.boosting || self.spent < 0.85;
        self.boosting = input.sprint && traction && !self.boost_lock && start_ok && may;
        if self.boosting {
            self.spent += dt / secs;
            if self.spent >= 1.0 {
                self.spent = 1.0;
                self.boost_lock = true;
                self.boosting = false;
            }
        } else if may && (!input.sprint || !matches!(self.mode, Mode::Bike | Mode::Wing | Mode::Kayak | Mode::Drive)) {
            let refill = if self.race_boost.is_some() {
                race_refill
            } else if self.mode == Mode::Wing {
                BOOST_REFILL_WING
            } else {
                BOOST_REFILL
            };
            self.spent = (self.spent - dt / refill).max(0.0);
        }
        self.boost_glow += (self.boosting as i32 as f32 - self.boost_glow) * (dt * 6.0).min(1.0);

        // The rope: what it would catch, and throwing or dropping it.
        self.find_hook(p, store);
        if hook_edge {
            self.hook_pressed(p);
        }

        let handled = match self.mode {
            Mode::Foot => {
                // Opening the wings with Space: in the air, falling, with room under.
                let room = self.clearance.map_or(true, |c| c > WING_CLEARANCE);
                if jump_edge && !p.body.on_ground && !p.swimming && p.body.vel.y < 1.0 && room && self.allowed.has(ModeId::Glide) {
                    self.start_wing(p, false, true);
                    self.wing(p, input, store, sea, dt);
                    true
                } else if self.try_grab(p, input, store, dt) {
                    self.climb(p, input, store, false, dt);
                    true
                } else {
                    false
                }
            }
            Mode::Bike => {
                let room = self.clearance.map_or(true, |c| c > WING_CLEARANCE);
                if jump_edge && self.air && room && self.allowed.has(ModeId::Glide) {
                    self.start_wing(p, true, true);
                    self.wing(p, input, store, sea, dt);
                } else {
                    self.bike(p, input, store, sea, slopes, jump_release, dt);
                }
                true
            }
            Mode::Wing => {
                self.wing(p, input, store, sea, dt);
                true
            }
            Mode::Climb => {
                self.climb(p, input, store, jump_edge, dt);
                true
            }
            Mode::Swing => {
                self.swing(p, input, store, sea, jump_edge, dt);
                true
            }
            Mode::Kayak => {
                self.kayak(p, input, store, sea, dt);
                true
            }
            Mode::Drive => {
                self.drive(p, input, store, sea, slopes, dt);
                self.mode == Mode::Drive
            }
        };
        self.step_parked(p, store, sea, slopes, dt);
        self.retune_t -= dt;
        if self.retune_t <= 0.0 {
            self.retune_t = 1.0;
            self.retune();
        }

        // After using the rope, a moment on the ground on foot puts it away.
        if self.mode == Mode::Foot && p.body.on_ground && self.rope_out && self.rope_used {
            self.rope_idle += dt;
            if self.rope_idle > 1.0 {
                self.rope_out = false;
                self.rope_used = false;
            }
        } else {
            self.rope_idle = 0.0;
        }
        self.feel(p, dt);
        handled
    }

    fn bike(&mut self, p: &mut Player, input: &MoveInput, store: &Store, sea: f32, slopes: bool, jump_release: bool, dt: f32) {
        // Steering: the bike turns toward where the player looks, as fast as
        // grip allows; A/D turn the view (and so the bike, in `key_turn`,
        // once a frame). Looking far to the side or back leaves the bike
        // going straight.
        let was_air = self.air;
        let diff = wrap(p.yaw - self.heading);
        let rate = if self.air { 1.2 } else { (16.0 / self.speed.abs().max(1.0)).min(3.2) };
        // Turn in proportion to how far the view is off the heading, then ease
        // the rate: the heading follows the view smoothly instead of in bursts.
        let want = if diff.abs() < 1.7 { (diff * 5.0).clamp(-rate, rate) } else { 0.0 };
        self.steer += (want - self.steer) * (dt * 9.0).min(1.0);
        let turn = self.steer * dt;
        self.heading = wrap(self.heading + turn);
        self.turn_rate += (self.steer - self.turn_rate) * (dt * 5.0).min(1.0);
        let dir = dir_of(self.heading);

        // Space held loads the suspension; letting go pops the bike up. Let go
        // just after the wheels leave a lip and the pop adds to the jump.
        if input.jump && !self.air {
            self.preload = (self.preload + dt / 0.35).min(1.0);
        }
        if jump_release && self.preload > 0.0 {
            let pop = 4.4 + 2.8 * self.preload;
            if !self.air {
                self.vy = self.vy.max(0.0) + pop;
                self.air = true;
                self.air_time = 0.0;
            } else if self.air_time < 0.2 {
                self.vy = self.vy.max(0.0) + pop * 0.9;
                self.note = Some("POP".into());
            }
            self.preload = 0.0;
        } else if !input.jump {
            self.preload = 0.0;
        }

        // Ctrl folds the rider into a tuck: less drag, a harder pull downhill
        // and no real pedalling. The time on the pedals is traded for speed
        // the hill gives back.
        let tucking = input.down && !self.air && !input.back;
        let want = tucking as i32 as f32;
        self.tuck += (want - self.tuck) * (dt / TUCK_IN).min(1.0);

        let here = ground_at(store, p.pos.x, p.pos.z, p.pos.y, 4.0, slopes);
        let surface = if let Ground::At(_, m) = here { m } else { material::DIRT };
        if !self.air {
            self.slope = self.slope_at(store, p.pos, dir, slopes);
            let sin = self.slope / (1.0 + self.slope * self.slope).sqrt();
            let mut a = -SLOPE_G * sin;
            // Tucked, the slope pulls harder: a rider off the saddle with
            // their weight forward holds a line a sitting one brakes on.
            let descent = (-sin).max(0.0);
            a += SLOPE_G * 0.4 * self.tuck * descent;
            // The bike rolls along by itself; W pedals harder. A tuck is not
            // a pedalling position, so most of that push goes while held.
            let cruise = if input.forward { PEDAL } else { CRUISE };
            if !input.back && self.speed < cruise {
                a += (6.0 * (1.0 - self.speed / cruise)).max(1.2) * (1.0 - 0.75 * self.tuck);
            }
            // Sprinting out of the saddle, up to about 94 km/h.
            if self.boosting && self.speed < 26.0 {
                a += 16.0 * (1.0 - self.speed / 30.0);
            }
            if input.back {
                a += if self.speed > 0.3 {
                    -14.0
                } else if self.speed > -2.5 {
                    -3.0
                } else {
                    0.0
                };
            }
            a -= BIKE_DRAG * (1.0 - 0.45 * self.tuck) * self.speed * self.speed.abs();
            // A tuck down a descent at speed stores a charge for the next jump.
            if descent > 0.05 {
                self.charge = (self.charge + dt * self.tuck * smoothstep(4.0, 12.0, self.speed) * smoothstep(0.05, 0.3, descent) / TUCK_FILL).min(1.0);
            }
            if self.tuck < 0.5 {
                self.charge = (self.charge - dt / TUCK_BLEED).max(0.0);
            }
            // Cornering scrubs a little speed.
            a -= self.steer.abs() * self.speed.abs() * 0.04 * self.speed.signum();
            let roll = rolling(surface) * dt;
            self.speed += a * dt;
            if self.speed.abs() <= roll && !input.back && self.slope.abs() < 0.08 && self.speed < 0.5 {
                self.speed = 0.0;
            } else {
                self.speed -= roll * self.speed.signum();
            }
        } else {
            self.speed -= BIKE_DRAG * 0.8 * self.speed * self.speed.abs() * dt;
        }

        // Move across, a fifth of a metre at a time.
        let total = dir * self.speed * dt;
        let n = (total.length() / 0.2).ceil().max(1.0) as i32;
        let sub = total / n as f32;
        let mut impact = 0.0f32;
        for _ in 0..n {
            // The world hasn't streamed in ahead yet: wait at the edge of it
            // rather than crashing into the unloaded space.
            let ahead = p.pos + sub * 3.0;
            if !store.loaded((ahead.x * 4.0).floor() as i32, (p.pos.y * 4.0).floor() as i32, (ahead.z * 4.0).floor() as i32) {
                break;
            }
            let mut moved = false;
            for d in [sub, Vec3::new(sub.x, 0.0, 0.0), Vec3::new(0.0, 0.0, sub.z)] {
                if d.length_squared() < 1e-10 {
                    continue;
                }
                let q = p.pos + d;
                if !self.blocked(store, q, p.pos.y, slopes) {
                    p.pos = Vec3::new(q.x, p.pos.y, q.z);
                    moved = true;
                    if d != sub {
                        // Sliding along a wall loses the part of the speed into it.
                        self.speed *= 0.995;
                    }
                    break;
                }
            }
            if !moved {
                impact = self.speed.abs();
                self.speed = -self.speed * 0.15;
                break;
            }
        }
        // Pushing through a bush slows the bike and shakes it a little.
        if hits_soft(store, p.pos + Vec3::Y * 0.4, 0.3, 1.2).1 {
            self.speed *= 1.0 - (2.0 * dt).min(1.0);
            self.shake += self.speed.abs() * 0.0004;
        }
        if impact > 4.0 {
            self.shake += (impact * 0.012).min(0.12);
            self.bob_v -= impact * 0.12;
            if impact > 10.0 {
                self.note = Some("OOF".into());
            }
        }

        // Up and down: follow the ground unless it falls away faster than
        // gravity would take the bike. That is judged a little ahead, so the
        // treads of voxel stairs don't each count as a lip.
        let g = ground_at(store, p.pos.x, p.pos.z, p.pos.y.max(p.pos.y + self.vy * dt), 4.0, slopes);
        if !self.air {
            let fall = p.pos.y + self.vy * dt - 0.5 * BIKE_G * dt * dt;
            let ahead = 0.8 * self.speed.signum();
            let t = 0.8 / self.speed.abs().max(1.0);
            let path = p.pos.y + self.vy * t - 0.5 * BIKE_G * t * t;
            let lip = match ground_at(store, p.pos.x + dir.x * ahead, p.pos.z + dir.z * ahead, p.pos.y.max(path), 4.0, slopes) {
                // Blocky ground steps a whole voxel at a time; that alone is no lip.
                Ground::At(h, _) => h < path - if slopes { 0.12 } else { 0.3 },
                Ground::Drop => true,
                Ground::Wall => false,
            };
            match g {
                Ground::At(h, _) if h >= fall - 0.04 || (!lip && h >= p.pos.y - 0.6) => {
                    p.pos.y = h;
                    self.slope = self.slope_at(store, p.pos, dir, slopes);
                    // The bike rises at the rate of the ground it just covered,
                    // so it leaves a lip with the climb of the ramp.
                    self.vy = self.slope_back(store, p.pos, dir, slopes) * self.speed;
                }
                Ground::Wall => {
                    // The wheels are in rock: a landing that ended in a step,
                    // or ground generated round the bike. Staying put means
                    // staying stuck, since every way out is blocked too, so
                    // step up until there is room to ride.
                    if let Some(y) = (1..48).map(|k| p.pos.y + k as f32 * 0.25).find(|&y| !self.blocked(store, p.pos, y, slopes)) {
                        p.pos.y = y;
                    }
                }
                _ => {
                    self.air = true;
                    self.air_time = 0.0;
                    p.pos.y = fall;
                    self.vy -= BIKE_G * dt;
                }
            }
        } else {
            self.air_time += dt;
            self.vy -= air_g(self.vy, self.float) * dt;
            let ny = p.pos.y + self.vy * dt;
            let g = ground_at(store, p.pos.x, p.pos.z, p.pos.y.max(ny), 4.0 + (p.pos.y - ny).max(0.0), slopes);
            match g {
                Ground::At(h, _) if h >= ny && self.vy <= 0.5 => {
                    p.pos.y = h;
                    self.land(store, p.pos, dir, slopes);
                }
                Ground::Wall if self.vy <= 0.0 => {
                    // Came down into a step taller than the waist test: sit on the voxel top.
                    if let Some(h) = top_below(store, p.pos.x, p.pos.z, p.pos.y + 1.0, 1.2) {
                        p.pos.y = h;
                    }
                    self.land(store, p.pos, dir, slopes);
                }
                _ => p.pos.y = ny,
            }
        }

        // Just left the ground, off a lip or off a pop: spend the tuck.
        if self.air && !was_air {
            self.take_off();
        }

        if p.pos.y + 0.5 < sea {
            self.mode = Mode::Foot;
            p.body.vel = dir * self.speed * 0.3;
            self.note = Some("SPLASH".into());
            return;
        }
        p.body.vel = dir * self.speed + Vec3::Y * self.vy;
        p.body.on_ground = !self.air;

        // Pose: lean into turns, pitch with the slope or the flight. In the air
        // the bike swings nose-up off the lip and noses over into the landing.
        let lean = (self.speed.abs() * self.turn_rate / 9.8).atan().clamp(-0.7, 0.7);
        self.lean += (lean - self.lean) * (dt * 6.0).min(1.0);
        let pitch = if self.air { (self.vy / self.speed.abs().max(4.0)).atan() * 0.85 } else { self.slope.atan() };
        self.body_pitch += (pitch - self.body_pitch) * (dt * if self.air { 3.0 } else { 14.0 }).min(1.0);

        // The eye rides a spring over the ground's kinks, sinks as the
        // suspension loads, and bobs once on landings.
        let target = p.pos.y + BIKE_EYE - 0.22 * self.preload - 0.16 * self.tuck;
        let (x, v) = smooth_damp((self.eye_y - self.bob - target).clamp(-0.7, 0.7), self.eye_v, 0.07, dt);
        let (b, bv) = spring(self.bob, self.bob_v, 2.3, 0.42, dt);
        self.bob = b.clamp(-0.6, 0.4);
        self.bob_v = bv;
        self.eye_y = target + x + self.bob;
        self.eye_v = v;

        // Rough ground at speed shakes; it decays fast.
        if !self.air {
            self.shake += self.speed.abs() * 0.0006 * dt * 60.0 * if rolling(surface) > 0.3 { 1.0 } else { 0.4 } * 0.02;
        }
        self.shake *= 1.0 - (dt * 6.0).min(1.0);
    }

    /// Leaving the ground, off a lip or off a pop: the charge a tuck stored
    /// goes into the jump, as a kick up and air that holds the whole arc
    /// (`air_g`).
    fn take_off(&mut self) {
        self.float = self.charge;
        self.charge = 0.0;
        if self.float > 0.25 {
            self.vy += 2.4 * self.float;
            self.note = Some(format!("FLOAT   {:.0}%", self.float * 100.0));
        } else if self.float > 0.0 {
            self.vy += 2.4 * self.float;
        }
    }

    fn land(&mut self, store: &Store, pos: Vec3, dir: Vec3, slopes: bool) {
        self.slope = self.slope_at(store, pos, dir, slopes);
        let follow = self.slope * self.speed;
        // How hard the bike meets the ground: the vertical speed the ground
        // would take minus the vertical speed it came down with.
        let impact = follow - self.vy;
        if self.air_time > 0.5 {
            if impact < 5.0 {
                self.speed += 1.2;
                self.note = Some(format!("CLEAN LANDING   {:.1} S AIR", self.air_time));
            } else {
                self.note = Some(format!("{:.1} S AIR", self.air_time));
            }
        }
        if impact > 7.0 {
            self.speed *= 1.0 - ((impact - 7.0) * 0.05).min(0.5);
        }
        self.bob_v -= impact.max(0.0) * 0.2;
        self.shake += (impact * 0.006).clamp(0.0, 0.1);
        self.vy = follow;
        self.air = false;
        self.preload = 0.0;
        self.float = 0.0;
    }

    /// Slope of the ground over the last 0.8 m behind `pos` (along `dir`).
    fn slope_back(&self, store: &Store, pos: Vec3, dir: Vec3, slopes: bool) -> f32 {
        let k = -0.8 * self.speed.signum();
        match ground_at(store, pos.x + dir.x * k, pos.z + dir.z * k, pos.y, 2.0, slopes) {
            Ground::At(h, _) => ((pos.y - h) / 0.8 * self.speed.signum()).clamp(-1.5, 1.5),
            _ => self.slope,
        }
    }

    /// Slope of the drawn ground along `dir` at `pos`, rise over run.
    fn slope_at(&self, store: &Store, pos: Vec3, dir: Vec3, slopes: bool) -> f32 {
        let s = |k: f32| match ground_at(store, pos.x + dir.x * k, pos.z + dir.z * k, pos.y, 2.0, slopes) {
            Ground::At(h, _) => Some(h),
            _ => None,
        };
        let s = match (s(0.8), s(-0.8)) {
            (Some(a), Some(b)) => (a - b) / 1.6,
            (Some(a), None) => (a - pos.y) / 0.8,
            (None, Some(b)) => (pos.y - b) / 0.8,
            _ => 0.0,
        };
        s.clamp(-1.5, 1.5)
    }

    /// Would the bike, moved to `q` (riding at height `y`), hit something?
    fn blocked(&self, store: &Store, q: Vec3, y: f32, slopes: bool) -> bool {
        let floor = if self.air {
            y
        } else {
            match ground_at(store, q.x, q.z, y, 4.0, slopes) {
                Ground::Wall => return true,
                Ground::At(h, _) if h - y > 0.8 => return true,
                Ground::At(h, _) => h.max(y),
                Ground::Drop => y,
            }
        };
        hits_hard(store, Vec3::new(q.x, floor + 0.55, q.z), 0.25, 1.0)
    }

    /// Field of view, blur, the eye spring and the chase camera's follow
    /// point, for any mode.
    fn feel(&mut self, p: &Player, dt: f32) {
        let spd = self.speed(p);
        // Flying close to anything adds to the rush: without sound this is
        // most of what a near miss feels like.
        let close = match (self.mode, self.soar.near) {
            (Mode::Wing, Some(d)) => (1.0 - smoothstep(1.5, 10.0, d)) * smoothstep(12.0, 28.0, spd),
            _ => 0.0,
        };
        let target = 1.0 + 0.34 * smoothstep(7.0, 38.0, spd) + 0.08 * self.boost_glow + 0.10 * close;
        if self.fov_k == 0.0 {
            self.fov_k = target;
            self.prev_fov_k = target;
        }
        self.fov_k += (target - self.fov_k) * (1.0 - (-3.0 * dt).exp());
        let fx = (smoothstep(12.0, 42.0, spd) + 0.35 * self.boost_glow + close * 0.6).min(1.0) * if self.mode == Mode::Foot { 0.5 } else { 1.0 };
        self.fx += (fx - self.fx) * (1.0 - (-4.0 * dt).exp());
        // Streaks at the rim of the view: only when really moving, and never
        // on foot, where nothing is fast enough to earn them.
        let st = if self.mode == Mode::Foot {
            0.0
        } else {
            (smoothstep(13.0, 26.0, spd) + 0.3 * self.boost_glow).min(1.0)
        };
        self.streaks += (st - self.streaks) * (1.0 - (-3.0 * dt).exp());
        if self.mode == Mode::Foot {
            self.lean *= 1.0 - (dt * 6.0).min(1.0);
            self.body_pitch *= 1.0 - (dt * 6.0).min(1.0);
            self.shake *= 1.0 - (dt * 6.0).min(1.0);
            self.eye_y = p.eye(1.0).y;
            self.eye_v = 0.0;
            self.bob = 0.0;
            self.bob_v = 0.0;
        }

        // The chase camera aims at a point that trails the body: a little
        // across, more up and down, most of all in the air.
        let mut cam = self.mode.cam(self.air);
        if let Some(car) = self.car() {
            // Each vehicle says how far back and how high the chase sits.
            cam.dist = car.spec.cam_dist;
            cam.height = car.spec.cam_height;
        }
        self.chase.follow(p.pos, cam, self.mode as u8, dt);
    }

    /// The rider's head, for aiming and sighting.
    pub fn head(&self, p: &Player, alpha: f32) -> Vec3 {
        if let Some(car) = self.car() {
            let (pos, rot) = car.pose(alpha);
            let eye = pos + rot * Vec3::from(car.spec.eye);
            return Vec3::new(eye.x, self.prev_eye_y + (self.eye_y - self.prev_eye_y) * alpha, eye.z);
        }
        let mut xz = p.prev_pos.lerp(p.pos, alpha);
        if self.mode == Mode::Bike {
            let h = self.prev_heading + wrap(self.heading - self.prev_heading) * alpha;
            xz -= dir_of(h) * 0.3;
        }
        Vec3::new(xz.x, self.prev_eye_y + (self.eye_y - self.prev_eye_y) * alpha, xz.z)
    }

    /// Where screenshot mode's free camera starts. In first person on a ride
    /// the eye is inside the rider, and screenshot mode draws the whole body,
    /// so start back where the chase camera would be, short of any wall.
    pub fn free_camera_start(&self, p: &Player, store: &Store, eye: Vec3, fwd: Vec3) -> Vec3 {
        if self.mode == Mode::Foot || self.chase_on() {
            return eye;
        }
        let spec = self.mode.cam(self.air);
        let follow = p.pos + Vec3::Y * spec.height;
        let dir = (-fwd + Vec3::Y * 0.3).normalize_or(Vec3::Y);
        let d = store.ray(follow, dir, spec.dist, |m| m != AIR).map_or(spec.dist, |h| (h.t - 0.3).max(0.0));
        follow + dir * d
    }

    /// Where the camera is and where it looks.
    pub fn camera(&self, p: &Player, store: &Store, alpha: f32) -> Camera {
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let fov_scale = lerp(self.prev_fov_k, self.fov_k).max(1.0);
        let lean = lerp(self.prev_lean, self.lean);
        let spin = lerp(self.prev_spin, self.spin);
        let body_pitch = lerp(self.prev_body_pitch, self.body_pitch);
        let t = self.t;
        let sh = self.shake.min(0.3);
        let jitter = Vec3::new(((t * 37.0).sin() + (t * 23.3).sin() * 0.6) * sh * 0.05, ((t * 41.0).sin() + (t * 17.9).cos() * 0.6) * sh * 0.05, 0.0);
        let chase = self.chase_on() && self.mode != Mode::Foot;
        // On the bike the view tips with the slope, and more with the flight.
        let tip = match self.mode {
            Mode::Bike if !chase => body_pitch * if self.air { 0.5 } else { 0.3 },
            Mode::Drive if !chase => body_pitch * 0.6,
            Mode::Kayak if !chase => body_pitch * 0.5,
            _ => 0.0,
        };
        // In the driver's seat p.yaw turns with the car a whole step at a
        // time (drive.rs); the car drawn this frame is part way through that
        // step, so the view is put back by the part it has not turned yet.
        let behind = if self.mode == Mode::Drive && !chase {
            wrap(self.prev_heading + wrap(self.heading - self.prev_heading) * alpha - self.heading)
        } else {
            0.0
        };
        let back = if self.look_back && self.mode == Mode::Drive { std::f32::consts::PI } else { 0.0 };
        let yaw = p.yaw + behind + back + jitter.x;
        let pitch = (p.pitch + tip + jitter.y).clamp(-1.55, 1.55);
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let fwd = Vec3::new(-sy * cp, sp, -cy * cp);
        let right = Vec3::new(cy, 0.0, -sy);
        let roll = match self.mode {
            Mode::Foot => lean,
            Mode::Bike => lean * if chase { 0.3 } else { 0.6 },
            Mode::Wing => lean * if chase { 0.35 } else { 0.6 } + spin,
            Mode::Climb | Mode::Swing => lean * 0.3,
            Mode::Kayak => lean * if chase { 0.4 } else { 0.7 },
            Mode::Drive => lean * if chase { 0.15 } else { 0.7 },
        };
        let up = (Vec3::Y * roll.cos() - right * roll.sin()).normalize();
        let speed_fx = self.fx;
        let streaks = self.streaks;
        if !chase {
            let eye = if self.mode == Mode::Foot { p.eye(alpha) } else { self.head(p, alpha) };
            return Camera {
                eye,
                fwd,
                up,
                fov_scale,
                speed_fx,
                streaks,
            };
        }
        // Faster, further back, so the bike does not fill the frame.
        let dist = self.mode.cam(self.air).dist + self.speed(p) * 0.03;
        self.chase.camera(fwd, up, dist, store, alpha, fov_scale, speed_fx, streaks)
    }

    /// The rider model to draw this frame, if any. `first` is the index of the
    /// first ride model in the game's model list.
    pub fn instance(&self, p: &Player, alpha: f32, first: usize, models: &[Model]) -> Option<(usize, Instance)> {
        let chase = self.chase_on() || self.show_body;
        let mut i = self.body_instance(p, alpha, first, models, chase)?;
        if !chase && matches!(self.mode, Mode::Bike | Mode::Kayak) {
            // The first-person cut casts no shadow; `stand_in` casts the whole one.
            i.1.shift[3] = NO_SHADOW;
        }
        Some(i)
    }

    /// In first person the view carries only part of the body (the sleeves,
    /// the bike without its rider, the hull), and a shadow cast from that part
    /// alone reads as two loose wings on the ground. This is the chase
    /// model drawn into the shadow map only, so the shadow is the whole rider.
    pub fn stand_in(&self, p: &Player, alpha: f32, first: usize, models: &[Model]) -> Option<(usize, Instance)> {
        if self.chase_on() || self.show_body || !matches!(self.mode, Mode::Bike | Mode::Wing | Mode::Kayak) {
            return None;
        }
        let mut i = self.body_instance(p, alpha, first, models, true)?;
        i.1.shift[3] = SHADOW_ONLY;
        Some(i)
    }

    fn body_instance(&self, p: &Player, alpha: f32, first: usize, models: &[Model], chase: bool) -> Option<(usize, Instance)> {
        let m = match (self.mode, chase) {
            (Mode::Bike, true) => first + BIKE_RIDER,
            (Mode::Bike, false) => first + BIKE_ONLY,
            (Mode::Wing, true) => first + FLYER,
            (Mode::Climb | Mode::Swing, true) => first + CLIMBER,
            (Mode::Kayak, true) => first + KAYAK_RIDER,
            (Mode::Kayak, false) => first + KAYAK_HULL,
            _ => return None,
        };
        let md = models.get(m)?;
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let mut pos = p.prev_pos.lerp(p.pos, alpha);
        let mut yaw = self.prev_heading + wrap(self.heading - self.prev_heading) * alpha;
        let lean = lerp(self.prev_lean, self.lean) + lerp(self.prev_spin, self.spin);
        let pitch = lerp(self.prev_body_pitch, self.body_pitch);
        let lift = if self.mode == Mode::Wing { 0.55 } else { 0.0 };
        let mut q = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(lean);
        if m == first + BIKE_ONLY {
            pos.y -= FP_DROP;
        }
        if self.mode == Mode::Kayak {
            // The hull sits in the water a hand deep and rides the swell.
            let eye = lerp(self.prev_eye_y, self.eye_y);
            pos.y = eye - kayak::KAYAK_EYE - 0.12;
            if !chase {
                pos += dir_of(yaw) * 0.05;
            }
        }
        if self.mode == Mode::Swing {
            // Hang from the rope: the body lines up with it, feet away from the anchor.
            let hand = pos + Vec3::Y * climb::HAND;
            let up = (self.anchor - hand).normalize_or(Vec3::Y);
            pos = hand - up * climb::HAND;
            q = Quat::from_rotation_arc(Vec3::Y, up) * Quat::from_rotation_y(yaw);
            yaw = 0.0;
        }
        Some((
            m,
            Instance {
                pos_yaw: [pos.x, pos.y + lift, pos.z, yaw],
                info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                tint: [1.0, 1.0, 1.0, 0.0],
                rot: [q.x, q.y, q.z, q.w],
                shift: [0.0; 4],
                flesh: [[0.0; 4]; 3],
                glow: [0.0; 4],
                fade: [0.0; 4],
            },
        ))
    }

    /// What the player sees of themself in first person besides the bike: the
    /// wing sleeves while gliding, both hands on the wall while climbing, and
    /// a hand on the rope. None in the chase camera or in screenshot mode.
    pub fn gear(&self, p: &Player, store: &Store, alpha: f32, first: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        if self.mode == Mode::Kayak {
            self.paddle(p, alpha, first, models, out);
            return;
        }
        if self.chase_on() || self.show_body {
            return;
        }
        let mut put = |m: usize, pos: Vec3, q: Quat| {
            if let Some(md) = models.get(first + m) {
                out.push((
                    first + m,
                    Instance {
                        pos_yaw: [pos.x, pos.y, pos.z, 0.0],
                        info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                        tint: [1.0, 1.0, 1.0, 0.0],
                        rot: [q.x, q.y, q.z, q.w],
                        shift: [0.0; 4],
                        flesh: [[0.0; 4]; 3],
                        glow: [0.0; 4],
                        fade: [0.0; 4],
                    },
                ));
            }
        };
        match self.mode {
            Mode::Bike => {
                // Follows the first-person bike's pose (see `instance`).
                let lerp = |a: f32, b: f32| a + (b - a) * alpha;
                let yaw = self.prev_heading + wrap(self.heading - self.prev_heading) * alpha;
                let q = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(lerp(self.prev_body_pitch, self.body_pitch)) * Quat::from_rotation_z(lerp(self.prev_lean, self.lean));
                let base = p.prev_pos.lerp(p.pos, alpha) - Vec3::Y * FP_DROP;
                put(BAR_FP, base + q * Vec3::new(0.0, BAR_FP_UP, -BAR_FP_AHEAD), q);
            }
            Mode::Wing => {
                // Fixed to the view, so they bank with it.
                let cam = self.camera(p, store, alpha);
                let right = cam.fwd.cross(cam.up).normalize();
                let up = right.cross(cam.fwd);
                let q = Quat::from_mat3(&Mat3::from_cols(right, up, -cam.fwd));
                for (m, side) in [(WINGS_FP, -1.0f32), (WING_FP_R, 1.0)] {
                    let w = wing_pivot(side);
                    put(m, cam.eye + right * w.x + up * w.y - cam.fwd * w.z, q);
                }
                // `stand_in` casts the shadow; these two alone would cast a pair of wings.
                let n = out.len();
                for (m, i) in &mut out[n.saturating_sub(2)..] {
                    if *m == first + WINGS_FP || *m == first + WING_FP_R {
                        i.shift[3] = NO_SHADOW;
                    }
                }
            }
            Mode::Climb if self.mantle.is_none() => {
                // On the rock in front, reaching in turn as the climber moves.
                let n = self.wall_n;
                let side = Vec3::new(-n.z, 0.0, n.x);
                let q = Quat::from_rotation_y(self.heading);
                let body = p.prev_pos.lerp(p.pos, alpha);
                for (k, s) in [(0.0f32, -1.0f32), (std::f32::consts::PI, 1.0)] {
                    let reach = 0.5 + 0.5 * (self.climb_phase + k).sin();
                    let hand = body - n * 0.24 + side * (0.2 * s) + Vec3::Y * (crate::player::EYE - 0.12 + 0.3 * reach);
                    put(GLOVE, hand - q * GLOVE_AT, q);
                }
            }
            Mode::Swing => {
                let hand = p.prev_pos.lerp(p.pos, alpha) + Vec3::Y * climb::HAND + Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin()) * 0.3 + dir_of(p.yaw) * 0.25 - Vec3::Y * 0.2;
                let up = (self.anchor - hand).normalize_or(Vec3::Y);
                let q = Quat::from_rotation_arc(Vec3::Y, up) * Quat::from_rotation_y(p.yaw);
                put(GLOVE, hand - q * GLOVE_AT, q);
            }
            _ => {}
        }
    }

    /// The rope, as short segment models from the hand to the anchor.
    pub fn rope(&self, p: &Player, alpha: f32, first: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        if self.mode != Mode::Swing {
            return;
        }
        let Some(md) = models.get(first + ROPE) else {
            return;
        };
        let mut hand = p.prev_pos.lerp(p.pos, alpha) + Vec3::Y * climb::HAND;
        if !self.chase_on() {
            // In first person the line leaves the right hand, not the eye.
            hand += Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin()) * 0.3 + dir_of(p.yaw) * 0.25 - Vec3::Y * 0.2;
        }
        let d = self.anchor - hand;
        let len = d.length();
        if len < 0.1 {
            return;
        }
        let dir = d / len;
        let q = Quat::from_rotation_arc(Vec3::Y, dir);
        let n = (len / ROPE_SEG).ceil() as i32;
        for i in 0..n.min(200) {
            let a = hand + dir * (i as f32 * ROPE_SEG);
            out.push((
                first + ROPE,
                Instance {
                    pos_yaw: [a.x, a.y, a.z, 0.0],
                    info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                    tint: [1.0, 1.0, 1.0, 0.0],
                    rot: [q.x, q.y, q.z, q.w],
                    shift: [0.0; 4],
                    flesh: [[0.0; 4]; 3],
                    glow: [0.0; 4],
                    fade: [0.0; 4],
                },
            ));
        }
    }

    /// The moored kayak, rocking on the swell where it was left.
    pub fn moored_instance(&self, time: f32, first: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        let Some((at, heading)) = self.moored else { return };
        let Some(md) = models.get(first + KAYAK_EMPTY) else { return };
        let bob = (time * 1.3 + at.x * 0.1).sin() * 0.04;
        let roll = (time * 0.9 + at.z * 0.1).sin() * 0.05;
        let q = Quat::from_rotation_z(roll);
        out.push((
            first + KAYAK_EMPTY,
            Instance {
                pos_yaw: [at.x, at.y - 0.12 + bob, at.z, heading],
                info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                tint: [1.0, 1.0, 1.0, 0.0],
                rot: [q.x, q.y, q.z, q.w],
                shift: [0.0; 4],
                flesh: [[0.0; 4]; 3],
                glow: [0.0; 4],
                fade: [0.0; 4],
            },
        ));
    }

    /// Boost left, 0 to 1, and whether it is firing; None unless biking or gliding.
    pub fn boost_meter(&self) -> Option<(f32, bool)> {
        matches!(self.mode, Mode::Bike | Mode::Wing | Mode::Kayak | Mode::Drive).then_some((1.0 - self.spent, self.boosting))
    }

    /// One line for the HUD while riding.
    pub fn hud(&self, p: &Player) -> Option<String> {
        let kmh = self.speed(p) * 3.6;
        match self.mode {
            Mode::Foot => None,
            Mode::Bike => Some(if self.air && self.air_time > 0.25 {
                let float = if self.float > 0.25 { "   FLOAT" } else { "" };
                format!("{kmh:.0} KM/H   AIR {:.1} S{float}", self.air_time)
            } else if self.charge > 0.05 {
                format!("{kmh:.0} KM/H   TUCK {:.0}%", self.charge * 100.0)
            } else {
                format!("{kmh:.0} KM/H")
            }),
            Mode::Wing => {
                let alt = self.clearance.map_or(">80".to_string(), |c| format!("{c:.0}"));
                let lift = match self.lift_kind {
                    glide::LiftKind::Thermal => "   THERMAL",
                    glide::LiftKind::Ridge => "   RIDGE LIFT",
                    glide::LiftKind::None => "",
                };
                Some(format!("GLIDING   {kmh:.0} KM/H   {alt} M UP   {:+.1} M/S{lift}", self.vario))
            }
            Mode::Climb => Some(format!("CLIMBING   {:.0} M UP", self.clearance.unwrap_or(0.0))),
            Mode::Swing => Some(format!("ROPE   {:.0} M OUT   {kmh:.0} KM/H", self.rope_len)),
            Mode::Drive => {
                let car = self.car()?;
                let mut line = format!("{}   {kmh:.0} KM/H", car.spec.title);
                if car.waiting {
                    line += "   WAITING FOR THE WORLD";
                } else if car.stalled {
                    line += "   STALLED";
                } else if car.wade > 0.2 {
                    line += "   WADING";
                } else if car.air_time > 0.25 {
                    line += &format!("   AIR {:.1} S", car.air_time);
                }
                Some(line)
            }
            Mode::Kayak => Some(if self.aground {
                format!("KAYAK   {kmh:.0} KM/H   AGROUND")
            } else {
                format!("KAYAK   {kmh:.0} KM/H")
            }),
        }
    }
}

/// `Instance::shift[3]` flags: drawn into the shadow map only, or everywhere but it.
pub const SHADOW_ONLY: f32 = 1.0;
pub const NO_SHADOW: f32 = 2.0;

// Ride models, appended after the game's other models in this order.
pub const BIKE_RIDER: usize = 0;
pub const BIKE_ONLY: usize = 1;
pub const FLYER: usize = 2;
pub const CLIMBER: usize = 3;
pub const ROPE: usize = 4;
pub const WINGS_FP: usize = 5;
pub const GLOVE: usize = 6;
pub const BAR_FP: usize = 7;
pub const WING_FP_R: usize = 8;
pub const KAYAK_RIDER: usize = 9;
pub const KAYAK_HULL: usize = 10;
pub const PADDLE: usize = 11;
/// The kayak with nobody in it, moored.
pub const KAYAK_EMPTY: usize = 12;
/// Length of one rope segment model, metres.
const ROPE_SEG: f32 = 0.5;

// Palette slots 48-63 belong to the ride models.
pub(crate) const FRAME: u8 = 48;
pub(crate) const TYRE: u8 = 49;
pub(crate) const METAL: u8 = 50;
pub(crate) const JACKET: u8 = 51;
pub(crate) const TROUSERS: u8 = 52;
pub(crate) const SKIN: u8 = 53;
pub(crate) const HELMET: u8 = 54;
const SUIT: u8 = 55;
pub(crate) const SUIT_DARK: u8 = 56;
pub(crate) const PACK: u8 = 57;
pub(crate) const BOOT: u8 = 58;
const CORD: u8 = 59;
const HULL: u8 = 60;
const BLADE: u8 = 61;

/// The ride models' colours, in their palette slots (48..64).
pub fn palette(p: &mut [[f32; 4]]) {
    use crate::models::srgb;
    p[FRAME as usize] = srgb(196, 72, 38, 0.0);
    p[TYRE as usize] = srgb(34, 32, 30, 0.0);
    p[METAL as usize] = srgb(150, 152, 156, 0.0);
    p[JACKET as usize] = srgb(52, 104, 112, 0.0);
    p[TROUSERS as usize] = srgb(120, 106, 78, 0.0);
    p[SKIN as usize] = srgb(214, 166, 128, 0.0);
    p[HELMET as usize] = srgb(232, 190, 60, 0.0);
    p[SUIT as usize] = srgb(222, 98, 40, 0.0);
    p[SUIT_DARK as usize] = srgb(52, 50, 56, 0.0);
    p[PACK as usize] = srgb(88, 96, 62, 0.0);
    p[BOOT as usize] = srgb(60, 44, 34, 0.0);
    p[CORD as usize] = srgb(236, 200, 64, 0.0);
    p[HULL as usize] = srgb(118, 60, 48, 0.0);
    p[BLADE as usize] = srgb(150, 124, 70, 0.0);
}

/// Fill every voxel within `r` of the segment a-b (model voxel coordinates).
fn seg(g: &mut Grid, a: [f32; 3], b: [f32; 3], r: f32, c: u8) {
    let (a, b) = (Vec3::from(a), Vec3::from(b));
    let lo = a.min(b) - Vec3::splat(r + 1.0);
    let hi = a.max(b) + Vec3::splat(r + 1.0);
    let ab = b - a;
    for y in lo.y.floor() as i32..=hi.y.ceil() as i32 {
        for z in lo.z.floor() as i32..=hi.z.ceil() as i32 {
            for x in lo.x.floor() as i32..=hi.x.ceil() as i32 {
                let pt = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                let t = ((pt - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                if (a + ab * t - pt).length() <= r {
                    g.set(x, y, z, c);
                }
            }
        }
    }
}

/// A wheel in the plane x = 16, centred at (y, z): a round tyre and a thin
/// rim, each a ring of rods, and four spokes, all drawn as tubes.
fn wheel(g: &mut Grid, y: f32, z: f32) {
    let ring = |g: &mut Grid, rad: f32, r: f32, c: u8| {
        let n = 24;
        for k in 0..n {
            let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
            g.rod([16.0, y + rad * a0.cos(), z + rad * a0.sin()], [16.0, y + rad * a1.cos(), z + rad * a1.sin()], r, c);
        }
    };
    ring(g, 4.95, 0.65, TYRE);
    ring(g, 4.1, 0.3, METAL);
    for k in 0..4 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4 + 0.3;
        g.rod([16.0, y - 4.0 * a.cos(), z - 4.0 * a.sin()], [16.0, y + 4.0 * a.cos(), z + 4.0 * a.sin()], 0.22, METAL);
    }
}

/// Mountain bike facing -Z, wheels on y = 0; 1/16 m voxels. `whole` false for
/// the first-person bike: no saddle (it sits right under the eye) and no stem
/// or bar, which `bar_fp` draws thinner.
fn bike(g: &mut Grid, whole: bool) {
    let (wy, front, rear) = (5.6, 6.5, 24.5);
    wheel(g, wy, front);
    wheel(g, wy, rear);
    let bb = [16.0, 4.5, 16.5];
    let head_lo = [16.0, 11.0, 9.0];
    let head_hi = [16.0, 14.0, 8.4];
    let seat = [16.0, 14.0, 19.0];
    // The frame, fork, bar and cranks are rods: round tubes at any angle.
    g.rod(bb, head_lo, 0.75, FRAME);
    g.rod(seat, head_hi, 0.6, FRAME);
    g.rod(bb, seat, 0.65, FRAME);
    g.rod(bb, [16.0, wy, rear], 0.5, FRAME);
    g.rod(seat, [16.0, wy, rear], 0.5, FRAME);
    g.rod(head_lo, head_hi, 0.8, FRAME);
    g.rod(head_lo, [16.0, wy, front], 0.55, METAL);
    if whole {
        // Stem, bar and grips.
        g.rod(head_hi, [16.0, 16.0, 8.0], 0.5, METAL);
        g.rod([10.5, 16.5, 8.5], [21.5, 16.5, 8.5], 0.45, METAL);
        g.rod([9.3, 16.5, 8.5], [10.5, 16.5, 8.5], 0.6, TYRE);
        g.rod([21.5, 16.5, 8.5], [22.7, 16.5, 8.5], 0.6, TYRE);
        // Saddle.
        g.fill([15, 15, 18], [18, 16, 22], TYRE);
    }
    // Cranks.
    g.rod([13.0, 4.5, 15.0], [19.0, 4.5, 18.0], 0.5, METAL);
}

/// A gloved hand closed on the grip at `x`, with a bit of sleeve behind it
/// (first person). Slimmer than the chase rider's arms, so they sit in the
/// corners of the view rather than filling them.
fn g_hand(g: &mut Grid, x: f32) {
    g.ellipsoid([x, 16.4, 8.2], [0.8, 0.7, 0.8], BOOT);
    seg(g, [x, 16.3, 9.0], [x + (x - 16.0) * 0.35, 13.5, 14.0], 0.55, JACKET);
}

/// The first-person bike is drawn this far below the real one, which puts the
/// bar a little lower in the view.
const FP_DROP: f32 = 0.07;
/// Where the first-person bar's pivot is, from the bike's ground contact:
/// ahead, and up (the 1 m model grid has to reach the bar).
const BAR_FP_AHEAD: f32 = 0.469;
const BAR_FP_UP: f32 = 0.5;

/// The first-person stem and bar in 1/32 m voxels: 3 cm thick where 1/16 m
/// voxels made a slab across the view. The bar runs between the gloves of the
/// first-person bike and the stem drops to its head tube.
fn bar_fp(g: &mut Grid) {
    g.rod([5.0, 16.5, 16.5], [27.0, 16.5, 16.5], 0.55, METAL);
    g.rod([16.0, 12.0, 16.5], [16.0, 16.5, 16.5], 0.7, METAL);
}

/// Rider standing on the pedals, leaning over the bar.
fn rider(g: &mut Grid) {
    for (x, z) in [(13.5, 15.0), (18.5, 18.0)] {
        g.fill([x as i32 - 1, 4, z as i32 - 1], [x as i32 + 1, 6, z as i32 + 2], BOOT);
        seg(g, [x, 6.0, z], [x, 12.5, z - 2.5], 0.95, TROUSERS);
        seg(g, [x, 12.5, z - 2.5], [x, 17.5, 18.5], 1.05, TROUSERS);
    }
    // Torso from hips to shoulders, leaning forward.
    seg(g, [16.0, 18.0, 18.0], [16.0, 23.0, 12.5], 2.4, JACKET);
    g.ellipsoid([16.0, 22.5, 16.0], [2.6, 2.2, 2.4], PACK);
    for x in [12.5f32, 19.5] {
        seg(g, [x, 22.5, 12.5], [x + (16.0 - x) * 0.1, 18.5, 9.5], 0.8, JACKET);
        seg(g, [x + (16.0 - x) * 0.1, 18.5, 9.5], [if x < 16.0 { 10.5 } else { 21.5 }, 16.5, 8.0], 0.7, SKIN);
    }
    g.ellipsoid([16.0, 25.5, 10.5], [1.9, 2.1, 2.0], SKIN);
    g.ellipsoid([16.0, 26.6, 10.8], [2.3, 1.8, 2.5], HELMET);
}

/// A flyer lying face down, head toward -Z, wings spread.
fn flyer(g: &mut Grid) {
    // Arm wings: from shoulders to wrists, the membrane back to the hips.
    for side in [-1.0f32, 1.0] {
        let sx = 16.0 + side * 2.5;
        let wrist = [16.0 + side * 14.0, 6.0, 11.0];
        for k in 0..=24 {
            let t = k as f32 / 24.0;
            let a = [sx + (wrist[0] - sx) * t, 6.0, 9.0 + (wrist[2] - 9.0) * t];
            let b = [16.0 + side * 2.0, 5.5, 20.0 - 6.0 * t];
            seg(g, a, b, 0.55, if t > 0.85 { SUIT_DARK } else { SUIT });
        }
        // The spar along the leading edge.
        g.rod([sx, 6.2, 9.0], wrist, 0.85, SUIT_DARK);
        g.fill([wrist[0] as i32 - 1, 5, 10], [wrist[0] as i32 + 1, 7, 12], SKIN);
    }
    // Leg wing between the legs.
    for k in 0..=12 {
        let t = k as f32 / 12.0;
        seg(g, [16.0 - 1.5 - 3.5 * t, 5.0, 20.0 + 10.0 * t], [16.0 + 1.5 + 3.5 * t, 5.0, 20.0 + 10.0 * t], 0.55, SUIT);
    }
    for side in [-1.0f32, 1.0] {
        g.rod([16.0 + side * 1.8, 5.2, 19.0], [16.0 + side * 5.0, 5.2, 30.0], 1.0, SUIT_DARK);
        g.fill([16 + (side * 5.0) as i32 - 1, 4, 30], [16 + (side * 5.0) as i32 + 1, 7, 32], BOOT);
    }
    // Body, pack and head.
    g.ellipsoid([16.0, 6.0, 14.5], [3.0, 2.0, 7.0], SUIT);
    g.ellipsoid([16.0, 8.0, 15.0], [2.2, 1.0, 3.5], PACK);
    g.ellipsoid([16.0, 6.6, 5.6], [1.9, 1.9, 2.1], HELMET);
    g.fill([15, 5, 3], [18, 7, 4], SUIT_DARK);
}

/// A climber facing -Z (the wall), reaching up with both hands.
fn climber(g: &mut Grid) {
    for x in [14.2f32, 17.8] {
        g.fill([x as i32 - 1, 0, 15], [x as i32 + 1, 2, 18], BOOT);
        seg(g, [x, 2.0, 16.6], [x + (16.0 - x) * 0.15, 13.0, 16.4], 1.05, TROUSERS);
    }
    seg(g, [16.0, 13.0, 16.4], [16.0, 21.0, 16.0], 2.3, JACKET);
    g.ellipsoid([16.0, 18.0, 18.6], [2.3, 2.8, 1.3], PACK);
    for side in [-1.0f32, 1.0] {
        let sh = [16.0 + side * 2.6, 21.0, 16.0];
        let hand = [16.0 + side * 3.6, 28.0, 15.2];
        seg(g, sh, hand, 0.8, JACKET);
        g.ellipsoid(hand, [0.8, 0.9, 0.8], SKIN);
    }
    g.ellipsoid([16.0, 23.6, 15.8], [1.8, 2.0, 1.9], SKIN);
    g.ellipsoid([16.0, 24.6, 16.1], [2.1, 1.5, 2.2], HELMET);
}

/// One first-person wing (`side` -1 left, 1 right) in camera space, 1/32 m
/// voxels, one model per arm so each gets the whole 32-voxel grid: voxel
/// (16 + 32(x - 0.46 side), 16 + 32(y + 0.5), 16 + 32(z + 0.45)) is x right,
/// y up and z back from the eye, in metres; the pivot is at `wing_pivot`.
/// The sleeve runs out to a lower corner of the view with the membrane below.
fn wing_fp(g: &mut Grid, side: f32) {
    let c = |x: f32, y: f32, z: f32| [16.0 + 32.0 * (x - 0.46 * side), 16.0 + 32.0 * (y + 0.5), 16.0 + 32.0 * (z + 0.45)];
    let shoulder = c(0.22 * side, -0.34, -0.05);
    let wrist = c(0.7 * side, -0.38, -0.85);
    // The membrane: from the arm down and back to the side of the body.
    for k in 0..=24 {
        let t = k as f32 / 24.0;
        let a = [
            shoulder[0] + (wrist[0] - shoulder[0]) * t,
            shoulder[1] + (wrist[1] - shoulder[1]) * t,
            shoulder[2] + (wrist[2] - shoulder[2]) * t,
        ];
        let b = c(0.3 * side, -0.62, -0.05 - 0.25 * t);
        seg(g, a, b, 0.6, if t > 0.92 { SUIT_DARK } else { SUIT });
    }
    // The sleeve along the leading edge, a darker seam, and the glove.
    g.rod(shoulder, wrist, 0.9, SUIT);
    g.rod([shoulder[0], shoulder[1] + 0.8, shoulder[2]], [wrist[0], wrist[1] + 0.8, wrist[2]], 0.5, SUIT_DARK);
    g.ellipsoid(wrist, [1.1, 0.9, 1.3], BOOT);
}

/// Camera-space position (x right, y up, z back) of a first-person wing's pivot.
fn wing_pivot(side: f32) -> Vec3 {
    Vec3::new(0.46 * side, -1.0, -0.45)
}

/// A gloved hand reaching forward (-Z) with its sleeve running back and down;
/// 1/64 m voxels, the glove `GLOVE_AT` from the pivot.
fn glove(g: &mut Grid) {
    g.ellipsoid([16.0, 20.0, 10.0], [3.0, 2.3, 3.4], BOOT);
    seg(g, [16.0, 19.0, 13.5], [16.0, 3.0, 31.0], 2.4, JACKET);
}

/// Where the glove sits in its model, from the pivot (metres).
const GLOVE_AT: Vec3 = Vec3::new(0.0, 20.0 / 64.0, -6.0 / 64.0);

/// One rope segment: 0.5 m of cord along +Y from the pivot (1/64 m voxels).
fn rope_seg(g: &mut Grid) {
    g.fill([15, 0, 15], [17, 32, 17], CORD);
}

/// The ride models, in the order of the constants above.
pub fn models() -> Vec<Model> {
    let mut a = Grid::new();
    bike(&mut a, true);
    rider(&mut a);
    let mut b = Grid::new();
    bike(&mut b, false);
    for x in [10.0f32, 22.0] {
        g_hand(&mut b, x);
    }
    let mut c = Grid::new();
    flyer(&mut c);
    let mut d = Grid::new();
    climber(&mut d);
    let mut e = Grid::new();
    rope_seg(&mut e);
    let mut f = Grid::new();
    wing_fp(&mut f, -1.0);
    let mut f2 = Grid::new();
    wing_fp(&mut f2, 1.0);
    let mut g = Grid::new();
    glove(&mut g);
    let mut h = Grid::new();
    bar_fp(&mut h);
    vec![
        a.build(1.0 / 16.0, 0.3, 1.8),
        b.build(1.0 / 16.0, 0.3, 1.1),
        c.build(1.0 / 16.0, 0.9, 0.6),
        d.build(1.0 / 16.0, 0.3, 1.8),
        e.build(1.0 / 64.0, 0.02, 0.5),
        f.build(1.0 / 32.0, 0.5, 1.0),
        g.build(1.0 / 64.0, 0.1, 0.5),
        h.build(1.0 / 32.0, 0.1, 0.6),
        f2.build(1.0 / 32.0, 0.5, 1.0),
        {
            let mut k = Grid::new();
            kayak::kayak_model(&mut k, true);
            k.build(1.0 / 8.0, 0.4, 1.4)
        },
        {
            let mut k = Grid::new();
            kayak::bow_model(&mut k);
            let mut m = k.build(1.0 / 16.0, 0.4, 0.4);
            kayak::bow_pivot(&mut m);
            m
        },
        {
            let mut k = Grid::new();
            kayak::paddle_model(&mut k);
            let mut m = k.build(2.2 / 32.0, 0.1, 0.5);
            kayak::paddle_pivot(&mut m);
            m
        },
        {
            let mut k = Grid::new();
            kayak::kayak_model(&mut k, false);
            k.build(1.0 / 8.0, 0.4, 1.4)
        },
    ]
}

/// Shared helpers for the movement tests.
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::world::material::{GRASS, STONE};
    use crate::world::store::Voxels;

    /// A store over x in [-32, 160) m and z in [-16, 16) m, 160 m tall,
    /// with ground at `top(x)` metres.
    pub fn land(top: impl Fn(f32) -> f32) -> Store {
        land_in(top, (-4, 20), (-2, 2))
    }

    /// Ground at `top(x)` over chunk ranges (8 m chunks) in x and z.
    pub fn land_in(top: impl Fn(f32) -> f32, cxs: (i32, i32), czs: (i32, i32)) -> Store {
        let mut s = Store::default();
        for cz in czs.0..czs.1 {
            for cx in cxs.0..cxs.1 {
                for cy in 0..20 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in cxs.0 * 32..cxs.1 * 32 {
            let h = (top(x as f32 / 4.0 + 0.125) * 4.0).round() as i32;
            for z in czs.0 * 32..czs.1 * 32 {
                for y in 0..h.max(1) {
                    s.set(x, y, z, if y + 1 == h { GRASS } else { STONE });
                }
            }
        }
        s
    }

    /// A body and what it is riding, which in the game is `Game::player` and
    /// `Game::ride` (a `Player` has no ride of its own). It derefs to the
    /// player, so a test reads `p.pos` and `p.ride.speed` as it always did.
    pub struct Rider {
        pub p: Player,
        pub ride: Ride,
    }

    impl std::ops::Deref for Rider {
        type Target = Player;
        fn deref(&self) -> &Player {
            &self.p
        }
    }

    impl std::ops::DerefMut for Rider {
        fn deref_mut(&mut self) -> &mut Player {
            &mut self.p
        }
    }

    impl Rider {
        pub fn update(&mut self, input: &MoveInput, store: &Store, sea: f32, water: (f32, glam::Vec2), slopes: bool, dt: f32) {
            self.p.update(&mut self.ride, input, store, sea, water, slopes, dt);
        }
    }

    pub fn rider_at(x: f32, y: f32) -> Rider {
        // Yaw -90 degrees faces +X.
        let mut p = Player::new(Vec3::new(x, y, 0.0), -std::f32::consts::FRAC_PI_2);
        p.pitch = 0.0;
        p.body.on_ground = true;
        Rider { p, ride: Ride::default() }
    }

    pub fn run(p: &mut Rider, s: &Store, input: MoveInput, secs: f32, slopes: bool, mut each: impl FnMut(&Rider, f32)) {
        let dt = 1.0 / 60.0;
        let n = (secs / dt) as i32;
        for i in 0..n {
            p.update(&input, s, -100.0, (-100.0, glam::Vec2::ZERO), slopes, dt);
            each(p, i as f32 * dt);
        }
    }

    pub fn mount(p: &mut Rider) {
        let Rider { p: body, ride } = p;
        ride.toggle_bike(body);
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    /// The bike rolls up to cruising speed on the flat by itself, and W
    /// pedals it faster.
    #[test]
    fn rolls_up_to_speed() {
        let s = land(|_| 20.0);
        let mut p = rider_at(0.0, 20.0);
        mount(&mut p);
        run(&mut p, &s, MoveInput::default(), 6.0, false, |_, _| {});
        let v = p.ride.speed(&p);
        assert!((8.0..10.0).contains(&v), "coasting speed {v}");
        assert!(!p.ride.air, "flat ground: should stay down");
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 6.0, false, |_, _| {});
        let w = p.ride.speed(&p);
        assert!(w > v + 1.5 && w < 12.6, "pedalling speed {w}");
    }

    /// Coasting down a slope speeds up, and a lip at the bottom of a ramp
    /// throws the bike into the air.
    #[test]
    fn ramp_lip_is_a_jump() {
        // Flat until x = 40, a 30% ramp up to x = 46, then flat at the top.
        let s = land(|x| {
            if x < 40.0 {
                20.0
            } else if x < 46.0 {
                20.0 + (x - 40.0) * 0.3
            } else {
                21.8 - 2.0
            }
        });
        for slopes in [false, true] {
            let mut p = rider_at(20.0, 20.0);
            mount(&mut p);
            p.ride.speed = 14.0;
            let mut max_air: f32 = 0.0;
            let mut max_h: f32 = 0.0;
            let mut air_x = None;
            run(&mut p, &s, MoveInput::default(), 4.0, slopes, |p, _| {
                if p.ride.air {
                    max_air = max_air.max(p.ride.air_time);
                    max_h = max_h.max(p.pos.y);
                    air_x.get_or_insert(p.pos.x);
                }
            });
            assert!(max_air > 0.4, "slopes {slopes}: air time {max_air}");
            assert!(max_h > 21.5, "slopes {slopes}: peak {max_h}");
            let ax = air_x.unwrap();
            assert!(ax > 44.0 && ax < 48.0, "slopes {slopes}: took off at {ax}");
            assert!(!p.ride.air, "landed");
        }
    }

    /// Holding Space up the ramp and letting go just after the lip jumps
    /// higher than rolling off it.
    #[test]
    fn popping_at_the_lip_jumps_higher() {
        let s = land(|x| {
            if x < 40.0 {
                20.0
            } else if x < 46.0 {
                20.0 + (x - 40.0) * 0.3
            } else {
                19.8
            }
        });
        let peak = |pop: bool| {
            let mut p = rider_at(20.0, 20.0);
            mount(&mut p);
            p.ride.speed = 12.0;
            let mut peak: f32 = 0.0;
            let dt = 1.0 / 60.0;
            let mut released = false;
            for _ in 0..240 {
                // Hold from the foot of the ramp; let go a few ticks after leaving it.
                let hold = pop && p.pos.x > 38.0 && !released && !(p.ride.air && p.ride.air_time > 0.05);
                if pop && p.ride.air && p.ride.air_time > 0.05 {
                    released = true;
                }
                p.update(&MoveInput { jump: hold, ..Default::default() }, &s, -100.0, (-100.0, glam::Vec2::ZERO), true, dt);
                peak = peak.max(p.pos.y);
            }
            peak
        };
        let (roll, pop) = (peak(false), peak(true));
        assert!(pop > roll + 1.0, "pop peaked at {pop}, rolling off at {roll}");
    }

    /// Rolling down a long gentle hill doesn't leave the ground, and gravity
    /// speeds the bike up without running away with it.
    #[test]
    fn stays_down_on_a_smooth_hill() {
        let s = land(|x| 40.0 - x * 0.15);
        for slopes in [false, true] {
            let mut p = rider_at(0.0, 40.0);
            mount(&mut p);
            let mut airs = 0;
            run(&mut p, &s, MoveInput::default(), 6.0, slopes, |p, _| {
                if p.ride.air && p.ride.air_time > 0.1 {
                    airs += 1;
                }
            });
            assert_eq!(airs, 0, "slopes {slopes}");
            let v = p.ride.speed(&p);
            assert!(v > 10.0 && v < 16.0, "slopes {slopes}: 15% hill speed {v}");
        }
    }

    /// A steep descent levels off near 70 km/h rather than running away.
    #[test]
    fn steep_descent_is_held() {
        let s = land(|x| 120.0 - (x + 30.0) * 0.4);
        let mut p = rider_at(-28.0, 119.0);
        mount(&mut p);
        let mut top: f32 = 0.0;
        run(&mut p, &s, MoveInput::default(), 10.0, true, |p, _| top = top.max(p.ride.speed));
        assert!(top < 22.0, "top speed {top} m/s");
        assert!(top > 15.0, "top speed {top} m/s");
    }

    /// Shift sprints in a short burst that empties the meter, then needs a refill.
    #[test]
    fn sprint_is_a_burst() {
        let s = land(|_| 20.0);
        let mut p = rider_at(-30.0, 20.0);
        mount(&mut p);
        p.ride.speed = 12.0;
        let sprint = MoveInput {
            forward: true,
            sprint: true,
            ..Default::default()
        };
        run(&mut p, &s, sprint, 1.0, true, |_, _| {});
        let fast = p.ride.speed;
        assert!(fast > 18.0, "sprinted to {fast}");
        run(&mut p, &s, sprint, 1.0, true, |_, _| {});
        assert!(!p.ride.boosting, "meter should be empty");
        assert!(p.ride.boost_meter().unwrap().0 < 0.05);
        // Holding Shift on an empty meter does nothing; letting go refills it.
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 4.0, true, |_, _| {});
        assert!(p.ride.boost_meter().unwrap().0 > 0.95);
    }

    /// Tucking (Ctrl) down a hill is faster than sitting up, and it stores a
    /// charge while it does.
    #[test]
    fn tucking_downhill_is_faster() {
        let s = land(|x| 60.0 - x * 0.2);
        let ride = |tuck: bool| {
            let mut p = rider_at(0.0, 60.0);
            mount(&mut p);
            run(&mut p, &s, MoveInput { down: tuck, ..Default::default() }, 7.0, true, |_, _| {});
            (p.ride.speed, p.ride.charge)
        };
        let (sat, _) = ride(false);
        let (tucked, charge) = ride(true);
        assert!(tucked > sat + 1.5, "tucked {tucked} m/s against {sat} sitting up");
        assert!(charge > 0.9, "a long tucked descent should fill the charge, got {charge}");
    }

    /// The charge a tuck stored buys a higher, longer jump.
    #[test]
    fn a_stored_tuck_floats_the_jump() {
        let s = land(|_| 20.0);
        let jump = |charge: f32| {
            let mut p = rider_at(0.0, 20.0);
            mount(&mut p);
            p.ride.speed = 11.0;
            p.ride.charge = charge;
            let dt = 1.0 / 60.0;
            let (mut peak, mut air) = (20.0f32, 0.0f32);
            for i in 0..180 {
                let hold = i < 30;
                p.update(&MoveInput { jump: hold, ..Default::default() }, &s, -100.0, (-100.0, glam::Vec2::ZERO), true, dt);
                peak = peak.max(p.pos.y);
                air = air.max(p.ride.air_time);
            }
            (peak - 20.0, air)
        };
        let (plain_h, plain_t) = jump(0.0);
        let (float_h, float_t) = jump(1.0);
        assert!(float_h > plain_h + 0.4, "floated {float_h} m high against {plain_h} m");
        assert!(float_t > plain_t * 1.15, "floated {float_t} s against {plain_t} s");
    }

    /// A wall stops the bike.
    #[test]
    fn wall_stops_the_bike() {
        let s = land(|x| if x < 20.0 { 20.0 } else { 24.0 });
        let mut p = rider_at(0.0, 20.0);
        mount(&mut p);
        p.ride.speed = 10.0;
        run(&mut p, &s, MoveInput::default(), 4.0, false, |_, _| {});
        assert!(p.pos.x < 20.0, "x = {}", p.pos.x);
        // It keeps rolling into the wall on its own, but never gets going.
        assert!(p.ride.speed.abs() < 6.0, "speed {}", p.ride.speed);
    }

    /// Mouse turns arrive in bursts (here one tick in three); the lean
    /// should still move smoothly rather than flicker with them.
    #[test]
    fn lean_is_smooth_under_bursty_steering() {
        let s = land(|_| 20.0);
        let mut p = rider_at(-20.0, 20.0);
        mount(&mut p);
        p.ride.speed = 10.0;
        let dt = 1.0 / 60.0;
        // Once the turn is steady, the lean should hold rather than rock: count
        // the ticks where its change reverses direction by a visible amount.
        let (mut prev, mut prev_d, mut rocks, mut peak) = (0.0f32, 0.0f32, 0, 0.0f32);
        for i in 0..180 {
            if i % 3 == 0 {
                p.yaw += 0.045;
            }
            p.update(&MoveInput { forward: true, ..Default::default() }, &s, -100.0, (-100.0, glam::Vec2::ZERO), true, dt);
            let d = p.ride.lean - prev;
            if i > 90 {
                peak = peak.max(p.ride.lean.abs());
                if d * prev_d < 0.0 && (d - prev_d).abs() > 0.002 {
                    rocks += 1;
                }
            }
            prev = p.ride.lean;
            prev_d = d;
        }
        assert!(peak > 0.2, "should lean into the turn, peak {peak}");
        assert!(rocks == 0, "lean rocked {rocks} times in a steady turn");
    }

    #[test]
    fn ride_models_mesh() {
        for m in models() {
            assert!(!m.mesh.quads.is_empty());
        }
    }
}
