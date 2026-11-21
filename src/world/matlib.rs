//! Library materials at run time: ids 17..=254 get their faces, slope smoothing,
//! names and texture recipes from the library, installed before any chunk is
//! generated or meshed. Ids are given per world (`Library::assign_ids`), so a
//! world loaded later installs a new table; the texture layers (`recipes`) are
//! the same for every world, only the id -> layer map changes, and
//! `generation` tells the renderer to upload it again. Ids below 17 are the
//! built-in materials in material.rs, which the shaders also know by number.

use crate::world::flora::Form;
use super::material::{self, layer};
use crate::library::{Library, Recipe};
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::OnceLock;

pub struct MatTable {
    faces: [Option<[u32; 3]>; 256],
    smooth: [bool; 256],
    rounds: [bool; 256],
    facets: [bool; 256],
    /// Faceted only where stepped (`bevel_steps`).
    steps: [bool; 256],
    tubes: [bool; 256],
    crisp: [bool; 256],
    passable: [bool; 256],
    flags: [u32; 256],
    names: [Option<&'static str>; 256],
    /// Library names (the JSON file's "name"), for saves.
    keys: [Option<&'static str>; 256],
    /// Wood laid along X and along Z (`grained`): a copy of the material
    /// whose grain the shader turns to run along that axis. 0 where none.
    grain: [[u8; 2]; 256],
    /// The material a grain copy was made from (itself for every other id).
    base: [u8; 256],
    /// The multiply that takes this material's own texture to each end of
    /// its declared colour field (`drift`): [calm, classic], each the two
    /// ends. None where it declares none and takes its `tint` class's ramp.
    drift: [[Option<[[f32; 3]; 2]>; 256]; 2],
    /// How grass grows on this ground: [sparse, dense, short] from the
    /// material's `grass` block, where it has one.
    grass: [Option<[f32; 3]>; 256],
    /// Texture layers after the built-in ones, in layer order.
    pub recipes: Vec<Recipe>,
}

/// The installed table. Each install leaks a new one (a handful a session) so
/// readers on worker threads can hold `&'static` references without locks.
static TABLE: AtomicPtr<MatTable> = AtomicPtr::new(std::ptr::null_mut());
static GENERATION: AtomicU32 = AtomicU32::new(0);

fn tab() -> Option<&'static MatTable> {
    // SAFETY: only ever set to a leaked Box that is never freed.
    unsafe { TABLE.load(Ordering::Acquire).as_ref() }
}

/// Taken by any test that installs a table and then asks it questions.
///
/// There is one table for the process and `install` replaces it, so two
/// tests installing different worlds on two threads can each read the
/// other's ids. That is not theoretical: `mesh::sprites`'s lily-pad test
/// looks up the floating plant's id in Lowmouth's table and lays pads with
/// it, and about one full suite in eight it laid none, because the id it
/// held was something else by the time it was used. It read as a mesh bug
/// and it is a measurement bug.
///
/// Poisoning is ignored on purpose: a test that panics while holding this
/// has already failed, and the rest should still be able to run.
#[cfg(test)]
pub fn table_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bumped by every install: the renderer re-uploads its material map when
/// this changes.
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Acquire)
}

/// Whether a table is installed yet.
pub fn installed() -> bool {
    tab().is_some()
}

