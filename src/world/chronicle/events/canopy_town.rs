use crate::world::chronicle::geom::segments_cross;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Growth, Poi, Pos};
use crate::world::gen::Tree;
use crate::world::noise::{hash32, perlin2, unit};
use crate::world::route::{self, ribbon::Ribbons};
use crate::world::stamp::trunk_reach;
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::f32::consts::TAU;

/// A town in the trees: giant host trees grown where the chronicle puts them,
/// a ring platform round each trunk, walkways strung between the platforms
/// and stairs wound up a few trunks from the ground.
///
/// The hosts are grown here, at chronicle time, from the seed flora will
/// grow them from (`Growth::seed`), so the platforms know each trunk's
/// girth at their height and the walkways know where every other trunk and
/// limb is. The walkways are picked the way a network is laid out: every
/// pair of platforms close enough, level enough and with a clear line
/// between them is a candidate, the shortest set that joins them all is
/// built first (a spanning tree), then a few loops where the way round is
/// much longer than the way across. Two walkways may cross in plan only
/// when one passes well over the other.
#[derive(Deserialize, Debug)]
pub struct CanopyTown {
    at: Pos,
    radius: f32,
    /// Plants to grow as hosts, one picked at random for each.
    hosts: Vec<String>,
    /// How many hosts to look for.
    count: usize,
    /// Least distance between two host trunks, metres.
    #[serde(default = "d_spacing")]
    spacing: f32,
    /// Platform heights over the ground at the trunk, metres: a smooth field
    /// across the town picks within this.
    #[serde(default = "d_levels")]
    levels: [f32; 2],
    /// Width of a ring platform's deck, metres.
    #[serde(default = "d_deck")]
    deck: f32,
    /// Kits: the ring's deck, its outer rail (broken where a walkway or a
    /// stair joins), the walkways and the stairs.
    ring: String,
    rail: String,
    walk: String,
    stair: String,
    /// How many trunks get a stair from the ground.
    #[serde(default = "d_stairs")]
    stairs: usize,
    /// The longest walkway, rim to rim, metres.
    #[serde(default = "d_link")]
    max_link: f32,
    /// The steepest walkway, rise over run.
    #[serde(default = "d_grade")]
    max_grade: f32,
    /// Loops past the spanning tree, as a share of the platforms.
    #[serde(default = "d_loops")]
    loops: f32,
    /// How steeply a stair climbs, rise over run along its arc.
    #[serde(default = "d_stair_grade")]
    stair_grade: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn d_spacing() -> f32 {
    20.0
}
fn d_levels() -> [f32; 2] {
    [9.0, 20.0]
}
fn d_deck() -> f32 {
    2.5
}
fn d_stairs() -> usize {
    3
}
fn d_link() -> f32 {
    34.0
}
fn d_grade() -> f32 {
    0.22
}
fn d_loops() -> f32 {
    0.25
}
fn d_stair_grade() -> f32 {
    0.42
}

/// Room left between a trunk and the inner edge of anything built round it.
const TRUNK_GAP: f32 = 0.35;
/// How far a walkway keeps from a trunk it passes, past its own edge.
const PASS_TRUNK: f32 = 1.2;
/// How far one walkway passes over another where they cross in plan.
const PASS_OVER: f32 = 4.5;
/// Headroom a walkway needs past a limb.
const HEADROOM: f32 = 2.4;
/// The narrowest angle between two things joining one platform, radians.
const MIN_APART: f32 = 0.6;

struct Host {
    c: Vec2,
    ground: f32,
    tree: Tree,
}

struct Platform {
    host: usize,
    /// Deck surface, metres above sea level as the world counts it.
    y: f32,
    /// Radius of the deck's centre line.
    r: f32,
    /// Outer edge of the deck.
    outer: f32,
}

/// A walkway that could be built between two platforms.
#[derive(Clone, Copy)]
struct Edge {
    a: usize,
    b: usize,
    /// Its ends, on each deck's outer edge.
    pa: Vec2,
    pb: Vec2,
    cost: f32,
}

impl Edge {
    fn len(&self) -> f32 {
        self.pa.distance(self.pb)
    }
}

impl Process for CanopyTown {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let centre = cx.pos(&self.at);
        let seed = cx.seed;
        let kits = [&self.ring, &self.rail, &self.walk, &self.stair].map(|k| route::kit_index(cx, k));
        let [Some(k_ring), Some(k_rail), Some(k_walk), Some(k_stair)] = kits else {
            return "skipped: a kit is missing".into();
        };
        let walk_half = cx.state.get::<Ribbons>().kits[k_walk].half;
        let stair_half = cx.state.get::<Ribbons>().kits[k_stair].half;
        let plants: Vec<usize> = self.hosts.iter().filter_map(|n| cx.lib.plant(n)).collect();
        if plants.is_empty() {
            return "skipped: no host plant found".into();
        }

