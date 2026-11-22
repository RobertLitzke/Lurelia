//! Plant growth forms and placement rules: the verbs that library plants and
//! biome entries choose from. A growth form turns a plant's parameters into a
//! tree the generator can draw; a placement rule decides where in its biome a
//! plant or a natural feature (a boulder, a fallen log) appears.

use super::canopy::Blob;
use super::fields::{World, CELL};
use super::gen::{Tree, TreeKind};
use super::noise::{fbm2, hash2, hash32, unit};
use super::season::{Leaf, Turn};
use crate::library::{Library, MorphPick, Op, Placement, PlantDef, Worn};
use glam::{Vec2, Vec3};
use std::f32::consts::TAU;

/// A "branching" tree's dome when its library entry gives none: most of the
/// way to one full crown over the limbs (2026-11-08; 0 was separate clumps,
/// which read as too fragmented).
pub const DOME: f32 = 0.8;
/// The radius the trunk has left where the first limbs leave it, as a share of
/// its radius at the ground (`limb` overrides it). Below that the limbs are
/// sized by area conservation (`share_r`), not by a fixed multiplier, so a
/// branching tree no longer pinches at its lowest fork.

/// Trunks of 0.25 m radius and up drawn a quarter thicker, but none past
/// 0.9 m unless the library already asked for more (the great oaks).
fn thicker(r: f32) -> f32 {
    if r >= 0.25 {
        (r * 1.25).min(r.max(0.9))
    } else {
        r
    }
}

crate::util::named! {
    /// The growth form a library plant names (`form`): the verb that draws it.
    pub enum Form {
        Broad = "broad",
        Birch = "birch",
        Conifer = "conifer",
        Columnar = "columnar",
        Spiral = "spiral",
        Shrub = "shrub",
        Snag = "snag",
        Umbrella = "umbrella",
        Weeping = "weeping",
        Sapling = "sapling",
        Branching = "branching",
        Herb = "herb",
        Vine = "vine",
        Crust = "crust",
        Canes = "canes",
        Cactus = "cactus",
        Kelp = "kelp",
        Pads = "pads",
        Bamboo = "bamboo",
        Gnarled = "gnarled",
        Mangrove = "mangrove",
        // Coral and sponges (`coral`, docs/UNDERWATER.md): water plants like
        // kelp, but their height is their own, not a share of the water.
        CoralBranch = "coral_branch",
        CoralAntler = "coral_antler",
        CoralTable = "coral_table",
        CoralMound = "coral_mound",
        CoralPlate = "coral_plate",
        CoralFinger = "coral_finger",
        SeaFan = "sea_fan",
        Sponge = "sponge",
    }
}

impl Form {
    /// The coral forms: grown on the bed under water (`water_depth`) at their
    /// own size, unlike kelp, which reaches up a share of the water over it.
    pub fn is_coral(self) -> bool {
        matches!(
            self,
            Form::CoralBranch | Form::CoralAntler | Form::CoralTable | Form::CoralMound | Form::CoralPlate | Form::CoralFinger | Form::SeaFan | Form::Sponge
        )
    }
}

crate::util::named! {
    /// How a biome places a plant or a feature (`placement.verb`).
    pub enum PlaceVerb {
        Scatter = "scatter",
        Clustered = "clustered",
        Edge = "edge",
        Riparian = "riparian",
        Gradient = "gradient",
        Ring = "ring",
        Lone = "lone",
    }
}

fn lerp(r: [f32; 2], t: f32) -> f32 {
    r[0] + (r[1] - r[0]) * t
}

/// A seed key from a name (FNV-1a), not from a place in a list: a new
/// biome or plant sorts in among the old ones, and placement seeded by index
/// would move everything after it in every world already made.
fn name_key(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// Which of a biome's plants, features or collectibles a seed is for. `mul`
/// keeps the uses of this apart from each other. A world on the first seed
/// scheme (`v1`, WorldDef::seeds_v1) keys by the index, as it was laid out
/// with; every other world keys by the name, so adding a plant to a biome
/// leaves the rest of them where they stood.
fn use_key(v1: bool, ui: usize, name: &str, mul: u32) -> u32 {
    let k = if v1 { ui as u32 } else { name_key(name) };
    k.wrapping_mul(mul)
}

/// Which patch of ground (x, z) falls in, for patches about `blob_m` across:
/// the nearest of one jittered point per cell, so the edges are irregular
/// rather than a grid. The hash of the patch, never 0.
///
/// A plant's coat is drawn per patch, not per plant: a cluster of wisteria is
/// one colour and the next along is another, and a meadow's grasses lie in
/// blobs instead of speckling three greens over each other.
pub fn blob(x: f32, z: f32, blob_m: f32, seed: u32) -> u32 {
    let m = blob_m.max(1.0);
    let (cx, cz) = ((x / m).floor() as i32, (z / m).floor() as i32);
    let mut best = (f32::INFINITY, 1u32);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (gx, gz) = (cx + dx, cz + dz);
            let h = hash2(gx, gz, seed);
            let (px, pz) = ((gx as f32 + unit(h)) * m, (gz as f32 + unit(hash32(h ^ 0x5bd1))) * m);
            let d = (px - x) * (px - x) + (pz - z) * (pz - z);
            if d < best.0 {
                best = (d, h | 1);
            }
        }
    }
    best.1
}

/// The coat plant `pi` wears at (x, z), and the materials that go with it.
/// `pick` is the biome's `morph`, which stands in for the plant's own
/// weights where it has one.
pub fn coat_at(lib: &Library, pi: usize, pick: Option<&MorphPick>, x: f32, z: f32, seed: u32) -> Worn {
    let coats = lib.plant_coats(pi);
    if coats.is_empty() {
        return Worn::default();
    }
    let shares: Vec<f32> = match pick {
        None => coats.iter().map(|c| c.share).collect(),
        Some(MorphPick::One(n)) => coats.iter().map(|c| if &c.name == n { 1.0 } else { 0.0 }).collect(),
        Some(MorphPick::Weights(w)) => coats.iter().map(|c| w.get(&c.name).copied().unwrap_or(0.0).max(0.0)).collect(),
    };
    let h = blob(x, z, lib.plants[pi].morph_blob_m, seed ^ (pi as u32).wrapping_mul(0x9e37_79b9));
    // Both draws come from the patch, so its shade is as steady as its colour.
    let Some((which, at)) = crate::variety::pick_coat(&shares, unit(h), unit(hash32(h ^ 0x5d1e)), |i| coats[i].shades) else {
        return Worn::default();
    };
    let shades = coats[which].shades.max(1);
    let k = if shades <= 1 { 0 } else { (at * (shades - 1) as f32).round() as u32 };
    lib.worn(pi, which, k)
}

