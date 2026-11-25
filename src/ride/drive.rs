//! Driving: the glue between the keys and a `Vehicle` (src/vehicle). U gets
//! in and out; getting out, the car goes, as the bike does, and U calls up
//! a new one at your feet. (A car saved parked still loads where it was
//! left, and U beside it gets back in.) Held, U puts the car back on its
//! wheels where it last had all four down.
//!
//! Keys: W throttle, S brake and then reverse, Ctrl handbrake, Space held
//! loads the springs and letting go hops, Shift boosts, V the camera. In the
//! air W/S pitch the car and A/D roll it. In the chase camera with look
//! steering (the default) the front wheels turn toward where the view points
//! and A/D turn the view, as on the bike. In the driver's seat the view is
//! fixed to the car, turning as it turns; A/D steer and the mouse looks round
//! the cab.

use super::{wrap, Mode, Ride};
use crate::modes::ModeId;
use crate::player::{MoveInput, Player};
use crate::vehicle::{Controls, Steering, Vehicle};
use crate::world::store::Store;
use glam::Vec3;

/// Within this far of the parked car, U gets back in; further, it is
/// brought over.
const REACH: f32 = 5.0;
/// Hold U this long to put the car back on its wheels.
const HOLD_RESET: f32 = 0.7;
/// Beyond this far the parked car stops being simulated.
const PARKED_SIM_M: f32 = 250.0;

impl Ride {
    /// The vehicle this world drives, by library name.
    pub fn vehicle_name(&self) -> Option<&str> {
        self.vehicles.get(self.vehicle_pick.min(self.vehicles.len().saturating_sub(1))).map(|s| s.as_str())
    }

    /// The car being driven.
    pub fn car(&self) -> Option<&Vehicle> {
        self.car.as_ref().filter(|_| self.mode == Mode::Drive)
    }

    /// The car being driven, to change (a race bumps it, or puts it back on
    /// the track).
    pub fn car_mut(&mut self) -> Option<&mut Vehicle> {
        self.car.as_mut().filter(|_| self.mode == Mode::Drive)
    }

    /// A knock from outside the car's own step (another kart meeting it in
    /// a race), `hard` m/s: the view shakes and the controller rumbles as
    /// for a crash.
    pub fn jolt(&mut self, hard: f32) {
        self.shake += (hard * 0.012).min(0.12);
        self.rumble = self.rumble.max(((hard - 3.0) / 12.0).clamp(0.0, 1.0));
    }

    /// Everything a race sets on the ride, back to what it is out of one:
    /// the car takes the keys and is drawn, unlit, the view looks ahead, and
    /// the boost meter is the ride's own.
    pub fn clear_race(&mut self) {
        self.hold = false;
        self.auto = None;
        self.race_boost = None;
        self.flash = false;
        self.hide = false;
        self.look_back = false;
    }

    /// Into a new car of `spec` at `at` (on the ground) facing `yaw`, from
    /// wherever the player was: how a race puts them on its grid.
    pub fn race_car(&mut self, p: &mut Player, spec: crate::vehicle::Spec, at: Vec3, yaw: f32) {
        self.car = Some(Vehicle::new(spec, at, yaw));
        self.parked = None;
        self.mode = Mode::Drive;
        self.rope_out = false;
        self.out_t = None;
        self.seat(p);
        p.prev_pos = p.pos;
        p.yaw = yaw;
        self.heading = yaw;
        self.prev_heading = yaw;
        self.chase.begin_step();
    }

    /// Take speed off the car being driven (it struck something soft).
    pub fn slow_car(&mut self, keep: f32) {
        if let Some(car) = self.car.as_mut().filter(|_| self.mode == Mode::Drive) {
            car.b.vel *= keep.clamp(0.0, 1.0);
        }
    }

    /// U pressed: get in, or (driving) start the hold that gets out on a
    /// tap and resets on a hold.
    pub fn vehicle_key(&mut self, p: &mut Player, store: &Store) {
        match self.mode {
            Mode::Foot => self.get_in(p, store),
            Mode::Drive => self.out_t = Some(0.0),
            Mode::Bike => self.note = Some("OFF THE BIKE FIRST (X)".into()),
            Mode::Kayak => self.note = Some("IN THE KAYAK: K TO GET OUT".into()),
            Mode::Wing | Mode::Climb | Mode::Swing => self.note = Some("LAND FIRST".into()),
        }
    }

