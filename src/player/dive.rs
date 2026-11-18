//! Scuba (docs/UNDERWATER.md): swimming free under the water, in any water
//! deep enough to go under. Ctrl at the surface dives; from then on the body
//! is a swimmer lying flat that goes where the view points.
//!
//! - W/S along the view, pitch included; A/D sideways; Space up, Ctrl down.
//!   Let go of everything and you hang where you are (neutral buoyancy),
//!   drifting with the current.
//! - Shift is a kick: a burst that eases back to cruising over a second. A
//!   kick in the window as the last one fades chains, up to three, each a
//!   little stronger; pressing again too soon does nothing.
//! - A breath lasts `BREATH` seconds under the surface and is whole again
//!   the moment the head is back out. Nobody drowns: over the last ten
//!   seconds the swim slows, and with none left the body rises on its own.
//! - Coming up fast breaks the surface in a leap, and the body falls back in.
//! - Held onto a big swimmer (`hitch`), the game moves the body with it.

use super::{Player, EYE, STAND_H};
use crate::physics::{hits_wet, move_body_with};
use crate::world::store::{Cell, Store};
use glam::Vec3;

/// Seconds of breath under the surface.
pub const BREATH: f32 = 60.0;
/// Water needed under the floating body to go under, metres.
pub const MIN_DEPTH: f32 = 1.5;
/// A swimmer lying flat: the collision box is this tall, so a gap in a
/// wreck or between coral heads a standing body would not fit can be swum.
pub const DIVE_H: f32 = 0.7;
/// The eye over the body's base while diving: about its middle.
pub const DIVE_EYE: f32 = 0.42;
/// Cruising speed, m/s, and the burst a kick gives.
const CRUISE: f32 = 2.6;
const KICK: f32 = 5.6;
/// How fast the swim eases toward what the keys ask, per second.
const EASE: f32 = 1.9;
/// A kick pressed this long after the last one chains; sooner is ignored.
const CHAIN_FROM: f32 = 0.45;
const CHAIN_UNTIL: f32 = 1.3;
/// Rising faster than this as the head breaks the surface is a leap.
const LEAP_V: f32 = 2.6;
/// The last seconds of a breath, in which the swim slows.
const TIRED: f32 = 10.0;

/// A big swimmer the diver is holding on to.
#[derive(Clone, Copy, Debug)]
pub struct Hitch {
    pub who: hecs::Entity,
    /// Where the diver hangs, in the animal's own frame (x right, y up,
    /// z behind its head), metres.
    pub off: Vec3,
}

