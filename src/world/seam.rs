//! Where two worlds meet. A world names the worlds
//! beside it in `world.json`:
//!
//! ```json
//! "neighbours": {"south": {"world": "ribshore", "offset": 0}}
//! ```
//!
//! and is built to meet them. The neighbour is older and never changes; this
//! world takes a strip off the neighbour's edge (`Edge::take`) and, after its
//! own chronicle has run, blends its land into that strip (`blend`), seeds its
//! coast and river distances from the neighbour's (`fix_distances`), and
//! drops any of its own trees that would stand in the neighbour's crowns
//! (`clear_trees`). Standalone, the world then runs to its edge already
//! matching the land on the far side.
//!
//! `offset` slides the neighbour along the shared edge: metres east (for a
//! north or south neighbour) or south (for an east or west one) of this
//! world's corner that the neighbour's corner sits.

use super::fields::{Fields, World};
use super::gen::Tree;
use super::noise::{hash2, smoothstep, unit};
use crate::library::Library;
use glam::Vec2;
use serde::Deserialize;

/// How far into this world the land is blended toward the neighbour's, m.
pub const BAND: f32 = 160.0;
/// How far the neighbour's slope across the edge carries on, m: enough that
/// the seam has no crease, short enough that a dune face doesn't run on as a
/// ramp. The offset it adds peaks at this distance and dies away after it.
const SLOPE_RUN: f32 = 8.0;
/// How far the small shapes of the neighbour's edge row (a dune crest, a
/// bank) carry into this world before only its broad profile is left, m.
/// Without this every ripple on the edge runs on as a ridge across the band.
const DETAIL_RUN: f32 = 10.0;
/// Half the window the edge row is smoothed over for its broad profile, cells.
const BROAD: usize = 20;
/// How far from the edge a neighbour's tree is kept for `clear_trees`, m.
const TREE_MARGIN: f32 = 24.0;

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NeighbourDef {
    pub world: String,
    #[serde(default)]
    pub offset: f32,
}

/// The worlds beside this one, by the side they are on.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Neighbours {
    pub north: Option<NeighbourDef>,
    pub south: Option<NeighbourDef>,
    pub east: Option<NeighbourDef>,
    pub west: Option<NeighbourDef>,
}

impl Neighbours {
    pub fn list(&self) -> Vec<(Side, &NeighbourDef)> {
        [(Side::North, &self.north), (Side::South, &self.south), (Side::East, &self.east), (Side::West, &self.west)]
            .into_iter()
            .filter_map(|(s, d)| d.as_ref().map(|d| (s, d)))
            .collect()
    }
}

/// Which side of this world the neighbour is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::North => "north",
            Side::South => "south",
            Side::East => "east",
            Side::West => "west",
        }
    }

    /// The cell `(x, z)` of a world `n` cells across, seen from this side:
    /// how far along the edge it is and how many cells in from the
    /// neighbour's edge row its centre is (1 for the row next to it).
    fn along_depth(self, n: usize, x: usize, z: usize) -> (usize, f32) {
        match self {
            Side::South => (x, (n - z) as f32),
            Side::North => (x, (z + 1) as f32),
            Side::East => (z, (n - x) as f32),
            Side::West => (z, (x + 1) as f32),
        }
    }

    /// The cell of this world `depth` cells in from the edge (1 the row
    /// next to it) at `along`.
    fn cell(self, n: usize, along: usize, depth: usize) -> (usize, usize) {
        match self {
            Side::South => (along, n - depth),
            Side::North => (along, depth - 1),
            Side::East => (n - depth, along),
            Side::West => (depth - 1, along),
        }
    }

    /// The neighbour's cell `row` rows back from its shared edge (0 the edge
    /// row) at `along` on its own edge, for a neighbour `nn` cells across.
    fn far_cell(self, nn: usize, along: usize, row: usize) -> (usize, usize) {
        match self {
            Side::South => (along, row),
            Side::North => (along, nn - 1 - row),
            Side::East => (row, along),
            Side::West => (nn - 1 - row, along),
        }
    }
}

