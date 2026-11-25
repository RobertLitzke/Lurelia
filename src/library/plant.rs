//! Plants: one entry per species, drawn by its growth form (`world::flora::Form`).

#[allow(unused_imports)]
use super::*;

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct PlantDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// The plant this one is a variety of (src/inherit.rs); already merged in.
    #[serde(default)]
    pub like: Option<String>,
    /// Above 0: a rare morph of its parent. Where the parent is placed, this
    /// fraction of them grow as this plant instead.
    #[serde(default)]
    pub rarity: f32,
    /// Growth form verb: broad, birch, conifer, columnar, spiral, shrub, snag,
    /// umbrella, weeping, sapling.
    pub form: crate::world::flora::Form,
    pub height: [f32; 2],
    #[serde(default = "default_trunk")]
    pub trunk_radius: [f32; 2],
    #[serde(default = "default_crown")]
    pub crown_radius: [f32; 2],
    pub bark: String,
    #[serde(default)]
    pub leaf: Option<String>,
    /// A second leaf material some individuals take (cherry trees not yet in
    /// blossom). Not for autumn colour: that is `autumn`, so a forest turns
    /// together instead of mixing seasons tree by tree. The older way of
    /// saying what `morphs` says: it compiles into two coats.
    #[serde(default)]
    pub leaf_alt: Option<(String, f32)>,
    /// The coats this plant comes in (see `MorphDef`): the purple, lilac and
    /// white wisteria. Never inherited through `like`, since a morph's colours
    /// are absolute and a child is a different plant.
    #[serde(default)]
    pub morphs: Vec<MorphDef>,
    /// How wide a patch of one coat is, in metres. The coat is drawn per
    /// patch, not per plant, so a cluster of wisteria is one colour and the
    /// next along is another, and a meadow's grasses lie in blobs rather than
    /// speckling colour over each other.
    #[serde(default = "default_blob")]
    pub morph_blob_m: f32,
    /// Root flare for big old trees: extra trunk radius at the ground in
    /// metres, in lobes like buttress roots, fading over about four times that
    /// height. 0 for none.
    #[serde(default)]
    pub buttress: f32,
    /// Leaf material in a world whose season is autumn (world.json `season`).
    #[serde(default)]
    pub autumn: Option<String>,
    /// Vertical scale of leaf blobs (below 1 flattens crowns).
    #[serde(default = "default_one")]
    pub squash: f32,
    /// Largest lean of the trunk, radians.
    #[serde(default)]
    #[allow(dead_code)] // not drawn yet
    pub lean: f32,
    /// Conifer forms: where the needles start, as a fraction of the height
    /// (default 0.2; ponderosa pines carry theirs high on a bare trunk).
    #[serde(default)]
    pub crown_base: Option<f32>,
    #[serde(default = "default_slope")]
    pub max_slope: f32,
    /// Form "branching": 0 to 1, how far the first limbs reach out sideways
    /// and how many there are; 1 gives a wide old crown.
    #[serde(default)]
    pub spread: f32,
    /// Form "canes": the most a cane leans from upright at its root, radians
    /// (default about 0.54, an ocotillo's fan); 0.1 for cane standing straight.
    #[serde(default)]
    pub cane_lean: Option<f32>,
    /// Form "canes" with canopy "tufts": how far a cane arches over toward
    /// its tip (default about 0.04-0.11, a buckthorn's arch); 0 for canes
    /// that stand straight (sugar cane, hazel).
    #[serde(default)]
    pub cane_bend: Option<f32>,
    /// Form "canes" with canopy "tufts": the chance of a short side spur on
    /// each cane segment (default 0.4, thorny); 0 for clean stems.
    #[serde(default)]
    pub cane_spurs: Option<f32>,
    /// Form "canes" with canopy "tufts": `flower` sits in clusters at the
    /// cane tips (rabbitbrush, roses) instead of packed along the stems.
    #[serde(default)]
    pub flower_tips: bool,
    /// Form "canes" with canopy "tufts": the chance of a leaf tuft at each
    /// joint on the upper half of a cane and at a spur's tip (default 1, a
    /// leafy shrub); low for near-leafless switches (broom, ephedra, reeds).
    #[serde(default)]
    pub cane_leaves: Option<f32>,
    /// Limbs thinner than this radius, metres, are drawn as sticks: one thin
    /// line of voxels meshed as a smooth tube along the limb (a thread),
    /// instead of a lumpy chain two or three voxels thick. For twiggy and
    /// gnarled trees, mangrove prop roots, bare winter crowns. Leave unset
    /// on dense forests: each stick costs about 15-20 quads a metre.
    #[serde(default)]
    pub thin_limbs: Option<f32>,
    /// Keep the sticks (`thin_limbs`) smooth tubes while the tree is in
    /// leaf, not only once it is bare: a planted tree whose clean limbs
    /// show under the crown (street oaks and maples).
    #[serde(default)]
    pub tube_limbs: bool,
    /// A dead tree's shape, on the generic entries placement v2 draws its
    /// woods' dead from (`flora::skeleton`): `standing` (a snag with its top
    /// gone), `broken` (snapped low), `stump` or `log` (fallen, lying along
    /// the ground). Drawn as the skeleton of a living tree, it takes that
    /// tree's size and bark; listed in a biome on its own (a drowned wood),
    /// its own. Under v2 stand structure an entry a biome lists is drawn as
    /// one of that biome's own trees, dead (picked by their density); a biome
    /// with no such trees keeps the generic one (docs/PLANTS.md).
    #[serde(default)]
    pub skeleton: Option<crate::world::flora::Skeleton>,
    /// How much of its radius the trunk loses from its foot to where the
    /// limbs leave, 0 to 0.85 (default `flora::TRUNK_TAPER`, 0.3). More
    /// for a trunk that narrows clearly into its limbs.
    #[serde(default)]
    pub trunk_taper: Option<f32>,
    /// Form "branching": the trunk's clear length up to the first limbs, as
    /// a share of the height (0.15 to 0.85). Unset, it is 0.46 less 0.12 x
    /// `spread`. High for a tree whose crown sits on top of a tall bare
    /// column (baobab); the crown is then flatter, since the limbs climb
    /// through what height is left.
    #[serde(default)]
    pub bole: Option<f32>,
    /// Form "canes" with canopy "tufts": an umbel at each cane tip, a mop of
    /// thin rays this long, metres (papyrus).
    #[serde(default)]
    pub cane_umbel: Option<f32>,
    /// Form "branching": 0 to 1, how far the leaf clusters at the limb tips
    /// are filled out into one domed crown, a shell of foliage over the limbs
    /// that stays hollow underneath (great oaks). 0: separate clumps. Default
    /// `flora::DOME` (the separate clumps read as too fragmented).
    #[serde(default)]
    pub dome: Option<f32>,
    /// Form "branching": a first limb's radius as a share of the trunk's
    /// (default `flora::LIMB`); more for thick-limbed trees like cottonwoods.
    #[serde(default)]
    pub limb: Option<f32>,
    /// Form "branching": the shape a limb takes (`flora::CURVES`) —
    /// "straight", "arch" (bows out and levels off, the default), "sweep"
    /// (out then up, a maple's S), "droop" (over and down) or "kinked".
    #[serde(default)]
    pub curve: Option<crate::world::flora::Curve>,
    /// Form "branching": how the foliage is massed (`flora::CANOPIES`) —
    /// "lobes" (one lobe over each big limb, notches between, the default),
    /// "layers" (flat tiers, an elder), "tufts" (leaves on the twigs only, an
    /// open crown), "cloud" (one soft mass) or "vase" (tall lobes leaning in,
    /// a beech). Form "canes" with "tufts": a thorny shrub of arching canes
    /// with spurs, leaf tufts and berries along the stems (sea buckthorn).
    #[serde(default)]
    pub canopy: Option<crate::world::flora::Canopy>,
    /// Form "branching", canopy "lobes": 0 to 1, how often the gap between
    /// two neighbouring lobes is bridged. 0 leaves every notch open (default
    /// 0.55).
    #[serde(default)]
    pub fill: Option<f32>,
    /// Form "branching": 0 to 1, how much of the trunk carries on up through
    /// the crown as a leader instead of being spent in the lowest fork. 0 is
    /// an oak's open vase, 1 a cottonwood's single spar (default 0.35).
    #[serde(default)]
    pub leader: Option<f32>,
    /// Conifer forms: how fast the cone narrows. Under 1 swells out low and
    /// tapers late (a spruce), over 1 is a straight-sided spire (default 0.9).
    #[serde(default)]
    pub taper: Option<f32>,
    /// Conifer forms: how the whorl branches sit (`flora::SWEEPS`) — "level"
    /// (a fir, the default), "droop" (a spruce), "up" (a young pine) or
    /// "ragged" (all three down one tree, a wind-beaten hemlock).
    #[serde(default)]
    pub sweep: Option<crate::world::flora::Sweep>,
    /// Conifer forms: how many whorls of branches, [lo, hi]. The default is
    /// one every metre and a quarter of crown, which is a few years' growth.
    #[serde(default)]
    pub whorls: Option<[f32; 2]>,
    /// Conifer forms: 0 to 1. Above 0 the crown is a stack of rounded pucks
    /// of needles centred on the trunk instead of a cone, one per `whorls`,
    /// small at the foot, widest about two fifths up and small again at the
    /// top, with this share of each puck's slot left as a gap where the trunk
    /// shows (an old ponderosa, about 0.25). Default 0, a closed cone.
    #[serde(default)]
    pub open: Option<f32>,
    /// Conifer forms: how much `crown_base` varies from tree to tree, as a
    /// share of the height either side of it (default 0).
    #[serde(default)]
    pub crown_base_vary: Option<f32>,
    /// Form "vine": what it hangs from, "tree" (crowns of broadleaf trees,
    /// and swags between them), "cliff" (the lip of a drop) and/or "wall"
    /// (the outer walls of placed structures). Empty means all three.
    /// Form "crust": what it grows on, "tree" (trunks) and/or "rock" (steep
    /// faces of bare ground). Empty means both.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Form "crust": how it sits on its host. "bracket" stands out from a
    /// trunk in shelves (bracket fungus); "patch" (the default) is a skin
    /// hugging the surface (moss, lichen). `crown_radius` is one patch's
    /// radius and `height` the band of the host, metres above the ground,
    /// a colony keeps to.
    #[serde(default)]
    pub crust: Option<String>,
    /// Form "crust": keeps to the side of its host that faces away from the
    /// midday sun, which is north. Defaults to true for patches (moss grows
    /// on the shaded face) and false for brackets.
    #[serde(default)]
    pub shaded: Option<bool>,
    /// Forms "canes", "kelp" and "bamboo": how many canes, fronds or culms, [lo, hi].
    #[serde(default)]
    pub stems: Option<[f32; 2]>,
    /// Flowers at the tips of canes and arms (forms "canes", "cactus"): a
    /// material, drawn as small blobs.
    #[serde(default)]
    pub flower: Option<String>,
    /// A flower stalk some plants send up from the heart of the rosette:
    /// [marker material, share of plants]. The marker is a herb voxel the
    /// sprite pass draws as the stalk (mesh/sprites.rs `Herb::Stalk`).
    #[serde(default)]
    pub stalk: Option<(String, f32)>,
    /// Seasons the `flower` shows in (world.json `season`); empty means all.
    #[serde(default)]
    pub flower_seasons: Vec<String>,
    /// Water plants (kelp): the depth of water, metres, it grows in. It is
    /// placed only on the bed under that much sea, and `height` is then a
    /// fraction of the depth (the fronds reach the surface).
    #[serde(default)]
    pub water_depth: Option<[f32; 2]>,
    /// Land plants that stand in the sea's shallows (cypress, mangrove): the
    /// depth of water, metres, the ground may lie under, from negative (that
    /// far above the sea) to positive. Default: only ground at least 0.6 m
    /// above the sea. The plant grows at full height from the bed.
    #[serde(default)]
    pub standing_water: Option<[f32; 2]>,
    /// What it offers animals: browse, perch, nest, cover
    /// (creatures::affordance::AFFORDANCES).
    #[serde(default)]
    pub affords: Vec<String>,
    /// The realms it belongs to (`library/realms/`): which world its kind
    /// comes from. `real` where it names none. A world's fill only draws a
    /// species into a biome whose realms it shares.
    #[serde(default)]
    pub realms: Vec<String>,
    /// The ground it does well on (world/site.rs), read only by placement
    /// v2 (world.json `"plants": "v2"`). Left out, it grows wherever its
    /// biome puts it.
    #[serde(default)]
    pub site: Option<SitePrefs>,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

