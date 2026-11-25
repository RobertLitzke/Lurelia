use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::crossing_points;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::route::{self, cost, ribbon::Ribbons};
use glam::Vec2;
use serde::Deserialize;

/// A set of places and the lines that join them, routed together: each link
/// finds its own way, and every line laid makes its ground dearer, so the
/// next one joins it at a junction rather than running alongside. Where two
/// or more meet, both are trimmed back and the crossing is aproned.
///
/// Links are laid widest first, so the lanes bend to meet the road and not
/// the other way about.
#[derive(Deserialize, Debug)]
pub struct Network {
    /// Named points the links join; a link may also name a chronicle place.
    #[serde(default)]
    nodes: HashMap<String, Pos>,
    links: Vec<Link>,
    #[serde(default)]
    terms: route::Terms,
    #[serde(default = "default_smooth")]
    smooth: f32,
    /// Lay every link level at this height above the sea instead of on the
    /// land: with a `bore` kit that is a cave system, its floor at this
    /// height wherever there is rock, open where a link comes out of a cliff
    /// or into a pit. Underground nothing on the surface matters, so links
    /// are not searched for: each runs from node to node, swinging up to
    /// `wander` m to either side, and the nodes give the network its shape.
    #[serde(default)]
    height: Option<f32>,
    #[serde(default = "default_wander")]
    wander: f32,
}

fn default_wander() -> f32 {
    6.0
}

#[derive(Deserialize, Debug)]
struct Link {
    from: String,
    to: String,
    kit: String,
    #[serde(default)]
    terms: Option<route::Terms>,
}

/// A line from `a` to `b` about a metre between points, swinging to either
/// side by up to `wander` m and back to nothing at both ends.
fn level_line(a: Vec2, b: Vec2, wander: f32, seed: u32) -> Vec<Vec2> {
    let len = a.distance(b).max(1.0);
    let dir = (b - a) / len;
    let side = Vec2::new(-dir.y, dir.x);
    let amp = wander.min(len * 0.22);
    let steps = len.ceil() as usize;
    (0..=steps)
        .map(|s| {
            let t = s as f32 / steps as f32;
            let ends = (t * (1.0 - t) * 4.0).min(1.0);
            let off = crate::world::noise::perlin2(t * len * 0.045, 1.7, seed) * 1.6 + crate::world::noise::perlin2(t * len * 0.13, 5.1, seed) * 0.5;
            a + dir * (len * t) + side * (off * amp * ends)
        })
        .collect()
}

fn default_smooth() -> f32 {
    60.0
}

