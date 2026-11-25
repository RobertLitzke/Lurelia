//! Habitats (`library/habitats/`): the kinds of ground a biome is an
//! instance of, about 37 of them (marsh, heath, conifer forest, coral
//! reef...). A world biome names its habitat (`"habitat": "marsh"`); the
//! habitat says, in the site fields' own terms (`world/site.rs`), what the
//! ground there is like on the whole. Nothing a world builds reads it unless
//! the world asks for fill (`WorldDef::fill`): then plants from the world's
//! own pool that suit their biome's habitat join its lists. Plan in
//! `notes/LIBRARY_HABITATS_PLAN.md`.

use super::*;
use crate::world::site::SiteChan;

/// What a habitat can offer animals that feed there.
pub const FORAGE: [&str; 5] = ["graze", "browse", "nectar", "seed", "fruit"];

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct HabitatDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// Ground water, 0 dry to 1 standing water: the habitat's baseline for
    /// the `wet` site field.
    #[serde(default)]
    pub wet: Option<f32>,
    /// Acid (0) to base-rich (1), as a biome's `soil`.
    #[serde(default)]
    pub soil: Option<f32>,
    /// Share of the ground under a canopy, 0 to 1.
    #[serde(default)]
    pub shade: Option<f32>,
    /// How much of the ground is stone, 0 to 1.
    #[serde(default)]
    pub rock: Option<f32>,
    /// How much people, fire or fresh ground have opened it, 0 to 1.
    #[serde(default)]
    pub disturbed: Option<f32>,
    /// How open the ground is to the sky and to running animals, 0 (closed
    /// wood) to 1 (open plain).
    #[serde(default)]
    pub open: Option<f32>,
    /// What it offers animals that feed there (`FORAGE`).
    #[serde(default)]
    #[allow(dead_code)] // read by animal placement later (plan H6)
    pub forage: Vec<String>,
}

impl HabitatDef {
    /// The habitat's baseline for a site field, where it gives one.
    pub fn baseline(&self, c: SiteChan) -> Option<f32> {
        match c {
            SiteChan::Wet => self.wet,
            SiteChan::Soil => self.soil,
            SiteChan::Shade => self.shade,
            SiteChan::Rock => self.rock,
            SiteChan::Disturbed => self.disturbed,
            _ => None,
        }
    }

    /// How well a plant with these site preferences suits the habitat on
    /// the whole: the product of its fit on each field both give (shade
    /// included, since a habitat's shade is the canopy it keeps), 1 where
    /// they share none.
    pub fn suits(&self, prefs: &SitePrefs) -> f32 {
        let mut s = 1.0;
        for (c, r) in [
            (SiteChan::Wet, prefs.wet),
            (SiteChan::Soil, prefs.soil),
            (SiteChan::Shade, prefs.shade),
            (SiteChan::Rock, prefs.rock),
            (SiteChan::Disturbed, prefs.disturbed),
        ] {
            if let (Some(r), Some(v)) = (r, self.baseline(c)) {
                s *= crate::world::site::fit(r, v);
            }
        }
        s
    }

    /// Every value it gives, by name, for checking.
    pub fn values(&self) -> [(&'static str, Option<f32>); 6] {
        [("wet", self.wet), ("soil", self.soil), ("shade", self.shade), ("rock", self.rock), ("disturbed", self.disturbed), ("open", self.open)]
    }
}
