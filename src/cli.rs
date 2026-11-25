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
    /// Which benchmark route (`--bench-route`): the flight, or a walk round
    /// a field.
    pub bench_route: crate::tools::bench::Route,
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
    /// `--eco-map LAYER`: draw the ecology map (game/eco.rs) in the shot.
    pub eco_map: Option<String>,
    /// `--eco-pin NAME`: pin the nearest walker whose species name has NAME
    /// in it (or `nearest`) for the inspector.
    pub eco_pin: Option<String>,
    /// `--eco-dump FILE`: a JSON line of every animal each `eco_every` seconds.
    pub eco_dump: Option<PathBuf>,
    pub eco_every: f32,
    /// `--eco-soak HOURS`: run the herd and pack records alone for HOURS game
    /// hours, as fast as they compute, printing their numbers, then exit.
    pub eco_soak: Option<f32>,
    /// `--seam out.png`: how the world meets its neighbours, with a map of
    /// it and them side by side.
    pub seam: Option<PathBuf>,
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
    /// (headless) the bots posed in a row of states (brain/sheet.rs).
    pub bot_sheet: bool,
    pub zombie_sheet: bool,
    pub style: Option<Style>,
    pub blocky: bool,
    /// Creatures drawn as voxels, not their decimated meshes.
    pub voxel_creatures: bool,
    pub show_help: bool,
    pub show_worlds: bool,
    /// (headless) draw a menu screen: title, settings, worlds, loading or pause.
    pub show_menu: Option<String>,
    pub world: Option<String>,
    /// A race's rivals (1 to 7) and laps, as the race's own screens set them.
    pub rivals: Option<u32>,
    pub laps: Option<u32>,
    pub hour: Option<f32>,
    /// Hold the weather at one kind (world/weather.rs): sun, heat, clouds,
    /// rain, snow or fog. It stops drifting, as `--hour` stops the clock.
    pub weather: Option<String>,
    /// Hold the world in one part of the year (world/season.rs): spring,
    /// summer, autumn or winter. Unset, it turns on the clock.
    pub season: Option<String>,
    pub gazetteer: bool,
    pub validate: bool,
    /// Measure the world's survey afresh, write its survey.bin, print it, exit.
    pub survey: bool,
    /// Build the world and print its plant site fields' spread by biome, exit.
    pub site_report: bool,
    /// Run the bot tournament: how many rounds each entrant plays.
    pub tourney: Option<u32>,
    pub tourney_seconds: Option<f32>,
    pub league: Option<u32>,
    pub league_weapon: Option<String>,
    /// Rounds of the weapon bench (`tools::duel`).
    pub duel: Option<u32>,
    /// Seeds of the capture-the-flag bench (`tools::ctf`), seconds a match,
    /// and bots a side.
    pub ctf: Option<u32>,
    pub ctf_seconds: Option<f32>,
    pub ctf_size: Option<u32>,
    pub ctf_skill: Option<String>,
    pub duel_seconds: Option<f32>,
    /// Only this weapon's row of the table, for a quick check on one gun.
    pub duel_weapon: Option<String>,
    /// (headless) ride before the screenshot: "bike" or "wing", for `ride_secs`.
    pub ride: Option<String>,
    pub ride_secs: f32,
    pub chase: bool,
    /// The view a car opens in, true for the chase camera: the player's
    /// saved one when the window starts a world. None takes the default.
    pub drive_chase: Option<bool>,
    pub boost: bool,
    /// (headless) hold Ctrl while riding: the bike's tuck.
    pub tuck: bool,
    /// (headless) which of the world's vehicles `--ride drive` takes.
    pub vehicle: Option<String>,
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
    /// Render the ambience at the camera to this WAV file and exit
    /// (src/sound/bake.rs), `secs` seconds of it.
    pub sound_bake: Option<PathBuf>,
    pub secs: f32,
    /// Render animal calls alone into this folder (src/sound/bake.rs
    /// `calls`): a WAV heard from `call_m` metres and a spectrogram of each.
    pub call_bake: Option<PathBuf>,
    /// Which calls, by name, comma-separated; every one when None.
    pub calls: Option<String>,
    pub call_m: f32,
    /// Open the EDIT page (src/editor) as soon as the world is in, on this
    /// creature or plant; an empty name opens it on the first creature.
    pub edit: Option<String>,
    /// With --catalog: only these kinds (comma list of creatures, plants, biomes, finds).
    pub only: Option<String>,
    /// With --catalog: only entries whose name contains one of these (comma list).
    pub matching: Option<String>,
    /// `--mode`: the way of playing, when the command line names one.
    pub mode: Option<String>,
    /// `--kit <name>`: the weapon kit for a match, and the guns `--gun-sheet` shows.
    pub kit: Option<String>,
    /// `--sheet-guns a,b`: only these weapons on the gun sheet.
    pub sheet_guns: Vec<String>,
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
  --voxel-creatures     creatures as voxels instead of smooth decimated meshes
  --bench               fly a fixed route and report frame times
  --bench-route field   walk a loop at eye height instead, round the spawn (or
                        --ground): in a field with animals, the work of playing
  --bench-route race    (windowed, a race world) drive the race on autopilot, to the
                        finish, and list the slowest frames with their parts
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
                        (with --kit, that kit's guns and their model options)
  --sheet-guns A,B      (with --gun-sheet) only these weapons
  --kit NAME            the weapon kit a match is played with (library/weapon_kits)
  --bot-sheet           (headless) the bots posed in a row of states (idle, walk, run,
                        sprint, backpedal, strafe, aim up/down, jump, firing), side view
                        above and three-quarter view below; use with --pos and --look
  --zombie-sheet        (headless) the zombie builds in each way of walking (shamble,
                        lurch, runner, crawler), standing and moving, laid out as --bot-sheet
  --map FILE            write a shaded top-down map of the world as PNG and exit
  --eco-map LAYER       (headless) draw the ecology map (backquote, then P) in the shot; LAYER is creatures,
                        a survey channel (water, cover, ...) or a plant site field (wet, shade, ...)
  --eco-pin NAME        (headless) pin the nearest walker whose species has NAME in it
                        (or nearest) for the inspector; VG_ECO_WALK=S walks at it from
                        S seconds in, VG_ECO_RUN=1 at a run; VG_ECO_FREEZE=S freezes
                        the animals S seconds in
  --eco-dump FILE       (headless) write every animal as one JSON line each --eco-every
                        seconds (default 10)
  --eco-soak HOURS      (headless) run the herd and pack records alone, nothing drawn,
                        for HOURS game hours as fast as they compute; print each
                        species' numbers as a JSON line every 6 hours, then exit
  --profile-gen         time terrain generation and meshing on one thread
  --world NAME          play worlds/NAME or local/worlds/NAME (default glassfall; random = a noise-generated island)
  --mode NAME           how to play it: explore or fps (default: what the world is built for)
  --hour H              time of day, 0 to 24, with the clock stopped (default 15, running)
  --weather KIND        hold the weather at sun, heat, clouds, rain, snow or fog (default: it drifts)
  --season NAME         hold the year at spring, summer, autumn or winter (default: it turns)
  --gazetteer           build the world and print what its chronicle made, then exit
  --validate            check the library and the world, print problems, then exit
  --survey              measure the world survey afresh, write survey.bin, print it, then exit
  --site-report         build the world, print each site field's spread by biome and how well each
                        plant's ground suits it, then exit (docs/PLANTS.md)
  --tourney N           score the bots on how they move, N rounds each (no window)
  --tourney-seconds S   how long each round lasts (default 60)
  --league N            five tuned bots in one match, N seeds x five slots, scored
                        on objectives and on moving like a player (no window)
  --league-weapon W     the same with one gun only (a weapon name, or all for each in turn)
  --duel N              bench the weapons against each other, N rounds (no window)
  --duel-seconds S      how long each duel lasts (default 45)
  --duel-weapon NAME    only this gun's row of the table
  --ctf N               capture the flag: the team brain against two controls,
                        N seeds with the sides swapped (no window)
  --ctf-seconds S       how long each match lasts (default 300)
  --ctf-size K          bots a side, 1 to 4 (default 3)
  --ctf-skill NAME      the bots' difficulty (default average)
  --ride bike|wing|launch|rope|climb|kayak|drive|park  (headless) ride for --ride-secs (default 4) before
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
  --rivals N, --laps N  a race's rivals (1 to 7) and laps
  --boost               (headless) hold Shift while riding
  --tuck                (headless) hold Ctrl while riding: the bike tucks, which stores
                        a charge on a descent and floats the next jump
  --vehicle NAME        (headless) with --ride drive or park, which of the world's
                        vehicles to take (library/vehicles); park drives, stops and
                        gets out, leaving it parked

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

Sound (see GUIDE.md, \"Sound\"):
  --sound-bake FILE     render the ambience heard at the camera (--pos/--ground,
                        --look, --hour, --weather, --season) into FILE.wav, print
                        what made it and how loud it came out, then exit
  --secs N              with --sound-bake: seconds to render (default 20)
  --call-bake DIR       render each animal call (src/sound/call.rs) into DIR:
                        NAME_SEED.wav heard from --call-m metres (default 10)
                        and NAME_SEED.png, a spectrogram of it dry; --calls
                        howl,roar for some of them. Needs no world
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
        rivals: None,
        laps: None,
        fresh: false,
        bench: false,
        bench_route: crate::tools::bench::Route::Fly,
        tour: None,
        vsync: false,
        dx12: false,
        msaa: None,
        hud: true,
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(4).clamp(1, 8),
        map: None,
        eco_map: None,
        eco_pin: None,
        eco_dump: None,
        eco_every: 10.0,
        eco_soak: None,
        seam: None,
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
        bot_sheet: false,
        zombie_sheet: false,
        style: None,
        blocky: false,
        voxel_creatures: false,
        show_help: false,
        show_worlds: false,
        show_menu: None,
        world: None,
        hour: None,
        weather: None,
        season: None,
        gazetteer: false,
        validate: false,
        survey: false,
        site_report: false,
        tourney: None,
        tourney_seconds: None,
        league: None,
        league_weapon: None,
        duel: None,
        ctf: None,
        ctf_seconds: None,
        ctf_size: None,
        ctf_skill: None,
        duel_seconds: None,
        duel_weapon: None,
        ride: None,
        ride_secs: 4.0,
        chase: false,
        drive_chase: None,
        boost: false,
        tuck: false,
        vehicle: None,
        camera: false,
        camera_out: false,
        lens: None,
        shoot: None,
        keep: false,
        book: None,
        edit: None,
        catalog: None,
        sound_bake: None,
        secs: 20.0,
        call_bake: None,
        calls: None,
        call_m: 10.0,
        only: None,
        matching: None,
        mode: None,
        kit: None,
        sheet_guns: Vec::new(),
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
            "--bench-route" => {
                a.bench = true;
                let v = val(&mut i);
                a.bench_route = crate::tools::bench::Route::parse(&v).unwrap_or_else(|| bad(&format!("no route {v}: fly, field or race")));
            }
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
            "--voxel-creatures" => a.voxel_creatures = true,
            "--world" => a.world = Some(val(&mut i)),
            // The launch screen's choice, for a run started from here: the
            // rest of its options keep their defaults.
            "--sheet-guns" => a.sheet_guns = val(&mut i).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "--kit" => {
                let name = val(&mut i);
                if crate::weapon_kit::get(&name).is_none() {
                    eprintln!("--kit: no weapon kit called {name} in library/weapon_kits");
                    std::process::exit(2);
                }
                a.kit = Some(name);
            }
            "--mode" => {
                let name = val(&mut i);
                match crate::games::get(&name) {
                    Some(g) => a.mode = Some(g.name.clone()),
                    None => {
                        eprintln!("--mode: no game called {name} in library/games");
                        std::process::exit(2);
                    }
                }
            }
            "--show-help" => a.show_help = true,
            "--show-worlds" => a.show_worlds = true,
            "--show-menu" => a.show_menu = Some(val(&mut i)),
            "--hour" => a.hour = Some(span(&mut i, 0.0, 24.0)),
            "--rivals" => a.rivals = Some(span(&mut i, 1.0, 7.0) as u32),
            "--laps" => a.laps = Some(span(&mut i, 1.0, 9.0) as u32),
            "--weather" => a.weather = Some(val(&mut i)),
            "--season" => a.season = Some(val(&mut i)),
            "--gazetteer" => a.gazetteer = true,
            "--validate" => a.validate = true,
            "--survey" => a.survey = true,
            "--site-report" => a.site_report = true,
            "--tourney" => a.tourney = Some(count(&mut i)),
            "--tourney-seconds" => a.tourney_seconds = Some(span(&mut i, 1.0, 36000.0)),
            "--league" => a.league = Some(count(&mut i)),
            "--league-weapon" => a.league_weapon = Some(val(&mut i)),
            "--duel" => a.duel = Some(count(&mut i)),
            "--ctf" => a.ctf = Some(count(&mut i)),
            "--ctf-seconds" => a.ctf_seconds = Some(span(&mut i, 1.0, 36000.0)),
            "--ctf-size" => a.ctf_size = Some(count(&mut i)),
            "--ctf-skill" => a.ctf_skill = Some(val(&mut i)),
            "--duel-seconds" => a.duel_seconds = Some(span(&mut i, 1.0, 36000.0)),
            "--duel-weapon" => a.duel_weapon = Some(val(&mut i)),
            "--ride" => a.ride = Some(val(&mut i)),
            "--ride-secs" => a.ride_secs = span(&mut i, 0.0, 3600.0),
            "--chase" => a.chase = true,
            "--boost" => a.boost = true,
            "--tuck" => a.tuck = true,
            "--vehicle" => a.vehicle = Some(val(&mut i)),
            "--camera" => a.camera = true,
            "--camera-out" => a.camera_out = true,
            "--lens" => a.lens = Some(val(&mut i)),
            "--shoot" => a.shoot = Some(val(&mut i).into()),
            "--keep" => a.keep = true,
            "--book" => a.book = Some(count(&mut i) as usize),
            "--catalog" => a.catalog = Some(val(&mut i).into()),
            "--sound-bake" => a.sound_bake = Some(val(&mut i).into()),
            "--secs" => a.secs = span(&mut i, 0.5, 600.0),
            "--call-bake" => a.call_bake = Some(val(&mut i).into()),
            "--calls" => a.calls = Some(val(&mut i)),
            "--call-m" => a.call_m = span(&mut i, 0.5, 5000.0),
            "--only" => a.only = Some(val(&mut i)),
            "--match" => a.matching = Some(val(&mut i)),
            "--map" => a.map = Some(val(&mut i).into()),
            "--eco-map" => a.eco_map = Some(val(&mut i)),
            "--eco-pin" => a.eco_pin = Some(val(&mut i)),
            "--eco-dump" => a.eco_dump = Some(val(&mut i).into()),
            "--eco-every" => a.eco_every = span(&mut i, 0.1, 3600.0),
            "--eco-soak" => a.eco_soak = Some(span(&mut i, 0.1, 10000.0)),
            "--seam" => a.seam = Some(val(&mut i).into()),
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
            "--bot-sheet" => a.bot_sheet = true,
            "--zombie-sheet" => a.zombie_sheet = true,
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
