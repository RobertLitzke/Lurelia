//! The camera rig: what every mode hands the renderer, and the chase camera
//! any of them can have.
//!
//! One `Camera` is what the renderer is given each frame, whoever built it --
//! a ride, the free camera, screenshot mode, a shooter's rules. `Game::camera`
//! picks which one this frame wants. A first-person view is
//! `Camera::looking`; a third-person one is a `Chase`, which keeps the follow
//! point that trails the body between frames and turns it into a camera when
//! the frame is drawn.
//!
//! This lives apart from the modes so that a chase camera is not a property of
//! riding a bike: anything with a body and a look direction can have one.
//! `src/camera/` is the photographic camera (film, tasks, the scrapbook),
//! which is a different thing with an unfortunately similar name.

use crate::player::smooth_damp;
use crate::world::store::Store;
use glam::Vec3;

/// What the camera does this frame.
pub struct Camera {
    pub eye: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    pub fov_scale: f32,
    pub speed_fx: f32,
    /// Speed streaks at the rim of the view, 0 to 1 (post.wgsl).
    pub streaks: f32,
}

impl Camera {
    /// A plain first-person view: at `eye`, looking where `yaw` and `pitch`
    /// say, level, with no speed about it. What the free camera and the
    /// screenshot camera both want.
    pub fn looking(eye: Vec3, yaw: f32, pitch: f32) -> Camera {
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        Camera {
            eye,
            fwd: Vec3::new(-sy * cp, sp, -cy * cp),
            up: Vec3::Y,
            fov_scale: 1.0,
            speed_fx: 0.0,
            streaks: 0.0,
        }
    }

    /// The same, rolled about the view direction (screenshot mode tilts).
    pub fn rolled(mut self, roll: f32) -> Camera {
        // Level right, from the view direction: the same vector as
        // (cos yaw, 0, -sin yaw), which is what `fwd` is built from.
        let right = Vec3::new(-self.fwd.z, 0.0, self.fwd.x).normalize_or(Vec3::X);
        let up_level = right.cross(self.fwd).normalize_or(Vec3::Y);
        self.up = (up_level * roll.cos() - right * roll.sin()).normalize_or(Vec3::Y);
        self
    }
}

/// How a chase camera sits behind its subject.
#[derive(Clone, Copy)]
pub struct ChaseSpec {
    /// How far above the body's feet the follow point sits, metres.
    pub height: f32,
    /// Seconds for the follow point to catch up across, and up and down.
    /// Longer is lazier: the camera lags the body and the body is seen to
    /// move within the frame.
    pub across: f32,
    pub upwards: f32,
    /// How far back the camera sits from the follow point, metres.
    pub dist: f32,
}

/// How far along the subject's look ray a chase camera aims, metres. Far
/// enough that the parallax between the camera and the body is small, near
/// enough that the aim still points at what is in front of them.
const AIM_AT: f32 = 80.0;
/// How far the follow point may trail the body before it is held, metres.
const MAX_TRAIL: f32 = 2.5;
/// How far either side of the line to the camera it looks round something
/// in the way, metres: anything narrower than twice this is let through.
const THIN: f32 = 0.6;

/// A chase camera's own state: the follow point that trails the body, which
/// has to be kept between frames because it is smoothed.
#[derive(Default, Clone, Copy)]
pub struct Chase {
    pt: Vec3,
    prev_pt: Vec3,
    vel: Vec3,
    /// What the follow point was last kept for. When the caller's answer
    /// changes -- another ride, another subject -- the point is put back on
    /// the body instead of flying across the world to it.
    of: Option<u8>,
}

impl Chase {
    /// Remember where the follow point was, for interpolating within the
    /// frame. Call once per fixed step, before `follow`.
    pub fn begin_step(&mut self) {
        self.prev_pt = self.pt;
    }

