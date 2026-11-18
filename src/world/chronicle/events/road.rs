use crate::world::chronicle::geom::{grade_strip, smoothed_ground, spline, Strip};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
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
}

fn default_smooth() -> f32 {
    80.0
}

impl Process for Road {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Road { path, width, mat, shoulder, smooth } = self;
        let cause = cx.cause;
        let pts: Vec<Vec2> = path.iter().map(|p| cx.pos(p)).collect();
        let line = spline(&pts, false, 1.0);
        let m = cx.mat(mat);
        let sh = shoulder.as_ref().map(|x| cx.mat(x)).unwrap_or(m);
        let base = smoothed_ground(&cx.f, &line, *smooth, false);
        let off = vec![0.0; line.len()];
        let spans = grade_strip(cx, &line, &base, &off, &[], &Strip { width: *width, mat: m, shoulder: sh, margin: 6.0 });
        for (a, b, y) in &spans {
            cx.pois.push(Poi { name: format!("road gap to bridge, deck {y:.1} m"), pos: (*a + *b) * 0.5, cause });
        }
        format!("road {} m, {width} m wide, {} gaps left for bridges", line.len(), spans.len())
    }
}
