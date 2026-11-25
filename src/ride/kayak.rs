//! The kayak: a sit-in boat on any water deep enough to float it, sea,
//! river or lake. Like the bike it goes where the view points (A/D turn the
//! view), glides on between strokes and has a sprint (Shift); W paddles in
//! alternate strokes, S back-paddles. The current carries it. It runs aground
//! where the bed comes within a hand of the surface, and the paddler's head
//! has to clear whatever it passes under: a stilt bridge lower than about a
//! metre over the water stops it. It cruises at about 30 km/h and coasts a
//! long way between strokes; when the bed comes up under the hull it says
//! so, and keeps saying so while you paddle at it.

use super::*;

/// Eye over the water surface, sitting in the boat.
pub(super) const KAYAK_EYE: f32 = 0.82;
/// Stroke power fades out toward these speeds, paddling with W and with
/// Shift, m/s; against the water's drag the boat settles at about 8.3 m/s
/// (30 km/h) and 11 m/s (40 km/h), so the kayak is plainly faster than
/// walking and a little faster than the bike on the flat.
const CRUISE_K: f32 = 9.5;
const SPRINT: f32 = 14.8;
/// Strokes a second (one on each side makes a full cycle of two).
const CADENCE: f32 = 1.5;
/// Water needed under the hull, metres.
pub(super) const DRAFT: f32 = 0.18;
/// From the seat to the bow and to the stern, metres.
const BOW: f32 = 1.55;
const STERN: f32 = 1.45;
/// Height over the water the paddler's head needs clear.
const HEADROOM: f32 = 1.05;

/// The bed under (x, z), searching down from just over the water. Water
/// deeper than the search (40 m) reads as 40 m deep: launching a kayak
/// offshore, from a swim, used to fail at 8 m.
fn bed(store: &Store, x: f32, z: f32, level: f32) -> Option<f32> {
    top_below(store, x, z, level + 0.6, 40.0)
        .or(Some(level - 40.0))
        .filter(|_| store.loaded((x * 4.0).floor() as i32, (level * 4.0).floor() as i32, (z * 4.0).floor() as i32))
}

impl Ride {
    /// K: get into the kayak at the water's edge, or out of it onto the
    /// nearest footing (or over the side into deep water). `level` is the
    /// water surface at the player.
    pub fn toggle_kayak(&mut self, p: &mut Player, store: &Store, level: f32) {
        match self.mode {
            Mode::Foot => {
                if !self.can(ModeId::Kayak) {
                    return;
                }
                // Back into the kayak left moored, from the water beside it.
                if let Some((at, heading)) = self.moored {
                    let near = Vec3::new(at.x - p.pos.x, 0.0, at.z - p.pos.z).length() < 2.6;
                    let afloat = p.swimming && (p.pos.y + crate::player::EYE - level).abs() < 1.2;
                    if near && afloat {
                        self.moored = None;
                        self.board(p, at, heading);
                        self.note = Some("BACK IN THE KAYAK".into());
                        return;
                    }
                }
                // The nearest water deep enough to float in, the feet first.
                let mut best: Option<(f32, Vec3)> = None;
                for ring in [0.0f32, 1.0, 2.0, 3.0] {
                    let n = if ring == 0.0 { 1 } else { 12 };
                    for k in 0..n {
                        let a = k as f32 * std::f32::consts::TAU / n as f32;
                        let q = p.pos + Vec3::new(a.cos(), 0.0, a.sin()) * ring;
                        let Some(b) = bed(store, q.x, q.z, level) else {
                            continue;
                        };
                        if level - b >= DRAFT + 0.1 && !hits_hard(store, Vec3::new(q.x, level + 0.1, q.z), 0.3, HEADROOM) && best.is_none_or(|(d, _)| ring < d) {
                            best = Some((ring, q));
                        }
                    }
                    if best.is_some() {
                        break;
                    }
                }
                let Some((_, q)) = best else {
                    self.note = Some("NO WATER DEEP ENOUGH HERE".into());
                    return;
                };
                // There is one kayak: launching it here takes it from
                // wherever it was moored.
                self.moored = None;
                self.board(p, Vec3::new(q.x, level, q.z), p.yaw);
            }
            Mode::Kayak => {
                // Step out onto the nearest footing: ground at most knee deep.
                let mut best: Option<(f32, Vec3)> = None;
                for ring in [0.0f32, 1.0, 1.8, 2.6] {
                    let n = if ring == 0.0 { 1 } else { 12 };
                    for k in 0..n {
                        let a = k as f32 * std::f32::consts::TAU / n as f32;
                        let q = p.pos + Vec3::new(a.cos(), 0.0, a.sin()) * ring;
                        let Some(b) = top_below(store, q.x, q.z, level + 1.4, 3.0) else {
                            continue;
                        };
                        if b > level - 0.5 && !hits_hard(store, Vec3::new(q.x, b + 0.1, q.z), 0.25, 1.6) && best.is_none_or(|(d, _)| ring < d) {
                            best = Some((ring, Vec3::new(q.x, b, q.z)));
                        }
                    }
                    if best.is_some() {
                        break;
                    }
                }
                self.mode = Mode::Foot;
                p.body.vel = dir_of(self.heading) * self.speed * 0.3;
                match best {
                    Some((_, q)) => {
                        p.pos = q + Vec3::Y * 0.02;
                        p.body.on_ground = true;
                        self.note = Some("ON FOOT".into());
                    }
                    None => {
                        // Over the side into deep water; the boat stays,
                        // moored, to climb back into (K beside it).
                        self.moored = Some((Vec3::new(p.pos.x, level, p.pos.z), self.heading));
                        p.pos.y = level - 1.0;
                        p.pos += dir_of(self.heading + std::f32::consts::FRAC_PI_2) * 0.9;
                        self.note = Some("OVER THE SIDE   CTRL DIVE   K BESIDE THE KAYAK TO CLIMB IN".into());
                    }
                }
            }
            _ => self.note = Some("LAND FIRST".into()),
        }
    }

