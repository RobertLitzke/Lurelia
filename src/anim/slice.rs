//! Slicing a creature's voxel grid into one part per bone.
//!
//! The creature's ops build the whole body as before. The same ops are run a
//! second time with each op's material replaced by its bone (from the op's
//! `part`, the root when it has none), so later ops win exactly as they do for
//! colour. Bone `claims` boxes then take voxels from the root or the bone's
//! parent, and each bone's voxels become a model whose pivot is its joint.

use super::{Bone, Rig, RigDef};
use crate::library::Op;
use crate::models::{self, Model};
use crate::world::shape;
use glam::Vec3;

const N: i32 = 32;
const NONE: u8 = u8::MAX;

fn idx(x: i32, y: i32, z: i32) -> usize {
    ((y * N + z) * N + x) as usize
}

/// The rig and its part models (bone `model` indices count from 0 in `parts`).
pub struct Sliced {
    pub rig: Rig,
    pub parts: Vec<Model>,
    /// Which bone owns each voxel of the whole grid (`u8::MAX` = empty).
    #[allow(dead_code)] // read by the slicer's tests
    pub owner: Vec<u8>,
}

/// `whole` is the compiled colour grid (32^3, x fastest, then z, then y, 0 =
/// empty, else palette slot + 1, as `models::from_cells` takes it);
/// `colour_ok` says whether an op's material resolved in that compile.
pub fn slice(def: &RigDef, ops: &[Op], whole: &[u8], colour_ok: &dyn Fn(&str) -> bool, voxel: f32, half_w: f32, height: f32) -> Sliced {
    let names: Vec<&str> = def.bones.iter().map(|b| b.name.as_str()).collect();
    let bone_of = |p: &Option<String>| -> usize { p.as_deref().and_then(|p| names.iter().position(|n| *n == p)).unwrap_or(0) };
    let tagged = retag(ops, &bone_of, colour_ok);
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        n.strip_prefix('#').and_then(|k| k.parse::<u8>().ok()).map(|k| k + 1)
    };
    let parts_grid = shape::compile(&tagged, [N, N, N], &resolve);

    // Owner of each solid voxel.
    let mut owner = vec![NONE; whole.len()];
    for i in 0..whole.len() {
        if whole[i] != 0 {
            owner[i] = parts_grid.mat[i].saturating_sub(1);
        }
    }

    // Parents (by index; a bad or missing parent means the root).
    let parent: Vec<Option<usize>> = def
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if i == 0 {
                None
            } else {
                Some(b.parent.as_deref().and_then(|p| names.iter().position(|n| *n == p)).filter(|&k| k < i).unwrap_or(0))
            }
        })
        .collect();

    // Claims take voxels from the root or from the bone's parent.
    for (b, bd) in def.bones.iter().enumerate() {
        for [from, to] in &bd.claims {
            for y in from[1].max(0)..to[1].min(N) {
                for z in from[2].max(0)..to[2].min(N) {
                    for x in from[0].max(0)..to[0].min(N) {
                        let i = idx(x, y, z);
                        let o = owner[i];
                        if o != NONE && (o == 0 || Some(o as usize) == parent[b]) {
                            owner[i] = b as u8;
                        }
                    }
                }
            }
        }
    }

    // Joints: given, or the centre of the bone's own voxels.
    let centre = |b: usize| -> Vec3 {
        let (mut s, mut n) = (Vec3::ZERO, 0.0);
        for y in 0..N {
            for z in 0..N {
                for x in 0..N {
                    if owner[idx(x, y, z)] == b as u8 {
                        s += Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        n += 1.0;
                    }
                }
            }
        }
        if n > 0.0 {
            s / n
        } else {
            Vec3::new(16.0, 0.0, 16.0)
        }
    };
    let joints: Vec<Vec3> = def.bones.iter().enumerate().map(|(b, bd)| bd.joint.map(Vec3::from).unwrap_or_else(|| centre(b))).collect();

    let mut parts = Vec::with_capacity(def.bones.len());
    let mut bones = Vec::with_capacity(def.bones.len());
    for (b, bd) in def.bones.iter().enumerate() {
        let mut mine: Vec<bool> = owner.iter().map(|&o| o == b as u8).collect();
        // Overlap: grow into the parent's voxels, a layer at a time.
        if let Some(p) = parent[b] {
            for _ in 0..bd.overlap {
                let prev = mine.clone();
                for y in 0..N {
                    for z in 0..N {
                        for x in 0..N {
                            let i = idx(x, y, z);
                            if prev[i] || owner[i] != p as u8 {
                                continue;
                            }
                            let near = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)].iter().any(|&(dx, dy, dz)| {
                                let (a, c, d) = (x + dx, y + dy, z + dz);
                                (0..N).contains(&a) && (0..N).contains(&c) && (0..N).contains(&d) && prev[idx(a, c, d)]
                            });
                            if near {
                                mine[i] = true;
                            }
                        }
                    }
                }
            }
        }
        let cells: Vec<u8> = whole.iter().zip(&mine).map(|(&c, &m)| if m { c } else { 0 }).collect();
        let mut m = models::from_cells(&cells, voxel, half_w, height);
        m.pivot = joints[b];
        parts.push(m);
        bones.push(Bone {
            name: bd.name.clone(),
            role: bd.role.clone(),
            parent: parent[b],
            joint: joints[b],
            model: b,
            side: match bd.side.as_deref() {
                Some("l") => -1.0,
                Some("r") => 1.0,
                _ => 0.0,
            },
            pair: bd.pair.unwrap_or(0),
        });
    }
    let floor = (0..N).find(|&y| (0..N).any(|z| (0..N).any(|x| owner[idx(x, y, z)] != NONE))).unwrap_or(0) as f32;
    Sliced {
        rig: Rig {
            plan: def.plan.clone(),
            bones,
            activities: def.activities.clone(),
            floor,
        },
        parts,
        owner,
    }
}

