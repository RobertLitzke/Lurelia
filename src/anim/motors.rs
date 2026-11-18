//! Motors: pure functions that write bone angles into a `Pose`, keyed by bone
//! role so they work on any creature with legs, a neck, a tail and so on.
//!
//! Sign conventions (model faces -Z, x right, y up): pitch +a swings a leg's
//! foot forward and tips a neck back (the head looks up); yaw +a turns left;
//! root pitch +a lifts the front of the body.

use crate::anim::{Plan, Role};
use super::{Pose, Rig};
use glam::Vec3;
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gait {
    Walk,
    Trot,
    Gallop,
    Bound,
}

impl Gait {
    pub fn name(self) -> &'static str {
        match self {
            Gait::Walk => "walk",
            Gait::Trot => "trot",
            Gait::Gallop => "gallop",
            Gait::Bound => "bound",
        }
    }
    /// Upper-leg swing and knee bend, radians.
    fn swing(self) -> (f32, f32) {
        match self {
            Gait::Walk => (0.32, 0.55),
            Gait::Trot => (0.5, 0.85),
            Gait::Gallop => (0.75, 1.1),
            Gait::Bound => (0.8, 1.0),
        }
    }
    /// Metres travelled per stride cycle, in leg lengths. A biped covers less
    /// ground per cycle than a quadruped does: a person's walking stride is
    /// about one and two thirds of a leg, where a deer's is nearly two and a
    /// half.
    pub fn cycle_for(self, plan: Option<Plan>) -> f32 {
        if plan != Some(Plan::Biped) {
            return self.cycle();
        }
        match self {
            Gait::Walk => 1.7,
            Gait::Trot => 2.4,
            Gait::Gallop | Gait::Bound => 3.2,
        }
    }

    /// Metres travelled per stride cycle, in leg lengths.
    pub fn cycle(self) -> f32 {
        match self {
            Gait::Walk => 2.4,
            Gait::Trot => 3.2,
            Gait::Gallop => 5.0,
            Gait::Bound => 4.0,
        }
    }
    /// Where in the cycle a leg starts its forward swing.
    fn offset(self, front: bool, left: bool) -> f32 {
        match (self, front, left) {
            // Lateral sequence: LH, LF, RH, RF.
            (Gait::Walk, false, true) => 0.0,
            (Gait::Walk, true, true) => 0.25,
            (Gait::Walk, false, false) => 0.5,
            (Gait::Walk, true, false) => 0.75,
            // Diagonal pairs.
            (Gait::Trot, true, true) | (Gait::Trot, false, false) => 0.0,
            (Gait::Trot, _, _) => 0.5,
            // Transverse gallop.
            (Gait::Gallop, false, true) => 0.0,
            (Gait::Gallop, false, false) => 0.1,
            (Gait::Gallop, true, true) => 0.45,
            (Gait::Gallop, true, false) => 0.55,
            // Hind legs together, then the fore legs.
            (Gait::Bound, false, l) => {
                if l {
                    0.0
                } else {
                    0.03
                }
            }
            (Gait::Bound, true, l) => {
                if l {
                    0.5
                } else {
                    0.53
                }
            }
        }
    }
}

/// The gait for a speed: by Froude number (v^2 / g L), so small and large
/// animals change gait at the same relative speed. Hoppers always bound.
///
/// A biped has the same three: a walk, a jog (`Trot`) and a run (`Gallop`).
/// It changes earlier than a quadruped because a person breaks into a jog at
/// a Froude number around 0.5 rather than carrying a walk up to 0.6.
pub fn gait_for(plan: Option<Plan>, speed: f32, leg_m: f32) -> Gait {
    if plan == Some(Plan::Hopper) {
        return Gait::Bound;
    }
    let fr = speed * speed / (9.8 * leg_m.max(0.05));
    if plan == Some(Plan::Biped) {
        return if fr < 0.5 {
            Gait::Walk
        } else if fr < 1.8 {
            Gait::Trot
        } else {
            Gait::Gallop
        };
    }
    if fr < 0.6 {
        Gait::Walk
    } else if fr < 2.5 || plan == Some(Plan::Sprawler) {
        Gait::Trot
    } else {
        Gait::Gallop
    }
}

