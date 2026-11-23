//! A planar street graph: nodes, and straight segments between them that
//! never cross without a node where they meet.
//!
//! Every street a layout draws goes in through `insert_line` (a line that
//! cuts through whatever it crosses, as a surveyed grid does) or is grown a
//! segment at a time and stopped at the first street it meets (`hits`), as a
//! lane is (`grow::grow_one`). Either way the graph stays planar, so a crossing is always a junction and two streets
//! never lie on top of each other.

use crate::util::FxHashMap as HashMap;
pub use crate::world::chronicle::geom::seg_foot;
use crate::world::chronicle::geom::cross_params;
use glam::Vec2;

/// Bucket size of the segment index, metres.
const B: f32 = 24.0;

#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: u32,
    pub b: u32,
    pub rank: u8,
    pub district: u8,
    pub alive: bool,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Vec2>,
    pub segs: Vec<Seg>,
    /// Nodes a layout means to be the end of a street (a close's turning
    /// circle); pruning leaves them.
    pub ends: Vec<u32>,
    /// The middle of every water crossing laid so far, so the next street
    /// to reach the river crosses it only if no bridge is near.
    pub bridges: Vec<Vec2>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn key(v: f32) -> i32 {
    (v / B).floor() as i32
}

/// Where segments ab and cd cross: (t along ab, u along cd).
pub fn cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f32, f32)> {
    cross_params(a, b, c, d, 1e-6)
}

