//! Measuring the static grids from a built world. Everything is read from
//! the fields and the lists beside them (structures, plants, lamps, roads),
//! not from voxels: full voxels are kept only near the player.
//!
//! The work is in two passes. The first goes over the world's 1 m cells and
//! notes what stands on each: how high something built rises over the
//! ground, whether a roof covers it, whether a trunk does, what crowns and
//! undergrowth shade it, how deep any water is. The second gathers each
//! 4 m survey cell from its sixteen 1 m cells, and the neighbourhood
//! channels (rise, built, openness, the distances) are worked out on the
//! 4 m grid after that.

use super::{water, Chan, CELL_M};
use crate::creatures::affordance::Affordance;
use crate::creatures::sign::{tread_of, Takes};
use crate::world::fields::{World, CELL};
use crate::world::gen::{TreeKind, VOXEL};
use crate::world::matlib::{self, flag};

/// 1 m cells per survey cell along a side.
const SUB: usize = (CELL_M / CELL) as usize;

/// What the graph (graph.rs) needs beyond the grids: the 1 m cells, where
/// a person can stand at 1 m, the heights in the way, the banks.
pub(super) struct Ground {
    pub cells: Vec<Cell>,
    /// A person can stand on each 1 m cell.
    pub pass1: Vec<bool>,
    pub obstr: Obstruction,
    /// Survey cells a body can drink from (dry, holding or beside fresh water).
    pub bank: Vec<bool>,
    /// Mean ground height per survey cell.
    pub ground: Vec<f32>,
}

/// What one 1 m cell has on it.
#[derive(Clone, Copy, Default)]
pub(super) struct Cell {
    /// The step up to what is built here, metres over the ground: the floor
    /// under the lowest headroom, or the top of a wall with none.
    pub built: f32,
    /// The top of what is built here, roof and all.
    pub tall: f32,
    pub roof: bool,
    trunk: bool,
    /// The top of the tree crowns over it, metres above the ground, and how
    /// many crowns overlap there.
    crown_top: f32,
    crowns: u8,
    /// Undergrowth (shrubs and tall herbs) under 3 m: its height.
    under: f32,
    browse: bool,
    nectar: bool,
    /// Water over the ground, metres, and whether it is the sea.
    depth: f32,
    salt: bool,
}

/// What each kind of body can cross, per 1 m cell: steepest slope (rise per
/// metre), the most cliff, deepest water, highest step built.
struct Limits {
    slope: f32,
    cliff: f32,
    depth: f32,
    step: f32,
    trunks: bool,
}

const SMALL: Limits = Limits { slope: 1.2, cliff: 0.6, depth: 0.15, step: 0.3, trunks: true };
const PERSON: Limits = Limits { slope: 1.0, cliff: 0.6, depth: 1.0, step: 0.6, trunks: true };
const LARGE: Limits = Limits { slope: 0.7, cliff: 0.4, depth: 1.2, step: 0.5, trunks: false };
const VEHICLE: Limits = Limits { slope: 0.45, cliff: 0.3, depth: 0.4, step: 0.3, trunks: false };
/// Water a swimmer needs under it.
const SWIM_DEPTH: f32 = 0.8;
/// How many of a survey cell's 16 1 m cells must pass for the cell to.
const PASS_OF_16: usize = 10;