/// What a material does beyond its texture. The low byte goes to the shaders
/// (render/shaders.rs puts the same names in the WGSL prelude as M_*).
pub mod flag {
    /// Single texels flash in the sun (glass).
    pub const GLINTS: u32 = 1;
    /// Calm style: drifts between sage and ochre meadow over tens of metres.
    pub const CALM_MEADOW: u32 = 1 << 1;
    /// Calm style: leaf colour fields.
    pub const CALM_LEAF: u32 = 1 << 2;
    /// Classic style: large patches of drier and lusher growth.
    pub const CLASSIC_PATCH: u32 = 1 << 3;
    /// Grass grows on it (mesh/sprites.rs gives it a meadow): near the camera
    /// its top is painted as the grass's shaded understory (scene.wgsl).
    pub const UNDERSTORY: u32 = 1 << 4;
    /// Gives its own light: bits GLOW_SHIFT.. hold the strength in tenths.
    pub const GLOW_SHIFT: u32 = 10;
    /// Leaves and needles: the bike brushes through, a rope won't hold.
    pub const FOLIAGE: u32 = 1 << 8;
    /// Water drawn as solid voxels: walking creatures keep out.
    pub const WATER: u32 = 1 << 9;
    /// Wood lying along X or Z: the shader turns the texture so the grain
    /// (the texture's v) runs along that axis instead of up.
    pub const GRAIN_X: u32 = 1 << 5;
    pub const GRAIN_Z: u32 = 1 << 6;
    /// Rounded rock drawn in flat facets and lit like sloped ground.
    pub const FACET: u32 = 1 << 7;
    /// Glow only from the texture's blue-green specks (scene.wgsl).
    pub const GLOW_SPECKS: u32 = 1 << 16;
    /// Its sprites grow under the sea too (eelgrass): mesh/sprites.rs marks
    /// their cards so foliage.wgsl doesn't hide them at the waterline.
    pub const UNDERWATER: u32 = 1 << 17;
    /// Its sprites lie flat on the sea's surface over where they grow
    /// (lily pads).
    pub const FLOATING: u32 = 1 << 18;
    /// Rime rather than glass: a wide soft sheen with glitter in it, instead
    /// of single texels flashing (scene.wgsl).
    pub const GLINTS_SOFT: u32 = 1 << 19;
    /// A plant wears this as a leaf, so the season colours it
    /// (world/season.rs). Set for every material any library plant names as
    /// its leaf, its second leaf or its autumn colour; the built-in leaves
    /// and needles are found by FOLIAGE instead.
    pub const GROWS: u32 = 1 << 26;
    /// Its glow is lamplight: off by day (the glass dark), lit as the sun
    /// goes down (scene.wgsl).
    pub const NIGHT_GLOW: u32 = 1 << 27;
    /// Hurts to stand in: bits HURT_SHIFT.. hold damage a second, whole
    /// numbers, 0 to 63. Nothing in the shaders reads it; it is here because
    /// this is where a material's behaviour lives and the rules can then ask
    /// one question of the voxel under a body rather than carry a list.
    pub const HURT_SHIFT: u32 = 20;
    /// Fills only the lower half of its voxel (`MaterialDef::slab`).
    pub const SLAB: u32 = 1 << 28;
    /// Floods into one sheet of standing water after rain, not puddles
    /// (`MaterialDef::floods`).
    pub const FLOODS: u32 = 1 << 29;
}

/// The multiply that takes `base` to `want`, both sRGB bytes. Clamped to the
/// range the shader can carry (0 to 2), and 1 where the base is black and no
/// multiply would reach anywhere.
fn ratio(want: [f32; 3], base: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| if base[i] > 1.0 { (want[i] / base[i]).clamp(0.0, 2.0) } else { 1.0 })
}

/// The two ends of material `m`'s declared colour field, as multiplies, for
/// the calm style (`style` 0) or the classic one (1). None where it declares
/// none: the shader then takes the ramp its `tint` class gives it.
pub fn drift(m: u8, style: usize) -> Option<[[f32; 3]; 2]> {
    tab().and_then(|t| t.drift[style][m as usize])
}

/// The axis wood runs along, for `grained`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// The axis a direction runs most along.
    pub fn of(d: glam::Vec3) -> Axis {
        let a = d.abs();
        if a.y >= a.x && a.y >= a.z {
            Axis::Y
        } else if a.x >= a.z {
            Axis::X
        } else {
            Axis::Z
        }
    }

    /// The same axis after a structure is turned `rot` quarter turns.
    pub fn turned(self, rot: u8) -> Axis {
        match (self, rot & 1) {
            (Axis::X, 1) => Axis::Z,
            (Axis::Z, 1) => Axis::X,
            (a, _) => a,
        }
    }
}

/// Wood material `m` laid along `axis`: its grain copy, or `m` itself when it
/// has none (not wood) or the axis is Y (wood stands upright by default). A
/// grain copy asked for another axis goes back through its base.
pub fn grained(m: u8, axis: Axis) -> u8 {
    let Some(t) = tab().filter(|_| grain_on()) else {
        return m;
    };
    let b = t.base[m as usize];
    let pick = match axis {
        Axis::Y => return b,
        Axis::X => t.grain[b as usize][0],
        Axis::Z => t.grain[b as usize][1],
    };
    if pick == 0 {
        m
    } else {
        pick
    }
}

