//! Octopuses: the `creep` movement.
//!
//! An octopus is a body and eight arms, and each arm is a serpent of its own
//! (anim/spine.rs): a path of points from where it leaves the body to its
//! tip. The arms are let down onto whatever is under them, which is what
//! drapes an octopus over a rock or a rib on the sea floor, and when the body
//! moves they are pulled along behind it. The mover keeps the body on the
//! bed, creeps it from one high spot to the next, pales as something comes
//! near, and when it is pressed too hard jets off backwards, mantle first,
//! with its arms streaming behind.
//!
//! The rig (plan `octopus`) is a root for the head and mantle and eight
//! chains of `arm` bones hanging off it, each pointing out from the body at
//! rest. The model faces -Z with the mantle behind (+Z).

use super::startle::{self, Nerve, Threat};
use super::terrain::{voxel_ground, Terrain};
use super::{Creature, Look, Species};
use crate::anim::spine::{self, Chain, Spine};
use crate::anim::{bone_rot, Anim, Pose, Rig, Role};
use crate::models::Model;
use crate::world::noise::{hash32, Rng};
use glam::{Quat, Vec3};
use std::f32::consts::{PI, TAU};

/// How far the arms are held off what they lie on, metres.
const LIFT: f32 = 0.05;
/// How fast the arms are let down onto the bed, per second.
const SETTLE: f32 = 5.0;
/// The least water over the bed it will go into, metres.
const DEPTH: f32 = 1.2;
/// The colour it pales toward, sRGB: the sand it lies on.
const SAND: [f32; 3] = [196.0, 178.0, 150.0];
/// How much of the way to the sand it goes at its palest; the rest of its own
/// pattern stays, so a pale octopus is still an octopus.
const PALEST: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Still on the bed with its arms spread over whatever is there.
    Rest,
    /// Creeping over the bed toward `goal`.
    Creep { goal: Vec3 },
    /// Startled: jetting off at `vel`, mantle first.
    Jet { vel: Vec3 },
}

/// One arm: its chain of bones, the path it lies along (one point per bone
/// and one for the tip), and which way it points from the body at rest.
pub struct Arm {
    pub chain: Chain,
    pub spine: Spine,
    pub angle: f32,
}

pub struct Octopus {
    pub arms: Vec<Arm>,
    pub mode: Mode,
    /// The way the head faces (model -Z), radians of yaw.
    pub face: f32,
    timer: f32,
    alarm: f32,
    /// Height of the body over the bed, metres: 0 resting, up in a jet.
    rise: f32,
    /// 0 its own colour, 1 as pale as it goes: it blanches as you come near.
    pub pale: f32,
    /// The tint and shift its `Morph` was rolled with, which paling works from.
    own: Option<([f32; 3], [f32; 3])>,
    t: f32,
    rng: Rng,
}

impl Octopus {
    /// What it is doing, for the headless stats.
    pub fn doing(&self) -> &'static str {
        match self.mode {
            Mode::Rest => "resting",
            Mode::Creep { .. } => "creeping",
            Mode::Jet { .. } => "jetting",
        }
    }
}

/// The arms of an octopus rig: every chain of `arm` bones hanging off the
/// root, each with the way it points at rest. The last bone's length is taken
/// to be the one before it, since nothing marks where its tip ends.
pub fn arms(rig: &Rig) -> Vec<(Chain, f32)> {
    let mut out = Vec::new();
    for (i, b) in rig.bones.iter().enumerate() {
        if b.role != Role::Arm || b.parent != Some(0) {
            continue;
        }
        let mut bones = vec![i];
        while let Some(next) = rig.bones.iter().position(|c| c.parent == Some(*bones.last().unwrap()) && c.role == Role::Arm) {
            bones.push(next);
            if bones.len() > rig.bones.len() {
                break;
            }
        }
        let mut d: Vec<f32> = bones.windows(2).map(|w| (rig.bones[w[1]].joint - rig.bones[w[0]].joint).length()).collect();
        d.push(d.last().copied().unwrap_or(2.0));
        let out_dir = rig.bones[*bones.last().unwrap()].joint - rig.bones[0].joint;
        out.push((Chain { bones, d }, out_dir.x.atan2(out_dir.z)));
    }
    out
}