/// Grow one plant at `base` (ground point) from its library entry.
///
/// A plant is always grown in full summer leaf and in flower, whatever the
/// year is doing: the season is put on afterwards, by `dress`, because the
/// world turns while the plants stay where they were grown.
pub fn grow(lib: &Library, p: &PlantDef, base: Vec3, seed: u32, worn: Worn) -> Tree {
    let mut rng = seed;
    let mut rnd = || {
        rng = hash32(rng.wrapping_add(0x6d2b_79f5));
        unit(rng)
    };
    // Each colour slot in the coat this one is wearing, or the plant's own.
    let slot = |name: &str, own: Option<&String>| worn.get(name).or_else(|| own.and_then(|m| lib.material(m)));
    let bark = slot("bark", Some(&p.bark)).unwrap_or(super::material::BARK);
    let mut leaf = slot("leaf", p.leaf.as_ref()).unwrap_or(0);
    // One draw either way, so the season or a second colour never changes
    // the shape of the tree.
    let draw = if p.leaf_alt.is_some() || p.autumn.is_some() { rnd() } else { 1.0 };
    if let Some((alt, chance)) = &p.leaf_alt {
        if draw < *chance {
            leaf = lib.material(alt).unwrap_or(leaf);
        }
    }
    // The colour it will wear in autumn, kept rather than used: `dress` puts
    // it on when the year reaches this tree.
    let autumn = p.autumn.as_ref().and_then(|_| slot("autumn", p.autumn.as_ref())).unwrap_or(0);
    let height = lerp(p.height, rnd());
    // Herbs (ferns, fireweed) are drawn as sprites from one marker voxel of the
    // leaf material (gen.rs draw_tree, mesh/sprites.rs Rule::Herb): no trunk.
    let trunk_r = if matches!(p.form, Form::Herb | Form::Kelp) { 0.0 } else { thicker(lerp(p.trunk_radius, rnd())) };
    let crown = lerp(p.crown_radius, rnd());
    let mut blobs: Vec<Blob> = Vec::new();
    let mut branches: Vec<(Vec3, Vec3, f32)> = Vec::new();
    // Grown in flower whatever the season; `dress` takes the blossom off
    // again outside the plant's own flowering season (`flower_seasons`).
    let flower = slot("flower", p.flower.as_ref());
    let stems = p.stems.unwrap_or([10.0, 16.0]);
    let (kind, trunk_h, canopy_lo) = match p.form {
        Form::Canes => {
            // Ocotillo: long whip-like canes fanning up and out from one root
            // crown, curving outward toward their tips, each tipped with a
            // narrow flame of flowers (`flower`).
            //
            // With `canopy: "tufts"` it is a thorny shrub instead (sea
            // buckthorn): stiff canes rising nearly upright and arching out
            // near the top, set with short spurs, small tufts of leaf on the
            // upper half and berries (`flower`) crowded along the stems.
            let n = lerp(stems, rnd()).round().max(3.0) as usize;
            // A shrub's canes may be thinner than a voxel: they are drawn as
            // tubes along their own lines (mesh/lean.rs).
            let thin = p.canopy == Some(Canopy::Tufts);
            let r = trunk_r.max(if thin { 0.05 } else { 0.1 });
            let a0 = rnd() * TAU;
            if thin {
                let lean = p.cane_lean.unwrap_or(0.3);
                // Leaf tufts, spurs and berries are a buckthorn's; a shrub
                // lower than one shrinks them with it.
                let sz = (height / 2.2).clamp(0.45, 1.0);
                let spurs = p.cane_spurs.unwrap_or(0.4);
                // A tuft smaller than this meshes to nothing.
                const LEAF_MIN: f32 = 0.22;
                let leafy = p.cane_leaves.unwrap_or(1.0);
                for k in 0..n {
                    let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.9;
                    let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                    let side = Vec3::new(-horiz.z, 0.0, horiz.x);
                    let tilt0 = 0.1 + rnd() * lean;
                    // Arching: the bend grows toward the tip.
                    let bend = match p.cane_bend {
                        Some(b) => b * (0.5 + rnd()),
                        None => 0.04 + rnd() * 0.07,
                    };
                    let len = height * (0.65 + 0.35 * rnd());
                    let segs = 7;
                    // Feet spread through the clump, so the canes stand apart.
                    let mut at = base + horiz * crown * (0.15 + 0.45 * rnd()) + Vec3::Y * 0.1;
                    for s in 0..segs {
                        let tilt = tilt0 + bend * (s * s) as f32 * 0.25;
                        let d = (Vec3::Y * tilt.cos() + horiz * tilt.sin()) * (len / segs as f32);
                        let end = at + d;
                        let rs = r * (1.0 - 0.45 * s as f32 / segs as f32);
                        branches.push((at, end, rs));
                        let up = s as f32 / segs as f32;
                        if s >= 2 {
                            // A spur off to one side, angled up.
                            if rnd() < spurs {
                                let sg = if rnd() < 0.5 { -1.0 } else { 1.0 };
                                let from = at.lerp(end, rnd());
                                let tip = from + (side * sg * 0.8 + horiz * 0.4 + Vec3::Y * 0.9).normalize() * (0.35 + 0.45 * rnd()) * sz;
                                branches.push((from, tip, rs * 0.7));
                                if up > 0.4 && rnd() < leafy {
                                    blobs.push(Blob::ball(tip, ((0.2 + 0.1 * rnd()) * sz).max(LEAF_MIN)));
                                }
                            }
                            // Berries packed round the stem.
                            if flower.is_some() && !p.flower_tips {
                                let berries = 2 + (rnd() * 3.0) as usize;
                                for _ in 0..berries {
                                    let a = rnd() * TAU;
                                    let c = at.lerp(end, rnd()) + (side * a.cos() + horiz * a.sin()) * (rs + 0.14 * sz);
                                    blobs.push(Blob::ball(c, (0.14 + 0.04 * rnd()) * sz.max(0.8)).of(flower.unwrap()));
                                }
                            }
                        }
                        if up > 0.45 && rnd() < leafy {
                            blobs.push(Blob::ball(end, ((0.22 + 0.12 * rnd()) * sz).max(LEAF_MIN)));
                        }
                        at = end;
                    }
                    // An umbel (papyrus): a mop of thin rays out of the tip,
                    // arching out and over, drawn as sticks.
                    if let Some(len) = p.cane_umbel {
                        let rays = 9;
                        let a1 = rnd() * TAU;
                        for j in 0..rays {
                            let az = a1 + j as f32 * TAU / rays as f32;
                            let o = Vec3::new(az.cos(), 0.0, az.sin());
                            let up = 0.35 + 0.5 * rnd();
                            let mid = at + (o * 0.7 + Vec3::Y * up).normalize() * len * 0.55;
                            let tip = mid + (o - Vec3::Y * 0.35).normalize() * len * 0.5;
                            branches.push((at, mid, 0.03));
                            branches.push((mid, tip, 0.03));
                        }
                    }
                    // Flowers in a cluster at the tip.
                    if let (Some(fl), true) = (flower, p.flower_tips) {
                        let r = (0.24 + 0.1 * rnd()) * sz.max(0.7);
                        // Flowers drawn as sprites (rabbitbrush) grow from the
                        // marker voxels touching the cane, and a ball under a
                        // voxel across often rasterizes to none at all.
                        let r = if crate::mesh::sprites::marker(fl) { r.max(0.34) } else { r };
                        blobs.push(Blob::ball(at, r).of(fl));
                    }
                }
                (TreeKind::Broad, 0.3, 0.0)
            } else {
                for k in 0..n {
                    let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.9;
                    // Inner canes nearly upright, outer ones leaning well out;
                    // `spread` lays them over (an agave's rosette at 1).
                    let tilt0 = match p.cane_lean {
                        Some(l) => rnd() * l + p.spread.clamp(0.0, 1.0) * 0.75,
                        None => 0.12 + rnd() * 0.42 + p.spread.clamp(0.0, 1.0) * 0.75,
                    };
                    let bend = if p.cane_lean.is_some() { 0.005 + rnd() * 0.02 } else { 0.02 + rnd() * 0.06 };
                    let len = height * (0.7 + 0.3 * rnd());
                    let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                    let segs = 6;
                    let mut at = base + horiz * 0.12 + Vec3::Y * 0.15;
                    for s in 0..segs {
                        let tilt = tilt0 + bend * s as f32;
                        let d = (Vec3::Y * tilt.cos() + horiz * tilt.sin()) * (len / segs as f32);
                        let end = at + d;
                        // Thinner toward the tip.
                        branches.push((at, end, r * (1.0 - 0.25 * s as f32 / segs as f32)));
                        at = end;
                    }
                    if let Some(fl) = flower {
                        let up = Vec3::Y * 0.9 + horiz * 0.1;
                        if crate::mesh::sprites::marker(fl) {
                            // Flowers drawn as sprites (ocotillo) grow from
                            // the marker voxels touching the cane tip: one
                            // ball round it, big enough to rasterize.
                            blobs.push(Blob::ball(at + up * 0.05, 0.34));
                        } else {
                            blobs.push(Blob::ball(at + up * 0.12, 0.17));
                            blobs.push(Blob::ball(at + up * 0.4, 0.13));
                        }
                    }
                }
                if let Some(fl) = flower {
                    leaf = fl;
                }
                (TreeKind::Broad, 0.3, 0.0)
            }
        }
        Form::Bamboo => {
            // A bamboo clump: `stems` culms from a patch `crown_radius` m
            // across. Each culm is one dead straight vertical stalk of even
            // thickness from the ground to near its top, then a short nod;
            // sprays of leaves stand out from it on the upper part.
            // `trunk_radius` is a culm's radius (keep it 0.12 or more so a
            // culm is at least a voxel wide), `crown_base` where the leaves
            // start (share of the height, default 0.55).
            let n = lerp(stems, rnd()).round().max(1.0) as usize;
            let r = trunk_r.max(0.12);
            let leaf_from = p.crown_base.unwrap_or(0.55);
            let lean_az = rnd() * TAU;
            let mut feet: Vec<Vec3> = Vec::new();
            for _ in 0..n {
                // Feet at least two culms apart, so stalks never merge.
                let mut foot = base;
                for _ in 0..6 {
                    let ra = rnd() * TAU;
                    let rr = crown * 0.5 * rnd().sqrt();
                    foot = base + Vec3::new(ra.cos() * rr, 0.0, ra.sin() * rr);
                    if feet.iter().all(|f| f.distance(foot) > r * 4.0) {
                        break;
                    }
                }
                feet.push(foot);
                let h = height * (0.65 + 0.35 * rnd());
                let stalk = h * 0.88;
                let top = foot + Vec3::Y * stalk;
                branches.push((foot, top, r));
                // The nod: the last stretch leans over a little, thinner.
                let az = lean_az + (rnd() - 0.5) * 1.2;
                let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                let tip = top + (Vec3::Y * 0.9 + horiz * 0.45).normalize() * (h - stalk);
                branches.push((top, tip, r * 0.8));
                if leaf == 0 {
                    continue;
                }
                // Sprays every metre and a half up the leafy part, turning
                // round the stalk, held clear of it.
                let mut y = h * leaf_from;
                let mut a = rnd() * TAU;
                while y < stalk {
                    a += 2.2 + 0.6 * rnd();
                    let side = Vec3::new(a.cos(), 0.0, a.sin());
                    let lr = 0.3 + 0.2 * rnd();
                    blobs.push(Blob::ball(foot + Vec3::Y * y + side * (lr + r + 0.3), lr));
                    y += 1.3 + 0.5 * rnd();
                }
                blobs.push(Blob::ball(tip + Vec3::Y * 0.1, 0.35 + 0.15 * rnd()));
            }
            (TreeKind::Broad, 0.2, height * leaf_from)
        }
        Form::Pads => {
            // Prickly pear: a low sprawl of flat jointed pads, each growing
            // from the rim of the one below at a new angle, fruit or
            // flowers (`flower`) on the top pads' rims.
            let n = lerp(stems, rnd()).round().max(1.0) as usize;
            let r = trunk_r.max(0.13);
            let pad = (height / 3.0).clamp(0.25, 0.5);
            for _ in 0..n {
                let mut az = rnd() * TAU;
                let mut at = base + Vec3::new(az.cos(), 0.0, az.sin()) * (rnd() * crown * 0.6) + Vec3::Y * 0.1;
                let joints = 2 + (rnd() * 2.5) as usize;
                for j in 0..joints {
                    az += (rnd() - 0.5) * 1.6;
                    let tilt = 0.3 + rnd() * 0.6;
                    let d = (Vec3::Y * tilt.cos() + Vec3::new(az.cos(), 0.0, az.sin()) * tilt.sin()) * pad;
                    let end = at + d;
                    // A pad is wide one way and thin the other: two capsules
                    // side by side across it.
                    let side = Vec3::new(-az.sin(), 0.0, az.cos()) * r * 0.6;
                    branches.push((at + side, end + side, r));
                    branches.push((at - side, end - side, r));
                    let bloom = rnd() < 0.7;
                    if j + 1 == joints && flower.is_some() && bloom {
                        blobs.push(Blob::ball(end + Vec3::Y * 0.12, 0.13));
                    }
                    at = end;
                }
            }
            if let Some(fl) = flower {
                leaf = fl;
            } else {
                leaf = 0;
            }
            (TreeKind::Broad, 0.2, 0.0)
        }
        Form::Cactus => {
            // Saguaro: a thick column with a rounded top and a few arms that
            // leave it level, then turn up. Young ones have no arms.
            let r = trunk_r;
            let top = base + Vec3::Y * (height - r * 0.6);
            branches.push((base + Vec3::Y * (height * 0.55), top, r * 0.78));
            // Arms come with age (height): none under 5 m, then one to five.
            let n = if height < 5.0 { 0 } else { (1 + (rnd() * (height - 4.0) / 1.5) as usize).min(5) };
            let a0 = rnd() * TAU;
            for k in 0..n {
                let az = a0 + k as f32 * TAU / n.max(1) as f32 + (rnd() - 0.5) * 0.8;
                let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                let y0 = height * (0.32 + 0.28 * rnd());
                let ar = r * (0.62 + 0.12 * rnd());
                let start = base + Vec3::Y * y0;
                let elbow = start + horiz * (r + 0.35 + 0.4 * rnd()) + Vec3::Y * 0.25;
                let up = (height - y0) * (0.35 + 0.45 * rnd()) + 0.8;
                let tip = elbow + Vec3::Y * up + horiz * 0.1;
                branches.push((start, elbow, ar));
                branches.push((elbow, tip, ar));
                if flower.is_some() {
                    blobs.push(Blob::ball(tip + Vec3::Y * ar, 0.28));
                }
            }
            if let Some(fl) = flower {
                blobs.push(Blob::ball(top + Vec3::Y * r * 0.7, 0.32));
                leaf = fl;
            } else {
                leaf = 0;
            }
            (TreeKind::Broad, height * 0.9, height)
        }
        f if f.is_coral() => coral(f, &mut rnd, base, height, trunk_r, crown, p.stems, flower, &mut blobs, &mut branches),
        Form::Kelp => {
            // Kelp is loose in the water and rides the tide, so it is not
            // voxels: one marker voxel on the bed (the float, or the blade
            // for kelp with none: matlib `Kelp`), which the sprite pass
            // draws as stipes reaching up to the surface and streaming
            // along it (mesh/sprites.rs `Herb::Kelp`, foliage.wgsl). Divers
            // and big swimmers pass through it.
            // Its height is still the stipe's reach (the water over it, set at
            // placement), so the catalogue and the gazetteer can find it.
            leaf = flower.unwrap_or(leaf);
            (TreeKind::Broad, height, 0.0)
        }
        Form::Gnarled => {
            // A desert juniper: a short trunk leaning off true, splitting low
            // (0.4 to 1.2 m) into two to four limbs. Each limb is a chain of
            // kinked segments that wanders out and up, and ends in a flattened
            // clump of foliage; one limb is often bare, long dead. `lean` is the
            // trunk's largest lean (radians), `spread` how far the limbs reach.
            let lean_az = rnd() * TAU;
            let lean_dir = Vec3::new(lean_az.cos(), 0.0, lean_az.sin());
            let lean = p.lean * (0.4 + 0.6 * rnd());
            let split = 0.4 + 0.8 * rnd();
            let trunk_top = base + Vec3::Y * (split + 0.1) + lean_dir * lean.tan() * split;
            let r0 = trunk_r.max(0.14);
            branches.push((base, trunk_top, r0));
            let n = 2 + (rnd() * 2.99) as usize;
            let reach = 1.0 + p.spread.clamp(0.0, 1.0);
            let a0 = rnd() * TAU;
            let mut dead = rnd() < 0.35;
            for k in 0..n {
                // Limbs fan out round the trunk, favouring the lean.
                let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.9;
                let mut horiz = (Vec3::new(az.cos(), 0.0, az.sin()) + lean_dir * 0.5).normalize();
                let mut tilt: f32 = 0.35 + 0.35 * rnd();
                let segs = 2 + (rnd() < 0.6) as usize;
                let total = (height - split) * (0.75 + 0.3 * rnd());
                let mut at = trunk_top;
                let mut r = r0 * (0.78 - 0.08 * k as f32).max(0.5);
                for s in 0..segs {
                    // Each joint kinks: the limb turns sideways and changes
                    // how steeply it climbs, so it never runs straight.
                    let twist = (rnd() - 0.5) * 1.3;
                    horiz = Vec3::new(horiz.x * twist.cos() - horiz.z * twist.sin(), 0.0, horiz.x * twist.sin() + horiz.z * twist.cos());
                    tilt = (tilt + (rnd() - 0.45) * 0.5).clamp(0.2, 0.95);
                    let len = total / segs as f32 * (0.8 + 0.4 * rnd());
                    let d = Vec3::Y * tilt.cos() * 0.9 + horiz * tilt.sin() * reach * 0.55;
                    let end = at + d.normalize() * len;
                    branches.push((at, end, r.max(0.1)));
                    // A short side twig off the middle joint.
                    if s == 0 && segs > 2 && rnd() < 0.6 {
                        let side = Vec3::new(-horiz.z, 0.0, horiz.x) * if rnd() < 0.5 { 1.0 } else { -1.0 };
                        let tip = end + (side * 0.8 + Vec3::Y * 0.6) * (0.5 + 0.4 * rnd());
                        branches.push((end, tip, (r * 0.6).max(0.08)));
                        if leaf != 0 {
                            blobs.push(Blob::ball(tip + Vec3::Y * 0.15, crown * 0.45 * (0.8 + 0.3 * rnd())));
                        }
                    }
                    at = end;
                    r *= 0.72;
                }
                if dead {
                    // One bare silver limb, no foliage.
                    dead = false;
                    continue;
                }
                // The clump: a broad, flat pad of foliage on the limb's end,
                // with a smaller one beside it.
                blobs.push(Blob::ball(at + Vec3::Y * 0.2, crown * (0.6 + 0.3 * rnd())));
                let off = Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * crown * 0.9;
                blobs.push(Blob::ball(at + off - Vec3::Y * 0.15, crown * (0.4 + 0.2 * rnd())));
            }
            (TreeKind::Broad, split + 0.1, split)
        }
        Form::Mangrove => {
            // Red mangrove: the trunk stands on a ring of arching prop roots
            // a metre or two up, so the water runs under the tree; each root
            // bows out and down into the mud. A low, wide crown of short
            // limbs, and a few drop roots hanging straight from it. `stems`
            // is the number of prop roots.
            let n = lerp(p.stems.unwrap_or([5.0, 8.0]), rnd()).round().max(3.0) as usize;
            let root_h = 1.3 + 0.2 * height;
            let reach = crown * 0.95;
            let rr = (trunk_r * 0.45).max(0.13);
            let s = base + Vec3::Y * root_h;
            let a0 = rnd() * TAU;
            for k in 0..n {
                let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.4;
                let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                let out = reach * (0.7 + 0.3 * rnd());
                let a = s + horiz * out * 0.3 + Vec3::Y * 0.25;
                let b = s + horiz * out * 0.65 + Vec3::Y * 0.05;
                let c = base + horiz * out * 0.9 + Vec3::Y * root_h * 0.35;
                let d = base + horiz * out - Vec3::Y * 0.4;
                branches.push((s, a, rr));
                branches.push((a, b, rr));
                branches.push((b, c, rr * 0.95));
                branches.push((c, d, rr * 0.9));
            }
            // The trunk still runs down to the mud (a tree's culling box
            // starts at its foot), thin under the roots.
            let trunk_h = height * 0.55 + root_h * 0.5;
            let top = base + Vec3::Y * trunk_h;
            let limbs = 4 + (rnd() * 2.0) as usize;
            let b0 = rnd() * TAU;
            for k in 0..limbs {
                let az = b0 + k as f32 * TAU / limbs as f32 + (rnd() - 0.5) * 0.6;
                let horiz = Vec3::new(az.cos(), 0.0, az.sin());
                let end = top + horiz * crown * (0.55 + 0.25 * rnd()) + Vec3::Y * height * (0.12 + 0.12 * rnd());
                branches.push((top, end, (trunk_r * 0.5).max(0.13)));
                blobs.push(Blob::ball(end + Vec3::Y * 0.3, crown * (0.42 + 0.12 * rnd())));
                if rnd() < 0.4 {
                    // A drop root from the limb straight down to the mud.
                    let foot = Vec3::new(end.x, base.y - 0.4, end.z);
                    branches.push((end - Vec3::Y * 0.3, foot, 0.13));
                }
            }
            blobs.push(Blob::ball(top + Vec3::Y * height * 0.25, crown * 0.55));
            (TreeKind::Broad, trunk_h, trunk_h)
        }
        Form::Branching => {
            // A trunk shedding limbs that fork again, each clothed in leaves,
            // under a crown of lobes. `curve` and `canopy` (canopy.rs) decide
            // the shape of a limb and how the foliage over it is massed, so
            // species differ by more than their numbers.
            branching(p, &mut rnd, base, height, trunk_r, crown, flower, &mut blobs, &mut branches)
        }
        Form::Conifer | Form::Columnar | Form::Sapling => {
            // Whorls of branches up a leader, each clothed in needles, over a
            // core of needles round the trunk itself (canopy.rs shapes, the
            // same ones the broadleaves use).
            conifer(p, &mut rnd, base, height, trunk_r, crown, &mut blobs, &mut branches)
        }
        Form::Birch => {
            let trunk_h = height * 0.7;
            for i in 0..3 {
                let y = trunk_h - crown + i as f32 * crown * 0.7;
                let off = Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * 0.8;
                blobs.push(Blob::ball(base + Vec3::new(0.0, y, 0.0) + off, crown * (1.0 - i as f32 * 0.17) + rnd() * 0.3));
            }
            (TreeKind::Birch, trunk_h, trunk_h - crown * 2.0)
        }
        Form::Spiral => {
            // Leaf blobs wound up the trunk in a helix, tighter toward the top.
            let trunk_h = height * 0.92;
            let turns = 1.6 + rnd() * 0.8;
            let n = 7;
            let a0 = rnd() * TAU;
            for k in 0..n {
                let t = (k as f32 + 0.5) / n as f32;
                let a = a0 + t * turns * TAU;
                let r = crown * (1.0 - t * 0.7);
                let c = base + Vec3::new(a.cos() * r, height * (0.3 + 0.62 * t), a.sin() * r);
                blobs.push(Blob::ball(c, 0.7 + crown * 0.35 * (1.0 - t * 0.5)));
            }
            (TreeKind::Broad, trunk_h, height * 0.22)
        }
        Form::Shrub => {
            let n = 2 + (rnd() * 3.0) as usize;
            for _ in 0..n {
                let a = rnd() * TAU;
                let d = rnd() * crown * 0.6;
                blobs.push(Blob::ball(
                    base + Vec3::new(a.cos() * d, crown * 0.55 + rnd() * height * 0.3, a.sin() * d),
                    crown * (0.55 + rnd() * 0.35),
                ));
            }
            (TreeKind::Broad, 0.3, 0.0)
        }
        Form::Herb => (TreeKind::Broad, 0.0, 0.0),
        Form::Snag => {
            // Dead: bare branch stubs, no leaves; none on a stump (crown 0).
            let n = if crown > 0.0 { 2 + (rnd() * 3.0) as usize } else { 0 };
            for _ in 0..n {
                let a = rnd() * TAU;
                let y = height * (0.45 + rnd() * 0.45);
                let tip = base + Vec3::new(a.cos() * crown * 0.6, y + 0.6, a.sin() * crown * 0.6);
                blobs.push(Blob::ball(tip, 0.0));
                // A broken limb out of the trunk to it, rising, and a short
                // dead twig off its upper side.
                let from = base + Vec3::Y * (y - 0.3);
                let rs = (trunk_r * 0.35).max(0.06);
                branches.push((from, tip, rs));
                let mid = from.lerp(tip, 0.6);
                branches.push((mid, mid + (tip - from).normalize_or_zero() * 0.3 + Vec3::Y * (0.4 + 0.3 * crown), rs * 0.6));
            }
            leaf = 0;
            (TreeKind::Broad, height, height)
        }
        Form::Umbrella => {
            let trunk_h = height - crown * 0.4;
            // The cap sits down on the trunk top rather than over it: a
            // squashed ball reaches only `crown * squash` below its middle,
            // and a flat crown centred above the trunk leaves the leaves
            // hanging clear of the wood.
            blobs.push(Blob::ball(base + Vec3::new(0.0, trunk_h - crown * 0.2, 0.0), crown));
            for k in 0..4 {
                let a = rnd() * TAU + k as f32;
                blobs.push(Blob::ball(base + Vec3::new(a.cos() * crown * 0.7, trunk_h - crown * 0.3, a.sin() * crown * 0.7), crown * 0.6));
            }
            // With `stems`, the trunk splits into that many limbs rising out
            // under the flat crown, each forking once (an acacia); without,
            // it runs straight up into the cap (a palm, a stone pine).
            match p.stems {
                Some(st) => {
                    let n = lerp(st, rnd()).round().max(2.0) as usize;
                    let fork = trunk_h * 0.45;
                    let at = base + Vec3::Y * fork;
                    let a0 = rnd() * TAU;
                    for k in 0..n {
                        let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.8;
                        let out = Vec3::new(az.cos(), 0.0, az.sin());
                        let elbow = base + out * crown * (0.3 + 0.1 * rnd()) + Vec3::Y * (fork + (trunk_h - fork) * 0.6);
                        let end = base + out * crown * (0.65 + 0.2 * rnd()) + Vec3::Y * (trunk_h - crown * 0.15);
                        branches.push((at, elbow, trunk_r * 0.6));
                        branches.push((elbow, end, trunk_r * 0.4));
                        for sg in [-1.0f32, 1.0] {
                            let side = (out + Vec3::new(-out.z, 0.0, out.x) * sg * 0.7).normalize();
                            let tip = base + side * crown * (0.55 + 0.25 * rnd()) + Vec3::Y * (trunk_h - crown * 0.1);
                            branches.push((elbow, tip, (trunk_r * 0.22).max(0.06)));
                        }
                    }
                    (TreeKind::Broad, fork + 0.4, trunk_h - crown * 0.5)
                }
                None => (TreeKind::Broad, trunk_h, trunk_h - crown * 0.5),
            }
        }
        Form::Weeping => {
            let trunk_h = height * 0.6;
            let top = base + Vec3::new(0.0, trunk_h + 0.6, 0.0);
            blobs.push(Blob::ball(top, crown * 0.8));
            let n = 6;
            let a0 = rnd() * TAU;
            let trunk_top = base + Vec3::Y * trunk_h;
            for k in 0..n {
                let a = a0 + k as f32 * TAU / n as f32;
                let c = top + Vec3::new(a.cos() * crown * 0.85, -crown * (0.6 + rnd() * 0.5), a.sin() * crown * 0.85);
                blobs.push(Blob::ball(c, crown * 0.5));
                // A limb arching up and out from the trunk top, then whips
                // hanging from its end down through the curtain of leaves:
                // what a willow is in winter.
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                let side = Vec3::new(-out.z, 0.0, out.x);
                let rise = trunk_top + out * crown * 0.3 + Vec3::Y * crown * 0.35;
                let arch = trunk_top + out * crown * 0.55 + Vec3::Y * crown * 0.25;
                branches.push((trunk_top, rise, trunk_r * 0.45));
                branches.push((rise, arch, trunk_r * 0.3));
                // The whips lean out as they fall, through the curtain and
                // ending inside it: hung plumb they would be voxel columns,
                // not tubes, and read as planks.
                for o in [-1.0f32, 0.0, 1.0] {
                    let from = arch + side * o * crown * 0.15;
                    let to = Vec3::new(from.x, c.y - crown * 0.15, from.z) + out * crown * 0.2 + side * o * crown * 0.06;
                    if o != 0.0 {
                        branches.push((arch, from, 0.06));
                    }
                    branches.push((from, to, 0.05));
                }
            }
            (TreeKind::Broad, trunk_h, 1.0)
        }
        _ => {
            // broad
            let trunk_h = (height - crown * 1.6).max(1.5);
            let top = base + Vec3::new(0.0, trunk_h + crown * 0.5, 0.0);
            blobs.push(Blob::ball(top, crown));
            let n = 3 + (rnd() * 2.0) as usize;
            let a0 = rnd() * TAU;
            for i in 0..n {
                let a = a0 + i as f32 * TAU / n as f32 + rnd() * 0.6;
                let d = crown * (0.6 + 0.4 * rnd());
                let c = top + Vec3::new(a.cos() * d, -crown * 0.3 + crown * 0.6 * rnd(), a.sin() * d);
                blobs.push(Blob::ball(c, crown * (0.6 + 0.35 * rnd())));
            }
            (TreeKind::Broad, trunk_h, trunk_h - 1.2)
        }
    };
    let squash = p.squash;
    // Each shape measures its own reach (canopy.rs), so a sleeve along a
    // limb and a plate on its side are boxed as tightly as a ball. Conifers
    // go through this too since 2026-11-14: their crown is blobs now, not one
    // cone whose bounds had to be guessed from its radius.
    let radius = blobs
        .iter()
        .map(|b| b.reach(base, squash, 1.35).0)
        .chain(branches.iter().map(|(_, e, r)| Vec3::new(e.x - base.x, 0.0, e.z - base.z).length() + r))
        .fold(1.0, f32::max);
    let top = blobs.iter().map(|b| b.reach(base, squash, 1.35).1).fold(trunk_h, f32::max);
    let plant = lib.plant(&p.name).map_or(crate::world::gen::NO_PLANT, |i| i as u16);
    let flare = p.buttress * (0.8 + 0.4 * rnd());
    Tree {
        pos: base,
        kind,
        trunk_h,
        trunk_r,
        blobs,
        canopy_lo: canopy_lo.max(0.0),
        top,
        radius: radius.max(trunk_r + flare * if flare >= super::stamp::PLANK_FLARE { super::stamp::PLANK_REACH } else { 1.0 } + 0.3),
        leaf,
        bark,
        seed,
        squash,
        branches,
        plant,
        flare,
        autumn,
        flower: flower.unwrap_or(0),
        sticks: p.thin_limbs.unwrap_or(0.0),
    }
}

crate::util::named! {
    /// The shape a branching plant's limb takes (`curve`).
    pub enum Curve {
        Straight = "straight",
        /// Bows out and levels off: the default.
        Arch = "arch",
        Sweep = "sweep",
        Droop = "droop",
        Kinked = "kinked",
    }
}

crate::util::named! {
    /// How a branching plant's foliage is massed (`canopy`).
    pub enum Canopy {
        /// One lobe over each big limb: the default.
        Lobes = "lobes",
        Layers = "layers",
        Tufts = "tufts",
        Cloud = "cloud",
        Vase = "vase",
    }
}

