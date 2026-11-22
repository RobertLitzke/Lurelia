//! Where water goes. Events that want land shaped the way running water or
//! ice shapes it (`drainage`, `glaciate`) work on a coarse copy of the height
//! field: route every cell downhill to the sea, count what drains through
//! each, cut where a lot does, and hand the change back to the fields.
//!
//! Nothing here is drawn by an author. A river's course, where its
//! tributaries join and at what angle all come from the land's own shape,
//! which is what makes a map read as land from above.

use crate::world::fields::{Fields, CELL};
use crate::world::noise::{hash2, perlin2, smoothstep, unit};
use glam::Vec2;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// No receiver: the cell is an outlet (the sea, or the world's edge).
pub const NONE: u32 = u32::MAX;

/// A coarse copy of the height field, `step` field cells to a side.
pub struct Grid {
    pub n: usize,
    pub step: usize,
    pub h: Vec<f32>,
}

impl Grid {
    /// The block means of the field's heights.
    pub fn from(f: &Fields, step: usize) -> Grid {
        let step = step.max(1);
        let n = f.n / step;
        let mut h = vec![0.0; n * n];
        for z in 0..n {
            for x in 0..n {
                let mut sum = 0.0;
                for dz in 0..step {
                    for dx in 0..step {
                        sum += f.h[(z * step + dz) * f.n + x * step + dx];
                    }
                }
                h[z * n + x] = sum / (step * step) as f32;
            }
        }
        Grid { n, step, h }
    }

    /// Metres to a side of one cell.
    pub fn cell(&self) -> f32 {
        self.step as f32 * CELL
    }

    pub fn centre(&self, i: usize) -> Vec2 {
        Vec2::new((i % self.n) as f32 + 0.5, (i / self.n) as f32 + 0.5) * self.cell()
    }

    pub fn idx(&self, p: Vec2) -> usize {
        let c = self.cell();
        let x = ((p.x / c) as i32).clamp(0, self.n as i32 - 1) as usize;
        let z = ((p.y / c) as i32).clamp(0, self.n as i32 - 1) as usize;
        z * self.n + x
    }

    /// A coarse field read between its cell centres.
    pub fn sample(&self, v: &[f32], p: Vec2) -> f32 {
        let c = self.cell();
        let (gx, gz) = ((p.x / c - 0.5).clamp(0.0, self.n as f32 - 1.0), (p.y / c - 0.5).clamp(0.0, self.n as f32 - 1.0));
        let (x0, z0) = (gx as usize, gz as usize);
        let (x1, z1) = ((x0 + 1).min(self.n - 1), (z0 + 1).min(self.n - 1));
        let (u, w) = (gx - x0 as f32, gz - z0 as f32);
        let a = v[z0 * self.n + x0] * (1.0 - u) + v[z0 * self.n + x1] * u;
        let b = v[z1 * self.n + x0] * (1.0 - u) + v[z1 * self.n + x1] * u;
        a * (1.0 - w) + b * w
    }

    /// Give the fields what changed here since `before`, so the fine detail
    /// the coarse copy never held is kept. Returns the square metres moved
    /// by more than a hand.
    pub fn apply_delta(&self, before: &[f32], f: &mut Fields, cause: u16) -> usize {
        let delta: Vec<f32> = self.h.iter().zip(before).map(|(a, b)| a - b).collect();
        let mut moved = 0;
        for i in 0..f.n * f.n {
            let d = self.sample(&delta, f.centre(i));
            if d.abs() > 1e-3 {
                f.h[i] += d;
                if d.abs() > 0.3 {
                    f.cause[i] = cause;
                    moved += 1;
                }
            }
        }
        moved
    }
}

/// Where every cell drains.
pub struct Flow {
    /// The cell each one drains into, or `NONE`.
    pub recv: Vec<u32>,
    /// Every cell, lowest first: a cell's receiver always comes before it.
    pub order: Vec<u32>,
    /// Square metres draining through each cell, its own included.
    pub area: Vec<f32>,
    /// The height water stands at: the land, or the lip of the hollow it is in.
    pub filled: Vec<f32>,
}

