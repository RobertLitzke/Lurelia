//! Materials: what a voxel is made of, its texture recipe, and its colours.

#[allow(unused_imports)]
use super::*;

#[derive(Deserialize, Clone, Debug)]
pub struct Recipe {
    /// Texture pattern verb: noise, speckle, strata, columns, cracks, blotch,
    /// planks, coursed, glass, ripples, grain, rings, thatch, flat.
    pub pattern: String,
    pub base: [f32; 3],
    #[serde(default)]
    pub alt: Option<[f32; 3]>,
    /// Pattern cells across one metre tile.
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_one")]
    pub contrast: f32,
    /// Colours for the calm style; the pattern is softened there automatically.
    #[serde(default)]
    pub calm: Option<CalmColours>,
}

pub(super) fn default_scale() -> f32 {
    4.0
}

/// How wide a patch of one coat is by default, in metres.
pub(super) fn default_blob() -> f32 {
    60.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct CalmColours {
    pub base: [f32; 3],
    #[serde(default)]
    pub alt: Option<[f32; 3]>,
    /// A different pattern verb for the calm style only.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Scales the recipe's contrast in the calm style (crack lines included).
    #[serde(default)]
    pub contrast: Option<f32>,
    /// Pattern cells per metre in the calm style, when it differs.
    #[serde(default)]
    pub scale: Option<f32>,
}

/// How grass grows on a ground material (`grass` in its file). The defaults
/// are what every grass material had before: thin cover in the bare patches,
/// thick in the lush ones, and tufts from about a hand to knee high.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GrassDef {
    /// Share of columns with a tuft where the meadow noise is at its lowest.
    #[serde(default = "grass_sparse")]
    pub sparse: f32,
    /// Share of columns with a tuft where it is at its highest.
    #[serde(default = "grass_dense")]
    pub dense: f32,
    /// Height of the shortest grass, in the old minimum's units: 3.0 makes
    /// the short grass three times as tall. The long grass of a meadow adds
    /// 1.7 on top of it whatever this is.
    #[serde(default = "default_one")]
    pub short: f32,
}

pub(super) fn grass_sparse() -> f32 {
    0.10
}

pub(super) fn grass_dense() -> f32 {
    0.85
}

