//! A body with nothing holding it up (ANIMATION.md "Limp bodies").
//!
//! Every part of the animal is a rigid cluster of points: its own joint, the
//! joints of the parts that hang off it, and a few voxels on its outside
//! (`Bone::hull`). Each step the points fall, hit the ground and slide on
//! their own; then, a few times over, every cluster is put back into its own
//! shape, turned to fit where its points have got to, and no joint is left
//! bent further than it goes. Neighbouring clusters share the joint where
//! they meet, and that is all that holds the animal together. It is the
//! shooter's corpse (shooter/corpse.rs) with rigid parts in place of sticks,
//! so each part comes out as a turn and the body is drawn through the same
//! `Pose` as a living animal.
//!
//! Nothing is faked about the floor. A body is down when its points are on
//! the ground, a leg rests where the ground stopped it, and a body hung by
//! the scruff from a mouth hangs below it, because that is where its points
//! fall to.
//!
//! All of it happens in the animal's own frame: x and z turned with the yaw
//! it had when it went limp, y up, the origin where its feet were
//! (`Look::pos`). The caller says where the ground is in that frame, and
//! moves the frame (`shift`) if it wants `Look::pos` to follow the body.

use super::{forward, Plan, Pose, Rig, Role};
use crate::world::noise::Rng;
use glam::{EulerRot, Mat3, Quat, Vec3};

/// The step a settle is run at, seconds.
pub const STEP: f32 = 1.0 / 30.0;
const GRAVITY: f32 = 9.8;
/// Of its speed, what a point keeps over a second in the air. Air is not
/// what slows a body down; this is the give in the joints, which keeps a
/// hanging carcass from swinging for ever.
const AIR_KEEP: f32 = 0.35;
/// Of its movement along the ground, what a point touching it keeps each
/// step: fur on grass grips.
const FLOOR_KEEP: f32 = 0.45;
/// A point on the ground that has moved less than this along it in a step
/// (metres, at `STEP`) does not move at all. Without it, a body lying still
/// creeps: lifting a point out of the floor and putting its part back into
/// shape turns the part a hair each time, always the same way, and a dead
/// deer turned slowly round on the grass for as long as anyone watched.
const GRIP_M: f32 = 0.012;
/// Times each step every part is put back into shape. Fewer and a long
/// neck stretches as it is dragged.
const PASSES: usize = 8;
/// How much heavier than any other point a held one is, so the parts
/// round it move to it and not it to them.
const HELD: f32 = 60.0;
/// How much heavier a point on the ground is while the parts are put back
/// into shape: the ground is pushing back on it. Without it, putting a part
/// back into shape pulled its points on the floor back into the floor, and
/// a body pressed on one part by the parts either side settled a few
/// centimetres into the ground.
const ON_GROUND: f32 = 8.0;
/// Below this many metres a step, a point is still.
const STILL: f32 = 0.0012;
/// Steps of everything being still before the body counts as settled.
const QUIET: u32 = 10;
/// Steps a settle may take before it is called done.
const MOST_STEPS: u32 = 900;
/// Passes made over a body that has come to rest (`Limp::polish`).
const POLISH: usize = 40;

/// How far a joint of each role bends from where it stands at rest, in any
/// direction, radians.
fn limit(role: Role) -> f32 {
    match role {
        Role::Leg => 1.5,
        Role::Arm => 1.7,
        Role::Neck => 1.2,
        Role::Head => 0.9,
        Role::Tail => 1.2,
        // A serpent's body bends freely but not into a knot.
        Role::Spine => 0.6,
        Role::Wing => 1.5,
        Role::Ear => 0.6,
        Role::Jaw => 0.35,
        Role::Root => std::f32::consts::PI,
    }
}

/// A rigid part of the body: the points it moves and where each sits in it.
struct Part {
    /// The part it hangs from, if any.
    parent: Option<usize>,
    /// What its bone is, for choosing where to take hold of it.
    role: Role,
    lim: f32,
    pts: Vec<usize>,
    /// Each point's place in the part at rest, from the part's own centre,
    /// metres.
    rest: Vec<Vec3>,
    /// Which way it is turned from rest.
    rot: Quat,
}

/// Where a body is taken hold of. Whatever is held, the rest of the body
/// hangs from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grip {
    /// The back of the neck, as a cat carries a kitten: the joint between
    /// neck and head, or the head's own if there is no neck.
    Scruff,
    /// The middle of the back.
    Back,
    /// Any one point of the body, joint or outside, by its number: wherever
    /// a mouth closed on it (`Limp::nearest` finds one).
    Point(usize),
}

