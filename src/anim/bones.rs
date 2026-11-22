//! The skeleton inside a creature, derived from the creature.
//!
//! Nothing here is authored. `slice.rs` already works out which bone owns each
//! voxel of a creature's 32³ body; this walks that grid and keeps, for each
//! bone, a thin structure well inside the flesh: a strut down a leg or a tail,
//! a ribcage and a spine through the body, a hollow skull.
//!
//! Every voxel it keeps is one that was solid in the living animal and was not
//! on its surface. That is the point, and `skeleton_stays_inside_the_animal`
//! asserts it for every rigged creature in the library: a bone can only ever
//! show through a flank, never stick out of one, so one day a thin or worn
//! animal can have part of its skeleton visible without anything moving.
//!
//! The parts come out on the creature's own rig, so a skeleton walks, lies,
//! sags and ragdolls through exactly the same code as the animal did.

use super::slice::Sliced;
use super::Rig;
use crate::anim::Role;
use crate::models::Model;
use glam::Vec3;

const N: i32 = 32;
const NONE: u8 = u8::MAX;

fn idx(x: i32, y: i32, z: i32) -> usize {
    ((y * N + z) * N + x) as usize
}

fn inside(x: i32, y: i32, z: i32) -> bool {
    (0..N).contains(&x) && (0..N).contains(&y) && (0..N).contains(&z)
}

/// How often a rib ring is kept along the body, in voxels.
const RIB_EVERY: i32 = 3;