/// The grids, in `Chan::ALL` order, and their side.
pub(super) fn grids(w: &World) -> (usize, Vec<Vec<u8>>, Ground) {
    let f = &w.f;
    let n1 = f.n;
    let n = n1.div_ceil(SUB);
    let cells = one_metre(w);
    let mut g: Vec<Vec<u8>> = vec![vec![0u8; n * n]; Chan::ALL.len()];

    // Each material's tread and whether it is grass, once.
    let mut loud = [0f32; 256];
    let mut takes = [0u8; 256];
    let mut grass = [false; 256];
    for m in 0..=255u8 {
        let t = tread_of(m);
        loud[m as usize] = t.loud;
        takes[m as usize] = match t.takes {
            Takes::Nothing => 0,
            Takes::Bends => 1,
            Takes::WhenSoft => 2,
            Takes::Print => 3,
        };
        grass[m as usize] = matlib::has(m, flag::UNDERSTORY) || matlib::grass(m).is_some();
    }
    let water_tread = tread_of_water();

    // Mean ground height per survey cell, and whether one holds any fresh
    // water a body could drink from.
    let mut ground = vec![0f32; n * n];
    let mut fresh_any = vec![false; n * n];
    let mut built_share = vec![0f32; n * n];
    let mut road_seed = vec![false; n * n];
    let slope_at = |x: usize, z: usize| {
        let at = |x: usize, z: usize| f.h[z.min(n1 - 1) * n1 + x.min(n1 - 1)];
        let gx = (at(x + 1, z) - at(x.saturating_sub(1), z)) / (2.0 * CELL);
        let gz = (at(x, z + 1) - at(x, z.saturating_sub(1))) / (2.0 * CELL);
        (gx * gx + gz * gz).sqrt()
    };
    let passes = |l: &Limits, i: usize, c: &Cell, slope: f32| {
        slope <= l.slope && f.cliff[i] <= l.cliff && c.depth <= l.depth && c.built <= l.step && (l.trunks || !c.trunk)
    };
    let mut pass1 = vec![false; n1 * n1];

    for cz in 0..n {
        for cx in 0..n {
            let k = cz * n + cx;
            let mut count = 0usize;
            let (mut small, mut person, mut large, mut vehicle, mut swim) = (0, 0, 0, 0, 0);
            let (mut fresh, mut salt, mut wet_depth) = (0usize, 0usize, 0f32);
            let (mut browse, mut nectar, mut graze, mut roof, mut canopy, mut under, mut low_wall, mut tall) = (0, 0, 0, 0, 0, 0, 0, 0);
            let (mut loud_sum, mut h_sum, mut paths) = (0f32, 0f32, 0usize);
            let mut prints = [0usize; 4];
            for sz in 0..SUB {
                for sx in 0..SUB {
                    let (x, z) = (cx * SUB + sx, cz * SUB + sz);
                    if x >= n1 || z >= n1 {
                        continue;
                    }
                    let i = z * n1 + x;
                    let c = &cells[i];
                    count += 1;
                    h_sum += f.h[i];
                    let s = slope_at(x, z);
                    small += passes(&SMALL, i, c, s) as usize;
                    let p1 = passes(&PERSON, i, c, s);
                    pass1[i] = p1;
                    person += p1 as usize;
                    large += passes(&LARGE, i, c, s) as usize;
                    vehicle += passes(&VEHICLE, i, c, s) as usize;
                    swim += (c.depth >= SWIM_DEPTH) as usize;
                    if c.depth > 0.05 {
                        if c.salt {
                            salt += 1;
                        } else {
                            fresh += 1;
                        }
                        wet_depth += c.depth;
                    }
                    let m = if f.surface[i] != 0 { f.surface[i] } else { w.biome_mats.get(f.biome[i] as usize).map_or(0, |b| b.surface) };
                    let dry = c.depth <= 0.05;
                    browse += c.browse as usize;
                    nectar += c.nectar as usize;
                    graze += (dry && grass[m as usize] && c.built < 0.3 && f.water[i] != 3) as usize;
                    roof += c.roof as usize;
                    canopy += (c.crowns > 0) as usize;
                    under += (c.under > 0.5) as usize;
                    low_wall += (c.built >= 0.8 && c.built < 3.0) as usize;
                    tall += (c.tall >= 2.0) as usize;
                    if dry {
                        loud_sum += loud[m as usize];
                        prints[takes[m as usize] as usize] += 1;
                    } else {
                        loud_sum += water_tread;
                    }
                    paths += (f.water[i] == 3) as usize;
                }
            }
            let cnt = count.max(1);
            let share = |v: usize| ((v as f32 / cnt as f32) * 255.0).round() as u8;
            let flag = |v: usize, need: usize| if v * 16 >= need * cnt { 255u8 } else { 0 };
            g[Chan::PassSmall as usize][k] = flag(small, PASS_OF_16);
            g[Chan::PassPerson as usize][k] = flag(person, PASS_OF_16);
            g[Chan::PassLarge as usize][k] = flag(large, PASS_OF_16);
            g[Chan::PassVehicle as usize][k] = flag(vehicle, PASS_OF_16);
            g[Chan::Swim as usize][k] = flag(swim, 8);
            let wet = fresh + salt;
            g[Chan::Water as usize][k] = if wet * 2 >= cnt {
                let d = wet_depth / wet as f32;
                if salt > fresh {
                    if d < 1.0 { water::SALT_SHALLOW } else { water::SALT_DEEP }
                } else if d < 0.3 {
                    water::FRESH_SHALLOW
                } else if d < 1.2 {
                    water::FRESH_WADE
                } else {
                    water::FRESH_DEEP
                }
            } else {
                water::DRY
            };
            fresh_any[k] = fresh > 0;
            g[Chan::Browse as usize][k] = share(browse);
            g[Chan::Nectar as usize][k] = share(nectar);
            g[Chan::Graze as usize][k] = share(graze);
            g[Chan::Roof as usize][k] = share(roof);
            let (cs, us, ls) = (canopy as f32 / cnt as f32, under as f32 / cnt as f32, low_wall as f32 / cnt as f32);
            let cover = 1.0 - (1.0 - 0.7 * cs) * (1.0 - 0.9 * us) * (1.0 - 0.8 * ls);
            g[Chan::Cover as usize][k] = (cover * 255.0).round() as u8;
            // Loudness 0..2 over the byte: ordinary ground (1) reads 0.5.
            g[Chan::Loud as usize][k] = ((loud_sum / cnt as f32 / 2.0).clamp(0.0, 1.0) * 255.0).round() as u8;
            g[Chan::Print as usize][k] = if wet * 2 >= cnt { 0 } else { (0..4).max_by_key(|&j| (prints[j], j)).unwrap_or(0) as u8 };
            ground[k] = h_sum / cnt as f32;
            built_share[k] = tall as f32 / cnt as f32;
            road_seed[k] = paths * 4 >= cnt;
        }
    }

    // Roads and paths: path cells, routed ribbons and race circuits.
    let mut seed_at = |x: f32, z: f32| {
        if x >= 0.0 && z >= 0.0 {
            let (cx, cz) = ((x / CELL_M) as usize, (z / CELL_M) as usize);
            if cx < n && cz < n {
                road_seed[cz * n + cx] = true;
            }
        }
    };
    for s in &w.ribbons.segs {
        let len = ((s.b - s.a).length() / 2.0).ceil().max(1.0) as usize;
        for j in 0..=len {
            let p = s.a.lerp(s.b, j as f32 / len as f32);
            seed_at(p.x, p.z);
        }
    }
    for c in &w.circuits {
        for p in &c.points {
            seed_at(p.x, p.y);
        }
    }
    g[Chan::Road as usize] = distance(&road_seed, n);

    // Drinking: dry ground a person can stand on, holding or beside fresh water.
    let pass_person = &g[Chan::PassPerson as usize];
    let wet_cell = |k: usize| g[Chan::Water as usize][k] != water::DRY;
    let mut bank = vec![false; n * n];
    for cz in 0..n {
        for cx in 0..n {
            let k = cz * n + cx;
            if wet_cell(k) || pass_person[k] == 0 {
                continue;
            }
            let near = |dx: i32, dz: i32| {
                let (x, z) = (cx as i32 + dx, cz as i32 + dz);
                x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n && fresh_any[z as usize * n + x as usize]
            };
            bank[k] = fresh_any[k] || near(1, 0) || near(-1, 0) || near(0, 1) || near(0, -1);
        }
    }
    g[Chan::Drink as usize] = distance(&bank, n);

    // Rise over the surroundings, and the built share round about.
    let rise = |r_m: f32| {
        let mean = box_mean(&ground, n, (r_m / CELL_M).round() as usize);
        ground.iter().zip(&mean).map(|(h, m)| (128.0 + (h - m).round()).clamp(0.0, 255.0) as u8).collect::<Vec<u8>>()
    };
    g[Chan::Rise50 as usize] = rise(50.0);
    g[Chan::Rise200 as usize] = rise(200.0);
    g[Chan::Built as usize] = box_mean(&built_share, n, (20.0 / CELL_M).round() as usize).iter().map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8).collect();

    // Lamplight.
    let light = &mut g[Chan::Light as usize];
    for l in &w.lamps {
        let r = l.reach.max(1.0);
        let (x0, x1) = (((l.pos.x - r) / CELL_M).floor().max(0.0) as usize, (((l.pos.x + r) / CELL_M).ceil().max(0.0) as usize).min(n));
        let (z0, z1) = (((l.pos.z - r) / CELL_M).floor().max(0.0) as usize, (((l.pos.z + r) / CELL_M).ceil().max(0.0) as usize).min(n));
        for cz in z0..z1 {
            for cx in x0..x1 {
                let (x, z) = ((cx as f32 + 0.5) * CELL_M, (cz as f32 + 0.5) * CELL_M);
                let d = ((x - l.pos.x).powi(2) + (z - l.pos.z).powi(2)).sqrt();
                if d < r {
                    let v = (l.strength.min(1.0) * (1.0 - d / r) * 255.0).round() as u8;
                    let k = cz * n + cx;
                    light[k] = light[k].max(v);
                }
            }
        }
    }

    let obstr = obstruction(w, &cells);
    g[Chan::Open as usize] = openness(w, &cells, &obstr, n);
    (n, g, Ground { cells, pass1, obstr, bank, ground })
}

