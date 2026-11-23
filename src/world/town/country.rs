//! The fields round a town.
//!
//! Fields are laid out from the roads, the way strip fields were: along
//! each side of every road out of town the ground is cut into strips square
//! to the road, a field's width apart, one or two rows deep, the back of
//! each row a line parallel to the road. Each patch of ground belongs to the
//! road whose verge is nearest it (as each plot in town belongs to its
//! street), so where two roads' strips meet they meet along a line, and the
//! boundary between any two fields is found once, from both sides: two
//! fields never each build a wall along the same line. Every field is
//! closed: the last row and the ragged limit of the farmland have a
//! boundary of their own, and the boundaries stop short of the roads.

use super::graph::seg_foot;
use super::{rank, Boundary, Districts, Edge, Ground, Plan, Rng, Spec};
use crate::util::FxHashMap as HashMap;
use crate::world::noise::{hash32, perlin2};
use glam::Vec2;

/// Raster step, metres.
const CELL: f32 = 2.0;
/// No field: the verge, the town, the land past the farms.
const NONE: u64 = u64::MAX;

/// Which crop, if any, each patch of farmland grows: a raster `step` m a
/// cell from `lo`, 255 for none.
#[derive(Default, Clone)]
pub struct Crops {
    pub lo: Vec2,
    pub step: f32,
    pub w: usize,
    pub h: usize,
    pub cells: Vec<u8>,
}

impl Crops {
    pub fn at(&self, p: Vec2) -> u8 {
        let q = (p - self.lo) / self.step;
        if q.x < 0.0 || q.y < 0.0 || q.x as usize >= self.w || q.y as usize >= self.h {
            return 255;
        }
        self.cells[q.y as usize * self.w + q.x as usize]
    }
}

/// What fields are laid from.
pub struct Farmland<'a> {
    /// The roads fields hang off: a line and its half-width.
    pub roads: Vec<(Vec<Vec2>, f32)>,
    /// Everything walls keep off: segments with their half-width (every
    /// street and lane), and points (farm buildings) with a radius.
    pub keep_off: Vec<(Vec2, Vec2, f32)>,
    pub avoid: Vec<(Vec2, f32)>,
    /// Is this farmland?
    pub band: &'a dyn Fn(Vec2) -> bool,
    /// Near a settlement: walled with stone rather than hedged.
    pub near: &'a dyn Fn(Vec2) -> bool,
    /// The box to work in.
    pub lo: Vec2,
    pub hi: Vec2,
    pub size: f32,
    pub seed: u32,
}

/// What laying them made.
pub struct LaidFields {
    pub boundaries: Vec<Boundary>,
    pub crops: Crops,
    /// Oaks standing out of the hedges.
    pub trees: Vec<(Vec2, super::Tree)>,
}

/// A town's fields: farmland `fields` m round it (or inside `farmland` when
/// the caller gives an outline), off the town's roads out and any roads the
/// world already has.
pub fn fields(plan: &mut Plan, dist: &Districts, spec: &Spec, ground: &dyn Ground) {
    // The farmland reaches `fields` m past the town's edge, give or take a
    // long way, so it follows the town's shape rather than a circle.
    let limit = |p: Vec2| spec.fields * (1.0 + 0.55 * perlin2(p.x * 0.007 + 3.0, p.y * 0.007, spec.seed ^ 0x77) + 0.2 * perlin2(p.x * 0.02, p.y * 0.02, spec.seed ^ 0x78));
    let reach = dist.ext.reach + spec.fields * 1.4;
    plan.farm = reach;
    let outline = spec.farmland.clone();
    let band = |p: Vec2| {
        let s = dist.sd(p);
        if !outline.is_empty() {
            return s > 8.0 && super::extent::in_poly(&outline, p);
        }
        s > 8.0 && s < limit(p)
    };
    let mut roads: Vec<(Vec<Vec2>, f32)> = plan.ways.iter().filter(|w| w.rank == rank::COUNTRY && w.pts.len() >= 2).map(|w| (w.pts.clone(), w.half)).collect();
    roads.extend(spec.roads.iter().cloned());
    let ok_farm = |p: Vec2| band(p) && dist.sd(p) > 40.0;
    let (farm_buildings, farm_at) = farmsteads(&roads, &ok_farm, spec.seed, ground);
    plan.buildings.extend(farm_buildings);
    let mut keep_off: Vec<(Vec2, Vec2, f32)> = plan.ways.iter().filter(|w| w.drawn).flat_map(|w| w.pts.windows(2).map(move |s| (s[0], s[1], w.half + w.walk))).collect();
    keep_off.extend(spec.roads.iter().flat_map(|(l, h)| l.windows(2).map(move |s| (s[0], s[1], *h))));
    let c = dist.c;
    let (lo, hi) = if outline.is_empty() {
        ((c - Vec2::splat(reach)).max(Vec2::ZERO), (c + Vec2::splat(reach)).min(Vec2::splat(ground.size())))
    } else {
        let lo = outline.iter().fold(Vec2::splat(f32::MAX), |a, &b| a.min(b));
        let hi = outline.iter().fold(Vec2::splat(f32::MIN), |a, &b| a.max(b));
        (lo.max(Vec2::ZERO), hi.min(Vec2::splat(ground.size())))
    };
    let near = |p: Vec2| dist.sd(p) < spec.fields.max(60.0) * 0.4;
    let laid = lay(
        &Farmland {
            roads,
            keep_off,
            avoid: farm_at.iter().map(|&p| (p, 14.0)).collect(),
            band: &band,
            near: &near,
            lo,
            hi,
            size: spec.field_size,
            seed: spec.seed,
        },
        ground,
    );
    plan.crops = laid.crops;
    plan.boundaries.extend(laid.boundaries);
    plan.trees.extend(laid.trees);
}

