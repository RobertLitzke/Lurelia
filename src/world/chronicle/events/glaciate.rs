use super::cirque::bowl;
use super::drainage::Source;
use super::trough::{carve, Wall};
use crate::world::chronicle::hydro::{self, eased_f, Grid, Inflow};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::noise::{hash32, smoothstep, unit};
use glam::Vec2;
use serde::Deserialize;

/// An ice age over the land as it stands. Ice gathers the way water does, so
/// this routes the land's drainage and puts a glacier down every valley that
/// collects enough of it; each one grinds a U-shaped trough whose width and
/// depth go by how much ice came down it. The trunk glacier, fed by
/// everything, cuts far deeper than the ones that join it, which is what
/// leaves their valleys hanging over its walls, and where its floor ends up
/// under the sea there is a fjord. None of the lines are drawn: steer it by
/// shaping the land first (`terrain`, `drainage`) and by `inflow`, the ice
/// arriving from beyond the map.
///
/// `width`, `depth` and `side` are each [least, most]: what the smallest
/// glacier and the trunk get. The walls and their ledges are a `trough`'s.
///
/// It names what it made, for later events to build on (see `Pos`):
/// `<name>_head` (the shore at the head of the fjord, facing up the valley),
/// `<name>_mouth`, `<name>_sill`, `<name>_1` to `<name>_9` (down the flooded
/// stretch from head to mouth) and `<name>_valley` (the dry valley floor
/// above the head). Each of `valleys` picks the tributary that runs nearest
/// its `near` and names `<valley>_head` (its cirque), `<valley>_1` to `_3`
/// (down its floor), `<valley>_above` (just short of the lip), `<valley>_lip`
/// and `<valley>_out` (out over the drop).
#[derive(Deserialize, Debug)]
pub struct Glaciate {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default)]
    inflow: Vec<Source>,
    /// The least catchment that holds a glacier, square metres.
    #[serde(default = "default_min_area")]
    min_area: f32,
    width: [f32; 2],
    depth: [f32; 2],
    side: [f32; 2],
    /// Absolute height the walls climb toward (m).
    rim: f32,
    #[serde(default = "default_shape")]
    shape: f32,
    #[serde(default)]
    ledge: f32,
    #[serde(default)]
    floor_mat: Option<String>,
    /// The deepest any floor goes, absolute (m).
    #[serde(default = "default_floor_min")]
    floor_min: f32,
    /// The moraine across the trunk's mouth.
    #[serde(default)]
    sill: Option<Moraine>,
    /// Valley heads above this (absolute, m) get a cirque. Unset: none but
    /// the named ones.
    #[serde(default)]
    cirque_above: Option<f32>,
    #[serde(default)]
    valleys: Vec<Valley>,
    #[serde(default)]
    poi: Option<String>,
    #[serde(default = "default_step")]
    step: usize,
}

#[derive(Deserialize, Debug)]
struct Moraine {
    /// Absolute height of its crest (m).
    top: f32,
    #[serde(default = "default_sill_len")]
    length: f32,
    /// How far up the fjord from the open sea it lies (m).
    #[serde(default = "default_inset")]
    inset: f32,
}

#[derive(Deserialize, Debug)]
struct Valley {
    name: String,
    near: Pos,
    #[serde(default)]
    poi: Option<String>,
    #[serde(default)]
    cirque: Option<Bowl>,
}