/// How loud a step in water is (sign.rs reads water by its flag).
fn tread_of_water() -> f32 {
    crate::creatures::sign::tread_of_name("", true, false, false).loud
}

/// What stands on every 1 m cell (see `Cell`).
fn one_metre(w: &World) -> Vec<Cell> {
    let f = &w.f;
    let n1 = f.n;
    let mut cells = vec![Cell::default(); n1 * n1];
    let at = |x: f32, z: f32| -> Option<usize> {
        (x >= 0.0 && z >= 0.0 && x < f.size && z < f.size).then(|| (z / CELL) as usize * n1 + (x / CELL) as usize)
    };

    // Water: the rivers and lakes, then the sea.
    for (&i, wet) in &f.inland {
        let i = i as usize;
        if i < cells.len() && wet.level > f.h[i] {
            cells[i].depth = wet.level - f.h[i];
        }
    }
    for (i, c) in cells.iter_mut().enumerate() {
        if c.depth == 0.0 && f.h[i] < f.sea {
            c.depth = f.sea - f.h[i];
            c.salt = true;
        }
    }

    // Structures, at their voxels' columns. One that stands only some of the
    // time (`when`) is left out. Each shape's columns are read once.
    let sometimes: std::collections::HashSet<u32> = w.when.iter().map(|(k, _)| *k).collect();
    let mut columns: std::collections::HashMap<usize, Vec<Col>> = std::collections::HashMap::new();
    for (k, p) in w.placed.iter().enumerate() {
        if sometimes.contains(&(k as u32)) {
            continue;
        }
        let s = &w.shapes[p.shape];
        let cols = columns.entry(p.shape).or_insert_with(|| shape_columns(s, &w.doors));
        for lz in 0..s.size[2] {
            for lx in 0..s.size[0] {
                let col = cols[(lz * s.size[0] + lx) as usize];
                if col.top == 0 && !col.covered {
                    continue;
                }
                let (rx, rz) = super::super::shape::rotate(lx, lz, s.size, p.rot);
                let (x, z) = ((p.origin[0] + rx) as f32 * VOXEL, (p.origin[2] + rz) as f32 * VOXEL);
                let Some(i) = at(x, z) else { continue };
                let base = p.origin[1] as f32 * VOXEL - f.h[i];
                let c = &mut cells[i];
                c.built = c.built.max(base + col.floor as f32 * VOXEL);
                c.tall = c.tall.max(base + col.top as f32 * VOXEL);
                c.roof |= col.covered;
            }
        }
    }
    // Buildings imported from map data: solid to their wall tops, roofed.
    for b in &w.city.buildings {
        let Some(outline) = b.rings.first() else { continue };
        for z in (b.lo.y.max(0.0) as usize)..(b.hi.y.ceil().max(0.0) as usize).min(n1) {
            for x in (b.lo.x.max(0.0) as usize)..(b.hi.x.ceil().max(0.0) as usize).min(n1) {
                let p = glam::Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
                if inside(outline, p) && !b.rings[1..].iter().any(|r| inside(r, p)) {
                    let i = z * n1 + x;
                    cells[i].built = cells[i].built.max(b.wall_top - f.h[i]);
                    cells[i].tall = cells[i].tall.max(b.wall_top - f.h[i]);
                    cells[i].roof = true;
                }
            }
        }
    }

    // Plants. A crown is over every cell within its radius; anything under
    // 3 m is undergrowth.
    let affords: Vec<(bool, bool, bool)> = w
        .lib
        .plants
        .iter()
        .map(|p| {
            let a = crate::creatures::affordance::Affords::from_names(&p.affords);
            (a.has(Affordance::Browse), a.has(Affordance::Nectar), a.has(Affordance::Cover))
        })
        .collect();
    for t in &w.trees {
        if matches!(t.kind, TreeKind::Vine | TreeKind::Crust | TreeKind::Bracket) {
            continue;
        }
        let (browse, nectar, cover) = affords.get(t.plant as usize).copied().unwrap_or((false, false, false));
        if t.trunk_r >= 0.12 {
            if let Some(i) = at(t.pos.x, t.pos.z) {
                cells[i].trunk = true;
            }
        }
        let tall = t.top >= 3.0 && t.canopy_lo >= 1.2;
        let r = t.radius.max(0.5);
        let ri = r.ceil() as i32;
        for dz in -ri..=ri {
            for dx in -ri..=ri {
                let (x, z) = (t.pos.x + dx as f32, t.pos.z + dz as f32);
                if (dx * dx + dz * dz) as f32 > r * r {
                    continue;
                }
                let Some(i) = at(x, z) else { continue };
                let c = &mut cells[i];
                if tall {
                    c.crown_top = c.crown_top.max(t.top);
                    c.crowns = c.crowns.saturating_add(1);
                } else if t.top > 0.5 || cover {
                    c.under = c.under.max(t.top.max(if cover { 1.0 } else { 0.0 }));
                }
                c.browse |= browse;
                c.nectar |= nectar;
            }
        }
    }
    cells
}