/// The root's joint (creature frame, metres) and turn for a pose.
fn root_frame(rig: &Rig, pose: &Pose, vox: f32, pivot: Vec3) -> (Vec3, Quat) {
    ((rig.bones[0].joint - pivot) * vox + pose.root, bone_rot(pose.bones[0]))
}

/// Where an arm leaves the body, in the creature's frame.
fn base(rig: &Rig, ch: &Chain, root: (Vec3, Quat), vox: f32) -> Vec3 {
    root.0 + root.1 * (rig.bones[ch.bones[0]].joint - rig.bones[0].joint) * vox
}

/// An arm laid out flat from `from` at heading `angle`, curling by `curl`
/// radians toward its tip and reaching `reach` of its length.
fn spread(ch: &Chain, vox: f32, from: Vec3, angle: f32, curl: f32, reach: f32) -> Vec<Vec3> {
    let total: f32 = ch.d.iter().sum::<f32>().max(1e-3);
    let mut pts = vec![from];
    let mut s = 0.0;
    for (i, &d) in ch.d.iter().enumerate() {
        s += d;
        let a = angle + curl * (s / total).powf(1.5);
        pts.push(pts[i] + Vec3::new(a.sin(), 0.0, a.cos()) * d * vox * reach);
    }
    pts
}

/// Put every arm's bones on its path. `at` is the creature's position: the
/// paths are in the world, the pose in the creature's frame.
fn lay(rig: &Rig, arms: &[Arm], root: Quat, at: Vec3, pose: &mut Pose) {
    for arm in arms {
        let local: Vec<Vec3> = arm.spine.pts.iter().map(|p| *p - at).collect();
        spine::pose_limb(rig, &arm.chain, root, &local, pose);
    }
}

/// A slow curl down each arm: the writhing of an octopus at rest.
fn writhe(arms: &[Arm], pose: &mut Pose, t: f32, amp: f32) {
    for (k, arm) in arms.iter().enumerate() {
        let n = arm.chain.n() as f32;
        for (i, &b) in arm.chain.bones.iter().enumerate() {
            if let Some(e) = pose.bones.get_mut(b) {
                e.y += amp * (i as f32 + 1.0) / n * (t * 0.9 + k as f32 * 1.7 + i as f32 * 0.8).sin();
            }
        }
    }
}

/// The bed under (x, z), looked for from `from` down: rock, sand or bone.
fn bed(terrain: &Terrain, x: f32, z: f32, from: f32) -> f32 {
    voxel_ground(terrain.store, x, z, from).map_or_else(|| terrain.bed(x, z), |g| g.0)
}

/// Put `n` octopuses of species `s` on the sea floor near (x, z). False if
/// the water there is too shallow.
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, models: &[Model], terrain: &Terrain, (x, z): (f32, f32), n: u32, seed: u32) -> bool {
    let m = &models[sp.model];
    let Some(rig) = m.rig.as_ref() else { return false };
    let chains = arms(rig);
    if chains.is_empty() {
        return false;
    }
    let want = sp.beh.shallows.unwrap_or(2.0).max(DEPTH);
    let mut rng = Rng::new(seed | 1);
    let mut placed = 0;
    for k in 0..n.max(1) {
        let a = rng.unit() * TAU;
        let r = if k == 0 { 0.0 } else { 3.0 + rng.unit() * 6.0 };
        let (px, pz) = (x + a.sin() * r, z + a.cos() * r);
        let gy = bed(terrain, px, pz, terrain.sea);
        if terrain.sea - gy < want {
            continue;
        }
        let at = Vec3::new(px, gy, pz);
        let face = rng.unit() * TAU;
        let mut pose = Pose::rest(rig.bones.len());
        pose.bones[0].y = face;
        let root = root_frame(rig, &pose, m.voxel, m.pivot);
        let arms: Vec<Arm> = chains
            .iter()
            .map(|(ch, angle)| {
                let from = at + base(rig, ch, root, m.voxel);
                let mut spine = Spine {
                    pts: spread(ch, m.voxel, from, angle + face, 0.0, 0.9),
                    phase: 0.0,
                };
                spine.settle(&|x, z| Some(bed(terrain, x, z, at.y + 1.0)), LIFT, 1.0);
                spine.tighten(ch, m.voxel);
                Arm {
                    chain: ch.clone(),
                    spine,
                    angle: *angle,
                }
            })
            .collect();
        let mut oct = Octopus {
            arms,
            mode: Mode::Rest,
            face,
            timer: 0.0,
            alarm: 0.0,
            rise: 0.0,
            pale: 0.0,
            own: None,
            t: rng.unit() * 50.0,
            rng: Rng::walker(hash32(seed ^ k.wrapping_mul(0x9e37)) | 1),
        };
        oct.timer = 10.0 + oct.rng.unit() * 30.0;
        lay(rig, &oct.arms, root.1, at, &mut pose);
        world.spawn((oct, Creature::of(s), Look::new(sp.model, at, 0.0), Anim::hold(pose)));
        placed += 1;
    }
    placed > 0
}

