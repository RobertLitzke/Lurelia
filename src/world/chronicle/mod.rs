//! A world's history and the processes it runs. `chronicle.json` lists dated
//! events (the LLM writes it from the world's story); each event kind is a
//! process that changes the world fields: raising land, carving a crater,
//! routing a river and later moving it, building and abandoning a settlement,
//! burning a slope. Events run oldest first, then biomes are read off the
//! result and plants and natural features are placed by the biome rules.
//!
//! Each event kind lives in its own file under `events/` and is listed once in
//! the registry in `events/mod.rs`.

mod classify;
mod ctx;
mod events;
mod gazetteer;
pub mod geom;
pub mod hydro;

pub use ctx::{compile_structure, EventCtx, Process};
pub use gazetteer::gazetteer;

use super::fields::*;
use super::flora;
use super::noise::hash32;
use crate::library::Library;
use crate::util::FxHashMap as HashMap;
use glam::{Vec2, Vec3};
use std::path::Path;

/// Parsed by `parse_chronicle`, which reports unknown keys.
#[derive(Debug, Default)]
pub struct Chronicle {
    /// Named places the events refer to, metres.
    pub places: HashMap<String, [f32; 2]>,
    pub events: Vec<Event>,
}

/// In chronicle.json each event is one object: `year`, `cause`, `event` (the
/// kind's name in the registry) and the kind's own fields.
#[derive(Debug)]
pub struct Event {
    /// Years before the present (negative); 0 is now.
    pub year: i32,
    /// Why it happened, in the story's words. Every event needs one.
    pub cause: String,
    /// The `event` name, as the registry spells it.
    pub kind: &'static str,
    /// Seed the event as if it stood at this place (1-based) in the sorted
    /// list: keeps its noise when events are added before it later.
    pub seed: Option<u32>,
    pub process: Box<dyn Process>,
}

/// Read chronicle.json. Each event's `event` field names its kind; the rest of
/// its fields are that kind's, and any key the kind doesn't know is reported.
fn parse_chronicle(v: serde_json::Value, warnings: &mut Vec<String>) -> Result<Chronicle, String> {
    let serde_json::Value::Object(mut top) = v else {
        return Err("chronicle.json: not an object".into());
    };
    let places = match top.remove("places") {
        Some(p) => serde_json::from_value(p).map_err(|e| format!("chronicle.json places: {e}"))?,
        None => HashMap::default(),
    };
    let Some(serde_json::Value::Array(list)) = top.remove("events") else {
        return Err("chronicle.json: no events list".into());
    };
    for k in top.keys() {
        warnings.push(format!("chronicle.json: unknown key {k}"));
    }
    let mut events = Vec::new();
    for (i, e) in list.into_iter().enumerate() {
        let serde_json::Value::Object(mut m) = e else {
            return Err(format!("chronicle.json: event {i} is not an object"));
        };
        let year = m.remove("year").and_then(|y| y.as_i64()).ok_or_else(|| format!("chronicle.json: event {i} has no year"))? as i32;
        let cause = m.remove("cause").and_then(|c| c.as_str().map(String::from)).unwrap_or_default();
        let seed = m.remove("seed").and_then(|s| s.as_u64()).map(|s| s as u32);
        let tag = m
            .remove("event")
            .and_then(|t| t.as_str().map(String::from))
            .ok_or_else(|| format!("chronicle.json: event {i} has no \"event\""))?;
        let Some(et) = events::find(&tag) else {
            return Err(format!("chronicle.json: event {i} ({tag}): {}", events::unknown(&tag)));
        };
        let process = (et.parse)(serde_json::Value::Object(m), &mut |k| warnings.push(format!("chronicle.json: event {i} ({tag}): unknown key {k}")))
            .map_err(|e| format!("chronicle.json: event {i} ({tag}): {e}"))?;
        events.push(Event {
            year,
            cause,
            kind: et.name,
            seed,
            process,
        });
    }
    Ok(Chronicle { places, events })
}

