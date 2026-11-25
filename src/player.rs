//! First-person movement: walk, sprint, jump with coyote time and buffering,
//! swim, and a no-clip free camera (F4). Bike, gliding, climbing and the rope
//! are in ride.rs.
//!
//! An arena world (`arena`, which the shooter's rules set) moves on its own
//! terms: the running pace is the old sprint, Shift is a boost over it out of
//! a meter that empties and refills, a double-tapped direction key is a
//! dodge, and there are three jumps in one airtime rather than one.

use crate::physics::{drawn_ground_lift, free_body, hits_wet, move_body_with, snap_down, Body};

pub mod dive;
use crate::ride::Ride;
use crate::world::store::Store;
use glam::Vec3;

pub const EYE: f32 = 1.62;
/// Standing height of the body, metres, and crouched.
pub const STAND_H: f32 = 1.75;
const CROUCH_H: f32 = 1.05;
/// How far the eye drops into a full crouch, metres.
const CROUCH_DROP: f32 = 0.62;
/// Seconds to go all the way down, and back up.
const CROUCH_RATE: f32 = 6.0;
/// Seconds for A/D's view turn while riding to come most of the way up to
/// speed, or down from it (`Player::turn_by_keys`).
const KEY_TURN_EASE: f32 = 0.12;
/// How hard the keys push a body in the air, m/s squared, against 60 on the
/// ground.
const AIR_ACCEL: f32 = 16.0;
const JUMP_V: f32 = 7.6;
/// How hard a jump taken in mid-air pushes, as a share of one off the
/// ground. Under 1, so the second one reads as a kick and not a second
/// launch, and two of them do not clear a wall a rocket jump is for.
const AIR_JUMP: f32 = 0.86;
const WALK: f32 = 5.0;
const SPRINT: f32 = 8.3;
/// Arena running: the pace that used to need Shift held is the pace you go
/// at, and Shift is this much faster again.
const BOOST: f32 = 1.15;
/// A full boost lasts this long held down...
const BOOST_SECS: f32 = 1.6;
/// ...and refills in this long, which is the bike's bargain (ride.rs).
const BOOST_REFILL: f32 = 4.5;
/// Two presses of the same direction key inside this long are a dodge.
const DOUBLE_TAP: f32 = 0.26;
/// How fast a dodge throws the body that way, m/s, and the lift that takes
/// it off the floor so the shove is not scrubbed off by friction.
const DODGE_V: f32 = 12.2;
const DODGE_LIFT: f32 = 2.4;
/// How long after a dodge the jump key turns it into a dodge-jump.
const DODGE_JUMP_S: f32 = 0.34;
/// What a dodge-jump lifts: more than a standing jump, because the whole
/// point of it is that the two together clear what neither does alone.
const DODGE_JUMP_V: f32 = 9.1;
/// How long the eye takes to catch up with a step up or down, in seconds.
const EYE_SMOOTH: f32 = 0.09;
/// The eye never trails the body by more than this, in metres.
const EYE_LAG_MAX: f32 = 0.7;

#[derive(Default, Clone, Copy)]
pub struct MoveInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sprint: bool,
    pub down: bool,
    /// Throw or drop the rope (Z, or a mouse side button).
    pub hook: bool,
    /// Look around without steering while gliding (Alt).
    pub look: bool,
    /// Q and E held: the camera's manual focus, nearer and further.
    pub focus_near: bool,
    pub focus_far: bool,
    /// U held: in the car, a tap gets out and a hold resets it.
    pub vehicle: bool,
    /// The controller's left stick, x right and y forward, 0..1 past its
    /// dead zone (src/pad.rs). Zero without one. The four direction keys
    /// above are also set from it, for whatever only reads keys.
    pub stick: glam::Vec2,
    /// The controller's triggers, 0..1: right is the throttle, left the brake.
    pub throttle: f32,
    pub brake: f32,
    /// The controller is what is being played with: the last thing pressed
    /// or moved was on it, not the keyboard.
    pub pad: bool,
}

impl MoveInput {
    /// Right minus left, -1..1: the stick when it is pushed, else the keys.
    pub fn side(&self) -> f32 {
        if self.stick.x != 0.0 {
            self.stick.x
        } else {
            self.right as i32 as f32 - self.left as i32 as f32
        }
    }

    /// Forward minus back, -1..1, the same way.
    pub fn ahead(&self) -> f32 {
        if self.stick.y != 0.0 {
            self.stick.y
        } else {
            self.forward as i32 as f32 - self.back as i32 as f32
        }
    }
}

pub struct Player {
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub fly: bool,
    /// Eye height relative to the body: the drawn ground's lift (see
    /// physics::drawn_ground_lift) plus a spring that smooths steps.
    pub eye_offset: f32,
    prev_eye_offset: f32,
    eye_vel: f32,
    /// How fast A/D are turning the view while riding, radians a second:
    /// eased toward the key's rate, so a press winds the turn up and a
    /// release winds it down instead of starting and stopping dead.
    key_turn_v: f32,
    lift: f32,
    pub swimming: bool,
    /// Seconds left of a haul-out: the push that carries a swimmer over the
    /// rim of a pool (`haul_out`). While it runs the body is ballistic, so
    /// the float that holds a swimmer at the surface does not pull it back.
    haul: f32,
    /// 0 standing, 1 all the way down (Ctrl on foot). It lowers the eye, the
    /// collision box and the walking speed.
    pub crouch: f32,
    prev_crouch: f32,
    coyote: f32,
    jump_buf: f32,
    jump_was: bool,
    /// Jumps taken since the feet last left the ground. The first is the
    /// ground jump; any after it are air jumps, and only an arena world
    /// allows them (`air_jumps`).
    jumps: u32,
    /// How many jumps are allowed in one airtime. 1 on foot in a world;
    /// 3 wherever the shooter's rules apply, which is the triple jump.
    pub air_jumps: u32,
    /// The arena's movement rules: the boost instead of the sprint, dodges,
    /// and more than one jump. Set off the world's modes (`update`) for the
    /// player, and by the shooter for a bot, which walks and never rides.
    pub arena: bool,
    /// A pace of its own without Shift, metres a second, in place of the
    /// 5 m/s walk: a zombie's shamble, between a walk and a sprint. None for
    /// everybody else.
    pub walk_speed: Option<f32>,
    /// What share of its pace the body keeps, 1 for all of it: the zombie
    /// round slows somebody a zombie has hold of (`zombies::GRAB_S`).
    pub slowed: f32,
    /// What share of its pace the ground lets it keep, 1 for all of it: a
    /// game's `terrain_pace` (`crate::tune`), so 1 everywhere else.
    pub ground_pace: f32,
    /// The boost meter: 0 full, 1 empty.
    spent: f32,
    /// Emptied it: no boost until Shift is let go.
    boost_lock: bool,
    /// Boosting this step, for the HUD and for whoever draws the body.
    pub boosting: bool,
    /// Seconds since each direction key was last pressed, and whether it was
    /// down last step: forward, back, left, right. A second press inside
    /// `DOUBLE_TAP` is a dodge that way.
    tap_t: [f32; 4],
    tap_was: [bool; 4],
    /// Seconds left in which the jump key makes a dodge-jump of the dodge
    /// just taken.
    dodge_jump: f32,
    /// Scuba: under the water and swimming free (player/dive.rs).
    pub dive: dive::Dive,
    /// The body's height standing, metres: `STAND_H`, or less for a body
    /// that goes about low (a zombie crawler, zombies.rs).
    pub stand_h: f32,
}