#[derive(Deserialize, Clone, Debug)]
pub struct MaterialDef {
    /// The id the entry asks for in its file (`"id"`), 17..254, or none.
    /// Each world keeps the ids its materials ask for where it can; entries
    /// without one (and any that clash in a world) take free ids at load.
    #[serde(rename = "id", default)]
    pub file_id: Option<u16>,
    /// The voxel id in the world being built, set by `Library::assign_ids`;
    /// 0 when this world has no room for it. Saves store names, not ids.
    #[serde(skip, default)]
    pub id: u8,
    /// Set on a material generated for a plant's coat (`coat_materials`):
    /// the material it was copied from, and the plant whose coat it is.
    /// Nothing in the library files sets either.
    #[serde(skip, default)]
    pub coat_of: Option<String>,
    #[serde(skip, default)]
    pub coat_plant: Option<String>,
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// Rock the mesher rounds like a tree crown (surface nets): free-standing
    /// natural forms such as hoodoos. Never for ground the land is made of.
    #[serde(default)]
    pub round: bool,
    /// Rounded rock drawn in flat facets like sloped ground, not soft blobs:
    /// the corners sit on the plain voxel surface (no blur) and the light
    /// is softened toward the face as on hillsides. Implies `round`.
    #[serde(default)]
    pub facet: bool,
    /// Faceted only where the surface is stepped: a corner where every face
    /// that meets it runs flat for two voxels (the edge of a box, a wall
    /// corner, the foot of a wall) stays square, and a staggered edge of
    /// one-voxel pieces is bevelled. For built things; implies `facet`. The
    /// arenas set it per world (`round_edges` in world.json).
    #[serde(default)]
    pub bevel_steps: bool,
    /// Only the bright, blue-green specks of its texture give light, not the
    /// rock between them (glow-worm ceilings, the lantern film).
    #[serde(default)]
    pub glow_specks: bool,
    /// Its `glow` is a lamp behind it: dark by day, lit from dusk to dawn.
    #[serde(default)]
    pub night_glow: bool,
    /// A thin stem the mesher draws as an octagon instead of a square
    /// (vines, culms, canes, poles): the edges along the stem are bevelled.
    /// Default: the leaf of a vine and the stem of bamboo, canes or kelp.
    #[serde(default)]
    pub tube: Option<bool>,
    /// Ground that the mesher smooths into slopes.
    #[serde(default)]
    pub smooth: bool,
    /// Ground whose border with other ground is drawn sharp, not blended with
    /// noise (paving, road surfaces).
    #[serde(default)]
    pub crisp: bool,
    /// Thin growth that bodies pass through (hanging vines): drawn like any
    /// voxel, but physics never collides with it.
    #[serde(default)]
    pub passable: bool,
    /// A half-height block, like a slab: it fills the lower half of its
    /// voxel and leaves the upper half open, so a body standing on it
    /// stands an eighth of a metre up. The mesher draws it half height and
    /// physics stands on its top. Ray casts still treat it as a whole voxel.
    #[serde(default)]
    pub slab: bool,
    /// Flashes in the sun texel by texel. Default: the name contains "glass".
    #[serde(default)]
    pub glints: Option<bool>,
    /// Broad colour fields across the land: "meadow" (like grass), "leaves" or
    /// "needles" (calm style only). Default none.
    #[serde(default)]
    pub tint: Option<String>,
    /// The two colours this material drifts between across the land, tens of
    /// metres at a time: a meadow going from sage to dry ochre, a canopy from
    /// cool to warm. Declared as colours and shipped to the shader as the
    /// multiply that takes the material's own texture to each end, so a
    /// material need not match its drift to its pattern by hand.
    ///
    /// Without it a material takes the ramp its `tint` class gives it, which
    /// is the same ramp for every meadow in every world.
    #[serde(default)]
    pub drift: Option<Drift>,
    /// How thick and how long the grass tufts on this ground grow
    /// (mesh/sprites.rs). Without it a material whose name holds GRASS takes
    /// the defaults below: a thin steppe and a hay meadow grew alike.
    #[serde(default)]
    pub grass: Option<GrassDef>,
    /// Leaves or needles: the bike brushes through and a rope won't hold.
    /// Default: a plant material whose name says leaf, needle, frond,
    /// canopy, bush or shrub.
    #[serde(default)]
    pub foliage: Option<bool>,
    /// Water drawn as solid voxels (rivers): walking creatures keep out.
    #[serde(default)]
    pub water: bool,
    /// Flat ground that a shower floods from edge to edge instead of in
    /// puddles: a salt pan or a playa, where an inch of water turns the
    /// whole flat into a mirror of the sky (scene.wgsl `damp_ground`).
    #[serde(default)]
    pub floods: bool,
    /// Its detail sprites grow under the sea as well as over it: eelgrass and
    /// the other drowned meadows, which are otherwise hidden at the
    /// waterline like land grass.
    #[serde(default)]
    pub underwater: bool,
    /// Its detail sprites lie flat on the sea's surface above where they
    /// grow, whatever the depth: lily pads and the other floating leaves.
    #[serde(default)]
    pub floating: bool,
    /// Gives its own light (bioluminescent moss, cave ceilings): drawn bright
    /// in shade and at night, strength 0 (none) to about 1.5.
    #[serde(default)]
    pub glow: f32,
    /// Damage a second to anything standing in it: molten slag, lava, acid.
    /// A property of the stuff rather than a hazard listed per world, so a
    /// pool of it bites wherever it is put -- the arena's own slag pit and
    /// an imported map's lava alike. 0 to 63; over about 20 is instant
    /// death rather than a thing to run across.
    #[serde(default)]
    pub hurt: f32,
    pub texture: Recipe,
    #[serde(default)]
    pub top: Option<Recipe>,
    #[serde(default)]
    pub bottom: Option<Recipe>,
}

/// The ends of a material's broad colour field (`drift`), in sRGB bytes: one
/// pair for both styles, or a pair for each. A style with no pair of its own
/// falls back to the ramp its `tint` class gives it.
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Drift {
    Both([[f32; 3]; 2]),
    PerStyle {
        #[serde(default)]
        calm: Option<[[f32; 3]; 2]>,
        #[serde(default)]
        classic: Option<[[f32; 3]; 2]>,
    },
}

impl Drift {
    /// The pair for the calm style, and the pair for the classic one.
    pub fn pairs(&self) -> (Option<[[f32; 3]; 2]>, Option<[[f32; 3]; 2]>) {
        match self {
            Drift::Both(p) => (Some(*p), Some(*p)),
            Drift::PerStyle { calm, classic } => (*calm, *classic),
        }
    }
}

/// The colour of water: sRGB bytes where it is shallow (over sand, a metre
/// or two) and where it is deep.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct WaterColour {
    pub shallow: [f32; 3],
    pub deep: [f32; 3],
    /// Still, murky water (a swamp, a bayou): no foam along the shore, less
    /// of the sky in it, and the bed lost a hand down. World water only.
    #[serde(default)]
    pub still: bool,
}

impl WaterColour {
    /// Linear RGB, w = 1 (set): (shallow, deep); deep's w is 2 for still water.
    pub fn linear(&self) -> ([f32; 4], [f32; 4]) {
        let f = |c: [f32; 3]| [(c[0] / 255.0).powf(2.2), (c[1] / 255.0).powf(2.2), (c[2] / 255.0).powf(2.2), 1.0];
        let mut d = f(self.deep);
        if self.still {
            d[3] = 2.0;
        }
        (f(self.shallow), d)
    }
}
