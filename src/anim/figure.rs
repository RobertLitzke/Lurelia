//! A biped driven by a body rather than by an activity table.
//!
//! `life.rs` animates an animal that decides for itself what to do: it picks
//! an activity, and the pose follows from it. A fighter is the other way
//! round. Something else -- a player's keys or a brain filling in the same
//! `Pilot` -- has already decided where the body is going, and this file's
//! only job is to show that honestly: which way it is moving relative to the
//! way it is facing, whether its feet are on the ground, how far it is
//! crouched, where it is looking, whether it has a weapon up, and whether it
//! is still alive.
//!
//! So the state here is not "walking" or "idle". It is a handful of
//! continuous quantities, each smoothed toward what the body is actually
//! doing, which is what lets a bot strafe out of a sprint into a jump and
//! read as one movement rather than as three clips.
//!
//! What it draws on: `motors.rs` for the stride, the postures and the look,
//! `ragdoll.rs` for the death. Everything a shooter needs beyond that -- the
//! weapon hold, the strafe, the jump and landing, the recoil, the turn -- is
//! here, because none of it means anything to an animal.

use crate::anim::{Plan, Role};
use super::motors::{self, Gait};
use super::{ragdoll::Ragdoll, Pose, Rig};
use glam::{Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Below this, the body counts as standing still.
const STILL: f32 = 0.35;
/// Seconds a landing takes to absorb.
const LAND_S: f32 = 0.32;
/// Seconds the push of a jump shows for.
const JUMP_S: f32 = 0.25;
/// The most of a dead body's own acceleration that reaches its limbs,
/// metres a second squared. The blast that throws a body changes its
/// velocity in one step, which is an acceleration of hundreds: fed in whole
/// it put every joint at its stop in that one step and the body stayed
/// arched and splayed for the rest of its flight. A jolt, not a crushing.
pub const SHAKE_MOST: f32 = 30.0;

/// What the body is doing this step. Everything the figure knows.
#[derive(Clone, Copy, Debug)]
pub struct Drive {
    pub dt: f32,
    /// World velocity, metres a second.
    pub vel: Vec3,
    /// Which way the body faces, radians.
    pub yaw: f32,
    /// Where it is looking, radians; positive is up.
    pub pitch: f32,
    pub on_ground: bool,
    /// 0 standing, 1 fully crouched.
    pub crouch: f32,
    pub sprint: bool,
    /// A weapon is out and up. A sprint drops it on its own.
    pub holding: bool,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            dt: 1.0 / 30.0,
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
            crouch: 0.0,
            sprint: false,
            holding: true,
        }
    }
}

/// A posed biped, and everything that is still settling.
pub struct Figure {
    pub pose: Pose,
    pub prev: Pose,
    /// Where it is in the stride, in cycles.
    phase: f32,
    gait: Gait,
    from: Gait,
    /// How far the change of gait has gone, 0..1.
    mix: f32,
    /// Smoothed speed along the body's own axes: x right, y forward.
    mv: Vec2,
    /// 0 standing still, 1 striding.
    stride_w: f32,
    /// Seconds off the ground.
    air: f32,
    /// What is left of a landing and of a jump's push, 1 down to 0.
    land: f32,
    jump: f32,
    was_ground: bool,
    crouch: f32,
    /// 0 weapon down, 1 weapon up.
    hold: f32,
    /// Smoothed aim: where it is looking, and how fast it is turning.
    pitch: f32,
    turn: f32,
    last_yaw: f32,
    /// Recoil, as a spring.
    kick: f32,
    kick_v: f32,
    t: f32,
    seed: f32,
    /// Dead: nothing is holding it up any more.
    ragdoll: Option<Box<Ragdoll>>,
    /// The velocity the last step was driven with, so a corpse's limbs can
    /// be shaken by what the body itself is doing: the lurch of the blast
    /// that threw it and the stop when it meets a wall.
    last_vel: Vec3,
}

impl Figure {
    pub fn new(rig: &Rig, seed: u32) -> Self {
        let n = rig.bones.len();
        let s = (seed & 0xffff) as f32 / 65536.0;
        Self {
            pose: Pose::rest(n),
            prev: Pose::rest(n),
            phase: s,
            gait: Gait::Walk,
            from: Gait::Walk,
            mix: 1.0,
            mv: Vec2::ZERO,
            stride_w: 0.0,
            air: 0.0,
            land: 0.0,
            jump: 0.0,
            was_ground: true,
            crouch: 0.0,
            hold: 1.0,
            pitch: 0.0,
            turn: 0.0,
            last_yaw: 0.0,
            kick: 0.0,
            kick_v: 0.0,
            t: s * 40.0,
            seed: s * 10.0,
            ragdoll: None,
            last_vel: Vec3::ZERO,
        }
    }