impl Player {
    pub fn new(pos: Vec3, yaw: f32) -> Self {
        Self {
            pos,
            prev_pos: pos,
            body: Body::new(0.3, STAND_H),
            yaw,
            pitch: -0.08,
            fly: false,
            eye_offset: 0.0,
            prev_eye_offset: 0.0,
            eye_vel: 0.0,
            key_turn_v: 0.0,
            lift: 0.0,
            swimming: false,
            haul: 0.0,
            crouch: 0.0,
            prev_crouch: 0.0,
            coyote: 0.0,
            jump_buf: 0.0,
            jump_was: false,
            jumps: 0,
            air_jumps: 1,
            arena: false,
            walk_speed: None,
            slowed: 1.0,
            ground_pace: 1.0,
            spent: 0.0,
            boost_lock: false,
            boosting: false,
            tap_t: [DOUBLE_TAP; 4],
            tap_was: [false; 4],
            dodge_jump: 0.0,
            dive: dive::Dive::default(),
            stand_h: STAND_H,
        }
    }

    /// The body's height `crouch` of the way down (0 standing, 1 crouched).
    fn crouched_h(&self, crouch: f32) -> f32 {
        self.stand_h - (self.stand_h - CROUCH_H.min(self.stand_h)) * crouch
    }

    /// Out of a crouch at once: riding and the free camera never crouch.
    fn stand_up(&mut self) {
        self.crouch = 0.0;
        self.prev_crouch = 0.0;
        self.body.height = self.stand_h;
    }

    /// Put the eye exactly where a camera that owns it says, with nothing
    /// smoothed: the EDIT page's turntable (src/editor/cam.rs) sets this
    /// every frame while the body is frozen, so a step spring or a half
    /// crouch left over from walking would only drift the view.
    pub fn look_from(&mut self, eye: Vec3, yaw: f32, pitch: f32) {
        self.pos = eye - Vec3::Y * EYE;
        self.prev_pos = self.pos;
        self.yaw = yaw;
        self.pitch = pitch;
        self.eye_offset = 0.0;
        self.prev_eye_offset = 0.0;
        self.eye_vel = 0.0;
        self.crouch = 0.0;
        self.prev_crouch = 0.0;
    }