/// Headroom a body needs over a floor, voxels (2 m).
const HEADROOM: i32 = (2.0 / VOXEL) as i32;

/// One column of a structure, in voxels over its base.
#[derive(Clone, Copy, Default)]
struct Col {
    /// The floor under the lowest 2 m of headroom: where a body stands in
    /// this column. The top of the column where there is no such gap.
    floor: i32,
    /// The highest solid voxel + 1.
    top: i32,
    /// Something solid over the floor's headroom (a roof, an upper floor, an
    /// awning).
    covered: bool,
}

/// What each (x, z) column of a shape is like to stand in. Roofs are found
/// from the voxels, so a structure need not tag its roof (`part: roof`) to
/// count as covered; cloth covers but is not stood on, and a door leaf
/// (`doors`) is an opening, so a house's ways in are its doors.
fn shape_columns(s: &crate::world::shape::Shape, doors: &[u8]) -> Vec<Col> {
    use crate::world::shape::{CARVE, PART_CLOTH};
    let mut out = vec![Col::default(); (s.size[0] * s.size[2]).max(0) as usize];
    for z in 0..s.size[2] {
        for x in 0..s.size[0] {
            let solid = |y: i32| {
                let (m, p) = s.get(x, y, z);
                m != 0 && m != CARVE && p != PART_CLOTH && !doors.contains(&m)
            };
            let any = |y: i32| {
                let (m, _) = s.get(x, y, z);
                m != 0 && m != CARVE
            };
            let top = (0..s.size[1]).rev().find(|&y| solid(y)).map_or(0, |y| y + 1);
            // The lowest air run of HEADROOM over something (or the base).
            let mut floor = top;
            let mut run = 0;
            for y in 0..s.size[1] {
                if solid(y) {
                    run = 0;
                } else {
                    run += 1;
                    if run == HEADROOM {
                        floor = y + 1 - HEADROOM;
                        break;
                    }
                }
            }
            let covered = floor < top && (floor + HEADROOM..s.size[1]).any(any);
            out[(z * s.size[0] + x) as usize] = Col { floor, top, covered };
        }
    }
    out
}

