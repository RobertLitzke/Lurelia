//! Game state without the GPU: the world, the player, creatures, edits, saving and
//! the HUD text. Simulation runs on a fixed 60 Hz step; rendering interpolates.

use crate::camera::{self, Camera};
use crate::entities::Entities;
use crate::models::{self, Model};
use crate::modes::{Activity, ActivityCtx, Allowed, ModeId};
use crate::player::{MoveInput, Player};
use crate::render::hud::Hud;
use crate::render::{FrameParams, FrameStats, Instance, Style};
use crate::ride::{soar, Ride};
use crate::screenshot::Screenshot;
use crate::stream::{ChunkSink, Streamer};
use crate::util::FxHashSet as HashSet;
use crate::world::gen::{EditMap, Generator};
use crate::world::material::{self, AIR};
use crate::world::spec::WorldSpec;
use crate::world::store::Store;
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

pub mod eco;
mod live;

pub const STEP: f32 = 1.0 / 60.0;
/// How a heavy footfall rocks the view at full strength (an elephant at
/// arm's length): metres the eye drops, and radians it rolls
/// (`Game::feel_steps`). Kept small on purpose: it should be felt more than
/// seen.
const THUMP_M: f32 = 0.012;
const THUMP_ROLL: f32 = 0.0022;
/// How fast it settles, per second.
const THUMP_FADE: f32 = 16.0;
/// How near the camera a `WorldVent` must be to breathe, metres.
const VENT_REACH: f32 = 220.0;
/// The hour the EDIT page's studio is lit at: late morning, the sun high
/// enough to light every side but the shadow still showing the shape.
const STUDIO_HOUR: f32 = 10.5;
/// Falling snow round the camera: half-width of the box (m) and flakes a
/// second in full snow.
const SNOW_BOX: f32 = 13.0;
const SNOW_RATE: f32 = 650.0;

/// Bodies within this of the camera are kept however the camera faces: they
/// can cast a shadow into the view. It covers both shadow cascades that draw
/// creatures (render/mod.rs `splits` and `SLICE_MARGIN`), with room to spare.
const SHADOW_CASTERS_M: f32 = 80.0;

/// Metres from where a zombie round failed to plan before it is tried again:
/// a start refuge is looked for within 90 m of the player.
const ZOMBIE_RETRY_M: f32 = 120.0;

/// The water surface over a point and the current there, as
/// `Player::update` wants it: the world's own water, which is a field.
/// `water_at` adds the water a structure holds as voxels, and is what
/// anything stepping a body should ask.
///
/// `sea` is the open sea's surface now (`Fields::sea_now` with a tide); a
/// hollow below sea level that the sea cannot reach stays at high water.
pub fn water_over(land: Option<&crate::world::fields::World>, sea: f32, p: Vec3) -> (f32, glam::Vec2) {
    match land {
        Some(w) => {
            // The open sea with its swell on it (`sea` is the level without).
            let sea = if w.f.basin(p.x, p.z) { w.f.sea } else { sea + (w.f.sea_at(p.x, p.z) - w.f.sea_now()) };
            match w.f.wet_now(p.x, p.z) {
                Some(wet) if wet.level > sea => (wet.level, glam::Vec2::from(wet.flow)),
                _ => (sea, glam::Vec2::ZERO),
            }
        }
        None => (sea, glam::Vec2::ZERO),
    }
}
/// The water surface over a point and the current there, with the water a
/// structure holds as voxels counted in. A free function because an
/// activity moving a body of its own, and the shooter stepping a bot, need
/// the same answer as the player while holding the player mutably
/// (modes::ActivityCtx::water_at, Game::water_at).
///
/// A voxel pool wins over the field: an imported map's pool stands on the
/// ground, and the field under it is the sea, far below.
pub fn water_at(land: Option<&crate::world::fields::World>, sea: f32, store: &crate::world::store::Store, p: Vec3) -> (f32, glam::Vec2) {
    let (level, flow) = water_over(land, sea, p);
    match crate::physics::water_voxel_top(store, p) {
        Some(top) if top > level => (top, glam::Vec2::ZERO),
        _ => (level, flow),
    }
}

pub const FOV_Y: f32 = 70.0 * std::f32::consts::PI / 180.0;
pub const NEAR: f32 = 0.05;
/// How far past a lamp's own reach it is still gathered for a frame, metres:
/// the camera may be outside the sphere while what it is looking at is in it.
const LAMP_SLACK: f32 = 50.0;
/// The half-angle of the cone lamps are kept in, as a tangent: wide enough
/// to hold the view frustum at any field of view this game uses, since a
/// lamp dropped while it is lighting something on screen is a room that
/// brightens as you walk into it.
const LAMP_CONE: f32 = 2.75;
pub const PALETTE_KEYS: [u8; 9] = [
    material::PLANKS,
    material::STONE,
    material::BRICK,
    material::DIRT,
    material::GRASS,
    material::SAND,
    material::LEAVES,
    material::BARK,
    material::SNOW,
];

#[derive(Default)]
pub struct Actions {
    pub dig: bool,
    pub place: bool,
    /// E in screenshot mode: tilt the camera clockwise.
    pub tilt_right: bool,
    /// Q in screenshot mode: tilt the camera anticlockwise.
    pub tilt_left: bool,
    pub toggle_fly: bool,
    pub select: Option<usize>,
    pub brush_delta: i32,
    /// Get on or off the mountain bike.
    pub bike_toggle: bool,
    /// Get into or out of the kayak.
    pub kayak_toggle: bool,
    /// U: get into or out of the world's vehicle.
    pub vehicle_toggle: bool,
    /// Launch, open or fold the wings.
    pub glide: bool,
    /// First person or chase camera while riding.
    pub cam_toggle: bool,
    /// Screenshot mode on or off.
    pub screenshot_toggle: bool,
    /// Save a screenshot of the view (screenshot mode, or F12 at any time).
    /// With a camera photo waiting, keep that instead.
    pub screenshot_take: bool,
    /// Take the camera out or put it away.
    pub photography: bool,
    /// Middle click: the camera back on autofocus.
    pub autofocus: bool,
    /// Open or close the logbook and scrapbook.
    pub logbook: bool,
    /// Load a new roll of film.
    pub reload: bool,
    /// Tab: the next frame shape (landscape, portrait, square).
    pub frame: bool,
    /// Esc while the camera has something to back out of.
    pub back: bool,
    /// Enter: go on with whatever is waiting. A finished match takes it as
    /// "play again" (src/shooter/round.rs).
    pub accept: bool,
}

/// The tuning panel's keys and, on the pause screen, its mouse.
const TUNE_KEYS: &str = "PGUP PGDN PICK  [ ] STEP  - = X10  BKSP FILE  P SAVE";
const TUNE_MOUSE: &str = "DRAG A LINE  WHEEL STEPS  RIGHT CLICK FILE  CLICK HERE TO SAVE";

/// Where the tuning panel is drawn (Game::tune_layout).
pub struct TuneLayout {
    pub x: f32,
    pub y0: f32,
    pub line: f32,
    pub s: f32,
    pub first: usize,
    pub show: usize,
    pub name_w: f32,
    pub wide: f32,
}

/// What the mouse is over on the tuning panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TuneHit {
    Line(usize),
    Save,
}

/// The settings a player keeps from one run to the next, whatever world it
/// is. `menu::Prefs` holds them (flattened, so settings.json reads as it
/// always has) and a game is handed them when it starts; the keys that change
/// them in play (G, T, B, F3, F6, and V in a car) change `Game::settings`,
/// and the window copies that back. Nothing here is saved with a world.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shadows: bool,
    /// A style this build cannot read takes its default and leaves the
    /// other settings alone.
    #[serde(deserialize_with = "crate::util::or_default")]
    pub style: Style,
    pub slopes: bool,
    /// Creatures drawn as smooth decimated meshes (true) or as voxels
    /// (docs/CREATURES.md "Model resolution and decimation").
    pub smooth_creatures: bool,
    pub show_stats: bool,
    /// The debug minimap (F6): a north-up map of the ground round the player.
    pub minimap: bool,
    /// The view a car is driven in, true for the chase camera and false for
    /// the driver's seat. V while driving changes it, and every vehicle of
    /// `Mode::Drive` (all of them four-wheeled) opens in it.
    pub drive_chase: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shadows: true,
            style: Style::default(),
            slopes: true,
            smooth_creatures: true,
            show_stats: false,
            minimap: false,
            drive_chase: true,
        }
    }
}

/// Missing fields take their defaults, so a file from an older or newer
/// version still loads.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Progress {
    /// `PROGRESS_VERSION` when written; 0 in a file from before it had one.
    /// Nothing reads it yet. It is here so that the first change to what a
    /// field *means* (rather than which fields there are, which the defaults
    /// already cover) has something to tell old files from new by.
    version: u32,
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
    // The free camera (F4) is not saved: a world always opens on your feet.
    // A session that ended in the air used to come back to it, which puts
    // you inside whatever hill the camera was in with no way to walk out.
    gems: Vec<(i32, i32)>,
    // The style and sloped ground used to be saved here too. They are the
    // player's, not the world's (`Settings`), and a file that still has them
    // loads with them ignored.
    #[serde(deserialize_with = "lenient")]
    camera: Option<camera::CameraSave>,
    /// The best glide scored in this world (ride/soar.rs).
    #[serde(default)]
    best_flight: f64,
    /// Where the world had got to in the year (world/season.rs), so coming
    /// back to it is not always the season it opens in.
    #[serde(default)]
    season: Option<String>,
    #[serde(default)]
    day: u32,
    /// The car left parked (ride/drive.rs): which, where and which way up.
    #[serde(default)]
    parked: Option<ParkedSave>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ParkedSave {
    name: String,
    pos: [f32; 3],
    rot: [f32; 4],
}

fn lenient<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

/// What `Progress::version` is written as. Raise it when a field changes
/// meaning, and have `load_progress` bring an older file up.
const PROGRESS_VERSION: u32 = 1;

pub struct Game {
    /// Time of day in hours; runs from dawn to dusk and starts over (daylight.rs).
    pub hour: f32,
    /// Whether the clock moves (`--hour` stops it for screenshots).
    pub clock_running: bool,
    /// What kind of day it is, and the one it is drifting into
    /// (world/weather.rs). It runs on the game clock, so it stops when the
    /// clock does.
    pub weather: crate::world::weather::Weather,
    /// What part of the year it is, what day it is, and how much snow is
    /// lying (world/season.rs). It runs on the game clock too, and a turn of
    /// the year has the loaded columns built again.
    pub year: crate::world::season::Year,
    /// What the weather has left on the ground: wet, puddles, frost and fog
    /// in the hollows (world/damp.rs). Stepped with the weather; not saved,
    /// so a world opens on ground that suits the sky it opens under.
    pub damp: crate::world::damp::Damp,
    /// Lightning out of a dry storm (world/lightning.rs): stepped on the
    /// wall clock, since a strike is over in half a second.
    pub lightning: crate::world::lightning::Lightning,
    /// Prints and trampled grass left by the animals and the player
    /// (creatures/sign.rs). Not saved: a world opens on clean ground.
    pub marks: crate::creatures::sign::Marks,
    /// Metres the player has walked since their last bootprint, and which
    /// foot it was.
    boot: (f32, bool),
    /// Where each rear tyre last laid a piece of track.
    tyre_marks: [Option<Vec3>; 2],
    /// Shrubs the big animals have flattened, and when each grows back
    /// (creatures/trample.rs).
    trample: crate::creatures::trample::Trample,
    /// The last heavy footfall felt through the ground: how hard it came to
    /// the player, and seconds since (`feel_steps`, `camera`).
    thump: (f32, f32),
    pub streamer: Streamer,
    pub store: Store,
    pub player: Player,
    /// What the player is riding, if anything. It sits beside the body rather
    /// than inside it: a ride steers the body it is carrying (`Ride::step`
    /// takes `&mut Player`), and every bot in a match owns a `Player` that
    /// would otherwise carry a bike, a wingsuit and a rope it never uses.
    pub ride: Ride,
    pub ents: Entities,
    /// The world's moving platforms and its teleporters and jump pads, and
    /// the step that runs them (src/world_parts.rs).
    pub parts: crate::world_parts::WorldParts,
    pub models: Vec<Model>,
    /// The models' colours, handed to the renderer with them: the fixed
    /// block (models::FIXED_SLOTS), then each library model's own block.
    pub palette: Vec<[f32; 4]>,
    /// How many colours the loaded world's library model blocks take, after
    /// the fixed block. The EDIT page puts its subject's block after these.
    pub library_colours: usize,
    /// The models or their palette changed while the game was running (the
    /// EDIT page): the window hands them to the renderer again.
    pub models_dirty: bool,
    /// The entity palette has changed but the models have not: the EDIT page
    /// painting a colour (editor/mod.rs `repaint`), which needs no mesh built
    /// or uploaded.
    pub palette_dirty: bool,
    pub settings: Settings,
    /// The key table is up (headless `--show-help`).
    pub show_help: bool,
    pub time: f32,
    acc: f32,
    pub alpha: f32,
    /// The alpha the world's bodies are drawn at while it is paused; None
    /// while it runs (see `run_steps`).
    still_alpha: Option<f32>,
    pub selected: usize,
    pub brush: i32,
    pub target: Option<([i32; 3], [i32; 3])>,
    save_dir: PathBuf,
    /// Whether save() writes anything. Off for headless runs, which start
    /// fresh and must not overwrite the player's saves.
    pub persist: bool,
    /// A match's spawn points come out of its seed rather than off the clock
    /// (`Shooter::repeatable`). On for headless runs, which are
    /// measurements: a report that does not repeat cannot be compared with
    /// the one before it, and the spawn draw was the one thing in a match
    /// that the clock had a say in.
    pub repeat: bool,
    /// The player's ground has loaded and physics is running.
    pub spawned: bool,
    /// This is a match: the world starts undamaged and the damage is never
    /// written to the save.
    arena: bool,
    /// The player was put at a named place rather than left to the world or
    /// to a match (`--pos`). See `Game::pin`.
    pinned: bool,
    /// The zombie round could not be planned, said once rather than every
    /// frame: `Some(None)` on a world with no survey, never tried again;
    /// `Some(Some(at))` with no refuge in reach of `at`, tried again once the
    /// player has walked `ZOMBIE_RETRY_M` from there.
    zombie_failed: Option<Option<Vec3>>,
    sea: f32,
    inst: Vec<(usize, Instance)>,
    /// Instances this frame had to leave out because more than the
    /// renderer's buffer holds were in view (see `cull_instances`). The
    /// stats HUD shows it, so the cap is never silent again.
    pub instances_dropped: usize,
    /// Sign boards and lettering drawn into the world (render/plates.rs),
    /// refilled every frame from the signs near the camera.
    plates: crate::render::plates::Plates,
    /// Particles: dust, splashes, chips, falling leaves (src/fx). Stepped
    /// with the world, drawn by render/particles.rs.
    fx: crate::fx::Fx,
    /// The point lights near the camera (render/lamps.rs), refilled every
    /// frame from the world's list; the only light a roofed place gets.
    lamps: crate::render::lamps::Lamps,
    /// The named area the player is in (world/places.rs), the one before it,
    /// and the time the two changed, so the label fades rather than blinks.
    area: Option<usize>,
    area_was: Option<usize>,
    area_since: f32,
    /// Lines drawn into the world this frame (render/overlay.rs): modes and
    /// activities push them while the frame is built; cleared every frame.
    pub overlay: crate::render::overlay::Overlay,
    /// A race's polished racing line, every mark so far, for the renderer.
    pub polish: crate::render::polish::Marks,
    pub frame_ms: VecDeque<f32>,
    dig_cd: f32,
    place_cd: f32,
    message: Option<(String, f32)>,
    autosave: f32,
    /// The creature or tree under the crosshair, shown in the corner.
    pub looking_at: Option<String>,
    /// For a creature under the crosshair: its sex, age and what it is doing
    /// (census.rs `doing`), shown above its name, and which one it is.
    pub looking_doing: Option<(String, hecs::Entity)>,
    pub last_render: FrameStats,
    pub gpu_name: String,
    pub freeze_creatures: bool,
    /// The animal debug view: the ecology map, the pinned animal and the
    /// animals' clock (game/eco.rs).
    pub eco: eco::Eco,
    /// Instances a headless tool adds to every frame (the bot pose sheet).
    pub extra_inst: Vec<(usize, crate::models::Instance)>,
    /// Index of the first ride model (bike, flyer) in `models`.
    pub ride_models: usize,
    /// Index of the first vehicle model (src/vehicle/draw.rs).
    pub vehicle_models: usize,
    /// Index of the first weapon view-model in `models` (combat/viewmodel.rs).
    pub weapon_models: usize,
    /// The camera: film, tasks, the scrapbook (src/camera).
    pub camera: Camera,
    /// Files being written (scrapbook photos); the window or headless code
    /// takes them to watch for errors.
    pub files: Vec<std::thread::JoinHandle<Result<PathBuf, String>>>,
    /// The activity layered over walking, if any (modes.rs): screenshot mode,
    /// or the shooter's rules on a world that has them.
    pub activity: Option<Box<dyn Activity>>,
    /// The shooter put aside while something else holds the activity slot
    /// (screenshot mode), so its score and its magazines survive the detour.
    parked: Option<Box<dyn Activity>>,
    /// The EDIT page is open (src/editor): the gun is put away and the
    /// shooter's rules stay parked, since a studio is no place for a match.
    pub editing: bool,
    /// While editing, light the studio itself rather than leaving it in the
    /// world's hour, weather and grade (`studio_lit`).
    pub studio_light: bool,
    /// The world's own spawn point and bearing: where a shooter's rules send
    /// a body back when they name no spawns of their own.
    home: (Vec3, f32),
    /// What the launch screen chose: the mode and the options this run has
    /// (src/launch.rs). Read, never changed, once a world is in.
    pub launch: crate::launch::Chosen,
    screenshot_request: bool,
}

