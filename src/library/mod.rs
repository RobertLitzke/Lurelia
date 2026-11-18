//! The shared library every world draws from: materials (with texture recipes),
//! plants, structures, creatures and biomes, one JSON file per entry under
//! `library/<kind>/`. Entries are data ("nouns") that choose among behaviours
//! written in Rust ("verbs"): texture patterns, tree forms, placement rules,
//! movement styles. A world adds entries here rather than keeping its own, so each
//! world enriches the pool for the next. Entries are versioned and never edited
//! in place once a world uses them; a change is a new version.

use crate::world::flora::Form;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where the library and worlds live: next to the working directory if present,
/// else in the source tree.
pub fn data_root() -> PathBuf {
    let here = PathBuf::from("library");
    if here.is_dir() {
        return PathBuf::from(".");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn default_arc_keep() -> f32 {
    0.5
}

fn default_bounce_keep() -> f32 {
    0.45
}

fn default_one() -> f32 {
    1.0
}
fn default_version() -> u32 {
    1
}

fn default_fringe() -> f32 {
    3.0
}

mod material;
mod plant;
mod shape;
mod creature;
mod look;
mod collectible;
mod weapon;
mod biome;
mod check;

pub use material::*;
pub use plant::*;
pub use shape::*;
pub use creature::*;
pub use look::*;
pub use collectible::*;
pub use weapon::*;
pub use biome::*;

// ---- the library --------------------------------------------------------------

#[derive(Default)]
pub struct Library {
    pub materials: Vec<MaterialDef>,
    pub plants: Vec<PlantDef>,
    pub structures: Vec<StructureDef>,
    /// Route kits: cross-sections for roads, fences, bridges (world/route).
    pub kits: Vec<crate::world::route::kit::KitDef>,
    /// City kits: what an imported city is made of, and what to make of it
    /// instead (world/city.rs `Kit`).
    pub cities: Vec<crate::world::city::Kit>,
    pub creatures: Vec<CreatureDef>,
    pub biomes: Vec<BiomeDef>,
    pub collectibles: Vec<CollectibleDef>,
    /// How signs look (world/places.rs).
    pub signs: Vec<crate::world::places::SignStyle>,
    /// The guns a shooter world may hand out (src/combat).
    pub weapons: Vec<WeaponDef>,
    /// Particle effects: dust, splashes, chips, falling leaves (src/fx).
    pub fx: Vec<crate::fx::FxDef>,
    /// The library names the world being built reaches (`worldload::reach`),
    /// or None when no world is known: then every material counts.
    pub reach: Option<std::collections::HashSet<String>>,
    /// Name -> index for each kind (materials: name -> id), built once at
    /// load: worldgen looks names up for every tree and chunk.
    index: Index,
}

#[derive(Default)]
struct Index {
    materials: HashMap<String, u8>,
    plants: HashMap<String, usize>,
    structures: HashMap<String, usize>,
    kits: HashMap<String, usize>,
    cities: HashMap<String, usize>,
    creatures: HashMap<String, usize>,
    biomes: HashMap<String, usize>,
    collectibles: HashMap<String, usize>,
    signs: HashMap<String, usize>,
    weapons: HashMap<String, usize>,
}

/// Name -> position, keeping the first of any duplicates (validate reports those).
fn positions<'a>(names: impl Iterator<Item = &'a String>) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, n) in names.enumerate() {
        m.entry(n.clone()).or_insert(i);
    }
    m
}

/// Like `load_dir`, but entries may name a parent with `like` (src/inherit.rs).
fn load_dir_inherit<T: for<'de> Deserialize<'de>>(dir: &Path, errors: &mut Vec<String>) -> Vec<(String, T)> {
    let raw: Vec<(String, serde_json::Value)> = load_dir(dir, errors);
    crate::inherit::resolve(raw, errors)
        .into_iter()
        .filter_map(|(file, v)| match parse_known::<T>(v, |k| errors.push(format!("{}: unknown key {k}", dir.join(&file).display()))) {
            Ok(t) => Some((file, t)),
            Err(e) => {
                errors.push(format!("{}: {e}", dir.join(&file).display()));
                None
            }
        })
        .collect()
}

/// Deserialize `v` as `T`, passing the path of every key `T` doesn't know to
/// `unknown`. Without this a misspelt key ("weathring") silently takes its
/// default. `notes` (read by tools/libfmt.py) and keys starting with `_` are
/// comments and pass. Keys inside internally tagged enums aren't seen here;
/// `Op` refuses them itself (`deny_unknown_fields`).
pub fn parse_known<T: serde::de::DeserializeOwned>(v: serde_json::Value, mut unknown: impl FnMut(String)) -> Result<T, serde_json::Error> {
    serde_ignored::deserialize(v, |path| {
        let p = path.to_string();
        let last = p.rsplit('.').next().unwrap_or(&p);
        if last != "notes" && !last.starts_with('_') {
            unknown(p);
        }
    })
}

