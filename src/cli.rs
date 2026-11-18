//! The command line: every flag, its default and the help text.

use crate::render::Style;
use glam::Vec3;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Args {
    pub headless: bool,
    pub size: (u32, u32),
    pub frames: u32,
    pub screenshot: Option<PathBuf>,
    /// (headless) save a screenshot through screenshot mode's capture, into screenshots/.
    pub screenshot_mode: bool,
    pub stats: Option<PathBuf>,
    pub pos: Option<Vec3>,
    pub ground: Option<(f32, f32)>,
    pub look: Option<(f32, f32)>,
    pub wait: f32,
    pub shadows: bool,
    pub view: f32,
    /// --view was given (it then wins over the saved setting).
    pub view_set: bool,
    /// --full-detail: the full-detail radius, metres, instead of
    /// `stream::FULL_DETAIL_M` (never past the view distance).
    pub full_detail: Option<f32>,
    pub save_dir: PathBuf,
    pub fresh: bool,
    pub bench: bool,
    /// (headless) walk a loop round the island for this many seconds and
    /// report every creature met, by species.
    pub tour: Option<f32>,
    pub vsync: bool,
    pub dx12: bool,
    /// Antialiasing samples (1, 2 or 4); the saved setting when left out.
    pub msaa: Option<u32>,
    pub hud: bool,
    pub threads: usize,
    pub map: Option<PathBuf>,
    pub profile_gen: bool,
    pub dig: u32,
    pub place: u32,
    /// (headless) hold the trigger for this many of the last steps, so a
    /// screenshot catches a shooter world firing.
    pub fire: u32,
    /// Steps of held right button before the screenshot (a shooter world).
    pub alt: u32,
    /// (headless) take out the weapon on this digit before firing.
    pub weapon: Option<usize>,
    /// (headless) take the player out of a shooter match, so what is
    /// measured is the bots against each other rather than the bots against
    /// a body standing at the spawn (`Shooter::bench_player`).
    pub no_player: bool,
    pub showcase: bool,
    /// (headless) a rigged creature's bones bent one by one (src/anim/sheet.rs).
    pub anim_sheet: Option<String>,
    /// (headless) every weapon view-model from several angles
    /// (src/combat/sheet.rs).
    pub gun_sheet: bool,
    pub style: Option<Style>,
    pub blocky: bool,
    pub show_help: bool,
    pub show_worlds: bool,
    /// (headless) draw a menu screen: title, settings, worlds, loading or pause.
    pub show_menu: Option<String>,
    pub world: Option<String>,
    pub hour: Option<f32>,
    /// Hold the weather at one kind (world/weather.rs): sun, heat, clouds,
    /// rain, snow or fog. It stops drifting, as `--hour` stops the clock.
    pub weather: Option<String>,
    /// Hold the world in one part of the year (world/season.rs): spring,
    /// summer, autumn or winter. Unset, it turns on the clock.
    pub season: Option<String>,
    pub gazetteer: bool,
    pub validate: bool,
    /// Run the bot tournament: how many rounds each entrant plays.
    pub tourney: Option<u32>,
    pub tourney_seconds: Option<f32>,
    /// Rounds of the weapon bench (`tools::duel`).
    pub duel: Option<u32>,
    pub duel_seconds: Option<f32>,
    /// Only this weapon's row of the table, for a quick check on one gun.
    pub duel_weapon: Option<String>,
    /// (headless) ride before the screenshot: "bike" or "wing", for `ride_secs`.
    pub ride: Option<String>,
    pub ride_secs: f32,
    pub chase: bool,
    pub boost: bool,
    /// (headless) hold Ctrl while riding: the bike's tuck.
    pub tuck: bool,
    /// (headless) the camera out with the viewfinder up.
    pub camera: bool,
    /// (headless) the camera out, viewfinder down.
    pub camera_out: bool,
    /// (headless) frame, focus, aperture and focal length: "portrait,12,2.8,85".
    pub lens: Option<String>,
    /// (headless) take a camera photo after the frames and write it here.
    pub shoot: Option<PathBuf>,
    /// (headless) keep a photo that did no task in the scrapbook too.
    pub keep: bool,
    /// (headless) show the logbook (0) or a scrapbook page in the screenshot.
    pub book: Option<usize>,
    /// (headless) render every creature, plant, biome and find of the world
    /// into a review page in this folder (src/tools/catalog.rs).
    pub catalog: Option<PathBuf>,
    /// Open the EDIT page (src/editor) as soon as the world is in, on this
    /// creature or plant; an empty name opens it on the first creature.
    pub edit: Option<String>,
    /// With --catalog: only these kinds (comma list of creatures, plants, biomes, finds).
    pub only: Option<String>,
    /// With --catalog: only entries whose name contains one of these (comma list).
    pub matching: Option<String>,
    /// `--mode`: the way of playing, when the command line names one.
    pub mode: Option<crate::launch::Mode>,
    /// What the launch screen chose: the mode and its options (src/launch.rs).
    /// None on the command line and in the headless tools, where the world's
    /// own defaults stand (`launch::Chosen::for_world`).
    pub launch: Option<crate::launch::Chosen>,
}

