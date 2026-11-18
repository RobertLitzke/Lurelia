use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{each_near, stamp_path};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::perlin2;
use crate::world::route::plan::{self, Layout, Plot};
use crate::world::route::{self, ribbon::Ribbons};
use glam::Vec2;
use serde::Deserialize;

/// A settlement laid out on its own streets. The layout decides where the
/// streets run; the plots come off their frontages, so buildings stand in
/// rows facing the street they are on, with a path from each door to the
/// kerb — rather than scattered in a disc all facing the middle.
#[derive(Deserialize, Debug)]
pub struct Village {
    name: String,
    at: Pos,
    radius: f32,
    /// grid, radial, organic or linear.
    layout: String,
    /// The kit for the main streets.
    street: String,
    /// The kit for the lanes; the street's kit when left out.
    #[serde(default)]
    lane: Option<String>,
    houses: u32,
    buildings: Vec<String>,
    /// Which way the plan is turned, degrees. A grid or a linear plan reads
    /// best on a quarter turn: its buildings then sit square to their street.
    #[serde(default)]
    bearing: f32,
    /// Metres between streets (grid), or the length of a lane off one.
    #[serde(default = "default_block")]
    block: f32,
    /// Metres between plots along a frontage.
    #[serde(default = "default_spacing")]
    spacing: f32,
    /// Metres from the street edge to the front of a building.
    #[serde(default = "default_setback")]
    setback: f32,
    /// Half the square of ground a building needs, metres. Small on a sand
    /// island where a round house is all that fits; large for a farmstead.
    #[serde(default = "default_plot")]
    plot: f32,
    /// Material for the path from each door to the kerb.
    #[serde(default)]
    walk: Option<String>,
    /// A ribbon round the back of the plots: a wall, a hedge, a fence.
    #[serde(default)]
    edge: Option<String>,
    #[serde(default)]
    extras: Vec<Extra>,
    #[serde(default)]
    slots: HashMap<String, String>,
}

#[derive(Deserialize, Debug)]
struct Extra {
    structure: String,
    #[serde(default = "one")]
    count: u32,
    /// centre or edge.
    #[serde(default = "centre")]
    near: String,
}

fn one() -> u32 {
    1
}
fn centre() -> String {
    "centre".into()
}
fn default_block() -> f32 {
    46.0
}
fn default_spacing() -> f32 {
    16.0
}
fn default_setback() -> f32 {
    3.0
}
fn default_plot() -> f32 {
    5.0
}

impl Process for Village {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(&self.at);
        let Some(layout) = Layout::read(&self.layout) else {
            cx.warnings.push(format!("village {}: unknown layout {}", self.name, self.layout));
            return "skipped".into();
        };
        let Some(main) = route::kit_index(cx, &self.street) else {
            return format!("skipped: unknown kit {}", self.street);
        };
        let lane = match &self.lane {
            Some(l) => route::kit_index(cx, l).unwrap_or(main),
            None => main,
        };
        let streets = plan::streets(layout, c, self.radius, self.bearing, self.block, seed);

        // Aprons where the plan crosses itself, laid before the streets so the
        // grading runs over them rather than under.
        let mut crossings: Vec<Vec2> = Vec::new();
        for i in 0..streets.len() {
            for j in i + 1..streets.len() {
                for a in streets[i].pts.windows(2) {
                    for b in streets[j].pts.windows(2) {
                        if let Some(p) = cross(a[0], a[1], b[0], b[1]) {
                            if crossings.iter().all(|q| q.distance(p) > 6.0) {
                                crossings.push(p);
                            }
                        }
                    }
                }
            }
        }

        // Lay the streets, keeping each line for the plots.
        let mut laid: Vec<(Vec<Vec2>, f32, bool)> = Vec::new();
        let mut metres = 0.0;
        for s in &streets {
            let k = if s.rank == 0 { main } else { lane };
            let half = cx.state.get::<Ribbons>().kits[k].half;
            for run in dry_runs(&cx.f, &s.pts, half) {
                let line = crate::world::chronicle::geom::spline(&run, false, 1.0);
                if line.len() < 4 {
                    continue;
                }
                let l = route::lay(cx, &line, k, 30.0);
                metres += l.length;
                laid.push((line, half, s.rank == 0));
            }
        }
        let mat = cx.state.get::<Ribbons>().kits[main].mat;
        for p in &crossings {
            route::junction(cx, *p, 4.0, mat);
        }

