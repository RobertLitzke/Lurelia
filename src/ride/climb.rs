//! Climbing and the rope: ways up cliffs and spires, to glide from them.
//!
//! Climbing borrows from Breath of the Wild: any wall at least head-high can
//! be climbed by walking (or gliding slowly) into it, and the climber pulls
//! over the top edge on their own. There is no stamina; it is quick (3.6 m/s
//! up, a lunge with Space+W) because the point is getting up to fly.
//!
//! The rope borrows from Spider-Man's anchors-anywhere swinging, Jusant's
//! rappel and swing, and Just Cause's chaining of grapple and wingsuit. Z
//! gets it out (it is out anyway while climbing), which shows where it would
//! catch; Z again throws it at the rock under the crosshair (with a little aim
//! assist) up to 48 m away. Z with nothing in reach puts it away, and so does
//! a moment on the ground after using it. On it, gravity swings the player as a pendulum
//! with a nudge at the bottom of each arc; W climbs the rope (fast enough to
//! zip up a cliff), S lets it out (rappelling), Space lets go with a hop, Z
//! throws again at the next anchor, F lets go and opens the wings.

use super::*;
use crate::physics::Body;
use crate::player::EYE;

/// Longest throw of the rope, metres.
pub const ROPE_MAX: f32 = 48.0;
/// The hands (and the rope) are this far above the feet.
pub(super) const HAND: f32 = 1.45;
const CLIMB_UP: f32 = 2.88;
const CLIMB_SIDE: f32 = 2.4;
const CLIMB_DOWN: f32 = 3.6;
const LUNGE_V: f32 = 9.0;
/// Gravity on the rope: lighter than walking's, so swings are big and slow
/// enough to steer.
const SWING_G: f32 = 14.0;
/// Climbing the rope, m/s.
const REEL: f32 = 8.8;
/// Letting the rope out, m/s.
const PAY_OUT: f32 = 5.0;

/// Rock, wood, anything solid but leaves.
fn hard_at(store: &Store, p: Vec3) -> bool {
    let v = |a: f32| (a * 4.0).floor() as i32;
    matches!(store.get(v(p.x), v(p.y), v(p.z)), Some(m) if m != AIR && !foliage(m))
}

/// Outward normal of the wall beside a body standing at `pos`, from the solid
/// voxels in rings around it at three heights. None when there is no wall, or
/// the rock is all round (a crack too tight to call one way).
fn wall_normal(store: &Store, pos: Vec3) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    for h in [0.4f32, 0.9, 1.4] {
        for r in [0.45f32, 0.7, 0.95] {
            for k in 0..16 {
                let a = k as f32 * std::f32::consts::TAU / 16.0;
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                if hard_at(store, pos + Vec3::Y * h + d * r) {
                    sum += d;
                }
            }
        }
    }
    (sum.length() > 0.5).then(|| -sum.normalize())
}

/// Solid in front of a climber facing into `n`, `h` metres above the feet.
fn wall_ahead(store: &Store, pos: Vec3, n: Vec3, h: f32) -> bool {
    let side = n.cross(Vec3::Y);
    [0.0f32, 0.2, -0.2].iter().any(|&s| hard_at(store, pos - n * 0.55 + side * s + Vec3::Y * h))
}

impl Ride {
    /// Walking (or swimming, or falling) into a wall at least head-high takes
    /// hold of it: at once in the air, after a moment's push on the ground.
    pub(super) fn try_grab(&mut self, p: &mut Player, input: &MoveInput, store: &Store, dt: f32) -> bool {
        if !input.forward || !self.allowed.has(ModeId::Climb) {
            self.push_t = 0.0;
            return false;
        }
        let f = dir_of(p.yaw);
        let reach = p.body.half_w + 0.25;
        // Rock at the chest and at head height, up to half a metre further off
        // than the body's edge: hoodoo bases flare out at the feet, and rock
        // leans back or narrows above a bulge.
        let near = |h: f32| [0.0f32, 0.25, 0.5].iter().any(|&k| hard_at(store, p.pos + f * (reach + k) + Vec3::Y * h));
        // On the ground, only once the feet have stopped against it.
        let stopped = !p.body.on_ground || p.body.vel.dot(f) < 1.0;
        if !(near(1.0) && near(1.6) && stopped) {
            self.push_t = 0.0;
            return false;
        }
        if p.body.on_ground {
            self.push_t += dt;
            if self.push_t < 0.25 {
                return false;
            }
        }
        self.push_t = 0.0;
        self.start_climb(p, store)
    }