#[derive(Debug)]
pub struct Dive {
    /// Under the water and swimming free.
    pub under: bool,
    /// Seconds of breath left.
    pub breath: f32,
    /// Seconds since the last kick, and how many have chained.
    since_kick: f32,
    pub chain: u32,
    sprint_was: bool,
    down_was: bool,
    pub hitch: Option<Hitch>,
    /// Seconds to the next breath of bubbles.
    bubble_t: f32,
    /// Bubbles and splashes for the game to throw (fx names, where, size).
    pub cues: Vec<(&'static str, Vec3, f32)>,
    /// A line for the HUD, taken by the game.
    pub note: Option<String>,
    /// How fast the sea's current is carrying the diver, m/s (0 out of one).
    pub stream: f32,
    /// Feel the sea's currents (off in the movement tests, which measure
    /// the swimming alone).
    pub currents: bool,
}

impl Default for Dive {
    fn default() -> Self {
        Dive {
            under: false,
            breath: BREATH,
            since_kick: 9.0,
            chain: 0,
            sprint_was: false,
            down_was: false,
            hitch: None,
            bubble_t: 2.0,
            cues: Vec::new(),
            note: None,
            stream: 0.0,
            currents: true,
        }
    }
}

/// Whether a body can be at `p`: open water or air, not rock.
fn open(store: &Store, p: Vec3) -> bool {
    let (x, y, z) = ((p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32);
    matches!(store.cell(x, y, z), Cell::Open | Cell::Water | Cell::Passable)
}

/// Is there at least `MIN_DEPTH` of water under the surface at (x, z)?
pub fn deep_enough(store: &Store, x: f32, z: f32, level: f32) -> bool {
    let mut y = level - 0.125;
    while y > level - MIN_DEPTH {
        if !open(store, Vec3::new(x, y, z)) {
            return false;
        }
        y -= 0.25;
    }
    true
}

/// The view's direction, pitch included.
fn look(p: &Player) -> Vec3 {
    let (sy, cy) = p.yaw.sin_cos();
    let (sp, cp) = p.pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

impl Player {
    /// One step of scuba, ahead of the walking step. True when it moved the
    /// body (diving), false to let the walk (and the surface float) run.
    pub(super) fn dive_step(&mut self, input: &super::MoveInput, store: &Store, level: f32, current: glam::Vec2, dt: f32) -> bool {
        let pressed_down = input.down && !self.dive.down_was;
        self.dive.down_was = input.down;
        let kick = input.sprint && !self.dive.sprint_was;
        self.dive.sprint_was = input.sprint;
        self.dive.since_kick += dt;
        if !self.dive.under {
            // The head is out: a whole breath again.
            if self.pos.y + EYE > level - 0.25 {
                self.dive.breath = BREATH;
            }
            if self.swimming && pressed_down {
                if deep_enough(store, self.pos.x, self.pos.z, level) {
                    self.go_under();
                } else {
                    self.dive.note = Some("TOO SHALLOW TO DIVE".into());
                }
            }
            if !self.dive.under {
                return false;
            }
        }
        let look = look(self);
        let (sy, cy) = self.yaw.sin_cos();
        let d = &mut self.dive;
        d.breath = (d.breath - dt).max(0.0);
        if d.hitch.is_some() {
            // The game carries the body; letting go of E or kicking off
            // leaves it swimming where it is, with the animal's speed.
            if !input.focus_far || kick {
                d.hitch = None;
            } else {
                return true;
            }
        }
        let right = Vec3::new(cy, 0.0, -sy);
        let f = input.forward as i32 as f32 - input.back as i32 as f32;
        let r = input.right as i32 as f32 - input.left as i32 as f32;
        let u = input.jump as i32 as f32 - input.down as i32 as f32;
        let mut wish = (look * f + right * r + Vec3::Y * u).normalize_or_zero();
        let mut speed = CRUISE;
        if d.breath < TIRED {
            speed *= 0.5 + 0.5 * d.breath / TIRED;
        }
        if d.breath <= 0.0 {
            // Out of breath: up, whatever the keys say.
            wish = Vec3::Y;
            speed = 1.6;
        }
        if kick && d.breath > 0.0 && d.since_kick > CHAIN_FROM {
            d.chain = if d.since_kick < CHAIN_UNTIL { (d.chain + 1).min(3) } else { 1 };
            d.since_kick = 0.0;
            let dir = if wish == Vec3::ZERO { look } else { wish };
            let burst = KICK * (1.0 + 0.22 * (d.chain - 1) as f32);
            let along = self.body.vel.dot(dir);
            self.body.vel += dir * (burst - along).max(0.0);
            d.cues.push(("bubbles", self.pos + Vec3::Y * DIVE_EYE, 0.6 + 0.2 * d.chain as f32));
        }
        if d.since_kick > CHAIN_UNTIL {
            d.chain = 0;
        }
        // Ease toward what the keys ask, plus the current -- a river's, or
        // the sea's streams -- with no gravity.
        let stream = if d.currents { sea_current(self.pos + Vec3::Y * DIVE_EYE, level, |x, z| deep_for_current(store, x, z, level)) } else { Vec3::ZERO };
        d.stream = stream.length();
        let target = wish * speed + Vec3::new(current.x, 0.0, current.y) + stream;
        self.body.vel += (target - self.body.vel) * (EASE * dt).min(1.0);
        d.bubble_t -= dt;
        if d.bubble_t <= 0.0 {
            d.bubble_t = 3.5 + (self.pos.x * 7.3 + self.pos.z * 3.1).sin().abs() * 1.5;
            d.cues.push(("bubbles", self.pos + Vec3::Y * DIVE_EYE + look * 0.2, 0.5));
        }
        let delta = self.body.vel * dt;
        move_body_with(store, &mut self.pos, &mut self.body, delta, 0.0, hits_wet);
        self.body.on_ground = false;
        // At the surface: out of the dive, or out of the water in a leap.
        let eye = self.pos.y + DIVE_EYE;
        if eye > level - 0.08 && self.body.vel.y >= -0.2 {
            let leap = self.body.vel.y > LEAP_V;
            self.come_up(store, level);
            if leap {
                self.body.vel.y = self.body.vel.y * 1.5 + 1.5;
                self.dive.cues.push(("splash", Vec3::new(self.pos.x, level, self.pos.z), 0.9));
            }
        }
        true
    }

    /// From floating at the surface to diving: the eye stays where it is
    /// and the body under it becomes a swimmer lying flat.
    fn go_under(&mut self) {
        let eye = self.pos.y + EYE + self.eye_offset;
        self.dive.under = true;
        self.dive.chain = 0;
        self.dive.since_kick = 9.0;
        self.body.height = DIVE_H;
        self.crouch = 0.0;
        self.prev_crouch = 0.0;
        self.pos.y = eye - DIVE_EYE;
        self.prev_pos.y = self.pos.y;
        self.eye_offset = DIVE_EYE - EYE;
        self.prev_eye_offset = self.eye_offset;
        self.eye_vel = 0.0;
        self.lift = 0.0;
        self.body.vel.y = self.body.vel.y.min(-1.4);
        self.dive.cues.push(("bubbles", self.pos + Vec3::Y * DIVE_EYE, 0.8));
    }

    /// Back to floating: the eye stays put and the body stands up under it,
    /// so the surface float picks it up from there.
    pub(super) fn come_up(&mut self, store: &Store, level: f32) {
        let eye = self.pos.y + DIVE_EYE;
        self.dive.under = false;
        self.dive.hitch = None;
        self.dive.stream = 0.0;
        self.dive.chain = 0;
        self.dive.breath = super::dive::BREATH;
        // The body stands up under the eye. Over a shallow reef that can put
        // its feet in the bed; the walking step frees a body from rock
        // (`free_body`), so it is never left half in a dive.
        let stood = Vec3::new(self.pos.x, eye - EYE, self.pos.z);
        let _ = (store, level);
        self.pos = stood;
        self.prev_pos.y = self.pos.y;
        self.body.height = STAND_H;
        self.eye_offset = 0.0;
        self.prev_eye_offset = 0.0;
    }

    /// Where the eye sits over the body: under water it is the middle of a
    /// swimmer lying flat.
    pub fn diving(&self) -> bool {
        self.dive.under
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::MoveInput;
    use crate::ride::testutil::land;
    use crate::ride::Ride;

    /// A sea 20 m deep over flat rock: the bed at 4 m, the surface at 24.
    fn sea() -> (Store, f32) {
        (land(|_| 4.0), 24.0)
    }

    fn step(p: &mut Player, r: &mut Ride, s: &Store, level: f32, input: &MoveInput, secs: f32) {
        let n = (secs * 60.0) as usize;
        for _ in 0..n {
            p.update(r, input, s, level, (level, glam::Vec2::ZERO), false, 1.0 / 60.0);
        }
    }

    fn floating(s: &Store, level: f32) -> (Player, Ride) {
        let mut p = Player::new(Vec3::new(40.0, level - 1.0, 0.0), 0.0);
        p.dive.currents = false;
        let mut r = Ride::default();
        step(&mut p, &mut r, s, level, &MoveInput::default(), 2.0);
        assert!(p.swimming && !p.dive.under, "not floating: {:?}", p.pos);
        (p, r)
    }

    #[test]
    fn ctrl_at_the_surface_dives_and_the_view_steers_down() {
        let (s, level) = sea();
        let (mut p, mut r) = floating(&s, level);
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 0.1);
        assert!(p.dive.under, "Ctrl did not dive");
        p.pitch = -0.9;
        step(&mut p, &mut r, &s, level, &MoveInput { forward: true, ..Default::default() }, 4.0);
        assert!(p.pos.y < level - 6.0, "swimming down the view went nowhere: {}", p.pos.y);
        assert!(p.pos.y > 4.0, "went through the bed");
    }

    #[test]
    fn letting_go_hangs_still() {
        let (s, level) = sea();
        let (mut p, mut r) = floating(&s, level);
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 1.5);
        let at = p.pos;
        step(&mut p, &mut r, &s, level, &MoveInput::default(), 3.0);
        assert!((p.pos - at).length() < 1.6, "drifted {:?} -> {:?}", at, p.pos);
        assert!(p.dive.under);
    }

    #[test]
    fn out_of_breath_the_body_rises_and_the_breath_comes_back() {
        let (s, level) = sea();
        let (mut p, mut r) = floating(&s, level);
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 4.0);
        assert!(p.dive.under && p.pos.y < level - 4.0);
        // Holding Ctrl the whole minute and more: no drowning, it comes up.
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 75.0);
        assert!(!p.dive.under, "still under after the breath ran out at {:?}", p.pos);
        assert_eq!(p.dive.breath, BREATH);
    }