    fn get_in(&mut self, p: &mut Player, store: &Store) {
        if p.swimming {
            self.note = Some("NOT IN THE WATER".into());
            return;
        }
        if !self.can(ModeId::Drive) {
            return;
        }
        let Some(name) = self.vehicle_name().map(str::to_string) else {
            self.note = Some("NO VEHICLE ON THIS MAP".into());
            return;
        };
        let near = self.parked.as_ref().is_some_and(|c| (c.pos() - p.pos).length() < REACH);
        let car = if near {
            self.parked.take().unwrap()
        } else {
            // Brought over, or a new one: on the ground at the player's feet,
            // facing the way they look.
            let spec = self.parked.as_ref().map_or_else(|| self.catalogue.get(&name), |c| c.spec.clone());
            let Some(car) = place(spec, p, store) else {
                self.note = Some("NO ROOM FOR IT HERE".into());
                return;
            };
            self.parked = None;
            car
        };
        self.note = Some(format!("{}   W DRIVE  S BRAKE  CTRL HANDBRAKE  SPACE HOP  SHIFT BOOST  U OUT, HELD: RESET", car.spec.title));
        // In the seat facing the way the car does.
        p.yaw = car.heading();
        self.heading = p.yaw;
        self.prev_heading = p.yaw;
        self.car = Some(car);
        self.mode = Mode::Drive;
        self.rope_out = false;
        self.out_t = None;
        self.seat(p);
        p.prev_pos = p.pos;
        self.chase.begin_step();
    }

    /// Out of the car beside it; the car is gone.
    fn get_out(&mut self, p: &mut Player, store: &Store) {
        let Some(car) = self.car.take() else {
            self.mode = Mode::Foot;
            return;
        };
        let right = car.b.rot * Vec3::X;
        let fwd = car.forward();
        let base = car.pos();
        let spots = [-right * 1.6, right * 1.6, fwd * 2.6, -fwd * 2.6, Vec3::Y * 1.8];
        let spot = spots.iter().map(|o| base + *o).find_map(|q| {
            let ground = crate::vehicle::draw_ground(store, q)?;
            let at = Vec3::new(q.x, ground, q.z);
            (!super::hits_hard(store, at + Vec3::Y * 0.05, 0.3, 1.75)).then_some(at)
        });
        let Some(at) = spot else {
            self.note = Some("NO ROOM TO GET OUT HERE".into());
            self.car = Some(car);
            return;
        };
        p.pos = at;
        p.prev_pos = at;
        p.body.vel = car.b.vel * 0.3;
        p.body.on_ground = true;
        self.mode = Mode::Foot;
        // The car goes when you leave it, as the bike does: U calls up a
        // fresh one at your feet.
        self.parked = None;
    }

    /// The player's body goes where the car is: on the ground under its
    /// centre, moving as it moves.
    fn seat(&self, p: &mut Player) {
        let Some(car) = &self.car else { return };
        let s = &car.spec;
        p.pos = car.pos() - Vec3::Y * (s.wheel_radius + s.com_height);
        p.body.vel = car.b.vel;
        p.body.on_ground = car.grounded > 0;
    }

