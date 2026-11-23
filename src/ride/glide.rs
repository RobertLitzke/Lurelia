//! Gliding: the one way to fly.
//!
//! An energy glider. Lift bends the path toward where the player aims without
//! adding speed; gravity and drag do the rest, so diving builds speed and
//! pulling up trades it for height. Drag is parasitic (grows with speed) plus
//! induced (grows as the wings work harder at low speed), which gives a best
//! glide of about 7:1 near 58 km/h and steeper glides either side of it.
//! The air itself rises in thermals over the land and where the wind runs up
//! a slope; rising air is applied as moving air, so a glider in a 4 m/s
//! thermal sinking 3 m/s through it climbs 1 m/s.
//!
//! F on the ground springs the player about 3.7 m straight up over a third of
//! a second and the wings open near the top, swinging the climb into forward
//! flight over half a second; F or Space in the air opens them; F again folds
//! them.
//!
//! A D roll the body; the lift tilts with the roll and the turn comes out of
//! the bank, so turns have to be set up and a hard bank costs the pull-up.
//! A and D together throw a barrel roll. What the flyer collides with is a
//! prone body round the eye rather than the standing box, leaves are flown
//! through, and a shallow contact at speed is a graze that scrubs speed
//! instead of a crash that ends the flight. soar.rs scores all of it.

use super::*;

const G: f32 = 9.8;
/// Drag per unit mass is K0 s² + KI / s². Both parts have twice been scaled
/// so the whole speed range is a tenth slower (K0 / 0.81, KI * 0.81) with the
/// same best glide ratio: top speed in a dive is about 175 km/h and the best
/// glide of 7:1 comes at about 47 km/h. The stall (LIFT_K) stayed put the
/// second time.
const K0: f32 = 0.0041667;
const KI: f32 = 117.45;
/// How quickly lift bends the path toward the aim, per m/s of airspeed.
const LIFT_RATE: f32 = 0.2;
/// Most lift the wings make, per (m/s)²: they hold level flight down to
/// about 9.4 m/s and stall below it, a tenth slower with the rest.
const LIFT_K: f32 = 0.1111;
/// The hardest lift can pull, m/s² (about 5.5 g).
const MAX_TURN: f32 = 55.0;
/// A launch starts with this much upward speed and then pushes: the legs
/// take a moment rather than firing the player off in a single frame.
const LAUNCH_VY: f32 = 3.0;
/// The push: this much upward acceleration for `LAUNCH_PUSH` seconds, on top
/// of gravity. Together with LAUNCH_VY that is a hop of about 3.7 m.
const LAUNCH_PUSH_A: f32 = 22.0;
const LAUNCH_PUSH: f32 = 0.3;
/// The wings open once a launch has slowed to this, near the top of the hop.
const LAUNCH_OPEN_VY: f32 = 1.0;
/// The climb then swings round into forward flight over this long, at no
/// less than LAUNCH_FLY_SPEED, rather than snapping across in one frame.
const LAUNCH_SWING: f32 = 0.5;
/// Just over the stall speed (about 9.4 m/s), so the wings hold from the top.
const LAUNCH_FLY_SPEED: f32 = 10.8;
/// Wind speed for ridge lift, m/s.
const WIND_SPEED: f32 = 7.0;
/// Thermals sit one to a cell this size, about half the cells.
const CELL: f32 = 90.0;
/// The flying body: a prone half-width and height, centred on the eye rather
/// than standing under it, so what the flyer clips is about what the viewport
/// shows. `BOX_LIFT` is the box's floor above the player's feet.
const BOX_HW: f32 = 0.22;
const BOX_H: f32 = 0.7;
const BOX_LIFT: f32 = WING_EYE - BOX_H * 0.5;
/// Hardest bank A or D can hold, radians (about 69 degrees).
const MAX_ROLL: f32 = 1.2;
/// The roll has mass. A and D push it over at this angular acceleration,
/// against a weak pull back to level (`ROLL_W`, rad/s) and damping below one
/// (`ROLL_ZETA`): held, it pins at MAX_ROLL; let go, it swings back through
/// level, past it by about 9 degrees, and settles over a couple of seconds.
/// The push is sized for a resting bank well past MAX_ROLL so holding a turn
/// sits firmly on the stop.
const ROLL_ACCEL: f32 = 10.8;
const ROLL_W: f32 = 2.6;
const ROLL_ZETA: f32 = 0.55;
/// The mouse banks as well as aims: how hard the turn rate it asks for pushes
/// the roll, and the most roll it can hold on its own — half of what A or D
/// hold (a steady push rests at push / ROLL_W²).
const MOUSE_ROLL: f32 = 2.0;
const MOUSE_ROLL_MAX: f32 = ROLL_W * ROLL_W * MAX_ROLL * 0.5;
/// A barrel roll takes this long.
const SPIN_SECS: f32 = 0.6;
/// A contact no squarer than this into the surface, at or above `GRAZE_SPEED`,
/// is a graze rather than a crash: about 25 degrees.
const GRAZE_INTO: f32 = 0.42;
const GRAZE_SPEED: f32 = 8.0;
/// A graze scrubs this share of the remaining speed per second, plus more the
/// squarer it is and more again where the surface is a floor: a wall can be
/// ridden, a flat belly slide drags to a halt in well under a second.
const GRAZE_SCRUB: f32 = 0.3;
const GRAZE_DRAG: f32 = 2.2;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum LiftKind {
    #[default]
    None,
    Thermal,
    Ridge,
}

