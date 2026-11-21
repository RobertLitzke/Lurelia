//! Flat sprites beside blocks: grass tufts and wildflowers standing on grass,
//! and leaf clusters around the outside of canopies. They make the world look
//! alive without changing it: nothing here is stored in the voxels, physics never
//! sees a sprite, and digging or building simply remeshes them along with the
//! chunk. Which block grows what is the table in `rule`.
//!
//! A sprite is one packed record of four u32, kept in the chunk's quad
//! allocation after the six face groups (see `Mesh::sprites`):
//!   word0: x:10 z:10 y:11, sixteenths of a voxel in the chunk (y biased by +2)
//!   word1: tint material:8 kind:8 slot:16 (slot filled in at upload)
//!   word2: angle:8 width:8 height:8 seed:8 (sizes in eighths of a voxel)
//!   word3: texture layer:8 face direction:3 (7 = up) plane:1 droop:6
//! Kind CARD stands upright on its point and turns by `angle`; a tuft is two
//! cards at right angles. Kind BILLBOARD is centred on its point, always faces
//! the camera and is rolled by `angle`. Kind SPRAY is a conifer branch: it grows
//! from its point out along the compass heading `angle`, drooping by `droop`
//! (1.5 degree steps); `plane` 0 lies flat, 1 stands upright, and a branch is
//! one of each, so it never vanishes edge-on. Tint material 0 draws the texture's own
//! colours; any other takes its colour from that material's texture, so grass
//! matches the ground it grows from. The shader adds wind (foliage.wgsl).

use super::{ground_height_h, Quad};
use crate::world::gen::{pidx, M, MY, PAD, PAD3, PADY};
use crate::world::material::*;
use crate::world::noise::{hash3, perlin2, smoothstep, unit};

pub const CARD: u32 = 0;
pub const BILLBOARD: u32 = 1;
pub const SPRAY: u32 = 2;
/// A fern frond: like SPRAY but lying flat and arching, rising from its root
/// at `droop` (1.5 degree steps) and curving over toward the tip.
pub const ARCH: u32 = 3;
/// A floating leaf: a square lying flat on the water over the bed it roots
/// in, whatever the depth, turned by `angle` and lifting on the swell. Its
/// root is kept at the water's own surface rather than at the bed, because
/// a sprite cannot read the fields: `scatter`'s `water` looks the level up
/// once per patch, so a lake or a river carries pads as the sea does.
pub const FLOAT: u32 = 4;

/// Sprite texture layers, in the order foliage textures are built.
pub mod layer {
    pub const TUFT_SHORT: u32 = 0;
    pub const TUFT_TALL: u32 = 1;
    pub const TUFT_SEED: u32 = 2;
    pub const FLOWERS_WHITE: u32 = 3;
    pub const FLOWERS_BLUE: u32 = 4;
    pub const LEAVES_A: u32 = 5;
    pub const LEAVES_B: u32 = 6;
    pub const NEEDLES: u32 = 7;
    /// Big plants, drawn at the full texture size (render/sprite_plants.rs).
    pub const FROND: u32 = 8;
    pub const FIREWEED: u32 = 9;
    pub const HEATHER: u32 = 10;
    pub const GLASSBLOOM: u32 = 11;
    /// Sagebrush leaf clusters (a billboard, like LEAVES_A).
    pub const SAGE: u32 = 12;
    /// Long grass, 1.1 to 1.6 m: a grass layer, but drawn at the full texture
    /// size like the big plants.
    pub const TUFT_LONG: u32 = 13;
    /// Rabbitbrush: a dome of upright grey-green stems with yellow heads.
    pub const RABBITBRUSH: u32 = 14;
    /// A 2 m patch of grass on one wide card, standing in for the tufts
    /// further off than they are drawn (FADE_FAR).
    pub const TUFT_PATCH: u32 = 15;
    /// Yellow composites and scarlet paintbrush: the prairie's extra colours
    /// (small layers like FLOWERS_WHITE, though numbered after the big plants).
    pub const FLOWERS_YELLOW: u32 = 16;
    pub const FLOWERS_RED: u32 = 17;
    /// Forest understorey shrubs (herbs): small-leaved huckleberry and
    /// broad-leaved thimbleberry, each a clump of leafy stems.
    pub const HUCKLEBERRY: u32 = 18;
    pub const THIMBLEBERRY: u32 = 19;
    /// Flower stalks over desert rosettes, stiff in the wind (STILL): an
    /// agave's tall candelabra of yellow flower clusters, the same gone dry,
    /// and a yucca's spike of creamy bells.
    pub const AGAVE_STALK: u32 = 20;
    pub const AGAVE_STALK_DRY: u32 = 21;
    pub const YUCCA_STALK: u32 = 22;
    /// Pink rose blooms among leaves, a billboard on rose bushes (untinted).
    pub const ROSES: u32 = 23;
    /// Marsh plants in their own colours (Lowmouth): a clump of cattail
    /// leaves, a cattail's stem and brown head (a stalk, like an agave's),
    /// a sawgrass tussock, and a saw palmetto's fans.
    pub const CATTAIL: u32 = 24;
    pub const CATTAIL_HEAD: u32 = 25;
    pub const SAWGRASS: u32 = 26;
    pub const PALMETTO: u32 = 27;
    /// Sugar cane: a tall clump of jointed stalks and ribbon leaves.
    pub const CANE: u32 = 28;
    /// Small pale yellow blooms among a vine's leaves (untinted).
    pub const VINE_FLOWERS: u32 = 29;
    /// Wisteria: hanging racemes, full at the shoulder and paling toward the
    /// tip. Painted pale and tinted by the strand it hangs from, so the vine's
    /// coat (violet, lilac, white) colours its flowers too.
    pub const WISTERIA: u32 = 30;
    /// Glow-worm threads: fine strands hung with drops of blue light, under
    /// a glowing cave roof (untinted, self-lit in foliage.wgsl).
    pub const GLOW_THREADS: u32 = 31;
    /// A water lily's leaf, seen from above: a round pad with a notch cut to
    /// its middle, drawn flat on the water (kind FLOAT).
    pub const LILY_PAD: u32 = 32;
    /// The lily's own bloom, a small flat star of petals in its own colours.
    pub const LILY_FLOWER: u32 = 33;
    /// Lupine: a clump of dense violet flower spikes over fans of leaves
    /// (Ribshore's dunes), in its own colours like fireweed.
    pub const LUPINE: u32 = 34;
    pub const COUNT: u32 = 35;

    /// Layers numbered after FROND that are still drawn small (sprite_tex.rs).
    pub fn small(l: u32) -> bool {
        l < FROND || l == FLOWERS_YELLOW || l == FLOWERS_RED || l == ROSES || l == VINE_FLOWERS || l == WISTERIA || l == GLOW_THREADS || l == LILY_FLOWER
    }

    /// Blooms drawn in their own colour on a canopy, not the block's.
    pub fn untinted(l: u32) -> bool {
        l == ROSES || l == VINE_FLOWERS || l == GLOW_THREADS || l == LILY_FLOWER
    }