impl Game {
    /// Errors when the selected world fails to load; only `random` plays the
    /// noise island.
    pub fn new(save_dir: PathBuf, view_m: f32, threads: usize, fresh: bool, launch: crate::launch::Chosen, settings: Settings) -> Result<Self, String> {
        // A chronicle world from worlds/<name> or local/worlds/<name>, or the random island.
        let name = crate::worldload::selected();
        let loaded = match crate::worldload::load(&name) {
            Ok(mut l) => {
                if let Some(w) = std::sync::Arc::get_mut(&mut l.world) {
                    w.f.drop_build_only();
                }
                Some(l)
            }
            Err(_) if name == "random" => None,
            Err(e) => return Err(e),
        };
        let save_dir = match &loaded {
            Some(l) => save_dir.join(&l.world.def.name),
            None => save_dir.join("random"),
        };
        let (spec, gen) = match &loaded {
            Some(l) => {
                let g = Generator::with_world(l.world.clone());
                (g.spec.clone(), g)
            }
            None => {
                let spec = WorldSpec::load_or_roll(&save_dir.join("world.json")).expect("world.json");
                (spec.clone(), Generator::new(spec))
            }
        };
        let (spawn, yaw) = gen.spawn_point();
        let allowed = match loaded.as_ref().and_then(|l| l.world.def.modes.as_ref()) {
            Some(names) => Allowed::from_names(names, |n| eprintln!("world.json: unknown mode {n}")),
            None => Allowed::all(),
        };
        // The launch screen's switches take modes away, and only a match
        // leaves the shooter's rules in (src/launch.rs).
        let allowed = launch.allowed(allowed);
        // Driving needs something to drive.
        let vehicles = loaded.as_ref().map(|l| l.world.def.vehicles.clone()).unwrap_or_default();
        let allowed = if vehicles.is_empty() { allowed.without(ModeId::Drive) } else { allowed };
        // This world's sun and northern lights, before anything reads an hour.
        let sun_path = loaded.as_ref().and_then(|l| l.world.def.sun.as_ref()).map_or(crate::render::daylight::SunPath::DEFAULT, |s| s.path());
        crate::render::daylight::set_path(sun_path);
        crate::render::aurora::set_strength(loaded.as_ref().map_or(0.0, |l| l.world.def.aurora), spec.seed as u32);
        // The weather this world gets, and the kind it opens on: whatever
        // `world.json` says, else the commonest kind its climate allows.
        let climate = loaded.as_ref().map(|l| crate::world::weather::Climate::from_weights(&l.world.def.weather)).unwrap_or_default();
        let held = launch.sky.as_deref().and_then(crate::world::weather::Kind::parse);
        let start_sky = if let Some(k) = held {
            // A race's conditions, held for the race.
            k
        } else if !launch.weather {
            // The launch screen's switch off: fair weather, held.
            crate::world::weather::Kind::Sun
        } else {
            loaded
                .as_ref()
                .and_then(|l| l.world.def.start_weather.as_deref())
                .and_then(crate::world::weather::Kind::parse)
                .unwrap_or_else(|| climate.commonest())
        };
        let weather = crate::world::weather::Weather::new(start_sky, climate, spec.seed, launch.weather && held.is_none());
        let height_vox = gen.height_vox;
        // A match opens in the arena as it was built. The craters of the
        // last one are that match, not the map, and a world you are
        // exploring is the only place where what you dug is worth keeping,
        // which is the same line the saved position is on below.
        let arena = launch.mode.fresh_start();
        let edits = if fresh || arena { EditMap::default() } else { load_edits(&save_dir.join("edits.bin")) };
        let progress: Option<Progress> = if fresh { None } else { load_progress(&save_dir.join("progress.json")) };
        // The seasons this world has, and the one it opens in: the launch
        // screen's pick, else `world.json`, else the first of its calendar.
        let calendar = loaded.as_ref().map(|l| l.world.def.calendar()).unwrap_or_default();
        let start_season = launch
            .season
            .as_deref()
            .and_then(crate::world::season::Season::parse)
            .or_else(|| progress.as_ref().and_then(|p| p.season.as_deref()).and_then(crate::world::season::Season::parse))
            .or_else(|| loaded.as_ref().and_then(|l| l.world.def.start_season.as_deref()).and_then(crate::world::season::Season::parse))
            .filter(|s| calendar.has(*s))
            .unwrap_or_else(|| calendar.first());
        // The year turns on the clock, so a held clock holds the season; a
        // season picked by hand on the launch screen holds it as well.
        // A map in space holds its hour, so the stars it is lit by stay out.
        let space = loaded.as_ref().is_some_and(|l| l.world.def.space);
        let gravity = loaded.as_ref().map(|l| l.world.def.gravity).unwrap_or_default();
        let turning = launch.day_night && launch.season.is_none() && !space;
        // The sun stands as high as the season it opens in puts it.
        crate::render::daylight::set_declination(start_season.phase().declination);
        // The hour this run opens at: the launch screen's pick (a preset,
        // moved to the same part of this world's day), else the world's own,
        // else mid-afternoon.
        let start_hour = launch
            .hour
            .map(|h| crate::render::daylight::path().solar(h))
            .or_else(|| loaded.as_ref().and_then(|l| l.world.def.start_hour))
            .map_or(crate::world::clock::START, crate::world::clock::wrap);
        let mut year = crate::world::season::Year::new(start_season, calendar, spec.seed, start_hour, turning);
        if let Some(l) = loaded.as_ref() {
            year.set_snows(crate::world::weather::Climate::from_weights(&l.world.def.weather).has(crate::world::weather::Kind::Snow));
        }
        year.day = progress.as_ref().map_or(0, |p| p.day);
        let mut player = Player::new(spawn + Vec3::Y * 0.5, yaw);
        let mut ride = Ride::default();
        let mut gems = HashSet::default();
        if let Some(p) = &progress {
            // Where you were is only worth coming back to in a world you
            // are exploring. A match opens on a spawn point, so remembering
            // the spot the last one ended on puts you back in it every
            // time -- which on one imported map is the bottom of the water.
            if !launch.mode.fresh_start() {
                player = Player::new(Vec3::from(p.pos), p.yaw);
                player.pitch = p.pitch;
            }
            ride.soar.best = p.best_flight;
            gems = p.gems.iter().copied().collect();
        }
        ride.allowed = allowed;
        ride.catalogue = crate::vehicle::Catalogue::load(&crate::library::data_root());
        ride.vehicle_pick = launch.vehicle.as_ref().and_then(|v| vehicles.iter().position(|n| n == v)).unwrap_or(0);
        ride.vehicles = vehicles;
        if let Some(p) = progress.as_ref().and_then(|p| p.parked.as_ref()).filter(|_| !launch.mode.fresh_start()) {
            let mut car = crate::vehicle::Vehicle::new(ride.catalogue.get(&p.name), Vec3::ZERO, 0.0);
            car.b.pos = Vec3::from(p.pos);
            car.b.rot = glam::Quat::from_array(p.rot).normalize();
            car.prev_pos = car.b.pos;
            car.prev_rot = car.b.rot;
            ride.parked = Some(car);
        }
        ride.glide_air = loaded.as_ref().and_then(|l| l.world.def.glide.as_ref()).map_or_else(Default::default, |g| g.air());
        let sea = spec.sea_level_m;
        let mut models = models::build_all();
        let mut palette = models::palette();
        let mut ents = Entities::new(gems, spec.seed);
        let mut library_colours = 0;
        let mut platforms = crate::platforms::Platforms::default();
        let mut portals = crate::portals::Portals::default();
        // A match's arena is held at full detail end to end.
        let arena_box = loaded.as_ref().and_then(|l| l.world.def.shooter.as_ref()).and_then(|sh| crate::stream::arena_box(&sh.nav));
        if let Some(l) = loaded {
            // The library models' blocks, straight after the fixed block,
            // which is where their `block` offsets were counted from.
            library_colours = l.colours.len();
            palette.extend_from_slice(&l.colours);
            let base = models.len();
            models.extend(l.models.into_iter().map(|mut m| {
                for r in m.rig.as_mut().into_iter().chain(m.skeleton.as_mut()) {
                    r.offset_models(base);
                }
                m
            }));
            // ANIMALS off on the launch screen: the models are still loaded,
            // because items and the EDIT page index into them, but nothing
            // has a species to spawn from.
            let species = l.species.into_iter().filter(|_| launch.animals).map(|mut sp| {
                sp.model += base;
                for f in [&mut sp.forms.female, &mut sp.forms.male, &mut sp.forms.juvenile] {
                    if let Some(m) = f.as_mut() {
                        *m += base;
                    }
                }
                sp
            });
            let items = l.items.into_iter().map(|mut it| {
                it.model += base;
                it
            });
            platforms = crate::platforms::Platforms::new(&l.platform_defs, &l.platforms, base + l.platform_models);
            // The last world's platforms keep no grass out of this one.
            crate::mesh::sprites::set_keep_out(Vec::new());
            portals = crate::portals::Portals::new(&l.world.def.portals, |w| eprintln!("world {name}: {w}"));
            ents.set_world(l.world, species.collect(), items.collect());
            // The herds as they were left (creatures/roster.rs), unless this
            // is a fresh start or a match.
            if !fresh && !arena && ents.load_roster(&save_dir) {
                eprintln!("herds: {} read from {}", ents.roster.herds.len(), save_dir.join(crate::creatures::roster::FILE).display());
            }
        }
        let pool = task_pool(&ents.species, &models, &gen, spawn);
        let finds = ents.gems_collected.len();
        let cam_save = progress.as_ref().and_then(|p| p.camera.clone());
        let camera = Camera::new(cam_save, finds, pool, spec.seed, camera::scrapbook::Scrapbook::open(&save_dir));
        let mut streamer = Streamer::new(gen, edits, view_m, threads);
        streamer.full_box = arena_box;
        streamer.set_smooth(settings.slopes, Vec3::ZERO);
        // Which part of the year the first columns are built for. Nothing is
        // loaded yet, so this costs no rebuild.
        streamer.set_turn(year.turn(), Vec3::ZERO);
        Ok(Self {
            hour: start_hour,
            clock_running: launch.day_night && !space,
            damp: crate::world::damp::Damp::settled(&weather.sky()).with_override(std::env::var("VG_DAMP").ok().as_deref()),
            lightning: crate::world::lightning::Lightning::new(spec.seed as u32),
            marks: crate::creatures::sign::Marks::default(),
            boot: (0.0, false),
            tyre_marks: [None; 2],
            trample: Default::default(),
            thump: (0.0, 9.0),
            weather,
            year,
            streamer,
            store: Store {
                height_vox,
                gravity,
                ..Store::default()
            },
            player,
            ride,
            ents,
            parts: crate::world_parts::WorldParts::new(platforms, portals, spawn),
            models,
            palette,
            library_colours,
            models_dirty: false,
            palette_dirty: false,
            settings,
            show_help: false,
            time: 0.0,
            acc: 0.0,
            alpha: 1.0,
            still_alpha: None,
            selected: 0,
            brush: 2,
            target: None,
            save_dir,
            persist: true,
            repeat: false,
            spawned: false,
            pinned: false,
            zombie_failed: None,
            arena,
            sea,
            inst: Vec::new(),
            instances_dropped: 0,
            plates: Default::default(),
            fx: Default::default(),
            lamps: Default::default(),
            area: None,
            area_was: None,
            area_since: 0.0,
            overlay: Default::default(),
            polish: Default::default(),
            frame_ms: VecDeque::new(),
            dig_cd: 0.0,
            place_cd: 0.0,
            message: None,
            autosave: 30.0,
            looking_at: None,
            looking_doing: None,
            last_render: FrameStats::default(),
            gpu_name: String::new(),
            freeze_creatures: false,
            eco: Default::default(),
            extra_inst: Vec::new(),
            ride_models: usize::MAX,
            vehicle_models: usize::MAX,
            weapon_models: usize::MAX,
            camera,
            files: Vec::new(),
            activity: None,
            parked: None,
            editing: false,
            studio_light: true,
            home: (spawn, -yaw.to_degrees()),
            launch,
            screenshot_request: false,
        })
    }

    /// Vertical field of view: narrower through the camera's zoom.
    pub fn fov(&self) -> f32 {
        if let Some(a) = &self.activity {
            FOV_Y * a.zoom()
        } else if self.camera.active && self.camera.raised {
            FOV_Y * self.camera.zoom(FOV_Y)
        } else {
            FOV_Y
        }
    }

    /// A screenshot was asked for this frame (the window or headless code takes it).
    pub fn in_screenshot(&self) -> bool {
        self.activity.as_ref().is_some_and(|a| a.id() == ModeId::Screenshot)
    }

    pub(crate) fn take_screenshot_request(&mut self) -> bool {
        std::mem::take(&mut self.screenshot_request)
    }

    /// A screenshot is on its way to `path`.
    pub fn screenshot_saved(&mut self, path: &str) {
        self.note(&format!("SCREENSHOT  {}", path.replace('\\', "/").to_uppercase()));
        if let Some(a) = &mut self.activity {
            a.shutter();
        }
    }

    /// A new scrapbook page for the renderer, when the page has changed.
    pub fn take_map_image(&mut self) -> Option<Vec<u8>> {
        if let Some(img) = self.eco_map_image() {
            return Some(img);
        }
        self.camera.page_image()
    }

    /// The camera's shutter fired: judge what it saw now, and say how to
    /// develop it (the frame's pixels on a `w` x `h` screen and the lens, with
    /// no viewfinder mask). The caller renders it and hands it to `developed`.
    pub fn take_shot(&mut self, w: u32, h: u32) -> Option<ShotPlan> {
        if !std::mem::take(&mut self.camera.shutter) {
            return None;
        }
        let aspect = w as f32 / h.max(1) as f32;
        let p = self.frame_params(aspect);
        let right = p.cam_fwd.cross(Vec3::Y).normalize_or(Vec3::X);
        let view = camera::scene::View {
            eye: p.cam_pos,
            fwd: p.cam_fwd,
            up: right.cross(p.cam_fwd).normalize(),
            fov: self.fov(),
            aspect,
        };
        let gen = self.streamer.generator();
        let shot = camera::scene::look(
            &view,
            self.camera.lens,
            self.camera.orient,
            self.hour,
            &self.ents.world,
            &self.ents.species,
            &self.models,
            &self.store,
            gen,
            self.sea,
        );
        self.camera.judge(shot);
        Some(ShotPlan {
            crop: camera::lens::frame_px(self.camera.orient, w, h),
            lens: self.camera.lens.fx(self.camera.orient, aspect, false),
        })
    }

    /// The photo from `take_shot`, cropped to the frame (RGBA).
    pub fn developed(&mut self, px: Vec<u8>, w: u32, h: u32) {
        let (writer, msg) = self.camera.developed(px, w, h);
        self.files.extend(writer);
        if !msg.is_empty() {
            self.note(&msg);
        }
    }