    /// Move the follow point toward the body at `pos` for one step.
    pub fn follow(&mut self, pos: Vec3, spec: ChaseSpec, of: u8, dt: f32) {
        let point = pos + Vec3::Y * spec.height;
        if self.of != Some(of) {
            self.of = Some(of);
            self.pt = point;
            self.prev_pt = point;
            self.vel = Vec3::ZERO;
        }
        let d = self.pt - point;
        let (x, vx) = smooth_damp(d.x, self.vel.x, spec.across, dt);
        let (y, vy) = smooth_damp(d.y, self.vel.y, spec.upwards, dt);
        let (z, vz) = smooth_damp(d.z, self.vel.z, spec.across, dt);
        let off = Vec3::new(x, y, z);
        let off = if off.length() > MAX_TRAIL { off * (MAX_TRAIL / off.length()) } else { off };
        self.pt = point + off;
        self.vel = Vec3::new(vx, vy, vz);
    }

    /// The camera this frame: pulled back along `fwd` from the follow point,
    /// stopped short of whatever it would otherwise sit inside, and aimed so
    /// the screen's centre is where the subject is looking.
    ///
    /// `dist` is how far back to sit, `up` the camera's up vector (rolled by
    /// the caller, which knows how far the body is leaning).
    pub fn camera(&self, fwd: Vec3, up: Vec3, dist: f32, store: &Store, alpha: f32, fov_scale: f32, speed_fx: f32, streaks: f32) -> Camera {
        let target = self.prev_pt.lerp(self.pt, alpha);
        let back = (-fwd + Vec3::Y * 0.25).normalize_or(-fwd);
        // Keep the camera out of the ground and the trees.
        let reach_from = |from: Vec3| match store.raycast(from, back, dist) {
            Some((h, _)) => {
                let c = Vec3::new(h[0] as f32 + 0.5, h[1] as f32 + 0.5, h[2] as f32 + 0.5) / 4.0;
                ((c - from).dot(back) - 0.35).clamp(0.4, dist)
            }
            None => dist,
        };
        let mut reach = reach_from(target);
        if reach < dist {
            // Something thin across the line (a lamp post, a sign's pole, a
            // trunk) that rays either side pass: the camera goes on through
            // it rather than jumping in to the subject's back for the few
            // frames it takes to go by.
            let side = back.cross(Vec3::Y).normalize_or(Vec3::X) * THIN;
            let round = reach_from(target + side).min(reach_from(target - side));
            if round > reach + 1.0 {
                reach = round;
            }
        }
        let eye = target + back * reach;
        // The subject aims from its own body, not from the camera: a camera
        // that merely looked along `fwd` put the screen's centre a metre or
        // two beside what they were pointing at, and the further it was
        // pulled back the worse the miss.
        let fwd = (target + fwd * AIM_AT - eye).normalize_or(fwd);
        Camera {
            eye,
            fwd,
            up,
            fov_scale,
            speed_fx,
            streaks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Store {
        Store::default()
    }

    const SPEC: ChaseSpec = ChaseSpec {
        height: 1.5,
        across: 0.05,
        upwards: 0.08,
        dist: 3.6,
    };

    /// `looking` and a hand-built first-person camera agree.
    #[test]
    fn looking_faces_where_it_is_told() {
        let c = Camera::looking(Vec3::new(1.0, 2.0, 3.0), 0.0, 0.0);
        assert!((c.fwd - Vec3::NEG_Z).length() < 1e-6, "yaw 0 looks north, got {:?}", c.fwd);
        let c = Camera::looking(Vec3::ZERO, -std::f32::consts::FRAC_PI_2, 0.0);
        assert!((c.fwd - Vec3::X).length() < 1e-6, "yaw -90 faces +X, got {:?}", c.fwd);
        let c = Camera::looking(Vec3::ZERO, 0.0, std::f32::consts::FRAC_PI_2);
        assert!(c.fwd.y > 0.99, "pitched up, got {:?}", c.fwd);
    }

    /// The follow point starts on the body and trails it when it moves, but
    /// never by more than `MAX_TRAIL`.
    #[test]
    fn the_follow_point_trails_the_body_and_is_held() {
        let mut ch = Chase::default();
        let mut pos = Vec3::ZERO;
        ch.follow(pos, SPEC, 0, 1.0 / 60.0);
        assert!((ch.pt - Vec3::Y * SPEC.height).length() < 1e-6, "it should start on the body");
        // Thrown across the world at speed: it lags, but within the cap.
        for _ in 0..120 {
            pos += Vec3::X * 0.5;
            ch.begin_step();
            ch.follow(pos, SPEC, 0, 1.0 / 60.0);
            let trail = (ch.pt - (pos + Vec3::Y * SPEC.height)).length();
            assert!(trail <= MAX_TRAIL + 1e-4, "trailed {trail} m");
        }
        assert!((ch.pt.x - pos.x).abs() > 0.05, "it should lag a body moving this fast");
    }

    /// A different subject puts the point back on the body rather than
    /// sweeping the camera across the world to it.
    #[test]
    fn changing_subject_does_not_sweep() {
        let mut ch = Chase::default();
        ch.follow(Vec3::ZERO, SPEC, 0, 1.0 / 60.0);
        let far = Vec3::new(400.0, 0.0, 0.0);
        ch.follow(far, SPEC, 1, 1.0 / 60.0);
        assert!((ch.pt - (far + Vec3::Y * SPEC.height)).length() < 1e-6);
    }

    /// With nothing in the way the camera sits `dist` back along the view,
    /// and its forward still points at what the body is looking at.
    #[test]
    fn the_camera_sits_back_and_keeps_the_aim() {
        let store = empty();
        let mut ch = Chase::default();
        ch.follow(Vec3::ZERO, SPEC, 0, 1.0 / 60.0);
        ch.begin_step();
        let fwd = Vec3::NEG_Z;
        let c = ch.camera(fwd, Vec3::Y, SPEC.dist, &store, 1.0, 1.0, 0.0, 0.0);
        let target = Vec3::Y * SPEC.height;
        assert!((c.eye - target).length() > SPEC.dist - 0.01, "it should be {} m back", SPEC.dist);
        assert!(c.eye.z > target.z, "it should be behind a body looking north");
        // The screen's centre lands on the look ray, not beside it.
        let want = target + fwd * AIM_AT;
        let along = (want - c.eye).dot(c.fwd);
        let miss = (want - (c.eye + c.fwd * along)).length();
        assert!(miss < 1e-3, "the aim missed by {miss} m");
    }

    /// A post between the subject and the camera is looked past; a wall
    /// across the whole view still brings the camera in.
    #[test]
    fn a_post_is_looked_past_but_a_wall_is_not() {
        use crate::world::store::Voxels;
        use crate::world::material::{AIR, STONE};
        let scene = |wall: bool| {
            let mut s = Store::default();
            for cx in 0..2 {
                for cz in 0..2 {
                    s.chunks.insert((cx, 0, cz), Voxels::Uniform(AIR));
                }
            }
            // At z 10 m, 2.4 m behind a body at z 8 looking north.
            let xs = if wall { 0..64 } else { 31..33 };
            for x in xs {
                for y in 0..24 {
                    s.set(x, y, 40, STONE);
                }
            }
            s
        };
        let mut ch = Chase::default();
        ch.follow(Vec3::new(8.0, 0.0, 8.0), SPEC, 0, 1.0 / 60.0);
        ch.begin_step();
        let target = Vec3::new(8.0, SPEC.height, 8.0);
        let post = ch.camera(Vec3::NEG_Z, Vec3::Y, SPEC.dist, &scene(false), 1.0, 1.0, 0.0, 0.0);
        assert!((post.eye - target).length() > SPEC.dist - 0.01, "pulled in for a post: {:?}", post.eye);
        let wall = ch.camera(Vec3::NEG_Z, Vec3::Y, SPEC.dist, &scene(true), 1.0, 1.0, 0.0, 0.0);
        assert!((wall.eye - target).length() < 2.5, "not pulled in for a wall: {:?}", wall.eye);
    }
}