        // ---- hosts ---------------------------------------------------------
        let mut rng = seed ^ 0x7472_6565;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            unit(rng)
        };
        let mut hosts: Vec<Host> = Vec::new();
        let sea = cx.f.sea;
        for _ in 0..self.count * 60 {
            if hosts.len() >= self.count {
                break;
            }
            let a = rnd() * TAU;
            let d = self.radius * rnd().sqrt();
            let p = centre + Vec2::new(a.cos(), a.sin()) * d;
            if p.x < 8.0 || p.y < 8.0 || p.x > cx.f.size - 8.0 || p.y > cx.f.size - 8.0 {
                continue;
            }
            let i = cx.f.idx(p.x, p.y);
            let ground = cx.f.height(p.x, p.y);
            if cx.f.water[i] != 0 || cx.f.occupied[i] || ground < sea + 1.0 || cx.f.slope[i] > 0.5 {
                continue;
            }
            if hosts.iter().any(|h| h.c.distance(p) < self.spacing) {
                continue;
            }
            let pi = plants[(rnd() * plants.len() as f32) as usize % plants.len()];
            let tseed = hash32(seed ^ 0x686f_7374 ^ (hosts.len() as u32).wrapping_mul(0x9e37_79b9));
            let base = Vec3::new(p.x, ground, p.y);
            // Exactly as flora's `place_growths` will grow it.
            let tree = crate::world::flora::grow(cx.lib, &cx.lib.plants[pi], base - Vec3::Y * 0.1, tseed, Default::default());
            cx.state.get::<crate::world::chronicle::events::tower::Growths>().0.push(Growth {
                plant: cx.lib.plants[pi].name.clone(),
                at: base,
                out: Vec2::X,
                seed: Some(tseed),
            });
            // Keep the forest's own trees off the foot of it.
            let foot = trunk_reach(&tree, 0.0, 0.0) + 2.5;
            crate::world::chronicle::geom::each_near(&mut cx.f, p, foot, |f, i, _, _| f.occupied[i] = true);
            hosts.push(Host { c: p, ground, tree });
        }
        if hosts.len() < 2 {
            return format!("skipped: only {} host found", hosts.len());
        }

        // ---- platforms -----------------------------------------------------
        // One per host, at a height from a smooth field so neighbours sit
        // near each other's level, and always under the crown.
        let deck_half = self.deck * 0.5;
        let plats: Vec<Platform> = hosts
            .iter()
            .enumerate()
            .map(|(k, h)| {
                let n = (perlin2(h.c.x * 0.011, h.c.y * 0.011, seed ^ 0x6c76) * 0.5 + 0.5).clamp(0.0, 1.0);
                let want = self.levels[0] + (self.levels[1] - self.levels[0]) * n;
                let top = (h.tree.canopy_lo - 4.0).max(self.levels[0]);
                let up = want.min(top);
                let inner = trunk_reach(&h.tree, up, 0.0) + TRUNK_GAP;
                let r = inner + deck_half;
                Platform { host: k, y: h.ground + up, r, outer: r + deck_half }
            })
            .collect();

