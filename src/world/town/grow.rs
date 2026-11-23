//! Laying out streets: the main roads in from the country, and each
//! district's pattern.

use super::graph::{seg_foot, Graph};
pub use crate::world::route::cost::simplify;
use super::{rank, Districts, Ground, Rng, Spec, Style};
use crate::world::noise::perlin2;
use glam::Vec2;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Grid step of the main-road search, metres.
const STEP: f32 = 5.0;

/// The roads from each gate to the centre, found across the land: a road
/// climbs as little as it can, crosses water where it is narrowest, and
/// joins a road already found rather than running beside it, so the roads
/// gather into a few before they reach the middle.
pub fn main_roads(c: Vec2, gates: &[Vec2], ground: &dyn Ground, seed: u32) -> Vec<Vec<Vec2>> {
    let size = ground.size();
    let mut lo = c;
    let mut hi = c;
    for g in gates {
        lo = lo.min(*g);
        hi = hi.max(*g);
    }
    lo = (lo - Vec2::splat(120.0)).max(Vec2::splat(STEP));
    hi = (hi + Vec2::splat(120.0)).min(Vec2::splat(size - STEP));
    let nx = ((hi.x - lo.x) / STEP) as usize + 1;
    let nz = ((hi.y - lo.y) / STEP) as usize + 1;
    let at = |x: usize, z: usize| lo + Vec2::new(x as f32, z as f32) * STEP;
    let hs: Vec<f32> = (0..nz).flat_map(|z| (0..nx).map(move |x| (x, z))).map(|(x, z)| ground.height(at(x, z))).collect();
    let wet: Vec<bool> = (0..nz).flat_map(|z| (0..nx).map(move |x| (x, z))).map(|(x, z)| ground.wet(at(x, z))).collect();
    let mut used = vec![false; nx * nz];
    let cell = |p: Vec2| -> usize {
        let x = (((p.x - lo.x) / STEP).round() as i64).clamp(0, nx as i64 - 1) as usize;
        let z = (((p.y - lo.y) / STEP).round() as i64).clamp(0, nz as i64 - 1) as usize;
        z * nx + x
    };
    let goal = cell(c);
    let mut out = Vec::new();
    for gate in gates {
        let start = cell(*gate);
        let mut dist = vec![f32::INFINITY; nx * nz];
        let mut prev = vec![u32::MAX; nx * nz];
        let mut heap = BinaryHeap::new();
        dist[start] = 0.0;
        heap.push(Reverse((0u32, start as u32)));
        let gp = at(goal % nx, goal / nx);
        while let Some(Reverse((_, i))) = heap.pop() {
            let i = i as usize;
            if i == goal {
                break;
            }
            let (x, z) = (i % nx, i / nx);
            for (dx, dz) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (xx, zz) = (x as i32 + dx, z as i32 + dz);
                if xx < 0 || zz < 0 || xx >= nx as i32 || zz >= nz as i32 {
                    continue;
                }
                let j = zz as usize * nx + xx as usize;
                let len = STEP * if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
                let grade = (hs[j] - hs[i]).abs() / len;
                let mut cost = len * (1.0 + 60.0 * grade * grade + if grade > 0.18 { 30.0 } else { 0.0 });
                if wet[j] {
                    cost += len * 14.0;
                }
                // A little wander, so a road over flat land isn't a ruler line.
                let p = at(xx as usize, zz as usize);
                cost *= 1.0 + 0.25 * (perlin2(p.x * 0.012, p.y * 0.012, seed ^ 0x77) + 1.0);
                if used[j] {
                    cost *= 0.35;
                }
                let nd = dist[i] + cost;
                if nd < dist[j] {
                    dist[j] = nd;
                    prev[j] = i as u32;
                    let h = at(xx as usize, zz as usize).distance(gp);
                    heap.push(Reverse(((nd + h) as u32, j as u32)));
                }
            }
        }
        let mut path = Vec::new();
        let mut i = goal;
        while i != start && prev[i] != u32::MAX {
            path.push(at(i % nx, i / nx));
            used[i] = true;
            i = prev[i] as usize;
        }
        path.push(*gate);
        path.reverse();
        // Smooth the staircase of grid steps into a road.
        let simple = simplify(&path, 4.0);
        out.push(resample(&chaikin(&simple, 3), 6.0));
    }
    out
}

pub fn chaikin(pts: &[Vec2], iters: usize) -> Vec<Vec2> {
    let mut p = pts.to_vec();
    for _ in 0..iters {
        if p.len() < 3 {
            break;
        }
        let mut q = vec![p[0]];
        for w in p.windows(2) {
            q.push(w[0].lerp(w[1], 0.25));
            q.push(w[0].lerp(w[1], 0.75));
        }
        q.push(*p.last().unwrap());
        p = q;
    }
    p
}

/// Points every `step` m along a polyline.
pub fn resample(pts: &[Vec2], step: f32) -> Vec<Vec2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut carry = 0.0;
    for w in pts.windows(2) {
        let len = w[0].distance(w[1]);
        let mut s = step - carry;
        while s < len {
            out.push(w[0].lerp(w[1], s / len));
            s += step;
        }
        carry = len - (s - step);
    }
    let last = *pts.last().unwrap();
    if out.last().unwrap().distance(last) > step * 0.3 {
        out.push(last);
    } else {
        *out.last_mut().unwrap() = last;
    }
    out
}