impl Process for Network {
    fn apply(&self, cx: &mut EventCtx) -> String {
        // Where each end lies: the network's own nodes first, then the
        // chronicle's places.
        let mut at: HashMap<String, Vec2> = HashMap::default();
        for (n, p) in &self.nodes {
            let v = cx.pos(p);
            at.insert(n.clone(), v);
        }
        let mut ends = Vec::new();
        for l in &self.links {
            for n in [&l.from, &l.to] {
                if !at.contains_key(n) {
                    let v = cx.pos(&Pos::Named(n.clone()));
                    at.insert(n.clone(), v);
                }
            }
            ends.push((at[&l.from], at[&l.to]));
        }
        // Resolve the kits, then lay the widest links first.
        let mut order: Vec<usize> = (0..self.links.len()).collect();
        let mut kits = Vec::new();
        for l in &self.links {
            kits.push(route::kit_index(cx, &l.kit));
        }
        let halves: Vec<f32> = kits
            .iter()
            .map(|k| k.map_or(0.0, |k| cx.state.get::<Ribbons>().kits[k].half))
            .collect();
        order.sort_by(|&a, &b| halves[b].partial_cmp(&halves[a]).unwrap_or(std::cmp::Ordering::Equal));

        let mut grid = cost::Grid::new(&cx.f, self.terms.step.unwrap_or(4.0).clamp(1.0, 16.0));
        let mut lines: Vec<Option<Vec<Vec2>>> = vec![None; self.links.len()];
        let mut failed = 0;
        for &k in &order {
            let Some(ki) = kits[k] else {
                failed += 1;
                continue;
            };
            let kit = cx.state.get::<Ribbons>().kits[ki].clone();
            let terms = self.links[k].terms.clone().unwrap_or_else(|| self.terms.clone()).over(&kit);
            let (a, b) = ends[k];
            let found = if self.height.is_some() { Some(level_line(a, b, self.wander, cx.seed.wrapping_add(k as u32))) } else { route::find_line(&grid, a, b, &terms) };
            match found {
                Some(line) => {
                    // The ground it took: the next link would rather join it.
                    grid.take(&line, kit.half + 1.0);
                    lines[k] = Some(line);
                }
                None => {
                    cx.warnings.push(format!("network link {} to {}: no way through", self.links[k].from, self.links[k].to));
                    failed += 1;
                }
            }
        }
        // How many links end at each node, and the widest of them.
        let mut deg: HashMap<String, (usize, f32, u8)> = HashMap::default();
        for (k, l) in self.links.iter().enumerate() {
            if lines[k].is_none() {
                continue;
            }
            let m = kits[k].map_or(0, |ki| cx.state.get::<Ribbons>().kits[ki].mat);
            for n in [&l.from, &l.to] {
                let e = deg.entry(n.clone()).or_insert((0, 0.0, 0));
                e.0 += 1;
                if halves[k] > e.1 {
                    e.1 = halves[k];
                    e.2 = m;
                }
            }
        }
        // A node where just two links of one kit meet is a bend in one road,
        // not a junction: the two are laid as one line, rounded through the
        // node, so the grade runs on unbroken and no apron caps the corner.
        let through = |n: &String| -> bool {
            if self.height.is_some() || deg.get(n).is_none_or(|d| d.0 != 2) {
                return false;
            }
            let ks: Vec<usize> = (0..self.links.len()).filter(|&k| lines[k].is_some() && (&self.links[k].from == n || &self.links[k].to == n)).collect();
            ks.len() == 2 && ks[0] != ks[1] && kits[ks[0]] == kits[ks[1]]
        };
        let bends: std::collections::HashSet<String> = deg.keys().filter(|n| through(n)).cloned().collect();

        // Junctions: how far back each line is trimmed at a shared end.
        let mut junctions = 0;
        let mut aprons: Vec<(Vec2, f32, u8)> = Vec::new();
        for (n, (count, half, mat)) in deg.clone() {
            if self.height.is_some() {
                // Passages run into each other at a node; nothing to pave.
                junctions += (count >= 2) as usize;
                continue;
            }
            if count < 2 || bends.contains(&n) {
                continue;
            }
            let r = (half + 1.5).max(2.5);
            for (k, l) in self.links.iter().enumerate() {
                let Some(line) = lines[k].as_mut() else { continue };
                if l.from == n {
                    trim_front(line, at[&n], r);
                }
                if l.to == n {
                    line.reverse();
                    trim_front(line, at[&n], r);
                    line.reverse();
                }
            }
            aprons.push((at[&n], r, mat));
            junctions += 1;
        }

        // Chains: links joined end to end through the bends.
        let mut used = vec![false; self.links.len()];
        let mut chains: Vec<(Vec<Vec2>, usize)> = Vec::new();
        for k0 in 0..self.links.len() {
            if used[k0] || lines[k0].is_none() || kits[k0].is_none() {
                continue;
            }
            used[k0] = true;
            let mut chain = lines[k0].clone().unwrap();
            let (mut head, mut tail) = (self.links[k0].from.clone(), self.links[k0].to.clone());
            // Grow at the tail, then at the head.
            for front in [false, true] {
                loop {
                    let n = if front { head.clone() } else { tail.clone() };
                    if !bends.contains(&n) {
                        break;
                    }
                    let Some(k) = (0..self.links.len()).find(|&k| !used[k] && lines[k].is_some() && (self.links[k].from == n || self.links[k].to == n)) else { break };
                    used[k] = true;
                    let mut next = lines[k].clone().unwrap();
                    let other = if self.links[k].from == n { self.links[k].to.clone() } else { next.reverse(); self.links[k].from.clone() };
                    // `next` now runs from n to `other`.
                    if front {
                        next.reverse();
                        next.pop();
                        next.extend(chain);
                        chain = next;
                        head = other;
                    } else {
                        chain.pop();
                        chain.extend(next);
                        tail = other;
                    }
                    chain = round_bend(&chain, at[&n], 14.0);
                }
            }
            chains.push((chain, kits[k0].unwrap()));
        }

        let mut total = 0.0;
        let mut spans = 0;
        let mut laid = 0;
        for (line, ki) in &chains {
            if line.len() < 2 {
                continue;
            }
            let l = if self.height.is_some() { route::lay_at(cx, line, *ki, self.smooth, self.height) } else { route::lay(cx, line, *ki, self.smooth) };
            total += l.length;
            spans += l.spans.len();
            laid += 1;
        }
        // The junctions after the roads, each tilted to meet the graded ends
        // running into it, so no road steps up or down onto its apron.
        for &(p, r, mat) in &aprons {
            let ends: Vec<Vec2> = chains
                .iter()
                .flat_map(|(l, _)| [l[0], l[l.len() - 1]])
                .filter(|e| e.distance(p) < r + 3.0)
                .collect();
            route::junction_fit(cx, p, r, mat, &ends);
        }
        let lines: Vec<Option<Vec<Vec2>>> = chains.iter().map(|(l, _)| Some(l.clone())).collect();
        let halves: Vec<f32> = chains.iter().map(|(_, ki)| cx.state.get::<Ribbons>().kits[*ki].half).collect();
        let kits: Vec<Option<usize>> = chains.iter().map(|(_, ki)| Some(*ki)).collect();
        // Two links that cross away from any node: without a junction the
        // one laid second cuts its grade through the first, leaving a step
        // across it. The crossing is aproned at the later grade, and the
        // apron's feather blends the earlier one into it. Underground,
        // passages simply open into each other.
        let mut crossings: Vec<Vec2> = Vec::new();
        if self.height.is_none() {
            let nodes: Vec<(Vec2, f32)> = deg.iter().map(|(n, &(_, half, _))| (at[n], (half + 1.5).max(2.5) + 3.0)).collect();
            let mut found: Vec<(Vec2, f32, u8)> = Vec::new();
            for i in 0..lines.len() {
                for j in i + 1..lines.len() {
                    let (Some(a), Some(b)) = (&lines[i], &lines[j]) else { continue };
                    let half = halves[i].max(halves[j]);
                    let mat = if halves[i] >= halves[j] { kits[i] } else { kits[j] }.map_or(0, |ki| cx.state.get::<Ribbons>().kits[ki].mat);
                    for p in crossing_points(a, b) {
                        if nodes.iter().any(|&(n, r)| n.distance(p) < r) || found.iter().any(|&(q, _, _)| q.distance(p) < 4.0) {
                            continue;
                        }
                        found.push((p, (half + 1.5).max(2.5), mat));
                    }
                }
            }
            for (p, r, mat) in found {
                route::junction(cx, p, r, mat);
                crossings.push(p);
            }
        }
        format!(
            "{laid} links, {total:.0} m, {junctions} junctions{}{}{}",
            if crossings.is_empty() {
                String::new()
            } else {
                format!(", {} crossings at {}", crossings.len(), crossings.iter().map(|p| format!("{:.0},{:.0}", p.x, p.y)).collect::<Vec<_>>().join("; "))
            },
            if spans == 0 { String::new() } else { format!(", {spans} spans") },
            if failed == 0 { String::new() } else { format!(", {failed} with no way through") },
        )
    }
}