    pub(super) fn start_climb(&mut self, p: &mut Player, store: &Store) -> bool {
        let Some(n) = wall_normal(store, p.pos) else {
            return false;
        };
        // Something to hold on to: rock at the chest and over the head, out
        // to an arm's reach, as walking into a wall asks for. A step at the
        // knee is ground, not a wall: a landing is no hold on the land.
        let side = n.cross(Vec3::Y);
        let holds = |h: f32| {
            [0.4f32, 0.7, 1.0]
                .iter()
                .any(|&d| [0.0f32, 0.25, -0.25].iter().any(|&k| hard_at(store, p.pos - n * d + side * k + Vec3::Y * h)))
        };
        if !holds(1.0) || !holds(1.6) {
            return false;
        }
        self.mode = Mode::Climb;
        self.wall_n = n;
        self.mantle = None;
        self.lunge = 0.0;
        p.body.vel = Vec3::ZERO;
        p.body.on_ground = false;
        self.heading = yaw_of(-n);
        self.prev_heading = self.heading;
        self.eye_y = p.pos.y + EYE;
        self.prev_eye_y = self.eye_y;
        self.tell(Mode::Climb, "CLIMBING   W UP  S DOWN  A D ACROSS  SPACE+W LUNGE  SPACE PUSH OFF  Z ROPE  C FLY");
        true
    }

    fn let_go(&mut self, p: &mut Player, v: Vec3) {
        self.mode = Mode::Foot;
        self.mantle = None;
        p.body.vel = v;
    }

    /// Pull over a top edge in front (facing into `n`) if there is room to
    /// stand on it. Starts the mantle; false when there is no top in reach.
    fn try_mantle(&mut self, p: &mut Player, store: &Store, n: Vec3) -> bool {
        let (hw, h) = (p.body.half_w, p.body.height);
        let q = p.pos - n * 0.8;
        let (vx, vz) = ((q.x * 4.0).floor() as i32, (q.z * 4.0).floor() as i32);
        let hi = ((p.pos.y + 2.6) * 4.0).floor() as i32;
        let lo = ((p.pos.y + 0.25) * 4.0).floor() as i32;
        for vy in (lo..=hi).rev() {
            if store.get(vx, vy, vz).map_or(true, |m| m != AIR) {
                let to = Vec3::new(q.x, (vy + 1) as f32 / 4.0, q.z);
                let above = Vec3::new(p.pos.x, to.y + 0.05, p.pos.z);
                if vy == hi || hits(store, to, hw, h) || hits(store, above, hw, h) {
                    return false;
                }
                self.mode = Mode::Climb;
                self.mantle = Some(Mantle { from: p.pos, to, t: 0.0 });
                self.heading = yaw_of(-n);
                p.body.vel = Vec3::ZERO;
                return true;
            }
        }
        false
    }

