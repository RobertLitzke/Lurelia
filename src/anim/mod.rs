//! Creature animation (ANIMATION.md): rigid voxel parts on a bone tree.
//!
//! A library creature with a `rig` is sliced at load into one part model per
//! bone (`slice.rs`), and, if it is big enough to leave bones, again into the
//! skeleton inside it (`bones.rs`). `ragdoll.rs` poses a body with nothing
//! holding it up.
//!
//! Each frame a `Pose` (a root offset and one rotation per
//! bone) is run through forward kinematics, and every part is drawn as its own
//! entity instance. A creature without an `Anim` component is drawn at rest.

pub mod bones;
pub mod figure;
pub mod flesh;
pub mod flight;
pub mod life;
pub mod motors;
pub mod ragdoll;
pub mod sheet;
pub mod slice;
pub mod spine;

use crate::library::{CreatureDef, Op};
use crate::models::Instance;
use glam::{EulerRot, Quat, Vec3};
use serde::Deserialize;
use std::collections::HashMap;

crate::util::named! {
    /// What a bone is, which decides how the motors move it (ANIMATION.md).
    pub enum Role {
        Root = "root",
        Spine = "spine",
        Neck = "neck",
        Head = "head",
        Jaw = "jaw",
        Tail = "tail",
        Leg = "leg",
        Arm = "arm",
        Wing = "wing",
        Ear = "ear",
    }
}

crate::util::named! {
    /// Body plans: posture tables and default activities. A rig that names
    /// none moves as a quadruped.
    pub enum Plan {
        Quadruped = "quadruped",
        Hopper = "hopper",
        Biped = "biped",
        Bird = "bird",
        Serpent = "serpent",
        Fish = "fish",
        Cetacean = "cetacean",
        Sprawler = "sprawler",
        /// Named in ANIMATION.md, with no motors yet: it moves as a quadruped.
        Flutterer = "flutterer",
        Ray = "ray",
        /// Eight legs out to the sides and two claws, walking sideways.
        Crab = "crab",
        /// A body and eight arms, each posed along a path of its own.
        Octopus = "octopus",
    }
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct RigDef {
    #[serde(default)]
    pub plan: Option<Plan>,
    /// Parents before children; the first bone is the root.
    pub bones: Vec<BoneDef>,
    /// Rows of the activity table that differ from the plan's defaults
    /// (life.rs): activity name -> { time: [lo, hi], next: { name: weight } }.
    #[serde(default)]
    pub activities: HashMap<String, life::ActDef>,
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct BoneDef {
    pub name: String,
    pub role: Role,
    #[serde(default)]
    pub parent: Option<String>,
    /// Joint position in model voxel units. The root's defaults to the centre
    /// of its voxels.
    #[serde(default)]
    pub joint: Option<[f32; 3]>,
    /// Boxes (`from` inclusive, `to` exclusive) whose voxels this bone takes
    /// from the root or from its parent.
    #[serde(default)]
    pub claims: Vec<[[i32; 3]; 2]>,
    /// "l" or "r".
    #[serde(default)]
    pub side: Option<String>,
    /// Leg pair, front to back (0 = front).
    #[serde(default)]
    pub pair: Option<u32>,
    /// Layers of the parent's voxels next to this part drawn with it too, to
    /// hide gaps when the joint bends.
    #[serde(default)]
    pub overlap: u32,
    /// How soft the part is, 0 rigid (the default) to 1 all flesh: it sags
    /// where the animal lies and lags as it moves (flesh.rs).
    #[serde(default)]
    pub flesh: f32,
}

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub role: Role,
    pub parent: Option<usize>,
    /// Model voxel units.
    pub joint: Vec3,
    /// The part model drawn for this bone.
    pub model: usize,
    /// -1 left, 1 right, 0 middle.
    pub side: f32,
    pub pair: u32,
}

#[derive(Clone, Debug)]
pub struct Rig {
    pub plan: Option<Plan>,
    pub bones: Vec<Bone>,
    pub activities: HashMap<String, life::ActDef>,
    /// Height of the lowest voxel, in model voxels: a perched bird stands
    /// on feet that hang this far above the model's floor.
    pub floor: f32,
    /// The soft body, if any bone has flesh (flesh.rs).
    pub soft: Option<flesh::Soft>,
}

impl Rig {
    /// Shift part model indices when the models are appended to a longer list.
    pub fn offset_models(&mut self, by: usize) {
        for b in &mut self.bones {
            b.model += by;
        }
    }

    /// The activity table for a creature with this rig and movement verb's routine.
    pub fn table(&self, routine: life::Routine) -> life::Table {
        life::table(self.plan, &self.activities, routine)
    }
}

/// A root offset (metres, creature frame) and one rotation per bone as
/// (pitch about x, yaw about y, roll about z) radians in the parent's frame;
/// bone 0's rotation tilts the whole body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    pub root: Vec3,
    pub bones: Vec<Vec3>,
    /// Per bone, how far its joint is pulled toward the root's joint (0 none,
    /// 1 all the way): a tortoise drawing its head and legs into the shell.
    /// Empty means none.
    pub tuck: Vec<f32>,
    /// A soft body's flesh (flesh.rs): how far it has settled, 0 to 1, and
    /// how far the jiggle has thrown it, creature frame, metres.
    pub sag: f32,
    pub jiggle: Vec3,
}

