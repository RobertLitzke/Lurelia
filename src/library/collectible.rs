//! Collectibles: the things a world scatters to be found.

#[allow(unused_imports)]
use super::*;

/// A small thing on (or in) the ground the player can pick up.
#[derive(Deserialize, Clone, Debug)]
pub struct CollectibleDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// What the HUD calls it when found.
    pub display: String,
    pub voxel: f32,
    pub colors: HashMap<String, Vec<f32>>,
    pub ops: Vec<Op>,
    /// Turns slowly in place (for things meant to catch the eye).
    #[serde(default)]
    pub spin: bool,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CollectibleUse {
    pub item: String,
    /// Items per square metre where the placement allows them fully.
    pub density: f32,
    #[serde(default)]
    pub placement: Placement,
}