/// The runs of a line inside `keep`, carried over water where the crossing
/// is at most `span` m and no other bridge stands within 110 m of it: a
/// town has a few bridges, not one per street.
pub fn bridged_runs(bridges: &mut Vec<Vec2>, pts: &[Vec2], ground: &dyn Ground, keep: impl Fn(Vec2) -> bool, span: f32) -> Vec<Vec<Vec2>> {
    let mut out = Vec::new();
    let mut run: Vec<Vec2> = Vec::new();
    let mut k = 0;
    let flush = |run: &mut Vec<Vec2>, out: &mut Vec<Vec<Vec2>>| {
        if run.len() >= 2 {
            out.push(std::mem::take(run));
        } else {
            run.clear();
        }
    };
    while k < pts.len() {
        let p = pts[k];
        if !keep(p) {
            flush(&mut run, &mut out);
            k += 1;
            continue;
        }
        if !ground.wet(p) {
            run.push(p);
            k += 1;
            continue;
        }
        // A wet stretch: find where it ends.
        let mut j = k;
        while j < pts.len() && ground.wet(pts[j]) && keep(pts[j]) {
            j += 1;
        }
        let len = if j < pts.len() && k > 0 { pts[k - 1].distance(pts[j]) } else { f32::MAX };
        let mid = pts[(k + j) / 2];
        if !run.is_empty() && len <= span && bridges.iter().all(|b| b.distance(mid) > 110.0) {
            bridges.push(mid);
            run.extend_from_slice(&pts[k..j]);
            // Land on the far bank even when it is another district's, so a
            // bridge never stops in the river.
            if j < pts.len() && !keep(pts[j]) {
                run.push(pts[j]);
                if j + 1 < pts.len() && !ground.wet(pts[j + 1]) {
                    run.push(pts[j + 1]);
                }
                flush(&mut run, &mut out);
                k = j + 2;
                continue;
            }
        } else {
            flush(&mut run, &mut out);
        }
        k = j;
    }
    flush(&mut run, &mut out);
    out
}

/// A straight street from the centre over the shortest crossing of the
/// water within half the radius, landing 40 m beyond the far bank.
pub fn bridge_street(c0: Vec2, radius: f32, ground: &dyn Ground) -> Option<Vec<Vec2>> {
    // From the centre, or from dry ground near it: a market at a river's
    // mouth has its bridge a little way upstream, where the river narrows.
    let mut origins = vec![c0];
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        for r in [40.0f32, 80.0, 120.0] {
            let o = c0 + Vec2::new(a.cos(), a.sin()) * r;
            if !ground.wet(o) && r < radius * 0.45 {
                origins.push(o);
            }
        }
    }
    let mut best_all: Option<(f32, Vec<Vec2>)> = None;
    for c in origins {
    let mut best: Option<(f32, Vec2, f32)> = None;
    for k in 0..90 {
        let a = k as f32 / 90.0 * std::f32::consts::TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let mut s = 4.0;
        while s < radius * 0.5 && !ground.wet(c + d * s) {
            s += 2.0;
        }
        if s >= radius * 0.5 {
            continue;
        }
        let wet0 = s;
        while s < radius * 0.9 && ground.wet(c + d * s) {
            s += 2.0;
        }
        let span = s - wet0;
        if s >= radius * 0.9 || span > 80.0 {
            continue;
        }
        // Short crossings win, near ones a little more.
        let score = span + wet0 * 0.25;
        if best.is_none_or(|b| score < b.0) {
            best = Some((score, d, s));
        }
    }
    let Some((score, d, s)) = best else { continue };
    let score = score + c.distance(c0) * 0.4;
    let end = s + 40.0;
    if best_all.as_ref().is_none_or(|b| score < b.0) {
        best_all = Some((score, (0..=(end / 4.0) as usize).map(|k| c + d * (k as f32 * 4.0)).collect()));
    }
    }
    best_all.map(|b| b.1)
}

/// The runs of a line not lying alongside a street already there.
pub fn uncrowded(g: &Graph, pts: &[Vec2], gap: f32) -> Vec<Vec<Vec2>> {
    split_runs(pts, |p, dir| !g.crowded(p, dir, gap))
}