impl Pose {
    pub fn rest(n: usize) -> Self {
        Self {
            root: Vec3::ZERO,
            bones: vec![Vec3::ZERO; n],
            tuck: Vec::new(),
            sag: 0.0,
            jiggle: Vec3::ZERO,
        }
    }

    pub fn lerp(&self, other: &Pose, t: f32) -> Pose {
        let n = self.bones.len().max(other.bones.len());
        let at = |p: &Pose, i: usize| p.bones.get(i).copied().unwrap_or(Vec3::ZERO);
        let tk = |p: &Pose, i: usize| p.tuck.get(i).copied().unwrap_or(0.0);
        let tuck = if self.tuck.is_empty() && other.tuck.is_empty() {
            Vec::new()
        } else {
            (0..n).map(|i| tk(self, i) + (tk(other, i) - tk(self, i)) * t).collect()
        };
        Pose {
            root: self.root.lerp(other.root, t),
            bones: (0..n).map(|i| at(self, i).lerp(at(other, i), t)).collect(),
            tuck,
            sag: self.sag + (other.sag - self.sag) * t,
            jiggle: self.jiggle.lerp(other.jiggle, t),
        }
    }
}

/// The pose a creature is drawn in, and the one before it, for interpolation
/// between fixed steps. Without `life` the pose is held (contact sheets).
pub struct Anim {
    pub pose: Pose,
    pub prev: Pose,
    pub life: Option<Box<life::Life>>,
    /// Birds: wings and attitude from their flight.
    pub flight: Option<Box<flight::Flight>>,
    /// The dead and the carried: a body with nothing holding it up.
    pub ragdoll: Option<Box<ragdoll::Ragdoll>>,
    /// A soft body's jiggle spring (flesh.rs).
    pub jiggle: flesh::Jiggle,
}

impl Anim {
    pub fn hold(pose: Pose) -> Self {
        Self {
            prev: pose.clone(),
            pose,
            life: None,
            flight: None,
            ragdoll: None,
            jiggle: flesh::Jiggle::default(),
        }
    }

    /// Advance a limp body one step (creatures/carry.rs). `shake` is what the
    /// carrier's pace does to it.
    pub fn step_ragdoll(&mut self, rig: &Rig, dt: f32, shake: Vec3) {
        let Some(r) = self.ragdoll.as_mut() else {
            return;
        };
        r.step(rig, dt, shake);
        self.prev = std::mem::replace(&mut self.pose, r.pose.clone());
        if self.prev.bones.len() != self.pose.bones.len() {
            self.prev = self.pose.clone();
        }
    }

    /// A creature that chooses its own activities.
    pub fn alive(rig: &Rig, table: life::Table, seed: u32) -> Self {
        let mut a = Self::hold(Pose::rest(rig.bones.len()));
        a.life = Some(Box::new(life::Life::new(table, seed)));
        a
    }

    /// A bird.
    pub fn flying(rig: &Rig, f: flight::Flight) -> Self {
        let mut a = Self::hold(Pose::rest(rig.bones.len()));
        a.flight = Some(Box::new(f));
        a
    }

    /// Advance a bird one step and compose its pose.
    pub fn step_flight(&mut self, rig: &Rig, voxel: f32, inp: &flight::Input) {
        let Some(f) = self.flight.as_mut() else {
            return;
        };
        f.step(inp);
        self.prev = std::mem::take(&mut self.pose);
        self.pose = f.compose(rig, voxel);
        if self.prev.bones.len() != self.pose.bones.len() {
            self.prev = self.pose.clone();
        }
    }

    pub fn mobility(&self) -> f32 {
        self.life.as_ref().map_or(1.0, |l| l.mobility())
    }

    /// Advance one fixed step and compose the new pose.
    pub fn step(&mut self, rig: &Rig, voxel: f32, inp: &life::Input) {
        let Some(l) = self.life.as_mut() else { return };
        l.step(rig, voxel, inp);
        self.prev = std::mem::take(&mut self.pose);
        self.pose = l.compose(rig, voxel);
        if let Some(soft) = &rig.soft {
            self.pose.jiggle = self.jiggle.step(&self.pose, inp.pos, inp.yaw, soft.height * voxel, inp.dt);
        }
        if self.prev.bones.len() != self.pose.bones.len() {
            self.prev = self.pose.clone();
        }
    }
}