pub fn upper_leg(rig: &Rig, i: usize) -> bool {
    let b = &rig.bones[i];
    b.role == Role::Leg && b.parent.is_none_or(|p| rig.bones[p].role != Role::Leg)
}

pub fn lower_leg(rig: &Rig, i: usize) -> bool {
    let b = &rig.bones[i];
    b.role == Role::Leg && b.parent.is_some_and(|p| rig.bones[p].role == Role::Leg)
}

/// How far down a chain of bones of one role this bone sits: 0 for the one
/// attached to the body, 1 for its child, 2 for the hand or foot. A biped's
/// limbs are three deep, which is what the motors below key on.
pub fn depth(rig: &Rig, i: usize) -> u32 {
    let role = &rig.bones[i].role;
    let (mut n, mut p) = (0, rig.bones[i].parent);
    while let Some(k) = p {
        if &rig.bones[k].role != role {
            break;
        }
        n += 1;
        p = rig.bones[k].parent;
        if n > 8 {
            break;
        }
    }
    n
}

/// Height of the hips above the feet, in model voxels.
pub fn hip(rig: &Rig) -> f32 {
    let (mut s, mut n) = (0.0, 0.0);
    for i in 0..rig.bones.len() {
        if upper_leg(rig, i) {
            s += rig.bones[i].joint.y;
            n += 1.0;
        }
    }
    if n > 0.0 {
        s / n
    } else {
        4.0
    }
}

fn has_lower(rig: &Rig, upper: usize) -> bool {
    rig.bones.iter().any(|b| b.role == Role::Leg && b.parent == Some(upper))
}

fn front(rig: &Rig, i: usize) -> bool {
    rig.bones[i].pair == 0
}

/// Posture levels: 0 standing, 1 sitting, 2 lying.
pub const STAND: f32 = 0.0;
pub const SIT: f32 = 1.0;
pub const LIE: f32 = 2.0;

/// A key posture's pose. `leg_m` is the hip height in metres.
fn key(rig: &Rig, level: u8, leg_m: f32) -> Pose {
    let mut p = Pose::rest(rig.bones.len());
    if rig.plan == Some(Plan::Biped) {
        return biped_key(rig, level, leg_m);
    }
    if level == 0 {
        return p;
    }
    let sprawl = rig.plan == Some(Plan::Sprawler);
    let hopper = rig.plan == Some(Plan::Hopper);
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        let fr = front(rig, i);
        let e = &mut p.bones[i];
        if upper_leg(rig, i) {
            let low = has_lower(rig, i);
            *e = match (level, fr, sprawl) {
                // Sitting: fore legs stay upright under a raised chest, hind legs fold.
                (1, true, false) => Vec3::new(if hopper { -0.6 } else { -0.45 }, 0.0, 0.0),
                (1, false, false) => Vec3::new(if low { 1.2 } else { 1.4 }, 0.0, 0.0),
                // Lying: fore legs fold back, hind legs forward (flat if one segment).
                (2, true, false) => Vec3::new(if low { -1.35 } else { -1.45 }, 0.0, 0.0),
                (2, false, false) => Vec3::new(if low { 1.35 } else { 1.45 }, 0.0, 0.0),
                // Sprawlers splay their legs out flat to rest on the belly.
                (_, _, true) => Vec3::new(0.0, 0.0, b.side * 1.1),
                _ => Vec3::ZERO,
            };
        } else if lower_leg(rig, i) && !sprawl {
            *e = match (level, fr) {
                (1, false) => Vec3::new(-1.9, 0.0, 0.0),
                (2, true) => Vec3::new(2.5, 0.0, 0.0),
                (2, false) => Vec3::new(-2.5, 0.0, 0.0),
                _ => Vec3::ZERO,
            };
        } else if b.role == Role::Neck && level == 2 && !sprawl {
            e.x = 0.1;
        }
    }
    match (level, sprawl) {
        (_, true) => p.root.y = -0.55 * leg_m,
        (1, false) => {
            p.root.y = -0.35 * leg_m;
            p.bones[0].x = if hopper { 0.6 } else { 0.45 };
        }
        _ => p.root.y = -0.72 * leg_m,
    }
    p
}