fn hash(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ 0x2545_f491;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

fn byte(h: u32, k: u32) -> f32 {
    ((h >> (k * 8)) & 0xff) as f32 / 255.0
}

/// Strength of the thermals at (x, z), m/s, before fading with height: round
/// columns 18-32 m across the core, 3.5-6.5 m/s at the centre.
pub(crate) fn thermal(x: f32, z: f32) -> f32 {
    let (cx, cz) = ((x / CELL).floor() as i32, (z / CELL).floor() as i32);
    let mut w = 0.0;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (ix, iz) = (cx + dx, cz + dz);
            let h = hash(ix, iz);
            if byte(h, 0) > 0.5 {
                continue;
            }
            let h2 = hash(ix ^ 0x5bd1, iz.wrapping_add(7919));
            let c = ((ix as f32 + 0.2 + 0.6 * byte(h, 1)) * CELL, (iz as f32 + 0.2 + 0.6 * byte(h, 2)) * CELL);
            let r = 18.0 + 14.0 * byte(h, 3);
            let d2 = ((x - c.0).powi(2) + (z - c.1).powi(2)) / (r * r);
            if d2 < 9.0 {
                w += (3.5 + 3.0 * byte(h2, 0)) * (-d2).exp();
            }
        }
    }
    w
}

/// The air a world gives the glider (world.json `glide`): how strong its
/// thermals are, and the wind that makes ridge lift. The default is the
/// engine's own: full thermals and `WIND` at `WIND_SPEED`.
#[derive(Clone, Copy, Debug)]
pub struct GlideAir {
    pub thermals: f32,
    /// Which way the ridge wind blows (x, z), unit.
    pub ridge: glam::Vec2,
    pub ridge_speed: f32,
}

impl Default for GlideAir {
    fn default() -> Self {
        GlideAir {
            thermals: 1.0,
            ridge: glam::Vec2::new(WIND.x, WIND.z),
            ridge_speed: WIND_SPEED,
        }
    }
}

impl Ride {
    /// Vertical speed of the air at `pos`, and what is lifting it.
    fn rising_air(&self, store: &Store, pos: Vec3, sea: f32) -> (f32, LiftKind) {
        let mut thermal_w = 0.0;
        if !self.calm_air {
            // Over land, from a few metres up to about 200 m above the ground.
            let above = self.clearance.unwrap_or(100.0);
            let over_land = self.clearance.map_or(true, |c| pos.y - c > sea + 0.5);
            if over_land {
                thermal_w = self.glide_air.thermals * thermal(pos.x, pos.z) * smoothstep(1.0, 10.0, above) * (1.0 - smoothstep(160.0, 240.0, above));
            }
        }
        // Ridge lift where the wind runs up a slope, fading with height.
        let mut ridge_w = 0.0;
        if let Some(c) = self.clearance {
            let wind = self.glide_air.ridge;
            let h = |k: f32| top_below(store, pos.x + wind.x * k, pos.z + wind.y * k, pos.y, 120.0);
            if let (Some(a), Some(b)) = (h(14.0), h(-14.0)) {
                let grad = ((a - b) / 28.0).clamp(0.0, 0.8);
                ridge_w = self.glide_air.ridge_speed * grad * (1.0 - smoothstep(20.0, 70.0, c));
            }
        }
        let kind = if thermal_w.max(ridge_w) < 0.8 {
            LiftKind::None
        } else if thermal_w > ridge_w {
            LiftKind::Thermal
        } else {
            LiftKind::Ridge
        };
        (thermal_w + ridge_w, kind)
    }

    /// Open the wings now, from whatever the player is doing (headless checks).
    pub fn launch_wing(&mut self, p: &mut Player) {
        if !self.can(ModeId::Glide) {
            return;
        }
        self.start_wing(p, false, true);
    }

    /// F: launch from the ground, open the wings in the air, fold them in flight.
    pub fn glide_key(&mut self, p: &mut Player) {
        if self.mode != Mode::Wing && !self.can(ModeId::Glide) {
            return;
        }
        match self.mode {
            Mode::Wing => {
                self.mode = Mode::Foot;
                p.body.on_ground = false;
                self.note = self.soar.bank(0.5).or(Some("WINGS FOLDED   C OR SPACE TO OPEN".into()));
            }
            Mode::Foot => {
                if p.swimming {
                    self.note = Some("NOT IN THE WATER".into());
                } else if p.body.on_ground {
                    self.launch(p, false);
                } else {
                    self.start_wing(p, false, false);
                }
            }
            Mode::Bike => {
                if self.air {
                    self.start_wing(p, true, false);
                } else {
                    self.launch(p, true);
                }
            }
            Mode::Climb => {
                // Push off the wall and fly.
                self.mantle = None;
                p.body.vel = self.wall_n * 6.0 + Vec3::Y * 2.0 + dir_of(p.yaw) * 2.0;
                self.start_wing(p, false, false);
            }
            Mode::Swing => self.start_wing(p, false, false),
            Mode::Kayak => self.note = Some("NOT FROM THE KAYAK".into()),
            Mode::Drive => self.note = Some("NOT FROM THE CAR".into()),
        }
    }

    /// Straight up from the ground, wings opening near the top.
    fn launch(&mut self, p: &mut Player, from_bike: bool) {
        let across = if from_bike {
            dir_of(self.heading) * self.speed
        } else {
            Vec3::new(p.body.vel.x, 0.0, p.body.vel.z)
        };
        self.start_wing(p, from_bike, false);
        p.body.vel = across * 0.6 + Vec3::Y * LAUNCH_VY;
        self.launch_t = 3.0;
        self.launch_push = LAUNCH_PUSH;
        self.note = Some("UP".into());
    }