pub struct Limp {
    pts: Vec<Vec3>,
    prev: Vec<Vec3>,
    /// How heavy each point is, 1 unless something has hold of it.
    weight: Vec<f32>,
    /// The ground under each point this step, plus its skin.
    floor: Vec<f32>,
    parts: Vec<Part>,
    /// The part that turns each bone: its own, or for a rigid bone the part
    /// of what it hangs from.
    part_of: Vec<usize>,
    /// Each bone's joint as a point.
    joint_pt: Vec<usize>,
    /// The root joint's place at rest, metres from the model pivot.
    root_rest: Vec3,
    /// A point is the middle of a voxel and the voxel has half its width
    /// round it.
    skin: f32,
    /// A point something has hold of and where it is held, in the frame.
    held: Option<(usize, Vec3)>,
    /// Passes made, for which way round the next one goes.
    pass_no: u32,
    quiet: u32,
    /// Steps since it was last let go, woken or shoved.
    steps: u32,
    /// Steps left in which its feet slide rather than grip: an animal
    /// going over where it stood.
    slip: u32,
    settled: bool,
}

impl Limp {
    /// Limp from this moment, in the pose it was in. `voxel` is the model's
    /// voxel edge (times any morph scale), `pivot` its pivot in model voxels.
    pub fn new(rig: &Rig, voxel: f32, pivot: Vec3, pose: &Pose) -> Limp {
        let n = rig.bones.len();
        let parent = |b: usize| rig.bones[b].parent.filter(|&p| p < b);
        // Tucked joints are a shell's business; the body goes limp out of it.
        let mut pose = pose.clone();
        pose.tuck.clear();
        if pose.bones.len() != n {
            pose.bones.resize(n, Vec3::ZERO);
        }
        let mut fk = Vec::new();
        forward(rig, &pose, voxel, pivot, &mut fk);

        let mut pts = Vec::new();
        let mut rest_at = Vec::new();
        // Each bone's joint.
        let joint_pt: Vec<usize> = (0..n)
            .map(|b| {
                pts.push(fk[b].0);
                rest_at.push((rig.bones[b].joint - pivot) * voxel);
                pts.len() - 1
            })
            .collect();
        // Which part moves each bone.
        let mut part_of = vec![0usize; n];
        let mut owns = Vec::new();
        for b in 0..n {
            match parent(b) {
                Some(p) if rig.bones[b].rigid => part_of[b] = part_of[p],
                _ => {
                    part_of[b] = owns.len();
                    owns.push(b);
                }
            }
        }
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); owns.len()];
        for b in 0..n {
            let k = part_of[b];
            // Its joint is in its own part, and in its parent's: that is the
            // hinge.
            members[k].push(joint_pt[b]);
            if let Some(p) = parent(b) {
                if part_of[p] != k {
                    members[part_of[p]].push(joint_pt[b]);
                }
            }
            for h in &rig.bones[b].hull {
                pts.push(fk[b].0 + fk[b].1 * ((*h - rig.bones[b].joint) * voxel));
                rest_at.push((*h - pivot) * voxel);
                members[k].push(pts.len() - 1);
            }
        }
        // A part with nothing but its joint (a rig made in code, or a part
        // too thin to have an outside) is given a tip, carried on from its
        // parent's joint through its own, so it has a direction to turn.
        for (k, &b) in owns.iter().enumerate() {
            if members[k].len() > 1 {
                continue;
            }
            let from = parent(b).map_or(rig.bones[b].joint + Vec3::Y, |p| rig.bones[p].joint);
            let along = (rig.bones[b].joint - from).try_normalize().unwrap_or(Vec3::NEG_Y) * 2.0;
            pts.push(fk[b].0 + fk[b].1 * (along * voxel));
            rest_at.push((rig.bones[b].joint + along - pivot) * voxel);
            members[k].push(pts.len() - 1);
        }
        let parts = owns
            .iter()
            .zip(members)
            .map(|(&b, mut pts_k)| {
                pts_k.sort_unstable();
                pts_k.dedup();
                let c0 = pts_k.iter().map(|&i| rest_at[i]).sum::<Vec3>() / pts_k.len() as f32;
                Part {
                    parent: parent(b).map(|p| part_of[p]),
                    role: rig.bones[b].role,
                    lim: limit(rig.bones[b].role),
                    rest: pts_k.iter().map(|&i| rest_at[i] - c0).collect(),
                    pts: pts_k,
                    rot: fk[b].1,
                }
            })
            .collect();
        let m = pts.len();
        Limp {
            prev: pts.clone(),
            pts,
            weight: vec![1.0; m],
            floor: vec![f32::MIN; m],
            parts,
            part_of,
            joint_pt,
            root_rest: (rig.bones[0].joint - pivot) * voxel,
            skin: voxel * 0.5,
            held: None,
            pass_no: 0,
            quiet: 0,
            steps: 0,
            slip: 0,
            settled: false,
        }
    }

    /// Going over where it stood: a turn about its own length onto one side
    /// (or, for something that stands upright, any way at all), and a little
    /// of its own in every point so no two fall alike.
    pub fn topple(&mut self, rig: &Rig, seed: u32) {
        let mut rng = Rng::new(seed.wrapping_mul(0x9e37_79b9) | 1);
        let middle = self.middle();
        let serpent = rig.plan == Some(Plan::Serpent);
        // The body's long axis as it stands now.
        let along = self.parts[0].rot * Vec3::Z;
        let spin = if serpent {
            Vec3::ZERO
        } else if rig.plan == Some(Plan::Biped) {
            // A person goes over any way at all.
            let a = rng.unit() * std::f32::consts::TAU;
            Vec3::new(a.cos(), 0.0, a.sin()) * (2.0 + rng.unit())
        } else {
            // Hard enough that a broad animal goes right over rather than
            // folding down onto its belly with its legs out.
            let side = if seed & 1 == 0 { 1.0 } else { -1.0 };
            along * side * (5.0 + rng.unit() * 1.5)
        };
        for i in 0..self.pts.len() {
            let wobble = Vec3::new(rng.signed(), rng.signed() * 0.3, rng.signed()) * 0.4;
            let v = spin.cross(self.pts[i] - middle) + wobble;
            self.prev[i] = self.pts[i] - v * STEP;
        }
        self.wake();
        // Feet that held the ground would keep a big animal propped up on
        // splayed legs as it went down, and it ended on its belly. They
        // slide out from under it instead, for the half second of the fall.
        self.slip = (0.5 / STEP) as u32;
    }

    /// Thrown: the body set turning at up to `rate` radians a second, about
    /// an axis of its own. Whatever carries it through the world (a box
    /// thrown by a blast, flung.rs) does the rest.
    pub fn tumble(&mut self, rate: f32, seed: u32) {
        let mut rng = Rng::new(seed ^ 0x51ed | 1);
        let middle = self.middle();
        let spin = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or_zero() * rate.min(9.0);
        for i in 0..self.pts.len() {
            self.prev[i] -= spin.cross(self.pts[i] - middle) * STEP;
        }
        self.wake();
    }

    /// Take hold of it at `grip`, at `at` in the frame, or let it go. The
    /// held point goes where it is held and the rest of the body hangs from
    /// it, swinging as the hand moves, or lies along the ground behind it if
    /// the hand is lower than the body is long.
    pub fn hold(&mut self, rig: &Rig, grip: Option<(Grip, Vec3)>) {
        match grip {
            Some((g, at)) => self.hold_point(self.point(rig, g), at),
            None => {
                if let Some((i, _)) = self.held.take() {
                    self.weight[i] = 1.0;
                }
                self.wake();
            }
        }
    }

    /// Hold the joint of bone `b` at `at`.
    pub fn hold_bone(&mut self, b: usize, at: Vec3) {
        self.hold_point(self.joint_pt[b.min(self.joint_pt.len() - 1)], at);
    }

    /// Hold point `i` at `at`.
    fn hold_point(&mut self, i: usize, at: Vec3) {
        if let Some((j, _)) = self.held {
            self.weight[j] = 1.0;
        }
        self.weight[i] = HELD;
        self.held = Some((i, at));
        self.wake();
    }

    /// Which point `grip` is.
    fn point(&self, rig: &Rig, grip: Grip) -> usize {
        match grip {
            Grip::Point(i) => i.min(self.pts.len() - 1),
            g => self.joint_pt[grip_bone(rig, g)],
        }
    }

    /// The point of the body nearest `at` (frame metres) on a part of one of
    /// these roles, or on any part if none of them has one: where a mouth
    /// reaching for it at `at` closes.
    pub fn nearest(&self, at: Vec3, roles: &[Role]) -> Grip {
        let pick = |only: bool| {
            self.parts
                .iter()
                .filter(|p| !only || roles.contains(&p.role))
                .flat_map(|p| p.pts.iter().copied())
                .min_by(|&a, &b| self.pts[a].distance_squared(at).total_cmp(&self.pts[b].distance_squared(at)))
        };
        Grip::Point(pick(true).or_else(|| pick(false)).unwrap_or(0))
    }

    /// Where the point at `grip` is now, in the frame.
    pub fn grip_at(&self, rig: &Rig, grip: Grip) -> Vec3 {
        self.pts[self.point(rig, grip)]
    }

    /// A pull on it at `grip`: that point is given `dv` metres a second.
    pub fn tug(&mut self, rig: &Rig, grip: Grip, dv: Vec3) {
        let i = self.point(rig, grip);
        self.prev[i] -= dv * STEP;
        self.wake();
    }

    /// Move the frame by `by` (frame metres): the body stays where it is in
    /// the world, so every point is that much less in the new frame.
    pub fn shift(&mut self, by: Vec3) {
        for p in self.pts.iter_mut().chain(self.prev.iter_mut()) {
            *p -= by;
        }
        if let Some((_, at)) = self.held.as_mut() {
            *at -= by;
        }
    }

    pub fn wake(&mut self) {
        self.settled = false;
        self.quiet = 0;
        self.steps = 0;
    }

    pub fn settled(&self) -> bool {
        self.settled
    }

    /// The middle of the body, frame metres.
    pub fn middle(&self) -> Vec3 {
        self.pts.iter().copied().sum::<Vec3>() / self.pts.len().max(1) as f32
    }

    /// The lowest and highest any point of it is, frame metres.
    pub fn span_y(&self) -> (f32, f32) {
        self.pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)))
    }

    /// One step of `dt` seconds. `ground` is the height of the ground under
    /// a point, in the frame; `push` is any acceleration besides gravity on
    /// every point, metres a second squared (the frame's own, turned round,
    /// when the frame is a moving thing).
    pub fn step(&mut self, dt: f32, ground: &dyn Fn(Vec3) -> f32, push: Vec3) {
        if self.settled && self.held.is_none() && push.length_squared() < 0.01 {
            return;
        }
        let keep = AIR_KEEP.powf(dt);
        let acc = Vec3::new(0.0, -GRAVITY, 0.0) + push;
        for (i, w) in self.weight.iter_mut().enumerate() {
            if self.held.is_none_or(|(h, _)| h != i) {
                *w = 1.0;
            }
        }
        for i in 0..self.pts.len() {
            let p = self.pts[i];
            let v = (p - self.prev[i]) * keep;
            self.prev[i] = p;
            self.pts[i] = p + v + acc * dt * dt;
            self.floor[i] = ground(self.pts[i]) + self.skin;
        }
        for pass in 0..PASSES {
            self.pin();
            // The ground holds up what is on it, and holds it where it is
            // unless it is pulled hard enough to slide.
            let grip = if self.slip > 0 { 0.0 } else { GRIP_M * (dt / STEP) };
            for i in 0..self.pts.len() {
                let f = self.floor[i];
                let p = &mut self.pts[i];
                if p.y > f {
                    continue;
                }
                p.y = f;
                self.weight[i] = self.weight[i].max(ON_GROUND);
                let was = self.prev[i];
                let along = Vec3::new(p.x - was.x, 0.0, p.z - was.z);
                // Sliding drags once a step, not once a pass.
                let keep = match along.length() < grip {
                    true => 0.0,
                    false if pass + 1 == PASSES && self.slip == 0 => FLOOR_KEEP,
                    false => 1.0,
                };
                p.x = was.x + along.x * keep;
                p.z = was.z + along.z * keep;
            }
            // Then every part into shape and every joint within its reach,
            // last, so the turns the body is drawn with are the turns its
            // points are in.
            self.shape();
            self.pin();
        }
        // What is on the ground does not bounce.
        let mut most = 0.0f32;
        for i in 0..self.pts.len() {
            let p = self.pts[i];
            if p.y <= self.floor[i] + 1e-3 && p.y < self.prev[i].y {
                self.prev[i].y = p.y;
            }
            most = most.max((p - self.prev[i]).length());
        }
        let still = most < STILL * (dt / STEP).max(0.25);
        self.quiet = if still && self.held.is_none() { self.quiet + 1 } else { 0 };
        self.steps = if self.held.is_none() { self.steps + 1 } else { 0 };
        // A body left to itself long enough is done, whatever is still
        // trembling in it: a small bird's parts can shiver against each other
        // by a millimetre for ever.
        self.slip = self.slip.saturating_sub(1);
        let done = self.quiet >= QUIET || self.steps >= MOST_STEPS;
        if done && !self.settled {
            self.polish();
        }
        self.settled = done;
    }

    /// Step it where it lies until it is still.
    pub fn run_out(&mut self, ground: &dyn Fn(Vec3) -> f32) {
        for _ in 0..MOST_STEPS {
            if self.settled {
                break;
            }
            self.step(STEP, ground, Vec3::ZERO);
        }
    }

    /// Put a body that has come to rest exactly together: passes with
    /// nothing falling, until each hinge is where both parts sharing it
    /// have it. The passes of a step leave a hinge a little between the
    /// two, and the body is drawn from the turns of its parts, so a wing
    /// was drawn a few centimetres into the ground its points lay on.
    fn polish(&mut self) {
        for _ in 0..POLISH {
            for i in 0..self.pts.len() {
                let f = self.floor[i];
                if self.pts[i].y < f {
                    self.pts[i].y = f;
                    self.weight[i] = self.weight[i].max(ON_GROUND);
                }
            }
            self.shape();
            self.pin();
        }
        self.prev.clone_from(&self.pts);
    }

    /// The held point back where it is held.
    fn pin(&mut self) {
        if let Some((i, at)) = self.held {
            self.pts[i] = at;
        }
    }

    /// Every part back into its own shape, turned to fit its points best
    /// (shape matching): parents first, so a hinge is left where the
    /// part further out wants it.
    fn shape(&mut self) {
        let mut turns: Vec<Quat> = self.parts.iter().map(|p| p.rot).collect();
        // From the root out on one pass and from the tips in on the next, so
        // a hinge is not always left where the part further out wants it.
        let n_parts = self.parts.len();
        let back = self.pass_no % 2 == 1;
        self.pass_no += 1;
        for step in 0..n_parts {
            let n = if back { n_parts - 1 - step } else { step };
            let part = &mut self.parts[n];
            // The centre of its points as they are and as they sit at rest,
            // both weighted alike.
            let mut total = 0.0;
            let mut c = Vec3::ZERO;
            let mut r0 = Vec3::ZERO;
            for (k, &i) in part.pts.iter().enumerate() {
                let w = self.weight[i];
                c += self.pts[i] * w;
                r0 += part.rest[k] * w;
                total += w;
            }
            c /= total;
            r0 /= total;
            let mut a = Mat3::ZERO;
            for (k, &i) in part.pts.iter().enumerate() {
                let d = (self.pts[i] - c) * self.weight[i];
                let r = part.rest[k] - r0;
                a.x_axis += d * r.x;
                a.y_axis += d * r.y;
                a.z_axis += d * r.z;
            }
            part.rot = turn_of(a, part.rot);
            // No further than the joint bends from where it stands at rest,
            // measured from the part it hangs off, which has already been
            // put into shape this pass.
            if let Some(p) = part.parent {
                part.rot = within(turns[p], part.rot, part.lim);
            }
            turns[n] = part.rot;
            for (k, &i) in part.pts.iter().enumerate() {
                self.pts[i] = c + part.rot * (part.rest[k] - r0);
            }
        }
    }

    /// The pose this body is in, in the frame it was made in.
    pub fn pose(&self, rig: &Rig) -> Pose {
        let n = rig.bones.len();
        let mut pose = Pose::rest(n);
        for b in 0..n {
            let own = self.part_of[b];
            let r = self.parts[own].rot;
            let local = match rig.bones[b].parent.filter(|&p| p < b) {
                // A rigid bone turns with its parent and is drawn so.
                Some(_) if rig.bones[b].rigid => continue,
                Some(p) => self.parts[self.part_of[p]].rot.inverse() * r,
                None => r,
            };
            let (y, x, z) = local.to_euler(EulerRot::YXZ);
            pose.bones[b] = Vec3::new(x, y, z);
        }
        pose.root = self.pts[self.joint_pt[0]] - self.root_rest;
        pose
    }
}