/// Somewhere to creep to: the highest of a few spots round `at` that still
/// has water over it, a rock or a rib before open sand.
fn pick_goal(terrain: &Terrain, at: Vec3, rng: &mut Rng) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..8 {
        let a = rng.unit() * TAU;
        let r = 3.0 + rng.unit() * 7.0;
        let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
        let gy = bed(terrain, x, z, terrain.sea);
        if terrain.sea - gy < DEPTH {
            continue;
        }
        let score = gy + rng.unit() * 0.6;
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, Vec3::new(x, gy, z)));
        }
    }
    best.map(|b| b.1)
}

/// Step every octopus.
pub fn update(world: &mut hecs::World, species: &[Species], models: &[Model], dt: f32, threat: &Threat, terrain: &Terrain) {
    for (oct, look, me, morph, anim) in world.query_mut::<(&mut Octopus, &mut Look, &Creature, Option<&mut crate::variety::Morph>, &mut Anim)>() {
        look.begin_step();
        let sp = &species[me.species];
        let m = &models[sp.model];
        let Some(rig) = m.rig.as_ref() else { continue };
        let scale = morph.as_ref().map_or(1.0, |x| x.scale);
        let pace = morph.as_ref().map_or(1.0, |x| x.speed);
        let vox = m.voxel * scale;
        let k = |rate: f32| (dt * rate).min(1.0);
        oct.t += dt;
        oct.timer -= dt;

        // How hard the player presses on it.
        let nerve = Nerve::of(sp, morph.as_deref(), super::age::Stage::Adult);
        let facing = Vec3::new(-oct.face.sin(), 0.0, -oct.face.cos());
        let press = startle::pressure(&nerve, look.pos, facing, threat, || false);
        let bolts = startle::bolts(&mut oct.alarm, press, dt);
        let away = Vec3::new(look.pos.x - threat.pos.x, 0.0, look.pos.z - threat.pos.z).normalize_or_zero();

        if bolts && !matches!(oct.mode, Mode::Jet { .. }) {
            let dir = (away + Vec3::Y * 0.25).normalize_or_zero();
            oct.mode = Mode::Jet { vel: dir * sp.beh.flee_speed * pace };
            oct.timer = 2.5 + oct.rng.unit();
        }
        let mut want_face = oct.face;
        let mut turn = 1.5;
        match oct.mode {
            Mode::Rest => {
                if oct.timer <= 0.0 {
                    match pick_goal(terrain, look.pos, &mut oct.rng) {
                        Some(goal) => {
                            oct.mode = Mode::Creep { goal };
                            oct.timer = 30.0;
                        }
                        None => oct.timer = 5.0,
                    }
                }
                oct.rise += (0.0 - oct.rise) * k(1.0);
            }
            Mode::Creep { goal } => {
                let to = Vec3::new(goal.x - look.pos.x, 0.0, goal.z - look.pos.z);
                if to.length() < 0.4 || oct.timer <= 0.0 {
                    oct.mode = Mode::Rest;
                    oct.timer = 15.0 + oct.rng.unit() * 45.0;
                } else {
                    // Head and arms first.
                    want_face = to.x.atan2(to.z) + PI;
                    let ahead = Vec3::new(-oct.face.sin(), 0.0, -oct.face.cos());
                    let lined = ahead.dot(to.normalize_or_zero()).max(0.0);
                    let step = to.normalize_or_zero() * sp.beh.walk_speed * pace * lined * dt;
                    look.pos.x += step.x;
                    look.pos.z += step.z;
                }
                oct.rise += (0.0 - oct.rise) * k(1.0);
            }
            Mode::Jet { vel } => {
                // Mantle first: the head faces away from the way it goes.
                want_face = vel.x.atan2(vel.z);
                turn = 6.0;
                look.pos.x += vel.x * dt;
                look.pos.z += vel.z * dt;
                oct.rise += (1.0 - oct.rise) * k(2.0);
                let vel = vel * (1.0 - 0.7 * dt);
                oct.mode = Mode::Jet { vel };
                if oct.timer <= 0.0 {
                    oct.mode = Mode::Rest;
                    oct.timer = 20.0 + oct.rng.unit() * 40.0;
                }
            }
        }
        // Never out of the water: back off a bed that has come up too near
        // the surface.
        let floor = bed(terrain, look.pos.x, look.pos.z, look.pos.y + 1.0);
        if terrain.sea - floor < DEPTH * 0.6 {
            let back = look.prev_pos - look.pos;
            look.pos.x += back.x;
            look.pos.z += back.z;
            if let Mode::Creep { .. } = oct.mode {
                oct.mode = Mode::Rest;
                oct.timer = 5.0;
            }
        }
        let floor = bed(terrain, look.pos.x, look.pos.z, look.pos.y + 1.0);
        look.pos.y = (floor + oct.rise).min(terrain.sea - 0.6).max(floor);
        let d = crate::util::wrap_angle(want_face - oct.face);
        oct.face += d.clamp(-turn * dt, turn * dt);
        look.yaw = 0.0;

        // The body: breathing, and pitched over in a jet so the mantle leads.
        let jet = matches!(oct.mode, Mode::Jet { .. });
        let mut pose = Pose::rest(rig.bones.len());
        pose.bones[0] = Vec3::new(if jet { 0.6 } else { 0.05 * (oct.t * 1.1).sin() }, oct.face, 0.0);
        let root = root_frame(rig, &pose, vox, m.pivot);
        let at = look.pos;
        let creeping = matches!(oct.mode, Mode::Creep { .. });
        for (i, arm) in oct.arms.iter_mut().enumerate() {
            let from = at + base(rig, &arm.chain, root, vox);
            if jet {
                // Streaming behind: each point pulled after the one before.
                arm.spine.lead(&arm.chain, vox, from);
                for p in &mut arm.spine.pts {
                    p.y = p.y.max(bed(terrain, p.x, p.z, p.y + 1.0) + LIFT);
                }
                continue;
            }
            // Spread round the body, curling slowly; creeping, each arm
            // reaches and pulls in turn.
            let stride = if creeping { 0.5 * (oct.t * 2.2 + i as f32 * PI * 0.5).sin() } else { 0.0 };
            let curl = 0.5 * (oct.t * 0.25 + i as f32 * 1.7).sin() + 0.35 + stride;
            let reach = if creeping { 0.9 } else { 0.85 };
            let target = spread(&arm.chain, vox, from, arm.angle + oct.face, curl, reach);
            arm.spine.pts[0] = from;
            arm.spine.ease(&target, k(if creeping { 3.0 } else { 1.2 }));
            arm.spine.tighten(&arm.chain, vox);
            let top = at.y + 1.0;
            arm.spine.settle(&|x, z| Some(bed(terrain, x, z, top)), LIFT, k(SETTLE));
            arm.spine.tighten(&arm.chain, vox);
        }
        lay(rig, &oct.arms, root.1, at, &mut pose);
        writhe(&oct.arms, &mut pose, oct.t, if jet { 0.25 } else { 0.08 });
        spine::continue_from(&mut pose, &anim.pose);
        spine::ease_joints(&mut pose, &anim.pose, dt);
        anim.prev = std::mem::replace(&mut anim.pose, pose);
        if anim.prev.bones.len() != anim.pose.bones.len() {
            anim.prev = anim.pose.clone();
        }

        // Paling as anything comes near: from its own brick red toward the
        // sand, on its own coat's tint, at once going pale and slowly back.
        let dist = (threat.pos - look.pos).length();
        let near = ((sp.beh.wary * 2.5 - dist) / (sp.beh.wary * 2.0).max(0.5)).clamp(0.0, 1.0);
        let want = near.max(oct.alarm.min(1.0));
        oct.pale += (want - oct.pale) * k(if want > oct.pale { 1.5 } else { 0.25 });
        if let Some(mo) = morph {
            let (tint, shift) = *oct.own.get_or_insert((mo.tint, mo.shift));
            let sand = crate::models::linear_colour(&SAND);
            let w = oct.pale * PALEST;
            mo.tint = tint.map(|v| v * (1.0 - w));
            mo.shift = [0, 1, 2].map(|i| shift[i] * (1.0 - w) + sand[i] * w);
        }
    }
}

