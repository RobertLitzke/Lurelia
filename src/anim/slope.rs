//! Sloped parts. A rig with `"slope": true` has its parts meshed here
//! instead of as square voxel faces: every corner is placed by surface nets
//! over the whole body's voxels (the way trees are rounded, mesh.rs
//! `round_corner`), so a box leg comes out bevelled and an ellipsoid body
//! smooth, and the corners where a part meets its parent are stretched.
//!
//! Stretching. A part draws no faces toward its parent's voxels or its
//! children's, so at rest the body is one closed surface with no caps at the
//! joints. Each corner of a part's faces that touches its parent's voxels
//! (the ring where a leg leaves the body) is marked to follow the parent's
//! rotation instead of its own, and the ring one voxel further along goes a
//! third of the way with it. The parent draws the same ring with its own
//! rotation, so when the joint bends the ring stays where both parts meet
//! and the faces between it and the rest of the part stretch over the gap.
//! The weights are two bits a corner in the size fields of the quad, which a
//! rounded quad leaves free (mesh/quad.rs `SEAM_SHIFT`); the parent's
//! rotation rides in the instance (`Instance::glow[1..]`, push_parts) and
//! the entity shaders blend the two (`entity_local` in common.wgsl).
//!
//! What it can't join: parts that touch without being parent and child (a
//! head resting on the body, two legs side by side) keep their caps toward
//! each other, as square parts do. A juvenile's head is drawn larger about
//! its own joint and its stretched ring is not, so a young animal's neck
//! seam can open a little.

use super::slice::{Dims, WINDOW};
use crate::mesh::{quad, Mesh, DIRS};
use crate::world::material::PALETTE_BASE;

const NONE: u8 = u8::MAX;

/// Whether sloped parts are on for a rig whose entry says `slope`:
/// `VOXEL_SLOPE_CREATURES=1` turns them on for every rigged creature and `0`
/// off for all, for comparisons.
pub fn wanted(entry: bool) -> bool {
    static FORCE: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    FORCE.get_or_init(|| std::env::var("VOXEL_SLOPE_CREATURES").ok().map(|v| v != "0")).unwrap_or(entry)
}

/// The whole body, as each part's mesher reads it.
pub struct Body<'a> {
    dims: Dims,
    /// Palette slot + 1 of each voxel, 0 empty.
    cells: &'a [u8],
    /// Bone owning each voxel (`NONE` empty).
    owner: &'a [u8],
    parent: &'a [Option<usize>],
    /// Solid voxels among the 3x3x3 round each voxel (0 to 27).
    dens: Vec<u8>,
}

