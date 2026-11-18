//! The studio's camera: a turntable around whatever is being edited.
//!
//! The EDIT page used to be a menu screen, and a menu screen cannot move the
//! camera -- the page aimed the view at the subject once, when it opened, and
//! that was the only look you got. Here the camera belongs to the studio: it
//! orbits a target, dollies in and out, pans, and frames the subject from its
//! height. The player's body is left exactly where it was standing and put
//! back when the page closes (`Game::editing` freezes it), so this moves a
//! camera and nothing else.
//!
//! The subject is framed left of centre rather than in the middle, because
//! the entry's list takes the right 44% of the screen (`form::BAND`)
//! and an animal drawn under it cannot be read. The whole rig slides sideways
//! rather than turning, so the target shifts across the screen without the
//! view rolling about it.

use glam::Vec3;

/// How close and how far the camera may get, metres.
const NEAR: f32 = 0.5;
const FAR: f32 = 400.0;

/// Where the subject sits across the screen, as a share of half the width:
/// 0 is the middle, 1 the left edge. The list starts at 0.56 of the width, so
/// this puts the subject in the middle of what is left with a margin either
/// side.
const SIDE: f32 = 0.42;

/// Radians a pixel of drag turns the camera.
const DRAG: f32 = 0.0062;
/// Radians a keypress turns it.
pub const KEY_TURN: f32 = 0.09;
/// How much one wheel notch or keypress changes the distance.
const ZOOM: f32 = 1.12;
/// How fast the turntable turns, radians a second.
const SPIN: f32 = 0.42;

/// Looking a little down on the subject, which is how you would stand to it.
const PITCH0: f32 = -0.16;
/// The camera never goes over the top or under the floor.
const PITCH_MAX: f32 = 1.45;

pub struct Cam {
    /// What it looks at: the middle of the subject.
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    /// The turntable turns while this is on.
    pub spinning: bool,
    /// What the distance was last framed for, metres, so a subject that
    /// changes size is framed again and one that does not is left alone.
    framed: f32,
}

impl Cam {
    /// A camera looking at `at` from where the player was standing: the view
    /// the page used to open with, which is the one to start from.
    pub fn new(at: Vec3, from: Vec3, tall: f32) -> Cam {
        let d = at - from;
        let yaw = if d.x.hypot(d.z) > 0.01 { (-d.x).atan2(-d.z) } else { 0.0 };
        let mut c = Cam {
            target: at + Vec3::Y * tall * 0.5,
            yaw,
            pitch: PITCH0,
            dist: 6.0,
            spinning: false,
            framed: 0.0,
        };
        c.frame(at, tall, tall, 16.0 / 9.0);
        c
    }

    /// The way the camera is pointing.
    pub fn dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp)
    }

    /// Screen right, level with the horizon.
    fn right(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(cy, 0.0, -sy)
    }

    /// Where the eye sits: back along the view by the distance, then slid
    /// sideways so the target lands left of centre rather than behind the
    /// list.
    pub fn eye(&self, fov_y: f32, aspect: f32) -> Vec3 {
        let side = SIDE * self.dist * (fov_y * 0.5).tan() * aspect;
        self.target - self.dir() * self.dist + self.right() * side
    }

    /// Frame a subject `tall` metres high standing at `at`: far enough back
    /// that it fits in the part of the screen the list leaves, and aimed at
    /// its middle.
    /// `tall` is what it stands; `span` how far it reaches across, which is
    /// what decides the distance (a crow is a hand high and a metre wide).
    pub fn frame(&mut self, at: Vec3, tall: f32, span: f32, aspect: f32) {
        let tall = tall.max(0.2);
        let span = span.max(tall);
        self.target = at + Vec3::Y * tall * 0.5;
        // Twice what it measures, which is near enough to fill the part of
        // the screen the list leaves without the wide-angle blow-up a closer
        // view gives anything the size of its own distance. Tall and thin
        // (a beech) is held back by its height instead.
        let _ = aspect;
        self.dist = (span * 2.0).max(tall * 1.9).clamp(NEAR, FAR);
        // Something small on the ground is looked down on, as you would
        // stand over it; something person-sized is looked at level.
        self.pitch = -(0.16 + 0.42 * (1.0 - (tall / 1.6).min(1.0)));
        self.framed = span;
    }

    /// Frame it again only if it has changed size: stepping a behaviour knob
    /// should not throw away the view you had set up.
    pub fn refit(&mut self, at: Vec3, tall: f32, span: f32, aspect: f32) {
        let same = (span - self.framed).abs() < self.framed.max(0.2) * 0.08;
        if same {
            // The subject may still have moved (another entry stands in the
            // same spot), so the target follows it without the distance
            // changing.
            self.target = at + Vec3::Y * tall.max(0.2) * 0.5;
            return;
        }
        self.frame(at, tall, span, aspect);
    }

    /// Drag: turn around the target.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.turn(dx * DRAG, dy * DRAG);
    }

    pub fn turn(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch - dpitch).clamp(-PITCH_MAX, PITCH_MAX);
    }

    /// Wheel or keypress: in and out. Positive is closer.
    pub fn dolly(&mut self, notches: f32) {
        self.dist = (self.dist * ZOOM.powf(-notches)).clamp(NEAR, FAR);
    }

    /// Drag with the other button: slide the target across the view, so a
    /// head or a hoof can be put in the middle.
    pub fn pan(&mut self, dx: f32, dy: f32, fov_y: f32, h: f32) {
        // A pixel of drag moves the target by as much as a pixel covers at
        // the target's distance, so what is under the cursor stays there.
        let per_px = 2.0 * self.dist * (fov_y * 0.5).tan() / h.max(1.0);
        let up = self.right().cross(self.dir()).normalize_or(Vec3::Y);
        self.target += (self.right() * -dx + up * -dy) * per_px;
    }

    /// Advance the turntable.
    pub fn spin(&mut self, dt: f32) {
        if self.spinning {
            self.yaw += SPIN * dt;
        }
    }

    /// Put the camera where it says, leaving the player's body alone.
    pub fn apply(&self, game: &mut crate::game::Game, aspect: f32) {
        let fov = crate::game::FOV_Y;
        game.look_from(self.eye(fov, aspect), self.yaw, self.pitch);
    }
}
