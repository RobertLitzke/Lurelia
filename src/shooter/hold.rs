//! Where a gun sits in the player's hands.
//!
//! Everything about holding a weapon in the camera's frame: the walk bob,
//! the swing after a turn, the recoil settling, the drop to the hip at a
//! sprint, the muzzle flash, and the frame that turns a point on a gun's
//! voxel grid into a point in the world.
//!
//! The rules (`super`) own a `Hold` and ask this file where the gun is. It
//! knows nothing about health, bots or score, and nothing about a `Shooter`.

use crate::combat::viewmodel;
use crate::models::{Instance, Model};
use crate::player::Player;
use glam::{Quat, Vec2, Vec3};


/// The nearest a shouldered gun's butt comes to the eye, metres ahead.
const BUTT_CLEAR: f32 = 0.45;
/// How far down the frame a gun's bore sits, as a share of the way from the
/// middle to the bottom edge.
const BORE_LOW: f32 = 0.95;

/// Where a shape's grip is held, from the eye: right, up, ahead, metres.
///
/// Every gun is placed by its bore rather than its grip: the line it shoots
/// along sits low in the frame (`BORE_LOW` of the way from the middle to the
/// bottom edge at the grip's distance) and to the right, so the grip and the
/// hand are below the bottom of the screen and what shows is the top of the
/// gun reaching in from the corner. A long gun's grip sits further out, its
/// butt `BUTT_CLEAR` ahead of the eye, so the stock never fills the view; a
/// pistol is closer in and nearer the middle, a rocket tube higher on the
/// shoulder. A knife or a shovel is low on the right.
fn hold_at(shape: usize) -> Vec3 {
    use crate::combat::real::Hands;
    // The bottom edge of the view at one metre out.
    let edge = (crate::game::FOV_Y * 0.5).tan();
    let bore = (viewmodel::muzzle(shape).y) * viewmodel::VOXEL;
    let at = |right: f32, low: f32, z: f32| Vec3::new(right * z, -low * edge * z - bore, z);
    match viewmodel::hands(shape) {
        Hands::Pistol => at(0.3, 0.72, 0.5),
        Hands::One | Hands::Haft => Vec3::new(0.34, -0.40, 0.58),
        Hands::Tube => at(0.6, 0.95, (BUTT_CLEAR + viewmodel::butt(shape).z).max(1.0)),
        Hands::Shoulder => at(0.55, BORE_LOW, (BUTT_CLEAR + viewmodel::butt(shape).z).max(0.7)),
    }
}
/// How far the muzzle is turned in toward the middle of the screen, radians:
/// a gun held parallel to the view points off to the side of everything the
/// crosshair is on.
const TOE_IN: f32 = 0.085;
/// How far the gun is canted over in the hand, radians.
const CANT: f32 = -0.07;
/// How far the gun drops and turns away when sprinting.
const LOWER: Vec3 = Vec3::new(0.06, -0.16, -0.06);
/// Seconds the muzzle flash is drawn for.
const FLASH_S: f32 = 0.055;

/// Everything about where the player's gun is held this frame: how far
/// through its walk cycle it is, how far it is still swinging after a turn,
/// how far the last shot drove it back, how far the barrels have turned, and
/// how long the muzzle flash has left.
#[derive(Default)]
pub struct Hold {
    walk: f32,
    bob: Vec3,
    sway: Vec2,
    last_look: Vec2,
    kick: f32,
    kick_v: f32,
    /// Radians the minigun's barrels have turned.
    pub spin: f32,
    /// Seconds of muzzle flash left.
    pub flash: f32,
    /// 0 held up, 1 dropped to the hip while sprinting.
    lowered: f32,
}

/// Shortest way round from `a` to `b`, radians.
fn turn(a: f32, b: f32) -> f32 {
    crate::util::wrap_angle(b - a)
}

impl Hold {
    /// One step of the walk cycle, the swing after a turn, the kick settling
    /// and the barrels turning. `spin` is how far the barrels are wound up.
    pub fn step(&mut self, player: &Player, sprinting: bool, spin: f32, dt: f32) {
        let speed = Vec2::new(player.body.vel.x, player.body.vel.z).length();
        let amp = (speed / 5.0).min(1.3) * if player.body.on_ground { 1.0 } else { 0.25 };
        self.walk += dt * (3.0 + speed * 1.5);
        self.bob = Vec3::new(self.walk.sin() * 0.022, -(self.walk * 2.0).sin().abs() * 0.015, 0.0) * amp;
        // Sway: the gun lags the view, then swings back.
        let look = Vec2::new(player.yaw, player.pitch);
        let d = Vec2::new(turn(self.last_look.x, look.x), look.y - self.last_look.y);
        self.last_look = look;
        let rate = d / dt.max(1e-4);
        let want = Vec2::new((rate.x * 0.016).clamp(-0.10, 0.10), (rate.y * -0.014).clamp(-0.09, 0.09));
        self.sway = self.sway.lerp(want, (dt * 7.0).min(1.0));
        // Recoil settles with a spring, so a burst stacks and then recovers.
        let (k, v) = crate::player::smooth_damp(self.kick, self.kick_v, 0.14, dt);
        self.kick = k;
        self.kick_v = v;
        self.flash = (self.flash - dt).max(0.0);
        self.spin += dt * spin * 26.0;
        let want = if sprinting && speed > 6.0 { 1.0 } else { 0.0 };
        self.lowered += (want - self.lowered).clamp(-6.0 * dt, 6.0 * dt);
    }

