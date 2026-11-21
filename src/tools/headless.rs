//! Rendering without a window (--headless): screenshots, stats, ride demos,
//! scripted digging and the anim sheet.

use crate::cli::Args;
use crate::game::{Actions, Game};
use crate::menu::{self, Menu, Place, Prefs, Slides};
use crate::player::MoveInput;
use crate::render::hud::Hud;
use crate::render::Renderer;
use crate::tools::bench::Bench;
use crate::{game, new_game, render::Gpu, ride, screenshot, show_world};
use crate::{render, worldload};
use glam::Vec3;
use std::time::{Duration, Instant};

/// Ride for a few seconds before a headless screenshot, printing what happens.
pub fn ride_demo(game: &mut Game, renderer: &mut Renderer, kind: &str, args: &Args) {
    use ride::Mode;
    let wing = kind == "wing" || kind == "launch";
    game.player.fly = false;
    game.spawned = true;
    let mut r = std::mem::take(&mut game.ride);
    if kind == "launch" {
        game.player.body.on_ground = true;
        r.glide_key(&mut game.player);
    } else if wing {
        game.player.body.vel = game.player.forward() * 18.0;
        r.launch_wing(&mut game.player);
    } else if kind == "kayak" {
        let level = game.water_at(game.player.pos).0;
        r.toggle_kayak(&mut game.player, &game.store, level);
    } else if kind == "rope" || kind == "climb" {
        // The rope is thrown on the first ticks, at whatever the view is on;
        // climbing just walks (W) into whatever is ahead.
    } else {
        game.player.body.on_ground = true;
        r.toggle_bike(&mut game.player);
    }
    r.set_chase(args.chase);
    game.ride = r;
    let input = MoveInput {
        forward: !wing,
        sprint: args.boost,
        down: args.tuck,
        ..Default::default()
    };
    let mut actions = Actions::default();
    let secs = if args.ride_secs > 0.0 { args.ride_secs } else { 4.0 };
    let ticks = (secs / game::STEP) as i32;
    // Vertical jolt while on the ground: RMS of the change in vertical speed
    // per tick, m/s per tick (how bumpy the ride is).
    let (mut ys, mut jolts) = (Vec::new(), Vec::new());
    for i in 0..ticks {
        // Z gets the rope out, Z again throws it.
        let inp = MoveInput {
            hook: kind == "rope" && (i == 0 || i == 2),
            ..input
        };
        game.tick(game::STEP, &inp, &mut actions);
        game.stream(&mut renderer.terrain, Duration::from_millis(3));
        ys.push(game.player.pos.y);
        if ys.len() >= 3 && game.player.body.on_ground && i > 30 {
            let n = ys.len();
            jolts.push((ys[n - 1] - 2.0 * ys[n - 2] + ys[n - 3]) / game::STEP);
        }
        if i % 30 == 0 {
            let p = &game.player;
            let r = &game.ride;
            let mode = match r.mode {
                Mode::Bike => "bike",
                Mode::Wing => "wing",
                Mode::Climb => "climb",
                Mode::Swing => "rope",
                Mode::Foot => "foot",
                Mode::Kayak => "kayak",
            };
            let soar = &r.soar;
            let score = match soar.hud(r.mode) {
                Some((line, _)) => format!("   {line}  near {:?}", soar.near.map(|d| (d * 10.0).round() / 10.0)),
                None => String::new(),
            };
            println!(
                "t {:4.1}  {mode}  pos {:6.1} {:5.1} {:6.1}  {}{score}",
                i as f32 * game::STEP,
                p.pos.x,
                p.pos.y,
                p.pos.z,
                r.hud(p).unwrap_or_default()
            );
        }
    }
    if !jolts.is_empty() {
        let rms = (jolts.iter().map(|j| j * j).sum::<f32>() / jolts.len() as f32).sqrt();
        let big = jolts.iter().filter(|j| j.abs() > 0.05).count();
        println!("ride jolt: rms {rms:.4} m/s per tick over {} ground ticks, {big} over 0.05", jolts.len());
    }
}

