//! Biomes: what grows, stands and lives where, and how it is placed.

#[allow(unused_imports)]
use super::*;

/// A placement rule: which verb, and its parameters.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct Placement {
    pub verb: crate::world::flora::PlaceVerb,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    #[serde(default)]
    pub field: Option<String>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            verb: crate::world::flora::PlaceVerb::Scatter,
            params: HashMap::new(),
            field: None,
        }
    }
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct PlantUse {
    pub plant: String,
    /// Plants per square metre where the placement allows them fully.
    pub density: f32,
    #[serde(default)]
    pub placement: Placement,
    /// Which of the plant's coats grow here (see `MorphPick`).
    #[serde(default)]
    pub morph: Option<MorphPick>,
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct FeatureUse {
    pub structure: String,
    pub density: f32,
    #[serde(default)]
    pub placement: Placement,
}

/// Which coats a biome wants of a creature or plant it lists: one by name
/// ("the pale hare here"), or weights that override the entry's own ("mostly
/// pale, some wild"). Left out, the entry's own weights stand.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
#[serde(untagged)]
pub enum MorphPick {
    One(String),
    Weights(HashMap<String, f32>),
}

impl MorphPick {
    /// The coat names this pick refers to, for checking them against the entry.
    pub fn names(&self) -> Vec<&str> {
        match self {
            MorphPick::One(n) => vec![n.as_str()],
            MorphPick::Weights(w) => w.keys().map(|k| k.as_str()).collect(),
        }
    }
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct CreatureUse {
    pub creature: String,
    /// How many to keep around the player while they are in this biome. A
    /// fraction is an extra one in that share of the country: 2.2 keeps two,
    /// and a third in about one 128 m square in five.
    pub count: f32,
    /// Which of the creature's coats turn up here (see `MorphPick`): the glass
    /// country's elk pale, the faerie wood's herons recoloured.
    #[serde(default)]
    pub morph: Option<MorphPick>,
}

#[derive(Deserialize, Clone, Debug, Default, schemars::JsonSchema)]
pub struct Rules {
    /// Height above sea, metres.
    #[serde(default)]
    pub elev: Option<[f32; 2]>,
    #[serde(default)]
    pub slope: Option<[f32; 2]>,
    #[serde(default)]
    pub moist: Option<[f32; 2]>,
    /// Distance to the coast, metres (negative at sea).
    #[serde(default)]
    pub coast: Option<[f32; 2]>,
    /// Distance to a flowing river channel, metres.
    #[serde(default)]
    pub river: Option<[f32; 2]>,
    /// Years since the last fire.
    #[serde(default)]
    pub burn_age: Option<[f32; 2]>,
    #[serde(default)]
    pub strange: Option<[f32; 2]>,
    /// Inside a named region painted by the chronicle.
    #[serde(default)]
    pub region: Option<String>,
    /// Read the region at the cell itself rather than through the warp that
    /// gives other biomes ragged edges: fields that end at a wall.
    #[serde(default)]
    pub crisp: bool,
    /// true: only under the sea; false: only above it; unset: above it,
    /// unless `amphibious`.
    #[serde(default)]
    pub underwater: Option<bool>,
    /// Land and the sea's shallows alike (a swamp standing in water, a
    /// marsh): with `underwater` unset, cells under the sea match too.
    #[serde(default)]
    pub amphibious: bool,
}

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct BiomeDef {
    pub name: String,
    /// The biome this one is a variant of (src/inherit.rs), already merged
    /// in: the faerie park is the city park with other plants, creatures and
    /// ground. Lists (plants, creatures, features) are replaced whole, not
    /// merged, so a child that changes one gives all of it.
    #[serde(default)]
    #[allow(dead_code)] // read by the catalogue and people
    pub like: Option<String>,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// The habitat this biome is an instance of (`library/habitats/`): what
    /// kind of ground it is, whatever world it is in. Inherited through
    /// `like` unless the child names its own. Read by fill
    /// (`WorldDef::fill`) and nothing else yet.
    #[serde(default)]
    pub habitat: Option<String>,
    /// Realms whose life comes through here on top of the world's mix
    /// (`WorldDef::realms`), with their strength 0 to 1: `{"fae": 0.8}` for
    /// a faerie glade in a real wood. Painted over the biome's cells as realm
    /// overlays (`world/realm_map.rs`), which fill and realm looks read.
    #[serde(default)]
    pub realm: Option<std::collections::BTreeMap<String, f32>>,
    /// Higher wins when several match.
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub rules: Rules,
    pub surface: String,
    pub subsurface: String,
    /// Exposed rock on steep ground; else the world's rock strata.
    #[serde(default)]
    pub rock: Option<String>,
    /// How far this biome's ground reaches across its edges into the
    /// neighbouring biome, metres: the edge breaks into interlocking patches
    /// instead of a line, and the grass growing on it fringes too. 0 keeps a
    /// hard edge, and a biome with 0 is never mixed into its neighbours
    /// either (a shore, a lava field, a paved square).
    #[serde(default = "default_fringe")]
    pub fringe: f32,
    #[serde(default)]
    pub plants: Vec<PlantUse>,
    #[serde(default)]
    pub features: Vec<FeatureUse>,
    #[serde(default)]
    pub creatures: Vec<CreatureUse>,
    /// Creatures for the caves under this biome (a lava tube, a passage):
    /// wanted instead of `creatures` while the player is under a roof, and
    /// out at any hour, since it is always dark down there.
    #[serde(default)]
    pub cave_creatures: Vec<CreatureUse>,
    #[serde(default)]
    pub collectibles: Vec<CollectibleUse>,
    /// Colour on the top-down map.
    #[serde(default)]
    pub map_color: Option<[f32; 3]>,
    /// How misty the biome is, 0 (clear) to 1 (thick banks of low fog); the
    /// renderer's mist map (Generator::mist_grid). Morning and dusk thicken it.
    #[serde(default)]
    pub mist: f32,
    /// The ground's soil, 0 (acid and poor: heath, bog, pine sand) to 1
    /// (base-rich: limestone, rich meadow); 0.5 if left out. Graded across
    /// biome borders (world/site.rs).
    #[serde(default)]
    pub soil: Option<f32>,
    /// Water over this biome in other colours than the world's (cenotes,
    /// a peat lake): at most two such biomes per world are drawn.
    #[serde(default)]
    pub water: Option<WaterColour>,
    /// What the place sounds like, where the defaults (sound/bed.rs) are not
    /// right for it. Left out, the bed is worked out from the ground.
    #[serde(default)]
    pub sound: Option<BiomeSound>,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

/// A biome's `sound` block. Every field is optional; what is left out comes
/// from the ground and from the voxels round the listener (sound/bed.rs).
#[derive(Deserialize, Clone, Debug, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BiomeSound {
    /// How much of the wind reaches the ground here, as a multiple of what
    /// open ground gets: under 1 in a closed wood or a hollow, over it on a
    /// steppe or a down that the wind never leaves.
    #[serde(default)]
    pub wind: Option<f32>,
    /// A multiple on the leaves heard, which otherwise come from the leaf
    /// voxels actually round the listener's head: under 1 for needles,
    /// which hiss rather than clatter; over 1 for big loose leaves.
    #[serde(default)]
    pub leaves: Option<f32>,
    /// How much grass rustles about the feet, 0 to 1. Otherwise taken from
    /// how thickly the surface material grows grass.
    #[serde(default)]
    pub grass: Option<f32>,
    /// What rain lands on: soft, grass, leaves, stone or water. Otherwise
    /// worked out from the surface material.
    #[serde(default)]
    pub ground: Option<crate::sound::Ground>,
    #[serde(default)]
    #[allow(dead_code)] // read by people
    pub notes: String,
}
