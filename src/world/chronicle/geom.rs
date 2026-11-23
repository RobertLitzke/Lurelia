//! Geometry shared by several events: visiting cells near a point, lines
//! through points, graded strips and painted paths.

use super::EventCtx;
use crate::util::FxHashMap as HashMap;
use crate::world::fields::*;
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;

/// Unit direction of a compass bearing in degrees (x east, y south).
pub fn dir_deg(b: f32) -> Vec2 {
    let r = b.to_radians();
    Vec2::new(r.sin(), -r.cos())
}

/// Visit every cell within `r` metres of `c`, with its distance. The fields are
/// handed to the visitor so it can change them.
pub fn each_near(f: &mut Fields, c: Vec2, r: f32, mut g: impl FnMut(&mut Fields, usize, f32, Vec2)) {
    let n = f.n as i32;
    let x0 = (((c.x - r) / CELL).floor() as i32).max(0);
    let x1 = (((c.x + r) / CELL).ceil() as i32).min(n - 1);
    let z0 = (((c.y - r) / CELL).floor() as i32).max(0);
    let z1 = (((c.y + r) / CELL).ceil() as i32).min(n - 1);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let i = z as usize * f.n + x as usize;
            let p = f.centre(i);
            let d = p.distance(c);
            if d <= r {
                g(f, i, d, p);
            }
        }
    }
}

/// Points about `step` m apart on a Catmull-Rom curve through `pts`.
pub fn spline(pts: &[Vec2], closed: bool, step: f32) -> Vec<Vec2> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let at = |k: i32| -> Vec2 {
        if closed {
            pts[k.rem_euclid(n as i32) as usize]
        } else {
            pts[k.clamp(0, n as i32 - 1) as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    let mut out = Vec::new();
    for s in 0..segs as i32 {
        let (p0, p1, p2, p3) = (at(s - 1), at(s), at(s + 1), at(s + 2));
        let steps = (p1.distance(p2) / step).ceil().max(1.0) as i32;
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            out.push(0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3));
        }
    }
    if !closed {
        out.push(pts[n - 1]);
    }
    out
}

/// Points every 2 m along a polyline, swinging side to side by up to `meander` m.
pub fn densify(pts: &[Vec2], meander: f32, seed: u32) -> Vec<Vec2> {
    let mut out = Vec::new();
    let mut s = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let d = (b - a) / len.max(1e-3);
        let perp = Vec2::new(-d.y, d.x);
        let steps = (len / 2.0).ceil().max(1.0) as i32;
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            let off = perlin2((s + t * len) * 0.012, 3.7, seed) * meander * 2.0 * (t * (1.0 - t) * 4.0).min(1.0).max(0.35);
            out.push(a + d * len * t + perp * off);
        }
        s += len;
    }
    if let Some(&l) = pts.last() {
        out.push(l);
    }
    out
}

/// The distance along `points` to each one, and the whole length. A closed
/// line's length takes in the segment from the last point back to the first.
pub fn cumulative(points: &[Vec2], closed: bool) -> (Vec<f32>, f32) {
    let n = points.len();
    let mut along = Vec::with_capacity(n);
    let mut s = 0.0;
    for i in 0..n {
        along.push(s);
        if closed || i + 1 < n {
            s += points[i].distance(points[(i + 1) % n]);
        }
    }
    (along, s)
}

/// The point `s` metres round a closed line and the direction it runs
/// there, given its `cumulative` distances and length. `s` wraps, either way.
pub fn at_closed(points: &[Vec2], along: &[f32], length: f32, s: f32) -> (Vec2, Vec2) {
    let n = points.len();
    let s = s.rem_euclid(length);
    let k = along.partition_point(|&a| a <= s).saturating_sub(1);
    let (a, b) = (points[k], points[(k + 1) % n]);
    let seg = a.distance(b).max(1e-4);
    (a.lerp(b, (s - along[k]) / seg), (b - a) / seg)
}

/// Ground height along a line sampled about every metre, averaged over a
/// triangle window `window` m long, so a road or track grades through bumps.
/// Samples more than `GAP` m below the grade are a gap the road bridges: they
/// drop out of the average, so a canyon doesn't drag the road down into it.
pub fn smoothed_ground(f: &Fields, line: &[Vec2], window: f32, closed: bool) -> Vec<f32> {
    let raw: Vec<f32> = line.iter().map(|p| f.height(p.x, p.y)).collect();
    let n = raw.len() as i32;
    let half = (window * 0.5).max(1.0) as i32;
    let mut cur = raw.clone();
    for pass in 0..4 {
        cur = (0..n)
            .map(|k| {
                let mut sum = 0.0;
                let mut cnt = 0.0;
                for j in -half..=half {
                    let q = (if closed { (k + j).rem_euclid(n) } else { (k + j).clamp(0, n - 1) }) as usize;
                    if pass > 0 && raw[q] < cur[q] - GAP {
                        continue;
                    }
                    let w = 1.0 - j.abs() as f32 / (half as f32 + 1.0);
                    sum += raw[q] * w;
                    cnt += w;
                }
                if cnt > 0.0 {
                    sum / cnt
                } else {
                    cur[k as usize]
                }
            })
            .collect();
    }
    cur.into_iter().map(|h| h.max(f.sea + 0.6)).collect()
}