impl Graph {
    fn cells(lo: Vec2, hi: Vec2) -> impl Iterator<Item = (i32, i32)> {
        let (x0, x1, z0, z1) = (key(lo.x), key(hi.x), key(lo.y), key(hi.y));
        (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (x, z)))
    }

    pub fn ends_of(&self, k: usize) -> (Vec2, Vec2) {
        let s = &self.segs[k];
        (self.nodes[s.a as usize], self.nodes[s.b as usize])
    }

    fn index(&mut self, k: usize) {
        let (a, b) = self.ends_of(k);
        for c in Self::cells(a.min(b), a.max(b)) {
            self.grid.entry(c).or_default().push(k as u32);
        }
    }

    /// Live segments whose buckets touch the box.
    pub fn near(&self, lo: Vec2, hi: Vec2) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for c in Self::cells(lo, hi) {
            if let Some(l) = self.grid.get(&c) {
                out.extend(l.iter().map(|&k| k as usize).filter(|&k| self.segs[k].alive));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn add_node(&mut self, p: Vec2) -> u32 {
        self.nodes.push(p);
        (self.nodes.len() - 1) as u32
    }

    /// A node on a live segment within `r` of p, nearest first.
    pub fn node_near(&self, p: Vec2, r: f32) -> Option<u32> {
        let mut best = None;
        let mut bd = r;
        for k in self.near(p - Vec2::splat(r), p + Vec2::splat(r)) {
            let s = self.segs[k];
            for n in [s.a, s.b] {
                let d = self.nodes[n as usize].distance(p);
                if d < bd {
                    bd = d;
                    best = Some(n);
                }
            }
        }
        best
    }

    pub fn node_at(&mut self, p: Vec2, r: f32) -> u32 {
        self.node_near(p, r).unwrap_or_else(|| self.add_node(p))
    }

    pub fn add_seg(&mut self, a: u32, b: u32, rank: u8, district: u8) -> Option<usize> {
        if a == b {
            return None;
        }
        let (pa, pb) = (self.nodes[a as usize], self.nodes[b as usize]);
        for k in self.near(pa.min(pb), pa.max(pb)) {
            let s = self.segs[k];
            if (s.a == a && s.b == b) || (s.a == b && s.b == a) {
                return None;
            }
        }
        self.segs.push(Seg { a, b, rank, district, alive: true });
        let k = self.segs.len() - 1;
        self.index(k);
        Some(k)
    }

    /// Split segment k at p, returning the node there (an end, when p is
    /// within a metre of one).
    pub fn split(&mut self, k: usize, p: Vec2) -> u32 {
        let s = self.segs[k];
        let (a, b) = self.ends_of(k);
        if p.distance(a) < 1.0 {
            return s.a;
        }
        if p.distance(b) < 1.0 {
            return s.b;
        }
        self.segs[k].alive = false;
        let n = self.add_node(p);
        self.add_seg(s.a, n, s.rank, s.district);
        self.add_seg(n, s.b, s.rank, s.district);
        n
    }

    /// The nearest live segment within `r` of p that `skip` doesn't refuse:
    /// (segment, distance, nearest point).
    pub fn nearest(&self, p: Vec2, r: f32, skip: impl Fn(usize) -> bool) -> Option<(usize, f32, Vec2)> {
        let mut best: Option<(usize, f32, Vec2)> = None;
        for k in self.near(p - Vec2::splat(r), p + Vec2::splat(r)) {
            if skip(k) {
                continue;
            }
            let (a, b) = self.ends_of(k);
            let (d, f, _) = seg_foot(a, b, p);
            if d <= r && best.is_none_or(|x| d < x.1) {
                best = Some((k, d, f));
            }
        }
        best
    }

    /// Crossings of p->q with live segments that don't touch node `from`,
    /// nearest first: (t along pq, segment, point).
    pub fn hits(&self, p: Vec2, q: Vec2, from: Option<u32>) -> Vec<(f32, usize, Vec2)> {
        let mut out = Vec::new();
        for k in self.near(p.min(q), p.max(q)) {
            let s = self.segs[k];
            if from.is_some_and(|n| s.a == n || s.b == n) {
                continue;
            }
            let (a, b) = self.ends_of(k);
            if let Some((t, _)) = cross(p, q, a, b) {
                if t > 1e-3 {
                    out.push((t, k, p + (q - p) * t));
                }
            }
        }
        out.sort_by(|x, y| x.0.total_cmp(&y.0));
        out
    }

    /// Is p within `gap` of a segment running roughly along `dir`? A street
    /// laid there would be a second carriageway beside the first.
    pub fn crowded(&self, p: Vec2, dir: Vec2, gap: f32) -> bool {
        for k in self.near(p - Vec2::splat(gap), p + Vec2::splat(gap)) {
            let (a, b) = self.ends_of(k);
            let (d, _, _) = seg_foot(a, b, p);
            if d < gap && (b - a).normalize_or_zero().dot(dir).abs() > 0.82 {
                return true;
            }
        }
        false
    }

    /// Lay a line that cuts through whatever it crosses, splitting it there.
    /// Each end reaches on up to `extend` m to meet a street just beyond it,
    /// so a grid line clipped short of a main road still joins it.
    pub fn insert_line(&mut self, pts: &[Vec2], rank: u8, district: u8, extend: f32) {
        if pts.len() < 2 {
            return;
        }
        let reach = |g: &mut Graph, end: Vec2, back: Vec2| -> Option<u32> {
            let d = (end - back).normalize_or_zero();
            if let Some(&(_, k, x)) = g.hits(end, end + d * extend, None).first() {
                return Some(g.split(k, x));
            }
            g.node_near(end, (extend * 0.4).max(2.0))
        };
        let start = reach(self, pts[0], pts[1]);
        let n = pts.len();
        let end = reach(self, pts[n - 1], pts[n - 2]);
        let mut cur = match start {
            Some(s) => {
                // The reach goes in as its own piece, so the line meets the
                // street it found.
                let first = self.node_at(pts[0], 0.5);
                if first != s {
                    self.add_seg(s, first, rank, district);
                }
                first
            }
            None => self.node_at(pts[0], 1.5),
        };
        for (i, &target) in pts.iter().enumerate().skip(1) {
            let last = i == n - 1;
            for _ in 0..64 {
                let p = self.nodes[cur as usize];
                if p.distance(target) < 0.3 {
                    break;
                }
                let hit = self.hits(p, target, Some(cur)).first().copied();
                match hit {
                    Some((_, k, x)) => {
                        let m = self.split(k, x);
                        if m == cur {
                            break;
                        }
                        self.add_seg(cur, m, rank, district);
                        cur = m;
                    }
                    None => {
                        let m = if last { self.node_at(target, 1.0) } else { self.add_node(target) };
                        self.add_seg(cur, m, rank, district);
                        cur = m;
                        break;
                    }
                }
            }
        }
        if let Some(e) = end {
            if e != cur {
                self.add_seg(cur, e, rank, district);
            }
        }
    }

    /// Degree of every node over live segments.
    pub fn degrees(&self) -> Vec<u32> {
        let mut d = vec![0u32; self.nodes.len()];
        for s in self.segs.iter().filter(|s| s.alive) {
            d[s.a as usize] += 1;
            d[s.b as usize] += 1;
        }
        d
    }

    fn adjacency(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.nodes.len()];
        for (k, s) in self.segs.iter().enumerate().filter(|(_, s)| s.alive) {
            adj[s.a as usize].push(k);
            adj[s.b as usize].push(k);
        }
        adj
    }

    /// For each node, whether a layout meant it to be the end of a street
    /// (`ends`).
    pub fn meant_ends(&self) -> Vec<bool> {
        let mut meant = vec![false; self.nodes.len()];
        for &e in &self.ends {
            meant[e as usize] = true;
        }
        meant
    }

    /// Take off dead ends shorter than `min` m (stubs left where a line ran
    /// past the street it was meant to meet), except the ends a layout meant.
    pub fn prune(&mut self, min: f32) {
        let meant = self.meant_ends();
        for _ in 0..8 {
            let adj = self.adjacency();
            let deg = self.degrees();
            let mut killed = false;
            for n in 0..self.nodes.len() {
                if deg[n] != 1 || meant[n] {
                    continue;
                }
                // Walk to the first junction.
                let mut chain = Vec::new();
                let (mut at, mut via) = (n, adj[n][0]);
                let mut len = 0.0;
                loop {
                    if chain.contains(&via) {
                        break;
                    }
                    chain.push(via);
                    let s = self.segs[via];
                    let next = if s.a as usize == at { s.b as usize } else { s.a as usize };
                    len += self.nodes[at].distance(self.nodes[next]);
                    at = next;
                    if deg[at] != 2 || len > min {
                        break;
                    }
                    via = adj[at].iter().copied().find(|&k| k != via).unwrap_or(via);
                }
                if len < min {
                    for k in chain {
                        self.segs[k].alive = false;
                    }
                    killed = true;
                }
            }
            if !killed {
                break;
            }
        }
    }

    /// The streets as polylines: runs of segments of one rank through nodes
    /// where nothing else joins. Each is (points, rank, district).
    pub fn ways(&self) -> Vec<(Vec<Vec2>, u8, u8)> {
        let adj = self.adjacency();
        let mut seen = vec![false; self.segs.len()];
        let mut out = Vec::new();
        let passes = |n: usize, via: usize, rank: u8| -> Option<usize> {
            let l: Vec<usize> = adj[n].iter().copied().filter(|&k| k != via).collect();
            (l.len() == 1 && adj[n].len() == 2 && self.segs[l[0]].rank == rank).then(|| l[0])
        };
        for k0 in 0..self.segs.len() {
            if !self.segs[k0].alive || seen[k0] {
                continue;
            }
            seen[k0] = true;
            let s0 = self.segs[k0];
            let mut fwd = vec![s0.a as usize, s0.b as usize];
            // forward from b
            let (mut at, mut via) = (s0.b as usize, k0);
            while let Some(k) = passes(at, via, s0.rank) {
                if seen[k] {
                    break;
                }
                seen[k] = true;
                let s = self.segs[k];
                at = if s.a as usize == at { s.b as usize } else { s.a as usize };
                via = k;
                fwd.push(at);
            }
            // backward from a
            let (mut at, mut via) = (s0.a as usize, k0);
            let mut back = Vec::new();
            while let Some(k) = passes(at, via, s0.rank) {
                if seen[k] {
                    break;
                }
                seen[k] = true;
                let s = self.segs[k];
                at = if s.a as usize == at { s.b as usize } else { s.a as usize };
                via = k;
                back.push(at);
            }
            back.reverse();
            back.extend(fwd);
            out.push((back.into_iter().map(|n| self.nodes[n]).collect(), s0.rank, s0.district));
        }
        out
    }

    /// Where a road of rank `to` meets one of rank `from`, the `from` road
    /// becomes `to` back to its first junction: a high street ends at a
    /// crossing, not partway along a road in the open.
    pub fn hand_over(&mut self, from: u8, to: u8) {
        let adj = self.adjacency();
        let deg = self.degrees();
        for n in 0..self.nodes.len() {
            let here: Vec<usize> = adj[n].iter().copied().filter(|&k| self.segs[k].alive).collect();
            if !here.iter().any(|&k| self.segs[k].rank == to) {
                continue;
            }
            for &k0 in &here {
                if self.segs[k0].rank != from {
                    continue;
                }
                let (mut at, mut via) = (n, k0);
                for _ in 0..400 {
                    self.segs[via].rank = to;
                    let s = self.segs[via];
                    at = if s.a as usize == at { s.b as usize } else { s.a as usize };
                    if deg[at] != 2 {
                        break;
                    }
                    match adj[at].iter().copied().find(|&k| k != via && self.segs[k].alive && self.segs[k].rank == from) {
                        Some(k) => via = k,
                        None => break,
                    }
                }
            }
        }
    }

    /// Take off every dead end whose end `outer` says is out at the edge,
    /// whatever its length (but not a road of rank `keep`, nor a close).
    pub fn prune_outer(&mut self, outer: impl Fn(Vec2) -> bool, keep: u8) {
        let meant = self.meant_ends();
        for _ in 0..12 {
            let deg = self.degrees();
            let mut killed = false;
            for k in 0..self.segs.len() {
                let s = self.segs[k];
                if !s.alive || s.rank == keep {
                    continue;
                }
                for n in [s.a, s.b] {
                    if deg[n as usize] == 1 && !meant[n as usize] && outer(self.nodes[n as usize]) {
                        self.segs[k].alive = false;
                        killed = true;
                        break;
                    }
                }
            }
            if !killed {
                break;
            }
        }
    }
}

