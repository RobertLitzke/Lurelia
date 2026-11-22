//! Four wheels: a rigid body on four sprung wheels, for the dune buggy and
//! whatever follows it (an ATV, karts). Independent of `Ride` and of the
//! keys: a `Vehicle` is driven by `Controls`, which the player's keys fill
//! in ride/drive.rs and a bot or a gamepad could fill the same way. A parked
//! car and the one being driven are the same struct.
//!
//! Each 60 Hz step runs `SUBSTEPS` substeps: the springs are soft, but the
//! bump stop and the tyres are stiff, and substeps are the cheap cure. The
//! ground under each wheel is read once a step, as a plane.
//!
//! notes/WHEELS_PLAN.md is the design.

mod assist;
mod body;
pub mod bot;
pub mod draw;
mod grip;
pub mod spec;
pub mod tune;
mod wheel;

pub use body::Body;
#[allow(unused_imports)]
pub use spec::Driven;
pub use spec::{Catalogue, Spec, Steering};

use crate::world::store::{Cell, Store};
use glam::{Quat, Vec3};
use wheel::Contact;

pub const SUBSTEPS: usize = 4;

/// What the driver (or a bot) asks of the car this step.
#[derive(Clone, Copy, Default, Debug)]
pub struct Controls {
    /// 0 to 1.
    pub throttle: f32,
    /// 0 to 1: brake while rolling forward, reverse once stopped.
    pub brake: f32,
    /// -1 (full left) to 1 (full right), of the lock at this speed.
    pub steer: f32,
    pub handbrake: bool,
    /// Held loads the springs; let go to hop.
    pub hop: bool,
    pub boost: bool,
    /// Air control: 1 noses down, 1 rolls right.
    pub pitch: f32,
    pub roll: f32,
}

/// One wheel's state, for the physics and for drawing it.
#[derive(Clone, Copy, Debug)]
pub struct Wheel {
    /// Where it hangs from, in the body frame.
    pub mount: Vec3,
    pub rear: bool,
    pub left: bool,
    /// Hub below the mount along the body's down, metres, and as it was a
    /// step ago (for drawing between steps).
    pub t: f32,
    pub prev_t: f32,
    pub contact: Option<Contact>,
    pub down: bool,
    /// Load on the tyre, newtons.
    pub load: f32,
    pub slip: f32,
    pub slide: f32,
    pub spin: f32,
    /// Compression, metres, and its rate.
    pub x: f32,
    pub x_dot: f32,
    /// The wheel's turn about its axle, radians, now and a step ago.
    pub roll: f32,
    pub prev_roll: f32,
    omega: f32,
}

/// A kart drift under way: which way (1 right, -1 left), for how long, and
/// the speed it carries, which a drift sheds only slowly.
#[derive(Clone, Copy, Debug)]
pub struct Drift {
    pub dir: f32,
    pub t: f32,
    pub keep: f32,
}

/// Something the car did this step, for the HUD, the sound and the dust.
#[derive(Clone, Debug, Default)]
pub struct Events {
    pub note: Option<String>,
    /// Hit the bump stop or landed hard: how hard, m/s.
    pub thump: f32,
    /// Struck rock with the chassis: how hard, m/s.
    pub crash: f32,
    pub clean_landing: bool,
}

