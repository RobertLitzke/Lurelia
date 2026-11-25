//! Plots and buildings along the frontages.
//!
//! Every metre of the town is given to the street whose kerb is nearest it,
//! on one side or the other, and remembers how far along that street it
//! lies. The dense districts are built as closed blocks from that
//! (`perimeter`); elsewhere a plot runs back from its frontage until the
//! ground stops being its street's, and a house stands in it (`frontage`).

use super::graph::seg_foot;
use super::{rank, Boundary, Building, Districts, Edge, Ground, Kind, Plan, Rng, Roof, Spec, Style, Tree, Way};
use glam::Vec2;

const FREE: u8 = 0;
const STREET: u8 = 1;
const BUILT: u8 = 2;
const OPEN: u8 = 3;
const WET: u8 = 4;
/// Under a structure an earlier event placed (`Ground::taken`): never built
/// on, and never claimed by a square, park or street mark, so a church
/// cannot stand in an older landmark's door.
const TAKEN: u8 = 5;
/// A footpath to a door: walked on, never built on.
const PATH: u8 = 6;

/// The town on a one-metre raster: who owns each cell and what is on it.
struct Raster {
    lo: Vec2,
    w: usize,
    h: usize,
    owner: Vec<u32>,
    /// Distance to the owning street's kerb (its pavement's outer edge).
    dist: Vec<f32>,
    /// How far along the owning street the cell's foot lies, metres.
    along: Vec<f32>,
    /// Distance to open water, metres (to 60).
    wd: Vec<f32>,
    used: Vec<u8>,
}

impl Raster {
    fn idx(&self, p: Vec2) -> Option<usize> {
        let x = (p.x - self.lo.x).floor();
        let z = (p.y - self.lo.y).floor();
        (x >= 0.0 && z >= 0.0 && (x as usize) < self.w && (z as usize) < self.h).then(|| z as usize * self.w + x as usize)
    }
    fn centre(&self, i: usize) -> Vec2 {
        self.lo + Vec2::new((i % self.w) as f32 + 0.5, (i / self.w) as f32 + 0.5)
    }
}

/// How plots are cut in each kind of district.
struct LotStyle {
    width: (f32, f32),
    setback: (f32, f32),
    depth: (f32, f32),
    /// Gap each side of a building inside its plot.
    side: (f32, f32),
    floors: (u8, u8),
    /// Most the ground may fall across a footprint, metres.
    fall: f32,
}

fn lot_style(style: Style, r: u8, far: f32) -> LotStyle {
    // `far` is 0 at the centre and 1 at the edge: plots loosen outward.
    let loose = ((far - 0.55) * 2.2).clamp(0.0, 1.0);
    match style {
        Style::Maze => LotStyle {
            width: (5.5, 8.5),
            setback: (0.0, 0.0),
            depth: (9.0, 15.0),
            side: (0.0, 0.6 * loose),
            floors: (2, if far < 0.3 { 4 } else { 3 }),
            fall: 3.0,
        },
        Style::Radial => LotStyle {
            width: (10.0, 15.0),
            setback: (0.0, 0.0),
            depth: (12.0, 17.0),
            side: (0.0, 0.0),
            floors: (if r <= rank::RING { 4 } else { 3 }, if far < 0.4 { 6 } else { 4 }),
            fall: 3.0,
        },
        Style::Grid => LotStyle {
            width: (6.5 + 4.0 * loose, 9.5 + 6.0 * loose),
            setback: (0.0, 1.5 + 3.0 * loose),
            depth: (12.0, 18.0 - 5.0 * loose),
            side: (0.0, 2.5 * loose),
            floors: (2 + (loose < 0.5) as u8, if far < 0.45 { 5 } else { 3 }),
            fall: 3.0,
        },
        Style::Suburb => LotStyle {
            width: (17.0, 22.0),
            setback: (6.0, 8.5),
            depth: (9.0, 12.0),
            side: (2.5, 4.0),
            floors: (1, 2),
            fall: 2.2,
        },
        Style::Ribbon => LotStyle {
            width: (8.0, 13.0),
            setback: (0.3, 2.0),
            depth: (7.0, 10.0),
            side: (0.0, 1.5),
            floors: (1, 2),
            fall: 2.5,
        },
        Style::Village => LotStyle {
            width: (9.0, 14.0),
            setback: (0.5, 2.0),
            depth: (7.0, 10.0),
            side: (0.0, 1.8),
            floors: (1, 2),
            fall: 2.2,
        },
    }
}