/// A biped's key postures. Standing is not the rest pose: a person at ease
/// has their arms a little out from their sides and their elbows a little
/// bent, and a rig drawn at attention reads as a mannequin.
fn biped_key(rig: &Rig, level: u8, leg_m: f32) -> Pose {
    let mut p = Pose::rest(rig.bones.len());
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        let (side, d) = (b.side, depth(rig, i));
        let e = &mut p.bones[i];
        match (b.role, level, d) {
            (Role::Arm, _, 0) => {
                e.z += side * 0.13;
                e.x -= 0.05;
            }
            (Role::Arm, _, 1) => e.x += 0.22,
            // Sitting: thighs forward, knees down, feet flat under them.
            (Role::Leg, 1, 0) => e.x += 1.45,
            (Role::Leg, 1, 1) => e.x -= 1.5,
            (Role::Leg, 1, 2) => e.x += 0.1,
            // Lying on the back, legs out straight with the knees just bent.
            (Role::Leg, 2, 0) => e.x += 0.18,
            (Role::Leg, 2, 1) => e.x -= 0.25,
            (Role::Leg, 2, 2) => e.x -= 0.5,
            _ => {}
        }
    }
    match level {
        0 => {}
        1 => {
            p.root.y = -0.52 * leg_m;
            p.bones[0].x = 0.12;
        }
        _ => {
            // Over onto the back: the trunk turns a quarter about the hips,
            // so the body has to come down by its own hip height to lie.
            p.bones[0].x = -1.45;
            p.root.y = -0.92 * leg_m;
        }
    }
    p
}

/// The posture pose at a level between 0 and 2. Without a sitting posture the
/// creature goes straight from standing to lying.
pub fn posture(rig: &Rig, level: f32, has_sit: bool, leg_m: f32) -> Pose {
    let stand = key(rig, 0, leg_m);
    if level <= 0.001 {
        return stand;
    }
    if !has_sit {
        return stand.lerp(&key(rig, 2, leg_m), level / 2.0);
    }
    if level <= 1.0 {
        stand.lerp(&key(rig, 1, leg_m), level)
    } else {
        key(rig, 1, leg_m).lerp(&key(rig, 2, leg_m), level - 1.0)
    }
}

/// Where a biped's limb is in the cycle. Phase 0 is the left foot flat under
/// the hip; the right leg is half a cycle behind it, and each arm swings with
/// the leg on the other side.
fn biped_phase(side: f32, arm: bool) -> f32 {
    let left = side <= 0.0;
    if left == arm {
        0.5
    } else {
        0.0
    }
}