    pub fn dead(&self) -> bool {
        self.ragdoll.is_some()
    }

    /// Let go: from this moment the body falls where it stands.
    pub fn die(&mut self, rig: &Rig, voxel: f32, seed: u32) {
        if self.ragdoll.is_none() {
            self.ragdoll = Some(Box::new(Ragdoll::new(rig, voxel, seed)));
        }
    }

    /// Thrown by whatever killed it: `speed` is how fast the body is leaving
    /// the ground, metres a second. A shot that only knocks a man down
    /// leaves him where he stood; a rocket sends him over the railing.
    pub fn hurl(&mut self, rig: &Rig, speed: f32, seed: u32) {
        if let Some(r) = self.ragdoll.as_mut() {
            r.hurl(rig, speed, seed);
        }
    }

    /// Back on its feet, whole.
    pub fn revive(&mut self) {
        self.ragdoll = None;
        self.last_vel = Vec3::ZERO;
        self.land = 0.0;
        self.jump = 0.0;
        self.kick = 0.0;
        self.kick_v = 0.0;
    }

    /// Driven back by a shot: `deg` is the weapon's kick.
    pub fn fired(&mut self, deg: f32) {
        self.kick = (self.kick + deg * 0.02).min(0.45);
    }

    /// The pose to draw between two fixed steps.
    pub fn at(&self, alpha: f32) -> Pose {
        self.prev.lerp(&self.pose, alpha)
    }

    /// One fixed step.
    pub fn step(&mut self, rig: &Rig, voxel: f32, d: &Drive) {
        let dt = d.dt.max(1e-4);
        self.t += dt;
        let leg_m = (motors::hip(rig) * voxel).max(0.05);
        if let Some(r) = self.ragdoll.as_mut() {
            // What the body is doing to the limbs hanging off it, in the
            // body's own frame: its acceleration. Flying through the air
            // that is nothing much; hitting a wall at ten metres a second
            // is what throws the arms forward.
            let acc = (d.vel - self.last_vel) / dt;
            self.last_vel = d.vel;
            let (sy, cy) = d.yaw.sin_cos();
            let shake = Vec3::new(acc.x * cy - acc.z * sy, acc.y, -acc.x * sy - acc.z * cy).clamp_length_max(SHAKE_MOST);
            r.aloft(!d.on_ground);
            r.step(rig, dt, shake);
            self.prev = std::mem::replace(&mut self.pose, r.pose.clone());
            if self.prev.bones.len() != self.pose.bones.len() {
                self.prev = self.pose.clone();
            }
            return;
        }
        let k = |rate: f32| (dt * rate).min(1.0);

        // Velocity in the body's own frame: +y forward, +x to its right.
        let (sy, cy) = d.yaw.sin_cos();
        let want = Vec2::new(d.vel.x * cy - d.vel.z * sy, -d.vel.x * sy - d.vel.z * cy);
        self.mv += (want - self.mv) * k(10.0);
        let speed = self.mv.length();
        self.stride_w += ((speed / STILL).clamp(0.0, 1.0) - self.stride_w) * k(8.0);

        // Gait, and the stride itself. A body that is mostly sliding sideways
        // still takes steps, so the cycle is advanced by the whole speed and
        // only shared out between the fore-and-aft swing and the side step.
        let g = motors::gait_for(Some(Plan::Biped), speed, leg_m);
        if self.stride_w < 0.05 {
            self.gait = g;
            self.mix = 1.0;
        }
        self.phase += speed * dt / (self.gait.cycle_for(Some(Plan::Biped)) * leg_m).max(0.05);
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            if g != self.gait {
                self.from = self.gait;
                self.gait = g;
                self.mix = 0.0;
            }
        }
        self.mix = (self.mix + dt / 0.25).min(1.0);

        // Off the ground, and what the landing costs.
        if d.on_ground {
            if !self.was_ground {
                self.land = (self.air / 0.5).clamp(0.25, 1.0);
            }
            self.air = 0.0;
        } else {
            if self.was_ground {
                self.jump = 1.0;
            }
            self.air += dt;
        }
        self.was_ground = d.on_ground;
        self.land = (self.land - dt / LAND_S).max(0.0);
        self.jump = (self.jump - dt / JUMP_S).max(0.0);

        self.crouch += (d.crouch.clamp(0.0, 1.0) - self.crouch) * k(9.0);
        // The weapon comes down at a sprint and while falling, and back up
        // sooner than it went down: being slow to raise a gun is a death.
        let up = if d.holding && !(d.sprint && speed > 4.5) { 1.0 } else { 0.0 };
        self.hold += (up - self.hold) * k(if up > 0.0 { 9.0 } else { 5.0 });
        self.pitch += (d.pitch.clamp(-1.4, 1.4) - self.pitch) * k(12.0);
        let rate = crate::util::wrap_angle(d.yaw - self.last_yaw) / dt;
        self.last_yaw = d.yaw;
        self.turn += (rate.clamp(-4.0, 4.0) - self.turn) * k(6.0);
        let (kk, kv) = crate::player::smooth_damp(self.kick, self.kick_v, 0.12, dt);
        self.kick = kk;
        self.kick_v = kv;