pub fn build(plan: &mut Plan, dist: &Districts, spec: &Spec, ground: &dyn Ground, rng: &mut Rng) {
    let c = dist.c;
    let ext = dist.ext.reach + 30.0;
    let lo = (c - Vec2::splat(ext)).max(Vec2::ZERO);
    let hi = (c + Vec2::splat(ext)).min(Vec2::splat(ground.size()));
    let (w, h) = ((hi.x - lo.x) as usize, (hi.y - lo.y) as usize);
    // Squares first: their edges are frontage like a street's.
    squares(plan, dist, spec, ground, rng);
    for sq in plan.plazas.clone() {
        let v = Vec2::new(-sq.u.y, sq.u.x);
        let corners = [sq.c - sq.u * sq.r - v * sq.w, sq.c + sq.u * sq.r - v * sq.w, sq.c + sq.u * sq.r + v * sq.w, sq.c - sq.u * sq.r + v * sq.w];
        let style = dist.at(sq.c).map_or(Style::Maze, |d| dist.style(d));
        for k in 0..4 {
            // Each side wound so its outward side is the plots' side.
            let (a, b) = (corners[(k + 1) % 4], corners[k]);
            let n = (a.distance(b) / 4.0).ceil().max(1.0) as usize;
            let pts: Vec<Vec2> = (0..=n).map(|j| a.lerp(b, j as f32 / n as f32)).collect();
            let wet = vec![false; pts.len()];
            plan.ways.push(Way { pts, rank: rank::STREET, style, half: 0.0, walk: 0.0, wet, drawn: false });
        }
    }
    let mut r = Raster {
        lo,
        w,
        h,
        owner: vec![u32::MAX; w * h],
        dist: vec![f32::MAX; w * h],
        along: vec![0.0; w * h],
        wd: vec![60.0; w * h],
        used: vec![FREE; w * h],
    };
    for i in 0..w * h {
        let p = r.centre(i);
        if ground.wet(p) {
            r.used[i] = WET;
        } else if ground.taken(p) {
            r.used[i] = TAKEN;
        }
    }
    // How far each cell is from the water: the waterfront is built up
    // whatever district it is in.
    {
        let mut q = std::collections::VecDeque::new();
        for i in 0..w * h {
            if r.used[i] == WET {
                r.wd[i] = 0.0;
                q.push_back(i);
            }
        }
        while let Some(i) = q.pop_front() {
            let (x, z) = (i % w, i / w);
            let d = r.wd[i] + 1.0;
            if d >= 60.0 {
                continue;
            }
            for (dx, dz) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (xx, zz) = (x as i64 + dx, z as i64 + dz);
                if xx < 0 || zz < 0 || xx >= w as i64 || zz >= h as i64 {
                    continue;
                }
                let j = zz as usize * w + xx as usize;
                if r.wd[j] > d {
                    r.wd[j] = d;
                    q.push_back(j);
                }
            }
        }
    }
    // Who owns each cell, and where the streets themselves are.
    const REACH: f32 = 45.0;
    for (wi, way) in plan.ways.iter().enumerate() {
        let edge = way.half + way.walk;
        let mut run = 0.0;
        for s in way.pts.windows(2) {
            let (a, b) = (s[0], s[1]);
            let seg = a.distance(b);
            run += seg;
            let start = run - seg;
            let blo = a.min(b) - Vec2::splat(REACH);
            let bhi = a.max(b) + Vec2::splat(REACH);
            let (x0, z0) = (((blo.x - lo.x).max(0.0)) as usize, ((blo.y - lo.y).max(0.0)) as usize);
            let (x1, z1) = ((((bhi.x - lo.x).max(0.0)) as usize).min(w), (((bhi.y - lo.y).max(0.0)) as usize).min(h));
            for z in z0..z1 {
                for x in x0..x1 {
                    let i = z * w + x;
                    let p = r.centre(i);
                    let (d, _, t) = seg_foot(a, b, p);
                    if way.drawn && d <= edge + 0.5 && r.used[i] != WET {
                        r.used[i] = STREET;
                    }
                    if way.rank != rank::ALLEY && d - edge < r.dist[i] {
                        r.dist[i] = d - edge;
                        r.along[i] = start + t * seg;
                        let side = ((b - a).perp_dot(p - a) > 0.0) as u32;
                        r.owner[i] = wi as u32 * 2 + side;
                    }
                }
            }
        }
    }
    // Turning circles keep their ground.
    for &p in &plan.turnings.clone() {
        mark_disc(&mut r, p, 8.0, STREET);
    }
    // The market and the squares at the busiest junctions of the dense
    // districts.
    for sq in plan.plazas.clone() {
        mark_square(&mut r, &sq, OPEN);
    }
    // The town wall and a strip of open ground along it: nothing is built
    // against the wall on the outside, and the ring road runs inside it.
    for b in plan.boundaries.iter().filter(|b| b.edge == Edge::TownWall) {
        for &p in &b.pts {
            mark_disc(&mut r, p, 3.2, OPEN);
        }
    }

    // Churches on the squares, facing into them, the market's first.
    let squares: Vec<super::Square> = plan.plazas.clone();
    for (k, sq) in squares.iter().take(spec.features.churches as usize).enumerate() {
        if !church(plan, &mut r, dist, sq.c, sq.w, ground, rng) {
            // The market's church is worth a warning; a small square with
            // no room for one is only worth saying.
            let msg = format!("no room for a church on square {k}");
            if k == 0 {
                plan.notes.push(msg);
            } else {
                plan.notes_quiet.push(msg);
            }
        }
    }

    // Landmarks where the caller asked.
    landmarks(plan, &mut r, dist, spec, ground, rng);

    // Parks in the outer districts.
    {
        let n = spec.features.parks;
        for k in 0..n {
            for _ in 0..40 {
                let a = rng.f() * std::f32::consts::TAU;
                let rr = rng.range(dist.r_core + 30.0, spec.radius * 0.85);
                let p = c + Vec2::new(a.cos(), a.sin()) * rr;
                let pr = rng.range(24.0, 36.0) + if k == 0 { 6.0 } else { 0.0 };
                if dist.at(p).is_none_or(|d| d == 0) {
                    continue;
                }
                if plan.parks.iter().any(|(q, qr)| q.distance(p) < qr + pr + 60.0) || ground.wet(p) {
                    continue;
                }
                mark_disc(&mut r, p, pr, OPEN);
                plan.parks.push((p, pr));
                for _ in 0..(pr * pr * 0.02) as usize {
                    let a = rng.f() * std::f32::consts::TAU;
                    let q = p + Vec2::new(a.cos(), a.sin()) * pr * rng.f().sqrt() * 0.85;
                    if r.idx(q).is_some_and(|i| r.used[i] == OPEN) {
                        plan.trees.push((q, Tree::Park));
                    }
                }
                break;
            }
        }
    }

    // Towers along the town wall, and a pair at every gap a road goes
    // through.
    towers(plan, &mut r, dist, rng);

    // The dense districts as closed blocks.
    perimeter(plan, &mut r, dist, spec, ground, rng);

    // Plots along every frontage, the most important streets first.
    let mut order: Vec<usize> = (0..plan.ways.len()).collect();
    order.sort_by_key(|&k| (plan.ways[k].rank, (plan.ways[k].pts.len() as i64) * -1));
    for wi in order {
        let way = plan.ways[wi].clone();
        for side in [1.0f32, -1.0] {
            frontage(plan, &mut r, dist, spec, &way, wi, side, ground, rng);
        }
    }

    // Footpaths to the doors of the structures on the street plots, then
    // more houses on the ground those leave, each with its own path.
    if spec.features.paths {
        for s in plan.sites.clone() {
            if let Some(cells) = door_path(&r, dist, ground, s.at, s.turns, s.size) {
                lay_path(plan, &mut r, &cells);
            }
        }
    }
    if spec.features.infill > 0 && !spec.features.houses.is_empty() {
        // With no footprint houses to draw, their plots are free ground.
        if !spec.features.footprints {
            for u in r.used.iter_mut().filter(|u| **u == BUILT) {
                *u = FREE;
            }
            for s in &plan.sites {
                let h = s.size * 0.5 + Vec2::splat(0.3);
                let quad = [s.at - h, Vec2::new(s.at.x + h.x, s.at.y - h.y), s.at + h, Vec2::new(s.at.x - h.x, s.at.y + h.y)];
                for i in quad_cells(&r, &quad).unwrap_or_default() {
                    r.used[i] = BUILT;
                }
            }
            plan.buildings.clear();
        }
        infill(plan, &mut r, dist, spec, ground, rng);
    }

    // A turning circle with no houses round it was a close nobody built.
    let turns = std::mem::take(&mut plan.turnings);
    plan.turnings = turns
        .into_iter()
        .filter(|&p| plan.buildings.iter().filter(|b| b.poly[0].distance(p) < 40.0).count() >= 2)
        .collect();

    // Street trees along the main roads and the grid's wide streets.
    for way in &plan.ways {
        // Trees line the main roads and the ring, not every street: a row of
        // crowns down every grid street hides the town.
        if way.rank > rank::RING || !matches!(way.style, Style::Grid | Style::Radial | Style::Suburb) {
            continue;
        }
        let mut s = 4.0;
        let len: f32 = way.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        while s < len - 4.0 {
            let (p, t) = point_at(&way.pts, s);
            let n = Vec2::new(-t.y, t.x);
            for side in [1.0f32, -1.0] {
                let q = p + n * side * (way.half + 0.6);
                if r.idx(q).is_some_and(|i| r.used[i] != WET) && dist.at(q).is_some() && !way.wet.iter().any(|&x| x) {
                    plan.trees.push((q, Tree::Street));
                }
            }
            s += 15.0;
        }
    }
}

fn mark_square(r: &mut Raster, sq: &super::Square, v: u8) {
    let reach = sq.r.max(sq.w) + 1.5;
    for z in 0..(reach * 2.0) as i32 {
        for x in 0..(reach * 2.0) as i32 {
            let q = sq.c - Vec2::splat(reach) + Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            if sq.contains(q, 0.0) {
                if let Some(i) = r.idx(q) {
                    if r.used[i] != WET && r.used[i] != TAKEN {
                        r.used[i] = v;
                    }
                }
            }
        }
    }
}

fn mark_disc(r: &mut Raster, p: Vec2, rad: f32, v: u8) {
    let lo = p - Vec2::splat(rad + 1.0);
    for z in 0..(rad * 2.0 + 3.0) as i32 {
        for x in 0..(rad * 2.0 + 3.0) as i32 {
            let q = lo + Vec2::new(x as f32, z as f32);
            if q.distance(p) <= rad {
                if let Some(i) = r.idx(q) {
                    if r.used[i] != WET && r.used[i] != TAKEN {
                        r.used[i] = v;
                    }
                }
            }
        }
    }
}

/// The point `s` m along a polyline and the direction there.
pub fn point_at(pts: &[Vec2], s: f32) -> (Vec2, Vec2) {
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if acc + l >= s && l > 1e-4 {
            let t = (s - acc) / l;
            return (w[0].lerp(w[1], t), (w[1] - w[0]) / l);
        }
        acc += l;
    }
    let n = pts.len();
    (pts[n - 1], (pts[n - 1] - pts[n.saturating_sub(2)]).normalize_or_zero())
}