    /// Driven back by a shot.
    pub fn fired(&mut self, kick_deg: f32, shots: u32) {
        self.kick = (self.kick + kick_deg * 0.013 * shots as f32).min(0.14);
        self.flash = FLASH_S;
    }
}

/// One instance of `model`, placed at `at` with its own pivot there.
/// The same, drawn `k` times its own size.
///
/// The shader takes a model voxel as `(cell - pivot) * voxel` and the pivot
/// is in voxels, so multiplying the voxel edge alone scales the whole thing
/// about the point it is placed at. That is cheaper than three models of
/// three sizes and it cannot drift out of step with them.
#[allow(clippy::too_many_arguments)] // a placement is a point, a turn, a colour and now a size
pub fn sized(model: usize, models: &[Model], at: Vec3, yaw: f32, rot: Quat, tint: [f32; 3], k: f32) -> Option<(usize, Instance)> {
    let (m, mut inst) = placed(model, models, at, yaw, rot, tint)?;
    inst.info[0] *= k;
    Some((m, inst))
}

pub fn placed(model: usize, models: &[Model], at: Vec3, yaw: f32, rot: Quat, tint: [f32; 3]) -> Option<(usize, Instance)> {
    let md = models.get(model)?;
    Some((
        model,
        Instance {
            pos_yaw: [at.x, at.y, at.z, yaw],
            info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
            tint: [tint[0], tint[1], tint[2], 0.0],
            rot: [rot.x, rot.y, rot.z, rot.w],
            shift: [0.0; 4],
            flesh: [[0.0; 4]; 3],
            glow: [0.0; 4],
            fade: [0.0; 4],
        },
    ))
}

/// Where a held model's grid sits in the world this frame. The shader turns
/// a model by its yaw about world Y after the instance's own rotation, so
/// model +x is the level right, +y is up and +z is level backward; the pitch
/// rides in `rot`.
pub struct Grip {
    pub pos: Vec3,
    yaw: f32,
    rot: Quat,
    ax: Vec3,
    az: Vec3,
}

impl Grip {
    /// A point of the model's 32-cube, in world metres.
    pub fn at(&self, v: Vec3) -> Vec3 {
        let r = self.rot * ((v - Vec3::new(16.0, 0.0, 16.0)) * viewmodel::VOXEL);
        self.pos + self.ax * r.x + Vec3::Y * r.y + self.az * r.z
    }

    /// An instance of `model`, placed as this grip holds it, with `extra`
    /// turned in before the pitch (the barrels' spin).
    pub fn instance(&self, model: usize, models: &[Model], pivot_at: Vec3, extra: Quat) -> Option<(usize, Instance)> {
        placed(model, models, pivot_at, self.yaw, self.rot * extra, [1.0; 3])
    }
}

/// Where the player's gun is held this frame.
pub fn grip(h: &Hold, player: &Player, alpha: f32, shape: usize) -> Grip {
    let (yaw, pitch) = (player.yaw, player.pitch);
    let fwd = player.forward();
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let up = right.cross(fwd).normalize_or(Vec3::Y);
    let o = hold_at(shape) + h.bob + Vec3::new(h.sway.x, h.sway.y, -h.kick) + LOWER * h.lowered;
    let pos = player.eye(alpha) + right * o.x + up * o.y + fwd * o.z;
    // The kick lifts the muzzle and the sprint drops it. A blade or a shovel
    // has no recoil: the same spring is its swing, down and across to the
    // left and back.
    let swung = matches!(viewmodel::hands(shape), crate::combat::real::Hands::One | crate::combat::real::Hands::Haft);
    let swing = if swung { (h.kick * 50.0).min(1.0) } else { 0.0 };
    // Between swings the blade rests raised a little, ready to come down.
    let tilt = if swung { 0.3 - swing * 1.4 } else { h.kick * 2.6 } - h.lowered * 0.5;
    // The model's own yaw is the view's, turned in a little; `ax` and `az`
    // must be that same yaw, or the muzzle this works out would not be the
    // muzzle that is drawn.
    let gy = yaw + TOE_IN - h.lowered * 0.25 + swing * 0.6;
    Grip {
        pos,
        yaw: gy,
        rot: Quat::from_rotation_x(pitch + tilt) * Quat::from_rotation_z(CANT),
        ax: Vec3::new(gy.cos(), 0.0, -gy.sin()),
        az: Vec3::new(gy.sin(), 0.0, gy.cos()),
    }
}

/// Where the drawn gun's muzzle is, in the world. The shot has to leave from
/// here and not from the eye: a tracer down the line of sight is end-on and
/// invisible, and one that starts anywhere else looks wrong.
pub fn muzzle(h: &Hold, player: &Player, alpha: f32, shape: usize) -> Vec3 {
    grip(h, player, alpha, shape).at(viewmodel::muzzle(shape))
}