impl<'a> Body<'a> {
    pub fn new(dims: Dims, cells: &'a [u8], owner: &'a [u8], parent: &'a [Option<usize>]) -> Self {
        let [nx, ny, nz] = dims.0;
        let mut dens = vec![0u8; dims.len()];
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    let mut n = 0;
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            for dx in -1..=1 {
                                let (a, b, c) = (x + dx, y + dy, z + dz);
                                n += (dims.inside(a, b, c) && owner[dims.idx(a, b, c)] != NONE) as u8;
                            }
                        }
                    }
                    dens[dims.idx(x, y, z)] = n;
                }
            }
        }
        Self { dims, cells, owner, parent, dens }
    }

    fn own(&self, p: [i32; 3]) -> u8 {
        if self.dims.inside(p[0], p[1], p[2]) {
            self.owner[self.dims.idx(p[0], p[1], p[2])]
        } else {
            NONE
        }
    }

    fn solid(&self, p: [i32; 3]) -> bool {
        self.own(p) != NONE
    }

    fn dens(&self, p: [i32; 3]) -> i32 {
        if self.dims.inside(p[0], p[1], p[2]) {
            self.dens[self.dims.idx(p[0], p[1], p[2])] as i32
        } else {
            0
        }
    }

    /// Corner offset in sixteenths of a voxel at lattice point `l` (the
    /// corner shared by voxels `l - 1 ..= l` on each axis): surface nets,
    /// each edge the surface crosses giving the point where the blurred
    /// density passes half way, as mesh.rs `round_corner` does for trees.
    fn corner(&self, l: [i32; 3]) -> [i32; 3] {
        let at = |i: i32, j: i32, k: i32| [l[0] - 1 + i, l[1] - 1 + j, l[2] - 1 + k];
        let mut s = [[[false; 2]; 2]; 2];
        let mut count = 0;
        for (i, si) in s.iter_mut().enumerate() {
            for (j, sj) in si.iter_mut().enumerate() {
                for (k, v) in sj.iter_mut().enumerate() {
                    *v = self.solid(at(i as i32, j as i32, k as i32));
                    count += *v as i32;
                }
            }
        }
        if count == 0 || count == 8 {
            return [0; 3];
        }
        let d = |i: usize, j: usize, k: usize| self.dens(at(i as i32, j as i32, k as i32));
        let cross = |da: i32, db: i32| -> i32 {
            let t = if da == db { 32 } else { ((27 - 2 * da) * 64 / (2 * (db - da))).clamp(16, 48) };
            t - 32
        };
        let (mut sum, mut n) = ([0i32; 3], 0i32);
        for a in 0..2 {
            for b in 0..2 {
                let (ha, hb) = (64 * a as i32 - 32, 64 * b as i32 - 32);
                if s[0][a][b] != s[1][a][b] {
                    sum[0] += cross(d(0, a, b), d(1, a, b));
                    sum[1] += ha;
                    sum[2] += hb;
                    n += 1;
                }
                if s[a][0][b] != s[a][1][b] {
                    sum[0] += ha;
                    sum[1] += cross(d(a, 0, b), d(a, 1, b));
                    sum[2] += hb;
                    n += 1;
                }
                if s[a][b][0] != s[a][b][1] {
                    sum[0] += ha;
                    sum[1] += hb;
                    sum[2] += cross(d(a, b, 0), d(a, b, 1));
                    n += 1;
                }
            }
        }
        // Mean in 64ths, to sixteenths, kept inside the cell.
        let mut o = sum.map(|v| (2 * v + 4 * n).div_euclid(8 * n).clamp(-8, 8));
        // Where the shape is one voxel thick along an axis (a fox's leg, an
        // ear) both sides move in, so a corner moves half as far there and
        // the part keeps three quarters of its width.
        for (a, oa) in o.iter_mut().enumerate() {
            let thin = (0..8).all(|i| {
                let v = at(i & 1, i >> 1 & 1, i >> 2 & 1);
                if !self.solid(v) {
                    return true;
                }
                let (mut lo, mut hi) = (v, v);
                lo[a] -= 1;
                hi[a] += 1;
                !self.solid(lo) && !self.solid(hi)
            });
            if thin {
                *oa /= 2;
            }
        }
        o
    }

    /// Whether any of the eight voxels round lattice point `l` is bone `q`'s.
    fn touches(&self, l: [i32; 3], q: u8) -> bool {
        (0..8).any(|i| self.own([l[0] - 1 + (i & 1), l[1] - 1 + (i >> 1 & 1), l[2] - 1 + (i >> 2 & 1)]) == q)
    }

    /// How far (0 to 3, in thirds) bone `b`'s corner at lattice point `l`
    /// follows its parent: all the way on the ring where the two meet, a
    /// third one voxel along.
    fn seam(&self, b: usize, l: [i32; 3]) -> u32 {
        let Some(p) = self.parent[b] else { return 0 };
        let p = p as u8;
        if self.touches(l, p) {
            return 3;
        }
        let near = (0..6).any(|k| {
            let mut m = l;
            m[k / 2] += if k % 2 == 0 { 1 } else { -1 };
            self.touches(m, p)
        });
        near as u32
    }

    /// Whether bones `a` and `b` are joined: one is the other's parent.
    fn joined(&self, a: usize, b: usize) -> bool {
        self.parent[a] == Some(b) || self.parent[b] == Some(a)
    }

    /// Bone `b`'s part, meshed in the 32^3 window from `origin`.
    pub fn mesh(&self, b: usize, origin: [i32; 3]) -> Mesh {
        let mut out = Mesh::default();
        let bb = b as u8;
        for y in 0..WINDOW {
            for z in 0..WINDOW {
                for x in 0..WINDOW {
                    let g = [origin[0] + x, origin[1] + y, origin[2] + z];
                    if self.own(g) != bb {
                        continue;
                    }
                    let colour = self.cells[self.dims.idx(g[0], g[1], g[2])];
                    for (d, &(na, sign, ua, va)) in DIRS.iter().enumerate() {
                        let mut q = g;
                        q[na] += sign;
                        let o = self.own(q);
                        if o == bb || (o != NONE && self.joined(b, o as usize)) {
                            continue;
                        }
                        let step = |p: [i32; 3], a: usize, s: i32| {
                            let mut p = p;
                            p[a] += s;
                            p
                        };
                        let sv = |p: [i32; 3]| self.solid(p);
                        let (um, up, vm, vp) = (sv(step(q, ua, -1)), sv(step(q, ua, 1)), sv(step(q, va, -1)), sv(step(q, va, 1)));
                        let diag = |su: i32, svv: i32| sv(step(step(q, ua, su), va, svv));
                        let ao = [ao(um, vm, diag(-1, -1)), ao(up, vm, diag(1, -1)), ao(up, vp, diag(1, 1)), ao(um, vp, diag(-1, 1))];
                        let mut offs = [[0i32; 3]; 4];
                        let mut seams = 0u32;
                        for (k, ok) in offs.iter_mut().enumerate() {
                            let mut l = g;
                            if sign > 0 {
                                l[na] += 1;
                            }
                            l[ua] += (k == 1 || k == 2) as i32;
                            l[va] += (k >= 2) as i32;
                            *ok = self.corner(l);
                            seams |= self.seam(b, l) << (2 * k);
                        }
                        let (w2, w3) = crate::mesh::pack_round(&offs);
                        let pos = [x as u32, y as u32, z as u32];
                        let w0 = quad::word0(pos, 1, 1, d, true) | seams << quad::SEAM_SHIFT;
                        let a = ao[0] | ao[1] << 2 | ao[2] << 4 | ao[3] << 6;
                        out.quads.push([w0, (PALETTE_BASE + colour - 1) as u32 | a << quad::AO_SHIFT, w2, w3]);
                        out.dir_counts[d] += 1;
                    }
                }
            }
        }
        // Grouped by direction, as the mesher leaves them.
        out.quads.sort_by_key(quad::dir);
        out
    }
}