    pub(super) fn climb(&mut self, p: &mut Player, input: &MoveInput, store: &Store, jump_edge: bool, dt: f32) {
        self.lunge_cd -= dt;
        self.lean = 0.0;
        self.body_pitch = 0.0;
        if let Some(mut m) = self.mantle {
            // Up first, then over the edge.
            m.t = (m.t + dt / 0.4).min(1.0);
            let mid = Vec3::new(m.from.x, m.to.y + 0.05, m.from.z);
            let was = p.pos;
            p.pos = if m.t < 0.55 {
                m.from.lerp(mid, smoothstep(0.0, 1.0, m.t / 0.55))
            } else {
                mid.lerp(m.to, smoothstep(0.0, 1.0, (m.t - 0.55) / 0.45))
            };
            p.body.vel = (p.pos - was) / dt;
            self.mantle = Some(m);
            self.eye_y = p.pos.y + EYE;
            if m.t >= 1.0 {
                self.mantle = None;
                self.mode = Mode::Foot;
                p.body.vel = Vec3::ZERO;
                p.body.on_ground = true;
            }
            return;
        }
        let Some(n) = wall_normal(store, p.pos) else {
            // The wall ran out under the hands: over the top if there is one.
            let n = self.wall_n;
            if !self.try_mantle(p, store, n) {
                self.let_go(p, Vec3::ZERO);
            }
            return;
        };
        self.wall_n = (self.wall_n * 0.7 + n * 0.3).normalize_or(n);
        let n = self.wall_n;
        self.heading = yaw_of(-n);
        let right = (-n).cross(Vec3::Y).normalize();
        let up = input.forward as i32 as f32 - input.back as i32 as f32;
        let side = input.right as i32 as f32 - input.left as i32 as f32;
        if jump_edge {
            if input.forward && self.lunge_cd <= 0.0 {
                self.lunge = 0.24;
                self.lunge_cd = 0.4;
            } else {
                // Push off: away from the wall and up, toward where the player looks.
                let away = n * 4.5 + dir_of(p.yaw) * 2.5;
                self.let_go(p, Vec3::new(away.x, 5.5, away.z));
                return;
            }
        }
        self.lunge -= dt;
        let vy = if self.lunge > 0.0 {
            LUNGE_V
        } else if up > 0.0 {
            CLIMB_UP
        } else if up < 0.0 {
            -CLIMB_DOWN
        } else {
            0.0
        };
        // Hands follow the rock round curves: a light pull into the wall.
        let v = Vec3::Y * vy + right * side * CLIMB_SIDE - n * 1.2;
        let was = p.pos;
        p.body.vel = v;
        move_body(store, &mut p.pos, &mut p.body, v * dt, 0.0);
        if vy > 0.0 && p.pos.y - was.y < vy * dt * 0.5 {
            // Under a bulge: lean out around it.
            let q = p.pos + n * 0.12;
            if !hits(store, q, p.body.half_w, p.body.height) {
                p.pos = q;
            }
        }
        p.body.vel = (p.pos - was) / dt;
        // The hands take turns reaching, about one reach per 0.6 m climbed.
        self.climb_phase += (p.pos - was).length() * 5.2;
        self.eye_y = p.pos.y + EYE;
        // At the top edge: rock at the waist but not at head height.
        if vy > 0.0 && !wall_ahead(store, p.pos, n, 1.75) && wall_ahead(store, p.pos, n, 0.5) {
            self.try_mantle(p, store, n);
        }
        if up < 0.0 && p.body.on_ground {
            self.let_go(p, Vec3::ZERO);
            p.body.on_ground = true;
        }
    }

