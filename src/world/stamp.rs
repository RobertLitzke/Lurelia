//! What the generator presses into a chunk.
//!
//! `world/gen.rs` rasterises whatever a world put on the land without knowing
//! what kinds of thing there are: it hands each one the chunk and lets it
//! write itself (`Stamp::press`). A new kind of feature is a function here
//! and an arm in `Stamp for Tree`, not another branch in the generator.

use super::canopy::{Blob, Lumps};
use super::gen::{pidx, seg_box_dist, seg_dist2, Tree, TreeKind, PAD, PAD3, PADY};
use super::material::AIR;
use super::matlib;
use super::noise::{hash3, hash32, perlin3, unit};
use glam::Vec3;

/// The chunk a stamp writes into: the padded voxels, the array's minimum
/// corner in metres, the voxel edge at this level of detail, and the world's
/// leaf seed (a stamp mixes its own seed into it).
pub struct Press<'a> {
    pub v: &'a mut [u8; PAD3],
    pub o: Vec3,
    pub vsize: f32,
    pub leaf_seed: u32,
}

/// Something the generator presses into a chunk's voxels, writing only into
/// air. True when anything landed.
pub trait Stamp {
    fn press(&self, p: &mut Press) -> bool;
}

impl Stamp for Tree {
    fn press(&self, p: &mut Press) -> bool {
        match self.kind {
            TreeKind::Vine => vine(self, p.v, p.o, p.vsize),
            TreeKind::Crust => crust(self, p.v, p.o, p.vsize, false),
            TreeKind::Bracket => crust(self, p.v, p.o, p.vsize, true),
            TreeKind::Broad | TreeKind::Birch | TreeKind::Pine => woody(self, p.v, p.o, p.vsize, p.leaf_seed),
        }
    }
}