    /// The camera's actions, every frame: taking it out and putting it away,
    /// the logbook, Esc, film earned by finds. True while the logbook is open,
    /// which pauses the world.
    fn camera_actions(&mut self, input: &MoveInput, actions: &mut Actions, dt: f32) -> bool {
        if let Some(m) = self.camera.credit_finds(self.ents.gems_collected.len()) {
            self.note(&m);
        }
        if std::mem::take(&mut actions.photography) && self.spawned {
            if self.camera.active {
                self.camera.put_away();
            } else if self.ride.can(ModeId::Camera) {
                self.park_activity();
                self.camera.active = true;
                self.note("CAMERA OUT: CLICK FOR THE VIEWFINDER");
            }
        }
        if std::mem::take(&mut actions.logbook) {
            self.camera.toggle_book();
        }
        if std::mem::take(&mut actions.back) {
            self.camera.back();
        }
        if self.camera.pending.is_some() && std::mem::take(&mut actions.screenshot_take) {
            let w = self.camera.keep();
            self.files.extend(w);
            self.note("KEPT IN THE SCRAPBOOK");
        }
        if self.camera.book.is_some() {
            self.camera.book_tick(input);
            return true;
        }
        if !self.camera.active {
            actions.reload = false;
        } else if self.activity.is_none() {
            if self.camera.raised {
                let eye = self.frame_params(1.0).cam_pos;
                let fwd = self.player.forward();
                let block = self.store.ray(eye, fwd, 400.0, |m| m != AIR).map(|h| h.t);
                let animal = self.ents.identify(&self.models, eye, fwd, 400.0).map(|(t, _)| t);
                self.camera.centre_m = match (block, animal) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
            if let Some(m) = self.camera.tick(input, actions, dt) {
                self.note(&m);
            }
        }
        false
    }

    /// Put the match aside while the EDIT page is open, and pick it up again
    /// when the page closes: a page opened in the middle of a deathmatch
    /// should not be shot at while it is used (src/editor).
    /// Whether the frame is lit by the EDIT page's studio rather than the
    /// world: a fixed sun, a clear sky and the default grade, so grey bark is
    /// grey and not gold in a low sun (`studio_light`).
    pub fn studio_lit(&self) -> bool {
        self.editing && self.studio_light
    }

    pub fn set_editing(&mut self, on: bool) {
        self.editing = on;
        if on {
            self.park_activity();
        }
    }

    /// Take the running activity out, letting it put back what it changed.
    /// The shooter is kept rather than dropped: a look through screenshot
    /// mode should not cost the player their score or their ammunition.
    fn park_activity(&mut self) {
        if let Some(mut a) = self.activity.take() {
            let keep = matches!(a.id(), ModeId::Shooter | ModeId::Race);
            a.exit(&mut self.player);
            if keep {
                self.parked = Some(a);
            }
        }
    }

    /// On a world with a `race` launched as one, the race starts as soon as
    /// the player is on the ground, and comes back after anything else has
    /// had the activity slot.
    fn start_race(&mut self) {
        if self.editing || self.activity.is_some() || !self.spawned || !self.ride.allowed.has(ModeId::Race) {
            return;
        }
        if let Some(a) = self.parked.take() {
            self.activity = Some(a);
            return;
        }
        let Some(w) = self.generator().world.clone() else { return };
        // The map's race as this game runs it (src/games.rs).
        let Some(def) = w.def.race.as_ref().map(|r| self.launch.def().race_rules(r)) else { return };
        let Some(circuit) = w.circuits.iter().find(|c| c.name == def.circuit) else {
            self.note(&format!("NO CIRCUIT CALLED {}", def.circuit.to_uppercase()));
            self.ride.allowed = self.ride.allowed.without(ModeId::Race);
            return;
        };
        // The launch screen's choices: laps, rivals, the player's kart, and
        // how hard the rivals drive.
        let mut def = def;
        if let Some(l) = self.launch.laps {
            def.laps = l;
        }
        if let Some(r) = self.launch.rivals {
            def.rivals = r;
        }
        let kart_race = crate::launch::kart_race(&def.vehicle);
        let mine = self.launch.kart.clone().filter(|_| kart_race).unwrap_or_else(|| def.vehicle.clone());
        let spec = self.ride.catalogue.get(&mine);
        // Rivals drive all three karts in turn in a kart race, or the
        // course's vehicle otherwise.
        let rivals: Vec<crate::vehicle::Spec> = (0..crate::race::MAX_RIVALS as usize)
            .map(|i| {
                let name = if kart_race { crate::launch::KARTS[(i + 1) % crate::launch::KARTS.len()].0 } else { def.vehicle.as_str() };
                self.ride.catalogue.get(name)
            })
            .collect();
        // The road's grip in this weather, for every kart and the bots'
        // judgement of it: wet, then snowy, is slipperier.
        let grip = match self.weather.name() {
            "rain" => 0.85,
            "snow" => 0.72,
            _ => 1.0,
        };
        let level = crate::launch::RACE_LEVELS
            .iter()
            .position(|l| l.eq_ignore_ascii_case(&self.launch.difficulty))
            .map_or(1.0, |k| crate::launch::RACE_LEVEL_SHARE[k]);
        let mut race = crate::race::Race::new(&def, circuit, spec, rivals, level, grip, self.vehicle_models, &w.lib);
        race.set_cup(self.launch.cup.clone());
        self.activity = Some(Box::new(race));
    }

    /// On a world whose world.json carries a `"shooter"` block, the gun is
    /// never put away: start the rules as soon as the player is on the
    /// ground, and again after anything else has had the activity slot.
    fn start_shooter(&mut self) {
        let zombies = self.launch.mode == crate::launch::Mode::Zombies;
        if self.editing || self.activity.is_some() || !self.spawned || !(zombies || self.ride.allowed.has(ModeId::Shooter)) {
            return;
        }
        if let Some(a) = self.parked.take() {
            self.activity = Some(a);
            return;
        }
        let Some(w) = self.generator().world.clone() else { return };
        // The zombie round is played on an explore world, which has no
        // shooter block: its map is planned from the world's survey, from
        // wherever the player is standing (src/shooter/zombies.rs).
        let mut plan = None;
        if zombies {
            match self.zombie_failed {
                Some(Some(at)) if at.distance(self.player.pos) < ZOMBIE_RETRY_M => return,
                Some(None) => return,
                _ => {}
            }
            let Some(survey) = w.survey.as_ref() else {
                eprintln!("zombies: this world has no survey (only explore worlds carry one)");
                self.note("NO ZOMBIE ROUND HERE: THIS WORLD HAS NO SURVEY");
                self.zombie_failed = Some(None);
                return;
            };
            // The kit the round is played with, as `shooter_rules` will
            // choose it below: the plan's caches hold its guns by role.
            let kit_name = crate::weapon_kit::chosen(self.launch.kit.as_deref(), self.launch.def().kit.as_deref(), None);
            let Some(kit) = crate::weapon_kit::get(kit_name).or_else(|| crate::weapon_kit::get(crate::weapon_kit::DEFAULT)) else {
                eprintln!("zombies: no weapon kit called {kit_name}");
                self.zombie_failed = Some(None);
                return;
            };
            match crate::shooter::zombies::plan(survey, self.player.pos, self.home.0, w.def.seed ^ if self.repeat { 0 } else { crate::shooter::spawn_seed(w.def.seed) }, kit) {
                Ok(p) => {
                    self.zombie_failed = None;
                    for (i, l) in p.legs.iter().enumerate() {
                        println!("zombies: leg {} of {}: {:.0} m to {:.0},{:.0} ({})", i + 1, p.legs.len(), l.route_m, l.goal.x, l.goal.z, l.name);
                    }
                    println!("zombies: {} pads", p.rules.pickups.len());
                    plan = Some(p);
                }
                Err(e) => {
                    eprintln!("zombies: {e}");
                    self.note(&format!("NO ZOMBIE ROUND HERE: {e}"));
                    self.zombie_failed = Some(Some(self.player.pos));
                    return;
                }
            }
        }
        let Some(map) = plan.as_ref().map(|p| &p.rules).or(w.def.shooter.as_ref()) else { return };
        // The map says where things are, the game how it is played and the
        // launch screen what was asked for this time: `games.rs` puts the
        // three together, so nothing here knows which game this is.
        let mut rules = self.launch.def().shooter_rules(map, &self.launch, w.def.rock.first().map(|r| r.0.as_str()));
        let seed = w.def.seed;
        // The kit fills the map's weapon spots by role. A role with a choice
        // of guns draws once for the match, from the clock unless the match
        // has to repeat.
        let kit_name = rules.kit.clone().unwrap_or_else(|| crate::weapon_kit::DEFAULT.to_string());
        match crate::weapon_kit::get(&kit_name) {
            Some(kit) => {
                let draw = if self.repeat { seed } else { crate::shooter::spawn_seed(seed) };
                let a = crate::weapon_kit::apply(&mut rules, &w.lib, kit, draw);
                for (from, to) in &a.swapped {
                    println!("kit {kit_name}: {from} -> {to}");
                }
                for gone in &a.dropped {
                    println!("kit {kit_name}: no {gone} (its role is not in the kit)");
                }
            }
            None => eprintln!("no weapon kit called {kit_name}; the map's own guns"),
        }
        let mut sh = match (plan, self.repeat) {
            (Some(p), repeat) => {
                // The launch screen's condition, else (from the command line)
                // `VG_ZOMBIE_CONDITION`, else a clear day.
                let cond = self
                    .launch
                    .condition
                    .clone()
                    .or_else(|| std::env::var("VG_ZOMBIE_CONDITION").ok())
                    .and_then(|c| crate::shooter::zombies::Condition::parse(&c))
                    .unwrap_or(crate::shooter::zombies::Condition::Clear);
                crate::shooter::Shooter::zombies(&w.lib, &rules, p, self.home, &self.store, seed, &self.launch.difficulty, repeat, cond)
            }
            (None, true) => crate::shooter::Shooter::repeatable(&w.lib, &rules, self.home, &self.store, seed),
            (None, false) => crate::shooter::Shooter::new(&w.lib, &rules, self.home, &self.store, seed),
        };
        if self.pinned {
            sh.pin();
        }
        self.activity = Some(Box::new(sh));
    }

    pub fn generator(&self) -> &Generator {
        self.streamer.generator()
    }

    /// The loaded world's library, which holds every entry whatever this
    /// world reaches (the EDIT page browses it).
    pub fn library(&self) -> Option<&crate::library::Library> {
        self.generator().world.as_ref().map(|w| &w.lib)
    }

    /// A library holding only this world's materials, for a caller that
    /// needs one while the game itself is being changed (the EDIT page's
    /// knobs, which read the material list and write to the world).
    pub fn materials_only(&self) -> crate::library::Library {
        crate::library::Library::of_materials(self.library().map(|l| l.materials.clone()).unwrap_or_default())
    }

    /// A library material's voxel id, if this world reaches it.
    pub fn material(&self, name: &str) -> Option<u8> {
        self.library().and_then(|l| l.material(name))
    }

    /// Put the camera somewhere specific (for screenshots and benchmarks).
    /// Put the eye where the EDIT page's turntable says, without disturbing
    /// the body, the ride or anything else the player goes back to.
    pub fn look_from(&mut self, eye: Vec3, yaw: f32, pitch: f32) {
        self.player.look_from(eye, yaw, pitch);
    }

    /// The player was put somewhere on purpose, so a match leaves them
    /// there instead of standing them on one of the spawn points.
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    pub fn set_camera(&mut self, pos: Vec3, yaw: f32, pitch: f32, fly: bool) {
        let allowed = self.ride.allowed;
        self.player = Player::new(pos, yaw);
        self.ride.allowed = allowed;
        self.player.pitch = pitch;
        self.player.fly = fly;
        self.spawned = fly;
    }

    pub fn stream(&mut self, sink: &mut dyn ChunkSink, budget: Duration) {
        // Settle which `when` structures stand before the streamer plans, so
        // the first columns are built without the ones that should not be
        // there: every flag starts out standing, and a far column built with
        // the structure in drew it, and its shadow, until it came round to
        // being rebuilt (Saltmirror's pad 12 at dusk).
        self.update_conditional();
        let fwd = self.player.forward();
        self.streamer.update(self.player.pos, fwd, sink, &mut self.store, budget);
    }

    /// Append the models the modes draw with -- the bike and the wingsuit,
    /// then the guns a shooter holds. Call before handing models to the
    /// renderer.
    pub fn add_ride_models(&mut self) {
        self.ride_models = self.models.len();
        self.models.extend(crate::ride::models());
        crate::ride::palette(&mut self.palette);
        self.vehicle_models = self.models.len();
        let spec = self.ride.catalogue.get(self.ride.vehicle_name().unwrap_or("sand_rail"));
        self.models.extend(crate::vehicle::draw::models(&spec, &mut self.palette));
        self.weapon_models = self.models.len();
        self.models.extend(crate::combat::viewmodel::models());
        crate::combat::viewmodel::palette(&mut self.palette);
        // The bots' bodies come straight after the guns, because an activity
        // is handed one base index and counts on from it. They are built from
        // the library here and again in `Shooter::new`, which wants the rig:
        // `avatar::build` is pure in the library, so the two agree.
        let mut palette = std::mem::take(&mut self.palette);
        let av = crate::brain::avatar::build_drawn(self.library(), &mut palette);
        self.palette = palette;
        self.models.extend(av.models);
    }

    pub fn note(&mut self, msg: &str) {
        self.message = Some((msg.to_string(), 2.5));
    }

    /// Set the tide for the time (world/tide.rs): how far the open sea has
    /// fallen, which every water query reads, and the sea's surface now. A
    /// tide is longer than half a day and shorter than a day, so it runs on
    /// the day count as well as the hour. The hour may also be set directly
    /// (N, the launch screen), so this runs every tick, not only as the clock
    /// moves.
    pub fn follow_tide(&mut self) {
        let Some(w) = self.streamer.generator().world.as_deref() else { return };
        let Some(t) = w.def.tide.filter(|_| w.f.tide.on()) else { return };
        w.f.tide.set_drop(t.drop_at(self.year.day as f32 * 24.0 + self.hour));
        let now = w.f.sea_now();
        let high = 1.0 - w.f.tide.drop() / t.range.max(0.01);
        self.sea = now;
        self.parts.platforms.set_tide(high, w.f.tide.drop());
    }

    /// Hand the swell the clock the renderer draws with and today's wind
    /// (world/surf.rs), so what floats rides the waves on screen.
    pub fn follow_swell(&mut self) {
        let wind = self.weather.sky().wind;
        if let Some(w) = self.streamer.generator().world.as_deref() {
            w.f.shore.set_clock(self.time, wind);
        }
    }

    pub fn tick(&mut self, dt: f32, input: &MoveInput, actions: &mut Actions) {
        if self.clock_running && !self.activity.as_ref().is_some_and(|a| a.pauses_world()) && self.camera.book.is_none() {
            let was = self.hour;
            self.hour = crate::world::clock::advance(self.hour, dt);
            // The weather runs on the game clock, not the wall clock, so a
            // stopped clock holds the sky as well as the light. Across
            // midnight the hour wraps; the length of the step doesn't.
            let step = (self.hour - was).rem_euclid(24.0);
            self.weather.advance(step);
            // The year leans on the weather (a winter is snowier) and the
            // weather feeds it back (snow lies while it falls), so the two
            // are stepped together.
            let sky = self.weather.sky();
            self.weather.lean(self.year.phase().weather);
            let lying = self.year.lying();
            if let Some(turn) = self.year.advance(step, self.hour, sky.snow) {
                let p = self.player.pos;
                self.streamer.set_turn(turn, p);
            }
            let conditions = crate::world::damp::Conditions {
                sky,
                sun_y: crate::render::daylight::sun_dir(self.hour).y,
                cold: self.frost_cold(),
                melt: (lying - self.year.lying()).max(0.0),
            };
            self.damp.advance(step, &conditions);
            self.marks.advance(step, &sky);
            if let Some(k) = self.year.wants_weather() {
                self.weather.nudge(k);
            }
        }
        self.follow_tide();
        self.follow_swell();
        if !self.activity.as_ref().is_some_and(|a| a.pauses_world()) {
            let sky = self.weather.sky();
            self.lightning.advance(dt, &sky, self.player.pos);
        }
        self.frame_ms.push_back(dt * 1000.0);
        while self.frame_ms.len() > 240 {
            self.frame_ms.pop_front();
        }
        if std::mem::take(&mut actions.screenshot_toggle) && self.spawned && (self.in_screenshot() || self.ride.can(ModeId::Screenshot)) {
            if self.in_screenshot() {
                if let Some(mut a) = self.activity.take() {
                    a.exit(&mut self.player);
                }
                self.activity = self.parked.take();
                self.note("BACK TO THE GAME");
            } else {
                self.park_activity();
                self.camera.put_away();
                let cam = self.frame_params(1.0);
                let eye = self.ride.free_camera_start(&self.player, &self.store, cam.cam_pos, cam.cam_fwd);
                self.activity = Some(Box::new(Screenshot::new(eye, self.player.yaw, self.player.pitch)));
            }
        }
        self.start_shooter();
        self.start_race();
        if self.camera_actions(input, actions, dt) {
            // The logbook is open: the world waits.
            actions.dig = false;
            actions.place = false;
            if let Some((_, t)) = &mut self.message {
                *t -= dt;
                if *t <= 0.0 {
                    self.message = None;
                }
            }
            return;
        }
        if std::mem::take(&mut actions.screenshot_take) && self.spawned {
            self.screenshot_request = true;
        }
        // The activity's frame: the keys it takes. Its simulation runs in
        // `fixed_step`, beside the player's body and the world's moving
        // parts, so a windowed match and a headless one are the same thing.
        self.with_activity(|a, mut cx| a.frame(&mut cx, input, actions, dt));
        if self.activity.as_ref().is_some_and(|a| a.pauses_world()) {
            // The world waits, and actions the activity didn't take are
            // dropped. The activity's own steps still run: the scoreboard
            // counts down behind a finished match.
            actions.place = false;
            actions.bike_toggle = false;
            actions.kayak_toggle = false;
            actions.vehicle_toggle = false;
            actions.glide = false;
            actions.cam_toggle = false;
            actions.toggle_fly = false;
            actions.select = None;
            self.run_steps(input, dt, true);
            if let Some((_, t)) = &mut self.message {
                *t -= dt;
                if *t <= 0.0 {
                    self.message = None;
                }
            }
            self.target = None;
            self.looking_at = None;
            self.looking_doing = None;
            return;
        }
        if actions.toggle_fly {
            self.player.fly = !self.player.fly;
            self.player.body.vel = Vec3::ZERO;
            let m = if self.player.fly { "FREE CAMERA" } else { "WALKING" };
            self.note(m);
            actions.toggle_fly = false;
        }
        if std::mem::take(&mut actions.bike_toggle) && self.spawned && !self.player.fly {
            self.ride.toggle_bike(&mut self.player);
        }
        // Taken once: testing the flag twice would spend the press on the
        // first test and leave K doing nothing.
        let kayak_key = std::mem::take(&mut actions.kayak_toggle) && self.spawned && !self.player.fly;
        if kayak_key && self.player.diving() {
            self.note("SURFACE FIRST");
        } else if kayak_key {
            let level = self.water_at(self.player.pos).0;
            self.ride.toggle_kayak(&mut self.player, &self.store, level);
        }
        if std::mem::take(&mut actions.vehicle_toggle) && self.spawned && !self.player.fly {
            self.ride.vehicle_key(&mut self.player, &self.store);
        }
        if std::mem::take(&mut actions.glide) && self.spawned && !self.player.fly {
            self.ride.glide_key(&mut self.player);
        }
        if std::mem::take(&mut actions.cam_toggle) {
            self.ride.toggle_chase();
            if self.ride.mode == crate::ride::Mode::Drive {
                self.settings.drive_chase = self.ride.chase_on();
            }
        }
        // A car is shown in the kept view, so the next one opens in the view
        // the last one was left in.
        self.ride.set_drive_chase(self.settings.drive_chase);
        if let Some(i) = actions.select.take() {
            if i < PALETTE_KEYS.len() {
                self.selected = i;
                self.note(material::name(PALETTE_KEYS[i]));
            }
        }
        if actions.brush_delta != 0 {
            self.brush = match (self.brush, actions.brush_delta > 0) {
                (1, true) => 2,
                (2, true) => 4,
                (4, false) => 2,
                (2, false) => 1,
                (b, _) => b,
            };
            actions.brush_delta = 0;
            self.note(&format!("BRUSH {}", self.brush));
        }

        // Spawn onto the ground once it has loaded.
        if !self.spawned {
            let p = self.player.pos;
            let (vx, vz) = ((p.x * 4.0).floor() as i32, (p.z * 4.0).floor() as i32);
            let top_vox = self.store.top_vox();
            if self.store.loaded(vx, 0, vz) && self.store.loaded(vx, top_vox - 1, vz) {
                let mut top = None;
                let start = ((p.y.max(1.0)) * 4.0) as i32 + 8;
                for vy in (0..start.min(top_vox - 1)).rev() {
                    if self.store.get(vx, vy, vz).unwrap_or(AIR) != AIR {
                        top = Some((vy + 1) as f32 / 4.0);
                        break;
                    }
                }
                if let Some(t) = top {
                    if t > p.y - 0.01 {
                        self.player.pos.y = t;
                    }
                    self.player.prev_pos = self.player.pos;
                }
                // A spot inside the ground -- a world rebuilt under an old
                // save, a camera that ended up in a hill -- comes up to the
                // first standing room above it rather than leaving you
                // buried in the rock with nothing to walk on.
                let feet = (self.player.pos.y * 4.0).floor() as i32;
                if self.store.get(vx, feet, vz).unwrap_or(AIR) != AIR {
                    let mut air = 0;
                    for vy in feet..top_vox {
                        if self.store.get(vx, vy, vz).unwrap_or(AIR) == AIR {
                            air += 1;
                            if air >= 8 {
                                self.player.pos.y = (vy - 7) as f32 / 4.0;
                                self.player.prev_pos = self.player.pos;
                                break;
                            }
                        } else {
                            air = 0;
                        }
                    }
                }
                self.spawned = true;
            }
        }

        // Turning with the keys is view input: once a frame, with the
        // frame's own dt, beside the mouse (see Player::turn_by_keys).
        self.player.turn_by_keys(&self.ride, input, dt);
        self.run_steps(input, dt, false);
        if let Some(m) = self.ride.note.take() {
            self.note(&m);
        }

        // Aim, dig and build.
        let tr = crate::trace::start();
        let eye = self.eye(self.alpha);
        let fwd = self.player.forward();
        self.target = if self.spawned { self.store.raycast(eye, fwd, 6.0) } else { None };
        (self.looking_at, self.looking_doing) = if self.spawned { self.identify(eye, fwd) } else { (None, None) };
        crate::trace::end("aim", tr);
        self.dig_cd -= dt;
        self.place_cd -= dt;
        if actions.dig && self.dig_cd <= 0.0 {
            if let Some((hit, _)) = self.target {
                self.dig(hit);
                self.dig_cd = 0.14;
            }
        }
        if !actions.dig {
            self.dig_cd = 0.0;
        }
        if actions.place && self.place_cd <= 0.0 {
            if let Some((_, prev)) = self.target {
                self.place(prev);
                self.place_cd = 0.2;
            }
        }
        if !actions.place {
            self.place_cd = 0.0;
        }
        if self.ents.just_collected > 0 {
            self.ents.just_collected = 0;
            let n = self.ents.gems_collected.len();
            match self.ents.last_found.take() {
                Some(what) => {
                    let same = self.ents.found_count(&what);
                    self.note(&format!("FOUND: {}  ({same})", what.to_uppercase()));
                }
                None => self.note(&format!("GEM FOUND  ({n})")),
            }
        }
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        self.autosave -= dt;
        if self.autosave <= 0.0 {
            self.autosave = 30.0;
            if self.streamer.dirty_edits {
                self.save_with(false);
            }
        }
    }

    /// In a zoo (creatures/zoo.rs), go to the pen `step` on from the one
    /// the player is at: PageDown and PageUp.
    pub fn zoo_jump(&mut self, step: i32) {
        let Some(z) = &self.ents.zoo else { return };
        let n = z.pens.len() as i32;
        let Some(here) = z.nearest(self.player.pos) else { return };
        let next = (here as i32 + step).rem_euclid(n.max(1)) as usize;
        self.go_to_pen(next);
    }

    fn go_to_pen(&mut self, i: usize) {
        let Some(z) = &self.ents.zoo else { return };
        let Some((at, look)) = z.viewpoint(i) else { return };
        let name = self.ents.species[z.pens[i].species].name.replace('_', " ").to_uppercase();
        let total = z.pens.len();
        let d = look - at;
        let yaw = (-d.x).atan2(-d.y);
        let h = self.generator().height(at.x, at.y);
        self.set_camera(Vec3::new(at.x, h + 2.0, at.y), yaw, -0.25, false);
        self.message = Some((format!("PEN {} OF {total}: {name}", i + 1), 2.5));
    }

    /// Start a display on the creature under the crosshair (the F10 dev
    /// key): the next of its species' displays that can start there.
    pub fn force_display(&mut self) {
        let msg = match &self.looking_doing {
            Some((_, e)) => {
                self.ents.shows.forced = Some(*e);
                "DISPLAY: TRYING"
            }
            None => "DISPLAY: NO CREATURE UNDER THE CROSSHAIR",
        };
        self.message = Some((msg.to_string(), 2.0));
    }

    /// Names the creature or tree on the line of sight, whichever is nearer.
    fn identify(&self, eye: Vec3, fwd: Vec3) -> (Option<String>, Option<(String, hecs::Entity)>) {
        match self.identify_inner(eye, fwd) {
            Some((name, doing)) => (Some(name), doing),
            None => (None, None),
        }
    }

    fn identify_inner(&self, eye: Vec3, fwd: Vec3) -> Option<(String, Option<(String, hecs::Entity)>)> {
        const RANGE: f32 = 60.0;
        let block = self.store.raycast(eye, fwd, RANGE);
        let block_t = block.map_or(RANGE, |(v, _)| {
            let c = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * 0.25;
            (c - eye).length()
        });
        if let Some((t, name, doing, e)) = self.ents.identify_one(&self.models, eye, fwd, RANGE) {
            if t < block_t + 0.5 {
                return Some((name, Some((doing, e))));
            }
        }
        let (v, _) = block?;
        let m = self.store.get(v[0], v[1], v[2]).unwrap_or(AIR);
        let p = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * 0.25;
        let gen = self.generator();
        let trees = gen.trees_near(p.x - 0.5, p.z - 0.5, p.x + 0.5, p.z + 0.5);
        let tree = trees
            .iter()
            .filter(|t| m == t.leaf || crate::world::matlib::base(m) == t.bark)
            .filter(|t| p.y > t.pos.y - 1.0 && p.y < t.pos.y + t.top + 1.0)
            .min_by(|a, b| {
                let da = Vec3::new(a.pos.x - p.x, 0.0, a.pos.z - p.z).length_squared();
                let db = Vec3::new(b.pos.x - p.x, 0.0, b.pos.z - p.z).length_squared();
                da.partial_cmp(&db).unwrap()
            })?;
        let name = match gen.world.as_ref().and_then(|w| w.lib.plants.get(tree.plant as usize)) {
            Some(pl) => pl.name.replace('_', " "),
            None => match tree.kind {
                crate::world::gen::TreeKind::Broad => "BROADLEAF".to_string(),
                crate::world::gen::TreeKind::Birch => "BIRCH".to_string(),
                crate::world::gen::TreeKind::Pine => "PINE".to_string(),
                crate::world::gen::TreeKind::Vine => "VINE".to_string(),
                crate::world::gen::TreeKind::Crust => "MOSS".to_string(),
                crate::world::gen::TreeKind::Bracket => "FUNGUS".to_string(),
            },
        };
        Some((name.to_uppercase(), None))
    }

    /// The water surface at a point (a river's, else the sea) and the
    /// current there. On dry land the level is the sea's, below the ground.
    ///
    /// Water inside a structure is voxels rather than a field -- an
    /// imported map fills its water zones solid -- and `water_at` counts
    /// those in too.
    pub fn water_at(&self, p: Vec3) -> (f32, glam::Vec2) {
        water_at(self.streamer.generator().world.as_deref(), self.sea, &self.store, p)
    }

    /// Structures that stand only at some hours or distances (`place`
    /// `when`): flip the ones whose time has come and redraw their ground.
    fn update_conditional(&mut self) {
        let Some(w) = self.streamer.generator().world.clone() else {
            return;
        };
        if w.when.is_empty() {
            return;
        }
        let p = self.player.pos;
        // The moon is out once the sun is 5 degrees down (render/daylight.rs
        // fades it in from 3 to 7).
        let moon_out = crate::render::daylight::sun_dir(self.hour).y < -(5.0f32.to_radians().sin());
        for (k, when) in &w.when {
            let Some(flag) = w.shown.get(*k as usize) else {
                continue;
            };
            let (x0, z0, x1, z1) = w.placed[*k as usize].footprint(&w.shapes);
            let (x0, z0, x1, z1) = (x0 as f32 * 0.25, z0 as f32 * 0.25, x1 as f32 * 0.25, z1 as f32 * 0.25);
            let c = glam::Vec2::new((x0 + x1) * 0.5, (z0 + z1) * 0.5);
            let was = flag.load(std::sync::atomic::Ordering::Relaxed);
            let now = when.stands(self.hour, c.distance(glam::Vec2::new(p.x, p.z)), moon_out, was);
            if now != was {
                flag.store(now, std::sync::atomic::Ordering::Relaxed);
                self.streamer.regen_area(x0 - 1.0, z0 - 1.0, x1 + 1.0, z1 + 1.0, p);
            }
        }
    }

    /// Spend the frame's time on fixed steps. `paused` is the activity
    /// holding the world still: only the moving parts and the activity run.
    fn run_steps(&mut self, input: &MoveInput, dt: f32, paused: bool) {
        // A paused world keeps each body's last two steps, and blending them
        // by a fresh alpha every frame shook anything caught mid-stride back
        // and forth (screenshot mode, F2). Hold the blend it paused at.
        self.still_alpha = if paused { Some(self.still_alpha.unwrap_or(self.alpha)) } else { None };
        self.acc = (self.acc + dt).min(0.25);
        while self.acc >= STEP {
            self.acc -= STEP;
            self.fixed_step(input, paused);
        }
        self.alpha = self.acc / STEP;
    }

    /// Hand the activity the world for one call (`frame` or `step`), then
    /// apply what it asked for: the voxels it dug or laid, a mended arena,
    /// the effects of its shots and a line for the corner.
    fn with_activity(&mut self, f: impl FnOnce(&mut dyn Activity, ActivityCtx)) {
        let Some(mut a) = self.activity.take() else {
            return;
        };
        let slopes = self.streamer.smooth();
        let mut edits: Vec<([i32; 3], u8)> = Vec::new();
        let cx = ActivityCtx {
            player: &mut self.player,
            store: &self.store,
            ents: &mut self.ents,
            models: &self.models,
            sea: self.sea,
            land: self.streamer.generator().world.as_deref(),
            slopes,
            screenshot_request: &mut self.screenshot_request,
            edits: &mut edits,
        };
        f(a.as_mut(), cx);
        // What the activity dug or laid, applied in one go: the streamer
        // marks the chunks and the mesher rebuilds them.
        //
        // The floor rule is `Game`'s, not the activity's: nothing below
        // what `Generator::dig_floor_vox` allows may be taken away,
        // because under the generated ground the world keeps no detail
        // and a hole into it is a hole out of the world. A gun that cuts
        // does not get to know about that, and should not have to.
        if !edits.is_empty() {
            let gen = self.streamer.generator();
            edits.retain(|&([x, y, z], m)| m != crate::world::material::AIR || (y as f32) >= gen.dig_floor_vox(x, z));
            self.streamer.edit(&mut self.store, &edits);
        }
        // A new match: the holes the last one blew in the arena go.
        if a.wants_world_clean() {
            let undone = self.streamer.clear_edits(self.player.pos);
            // A race starting over says nothing: its grid says it all.
            if undone > 0 && a.id() != ModeId::Race {
                self.note("THE ARENA IS MENDED");
            }
        }
        if let Some(sh) = a.shooter_mut() {
            let mut cues = sh.combat.take_cues();
            // Only what bleeds bleeds.
            cues.retain(|c| !matches!(c, crate::combat::Cue::Flesh { who, .. } if !sh.bleeds(*who)));
            self.combat_fx(&cues);
            self.combat_sounds(&cues);
        } else if let Some(r) = a.race_mut() {
            let mut cues = r.combat_cues();
            cues.retain(|c| !matches!(c, crate::combat::Cue::Flesh { .. }));
            self.combat_fx(&cues);
            self.combat_sounds(&cues);
        }
        if let Some(m) = a.note() {
            self.note(&m);
        }
        self.activity = Some(a);
    }

    fn fixed_step(&mut self, input: &MoveInput, paused: bool) {
        // The moving parts first, then the activity, then the player: a
        // rider steps off a floor that has already moved this step.
        let tr = crate::trace::start();
        let player = self.spawned.then_some(&mut self.player);
        self.parts.step(STEP, &mut self.store, player, self.activity.as_deref_mut());
        crate::trace::end("platforms", tr);
        if std::mem::take(&mut self.parts.placed) {
            // A platform has just been put where it lives: keep the grass
            // out of the room it sweeps, and mesh the ground under it again.
            let boxes = self.parts.platforms.keep_out();
            crate::mesh::sprites::set_keep_out(boxes.clone());
            for b in boxes {
                self.streamer.remesh_box(&self.store, Vec3::new(b[0], b[1] - 2.0, b[2]), Vec3::new(b[3], b[4], b[5]));
            }
        }
        let tr = crate::trace::start();
        self.with_activity(|a, cx| a.step(cx, input));
        crate::trace::end("activity", tr);
        if paused {
            return;
        }
        self.time += STEP;
        let slopes = self.streamer.smooth();
        // The EDIT page's camera owns the eye while it is open (editor/cam.rs),
        // so the body is left standing where it was and put back untouched
        // when the page closes.
        let tr = crate::trace::start();
        if self.spawned && !self.editing {
            let water = self.water_at(self.player.pos);
            // How the water slopes here, for the kayak to pitch and roll with
            // the swell.
            let p = self.player.pos;
            let e = 0.8;
            let dx = self.water_at(p + Vec3::X * e).0 - self.water_at(p - Vec3::X * e).0;
            let dz = self.water_at(p + Vec3::Z * e).0 - self.water_at(p - Vec3::Z * e).0;
            self.ride.water_slope = if (dx.abs() + dz.abs()) < 1.0 { glam::Vec2::new(dx, dz) / (2.0 * e) } else { glam::Vec2::ZERO };
            let before = (self.player.body.on_ground, self.player.body.vel, self.player.pos);
            self.player.update(&mut self.ride, input, &self.store, self.sea, water, slopes, STEP);
            if let Some(r) = self.activity.as_mut().and_then(|a| a.race_mut()) {
                r.tick(&mut self.ride, &mut self.player, &self.store, self.sea, slopes, STEP);
                let (cleared, marks) = r.polish_marks();
                self.polish.take(cleared, marks);
                // A finish goes to the records in the save folder, which the
                // race's course cards read.
                if let Some((laps, race, lap, kart)) = r.result() {
                    let root = self.save_dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.save_dir.clone());
                    let mut recs = crate::race::records::Records::load(&root);
                    let beaten = recs.note(&self.launch.world, laps, race, lap, &kart);
                    recs.save(&root);
                    r.beat(beaten);
                }
            } else {
                // Out of a race the ride is its own again.
                self.ride.clear_race();
            }
            self.step_fx(before, water.0);
            self.dive_extras();
            // Off the edge of a map in space with no match on to kill you:
            // back to the spawn, rather than standing on the world's floor
            // under it. A match does its own (Shooter `scald`).
            if self.activity.is_none() && !self.player.fly {
                let void = self.generator().world.as_ref().and_then(|w| w.def.shooter.as_ref()).and_then(|s| s.void_m);
                if void.is_some_and(|v| self.player.pos.y < v) {
                    self.player = Player::new(self.home.0 + Vec3::Y * 0.5, self.player.yaw);
                }
            }
        } else {
            self.player.prev_pos = self.player.pos;
        }
        crate::trace::end("player", tr);
        let runs = self.eco_runs();
        if runs > 0 {
            // How the animals read the player: where, how fast and how they
            // travel (creatures/startle.rs). Flying with the cheat on reads
            // as standing still, so nothing bolts from a camera in the sky.
            let vel = if self.player.fly { Vec3::ZERO } else { self.player.body.vel };
            let threat = self.player_threat(vel);
            let gen = self.streamer.generator();
            self.ents.hour = self.hour;
            self.ents.sky = self.weather.sky();
            self.ents.phase = self.year.phase();
            self.ents.set_season(self.year.name());
            crate::render::daylight::set_declination(self.ents.phase.declination);
            let f = self.player.forward();
            self.ents.facing = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
            let tr = crate::trace::start();
            for _ in 0..runs {
                self.ents.update(STEP, &self.store, gen, &threat, slopes, &self.models);
            }
            crate::trace::end("creatures", tr);
            self.eco_after_step();
            self.live_step(STEP * runs as f32, &threat);
            // VG_ZOO picked the pens: start at the first.
            // A camera pinned with --pos stays where it was put.
            if self.ents.zoo.as_mut().is_some_and(|z| std::mem::take(&mut z.jump)) && !self.pinned {
                self.go_to_pen(0);
            }
            // Spray the animals threw up this step (creatures/swim.rs).
            self.fx.take(&mut self.ents.cues);
            self.lay_marks();
            self.trample_shrubs();
        }
        self.thump.1 += STEP;
        if self.spawned && !self.editing {
            self.hitch_step(input.focus_far && !self.camera.active);
        }
        if self.spawned && !self.player.fly {
            if self.ride.car().is_some() {
                self.shove_animals();
                self.crush_shrubs();
            } else {
                self.shoulder_past_animals();
            }
        }
    }