fn load_dir<T: for<'de> Deserialize<'de>>(dir: &Path, errors: &mut Vec<String>) -> Vec<(String, T)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    for p in paths {
        let file = p.file_name().unwrap().to_string_lossy().to_string();
        match std::fs::read_to_string(&p) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text).and_then(|v| parse_known::<T>(v, |k| errors.push(format!("{}: unknown key {k}", p.display())))) {
                Ok(v) => out.push((file, v)),
                Err(e) => errors.push(format!("{}: {e}", p.display())),
            },
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    out
}

impl Library {
    /// A library holding nothing but these materials, for code that needs a
    /// material list in a `Library` (the EDIT page: `matlib::install` takes
    /// one, and the page's knobs read the names).
    pub fn of_materials(materials: Vec<MaterialDef>) -> Library {
        Library { materials, ..Default::default() }
    }

    /// Load every entry under `root/library`. Entries that fail to parse are
    /// reported in the returned list and skipped.
    pub fn load(root: &Path) -> (Library, Vec<String>) {
        let lib_dir = root.join("library");
        let mut errors = Vec::new();
        let mut lib = Library {
            index: Index::default(),
            reach: None,
            materials: load_dir::<MaterialDef>(&lib_dir.join("materials"), &mut errors).into_iter().map(|x| x.1).collect(),
            plants: load_dir_inherit::<PlantDef>(&lib_dir.join("plants"), &mut errors).into_iter().map(|x| x.1).collect(),
            structures: load_dir::<StructureDef>(&lib_dir.join("structures"), &mut errors).into_iter().map(|x| x.1).collect(),
            kits: load_dir::<crate::world::route::kit::KitDef>(&lib_dir.join("kits"), &mut errors).into_iter().map(|x| x.1).collect(),
            // City kits inherit (`"like": "modern"`), so a theme is a short file.
            cities: load_dir_inherit::<crate::world::city::Kit>(&lib_dir.join("cities"), &mut errors).into_iter().map(|x| x.1).collect(),
            creatures: load_dir_inherit::<CreatureDef>(&lib_dir.join("creatures"), &mut errors).into_iter().map(|x| x.1).collect(),
            biomes: load_dir::<BiomeDef>(&lib_dir.join("biomes"), &mut errors).into_iter().map(|x| x.1).collect(),
            collectibles: load_dir::<CollectibleDef>(&lib_dir.join("collectibles"), &mut errors).into_iter().map(|x| x.1).collect(),
            signs: load_dir::<crate::world::places::SignStyle>(&lib_dir.join("signs"), &mut errors).into_iter().map(|x| x.1).collect(),
            weapons: load_dir::<WeaponDef>(&lib_dir.join("weapons"), &mut errors).into_iter().map(|x| x.1).collect(),
            fx: load_dir::<crate::fx::FxDef>(&lib_dir.join("fx"), &mut errors).into_iter().map(|x| x.1).collect(),
        };
        lib.index = Index {
            materials: HashMap::new(),
            plants: positions(lib.plants.iter().map(|x| &x.name)),
            structures: positions(lib.structures.iter().map(|x| &x.name)),
            kits: positions(lib.kits.iter().map(|x| &x.name)),
            cities: positions(lib.cities.iter().map(|x| &x.name)),
            creatures: positions(lib.creatures.iter().map(|x| &x.name)),
            biomes: positions(lib.biomes.iter().map(|x| &x.name)),
            collectibles: positions(lib.collectibles.iter().map(|x| &x.name)),
            signs: positions(lib.signs.iter().map(|x| &x.name)),
            weapons: positions(lib.weapons.iter().map(|x| &x.name)),
        };
        lib.coat_materials();
        lib.assign_ids(None);
        (lib, errors)
    }

