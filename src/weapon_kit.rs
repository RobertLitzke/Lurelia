//! Weapon kits: which guns fill the arena's slots (`library/weapon_kits/*.json`).
//!
//! A map's pads and its `weapons` list name guns, but what a pad stands for
//! is the gun's *role*: the rocket launcher's pad is where the explosive
//! launcher lies, the scrap gun's is where the close-spread gun lies. Every
//! weapon names its role (`WeaponDef::role`), and a kit says which guns fill
//! each role. Played with another kit, every gun on the map is swapped for the
//! kit's gun in the same role, so the same spot holds the same kind of weapon
//! whichever kit is in play.
//!
//! A role may offer more than one gun (the realistic kit's melee is a shovel or
//! a knife). One of them is drawn per role when the match starts, and every
//! pad of that role takes it: a weapon pad and the ammunition pads beside it
//! always agree, so there are never rounds lying about for a gun the map does
//! not have. A role the kit leaves out takes its guns and their pads off the
//! map.
//!
//! The arena kit lists each role's own arena gun, so a map played with it is
//! the map as it was written.

use crate::library::Library;
use crate::world::fields::ShooterDef;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

/// The roles, in slot order: name, the digit that picks it, and what it is
/// for. The digit is the role's, so every gun in a role sits on the same key
/// in every kit.
pub const ROLES: [(&str, u32, &str); 10] = [
    ("melee", 1, "close work with no ammunition: the Digger, a shovel, a knife"),
    ("sidearm", 2, "the gun you are left with after a death"),
    ("lobber", 3, "lobbed rounds that go off where they land or later"),
    ("breacher", 4, "long reach that goes through walls"),
    ("stream", 5, "a held stream: damage by the second"),
    ("midrange", 6, "the all-rounder for a fight across a room"),
    ("heavy", 7, "a sustained wall of rounds"),
    ("spread", 8, "a cone of shot for touching distance"),
    ("explosive", 9, "an explosive launcher: a burst on whatever it meets"),
    ("marksman", 10, "one aimed shot at range, worth most in the head"),
];

/// The kit a match uses where neither the map nor the game nor the run names
/// one.
pub const DEFAULT: &str = "arena";

/// The slot a role sits on.
pub fn slot(role: &str) -> Option<u32> {
    ROLES.iter().find(|r| r.0 == role).map(|r| r.1)
}

/// One kit.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KitDef {
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    introduced_by: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    notes: Option<String>,
    /// How the launch screen shows it.
    pub title: String,
    /// The guns that may fill each role. One is drawn per role per match.
    pub roles: BTreeMap<String, Vec<String>>,
}

pub struct Catalogue {
    pub kits: Vec<KitDef>,
    pub errors: Vec<String>,
}

fn read(dir: &Path) -> Catalogue {
    let mut kits = Vec::new();
    let mut errors = Vec::new();
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect())
        .unwrap_or_default();
    files.sort();
    for p in files {
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<KitDef>(&s).map_err(|e| e.to_string()))
        {
            Ok(k) => kits.push(k),
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    // The kits come out in a fixed order with the arena first, which is the
    // order the launch screen offers them in.
    kits.sort_by_key(|k| (k.name != DEFAULT, k.name.clone()));
    Catalogue { kits, errors }
}

/// The library's kits, read once.
pub fn catalogue() -> &'static Catalogue {
    static C: OnceLock<Catalogue> = OnceLock::new();
    C.get_or_init(|| {
        let c = read(&crate::library::data_root().join("library").join("weapon_kits"));
        for e in &c.errors {
            eprintln!("weapon kits: {e}");
        }
        c
    })
}

pub fn get(name: &str) -> Option<&'static KitDef> {
    catalogue().kits.iter().find(|k| k.name == name)
}

/// What a kit did to a match's guns, for the console.
#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    /// Map gun -> the kit's gun that stands in for it.
    pub swapped: Vec<(String, String)>,
    /// Map guns whose role the kit leaves empty: gone, pads and all.
    pub dropped: Vec<String>,
}

