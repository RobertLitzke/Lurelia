use super::*;

/// A key an op does not have fails the op, naming the key, where it used
/// to be dropped without a word (`Op` is internally tagged, which
/// `parse_known` cannot see into).
#[test]
fn an_op_refuses_a_key_it_does_not_have() {
    let ok: Result<Op, _> = serde_json::from_value(serde_json::json!({"op": "recolour", "at": [0, 0, 0], "until": [1, 1, 1], "from": "a", "to": "b"}));
    assert!(ok.is_ok(), "{ok:?}");
    let bad: Result<Op, _> = serde_json::from_value(serde_json::json!({"op": "recolour", "at": [0, 0, 0], "until": [1, 1, 1], "from": "a", "to": "b", "part": "tail"}));
    let e = bad.expect_err("an unknown key inside an op was accepted").to_string();
    assert!(e.contains("part"), "the error does not name the key: {e}");
}
use serde_json::json;

/// Every library entry parses with no unknown keys and passes validation,
/// so a broken entry fails `cargo test` before any world loads it.
#[test]
fn library_is_valid() {
    let (lib, errors) = Library::load(&data_root());
    assert!(errors.is_empty(), "library load: {errors:#?}");
    let problems = lib.validate();
    assert!(problems.is_empty(), "library validate: {problems:#?}");
    let (looks, errors) = load_looks(&data_root());
    assert!(errors.is_empty(), "looks: {errors:#?}");
    for style in ["classic", "saturated"] {
        assert!(looks.iter().any(|l| l.name == style), "no look for the {style} style");
    }
}

/// `library/schema/structure.schema.json` is generated from `StructureDef`
/// and `Op` and their doc comments, so it says what the loader reads. After
/// changing either, rewrite it with `WRITE_SCHEMA=1 cargo test structure_schema`.
#[test]
fn structure_schema_is_current() {
    let want = serde_json::to_string_pretty(&schemars::schema_for!(StructureDef)).unwrap() + "\n";
    let path = data_root().join("library/schema/structure.schema.json");
    if std::env::var_os("WRITE_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(have == want, "{} is out of date: run WRITE_SCHEMA=1 cargo test structure_schema", path.display());
}

/// The schemas of the other kinds, generated the same way from their loader
/// types: `library/schema/<kind>.schema.json`. Each allows `notes` and keys
/// starting with `_` at the top, as the loader does, and for a kind that
/// inherits, the keys `src/inherit.rs` reads (`like`, and `scale`,
/// `extra_ops`, `shade`, `hue` where they apply), and asks for the required
/// keys only of an entry with no `like`, since a child takes them from its
/// parent. Rewrite with `WRITE_SCHEMA=1 cargo test kind_schemas`.
#[test]
fn kind_schemas_are_current() {
    use serde_json::{json, Value};
    fn shape(mut s: Value, extra: &[(&str, Value)], inherits: bool) -> Value {
        let o = s.as_object_mut().unwrap();
        let props = o.entry("properties").or_insert_with(|| json!({})).as_object_mut().unwrap();
        props
            .entry("notes")
            .or_insert(json!({"description": "One sentence on what the entry is, for people and the catalogue.", "type": "string"}));
        for (k, v) in extra {
            props.insert(k.to_string(), v.clone());
        }
        o.insert("additionalProperties".into(), json!(false));
        o.insert("patternProperties".into(), json!({"^_": {}}));
        if inherits {
            if let Some(req) = o.remove("required") {
                o.insert("if".into(), json!({"not": {"required": ["like"]}}));
                o.insert("then".into(), json!({"required": req}));
            }
        }
        s
    }
    let like = || {
        (
            "like",
            json!({"description": "The entry this one is a variant of (src/inherit.rs): it starts from that entry and overrides what it gives.", "type": "string"}),
        )
    };
    let scale = || {
        (
            "scale",
            json!({"description": "With `like`: multiplies the parent's sizes; sizes this entry gives are taken as written.", "type": "number"}),
        )
    };
    let kinds: Vec<(&str, Value)> = vec![
        (
            "material",
            shape(
                serde_json::to_value(schemars::schema_for!(MaterialDef)).unwrap(),
                &[
                    like(),
                    (
                        "shade",
                        json!({"description": "With `like`: a brightness multiple on every colour taken from the parent.", "type": "number"}),
                    ),
                    (
                        "hue",
                        json!({"description": "With `like`: warms (above 0) or cools (below) every colour taken from the parent.", "type": "number"}),
                    ),
                ],
                true,
            ),
        ),
        ("plant", shape(serde_json::to_value(schemars::schema_for!(PlantDef)).unwrap(), &[like(), scale()], true)),
        (
            "creature",
            shape(
                serde_json::to_value(schemars::schema_for!(CreatureDef)).unwrap(),
                &[
                    like(),
                    scale(),
                    (
                        "extra_ops",
                        json!({"description": "With `like`: ops appended to the parent's (later ops win; `\"mat\": \"air\"` carves).", "type": "array", "items": {"$ref": "#/$defs/Op"}}),
                    ),
                ],
                true,
            ),
        ),
        ("biome", shape(serde_json::to_value(schemars::schema_for!(BiomeDef)).unwrap(), &[like()], true)),
        ("kit", shape(serde_json::to_value(schemars::schema_for!(crate::world::route::kit::KitDef)).unwrap(), &[], false)),
    ];
    let mut stale = Vec::new();
    for (kind, schema) in kinds {
        let want = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        let path = data_root().join(format!("library/schema/{kind}.schema.json"));
        if std::env::var_os("WRITE_SCHEMA").is_some() {
            std::fs::write(&path, &want).unwrap();
            continue;
        }
        if std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n") != want {
            stale.push(path.display().to_string());
        }
    }
    assert!(stale.is_empty(), "out of date, run WRITE_SCHEMA=1 cargo test kind_schemas: {stale:?}");
}