/// Cells of a convex quad.
fn quad_cells(r: &Raster, q: &[Vec2; 4]) -> Option<Vec<usize>> {
    let lo = q.iter().fold(Vec2::splat(f32::MAX), |a, &b| a.min(b));
    let hi = q.iter().fold(Vec2::splat(f32::MIN), |a, &b| a.max(b));
    let mut out = Vec::new();
    // Winding-independent inside test for a convex quad.
    let inside = |p: Vec2| {
        let mut sgn = 0.0f32;
        for k in 0..4 {
            let (a, b) = (q[k], q[(k + 1) % 4]);
            let c = (b - a).perp_dot(p - a);
            if c.abs() < 1e-6 {
                continue;
            }
            if sgn == 0.0 {
                sgn = c.signum();
            } else if sgn != c.signum() {
                return false;
            }
        }
        true
    };
    let mut z = lo.y.floor();
    while z <= hi.y {
        let mut x = lo.x.floor();
        while x <= hi.x {
            let p = Vec2::new(x + 0.5, z + 0.5);
            if inside(p) {
                out.push(r.idx(p)?);
            }
            x += 1.0;
        }
        z += 1.0;
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
fn frontage(plan: &mut Plan, r: &mut Raster, dist: &Districts, spec: &Spec, way: &Way, wi: usize, side: f32, ground: &dyn Ground, rng: &mut Rng) {
    let len: f32 = way.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let me = wi as u32 * 2 + (side > 0.0) as u32;
    let mut s = 0.0;
    let mut row_floors: Option<u8> = None;
    while s < len - 3.0 {
        let (p_mid, _) = point_at(&way.pts, s);
        let (_, t0) = point_at(&way.pts, s + 1.0);
        let probe = p_mid + Vec2::new(-t0.y, t0.x) * side * (way.half + way.walk + 3.0);
        let Some(d) = dist.at(probe) else {
            plan.stats.outside += 1;
            s += 4.0;
            row_floors = None;
            continue;
        };
        let style = dist.style(d);
        let far = dist.far(probe);
        if dense_cell(dist, r, probe).is_some() {
            s += 4.0;
            continue;
        }
        let ls = lot_style(style, way.rank, far);
        let mut wdt = rng.range(ls.width.0, ls.width.1);
        // A library house turns only by quarter turns, so it stands only
        // where the street runs within 20 degrees of north-south or
        // east-west; its plot is as wide as its turned footprint needs.
        let detached = matches!(style, Style::Village | Style::Ribbon | Style::Suburb);
        let mut house: Option<(usize, u8, Vec2)> = None;
        if detached && !spec.features.houses.is_empty() {
            let face = -Vec2::new(-t0.y, t0.x) * side;
            let turns = super::quarter_turns(face);
            let mis = face.angle_to(super::quarter_axis(turns)).abs();
            if mis < 20f32.to_radians() {
                let k = (rng.f() * spec.features.houses.len() as f32) as usize % spec.features.houses.len();
                let sz = spec.features.houses[k].size;
                let ws = if turns % 2 == 1 { Vec2::new(sz.y, sz.x) } else { sz };
                // Along the street, the turned box spans this much.
                let span = (ws.x * t0.x).abs() + (ws.y * t0.y).abs();
                wdt = span + 2.0 * rng.range(ls.side.0.max(1.0), ls.side.1.max(2.5)).max(spec.gaps * 0.5);
                house = Some((k, turns, ws));
            }
        }
        if s + wdt > len {
            // The last plot on a street may be a little narrower.
            if len - s >= ls.width.0 * 0.8 {
                wdt = len - s - 0.5;
            } else {
                break;
            }
        }
        let (p0, _) = point_at(&way.pts, s);
        let (p1, _) = point_at(&way.pts, s + wdt);
        let t = (p1 - p0).normalize_or_zero();
        let n = Vec2::new(-t.y, t.x) * side;
        let setback = rng.range(ls.setback.0, ls.setback.1);
        let o = way.half + way.walk + 0.4 + setback;
        let f0 = p0 + n * o;
        let f1 = p1 + n * o;
        // How deep the plot is: back to where the ground stops being ours,
        // measured where the building will stand, inside its side gaps.
        let gap = rng.range(ls.side.0, ls.side.1);
        let inset = ((gap + 0.5) / wdt.max(1.0)).min(0.4);
        let mut depth = f32::MAX;
        for a in [inset, 0.5, 1.0 - inset] {
            let f = f0.lerp(f1, a);
            let mut k = 0.0;
            // A suburban garden runs back to the plot behind; elsewhere the
            // probe need only find room for the building.
            let reach = if style == Style::Suburb { 34.0 } else { ls.depth.1 + 2.0 };
            while k < reach {
                let q = f + n * (k + 0.5);
                let Some(i) = r.idx(q) else { break };
                if r.owner[i] != me || r.used[i] != FREE {
                    break;
                }
                k += 1.0;
            }
            depth = depth.min(k);
        }
        depth -= 0.5;
        if depth < (ls.depth.0 * 0.5).max(5.5) {
            plan.stats.shallow += 1;
            s += 1.0;
            row_floors = None;
            continue;
        }
        // Out toward the edge, away from the roads in, more plots stand
        // empty.
        if rng.f() > keep(dist, spec, f0.lerp(f1, 0.5)) {
            s += wdt;
            row_floors = None;
            continue;
        }
        if let Some((k, turns, ws)) = house {
            // The box set back so its nearest corner is at the building
            // line, checked cell by cell like a landmark.
            let reach = (ws.x * n.x).abs() * 0.5 + (ws.y * n.y).abs() * 0.5;
            let at = f0.lerp(f1, 0.5) + n * reach;
            let h = ws * 0.5 + Vec2::splat(0.3);
            let quad = [at - h, Vec2::new(at.x + h.x, at.y - h.y), at + h, Vec2::new(at.x - h.x, at.y + h.y)];
            let fits = quad_cells(r, &quad).filter(|c| !c.is_empty() && c.iter().all(|&i| r.used[i] == FREE && r.owner[i] == me));
            let fall = super::fall(ground, &quad);
            match fits {
                Some(cells) if fall <= ls.fall + 0.5 => {
                    for &i in &cells {
                        r.used[i] = BUILT;
                    }
                    plan.sites.push(super::Site { id: spec.features.houses[k].id, at, turns, size: ws });
                    plan.stats.built += 1;
                    s += wdt;
                }
                _ => {
                    plan.stats.taken += 1;
                    s += 1.5;
                }
            }
            continue;
        }
        let bd = depth.min(rng.range(ls.depth.0, ls.depth.1));
        let (a0, a1) = (f0 + t * gap, f1 - t * gap);
        if a0.distance(a1) < 4.5 {
            s += 1.0;
            continue;
        }
        let quad = [a0, a1, a1 + n * bd, a0 + n * bd];
        let Some(cells) = quad_cells(r, &quad) else {
            plan.stats.edge += 1;
            s += 1.0;
            continue;
        };
        if cells.is_empty() || cells.iter().any(|&i| r.used[i] != FREE) {
            plan.stats.taken += 1;
            s += 1.0;
            row_floors = None;
            continue;
        }
        let fall = super::fall(ground, &quad);
        if fall > ls.fall {
            plan.stats.steep += 1;
            s += 2.0;
            row_floors = None;
            continue;
        }
        for &i in &cells {
            r.used[i] = BUILT;
        }
        // Neighbours in a row keep near one height, stepping now and then.
        let floors = match row_floors {
            Some(f) if rng.f() < 0.6 => f,
            _ => ls.floors.0 + (rng.f() * (ls.floors.1 - ls.floors.0 + 1) as f32) as u8,
        }
        .clamp(ls.floors.0, ls.floors.1);
        row_floors = Some(floors);
        let terraced = ls.side.1 < 0.8;
        let kind = match style {
            Style::Suburb | Style::Village | Style::Ribbon => Kind::House,
            _ if way.rank <= rank::RING && rng.f() < 0.7 => Kind::Shop,
            _ if terraced => Kind::Row,
            _ => Kind::House,
        };
        let roof = roof_for(style, far, rng);
        let mut poly = quad.to_vec();
        if style == Style::Suburb {
            // A garage beside the house, set back a little, with a drive
            // to the street; the back yard fenced.
            let hw = a0.distance(a1);
            if hw > 15.0 && rng.f() < 0.7 {
                let gw = 6.0;
                let right = rng.f() < 0.5;
                let (h0, h1, g0) = if right { (a0, a1 - t * gw, a1 - t * gw) } else { (a0 + t * gw, a1, a0) };
                poly = vec![h0, h1, h1 + n * bd, h0 + n * bd];
                let gq = vec![g0 + n * 1.2, g0 + t * gw + n * 1.2, g0 + t * gw + n * 7.6, g0 + n * 7.6];
                let gf = g0 + t * (gw * 0.5) + n * 1.2;
                plan.drives.push((gf - n * (o - way.half + 1.6), gf));
                plan.buildings.push(Building {
                    poly: gq,
                    front: 0,
                    kind: Kind::House,
                    roof: Roof::Gable,
                    floors: 1,
                    style,
                    seed: (rng.f() * 4e9) as u32,
                    ridge: Some(n),
                    tone: rng.f().to_bits(),
                });
            }
            // A tree on most front lawns.
            if rng.f() < 0.6 && setback > 4.5 {
                let side_t = if rng.f() < 0.5 { 0.25 } else { 0.75 };
                plan.trees.push((f0.lerp(f1, side_t) - n * (setback * 0.5), Tree::Park));
            }
            if spec.features.yards {
                let back = depth + 0.5;
                let (b0, b1) = (f0 + n * back, f1 + n * back);
                // Behind back-to-back gardens one fence will do: the plot
                // whose neighbour behind has the higher number builds it.
                let beyond = r.idx(f0.lerp(f1, 0.5) + n * (back + 1.5)).map_or(u32::MAX, |i| r.owner[i]);
                if beyond == u32::MAX || beyond >= me {
                    plan.boundaries.push(Boundary { pts: vec![b0, b0.lerp(b1, 0.5), b1], edge: Edge::Garden });
                }
                // The side fence from the house's front to the back.
                let start = f0 + n * 2.0;
                if start.distance(b0) > 4.0 {
                    plan.boundaries.push(Boundary { pts: vec![start, start.lerp(b0, 0.5), b0], edge: Edge::Garden });
                }
            }
        }
        plan.buildings.push(Building {
            poly,
            front: 0,
            kind,
            roof,
            floors,
            style,
            seed: (rng.f() * 4e9) as u32,
            ridge: None,
            tone: rng.f().to_bits(),
        });
        plan.stats.built += 1;
        s += wdt;
    }
}

/// The chance a plot here is built on: always deep in the town and along
/// the roads in from the country, less and less toward the edge away from
/// them, so houses cluster on the roads and thin out between.
fn keep(dist: &Districts, spec: &Spec, p: Vec2) -> f32 {
    // A suburb is planned and built out; only a town that grew thins.
    if !spec.gradient || dist.at(p).is_some_and(|d| dist.style(d) == Style::Suburb) {
        return 1.0;
    }
    // Only the fringe thins: the last sixth of the radius or so, outside the
    // corridors along the roads in.
    let depth = (-dist.sd(p) / (dist.radius * 0.16)).clamp(0.0, 1.0);
    let road = (1.0 - dist.ext.road(p) / 70.0).clamp(0.0, 1.0);
    // The waterfront is wanted ground too.
    let water = (1.0 - dist.ext.water(p) / 60.0).clamp(0.0, 1.0);
    (0.2 + 0.8 * depth.max(road).max(water)).min(1.0)
}

/// The church: the biggest building on the plaza, long axis pointing at
/// the centre, its door on the plaza.
fn church(plan: &mut Plan, r: &mut Raster, dist: &Districts, pc: Vec2, pr: f32, ground: &dyn Ground, rng: &mut Rng) -> bool {
    let (len0, wid0) = (rng.range(24.0, 32.0), rng.range(11.0, 14.0));
    let a0 = rng.f() * std::f32::consts::TAU;
    // Smaller churches and ones standing further into the square are tried
    // only when a big one at the edge won't go.
    for scale in [1.0f32, 0.8, 0.65, 0.5] {
        let (len, wid) = (len0 * scale, wid0 * scale);
        for inset in [0.0f32, 0.4, 0.75] {
            for k in 0..32 {
                let a = a0 + k as f32 * std::f32::consts::TAU / 32.0;
                let out = Vec2::new(a.cos(), a.sin());
                let side = Vec2::new(-out.y, out.x);
                // The west door faces the square; the church may stand on
                // its edge, but leaves the middle clear.
                let front = pc + out * (pr + 1.0 - len * inset).max(8.0);
                let quad = [front - side * wid * 0.5, front + side * wid * 0.5, front + side * wid * 0.5 + out * len, front - side * wid * 0.5 + out * len];
                let Some(cells) = quad_cells(r, &quad) else { continue };
                if cells.iter().any(|&i| r.used[i] != FREE && r.used[i] != OPEN) || dist.at(front + out * len).is_none() {
                    continue;
                }
                if super::fall(ground, &quad) > 3.0 {
                    continue;
                }
                for &i in &cells {
                    r.used[i] = BUILT;
                }
                plan.buildings.push(Building {
                    poly: quad.to_vec(),
                    front: 0,
                    kind: Kind::Church,
                    roof: Roof::Gable,
                    floors: 4,
                    style: dist.style(0),
                    seed: (rng.f() * 4e9) as u32,
                    ridge: None,
                    tone: rng.f().to_bits(),
                });
                return true;
            }
        }
    }
    false
}

/// The wall round the core, a few metres outside the ring road, with a gap
/// wherever a street passes through.
pub fn town_wall(dist: &Districts, ways: &[Way], ground: &dyn Ground) -> Vec<Boundary> {
    let n = 360;
    let mut out = Vec::new();
    let mut run: Vec<Vec2> = Vec::new();
    for k in 0..=n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let p = dist.c + d * (dist.ring_r(d) + dist.wall.max(8.5));
        let clear = !ground.wet(p)
            && ways.iter().filter(|w| w.rank != rank::RING).all(|w| {
                w.pts.windows(2).all(|s| seg_foot(s[0], s[1], p).0 > w.half + w.walk + 2.5)
            });
        if clear {
            run.push(p);
        } else if run.len() >= 3 {
            out.push(Boundary { pts: std::mem::take(&mut run), edge: Edge::TownWall });
        } else {
            run.clear();
        }
    }
    if run.len() >= 3 {
        out.push(Boundary { pts: run, edge: Edge::TownWall });
    }
    out
}

/// Square towers on the town wall: every 40 m or so, and one each side of
/// every gap.
fn towers(plan: &mut Plan, r: &mut Raster, dist: &Districts, rng: &mut Rng) {
    let walls: Vec<Vec<Vec2>> = plan.boundaries.iter().filter(|b| b.edge == Edge::TownWall).map(|b| b.pts.clone()).collect();
    for pts in walls {
        let len: f32 = pts.windows(2).map(|s| s[0].distance(s[1])).sum();
        let n = (len / 40.0).round().max(1.0) as usize;
        for k in 0..=n {
            let s = (len * k as f32 / n as f32).clamp(2.5, (len - 2.5).max(2.5));
            let (p, t) = point_at(&pts, s);
            let nn = Vec2::new(-t.y, t.x);
            let gate = k == 0 || k == n;
            let h = if gate { 4.4 } else { 2.9 };
            let quad = [p - t * h - nn * h, p + t * h - nn * h, p + t * h + nn * h, p - t * h + nn * h];
            if let Some(cells) = quad_cells(r, &quad) {
                for &i in &cells {
                    r.used[i] = BUILT;
                }
            }
            plan.buildings.push(Building {
                poly: quad.to_vec(),
                front: 0,
                kind: Kind::Tower,
                roof: if gate || rng.f() < 0.3 { Roof::Hip } else { Roof::Flat },
                floors: if gate { 4 } else { 3 },
                style: dist.style(0),
                seed: (rng.f() * 4e9) as u32,
                ridge: None,
                tone: rng.f().to_bits(),
            });
        }
    }
}

/// Is a point in a district built as closed blocks (terraces round a yard)
/// rather than houses in gardens? Its style and how far out it is.
fn dense_at(dist: &Districts, p: Vec2) -> Option<(Style, f32)> {
    let d = dist.at(p)?;
    let style = dist.style(d);
    let far = dist.far(p);
    matches!(style, Style::Maze | Style::Radial | Style::Grid).then_some((style, far))
}

/// `dense_at`, and the waterfront too: within 40 m of the water a suburb
/// or a village is built up as closed blocks like the grid.
fn dense_cell(dist: &Districts, r: &Raster, p: Vec2) -> Option<(Style, f32)> {
    dense_at(dist, p).or_else(|| {
        let i = r.idx(p)?;
        let d = dist.at(p)?;
        let far = dist.far(p);
        (dist.radius >= 120.0 && r.wd[i] < 40.0 && matches!(dist.style(d), Style::Suburb | Style::Village | Style::Ribbon)).then_some((Style::Grid, far.min(0.7)))
    })
}

/// Convex hull (monotone chain), counter-clockwise.
/// The tidied convex hull of a set of raster cells, corner to corner.
fn cell_hull(r: &Raster, cells: &[usize]) -> Vec<Vec2> {
    let mut pts = Vec::with_capacity(cells.len() * 4);
    for &i in cells {
        let c = r.centre(i);
        for d in [Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5)] {
            pts.push(c + d);
        }
    }
    tidy(hull(pts))
}

