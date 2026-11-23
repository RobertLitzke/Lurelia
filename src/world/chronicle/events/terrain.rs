use crate::world::chronicle::geom::{dir_deg, spline};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::noise::{fbm2, ridged2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// The land before anything happens to it, from a loose sketch. Where
/// `landmass` is one round island, this is whatever the sketch says: high
/// ground here, a basin there, a broad valley wandering between them, the
/// sea along one edge or none. Follow it with `drainage` (and `glaciate`,
/// in ice country) so the valleys are the ones water would cut.
///
/// The sketch is deliberately coarse, and nothing in it comes out as drawn.
/// Every point is read through a warp, so a circle comes out as a lobed
/// massif and a three-point course as a valley with bends of its own. The
/// hills, ridges and knobbly ground change in strength from one part of the
/// map to another (`vary`), so one quarter is rugged and the next is plain.
#[derive(Deserialize, Debug)]
pub struct Terrain {
    /// The general height of the land, metres above the world's floor.
    base: f32,
    /// Height of the rolling hills, m.
    #[serde(default)]
    hills: f32,
    /// Height of the sharp ridges that stand in some of the hills, m.
    #[serde(default)]
    ridges: f32,
    /// Height of the knobs and hollows a stone's throw across, m.
    #[serde(default)]
    rough: f32,
    /// How unevenly the three above are spread, 0 (the same everywhere) to 1.
    #[serde(default = "default_vary")]
    vary: f32,
    /// How far the sketch wanders from where it was drawn, m.
    #[serde(default = "default_warp")]
    warp: f32,
    /// Stretch every hill and ridge by this (1: hills about 200 m across).
    #[serde(default = "one")]
    scale: f32,
    /// High ground and basins.
    #[serde(default)]
    relief: Vec<Bump>,
    /// Broad valleys: where the land sags along a line, for water to find.
    #[serde(default)]
    courses: Vec<Course>,
    /// Which edges of the world are sea. None: land to every horizon.
    #[serde(default)]
    sea: Vec<Coast>,
    /// Depth of the sea floor off the coast, m.
    #[serde(default = "default_shelf")]
    shelf: f32,
    /// "add": lay all of this over the ground already there (a `heightmap`'s,
    /// a race track's fitted land) instead of replacing it, `base` measured
    /// from that ground. The sea still takes the coasts.
    #[serde(default)]
    mode: Option<String>,
}

/// A rise (`height` above zero) or a basin (below), fading out over `radius`.
#[derive(Deserialize, Debug)]
struct Bump {
    at: Pos,
    radius: f32,
    height: f32,
    /// [bearing, factor]: drawn out along a bearing into a range or a trough.
    #[serde(default)]
    stretch: Option<[f32; 2]>,
}

#[derive(Deserialize, Debug)]
struct Course {
    path: Vec<Pos>,
    /// How far to either side the sag reaches, m.
    width: f32,
    /// How far the land is let down along the line, m.
    depth: f32,
    /// At the far end, if different.
    #[serde(default)]
    depth_end: Option<f32>,
    /// 0 (the default) is a soft sag. Toward 1 it is a block let down
    /// between two faults: a level floor four fifths of `width` either
    /// side of the line, with a steep wall up each side of it.
    #[serde(default)]
    walls: f32,
}

#[derive(Deserialize, Debug)]
struct Coast {
    /// "north", "south", "east" or "west".
    edge: String,
    /// How far in from the edge the coast runs, on average, m.
    reach: f32,
    /// How far headlands and bays swing either side of that, m.
    #[serde(default)]
    bays: Option<f32>,
    /// Metres from the waterline to the land's full height: 30 a cliffed
    /// coast, 200 a long low shore.
    #[serde(default = "default_rise")]
    rise: f32,
    /// Rocks and islets off the shore, 0 to 1.
    #[serde(default)]
    skerries: f32,
}

fn default_vary() -> f32 {
    0.6
}
fn default_warp() -> f32 {
    60.0
}
fn one() -> f32 {
    1.0
}
fn default_shelf() -> f32 {
    14.0
}
fn default_rise() -> f32 {
    60.0
}

/// The sketch is worked out on a grid this coarse and read between its
/// points: it is smooth, and a course is a long line to measure against.
const SKETCH: usize = 4;

impl Process for Terrain {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let noise_at = cx.noise_at;
        let bumps: Vec<(Vec2, f32, f32, Option<(Vec2, f32)>)> =
            self.relief.iter().map(|b| (cx.pos(&b.at), b.radius.max(1.0), b.height, b.stretch.map(|s| (dir_deg(s[0]), s[1].max(1.0))))).collect();
        let courses: Vec<(Vec<Vec2>, f32, f32, f32, f32)> = self
            .courses
            .iter()
            .map(|c| {
                let pts: Vec<Vec2> = c.path.iter().map(|p| cx.pos(p)).collect();
                (spline(&pts, false, 12.0), c.width.max(1.0), c.depth, c.depth_end.unwrap_or(c.depth), c.walls.clamp(0.0, 1.0))
            })
            .collect();
        let f = &mut cx.f;
        let (n, size, sea) = (f.n, f.size, f.sea);
        // The sketch on its coarse grid.
        let sn = n / SKETCH + 2;
        let mut sketch = vec![0.0f32; sn * sn];
        for sz in 0..sn {
            for sx in 0..sn {
                let p = Vec2::new(sx as f32 - 0.5, sz as f32 - 0.5) * SKETCH as f32;
                let mut h = self.base;
                for (c, r, height, stretch) in &bumps {
                    let mut v = p - *c;
                    if let Some((axis, k)) = stretch {
                        let along = v.dot(*axis);
                        v -= *axis * along * (1.0 - 1.0 / k);
                    }
                    h += height * (-(v.length_squared() / (r * r))).exp();
                }
                for (line, width, d0, d1, walls) in &courses {
                    let mut best = (f32::MAX, 0usize);
                    for (k, q) in line.iter().enumerate() {
                        let d = q.distance_squared(p);
                        if d < best.0 {
                            best = (d, k);
                        }
                    }
                    let t = best.1 as f32 / (line.len() - 1).max(1) as f32;
                    let soft = (-(best.0 / (width * width))).exp();
                    let block = smoothstep(width * 1.05, width * 0.8, best.0.sqrt());
                    h -= (d0 + (d1 - d0) * t) * (soft + (block - soft) * walls);
                }
                sketch[sz * sn + sx] = h;
            }
        }
        let read = |p: Vec2| -> f32 {
            let (gx, gz) = ((p.x / SKETCH as f32 + 0.5).clamp(0.0, sn as f32 - 1.001), (p.y / SKETCH as f32 + 0.5).clamp(0.0, sn as f32 - 1.001));
            let (x0, z0) = (gx as usize, gz as usize);
            let (u, w) = (gx - x0 as f32, gz - z0 as f32);
            let a = sketch[z0 * sn + x0] * (1.0 - u) + sketch[z0 * sn + x0 + 1] * u;
            let b = sketch[(z0 + 1) * sn + x0] * (1.0 - u) + sketch[(z0 + 1) * sn + x0 + 1] * u;
            a * (1.0 - w) + b * w
        };
        let coasts: Vec<(u8, f32, f32, f32, f32)> = self
            .sea
            .iter()
            .filter_map(|c| {
                let e = match c.edge.as_str() {
                    "west" => 0,
                    "east" => 1,
                    "north" => 2,
                    "south" => 3,
                    _ => return None,
                };
                Some((e, c.reach, c.bays.unwrap_or(c.reach * 0.5), c.rise.max(4.0), c.skerries.clamp(0.0, 1.0)))
            })
            .collect();
        let k = 1.0 / self.scale.max(0.05);
        let vary = self.vary.clamp(0.0, 1.0);
        let over: Option<Vec<f32>> = match self.mode.as_deref() {
            Some("add") => Some(f.h.clone()),
            None | Some("replace") => None,
            Some(m) => {
                cx.warnings.push(format!("terrain: mode {m}: add or replace"));
                None
            }
        };
        let f = &mut cx.f;
        let mut top = f32::MIN;
        for i in 0..n * n {
            let at = f.centre(i);
            let p = at + noise_at;
            // Two warps: one that moves whole landforms, one that frets
            // their edges.
            let big = Vec2::new(fbm2(p.x * 0.0021, p.y * 0.0021, 3, seed), fbm2(p.x * 0.0021 + 71.0, p.y * 0.0021, 3, seed + 1)) * self.warp;
            let small = Vec2::new(fbm2(p.x * 0.011, p.y * 0.011, 2, seed + 2), fbm2(p.x * 0.011 - 37.0, p.y * 0.011, 2, seed + 3)) * self.warp * 0.25;
            let q = at + big + small;
            let mut h = read(q);
            if let Some(o) = &over {
                h += o[i];
            }
            // How strongly each kind of roughness shows here.
            let slow = |s: u32| (1.0 + vary * 1.6 * fbm2(p.x * 0.0016 + s as f32 * 13.0, p.y * 0.0016, 2, seed + 10 + s)).clamp(0.08, 2.2);
            let w = p + big * 0.6;
            let hv = (fbm2(w.x * 0.005 * k, w.y * 0.005 * k, 5, seed + 4) * 0.5 + 0.5).clamp(0.0, 1.0).powf(1.3);
            h += (hv - 0.4) * self.hills * slow(0);
            if self.ridges > 0.0 {
                let mask = smoothstep(0.35, 0.7, fbm2(p.x * 0.003 * k, p.y * 0.003 * k - 9.0, 2, seed + 6) * 0.5 + 0.5);
                h += ridged2(w.x * 0.007 * k, w.y * 0.007 * k, 4, seed + 5) * self.ridges * mask * slow(1);
            }
            if self.rough > 0.0 {
                let knob = slow(2);
                h += fbm2(w.x * 0.021 * k, w.y * 0.021 * k, 3, seed + 7) * self.rough * knob * knob;
            }
            h += fbm2(p.x * 0.04, p.y * 0.04, 2, seed + 8) * 0.6;
            // The sea, along whichever edges have one.
            let mut shore = f32::MAX;
            let mut isles = 0.0f32;
            for (e, reach, bays, rise, skerries) in &coasts {
                let d = match e {
                    0 => at.x,
                    1 => size - at.x,
                    2 => at.y,
                    _ => size - at.y,
                };
                let inland = d - reach - bays * (fbm2(p.x * 0.0042, p.y * 0.0042, 3, seed + 20 + *e as u32) * 2.4 + fbm2(p.x * 0.015, p.y * 0.015, 3, seed + 30 + *e as u32) * 0.6);
                let s = inland / rise;
                if s < shore {
                    shore = s;
                    // Islets stand in a band just off the shore, where the
                    // fine noise peaks.
                    let band = smoothstep(-2.5, -0.4, s) * smoothstep(0.0, -0.3, s);
                    let peak = smoothstep(0.42 - 0.2 * skerries, 0.6, fbm2(p.x * 0.021, p.y * 0.021, 3, seed + 40));
                    isles = band * peak * skerries;
                }
            }
            if shore < f32::MAX {
                // A shallow shelf still falls away before the world's edge.
                let edge = at.x.min(at.y).min(size - at.x).min(size - at.y);
                let shelf = if self.shelf < 14.0 { 14.0 + (self.shelf - 14.0) * smoothstep(20.0, 140.0, edge) } else { self.shelf };
                let floor = sea - shelf;
                let land = smoothstep(-0.25, 1.0, shore);
                h = floor + (h - floor) * land;
                if isles > 0.0 {
                    h = h.max(sea + isles * (4.0 + 14.0 * isles));
                }
            }
            f.h[i] = h.max(0.5);
            f.moist[i] = fbm2(p.x * 0.004 + 300.0, p.y * 0.004, 3, seed + 9) * 0.5 + 0.5;
            f.cause[i] = cause;
            top = top.max(f.h[i]);
        }
        format!("land about {:.0} m, highest {top:.0} m, {} rises and basins, {} courses, sea on {} edges", self.base, bumps.len(), courses.len(), coasts.len())
    }
}