/// Yaw, then pitch, then roll.
pub fn bone_rot(e: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, e.y, e.x, e.z)
}

/// Each bone's joint position (metres, creature frame, from the model pivot)
/// and orientation. Parents come before children.
pub fn forward(rig: &Rig, pose: &Pose, voxel: f32, pivot: Vec3, out: &mut Vec<(Vec3, Quat)>) {
    out.clear();
    for (i, b) in rig.bones.iter().enumerate() {
        let r = pose.bones.get(i).map_or(Quat::IDENTITY, |e| bone_rot(*e));
        match b.parent.filter(|&p| p < i) {
            None => out.push(((b.joint - pivot) * voxel + pose.root, r)),
            Some(p) => {
                let (pp, pq) = out[p];
                let mut off = (b.joint - rig.bones[p].joint) * voxel;
                if let Some(&t) = pose.tuck.get(i).filter(|t| **t > 0.0) {
                    // Drawn in toward the body's centre, in the parent's frame.
                    off += (rig.bones[0].joint - b.joint) * voxel * t;
                }
                out.push((pp + pq * off, pq * r));
            }
        }
    }
}

/// Instances for a rigged creature: one per non-empty part. `at` is the
/// model pivot in the world (bob and ground lift included).
#[allow(clippy::too_many_arguments)]
pub fn push_parts(
    rig: &Rig,
    pose: &Pose,
    models: &[crate::models::Model],
    voxel: f32,
    scale: f32,
    // `head`: head, jaw and ears drawn this much larger again (a juvenile's
    // head is big for its body). Grown about each part's own joint, so the
    // head grows away from the neck; 1 for everything else.
    head: f32,
    pivot: Vec3,
    at: Vec3,
    yaw: f32,
    tint: [f32; 4],
    // Added after the tint multiplies (variety::Repaint): the named coat this
    // one wears, if any.
    shift: [f32; 3],
    fk: &mut Vec<(Vec3, Quat)>,
    out: &mut Vec<(usize, Instance)>,
) {
    push_parts_at(rig, 0, pose, models, voxel, scale, head, pivot, at, yaw, tint, shift, fk, out);
}

/// The same, for a rig whose bones index a block of models that was appended
/// somewhere else: the shooter's bots are built apart from the world's own
/// creatures, so their parts only know where they sit relative to each other
/// (`brain::avatar`).
#[allow(clippy::too_many_arguments)]
pub fn push_parts_at(
    rig: &Rig,
    base: usize,
    pose: &Pose,
    models: &[crate::models::Model],
    voxel: f32,
    scale: f32,
    head: f32,
    pivot: Vec3,
    at: Vec3,
    yaw: f32,
    tint: [f32; 4],
    shift: [f32; 3],
    fk: &mut Vec<(Vec3, Quat)>,
    out: &mut Vec<(usize, Instance)>,
) {
    forward(rig, pose, voxel * scale, pivot, fk);
    let yq = Quat::from_rotation_y(yaw);
    for (bi, (b, &(jp, q))) in rig.bones.iter().zip(fk.iter()).enumerate() {
        let model = b.model + base;
        let Some(pm) = models.get(model) else {
            continue;
        };
        if pm.mesh.quads.is_empty() {
            continue;
        }
        let s = if head != 1.0 && matches!(b.role, Role::Head | Role::Jaw | Role::Ear) {
            scale * head
        } else {
            scale
        };
        let w = at + yq * jp;
        out.push((
            model,
            Instance {
                pos_yaw: [w.x, w.y, w.z, yaw],
                info: [pm.voxel * s, pm.pivot.x, pm.pivot.y, pm.pivot.z],
                tint,
                rot: [q.x, q.y, q.z, q.w],
                shift: [shift[0], shift[1], shift[2], 0.0],
                flesh: rig.soft.as_ref().map_or([[0.0; 4]; 3], |s| s.instance(rig, pose, &fk[..], voxel * scale, bi)),
                glow: [0.0; 4],
            },
        ));
    }
}