fn hull(mut pts: Vec<Vec2>) -> Vec<Vec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<Vec2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Drop corners that barely turn, so a footprint has a handful of walls.
fn tidy(poly: Vec<Vec2>) -> Vec<Vec2> {
    let mut p = poly;
    loop {
        let n = p.len();
        if n <= 4 {
            return p;
        }
        let mut worst = (usize::MAX, f32::MAX);
        for k in 0..n {
            let (a, b, c) = (p[(k + n - 1) % n], p[k], p[(k + 1) % n]);
            // How far b stands off the line a-c.
            let d = seg_foot(a, c, b).0;
            if d < worst.1 {
                worst = (k, d);
            }
        }
        if worst.1 > 0.6 {
            return p;
        }
        p.remove(worst.0);
    }
}

fn area(poly: &[Vec2]) -> f32 {
    let n = poly.len();
    (0..n).map(|k| poly[k].perp_dot(poly[(k + 1) % n])).sum::<f32>().abs() * 0.5
}

fn roof_for(style: Style, far: f32, rng: &mut Rng) -> Roof {
    match style {
        Style::Maze => {
            if rng.f() < 0.85 {
                Roof::Gable
            } else {
                Roof::Hip
            }
        }
        Style::Radial => match rng.f() {
            x if x < 0.45 => Roof::Hip,
            x if x < 0.75 => Roof::Flat,
            _ => Roof::Gable,
        },
        Style::Grid => match rng.f() {
            x if x < 0.4 && far < 0.6 => Roof::Flat,
            x if x < 0.75 => Roof::Gable,
            _ => Roof::Hip,
        },
        _ => {
            if rng.f() < 0.6 {
                Roof::Gable
            } else {
                Roof::Hip
            }
        }
    }
}