/// A material this world cannot reach gets no id and no texture layer,
/// so a material the engine names in code must be listed in
/// `worldload::ENGINE_MATERIALS` or it would quietly draw as air.
#[test]
fn materials_named_in_code_are_reached_by_every_world() {
    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    rs_files(&data_root().join("src"), &mut files);
    assert!(!files.is_empty(), "no source files found to scan");
    let (lib, _) = Library::load(&data_root());
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap_or_default();
        for pat in ["id_of(\"", ".material(\""] {
            for (at, _) in src.match_indices(pat) {
                let rest = &src[at + pat.len()..];
                let Some(end) = rest.find('"') else { continue };
                let name = &rest[..end];
                if name == "air" || crate::world::matlib::builtin_id(name).is_some() {
                    continue;
                }
                // A generated material (a plant coat, a grain copy) is
                // reached with the entry it was made from, never on its
                // own, so it is not an engine material.
                if name.contains('@') {
                    continue;
                }
                if !lib.materials.iter().any(|m| m.name == name) {
                    continue; // not a library material: nothing to reach
                }
                assert!(
                    crate::worldload::ENGINE_MATERIALS.contains(&name),
                    "{} names the library material {name:?} in code, so it must be in \
                     worldload::ENGINE_MATERIALS or worlds that do not reach it draw nothing",
                    f.display()
                );
            }
        }
    }
}

/// Morph weights are relative and come out as shares, and `leaf_alt` says
/// the same thing the older way, so it compiles into the same two coats.
#[test]
fn morph_weights_become_shares() {
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let wi = lib.plant("wisteria").expect("wisteria");
    let coats = lib.plant_coats(wi);
    assert_eq!(coats.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["violet", "lilac", "white"]);
    // Written 6, 3 and 1, so six in ten, three in ten and one in ten.
    assert!((coats[0].share - 0.6).abs() < 1e-6, "{}", coats[0].share);
    assert!((coats[2].share - 0.1).abs() < 1e-6);
    assert!((coats.iter().map(|c| c.share).sum::<f32>() - 1.0).abs() < 1e-6);
    assert_eq!(coats[0].shades, 3);

    // flowering_cherry has `leaf_alt: ["leaves", 0.2]` and no morphs.
    let ch = lib.plant("flowering_cherry").expect("flowering_cherry");
    let coats = lib.plant_coats(ch);
    assert_eq!(coats.len(), 2, "the leaf and the alternative");
    assert!((coats[0].share - 0.8).abs() < 1e-6);
    assert!((coats[1].share - 0.2).abs() < 1e-6);
    assert_eq!(coats[1].paint, vec![("leaf".to_string(), Paint::Material("leaves".into()))]);

    // A plant with neither comes in one colouring and has no coats, so
    // nothing downstream treats "one coat" as a case of plain.
    assert!(lib.plant_coats(lib.plant("oak").unwrap()).is_empty());
}