pub struct Vehicle {
    pub spec: Spec,
    pub b: Body,
    pub prev_pos: Vec3,
    pub prev_rot: Quat,
    pub wheels: [Wheel; 4],
    /// The front wheels' angle, radians, positive to the left.
    pub steer: f32,
    pub prev_steer: f32,
    preload: f32,
    hop_was: bool,
    /// The kart's drift, and seconds of drift boost left.
    pub drift: Option<Drift>,
    pub kick_t: f32,
    pub air_time: f32,
    pub grounded: usize,
    /// Touched down after a jump: seconds since, and the hardest impact.
    landing: Option<(f32, f32, f32)>,
    upset: f32,
    pub righting: f32,
    /// The last place it had four wheels down and was upright.
    pub last_good: Option<(Vec3, Quat)>,
    good_t: f32,
    /// The drawn chassis' extra roll and pitch, and their rates.
    vis: [f32; 4],
    pub prev_vis: [f32; 2],
    prev_vel: Vec3,
    /// The engine: revs 0 (idle) to 1, and how hard it is working.
    pub rpm: f32,
    pub load: f32,
    pub boosting: bool,
    /// Water over the floor, metres.
    pub wade: f32,
    pub stalled: bool,
    still: f32,
    pub asleep: bool,
    /// Waiting at the edge of the loaded world.
    pub waiting: bool,
    /// Whether the ground is drawn sloped (the last step's): the chassis
    /// meets the same ground the wheels do.
    slopes: bool,
    pub events: Events,
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The yaw (the game's convention: forward is (-sin, -cos)) of a body
/// orientation's forward.
pub fn yaw_of(rot: Quat) -> f32 {
    let f = rot * Vec3::NEG_Z;
    (-f.x).atan2(-f.z)
}

/// The ground under `q` (voxel tops, within a couple of metres above it and
/// six below): where a car is set down or a driver steps out.
pub fn draw_ground(store: &Store, q: Vec3) -> Option<f32> {
    wheel::height(store, q.x, q.z, q.y + 2.0, 8.0, false).map(|(h, _)| h)
}

/// Two cars touching: their chassis spheres pushed apart and an impulse
/// between them at the contact, so a nudge turns the car it hits. Returns
/// how hard they met, m/s. Only the tournament test races cars together
/// so far.
#[allow(dead_code)]
pub fn bump(a: &mut Vehicle, b: &mut Vehicle) -> f32 {
    if (a.b.pos - b.b.pos).length() > 6.0 {
        return 0.0;
    }
    let mut hardest = 0.0f32;
    for sa in a.spec.collide.clone() {
        for sb in b.spec.collide.clone() {
            let ca = a.b.pos + a.b.rot * Vec3::new(sa[0], sa[1], sa[2]);
            let cb = b.b.pos + b.b.rot * Vec3::new(sb[0], sb[1], sb[2]);
            let d = ca - cb;
            let dist = d.length();
            let reach = sa[3] + sb[3];
            if dist >= reach || dist < 1e-4 {
                continue;
            }
            let n = d / dist;
            let pen = reach - dist;
            a.b.pos += n * pen * 0.5;
            b.b.pos -= n * pen * 0.5;
            let p = cb + n * sb[3];
            let vn = (a.b.point_vel(p) - b.b.point_vel(p)).dot(n);
            if vn >= 0.0 {
                continue;
            }
            hardest = hardest.max(-vn);
            let k = |body: &Body| {
                let r = p - body.pos;
                1.0 / body.mass + n.dot(body.inv_i(r.cross(n)).cross(r))
            };
            let j = -(1.2 * vn) / (k(&a.b) + k(&b.b));
            a.b.impulse(p, n * j);
            b.b.impulse(p, -n * j);
        }
    }
    if hardest > 0.0 {
        a.asleep = false;
        b.asleep = false;
    }
    hardest
}

impl Vehicle {
    /// Whether the chassis is clear of rock where it stands.
    pub fn fits(&self, store: &Store) -> bool {
        self.spec.collide.iter().all(|s| {
            let c = self.b.pos + self.b.rot * Vec3::new(s[0], s[1], s[2]);
            body::push_out(store, c, s[3], self.slopes).is_none_or(|(_, pen)| pen < 0.08)
        })
    }

    /// A car of `spec` standing on its wheels with its contact patches at
    /// `ground` height under `at`, facing `yaw`.
    pub fn new(spec: Spec, at: Vec3, yaw: f32) -> Vehicle {
        let rot = Quat::from_rotation_y(yaw);
        let ride = spec.wheel_radius + spec.com_height;
        let pos = Vec3::new(at.x, at.y + ride, at.z);
        let wheels = std::array::from_fn(|i| {
            let rear = i >= 2;
            let t = spec.travel - spec.corner(rear).sag;
            Wheel {
                mount: spec.mount(i),
                rear,
                left: i.is_multiple_of(2),
                t,
                prev_t: t,
                contact: None,
                down: false,
                load: 0.0,
                slip: 0.0,
                slide: 0.0,
                spin: 0.0,
                x: 0.0,
                x_dot: 0.0,
                roll: 0.0,
                prev_roll: 0.0,
                omega: 0.0,
            }
        });
        Vehicle {
            b: Body {
                pos,
                rot,
                vel: Vec3::ZERO,
                ang: Vec3::ZERO,
                mass: spec.mass,
                inertia: Vec3::from(spec.inertia),
            },
            spec,
            prev_pos: pos,
            prev_rot: rot,
            wheels,
            steer: 0.0,
            prev_steer: 0.0,
            preload: 0.0,
            hop_was: false,
            drift: None,
            kick_t: 0.0,
            air_time: 0.0,
            grounded: 0,
            landing: None,
            upset: 0.0,
            righting: 0.0,
            last_good: None,
            good_t: 0.0,
            vis: [0.0; 4],
            prev_vis: [0.0; 2],
            prev_vel: Vec3::ZERO,
            rpm: 0.0,
            load: 0.0,
            boosting: false,
            wade: 0.0,
            stalled: false,
            still: 0.0,
            asleep: false,
            waiting: false,
            slopes: false,
            events: Events::default(),
        }
    }

    /// Take a new tune (the file changed), keeping where the car is.
    pub fn retune(&mut self, spec: Spec) {
        for (i, w) in self.wheels.iter_mut().enumerate() {
            w.mount = spec.mount(i);
        }
        self.b.mass = spec.mass;
        self.b.inertia = Vec3::from(spec.inertia);
        self.spec = spec;
        self.asleep = false;
    }

    pub fn pos(&self) -> Vec3 {
        self.b.pos
    }

    pub fn up(&self) -> Vec3 {
        self.b.rot * Vec3::Y
    }

    pub fn forward(&self) -> Vec3 {
        self.b.rot * Vec3::NEG_Z
    }

    pub fn heading(&self) -> f32 {
        yaw_of(self.b.rot)
    }

    /// Speed along the car's own forward, m/s.
    pub fn v_fwd(&self) -> f32 {
        self.b.vel.dot(self.forward())
    }

    /// The steering lock at this speed, radians.
    pub fn lock(&self) -> f32 {
        let s = &self.spec;
        let k = smoothstep(0.0, s.top_speed, self.v_fwd().abs());
        (s.lock + (s.lock_fast - s.lock) * k).to_radians()
    }

    /// The pose to draw, `alpha` of the way from the last step to this one.
    pub fn pose(&self, alpha: f32) -> (Vec3, Quat) {
        (self.prev_pos.lerp(self.b.pos, alpha), self.prev_rot.slerp(self.b.rot, alpha))
    }

    /// Put the car back on its wheels at the last place it had them all
    /// down, or upright where it is.
    pub fn reset(&mut self) {
        let (p, r) = self.last_good.unwrap_or((self.b.pos + Vec3::Y * 0.5, Quat::from_rotation_y(self.heading())));
        self.b.pos = p + Vec3::Y * 0.3;
        self.b.rot = r;
        self.b.vel = Vec3::ZERO;
        self.b.ang = Vec3::ZERO;
        self.prev_pos = self.b.pos;
        self.prev_rot = r;
        self.righting = 0.0;
        self.upset = 0.0;
        self.asleep = false;
    }

    /// One fixed step of `dt` seconds. `level` is the water's surface here.
    pub fn step(&mut self, c: &Controls, store: &Store, level: f32, slopes: bool, dt: f32) {
        self.slopes = slopes;
        self.prev_pos = self.b.pos;
        self.prev_rot = self.b.rot;
        self.prev_steer = self.steer;
        self.prev_vis = [self.vis[0], self.vis[2]];
        for w in &mut self.wheels {
            w.prev_t = w.t;
            w.prev_roll = w.roll;
        }
        self.events = Events::default();

        // The world hasn't streamed in under or ahead of the car: wait at the
        // edge of it, as the bike does.
        let v = |p: Vec3| ((p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32);
        let ahead = self.b.pos + Vec3::new(self.b.vel.x, 0.0, self.b.vel.z) * 0.6;
        let (ax, ay, az) = v(ahead);
        let (px, py, pz) = v(self.b.pos);
        self.waiting = !store.loaded(px, py, pz) || !store.loaded(ax, ay, az);
        // Paused there, keeping its speed: it carries on as it was once the
        // ground arrives, instead of stopping dead in the middle of a lap.
        if self.waiting {
            return;
        }

        self.boosting = c.boost || self.kick_t > 0.0;
        let input = c.throttle > 0.0 || c.brake > 0.0 || c.steer != 0.0 || c.hop || c.handbrake;
        if self.asleep && !input && !self.ground_moved(store, level, slopes) {
            return;
        }
        self.asleep = false;

        // Steering, eased toward what is asked so a key press is not a step.
        // Drifting, the wheels stay turned into the drift: the keys only
        // tighten or widen it.
        let ask = match self.drift {
            Some(d) => d.dir * (0.5 + 0.5 * (c.steer * d.dir).clamp(-1.0, 1.0)),
            None => c.steer.clamp(-1.0, 1.0),
        };
        let want = -ask * self.lock();
        let rate = self.spec.steer_rate.to_radians() * dt;
        self.steer += (want - self.steer).clamp(-rate, rate);

        self.probe(store, level, slopes);
        let h = dt / SUBSTEPS as f32;
        let mut thump = 0.0f32;
        let mut crash = 0.0f32;
        for _ in 0..SUBSTEPS {
            let (t, k) = self.substep(c, store, level, h);
            thump = thump.max(t);
            crash = crash.max(k);
        }
        self.events.thump = thump;
        self.events.crash = crash;
        self.after(c, dt);
    }

    /// The ground under every wheel, once a step.
    fn probe(&mut self, store: &Store, level: f32, slopes: bool) {
        let s = &self.spec;
        let (r, travel) = (s.wheel_radius, s.travel);
        let up = self.up();
        let fwd = self.forward();
        for w in &mut self.wheels {
            let mount = self.b.pos + self.b.rot * w.mount;
            let hub = mount - up * w.t;
            // Look from a little above the tyre's top to well below full droop.
            let from = mount.y.max(hub.y + r * 0.6);
            let reach = (from - hub.y) + travel + r + 1.5;
            w.contact = wheel::probe(store, hub, fwd, r, from, reach, level, slopes);
        }
    }

    /// Whether the ground under a sleeping car has changed (dug out, built on).
    fn ground_moved(&self, store: &Store, level: f32, slopes: bool) -> bool {
        let s = &self.spec;
        let up = self.up();
        self.wheels.iter().any(|w| {
            let mount = self.b.pos + self.b.rot * w.mount;
            let hub = mount - up * w.t;
            let now = wheel::probe(
                store,
                hub,
                self.forward(),
                s.wheel_radius,
                mount.y.max(hub.y + s.wheel_radius * 0.6),
                s.travel + s.wheel_radius + 2.0,
                level,
                slopes,
            );
            match (now, w.contact) {
                (Some(a), Some(b)) => (a.point.y - b.point.y).abs() > 0.02,
                (None, None) => false,
                _ => true,
            }
        })
    }

    /// One substep: forces, integration, collision. Returns the hardest
    /// bump-stop hit and chassis strike, m/s.
    #[allow(clippy::needless_range_loop)] // four wheels read and written by index
    fn substep(&mut self, c: &Controls, store: &Store, level: f32, h: f32) -> (f32, f32) {
        let s = self.spec.clone();
        let g = s.gravity;
        let up = self.up();
        let fwd = self.forward();
        let mut f = Vec3::new(0.0, -g * s.mass, 0.0);
        let mut t = Vec3::ZERO;
        let v_fwd = self.b.vel.dot(fwd);
        let speed = self.b.vel.length();

        // Springs first: every tyre's load is needed before the grip.
        let mut springs: [Option<wheel::Spring>; 4] = [None, None, None, None];
        let mut thump = 0.0f32;
        for (i, w) in self.wheels.iter().enumerate() {
            let Some(con) = &w.contact else { continue };
            let corner = s.corner(w.rear);
            let mount = self.b.pos + self.b.rot * w.mount;
            let vm = self.b.point_vel(mount);
            springs[i] = wheel::suspend(con, mount, up, vm, s.wheel_radius, s.travel, corner.k, corner.mass, (s.damp_bump, s.damp_rebound, s.damp_knee), h);
            if let Some(sp) = &springs[i] {
                if sp.x > s.travel {
                    thump = thump.max(sp.x_dot);
                }
            }
        }
        // Anti-roll bars: load moved from the less compressed side to the more.
        for axle in [0usize, 2] {
            if let (Some(l), Some(r)) = (springs[axle].as_ref().map(|l| (l.x, l.force)), springs[axle + 1].as_ref().map(|r| (r.x, r.force))) {
                let k = s.anti_roll * s.corner(axle == 2).k * (l.0 - r.0);
                springs[axle].as_mut().unwrap().force = (l.1 + k).max(0.0);
                springs[axle + 1].as_mut().unwrap().force = (r.1 - k).max(0.0);
            }
        }
        // A held hop loads the springs: the car squats into them.
        if c.hop && self.grounded > 0 && s.drift_boost <= 0.0 {
            f -= up * s.mass * g * 0.35 * self.preload;
        }

        // The drivetrain, shared among the driven wheels that are down.
        let depth = self.wheels.iter().filter_map(|w| w.contact.map(|c| c.depth)).fold(0.0f32, f32::max);
        let r = s.wheel_radius;
        let drown = (1.0 - (depth - r) / (0.5 * r)).clamp(0.0, 1.0);
        self.stalled = drown <= 0.0;
        let top = s.top_speed + if self.boosting { s.boost_speed } else { 0.0 };
        let mut drive = 0.0;
        let mut brake = 0.0;
        if c.throttle > 0.0 {
            if v_fwd < -0.8 {
                brake = c.throttle;
            } else {
                drive = s.force.min(s.power * 1000.0 / v_fwd.abs().max(1.0)) * c.throttle * ((top - v_fwd) / 3.0).clamp(0.0, 1.0);
                if self.boosting && v_fwd < top {
                    drive += s.boost_force;
                }
            }
        }
        if c.brake > 0.0 {
            if v_fwd > 0.8 {
                brake = brake.max(c.brake);
            } else if c.throttle == 0.0 {
                drive = -s.force * 0.6 * c.brake * ((s.reverse_speed + v_fwd) / 2.0).clamp(0.0, 1.0);
            }
        }
        drive *= drown;
        let driven: Vec<usize> = (0..4).filter(|&i| springs[i].is_some() && (s.driven == spec::Driven::All || self.wheels[i].rear)).collect();
        // Nothing asked and barely moving: the brakes hold it, as an automatic
        // with a hill hold does, so a parked car stays parked on a dune.
        let hold = c.throttle == 0.0 && c.brake == 0.0 && speed < 2.0;
        let brake_total = s.brake * 9.81 * s.mass * brake;

        let corner_m = s.mass * 0.25;
        for i in 0..4 {
            let Some(sp) = &springs[i] else { continue };
            let w = &self.wheels[i];
            let con = w.contact.unwrap();
            let mount = self.b.pos + self.b.rot * w.mount;
            let hub = mount - up * sp.t;
            let n = con.n;
            let patch = hub - n * r;
            // The tyre's push acts a little above the patch: less leverage
            // to roll the car over than a real tyre has.
            let lift = s.roll_lever * (self.b.pos - patch).dot(n).max(0.0);
            let at = patch + n * lift;
            f += up * sp.force;
            t += (hub - self.b.pos).cross(up * sp.force);

            let steer = if w.rear { 0.0 } else { self.steer };
            let heading = self.b.rot * (Quat::from_rotation_y(steer) * Vec3::NEG_Z);
            let fl = (heading - n * heading.dot(n)).normalize_or(fwd);
            let sd = n.cross(fl).normalize_or(self.b.rot * Vec3::X);
            let feel = con.surface.feel();
            // Water takes the bite out of a tyre and drags at it.
            let wet = (con.depth / r).clamp(0.0, 1.0);
            let loose = if w.rear && self.drift.is_some() { s.drift_grip } else { 1.0 };
            let mu = s.grip * feel.grip * (1.0 - 0.3 * wet) * loose;
            let v = self.b.point_vel(at);
            let plane = feel.drag + feel.soft / (1.0 + v.length() / 6.0) + 3.0 * wet;
            let share = if driven.contains(&i) {
                let rear_share: f32 = if s.driven != spec::Driven::All {
                    1.0
                } else if w.rear {
                    1.2
                } else {
                    0.8
                };
                drive / driven.len() as f32 * rear_share
            } else {
                0.0
            };
            let bshare = if w.rear { 0.4 } else { 0.6 } * 0.5;
            let ask = grip::Ask {
                drive: share,
                brake: brake_total * bshare,
                locked: c.handbrake && w.rear,
                hold,
            };
            let gr = grip::slip(&s, ask, sp.force, v, fl, sd, mu, feel.peak, plane, corner_m, h);
            f += gr.force;
            t += (at - self.b.pos).cross(gr.force);
            let w = &mut self.wheels[i];
            w.slip = gr.slip;
            w.slide = gr.slide;
            w.spin = gr.spin;
            w.omega = if w.rear && c.handbrake { 0.0 } else { gr.v_long / r * (1.0 + gr.spin * 2.0) };
        }
        for (i, w) in self.wheels.iter_mut().enumerate() {
            match &springs[i] {
                Some(sp) => {
                    w.t = sp.t;
                    w.x = sp.x;
                    w.x_dot = sp.x_dot;
                    w.load = sp.force;
                    w.down = true;
                }
                None => {
                    // Off the ground: the wheel drops to full droop on its spring.
                    w.t += (s.travel - w.t) * (h * 12.0).min(1.0);
                    w.x = 0.0;
                    w.x_dot = 0.0;
                    w.load = 0.0;
                    w.down = false;
                    w.slide = 0.0;
                    w.slip = 0.0;
                    w.spin = if c.throttle > 0.0 && (s.driven == spec::Driven::All || w.rear) { 1.0 } else { 0.0 };
                    w.omega *= 1.0 - (h * 0.5).min(1.0);
                    if w.spin > 0.0 {
                        w.omega += (s.top_speed / r - w.omega) * (h * 1.5).min(1.0);
                    }
                }
            }
            w.roll = (w.roll + w.omega * h) % std::f32::consts::TAU;
        }
        let down = springs.iter().filter(|s| s.is_some()).count();

        // Air drag, and down-force with the wheels down.
        f -= self.b.vel * self.b.vel.length() * s.drag;
        if down >= 3 {
            f -= up * s.downforce * v_fwd * v_fwd;
        }
        // Water: drag, and the car floats once it is deep enough.
        let floor = self.b.pos.y - (r + s.com_height) + 0.1;
        let sub = (level - floor).clamp(0.0, 1.6);
        self.wade = sub;
        if sub > 0.0 {
            f += Vec3::Y * s.mass * g * (sub / 1.3) * 0.95;
            f -= self.b.vel * s.mass * 1.2 * sub.min(1.0);
            t -= self.b.ang * s.inertia[0] * 2.0 * sub.min(1.0);
        }

        // Assists.
        if let Some(d) = self.drift.filter(|_| down >= 2) {
            // Held at the drift's angle to the way it is going: the keys
            // tighten it (more angle) or widen it, and the nose is steered
            // to that angle rather than let go, so it neither spins nor
            // scrubs off its speed.
            let flat = Vec3::new(self.b.vel.x, 0.0, self.b.vel.z);
            if flat.length() > 2.0 {
                let (vd, f) = (flat.normalize(), Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(fwd));
                let slip = vd.cross(f).y.atan2(vd.dot(f));
                let angle = s.drift_angle.to_radians() * (0.75 + 0.35 * (c.steer * d.dir).clamp(-1.0, 1.0));
                let r = self.b.ang.dot(up);
                let r_path = v_fwd * self.steer.tan() / s.wheelbase;
                t += up * ((-d.dir * angle - slip) * 45.0 + (r_path - r) * 6.0) * s.inertia[1];
            }
        } else {
            t += assist::yaw_damp(&s, &self.b, self.steer, v_fwd);
        }
        // Kept on with only one axle down: over a crest the front leaves
        // first and the rear springs, still loaded, kick the tail up, and
        // undamped that is a nose-first landing.
        if down >= 1 {
            t += assist::settle(&s, &self.b);
        }
        {
            let (sum, k) = self.wheels.iter().filter_map(|w| w.contact.map(|c| c.n)).fold((Vec3::ZERO, 0), |(a, k), n| (a + n, k + 1));
            let n = if k > 0 { sum.normalize_or(Vec3::Y) } else { Vec3::Y };
            t += assist::upright(&s, &self.b, n);
        }
        if down == 0 {
            let below = self.wheels.iter().find_map(|w| w.contact.map(|c| c.n));
            t += assist::air(&s, &self.b, c, below);
        }
        if self.righting > 0.0 {
            let (rt, lift) = assist::right_up(&s, &self.b);
            t += rt;
            f += lift;
        }

        self.b.integrate(f, t, h);
        // A drift carries its speed: it is shed at 1.5 m/s a second, not
        // scrubbed off by the sliding tyres.
        if let Some(d) = self.drift.as_mut().filter(|_| down >= 2) {
            d.keep = (d.keep - 1.5 * h).min(top);
            let flat = Vec3::new(self.b.vel.x, 0.0, self.b.vel.z);
            let speed = flat.length();
            if speed > 1.0 && speed < d.keep {
                let lift = flat * (d.keep / speed - 1.0);
                self.b.vel += lift;
            }
        }
        // The tyres meet what stands beside them (a trunk, a post) as the
        // chassis does: a sphere in each wheel, its bottom well above the
        // ground the wheel rolls on, so it never takes the wheel's load.
        let r = s.wheel_radius;
        let tyres = self.wheels.iter().map(|w| [w.mount.x, w.mount.y - w.t + 0.3 * r, w.mount.z, 0.7 * r]);
        let spheres: Vec<[f32; 4]> = s.collide.iter().copied().chain(tyres).collect();
        let crash = self.b.collide(store, &spheres, self.slopes);
        // Foliage drags at the chassis as it pushes through.
        let centre = self.b.pos + up * 0.6;
        if matches!(
            store.cell((centre.x * 4.0).floor() as i32, (centre.y * 4.0).floor() as i32, (centre.z * 4.0).floor() as i32),
            Cell::Foliage
        ) {
            self.b.vel *= 1.0 - (1.5 * h).min(1.0);
        }
        (thump, crash)
    }

    /// Bookkeeping once a step: landings, the hop, righting, the engine.
    fn after(&mut self, c: &Controls, dt: f32) {
        let down = self.wheels.iter().filter(|w| w.down).count();
        let was = self.grounded;
        self.grounded = down;
        let up = self.up();
        let fwd = self.forward();

        // The hop: on a kart a hop and drift (kart_hop); otherwise Space held
        // loads the springs and letting go pops.
        if self.spec.drift_boost > 0.0 {
            self.kart_hop(c, down, dt);
        } else {
            if c.hop && down >= 2 {
                self.preload = (self.preload + dt / 0.4).min(1.0);
            }
            if !c.hop && self.hop_was && down >= 2 {
                let pop = self.spec.hop + (self.spec.hop_loaded - self.spec.hop) * self.preload;
                self.b.vel += up * pop;
                self.preload = 0.0;
            } else if !c.hop {
                self.preload = 0.0;
            }
            self.hop_was = c.hop;
        }
        let s = &self.spec;

        // Air time, and how the landing went.
        if down == 0 {
            self.air_time += dt;
        } else if was == 0 && self.air_time > 0.0 {
            let impact = self.wheels.iter().map(|w| w.x_dot).fold(0.0f32, f32::max);
            self.landing = Some((0.0, impact, self.air_time));
            self.air_time = 0.0;
            if impact > 3.0 {
                self.events.thump = self.events.thump.max(impact);
            }
        }
        if let Some((t, impact, air)) = &mut self.landing {
            *t += dt;
            *impact = self.wheels.iter().map(|w| w.x_dot).fold(*impact, f32::max);
            if down >= 3 && *t < 0.3 {
                if *air > 0.5 {
                    if *impact < s.clean_impact && up.y > 0.8 {
                        self.b.vel += fwd * s.clean_kick;
                        self.events.clean_landing = true;
                        self.events.note = Some(format!("CLEAN LANDING   {:.1} S AIR", air));
                    } else {
                        self.events.note = Some(format!("{:.1} S AIR", air));
                    }
                }
                self.landing = None;
            } else if *t >= 0.3 {
                self.landing = None;
            }
        }

        // On its side or roof and nearly stopped for a second: turn it back.
        let slow = self.b.vel.length() < 2.5;
        if up.y < 0.4 && slow && down < 3 {
            self.upset += dt;
        } else {
            self.upset = 0.0;
        }
        if self.upset > 1.0 && self.righting <= 0.0 {
            self.righting = 3.0;
            self.events.note = Some("RIGHTING".into());
        }
        if self.righting > 0.0 {
            self.righting -= dt;
            if up.y > 0.92 {
                self.righting = 0.0;
            }
        }
        // Remember the last place it stood properly, to be put back there.
        if down == 4 && up.y > 0.85 {
            self.good_t += dt;
            if self.good_t > 0.5 {
                self.good_t = 0.0;
                self.last_good = Some((self.b.pos, Quat::from_rotation_y(self.heading())));
            }
        }

        // The drawn chassis leans with the acceleration, on a spring.
        let acc = self.b.rot.inverse() * ((self.b.vel - self.prev_vel) / dt);
        self.prev_vel = self.b.vel;
        let g = 9.81;
        let roll_to = if down > 0 { -acc.x / g * s.visual_roll } else { 0.0 };
        let pitch_to = if down > 0 { acc.z / g * s.visual_pitch } else { 0.0 };
        let spring = |x: f32, v: f32, to: f32| {
            let w = std::f32::consts::TAU * 1.8;
            let v = v + (-(x - to) * w * w - 2.0 * 0.5 * w * v) * dt;
            ((x + v * dt).clamp(-0.12, 0.12), v)
        };
        (self.vis[0], self.vis[1]) = spring(self.vis[0], self.vis[1], roll_to.clamp(-0.12, 0.12));
        (self.vis[2], self.vis[3]) = spring(self.vis[2], self.vis[3], pitch_to.clamp(-0.1, 0.1));

        // The engine: revs through four gears with the road speed, flaring
        // when the wheels spin or leave the ground under throttle.
        let frac = (self.v_fwd().abs() / s.top_speed).clamp(0.0, 1.2);
        let g4 = (frac * 4.0).min(3.95);
        let in_gear = g4 - g4.floor();
        let spin = self.wheels.iter().filter(|w| w.rear || s.driven == spec::Driven::All).map(|w| w.spin).fold(0.0f32, f32::max);
        let want = if self.stalled {
            0.0
        } else {
            let base = if frac < 0.02 { 0.0 } else { 0.25 + 0.75 * in_gear };
            (base + c.throttle * 0.35 * (1.0 - base) + spin * c.throttle * 0.6).min(1.0)
        };
        self.rpm += (want - self.rpm) * (dt * 6.0).min(1.0);
        let load_to = if self.stalled { 0.0 } else { c.throttle.max(c.brake * 0.5) };
        self.load += (load_to - self.load) * (dt * 8.0).min(1.0);

        // Asleep: still, on its wheels, nothing asked. Woken by input or by
        // the ground under it changing.
        let input = c.throttle > 0.0 || c.brake > 0.0 || c.steer != 0.0 || c.hop;
        if !input && down == 4 && self.b.vel.length() < 0.05 && self.b.ang.length() < 0.05 && self.wade <= 0.0 {
            self.still += dt;
            if self.still > 0.5 {
                self.asleep = true;
                self.b.vel = Vec3::ZERO;
                self.b.ang = Vec3::ZERO;
            }
        } else {
            self.still = 0.0;
        }
    }

    /// A kart's Space: a hop on the press, and held with the steering over
    /// at speed, a drift that charges boost; letting go fires it.
    fn kart_hop(&mut self, c: &Controls, down: usize, dt: f32) {
        let (charge, boost, hop) = (self.spec.drift_charge, self.spec.drift_boost, self.spec.hop);
        let (up, fwd) = (self.up(), self.forward());
        if c.hop && !self.hop_was && down >= 2 {
            self.b.vel += up * hop;
        }
        self.hop_was = c.hop;
        self.kick_t = (self.kick_t - dt).max(0.0);
        let v = self.v_fwd();
        match &mut self.drift {
            None => {
                if c.hop && down >= 3 && c.steer.abs() > 0.3 && v > 8.0 {
                    let keep = Vec3::new(self.b.vel.x, 0.0, self.b.vel.z).length();
                    self.drift = Some(Drift { dir: c.steer.signum(), t: 0.0, keep });
                }
            }
            Some(d) => {
                if down >= 2 {
                    d.t += dt;
                }
                if !c.hop || v < 5.0 {
                    let tiers = (d.t / charge.max(0.1)).floor().min(2.0);
                    if !c.hop && tiers > 0.0 {
                        self.b.vel += fwd * boost * tiers;
                        self.kick_t = 0.35 * tiers;
                    }
                    self.drift = None;
                }
            }
        }
    }

    /// The boost a drift has charged so far: 0, 1 or 2.
    pub fn drift_tier(&self) -> u32 {
        self.drift.map_or(0, |d| (d.t / self.spec.drift_charge.max(0.1)).floor().min(2.0) as u32)
    }

    /// The drawn chassis' extra lean, `alpha` of the way through the step.
    pub fn visual_lean(&self, alpha: f32) -> (f32, f32) {
        let l = |a: f32, b: f32| a + (b - a) * alpha;
        (l(self.prev_vis[0], self.vis[0]), l(self.prev_vis[1], self.vis[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::{DIRT, GRASS, STONE};
    use crate::world::store::Voxels;

    /// Ground at `top(x, z)` metres with `skin` on top, over chunk ranges
    /// (8 m chunks) in x and z.
    fn land(top: impl Fn(f32, f32) -> f32, skin: u8, cxs: (i32, i32), czs: (i32, i32)) -> Store {
        let mut s = Store::default();
        for cz in czs.0..czs.1 {
            for cx in cxs.0..cxs.1 {
                for cy in 0..12 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in cxs.0 * 32..cxs.1 * 32 {
            for z in czs.0 * 32..czs.1 * 32 {
                let h = (top(x as f32 / 4.0 + 0.125, z as f32 / 4.0 + 0.125) * 4.0).round() as i32;
                for y in 0..h.max(1) {
                    s.set(x, y, z, if y + 1 == h { skin } else { STONE });
                }
            }
        }
        s
    }

    fn run(v: &mut Vehicle, s: &Store, c: Controls, secs: f32, slopes: bool, mut each: impl FnMut(&Vehicle, f32)) {
        let dt = 1.0 / 60.0;
        for i in 0..(secs / dt) as i32 {
            v.step(&c, s, -100.0, slopes, dt);
            each(v, i as f32 * dt);
        }
    }

    /// Yaw -90 degrees faces +X.
    const EAST: f32 = -std::f32::consts::FRAC_PI_2;

    /// Dropped from a metre, it settles and is still within a second or so,
    /// without bouncing away or creeping.
    #[test]
    fn a_dropped_car_comes_to_rest() {
        let s = land(|_, _| 4.0, DIRT, (-2, 2), (-2, 2));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 5.0, 0.0), EAST);
        let mut peak_after = 0.0f32;
        run(&mut v, &s, Controls::default(), 2.0, false, |v, t| {
            if t > 1.2 {
                peak_after = peak_after.max(v.b.vel.length());
            }
        });
        assert!(peak_after < 0.15, "still moving at {peak_after} m/s after 1.2 s");
        assert_eq!(v.grounded, 4);
        assert!(v.up().y > 0.99);
        let at = v.pos();
        assert!(at.x.abs() < 0.1 && at.z.abs() < 0.1, "drifted to {at}");
        // Resting on its springs, the belly clear of the ground.
        let floor = at.y - 4.0;
        assert!(floor > 0.2 && floor < 0.5, "centre of mass {floor} m up");
    }

    /// Parked on a 20 degree slope, pointing up it or across it, it stays.
    #[test]
    fn parked_on_a_slope_it_does_not_creep() {
        let tan = 20f32.to_radians().tan();
        let s = land(|x, _| 30.0 + x * tan, GRASS, (-3, 3), (-3, 3));
        for (yaw, name) in [(EAST, "up"), (0.0, "across")] {
            let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 30.4, 0.0), yaw);
            run(&mut v, &s, Controls::default(), 1.5, true, |_, _| {});
            let a = v.pos();
            run(&mut v, &s, Controls::default(), 3.0, true, |_, _| {});
            let moved = (v.pos() - a).length();
            assert!(moved < 0.05, "{name} the slope: crept {moved} m in 3 s");
        }
    }

    /// From rest on hardpack: 0 to 60 km/h and the top speed in a band.
    #[test]
    fn pace_is_in_its_band() {
        let s = land(|_, _| 2.0, DIRT, (-1, 120), (-1, 1));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 2.0, 0.0), EAST);
        run(&mut v, &s, Controls::default(), 0.5, false, |_, _| {});
        let mut sixty = None;
        let mut top = 0.0f32;
        run(&mut v, &s, Controls { throttle: 1.0, ..Default::default() }, 22.0, false, |v, t| {
            if sixty.is_none() && v.v_fwd() > 60.0 / 3.6 {
                sixty = Some(t);
            }
            top = top.max(v.v_fwd());
        });
        let sixty = sixty.expect("never reached 60 km/h");
        assert!((1.8..4.5).contains(&sixty), "0-60 km/h in {sixty} s");
        assert!((27.0..32.0).contains(&top), "top speed {top} m/s");
        assert!(v.up().y > 0.98 && v.pos().z.abs() < 1.0, "wandered: z {}", v.pos().z);
    }

    /// Flat at 4 m, then a kicker: 2 m curving up into a 30% ramp that
    /// tops out 1.8 m up at x = 48, and a drop back to the flat.
    fn ramp_at(x: f32, _z: f32) -> f32 {
        if (40.0..42.0).contains(&x) {
            let t = x - 40.0;
            4.0 + 0.3 * t * t / 4.0
        } else if (42.0..48.0).contains(&x) {
            4.3 + (x - 42.0) * 0.3
        } else {
            4.0
        }
    }

    /// Prints a ramp run, for tuning: `cargo test ramp_trace -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn ramp_trace() {
        let slopes = std::env::var("SLOPES").is_ok();
        let s = land(ramp_at, DIRT, (-1, 30), (-2, 2));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 4.0, 0.0), EAST);
        run(&mut v, &s, Controls::default(), 0.5, slopes, |_, _| {});
        v.b.vel = v.forward() * (60.0 / 3.6);
        run(&mut v, &s, Controls::default(), 4.0, slopes, |v, t| {
            let f = v.forward();
            println!(
                "t {t:4.2} x {:6.2} y {:5.2} v {:5.2} pitch {:6.3} up.y {:5.2} down {} x {:?}",
                v.pos().x,
                v.pos().y,
                v.v_fwd(),
                f.y.asin(),
                v.up().y,
                v.grounded,
                v.wheels.map(|w| (w.x * 100.0).round() / 100.0)
            );
        });
    }

