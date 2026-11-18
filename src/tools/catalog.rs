//! `--catalog DIR`: a review page of one world's content. Every creature,
//! plant, biome and find the world reaches is looked up where the world put
//! it, framed by a camera and rendered headless; DIR gets the pictures, a
//! shaded map, `catalog.js` (what was found, the library entry and what the
//! engine made of it) and `index.html` (src/tools/catalog.html), which reads
//! it from file:// and keeps ratings and notes in the browser.
//!
//! `--match` re-renders only some entries and keeps the rest of an existing
//! catalogue, so one fix can be checked without waiting for the whole world.

use crate::world::flora::Form;
use crate::anim::Plan;
use crate::cli::Args;
use crate::creatures::locomotion::Mover;
use crate::creatures::terrain::voxel_ground;
use crate::entities::Look;
use crate::game::{Actions, Game, FOV_Y};
use crate::player::MoveInput;
use crate::render::hud::Hud;
use crate::render::{Gpu, Renderer};
use crate::world::fields::World;
use crate::world::gen::NO_PLANT;
use crate::{new_game, render, show_world, worldload};
use glam::Vec3;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Everything the renderer needs to take a picture.
struct Studio {
    gpu: Gpu,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    renderer: Renderer,
    hud: Hud,
    w: u32,
    h: u32,
    wait: f32,
}

impl Studio {
    /// Put the eye at `eye` looking at `at`, flying, and load what it sees.
    fn aim(&mut self, game: &mut Game, eye: Vec3, at: Vec3) {
        let d = (at - eye).normalize_or(Vec3::NEG_Z);
        let yaw = (-d.x).atan2(-d.z);
        let pitch = d.y.clamp(-1.0, 1.0).asin();
        game.set_camera(eye, yaw, pitch, true);
        // The camera sits at eye height above the player's feet.
        let off = game.eye(1.0) - game.player.pos;
        game.set_camera(eye - off, yaw, pitch, true);
        self.settle(game);
    }