/// Load a world folder and build it: run the chronicle, classify biomes, place
/// plants and features.
/// A seed key from a name (FNV-1a), the same hash flora uses.
fn name_key(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// What each event's own randomness is keyed by, in the order the chronicle
/// was sorted into: the `seed` it names, else its kind, its year and which of
/// that kind in that year it is.
///
/// The first scheme (`v1`, the worlds written before this) keyed an event by
/// its place in the sorted list, so adding one in the middle moved every
/// event after it onto a new seed and reshuffled half the world. A world laid
/// out that way says `"seeds": 1` in its world.json and keeps it.
fn event_keys<'a>(events: impl Iterator<Item = (&'a str, i32, Option<u32>)>, v1: bool) -> Vec<u32> {
    let mut seen: std::collections::HashMap<(&str, i32), u32> = std::collections::HashMap::new();
    events
        .enumerate()
        .map(|(k, (kind, year, own))| {
            let nth = *seen.entry((kind, year)).and_modify(|n| *n += 1).or_insert(0);
            match (own, v1) {
                (Some(s), _) => s,
                (None, true) => (k + 1) as u32,
                (None, false) => hash32(name_key(kind) ^ (year as u32).wrapping_mul(0x85eb_ca6b) ^ nth.wrapping_mul(0x27d4_eb2d)),
            }
        })
        .collect()
}

/// Keys for the items one event left, from the event's own key, each item's
/// kind and which of that kind it is in the event. Moving a find, or adding
/// an event anywhere else in the chronicle, leaves every key as it was; only
/// changing the event that left it (or the order it leaves things of one
/// kind) moves one.
fn item_keys_for(event: u32, items: &[super::fields::Item]) -> Vec<u32> {
    let mut seen: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    items
        .iter()
        .map(|it| {
            let nth = *seen.entry(it.kind.as_str()).and_modify(|n| *n += 1).or_insert(0);
            // Never 0..=i32::MAX's negative half: the save keeps it as an i32.
            hash32(event ^ name_key(&it.kind).wrapping_mul(0x9e37_79b9) ^ nth.wrapping_mul(0x85eb_ca6b)) & 0x7fff_ffff
        })
        .collect()
}

pub fn build_world(dir: &Path, lib: Library) -> Result<World, String> {
    build_world_with(dir, lib, &[])
}