/// A person's stride: legs alternating, knees flexing through the swing,
/// ankles keeping the feet flat, arms counter-swinging, and the hips
/// dropping, rolling and turning against the shoulders.
///
/// Written apart from the quadruped cycle rather than as another row in its
/// tables, because almost nothing carries over: two legs instead of four, a
/// knee that bends only backwards against an elbow that bends only forwards,
/// and a trunk that twists where a quadruped's stays still.
fn biped(rig: &Rig, pose: &mut Pose, g: Gait, phase: f32, w: f32, leg_m: f32) {
    // Thigh swing, knee flexion, arm swing, and how far the body leans in.
    let (swing, knee, arm, lean) = match g {
        Gait::Walk => (0.42, 0.95, 0.30, 0.03),
        Gait::Trot => (0.62, 1.35, 0.60, 0.11),
        Gait::Gallop | Gait::Bound => (0.70, 1.55, 0.82, 0.16),
    };
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        let (role, side, d) = (b.role, b.side, depth(rig, i));
        let leg = role == Role::Leg;
        if !leg && role != Role::Arm {
            continue;
        }
        let t = TAU * (phase + biped_phase(side, !leg));
        let (s, c) = t.sin_cos();
        let e = &mut pose.bones[i];
        if leg {
            // The knee is folded hardest just after the foot leaves the
            // ground, which is a little before the thigh is at its most
            // forward: hence the lead in the cosine. A raised cosine rather
            // than a clamped one, because a clamp is a corner, and a corner
            // in a joint angle is a visible tick in the walk.
            let fold = (0.5 + 0.5 * (t + 0.7).cos()).powi(2);
            match d {
                // Thigh: forward through the swing, back through the stance,
                // with the whole leg a touch forward of straight.
                0 => e.x += (swing * s + 0.06) * w,
                // Knee: bends backwards only. A little flex is kept through
                // the stance too, so the leg never snaps to a straight line.
                1 => e.x -= (knee * fold + 0.10) * w,
                // Ankle: holds the foot flat against the two joints above it,
                // then points the toes as the foot pushes off.
                _ => {
                    let toe = (0.5 + 0.5 * (t + 2.5).cos()).powi(3);
                    e.x += ((swing * s * -0.55 + knee * fold * 0.5) - 0.45 * toe * (1.0 + lean)) * w;
                }
            }
        } else {
            match d {
                // Upper arm: counter-swings, and is held a little out from
                // the ribs so it does not saw through them.
                0 => {
                    e.x += arm * s * w;
                    e.z += side * (0.10 + 0.08 * lean) * w;
                }
                // Elbow: bends forwards only, most when the arm is back.
                1 => e.x += (arm * 0.7 * (0.55 - 0.45 * c) + 0.12) * w,
                // Wrist: steadies the hand against the forearm.
                _ => e.x -= arm * 0.25 * (0.5 - 0.5 * c) * w,
            }
        }
    }
    // The hips: two dips a cycle, a roll toward the leg in the air, and a
    // turn that the shoulders answer with the opposite one.
    let t = TAU * phase;
    let s = t.sin();
    pose.root.y -= (0.5 - 0.5 * (2.0 * t).cos()) * (0.035 + 0.05 * lean) * leg_m * w;
    pose.bones[0].z += 0.07 * s * w;
    pose.bones[0].y += (0.10 + 0.25 * lean) * s * w;
    pose.bones[0].x += lean * w;
    for i in 0..rig.bones.len() {
        if rig.bones[i].role == Role::Spine {
            let share = (depth(rig, i) + 1) as f32 * 0.5;
            pose.bones[i].y -= (0.10 + 0.25 * lean) * s * share * w;
            pose.bones[i].x += lean * 0.4 * w;
        }
    }
    // Running leaves the ground: one rise and fall a step, not a cycle.
    if matches!(g, Gait::Gallop | Gait::Bound) {
        pose.root.y += 0.06 * leg_m * (2.0 * t + 1.2).sin() * w;
    }
}