    /// Billboards that are drawn with a top and a bottom: a wisteria raceme
    /// hangs and a glowworm thread drops from the roof, so these keep the
    /// camera's own up instead of taking a random roll.
    pub fn upright(l: u32) -> bool {
        l == WISTERIA || l == GLOW_THREADS
    }
}

/// How a card shrinks away with distance (word3 bits 18-19, foliage.wgsl):
/// grass, near-only fill grass, out as far as leaves, or far-only grass
/// patches that grow in as the tufts shrink away.
pub const FADE_GRASS: u32 = 0;
pub const FADE_NEAR: u32 = 1 << 18;
pub const FADE_LEAF: u32 = 2 << 18;
pub const FADE_FAR: u32 = 3 << 18;
/// Word3 bit 20: a card the wind doesn't bend (foliage.wgsl), for woody stalks.
pub const STILL: u32 = 1 << 20;
/// Word3 bit 21: a card that grows under the sea as well as over it, so
/// foliage.wgsl doesn't hide it at the waterline (eelgrass).
pub const WET: u32 = 1 << 21;

#[derive(Clone, Copy)]
enum Rule {
    /// Tufts on the top face. In a meadow, a noise field decides how much grows
    /// tall (density runs from `sparse` to `dense` with it); elsewhere `tall` is fixed.
    /// `short` scales the height of the shortest grass (the material's `grass`
    /// block; 1.0 is what every grass had before).
    /// `prairie` grass grows four flower colours in patches, and now and then a
    /// field of one (`flower`).
    Ground {
        tint: u8,
        sparse: f32,
        dense: f32,
        meadow: bool,
        tall: f32,
        prairie: bool,
        wild: bool,
        /// Height of the shortest grass, 1.0 being what every grass had
        /// before the material's `grass` block could raise it.
        short: f32,
    },
    /// Clusters on open faces of a canopy.
    /// `tint` is the material whose colour the leaf cards take; 0 is the
    /// block's own. An autumn vine is a brown stem with red leaves on it.
    Canopy { layers: &'static [u32], chance: f32, size: (f32, f32), tint: u8 },
    /// Branch sprays out of the open sides of a conifer, `size` long.
    Branches { chance: f32, size: (f32, f32) },
    /// A whole small plant drawn as sprites from the one voxel that marks it
    /// (flora.rs form "herb"): a ring of arching fronds, or clumps of stems.
    Herb(Herb),
}

#[derive(Clone, Copy)]
enum Herb {
    /// 8 to 12 arching fronds round the root.
    Fern,
    /// `count` clumps, each a crossed pair of cards of `layer` in its own
    /// colours, scattered over `spread` voxels, `height` voxels tall and
    /// `aspect` as wide as tall (sprite_plants.rs draws each to its aspect).
    Clumps {
        layer: u32,
        count: (u32, u32),
        spread: f32,
        height: (f32, f32),
        aspect: f32,
    },
    /// A flower stalk rising from a rosette (flora.rs plant `stalk`): one
    /// crossed pair of still cards, `height` voxels tall and `aspect` as wide,
    /// of `layer`, or of `dry` on a share `dry_share` of plants.
    Stalk { layer: u32, dry: u32, dry_share: f32, height: (f32, f32), aspect: f32 },
    /// Floating leaves (kind FLOAT): `count` pads of `layer`, each lying flat on
    /// the water over the bed, scattered over `spread` voxels and `size`
    /// voxels across, a share `bloom` of them carrying a `flower` card.
    Pads {
        layer: u32,
        flower: u32,
        count: (u32, u32),
        spread: f32,
        size: (f32, f32),
        bloom: f32,
    },
}

fn rule(m: u8) -> Option<Rule> {
    match m {
        GRASS => Some(Rule::Ground {
            tint: GRASS,
            sparse: 0.10,
            dense: 0.85,
            meadow: true,
            tall: 0.0,
            prairie: false,
            wild: false,
            short: 1.0,
        }),
        // Dune grass, thin and tinted like the meadow.
        SAND => Some(Rule::Ground {
            tint: GRASS,
            sparse: 0.06,
            dense: 0.06,
            meadow: false,
            tall: 1.0,
            prairie: false,
            wild: false,
            short: 1.0,
        }),
        LEAVES | BIRCH_LEAVES | LEAVES_AUTUMN => Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_A, layer::LEAVES_B],
            chance: 0.5,
            size: (2.2, 3.2),
        }),
        PINE => Some(Rule::Branches { chance: 0.75, size: (1.7, 2.6) }),
        _ => library_rule(m),
    }
}