/// `build_world`, meeting the neighbours' edges (`world/seam.rs`): the
/// strips are taken by the caller, from neighbours built first
/// (`worldload::edges_for`), since building one installs its own
/// material table.
pub fn build_world_with(dir: &Path, lib: Library, edges: &[super::seam::Edge]) -> Result<World, String> {
    let t0 = std::time::Instant::now();
    let mut warnings = Vec::new();
    let read = |name: &str| -> Result<serde_json::Value, String> {
        let text = std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))
    };
    let def: WorldDef = crate::library::parse_known(read("world.json")?, |k| warnings.push(format!("world.json: unknown key {k}"))).map_err(|e| format!("world.json: {e}"))?;
    for k in crate::world::weather::Climate::unknown(&def.weather) {
        warnings.push(format!("world.json: no such weather as {k}"));
    }
    if let Some(w) = def.start_weather.as_deref() {
        if crate::world::weather::Kind::parse(w).is_none() {
            warnings.push(format!("world.json: no such weather as {w}"));
        }
    }
    for k in crate::world::season::Calendar::unknown(&def.seasons) {
        warnings.push(format!("world.json: no such season as {k}"));
    }
    if let Some(s) = def.start_season.as_deref() {
        match crate::world::season::Season::parse(s) {
            Some(k) if !def.calendar().has(k) => warnings.push(format!("world.json: starts in {s}, which is not one of its seasons")),
            Some(_) => {}
            None => warnings.push(format!("world.json: no such season as {s}")),
        }
    }
    let mut chron = parse_chronicle(read("chronicle.json")?, &mut warnings)?;
    let mut log = Vec::new();
    let mut events = std::mem::take(&mut chron.events);
    events.sort_by_key(|e| e.year);
    let mut cx = EventCtx::new(&lib, &chron.places, dir, Fields::new(def.size_m, def.sea_level_m), warnings);
    cx.noise_at = Vec2::from(def.noise_at);
    let keys = event_keys(events.iter().map(|e| (e.kind, e.year, e.seed)), def.seeds_v1());
    let mut item_keys: Vec<u32> = Vec::new();
    for (k, ev) in events.iter().enumerate() {
        let cause = (k + 1) as u16;
        if ev.cause.trim().is_empty() {
            cx.warnings.push(format!("event {k} ({}) has no cause", ev.kind));
        }
        // The world seed and the event's key (above).
        cx.year = ev.year;
        cx.cause = cause;
        cx.seed = hash32(def.seed ^ keys[k].wrapping_mul(0x9e37_79b9));
        let before = (cx.placed.len(), cx.items.len());
        let t_ev = std::time::Instant::now();
        let msg = ev.process.apply(&mut cx);
        item_keys.extend(item_keys_for(keys[k], &cx.items[before.1.min(cx.items.len())..]));
        // What each event cost, so a world that takes a quarter of a minute
        // to build says which event spent it.
        let ms = t_ev.elapsed().as_secs_f32() * 1000.0;
        log.push(format!(
            "{:>6}  {:<10} {:>7.0} ms  {}{}",
            ev.year,
            ev.kind,
            ms,
            msg,
            if cx.placed.len() > before.0 || cx.items.len() > before.1 {
                format!(" [+{} structures, +{} items]", cx.placed.len() - before.0, cx.items.len() - before.1)
            } else {
                String::new()
            }
        ));
    }
    // The stages after the chronicle, timed the same way the events are: on a
    // big world most of the build is here, not in the events.
    let mut stages: Vec<(&str, f32)> = Vec::new();
    let mut stage = {
        let mut last = std::time::Instant::now();
        move |name: &'static str, into: &mut Vec<(&'static str, f32)>| {
            into.push((name, last.elapsed().as_secs_f32() * 1000.0));
            last = std::time::Instant::now();
        }
    };
    // The land along each neighbour's edge, blended to meet it.
    for (side, nb) in def.neighbours.list() {
        if !edges.iter().any(|e| e.side == side) {
            log.push(format!("  seam  built without its {} neighbour ({}): the edge is left as the chronicle made it", side.name(), nb.world));
        }
    }
    for e in edges {
        let m = super::seam::blend(&mut cx.f, &lib, e, hash32(def.seed ^ 0x5ea3));
        log.push(format!(
            "  seam  {} edge against {}: before blending the land missed by {:.2} m at worst, {:.2} m on average; sea and land disagreed on {} of {} cells",
            e.side.name(),
            e.world,
            m.max_step,
            m.mean_step,
            m.coast_cells,
            m.cells
        ));
    }
    // A spawn on land the generator shaped is given by name (`spawn_at`).
    let spawn_at = def.spawn_at.clone().map(|p| (cx.pos(&p), cx.turn(&p)));
    let rivers = cx.state.take::<events::river::Rivers>();
    let lakes = cx.state.take::<events::lake::Lakes>();
    // With a tide the rivers run on across the flats it uncovers, down to low
    // water (river.rs `extend_to_low`).
    if let Some(t) = def.tide {
        cx.f.low_water = def.sea_level_m - t.range.max(0.0);
    }
    events::river::settle_water(&mut cx.f, &lakes, &rivers);
    let cenotes = cx.state.take::<events::cenote::Cenotes>();
    events::cenote::settle(&mut cx.f, &cenotes);
    stage("water settling", &mut stages);
    cx.f.derive();
    for e in edges {
        super::seam::fix_distances(&mut cx.f, e);
    }
    if let Some(t) = def.tide {
        cx.f.start_tide(t.range);
        if t.range > 0.0 && !cx.f.tide.on() {
            cx.warnings.push("world.json: a tide, but no open sea reaches the edge of the map, so the water stays still".to_string());
        }
        if t.period_h < 1.0 {
            cx.warnings.push(format!("world.json: tide period_h is {}; an hour is the least", t.period_h));
        }
    }
    if let Some(s) = def.surf {
        cx.f.shore = crate::world::surf::Shore::build(&cx.f, &s, crate::render::HEIGHTS_N as usize);
    }
    stage("fields derive", &mut stages);
    let tubes = cx.state.take::<events::lava_tube::Tubes>().0;
    let towers = cx.state.take::<events::tower::Towers>().0;
    let mut bones = cx.state.take::<crate::world::bones::Bones>();
    bones.index(cx.f.size);
    let growths = cx.state.take::<events::tower::Growths>().0;
    let mut cx_when = std::mem::take(&mut cx.when);
    let city = cx.state.take::<super::city::City>();
    let circuits = cx.state.take::<events::circuit::Circuits>().0;
    let mut ribbons = cx.state.take::<super::route::ribbon::Ribbons>();
    ribbons.index();
    let EventCtx {
        mut f,
        shapes: world_shapes,
        placed,
        items,
        pois,
        areas,
        signs,
        lamps,
        vents,
        mut warnings,
        ..
    } = cx;

    // Biomes.
    let biome_default = lib.biome(&def.default_biome).ok_or(format!("default biome {} not in the library", def.default_biome))?;
    let seabed = def.seabed_biome.as_ref().and_then(|b| lib.biome(b));
    // The world's own biome list; without one every library biome competes.
    let allowed: Vec<bool> = match &def.biomes {
        Some(list) => {
            for n in list {
                if lib.biome(n).is_none() {
                    warnings.push(format!("world biomes: unknown biome {n}"));
                }
            }
            if !list.contains(&def.default_biome) {
                warnings.push(format!("world biomes: the default biome {} is not in the list", def.default_biome));
            }
            lib.biomes.iter().map(|b| list.contains(&b.name)).collect()
        }
        None => vec![true; lib.biomes.len()],
    };
    classify::classify(&mut f, &lib, biome_default, seabed, &allowed);
    stage("biome classify", &mut stages);
    let biome_mats: Vec<BiomeMats> = lib
        .biomes
        .iter()
        .map(|b| BiomeMats {
            surface: lib.material(&b.surface).unwrap_or(1),
            subsurface: lib.material(&b.subsurface).unwrap_or(2),
            rock: b.rock.as_ref().and_then(|r| lib.material(r)).unwrap_or(0),
            fringe: b.fringe.max(0.0),
        })
        .collect();

    // Rock formations by height.
    let mut strata = Vec::new();
    for (name, base) in &def.strata {
        match lib.material(name) {
            Some(m) => strata.push((m, *base)),
            None => warnings.push(format!("world strata: unknown material {name}")),
        }
    }
    strata.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let strata_level: Vec<u8> = def.strata_level.iter().filter_map(|n| lib.material(n)).collect();

    // Bedrock bands.
    let total: f32 = def.rock.iter().map(|r| r.1).sum::<f32>().max(1e-6);
    let mut acc = 0.0;
    let mut rock = Vec::new();
    for (name, w) in &def.rock {
        acc += w / total;
        match lib.material(name) {
            Some(m) => rock.push((m, acc)),
            None => warnings.push(format!("world rock: unknown material {name}")),
        }
    }

    // This world's own grading, if it has any (library::WorldLook); the
    // renderer puts it over each style's look when the world is shown.
    let (look, look_problems) = crate::library::load_world_look(dir);
    warnings.extend(look_problems);

    let doors = lib.materials.iter().filter(|m| m.name.starts_with("door_")).filter_map(|m| lib.material(&m.name)).collect();
    let mut world = World {
        doors,
        spawn: Vec3::ZERO,
        spawn_yaw: 0.0,
        def,
        look,
        dir: dir.to_path_buf(),
        lib,
        f,
        shapes: world_shapes,
        placed,
        placed_grid: HashMap::default(),
        trees: Vec::new(),
        tree_grid: HashMap::default(),
        items,
        item_keys,
        pois,
        areas,
        signs,
        lamps,
        vents,
        biome_mats,
        rock,
        strata,
        strata_level,
        tubes,
        towers,
        bones,
        growths,
        when: std::mem::take(&mut cx_when),
        shown: Vec::new(),
        standing: Vec::new(),
        city,
        ribbons,
        circuits,
        log,
        warnings,
        build_ms: 0.0,
    };
    stage("world assembly", &mut stages);
    flora::reseat_features(&mut world);
    flora::place_features(&mut world);
    stage("features", &mut stages);
    // Decay and gravity, once per structure, before the vines look for walls.
    let maps: Vec<_> = world.placed.iter().map(|p| super::gen::standing_map(&world, p)).collect();
    let (fell, rebuilt, from) = maps.iter().fold((0, 0, 0), |(n, r, k), m| (n + m.1, r + m.2, k + (m.1 + m.2 > 0) as usize));
    if fell + rebuilt > 0 {
        world
            .log
            .push(format!("gravity: {fell} voxels fell and {rebuilt} were put back to hold up the rest, in {from} decayed structures"));
    }
    world.standing = maps.into_iter().map(|m| m.0).collect();
    stage("decay and gravity", &mut stages);
    flora::place_plants(&mut world);
    for e in edges {
        let gone = super::seam::clear_trees(&mut world.trees, e);
        if gone > 0 {
            world.log.push(format!("  seam  {gone} trees taken out of {}'s crowns along the {} edge", e.world, e.side.name()));
        }
    }
    stage("plants", &mut stages);
    // City street trees, at the positions the import gave. Growing one is
    // pure -- the library, a spot and a seed in, a tree out -- and a city has
    // ninety thousand of them, so they are grown in bands across the
    // machine's cores. Which tree goes where is still decided here, in order,
    // so the world comes out the same whatever the band split.
    world.city.index();
    let planted: Vec<(usize, usize, Vec2)> = {
        let mut out = Vec::new();
        for (k, (p, name)) in std::mem::take(&mut world.city.trees).into_iter().enumerate() {
            if world.city.under_deck(p, 6.0) || world.city.near_building(p, 2.0) {
                continue;
            }
            match world.lib.plant(&name) {
                Some(pi) => out.push((k, pi, p)),
                None => world.warnings.push(format!("city tree: unknown plant {name}")),
            }
        }
        out
    };
    {
        let threads = crate::world::fields::build_threads();
        let per = planted.len().div_ceil(threads).max(1);
        let (lib, def, f) = (&world.lib, &world.def, &world.f);
        let mut grown: Vec<Vec<super::gen::Tree>> = Vec::new();
        std::thread::scope(|sc| {
            let mut handles = Vec::new();
            for band in planted.chunks(per) {
                handles.push(sc.spawn(move || {
                    band.iter()
                        .map(|&(k, pi, p)| {
                            let base = Vec3::new(p.x, f.height(p.x, p.y) - 0.1, p.y);
                            flora::grow(
                                lib,
                                &lib.plants[pi],
                                base,
                                hash32(def.seed ^ 0x7bee ^ (k as u32).wrapping_mul(0x9e37_79b9)),
                                flora::coat_at(lib, pi, None, p.x, p.y, def.seed),
                            )
                        })
                        .collect::<Vec<_>>()
                }));
            }
            grown = handles.into_iter().map(|h| h.join().expect("a band of city trees")).collect();
        });
        world.trees.extend(grown.into_iter().flatten());
    }
    stage("city trees", &mut stages);
    flora::place_collectibles(&mut world);
    stage("collectibles", &mut stages);
    world.shown = (0..world.placed.len()).map(|_| std::sync::atomic::AtomicBool::new(true)).collect();
    // Spawn: where the world says, else the first settlement or the centre.
    let sp = spawn_at.map(|s| s.0).or(world.def.spawn.map(|s| Vec2::new(s[0], s[1]))).unwrap_or(Vec2::splat(world.def.size_m * 0.5));
    world.spawn = Vec3::new(sp.x, world.f.height(sp.x, sp.y) + world.def.spawn_up, sp.y);
    // Yaw 0 looks north and turns toward the west as it grows: yaw = -bearing.
    world.spawn_yaw = -(world.def.spawn_bearing + 90.0 * spawn_at.map_or(0, |s| s.1) as f32).to_radians();
    world.index();
    for (name, ms) in stages {
        if ms >= 1.0 {
            world.log.push(format!("        {name:<18} {ms:>7.0} ms"));
        }
    }
    world.build_ms = t0.elapsed().as_secs_f32() * 1000.0;
    Ok(world)
}