/// `turn` brought within `lim` radians of `parent`'s turn.
fn within(parent: Quat, turn: Quat, lim: f32) -> Quat {
    let mut rel = parent.inverse() * turn;
    if rel.w < 0.0 {
        rel = -rel;
    }
    let angle = 2.0 * rel.w.clamp(-1.0, 1.0).acos();
    if angle <= lim {
        return turn;
    }
    (parent * Quat::IDENTITY.slerp(rel, lim / angle)).normalize()
}

/// The bone that is taken hold of at `grip`.
pub fn grip_bone(rig: &Rig, grip: Grip) -> usize {
    let find = |role: Role| rig.bones.iter().position(|b| b.role == role);
    match grip {
        Grip::Scruff => find(Role::Head).or_else(|| find(Role::Neck)).unwrap_or(0),
        Grip::Back | Grip::Point(_) => 0,
    }
}

/// The rotation nearest to the linear map `a`, starting from `q`: Müller et
/// al., "A Robust Method to Extract the Rotational Part of Deformations"
/// (2016). Started from last step's answer it needs only a few rounds.
fn turn_of(a: Mat3, mut q: Quat) -> Quat {
    for _ in 0..4 {
        let r = Mat3::from_quat(q);
        let top = r.x_axis.cross(a.x_axis) + r.y_axis.cross(a.y_axis) + r.z_axis.cross(a.z_axis);
        let bottom = (r.x_axis.dot(a.x_axis) + r.y_axis.dot(a.y_axis) + r.z_axis.dot(a.z_axis)).abs() + 1e-9;
        let w = top / bottom;
        let angle = w.length();
        if angle < 1e-7 {
            break;
        }
        q = (Quat::from_axis_angle(w / angle, angle) * q).normalize();
    }
    q
}