    /// A dune: flat at 4 m, then a smooth hump `high` metres up over `long`
    /// metres starting at x = 40.
    fn dune(high: f32, long: f32) -> impl Fn(f32, f32) -> f32 {
        move |x, _| {
            let t = (x - 40.0) / long;
            if (0.0..1.0).contains(&t) {
                4.0 + high * 0.5 * (1.0 - (t * std::f32::consts::TAU).cos())
            } else {
                4.0
            }
        }
    }

    /// Over a dune at `kmh` with the throttle held: the worst nose-down
    /// pitch against the ground under it on touching down, degrees, and the
    /// least up.y.
    fn dune_run(spec: &Spec, high: f32, long: f32, kmh: f32, trace: bool) -> (f32, f32) {
        let s = land(dune(high, long), DIRT, (-1, 30), (-2, 2));
        let mut v = Vehicle::new(spec.clone(), Vec3::new(10.0, 4.0, 0.0), EAST);
        run(&mut v, &s, Controls::default(), 0.5, true, |_, _| {});
        v.b.vel = v.forward() * (kmh / 3.6);
        let gas = Controls { throttle: 0.4, ..Default::default() };
        let (mut nose, mut min_up, mut was) = (0.0f32, 1.0f32, 4);
        let ground = dune(high, long);
        run(&mut v, &s, gas, 4.0, true, |v, t| {
            let pitch = v.forward().y.asin().to_degrees();
            if v.pos().x > 40.0 {
                if was == 0 && v.grounded > 0 {
                    let x = v.pos().x;
                    let slope = ((ground(x + 0.5, 0.0) - ground(x - 0.5, 0.0)) / 1.0).atan().to_degrees();
                    nose = nose.min(pitch - slope);
                }
                min_up = min_up.min(v.up().y);
            }
            was = v.grounded;
            if trace && v.pos().x > 36.0 && v.pos().x < 40.0 + long + 12.0 {
                println!("t {t:4.2} x {:6.2} y {:5.2} v {:5.2} pitch {pitch:6.1} down {} rate {:5.2} ground {:5.2}", v.pos().x, v.pos().y, v.v_fwd(), v.grounded, v.b.ang.dot(v.b.rot * Vec3::X), dune(high, long)(v.pos().x, 0.0));
            }
        });
        (nose, min_up)
    }

