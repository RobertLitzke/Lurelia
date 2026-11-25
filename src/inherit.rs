//! Library inheritance: an entry that names a parent with `"like"` starts from
//! the parent's resolved JSON and overrides it. Used for creatures, plants,
//! city kits, materials and biomes.
//!
//! - Every key the child gives replaces the parent's, except the objects in
//!   `MERGED` (colours, behaviour, variation), which merge key by key, and
//!   `extra_ops`, which is appended to the parent's `ops` (later ops win, and
//!   `"mat": "air"` carves, so a child can remove a part of the parent too).
//! - `scale` multiplies the parent's sizes (`SCALED`); sizes the child gives
//!   itself are taken as written.
//! - A material's `texture`, `top` and `bottom` recipes merge key by key, and
//!   so does each one's `calm` block. `shade` (a brightness multiple) and
//!   `hue` (toward warm above 0, cool below, as a creature's `vary.hue`)
//!   repaint every colour the child takes from its parent and leave the ones
//!   it gives itself, so a darker bed of the same rock is three lines:
//!   `"like": "quartzite", "shade": 0.78`.
//! - Identity is never inherited: name, version, introduced_by, notes, rarity,
//!   a material's legacy `id` and its old names (`was`), and nor are
//!   `morphs`, whose colours are absolute, or a creature's `forms`.
//! - Chains are allowed (a child of a child); cycles and unknown parents are
//!   errors and the entry is skipped.

use serde_json::{Map, Value};
use std::collections::HashMap;

/// Keys merged key by key rather than replaced.
const MERGED: [&str; 7] = ["colors", "behaviour", "in_water", "vary", "texture", "top", "bottom"];
/// Blocks inside a merged one that merge key by key too (a recipe's `calm`).
const MERGED_INNER: [&str; 1] = ["calm"];
/// A material's colour recipes, which `shade` and `hue` repaint.
const RECIPES: [&str; 3] = ["texture", "top", "bottom"];
/// Keys never inherited. `morphs` is among them because a morph's colours are
/// absolute: a child is a different animal or plant, so its parent's coats
/// would paint it the parent's colours. A child that comes in coats says so
/// itself.
/// A creature's `forms` likewise: an elk's hind is an elk, not a deer.
const OWN: [&str; 10] = ["name", "version", "introduced_by", "notes", "rarity", "like", "morphs", "forms", "id", "was"];
/// Sizes `scale` multiplies: single numbers or [lo, hi] ranges.
const SCALED: [&str; 6] = ["voxel", "half_w", "height", "trunk_radius", "crown_radius", "fly_height"];

fn scale_value(v: &mut Value, s: f64) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                *v = Value::from(f * s);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| scale_value(x, s)),
        _ => {}
    }
}

fn scale_sizes(m: &mut Map<String, Value>, s: f64, skip: &Map<String, Value>) {
    for k in SCALED {
        if skip.contains_key(k) {
            continue;
        }
        if let Some(v) = m.get_mut(k) {
            scale_value(v, s);
        }
    }
    // Flying height lives in the behaviour block.
    if let Some(Value::Object(b)) = m.get_mut("behaviour") {
        let own = skip.get("behaviour").and_then(|b| b.as_object()).is_some_and(|b| b.contains_key("fly_height"));
        if !own {
            if let Some(v) = b.get_mut("fly_height") {
                scale_value(v, s);
            }
        }
    }
}

/// Repaint the colours of a material's recipes the child does not give
/// itself: `shade` multiplies them, `hue` warms them (red up, blue down) or,
/// below 0, cools them. Colours stay in 0..255 and keep a fourth value.
fn repaint(m: &mut Map<String, Value>, shade: f64, hue: f64, child: &Map<String, Value>) {
    let mul = [shade * (1.0 + hue), shade, shade * (1.0 - hue)];
    let paint = |v: &mut Value| {
        if let Value::Array(a) = v {
            for (i, x) in a.iter_mut().take(3).enumerate() {
                if let Some(f) = x.as_f64() {
                    *x = Value::from((f * mul[i]).round().clamp(0.0, 255.0) as i64);
                }
            }
        }
    };
    let own = |block: Option<&Value>, key: &str| block.and_then(|b| b.as_object()).is_some_and(|b| b.contains_key(key));
    for r in RECIPES {
        let Some(Value::Object(rec)) = m.get_mut(r) else { continue };
        let mine = child.get(r);
        for key in ["base", "alt"] {
            if !own(mine, key) {
                if let Some(v) = rec.get_mut(key) {
                    paint(v);
                }
            }
        }
        let mine_calm = mine.and_then(|b| b.get("calm"));
        if let Some(Value::Object(calm)) = rec.get_mut("calm") {
            for key in ["base", "alt"] {
                if !own(mine_calm, key) {
                    if let Some(v) = calm.get_mut(key) {
                        paint(v);
                    }
                }
            }
        }
    }
}

/// One entry laid over another, for a caller holding the two as values: the
/// EDIT page resolves the entry it is showing the same way the loader does
/// (src/editor).
pub fn over(parent: &Value, child: &Value) -> Value {
    match (parent.as_object(), child.as_object()) {
        (Some(p), Some(c)) => Value::Object(merge(p, c)),
        _ => child.clone(),
    }
}