/// A trunk with branches and leaf blobs (broadleaf, birch, conifer), drawn into a padded chunk, writing only into air. Each part (trunk,
/// branches, each leaf blob, the pine cone) is rasterised over its own bounding
/// box, so most voxels are tested against one shape instead of all of them.
/// `o` is the padded array's minimum corner in metres. Returns true if anything
/// was written.
fn woody(t: &Tree, v: &mut [u8; PAD3], o: Vec3, vsize: f32, leaf_seed: u32) -> bool {
    let mut any = false;
    let mut herb = false;
    let mut raster = |lo: Vec3, hi: Vec3, v: &mut [u8; PAD3], f: &dyn Fn(Vec3) -> u8| {
        let lim = PAD as i32 - 1;
        let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
        let (x0, x1) = (idx(lo.x, o.x).max(0), idx(hi.x, o.x).min(lim));
        let (y0, y1) = (idx(lo.y, o.y).max(0), idx(hi.y, o.y).min(PADY as i32 - 1));
        let (z0, z1) = (idx(lo.z, o.z).max(0), idx(hi.z, o.z).min(lim));
        for py in y0..=y1 {
            let ym = o.y + (py as f32 + 0.5) * vsize;
            for pz in z0..=z1 {
                let zm = o.z + (pz as f32 + 0.5) * vsize;
                for px in x0..=x1 {
                    let i = pidx(px as usize, py as usize, pz as usize);
                    if v[i] != AIR {
                        continue;
                    }
                    let m = f(Vec3::new(o.x + (px as f32 + 0.5) * vsize, ym, zm));
                    if m != AIR {
                        v[i] = m;
                        any = true;
                    }
                }
            }
        }
    };

    // Where the trunk stands and how wide it is. A conifer's needle cone
    // has to agree with it: the leaf mesher works in cells of 2x2x2
    // voxels and calls a cell leaves only when five of its eight are
    // (mesh.rs BLOCK_MIN), so a shell of needles thinner than one cell
    // yields no leaf surface at all and the bark behind it shows through
    // the crown. Both passes below take their numbers from here.
    let tree = t.trunk_r >= 0.12;
    let snap = |m: f32| if tree { (m / vsize).round() * vsize } else { m };
    let stem_min: f32 = if tree { 0.19 } else { 0.0 };
    let c = Vec3::new(snap(t.pos.x), t.pos.y, snap(t.pos.z));
    // Root flare: lobes like buttress roots, gone by a few metres up.
    let flare_h = (t.flare * 4.0).max(1.0);
    let leaf_cell = 2.0 * vsize;
    // The trunk's widest radius at height `hy`: the taper, the foot and
    // the buttress lobes at their crest, matching the raster below.
    let trunk_at = |hy: f32| -> f32 {
        let taper = 1.0 - 0.3 * (hy / t.trunk_h).max(0.0);
        let mut flare = 0.14 * (1.0 - hy / 0.5).max(0.0);
        if t.flare > 0.0 {
            let k = (1.0 - hy / flare_h).max(0.0);
            flare += t.flare * k * k;
        }
        (t.trunk_r * taper).max(stem_min) + flare
    };
    let mut trunk_top = t.trunk_h;
    // Past the full-detail band a conifer's leaf cells are 1 m and 2 m
    // across, far too coarse to clothe a 30 cm trunk, so the trunk simply
    // stops where the crown starts. Nobody can see a conifer's trunk
    // through its own crown at that range, and a needle core that cannot
    // cover it is what broke distant firs into stacked lumps with bark
    // between them. At full detail the core sleeves (flora.rs `conifer`)
    // clothe it, so the trunk runs its whole length.
    if t.kind == TreeKind::Pine && vsize > 0.3 {
        trunk_top = trunk_top.min(t.canopy_lo + 0.4);
    }

    // Trunk, with a slight taper and a flare at the root. Centred on a voxel
    // corner and never under 0.19 m across, so a trunk is at least 2x2 voxels
    // all the way up instead of pinching to a single column (0.18 just
    // reaches the four voxel centres round a corner). Shrub stems (under
    // 0.12 m) keep their own size and place. Herbs (trunk_r 0) have none.
    if t.trunk_r > 0.0 {
        let rmin = stem_min;
        let rmax = t.trunk_r.max(rmin) + 0.14 + t.flare;
        let lobes = 5.0 + (t.seed % 3) as f32;
        let phase = (t.seed % 628) as f32 * 0.01;
        raster(c + Vec3::new(-rmax, -0.8, -rmax), c + Vec3::new(rmax, trunk_top, rmax), v, &|p: Vec3| {
            let hy = p.y - c.y;
            let (dx, dz) = (p.x - c.x, p.z - c.z);
            let taper = 1.0 - 0.3 * (hy / t.trunk_h).max(0.0);
            let mut flare = 0.14 * (1.0 - hy / 0.5).max(0.0);
            if t.flare > 0.0 {
                let k = (1.0 - hy / flare_h).max(0.0);
                let ang = dz.atan2(dx);
                flare += t.flare * k * k * (0.55 + 0.45 * (lobes * ang + phase).cos());
            }
            let r = (t.trunk_r * taper).max(rmin) + flare;
            if dx * dx + dz * dz < r * r {
                t.bark
            } else {
                AIR
            }
        });
    } else if t.leaf != AIR {
        // A herb: one voxel of its leaf material just above the ground,
        // which the sprite pass turns into the whole plant.
        let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
        let (px, pz) = (idx(t.pos.x, o.x), idx(t.pos.z, o.z));
        if (0..PAD as i32).contains(&px) && (0..PAD as i32).contains(&pz) {
            let mut py = idx(t.pos.y + 0.12, o.y);
            // Up out of the ground (the base sits 0.1 m into it), or out
            // of the heart of a rosette for a flower stalk's marker.
            for _ in 0..8 {
                if (0..PADY as i32).contains(&py) && v[pidx(px as usize, py as usize, pz as usize)] != AIR {
                    py += 1;
                }
            }
            // And down onto it: the smoothed ground the plant was placed
            // on can stand a little over its own column's top, which left
            // the marker hanging a voxel clear. The sprite pass only
            // grows a plant from a marker standing on something, so one
            // hanging drew nothing at all.
            for _ in 0..8 {
                if py > 0 && py < PADY as i32 && v[pidx(px as usize, (py - 1) as usize, pz as usize)] == AIR {
                    py -= 1;
                } else {
                    break;
                }
            }
            if (0..PADY as i32).contains(&py) && v[pidx(px as usize, py as usize, pz as usize)] == AIR {
                v[pidx(px as usize, py as usize, pz as usize)] = t.leaf;
                herb = true;
            }
        }
    }

    let ls = leaf_seed ^ t.seed;
    {
        {
            // Branches from the trunk top toward each outer blob, or the
            // tree's own limbs.
            if t.branches.is_empty() {
                let top = t.pos + Vec3::new(0.0, t.trunk_h - 0.4, 0.0);
                for b in t.blobs.iter().skip(1) {
                    let target = top + (b.c - top) * 0.75;
                    let pad = Vec3::splat(0.14);
                    let wood = matlib::grained(t.bark, matlib::Axis::of(target - top));
                    raster(top.min(target) - pad, top.max(target) + pad, v, &|p: Vec3| {
                        if seg_dist2(p, top, target) < 0.13 * 0.13 {
                            wood
                        } else {
                            AIR
                        }
                    });
                }
            } else {
                // At full detail (0.25 m) these are the old numbers; at the
                // coarser levels they grow with the voxel, or a limb thinner
                // than a voxel falls apart into floating pieces.
                let (thin, chain) = (0.8 * vsize, 0.72 * vsize);
                for &(a, b, r) in &t.branches {
                    let pad = Vec3::splat(if vsize > 0.3 { r.max(chain) + 0.05 } else { r + 0.05 });
                    // Grain along the limb (matlib::grained).
                    let wood = matlib::grained(t.bark, matlib::Axis::of(b - a));
                    raster(a.min(b) - pad, a.max(b) + pad, v, &|p: Vec3| {
                        // Thin limbs also take every voxel their centre line
                        // passes through (box distance, with a little margin),
                        // so a twig on a diagonal stays one face-connected
                        // chain instead of separate blobs once rounded.
                        if seg_dist2(p, a, b) < r * r || (r < thin && seg_box_dist(p, a, b) < chain) {
                            wood
                        } else {
                            AIR
                        }
                    });
                }
            }
            // A conifer's trunk is clothed in needles for its whole length
            // inside the crown. The leaf mesher calls a 2x2x2 cell leaves
            // only when five of its eight are (mesh.rs BLOCK_MIN), and a
            // clump is drawn solid only to about two thirds of its radius,
            // so the core sleeves alone cannot promise a whole cell of
            // needles round the wood — and a shell thinner than one cell
            // yields no leaf surface at all, which is how an orange window
            // used to open through a fir. The sleeves carry the look; this
            // carries the guarantee, at whatever the voxel is, and sits
            // inside them at full detail.
            if t.kind == TreeKind::Pine && t.leaf != AIR && trunk_top > t.canopy_lo {
                // One and a half cells, not one: a cell counts as leaves
                // only when five of its eight voxels are, so the needles
                // have to leave room for a whole cell clear of the bark,
                // not merely reach one cell past it. How much is needed
                // falls away with the trunk, though — a sapling's stem is
                // one voxel across and the cells beside it are leaves
                // already — so the margin is capped against the bark's own
                // width, or a sapling comes out a flat-topped slab of
                // needles wider than the tree (flora.rs `conifer_clothe`
                // is the same rule where the shapes are chosen).
                let rr = |hy: f32| {
                    let bark = trunk_at(hy.clamp(0.0, trunk_top));
                    bark + (leaf_cell * 1.5).min(2.5 * bark + 0.5 * vsize)
                };
                let (lo, hi) = (t.canopy_lo, trunk_top + leaf_cell);
                let wide = rr(lo);
                raster(c + Vec3::new(-wide, lo, -wide), c + Vec3::new(wide, hi, wide), v, &|p: Vec3| {
                    let r = rr(p.y - c.y);
                    let (dx, dz) = (p.x - c.x, p.z - c.z);
                    if dx * dx + dz * dz < r * r {
                        t.leaf
                    } else {
                        AIR
                    }
                });
            }
            // Each clump of foliage is one of four shapes (canopy.rs),
            // with a ragged edge, sparse gaps near the surface, and broad
            // lumps that keep the crown lobed from any distance.
            for (bi, b) in t.blobs.iter().enumerate() {
                let mat = if b.mat != AIR { b.mat } else { t.leaf };
                if mat == AIR || b.r <= 0.0 {
                    continue;
                }
                let outer = 1.32;
                let (lo, hi) = b.bounds(t.squash, outer);
                let lumps = Lumps::new(ls ^ (bi as u32).wrapping_mul(0x9e37_79b9), 0.20);
                raster(b.c + lo, b.c + hi, v, &|p: Vec3| {
                    let d = b.dist(p, t.squash);
                    if d > outer {
                        return AIR;
                    }
                    // The lumps move the whole skin in and out together,
                    // core as well as edge, so a notch cuts deep enough to
                    // read as one; they cost two dot products on a
                    // direction the distance already gave us.
                    let edge = lumps.at(p - b.c, d * b.r);
                    if d < 0.68 * edge {
                        return mat;
                    }
                    if d < edge + 0.30 * perlin3(p.x * 1.7, p.y * 1.7, p.z * 1.7, ls) {
                        if d > 0.82 * edge {
                            let h = hash3((p.x * 4.0) as i32, (p.y * 4.0) as i32, (p.z * 4.0) as i32, ls);
                            // The sparse skin is a full-detail effect: at
                            // a coarser level one dropped voxel is a
                            // metre-wide bite out of the crown, which is
                            // what left a tree's leaves floating clear of
                            // its trunk a few tens of metres off.
                            if unit(h) < 0.22 * (0.25 / vsize).min(1.0) {
                                return AIR;
                            }
                        }
                        return mat;
                    }
                    AIR
                });
            }
        }
    }
    any || herb
}

