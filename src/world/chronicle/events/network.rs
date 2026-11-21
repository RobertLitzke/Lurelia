use crate::util::FxHashMap as HashMap;
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
        // Junctions: how far back each line is trimmed at a shared end.
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
        let mut junctions = 0;
        for (n, (count, half, mat)) in deg.clone() {
            if self.height.is_some() {
                // Passages run into each other at a node; nothing to pave.
                junctions += (count >= 2) as usize;
                continue;
            }
            if count < 2 {
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
            route::junction(cx, at[&n], r, mat);
            junctions += 1;
        }
        let mut total = 0.0;
        let mut spans = 0;
        let mut laid = 0;
        for (k, line) in lines.iter().enumerate() {
            let (Some(line), Some(ki)) = (line, kits[k]) else { continue };
            if line.len() < 2 {
                continue;
            }
            let l = if self.height.is_some() { route::lay_at(cx, line, ki, self.smooth, self.height) } else { route::lay(cx, line, ki, self.smooth) };
            total += l.length;
            spans += l.spans.len();
            laid += 1;
        }
        format!(
            "{laid} links, {total:.0} m, {junctions} junctions{}{}",
            if spans == 0 { String::new() } else { format!(", {spans} spans") },
            if failed == 0 { String::new() } else { format!(", {failed} with no way through") },
        )
    }
}

/// Drop the points at the front of a line that fall inside `r` of the node,
/// leaving the line starting on the junction's rim.
fn trim_front(line: &mut Vec<Vec2>, node: Vec2, r: f32) {
    let keep = line.iter().position(|p| p.distance(node) > r).unwrap_or(line.len().saturating_sub(1));
    if keep > 0 && line.len() - keep >= 2 {
        line.drain(..keep);
    }
}