/// Play `rules` with `kit`: every gun it issues and every weapon and ammo pad
/// on its floor becomes the kit's gun in the same role. `seed` draws between
/// the guns a role offers, once per role, so every pad of a role agrees.
///
/// A gun the kit lists in its own role is kept as it is, which is why the
/// arena kit changes nothing. A gun with no role, or a role the kit has no
/// guns for, is taken off the map with its pads.
pub fn apply(rules: &mut ShooterDef, lib: &Library, kit: &KitDef, seed: u32) -> Applied {
    let mut out = Applied::default();
    let role_of = |name: &str| lib.weapons.iter().find(|w| w.name == name).map(|w| w.role.clone()).unwrap_or_default();
    // The guns a role offers that this library has.
    let offered = |role: &str| -> Vec<&str> {
        kit.roles.get(role).into_iter().flatten().map(|s| s.as_str()).filter(|n| lib.weapons.iter().any(|w| w.name == *n)).collect()
    };
    // One draw per role, made the first time the role is asked about.
    let mut drawn: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut swap = |name: &str| -> Option<String> {
        let role = role_of(name);
        let choices = offered(&role);
        if choices.contains(&name) {
            return Some(name.to_string());
        }
        drawn
            .entry(role.clone())
            .or_insert_with(|| {
                (!choices.is_empty()).then(|| {
                    let k = crate::world::noise::hash32(seed ^ crate::world::noise::hash32(role.bytes().fold(0x9e37_79b9u32, |h, b| h.rotate_left(5) ^ b as u32))) as usize;
                    choices[k % choices.len()].to_string()
                })
            })
            .clone()
    };
    let mut weapons = Vec::new();
    for w in &rules.weapons {
        match swap(w) {
            Some(n) => {
                if n != *w {
                    out.swapped.push((w.clone(), n.clone()));
                }
                if !weapons.contains(&n) {
                    weapons.push(n);
                }
            }
            None => out.dropped.push(w.clone()),
        }
    }
    rules.weapons = weapons;
    rules.pickups.retain_mut(|p| {
        let Some(w) = p.weapon.clone() else { return true };
        match swap(&w) {
            Some(n) => {
                if n != w {
                    // The platform keeps the map's colour: it says what kind
                    // of gun lies here, and that is the role.
                    p.pad_of.get_or_insert(w);
                    p.weapon = Some(n);
                }
                true
            }
            None => false,
        }
    });
    out
}

/// Which kit a match uses: the run's, else the game's, else the map's, else
/// the arena.
pub fn chosen<'a>(run: Option<&'a str>, game: Option<&'a str>, map: Option<&'a str>) -> &'a str {
    run.or(game).or(map).unwrap_or(DEFAULT)
}

/// The guns the arena maps are written with: those with a pad colour of
/// their own (`weapon_pad_<gun>`). A kit's other guns stand on the colour of
/// the arena gun whose role they fill (`PickupDef::pad_of`).
pub fn arena_guns(lib: &Library) -> impl Iterator<Item = &crate::library::WeaponDef> {
    lib.weapons.iter().filter(|w| {
        let pad = format!("weapon_pad_{}", w.name);
        lib.materials.iter().any(|m| m.name == pad)
    })
}

