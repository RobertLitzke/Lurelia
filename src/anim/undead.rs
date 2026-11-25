//! How a zombie moves: four ways of walking laid over a person's stride.
//!
//! `figure.rs` shows a body doing what something else decided, and a zombie
//! is no different: the brain still decides where it goes and how fast. The
//! style is how the body manages it, and it is the zombie's own for as long
//! as it is up (`Figure::walk`):
//!
//! - `Shamble`: short dragging steps, head down and to one side, arms
//!   hanging and swinging only with the body, one foot scuffed along.
//! - `Lurch`: one leg will not bend, so every other step is a stiff swing
//!   from the hip with the body thrown over it, and both arms held out
//!   ahead with the hands hanging.
//! - `Runner`: a sprint leaning hard into it, arms clawing forward and the
//!   head thrust out, with a twitch through the head and hands.
//! - `Crawler`: down on its front, dragging itself on its forearms with the
//!   legs trailing. The collider is the brain's business (it is still the
//!   standing box until the zombie round gives it a low one).
//!
//! Each is a pass over the pose the stride left, plus two numbers the stride
//! itself uses (`stride`): how long a step is, and how fast the legs go for
//! a given speed.

use super::motors::depth;
use super::{Pose, Rig, Role};
use std::f32::consts::TAU;

crate::util::named! {
    /// How a figure walks. `Upright` is a person's walk and adds nothing.
    #[derive(Default)]
    pub enum Walk {
        #[default]
        Upright = "upright",
        Shamble = "shamble",
        Lurch = "lurch",
        Runner = "runner",
        Crawler = "crawler",
    }
}

impl Walk {
    /// A zombie's style from its seed. A runner is the round's decision (it
    /// is faster, so it is a difficulty), and so is a crawler; of the rest
    /// about half shamble and half lurch.
    pub fn for_zombie(seed: u32, runner: bool, crawler: bool) -> Walk {
        if crawler {
            Walk::Crawler
        } else if runner {
            Walk::Runner
        } else if crate::world::noise::hash32(seed ^ 0x5a11) & 1 == 0 {
            Walk::Shamble
        } else {
            Walk::Lurch
        }
    }

    /// The stride under this style: how far each step swings, as a share of
    /// a person's, and how long a cycle is, as a share of a person's for the
    /// same leg. Short steps at the same speed come round faster.
    pub fn stride(self) -> (f32, f32) {
        match self {
            Walk::Upright => (1.0, 1.0),
            Walk::Shamble => (0.55, 0.62),
            Walk::Lurch => (0.85, 0.9),
            Walk::Runner => (1.15, 0.85),
            Walk::Crawler => (0.0, 0.45),
        }
    }

    /// The phase the legs are drawn at. A lurch's stiff leg swings through
    /// quickly and the good one takes its time, so the cycle is bent: the
    /// first half (the stiff leg's swing) goes by in about a third of it.
    pub fn bend(self, phase: f32) -> f32 {
        match self {
            Walk::Lurch => phase + 0.09 * (TAU * phase).sin(),
            _ => phase,
        }
    }
}

/// The bones of one limb, upper to lower, on one side (-1 left, 1 right).
fn limb(rig: &Rig, role: Role, side: f32) -> [Option<usize>; 3] {
    let find = |d: u32| (0..rig.bones.len()).find(|&i| rig.bones[i].role == role && rig.bones[i].side * side > 0.0 && depth(rig, i) == d);
    [find(0), find(1), find(2)]
}

fn each(rig: &Rig, role: Role) -> impl Iterator<Item = usize> + '_ {
    (0..rig.bones.len()).filter(move |&i| rig.bones[i].role == role)
}

/// Tip the trunk forward by `a` over the spine bones (as `figure::lean`).
fn lean(rig: &Rig, p: &mut Pose, a: f32) {
    let n = each(rig, Role::Spine).count().max(1) as f32;
    for i in each(rig, Role::Spine) {
        p.bones[i].x -= a / n;
    }
}

/// The head's nod (forward is negative, as for the spine) and its tilt to
/// one side, shared between the neck and the head.
fn head(rig: &Rig, p: &mut Pose, nod: f32, tilt: f32) {
    for i in each(rig, Role::Neck) {
        p.bones[i].x -= nod * 0.5;
        p.bones[i].z += tilt * 0.4;
    }
    for i in each(rig, Role::Head) {
        p.bones[i].x -= nod * 0.5;
        p.bones[i].z += tilt * 0.6;
    }
}

