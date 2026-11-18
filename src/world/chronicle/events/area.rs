use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::places::Area;
use glam::Vec2;
use serde::Deserialize;

/// Names a stretch of ground. Walk far enough into it and the HUD names it in
/// the corner; the gazetteer lists them all. Either a ragged circle round a
/// point (`at` and `radius`) or an outline (`poly`). Areas may overlap and
/// nest: the smallest one you are properly inside is the one you are in.
#[derive(Deserialize, Debug)]
pub struct AreaEvent {
    name: String,
    /// What kind of place it is, for an author reading the gazetteer:
    /// "village", "wood", "coast", "street". Shown nowhere in the game.
    #[serde(default)]
    kind: String,
    #[serde(default)]
    at: Option<Pos>,
    #[serde(default)]
    radius: f32,
    /// How ragged the circle's edge is, 0 to 1.
    #[serde(default = "default_wobble")]
    wobble: f32,
    /// An outline in metres instead of a circle.
    #[serde(default)]
    poly: Vec<[f32; 2]>,
    /// How far inside the edge you must be before the HUD names it. The
    /// default is a tenth of the radius, between 4 and 20 m.
    #[serde(default)]
    inside: Option<f32>,
}

fn default_wobble() -> f32 {
    0.12
}

impl Process for AreaEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let AreaEvent { name, kind, at, radius, wobble, poly, inside } = self;
        let kind = if kind.is_empty() { "place".to_string() } else { kind.clone() };
        let mut area = if !poly.is_empty() {
            if poly.len() < 3 {
                cx.warnings.push(format!("area {name}: an outline needs at least three points"));
                return "skipped".into();
            }
            Area::new(name.clone(), kind, poly.iter().map(|p| Vec2::new(p[0], p[1])).collect(), 0.0)
        } else {
            let Some(at) = at else {
                cx.warnings.push(format!("area {name}: needs either at and radius, or poly"));
                return "skipped".into();
            };
            if *radius <= 0.0 {
                cx.warnings.push(format!("area {name}: radius must be more than 0"));
                return "skipped".into();
            }
            let c = cx.pos(at);
            Area::disc(name.clone(), kind, c, *radius, *wobble, cx.seed)
        };
        // How far in before it is named: given, else a tenth of the way across.
        let across = (area.hi - area.lo).min_element();
        area.inside_m = inside.unwrap_or((across * 0.1).clamp(4.0, 20.0));
        let (c, m2) = ((area.lo + area.hi) * 0.5, area.area_m2);
        cx.areas.push(area);
        format!("area {name} round {:.0},{:.0}, {:.1} ha", c.x, c.y, m2 / 10000.0)
    }
}