    pub(super) fn start_wing(&mut self, p: &mut Player, from_bike: bool, push: bool) {
        let v = if from_bike && self.mode == Mode::Bike {
            dir_of(self.heading) * self.speed + Vec3::Y * self.vy
        } else {
            p.body.vel
        };
        self.mode = Mode::Wing;
        self.from_bike = from_bike;
        self.launch_t = 0.0;
        self.launch_push = 0.0;
        self.launch_swing = 0.0;
        self.mantle = None;
        self.aim_yaw = p.yaw;
        self.aim_pitch = p.pitch;
        self.look_was = false;
        // A push off the edge so the wings have air over them from the start.
        p.body.vel = if push { v + dir_of(p.yaw) * 4.0 } else { v };
        p.body.on_ground = false;
        self.roll = 0.0;
        self.roll_v = 0.0;
        self.aim_rate = 0.0;
        self.spin = 0.0;
        self.spin_cd = 0.0;
        self.soar.start();
        self.tell(
            Mode::Wing,
            "GLIDING   MOUSE AIMS  A D ROLL  A+D BARREL ROLL  W TUCK  S FLARE  SHIFT BOOST  ALT LOOK AROUND  C FOLD   FLY CLOSE TO SCORE",
        );
    }

    pub(super) fn wing(&mut self, p: &mut Player, input: &MoveInput, store: &Store, sea: f32, dt: f32) {
        // Aim: the mouse for the nose, A D for the roll. Rolling tilts the
        // lift, and the turn comes out of the bank (g tan roll across the
        // path), so fast flight turns wider than slow and a turn has to be
        // rolled into. A and D together throw a barrel roll. Holding Alt
        // looks around while the aim holds; letting go snaps the view back.
        let keys = input.side();
        let both = input.left && input.right;
        // Aiming across with the mouse also banks the body, half as hard as A
        // or D. How far the mouse has swung since the last step is how far
        // p.yaw has moved off the aim this step starts from; Alt (look) aims
        // nothing, so it banks nothing.
        let swing = if input.look || self.look_was { 0.0 } else { wrap(p.yaw - self.aim_yaw) };
        self.aim_rate += (swing / dt.max(1e-4) - self.aim_rate) * (dt * 12.0).min(1.0);
        let mouse = (-self.aim_rate * MOUSE_ROLL).clamp(-MOUSE_ROLL_MAX, MOUSE_ROLL_MAX);
        self.spin_cd = (self.spin_cd - dt).max(0.0);
        if both && self.spin == 0.0 && self.spin_cd <= 0.0 && self.launch_t <= 0.0 {
            self.spin = 1e-4;
            self.soar.award(soar::Bonus::Roll);
        }
        if self.spin > 0.0 {
            self.spin += dt / SPIN_SECS * std::f32::consts::TAU;
            if self.spin >= std::f32::consts::TAU {
                self.spin = 0.0;
                self.spin_cd = 1.5;
            }
        }
        let push = if both { 0.0 } else { keys * ROLL_ACCEL + mouse };
        self.roll_v += (push - 2.0 * ROLL_ZETA * ROLL_W * self.roll_v - ROLL_W * ROLL_W * self.roll) * dt;
        self.roll += self.roll_v * dt;
        if self.roll.abs() > MAX_ROLL {
            self.roll = self.roll.clamp(-MAX_ROLL, MAX_ROLL);
            self.roll_v = 0.0;
        }
        let yaw_rate = G * self.roll.tan() / p.body.vel.length().max(10.8);
        if input.look {
            self.aim_yaw -= yaw_rate * dt;
        } else {
            if self.look_was {
                p.yaw = self.aim_yaw;
                p.pitch = self.aim_pitch;
            }
            p.yaw -= yaw_rate * dt;
            self.aim_yaw = p.yaw;
            self.aim_pitch = p.pitch;
        }
        self.look_was = input.look;
        // Hard over, the wings have little left for pulling the nose up.
        let up_cap = 0.6 - 0.45 * (self.roll.abs() / MAX_ROLL).min(1.0);
        let pitch = self.aim_pitch.clamp(-1.35, up_cap);
        let (sy, cy) = self.aim_yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let aim = Vec3::new(-sy * cp, sp, -cy * cp);

        let (w, kind) = self.rising_air(store, p.pos, sea);
        self.lift = w;
        self.lift_kind = kind;
        let air = Vec3::Y * w;
        let s0 = p.body.vel.length();
        let mut va = p.body.vel - air;
        let flare = input.back;
        let tuck = input.forward && !flare;
        let gravity = Vec3::Y * -G;

        let s = va.length();
        if self.launch_t > 0.0 || s < 0.01 {
            // Going up: the legs push for a moment, then no lift until the
            // climb slows, and then the climb swings round into forward
            // flight over half a second rather than snapping across.
            self.launch_t -= dt;
            va += gravity * dt;
            if self.launch_push > 0.0 {
                va += Vec3::Y * LAUNCH_PUSH_A * dt.min(self.launch_push);
                self.launch_push -= dt;
            } else if self.launch_swing > 0.0 || va.y < LAUNCH_OPEN_VY {
                if self.launch_swing <= 0.0 {
                    self.launch_swing = LAUNCH_SWING;
                }
                let fwd = Vec3::new(aim.x, aim.y.min(0.0), aim.z).normalize_or(dir_of(self.aim_yaw));
                let target = fwd * va.length().max(LAUNCH_FLY_SPEED);
                va = va.lerp(target, (dt / self.launch_swing).min(1.0));
                self.launch_swing -= dt;
                if self.launch_swing <= 0.0 {
                    self.launch_t = 0.0;
                }
            }
        } else {
            // Lift is one force across the path, capped by what the wings can
            // make (which grows with the square of airspeed). It is asked to
            // cancel gravity's pull across the path, so the path follows the
            // aim rather than sagging below it, and to turn the path toward
            // the aim. Too slow and it can't do both: the nose drops (a stall).
            let vn = va / s;
            let across = gravity - vn * gravity.dot(vn);
            let rate = LIFT_RATE
                * s
                * if flare {
                    1.3
                } else if tuck {
                    0.75
                } else {
                    1.0
                };
            let ang = vn.angle_between(aim);
            let to_aim = aim - vn * aim.dot(vn);
            let steer = if to_aim.length() > 1e-5 {
                to_aim.normalize() * (ang * rate).min(ang / dt) * s
            } else {
                Vec3::ZERO
            };
            let cap = (LIFT_K
                * s
                * s
                * if flare {
                    1.4
                } else if tuck {
                    0.8
                } else {
                    1.0
                })
            .min(MAX_TURN);
            let mut lift = steer - across;
            if lift.length() > cap {
                lift *= cap / lift.length();
            }
            // Gravity along the path changes the speed; the rest only turns
            // it. Turning costs a little speed.
            let turn = across + lift;
            let nv = (vn + turn * (dt / s)).normalize();
            let turned = (turn.length() - across.length()).max(0.0) * dt / s;
            va = nv * (s + gravity.dot(vn) * dt) * (1.0 - turned * 0.15);
        }
        // Drag. S flares (slow and floaty), W tucks (fast and steep). Close to
        // the ground the wings work less hard (ground effect).
        let s = va.length();
        if s > 0.01 {
            let (k0, ki) = if flare {
                (2.2, 0.6)
            } else if tuck {
                (0.55, 1.5)
            } else {
                (1.0, 1.0)
            };
            let ground_effect = 0.55 + 0.45 * self.clearance.map_or(1.0, |c| smoothstep(0.5, 5.0, c));
            let induced = if self.launch_t > 0.0 { 0.0 } else { KI * ki * ground_effect / s.max(6.3).powi(2) };
            let d = K0 * k0 * s * s + induced;
            va -= va / s * (d * dt).min(s * 0.5);
        }
        if self.boosting {
            va += aim * 11.0 * dt;
        }
        let v = va + air;
        self.vario = v.y;

        // The body shows the bank it is flying at. `lean` is the pose the
        // camera and the model read, and there a right bank is negative (as
        // the bike sets it), the opposite sign to `roll`. A barrel roll is
        // carried apart from it (self.spin), so the view takes all of it.
        let vh = Vec3::new(v.x, 0.0, v.z);
        self.lean += (-self.roll - self.lean) * (dt * 10.0).min(1.0);
        let fp = (v.y / v.length().max(1.0)).clamp(-1.0, 1.0).asin();
        self.body_pitch += (fp * 0.6 + pitch * 0.4 - self.body_pitch) * (dt * 6.0).min(1.0);
        if vh.length() > 1.0 {
            self.heading = wrap(yaw_of(vh));
        }

        // Move. Fast flight can outrun the world streaming in. Unloaded space
        // is solid to collision, so fly straight through it, and if terrain
        // then loads around the flyer, lift them out on top.
        p.body.vel = v;
        // What flies is a prone body round the eye, not the standing box, and
        // leaves are no obstacle to it.
        let (stand_hw, stand_h) = (p.body.half_w, p.body.height);
        p.body.half_w = BOX_HW;
        p.body.height = BOX_H;
        p.pos.y += BOX_LIFT;
        let next = p.pos + v * dt * 4.0;
        let res = if box_loaded(store, next, BOX_HW, BOX_H) && box_loaded(store, p.pos, BOX_HW, BOX_H) {
            move_body_with(store, &mut p.pos, &mut p.body, v * dt, 0.0, hits_hard)
        } else {
            p.pos += v * dt;
            p.body.on_ground = false;
            crate::physics::MoveResult { stepped: 0.0, blocked: false }
        };
        // A crown costs a little speed and rattles the view; it stops nothing.
        if hits_soft(store, p.pos, BOX_HW, BOX_H).1 {
            p.body.vel *= 1.0 - (3.0 * dt).min(0.5);
            self.shake += 2.0 * dt;
        }
        if box_loaded(store, p.pos, BOX_HW, BOX_H) {
            crate::physics::free_body(store, &mut p.pos, BOX_HW, BOX_H, 2.0, hits_hard);
        }
        let out = p.body.vel;
        p.pos.y -= BOX_LIFT;
        p.body.half_w = stand_hw;
        p.body.height = stand_h;

        // Which way out of whatever was touched: move_body zeroes the
        // velocity on the axes it stopped, and voxel faces are axis-aligned,
        // so those axes give the surface normal.
        let touched = res.blocked || p.body.on_ground;
        let mut n = Vec3::ZERO;
        if touched {
            for a in 0..3 {
                if v[a] != 0.0 && out[a] == 0.0 {
                    n[a] = -v[a].signum();
                }
            }
            if n == Vec3::ZERO {
                n = Vec3::Y;
            }
            n = n.normalize();
        }
        let speed = v.length();
        let into = if touched { (-v.normalize_or_zero().dot(n)).max(0.0) } else { 1.0 };
        let hit_speed = Vec3::new(v.x, 0.0, v.z).length();
        if p.pos.y < sea {
            self.mode = Mode::Foot;
            p.body.vel = v * 0.2;
            self.note = self.soar.bank(0.6).or(Some("SPLASH".into()));
        } else if touched && into < GRAZE_INTO && speed > GRAZE_SPEED && !flare {
            // Shallow and fast: ride along the surface, losing some speed,
            // rather than ending the flight.
            // The speed into the surface is gone either way; what is left
            // scrubs by the second, so a long slide costs without a single
            // touch being fatal.
            let along = v - n * v.dot(n);
            let scrub = GRAZE_SCRUB + 1.2 * into + GRAZE_DRAG * n.y.max(0.0);
            p.body.vel = along * (1.0 - scrub * dt).max(0.0);
            p.body.on_ground = false;
            let aside = p.pos + n * 0.12;
            if !hits_hard(store, aside + Vec3::Y * BOX_LIFT, BOX_HW, BOX_H) {
                p.pos = aside;
            }
            self.shake += (0.6 + 2.0 * into) * dt;
            self.soar.award(soar::Bonus::Graze);
        } else if p.body.on_ground {
            self.touch_down(p, store, v);
        } else if res.blocked {
            // Into a cliff slowly enough: take hold of it. Only a face that
            // is mostly a wall counts, and the body has to be out of what
            // the prone box clipped first: the player's feet fly more than
            // half a metre under that box, so at a bump on flat ground the
            // climbing test would otherwise find rock all round them.
            self.feet_down(p, store);
            if box_loaded(store, p.pos, stand_hw, stand_h) {
                crate::physics::free_body(store, &mut p.pos, stand_hw, stand_h, 2.0, hits_hard);
            }
            if n.y.abs() < 0.6 && hit_speed < 20.0 && self.start_climb(p, store) {
                self.shake += 0.05;
                self.soar.bank(0.8);
            } else {
                self.mode = Mode::Foot;
                p.body.vel = -Vec3::new(v.x, 0.0, v.z) * 0.15 + Vec3::Y * v.y.min(0.0);
                self.shake += (hit_speed * 0.01).min(0.25);
                self.soar.crashed();
                let said = if hit_speed > 15.0 { "WHUMP   C TO OPEN THE WINGS AGAIN" } else { "BUMP" };
                self.note = self.soar.bank(0.6).or(Some(said.into()));
            }
        }
        if self.mode == Mode::Wing {
            self.eye_y = p.pos.y + WING_EYE;
            self.eye_v = 0.0;
            let eye = Vec3::new(p.pos.x, self.eye_y, p.pos.z);
            self.soar.step(store, eye, p.body.vel, self.clearance, dt);
        } else if self.mode == Mode::Foot && box_loaded(store, p.pos, stand_hw, stand_h) {
            // Standing again in a box taller than the one that was flying:
            // step out of anything it now overlaps.
            crate::physics::free_body(store, &mut p.pos, stand_hw, stand_h, 2.0, hits_hard);
        }
        self.shake += (s0 - 25.0).max(0.0) * 0.00015;
        self.shake *= 1.0 - (dt * 6.0).min(1.0);
    }