    /// A heavy animal's footfall near the player is felt as a small jolt of
    /// the view: a centimetre or so beside an elephant, nothing past a
    /// couple of dozen metres, and a little more from a brachiosaur, which
    /// is felt further off too. The strongest recent one wins rather than
    /// adding up, so a herd going by is a steady tremor, not an earthquake.
    /// `VG_THUMP=k` scales it (0 turns it off).
    fn feel_steps(&mut self, steps: &[crate::creatures::sign::Step]) {
        static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        let scale = *SCALE.get_or_init(|| std::env::var("VG_THUMP").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0));
        if scale <= 0.0 {
            return;
        }
        let me = self.player.pos;
        for s in steps.iter().filter(|s| s.weight > 0.0) {
            let range = 10.0 + 12.0 * s.weight;
            let d = (s.pos - me).length();
            if d >= range {
                continue;
            }
            let k = s.weight * (1.0 - d / range).powi(2) * scale;
            self.ents.pathing.thumps_felt += 1;
            // What is left of the last one, against this one.
            let left = self.thump.0 * (-self.thump.1 * THUMP_FADE).exp();
            if k > left {
                self.thump = (k.min(2.5), 0.0);
            }
        }
    }

    /// Flatten the shrubs the big animals walked into this step, and stand
    /// back up the ones that have been down long enough
    /// (creatures/trample.rs).
    fn trample_shrubs(&mut self) {
        let bases = std::mem::take(&mut self.ents.crush);
        let now = self.ents.clock;
        let world = self.streamer.generator().world.clone();
        let bark_of = |i: u16| crate::creatures::trample::bark_of(world.as_deref(), i);
        let mut changes = if bases.is_empty() { Vec::new() } else { self.trample.flatten(&self.ents.plants, &self.store, bark_of, now, &bases) };
        self.ents.pathing.trampled = self.trample.count;
        // The walkers' shapes are only built on a step a shrub is due.
        if self.trample.due(now) {
            let big: Vec<crate::creatures::crowd::Shape> = crate::creatures::crowd::shapes(&self.ents.world, &self.models)
                .into_iter()
                .map(|(_, s, _)| s)
                .filter(|s| crate::creatures::trample::big(s.r, s.y1 - s.y0))
                .collect();
            changes.extend(self.trample.regrow(&self.store, now, |q, r| big.iter().any(|s| s.spine_dist(q) < s.r + r)));
        }
        if !changes.is_empty() {
            self.streamer.edit_passing(&mut self.store, &changes);
        }
    }

    /// The car's rear tyres leave a track on ground that takes one: a piece
    /// every `Foot::Tyre` length, laid from the last piece to here. A kart
    /// leaves none: a circuit stays clean but for the polish.
    fn lay_tracks(&mut self, soft: f32) {
        use crate::creatures::sign::{Foot, Step};
        let Some(car) = self.ride.car().filter(|c| c.spec.model != "kart") else {
            self.tyre_marks = [None; 2];
            return;
        };
        let len = Foot::Tyre.size(1.0);
        let up = car.up();
        let r = car.spec.wheel_radius;
        let mut laid = Vec::new();
        for (k, w) in car.wheels.iter().filter(|w| w.rear).enumerate() {
            let patch = match (w.down, w.contact) {
                (true, Some(c)) if c.depth < 0.05 => crate::vehicle::draw::drawn_at(car, car.pos() + car.b.rot * w.mount - up * w.t - c.n * r),
                _ => {
                    self.tyre_marks[k] = None;
                    continue;
                }
            };
            let Some(last) = self.tyre_marks[k] else {
                self.tyre_marks[k] = Some(patch);
                continue;
            };
            let d = patch - last;
            if Vec3::new(d.x, 0.0, d.z).length() > 4.0 {
                self.tyre_marks[k] = Some(patch);
                continue;
            }
            // As many pieces as the tyre covered this step: one a step fell
            // behind above 0.6 m a step (130 km/h), and the track wandered
            // off the tyre on a fast lap.
            let mut last = last;
            loop {
                let d = patch - last;
                let flat = Vec3::new(d.x, 0.0, d.z);
                if flat.length() < len {
                    break;
                }
                let dir = flat.normalize();
                let end = last + dir * len;
                // Height along the way to the patch, so a piece on a slope
                // lies on it.
                let y = last.y + (patch.y - last.y) * (len / flat.length());
                let mid = last + dir * len * 0.5;
                laid.push(Step {
                    pos: Vec3::new(mid.x, (last.y + y) * 0.5 + 0.01, mid.z),
                    yaw: (-dir.x).atan2(-dir.z),
                    foot: Foot::Tyre,
                    height: 1.0,
                    weight: 0.0,
                });
                last = Vec3::new(end.x, y, end.z);
            }
            self.tyre_marks[k] = Some(last);
        }
        for step in laid {
            let ground = crate::creatures::sign::ground_at(&self.store, step.pos + Vec3::Y * 0.3);
            self.marks.lay(&step, ground, soft);
        }
    }

    /// One step of the particles: what the player's body did this step
    /// (`before` is its footing, velocity and position going in), what the
    /// trees are dropping, and the pool moved on. Cosmetic only: nothing
    /// here is read back by the simulation.
    /// Lay what feet left this step (creatures/sign.rs): the walkers'
    /// footfalls, and a bootprint each stride the player walks.
    fn lay_marks(&mut self) {
        let soft = self.damp.soft();
        // Dev hook: VG_MARKS=1 lays a short trail of each foot ahead of the
        // player once the world is in, so the prints can be judged in a
        // headless shot; on ground that keeps no print they show nothing.
        if self.marks.version == 0 && self.spawned && std::env::var("VG_MARKS").is_ok() {
            use crate::creatures::sign::{Foot, Step};
            let f = self.player.forward();
            let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
            let across = Vec3::new(-fwd.z, 0.0, fwd.x);
            let yaw = (-fwd.x).atan2(-fwd.z);
            let feet = [(Foot::Hoof, 1.4f32), (Foot::Paw, 0.6), (Foot::Bear, 1.3), (Foot::Bird, 1.0), (Foot::Boot, 1.8)];
            for (row, (foot, height)) in feet.into_iter().enumerate() {
                for i in 0..6 {
                    let side = if i % 2 == 0 { 0.06 } else { -0.06 } * height;
                    let mut pos = self.player.pos + fwd * (2.0 + i as f32 * 0.6 * height.max(0.8)) + across * ((row as f32 - 2.0) * 0.9 + side);
                    // Down the column to the first solid ground, then onto
                    // the drawn (sloped) surface.
                    let (vx, vz) = ((pos.x * 4.0).floor() as i32, (pos.z * 4.0).floor() as i32);
                    let top = ((pos.y + 4.0) * 4.0).floor() as i32;
                    if let Some(vy) = (0..64).map(|d| top - d).find(|&y| {
                        self.store
                            .get(vx, y, vz)
                            .is_some_and(|m| m != crate::world::material::AIR && !crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE))
                    }) {
                        pos.y = (vy + 1) as f32 / 4.0;
                        pos.y += crate::physics::drawn_ground_lift(&self.store, pos);
                    }
                    let step = Step { pos, yaw, foot, height, weight: 0.0 };
                    let ground = crate::creatures::sign::ground_at(&self.store, pos + Vec3::Y * 0.5);
                    self.marks.lay(&step, ground, 1.0);
                }
            }
        }
        let steps = std::mem::take(&mut self.ents.steps);
        self.feel_steps(&steps);
        for step in steps {
            let ground = crate::creatures::sign::ground_at(&self.store, step.pos);
            self.marks.lay(&step, ground, soft);
        }
        self.lay_tracks(soft);
        let on_foot = self.ride.mode == crate::ride::Mode::Foot && self.player.body.on_ground && !self.player.fly && !self.player.swimming && self.activity.is_none();
        if !on_foot {
            return;
        }
        let d = self.player.pos - self.player.prev_pos;
        let moved = Vec3::new(d.x, 0.0, d.z).length();
        if moved > 1.0 {
            return;
        }
        self.boot.0 += moved;
        if self.boot.0 < 0.75 {
            return;
        }
        self.boot = (0.0, !self.boot.1);
        let f = self.player.forward();
        let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
        let across = Vec3::new(-fwd.z, 0.0, fwd.x) * if self.boot.1 { 0.13 } else { -0.13 };
        let step = crate::creatures::sign::Step {
            pos: self.player.pos + across,
            yaw: (-fwd.x).atan2(-fwd.z),
            foot: crate::creatures::sign::Foot::Boot,
            height: 1.8,
            weight: 0.0,
        };
        let ground = crate::creatures::sign::ground_at(&self.store, step.pos);
        self.marks.lay(&step, ground, soft);
    }

    /// How the animals sense the player this step (creatures/startle.rs):
    /// where, how fast, how they travel and crouch, how loud the ground
    /// underfoot is with the wet on it and the rain over it, and where the
    /// wind takes their scent. The wind is the world's one direction
    /// (`world::WIND`, which the grass leans with, so the player can read
    /// it), swung a little by the gusts; on a still day the scent pools.
    fn player_threat(&self, vel: Vec3) -> crate::creatures::startle::Threat {
        let sky = self.weather.sky();
        let under = crate::fx::hooks::ground_under(&self.store, self.player.pos);
        let tread = crate::creatures::sign::tread_of(under);
        let snow = if self.year.turn().snow > 0 { 0.6 } else { 1.0 };
        let loud = tread.loudness(self.damp.wet) * snow;
        let masked = sky.rain * 0.7;
        let w = crate::world::WIND;
        let swing = (self.time * 0.23).sin() * 0.6 * sky.gustiness();
        let (s, c) = swing.sin_cos();
        let toward = Vec3::new(w.x * c - w.y * s, 0.0, w.x * s + w.y * c);
        let carry = sky.wind.clamp(0.0, 1.2) * (1.0 - 0.5 * sky.rain);
        let pool = (1.0 - sky.wind / 0.35).clamp(0.0, 1.0);
        crate::creatures::startle::Threat::new(self.player.pos, vel, self.ride.mode)
            .crouched(self.player.crouch)
            .heard(loud, masked)
            .scented(toward, if pool > 0.0 { 0.0 } else { carry }, pool)
    }

    /// How readily frost forms on a clear night in this part of this world's
    /// How strong the mirage is just now, 0 to 1 (post.wgsl `mirage`): the
    /// world's own strength, at full in the heat and at about half on a fair
    /// day, only with the sun well up, and gone once the ground is wet.
    fn mirage(&self) -> f32 {
        let k = self.generator().world.as_ref().map_or(0.0, |w| w.def.mirage);
        if k <= 0.0 {
            return 0.0;
        }
        let sky = self.weather.sky();
        let air = (sky.heat + 0.5 * (1.0 - sky.overcast) * (1.0 - sky.cover.max(0.0))).min(1.0) * (1.0 - sky.rain);
        let sun_y = crate::render::daylight::sun_dir(self.hour).y;
        let noon = ((sun_y - 0.35) / 0.4).clamp(0.0, 1.0);
        let dry = 1.0 - (self.damp.wet * 1.5).min(1.0);
        (k * air * noon * dry).clamp(0.0, 1.0)
    }

    /// year (world/damp.rs `Conditions::cold`): every winter night can, a
    /// spring or autumn one half as hard, a summer one never. A country
    /// whose climate never snows gets a touch of it at most.
    fn frost_cold(&self) -> f32 {
        use crate::world::season::Season;
        let season = match self.year.season {
            Season::Winter => 1.0,
            Season::Spring | Season::Autumn => 0.55,
            Season::Summer => 0.0,
        };
        let snows = self.weather.climate().has(crate::world::weather::Kind::Snow);
        season * if snows { 1.0 } else { 0.25 }
    }

    fn step_fx(&mut self, before: (bool, Vec3, Vec3), water: f32) {
        use crate::world::season::Season;
        if !self.fx.on {
            return;
        }
        if let Some(w) = self.streamer.generator().world.as_deref() {
            self.fx.use_library(&w.lib);
        }
        let sky = self.weather.sky();
        let w = crate::world::WIND;
        self.fx.wind = Vec3::new(w.x, 0.0, w.y) * 3.0 * sky.wind;
        let season = self.year.season;
        let snow = self.year.turn().snow > 0;
        // A raw morning or evening in spring or autumn is cold enough too.
        let raw = matches!(season, Season::Spring | Season::Autumn) && (self.hour < 8.5 || self.hour > 20.5);
        let cold = season == Season::Winter || snow || sky.snow > 0.05 || raw;
        let eye = self.eye(1.0);
        let body = crate::fx::hooks::Body {
            pos: self.player.pos,
            prev_pos: before.2,
            vel: self.player.body.vel,
            vel_before: before.1,
            on_ground: self.player.body.on_ground,
            was_ground: before.0,
            swimming: self.player.swimming,
            fly: self.player.fly,
            water,
            mode: self.ride.mode,
            forward: self.player.forward(),
            eye,
            strokes: self.ride.strokes(),
            stroke_side: self.ride.stroke_side(),
            cold,
            snow,
        };
        crate::fx::hooks::step(&mut self.fx, &body, &self.store, STEP);
        if let Some(car) = self.ride.car() {
            crate::fx::hooks::vehicle(&mut self.fx, car, &self.ride.drive_events, STEP);
            crate::fx::hooks::drift(&mut self.fx, car, STEP);
        }
        // The rivals' drifts smoke and spark as the player's do.
        if let Some(a) = &self.activity {
            let fx = &mut self.fx;
            a.each_race_kart(&mut |car| {
                if (car.pos() - eye).length() < 120.0 {
                    crate::fx::hooks::drift(fx, car, STEP);
                }
            });
        }
        crate::fx::ambient::step(&mut self.fx, &self.store, eye, season, sky.wind, STEP);
        // Falling snow near the camera, as flakes in the world: they drift
        // past as you move, which the screen-space fall (post.wgsl) cannot.
        // Spawned over a box round the eye, a little above it, so they come
        // down into view; split over several points a step so they don't clump.
        if sky.snow > 0.02 {
            let n = self.time / STEP;
            const SPOTS: usize = 8;
            for k in 0..SPOTS {
                let s = n * SPOTS as f32 + k as f32;
                let fx = ((s * 0.7548777) % 1.0) * 2.0 - 1.0;
                let fz = ((s * 0.5698403) % 1.0) * 2.0 - 1.0;
                let up = 3.0 + ((s * 0.3819660) % 1.0) * 8.0;
                let at = eye + Vec3::new(fx * SNOW_BOX, up, fz * SNOW_BOX);
                self.fx.trickle("snowflake", at, -Vec3::Y, 0, SNOW_RATE * sky.snow / SPOTS as f32, STEP);
            }
        }
        // What the world breathes all the time (`WorldVent`): steam off a hot
        // spring, smoke from a chimney, while the camera is near enough to see.
        if let Some(w) = self.generator().world.clone() {
            // Diamond dust: crystals in clear frozen air, only while the sun
            // is up to catch them and the sky is open.
            let dd = w.def.diamond_dust;
            if dd > 0.0 && sky.snow < 0.05 && sky.overcast < 0.4 && crate::world::clock::sun_elevation(self.hour) > 0.0 {
                let n = self.time / STEP;
                let (a, r) = ((n * 2.399) % std::f32::consts::TAU, 1.5 + 9.0 * ((n * 0.618) % 1.0));
                let at = eye + Vec3::new(a.cos() * r, ((n * 0.37) % 1.0) * 5.0 - 1.0, a.sin() * r);
                self.fx.trickle("diamond_dust", at, Vec3::Y, 0, 70.0 * dd, STEP);
            }
            for (k, v) in w.vents.iter().enumerate() {
                if v.pos.distance_squared(eye) > VENT_REACH * VENT_REACH {
                    continue;
                }
                // A point somewhere across the vent, moved on each step.
                let n = self.time / STEP;
                let a = (n * 2.399 + k as f32 * 1.7) % std::f32::consts::TAU;
                let r = v.spread * (((n * 0.618 + k as f32) % 1.0).sqrt());
                let at = v.pos + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                self.fx.trickle(&v.fx, at, Vec3::Y, 0, v.rate, STEP);
            }
        }
        self.fx.step(STEP, &self.store);
    }

    /// A fight's cues as sounds (src/sound): what a round struck, by what
    /// it is made of, now and then a ricochet off something hard, and the
    /// bursts. The shots themselves are posted where they are fired.
    fn combat_sounds(&self, cues: &[crate::combat::Cue]) {
        use crate::combat::Cue;
        use crate::sound::{emit, Kind, Noise};
        for c in cues {
            match *c {
                Cue::Impact { at, dir } => {
                    let d = dir.normalize_or(Vec3::NEG_Y);
                    let p = at + d * 0.06;
                    let m = self.store.get((p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32).unwrap_or(AIR);
                    if m == AIR {
                        continue;
                    }
                    let g = crate::sound::watch::ground_of(m);
                    emit(Noise::at(Kind::Impact(g), at, 0.6));
                    // One round in four whines off rock or metal; which one
                    // is down to where it struck, so it needs no generator.
                    let hash = ((at.x * 12.9898 + at.y * 78.233 + at.z * 37.719).sin() * 43_758.547).fract().abs();
                    if g == crate::sound::Ground::Stone && hash < 0.25 {
                        emit(Noise::at(Kind::Ricochet, at, 0.5));
                    }
                }
                Cue::Burst {
                    at,
                    radius,
                    look: crate::library::SplashLook::Goo,
                } => emit(Noise::at(Kind::Splat(radius), at, 1.2)),
                Cue::Burst { at, radius, .. } => emit(Noise::at(Kind::Burst(radius), at, 2.0 * (radius / 3.0).clamp(0.4, 2.0).sqrt())),
                Cue::Sparks { at } => emit(Noise::at(Kind::Sparks, at, 0.6)),
                Cue::Flesh { .. } => {}
                Cue::Muzzle { .. } => {}
            }
        }
    }

    /// A fight's cues as particles: smoke and sparks at a muzzle, chips off
    /// a wall in its own colour, the smoke and dirt of a burst.
    fn combat_fx(&mut self, cues: &[crate::combat::Cue]) {
        use crate::combat::Cue;
        use crate::world::material::AIR;
        if !self.fx.on {
            return;
        }
        let vox = |p: Vec3| [(p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32];
        for c in cues {
            match *c {
                Cue::Muzzle { at, dir } => {
                    self.fx.burst("muzzle_smoke", at + dir * 0.05, dir, 0, 1.0);
                    self.fx.burst("bore_sparks", at, dir, 0, 1.0);
                }
                Cue::Impact { at, dir } => {
                    let d = dir.normalize_or(Vec3::NEG_Y);
                    let [x, y, z] = vox(at + d * 0.06);
                    let m = self.store.get(x, y, z).unwrap_or(AIR);
                    if m == AIR {
                        continue;
                    }
                    // Back out of the wall, the way the round came in.
                    let out = at - d * 0.04;
                    self.fx.burst("impact_chips", out, -d, m, 1.0);
                    self.fx.burst("impact_dust", out, -d, m, 1.0);
                    if crate::fx::hooks::hard(m) {
                        self.fx.burst("impact_sparks", out, -d, 0, 1.0);
                    }
                }
                Cue::Flesh { at, dir, .. } => {
                    // Back out the way the round went in: a puff of mist and
                    // a few drops that fall.
                    let d = dir.normalize_or(Vec3::NEG_Y);
                    let out = at - d * 0.05;
                    self.fx.burst("blood_puff", out, -d, 0, 1.0);
                    self.fx.burst("blood_drops", out, -d + Vec3::Y * 0.3, 0, 1.0);
                }
                Cue::Sparks { at } => {
                    self.fx.burst("impact_sparks", at, Vec3::Y, 0, 2.0);
                    self.fx.burst("bore_sparks", at, Vec3::Y, 0, 1.5);
                }
                Cue::Burst {
                    at,
                    radius,
                    look: crate::library::SplashLook::Goo,
                } => {
                    // Slime, not fire: a pale green flash, a green spray
                    // that hangs and drips, droplets flung out and falling,
                    // and a sour green haze where the smoke would be.
                    let k = (radius / 3.0).clamp(0.5, 2.0);
                    self.fx.burst("goo_burst_flash", at, Vec3::Y, 0, 1.0);
                    self.fx.burst_in("goo_burst", at, radius * 0.08, Vec3::Y, 0, k);
                    self.fx.burst_in("goo_burst_deep", at, radius * 0.1, Vec3::Y, 0, k);
                    self.fx.burst("goo_burst_drops", at, Vec3::Y, 0, k);
                    self.fx.burst("goo_burst_haze", at, Vec3::Y, 0, k);
                }
                Cue::Burst { at, radius, .. } => {
                    let k = (radius / 3.0).clamp(0.5, 2.0);
                    // A white flash, an orange fireball with a red edge that
                    // outlasts it, sparks and a few cold streaks off the
                    // shock, embers drifting down, then the smoke.
                    self.fx.burst("burst_flash", at, Vec3::Y, 0, 1.0);
                    self.fx.burst_in("burst_fire", at, radius * 0.08, Vec3::Y, 0, k);
                    self.fx.burst_in("burst_fire_deep", at, radius * 0.1, Vec3::Y, 0, k);
                    self.fx.burst("burst_sparks", at, Vec3::Y, 0, k);
                    self.fx.burst("burst_shock", at, Vec3::Y, 0, k);
                    self.fx.burst("burst_embers", at, Vec3::Y, 0, k);
                    self.fx.burst("burst_smoke", at, Vec3::Y, 0, k);
                    let m = crate::fx::hooks::ground_under(&self.store, at + Vec3::Y * 0.3);
                    if m != AIR {
                        self.fx.burst("burst_debris", at + Vec3::Y * 0.1, Vec3::Y, m, k);
                    }
                }
            }
        }
    }

    /// An animal big enough to be in the way is in the way.
    ///
    /// Only the player gives ground. An animal that was shoved by walking
    /// into it would be an animal whose own movement code had been
    /// overruled from outside, and the first thing that buys you is a bison
    /// pushed through a cliff. Walking into one should feel like walking
    /// into a bison, and that is the player stopping.
    ///
    /// Small things are walked through on purpose: being nudged off a ledge
    /// by a sparrow is worse than passing through it, and the threshold is
    /// on the drawn height rather than on the species, so nothing has to be
    /// listed anywhere.
    fn shoulder_past_animals(&mut self) {
        use crate::creatures::age::Age;
        use crate::creatures::walk::Critter;
        use crate::creatures::{Creature, Look};
        const BLOCKS_AT_M: f32 = 0.6;
        let (hw, h) = (self.player.body.half_w, self.player.body.height);
        let mut by = Vec3::ZERO;
        for (e, look, me, crit, age) in self.ents.world.query::<(hecs::Entity, &Look, Option<&Creature>, Option<&Critter>, Option<&Age>)>().iter() {
            if (me.is_none() && crit.is_none()) || age.is_some_and(|a| a.dead()) {
                continue;
            }
            let Some(m) = self.models.get(look.model) else { continue };
            if m.height < BLOCKS_AT_M {
                continue;
            }
            // Cheap reject before the shape test: most animals in the world
            // are nowhere near you. The reach covers a sauropod's length.
            let far = m.reach[0].abs().max(m.reach[1]) + m.half_w + 1.0;
            if (look.pos.x - self.player.pos.x).abs() > far || (look.pos.z - self.player.pos.z).abs() > far {
                continue;
            }
            // Against its whole body, not the square box at its feet: a
            // triceratops is nine metres of horns and rump (creatures/crowd.rs).
            let (morph, body) = (self.ents.world.get::<&crate::variety::Morph>(e).ok().map(|m| m.scale), self.ents.world.get::<&crate::physics::Body>(e).ok().map(|b| (b.half_w, b.height)));
            let (bhw, bh) = body.unwrap_or((m.half_w, m.height));
            let them = crate::creatures::crowd::Shape::of(look.pos, look.yaw, bhw, bh, m.reach, morph.unwrap_or(1.0));
            let me = crate::creatures::crowd::Shape::of(self.player.pos, 0.0, hw, h, [-hw, hw], 1.0);
            if let Some(p) = me.push_from(&them) {
                by += Vec3::new(p.x, 0.0, p.y);
            }
        }
        if by != Vec3::ZERO {
            crate::physics::nudge(&self.store, &mut self.player.pos, by, hw, h);
        }
    }

    /// A car meeting an animal: the animal is pushed out of the way unharmed
    /// and sent off at a run, and the car loses a little speed for it, more
    /// for a big one. Most are gone before it arrives (the car is a loud,
    /// fast threat to creatures/startle.rs); this is for the rest.
    fn shove_animals(&mut self) {
        use crate::creatures::age::Age;
        use crate::creatures::walk::{Critter, Mood};
        use crate::creatures::Look;
        use crate::physics::Body;
        let Some(car) = self.ride.car() else { return };
        let (pos, rot) = (car.pos(), car.b.rot);
        let inv = rot.inverse();
        let s = &car.spec;
        // The car's footprint in its own frame, from the ground up to the cage.
        let half = Vec3::new(s.track * 0.5 + s.tyre_width * 0.5, 0.0, s.wheelbase * 0.5 + s.wheel_radius + 0.15);
        let floor = -(s.wheel_radius + s.com_height);
        let vel = car.b.vel;
        let mut lost = 0.0f32;
        for (look, body, crit, age) in self.ents.world.query_mut::<(&mut Look, &mut Body, Option<&mut Critter>, Option<&Age>)>() {
            if age.is_some_and(|a| a.dead()) {
                continue;
            }
            if (look.pos.x - pos.x).abs() > 4.0 || (look.pos.z - pos.z).abs() > 4.0 {
                continue;
            }
            let rel = inv * (look.pos - pos);
            if rel.y + body.height < floor || rel.y > floor + 1.8 {
                continue;
            }
            // Overlap across and along, counting the animal's own width.
            let ox = half.x + body.half_w - rel.x.abs();
            let oz = half.z + body.half_w - rel.z.abs();
            if ox <= 0.0 || oz <= 0.0 {
                continue;
            }
            // Out to the nearer side, mostly across: an animal clipped by the
            // nose is swept aside rather than pushed along in front.
            let side = if rel.x >= 0.0 { 1.0 } else { -1.0 };
            let by = if ox < oz * 1.5 {
                rot * Vec3::new(side * ox, 0.0, 0.0)
            } else {
                rot * Vec3::new(0.0, 0.0, rel.z.signum() * oz)
            };
            let by = Vec3::new(by.x, 0.0, by.z);
            crate::physics::nudge(&self.store, &mut look.pos, by, body.half_w, body.height);
            let away = by.normalize_or_zero();
            let speed = Vec3::new(vel.x, 0.0, vel.z).length();
            body.vel = away * (2.0 + speed * 0.3).min(7.0) + Vec3::Y * (speed * 0.05).min(1.5);
            if let Some(c) = crit {
                c.mood = Mood::Flee;
                c.heading = (-away.x).atan2(-away.z);
                c.timer = c.timer.max(3.0);
            }
            // Bulk of the animal against the car's mass.
            let bulk = (body.half_w * 2.0).powi(2) * body.height * 600.0;
            lost = lost.max((bulk / s.mass).min(0.35));
        }
        if lost > 0.0 {
            self.ride.slow_car(1.0 - lost);
        }
    }

    /// A shrub the car is about to run over is flattened: its stems and
    /// leaves are taken out of the store just ahead of the car, so it neither
    /// stops the car nor throws it. The change is not saved
    /// (`Streamer::edit_passing`): the shrub is back once its ground streams
    /// in again. A shrub is a placed plant under `TALL` metres; anything
    /// taller (a juniper, an ocotillo) is met by the chassis as before.
    fn crush_shrubs(&mut self) {
        const TALL: f32 = 1.8;
        let Some(car) = self.ride.car() else { return };
        let Some(world) = self.streamer.generator().world.clone() else { return };
        let (pos, rot) = (car.pos(), car.b.rot);
        let inv = rot.inverse();
        let s = &car.spec;
        let half = Vec3::new(s.track * 0.5 + s.tyre_width * 0.5, 0.0, s.wheelbase * 0.5 + s.wheel_radius + 0.15);
        let floor = -(s.wheel_radius + s.com_height);
        // A tenth of a second ahead of the nose (or the tail, reversing), so
        // the shrub is gone before the chassis or a tyre reaches it.
        let v_fwd = car.v_fwd();
        let ahead = (v_fwd.abs() * 0.1).min(3.0);
        let reach = half.z + ahead + 2.0;
        let mut changes = Vec::new();
        let mut bulk = 0.0f32;
        for p in self.ents.plants.near(pos.x, pos.z, reach, None) {
            if p.height > TALL {
                continue;
            }
            let rel = inv * (p.pos - pos);
            if rel.y > floor + 1.0 || rel.y + p.height < floor - 0.5 {
                continue;
            }
            // Front is -Z in the car's frame.
            let (front, back) = if v_fwd >= 0.0 { (half.z + ahead, half.z) } else { (half.z, half.z + ahead) };
            let r = p.crown * 0.7;
            if rel.x.abs() > half.x + r || rel.z < -front - r || rel.z > back + r {
                continue;
            }
            let Some(def) = world.lib.plants.get(p.plant as usize) else { continue };
            let bark = crate::world::matlib::id_of(&def.bark);
            let ours = |m: u8| {
                Some(crate::world::matlib::base(m)) == bark || crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE | crate::world::matlib::flag::GROWS)
            };
            let c = p.crown + 0.25;
            let lo = ((p.pos - Vec3::new(c, 0.25, c)) * 4.0).floor();
            let hi = ((p.pos + Vec3::new(c, p.height + 0.25, c)) * 4.0).floor();
            let before = changes.len();
            for y in lo.y as i32..=hi.y as i32 {
                for z in lo.z as i32..=hi.z as i32 {
                    for x in lo.x as i32..=hi.x as i32 {
                        if self.store.get(x, y, z).is_some_and(|m| m != crate::world::material::AIR && ours(m)) {
                            changes.push(([x, y, z], crate::world::material::AIR));
                        }
                    }
                }
            }
            if changes.len() > before {
                bulk += p.height * p.crown * p.crown;
            }
        }
        if changes.is_empty() {
            return;
        }
        self.streamer.edit_passing(&mut self.store, &changes);
        // Pushing through brush costs a little speed, a big shrub more.
        self.ride.slow_car(1.0 - (bulk * 0.04).min(0.08));
    }

    fn dig(&mut self, hit: [i32; 3]) {
        let c = Vec3::new(hit[0] as f32 + 0.5, hit[1] as f32 + 0.5, hit[2] as f32 + 0.5);
        let r = 1.8f32;
        let mut changes = Vec::new();
        // Nothing below what the world keeps detail for can be dug
        // (`Generator::dig_floor_vox`).
        let floor = |x: i32, z: i32| self.streamer.generator().dig_floor_vox(x, z);
        let mut refused = false;
        let ri = r.ceil() as i32;
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let p = [hit[0] + dx, hit[1] + dy, hit[2] + dz];
                    let d = Vec3::new(p[0] as f32 + 0.5, p[1] as f32 + 0.5, p[2] as f32 + 0.5) - c;
                    if d.length() <= r {
                        if (p[1] as f32) < floor(p[0], p[2]) {
                            refused = true;
                        } else {
                            changes.push((p, AIR));
                        }
                    }
                }
            }
        }
        if changes.is_empty() && refused {
            self.note("BEDROCK: TOO DEEP TO DIG");
        }
        if !changes.is_empty() {
            let m = self.store.get(hit[0], hit[1], hit[2]).unwrap_or(AIR);
            crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Dig(crate::sound::watch::ground_of(m)), c * 0.25, 0.9));
        }
        self.streamer.edit(&mut self.store, &changes);
    }

    fn place(&mut self, prev: [i32; 3]) {
        let k = self.brush;
        let base = [prev[0].div_euclid(k) * k, prev[1].div_euclid(k) * k, prev[2].div_euclid(k) * k];
        let m = PALETTE_KEYS[self.selected];
        let p = self.player.pos;
        let (hw, h) = (self.player.body.half_w, self.player.body.height);
        let mut changes = Vec::new();
        for dy in 0..k {
            for dz in 0..k {
                for dx in 0..k {
                    let v = [base[0] + dx, base[1] + dy, base[2] + dz];
                    let (x0, y0, z0) = (v[0] as f32 / 4.0, v[1] as f32 / 4.0, v[2] as f32 / 4.0);
                    let inside = x0 + 0.25 > p.x - hw && x0 < p.x + hw && z0 + 0.25 > p.z - hw && z0 < p.z + hw && y0 + 0.25 > p.y && y0 < p.y + h;
                    if inside || self.store.get(v[0], v[1], v[2]) != Some(AIR) {
                        continue;
                    }
                    changes.push((v, m));
                }
            }
        }
        if let Some((v, _)) = changes.first() {
            let at = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) * 0.25;
            crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Place, at, 0.8));
        }
        self.streamer.edit(&mut self.store, &changes);
    }

    /// Where the eye is this frame, whatever is driving the camera.
    /// Where the eye is this frame: the rider's head while riding, else the
    /// walking eye. `Player::eye` is the walking one alone, because a player
    /// on foot and a bot in a match have no ride to ask.
    pub fn eye(&self, alpha: f32) -> Vec3 {
        if self.ride.mode != crate::ride::Mode::Foot && !self.player.fly {
            return self.ride.head(&self.player, alpha);
        }
        self.player.eye(alpha)
    }

    /// Where the ear is and which way it faces: the camera this frame is
    /// drawn from (sound/bed.rs).
    pub fn listener(&self) -> (Vec3, Vec3) {
        let c = self.camera();
        (c.eye, c.fwd)
    }

    fn camera(&self) -> crate::rig::Camera {
        let mut c = if let Some(c) = self.activity.as_ref().and_then(|a| a.camera(&self.player)) {
            c
        } else if self.player.fly {
            crate::rig::Camera::looking(self.player.eye(self.alpha), self.player.yaw, self.player.pitch)
        } else {
            self.ride.camera(&self.player, &self.store, self.alpha)
        };
        // `VG_ROLL=degrees` banks the view about where it looks, for checking
        // a banked frame (the wingsuit's) in a headless shot.
        static ROLL: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        let roll = *ROLL.get_or_init(|| std::env::var("VG_ROLL").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0));
        if roll != 0.0 {
            c.up = glam::Quat::from_axis_angle(c.fwd.normalize(), roll.to_radians()) * c.up;
        }
        // A heavy footfall nearby (`feel_steps`): the view drops a touch and
        // rocks, and settles within a fifth of a second.
        let (k, t) = self.thump;
        let t = t + self.alpha * STEP;
        let fade = k * (-t * THUMP_FADE).exp();
        if fade > 1e-3 && self.activity.is_none() {
            c.eye.y -= THUMP_M * fade * (t * 45.0).cos();
            let roll = THUMP_ROLL * fade * (t * 38.0).sin();
            c.up = glam::Quat::from_axis_angle(c.fwd.normalize(), roll) * c.up;
        }
        c
    }

    /// The lamps that can reach the camera's neighbourhood, nearest first.
    /// A lamp lights a sphere, so one well out of view still matters: the
    /// test is its own reach, plus a little, against the eye.
    fn prepare_lamps(&mut self) {
        self.lamps.clear();
        let cam = self.camera();
        let (eye, fwd) = (cam.eye, cam.fwd);
        // A burst lights the room it goes off in for a moment.
        if let Some(sh) = self.activity.as_ref().and_then(|a| a.shooter()) {
            for (at, reach, colour, strength) in sh.combat.flashes() {
                if (at - eye).length() < reach + LAMP_SLACK {
                    self.lamps.push(at, reach, colour, strength);
                }
            }
        }
        if let Some(r) = self.activity.as_mut().and_then(|a| a.race_mut()) {
            for (at, reach, colour, strength) in r.flashes() {
                if (at - eye).length() < reach + LAMP_SLACK {
                    self.lamps.push(at, reach, colour, strength);
                }
            }
        }
        // How far into the evening it is, for lamps lit only after dark,
        // and for the cars' lights, which heavy weather turns on by day.
        let e = crate::world::clock::sun_elevation(self.hour);
        let dark = ((2.0 - e) / 6.0).clamp(0.0, 1.0);
        let lit = dark.max(crate::vehicle::lights::gloom(&self.weather.sky()));
        crate::vehicle::lights::set_lit(lit);
        let Some(w) = self.generator().world.clone() else {
            self.lamps.sort_to(eye);
            return;
        };
        for l in &w.lamps {
            let v = l.pos - eye;
            let d = v.length();
            // Anything whose sphere reaches the eye lights the room you are
            // standing in, whichever way you are facing.
            if d >= l.reach + LAMP_SLACK {
                continue;
            }
            if d > l.reach + 2.0 {
                // Further off, it only matters if its sphere is somewhere
                // the view can see. A cone wide enough to hold the frustum
                // whatever the field of view, so this drops what is behind
                // you and well off to the side and nothing else.
                let along = v.dot(fwd);
                if along < -l.reach {
                    continue;
                }
                let perp = (v - fwd * along).length();
                if perp > along.max(0.0) * LAMP_CONE + l.reach {
                    continue;
                }
            }
            let strength = if l.after_dark { l.strength * dark } else { l.strength };
            if strength <= 0.01 {
                continue;
            }
            self.lamps.push(l.pos, l.reach, l.colour, strength);
        }
        // The karts' lights after dark (and in heavy weather): headlights,
        // and tail lights brighter when braking (src/vehicle/lights.rs).
        {
            let lamps = &mut self.lamps;
            let mut light = |car: &crate::vehicle::Vehicle| {
                if !crate::vehicle::lights::any(car, lit) || car.pos().distance(eye) > crate::vehicle::lights::REACH + LAMP_SLACK + 40.0 {
                    return;
                }
                for l in crate::vehicle::lights::of(car, lit) {
                    match l.spot {
                        Some((dir, half)) => lamps.push_spot(l.pos, dir, half, l.reach, l.colour, l.strength),
                        None => lamps.push(l.pos, l.reach, l.colour, l.strength),
                    }
                }
            };
            if let Some(c) = self.ride.car().filter(|_| !self.ride.hide) {
                light(c);
            }
            if let Some(a) = &self.activity {
                a.each_race_kart(&mut light);
            }
        }
        self.lamps.sort_to(eye);
    }

    /// The boards and lettering of the signs near the camera, and which named
    /// area the player is standing in.
    fn prepare_places(&mut self) {
        self.plates.clear();
        let Some(w) = self.generator().world.clone() else {
            return;
        };
        let eye = self.camera().eye;
        for sg in &w.signs {
            if sg.at.distance(eye) <= crate::world::places::BOARD_RANGE {
                sg.draw(&w.lib.signs[sg.style], eye, &mut self.plates);
            }
        }
        if !w.cloth.flags.is_empty() || !w.cloth.kites.is_empty() {
            let day = crate::render::daylight::sun_dir(self.hour).y > 0.05;
            let air = crate::world::cloth::Air::new(self.weather.sky().wind, self.time, day);
            w.cloth.draw(&air, eye, &mut self.plates);
        }
        if !w.areas.is_empty() && self.spawned {
            let p = glam::Vec2::new(self.player.pos.x, self.player.pos.z);
            let now = crate::world::places::naming(&w.areas, p, self.area);
            if now != self.area {
                self.area_was = self.area;
                self.area = now;
                self.area_since = self.time;
            }
        }
    }

    /// Interpolate creature transforms for this frame; call before
    /// `frame_params`, with the same aspect ratio.
    pub fn prepare_frame(&mut self, aspect: f32) {
        self.overlay.clear();
        self.fx.alpha = self.alpha;
        self.prepare_places();
        self.prepare_lamps();
        if let Some(a) = &self.activity {
            a.overlay(&self.player, &mut self.overlay);
        }
        self.eco_overlay();
        // The moving parts keep stepping in a paused world; nothing else does.
        let alpha = self.still_alpha.unwrap_or(self.alpha);
        let (store, sea) = (&self.store, self.sea);
        let ground = |x: f32, z: f32| {
            use crate::world::gen::VPM;
            store.ground_vox((x * VPM).floor() as i32, (z * VPM).floor() as i32).map(|g| (g / VPM).max(sea))
        };
        self.ents.instances(alpha, &self.models, &ground, &mut self.inst);
        let pinned_from = self.inst.len();
        if let (Some(a), true) = (&self.activity, self.weapon_models != usize::MAX) {
            a.instances(&self.player, alpha, self.weapon_models, &self.models, &mut self.inst);
        }
        // A match parked behind screenshot mode still has its bots standing
        // where they were; only the gun in the player's hands goes.
        if let (Some(a), true) = (&self.parked, self.in_screenshot() && self.weapon_models != usize::MAX) {
            a.parked_instances(&self.player, alpha, self.weapon_models, &self.models, &mut self.inst);
        }
        self.ride.show_body = self.in_screenshot();
        if let Some(i) = self.ride.instance(&self.player, alpha, self.ride_models, &self.models) {
            self.inst.push(i);
        }
        if let Some(i) = self.ride.stand_in(&self.player, alpha, self.ride_models, &self.models) {
            self.inst.push(i);
        }
        self.ride.rope(&self.player, alpha, self.ride_models, &self.models, &mut self.inst);
        self.ride.gear(&self.player, &self.store, alpha, self.ride_models, &self.models, &mut self.inst);
        self.ride.moored_instance(self.time, self.ride_models, &self.models, &mut self.inst);
        if self.vehicle_models != usize::MAX {
            self.ride.vehicle_instances(alpha, self.vehicle_models, &self.models, &mut self.inst);
        }
        self.inst.extend(self.extra_inst.iter().cloned());
        let pinned_to = self.inst.len();
        self.parts.platforms.instances(self.alpha, &self.models, &mut self.inst);
        self.cull_instances(aspect, pinned_from..pinned_to);
        self.inst.sort_by_key(|(m, _)| *m);
    }

    /// Drop the instances this frame has no use for, and keep the rest inside
    /// the renderer's buffer.
    ///
    /// Every creature, item and platform in the world used to be uploaded and
    /// drawn, whichever way the camera faced, and anything past the 8192nd was
    /// silently left out -- so in a crowded world the ones that went missing
    /// were whatever happened to sort last, not the ones too far off to see.
    /// Now a body is kept when it reaches into the view, or when it is near
    /// enough to cast a shadow into it (`SHADOW_CASTERS_M`, which covers both
    /// cascades that draw creatures); if more than the buffer holds survive,
    /// the nearest win and `instances_dropped` says how many did not.
    ///
    /// `pinned` is the activity's and the ride's own instances (the shooter's
    /// bots and guns, the player's bike or glider). They never lose to
    /// distance: a bot vanishing in a crowded arena is worse than a far shrub.
    fn cull_instances(&mut self, aspect: f32, pinned: std::ops::Range<usize>) {
        let cam = self.camera();
        let fov = (self.fov() * cam.fov_scale).min(2.2);
        let proj = Mat4::perspective_infinite_reverse_rh(fov, aspect, NEAR);
        let planes = crate::render::frustum_planes(proj * Mat4::look_to_rh(cam.eye, cam.fwd, cam.up));
        let eye = cam.eye;
        let near2 = SHADOW_CASTERS_M * SHADOW_CASTERS_M;
        let models = &self.models;
        // The ball that holds the model as it is drawn: its own size, scaled
        // by whatever the instance shrank or grew it to.
        let reach = |m: usize, i: &Instance| -> f32 {
            let Some(md) = models.get(m) else { return 2.0 };
            let s = if md.voxel > 0.0 { (i.info[0] / md.voxel).abs().max(0.01) } else { 1.0 };
            (md.half_w.max(md.height) * s + 0.5).max(0.5)
        };
        // A retain by hand, so the pinned range can be followed through it.
        let (mut kept, mut from, mut to) = (0, 0, 0);
        for k in 0..self.inst.len() {
            let (m, i) = &self.inst[k];
            let p = Vec3::new(i.pos_yaw[0], i.pos_yaw[1], i.pos_yaw[2]);
            if k == pinned.start {
                from = kept;
            }
            if k == pinned.end {
                to = kept;
            }
            if (p - eye).length_squared() <= near2 || crate::render::sphere_in_frustum(&planes, p, reach(*m, i)) {
                self.inst.swap(kept, k);
                kept += 1;
            }
        }
        if pinned.start >= self.inst.len() {
            from = kept;
        }
        if pinned.end >= self.inst.len() {
            to = kept;
        }
        self.inst.truncate(kept);
        let cap = crate::render::MAX_INSTANCES as usize;
        self.instances_dropped = self.inst.len().saturating_sub(cap);
        if self.instances_dropped > 0 {
            // Pinned ones to the front, then the rest nearest first.
            self.inst[..to].rotate_right(to - from);
            let d = |i: &Instance| (Vec3::new(i.pos_yaw[0], i.pos_yaw[1], i.pos_yaw[2]) - eye).length_squared();
            self.inst[to - from..].sort_by(|a, b| d(&a.1).total_cmp(&d(&b.1)));
            self.inst.truncate(cap);
        }
    }

    pub fn frame_params(&self, aspect: f32) -> FrameParams<'_> {
        let cam = self.camera();
        let (eye, fwd) = (cam.eye, cam.fwd);
        let view = Mat4::look_to_rh(eye, fwd, cam.up);
        let fov = (self.fov() * cam.fov_scale).min(2.2);
        let proj = Mat4::perspective_infinite_reverse_rh(fov, aspect, NEAR);
        // No dig outline on the EDIT page: the camera there is the studio's,
        // and the box would sit on the floor beside the subject in every shot.
        // Nor at the wheel, where it floats on the road ahead.
        let highlight = self.target.filter(|_| !self.editing && self.ride.car().is_none()).map(|(h, _)| {
            let a = Vec3::new(h[0] as f32, h[1] as f32, h[2] as f32) / 4.0;
            (a, a + Vec3::splat(0.25))
        });
        FrameParams {
            view,
            proj,
            cam_pos: eye,
            cam_fwd: fwd,
            time: self.time,
            underwater: eye.y < self.water_at(eye).0,
            sea: self.sea,
            water_level: {
                let w = self.water_at(eye).0;
                if eye.y < w {
                    w
                } else {
                    self.sea
                }
            },
            highlight,
            overlay: &self.overlay,
            polish: &self.polish,
            entities: &self.inst,
            plates: &self.plates,
            fx: &self.fx,
            marks: &self.marks,
            lamps: &self.lamps,
            shadows: self.settings.shadows,
            decimated: self.settings.smooth_creatures,
            style: self.settings.style,
            slopes: self.streamer.smooth(),
            speed_fx: cam.speed_fx,
            streaks: cam.streaks,
            // The EDIT page's own light: a fixed sun in a clear sky.
            hour: if self.studio_lit() { STUDIO_HOUR } else { self.hour },
            sky: if self.studio_lit() { crate::world::weather::Sky::default() } else { self.weather.sky() },
            season: {
                let mut p = self.year.phase();
                let dry = self.generator().world.as_ref().map_or(0.0, |w| w.def.straw);
                p.straw = p.straw.max(dry).min(1.0);
                p
            },
            damp: if self.studio_lit() { [0.0; 4] } else { self.damp.uniform() },
            ground_fog: if self.studio_lit() { 0.0 } else { self.damp.mist() },
            bolt: if self.studio_lit() { [0.0; 4] } else { self.lightning.uniform() },
            mirage: if self.studio_lit() { 0.0 } else { self.mirage() },
            mirage_floor: self.generator().ground_under(eye.x, eye.z),
            snow_cover: {
                let lying = self.year.turn().snow as f32 * 0.5;
                let floor = self.generator().world.as_ref().map_or(0.0, |w| w.def.snow_cover);
                lying.max(floor).min(1.0)
            },
            space: self.generator().world.as_ref().is_some_and(|w| w.def.space),
            plain_grade: self.studio_lit(),
            studio: self
                .generator()
                .studio_floor
                .read()
                .ok()
                .and_then(|f| *f)
                .map(|f| (f.x, f.z, f.half_m + crate::world::gen::STUDIO_BANK_M)),
            lens: (self.camera.active && self.camera.raised && self.camera.book.is_none() && self.activity.is_none()).then(|| self.camera.lens.fx(self.camera.orient, aspect, true)),
        }
    }

    /// The animal calls sounding now, loudest first, for the stats HUD: who,
    /// what, how far, which way and about how loud.
    fn heard_debug(&self) -> Vec<String> {
        const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        let heard = crate::sound::audible();
        if heard.is_empty() {
            return Vec::new();
        }
        let mut out = vec![format!("HEARD {}", heard.len())];
        for a in heard.iter().take(8) {
            let who = if (a.who as usize) < self.ents.species.len() { crate::creatures::display_name(&self.ents.species, a.who as usize) } else { "?" };
            let point = POINTS[((a.bearing / std::f32::consts::FRAC_PI_4).round() as usize) % 8];
            out.push(format!("{}  {}  {:.0} M  {}  {:.0} DB", who.replace('_', " "), a.cry.name(), a.dist, point, a.db).to_uppercase());
        }
        out
    }

    /// The biome the player stands in, with the ground as the fields see it:
    /// for the stats HUD, so a look at the view can be checked against what
    /// the chronicle decided here. None on a world without fields.
    fn biome_debug(&self) -> Option<String> {
        let w = self.generator().world.as_ref()?;
        let f = &w.f;
        let p = self.player.pos;
        let i = f.idx(p.x, p.z);
        let name = w.lib.biomes.get(f.biome[i] as usize).map_or("?", |b| b.name.as_str()).to_uppercase().replace('_', " ");
        let wet = match f.water.get(i).copied().unwrap_or(0) {
            1 => "  RIVER",
            2 => "  OLD BED",
            3 => "  PATH",
            _ if f.h[i] < f.sea => "  SEA",
            _ => "",
        };
        Some(format!("{name}  H {:.1}  MOIST {:.2}{wet}", f.h[i], f.moist.get(i).copied().unwrap_or(0.0)))
    }

    /// What the crosshair is on: the nearest creature or find if one is in
    /// front of the ground, else the material under the pointer with its id,
    /// how far off it is and which voxel it is, and the library structure the
    /// voxel belongs to if it is part of one. For the stats HUD.
    fn aim_debug(&self) -> String {
        const RANGE: f32 = 120.0;
        let eye = self.eye(self.alpha);
        let fwd = self.player.forward();
        let block = self.store.raycast(eye, fwd, RANGE);
        let hit_at = |v: [i32; 3]| (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * 0.25;
        let block_t = block.map_or(RANGE, |(v, _)| (hit_at(v) - eye).length());
        if let Some((t, name)) = self.ents.identify(&self.models, eye, fwd, RANGE) {
            if t < block_t + 0.5 {
                return format!("{name}  {t:.1}M");
            }
        }
        let Some((v, _)) = block else {
            return "SKY".to_string();
        };
        let m = self.store.get(v[0], v[1], v[2]).unwrap_or(AIR);
        let base = crate::world::matlib::base(m);
        let grain = if base != m { format!("/{base}") } else { String::new() };
        let within = self.structure_at(v).map_or(String::new(), |n| format!("  IN {n}"));
        format!("{} #{m}{grain}  {block_t:.1}M  {} {} {}{within}", material::name(m), v[0], v[1], v[2])
    }

    /// The name of the placed structure whose shape is solid at world voxel
    /// `v`, counting the foundation it fills down under its bottom row. A
    /// voxel inside a structure's box that the shape leaves empty is the
    /// terrain's, so the name only shows on the structure itself.
    fn structure_at(&self, v: [i32; 3]) -> Option<&str> {
        use crate::world::shape::{unrotate, CARVE};
        let w = self.generator().world.as_ref()?;
        let (mx, mz) = (v[0] as f32 * 0.25, v[2] as f32 * 0.25);
        for k in w.placed_near(mx, mz, mx, mz) {
            let k = k as usize;
            // Not standing just now (`place` `when`).
            if w.shown.get(k).is_some_and(|s| !s.load(std::sync::atomic::Ordering::Relaxed)) {
                continue;
            }
            let p = &w.placed[k];
            let s = &w.shapes[p.shape];
            let (x0, z0, x1, z1) = p.footprint(&w.shapes);
            if v[0] < x0 || v[0] >= x1 || v[2] < z0 || v[2] >= z1 {
                continue;
            }
            let def = &w.lib.structures[p.def];
            let (lx, lz) = unrotate(v[0] - x0, v[2] - z0, s.size, p.rot);
            let ly = v[1] - p.origin[1];
            let ly = if ly < 0 && ly >= -def.foundation { 0 } else { ly };
            let (m, _) = s.get(lx, ly, lz);
            if m != AIR && m != CARVE {
                return Some(&def.name);
            }
        }
        None
    }

    /// A small north-up map of the ground around the player, drawn from the
    /// world's fields in their map colours with hill shading: enough to tell
    /// where you are and which way the land runs. `px` is the side in pixels.
    /// Rows of equal colour merge into one quad, so a map costs a few hundred.
    fn build_minimap(&self, hud: &mut Hud, x: f32, y: f32, px: f32, s: f32) {
        /// Samples across the map.
        const N: usize = 56;
        /// How much ground it covers, metres.
        const SPAN: f32 = 320.0;
        let dim = [0.85, 0.9, 0.95, 0.85];
        hud.rect(x - 2.0, y - 2.0, px + 4.0, px + 4.0, [0.0, 0.0, 0.0, 0.5]);
        let Some(w) = self.generator().world.as_ref() else {
            hud.label(x + 4.0, y + px * 0.5 - 5.0 * s, s, dim, "NO MAP");
            return;
        };
        let f = &w.f;
        let p = self.player.pos;
        let step = SPAN / N as f32;
        let cell = px / N as f32;
        let colour = |wx: f32, wz: f32| -> [f32; 4] {
            if wx < 0.0 || wz < 0.0 || wx >= f.size || wz >= f.size {
                return [0.1, 0.1, 0.12, 0.8];
            }
            let i = f.idx(wx, wz);
            let h = f.h[i];
            let mut c = w.lib.biomes.get(f.biome[i] as usize).and_then(|b| b.map_color).unwrap_or([128.0, 128.0, 128.0]);
            if h < f.sea {
                let d = ((f.sea - h) / 12.0).min(1.0);
                c = [40.0 + 50.0 * (1.0 - d), 90.0 + 60.0 * (1.0 - d), 140.0 + 40.0 * (1.0 - d)];
            } else {
                // Hill shading from the drop to the next sample east and south.
                let sh = (1.0 + ((h - f.height(wx + step, wz)) + (h - f.height(wx, wz + step))) * 0.5 / step).clamp(0.55, 1.35);
                c = [c[0] * sh, c[1] * sh, c[2] * sh];
            }
            match f.water.get(i).copied().unwrap_or(0) {
                1 => c = [70.0, 130.0, 190.0],
                2 => c = [170.0, 150.0, 120.0],
                3 => c = [150.0, 135.0, 110.0],
                _ => {}
            }
            // Quantise so flat ground merges into runs instead of one quad a cell.
            let q = |v: f32| (v / 255.0 * 12.0).round() / 12.0;
            [q(c[0]), q(c[1]), q(c[2]), 0.9]
        };
        for row in 0..N {
            let wz = p.z + (row as f32 - N as f32 * 0.5 + 0.5) * step;
            let mut run_c = colour(p.x + (0.5 - N as f32 * 0.5) * step, wz);
            let mut run_x = 0usize;
            for col in 1..=N {
                let c = if col == N { [-1.0; 4] } else { colour(p.x + (col as f32 - N as f32 * 0.5 + 0.5) * step, wz) };
                if c != run_c {
                    let x0 = (x + run_x as f32 * cell).floor();
                    let x1 = (x + col as f32 * cell).ceil();
                    hud.rect(x0, (y + row as f32 * cell).floor(), x1 - x0, cell.ceil(), run_c);
                    run_c = c;
                    run_x = col;
                }
            }
        }
        // The player at the middle, with a stalk the way they face.
        let (mx, my) = ((x + px * 0.5).floor(), (y + px * 0.5).floor());
        let fv = self.player.forward();
        let (dx, dz) = {
            let l = (fv.x * fv.x + fv.z * fv.z).sqrt().max(0.001);
            (fv.x / l, fv.z / l)
        };
        for k in 1..=5 {
            let d = k as f32 * 1.6 * s;
            hud.rect(mx + dx * d - 1.0, my + dz * d - 1.0, 2.0, 2.0, [0.1, 0.05, 0.05, 0.9]);
        }
        let r = (1.5 * s).ceil();
        hud.rect(mx - r - 1.0, my - r - 1.0, r * 2.0 + 2.0, r * 2.0 + 2.0, [0.0, 0.0, 0.0, 0.9]);
        hud.rect(mx - r, my - r, r * 2.0, r * 2.0, [1.0, 0.3, 0.25, 1.0]);
        // North at the top, and how wide the map is, under it.
        hud.label(x + 3.0, y + 2.0, s * 0.8, dim, "N");
        hud.label(x, y + px + 4.0, s * 0.8, dim, &format!("{:.0}M ACROSS", SPAN));
    }

    pub fn frame_summary(&self) -> (f32, f32, f32) {
        if self.frame_ms.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let avg = self.frame_ms.iter().sum::<f32>() / self.frame_ms.len() as f32;
        let mut v: Vec<f32> = self.frame_ms.iter().copied().collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p99 = v[((v.len() as f32 * 0.99) as usize).min(v.len() - 1)];
        (avg, p99, *v.last().unwrap())
    }

    /// The frame-stats panel, top left: it is drawn over whatever else the
    /// HUD is doing, an activity's own HUD included.
    fn build_stats(&self, hud: &mut Hud, s: f32, dim: [f32; 4], w: f32, h: f32) {
        if !self.settings.show_stats {
            return;
        }
        let line = 10.0 * s;
        let (avg, p99, max) = self.frame_summary();
        let st = &self.streamer.stats;
        let r = &self.last_render;
        let p = self.player.pos;
        let _ = max;
        let mut lines = vec![
            format!("{:.0} FPS  {:.1} MS  P99 {:.1}", 1000.0 / avg.max(0.001), avg, p99),
            format!("QUADS {:.1}M  SHADOW {:.1}M  QUEUE {}", r.quads as f64 / 1e6, r.shadow_quads as f64 / 1e6, st.queued),
            format!("{:.0} {:.0} {:.0}  {}", p.x, p.y, p.z, if self.player.fly { "FLY" } else { self.ride.mode.name() }),
        ];
        // The worst hitch of the last few seconds and what took the time.
        if let Some((ms, part)) = crate::trace::worst() {
            lines.push(format!("HITCH {:.0} MS  {}", ms, part.to_uppercase()));
        }
        if self.instances_dropped > 0 {
            lines.push(format!("{} BODIES OVER THE INSTANCE CAP", self.instances_dropped));
        }
        if let Some(b) = self.biome_debug() {
            lines.push(b);
        }
        lines.push(format!(
            "DAY {}  {:02}:{:02}  {}  {}",
            self.year.day + 1,
            self.hour.floor(),
            (self.hour.fract() * 60.0) as u32,
            self.year.name().to_uppercase(),
            self.weather.name().to_uppercase()
        ));
        if self.spawned {
            lines.push(self.aim_debug());
        }
        lines.extend(self.heard_debug());
        let wmax = lines.iter().map(|l| Hud::text_width(l, s)).fold(0.0, f32::max);
        hud.rect(8.0, 8.0, wmax + 16.0, line * lines.len() as f32 + 12.0, [0.0, 0.0, 0.0, 0.35]);
        for (i, l) in lines.iter().enumerate() {
            hud.label(16.0, 14.0 + i as f32 * line, s, dim, l);
        }
        // Driving: each wheel's load, slip and compression, low on the left
        // where nothing else is, for tuning the car (library/vehicles). Not
        // in a race, whose own HUD has the screen.
        if self.activity.is_some() {
            return;
        }
        let wheels = self.ride.drive_debug();
        if !wheels.is_empty() {
            let wmax = wheels.iter().map(|l| Hud::text_width(l, s)).fold(0.0, f32::max);
            let top = line * wheels.len() as f32 + 12.0;
            let y0 = h - top - 60.0 * s;
            hud.rect(8.0, y0 - 6.0, wmax + 16.0, top, [0.0, 0.0, 0.0, 0.35]);
            for (i, l) in wheels.iter().enumerate() {
                hud.label(16.0, y0 + i as f32 * line, s, dim, l);
            }
        }
        self.build_tune(hud, w, h, false);
    }

    /// Where the tuning panel's lines go: on the right, a window round the
    /// picked line while playing, every line (smaller) on the pause screen.
    pub fn tune_layout(&self, w: f32, h: f32, paused: bool) -> Option<TuneLayout> {
        if !self.settings.show_stats || self.activity.is_some() {
            return None;
        }
        let lines = self.ride.tune_lines();
        if lines.is_empty() {
            return None;
        }
        let base = if h > 1200.0 { 3.0 } else { 2.0 };
        let n = lines.len();
        let (s, show) = if paused {
            let s = ((h * 0.92) / ((n + 3) as f32 * 10.0)).min(base);
            (s, n)
        } else {
            (base, (((h * 0.6) / (10.0 * base)) as usize).clamp(8, n))
        };
        let line = 10.0 * s;
        let pick = lines.iter().position(|l| l.3).unwrap_or(0);
        let first = pick.saturating_sub(show / 2).min(n - show);
        let name_w = lines.iter().map(|l| Hud::text_width(&l.0, s)).fold(0.0, f32::max) + 12.0 * s;
        let wide = (name_w + 80.0 * s)
            .max(Hud::text_width(TUNE_KEYS, s * 0.8))
            .max(Hud::text_width(TUNE_MOUSE, s * 0.8))
            .max(Hud::text_width("TUNE  BEACH BUGGY  00 CHANGED", s));
        let x = (w - wide - 24.0).floor();
        let rows = show + 3;
        let y0 = ((h - rows as f32 * line) * 0.5).floor();
        Some(TuneLayout {
            x,
            y0,
            line,
            s,
            first,
            show,
            name_w,
            wide,
        })
    }

    /// What on the tuning panel is under (cx, cy): a line, or the save line.
    pub fn tune_hit(&self, w: f32, h: f32, cx: f32, cy: f32) -> Option<TuneHit> {
        let l = self.tune_layout(w, h, true)?;
        if cx < l.x - 8.0 || cx > l.x + l.wide + 8.0 {
            return None;
        }
        let row = ((cy - l.y0 + 2.0) / l.line).floor();
        if row < 1.0 {
            return None;
        }
        let row = row as usize - 1;
        if row < l.show {
            Some(TuneHit::Line(l.first + row))
        } else if row <= l.show + 1 {
            Some(TuneHit::Save)
        } else {
            None
        }
    }

    /// The vehicle tuning panel (vehicle/tune.rs), with the file's value
    /// beside any line that has moved off it.
    pub fn build_tune(&self, hud: &mut Hud, w: f32, h: f32, paused: bool) {
        let Some(l) = self.tune_layout(w, h, paused) else { return };
        let lines = self.ride.tune_lines();
        let s = l.s;
        let dim = [0.85, 0.9, 0.95, 0.85];
        let white = [1.0, 1.0, 1.0, 0.95];
        let warm = [1.0, 0.8, 0.4, 1.0];
        let changed = lines.iter().filter(|l| l.2.is_some()).count();
        let head = format!("TUNE  {}  {} CHANGED", self.ride.car().map_or(String::new(), |c| c.spec.title.clone()), changed);
        let rows = l.show + 3;
        hud.rect(l.x - 8.0, l.y0 - 6.0, l.wide + 16.0, rows as f32 * l.line + 12.0, [0.0, 0.0, 0.0, 0.55]);
        hud.label(l.x, l.y0, s, white, &head);
        for (row, (name, v, f, on)) in lines[l.first..l.first + l.show].iter().enumerate() {
            let y = l.y0 + (row + 1) as f32 * l.line;
            if *on {
                hud.rect(l.x - 4.0, y - 2.0, l.wide + 8.0, l.line, [1.0, 1.0, 1.0, 0.14]);
            }
            let c = if f.is_some() {
                warm
            } else if *on {
                white
            } else {
                dim
            };
            hud.label(l.x, y, s, c, name);
            let vx = l.x + l.name_w;
            let vw = hud.label(vx, y, s, c, v);
            if let Some(f) = f {
                hud.label(vx + vw + 8.0 * s, y, s, [dim[0], dim[1], dim[2], 0.6], &format!("({f})"));
            }
        }
        let keys = if paused { TUNE_MOUSE } else { TUNE_KEYS };
        hud.label(l.x, l.y0 + (l.show + 1) as f32 * l.line + 4.0, s * 0.8, dim, keys);
    }

    pub fn build_hud(&self, hud: &mut Hud, w: f32, h: f32) {
        hud.clear();
        let white = [1.0, 1.0, 1.0, 0.95];
        let dim = [0.85, 0.9, 0.95, 0.85];
        let s = if h > 1200.0 { 3.0 } else { 2.0 };
        let (cx, cy) = ((w * 0.5).floor(), (h * 0.5).floor());
        let line = 10.0 * s;
        if self.show_help {
            self.build_help(hud, w, h, s);
            return;
        }
        if self.camera.book.is_some() {
            self.camera.book_hud(hud, w, h, s);
            return;
        }
        if self.eco.map {
            let under = self.build_eco_map(hud, w, h, s);
            self.build_inspector(hud, w, h, s, Some(under));
            if let Some((m, t)) = &self.message {
                let mw = Hud::text_width(m, s * 1.5);
                hud.label((w * 0.5 - mw * 0.5).floor(), h - 40.0 * s, s * 1.5, [1.0, 1.0, 1.0, t.min(1.0)], m);
            }
            return;
        }
        if self.camera.active && self.camera.raised && self.activity.is_none() {
            self.camera.hud(hud, w, h, s);
            if let Some((m, t)) = &self.message {
                let a = t.min(1.0);
                let mw = Hud::text_width(m, s * 1.5);
                hud.label((w * 0.5 - mw * 0.5).floor(), h * 0.3, s * 1.5, [1.0, 1.0, 1.0, a], m);
            }
            return;
        }
        if let Some(a) = &self.activity {
            a.hud(hud, w, h, s, self.fov());
            let marks = a.marks(&self.player);
            if !marks.is_empty() {
                let cam = self.camera();
                let fov = (self.fov() * cam.fov_scale).min(2.2);
                let vp = Mat4::perspective_infinite_reverse_rh(fov, w / h.max(1.0), NEAR) * Mat4::look_to_rh(cam.eye, cam.fwd, cam.up);
                crate::modes::draw_marks(hud, &marks, vp, w, h, s);
            }
            if let Some((m, t)) = &self.message {
                let a = t.min(1.0);
                let mw = Hud::text_width(m, s * 1.5);
                hud.label((w * 0.5 - mw * 0.5).floor(), h * 0.3, s * 1.5, [1.0, 1.0, 1.0, a], m);
            }
            // An activity draws its own HUD and nothing else of the game's,
            // but the frame stats are not the game's: they are the player's
            // instrument, and an arena is exactly where they are wanted.
            self.build_stats(hud, s, dim, w, h);
            return;
        }
        if self.camera.active {
            // With the camera out, the focus box the viewfinder uses.
            crate::camera::focus_box(hud, cx, cy, s, [1.0, 1.0, 1.0, 0.8]);
        } else {
            // Crosshair.
            hud.rect(cx - 9.0, cy - 1.0, 18.0, 2.0, [1.0, 1.0, 1.0, 0.8]);
            hud.rect(cx - 1.0, cy - 9.0, 2.0, 18.0, [1.0, 1.0, 1.0, 0.8]);
            // Where the rope would catch: four corners round the crosshair,
            // or round the spot the aim assist found when it isn't there.
            if let (Some(a), false) = (self.ride.hook_target, self.player.fly) {
                let fp = self.frame_params(w / h);
                let c = fp.proj * fp.view * a.extend(1.0);
                if c.w > 0.0 {
                    let (sx, sy) = if self.ride.hook_assisted {
                        ((c.x / c.w * 0.5 + 0.5) * w, (0.5 - c.y / c.w * 0.5) * h)
                    } else {
                        (cx + 0.5, cy + 0.5)
                    };
                    let (r, t, l) = (7.0 * s, 1.0 * s, 3.0 * s);
                    let col = [1.0, 0.95, 0.9, 0.85];
                    for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        let (x, y) = (sx + dx * r, sy + dy * r);
                        hud.rect(x - if dx > 0.0 { l } else { 0.0 }, y - t * 0.5, l, t, col);
                        hud.rect(x - t * 0.5, y - if dy > 0.0 { l } else { 0.0 }, t, l, col);
                    }
                }
            }
        }
        if self.spawned {
            // The compass is always in hand.
            let r = (22.0 * s).floor();
            self.build_compass(hud, cx, 16.0 + r, r);
        }

        self.build_stats(hud, s, dim, w, h);
        self.build_inspector(hud, w, h, s, None);
        self.build_eco_keys(hud, w, s);
        if self.settings.minimap {
            let panel = if self.settings.show_stats { line * 5.0 + 20.0 } else { 0.0 };
            self.build_minimap(hud, 16.0, 16.0 + panel, 60.0 * s, s);
        }

        if let Some(r) = self.ride.hud(&self.player) {
            let line_h = line;
            let rw = Hud::text_width(&r, s * 1.5);
            let y = h - line * 1.5 - 30.0;
            hud.label(cx - rw * 0.5, y, s * 1.5, [1.0, 0.96, 0.86, 0.95], &r);
            let (bw, bh) = (60.0 * s, 2.0 * s);
            let mut by = y + 15.0 * s;
            if let Some((left, firing)) = self.ride.boost_meter() {
                hud.rect(cx - bw * 0.5 - 1.0, by - 1.0, bw + 2.0, bh + 2.0, [0.0, 0.0, 0.0, 0.45]);
                let c = if firing {
                    [1.0, 0.8, 0.3, 1.0]
                } else if left > 0.15 {
                    [1.0, 0.96, 0.86, 0.9]
                } else {
                    [0.9, 0.4, 0.3, 0.9]
                };
                hud.rect(cx - bw * 0.5, by, bw * left, bh, c);
                by += 7.0 * s;
            }
            // Gliding: the run's score over the line, and the multiplier as a
            // second bar under the boost.
            let soar = &self.ride.soar;
            if let Some((line, charge)) = soar.hud(self.ride.mode) {
                let lw = Hud::text_width(&line, s * 1.5);
                hud.label(cx - lw * 0.5, y - line_h * 1.5, s * 1.5, [1.0, 0.92, 0.72, 0.95], &line);
                hud.rect(cx - bw * 0.5 - 1.0, by - 1.0, bw + 2.0, bh + 2.0, [0.0, 0.0, 0.0, 0.45]);
                let hot = charge.clamp(0.0, 1.0);
                let c = [1.0, 0.86 - 0.5 * hot, 0.5 - 0.35 * hot, 0.9];
                hud.rect(cx - bw * 0.5, by, bw * hot, bh, c);
                // The call-out for whatever was just pulled off: above the
                // score, in the same column, rather than across the view.
                if let Some((name, points, left)) = soar.call {
                    let text = format!("{name}  +{}", soar::thousands(points as f64));
                    let a = (left / 0.7).min(1.0) * 0.85;
                    let cw = Hud::text_width(&text, s * 1.6);
                    hud.label(cx - cw * 0.5, y - line_h * 3.0, s * 1.6, [1.0, 0.88, 0.55, a], &text);
                }
            }
        }
        if self.player.diving() && !self.camera.active {
            self.build_breath(hud, cx, h, s, line);
        }
        if self.camera.active {
            self.camera.hud(hud, w, h, s);
        }
        let gems = format!("{} {}", if self.ents.has_world() { "FINDS" } else { "GEMS" }, self.ents.gems_collected.len());
        let gw = Hud::text_width(&gems, s * 1.5);
        hud.label(w - gw - 20.0, 16.0, s * 1.5, [1.0, 0.95, 0.9, 0.8], &gems);

        // With the camera in hand its two lines of guidance sit in the
        // bottom left corner, so what you are looking at and where you are
        // move up above them rather than printing over them.
        let foot = if self.camera.active && !self.camera.raised { line * 2.0 + 20.0 } else { 0.0 };
        // The area name sits above whatever is printed under it.
        let mut area_y = h - foot - line * 3.0 - 16.0;
        if let Some(name) = &self.looking_at {
            let y = h - line * 1.5 - 12.0 - foot;
            hud.label(16.0, y, s * 1.5, [1.0, 0.96, 0.86, 0.95], name);
            // Above the name: its sex, age and what it is doing, so a
            // display (creatures/display) can be seen to be running.
            if let Some((doing, _)) = self.looking_doing.as_ref().filter(|d| !d.0.is_empty()) {
                let dy = y - line * 1.1;
                hud.label(16.0, dy, s * 1.1, [0.85, 0.9, 1.0, 0.85], doing);
                area_y = area_y.min(dy - line * 1.5 - 4.0);
            }
        }
        self.build_area_name(hud, area_y, s);
        // In a zoo (creatures/zoo.rs): whose pen this is and which row.
        if let Some(pen) = self.ents.zoo.as_ref().and_then(|z| z.pen_at(self.player.pos)) {
            let name = self.ents.species.get(pen.species).map_or(String::new(), |sp| sp.name.replace('_', " ").to_uppercase());
            let row = pen.row_at(self.player.pos.z).unwrap_or("").to_uppercase();
            let t = format!("PEN: {name}   ROW: {row}   PGUP/PGDN: OTHER PENS");
            let tw = Hud::text_width(&t, s * 1.2);
            hud.label(cx - tw * 0.5, 16.0 + line * 1.6, s * 1.2, [1.0, 0.96, 0.8, 0.9], &t);
        }

        if let Some((m, t)) = &self.message {
            let a = t.min(1.0);
            let mw = Hud::text_width(m, s * 1.5);
            hud.label(cx - mw * 0.5, h * 0.3, s * 1.5, [1.0, 1.0, 1.0, a], m);
        }
        if !self.spawned {
            let m = "LOADING";
            let mw = Hud::text_width(m, s * 2.0);
            hud.label(cx - mw * 0.5, cy + 40.0, s * 2.0, white, m);
        }
    }

    /// Where you are, low in the corner: it fades up as you walk into a named
    /// area, holds for a few seconds, then settles to a dim line that stays
    /// while you are in it, and fades away as you leave.
    fn build_area_name(&self, hud: &mut Hud, y: f32, s: f32) {
        let Some(w) = self.generator().world.as_ref() else {
            return;
        };
        let t = (self.time - self.area_since).max(0.0);
        let (name, a) = match (self.area, self.area_was) {
            (Some(k), _) => {
                let up = (t / 1.2).clamp(0.0, 1.0);
                // Bright while it is news, then dim while you are in it.
                let settle = 1.0 - 0.55 * ((t - 4.5) / 2.0).clamp(0.0, 1.0);
                (&w.areas[k].name, up * up * (3.0 - 2.0 * up) * settle)
            }
            (None, Some(k)) => (&w.areas[k].name, (1.0 - t / 1.0).clamp(0.0, 1.0) * 0.45),
            (None, None) => return,
        };
        if a <= 0.01 {
            return;
        }
        hud.label(16.0, y, s * 1.5, [0.98, 0.95, 0.88, a], name);
    }

    /// The controls, over a dimmed and paused game.
    fn build_help(&self, hud: &mut Hud, w: f32, h: f32, s: f32) {
        draw_help(hud, self.ride.allowed, w, h, s, "Esc or a click to go back");
    }

    /// A pocket compass seen face on. The mark at the top is where you face; the
    /// card turns under it, and the red needle points north.
    fn build_compass(&self, hud: &mut Hud, cx: f32, cy: f32, r: f32) {
        let f = self.player.forward();
        let b = crate::util::bearing(glam::Vec2::new(f.x, f.z));
        let ink = [1.0, 0.96, 0.86, 0.95];
        let red = [0.92, 0.28, 0.2, 1.0];
        // Screen point at clockwise angle `a` from straight up, `d` from the centre.
        let at = |a: f32, d: f32| (cx + d * a.sin(), cy - d * a.cos());
        let dot = |hud: &mut Hud, (x, y): (f32, f32), size: f32, c: [f32; 4]| {
            hud.rect(x - size * 0.5, y - size * 0.5, size, size, c);
        };
        // The face, as a disc of 2 px strips, and a rim.
        let mut y = -r;
        while y < r {
            let mid = y + 1.0;
            let half = (r * r - mid * mid).max(0.0).sqrt().round();
            hud.rect(cx - half, cy + y, half * 2.0, 2.0, [0.04, 0.04, 0.03, 0.5]);
            y += 2.0;
        }
        for i in 0..120 {
            let a = i as f32 / 120.0 * std::f32::consts::TAU;
            dot(hud, at(a, r - 0.5), 1.5, [1.0, 0.96, 0.86, 0.45]);
        }
        // The card: ticks every 10 degrees, longer every 30, letters at the cardinals.
        for i in 0..36 {
            let a = (i as f32 * 10.0).to_radians() - b;
            if i % 9 == 0 {
                continue;
            }
            let len = if i % 3 == 0 { r * 0.16 } else { 1.0 };
            let mut d = r - 4.0;
            while d >= r - 4.0 - len {
                dot(hud, at(a, d), 2.0, ink);
                d -= 1.0;
            }
        }
        let ls = (r / 22.0).max(1.0);
        for (i, l) in ["N", "E", "S", "W"].iter().enumerate() {
            let a = (i as f32 * 90.0).to_radians() - b;
            let (x, y) = at(a, r - 5.5 * ls - 3.0);
            let c = if i == 0 { red } else { ink };
            hud.label(x - 2.5 * ls, y - 3.5 * ls, ls, c, l);
        }
        // The needle: red to the north, pale to the south, tapering to points.
        let len = r * 0.42;
        let mut d = 0.0;
        while d <= len {
            let wdt = 1.0 + 3.0 * (1.0 - d / len);
            dot(hud, at(-b, d), wdt, red);
            if d <= len * 0.75 {
                dot(hud, at(std::f32::consts::PI - b, d), wdt, [0.85, 0.85, 0.8, 0.9]);
            }
            d += 0.75;
        }
        dot(hud, (cx, cy), 4.0, [0.15, 0.15, 0.12, 1.0]);
        // The fixed mark: a small wedge over the rim, pointing in.
        for k in 0..4 {
            let half = (4 - k) as f32;
            hud.rect(cx - half, cy - r - 9.0 + 2.0 * k as f32, half * 2.0, 2.0, ink);
        }
    }

    /// Read the herds from the save folder (the headless `VG_LOAD_HERDS`
    /// hook; a game that is not fresh does this as it opens).
    pub fn load_herds(&mut self) -> bool {
        self.ents.load_roster(&self.save_dir)
    }

    /// Write the herds alone (the headless `VG_SAVE_HERDS` hook): how many.
    pub fn save_herds(&self) -> std::io::Result<usize> {
        let _ = std::fs::create_dir_all(&self.save_dir);
        self.ents.save_roster(&self.save_dir).map(|_| self.ents.roster.herds.len())
    }

    /// Save: the player asked (F5, the pause menu), or the world is closing
    /// (the menu, another world, quitting). The herds are written too.
    pub fn save(&mut self) {
        self.save_with(true);
    }

    /// Save what is owed. `herds`: write the roster as well
    /// (creatures/roster.rs); the half-minute autosave leaves it, since it
    /// is a few hundred kilobytes and only needs to be right when the
    /// player saves or leaves.
    fn save_with(&mut self, herds: bool) {
        if !self.persist {
            return;
        }
        let _ = std::fs::create_dir_all(&self.save_dir);
        // What a match did to the arena is not saved, and neither is it
        // allowed to wipe what exploring this world dug: the file is left
        // exactly as it was.
        let ok = if self.arena {
            true
        } else {
            let edits = self.streamer.edits_snapshot();
            save_edits(&self.save_dir.join("edits.bin"), &edits).is_ok()
        };
        let p = Progress {
            version: PROGRESS_VERSION,
            pos: self.player.pos.to_array(),
            yaw: self.player.yaw,
            pitch: self.player.pitch,
            gems: self.ents.gems_collected.iter().copied().collect(),
            camera: Some(self.camera.save()),
            best_flight: self.ride.soar.best,
            season: Some(self.year.season.name().to_string()),
            day: self.year.day,
            // A car being driven is saved as parked where it is.
            parked: self.ride.car().or(self.ride.parked.as_ref()).map(|c| ParkedSave {
                name: c.spec.name.clone(),
                pos: c.pos().to_array(),
                rot: c.b.rot.to_array(),
            }),
        };
        let path = self.save_dir.join("progress.json");
        let tmp = path.with_extension("tmp");
        let ok = ok && std::fs::write(&tmp, serde_json::to_string_pretty(&p).unwrap()).is_ok() && std::fs::rename(&tmp, &path).is_ok();
        let ok = ok && (!herds || self.arena || self.ents.save_roster(&self.save_dir).is_ok());
        // A save that failed leaves the edits owed, so the autosave tries
        // again in half a minute instead of believing it is up to date.
        if ok {
            self.streamer.dirty_edits = false;
        }
        self.note(if ok { "SAVED" } else { "SAVE FAILED" });
    }
}

