use crate::world::chronicle::geom::{densify, spline};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::route;
use glam::Vec2;
use serde::Deserialize;

/// One line through the world built to a kit: a fence along a field edge, a
/// guard rail, a boardwalk over a marsh, a skywalk between two towers. The
/// kit (`library/kits/`) decides everything about the cross-section; this
/// event only says where the line goes.
#[derive(Deserialize, Debug)]
pub struct Ribbon {
    path: Vec<Pos>,
    kit: String,
    /// Join the last point back to the first (a yard, a pasture, a ring road).
    #[serde(default)]
    closed: bool,
    /// Round the corners through a curve instead of turning sharply.
    #[serde(default = "yes")]
    curve: bool,
    /// How far the line wanders off the points, metres (a sheep track).
    #[serde(default)]
    meander: f32,
    /// Metres of ground the grade is averaged over; a lane follows the land
    /// more closely than a highway does.
    #[serde(default = "default_smooth")]
    smooth: f32,
    /// Hold the line at this height above sea level instead of letting it
    /// follow the land: a bored passage, a walkway strung between towers.
    #[serde(default)]
    height: Option<f32>,
    #[serde(default)]
    poi: Option<String>,
}

fn yes() -> bool {
    true
}
fn default_smooth() -> f32 {
    40.0
}

impl Process for Ribbon {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        if pts.len() < 2 {
            return "skipped: a ribbon needs two points".into();
        }
        let Some(k) = route::kit_index(cx, &self.kit) else {
            return format!("skipped: unknown kit {}", self.kit);
        };
        let mut line = if self.curve { spline(&pts, self.closed, 1.0) } else { pts.clone() };
        if self.closed && !self.curve {
            line.push(pts[0]);
        }
        if self.meander > 0.0 {
            line = densify(&line, self.meander, cx.seed);
        }
        let laid = route::lay_at(cx, &line, k, self.smooth, self.height);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi {
                name: name.clone(),
                pos: line[line.len() / 2],
                cause: cx.cause,
            });
        }
        format!(
            "{} {:.0} m in {} segments{}{}",
            self.kit,
            laid.length,
            laid.segs,
            if laid.spans.is_empty() { String::new() } else { format!(", {} spans", laid.spans.len()) },
            if laid.piers == 0 { String::new() } else { format!(", {} piers", laid.piers) },
        )
    }
}