/// Closed blocks: in the dense districts every frontage is built up for a
/// plot's depth all round the block, leaving a yard in the middle. The band
/// along each frontage is cut into strips square to the street, a plot's
/// width apart; a strip is the cells whose nearest street is that one and
/// whose foot on it falls in that stretch, so plots meet their neighbours
/// exactly, and the two part-strips that meet at a corner join into one
/// corner building. Without `spec.irregular`, a corner building is cut back to the
/// largest rectangle square to its street instead of filling the corner.
fn perimeter(plan: &mut Plan, r: &mut Raster, dist: &Districts, spec: &Spec, ground: &dyn Ground, rng: &mut Rng) {
    use crate::util::FxHashMap as HashMap;
    // Where each way side is cut into plots.
    let mut cuts: Vec<[Vec<f32>; 2]> = Vec::with_capacity(plan.ways.len());
    for way in &plan.ways {
        let len: f32 = way.pts.windows(2).map(|s| s[0].distance(s[1])).sum();
        let mid = point_at(&way.pts, len * 0.5).0;
        let style = dist.at(mid).map(|d| dist.style(d)).unwrap_or(Style::Village);
        let far = dist.far(mid);
        let ls = lot_style(style, way.rank, far);
        let mut sides: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        for side in sides.iter_mut() {
            let mut s = 0.0;
            side.push(s);
            while s < len {
                s += rng.range(ls.width.0, ls.width.1);
                side.push(s);
            }
        }
        cuts.push(sides);
    }
    // Strips of cells, keyed by way side and plot.
    let mut groups: HashMap<(u32, u32), Vec<usize>> = HashMap::default();
    let mut yards: HashMap<(u32, u32), Vec<usize>> = HashMap::default();
    let mut depth_of: HashMap<(u32, u32), f32> = HashMap::default();
    for i in 0..r.used.len() {
        if r.used[i] != FREE || r.owner[i] == u32::MAX {
            continue;
        }
        let o = r.owner[i];
        let way = &plan.ways[(o / 2) as usize];
        if way.rank == rank::COUNTRY {
            continue;
        }
        let p = r.centre(i);
        let Some((style, far)) = dense_cell(dist, r, p) else { continue };
        let kd = r.dist[i];
        if kd < 0.3 {
            continue;
        }
        let list = &cuts[(o / 2) as usize][(o % 2) as usize];
        let bin = list.partition_point(|&c| c <= r.along[i]).saturating_sub(1) as u32;
        // A small town's houses stand a little apart.
        if spec.gaps > 0.0 {
            let c0 = list[bin as usize];
            let c1 = list.get(bin as usize + 1).copied().unwrap_or(f32::MAX);
            if r.along[i] - c0 < spec.gaps * 0.5 || c1 - r.along[i] < spec.gaps * 0.5 {
                continue;
            }
        }
        let key = (o, bin);
        let ls = lot_style(style, way.rank, far);
        let depth = *depth_of.entry(key).or_insert_with(|| rng.range(ls.depth.0, ls.depth.1));
        if kd > depth {
            // A deep yard may have a workshop or a coach house at its back.
            if kd > depth + 4.0 && kd < depth + 11.0 {
                yards.entry(key).or_default().push(i);
            }
            continue;
        }
        groups.entry(key).or_default().push(i);
    }
    // Part-strips (at corners, at a bend) join the neighbour they share the
    // most edge with.
    let mut keys: Vec<(u32, u32)> = groups.keys().copied().collect();
    keys.sort_unstable();
    let mut cell_group: HashMap<usize, (u32, u32)> = HashMap::default();
    for k in &keys {
        for &i in &groups[k] {
            cell_group.insert(i, *k);
        }
    }
    let mut by_size = keys.clone();
    by_size.sort_by_key(|k| groups[k].len());
    let mut corner: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for k in by_size {
        let Some(cells) = groups.get(&k) else { continue };
        if cells.len() >= 28 {
            continue;
        }
        let mut count: HashMap<(u32, u32), usize> = HashMap::default();
        for &i in cells {
            for j in [i.wrapping_sub(1), i + 1, i.wrapping_sub(r.w), i + r.w] {
                if let Some(&g) = cell_group.get(&j) {
                    if g != k {
                        *count.entry(g).or_default() += 1;
                    }
                }
            }
        }
        // Never into a neighbour already a full plot or more: a chain of
        // joins would make one building of half a block.
        let room = |g: &(u32, u32)| groups.get(g).map_or(0, |c| c.len()) + cells.len() <= 230;
        let best = count.into_iter().filter(|(g, _)| room(g)).max_by_key(|(g, n)| (*n, std::cmp::Reverse(*g)));
        let cells = groups.remove(&k).unwrap();
        match best {
            Some((g, n)) if n >= 3 => {
                for &i in &cells {
                    cell_group.insert(i, g);
                }
                // Joining a strip of another street makes a corner.
                if g.0 / 2 != k.0 / 2 {
                    corner.insert(g);
                }
                groups.get_mut(&g).unwrap().extend(cells);
            }
            _ => {
                for i in cells {
                    cell_group.remove(&i);
                }
            }
        }
    }
    let mut keys: Vec<(u32, u32)> = groups.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        let cells = &groups[&key];
        if cells.len() < 24 {
            continue;
        }
        let way = &plan.ways[(key.0 / 2) as usize];
        let p0 = r.centre(cells[0]);
        let Some((style, far)) = dense_cell(dist, r, p0) else { continue };
        if rng.f() > keep(dist, spec, p0) {
            continue;
        }
        let ls = lot_style(style, way.rank, far);
        let mut poly = cell_hull(r, cells);
        let fill = cells.len() as f32 / area(&poly).max(1.0);
        // A hull much bigger than its cells reaches over the plots beside
        // it; square it off whatever `irregular` says.
        // And a hull of many corners is a blob or a drum from a curving
        // lane: square that off too.
        let round = poly.len() > 8;
        if (fill < 0.82 && !spec.irregular) || fill < 0.72 || round {
            // Square to the street: the cells' extent along and across it,
            // shrunk from whichever end is emptiest until it is nearly full.
            let mean = cells.iter().map(|&i| r.along[i]).sum::<f32>() / cells.len() as f32;
            let (_, t) = point_at(&way.pts, mean);
            let n = Vec2::new(-t.y, t.x);
            let origin = r.centre(cells[0]);
            let uv: Vec<Vec2> = cells.iter().map(|&i| {
                let q = r.centre(i) - origin;
                Vec2::new(q.dot(t), q.dot(n))
            }).collect();
            let (mut lo, mut hi) = uv.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(l, h), &q| (l.min(q), h.max(q)));
            for _ in 0..30 {
                let inside = uv.iter().filter(|q| q.x >= lo.x && q.x <= hi.x && q.y >= lo.y && q.y <= hi.y).count() as f32;
                let rect = ((hi.x - lo.x + 1.0) * (hi.y - lo.y + 1.0)).max(1.0);
                if inside / rect >= 0.9 {
                    break;
                }
                // Count cells in the outermost metre of each side; cut the
                // thinnest.
                let edge = |f: &dyn Fn(&Vec2) -> bool| uv.iter().filter(|q| q.x >= lo.x && q.x <= hi.x && q.y >= lo.y && q.y <= hi.y && f(q)).count();
                let sides = [
                    edge(&|q: &Vec2| q.x < lo.x + 1.0) as f32 / (hi.y - lo.y + 1.0),
                    edge(&|q: &Vec2| q.x > hi.x - 1.0) as f32 / (hi.y - lo.y + 1.0),
                    edge(&|q: &Vec2| q.y > hi.y - 1.0) as f32 / (hi.x - lo.x + 1.0),
                ];
                let k = (0..3).min_by(|&a, &b| sides[a].total_cmp(&sides[b])).unwrap();
                match k {
                    0 => lo.x += 1.0,
                    1 => hi.x -= 1.0,
                    _ => hi.y -= 1.0,
                }
            }
            if hi.x - lo.x < 4.0 || hi.y - lo.y < 4.0 {
                // No rectangle worth building: a round hull that only
                // looked odd keeps its shape; one that laps its neighbours
                // goes.
                if !(round && fill >= 0.72 && (spec.irregular || fill >= 0.82)) {
                    continue;
                }
            } else {
                let at = |u: f32, v: f32| origin + t * u + n * v;
                let (u0, u1, v0, v1) = (lo.x - 0.5, hi.x + 0.5, lo.y - 0.5, hi.y + 0.5);
                poly = vec![at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)];
            }
        }
        if poly.len() < 3 || area(&poly) < 22.0 {
            continue;
        }
        if super::fall(ground, &poly) > ls.fall {
            plan.stats.steep += 1;
            continue;
        }
        // The front: the wall nearest the street.
        let cen = poly.iter().copied().sum::<Vec2>() / poly.len() as f32;
        let m = poly.len();
        let front = (0..m)
            .min_by(|&a, &b| {
                let kd = |k: usize| {
                    let mid = (poly[k] + poly[(k + 1) % m]) * 0.5;
                    let q = mid + (cen - mid).normalize_or_zero() * 0.7;
                    r.idx(q).map_or(f32::MAX, |i| r.dist[i])
                };
                kd(a).total_cmp(&kd(b))
            })
            .unwrap_or(0);
        for &i in cells {
            r.used[i] = BUILT;
        }
        // A row keeps near one height: plots in threes share a roll.
        let h = crate::world::noise::hash32(key.0.wrapping_mul(0x9e37_79b9) ^ (key.1 / 3) ^ spec.seed);
        let span = (ls.floors.1 - ls.floors.0 + 1) as u32;
        let mut floors = ls.floors.0 + ((h >> 8) % span) as u8;
        if rng.f() < 0.25 {
            floors = (floors + 1).min(ls.floors.1);
        }
        // A small town is mostly two storeys, three on its main streets.
        if spec.radius < 200.0 {
            floors = if way.rank <= rank::RING && rng.f() < 0.5 { 3 } else { 2 };
        }
        // The yard behind: a fence along the plot's side, from the back of
        // the building to the back of the plot.
        // Yard fences only where the strips are straight: a planned grid or
        // rings. Off a winding lane they splay out at every angle.
        if spec.features.yards && key.1 > 0 && matches!(style, Style::Grid | Style::Radial) {
            let list = &cuts[(key.0 / 2) as usize][(key.0 % 2) as usize];
            let cut = list[key.1 as usize];
            let (p, t) = point_at(&way.pts, cut);
            let side = if key.0 % 2 == 1 { 1.0 } else { -1.0 };
            let n = Vec2::new(-t.y, t.x) * side;
            let edge = way.half + way.walk;
            let start = p + n * (edge + depth_of.get(&key).copied().unwrap_or(10.0) + 0.6);
            let mut pts = vec![start];
            let mut k = 1.0;
            while k < 25.0 {
                let q = start + n * k;
                let Some(i) = r.idx(q) else { break };
                if r.owner[i] / 2 != key.0 / 2 || r.used[i] != FREE || dense_cell(dist, r, q).is_none() {
                    break;
                }
                k += 1.0;
            }
            if k >= 4.0 {
                pts.push(start + n * (k - 0.6));
                plan.boundaries.push(Boundary { pts, edge: Edge::Garden });
            }
        }
        let is_corner = corner.contains(&key);
        if is_corner {
            floors = (floors + 1).min(ls.floors.1 + 1);
        }
        let quay = way.rank == rank::QUAY;
        let kind = if quay && rng.f() < 0.6 {
            Kind::Hall
        } else if way.rank <= rank::RING && rng.f() < 0.75 || is_corner && rng.f() < 0.5 {
            Kind::Shop
        } else {
            Kind::Row
        };
        let roof = if kind == Kind::Hall { Roof::Gable } else if is_corner && rng.f() < 0.6 { Roof::Hip } else { roof_for(style, far, rng) };
        // In a row the ridges run along the street, so neighbours' roofs
        // line up instead of standing gable to gable at random.
        let mean = cells.iter().map(|&i| r.along[i]).sum::<f32>() / cells.len() as f32;
        let ridge = (!is_corner && kind != Kind::Hall).then(|| point_at(&way.pts, mean).1);
        plan.buildings.push(Building {
            poly,
            front,
            kind,
            roof,
            floors: if kind == Kind::Hall { floors.min(3) } else { floors },
            style,
            seed: (rng.f() * 4e9) as u32,
            ridge,
            // Most take their street's tone.
            tone: if rng.f() < 0.85 { crate::world::noise::hash32(key.0 / 2 ^ spec.seed) } else { rng.f().to_bits() },
        });
        plan.stats.built += 1;
    }
    // Back buildings at the far end of the deepest yards.
    let mut ykeys: Vec<(u32, u32)> = yards.keys().copied().collect();
    ykeys.sort_unstable();
    for key in ykeys {
        let cells: Vec<usize> = yards[&key].iter().copied().filter(|&i| r.used[i] == FREE).collect();
        if cells.len() < 24 || rng.f() > 0.45 {
            continue;
        }
        let p0 = r.centre(cells[0]);
        let Some((style, _)) = dense_cell(dist, r, p0) else { continue };
        let poly = cell_hull(r, &cells);
        if poly.len() < 3 || area(&poly) < 18.0 || (cells.len() as f32) < area(&poly) * 0.75 {
            continue;
        }
        for &i in &cells {
            r.used[i] = BUILT;
        }
        plan.buildings.push(Building {
            poly,
            front: 0,
            kind: Kind::Row,
            roof: Roof::Gable,
            floors: 1 + (rng.f() < 0.3) as u8,
            style,
            seed: (rng.f() * 4e9) as u32,
            ridge: None,
            tone: crate::world::noise::hash32(key.0 / 2 ^ spec.seed),
        });
    }
}

