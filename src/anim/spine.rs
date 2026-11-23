//! Serpents: a body posed along a path through the world.
//!
//! A snake's rig is one chain of bones, the head at the root and the tail tip
//! at the far end. Nothing swings: the motion is the path. The mover leads the
//! head, every point behind it keeps its own distance from the one ahead, so
//! the body follows where the head has been, and the points are let down onto
//! the ground, which is what puts a snake over a rock instead of through it. A
//! travelling sine laid across that path is the sinuous crawl; a coil, a
//! branch and a swim are paths of their own.
//!
//! The chain is anchored at the head, so the head always lands exactly where
//! the mover put it and the body accumulates behind it. Where a waved path's
//! points are further apart than the bones are long the body falls a little
//! short of the tail end of the path, which is cheaper to accept than to fix.

use super::{Pose, Rig};
use glam::{EulerRot, Quat, Vec3};
use std::f32::consts::TAU;

/// The bones of a serpent rig in order, head first, with the distance between
/// each pair of joints in model voxels.
#[derive(Clone, Debug)]
pub struct Chain {
    pub bones: Vec<usize>,
    /// `d[i]`: joint `i` to joint `i + 1`, in model voxels.
    pub d: Vec<f32>,
}

/// The chain of a rig whose bones form one line from the root.
pub fn chain(rig: &Rig) -> Chain {
    let mut bones = vec![0usize];
    while let Some(next) = rig.bones.iter().position(|b| b.parent == Some(*bones.last().unwrap())) {
        bones.push(next);
        if bones.len() > rig.bones.len() {
            break;
        }
    }
    let d = bones.windows(2).map(|w| (rig.bones[w[1]].joint - rig.bones[w[0]].joint).length()).collect();
    Chain { bones, d }
}

impl Chain {
    pub fn n(&self) -> usize {
        self.bones.len()
    }

    /// Metres from joint `i` to joint `i + 1` at this voxel size.
    pub fn seg(&self, i: usize, vox: f32) -> f32 {
        self.d.get(i).copied().unwrap_or(0.0) * vox
    }

    /// Nose to tail, metres.
    pub fn length(&self, vox: f32) -> f32 {
        self.d.iter().sum::<f32>() * vox
    }
}

/// A travelling wave down a body: how far each joint turns (radians, about a
/// third of a radian for a crawl), the wavelength in metres, and the phase in
/// cycles. The amplitude grows toward the tail, as a snake's does.
///
/// The wave is in the joints rather than in the path on purpose. Offsetting
/// the path sideways by anything near a bone's own length folds it back on
/// itself wherever the path is already curved — a snake crawling out of its
/// own coil — and a folded path swings the parts about wildly. A bend per
/// joint cannot do that, whatever the path is doing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wave {
    pub amp: f32,
    pub lambda: f32,
    pub phase: f32,
}

/// The path a serpent's body lies along: one world point per bone, head first.
#[derive(Clone, Debug)]
pub struct Spine {
    pub pts: Vec<Vec3>,
    /// Phase of the travelling wave, in cycles; the mover advances it by the
    /// distance travelled so the wave stays tied to the ground.
    pub phase: f32,
}

impl Spine {
    /// Stretched straight out behind a head at `head` facing `yaw` (the model
    /// faces -Z, so the body runs the other way).
    pub fn straight(ch: &Chain, vox: f32, head: Vec3, yaw: f32) -> Self {
        let back = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let mut pts = Vec::with_capacity(ch.n());
        let mut p = head;
        pts.push(p);
        for i in 0..ch.n().saturating_sub(1) {
            p += back * ch.seg(i, vox);
            pts.push(p);
        }
        Self { pts, phase: 0.0 }
    }

    /// Where the head is now.
    pub fn head(&self) -> Vec3 {
        self.pts.first().copied().unwrap_or(Vec3::ZERO)
    }

    /// Lead the head to `head`: every point behind it is pulled up to its own
    /// distance from the one ahead, so the body follows the head's track.
    pub fn lead(&mut self, ch: &Chain, vox: f32, head: Vec3) {
        if self.pts.is_empty() {
            return;
        }
        self.pts[0] = head;
        for i in 1..self.pts.len() {
            let len = ch.seg(i - 1, vox);
            let d = self.pts[i] - self.pts[i - 1];
            let dir = if d.length_squared() < 1e-8 { Vec3::Z } else { d.normalize() };
            self.pts[i] = self.pts[i - 1] + dir * len;
        }
    }