#[derive(Deserialize, Debug)]
struct Bowl {
    radius: f32,
    #[serde(default)]
    ice: Option<String>,
    #[serde(default = "default_ice_m")]
    ice_m: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn default_name() -> String {
    "fjord".into()
}
fn default_min_area() -> f32 {
    40_000.0
}
fn default_shape() -> f32 {
    2.0
}
fn default_floor_min() -> f32 {
    1.0
}
fn default_sill_len() -> f32 {
    45.0
}
fn default_inset() -> f32 {
    70.0
}
fn default_ice_m() -> f32 {
    6.0
}
fn default_step() -> usize {
    2
}

fn lerp(r: [f32; 2], t: f32) -> f32 {
    r[0] + (r[1] - r[0]) * t
}

fn bearing_of(v: Vec2) -> f32 {
    crate::util::bearing(v).to_degrees().rem_euclid(360.0)
}

/// The point `metres` back up a line from index `k`.
fn back(pts: &[Vec2], k: usize, metres: f32) -> usize {
    let mut left = metres;
    let mut i = k;
    while i > 0 && left > 0.0 {
        left -= pts[i].distance(pts[i - 1]);
        i -= 1;
    }
    i
}

impl Process for Glaciate {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let inflow: Vec<Inflow> = self.inflow.iter().map(|s| Inflow { at: cx.pos(&s.at), area: s.area }).collect();
        let floor_mat = self.floor_mat.as_ref().map(|m| cx.mat(m));
        let hints: Vec<Vec2> = self.valleys.iter().map(|v| cx.pos(&v.near)).collect();
        let ices: Vec<Option<u8>> = self.valleys.iter().map(|v| v.cirque.as_ref().and_then(|c| c.ice.as_ref()).map(|m| cx.mat(m))).collect();
        let (seed, cause) = (cx.seed, cx.cause);
        let sea = cx.f.sea;
        let g = Grid::from(&cx.f, self.step);
        let flow = hydro::route(&g, sea, false, seed, 0.4, &inflow, &[]);
        let mut rs = hydro::reaches(&g, &flow, self.min_area);
        if rs.is_empty() {
            cx.warnings.push("glaciate: no valley collects enough ice (lower min_area, or add an inflow)".into());
            return "skipped".into();
        }
        // A tributary ends on the stream it joins, and that last point
        // carries the bigger stream's ice: give it the tributary's own.
        let most = rs.iter().map(|r| *r.area.last().unwrap()).fold(0.0, f32::max);
        for r in rs.iter_mut() {
            let n = r.area.len();
            if r.area[n - 1] < most && n > 1 {
                r.area[n - 1] = r.area[n - 2];
            }
        }
        // Smallest first, so the trunk is cut last, across the mouths of
        // the rest.
        rs.sort_by(|a, b| a.area.last().unwrap().partial_cmp(b.area.last().unwrap()).unwrap());
        let trunk = rs.len() - 1;
        let amax = rs[trunk].area.iter().cloned().fold(0.0, f32::max).max(1.0);
        // Which reach each named valley is.
        let mut named: Vec<Option<usize>> = vec![None; rs.len()];
        for (v, hint) in hints.iter().enumerate() {
            let mut best = (f32::MAX, None);
            for (k, r) in rs.iter().enumerate().take(trunk) {
                if named[k].is_some() {
                    continue;
                }
                let d = r.pts.iter().map(|p| p.distance(*hint)).fold(f32::MAX, f32::min);
                if d < best.0 {
                    best = (d, Some(k));
                }
            }
            match best.1 {
                Some(k) => named[k] = Some(v),
                None => cx.warnings.push(format!("glaciate: no tributary left for valley {}", self.valleys[v].name)),
            }
        }
        let half = (12.0 / g.cell()) as usize;
        let mut cut_total = 0;
        let mut deepest = f32::MAX;
        let mut beds: Vec<Vec<f32>> = Vec::with_capacity(rs.len());
        let mut widths: Vec<Vec<f32>> = Vec::with_capacity(rs.len());
        let mut mouth_k = 0;
        for (k, r) in rs.iter().enumerate() {
            let s = eased_f(&r.area.iter().map(|a| (a / amax).sqrt().min(1.0)).collect::<Vec<_>>(), half);
            let mut bed: Vec<f32> = Vec::with_capacity(r.pts.len());
            for (j, level) in r.level.iter().enumerate() {
                let raw = level - lerp(self.depth, s[j]);
                // Ice only ever grinds down along its way.
                bed.push(if j == 0 { raw } else { raw.min(bed[j - 1]) });
            }
            let mut bed: Vec<f32> = eased_f(&bed, half * 2).into_iter().map(|b| b.max(self.floor_min)).collect();
            if k == trunk {
                // The open sea: where the land was already under water
                // before the ice came.
                mouth_k = r.pts.iter().position(|p| g.h[g.idx(*p)] < sea).unwrap_or(r.pts.len() - 1);
                if let Some(m) = &self.sill {
                    let at = r.pts[back(&r.pts, mouth_k, m.inset)];
                    for (j, p) in r.pts.iter().enumerate() {
                        let t = 1.0 - smoothstep(0.0, m.length.max(1.0), p.distance(at));
                        bed[j] = bed[j].max(bed[j] + (m.top - bed[j]) * t * t * (3.0 - 2.0 * t));
                    }
                }
            }
            let halves: Vec<f32> = s.iter().map(|t| lerp(self.width, *t) * 0.5).collect();
            let mean = s.iter().sum::<f32>() / s.len() as f32;
            let wall = Wall { rim: self.rim, side: lerp(self.side, mean), shape: self.shape, ledge: self.ledge, floor_mat };
            let (cells, low) = carve(&mut cx.f, &r.pts, &bed, &halves, &wall, seed.wrapping_add(k as u32 * 7), cause);
            cut_total += cells;
            deepest = deepest.min(low);
            beds.push(bed);
            widths.push(halves);
        }
        // Cirques at the heads, and the names.
        let mut bowls = 0;
        for (k, r) in rs.iter().enumerate() {
            let n = r.pts.len();
            let v = named[k];
            let own = v.and_then(|v| self.valleys[v].cirque.as_ref());
            let head = r.pts[0];
            let down = (r.pts[(n - 1).min(12)] - head).normalize_or_zero();
            let high = self.cirque_above.is_some_and(|a| r.level[0] >= a) && k != trunk && r.length() > 90.0;
            let mut centre = head;
            if own.is_some() || high {
                let radius = match own {
                    Some(b) => b.radius,
                    None => (widths[k][0] * 2.0 + 14.0) * (0.8 + 0.4 * unit(hash32(seed ^ (k as u32).wrapping_mul(0x9e37)))),
                };
                centre = head - down * radius * 0.3;
                let floor = beds[k][0] - 1.5;
                let (ice, ice_m) = match (v, own) {
                    (Some(v), Some(b)) => (ices[v], b.ice_m),
                    _ => (None, 0.0),
                };
                bowl(&mut cx.f, centre, radius, floor, floor + radius * 0.95, down, ice, ice_m, seed.wrapping_add(900 + k as u32), cause);
                bowls += 1;
                if let Some(name) = own.and_then(|b| b.poi.as_ref()) {
                    cx.pois.push(Poi { name: name.clone(), pos: centre, cause });
                }
            }
            let Some(v) = v else {
                continue;
            };
            let name = &self.valleys[v].name;
            // The lip: where the trunk has taken the floor away.
            let lip = (0..n).find(|&j| cx.f.height(r.pts[j].x, r.pts[j].y) < beds[k][j] - 4.0).map_or(n - 1, |j| j.saturating_sub(1));
            let above = back(&r.pts, lip, 30.0);
            let out_dir = (r.pts[lip] - r.pts[back(&r.pts, lip, 24.0)]).normalize_or_zero();
            cx.seat(&format!("{name}_head"), centre, Some(bearing_of(down)));
            cx.seat(&format!("{name}_lip"), r.pts[lip], Some(bearing_of(out_dir)));
            cx.seat(&format!("{name}_above"), r.pts[above], Some(bearing_of(out_dir)));
            cx.seat(&format!("{name}_out"), r.pts[lip] + out_dir * 90.0, None);
            for q in 1..=3 {
                cx.seat(&format!("{name}_{q}"), r.pts[above * q / 4], None);
            }
            if let Some(poi) = &self.valleys[v].poi {
                cx.pois.push(Poi { name: poi.clone(), pos: r.pts[above / 2], cause });
            }
        }
        // The trunk's own names.
        let (r, bed) = (&rs[trunk], &beds[trunk]);
        let n = r.pts.len();
        let name = &self.name;
        let head = (0..n).find(|&j| bed[j] < sea - 0.3).unwrap_or(n - 1);
        let up = (r.pts[back(&r.pts, head, 40.0)] - r.pts[(head + 20).min(n - 1)]).normalize_or_zero();
        cx.seat(&format!("{name}_head"), r.pts[head], Some(bearing_of(up)));
        cx.seat(&format!("{name}_valley"), r.pts[back(&r.pts, head, 150.0)], Some(bearing_of(up)));
        let mouth = mouth_k.max(head);
        cx.seat(&format!("{name}_mouth"), r.pts[mouth], None);
        if let Some(m) = &self.sill {
            cx.seat(&format!("{name}_sill"), r.pts[back(&r.pts, mouth, m.inset)], None);
        }
        for q in 1..=9 {
            cx.seat(&format!("{name}_{q}"), r.pts[head + (mouth - head) * q / 10], None);
        }
        if let Some(poi) = &self.poi {
            cx.pois.push(Poi { name: poi.clone(), pos: r.pts[head + (mouth - head) / 2], cause });
        }
        let hp = r.pts[head];
        // Where each tributary runs, so an author can name one (`valleys`).
        let list: Vec<String> = rs.iter().take(trunk).rev().map(|t| format!("{:.0},{:.0}->{:.0},{:.0}", t.pts[0].x, t.pts[0].y, t.pts[t.pts.len() - 1].x, t.pts[t.pts.len() - 1].y)).collect();
        format!(
            "{} glaciers, the trunk {:.0} m long; {cut_total} m2 cut, deepest {deepest:.0} m; {bowls} cirques; fjord head at {:.0},{:.0}, mouth at {:.0},{:.0}; tributaries, biggest first: {}",
            rs.len(),
            r.length(),
            hp.x,
            hp.y,
            r.pts[mouth].x,
            r.pts[mouth].y,
            list.join(" ")
        )
    }
}