/// Where two or more ways end at one point, with the ways' ranks.
fn junctions(plan: &Plan) -> Vec<(Vec2, Vec<u8>)> {
    let mut out: Vec<(Vec2, Vec<u8>)> = Vec::new();
    for w in &plan.ways {
        if w.rank == rank::COUNTRY || w.pts.len() < 2 {
            continue;
        }
        for p in [w.pts[0], *w.pts.last().unwrap()] {
            match out.iter_mut().find(|(q, _)| q.distance(p) < 0.75) {
                Some((_, ranks)) => ranks.push(w.rank),
                None => out.push((p, vec![w.rank])),
            }
        }
    }
    out.retain(|(_, r)| r.len() >= 3);
    out
}

/// Small squares where the most important streets of the dense districts
/// meet, spread apart and away from the market.
fn squares(plan: &mut Plan, dist: &Districts, spec: &Spec, ground: &dyn Ground, rng: &mut Rng) {
    if spec.features.squares == 0 {
        return;
    }
    let mut cands: Vec<(f32, Vec2)> = junctions(plan)
        .into_iter()
        .filter(|(p, _)| dense_at(dist, *p).is_some() && !ground.wet(*p))
        .map(|(p, ranks)| {
            // Busy junctions of main streets first.
            let weight: f32 = ranks.iter().map(|&k| if k <= rank::RING { 3.0 } else if k == rank::STREET { 2.0 } else { 1.0 }).sum();
            (weight + rng.f(), p)
        })
        .collect();
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let apart = (spec.radius * 0.3).max(60.0);
    let sr = (spec.radius * 0.05).clamp(9.0, 16.0);
    let mut placed = 0;
    for (_, p) in cands {
        if placed >= spec.features.squares {
            break;
        }
        if plan.plazas.iter().any(|q| q.c.distance(p) < apart.max(q.r + sr + 30.0)) {
            continue;
        }
        // Square to the busiest street through the junction.
        let u = plan
            .ways
            .iter()
            .filter(|w| w.pts.len() >= 2 && (w.pts[0].distance(p) < 1.0 || w.pts[w.pts.len() - 1].distance(p) < 1.0))
            .min_by_key(|w| w.rank)
            .map(|w| (w.pts[w.pts.len() / 2] - p).normalize_or_zero())
            .unwrap_or(Vec2::X);
        plan.plazas.push(super::Square { c: p, u, r: sr * 1.1, w: sr * 0.8 });
        placed += 1;
    }
    if placed < spec.features.squares {
        plan.notes.push(format!("room for {placed} of {} squares", spec.features.squares));
    }
}

