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

const NONE: u8 = u8::MAX;

/// The size of a creature's grid in voxels, x, y and z, laid out x fastest,
/// then z, then y (`shape::compile`'s order). Most are 32 on every side; a
/// big animal may have more (`CreatureDef::grid`), and then each of its parts
/// is meshed in a 32^3 window of its own (`WINDOW`), since a mesh's positions
/// are packed in five bits (mesh/quad.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dims(pub [i32; 3]);

/// The edge of the window a part is meshed in, voxels.
pub const WINDOW: i32 = 32;

impl Dims {
    pub const CUBE: Dims = Dims([WINDOW; 3]);
    pub fn len(self) -> usize {
        (self.0[0] * self.0[1] * self.0[2]) as usize
    }
    pub fn idx(self, x: i32, y: i32, z: i32) -> usize {
        ((y * self.0[2] + z) * self.0[0] + x) as usize
    }
    pub fn inside(self, x: i32, y: i32, z: i32) -> bool {
        (0..self.0[0]).contains(&x) && (0..self.0[1]).contains(&y) && (0..self.0[2]).contains(&z)
    }
    /// The x, y, z of index `i`.
    pub fn at(self, i: usize) -> (i32, i32, i32) {
        let i = i as i32;
        (i % self.0[0], i / (self.0[0] * self.0[2]), (i / self.0[0]) % self.0[2])
    }
    /// The whole model's pivot: the bottom centre.
    pub fn pivot(self) -> Vec3 {
        Vec3::new(self.0[0] as f32 * 0.5, 0.0, self.0[2] as f32 * 0.5)
    }
    /// The 32^3 window inside the grid that best holds a part whose voxels
    /// span `lo..=hi`: centred on it, moved in from the grid's edges. In a
    /// 32^3 grid it is the grid itself.
    pub fn window(self, lo: [i32; 3], hi: [i32; 3]) -> [i32; 3] {
        std::array::from_fn(|a| {
            let ext = hi[a] - lo[a] + 1;
            (lo[a] - (WINDOW - ext).max(0) / 2).clamp(0, (self.0[a] - WINDOW).max(0))
        })
    }
    /// The 32^3 cells of `cells` (a grid of this size) from `origin`, keeping
    /// only the voxels `keep` says.
    pub fn cut(self, cells: &[u8], origin: [i32; 3], keep: &dyn Fn(usize) -> bool) -> Vec<u8> {
        if self == Dims::CUBE {
            return cells.iter().enumerate().map(|(i, &c)| if keep(i) { c } else { 0 }).collect();
        }
        let mut out = vec![0u8; Dims::CUBE.len()];
        for y in 0..WINDOW {
            for z in 0..WINDOW {
                for x in 0..WINDOW {
                    let (gx, gy, gz) = (origin[0] + x, origin[1] + y, origin[2] + z);
                    if self.inside(gx, gy, gz) {
                        let i = self.idx(gx, gy, gz);
                        if keep(i) {
                            out[Dims::CUBE.idx(x, y, z)] = cells[i];
                        }
                    }
                }
            }
        }
        out
    }
}

/// The rig and its part models (bone `model` indices count from 0 in `parts`).
pub struct Sliced {
    pub rig: Rig,
    pub parts: Vec<Model>,
    /// Which bone owns each voxel of the whole grid (`u8::MAX` = empty).
    pub owner: Vec<u8>,
    pub dims: Dims,
    /// Where each part's 32^3 window starts in the grid (all zero in a 32^3
    /// grid): a part's own voxel (x, y, z) is the grid's origin + (x, y, z).
    pub origins: Vec<[i32; 3]>,
    /// Bones whose voxels reach further than one window holds; the voxels
    /// past it are left out of the part.
    pub oversize: Vec<String>,
}

/// What to tell the author of `who` (a creature, as the caller names it)
/// about a bone in `Sliced::oversize`.
pub fn oversize_problem(who: &str, bone: &str) -> String {
    format!("{who}: part {bone} is bigger than one 32^3 window; split it into more bones")
}