#[derive(PartialEq)]
struct Front(f32, u32);
impl Eq for Front {}
impl Ord for Front {
    fn cmp(&self, o: &Self) -> Ordering {
        // Lowest first out of a max-heap; the index settles ties the same
        // way on every run.
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then(o.1.cmp(&self.1))
    }
}
impl PartialOrd for Front {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Water brought in from beyond the world's edge: a catchment the map does
/// not hold, arriving at one cell.
#[derive(Clone, Copy)]
pub struct Inflow {
    pub at: Vec2,
    pub area: f32,
}

/// Route every cell to the sea (and, with `edges`, off the edge of the
/// world), filling hollows to their lips on the way. The flood works up from
/// the outlets, lowest ground first, so each cell is reached from its lowest
/// neighbour: that neighbour is where it drains.
///
/// `wander` (metres) roughens the surface the water reads, in swells a few
/// strides across. Without it, water on an even slope runs in the grid's
/// eight directions and every stream is a ruled line; with it a stream
/// picks its way, and keeps to the same way each time it is asked.
pub fn route(g: &Grid, sea: f32, edges: bool, seed: u32, wander: f32, inflow: &[Inflow], sinks: &[Vec2]) -> Flow {
    let n = g.n;
    let nn = n * n;
    let cell = g.cell();
    let jit = |i: usize| {
        let (x, z) = ((i % n) as f32 * cell, (i / n) as f32 * cell);
        (perlin2(x * 0.043, z * 0.043, seed) + 0.45 * perlin2(x * 0.12, z * 0.12, seed + 1)) * wander + unit(hash2((i % n) as i32, (i / n) as i32, seed)) * 0.06 * cell
    };
    let mut filled = vec![f32::MAX; nn];
    let mut recv = vec![NONE; nn];
    let mut order: Vec<u32> = Vec::with_capacity(nn);
    let mut heap = BinaryHeap::new();
    let mut any = false;
    // The sea is the water that reaches the edge of the world. A hollow
    // inland that happens to lie below sea level is a lake like any other,
    // and a river does not end in it.
    let mut open = vec![false; nn];
    let mut stack: Vec<usize> = (0..nn)
        .filter(|&i| {
            let (x, z) = (i % n, i / n);
            (x == 0 || z == 0 || x == n - 1 || z == n - 1) && g.h[i] < sea
        })
        .collect();
    for &i in &stack {
        open[i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, z) = (i % n, i / n);
        for q in [
            if x > 0 { i - 1 } else { i },
            if x + 1 < n { i + 1 } else { i },
            if z > 0 { i - n } else { i },
            if z + 1 < n { i + n } else { i },
        ] {
            if !open[q] && g.h[q] < sea {
                open[q] = true;
                stack.push(q);
            }
        }
    }
    // A sink is where water goes into the ground or stands with no way
    // out: a lake in a closed basin above the sea. Rivers end there.
    for s in sinks {
        open[g.idx(*s)] = true;
    }
    // No sea at the edge: any water below sea level will have to do.
    let landlocked = !open.iter().any(|&o| o);
    for i in 0..nn {
        let (x, z) = (i % n, i / n);
        let edge = x == 0 || z == 0 || x == n - 1 || z == n - 1;
        if open[i] || (landlocked && g.h[i] < sea) || (edges && edge) {
            filled[i] = g.h[i];
            heap.push(Front(g.h[i], i as u32));
            any = true;
        }
    }
    if !any {
        // A world with no sea and closed edges still has to drain somewhere:
        // its lowest edge cell.
        let mut low = 0;
        for i in 0..nn {
            let (x, z) = (i % n, i / n);
            if (x == 0 || z == 0 || x == n - 1 || z == n - 1) && g.h[i] < g.h[low] {
                low = i;
            }
        }
        filled[low] = g.h[low];
        heap.push(Front(g.h[low], low as u32));
    }
    let mut done = vec![false; nn];
    while let Some(Front(level, i)) = heap.pop() {
        let i = i as usize;
        if done[i] {
            continue;
        }
        done[i] = true;
        order.push(i as u32);
        let (x, z) = ((i % n) as i32, (i / n) as i32);
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (qx, qz) = (x + dx, z + dz);
            if qx < 0 || qz < 0 || qx >= n as i32 || qz >= n as i32 {
                continue;
            }
            let q = qz as usize * n + qx as usize;
            if filled[q] != f32::MAX {
                continue;
            }
            // A diagonal step is longer, so it has to be that much more
            // downhill to win.
            let reach = if dx != 0 && dz != 0 { 1e-4 * 1.414 } else { 1e-4 };
            filled[q] = (g.h[q] + jit(q)).max(level + reach);
            recv[q] = i as u32;
            heap.push(Front(filled[q], q as u32));
        }
    }
    let cell2 = g.cell() * g.cell();
    let mut area = vec![cell2; nn];
    for s in inflow {
        area[g.idx(s.at)] += s.area;
    }
    for &i in order.iter().rev() {
        let r = recv[i as usize];
        if r != NONE {
            area[r as usize] += area[i as usize];
        }
    }
    Flow { recv, order, area, filled }
}