/// Problems with a creature's rig, for `--validate`.
pub fn validate(c: &CreatureDef, out: &mut Vec<String>) {
    let Some(rig) = &c.rig else { return };
    let what = format!("creature {}", c.name);
    if rig.plan == Some(Plan::Flutterer) {
        out.push(format!("{what}: plan flutterer isn't built yet; it would move as a quadruped"));
    }
    for (name, a) in &rig.activities {
        for n in std::iter::once(name).chain(a.next.keys()) {
            if life::Act::parse(n).is_none() {
                out.push(format!("{what}: unknown activity {n}"));
            }
        }
    }
    if rig.bones.is_empty() {
        out.push(format!("{what}: rig has no bones"));
        return;
    }
    // A serpent is posed along a path, one point per bone (anim/spine.rs), so
    // its bones must be one line from the head.
    if rig.plan == Some(Plan::Serpent) {
        if rig.bones.len() < 4 {
            out.push(format!("{what}: a serpent needs at least four bones to bend"));
        }
        if !rig.bones.iter().enumerate().skip(1).all(|(i, b)| b.parent.as_deref() == Some(rig.bones[i - 1].name.as_str())) {
            out.push(format!("{what}: a serpent's bones must form one chain, each the child of the one before it"));
        }
    }
    if let Some(p) = &c.behaviour.flight {
        if !flight::PATTERNS.contains(&p.as_str()) {
            out.push(format!("{what}: unknown flight pattern {p}"));
        }
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, b) in rig.bones.iter().enumerate() {
        if seen.insert(&b.name, i).is_some() {
            out.push(format!("{what}: bone {} named twice", b.name));
        }
        match (&b.parent, i) {
            (None, 0) => {}
            (None, _) => out.push(format!("{what}: bone {} has no parent (only the first bone is the root)", b.name)),
            (Some(p), _) => match seen.get(p.as_str()) {
                Some(&k) if k < i => {}
                _ => out.push(format!("{what}: bone {}: parent {p} must be listed before it", b.name)),
            },
        }
        if i > 0 && b.joint.is_none() {
            out.push(format!("{what}: bone {} needs a joint", b.name));
        }
        if let Some(j) = b.joint {
            if j.iter().any(|v| !(0.0..=32.0).contains(v)) {
                out.push(format!("{what}: bone {}: joint outside the 32^3 grid", b.name));
            }
        }
        if let Some(s) = &b.side {
            if s != "l" && s != "r" {
                out.push(format!("{what}: bone {}: side must be l or r", b.name));
            }
        }
    }
    let mut bad = Vec::new();
    walk_parts(&c.ops, &mut |p| {
        if !seen.contains_key(p) && !bad.iter().any(|x: &String| x == p) {
            bad.push(p.to_string());
        }
    });
    for p in bad {
        out.push(format!("{what}: an op's part {p} is not a bone"));
    }
}

fn walk_parts(ops: &[Op], f: &mut dyn FnMut(&str)) {
    for op in ops {
        match op {
            Op::Box { part, .. }
            | Op::Walls { part, .. }
            | Op::Gable { part, .. }
            | Op::Ellipsoid { part, .. }
            | Op::Cylinder { part, .. }
            | Op::Log { part, .. }
            | Op::Disc { part, .. }
            | Op::Dish { part, .. }
            | Op::Segment { part, .. }
            | Op::Set { part, .. } => {
                if let Some(p) = part {
                    f(p);
                }
            }
            Op::RecolourBelow { .. } | Op::Recolour { .. } => {}
            Op::Repeat { ops, .. } => walk_parts(ops, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Rig {
        let b = |name: &str, parent: Option<usize>, joint: [f32; 3]| Bone {
            name: name.into(),
            role: Role::Neck,
            parent,
            joint: Vec3::from(joint),
            model: 0,
            side: 0.0,
            pair: 0,
        };
        Rig {
            plan: None,
            activities: HashMap::new(),
            floor: 0.0,
            soft: None,
            bones: vec![b("body", None, [16.0, 10.0, 16.0]), b("neck", Some(0), [16.0, 12.0, 10.0]), b("head", Some(1), [16.0, 18.0, 10.0])],
        }
    }

    #[test]
    fn children_follow_their_parent() {
        let rig = chain();
        let mut out = Vec::new();
        // Pitch the neck a quarter turn: the head joint, 6 voxels above the
        // neck joint, swings round it and keeps its distance.
        let mut pose = Pose::rest(3);
        pose.bones[1] = Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        forward(&rig, &pose, 1.0, Vec3::new(16.0, 0.0, 16.0), &mut out);
        let (neck, head) = (out[1].0, out[2].0);
        assert!(((head - neck).length() - 6.0).abs() < 1e-4);
        assert!((head.y - neck.y).abs() < 1e-4, "head should be level with the neck joint after a 90 degree pitch");
        // Tilting the body moves both.
        let mut tilt = Pose::rest(3);
        tilt.bones[0] = Vec3::new(0.3, 0.0, 0.0);
        let mut out2 = Vec::new();
        forward(&rig, &tilt, 1.0, Vec3::new(16.0, 0.0, 16.0), &mut out2);
        assert!((out2[2].0 - out[2].0).length() > 0.1);
        assert!((out2[0].0 - Vec3::new(0.0, 10.0, 0.0)).length() < 1e-4, "the root joint stays put");
    }
}