        self.prev = std::mem::take(&mut self.pose);
        self.pose = self.compose(rig, leg_m, d);
        if self.prev.bones.len() != self.pose.bones.len() {
            self.prev = self.pose.clone();
        }
    }

    fn compose(&self, rig: &Rig, leg_m: f32, d: &Drive) -> Pose {
        let mut p = motors::posture(rig, 0.0, false, leg_m);
        // How the movement is shared between walking and side-stepping. The
        // fore-and-aft cycle runs backwards when the body is backing up.
        let speed = self.mv.length();
        let (fwd, side) = if speed > 1e-3 { (self.mv.y / speed, self.mv.x / speed) } else { (1.0, 0.0) };
        let w = self.stride_w * (1.0 - self.crouch * 0.45);
        let phase = if fwd < 0.0 { -self.phase } else { self.phase };
        motors::gait(rig, &mut p, self.gait, phase, w * fwd.abs() * self.mix, leg_m);
        motors::gait(rig, &mut p, self.from, phase, w * fwd.abs() * (1.0 - self.mix), leg_m);
        side_step(rig, &mut p, self.phase, w * side.abs(), side.signum(), leg_m);
        // Backing up: the body leans away from where it is going and the
        // steps are shorter, which is most of what tells a backpedal apart
        // from a walk seen from behind.
        if fwd < 0.0 {
            p.bones[0].x -= 0.12 * w * fwd.abs();
        }

        crouch(rig, &mut p, self.crouch, leg_m);
        airborne(rig, &mut p, self.air, self.jump, d.vel.y, leg_m);
        absorb(rig, &mut p, self.land, leg_m);
        // Leaning into a turn, and into a hard strafe.
        p.bones[0].z += (self.turn * 0.07 + side * w * 0.10).clamp(-0.3, 0.3);

        // Where it is looking. The head carries the aim; the chest takes a
        // share of it so the whole body reads as pointed at something, and
        // the shoulders stay level with the hips while it does.
        motors::look(rig, &mut p, 0.0, self.pitch);
        let chest = (self.pitch * 0.3).clamp(-0.35, 0.35) * self.hold;
        for i in 0..rig.bones.len() {
            if rig.bones[i].role == Role::Spine {
                p.bones[i].x += chest * 0.5;
            }
        }

        weapon(rig, &mut p, self.hold, self.pitch, self.kick);
        motors::breathe(&mut p, self.t, 2.2, leg_m);
        // Standing still, the weight shifts from one foot to the other: a
        // figure that holds perfectly still reads as a prop.
        let idle = (1.0 - self.stride_w) * (1.0 - self.crouch * 0.5);
        if idle > 0.0 {
            let s = (self.t * 0.6 + self.seed).sin();
            p.bones[0].z += 0.035 * s * idle;
            p.bones[0].y += 0.05 * (self.t * 0.37 + self.seed).sin() * idle;
            p.root.y -= 0.01 * leg_m * (1.0 + s) * idle;
        }
        p
    }

    /// Where a held weapon sits, in the world: its grip point, the yaw to
    /// draw it at, and the turn that points it along the aim.
    ///
    /// Taken from the hand itself rather than from a fixed offset on the
    /// body, so the gun goes where the arms went -- down at a sprint, up
    /// into the aim, back with the recoil -- without any of that being said
    /// twice. Only the direction is the aim's, because a shot has to leave
    /// along the line the bot is actually shooting.
    #[allow(clippy::too_many_arguments)]
    pub fn grip(&self, rig: &Rig, voxel: f32, scale: f32, pivot: Vec3, at: Vec3, yaw: f32, pitch: f32, alpha: f32) -> (Vec3, f32, Quat) {
        let pose = self.at(alpha);
        let mut fk = Vec::new();
        super::forward(rig, &pose, voxel * scale, pivot, &mut fk);
        let p = match hand_bone(rig) {
            Some(i) => at + Quat::from_rotation_y(yaw) * fk[i].0,
            None => at + Vec3::Y * 1.1,
        };
        (p, yaw, Quat::from_rotation_x(pitch))
    }
}

/// The bone a weapon is held in: the end of the right arm, or the deepest
/// arm bone there is on that side.
pub fn hand_bone(rig: &Rig) -> Option<usize> {
    (0..rig.bones.len())
        .filter(|&i| rig.bones[i].role == Role::Arm && rig.bones[i].side > 0.0)
        .max_by_key(|&i| motors::depth(rig, i))
}