    /// A/D turning the view while riding, for one frame. View input belongs
    /// with the mouse rather than in the fixed step, so this is called once a
    /// frame with the frame's own dt (see `Ride::key_turn`).
    pub fn turn_by_keys(&mut self, ride: &Ride, input: &MoveInput, dt: f32) {
        let rate = ride.key_turn();
        if rate == 0.0 {
            self.key_turn_v = 0.0;
            return;
        }
        let keys = input.side();
        // A tenth of a second or so to reach the full rate, and as long to
        // stop: a held key turns as before, a tap nudges.
        let k = 1.0 - (-dt / KEY_TURN_EASE).exp();
        let v0 = self.key_turn_v;
        self.key_turn_v += (keys * rate - v0) * k;
        // The average rate over the frame, so the turn is the same at any fps.
        self.yaw -= 0.5 * (v0 + self.key_turn_v) * dt;
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp)
    }

    /// The walking (or flying) eye. A rider's eye is `Ride::head`, and
    /// `Game::eye` is the one that knows which of the two this frame wants.
    pub fn eye(&self, alpha: f32) -> Vec3 {
        let off = self.prev_eye_offset + (self.eye_offset - self.prev_eye_offset) * alpha;
        let crouch = self.prev_crouch + (self.crouch - self.prev_crouch) * alpha;
        // A body lower than a person's standing (a crawler) has its eye as
        // far under its top as a person's is.
        let low = STAND_H - self.stand_h;
        self.prev_pos.lerp(self.pos, alpha) + Vec3::new(0.0, EYE - low - CROUCH_DROP * crouch + off, 0.0)
    }

    /// `slopes`: the ground is drawn sloped, so the eye follows the drawn
    /// surface under the feet rather than the voxel the body stands on.
    /// `water`: the water surface over the player (a river's, else `sea`)
    /// and its current, which carries a swimmer and drags at a wader.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, ride: &mut Ride, input: &MoveInput, store: &Store, sea: f32, water: (f32, glam::Vec2), slopes: bool, dt: f32) {
        let (level, current) = water;
        // A world with an arena in it moves on the arena's terms: three
        // jumps in one airtime, dodges, and the boost in place of the
        // sprint. The walking worlds do not, where a double jump would undo
        // every climb and drop the terrain is laid out for. A bot steps with
        // `walk` and is given its own `arena` by the shooter.
        self.arena = ride.allowed.has(crate::modes::ModeId::Shooter);
        self.air_jumps = if self.arena { 3 } else { 1 };
        self.begin_step();
        if self.fly {
            if ride.mode != crate::ride::Mode::Foot || ride.rope_out {
                ride.drop_everything();
            }
        } else {
            ride.current = current;
            // Rides end in water: a river's surface counts as the sea for them.
            if ride.step(self, input, store, level.max(sea), slopes, dt) {
                // The walking eye picks up where the ride leaves it.
                self.stand_up();
                self.eye_offset = 0.0;
                self.prev_eye_offset = 0.0;
                self.eye_vel = 0.0;
                self.lift = 0.0;
                self.jump_was = input.jump;
                self.coyote = 0.0;
                self.jump_buf = 0.0;
                return;
            }
        }
        self.on_foot(input, store, water, slopes, dt);
    }

    /// Where the body was at the start of this step, for interpolating the
    /// drawn frame between steps. Stamped once a step, before anything moves
    /// it -- a ride included, since a ride that ends mid-step leaves the body
    /// somewhere new and the frame still has to be drawn from where it began.
    fn begin_step(&mut self) {
        self.prev_pos = self.pos;
        self.prev_eye_offset = self.eye_offset;
        self.prev_crouch = self.crouch;
    }

    /// One step on foot (or in the free camera), with nothing ridden. A
    /// shooter's bots walk and never ride, so this is what they step with.
    /// There is no `sea` here: only a ride ends in it, and the water over
    /// the body is `water.0`.
    #[allow(clippy::too_many_arguments)]
    pub fn walk(&mut self, input: &MoveInput, store: &Store, water: (f32, glam::Vec2), slopes: bool, dt: f32) {
        self.begin_step();
        self.on_foot(input, store, water, slopes, dt);
    }

    /// The walking step itself, with `begin_step` already done.
    #[allow(clippy::too_many_arguments)]
    fn on_foot(&mut self, input: &MoveInput, store: &Store, water: (f32, glam::Vec2), slopes: bool, dt: f32) {
        let (level, current) = water;
        let (sy, cy) = self.yaw.sin_cos();
        let fwd = Vec3::new(-sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, -sy);
        let (f, r) = (input.ahead(), input.side());
        // A stick pushed part way walks slower; keys are all or nothing.
        let wish = (fwd * f + right * r).clamp_length_max(1.0);
        let wish = if input.stick == glam::Vec2::ZERO { wish.normalize_or_zero() } else { wish };
        let dodge = self.double_tap([input.forward, input.back, input.left, input.right], dt);
        if self.fly {
            self.stand_up();
            let speed = if input.sprint { 60.0 } else { 18.0 };
            let up = input.jump as i32 as f32 - input.down as i32 as f32;
            let target = wish * speed + Vec3::Y * up * speed * 0.7;
            self.body.vel = self.body.vel.lerp(target, (dt * 10.0).min(1.0));
            self.pos += self.body.vel * dt;
            self.eye_offset = 0.0;
            self.prev_eye_offset = 0.0;
            self.eye_vel = 0.0;
            self.lift = 0.0;
            return;
        }

        // Under the water and swimming free: scuba moves the body instead.
        if self.dive_step(input, store, level, current, dt) {
            return;
        }
        self.swimming = self.pos.y + 1.0 < level;
        // Crouching (Ctrl): down at once, up only where there is headroom, so
        // you can't stand up inside a one-metre gap. Under water Ctrl dives
        // instead.
        let want = if input.down && !self.swimming { 1.0 } else { 0.0 };
        if want > self.crouch {
            self.crouch = (self.crouch + CROUCH_RATE * dt).min(1.0);
        } else if want < self.crouch {
            let up = (self.crouch - CROUCH_RATE * dt).max(0.0);
            let h = self.crouched_h(up);
            if !hits_wet(store, self.pos, self.body.half_w, h) {
                self.crouch = up;
            }
        }
        self.body.height = self.crouched_h(self.crouch);
        // Wading: water over the knees slows the walk.
        let wading = !self.swimming && self.pos.y + 0.45 < level;
        let mut speed = self.pace(input, dt);
        if self.swimming {
            speed *= 0.6;
        } else if wading {
            speed *= 0.65;
        }
        speed *= 1.0 - 0.58 * self.crouch;
        // The water moves the body: all of the current for a swimmer, a
        // little for a wader.
        let drift = Vec3::new(current.x, 0.0, current.y)
            * if self.swimming {
                1.0
            } else if wading {
                0.25
            } else {
                0.0
            };
        let accel = if self.body.on_ground {
            60.0
        } else if self.swimming {
            14.0
        } else {
            AIR_ACCEL
        };
        let v = Vec3::new(self.body.vel.x, 0.0, self.body.vel.z);
        let gravity = store.gravity;
        if gravity.low() && !self.body.on_ground && !self.swimming {
            // Afloat in low gravity: a jump hangs for seconds, so the air is
            // somewhere you steer. The keys turn the body toward where they
            // point however fast it is going, without taking off any speed
            // it already has that way; with no key held it drifts on as it
            // was thrown rather than stopping dead over a drop.
            if wish != Vec3::ZERO {
                let dv = wish * speed.max(v.dot(wish)) - v;
                let max = accel * dt;
                let dv = if dv.length() > max { dv.normalize() * max } else { dv };
                self.body.vel.x += dv.x;
                self.body.vel.z += dv.z;
            }
        } else if !self.body.on_ground && !self.swimming && v.length() > SPRINT + 0.5 {
            // Flying along with momentum (off the rope, a wall, the bike): keep
            // it. Keys only add speed toward where they point.
            let add = (speed - v.dot(wish)).clamp(0.0, accel * dt);
            let k = 1.0 - (0.15 * dt).min(1.0);
            self.body.vel.x = self.body.vel.x * k + wish.x * add;
            self.body.vel.z = self.body.vel.z * k + wish.z * add;
        } else {
            let dv = wish * speed + drift - v;
            let max = accel * dt;
            let dv = if dv.length() > max { dv.normalize() * max } else { dv };
            self.body.vel.x += dv.x;
            self.body.vel.z += dv.z;
        }

        if self.swimming && self.haul > 0.0 {
            // Hauling out: the water has let go of the body.
            self.haul -= dt;
            self.body.vel += gravity.pull(self.pos) * dt;
            self.body.vel.y = self.body.vel.y.max(-50.0);
        } else if self.swimming {
            self.haul = 0.0;
            if input.down {
                // Diving.
                self.body.vel.y -= 5.0 * dt;
            } else {
                // Afloat: the eyes ride just above the surface.
                let float_y = level - EYE + 0.15;
                self.body.vel.y += ((float_y - self.pos.y) * 4.0 - self.body.vel.y) * (3.0 * dt).min(1.0);
            }
            self.body.vel.y *= 1.0 - (2.5 * dt).min(1.0);
            if input.jump {
                self.body.vel.y = self.body.vel.y.max(3.2);
            }
        } else {
            self.haul = 0.0;
            self.body.vel += gravity.pull(self.pos) * dt;
            self.body.vel.y = self.body.vel.y.max(-50.0);
        }

        // The dodge: a double-tapped direction key throws the body that way,
        // hard enough that the air-control branch above keeps it rather than
        // steering out of it. Off the ground only, and it is not a jump --
        // it spends none of them.
        if let Some(i) = dodge {
            if self.arena && !self.swimming && (self.body.on_ground || self.coyote > 0.0) {
                let d = [fwd, -fwd, -right, right][i];
                self.body.vel.x = d.x * DODGE_V;
                self.body.vel.z = d.z * DODGE_V;
                self.body.vel.y = self.body.vel.y.max(DODGE_LIFT);
                self.body.on_ground = false;
                self.dodge_jump = DODGE_JUMP_S;
            }
        }

        // Jump: buffered a little before landing, allowed a little after leaving an edge.
        if self.body.on_ground {
            self.coyote = 0.1;
        } else {
            self.coyote -= dt;
        }
        // An air jump has to be a fresh press, or holding the key through a
        // jump spends every one of them in the same breath.
        let pressed = input.jump && !self.jump_was;
        if pressed || (input.jump && self.body.on_ground) {
            self.jump_buf = 0.12;
        } else {
            self.jump_buf -= dt;
        }
        self.jump_was = input.jump;
        let was_grounded = self.body.on_ground;
        if self.body.on_ground {
            self.jumps = 0;
        }
        let mut jumped = false;
        if self.dodge_jump > 0.0 && pressed && !self.swimming && !self.fly {
            // The dodge-jump: jump inside a moment of a dodge and it lifts
            // more than a standing jump while the dodge's run carries on.
            // It stands in for the ground jump rather than coming on top of
            // it, so the airtime is still three jumps all told.
            jumped = true;
            self.jumps = self.jumps.max(1);
            self.body.vel.y = self.body.vel.y.max(0.0).max(DODGE_JUMP_V);
            self.dodge_jump = 0.0;
            self.jump_buf = 0.0;
            self.coyote = 0.0;
        } else if self.jump_buf > 0.0 && self.coyote > 0.0 && !self.swimming {
            jumped = true;
            self.jumps = 1;
            self.body.vel.y = JUMP_V;
            self.coyote = 0.0;
            self.jump_buf = 0.0;
            self.body.on_ground = false;
        } else if pressed && !self.swimming && !self.fly && self.jumps < self.air_jumps.max(1) && self.coyote <= 0.0 {
            // The double jump: off nothing, mid-air. It sets the climb
            // rather than adding to it, so falling fast and jumping still
            // gets you up, and rising fast gains nothing by it.
            jumped = true;
            self.jumps = self.jumps.max(1) + 1;
            self.body.vel.y = self.body.vel.y.max(0.0).max(JUMP_V * AIR_JUMP);
            self.jump_buf = 0.0;
        }

        // Something may have left the body inside the world: a ride that
        // ended in rock, an old save, terrain generated around it. Collision
        // blocks every move out of solid rock, so the body would be stuck
        // where it stands; step it clear first.
        let foot = ((self.pos.y - 0.1) * 4.0).floor() as i32;
        if store.loaded((self.pos.x * 4.0).floor() as i32, foot, (self.pos.z * 4.0).floor() as i32) {
            free_body(store, &mut self.pos, self.body.half_w, self.body.height, 2.0, hits_wet);
        }

        let delta = self.body.vel * dt;
        let res = move_body_with(store, &mut self.pos, &mut self.body, delta, 0.5, hits_wet);
        // Swimming into the side of a pool: haul out over the rim.
        if self.swimming && res.blocked && self.haul <= 0.0 {
            self.haul_out(store, wish, level);
        }
        // Walking off a small ledge drops straight onto the ground below rather
        // than falling, so going downhill is as smooth as going up.
        let mut dropped = 0.0;
        if was_grounded && !jumped && !self.body.on_ground && !self.swimming && self.body.vel.y <= 0.0 {
            dropped = snap_down(store, &mut self.pos, &mut self.body, 0.5, hits_wet);
        }
        // The body moves in whole voxels. On a tick where it steps, the eye keeps
        // its height and springs after it, so steps read as a slope. Where the
        // ground is drawn sloped the eye rides the drawn ground (the lift),
        // which already continues through a step, so the spring has almost
        // nothing to do there.
        let lift = if slopes && !self.swimming { drawn_ground_lift(store, self.pos) } else { 0.0 };
        let mut x = self.eye_offset - self.lift;
        let step = res.stepped - dropped;
        if step != 0.0 {
            x -= step + (lift - self.lift);
        }
        let (x, v) = smooth_damp(x.clamp(-EYE_LAG_MAX, EYE_LAG_MAX), self.eye_vel, EYE_SMOOTH, dt);
        let (x, v) = if x.abs() < 1e-4 && v.abs() < 1e-3 { (0.0, 0.0) } else { (x, v) };
        self.eye_vel = v;
        self.lift = lift;
        self.eye_offset = x + lift;
    }

    /// How fast the body walks this step, and the boost meter behind it.
    ///
    /// Outside an arena this is the sprint it always was: Shift for 8.3 m/s
    /// and 5 without. In an arena 8.3 is the pace you go at with nothing
    /// held, and Shift is 15% over that out of a meter that empties in
    /// `BOOST_SECS` and refills in `BOOST_REFILL` -- the bike's bargain,
    /// which is what makes the boost a decision rather than a key to hold.
    fn pace(&mut self, input: &MoveInput, dt: f32) -> f32 {
        if !self.arena {
            self.boosting = false;
            return self.slowed * self.ground_pace * if input.sprint { SPRINT } else { self.walk_speed.unwrap_or(WALK) };
        }
        if !input.sprint {
            self.boost_lock = false;
        }
        // A boost may not be started on the dregs of the meter, or tapping
        // Shift gives a stutter of them; one already running may spend them.
        let start_ok = self.boosting || self.spent < 0.85;
        self.boosting = input.sprint && !self.boost_lock && start_ok && !self.swimming;
        if self.boosting {
            self.spent += dt / BOOST_SECS;
            if self.spent >= 1.0 {
                self.spent = 1.0;
                self.boost_lock = true;
                self.boosting = false;
            }
        } else if !input.sprint {
            // Only once the key is let go, so a held Shift on an empty
            // meter does not top it up a sliver at a time and stutter.
            self.spent = (self.spent - dt / BOOST_REFILL).max(0.0);
        }
        self.slowed * self.ground_pace * SPRINT * if self.boosting { BOOST } else { 1.0 }
    }

    /// The boost meter: how much is left and whether it is being spent, or
    /// `None` where there is no boost to show. The shape `Ride::boost_meter`
    /// has, because the HUD draws it the same way.
    pub fn boost_meter(&self) -> Option<(f32, bool)> {
        self.arena.then_some((1.0 - self.spent, self.boosting))
    }

    /// Which direction key was just double-tapped, as an index into
    /// forward, back, left, right. A press counts only on the edge, and only
    /// with a release behind it, so holding a key is never a dodge.
    fn double_tap(&mut self, keys: [bool; 4], dt: f32) -> Option<usize> {
        self.dodge_jump = (self.dodge_jump - dt).max(0.0);
        let mut hit = None;
        for (i, down) in keys.into_iter().enumerate() {
            self.tap_t[i] += dt;
            if down && !self.tap_was[i] {
                if self.tap_t[i] < DOUBLE_TAP {
                    hit = Some(i);
                }
                self.tap_t[i] = 0.0;
            }
            self.tap_was[i] = down;
        }
        hit
    }

    /// A swimmer pressed against the side of a pool: climb out of it.
    ///
    /// The pools an imported map fills are tanks with vertical sides, and a
    /// swimmer floats with the eyes at the waterline, which leaves the feet
    /// a body's length below the rim. Walking forward does nothing there --
    /// the wall is in the way all the way up -- so a swimmer would be stuck
    /// in the water. This looks for somewhere to stand just past the wall,
    /// at the surface or a little above it, and pushes off hard enough to
    /// land on it: the one move that gets a body out of water it is level
    /// with. Nothing happens when there is nowhere to climb to.
    fn haul_out(&mut self, store: &Store, wish: Vec3, level: f32) {
        if wish == Vec3::ZERO {
            return;
        }
        let ahead = self.pos + wish * (self.body.half_w + 0.3);
        for k in 0..=HAUL_RISE {
            let y = level + k as f32 * 0.25;
            let foot = Vec3::new(ahead.x, y, ahead.z);
            // Room for the body, with something under its feet to stand on.
            if hits_wet(store, foot, self.body.half_w, self.body.height) {
                continue;
            }
            if !hits_wet(store, foot - Vec3::Y * 0.1, self.body.half_w, 0.05) {
                continue;
            }
            let rise = (y - self.pos.y).max(0.0) + 0.3;
            self.body.vel.y = (2.0 * store.gravity.pull(self.pos).length() * rise).sqrt();
            self.haul = 0.8;
            return;
        }
    }
}