/// How to develop a camera photo: the frame's pixels and the lens.
pub struct ShotPlan {
    /// x, y, w, h in screen pixels.
    pub crop: [u32; 4],
    pub lens: crate::render::LensFx,
}

/// What the camera's tasks may ask for in this world: its creatures (not
/// rare morphs, fish or insects) and the trees near the landing.
fn task_pool(species: &[crate::entities::Species], models: &[Model], gen: &Generator, spawn: Vec3) -> camera::tasks::Pool {
    use crate::creatures::locomotion::Mover;
    let mut pool = camera::tasks::Pool::default();
    let morphs: Vec<usize> = species.iter().flat_map(|s| s.morphs.iter().map(|m| m.0)).collect();
    // Only creatures some biome of this world asks for: the library is shared
    // between worlds.
    let lives_here: Option<HashSet<&str>> = gen.world.as_ref().map(|w| {
        let mut used = [false; 256];
        for &b in &w.f.biome {
            used[b as usize] = true;
        }
        w.lib
            .biomes
            .iter()
            .enumerate()
            .filter(|(i, _)| used.get(*i).copied().unwrap_or(false))
            .flat_map(|(_, b)| b.creatures.iter().map(|c| c.creature.as_str()))
            .collect()
    });
    for (i, sp) in species.iter().enumerate() {
        if morphs.contains(&i) || lives_here.as_ref().is_some_and(|l| !l.contains(sp.name.as_str())) {
            continue;
        }
        // Too small to fill much of a frame (insects).
        if models.get(sp.model).is_some_and(|m| m.height < 0.12) {
            continue;
        }
        let mover = sp.verb().mover;
        let (flies, walks) = match mover {
            Mover::School | Mover::Breach | Mover::Creep | Mover::Buzz { .. } => continue,
            Mover::Flock { .. } | Mover::Circle { .. } | Mover::Flit | Mover::Cling | Mover::Hum => (true, false),
            Mover::Walk { .. } => (false, models.get(sp.model).is_some_and(|m| m.rig.is_some())),
            _ => (false, false),
        };
        pool.animals.push(camera::tasks::Animal {
            name: camera::scene::display(&sp.name),
            flies,
            walks,
            groups: sp.beh.group.is_some(),
        });
    }
    if species.is_empty() {
        for (name, flies) in [("DEER", false), ("RABBIT", false), ("FOX", false), ("GULL", true)] {
            pool.animals.push(camera::tasks::Animal {
                name: name.into(),
                flies,
                walks: false,
                groups: false,
            });
        }
    }
    let mut plants: Vec<String> = gen
        .trees_near(spawn.x - 150.0, spawn.z - 150.0, spawn.x + 150.0, spawn.z + 150.0)
        .iter()
        .filter(|t| t.top >= 2.0 && t.pos.y > gen.spec.sea_level_m)
        .map(|t| camera::scene::tree_name(gen, t))
        .collect();
    plants.sort();
    plants.dedup();
    pool.plants = plants;
    pool
}