    /// Give materials voxel ids (17..254) for one world. Voxels are a byte, so
    /// the library as a whole may hold more materials than there are ids; what
    /// must fit is what one world reaches (`reach`, all of them when None).
    /// Reached materials keep the id their file asks for, then the rest of the
    /// reached ones take the lowest free ids, then room is kept for the grain
    /// copies of the reached woods (`matlib::install`), and only then do the
    /// unreached materials get ids, their own where free, while any are left.
    /// Returns the reached materials left without an id.
    pub fn assign_ids(&mut self, reach: Option<std::collections::HashSet<String>>) -> Vec<String> {
        let mut taken = [false; 256];
        let reached: Vec<bool> = self.materials.iter().map(|m| reach.as_ref().is_none_or(|r| r.contains(&m.name))).collect();
        let mut ids = vec![0u8; self.materials.len()];
        let own = |m: &MaterialDef| m.file_id.filter(|i| (17..255).contains(i)).map(|i| i as usize);
        // Reached: own ids, then free ones.
        for (k, m) in self.materials.iter().enumerate() {
            if let Some(i) = own(m).filter(|&i| reached[k] && !taken[i]) {
                taken[i] = true;
                ids[k] = i as u8;
            }
        }
        let mut missing = Vec::new();
        for (k, m) in self.materials.iter().enumerate() {
            if reached[k] && ids[k] == 0 {
                match (17..255usize).find(|&i| !taken[i]) {
                    Some(i) => {
                        taken[i] = true;
                        ids[k] = i as u8;
                    }
                    None => missing.push(m.name.clone()),
                }
            }
        }
        // A material the world cannot reach keeps no id at all: it is never
        // drawn here, so it costs neither an id nor a texture layer
        // (`matlib::install` skips it), and the ids it leaves go to the
        // reached woods' grain copies. A world with no chronicle (the random
        // island) reaches everything, as before.
        for (m, id) in self.materials.iter_mut().zip(ids) {
            m.id = id;
        }
        self.index.materials = self.materials.iter().rev().filter(|m| m.id != 0).map(|m| (m.name.clone(), m.id)).collect();
        self.reach = reach;
        missing
    }

    /// Material id by name: library materials first, then the built-in ones.
    pub fn material(&self, name: &str) -> Option<u8> {
        if name == "air" {
            return Some(0);
        }
        self.index.materials.get(name).copied().or_else(|| crate::world::matlib::builtin_id(name))
    }

    /// Rare morphs of plant `pi`: (plant index, chance).
    pub fn plant_morphs(&self, pi: usize) -> Vec<(usize, f32)> {
        let name = &self.plants[pi].name;
        self.plants
            .iter()
            .enumerate()
            .filter(|(_, p)| p.rarity > 0.0 && p.like.as_ref() == Some(name))
            .map(|(i, p)| (i, p.rarity))
            .collect()
    }

    /// Give every coat of every plant the materials it needs. A creature's
    /// coat is a tint over its model, which costs nothing; a plant's colour
    /// is a texture, so a coat that paints a slot needs a material of its
    /// own, generated here from the plant's own one with the recipe's colours
    /// carried by the same multiply, so the pattern stays the pattern.
    ///
    /// A coat that names an existing material in a slot generates nothing:
    /// that material is used as it is (what `leaf_alt` always did).
    fn coat_materials(&mut self) {
        let mut made: Vec<MaterialDef> = Vec::new();
        for pi in 0..self.plants.len() {
            let coats = self.plant_coats(pi);
            let plant = self.plants[pi].name.clone();
            for c in &coats {
                for (slot, paint) in &c.paint {
                    let Some((lo, hi)) = paint.ends() else { continue };
                    let Some(base) = self.plant_slot(pi, slot) else { continue };
                    let Some(src) = self.materials.iter().find(|m| &m.name == base) else { continue };
                    for k in 0..c.shades {
                        // Where in the range this shade sits: the one end for
                        // a single shade, the two ends and the steps between
                        // for more.
                        let t = if c.shades <= 1 { 0.5 } else { k as f32 / (c.shades - 1) as f32 };
                        let want: Vec<f32> = (0..lo.len().min(hi.len())).map(|i| lo[i] + (hi[i] - lo[i]) * t).collect();
                        let mut m = src.clone();
                        m.name = coat_material(&plant, slot, &c.name, k, c.shades);
                        m.file_id = None;
                        m.id = 0;
                        m.coat_of = Some(src.name.clone());
                        m.coat_plant = Some(plant.clone());
                        repaint_recipe(&mut m.texture, &want);
                        for r in [m.top.as_mut(), m.bottom.as_mut()].into_iter().flatten() {
                            repaint_recipe(r, &want);
                        }
                        // A fourth value is a glow, as everywhere else.
                        if let Some(g) = want.get(3) {
                            m.glow = *g;
                        }
                        made.push(m);
                    }
                }
            }
        }
        self.materials.extend(made);
    }

    /// Widen a world's reach to the coat materials of the plants it reaches.
    /// They are generated, so no file names them and `reach` cannot find them
    /// by walking the JSON.
    pub fn reach_coats(&self, reach: &mut std::collections::HashSet<String>) {
        let names: Vec<String> = self
            .materials
            .iter()
            .filter(|m| m.coat_plant.as_ref().is_some_and(|p| reach.contains(p)))
            .map(|m| m.name.clone())
            .collect();
        reach.extend(names);
    }