/// The corner of a line at `node` rounded: the points within `reach` m of it
/// (or less, on a short leg) give way to a curve from where the line comes
/// in to where it goes out, through the middle of the corner.
fn round_bend(line: &[Vec2], node: Vec2, reach: f32) -> Vec<Vec2> {
    let Some(k) = (0..line.len()).min_by(|&a, &b| line[a].distance(node).total_cmp(&line[b].distance(node))) else {
        return line.to_vec();
    };
    if k == 0 || k + 1 >= line.len() {
        return line.to_vec();
    }
    // How far along each leg to start the curve: at most half the shorter leg.
    let leg = |range: &mut dyn Iterator<Item = usize>| {
        let mut d = 0.0;
        let mut last = k;
        let mut out = k;
        for j in range {
            d += line[j].distance(line[last]);
            last = j;
            out = j;
            if d >= reach {
                break;
            }
        }
        (out, d)
    };
    let (i0, d0) = leg(&mut (0..k).rev());
    let (i1, d1) = leg(&mut (k + 1..line.len()));
    let d = d0.min(d1);
    if d < 2.0 {
        return line.to_vec();
    }
    let (p0, p1, c) = (line[i0], line[i1], line[k]);
    let n = ((d0 + d1) / 1.0).ceil().max(2.0) as usize;
    let mut out: Vec<Vec2> = line[..i0].to_vec();
    for j in 0..=n {
        let t = j as f32 / n as f32;
        let u = 1.0 - t;
        out.push(p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t));
    }
    out.extend_from_slice(&line[i1 + 1..]);
    out
}

/// Drop the points at the front of a line that fall inside `r` of the node,
/// leaving the line starting on the junction's rim.
fn trim_front(line: &mut Vec<Vec2>, node: Vec2, r: f32) {
    let keep = line.iter().position(|p| p.distance(node) > r).unwrap_or(line.len().saturating_sub(1));
    if keep > 0 && line.len() - keep >= 2 {
        line.drain(..keep);
    }
}