/// The mistakes a morph invites, each caught before a world is built.
#[test]
fn bad_morphs_are_caught() {
    let one = |json: serde_json::Value| -> Vec<String> {
        let morphs: Vec<MorphDef> = serde_json::from_value(json).expect("parses");
        let (lib, _) = Library::load(&data_root());
        let mut out = Vec::new();
        let slots = ["coat".to_string(), "belly".to_string()];
        check_morphs(&morphs, &slots, false, "creature test", &lib, &mut out);
        check_coats(&coats(&morphs), "creature test", &mut out);
        out
    };
    let says = |out: &[String], what: &str| assert!(out.iter().any(|e| e.contains(what)), "wanted {what:?} in {out:#?}");

    says(&one(json!([{"name": "a", "colors": {"nose": [1, 2, 3]}}])), "not one of its colours");
    says(&one(json!([{"name": "a", "colors": {"coat": "bark"}}])), "must be a colour, not the material");
    says(&one(json!([{"name": "a", "colors": {"coat": [1, 2]}}])), "wants 3 or 4 numbers");
    says(&one(json!([{"name": "a", "colors": {"coat": [1, 2, 300]}}])), "outside 0..255");
    says(&one(json!([{"name": "a", "colors": {"coat": [[1, 2, 3], [4, 5, 6, 1.2]]}}])), "one end glows");
    says(&one(json!([{"name": "a", "weight": 0, "colors": {"coat": [1, 2, 3]}}])), "weight must be above 0");
    says(&one(json!([{"name": "a", "shades": 9, "colors": {"coat": [1, 2, 3]}}])), "shades must be 1 to 8");
    says(
        &one(json!([{"name": "a", "colors": {"coat": [1, 2, 3]}}, {"name": "a", "colors": {"coat": [9, 9, 9]}}])),
        "two morphs called a",
    );
    says(&one(json!([{"name": "a", "colors": {}}])), "paints nothing does nothing");
    // Two coats nobody could tell apart, and one too rare to ever meet.
    says(
        &one(json!([{"name": "a", "colors": {"coat": [100, 100, 100]}}, {"name": "b", "colors": {"coat": [103, 101, 100]}}])),
        "the same colour to look at",
    );
    says(
        &one(json!([{"name": "a", "weight": 1000, "colors": {"coat": [10, 20, 30]}}, {"name": "b", "weight": 1, "colors": {"coat": [200, 180, 160]}}])),
        "too rare to ever be met",
    );
    // Shades of one coat that could not differ from each other.
    says(&one(json!([{"name": "a", "shades": 8, "colors": {"coat": [[100, 100, 100], [104, 100, 100]]}}])), "steps of under");
    // And the good case says nothing at all.
    assert!(one(json!([
        {"name": "wild", "weight": 9, "colors": {"coat": [[120, 90, 60], [150, 115, 80]]}},
        {"name": "pale", "weight": 1, "shades": 2, "colors": {"coat": [[220, 215, 205], [245, 243, 238]]}}
    ]))
    .is_empty());
}