    /// Into the kayak at `at` (on the water), facing `heading`.
    fn board(&mut self, p: &mut Player, at: Vec3, heading: f32) {
        self.mode = Mode::Kayak;
        self.rope_out = false;
        p.pos = at;
        p.swimming = false;
        p.dive.under = false;
        self.heading = heading;
        self.prev_heading = heading;
        p.yaw = heading;
        self.speed = 0.0;
        self.aground = false;
        if self.eye_y == 0.0 {
            self.eye_y = p.eye(1.0).y;
            self.prev_eye_y = self.eye_y;
        }
        self.note = Some("KAYAK   W PADDLE  SHIFT SPRINT  S BACK  A D TURN  K GET OUT  V CAMERA".into());
    }

    /// One step paddling. `level` is the water surface at the boat.
    pub(super) fn kayak(&mut self, p: &mut Player, input: &MoveInput, store: &Store, level: f32, dt: f32) {
        // Steering: toward the view, slowly; A/D turn the view (in
        // `Ride::key_turn`, once a frame), and with it the boat, in sweep
        // strokes even when it isn't moving.
        let diff = wrap(p.yaw - self.heading);
        let rate = 1.1 + 0.25 * (1.0 - smoothstep(0.0, 4.0, self.speed.abs()));
        let want = if diff.abs() < 1.7 { (diff * 3.0).clamp(-rate, rate) } else { 0.0 };
        self.steer += (want - self.steer) * (dt * 3.5).min(1.0);
        self.heading = wrap(self.heading + self.steer * dt);
        self.turn_rate += (self.steer - self.turn_rate) * (dt * 4.0).min(1.0);

        // Strokes: the paddle turns over while paddling or sweeping; each
        // stroke pushes hardest mid-way, so the boat surges a little with each.
        let keys = input.side();
        let working = input.forward || input.back || keys != 0.0 || self.steer.abs() > 0.25;
        let cadence = CADENCE * if self.boosting { 1.35 } else { 1.0 };
        if working {
            self.paddle_t += dt * cadence;
        } else {
            // Bring the paddle level between strokes.
            let frac = self.paddle_t.fract();
            if frac > 0.02 {
                self.paddle_t += dt * cadence * 0.8;
                if self.paddle_t.fract() < frac {
                    self.paddle_t = self.paddle_t.floor();
                }
            }
        }
        let pulse = if working { (self.paddle_t.fract() * std::f32::consts::PI).sin() } else { 0.0 };
        // Left stroke on even strokes, right on odd.
        let side = if (self.paddle_t.floor() as i64) % 2 == 0 { -1.0 } else { 1.0 };
        self.stroke = pulse * side;
        let cruise = if self.boosting { SPRINT } else { CRUISE_K };
        let mut a = 0.0;
        if input.forward && self.speed < cruise {
            a += 5.0 * pulse * (1.0 - self.speed / cruise).max(0.0) + 0.2;
        }
        if input.back {
            a -= if self.speed > 0.2 {
                3.5
            } else if self.speed > -1.2 {
                1.6 * pulse
            } else {
                0.0
            };
        }
        // Glide: water drag, light at paddling speed, so the boat coasts a
        // long way (about 13 s to lose half its speed from a cruise).
        a -= 0.014 * self.speed + 0.007 * self.speed * self.speed.abs();
        // Turning scrubs speed.
        a -= self.steer.abs() * self.speed * 0.12;
        self.speed += a * dt;
        // Each stroke nudges the bow away from its side.
        self.heading = wrap(self.heading - side * pulse * 0.05 * dt * (input.forward as i32 as f32));
        let dir = dir_of(self.heading);

        // Over the water: the boat's own way plus the current.
        let drift = Vec3::new(self.current.x, 0.0, self.current.y);
        let total = (dir * self.speed + drift) * dt;
        let n = (total.length() / 0.15).ceil().max(1.0) as i32;
        let sub = total / n as f32;
        let mut aground = false;
        let mut impact = 0.0f32;
        for _ in 0..n {
            let ahead = p.pos + sub * 4.0;
            if !store.loaded((ahead.x * 4.0).floor() as i32, (level * 4.0).floor() as i32, (ahead.z * 4.0).floor() as i32) {
                break;
            }
            let mut moved = false;
            for d in [sub, Vec3::new(sub.x, 0.0, 0.0), Vec3::new(0.0, 0.0, sub.z)] {
                if d.length_squared() < 1e-10 {
                    continue;
                }
                match self.hull_clear(store, p.pos + d, level) {
                    Ok(()) => {
                        p.pos += Vec3::new(d.x, 0.0, d.z);
                        moved = true;
                        if d != sub {
                            self.speed *= 0.99;
                        }
                        break;
                    }
                    Err(shallow) => aground |= shallow,
                }
            }
            if !moved {
                impact = self.speed.abs();
                self.speed = if aground { self.speed * 0.5 } else { -self.speed * 0.2 };
                break;
            }
        }
        // Aground is a fact about where the boat sits, not about the last
        // move: test the hull where it is, so the HUD keeps saying so while
        // you sit still instead of only while you push at the bar.
        let aground = aground || matches!(self.hull_clear(store, p.pos, level), Err(true));
        if aground && !self.aground {
            self.note = Some("AGROUND   BACK OFF WITH S, OR K TO GET OUT".into());
            self.aground_said = self.t;
        } else if aground && self.t - self.aground_said > 4.0 && (input.forward || input.back) {
            self.note = Some("STILL AGROUND   BACK OFF WITH S, OR K TO GET OUT".into());
            self.aground_said = self.t;
        }
        self.aground = aground;
        if impact > 2.0 {
            self.shake += (impact * 0.015).min(0.08);
            self.bob_v -= impact * 0.05;
        }
        // Pushing through reeds and leaves slows the boat.
        if hits_soft(store, p.pos + Vec3::Y * 0.2, 0.3, 0.9).1 {
            self.speed *= 1.0 - (1.5 * dt).min(1.0);
        }

        // The water fell away under the boat (a river's surface ended): out.
        match bed(store, p.pos.x, p.pos.z, level) {
            Some(b) if b > level - 0.02 => {
                self.mode = Mode::Foot;
                p.pos.y = b;
                p.body.vel = Vec3::ZERO;
                self.note = Some("ON FOOT".into());
                return;
            }
            _ => {}
        }
        p.pos.y = level;
        p.body.vel = dir * self.speed + drift;
        p.body.on_ground = true;
        p.swimming = false;

        // Pose: lean into the turn, a little roll with each stroke, and a
        // slow swell under the boat. On the sea's swell the hull pitches
        // and rolls with the water's slope: nose up as a wave lifts the bow,
        // the high side up as one runs under it from the side.
        let fwd = dir_of(self.heading);
        let right = Vec3::new(self.heading.cos(), 0.0, -self.heading.sin());
        let ws = Vec3::new(self.water_slope.x, 0.0, self.water_slope.y);
        let wave_pitch = ws.dot(fwd).atan().clamp(-0.35, 0.35);
        let wave_roll = ws.dot(right).atan().clamp(-0.3, 0.3);
        let lean = (self.speed * self.turn_rate * 0.06).clamp(-0.18, 0.18) + 0.025 * self.stroke + wave_roll;
        self.lean += (lean - self.lean) * (dt * 5.0).min(1.0);
        self.body_pitch += (wave_pitch - self.body_pitch) * (dt * 4.0).min(1.0);
        let swell = 0.025 * (self.t * 1.3).sin() + 0.012 * (self.t * 2.9 + 1.0).sin();
        let target = level + KAYAK_EYE + swell;
        let (x, v) = smooth_damp((self.eye_y - self.bob - target).clamp(-0.7, 0.7), self.eye_v, 0.12, dt);
        let (b, bv) = spring(self.bob, self.bob_v, 1.8, 0.5, dt);
        self.bob = b.clamp(-0.3, 0.3);
        self.bob_v = bv;
        self.eye_y = target + x + self.bob;
        self.eye_v = v;
        self.shake *= 1.0 - (dt * 6.0).min(1.0);
    }