fn split_runs(pts: &[Vec2], keep: impl Fn(Vec2, Vec2) -> bool) -> Vec<Vec<Vec2>> {
    let mut out = Vec::new();
    let mut run: Vec<Vec2> = Vec::new();
    for (k, &p) in pts.iter().enumerate() {
        let a = pts[k.saturating_sub(1)];
        let b = pts[(k + 1).min(pts.len() - 1)];
        let dir = (b - a).normalize_or_zero();
        if keep(p, dir) {
            run.push(p);
        } else if run.len() >= 2 {
            out.push(std::mem::take(&mut run));
        } else {
            run.clear();
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

/// Lay out one district.
pub fn fill(g: &mut Graph, dist: &Districts, d: u8, style: Style, ground: &dyn Ground, spec: &Spec, rng: &mut Rng) {
    let big = spec.radius > 260.0;
    match style {
        Style::Grid => {
            // Square to the main road on the sector's near side.
            let angle = grid_angle(g, dist, d);
            let (a, b) = if big { (rng.range(75.0, 95.0), rng.range(50.0, 58.0)) } else { (rng.range(70.0, 90.0), rng.range(44.0, 52.0)) };
            grid(g, dist, d, ground, angle, a, b, spec.features.alleys, rng);
        }
        Style::Radial => radial(g, dist, d, ground, rng),
        Style::Maze => {
            // A small town's plots run back from its main streets to a back
            // lane on each side.
            if spec.radius < 200.0 {
                back_lanes(g, dist, d, ground);
            }
            // A big quarter gets two or three streets through it first,
            // from the ring out to the edge, for the lanes to hang off.
            if d != 0 && big {
                through_streets(g, dist, d, ground, rng);
            }
            grow_fill(
            g,
            dist,
            d,
            ground,
            &Growth {
                block: if big { 19.0 } else { 17.0 },
                step: 5.0,
                wiggle: if d != 0 && big { 0.14 } else { 0.22 },
                cul: 0.12,
                rank: rank::LANE,
                gap: 11.0,
                over: 50.0,
                bulb: false,
                spacing: 14.0,
                limit: usize::MAX,
            },
            rng,
        );
            // Passages through the bigger blocks.
            if spec.features.alleys {
                grow_fill(
                    g,
                    dist,
                    d,
                    ground,
                    &Growth {
                        block: 12.0,
                        step: 3.0,
                        wiggle: 0.12,
                        cul: 0.3,
                        rank: rank::ALLEY,
                        gap: 7.0,
                        over: 10.0,
                        bulb: false,
                        spacing: 9.0,
                        limit: (dist.r_core / 8.0) as usize + 3,
                    },
                    rng,
                );
            }
        }
        Style::Suburb => {
            if dist.radius > 120.0 {
                collectors(g, dist, d, ground, rng);
            }
            grow_fill(
                g,
                dist,
                d,
                ground,
                &Growth {
                    block: 36.0,
                    step: 6.0,
                    wiggle: 0.2,
                    cul: 0.4,
                    rank: rank::CLOSE,
                    gap: 44.0,
                    over: 50.0,
                    bulb: true,
                    spacing: 40.0,
                    limit: usize::MAX,
                },
                rng,
            );
        }
        Style::Ribbon => {}
        Style::Village => grow_fill(
            g,
            dist,
            d,
            ground,
            &Growth {
                block: 38.0,
                step: 6.0,
                wiggle: 0.1,
                cul: 0.35,
                rank: rank::LANE,
                gap: 24.0,
                over: 30.0,
                bulb: false,
                spacing: 30.0,
                limit: usize::MAX,
            },
            rng,
        ),
    }
}

/// The bearing of the main road nearest the middle of sector `d`, radians.
fn grid_angle(g: &Graph, dist: &Districts, d: u8) -> f32 {
    let mid = district_mid(dist, d);
    // Near water the grid runs along the shore, the way a waterfront is
    // laid out.
    if dist.ext.water(mid) < dist.radius * 0.6 {
        let e = 8.0;
        let gx = dist.ext.water(mid + Vec2::new(e, 0.0)) - dist.ext.water(mid - Vec2::new(e, 0.0));
        let gz = dist.ext.water(mid + Vec2::new(0.0, e)) - dist.ext.water(mid - Vec2::new(0.0, e));
        let grad = Vec2::new(gx, gz);
        if grad.length() > 1e-3 && gx.is_finite() && gz.is_finite() {
            return grad.y.atan2(grad.x) + std::f32::consts::FRAC_PI_2;
        }
    }
    let best = g
        .segs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.alive && s.rank <= rank::RING)
        .map(|(k, _)| {
            let (a, b) = g.ends_of(k);
            (seg_foot(a, b, mid).0, b - a)
        })
        .min_by(|x, y| x.0.total_cmp(&y.0));
    best.map_or(0.3, |(_, v)| v.y.atan2(v.x))
}

/// A point in the middle of district `d`.
pub fn district_mid(dist: &Districts, d: u8) -> Vec2 {
    if d == 0 {
        return dist.c;
    }
    let k = d as usize - 1;
    let (a0, a1) = dist.span(k);
    let a = (a0 + a1) * 0.5;
    let r = (dist.r_core + dist.radius) * 0.5;
    dist.c + Vec2::new(a.cos(), a.sin()) * r
}

/// Lines across the whole town every `b` m one way and `a` m the other,
/// cut to the district and laid through each other.
#[allow(clippy::too_many_arguments)]
fn grid(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, angle: f32, a: f32, b: f32, alleys: bool, rng: &mut Rng) {
    let u = Vec2::new(angle.cos(), angle.sin());
    let v = Vec2::new(-u.y, u.x);
    let reach = dist.ext.reach * 1.05;
    let o = dist.c + v * rng.range(0.0, b) + u * rng.range(0.0, a);
    let mut long_offs: Vec<f32> = Vec::new();
    for (dir, across, gap) in [(u, v, b), (v, u, a)] {
        let n = (reach / gap) as i32 + 1;
        for i in -n..=n {
            let off = i as f32 * gap + rng.range(-0.08, 0.08) * gap;
            if dir == u {
                long_offs.push(off);
            }
            let base = o + across * off;
            let pts: Vec<Vec2> = (-(reach as i32) / 4..=(reach as i32) / 4).map(|s| base + dir * (s as f32 * 4.0)).collect();
            let inside = bridged_runs(&mut g.bridges, &pts, ground, |p| dist.at(p) == Some(d) && !ground.taken(p), 70.0);
            for run in inside {
                let run = ragged(run, dist, rng);
                for r2 in uncrowded(g, &run, gap * 0.45) {
                    if r2.len() >= 3 {
                        g.insert_line(&r2, rank::STREET, d, gap * 0.7);
                    }
                }
            }
        }
    }
    // Back lanes down the middle of most long blocks.
    if alleys {
        for w in long_offs.windows(2) {
            if rng.f() < 0.35 {
                continue;
            }
            let base = o + v * ((w[0] + w[1]) * 0.5);
            let pts: Vec<Vec2> = (-(reach as i32) / 4..=(reach as i32) / 4).map(|s| base + u * (s as f32 * 4.0)).collect();
            for run in split_runs(&pts, |p, _| dist.at(p) == Some(d) && !ground.wet(p) && !ground.taken(p)) {
                for r2 in uncrowded(g, &run, 9.0) {
                    if r2.len() >= 4 {
                        g.insert_line(&r2, rank::ALLEY, d, a * 0.6);
                    }
                }
            }
        }
    }
}

/// Cut back each end of a run (points 4 m apart) that reaches out to the
/// town's edge by its own distance, up to 40 m and mostly little, so a
/// grid's streets don't all stop on one line there.
fn ragged(mut run: Vec<Vec2>, dist: &Districts, rng: &mut Rng) -> Vec<Vec2> {
    for end in 0..2 {
        if run.len() < 3 {
            break;
        }
        let p = if end == 0 { run[0] } else { run[run.len() - 1] };
        if dist.sd(p) < -30.0 {
            continue;
        }
        let cut = ((10.0 * rng.f() * rng.f()) as usize).min(run.len().saturating_sub(3));
        if end == 0 {
            run.drain(..cut);
        } else {
            run.truncate(run.len() - cut);
        }
    }
    run
}

/// Rings and spokes. In the core they centre on the plaza; in an outer
/// sector they are arcs, so the district reads as part of the same circle.
fn radial(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, rng: &mut Rng) {
    let gap = rng.range(38.0, 48.0);
    // In the core the first ring stands a plot's depth clear of the market.
    let (r0, r1) = if d == 0 { ((dist.plaza + 30.0).max(gap * 0.9), dist.r_core * 1.6) } else { (dist.r_core + gap, dist.ext.reach) };
    let mut r = r0;
    let wob = rng.f() * 100.0;
    while r < r1 - gap * 0.4 {
        let n = ((r * std::f32::consts::TAU) / 4.0) as usize;
        let ring: Vec<Vec2> = (0..=n)
            .map(|k| {
                let t = k as f32 / n as f32 * std::f32::consts::TAU;
                let dd = Vec2::new(t.cos(), t.sin());
                dist.c + dd * r * (1.0 + 0.1 * perlin2(dd.x * 1.6 + wob, dd.y * 1.6, dist.seed) + 0.03 * perlin2(dd.x * 5.0, dd.y * 5.0 + wob, dist.seed))
            })
            .collect();
        for run in bridged_runs(&mut g.bridges, &ring, ground, |p| dist.at(p) == Some(d) && !ground.taken(p), 70.0) {
            for r2 in uncrowded(g, &run, gap * 0.4) {
                if r2.len() >= 3 {
                    g.insert_line(&r2, rank::STREET, d, gap * 0.6);
                }
            }
        }
        r += gap * rng.range(0.9, 1.1);
    }
    // Spokes: about one every 70 m round the middle of the band, each
    // starting at the plaza or the ring and running out to the edge.
    let mid = (r0 + r1) * 0.5;
    let n = ((mid * std::f32::consts::TAU) / 70.0).round().max(5.0) as usize;
    let off = rng.f();
    for k in 0..n {
        let t = (k as f32 + off) / n as f32 * std::f32::consts::TAU;
        let dd = Vec2::new(t.cos(), t.sin());
        let start = if d == 0 { 14.0 } else { dist.r_core * 0.9 };
        let pts: Vec<Vec2> = (0..((r1 * 1.2 - start) / 4.0) as usize).map(|s| dist.c + dd * (start + s as f32 * 4.0)).collect();
        for run in split_runs(&pts, |p, _| dist.at(p) == Some(d) && !ground.wet(p) && !ground.taken(p)) {
            for r2 in uncrowded(g, &run, 18.0) {
                if r2.len() >= 3 {
                    g.insert_line(&r2, rank::STREET, d, 26.0);
                }
            }
        }
    }
}

/// Through roads across a suburb, roughly round the town, for the closes to
/// come off.
fn collectors(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, rng: &mut Rng) {
    if d == 0 {
        return;
    }
    let k = d as usize - 1;
    let (a0, a1) = dist.span(k);
    let rr = dist.r_core + (dist.radius - dist.r_core) * rng.range(0.45, 0.6);
    let steps = (((a1 - a0) * rr) / 5.0) as usize;
    let seed = (rng.f() * 1000.0) as u32;
    let pts: Vec<Vec2> = (0..=steps)
        .map(|s| {
            let a = a0 - 0.05 + (a1 - a0 + 0.1) * s as f32 / steps.max(1) as f32;
            let r = rr * (1.0 + 0.12 * perlin2(a * 1.6, 0.5, seed));
            dist.c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    for run in split_runs(&pts, |p, _| dist.at(p) == Some(d) && !ground.wet(p) && !ground.taken(p)) {
        if run.len() >= 3 {
            g.insert_line(&run, rank::STREET, d, 30.0);
        }
    }
}

/// How a district that grew lays its streets.
struct Growth {
    /// Stop when no point is farther than this from a street, metres.
    block: f32,
    step: f32,
    /// Turn per step, radians, at most.
    wiggle: f32,
    /// Chance a new street ends past the gap it was laid to fill instead of
    /// running on to meet another.
    cul: f32,
    rank: u8,
    /// Closest a street may run alongside another.
    gap: f32,
    /// How far a dead end runs past the point it was laid for.
    over: f32,
    /// A dead end is a close with a turning circle.
    bulb: bool,
    /// Closest two junctions may be along a street.
    spacing: f32,
    /// Most streets to lay.
    limit: usize,
}

/// Fill a district by laying a street through its emptiest point, again
/// and again: each new street leaves a street already there at a right
/// angle, passes the point, and runs on until it meets another (or ends, a
/// close). The blocks come out the size `block` asks and no two alike.
fn grow_fill(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, p: &Growth, rng: &mut Rng) {
    let span = if d == 0 { (dist.r_core * 1.8).min(dist.ext.reach) } else { dist.ext.reach * 1.05 };
    let centre = dist.c;
    let mut refused: Vec<Vec2> = Vec::new();
    let mut laid = 0;
    for _ in 0..600 {
        if laid >= p.limit {
            break;
        }
        // The emptiest of a handful of points in the district.
        let mut cands: Vec<(f32, Vec2)> = Vec::new();
        for _ in 0..260 {
            let q = centre + Vec2::new(rng.range(-span, span), rng.range(-span, span));
            if dist.at(q) != Some(d) || ground.wet(q) || refused.iter().any(|r| r.distance(q) < p.block * 0.6) {
                continue;
            }
            let e = g.nearest(q, p.block * 3.0, |_| false).map_or(p.block * 3.0, |x| x.1);
            cands.push((e, q));
        }
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        let Some(&(best, _)) = cands.first() else { break };
        if best < p.block {
            break;
        }
        let mut done = false;
        for &(e, q) in cands.iter().take(6) {
            if e < p.block {
                break;
            }
            if grow_one(g, dist, d, ground, p, q, rng) {
                laid += 1;
                done = true;
                break;
            }
            refused.push(q);
        }
        if !done && refused.len() > 400 {
            break;
        }
    }
}

/// Lay one street from the street nearest `q` through it.
fn grow_one(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, p: &Growth, q: Vec2, rng: &mut Rng) -> bool {
    let Some((k, _, foot)) = g.nearest(q, p.block * 4.0, |_| false) else {
        return false;
    };
    let (a, b) = g.ends_of(k);
    let along = (b - a).normalize_or_zero();
    let mut dir = (q - foot).normalize_or_zero();
    if dir == Vec2::ZERO {
        return false;
    }
    // Leave at near a right angle even from the end of a street.
    let perp = Vec2::new(-along.y, along.x);
    let perp = if perp.dot(dir) < 0.0 { -perp } else { perp };
    dir = (perp * 2.0 + dir).normalize_or_zero();
    // Not too near another junction on the same street.
    let deg = g.degrees();
    let snap = g.node_near(foot, 6.0);
    let start_p = match snap {
        Some(n) => g.nodes[n as usize],
        None => foot,
    };
    if snap.is_none() {
        for n in [g.segs[k].a, g.segs[k].b] {
            if deg[n as usize] >= 3 && g.nodes[n as usize].distance(foot) < p.spacing {
                return false;
            }
        }
    }
    // Walk the line out before touching the graph.
    let want = start_p.distance(q) + if rng.f() < p.cul { p.over * rng.range(0.6, 1.2) } else { p.block * 8.0 };
    // How far short of the town's edge this street stops: each its own, so
    // the ends out there don't line up along one contour.
    let short = 15.0 + 30.0 * rng.f() * rng.f();
    let mut pts = vec![start_p];
    let mut pos = start_p;
    let mut meets: Option<(usize, Vec2)> = None;
    let mut walked = 0.0;
    while walked < want {
        // Of a few turns, take the one that climbs least: on a hill the
        // lanes follow the slope round instead of going straight up it.
        let base = (rng.f() - 0.5) * 2.0 * p.wiggle;
        let h0 = ground.height(pos);
        let mut best = (f32::MAX, dir);
        for t in [base - p.wiggle * 0.7, base, base + p.wiggle * 0.7] {
            let (s, c) = t.sin_cos();
            let dd = Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c).normalize_or_zero();
            let climb = (ground.height(pos + dd * p.step) - h0).abs() / p.step;
            let score = climb * 4.0 + (t - base).abs() * 0.3 + rng.f() * 0.05;
            if score < best.0 {
                best = (score, dd);
            }
        }
        dir = best.1;
        let next = pos + dir * p.step;
        // Meeting a street: look a little past the step so no sliver is left.
        let look = pos + dir * (p.step + p.gap * 0.5);
        let hit = g.hits(pos, look, snap).into_iter().find(|&(_, kk, _)| walked > 1.0 || kk != k);
        if let Some((_, kk, x)) = hit {
            // A crossing at a glancing angle makes a sliver; refuse it.
            let (ha, hb) = g.ends_of(kk);
            if (hb - ha).normalize_or_zero().dot(dir).abs() > 0.8 {
                break;
            }
            meets = Some((kk, x));
            break;
        }
        if dist.at(next) != Some(d) || ground.wet(next) || ground.taken(next) || dist.sd(next) > -short {
            break;
        }
        if walked > p.gap && g.crowded(next, dir, p.gap) {
            break;
        }
        pts.push(next);
        pos = next;
        walked += p.step;
    }
    if meets.is_none() && walked < p.block.min(20.0) {
        return false;
    }
    // Now lay it.
    let n0 = match snap {
        Some(n) => n,
        None => g.split(k, foot),
    };
    let mut cur = n0;
    for &pt in pts.iter().skip(1) {
        let m = g.add_node(pt);
        g.add_seg(cur, m, p.rank, d);
        cur = m;
    }
    match meets {
        Some((kk, x)) => {
            if g.segs[kk].alive {
                let m = g.split(kk, x);
                g.add_seg(cur, m, p.rank, d);
            } else {
                // The segment it was to meet was split while laying (it
                // crossed itself): settle for the nearest node.
                if let Some(m) = g.node_near(x, 4.0) {
                    g.add_seg(cur, m, p.rank, d);
                }
            }
        }
        None => {
            if p.bulb || walked < p.gap * 2.0 {
                g.ends.push(cur);
            }
        }
    }
    true
}

/// Quays: lines `off` m back from the water's edge through the town, as
/// polylines. Found as the contour of distance-to-water on a 3 m grid
/// (marching squares), so a river through the town gets one along each
/// bank and a harbour front follows its shore.
pub fn quays(dist: &Districts, ground: &dyn Ground, off: f32) -> Vec<Vec<Vec2>> {
    const S: f32 = 3.0;
    let ext = dist.ext.reach + 12.0;
    let lo = (dist.c - Vec2::splat(ext)).max(Vec2::splat(S));
    let hi = (dist.c + Vec2::splat(ext)).min(Vec2::splat(ground.size() - S));
    let nx = ((hi.x - lo.x) / S) as usize + 1;
    let nz = ((hi.y - lo.y) / S) as usize + 1;
    let at = |x: usize, z: usize| lo + Vec2::new(x as f32, z as f32) * S;
    // Distance to water, metres.
    let d = super::extent::chamfer(nx, nz, S, |i| ground.wet(at(i % nx, i / nx)));
    if d.iter().all(|&v| v > 0.0) {
        return Vec::new();
    }
    // Marching squares: one segment per crossing square, keyed by the grid
    // edges its ends lie on, so the pieces can be strung together.
    let edge_pt = |a: (usize, usize), b: (usize, usize)| {
        let (da, db) = (d[a.1 * nx + a.0], d[b.1 * nx + b.0]);
        let t = ((off - da) / (db - da)).clamp(0.0, 1.0);
        at(a.0, a.1).lerp(at(b.0, b.1), t)
    };
    // An edge id: (x, z, 0 horizontal / 1 vertical).
    let mut segs: Vec<((usize, usize, u8), (usize, usize, u8))> = Vec::new();
    for z in 0..nz - 1 {
        for x in 0..nx - 1 {
            let v = [d[z * nx + x], d[z * nx + x + 1], d[(z + 1) * nx + x + 1], d[(z + 1) * nx + x]];
            let mut code = 0;
            for (k, &vv) in v.iter().enumerate() {
                if vv < off {
                    code |= 1 << k;
                }
            }
            if code == 0 || code == 15 {
                continue;
            }
            // Edges: 0 top (x,z)-(x+1,z), 1 right, 2 bottom, 3 left.
            let e = [(x, z, 0u8), (x + 1, z, 1u8), (x, z + 1, 0u8), (x, z, 1u8)];
            let crossed: Vec<usize> = (0..4).filter(|&k| ((code >> k) & 1) != ((code >> ((k + 1) % 4)) & 1)).collect();
            if crossed.len() == 2 {
                segs.push((e[crossed[0]], e[crossed[1]]));
            } else if crossed.len() == 4 {
                segs.push((e[0], e[1]));
                segs.push((e[2], e[3]));
            }
        }
    }
    let pos = |e: (usize, usize, u8)| if e.2 == 0 { edge_pt((e.0, e.1), (e.0 + 1, e.1)) } else { edge_pt((e.0, e.1), (e.0, e.1 + 1)) };
    let mut by_end: crate::util::FxHashMap<(usize, usize, u8), Vec<usize>> = Default::default();
    for (k, s) in segs.iter().enumerate() {
        by_end.entry(s.0).or_default().push(k);
        by_end.entry(s.1).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for k0 in 0..segs.len() {
        if used[k0] {
            continue;
        }
        used[k0] = true;
        let mut chain = vec![segs[k0].0, segs[k0].1];
        for dir in 0..2 {
            loop {
                let end = if dir == 0 { *chain.last().unwrap() } else { chain[0] };
                let Some(&k) = by_end.get(&end).and_then(|l| l.iter().find(|&&k| !used[k])) else { break };
                used[k] = true;
                let next = if segs[k].0 == end { segs[k].1 } else { segs[k].0 };
                if dir == 0 {
                    chain.push(next);
                } else {
                    chain.insert(0, next);
                }
            }
        }
        let pts: Vec<Vec2> = chain.into_iter().map(pos).collect();
        // Only the stretches inside the town and on level enough ground.
        for run in split_runs(&pts, |p, _| dist.at(p).is_some()) {
            let line = resample(&chaikin(&run, 2), 5.0);
            let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
            if len > 50.0 {
                out.push(line);
            }
        }
    }
    out
}

/// Join the dead ends out at the edge of the town in pairs, with a lane
/// along the edge, so a grid's last streets close their blocks instead of
/// running out into the fields.
pub fn link_edge_ends(g: &mut Graph, dist: &Districts, ground: &dyn Ground) {
    let (deg, meant) = (g.degrees(), g.meant_ends());
    let ends: Vec<u32> = (0..g.nodes.len() as u32)
        .filter(|&n| deg[n as usize] == 1 && !meant[n as usize])
        .filter(|&n| {
            let p = g.nodes[n as usize];
            dist.sd(p) > -0.25 * dist.radius
        })
        .collect();
    let mut joined = vec![false; ends.len()];
    for i in 0..ends.len() {
        if joined[i] {
            continue;
        }
        let p = g.nodes[ends[i] as usize];
        let mut best: Option<(f32, usize)> = None;
        for j in 0..ends.len() {
            if j == i || joined[j] {
                continue;
            }
            let q = g.nodes[ends[j] as usize];
            let dd = p.distance(q);
            if (12.0..110.0).contains(&dd) && best.is_none_or(|b| dd < b.0) {
                best = Some((dd, j));
            }
        }
        let Some((dd, j)) = best else { continue };
        let q = g.nodes[ends[j] as usize];
        let steps = (dd / 5.0).ceil() as usize;
        let line: Vec<Vec2> = (0..=steps).map(|k| p.lerp(q, k as f32 / steps as f32)).collect();
        if line.iter().any(|&x| ground.wet(x)) {
            continue;
        }
        // Not through the town: the link must cross nothing on its way.
        if g.hits(p, q, Some(ends[i])).iter().any(|h| h.0 < 0.98) {
            continue;
        }
        let d = dist.at(p.lerp(q, 0.5)).unwrap_or(0);
        g.insert_line(&line, rank::LANE, d, 2.0);
        joined[i] = true;
        joined[j] = true;
    }
}

/// Jetties off the quays that face open water (a harbour or the sea, not a
/// river's far bank): one every 40 m or so, 25 to 40 m out.
pub fn piers(quays: &[Vec<Vec2>], ground: &dyn Ground, rng: &mut Rng, near: Vec2, within: f32) -> Vec<(Vec2, Vec2)> {
    let mut out: Vec<(Vec2, Vec2)> = Vec::new();
    for q in quays {
        let len: f32 = q.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut s = rng.range(10.0, 30.0);
        while s < len - 10.0 {
            let (p, t) = super::lots::point_at(q, s);
            for side in [1.0f32, -1.0] {
                let n = Vec2::new(-t.y, t.x) * side;
                // The water side, and open water 80 m out.
                if !ground.wet(p + n * 12.0) || !(1..=8).all(|k| ground.wet(p + n * (12.0 + k as f32 * 10.0))) {
                    continue;
                }
                let start = p + n * 6.0;
                let end = p + n * rng.range(35.0, 55.0);
                // The harbour is by the market, and has a handful of piers,
                // not a comb of them down the whole coast.
                if start.distance(near) > within || out.len() >= 6 {
                    continue;
                }
                if out.iter().all(|(a, _)| a.distance(start) > 30.0) {
                    out.push((start, end));
                }
            }
            s += rng.range(35.0, 50.0);
        }
    }
    out
}

/// Carry each dead end on, in the direction it was going, to the first
/// street within 60 m ahead, so a street that stopped just short of
/// another meets it. Ends the layout meant (closes) stay.
pub fn reach_on(g: &mut Graph, ground: &dyn Ground) {
    let (deg, meant) = (g.degrees(), g.meant_ends());
    let ends: Vec<u32> = (0..g.nodes.len() as u32).filter(|&n| deg[n as usize] == 1 && !meant[n as usize]).collect();
    for n in ends {
        // The segment into the end, for its direction.
        let Some(k) = g.segs.iter().position(|s| s.alive && (s.a == n || s.b == n)) else { continue };
        let s = g.segs[k];
        let other = if s.a == n { s.b } else { s.a };
        let p = g.nodes[n as usize];
        let dir = (p - g.nodes[other as usize]).normalize_or_zero();
        let Some(&(_, hk, x)) = g.hits(p, p + dir * 60.0, Some(n)).first() else { continue };
        // Over water only across a canal: a short wet stretch, with no
        // other bridge close by. That is where the quarters either side of
        // a canal get their bridges.
        let steps = (p.distance(x) / 3.0).ceil().max(1.0) as usize;
        let wet: Vec<Vec2> = (1..steps).map(|j| p.lerp(x, j as f32 / steps as f32)).filter(|&q| ground.wet(q)).collect();
        if !wet.is_empty() {
            let span = wet.len() as f32 * p.distance(x) / steps as f32;
            let mid = wet[wet.len() / 2];
            if span > 25.0 || g.bridges.iter().any(|b| b.distance(mid) < 45.0) {
                continue;
            }
            g.bridges.push(mid);
        }
        let (ha, hb) = g.ends_of(hk);
        if (hb - ha).normalize_or_zero().dot(dir).abs() > 0.85 {
            continue;
        }
        let m = g.split(hk, x);
        g.add_seg(n, m, s.rank, s.district);
    }
}

/// Streets from the ring to the edge across an outer district, spread
/// across its width and wandering a little, so a quarter of lanes has a
/// few through-routes.
fn through_streets(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground, rng: &mut Rng) {
    let k = d as usize - 1;
    let (a0, a1) = dist.span(k);
    let count = if a1 - a0 > 1.2 { 3 } else { 2 };
    for j in 0..count {
        let a = a0 + (a1 - a0) * (j as f32 + 1.0) / (count as f32 + 1.0) + rng.range(-0.08, 0.08);
        let seed = (rng.f() * 1e6) as u32;
        let pts: Vec<Vec2> = (0..(dist.ext.reach / 5.0) as usize)
            .map(|s| {
                let r = dist.r_core * 0.9 + s as f32 * 5.0;
                let wob = 0.12 * perlin2(r * 0.01, a * 3.0, seed);
                dist.c + Vec2::new((a + wob).cos(), (a + wob).sin()) * r
            })
            .collect();
        for run in split_runs(&pts, |p, _| dist.at(p) == Some(d) && !ground.wet(p) && !ground.taken(p)) {
            for r2 in uncrowded(g, &run, 20.0) {
                if r2.len() >= 4 {
                    g.insert_line(&r2, rank::STREET, d, 20.0);
                }
            }
        }
    }
}

/// Lanes behind the plots of the main streets: each main road through the
/// district, offset 42 m to either side, as far as the district goes. The
/// plots between are long and narrow, a burgage pattern.
fn back_lanes(g: &mut Graph, dist: &Districts, d: u8, ground: &dyn Ground) {
    for line in &dist.mains {
        let n = line.len();
        if n < 3 {
            continue;
        }
        for side in [1.0f32, -1.0] {
            let pts: Vec<Vec2> = (0..n)
                .map(|k| {
                    let t = (line[(k + 1).min(n - 1)] - line[k.saturating_sub(1)]).normalize_or_zero();
                    line[k] + Vec2::new(-t.y, t.x) * side * 42.0
                })
                .collect();
            let pts = resample(&chaikin(&pts, 2), 4.0);
            for run in split_runs(&pts, |p, _| dist.at(p) == Some(d) && !ground.wet(p) && !ground.taken(p) && dist.sd(p) < -10.0) {
                for r2 in uncrowded(g, &run, 20.0) {
                    if r2.len() >= 6 {
                        g.insert_line(&r2, rank::LANE, d, 12.0);
                    }
                }
            }
        }
    }
}

/// Bridges over canals: each dead end by the water is joined to the nearest
/// street node across it, where the crossing is canal-sized (water no wider
/// than 25 m, the whole span under 45 m), it crosses no other street, and no
/// bridge stands within 45 m. A quarter cut off by a canal gets its bridges
/// this way, whatever pattern laid its lanes.
pub fn bridge_canals(g: &mut Graph, ground: &dyn Ground) -> usize {
    let (deg, meant) = (g.degrees(), g.meant_ends());
    let ends: Vec<u32> = (0..g.nodes.len() as u32).filter(|&n| deg[n as usize] == 1 && !meant[n as usize]).collect();
    let mut made = 0;
    for n in ends {
        let p = g.nodes[n as usize];
        // Only ends by the water.
        if !(0..8).any(|k| {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            ground.wet(p + Vec2::new(a.cos(), a.sin()) * 9.0)
        }) {
            continue;
        }
        let mut best: Option<(f32, u32, Vec2)> = None;
        for k in g.near(p - Vec2::splat(45.0), p + Vec2::splat(45.0)) {
            let s = g.segs[k];
            for m in [s.a, s.b] {
                if m == n {
                    continue;
                }
                let q = g.nodes[m as usize];
                let dd = p.distance(q);
                if !(8.0..45.0).contains(&dd) || ground.wet(q) || best.is_some_and(|b| dd >= b.0) {
                    continue;
                }
                let steps = (dd / 1.5).ceil() as usize;
                let wet: Vec<Vec2> = (1..steps).map(|j| p.lerp(q, j as f32 / steps as f32)).filter(|&x| ground.wet(x)).collect();
                if wet.is_empty() || wet.len() as f32 * dd / steps as f32 > 25.0 {
                    continue;
                }
                best = Some((dd, m, wet[wet.len() / 2]));
            }
        }
        let Some((_, m, mid)) = best else { continue };
        if g.bridges.iter().any(|b| b.distance(mid) < 45.0) {
            continue;
        }
        let q = g.nodes[m as usize];
        if g.hits(p, q, Some(n)).iter().any(|h| h.0 < 0.98) {
            continue;
        }
        let s = g.segs.iter().find(|s| s.alive && (s.a == n || s.b == n)).copied();
        let (rank, district) = s.map_or((rank::STREET, 0), |s| (s.rank.min(rank::STREET), s.district));
        // A run of points so the wet stretch is found and decked.
        let steps = (p.distance(q) / 4.0).ceil().max(2.0) as usize;
        let mut prev = n;
        for j in 1..steps {
            let x = g.add_node(p.lerp(q, j as f32 / steps as f32));
            g.add_seg(prev, x, rank, district);
            prev = x;
        }
        g.add_seg(prev, m, rank, district);
        g.bridges.push(mid);
        made += 1;
    }
    made
}