const GAP: f32 = 3.0;

/// How a graded strip looks: its width, surface and shoulder materials, and
/// how many metres beside it the cut is feathered back.
pub struct Strip {
    pub width: f32,
    pub mat: u8,
    pub shoulder: u8,
    pub margin: f32,
}

/// Cut and fill a strip along `line` to `base + off` (tilted by `bank` m per
/// metre across, positive lifting the line's left), paint it, and feather the
/// ground beside it back to what was there.
/// Ground more than `GAP` m under the grade is left alone: a gap for a bridge.
/// Returns those gaps as (start, end, deck height).
pub fn grade_strip(cx: &mut EventCtx, line: &[Vec2], base: &[f32], off: &[f32], bank: &[f32], s: &Strip) -> Vec<(Vec2, Vec2, f32)> {
    grade_strip_except(cx, line, base, off, bank, s, &[])
}

/// Two stretches of road count as one over the other, not side by side,
/// when their heights are more than this far apart, metres.
pub const UNDER: f32 = 3.0;
/// A road's wall is this thick, standing just outside the kerb, metres.
pub const WALL_THICK: f32 = 1.2;

/// A line point near a cell: how far, which point, and how many metres
/// left of the line the cell is there.
#[derive(Clone, Copy)]
pub struct Nearest {
    pub d: f32,
    pub k: usize,
    pub side: f32,
}

/// One cell near a line: its nearest point, and the point whose road has it.
pub struct Claim {
    pub cell: usize,
    pub near: Nearest,
    /// `near`, unless that point is `off` (a deck, a tunnel: built some
    /// other way). Then the nearest point that is not off, if its road is
    /// more than `UNDER` above or below (the cell is under a deck over that
    /// road); otherwise none, and the cell is left alone.
    pub owner: Option<Nearest>,
}

/// Every cell within `reach(k)` of some point `k` of `line`, in cell order,
/// each by its nearest point of the whole line: where a road passes close
/// to itself, a cell never goes to the further stretch. `y(k)` is the
/// road's height at a point. A closed line's direction at its ends wraps.
pub fn claims(f: &mut Fields, line: &[Vec2], closed: bool, reach: impl Fn(usize) -> f32, off: impl Fn(usize) -> bool, y: impl Fn(usize) -> f32) -> Vec<Claim> {
    let n = line.len();
    let none = Nearest { d: f32::MAX, k: 0, side: 0.0 };
    let mut best: HashMap<usize, Nearest> = HashMap::default();
    let mut on: HashMap<usize, Nearest> = HashMap::default();
    for (k, &p) in line.iter().enumerate() {
        let dir = if closed { (line[(k + 1) % n] - line[(k + n - 1) % n]).normalize_or(Vec2::X) } else { (line[(k + 1).min(n - 1)] - line[k.saturating_sub(1)]).normalize_or_zero() };
        let left = Vec2::new(dir.y, -dir.x);
        let off_k = off(k);
        each_near(f, p, reach(k), |_f: &mut Fields, i, d, q| {
            let here = Nearest { d, k, side: (q - p).dot(left) };
            let e = best.entry(i).or_insert(none);
            if d < e.d {
                *e = here;
            }
            if !off_k {
                let e = on.entry(i).or_insert(none);
                if d < e.d {
                    *e = here;
                }
            }
        });
    }
    let mut out: Vec<Claim> = best
        .into_iter()
        .map(|(cell, near)| {
            let owner = if off(near.k) { on.get(&cell).filter(|b| (y(b.k) - y(near.k)).abs() > UNDER).copied() } else { Some(near) };
            Claim { cell, near, owner }
        })
        .collect();
    out.sort_unstable_by_key(|c| c.cell);
    out
}

