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

/// A root flare at least this wide (metres, `buttress` on a plant) is drawn
/// as separate plank buttresses rather than one fluted cone.
pub const PLANK_FLARE: f32 = 1.5;

/// How far a plank buttress runs out past the trunk, as a multiple of the
/// flare: a fin is a long low wall, not a lobe.
pub const PLANK_REACH: f32 = 1.8;

/// A trunk's widest radius at `hy` m over its base: the taper, the foot and
/// the buttress lobes or plank fins at their crest. `woody` draws the trunk
/// inside it; an event that builds round a trunk (a canopy platform, a stair
/// wound up it) keeps outside it. `stem_min` is the thinnest the trunk is
/// drawn at the voxel size in hand (0 for a big tree).
pub fn trunk_reach(t: &Tree, hy: f32, stem_min: f32) -> f32 {
    let plank = t.flare >= PLANK_FLARE;
    let flare_h = if plank { t.flare * 2.2 } else { (t.flare * 4.0).max(1.0) };
    let taper = 1.0 - t.taper * (hy / t.trunk_h).max(0.0);
    let mut flare = 0.14 * (1.0 - hy / 0.5).max(0.0);
    if plank {
        let k = (1.0 - hy / flare_h).max(0.0);
        flare += t.flare * PLANK_REACH * k.powf(1.6);
    } else if t.flare > 0.0 {
        let k = (1.0 - hy / flare_h).max(0.0);
        flare += t.flare * k * k;
    }
    (t.trunk_r * taper).max(stem_min) + flare
}

/// How deep the lumps on a crown clump cut (`canopy::Lumps`).
const LUMP_AMP: f32 = 0.20;

/// The most `noise::perlin3` ever answers in either direction: 1.036 at
/// worst over every choice of lattice gradients, rounded up.
const PERLIN3_MAX: f32 = 1.1;

/// Voxels to a side of the blocks `raster_into` settles at once. Measured on
/// a grove of 80 m trees: 4 is fastest; 8 settles too few blocks, and
/// splitting an unsettled block further costs more in classifier calls than
/// it saves in voxel tests.
const BLOCK: i32 = 4;

/// Rasterise `f` over the box `lo`..`hi` (metres) into the padded chunk,
/// writing only into air. The box is walked in blocks of `BLOCK` voxels a
/// side, and `class` is shown each block's extent first (the centres of its
/// lowest and highest voxels): `Some(AIR)` skips the block, `Some(m)` fills
/// its air with `m`, `None` tests each voxel with `f`. A classifier may only answer
/// where `f` would have given that same answer for every voxel in the
/// block. Returns whether anything was written.
#[allow(clippy::too_many_arguments)]
fn raster_into(v: &mut [u8; PAD3], o: Vec3, vsize: f32, lo: Vec3, hi: Vec3, class: &dyn Fn(Vec3, Vec3) -> Option<u8>, f: &dyn Fn(Vec3) -> u8) -> bool {
    let lim = PAD as i32 - 1;
    let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
    let (x0, x1) = (idx(lo.x, o.x).max(0), idx(hi.x, o.x).min(lim));
    let (y0, y1) = (idx(lo.y, o.y).max(0), idx(hi.y, o.y).min(PADY as i32 - 1));
    let (z0, z1) = (idx(lo.z, o.z).max(0), idx(hi.z, o.z).min(lim));
    let mut st = Walk { v, o, vsize, class, f, any: false };
    let mut by = y0;
    while by <= y1 {
        let ey = (by + BLOCK - 1).min(y1);
        let mut bz = z0;
        while bz <= z1 {
            let ez = (bz + BLOCK - 1).min(z1);
            let mut bx = x0;
            while bx <= x1 {
                let ex = (bx + BLOCK - 1).min(x1);
                st.block([bx, by, bz], [ex, ey, ez]);
                bx = ex + 1;
            }
            bz = ez + 1;
        }
        by = ey + 1;
    }
    st.any
}

/// `raster_into`'s state while it walks the blocks.
struct Walk<'a> {
    v: &'a mut [u8; PAD3],
    o: Vec3,
    vsize: f32,
    class: &'a dyn Fn(Vec3, Vec3) -> Option<u8>,
    f: &'a dyn Fn(Vec3) -> u8,
    any: bool,
}