    /// One step driving.
    pub(super) fn drive(&mut self, p: &mut Player, input: &MoveInput, store: &Store, sea: f32, slopes: bool, dt: f32) {
        // U held after a press: a tap gets out, a hold resets.
        if let Some(t) = self.out_t {
            if input.vehicle {
                let t = t + dt;
                self.out_t = Some(t);
                if t >= HOLD_RESET {
                    self.out_t = None;
                    if let Some(car) = &mut self.car {
                        car.reset();
                        self.note = Some("BACK ON ITS WHEELS".into());
                    }
                }
            } else {
                self.out_t = None;
                let fast = self.car.as_ref().is_some_and(|c| c.b.vel.length() > 8.0);
                if fast {
                    self.note = Some("SLOW DOWN FIRST".into());
                } else {
                    self.get_out(p, store);
                    return;
                }
            }
        }
        let boosting = self.boosting;
        let seat = !self.chase_on();
        let Some(car) = &mut self.car else {
            self.mode = Mode::Foot;
            return;
        };
        let key = |b: bool| b as i32 as f32;
        // In the driver's seat the view is fixed to the car, so it cannot be
        // what the wheels steer toward: A/D steer there, whatever the tune.
        // With the controller the left stick steers, whatever the camera:
        // the right stick is for looking round, so the view cannot steer.
        let direct = car.spec.steering == Steering::Direct || seat || input.pad;
        let steer = if input.pad {
            // Softer near the centre, so small corrections are small.
            let x = input.stick.x;
            if x != 0.0 {
                x.signum() * x.abs().powf(1.5)
            } else {
                key(input.right) - key(input.left)
            }
        } else if direct {
            key(input.right) - key(input.left)
        } else {
            // Toward where the view points; looking far aside or behind
            // leaves the wheels straight, as on the bike.
            let diff = wrap(p.yaw - car.heading());
            if diff.abs() < 1.7 {
                (-diff / car.lock().max(0.05)).clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        let c = if self.hold {
            Controls::default()
        } else if let Some(c) = self.auto.take() {
            c
        } else {
            // The triggers are the pedals; W and S still work beside them.
            let (throttle, brake) = if input.pad {
                (input.throttle.max(key(input.forward)), input.brake.max(key(input.back)))
            } else {
                (key(input.forward), key(input.back))
            };
            Controls {
                throttle,
                brake,
                steer,
                handbrake: input.down,
                hop: input.jump,
                boost: boosting,
                pitch: input.ahead(),
                roll: input.side(),
            }
        };
        let h0 = car.heading();
        car.step(&c, store, sea, slopes, dt);
        if seat {
            // The view turns with the car. This moves it in 60 Hz steps;
            // `Ride::camera` takes back what the frame has not reached yet.
            p.yaw = wrap(p.yaw + wrap(car.heading() - h0));
        } else if direct && car.grounded > 0 {
            // The chase camera swings round to follow where the car is
            // going, so a slide shows the corner it is sliding into.
            let v = car.b.vel;
            let want = if Vec3::new(v.x, 0.0, v.z).length() > 3.0 { super::yaw_of(v) } else { car.heading() };
            p.yaw = wrap(p.yaw + wrap(want - p.yaw) * (dt * 2.5).min(1.0));
        }
        let ev = std::mem::take(&mut car.events);
        if let Some(n) = &ev.note {
            self.note = Some(n.clone());
        }
        if ev.clean_landing {
            self.spent = (self.spent - 0.4).max(0.0);
            self.boost_glow = 1.0;
        }
        // The controller shakes with the knocks: a landing, a hit.
        self.rumble = self.rumble.max(((ev.thump - 2.0) / 10.0).max((ev.crash - 3.0) / 12.0).clamp(0.0, 1.0));
        if ev.thump > 3.0 {
            self.shake += (ev.thump * 0.01).min(0.1);
            self.bob_v -= ev.thump * 0.05;
            // The springs hitting their stops, or the car landing on them.
            crate::sound::emit(crate::sound::Noise::ear(crate::sound::Kind::Land(ev.thump * 1.5), (0.3 + ev.thump / 10.0).min(1.0)));
        }
        if ev.crash > 4.0 {
            self.shake += (ev.crash * 0.012).min(0.12);
            if ev.crash > 10.0 {
                self.note = Some("OOF".into());
            }
        }
        self.drive_events = ev;
        // The view's lean and tip follow the car, eased, so the slope shows
        // but a bump under one wheel does not jolt the horizon.
        let (_, rot) = car.pose(1.0);
        let right = rot * Vec3::X;
        let fwd = rot * Vec3::NEG_Z;
        let k = (dt * 5.0).min(1.0);
        self.lean += ((-right.y).clamp(-1.0, 1.0).asin() - self.lean) * k;
        self.body_pitch += (fwd.y.clamp(-1.0, 1.0).asin() - self.body_pitch) * k;
        self.air = car.grounded == 0;
        self.air_time = car.air_time;
        self.heading = car.heading();
        self.speed = car.v_fwd();
        // The head rides a short spring over the chassis' jolts, a hand's
        // breadth at most, the way a driver's neck takes them.
        let eye = rot * Vec3::from(car.spec.eye) + car.pos();
        let (x, v) = crate::player::smooth_damp((self.eye_y - eye.y).clamp(-0.12, 0.12), self.eye_v, 0.1, dt);
        self.eye_y = eye.y + x;
        self.eye_v = v;
        self.shake *= 1.0 - (dt * 6.0).min(1.0);
        self.seat(p);
    }

    /// The parked car settles and rolls where it was left, while it is near
    /// enough to matter.
    pub(super) fn step_parked(&mut self, p: &Player, store: &Store, sea: f32, slopes: bool, dt: f32) {
        if let Some(car) = &mut self.parked {
            if (car.pos() - p.pos).length() < PARKED_SIM_M {
                car.step(&Controls::default(), store, sea, slopes, dt);
                car.events = Default::default();
            }
        }
    }

    /// A tuning panel key while driving (vehicle/tune.rs); false when it
    /// was not one or there is no car.
    pub fn tune_key(&mut self, code: winit::keyboard::KeyCode) -> bool {
        use crate::vehicle::tune::{self, KNOBS};
        use winit::keyboard::KeyCode as K;
        if !tune::is_key(code) || self.car().is_none() {
            return false;
        }
        let n = KNOBS.len();
        let i = self.tune_pick.min(n - 1);
        match code {
            K::PageUp => self.tune_pick = (i + n - 1) % n,
            K::PageDown => self.tune_pick = (i + 1) % n,
            K::BracketLeft => self.tune_step(i, -1.0),
            K::BracketRight => self.tune_step(i, 1.0),
            K::Minus => self.tune_step(i, -10.0),
            K::Equal => self.tune_step(i, 10.0),
            K::Backspace => self.tune_reset(i),
            K::KeyP => self.tune_save(),
            _ => {}
        }
        true
    }

    /// The tune of the car being driven, to step a line from.
    pub fn tune_spec(&self) -> Option<crate::vehicle::Spec> {
        self.car().map(|c| c.spec.clone())
    }

    /// Pick line `i` and set the car to `base` with that line moved `steps`
    /// steps: a drag measures from where it started, so it never drifts.
    pub fn tune_apply(&mut self, i: usize, base: &crate::vehicle::Spec, steps: f32) {
        use crate::vehicle::tune::{self, KNOBS};
        let i = i.min(KNOBS.len() - 1);
        self.tune_pick = i;
        let Some(car) = self.car.as_mut().filter(|_| self.mode == Mode::Drive) else { return };
        let mut spec = base.clone();
        tune::nudge(&mut spec, &KNOBS[i], steps);
        car.retune(spec);
    }

    /// Step line `i` from where it is now.
    pub fn tune_step(&mut self, i: usize, steps: f32) {
        if let Some(base) = self.tune_spec() {
            self.tune_apply(i, &base, steps);
        }
    }

    /// Put line `i` back to the file's value.
    pub fn tune_reset(&mut self, i: usize) {
        use crate::vehicle::tune::{self, KNOBS};
        let i = i.min(KNOBS.len() - 1);
        self.tune_pick = i;
        let Some(car) = self.car.as_mut().filter(|_| self.mode == Mode::Drive) else { return };
        let from = self.catalogue.get(&car.spec.name);
        let mut spec = car.spec.clone();
        tune::reset(&mut spec, &from, KNOBS[i].key);
        car.retune(spec);
    }

    /// Write the changed lines into the vehicle's file.
    pub fn tune_save(&mut self) {
        let Some(spec) = self.tune_spec() else { return };
        let from = self.catalogue.get(&spec.name);
        let Some(path) = self.catalogue.files.get(&spec.name).cloned() else {
            self.note = Some("NO FILE FOR THIS VEHICLE".into());
            return;
        };
        let file = path.file_name().map(|f| f.to_string_lossy().to_uppercase()).unwrap_or_default();
        self.note = Some(match crate::vehicle::tune::save(&path, &spec, &from) {
            Ok(0) => "NOTHING CHANGED TO SAVE".into(),
            Ok(n) => format!("WROTE {n} TO {file}"),
            Err(e) => format!("NOT SAVED: {}", e.to_uppercase()),
        });
    }

    /// The tuning panel's lines: name, value, the file's value when it
    /// differs, and whether it is the picked line.
    pub fn tune_lines(&self) -> Vec<(String, String, Option<String>, bool)> {
        use crate::vehicle::tune::{value, KNOBS};
        let Some(car) = self.car() else { return Vec::new() };
        let from = self.catalogue.get(&car.spec.name);
        KNOBS
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let (v, f) = (value(&car.spec, k.key), value(&from, k.key));
                (k.key.replace('_', " ").to_uppercase(), v.to_uppercase(), (v != f).then(|| f.to_uppercase()), i == self.tune_pick)
            })
            .collect()
    }

