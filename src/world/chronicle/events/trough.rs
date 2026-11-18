use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{densify, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A glacial trough along `path` (head first): the U-shaped valley a glacier
/// leaves, not the V a river cuts. A flat floor `width` m across at an
/// absolute height (`floor`, `floor_end` at the mouth, which may be well
/// under the sea: a fjord), and walls that rise steeply from it and curve up
/// toward `rim` over `side` m, steepening as they climb, so the foot is a
/// smooth curve and the upper wall a cliff. It only lowers the ground.
///
/// A tributary trough with a higher floor, cut first, is left hanging where
/// the main trough's wall crosses its mouth: its floor ends at a lip high on
/// the wall, which is where a hanging valley's waterfall goes over.
///
/// `sill`: the moraine the glacier dropped where it stopped, a ridge across
/// the floor near the mouth that the sea only just covers.
#[derive(Deserialize, Debug)]
pub struct Trough {
    path: Vec<Pos>,
    width: f32,
    /// Floor width at the mouth, if different.
    #[serde(default)]
    width_end: Option<f32>,
    /// Absolute height of the floor at the head (m).
    floor: f32,
    /// At the mouth, if different; the floor falls evenly along the path.
    #[serde(default)]
    floor_end: Option<f32>,
    /// Absolute height the walls climb toward (m).
    rim: f32,
    /// Horizontal run of each wall, floor edge to rim (m).
    side: f32,
    /// How the wall steepens: 1 a straight slope, 2.5 (the default) a glacial
    /// U, higher a flatter foot under a sheerer cliff.
    #[serde(default = "default_shape")]
    shape: f32,
    #[serde(default = "default_meander")]
    meander: f32,
    /// Height of the steps the walls climb in, m (0, the default: a smooth
    /// wall). Basalt walls built of lava flows step like this.
    #[serde(default)]
    ledge: f32,
    /// Floor material above the sea (a gravel flat at a fjord's head).
    #[serde(default)]
    floor_mat: Option<String>,
    #[serde(default)]
    sill: Option<Sill>,
    #[serde(default)]
    poi: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Sill {
    at: Pos,
    /// Absolute height of the sill's crest (m).
    top: f32,
    /// How far along the trough it spreads either way (m).
    #[serde(default = "default_sill_len")]
    length: f32,
}

fn default_shape() -> f32 {
    2.5
}
fn default_meander() -> f32 {
    4.0
}
fn default_sill_len() -> f32 {
    40.0
}

impl Process for Trough {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let line = densify(&pts, self.meander, cx.seed);
        let floor_m = self.floor_mat.as_ref().map(|m| cx.mat(m));
        let sill = self.sill.as_ref().map(|s| (cx.pos(&s.at), s.top, s.length.max(1.0)));
        let (seed, cause) = (cx.seed, cx.cause);
        if line.len() < 2 {
            return "skipped".into();
        }
        let f = &mut cx.f;
        let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>().max(1.0);
        let floor_end = self.floor_end.unwrap_or(self.floor);
        let width_end = self.width_end.unwrap_or(self.width);
        // Floor and half-width at each point of the line, and the sill as a
        // smooth hump on the floor.
        let mut bed = Vec::with_capacity(line.len());
        let mut halves = Vec::with_capacity(line.len());
        let mut along = 0.0;
        for (k, p) in line.iter().enumerate() {
            if k > 0 {
                along += p.distance(line[k - 1]);
            }
            let t = along / total;
            let mut b = self.floor + (floor_end - self.floor) * t;
            if let Some((at, top, len)) = sill {
                let s = 1.0 - smoothstep(0.0, len, p.distance(at));
                b = b.max(b + (top - b) * s * s * (3.0 - 2.0 * s));
            }
            bed.push(b);
            halves.push(0.5 * (self.width + (width_end - self.width) * t));
        }
        let reach = halves.iter().cloned().fold(0.0, f32::max) + self.side + 16.0;
        let mut best: HashMap<usize, (f32, usize)> = HashMap::default();
        for (k, p) in line.iter().enumerate() {
            each_near(f, *p, reach, |_f: &mut Fields, i, d, _| {
                let e = best.entry(i).or_insert((f32::MAX, 0));
                if d < e.0 {
                    *e = (d, k);
                }
            });
        }
        let p_exp = self.shape.max(1.0);
        let mut cells = 0;
        let mut deepest = f32::MAX;
        for (i, (d, k)) in best {
            let p = f.centre(i);
            let (b, half) = (bed[k], halves[k]);
            // The walls wander in and out, in long bays and short spurs, and
            // the run of each stretch of wall varies, so neither the floor's
            // edge nor the skyline is a ruled line.
            let wob = perlin2(p.x * 0.012, p.y * 0.012, seed) * half * 0.25 + perlin2(p.x * 0.05, p.y * 0.05, seed + 1) * 4.0;
            let run = self.side * (1.0 + 0.3 * perlin2(p.x * 0.008, p.y * 0.008, seed + 2));
            let x = (d - half + wob).max(0.0);
            let t = (x / run.max(1.0)).min(1.0);
            // Gullies: avalanche chutes notched into the upper wall.
            let chute = (1.0 - (perlin2(p.x * 0.045, p.y * 0.045, seed + 3) * 3.0).abs()).max(0.0).powi(3) * t * 10.0;
            let smooth_wall = b + (self.rim - b) * t.powf(p_exp) - chute;
            // Lava-flow ledges: the wall steps up in risers about `ledge` m
            // high, each with a narrow shelf on top that holds snow. The
            // shelves sit on fixed heights, so they run level along the wall
            // and line up across the fjord.
            let (target, shelf) = if self.ledge > 0.0 && t > 0.08 {
                // Each flow is thicker in some places than others, and a
                // shelf widens, narrows and in places pinches out.
                let lh = self.ledge * (1.0 + 0.22 * perlin2(p.x * 0.006, p.y * 0.006, seed + 6));
                // Measured against the beds, which a `dip` may have tilted.
                let q = (smooth_wall - b + 3.0 * perlin2(p.x * 0.01, p.y * 0.01, seed + 7) - f.lift_at(i)) / lh;
                let fr = q.fract();
                let width = 0.3 + 0.16 * perlin2(p.x * 0.03 + q.floor() * 7.1, p.y * 0.03, seed + 8);
                let stepped = (q.floor() + smoothstep(width, 1.0, fr)) * lh + b;
                // Never stepped below the floor (a tilted bed can put the
                // first step under it).
                ((smooth_wall + (stepped - smooth_wall) * 0.85).max(b), fr < width - 0.02)
            } else {
                (smooth_wall, false)
            };
            if target < f.h[i] - 0.05 {
                f.h[i] = target;
                f.cause[i] = cause;
                cells += 1;
                deepest = deepest.min(target);
                // Steep wall is bare cliff, in bands and buttresses: where the
                // threshold dips the rock shows, where it rises snow holds on
                // the ledges between.
                let grad = (self.rim - b) * p_exp * t.powf(p_exp - 1.0) / run.max(1.0);
                let holds = 1.3 + 0.9 * perlin2(p.x * 0.035, p.y * 0.035, seed + 4) + 0.5 * perlin2(p.x * 0.11, p.y * 0.11, seed + 5);
                // The chutes hold snow all the way down: white couloirs
                // between the buttresses.
                if grad > holds && chute < 2.0 && !shelf {
                    f.cliff[i] = 1.0;
                } else {
                    f.cliff[i] = 0.0;
                }
                if f.water[i] == 3 {
                    f.water[i] = 0;
                }
                f.surface[i] = 0;
                if x <= 2.0 && target > f.sea {
                    if let Some(m) = floor_m {
                        f.surface[i] = m;
                    }
                }
            }
        }
        if let (Some(name), Some(&head)) = (&self.poi, pts.first()) {
            cx.pois.push(Poi { name: name.clone(), pos: head, cause });
        }
        format!("trough {total:.0} m long, floor {:.0} -> {:.0} m, deepest {deepest:.0} m, {cells} m2 cut", bed[0], bed[bed.len() - 1])
    }
}
