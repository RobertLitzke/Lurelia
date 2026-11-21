//! Shape programs: the ops that build structures and creature models out of voxels.

#[allow(unused_imports)]
use super::*;

/// One step of a shape program, in voxel units with y up. Later steps override
/// earlier ones; `mat: "air"` carves.
///
/// An op is internally tagged, which `parse_known` cannot see inside, so a key
/// an op does not have is refused here instead: the entry fails to load,
/// naming the key, rather than drawing without it.
///
/// These doc comments are also the descriptions in
/// `library/schema/structure.schema.json`; see `structure_schema_is_current`.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// Solid box, `from` inclusive, `to` exclusive.
    Box {
        from: [i32; 3],
        to: [i32; 3],
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// Four walls of a box (no floor or ceiling), `thick` voxels.
    Walls {
        from: [i32; 3],
        to: [i32; 3],
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default = "one_i")]
        thick: i32,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// Pitched roof over the rectangle `from`..`to` (y of `from` is the eaves),
    /// ridge along `axis` ("x" or "z"), stepping in one voxel per `rise` layers.
    Gable {
        from: [i32; 3],
        to: [i32; 3],
        #[schemars(with = "Axis")]
        axis: String,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default = "one_i")]
        rise: i32,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// Solid ellipsoid: every voxel whose centre (+0.5) lies within the
    /// semi-axes `radius` (x, y, z) of `centre`. Boulders, mounds, cairn stones.
    Ellipsoid {
        centre: [f32; 3],
        radius: [f32; 3],
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// Upright cylinder: `base` is the centre of its bottom face, `height`
    /// voxels tall. One thinner than 0.75 always fills the voxel its axis runs
    /// through, so thin posts never vanish.
    Cylinder {
        base: [f32; 3],
        radius: f32,
        height: f32,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// Horizontal cylinder along "x" or "z" (logs, wheels' axles), turned
    /// `yaw` degrees about its `base` (from +x toward +z), so a log can lie
    /// at any bearing. `base` is the centre of one end; the log runs `length`
    /// voxels from it along `axis`.
    Log {
        base: [f32; 3],
        radius: f32,
        length: f32,
        #[schemars(with = "Axis")]
        axis: String,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
        #[serde(default)]
        yaw: f32,
    },
    /// Disc standing upright, its axis along "x" or "z" (wheels), turned
    /// `yaw` degrees about its centre like a log.
    Disc {
        centre: [f32; 3],
        radius: f32,
        thick: f32,
        #[schemars(with = "Axis")]
        axis: String,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
        #[serde(default)]
        yaw: f32,
    },
    /// One voxel.
    Set {
        at: [i32; 3],
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// A rod, rope or spar through `points` (two or more, voxel units, the
    /// centre of a voxel is +0.5), `radius` voxels thick: rigging, stays,
    /// spokes, diagonal legs and braces. Drawn as one thin line of voxels,
    /// and where it leans the mesher draws it as a smooth tube along its own
    /// line (a thread, mesh/lean.rs) instead of a staircase. A radius under
    /// about 0.6 is still one voxel of line, drawn as a tube that thin.
    Segment {
        points: Vec<[f32; 3]>,
        radius: f32,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
        part: Option<String>,
    },
    /// A paraboloid shell: a radio dish or a reflector. `vertex` is the
    /// bottom of the bowl, `radius` the rim's radius and `depth` how far the
    /// rim stands above the vertex along the dish's axis, `thick` the
    /// shell's thickness (voxels). The axis points straight up, then leans
    /// `tilt` degrees toward `yaw` (degrees from +x toward +z), so a dish
    /// can face the zenith, the horizon or the ground.
    Dish {
        vertex: [f32; 3],
        radius: f32,
        depth: f32,
        #[serde(default = "dish_thick")]
        thick: f32,
        #[serde(default)]
        tilt: f32,
        #[serde(default)]
        yaw: f32,
        #[schemars(with = "MatRef")]
        mat: String,
        #[serde(default)]
        #[schemars(with = "Option<PartRef>")]
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
    /// Runs `ops` `count` times, run k (from 0) shifted by k * `step`
    /// voxels: rows of windows, fence posts, stair treads.
    Repeat {
        count: i32,
        step: [i32; 3],
        ops: Vec<Op>,
    },
}

// Schema-only types: what some of the strings in a shape program mean. The
// loader reads them as plain strings; these give them a description, and an
// enum where the engine accepts only a few values, in the generated schema.

/// What an op draws with: a slot named in the entry's `slots`, a library
/// material by name, `"air"` (empties the structure's own voxels and leaves
/// the terrain there) or `"carve"` (removes the terrain too: alcoves, adits,
/// arches worn through rock). In a creature or collectible model, a colour
/// named in its palette.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct MatRef(String);

/// What the voxels are for. In a structure, how they decay with age: `roof`,
/// `cloth` and `glass` fail in whole sections; `timber`, `frame`, `wall`,
/// `stone` and `metal` crumble from the top of each column; `floor` mostly
/// stays. Any other name, or none, never decays. `stone` and `wall` grow moss.
/// In a creature, the body part the voxels move with (`head`, `tail`, ...).
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct PartRef(String);

/// A horizontal axis.
#[derive(schemars::JsonSchema)]
#[schemars(rename_all = "lowercase")]
#[allow(dead_code)]
enum Axis {
    X,
    Z,
}

/// Where y = 0 goes when the structure is placed, if not on the ground.
#[derive(schemars::JsonSchema)]
#[schemars(rename_all = "lowercase")]
#[allow(dead_code)]
enum Base {
    /// One metre under sea level: piers, landings, sea stacks. `spires`
    /// stands these only in water 1.5 to 16 m deep.
    Sea,
    /// The anchor voxel sits at the highest ground under the footprint and
    /// nothing is levelled: bridges spanning a gap, things hung on a cliff.
    Rim,
    /// Sunk half a metre into the lowest sea bed under the footprint: wrecks.
    Bed,
}

pub(super) fn one_i() -> i32 {
    1
}

fn dish_thick() -> f32 {
    1.5
}

/// A structure: a fixed block of 0.25 m voxels drawn by a shape program and
/// placed on a quarter turn by chronicle events (`place`, `settlement`,
/// `spires`, ...) or as a biome feature. One file per entry in
/// `library/structures/`, named `<name>.json`.
///
/// Keys starting with `_` are ignored, as notes to whoever edits the file.
/// `_generated_by` names the script that writes the file, for an entry
/// computed by code (an arena, an imported map): edit the script, not the
/// file. Without it the file is the source.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
#[schemars(extend("additionalProperties" = false, "patternProperties" = {"^_": {}}))]
pub struct StructureDef {
    /// What chronicle events and biomes call it.
    pub name: String,
    /// 1 for a new entry. An entry another world uses is never changed; a
    /// changed copy gets a new name (`name_v2`).
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    /// The world that added it.
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// Bounding size in voxels (x, y, z); y = 0 is ground level. Ops outside
    /// it are clipped. The front faces local -z.
    pub size: [i32; 3],
    /// The local voxel that sits on the placement point.
    #[serde(default)]
    pub anchor: [i32; 3],
    /// Up to this many voxels under y = 0 are filled, where the ground has
    /// fallen away, with the material of the bottom row above them, so the
    /// structure stands on footings on uneven ground.
    #[serde(default)]
    pub foundation: i32,
    /// Slot name -> material name. Ops name a slot or a material; a world's
    /// `place` event can swap a slot's material.
    #[serde(default)]
    pub slots: HashMap<String, String>,
    /// The shape program, run in order: later ops overwrite earlier voxels.
    pub ops: Vec<Op>,
    /// Free labels for people and tools (`settler`, `house`). The engine reads
    /// two: `ruin` lets vines cover it fully, and `anomaly` lists it in the
    /// gazetteer though it does not decay.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether it weathers with the age it is placed at (parts decay, loose
    /// pieces fall, moss grows). False for natural features: rock, logs.
    #[serde(default = "yes")]
    pub decays: bool,
    /// True: the ground under it is levelled, feathered out over 3 m round
    /// the footprint. False: it settles a quarter metre into the lowest
    /// ground under it and nothing grows through it (boulders, logs).
    #[serde(default = "yes")]
    pub level: bool,
    /// Where it stands, if not on the ground; see `Base`.
    #[serde(default)]
    #[schemars(with = "Option<Base>")]
    pub base: Option<String>,
    /// How readily a biome's vines (hosts "wall") hang on its outer walls, 0
    /// to 1, times the biome entry's density. Unset: 0 for anything that
    /// doesn't decay (rock, glass, bridges) unless tagged "ruin", 1 for a
    /// "ruin", otherwise 0.25 for a kept-up building rising toward 1 as it
    /// decays (flora::wall_vine_share).
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub vines: Option<f32>,
    /// One sentence on what it is and how to place it: which way it faces,
    /// what it carves.
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

pub(super) fn yes() -> bool {
    true
}