pub fn run_headless(args: Args) {
    let t_start = Instant::now();
    let gpu = Gpu::new(Gpu::instance(args.dx12), None);
    let (w, h) = args.size;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
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
    // "pause" and the LOOK page are drawn over a world, so they fall through.
    if let Some(m) = args.show_menu.as_deref().filter(|m| *m != "pause" && !m.starts_with("look")) {
        menu_shot(&gpu, &target, &view, format, &args, m);
        return;
    }
    let mut game = match new_game(&args) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("world {}: {e}", worldload::selected());
            std::process::exit(1);
        }
    };
    game.persist = false;
    // A headless run is a measurement, so it has to repeat.
    game.repeat = true;
    game.gpu_name = gpu.describe();
    let t_game = Instant::now();
    let mut renderer = Renderer::new(&gpu, format, w, h, args.msaa.unwrap_or(4));
    show_world(&mut renderer, &game);
    let init_ms = t_game.elapsed().as_secs_f32() * 1000.0;
    let mut hud = Hud::default();
    let input = MoveInput::default();
    let mut actions = Actions::default();
    let mut bench = if args.bench { Some(Bench::new(game.player.pos + Vec3::Y * 2.0)) } else { None };

    // Load until the streamer has nothing left to do for this view. The benchmark
    // loads its starting view the same way, then flies with streaming live.
    if let Some(b) = &bench {
        let (p, yaw, pitch) = b.camera(&game);
        game.set_camera(p, yaw, pitch, true);
    }
    let t_load = Instant::now();
    let mut first_walkable = None;
    {
        loop {
            game.stream(&mut renderer.terrain, Duration::from_millis(50));
            game.tick(0.0, &input, &mut actions);
            if first_walkable.is_none() && game.spawned {
                first_walkable = Some(t_load.elapsed().as_secs_f32());
            }
            if game.streamer.idle() && t_load.elapsed().as_millis() > 100 {
                break;
            }
            if t_load.elapsed().as_secs_f32() > args.wait {
                eprintln!("gave up waiting for the view to load");
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let load_s = t_load.elapsed().as_secs_f32();
    if let Some(kind) = &args.ride {
        ride_demo(&mut game, &mut renderer, kind, &args);
    }
    game.freeze_creatures = args.showcase || args.anim_sheet.is_some() || args.gun_sheet;
    let mut gun_sheet = None;
    if args.gun_sheet {
        // The camera is moved back against a wall and pinned level and
        // flying before the sheet is hung: the grid is then square to the
        // lens with nothing but wall behind it, and the body under it
        // cannot settle, duck or fall during the frames that follow.
        let yaw = game.player.yaw;
        let eye = crate::combat::sheet::place(&game.store, game.eye(1.0), yaw);
        game.set_camera(eye, yaw, 0.0, true);
        let eye = game.eye(1.0);
        let base = game.weapon_models;
        let names = crate::combat::sheet::names(game.library());
        let sheet = crate::combat::sheet::spawn(&mut game.ents, names, base, eye, yaw);
        sheet.lines().iter().for_each(|l| println!("{l}"));
        gun_sheet = Some(sheet);
    }
    if args.bot_sheet {
        let eye = game.eye(1.0);
        let yaw = game.player.yaw;
        let mut palette = crate::models::palette();
        let av = crate::brain::avatar::build(game.library(), &mut palette);
        let sheet = crate::brain::sheet::spawn(&av, eye, yaw);
        sheet.lines().iter().for_each(|l| println!("{l}"));
        let mut out = Vec::new();
        sheet.instances(&av, game.weapon_models, &game.models, &mut out);
        game.extra_inst = out;
    }
    if let Some(name) = &args.anim_sheet {
        let eye = game.eye(1.0);
        let yaw = game.player.yaw;
        match crate::anim::sheet::spawn(&mut game.ents, &game.models, name, eye, yaw) {
            Ok(lines) => lines.iter().for_each(|l| println!("{l}")),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    // `--edit NAME[:page]`: the EDIT page over a studio, for a screenshot of
    // the same page the window draws (src/editor).
    let mut edit = None;
    if let Some(want) = args.edit.as_deref() {
        // NAME[:PAGE[:world]] -- the group to show, and "world" for the
        // world's own ground in place of the grey grid.
        let mut parts = want.split(':');
        let name = parts.next().unwrap_or_default();
        let at: u8 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        // The third part is "world" for the world's own ground, or "drop"
        // to leave a list dropped open under the entry row.
        let rest: Vec<&str> = parts.collect();
        let ground = rest.contains(&"world");
        let dropped = rest.contains(&"drop");
        let name = (!name.is_empty()).then_some(name);
        let aspect = w as f32 / h.max(1) as f32;
        match crate::editor::open(&mut game, name, aspect) {
            Ok((mut p, mut st)) => {
                if ground {
                    p.grid = false;
                    crate::editor::floor(&p, &mut st, &mut game);
                }
                // The platform and the animal both wait on ground that has
                // only just streamed in, and the platform's remesh takes a
                // few frames to come back.
                if at > 0 {
                    // `--edit NAME:3` opens that heading on its own.
                    p.only_section(at as usize);
                }
                let mut pair = (p, st);
                // Dev hooks for the studio's camera and the subject's pose,
                // so a screenshot can show the page from another side or the
                // animal doing something: VG_EDIT_POSE names a pose
                // (editor/pose.rs) and VG_EDIT_ORBIT turns the camera that
                // many degrees, with an optional pitch and distance after it.
                if let Ok(want) = std::env::var("VG_EDIT_POSE") {
                    if let Some(found) = pair.0.poses().into_iter().find(|p| p.name() == want) {
                        pair.0.pose = found;
                        crate::editor::apply(&mut pair.0, &mut pair.1, &mut game, aspect);
                    } else {
                        println!("EDIT: no pose called {want}");
                    }
                }
                if let Ok(spec) = std::env::var("VG_EDIT_ORBIT") {
                    let v: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    let d2r = std::f32::consts::PI / 180.0;
                    pair.1.cam.turn(v.first().copied().unwrap_or(0.0) * d2r, v.get(1).copied().unwrap_or(0.0) * d2r);
                    if let Some(k) = v.get(2) {
                        pair.1.cam.dist *= k;
                    }
                }
                // Four seconds of game time before the shot, or as long as
                // VG_EDIT_SECS asks: long enough to see whether a tethered
                // animal is still where the page put it.
                let secs: f32 = std::env::var("VG_EDIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
                for _ in 0..(secs.clamp(0.1, 600.0) * 60.0) as u32 {
                    crate::editor::tick(&mut pair.0, &mut pair.1, &mut game, aspect, game::STEP);
                    crate::upload_models(&mut renderer, &mut game);
                    game.tick(game::STEP, &input, &mut actions);
                    game.stream(&mut renderer.terrain, Duration::from_millis(4));
                }
                crate::upload_models(&mut renderer, &mut game);
                // The studio's own camera already frames the subject
                // (editor/cam.rs); the page is drawn from wherever it has
                // been turned to.
                for _ in 0..60 {
                    crate::editor::tick(&mut pair.0, &mut pair.1, &mut game, aspect, game::STEP);
                    crate::upload_models(&mut renderer, &mut game);
                    game.tick(game::STEP, &input, &mut actions);
                    game.stream(&mut renderer.terrain, Duration::from_millis(4));
                }
                // Dev hook: VG_EDIT_STEP="KNOB,DIR,TIMES" steps one of the
                // entry's fields before the shot, so a screenshot can show an
                // edit rather than only the entry as it is on disk.
                // Dev hook: VG_EDIT_POSE2 switches pose once the subject is
                // standing, which is the path a click on the POSE row takes.
                if let Ok(want) = std::env::var("VG_EDIT_POSE2") {
                    let was = pair.1.subject_entity(&game);
                    match pair.0.poses().into_iter().find(|p| p.name() == want) {
                        Some(found) => {
                            pair.0.pose = found;
                            pair.0.wants(crate::editor::Impact::Row);
                            crate::editor::settle(&mut pair.0, &mut pair.1, &mut game, aspect);
                            let now = pair.1.subject_entity(&game);
                            println!(
                                "EDIT: posing as {want} is Row; the individual {}",
                                match (was, now) {
                                    (Some(a), Some(b)) if a == b => "stayed",
                                    (_, Some(_)) => "was spawned again",
                                    (_, None) => "is not there",
                                }
                            );
                            for _ in 0..60 {
                                crate::editor::tick(&mut pair.0, &mut pair.1, &mut game, aspect, game::STEP);
                                crate::upload_models(&mut renderer, &mut game);
                                game.tick(game::STEP, &input, &mut actions);
                                game.stream(&mut renderer.terrain, Duration::from_millis(4));
                            }
                        }
                        None => println!("EDIT: no pose called {want}"),
                    }
                }
                // Dev hook: VG_EDIT_PAINT="SECTION,INDEX,R,G,B" paints one of
                // the entry's colours (0..255), which is what the picker does
                // as it is dragged, and says what it cost.
                if let Ok(spec) = std::env::var("VG_EDIT_PAINT") {
                    let v: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if v.len() >= 5 {
                        let was = pair.1.subject_entity(&game);
                        pair.0.set_colour(v[0] as u8, v[1] as usize, [v[2] / 255.0, v[3] / 255.0, v[4] / 255.0]);
                        let how = pair.0.pending;
                        crate::editor::settle(&mut pair.0, &mut pair.1, &mut game, aspect);
                        let now = pair.1.subject_entity(&game);
                        println!(
                            "EDIT: painting is {how:?}; the individual {}",
                            match (was, now) {
                                (Some(a), Some(b)) if a == b => "stayed",
                                (_, Some(_)) => "was spawned again",
                                (_, None) => "is not there",
                            }
                        );
                        for _ in 0..30 {
                            crate::editor::tick(&mut pair.0, &mut pair.1, &mut game, aspect, game::STEP);
                            crate::upload_models(&mut renderer, &mut game);
                            game.tick(game::STEP, &input, &mut actions);
                            game.stream(&mut renderer.terrain, Duration::from_millis(4));
                        }
                    }
                }
                if let Ok(spec) = std::env::var("VG_EDIT_STEP") {
                    let v: Vec<i32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if let (Some(&k), Some(&dir)) = (v.first(), v.get(1)) {
                        let lib = game.materials_only();
                        let was = pair.1.subject_entity(&game);
                        for _ in 0..v.get(2).copied().unwrap_or(1) {
                            pair.0.step(k as usize, dir, &lib);
                        }
                        let how = pair.0.pending;
                        // Through the same door the window uses, so what is
                        // measured here is what a keypress really costs.
                        crate::editor::settle(&mut pair.0, &mut pair.1, &mut game, aspect);
                        let now = pair.1.subject_entity(&game);
                        println!(
                            "EDIT: knob {k} is {how:?}; the individual {}",
                            match (was, now) {
                                (Some(a), Some(b)) if a == b => "stayed",
                                (_, Some(_)) => "was spawned again",
                                (_, None) => "is not there",
                            }
                        );
                        for _ in 0..90 {
                            crate::editor::tick(&mut pair.0, &mut pair.1, &mut game, aspect, game::STEP);
                            crate::upload_models(&mut renderer, &mut game);
                            game.tick(game::STEP, &input, &mut actions);
                            game.stream(&mut renderer.terrain, Duration::from_millis(4));
                        }
                        println!("EDIT: stepped knob {k} by {dir}: {}", pair.0.shown(k as usize));
                    }
                }
                println!(
                    "EDIT: {} ({}) {}",
                    pair.0.name(),
                    pair.0.kind.label(),
                    match (pair.0.kind, pair.1.species.is_some()) {
                        (crate::editor::Kind::Material, _) => "as a block on the platform",
                        (crate::editor::Kind::Plant, _) => "grown on the platform",
                        (_, true) => "in the studio",
                        (_, false) if pair.1.prop.is_some() => "standing on the platform",
                        (_, false) => "nothing standing",
                    }
                );
                if let Some(m) = &pair.0.msg {
                    println!("EDIT: {m}");
                }
                edit = Some((pair.0, pair.1, at, dropped));
            }
            Err(e) => {
                eprintln!("--edit: {e}");
                std::process::exit(1);
            }
        }
    }
    if let Some(h) = args.hour {
        game.hour = crate::world::clock::wrap(h);
        game.clock_running = false;
    }
    if let Some(w) = args.weather.as_deref() {
        match crate::world::weather::Kind::parse(w) {
            // A headless shot has no time to crossfade, so the kind is
            // arrived at rather than set and drifted into.
            Some(k) => game.weather = crate::world::weather::Weather::new(k, Default::default(), 0, false),
            None => {
                eprintln!("--weather: no such weather as {w}");
                std::process::exit(1);
            }
        }
    }
    if args.screenshot_mode {
        // Into screenshot mode, so --screenshot shows its overlay and the saved image doesn't.
        let mut a = Actions {
            screenshot_toggle: true,
            ..Default::default()
        };
        game.tick(0.0, &input, &mut a);
    }
    // Runs that mustn't write keep no scrapbook, unless --save names a test folder.
    if args.save_dir == std::path::Path::new("saves") {
        game.camera.scrapbook.dir = None;
    }
    if args.camera || args.camera_out || args.shoot.is_some() {
        let mut a = Actions {
            photography: true,
            ..Default::default()
        };
        game.tick(0.0, &input, &mut a);
        let mut a = Actions {
            dig: !args.camera_out || args.shoot.is_some(),
            ..Default::default()
        };
        game.tick(0.0, &input, &mut a);
        if let Some(l) = &args.lens {
            set_lens(&mut game.camera, l);
        }
    }
    if let Some(page) = args.book {
        game.camera.book = Some(page.min(game.camera.scrapbook.pages()));
        game.camera.page_dirty = true;
    }
    if args.showcase {
        let eye = game.eye(1.0);
        let yaw = game.player.yaw;
        game.ents.showcase(&game.store, eye, yaw);
    }
    // Out of the match: without this, a headless run measures two bots
    // against the body standing at the spawn rather than against each other,
    // and the statue is both a free kill and a reason to stop walking the
    // map (`Shooter::bench_player`).
    if args.no_player {
        match game.activity.as_mut().and_then(|a| a.shooter_mut()) {
            Some(sh) => sh.bench_player(),
            None => eprintln!("--no-player: this world is not running a match"),
        }
    }
    // Scripted edits at the crosshair, each followed by a few frames so remeshes land.
    if let Some(n) = args.weapon {
        let mut a = Actions {
            select: Some(n.saturating_sub(1)),
            ..Default::default()
        };
        game.tick(game::STEP, &input, &mut a);
    }
    let mut edits_done = 0;
    for k in 0..(args.dig + args.place) {
        let mut a = Actions {
            dig: k < args.dig,
            place: k >= args.dig,
            ..Default::default()
        };
        game.tick(0.3, &input, &mut a);
        for _ in 0..20 {
            game.stream(&mut renderer.terrain, Duration::from_millis(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        edits_done += 1;
    }

    let frames = if bench.is_some() {
        60 * 30
    } else if let Some(t) = args.tour {
        (t * 60.0) as u32
    } else {
        args.frames
    };
    if args.tour.is_some() {
        game.ents.met = Some(Default::default());
    }
    let mut tour_t = 0.0f32;
    let mut frame_times = Vec::new();
    let mut last = FrameTiming::default();
    let mut parts = [0.0f32; 4];
    let mut parts_max = [0.0f32; 4];
    for frame in 0..frames {
        let ft = Instant::now();
        // The trigger goes down for the last `--fire` steps, so the shot
        // catches tracers in the air rather than an empty room.
        actions.dig = args.fire > 0 && frame + args.fire >= frames;
        actions.place = args.alt > 0 && frame + args.alt >= frames;
        if let Some(b) = &mut bench {
            let (p, yaw, pitch) = b.camera(&game);
            game.set_camera(p, yaw, pitch, true);
            b.t += game::STEP;
        }
        if args.tour.is_some() {
            // A circle 30% of the world's width out from its middle, at 5 m/s.
            let w = game.generator().spec.size_m;
            let r = w * 0.3;
            let a = tour_t * 5.0 / r;
            let (x, z) = (w * 0.5 + a.cos() * r, w * 0.5 + a.sin() * r);
            let y = game.generator().height(x, z).max(game.generator().spec.sea_level_m) + 2.0;
            game.set_camera(Vec3::new(x, y, z), (-(-a.sin())).atan2(-a.cos()), -0.1, true);
            tour_t += game::STEP;
        }
        game.tick(game::STEP, &input, &mut actions);
        let t1 = ft.elapsed().as_secs_f32();
        game.stream(&mut renderer.terrain, Duration::from_millis(4));
        let t2 = ft.elapsed().as_secs_f32();
        if args.hud {
            game.show_help = args.show_help;
            // A --hud shot is a debug shot, so the minimap comes with it.
            game.settings.minimap = true;
            game.build_hud(&mut hud, w as f32, h as f32);
            if args.show_worlds {
                let here = worldload::selected();
                let mut l = crate::launch::Launch::default();
                l.world = here.clone();
                l.mode = crate::launch::modes(&here).last().copied().unwrap_or_default();
                let items = menu::launch_items(&l, Some(&here));
                menu::draw_menu(
                    &mut hud,
                    &mut Menu::default(),
                    "OPEN WORLD",
                    &items,
                    "LEFT AND RIGHT TO CHANGE, ENTER TO GO",
                    Place::Centre,
                    w as f32,
                    h as f32,
                );
            }
            if let Some((page, _, _at, dropped)) = &mut edit {
                hud.clear();
                let rows = page.rows();
                let title = page.title();
                let foot = page.foot();
                let mut m = Menu::default();
                crate::editor::panel::draw(&mut hud, &mut m, page, &rows, &title, &foot, w as f32, h as f32);
                // The entry list dropped open under its row, as a click on it
                // leaves the page in the window.
                if *dropped {
                    if let Some(lib) = game.library() {
                        let which = crate::editor::DROP_ENTRY;
                        let list = page.drop_items(which, lib);
                        let in_force = page.drop_at(which, lib);
                        let anchor = m.drops.get(1).copied().unwrap_or([0.0; 4]);
                        let mut open = Menu {
                            at: in_force,
                            top: in_force.saturating_sub(2),
                            ..Menu::default()
                        };
                        menu::draw_dropdown(&mut hud, &mut open, &list, anchor, in_force, w as f32, h as f32);
                    }
                }
            }
            if args.show_menu.as_deref() == Some("pause") {
                hud.clear();
                menu::draw_menu(&mut hud, &mut Menu::default(), "PAUSED", &menu::pause_items(), "ESC TO RESUME", Place::Centre, w as f32, h as f32);
            }
            // The LOOK page over the world: "look", or a group by name, or
            // "look-pick" with the picker open on the first ramp stop.
            if let Some(rest) = args.show_menu.as_deref().and_then(|m| m.strip_prefix("look")) {
                hud.clear();
                let style = game.settings.style;
                let look = renderer.look_of(style);
                let page = match rest {
                    "-light" => 1,
                    "-tint" => 3,
                    "-map" => 4,
                    "-grade" => 5,
                    "" => 0,
                    _ => 2,
                };
                let items = if rest == "-pick" {
                    crate::lookpage::pick_items([38.0, 0.43, 0.99], crate::lookpage::stop_distance(renderer.grade_of(style), crate::lookpage::SW_RAMP))
                } else {
                    crate::lookpage::items(page, &look, renderer.grade_of(style))
                };
                let title = if rest == "-pick" {
                    "RAMP STOP 1".to_string()
                } else {
                    crate::lookpage::page_title(page).to_string()
                };
                let foot = if rest == "-pick" {
                    "Drag in the square, the hue bar or the distance bar; Esc when it is right"
                } else {
                    crate::lookpage::page_foot(page)
                };
                crate::lookpage::draw(&mut hud, &mut Menu::default(), &title, &items, foot, w as f32, h as f32);
                if rest == "-pick" {
                    let g = renderer.grade_of(style);
                    let cursor = (w as f32 * 0.66, h as f32 * 0.52);
                    let probe = crate::lookpage::probe(&game, cursor, w as f32, h as f32);
                    let bar = crate::lookpage::stop_distance(g, crate::lookpage::SW_RAMP).map(|at| crate::lookpage::DistBar::new(g, at, probe));
                    crate::lookpage::draw_picker(&mut hud, [38.0, 0.43, 0.99], &title, bar, w as f32, h as f32);
                }
                // The cursor's range, as the page shows it; headless has no
                // mouse, so put it two thirds across the frame.
                let cursor = (w as f32 * 0.66, h as f32 * 0.52);
                if let Some(d) = crate::lookpage::probe(&game, cursor, w as f32, h as f32) {
                    crate::lookpage::draw_probe(&mut hud, renderer.grade_of(style), d, cursor, w as f32, h as f32);
                }
            }
        } else {
            hud.clear();
        }
        // The sheet names its own rows whether or not the game's HUD is on.
        if let Some(sheet) = &gun_sheet {
            sheet.label(&mut hud, w as f32, h as f32, game.fov());
        }
        if let Some(img) = game.take_map_image() {
            renderer.upload_map(&img);
        }
        let stats = {
            let aspect = w as f32 / h as f32;
            game.prepare_frame(aspect);
            let params = game.frame_params(aspect);
            renderer.render(&game.streamer, &params, &hud, &view)
        };
        game.last_render = stats.clone();
        let t3 = ft.elapsed().as_secs_f32();
        gpu.device.poll(wgpu::PollType::Wait).expect("poll");
        let ms = ft.elapsed().as_secs_f32() * 1000.0;
        let split = [t1 * 1000.0, (t2 - t1) * 1000.0, (t3 - t2) * 1000.0, ms - t3 * 1000.0];
        if ms > 16.0 && std::env::var("VOXEL_TRACE").is_ok() {
            let st = &game.streamer.stats;
            eprintln!(
                "slow frame {:.1} ms: tick {:.1} stream {:.1} (accept {:.1} x{} plan {:.1} vis {:.1} unload {:.1} x{}) submit {:.1} gpu {:.1} | shadow quads {} main quads {} | queued {} busy {}",
                ms,
                split[0],
                split[1],
                st.accept_ms,
                st.accepted,
                st.plan_ms,
                st.vis_ms,
                st.unload_ms,
                st.unloaded,
                split[2],
                split[3],
                game.last_render.shadow_quads,
                game.last_render.quads,
                st.queued,
                st.busy
            );
        }
        for k in 0..4 {
            parts[k] += split[k];
            parts_max[k] = parts_max[k].max(split[k]);
        }
        frame_times.push(ms);
        if let Some(b) = &mut bench {
            b.times.push(ms);
            let st = &game.streamer.stats;
            if st.queued + st.busy > 0 {
                b.loading_frames += 1;
            }
            b.max_queue = b.max_queue.max(st.queued + st.busy);
        }
        last = FrameTiming { stats };
    }

    if args.screenshot_mode {
        let path = screenshot::next_path(std::path::Path::new("screenshots"), &worldload::selected());
        match screenshot::capture(&gpu, &mut renderer, &game, format, w, h, path).join() {
            Ok(Ok(p)) => println!("screenshot saved to {}", p.display()),
            Ok(Err(e)) => eprintln!("screenshot failed: {e}"),
            Err(_) => eprintln!("screenshot writer panicked"),
        }
    }
    if let Some(path) = &args.shoot {
        shoot(&gpu, &mut renderer, &mut game, format, w, h, path, args.keep);
    }
    if let Some(path) = &args.screenshot {
        let pixels = render::read_texture(&gpu, &target, w, h);
        crate::util::save_png(path, w, h, &pixels).expect("screenshot file");
    }
    let mut sorted = frame_times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let st = &game.streamer.stats;
    let p = game.player.pos;
    let memory = {
        let (data, uniform) = game.store.census();
        let w = game.generator().world.as_ref();
        let fields = w.map_or(0, |w| w.f.bytes());
        let trees = w.map_or(0, |w| w.tree_bytes());
        serde_json::json!({ "voxel_chunks_full": data, "voxel_chunks_uniform": uniform, "voxel_mb": data as f64 * 32768.0 / 1048576.0, "fields_mb": fields as f64 / 1048576.0, "trees_mb": trees as f64 / 1048576.0 })
    };
    let mut report = serde_json::json!({
        "gpu": gpu.describe(),
        "msaa": renderer.msaa(),
        "size": [w, h],
        "camera": [p.x, p.y, p.z, game.player.yaw.to_degrees(), game.player.pitch.to_degrees()],
        "startup_ms": init_ms,
        "load_s": load_s,
        "walkable_after_s": first_walkable,
        "total_s": t_start.elapsed().as_secs_f32(),
        "frames": frame_times.len(),
        "frame_ms_avg": frame_times.iter().sum::<f32>() / frame_times.len().max(1) as f32,
        "frame_ms_p50": sorted.get(sorted.len() / 2).copied().unwrap_or(0.0),
        "frame_ms_max": sorted.last().copied().unwrap_or(0.0),
        "render_cpu_ms": last.stats.cpu_ms,
        "draws": last.stats.draws,
        "shadow_draws": last.stats.shadow_draws,
        "quads_drawn": last.stats.quads,
        "shadow_quads": last.stats.shadow_quads,
        "chunks_visible": last.stats.chunks_visible,
        "columns": st.columns,
        "chunks": st.chunks,
        "quads_resident": st.quads,
        "per_lod": st.per_lod,
        "gen_jobs": st.gen_jobs,
        "gen_ms_per_column": if st.gen_jobs > 0 { st.gen_micros as f64 / st.gen_jobs as f64 / 1000.0 } else { 0.0 },
        "chunks_generated": st.chunks_generated,
        "gpu_quads_used": renderer.terrain.used_quads(),
        "gpu_chunk_slots": renderer.terrain.slots_used(),
        "gpu_free_ranges": renderer.terrain.free_ranges(),
        "creatures": game.ents.critter_count(),
        "creature_activities": game.ents.activities(),
        "creature_pathing": game.ents.pathing,
        "swimmers": game.ents.swim,
        "creature_census": game.ents.census(),
        "scripted_edits": edits_done,
        "bench": bench.as_ref().map(|b| b.report()),
        "split_avg_ms": { "tick": parts[0] / frames.max(1) as f32, "stream": parts[1] / frames.max(1) as f32, "render_submit": parts[2] / frames.max(1) as f32, "gpu_wait": parts[3] / frames.max(1) as f32 },
        "split_max_ms": { "tick": parts_max[0], "stream": parts_max[1], "render_submit": parts_max[2], "gpu_wait": parts_max[3] },
    });
    report["memory"] = memory;
    // Outside json!: one more key there passes its recursion limit.
    report["drawlist_ms"] = serde_json::json!(last.stats.drawlist_ms);
    report["sprites_drawn"] = last.stats.sprites.into();
    // How the match went, when one was being played: the one way to see
    // from outside whether the bots actually fought (src/shooter).
    if let Some(sh) = game.activity.as_ref().and_then(|a| a.shooter()) {
        report["shooter"] = serde_json::json!({
            "nav_nodes": sh.nav().len(),
            "match_s": sh.elapsed(),
            "pads_standing": sh.pads_standing(),
            "scores": sh.scores().iter().map(|(n, c)| serde_json::json!({ "who": n, "kills": c.kills, "deaths": c.deaths, "shots": c.shots, "hits": c.hits, "dealt": c.dealt, "scalded": c.scalded, "scald_deaths": c.scald_deaths })).collect::<Vec<_>>(),
            "bots": sh.bot_report(),
            "nav_audit": sh.nav_audit(),
        });
    }
    // Where every platform ended up, how far it travelled and how often it
    // was called: whether a lift worked (src/platforms.rs).
    if !game.parts.platforms.is_empty() {
        report["platforms"] = game.parts.platforms.report();
    }
    // And every teleporter and jump pad, with how often it was used: one
    // nobody can reach looks exactly like one that works (src/portals.rs).
    if !game.parts.portals.is_empty() {
        report["portals"] = game.parts.portals.report();
    }
    let cr = renderer.cascade_redraws();
    report["cascade_redraws"] = serde_json::json!([cr[0], cr[1]]);
    let (level, current) = game.water_at(p);
    if let (Some(t), Some(met)) = (args.tour, game.ents.met.as_ref()) {
        let counts: std::collections::BTreeMap<&String, usize> = met.iter().map(|(k, v)| (k, v.len())).collect();
        report["tour"] = serde_json::json!({ "seconds": t, "met": counts });
    }
    report["player"] = serde_json::json!({ "pos": [p.x, p.y, p.z], "swimming": game.player.swimming, "water_level": level, "current": [current.x, current.y] });
    report["camera_tasks"] = game
        .camera
        .tasks
        .iter()
        .map(|t| serde_json::json!({ "title": t.title, "needs": t.needs.iter().map(|n| n.line()).collect::<Vec<_>>(), "reward": t.reward }))
        .collect::<Vec<_>>()
        .into();
    let text = serde_json::to_string_pretty(&report).unwrap();
    match &args.stats {
        Some(path) => std::fs::write(path, &text).expect("stats file"),
        None => println!("{text}"),
    }
}

#[derive(Default)]
pub struct FrameTiming {
    stats: render::FrameStats,
}

/// "portrait,12,2.8,85": frame, focus (m), f-number, focal length (mm); any
/// part may be left out.
fn set_lens(cam: &mut crate::camera::Camera, spec: &str) {
    use crate::camera::lens::{Orient, APERTURES};
    let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
    if let Some(o) = parts.first() {
        cam.orient = match *o {
            "portrait" => Orient::Portrait,
            "square" => Orient::Square,
            _ => Orient::Landscape,
        };
    }
    if let Some(f) = parts.get(1).and_then(|x| x.parse::<f32>().ok()) {
        cam.lens.focus_m = f;
    }
    if let Some(n) = parts.get(2).and_then(|x| x.parse::<f32>().ok()) {
        cam.lens.stop = APERTURES.iter().enumerate().min_by(|a, b| (a.1 - n).abs().total_cmp(&(b.1 - n).abs())).map_or(3, |(i, _)| i);
    }
    if let Some(mm) = parts.get(3).and_then(|x| x.parse::<f32>().ok()) {
        cam.lens.focal_mm = mm;
    }
}

/// Fire the camera at the current view, develop the photo, write it to
/// `path` and print what it saw and how it did against each task.
#[allow(clippy::too_many_arguments)]
fn shoot(gpu: &Gpu, renderer: &mut Renderer, game: &mut Game, format: wgpu::TextureFormat, w: u32, h: u32, path: &std::path::Path, keep: bool) {
    let tasks = game.camera.tasks.clone();
    game.camera.film = game.camera.film.max(1);
    let mut a = Actions { dig: true, ..Default::default() };
    game.tick(0.0, &MoveInput::default(), &mut a);
    game.prepare_frame(w as f32 / h.max(1) as f32);
    let Some(plan) = game.take_shot(w, h) else {
        eprintln!("the shutter didn't fire");
        return;
    };
    if let Some((shot, _, _)) = &game.camera.judged {
        println!(
            "photo at {:.1} h, {:?}, {:.0} mm f/{} focus {:.1} m, water {}",
            shot.hour,
            shot.orient,
            shot.lens.focal_mm,
            shot.lens.f_number(),
            shot.lens.focus_m,
            shot.water
        );
        // Everything in the frame, and animals near it.
        for s in shot.seen.iter().filter(|s| s.in_frame() || s.doing.is_some()) {
            println!(
                "  {}{:<20} {:>5.1} m  at ({:.2}, {:.2})  size {:.2}  elev {:+.0}  {}{}{}  {:?}  bg blur {:.1}",
                if s.in_frame() { "" } else { "(out) " },
                s.name,
                s.dist,
                s.u,
                s.v,
                s.size,
                s.elev,
                if s.visible { "seen" } else { "hidden" },
                if s.sharp { " sharp" } else { " soft" },
                if s.sky_behind { " sky" } else { "" },
                s.doing,
                s.bg_blur
            );
        }
        for t in &tasks {
            match t.check(shot) {
                Ok(()) => println!("  task {:<36} DONE", t.title),
                Err(m) => println!("  task {:<36} missing: {}", t.title, m.join("; ")),
            }
        }
    }
    let (px, cw, ch) = crate::screenshot::develop(gpu, renderer, game, format, w, h, &plan);
    if let Err(e) = crate::screenshot::write_png(px.clone(), cw, ch, false, path.to_path_buf()).join().unwrap() {
        eprintln!("photo not written: {e}");
    }
    game.developed(px, cw, ch);
    if keep {
        let mut a = Actions {
            screenshot_take: true,
            ..Default::default()
        };
        game.tick(0.0, &MoveInput::default(), &mut a);
    }
    for f in game.files.drain(..) {
        match f.join() {
            Ok(Ok(p)) => println!("scrapbook: {}", p.display()),
            Ok(Err(e)) => eprintln!("scrapbook: {e}"),
            Err(_) => eprintln!("scrapbook writer panicked"),
        }
    }
}

/// A menu screen with no world behind it (--show-menu title, settings,
/// worlds or loading), written to --screenshot.
fn menu_shot(gpu: &Gpu, target: &wgpu::Texture, view: &wgpu::TextureView, format: wgpu::TextureFormat, args: &Args, which: &str) {
    let (w, h) = args.size;
    let (wf, hf) = (w as f32, h as f32);
    let _ = worldload::library();
    let mut renderer = Renderer::new(gpu, format, w, h, 4);
    let world = worldload::selected();
    // The loading and launch screens show the world on the line; the rest
    // show every world's pictures (src/main.rs `show_pictures`).
    let one = matches!(which, "loading" | "launch" | "launch-open");
    let pics = if one { menu::picture_paths(Some(&world)) } else { menu::picture_paths(None) };
    println!("{} pictures", pics.len());
    let mut slides = Slides::new(pics, 1);
    slides.wait_first(&renderer);
    let mut hud = Hud::default();
    slides.draw(&mut hud, wf, hf, 0.0);
    let prefs = Prefs::default();
    let mut m = Menu::default();
    match which {
        "loading" => menu::draw_loading(&mut hud, &world, "GROWING THE VIEW", Some(0.4), wf, hf, 0.0),
        "settings" => {
            m.at = 4;
            menu::draw_menu(
                &mut hud,
                &mut m,
                "SETTINGS",
                &menu::settings_items(&prefs, false, renderer.msaa()),
                "Left and right, or a click, to change",
                Place::Left,
                wf,
                hf,
            );
        }
        // "launch-open" shows the world list standing open on it.
        "worlds" | "launch" | "launch-open" => {
            let mut l = crate::launch::Launch::default();
            l.world = world.clone();
            l.mode = crate::launch::modes(&world).last().copied().unwrap_or_default();
            let items = menu::launch_items(&l, None);
            let foot = items.first().map(|i| i.help).unwrap_or("");
            menu::draw_menu(&mut hud, &mut m, "OPEN WORLD", &items, foot, Place::Left, wf, hf);
            if which == "launch-open" {
                let anchor = m.drops.first().copied().unwrap_or([0.0; 4]);
                let at = menu::drop_at(&l, 0);
                let list = menu::drop_items(&l, 0);
                let mut dm = Menu { at, ..Menu::default() };
                menu::dim(&mut hud, wf, hf);
                menu::draw_dropdown(&mut hud, &mut dm, &list, anchor, at, wf, hf);
            }
        }
        // The end-of-match scoreboard over the pictures, with a made-up
        // board: what `--show-menu` is for everywhere else in this match.
        "scoreboard" => {
            use crate::brain::Score;
            let board: Vec<(String, Score)> = [("DROSSEL", 15, 9), ("YOU", 12, 11), ("KILN", 7, 14), ("TUYERE", 4, 16)]
                .iter()
                .map(|(n, k, d)| {
                    (
                        n.to_string(),
                        Score {
                            kills: *k,
                            deaths: *d,
                            ..Default::default()
                        },
                    )
                })
                .collect();
            let mut round = crate::shooter::round::Round::new(15);
            round.after_kill(1, 0, &board);
            round.draw(&mut hud, wf, hf, menu::scale(hf), &board);
        }
        "controls" => {
            crate::game::draw_help(&mut hud, crate::modes::Allowed::all(), wf, hf, menu::scale(hf), "H, Esc or a click to go back");
        }
        _ => {
            menu::draw_menu(&mut hud, &mut m, "", &menu::title_items(&prefs), "", Place::Left, wf, hf);
            menu::draw_logo(&mut hud, wf, hf);
        }
    }
    renderer.render_ui(&hud, view);
    if let Some(path) = &args.screenshot {
        let pixels = render::read_texture(gpu, target, w, h);
        crate::util::save_png(path, w, h, &pixels).expect("screenshot file");
    }
}