/// Grain copies in use, unless VOXEL_GRAIN=0 (for before/after comparisons).
fn grain_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_GRAIN").map(|v| v != "0").unwrap_or(true))
}

/// The axis a material's grain runs along (Y unless it is a grain copy).
pub fn grain_axis(m: u8) -> Axis {
    let f = flags(m);
    if f & flag::GRAIN_X != 0 {
        Axis::X
    } else if f & flag::GRAIN_Z != 0 {
        Axis::Z
    } else {
        Axis::Y
    }
}

/// The material a grain copy was made from; any other id is its own base.
pub fn base(m: u8) -> u8 {
    tab().map_or(m, |t| t.base[m as usize])
}

/// Whether wood of this material gets grain copies: the built-in barks, every
/// plant's bark, and library materials named like bark or wood.
pub fn wants_grain(lib: &Library, name: &str) -> bool {
    // Forms with no trunk or limbs (canes, herbs, cactus, kelp, pads) never
    // lay wood sideways, so their stems don't spend two ids on grain copies.
    let no_limbs = |f: Form| matches!(f, Form::Canes | Form::Herb | Form::Cactus | Form::Kelp | Form::Pads);
    name.ends_with("bark") || name.contains("wood") || lib.plants.iter().any(|p| p.bark == name && !no_limbs(p.form) && !p.form.is_coral())
}

/// Names a library material's `tint` may take.
pub const TINTS: [&str; 3] = ["meadow", "leaves", "needles"];

fn tint_flags(t: &str) -> u32 {
    match t {
        "meadow" => flag::CALM_MEADOW | flag::CLASSIC_PATCH,
        "leaves" => flag::CALM_LEAF | flag::CLASSIC_PATCH,
        "needles" => flag::CALM_LEAF,
        _ => 0,
    }
}

const fn builtin_flags(m: u8) -> u32 {
    use material::*;
    match m {
        GRASS => flag::CALM_MEADOW | flag::CLASSIC_PATCH | flag::UNDERSTORY,
        LEAVES | BIRCH_LEAVES => flag::CALM_LEAF | flag::CLASSIC_PATCH | flag::FOLIAGE,
        PINE => flag::CALM_LEAF | flag::FOLIAGE,
        LEAVES_AUTUMN => flag::FOLIAGE,
        _ => 0,
    }
}

/// A material's flags (see `flag`).
#[inline]
/// Damage a second for standing in material `m`, 0 for everything safe.
pub fn hurt(m: u8) -> f32 {
    ((flags(m) >> flag::HURT_SHIFT) & 63) as f32
}

pub fn flags(m: u8) -> u32 {
    if m < 17 {
        builtin_flags(m)
    } else {
        tab().map_or(0, |t| t.flags[m as usize])
    }
}

#[inline]
pub fn has(m: u8, f: u32) -> bool {
    flags(m) & f != 0
}

/// The built-in materials by library-style name.
const BUILTIN: [(&str, u8); 17] = {
    use material::*;
    [
        ("air", AIR),
        ("grass", GRASS),
        ("dirt", DIRT),
        ("sand", SAND),
        ("stone", STONE),
        ("stone_dark", STONE_DARK),
        ("stone_light", STONE_LIGHT),
        ("bark", BARK),
        ("leaves", LEAVES),
        ("pine", PINE),
        ("planks", PLANKS),
        ("gravel", GRAVEL),
        ("birch_bark", BIRCH_BARK),
        ("leaves_autumn", LEAVES_AUTUMN),
        ("snow", SNOW),
        ("birch_leaves", BIRCH_LEAVES),
        ("brick", BRICK),
    ]
};

/// Built-in material ids by library-style name.
pub fn builtin_id(key: &str) -> Option<u8> {
    BUILTIN.iter().find(|b| b.0 == key).map(|b| b.1)
}

/// A material's stable name: the built-in key or the library entry's name.
/// Saves store these rather than ids, which may be renumbered.
pub fn key(m: u8) -> Option<&'static str> {
    BUILTIN.iter().find(|b| b.1 == m).map(|b| b.0).or_else(|| tab().and_then(|t| t.keys[m as usize]))
}

