use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{densify, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A gorge cut along `path` (head first): a flat floor `width` m wide falling
/// steadily downstream, walls that climb in benches `bench` m high to the
/// rim, `wall` m of run per metre of depth. Benches sit at fixed heights, so
/// they line up with the world's strata and from one canyon to the next.
#[derive(Deserialize, Debug)]
pub struct Canyon {
    path: Vec<Pos>,
    width: f32,
    depth: f32,
    /// Depth at the mouth, if different: the depth changes evenly along the path.
    #[serde(default)]
    depth_end: Option<f32>,
    #[serde(default = "default_wall")]
    wall: f32,
    #[serde(default = "default_bench")]
    bench: f32,
    #[serde(default = "default_meander")]
    meander: f32,
    /// Floor material (a sandy wash).
    #[serde(default)]
    floor: Option<String>,
    #[serde(default)]
    poi: Option<String>,
    /// 0 (the default): benches on fixed heights, level and even. Up to 1:
    /// bench heights, their level and the width of each ledge wander along
    /// the walls, and benches follow beds that a `reef` lifted.
    #[serde(default)]
    irregular: f32,
}

fn default_wall() -> f32 {
    0.6
}
fn default_bench() -> f32 {
    8.0
}
fn default_meander() -> f32 {
    6.0
}

impl Process for Canyon {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let line = densify(&pts, self.meander, cx.seed);
        let floor_m = self.floor.as_ref().map(|m| cx.mat(m));
        let msg = self.cut(cx, &line, floor_m);
        if let (Some(name), Some(&head)) = (&self.poi, pts.first()) {
            cx.pois.push(Poi { name: name.clone(), pos: head, cause: cx.cause });
        }
        msg
    }
}

impl Canyon {
    /// Cut the gorge along `line`, the path as densified.
    fn cut(&self, cx: &mut EventCtx, line: &[Vec2], floor_m: Option<u8>) -> String {
        let (width, depth, depth_end, wall, bench) = (self.width, self.depth, self.depth_end.unwrap_or(self.depth), self.wall, self.bench);
        let (seed, cause) = (cx.seed, cx.cause);
        let f = &mut cx.f;
        if line.len() < 2 {
            return "skipped".into();
        }
        // Floor along the line: `depth` below the ground, deepening over the first
        // 30 m from the head (a pour-off), never rising downstream.
        let mut bed: Vec<f32> = Vec::with_capacity(line.len());
        let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>().max(1.0);
        let mut along = 0.0;
        for (k, p) in line.iter().enumerate() {
            if k > 0 {
                along += p.distance(line[k - 1]);
            }
            let g = f.height(p.x, p.y);
            let dk = depth + (depth_end - depth) * (along / total);
            let want = (g - dk * smoothstep(0.0, 30.0, along).max(0.25)).max(f.sea - 3.0);
            bed.push(if k == 0 { want } else { want.min(bed[k - 1] - 0.01) });
        }
        // No steeper than 1 in 7 going downstream: where the ground drops (a
        // cliff line) the floor above is cut deeper instead of plunging.
        for k in (0..bed.len().saturating_sub(1)).rev() {
            let ds = line[k].distance(line[k + 1]);
            bed[k] = bed[k].min(bed[k + 1] + ds / 7.0);
        }
        let half = width * 0.5;
        let wall = wall.max(0.05);
        let reach = half + depth * wall * 1.4 + 12.0;
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
            // Walls wander in and out so the canyon is never a ruled channel.
            let wob = perlin2(p.x * 0.035, p.y * 0.035, seed) * 5.0 + perlin2(p.x * 0.11, p.y * 0.11, seed + 1) * 1.5;
            let x = (d - half + wob).max(0.0);
            // Climb at 1/wall, then terrace on fixed heights: each bench is flat
            // for most of its run and ends in a sheer riser.
            let y = b + x / wall;
            let ir = self.irregular;
            // Irregular walls: each stretch has its own bench height and
            // level (`ph`), and the beds may be tilted by a fold.
            let bk = bench * (1.0 + 0.35 * ir * perlin2(p.x * 0.011, p.y * 0.011, seed + 7));
            let ph = ir * (perlin2(p.x * 0.006, p.y * 0.006, seed + 8) * bench * 0.7 - f.lift_at(i));
            let q = (y + ph) / bk;
            let riser = 0.72 - 0.12 * ir + (0.1 + 0.16 * ir) * perlin2(p.x * 0.05, p.y * 0.05, seed + 2);
            let target = if x <= 0.0 { b } else { ((q.floor() + smoothstep(riser, 1.0, q.fract())) * bk - ph).max(b) };
            if target < f.h[i] - 0.05 {
                let drop = f.h[i] - target;
                f.h[i] = target;
                f.cause[i] = cause;
                cells += 1;
                if x > 0.0 && (q.fract() > riser - 0.04 || drop > bench * 1.5) {
                    f.cliff[i] = 1.0;
                }
                // Whatever lay on the ground here (a road cut by a later
                // flood) went with it.
                if drop > 0.8 && x > 1.0 {
                    f.surface[i] = 0;
                    if f.water[i] == 3 {
                        f.water[i] = 0;
                    }
                }
                if x <= 1.0 {
                    if let Some(m) = floor_m {
                        f.surface[i] = m;
                    }
                }
            }
        }
        let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
        format!("canyon {len:.0} m long, floor {:.0} -> {:.0} m, {cells} m2 cut", bed[0], bed[bed.len() - 1])
    }
}