/// Coats are not inherited: a morph's colours are absolute, so a child
/// would wear its parent's.
#[test]
fn coats_do_not_pass_to_a_child() {
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let wisteria = &lib.plants[lib.plant("wisteria").unwrap()];
    assert!(!wisteria.morphs.is_empty(), "the worked example still has its coats");
    // karst_vine is `like hanging_vine`, which has none; give the parent
    // coats in a resolve and the child must still have none.
    let parent = json!({"name": "p", "form": "vine", "height": [1, 2], "bark": "x", "leaf": "x",
        "morphs": [{"name": "violet", "colors": {"leaf": [1, 2, 3]}}]});
    let child = json!({"name": "c", "like": "p", "height": [3, 4]});
    let mut errors = Vec::new();
    let out = crate::inherit::resolve(vec![("p".into(), parent), ("c".into(), child)], &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(out[0].1.get("morphs").is_some(), "the parent keeps its own");
    assert!(out[1].1.get("morphs").is_none(), "the child inherits none");
}

/// A species with an `in_water` block gets a second form at load, sharing
/// its model and its place in a biome's quota, so one entry and one name
/// covers the animal on the bank and in the water.
#[test]
fn a_water_form_is_the_same_animal() {
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    // No world should still be reaching for a separate swimming entry.
    assert!(!lib.creatures.iter().any(|c| c.name.starts_with("swimming_")), "swimming_X entries are gone");
    // Loading a world installs its material table, which is one per
    // process (`matlib::table_lock`).
    let _table = crate::world::matlib::table_lock();
    let world = crate::worldload::load("lowmouth").expect("lowmouth builds");
    let species = &world.species;
    let find = |n: &str| species.iter().position(|s| s.name == n).unwrap_or_else(|| panic!("no {n}"));
    let gator = find("alligator");
    let swimming = species[gator].water.expect("an alligator can swim");
    assert!(species[swimming].is_water_form);
    assert_eq!(species[swimming].name, "alligator@water");
    assert_eq!(species[swimming].model, species[gator].model, "the same animal, so the same model");
    assert_eq!(species[swimming].quota, gator, "it fills the alligator's place in the biome");
    assert_eq!(species[swimming].water, Some(gator), "and points back, so it can haul out");
    assert_eq!(species[swimming].beh.movement, "school");
    assert_eq!(species[gator].beh.movement, "haulout");
    // Old Pale is a morph of the alligator itself now, so it can be met
    // basking as well as swimming: both forms carry it.
    let pale = find("old_pale");
    assert!(species[gator].morphs.iter().any(|&(m, _)| m == pale), "a morph of the animal on the bank");
    let pale_water = species[pale].water.expect("Old Pale can swim");
    assert!(species[swimming].morphs.iter().any(|&(m, _)| m == pale_water), "and of the one in the water");
}

/// Every world can give an id to each material it reaches: past 238 a
/// world would leave some of its own materials undrawable.
#[test]
fn every_world_has_ids_for_the_materials_it_reaches() {
    let root = data_root();
    let Ok(rd) = std::fs::read_dir(root.join("worlds")) else {
        return;
    };
    let mut dirs: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("chronicle.json").exists()).collect();
    dirs.sort();
    for dir in dirs {
        let (mut lib, _) = Library::load(&root);
        let reach = crate::worldload::reach(&root.join("library"), &dir, &lib);
        let missing = lib.assign_ids(Some(reach));
        assert!(missing.is_empty(), "{}: no voxel id left for {:?}", dir.display(), missing);
    }
}

/// Every world up to 2 km builds without warnings (bigger ones take too
/// long for a unit test; check those with --validate).
#[test]
fn nerve_follows_like() {
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let f = |name: &str| {
        let c = lib.creatures.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {name}"));
        (c.behaviour.flightiness, c.behaviour.juvenile_flightiness)
    };
    // A manta ray carries on as if nobody were there, and its children do.
    assert_eq!(f("manta_ray").0, 0.0);
    assert_eq!(f("sky_ray").0, 0.0);
    assert_eq!(f("kite_ray").0, 0.0);
    // A crocodile is slow to mind you, and its young are jumpier than it
    // is. In the water it is the same animal: `in_water` says how it
    // swims and the temperament carries over, here and down to the
    // alligator that inherits both.
    assert_eq!(f("crocodile"), (0.2, 1.4));
    assert_eq!(f("alligator"), (0.2, 1.4));
    for n in ["crocodile", "alligator"] {
        let w = lib.creatures.iter().find(|c| c.name == n).unwrap().in_water.as_ref().unwrap_or_else(|| panic!("{n} has no in_water"));
        assert_eq!(w.movement, "school", "{n} swims");
        assert!(w.awash, "{n} swims with its eyes out");
        assert_eq!((w.flightiness, w.juvenile_flightiness), (0.2, 1.4), "{n} keeps its nerve in the water");
    }
    // A hare is at the other end, and a fawn stands and stares.
    assert_eq!(f("jackrabbit").0, 1.8);
    assert!(f("deer").1 < 1.0);
    // Children override their parent: the starling is the root bird, and
    // an eagle is nothing like a sparrow about people.
    assert_eq!(f("starling").0, 1.6);
    assert_eq!(f("sparrow").0, 1.7);
    assert_eq!(f("eagle").0, 0.4);
    // A child that says nothing takes its parent's.
    assert_eq!(f("dusk_wren").0, f("sparrow").0);
    assert_eq!(f("serow").0, f("bighorn").0);
}