/// The id of a material by its stable name (see `key`).
pub fn id_of(key: &str) -> Option<u8> {
    builtin_id(key).or_else(|| tab().and_then(|t| t.keys.iter().position(|k| *k == Some(key)).map(|i| i as u8)))
}

/// Install the library's materials with the ids `Library::assign_ids` gave
/// them, replacing any table installed before (the table is global because the
/// mesher and generator ask about materials from worker threads).
pub fn install(lib: &Library) {
    let mut t = MatTable {
        faces: [None; 256],
        smooth: [false; 256],
        rounds: [false; 256],
        facets: [false; 256],
        steps: [false; 256],
        tubes: [false; 256],
        crisp: [false; 256],
        passable: [false; 256],
        flags: [0; 256],
        names: [None; 256],
        keys: [None; 256],
        grain: [[0; 2]; 256],
        base: std::array::from_fn(|i| i as u8),
        drift: [[None; 256]; 2],
        grass: [None; 256],
        recipes: Vec::new(),
    };
    let mut next = layer::COUNT;
    // Only the materials this world has an id for get texture layers: a
    // library of thousands then still costs a world only the few dozen
    // recipes it can draw (`Library::assign_ids` leaves the rest at id 0).
    for m in &lib.materials {
        let i = m.id as usize;
        if !(17..255).contains(&i) {
            continue;
        }
        let side = next;
        t.recipes.push(m.texture.clone());
        next += 1;
        let top = match &m.top {
            Some(r) => {
                t.recipes.push(r.clone());
                next += 1;
                next - 1
            }
            None => side,
        };
        let bottom = match &m.bottom {
            Some(r) => {
                t.recipes.push(r.clone());
                next += 1;
                next - 1
            }
            None => top,
        };
        t.faces[i] = Some([top, side, bottom]);
        t.smooth[i] = m.smooth;
        t.rounds[i] = m.round || m.facet || m.bevel_steps;
        t.facets[i] = m.facet || m.bevel_steps;
        t.steps[i] = m.bevel_steps;
        t.crisp[i] = m.crisp;
        t.passable[i] = m.passable;
        t.grass[i] = m.grass.as_ref().map(|g| [g.sparse, g.dense, g.short]);
        if let Some(d) = &m.drift {
            // Against the texture the style actually draws: the top face
            // where the material has one (ground is seen from above), and
            // the calm recipe's own colours in the calm style.
            let face = m.top.as_ref().unwrap_or(&m.texture);
            let (calm, classic) = d.pairs();
            let calm_base = face.calm.as_ref().map_or(face.base, |c| c.base);
            t.drift[0][i] = calm.map(|p| p.map(|c| ratio(c, calm_base)));
            t.drift[1][i] = classic.map(|p| p.map(|c| ratio(c, face.base)));
        }
        // A material generated for a plant's coat answers to its base
        // material's name: the sprite rules, the foliage and glints
        // defaults and the grain all read a name, and a recoloured wisteria
        // is still wisteria. Its key stays its own, since saves store keys.
        let named = m.coat_of.as_ref().unwrap_or(&m.name);
        t.names[i] = Some(Box::leak(named.to_uppercase().replace('_', " ").into_boxed_str()));
        t.keys[i] = Some(Box::leak(m.name.clone().into_boxed_str()));
        let lname = named.to_ascii_lowercase();
        let glinty = m.glints.unwrap_or(lname.contains("glass"));
        // Only glass flashes facet by facet; frost and ice get the soft sheen.
        let hard = lname.contains("glass");
        t.flags[i] = if glinty { flag::GLINTS } else { 0 }
            | if glinty && !hard { flag::GLINTS_SOFT } else { 0 }
            | m.tint.as_deref().map_or(0, tint_flags)
            | if m.water { flag::WATER } else { 0 }
            | if m.facet { flag::FACET } else { 0 }
            | if m.glow_specks { flag::GLOW_SPECKS } else { 0 }
            | if m.night_glow { flag::NIGHT_GLOW } else { 0 }
            | if lname.contains("grass") { flag::UNDERSTORY } else { 0 }
            | if m.underwater { flag::UNDERWATER } else { 0 }
            | if m.floating { flag::FLOATING } else { 0 }
            | if m.slab { flag::SLAB } else { 0 }
            | if m.floods { flag::FLOODS } else { 0 }
            | ((m.glow * 10.0).round().clamp(0.0, 63.0) as u32) << flag::GLOW_SHIFT
            | (m.hurt.round().clamp(0.0, 63.0) as u32) << flag::HURT_SHIFT;
    }
    // Plant materials are rounded into blobs by the mesher, like the built-in trees.
    for p in lib.plants.iter().filter(|p| p.form != Form::Vine) {
        for name in [Some(&p.bark), p.leaf.as_ref(), p.leaf_alt.as_ref().map(|x| &x.0), p.autumn.as_ref(), p.flower.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(id) = lib.material(name) {
                if (17..255).contains(&(id as usize)) {
                    t.rounds[id as usize] = true;
                }
            }
        }
    }
    // Every material a plant wears as a leaf, so the season can colour it
    // the way it colours the built-in leaves (world/season.rs).
    for p in &lib.plants {
        for name in [p.leaf.as_ref(), p.leaf_alt.as_ref().map(|x| &x.0), p.autumn.as_ref()].into_iter().flatten() {
            if let Some(id) = lib.material(name) {
                if (17..255).contains(&(id as usize)) {
                    t.flags[id as usize] |= flag::GROWS;
                }
            }
        }
    }
    // Thin stems are bevelled, not rounded: the leaf of a vine and the stem
    // of bamboo, canes and kelp, unless the material says otherwise.
    for p in &lib.plants {
        // A vine's autumn colour is a stem as much as its summer one, or a
        // creeper would change shape as well as colour when the year turns.
        let stem: [Option<&String>; 2] = match p.form {
            Form::Vine => [p.leaf.as_ref(), p.autumn.as_ref()],
            Form::Bamboo | Form::Canes | Form::Kelp => [Some(&p.bark), None],
            // Coral limbs are a voxel or two thick: bevelled, or the
            // rounding pass blurs them away.
            f if f.is_coral() => [Some(&p.bark), None],
            _ => [None, None],
        };
        for id in stem.into_iter().flatten().filter_map(|n| lib.material(n)) {
            if (17..255).contains(&(id as usize)) {
                t.tubes[id as usize] = true;
            }
        }
    }
    for m in &lib.materials {
        let i = m.id as usize;
        if (17..255).contains(&i) {
            if let Some(b) = m.tube {
                t.tubes[i] = b;
            }
            if t.tubes[i] {
                t.rounds[i] = false;
            }
        }
    }
    // Foliage defaults to rounded plant material named like leaves.
    for m in &lib.materials {
        let i = m.id as usize;
        if !(17..255).contains(&i) {
            continue;
        }
        let lname = m.coat_of.as_ref().unwrap_or(&m.name).to_ascii_lowercase();
        let leafy = t.rounds[i] && ["leaf", "leaves", "needle", "frond", "canopy", "bush", "shrub"].iter().any(|k| lname.contains(k));
        if m.foliage.unwrap_or(leafy) {
            t.flags[i] |= flag::FOLIAGE;
        }
    }
    // Grain copies of every wood, along X and along Z, on ids the library
    // leaves free. Saves name them "<key>@x" / "<key>@z", so their numbers
    // may change when the library does.
    let mut woods: Vec<(u8, &'static str)> = vec![(material::BARK, "bark"), (material::BIRCH_BARK, "birch_bark")];
    for m in &lib.materials {
        let reached = lib.reach.as_ref().is_none_or(|r| r.contains(&m.name));
        if (17..255).contains(&(m.id as usize)) && reached && wants_grain(lib, m.coat_of.as_ref().unwrap_or(&m.name)) {
            woods.push((m.id as u8, t.keys[m.id as usize].unwrap_or("")));
        }
    }
    let free: Vec<usize> = (17..255usize).filter(|&i| t.keys[i].is_none() && t.faces[i].is_none()).collect();
    let mut free = free.into_iter();
    'woods: for (b, key) in woods {
        let bi = b as usize;
        for (k, (suffix, gflag)) in [("x", flag::GRAIN_X), ("z", flag::GRAIN_Z)].into_iter().enumerate() {
            let Some(i) = free.next() else {
                // With no world chosen every wood in the library is counted
                // as reached and there are never ids for them all; nothing is
                // built from that table, so it is not worth a line.
                if lib.reach.is_some() {
                    eprintln!("materials: no free id left for the grain of {key}");
                }
                break 'woods;
            };
            if b < 17 {
                t.faces[i] = Some(material::faces(b));
                t.rounds[i] = true;
                t.names[i] = Some(material::name(b));
                t.flags[i] = builtin_flags(b) | gflag;
            } else {
                t.faces[i] = t.faces[bi];
                t.smooth[i] = t.smooth[bi];
                t.rounds[i] = t.rounds[bi];
                t.facets[i] = t.facets[bi];
                t.steps[i] = t.steps[bi];
                t.tubes[i] = t.tubes[bi];
                t.crisp[i] = t.crisp[bi];
                t.passable[i] = t.passable[bi];
                t.names[i] = t.names[bi];
                t.flags[i] = t.flags[bi] | gflag;
            }
            t.keys[i] = Some(Box::leak(format!("{key}@{suffix}").into_boxed_str()));
            t.base[i] = b;
            t.grain[bi][k] = i as u8;
        }
    }
    TABLE.store(Box::into_raw(Box::new(t)), Ordering::Release);
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub fn table() -> Option<&'static MatTable> {
    tab()
}