/// What this world needs from the strip along a neighbour's edge, indexed by
/// this world's cell along the edge. `None` where the neighbour doesn't
/// reach (a smaller world, or one slid along).
pub struct Edge {
    pub side: Side,
    pub world: String,
    /// The neighbour's edge row and the row behind it, for the slope across.
    pub h0: Vec<Option<f32>>,
    pub h1: Vec<Option<f32>>,
    pub moist: Vec<f32>,
    pub cliff: Vec<f32>,
    pub coast: Vec<f32>,
    pub river: Vec<f32>,
    /// Surface override material and region, by name, on the edge row.
    pub surface: Vec<Option<String>>,
    pub region: Vec<Option<String>>,
    pub biome: Vec<String>,
    /// Trees near the edge, in this world's metres: trunk and crown radius.
    pub trees: Vec<(Vec2, f32)>,
    /// A river runs over the edge row here (`Fields::water` 1).
    pub river_cells: Vec<usize>,
}

impl Edge {
    /// The strip off `far`'s edge that faces a world `n` cells across with
    /// sea level `sea`, `far` lying on `side` of it slid `offset` metres.
    /// The neighbour's heights are moved by the difference in sea level.
    pub fn take(far: &World, side: Side, offset: f32, n: usize, sea: f32) -> Edge {
        let f = &far.f;
        let nn = f.n;
        let shift = sea - f.sea;
        let lib = &far.lib;
        let name_of = |id: u8| lib.materials.iter().find(|m| m.id == id).map(|m| m.name.clone());
        let mut e = Edge {
            side,
            world: far.def.name.clone(),
            h0: vec![None; n],
            h1: vec![None; n],
            moist: vec![0.5; n],
            cliff: vec![0.0; n],
            coast: vec![0.0; n],
            river: vec![1e9; n],
            surface: vec![None; n],
            region: vec![None; n],
            biome: vec![String::new(); n],
            trees: Vec::new(),
            river_cells: Vec::new(),
        };
        let off = offset.round() as i64;
        for a in 0..n {
            let fa = a as i64 - off;
            if fa < 0 || fa >= nn as i64 {
                continue;
            }
            let fa = fa as usize;
            let at = |row: usize| {
                let (x, z) = side.far_cell(nn, fa, row);
                z * nn + x
            };
            let (k0, k1) = (at(0), at(1));
            e.h0[a] = Some(f.h[k0] + shift);
            e.h1[a] = Some(f.h[k1] + shift);
            e.moist[a] = f.moist[k0];
            e.cliff[a] = f.cliff[k0];
            e.coast[a] = f.coast_dist[k0];
            e.river[a] = f.river_dist[k0];
            if f.surface[k0] != 0 {
                e.surface[a] = name_of(f.surface[k0]);
            }
            if f.region[k0] != 0 {
                e.region[a] = f.regions.get(f.region[k0] as usize - 1).cloned();
            }
            e.biome[a] = lib.biomes.get(f.biome[k0] as usize).map(|b| b.name.clone()).unwrap_or_default();
            if f.water[k0] == 1 {
                e.river_cells.push(a);
            }
        }
        // The neighbour's trees near the edge, moved into this world's frame.
        let size = n as f32;
        let far_size = f.size;
        for t in &far.trees {
            let p = Vec2::new(t.pos.x, t.pos.z);
            let (back, mine) = match side {
                Side::South => (p.y, Vec2::new(p.x + offset, size + p.y)),
                Side::North => (far_size - p.y, Vec2::new(p.x + offset, p.y - far_size)),
                Side::East => (p.x, Vec2::new(size + p.x, p.y + offset)),
                Side::West => (far_size - p.x, Vec2::new(p.x - far_size, p.y + offset)),
            };
            if back < TREE_MARGIN + t.radius {
                e.trees.push((mine, t.radius));
            }
        }
        e
    }
}

