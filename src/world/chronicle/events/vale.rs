use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{densify, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A soft river valley along `path` (head first): a floor `width` m wide
/// about `depth` m under the ground round it, falling gently downstream, and
/// grassy sides that ease up to the old ground over `side` m with no rim or
/// bare rock. The lowland counterpart of `canyon`; run a `river` along the
/// same path afterwards for water.
#[derive(Deserialize, Debug)]
pub struct Vale {
    path: Vec<Pos>,
    width: f32,
    depth: f32,
    /// Depth at the mouth, if different: changes evenly along the path.
    #[serde(default)]
    depth_end: Option<f32>,
    /// Run of each side, metres from the floor's edge to the old ground.
    side: f32,
    #[serde(default = "default_meander")]
    meander: f32,
    /// Steepest fall of the floor downstream, as run per metre of drop (7 =
    /// 1 in 7). Where the ground drops away (a scarp) the floor above is cut
    /// deeper, so a combe opens out at the foot instead of ending in a step.
    /// Unset: no limit (a river valley that goes over a fall).
    #[serde(default)]
    fall: Option<f32>,
    /// Bend the path into a smooth curve through its points first (see `canyon`).
    #[serde(default)]
    curve: bool,
}

fn default_meander() -> f32 {
    6.0
}

impl Process for Vale {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let pts = if self.curve { crate::world::chronicle::geom::spline(&pts, false, 8.0) } else { pts };
        let line = densify(&pts, self.meander, cx.seed);
        let (seed, cause) = (cx.seed, cx.cause);
        let f = &mut cx.f;
        if line.len() < 2 {
            return "skipped".into();
        }
        let depth_end = self.depth_end.unwrap_or(self.depth);
        let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>().max(1.0);
        // The floor follows the ground averaged across the valley, so a knoll
        // on one side doesn't lift it, and never rises downstream.
        let mut bed: Vec<f32> = Vec::with_capacity(line.len());
        let mut along = 0.0;
        for (k, p) in line.iter().enumerate() {
            if k > 0 {
                along += p.distance(line[k - 1]);
            }
            let d = if k + 1 < line.len() { line[k + 1] - *p } else { *p - line[k - 1] };
            let n = Vec2::new(-d.y, d.x).normalize_or_zero() * (self.width * 0.5 + self.side * 0.5);
            let g = [*p, *p + n, *p - n].iter().map(|q| f.height(q.x, q.y)).fold(f32::MAX, f32::min);
            let dk = self.depth + (depth_end - self.depth) * (along / total);
            let want = (g - dk).max(f.sea + 0.5);
            bed.push(if k == 0 { want } else { want.min(bed[k - 1] - 0.005) });
        }
        if let Some(run) = self.fall {
            for k in (0..bed.len().saturating_sub(1)).rev() {
                let ds = line[k].distance(line[k + 1]);
                bed[k] = bed[k].min(bed[k + 1] + ds / run.max(0.5));
            }
        }
        let half = self.width * 0.5;
        let reach = half + self.side + 8.0;
        let mut best: HashMap<usize, (f32, f32)> = HashMap::default();
        for (k, p) in line.iter().enumerate() {
            each_near(f, *p, reach, |_f: &mut Fields, i, d, _| {
                let e = best.entry(i).or_insert((f32::MAX, 0.0));
                if d < e.0 {
                    *e = (d, bed[k]);
                }
            });
        }
        let mut cells = 0;
        for (i, (d, b)) in best {
            let p = f.centre(i);
            // The floor's edge and the sides' run wander, so the valley is
            // wider in places and its shoulders never read as a line.
            let wob = perlin2(p.x * 0.03, p.y * 0.03, seed) * half * 0.5;
            let run = self.side * (1.0 + 0.35 * perlin2(p.x * 0.02, p.y * 0.02, seed + 1));
            let x = (d - half + wob).max(0.0);
            let s = smoothstep(0.0, run.max(1.0), x);
            let target = b + (f.h[i] - b) * s;
            if target < f.h[i] - 0.02 {
                f.h[i] = target;
                f.cause[i] = cause;
                cells += 1;
            }
        }
        format!("vale {total:.0} m long, floor {:.1} -> {:.1} m, {cells} m2 lowered", bed[0], bed[bed.len() - 1])
    }
}