        // ---- candidate walkways ---------------------------------------------
        let mut cands: Vec<Edge> = Vec::new();
        for a in 0..plats.len() {
            for b in a + 1..plats.len() {
                let (pa, pb) = (&plats[a], &plats[b]);
                let (ca, cb) = (hosts[pa.host].c, hosts[pb.host].c);
                let dir = (cb - ca).normalize_or_zero();
                // Each end sits a little inside its deck's outer edge.
                let ea = ca + dir * (pa.outer - 0.3);
                let eb = cb - dir * (pb.outer - 0.3);
                let run = ea.distance(eb);
                if run < 2.0 || run > self.max_link {
                    continue;
                }
                let grade = (pa.y - pb.y).abs() / run;
                if grade > self.max_grade {
                    continue;
                }
                if !self.clear_line(&hosts, &plats, a, b, ea, eb, walk_half) {
                    continue;
                }
                cands.push(Edge { a, b, pa: ea, pb: eb, cost: run * (1.0 + 2.0 * grade) });
            }
        }
        cands.sort_by(|x, y| x.cost.partial_cmp(&y.cost).unwrap_or(std::cmp::Ordering::Equal));

        // ---- the spanning tree, then the loops -------------------------------
        let mut parent: Vec<usize> = (0..plats.len()).collect();
        fn root(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        let mut built: Vec<Edge> = Vec::new();
        let mut rejected_cross = 0;
        for e in &cands {
            let (ra, rb) = (root(&mut parent, e.a), root(&mut parent, e.b));
            if ra == rb {
                continue;
            }
            if !self.fits(e, &built, &plats, &hosts) {
                rejected_cross += 1;
                continue;
            }
            parent[ra] = rb;
            built.push(*e);
        }
        let tree_links = built.len();
        let budget = (self.loops * plats.len() as f32).round() as usize;
        let mut loops = 0;
        for e in &cands {
            if loops >= budget {
                break;
            }
            if built.iter().any(|b| (b.a == e.a && b.b == e.b) || (b.a == e.b && b.b == e.a)) {
                continue;
            }
            // Only where the way round is much longer than the way across.
            let round = path_len(&built, plats.len(), e.a, e.b);
            if round < e.len() * 2.5 + 20.0 {
                continue;
            }
            if !self.fits(e, &built, &plats, &hosts) {
                continue;
            }
            built.push(*e);
            loops += 1;
        }
        let mut groups: Vec<usize> = (0..plats.len()).map(|k| root(&mut parent, k)).collect();
        groups.sort_unstable();
        groups.dedup();
        if groups.len() > 1 {
            cx.warnings.push(format!("canopy_town: {} platforms in {} separate groups", plats.len(), groups.len()));
        }

        // ---- stairs ---------------------------------------------------------
        // Spread over the town: the lowest platform first, then each time the
        // one furthest from every stair so far.
        let mut stair_at: Vec<usize> = Vec::new();
        if self.stairs > 0 {
            let low = (0..plats.len()).min_by(|&x, &y| (plats[x].y - hosts[plats[x].host].ground).partial_cmp(&(plats[y].y - hosts[plats[y].host].ground)).unwrap()).unwrap();
            stair_at.push(low);
            while stair_at.len() < self.stairs.min(plats.len()) {
                let far = (0..plats.len())
                    .filter(|k| !stair_at.contains(k))
                    .max_by(|&x, &y| {
                        let d = |k: usize| stair_at.iter().map(|&s| hosts[plats[s].host].c.distance(hosts[plats[k].host].c)).fold(f32::MAX, f32::min);
                        d(x).partial_cmp(&d(y)).unwrap()
                    })
                    .unwrap();
                stair_at.push(far);
            }
        }

        // ---- lay it all -----------------------------------------------------
        // Where each platform's rail is broken: the bearing of every walkway
        // end and stair head on it, and how wide a gap each needs.
        let mut gaps: Vec<Vec<(f32, f32)>> = vec![Vec::new(); plats.len()];
        let mut total = 0.0;
        for e in &built {
            for (k, p) in [(e.a, e.pa), (e.b, e.pb)] {
                let c = hosts[plats[k].host].c;
                let ang = (p.y - c.y).atan2(p.x - c.x);
                gaps[k].push((ang, (walk_half + 0.35) / plats[k].outer));
            }
            let n = (e.len().ceil() as usize).max(2);
            let line: Vec<Vec2> = (0..=n).map(|s| e.pa.lerp(e.pb, s as f32 / n as f32)).collect();
            let (ya, yb) = (plats[e.a].y, plats[e.b].y);
            let hs: Vec<f32> = (0..=n).map(|s| ya + (yb - ya) * s as f32 / n as f32).collect();
            total += route::lay_heights(cx, &line, k_walk, hs).length;
        }
        let mut stair_len = 0.0;
        for &k in &stair_at {
            let (line, hs, head) = self.stair_line(&hosts[plats[k].host], &plats[k], stair_half, &gaps[k]);
            if line.len() < 2 {
                continue;
            }
            gaps[k].push((head, (stair_half + 0.35) / plats[k].outer));
            stair_len += route::lay_heights(cx, &line, k_stair, hs).length;
        }
        if std::env::var_os("VG_CANOPY_DEBUG").is_some() {
            for (k, p) in plats.iter().enumerate() {
                let h = &hosts[p.host];
                eprintln!("platform {k}: at {:.0},{:.0} ground {:.1} deck {:.1} (+{:.1}) r {:.1} crown from +{:.1} stair {}", h.c.x, h.c.y, h.ground, p.y, p.y - h.ground, p.r, h.tree.canopy_lo, stair_at.contains(&k));
            }
            for e in &built {
                eprintln!("walk {}-{}: {:.0} m, {:.1} -> {:.1}", e.a, e.b, e.len(), plats[e.a].y, plats[e.b].y);
            }
        }
        for (k, p) in plats.iter().enumerate() {
            let c = hosts[p.host].c;
            let ring = circle(c, p.r, 0.0, TAU);
            let n = ring.len();
            route::lay_heights(cx, &ring, k_ring, vec![p.y; n]);
            for (a0, a1) in rail_arcs(&gaps[k]) {
                let arc = circle(c, p.r, a0, a1);
                let n = arc.len();
                route::lay_heights(cx, &arc, k_rail, vec![p.y; n]);
            }
        }
        if let Some(name) = &self.poi {
            cx.pois.push(Poi {
                name: name.clone(),
                pos: centre,
                cause: cx.cause,
            });
        }
        format!(
            "{} hosts, {} walkways ({} in the tree, {loops} loops), {total:.0} m of walkway, {} stairs ({stair_len:.0} m){}{}",
            hosts.len(),
            built.len(),
            tree_links,
            stair_at.len(),
            if groups.len() > 1 { format!(", {} separate groups", groups.len()) } else { String::new() },
            if rejected_cross > 0 { format!(", {rejected_cross} links refused for crossing or crowding") } else { String::new() },
        )
    }
}