    fn settle(&mut self, game: &mut Game) {
        let t = Instant::now();
        let mut actions = Actions::default();
        loop {
            game.stream(&mut self.renderer.terrain, Duration::from_millis(50));
            game.tick(0.0, &MoveInput::default(), &mut actions);
            if game.streamer.idle() && t.elapsed().as_millis() > 100 {
                break;
            }
            if t.elapsed().as_secs_f32() > self.wait {
                eprintln!("  gave up waiting for the view to load");
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Render a few frames (so remeshes and shadows land) and write the last.
    fn snap(&mut self, game: &mut Game, path: &Path) {
        let mut actions = Actions::default();
        let aspect = self.w as f32 / self.h as f32;
        for _ in 0..10 {
            game.tick(crate::game::STEP, &MoveInput::default(), &mut actions);
            game.stream(&mut self.renderer.terrain, Duration::from_millis(4));
            self.hud.clear();
            game.prepare_frame(aspect);
            let params = game.frame_params(aspect);
            let stats = self.renderer.render(&game.streamer, &params, &self.hud, &self.view);
            game.last_render = stats;
            self.gpu.device.poll(wgpu::PollType::Wait).expect("poll");
        }
        let pixels = render::read_texture(&self.gpu, &self.target, self.w, self.h);
        crate::util::save_png(path, self.w, self.h, &pixels).expect("catalogue picture");
    }
}

/// One entry on the page.
struct Entry {
    kind: &'static str,
    name: String,
    shots: Vec<(String, String)>,
    facts: Vec<(String, String)>,
    at: Option<[f32; 2]>,
    note: Option<String>,
}

impl Entry {
    fn new(kind: &'static str, name: &str) -> Self {
        Entry {
            kind,
            name: name.to_string(),
            shots: Vec::new(),
            facts: Vec::new(),
            at: None,
            note: None,
        }
    }
    fn fact(&mut self, k: &str, v: impl ToString) {
        self.facts.push((k.to_string(), v.to_string()));
    }
}

/// Where each biome is deepest: the cell farthest from any other biome (and
/// the same over sea deeper than 1.5 m or a river or lake, for swimmers),
/// with that distance in metres.
struct Biomes {
    count: Vec<usize>,
    heart: Vec<Option<(usize, f32)>>,
    wet: Vec<Option<(usize, f32)>>,
}

/// Each age this creature comes in: a label, the individual's own roll, and
/// the `Age` that makes it be drawn as bones.
fn life_stages(game: &crate::game::Game, si: usize) -> Vec<(String, crate::variety::Morph, Option<crate::creatures::age::Age>)> {
    use crate::creatures::age::{Age, Stage};
    let sp = &game.ents.species[si];
    let mut out = Vec::new();
    for (k, stage) in [Stage::Juvenile, Stage::Adult, Stage::Aged].into_iter().enumerate() {
        let has = match stage {
            Stage::Juvenile => sp.ages.has_juvenile(),
            Stage::Aged => sp.ages.aged,
            _ => true,
        };
        if !has {
            continue;
        }
        let mut mo = crate::variety::Morph::roll(&sp.vary, crate::world::noise::hash32(k as u32 + 11));
        mo.age(stage, &sp.ages, sp.aged_tint);
        out.push((stage.name().to_string(), mo, None));
    }
    if sp.ages.bones {
        let mo = crate::variety::Morph::roll(&sp.vary, 17);
        out.push((Stage::Dead.name().to_string(), mo, Some(Age::old_bones(0.0, 1.0))));
    }
    out
}

fn survey(w: &World) -> Biomes {
    let f = &w.f;
    let n = f.n;
    let nb = w.lib.biomes.len().max(1 + f.biome.iter().copied().max().unwrap_or(0) as usize);
    let mut count = vec![0usize; nb];
    for &b in &f.biome {
        count[b as usize] += 1;
    }
    // Chamfer distance (in cells) to the nearest cell of another biome or the
    // world's edge.
    let mut d = vec![f32::MAX; n * n];
    for z in 0..n {
        for x in 0..n {
            let i = z * n + x;
            let b = f.biome[i];
            let edge = x == 0 || z == 0 || x == n - 1 || z == n - 1;
            if edge || f.biome[i - 1] != b || f.biome[i + 1] != b || f.biome[i - n] != b || f.biome[i + n] != b {
                d[i] = 0.0;
            }
        }
    }
    const D: f32 = std::f32::consts::SQRT_2;
    for z in 1..n {
        for x in 1..n - 1 {
            let i = z * n + x;
            d[i] = d[i].min(d[i - 1] + 1.0).min(d[i - n] + 1.0).min(d[i - n - 1] + D).min(d[i - n + 1] + D);
        }
    }
    for z in (0..n - 1).rev() {
        for x in (1..n - 1).rev() {
            let i = z * n + x;
            d[i] = d[i].min(d[i + 1] + 1.0).min(d[i + n] + 1.0).min(d[i + n + 1] + D).min(d[i + n - 1] + D);
        }
    }
    let mut heart: Vec<Option<(usize, f32)>> = vec![None; nb];
    let mut wet: Vec<Option<(usize, f32)>> = vec![None; nb];
    for i in 0..n * n {
        let b = f.biome[i] as usize;
        if heart[b].is_none_or(|(_, v)| d[i] > v) {
            heart[b] = Some((i, d[i]));
        }
        if (f.h[i] < f.sea - 1.5 || f.water[i] == 1) && wet[b].is_none_or(|(_, v)| d[i] > v) {
            wet[b] = Some((i, d[i]));
        }
    }
    let cell = f.size / n as f32;
    let scale = |v: Vec<Option<(usize, f32)>>| v.into_iter().map(|o| o.map(|(i, d)| (i, d * cell))).collect();
    Biomes {
        count,
        heart: scale(heart),
        wet: scale(wet),
    }
}

/// World position of a field cell's centre (x, z).
fn cell_xz(w: &World, i: usize) -> (f32, f32) {
    let n = w.f.n;
    let cell = w.f.size / n as f32;
    (((i % n) as f32 + 0.5) * cell, ((i / n) as f32 + 0.5) * cell)
}

fn heading(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// The heading from (x, z) that stays in biome `b` longest over 60 m.
fn best_heading(w: &World, b: usize, x: f32, z: f32) -> f32 {
    let mut best = (0, 0.0);
    for k in 0..16 {
        let yaw = k as f32 / 16.0 * std::f32::consts::TAU;
        let d = heading(yaw);
        let mut run = 0;
        for s in 1..=30 {
            let p = Vec3::new(x, 0.0, z) + d * (s as f32 * 2.0);
            if p.x < 0.0 || p.z < 0.0 || p.x >= w.f.size || p.z >= w.f.size || w.f.biome[w.f.idx(p.x, p.z)] as usize != b {
                break;
            }
            run = s;
        }
        if run > best.0 {
            best = (run, yaw);
        }
    }
    best.1
}

/// Ground under (x, z) as loaded, else the generator's height. The search
/// starts just above the terrain, so tree crowns don't count as ground.
fn ground(game: &Game, x: f32, z: f32) -> f32 {
    let g = game.generator().height(x, z);
    voxel_ground(&game.store, x, z, g + 1.5).map_or(g, |v| v.0)
}

/// Whether a placed plant is in the loaded world: its bark or leaf material
/// within 2.5 m above its foot.
fn stands(game: &Game, t: &crate::world::gen::Tree) -> bool {
    let v = crate::world::gen::VOXEL;
    let (x, z) = ((t.pos.x / v).floor() as i32, (t.pos.z / v).floor() as i32);
    let y0 = ((t.pos.y - 0.5) / v).floor() as i32;
    (0..)
        .take((3.0 / v) as usize)
        .any(|dy| (-1..=1).any(|dx| (-1..=1).any(|dz| game.store.get(x + dx, y0 + dy, z + dz).is_some_and(|m| m != 0 && (m == t.bark || m == t.leaf)))))
}

/// A camera `d` from `at` with a clear line of sight to it, trying twelve
/// headings starting from `yaw` (the heading the camera looks along).
/// `rise` is the camera's height over `at`; it never goes below `clear`
/// over the ground or the water (pass a very low `sea` to film under it).
/// A hit within `margin` of `at` is the subject itself. Loads the view first so the test sees
/// voxels; falls back to the heading with the longest free line.
#[allow(clippy::too_many_arguments)]
fn frame(st: &mut Studio, game: &mut Game, at: Vec3, d: f32, rise: f32, clear: f32, yaw: f32, sea: f32, margin: f32) -> Vec3 {
    let place = |game: &Game, yaw: f32| {
        let mut c = at - heading(yaw) * d;
        c.y = (at.y + rise).max(ground(game, c.x, c.z).max(sea) + clear);
        c
    };
    let first = place(game, yaw);
    st.aim(game, first, at);
    let mut best = (first, -1.0f32);
    for k in 0..12 {
        // 0, +30, -30, +60, -60 ... degrees from the preferred heading.
        let turn = ((k + 1) / 2) as f32 * if k % 2 == 1 { 1.0 } else { -1.0 } * std::f32::consts::PI / 6.0;
        let c = place(game, yaw + turn);
        let to = at - c;
        let len = to.length();
        let free = game.store.raycast(c, to / len, len).map_or(len, |(hit, _)| {
            (Vec3::new(hit[0] as f32 + 0.5, hit[1] as f32 + 0.5, hit[2] as f32 + 0.5) * crate::world::gen::VOXEL - c).length()
        });
        // Close enough to the subject counts as clear: it may be what was hit.
        if free >= len - margin {
            best = (c, f32::MAX);
            break;
        }
        if free > best.1 {
            best = (c, free);
        }
    }
    if std::env::var("VG_CATALOG_DEBUG").is_ok() {
        eprintln!("  frame at {at:.1} d {d:.1}: cam {:.1} free {:.1}", best.0, best.1);
    }
    if best.0 != first {
        st.aim(game, best.0, at);
    }
    best.0
}

/// The library entries of one kind, by name, as written (with `ops` folded
/// to a count, so the page isn't mostly voxel boxes).
fn library_json(sub: &str) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let dir = crate::library::data_root().join("library").join(sub);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(o) = v.as_object_mut() {
            if let Some(ops) = o.get("ops").and_then(|x| x.as_array()).map(|a| a.len()) {
                o.insert("ops".into(), json!(format!("{ops} shape ops")));
            }
            o.insert("_file".into(), json!(format!("library/{sub}/{}", p.file_name().unwrap().to_string_lossy())));
        }
        if let Some(name) = v.get("name").and_then(|x| x.as_str()) {
            out.insert(name.to_string(), v);
        }
    }
    out
}

fn wanted(args: &Args, kind: &str, name: &str) -> bool {
    let kind_ok = args.only.as_deref().is_none_or(|o| o.split(',').any(|k| k.trim() == kind));
    let name_ok = args.matching.as_deref().is_none_or(|m| m.split(',').any(|k| !k.trim().is_empty() && name.contains(k.trim())));
    kind_ok && name_ok
}

pub fn run(args: &Args) {
    let t_start = Instant::now();
    let dir: PathBuf = args.catalog.clone().unwrap();
    std::fs::create_dir_all(dir.join("img")).expect("catalogue folder");
    // 1600x900 is the headless default; the page wants smaller pictures.
    let (w, h) = if args.size == (1600, 900) { (1024, 640) } else { args.size };
    let gpu = Gpu::new(Gpu::instance(args.dx12), None);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("catalogue"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    // A smaller full-detail radius loads each view faster; the pictures
    // reach past it at lower detail.
    let mut args = args.clone();
    if !args.view_set {
        args.view = 64.0;
    }
    let args = &args;
    let mut game = match new_game(args) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("world {}: {e}", worldload::selected());
            std::process::exit(1);
        }
    };
    game.persist = false;
    game.camera.scrapbook.dir = None;
    let mut renderer = Renderer::new(&gpu, format, w, h, 4);
    show_world(&mut renderer, &game);
    let mut st = Studio {
        gpu,
        target,
        view,
        renderer,
        hud: Hud::default(),
        w,
        h,
        wait: args.wait,
    };
    let Some(world) = game.generator().world.clone() else {
        eprintln!("--catalog needs a library world (not the random island)");
        std::process::exit(1);
    };
    game.freeze_creatures = true;
    game.hour = crate::world::clock::wrap(args.hour.unwrap_or(11.0));
    game.clock_running = false;
    let world_name = worldload::selected();
    let sea = world.f.sea;
    let bio = survey(&world);
    let total: usize = bio.count.iter().sum();
    let img = |kind: &str, name: &str, k: usize| format!("img/{kind}-{name}-{k}.png");
    let mut entries: Vec<Entry> = Vec::new();
    let mut done = 0;
    let progress = |e: &Entry, done: usize| println!("{:>3}  {:<9} {:<24} {:.0} s", done, e.kind, e.name, t_start.elapsed().as_secs_f32());

    // Biomes present, biggest first.
    let mut present: Vec<usize> = (0..bio.count.len()).filter(|&b| bio.count[b] > 0 && b < world.lib.biomes.len()).collect();
    present.sort_by_key(|&b| std::cmp::Reverse(bio.count[b]));
    let biome_lists = |pick: &dyn Fn(&crate::library::BiomeDef) -> Vec<String>, name: &str| -> Vec<String> {
        present
            .iter()
            .filter(|&&b| pick(&world.lib.biomes[b]).iter().any(|x| x == name))
            .map(|&b| world.lib.biomes[b].name.clone())
            .collect()
    };

    // ---- Biomes: a ground view along the longest run inside, and an aerial.
    for &b in &present {
        let def = &world.lib.biomes[b];
        if !wanted(args, "biomes", &def.name) {
            continue;
        }
        let mut e = Entry::new("biomes", &def.name);
        let (i, depth) = bio.heart[b].unwrap();
        let (x, z) = cell_xz(&world, i);
        e.at = Some([x, z]);
        e.fact(
            "area",
            format!(
                "{:.1}% of the world ({:.2} km²)",
                100.0 * bio.count[b] as f32 / total as f32,
                bio.count[b] as f32 * (world.f.size / world.f.n as f32).powi(2) / 1e6
            ),
        );
        e.fact("deepest point", format!("{depth:.0} m from its edge at ({x:.0}, {z:.0})"));
        let yaw = best_heading(&world, b, x, z);
        let fwd = heading(yaw);
        let gh = game.generator().height(x, z);
        let eye = Vec3::new(x, gh.max(sea) + 1.7, z);
        st.aim(&mut game, eye, eye + fwd * 30.0 + Vec3::Y * (-3.0));
        let gh = ground(&game, x, z);
        let eye = Vec3::new(x, gh.max(sea) + 1.7, z);
        st.aim(&mut game, eye, eye + fwd * 30.0 + Vec3::Y * (-3.0));
        game.ents.world.clear();
        let f = img("biomes", &def.name, 0);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "on the ground".into()));
        let back = depth.clamp(30.0, 140.0) * 1.2;
        let at = Vec3::new(x, gh.max(sea), z) + fwd * (back * 0.3);
        let cam = Vec3::new(x, gh.max(sea), z) - fwd * back + Vec3::Y * back * 0.6;
        st.aim(&mut game, cam, at);
        game.ents.world.clear();
        let f = img("biomes", &def.name, 1);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "from above".into()));
        done += 1;
        progress(&e, done);
        entries.push(e);
    }

    // ---- Plants: the tree of each kind with the most room round it.
    let mut by_plant: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for (k, t) in world.trees.iter().enumerate() {
        if t.plant != NO_PLANT {
            by_plant.entry(t.plant).or_default().push(k);
        }
    }
    let cell = 8.0f32;
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, t) in world.trees.iter().enumerate() {
        grid.entry(((t.pos.x / cell).floor() as i32, (t.pos.z / cell).floor() as i32)).or_default().push(k);
    }
    let near = |p: Vec3, r: f32| -> Vec<usize> {
        let (cx0, cz0) = (((p.x - r) / cell).floor() as i32, ((p.z - r) / cell).floor() as i32);
        let (cx1, cz1) = (((p.x + r) / cell).floor() as i32, ((p.z + r) / cell).floor() as i32);
        let mut v = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                if let Some(l) = grid.get(&(cx, cz)) {
                    v.extend(l.iter().copied().filter(|&k| (world.trees[k].pos - p).with_y(0.0).length() < r));
                }
            }
        }
        v
    };
    let mut placed_plants = std::collections::HashSet::new();
    for (&pi, list) in &by_plant {
        let Some(def) = world.lib.plants.get(pi as usize) else {
            continue;
        };
        placed_plants.insert(def.name.clone());
        if !wanted(args, "plants", &def.name) {
            continue;
        }
        let mut e = Entry::new("plants", &def.name);
        // Room round each candidate: the gap to the nearest other crown.
        let step = (list.len() / 300).max(1);
        let mut ranked: Vec<(usize, f32)> = Vec::new();
        for &k in list.iter().step_by(step) {
            let t = &world.trees[k];
            if t.pos.y < sea - 0.5 && def.form != Form::Kelp && !def.name.contains("kelp") {
                continue;
            }
            let mut room = 12.0f32;
            for o in near(t.pos, 14.0) {
                if o != k {
                    let ot = &world.trees[o];
                    room = room.min((ot.pos - t.pos).with_y(0.0).length() - ot.radius - t.radius);
                }
            }
            ranked.push((k, room));
        }
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        // Later passes (paths, structures, decay) can clear a placed plant
        // away: take the roomiest one whose bark or leaf is really there.
        let mut k = ranked.first().map_or(list[0], |b| b.0);
        for &(c, _) in ranked.iter().take(6) {
            let t = &world.trees[c];
            st.aim(&mut game, t.pos + Vec3::new(3.0, 2.0, 3.0), t.pos);
            if stands(&game, t) {
                k = c;
                break;
            }
        }
        let t = &world.trees[k];
        // Herbs are sprites on one marker voxel: their tree has no height,
        // so take the library's, and look down on them.
        let herb = t.top < 0.05;
        let tall = if herb { (def.height[0] + def.height[1]) * 0.5 } else { t.top }.max(0.3);
        let r = t.radius.max(0.2);
        let heights: Vec<f32> = list.iter().map(|&k| world.trees[k].top).collect();
        let (lo, hi) = heights.iter().fold((f32::MAX, 0.0f32), |a, &v| (a.0.min(v), a.1.max(v)));
        e.at = Some([t.pos.x, t.pos.z]);
        // Herbs are sprites on a marker voxel: their height isn't in the tree.
        if hi > 0.05 {
            e.fact("in this world", format!("{} placed, {:.1} to {:.1} m tall", list.len(), lo, hi));
        } else {
            e.fact("in this world", format!("{} placed", list.len()));
        }
        e.fact("shown", format!("{:.1} m tall, crown {r:.1} m, at ({:.0}, {:.0})", t.top, t.pos.x, t.pos.z));
        e.fact("form", &def.form);
        let bs = biome_lists(&|b| b.plants.iter().map(|u| u.plant.clone()).collect(), &def.name);
        if !bs.is_empty() {
            e.fact("biomes", bs.join(", "));
        }
        // Fit the plant into the frame and look from the least crowded side.
        let fit = (tall * 0.5 + 0.3) / (FOV_Y * 0.5).tan();
        let d = fit.max(r * 2.2).max(1.6) + 0.5;
        let mid = t.pos + Vec3::Y * tall * 0.45;
        let mut look = (usize::MAX, 0.0f32);
        for s in 0..8 {
            let yaw = s as f32 / 8.0 * std::f32::consts::TAU + 0.3;
            let cam = t.pos - heading(yaw) * d;
            let blocking = near(cam.lerp(t.pos, 0.5), d * 0.6).into_iter().filter(|&o| o != k).count();
            let wet = world.f.h[world.f.idx(cam.x, cam.z)] < sea;
            let score = blocking * 2 + wet as usize * 3;
            if score < look.0 {
                look = (score, yaw);
            }
        }
        let near_m = r + 0.4;
        frame(&mut st, &mut game, mid, d, if herb { d * 0.7 } else { 0.0 }, 0.8, look.1, sea, near_m);
        game.ents.world.clear();
        let f = img("plants", &def.name, 0);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "close".into()));
        frame(&mut st, &mut game, mid, d * 1.8 + 4.0, 1.0 + tall * 0.3, 1.7, look.1, sea, near_m);
        game.ents.world.clear();
        let f = img("plants", &def.name, 1);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "with its neighbours".into()));
        done += 1;
        progress(&e, done);
        entries.push(e);
    }
    // Plants a biome here lists that never got placed.
    for &b in &present {
        for u in &world.lib.biomes[b].plants {
            if !placed_plants.contains(&u.plant) && wanted(args, "plants", &u.plant) && !entries.iter().any(|e| e.kind == "plants" && e.name == u.plant) {
                let mut e = Entry::new("plants", &u.plant);
                e.note = Some(format!("listed by {} but none placed in this world", world.lib.biomes[b].name));
                entries.push(e);
            }
        }
    }

    // ---- Creatures: in their habitat, three-quarters on, then side-on
    // against the sky so nothing hides the model.
    let mut names: Vec<String> = Vec::new();
    for &b in &present {
        for c in &world.lib.biomes[b].creatures {
            if !names.contains(&c.creature) {
                names.push(c.creature.clone());
            }
        }
    }
    for name in &names {
        if !wanted(args, "creatures", name) {
            continue;
        }
        let mut e = Entry::new("creatures", name);
        let Some(si) = game.ents.species.iter().position(|s| &s.name == name) else {
            e.note = Some("listed by a biome but not loaded as a species".into());
            entries.push(e);
            continue;
        };
        let sp = &game.ents.species[si];
        let model = sp.model;
        let m = &game.models[model];
        let verb = sp.verb();
        let mover = verb.mover;
        let bs = biome_lists(&|b| b.creatures.iter().map(|u| u.creature.clone()).collect(), name);
        e.fact("movement", format!("{} ({mover:?})", verb.name));
        // The hours it keeps, and how much of it is abroad outside them.
        e.fact("active", format!("{}, {:.0}% of it abroad outside those hours", sp.beh.period().name(), sp.beh.off_hours() * 100.0));
        if let Some(rig) = &m.rig {
            e.fact("rig", format!("{} plan, {} bones", rig.plan.map_or("no", |p| p.name()), rig.bones.len()));
        }
        e.fact("size", format!("{:.2} m tall, {:.2} m wide, voxel {:.3} m", m.height, m.half_w * 2.0, m.voxel));
        if let Some(l) = &sp.like {
            e.fact("like", l);
        }
        if !sp.morphs.is_empty() {
            e.fact(
                "morphs",
                sp.morphs
                    .iter()
                    .map(|(i, c)| format!("{} {:.1}%", game.ents.species[*i].name, c * 100.0))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        // Which ages it comes in (creatures/age.rs).
        {
            let a = &sp.ages;
            let mut has: Vec<String> = Vec::new();
            if a.has_juvenile() {
                has.push(format!("juvenile at x{:.2}", a.juvenile));
            }
            has.push("adult".into());
            if a.aged {
                has.push("aged".into());
            }
            has.push(if a.bones { "leaves bones".into() } else { "leaves nothing".to_string() });
            if sp.beh.sleeps_on_side {
                has.push("sleeps on its side".into());
            }
            e.fact("ages", has.join(", "));
        }
        let bird = m.rig.as_ref().is_some_and(|r| r.plan == Some(Plan::Bird));
        // Just the sizes: the model itself stays borrowed from the game otherwise.
        let (height, half_w) = (m.height, m.half_w);
        let swims = matches!(mover, Mover::School | Mover::Breach);
        let floats = matches!(mover, Mover::Paddle);
        // Habitat: the heart of its largest biome (over deep water for swimmers).
        let home = present
            .iter()
            .copied()
            .filter(|&b| bs.contains(&world.lib.biomes[b].name))
            .find_map(|b| if swims || floats { bio.wet[b].or(bio.heart[b]) } else { bio.heart[b] });
        let Some((i, _)) = home else { continue };
        let (x, z) = cell_xz(&world, i);
        e.at = Some([x, z]);
        let size = (height * 1.3).max(half_w * 2.5).max(0.05);
        // Insects are a few centimetres: no fixed allowance.
        let d = size * 1.5 + (size * 0.5).min(0.4);
        let yaw = 0.8f32;
        let gh = game.generator().height(x, z);
        st.aim(&mut game, Vec3::new(x, gh.max(sea) + 2.0, z) - heading(yaw) * d, Vec3::new(x, gh, z));
        let gy = ground(&game, x, z);
        let level = game.water_at(Vec3::new(x, gy, z)).0;
        let lift = match mover {
            Mover::Buzz { .. } => 0.5,
            Mover::Circle { .. } | Mover::Flock { soar: true } => size,
            _ => 0.0,
        };
        let wet = level > gy + 0.3;
        let foot = if swims && wet {
            (level - 0.4 - height).max(gy + 0.1)
        } else if floats && wet {
            level
        } else {
            gy.max(level) + lift
        };
        let at = Vec3::new(x, foot, z);
        let mid = at + Vec3::Y * height * 0.5;
        // Swimmers are filmed level with them, under the water.
        let (rise, clear, floor) = if swims && wet {
            (0.0, 0.1, f32::MIN)
        } else {
            (if height < 0.8 { d * 0.9 } else { size * 0.35 }, 0.15, level)
        };
        // (Small animals from above: grass a metre tall hides them side-on.)
        let cam = frame(&mut st, &mut game, mid, d, rise, clear, yaw, floor, 0.6);
        let look = mid - cam;
        let cam_yaw = (-look.x).atan2(-look.z);
        game.ents.world.clear();
        game.ents.world.spawn((Look::new(model, at, cam_yaw + std::f32::consts::PI + 0.7),));
        let f = img("creatures", name, 0);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, if swims && wet { "in the water" } else { "in its habitat" }.into()));
        // Side-on, floating a little above the eye so the sky is behind it.
        let body = Vec3::new(x, gy.max(level) + 1.7 + d * 0.3, z);
        let cam = frame(&mut st, &mut game, body + Vec3::Y * height * 0.5, d * 1.2, -d * 0.3, 1.5, yaw, level, 0.6);
        let look = body - cam;
        let cam_yaw = (-look.x).atan2(-look.z);
        game.ents.world.clear();
        game.ents.world.spawn((Look::new(model, body, cam_yaw + std::f32::consts::FRAC_PI_2 + if bird { 0.7 } else { 0.0 }),));
        let f = img("creatures", name, 1);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "side-on".into()));
        // The ages in a row beside each other, in the same light.
        let stages = life_stages(&game, si);
        if stages.len() > 1 {
            let span = d * 1.05 * stages.len() as f32;
            let body = Vec3::new(x, gy.max(level) + 1.7 + d * 0.3, z);
            let cam = frame(&mut st, &mut game, body + Vec3::Y * height * 0.5, span * 0.78, -d * 0.3, 1.5, yaw, level, 0.6);
            let look = body - cam;
            let cam_yaw = (-look.x).atan2(-look.z);
            let right = Vec3::new(cam_yaw.cos(), 0.0, -cam_yaw.sin());
            let face = cam_yaw + std::f32::consts::FRAC_PI_2 + if bird { 0.7 } else { 0.0 };
            game.ents.world.clear();
            let labels: Vec<String> = stages.iter().map(|(l, _, _)| l.clone()).collect();
            for (k, (_, morph, age)) in stages.iter().enumerate() {
                let off = (k as f32 - (stages.len() as f32 - 1.0) * 0.5) * d * 1.05;
                let e2 = game.ents.world.spawn((Look::new(model, body + right * off, face),));
                let _ = game.ents.world.insert_one(e2, morph.clone());
                if let Some(a) = age {
                    let _ = game.ents.world.insert_one(e2, *a);
                }
            }
            let f = img("creatures", name, 2);
            st.snap(&mut game, &dir.join(&f));
            e.shots.push((f, labels.join(", ")));
            game.ents.world.clear();
        }
        done += 1;
        progress(&e, done);
        entries.push(e);
    }

    // ---- Finds: one of each kind where the world put it.
    let mut kinds: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (k, it) in world.items.iter().enumerate() {
        kinds.entry(it.kind.clone()).or_default().push(k);
    }
    for (kind, list) in &kinds {
        if !wanted(args, "finds", kind) {
            continue;
        }
        let mut e = Entry::new("finds", kind);
        let surface = list.iter().filter(|&&k| world.items[k].surface).count();
        e.fact("in this world", format!("{} ({} on the surface, {} buried)", list.len(), surface, list.len() - surface));
        let k = list.iter().copied().find(|&k| world.items[k].surface).unwrap_or(list[0]);
        let it = &world.items[k];
        e.at = Some([it.pos.x, it.pos.z]);
        e.fact("size", format!("{:.2} m", it.size));
        let (model, display) = game.ents.item_model(kind).unwrap_or((crate::models::GEM_M, kind.clone()));
        if &display != kind {
            e.fact("display", display);
        }
        let yaw = 0.5f32;
        let d = (it.size * 3.0).max(0.45);
        st.aim(&mut game, it.pos - heading(yaw) * d + Vec3::Y * d * 0.8, it.pos);
        let pos = if it.surface { Vec3::new(it.pos.x, ground(&game, it.pos.x, it.pos.z), it.pos.z) } else { it.pos };
        frame(&mut st, &mut game, pos + Vec3::Y * it.size * 0.3, d, d * 0.8, 0.3, yaw, sea, 0.3);
        game.ents.world.clear();
        game.ents.world.spawn((Look::new(model, pos, 0.4),));
        let f = img("finds", kind, 0);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, if it.surface { "where it lies" } else { "buried (the ground may hide it)" }.into()));
        let body = pos + Vec3::Y * 1.85;
        frame(&mut st, &mut game, body, 0.5, -0.15, 1.5, yaw, sea, 0.3);
        game.ents.world.clear();
        game.ents.world.spawn((Look::new(model, body, 0.4),));
        let f = img("finds", kind, 1);
        st.snap(&mut game, &dir.join(&f));
        e.shots.push((f, "against the sky".into()));
        done += 1;
        progress(&e, done);
        entries.push(e);
    }

    // ---- The page.
    // The game's copy has dropped the fields the map needs; build another.
    match worldload::load(&world_name) {
        Ok(l) => {
            if let Err(err) = crate::world::atlas::write(&l.world, &dir.join("map.png"), 1024) {
                eprintln!("map: {err}");
            }
        }
        Err(err) => eprintln!("map: {err}"),
    }
    let libs: HashMap<&str, HashMap<String, Value>> = [
        ("creatures", library_json("creatures")),
        ("plants", library_json("plants")),
        ("biomes", library_json("biomes")),
        ("finds", library_json("collectibles")),
    ]
    .into_iter()
    .collect();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut out: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "kind": e.kind,
                "name": e.name,
                "shots": e.shots.iter().map(|(f, l)| json!({"file": f, "label": l})).collect::<Vec<_>>(),
                "facts": e.facts.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                "at": e.at,
                "note": e.note,
                "entry": libs.get(e.kind).and_then(|l| l.get(&e.name)),
                "stamp": stamp,
            })
        })
        .collect();
    // With --match or --only, keep what an earlier run rendered for the rest.
    let js = dir.join("catalog.js");
    if args.matching.is_some() || args.only.is_some() {
        if let Some(old) = std::fs::read_to_string(&js)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches("window.CATALOG = ").trim_end().trim_end_matches(';')).ok())
        {
            if old["world"] == json!(world_name) {
                let fresh: std::collections::HashSet<(String, String)> = entries.iter().map(|e| (e.kind.to_string(), e.name.clone())).collect();
                let kept = old["entries"].as_array().cloned().unwrap_or_default().into_iter().filter(|v| {
                    let key = (v["kind"].as_str().unwrap_or("").to_string(), v["name"].as_str().unwrap_or("").to_string());
                    !fresh.contains(&key) && !(wanted(args, key.0.as_str(), &key.1) && args.matching.is_none())
                });
                let mut all: Vec<Value> = kept.collect();
                all.append(&mut out);
                out = all;
            }
        }
    }
    let order = |k: &str| ["biomes", "plants", "creatures", "finds"].iter().position(|x| *x == k).unwrap_or(9);
    out.sort_by(|a, b| {
        order(a["kind"].as_str().unwrap_or(""))
            .cmp(&order(b["kind"].as_str().unwrap_or("")))
            .then(a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let manifest = json!({
        "world": world_name,
        "size_m": world.f.size,
        "sea_m": sea,
        "hour": game.hour,
        "generated": stamp,
        "entries": out,
    });
    std::fs::write(&js, format!("window.CATALOG = {};\n", serde_json::to_string(&manifest).unwrap())).expect("catalog.js");
    std::fs::write(dir.join("index.html"), include_str!("catalog.html")).expect("index.html");
    println!("{} entries in {:.0} s -> {}", entries.len(), t_start.elapsed().as_secs_f32(), dir.join("index.html").display());
}