/// Crouching: the knees fold, the hips drop and the back comes forward over
/// them. `w` is 0 standing, 1 all the way down.
fn crouch(rig: &Rig, p: &mut Pose, w: f32, leg_m: f32) {
    if w <= 0.0 {
        return;
    }
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        if b.role != Role::Leg {
            continue;
        }
        match motors::depth(rig, i) {
            0 => p.bones[i].x += 1.15 * w,
            1 => p.bones[i].x -= 2.0 * w,
            _ => p.bones[i].x += 0.85 * w,
        }
    }
    p.bones[0].x += 0.3 * w;
    p.root.y -= 0.6 * leg_m * w;
}

/// Off the ground: the push of the jump, the tuck at the top, and the legs
/// reaching for the ground on the way down.
fn airborne(rig: &Rig, p: &mut Pose, air: f32, jump: f32, vy: f32, leg_m: f32) {
    if air <= 0.0 && jump <= 0.0 {
        return;
    }
    let up = (air > 0.0) as u8 as f32;
    // Rising: knees up under the body. Falling: legs down and apart, ready.
    let rise = (vy / 6.0).clamp(-1.0, 1.0);
    let tuck = (rise.max(0.0) * 0.8 + jump * 0.5) * up;
    let reach = (-rise).max(0.0) * up;
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        let (role, side, dep) = (b.role, b.side, motors::depth(rig, i));
        let e = &mut p.bones[i];
        match (role, dep) {
            (Role::Leg, 0) => e.x += 1.1 * tuck - 0.25 * reach,
            (Role::Leg, 1) => e.x -= 1.5 * tuck + 0.35 * reach,
            (Role::Leg, 2) => {
                e.x += 0.5 * tuck - 0.3 * reach;
                e.z += side * 0.15 * reach;
            }
            // The arms come up as it leaves the ground and out as it falls.
            (Role::Arm, 0) => {
                e.x -= 0.5 * tuck;
                e.z += side * 0.35 * reach;
            }
            (Role::Arm, 1) => e.x += 0.45 * tuck,
            _ => {}
        }
    }
    p.bones[0].x += 0.18 * tuck - 0.12 * reach;
    p.root.y += 0.08 * leg_m * jump;
}

/// Taking a landing: the knees give, the hips drop and come back up.
fn absorb(rig: &Rig, p: &mut Pose, land: f32, leg_m: f32) {
    if land <= 0.0 {
        return;
    }
    // Hardest at the moment of contact, gone a third of a second later.
    let w = land * land;
    for i in 0..rig.bones.len() {
        if rig.bones[i].role != Role::Leg {
            continue;
        }
        match motors::depth(rig, i) {
            0 => p.bones[i].x += 0.85 * w,
            1 => p.bones[i].x -= 1.5 * w,
            _ => p.bones[i].x += 0.6 * w,
        }
    }
    p.bones[0].x += 0.3 * w;
    p.root.y -= 0.45 * leg_m * w;
}