    /// Put every point back at its own distance from the one ahead, without
    /// moving the head. Letting the body down onto the ground, and easing it
    /// toward a shape, both stretch it a little; this takes that back out.
    pub fn tighten(&mut self, ch: &Chain, vox: f32) {
        let head = self.head();
        self.lead(ch, vox, head);
    }

    /// Let the body down onto the ground: each point eases toward the surface
    /// under it plus `lift`. Easing rather than snapping keeps the body from
    /// flicking as it crosses a voxel step.
    pub fn settle(&mut self, ground: &dyn Fn(f32, f32) -> Option<f32>, lift: f32, k: f32) {
        for p in &mut self.pts {
            if let Some(g) = ground(p.x, p.z) {
                p.y += (g + lift - p.y) * k.clamp(0.0, 1.0);
            }
        }
    }

    /// Ease the whole body toward another path (a coil, a branch): used where
    /// the shape is given rather than followed.
    pub fn ease(&mut self, target: &[Vec3], k: f32) {
        let k = k.clamp(0.0, 1.0);
        for (p, t) in self.pts.iter_mut().zip(target) {
            *p = p.lerp(*t, k);
        }
    }

    /// The pose that draws the body along its path, with the wave running
    /// down it, for a creature drawn at `at` with yaw `yaw`.
    #[allow(clippy::too_many_arguments)]
    pub fn pose(&self, rig: &Rig, ch: &Chain, vox: f32, pivot: Vec3, at: Vec3, yaw: f32, w: &Wave) -> Pose {
        let mut pose = pose_along(rig, ch, vox, pivot, at, yaw, &self.pts);
        add_wave(&mut pose, ch, vox, w);
        pose
    }
}

/// Lay a travelling wave down a posed chain: each joint turns by a sine of
/// how far along the body it is, growing toward the tail.
pub fn add_wave(pose: &mut Pose, ch: &Chain, vox: f32, w: &Wave) {
    if w.amp.abs() < 1e-5 || w.lambda <= 1e-4 {
        return;
    }
    let total = ch.length(vox).max(1e-4);
    let mut s = 0.0;
    for (k, &b) in ch.bones.iter().enumerate() {
        if k > 0 {
            s += ch.seg(k - 1, vox);
        }
        let grow = 0.35 + 0.65 * (s / total);
        if let Some(e) = pose.bones.get_mut(b) {
            e.y += w.amp * grow * (TAU * (s / w.lambda + w.phase)).sin();
        }
    }
}

/// The pose that lays a chain rig along `pts` (one point per chain bone, head
/// first). Bones outside the chain are left at rest.
#[allow(clippy::too_many_arguments)]
pub fn pose_along(rig: &Rig, ch: &Chain, vox: f32, pivot: Vec3, at: Vec3, yaw: f32, pts: &[Vec3]) -> Pose {
    let mut pose = Pose::rest(rig.bones.len());
    if ch.n() < 2 || pts.len() < ch.n() {
        return pose;
    }
    // Into the creature's own frame: the shader turns the whole body by `yaw`.
    let inv = Quat::from_rotation_y(-yaw);
    let local: Vec<Vec3> = pts.iter().map(|p| inv * (*p - at)).collect();
    // A bone's rest offset to its child runs down +Z (the head is at the
    // front, -Z), so each bone's rotation is the one that turns +Z onto the
    // step from its own point to the next.
    let mut global = Vec::with_capacity(ch.n());
    for i in 0..ch.n() - 1 {
        let d = local[i + 1] - local[i];
        let u = if d.length_squared() < 1e-8 { Vec3::Z } else { d.normalize() };
        global.push(Quat::from_euler(EulerRot::YXZ, u.x.atan2(u.z), -u.y.clamp(-1.0, 1.0).asin(), 0.0));
    }
    // The tail tip carries on as the bone before it.
    global.push(*global.last().unwrap());
    for (k, &b) in ch.bones.iter().enumerate() {
        let q = match k {
            0 => global[0],
            _ => global[k - 1].inverse() * global[k],
        };
        let (y, x, z) = q.to_euler(EulerRot::YXZ);
        pose.bones[b] = Vec3::new(x, y, z);
    }
    // The root is placed so the head lands on the first point.
    let root = rig.bones[ch.bones[0]].joint;
    pose.root = local[0] - (root - pivot) * vox;
    pose
}