/// How hard running water cuts.
pub struct Cut {
    /// Passes. Cutting works back from the sea, so more passes reach further
    /// inland.
    pub iters: u32,
    /// The share of the drop to its receiver a cell loses each pass, where
    /// a hectare drains through it.
    pub strength: f32,
    /// How much faster a big stream cuts than a small one (the power on the
    /// drained area; 0.45 is what rivers do).
    pub exponent: f32,
    /// How fast steep valley sides slump back, 0 to 0.25.
    pub diffuse: f32,
    pub edges: bool,
    /// How far streams stray from the straight way down, m (see `route`).
    pub wander: f32,
    /// Square metres that must drain to a spot before the water there cuts
    /// at all. Below it the ground sheds water without channelling it; with
    /// none, every hillside is scored by the runoff down it, in the grid's
    /// own straight lines.
    pub source: f32,
    /// Places water ends at above the sea (see `route`).
    pub sinks: Vec<Vec2>,
}

/// Cut valleys: each pass routes the water again, lowers every cell toward
/// the one it drains into by an amount that grows with what drains through
/// it, and lets the steepest slopes slump. Small streams notch, large ones
/// cut deep, and the valleys meet the way the water does.
pub fn cut(g: &mut Grid, sea: f32, c: &Cut, seed: u32, inflow: &[Inflow]) -> Flow {
    let n = g.n;
    let cell = g.cell();
    for _ in 0..c.iters {
        let flow = route(g, sea, c.edges, seed, c.wander, inflow, &c.sinks);
        for &i in &flow.order {
            let i = i as usize;
            let r = flow.recv[i];
            if r == NONE || g.h[i] < sea {
                continue;
            }
            let r = r as usize;
            // Never below the sea, whatever the sea floor does beyond.
            let base = g.h[r].max(sea - 0.5);
            // Nor below a sink: the cell that drains into one stops at it.
            if flow.recv[r] == NONE && g.h[r] >= sea && g.h[i] <= g.h[r] + 0.2 {
                continue;
            }
            if g.h[i] <= base {
                continue;
            }
            let drained = flow.area[i] - c.source;
            if drained <= 0.0 {
                continue;
            }
            let k = c.strength * (drained / 10_000.0).powf(c.exponent) * (2.0 / cell);
            g.h[i] = (g.h[i] + k * base) / (1.0 + k);
        }
        if c.diffuse > 0.0 {
            let old = g.h.clone();
            for z in 1..n - 1 {
                for x in 1..n - 1 {
                    let i = z * n + x;
                    if old[i] < sea {
                        continue;
                    }
                    let (a, b, cc, d) = (old[i - 1], old[i + 1], old[i - n], old[i + n]);
                    let gx = (b - a) / (2.0 * cell);
                    let gz = (d - cc) / (2.0 * cell);
                    let slope = (gx * gx + gz * gz).sqrt();
                    // Gentle ground keeps its shape; only what stands
                    // steeper than scree moves.
                    let w = c.diffuse * smoothstep(0.55, 1.4, slope);
                    g.h[i] += w * ((a + b + cc + d) * 0.25 - old[i]);
                }
            }
        }
    }
    route(g, sea, c.edges, seed, c.wander, inflow, &c.sinks)
}

/// One stream from its head to where it joins a bigger one (or the sea).
pub struct Reach {
    /// Head first. The last point is on the stream it joins.
    pub pts: Vec<Vec2>,
    /// Square metres draining through each point.
    pub area: Vec<f32>,
    /// Water level at each point before anything was cut.
    pub level: Vec<f32>,
}