/// Stepping sideways: a cross-over, where the trailing leg passes in front
/// of the leading one and then behind it on the next step. `dir` is 1 to its
/// right, -1 to its left.
///
/// This is a carioca, which is how a person actually travels sideways at
/// speed and is nothing like what was here before. The old version rolled
/// both thighs out and in against each other and never let them meet, which
/// reads as a shuffle: a man sidling along a ledge, not a man fighting.
///
/// Three channels carry it and each does one job.
///
/// - Roll takes the leg sideways. A roll is a compass sweep in the frontal
///   plane, so it moves the foot a long way across for a small angle and
///   leaves the toes pointing dead ahead the whole time.
/// - Yaw turns the leg as it crosses, and this is the one that makes it
///   read. A leg that arrives on the far side of the body still pointing
///   forward is a splay; a leg that arrives turned has stepped over. The
///   sign is `-side`, which is inward for either leg, and the rotation is
///   composed Ry*Rx*Rz, so the pitch below is applied first and the yaw then
///   sweeps a leg that is already swung -- which is the cross-over itself.
/// - Pitch puts the crossing leg in front on one step and behind on the
///   next, which is what makes it a cycle rather than a repeated hop.
///
/// The two legs run a quarter of a cycle apart rather than a half: one is
/// crossing while the other is planted, and a half would have them trade
/// places in mid-air with nothing on the floor.
fn side_step(rig: &Rig, p: &mut Pose, phase: f32, w: f32, dir: f32, leg_m: f32) {
    if w <= 0.0 {
        return;
    }
    // How far each channel goes at full weight. `YAW` is held under the 0.8
    // where the hip socket starts to open a seam between two rigid parts.
    const ROLL: f32 = 0.42;
    const YAW: f32 = 0.55;
    const CROSS: f32 = 0.38;
    const KNEE: f32 = 0.8;
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        if b.role != Role::Leg {
            continue;
        }
        // The leg on the side it is going toward leads.
        let lead = b.side * dir > 0.0;
        let ph = phase + if lead { 0.0 } else { 0.25 };
        // Where the leg is across the body, -1 to 1 in the travel direction:
        // two steps to a cycle.
        let lat = (2.0 * TAU * ph).sin();
        // Which way it passes the other leg: in front for one step, behind
        // for the next, so it alternates over the full cycle.
        let cross = (TAU * ph).cos();
        // Off the floor through the middle of its swing.
        let lift = (2.0 * TAU * ph).cos().max(0.0);
        // How far it is onto the other leg's side of the body, which is the
        // only part of the swing that is a crossing.
        let over = (-b.side * dir * lat).max(0.0);
        match motors::depth(rig, i) {
            0 => {
                p.bones[i].z += dir * ROLL * lat * w;
                p.bones[i].y -= b.side * YAW * over * w;
                p.bones[i].x += CROSS * cross * lift * w;
            }
            // The knee folds while the foot is in the air and carries a
            // little bend the rest of the time, as a moving man's does.
            1 => p.bones[i].x -= (KNEE * lift + 0.1) * w,
            // The sole stays flatter to the floor than the thigh's roll
            // would leave it.
            _ => p.bones[i].z -= dir * ROLL * lat * 0.7 * w,
        }
    }
    // The hips dip under each step, which is twice a cycle, and the
    // shoulders turn against the legs.
    p.root.y -= 0.04 * leg_m * (1.0 - (4.0 * TAU * phase).cos()) * 0.5 * w;
    p.bones[0].y -= dir * 0.10 * (TAU * phase).sin() * w;
    p.bones[0].z += dir * 0.06 * w;
}