/// Library materials (ids 17 and up) go by name for now. Better would be a
/// "sprites" entry in each material's JSON, so a new material can say what
/// grows on it.
fn library_rule(m: u8) -> Option<Rule> {
    let name = crate::world::matlib::name(m)?;
    // Prairie grass grows the four-colour flower fields; wildflower grass (a
    // hay meadow) grows them thicker, short grass included.
    let wild = name.starts_with("WILDFLOWER");
    let prairie = name == "PRAIRIE GRASS" || wild;
    let own = crate::world::matlib::grass(m);
    let ground = |sparse: f32, dense: f32, meadow, tall| {
        let [sparse, dense, short] = own.unwrap_or([sparse, dense, 1.0]);
        Some(Rule::Ground {
            tint: m,
            sparse,
            dense,
            meadow,
            tall,
            prairie,
            wild,
            short,
        })
    };
    let clumps = |layer, count, spread, height, aspect| Some(Rule::Herb(Herb::Clumps { layer, count, spread, height, aspect }));
    // Floating leaves, whatever the material is called: its marker voxel sits
    // on the bed and the pads lie on the water over it.
    if crate::world::matlib::has(m, crate::world::matlib::flag::FLOATING) {
        return Some(Rule::Herb(Herb::Pads {
            layer: layer::LILY_PAD,
            flower: layer::LILY_FLOWER,
            count: (4, 9),
            spread: 3.8,
            size: (2.2, 4.4),
            bloom: 0.13,
        }));
    }
    if name == "AGAVE STALK" {
        return Some(Rule::Herb(Herb::Stalk {
            layer: layer::AGAVE_STALK,
            dry: layer::AGAVE_STALK_DRY,
            dry_share: 0.35,
            height: (8.4, 14.0),
            aspect: 0.6,
        }));
    }
    if name == "YUCCA STALK" {
        return Some(Rule::Herb(Herb::Stalk {
            layer: layer::YUCCA_STALK,
            dry: layer::YUCCA_STALK,
            dry_share: 0.0,
            height: (4.5, 8.0),
            aspect: 0.32,
        }));
    }
    // Marsh plants, before anything that matches on GRASS.
    if name == "CATTAIL LEAVES" {
        return clumps(layer::CATTAIL, (2, 3), 0.8, (6.0, 8.5), 0.5);
    }
    if name == "CATTAIL HEAD" {
        return Some(Rule::Herb(Herb::Stalk {
            layer: layer::CATTAIL_HEAD,
            dry: layer::CATTAIL_HEAD,
            dry_share: 0.0,
            height: (7.0, 9.6),
            aspect: 0.14,
        }));
    }
    if name == "SAWGRASS BLADES" {
        return clumps(layer::SAWGRASS, (2, 4), 1.4, (5.0, 6.8), 0.85);
    }
    if name == "CANE LEAVES" {
        return clumps(layer::CANE, (3, 5), 1.1, (11.0, 15.0), 0.45);
    }
    if name == "PALMETTO FRONDS" {
        return clumps(layer::PALMETTO, (2, 3), 1.3, (3.0, 4.6), 1.3);
    }
    if name == "FERN" || name.contains("FERN ") {
        Some(Rule::Herb(Herb::Fern))
    } else if name.contains("LUPINE") {
        clumps(layer::LUPINE, (3, 4), 1.4, (2.6, 4.2), 0.75)
    } else if name.contains("FIREWEED") {
        clumps(layer::FIREWEED, (2, 3), 1.2, (4.0, 6.6), 0.6)
    } else if name == "HEATHER" {
        // A low mat: several wide, short clumps run together.
        clumps(layer::HEATHER, (4, 6), 2.4, (1.4, 2.6), 1.4)
    } else if name == "GLASSBLOOM" {
        clumps(layer::GLASSBLOOM, (2, 3), 1.0, (3.6, 5.6), 0.9)
    } else if name == "HUCKLEBERRY" {
        // A loose, knee-to-waist-high bush: several clumps run together.
        clumps(layer::HUCKLEBERRY, (3, 5), 2.2, (3.0, 4.6), 1.25)
    } else if name == "THIMBLEBERRY" {
        clumps(layer::THIMBLEBERRY, (3, 4), 2.0, (3.2, 5.0), 1.2)
    } else if name == "RABBITBRUSH" {
        // A rounded bush of upright stems: a few wide clumps run together.
        clumps(layer::RABBITBRUSH, (3, 4), 1.4, (2.6, 4.0), 1.15)
    } else if name == "TAMARISK" {
        // Feathery scale-leaved sprays round the tamarisk's crown.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::SAGE],
            chance: 0.75,
            size: (2.0, 3.0),
        })
    } else if name == "PINE DUFF" {
        // Ponderosa parkland: brown needle litter between tan bunchgrass. The
        // tufts take their colour from dry_bunchgrass, not from the duff.
        let tint = crate::world::matlib::id_of("dry_bunchgrass").unwrap_or(m);
        Some(Rule::Ground {
            tint,
            sparse: 0.3,
            dense: 0.85,
            meadow: true,
            tall: 0.0,
            prairie: false,
            wild: false,
            short: 1.0,
        })
    } else if name == "JUNIPER FOLIAGE" {
        // Scale-leaved sprays feathering the juniper's clumps, in its own colour.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::SAGE],
            chance: 0.7,
            size: (1.6, 2.4),
        })
    } else if name == "PALM FROND" {
        // Stiff fans sticking out all round a cabbage palm's head.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::PALMETTO],
            chance: 0.85,
            size: (1.8, 2.6),
        })
    } else if name == "ROSE BUSH" {
        // Leaves with roses among them, the roses in their own colour.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_B, layer::ROSES, layer::ROSES],
            chance: 0.8,
            size: (1.0, 1.6),
        })
    } else if name == "SAGE" {
        // Small grey leaves feathering the sagebrush's rounded crown.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::SAGE],
            chance: 0.6,
            size: (1.8, 2.6),
        })
    } else if wild {
        ground(0.3, 0.9, true, 0.0)
    } else if name == "STEPPE GRASS" {
        // Grazed turf in an open arena: low tufts everywhere and no meadow
        // stands of long grass, which in a fight are a wall. `tall` 0.4 picks
        // the tall and seeding cards, whose colour the straw turns gold, over
        // the short card's dark green, and still stands only knee-high.
        ground(0.3, 0.3, false, 0.4)
    } else if name.contains("SHORE GRASS") {
        ground(0.04, 0.35, true, 0.0)
    } else if name.contains("GRASS") {
        ground(0.10, 0.85, true, 0.0)
    } else if name.contains("FOREST FLOOR") {
        ground(0.12, 0.12, false, 0.2)
    } else if name.contains("NEEDLES") {
        Some(Rule::Branches { chance: 0.75, size: (1.7, 2.6) })
    } else if name == "LANTERN FILM" || name == "GLOWWORM CEILING" {
        // Threads hanging from a glowing cave roof.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::GLOW_THREADS],
            chance: 0.1,
            size: (1.0, 2.0),
        })
    } else if name == "KARST MOSS" {
        // Now and then a fern sticking out of the moss on a wall.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::FROND],
            chance: 0.06,
            size: (1.4, 2.4),
        })
    } else if name.contains("WISTERIA") {
        // Wisteria strands: leaves, and racemes hanging all down them.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_B, layer::WISTERIA, layer::WISTERIA, layer::WISTERIA],
            chance: 0.75,
            size: (1.7, 2.6),
        })
    } else if name == "VINE LEAVES" {
        // The common vine: small leaves, so a strand stays a strand, and now
        // and then a spray of pale yellow flowers.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_A, layer::LEAVES_B, layer::LEAVES_A, layer::LEAVES_B, layer::VINE_FLOWERS],
            chance: 0.45,
            size: (1.2, 1.9),
        })
    } else if name == "AUTUMN VINE" {
        // A vine in autumn: the stem goes brown and keeps its small leaves,
        // which turn red (maple_red's colour) rather than the stem's.
        Some(Rule::Canopy {
            tint: crate::world::matlib::id_of("maple_red").unwrap_or(0),
            layers: &[layer::LEAVES_A, layer::LEAVES_B],
            chance: 0.6,
            size: (1.2, 1.9),
        })
    } else if name.contains("VINE") {
        // Hanging strands: small leaves, so a strand stays a strand.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_A, layer::LEAVES_B],
            chance: 0.4,
            size: (1.2, 1.9),
        })
    } else if name.contains("FLOWER") && !name.contains("GRASS") {
        // Flower heads on a crown (an elder's corymbs): leaf cards in the
        // block's own cream, so the heads read as blossom and not as slabs.
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_B, layer::LEAVES_A],
            chance: 0.7,
            size: (1.3, 2.0),
        })
    } else if name.contains("LEAVES") {
        Some(Rule::Canopy {
            tint: 0,
            layers: &[layer::LEAVES_A, layer::LEAVES_B],
            chance: 0.5,
            size: (2.2, 3.2),
        })
    } else {
        None
    }
}