    #[test]
    fn no_diving_in_the_shallows() {
        let (s, level) = (land(|_| 23.0), 24.0);
        let mut p = Player::new(Vec3::new(40.0, 23.0, 0.0), 0.0);
        let mut r = Ride::default();
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 1.0);
        assert!(!p.dive.under);
    }

    #[test]
    fn a_kick_is_faster_than_cruising_and_mashing_does_not_stack() {
        let (s, level) = sea();
        let (mut p, mut r) = floating(&s, level);
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 1.0);
        p.pitch = 0.0;
        step(&mut p, &mut r, &s, level, &MoveInput { forward: true, ..Default::default() }, 3.0);
        let cruise = p.body.vel.length();
        step(&mut p, &mut r, &s, level, &MoveInput { forward: true, sprint: true, ..Default::default() }, 1.0 / 60.0);
        let kicked = p.body.vel.length();
        assert!(kicked > cruise * 1.8, "kick {kicked} vs cruise {cruise}");
        // Mashing: release and press again at once.
        step(&mut p, &mut r, &s, level, &MoveInput { forward: true, ..Default::default() }, 1.0 / 60.0);
        step(&mut p, &mut r, &s, level, &MoveInput { forward: true, sprint: true, ..Default::default() }, 1.0 / 60.0);
        assert_eq!(p.dive.chain, 1, "a kick 2 frames after the last one counted");
    }

    #[test]
    fn coming_up_fast_leaps_clear() {
        let (s, level) = sea();
        let (mut p, mut r) = floating(&s, level);
        step(&mut p, &mut r, &s, level, &MoveInput { down: true, ..Default::default() }, 2.5);
        p.pitch = 1.4;
        let mut top = f32::MIN;
        for k in 0..240 {
            let sprint = k % 50 == 0;
            step(&mut p, &mut r, &s, level, &MoveInput { forward: true, sprint, ..Default::default() }, 1.0 / 60.0);
            top = top.max(p.pos.y + EYE);
        }
        assert!(top > level + 0.6, "never cleared the water: eye peaked at {top}");
    }
}