    /// A changed tune file reaches the car at once.
    pub(super) fn retune(&mut self) {
        if !self.catalogue.reload_if_changed() {
            return;
        }
        for car in [&mut self.car, &mut self.parked].into_iter().flatten() {
            let spec = self.catalogue.get(&car.spec.name);
            car.retune(spec);
        }
        self.note = Some(match self.catalogue.errors.len() {
            0 => "VEHICLE TUNE RELOADED".into(),
            n => format!("VEHICLE TUNE RELOADED, {n} FILE(S) KEPT LAST GOOD TUNE"),
        });
    }

    /// The cars to draw: the one being driven (with its driver in the chase
    /// camera, or the gloves on the wheel in first person) and the parked one.
    /// `base` is the first vehicle model.
    pub fn vehicle_instances(&self, alpha: f32, base: usize, models: &[crate::models::Model], out: &mut Vec<(usize, crate::models::Instance)>) {
        if let Some(car) = self.car().filter(|_| !self.hide) {
            let outside = self.chase_on() || self.show_body;
            let from = out.len();
            crate::vehicle::draw::instances(car, alpha, base, models, true, !outside, out);
            if self.flash {
                crate::vehicle::draw::flash(&mut out[from..]);
            }
        }
        if let Some(car) = &self.parked {
            crate::vehicle::draw::instances(car, alpha, base, models, false, false, out);
        }
    }