/// Stand each landmark where the caller asked, square to the world (a
/// placed structure turns only by quarters), its front to what it is near.
fn landmarks(plan: &mut Plan, r: &mut Raster, dist: &Districts, spec: &Spec, ground: &dyn Ground, rng: &mut Rng) {
    use super::Near;
    let mut square_k = 1usize;
    for lm in &spec.features.landmarks {
        // Candidate centres, best first, each with the point it faces.
        let mut cands: Vec<(Vec2, Vec2)> = Vec::new();
        let around = |pc: Vec2, pr: f32, cands: &mut Vec<(Vec2, Vec2)>, rng: &mut Rng| {
            let a0 = rng.f() * std::f32::consts::TAU;
            for k in 0..24 {
                let a = a0 + k as f32 / 24.0 * std::f32::consts::TAU;
                let d = Vec2::new(a.cos(), a.sin());
                for extra in [1.0f32, 4.0, 8.0] {
                    cands.push((pc + d * (pr + lm.size.max_element() * 0.5 + extra), pc));
                }
            }
        };
        match lm.near {
            Near::Market => {
                if let Some(sq) = plan.plazas.first() {
                    around(sq.c, sq.w, &mut cands, rng);
                    // A market hall or a cross may stand in the square, at
                    // one end of it, facing down its length.
                    for t in [0.55f32, -0.55, 0.0] {
                        cands.push((sq.c + sq.u * sq.r * t, sq.c - sq.u * sq.r * t.signum()));
                    }
                }
            }
            Near::Square => {
                let k = if plan.plazas.len() > 1 { 1 + (square_k - 1) % (plan.plazas.len() - 1) } else { 0 };
                square_k += 1;
                if let Some(sq) = plan.plazas.get(k) {
                    around(sq.c, sq.w, &mut cands, rng);
                }
            }
            Near::High => {
                let mut pts: Vec<(f32, Vec2)> = (0..600)
                    .map(|_| {
                        let a = rng.f() * std::f32::consts::TAU;
                        let d = Vec2::new(a.cos(), a.sin());
                        let p = dist.c + d * dist.ext.reach * rng.f().sqrt();
                        (if dist.sd(p) < -3.0 { ground.height(p) } else { f32::MIN }, p)
                    })
                    .collect();
                pts.sort_by(|a, b| b.0.total_cmp(&a.0));
                cands.extend(pts.into_iter().map(|(_, p)| (p, dist.c)));
            }
            Near::Edge | Near::Water | Near::Anywhere => {
                // On a street first, facing it: those where the street runs
                // near a compass line (a structure turns only by quarter
                // turns), then the rest; any open ground after that.
                if lm.near != Near::Water {
                    let (mut square, mut askew) = (Vec::new(), Vec::new());
                    for w in plan.ways.iter().filter(|w| w.drawn && w.rank != rank::COUNTRY && w.rank != rank::ALLEY && w.rank != rank::QUAY) {
                        let len: f32 = w.pts.windows(2).map(|s| s[0].distance(s[1])).sum();
                        let mut s = 4.0;
                        while s < len - 4.0 {
                            let (p, t) = point_at(&w.pts, s);
                            for side in [-1.0f32, 1.0] {
                                let n = Vec2::new(-t.y, t.x) * side;
                                let turns = super::quarter_turns(-n);
                                let ws = if turns % 2 == 1 { Vec2::new(lm.size.y, lm.size.x) } else { lm.size };
                                let reach = (ws.x * n.x).abs() * 0.5 + (ws.y * n.y).abs() * 0.5;
                                let at = p + n * (w.half + w.walk + 1.2 + reach);
                                let sd = dist.sd(at);
                                if sd > -2.0 || (lm.near == Near::Edge && sd < -0.35 * dist.radius) {
                                    continue;
                                }
                                let mis = (-n).angle_to(super::quarter_axis(turns)).abs();
                                if mis < 20f32.to_radians() {
                                    square.push((at, p));
                                } else {
                                    askew.push((at, p));
                                }
                            }
                            s += 5.0;
                        }
                    }
                    for list in [&mut square, &mut askew] {
                        for k in (1..list.len()).rev() {
                            let j = (rng.f() * (k + 1) as f32) as usize % (k + 1);
                            list.swap(k, j);
                        }
                    }
                    cands.extend(square);
                    cands.extend(askew);
                }
                for _ in 0..600 {
                    let a = rng.f() * std::f32::consts::TAU;
                    let d = Vec2::new(a.cos(), a.sin());
                    let p = dist.c + d * dist.ext.reach * rng.f().sqrt();
                    let sd = dist.sd(p);
                    if sd > -2.0 || (lm.near == Near::Edge && sd < -0.2 * dist.radius) {
                        continue;
                    }
                    if lm.near == Near::Water {
                        // Within 25 m of water.
                        let near_water = (0..8).any(|k| {
                            let b = k as f32 / 8.0 * std::f32::consts::TAU;
                            ground.wet(p + Vec2::new(b.cos(), b.sin()) * (lm.size.max_element() * 0.5 + 12.0))
                        });
                        if !near_water {
                            continue;
                        }
                    }
                    cands.push((p, dist.c));
                }
            }
        }
        let mut done = false;
        for (at, face) in cands {
            // The quarter turn whose front (-Z unturned) points nearest the
            // thing it faces.
            let to = face - at;
            let turns = super::quarter_turns(to);
            let size = if turns % 2 == 1 { Vec2::new(lm.size.y, lm.size.x) } else { lm.size };
            let h = size * 0.5 + Vec2::splat(0.5);
            let quad = [at - h, Vec2::new(at.x + h.x, at.y - h.y), at + h, Vec2::new(at.x - h.x, at.y + h.y)];
            let Some(cells) = quad_cells(r, &quad) else { continue };
            let on_square = matches!(lm.near, Near::Market | Near::Square);
            if cells.is_empty() || cells.iter().any(|&i| r.used[i] != FREE && !(on_square && r.used[i] == OPEN)) || dist.at(at).is_none() {
                continue;
            }
            if super::fall(ground, &quad) > 2.5 {
                continue;
            }
            for &i in &cells {
                r.used[i] = BUILT;
            }
            plan.sites.push(super::Site { id: lm.id, at, turns, size });
            done = true;
            break;
        }
        if !done {
            plan.notes.push(format!("no site for landmark {} ({:?})", lm.id, lm.near));
        }
    }
}

/// Where a structure's door is: the middle of its front face, turned.
fn door_at(at: Vec2, turns: u8, size: Vec2) -> (Vec2, Vec2) {
    let face = super::quarter_axis(turns);
    let reach = (size.x * face.x).abs() * 0.5 + (size.y * face.y).abs() * 0.5;
    (at + face * reach, face)
}