/// What is wrong with the kits against this library.
pub fn check(lib: &Library) -> Vec<String> {
    let mut out = catalogue().errors.clone();
    if get(DEFAULT).is_none() {
        out.push(format!("library/weapon_kits: no {DEFAULT}.json"));
    }
    for k in &catalogue().kits {
        for (role, guns) in &k.roles {
            if slot(role).is_none() {
                out.push(format!("weapon kit {}: no role called {role} ({})", k.name, ROLES.map(|r| r.0).join(", ")));
            }
            if guns.is_empty() {
                out.push(format!("weapon kit {}: {role} lists no guns (leave the role out instead)", k.name));
            }
            for g in guns {
                match lib.weapons.iter().find(|w| w.name == *g) {
                    None => out.push(format!("weapon kit {}: no weapon called {g}", k.name)),
                    Some(w) if w.role != *role => out.push(format!("weapon kit {}: {g} is a {} gun, listed under {role}", k.name, w.role)),
                    Some(_) => {}
                }
            }
        }
    }
    // The arena kit is the map as written: every arena gun on its own role.
    if let Some(a) = get(DEFAULT) {
        for w in arena_guns(lib) {
            if !a.roles.get(&w.role).is_some_and(|g| g.contains(&w.name)) {
                out.push(format!("weapon kit {DEFAULT}: {} is not under its role {}", w.name, w.role));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::PickupDef;

    fn lib() -> Library {
        let (lib, errs) = Library::load(Path::new("."));
        assert!(errs.is_empty(), "{errs:?}");
        lib
    }

    fn rules(weapons: &[&str], pads: &[(&str, &str)]) -> ShooterDef {
        let pickups: Vec<serde_json::Value> = pads.iter().map(|(k, w)| serde_json::json!({"kind": k, "at": [0, 0], "weapon": w})).collect();
        serde_json::from_value(serde_json::json!({"weapons": weapons, "pickups": pickups})).unwrap()
    }

    #[test]
    fn the_kits_check() {
        assert_eq!(check(&lib()), Vec::<String>::new());
    }

    /// The arena kit is the map as it was written.
    #[test]
    fn the_arena_kit_changes_nothing() {
        let lib = lib();
        let all: Vec<&str> = arena_guns(&lib).map(|w| w.name.as_str()).collect();
        assert_eq!(all.len(), ROLES.len(), "an arena gun for each role: {all:?}");
        let pads: Vec<(&str, &str)> = all.iter().flat_map(|w| [("weapon", *w), ("ammo", *w)]).collect();
        let mut r = rules(&all, &pads);
        let before = (r.weapons.clone(), r.pickups.iter().map(|p| p.weapon.clone()).collect::<Vec<_>>());
        let a = apply(&mut r, &lib, get(DEFAULT).unwrap(), 7);
        assert_eq!(a, Applied::default());
        assert_eq!(before, (r.weapons.clone(), r.pickups.iter().map(|p| p.weapon.clone()).collect::<Vec<_>>()));
        assert!(r.pickups.iter().all(|p| p.pad_of.is_none()));
    }

    /// Played with the realistic kit, a rocket pad holds the explosive
    /// launcher, and a role with two guns hands every pad of it the same one
    /// whatever the draw.
    #[test]
    fn a_kit_swaps_by_role_and_keeps_pads_in_step() {
        let lib = lib();
        let kit = get("realistic").expect("a realistic kit");
        for seed in 0..40 {
            let mut r = rules(
                &["digger", "sidearm", "rocket_launcher", "scrap_gun"],
                &[("weapon", "rocket_launcher"), ("ammo", "rocket_launcher"), ("ammo", "rocket_launcher"), ("weapon", "digger"), ("ammo", "scrap_gun"), ("weapon", "scrap_gun")],
            );
            apply(&mut r, &lib, kit, seed);
            let role = |n: &str| lib.weapons.iter().find(|w| w.name == n).unwrap().role.clone();
            assert_eq!(r.weapons.iter().map(|w| role(w)).collect::<Vec<_>>(), ["melee", "sidearm", "explosive", "spread"]);
            for p in &r.pickups {
                let w = p.weapon.as_deref().unwrap();
                assert!(r.weapons.iter().any(|x| x == w), "seed {seed}: a pad for {w}, which nobody issues");
                assert!(kit.roles[&role(w)].iter().any(|x| x == w));
                // The platform is still the map's colour.
                assert!(p.pad_material().unwrap().starts_with("weapon_pad_"));
                assert!(lib.material(&p.pad_material().unwrap()).is_some() || p.pad_of.is_some());
            }
        }
        // Both melee guns come up over enough draws.
        let melee: std::collections::BTreeSet<String> = (0..40)
            .map(|s| {
                let mut r = rules(&["digger"], &[]);
                apply(&mut r, &lib, kit, s);
                r.weapons[0].clone()
            })
            .collect();
        assert_eq!(melee.len(), kit.roles["melee"].len());
    }

    /// A role the kit leaves out takes its gun off the map, pads and all.
    #[test]
    fn a_role_a_kit_has_not_got_goes_with_its_pads() {
        let lib = lib();
        let kit: KitDef = serde_json::from_value(serde_json::json!({"name": "k", "title": "K", "roles": {"spread": ["scrap_gun"]}})).unwrap();
        let mut r = rules(&["scrap_gun", "rocket_launcher"], &[("weapon", "rocket_launcher"), ("ammo", "rocket_launcher"), ("ammo", "scrap_gun"), ("health", "")]);
        r.pickups[3].weapon = None;
        let a = apply(&mut r, &lib, &kit, 1);
        assert_eq!(r.weapons, ["scrap_gun"]);
        assert_eq!(a.dropped, ["rocket_launcher"]);
        assert_eq!(r.pickups.iter().map(|p: &PickupDef| p.kind.as_str()).collect::<Vec<_>>(), ["ammo", "health"]);
    }
}