/// Lay fields: each patch of farmland goes to the road whose verge is
/// nearest, cut into strips square to it and rows behind, and a boundary is
/// built once for each pair of neighbouring fields.
pub fn lay(f: &Farmland, ground: &dyn Ground) -> LaidFields {
    let mut rng = Rng::new(f.seed ^ 0xf1e1d);
    let size = f.size.max(30.0);
    let lo = f.lo;
    let w = (((f.hi.x - lo.x) / CELL) as usize).max(1);
    let h = (((f.hi.y - lo.y) / CELL) as usize).max(1);
    let mut out = LaidFields { boundaries: Vec::new(), crops: Crops { lo, step: CELL, w, h, cells: vec![255; w * h] }, trees: Vec::new() };
    let near_road = |p: Vec2, m: f32| f.keep_off.iter().any(|&(a, b, hw)| seg_foot(a, b, p).0 < hw + m) || f.avoid.iter().any(|&(q, r)| q.distance(p) < r);
    let in_band = |p: Vec2| (f.band)(p);
    if f.roads.is_empty() {
        return out;
    }
    // The fields square to the road's general line: a bend or a zigzag
    // smaller than a quarter field (a road dropping to a ford) would fan
    // the cuts out from it. Walls still keep off the road as laid.
    let roads: Vec<(Vec<Vec2>, f32)> = f.roads.iter().map(|(p, h)| (super::grow::simplify(p, size * 0.25), *h)).collect();
    let roads = &roads;
    struct Side {
        cuts: Vec<f32>,
        deep: f32,
    }
    let mut sides: Vec<Side> = Vec::new();
    for (pts, _) in roads.iter() {
        let len: f32 = pts.windows(2).map(|s| s[0].distance(s[1])).sum();
        for _ in 0..2 {
            let mut cuts = vec![0.0];
            let mut s = rng.range(0.0, size * 0.5);
            while s < len {
                cuts.push(s);
                s += size * rng.range(0.45, 0.75);
            }
            cuts.push(len + 1.0);
            sides.push(Side { cuts, deep: size * rng.range(0.8, 1.15) });
        }
    }

    // Each cell: the nearest road side, its distance past the verge, and how
    // far along the road its foot is.
    let mut owner = vec![u32::MAX; w * h];
    let mut kd = vec![f32::MAX; w * h];
    let mut along = vec![0.0f32; w * h];
    // Past a road's end its nearest point is the end itself, and distance
    // from it rings out in arcs: such ground gets plain fields instead.
    let mut cap = vec![false; w * h];
    // Every road's fields run back until they meet another road's, or the
    // end of the farmland.
    let max_deep = (f.hi - f.lo).max_element().min(600.0);
    for (ri, (pts, half)) in roads.iter().enumerate() {
        let mut run = 0.0;
        let nseg = pts.len().saturating_sub(1);
        for (si, seg) in pts.windows(2).enumerate() {
            let (a, b) = (seg[0], seg[1]);
            let len = a.distance(b);
            let start = run;
            run += len;
            let blo = a.min(b) - Vec2::splat(max_deep);
            let bhi = a.max(b) + Vec2::splat(max_deep);
            let x0 = (((blo.x - lo.x) / CELL).max(0.0)) as usize;
            let z0 = (((blo.y - lo.y) / CELL).max(0.0)) as usize;
            let x1 = (((bhi.x - lo.x) / CELL).max(0.0) as usize).min(w);
            let z1 = (((bhi.y - lo.y) / CELL).max(0.0) as usize).min(h);
            for z in z0..z1 {
                for x in x0..x1 {
                    let i = z * w + x;
                    let p = lo + Vec2::new((x as f32 + 0.5) * CELL, (z as f32 + 0.5) * CELL);
                    let (d, _, t) = seg_foot(a, b, p);
                    let d = d - half;
                    if d < kd[i] {
                        kd[i] = d;
                        let side = ((b - a).perp_dot(p - a) > 0.0) as u32;
                        owner[i] = ri as u32 * 2 + side;
                        along[i] = start + t * len;
                        cap[i] = (si == 0 && t <= 0.0) || (si + 1 == nseg && t >= 1.0);
                    }
                }
            }
        }
    }
    // Each cell's field: road side, strip, row.
    let mut key = vec![NONE; w * h];
    for i in 0..w * h {
        if owner[i] == u32::MAX {
            continue;
        }
        let p = lo + Vec2::new(((i % w) as f32 + 0.5) * CELL, ((i / w) as f32 + 0.5) * CELL);
        if kd[i] < 3.0 || !in_band(p) || ground.wet(p) {
            continue;
        }
        if cap[i] {
            // A plain grid of fields, about a field and a half across.
            let g = size * 1.4;
            let (gx, gz) = (((p.x / g).floor() as i64 + 100_000) as u64, ((p.y / g).floor() as i64 + 100_000) as u64);
            key[i] = (0xFFFF_0000u64 << 32) | ((gx & 0xFFFF) << 16) | (gz & 0xFFFF);
            continue;
        }
        let sd = &sides[owner[i] as usize];
        let raw_row = (kd[i] / sd.deep) as u32;
        // Far back, rows pair up, so a field never gets very deep or thin.
        let row = if raw_row < 4 { raw_row } else { 4 + (raw_row - 4) / 2 }.min(15);
        // Further back the fields widen to every other cut and no more, so
        // the cuts of the rows behind still meet the ones in front.
        let strip = sd.cuts.partition_point(|&k| k <= along[i]) as u64 / (1 + (row as u64 / 2).min(1));
        key[i] = ((owner[i] as u64) << 32) | (strip << 4) | row as u64;
    }
    // Crops by field.
    for i in 0..w * h {
        if key[i] != NONE {
            out.crops.cells[i] = (hash32((key[i] as u32) ^ ((key[i] >> 32) as u32).wrapping_mul(0x9e37_79b9) ^ f.seed) >> 29) as u8;
        }
    }

    // The boundaries: every edge between two cells of different fields
    // (or a field and no field), gathered by pair.
    let mut pairs: HashMap<(u64, u64), Vec<Vec2>> = HashMap::default();
    for z in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let a = key[z * w + x];
            for (b, off) in [(key[z * w + x + 1], Vec2::new(1.0, 0.5)), (key[(z + 1) * w + x], Vec2::new(0.5, 1.0))] {
                if a == b || (a == NONE && b == NONE) {
                    continue;
                }
                let p = lo + (Vec2::new(x as f32, z as f32) + off) * CELL;
                // Not along the verge: the road is the field's edge there.
                if near_road(p, 3.0) {
                    continue;
                }
                pairs.entry((a.min(b), a.max(b))).or_default().push(p);
            }
        }
    }
    // A farm's boundaries are of a kind: walls near the town and on high
    // ground, hedges further out and lower down, the odd railed paddock.
    let mean_h = {
        let hs: Vec<f32> = roads.iter().flat_map(|r| r.0.iter().map(|&p| ground.height(p))).collect();
        hs.iter().sum::<f32>() / hs.len().max(1) as f32
    };
    let kind_at = |p: Vec2, k: u64| {
        let near = (f.near)(p);
        let high = ground.height(p) > mean_h + 4.0;
        if hash32((k >> 4) as u32 ^ f.seed) % 12 == 0 {
            Edge::Rail
        } else if near || high {
            Edge::FieldWall
        } else {
            Edge::Hedge
        }
    };
    let mut keys: Vec<(u64, u64)> = pairs.keys().copied().collect();
    keys.sort_unstable();
    for pk in keys {
        let pts = &pairs[&pk];
        if pts.len() < 3 {
            continue;
        }
        // Order the edge's points along it (its principal axis), then cut
        // it wherever it jumps, and straighten what is left.
        let mean = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
        let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
        for &p in pts {
            let d = p - mean;
            sxx += d.x * d.x;
            sxy += d.x * d.y;
            syy += d.y * d.y;
        }
        let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let axis = Vec2::new(ang.cos(), ang.sin());
        let mut sorted: Vec<Vec2> = pts.clone();
        sorted.sort_by(|a, b| (*a - mean).dot(axis).total_cmp(&(*b - mean).dot(axis)));
        let edge = kind_at(mean, pk.0.min(pk.1));
        let mut piece: Vec<Vec2> = Vec::new();
        let flush = |piece: &mut Vec<Vec2>, out: &mut LaidFields| {
            if piece.len() >= 3 {
                let line = super::grow::simplify(piece, 1.2);
                let len: f32 = line.windows(2).map(|s| s[0].distance(s[1])).sum();
                if len > 6.0 {
                    let line = super::grow::resample(&line, 2.0);
                    if edge == Edge::Hedge {
                        for (k, &p) in line.iter().enumerate() {
                            if k % 17 == 8 && hash32((p.x as u32) ^ ((p.y as u32) << 12) ^ f.seed) % 3 == 0 {
                                out.trees.push((p, super::Tree::Hedge));
                            }
                        }
                    }
                    out.boundaries.push(Boundary { pts: line, edge });
                }
            }
            piece.clear();
        };
        for &p in &sorted {
            if piece.last().is_some_and(|q: &Vec2| q.distance(p) > 5.0) {
                flush(&mut piece, &mut out);
            }
            piece.push(p);
        }
        flush(&mut piece, &mut out);
    }
    out
}