/// Flat ground at the frame's own floor.
pub fn flat(_: Vec3) -> f32 {
    0.0
}

/// A body let go where it stood and run to a standstill on flat ground:
/// what bones that have been lying in a field look like. The same seed
/// always gives the same pose.
pub fn settled(rig: &Rig, voxel: f32, pivot: Vec3, seed: u32) -> Pose {
    // Nobody saw it fall, so it starts most of the way over onto a side
    // and settles from there.
    let mut start = Pose::rest(rig.bones.len());
    if rig.plan != Some(Plan::Serpent) {
        let side = if seed & 1 == 0 { 1.0 } else { -1.0 };
        start.bones[0].z = side * (1.0 + (seed >> 1 & 7) as f32 * 0.05);
    }
    let mut l = Limp::new(rig, voxel, pivot, &start);
    let (lo, _) = l.span_y();
    l.shift(Vec3::new(0.0, lo - voxel * 0.5, 0.0));
    l.topple(rig, seed);
    l.run_out(&flat);
    l.pose(rig)
}

/// Lying on one side, every limb let go onto the ground: the pose an animal
/// that sleeps that way is in. `side` is -1 or 1.
pub fn on_side(rig: &Rig, voxel: f32, pivot: Vec3, side: f32, seed: u32) -> Pose {
    let mut start = Pose::rest(rig.bones.len());
    // Put over onto the side first, about the middle of the body, so it
    // starts out lying rather than having to fall there.
    start.bones[0] = Vec3::new(0.0, 0.0, side * std::f32::consts::FRAC_PI_2);
    let mut l = Limp::new(rig, voxel, pivot, &start);
    let (lo, _) = l.span_y();
    l.shift(Vec3::new(0.0, lo - voxel * 0.5, 0.0));
    let mut rng = Rng::new(seed | 1);
    // A little give in every point, so no two sleepers lie the same way.
    for i in 0..l.pts.len() {
        l.prev[i] = l.pts[i] - Vec3::new(rng.signed(), 0.0, rng.signed()) * 0.2 * STEP;
    }
    l.run_out(&flat);
    l.pose(rig)
}

