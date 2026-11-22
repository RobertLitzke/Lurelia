//! Tools that inspect a world without playing it: generation profile, map,
//! validation and gazetteer.

use crate::cli::Args;
use crate::{mesh, world, worldload};
use std::time::Instant;

/// report where the time goes.
pub fn profile_gen(args: &Args) {
    use std::sync::atomic::Ordering;
    use world::gen::{ColKey, GenScratch, GEN_NANOS, VPM};
    let gen = match worldload::load(&worldload::selected()) {
        Ok(l) => world::gen::Generator::with_world(l.world),
        Err(_) => world::gen::Generator::new(world::spec::WorldSpec::load_or_create(&args.save_dir.join("random").join("world.json")).expect("world.json")),
    };
    let (sp, _) = gen.spawn_point();
    let (cx, cz) = ((sp.x * VPM) as i32 / 32, (sp.z * VPM) as i32 / 32);
    let mut scratch = GenScratch::default();
    let mut m = mesh::Mesh::default();
    let (mut chunks, mut quads, mut mesh_ns) = (0u64, 0u64, 0u64);
    let t = Instant::now();
    let mut n = 0;
    for dz in -8..8 {
        for dx in -9..10 {
            let key = ColKey { lod: 0, x: cx + dx, z: cz + dz };
            gen.gen_column_with(key, None, &mut scratch, |_, v, _| {
                let tm = Instant::now();
                mesh::mesh_chunk_h(v, !args.blocky, None, true, &mut m);
                mesh_ns += tm.elapsed().as_nanos() as u64;
                chunks += 1;
                quads += m.quads.len() as u64;
            });
            n += 1;
        }
    }
    let total = t.elapsed().as_secs_f64() * 1000.0;
    let ms = |i: usize| GEN_NANOS[i].load(Ordering::Relaxed) as f64 / 1e6;
    println!(
        "{n} columns, {chunks} chunks, {quads} quads in {total:.0} ms ({:.2} ms/column, {:.2} ms/chunk)",
        total / n as f64,
        total / chunks as f64
    );
    println!(
        "  column samples {:.0} ms | tree lookup {:.0} ms | voxel fill {:.0} ms | tree drawing {:.0} ms | meshing {:.0} ms",
        ms(0),
        ms(1),
        ms(2),
        ms(3),
        mesh_ns as f64 / 1e6
    );
}

/// Top-down map for tuning the generator: height-shaded colours, plus a summary.
pub fn write_map(args: &Args, path: &std::path::Path) {
    let spec = world::spec::WorldSpec::load_or_create(&args.save_dir.join("random").join("world.json")).expect("world.json");
    let gen = world::gen::Generator::new(spec.clone());
    let n = 512usize;
    let cell = spec.size_m / n as f32;
    let hs = gen.height_grid(n);
    let sea = spec.sea_level_m;
    let mut img = vec![0u8; n * n * 4];
    let (mut land, mut steep, mut maxh) = (0usize, 0usize, f32::MIN);
    for z in 0..n {
        for x in 0..n {
            let h = hs[z * n + x];
            let hx = hs[z * n + (x + 1).min(n - 1)];
            let hz = hs[(z + 1).min(n - 1) * n + x];
            let slope = ((hx - h).powi(2) + (hz - h).powi(2)).sqrt() / cell;
            maxh = maxh.max(h);
            let shade = (1.0 + ((h - hx) + (h - hz)) * 0.6).clamp(0.5, 1.4);
            let c: [f32; 3] = if h < sea {
                let d = ((sea - h) / 10.0).min(1.0);
                [30.0 + 40.0 * (1.0 - d), 90.0 + 60.0 * (1.0 - d), 140.0 + 40.0 * (1.0 - d)]
            } else {
                land += 1;
                if slope > 1.0 {
                    steep += 1;
                }
                if h < sea + 1.0 {
                    [220.0, 205.0, 150.0]
                } else if slope > 1.0 {
                    [130.0, 128.0, 124.0]
                } else if h > spec.snow_line_m {
                    [240.0, 244.0, 250.0]
                } else {
                    let t = ((h - sea) / 60.0).clamp(0.0, 1.0);
                    [80.0 + 70.0 * t, 140.0 - 20.0 * t, 60.0 + 30.0 * t]
                }
            };
            let i = (z * n + x) * 4;
            for k in 0..3 {
                img[i + k] = (c[k] * shade).clamp(0.0, 255.0) as u8;
            }
            img[i + 3] = 255;
        }
    }
    let (sp, _) = gen.spawn_point();
    let (sx, sz) = ((sp.x / cell) as i32, (sp.z / cell) as i32);
    for d in -4i32..=4 {
        for (x, z) in [(sx + d, sz), (sx, sz + d)] {
            if (0..n as i32).contains(&x) && (0..n as i32).contains(&z) {
                let i = (z as usize * n + x as usize) * 4;
                img[i..i + 3].copy_from_slice(&[255, 0, 0]);
            }
        }
    }
    crate::util::save_png(path, n as u32, n as u32, &img).expect("map file");
    println!(
        "land {:.0}%  steep {:.1}% of land  highest {:.1} m (sea {:.0} m)  spawn {:.0},{:.1},{:.0}",
        land as f32 * 100.0 / (n * n) as f32,
        steep as f32 * 100.0 / land.max(1) as f32,
        maxh,
        sea,
        sp.x,
        sp.y,
        sp.z
    );
}