/// Read progress.json. One that won't parse is copied to progress.json.bad and
/// reported, so the next autosave doesn't silently replace it.
///
/// So is one that parses but loses something on the way in. The camera's
/// film, tasks and points load leniently (`lenient`), and so does the style,
/// which is right -- a renamed task should not cost the player their place in
/// the world -- but it used to mean that the first save afterwards wrote the
/// reset over the only copy. A file from a newer build is kept for the same
/// reason: this build would write it back without the fields it does not know.
fn load_progress(path: &std::path::Path) -> Option<Progress> {
    let s = std::fs::read_to_string(path).ok()?;
    let raw: serde_json::Value = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => {
            crate::util::set_aside(path, format_args!("{e}; starting without it"));
            return None;
        }
    };
    let p: Progress = match serde_json::from_value(raw.clone()) {
        Ok(p) => p,
        Err(e) => {
            crate::util::set_aside(path, format_args!("{e}; starting without it"));
            return None;
        }
    };
    let had = |key: &str| raw.get(key).is_some_and(|v| !v.is_null());
    if had("camera") && p.camera.is_none() {
        crate::util::set_aside(path, "the camera's film, tasks and points could not be read and start over");
    } else if p.version > PROGRESS_VERSION {
        crate::util::set_aside(path, format_args!("written by a newer build (version {}, this one writes {PROGRESS_VERSION})", p.version));
    }
    Some(p)
}