/// The same split again over a different colour grid: one part per bone, each
/// masked to the voxels that bone owns and pivoted on its joint. `bones.rs`
/// uses it to put a skeleton on the creature's own rig.
pub fn parts_from(sliced: &Sliced, cells: &[u8], voxel: f32, half_w: f32, height: f32) -> Vec<Model> {
    sliced
        .rig
        .bones
        .iter()
        .enumerate()
        .map(|(b, bone)| {
            let masked: Vec<u8> = cells.iter().zip(&sliced.owner).map(|(&c, &o)| if o == b as u8 { c } else { 0 }).collect();
            let mut m = models::from_cells(&masked, voxel, half_w, height);
            m.pivot = bone.joint;
            m
        })
        .collect()
}

/// The ops with each material replaced by `#bone`, dropping what can't change
/// which voxels are solid (recolouring) and ops whose colour didn't resolve.
fn retag(ops: &[Op], bone_of: &dyn Fn(&Option<String>) -> usize, colour_ok: &dyn Fn(&str) -> bool) -> Vec<Op> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let mut o = op.clone();
        match &mut o {
            Op::Box { mat, part, .. }
            | Op::Walls { mat, part, .. }
            | Op::Gable { mat, part, .. }
            | Op::Ellipsoid { mat, part, .. }
            | Op::Cylinder { mat, part, .. }
            | Op::Log { mat, part, .. }
            | Op::Disc { mat, part, .. }
            | Op::Set { mat, part, .. } => {
                if mat != "air" {
                    if !colour_ok(mat) {
                        continue;
                    }
                    *mat = format!("#{}", bone_of(part));
                }
            }
            Op::RecolourBelow { .. } | Op::Recolour { .. } => continue,
            Op::Repeat { ops, .. } => *ops = retag(ops, bone_of, colour_ok),
        }
        out.push(o);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{forward, Pose};
    use crate::library::Library;
    use glam::Quat;

    /// Every library creature with a rig, sliced as worldload does it.
    fn rigged() -> Vec<(String, Sliced, Vec<u8>, f32)> {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let mut out = Vec::new();
        for c in &lib.creatures {
            let Some(def) = &c.rig else { continue };
            let names: Vec<&String> = c.colors.keys().collect();
            let slot = |n: &str| names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1);
            let resolve = |n: &str| if n == "air" { Some(0) } else { slot(n) };
            let whole = shape::compile(&c.ops, [32, 32, 32], &resolve).mat;
            let s = slice(def, &c.ops, &whole, &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
            out.push((c.name.clone(), s, whole, c.voxel));
        }
        out
    }

    #[test]
    fn parts_add_up_to_the_whole() {
        let all = rigged();
        assert!(!all.is_empty(), "no rigged creatures in the library");
        for (name, s, whole, _) in &all {
            let solid = whole.iter().filter(|&&c| c != 0).count();
            let owned = s.owner.iter().filter(|&&o| o != NONE).count();
            assert_eq!(solid, owned, "{name}: every solid voxel has a bone");
            for (b, bone) in s.rig.bones.iter().enumerate() {
                let n = s.owner.iter().filter(|&&o| o == b as u8).count();
                assert!(n > 0, "{name}: bone {} has no voxels", bone.name);
            }
        }
    }

    /// At rest the parts, placed the way the shader places instances, put every
    /// voxel where the unsliced model put it.
    #[test]
    fn rest_pose_reproduces_the_whole_model() {
        for (name, s, whole, voxel) in rigged() {
            let pivot = Vec3::new(16.0, 0.0, 16.0);
            let (at, yaw) = (Vec3::new(5.0, 2.0, -3.0), 0.7);
            let yq = Quat::from_rotation_y(yaw);
            let mut fk = Vec::new();
            forward(&s.rig, &Pose::rest(s.rig.bones.len()), voxel, pivot, &mut fk);
            for y in 0..N {
                for z in 0..N {
                    for x in 0..N {
                        let i = idx(x, y, z);
                        if whole[i] == 0 {
                            continue;
                        }
                        let local = Vec3::new(x as f32, y as f32, z as f32);
                        let want = at + yq * ((local - pivot) * voxel);
                        let b = s.owner[i] as usize;
                        let (jp, q) = fk[b];
                        let part_pivot = s.parts[b].pivot;
                        // scene.wgsl: pos + yaw(qrot(rot, (local - pivot) * s)).
                        let got = (at + yq * jp) + yq * (q * ((local - part_pivot) * voxel));
                        assert!((got - want).length() < 1e-4, "{name}: voxel {x},{y},{z} of {} is off by {}", s.rig.bones[b].name, (got - want).length());
                    }
                }
            }
        }
    }
}
