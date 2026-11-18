//! Plants: one entry per species, drawn by its growth form (`world::flora::Form`).

#[allow(unused_imports)]
use super::*;

#[derive(Deserialize, Clone, Debug)]
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
    /// a beech).
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
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
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