impl CanopyTown {
    /// Is the straight walkway from `ea` to `eb` (platforms `a` to `b`) clear
    /// of every other trunk, every other platform near its height and every
    /// limb it would have to pass under?
    #[allow(clippy::too_many_arguments)]
    fn clear_line(&self, hosts: &[Host], plats: &[Platform], a: usize, b: usize, ea: Vec2, eb: Vec2, half: f32) -> bool {
        let (ya, yb) = (plats[a].y, plats[b].y);
        let at = |t: f32| ya + (yb - ya) * t;
        for (k, h) in hosts.iter().enumerate() {
            if k == plats[a].host || k == plats[b].host {
                // A walkway leaving its own trunk still passes under that
                // tree's limbs.
                if !limbs_clear(&h.tree, ea, eb, ya, yb, half) {
                    return false;
                }
                continue;
            }
            let (d, t) = seg_point(ea, eb, h.c);
            let y = at(t);
            if d < trunk_reach(&h.tree, y - h.ground, 0.0) + half + PASS_TRUNK {
                return false;
            }
            if !limbs_clear(&h.tree, ea, eb, ya, yb, half) {
                return false;
            }
        }
        for (k, p) in plats.iter().enumerate() {
            if k == a || k == b {
                continue;
            }
            let (d, t) = seg_point(ea, eb, hosts[p.host].c);
            if (at(t) - p.y).abs() < PASS_OVER && d < p.outer + half + 0.6 {
                return false;
            }
        }
        true
    }

