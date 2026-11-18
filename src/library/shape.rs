//! Shape programs: the ops that build structures and creature models out of voxels.

#[allow(unused_imports)]
use super::*;

/// One step of a shape program, in voxel units with y up. Later steps override
/// earlier ones; `mat: "air"` carves.
///
/// An op is internally tagged, which `parse_known` cannot see inside, so a key
/// an op does not have is refused here instead: the entry fails to load,
/// naming the key, rather than drawing without it.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// Solid box, `from` inclusive, `to` exclusive.
    Box {
        from: [i32; 3],
        to: [i32; 3],
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    /// Four walls of a box (no floor or ceiling), `thick` voxels.
    Walls {
        from: [i32; 3],
        to: [i32; 3],
        mat: String,
        #[serde(default = "one_i")]
        thick: i32,
        #[serde(default)]
        part: Option<String>,
    },
    /// Pitched roof over the rectangle `from`..`to` (y of `from` is the eaves),
    /// ridge along `axis` ("x" or "z"), stepping in one voxel per `rise` layers.
    Gable {
        from: [i32; 3],
        to: [i32; 3],
        axis: String,
        mat: String,
        #[serde(default = "one_i")]
        rise: i32,
        #[serde(default)]
        part: Option<String>,
    },
    Ellipsoid {
        centre: [f32; 3],
        radius: [f32; 3],
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    /// Upright cylinder.
    Cylinder {
        base: [f32; 3],
        radius: f32,
        height: f32,
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    /// Horizontal cylinder along "x" or "z" (logs, wheels' axles).
    Log {
        base: [f32; 3],
        radius: f32,
        length: f32,
        axis: String,
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    /// Disc standing upright, its axis along "x" or "z" (wheels).
    Disc {
        centre: [f32; 3],
        radius: f32,
        thick: f32,
        axis: String,
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    Set {
        at: [i32; 3],
        mat: String,
        #[serde(default)]
        part: Option<String>,
    },
    /// Recolour solid voxels below a height (creature bellies, socks).
    RecolourBelow {
        y: i32,
        from: String,
        to: String,
    },
    /// Recolour solid voxels of colour `from` inside a box (`at` inclusive,
    /// `until` exclusive): stripes, patches and bands that follow the body.
    Recolour {
        at: [i32; 3],
        until: [i32; 3],
        from: String,
        to: String,
    },
    Repeat {
        count: i32,
        step: [i32; 3],
        ops: Vec<Op>,
    },
}

pub(super) fn one_i() -> i32 {
    1
}

#[derive(Deserialize, Clone, Debug)]
pub struct StructureDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// Bounding size in voxels (x, y, z); y = 0 is ground level.
    pub size: [i32; 3],
    /// The local voxel that sits on the placement point.
    #[serde(default)]
    pub anchor: [i32; 3],
    /// Voxels below ground filled with the foundation slot under the footprint,
    /// so the structure sits on uneven ground.
    #[serde(default)]
    pub foundation: i32,
    /// Slot name -> material name. Ops may name a slot or a material.
    #[serde(default)]
    pub slots: HashMap<String, String>,
    pub ops: Vec<Op>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Natural features (boulders, logs) don't decay and aren't levelled.
    #[serde(default = "yes")]
    pub decays: bool,
    #[serde(default = "yes")]
    pub level: bool,
    /// "sea": the structure's ground level is the sea surface (landings, piers).
    #[serde(default)]
    pub base: Option<String>,
    /// How readily a biome's vines (hosts "wall") hang on its outer walls, 0
    /// to 1, times the biome entry's density. Unset: 0 for anything that
    /// doesn't decay (rock, glass, bridges) unless tagged "ruin", 1 for a
    /// "ruin", otherwise 0.25 for a kept-up building rising toward 1 as it
    /// decays (flora::wall_vine_share).
    #[serde(default)]
    pub vines: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

pub(super) fn yes() -> bool {
    true
}