/// Every world that is a place states a climate of its own, names only
/// kinds the engine has, and opens on one of them. Two of them are rules
/// about play rather than about the place, and are worth saying out loud:
/// Windbarrow never gets heat, because heat is a windless day and the
/// wind there never stops, and the duel bench gets one kind of day for
/// ever so a gun is always measured under the same sky.
#[test]
fn every_world_states_its_own_climate() {
    use crate::world::weather::{Climate, Kind};
    let root = data_root();
    let Ok(rd) = std::fs::read_dir(root.join("worlds")) else {
        return;
    };
    let mut dirs: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("chronicle.json").exists()).collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let def: crate::world::fields::WorldDef = serde_json::from_str(&std::fs::read_to_string(dir.join("world.json")).unwrap()).unwrap();
        assert!(!def.weather.is_empty(), "{name} has no climate");
        assert!(Climate::unknown(&def.weather).is_empty(), "{name}: {:?}", Climate::unknown(&def.weather));
        let climate = Climate::from_weights(&def.weather);
        let start = def.start_weather.as_deref().and_then(Kind::parse).unwrap_or_else(|| climate.commonest());
        assert!(climate.has(start), "{name} opens on a kind of day it never has");
        if let Some(w) = &def.start_weather {
            assert!(Kind::parse(w).is_some(), "{name}: no such weather as {w}");
        }
        match name.as_str() {
            "windbarrow" => assert!(!climate.has(Kind::Heat), "the wind at Windbarrow never stops"),
            "duelbox" => {
                for k in [Kind::Heat, Kind::Clouds, Kind::Rain, Kind::Snow, Kind::Fog] {
                    assert!(!climate.has(k), "the bench must not vary");
                }
                assert!(climate.has(Kind::Sun));
            }
            "slagdeck" => assert!(!climate.has(Kind::Fog), "a match should not turn on how far you can see"),
            _ => {}
        }
    }
}

/// Every world states the parts of the year it has, names only real
/// seasons, and opens in one it has. The three worlds whose calendars
/// are rules about play rather than about the place are pinned by name.
#[test]
fn every_world_states_its_own_seasons() {
    use crate::world::season::Season;
    let root = data_root();
    let Ok(rd) = std::fs::read_dir(root.join("worlds")) else {
        return;
    };
    let mut dirs: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("chronicle.json").exists()).collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let def: crate::world::fields::WorldDef = serde_json::from_str(&std::fs::read_to_string(dir.join("world.json")).unwrap()).unwrap();
        assert!(!def.seasons.is_empty(), "{name} states no seasons");
        let unknown = crate::world::season::Calendar::unknown(&def.seasons);
        assert!(unknown.is_empty(), "{name}: {unknown:?}");
        let cal = def.calendar();
        let start = def.start_season.as_deref().and_then(Season::parse).unwrap_or_else(|| cal.first());
        assert!(cal.has(start), "{name} opens in a season it never has");
        // A world that goes bare needs trees that can: a winter with no
        // deciduous tree in it is only a change of light.
        match name.as_str() {
            "duelbox" | "slagdeck" => {
                assert_eq!(cal.list(), vec![Season::Summer], "a match is played under one sky");
            }
            "thornpan" => {
                assert!(cal.has(Season::Winter), "the delta has a dry season");
                // And it never snows in it: that is what makes it dry.
                assert!(!crate::world::weather::Climate::from_weights(&def.weather).has(crate::world::weather::Kind::Snow));
            }
            "jadewater" => assert!(!cal.has(Season::Winter), "nothing goes bare in the tropics"),
            _ => {}
        }
    }
}

