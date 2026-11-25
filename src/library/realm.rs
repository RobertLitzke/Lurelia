//! Realms (`library/realms/`): which world a place's life comes from. `real`
//! is the default; `ice_age`, `dinosaur`, `fae` and `bloom` (what the
//! skystone changed) are the others. A plant or creature says which realms
//! it belongs to (`realms`), a world gives its mix (`WorldDef::realms`), and
//! a biome may set its own (`BiomeDef::realms`). Today only fill reads them:
//! it draws a species into a biome only where they share a realm. Later a
//! realm also carries its look (ground swaps, mist, glow) and its iconic cast
//! by habitat. Plan in `notes/LIBRARY_HABITATS_PLAN.md`.

use super::*;

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct RealmDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// What each habitat holds in this realm, by habitat name: a dinosaur
    /// marsh has giant horsetail and hadrosaurs. Drawn in by fill
    /// (`WorldDef::fill`) wherever the realm comes through, so a world with
    /// dinosaur ground gets the cast without a biome listing it.
    #[serde(default)]
    pub habitats: std::collections::BTreeMap<String, RealmCast>,
    /// Its look, in a world that asks for realm looks
    /// (`WorldDef::realm_looks`): ground materials it turns into others, by
    /// name (`{"meadow_grass": "pale_grass"}`), where its share of the
    /// ground passes a noisy threshold, so changed ground comes in blobs.
    #[serde(default)]
    pub ground: std::collections::BTreeMap<String, String>,
    /// Ground mist where it is strong, 0 to 1, as a biome's `mist`.
    #[serde(default)]
    pub mist: f32,
}

/// A realm's own species for one habitat (`RealmDef::habitats`).
#[derive(Deserialize, Clone, Debug, Default, schemars::JsonSchema)]
pub struct RealmCast {
    #[serde(default)]
    pub plants: Vec<PlantUse>,
    /// Read by animal fill later (habitats plan H6); checked now.
    #[serde(default)]
    pub creatures: Vec<CreatureUse>,
}