/// Moss, lichen or shelf fungus on a host (flora::place_crusts): each entry of
/// `branches` is (centre, the way it faces, radius). A patch is a decal: it
/// recolours the host's own outermost voxels, so moss adds nothing to the
/// trunk's girth, with its edge nibbled by noise so colonies are not discs. A
/// bracket does stand out from the trunk, and writes only into air so it
/// never eats into it. Both need the host drawn first, which it is: crusts
/// are added to `trees` after their hosts.
fn crust(t: &Tree, v: &mut [u8; PAD3], o: Vec3, vsize: f32, bracket: bool) -> bool {
    let mut any = false;
    let lim = PAD as i32 - 1;
    for (k, &(c, n, r)) in t.branches.iter().enumerate() {
        let s = hash32(t.seed ^ (k as u32).wrapping_mul(0x9e37_79b9));
        let side = Vec3::new(-n.z, 0.0, n.x);
        let reach = r + vsize;
        let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
        let (x0, x1) = (idx(c.x - reach, o.x).max(0), idx(c.x + reach, o.x).min(lim));
        let (z0, z1) = (idx(c.z - reach, o.z).max(0), idx(c.z + reach, o.z).min(lim));
        let (y0, y1) = (idx(c.y - reach, o.y).max(0), idx(c.y + reach, o.y).min(PADY as i32 - 1));
        for py in y0..=y1 {
            for pz in z0..=z1 {
                for px in x0..=x1 {
                    let i = pidx(px as usize, py as usize, pz as usize);
                    // A bracket fills air beside the trunk; a patch recolours
                    // the solid skin it lies on and adds no voxels at all.
                    if (v[i] != AIR) == bracket {
                        continue;
                    }
                    let p = Vec3::new(o.x + (px as f32 + 0.5) * vsize, o.y + (py as f32 + 0.5) * vsize, o.z + (pz as f32 + 0.5) * vsize);
                    let d = p - c;
                    let out = d.dot(n);
                    let keep = if bracket {
                        // A shelf: out from the face on its own side only,
                        // one or two voxels deep, narrowing as it reaches out.
                        let across = d.dot(side).abs();
                        out > -vsize && out < r && d.y.abs() < vsize * 0.6 && across < (r - out * 0.55).max(0.0)
                    } else {
                        // A decal: the host's own outer voxels, just inside
                        // where the face stands, wobbly at the rim.
                        let flat = (d - n * out).length();
                        let edge = r * (0.65 + 0.45 * unit(hash32(s ^ (px as u32) << 8 ^ (py as u32) << 16 ^ pz as u32)));
                        out > -vsize * 1.4 && out < vsize * 0.3 && flat < edge
                    };
                    if keep {
                        v[i] = t.leaf;
                        any = true;
                    }
                }
            }
        }
    }
    any
}

