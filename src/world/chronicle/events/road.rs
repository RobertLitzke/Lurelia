use crate::world::chronicle::geom::{grade_strip, smoothed_ground, spline, Strip};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::noise::smoothstep;
use glam::Vec2;
use serde::Deserialize;

/// A graded road along a smooth curve through `path`: the ground under it
/// is cut and filled to an even grade (heights averaged over `smooth` m),
/// with `shoulder` material at the edges and the cut feathered back.
#[derive(Deserialize, Debug)]
pub struct Road {
    path: Vec<Pos>,
    width: f32,
    mat: String,
    #[serde(default)]
    shoulder: Option<String>,
    #[serde(default = "default_smooth")]
    smooth: f32,
    /// Stretches held flat at one height, for a bridge deck the road must
    /// meet at both ends: give the bridge's `place` the same `height`.
    #[serde(default)]
    level: Vec<Level>,
}

/// `length` m of road centred on the point of the line nearest `at`, its
/// surface `height` m above the sea, easing back to the road's own grade
/// over `ease` m beyond each end.
#[derive(Deserialize, Debug)]
pub struct Level {
    at: Pos,
    length: f32,
    height: f32,
    #[serde(default = "default_ease")]
    ease: f32,
}

fn default_ease() -> f32 {
    40.0
}

fn default_smooth() -> f32 {
    80.0
}

impl Process for Road {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Road { path, width, mat, shoulder, smooth, level } = self;
        let cause = cx.cause;
        let pts: Vec<Vec2> = path.iter().map(|p| cx.pos(p)).collect();
        let line = spline(&pts, false, 1.0);
        let m = cx.mat(mat);
        let sh = shoulder.as_ref().map(|x| cx.mat(x)).unwrap_or(m);
        let mut base = smoothed_ground(&cx.f, &line, *smooth, false);
        for lv in level {
            let at = cx.pos(&lv.at);
            let y = cx.f.sea + lv.height;
            // The line is a point a metre, so stations from the nearest one
            // count metres along the road.
            let Some(k0) = (0..line.len()).min_by(|&a, &b| line[a].distance_squared(at).total_cmp(&line[b].distance_squared(at))) else {
                continue;
            };
            let half = lv.length * 0.5;
            for (k, h) in base.iter_mut().enumerate() {
                let s = (k as f32 - k0 as f32).abs();
                if s <= half {
                    *h = y;
                } else if s < half + lv.ease {
                    let t = smoothstep(0.0, 1.0, (s - half) / lv.ease);
                    *h = y + (*h - y) * t;
                }
            }
        }
        let off = vec![0.0; line.len()];
        let spans = grade_strip(cx, &line, &base, &off, &[], &Strip { width: *width, mat: m, shoulder: sh, margin: 6.0 });
        for (a, b, y) in &spans {
            cx.pois.push(Poi { name: format!("road gap to bridge, deck {y:.1} m"), pos: (*a + *b) * 0.5, cause });
        }
        format!("road {} m, {width} m wide, {} gaps left for bridges", line.len(), spans.len())
    }
}