/// `rule` for every material id, worked out again whenever a world installs
/// its own material ids (`matlib::generation`); each table is leaked, a
/// handful a session.
///
/// Read without a lock: the mesher asks for every voxel on every worker
/// thread, and a shared RwLock there made eight workers slower than one.
/// Two threads seeing a new generation at once may each build a table; one
/// wins and the other is leaked too.
fn rules() -> &'static [Option<Rule>; 256] {
    use std::sync::atomic::{AtomicPtr, Ordering};
    struct Rules {
        gen: u32,
        table: [Option<Rule>; 256],
    }
    static RULES: AtomicPtr<Rules> = AtomicPtr::new(std::ptr::null_mut());
    let gen = crate::world::matlib::generation();
    // SAFETY: only ever set to a leaked Box that is never freed.
    if let Some(r) = unsafe { RULES.load(Ordering::Acquire).as_ref() } {
        if r.gen == gen {
            return &r.table;
        }
    }
    let r: &'static mut Rules = Box::leak(Box::new(Rules {
        gen,
        table: std::array::from_fn(|m| rule(m as u8)),
    }));
    RULES.store(r, Ordering::Release);
    &r.table
}

/// Whether `m` marks a herb, a plant drawn only as sprites: the mesher draws
/// no faces for it.
/// Material `m` grows leaf clusters or branch sprays on its open faces, so
/// something standing on it sits on the sprites, not the voxel top
/// (flit.rs perches).
pub fn leafy(m: u8) -> bool {
    matches!(rules()[m as usize], Some(Rule::Canopy {
            tint: 0, .. }) | Some(Rule::Branches { .. }))
}

/// Fill tufts and long grass, on unless VOXEL_GRASS_FILL=0 (for measuring
/// what they cost).
fn extras_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_GRASS_FILL").map(|v| v != "0").unwrap_or(true))
}

/// A far cover card, in voxels of its own chunk: 1.6 m wide at full detail,
/// and twice that at each coarser level, which is what keeps the count flat
/// as the rings widen. It stands about as tall as it is wide on purpose. A
/// card much wider than tall is a stroke, and a field of strokes all turned
/// the same way (they face the camera) combs the hillside into corduroy;
/// near enough to square, the same field reads as fuzz.
pub const PATCH_W: f32 = 6.4;
/// The cells the cards stand in, in voxels: 1 m at full detail. The cards are
/// wider than the cells, so each piece of ground lies under two or three of
/// them and they run together into one stand of grass instead of reading as
/// a scatter of separate marks.
pub const PATCH_CELL: i32 = 4;

/// Far grass patches, on unless VOXEL_GRASS_PATCHES=0 (for comparisons).
fn patches_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_GRASS_PATCHES").map(|v| v != "0").unwrap_or(true))
}

/// Boxes no sprite may be rooted in, metres `[lo, hi]`: the room a moving
/// platform sweeps (`platforms::Platforms::keep_out`). A sprite is drawn
/// from the voxels alone and the mesher never sees a platform, so without
/// this the grass on the floor of a lift pit grows straight up through the
/// plate resting on it.
static KEEP_OUT: std::sync::RwLock<Vec<[f32; 6]>> = std::sync::RwLock::new(Vec::new());

/// Replace the boxes no sprite may grow in. Chunks already meshed keep what
/// they have until they are meshed again.
pub fn set_keep_out(boxes: Vec<[f32; 6]>) {
    *KEEP_OUT.write().unwrap() = boxes;
}

/// How far under a box a root is still kept out, metres: the tallest grass
/// card stands about this high, so anything rooted closer would poke up
/// into the plate.
const KEEP_BELOW: f32 = 1.7;

pub fn marker(m: u8) -> bool {
    m >= 17 && matches!(rules()[m as usize], Some(Rule::Herb(_)))
}

/// The flower layer for a meadow tuft at voxel (fx, fz), if it flowers. `pick`
/// is the tuft's own uniform draw. Plain meadows grow white and blue drifts.
/// Prairie grass grows those plus yellow and scarlet, one colour leading in
/// each ~25 m patch with the others sprinkled through, and here and there a
/// field where flowers crowd in and nearly all are one colour. Over a whole
/// prairie that comes to about 15% more flowers than a plain meadow (test
/// `prairie_flower_share`).
fn flower(fx: f32, fz: f32, tall: f32, pick: f32, prairie: bool) -> Option<u32> {
    let drift = smoothstep(0.1, 0.45, perlin2(fx * 0.05, fz * 0.05, 503));
    if !prairie {
        if pick >= 0.12 * tall * drift {
            return None;
        }
        let blue = perlin2(fx * 0.03, fz * 0.03, 504) > 0.0;
        return Some(if blue { layer::FLOWERS_BLUE } else { layer::FLOWERS_WHITE });
    }
    let field = smoothstep(FIELD.0, FIELD.1, perlin2(fx * 0.009, fz * 0.009, 507));
    let p = 0.12 * tall * drift * (1.0 - field) + field * FIELD.2 * (0.5 + 0.5 * tall);
    if pick >= p {
        return None;
    }
    const COLOURS: [u32; 4] = [layer::FLOWERS_WHITE, layer::FLOWERS_BLUE, layer::FLOWERS_YELLOW, layer::FLOWERS_RED];
    // The patch colour: four bands of a slow noise, each about a quarter of the
    // ground, slow enough that a field (about 10 m) is mostly one colour.
    let hue = perlin2(fx * 0.01, fz * 0.01, 508);
    let lead = [-0.2f32, 0.0, 0.2].iter().filter(|&&b| hue > b).count();
    // Where in [0, p) the pick fell is a second uniform draw, free.
    let u = pick / p;
    let pure = 0.65 + 0.33 * field;
    if u < pure {
        return Some(COLOURS[lead]);
    }
    let other = ((u - pure) / (1.0 - pure) * 3.0) as usize;
    Some(COLOURS[(lead + 1 + other.min(2)) % 4])
}

/// Flower fields on prairie grass: the noise band where they fade in, and
/// their flower share at full height.
const FIELD: (f32, f32, f32) = (0.62, 0.74, 0.3);

