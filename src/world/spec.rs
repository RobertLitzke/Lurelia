//! The parameter block a world is generated from. A saved world is this file plus
//! the player's edits; regenerating terrain from it is cheaper than loading voxels.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldSpec {
    pub seed: u32,
    /// Side of the square world, metres.
    pub size_m: f32,
    /// Height of the voxel volume, metres.
    pub height_m: f32,
    pub sea_level_m: f32,
    /// Peak contribution of rolling hills above the coast, metres.
    pub hill_height_m: f32,
    /// Peak contribution of ridged spines, metres.
    pub ridge_height_m: f32,
    /// 0 = no terraced cliffs or overhangs, 1 = the default amount.
    pub cliff_amount: f32,
    /// Height of one terrace step on inland cliffs, metres.
    pub terrace_step_m: f32,
    /// Multiplier on tree placement probability.
    pub tree_density: f32,
    /// Height above which grass gives way to snow, metres.
    pub snow_line_m: f32,
    /// Centre of the island as a fraction of the world size.
    pub island_centre: [f32; 2],
    /// How much farther the coast is on the west than the east (>1 pushes the sea east).
    pub east_squash: f32,
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            seed: 1337,
            size_m: 512.0,
            height_m: 128.0,
            sea_level_m: 24.0,
            hill_height_m: 40.0,
            ridge_height_m: 34.0,
            cliff_amount: 1.0,
            terrace_step_m: 4.0,
            tree_density: 1.0,
            snow_line_m: 72.0,
            island_centre: [0.47, 0.5],
            east_squash: 1.3,
        }
    }
}

impl WorldSpec {
    pub fn load_or_create(path: &Path) -> std::io::Result<Self> {
        Self::load_or(path, Self::default())
    }

    /// The random island's spec: the saved one, or the defaults with a new
    /// seed from the clock, written so the island stays the same next time.
    /// Delete the file to roll another island.
    pub fn load_or_roll(path: &Path) -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let seed = (nanos ^ (nanos >> 32)) as u32;
        Self::load_or(path, Self { seed, ..Self::default() })
    }

    fn load_or(path: &Path, fresh: Self) -> std::io::Result<Self> {
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            serde_json::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        } else {
            let spec = fresh;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&spec).unwrap())?;
            Ok(spec)
        }
    }
}