/// What `blend` found before it changed anything: how far this world's own
/// land missed the neighbour's along the edge.
pub struct Mismatch {
    pub max_step: f32,
    pub mean_step: f32,
    /// Cells along the edge where one side is sea and the other land.
    pub coast_cells: usize,
    pub cells: usize,
}

/// Blend this world's land into the neighbour's over `BAND` metres: heights
/// (carrying the neighbour's slope across the edge a little way), wetness,
/// cliffiness, and a dithered handover of surface overrides and regions.
/// Runs after the chronicle and before water settles.
pub fn blend(f: &mut Fields, lib: &Library, e: &Edge, seed: u32) -> Mismatch {
    let n = f.n;
    let rows = (BAND as usize).min(n);
    let mut m = Mismatch {
        max_step: 0.0,
        mean_step: 0.0,
        coast_cells: 0,
        cells: 0,
    };
    // The edge row's broad profile: a moving average over what the
    // neighbour covers.
    let broad: Vec<Option<f32>> = (0..n)
        .map(|a| {
            e.h0[a]?;
            let (lo, hi) = (a.saturating_sub(BROAD), (a + BROAD).min(n - 1));
            let (s, c) = (lo..=hi).filter_map(|k| e.h0[k]).fold((0.0, 0), |(s, c), h| (s + h, c + 1));
            Some(s / c as f32)
        })
        .collect();
    for a in 0..n {
        let (Some(h0), Some(h1), Some(hb)) = (e.h0[a], e.h1[a], broad[a]) else { continue };
        let g = (h0 - h1).clamp(-1.5, 1.5);
        {
            let (x, z) = e.side.cell(n, a, 1);
            let own = f.h[z * n + x];
            let step = (own - (h0 + g * (-1.0 / SLOPE_RUN).exp())).abs();
            m.max_step = m.max_step.max(step);
            m.mean_step += step;
            m.cells += 1;
            if (own >= f.sea) != (h0 >= f.sea) {
                m.coast_cells += 1;
            }
        }
        let surface = e.surface[a].as_deref().and_then(|s| lib.material(s));
        let region = e.region[a].as_deref().map(|r| f.region_id(r));
        for depth in 1..=rows {
            let (x, z) = e.side.cell(n, a, depth);
            let k = z * n + x;
            let d = e.side.along_depth(n, x, z).1;
            let w = smoothstep(0.0, BAND, d);
            let target = hb + (h0 - hb) * (-d / DETAIL_RUN).exp() + g * d * (-d / SLOPE_RUN).exp();
            f.h[k] = target + (f.h[k] - target) * w;
            f.moist[k] = e.moist[a] + (f.moist[k] - e.moist[a]) * w;
            f.cliff[k] = e.cliff[a] + (f.cliff[k] - e.cliff[a]) * w;
            // Overrides hand over cell by cell rather than averaged; only
            // the near half of the band, where the neighbour still leads.
            let keep = unit(hash2(x as i32, z as i32, seed)) < w * 2.0;
            if !keep {
                if let Some(s) = surface {
                    if f.surface[k] == 0 {
                        f.surface[k] = s;
                    }
                }
                if let Some(r) = region {
                    if f.region[k] == 0 {
                        f.region[k] = r;
                    }
                }
            }
        }
    }
    if m.cells > 0 {
        m.mean_step /= m.cells as f32;
    }
    m
}