#[cfg(test)]
mod key_tests {
    use super::event_keys;

    /// What a chronicle hands `event_keys`: kind, year, and the seed the event
    /// asked for, if any.
    const RIVER: &str = "river";

    fn keys(rows: &[(&'static str, i32, Option<u32>)], v1: bool) -> Vec<u32> {
        event_keys(rows.iter().copied(), v1)
    }

    /// Adding an event to the middle of a chronicle leaves every other
    /// event's seed where it was, which is the whole point of the second
    /// scheme: a new event should change what it does, not the world around
    /// it.
    #[test]
    fn an_event_added_in_the_middle_moves_nothing_else() {
        let before = [("landmass", -9000, None), (RIVER, -3000, None), ("lake", -500, None)];
        let after = [("landmass", -9000, None), ("mesa", -4000, None), (RIVER, -3000, None), ("lake", -500, None)];
        let (a, b) = (keys(&before, false), keys(&after, false));
        assert_eq!(a[0], b[0], "the landmass moved");
        assert_eq!(a[1], b[2], "the river moved");
        assert_eq!(a[2], b[3], "the lake moved");
        // And under the first scheme it moved everything after it, which is
        // why the worlds already made are pinned to it.
        let (a, b) = (keys(&before, true), keys(&after, true));
        assert_eq!(a[1], 2);
        assert_eq!(b[2], 3, "the first scheme counts places, not events");
    }

    /// Two events of the same kind in the same year still get their own
    /// seeds, in the order the chronicle sorted them into.
    #[test]
    fn events_of_a_kind_in_a_year_differ() {
        let k = keys(&[("bloom", -30, None), ("bloom", -30, None), ("bloom", -30, None)], false);
        assert_ne!(k[0], k[1]);
        assert_ne!(k[1], k[2]);
        assert_ne!(k[0], k[2]);
    }

    /// An event that names its own seed gets it, whichever scheme is in use.
    #[test]
    fn a_named_seed_wins() {
        for v1 in [false, true] {
            assert_eq!(keys(&[(RIVER, 0, Some(77))], v1), vec![77]);
        }
    }

    /// The kind and the year both tell events apart.
    #[test]
    fn kind_and_year_both_count() {
        let a = keys(&[(RIVER, -3000, None)], false)[0];
        assert_ne!(a, keys(&[("lake", -3000, None)], false)[0]);
        assert_ne!(a, keys(&[(RIVER, -2999, None)], false)[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::Item;

    fn item(kind: &str, x: f32) -> Item {
        Item {
            kind: kind.into(),
            pos: Vec3::new(x, 0.0, 0.0),
            size: 1.0,
            cause: 1,
            surface: true,
        }
    }

    /// A find keeps its key when it moves, and when another kind of thing
    /// is added to the same event before it.
    #[test]
    fn item_keys_survive_moves_and_other_kinds() {
        let a = item_keys_for(7, &[item("skystone", 1.0), item("skystone", 2.0)]);
        let moved = item_keys_for(7, &[item("skystone", 40.0), item("skystone", 2.0)]);
        let more = item_keys_for(7, &[item("cache", 0.0), item("skystone", 1.0), item("skystone", 2.0)]);
        assert_eq!(a, moved);
        assert_eq!(a[..], more[1..]);
        assert_ne!(a[0], a[1]);
        assert_ne!(a, item_keys_for(8, &[item("skystone", 1.0), item("skystone", 2.0)]));
        assert!(a.iter().all(|&k| k <= i32::MAX as u32));
    }
}