    /// Set the feet on the surface the prone box came down on. The box flies
    /// round the eye, so its floor is `BOX_LIFT` above the player's feet, and
    /// a landing leaves them that far into the ground until they are moved.
    fn feet_down(&mut self, p: &mut Player, store: &Store) {
        if let Some(h) = top_below(store, p.pos.x, p.pos.z, p.pos.y + BOX_LIFT + 0.05, BOX_LIFT + 1.0) {
            p.pos.y = p.pos.y.max(h);
        }
    }

    fn touch_down(&mut self, p: &mut Player, store: &Store, v: Vec3) {
        self.feet_down(p, store);
        let vh = Vec3::new(v.x, 0.0, v.z);
        let hard = -v.y;
        self.shake += (hard * 0.01).min(0.2);
        if self.from_bike {
            self.start_bike(p, vh.length() * 0.8);
            self.heading = wrap(yaw_of(vh));
            self.prev_heading = self.heading;
            self.air = false;
            self.vy = 0.0;
            self.eye_y = p.pos.y + BIKE_EYE + 0.4;
            self.prev_eye_y = self.eye_y;
            self.bob_v = -hard * 0.15;
            self.note = self.soar.bank(1.0).or(Some("TOUCHDOWN".into()));
        } else {
            self.mode = Mode::Foot;
            p.body.vel = vh * 0.4;
            self.note = self.soar.bank(1.0).or(Some("TOUCHDOWN".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    fn flyer_at(x: f32, y: f32, pitch: f32) -> Rider {
        let mut p = rider_at(x, y);
        p.body.on_ground = false;
        p.body.vel = Vec3::new(8.0, -2.0, 0.0);
        p.pitch = pitch;
        p.ride.jump_was = false;
        p.ride.calm_air = true;
        p
    }

    fn open(p: &mut Rider, s: &Store) {
        run(p, s, MoveInput { jump: true, ..Default::default() }, 1.0 / 60.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Wing);
    }

    /// Open the wings with no room underneath (Space needs WING_CLEARANCE).
    fn open_anywhere(p: &mut Rider) {
        let Rider { p: body, ride } = p;
        ride.launch_wing(body);
        assert_eq!(ride.mode, Mode::Wing);
    }

    /// Aimed a little below the horizon, the glider settles near 65 km/h and
    /// goes better than five metres forward for each metre down, then lands.
    #[test]
    fn cruise_glide() {
        // A long strip: the glide covers most of a kilometre.
        let s = land_in(|_| 2.0, (-4, 120), (-2, 2));
        let mut p = flyer_at(-28.0, 100.0, -0.18);
        p.body.vel = Vec3::new(20.0, -3.6, 0.0);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput::default(), 4.0, false, |_, _| {});
        let (x0, y0) = (p.pos.x, p.pos.y);
        let mut speeds = Vec::new();
        run(&mut p, &s, MoveInput::default(), 4.0, false, |p, _| speeds.push(p.body.vel.length()));
        let ratio = (p.pos.x - x0) / (y0 - p.pos.y);
        let v = speeds.iter().sum::<f32>() / speeds.len() as f32;
        assert!((5.0..8.0).contains(&ratio), "glide ratio {ratio}");
        assert!((14.0..28.0).contains(&v), "speed {v}");
        // Down from 100 m at about 3.5 m/s, then a belly skim that scrubs the
        // last of the speed off.
        run(&mut p, &s, MoveInput::default(), 30.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Foot, "landed at y {}", p.pos.y);
    }

    /// Diving then pulling up climbs: the glider trades speed for height.
    #[test]
    fn dive_and_zoom() {
        let s = land(|_| 2.0);
        let mut p = flyer_at(0.0, 140.0, -1.2);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 3.0, false, |_, _| {});
        let fast = p.body.vel.length();
        assert!(fast > 28.0, "dive speed {fast}");
        p.pitch = 0.5;
        let y0 = p.pos.y;
        let mut peak = y0;
        run(&mut p, &s, MoveInput::default(), 4.0, false, |p, _| peak = peak.max(p.pos.y));
        assert!(peak > y0 + 10.0, "zoom climb from {y0} peaked at {peak}");
    }

    /// Flying along a slope that faces the wind loses less height than
    /// flying the same height over flat ground.
    #[test]
    fn ridge_lift_extends_a_flight() {
        // The ground rises with the wind (+X); the flyer goes along it (+Z).
        let ridge = land_in(|x| (2.0 + (x + 32.0) * 0.6).min(64.0), (-4, 12), (-2, 12));
        let flat = land_in(|_| 33.0, (-4, 12), (-2, 12));
        let height_lost = |s: &Store| {
            let mut p = flyer_at(20.0, 60.0, -0.15);
            p.yaw = std::f32::consts::PI;
            p.body.vel = Vec3::new(0.0, -2.0, 8.0);
            open(&mut p, s);
            run(&mut p, s, MoveInput::default(), 4.0, false, |_, _| {});
            assert_eq!(p.ride.mode, Mode::Wing);
            60.0 - p.pos.y
        };
        let (a, b) = (height_lost(&flat), height_lost(&ridge));
        assert!(b < a * 0.7, "ridge lost {b} m vs flat {a} m");
    }

    /// About half the 90 m cells hold a thermal, strong at the core.
    #[test]
    fn thermals_are_scattered() {
        let mut strong = 0;
        let mut cells = std::collections::HashSet::new();
        for z in 0..100 {
            for x in 0..100 {
                let (px, pz) = (x as f32 * 9.0, z as f32 * 9.0);
                if thermal(px, pz) > 3.0 {
                    strong += 1;
                    cells.insert(((px / CELL) as i32, (pz / CELL) as i32));
                }
            }
        }
        assert!(cells.len() > 25 && cells.len() < 80, "{} of 100 cells with a thermal", cells.len());
        assert!(strong > 100, "{strong} strong samples");
    }

    /// Circling slowly in a thermal climbs.
    #[test]
    fn thermal_climbs() {
        // The strongest thermal core near the middle of a big flat store.
        let mut best = (0.0, Vec3::ZERO);
        for z in (0..400).step_by(2) {
            for x in (0..400).step_by(2) {
                let w = thermal(x as f32, z as f32);
                if w > best.0 {
                    best = (w, Vec3::new(x as f32, 0.0, z as f32));
                }
            }
        }
        assert!(best.0 > 4.0, "no strong thermal near the test area ({})", best.0);
        let (cx, cz) = ((best.1.x / 8.0) as i32, (best.1.z / 8.0) as i32);
        let s = land_in(|_| 2.0, (cx - 6, cx + 6), (cz - 6, cz + 6));
        let mut p = flyer_at(best.1.x, 60.0, -0.05);
        p.pos.z = best.1.z;
        p.ride.calm_air = false;
        p.body.vel = Vec3::new(12.0, 0.0, 0.0);
        open(&mut p, &s);
        let y0 = p.pos.y;
        // A tight circle: turn the aim steadily while flaring.
        for _ in 0..600 {
            p.yaw += 0.9 / 60.0;
            p.update(&MoveInput { back: true, ..Default::default() }, &s, -100.0, (-100.0, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
        assert_eq!(p.ride.mode, Mode::Wing);
        assert!(p.pos.y > y0, "circled from {y0} to {}", p.pos.y);
    }

    /// F on the ground launches well up, and the wings open near the top.
    #[test]
    fn launch_from_the_ground() {
        let s = land(|_| 20.0);
        let mut p = rider_at(0.0, 20.0);
        p.ride.calm_air = true;
        let mut r = std::mem::take(&mut p.ride);
        r.glide_key(&mut p);
        p.ride = r;
        assert_eq!(p.ride.mode, Mode::Wing);
        let start = p.pos.y;
        let mut peak: f32 = 0.0;
        run(&mut p, &s, MoveInput::default(), 4.0, false, |p, _| peak = peak.max(p.pos.y));
        // A hop of about 3.7 m, then gliding.
        let hop = peak - start;
        assert!((3.0..4.5).contains(&hop), "launch rose {hop} m");
        assert_eq!(p.ride.mode, Mode::Wing);
        assert!(p.body.vel.length() > 8.0, "flying at {}", p.body.vel.length());
    }

    /// A store with flat ground and a wall running along X: solid for
    /// z >= `z0`, up to `top` metres.
    fn wall_along_x(z0: f32, top: f32) -> Store {
        let mut s = land_in(|_| 2.0, (-4, 40), (-6, 6));
        let vz0 = (z0 * 4.0).round() as i32;
        for z in vz0..6 * 32 {
            for x in -4 * 32..40 * 32 {
                for y in 0..(top * 4.0) as i32 {
                    s.set(x, y, z, crate::world::material::STONE);
                }
            }
        }
        s
    }

    /// Grazing a wall at a shallow angle keeps the flight: it scrubs some
    /// speed and slides along instead of landing.
    #[test]
    fn a_graze_does_not_end_the_flight() {
        let s = wall_along_x(1.0, 60.0);
        let mut p = flyer_at(-28.0, 40.0, -0.1);
        p.pos.z = 0.2;
        // Fast along the wall, easing into it a little.
        p.body.vel = Vec3::new(30.0, -1.0, 2.0);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput::default(), 1.5, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Wing, "a graze ended the flight at x {}", p.pos.x);
        let v = p.body.vel.length();
        assert!(v > 18.0, "a graze cost too much speed: {v}");
        assert!(p.pos.x > 0.0, "it stopped going along the wall at x {}", p.pos.x);
        assert!(p.ride.soar.score > 0.0, "a graze scored nothing");
    }

    /// Flying square into the same wall still crashes.
    #[test]
    fn a_square_hit_still_crashes() {
        let s = wall_along_x(1.0, 60.0);
        let mut p = flyer_at(-28.0, 40.0, -0.1);
        p.pos.z = -8.0;
        // Aim and fly straight at the wall: dir_of(PI) is +Z.
        p.yaw = std::f32::consts::PI;
        p.body.vel = Vec3::new(0.0, -1.0, 26.0);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput::default(), 2.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Foot, "a square hit kept flying");
    }

    /// Leaves are flown through: they cost speed, not the flight.
    #[test]
    fn leaves_do_not_stop_the_flyer() {
        let mut s = land_in(|_| 2.0, (-4, 40), (-2, 2));
        // A thicket across the path, 8 m of it.
        for x in 0..32 {
            for z in -8..8 {
                for y in 8 * 4..12 * 4 {
                    s.set(x, y, z, crate::world::material::LEAVES);
                }
            }
        }
        let mut p = flyer_at(-28.0, 10.2, -0.05);
        p.body.vel = Vec3::new(26.0, 0.0, 0.0);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput::default(), 1.5, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Wing, "leaves ended the flight at x {}", p.pos.x);
        assert!(p.pos.x > 4.0, "it did not get through the thicket: x {}", p.pos.x);
    }

    /// The flying body is prone round the eye, so it fits through a slot the
    /// standing body could not.
    #[test]
    fn the_flying_body_fits_a_low_slot() {
        let mut s = land_in(|_| 2.0, (-4, 40), (-2, 2));
        // A roof 1 m above a shelf, the length of the run: a slot 1 m tall.
        let (shelf, roof) = (10.0f32, 11.0f32);
        for x in -4 * 32..40 * 32 {
            for z in -2 * 32..2 * 32 {
                for y in 0..(shelf * 4.0) as i32 {
                    s.set(x, y, z, crate::world::material::STONE);
                }
                for y in (roof * 4.0) as i32..(roof * 4.0) as i32 + 16 {
                    s.set(x, y, z, crate::world::material::STONE);
                }
            }
        }
        // The eye half way up the slot: the prone box clears it, the
        // standing box never would.
        let eye = (shelf + roof) * 0.5;
        let mut p = flyer_at(-28.0, eye - WING_EYE, 0.0);
        p.body.vel = Vec3::new(26.0, 0.0, 0.0);
        assert!(hits(&s, p.pos, p.body.half_w, p.body.height), "the standing box should not fit the slot");
        open_anywhere(&mut p);
        run(&mut p, &s, MoveInput::default(), 1.5, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Wing, "the slot stopped the flight at x {}", p.pos.x);
        assert!(p.pos.x > 0.0, "it did not get along the slot: x {}", p.pos.x);
        // A roof and a floor within 3 m is an arch: it pays out.
        assert!(p.ride.soar.score > 1000.0, "threading the slot scored {}", p.ride.soar.score);
    }

    /// Sweeping the aim across with the mouse banks the body too, about half
    /// as far as a key holds it, and the other way for the other way round.
    #[test]
    fn the_mouse_banks_the_glider() {
        let s = land_in(|_| 2.0, (-20, 40), (-30, 30));
        let banked = |rate: f32| {
            let mut p = flyer_at(0.0, 120.0, -0.1);
            p.body.vel = Vec3::new(18.0, -1.0, 0.0);
            open(&mut p, &s);
            let dt = 1.0 / 60.0;
            for _ in 0..90 {
                // What the mouse does: a steady sweep across, right for a
                // falling yaw, as the window's own handler turns it.
                p.yaw -= rate * dt;
                p.update(&MoveInput::default(), &s, -100.0, (-100.0, glam::Vec2::ZERO), false, dt);
            }
            assert_eq!(p.ride.mode, Mode::Wing);
            p.ride.roll
        };
        let right = banked(1.2);
        let left = banked(-1.2);
        assert!((0.2..0.55).contains(&right), "a sweep right banked {right}");
        assert!((-0.55..-0.2).contains(&left), "a sweep left banked {left}");
        // The mouse alone can't hold more than half the bank a key does
        // (a little past it on the way, as the roll swings).
        let hard = banked(9.0);
        assert!(hard < MAX_ROLL * 0.6, "the mouse held {hard} of a {MAX_ROLL} bank");
    }

    /// Coming down on open ground ends the flight on foot, standing on the
    /// ground: not with a hold on it (the flying body's feet are half a metre
    /// under the box it flies with, so the wall test used to find rock all
    /// round them), and not inside it.
    #[test]
    fn a_landing_on_open_ground_ends_on_foot() {
        // Open ground with a knee-high step across it, the sort of thing a
        // landing runs into.
        let s = land(|x| 2.0 + (x.max(0.0) / 6.0).floor() * 0.75);
        for speed in [11.0f32, 14.0, 17.0] {
            let mut p = flyer_at(-28.0, 12.0, -0.12);
            p.body.vel = Vec3::new(speed, -1.5, 0.0);
            open(&mut p, &s);
            run(&mut p, &s, MoveInput::default(), 20.0, false, |_, _| {});
            assert_eq!(p.ride.mode, Mode::Foot, "at {speed} m/s the flight ended {:?}", p.ride.mode);
            assert!(!hits(&s, p.pos, p.body.half_w, p.body.height), "at {speed} m/s it ended inside the ground at {:?}", p.pos);
        }
    }

    /// Holding D banks right and turns right, A the other way, and the same
    /// bank turns more slowly the faster the flight. The pose the camera and
    /// the model read (`lean`) tilts the same way as the turn.
    #[test]
    fn rolling_turns_the_flight() {
        let s = land_in(|_| 2.0, (-20, 40), (-30, 30));
        let turned = |speed: f32, right: bool| {
            let mut p = flyer_at(0.0, 120.0, -0.1);
            p.body.vel = Vec3::new(speed, -1.0, 0.0);
            open(&mut p, &s);
            let y0 = p.yaw;
            let input = MoveInput {
                right,
                left: !right,
                ..Default::default()
            };
            run(&mut p, &s, input, 2.0, false, |_, _| {});
            (wrap(p.yaw - y0), p.ride.lean)
        };
        let (slow, lean) = turned(16.0, true);
        let (fast, _) = turned(34.0, true);
        let (other, lean_l) = turned(16.0, false);
        // Turning right means the yaw falls (dir_of turns toward +X as it does).
        assert!(slow < -0.5, "D should turn right: {slow} rad");
        assert!(other > 0.5, "A should turn left: {other} rad");
        // The camera and the model take a right bank as a negative lean.
        assert!(lean < -0.9, "D should bank right: lean {lean}");
        assert!(lean_l > 0.9, "A should bank left: lean {lean_l}");
        assert!(fast.abs() < slow.abs() * 0.8, "fast {fast} rad should turn wider than slow {slow} rad");
    }

    /// The roll has mass: letting go of D does not snap the wings flat, it
    /// swings back through level and settles the other side before levelling.
    #[test]
    fn the_roll_swings_back_through_level() {
        let s = land_in(|_| 2.0, (-20, 40), (-30, 30));
        let mut p = flyer_at(0.0, 200.0, -0.1);
        p.body.vel = Vec3::new(24.0, -2.0, 0.0);
        open(&mut p, &s);
        run(&mut p, &s, MoveInput { right: true, ..Default::default() }, 1.5, false, |_, _| {});
        let banked = p.ride.lean;
        assert!(banked < -1.1, "D should pin the bank over: lean {banked}");
        // Let go: it takes a moment to come back, rather than snapping flat.
        let mut at_quarter = 0.0;
        let mut overshoot: f32 = 0.0;
        run(&mut p, &s, MoveInput::default(), 2.5, false, |p, t| {
            if t < 0.26 {
                at_quarter = p.ride.lean;
            }
            overshoot = overshoot.max(p.ride.lean);
        });
        assert!(at_quarter < -0.75, "a quarter second after letting go it was already at {at_quarter}");
        assert!(overshoot > 0.05, "it never swung past level: best {overshoot}");
        assert!(p.ride.lean.abs() < 0.12, "it never settled: lean {}", p.ride.lean);
    }

    /// A and D together throw a barrel roll, which comes all the way round.
    #[test]
    fn a_barrel_roll_comes_round() {
        let s = land(|_| 2.0);
        let mut p = flyer_at(-28.0, 120.0, -0.1);
        p.body.vel = Vec3::new(24.0, -2.0, 0.0);
        open(&mut p, &s);
        let both = MoveInput {
            left: true,
            right: true,
            ..Default::default()
        };
        let mut peak: f32 = 0.0;
        run(&mut p, &s, both, 0.3, false, |p, _| peak = peak.max(p.ride.spin));
        assert!(peak > 2.0, "the roll barely started: {peak} rad");
        run(&mut p, &s, both, 0.5, false, |_, _| {});
        assert_eq!(p.ride.spin, 0.0, "the roll did not come round");
        assert!(p.ride.soar.score > 0.0, "a barrel roll scored nothing");
    }

    /// The same flight down a narrow canyon scores far more than out in the
    /// open, and lands with a line to show for it.
    #[test]
    fn a_canyon_outscores_the_open_sky() {
        let open_land = land_in(|_| 2.0, (-4, 40), (-6, 6));
        let mut canyon = land_in(|_| 2.0, (-4, 40), (-6, 6));
        // Walls about 3 m either side of the line: a gap to thread, not a valley.
        for z in [-12i32, 12] {
            for dz in 0..4 {
                for x in -4 * 32..40 * 32 {
                    for y in 0..60 * 4 {
                        canyon.set(x, y, z + dz * z.signum(), crate::world::material::STONE);
                    }
                }
            }
        }
        let flight = |s: &Store, y: f32| {
            let mut p = flyer_at(-28.0, y, -0.12);
            p.body.vel = Vec3::new(26.0, -3.0, 0.0);
            open(&mut p, s);
            run(&mut p, s, MoveInput::default(), 3.0, false, |_, _| {});
            assert_eq!(p.ride.mode, Mode::Wing, "came down early at x {}", p.pos.x);
            p.ride.soar.score
        };
        let sky = flight(&open_land, 120.0);
        let slot = flight(&canyon, 30.0);
        assert!(sky < 100.0, "open sky scored {sky}");
        assert!(slot > 4_000.0, "the canyon scored only {slot}");
    }

    /// Shift fires a short boost that empties the meter, then needs a refill.
    #[test]
    fn boost_is_a_burst() {
        let s = land(|_| 2.0);
        let mut p = flyer_at(-28.0, 120.0, -0.1);
        open(&mut p, &s);
        let boost = MoveInput { sprint: true, ..Default::default() };
        run(&mut p, &s, boost, 2.0, false, |_, _| {});
        assert!(!p.ride.boosting, "meter should be empty");
        let (left, _) = p.ride.boost_meter().unwrap();
        assert!(left < 0.05, "left {left}");
        run(&mut p, &s, MoveInput::default(), 4.5, false, |_, _| {});
        assert!(p.ride.boost_meter().unwrap().0 > 0.95);
    }
}