/// The cheapest way on foot from a door out to a street, a square or a
/// path already laid: round the buildings, not up the rim, keeping off
/// walls where it can and sharing the paths there are. Cells from the
/// door out, the last one on the street; None if nothing is in reach.
fn door_path(r: &Raster, dist: &Districts, ground: &dyn Ground, at: Vec2, turns: u8, size: Vec2) -> Option<Vec<usize>> {
    use std::cmp::Reverse;
    const REACH: f32 = 40.0;
    let (door, face) = door_at(at, turns, size);
    let start = r.idx(door + face * 0.9)?;
    let goal = |u: u8| u == STREET || u == PATH || u == OPEN;
    if r.used[start] != FREE && !goal(r.used[start]) {
        return None;
    }
    if goal(r.used[start]) {
        return Some(vec![start]);
    }
    let mut cost: crate::util::FxHashMap<usize, f32> = Default::default();
    let mut from: crate::util::FxHashMap<usize, usize> = Default::default();
    let mut heap = std::collections::BinaryHeap::new();
    cost.insert(start, 0.0);
    heap.push(Reverse((0u32, start)));
    let built = |x: i64, z: i64| x >= 0 && z >= 0 && (x as usize) < r.w && (z as usize) < r.h && r.used[z as usize * r.w + x as usize] == BUILT;
    while let Some(Reverse((cb, i))) = heap.pop() {
        // Costs are positive, so their bits order like the floats.
        let c = f32::from_bits(cb);
        if c > *cost.get(&i).unwrap_or(&f32::MAX) {
            continue;
        }
        if goal(r.used[i]) {
            let mut out = vec![i];
            let mut k = i;
            while let Some(&j) = from.get(&k) {
                out.push(j);
                k = j;
            }
            out.reverse();
            return Some(out);
        }
        let (x, z) = ((i % r.w) as i64, (i / r.w) as i64);
        let hp = ground.height(r.centre(i));
        for (dx, dz) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (xx, zz) = (x + dx, z + dz);
            if xx < 0 || zz < 0 || xx >= r.w as i64 || zz >= r.h as i64 {
                continue;
            }
            let j = zz as usize * r.w + xx as usize;
            let u = r.used[j];
            if u != FREE && !goal(u) {
                continue;
            }
            // A diagonal step may not cut a building's corner.
            if dx != 0 && dz != 0 && (built(xx, z) || built(x, zz)) {
                continue;
            }
            let q = r.centre(j);
            if q.distance(door) > REACH || dist.at(q).is_none() {
                continue;
            }
            let len = if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let climb = (ground.height(q) - hp).abs() / len;
            if climb > 0.8 {
                continue;
            }
            let mut w = match u {
                PATH => 0.35,
                STREET | OPEN => 0.5,
                _ => 1.0,
            };
            // Keep a step off the walls where there is room.
            if u == FREE && (built(xx - 1, zz) || built(xx + 1, zz) || built(xx, zz - 1) || built(xx, zz + 1)) {
                w += 0.8;
            }
            w += climb * 2.0;
            let nc = c + len * w;
            if nc < *cost.get(&j).unwrap_or(&f32::MAX) {
                cost.insert(j, nc);
                from.insert(j, i);
                heap.push(Reverse((nc.to_bits(), j)));
            }
        }
    }
    None
}

/// Lay a footpath along cells from `door_path`: they are kept clear of
/// building, and the line is straightened where nothing is in the way.
fn lay_path(plan: &mut Plan, r: &mut Raster, cells: &[usize]) {
    for &i in cells {
        if r.used[i] == FREE {
            r.used[i] = PATH;
        }
    }
    if cells.len() < 2 {
        return;
    }
    let pts: Vec<Vec2> = cells.iter().map(|&i| r.centre(i)).collect();
    // String-pull: from each kept point, the furthest later point in a
    // clear straight line, a path's half-width from any wall.
    let clear = |a: Vec2, b: Vec2| {
        let n = (a.distance(b) * 3.0).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let p = a.lerp(b, k as f32 / n as f32);
            [Vec2::ZERO, Vec2::new(0.7, 0.0), Vec2::new(-0.7, 0.0), Vec2::new(0.0, 0.7), Vec2::new(0.0, -0.7)]
                .iter()
                .all(|&o| r.idx(p + o).is_some_and(|i| !matches!(r.used[i], BUILT | WET | TAKEN)))
        })
    };
    let mut line = vec![pts[0]];
    let mut k = 0;
    while k < pts.len() - 1 {
        let mut j = pts.len() - 1;
        while j > k + 1 && !clear(pts[k], pts[j]) {
            j -= 1;
        }
        line.push(pts[j]);
        k = j;
    }
    plan.paths.push(line);
}

/// More of the world's own houses on the ground the street plots left:
/// nearest the streets first, each turned to face its street (or the next
/// best way when that won't fit), a gap all round, and a footpath out that
/// the houses after it must leave clear.
fn infill(plan: &mut Plan, r: &mut Raster, dist: &Districts, spec: &Spec, ground: &dyn Ground, rng: &mut Rng) {
    let gap = spec.gaps.max(1.5);
    let mut cands: Vec<(f32, Vec2)> = Vec::new();
    for z in (0..r.h).step_by(2) {
        for x in (0..r.w).step_by(2) {
            let i = z * r.w + x;
            if r.used[i] != FREE || r.owner[i] == u32::MAX {
                continue;
            }
            let p = r.centre(i);
            if dist.at(p).is_none() || dist.sd(p) > -3.0 {
                continue;
            }
            cands.push((r.dist[i] + rng.f() * 3.0, p));
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    let houses = &spec.features.houses;
    let mut used = vec![0u32; houses.len()];
    let mut added = 0;
    for (_, p) in cands {
        if added >= spec.features.infill {
            break;
        }
        let Some(i) = r.idx(p) else { continue };
        if r.used[i] != FREE {
            continue;
        }
        // The nearest point on the cell's own street.
        let way = &plan.ways[(r.owner[i] / 2) as usize];
        let mut foot = (f32::MAX, p);
        for seg in way.pts.windows(2) {
            let (d, q, _) = seg_foot(seg[0], seg[1], p);
            if d < foot.0 {
                foot = (d, q);
            }
        }
        // The least-built kinds first, so the kinds stay mixed.
        let mut kinds: Vec<usize> = (0..houses.len()).collect();
        kinds.sort_by_key(|&k| (used[k], (rng.f() * 1000.0) as u32));
        let want = super::quarter_turns(foot.1 - p);
        'kind: for &k in kinds.iter().take(2) {
            for dt in [0u8, 1, 3, 2] {
                let turns = (want + dt) % 4;
                let sz = houses[k].size;
                let ws = if turns % 2 == 1 { Vec2::new(sz.y, sz.x) } else { sz };
                // The box set back so its front, not its middle, is at the
                // candidate.
                let face = super::quarter_axis(turns);
                let reach = (ws.x * face.x).abs() * 0.5 + (ws.y * face.y).abs() * 0.5;
                let at = p - face * reach;
                let h = ws * 0.5 + Vec2::splat(gap);
                let quad = [at - h, Vec2::new(at.x + h.x, at.y - h.y), at + h, Vec2::new(at.x - h.x, at.y + h.y)];
                // Clear of the edge: room to walk round the back.
                if quad.iter().any(|&q| dist.sd(q) > -2.5) {
                    continue;
                }
                let Some(cells) = quad_cells(r, &quad) else { continue };
                if cells.is_empty() || cells.iter().any(|&c| r.used[c] != FREE) {
                    continue;
                }
                let inner = ws * 0.5 + Vec2::splat(0.3);
                let body = [at - inner, Vec2::new(at.x + inner.x, at.y - inner.y), at + inner, Vec2::new(at.x - inner.x, at.y + inner.y)];
                if super::fall(ground, &body) > 2.7 {
                    continue;
                }
                let Some(own) = quad_cells(r, &body) else { continue };
                for &c in &own {
                    r.used[c] = BUILT;
                }
                match door_path(r, dist, ground, at, turns, ws) {
                    Some(path) if path.len() < 30 => {
                        plan.sites.push(super::Site { id: houses[k].id, at, turns, size: ws });
                        if spec.features.paths {
                            lay_path(plan, r, &path);
                        } else {
                            for &c in &path {
                                if r.used[c] == FREE {
                                    r.used[c] = PATH;
                                }
                            }
                        }
                        used[k] += 1;
                        added += 1;
                        plan.stats.built += 1;
                        break 'kind;
                    }
                    _ => {
                        for &c in &own {
                            r.used[c] = FREE;
                        }
                    }
                }
            }
        }
    }
    if added < spec.features.infill {
        plan.notes_quiet.push(format!("infill: {added} of {} houses found room", spec.features.infill));
    }
}