fn ao(s1: bool, s2: bool, c: bool) -> u32 {
    if s1 && s2 {
        0
    } else {
        3 - (s1 as u32 + s2 as u32 + c as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body box with a leg box under it: no faces between them, and the
    /// leg's corners where it leaves the body follow the body.
    #[test]
    fn a_leg_joins_its_body() {
        let dims = Dims::CUBE;
        let mut owner = vec![NONE; dims.len()];
        let mut cells = vec![0u8; dims.len()];
        for y in 0..12 {
            for z in 8..24 {
                for x in 8..24 {
                    let body = y >= 6;
                    let leg = !body && (10..13).contains(&x) && (10..13).contains(&z);
                    if body || leg {
                        let i = dims.idx(x, y, z);
                        owner[i] = if body { 0 } else { 1 };
                        cells[i] = 1;
                    }
                }
            }
        }
        let parent = [None, Some(0)];
        let body = Body::new(dims, &cells, &owner, &parent);
        let leg = body.mesh(1, [0; 3]);
        // Up faces of the leg would sit against the body: none drawn.
        assert!(leg.quads.iter().all(|q| quad::dir(q) != 2), "leg drew a face into the body");
        // The body draws none down into the leg either.
        let trunk = body.mesh(0, [0; 3]);
        let downs = trunk.quads.iter().filter(|q| quad::dir(q) == 3).count();
        assert_eq!(downs, 16 * 16 - 9, "body's underside less the leg");
        // Side faces of the top leg voxels: top corners at 3, bottom at 1.
        let top = leg.quads.iter().filter(|q| quad::pos(q)[1] == 5 && quad::dir(q) == 0).collect::<Vec<_>>();
        assert!(!top.is_empty());
        for q in top {
            let s = q[0] >> quad::SEAM_SHIFT & 0xff;
            let ws: Vec<u32> = (0..4).map(|k| s >> (2 * k) & 3).collect();
            assert!(ws.contains(&3) && ws.iter().all(|&w| w == 3 || w == 1), "weights {ws:?}");
        }
        // Low on the leg nothing follows the body.
        assert!(leg.quads.iter().filter(|q| quad::pos(q)[1] < 3).all(|q| q[0] >> quad::SEAM_SHIFT & 0xff == 0));
    }

    /// A corner on the flat middle of a big face doesn't move; one on an
    /// edge is pulled in.
    #[test]
    fn edges_round_and_faces_stay_flat() {
        let dims = Dims::CUBE;
        let mut owner = vec![NONE; dims.len()];
        let cells = vec![1u8; dims.len()];
        for y in 4..20 {
            for z in 4..20 {
                for x in 4..20 {
                    owner[dims.idx(x, y, z)] = 0;
                }
            }
        }
        let parent = [None];
        let body = Body::new(dims, &cells, &owner, &parent);
        assert_eq!(body.corner([12, 20, 12]), [0, 0, 0]);
        let e = body.corner([4, 20, 12]);
        assert!(e[0] > 0 && e[1] < 0 && e[2] == 0, "edge corner {e:?}");
    }

    /// A leg one voxel wide keeps three quarters of its width.
    #[test]
    fn thin_legs_stay_thick() {
        let dims = Dims::CUBE;
        let mut owner = vec![NONE; dims.len()];
        let cells = vec![1u8; dims.len()];
        for y in 2..20 {
            owner[dims.idx(10, y, 10)] = 0;
        }
        let parent = [None];
        let body = Body::new(dims, &cells, &owner, &parent);
        let c = body.corner([11, 10, 11]);
        assert_eq!((c[0], c[2]), (-2, -2), "corner {c:?}");
    }
}