/// edits.bin: "VXE2", then the names of the materials used (u16 count; each
/// u8 id, u8 length, name bytes), then u32 count and per edit i32 x, y, z and
/// the u8 id. Names, not ids, decide the material on load, so renumbering
/// library materials can't change a saved build. "VXED" files (ids only, no
/// names) still load.
fn save_edits(path: &std::path::Path, edits: &EditMap) -> std::io::Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(b"VXE2");
    let mut used = [false; 256];
    for list in edits.values() {
        for &m in list.values() {
            used[m as usize] = true;
        }
    }
    let names: Vec<(u8, &str)> = (0..=255u8).filter(|&m| used[m as usize]).filter_map(|m| crate::world::matlib::key(m).map(|k| (m, k))).collect();
    out.extend_from_slice(&(names.len() as u16).to_le_bytes());
    for (m, k) in &names {
        out.push(*m);
        out.push(k.len() as u8);
        out.extend_from_slice(k.as_bytes());
    }
    let mut n = 0u32;
    let mut body = Vec::new();
    for (&(cx, cy, cz), list) in edits {
        for (&i, &m) in list {
            let i = i as i32;
            let (x, y, z) = (cx * 32 + i % 32, cy * 32 + i / 1024, cz * 32 + (i / 32) % 32);
            body.extend_from_slice(&x.to_le_bytes());
            body.extend_from_slice(&y.to_le_bytes());
            body.extend_from_slice(&z.to_le_bytes());
            body.push(m);
            n += 1;
        }
    }
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&body);
    let tmp = path.with_extension("tmp");
    std::fs::File::create(&tmp)?.write_all(&out)?;
    std::fs::rename(tmp, path)
}

