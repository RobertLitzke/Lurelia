use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A long rock face: the ground on the right of the line from `from` to `to`
/// (facing along it) stands `height` m above the left, with a cliff between.
///
/// Without `reach` only cells within a circle round the line's middle
/// (half its length + 4 widths + 40 m) change, so on the raised side the
/// rise ends in a circular step. With `reach` the whole raised side within
/// that many metres of the line rises, fading out over 40 m beyond it: use
/// it for long cliff lines that cross a world.
///
/// With `path` (five or more points that are not in a row) the face follows
/// a curve through them instead of the straight line from `from` to `to`:
/// it swings out in noses and back in bays, stands taller on some stretches
/// than others, and the ground on its right, facing along the path, is the
/// high side. Start and end it off the map, or the face stops dead. Give
/// `reach`. Follow it with `drainage` and streams cut combes into it.
#[derive(Deserialize, Debug)]
pub struct Escarpment {
    #[serde(default = "origin")]
    from: Pos,
    #[serde(default = "origin")]
    to: Pos,
    #[serde(default)]
    path: Vec<Pos>,
    /// With `path`: name the raised side as a region, for a biome that
    /// should cover exactly the high ground behind the face.
    #[serde(default)]
    region: Option<String>,
    height: f32,
    #[serde(default = "default_width")]
    width: f32,
    #[serde(default)]
    reach: Option<f32>,
}

fn default_width() -> f32 {
    10.0
}

fn origin() -> Pos {
    Pos::At([0.0, 0.0])
}

/// The signed-distance field is worked out on a grid this coarse.
const COARSE: usize = 4;

impl Escarpment {
    /// The curved form: height by signed distance from a curve through `pts`.
    fn along_path(&self, cx: &mut EventCtx, pts: &[Vec2]) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let line = crate::world::chronicle::geom::spline(pts, false, 6.0);
        let f = &mut cx.f;
        let n = f.n;
        let cn = n / COARSE + 2;
        // Distance to the curve, positive on its right.
        let mut sd = vec![0.0f32; cn * cn];
        for cz in 0..cn {
            for cxi in 0..cn {
                let p = Vec2::new(cxi as f32 - 0.5, cz as f32 - 0.5) * COARSE as f32;
                let mut best = (f32::MAX, 0usize);
                for (k, q) in line.iter().enumerate() {
                    let d = q.distance_squared(p);
                    if d < best.0 {
                        best = (d, k);
                    }
                }
                let k = best.1.min(line.len() - 2);
                let dir = (line[k + 1] - line[k]).normalize_or_zero();
                let right = Vec2::new(-dir.y, dir.x);
                sd[cz * cn + cxi] = best.0.sqrt() * (p - line[best.1]).dot(right).signum();
            }
        }
        let read = |p: Vec2| -> f32 {
            let (gx, gz) = ((p.x / COARSE as f32 + 0.5).clamp(0.0, cn as f32 - 1.001), (p.y / COARSE as f32 + 0.5).clamp(0.0, cn as f32 - 1.001));
            let (x0, z0) = (gx as usize, gz as usize);
            let (u, w) = (gx - x0 as f32, gz - z0 as f32);
            let a = sd[z0 * cn + x0] * (1.0 - u) + sd[z0 * cn + x0 + 1] * u;
            let b = sd[(z0 + 1) * cn + x0] * (1.0 - u) + sd[(z0 + 1) * cn + x0 + 1] * u;
            a * (1.0 - w) + b * w
        };
        let (height, width) = (self.height, self.width.max(2.0));
        let reach = self.reach.unwrap_or(f.size * 2.0);
        let rid = self.region.as_ref().map(|r| f.region_id(r));
        let mut raised = 0usize;
        for i in 0..n * n {
            let p = f.centre(i);
            // Noses and bays a few hundred metres across, and smaller ones on them.
            let wob = perlin2(p.x * 0.0045, p.y * 0.0045, seed) * width * 4.5 + perlin2(p.x * 0.017, p.y * 0.017, seed + 1) * width * 1.6;
            let s = read(p) - wob;
            if s < -width || f.h[i] < f.sea - 1.0 {
                continue;
            }
            let fade = smoothstep(reach + 40.0, reach, s);
            if fade <= 0.0 {
                continue;
            }
            // Taller here, lower there.
            let tall = 1.0 + 0.3 * perlin2(p.x * 0.0031 + 9.0, p.y * 0.0031, seed + 2);
            let t = smoothstep(-width * 0.5, width * 0.5, s);
            let step = smoothstep(0.35, 0.65, t);
            f.h[i] += height * tall * fade * (step * 0.85 + t * 0.15);
            if s.abs() < width * 0.45 && fade > 0.3 {
                f.cliff[i] = f.cliff[i].max(fade);
            }
            f.cause[i] = cause;
            if let (Some(rid), true) = (rid, s > width * 0.6 && fade > 0.5) {
                f.region[i] = rid;
            }
            raised += 1;
        }
        let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
        format!("escarpment {height} m high along {len:.0} m of curve, {raised} m2 raised")
    }
}

impl Process for Escarpment {
    fn apply(&self, cx: &mut EventCtx) -> String {
        if self.path.len() >= 2 {
            let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
            return self.along_path(cx, &pts);
        }
        let Escarpment { from, to, height, width, reach, .. } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let (a, b) = (cx.pos(from), cx.pos(to));
        let f = &mut cx.f;
        let len = a.distance(b).max(1.0);
        let d = (b - a) / len;
        let right = Vec2::new(-d.y, d.x);
        let mid = (a + b) * 0.5;
        let r = match reach {
            Some(_) => f.size * 2.0,
            None => len * 0.5 + width * 4.0 + 40.0,
        };
        each_near(f, mid, r, |f: &mut Fields, i, _, p| {
            let rel = p - a;
            let along = rel.dot(d);
            let fade = match reach {
                Some(k) => smoothstep(k + 40.0, *k, rel.dot(right)),
                None => 1.0,
            };
            if fade <= 0.0 {
                return;
            }
            // Fade out past the ends, and let the face wander a little.
            let fade = fade * smoothstep(-40.0, 0.0, along) * smoothstep(len + 40.0, len, along);
            if fade <= 0.0 || f.h[i] < f.sea - 1.0 {
                return;
            }
            let wob = perlin2(along * 0.02, 5.0, seed) * width * 1.5;
            let s = rel.dot(right) - wob;
            let t = smoothstep(-width * 0.5, width * 0.5, s);
            let step = smoothstep(0.35, 0.65, t);
            f.h[i] += height * fade * (step * 0.85 + t * 0.15);
            if (s.abs() < width * 0.45) && fade > 0.3 {
                f.cliff[i] = f.cliff[i].max(fade);
            }
            f.cause[i] = cause;
        });
        format!("escarpment {height} m high, {len:.0} m long")
    }
}