/// Lay one limb of a rig along `pts`, writing its bones into `pose`. The
/// chain `ch` hangs off a parent turned by `parent_rot` in the creature's
/// frame, and `pts` holds one point per bone and one more for the tip, in the
/// creature's frame, the first on the limb's own joint. Unlike `pose_along`
/// the limb need not run down +Z at rest: each bone is turned from the way it
/// points in the model onto the way the path runs. An octopus's arms are
/// limbs like this, one per arm.
pub fn pose_limb(rig: &Rig, ch: &Chain, parent_rot: Quat, pts: &[Vec3], pose: &mut Pose) {
    let n = ch.n();
    if n == 0 || pts.len() < n + 1 {
        return;
    }
    // Which way each bone points at rest; the last one carries on as the
    // bone before it, having no joint beyond it to point at.
    let joint = |k: usize| rig.bones[ch.bones[k]].joint;
    let rest: Vec<Vec3> = (0..n)
        .map(|k| {
            let d = if k + 1 < n {
                joint(k + 1) - joint(k)
            } else if n > 1 {
                joint(k) - joint(k - 1)
            } else {
                Vec3::Z
            };
            d.normalize_or(Vec3::Z)
        })
        .collect();
    let mut parent = parent_rot;
    for k in 0..n {
        let d = pts[k + 1] - pts[k];
        let u = if d.length_squared() < 1e-8 { parent * rest[k] } else { d.normalize() };
        let global = Quat::from_rotation_arc(rest[k], u);
        let (y, x, z) = (parent.inverse() * global).to_euler(EulerRot::YXZ);
        if let Some(e) = pose.bones.get_mut(ch.bones[k]) {
            *e = Vec3::new(x, y, z);
        }
        parent = global;
    }
}

/// Take the turn out of a new pose: every angle is moved by whole turns until
/// it is within half a turn of the pose before it. Euler angles come back from
/// a rotation wrapped to +-PI, so a snake whose heading crosses south would
/// otherwise have its head spin a full turn between two steps — which the
/// interpolation between fixed steps would draw.
pub fn continue_from(pose: &mut Pose, prev: &Pose) {
    for (e, p) in pose.bones.iter_mut().zip(&prev.bones) {
        e.x = p.x + crate::util::wrap_angle(e.x - p.x);
        e.y = p.y + crate::util::wrap_angle(e.y - p.y);
        e.z = p.z + crate::util::wrap_angle(e.z - p.z);
    }
}

/// How fast a joint may turn, radians a second. A path can ask for more than
/// a body can give: a snake crawling out of its own coil drags a hairpin down
/// its length, and the joint it is passing would turn half a radian in a
/// thirtieth of a second. Holding every joint to this rate costs a little lag
/// where the path bends hardest, which reads as a body with weight in it.
pub const JOINT_RATE: f32 = 6.0;

/// Hold a new pose to `JOINT_RATE` from the pose before it.
pub fn ease_joints(pose: &mut Pose, prev: &Pose, dt: f32) {
    let max = JOINT_RATE * dt;
    for (e, p) in pose.bones.iter_mut().zip(&prev.bones) {
        *e = *p + (*e - *p).clamp(Vec3::splat(-max), Vec3::splat(max));
    }
}

