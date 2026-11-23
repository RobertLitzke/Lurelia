use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use serde::Deserialize;

/// One structure: a landing, a shrine, an anomaly.
#[derive(Deserialize, Debug)]
pub struct Place {
    structure: String,
    at: Pos,
    #[serde(default)]
    bearing: Option<f32>,
    #[serde(default)]
    face: Option<Pos>,
    #[serde(default)]
    slots: HashMap<String, String>,
    /// Years of neglect for decay.
    #[serde(default)]
    age: f32,
    #[serde(default)]
    poi: Option<String>,
    /// Hang it at this height above sea level (its anchor voxel there),
    /// whatever the ground: a rope bridge strung between two rock walls.
    #[serde(default)]
    height: Option<f32>,
    /// When it stands: hours of the day, only from afar, only close by
    /// (fields::When). The rest of the time its plot is empty.
    #[serde(default)]
    when: Option<crate::world::fields::When>,
}

impl Process for Place {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Place { structure, at, bearing, face, slots, age, poi, height, when } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let p = cx.pos(at);
        let Some(d) = cx.lib.structure(structure) else {
            cx.warnings.push(format!("place: unknown structure {structure}"));
            return "skipped".into();
        };
        let b = match (bearing, face, at) {
            (Some(b), _, _) => *b + 90.0 * cx.turn(at) as f32,
            (None, Some(t), _) => crate::util::bearing(cx.pos(t) - p).to_degrees(),
            // Beside the race track: facing the road.
            (None, None, Pos::Along { along, .. }) => match &cx.track {
                Some(t) => crate::util::bearing(t.at(*along).0 - p).to_degrees(),
                None => 0.0,
            },
            _ => 0.0,
        };
        let k = cx.place(d, p, b, slots, *age, seed);
        if let (Some(k), Some(h)) = (k, height) {
            let lift = cx.lib.structures[d].anchor[1];
            cx.placed[k].origin[1] = ((cx.f.sea + h) * 4.0).round() as i32 - lift;
        }
        if let (Some(k), Some(w)) = (k, when) {
            cx.when.push((k as u32, w.clone()));
        }
        cx.pois.push(Poi { name: poi.clone().unwrap_or(structure.clone()), pos: p, cause });
        format!("{structure} at {:.0},{:.0} facing {b:.0} deg", p.x, p.y)
    }
}