    /// Lines for the stats panel while driving: each wheel's load, slip and
    /// compression, and what it is on.
    pub fn drive_debug(&self) -> Vec<String> {
        let Some(car) = self.car() else { return Vec::new() };
        let names = ["FL", "FR", "RL", "RR"];
        let mut out: Vec<String> = car
            .wheels
            .iter()
            .zip(names)
            .map(|(w, n)| {
                let on = w.contact.filter(|_| w.down).map_or("AIR".to_string(), |c| c.surface.name().to_uppercase());
                format!(
                    "{n}  LOAD {:5.0} N  SLIP {:4.1}  SLIDE {:4.1}  SPIN {:3.0}%  COMP {:4.2} M  {on}",
                    w.load,
                    w.slip.to_degrees(),
                    w.slide,
                    w.spin * 100.0,
                    w.x
                )
            })
            .collect();
        let up = car.up();
        out.push(format!(
            "STEER {:5.1}  YAW {:5.2}  UP {:4.2}  RPM {:3.0}%  WADE {:3.1}",
            car.steer.to_degrees(),
            car.b.ang.dot(up),
            up.y,
            car.rpm * 100.0,
            car.wade
        ));
        out
    }
}

/// A new car of `spec` at the player's feet, facing the way they look,
/// or nearby where there is room for it.
fn place(spec: crate::vehicle::Spec, p: &Player, store: &Store) -> Option<Vehicle> {
    let f = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let r = Vec3::new(-f.z, 0.0, f.x);
    let tries = [Vec3::ZERO, f * 2.5, r * 2.5, -r * 2.5, -f * 2.5, f * 5.0, r * 5.0, -r * 5.0];
    tries.iter().find_map(|o| {
        let q = p.pos + *o;
        let ground = crate::vehicle::draw_ground(store, q)?;
        let car = Vehicle::new(spec.clone(), Vec3::new(q.x, ground + 0.15, q.z), p.yaw);
        car.fits(store).then_some(car)
    })
}