const HELP: &str = "\
lurelia [options]

With no options the game opens on the title screen; --world NAME goes straight
into that world.

  --save DIR            save folder (default: saves)
  --fresh               ignore saved edits and progress
  --view METRES         view distance: where the coarse rings end (default 128)
  --full-detail METRES  full-detail radius (default the view distance, at most 128)
  --threads N           chunk worker threads (default: cores - 4, at most 8)
  --vsync               cap to the display refresh rate
  --dx12                use DirectX 12 instead of Vulkan
  --msaa N              antialiasing samples: 1 (off) or 4 (default the saved setting); 2 is
                        still taken here for comparing, though SETTINGS no longer offers it
  --no-shadows          start with shadows off
  --style NAME          classic or calm (default: the saved choice)
  --blocky              square voxel steps instead of slopes
  --bench               fly a fixed route and report frame times
  --tour SECONDS        (headless) walk a loop round the island at walking pace, 2 m up,
                        and report how many of each creature turned up (stats key tour)

Headless (no window):
  --headless            render offscreen
  --size WxH            image size (default 1600x900)
  --pos X,Y,Z           camera position in metres (flying)
  --ground X,Z          stand on the ground at X,Z
  --look YAW,PITCH      degrees; yaw 0 looks north (-Z), 90 looks west
  --frames N            frames to render after loading (default 30)
  --wait SECONDS        longest to wait for the view to load (default 30)
  --screenshot FILE     write the last frame as PNG
  --screenshot-mode     (headless) also save the view through screenshot mode (F2) into screenshots/
  --stats FILE          write timing and streaming stats as JSON
  --no-hud              hide the HUD in screenshots
  --show-help           show the controls screen (H) in the screenshot
  --show-worlds         show the world selector (L) in the screenshot
  --show-menu NAME      (headless) draw a menu screen: title, settings, launch, scoreboard
                        and loading
                        over the title pictures (no world is loaded), pause over the world
                        (--show-menu look, look-light, look-ramp, look-tint, look-map,
                        look-grade and look-pick draw the LOOK page and its colour
                        picker over the world)
  --dig N, --place N    dig or build N times at the crosshair before the screenshot
  --fire N              hold the trigger for the last N steps (a shooter world)
  --alt N               hold the right button for the last N steps: the second fire mode,
                        a charge building, or a sight (a shooter world)
  --weapon N            take out the weapon on digit N first
  --no-player           (headless) take the player out of a match: the bots fight each
                        other instead of the statue standing at the spawn
  --showcase            stand one of each creature model in front of the camera
  --anim-sheet NAME     (headless) a rigged creature with each bone bent in turn
  --gun-sheet           (headless) every weapon from several angles, one sheet
  --map FILE            write a shaded top-down map of the world as PNG and exit
  --profile-gen         time terrain generation and meshing on one thread
  --world NAME          play worlds/NAME or local/worlds/NAME (default glassfall; random = a noise-generated island)
  --mode NAME           how to play it: explore or fps (default: what the world is built for)
  --hour H              time of day, 0 to 24, with the clock stopped (default 15, running)
  --weather KIND        hold the weather at sun, heat, clouds, rain, snow or fog (default: it drifts)
  --season NAME         hold the year at spring, summer, autumn or winter (default: it turns)
  --gazetteer           build the world and print what its chronicle made, then exit
  --validate            check the library and the world, print problems, then exit
  --tourney N           score the bots on how they move, N rounds each (no window)
  --tourney-seconds S   how long each round lasts (default 60)
  --duel N              bench the weapons against each other, N rounds (no window)
  --duel-seconds S      how long each duel lasts (default 45)
  --duel-weapon NAME    only this gun's row of the table
  --ride bike|wing|launch|rope|climb|kayak  (headless) ride for --ride-secs (default 4) before
                        the screenshot: the bike, gliding from --pos, a glide launched from
                        the ground, the rope thrown at the view, walking into a wall, or
                        paddling the kayak from the nearest water
  --camera              (headless) take the camera out with the viewfinder up
  --camera-out          (headless) take the camera out, viewfinder down
  --lens O,FOCUS,F,MM   (headless) frame (landscape, portrait, square), focus metres,
                        f-number and focal length, e.g. portrait,12,2.8,85
  --shoot FILE          (headless) take a camera photo after the frames, write it to FILE
                        and print how it did against the tasks; it goes into the scrapbook
                        only when --save names a folder other than saves
  --keep                (headless) with --shoot, keep a photo that did no task as well
  --book N              (headless) show the logbook (0) or scrapbook page N
  --chase               (headless) chase camera while riding
  --boost               (headless) hold Shift while riding
  --tuck                (headless) hold Ctrl while riding: the bike tucks, which stores
                        a charge on a descent and floats the next jump