impl Reach {
    pub fn length(&self) -> f32 {
        self.pts.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

/// Every stream that at least `min_area` square metres drain through, as
/// lines. At each meeting the bigger stream carries on and the smaller one
/// ends, so the longest line is the trunk and the rest are its tributaries.
/// The lines are eased so they turn in curves, not at the grid's corners.
pub fn reaches(g: &Grid, flow: &Flow, min_area: f32) -> Vec<Reach> {
    let nn = g.n * g.n;
    // Each cell's biggest donor.
    let mut main = vec![NONE; nn];
    for i in 0..nn {
        let r = flow.recv[i];
        if r == NONE || flow.area[i] < min_area {
            continue;
        }
        let r = r as usize;
        if main[r] == NONE || flow.area[main[r] as usize] < flow.area[i] {
            main[r] = i as u32;
        }
    }
    let mut out = Vec::new();
    for head in 0..nn {
        if flow.area[head] < min_area || main[head] != NONE {
            continue;
        }
        let mut cells = vec![head];
        let mut i = head;
        loop {
            let r = flow.recv[i];
            if r == NONE {
                break;
            }
            let r = r as usize;
            cells.push(r);
            if main[r] != i as u32 {
                break;
            }
            i = r;
        }
        if cells.len() < 4 {
            continue;
        }
        let mut pts: Vec<Vec2> = cells.iter().map(|&c| g.centre(c)).collect();
        for _ in 0..3 {
            pts = eased(&pts, (16.0 / g.cell()) as usize);
        }
        out.push(Reach {
            pts,
            area: cells.iter().map(|&c| flow.area[c]).collect(),
            level: cells.iter().map(|&c| flow.filled[c]).collect(),
        });
    }
    out
}

/// A running mean along a line, `half` points either way, the ends held.
pub fn eased(pts: &[Vec2], half: usize) -> Vec<Vec2> {
    let n = pts.len();
    (0..n)
        .map(|k| {
            let w = half.min(k).min(n - 1 - k);
            let mut sum = Vec2::ZERO;
            for p in &pts[k - w..=k + w] {
                sum += *p;
            }
            sum / (2 * w + 1) as f32
        })
        .collect()
}

/// The same for numbers.
pub fn eased_f(v: &[f32], half: usize) -> Vec<f32> {
    let n = v.len();
    (0..n)
        .map(|k| {
            let w = half.min(k).min(n - 1 - k);
            v[k - w..=k + w].iter().sum::<f32>() / (2 * w + 1) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slope(n: usize) -> Grid {
        // Falls to the west, with a hollow in the middle.
        let mut h = vec![0.0; n * n];
        for z in 0..n {
            for x in 0..n {
                let d = ((x as f32 - 20.0).powi(2) + (z as f32 - 16.0).powi(2)).sqrt();
                h[z * n + x] = 30.0 + x as f32 - 12.0 * (1.0 - d / 6.0).max(0.0);
            }
        }
        for z in 0..n {
            h[z * n] = 10.0;
        }
        Grid { n, step: 1, h }
    }

    /// Every cell reaches the sea, hollows included, and nothing is lost on
    /// the way: what arrives at the outlets is the whole map.
    #[test]
    fn all_of_it_reaches_the_sea() {
        let g = slope(32);
        let flow = route(&g, 20.0, false, 1, 0.0, &[], &[]);
        assert_eq!(flow.order.len(), 32 * 32);
        let out: f32 = (0..32 * 32).filter(|&i| flow.recv[i] == NONE).map(|i| flow.area[i]).sum();
        assert_eq!(out, (32 * 32) as f32);
        // The hollow is filled to its lip: water in it stands above its bed.
        let pit = 16 * 32 + 20;
        assert!(flow.filled[pit] > g.h[pit] + 5.0);
        // A receiver is always dealt with first.
        let mut seen = vec![false; 32 * 32];
        for &i in &flow.order {
            let r = flow.recv[i as usize];
            assert!(r == NONE || seen[r as usize]);
            seen[i as usize] = true;
        }
    }

    /// Cutting only lowers land, never the sea floor, and never below the sea.
    #[test]
    fn cutting_lowers_and_stops_at_the_sea() {
        let mut g = slope(32);
        let before = g.h.clone();
        cut(
            &mut g,
            20.0,
            &Cut {
                iters: 8,
                strength: 0.5,
                exponent: 0.45,
                diffuse: 0.0,
                edges: false,
                wander: 0.5,
                source: 0.0,
                sinks: Vec::new(),
            },
            3,
            &[],
        );
        for i in 0..32 * 32 {
            assert!(g.h[i] <= before[i] + 1e-4);
            if before[i] < 20.0 {
                assert_eq!(g.h[i], before[i]);
            } else {
                assert!(g.h[i] >= 19.5 - 1e-3);
            }
        }
    }
}
