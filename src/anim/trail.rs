//! Long necks and tails that bend out of line with the body: a sauropod's.
//!
//! A tail of three bones or more (two for a sprawler: a crocodile, a
//! lizard) is a rope dragged behind the hips, the
//! snake's way (spine.rs): its root follows the body, every point behind it
//! is pulled up to its own distance from the one ahead, so when the animal
//! turns the tail swings wide round the turn and straightens as it walks on.
//! The rope runs on the flat; each bone keeps its own droop from the model,
//! and no joint bends further than `MAX_BEND`, so turning on the spot folds
//! the tail round in a curve rather than a hairpin.
//!
//! A neck of three bones or more browses: it swings well out to one side and
//! the other while the animal grazes, and leads into a turn while it walks,
//! the bend growing toward the head.
//!
//! Neither counts in the crowd (creatures/crowd.rs, `trunk_reach`): a neck
//! or tail that bends gets out of a neighbour's way, so only the trunk keeps
//! animals apart.

use super::spine::{self, Chain, Spine};
use super::{Pose, Rig, Role};
use glam::{Quat, Vec2, Vec3};

/// The fewest bones a neck or tail needs to bend this way.
pub const MIN_BONES: usize = 3;

/// The most one joint of a tail turns from the bone before it, radians.
const MAX_BEND: f32 = 0.45;

/// How fast a tail left standing eases back straight, a share a second.
const STRAIGHTEN: f32 = 0.12;

/// The chain of bones of one role hanging off the body: the first bone of
/// that role whose parent is not, then each child of the same role. Its
/// `d` has one entry per bone, the last repeating the one before as the tip.
pub fn role_chain(rig: &Rig, role: Role) -> Option<Chain> {
    let first = rig.bones.iter().position(|b| b.role == role && b.parent.is_none_or(|p| rig.bones[p].role != role))?;
    let mut bones = vec![first];
    while let Some(next) = rig.bones.iter().position(|b| b.role == role && b.parent == Some(*bones.last().unwrap())) {
        bones.push(next);
        if bones.len() > rig.bones.len() {
            break;
        }
    }
    let mut d: Vec<f32> = bones.windows(2).map(|w| (rig.bones[w[1]].joint - rig.bones[w[0]].joint).length()).collect();
    d.push(d.last().copied().unwrap_or(2.0));
    Some(Chain { bones, d })
}

/// A long enough chain of this role, or none.
pub fn flexible(rig: &Rig, role: Role) -> Option<Chain> {
    role_chain(rig, role).filter(|c| c.n() >= MIN_BONES)
}

/// The bones of this rig's tail that are dragged as a rope (`Tail`), or
/// none: a tail of `MIN_BONES` or more, or of two on a sprawler (a
/// crocodile, a lizard).
pub fn dragged_tail(rig: &Rig) -> Option<Chain> {
    let min = if rig.plan == Some(super::Plan::Sprawler) { 2 } else { MIN_BONES };
    role_chain(rig, Role::Tail).filter(|c| c.n() >= min)
}

/// A tail dragged as a rope.
#[derive(Clone, Debug)]
pub struct Tail {
    ch: Chain,
    /// The same chain measured on the flat, for the rope.
    flat: Chain,
    /// One world point per joint and one for the tip, on the flat.
    rope: Option<Spine>,
    /// The rope lying straight back from the hips, rebuilt each step.
    straight: Vec<Vec3>,
    /// The rope in the creature's frame, metres, with each joint's own
    /// height from the model: what `pose_limb` lays the bones along.
    pub local: Vec<Vec3>,
}

impl Tail {
    pub fn new(rig: &Rig) -> Option<Self> {
        let ch = dragged_tail(rig)?;
        let joint = |k: usize| rig.bones[ch.bones[k.min(ch.n() - 1)]].joint;
        let flat = Chain {
            bones: ch.bones.clone(),
            d: (0..ch.n())
                .map(|k| {
                    let v = if k + 1 < ch.n() { joint(k + 1) - joint(k) } else { joint(k) - joint(k.saturating_sub(1)) };
                    Vec2::new(v.x, v.z).length().max(0.1)
                })
                .collect(),
        };
        Some(Self {
            ch,
            flat,
            rope: None,
            straight: Vec::new(),
            local: Vec::new(),
        })
    }

