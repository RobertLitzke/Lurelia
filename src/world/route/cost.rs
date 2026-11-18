//! Finding a line across a world.
//!
//! A route is an A\* search over a coarse grid laid on the height field. What
//! it costs to step from one cell to the next is distance times what the land
//! asks of a builder: a slope steeper than the kit will climb, open water, a
//! cliff, ground another event has already taken. A road therefore finds the
//! pass rather than climbing the wall, and a footpath takes the dry side of a
//! marsh — without the chronicle naming either.

use crate::world::fields::Fields;
use glam::Vec2;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// What a line will and will not pay for.
#[derive(Clone, Copy, Debug)]
pub struct Terms {
    /// Grid step, metres. Coarser is faster and smoother; 4 m suits a road.
    pub step: f32,
    /// The steepest grade the line holds before it starts paying, rise over
    /// run (0.08 is a highway, 0.25 a mountain track, 0.6 a footpath).
    pub max_grade: f32,
    /// What each unit of grade over the limit costs, as a multiple of distance.
    pub climb: f32,
    /// Multiple of distance for crossing open water, or none to refuse it.
    pub water: Option<f32>,
    /// The deepest water the line will simply walk through, metres. Shallow
    /// enough and a crossing is a ford, not a bridge: the track goes into the
    /// water and comes out the far side, which across a delta in flood is
    /// what a track does. 0 refuses to wade at all.
    pub wade: f32,
    /// The longest unsupported crossing, metres: water or a gap wider than
    /// this is refused however much it would pay.
    pub max_span: f32,
    /// Multiple of distance for ground an earlier event took (a building pad,
    /// another route). 1.0 ignores it.
    pub taken: f32,
    /// How much the line prefers to stay level with its surroundings, which
    /// keeps it off ridge tops and out of gullies.
    pub cliff: f32,
    /// Water does not climb. Above 0 the search is a watercourse: only the
    /// rise of a step is paid for (at this much a metre), falling is free,
    /// and a step that climbs more than `max_climb` is refused outright. The
    /// line that comes back is one water could take.
    pub downhill: f32,
    /// The most a watercourse may climb in one step, metres: the slack that
    /// lets a channel cross a bar or a levee instead of giving up.
    pub max_climb: f32,
}

impl Default for Terms {
    fn default() -> Terms {
        Terms {
            step: 4.0,
            max_grade: 0.12,
            climb: 14.0,
            water: Some(6.0),
            wade: 0.0,
            max_span: 60.0,
            taken: 2.5,
            cliff: 3.0,
            downhill: 0.0,
            max_climb: 0.4,
        }
    }
}

struct Node(f32, u32);

impl PartialEq for Node {
    fn eq(&self, o: &Node) -> bool {
        self.0 == o.0
    }
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Node) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Node) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// The coarse grid a search runs on, sampled from the fields once.
pub struct Grid {
    pub step: f32,
    pub n: usize,
    h: Vec<f32>,
    /// Open water (a river or a lake), per cell.
    wet: Vec<bool>,
    /// How deep the water over a wet cell is, metres.
    depth: Vec<f32>,
    cliff: Vec<f32>,
    taken: Vec<bool>,
}

impl Grid {
    pub fn new(f: &Fields, step: f32) -> Grid {
        let n = ((f.size / step).ceil() as usize).max(2);
        let mut g = Grid {
            step,
            n,
            h: vec![0.0; n * n],
            wet: vec![false; n * n],
            depth: vec![0.0; n * n],
            cliff: vec![0.0; n * n],
            taken: vec![false; n * n],
        };
        for z in 0..n {
            for x in 0..n {
                let p = Vec2::new((x as f32 + 0.5) * step, (z as f32 + 0.5) * step);
                let i = f.idx(p.x, p.y);
                let k = z * n + x;
                g.h[k] = f.h[i];
                g.wet[k] = f.water[i] == 1 || f.h[i] < f.sea;
                g.depth[k] = (f.sea - f.h[i]).max(0.0);
                g.cliff[k] = f.cliff[i];
                g.taken[k] = f.occupied[i];
            }
        }
        g
    }

    #[inline]
    fn at(&self, p: Vec2) -> usize {
        let x = (p.x / self.step).floor().clamp(0.0, self.n as f32 - 1.0) as usize;
        let z = (p.y / self.step).floor().clamp(0.0, self.n as f32 - 1.0) as usize;
        z * self.n + x
    }

    #[inline]
    fn centre(&self, k: usize) -> Vec2 {
        Vec2::new(((k % self.n) as f32 + 0.5) * self.step, ((k / self.n) as f32 + 0.5) * self.step)
    }