crate::util::named! {
    /// How a conifer's whorl branches sit (`sweep`).
    pub enum Sweep {
        /// A fir: the default.
        Level = "level",
        Droop = "droop",
        Up = "up",
        Ragged = "ragged",
    }
}

/// A conifer: a leader with whorls of branches up it, each whorl clothed in
/// needles, over a core of needles round the trunk.
///
/// Until 2026-11-14 this was one cone — a radius function of height, scalloped
/// into tiers and wobbled by noise — which read as a smooth green tent with a
/// fringe of needle cards and nothing in between, and whose thin waists fell
/// under one leaf cell and let the bark show through (see GUIDE "Tree crowns").
/// Built from `canopy.rs` shapes instead, a conifer gets the ragged edge, the
/// broad lumps and the per-level skin damping every other crown has, and its
/// depth comes from its own branches rather than from a wobble.
///
/// Three parts, outermost last so each covers the one inside it:
/// the **core**, sleeves up the leader that clothe the trunk and close the
/// gaps between whorls; a **tier** dome per whorl, wide and shallow; and a
/// **branch** per whorl arm with a sleeve of needles along its outer half.
fn conifer(p: &PlantDef, rnd: &mut dyn FnMut() -> f32, base: Vec3, height: f32, trunk_r: f32, crown: f32, blobs: &mut Vec<Blob>, branches: &mut Vec<(Vec3, Vec3, f32)>) -> (TreeKind, f32, f32) {
    // A columnar species (a fir) is narrow for its height; a conifer proper
    // (a pine) spreads to its full crown radius. The old cone only ever
    // reached this radius at the very foot of the crown, the tier scallop
    // taking a third off everywhere else, so the authored numbers are for a
    // tree narrower than they say; the whorls reach it honestly, and 0.6 left
    // every fir a pencil.
    let rmax = if p.form == Form::Columnar { crown * 0.62 } else { crown };
    let lo = (height * p.crown_base.unwrap_or(0.2)).max(0.4);
    let sweep = p.sweep.unwrap_or(Sweep::Level);
    let whorls = p.whorls.map(|w| lerp(w, rnd()));
    let trunk_h = conifer_crown(rnd, base, height, trunk_r, rmax, lo, p.taper.unwrap_or(CONIFER_TAPER), sweep, whorls, blobs, branches);
    (TreeKind::Pine, trunk_h, lo)
}

/// The conifer geometry itself, without a library entry: the worlds that
/// generate their own trees (`gen.rs`, the noise islands) build the same shape
/// through this. Returns the trunk's clear length.
#[allow(clippy::too_many_arguments)]
pub fn conifer_crown(
    rnd: &mut dyn FnMut() -> f32,
    base: Vec3,
    height: f32,
    trunk_r: f32,
    rmax: f32,
    lo: f32,
    taper: f32,
    sweep: Sweep,
    whorls: Option<f32>,
    blobs: &mut Vec<Blob>,
    branches: &mut Vec<(Vec3, Vec3, f32)>,
) -> f32 {
    let span = (height - lo).max(0.5);
    // How fast the cone narrows: under 1 swells out low and tapers late (a
    // spruce), over 1 is a straight-sided spire.
    let taper = taper.clamp(0.35, 2.0);
    // The cone's profile: how far a whorl at crown fraction `f` reaches.
    let cone = |f: f32| rmax * (1.0 - f).clamp(0.0, 1.0).powf(taper);
    // Needles wanted round the trunk: enough that a leaf cell of them stands
    // outside the wood at full detail, or the mesher finds no leaf surface
    // there and the bark shows through (gen.rs draws the guarantee; this is
    // the same number, so the shape agrees with it).
    // The floors below are absolute sizes in metres, set by what the leaf
    // cells can mesh, and a sapling is smaller than all of them: capped
    // against the crown's own radius so a small tree is a small cone instead
    // of one solid lump of minimums.
    let floor = |m: f32| m.min(rmax * 0.55);
    let clothe = trunk_r + conifer_clothe(trunk_r);
    // So the trunk stops where the cone narrows past what can clothe it, and
    // above that the crown is needles alone and free to taper to a point. A
    // conifer's leader really does run to the tip, but at 25 cm voxels it
    // would be a 19 cm rod needing a 70 cm skirt of needles the whole way,
    // which is what turned the first attempt at this into a cylinder.
    let ft = if rmax > clothe { 1.0 - (clothe / rmax).powf(1.0 / taper) } else { 0.0 };
    let trunk_h = (lo + span * ft).clamp(lo + 0.4, height * 0.95);
    // Whorls roughly every metre and a quarter, which is a few years' growth,
    // within whatever the species asks for.
    let n = (whorls.unwrap_or(span / 1.25).round() as usize).clamp(3, 14);
    // The needle core up the leader: it closes the gaps between whorls and
    // keeps the crown one connected mass. It follows the cone, with a floor of
    // the clothing radius while there is trunk inside it.
    // Enough segments that the core steps down smoothly instead of standing
    // in two or three flat-sided storeys. It runs the whole way to the tip: a
    // sleeve is a capsule, so its top is already a rounded cap and the tree
    // needs no separate apex clump — one that did not quite reach the core
    // below it left a feathery spire floating over a gap.
    let segs = n.clamp(4, 9);
    for s in 0..segs {
        let (t0, t1) = (s as f32 / segs as f32, (s + 1) as f32 / segs as f32);
        let tm = (t0 + t1) * 0.5;
        // Above the trunk the floor draws in toward the tip. Held flat it is
        // the whole of a small tree — a 3 m sapling came out a box with a flat
        // top — and on a big one it is the pipe the tip must not become.
        let r = (cone(tm) * 0.80).max(if tm < ft { clothe } else { floor(TIP_NEEDLE) * (1.0 - tm * 0.55) });
        let a = base + Vec3::Y * (lo + span * t0);
        blobs.push(Blob::sleeve(a, r, Vec3::Y * span * (t1 - t0)));
    }
    const PHI: f32 = 2.399_963;
    let mut az = rnd() * TAU;
    for i in 0..n {
        // Whorls bunched toward the tip, the way a conifer's internodes
        // shorten as it slows: evenly spaced, the top ones stood too far
        // apart for their own width and the crown thinned to a thread.
        let f = 1.0 - (1.0 - i as f32 / n as f32).powf(1.25);
        let rad = cone(f).max(floor(MIN_NEEDLE)) * (0.96 + 0.08 * rnd());
        let y = lo + span * f;
        let hub = base + Vec3::Y * y;
        // The tier: wide and shallow, domed over and flatter underneath, so a
        // whorl reads as a plate of needles rather than a ball.
        blobs.push(Blob::dome(hub - Vec3::Y * rad * 0.06, (rad * 0.92).max(floor(TIP_NEEDLE)), 0.26 + 0.08 * rnd()));
        // Whether this whorl's arms carry wood. A limb thinner than a voxel
        // is rasterised as the whole chain of voxels its centre line crosses,
        // with a margin (gen.rs), so it comes out fatter than the radius it
        // asked for; where the arm is too short for a sleeve that allows for
        // that, the needles go on alone rather than leaving bare orange stubs
        // out at the tier edges. The top of a conifer is short dense branches,
        // not bare wood, so nothing is lost by it.
        let woody = rad >= 0.9;
        let arms = 4 + (rnd() < 0.5) as usize;
        for _ in 0..arms {
            az += PHI;
            let out = Vec3::new(az.cos(), 0.0, az.sin());
            // How the arm sits: a fir holds its branches out level, a spruce
            // lets them fall, a young pine throws them up, and "ragged" mixes
            // the three down one tree (a wind-beaten hemlock).
            let rise = match sweep {
                Sweep::Droop => -0.34 - 0.18 * rnd(),
                Sweep::Up => 0.20 + 0.18 * rnd(),
                Sweep::Ragged => (rnd() - 0.45) * 0.8,
                // Level, and a touch lower on the long low arms.
                Sweep::Level => -0.05 - 0.12 * (1.0 - f) * rnd(),
            };
            let len = rad * (0.86 + 0.14 * rnd());
            let tip = hub + out * len + Vec3::Y * len * rise;
            // The wood stops well inside the needles that clothe it.
            if woody {
                branches.push((hub, hub.lerp(tip, 0.62), (trunk_r * 0.26).clamp(0.05, 0.14)));
            }
            let sleeve = if woody { (rad * 0.32).max(floor(ARM_NEEDLE)) } else { (rad * 0.44).max(floor(MIN_NEEDLE)) };
            blobs.push(Blob::sleeve(hub.lerp(tip, 0.22), sleeve, (tip - hub) * 0.86));
        }
    }
    trunk_h
}

/// How far a conifer's needles have to stand outside its trunk, at full
/// detail, for the trunk to be covered at all.
///
/// Leaves are meshed in cells of 2x2x2 voxels and a cell is leaves only when
/// five of its eight are (`mesh.rs` `BLOCK_MIN`), so it is not enough for the
/// needles to reach one cell past the bark: there has to be room for a whole
/// cell that holds no bark, and a cell is 50 cm across with its corners 35 cm
/// from its middle. Under about 0.7 m of needles every cell round the trunk
/// is part bark, none of them is leaves, and the wood shows through a crown
/// that looks solid — which is exactly what it did at 0.5.
pub const CONIFER_CLOTHE: f32 = 0.75;

/// How far the needles have to stand outside a trunk of radius `trunk_r`,
/// which is `CONIFER_CLOTHE` only once the trunk is thick enough to need it.
///
/// The margin is there so a leaf cell can fall clear of the bark, so what it
/// really depends on is how many voxels of bark there are: a sapling's stem is
/// one voxel across and the cells beside it are already all leaves, while a
/// redwood's fills cells three deep. Charging every conifer the full 0.75 m
/// made the saplings — whole trees barely a metre wide — into flat-topped
/// slabs of needles.
pub fn conifer_clothe(trunk_r: f32) -> f32 {
    CONIFER_CLOTHE.min(2.5 * trunk_r + 0.12)
}

/// The narrowest any piece of a conifer's needle mass is allowed to get: a
/// clump is drawn solid only to about two thirds of its radius, so below this
/// a piece is under two voxels through and comes apart into specks.
///
/// It is deliberately not large enough to guarantee a full leaf cell. Holding
/// every clump to that (0.7) made the top of each tree a flat-sided pipe of
/// needles as tall as the crown is thin — a 25 m fir tapers to well under a
/// metre across, and at 25 cm voxels nothing can be both that narrow and that
/// solid. The tip is allowed to go thin and lean on its leaf cards for the
/// outline; what must never fail is the needles round the *trunk*, which is
/// `CONIFER_CLOTHE` and is enforced in `gen.rs` as well.
const MIN_NEEDLE: f32 = 0.4;

/// The narrowest the needle core up the leader gets, at the very tip. Wide
/// enough to stay one solid thread of voxels that the mesher can still find
/// leaf cells in, narrow enough that a tall fir ends in a taper rather than a
/// flat-sided pipe.
const TIP_NEEDLE: f32 = 0.5;

/// The narrowest a whorl arm's needle sleeve gets. A limb thinner than a
/// voxel is rasterised as every voxel its centre line crosses, with a margin
/// (`gen.rs`), so the wood is fatter than the radius it asked for: a sleeve
/// that only just covered the nominal limb left bare orange stubs out at the
/// tier edges all over the tree.
const ARM_NEEDLE: f32 = 0.62;

/// How fast a conifer's cone narrows by default. Near 1, which is a straight
/// cone: the silhouette at a few hundred metres is the whole of what a fir is
/// out there, and holding the crown wide to fatten its top (0.62) turned every
/// one of them into a knobbly column. The thin tip is carried by the leaf
/// cards instead.
pub const CONIFER_TAPER: f32 = 0.95;

/// How much of the parent's cross-section a child limb keeps: area is shared
/// out among the children to the power 1/2.3 (Leonardo's rule at 1/2, real
/// trees a little over it), so a limb never starts thicker than the wood that
/// feeds it and the trunk thins as it sheds limbs instead of pinching at one
/// fork.
fn share_r(parent_r: f32, share: f32) -> f32 {
    parent_r * share.clamp(0.02, 1.0).powf(1.0 / 2.3)
}

/// How far a curve turns a limb over its whole length, as (outward, upward)
/// radians. A limb is aimed half this much the other way before it is traced,
/// so the bow fits inside the crown the plant asked for instead of being added
/// to it — an arching limb used to end half as far out again as it was aimed.
fn bend(curve: Curve) -> glam::Vec2 {
    match curve {
        Curve::Arch => glam::Vec2::new(0.78, -0.16),
        Curve::Sweep => glam::Vec2::new(0.70, 0.76),
        Curve::Droop => glam::Vec2::new(0.40, -1.05),
        Curve::Kinked => glam::Vec2::new(0.28, 0.0),
        Curve::Straight => glam::Vec2::new(0.10, 0.0),
    }
}

/// Trace one limb as a chain of capsules along a curve, from `at` leaving
/// along `dir` and `len` m long, tapering from `r0` to `r1`. `out` is the
/// horizontal direction away from the trunk. Returns the path's points and
/// the direction it ends in, so foliage and the next fork can follow it.
#[allow(clippy::too_many_arguments)]
fn trace(branches: &mut Vec<(Vec3, Vec3, f32)>, curve: Curve, at: Vec3, dir: Vec3, out: Vec3, len: f32, r0: f32, r1: f32, segs: usize, rnd: &mut dyn FnMut() -> f32) -> (Vec<Vec3>, Vec3) {
    let mut d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        d = Vec3::Y;
    }
    let side = Vec3::new(-out.z, 0.0, out.x);
    let step = len / segs as f32;
    let mut p = at;
    let mut pts = vec![p];
    // How hard the limb turns per segment, and which way, by style.
    let wander = 0.1 + 0.12 * rnd();
    for s in 0..segs {
        let t = (s as f32 + 0.5) / segs as f32;
        let turn = match curve {
            // Bows outward over its whole length and levels off: the classic
            // spreading limb.
            Curve::Arch => out * (0.52 / segs as f32) * (1.0 + t) - Vec3::Y * (0.16 / segs as f32) * t * 2.0,
            // Out, then up: the S of a maple or an elm, tip lifted to the light.
            Curve::Sweep => out * (0.78 / segs as f32) * (1.4 - t) + Vec3::Y * (0.95 / segs as f32) * t * t * 1.6,
            // Over and down, a willow's or an old apple's bough.
            Curve::Droop => out * (0.40 / segs as f32) - Vec3::Y * (1.05 / segs as f32) * t * 1.8,
            // Kinks: no net bow, a new angle at every joint (juniper, hazel).
            Curve::Kinked => (side * if s % 2 == 0 { 1.0 } else { -1.0 } + out * 0.25) * (0.55 / segs as f32) * 2.0,
            Curve::Straight => out * (0.10 / segs as f32),
        };
        let jitter = (side * (rnd() - 0.5) + Vec3::Y * (rnd() - 0.5) * 0.6) * wander / segs as f32 * 2.0;
        d = (d + turn + jitter).normalize_or_zero();
        if d == Vec3::ZERO {
            d = Vec3::Y;
        }
        let end = p + d * step;
        let r = r0 + (r1 - r0) * t;
        branches.push((p, end, r.max(0.1)));
        p = end;
        pts.push(p);
    }
    (pts, d)
}

/// A point a fraction `f` along a traced path.
fn along(pts: &[Vec3], f: f32) -> Vec3 {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or(Vec3::ZERO);
    }
    let x = f.clamp(0.0, 1.0) * (pts.len() - 1) as f32;
    let i = (x.floor() as usize).min(pts.len() - 2);
    pts[i] + (pts[i + 1] - pts[i]) * (x - i as f32)
}

/// Build a branching tree: trunk, limbs and crown. Returns (kind, trunk
/// height, the height foliage starts at), as every growth form does.
#[allow(clippy::too_many_arguments)]
fn branching(
    p: &PlantDef,
    rnd: &mut dyn FnMut() -> f32,
    base: Vec3,
    height: f32,
    trunk_r: f32,
    crown: f32,
    flower: Option<u8>,
    blobs: &mut Vec<Blob>,
    branches: &mut Vec<(Vec3, Vec3, f32)>,
) -> (TreeKind, f32, f32) {
    let spread = p.spread.clamp(0.0, 1.0);
    let curve = p.curve.unwrap_or(Curve::Arch);
    let canopy = p.canopy.unwrap_or(Canopy::Lobes);
    let fill = p.fill.unwrap_or(0.55).clamp(0.0, 1.0);
    // The leader: how much of the trunk carries on up through the crown
    // instead of being spent in the lowest fork. 0 gives an oak's open vase,
    // 1 a cottonwood's single spar with laterals all the way up.
    let leader = p.leader.unwrap_or(0.35).clamp(0.0, 1.0);
    // Several stems from one root (elder, hazel): they leave the ground
    // leaning and take the place of the first limbs.
    let basal = p.stems.filter(|_| p.form == Form::Branching).map(|s| lerp(s, rnd()).round().max(1.0) as usize).unwrap_or(1);

    // The trunk's clear length. A wide crown forks lower; a multi-stemmed
    // plant has almost no trunk at all.
    let trunk_h = if basal > 1 { height * 0.1 + 0.25 } else { height * (0.46 - 0.12 * spread) };
    let first_lo = if basal > 1 { trunk_h * 0.55 } else { trunk_h * 0.72 };

    // Where the first limbs leave: staggered up the top quarter of the trunk,
    // never all from one node. The node that pinched every old branching tree
    // was one point with three limbs radiating from it.
    let n_first = if basal > 1 {
        basal
    } else {
        3 + (rnd() < 0.35 + 0.5 * spread) as usize + (rnd() < spread * 0.5) as usize
    };
    let depth = if height > 9.0 { 2 } else { 1 };
    let a0 = rnd() * TAU;
    // The golden angle, so limbs never line up two above two.
    const PHI: f32 = 2.399_963;
    let mut trunk_r_at = trunk_r * p.limb.unwrap_or(0.88);
    // Each first-order limb, as the twigs it ends in: a lobe of foliage is
    // then grown to cover those twigs, so the leaves always reach the ends of
    // the wood instead of leaving long bare limbs sticking out of the crown.
    let mut lobes: Vec<(Vec<Vec3>, Vec3)> = Vec::new();
    let mut twigs: Vec<Vec<Vec3>> = Vec::new();

    for k in 0..n_first {
        let f = k as f32 / n_first.max(1) as f32;
        let y = first_lo + (trunk_h - first_lo) * f + (rnd() - 0.5) * 0.2;
        let az = a0 + k as f32 * PHI + (rnd() - 0.5) * 0.5;
        let out = Vec3::new(az.cos(), 0.0, az.sin());
        let foot = base + Vec3::Y * y;
        // A limb is aimed by the box the crown is meant to fill: so far out
        // (`run`) and so far up (`rise`). The lowest limbs reach furthest out
        // and climb least, the top ones the other way about, and the two
        // together set the limb's length — a tall tree's crown then reaches
        // its height instead of spreading in a low flat plate.
        let deep = height - trunk_h;
        // The limb covers a little over half the crown's reach; the forks off
        // it and the lobe over them make up the rest.
        let run = crown * (0.30 + 0.20 * spread) * (1.2 - 0.4 * f) * (0.85 + 0.3 * rnd());
        let rise = deep * (0.38 + 0.30 * f) * (1.0 - 0.25 * spread) * (0.85 + 0.3 * rnd());
        let aim = (Vec3::Y * rise + out * run).normalize();
        let b = bend(curve);
        let dir = (aim - out * b.x * 0.5 - Vec3::Y * b.y * 0.5).normalize();
        // Area conservation: this limb takes a share of the trunk and the
        // trunk goes on thinner. The share falls as the leader keeps more.
        let left = (n_first - k) as f32;
        let share = (1.0 - leader * 0.55) / left;
        let r = share_r(trunk_r_at, share.clamp(0.12, 0.85));
        trunk_r_at = share_r(trunk_r_at, (1.0 - share).clamp(0.2, 0.97));
        let len = (rise * rise + run * run).sqrt();
        let (pts, end_dir) = trace(branches, curve, foot, dir, out, len, r, r * 0.68, 4, rnd);
        let mut mine = Vec::new();
        fork(branches, curve, &pts, end_dir, out, len, r * 0.68, depth, rnd, &mut mine);
        lobes.push((mine.iter().map(|t: &Vec<Vec3>| *t.last().unwrap()).collect(), out));
        twigs.extend(mine);
    }
    // The leader above the last fork: a spar of what is left of the trunk,
    // with its own small limbs, for the trees that keep one (maples,
    // cottonwoods). Drawn as limbs, so the trunk's cylinder stops at the fork.
    if leader > 0.08 && basal == 1 {
        let foot = base + Vec3::Y * trunk_h;
        let len = height * 0.32 * leader + 0.6;
        let az = rnd() * TAU;
        let out = Vec3::new(az.cos(), 0.0, az.sin());
        let (pts, end_dir) = trace(branches, Curve::Straight, foot, Vec3::Y, out * 0.3, len, trunk_r_at, trunk_r_at * 0.6, 3, rnd);
        let mut mine = Vec::new();
        fork(branches, curve, &pts, end_dir, out, len * 0.8, trunk_r_at * 0.6, depth, rnd, &mut mine);
        lobes.push((mine.iter().map(|t: &Vec<Vec3>| *t.last().unwrap()).collect(), out));
        twigs.extend(mine);
    }

    crown_mass(canopy, blobs, &lobes, &twigs, crown, fill, spread, p.dome.unwrap_or(DOME).clamp(0.0, 1.0), flower, rnd);
    let canopy_lo = lobes.iter().flat_map(|(tips, _)| tips.iter().map(|t| t.y)).fold(f32::MAX, f32::min).min(base.y + trunk_h) - base.y - crown * 0.4;
    (TreeKind::Broad, trunk_h, canopy_lo.max(0.0))
}

