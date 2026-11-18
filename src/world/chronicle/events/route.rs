use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::route::{self, cost};
use serde::Deserialize;

/// A line that finds its own way from one place to another: the search pays
/// for climbing, for open water and for ground another event has taken, so
/// the road takes the pass and the footpath takes the dry side. The kit says
/// what gets built along it, and what to span a gap with.
#[derive(Deserialize, Debug)]
pub struct Route {
    from: Pos,
    to: Pos,
    kit: String,
    #[serde(default)]
    terms: route::Terms,
    #[serde(default = "default_smooth")]
    smooth: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn default_smooth() -> f32 {
    60.0
}

impl Process for Route {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (a, b) = (cx.pos(&self.from), cx.pos(&self.to));
        let Some(k) = route::kit_index(cx, &self.kit) else {
            return format!("skipped: unknown kit {}", self.kit);
        };
        let kit = cx.state.get::<route::ribbon::Ribbons>().kits[k].clone();
        let terms = self.terms.over(&kit);
        let grid = cost::Grid::new(&cx.f, terms.step);
        let Some(line) = route::find_line(&grid, a, b, &terms) else {
            cx.warnings.push(format!("route {}: no way from {:.0},{:.0} to {:.0},{:.0}", self.kit, a.x, a.y, b.x, b.y));
            return "skipped: no way through".into();
        };
        let laid = route::lay(cx, &line, k, self.smooth);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi {
                name: name.clone(),
                pos: line[line.len() / 2],
                cause: cx.cause,
            });
        }
        format!(
            "{} {:.0} m found across {:.0} m of country{}{}",
            self.kit,
            laid.length,
            a.distance(b),
            if laid.spans.is_empty() {
                String::new()
            } else {
                format!(
                    ", spanning {} gaps at {}",
                    laid.spans.len(),
                    laid.spans.iter().map(|(a, b, y)| format!("{:.0},{:.0} ({:.0} m long, deck {y:.0} m)", (a.x + b.x) * 0.5, (a.y + b.y) * 0.5, a.distance(*b))).collect::<Vec<_>>().join("; ")
                )
            },
            if laid.piers == 0 { String::new() } else { format!(" on {} piers", laid.piers) },
        )
    }
}