// ---- Currents ----------------------------------------------------------------

/// Currents sit one to a cell this size, in a little under half the cells.
const CURRENT_CELL: f32 = 140.0;
/// Water at least this deep to carry one.
pub const CURRENT_DEPTH: f32 = 5.0;

fn chash(x: i32, z: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

fn cbyte(h: u32, k: u32) -> f32 {
    ((h >> (k * 8)) & 0xff) as f32 / 255.0
}

/// The sea's currents at `p` under a surface at `level` (docs/UNDERWATER.md),
/// m/s: like the glide's thermals turned on their side. Scattered by a hash
/// over 140 m cells, each a ribbon of fast water 2.5-7.5 m down, 4-8 m
/// across and 90-160 m long, meandering, 3-5.5 m/s along it and fading out
/// at its ends and edges. Only where `deep` says there is
/// `CURRENT_DEPTH` of water, so none runs over a reef flat or up a beach.
pub fn sea_current(p: Vec3, level: f32, deep: impl Fn(f32, f32) -> bool) -> Vec3 {
    let (cx, cz) = ((p.x / CURRENT_CELL).floor() as i32, (p.z / CURRENT_CELL).floor() as i32);
    let mut v = Vec3::ZERO;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (ix, iz) = (cx + dx, cz + dz);
            let h = chash(ix, iz, 0x51ea_c0de);
            if cbyte(h, 0) > 0.45 {
                continue;
            }
            let h2 = chash(ix, iz, 0x0dd_ba11);
            let c = glam::Vec2::new((ix as f32 + 0.25 + 0.5 * cbyte(h, 1)) * CURRENT_CELL, (iz as f32 + 0.25 + 0.5 * cbyte(h, 2)) * CURRENT_CELL);
            let a = cbyte(h, 3) * std::f32::consts::TAU;
            let dir = glam::Vec2::new(a.cos(), a.sin());
            let half = 45.0 + 35.0 * cbyte(h2, 0);
            let d = glam::Vec2::new(p.x, p.z) - c;
            let along = d.dot(dir);
            if along.abs() > half + 20.0 {
                continue;
            }
            // It meanders: the line swings 6 m either side as it goes.
            let swing = (along * 0.045 + cbyte(h2, 1) * 6.0).sin();
            let across = d.dot(glam::Vec2::new(-dir.y, dir.x)) - swing * 6.0;
            let r = 4.0 + 4.0 * cbyte(h2, 2);
            let y = level - (2.5 + 5.0 * cbyte(h2, 3));
            let dy = p.y - y;
            let w = (-(across * across + dy * dy * 1.6) / (r * r)).exp() * smoothstep01((half + 20.0 - along.abs()) / 30.0);
            if w < 0.02 {
                continue;
            }
            // The way it runs at this point of the meander.
            let slope = (along * 0.045 + cbyte(h2, 1) * 6.0).cos() * 0.045 * 6.0;
            let tangent = (dir + glam::Vec2::new(-dir.y, dir.x) * slope).normalize();
            let speed = 3.0 + 2.5 * cbyte(h, 0) / 0.45;
            v += Vec3::new(tangent.x, 0.0, tangent.y) * speed * w;
        }
    }
    if v != Vec3::ZERO && !deep(p.x, p.z) {
        return Vec3::ZERO;
    }
    v
}

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Is there `CURRENT_DEPTH` of open water under the surface at (x, z)?
pub fn deep_for_current(store: &Store, x: f32, z: f32, level: f32) -> bool {
    let mut y = level - 0.125;
    while y > level - CURRENT_DEPTH {
        if !open(store, Vec3::new(x, y, z)) {
            return false;
        }
        y -= 0.5;
    }
    true
}