/// How far above the water a haul-out will look for a floor, in quarter
/// metres: a step out of the water, not a climb.
const HAUL_RISE: i32 = 4;

/// Critically damped spring toward 0 (the "SmoothDamp" approximation from Game
/// Programming Gems 4): no overshoot, and velocity stays continuous when the
/// target jumps.
pub(crate) fn smooth_damp(x: f32, v: f32, time: f32, dt: f32) -> (f32, f32) {
    let omega = 2.0 / time;
    let k = omega * dt;
    let e = 1.0 / (1.0 + k + 0.48 * k * k + 0.235 * k * k * k);
    let temp = (v + omega * x) * dt;
    ((x + temp) * e, (v - omega * temp) * e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    /// A/D on the bike: held for a second it turns the same at 60 and 240
    /// fps, nearly the full 2 rad; a single frame's tap turns only a little;
    /// and let go, the turn winds down rather than stopping dead.
    #[test]
    fn key_turn_eases_in_and_out() {
        let mut ride = crate::ride::Ride::default();
        ride.mode = crate::ride::Mode::Bike;
        let left = MoveInput { left: true, ..Default::default() };
        let mut held = Vec::new();
        for fps in [60.0f32, 240.0] {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            let dt = 1.0 / fps;
            for _ in 0..fps as i32 {
                p.turn_by_keys(&ride, &left, dt);
            }
            let at_release = p.yaw;
            p.turn_by_keys(&ride, &MoveInput::default(), dt);
            assert!(p.yaw > at_release, "stopped dead on release");
            held.push(at_release);
        }
        assert!((held[0] - held[1]).abs() < 0.01, "fps changes the turn: {held:?}");
        assert!(held[0] > 1.7 && held[0] < 2.0, "a held second turned {}", held[0]);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.turn_by_keys(&ride, &left, 1.0 / 60.0);
        assert!(p.yaw < 0.5 * 2.0 / 60.0, "a tap jumped {}", p.yaw);
    }

    fn stairs() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cx in -1..=3 {
                for cy in 0..=3 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        // Ground top at y = 20 m at x = 0, dropping 0.25 m every 0.5 m of x.
        for x in -8..96 {
            let top = 80 - (x.max(0) / 2);
            for z in -8..8 {
                for y in 0..top {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    /// A ride in a walking world. `Ride::default()` allows every mode, the
    /// shooter's among them, and a world that allows the shooter moves on the
    /// arena's terms (`Player::update`): 8.3 m/s with nothing held, not 5.
    /// The eye's spring is tuned for the walk, so the tests that measure it
    /// have to be walking.
    fn walking_ride() -> crate::ride::Ride {
        let mut ride = crate::ride::Ride::default();
        ride.allowed = crate::modes::Allowed::all().without(crate::modes::ModeId::Shooter);
        ride
    }

    /// A flat floor to jump off.
    fn floor() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                for cy in 0..=3 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -16..16 {
            for z in -16..16 {
                for y in 0..40 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    /// How high the player gets off one press of jump, with `air_jumps`
    /// allowed and the key tapped `taps` times.
    fn apex(arena: bool, taps: usize) -> f32 {
        let s = floor();
        let mut ride = crate::ride::Ride::default();
        let mut p = Player::new(Vec3::new(0.0, 10.1, 0.0), 0.0);
        // The second jump is the arena's, so the world has to allow one.
        ride.allowed = if arena {
            crate::modes::Allowed::all()
        } else {
            crate::modes::Allowed::all().without(crate::modes::ModeId::Shooter)
        };
        p.body.on_ground = true;
        // Settle onto the floor first, so the jump leaves from the ground.
        for _ in 0..10 {
            p.update(&mut ride, &MoveInput::default(), &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
        let foot = p.pos.y;
        let mut high: f32 = 0.0;
        let mut done = 0;
        let mut held = false;
        for i in 0..160 {
            // A tap is one frame down, one frame up: an air jump is a fresh
            // press, so holding the key cannot spend them all at once.
            let jump = if done < taps && i % 18 == 0 && !held {
                done += 1;
                held = true;
                true
            } else {
                held = false;
                false
            };
            let input = MoveInput { jump, ..Default::default() };
            p.update(&mut ride, &input, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
            high = high.max(p.pos.y - foot);
        }
        high
    }

    /// A tank of water voxels with its surface flush with the floor round
    /// it, as an imported map's water zone is: three metres of water in a
    /// pit from x = 0 to x = 3 m.
    fn pool() -> (Store, u8) {
        let (water, _table) = crate::physics::tests::water_material();
        let mut s = Store::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                s.chunks.insert((cx, 0, cz), Voxels::Uniform(STONE));
                s.chunks.insert((cx, 1, cz), Voxels::Uniform(0));
                s.chunks.insert((cx, 2, cz), Voxels::Uniform(0));
            }
        }
        for z in -8..8 {
            for x in 0..12 {
                for y in 20..32 {
                    s.set(x, y, z, water);
                }
            }
        }
        (s, water)
    }

    /// Water held as voxels is water: the player sinks into the pool
    /// instead of walking on it, floats at its surface, and hauls out over
    /// the rim when pushed against the side.
    #[test]
    fn the_player_swims_in_a_voxel_pool_and_climbs_out_of_it() {
        let (s, _) = pool();
        let mut ride = crate::ride::Ride::default();
        // Yaw a quarter turn, so forward is -x: out of the pool.
        let mut p = Player::new(Vec3::new(1.5, 9.0, 0.0), std::f32::consts::FRAC_PI_2);
        let step = |p: &mut Player, ride: &mut crate::ride::Ride, input: &MoveInput| {
            let water = crate::game::water_at(None, 0.0, &s, p.pos);
            p.update(ride, input, &s, 0.0, water, false, 1.0 / 60.0);
        };
        // Dropped in: it falls through the surface and comes to rest afloat.
        for _ in 0..180 {
            step(&mut p, &mut ride, &MoveInput::default());
        }
        assert!(p.swimming, "it stood on the water instead of swimming in it: y = {}", p.pos.y);
        assert!((p.pos.y - (8.0 - EYE + 0.15)).abs() < 0.2, "it did not float at the surface: y = {}", p.pos.y);
        // Swimming into the side: it ends up out of the water, on the floor.
        let fwd = MoveInput { forward: true, ..Default::default() };
        for _ in 0..240 {
            step(&mut p, &mut ride, &fwd);
        }
        assert!(!p.swimming, "it is still in the water at {:?}", p.pos);
        assert!(p.pos.x < 0.0, "it did not get over the rim: {:?}", p.pos);
        assert!((p.pos.y - 8.0).abs() < 0.1, "it is not standing on the floor: {:?}", p.pos);
    }

    /// A second tap in mid-air lifts you again, and a third once more, and
    /// only where the world allows it: the arena worlds, not the walking
    /// ones. A fourth buys nothing.
    #[test]
    fn taps_in_the_air_are_a_triple_jump() {
        let one = apex(false, 2);
        let two = apex(true, 2);
        assert!(two > one * 1.4, "two taps reached {two} m, one reached {one} m");
        let three = apex(true, 3);
        assert!(three > two * 1.2, "three taps reached {three} m, two reached {two} m");
        let four = apex(true, 4);
        assert!((four - three).abs() < 0.05, "a fourth jump added {} m", four - three);
    }

    /// A world that says `"gravity": {"g": 0.3}` is one where a jump goes
    /// about three times as high, and where the keys turn a body round in
    /// the air that the engine's own gravity would leave flying on.
    #[test]
    fn low_gravity_jumps_higher_and_steers_in_the_air() {
        let jump = |g: f32| {
            let mut s = rink();
            s.gravity.g = g;
            let mut ride = walking_ride();
            let mut p = Player::new(Vec3::new(0.0, 8.1, 0.0), 0.0);
            run(&mut p, &mut ride, &s, &MoveInput::default(), 0.3);
            let foot = p.pos.y;
            let mut high: f32 = 0.0;
            for i in 0..600 {
                let input = MoveInput { jump: i == 0, ..Default::default() };
                p.update(&mut ride, &input, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
                high = high.max(p.pos.y - foot);
            }
            assert!(p.body.on_ground, "it never came back down at g = {g}");
            high
        };
        let (heavy, light) = (jump(1.0), jump(0.3));
        assert!(light > heavy * 2.8 && light < heavy * 3.8, "a jump reached {light} m at 0.3 g against {heavy} m");

        // Thrown north at dodge speed, well clear of the floor, with the
        // right key held for a second. The engine's own gravity adds the
        // sidestep and leaves the throw alone; low gravity turns the body.
        let thrown = |g: f32| {
            let mut s = rink();
            s.gravity.g = g;
            let mut ride = walking_ride();
            let mut p = Player::new(Vec3::new(0.0, 28.0, 0.0), 0.0);
            p.body.vel = Vec3::new(0.0, 0.0, -DODGE_V);
            let right = MoveInput { right: true, ..Default::default() };
            run(&mut p, &mut ride, &s, &right, 1.0);
            assert!(!p.body.on_ground);
            p.body.vel.z
        };
        assert!(thrown(1.0) < -SPRINT, "the engine's own gravity should keep the throw: {} m/s", thrown(1.0));
        assert!(thrown(0.3) > -1.0, "low gravity should have turned it: still {} m/s north", thrown(0.3));

        // With no key held, a floating body drifts on instead of stopping.
        let mut s = rink();
        s.gravity.g = 0.3;
        let mut ride = walking_ride();
        let mut p = Player::new(Vec3::new(0.0, 28.0, 0.0), 0.0);
        p.body.vel = Vec3::new(0.0, 0.0, -WALK);
        run(&mut p, &mut ride, &s, &MoveInput::default(), 1.0);
        assert!(p.body.vel.z < -WALK * 0.95, "it lost its drift: {} m/s", p.body.vel.z);
    }

    /// A hundred metres of solid floor with its top at y = 8, made of whole
    /// uniform chunks so it costs nothing to build. `floor()` is four metres
    /// across, which the arena's pace crosses in half a second: measuring a
    /// run on it measures a fall off the end of it.
    fn rink() -> Store {
        let mut s = Store::default();
        for cz in -12..=12 {
            for cx in -12..=12 {
                s.chunks.insert((cx, 0, cz), Voxels::Uniform(STONE));
                for cy in 1..=3 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        s
    }

    /// Settle a body on the arena's floor, ready to be driven a step at a
    /// time. The arena's rules come off the world's modes, as they do in a
    /// match.
    fn arena_body() -> (Store, crate::ride::Ride, Player) {
        let s = rink();
        let mut ride = crate::ride::Ride::default();
        ride.allowed = crate::modes::Allowed::all();
        let mut p = Player::new(Vec3::new(0.0, 8.1, 0.0), 0.0);
        p.body.on_ground = true;
        for _ in 0..20 {
            p.update(&mut ride, &MoveInput::default(), &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
        (s, ride, p)
    }

    fn run(p: &mut Player, ride: &mut crate::ride::Ride, s: &Store, input: &MoveInput, secs: f32) {
        for _ in 0..(secs * 60.0) as i32 {
            p.update(ride, input, s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
    }

    fn flat_speed(p: &Player) -> f32 {
        Vec3::new(p.body.vel.x, 0.0, p.body.vel.z).length()
    }

    /// In an arena, running with nothing held is the old sprint, Shift is
    /// 15% over that, and holding it empties the meter and gives it back.
    #[test]
    fn the_arena_runs_at_the_old_sprint_and_boosts_over_it() {
        let (s, mut ride, mut p) = arena_body();
        let fwd = MoveInput { forward: true, ..Default::default() };
        run(&mut p, &mut ride, &s, &fwd, 1.0);
        let walk = flat_speed(&p);
        assert!((walk - SPRINT).abs() < 0.3, "running came out at {walk} m/s, not {SPRINT}");
        let boost = MoveInput { sprint: true, ..fwd };
        run(&mut p, &mut ride, &s, &boost, 0.4);
        let fast = flat_speed(&p);
        assert!(fast > walk * 1.1, "the boost made {fast} m/s against {walk}");
        assert!(fast < walk * 1.2, "the boost made {fast} m/s, which is more than 15% over {walk}");
        // Held down, it runs the meter out and drops back to the pace.
        run(&mut p, &mut ride, &s, &boost, BOOST_SECS + 0.5);
        assert!(!p.boosting, "the meter should be empty");
        assert!(p.boost_meter().unwrap().0 < 0.05);
        assert!(flat_speed(&p) < walk * 1.05, "still boosting on an empty meter");
        // Let go, and it comes back.
        run(&mut p, &mut ride, &s, &fwd, BOOST_REFILL + 0.2);
        assert!(p.boost_meter().unwrap().0 > 0.95, "the meter did not refill");
    }

    /// Outside an arena nothing changed: Shift is the sprint, and there is
    /// no meter to show.
    #[test]
    fn a_walking_world_still_just_sprints() {
        let s = rink();
        let mut ride = crate::ride::Ride::default();
        ride.allowed = crate::modes::Allowed::all().without(crate::modes::ModeId::Shooter);
        let mut p = Player::new(Vec3::new(0.0, 8.1, 0.0), 0.0);
        p.body.on_ground = true;
        let fwd = MoveInput { forward: true, ..Default::default() };
        run(&mut p, &mut ride, &s, &fwd, 1.5);
        assert!((flat_speed(&p) - WALK).abs() < 0.3, "walked at {} m/s", flat_speed(&p));
        let sprint = MoveInput { sprint: true, ..fwd };
        run(&mut p, &mut ride, &s, &sprint, 4.0);
        assert!((flat_speed(&p) - SPRINT).abs() < 0.3, "sprinted at {} m/s, and for as long as it liked", flat_speed(&p));
        assert!(p.boost_meter().is_none(), "a walking world should have no boost meter");
    }

    /// A double-tapped direction key throws the body that way faster than
    /// it can run, and the jump key straight after lifts it further than a
    /// standing jump does. One tap does neither.
    #[test]
    fn a_double_tap_dodges_and_the_jump_after_it_goes_further() {
        // One tap: it just walks.
        let (s, mut ride, mut p) = arena_body();
        let right = MoveInput { right: true, ..Default::default() };
        run(&mut p, &mut ride, &s, &right, 0.3);
        assert!(flat_speed(&p) < SPRINT + 0.5, "one tap dodged: {} m/s", flat_speed(&p));

        // Tap, release, tap: a dodge.
        let (s, mut ride, mut p) = arena_body();
        let step = |p: &mut Player, ride: &mut crate::ride::Ride, input: &MoveInput| {
            p.update(ride, input, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        };
        step(&mut p, &mut ride, &right);
        step(&mut p, &mut ride, &MoveInput::default());
        step(&mut p, &mut ride, &right);
        let dodged = flat_speed(&p);
        assert!(dodged > SPRINT * 1.3, "the dodge only made {dodged} m/s");
        assert!(!p.body.on_ground, "the dodge left the body on the floor");
        let along = p.body.vel.x;
        assert!(along > SPRINT, "the dodge went {along} m/s to the right, which is not where the key pointed");

        // The jump key inside the window: higher than a standing jump.
        let foot = p.pos.y;
        let jump = MoveInput { jump: true, ..right };
        step(&mut p, &mut ride, &jump);
        assert!(p.body.vel.y > JUMP_V, "a dodge-jump lifted {} m/s, a standing jump lifts {JUMP_V}", p.body.vel.y);
        let mut high: f32 = 0.0;
        let idle = MoveInput::default();
        for i in 0..120 {
            step(&mut p, &mut ride, if i % 2 == 0 { &right } else { &idle });
            high = high.max(p.pos.y - foot);
        }
        assert!(high > 1.2, "the dodge-jump only reached {high} m");
    }

    /// A bot's tapper (`brain::steer::Tapper`) is a hand on the same keys:
    /// four body steps of it dodge the body exactly as a player's two taps
    /// do. This is the claim the whole bot design rests on, and a dodge is
    /// the one move where the body reads the keys rather than the keys
    /// saying what the body does, so it is worth pinning from this side.
    #[test]
    fn a_tapper_dodges_the_body_the_way_a_player_does() {
        let (s, mut ride, mut p) = arena_body();
        let mut tap = crate::brain::steer::Tapper::default();
        let walking = MoveInput { forward: true, ..Default::default() };
        // Walking forward first, so the dodge is not just the walk.
        run(&mut p, &mut ride, &s, &walking, 0.5);
        let before = p.body.vel.x;
        assert!(before.abs() < 1.0, "it was already going sideways at {before} m/s");
        let mut want = Some(crate::brain::steer::Dodge::Right);
        for _ in 0..4 {
            let keys = tap.step(want.take(), walking);
            p.update(&mut ride, &keys, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 30.0);
        }
        assert!(p.body.vel.x > SPRINT, "the tap moved the body {} m/s to the right", p.body.vel.x);
        assert!(!p.body.on_ground, "the dodge left the body on the floor");
    }

    /// Holding the key down through a jump does not spend the air jump: it
    /// has to be let go and pressed again.
    #[test]
    fn holding_jump_does_not_double_jump() {
        let s = floor();
        let mut ride = crate::ride::Ride::default();
        let mut p = Player::new(Vec3::new(0.0, 10.1, 0.0), 0.0);
        p.body.on_ground = true;
        for _ in 0..10 {
            p.update(&mut ride, &MoveInput::default(), &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
        let foot = p.pos.y;
        ride.allowed = crate::modes::Allowed::all();
        let held = MoveInput { jump: true, ..Default::default() };
        let mut high: f32 = 0.0;
        for _ in 0..90 {
            p.update(&mut ride, &held, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
            high = high.max(p.pos.y - foot);
        }
        assert!(high < apex(false, 2) * 1.3, "a held key climbed {high} m, which is more than one jump");
    }

    /// Walking east down a staircase (one voxel down every half metre) keeps
    /// the player on the ground, and the eye moves smoothly instead of in
    /// quarter-metre jumps.
    #[test]
    fn walks_down_stairs_smoothly() {
        let s = stairs();
        let mut ride = walking_ride();
        let mut p = Player::new(Vec3::new(0.1, 20.0, 0.0), -std::f32::consts::FRAC_PI_2);
        p.body.on_ground = true;
        let input = MoveInput { forward: true, ..Default::default() };
        let mut last_eye = p.eye(1.0).y;
        let mut max_step: f32 = 0.0;
        for i in 0..120 {
            p.update(&mut ride, &input, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
            let eye = p.eye(1.0).y;
            if i > 5 {
                assert!(p.body.on_ground, "airborne at tick {i}, x = {}", p.pos.x);
                max_step = max_step.max((eye - last_eye).abs());
            }
            last_eye = eye;
        }
        assert!(p.pos.x > 8.0, "x = {}", p.pos.x);
        assert!(max_step < 0.08, "eye moved {max_step} m in one tick");
    }

    /// With the stairs drawn as a slope, the eye stays at eye height above the
    /// drawn ground, not above the voxel steps.
    #[test]
    fn eye_follows_the_drawn_slope() {
        let s = stairs();
        let mut ride = walking_ride();
        let mut p = Player::new(Vec3::new(0.1, 20.0, 0.0), -std::f32::consts::FRAC_PI_2);
        p.body.on_ground = true;
        let input = MoveInput { forward: true, ..Default::default() };
        let (mut worst, mut max_step, mut last): (f32, f32, f32) = (0.0, 0.0, p.eye(1.0).y);
        for i in 0..120 {
            p.update(&mut ride, &input, &s, -100.0, (-100.0, glam::Vec2::ZERO), true, 1.0 / 60.0);
            let eye = p.eye(1.0).y;
            if i > 20 {
                let lift = drawn_ground_lift(&s, p.pos);
                worst = worst.max((eye - (p.pos.y + lift + EYE)).abs());
                max_step = max_step.max((eye - last).abs());
            }
            last = eye;
        }
        assert!(p.pos.x > 8.0, "x = {}", p.pos.x);
        assert!(worst < 0.06, "eye strayed {worst} m from the drawn ground");
        assert!(max_step < 0.06, "eye moved {max_step} m in one tick");
    }
}