/// Fork the end of a limb into two or three, and again, down to the twigs,
/// which are collected in `twigs` for the foliage pass.
#[allow(clippy::too_many_arguments)]
fn fork(branches: &mut Vec<(Vec3, Vec3, f32)>, curve: Curve, pts: &[Vec3], dir: Vec3, out: Vec3, len: f32, r: f32, depth: u32, rnd: &mut dyn FnMut() -> f32, twigs: &mut Vec<Vec<Vec3>>) {
    let at = *pts.last().unwrap();
    if depth == 0 {
        twigs.push(pts.to_vec());
        return;
    }
    let n = 2 + (rnd() < 0.45) as usize;
    let a0 = rnd() * TAU;
    for k in 0..n {
        let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.7;
        // A child leans out from its parent, not from the trunk: the spread
        // of a crown comes from these, not from the first limbs' angle.
        let o = (Vec3::new(az.cos(), 0.0, az.sin()) * 0.75 + out * 0.5).normalize_or_zero();
        let o = if o == Vec3::ZERO { out } else { o };
        let tilt = 0.20 + 0.22 * rnd();
        let d = (dir * (1.0 - tilt) + o * tilt).normalize();
        let cr = share_r(r, 1.0 / n as f32);
        // Well under half the parent: the twigs stay inside the lobe of
        // foliage grown over them.
        let cl = len * (0.26 + 0.12 * rnd());
        let (cp, cd) = trace(branches, curve, at, d, o, cl, cr, cr * 0.6, 3, rnd);
        fork(branches, curve, &cp, cd, o, cl, cr * 0.6, depth - 1, rnd, twigs);
    }
}

/// Mass the foliage over the limbs in the named style. Each entry of `lobes`
/// is one first-order limb's twig tips with the way the limb heads; `twigs`
/// are all the terminal paths.
#[allow(clippy::too_many_arguments)]
fn crown_mass(style: Canopy, blobs: &mut Vec<Blob>, lobes: &[(Vec<Vec3>, Vec3)], twigs: &[Vec<Vec3>], crown: f32, fill: f32, spread: f32, dome: f32, flower: Option<u8>, rnd: &mut dyn FnMut() -> f32) {
    // Leaves clothing the outer part of every twig, in every style: a limb
    // ends in foliage along it, not in one ball stuck on the tip.
    let sleeve_r = crown
        * match style {
            Canopy::Tufts => 0.19,
            Canopy::Layers => 0.16,
            _ => 0.17,
        };
    for pts in twigs {
        let a = along(pts, 0.45);
        let b = *pts.last().unwrap();
        let r = sleeve_r * (0.85 + 0.3 * rnd());
        if style == Canopy::Tufts {
            blobs.push(Blob::ball(b + Vec3::Y * r * 0.3, r * 1.2));
        } else {
            blobs.push(Blob::sleeve(a, r, b - a));
        }
    }
    // One lobe per limb: centred on its twigs and just big enough to cover
    // them, so no wood is left bare outside the leaves.
    let mut mass: Vec<(Vec3, f32, Vec3)> = Vec::new();
    for (tips, out) in lobes {
        if tips.is_empty() {
            continue;
        }
        let c = tips.iter().fold(Vec3::ZERO, |a, b| a + *b) / tips.len() as f32;
        let span = tips.iter().map(|t| t.distance(c)).fold(0.0f32, f32::max);
        let wide = if style == Canopy::Vase { 0.68 } else { 0.55 };
        let r = (span + sleeve_r * 0.9).clamp(crown * 0.3, crown * wide);
        mass.push((c, r, *out));
    }
    if mass.is_empty() {
        return;
    }
    let n = mass.len() as f32;
    let mid = mass.iter().fold(Vec3::ZERO, |a, b| a + b.0) / n;
    let rx = mass
        .iter()
        .map(|(c, r, _)| Vec3::new(c.x - mid.x, 0.0, c.z - mid.z).length() + r * 0.5)
        .fold(0.0f32, f32::max)
        .max(crown * 0.5);
    let top = mass.iter().map(|(c, r, _)| c.y + r * 0.4).fold(f32::MIN, f32::max);

    match style {
        // Flat tiers over the limbs: an elder's or a dogwood's layered crown,
        // a cypress's flat open top. A tier is a low dome over each lobe with
        // a plate knitting it to the next, not one disc across the whole
        // crown, which came out a hard mushroom cap.
        Canopy::Layers => {
            for (c, r, _) in &mass {
                blobs.push(Blob::dome(*c + Vec3::Y * r * 0.05, r * (1.05 + 0.15 * rnd()), 0.42 + 0.12 * rnd()));
            }
            for i in 0..mass.len() {
                if mass.len() < 2 || rnd() > 0.5 + 0.5 * fill {
                    continue;
                }
                let (a, ra, _) = mass[i];
                let (b, rb, _) = mass[(i + 1) % mass.len()];
                if a.distance(b) > (ra + rb) * 1.4 {
                    continue;
                }
                blobs.push(Blob::plate((a + b) * 0.5, (a.distance(b) * 0.45).max(crown * 0.3), Vec3::Y, 0.4 + 0.15 * rnd()));
            }
        }
        // Nothing over the twigs: an open, sparse crown.
        Canopy::Tufts => {}
        // One soft mass over the whole crown, few notches: the lobes with
        // every notch bridged, under a dome that sits on top of them. Centred
        // at the lobes' tops, so the crown stays hollow underneath and the
        // limbs show below it (a live oak's reaching boughs).
        Canopy::Cloud => {
            for (c, r, _) in &mass {
                blobs.push(Blob::dome(*c + Vec3::Y * r * 0.1, *r, 0.95));
            }
            for i in 0..mass.len() {
                let (a, _, _) = mass[i];
                let (b, _, _) = mass[(i + 1) % mass.len()];
                if mass.len() > 1 {
                    blobs.push(Blob::plate((a + b) * 0.5, (a.distance(b) * 0.5).max(crown * 0.35), Vec3::Y, 0.5));
                }
            }
            blobs.push(Blob::dome(Vec3::new(mid.x, top - crown * 0.15, mid.z), rx * (0.8 + 0.2 * dome), 0.7));
        }
        // Lobes over the limbs. "vase" stands them taller and pulls them in
        // over the trunk, for a beech's or an elm's upswept crown; the
        // default lets them lie out broad, an oak's or a maple's.
        _ => {
            let (rise, pull, bridge) = if style == Canopy::Vase { (1.25, -0.18, (fill * 0.5 + 0.5).min(1.0)) } else { (1.0, 0.08, fill) };
            for (c, r, out) in &mass {
                blobs.push(Blob::dome(*c + *out * r * pull + Vec3::Y * r * 0.12, *r, rise + 0.2 * rnd()));
            }
            let fill = bridge;
            // Infill: a plate bridging each neighbouring pair of lobes, some
            // of the time, part of the way. `fill` 0 leaves every notch open.
            for i in 0..mass.len() {
                if mass.len() < 2 || rnd() > fill {
                    continue;
                }
                let (a, ra, _) = mass[i];
                let (b, rb, _) = mass[(i + 1) % mass.len()];
                let gap = a.distance(b);
                if gap > (ra + rb) * 1.5 {
                    continue;
                }
                let c = (a + b) * 0.5 + Vec3::Y * crown * 0.08;
                blobs.push(Blob::plate(c, (gap * 0.45).clamp(crown * 0.25, crown * 0.7) * (0.7 + 0.35 * fill), Vec3::Y, 0.45 + 0.2 * rnd()));
            }
            // A broken crest: a few small domes riding over the top, where
            // `dome` asks for a fuller crown.
            let crest = (dome * (2.0 + 2.0 * spread)).round() as usize;
            for _ in 0..crest {
                let a = rnd() * TAU;
                let d = rx * 0.6 * rnd().sqrt();
                blobs.push(Blob::dome(
                    Vec3::new(mid.x + a.cos() * d, top - crown * (0.1 + 0.25 * rnd()), mid.z + a.sin() * d),
                    crown * (0.36 + 0.2 * rnd()),
                    0.85,
                ));
            }
        }
    }

    // Flower heads: flat corymbs lying on the sunlit top of the crown, in
    // their own colour over the green (elder, rowan, hawthorn).
    if let Some(fl) = flower {
        let heads = 6 + (rnd() * 7.0) as usize;
        for _ in 0..heads {
            let a = rnd() * TAU;
            let d = rx * (0.2 + 0.8 * rnd().sqrt());
            let y = top - crown * (0.22 - 0.2 * rnd());
            blobs.push(Blob::plate(Vec3::new(mid.x + a.cos() * d, y, mid.z + a.sin() * d), crown * (0.16 + 0.1 * rnd()), Vec3::Y, 0.3).of(fl));
        }
    }
}

/// A herb marker for a flower stalk (plant `stalk`) at the heart of the plant
/// standing at `base`: one voxel of `mat` that the sprite pass draws as the
/// stalk (mesh/sprites.rs `Herb::Stalk`). The marker climbs out of the
/// rosette's solid heart when it is drawn (gen.rs draw_tree).
fn stalk_marker(mat: u8, base: Vec3, seed: u32) -> Tree {
    Tree {
        pos: base,
        kind: TreeKind::Broad,
        trunk_h: 0.0,
        trunk_r: 0.0,
        blobs: Vec::new(),
        canopy_lo: 0.0,
        top: 1.0,
        radius: 1.0,
        leaf: mat,
        bark: 0,
        seed,
        squash: 1.0,
        branches: Vec::new(),
        plant: crate::world::gen::NO_PLANT,
        flare: 0.0,
        autumn: 0,
        flower: 0,
        sticks: 0.0,
    }
}

/// How much a placement rule wants a plant at `p` (0..1).
fn weight(w: &World, pl: &Placement, p: Vec2, biome: u8, seed: u32) -> f32 {
    let f = &w.f;
    let par = |k: &str, d: f32| pl.params.get(k).copied().unwrap_or(d);
    match pl.verb {
        PlaceVerb::Clustered => {
            let s = par("size", 40.0);
            let n = fbm2(p.x / s, p.y / s, 3, seed) * 0.5 + 0.5;
            super::noise::smoothstep(par("min", 0.45), par("max", 0.62), n)
        }
        PlaceVerb::Edge => {
            // Near where this biome meets another; with `field`, only one of
            // the biomes it names, comma-separated ("rib_dunes,rib_plain":
            // a beach's edge where the grass starts).
            let width = par("width", 14.0);
            let only = pl
                .field
                .as_deref()
                .map(|names| names.split(',').filter_map(|n| w.lib.biomes.iter().position(|b| b.name == n.trim())).collect::<Vec<_>>());
            let mut near = false;
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let q = p + Vec2::new(a.cos(), a.sin()) * width;
                let other = f.biome[f.idx(q.x, q.y)];
                if other != biome && only.as_ref().is_none_or(|o| o.contains(&(other as usize))) {
                    near = true;
                    break;
                }
            }
            if near {
                1.0
            } else {
                par("inner", 0.05)
            }
        }
        PlaceVerb::Riparian => {
            let d = f.river_dist[f.idx(p.x, p.y)];
            (-d / par("width", 12.0)).exp()
        }
        PlaceVerb::Gradient => {
            let i = f.idx(p.x, p.y);
            let v = match pl.field.as_deref() {
                Some("strange") => f.strange[i],
                Some("moist") => f.moist[i],
                Some("elev") => f.h[i] - f.sea,
                Some("slope") => f.slope[i],
                Some("coast") => f.coast_dist[i],
                _ => 1.0,
            };
            super::noise::smoothstep(par("min", 0.0), par("max", 1.0), v)
        }
        PlaceVerb::Ring => {
            let c = Vec2::new(par("x", 0.0), par("z", 0.0));
            let d = p.distance(c);
            (-((d - par("radius", 50.0)) / par("width", 10.0)).powi(2)).exp()
        }
        _ => 1.0,
    }
}

/// Every biome's plants, by their placement rules.
/// A plant the scan has decided on, waiting to be grown.
///
/// Growing one is pure -- the library, a spot and a seed in, a tree out --
/// and it is most of what this stage costs, so the growing is done after the
/// scan, in bands across the machine's cores. The scan itself has to stay in
/// one piece: every plant placed blocks the ground around it for the ones
/// after it (`taken`), so which plants there are depends on the order they
/// are decided in.
enum Sow {
    /// A library plant. `depth` is the water over it, for one that grows up
    /// to the surface.
    Plant { gi: usize, depth: Option<f32>, base: Vec3, seed: u32, worn: Worn },
    /// The flower stalk from the heart of a rosette.
    Stalk { mat: u8, base: Vec3, seed: u32 },
}

/// Grow what the scan decided on, in the order it decided.
fn grow_all(w: &World, sow: &[Sow]) -> Vec<super::gen::Tree> {
    let threads = crate::world::fields::build_threads();
    let per = sow.len().div_ceil(threads).max(1);
    let lib = &w.lib;
    let one = |s: &Sow| match s {
        Sow::Stalk { mat, base, seed } => stalk_marker(*mat, *base, *seed),
        Sow::Plant { gi, depth, base, seed, worn } => {
            let plant = &lib.plants[*gi];
            match depth {
                // Kelp's height is a share of the water over it: up to the
                // surface. Coral keeps its own.
                Some(d) if plant.form == Form::Kelp => {
                    let mut g = plant.clone();
                    g.height = [d * g.height[0], d * g.height[1]];
                    grow(lib, &g, *base, *seed, *worn)
                }
                // Coral stays under the water it grows in: on a shallow flat a
                // colony is as tall as the water allows, not taller.
                Some(d) if plant.form.is_coral() && plant.height[1] > d - 0.35 => {
                    let mut g = plant.clone();
                    let top = (d - 0.35).max(0.3);
                    g.height = [g.height[0].min(top), top];
                    g.crown_radius = g.crown_radius.map(|c| c * (top / plant.height[1]).clamp(0.5, 1.0));
                    grow(lib, &g, *base, *seed, *worn)
                }
                _ => grow(lib, plant, *base, *seed, *worn),
            }
        }
    };
    let mut bands: Vec<Vec<super::gen::Tree>> = Vec::new();
    std::thread::scope(|sc| {
        let mut handles = Vec::new();
        for band in sow.chunks(per) {
            let one = &one;
            handles.push(sc.spawn(move || band.iter().map(one).collect::<Vec<_>>()));
        }
        bands = handles.into_iter().map(|h| h.join().expect("a band of plants")).collect();
    });
    bands.into_iter().flatten().collect()
}