/// Lay the style over a pose the stride has already been drawn into.
///
/// `phase` is the stride's (cycles), `w` how much it is moving (0 to 1),
/// `t` seconds alive and `seed` its own 0..10, for the twitches and for
/// which side is the bad one.
#[allow(clippy::too_many_arguments)]
pub fn pose(rig: &Rig, p: &mut Pose, walk: Walk, phase: f32, w: f32, t: f32, seed: f32, leg_m: f32) {
    // The bad side: the dragged foot, the stiff knee, the way the head lolls.
    let bad = if seed.fract() < 0.5 { -1.0 } else { 1.0 };
    match walk {
        Walk::Upright => {}
        Walk::Shamble => shamble(rig, p, phase, w, t, seed, bad, leg_m),
        Walk::Lurch => lurch(rig, p, phase, w, t, seed, bad, leg_m),
        Walk::Runner => runner(rig, p, phase, w, t, seed, leg_m),
        Walk::Crawler => crawler(rig, p, phase, w, t, seed, leg_m),
    }
    // Every one of them sways a little standing, more than a living man.
    if walk != Walk::Upright && walk != Walk::Crawler {
        let idle = 1.0 - w;
        p.bones[0].z += 0.05 * (t * 0.9 + seed).sin() * idle;
        lean(rig, p, 0.04 * (t * 0.55 + seed * 1.7).sin() * idle);
    }
}

#[allow(clippy::too_many_arguments)]
fn shamble(rig: &Rig, p: &mut Pose, phase: f32, w: f32, t: f32, seed: f32, bad: f32, leg_m: f32) {
    lean(rig, p, 0.24 + 0.06 * w);
    head(rig, p, 0.38, bad * 0.32 + 0.05 * (t * 0.7 + seed).sin());
    // The arms hang dead and swing only with the body's roll, a beat late.
    for side in [-1.0f32, 1.0] {
        let [up, fore, hand] = limb(rig, Role::Arm, side);
        let swing = 0.14 * (TAU * phase * 2.0 + 1.3 + side).sin() * w;
        if let Some(i) = up {
            // Cancel most of the stride's own counter-swing.
            p.bones[i].x = p.bones[i].x * 0.25 + swing + 0.05;
            p.bones[i].z += side * 0.04;
        }
        if let Some(i) = fore {
            p.bones[i].x = 0.08 + 0.06 * w;
        }
        if let Some(i) = hand {
            p.bones[i].x = 0.15;
        }
    }
    // The bad foot hardly lifts: knee nearly straight, toes scuffing.
    let [thigh, shin, foot] = limb(rig, Role::Leg, bad);
    if let Some(i) = thigh {
        p.bones[i].x *= 0.7;
        p.bones[i].z += bad * 0.06 * w;
    }
    if let Some(i) = shin {
        p.bones[i].x *= 0.3;
    }
    if let Some(i) = foot {
        p.bones[i].x = p.bones[i].x * 0.4 + 0.25 * w;
    }
    // The hips roll and drop toward the bad leg each time it takes weight.
    let s = (TAU * phase).sin();
    p.bones[0].z += (0.10 * s + 0.05 * bad) * w;
    p.root.y -= 0.025 * leg_m * (0.5 + 0.5 * (TAU * phase + if bad > 0.0 { std::f32::consts::PI } else { 0.0 }).cos()) * w;
}

