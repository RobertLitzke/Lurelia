//! Building one edited entry into something the engine can draw, without
//! touching what the world already loaded.
//!
//! A creature's models, species and palette block are appended after the
//! world's own (`Studio::load_creature`), so stepping a knob costs one model
//! build rather than a reload, and dropping the subject is a truncate: no
//! entity but the subject's own ever refers to them.
//!
//! This mirrors the per-creature part of `worldload::creatures`. The two are
//! kept apart on purpose: the loader builds every creature of a world at once,
//! while the editor builds one at a time, over and over, and must leave the
//! world's own models alone.

use crate::creatures::Species;
use crate::library::{CreatureDef, Library, PlantDef};
use crate::models::{self, Model};
use glam::Vec3;
use std::collections::HashMap;

/// One built subject: its models (the whole animal first, then the part
/// models its rig and skeleton point at), the species row that drives it, and
/// the palette colours it needs appended to the world's.
pub struct Built {
    pub models: Vec<Model>,
    pub species: Species,
    pub colours: Vec<[f32; 4]>,
    pub problems: Vec<String>,
    /// How far across the model actually reaches, metres: the widest of its
    /// three dimensions. The collision height is no guide to this -- a crow
    /// stands a hand high and spans a metre of wing -- and the studio's
    /// camera frames on it.
    pub span: f32,
    /// Which slot of its palette block each colour name was given, sorted by
    /// name. A colour edit can repaint the palette and leave the model
    /// alone, but only while this has not moved: identical colours share a
    /// slot, so one of them changing can shift every slot after it.
    pub paint: Vec<(String, u8)>,
}

/// The palette block an entry wants: its colours, which slot of the block
/// each name takes, and the two bone slots a skeleton gets.
pub struct Paint {
    pub colours: Vec<[f32; 4]>,
    pub slot: HashMap<String, u8>,
    pub bone: u8,
    pub bone_dark: u8,
}

impl Paint {
    /// The slots by name, sorted, for comparing one build against another.
    pub fn slots(&self) -> Vec<(String, u8)> {
        let mut out: Vec<(String, u8)> = self.slot.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.sort();
        out
    }
}

/// Work out an entry's colours without building anything else, so painting
/// one need not rebuild the model.
pub fn paint_of(def: &CreatureDef) -> Paint {
    let mut colours: Vec<[f32; 4]> = Vec::new();
    let mut slot: HashMap<String, u8> = HashMap::new();
    let mut names: Vec<&String> = def.colors.keys().collect();
    names.sort();
    for name in names {
        let s = models::block_slot(&mut colours, 0, models::linear_colour(&def.colors[name]));
        slot.insert(name.clone(), s);
    }
    // Bone and shadowed bone, the same two colours the loader puts at the
    // end of the block of a creature with a skeleton. Here they are always
    // there, whether a skeleton comes of it or not.
    let bone = models::block_slot(&mut colours, 0, models::linear_colour(&[238.0, 232.0, 214.0]));
    let bone_dark = models::block_slot(&mut colours, 0, models::linear_colour(&[186.0, 176.0, 152.0]));
    Paint { colours, slot, bone, bone_dark }
}

/// The species row an entry asks for, with the model it is already drawn by
/// and whether that model has bones. Nothing here touches the model, so a
/// behaviour or variety edit can patch this in place.
pub fn row(def: &CreatureDef, lib: &Library, model: usize, index: usize, bones: bool) -> Species {
    let mut ages = crate::creatures::age::Stages::resolve(&def.ages, def.height);
    ages.bones &= bones;
    Species {
        name: def.name.clone(),
        model,
        beh: def.behaviour.clone(),
        flees: def.behaviour.flees_from.iter().filter_map(|n| lib.creature(n)).collect(),
        hunts: def.behaviour.hunts.iter().filter_map(|n| lib.creature(n)).collect(),
        flees_player: def.behaviour.flees_from.iter().any(|n| n == "player"),
        like: def.like.clone(),
        quota: index,
        water: None,
        is_water_form: false,
        vary: def.vary.clone(),
        coats: crate::variety::wears(&crate::library::coats_of(&def.morphs), &def.colors),
        // The editor shows the entry in front of you, never a rare morph
        // standing in for it.
        morphs: Vec::new(),
        ages,
        aged_tint: crate::variety::aged_tint(&def.colors.values().map(|v| [0, 1, 2].map(|i| models::linear_colour(v)[i])).collect::<Vec<_>>()),
        forms: Default::default(),
    }
}