/// A snake coiled on itself: the body wound in a ring that rises a little as
/// it goes round, and the head lifted over the middle of it facing `yaw`.
/// `rise` is how high the head is held, metres.
///
/// The ring is never wound tighter than the bones are long. A loop of a
/// smaller radius than one segment puts a hairpin at every joint, and the
/// kinks that travel down the body as it unwinds are then visible as jumps.
pub fn coil(ch: &Chain, vox: f32, centre: Vec3, yaw: f32, rise: f32) -> Vec<Vec3> {
    let n = ch.n();
    let mut pts = vec![centre; n];
    if n < 4 {
        return pts;
    }
    let body: f32 = (0..n - 1).map(|i| ch.seg(i, vox)).sum();
    let seg = (0..n - 1).map(|i| ch.seg(i, vox)).fold(0.0, f32::max);
    // One open loop, about a body long round, and never wound tighter than
    // the bones are long: a smaller ring puts a hairpin at every joint, and
    // the kinks that travel down the body as it unwinds show as jumps.
    let r = (seg / 0.5).max(body / (1.5 * TAU));
    let climb = seg * 0.35;
    let mut theta = 0.0f32;
    for i in (0..n).rev() {
        pts[i] = centre + Vec3::new(theta.sin() * r, climb * theta / TAU, theta.cos() * r);
        if i > 0 {
            theta += ch.seg(i - 1, vox) / r;
        }
    }
    // The neck comes up off the ring and the head rides over it, looking out.
    let facing = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let out = (pts[2] - centre).normalize_or_zero();
    let lift = (rise / ch.seg(1, vox).max(1e-3)).clamp(0.2, 0.9);
    pts[1] = pts[2] + (Vec3::Y * lift - out * 0.3).normalize() * ch.seg(1, vox);
    pts[0] = pts[1] + (facing * 1.0 + Vec3::Y * 0.12).normalize() * ch.seg(0, vox);
    pts
}

/// A snake draped over a branch: the body behind `anchor` lying along the
/// branch toward `along`, and the front `fall` bones curving off the end of
/// it and hanging, the way a snake in a tree spends its day. `sway` swings
/// the hanging part along the branch, metres.
pub fn drape(ch: &Chain, vox: f32, anchor: Vec3, along: Vec3, fall: usize, sway: f32) -> Vec<Vec3> {
    let n = ch.n();
    let mut pts = vec![anchor; n];
    if n < 2 {
        return pts;
    }
    let along = if along.length_squared() < 1e-8 { Vec3::Z } else { along.normalize() };
    let fall = fall.clamp(1, n - 1);
    // Back from the anchor along the branch.
    let mut p = anchor;
    for (i, slot) in pts.iter_mut().enumerate().skip(fall) {
        *slot = p;
        p += along * ch.seg(i.min(n - 2), vox);
    }
    // Forward from the anchor, bending over the end of the branch: the
    // heading turns from along the branch to straight down as it goes, so
    // the body hangs in a hook rather than a right angle.
    let out = -along;
    let side = Vec3::new(out.z, 0.0, -out.x).normalize_or_zero();
    let mut p = anchor;
    for i in (0..fall).rev() {
        let t = (fall - i) as f32 / fall as f32;
        let a = std::f32::consts::FRAC_PI_2 * t.powf(0.8);
        let dir = (out * a.cos() - Vec3::Y * a.sin()).normalize_or_zero();
        p += dir * ch.seg(i, vox) + side * sway * t / fall as f32;
        pts[i] = p;
    }
    pts
}

/// Does this rig's bones form one chain, as a serpent's must?
pub fn is_chain(rig: &Rig) -> bool {
    rig.bones.len() > 1 && rig.bones.iter().enumerate().skip(1).all(|(i, b)| b.parent == Some(i - 1))
}