    /// Can walkway `e` go in beside those already built: no crossing in plan
    /// unless one passes well over the other, and no two things joining a
    /// platform at nearly the same bearing?
    fn fits(&self, e: &Edge, built: &[Edge], plats: &[Platform], hosts: &[Host]) -> bool {
        let height = |x: &Edge, p: Vec2| {
            let t = x.pa.distance(p) / x.len().max(1e-3);
            plats[x.a].y + (plats[x.b].y - plats[x.a].y) * t
        };
        for b in built {
            let shared = [b.a, b.b].contains(&e.a) || [b.a, b.b].contains(&e.b);
            if !shared {
                if let Some(p) = segments_cross(e.pa, e.pb, b.pa, b.pb) {
                    if (height(e, p) - height(b, p)).abs() < PASS_OVER {
                        return false;
                    }
                }
            }
            for k in [e.a, e.b] {
                if b.a != k && b.b != k {
                    continue;
                }
                let c = hosts[plats[k].host].c;
                let mine = if e.a == k { e.pa } else { e.pb };
                let theirs = if b.a == k { b.pa } else { b.pb };
                if angle_between(mine - c, theirs - c) < MIN_APART {
                    return false;
                }
            }
        }
        true
    }

    /// A stair wound up the trunk from the ground to platform `p`: a helix
    /// just outside the deck's outer edge (so it never runs under the deck),
    /// climbing anticlockwise, and a short flat landing straight in onto the
    /// deck at the top. Its last turn passes close under the deck's edge, so
    /// it arrives at the far end of a stretch of rail long enough for that
    /// turn to have no walkway over it. Returns the line, its heights and
    /// the bearing it arrives at.
    fn stair_line(&self, h: &Host, p: &Platform, half: f32, gaps: &[(f32, f32)]) -> (Vec<Vec2>, Vec<f32>, f32) {
        let around = p.outer + 0.3 + half;
        // How far back round from the head the stair is still too close
        // under the deck for headroom.
        let close = (HEADROOM + 0.6) / (around * self.stair_grade);
        let arcs = rail_arcs(gaps);
        let head = arcs
            .iter()
            .max_by(|x, y| (x.1 - x.0).partial_cmp(&(y.1 - y.0)).unwrap())
            .map(|&(a0, a1)| (a1 - (half + 0.4) / around).max(a0 + close.min((a1 - a0) * 0.5)))
            .unwrap_or(0.0);
        let rise = p.y - h.ground;
        let radius = |up: f32| (trunk_reach(&h.tree, up.max(0.0), 0.0) + TRUNK_GAP + half).max(around);
        let dir = Vec2::new(head.cos(), head.sin());
        // The landing, from the deck's edge out to the helix, flat.
        let mut pts = vec![h.c + dir * (p.outer - 0.3), h.c + dir * around];
        let mut hs = vec![p.y, p.y];
        // Then the helix, walked down from the top.
        let (mut ang, mut up) = (head, rise);
        let step = 0.7;
        loop {
            ang -= step / radius(up);
            up -= step * self.stair_grade;
            if up <= 0.0 {
                break;
            }
            pts.push(h.c + Vec2::new(ang.cos(), ang.sin()) * radius(up));
            hs.push(h.ground + up);
        }
        pts.push(h.c + Vec2::new(ang.cos(), ang.sin()) * radius(0.0));
        hs.push(h.ground);
        pts.reverse();
        hs.reverse();
        (pts, hs, head.rem_euclid(TAU))
    }
}

/// Distance from `p` to the segment and how far along it the nearest point is.
fn seg_point(a: Vec2, b: Vec2, p: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (p.distance(a + ab * t), t)
}