/// Hanging vine strands into a padded chunk, only into air: each strand one
/// voxel thick, swaying a little side to side as it falls, with a leafy knot
/// now and then. The strand material grows leaf clusters (mesh/sprites.rs).
fn vine(t: &Tree, v: &mut [u8; PAD3], o: Vec3, vsize: f32) -> bool {
    let mut any = false;
    let mut put = |p: Vec3, v: &mut [u8; PAD3]| {
        let (px, py, pz) = (((p.x - o.x) / vsize).floor() as i32, ((p.y - o.y) / vsize).floor() as i32, ((p.z - o.z) / vsize).floor() as i32);
        if px < 0 || pz < 0 || py < 0 || px >= PAD as i32 || pz >= PAD as i32 || py >= PADY as i32 {
            return;
        }
        let i = pidx(px as usize, py as usize, pz as usize);
        if v[i] == AIR {
            v[i] = t.leaf;
            any = true;
        }
    };
    // Pieces of one strand down a face (flora::down_face) follow each other a
    // metre apart: where the next one stands out from this one's foot (over a
    // bulge), a run of strand joins them.
    for w in t.blobs.windows(2) {
        let (a, b) = (w[0].c - Vec3::Y * w[0].r, w[1].c);
        let gap = glam::Vec2::new(b.x - a.x, b.z - a.z).length();
        if (a.y - b.y).abs() < 0.3 && gap > 0.2 && gap < 6.5 {
            let n = (gap / (vsize * 0.7)).ceil() as i32;
            for j in 0..=n {
                put(a.lerp(b, j as f32 / n as f32), v);
            }
        }
    }
    for (k, &Blob { c: top, r: len, .. }) in t.blobs.iter().enumerate() {
        let s = hash32(t.seed ^ (k as u32).wrapping_mul(0x9e37_79b9));
        let phase = unit(s) * std::f32::consts::TAU;
        let (ax, az) = (unit(s >> 3) - 0.5, unit(s >> 7) - 0.5);
        // A long strand bows out to one side as it falls, a sixteenth to a sixth
        // of its length (1 m at most); metre lengths down a cliff or wall
        // stay all but straight against the face.
        let a = (unit(s >> 11) * std::f32::consts::TAU, unit(s >> 15));
        let bow = Vec3::new(a.0.cos(), 0.0, a.0.sin()) * ((0.06 + 0.1 * a.1) * len).min(1.0) * (len / 2.0).min(1.0);
        let steps = (len / vsize).ceil() as i32;
        for j in 0..=steps {
            let d = j as f32 * vsize;
            let u = (d / len.max(0.5)).min(1.0);
            // Sway grows toward the free end.
            let sway = 0.18 * u * (len / 3.0).min(1.0);
            let p = top + bow * u * u + Vec3::new(ax * sway * (d * 0.9 + phase).sin() * 2.0, -d, az * sway * (d * 0.7 + phase).cos() * 2.0);
            put(p, v);
            if unit(hash32(s ^ j as u32)) < 0.18 && j + 2 < steps {
                let side = if unit(hash32(s ^ (j as u32) << 8)) < 0.5 { Vec3::X } else { Vec3::Z };
                put(p + side * vsize, v);
            }
        }
    }
    // Swags: a strand slung between two crowns, sagging `sag` metres at the
    // middle in a catenary, leafy here and there, with short ends dangling.
    for (k, &(a, b, sag)) in t.branches.iter().enumerate() {
        let s = hash32(t.seed ^ 0x7377_6167 ^ (k as u32).wrapping_mul(0x9e37_79b9));
        let c = 1.6f32;
        let span = (b - a).length();
        let steps = (span * 1.3 / vsize).ceil().max(2.0) as i32;
        for j in 0..=steps {
            let u = j as f32 / steps as f32;
            // 0 at the ends, -1 at the middle.
            let cat = ((c * (2.0 * u - 1.0)).cosh() - c.cosh()) / (c.cosh() - 1.0);
            let p = a + (b - a) * u + Vec3::Y * cat * sag;
            put(p, v);
            let h = hash32(s ^ j as u32);
            if unit(h) < 0.22 {
                let side = if unit(h >> 8) < 0.5 { Vec3::X } else { Vec3::Z };
                put(p + side * vsize, v);
            }
            // Now and then a dangle off the lower part of the swag.
            if j > 1 && j + 1 < steps && cat < -0.3 && unit(hash32(h ^ 0x64)) < 0.05 * vsize / 0.25 {
                let n = ((0.3 + 1.0 * unit(h >> 12)) / vsize).ceil() as i32;
                for i in 1..=n {
                    put(p - Vec3::Y * (i as f32 * vsize), v);
                }
            }
        }
    }
    any
}