/// For each bone, the voxels furthest out from its joint in fourteen
/// directions (the six faces of a box and its eight corners) and the one
/// furthest from the joint of all, model voxel units, centres of voxels.
/// These are what a limp body lies on (anim/limp.rs): a ribcage on its flank
/// rests on the outside of the flank, not on the line of its spine.
///
/// `keep` picks which of the bone's voxels count: all of them for the
/// animal, only the bone voxels for its skeleton.
pub fn hulls(dims: Dims, owner: &[u8], keep: &dyn Fn(usize) -> bool, joints: &[Vec3]) -> Vec<Vec<Vec3>> {
    const R: f32 = 0.577_350_3;
    const DIRS: [Vec3; 14] = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::new(R, R, R),
        Vec3::new(R, R, -R),
        Vec3::new(R, -R, R),
        Vec3::new(R, -R, -R),
        Vec3::new(-R, R, R),
        Vec3::new(-R, R, -R),
        Vec3::new(-R, -R, R),
        Vec3::new(-R, -R, -R),
    ];
    let n = joints.len();
    // Per bone: the best voxel and its score in each direction, then the
    // furthest one from the joint.
    let mut best: Vec<[(f32, Vec3); 15]> = vec![[(f32::MIN, Vec3::ZERO); 15]; n];
    for (i, &o) in owner.iter().enumerate() {
        let b = o as usize;
        if b >= n || !keep(i) {
            continue;
        }
        let (x, y, z) = dims.at(i);
        let v = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
        let d = v - joints[b];
        let slot = &mut best[b];
        for (k, dir) in DIRS.iter().enumerate() {
            let s = d.dot(*dir);
            if s > slot[k].0 {
                slot[k] = (s, v);
            }
        }
        let far = d.length_squared();
        if far > slot[14].0 {
            slot[14] = (far, v);
        }
    }
    best.iter()
        .enumerate()
        .map(|(b, slot)| {
            let mut out: Vec<Vec3> = Vec::new();
            for &(s, v) in slot {
                // Nothing found, on the joint itself, or next to one already kept.
                if s == f32::MIN || v.distance(joints[b]) < 1.0 || out.iter().any(|o| o.distance(v) < 1.5) {
                    continue;
                }
                out.push(v);
            }
            out
        })
        .collect()
}

/// `whole` is the compiled colour grid (`dims`, x fastest, then z, then y, 0
/// = empty, else palette slot + 1, as `models::from_cells` takes it);
/// `colour_ok` says whether an op's material resolved in that compile.
#[allow(clippy::too_many_arguments)]
pub fn slice(def: &RigDef, ops: &[Op], whole: &[u8], dims: Dims, colour_ok: &dyn Fn(&str) -> bool, voxel: f32, half_w: f32, height: f32) -> Sliced {
    let [nx, ny, nz] = dims.0;
    let idx = |x: i32, y: i32, z: i32| dims.idx(x, y, z);
    let names: Vec<&str> = def.bones.iter().map(|b| b.name.as_str()).collect();
    let bone_of = |p: &Option<String>| -> usize { p.as_deref().and_then(|p| names.iter().position(|n| *n == p)).unwrap_or(0) };
    let tagged = retag(ops, &bone_of, colour_ok);
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        n.strip_prefix('#').and_then(|k| k.parse::<u8>().ok()).map(|k| k + 1)
    };
    let parts_grid = shape::compile(&tagged, dims.0, &resolve);

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
            for y in from[1].max(0)..to[1].min(ny) {
                for z in from[2].max(0)..to[2].min(nz) {
                    for x in from[0].max(0)..to[0].min(nx) {
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
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
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
            dims.pivot()
        }
    };
    let joints: Vec<Vec3> = def.bones.iter().enumerate().map(|(b, bd)| bd.joint.map(Vec3::from).unwrap_or_else(|| centre(b))).collect();

    let mut parts = Vec::with_capacity(def.bones.len());
    let mut bones = Vec::with_capacity(def.bones.len());
    let mut origins = Vec::with_capacity(def.bones.len());
    let mut oversize = Vec::new();
    for (b, bd) in def.bones.iter().enumerate() {
        let mut mine: Vec<bool> = owner.iter().map(|&o| o == b as u8).collect();
        // Overlap: grow into the parent's voxels, a layer at a time.
        if let Some(p) = parent[b] {
            for _ in 0..bd.overlap {
                let prev = mine.clone();
                for y in 0..ny {
                    for z in 0..nz {
                        for x in 0..nx {
                            let i = idx(x, y, z);
                            if prev[i] || owner[i] != p as u8 {
                                continue;
                            }
                            let near = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)].iter().any(|&(dx, dy, dz)| {
                                let (a, c, d) = (x + dx, y + dy, z + dz);
                                dims.inside(a, c, d) && prev[idx(a, c, d)]
                            });
                            if near {
                                mine[i] = true;
                            }
                        }
                    }
                }
            }
        }
        let (origin, fits) = window_of(dims, &mine);
        if !fits {
            oversize.push(bd.name.clone());
        }
        let cells = dims.cut(whole, origin, &|i| mine[i]);
        let mut m = models::from_cells(&cells, voxel, half_w, height).for_lod(&cells);
        m.pivot = joints[b] - Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
        parts.push(m);
        origins.push(origin);
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
            spread: bd.spread.map(Vec3::from),
            rigid: bd.rigid,
            loose: bd.loose,
            hull: Vec::new(),
        });
    }
    for (b, h) in hulls(dims, &owner, &|_| true, &joints).into_iter().enumerate() {
        bones[b].hull = h;
    }
    let floor = (0..ny).find(|&y| (0..nz).any(|z| (0..nx).any(|x| owner[idx(x, y, z)] != NONE))).unwrap_or(0) as f32;
    // The soft body: the bones with flesh and the box their voxels fill.
    let weight: Vec<f32> = def.bones.iter().map(|b| b.flesh.clamp(0.0, 1.0)).collect();
    let soft = if weight.iter().any(|w| *w > 0.0) {
        let mut ext: Vec<Option<(Vec3, Vec3)>> = vec![None; def.bones.len()];
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    let o = owner[idx(x, y, z)] as usize;
                    if let Some(e) = ext.get_mut(o) {
                        let (a, b) = (Vec3::new(x as f32, y as f32, z as f32), Vec3::new(x as f32 + 1.0, y as f32 + 1.0, z as f32 + 1.0));
                        *e = Some(e.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
                    }
                }
            }
        }
        super::flesh::Soft::new(weight, &ext)
    } else {
        None
    };
    Sliced {
        rig: Rig {
            plan: def.plan.clone(),
            bones,
            activities: def.activities.clone(),
            floor,
            soft,
            sit_up: def.sit_up.unwrap_or(0.0),
        },
        parts,
        owner,
        dims,
        origins,
        oversize,
    }
}

