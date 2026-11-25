//! Per-game overrides of the mechanics: how a game changes the guns and the
//! way bodies move without touching what every other game uses.
//!
//! There are three layers. The engine's own rules (the 5 m/s walk, the
//! physics) are the same everywhere. The library's entries (a weapon's file)
//! are what every game gets by default. A game file (`library/games/`) may
//! then override either for its own matches only:
//!
//! - `weapon_tune`: JSON merged onto each weapon's own file before the match
//!   is armed, under `"*"` for every gun, then the gun's role (`midrange`),
//!   then its name (`assault_rifle`), later ones winning. An object merges key
//!   by key, so `{"fire": {"kick": 0.6}}` changes only the kick. A number can
//!   be scaled rather than replaced: `{"ammo_max": {"times": 0.5}}`.
//! - `terrain_pace`: what share of their pace bodies keep in each terrain
//!   class (`world::survey::terrain::Terrain`), by the class's name; a class
//!   left out is walked at full pace. Empty, as in every game but the ones
//!   that ask, and the ground is all one.
//!
//! The zombie game uses both; explore and the arenas use neither, so a marsh
//! never slows anybody down outside it.

use crate::library::{Library, WeaponDef};
use serde_json::Value;
use std::collections::BTreeMap;

/// Whether `v` scales something (`{"times": x}`, at any depth): a scale has
/// nothing to scale where the entry has no such key, so it is skipped there
/// rather than written in. A shovel has no reload to make slower.
fn scales(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.contains_key("times") || o.values().any(scales),
        _ => false,
    }
}

/// Merge `over` onto `base`: objects key by key, `{"times": x}` onto a number
/// scales it, anything else replaces. A whole block the entry does not have
/// at its top level is not made up (an `alt` on a gun with one way of
/// firing), though a new key inside a block it has is (`move_spread` in its
/// `fire`).
pub fn merge(base: &mut Value, over: &Value) {
    merge_at(base, over, 0);
}

fn merge_at(base: &mut Value, over: &Value, depth: u32) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(Value::Null) if v.is_object() && depth == 0 => {}
                    Some(bv) => merge_at(bv, v, depth + 1),
                    None if scales(v) || (v.is_object() && depth == 0) => {}
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b @ Value::Number(_), Value::Object(o)) if o.len() == 1 && o.contains_key("times") => {
            if let (Some(x), Some(t)) = (b.as_f64(), o["times"].as_f64()) {
                let v = x * t;
                // A count stays a count.
                *b = if b.is_u64() { Value::from(v.round().max(0.0) as u64) } else { Value::from(v) };
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// Weapon `name` as a game with `tune` plays it, or None where the game
/// changes nothing about it (or the library has no file for it).
pub fn weapon(lib: &Library, name: &str, tune: &BTreeMap<String, Value>) -> Option<WeaponDef> {
    if tune.is_empty() {
        return None;
    }
    let def = lib.weapon(name).map(|i| &lib.weapons[i])?;
    let keys = ["*", def.role.as_str(), name];
    if !keys.iter().any(|k| tune.contains_key(*k)) {
        return None;
    }
    let mut v = lib.weapon_json.get(name)?.clone();
    for k in keys {
        if let Some(o) = tune.get(k) {
            merge(&mut v, o);
        }
    }
    match serde_json::from_value::<WeaponDef>(v) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("weapon_tune for {name}: {e}; the library's own");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_scales_and_replaces() {
        let mut v = json!({"ammo_max": 300, "reload_s": 2.0, "fire": {"kick": 0.4, "rpm": 700.0}, "role": "midrange"});
        merge(&mut v, &json!({"ammo_max": {"times": 0.5}, "reload_s": {"times": 1.15}, "fire": {"kick": {"times": 1.5}}, "role": "heavy"}));
        assert_eq!(v["ammo_max"], json!(150));
        assert!((v["reload_s"].as_f64().unwrap() - 2.3).abs() < 1e-9);
        assert!((v["fire"]["kick"].as_f64().unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(v["fire"]["rpm"], json!(700.0), "a key the tune leaves alone is kept");
        assert_eq!(v["role"], json!("heavy"));
    }

    #[test]
    fn adds_keys_inside_blocks_but_not_blocks() {
        let mut v = json!({"fire": {"kick": 0.4}});
        merge(&mut v, &json!({"fire": {"move_spread": {"per_mps": 0.3}}, "alt": {"kick": 1.0}}));
        assert_eq!(v["fire"]["move_spread"]["per_mps"], json!(0.3), "a new key inside the fire block");
        assert!(v.get("alt").is_none(), "no second way of firing made up");
    }

    #[test]
    fn a_game_tunes_its_own_guns_only() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let base = &lib.weapons[lib.weapon("assault_rifle").expect("the library has an assault rifle")];
        let tune: BTreeMap<String, Value> = serde_json::from_value(json!({"*": {"ammo_max": {"times": 0.5}}, "midrange": {"fire": {"kick": {"times": 2.0}}}})).unwrap();
        let t = weapon(&lib, "assault_rifle", &tune).expect("tuned");
        assert_eq!(t.ammo_max, (base.ammo_max as f64 * 0.5).round() as u32);
        assert!((t.fire.kick - base.fire.kick * 2.0).abs() < 1e-4);
        assert!(weapon(&lib, "assault_rifle", &BTreeMap::new()).is_none(), "no tune, no change");
    }
}