    /// Can the boat sit with its seat at `q`? Err(true) when the bed comes up
    /// too close under the hull (aground), Err(false) when something stands
    /// in the way at the waterline or over the paddler's head.
    fn hull_clear(&self, store: &Store, q: Vec3, level: f32) -> Result<(), bool> {
        let dir = dir_of(self.heading);
        for (k, along) in [(0, 0.0f32), (1, BOW), (2, -STERN)] {
            let s = q + dir * along;
            // A post, a root, a bank: anything solid just over the water.
            if hits_hard(store, Vec3::new(s.x, level + 0.05, s.z), if k == 0 { 0.3 } else { 0.12 }, 0.4) {
                return Err(false);
            }
            if bed(store, s.x, s.z, level).is_some_and(|b| b > level - DRAFT) {
                return Err(true);
            }
        }
        // The paddler's head.
        if hits_hard(store, Vec3::new(q.x, level + 0.45, q.z), 0.25, HEADROOM - 0.45) {
            return Err(false);
        }
        Ok(())
    }

    /// The paddle, drawn in both cameras: held across the body, turning over
    /// stroke by stroke.
    pub(super) fn paddle(&self, p: &Player, alpha: f32, first: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        let Some(md) = models.get(first + PADDLE) else {
            return;
        };
        let yaw = self.prev_heading + wrap(self.heading - self.prev_heading) * alpha;
        let chase = self.chase_on() || self.show_body;
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let base = p.prev_pos.lerp(p.pos, alpha);
        let eye = self.prev_eye_y + (self.eye_y - self.prev_eye_y) * alpha;
        // Grip centre: in front of the chest, lower in first person so the
        // shaft runs across the bottom of the view.
        let fwd = dir_of(yaw);
        let centre = if chase {
            Vec3::new(base.x, eye - 0.3, base.z) + fwd * 0.35
        } else {
            Vec3::new(base.x, eye - 0.7, base.z) + fwd * 0.55
        };
        // The shaft rolls side to side: the working blade dips into the water.
        let roll = 0.55 * self.stroke + 0.04 * (self.t * 2.0).sin();
        let sweep = 0.3 * self.stroke;
        let q = Quat::from_rotation_y(yaw + sweep) * Quat::from_rotation_z(roll + lerp(self.prev_lean, self.lean));
        // The model's pivot is the shaft's middle.
        out.push((
            first + PADDLE,
            Instance {
                pos_yaw: [centre.x, centre.y, centre.z, 0.0],
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

/// The kayak, bow toward -Z, the cockpit about the pivot; 1/8 m voxels. With
/// `paddler`, the paddler sitting in it (the chase camera's model).
pub(super) fn kayak_model(g: &mut Grid, paddler: bool) {
    // Hull: an ellipsoid's top half pinched to points, 3.0 m long, 0.6 m wide.
    for z in 3..29 {
        let t = (z as f32 + 0.5 - 16.0) / 13.0;
        let half = 2.4 * (1.0 - t * t).max(0.0).sqrt() * (1.0 - 0.25 * t.abs());
        if half < 0.4 {
            continue;
        }
        let deck = 2.0 + 0.6 * t * t;
        for x in 0..32 {
            let dx = x as f32 + 0.5 - 16.0;
            if dx.abs() <= half {
                let y_hi = (deck * (1.0 - (dx / half.max(0.5)).powi(2) * 0.35)).ceil() as i32;
                g.fill([x, 0, z], [x + 1, y_hi.max(1), z + 1], HULL);
            }
        }
    }
    // The cockpit rim and the dark well.
    g.fill([14, 2, 14], [18, 3, 21], SUIT_DARK);
    seg(g, [13.5, 2.6, 13.5], [18.5, 2.6, 13.5], 0.5, TYRE);
    // Deck lines fore and aft.
    seg(g, [16.0, 2.7, 5.0], [16.0, 2.7, 12.0], 0.35, CORD);
    seg(g, [16.0, 2.6, 22.0], [16.0, 2.6, 27.0], 0.35, CORD);
    if paddler {
        // Torso in a buoyancy vest, arms forward to the paddle, head.
        g.ellipsoid([16.0, 5.5, 17.0], [2.1, 3.0, 1.6], SUIT);
        g.ellipsoid([16.0, 4.0, 17.2], [2.3, 1.2, 1.8], JACKET);
        for x in [13.6f32, 18.4] {
            seg(g, [x, 7.0, 16.6], [x + (16.0 - x) * 0.2, 6.2, 14.0], 0.6, JACKET);
        }
        g.ellipsoid([16.0, 9.6, 16.8], [1.3, 1.4, 1.3], SKIN);
        g.ellipsoid([16.0, 10.5, 16.9], [1.6, 0.7, 1.6], HELMET);
    }
}

/// The first-person bow: the deck from the cockpit forward, 1/16 m voxels so
/// its edges stay fine this close to the eye. The seat is at voxel z 30 (the
/// pivot, `bow_pivot`), the bow at z 5.
pub(super) fn bow_model(g: &mut Grid) {
    for z in 4..32 {
        // 0 at the bow tip, 1 at the seat.
        let t = ((z as f32 + 0.5 - 4.0) / 26.0).clamp(0.0, 1.0);
        let half = 5.0 * (t * (2.0 - t)).sqrt();
        if half < 0.5 {
            continue;
        }
        // The deck crowns along the middle and sheers up toward the bow.
        let deck = 4.0 + 1.2 * (1.0 - t);
        for x in 0..32 {
            let dx = x as f32 + 0.5 - 16.0;
            if dx.abs() <= half {
                let k = (dx / half).powi(2);
                let y_hi = (deck - 1.6 * k).round().max(1.0) as i32;
                g.fill([x, 0, z], [x + 1, y_hi, z + 1], HULL);
            }
        }
    }
    // The cockpit's front rim and the deck line.
    for x in 11..21 {
        g.fill([x, 4, 28], [x + 1, 6, 30], TYRE);
    }
    g.fill([16, 5, 8], [17, 6, 27], TYRE);
}

/// The first-person bow's pivot: the seat.
pub(super) fn bow_pivot(m: &mut Model) {
    m.pivot = Vec3::new(16.0, 0.0, 30.0);
}

/// The paddle: a shaft along X with a blade at each end, pivot in the
/// middle; 1/14 m voxels (2.2 m long). With hands on the shaft.
pub(super) fn paddle_model(g: &mut Grid) {
    seg(g, [1.5, 4.5, 16.5], [30.5, 4.5, 16.5], 0.5, SUIT_DARK);
    for x0 in [0i32, 27] {
        g.fill([x0, 2, 16], [x0 + 5, 7, 17], BLADE);
    }
    for x in [10.5f32, 21.5] {
        g.ellipsoid([x, 4.5, 16.5], [0.9, 0.9, 0.9], BOOT);
    }
}

/// The paddle model's pivot is its middle, not its foot.
pub(super) fn paddle_pivot(m: &mut Model) {
    m.pivot = Vec3::new(16.0, 4.5, 16.5);
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    /// Paddle for `secs` with the water at `level` (the bed is the store's ground).
    fn paddle(p: &mut Rider, s: &Store, level: f32, secs: f32) {
        let input = MoveInput { forward: true, ..Default::default() };
        let dt = 1.0 / 60.0;
        for _ in 0..(secs / dt) as i32 {
            p.update(&input, s, level, (level, glam::Vec2::ZERO), false, dt);
        }
    }

    fn launch(p: &mut Rider, s: &Store, level: f32) {
        let mut r = std::mem::take(&mut p.ride);
        r.toggle_kayak(p, s, level);
        p.ride = r;
    }

    #[test]
    fn paddles_on_water_a_metre_deep() {
        let s = land(|_| 10.0);
        let mut p = rider_at(0.0, 11.0);
        launch(&mut p, &s, 11.0);
        assert_eq!(p.ride.mode, Mode::Kayak);
        paddle(&mut p, &s, 11.0, 6.0);
        assert!(p.pos.x > 12.0, "only got to {}", p.pos.x);
        assert!((p.pos.y - 11.0).abs() < 0.01, "floats at the surface, not {}", p.pos.y);
        let v = p.ride.speed(&p);
        assert!(v > 6.5 && v < 9.0, "cruising speed {v}");
    }

    #[test]
    fn runs_aground_in_the_shallows() {
        // A metre of water, then a bar 10 cm under the surface from x = 8.
        let s = land(|x| if x < 8.0 { 10.0 } else { 10.9 });
        let mut p = rider_at(0.0, 11.0);
        launch(&mut p, &s, 11.0);
        paddle(&mut p, &s, 11.0, 8.0);
        assert!(p.pos.x < 8.0, "got over the bar to {}", p.pos.x);
        assert_eq!(p.ride.mode, Mode::Kayak);
    }

    #[test]
    fn kayak_then_scuba_then_kayak_again() {
        // Ten metres of water over flat rock: roll off, dive, come back up
        // by the boat and climb back in.
        let s = land(|_| 10.0);
        let mut p = rider_at(40.0, 20.0);
        p.dive.currents = false;
        launch(&mut p, &s, 20.0);
        paddle(&mut p, &s, 20.0, 2.0);
        launch(&mut p, &s, 20.0);
        assert_eq!(p.ride.mode, Mode::Foot);
        let (boat, _) = p.ride.moored.unwrap_or_else(|| panic!("the kayak was not left moored: {:?} {:?}", p.pos, p.ride.note));
        let dt = 1.0 / 60.0;
        let step = |p: &mut Rider, input: MoveInput, secs: f32| {
            for _ in 0..(secs / dt) as i32 {
                p.update(&input, &s, 20.0, (20.0, glam::Vec2::ZERO), false, dt);
            }
        };
        step(&mut p, MoveInput::default(), 1.5);
        step(&mut p, MoveInput { down: true, ..Default::default() }, 3.0);
        assert!(p.dive.under && p.pos.y < 17.0, "did not dive: {:?}", p.pos);
        step(&mut p, MoveInput { jump: true, ..Default::default() }, 6.0);
        assert!(!p.dive.under, "did not come up: {:?}", p.pos);
        step(&mut p, MoveInput::default(), 1.0);
        assert!(Vec3::new(boat.x - p.pos.x, 0.0, boat.z - p.pos.z).length() < 2.6, "drifted from the boat");
        launch(&mut p, &s, 20.0);
        assert_eq!(p.ride.mode, Mode::Kayak, "could not climb back in");
        assert!(p.ride.moored.is_none());
    }

    #[test]
    fn no_kayak_on_dry_land() {
        let s = land(|_| 10.0);
        let mut p = rider_at(0.0, 10.0);
        launch(&mut p, &s, 5.0);
        assert_eq!(p.ride.mode, Mode::Foot);
    }
}