/// `--gazetteer` and `--validate`: build the selected world and report on it and
/// the library. Returns the exit code (1 when anything is wrong).
pub fn check_world(args: &Args) -> i32 {
    let name = worldload::selected();
    let (edges, seam_problems) = worldload::edges_for(&name);
    let (lib, errors) = worldload::library_for(&name);
    let mut problems: Vec<String> = errors;
    problems.extend(seam_problems);
    problems.extend(lib.validate());
    problems.extend(crate::brain::avatar::build(Some(&lib), &mut crate::models::palette()).problems);
    println!("library: {}; {} texture layers for this world", lib.summary(), world::matlib::layer_count());
    let dir = worldload::world_dir(&name);
    match world::chronicle::build_world_with(&dir, lib, &edges) {
        Ok(w) => {
            if args.gazetteer {
                println!("{}", world::chronicle::gazetteer(&w));
            }
            problems.extend(w.warnings.iter().map(|x| format!("world {name}: {x}")));
            problems.extend(w.def.gravity.problem().map(|x| format!("world {name}: {x}")));
            if let Some(sh) = w.def.shooter.as_ref() {
                problems.extend(crate::shooter::teams::check(sh).into_iter().map(|x| format!("world {name}: {x}")));
                // Every fighter its own spawn: the player and the most bots
                // the launch screen offers.
                if let Some(most) = sh.max_bots {
                    if most + 1 > sh.spawns.len() {
                        problems.push(format!("world {name}: max_bots {most} with the player is {} fighters for {} spawns", most + 1, sh.spawns.len()));
                    }
                    if most > crate::launch::BOT_NAMES.len() {
                        problems.push(format!("world {name}: max_bots {most}, and there are names for only {}", crate::launch::BOT_NAMES.len()));
                    }
                }
            }
            let (_, species, _, colours, colour_problems) = worldload::creatures(&w);
            problems.extend(colour_problems.iter().map(|x| format!("world {name}: {x}")));
            // What a new world feature can still spend: voxel ids (a byte,
            // 17..=254, grain copies included). Creature colours have no
            // budget (each model has a palette block of its own), so they
            // are only counted.
            let ids = (17u8..255).filter(|&m| world::matlib::key(m).is_some()).count();
            println!("world {name}: budgets: {ids} of 238 material ids; {} creature and collectible colours", colours.len());
            let used = worldload::used_entries(&w);
            println!(
                "world {name}: built in {:.0} ms; {} of {} creature species used",
                w.build_ms,
                species.iter().filter(|s| used.contains(&s.name)).count(),
                species.len()
            );
            for fall in &w.f.falls {
                println!(
                    "waterfall: lip ({:.0}, {:.1}, {:.0}), drop {:.1} m, {:.1} m wide, lands at ({:.0}, {:.0})",
                    fall.lip.x,
                    fall.lip.y,
                    fall.lip.z,
                    fall.lip.y - fall.foot,
                    fall.width,
                    fall.splash.x,
                    fall.splash.z
                );
            }
        }
        Err(e) => problems.push(format!("world {name}: {e}")),
    }
    for p in &problems {
        println!("problem: {p}");
    }
    if problems.is_empty() {
        println!("no problems");
        0
    } else {
        1
    }
}