#[test]
fn small_worlds_build_cleanly() {
    let root = data_root();
    let Ok(rd) = std::fs::read_dir(root.join("worlds")) else {
        return;
    };
    let mut dirs: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("chronicle.json").exists()).collect();
    dirs.sort();
    for dir in dirs {
        let def: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("world.json")).unwrap()).unwrap();
        if def["size_m"].as_f64().unwrap_or(0.0) > 2048.0 {
            continue;
        }
        // Ids by this world's reach, as the game loads a world
        // (`worldload::library_for`). With every material counted as
        // reached the library no longer fits in 238 ids, so the ones that
        // missed out would come back as "unknown material".
        let (mut lib, _) = Library::load(&root);
        let reach = crate::worldload::reach(&root.join("library"), &dir, &lib);
        lib.assign_ids(Some(reach));
        let world = crate::world::chronicle::build_world(&dir, lib).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        assert!(world.warnings.is_empty(), "{}: {:#?}", dir.display(), world.warnings);
    }
}

/// A coat's fourth number sets how bright a glow burns: the glassmoth's
/// lantern coat asks its wings for 2.4 over the model's 1.8.
#[test]
fn a_coat_can_brighten_a_glow() {
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let ci = lib.creature("glassmoth").expect("glassmoth");
    let coats = lib.creature_coats(ci);
    let wears = crate::variety::wears(&coats, &lib.creatures[ci].colors);
    let glow = |n: &str| wears.iter().find(|w| w.name == n).expect(n).at(0.5).glow;
    assert!((glow("lantern") - 2.4 / 1.8).abs() < 1e-3, "{}", glow("lantern"));
    assert!((glow("violet") - 1.0).abs() < 1e-6, "a coat that names no glow leaves it alone");
    // And nothing can light a colour that has no glow at all.
    let mut bad = lib.creatures[ci].clone();
    bad.morphs[1].colors.insert("mothbody".into(), Paint::One(vec![36.0, 28.0, 24.0, 1.0]));
    let mut out = Vec::new();
    super::creature::check_fit(&super::creature::coats_of(&bad.morphs), &bad.colors, "glassmoth", &mut out);
    assert!(out.iter().any(|e| e.contains("does not glow")), "{out:#?}");
}

/// A material merged into another (`was`) still answers to its old name, so
/// a save or a local file that names it loads the material it became.
#[test]
fn a_merged_material_answers_to_its_old_name() {
    let (mut lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let reach: std::collections::HashSet<String> = ["reedmace_head".to_string()].into_iter().collect();
    lib.assign_ids(Some(reach));
    // Through variables: a quoted name in `.material(` reads as an engine
    // material to `materials_named_in_code_are_reached_by_every_world`.
    let (now, was) = ("cattail_head", "reedmace_head");
    let id = lib.material(now).expect("an old name reaches the material it became");
    assert_eq!(lib.material(was), Some(id));
}

/// Every material as the loader resolves it, less what does not draw (name
/// aside): `DUMP_MATERIALS=out.txt cargo test dump_materials -- --ignored`.
/// Run before and after turning entries into `like` children and diff the
/// two files: a conversion that changes nothing on screen changes nothing here.
#[test]
#[ignore]
fn dump_materials() {
    let Ok(path) = std::env::var("DUMP_MATERIALS") else { return };
    let (lib, errs) = Library::load(&data_root());
    assert!(errs.is_empty(), "{errs:#?}");
    let mut rows: Vec<String> = lib
        .materials
        .iter()
        .filter(|m| m.coat_of.is_none())
        .map(|m| {
            let mut m = m.clone();
            (m.version, m.introduced_by, m.like, m.was, m.file_id) = (0, String::new(), None, Vec::new(), None);
            format!("{m:?}")
        })
        .collect();
    // Plants, biomes and creatures as resolved JSON (sorted keys, so the
    // dump is the same from run to run, which a HashMap's Debug is not),
    // without identity and without nulls, which read as a missing key does.
    fn bare(v: &mut serde_json::Value) {
        if let serde_json::Value::Object(m) = v {
            m.retain(|k, x| !x.is_null() && !["like", "version", "introduced_by", "notes"].contains(&k.as_str()));
        }
    }
    for kind in ["plants", "biomes", "creatures"] {
        let mut e = Vec::new();
        let raw: Vec<(String, serde_json::Value)> = super::load_dir(&data_root().join("library").join(kind), &mut e);
        for (_, mut v) in crate::inherit::resolve(raw, &mut e) {
            bare(&mut v);
            rows.push(format!("{kind} {v}"));
        }
        assert!(e.is_empty(), "{e:#?}");
    }
    rows.sort();
    std::fs::write(path, rows.join("\n")).unwrap();
}