///
/// A file that is cut short, or that starts with neither name, is copied to
/// edits.bin.bad and whatever could be read of it is used. Without the copy
/// the next autosave after one dig wrote the part that loaded -- or nothing
/// -- over everything the player had built.
fn load_edits(path: &std::path::Path) -> EditMap {
    let (map, damage) = read_edits(path);
    if let Some(why) = damage {
        let kept: usize = map.values().map(|l| l.len()).sum();
        crate::util::set_aside(path, format_args!("{why}; {kept} edits could be read"));
    }
    map
}

/// The edits in the file, and what was wrong with it if anything was.
fn read_edits(path: &std::path::Path) -> (EditMap, Option<&'static str>) {
    let mut map = EditMap::default();
    let Ok(mut f) = std::fs::File::open(path) else {
        return (map, None);
    };
    let mut data = Vec::new();
    if f.read_to_end(&mut data).is_err() {
        return (map, Some("it could not be read"));
    }
    if data.len() < 8 {
        return (map, Some("it is too short to be a list of edits"));
    }
    // Saved id -> current id; ids with no saved name keep their number.
    let mut remap: [Option<u8>; 256] = std::array::from_fn(|m| Some(m as u8));
    let mut o = 4;
    match &data[..4] {
        b"VXED" => {}
        b"VXE2" => {
            let count = u16::from_le_bytes([data[4], data[5]]) as usize;
            o = 6;
            for _ in 0..count {
                let (Some(&m), Some(&len)) = (data.get(o), data.get(o + 1)) else {
                    return (map, Some("it ends inside its list of materials"));
                };
                let Some(name) = data.get(o + 2..o + 2 + len as usize) else {
                    return (map, Some("it ends inside its list of materials"));
                };
                let name = String::from_utf8_lossy(name);
                remap[m as usize] = crate::world::matlib::id_of(&name);
                if remap[m as usize].is_none() {
                    eprintln!("{}: material {name} is no longer in the library; its edits are dropped", path.display());
                }
                o += 2 + len as usize;
            }
        }
        _ => return (map, Some("it does not start with VXE2 or VXED")),
    }
    let Some(head) = data.get(o..o + 4) else {
        return (map, Some("it ends before its edits begin"));
    };
    let n = u32::from_le_bytes(head.try_into().unwrap()) as usize;
    o += 4;
    let mut damage = None;
    for _ in 0..n {
        if o + 13 > data.len() {
            damage = Some("it ends before the last of its edits");
            break;
        }
        let rd = |o: usize| i32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let (x, y, z) = (rd(o), rd(o + 4), rd(o + 8));
        let m = remap[data[o + 12] as usize];
        o += 13;
        let Some(m) = m else { continue };
        let i = ((y & 31) * 1024 + (z & 31) * 32 + (x & 31)) as u16;
        map.entry((x >> 5, y >> 5, z >> 5)).or_default().insert(i, m);
    }
    (map, damage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::{PLANKS, STONE};

    /// Edits survive a save and load, and the old id-only format still loads.
    #[test]
    fn edits_round_trip() {
        let dir = std::env::temp_dir().join(format!("voxel_edits_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edits.bin");
        let mut edits = EditMap::default();
        edits.entry((-1, 3, 2)).or_default().insert(5 * 1024 + 6 * 32 + 7, PLANKS);
        edits.entry((0, 0, 0)).or_default().insert(0, AIR);
        save_edits(&path, &edits).unwrap();
        assert_eq!(load_edits(&path), edits);
        // Version 1: "VXED", count, then x, y, z, id.
        let mut v1 = b"VXED".to_vec();
        v1.extend_from_slice(&1u32.to_le_bytes());
        for c in [-25i32, 100, 64] {
            v1.extend_from_slice(&c.to_le_bytes());
        }
        v1.push(STONE);
        std::fs::write(&path, v1).unwrap();
        let old = load_edits(&path);
        // y = 64 sits on a chunk floor, so its term is zero; written out so
        // the index reads the same way the loader builds it.
        #[allow(clippy::erasing_op)]
        let i = ((100 & 31) * 1024 + (64 & 31) * 32 + (-25i32 & 31)) as u16;
        assert_eq!(old.get(&(-1, 3, 2)).and_then(|l| l.get(&i)), Some(&STONE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("voxel_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A damaged edits.bin is copied aside before anything can save over it:
    /// one cut short gives up the edits that are whole, and one that is not
    /// an edits file at all gives none, and both leave a `.bad` holding the
    /// bytes that were there. A sound file leaves no copy.
    #[test]
    fn a_damaged_edits_file_is_kept() {
        let dir = scratch("edits_damaged");
        let path = dir.join("edits.bin");
        let bad = dir.join("edits.bin.bad");
        let mut edits = EditMap::default();
        for k in 0..6u16 {
            edits.entry((0, 0, 0)).or_default().insert(k, STONE);
        }
        save_edits(&path, &edits).unwrap();
        assert_eq!(load_edits(&path), edits);
        assert!(!bad.exists(), "a sound file was set aside");

        // Cut it off in the middle of the fourth edit.
        let whole = std::fs::read(&path).unwrap();
        let cut = &whole[..whole.len() - 13 * 2 - 5];
        std::fs::write(&path, cut).unwrap();
        let part = load_edits(&path);
        assert_eq!(part.values().map(|l| l.len()).sum::<usize>(), 3, "the whole edits before the cut load");
        assert_eq!(std::fs::read(&bad).unwrap(), cut, "the copy is the file as it was found");

        std::fs::remove_file(&bad).unwrap();
        std::fs::write(&path, b"not an edits file at all").unwrap();
        assert!(load_edits(&path).is_empty());
        assert!(bad.exists(), "a file with the wrong start was not set aside");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// progress.json: a camera that will not read costs the camera and
    /// nothing else, and the file as it was is kept beside it. A style or
    /// slopes left in an older file are ignored, since they are the player's
    /// settings now (`Settings`). What is written carries the version.
    #[test]
    fn progress_survives_what_it_cannot_read() {
        let dir = scratch("progress");
        let path = dir.join("progress.json");
        let bad = dir.join("progress.json.bad");

        std::fs::write(&path, r#"{"pos":[1.0,2.0,3.0],"gems":[[4,5]],"day":7}"#).unwrap();
        let p = load_progress(&path).expect("an old file with no version loads");
        assert_eq!((p.pos, p.day, p.version), ([1.0, 2.0, 3.0], 7, 0));
        assert!(!bad.exists(), "a sound file was set aside");

        // A file from when the style and slopes were saved with the world:
        // they are ignored, and nothing else is lost over them.
        std::fs::write(&path, r#"{"version":1,"pos":[1.0,2.0,3.0],"style":"a style from next year","slopes":false,"gems":[[4,5]]}"#).unwrap();
        let p = load_progress(&path).expect("an old style is not a reason to lose the file");
        assert_eq!(p.pos, [1.0, 2.0, 3.0]);
        assert_eq!(p.gems, vec![(4, 5)]);
        assert!(!bad.exists(), "a file was set aside over a setting it no longer holds");

        std::fs::write(&path, r#"{"pos":[1.0,2.0,3.0],"camera":{"tasks":"this was a list once"}}"#).unwrap();
        let p = load_progress(&path).expect("a camera that will not read is not a reason to lose the file");
        assert_eq!(p.pos, [1.0, 2.0, 3.0]);
        assert!(p.camera.is_none());
        assert!(bad.exists(), "the file with the unread camera was not kept");

        std::fs::remove_file(&bad).unwrap();
        std::fs::write(&path, "{ this is not json").unwrap();
        assert!(load_progress(&path).is_none());
        assert!(bad.exists());

        let written = serde_json::to_value(Progress {
            version: PROGRESS_VERSION,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(written["version"], PROGRESS_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The controls screen: the key table (input.rs), without the modes a world
/// leaves out (`allowed`), over a dimmed background.
pub fn draw_help(hud: &mut Hud, allowed: crate::modes::Allowed, w: f32, h: f32, _s: f32, foot: &str) {
    let boxes = crate::input::help_boxes(allowed);
    crate::menu::draw_controls(hud, &boxes, w, h, foot);
}

// ---- Scuba (player/dive.rs, docs/UNDERWATER.md) ----------------------------

impl Game {
    /// What the dive asked for this step: a line on the HUD and its bubbles.
    fn dive_extras(&mut self) {
        if let Some(n) = self.player.dive.note.take() {
            self.note(&n);
        }
        let cues: Vec<crate::fx::Cue> = self
            .player
            .dive
            .cues
            .drain(..)
            .map(|(name, at, amount)| crate::fx::Cue {
                name,
                at,
                radius: 0.12,
                dir: Vec3::Y,
                amount,
            })
            .collect();
        if self.fx.on {
            let mut cues = cues;
            self.fx.take(&mut cues);
        }
        // A current shows as streaks in the water flowing past, the way
        // the motes give away a shaft of sun.
        if self.player.diving() && self.fx.on {
            let level = self.water_at(self.player.pos).0;
            let me = self.player.pos;
            let seed = (self.time * 977.0) as u32;
            for k in 0..3u32 {
                let h = crate::world::noise::hash32(seed ^ k.wrapping_mul(0x9e37_79b9));
                let off = Vec3::new(crate::world::noise::unit(h) - 0.5, crate::world::noise::unit(h >> 8) - 0.5, crate::world::noise::unit(h >> 16) - 0.5) * 16.0;
                let at = me + off;
                if at.y > level - 0.3 {
                    continue;
                }
                let store = &self.store;
                let v = crate::player::dive::sea_current(at, level, |x, z| crate::player::dive::deep_for_current(store, x, z, level));
                if v.length() > 1.2 {
                    self.fx.trickle("current_streak", at, v.normalize(), 0, 14.0 * (v.length() / 4.0).min(1.5), STEP);
                }
            }
        }
    }

    /// Holding on to a big swimmer: E held under water within reach of a
    /// whale, a manta or a turtle takes hold, and from then on the diver
    /// goes where it goes, at the place on its body they took hold of,
    /// until E is let go (player/dive.rs lets go).
    fn hitch_step(&mut self, grab: bool) {
        use crate::creatures::{Creature, Look};
        use crate::player::dive::Hitch;
        if !self.player.diving() {
            self.player.dive.hitch = None;
            return;
        }
        let world = &self.ents.world;
        if let Some(h) = self.player.dive.hitch {
            let Ok(look) = world.get::<&Look>(h.who) else {
                self.player.dive.hitch = None;
                return;
            };
            let q = glam::Quat::from_rotation_y(look.yaw);
            let to = look.pos + q * h.off;
            let from = self.player.pos;
            if self.store.loaded((to.x * 4.0).floor() as i32, (to.y * 4.0).floor() as i32, (to.z * 4.0).floor() as i32)
                && !crate::physics::hits_wet(&self.store, to, self.player.body.half_w, self.player.body.height)
            {
                self.player.pos = to;
                self.player.body.vel = (to - from) / STEP;
            } else {
                // Dragged into rock: let go.
                self.player.dive.hitch = None;
            }
            return;
        }
        // Out of breath, nobody takes hold: the diver is on the way up.
        if !grab || self.player.dive.breath <= 0.0 {
            return;
        }
        let me = self.player.pos + Vec3::Y * crate::player::dive::DIVE_EYE;
        let mut best: Option<(f32, hecs::Entity, Vec3, f32)> = None;
        for (e, look, _) in world.query::<(hecs::Entity, &Look, &Creature)>().iter() {
            let m = &self.models[look.model];
            // Big enough to hold on to: mantas, turtles, whales.
            if m.half_w < 0.32 && m.height < 0.9 {
                continue;
            }
            let reach = 1.4 + m.half_w.max(m.height * 0.5);
            let d = (look.pos + Vec3::Y * m.height * 0.5 - me).length();
            if d < reach && best.is_none_or(|b| d < b.0) {
                best = Some((d, e, look.pos, look.yaw));
            }
        }
        if let Some((_, who, at, yaw)) = best {
            let q = glam::Quat::from_rotation_y(yaw);
            let mut off = q.inverse() * (self.player.pos - at);
            // Hang on close: within an arm's length of its back or flank.
            off = off.clamp_length_max(1.6);
            self.player.dive.hitch = Some(Hitch { who, off });
            self.note("HOLDING ON   LET GO OF E TO LET GO");
        }
    }

    /// The breath left, under the ride's line at the foot of the screen, and
    /// where the kayak is when it is moored somewhere.
    fn build_breath(&self, hud: &mut Hud, cx: f32, h: f32, s: f32, line: f32) {
        let left = (self.player.dive.breath / crate::player::dive::BREATH).clamp(0.0, 1.0);
        let y = h - line * 1.5 - 30.0;
        let mut text = String::from(if self.player.dive.stream > 1.2 { "BREATH   CURRENT" } else { "BREATH" });
        if let Some((at, _)) = self.ride.moored {
            let d = at - self.player.pos;
            let flat = glam::Vec2::new(d.x, d.z);
            // Compass bearing: 0 north (-z), 90 east (+x).
            let bearing = flat.x.atan2(-flat.y).to_degrees().rem_euclid(360.0);
            const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
            let point = POINTS[((bearing + 22.5) / 45.0) as usize % 8];
            text = format!("{text}     KAYAK {:.0} M {point}", flat.length());
        }
        let tw = Hud::text_width(&text, s * 1.5);
        hud.label(cx - tw * 0.5, y, s * 1.5, [0.86, 0.97, 1.0, 0.9], &text);
        let (bw, bh) = (90.0 * s, 3.0 * s);
        let by = y + 15.0 * s;
        hud.rect(cx - bw * 0.5 - 1.0, by - 1.0, bw + 2.0, bh + 2.0, [0.0, 0.0, 0.0, 0.45]);
        let c = if left > 10.0 / crate::player::dive::BREATH {
            [0.75, 0.95, 1.0, 0.9]
        } else {
            [1.0, 0.55, 0.4, 0.95]
        };
        hud.rect(cx - bw * 0.5, by, bw * left, bh, c);
    }
}