    /// Where the rope would catch along the view: the rock under the
    /// crosshair, or failing that the nearest rock within a few degrees.
    pub(super) fn find_hook(&mut self, p: &Player, store: &Store) {
        self.hook_target = None;
        self.hook_assisted = false;
        if !self.allowed.has(ModeId::Rope) {
            return;
        }
        let in_hand = match self.mode {
            Mode::Bike | Mode::Kayak => false,
            Mode::Climb | Mode::Swing => true,
            Mode::Foot | Mode::Wing => self.rope_out,
        };
        if !in_hand {
            return;
        }
        let from = p.pos + Vec3::Y * EYE;
        let dir = p.forward();
        let cast = |d: Vec3| -> Option<(Vec3, [i32; 3])> {
            let (h, prev) = store.raycast(from, d, ROPE_MAX)?;
            let m = store.get(h[0], h[1], h[2])?;
            if m == AIR || foliage(m) {
                return None;
            }
            let c = |v: [i32; 3]| Vec3::new(v[0] as f32 + 0.5, v[1] as f32 + 0.5, v[2] as f32 + 0.5) * 0.25;
            let a = (c(h) + c(prev)) * 0.5;
            ((a - from).length() > 2.0).then_some((a, h))
        };
        let mut found = cast(dir);
        if found.is_none() {
            self.hook_assisted = true;
            let right = dir.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = right.cross(dir);
            'rings: for r in [0.05f32, 0.1] {
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::TAU / 8.0;
                    if let Some(f) = cast((dir + (right * a.cos() + up * a.sin()) * r).normalize()) {
                        found = Some(f);
                        break 'rings;
                    }
                }
            }
        }
        if let Some((a, v)) = found {
            self.hook_target = Some(a);
            self.hook_target_vox = v;
        }
    }

    /// Z: get the rope out, throw it, throw it again at a new anchor, let it
    /// go, or (with nothing in reach) put it away.
    pub(super) fn hook_pressed(&mut self, p: &mut Player) {
        if self.mode != Mode::Swing && !self.can(ModeId::Rope) {
            return;
        }
        match (self.mode, self.hook_target) {
            (Mode::Bike, _) => self.note = Some("OFF THE BIKE FIRST (X)".into()),
            (Mode::Swing, Some(t)) if (t - self.anchor).length() > 3.0 => self.attach(p, t),
            (Mode::Swing, _) => self.mode = Mode::Foot,
            (Mode::Foot | Mode::Wing, _) if !self.rope_out => {
                self.rope_out = true;
                self.rope_used = false;
                self.note = Some("ROPE OUT   Z THROWS IT AT THE MARK   Z AT NOTHING PUTS IT AWAY".into());
            }
            (_, Some(t)) => self.attach(p, t),
            (Mode::Climb, None) => self.note = Some("NOTHING IN REACH FOR THE ROPE".into()),
            (_, None) => {
                self.rope_out = false;
                self.note = Some("ROPE AWAY".into());
            }
        }
    }

    fn attach(&mut self, p: &mut Player, t: Vec3) {
        if self.mode == Mode::Climb {
            p.body.vel = Vec3::ZERO;
        }
        if self.mode == Mode::Foot || self.mode == Mode::Climb {
            self.eye_y = p.pos.y + EYE;
            self.prev_eye_y = self.eye_y;
        }
        self.mode = Mode::Swing;
        self.rope_out = true;
        self.rope_used = true;
        self.mantle = None;
        self.anchor = t;
        self.anchor_vox = self.hook_target_vox;
        self.rope_len = (p.pos + Vec3::Y * HAND - t).length().max(1.5);
        self.heading = p.yaw;
        self.tell(Mode::Swing, "ROPE   W CLIMB IT  S LET IT OUT  A D SWING  SPACE LET GO  Z NEXT ANCHOR  C FLY");
    }

    pub(super) fn swing(&mut self, p: &mut Player, input: &MoveInput, store: &Store, sea: f32, jump_edge: bool, dt: f32) {
        let [ax, ay, az] = self.anchor_vox;
        if store.get(ax, ay, az).map_or(true, |m| m == AIR) {
            self.mode = Mode::Foot;
            self.note = Some("THE ROPE CAME LOOSE".into());
            return;
        }
        if jump_edge {
            // Let go with a hop and a little extra speed.
            let mut v = p.body.vel;
            let h = Vec3::new(v.x, 0.0, v.z);
            v += h * 0.08;
            v.y = v.y.max(0.0) + 4.0;
            self.mode = Mode::Foot;
            p.body.vel = v;
            p.body.on_ground = false;
            return;
        }
        let reel = input.forward && !input.back;
        let pay = input.back;
        if reel {
            self.rope_len = (self.rope_len - REEL * dt).max(1.2);
        } else if pay {
            self.rope_len = (self.rope_len + PAY_OUT * dt).min(ROPE_MAX);
        }
        let mut v = p.body.vel;
        v.y -= SWING_G * dt;
        let side = input.right as i32 as f32 - input.left as i32 as f32;
        let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
        v += right * side * 5.0 * dt;
        let d = p.pos + Vec3::Y * HAND - self.anchor;
        let dist = d.length().max(1e-3);
        let r = d / dist;
        let taut = dist >= self.rope_len - 0.05;
        if taut {
            // The rope takes any speed away from the anchor (beyond what is
            // being let out), and climbing it pulls in.
            let vr = v.dot(r);
            let want = if reel {
                -REEL
            } else if pay {
                PAY_OUT
            } else {
                0.0
            };
            if vr > want {
                v -= r * (vr - want);
            }
            // A nudge along the arc near the bottom keeps a swing going.
            let tang = v - r * v.dot(r);
            let ts = tang.length();
            if -r.y > 0.8 && ts > 3.0 {
                v += tang / ts * 3.0 * dt;
            }
        }
        if p.body.on_ground {
            // Standing under the rope: feet grip.
            let k = 1.0 - (8.0 * dt).min(1.0);
            v.x *= k;
            v.z *= k;
            if !taut && !reel {
                // Down at the bottom of a rappel, or the rope gone slack.
                self.mode = Mode::Foot;
                p.body.vel = v;
                return;
            }
        }
        p.body.vel = v;
        let res = move_body(store, &mut p.pos, &mut p.body, v * dt, 0.3);
        // Back onto the rope's reach.
        let d = p.pos + Vec3::Y * HAND - self.anchor;
        if d.length() > self.rope_len + 0.01 {
            let target = self.anchor + d.normalize() * self.rope_len - Vec3::Y * HAND;
            let mut b = Body::new(p.body.half_w, p.body.height);
            b.on_ground = p.body.on_ground;
            let delta = target - p.pos;
            move_body(store, &mut p.pos, &mut b, delta, 0.0);
        }
        self.eye_y = p.pos.y + EYE;
        self.vario = p.body.vel.y;
        let vh = Vec3::new(p.body.vel.x, 0.0, p.body.vel.z);
        self.heading = if vh.length() > 1.0 { yaw_of(vh) } else { p.yaw };
        self.lean *= 1.0 - (dt * 4.0).min(1.0);
        self.body_pitch = 0.0;

        if p.pos.y < sea - 0.5 {
            self.mode = Mode::Foot;
            return;
        }
        // Climbing the rope into a wall takes hold of it; at the top of the
        // rope, over the edge if there is one.
        if reel && res.blocked && self.start_climb(p, store) {
            return;
        }
        if reel && self.rope_len <= 1.25 {
            if let Some(n) = wall_normal(store, p.pos) {
                if !self.try_mantle(p, store, n) {
                    self.start_climb(p, store);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;
    use crate::world::material::STONE;

    /// Z to get the rope out, then Z again to throw it.
    fn throw(p: &mut Rider, s: &Store) {
        let z = MoveInput { hook: true, ..Default::default() };
        run(p, s, z, 1.0 / 60.0, false, |_, _| {});
        assert!(p.ride.rope_out, "first Z gets the rope out");
        run(p, s, MoveInput::default(), 1.0 / 60.0, false, |_, _| {});
        assert!(p.ride.hook_target.is_some(), "no mark with the rope out");
        run(p, s, z, 1.0 / 60.0, false, |_, _| {});
    }

    /// The mark shows only with the rope out or while climbing; Z at nothing
    /// puts the rope away; and landing after a swing puts it away too.
    #[test]
    fn rope_in_hand() {
        let s = cliff();
        let mut p = rider_at(6.0, 20.0);
        let idle = MoveInput::default();
        run(&mut p, &s, idle, 0.1, false, |_, _| {});
        assert!(p.ride.hook_target.is_none());
        // Climbing: the rope is out on its own.
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 2.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Climb);
        // Look back down at the ground (the wall itself is too close to hook).
        p.yaw = std::f32::consts::FRAC_PI_2;
        p.pitch = -0.6;
        run(&mut p, &s, idle, 1.0 / 60.0, false, |_, _| {});
        assert!(p.ride.hook_target.is_some(), "climbing shows the mark");
        // Down, and look at the sky: Z gets the rope out, Z again puts it away.
        run(&mut p, &s, MoveInput { back: true, ..Default::default() }, 2.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Foot);
        p.yaw = std::f32::consts::FRAC_PI_2;
        p.pitch = 1.2;
        let z = MoveInput { hook: true, ..Default::default() };
        run(&mut p, &s, z, 1.0 / 60.0, false, |_, _| {});
        run(&mut p, &s, idle, 1.0 / 60.0, false, |_, _| {});
        assert!(p.ride.rope_out && p.ride.hook_target.is_none());
        run(&mut p, &s, z, 1.0 / 60.0, false, |_, _| {});
        assert!(!p.ride.rope_out, "Z at nothing puts it away");
    }

    /// Flat ground at 20 m with a 20 m cliff face rising at x = 10 (the top at
    /// 40 m runs on to the east).
    fn cliff() -> Store {
        land(|x| if x < 10.0 { 20.0 } else { 40.0 })
    }

    /// Walking into the cliff takes hold of it; W climbs to the top and over.
    #[test]
    fn climbs_a_cliff_and_over_the_top() {
        let s = cliff();
        let mut p = rider_at(6.0, 20.0);
        let fwd = MoveInput { forward: true, ..Default::default() };
        let mut climbed = false;
        let mut t_top = None;
        run(&mut p, &s, fwd, 12.0, false, |p, t| {
            climbed |= p.ride.mode == Mode::Climb;
            if t_top.is_none() && p.ride.mode == Mode::Foot && p.pos.y > 39.0 {
                t_top = Some(t);
            }
        });
        assert!(climbed, "never took hold");
        let t = t_top.unwrap_or_else(|| panic!("didn't reach the top: at {:?}", p.pos));
        assert!(t < 8.0, "took {t} s to climb 20 m");
        assert!(p.pos.x > 10.0, "standing on top at x {}", p.pos.x);
    }

    /// A round spire that bulges and narrows (like a hoodoo) can be climbed
    /// to the top.
    #[test]
    fn climbs_a_bulging_spire() {
        let mut s = land(|_| 20.0);
        let (cx, cz) = (20.0f32, 0.0f32);
        for y in 80..200 {
            let r = 2.2 + 0.7 * ((y as f32 / 4.0) / 2.5).sin();
            for z in -16..16 {
                for x in 60..100 {
                    let (px, pz) = (x as f32 / 4.0 + 0.125, z as f32 / 4.0 + 0.125);
                    if ((px - cx).powi(2) + (pz - cz).powi(2)).sqrt() < r {
                        s.set(x, y, z, STONE);
                    }
                }
            }
        }
        let mut p = rider_at(14.0, 20.0);
        let mut top: f32 = 0.0;
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 15.0, false, |p, _| top = top.max(p.pos.y));
        assert!(top > 49.0 && p.ride.mode == Mode::Foot, "got to {top}, ending {:?} at {:?}", p.ride.mode, p.pos);
    }

    /// Space pushes off the wall; the player falls clear of it.
    #[test]
    fn pushes_off_the_wall() {
        let s = cliff();
        let mut p = rider_at(6.0, 20.0);
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 2.5, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Climb);
        p.yaw = std::f32::consts::FRAC_PI_2; // look back west
        run(&mut p, &s, MoveInput { jump: true, ..Default::default() }, 0.1, false, |_, _| {});
        run(&mut p, &s, MoveInput::default(), 1.0, false, |_, _| {});
        assert_eq!(p.ride.mode, Mode::Foot);
        assert!(p.pos.x < 8.5, "x {}", p.pos.x);
    }

    /// Throwing the rope at the cliff top from the ground and climbing it
    /// gets the player up the cliff, faster than climbing.
    #[test]
    fn rope_zips_up_a_cliff() {
        let s = cliff();
        let mut p = rider_at(-6.0, 20.0);
        // Look at the cliff's top edge.
        let edge = Vec3::new(10.2, 39.9, 0.0);
        let d = edge - (p.pos + Vec3::Y * EYE);
        p.pitch = (d.y / Vec3::new(d.x, 0.0, d.z).length()).atan();
        run(&mut p, &s, MoveInput::default(), 1.0 / 60.0, false, |_, _| {});
        assert!(p.ride.hook_target.is_none(), "the rope is put away: no mark");
        throw(&mut p, &s);
        assert_eq!(p.ride.mode, Mode::Swing);
        let mut t_top = None;
        run(&mut p, &s, MoveInput { forward: true, ..Default::default() }, 8.0, false, |p, t| {
            if t_top.is_none() && p.ride.mode == Mode::Foot && p.pos.y > 39.0 {
                t_top = Some(t);
            }
        });
        let t = t_top.unwrap_or_else(|| panic!("didn't get up: {:?} mode {:?}", p.pos, p.ride.mode));
        // Faster than climbing the 20 m face by hand.
        assert!(t < 20.0 / CLIMB_UP, "took {t} s");
        // Standing on top after using the rope: it is put away.
        run(&mut p, &s, MoveInput::default(), 1.5, false, |_, _| {});
        assert!(!p.ride.rope_out, "rope still out after landing");
    }

    /// Hanging from an overhang and letting go at the bottom of the swing
    /// carries the player well along.
    #[test]
    fn swing_carries_momentum() {
        // An arch: a slab 30 m up, from x = 20 to 40.
        let mut s = land(|_| 2.0);
        for x in 80..160 {
            for z in -8..8 {
                for y in 120..124 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        let mut p = rider_at(10.0, 20.0);
        p.body.on_ground = false;
        p.body.vel = Vec3::ZERO;
        let anchor = Vec3::new(30.0, 29.9, 0.0);
        let d = anchor - (p.pos + Vec3::Y * EYE);
        p.pitch = (d.y / Vec3::new(d.x, 0.0, d.z).length()).atan();
        throw(&mut p, &s);
        assert_eq!(p.ride.mode, Mode::Swing);
        let mut max_v: f32 = 0.0;
        let mut low_x = 0.0;
        let mut low_y = 100.0;
        run(&mut p, &s, MoveInput::default(), 3.0, false, |p, _| {
            max_v = max_v.max(p.body.vel.length());
            if p.pos.y < low_y {
                low_y = p.pos.y;
                low_x = p.pos.x;
            }
        });
        assert!(max_v > 12.0, "swing speed {max_v}");
        assert!((low_x - 30.0f32).abs() < 4.0, "lowest point at x {low_x}");
        assert!(low_y > 4.0, "hit the ground at {low_y}");
    }
}