/// A plant's soft range on each site field, 0 to 1: full suitability inside
/// `[lo, hi]`, falling to almost nothing over `site::SOFT` outside. A field
/// left out doesn't matter to it.
#[derive(Deserialize, Clone, Debug, Default, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SitePrefs {
    /// Water in the ground: 0.1-0.3 a dry ridge, 0.5-0.8 a gully floor, 1 open water.
    #[serde(default)]
    pub wet: Option<[f32; 2]>,
    /// 0 acid and poor (heath, bog) to 1 base-rich (limestone, rich meadow).
    #[serde(default)]
    pub soil: Option<[f32; 2]>,
    /// Warmth by slope and aspect: 0.5 flat, low on north faces, high on south.
    #[serde(default)]
    pub sun: Option<[f32; 2]>,
    /// Height over the land round it and facing into the wind.
    #[serde(default)]
    pub exposure: Option<[f32; 2]>,
    /// Cold: 0.15 at sea level up to 1 about 220 m above it, colder on north
    /// faces and in frost hollows, milder by the sea.
    #[serde(default)]
    pub cold: Option<[f32; 2]>,
    /// Steep, cliff, bare stone.
    #[serde(default)]
    pub rock: Option<[f32; 2]>,
    /// Recent fire, paths, roads, buildings.
    #[serde(default)]
    pub disturbed: Option<[f32; 2]>,
    /// The land's shade and the crowns of the taller plants placed before it.
    #[serde(default)]
    pub shade: Option<[f32; 2]>,
}

impl SitePrefs {
    /// Each range given, by its field's name.
    pub fn ranges(&self) -> [(&'static str, Option<[f32; 2]>); 8] {
        [
            ("wet", self.wet),
            ("soil", self.soil),
            ("sun", self.sun),
            ("exposure", self.exposure),
            ("cold", self.cold),
            ("rock", self.rock),
            ("disturbed", self.disturbed),
            ("shade", self.shade),
        ]
    }
}

pub(super) fn default_trunk() -> [f32; 2] {
    [0.2, 0.3]
}
pub(super) fn default_crown() -> [f32; 2] {
    [1.5, 2.5]
}
pub(super) fn default_slope() -> f32 {
    0.6
}