/// Legs swinging through a stride, weighted by `w` (0 standing, 1 moving).
pub fn gait(rig: &Rig, pose: &mut Pose, g: Gait, phase: f32, w: f32, leg_m: f32) {
    if w <= 0.0 {
        return;
    }
    if rig.plan == Some(Plan::Biped) {
        return biped(rig, pose, g, phase, w, leg_m);
    }
    let (a, k) = g.swing();
    for i in 0..rig.bones.len() {
        if !upper_leg(rig, i) {
            continue;
        }
        let b = &rig.bones[i];
        let fr = front(rig, i);
        let t = TAU * (phase + g.offset(fr, b.side <= 0.0));
        let swing = a * t.sin() * w;
        if rig.plan == Some(Plan::Sprawler) {
            // Legs out to the side swing round a vertical axis.
            pose.bones[i].y += swing * -b.side;
            pose.bones[i].z += b.side * 0.5 * w;
        } else {
            pose.bones[i].x += swing;
        }
        // Bend the knee while the foot comes forward, so it clears the ground.
        let lift = k * t.cos().max(0.0) * w;
        let lower = rig.bones.iter().position(|c| c.parent == Some(i) && c.role == Role::Leg);
        match lower {
            Some(j) => pose.bones[j].x += if fr { -lift } else { lift * 0.8 },
            None if rig.plan != Some(Plan::Sprawler) => pose.bones[i].x += lift * 0.25 * if fr { 1.0 } else { -1.0 },
            None => {}
        }
    }
    let t = TAU * phase;
    match g {
        Gait::Walk | Gait::Trot => pose.root.y -= 0.03 * leg_m * (2.0 * t).sin().abs() * w,
        Gait::Gallop | Gait::Bound => {
            pose.bones[0].x += 0.12 * (t + 1.2).sin() * w;
            pose.root.y += 0.08 * leg_m * (t + 0.6).sin() * w;
        }
    }
    // Sprawlers swing their spine and tail against the legs.
    if rig.plan == Some(Plan::Sprawler) {
        for i in 0..rig.bones.len() {
            let r = rig.bones[i].role;
            if r == Role::Spine || r == Role::Tail {
                pose.bones[i].y += 0.35 * (t + 0.5).sin() * w;
            }
        }
    }
}

/// Turn the neck and head by (yaw, pitch): about 60% in the neck bones and 40%
/// in the head, or all in whichever exists.
pub fn look(rig: &Rig, pose: &mut Pose, yaw: f32, pitch: f32) {
    let necks: Vec<usize> = (0..rig.bones.len()).filter(|&i| rig.bones[i].role == Role::Neck).collect();
    let head = rig.bones.iter().position(|b| b.role == Role::Head);
    let (nshare, hshare) = match (necks.is_empty(), head.is_some()) {
        (false, true) => (0.6, 0.4),
        (false, false) => (1.0, 0.0),
        (true, true) => (0.0, 1.0),
        (true, false) => return,
    };
    let per = nshare / necks.len().max(1) as f32;
    for &i in &necks {
        pose.bones[i].y += yaw * per;
        pose.bones[i].x += pitch * per * 0.5;
    }
    if let Some(h) = head {
        pose.bones[h].y += yaw * hshare;
        pose.bones[h].x += pitch * (hshare + nshare * 0.5);
    }
}

/// Head down to the ground and nibbling, weighted by `w`.
pub fn graze(rig: &Rig, pose: &mut Pose, w: f32, t: f32) {
    if w <= 0.0 {
        return;
    }
    let necks = rig.bones.iter().filter(|b| b.role == Role::Neck).count();
    // Without a neck the head does all the bending.
    let head = if necks == 0 { 1.1 } else { 0.55 };
    for i in 0..rig.bones.len() {
        match rig.bones[i].role {
            Role::Neck => pose.bones[i].x -= 1.3 / necks as f32 * w,
            Role::Head => pose.bones[i].x -= (head + 0.08 * (t * 9.0).sin()) * w,
            Role::Root | Role::Spine | Role::Jaw | Role::Tail | Role::Leg | Role::Arm | Role::Wing | Role::Ear => {}
        }
    }
}

/// Tail swish and stride bounce; `alarm` raises it (a deer's white flag).
pub fn tail(rig: &Rig, pose: &mut Pose, t: f32, seed: f32, phase: f32, w: f32, alarm: f32) {
    for i in 0..rig.bones.len() {
        if rig.bones[i].role != Role::Tail {
            continue;
        }
        pose.bones[i].y += 0.25 * (t * 1.7 + seed).sin() * (1.0 - w * 0.6);
        // Sprawlers keep their tails down when they run.
        let flag = if rig.plan == Some(Plan::Sprawler) { 0.0 } else { 0.7 };
        pose.bones[i].x += -0.15 * (TAU * 2.0 * phase).sin() * w - flag * alarm;
    }
}