/// Poses for `--anim-sheet NAME:moves`, through the same paths the mover
/// uses: spread flat, curled, creeping at four points of its stride, and
/// draped over a mound, then jetting with its arms streaming behind.
pub fn sheet(rig: &Rig, vox: f32, pivot: Vec3) -> [Vec<(String, Pose)>; 2] {
    let chains = arms(rig);
    let shape = |curl: &dyn Fn(usize) -> f32, ground: &dyn Fn(f32, f32) -> Option<f32>, pitch: f32, trail: bool| -> Pose {
        let mut pose = Pose::rest(rig.bones.len());
        pose.bones[0].x = pitch;
        let root = root_frame(rig, &pose, vox, pivot);
        let arms: Vec<Arm> = chains
            .iter()
            .enumerate()
            .map(|(i, (ch, angle))| {
                let from = base(rig, ch, root, vox);
                let mut pts = if trail {
                    // Bundled behind the head, a little apart.
                    spread(ch, vox, from, PI + 0.12 * (*angle).sin(), 0.0, 1.0)
                } else {
                    spread(ch, vox, from, *angle, curl(i), 0.88)
                };
                if !trail {
                    let mut s = Spine { pts, phase: 0.0 };
                    s.settle(ground, LIFT, 1.0);
                    s.tighten(ch, vox);
                    pts = s.pts;
                }
                Arm {
                    chain: ch.clone(),
                    spine: Spine { pts, phase: 0.0 },
                    angle: *angle,
                }
            })
            .collect();
        lay(rig, &arms, root.1, Vec3::ZERO, &mut pose);
        if trail {
            writhe(&arms, &mut pose, 0.6, 0.25);
        }
        pose
    };
    let flat = |_: f32, _: f32| Some(0.0f32);
    // A mound under the body, falling away all round: a rock or a rib.
    let mound = |x: f32, z: f32| Some((0.6 - 0.18 * (x * x + z * z)).max(0.0) - 0.6);
    let mut lower = vec![
        ("spread".to_string(), shape(&|_| 0.0, &flat, 0.0, false)),
        ("curled".to_string(), shape(&|i| 0.5 * (i as f32 * 1.7).sin() + 0.35, &flat, 0.0, false)),
    ];
    for s in 0..4 {
        let ph = s as f32 * 0.25;
        lower.push((format!("creep {ph}"), shape(&|i| 0.35 + 0.5 * (TAU * ph + i as f32 * PI * 0.5).sin(), &flat, 0.0, false)));
    }
    let upper = vec![
        ("draped over a mound".to_string(), shape(&|i| 0.2 * (i as f32).sin(), &mound, 0.0, false)),
        ("jetting".to_string(), shape(&|_| 0.0, &flat, 0.6, true)),
    ];
    [lower, upper]
}