/// Distances that cross the edge: a cell near it may be nearer the
/// neighbour's coast or river than any of its own. And near the edge the
/// coast distance leans toward the neighbour's own number, which knows
/// nothing of this world's coast: the biomes read it, and the two sides
/// must agree where they meet. Runs after `Fields::derive`.
pub fn fix_distances(f: &mut Fields, e: &Edge) {
    let n = f.n;
    let reach = 600usize.min(n);
    for a in 0..n {
        if e.h0[a].is_none() {
            continue;
        }
        let (c0, r0) = (e.coast[a], e.river[a]);
        for depth in 1..=reach {
            let (x, z) = e.side.cell(n, a, depth);
            let k = z * n + x;
            let d = depth as f32;
            if f.h[k] >= f.sea {
                let via = if c0 > 0.0 { c0 + d } else { d };
                f.coast_dist[k] = f.coast_dist[k].min(via);
            } else {
                let via = if c0 < 0.0 { -c0 + d } else { d };
                f.coast_dist[k] = -(-f.coast_dist[k]).min(via);
            }
            f.river_dist[k] = f.river_dist[k].min(r0 + d);
            // Lean toward the neighbour's number across the band, where
            // both sides agree on land or sea.
            let own = f.coast_dist[k];
            if (own > 0.0) == (c0 > 0.0) && d < BAND {
                let w = smoothstep(0.0, BAND, d);
                f.coast_dist[k] = c0 + (own - c0) * w;
            }
        }
    }
}

/// Drop this world's trees that would stand in a neighbour's crown, so the
/// two never grow through each other when they are drawn side by side.
/// Returns how many went.
pub fn clear_trees(trees: &mut Vec<Tree>, e: &Edge) -> usize {
    if e.trees.is_empty() {
        return 0;
    }
    let before = trees.len();
    trees.retain(|t| {
        let p = Vec2::new(t.pos.x, t.pos.z);
        !e.trees.iter().any(|&(q, r)| p.distance(q) < (r + t.radius) * 0.6)
    });
    before - trees.len()
}

/// `--seam`: how the built world meets each neighbour, a line or two each.
pub fn report(w: &World, e: &Edge) -> String {
    let f = &w.f;
    let n = f.n;
    let (mut worst, mut sum, mut cells, mut coast, mut biome) = (0f32, 0f32, 0usize, 0usize, 0usize);
    let mut runs: Vec<(usize, usize, String, String)> = Vec::new();
    // Runs of cells along the edge where the step is over a metre: first,
    // last, worst.
    let mut steps: Vec<(usize, usize, f32)> = Vec::new();
    for a in 0..n {
        let (Some(h0), Some(h1)) = (e.h0[a], e.h1[a]) else { continue };
        let (x, z) = e.side.cell(n, a, 1);
        let k = z * n + x;
        let step = (f.h[k] - (h0 + (h0 - h1).clamp(-1.5, 1.5) * (-1.0 / SLOPE_RUN).exp())).abs();
        worst = worst.max(step);
        if step > 1.0 {
            match steps.last_mut() {
                Some(r) if r.1 + 1 == a => {
                    r.1 = a;
                    r.2 = r.2.max(step);
                }
                _ => steps.push((a, a, step)),
            }
        }
        sum += step;
        cells += 1;
        if (f.h[k] >= f.sea) != (h0 >= f.sea) {
            coast += 1;
        }
        let mine = w.lib.biomes.get(f.biome[k] as usize).map(|b| b.name.as_str()).unwrap_or("");
        if mine != e.biome[a] {
            biome += 1;
            match runs.last_mut() {
                Some(r) if r.1 + 1 == a && r.2 == mine && r.3 == e.biome[a] => r.1 = a,
                _ => runs.push((a, a, mine.to_string(), e.biome[a].clone())),
            }
        }
    }
    let mut s = format!(
        "{} edge, against {}: {} cells; height step worst {:.2} m, mean {:.3} m; coast disagrees on {} cells; biome differs on {} cells",
        e.side.name(),
        e.world,
        cells,
        worst,
        sum / cells.max(1) as f32,
        coast,
        biome
    );
    for r in &steps {
        s.push_str(&format!("\n  {}..{}: steps up to {:.2} m", r.0, r.1, r.2));
    }
    for r in runs.iter().filter(|r| r.1 - r.0 >= 4) {
        s.push_str(&format!("\n  {}..{}: {} here, {} there", r.0, r.1, r.2, r.3));
    }
    if !e.river_cells.is_empty() {
        s.push_str(&format!("\n  the neighbour has river water on its edge at {:?}; nothing meets it yet", e.river_cells));
    }
    s
}