    /// Prints a dune run for a spec: `TUNE=beach_buggy KMH=35 cargo test dune_trace -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dune_trace() {
        let name = std::env::var("TUNE").unwrap_or_else(|_| "beach_buggy".into());
        let kmh: f32 = std::env::var("KMH").ok().and_then(|k| k.parse().ok()).unwrap_or(35.0);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let spec = Catalogue::load(&root).get(&name);
        let (nose, up) = dune_run(&spec, 1.5, 10.0, kmh, true);
        println!("nose {nose:.1} deg  min up {up:.2}");
        for (h, l) in [(1.0, 8.0), (1.5, 10.0), (2.0, 10.0), (2.5, 14.0)] {
            for kmh in [25.0, 35.0, 45.0, 60.0] {
                let (nose, up) = dune_run(&spec, h, l, kmh, false);
                println!("dune {h} m x {l} m at {kmh} km/h: nose {nose:5.1} deg  min up {up:.2}");
            }
        }
    }

    /// Tilted runs: a dune met at an angle, and full lock across a side
    /// slope, printing the least up.y and the biggest roll rate.
    /// `TUNE=beach_buggy cargo test tilt_trace -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tilt_trace() {
        let name = std::env::var("TUNE").unwrap_or_else(|_| "beach_buggy".into());
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let spec = Catalogue::load(&root).get(&name);
        let verbose = std::env::var("V").ok();
        for (h, l) in [(1.0f32, 8.0f32), (1.5, 10.0), (2.0, 10.0)] {
            for angle in [20.0f32, 40.0, 60.0] {
                for kmh in [35.0f32, 50.0, 70.0] {
                    // The ridge runs at `angle` off square to the track.
                    let (sa, ca) = angle.to_radians().sin_cos();
                    let s = land(move |x, z| dune(h, l)(40.0 + (x - 40.0) * ca + z * sa, 0.0), DIRT, (-1, 30), (-6, 6));
                    let mut v = Vehicle::new(spec.clone(), Vec3::new(10.0, 4.0, 0.0), EAST);
                    run(&mut v, &s, Controls::default(), 0.5, true, |_, _| {});
                    v.b.vel = v.forward() * (kmh / 3.6);
                    let gas = Controls { throttle: 0.4, ..Default::default() };
                    let (mut min_up, mut rr) = (1.0f32, 0.0f32);
                    let tag = format!("{h}/{angle}/{kmh}");
                    run(&mut v, &s, gas, 4.0, true, |v, t| {
                        min_up = min_up.min(v.up().y);
                        rr = rr.max(v.b.ang.dot(v.forward()).abs());
                        if verbose.as_deref() == Some(tag.as_str()) {
                            let r = v.b.rot * Vec3::X;
                            println!("t {t:4.2} x {:6.2} z {:5.2} up {:5.2} roll {:6.1} pitch {:6.1} down {} w {:?} spd {:5.1}", v.pos().x, v.pos().z, v.up().y, r.y.asin().to_degrees(), v.forward().y.asin().to_degrees(), v.grounded, v.b.ang, v.b.vel.length());
                        }
                    });
                    println!("dune {h} m at {angle} deg, {kmh} km/h: min up {min_up:5.2}  roll rate {rr:4.1}");
                }
            }
        }
        // Full lock across a 15 degree side slope.
        for kmh in [30.0f32, 45.0, 60.0] {
            let tan = 15f32.to_radians().tan();
            let s = land(move |_, z| 20.0 + z * tan, DIRT, (-8, 8), (-8, 8));
            let mut v = Vehicle::new(spec.clone(), Vec3::new(0.0, 20.5, 0.0), EAST);
            run(&mut v, &s, Controls::default(), 0.5, true, |_, _| {});
            v.b.vel = v.forward() * (kmh / 3.6);
            let mut min_up = 1.0f32;
            for steer in [1.0f32, -1.0] {
                run(&mut v, &s, Controls { throttle: 0.6, steer, ..Default::default() }, 2.0, true, |v, _| min_up = min_up.min(v.up().y));
            }
            println!("lock on a side slope at {kmh} km/h: min up {min_up:5.2}");
        }
    }

    /// The beach buggy over a dune at 35-60 km/h, square or at an angle,
    /// lands on its wheels, not on its nose or over on its side.
    #[test]
    fn the_buggy_lands_dunes_on_its_wheels() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let spec = Catalogue::load(&root).get("beach_buggy");
        for kmh in [35.0, 45.0, 60.0] {
            let (nose, up) = dune_run(&spec, 1.5, 10.0, kmh, false);
            assert!(nose > -25.0 && up > 0.6, "{kmh} km/h: landed {nose} deg nose down, up.y {up}");
        }
        let s = land(|x, z| dune(1.5, 10.0)(40.0 + (x - 40.0) * 0.94 + z * 0.34, 0.0), DIRT, (-1, 30), (-6, 6));
        let mut v = Vehicle::new(spec, Vec3::new(10.0, 4.0, 0.0), EAST);
        run(&mut v, &s, Controls::default(), 0.5, true, |_, _| {});
        v.b.vel = v.forward() * (50.0 / 3.6);
        let mut min_up = 1.0f32;
        run(&mut v, &s, Controls { throttle: 0.4, ..Default::default() }, 4.0, true, |v, _| min_up = min_up.min(v.up().y));
        assert!(min_up > 0.6, "a dune at 20 degrees tipped it to up.y {min_up}");
    }

    /// A 30% ramp at 60 km/h is a jump, and the car comes down on its wheels.
    #[test]
    fn a_ramp_is_a_jump_that_lands() {
        let ramp = ramp_at;
        let s = land(ramp, DIRT, (-1, 30), (-2, 2));
        for slopes in [false, true] {
            let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 4.0, 0.0), EAST);
            run(&mut v, &s, Controls::default(), 0.5, slopes, |_, _| {});
            v.b.vel = v.forward() * (60.0 / 3.6);
            let (mut air, mut min_up) = (0.0f32, 1.0f32);
            run(&mut v, &s, Controls::default(), 5.0, slopes, |v, _| {
                air = air.max(v.air_time);
                min_up = min_up.min(v.up().y);
            });
            assert!(air > 0.4, "slopes {slopes}: {air} s in the air");
            assert!(min_up > 0.6, "slopes {slopes}: tipped to up.y {min_up}");
            assert_eq!(v.grounded, 4, "slopes {slopes}: not on its wheels");
        }
    }

    /// Full lock at top speed on hardpack slides or turns, but never rolls.
    #[test]
    fn full_lock_at_speed_does_not_roll_it() {
        let s = land(|_, _| 2.0, DIRT, (-12, 12), (-12, 12));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(-60.0, 2.0, 0.0), EAST);
        run(&mut v, &s, Controls::default(), 0.5, false, |_, _| {});
        v.b.vel = v.forward() * 28.0;
        let mut min_up = 1.0f32;
        let mut turned = 0.0f32;
        let h0 = v.heading();
        run(
            &mut v,
            &s,
            Controls {
                throttle: 1.0,
                steer: 1.0,
                ..Default::default()
            },
            3.0,
            false,
            |v, _| {
                min_up = min_up.min(v.up().y);
                turned = turned.max((v.heading() - h0).abs());
            },
        );
        assert!(min_up > 0.7, "tipped to up.y {min_up}");
        assert!(turned > 0.5, "turned only {turned} rad");
    }

    /// Off a cliff onto its roof, it is righted.
    #[test]
    fn an_upturned_car_is_righted() {
        let s = land(|_, _| 4.0, DIRT, (-2, 2), (-2, 2));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 6.0, 0.0), EAST);
        v.b.rot = Quat::from_rotation_z(std::f32::consts::PI) * v.b.rot;
        run(&mut v, &s, Controls::default(), 7.0, false, |_, _| {});
        assert!(v.up().y > 0.85, "still upset: up.y {}", v.up().y);
    }

    /// Driven into a wall it stops, and does not climb or tunnel through.
    #[test]
    fn a_wall_stops_it() {
        let s = land(|x, _| if x < 30.0 { 4.0 } else { 8.0 }, DIRT, (-1, 8), (-2, 2));
        let mut v = Vehicle::new(Spec::default(), Vec3::new(0.0, 4.0, 0.0), EAST);
        run(&mut v, &s, Controls { throttle: 1.0, ..Default::default() }, 6.0, false, |_, _| {});
        assert!(v.pos().x < 30.0, "through the wall: x {}", v.pos().x);
        assert!(v.pos().y < 6.0, "climbed it: y {}", v.pos().y);
    }

    /// A juniper's trunk hit at 10 to 25 km/h, square or on a corner, or a
    /// sagebrush's stem at 30 to 45 km/h, stops the car, turns it aside or
    /// is driven over: it does not roll the car or throw it in the air.
    /// `TRACE=1` prints each run.
    #[test]
    fn a_trunk_does_not_roll_it() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cat = Catalogue::load(&root);
        let trace = std::env::var("TRACE").is_ok();
        let mut worst = (1.0f32, String::new());
        let mut highest = (0.0f32, String::new());
        // (what, km/h, size in voxels, height in voxels)
        let cases = [("trunk", 10.0f32, 2, 12), ("trunk", 15.0, 2, 12), ("trunk", 25.0, 2, 12), ("stem", 30.0, 1, 3), ("stem", 45.0, 1, 3)];
        for name in ["beach_buggy", "sand_rail"] {
            let spec = cat.get(name);
            for slopes in [false, true] {
                for &(what, kmh, size, tall) in &cases {
                    for off in [-1.1f32, -0.9, -0.7, -0.4, 0.0] {
                        // On flat ground 2 m up, at x 20.
                        let mut s = land(|_, _| 2.0, DIRT, (-1, 6), (-2, 2));
                        let (tx, tz) = (80, (off * 4.0).round() as i32);
                        for x in tx..tx + size {
                            for z in tz - size / 2..tz + size - size / 2 {
                                for y in 8..8 + tall {
                                    s.set(x, y, z, STONE);
                                }
                            }
                        }
                        let mut v = Vehicle::new(spec.clone(), Vec3::new(10.0, 2.0, 0.0), EAST);
                        run(&mut v, &s, Controls::default(), 0.5, slopes, |_, _| {});
                        let y0 = v.pos().y;
                        v.b.vel = v.forward() * kmh / 3.6;
                        let gas = Controls { throttle: 0.3, ..Default::default() };
                        let (mut min_up, mut rise) = (1.0f32, 0.0f32);
                        run(&mut v, &s, gas, 4.0, slopes, |v, _| {
                            min_up = min_up.min(v.up().y);
                            rise = rise.max(v.pos().y - y0);
                        });
                        let line = format!("{name} slopes {slopes} {what} {kmh} km/h off {off}: min up.y {min_up:.2} rise {rise:.2} m, past x {:.1}", v.pos().x);
                        if trace {
                            eprintln!("{line}");
                        }
                        if min_up < worst.0 {
                            worst = (min_up, line.clone());
                        }
                        if rise > highest.0 {
                            highest = (rise, line);
                        }
                    }
                }
            }
        }
        assert!(worst.0 > 0.6, "tipped: {}", worst.1);
        assert!(highest.0 < 1.0, "thrown up: {}", highest.1);
    }

    /// How a library vehicle drives, for tuning it: pace, bumps, cornering
    /// and drifting, printed.
    /// `TUNE=beach_buggy cargo test vehicle_report -- --ignored --nocapture`;
    /// it reads the JSON each run, so edit and run again.
    #[test]
    #[ignore]
    fn vehicle_report() {
        let name = std::env::var("TUNE").unwrap_or_else(|_| "beach_buggy".into());
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let spec = Catalogue::load(&root).get(&name);
        let gas = Controls { throttle: 1.0, ..Default::default() };
        let roll_pitch = |v: &Vehicle| {
            let (r, f) = (v.b.rot * Vec3::X, v.b.rot * Vec3::NEG_Z);
            (r.y.asin().to_degrees(), f.y.asin().to_degrees())
        };

        // Pace on flat hardpack.
        let flat = land(|_, _| 2.0, DIRT, (-1, 120), (-1, 1));
        let mut v = Vehicle::new(spec.clone(), Vec3::new(0.0, 2.0, 0.0), EAST);
        run(&mut v, &flat, Controls::default(), 0.5, false, |_, _| {});
        let (mut t100, mut top) = (None, 0.0f32);
        run(&mut v, &flat, gas, 20.0, false, |v, t| {
            if t100.is_none() && v.v_fwd() > 100.0 / 3.6 {
                t100 = Some(t);
            }
            top = top.max(v.v_fwd());
        });
        eprintln!("PACE   0-100 km/h {:?} s   top {:.0} km/h", t100.map(|t| (t * 10.0).round() / 10.0), top * 3.6);

        // One voxel-high bump, a metre and a half long, across the track or
        // under the left wheels only, hit at speed with the throttle off.
        for (label, left_only) in [("BUMP", false), ("SIDE BUMP", true)] {
            let ground = land(
                move |x, z| {
                    let on = (40.0..41.5).contains(&x) && (!left_only || z < 0.0);
                    if on {
                        4.25
                    } else {
                        4.0
                    }
                },
                DIRT,
                (-1, 30),
                (-2, 2),
            );
            let mut out = Vec::new();
            for kmh in [54.0f32, 108.0] {
                let mut v = Vehicle::new(spec.clone(), Vec3::new(10.0, 4.0, 0.0), EAST);
                run(&mut v, &ground, Controls::default(), 0.5, true, |_, _| {});
                v.b.vel = v.forward() * kmh / 3.6;
                let rest = v.pos().y;
                let (mut roll, mut pitch, mut rise, mut air, mut stops) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0);
                let keep = Controls { throttle: 0.0, ..Default::default() };
                let speed = kmh / 3.6;
                // Hold the speed so both cases meet the bump the same way.
                let secs = (60.0 - 10.0) / speed;
                run(&mut v, &ground, keep, secs, true, |v, _| {
                    let (ro, pi) = roll_pitch(v);
                    roll = roll.max(ro.abs());
                    pitch = pitch.max(pi.abs());
                    rise = rise.max(v.pos().y - rest);
                    if v.grounded < 4 && v.pos().x > 38.0 {
                        air += 1.0 / 60.0;
                    }
                    stops += (v.events.thump > 0.5) as i32;
                });
                out.push(format!("{kmh:3.0}: roll {roll:4.1} pitch {pitch:4.1} deg  rise {rise:.2} m  wheel off {air:.2} s  stops {stops}"));
            }
            eprintln!("{label:9} {}", out.join("  |  "));
        }

        // Rough ground: bumps of a few voxels, every few metres, both ways.
        for (label, amp) in [("ROLLING", 0.12f32), ("ROUGH", 0.25)] {
            let rough = land(
                move |x, z| 4.0 + amp * ((x * 0.7).sin() * (z * 0.9 + 1.0).sin() + 0.5 * (x * 1.9 + z * 0.4).sin()),
                DIRT,
                (-1, 60),
                (-2, 2),
            );
            let mut v = Vehicle::new(spec.clone(), Vec3::new(2.0, 6.0, 0.0), EAST);
            run(&mut v, &rough, Controls::default(), 1.0, true, |_, _| {});
            let (mut n, mut rr, mut pr, mut max_roll, mut max_pitch, mut acc, mut air) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut last_vy = v.b.vel.y;
            let mut min_up = 1.0f32;
            let (mut hits, mut stops, mut vsum) = (0, 0, 0.0f32);
            run(&mut v, &rough, gas, 10.0, true, |v, t| {
                if t < 2.0 || v.pos().x > 440.0 {
                    return;
                }
                vsum += v.v_fwd();
                hits += (v.events.crash > 0.5) as i32;
                stops += (v.events.thump > 0.5) as i32;
                let up = v.up();
                let (r, f) = (v.b.rot * Vec3::X, v.b.rot * Vec3::NEG_Z);
                rr += v.b.ang.dot(f).powi(2);
                pr += v.b.ang.dot(r).powi(2);
                let (ro, pi) = roll_pitch(v);
                max_roll = max_roll.max(ro.abs());
                max_pitch = max_pitch.max(pi.abs());
                acc += ((v.b.vel.y - last_vy) * 60.0).powi(2);
                last_vy = v.b.vel.y;
                min_up = min_up.min(up.y);
                if v.grounded == 0 {
                    air += 1.0 / 60.0;
                }
                n += 1.0;
            });
            let n = n.max(1.0);
            eprintln!(
                "{label:7} mean {:.0} km/h  roll rate {:.2}  pitch rate {:.2} rad/s rms  max roll {:.0}  pitch {:.0} deg  vert acc {:.1} m/s2 rms  air {:.1} s  min up {:.2}  strikes {hits}  stops {stops}",
                vsum / n * 3.6,
                (rr / n).sqrt(),
                (pr / n).sqrt(),
                max_roll,
                max_pitch,
                (acc / n).sqrt(),
                air,
                min_up
            );
        }

        // Cornering on a wide flat: full lock at three speeds.
        let pad = land(|_, _| 2.0, DIRT, (-12, 12), (-12, 12));
        for kmh in [40.0f32, 70.0, 100.0] {
            let mut v = Vehicle::new(spec.clone(), Vec3::new(-60.0, 2.0, 0.0), EAST);
            run(&mut v, &pad, Controls::default(), 0.3, false, |_, _| {});
            v.b.vel = v.forward() * kmh / 3.6;
            let (mut max_roll, mut min_up, mut yaw, mut slip) = (0.0f32, 1.0f32, 0.0f32, 0.0f32);
            let hold = Controls {
                throttle: 0.6,
                steer: 1.0,
                ..Default::default()
            };
            run(&mut v, &pad, hold, 2.5, false, |v, t| {
                max_roll = max_roll.max(roll_pitch(v).0.abs());
                min_up = min_up.min(v.up().y);
                if t > 1.0 {
                    yaw = yaw.max(v.b.ang.y.abs());
                    let vel = Vec3::new(v.b.vel.x, 0.0, v.b.vel.z);
                    if vel.length() > 2.0 {
                        slip = slip.max(vel.normalize().dot(v.forward()).clamp(-1.0, 1.0).acos().to_degrees());
                    }
                }
            });
            eprintln!(
                "TURN {kmh:3.0}  max roll {:.0} deg  min up {:.2}  yaw rate {:.2} rad/s  body slip {:.0} deg  end speed {:.0} km/h",
                max_roll,
                min_up,
                yaw,
                slip,
                v.b.vel.length() * 3.6
            );
        }

        // A kart's drift: at 60 km/h, Space held with the steering hard over
        // for 2 s (hopping in), then let go. Does it hold an angle without
        // spinning, charge both tiers, and fire the boost?
        if spec.drift_boost > 0.0 {
            let mut v = Vehicle::new(spec.clone(), Vec3::new(-60.0, 2.0, 0.0), EAST);
            run(&mut v, &pad, Controls::default(), 0.3, false, |_, _| {});
            v.b.vel = v.forward() * 60.0 / 3.6;
            let (mut peak, mut sum, mut n, mut tier, mut spun) = (0.0f32, 0.0f32, 0.0f32, 0u32, false);
            let hold = Controls { throttle: 1.0, steer: 0.8, hop: true, ..Default::default() };
            run(&mut v, &pad, hold, 2.0, false, |v, t| {
                let vel = Vec3::new(v.b.vel.x, 0.0, v.b.vel.z);
                if vel.length() > 2.0 {
                    let a = vel.normalize().dot(v.forward()).clamp(-1.0, 1.0).acos().to_degrees();
                    peak = peak.max(a);
                    spun |= a > 90.0;
                    if t > 0.5 {
                        sum += a;
                        n += 1.0;
                    }
                }
                tier = tier.max(v.drift_tier());
            });
            let before = v.v_fwd();
            let drifting = v.drift.is_some();
            run(&mut v, &pad, Controls { throttle: 1.0, ..Default::default() }, 0.1, false, |_, _| {});
            eprintln!(
                "HOP DRIFT  in a drift at the end {drifting}  mean slip {:.0}  peak {:.0} deg  spun {spun}  tier {tier}  {:.0} -> {:.0} km/h on letting go",
                sum / n.max(1.0),
                peak,
                before * 3.6,
                v.v_fwd() * 3.6
            );
        }

        // A drift as Look steering drives one: at 70 km/h a flick of the
        // handbrake, then the view swings round a corner at `turn` rad/s with
        // the throttle held, and the wheels point where the view does, as
        // ride/drive.rs steers them. Then the view straightens.
        for turn in [0.6f32, 1.0] {
            let mut v = Vehicle::new(spec.clone(), Vec3::new(-60.0, 2.0, 0.0), EAST);
            run(&mut v, &pad, Controls::default(), 0.3, false, |_, _| {});
            v.b.vel = v.forward() * 70.0 / 3.6;
            let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let mut view = v.heading();
            let slip_of = |v: &Vehicle| {
                let vel = Vec3::new(v.b.vel.x, 0.0, v.b.vel.z);
                if vel.length() < 2.0 {
                    return 0.0;
                }
                let f = v.forward();
                (vel.normalize().cross(f).y).atan2(vel.normalize().dot(f)).to_degrees()
            };
            let dt = 1.0 / 60.0;
            let (mut sum, mut n, mut peak, mut spun) = (0.0f32, 0.0f32, 0.0f32, false);
            let mut straight_peak = 0.0f32;
            for i in 0..(5.0 / dt) as i32 {
                let t = i as f32 * dt;
                let flick = t < 0.4;
                if !flick && t < 3.4 {
                    // Turning right: yaw goes negative in this convention.
                    view -= turn * dt;
                }
                let diff = wrap(view - v.heading());
                let steer = if flick {
                    1.0
                } else if diff.abs() < 1.7 {
                    (-diff / v.lock().max(0.05)).clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                let c = Controls {
                    throttle: if flick { 0.5 } else { 1.0 },
                    steer,
                    handbrake: flick,
                    ..Default::default()
                };
                v.step(&c, &pad, -100.0, false, dt);
                let a = slip_of(&v);
                if (0.4..3.4).contains(&t) {
                    sum += a.abs();
                    n += 1.0;
                    peak = peak.max(a.abs());
                }
                if t >= 3.4 {
                    straight_peak = if t > 4.4 { straight_peak.max(a.abs()) } else { straight_peak };
                }
                spun |= a.abs() > 100.0;
            }
            eprintln!(
                "DRIFT {turn:.1} rad/s  mean slip {:.0}  peak {:.0} deg  spun {spun}  slip a second after straightening {:.0}  end {:.0} km/h",
                sum / n.max(1.0),
                peak,
                straight_peak,
                v.b.vel.length() * 3.6
            );
        }
    }

    /// The test circuit: a loop about 450 m round on flat hardpack, with a
    /// hairpin, an S, two long fast bends and a straight, and rolling bumps
    /// a voxel or so high along the back (x < -20).
    fn circuit() -> (Store, bot::Track) {
        let n = 900;
        let pts: Vec<glam::Vec2> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                glam::Vec2::new(85.0 * a.cos() + 10.0 * (2.0 * a).cos(), -(45.0 * a.sin() + 15.0 * (2.0 * a).sin() + 5.0 * (3.0 * a).sin()))
            })
            .collect();
        // An S on the top straight: 80 m of it pushed out to the side and back.
        let base = bot::Track::new(pts.clone(), 12.0);
        let pts: Vec<glam::Vec2> = (0..n)
            .map(|i| {
                let u = (base.along[i] - 95.0) / 80.0;
                if !(0.0..1.0).contains(&u) {
                    return pts[i];
                }
                let d = pts[(i + 1) % n] - pts[(i + n - 1) % n];
                let normal = glam::Vec2::new(-d.y, d.x).normalize();
                let pi = std::f32::consts::PI;
                pts[i] + normal * 9.0 * (2.0 * pi * u).sin() * (pi * u).sin()
            })
            .collect();
        let track = bot::Track::new(pts, 12.0);
        let ground = land(
            |x, z| {
                // BUMPS= scales the back section's bumps (0 for a smooth circuit).
                let k = std::env::var("BUMPS").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
                let bumps = if x < -20.0 { k * 0.15 * ((x * 0.8).sin() * (z * 0.6).sin() + 0.5 * (x * 1.7 + z).sin()) } else { 0.0 };
                2.0 + bumps
            },
            DIRT,
            (-14, 14),
            (-10, 10),
        );
        (ground, track)
    }

    /// The steady bot drives three laps of the circuit in a library
    /// vehicle; prints lap times and everything that went wrong.
    /// `TUNE=beach_buggy cargo test vehicle_laps -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vehicle_laps() {
        let name = std::env::var("TUNE").unwrap_or_else(|_| "beach_buggy".into());
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let spec = Catalogue::load(&root).get(&name);
        let (ground, track) = circuit();
        let tightest = track.curve.iter().fold(0.0f32, |m, k| m.max(k.abs()));
        let (p0, d0) = track.at(0.0);
        let yaw = (-d0.x).atan2(-d0.y);
        let mut car = Vehicle::new(spec, Vec3::new(p0.x, 2.0, p0.y), yaw);
        let mut bot = bot::Driver::new(bot::Style::default());
        let dt = 1.0 / 60.0;
        run(&mut car, &ground, Controls::default(), 0.5, true, |_, _| {});

        let (mut laps, mut lap_start, mut last_s) = (Vec::new(), 0.0f32, 0.0f32);
        let (mut off_t, mut max_off, mut spins, mut flips, mut resets) = (0.0f32, 0.0f32, 0, 0, 0);
        let (mut slow_t, mut upset_t, mut max_roll, mut max_slip, mut vsum, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let (mut spinning, mut flipped, mut halfway) = (false, false, false);
        let (mut off_sq, mut short) = (0.0f32, 0.0f32);
        let mut t = 0.0f32;
        // DUMP=file writes the track and the path driven, for plotting.
        let mut dump = std::env::var("DUMP").ok().map(|_| String::new());
        if let Some(d) = &mut dump {
            for p in &track.points {
                d.push_str(&format!(
                    "T,{},{}
",
                    p.x, p.y
                ));
            }
        }
        while laps.len() < 3 && t < 200.0 {
            let c = bot.drive(&car, &track);
            car.step(&c, &ground, -100.0, true, dt);
            t += dt;
            if let Some(d) = &mut dump {
                d.push_str(&format!(
                    "C,{},{},{},{}
",
                    car.pos().x,
                    car.pos().z,
                    car.v_fwd(),
                    bot.want
                ));
            }
            let pl = bot.place.unwrap();
            // A lap counts once it has been round past halfway, so a cut
            // across the infield is not one.
            halfway |= (track.length * 0.45..track.length * 0.55).contains(&pl.s);
            if last_s > track.length * 0.8 && pl.s < track.length * 0.2 {
                if halfway {
                    laps.push(t - lap_start);
                }
                lap_start = t;
                halfway = false;
            }
            off_sq += pl.off * pl.off;
            short += (bot.want.min(car.spec.top_speed) - car.v_fwd()).max(0.0);
            last_s = pl.s;
            let off = pl.off.abs() - track.width * 0.5;
            if off > 0.0 {
                off_t += dt;
            }
            max_off = max_off.max(off);
            let (r, f) = (car.b.rot * Vec3::X, car.b.rot * Vec3::NEG_Z);
            max_roll = max_roll.max(r.y.asin().to_degrees().abs());
            let vel = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
            let slip = if vel.length() > 3.0 {
                vel.normalize().dot(Vec3::new(f.x, 0.0, f.z).normalize()).clamp(-1.0, 1.0).acos().to_degrees()
            } else {
                0.0
            };
            max_slip = max_slip.max(slip);
            if slip > 90.0 && !spinning {
                spins += 1;
            }
            spinning = slip > 60.0;
            let up = car.up().y;
            if up < 0.3 && !flipped {
                flips += 1;
            }
            flipped = up < 0.5;
            vsum += car.v_fwd();
            n += 1.0;
            // Stuck or on its side: put it back on the track where it is,
            // facing the way round, as a race would.
            slow_t = if car.b.vel.length() < 1.0 { slow_t + dt } else { 0.0 };
            upset_t = if up < 0.5 { upset_t + dt } else { 0.0 };
            if slow_t > 3.0 || upset_t > 2.0 {
                resets += 1;
                let (p, d) = track.at(pl.s);
                car = Vehicle::new(car.spec.clone(), Vec3::new(p.x, 2.2, p.y), (-d.x).atan2(-d.y));
                slow_t = 0.0;
                upset_t = 0.0;
            }
        }
        if let (Some(d), Ok(f)) = (&dump, std::env::var("DUMP")) {
            std::fs::write(f, d).unwrap();
        }
        let lap = |x: &f32| format!("{x:.1}");
        eprintln!(
            "LAPS {}  ({:.0} m, tightest bend {:.0} m radius)  mean {:.0} km/h  line {:.1} m rms  short of asked {:.1} km/h  off track {:.1} s, {:.1} m at most  spins {spins}  flips {flips}  resets {resets}  max roll {:.0}  max slip {:.0} deg",
            laps.iter().map(lap).collect::<Vec<_>>().join(" "),
            track.length,
            1.0 / tightest,
            vsum / n.max(1.0) * 3.6,
            (off_sq / n.max(1.0)).sqrt(),
            short / n.max(1.0) * 3.6,
            off_t,
            max_off.max(0.0),
            max_roll,
            max_slip
        );
    }

    /// A small repeatable random source for the tournament.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        fn pick(&mut self, n: usize) -> usize {
            ((self.next() * n as f32) as usize).min(n - 1)
        }
    }

    /// Whether a tune is fit to race: a one-voxel bump under one side at
    /// 108 km/h rolls it under 6 degrees, it stays off its roof over rough
    /// ground, and a handbrake drift steered as Look steering does holds
    /// rather than spins.
    fn fit(spec: &Spec) -> Result<(), String> {
        if spec.drift_boost > 0.0 {
            // A kart: paved circuits only, so no bumps or rough ground; its
            // drift must hold without spinning, and charge.
            let pad = land(|_, _| 2.0, DIRT, (-12, 12), (-12, 12));
            let mut v = Vehicle::new(spec.clone(), Vec3::new(-60.0, 2.0, 0.0), EAST);
            run(&mut v, &pad, Controls::default(), 0.3, false, |_, _| {});
            v.b.vel = v.forward() * 60.0 / 3.6;
            let (mut spun, mut tier) = (false, 0);
            run(&mut v, &pad, Controls { throttle: 1.0, steer: 0.8, hop: true, ..Default::default() }, 2.0, false, |v, _| {
                let vel = Vec3::new(v.b.vel.x, 0.0, v.b.vel.z);
                spun |= vel.length() > 2.0 && vel.normalize().dot(v.forward()) < 0.0;
                tier = tier.max(v.drift_tier());
            });
            return if spun {
                Err("spins in a drift".into())
            } else if tier < 2 {
                Err("a drift doesn't charge".into())
            } else {
                Ok(())
            };
        }
        let side = land(|x, z| if (40.0..41.5).contains(&x) && z < 0.0 { 4.25 } else { 4.0 }, DIRT, (-1, 30), (-2, 2));
        let mut v = Vehicle::new(spec.clone(), Vec3::new(10.0, 4.0, 0.0), EAST);
        run(&mut v, &side, Controls::default(), 0.5, true, |_, _| {});
        v.b.vel = v.forward() * 30.0;
        let mut roll = 0.0f32;
        run(&mut v, &side, Controls::default(), 1.7, true, |v, _| roll = roll.max((v.b.rot * Vec3::X).y.asin().to_degrees().abs()));
        if roll > 6.0 {
            return Err(format!("side bump rolls it {roll:.0} deg"));
        }
        let rough = land(|x, z| 4.0 + 0.25 * ((x * 0.7).sin() * (z * 0.9 + 1.0).sin() + 0.5 * (x * 1.9 + z * 0.4).sin()), DIRT, (-1, 60), (-2, 2));
        let mut v = Vehicle::new(spec.clone(), Vec3::new(2.0, 6.0, 0.0), EAST);
        run(&mut v, &rough, Controls::default(), 1.0, true, |_, _| {});
        let mut min_up = 1.0f32;
        run(&mut v, &rough, Controls { throttle: 1.0, ..Default::default() }, 10.0, true, |v, _| {
            if v.pos().x < 440.0 {
                min_up = min_up.min(v.up().y);
            }
        });
        if min_up < 0.3 {
            return Err("over on rough ground".into());
        }
        let pad = land(|_, _| 2.0, DIRT, (-12, 12), (-12, 12));
        let mut v = Vehicle::new(spec.clone(), Vec3::new(-60.0, 2.0, 0.0), EAST);
        run(&mut v, &pad, Controls::default(), 0.3, false, |_, _| {});
        v.b.vel = v.forward() * 70.0 / 3.6;
        let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let mut view = v.heading();
        let dt = 1.0 / 60.0;
        for i in 0..(3.4 / dt) as i32 {
            let t = i as f32 * dt;
            let flick = t < 0.4;
            if !flick {
                view -= dt;
            }
            let diff = wrap(view - v.heading());
            let steer = if flick {
                1.0
            } else if diff.abs() < 1.7 {
                (-diff / v.lock().max(0.05)).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            v.step(
                &Controls {
                    throttle: if flick { 0.5 } else { 1.0 },
                    steer,
                    handbrake: flick,
                    ..Default::default()
                },
                &pad,
                -100.0,
                false,
                dt,
            );
            let vel = Vec3::new(v.b.vel.x, 0.0, v.b.vel.z);
            if vel.length() > 2.0 && vel.normalize().dot(v.forward()) < -0.17 {
                return Err("spins in a drift".into());
            }
        }
        Ok(())
    }

    /// What one race came to for each car (by grid slot): finishing time,
    /// or None for not finishing, and resets.
    struct RaceResult {
        time: [Option<f32>; 3],
        resets: [u32; 3],
        hits: u32,
    }

    /// Three laps, three cars: `specs[i]` driven by `styles[i]`, starting in
    /// slot i of a staggered grid behind the line.
    fn race(ground: &Store, track: &bot::Track, specs: [&Spec; 3], styles: [bot::Style; 3]) -> RaceResult {
        let laps = 3.0;
        let mut cars: Vec<Vehicle> = Vec::new();
        let mut bots: Vec<bot::Driver> = Vec::new();
        let mut progress = [0.0f32; 3];
        for i in 0..3 {
            let s = -7.0 * (i as f32 + 1.0);
            let (p, d) = track.at(s);
            let side = if i % 2 == 0 { 2.5 } else { -2.5 };
            let at = p + glam::Vec2::new(-d.y, d.x) * side;
            cars.push(Vehicle::new(specs[i].clone(), Vec3::new(at.x, 2.2, at.y), (-d.x).atan2(-d.y)));
            let mut b = bot::Driver::new(styles[i]);
            b.lane = side;
            bots.push(b);
            progress[i] = s;
        }
        let mut last_s: Vec<f32> = progress.iter().map(|s| s.rem_euclid(track.length)).collect();
        let mut out = RaceResult {
            time: [None; 3],
            resets: [0; 3],
            hits: 0,
        };
        let (mut slow, mut upset) = ([0.0f32; 3], [0.0f32; 3]);
        let dt = 1.0 / 60.0;
        // Settle on the springs before the start.
        for c in cars.iter_mut() {
            for _ in 0..30 {
                c.step(&Controls::default(), ground, -100.0, true, dt);
            }
        }
        let mut t = 0.0;
        while t < 150.0 && out.time.iter().any(|x| x.is_none()) {
            let places: Vec<bot::Place> = (0..3).map(|i| track.place(cars[i].pos(), bots[i].place)).collect();
            for i in 0..3 {
                if out.time[i].is_some() {
                    // Finished: braking to a stop, out of the race.
                    cars[i].step(&Controls { brake: 0.3, ..Default::default() }, ground, -100.0, true, dt);
                    continue;
                }
                let rivals: Vec<bot::Rival> = (0..3)
                    .filter(|&j| j != i && out.time[j].is_none())
                    .map(|j| bot::Rival {
                        place: places[j],
                        speed: cars[j].v_fwd(),
                    })
                    .collect();
                let c = bots[i].race(&cars[i], track, &rivals);
                cars[i].step(&c, ground, -100.0, true, dt);
            }
            for i in 0..3 {
                for j in i + 1..3 {
                    let (a, b) = cars.split_at_mut(j);
                    if bump(&mut a[i], &mut b[0]) > 1.0 {
                        out.hits += 1;
                    }
                }
            }
            t += dt;
            for i in 0..3 {
                if out.time[i].is_some() {
                    continue;
                }
                let s = bots[i].place.map_or(0.0, |p| p.s);
                let mut ds = s - last_s[i];
                if ds < -track.length * 0.5 {
                    ds += track.length;
                } else if ds > track.length * 0.5 {
                    ds -= track.length;
                }
                progress[i] += ds;
                last_s[i] = s;
                if progress[i] >= laps * track.length {
                    out.time[i] = Some(t);
                }
                slow[i] = if cars[i].b.vel.length() < 1.0 { slow[i] + dt } else { 0.0 };
                upset[i] = if cars[i].up().y < 0.5 { upset[i] + dt } else { 0.0 };
                if slow[i] > 3.0 || upset[i] > 2.0 {
                    out.resets[i] += 1;
                    let (p, d) = track.at(s);
                    cars[i] = Vehicle::new(cars[i].spec.clone(), Vec3::new(p.x, 2.4, p.y), (-d.x).atan2(-d.y));
                    slow[i] = 0.0;
                    upset[i] = 0.0;
                }
            }
        }
        out
    }

    /// The car numbers the tournament tunes, with the range each may take.
    const CAR_KNOBS: &[(&str, f32, f32)] = &[
        ("grip", 1.0, 1.5),
        ("slide", 0.7, 0.95),
        ("power", 120.0, 220.0),
        ("force", 5000.0, 8500.0),
        ("top_speed", 32.0, 44.0),
        ("freq_front", 1.2, 2.2),
        ("freq_rear", 1.2, 2.4),
        ("damp_bump", 0.25, 0.7),
        ("damp_rebound", 0.35, 0.9),
        ("damp_knee", 0.2, 1.0),
        ("anti_roll", 0.0, 0.6),
        ("settle", 3.0, 14.0),
        ("upright", 8.0, 30.0),
        ("downforce", 0.0, 2.0),
        ("yaw_damp", 0.5, 4.0),
        ("lock", 25.0, 40.0),
        ("brake", 0.9, 1.6),
        ("rear_weight", 0.45, 0.65),
    ];

    fn car_knob<'a>(s: &'a mut Spec, key: &str) -> &'a mut f32 {
        match key {
            "grip" => &mut s.grip,
            "slide" => &mut s.slide,
            "power" => &mut s.power,
            "force" => &mut s.force,
            "top_speed" => &mut s.top_speed,
            "freq_front" => &mut s.freq_front,
            "freq_rear" => &mut s.freq_rear,
            "damp_bump" => &mut s.damp_bump,
            "damp_rebound" => &mut s.damp_rebound,
            "damp_knee" => &mut s.damp_knee,
            "anti_roll" => &mut s.anti_roll,
            "settle" => &mut s.settle,
            "upright" => &mut s.upright,
            "downforce" => &mut s.downforce,
            "yaw_damp" => &mut s.yaw_damp,
            "lock" => &mut s.lock,
            "brake" => &mut s.brake,
            "rear_weight" => &mut s.rear_weight,
            _ => unreachable!(),
        }
    }

    /// The bot numbers the tournament tunes, with their ranges.
    const BOT_KNOBS: &[(&str, f32, f32)] = &[
        ("corner_g", 10.0, 19.0),
        ("brake_g", 8.0, 16.0),
        ("look_secs", 0.25, 0.9),
        ("look_add", 2.0, 9.0),
        ("pass_look", 8.0, 30.0),
        ("pass_side", 2.2, 4.5),
    ];

    fn bot_knob<'a>(s: &'a mut bot::Style, key: &str) -> &'a mut f32 {
        match key {
            "corner_g" => &mut s.corner_g,
            "brake_g" => &mut s.brake_g,
            "look_secs" => &mut s.look_secs,
            "look_add" => &mut s.look_add,
            "pass_look" => &mut s.pass_look,
            "pass_side" => &mut s.pass_side,
            _ => unreachable!(),
        }
    }

    /// Two of `knobs` moved by up to 15% each, kept in range; a line saying
    /// what changed.
    fn mutate<T>(x: &mut T, knobs: &[(&str, f32, f32)], get: for<'a> fn(&'a mut T, &str) -> &'a mut f32, rng: &mut Rng) -> String {
        let mut said = Vec::new();
        for _ in 0..2 {
            let (k, lo, hi) = knobs[rng.pick(knobs.len())];
            let v = get(x, k);
            let was = *v;
            // Kept in the range only if it started in it: a vehicle built
            // outside the buggy's ranges (a kart) moves freely.
            let to = was * (1.0 + (rng.next() * 2.0 - 1.0) * 0.15);
            let to = if (lo..=hi).contains(&was) { to.clamp(lo, hi) } else { to };
            // Kept to round numbers: three figures.
            let step = if to >= 100.0 {
                10.0
            } else if to >= 10.0 {
                0.1
            } else {
                0.01
            };
            *v = ((to / step).round() * step * 100.0).round() / 100.0;
            said.push(format!("{k} {was}->{}", *v));
        }
        said.join(", ")
    }

    /// Twenty rounds of three races between three bots in three cars. Each
    /// round every bot drives every car once, from every grid slot, so a
    /// car's points are the car's and a bot's the bot's. Odd rounds tune the
    /// cars: the one with fewest points becomes a variation on the one with
    /// most, if the variation passes `fit`. Even rounds do the same for the
    /// bots' styles.
    /// `cargo test vehicle_tournament -- --ignored --nocapture`; SEED= and
    /// ROUNDS= to vary it.
    #[test]
    #[ignore]
    fn vehicle_tournament() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base = Catalogue::load(&root).get(&std::env::var("TUNE").unwrap_or_else(|_| "beach_buggy".into()));
        let seed = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x9e37_79b9_7f4a_7c15u64);
        let rounds = std::env::var("ROUNDS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
        let mut rng = Rng(seed);
        let (ground, track) = circuit();
        let names = ["A", "B", "C"];
        // The file's tune and two fit variations on it.
        let mut cars = [base.clone(), base.clone(), base.clone()];
        for c in cars.iter_mut().skip(1) {
            for _ in 0..8 {
                let mut m = base.clone();
                mutate(&mut m, CAR_KNOBS, car_knob, &mut rng);
                if fit(&m).is_ok() {
                    *c = m;
                    break;
                }
            }
        }
        let steady = bot::Style::default();
        let mut bots = [
            steady,
            bot::Style {
                corner_g: 15.5,
                brake_g: 13.0,
                look_secs: 0.45,
                ..steady
            },
            bot::Style {
                corner_g: 12.5,
                brake_g: 11.0,
                look_secs: 0.6,
                ..steady
            },
        ];
        for round in 1..=rounds {
            let mut car_pts = [0u32; 3];
            let mut bot_pts = [0u32; 3];
            let mut car_time = [0.0f32; 3];
            let mut bot_time = [0.0f32; 3];
            let (mut best, mut resets, mut hits) = (f32::MAX, 0, 0);
            for k in 0..3 {
                // Bot b drives car (b + k) % 3 from grid slot (b + k) % 3.
                let mut seat = [(0usize, 0usize); 3];
                for bo in 0..3 {
                    seat[(bo + k) % 3] = (bo, (bo + k) % 3);
                }
                let specs = [&cars[seat[0].1], &cars[seat[1].1], &cars[seat[2].1]];
                let styles = [bots[seat[0].0], bots[seat[1].0], bots[seat[2].0]];
                let r = race(&ground, &track, specs, styles);
                hits += r.hits;
                let mut order: Vec<usize> = (0..3).collect();
                order.sort_by(|&a, &b| r.time[a].unwrap_or(999.0).total_cmp(&r.time[b].unwrap_or(999.0)));
                for (pos, &slot) in order.iter().enumerate() {
                    let pts = if r.time[slot].is_some() { 3 - pos as u32 } else { 0 };
                    let (bo, ca) = seat[slot];
                    car_pts[ca] += pts;
                    bot_pts[bo] += pts;
                    let tm = r.time[slot].unwrap_or(200.0);
                    car_time[ca] += tm;
                    bot_time[bo] += tm;
                    best = best.min(tm);
                    resets += r.resets[slot];
                }
            }
            let rank = |pts: &[u32; 3], tm: &[f32; 3]| {
                let mut o: Vec<usize> = (0..3).collect();
                o.sort_by(|&a, &b| pts[b].cmp(&pts[a]).then(tm[a].total_cmp(&tm[b])));
                o
            };
            let cars_rank = rank(&car_pts, &car_time);
            let bots_rank = rank(&bot_pts, &bot_time);
            let tuning_cars = round % 2 == 1;
            let change = if tuning_cars {
                let (top, low) = (cars_rank[0], cars_rank[2]);
                let mut said = String::from("no fit variation");
                for _ in 0..6 {
                    let mut m = cars[top].clone();
                    let what = mutate(&mut m, CAR_KNOBS, car_knob, &mut rng);
                    match fit(&m) {
                        Ok(()) => {
                            cars[low] = m;
                            said = format!("car {} <- car {} with {what}", names[low], names[top]);
                            break;
                        }
                        Err(e) => said = format!("rejected ({e}): {what}"),
                    }
                }
                said
            } else {
                let (top, low) = (bots_rank[0], bots_rank[2]);
                let mut m = bots[top];
                let what = mutate(&mut m, BOT_KNOBS, bot_knob, &mut rng);
                bots[low] = m;
                format!("bot {} <- bot {} with {what}", low + 1, top + 1)
            };
            eprintln!(
                "R{round:02} {}  cars A {} B {} C {} ({:.0}/{:.0}/{:.0} s)  bots 1 {} 2 {} 3 {} ({:.0}/{:.0}/{:.0} s)  best {:.1} s  resets {resets}  hits {hits}  | {change}",
                if tuning_cars { "CARS" } else { "BOTS" },
                car_pts[0],
                car_pts[1],
                car_pts[2],
                car_time[0],
                car_time[1],
                car_time[2],
                bot_pts[0],
                bot_pts[1],
                bot_pts[2],
                bot_time[0],
                bot_time[1],
                bot_time[2],
                best
            );
        }
        eprintln!("FINAL CARS (changes from {}.json):", base.name);
        for (i, c) in cars.iter().enumerate() {
            let mut cc = c.clone();
            let mut bb = base.clone();
            let diff: Vec<String> = CAR_KNOBS
                .iter()
                .filter_map(|(k, _, _)| {
                    let (a, b) = (*car_knob(&mut cc, k), *car_knob(&mut bb, k));
                    ((a - b).abs() > 1e-4).then(|| format!("\"{k}\": {a}"))
                })
                .collect();
            eprintln!("  {}: {}", names[i], diff.join(", "));
        }
        eprintln!("FINAL BOTS:");
        for (i, b) in bots.iter().enumerate() {
            eprintln!(
                "  {}: corner_g {:.2} brake_g {:.2} look_secs {:.2} look_add {:.2} pass_look {:.1} pass_side {:.2}",
                i + 1,
                b.corner_g,
                b.brake_g,
                b.look_secs,
                b.look_add,
                b.pass_look,
                b.pass_side
            );
        }
    }
}