impl Walk<'_> {
    fn at(&self, p: [i32; 3]) -> Vec3 {
        self.o + (Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) + 0.5) * self.vsize
    }

    /// Settle the block of voxels `a..=b`, or test each of its voxels.
    fn block(&mut self, a: [i32; 3], b: [i32; 3]) {
        let settled = (self.class)(self.at(a), self.at(b));
        if settled == Some(AIR) {
            return;
        }
        for py in a[1]..=b[1] {
            for pz in a[2]..=b[2] {
                for px in a[0]..=b[0] {
                    let i = pidx(px as usize, py as usize, pz as usize);
                    if self.v[i] != AIR {
                        continue;
                    }
                    let m = match settled {
                        Some(m) => m,
                        None => (self.f)(self.at([px, py, pz])),
                    };
                    if m != AIR {
                        self.v[i] = m;
                        self.any = true;
                    }
                }
            }
        }
    }
}

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
    let any = std::cell::Cell::new(false);
    let mut herb = false;
    let raster = |lo: Vec3, hi: Vec3, v: &mut [u8; PAD3], f: &dyn Fn(Vec3) -> u8| {
        if raster_into(v, o, vsize, lo, hi, &|_, _| None, f) {
            any.set(true);
        }
    };
    // The same, with a classifier that settles whole blocks at once.
    let raster_b = |lo: Vec3, hi: Vec3, v: &mut [u8; PAD3], class: &dyn Fn(Vec3, Vec3) -> Option<u8>, f: &dyn Fn(Vec3) -> u8| raster_into(v, o, vsize, lo, hi, class, f);

    // Where the trunk stands and how wide it is. A conifer's needle cone
    // has to agree with it: the leaf mesher works in cells of 2x2x2
    // voxels and calls a cell leaves only when five of its eight are
    // (mesh.rs BLOCK_MIN), so a shell of needles thinner than one cell
    // yields no leaf surface at all and the bark behind it shows through
    // the crown. Both passes below take their numbers from here.
    let tree = t.trunk_r >= 0.12;
    // Past full detail a trunk centred on a voxel corner needs a radius of
    // 0.71 voxels to reach the four voxel centres round it, and a thinner
    // one (a drowned spruce's 0.3 m at 0.5 m or 1 m voxels) drew nothing,
    // leaving its limbs, which grow with the voxel, floating in the air.
    // Such a trunk is centred on a voxel instead and is at least that one
    // column.
    let slim = tree && vsize > 0.3 && t.trunk_r < 0.75 * vsize;
    let snap = |m: f32| {
        if slim {
            ((m / vsize).floor() + 0.5) * vsize
        } else if tree {
            (m / vsize).round() * vsize
        } else {
            m
        }
    };
    let stem_min: f32 = if slim {
        0.55 * vsize
    } else if tree {
        0.19
    } else {
        0.0
    };
    let c = Vec3::new(snap(t.pos.x), t.pos.y, snap(t.pos.z));
    // Root flare: lobes like buttress roots, gone by a few metres up. A big
    // flare is drawn as plank buttresses instead (`PLANK_FLARE`): thin walls
    // standing out from the trunk, rather than one fluted cone, which at
    // rainforest sizes reads as a plinth the tree stands on.
    let plank = t.flare >= PLANK_FLARE;
    let flare_h = if plank { t.flare * 2.2 } else { (t.flare * 4.0).max(1.0) };
    let leaf_cell = 2.0 * vsize;
    // The trunk's widest radius at height `hy`: the taper, the foot and
    // the buttress lobes at their crest, matching the raster below.
    let trunk_at = |hy: f32| -> f32 { trunk_reach(t, hy, stem_min) };
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
        let rmax = t.trunk_r.max(rmin) + 0.14 + t.flare * if plank { PLANK_REACH } else { 1.0 };
        let lobes = 5.0 + (t.seed % 3) as f32;
        let phase = (t.seed % 628) as f32 * 0.01;
        // Plank buttresses: each fin a direction, a reach (a fraction of the
        // flare) and a height of its own, so no two trees stand alike.
        let fins: Vec<(f32, f32, f32, f32)> = if plank {
            let n = 4 + (t.seed % 4) as usize;
            (0..n)
                .map(|i| {
                    let h = hash32(t.seed ^ (i as u32).wrapping_mul(0x9e37_79b9));
                    let a = phase + i as f32 * std::f32::consts::TAU / n as f32 + (unit(h) - 0.5) * 0.9;
                    let reach = 0.55 + 0.45 * unit(hash32(h ^ 0x68e3));
                    (a.cos(), a.sin(), reach, 0.7 + 0.3 * unit(hash32(h ^ 0x2c1b)))
                })
                .collect()
        } else {
            Vec::new()
        };
        // A fin is at least this thick, so it survives the coarser voxels
        // of the far bands instead of breaking into a dotted line.
        let fin_min = 0.6 * vsize;
        // A block whose every point is nearer the axis than the trunk is
        // at its narrowest (the block's top) is all bark; one whose every
        // point is further out than anything reaches at the block's foot,
        // fins included, is all air. Exact either way: the taper, the flare
        // and the fins only shrink as they climb.
        let trunk_class = |a: Vec3, b: Vec3| -> Option<u8> {
            let m = (a + b) * 0.5;
            let rc = ((m.x - c.x).powi(2) + (m.z - c.z).powi(2)).sqrt();
            let hh = (((b.x - a.x) * 0.5).powi(2) + ((b.z - a.z) * 0.5).powi(2)).sqrt();
            let (hy_lo, hy_hi) = (a.y - c.y, b.y - c.y);
            let inner = t.trunk_r * (1.0 - t.taper * (hy_hi / t.trunk_h).max(0.0));
            if rc + hh < inner {
                return Some(t.bark);
            }
            let k = (1.0 - hy_lo / flare_h).max(0.0);
            let outer = trunk_at(hy_lo) + t.flare * 0.12 * k * k + if plank { 0.5 + fin_min } else { 0.0 } + 0.01;
            if rc - hh > outer {
                return Some(AIR);
            }
            None
        };
        if raster_b(c + Vec3::new(-rmax, -0.8, -rmax), c + Vec3::new(rmax, trunk_top, rmax), v, &trunk_class, &|p: Vec3| {
            let hy = p.y - c.y;
            let (dx, dz) = (p.x - c.x, p.z - c.z);
            let taper = 1.0 - t.taper * (hy / t.trunk_h).max(0.0);
            let mut flare = 0.14 * (1.0 - hy / 0.5).max(0.0);
            if plank {
                let core = t.trunk_r * taper;
                // A little swell of the trunk itself, so the fins grow out
                // of it rather than being stuck on.
                let k = (1.0 - hy / flare_h).max(0.0);
                let r = core + flare + t.flare * 0.12 * k * k;
                let d2 = dx * dx + dz * dz;
                if d2 < r * r {
                    return t.bark;
                }
                for &(fx, fz, reach, tall) in &fins {
                    let along = dx * fx + dz * fz;
                    if along <= 0.0 {
                        continue;
                    }
                    let k = (1.0 - hy / (flare_h * tall)).max(0.0);
                    if k <= 0.0 {
                        continue;
                    }
                    // The fin's outer edge falls in a concave sweep from
                    // high on the trunk to far out on the ground.
                    let out = core + t.flare * PLANK_REACH * reach * k.powf(1.6);
                    if along > out {
                        continue;
                    }
                    let across = (dx * fz - dz * fx).abs();
                    // Thickest where it leaves the trunk, thinning outward.
                    let edge = 1.0 - ((along - core).max(0.0) / (out - core).max(0.01));
                    let half = (0.18 + 0.22 * edge + 0.08 * k).max(fin_min * 0.5);
                    if across < half {
                        return t.bark;
                    }
                }
                return AIR;
            }
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
        }) {
            any.set(true);
        }
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
                // A thin stem (`tube` bark: canes, bamboo) is drawn as one
                // line of voxels, joined at their corners where it leans:
                // the mesher fits a slanted tube through such a line
                // (mesh/lean.rs), and the face-connected chain below would
                // make it two or three voxels thick on a diagonal.
                // So is a limb thinner than the plant's `thin_limbs` (sticks),
                // whatever its bark.
                let tube = matlib::tubes(t.bark);
                // Past full detail a conifer draws no limbs at all, for the
                // same reason its trunk stops at the crown base above. A
                // whorl's limb is 5 to 14 cm, but here it is drawn as the
                // whole chain of 0.5 m or 1 m voxels its centre line crosses,
                // and that chain stands out through the needle sleeve meant
                // to hide it: distant Sitka spruce came out with rows of
                // rust-coloured blocks across their crowns.
                let limbs = !(t.kind == TreeKind::Pine && vsize > 0.3);
                for &(a, b, r) in t.branches.iter().filter(|_| limbs) {
                    if tube || r < t.sticks {
                        let rr = r.max(0.62 * vsize);
                        let pad = Vec3::splat(rr + 0.05);
                        let wood = matlib::grained(t.bark, matlib::Axis::of(b - a));
                        raster(a.min(b) - pad, a.max(b) + pad, v, &|p: Vec3| if seg_dist2(p, a, b) < rr * rr { wood } else { AIR });
                        continue;
                    }
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
            // Past full detail a conifer's needles also run up its axis as a
            // spine one leaf cell across (a 2x2 column of 0.5 m voxels, which
            // the leaf mesher takes as a cell, or one column of 1 m voxels),
            // as high as the clumps cover the axis. Near the tip the leader's
            // needle core is under 0.3 m solid, so at these voxels it hit a
            // voxel centre in some layers and not others, and the top of
            // every spruce broke into flecks hanging over the crown. The
            // spine keeps what is there in one piece without widening it.
            if t.kind == TreeKind::Pine && t.leaf != AIR && vsize > 0.3 {
                let cell = if vsize < 0.75 { 2.0 * vsize } else { vsize };
                let (x0, z0) = ((t.pos.x / cell).floor() * cell, (t.pos.z / cell).floor() * cell);
                let e = 0.01 * vsize;
                raster(Vec3::new(x0 + e, t.pos.y + t.canopy_lo, z0 + e), Vec3::new(x0 + cell - e, t.pos.y + t.top, z0 + cell - e), v, &|p: Vec3| {
                    let q = Vec3::new(t.pos.x, p.y, t.pos.z);
                    if t.blobs.iter().any(|b| b.dist(q, t.squash) < 1.0) {
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
                let lumps = Lumps::new(ls ^ (bi as u32).wrapping_mul(0x9e37_79b9), LUMP_AMP);
                // How fast the normalised distance can change per metre
                // (canopy.rs `Blob::max_rate`): a block whose centre is
                // further than that allows from the shape is wholly outside
                // it, and one deep enough in is wholly inside the solid
                // core, where every voxel would come out leaf anyway. How
                // deep is enough depends on how far in the lumps can pull
                // the edge over the block (`Lumps::least`). Only the blocks
                // across the skin are tested voxel by voxel.
                let rate = b.max_rate(t.squash);
                let blob_class = |a: Vec3, z: Vec3| -> Option<u8> {
                    let m = (a + z) * 0.5;
                    let half = (z - a).length() * 0.5;
                    let d = b.dist(m, t.squash);
                    let slack = rate * half;
                    if d - slack > outer {
                        return Some(AIR);
                    }
                    let low = d - slack;
                    if low > 0.0 {
                        let stretch = ((m - b.c).length() + half) / (low * b.r);
                        if d + slack < 0.68 * Lumps::least(LUMP_AMP, stretch) {
                            return Some(mat);
                        }
                    }
                    None
                };
                if raster_b(b.c + lo, b.c + hi, v, &blob_class, &|p: Vec3| {
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
                    // The ragged edge moves the surface by at most
                    // 0.3 x PERLIN3_MAX, so outside that band the noise
                    // cannot change the answer and is not looked up.
                    let rag = 0.30 * PERLIN3_MAX;
                    if d >= edge + rag {
                        return AIR;
                    }
                    if d < edge - rag || d < edge + 0.30 * perlin3(p.x * 1.7, p.y * 1.7, p.z * 1.7, ls) {
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
                }) {
                    any.set(true);
                }
            }
        }
    }
    any.get() || herb
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

#[cfg(test)]
mod tip_tests {
    use super::*;
    use crate::library::Library;
    use crate::world::gen::{M, MY};
    use crate::world::material::{BARK, LEAVES};
    use std::path::Path;

    /// A tree pressed whole into one grid of `vsize` voxels, by stacking
    /// presses of chunk-sized windows. Returns (grid, nx, ny, origin).
    fn press_whole(t: &Tree, vsize: f32) -> (Vec<u8>, usize, usize, Vec3) {
        let n = 32usize;
        // On even world voxels, as the leaf cells are.
        let even = |m: f32, back: f32| ((m / vsize - back) / 2.0).floor() * 2.0 * vsize;
        let o = Vec3::new(even(t.pos.x, 16.0), even(t.pos.y, 2.0), even(t.pos.z, 16.0));
        let ny = ((t.top + 4.0) / vsize).ceil() as usize;
        let mut g = vec![AIR; n * n * ny];
        let mut y0 = 0usize;
        while y0 < ny {
            let mut v = Box::new([AIR; PAD3]);
            let po = o + Vec3::new(-(M as f32) * vsize, (y0 as f32 - MY as f32) * vsize, -(M as f32) * vsize);
            t.press(&mut Press { v: &mut v, o: po, vsize, leaf_seed: 7 });
            for y in 0..32usize {
                if y0 + y >= ny {
                    break;
                }
                for z in 0..n {
                    for x in 0..n {
                        g[((y0 + y) * n + z) * n + x] = v[pidx(x + M, y + MY, z + M)];
                    }
                }
            }
            y0 += 32;
        }
        (g, n, ny, o)
    }

    /// Leaf voxels (26-connected) outside the largest leaf component, and
    /// the highest of them above the main mass's top, in metres.
    fn strays(g: &[u8], n: usize, ny: usize, leaf: u8) -> (usize, usize) {
        let mut comp = vec![0u32; g.len()];
        let mut sizes = vec![0usize];
        for s in 0..g.len() {
            if g[s] != leaf || comp[s] != 0 {
                continue;
            }
            let id = sizes.len() as u32;
            sizes.push(0);
            let mut stack = vec![s];
            comp[s] = id;
            while let Some(i) = stack.pop() {
                sizes[id as usize] += 1;
                let (x, z, y) = (i % n, (i / n) % n, i / (n * n));
                for dy in -1i32..=1 {
                    for dz in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let (xx, yy, zz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                            if xx < 0 || zz < 0 || yy < 0 || xx >= n as i32 || zz >= n as i32 || yy >= ny as i32 {
                                continue;
                            }
                            let j = (yy as usize * n + zz as usize) * n + xx as usize;
                            if g[j] == leaf && comp[j] == 0 {
                                comp[j] = id;
                                stack.push(j);
                            }
                        }
                    }
                }
            }
        }
        let main = (1..sizes.len()).max_by_key(|&k| sizes[k]).unwrap_or(0);
        let total: usize = sizes.iter().sum();
        (total - sizes.get(main).copied().unwrap_or(0), sizes.len() - 2.min(sizes.len()))
    }

    /// Past full detail a spruce's crown is drawn in one piece: no needles
    /// left hanging over the tip where the leader's thin core missed the
    /// coarse voxels (the spine in `woody`). Checked on what the mesher
    /// draws: leaf cells of 2x2x2 voxels at 0.5 m, the voxels themselves at
    /// 1 m. Full detail is not held to it; its thin tip leans on leaf cards.
    #[test]
    fn coarse_conifer_crowns_hold_together() {
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        let p = &lib.plants[lib.plant("sitka_spruce").unwrap()];
        for vsize in [0.5f32, 1.0] {
            let (mut sv, mut sc, mut cv, mut cc) = (0, 0, 0, 0);
            for s in 0..80u32 {
                let h = hash32(s ^ 0x5151);
                let mut t = crate::world::flora::grow(&lib, p, Vec3::new(37.0 * unit(h), 40.0 + 9.0 * unit(h >> 8), 23.0 * unit(h >> 16)), hash32(s.wrapping_mul(2654435761)), crate::library::Worn::default());
                t.leaf = LEAVES;
                t.bark = BARK;
                let (g, n, ny, _) = press_whole(&t, vsize);
                let (v, c) = strays(&g, n, ny, LEAVES);
                sv += v;
                sc += c;
                // The leaf cells the mesher draws at this level (mesh.rs
                // leaf_blocks: five of a 2x2x2 cell's voxels).
                let (cn, cy) = (n / 2, ny / 2);
                let mut cells = vec![AIR; cn * cn * cy];
                for y in 0..cy {
                    for z in 0..cn {
                        for x in 0..cn {
                            let k = (0..8).filter(|i| g[((2 * y + (i >> 1 & 1)) * n + 2 * z + (i >> 2 & 1)) * n + 2 * x + (i & 1)] == LEAVES).count();
                            if k >= 5 {
                                cells[(y * cn + z) * cn + x] = LEAVES;
                            }
                        }
                    }
                }
                let (v, c) = strays(&cells, cn, cy, LEAVES);
                cv += v;
                cc += c;
            }
            println!("vsize {vsize}: {sv} stray leaf voxels in {sc} pieces; {cv} stray leaf cells in {cc} pieces, over 80 trees");
            let drawn = if vsize < 0.75 { cc } else { sc };
            assert_eq!(drawn, 0, "at {vsize} m voxels, {drawn} pieces of spruce crown float clear of the tree");
        }
    }
}