#[allow(clippy::too_many_arguments)]
fn lurch(rig: &Rig, p: &mut Pose, phase: f32, w: f32, t: f32, seed: f32, bad: f32, leg_m: f32) {
    // The stiff leg: no knee at all, so it swings out round from the hip.
    let [thigh, shin, foot] = limb(rig, Role::Leg, bad);
    // Its swing is the half of the cycle it is in the air.
    let mine = if bad < 0.0 { 0.0 } else { 0.5 };
    let air = (TAU * (phase + mine)).cos().max(0.0);
    if let Some(i) = shin {
        p.bones[i].x *= 0.1;
    }
    if let Some(i) = thigh {
        p.bones[i].z += bad * 0.22 * air * w;
    }
    if let Some(i) = foot {
        p.bones[i].x *= 0.3;
    }
    // The hip hitches up to get it through, and the body is thrown over the
    // good leg and forward as it lands.
    p.bones[0].z -= bad * 0.16 * air * w;
    p.root.y += 0.03 * leg_m * air * w;
    lean(rig, p, 0.16 + 0.12 * (1.0 - air) * w);
    head(rig, p, 0.12 - 0.15 * air * w, -bad * 0.18 * (TAU * phase).sin() * w + bad * 0.12);
    // Both arms out ahead, a little apart and drooping, the hands hanging.
    for side in [-1.0f32, 1.0] {
        let [up, fore, hand] = limb(rig, Role::Arm, side);
        let bob = 0.06 * (TAU * phase * 2.0 + side).sin() * w + 0.03 * (t * 1.3 + seed + side).sin();
        if let Some(i) = up {
            p.bones[i].x = 1.22 + bob - if side == bad { 0.12 } else { 0.0 };
            p.bones[i].z = -side * 0.08;
            p.bones[i].y = 0.0;
        }
        if let Some(i) = fore {
            p.bones[i].x = 0.22;
        }
        if let Some(i) = hand {
            p.bones[i].x = -0.55;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn runner(rig: &Rig, p: &mut Pose, phase: f32, w: f32, t: f32, seed: f32, _leg_m: f32) {
    lean(rig, p, 0.18 + 0.26 * w);
    // The head thrust forward on the neck and lifted to see where it goes,
    // and twitching.
    let twitch = 0.06 * (t * 13.0 + seed).sin() * (t * 2.3 + seed).sin().max(0.0);
    for i in each(rig, Role::Neck) {
        p.bones[i].x -= 0.35 * w;
    }
    for i in each(rig, Role::Head) {
        p.bones[i].x += 0.4 * w + twitch;
        p.bones[i].z += twitch;
    }
    // Arms clawing forward in turn rather than pumping: each reaches high
    // ahead and rakes down, elbows open, out from the body.
    for side in [-1.0f32, 1.0] {
        let [up, fore, hand] = limb(rig, Role::Arm, side);
        let c = (TAU * phase + if side < 0.0 { 0.0 } else { std::f32::consts::PI }).sin();
        if let Some(i) = up {
            p.bones[i].x = (0.95 + 0.5 * c) * w + p.bones[i].x * (1.0 - w);
            p.bones[i].z += side * 0.22 * w;
        }
        if let Some(i) = fore {
            p.bones[i].x = (0.35 + 0.25 * (1.0 - c)) * w + p.bones[i].x * (1.0 - w);
        }
        if let Some(i) = hand {
            p.bones[i].x = -0.3 * w + 0.15 * twitch;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn crawler(rig: &Rig, p: &mut Pose, phase: f32, w: f32, t: f32, seed: f32, leg_m: f32) {
    // Face down: the whole body turned a quarter about the hips and let down
    // until the chest is on the ground.
    let s = (TAU * phase).sin();
    p.bones[0].x = -1.42;
    p.bones[0].y = 0.10 * s * w;
    p.bones[0].z = 0.0;
    p.root.y = -0.82 * leg_m + 0.02 * leg_m * (TAU * phase * 2.0).sin().abs() * w;
    // The trunk straight along the ground, the head lifted to look ahead.
    for i in each(rig, Role::Spine) {
        p.bones[i].x = 0.0;
        p.bones[i].y = -0.08 * s * w;
    }
    for i in each(rig, Role::Neck) {
        p.bones[i].x = 0.45;
    }
    for i in each(rig, Role::Head) {
        p.bones[i].x = 0.4 + 0.05 * (t * 0.8 + seed).sin();
        p.bones[i].z = 0.0;
    }
    // The legs trail, a little apart, knees just bent, one twitching.
    for side in [-1.0f32, 1.0] {
        let [thigh, shin, foot] = limb(rig, Role::Leg, side);
        if let Some(i) = thigh {
            p.bones[i] = glam::Vec3::new(-0.05 + 0.06 * (TAU * phase + side).sin() * w, 0.0, side * 0.14);
        }
        if let Some(i) = shin {
            p.bones[i] = glam::Vec3::new(-0.25 - 0.15 * (t * 3.1 + seed + side).sin().max(0.0), 0.0, 0.0);
        }
        if let Some(i) = foot {
            p.bones[i] = glam::Vec3::new(-0.9, 0.0, 0.0);
        }
    }
    // The arms do the work: each reaches out ahead, plants the forearm and
    // drags the body up to it, the two half a cycle apart. Lying down, an
    // arm raised over the head is an arm reaching along the ground.
    for side in [-1.0f32, 1.0] {
        let [up, fore, hand] = limb(rig, Role::Arm, side);
        let c = (TAU * phase + if side < 0.0 { 0.0 } else { std::f32::consts::PI }).sin();
        let reach = 0.5 + 0.5 * c;
        if let Some(i) = up {
            p.bones[i] = glam::Vec3::new(1.55 + 0.75 * reach * w + 0.3 * (1.0 - w), 0.0, side * (0.35 - 0.15 * reach));
        }
        if let Some(i) = fore {
            p.bones[i] = glam::Vec3::new(1.2 - 0.7 * reach * w, 0.0, 0.0);
        }
        if let Some(i) = hand {
            p.bones[i] = glam::Vec3::new(0.2, 0.0, 0.0);
        }
    }
}