pub fn faces(m: u8) -> Option<[u32; 3]> {
    tab().and_then(|t| t.faces.get(m as usize).copied().flatten())
}

pub fn smooth(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.smooth.get(m as usize).copied().unwrap_or(false))
}

pub fn rounds(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.rounds.get(m as usize).copied().unwrap_or(false))
}

/// Rounded in flat facets (see flag::FACET).
/// Faceted only where the surface is stepped (see `Material::bevel_steps`).
pub fn steps_only(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.steps.get(m as usize).copied().unwrap_or(false))
}

pub fn facets(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.facets.get(m as usize).copied().unwrap_or(false))
}

pub fn tubes(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.tubes.get(m as usize).copied().unwrap_or(false))
}

/// Ground drawn with a sharp border against other ground (no noisy blend).
pub fn crisp(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.crisp.get(m as usize).copied().unwrap_or(false))
}

/// Does material `m` fill only the lower half of its voxel?
#[inline]
pub fn slab(m: u8) -> bool {
    m >= 17 && has(m, flag::SLAB)
}

/// Thin growth bodies pass through (store.rs `solid`).
#[inline]
pub fn passable(m: u8) -> bool {
    m >= 17 && tab().is_some_and(|t| t.passable[m as usize])
}

/// How grass grows on ground `m`: [sparse, dense, short], where its file says.
pub fn grass(m: u8) -> Option<[f32; 3]> {
    (m >= 17).then(|| tab().and_then(|t| t.grass[m as usize])).flatten()
}