/// Face normals in mesh.rs direction order.
const NORMALS: [[isize; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

fn pack(p: [f32; 3], tint: u8, kind: u32, angle: u32, w: f32, h: f32, seed: u32, layer: u32, dir: u32) -> Quad {
    pack_ext(p, tint, kind, angle, w, h, seed, layer | dir << 8)
}

fn pack_ext(p: [f32; 3], tint: u8, kind: u32, angle: u32, w: f32, h: f32, seed: u32, word3: u32) -> Quad {
    let q = |v: f32, max: u32| ((v * 16.0).round().max(0.0) as u32).min(max);
    let e = |v: f32| ((v * 8.0).round().max(1.0) as u32).min(255);
    [
        q(p[0], 1023) | q(p[2], 1023) << 10 | q(p[1] + 2.0, 2047) << 20,
        tint as u32 | kind << 8,
        (angle & 255) | e(w) << 8 | e(h) << 16 | (seed & 255) << 24,
        word3,
    ]
}

/// Sprites for one chunk. `v` is the padded voxel array the mesher reads,
/// `origin` the world voxel coordinates of the chunk's (0, 0, 0) and `smooth`
/// whether the ground is drawn sloped (tufts stand on the drawn slope).
///
/// `out` comes back in four runs so the renderer can draw less of it further
/// away: fill tufts (drawn only near the camera, foliage::NEAR), then the
/// other grass and flowers (out to foliage::GRASS_FAR), then everything else
/// (leaves, sprays, herbs; out to foliage::FAR), then the far grass patches
/// (from foliage::PATCH_FADE out to foliage::PATCH_FAR), with the flower stalks
/// (out to foliage::STALK_FAR) just before the patches. Returns the lengths of
/// the fill, grass, patch and stalk runs.
#[allow(clippy::too_many_arguments)]
pub fn scatter(v: &[u8; PAD3], smooth: bool, colh: Option<&[f32]>, origin: [i32; 3], scale: f32, cover_only: bool, water: &dyn Fn(f32, f32) -> f32, out: &mut Vec<Quad>) -> (u32, u32, u32, u32) {
    // Sizes are stored in the column's own voxels, so a card doubles in
    // metres with every coarser level. That is right for its width, which
    // has to keep up with the ring it covers, but grass four metres tall
    // would stand off a far ridge like a hedge; take the square root of the
    // step out of the height, so it grows but more slowly.
    let lodf = (scale / crate::world::gen::VOXEL).max(1.0);
    let hscale = 1.0 / lodf.sqrt();
    let keep_out = KEEP_OUT.read().unwrap().clone();
    let mut fill: Vec<Quad> = Vec::new();
    let mut grass: Vec<Quad> = Vec::new();
    let mut patches: Vec<Quad> = Vec::new();
    // Flower stalks: obvious from far off, so they get their own run that
    // far chunks still draw (foliage::STALK_FADE).
    let mut stalks: Vec<Quad> = Vec::new();
    let ch = |x: isize, z: isize| -> Option<f32> {
        let c = colh?;
        if x < 0 || z < 0 || x >= PAD as isize || z >= PAD as isize {
            return None;
        }
        Some(c[z as usize * PAD + x as usize])
    };
    out.clear();
    let at = |x: isize, y: isize, z: isize| -> u8 {
        if x < 0 || z < 0 || y < 0 || x >= PAD as isize || z >= PAD as isize || y >= PADY as isize {
            AIR
        } else {
            v[pidx(x as usize, y as usize, z as usize)]
        }
    };
    // Material of the ground surface of column (x, z) within a level of t, if any.
    let surface = |x: isize, z: isize, t: isize| -> Option<u8> {
        [t, t + 1, t - 1].into_iter().find_map(|s| {
            let m = at(x, s - 1, z);
            (m != AIR && at(x, s, z) == AIR).then_some(m)
        })
    };
    let (m0, my0) = (M as isize, MY as isize);
    for y in 0..32isize {
        let py = y + my0;
        for z in 0..32isize {
            let pz = z + m0;
            for x in 0..32isize {
                let px = x + m0;
                let m = at(px, py, pz);
                if m == AIR {
                    continue;
                }
                let Some(r) = rules()[m as usize] else {
                    continue;
                };
                // Past the full-detail radius a column carries the far cover
                // and nothing else: tufts, leaves and herbs have all shrunk
                // away long before, and the cover is what the distance is
                // made of.
                if cover_only && !matches!(r, Rule::Ground { .. }) {
                    continue;
                }
                let (wx, wy, wz) = (origin[0] + x as i32, origin[1] + y as i32, origin[2] + z as i32);
                if !keep_out.is_empty() {
                    // The top of this voxel, in metres, and the cell round it.
                    let (rx, ry, rz) = ((wx as f32 + 0.5) * scale, (wy + 1) as f32 * scale, (wz as f32 + 0.5) * scale);
                    let h = scale * 0.5;
                    if keep_out.iter().any(|b| rx + h > b[0] && rx - h < b[3] && rz + h > b[2] && rz - h < b[5] && ry > b[1] - KEEP_BELOW && ry <= b[4]) {
                        continue;
                    }
                }
                match r {
                    Rule::Ground {
                        tint,
                        sparse,
                        dense,
                        meadow,
                        tall,
                        prairie,
                        wild,
                        short,
                    } => {
                        if at(px, py + 1, pz) != AIR {
                            continue;
                        }
                        // Meadows: broad patches where most of the grass grows tall.
                        let (fx, fz) = (wx as f32, wz as f32);
                        let tall = if meadow {
                            let n = perlin2(fx * 0.018, fz * 0.018, 501) * 0.7 + perlin2(fx * 0.07, fz * 0.07, 502) * 0.3;
                            smoothstep(-0.15, 0.3, n)
                        } else {
                            tall
                        };
                        let d = sparse + (dense - sparse) * tall;
                        // A drowned meadow (eelgrass): its cards are not hidden
                        // at the waterline the way land grass is.
                        let wet = if crate::world::matlib::has(m, crate::world::matlib::flag::UNDERWATER) { WET } else { 0 };
                        // Far patches: in each 2 m cell (8 x 8 columns) one column,
                        // picked by hash, may hold a crossed pair of wide cards
                        // standing in for the cell's tufts. A cell of sparse
                        // grass (d = 0.1) holds about six tufts, whose cards cover
                        // about as much as one patch, so almost every cell gets
                        // one; sparse ones are lower.
                        if patches_on() {
                            let (cx, cz) = (wx.div_euclid(PATCH_CELL), wz.div_euclid(PATCH_CELL));
                            let hc = hash3(cx, 0, cz, 23);
                            let cm = PATCH_CELL as u32 - 1;
                            if wx.rem_euclid(PATCH_CELL) == (hc & cm) as i32 && wz.rem_euclid(PATCH_CELL) == (hc >> 3 & cm) as i32 && unit(hash3(cx, wy, cz, 24)) < (d * 20.0).min(1.0) {
                                let ground = if smooth {
                                    ground_height_h(at, ch, px as f32 + 0.5, pz as f32 + 0.5, (py + 1) as f32).unwrap_or((py + 1) as f32)
                                } else {
                                    (py + 1) as f32
                                };
                                // Sunk a little, so on a slope the low end stays down.
                                let pos = [x as f32 + 0.5, ground - my0 as f32 - 0.4, z as f32 + 0.5];
                                // Half again as tall as the tufts it stands
                                // for: a card this wide, seen from above down
                                // a slope, reads as a dark stroke lying on
                                // the ground unless it has some height to it.
                                let hgt = ((short + 1.7 * tall) * (0.75 + 0.25 * d.min(1.0)) + 0.35) * 1.5 * hscale;
                                // One card, not a crossed pair: it turns to
                                // face the camera (foliage.wgsl), so the
                                // second of a pair would land on top of it.
                                let a = hc >> 8 & 255;
                                patches.push(pack(pos, tint, CARD, a, PATCH_W, hgt, hc >> 16 & 255, layer::TUFT_PATCH | FADE_FAR | wet, 7));
                            }
                        }
                        if cover_only {
                            continue;
                        }
                        // One tuft a column at most, seen from any distance, and in
                        // meadows up to three more that only the near ring draws
                        // (FADE_NEAR): close up the gaps between tufts show, while
                        // further off the cards overlap anyway. Dunes and forest
                        // floor stay sparse.
                        let base = unit(hash3(wx, wy, wz, 17)) < d;
                        let mut rng = hash3(wx, wy, wz, 18);
                        let mut rnd = || {
                            rng = crate::world::noise::hash32(rng.wrapping_add(0x9e37_79b9));
                            unit(rng)
                        };
                        let fill_p = if meadow && extras_on() { (d + 0.15).min(0.9) } else { 0.0 };
                        let fills = (0..3).filter(|_| rnd() < fill_p).count();
                        if !base && fills == 0 {
                            continue;
                        }
                        // No tufts on a border with another ground: the shader blends
                        // the two there, and grass standing on sand looks wrong.
                        if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dz)| surface(px + dx, pz + dz, py + 1).is_some_and(|s| s != m)) {
                            continue;
                        }
                        // Long grass: patches inside the tall meadow where blades
                        // reach 1.1 to 1.6 m, where there is room overhead. The
                        // stands thin out over several metres at their edges, and
                        // fill tufts are seldom long, or close up a stand is a wall.
                        let long = if meadow && extras_on() {
                            tall * smoothstep(0.0, 0.5, perlin2(fx * 0.03, fz * 0.03, 505))
                        } else {
                            0.0
                        };
                        let room = (2..=8).take_while(|&k| at(px, py + k, pz) == AIR).count();
                        for i in 0..(base as usize + fills) {
                            let near = !base || i > 0;
                            // Anywhere in the column, so close up the tufts show no grid.
                            let ox = 0.05 + 0.9 * rnd();
                            let oz = 0.05 + 0.9 * rnd();
                            let ground = if smooth {
                                ground_height_h(at, ch, (px as f32) + ox, (pz as f32) + oz, (py + 1) as f32).unwrap_or((py + 1) as f32)
                            } else {
                                (py + 1) as f32
                            };
                            let pos = [x as f32 + ox, ground - my0 as f32, z as f32 + oz];
                            let jitter = 0.75 + 0.5 * rnd();
                            let pick = rnd();
                            let is_long = room == 7 && rnd() < long * if near { 0.25 } else { 0.8 };
                            let (width, height) = if is_long {
                                let hgt = 4.4 + 2.2 * rnd();
                                (hgt * 0.75, hgt)
                            } else {
                                // Fill tufts a little shorter, an understorey.
                                let mut hgt = (short + 1.7 * tall) * jitter * if near { 0.85 } else { 1.0 };
                                if room == 0 {
                                    hgt = hgt.min(1.0);
                                }
                                ((hgt * 1.1).clamp(1.2, 2.6), hgt)
                            };
                            let bloom = if !near && !is_long && meadow {
                                if wild {
                                    flower(fx, fz, tall.max(0.7), pick * 0.4, true)
                                } else {
                                    flower(fx, fz, tall, pick, prairie)
                                }
                            } else {
                                None
                            };
                            let (tint, layer) = if let Some(l) = bloom {
                                (0, l)
                            } else if is_long {
                                (tint, layer::TUFT_LONG)
                            } else if tall < 0.35 {
                                (tint, layer::TUFT_SHORT)
                            } else if pick < 0.35 * tall {
                                (tint, layer::TUFT_SEED)
                            } else {
                                (tint, layer::TUFT_TALL)
                            };
                            let a = (rnd() * 256.0) as u32;
                            let seed = (rnd() * 255.0) as u32;
                            let (dst, fade) = if near { (&mut fill, FADE_NEAR) } else { (&mut grass, FADE_GRASS) };
                            for turn in [0, 64] {
                                dst.push(pack(pos, tint, CARD, a + turn, width, height, seed, layer | fade | wet, 7));
                            }
                        }
                    }
                    Rule::Canopy { layers, chance, size, tint: leaf_tint } => {
                        for (d, n) in NORMALS.iter().enumerate() {
                            if at(px + n[0], py + n[1], pz + n[2]) != AIR {
                                continue;
                            }
                            // In a gap inside a crown drawn as leaf blocks: unseen.
                            if crate::mesh::in_leaf_block(v, smooth, px + n[0], py + n[1], pz + n[2]) {
                                continue;
                            }
                            let h = hash3(wx * 6 + d as i32, wy, wz, 31);
                            // Fewer underneath, where they would only be seen from below.
                            let c = if d == 3 { chance * 0.5 } else { chance };
                            if unit(h) >= c {
                                continue;
                            }
                            let h2 = crate::world::noise::hash32(h);
                            let mut p = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                            for k in 0..3 {
                                let j = if n[k] == 0 { (unit(h2 >> (k * 8) as u32 | h2 << 7) - 0.5) * 0.7 } else { 0.0 };
                                p[k] += n[k] as f32 * 0.55 + j;
                            }
                            let s = size.0 + (size.1 - size.0) * unit(h2);
                            let layer = layers[(h2 >> 4) as usize % layers.len()];
                            // Flowers keep their own colour; leaves take the block's.
                            let tint = if layer::untinted(layer) { 0 } else if leaf_tint != 0 { leaf_tint } else { m };
                            let roll = if layer::upright(layer) { 0 } else { h >> 24 };
                            out.push(pack(p, tint, BILLBOARD, roll, s, s, h >> 16, layer, d as u32));
                        }
                    }
                    Rule::Herb(herb) => {
                        // Only the marker voxel standing on something grows the plant.
                        if at(px, py - 1, pz) == AIR || at(px, py - 1, pz) == m {
                            continue;
                        }
                        let h = hash3(wx, wy, wz, 41);
                        let mut rng = h;
                        let mut rnd = || {
                            rng = crate::world::noise::hash32(rng.wrapping_add(0x9e37_79b9));
                            unit(rng)
                        };
                        let ground = if smooth {
                            ground_height_h(at, ch, px as f32 + 0.5, pz as f32 + 0.5, py as f32).unwrap_or(py as f32)
                        } else {
                            py as f32
                        };
                        let gy = ground - my0 as f32;
                        if let Herb::Stalk {
                            layer,
                            dry,
                            dry_share,
                            height,
                            aspect,
                        } = herb
                        {
                            let l = if rnd() < dry_share { dry } else { layer };
                            let hgt = height.0 + (height.1 - height.0) * rnd();
                            let p = [x as f32 + 0.5, gy - 0.1, z as f32 + 0.5];
                            let a = (rnd() * 256.0) as u32;
                            let sd = (rnd() * 255.0) as u32;
                            for turn in [0, 64] {
                                stalks.push(pack(p, 0, CARD, a + turn, hgt * aspect, hgt, sd, l | FADE_LEAF | STILL, 7));
                            }
                        } else if let Herb::Pads {
                            layer,
                            flower,
                            count,
                            spread,
                            size,
                            bloom,
                        } = herb
                        {
                            // Lily pads: flat leaves on the water over the bed,
                            // each turned its own way, now and then a bloom.
                            // The water's surface, not the bed, is the root
                            // the shader lays them on (FLOAT): one lookup
                            // for the patch, at its marker voxel. Never
                            // below the bed, so a pad is never buried.
                            let surface = {
                                let (mx, mz) = ((wx as f32 + 0.5) * scale, (wz as f32 + 0.5) * scale);
                                (water(mx, mz) / scale - origin[1] as f32).max(gy)
                            };
                            let n = count.0 + (rnd() * (count.1 - count.0 + 1) as f32) as u32;
                            for _ in 0..n.min(count.1) {
                                let ox = (rnd() - 0.5) * spread;
                                let oz = (rnd() - 0.5) * spread;
                                let p = [x as f32 + 0.5 + ox, surface, z as f32 + 0.5 + oz];
                                let s = size.0 + (size.1 - size.0) * rnd();
                                let a = (rnd() * 256.0) as u32;
                                let sd = (rnd() * 255.0) as u32;
                                out.push(pack(p, m, FLOAT, a, s, s, sd, layer | FADE_LEAF, 7));
                                if rnd() < bloom {
                                    out.push(pack(p, 0, FLOAT, a + 37, s * 0.45, s * 0.45, sd ^ 91, flower | FADE_LEAF, 7));
                                }
                            }
                        } else if let Herb::Clumps { layer, count, spread, height, aspect } = herb {
                            // Clumps of stems, each on a crossed pair of cards.
                            let n = count.0 + (rnd() * (count.1 - count.0 + 1) as f32) as u32;
                            for _ in 0..n.min(count.1) {
                                let ox = (rnd() - 0.5) * spread;
                                let oz = (rnd() - 0.5) * spread;
                                let p = [x as f32 + 0.5 + ox, gy, z as f32 + 0.5 + oz];
                                let hgt = height.0 + (height.1 - height.0) * rnd();
                                let a = (rnd() * 256.0) as u32;
                                let sd = (rnd() * 255.0) as u32;
                                for turn in [0, 64] {
                                    out.push(pack(p, 0, CARD, a + turn, hgt * aspect, hgt, sd, layer | FADE_LEAF, 7));
                                }
                            }
                        } else {
                            // A shuttlecock of 8 to 12 fronds, the inner ones steeper.
                            let n = 8 + (rnd() * 5.0) as u32;
                            let a0 = rnd();
                            for k in 0..n {
                                let heading = a0 + (k as f32 + 0.4 * rnd()) / n as f32;
                                let a = ((heading.rem_euclid(1.0)) * 256.0) as u32;
                                let len = 3.2 + 1.8 * rnd();
                                let rise = 42 + (rnd() * 14.0) as u32;
                                let p = [x as f32 + 0.5, gy + 0.15, z as f32 + 0.5];
                                let word3 = layer::FROND | 7 << 8 | rise << 12;
                                out.push(pack_ext(p, m, ARCH, a, len * 0.32, len, rnd().to_bits() >> 16, word3));
                            }
                        }
                    }
                    Rule::Branches { chance, size } => {
                        // Sides only: a conifer reads as tiers of level branches.
                        for d in [0usize, 1, 4, 5] {
                            let n = NORMALS[d];
                            if at(px + n[0], py, pz + n[2]) != AIR {
                                continue;
                            }
                            // In a gap inside a crown drawn as leaf blocks: unseen.
                            if crate::mesh::in_leaf_block(v, smooth, px + n[0], py, pz + n[2]) {
                                continue;
                            }
                            let h = hash3(wx * 6 + d as i32, wy, wz, 37);
                            if unit(h) >= chance {
                                continue;
                            }
                            let h2 = crate::world::noise::hash32(h);
                            // Root well inside the face, so only the outer part of
                            // the branch shows past the rounded crown.
                            let mut p = [x as f32 + 0.5, y as f32 + 0.3 + 0.4 * unit(h2 >> 3), z as f32 + 0.5];
                            p[0] -= n[0] as f32 * 0.15;
                            p[2] -= n[2] as f32 * 0.15;
                            // Branches are longer lower in the crown: count crown above.
                            let above = (1..=6).take_while(|&k| at(px, py + k, pz) == m || at(px - n[0], py + k, pz - n[2]) == m).count();
                            // Heading: the face normal, turned up to 35 degrees either way.
                            let base = (n[2] as f32).atan2(n[0] as f32);
                            let yaw = base + (unit(h2) - 0.5) * 1.2;
                            let a = ((yaw / std::f32::consts::TAU).rem_euclid(1.0) * 256.0) as u32;
                            let len = (size.0 + (size.1 - size.0) * unit(h2 << 9 | h2 >> 23)) * (0.7 + 0.08 * above as f32);
                            let droop = 10 + (unit(h2 << 17 | h2 >> 15) * 10.0) as u32 + 2 * above as u32;
                            for plane in 0..2u32 {
                                let w = if plane == 0 { len * 0.8 } else { len * 0.5 };
                                let word3 = layer::NEEDLES | (d as u32) << 8 | plane << 11 | droop << 12;
                                out.push(pack_ext(p, m, SPRAY, a, w, len, h >> 16, word3));
                            }
                        }
                    }
                }
            }
        }
    }
    let (n_fill, n_grass, n_patch, n_stalk) = (fill.len() as u32, grass.len() as u32, patches.len() as u32, stalks.len() as u32);
    if n_fill + n_grass + n_patch + n_stalk > 0 {
        let rest = std::mem::take(out);
        out.reserve(fill.len() + grass.len() + rest.len() + stalks.len() + patches.len());
        out.extend(fill);
        out.extend(grass);
        out.extend(rest);
        out.extend(stalks);
        out.extend(patches);
    }
    (n_fill, n_grass, n_patch, n_stalk)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pad lies on the water the world has at that spot, not on the sea:
    /// the surface goes into the sprite's root (foliage.wgsl kind FLOAT
    /// lays the leaf on it), so a lake or a river two metres above sea
    /// level carries its lilies two metres up.
    #[test]
    fn lily_pads_lie_on_the_water_over_their_bed() {
        // A world's own library, because a material has a voxel id only in
        // a world that reaches it (`Library::assign_ids`). The lock is held
        // until the pads have been laid: the table is one per process, and
        // another test installing another world mid-test takes this id's
        // meaning away (`matlib::table_lock`).
        let _table = crate::world::matlib::table_lock();
        let _ = crate::worldload::library_for("lowmouth");
        let lily = (17u8..=254)
            .find(|&m| crate::world::matlib::has(m, crate::world::matlib::flag::FLOATING))
            .expect("the library has a floating plant in it");
        let mut v: Box<[u8; PAD3]> = vec![AIR; PAD3].into_boxed_slice().try_into().unwrap();
        // A bed at the bottom of the chunk with the marker standing on it.
        for z in 0..PAD {
            for x in 0..PAD {
                for y in 0..MY + 4 {
                    v[pidx(x, y, z)] = DIRT;
                }
                v[pidx(x, MY + 4, z)] = lily;
            }
        }
        let scale = crate::world::gen::VOXEL;
        // The chunk starts at 0 m, the bed is at 1 m, the lake surface 3 m up.
        let level = 3.0f32;
        let mut out = Vec::new();
        scatter(&v, false, None, [0, 0, 0], scale, false, &|_, _| level, &mut out);
        let pads: Vec<f32> = out.iter().filter(|q| (q[1] >> 8) & 255 == FLOAT).map(|q| (((q[0] >> 20) & 2047) as f32 / 16.0 - 2.0) * scale).collect();
        assert!(!pads.is_empty(), "no pads were laid at all");
        for y in &pads {
            assert!((y - level).abs() < 0.05, "a pad lies at {y} m and the water is at {level} m");
        }
    }

    fn meadow() -> Box<[u8; PAD3]> {
        let mut v: Box<[u8; PAD3]> = vec![AIR; PAD3].into_boxed_slice().try_into().unwrap();
        for y in 0..MY + 8 {
            for z in 0..PAD {
                for x in 0..PAD {
                    v[pidx(x, y, z)] = if y + 1 == MY + 8 { GRASS } else { DIRT };
                }
            }
        }
        v
    }

    #[test]
    fn grass_grows_tufts_on_its_surface() {
        let v = meadow();
        let mut out = Vec::new();
        scatter(&v, true, None, [0, 0, 0], crate::world::gen::VOXEL, false, &|_, _| f32::MIN, &mut out);
        assert!(out.len() > 100, "{} sprites", out.len());
        for q in &out {
            let y = (q[0] >> 20) as f32 / 16.0 - 2.0;
            // The ground top is at chunk level 8; flat ground, so tufts stand on
            // it and far cover cards are sunk a little into it.
            let sunk = if q[3] & (3 << 18) == FADE_FAR { 0.4 } else { 0.0 };
            assert!((y - 8.0 + sunk).abs() < 0.07, "tuft base at {y}");
        }
    }

    #[test]
    fn grass_comes_back_in_runs_by_draw_distance() {
        let v = meadow();
        let mut out = Vec::new();
        // A spot where the meadow is tall and long grass grows, and one where it doesn't.
        for origin in [[0, 0, 0], [640, 0, 320], [-900, 0, 1500]] {
            let (n_fill, n_grass, n_patch, _) = scatter(&v, true, None, origin, crate::world::gen::VOXEL, false, &|_, _| f32::MIN, &mut out);
            assert!(n_fill > 0 && n_grass > 0 && n_patch > 0, "{n_fill} fill, {n_grass} grass, {n_patch} patches at {origin:?}");
            assert_eq!((n_fill + n_grass + n_patch) as usize, out.len(), "nothing but grass on a meadow");
            // At most one far cover card per cell, and a chunk holds
            // (32 / PATCH_CELL)^2 of them.
            let cells = (32 / PATCH_CELL) * (32 / PATCH_CELL);
            assert!(n_patch <= cells as u32, "{n_patch} patches, {cells} cells");
            for (i, q) in out.iter().enumerate() {
                let fade = q[3] & (3 << 18);
                let i = i as u32;
                let want = if i < n_fill {
                    FADE_NEAR
                } else if i < n_fill + n_grass {
                    FADE_GRASS
                } else {
                    FADE_FAR
                };
                assert_eq!(fade, want, "sprite {i}");
            }
        }
    }

    #[test]
    fn covered_grass_grows_nothing() {
        let mut v = meadow();
        for z in 0..PAD {
            for x in 0..PAD {
                v[pidx(x, MY + 8, z)] = PLANKS;
            }
        }
        let mut out = Vec::new();
        scatter(&v, false, None, [0, 0, 0], crate::world::gen::VOXEL, false, &|_, _| f32::MIN, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn pine_grows_level_branches_from_its_sides() {
        let mut v: Box<[u8; PAD3]> = vec![AIR; PAD3].into_boxed_slice().try_into().unwrap();
        for i in 0..8 {
            v[pidx(M + 4 * i, MY + 10, M + 5)] = PINE;
        }
        let mut out = Vec::new();
        scatter(&v, false, None, [0, 0, 0], crate::world::gen::VOXEL, false, &|_, _| f32::MIN, &mut out);
        assert!(!out.is_empty());
        for q in &out {
            assert_eq!(q[1] >> 8 & 255, SPRAY);
            let d = q[3] >> 8 & 7;
            assert!(d != 2 && d != 3, "a branch on face {d}");
        }
        // One flat and one upright card per branch.
        assert_eq!(out.iter().filter(|q| q[3] >> 11 & 1 == 0).count() * 2, out.len());
    }

    #[test]
    fn a_leaf_block_grows_clusters_outward() {
        let mut v: Box<[u8; PAD3]> = vec![AIR; PAD3].into_boxed_slice().try_into().unwrap();
        let mut out = Vec::new();
        // Many single leaf blocks, so the hash lets some faces through.
        for i in 0..8 {
            v[pidx(M + 4 * i, MY + 10, M + 5)] = LEAVES;
        }
        scatter(&v, false, None, [0, 0, 0], crate::world::gen::VOXEL, false, &|_, _| f32::MIN, &mut out);
        assert!(!out.is_empty());
        for q in &out {
            assert_eq!(q[1] >> 8 & 255, BILLBOARD);
            let y = (q[0] >> 20) as f32 / 16.0 - 2.0;
            assert!((9.5..=11.5).contains(&y), "cluster at {y}");
        }
    }
}

#[cfg(test)]
mod flower_tests {
    use super::*;

    /// Prairie grass should flower about 15% more than a plain meadow, counted
    /// over a big square with the meadow's own tall-grass noise.
    #[test]
    fn prairie_flower_share() {
        let (mut plain, mut prairie, mut n, mut fields) = (0u32, 0u32, 0u32, 0u32);
        let mut colours = [0u32; layer::COUNT as usize];
        for z in 0..1600 {
            for x in 0..1600 {
                let (fx, fz) = (x as f32, z as f32);
                let nn = perlin2(fx * 0.018, fz * 0.018, 501) * 0.7 + perlin2(fx * 0.07, fz * 0.07, 502) * 0.3;
                let tall = smoothstep(-0.15, 0.3, nn);
                let pick = unit(hash3(x, 0, z, 99));
                plain += flower(fx, fz, tall, pick, false).is_some() as u32;
                if let Some(l) = flower(fx, fz, tall, pick, true) {
                    prairie += 1;
                    colours[l as usize] += 1;
                }
                fields += (perlin2(fx * 0.009, fz * 0.009, 507) > FIELD.0) as u32;
                n += 1;
            }
        }
        let ratio = prairie as f32 / plain as f32;
        eprintln!(
            "plain {:.4} prairie {:.4} ratio {ratio:.3} field ground {:.3} colours w/b/y/r {:?}",
            plain as f32 / n as f32,
            prairie as f32 / n as f32,
            fields as f32 / n as f32,
            [3, 4, 16, 17].map(|l| colours[l])
        );
        assert!((1.08..1.25).contains(&ratio), "ratio {ratio}");
    }
}