/// Point in a polygon (even-odd).
pub(super) fn inside(ring: &[glam::Vec2], p: glam::Vec2) -> bool {
    let mut odd = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Distance from the nearest seed cell, in cells (a two-pass chamfer, 1 and
/// 1.4), as bytes; 255 is that far or no seed at all.
pub(super) fn distance(seed: &[bool], n: usize) -> Vec<u8> {
    let mut d: Vec<f32> = seed.iter().map(|&s| if s { 0.0 } else { f32::MAX }).collect();
    let diag = std::f32::consts::SQRT_2;
    for z in 0..n {
        for x in 0..n {
            let k = z * n + x;
            let mut v = d[k];
            if x > 0 {
                v = v.min(d[k - 1] + 1.0);
            }
            if z > 0 {
                v = v.min(d[k - n] + 1.0);
                if x > 0 {
                    v = v.min(d[k - n - 1] + diag);
                }
                if x + 1 < n {
                    v = v.min(d[k - n + 1] + diag);
                }
            }
            d[k] = v;
        }
    }
    for z in (0..n).rev() {
        for x in (0..n).rev() {
            let k = z * n + x;
            let mut v = d[k];
            if x + 1 < n {
                v = v.min(d[k + 1] + 1.0);
            }
            if z + 1 < n {
                v = v.min(d[k + n] + 1.0);
                if x + 1 < n {
                    v = v.min(d[k + n + 1] + diag);
                }
                if x > 0 {
                    v = v.min(d[k + n - 1] + diag);
                }
            }
            d[k] = v;
        }
    }
    d.iter().map(|&v| v.round().min(255.0) as u8).collect()
}

/// The mean of a grid over a square of `r` cells either side, from a summed
/// area table; the square is clipped at the world's edge.
pub(super) fn box_mean(v: &[f32], n: usize, r: usize) -> Vec<f32> {
    let mut sat = vec![0f64; (n + 1) * (n + 1)];
    for z in 0..n {
        let mut row = 0f64;
        for x in 0..n {
            row += v[z * n + x] as f64;
            sat[(z + 1) * (n + 1) + x + 1] = sat[z * (n + 1) + x + 1] + row;
        }
    }
    let mut out = vec![0f32; n * n];
    for z in 0..n {
        let (z0, z1) = (z.saturating_sub(r), (z + r + 1).min(n));
        for x in 0..n {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(n));
            let s = sat[z1 * (n + 1) + x1] - sat[z0 * (n + 1) + x1] - sat[z1 * (n + 1) + x0] + sat[z0 * (n + 1) + x0];
            out[z * n + x] = (s / ((z1 - z0) * (x1 - x0)) as f64) as f32;
        }
    }
    out
}

/// Bearings openness is measured along, and how far, metres.
const BEARINGS: usize = 16;
const OPEN_REACH: f32 = 100.0;
const OPEN_STEP: f32 = 2.0;
pub(super) const EYE: f32 = 1.6;
/// How much a crown blocks the view through it, 0 to 1.
const CROWN_BLOCKS: f32 = 0.6;

/// The heights in the way, on a 2 m grid: the solid top (ground and what
/// is built) and the crown top.
pub(super) struct Obstruction {
    pub m: usize,
    pub solid: Vec<f32>,
    pub crown: Vec<f32>,
}

impl Obstruction {
    /// The 2 m cell under a world point, if it is in the world.
    pub fn at(&self, x: f32, z: f32) -> Option<usize> {
        let (gx, gz) = ((x / 2.0).floor(), (z / 2.0).floor());
        (gx >= 0.0 && gz >= 0.0 && (gx as usize) < self.m && (gz as usize) < self.m).then(|| gz as usize * self.m + gx as usize)
    }
}

fn obstruction(w: &World, cells: &[Cell]) -> Obstruction {
    let f = &w.f;
    let n1 = f.n;
    let m = n1.div_ceil(2);
    let mut solid = vec![f32::MIN; m * m];
    let mut crown = vec![f32::MIN; m * m];
    for z in 0..n1 {
        for x in 0..n1 {
            let i = z * n1 + x;
            let k = (z / 2) * m + x / 2;
            let c = &cells[i];
            solid[k] = solid[k].max(f.h[i] + c.tall.max(0.0));
            if c.crowns > 0 {
                crown[k] = crown[k].max(f.h[i] + c.crown_top);
            }
        }
    }
    Obstruction { m, solid, crown }
}

/// How open the horizon is from eye height at each survey cell's middle:
/// over 16 bearings to 100 m, one less the steepest angle up to anything in
/// the way as a share of straight up, crowns counted at `CROWN_BLOCKS`;
/// then dimmed under a crown or a roof.
fn openness(w: &World, cells: &[Cell], o: &Obstruction, n: usize) -> Vec<u8> {
    let f = &w.f;
    let n1 = f.n;
    let (m, solid, crown) = (o.m, &o.solid, &o.crown);
    let dirs: Vec<(f32, f32)> = (0..BEARINGS).map(|b| (b as f32 / BEARINGS as f32 * std::f32::consts::TAU).sin_cos()).collect();
    let steps = (OPEN_REACH / OPEN_STEP) as usize;
    let mut out = vec![0u8; n * n];
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).clamp(1, 16);
    let rows_per = n.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (band, rows) in out.chunks_mut(rows_per * n).enumerate() {
            let dirs = &dirs;
            sc.spawn(move || {
                let look = |x: f32, z: f32| -> Option<usize> {
                    let (gx, gz) = ((x / 2.0).floor(), (z / 2.0).floor());
                    (gx >= 0.0 && gz >= 0.0 && (gx as usize) < m && (gz as usize) < m).then(|| gz as usize * m + gx as usize)
                };
                for (j, o) in rows.iter_mut().enumerate() {
                    let (cz, cx) = (band * rows_per + j / n, j % n);
                    let (x, z) = ((cx as f32 + 0.5) * CELL_M, (cz as f32 + 0.5) * CELL_M);
                    let i = ((z / CELL) as usize).min(n1 - 1) * n1 + ((x / CELL) as usize).min(n1 - 1);
                    let here = &cells[i];
                    // Stand on what is built here (a deck, a floor), else the ground.
                    let floor = f.h[i] + if here.built <= PERSON.step { here.built.max(0.0) } else { 0.0 };
                    let eye = floor.max(f.sea) + EYE;
                    let mut open = 0.0;
                    for &(s, c) in dirs {
                        let mut steep = 0f32;
                        for k in 1..=steps {
                            let d = k as f32 * OPEN_STEP;
                            let Some(g) = look(x + s * d, z + c * d) else { break };
                            let a = ((solid[g] - eye) / d).atan();
                            let b = ((crown[g] - eye) / d).atan() * CROWN_BLOCKS;
                            steep = steep.max(a).max(b);
                        }
                        open += 1.0 - steep.max(0.0) / std::f32::consts::FRAC_PI_2;
                    }
                    let mut v = open / BEARINGS as f32;
                    if here.crowns > 0 {
                        v *= 1.0 - CROWN_BLOCKS * 0.7;
                    }
                    if here.roof {
                        v *= 0.15;
                    }
                    *o = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_cells_from_the_nearest_seed() {
        let n = 5;
        let mut seed = vec![false; n * n];
        seed[0] = true;
        let d = distance(&seed, n);
        assert_eq!(d[0], 0);
        assert_eq!(d[4], 4);
        assert_eq!(d[n * 4], 4);
        assert_eq!(d[n + 1], 1);
        assert_eq!(distance(&[false; 4], 2), vec![255; 4]);
    }

    #[test]
    fn box_mean_averages_the_clipped_square() {
        let v = [1.0, 2.0, 3.0, 4.0];
        let m = box_mean(&v, 2, 1);
        assert!(m.iter().all(|&x| (x - 2.5).abs() < 1e-6));
        assert_eq!(box_mean(&v, 2, 0), v.to_vec());
    }

    #[test]
    fn a_point_is_inside_a_square_and_not_beside_it() {
        let sq = [glam::Vec2::new(0.0, 0.0), glam::Vec2::new(2.0, 0.0), glam::Vec2::new(2.0, 2.0), glam::Vec2::new(0.0, 2.0)];
        assert!(inside(&sq, glam::Vec2::new(1.0, 1.0)));
        assert!(!inside(&sq, glam::Vec2::new(3.0, 1.0)));
    }
}