/// Build `def` as a species of its own. `model_base` is the index its first
/// model will take in `Game::models`, `block` where its colours will start in
/// the palette (`Studio::block`), and `index` the species index it will take
/// (its own quota).
pub fn creature(def: &CreatureDef, lib: &Library, model_base: usize, block: u32, index: usize) -> Result<Built, String> {
    let mut problems: Vec<String> = Vec::new();
    let paint = paint_of(def);
    let (bone, bone_dark) = (paint.bone, paint.bone_dark);
    problems.extend(models::block_problem(&def.name, paint.colours.len()));
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        paint.slot.get(n).copied()
    };
    let dims = crate::anim::slice::Dims(def.grid);
    let s = crate::world::shape::compile(&def.ops, def.grid, &resolve);
    if s.solid == 0 {
        return Err(format!("{}: the ops fill nothing", def.name));
    }
    let mut model = models::from_grid(&s.mat, dims, def.voxel, def.half_w, def.height);
    let mut parts: Vec<Model> = Vec::new();
    let stages = crate::creatures::age::Stages::resolve(&def.ages, def.height);
    let mut has_bones = false;
    if let Some(rig_def) = def.rig.as_ref() {
        let sliced = crate::anim::slice::slice(rig_def, &def.ops, &s.mat, dims, &|n| resolve(n).is_some(), def.voxel, def.half_w, def.height);
        problems.extend(sliced.oversize.iter().map(|b| crate::anim::slice::oversize_problem(&def.name, b)));
        let base = sliced.rig.clone();
        let skeleton = stages
            .bones
            .then(|| crate::anim::bones::skeletonise(&sliced, bone, bone_dark, def.voxel, def.half_w, def.height))
            .and_then(|(cells, built)| {
                let n = crate::anim::bones::voxels(&cells);
                (n >= crate::anim::bones::MIN_BONE_VOXELS).then(|| (crate::anim::bones::hulls(&sliced, &cells), built))
            });
        let mut rig = base.clone();
        rig.offset_models(model_base + 1 + parts.len());
        parts.extend(sliced.parts);
        model.rig = Some(rig);
        if let Some((hulls, bones)) = skeleton {
            let mut srig = base;
            // Bones do not sag, and lie on bone rather than on flesh.
            srig.soft = None;
            crate::anim::bones::set_hulls(&mut srig, hulls);
            srig.offset_models(model_base + 1 + parts.len());
            parts.extend(bones);
            model.skeleton = Some(srig);
            has_bones = true;
        }
    }
    let species = row(def, lib, model_base, index, has_bones);
    let mut out = vec![model];
    out.extend(parts);
    for m in &mut out {
        m.block = block;
    }
    let slots = paint.slots();
    Ok(Built {
        models: out,
        species,
        colours: paint.colours,
        problems,
        span: span_of(&s, def.voxel),
        paint: slots,
    })
}

/// One model standing on the platform with nothing driving it: a found
/// thing, a gun. It has no species, so none of a creature's machinery
/// applies.
pub struct Prop {
    pub models: Vec<Model>,
    pub colours: Vec<[f32; 4]>,
    pub problems: Vec<String>,
    /// How far it reaches across and how tall it stands, metres.
    pub span: f32,
    pub tall: f32,
}

/// Build a found thing: one model from its ops, in colours of its own. It
/// has no species and nothing drives it, so this is the creature build with
/// everything but the model taken out (`worldload::creatures` builds the
/// world's own the same way).
/// `block` is where its colours will start in the palette.
pub fn collectible(def: &crate::library::CollectibleDef, block: u32) -> Result<Prop, String> {
    let mut colours: Vec<[f32; 4]> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut slot: HashMap<String, u8> = HashMap::new();
    let mut names: Vec<&String> = def.colors.keys().collect();
    names.sort();
    for name in names {
        let s = models::block_slot(&mut colours, 0, models::linear_colour(&def.colors[name]));
        slot.insert(name.clone(), s);
    }
    problems.extend(models::block_problem(&def.name, colours.len()));
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        slot.get(n).copied()
    };
    let s = crate::world::shape::compile(&def.ops, [32, 32, 32], &resolve);
    if s.solid == 0 {
        return Err(format!("{}: the ops fill nothing", def.name));
    }
    let span = span_of(&s, def.voxel);
    let tall = (s.top.max(1) as f32 * def.voxel).max(0.02);
    let mut model = models::from_cells(&s.mat, def.voxel, span * 0.5, tall);
    model.block = block;
    Ok(Prop {
        models: vec![model],
        colours,
        problems,
        span,
        tall,
    })
}

/// How far the compiled shape reaches across, metres: the widest of its
/// solid extent in x, y and z.
fn span_of(s: &crate::world::shape::Shape, voxel: f32) -> f32 {
    let [sx, sy, sz] = s.size;
    let (mut lo_x, mut hi_x, mut lo_z, mut hi_z) = (sx, -1, sz, -1);
    for z in 0..sz {
        for y in 0..sy {
            for x in 0..sx {
                if s.idx(x, y, z).is_some_and(|i| s.mat[i] != 0) {
                    lo_x = lo_x.min(x);
                    hi_x = hi_x.max(x);
                    lo_z = lo_z.min(z);
                    hi_z = hi_z.max(z);
                }
            }
        }
    }
    let wide = (hi_x - lo_x + 1).max(hi_z - lo_z + 1).max(s.top).max(1);
    wide as f32 * voxel
}

/// Grow one plant from an edited entry at `base`, as `flora::place_plants`
/// would: the same roll decides its shape and, where the entry comes in
/// several coats, which one this individual wears. `pi` is the entry's place
/// in the library, which is what the coats are keyed by.
/// The season it is in is put on later, by `flora::dress`, on its way into
/// the column like any other plant.
pub fn plant(def: &PlantDef, lib: &Library, pi: usize, base: Vec3, seed: u32) -> crate::world::gen::Tree {
    let worn = crate::world::flora::coat_at(lib, pi, None, base.x, base.z, seed);
    crate::world::flora::grow(lib, def, base, seed, worn)
}