/// The window for the voxels `mine` marks, and whether they all fit in it.
fn window_of(dims: Dims, mine: &[bool]) -> ([i32; 3], bool) {
    if dims == Dims::CUBE {
        return ([0; 3], true);
    }
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for (i, _) in mine.iter().enumerate().filter(|(_, &m)| m) {
        let (x, y, z) = dims.at(i);
        for (a, v) in [x, y, z].into_iter().enumerate() {
            lo[a] = lo[a].min(v);
            hi[a] = hi[a].max(v);
        }
    }
    if lo[0] > hi[0] {
        return ([0; 3], true);
    }
    let fits = (0..3).all(|a| hi[a] - lo[a] < WINDOW);
    (dims.window(lo, hi), fits)
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
            let origin = sliced.origins[b];
            let masked = sliced.dims.cut(cells, origin, &|i| sliced.owner[i] == b as u8);
            let mut m = models::from_cells(&masked, voxel, half_w, height);
            m.pivot = bone.joint - Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
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
            | Op::Dish { mat, part, .. }
            | Op::Segment { mat, part, .. }
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
            let whole = shape::compile(&c.ops, c.grid, &resolve).mat;
            let s = slice(def, &c.ops, &whole, Dims(c.grid), &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
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
            let pivot = s.dims.pivot();
            let (at, yaw) = (Vec3::new(5.0, 2.0, -3.0), 0.7);
            let yq = Quat::from_rotation_y(yaw);
            let mut fk = Vec::new();
            forward(&s.rig, &Pose::rest(s.rig.bones.len()), voxel, pivot, &mut fk);
            let [nx, ny, nz] = s.dims.0;
            for y in 0..ny {
                for z in 0..nz {
                    for x in 0..nx {
                        let i = s.dims.idx(x, y, z);
                        if whole[i] == 0 {
                            continue;
                        }
                        let local = Vec3::new(x as f32, y as f32, z as f32);
                        let want = at + yq * ((local - pivot) * voxel);
                        let b = s.owner[i] as usize;
                        let (jp, q) = fk[b];
                        // The part's own voxel coordinates start at its window.
                        let o = s.origins[b];
                        let own = local - Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32);
                        let part_pivot = s.parts[b].pivot;
                        // scene.wgsl: pos + yaw(qrot(rot, (local - pivot) * s)).
                        let got = (at + yq * jp) + yq * (q * ((own - part_pivot) * voxel));
                        assert!((got - want).length() < 1e-4, "{name}: voxel {x},{y},{z} of {} is off by {}", s.rig.bones[b].name, (got - want).length());
                    }
                }
            }
        }
    }
}