/// Where each bone of a chain rig is actually drawn: the pose read back, for
/// tests and for anything that needs a point on the body.
#[allow(dead_code)]
pub fn joints(rig: &Rig, pose: &Pose, vox: f32, pivot: Vec3, at: Vec3, yaw: f32) -> Vec<Vec3> {
    let mut fk = Vec::new();
    super::forward(rig, pose, vox, pivot, &mut fk);
    let yq = Quat::from_rotation_y(yaw);
    fk.iter().map(|(p, _)| at + yq * *p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Bone, Plan, Rig, Role};
    use std::collections::HashMap;

    /// A ten-bone snake, head at z = 3, tail at z = 30, two voxels a segment.
    fn snake() -> (Rig, Chain) {
        let n = 10;
        let bones = (0..n)
            .map(|i| Bone {
                name: format!("b{i}"),
                role: if i == 0 {
                    Role::Head
                } else if i == n - 1 {
                    Role::Tail
                } else {
                    Role::Spine
                },
                parent: (i > 0).then(|| i - 1),
                joint: Vec3::new(16.0, 1.0, 3.0 + 3.0 * i as f32),
                model: 0,
                side: 0.0,
                pair: 0,
                spread: None,
            })
            .collect();
        let rig = Rig {
            soft: None,
            plan: Some(Plan::Serpent),
            bones,
            activities: HashMap::new(),
            floor: 0.0,
        };
        let ch = chain(&rig);
        (rig, ch)
    }

    #[test]
    fn the_chain_is_read_head_first() {
        let (rig, ch) = snake();
        assert_eq!(ch.bones, (0..rig.bones.len()).collect::<Vec<_>>());
        assert_eq!(ch.d.len(), rig.bones.len() - 1);
        assert!((ch.length(0.05) - 27.0 * 0.05).abs() < 1e-5);
        assert!(is_chain(&rig));
    }

    /// Every bone is drawn on the point the path put it on, however the path
    /// bends and whatever the creature's yaw: this is what makes a snake lie
    /// along the ground instead of through it.
    #[test]
    fn bones_land_on_the_path() {
        let (rig, ch) = snake();
        let vox = 0.05;
        let at = Vec3::new(100.0, 12.0, -40.0);
        for &yaw in &[0.0, 1.1, -2.4] {
            // A path that turns and climbs.
            let mut pts = vec![at];
            for i in 1..ch.n() {
                let a = i as f32 * 0.35;
                let step = Vec3::new(a.sin(), 0.12 * (i as f32 * 0.5).cos(), a.cos()).normalize();
                pts.push(pts[i - 1] + step * ch.seg(i - 1, vox));
            }
            let pose = pose_along(&rig, &ch, vox, Vec3::new(16.0, 0.0, 16.0), at, yaw, &pts);
            let drawn = joints(&rig, &pose, vox, Vec3::new(16.0, 0.0, 16.0), at, yaw);
            for (i, (a, b)) in drawn.iter().zip(&pts).enumerate() {
                assert!((*a - *b).length() < 1e-3, "bone {i} drawn at {a} for path point {b} (yaw {yaw})");
            }
        }
    }

    /// A limb that leaves its parent sideways is drawn on its path too, and
    /// whatever the parent's own turn: the octopus's arms.
    #[test]
    fn a_limb_lands_on_its_path() {
        let b = |name: &str, parent: Option<usize>, joint: [f32; 3]| Bone {
            name: name.into(),
            role: if parent.is_none() { Role::Root } else { Role::Arm },
            parent,
            joint: Vec3::from(joint),
            model: 0,
            side: 0.0,
            pair: 0,
            spread: None,
        };
        let rig = Rig {
            soft: None,
            plan: Some(Plan::Octopus),
            activities: HashMap::new(),
            floor: 0.0,
            bones: vec![
                b("body", None, [16.0, 2.0, 16.0]),
                b("a0", Some(0), [18.0, 1.0, 16.0]),
                b("a1", Some(1), [21.0, 1.0, 16.0]),
                b("a2", Some(2), [24.0, 1.0, 16.0]),
            ],
        };
        let ch = Chain {
            bones: vec![1, 2, 3],
            d: vec![3.0, 3.0, 3.0],
        };
        let (vox, pivot) = (0.1, Vec3::new(16.0, 0.0, 16.0));
        for &(pitch, yaw) in &[(0.0, 0.0), (0.4, 1.3), (-0.3, -2.6)] {
            let mut pose = Pose::rest(4);
            pose.bones[0] = Vec3::new(pitch, yaw, 0.0);
            let rq = crate::anim::bone_rot(pose.bones[0]);
            let root = (rig.bones[0].joint - pivot) * vox;
            // The path starts on the limb's joint wherever the body has
            // turned it, then curls down and round.
            let mut pts = vec![root + rq * (rig.bones[1].joint - rig.bones[0].joint) * vox];
            for i in 0..3 {
                let a = 0.6 * i as f32;
                pts.push(pts[i] + Vec3::new(a.cos(), -0.3, a.sin()).normalize() * 0.3);
            }
            pose_limb(&rig, &ch, rq, &pts, &mut pose);
            let drawn = joints(&rig, &pose, vox, pivot, Vec3::ZERO, 0.0);
            for k in 0..3 {
                assert!((drawn[k + 1] - pts[k]).length() < 1e-3, "joint {k} drawn at {} for {} (pitch {pitch}, yaw {yaw})", drawn[k + 1], pts[k]);
            }
        }
    }

    /// Leading the head round a circle keeps the body in one piece: every
    /// segment stays its own length.
    #[test]
    fn leading_the_head_keeps_the_body_together() {
        let (_, ch) = snake();
        let vox = 0.05;
        let mut s = Spine::straight(&ch, vox, Vec3::ZERO, 0.0);
        for k in 0..400 {
            let a = k as f32 * 0.04;
            s.lead(&ch, vox, Vec3::new(a.sin() * 1.5, 0.0, a.cos() * 1.5));
        }
        for i in 1..s.pts.len() {
            let d = (s.pts[i] - s.pts[i - 1]).length();
            assert!((d - ch.seg(i - 1, vox)).abs() < 1e-4, "segment {i} is {d} m, not {}", ch.seg(i - 1, vox));
        }
        // And the body trails behind the head, not on top of it.
        assert!((s.pts[0] - *s.pts.last().unwrap()).length() > ch.length(vox) * 0.3);
    }

    /// A coiled snake is a tight heap: everything within a couple of body
    /// widths of the middle, and the head up off the ground.
    #[test]
    fn a_coil_is_tight_and_the_head_is_up() {
        let (_, ch) = snake();
        let vox = 0.05;
        let centre = Vec3::new(5.0, 2.0, -3.0);
        let pts = coil(&ch, vox, centre, 0.7, 0.18);
        let len = ch.length(vox);
        for (i, p) in pts.iter().enumerate() {
            let r = Vec3::new(p.x - centre.x, 0.0, p.z - centre.z).length();
            assert!(r < len * 0.35, "point {i} is {r} m out of a {len} m snake");
        }
        assert!(pts[0].y > centre.y + 0.1, "the head is not raised: {:?}", pts[0]);
        assert!(pts[0].y > pts[1].y && pts[1].y > pts[2].y, "the neck does not rise to the head");
    }

    /// A draped snake lies along the branch behind the anchor and hangs below
    /// it in front, and the hanging part never rises above the branch.
    #[test]
    fn a_drape_hangs_below_its_branch() {
        let (_, ch) = snake();
        let vox = 0.05;
        let anchor = Vec3::new(0.0, 6.0, 0.0);
        let pts = drape(&ch, vox, anchor, Vec3::Z, 4, 0.03);
        assert!(pts[0].y < anchor.y - ch.seg(0, vox), "the head does not hang: {:?}", pts[0]);
        for (i, p) in pts.iter().enumerate() {
            assert!(p.y <= anchor.y + 1e-4, "point {i} is above the branch");
        }
        // Behind the anchor it follows the branch.
        for (i, p) in pts.iter().enumerate().skip(5) {
            assert!((p.y - anchor.y).abs() < 1e-4, "point {i} left the branch");
        }
    }

    /// The wave bends the joints, side to side, and travels down the body.
    #[test]
    fn the_wave_bends_the_joints_and_travels() {
        let (rig, ch) = snake();
        let vox = 0.05;
        let s = Spine::straight(&ch, vox, Vec3::ZERO, 0.0);
        let at = |phase: f32| {
            s.pose(
                &rig,
                &ch,
                vox,
                Vec3::new(16.0, 0.0, 16.0),
                Vec3::ZERO,
                0.0,
                &Wave {
                    amp: 0.3,
                    lambda: ch.length(vox) * 0.6,
                    phase,
                },
            )
        };
        let (a, b) = (at(0.0), at(0.25));
        assert_ne!(a.bones, b.bones, "the wave did not travel");
        // Side to side, and never more than the amplitude at a joint.
        for (i, e) in a.bones.iter().enumerate() {
            assert!(e.y.abs() <= 0.31, "joint {i} turned {} rad", e.y);
            assert!(e.x.abs() < 1e-4 && e.z.abs() < 1e-4, "joint {i} bent out of the horizontal: {e:?}");
        }
        // Both ways along the body: a wave, not a turn.
        assert!(a.bones.iter().any(|e| e.y > 0.05) && a.bones.iter().any(|e| e.y < -0.05), "the body only bent one way: {:?}", a.bones);
    }
}