/// Ear flick (`flick` 0..1 rises and falls) and ears forward when alert.
pub fn ears(rig: &Rig, pose: &mut Pose, flick: f32, alert: f32) {
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        if b.role != Role::Ear {
            continue;
        }
        pose.bones[i].z += b.side * 0.4 * flick;
        pose.bones[i].x += 0.25 * alert;
    }
}

/// A slow rise and fall of the body.
pub fn breathe(pose: &mut Pose, t: f32, rate: f32, leg_m: f32) {
    pose.root.y += 0.008 * leg_m * (t * rate).sin();
}

/// Wings folded back along the body, for a bird walking on the ground (the
/// same fold flight.rs uses for a perched bird).
pub fn fold_wings(rig: &Rig, pose: &mut Pose) {
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        if b.role != Role::Wing {
            continue;
        }
        let inner = b.parent.is_none_or(|q| rig.bones[q].role != Role::Wing);
        pose.bones[i] = if inner {
            Vec3::new(0.0, -b.side * 1.45, -b.side * 0.25)
        } else {
            Vec3::new(0.0, -b.side * 2.8, 0.0)
        };
    }
}

/// Head, neck, legs and tail drawn into the shell (`w` 0 out, 1 in): their
/// joints pulled toward the body's centre, straightened, and the shell let
/// down onto the ground.
pub fn retract(rig: &Rig, pose: &mut Pose, w: f32, leg_m: f32) {
    if w <= 0.0 {
        return;
    }
    if pose.tuck.len() < rig.bones.len() {
        pose.tuck.resize(rig.bones.len(), 0.0);
    }
    for i in 1..rig.bones.len() {
        let pull = match rig.bones[i].role {
            Role::Head | Role::Neck => 0.75,
            Role::Leg if upper_leg(rig, i) => 0.6,
            Role::Tail => 0.5,
            Role::Root | Role::Spine | Role::Jaw | Role::Leg | Role::Arm | Role::Wing | Role::Ear => 0.0,
        };
        if pull > 0.0 {
            pose.tuck[i] = pull * w;
            pose.bones[i] *= 1.0 - w;
            // Legs that stick out sideways (sea turtle flippers) fold back
            // flat under the rim of the shell.
            if rig.bones[i].role == Role::Leg && rig.bones[i].side != 0.0 {
                let fold = if rig.bones[i].pair == 0 { 1.2 } else { 0.7 };
                pose.bones[i].y += -rig.bones[i].side * fold * w;
            }
        }
    }
    pose.root.y = pose.root.y * (1.0 - w) - 0.4 * leg_m * w;
    pose.bones[0].x *= 1.0 - w;
}

/// A ray's wings: a wave running out along each wing chain (`wing` role,
/// inner bone first), so the wing rises and falls in a ripple rather than a
/// hinge. `phase` in cycles, `amp` radians at the inner joint, `w` 0..1.
pub fn undulate(rig: &Rig, pose: &mut Pose, phase: f32, amp: f32, w: f32) {
    for i in 0..rig.bones.len() {
        let b = &rig.bones[i];
        match b.role {
            Role::Wing => {
                // How far out along the chain this bone is.
                let mut k = 0;
                let mut q = b.parent;
                while let Some(j) = q {
                    if rig.bones[j].role != Role::Wing {
                        break;
                    }
                    k += 1;
                    q = rig.bones[j].parent;
                }
                let a = amp * (1.0 + 0.35 * k as f32) * if k == 0 { 1.0 } else { 0.9 };
                let wave = (TAU * (phase - 0.18 * k as f32)).sin();
                pose.bones[i].z += b.side * a * wave * w;
            }
            Role::Tail => pose.bones[i].x += 0.12 * (TAU * (phase - 0.5)).sin() * w,
            Role::Root | Role::Spine | Role::Neck | Role::Head | Role::Jaw | Role::Leg | Role::Arm | Role::Ear => {}
        }
    }
    // The body rises a little on each downstroke.
    pose.bones[0].x += 0.05 * (TAU * (phase + 0.25)).sin() * w;
}
