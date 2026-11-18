//! Looks: a style's light and grade, and a world's own grade on top (the LOOK page).

#[allow(unused_imports)]
use super::*;

/// The light a style (render::Style) is drawn in: sky, ground bounce, fill,
/// sun and haze. One file per style, named after it (library/looks/calm.json).
#[derive(Deserialize, Clone, Debug)]
pub struct LookDef {
    pub name: String,
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub ground: [f32; 3],
    /// Sky fill for shadowed and indirect light, kept apart from the sky
    /// colours so the sky can be tuned without changing the ground.
    pub ambient: [f32; 3],
    pub sun_tint: [f32; 3],
    pub sun_gain: f32,
    /// Fog density is 1 / this, in metres.
    pub fog_distance: f32,
    /// The gradient strips and the grade's knobs (render/grade.rs). The LOOK
    /// page edits these and EXPORT writes them back here.
    #[serde(default)]
    pub grade: crate::render::Grade,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

/// A world's own grading, over whichever style it is seen in
/// (`worlds/<name>/look.json`). Every field is optional: what the world
/// leaves out it takes from the style, so Redrock can own its distance ramp
/// and the colour it recedes through without owning the whole look, and the
/// two styles keep their own feel underneath. `styles` holds a second patch
/// for one style only, applied after the shared one.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct WorldLook {
    /// What this grading is after, in words. Kept by EXPORT.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zenith: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun_tint: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun_gain: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fog_distance: Option<f32>,
    /// The distance strip, near to far. Given at all, it replaces the
    /// style's: a ramp is a designed sequence, not something to average.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp: Option<Vec<crate::render::grade::Stop>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Vec<crate::render::grade::Stop>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<Vec<crate::render::grade::Stop>>,
    /// Grade knobs by their name in render/grade.rs (`ramp_reach`,
    /// `haze_height`, `saturation`, ...).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub knobs: std::collections::BTreeMap<String, f32>,
    /// A patch for one style by name ("saturated", "classic"), applied over
    /// the shared one. Only the outermost `styles` is read.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub styles: std::collections::BTreeMap<String, WorldLook>,
}

impl WorldLook {
    /// Put this world's grading over a style's light and grade, in place.
    pub fn apply(&self, style: &str, look: &mut crate::render::Look, grade: &mut crate::render::Grade) {
        self.apply_one(look, grade);
        if let Some(p) = self.styles.get(style) {
            p.apply_one(look, grade);
        }
    }

    fn apply_one(&self, look: &mut crate::render::Look, grade: &mut crate::render::Grade) {
        if let Some(c) = self.zenith {
            look.zenith = c;
        }
        if let Some(c) = self.horizon {
            look.horizon = c;
        }
        if let Some(c) = self.ground {
            look.ground = c;
        }
        if let Some(c) = self.ambient {
            look.ambient = c;
        }
        if let Some(c) = self.sun_tint {
            look.sun_tint = glam::Vec3::from(c);
        }
        if let Some(v) = self.sun_gain {
            look.sun_gain = v;
        }
        if let Some(v) = self.fog_distance {
            look.fog_density = 1.0 / v.max(1.0);
        }
        for (stops, row) in [
            (&self.ramp, crate::render::grade::ROW_RAMP),
            (&self.tint, crate::render::grade::ROW_TINT),
            (&self.map, crate::render::grade::ROW_MAP),
        ] {
            if let Some(s) = stops {
                *grade.stops_mut(row) = s.clone();
            }
        }
        for (name, v) in &self.knobs {
            if let Some(slot) = grade.field_mut(name) {
                *slot = *v;
            }
        }
        grade.sort();
    }

    /// What is wrong with this block: knobs nothing is called, styles that
    /// aren't styles, strips with too few stops. Reported when the world is
    /// checked (`--validate`) and on load.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.problems_one("look.json", &mut out);
        for (name, p) in &self.styles {
            if crate::render::Style::parse(name).is_none() {
                out.push(format!("look.json: styles has {name}, which is not a style"));
            }
            p.problems_one(&format!("look.json: styles {name}"), &mut out);
            if !p.styles.is_empty() {
                out.push(format!("look.json: styles {name} has styles of its own, which are not read"));
            }
        }
        out
    }

    fn problems_one(&self, what: &str, out: &mut Vec<String>) {
        for name in self.knobs.keys() {
            if crate::render::Grade::default().field(name).is_none() {
                out.push(format!("{what}: no knob is called {name} (they are {})", crate::render::grade::FIELD_NAMES.join(", ")));
            }
        }
        for (stops, row) in [(&self.ramp, "ramp"), (&self.tint, "tint"), (&self.map, "map")] {
            if let Some(s) = stops {
                if s.len() < 2 {
                    out.push(format!("{what}: the {row} strip needs two stops or more, not {}", s.len()));
                }
                if s.len() > 8 {
                    out.push(format!("{what}: the {row} strip takes eight stops at most, not {}", s.len()));
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == WorldLook::default()
    }
}

/// A world's own look, if it has one (`worlds/<name>/look.json`), and any
/// problems with it. A missing file is no world look and no problem.
pub fn load_world_look(dir: &Path) -> (Option<WorldLook>, Vec<String>) {
    let path = dir.join("look.json");
    if !path.exists() {
        return (None, Vec::new());
    }
    let mut problems = Vec::new();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return (None, vec![format!("look.json: {e}")]),
    };
    let look: WorldLook = match serde_json::from_str::<serde_json::Value>(&text).and_then(|v| parse_known(v, |k| problems.push(format!("look.json: unknown key {k}")))) {
        Ok(l) => l,
        Err(e) => return (None, vec![format!("look.json: {e}")]),
    };
    problems.extend(look.problems());
    (Some(look), problems)
}

/// Every look under `root/library/looks`, with any problems reading them.
pub fn load_looks(root: &Path) -> (Vec<LookDef>, Vec<String>) {
    let mut errors = Vec::new();
    let looks = load_dir::<LookDef>(&root.join("library").join("looks"), &mut errors).into_iter().map(|x| x.1).collect();
    (looks, errors)
}