    /// The material a plant draws one of its colour slots in, before any coat.
    pub fn plant_slot(&self, pi: usize, slot: &str) -> Option<&String> {
        let p = &self.plants[pi];
        match slot {
            "bark" => Some(&p.bark),
            "leaf" => p.leaf.as_ref(),
            "flower" => p.flower.as_ref(),
            "autumn" => p.autumn.as_ref(),
            "stalk" => p.stalk.as_ref().map(|(m, _)| m),
            _ => None,
        }
    }

    /// The materials plant `pi` wears in coat `which` at shade `k`: the coat's
    /// own material where it names one, the generated one where it paints a
    /// colour, and nothing for a slot it leaves alone.
    pub fn worn(&self, pi: usize, which: usize, k: u32) -> Worn {
        let mut out = Worn::default();
        let coats = self.plant_coats(pi);
        let Some(c) = coats.get(which) else { return out };
        for (slot, paint) in &c.paint {
            let Some(i) = PLANT_SLOTS.iter().position(|s| s == slot) else { continue };
            out.0[i] = match paint {
                Paint::Material(n) => self.material(n),
                _ => self.material(&coat_material(&self.plants[pi].name, slot, &c.name, k, c.shades)),
            };
        }
        out
    }

    /// The coats plant `pi` comes in, weights normalised. Empty where it has
    /// one colouring. `leaf_alt` is the older way of saying the same thing, so
    /// it compiles into two coats here and nothing downstream reads it.
    pub fn plant_coats(&self, pi: usize) -> Vec<Coat> {
        let p = &self.plants[pi];
        if !p.morphs.is_empty() {
            return coats(&p.morphs);
        }
        match &p.leaf_alt {
            Some((mat, share)) if *share > 0.0 && *share < 1.0 => coats(&[
                MorphDef {
                    name: "leaf".into(),
                    weight: 1.0 - share,
                    shades: 1,
                    colors: HashMap::new(),
                    notes: String::new(),
                },
                MorphDef {
                    name: "leaf_alt".into(),
                    weight: *share,
                    shades: 1,
                    colors: HashMap::from([("leaf".to_string(), Paint::Material(mat.clone()))]),
                    notes: String::new(),
                },
            ]),
            _ => Vec::new(),
        }
    }

    /// The coats creature `ci` comes in, weights normalised.
    pub fn creature_coats(&self, ci: usize) -> Vec<Coat> {
        coats(&self.creatures[ci].morphs)
    }

    /// Rare morphs of creature `ci`: (creature index, chance).
    pub fn creature_morphs(&self, ci: usize) -> Vec<(usize, f32)> {
        let name = &self.creatures[ci].name;
        self.creatures
            .iter()
            .enumerate()
            .filter(|(_, c)| c.rarity > 0.0 && c.like.as_ref() == Some(name))
            .map(|(i, c)| (i, c.rarity))
            .collect()
    }

    pub fn plant(&self, name: &str) -> Option<usize> {
        self.index.plants.get(name).copied()
    }
    pub fn structure(&self, name: &str) -> Option<usize> {
        self.index.structures.get(name).copied()
    }
    pub fn kit(&self, name: &str) -> Option<usize> {
        self.index.kits.get(name).copied()
    }

    /// The city kit called `name`.
    pub fn city_kit(&self, name: &str) -> Option<&crate::world::city::Kit> {
        self.index.cities.get(name).map(|&i| &self.cities[i])
    }
    pub fn creature(&self, name: &str) -> Option<usize> {
        self.index.creatures.get(name).copied()
    }
    pub fn biome(&self, name: &str) -> Option<usize> {
        self.index.biomes.get(name).copied()
    }
    pub fn collectible(&self, name: &str) -> Option<usize> {
        self.index.collectibles.get(name).copied()
    }
    pub fn sign_style(&self, name: &str) -> Option<usize> {
        self.index.signs.get(name).copied()
    }
    pub fn weapon(&self, name: &str) -> Option<usize> {
        self.index.weapons.get(name).copied()
    }


    pub fn summary(&self) -> String {
        format!(
            "{} materials, {} plants, {} structures, {} kits, {} creatures, {} biomes, {} collectibles, {} weapons",
            self.materials.len(),
            self.plants.len(),
            self.structures.len(),
            self.kits.len(),
            self.creatures.len(),
            self.biomes.len(),
            self.collectibles.len(),
            self.weapons.len()
        )
    }
}

#[cfg(test)]
mod tests;