pub fn place_plants(w: &mut World) {
    let n = w.f.n;
    let mut taken = vec![false; n * n];
    // Structures and paths are off limits; mark footprints. So is the ground
    // under a deck that stands clear of it: a crown would come up through the
    // planks.
    // Water plants may grow in a lake or a cenote deeper than a couple of
    // metres; everything else keeps out of open water.
    // Water at the sea's own level is the sea's (a river's channel on past
    // its mouth, for the tide), as `Generator::water_m` has it.
    let inland_depth = |i: usize| w.f.inland.get(&(i as u32)).filter(|wet| wet.level > w.f.sea).map(|wet| wet.level - w.f.h[i]);
    // Under a high deck (a canopy walk) the ground still grows, but only
    // what stays under the planks: the room there, per cell, metres.
    let mut room = vec![f32::MAX; n * n];
    for i in 0..n * n {
        let deep_inland = w.f.water[i] == 1 && inland_depth(i).is_some_and(|d| d > 2.0);
        if w.f.occupied[i] || (w.f.water[i] != 0 && !deep_inland) {
            taken[i] = true;
        }
        if w.ribbons.is_empty() {
            continue;
        }
        if let Some(y) = w.ribbons.deck_over(w.f.centre(i), 2.0) {
            let up = y - w.f.h[i];
            if up < super::route::HIGH_DECK {
                taken[i] = true;
            } else {
                room[i] = up;
            }
        }
    }
    let size = w.f.size;
    let sea = w.f.sea;
    let mut sow: Vec<Sow> = Vec::new();
    let mut vines = Vec::new();
    // Only the biomes some cell took: the library holds every world's, and
    // each plant entry walks the whole map.
    let mut present = vec![false; w.lib.biomes.len()];
    for &b in &w.f.biome {
        present[b as usize] = true;
    }
    for (bi, b) in w.lib.biomes.iter().enumerate() {
        if !present[bi] {
            continue;
        }
        for (ui, u) in b.plants.iter().enumerate() {
            let Some(pi) = w.lib.plant(&u.plant) else {
                continue;
            };
            let plant = &w.lib.plants[pi];
            let morphs = w.lib.plant_morphs(pi);
            if u.density <= 0.0 {
                continue;
            }
            // Vines and crusts need their hosts placed first.
            if plant.form == Form::Vine || plant.form == Form::Crust {
                vines.push((bi, ui, pi));
                continue;
            }
            let g = (1.0 / u.density).sqrt().max(0.8);
            let seed = hash32(w.def.seed ^ name_key(&w.lib.biomes[bi].name) ^ use_key(w.def.seeds_v1(), ui, &u.plant, 0x9e37_79b9));
            let spacing = pl_spacing(&u.placement);
            let mut lone: Vec<Vec2> = Vec::new();
            let cells = (size / g).ceil() as i32;
            let clear = match plant.form {
                Form::Herb => 0.7,
                Form::Shrub => 0.8,
                _ => 1.6,
            };
            for cz in 0..cells {
                for cx in 0..cells {
                    let h0 = hash2(cx, cz, seed);
                    let p = Vec2::new((cx as f32 + 0.15 + 0.7 * unit(h0)) * g, (cz as f32 + 0.15 + 0.7 * unit(hash32(h0 ^ 1))) * g);
                    if p.x < 2.0 || p.y < 2.0 || p.x > size - 2.0 || p.y > size - 2.0 {
                        continue;
                    }
                    let i = w.f.idx(p.x, p.y);
                    if w.f.biome[i] as usize != bi || taken[i] || room[i] < plant.height[1] + 1.0 {
                        continue;
                    }
                    // A tree whose crown would stand round a high deck (a
                    // canopy platform, a walkway) grows somewhere else: from
                    // the deck you would see nothing but its leaves.
                    if plant.height[1] > super::route::HIGH_DECK && plant.form != Form::Herb && !w.ribbons.is_empty() {
                        let g = w.f.h[i];
                        let crown = (g + plant.crown_base.unwrap_or(0.35) * plant.height[0] - 2.0, g + plant.height[1] + 2.0);
                        if w.ribbons.deck_in(p, plant.crown_radius[1] + 1.5, g + super::route::HIGH_DECK, crown.0, crown.1) {
                            continue;
                        }
                    }
                    if w.f.slope[i] > plant.max_slope {
                        continue;
                    }
                    // Beside a sheer drop the smoothed height is somewhere in
                    // the air between the top and the foot: nothing grows there.
                    if (w.f.height(p.x, p.y) - w.f.h[i]).abs() > 0.75 {
                        continue;
                    }
                    // Land plants keep out of the sea; water plants grow only
                    // under their depth of it.
                    // Under a lake or a cenote, its own surface; else the sea's.
                    let depth = inland_depth(i).filter(|_| w.f.water[i] == 1).unwrap_or(sea - w.f.h[i]);
                    match (plant.water_depth, plant.standing_water) {
                        (Some([lo, hi]), _) | (None, Some([lo, hi])) if depth < lo || depth > hi => continue,
                        (None, None) if depth > -0.6 => continue,
                        _ => {}
                    }
                    // A coral needs room under the surface for its smallest
                    // colony; on a flat too shallow for it, something lower
                    // grows instead.
                    if plant.form.is_coral() && depth - 0.35 < plant.height[0] * 0.75 {
                        continue;
                    }
                    if unit(hash32(h0 ^ 2)) > weight(w, &u.placement, p, bi as u8, seed) {
                        continue;
                    }
                    if let Some(sp) = spacing {
                        if lone.iter().any(|q| q.distance(p) < sp) {
                            continue;
                        }
                        lone.push(p);
                    }
                    // Keep trunks apart.
                    let r = (clear / CELL) as i32;
                    let (ix, iz) = ((p.x / CELL) as i32, (p.y / CELL) as i32);
                    let mut blocked = false;
                    'chk: for dz in -r..=r {
                        for dx in -r..=r {
                            let (x, z) = (ix + dx, iz + dz);
                            if x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n && taken[z as usize * n + x as usize] && (dx != 0 || dz != 0) {
                                blocked = true;
                                break 'chk;
                            }
                        }
                    }
                    if blocked {
                        continue;
                    }
                    taken[i] = true;
                    let base = Vec3::new(p.x, w.f.height(p.x, p.y) - 0.1, p.y);
                    // Now and then a rare morph grows in its place.
                    let gi = crate::variety::pick(&morphs, unit(hash32(h0 ^ 0x6d6f_7270))).unwrap_or(pi);
                    let grown = &w.lib.plants[gi];
                    // What this patch of ground is growing: one coat for the
                    // blob, so a cluster is of one colour.
                    let worn = coat_at(&w.lib, gi, u.morph.as_ref(), p.x, p.y, seed);
                    sow.push(Sow::Plant {
                        gi,
                        depth: grown.water_depth.is_some().then_some(depth),
                        base,
                        seed: hash32(h0 ^ 0x1234_5678),
                        worn,
                    });
                    // A flower stalk from the heart of some rosettes.
                    if let Some((mat, share)) = &grown.stalk {
                        if unit(hash32(h0 ^ 0x7374_616c)) < *share {
                            if let Some(m) = worn.get("stalk").or_else(|| w.lib.material(mat)) {
                                sow.push(Sow::Stalk {
                                    mat: m,
                                    base,
                                    seed: hash32(h0 ^ 0x5157),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    let mut trees = grow_all(w, &sow);
    trees.extend(place_growths(w));
    for (bi, ui, pi) in vines {
        if w.lib.plants[pi].form == Form::Crust {
            place_crusts(w, &mut trees, bi, ui, pi);
        } else {
            place_vines(w, &mut trees, bi, ui, pi);
        }
    }
    w.trees = trees;
}

/// Plants the chronicle put at exact spots (tower ledges): trees grown where
/// asked, vines as a curtain hanging from the spot, across the way out.
fn place_growths(w: &World) -> Vec<super::gen::Tree> {
    use super::gen::{Tree, TreeKind};
    let mut out = Vec::new();
    for (k, g) in w.growths.iter().enumerate() {
        let Some(pi) = w.lib.plant(&g.plant) else {
            continue;
        };
        let p = &w.lib.plants[pi];
        let seed = g.seed.unwrap_or_else(|| hash32(w.def.seed ^ 0x6c65_6467 ^ (k as u32).wrapping_mul(0x9e37_79b9)));
        let mut rng = seed;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            unit(rng)
        };
        if p.form != Form::Vine {
            out.push(grow(&w.lib, p, g.at - Vec3::Y * 0.1, seed, coat_at(&w.lib, pi, None, g.at.x, g.at.z, w.def.seed)));
            continue;
        }
        let Some(leaf) = p.leaf.as_ref().and_then(|l| w.lib.material(l)) else {
            continue;
        };
        let width = p.crown_radius[0] + (p.crown_radius[1] - p.crown_radius[0]) * rnd();
        let side = Vec2::new(-g.out.y, g.out.x);
        let n = (width / 0.5).max(2.0) as i32;
        let mut strands = Vec::new();
        for j in 0..n {
            let off = (j as f32 / (n - 1).max(1) as f32 - 0.5) * width;
            let at = Vec2::new(g.at.x, g.at.z) + side * off + g.out * (0.25 + 0.3 * rnd());
            let len = (p.height[0] + (p.height[1] - p.height[0]) * rnd()) * (0.7 + 0.6 * rnd());
            strands.extend(down_face(w, at, g.out, g.at.y + 0.1, len).into_iter().map(|(c, l)| Blob::ball(c, l)));
        }
        if strands.is_empty() {
            continue;
        }
        let c = strands.iter().fold(Vec3::ZERO, |a, x| a + x.c) / strands.len() as f32;
        let bottom = strands.iter().map(|x| x.c.y - x.r).fold(f32::MAX, f32::min);
        let top = strands.iter().map(|x| x.c.y).fold(f32::MIN, f32::max);
        let radius = strands.iter().map(|x| Vec2::new(x.c.x - c.x, x.c.z - c.z).length()).fold(0.0, f32::max) + 0.6;
        out.push(Tree {
            pos: Vec3::new(c.x, bottom, c.z),
            kind: TreeKind::Vine,
            trunk_h: 0.0,
            trunk_r: 0.0,
            blobs: strands,
            canopy_lo: 0.0,
            top: top - bottom + 0.3,
            radius,
            leaf,
            bark: leaf,
            seed,
            squash: 1.0,
            branches: Vec::new(),
            plant: pi as u16,
            flare: 0.0,
            autumn: 0,
            flower: 0,
            sticks: 0.0,
        });
    }
    out
}

/// Whether (x, z) is the top of a volumetric tower.
fn on_tower(w: &World, q: Vec2) -> bool {
    w.towers.iter().any(|t| t.under_at(q.x, q.y).is_some())
}

/// Rock at p: inside one of `towers`, or under the ground (a tower's foot,
/// not its top).
fn rock_at(w: &World, towers: &[&super::fields::Tower], p: Vec3) -> bool {
    if towers.iter().any(|t| t.inside(p)) {
        return true;
    }
    let ground = towers.iter().filter_map(|t| t.under_at(p.x, p.z)).fold(w.f.height(p.x, p.z), f32::min);
    p.y < ground
}

/// A vine strand hanging from `top` (y) at `at`, `len` m down a face whose
/// open side is `out`: metre lengths, each moved out just far enough to clear
/// the rock over its whole metre, so the strand follows a cliff or a tower's
/// bulges down (draw_vine joins the pieces) instead of running into the rock
/// and showing again below. Stops a little above the ground, or where it
/// would have to stand more than 6 m off the face.
fn down_face(w: &World, at: Vec2, out: Vec2, top: f32, len: f32) -> Vec<(Vec3, f32)> {
    let mut pieces = Vec::new();
    // Only the towers the strand can touch: it stays within 6 m of `at`
    // (Jadewater has fifty towers and thousands of strands).
    let towers: Vec<&super::fields::Tower> = w.towers.iter().filter(|t| t.c.distance(at) < t.reach + 8.0).collect();
    let end = top - len;
    let mut y = top;
    let mut o: f32 = 0.0;
    while y > end + 0.05 {
        let seg = (y - end).min(1.0);
        // Out until clear; then back in toward the face while that stays clear.
        let clear = |o: f32| {
            let q = at + out * o;
            [0.0, 0.35, 0.7, seg].iter().all(|dy| !rock_at(w, &towers, Vec3::new(q.x, y - dy, q.y)))
        };
        while o < 6.0 && !clear(o) {
            o += 0.25;
        }
        if o >= 6.0 {
            break;
        }
        while o >= 0.25 && clear(o - 0.25) {
            o -= 0.25;
        }
        let q = at + out * o;
        let ground = w.f.height(q.x, q.y).min(towers.iter().filter_map(|t| t.under_at(q.x, q.y)).fold(f32::MAX, f32::min));
        if y - seg < ground + 0.3 && !towers.iter().any(|t| t.inside(Vec3::new(q.x, y - seg - 0.3, q.y))) {
            // The ground is close: the last piece reaches down to it.
            let last = (y - ground - 0.3).max(0.0);
            if last > 0.1 {
                pieces.push((Vec3::new(q.x, y, q.y), last.min(seg)));
            }
            break;
        }
        pieces.push((Vec3::new(q.x, y, q.y), seg));
        y -= 1.0;
    }
    pieces
}

/// A vine plant's strands on its hosts in biome `bi`: hanging from the crowns
/// of broadleaf trees and swagged between neighbouring crowns, draping down
/// from the lip of cliffs, and hanging on the outer walls of structures. The
/// biome entry's `density` is the chance that a host carries one: each
/// broadleaf tree, each 3 m cell along a cliff top, and (scaled by
/// `wall_vine_share`) each structure.
fn place_vines(w: &World, trees: &mut Vec<super::gen::Tree>, bi: usize, ui: usize, pi: usize) {
    use super::gen::{Tree, TreeKind};
    let u = &w.lib.biomes[bi].plants[ui];
    let p = &w.lib.plants[pi];
    let Some(plain) = p.leaf.as_ref().and_then(|l| w.lib.material(l)) else {
        return;
    };
    let host = |h: &str| p.hosts.is_empty() || p.hosts.iter().any(|x| x == h);
    let seed = hash32(w.def.seed ^ name_key(&w.lib.biomes[bi].name) ^ use_key(w.def.seeds_v1(), ui, &u.plant, 0x85eb_ca6b) ^ 0x7669_6e65);
    // A vine is one material all through, so its coat is drawn where it
    // hangs: a curtain of wisteria is violet or lilac or white, and the next
    // tree along carries another.
    let leaf_at = |x: f32, z: f32| coat_at(&w.lib, pi, u.morph.as_ref(), x, z, seed).get("leaf").unwrap_or(plain);
    // A vine turns with the year like any other leaf (world/season.rs). It
    // keeps hanging where it hangs in winter, wearing that colour: a creeper
    // stripped bare would leave nothing on the wall at all.
    let autumn = p.autumn.as_ref().and_then(|a| w.lib.material(a)).unwrap_or(0);
    let f = &w.f;
    // Strands are (top, length); swags (one crown, the other, sag) go in
    // `branches`. The record's bounds cover both.
    let make = |strands: Vec<(Vec3, f32)>, swags: Vec<(Vec3, Vec3, f32)>, s: u32| -> Option<Tree> {
        if strands.is_empty() && swags.is_empty() {
            return None;
        }
        let mut pts: Vec<Vec3> = strands.iter().map(|x| x.0).collect();
        let mut lo = strands.iter().map(|x| x.0.y - x.1).fold(f32::MAX, f32::min);
        for &(a, b, sag) in &swags {
            pts.push(a);
            pts.push(b);
            // The low point, and the dangles hanging from the swag.
            lo = lo.min(a.y.min(b.y) - sag - 1.4);
        }
        let n = pts.len() as f32;
        let c = pts.iter().fold(Vec3::ZERO, |a, x| a + *x) / n;
        let leaf = leaf_at(c.x, c.z);
        let top = pts.iter().map(|x| x.y).fold(f32::MIN, f32::max);
        let radius = pts.iter().map(|x| Vec2::new(x.x - c.x, x.z - c.z).length()).fold(0.0, f32::max) + 1.0;
        Some(Tree {
            pos: Vec3::new(c.x, lo, c.z),
            kind: TreeKind::Vine,
            trunk_h: 0.0,
            trunk_r: 0.0,
            blobs: strands.into_iter().map(|(c, l)| Blob::ball(c, l)).collect(),
            canopy_lo: 0.0,
            top: top - lo + 0.3,
            radius,
            leaf,
            bark: leaf,
            seed: s,
            squash: 1.0,
            branches: swags,
            plant: pi as u16,
            flare: 0.0,
            autumn,
            flower: 0,
            sticks: 0.0,
        })
    };
    let mut add = Vec::new();
    if host("tree") {
        // Broadleaf crowns by 8 m cell, for the swags between neighbours.
        let crown = |t: &Tree| (t.kind == TreeKind::Broad || t.kind == TreeKind::Birch) && t.leaf != 0 && t.top >= 4.0 && t.trunk_r >= 0.12;
        let cell = |v: f32| (v / 8.0).floor() as i32;
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
        for (k, t) in trees.iter().enumerate() {
            if crown(t) {
                grid.entry((cell(t.pos.x), cell(t.pos.z))).or_default().push(k);
            }
        }
        // The leafy blob of `t` reaching furthest along `d`, and a point low on
        // its rim there: where a swag leaves the crown.
        let rim = |t: &Tree, d: Vec2| -> Option<Vec3> {
            t.blobs
                .iter()
                .filter(|b| b.r > 0.5)
                .max_by(|a, b| {
                    let fa = Vec2::new(a.c.x - t.pos.x, a.c.z - t.pos.z).dot(d) + a.r;
                    let fb = Vec2::new(b.c.x - t.pos.x, b.c.z - t.pos.z).dot(d) + b.r;
                    fa.total_cmp(&fb)
                })
                .map(|b| b.c + Vec3::new(d.x, 0.0, d.y) * b.r * 0.7 - Vec3::Y * b.r * t.squash * 0.45)
        };
        let mut paired = std::collections::HashSet::new();
        for (k, t) in trees.iter().enumerate() {
            if !crown(t) {
                continue;
            }
            if f.biome[f.idx(t.pos.x, t.pos.z)] as usize != bi {
                continue;
            }
            let h = hash32(seed ^ (k as u32).wrapping_mul(0x9e37_79b9));
            if unit(h) >= u.density {
                continue;
            }
            let mut rng = h;
            let mut rnd = || {
                rng = hash32(rng.wrapping_add(0x6d2b_79f5));
                unit(rng)
            };
            let leafy: Vec<(Vec3, f32)> = t.blobs.iter().filter(|b| b.r > 0.5).map(|b| (b.c, b.r)).collect();
            if leafy.is_empty() {
                continue;
            }
            // Strands from round the crown's edge and underside, longest at
            // the edge where nothing is below them.
            let n = 8 + (rnd() * 12.0) as usize;
            let mut strands = Vec::new();
            for _ in 0..n {
                let (c, r) = leafy[(rnd() * leafy.len() as f32) as usize % leafy.len()];
                let a = rnd() * TAU;
                let q = 0.6 + 0.45 * rnd().sqrt();
                let (x, z) = (c.x + a.cos() * r * q, c.z + a.sin() * r * q);
                let under = c.y - (1.0 - q * q).max(0.0).sqrt() * r * t.squash * 0.85;
                let ground = f.height(x, z);
                let room = under - ground - 0.5;
                if room < 1.0 {
                    continue;
                }
                let len = (p.height[0] + (p.height[1] - p.height[0]) * rnd()) * (0.5 + 0.5 * q);
                strands.push((Vec3::new(x, under + 0.4, z), len.min(room)));
            }
            // Swags: strands slung from this crown to one to three of its four
            // nearest neighbours whose crowns stand 1.5 to 8 m off, sagging in
            // a catenary.
            let mut swags = Vec::new();
            let mut near: Vec<(f32, usize)> = Vec::new();
            for gz in cell(t.pos.z) - 1..=cell(t.pos.z) + 1 {
                for gx in cell(t.pos.x) - 1..=cell(t.pos.x) + 1 {
                    for &j in grid.get(&(gx, gz)).map_or(&[][..], |v| &v[..]) {
                        let d = Vec2::new(trees[j].pos.x - t.pos.x, trees[j].pos.z - t.pos.z).length();
                        if j != k && d > 1.0 && d < 14.0 {
                            near.push((d, j));
                        }
                    }
                }
            }
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            let want = (1.0 + rnd() * 2.2) as usize;
            let mut hung = 0;
            for &(_, j) in near.iter().take(4) {
                if hung >= want {
                    break;
                }
                if !paired.insert((k.min(j), k.max(j))) {
                    continue;
                }
                let o = &trees[j];
                let d = Vec2::new(o.pos.x - t.pos.x, o.pos.z - t.pos.z).normalize();
                let (Some(a), Some(b)) = (rim(t, d), rim(o, -d)) else {
                    continue;
                };
                let span = Vec2::new(b.x - a.x, b.z - a.z).length();
                if !(1.5..=8.0).contains(&span) || (a.y - b.y).abs() > span * 0.8 {
                    continue;
                }
                // Sag: a fifth to a third of the span, kept clear of the ground
                // under the middle.
                let mid = (a + b) * 0.5;
                let room = a.y.min(b.y) - f.height(mid.x, mid.z) - 2.2;
                let sag = (span * (0.18 + 0.17 * rnd())).min(room);
                if sag < span * 0.08 {
                    continue;
                }
                swags.push((a, b, sag));
                hung += 1;
                // Often a second strand alongside, hung lower or higher.
                if rnd() < 0.5 {
                    let off = Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * 0.8;
                    let sag2 = (sag * (0.6 + 0.8 * rnd())).min(room);
                    swags.push((a + off, b - off, sag2.max(span * 0.08)));
                }
            }
            if let Some(v) = make(strands, swags, h) {
                add.push(v);
            }
        }
    }
    if host("cliff") {
        let step = 3.0;
        let cells = (f.size / step) as i32;
        for cz in 1..cells - 1 {
            for cx in 1..cells - 1 {
                let h0 = hash2(cx, cz, seed);
                if unit(h0) >= u.density {
                    continue;
                }
                let q = Vec2::new((cx as f32 + unit(hash32(h0 ^ 1))) * step, (cz as f32 + unit(hash32(h0 ^ 2))) * step);
                let top = f.height(q.x, q.y);
                if top < f.sea + 1.0 || f.occupied[f.idx(q.x, q.y)] {
                    continue;
                }
                // The steepest way down within 3 m.
                let mut best = (0.0f32, Vec2::ZERO);
                for k in 0..8 {
                    let a = k as f32 * TAU / 8.0;
                    let d = Vec2::new(a.cos(), a.sin());
                    let drop = top - f.height(q.x + d.x * 3.0, q.y + d.y * 3.0);
                    if drop > best.0 {
                        best = (drop, d);
                    }
                }
                let (drop, d) = best;
                if drop < 3.0 {
                    continue;
                }
                let foot = q + d * 3.0;
                let in_biome = |v: Vec2| f.biome[f.idx(v.x, v.y)] as usize == bi;
                if !in_biome(q) && !in_biome(foot) {
                    continue;
                }
                // The lip: walk out until the ground falls away.
                let mut lip = q;
                for j in 1..=12 {
                    let v = q + d * (j as f32 * 0.25);
                    if f.height(v.x, v.y) < top - 0.6 {
                        break;
                    }
                    lip = v;
                }
                let mut rng = h0;
                let mut rnd = || {
                    rng = hash32(rng.wrapping_add(0x6d2b_79f5));
                    unit(rng)
                };
                let width = p.crown_radius[0] + (p.crown_radius[1] - p.crown_radius[0]) * rnd();
                let side = Vec2::new(-d.y, d.x);
                let n = (width / 0.5).max(2.0) as i32;
                let mut strands = Vec::new();
                let towered = on_tower(w, q);
                for j in 0..n {
                    let off = (j as f32 / (n - 1).max(1) as f32 - 0.5) * width;
                    let at = lip + side * off + d * 0.2;
                    let lip_y = f.height(at.x - d.x * 0.2, at.y - d.y * 0.2);
                    // Down a tower's wall a curtain may run the whole way to the
                    // foot; on a plain cliff it stops above the ground.
                    let tall = if towered { 1.0 + 0.9 * rnd() } else { 1.0 };
                    let len = (p.height[0] + (p.height[1] - p.height[0]) * rnd()) * (0.6 + 0.4 * rnd()) * tall;
                    strands.extend(down_face(w, at, d, lip_y + 0.15, len));
                }
                if let Some(v) = make(strands, Vec::new(), h0) {
                    add.push(v);
                }
            }
        }
    }
    if host("wall") {
        add.extend(wall_vines(w, bi, u.density, p, seed).into_iter().filter_map(|(s, h)| make(s, Vec::new(), h)));
    }
    trees.extend(add);
}

/// The side of a host that faces away from the midday sun: north, give or
/// take, which is the side moss and lichen grow on.
fn shaded_dir(r: f32) -> Vec2 {
    let a = -std::f32::consts::FRAC_PI_2 + (r - 0.5) * 1.8;
    Vec2::new(a.cos(), a.sin())
}

/// A crust plant's colonies on its hosts in biome `bi`: shelf fungus standing
/// out from tree trunks, and skins of moss or lichen on trunks and on steep
/// faces of bare ground. Each patch is (centre, the way it faces, radius) in
/// the record's `branches`, and `blobs` stays empty. The biome entry's
/// `density` is the chance a host carries a colony: each tree, and each 4 m
/// cell of steep ground.
fn place_crusts(w: &World, trees: &mut Vec<super::gen::Tree>, bi: usize, ui: usize, pi: usize) {
    let u = &w.lib.biomes[bi].plants[ui];
    let p = &w.lib.plants[pi];
    let name = p.leaf.as_ref().unwrap_or(&p.bark);
    let Some(plain) = w.lib.material(name) else {
        return;
    };
    let slot = if p.leaf.is_some() { "leaf" } else { "bark" };
    let bracket = p.crust.as_deref() == Some("bracket");
    let shaded = p.shaded.unwrap_or(!bracket);
    let host = |h: &str| p.hosts.is_empty() || p.hosts.iter().any(|x| x == h);
    let seed = hash32(w.def.seed ^ name_key(&w.lib.biomes[bi].name) ^ use_key(w.def.seeds_v1(), ui, &u.plant, 0x27d4_eb2d) ^ 0x6372_7573);
    let f = &w.f;
    let make = |pats: Vec<(Vec3, Vec3, f32)>, s: u32| -> Option<Tree> {
        if pats.is_empty() {
            return None;
        }
        let big = pats.iter().map(|x| x.2).fold(0.0, f32::max);
        let n = pats.len() as f32;
        let c = pats.iter().fold(Vec3::ZERO, |a, x| a + x.0) / n;
        let mat = coat_at(&w.lib, pi, u.morph.as_ref(), c.x, c.z, seed).get(slot).unwrap_or(plain);
        let lo = pats.iter().map(|x| x.0.y).fold(f32::MAX, f32::min) - big;
        let hi = pats.iter().map(|x| x.0.y).fold(f32::MIN, f32::max) + big;
        let radius = pats.iter().map(|x| Vec2::new(x.0.x - c.x, x.0.z - c.z).length()).fold(0.0, f32::max) + big + 0.3;
        Some(Tree {
            pos: Vec3::new(c.x, lo, c.z),
            kind: if bracket { TreeKind::Bracket } else { TreeKind::Crust },
            trunk_h: 0.0,
            trunk_r: 0.0,
            blobs: Vec::new(),
            canopy_lo: 0.0,
            top: hi - lo + 0.3,
            radius,
            leaf: mat,
            bark: mat,
            seed: s,
            squash: 1.0,
            branches: pats,
            plant: pi as u16,
            flare: 0.0,
            autumn: 0,
            flower: 0,
            sticks: 0.0,
        })
    };
    let mut add = Vec::new();
    if host("tree") {
        for (k, t) in trees.iter().enumerate() {
            // Trunks only, and thick enough to carry anything.
            if matches!(t.kind, TreeKind::Vine | TreeKind::Crust | TreeKind::Bracket) || t.trunk_r < 0.1 || t.trunk_h < 1.5 {
                continue;
            }
            if f.biome[f.idx(t.pos.x, t.pos.z)] as usize != bi {
                continue;
            }
            let h = hash32(seed ^ (k as u32).wrapping_mul(0x9e37_79b9));
            if unit(h) >= u.density {
                continue;
            }
            let mut rng = h;
            let mut rnd = || {
                rng = hash32(rng.wrapping_add(0x6d2b_79f5));
                unit(rng)
            };
            // The drawn trunk is never thinner than 0.19 m across the corner
            // and flares at the foot: sit the colony just clear of it.
            let skin = t.trunk_r.max(0.19) + 0.12;
            let lo = p.height[0].max(0.6);
            let hi = p.height[1].min(t.trunk_h - 0.3);
            if hi <= lo {
                continue;
            }
            let count = if bracket { 2 + (rnd() * 3.0) as usize } else { 2 + (rnd() * 3.0) as usize };
            // One side of the trunk per colony: brackets shelve up a single
            // face, and moss keeps to the one that faces away from the sun.
            let base = if shaded {
                shaded_dir(rnd())
            } else {
                let a = rnd() * TAU;
                Vec2::new(a.cos(), a.sin())
            };
            let spread = if bracket { 0.7 } else { 1.0 };
            // A patch wider than the trunk would sleeve right round it.
            let cap = skin * 1.1;
            let mut y = lo + (hi - lo) * 0.35 * rnd();
            let mut pats = Vec::new();
            for _ in 0..count {
                let a = base.y.atan2(base.x) + (rnd() - 0.5) * spread;
                let d = Vec2::new(a.cos(), a.sin());
                y += (hi - lo) * (0.12 + 0.3 * rnd());
                if y > hi {
                    break;
                }
                let r = (p.crown_radius[0] + (p.crown_radius[1] - p.crown_radius[0]) * rnd()).min(cap);
                let c = Vec3::new(t.pos.x + d.x * skin, t.pos.y + y, t.pos.z + d.y * skin);
                pats.push((c, Vec3::new(d.x, 0.0, d.y), r));
            }
            if let Some(c) = make(pats, h) {
                add.push(c);
            }
        }
    }
    if host("rock") {
        let step = 4.0;
        let cells = (f.size / step) as i32;
        for cz in 1..cells - 1 {
            for cx in 1..cells - 1 {
                let h0 = hash2(cx, cz, seed);
                if unit(h0) >= u.density {
                    continue;
                }
                let q = Vec2::new((cx as f32 + unit(hash32(h0 ^ 1))) * step, (cz as f32 + unit(hash32(h0 ^ 2))) * step);
                let i = f.idx(q.x, q.y);
                // A water crust (plate coral) grows on faces under the
                // water, of the sea or of a lake or a cenote, whatever the
                // top of the face is (each patch checks its own depth
                // below); any other keeps to dry ground.
                let level = f.inland.get(&(i as u32)).map_or(f.sea, |w| w.level);
                let dry = f.h[i] >= level + 0.5;
                let wet_crust = p.water_depth.is_some();
                if f.biome[i] as usize != bi || f.occupied[i] || (!wet_crust && !dry) || f.slope[i] < 0.7 {
                    continue;
                }
                let top = f.height(q.x, q.y);
                // The steepest way down within 3 m is the way the face looks.
                let mut best = (0.0f32, Vec2::ZERO);
                for k in 0..8 {
                    let a = k as f32 * TAU / 8.0;
                    let d = Vec2::new(a.cos(), a.sin());
                    let drop = top - f.height(q.x + d.x * 3.0, q.y + d.y * 3.0);
                    if drop > best.0 {
                        best = (drop, d);
                    }
                }
                let (drop, d) = best;
                if drop < 1.5 {
                    continue;
                }
                // Moss keeps to the faces that look away from the sun.
                if shaded && d.y > -0.25 {
                    continue;
                }
                let mut rng = h0;
                let mut rnd = || {
                    rng = hash32(rng.wrapping_add(0x6d2b_79f5));
                    unit(rng)
                };
                let n = 2 + (rnd() * 4.0) as usize;
                let side = Vec2::new(-d.y, d.x);
                let mut pats = Vec::new();
                for _ in 0..n {
                    let at = q + side * ((rnd() - 0.5) * 3.0);
                    let down = 0.4 + rnd() * (drop - 0.6).max(0.4);
                    let lip = f.height(at.x, at.y);
                    if let Some(&(c, _)) = down_face(w, at, d, lip - down, 0.3).first() {
                        if let Some([lo, hi]) = p.water_depth {
                            let under = level - c.y;
                            if under < lo || under > hi {
                                continue;
                            }
                        }
                        let r = p.crown_radius[0] + (p.crown_radius[1] - p.crown_radius[0]) * rnd();
                        pats.push((c, Vec3::new(d.x, 0.0, d.y), r));
                    }
                }
                if let Some(c) = make(pats, h0) {
                    add.push(c);
                }
            }
        }
    }
    trees.extend(add);
}

/// How readily vines take to a structure's walls, 0 to 1 (times the biome
/// entry's density): the structure's own `vines` if it sets one; otherwise
/// a ruin always takes them, rock, glass and anything else that doesn't
/// decay never does, and a building takes a quarter as much kept up, rising
/// to all of it as it falls down (`wear` 0 to 1).
pub fn wall_vine_share(def: &crate::library::StructureDef, wear: f32) -> f32 {
    if let Some(v) = def.vines {
        return v.clamp(0.0, 1.0);
    }
    if def.tags.iter().any(|t| t == "ruin") {
        return 1.0;
    }
    if !def.decays {
        return 0.0;
    }
    0.25 + 0.75 * wear.clamp(0.0, 1.0)
}

/// Vine curtains on the outer walls of the structures in biome `bi`. A
/// structure carries vines with chance `share * min(1, 0.5 + 4 * density)`;
/// then each patch of its walls (`crown_radius` wide) is covered with chance
/// `0.25 + 0.6 * share`. A strand either hangs from the top of what still stands or climbs
/// from the ground, `height` long, and comes down the wall in metre lengths,
/// each just outside the wall's face at its height. One curtain per patch:
/// (strands, seed).
fn wall_vines(w: &World, bi: usize, density: f32, p: &PlantDef, seed: u32) -> Vec<(Vec<(Vec3, f32)>, u32)> {
    use super::gen::{still_standing, structure_wear, VPM};
    use super::shape::{unrotate, CARVE};
    let f = &w.f;
    let mut out = Vec::new();
    for (k, pl) in w.placed.iter().enumerate() {
        let def = &w.lib.structures[pl.def];
        if def.base.as_deref() == Some("sea") {
            continue;
        }
        let wear = structure_wear(w, pl);
        let share = wall_vine_share(def, wear);
        if share <= 0.0 {
            continue;
        }
        let (x0, z0, x1, z1) = pl.footprint(&w.shapes);
        let (cx, cz) = ((x0 + x1) as f32 * 0.5 / VPM, (z0 + z1) as f32 * 0.5 / VPM);
        if cx < 0.0 || cz < 0.0 || cx >= f.size || cz >= f.size || f.biome[f.idx(cx, cz)] as usize != bi {
            continue;
        }
        let h0 = hash32(seed ^ 0x7761_6c6c ^ (k as u32).wrapping_mul(0x9e37_79b9));
        if unit(h0) >= (0.5 + density * 4.0).min(1.0) * share {
            continue;
        }
        let cover = 0.25 + 0.6 * share;
        let s = &w.shapes[pl.shape];
        // A standing voxel at world column (wx, wz), structure height ly.
        let standing = |wx: i32, wz: i32, ly: i32| -> bool {
            if wx < x0 || wx >= x1 || wz < z0 || wz >= z1 || ly < 0 || ly >= s.top {
                return false;
            }
            let (lx, lz) = unrotate(wx - x0, wz - z0, s.size, pl.rot);
            let (m, _) = s.get(lx, ly, lz);
            if m == 0 || m == CARVE {
                return false;
            }
            wear <= 0.0 || still_standing(w, k, lx, ly, lz)
        };
        // The four sides: a start, the way along, the way in, the length.
        let (wd, dp) = (x1 - x0, z1 - z0);
        let sides = [
            ((x0, z0), (1, 0), (0, 1), wd, dp),
            ((x0, z1 - 1), (1, 0), (0, -1), wd, dp),
            ((x0, z0), (0, 1), (1, 0), dp, wd),
            ((x1 - 1, z0), (0, 1), (-1, 0), dp, wd),
        ];
        for (si, &((sx, sz), (ax, az), (ix, iz), len, depth)) in sides.iter().enumerate() {
            let hs = hash32(h0 ^ (si as u32 + 1).wrapping_mul(0x85eb_ca6b));
            let patch = ((p.crown_radius[0] + (p.crown_radius[1] - p.crown_radius[0]) * unit(hs)) * VPM).max(4.0) as i32;
            let reach = (depth / 2).min(24);
            // The first standing column in from the edge with any voxel in
            // heights lo..=hi: the wall's face there.
            let face = |t: i32, lo: i32, hi: i32| -> Option<i32> { (0..reach).find(|&j| (lo.max(0)..=hi).any(|ly| standing(sx + ax * t + ix * j, sz + az * t + iz * j, ly))) };
            let mut t = 0;
            while t < len {
                let pid = t / patch;
                let hp = hash32(hs ^ (pid as u32).wrapping_mul(0x9e37_79b9));
                let end = ((pid + 1) * patch).min(len);
                if unit(hp) >= cover {
                    t = end;
                    continue;
                }
                let mut rng = hp;
                let mut rnd = || {
                    rng = hash32(rng.wrapping_add(0x6d2b_79f5));
                    unit(rng)
                };
                let mut strands = Vec::new();
                let mut tt = t + (rnd() * 2.0) as i32;
                while tt < end {
                    let this = tt;
                    tt += 2;
                    if rnd() < 0.2 {
                        continue;
                    }
                    // The wall's top here: the highest standing voxel in the
                    // first column in from the edge that is over 1.5 m high.
                    let mut top = None;
                    for j in 0..reach {
                        let (wx, wz) = (sx + ax * this + ix * j, sz + az * this + iz * j);
                        if let Some(ly) = (0..s.top).rev().find(|&ly| standing(wx, wz, ly)) {
                            if ly >= 6 {
                                top = Some(ly);
                                break;
                            }
                        }
                    }
                    let Some(top) = top else { continue };
                    let length = p.height[0] + (p.height[1] - p.height[0]) * rnd();
                    // Hanging from the top, or (two in five) climbing from the
                    // ground: start and end heights in voxels above the base.
                    let (from, to) = if rnd() < 0.4 {
                        (((length * (0.6 + 0.4 * rnd())) * VPM) as i32, 0)
                    } else {
                        (top + 1, top + 1 - (length * VPM) as i32)
                    };
                    let from = from.min(top + 1);
                    let to = to.max(0);
                    if from - to < 4 {
                        continue;
                    }
                    let mut y = from;
                    let mut col = None;
                    while y > to {
                        let piece = (y - to).min(4);
                        // Outside the face at this metre; an opening keeps the
                        // last place.
                        if let Some(j) = face(this, y - 4, y) {
                            col = Some(j - 1);
                        }
                        let Some(j) = col else { break };
                        let (wx, wz) = (sx + ax * this + ix * j, sz + az * this + iz * j);
                        let (x, z) = ((wx as f32 + 0.5) / VPM, (wz as f32 + 0.5) / VPM);
                        let ym = (pl.origin[1] + y) as f32 / VPM + 0.05;
                        if f.height(x, z) < f.sea || ym < f.height(x, z) + 0.2 {
                            break;
                        }
                        strands.push((Vec3::new(x, ym, z), piece as f32 / VPM));
                        y -= 4;
                    }
                }
                if !strands.is_empty() {
                    out.push((strands, hp));
                }
                t = end;
            }
        }
    }
    out
}

fn pl_spacing(p: &Placement) -> Option<f32> {
    if p.verb == PlaceVerb::Lone {
        Some(p.params.get("spacing").copied().unwrap_or(40.0))
    } else {
        p.params.get("spacing").copied()
    }
}

/// Natural features the chronicle placed (fallen blocks round a tower's
/// foot, scree, logs) sit on the ground as it stood when their event ran. An
/// event that came later and cut the ground away under them — a river
/// carving its gorge across a tower's rubble field — left them hanging in
/// the air over the water. Drop each one back onto the ground as it finally
/// stands, before anything is drawn from it.
///
/// Only free-standing natural shapes move. Anything that levels its own pad,
/// decays, or is meant to hang (`base` "sea" or "rim": landings, piers, the
/// Order's rope bridges) stays where its event put it.
pub fn reseat_features(w: &mut World) {
    let mut moved = 0;
    for k in 0..w.placed.len() {
        let def = &w.lib.structures[w.placed[k].def];
        if def.level || def.decays || def.base.is_some() {
            continue;
        }
        let (fw, fd) = super::shape::rotated_size(w.shapes[w.placed[k].shape].size, w.placed[k].rot);
        let [vx, gy, vz] = w.placed[k].origin;
        let mut lo = f32::MAX;
        for dz in 0..=fd / 4 {
            for dx in 0..=fw / 4 {
                lo = lo.min(w.f.height(vx as f32 * 0.25 + dx as f32, vz as f32 * 0.25 + dz as f32));
            }
        }
        // The same seat `place_features` gives a boulder: a quarter of a
        // metre into the ground under its lowest corner.
        let want = ((lo - 0.25) * 4.0).floor() as i32;
        if want < gy {
            w.placed[k].origin[1] = want;
            moved += 1;
        }
    }
    if moved > 0 {
        w.log.push(format!("gravity: {moved} natural features dropped onto ground that later events cut away"));
    }
}

/// Natural features (boulders, logs, outcrops) by the biomes' placement rules.
pub fn place_features(w: &mut World) {
    let size = w.f.size;
    let sea = w.f.sea;
    let mut shape_of: std::collections::HashMap<(usize, u8), (usize, [i32; 3])> = std::collections::HashMap::new();
    let mut adds = Vec::new();
    for (bi, b) in w.lib.biomes.iter().enumerate() {
        for (ui, u) in b.features.iter().enumerate() {
            let Some(si) = w.lib.structure(&u.structure) else {
                continue;
            };
            if u.density <= 0.0 {
                continue;
            }
            let g = (1.0 / u.density).sqrt().max(2.0);
            let seed = hash32(w.def.seed ^ 0xfeed ^ name_key(&w.lib.biomes[bi].name) ^ use_key(w.def.seeds_v1(), ui, &u.structure, 0x85eb_ca6b));
            let cells = (size / g).ceil() as i32;
            let spacing = pl_spacing(&u.placement);
            // `seat`: the most the ground may rise across the footprint, m.
            let seat = u.placement.params.get("seat").copied();
            let mut lone: Vec<Vec2> = Vec::new();
            // A `base: "sea"` feature (cypress knees, a floating mat) stands
            // in the shallows instead: from 0.2 to 2.5 m of water.
            let in_sea = w.lib.structures[si].base.as_deref() == Some("sea");
            for cz in 0..cells {
                for cx in 0..cells {
                    let h0 = hash2(cx, cz, seed);
                    let p = Vec2::new((cx as f32 + 0.15 + 0.7 * unit(h0)) * g, (cz as f32 + 0.15 + 0.7 * unit(hash32(h0 ^ 1))) * g);
                    if p.x < 6.0 || p.y < 6.0 || p.x > size - 6.0 || p.y > size - 6.0 {
                        continue;
                    }
                    let i = w.f.idx(p.x, p.y);
                    let wet = sea - w.f.h[i];
                    let fits = if in_sea { (0.2..2.5).contains(&wet) } else { wet <= -0.3 };
                    if w.f.biome[i] as usize != bi || w.f.occupied[i] || w.f.water[i] != 0 || !fits {
                        continue;
                    }
                    if !w.ribbons.is_empty() && w.ribbons.covers(p, 1.5) {
                        continue;
                    }
                    if unit(hash32(h0 ^ 2)) > weight(w, &u.placement, p, bi as u8, seed) {
                        continue;
                    }
                    if let Some(sp) = spacing {
                        if lone.iter().any(|q| q.distance(p) < sp) {
                            continue;
                        }
                        lone.push(p);
                    }
                    let along = u.placement.params.get("along").copied();
                    adds.push((si, p, unit(hash32(h0 ^ 3)) * 360.0, hash32(h0 ^ 4), seat, along));
                }
            }
        }
    }
    for (si, p, bearing, seed, seat, along) in adds {
        let sd = &w.lib.structures[si];
        // Logs and bone shards lie at any bearing; anything else turns by
        // quarters. A log-only feature is compiled once per 15-degree turn
        // within the quarter.
        let loose = !sd.level && !sd.decays && sd.base.is_none() && sd.ops.iter().all(|o| matches!(o, Op::Log { .. } | Op::Disc { .. }));
        let mut rot = (((bearing + 45.0) / 90.0) as u8) % 4;
        let mut turn = 0u8;
        if loose {
            let jitter = |spread: f32| (unit(hash32(seed ^ 0x51)) - 0.5) * 2.0 * spread + if seed & 0x100 != 0 { 180.0 } else { 0.0 };
            let contour = |dx: f32, dz: f32| dz.atan2(dx).to_degrees() + 90.0;
            let mut theta = bearing;
            if let Some(spread) = along {
                // Along the shore, as the tide leaves a log: across the line
                // to the sea, give or take `along` degrees.
                let i = |x: f32, z: f32| w.f.coast_dist[w.f.idx(x, z)];
                theta = contour(i(p.x + 4.0, p.y) - i(p.x - 4.0, p.y), i(p.x, p.y + 4.0) - i(p.x, p.y - 4.0)) + jitter(spread);
            } else if seat.is_some() {
                // Across the slope, not up it, so both ends stand at about
                // the same height.
                let dx = w.f.height(p.x + 3.0, p.y) - w.f.height(p.x - 3.0, p.y);
                let dz = w.f.height(p.x, p.y + 3.0) - w.f.height(p.x, p.y - 3.0);
                theta = contour(dx, dz) + jitter(12.0);
            }
            let steps = (theta.rem_euclid(360.0) / 15.0).round() as i32 % 24;
            rot = (steps / 6) as u8;
            turn = (steps % 6) as u8;
        } else if seat.is_some() {
            // Long axis (local X) along the contour, so both ends stand at
            // about the same height: across the slope, not up it.
            let dx = w.f.height(p.x + 3.0, p.y) - w.f.height(p.x - 3.0, p.y);
            let dz = w.f.height(p.x, p.y + 3.0) - w.f.height(p.x, p.y - 3.0);
            let along_z = dx.abs() > dz.abs();
            rot = (rot & 2) | along_z as u8;
        }
        let (sh, anchor) = *shape_of.entry((si, turn)).or_insert_with(|| {
            let sd = &w.lib.structures[si];
            let turned = if turn > 0 {
                super::shape::turn_logs(&sd.ops, sd.size, sd.anchor, turn as f32 * 15.0)
            } else {
                None
            };
            let (shape, anchor) = match turned {
                Some((ops, size, anchor)) => {
                    let resolve = |n: &str| w.lib.material(sd.slots.get(n).map(|s| s.as_str()).unwrap_or(n));
                    (super::shape::compile(&ops, size, &resolve), anchor)
                }
                None => (super::chronicle::compile_structure(&w.lib, si, &[]), sd.anchor),
            };
            w.shapes.push(shape);
            (w.shapes.len() - 1, anchor)
        });
        let size3 = w.shapes[sh].size;
        let (ax, az) = super::shape::rotate(anchor[0], anchor[2], size3, rot);
        let (fw, fd) = super::shape::rotated_size(size3, rot);
        let vx = (p.x * 4.0).floor() as i32 - ax;
        let vz = (p.y * 4.0).floor() as i32 - az;
        let mut lo = f32::MAX;
        for dz in 0..=fd / 4 {
            for dx in 0..=fw / 4 {
                lo = lo.min(w.f.height(vx as f32 * 0.25 + dx as f32, vz as f32 * 0.25 + dz as f32));
            }
        }
        // Nothing in the water or on top of a building.
        let mut wet = false;
        for dz in 0..=fd / 4 {
            for dx in 0..=fw / 4 {
                let i = w.f.idx(vx as f32 * 0.25 + dx as f32, vz as f32 * 0.25 + dz as f32);
                wet |= w.f.water[i] != 0 || w.f.occupied[i];
            }
        }
        if wet {
            continue;
        }
        if let Some(relief) = seat {
            // Only where the ground under it is near level; otherwise one
            // side stands buried in the slope.
            let mut hi = f32::MIN;
            for dz in 0..=fd / 4 {
                for dx in 0..=fw / 4 {
                    hi = hi.max(w.f.height(vx as f32 * 0.25 + dx as f32, vz as f32 * 0.25 + dz as f32));
                }
            }
            if hi - lo > relief {
                continue;
            }
        }
        let gy = ((lo - 0.25) * 4.0).floor() as i32;
        for dz in 0..=fd / 4 {
            for dx in 0..=fw / 4 {
                let i = w.f.idx(vx as f32 * 0.25 + dx as f32, vz as f32 * 0.25 + dz as f32);
                w.f.occupied[i] = true;
            }
        }
        w.placed.push(super::fields::Placed {
            shape: sh,
            def: si,
            origin: [vx, gy, vz],
            rot,
            age: 0.0,
            cause: 0,
            seed,
        });
    }
}

/// Small finds on the ground, by the biomes' placement rules.
pub fn place_collectibles(w: &mut World) {
    let size = w.f.size;
    let sea = w.f.sea;
    let mut adds = Vec::new();
    for (bi, b) in w.lib.biomes.iter().enumerate() {
        for (ui, u) in b.collectibles.iter().enumerate() {
            if u.density <= 0.0 || w.lib.collectible(&u.item).is_none() {
                continue;
            }
            let g = (1.0 / u.density).sqrt().max(1.5);
            let seed = hash32(w.def.seed ^ 0xc011 ^ name_key(&w.lib.biomes[bi].name) ^ use_key(w.def.seeds_v1(), ui, &u.item, 0x27d4_eb2d));
            let cells = (size / g).ceil() as i32;
            for cz in 0..cells {
                for cx in 0..cells {
                    let h0 = hash2(cx, cz, seed);
                    let p = Vec2::new((cx as f32 + 0.1 + 0.8 * unit(h0)) * g, (cz as f32 + 0.1 + 0.8 * unit(hash32(h0 ^ 1))) * g);
                    if p.x < 2.0 || p.y < 2.0 || p.x > size - 2.0 || p.y > size - 2.0 {
                        continue;
                    }
                    let i = w.f.idx(p.x, p.y);
                    if w.f.biome[i] as usize != bi || w.f.water[i] == 1 || w.f.h[i] < sea + 0.2 {
                        continue;
                    }
                    if unit(hash32(h0 ^ 2)) > weight(w, &u.placement, p, bi as u8, seed) {
                        continue;
                    }
                    adds.push(super::fields::Item {
                        kind: u.item.clone(),
                        pos: Vec3::new(p.x, w.f.height(p.x, p.y), p.y),
                        size: 1.0,
                        cause: 0,
                        surface: true,
                    });
                }
            }
        }
    }
    w.items.extend(adds);
}

#[cfg(test)]
mod key_tests {
    use super::{name_key, use_key};

    /// A biome's plants are keyed by name, so adding one to the middle of a
    /// biome's list leaves the others where they stood. Under the first
    /// scheme they are keyed by their place in the list, which is what the
    /// worlds already made were laid out with.
    #[test]
    fn a_plants_key_follows_its_name_not_its_place() {
        let m = 0x9e37_79b9;
        assert_eq!(use_key(false, 0, "fir", m), use_key(false, 5, "fir", m), "the place in the list moved it");
        assert_ne!(use_key(false, 0, "fir", m), use_key(false, 0, "birch", m), "two plants shared a key");
        assert_ne!(use_key(true, 0, "fir", m), use_key(true, 5, "fir", m), "the first scheme counts places");
        assert_eq!(use_key(true, 3, "fir", m), use_key(true, 3, "birch", m), "and nothing else");
    }

    /// The multiplier keeps the uses of a key apart, so a plant and the
    /// structure next to it in the same biome do not land on one seed.
    #[test]
    fn the_multiplier_separates_the_uses() {
        assert_ne!(use_key(false, 0, "fir", 0x9e37_79b9), use_key(false, 0, "fir", 0x85eb_ca6b));
    }

    #[test]
    fn names_hash_apart() {
        assert_ne!(name_key("meadow"), name_key("moor"));
        assert_eq!(name_key("meadow"), name_key("meadow"));
    }
}

/// Put a grown tree into the clothes the year gives it (world/season.rs).
///
/// `grow` makes one plant, in summer leaf and in flower; this is what makes it
/// autumn, bare or out of bloom, and it runs on every tree on its way into a
/// column (`gen::trees_near`), so a column built now and the same column built
/// an hour later differ by exactly this.
///
/// Each tree asks the turn which season it is wearing, and the turn answers
/// from the tree's own seed, so a wood changes tree by tree over the first
/// half of a season rather than all at once.
pub fn dress(t: &mut Tree, lib: &Library, turn: Turn) {
    use crate::world::material::AIR;
    let s = turn.wears(t.seed);
    // Blossom: off unless this is one of the plant's flowering seasons. A
    // plant that names none flowers all year, which is most of them.
    if t.flower != AIR {
        let seasons = (t.plant as usize) < lib.plants.len() && !lib.plants[t.plant as usize].flower_seasons.is_empty();
        if seasons && !lib.plants[t.plant as usize].flower_seasons.iter().any(|x| x == s.name()) {
            let f = t.flower;
            t.blobs.retain(|b| b.mat != f);
        }
    }
    // Only a plant with an autumn colour has leaves to turn or lose.
    if t.autumn == AIR {
        return;
    }
    match s.leaf() {
        Leaf::Green | Leaf::Blossom => {}
        Leaf::Autumn => {
            let was = t.leaf;
            for b in t.blobs.iter_mut().filter(|b| b.mat == was) {
                b.mat = t.autumn;
            }
            t.leaf = t.autumn;
        }
        Leaf::Bare => {
            // Only a plant with wood to show stands bare. A vine or a herb
            // is all leaf, so stripping it would leave nothing hanging on the
            // wall at all; it wears its autumn colour through the winter
            // instead.
            if t.trunk_r <= 0.0 && t.branches.is_empty() {
                let was = t.leaf;
                for b in t.blobs.iter_mut().filter(|b| b.mat == was) {
                    b.mat = t.autumn;
                }
                t.leaf = t.autumn;
                return;
            }
            // The leaf blobs go and the limbs stay. A blob with a material of
            // its own that is not the leaf (a flower head) stays with them,
            // since nothing that is not a leaf falls.
            let was = t.leaf;
            t.blobs.retain(|b| b.mat != AIR && b.mat != was);
            t.leaf = AIR;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A branching tree fills the box its library entry asks for: the crown
    /// reaches near the plant's height and about its crown radius out, and the
    /// trunk is clear below it.
    #[test]
    fn branching_trees_fill_their_box() {
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        for name in ["great_oak", "beech", "bigleaf_maple", "elder", "live_oak", "cottonwood"] {
            let p = &lib.plants[lib.plant(name).unwrap_or_else(|| panic!("no {name}"))];
            let (mut tops, mut rads) = (0.0f32, 0.0f32);
            for s in 0..24u32 {
                let t = grow(&lib, p, Vec3::ZERO, hash32(s.wrapping_mul(2654435761)), Worn::default());
                tops = tops.max(t.top);
                rads = rads.max(t.radius);
                println!(
                    "{name} seed {s}: trunk_h {:.1} top {:.1} radius {:.1} blobs {} limbs {}",
                    t.trunk_h,
                    t.top,
                    t.radius,
                    t.blobs.len(),
                    t.branches.len()
                );
            }
            assert!(tops >= p.height[1] * 0.8, "{name}: tallest crown {tops:.1} m under {:.1}", p.height[1] * 0.8);
            assert!(rads >= p.crown_radius[1] * 0.9, "{name}: widest crown {rads:.1} m under {:.1}", p.crown_radius[1] * 0.9);
            // And is not a pancake: a crown no more than about half as wide
            // again as its radius asks, however the limbs happen to fall.
            assert!(rads <= p.crown_radius[1] * 2.1, "{name}: widest crown {rads:.1} m over {:.1}", p.crown_radius[1] * 2.1);
        }
    }

    /// The year dresses a tree: an oak wears its autumn colour and then
    /// stands bare, keeping its trunk and limbs, while a fir never changes.
    /// And a wood turns tree by tree rather than all at once.
    #[test]
    fn the_year_dresses_a_tree() {
        use crate::world::season::{Season, Turn, STEPS};
        let _guard = crate::world::matlib::table_lock();
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        let grown = |name: &str, seed: u32| {
            let p = &lib.plants[lib.plant(name).unwrap_or_else(|| panic!("no {name}"))];
            grow(&lib, p, Vec3::ZERO, seed, Worn::default())
        };
        let oak = grown("oak", 7);
        assert!(oak.autumn != 0, "an oak has an autumn colour");
        let mut summer = oak.clone();
        dress(&mut summer, &lib, Turn::settled(Season::Summer, 0));
        assert_eq!(summer.leaf, oak.leaf, "summer leaves it alone");
        assert_eq!(summer.blobs.len(), oak.blobs.len());
        let mut autumn = oak.clone();
        dress(&mut autumn, &lib, Turn::settled(Season::Autumn, 0));
        assert_eq!(autumn.leaf, oak.autumn, "it wears its autumn colour");
        assert_eq!(autumn.blobs.len(), oak.blobs.len(), "and keeps every leaf");
        let mut winter = oak.clone();
        dress(&mut winter, &lib, Turn::settled(Season::Winter, 0));
        assert_eq!(winter.leaf, crate::world::material::AIR, "the leaves are gone");
        assert!(winter.blobs.iter().all(|b| b.mat != 0 && b.mat != oak.leaf), "no leaf clump left");
        assert_eq!(winter.branches.len(), oak.branches.len(), "the limbs stay");
        assert_eq!(winter.trunk_h, oak.trunk_h);
        // A conifer has no autumn colour, so nothing about it moves.
        let fir = grown("fir", 11);
        assert_eq!(fir.autumn, 0);
        for s in [Season::Autumn, Season::Winter] {
            let mut t = fir.clone();
            dress(&mut t, &lib, Turn::settled(s, 0));
            assert_eq!(t.leaf, fir.leaf, "a fir keeps its needles in {}", s.name());
            assert_eq!(t.blobs.len(), fir.blobs.len());
        }
        // Halfway through a turn, some of the wood has changed and some has not.
        let half = Turn {
            from: Season::Summer,
            to: Season::Autumn,
            step: STEPS / 2,
            snow: 0,
        };
        let mut turned = 0;
        for seed in 0..60u32 {
            let mut t = grown("oak", seed);
            let was = t.leaf;
            dress(&mut t, &lib, half);
            if t.leaf != was {
                turned += 1;
            }
        }
        assert!(turned > 10 && turned < 50, "{turned} of 60 oaks had turned");
    }

    /// A conifer's needle mass is one unbroken column from the foot of the
    /// crown to the tip, and it covers the wood all the way up.
    ///
    /// Both halves are bugs this cost a round each to find. A clump is drawn
    /// solid only to about two thirds of its radius, so pieces sized by eye
    /// kept falling under what the 2x2x2 leaf cells can mesh: once the top
    /// whorls thinned away and left the apex floating over a gap, and once the
    /// needles merely reached the bark instead of covering it and the trunk
    /// showed through a crown that looked solid. The clear leaf cell on top of
    /// this is `CONIFER_CLOTHE`, which `gen.rs` rasterises directly round the
    /// trunk because only it knows the voxel size.
    #[test]
    fn conifer_crowns_are_one_column_over_their_wood() {
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        // How far out from the axis the needles are solid at height `y`.
        let solid = |t: &crate::world::gen::Tree, y: f32| -> f32 {
            t.blobs
                .iter()
                .map(|b| {
                    let mut r = 0.0f32;
                    while r < 6.0 && b.dist(Vec3::new(r, y, 0.0), t.squash) < 0.68 {
                        r += 0.05;
                    }
                    r
                })
                .fold(0.0f32, f32::max)
        };
        for name in ["fir", "giant_fir", "pine", "sapling_fir", "karst_pine"] {
            let Some(pi) = lib.plant(name) else { continue };
            let p = &lib.plants[pi];
            for s in 0..16u32 {
                let t = grow(&lib, p, Vec3::ZERO, hash32(s.wrapping_mul(2654435761)), Worn::default());
                // Not every plant with a conifer's name has a conifer's form:
                // Jadewater's karst pine is a `branching` tree.
                if t.kind != TreeKind::Pine {
                    break;
                }
                // The top of the solid mass. Above it the crown is the ragged
                // band and the leaf cards, which is the tip and is meant to be
                // thin, so the column is checked up to here and no further.
                let mut solid_top = t.canopy_lo;
                let mut y = t.canopy_lo;
                while y <= t.top {
                    if solid(&t, y) > 0.0 {
                        solid_top = y;
                    }
                    y += 0.25;
                }
                // The thin tip is allowed, but only the tip: on a sapling the
                // ragged band is most of the tree, so the allowance is a flat
                // 0.9 m or a third of the crown, whichever is larger.
                let bare = (t.top - t.canopy_lo) * 0.35;
                assert!(
                    t.top - solid_top <= bare.max(0.9),
                    "{name} seed {s}: needles solid only to {solid_top:.1} m of a crown from {:.1} to {:.1}",
                    t.canopy_lo,
                    t.top
                );
                y = t.canopy_lo;
                while y <= solid_top {
                    let r = solid(&t, y);
                    // Unbroken: no height inside the crown with nothing on the
                    // axis and more needles above it — that is the apex left
                    // floating clear of the tree.
                    assert!(
                        r > 0.0,
                        "{name} seed {s}: the crown has a gap at {y:.1} m, under needles reaching {solid_top:.1} m \
                         (canopy_lo {:.1}, trunk_h {:.1}, top {:.1})",
                        t.canopy_lo,
                        t.trunk_h,
                        t.top
                    );
                    // And over the wood: where the trunk runs, the needles are
                    // solid past the bark, so gen.rs's cell of clothing has
                    // something to stand on rather than covering for a crown
                    // that is not there.
                    if y <= t.trunk_h {
                        let bark = (t.trunk_r * (1.0 - 0.3 * (y / t.trunk_h).max(0.0))).max(0.19);
                        assert!(r >= bark, "{name} seed {s}: needles solid to only {r:.2} m at {y:.1} m, inside the bark at {bark:.2} m",);
                    }
                    y += 0.25;
                }
            }
        }
    }
}

#[cfg(test)]
mod coat_tests {
    use super::*;
    use crate::library::{data_root, Library, MorphPick};

    /// The library with one world's material ids, the way `worldload` builds
    /// it but without installing the table: a coat's material is generated,
    /// so it only has an id where a world reaches the plant it belongs to.
    fn lib(world: &str) -> Library {
        let root = data_root();
        let (mut lib, errors) = Library::load(&root);
        assert!(errors.is_empty(), "{errors:?}");
        let mut r = crate::worldload::reach(&root.join("library"), &crate::worldload::world_dir(world), &lib);
        lib.reach_coats(&mut r);
        assert!(lib.assign_ids(Some(r)).is_empty(), "every reached material got an id");
        lib
    }

    /// Every coat a plant declares gets a material, named for the plant so
    /// two plants sharing a base material do not share its coats.
    #[test]
    fn a_coat_gets_its_own_material() {
        let barrow = lib("windbarrow");
        let jade = lib("jadewater");
        let lib = barrow;
        // feather_tussock and sawgrass are both drawn in sawgrass_blades.
        assert!(lib.material("feather_tussock_leaf@sage").is_some());
        assert!(lib.material("sawgrass_leaf@sage").is_none());
        // Wisteria's violet comes in three shades, so three materials.
        for k in 0..3 {
            assert!(jade.material(&format!("wisteria_leaf@violet.{k}")).is_some(), "shade {k}");
        }
        assert!(jade.material("wisteria_leaf@violet.3").is_none());
        // The coat material keeps the base's pattern, repainted.
        let m = lib.materials.iter().find(|m| m.name == "feather_tussock_leaf@sage").unwrap();
        let base = lib.materials.iter().find(|m| m.name == "sawgrass_blades").unwrap();
        assert_eq!(m.texture.pattern, base.texture.pattern);
        assert_eq!(m.texture.base, [114.0, 130.0, 84.0], "painted what the coat asked");
        assert_ne!(m.texture.alt, base.texture.alt, "the pattern's second colour came along");
    }

    /// The coat is drawn per patch of ground, not per plant, and
    /// the whole point of the blob field.
    #[test]
    fn coats_lie_in_patches_and_keep_their_shares() {
        let lib = lib("windbarrow");
        let pi = lib.plant("feather_tussock").unwrap();
        let blob_m = lib.plants[pi].morph_blob_m;
        let coat = |x: f32, z: f32| coat_at(&lib, pi, None, x, z, 1234).get("leaf");
        // Neighbours a metre apart are nearly always the same colour; points
        // several patches apart often are not.
        let near = (0..400).filter(|k| {
            let (x, z) = ((*k % 20) as f32 * 37.0, (*k / 20) as f32 * 41.0);
            coat(x, z) == coat(x + 1.0, z + 1.0)
        });
        assert!(near.count() > 380, "a patch holds one colour");
        let far = (0..400).filter(|k| {
            let (x, z) = ((*k % 20) as f32 * 37.0, (*k / 20) as f32 * 41.0);
            coat(x, z) == coat(x + blob_m * 3.0, z)
        });
        let far = far.count();
        assert!((80..320).contains(&far), "patches further off differ: {far}/400");
        // Over a wide field the three coats come out near their weights
        // (5:3:2), which is what makes a rare coat rare.
        let mut seen: std::collections::HashMap<Option<u8>, usize> = Default::default();
        for k in 0..10_000 {
            let (x, z) = ((k % 100) as f32 * 29.0, (k / 100) as f32 * 31.0);
            *seen.entry(coat(x, z)).or_default() += 1;
        }
        assert_eq!(seen.len(), 3, "all three coats turn up");
        let mut shares: Vec<f32> = seen.values().map(|n| *n as f32 / 10_000.0).collect();
        shares.sort_by(|a, b| b.partial_cmp(a).unwrap());
        for (got, want) in shares.iter().zip([0.5, 0.3, 0.2]) {
            assert!((got - want).abs() < 0.05, "{shares:?}");
        }
    }

    /// A biome's `morph` stands in for the plant's own weights.
    #[test]
    fn a_biome_can_ask_for_one_coat() {
        let lib = lib("windbarrow");
        let pi = lib.plant("feather_tussock").unwrap();
        let pick = MorphPick::One("silver".into());
        let want = lib.material("feather_tussock_leaf@silver");
        assert!(want.is_some());
        for k in 0..200 {
            let (x, z) = ((k % 20) as f32 * 53.0, (k / 20) as f32 * 59.0);
            assert_eq!(coat_at(&lib, pi, Some(&pick), x, z, 7).get("leaf"), want);
        }
    }
}

/// Coral and sponges (docs/UNDERWATER.md), grown from the same limbs and
/// blobs as the trees. Limbs wear the plant's `bark` (the skeleton's colour),
/// blobs its `leaf` (the living colony), and `flower` where it has one tips
/// the branches (the paler growing ends). `height` is how tall the colony
/// stands, `crown` how far it spreads, `trunk_r` how thick its limbs are.
#[allow(clippy::too_many_arguments)]
fn coral(
    form: Form,
    rnd: &mut dyn FnMut() -> f32,
    base: Vec3,
    height: f32,
    trunk_r: f32,
    crown: f32,
    stems: Option<[f32; 2]>,
    flower: Option<u8>,
    blobs: &mut Vec<Blob>,
    branches: &mut Vec<(Vec3, Vec3, f32)>,
) -> (TreeKind, f32, f32) {
    let r0 = trunk_r.max(0.08);
    let tip = |blobs: &mut Vec<Blob>, at: Vec3, r: f32| {
        let mut b = Blob::ball(at, r);
        if let Some(m) = flower {
            b.mat = m;
        }
        blobs.push(b);
    };
    let count = |rnd: &mut dyn FnMut() -> f32, dflt: f32, min: f32| stems.map_or(dflt, |s| s[0] + (s[1] - s[0]) * rnd()).round().max(min) as usize;
    match form {
        // Staghorn: limbs forking up and out from one foot, each fork
        // shorter, the ends pale and blunt. A thicket of them is a field of
        // antlers.
        Form::CoralBranch => {
            fn fork(branches: &mut Vec<(Vec3, Vec3, f32)>, tips: &mut Vec<(Vec3, f32)>, at: Vec3, d: Vec3, len: f32, r: f32, depth: u32, rnd: &mut dyn FnMut() -> f32) {
                let end = at + (d + Vec3::Y * 0.15).normalize() * len * (0.8 + 0.35 * rnd());
                branches.push((at, end, r));
                if depth == 0 {
                    tips.push((end, r));
                    return;
                }
                let side = d.cross(Vec3::Y).normalize_or(Vec3::X);
                let axis = glam::Quat::from_axis_angle(d.normalize_or(Vec3::Y), rnd() * TAU) * side;
                let spread = 0.45 + 0.3 * rnd();
                for s in [-1.0f32, 1.0] {
                    let nd = glam::Quat::from_axis_angle(axis, spread * s) * d;
                    fork(branches, tips, end, nd, len * 0.72, (r * 0.85).max(0.08), depth - 1, rnd);
                }
            }
            let n = count(rnd, 5.0, 2.0);
            let a0 = rnd() * TAU;
            let mut tips = Vec::new();
            for k in 0..n {
                let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.8;
                let tilt = 0.3 + 0.55 * rnd();
                let d = Vec3::Y * tilt.cos() + Vec3::new(az.cos(), 0.0, az.sin()) * tilt.sin() * (crown / height.max(0.3)).min(1.6);
                fork(branches, &mut tips, base + Vec3::Y * 0.05, d.normalize(), height * 0.5, r0, 2, rnd);
            }
            for (t, r) in tips {
                tip(blobs, t, r * 1.35);
            }
            (TreeKind::Broad, 0.1, 0.0)
        }
        // Elkhorn: a few thick limbs that flatten into broad palms facing up
        // and out, like antlers lying in the surge.
        Form::CoralAntler => {
            let n = count(rnd, 4.0, 2.0);
            let a0 = rnd() * TAU;
            for k in 0..n {
                let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.6;
                let out = Vec3::new(az.cos(), 0.0, az.sin());
                let mid = base + Vec3::Y * height * (0.35 + 0.15 * rnd()) + out * crown * 0.3;
                let end = mid + Vec3::Y * height * (0.3 + 0.2 * rnd()) + out * crown * (0.35 + 0.2 * rnd());
                branches.push((base + Vec3::Y * 0.1, mid, r0 * 1.2));
                branches.push((mid, end, r0));
                // The palm: a thick rounded sleeve along the limb's outer
                // half and a lobe at its end. (A flat plate at a voxel or two
                // thick drew as a board.)
                let w = crown * (0.2 + 0.06 * rnd());
                blobs.push(Blob::sleeve(mid.lerp(end, 0.3), w, end - mid.lerp(end, 0.3)));
                blobs.push(Blob::dome(end, w * 1.25, 0.6));
                // A shorter tine off the side of each limb.
                if rnd() < 0.7 {
                    let side = glam::Quat::from_rotation_y((rnd() - 0.5) * 1.6) * out;
                    let s = mid + side * crown * 0.35 + Vec3::Y * height * 0.2;
                    branches.push((mid, s, r0 * 0.8));
                    blobs.push(Blob::sleeve(mid.lerp(s, 0.4), w * 0.8, s - mid.lerp(s, 0.4)));
                }
            }
            (TreeKind::Broad, 0.1, 0.0)
        }
        // Table coral: a short stalk and a wide flat plate; now and then a
        // second, smaller tier.
        Form::CoralTable => {
            let stalk = height * (0.55 + 0.25 * rnd());
            let lean = Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * 0.4;
            let top = base + (Vec3::Y + lean).normalize() * stalk;
            branches.push((base, top, r0 * 1.4));
            let axis = (Vec3::Y + Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * 0.25).normalize();
            blobs.push(Blob::plate(top + Vec3::Y * 0.05, crown, axis, 0.09));
            if rnd() < 0.35 {
                let a = rnd() * TAU;
                let off = Vec3::new(a.cos(), 0.0, a.sin()) * crown * 0.7;
                let t2 = base + off * 0.6 + Vec3::Y * stalk * 0.55;
                branches.push((base, t2, r0));
                blobs.push(Blob::plate(t2, crown * 0.55, axis, 0.1));
            }
            (TreeKind::Broad, 0.1, 0.0)
        }
        // Brain and boulder coral: a dome sitting on the bed, now and then
        // with smaller domes grown against it.
        Form::CoralMound => {
            let rise = (0.65 + 0.35 * rnd()) * (height / crown.max(0.2)).clamp(0.5, 1.6);
            let c = base + Vec3::Y * crown * 0.08;
            blobs.push(Blob::dome(c, crown, rise));
            let extra = (rnd() * 3.2) as usize;
            for _ in 0..extra {
                let a = rnd() * TAU;
                let r = crown * (0.45 + 0.25 * rnd());
                let at = c + Vec3::new(a.cos(), 0.0, a.sin()) * (crown + r) * 0.65;
                blobs.push(Blob::dome(at, r, rise));
            }
            (TreeKind::Broad, 0.0, 0.0)
        }
        // Plate and lettuce coral: overlapping tilted plates in a whorl,
        // climbing a little toward the middle.
        Form::CoralPlate => {
            let n = count(rnd, 6.0, 3.0);
            let a0 = rnd() * TAU;
            for k in 0..n {
                let az = a0 + k as f32 * TAU / n as f32 + (rnd() - 0.5) * 0.7;
                let out = Vec3::new(az.cos(), 0.0, az.sin());
                let tilt = 0.35 + 0.45 * rnd();
                let axis = (Vec3::Y * tilt.cos() - out * tilt.sin()).normalize();
                let c = base + out * crown * (0.35 + 0.25 * rnd()) + Vec3::Y * height * (0.45 + 0.55 * (1.0 - k as f32 / n as f32) * rnd());
                blobs.push(Blob::plate(c, crown * (0.45 + 0.2 * rnd()), axis, 0.22));
            }
            (TreeKind::Broad, 0.0, 0.0)
        }
        // Finger coral: a low cushion bristling with short blunt fingers.
        Form::CoralFinger => {
            blobs.push(Blob::dome(base - Vec3::Y * 0.05, crown * 0.7, 0.35));
            let n = count(rnd, 9.0, 3.0);
            for _ in 0..n {
                let a = rnd() * TAU;
                let d = crown * 0.65 * rnd().sqrt();
                let foot = base + Vec3::new(a.cos() * d, crown * 0.15, a.sin() * d);
                let lean = Vec3::new(a.cos(), 0.0, a.sin()) * (d / crown.max(0.1)) * 0.35;
                let top = foot + (Vec3::Y + lean).normalize() * height * (0.55 + 0.45 * rnd());
                branches.push((foot, top, r0));
                tip(blobs, top, r0 * 1.2);
            }
            (TreeKind::Broad, 0.0, 0.0)
        }
        // A sea fan: a lattice of fine branches spread flat in one upright
        // plane across the surge. Fans on one reef face the same way, give
        // or take, since they all grow across the same swell.
        Form::SeaFan => {
            let face = 0.6 + (rnd() - 0.5) * 0.9;
            let across = Vec3::new(face.cos(), 0.0, face.sin());
            let normal = across.cross(Vec3::Y);
            #[allow(clippy::too_many_arguments)]
            fn fan(branches: &mut Vec<(Vec3, Vec3, f32)>, at: Vec3, d: Vec3, normal: Vec3, len: f32, r: f32, depth: u32, rnd: &mut dyn FnMut() -> f32) {
                let end = at + d * len * (0.8 + 0.3 * rnd());
                branches.push((at, end, r));
                if depth == 0 {
                    return;
                }
                let n = if depth > 2 { 2 } else { 2 + (rnd() < 0.4) as usize };
                for k in 0..n {
                    let s = if n == 2 { k as f32 * 2.0 - 1.0 } else { k as f32 - 1.0 };
                    let nd = glam::Quat::from_axis_angle(normal, s * (0.42 + 0.2 * rnd())) * d;
                    // Keep growing upward, not round and down.
                    let nd = if nd.y < 0.15 { (nd + Vec3::Y * 0.4).normalize() } else { nd };
                    fan(branches, end, nd, normal, len * 0.72, (r * 0.85).max(0.06), depth - 1, rnd);
                }
            }
            let stalk = base + Vec3::Y * height * 0.12;
            branches.push((base, stalk, r0 * 1.3));
            for s in [-1.0f32, 0.0, 1.0] {
                let d = (Vec3::Y + across * s * 0.9).normalize();
                fan(branches, stalk, d, normal, height * 0.38, r0, 3, rnd);
            }
            (TreeKind::Broad, 0.0, 0.0)
        }
        // Sponges: a cluster of upright tubes, fat and blunt; a single one
        // is a barrel. (Only a coral form is ever handed in, so the arm
        // that catches the rest is the sponge's.)
        _ => {
            let n = count(rnd, 4.0, 1.0);
            for k in 0..n {
                let a = rnd() * TAU;
                let d = if k == 0 { 0.0 } else { crown * (0.3 + 0.5 * rnd()) };
                let foot = base + Vec3::new(a.cos() * d, 0.0, a.sin() * d);
                let lean = Vec3::new(a.cos(), 0.0, a.sin()) * 0.25 * (d / crown.max(0.1));
                let top = foot + (Vec3::Y + lean).normalize() * height * (0.5 + 0.5 * rnd());
                let r = r0 * (0.8 + 0.4 * rnd());
                branches.push((foot, top, r));
                blobs.push(Blob::ball(top - Vec3::Y * r * 0.3, r * 1.15));
            }
            (TreeKind::Broad, 0.0, 0.0)
        }
    }
}