/// A weapon in both hands. `w` is 0 down at the hip, 1 up and ready; `pitch`
/// is the aim and `kick` what the last shot did.
///
/// The angles are absolute and blended over whatever the stride left in the
/// arms, because a gun is held still against a moving body: an arm that went
/// on swinging with a rifle in it would saw it back and forth across the
/// screen.
fn weapon(rig: &Rig, p: &mut Pose, w: f32, pitch: f32, kick: f32) {
    if w <= 0.0 {
        return;
    }
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        if b.role != Role::Arm {
            continue;
        }
        let right = b.side > 0.0;
        // Up to the line of the aim: a straight-down arm is 0, level is a
        // quarter turn, and the aim carries it on from there.
        let a = FRAC_PI_2 * 0.62 + pitch * 0.55 - kick * 0.6;
        // Sideways is yaw here and not roll, which is the one thing about
        // these angles that is not a matter of taste. A bone's rotations are
        // read roll, then pitch, then yaw: an arm rolled out to the side
        // first is pointing along the axis the pitch turns about, so the
        // pitch then does nothing to it and the arm stays out at the side.
        // Pitched forward first and yawed in afterwards, the hands come
        // together in front of the chest, which is where a gun is.
        let want = match (motors::depth(rig, i), right) {
            // Firing hand: forward and in to the middle, wrist on the grip.
            (0, true) => Vec3::new(a, 0.22, 0.0),
            (1, true) => Vec3::new(0.52 - pitch * 0.12 + kick * 0.5, 0.1, 0.0),
            (2, true) => Vec3::new(-0.12, 0.0, 0.0),
            // Supporting hand: further across, under the barrel.
            (0, false) => Vec3::new(a + 0.06, -0.42, 0.0),
            (1, false) => Vec3::new(0.86 - pitch * 0.12 + kick * 0.25, -0.12, 0.0),
            _ => Vec3::new(-0.12, 0.0, 0.0),
        };
        p.bones[i] = p.bones[i].lerp(want, w);
    }
    // The shoulders take the shot too.
    for i in 0..rig.bones.len() {
        if rig.bones[i].role == Role::Spine {
            p.bones[i].x -= kick * 0.12 * w;
        }
    }
    p.bones[0].x -= kick * 0.1 * w;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;

    /// The robot's rig, sliced from the library as the loader does it.
    fn robot() -> (Rig, f32) {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let c = lib.creatures.iter().find(|c| c.name == "robot").expect("robot");
        let names: Vec<&String> = c.colors.keys().collect();
        let resolve = |n: &str| {
            if n == "air" {
                Some(0u8)
            } else {
                names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
            }
        };
        let whole = crate::world::shape::compile(&c.ops, [32, 32, 32], &resolve).mat;
        let s = crate::anim::slice::slice(c.rig.as_ref().expect("rig"), &c.ops, &whole, &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
        (s.rig, c.voxel)
    }

    fn run(f: &mut Figure, rig: &Rig, voxel: f32, d: Drive, steps: u32) -> Vec<Pose> {
        (0..steps)
            .map(|_| {
                f.step(rig, voxel, &d);
                f.pose.clone()
            })
            .collect()
    }

    /// The rig is the one the motors expect: two three-bone legs, two
    /// three-bone arms, and a trunk between them.
    #[test]
    fn the_humanoid_is_rigged_as_a_biped() {
        let (rig, voxel) = robot();
        assert_eq!(rig.plan, Some(Plan::Biped));
        let legs: Vec<usize> = (0..rig.bones.len()).filter(|&i| rig.bones[i].role == Role::Leg).collect();
        let arms: Vec<usize> = (0..rig.bones.len()).filter(|&i| rig.bones[i].role == Role::Arm).collect();
        assert_eq!(legs.len(), 6, "a biped has two legs of three bones");
        assert_eq!(arms.len(), 6, "and two arms of three");
        for (what, v) in [("leg", &legs), ("arm", &arms)] {
            for d in 0..3 {
                let n = v.iter().filter(|&&i| motors::depth(&rig, i) == d).count();
                assert_eq!(n, 2, "{what}s: {n} bones at depth {d}, want one a side");
            }
            assert_eq!(v.iter().filter(|&&i| rig.bones[i].side < 0.0).count(), 3);
            assert_eq!(v.iter().filter(|&&i| rig.bones[i].side > 0.0).count(), 3);
        }
        // Hips about half way up a 1.75 m body, which is what every motor
        // scales its amplitudes by.
        let hip = motors::hip(&rig) * voxel;
        assert!((0.8..1.0).contains(&hip), "hips at {hip} m");
        // No part model is empty: every bone owns some of the body.
        assert!(rig.bones.iter().all(|b| b.model < 64));
    }

    /// Walking, the legs swing in opposition and the arms against them, and
    /// nothing jumps between one step and the next.
    #[test]
    fn a_walk_is_a_walk() {
        let (rig, voxel) = robot();
        let mut f = Figure::new(&rig, 5);
        let d = Drive {
            vel: Vec3::new(0.0, 0.0, -2.6),
            ..Default::default()
        };
        let poses = run(&mut f, &rig, voxel, d, 300);
        let thigh = |s: f32| {
            (0..rig.bones.len())
                .find(|&i| rig.bones[i].role == Role::Leg && rig.bones[i].side == s && motors::depth(&rig, i) == 0)
                .unwrap()
        };
        let (tl, tr) = (thigh(-1.0), thigh(1.0));
        let arm = |s: f32| {
            (0..rig.bones.len())
                .find(|&i| rig.bones[i].role == Role::Arm && rig.bones[i].side == s && motors::depth(&rig, i) == 0)
                .unwrap()
        };
        let (al, ar) = (arm(-1.0), arm(1.0));
        let late = &poses[150..];
        let (mut opposed, mut both) = (0, 0);
        for p in late {
            let (l, r) = (p.bones[tl].x, p.bones[tr].x);
            if (l - r).abs() > 0.05 {
                both += 1;
                if l * r < 0.0 {
                    opposed += 1;
                }
            }
        }
        assert!(both > 100 && opposed * 4 > both * 3, "legs rarely opposed: {opposed} of {both}");
        // The hands are on a weapon, so the arms are not swinging: the test
        // of counter-swing belongs to a figure with nothing in its hands.
        let mut g = Figure::new(&rig, 5);
        let empty = Drive { holding: false, ..d };
        let poses = run(&mut g, &rig, voxel, empty, 300);
        let swing: f32 = poses[150..].iter().map(|p| (p.bones[al].x - p.bones[ar].x).abs()).sum::<f32>() / 150.0;
        assert!(swing > 0.15, "arms barely swing: {swing}");
        for p in poses[150..].iter() {
            // An arm swings against the leg on its own side.
            let (leg, armx) = (p.bones[tl].x, p.bones[al].x);
            if leg.abs() > 0.2 {
                assert!(leg * armx <= 0.06, "left arm swinging with the left leg: {leg} {armx}");
            }
        }
        // Smooth, once the arms have finished coming down. A knee at a jog
        // covers a quarter of a radian in a thirtieth of a second, so the
        // test is on the second difference: the pose may move fast, but it
        // may not change speed in one step.
        for w in poses[150..].windows(3) {
            for i in 0..rig.bones.len() {
                let (a, b, c) = (w[0].bones[i], w[1].bones[i], w[2].bones[i]);
                assert!((a - b * 2.0 + c).abs().max_element() < 0.15, "bone {i} jumped: {a:?} {b:?} {c:?}");
            }
            let (a, b, c) = (w[0].root, w[1].root, w[2].root);
            assert!((a - b * 2.0 + c).abs().max_element() < 0.05, "the body jumped: {a:?} {b:?} {c:?}");
        }
    }

    /// Strafing is not walking: the legs go out to the side, and the figure
    /// keeps facing the way it was facing.
    #[test]
    fn strafing_steps_sideways() {
        let (rig, voxel) = robot();
        let fwd = {
            let mut f = Figure::new(&rig, 1);
            run(
                &mut f,
                &rig,
                voxel,
                Drive {
                    vel: Vec3::new(0.0, 0.0, -3.0),
                    ..Default::default()
                },
                240,
            )
        };
        let side = {
            let mut f = Figure::new(&rig, 1);
            run(
                &mut f,
                &rig,
                voxel,
                Drive {
                    vel: Vec3::new(3.0, 0.0, 0.0),
                    ..Default::default()
                },
                240,
            )
        };
        let thighs: Vec<usize> = (0..rig.bones.len()).filter(|&i| rig.bones[i].role == Role::Leg && motors::depth(&rig, i) == 0).collect();
        // Where a foot is, which is the thing being asserted. An earlier
        // version of this measured the thighs' roll, which named the channel
        // rather than the property, and so failed the day the sidestep
        // became a cross-over and started carrying most of the motion in
        // yaw -- while being strictly more sideways than before.
        let feet = |p: &Pose| -> Vec<Vec3> {
            let mut fk = Vec::new();
            crate::anim::forward(&rig, p, voxel, Vec3::new(16.0, 0.0, 16.0), &mut fk);
            (0..rig.bones.len()).filter(|&i| rig.bones[i].role == Role::Leg && motors::depth(&rig, i) == 2).map(|i| fk[i].0).collect()
        };
        // How far across the body each foot travels over the cycle. Not the
        // mean distance from the middle, which a cross-over actually lowers:
        // a foot that sweeps from one side to the other spends its time near
        // the midline.
        let sweep = |ps: &[Pose]| -> f32 {
            let n = feet(&ps[0]).len();
            (0..n)
                .map(|k| {
                    let xs: Vec<f32> = ps[120..].iter().map(|p| feet(p)[k].x).collect();
                    xs.iter().copied().fold(f32::MIN, f32::max) - xs.iter().copied().fold(f32::MAX, f32::min)
                })
                .sum::<f32>()
                / n as f32
        };
        assert!(sweep(&side) > sweep(&fwd) * 3.0, "strafe {} vs walk {}", sweep(&side), sweep(&fwd));
        // And they cross: at some point in the cycle a foot is on the other
        // leg's side of the body. Walking never does this.
        let crossed = |ps: &[Pose]| -> bool {
            ps[120..].iter().any(|p| {
                let f = feet(p);
                thighs.iter().enumerate().any(|(k, &i)| rig.bones[i].side * f[k].x < -0.02)
            })
        };
        assert!(crossed(&side), "the legs never crossed, so it is still a shuffle");
        assert!(!crossed(&fwd), "walking crossed its legs");
    }

    /// A jump reads as a jump: the knees come up off the ground and the
    /// landing is taken in the legs.
    #[test]
    fn a_jump_tucks_and_the_landing_gives() {
        let (rig, voxel) = robot();
        let mut f = Figure::new(&rig, 3);
        let shin = (0..rig.bones.len()).find(|&i| rig.bones[i].role == Role::Leg && motors::depth(&rig, i) == 1).unwrap();
        let ground = Drive::default();
        run(&mut f, &rig, voxel, ground, 30);
        let standing = f.pose.bones[shin].x;
        let rising = Drive {
            vel: Vec3::new(0.0, 5.0, 0.0),
            on_ground: false,
            ..Default::default()
        };
        run(&mut f, &rig, voxel, rising, 10);
        let tucked = f.pose.bones[shin].x;
        assert!(tucked < standing - 0.5, "knees did not come up: {standing} -> {tucked}");
        // Down again: the first steps back on the ground give at the knee.
        run(
            &mut f,
            &rig,
            voxel,
            Drive {
                vel: Vec3::new(0.0, -8.0, 0.0),
                on_ground: false,
                ..Default::default()
            },
            20,
        );
        f.step(&rig, voxel, &ground);
        let landed = f.pose.bones[shin].x;
        assert!(landed < standing - 0.4, "landing was not absorbed: {standing} -> {landed}");
        // And it comes back out of it.
        run(&mut f, &rig, voxel, ground, 40);
        assert!((f.pose.bones[shin].x - standing).abs() < 0.2, "still crouched after landing");
    }

    /// Aiming up puts the gun up: the right hand rises with the aim, and the
    /// grip the shooter draws the weapon at comes from the hand itself.
    #[test]
    fn the_aim_carries_the_hands() {
        let (rig, voxel) = robot();
        let pivot = Vec3::new(16.0, 0.0, 16.0);
        let hand = |pitch: f32| -> Vec3 {
            let mut f = Figure::new(&rig, 2);
            let d = Drive { pitch, ..Default::default() };
            run(&mut f, &rig, voxel, d, 90);
            let (at, _, _) = f.grip(&rig, voxel, 1.0, pivot, Vec3::ZERO, 0.0, pitch, 1.0);
            at
        };
        let (down, level, up) = (hand(-0.9), hand(0.0), hand(0.9));
        assert!(up.y > level.y + 0.1 && level.y > down.y + 0.1, "hand height {down:?} {level:?} {up:?}");
        // The weapon is out in front of the body, not inside it.
        assert!(level.z < -0.1, "the gun is not held in front: {level:?}");
        assert!(level.y > 0.9 && level.y < 1.6, "held at {} m", level.y);
    }

    /// A body thrown by a blast turns in the air, and once it is down it
    /// goes over and lies on the floor like any other corpse. Printed so a
    /// failure says where in the flight it went wrong.
    #[test]
    fn a_thrown_body_lands_and_lies_down() {
        let (rig, voxel) = robot();
        for air in [2, 10, 40] {
            let mut f = Figure::new(&rig, 4);
            run(&mut f, &rig, voxel, Drive::default(), 30);
            f.die(&rig, voxel, 7);
            f.hurl(&rig, 9.0, 7);
            let flying = Drive {
                vel: Vec3::new(9.0, 3.0, 0.0),
                on_ground: false,
                ..Default::default()
            };
            run(&mut f, &rig, voxel, flying, air);
            // Down: on the floor from here on.
            let down = Drive {
                on_ground: true,
                ..Default::default()
            };
            run(&mut f, &rig, voxel, down, 30);
            let over = f.pose.bones[0].x.abs().max(f.pose.bones[0].z.abs());
            let mut fk = Vec::new();
            crate::anim::forward(&rig, &f.pose, voxel, Vec3::new(16.0, 0.0, 16.0), &mut fk);
            let high = fk.iter().map(|(p, _)| p.y).fold(f32::MIN, f32::max);
            let low = fk.iter().map(|(p, _)| p.y).fold(f32::MAX, f32::min);
            assert!(over > 0.8, "{air} steps in the air: still upright a second and a half after landing ({:?})", f.pose.bones[0]);
            // Looser than a body that was dropped: a tumble can leave a
            // knee or an elbow up, which is what a sprawl looks like.
            assert!(high < 1.0, "{air} steps in the air: standing after landing, highest joint {high:.2} m");
            assert!(low > -0.05 && low < 0.45, "{air} steps in the air: lying {low:.2} m off the floor");
        }
    }

    /// Dying hands the body to the ragdoll and it settles and stays down.
    #[test]
    fn the_dead_fall_over_and_stay_there() {
        let (rig, voxel) = robot();
        let mut f = Figure::new(&rig, 4);
        run(&mut f, &rig, voxel, Drive::default(), 30);
        let standing = f.pose.clone();
        f.die(&rig, voxel, 7);
        run(&mut f, &rig, voxel, Drive::default(), 400);
        assert!(f.dead());
        // Over, by whichever channel it went over on: a body that falls on
        // its face is as dead as one that falls on its side, and naming the
        // channel here is what made this fail when forwards became possible.
        let over = f.pose.bones[0].x.abs().max(f.pose.bones[0].z.abs());
        assert!(over > 0.8, "the body did not go over: {:?}", f.pose.bones[0]);
        // Where the body ends up, said in the only terms that matter: a
        // corpse lies ON the floor it fell on. The old version of this
        // pinned `root.y < -0.3`, which is a number off the drop that was
        // there at the time and says nothing about whether the body is in
        // the ground -- and a body in the ground passed it comfortably.
        let mut fk = Vec::new();
        crate::anim::forward(&rig, &f.pose, voxel, Vec3::new(16.0, 0.0, 16.0), &mut fk);
        let low = fk.iter().map(|(p, _)| p.y).fold(f32::MAX, f32::min);
        let high = fk.iter().map(|(p, _)| p.y).fold(f32::MIN, f32::max);
        assert!(low > -0.05, "the body settled {low} m into the floor");
        assert!(low < 0.45, "the body is lying {low} m above the floor");
        assert!(high < 0.9, "it is still standing up: its highest joint is at {high} m");
        let settled = f.pose.clone();
        run(&mut f, &rig, voxel, Drive::default(), 60);
        assert_eq!(settled.bones, f.pose.bones, "a settled body is still moving");
        f.revive();
        run(&mut f, &rig, voxel, Drive::default(), 60);
        assert!(!f.dead());
        assert!((f.pose.root.y - standing.root.y).abs() < 0.1, "it did not get back up");
    }
}