    /// Drag the tail after a body standing at `pos` facing `yaw`. `vox` is
    /// metres per model voxel at this animal's size.
    pub fn step(&mut self, rig: &Rig, vox: f32, pivot: Vec3, pos: Vec3, yaw: f32, dt: f32) {
        let yq = Quat::from_rotation_y(yaw);
        let n = self.ch.n();
        let joint = |k: usize| rig.bones[self.ch.bones[k]].joint;
        let root = pos + yq * ((joint(0) - pivot) * vox);
        let root = Vec3::new(root.x, 0.0, root.z);
        // The way it lies at rest: straight back (+Z in the model).
        let back = yq * Vec3::Z;
        let mut p = root;
        self.straight.clear();
        self.straight.push(p);
        for k in 0..n {
            p += back * self.flat.seg(k, vox);
            self.straight.push(p);
        }
        let straight = &self.straight;
        let rope = self.rope.get_or_insert_with(|| Spine { pts: straight.clone(), phase: 0.0 });
        // A body that jumped (spawned, carried, teleported) starts straight.
        if rope.pts.len() != n + 1 || rope.head().distance(root) > self.flat.length(vox) {
            rope.pts.clone_from(straight);
        }
        rope.ease(straight, dt * STRAIGHTEN);
        rope.lead(&self.flat, vox, root);
        // Into the creature's frame, bending no joint past MAX_BEND.
        let inv = yq.inverse();
        let mut dir = Vec2::new(0.0, 1.0);
        let mut at = Vec2::ZERO;
        self.local.clear();
        self.local.push(Vec3::new(0.0, 0.0, 0.0));
        for k in 0..n {
            let d = inv * (rope.pts[k + 1] - rope.pts[k]);
            let want = Vec2::new(d.x, d.z).normalize_or(dir);
            let turn = dir.perp_dot(want).atan2(dir.dot(want)).clamp(-MAX_BEND, MAX_BEND);
            dir = Vec2::from_angle(turn).rotate(dir);
            at += dir * self.flat.seg(k, vox);
            let y = if k + 1 < n { joint(k + 1).y - joint(0).y } else { 2.0 * joint(k).y - joint(k - 1).y - joint(0).y };
            self.local.push(Vec3::new(at.x, y * vox, at.y));
        }
        // Write the clamped shape back, so the rope remembers it.
        for k in 0..=n {
            let l = self.local[k];
            rope.pts[k] = root + yq * Vec3::new(l.x, 0.0, l.z);
        }
    }

    /// Lay the tail along its rope, weighted by `w` against what the pose
    /// already holds.
    pub fn apply(&self, rig: &Rig, pose: &mut Pose, w: f32) {
        if w <= 0.0 || self.local.len() != self.ch.n() + 1 {
            return;
        }
        let parent = rig.bones[self.ch.bones[0]].parent.map_or(Quat::IDENTITY, |p| super::bone_rot(pose.bones[p]));
        // `pose_limb` writes the chain's bones and nothing else.
        let held: Vec<Vec3> = self.ch.bones.iter().map(|&b| pose.bones[b]).collect();
        spine::pose_limb(rig, &self.ch, parent, &self.local, pose);
        for (&b, was) in self.ch.bones.iter().zip(held) {
            pose.bones[b] = was.lerp(pose.bones[b], w.min(1.0));
        }
    }
}

/// A long neck swinging out to the side.
#[derive(Clone, Debug)]
pub struct Neck {
    bones: Vec<usize>,
    /// Yaw now and wanted, radians over the whole neck.
    swing: f32,
    want: f32,
    timer: f32,
}

impl Neck {
    pub fn new(rig: &Rig) -> Option<Self> {
        let ch = flexible(rig, Role::Neck)?;
        Some(Self { bones: ch.bones, swing: 0.0, want: 0.0, timer: 0.0 })
    }

    /// `graze` is how far the head is down feeding, `turn` the body's turn
    /// rate (radians a second, the way yaw grows), `walk` how far it is
    /// walking, and `r` a fresh random number in 0..1.
    pub fn step(&mut self, dt: f32, graze: f32, turn: f32, walk: f32, r: f32) {
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.timer = 3.0 + r * 5.0;
            // Browsing: well out to one side or the other, or straight ahead.
            let s = (r * 7.31).fract();
            self.want = if s < 0.25 { 0.0 } else { (s - 0.625) * 2.8 };
        }
        let lead = (turn * 0.9).clamp(-0.7, 0.7);
        let target = self.want * graze.max(1.0 - walk) * (1.0 - walk * 0.7) + lead * walk;
        self.swing += (target - self.swing) * (dt * 0.8).min(1.0);
    }

    /// Bend the neck by the swing, more toward the head.
    pub fn apply(&self, pose: &mut Pose) {
        let total: f32 = (1..=self.bones.len()).map(|k| k as f32).sum();
        for (k, &b) in self.bones.iter().enumerate() {
            pose.bones[b].y += self.swing * (k as f32 + 1.0) / total;
        }
    }
}