/// `--seam out.png`: build the selected world against its neighbours, say how
/// each edge meets, and write a map of the world and its neighbours side by
/// side at half a pixel a metre (`VG_MAP_PX_PER_M` changes it).
pub fn seam(path: &std::path::Path) -> i32 {
    use glam::Vec2;
    use world::seam::{Edge, Side};
    let name = worldload::selected();
    let (near, problems) = worldload::neighbours_of(&name);
    for p in &problems {
        eprintln!("{p}");
    }
    if near.is_empty() {
        eprintln!("world {name} names no neighbours that build");
        return 1;
    }
    let dir = worldload::world_dir(&name);
    let def: serde_json::Value = std::fs::read_to_string(dir.join("world.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let size = def["size_m"].as_f64().unwrap_or(0.0) as f32;
    let sea = def["sea_level_m"].as_f64().unwrap_or(0.0) as f32;
    let n = (size / world::fields::CELL).round() as usize;
    let edges: Vec<Edge> = near.iter().map(|(s, o, w)| Edge::take(w, *s, *o, n, sea)).collect();
    let (lib, _) = worldload::library_for(&name);
    let w = match world::chronicle::build_world_with(&dir, lib, &edges) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("world {name}: {e}");
            return 1;
        }
    };
    for l in w.log.iter().filter(|l| l.contains("seam")) {
        println!("{}", l.trim());
    }
    for e in &edges {
        println!("{}", world::seam::report(&w, e));
    }
    // Each world's corner in this world's metres, then one canvas round all.
    let per_m: f32 = std::env::var("VG_MAP_PX_PER_M").ok().and_then(|v| v.parse().ok()).unwrap_or(0.5);
    let mut tiles: Vec<(Vec2, &world::fields::World)> = vec![(Vec2::ZERO, &w)];
    for (side, off, far) in &near {
        let fs = far.f.size;
        let at = match side {
            Side::South => Vec2::new(*off, size),
            Side::North => Vec2::new(*off, -fs),
            Side::East => Vec2::new(size, *off),
            Side::West => Vec2::new(-fs, *off),
        };
        tiles.push((at, far));
    }
    let lo = tiles.iter().fold(Vec2::splat(f32::MAX), |a, (p, _)| a.min(*p));
    let hi = tiles.iter().fold(Vec2::splat(f32::MIN), |a, (p, t)| a.max(*p + Vec2::splat(t.f.size)));
    let (cw, ch) = (((hi.x - lo.x) * per_m) as usize, ((hi.y - lo.y) * per_m) as usize);
    let mut canvas = vec![0u8; cw * ch * 4];
    for (at, t) in &tiles {
        let px = (t.f.size * per_m) as usize;
        let img = world::atlas::image(t, px);
        let (ox, oy) = (((at.x - lo.x) * per_m) as usize, ((at.y - lo.y) * per_m) as usize);
        for y in 0..px.min(ch.saturating_sub(oy)) {
            let row = &img[y * px * 4..(y + 1) * px * 4];
            let start = ((oy + y) * cw + ox) * 4;
            let len = (px * 4).min((cw - ox) * 4);
            canvas[start..start + len].copy_from_slice(&row[..len]);
        }
    }
    if let Err(e) = crate::util::save_png(path, cw as u32, ch as u32, &canvas) {
        eprintln!("{}: {e}", path.display());
        return 1;
    }
    println!("wrote {} ({cw} x {ch})", path.display());
    0
}