        // Plots along the frontages, main streets first.
        let walk = self.walk.as_ref().map(|w| cx.mat(w));
        let defs: Vec<usize> = self
            .buildings
            .iter()
            .filter_map(|b| {
                let d = cx.lib.structure(b);
                if d.is_none() {
                    cx.warnings.push(format!("village {}: unknown structure {b}", self.name));
                }
                d
            })
            .collect();
        if defs.is_empty() {
            return format!("{}: {metres:.0} m of streets, no buildings to put on them", self.name);
        }
        let mut order: Vec<usize> = (0..laid.len()).collect();
        order.sort_by_key(|&k| !laid[k].2);
        let mut sites: Vec<Vec2> = Vec::new();
        let mut members = Vec::new();
        let mut want = self.houses as usize;
        let (mut tried, mut rejected_site) = (0usize, 0usize);
        // Two passes: the first spreads the houses over every frontage, the
        // second fills up wherever there is still room. Without the quota a
        // long main street takes the whole village and the lanes stand empty.
        let quota = (self.houses as usize).div_ceil(laid.len().max(1)).max(2);
        for pass in 0..2 {
            for &k in &order {
            if want == 0 {
                break;
            }
            let mut here = 0usize;
            let (line, half, _) = &laid[k];
            let f = &cx.f;
            let taken = sites.clone();
            let (plot, apart) = (self.plot, (self.plot * 2.2).max(8.0));
            let found: Vec<Plot> = plan::plots(line, *half, self.setback, self.spacing, plot * 1.6, seed ^ k as u32, |p, kerb| {
                tried += 1;
                let ok = plan::plot_ok(f, p, kerb, plot) && taken.iter().all(|s| s.distance(p) > apart);
                if !ok {
                    rejected_site += 1;
                }
                ok
            });
            for pl in found {
                if want == 0 || (pass == 0 && here >= quota) {
                    break;
                }
                if sites.iter().any(|s| s.distance(pl.at) <= apart) {
                    continue;
                }
                let d = defs[members.len() % defs.len()];
                let Some(ix) = cx.place(d, pl.at, pl.face, &self.slots, 0.0, seed ^ (sites.len() as u32 + 1)) else {
                    continue;
                };
                members.push(ix);
                sites.push(pl.at);
                want -= 1;
                here += 1;
                if let Some(m) = walk {
                    stamp_path(&mut cx.f, &[pl.at, pl.kerb], 1.2, m);
                }
            }
            }
        }

        // Extras: a church, a well, a market cross.
        for ex in &self.extras {
            let Some(d) = cx.lib.structure(&ex.structure) else {
                cx.warnings.push(format!("village {}: unknown structure {}", self.name, ex.structure));
                continue;
            };
            for j in 0..ex.count {
                let want_far = ex.near == "edge";
                let mut best: Option<(f32, Vec2)> = None;
                for ring in 0..240 {
                    let a = ring as f32 * 2.399_963;
                    let rr = (ring as f32 / 240.0).sqrt() * self.radius;
                    let p = c + Vec2::new(a.cos(), a.sin()) * rr;
                    if !plan::site_ok(&cx.f, p, self.plot + 1.0) || sites.iter().any(|s| s.distance(p) < self.plot * 2.2) {
                        continue;
                    }
                    let score = if want_far { -rr } else { rr };
                    if best.is_none_or(|b| score < b.0) {
                        best = Some((score, p));
                    }
                }
                let Some((_, p)) = best else {
                    cx.warnings.push(format!("village {}: no site for {} #{j}", self.name, ex.structure));
                    continue;
                };
                sites.push(p);
                let bearing = crate::util::bearing(c - p).to_degrees();
                if let Some(ix) = cx.place(d, p, bearing, &self.slots, 0.0, seed ^ (500 + sites.len()) as u32) {
                    members.push(ix);
                }
            }
        }

        // A wall or hedge round the whole thing.
        if let Some(e) = &self.edge {
            if let Some(k) = route::kit_index(cx, e) {
                let ring: Vec<Vec2> = (0..28)
                    .map(|j| {
                        let t = j as f32 / 28.0 * std::f32::consts::TAU;
                        let d = Vec2::new(t.cos(), t.sin());
                        c + d * self.radius * (1.02 + 0.08 * perlin2(d.x * 2.0, d.y * 2.0, seed ^ 17))
                    })
                    .collect();
                let line = crate::world::chronicle::geom::spline(&ring, true, 1.0);
                route::lay(cx, &line, k, 12.0);
            }
        }

        let rid = cx.f.region_id(&self.name);
        let radius = self.radius;
        each_near(&mut cx.f, c, radius * 1.5, |f: &mut Fields, i, d, p| {
            if d < radius * (1.12 + 0.28 * perlin2(p.x * 0.03, p.y * 0.03, seed)) {
                f.region[i] = rid;
            }
        });
        cx.state.get::<super::settlement::Settlements>().0.insert(self.name.clone(), members.clone());
        cx.pois.push(Poi {
            name: self.name.clone(),
            pos: c,
            cause,
        });
        format!(
            "{}: {} layout, {metres:.0} m of streets, {} crossings, {} buildings on their frontages ({tried} plots tried, {rejected_site} refused)",
            self.name,
            self.layout,
            crossings.len(),
            members.len()
        )
    }
}

/// Where two segments cross, if they do away from their ends.
fn cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    if den.abs() < 1e-5 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    ((0.02..0.98).contains(&t) && (0.02..0.98).contains(&u)).then(|| a + r * t)
}

/// Split a street line at the water it would cross, so a plan laid over a
/// pond or a river leaves the wet stretch out instead of paving it.
fn dry_runs(f: &Fields, pts: &[Vec2], half: f32) -> Vec<Vec<Vec2>> {
    let mut fine: Vec<Vec2> = Vec::new();
    for w in pts.windows(2) {
        let steps = (w[0].distance(w[1]) / 4.0).ceil().max(1.0) as usize;
        for s in 0..steps {
            fine.push(w[0].lerp(w[1], s as f32 / steps as f32));
        }
    }
    if let Some(&l) = pts.last() {
        fine.push(l);
    }
    let mut out = Vec::new();
    let mut run: Vec<Vec2> = Vec::new();
    for p in fine {
        let i = f.idx(p.x, p.y);
        let dry = p.x > half && p.y > half && p.x < f.size - half && p.y < f.size - half && f.water[i] != 1 && f.h[i] > f.sea + 0.5;
        if dry {
            run.push(p);
        } else if run.len() >= 3 {
            out.push(std::mem::take(&mut run));
        } else {
            run.clear();
        }
    }
    if run.len() >= 3 {
        out.push(run);
    }
    out
}