    /// Mark a line's own ground as taken, so the next route prefers to join it
    /// at a junction rather than run alongside it.
    pub fn take(&mut self, line: &[Vec2], width: f32) {
        for w in line.windows(2) {
            let steps = (w[0].distance(w[1]) / self.step).ceil().max(1.0) as usize;
            for s in 0..=steps {
                let p = w[0].lerp(w[1], s as f32 / steps as f32);
                let r = (width / self.step).ceil() as i32;
                let k = self.at(p);
                let (cx, cz) = ((k % self.n) as i32, (k / self.n) as i32);
                for dz in -r..=r {
                    for dx in -r..=r {
                        let (x, z) = (cx + dx, cz + dz);
                        if x >= 0 && z >= 0 && (x as usize) < self.n && (z as usize) < self.n {
                            self.taken[z as usize * self.n + x as usize] = true;
                        }
                    }
                }
            }
        }
    }
}

/// What a step from one cell to the next costs, or none when it is refused.
fn step_cost(g: &Grid, from: usize, to: usize, t: &Terms) -> Option<f32> {
    let d = g.centre(from).distance(g.centre(to));
    // A wet or missing cell is a crossing: priced per metre, refused outright
    // when the kit cannot span that far.
    if g.wet[to] && t.downhill > 0.0 {
        return Some(d * 0.8);
    }
    if g.wet[to] {
        // Shallow enough to wade: a ford, priced as slow going rather than as
        // a crossing to be built.
        if t.wade > 0.0 && g.depth[to] <= t.wade {
            return Some(d * 3.0);
        }
        let w = t.water?;
        if g.step > t.max_span {
            return None;
        }
        return Some(d * w);
    }
    // A watercourse pays only for climbing, and refuses to climb far.
    if t.downhill > 0.0 {
        let rise = g.h[to] - g.h[from];
        if rise > t.max_climb {
            return None;
        }
        let mut c = d + rise.max(0.0) * t.downhill;
        if g.taken[to] {
            // Channels join: running into one already cut is what water does.
            c *= t.taken;
        }
        return Some(c);
    }
    let rise = (g.h[to] - g.h[from]).abs();
    let grade = rise / d.max(0.1);
    let over = (grade - t.max_grade).max(0.0);
    let mut c = d * (1.0 + over * t.climb);
    c += g.cliff[to] * t.cliff * d;
    if g.taken[to] {
        c *= t.taken;
    }
    Some(c)
}

/// A line of points from `a` to `b` on the grid, or none when nothing joins
/// them under these terms. The points are grid centres, about `step` m apart.
pub fn find(g: &Grid, a: Vec2, b: Vec2, t: &Terms) -> Option<Vec<Vec2>> {
    let (start, goal) = (g.at(a), g.at(b));
    if start == goal {
        return Some(vec![a, b]);
    }
    // The two ends are where the chronicle says they are, so the search may
    // always stand on them: a landing, a ford and a jetty all begin on ground
    // a line would otherwise refuse.
    let n = g.n;
    let mut best = vec![f32::MAX; n * n];
    let mut from = vec![u32::MAX; n * n];
    let mut heap = BinaryHeap::new();
    let gc = g.centre(goal);
    let h = |k: usize| g.centre(k).distance(gc);
    best[start] = 0.0;
    heap.push(Node(h(start), start as u32));
    // The search is bounded: a world is at most a few hundred cells across,
    // and a route that has looked at every cell twice is not going to find one.
    let mut seen = 0usize;
    while let Some(Node(_, k)) = heap.pop() {
        let k = k as usize;
        if k == goal {
            break;
        }
        seen += 1;
        if seen > n * n * 2 {
            break;
        }
        let (x, z) = ((k % n) as i32, (k / n) as i32);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx as usize >= n || nz as usize >= n {
                continue;
            }
            let m = nz as usize * n + nx as usize;
            let Some(c) = (if m == goal { step_cost(g, k, m, t).or(Some(g.step * 8.0)) } else { step_cost(g, k, m, t) }) else {
                continue;
            };
            let cost = best[k] + c;
            if cost < best[m] {
                best[m] = cost;
                from[m] = k as u32;
                heap.push(Node(cost + h(m), m as u32));
            }
        }
    }
    if from[goal] == u32::MAX {
        return None;
    }
    let mut out = vec![b];
    let mut k = goal;
    while k != start {
        out.push(g.centre(k));
        k = from[k] as usize;
    }
    out.push(a);
    out.reverse();
    Some(out)
}

/// Drop the points that say nothing: the line is kept within `tol` m of the
/// one found (Douglas-Peucker), so a straight run across a plain becomes two
/// points and a pass keeps its bends.
pub fn simplify(line: &[Vec2], tol: f32) -> Vec<Vec2> {
    if line.len() < 3 {
        return line.to_vec();
    }
    let (a, b) = (line[0], line[line.len() - 1]);
    let ab = b - a;
    let l2 = ab.length_squared().max(1e-6);
    let mut worst = (0.0f32, 0usize);
    for (k, p) in line.iter().enumerate().skip(1).take(line.len() - 2) {
        let t = ((*p - a).dot(ab) / l2).clamp(0.0, 1.0);
        let d = p.distance(a + ab * t);
        if d > worst.0 {
            worst = (d, k);
        }
    }
    if worst.0 <= tol {
        return vec![a, b];
    }
    let mut out = simplify(&line[..=worst.1], tol);
    out.pop();
    out.extend(simplify(&line[worst.1..], tol));
    out
}