/// An entry with no `like`, as the loader resolves it: a root entry may
/// still scale itself and carry `extra_ops`. The EDIT page uses this for
/// the top of a chain, so a scaled root such as the anhinga opens there.
pub fn root(v: &Value) -> Value {
    match v.as_object() {
        Some(own) => Value::Object(root_map(own)),
        None => v.clone(),
    }
}

fn root_map(own: &Map<String, Value>) -> Map<String, Value> {
    let mut m = own.clone();
    if let Some(s) = own.get("scale").and_then(|s| s.as_f64()) {
        scale_sizes(&mut m, s, &Map::new());
    }
    m.remove("scale");
    if let Some(Value::Array(extra)) = m.remove("extra_ops") {
        if let Some(Value::Array(ops)) = m.get_mut("ops") {
            ops.extend(extra);
        }
    }
    m
}

/// Child over resolved parent.
fn merge(parent: &Map<String, Value>, child: &Map<String, Value>) -> Map<String, Value> {
    let mut out: Map<String, Value> = parent.iter().filter(|(k, _)| !OWN.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    // A creature's voice is its own (`Behaviour::voice`): `like` is mostly
    // about shape, and a crow built on a starling doesn't sing.
    for b in ["behaviour", "in_water"] {
        if let Some(Value::Object(o)) = out.get_mut(b) {
            o.remove("voice");
        }
    }
    if let Some(s) = child.get("scale").and_then(|s| s.as_f64()) {
        scale_sizes(&mut out, s, child);
    }
    let shade = child.get("shade").and_then(|s| s.as_f64());
    let hue = child.get("hue").and_then(|s| s.as_f64());
    if shade.is_some() || hue.is_some() {
        repaint(&mut out, shade.unwrap_or(1.0), hue.unwrap_or(0.0), child);
    }
    for (k, v) in child {
        match k.as_str() {
            "scale" => {}
            // A material's repaint, applied above. Only a material has
            // recipes to repaint; anywhere else the keys are the entry's own.
            "shade" | "hue" if RECIPES.iter().any(|r| out.contains_key(*r)) => {}
            "extra_ops" => {
                let ops = out.entry("ops").or_insert_with(|| Value::Array(Vec::new()));
                if let (Value::Array(o), Value::Array(extra)) = (ops, v) {
                    o.extend(extra.iter().cloned());
                }
            }
            key if MERGED.contains(&key) => match (out.get_mut(key), v) {
                (Some(Value::Object(o)), Value::Object(c)) => {
                    for (ck, cv) in c {
                        match (o.get_mut(ck), cv) {
                            (Some(Value::Object(oi)), Value::Object(ci)) if MERGED_INNER.contains(&ck.as_str()) => {
                                for (ik, iv) in ci {
                                    oi.insert(ik.clone(), iv.clone());
                                }
                            }
                            _ => {
                                o.insert(ck.clone(), cv.clone());
                            }
                        }
                    }
                }
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            },
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Resolve every entry's `like` chain. Entries come as (file, JSON); the
/// result keeps the input order and drops entries that can't be resolved.
pub fn resolve(entries: Vec<(String, Value)>, errors: &mut Vec<String>) -> Vec<(String, Value)> {
    let names: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, (_, v))| v.get("name").and_then(|n| n.as_str()).map(|n| (n.to_string(), i)))
        .collect();
    let mut done: Vec<Option<Map<String, Value>>> = vec![None; entries.len()];
    let mut failed = vec![false; entries.len()];
    fn go(
        i: usize,
        entries: &[(String, Value)],
        names: &HashMap<String, usize>,
        done: &mut Vec<Option<Map<String, Value>>>,
        failed: &mut Vec<bool>,
        stack: &mut Vec<usize>,
        errors: &mut Vec<String>,
    ) -> bool {
        if done[i].is_some() {
            return true;
        }
        if failed[i] {
            return false;
        }
        let (file, v) = &entries[i];
        let Some(own) = v.as_object() else {
            errors.push(format!("{file}: not a JSON object"));
            failed[i] = true;
            return false;
        };
        let resolved = match own.get("like").and_then(|l| l.as_str()) {
            None => root_map(own),
            Some(parent) => {
                let Some(&p) = names.get(parent) else {
                    errors.push(format!("{file}: like {parent}, which is not in the library"));
                    failed[i] = true;
                    return false;
                };
                if stack.contains(&p) || p == i {
                    errors.push(format!("{file}: like {parent} makes a cycle"));
                    failed[i] = true;
                    return false;
                }
                stack.push(i);
                let ok = go(p, entries, names, done, failed, stack, errors);
                stack.pop();
                if !ok {
                    errors.push(format!("{file}: its parent {parent} failed to load"));
                    failed[i] = true;
                    return false;
                }
                let mut m = merge(done[p].as_ref().unwrap(), own);
                m.insert("like".into(), Value::from(parent));
                m
            }
        };
        done[i] = Some(resolved);
        true
    }
    for i in 0..entries.len() {
        let mut stack = Vec::new();
        go(i, &entries, &names, &mut done, &mut failed, &mut stack, errors);
    }
    entries
        .into_iter()
        .zip(done)
        .filter_map(|((f, _), d)| {
            d.map(|mut m| {
                fold_in_water(&mut m);
                (f, Value::Object(m))
            })
        })
        .collect()
}

/// A creature's `in_water` block says only what changes once it is in the
/// water, so lay it over the creature's own `behaviour`: its temperament (how
/// readily it bolts, what it runs from) carries over, its locomotion does not.
/// Done on the way out rather than during the merge, so a child inherits the
/// parent's water block as written and folds its own.
fn fold_in_water(m: &mut Map<String, Value>) {
    let Some(Value::Object(water)) = m.get("in_water").cloned() else {
        return;
    };
    let Some(Value::Object(land)) = m.get("behaviour") else {
        return;
    };
    let mut out = land.clone();
    for (k, v) in water {
        out.insert(k, v);
    }
    m.insert("in_water".into(), Value::Object(out));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn children_override_merge_append_and_scale() {
        let parent = json!({"name": "rabbit", "version": 3, "voxel": 0.03, "height": 0.35, "colors": {"coat": [1, 2, 3], "dark": [0, 0, 0]},
            "ops": [{"op": "set", "at": [0, 0, 0], "mat": "coat"}], "behaviour": {"movement": "hop", "wary": 5}, "rarity": 0.5, "notes": "x"});
        let child = json!({"name": "hare", "like": "rabbit", "scale": 2.0, "height": 0.5, "colors": {"coat": [9, 9, 9]},
            "extra_ops": [{"op": "set", "at": [1, 1, 1], "mat": "dark"}], "behaviour": {"wary": 9}});
        let grandchild = json!({"name": "jack", "like": "hare", "scale": 1.5});
        let mut errors = Vec::new();
        let out = resolve(vec![("r".into(), parent), ("h".into(), child), ("j".into(), grandchild)], &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        let h = &out[1].1;
        assert_eq!(h["name"], "hare");
        assert!((h["voxel"].as_f64().unwrap() - 0.06).abs() < 1e-9, "scaled");
        assert_eq!(h["height"], 0.5, "the child's own size is not scaled");
        assert_eq!(h["colors"]["coat"], json!([9, 9, 9]));
        assert_eq!(h["colors"]["dark"], json!([0, 0, 0]), "colours merge");
        assert_eq!(h["behaviour"]["movement"], "hop", "behaviour merges");
        assert_eq!(h["behaviour"]["wary"], 9);
        assert_eq!(h["ops"].as_array().unwrap().len(), 2, "extra ops append");
        assert!(h.get("rarity").is_none() && h.get("version").is_none() && h.get("notes").is_none(), "identity is not inherited");
        let j = &out[2].1;
        assert!((j["voxel"].as_f64().unwrap() - 0.09).abs() < 1e-9, "scales compound down a chain");
        assert!((j["height"].as_f64().unwrap() - 0.75).abs() < 1e-9);
        assert_eq!(j["like"], "hare");
    }

    #[test]
    fn a_material_takes_its_parents_recipe_repainted() {
        let parent = json!({"name": "rock", "smooth": true, "id": 40, "was": ["stone_old"],
            "texture": {"pattern": "strata", "base": [100, 100, 100], "alt": [200, 200, 200], "scale": 3,
                        "calm": {"base": [120, 120, 120], "alt": [160, 160, 160]}}});
        let child = json!({"name": "dark_rock", "like": "rock", "shade": 0.5, "hue": 0.1,
            "texture": {"alt": [10, 20, 30], "calm": {"alt": [1, 2, 3]}}});
        let mut errors = Vec::new();
        let out = resolve(vec![("p".into(), parent), ("c".into(), child)], &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        let c = &out[1].1;
        let t = &c["texture"];
        // Taken from the parent: repainted, warmer.
        assert_eq!(t["base"], json!([55, 50, 45]));
        assert_eq!(t["calm"]["base"], json!([66, 60, 54]));
        // Given by the child: as written, and the rest of the recipe kept.
        assert_eq!(t["alt"], json!([10, 20, 30]));
        assert_eq!(t["calm"]["alt"], json!([1, 2, 3]));
        assert_eq!(t["pattern"], json!("strata"));
        assert_eq!(c["smooth"], json!(true));
        // Never the parent's identity: no id, no old names, no repaint keys.
        assert!(c.get("id").is_none() && c.get("was").is_none());
        assert!(c.get("shade").is_none() && c.get("hue").is_none());
    }

    #[test]
    fn unknown_parents_and_cycles_are_errors() {
        let mut errors = Vec::new();
        let out = resolve(
            vec![
                ("a".into(), json!({"name": "a", "like": "b"})),
                ("b".into(), json!({"name": "b", "like": "a"})),
                ("c".into(), json!({"name": "c", "like": "nope"})),
            ],
            &mut errors,
        );
        assert!(out.is_empty());
        assert!(errors.iter().any(|e| e.contains("cycle")));
        assert!(errors.iter().any(|e| e.contains("not in the library")));
    }
}