/// Farmsteads along roads: a farmhouse by the road and a barn behind it,
/// every 250 m or so where `ok` says there is farmland.
pub fn farmsteads(roads: &[(Vec<Vec2>, f32)], ok: &dyn Fn(Vec2) -> bool, seed: u32, ground: &dyn Ground) -> (Vec<super::Building>, Vec<Vec2>) {
    use super::{Building, Kind, Roof};
    let mut rng = Rng::new(seed ^ 0xfa53);
    let mut at: Vec<Vec2> = Vec::new();
    let mut out = Vec::new();
    for (pts, half) in roads {
        let len: f32 = pts.windows(2).map(|s| s[0].distance(s[1])).sum();
        let mut s = rng.range(60.0, 160.0);
        while s < len - 20.0 {
            let (p, t) = super::lots::point_at(pts, s);
            if ok(p) {
                let side = if rng.f() < 0.5 { 1.0 } else { -1.0 };
                let n = Vec2::new(-t.y, t.x) * side;
                let house_c = p + n * (half + 9.0);
                let barn_c = house_c + n * 16.0 + t * rng.range(-6.0, 6.0);
                let rect = |cc: Vec2, along: f32, across: f32| -> Vec<Vec2> {
                    vec![cc - t * along - n * across, cc + t * along - n * across, cc + t * along + n * across, cc - t * along + n * across]
                };
                let fits = |poly: &[Vec2]| {
                    poly.iter().all(|&q| !ground.wet(q) && ok(q)) && super::fall(ground, poly) < 2.5
                };
                let house = rect(house_c, 5.5, 3.8);
                let barn = rect(barn_c, 9.0, 5.0);
                if fits(&house) && fits(&barn) && at.iter().all(|q| q.distance(house_c) > 150.0) {
                    out.push(Building { poly: house, front: 0, kind: Kind::House, roof: Roof::Gable, floors: 2, style: super::Style::Village, seed: rng.f().to_bits(), ridge: Some(t), tone: rng.f().to_bits() });
                    out.push(Building { poly: barn, front: 0, kind: Kind::Barn, roof: Roof::Gable, floors: 1, style: super::Style::Village, seed: rng.f().to_bits(), ridge: Some(t), tone: rng.f().to_bits() });
                    at.push(house_c);
                    at.push(barn_c);
                }
            }
            s += rng.range(200.0, 320.0);
        }
    }
    (out, at)
}