/// The skeleton's colour grid (palette slot + 1, 0 empty, as
/// `models::from_cells` takes it) and its parts on `sliced.rig`.
///
/// `bone` and `bone_dark` are palette slots + 1.
pub fn skeletonise(sliced: &Sliced, bone: u8, bone_dark: u8, voxel: f32, half_w: f32, height: f32) -> (Vec<u8>, Vec<Model>) {
    let owner = &sliced.owner;
    let rig = &sliced.rig;
    let solid = |x: i32, y: i32, z: i32| inside(x, y, z) && owner[idx(x, y, z)] != NONE;
    // The core: solid voxels with no face on the outside world. Everything
    // below only ever removes voxels from this, which is what keeps the
    // skeleton inside the animal.
    let core: Vec<bool> = (0..owner.len())
        .map(|i| {
            if owner[i] == NONE {
                return false;
            }
            let (x, y, z) = ((i as i32) % N, (i as i32) / (N * N), ((i as i32) / N) % N);
            [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                .iter()
                .all(|&(a, b, c)| solid(x + a, y + b, z + c))
        })
        .collect();

    let mut cells = vec![0u8; owner.len()];
    for (b, bn) in rig.bones.iter().enumerate() {
        // Ears are cartilage and skin; there is no bone in them to keep.
        if bn.role == Role::Ear {
            continue;
        }
        let mine: Vec<usize> = (0..owner.len()).filter(|&i| owner[i] == b as u8 && core[i]).collect();
        if mine.is_empty() {
            continue;
        }
        let at = |i: usize| Vec3::new((i as i32 % N) as f32 + 0.5, (i as i32 / (N * N)) as f32 + 0.5, ((i as i32 / N) % N) as f32 + 0.5);
        let axis = axis_of(rig, b, &mine, &at);
        let along = |i: usize| (at(i) - bn.joint).dot(axis);
        match bn.role {
            Role::Root | Role::Spine => ribcage(&mine, &core, axis, &mut cells, bone, bone_dark),
            Role::Head => skull(&mine, &core, &mut cells, bone, bone_dark),
            // Legs, neck, tail, jaw, wing, arm: one strut down the middle.
            Role::Neck | Role::Jaw | Role::Tail | Role::Leg | Role::Arm | Role::Wing | Role::Ear => strut(&mine, &at, &along, axis, bn.joint, &mut cells, bone),
        }
    }

    let parts = super::slice::parts_from(sliced, &cells, voxel, half_w, height);
    (cells, parts)
}

/// The bone's long axis: toward the mean of its children's joints, or, with no
/// children, toward the far end of its own voxels.
fn axis_of(rig: &Rig, b: usize, mine: &[usize], at: &dyn Fn(usize) -> Vec3) -> Vec3 {
    let kids: Vec<Vec3> = rig.bones.iter().filter(|c| c.parent == Some(b)).map(|c| c.joint).collect();
    let joint = rig.bones[b].joint;
    let v = if kids.is_empty() {
        let far = mine.iter().copied().max_by(|&p, &q| (at(p) - joint).length().total_cmp(&(at(q) - joint).length()));
        far.map_or(Vec3::NEG_Y, |f| at(f) - joint)
    } else {
        kids.iter().copied().sum::<Vec3>() / kids.len() as f32 - joint
    };
    v.try_normalize().unwrap_or(Vec3::NEG_Y)
}

/// One or two voxels per slice, nearest the line through the joint: a limb,
/// a neck or a tail bone.
#[allow(clippy::too_many_arguments)]
fn strut(mine: &[usize], at: &dyn Fn(usize) -> Vec3, along: &dyn Fn(usize) -> f32, axis: Vec3, joint: Vec3, cells: &mut [u8], bone: u8) {
    let mut slices: std::collections::BTreeMap<i32, Vec<usize>> = Default::default();
    for &i in mine {
        slices.entry(along(i).round() as i32).or_default().push(i);
    }
    for (_, mut group) in slices {
        // Distance from the bone's own line.
        let off = |i: usize| {
            let d = at(i) - joint;
            (d - axis * d.dot(axis)).length()
        };
        group.sort_by(|&p, &q| off(p).total_cmp(&off(q)));
        for &i in group.iter().take(if group.len() > 12 { 2 } else { 1 }) {
            cells[i] = bone;
        }
    }
}

/// Ribs every few voxels along the body with a spine over the top of them.
#[allow(clippy::too_many_arguments)]
fn ribcage(mine: &[usize], core: &[bool], axis: Vec3, cells: &mut [u8], bone: u8, bone_dark: u8) {
    // Slice along whichever of the body's own axes runs down its length.
    let long = if axis.z.abs() >= axis.x.abs() && axis.z.abs() >= axis.y.abs() {
        2
    } else if axis.x.abs() >= axis.y.abs() {
        0
    } else {
        1
    };
    let coord = |i: usize| match long {
        0 => i as i32 % N,
        1 => i as i32 / (N * N),
        _ => (i as i32 / N) % N,
    };
    let (lo, hi) = (mine.iter().map(|&i| coord(i)).min().unwrap(), mine.iter().map(|&i| coord(i)).max().unwrap());
    let mut slices: std::collections::BTreeMap<i32, Vec<usize>> = Default::default();
    for &i in mine {
        slices.entry(coord(i)).or_default().push(i);
    }
    for (k, group) in &slices {
        // A ring at each end (shoulder and pelvis) and every few voxels between.
        let ring = *k == lo || *k == hi || (*k - lo) % RIB_EVERY == 0;
        // The spine: the highest core voxel of every slice, all the way along.
        if let Some(&top) = group.iter().max_by_key(|&&i| i as i32 / (N * N)) {
            cells[top] = bone_dark;
        }
        if !ring {
            continue;
        }
        for &i in group {
            // The ring is the slice's own edge: a core voxel with a
            // neighbour in this slice that is not core.
            let (x, y, z) = (i as i32 % N, i as i32 / (N * N), (i as i32 / N) % N);
            let edge = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                .iter()
                .filter(|&&(a, b, c)| [a, b, c][long] == 0)
                .any(|&(a, b, c)| !inside(x + a, y + b, z + c) || !core[idx(x + a, y + b, z + c)]);
            if edge && cells[i] == 0 {
                cells[i] = bone;
            }
        }
    }
}

/// A hollow skull: the head's core kept as a shell, with its lowest voxels
/// darker so the eye end reads as sockets rather than a solid block.
fn skull(mine: &[usize], core: &[bool], cells: &mut [u8], bone: u8, bone_dark: u8) {
    let ys: Vec<i32> = mine.iter().map(|&i| i as i32 / (N * N)).collect();
    let (lo, hi) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    let band = lo + (hi - lo) / 3;
    for &i in mine {
        let (x, y, z) = (i as i32 % N, i as i32 / (N * N), (i as i32 / N) % N);
        let shell = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
            .iter()
            .any(|&(a, b, c)| !inside(x + a, y + b, z + c) || !core[idx(x + a, y + b, z + c)]);
        if shell {
            cells[i] = if y <= band { bone_dark } else { bone };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every voxel of every creature's skeleton is one the living animal had,
    /// and is not on the animal's surface.
    #[test]
    fn skeleton_stays_inside_the_animal() {
        let (lib, problems) = crate::library::Library::load(&crate::library::data_root());
        assert!(problems.is_empty(), "{problems:?}");
        let mut checked = 0;
        for c in &lib.creatures {
            let Some(def) = c.rig.as_ref() else { continue };
            let names: Vec<&String> = c.colors.keys().collect();
            let resolve = |n: &str| -> Option<u8> {
                if n == "air" {
                    Some(0)
                } else {
                    names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
                }
            };
            let whole = crate::world::shape::compile(&c.ops, [32, 32, 32], &resolve).mat;
            let sliced = crate::anim::slice::slice(def, &c.ops, &whole, &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
            let (cells, parts) = skeletonise(&sliced, 65, 66, c.voxel, c.half_w, c.height);
            let owner = &sliced.owner;
            let solid = |x: i32, y: i32, z: i32| inside(x, y, z) && owner[idx(x, y, z)] != NONE;
            let mut kept = 0;
            for i in 0..cells.len() {
                if cells[i] == 0 {
                    continue;
                }
                kept += 1;
                let (x, y, z) = (i as i32 % N, i as i32 / (N * N), (i as i32 / N) % N);
                assert!(solid(x, y, z), "{}: skeleton voxel at {x},{y},{z} is not in the animal", c.name);
                let buried = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .all(|&(a, b, d)| solid(x + a, y + b, z + d));
                assert!(buried, "{}: skeleton voxel at {x},{y},{z} is on the animal's surface", c.name);
            }
            assert_eq!(parts.len(), sliced.rig.bones.len());
            // Anything of any size leaves bones that read. Thin creatures can
            // fall below the line, and the loader drops those rather than
            // drawing a handful of voxels (worldload.rs).
            if c.height >= 1.2 {
                assert!(kept >= MIN_BONE_VOXELS, "{}: only {kept} bone voxels, too few to read", c.name);
            }
            checked += 1;
        }
        assert!(checked > 20, "only {checked} rigged creatures checked");
    }
}

/// Fewer bone voxels than this and a skeleton reads as litter rather than as
/// an animal, so the loader leaves that creature without one however big it
/// is (`--validate` says so for an entry that asked for bones outright).
pub const MIN_BONE_VOXELS: usize = 24;

/// How many voxels a skeleton grid came out with.
pub fn voxels(cells: &[u8]) -> usize {
    cells.iter().filter(|&&c| c != 0).count()
}