pub fn name(m: u8) -> Option<&'static str> {
    tab().and_then(|t| t.names.get(m as usize).copied().flatten())
}

/// Every texture layer: the built-in ones plus one per library recipe.
pub fn layer_count() -> u32 {
    layer::COUNT + tab().map_or(0, |t| t.recipes.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Drift;

    #[test]
    fn a_drift_becomes_the_multiply_that_reaches_it() {
        // The declared colour divided by the texture's own, which is what
        // lets an author write the colours they want to see rather than
        // working out a multiplier against a pattern they cannot picture.
        let m = ratio([166.0, 152.0, 110.0], [146.0, 148.0, 116.0]);
        for (got, want) in m.iter().zip([1.137, 1.027, 0.948]) {
            assert!((got - want).abs() < 0.002, "{m:?}");
        }
        // Clamped to what the shader can carry, and left alone where the
        // texture is black and no multiply would reach anywhere.
        assert_eq!(ratio([255.0, 0.0, 128.0], [10.0, 200.0, 0.0]), [2.0, 0.0, 1.0]);
    }

    #[test]
    fn a_drift_is_declared_for_both_styles_or_each() {
        let both: Drift = serde_json::from_str("[[1, 2, 3], [4, 5, 6]]").unwrap();
        let (c, k) = both.pairs();
        assert_eq!(c.unwrap()[1], [4.0, 5.0, 6.0]);
        assert_eq!(k.unwrap()[0], [1.0, 2.0, 3.0]);
        let one: Drift = serde_json::from_str(r#"{"classic": [[1, 2, 3], [4, 5, 6]]}"#).unwrap();
        let (c, k) = one.pairs();
        assert!(c.is_none(), "a style with no pair keeps its class ramp");
        assert!(k.is_some());
    }
}