#[cfg(test)]
mod current_tests {
    use super::*;

    #[test]
    fn currents_are_scattered_streams_a_few_metres_down() {
        // Sample a 1.4 km square of open sea at 5 m down: some of it runs
        // fast, most of it is still, and nothing runs in the shallows.
        let (mut fast, mut n) = (0, 0);
        for i in 0..140 {
            for k in 0..140 {
                let p = Vec3::new(i as f32 * 10.0, 19.0, k as f32 * 10.0);
                let v = sea_current(p, 24.0, |_, _| true);
                assert!(v.length() < 9.0, "{v:?} at {p:?}");
                fast += (v.length() > 2.0) as u32;
                n += 1;
                assert_eq!(sea_current(p, 24.0, |_, _| false), Vec3::ZERO);
            }
        }
        let share = fast as f32 / n as f32;
        assert!(share > 0.005 && share < 0.12, "{share} of the sea runs fast");
    }

    #[test]
    fn a_current_carries_a_diver_who_lets_go() {
        // Find a stream in deep open water and drop a diver in its middle.
        let mut at = None;
        'find: for i in 0..300 {
            for k in 0..300 {
                let p = Vec3::new(i as f32 * 3.0, 19.0, k as f32 * 3.0);
                if sea_current(p, 24.0, |_, _| true).length() > 3.0 {
                    at = Some(p);
                    break 'find;
                }
            }
        }
        let p = at.expect("no current anywhere");
        let v = sea_current(p, 24.0, |_, _| true);
        assert!(v.length() > 3.0 && v.y == 0.0);
    }
}