Editing (see GUIDE.md):
  --edit NAME[:PAGE[:world]]
                        open the EDIT page on this creature or plant: a studio in
                        front of the camera with the entry beside it. PAGE picks a
                        group of fields, `world` drops the grey grid for the
                        world's own ground

Catalogue (a review page of everything in one world; see GUIDE.md):
  --catalog DIR         render each creature, plant, biome and find of --world into DIR
                        (index.html plus pictures), then exit
  --only KINDS          with --catalog: creatures, plants, biomes and/or finds, comma separated
  --match NAMES         with --catalog: only entries whose name contains one of these;
                        the rest of an existing catalogue in DIR is kept
";

pub fn parse_args() -> Args {
    let mut a = Args {
        headless: false,
        size: (1600, 900),
        frames: 30,
        screenshot: None,
        screenshot_mode: false,
        stats: None,
        pos: None,
        ground: None,
        look: None,
        wait: 30.0,
        shadows: true,
        view: 128.0,
        view_set: false,
        full_detail: None,
        save_dir: PathBuf::from("saves"),
        fresh: false,
        bench: false,
        tour: None,
        vsync: false,
        dx12: false,
        msaa: None,
        hud: true,
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(4).clamp(1, 8),
        map: None,
        profile_gen: false,
        dig: 0,
        fire: 0,
        alt: 0,
        weapon: None,
        no_player: false,
        place: 0,
        showcase: false,
        anim_sheet: None,
        gun_sheet: false,
        style: None,
        blocky: false,
        show_help: false,
        show_worlds: false,
        show_menu: None,
        world: None,
        hour: None,
        weather: None,
        season: None,
        gazetteer: false,
        validate: false,
        tourney: None,
        tourney_seconds: None,
        duel: None,
        duel_seconds: None,
        duel_weapon: None,
        ride: None,
        ride_secs: 4.0,
        chase: false,
        boost: false,
        tuck: false,
        camera: false,
        camera_out: false,
        lens: None,
        shoot: None,
        keep: false,
        book: None,
        edit: None,
        catalog: None,
        only: None,
        matching: None,
        mode: None,
        launch: None,
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let next = |i: &mut usize| -> String {
            *i += 1;
            v.get(*i).cloned().unwrap_or_default()
        };
        // A mistyped value used to fall back to the default in silence, which
        // is how `--frames 10x` measured 30 frames and `--view abc` measured
        // 128 m. Every value goes through one of these instead, and a bad one
        // stops the run with the flag named.
        let flag = v[i].clone();
        let bad = |what: &str| -> ! {
            eprintln!("{flag}: {what}\n\n{HELP}");
            std::process::exit(2)
        };
        let val = |i: &mut usize| -> String {
            let s = next(i);
            if s.is_empty() || s.starts_with("--") {
                bad("needs a value");
            }
            s
        };
        let num = |i: &mut usize| -> f32 {
            let s = val(i);
            match s.parse::<f32>() {
                Ok(n) if n.is_finite() => n,
                _ => bad(&format!("{s} is not a number")),
            }
        };
        let count = |i: &mut usize| -> u32 {
            let n = num(i);
            if n < 0.0 || n.fract() != 0.0 || n > u32::MAX as f32 {
                bad("wants a whole number, zero or more");
            }
            n as u32
        };
        let span = |i: &mut usize, lo: f32, hi: f32| -> f32 {
            let n = num(i);
            if n < lo || n > hi {
                bad(&format!("wants {lo} to {hi}, not {n}"));
            }
            n
        };
        // `n` numbers separated by commas, all of them required.
        let list = |i: &mut usize, n: usize| -> Vec<f32> {
            let s = val(i);
            let out: Vec<f32> = s.split(',').map(|x| x.trim().parse::<f32>().unwrap_or(f32::NAN)).collect();
            if out.len() != n || out.iter().any(|x| !x.is_finite()) {
                bad(&format!("wants {n} numbers separated by commas, not {s}"));
            }
            out
        };
        match v[i].as_str() {
            "--headless" => a.headless = true,
            "--size" => {
                let s = val(&mut i);
                let p: Vec<u32> = s.split('x').map(|x| x.trim().parse::<u32>().unwrap_or(0)).collect();
                if p.len() != 2 || p[0] == 0 || p[1] == 0 {
                    bad(&format!("wants WIDTHxHEIGHT, not {s}"));
                }
                a.size = (p[0], p[1]);
            }
            "--frames" => a.frames = count(&mut i),
            "--screenshot" => a.screenshot = Some(val(&mut i).into()),
            "--screenshot-mode" => a.screenshot_mode = true,
            "--stats" => a.stats = Some(val(&mut i).into()),
            "--pos" => {
                let n = list(&mut i, 3);
                a.pos = Some(Vec3::new(n[0], n[1], n[2]));
            }
            "--ground" => {
                let n = list(&mut i, 2);
                a.ground = Some((n[0], n[1]));
            }
            "--look" => {
                let n = list(&mut i, 2);
                a.look = Some((n[0], n[1]));
            }
            "--wait" => a.wait = span(&mut i, 0.0, 3600.0),
            "--no-shadows" => a.shadows = false,
            "--view" => {
                a.view = span(&mut i, 16.0, 4096.0);
                a.view_set = true;
            }
            "--full-detail" => a.full_detail = Some(span(&mut i, 16.0, 4096.0)),
            "--save" => a.save_dir = val(&mut i).into(),
            "--fresh" => a.fresh = true,
            "--bench" => a.bench = true,
            "--tour" => a.tour = Some(span(&mut i, 0.0, 36000.0)),
            "--vsync" => a.vsync = true,
            "--dx12" => a.dx12 = true,
            "--msaa" => {
                let n = count(&mut i);
                if ![1, 2, 4].contains(&n) {
                    bad("wants 1, 2 or 4");
                }
                a.msaa = Some(n);
            }
            "--no-hud" => a.hud = false,
            "--style" => {
                let name = val(&mut i);
                match Style::parse(&name) {
                    Some(st) => a.style = Some(st),
                    None => bad(&format!("no style called {name}")),
                }
            }
            "--blocky" => a.blocky = true,
            "--world" => a.world = Some(val(&mut i)),
            // The launch screen's choice, for a run started from here: the
            // rest of its options keep their defaults.
            "--mode" => {
                let name = val(&mut i);
                match crate::launch::Mode::parse(&name) {
                    Some(m) => a.mode = Some(m),
                    None => {
                        eprintln!("--mode: no game called {name} (explore or fps)");
                        std::process::exit(2);
                    }
                }
            }
            "--show-help" => a.show_help = true,
            "--show-worlds" => a.show_worlds = true,
            "--show-menu" => a.show_menu = Some(val(&mut i)),
            "--hour" => a.hour = Some(span(&mut i, 0.0, 24.0)),
            "--weather" => a.weather = Some(val(&mut i)),
            "--season" => a.season = Some(val(&mut i)),
            "--gazetteer" => a.gazetteer = true,
            "--validate" => a.validate = true,
            "--tourney" => a.tourney = Some(count(&mut i)),
            "--tourney-seconds" => a.tourney_seconds = Some(span(&mut i, 1.0, 36000.0)),
            "--duel" => a.duel = Some(count(&mut i)),
            "--duel-seconds" => a.duel_seconds = Some(span(&mut i, 1.0, 36000.0)),
            "--duel-weapon" => a.duel_weapon = Some(val(&mut i)),
            "--ride" => a.ride = Some(val(&mut i)),
            "--ride-secs" => a.ride_secs = span(&mut i, 0.0, 3600.0),
            "--chase" => a.chase = true,
            "--boost" => a.boost = true,
            "--tuck" => a.tuck = true,
            "--camera" => a.camera = true,
            "--camera-out" => a.camera_out = true,
            "--lens" => a.lens = Some(val(&mut i)),
            "--shoot" => a.shoot = Some(val(&mut i).into()),
            "--keep" => a.keep = true,
            "--book" => a.book = Some(count(&mut i) as usize),
            "--catalog" => a.catalog = Some(val(&mut i).into()),
            "--only" => a.only = Some(val(&mut i)),
            "--match" => a.matching = Some(val(&mut i)),
            "--map" => a.map = Some(val(&mut i).into()),
            "--profile-gen" => a.profile_gen = true,
            "--dig" => a.dig = count(&mut i),
            "--fire" => a.fire = count(&mut i),
            "--alt" => a.alt = count(&mut i),
            "--weapon" => a.weapon = Some(count(&mut i) as usize),
            "--no-player" => a.no_player = true,
            "--place" => a.place = count(&mut i),
            "--showcase" => a.showcase = true,
            // The name is optional, so a bare --edit does not eat the flag
            // that follows it.
            "--edit" => {
                let name = if v.get(i + 1).is_some_and(|x| !x.starts_with("--")) { next(&mut i) } else { String::new() };
                a.edit = Some(name);
            }
            "--anim-sheet" => a.anim_sheet = Some(val(&mut i)),
            "--gun-sheet" => a.gun_sheet = true,
            "--threads" => a.threads = (count(&mut i) as usize).max(1),
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown option {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    a
}