/// `grade_strip`, leaving out the line points marked in `skip`: a stretch
/// built some other way (a circuit's decks), which neither grades the ground
/// under it nor claims the cells another stretch below it grades.
pub fn grade_strip_except(cx: &mut EventCtx, line: &[Vec2], base: &[f32], off: &[f32], bank: &[f32], s: &Strip, skip: &[bool]) -> Vec<(Vec2, Vec2, f32)> {
    let (width, m, shoulder, margin, cause) = (s.width, s.mat, s.shoulder, s.margin, cx.cause);
    let f = &mut cx.f;
    let half = width * 0.5;
    let reach = half + 1.0 + margin;
    // Each cell goes by its nearest line point. Every point takes part,
    // and a cell whose nearest point is skipped is left alone unless a road
    // under or over the skipped stretch has it (`claims`), so the stretch
    // before a skipped one never claims the ground past its end.
    let skipped = |k: usize| skip.get(k).copied().unwrap_or(false);
    let cells = claims(f, line, false, |_| reach, skipped, |k| base[k] + off[k]);
    // Gaps along the centre line.
    let gap: Vec<bool> = line.iter().enumerate().map(|(k, p)| f.height(p.x, p.y) < base[k] + off[k] - GAP).collect();
    let mut spans = Vec::new();
    let mut k = 0;
    while k < line.len() {
        if gap[k] {
            let a = k;
            while k < line.len() && gap[k] {
                k += 1;
            }
            spans.push((line[a], line[k - 1], base[a]));
        }
        k += 1;
    }
    for c in cells {
        let (i, Some(Nearest { d, k, side })) = (c.cell, c.owner) else { continue };
        if f.water[i] == 1 {
            continue;
        }
        let tilt = bank.get(k).copied().unwrap_or(0.0) * side.clamp(-half, half);
        let target = base[k] + off[k] + tilt;
        if f.h[i] < target - GAP {
            continue;
        }
        if d < half + 1.0 {
            f.h[i] = target;
            f.surface[i] = if d < half { m } else { shoulder };
            f.occupied[i] = true;
            f.water[i] = 3;
            f.cliff[i] = 0.0;
            f.cause[i] = cause;
        } else {
            let t = smoothstep(reach, half + 1.0, d);
            f.h[i] += (target - f.h[i]) * t;
            f.cliff[i] *= 1.0 - t;
            if t > 0.3 {
                f.occupied[i] = true;
            }
        }
    }
    spans
}

/// Paint a path `width` m wide along a line on dry land and mark it taken.
pub fn stamp_path(f: &mut Fields, line: &[Vec2], width: f32, m: u8) {
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        let steps = (len / 0.7).ceil().max(1.0) as i32;
        for k in 0..=steps {
            let p = w[0].lerp(w[1], k as f32 / steps as f32);
            each_near(f, p, width * 0.5, |f: &mut Fields, i, _, _| {
                if f.water[i] == 0 && f.h[i] > f.sea {
                    f.surface[i] = m;
                    f.occupied[i] = true;
                }
            });
        }
    }
}

/// Where two polylines cross each other, in plan.
pub fn crossing_points(a: &[Vec2], b: &[Vec2]) -> Vec<Vec2> {
    let mut out = Vec::new();
    let bound = |l: &[Vec2]| l.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let ((alo, ahi), (blo, bhi)) = (bound(a), bound(b));
    if alo.x > bhi.x || blo.x > ahi.x || alo.y > bhi.y || blo.y > ahi.y {
        return out;
    }
    for u in a.windows(2) {
        for v in b.windows(2) {
            if let Some(p) = segments_cross(u[0], u[1], v[0], v[1]) {
                out.push(p);
            }
        }
    }
    out
}

/// Where segment `a`-`b` crosses `c`-`d`, if it does.
pub fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    cross_params(a, b, c, d, 1e-9).map(|(t, _)| a + (b - a) * t)
}

/// Where segments `a`-`b` and `c`-`d` cross, as (t along ab, u along cd).
/// They count as parallel, and never cross, when the cross product of their
/// vectors is under `parallel`.
pub fn cross_params(a: Vec2, b: Vec2, c: Vec2, d: Vec2, parallel: f32) -> Option<(f32, f32)> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    if den.abs() < parallel {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

/// The distance from `p` to segment `a`-`b`, the nearest point on it, and
/// how far along it that is (0 at `a`, 1 at `b`).
pub fn seg_foot(a: Vec2, b: Vec2, p: Vec2) -> (f32, Vec2, f32) {
    let ab = b - a;
    let l2 = ab.length_squared().max(1e-9);
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    let f = a + ab * t;
    (p.distance(f), f, t)
}

/// `seg_foot` without the point: the distance and how far along.
pub fn seg_dist(a: Vec2, b: Vec2, p: Vec2) -> (f32, f32) {
    let (d, _, t) = seg_foot(a, b, p);
    (d, t)
}

#[cfg(test)]
mod cross_tests {
    use super::*;

    #[test]
    fn segments_cross_where_they_meet() {
        let p = segments_cross(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0), Vec2::new(10.0, 0.0)).unwrap();
        assert!((p - Vec2::new(5.0, 5.0)).length() < 1e-4);
        assert!(segments_cross(Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0)).is_none());
        let a = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)];
        let b = [Vec2::new(5.0, -5.0), Vec2::new(5.0, 5.0), Vec2::new(15.0, 5.0)];
        assert_eq!(crossing_points(&a, &b).len(), 2);
    }
}