/// Unwrap the angles of `pose` onto those of `prev`, so drawing between
/// the two does not go the long way round. Returns false if any joint jumped
/// further than drawing between them could make sense of (a turn passing
/// straight up through the pole of the angles), when the caller should not
/// draw between them at all.
pub fn follow(pose: &mut Pose, prev: &Pose) -> bool {
    use std::f32::consts::{PI, TAU};
    let mut smooth = pose.bones.len() == prev.bones.len();
    for (e, p) in pose.bones.iter_mut().zip(&prev.bones) {
        e.y += ((p.y - e.y) / TAU).round() * TAU;
        e.z += ((p.z - e.z) / TAU).round() * TAU;
        if (*e - *p).abs().max_element() > PI * 0.4 {
            smooth = false;
        }
    }
    smooth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;

    /// Every rigged creature in the library, sliced as worldload does it,
    /// with its voxel edge and pivot, and its skeleton's rig if it has one.
    fn rigs() -> Vec<(String, Rig, f32, Vec3, Option<Rig>)> {
        let (lib, errors) = Library::load(&crate::library::data_root());
        let errors: Vec<_> = errors.iter().filter(|e| e.contains("creatures")).collect();
        assert!(errors.is_empty(), "{errors:?}");
        lib.creatures
            .iter()
            .filter_map(|c| {
                let def = c.rig.as_ref()?;
                let names: Vec<&String> = c.colors.keys().collect();
                let resolve = |n: &str| {
                    if n == "air" {
                        Some(0u8)
                    } else {
                        names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
                    }
                };
                let dims = crate::anim::slice::Dims(c.grid);
                let whole = crate::world::shape::compile(&c.ops, c.grid, &resolve).mat;
                let s = crate::anim::slice::slice(def, &c.ops, &whole, dims, &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
                let bones = crate::creatures::age::Stages::resolve(&c.ages, c.height).bones.then(|| {
                    let (cells, _) = crate::anim::bones::skeletonise(&s, 250, 251, c.voxel, c.half_w, c.height);
                    let mut r = s.rig.clone();
                    crate::anim::bones::set_hulls(&mut r, crate::anim::bones::hulls(&s, &cells));
                    r
                });
                Some((c.name.clone(), s.rig, c.voxel, dims.pivot(), bones))
            })
            .collect()
    }

    fn find(name: &str) -> (Rig, f32, Vec3, Option<Rig>) {
        let (_, r, v, p, b) = rigs().into_iter().find(|r| r.0 == name).unwrap_or_else(|| panic!("no {name}"));
        (r, v, p, b)
    }

    /// The hip height of a standing animal, metres: what "lying down" is
    /// measured against.
    fn stands(rig: &Rig, voxel: f32, pivot: Vec3) -> f32 {
        let l = Limp::new(rig, voxel, pivot, &Pose::rest(rig.bones.len()));
        l.span_y().1
    }

    /// The highest point of a body posed as `pose`, frame metres.
    fn top(rig: &Rig, voxel: f32, pivot: Vec3, pose: &Pose) -> f32 {
        Limp::new(rig, voxel, pivot, pose).span_y().1
    }

    /// The whole reason for this file: a four-legged skeleton let go in a
    /// field ends up lying on the ground, not standing on its legs.
    #[test]
    fn skeletons_lie_down() {
        let mut checked = 0;
        for (name, rig, voxel, pivot, bones) in rigs() {
            let Some(bones) = bones else { continue };
            // A sprawler is about as wide as it is tall, so lying on its side
            // is no lower than standing.
            if rig.plan.is_some_and(|p| !matches!(p, Plan::Quadruped | Plan::Hopper)) {
                continue;
            }
            // Measured at the root joint, the middle of the body, which on
            // its side is half the animal's width off the ground. One wider
            // than its legs are long (an ankylosaur) lies no lower than it
            // stands, so this cannot tell for it.
            let standing = Limp::new(&bones, voxel, pivot, &Pose::rest(bones.bones.len()));
            let tall = standing.pts[0].y;
            let half_w = standing.pts.iter().map(|p| p.x.abs()).fold(0.0, f32::max);
            if half_w > tall * 0.6 {
                continue;
            }
            for seed in 0..6u32 {
                let pose = settled(&bones, voxel, pivot, seed);
                let high = Limp::new(&bones, voxel, pivot, &pose).pts[0].y;
                // Two thirds, not a half: a triceratops on its side is propped
                // up at the front by its frill.
                assert!(high < tall * 0.66, "{name} seed {seed}: its middle is {high:.2} m up, standing it is {tall:.2} m");
                for e in &pose.bones {
                    assert!(e.is_finite(), "{name} seed {seed}: {e:?}");
                }
            }
            checked += 1;
        }
        assert!(checked > 10, "only {checked} skeletons");
    }

    /// Down to the ground and no further: nothing of a settled body is in
    /// the floor, and something of it is on it.
    #[test]
    fn a_body_rests_on_the_ground() {
        for name in ["deer", "coyote", "rabbit", "bear"] {
            let (rig, voxel, pivot, _) = find(name);
            for seed in 0..4u32 {
                let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
                l.topple(&rig, seed);
                l.run_out(&flat);
                assert!(l.settled(), "{name} seed {seed}: still moving");
                let (lo, _) = l.span_y();
                assert!(lo > -0.01 && lo < voxel * 1.5, "{name} seed {seed}: lowest point at {lo:.3}");
            }
        }
    }

    /// A body held up by the scruff hangs below the hand that holds it.
    #[test]
    fn held_by_the_scruff_it_hangs() {
        let (rig, voxel, pivot, _) = find("rabbit");
        let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
        let at = Vec3::new(0.0, 1.5, 0.0);
        l.hold(&rig, Some((Grip::Scruff, at)));
        for _ in 0..120 {
            l.step(STEP, &flat, Vec3::ZERO);
        }
        let m = l.middle();
        assert!(m.y < at.y - 0.05, "the middle of it is at {m:?}, not below the scruff at {at:?}");
        let off = Vec3::new(m.x - at.x, 0.0, m.z - at.z).length();
        assert!(off < (at.y - m.y) * 0.6, "it sticks out sideways from the hand: {m:?}");
        assert!(l.grip_at(&rig, Grip::Scruff).distance(at) < 0.01, "it slipped out of the hand");
    }

    /// Taken by whatever point is nearest a hand reaching in from behind
    /// and above, a body hangs below that point, whichever it is.
    #[test]
    fn held_anywhere_it_hangs_from_there() {
        for name in ["rabbit", "coyote", "deer"] {
            let (rig, voxel, pivot, _) = find(name);
            let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
            // Reaching for the rump: a model faces -Z, so behind is +Z.
            let (lo, hi) = l.span_y();
            let reach = Vec3::new(0.0, hi, l.pts.iter().map(|p| p.z).fold(f32::MIN, f32::max));
            let grip = l.nearest(reach, &[Role::Root, Role::Spine]);
            let Grip::Point(i) = grip else { panic!("{name}: {grip:?}") };
            let at = l.pts[i] + Vec3::Y * (hi - lo + 1.0);
            l.hold(&rig, Some((grip, at)));
            for _ in 0..150 {
                l.step(STEP, &flat, Vec3::ZERO);
            }
            assert!(l.grip_at(&rig, grip).distance(at) < 0.01, "{name}: it slipped out of the hand");
            let m = l.middle();
            assert!(m.y < at.y - 0.05, "{name}: its middle {m:?} is not below the hand {at:?}");
            let off = Vec3::new(m.x - at.x, 0.0, m.z - at.z).length();
            assert!(off < (at.y - m.y) * 0.5, "{name}: it sticks out sideways from the hand, middle {m:?}, hand {at:?}");
        }
    }

    /// A body held by the scruff lower than it is long lies along the
    /// ground behind the hand when the hand moves off: a deer dragged.
    #[test]
    fn dragged_it_trails() {
        let (rig, voxel, pivot, _) = find("deer");
        let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
        l.topple(&rig, 3);
        l.run_out(&flat);
        let start = l.grip_at(&rig, Grip::Scruff);
        let mut at = Vec3::new(start.x, 0.5, start.z);
        for _ in 0..150 {
            at.x += 1.2 * STEP;
            l.hold(&rig, Some((Grip::Scruff, at)));
            l.step(STEP, &flat, Vec3::ZERO);
        }
        let m = l.middle();
        assert!(m.x < at.x - 0.3, "the body is not behind the hand: middle {m:?}, hand {at:?}");
        assert!(l.span_y().0 > -0.01, "it was dragged through the floor");
    }

    /// An animal asleep on its side has its legs on the ground, not up in
    /// the air.
    #[test]
    fn asleep_on_its_side_the_legs_are_down() {
        for name in ["coyote", "cougar", "bear"] {
            let (rig, voxel, pivot, _) = find(name);
            let tall = stands(&rig, voxel, pivot);
            let pose = on_side(&rig, voxel, pivot, 1.0, 5);
            let mut fk = Vec::new();
            forward(&rig, &pose, voxel, pivot, &mut fk);
            for (b, bone) in rig.bones.iter().enumerate() {
                if bone.role == Role::Leg {
                    assert!(fk[b].0.y < tall * 0.6, "{name}: {} is held up at {:.2} m of {tall:.2}", bone.name, fk[b].0.y);
                }
            }
            // And it is over, not sitting up.
            assert!(top(&rig, voxel, pivot, &pose) < tall * 0.85, "{name} is not lying down");
        }
    }

    /// Every creature goes limp and settles without coming apart, turning a
    /// joint past its stop or going to NaN.
    #[test]
    fn every_creature_settles_in_one_piece() {
        for (name, rig, voxel, pivot, _) in rigs() {
            let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
            l.topple(&rig, 11);
            l.run_out(&flat);
            let pose = l.pose(&rig);
            assert!(pose.root.is_finite(), "{name}: root {:?}", pose.root);
            for (b, part) in l.parts.iter().enumerate() {
                if let Some(p) = part.parent {
                    let rel = l.parts[p].rot.inverse() * part.rot;
                    let angle = 2.0 * rel.w.abs().clamp(0.0, 1.0).acos();
                    assert!(angle < part.lim + 0.05, "{name}: part {b} bent {angle:.2} past {:.2}", part.lim);
                }
            }
            // Drawn from its turns, nothing of it is in the floor.
            let mut fk = Vec::new();
            forward(&rig, &pose, voxel, pivot, &mut fk);
            for (b, bone) in rig.bones.iter().enumerate() {
                for h in &bone.hull {
                    let y = (fk[b].0 + fk[b].1 * ((*h - bone.joint) * voxel)).y;
                    assert!(y > -voxel * 1.5 - 0.01, "{name}: {} is drawn {y:.3} m into the floor", bone.name);
                }
            }
        }
    }

    /// Not a check: how every creature settles, one line each (steps taken,
    /// last movement, lowest point, how high its middle ends and how far it
    /// is over). `cargo test --release limp::tests::report -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn report() {
        for (name, rig, voxel, pivot, _) in rigs() {
            let stand = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len())).pts[0].y;
            for seed in 0..3u32 {
                let mut l = Limp::new(&rig, voxel, pivot, &Pose::rest(rig.bones.len()));
                l.topple(&rig, seed);
                let mut steps = 0;
                let mut last = 0.0f32;
                while !l.settled() && steps < MOST_STEPS {
                    let before = l.pts.clone();
                    l.step(STEP, &flat, Vec3::ZERO);
                    last = before.iter().zip(&l.pts).map(|(a, b)| a.distance(*b)).fold(0.0, f32::max);
                    steps += 1;
                }
                let pose = l.pose(&rig);
                let mut fk = Vec::new();
                forward(&rig, &pose, voxel, pivot, &mut fk);
                let mut drawn = (f32::MAX, String::new());
                for (b, bone) in rig.bones.iter().enumerate() {
                    for h in &bone.hull {
                        let y = (fk[b].0 + fk[b].1 * ((*h - bone.joint) * voxel)).y;
                        if y < drawn.0 {
                            drawn = (y, bone.name.clone());
                        }
                    }
                }
                let up = l.parts[0].rot * Vec3::Y;
                eprintln!(
                    "{name:>22} {seed}: steps {steps:>3} last {last:.4} sim-low {:+.3} drawn-low {:+.3} ({}) root {:.2}/{stand:.2} tilt {:.0} voxel {voxel:.3}",
                    l.span_y().0,
                    drawn.0,
                    drawn.1,
                    l.pts[0].y,
                    up.y.clamp(-1.0, 1.0).acos().to_degrees()
                );
            }
        }
    }

    /// No two fall alike, and one seed always falls the same way.
    #[test]
    fn no_two_settle_alike() {
        let (rig, voxel, pivot, _) = find("deer");
        let a = settled(&rig, voxel, pivot, 1);
        let b = settled(&rig, voxel, pivot, 2);
        assert_ne!(a.bones, b.bones);
        assert_eq!(settled(&rig, voxel, pivot, 1).bones, a.bones);
    }
}