/// Does a walkway from `ea` (height `ya`) to `eb` keep its headroom clear of
/// the tree's limbs? Sampled every metre: a limb is a segment with a radius.
fn limbs_clear(t: &Tree, ea: Vec2, eb: Vec2, ya: f32, yb: f32, half: f32) -> bool {
    if t.branches.is_empty() {
        return true;
    }
    let n = (ea.distance(eb).ceil() as usize).max(1);
    for s in 0..=n {
        let k = s as f32 / n as f32;
        let q = ea.lerp(eb, k);
        let mid = Vec3::new(q.x, ya + (yb - ya) * k + HEADROOM * 0.5, q.y);
        for &(a, b, r) in &t.branches {
            if r < 0.12 {
                continue;
            }
            let ab = b - a;
            let u = ((mid - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            if (a + ab * u).distance(mid) < r + half.max(HEADROOM * 0.5) + 0.3 {
                return false;
            }
        }
    }
    true
}

/// The smaller angle between two directions, radians.
fn angle_between(a: Vec2, b: Vec2) -> f32 {
    let d = a.normalize_or_zero().dot(b.normalize_or_zero()).clamp(-1.0, 1.0);
    d.acos()
}

/// The length of the shortest way from `a` to `b` over the built walkways,
/// or f32::MAX when there is none.
fn path_len(built: &[Edge], n: usize, a: usize, b: usize) -> f32 {
    let mut best = vec![f32::MAX; n];
    let mut done = vec![false; n];
    best[a] = 0.0;
    loop {
        let Some(k) = (0..n).filter(|&k| !done[k] && best[k] < f32::MAX).min_by(|&x, &y| best[x].partial_cmp(&best[y]).unwrap()) else {
            return f32::MAX;
        };
        if k == b {
            return best[k];
        }
        done[k] = true;
        for e in built {
            let o = if e.a == k {
                e.b
            } else if e.b == k {
                e.a
            } else {
                continue;
            };
            let c = best[k] + e.len();
            if c < best[o] {
                best[o] = c;
            }
        }
    }
}

/// Points about 0.8 m apart round a circle from bearing `a0` to `a1`
/// (radians, anticlockwise from +x), both ends included.
fn circle(c: Vec2, r: f32, a0: f32, a1: f32) -> Vec<Vec2> {
    let n = (((a1 - a0) * r / 0.8).ceil() as usize).max(2);
    (0..=n).map(|s| a0 + (a1 - a0) * s as f32 / n as f32).map(|a| c + Vec2::new(a.cos(), a.sin()) * r).collect()
}

/// The stretches of a ring's rail left between its gaps, as bearing ranges
/// (`a0` < `a1`, possibly past TAU). Each gap is a bearing and a half-width.
fn rail_arcs(gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if gaps.is_empty() {
        return vec![(0.0, TAU)];
    }
    let mut g: Vec<(f32, f32)> = gaps.iter().map(|&(a, w)| (a.rem_euclid(TAU), w)).collect();
    g.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut out = Vec::new();
    for k in 0..g.len() {
        let (a, wa) = g[k];
        let (b, wb) = if k + 1 < g.len() { g[k + 1] } else { (g[0].0 + TAU, g[0].1) };
        let (s, e) = (a + wa, b - wb);
        if e - s > 0.05 {
            out.push((s, e));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_arcs_leave_the_gaps_open() {
        let arcs = rail_arcs(&[(0.0, 0.2), (3.0, 0.1)]);
        assert_eq!(arcs.len(), 2);
        assert!((arcs[0].0 - 0.2).abs() < 1e-5 && (arcs[0].1 - 2.9).abs() < 1e-5);
        assert!((arcs[1].0 - 3.1).abs() < 1e-5 && (arcs[1].1 - (TAU - 0.2)).abs() < 1e-5);
        assert_eq!(rail_arcs(&[]), vec![(0.0, TAU)]);
    }

    #[test]
    fn path_len_goes_round() {
        let e = |a, b, x: f32| Edge {
            a,
            b,
            pa: Vec2::ZERO,
            pb: Vec2::new(x, 0.0),
            cost: x,
        };
        let built = [e(0, 1, 10.0), e(1, 2, 5.0)];
        assert_eq!(path_len(&built, 3, 0, 2), 15.0);
        assert_eq!(path_len(&built, 4, 0, 3), f32::MAX);
    }
}
