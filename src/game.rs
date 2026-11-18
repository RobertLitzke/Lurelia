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

pub const STEP: f32 = 1.0 / 60.0;
/// How near the camera a `WorldVent` must be to breathe, metres.
const VENT_REACH: f32 = 220.0;
/// Falling snow round the camera: half-width of the box (m) and flakes a
/// second in full snow.
const SNOW_BOX: f32 = 13.0;
const SNOW_RATE: f32 = 650.0;

/// Bodies within this of the camera are kept however the camera faces: they
/// can cast a shadow into the view. It covers both shadow cascades that draw
/// creatures (render/mod.rs `splits` and `SLICE_MARGIN`), with room to spare.
const SHADOW_CASTERS_M: f32 = 80.0;

/// The water surface over a point and the current there, as
/// `Player::update` wants it: the world's own water, which is a field.
/// `water_at` adds the water a structure holds as voxels, and is what
/// anything stepping a body should ask.
pub fn water_over(land: Option<&crate::world::fields::World>, sea: f32, p: Vec3) -> (f32, glam::Vec2) {
    match land {
        Some(w) => match w.f.wet(p.x, p.z) {
            Some(wet) if wet.level > sea => (wet.level, glam::Vec2::from(wet.flow)),
            _ => (sea, glam::Vec2::ZERO),
        },
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

/// The settings a player keeps from one run to the next, whatever world it
/// is. `menu::Prefs` holds them (flattened, so settings.json reads as it
/// always has) and a game is handed them when it starts; the keys that change
/// them in play (G, T, B, F3, F6) change `Game::settings`, and the window
/// copies that back. Nothing here is saved with a world.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shadows: bool,
    /// A style this build cannot read takes its default and leaves the
    /// other settings alone.
    #[serde(deserialize_with = "crate::util::or_default")]
    pub style: Style,
    pub slopes: bool,
    pub show_stats: bool,
    /// The debug minimap (F6): a north-up map of the ground round the player.
    pub minimap: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shadows: true,
            style: Style::default(),
            slopes: true,
            show_stats: false,
            minimap: false,
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
    /// The models' colours (models::PALETTE_SLOTS), handed to the renderer with them.
    pub palette: Vec<[f32; 4]>,
    /// How many of the palette's library colour slots the loaded world filled.
    /// The EDIT page appends its subject's colours after these.
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
    pub frame_ms: VecDeque<f32>,
    dig_cd: f32,
    place_cd: f32,
    message: Option<(String, f32)>,
    autosave: f32,
    /// The creature or tree under the crosshair, shown in the corner.
    pub looking_at: Option<String>,
    pub last_render: FrameStats,
    pub gpu_name: String,
    pub freeze_creatures: bool,
    /// Index of the first ride model (bike, flyer) in `models`.
    pub ride_models: usize,
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
        // This world's sun and northern lights, before anything reads an hour.
        let sun_path = loaded.as_ref().and_then(|l| l.world.def.sun.as_ref()).map_or(crate::render::daylight::SunPath::DEFAULT, |s| s.path());
        crate::render::daylight::set_path(sun_path);
        crate::render::aurora::set_strength(loaded.as_ref().map_or(0.0, |l| l.world.def.aurora), spec.seed as u32);
        // The weather this world gets, and the kind it opens on: whatever
        // `world.json` says, else the commonest kind its climate allows.
        let climate = loaded.as_ref().map(|l| crate::world::weather::Climate::from_weights(&l.world.def.weather)).unwrap_or_default();
        let start_sky = if !launch.weather {
            // The launch screen's switch off: fair weather, held.
            crate::world::weather::Kind::Sun
        } else {
            loaded
                .as_ref()
                .and_then(|l| l.world.def.start_weather.as_deref())
                .and_then(crate::world::weather::Kind::parse)
                .unwrap_or_else(|| climate.commonest())
        };
        let weather = crate::world::weather::Weather::new(start_sky, climate, spec.seed, launch.weather);
        let height_vox = gen.height_vox;
        // A match opens in the arena as it was built. The craters of the
        // last one are that match, not the map, and a world you are
        // exploring is the only place where what you dug is worth keeping,
        // which is the same line the saved position is on below.
        let arena = launch.mode == crate::launch::Mode::Fps;
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
        let turning = launch.day_night && launch.season.is_none();
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
        year.day = progress.as_ref().map_or(0, |p| p.day);
        let mut player = Player::new(spawn + Vec3::Y * 0.5, yaw);
        let mut ride = Ride::default();
        let mut gems = HashSet::default();
        if let Some(p) = &progress {
            // Where you were is only worth coming back to in a world you
            // are exploring. A match opens on a spawn point, so remembering
            // the spot the last one ended on puts you back in it every
            // time -- which on one imported map is the bottom of the water.
            if launch.mode != crate::launch::Mode::Fps {
                player = Player::new(Vec3::from(p.pos), p.yaw);
                player.pitch = p.pitch;
            }
            ride.soar.best = p.best_flight;
            gems = p.gems.iter().copied().collect();
        }
        ride.allowed = allowed;
        ride.glide_air = loaded.as_ref().and_then(|l| l.world.def.glide.as_ref()).map_or_else(Default::default, |g| g.air());
        ride.set_seed(spec.seed);
        let sea = spec.sea_level_m;
        let mut models = models::build_all();
        let mut palette = models::palette();
        let mut ents = Entities::new(gems, spec.seed);
        let mut library_colours = 0;
        let mut platforms = crate::platforms::Platforms::default();
        let mut portals = crate::portals::Portals::default();
        if let Some(l) = loaded {
            library_colours = l.colours.len();
            for (slot, c) in models::LIBRARY_COLOURS.zip(&l.colours) {
                palette[slot] = *c;
            }
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
        }
        let pool = task_pool(&ents.species, &models, &gen, spawn);
        let finds = ents.gems_collected.len();
        let cam_save = progress.as_ref().and_then(|p| p.camera.clone());
        let camera = Camera::new(cam_save, finds, pool, spec.seed, camera::scrapbook::Scrapbook::open(&save_dir));
        let mut streamer = Streamer::new(gen, edits, view_m, threads);
        streamer.set_smooth(settings.slopes, Vec3::ZERO);
        // Which part of the year the first columns are built for. Nothing is
        // loaded yet, so this costs no rebuild.
        streamer.set_turn(year.turn(), Vec3::ZERO);
        Ok(Self {
            hour: start_hour,
            clock_running: launch.day_night,
            weather,
            year,
            streamer,
            store: Store { height_vox, ..Store::default() },
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
            frame_ms: VecDeque::new(),
            dig_cd: 0.0,
            place_cd: 0.0,
            message: None,
            autosave: 30.0,
            looking_at: None,
            last_render: FrameStats::default(),
            gpu_name: String::new(),
            freeze_creatures: false,
            ride_models: usize::MAX,
            weapon_models: usize::MAX,
            camera,
            files: Vec::new(),
            activity: None,
            parked: None,
            editing: false,
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
            let keep = a.id() == ModeId::Shooter;
            a.exit(&mut self.player);
            if keep {
                self.parked = Some(a);
            }
        }
    }

    /// On a world whose world.json carries a `"shooter"` block, the gun is
    /// never put away: start the rules as soon as the player is on the
    /// ground, and again after anything else has had the activity slot.
    fn start_shooter(&mut self) {
        if self.editing || self.activity.is_some() || !self.spawned || !self.ride.allowed.has(ModeId::Shooter) {
            return;
        }
        if let Some(a) = self.parked.take() {
            self.activity = Some(a);
            return;
        }
        let Some(w) = self.generator().world.clone() else { return };
        let Some(mut rules) = w.def.shooter.clone() else { return };
        // The launch screen says how many bots there are and how well they
        // shoot; the world says everything else about the match. Asked for
        // exactly what the world's own rules name, the world's own bots are
        // played, names and characters and all.
        if !self.launch.world_roster(&rules.bots) {
            rules.bots = self.launch.roster();
        }
        rules.target_score = self.launch.target;
        if rules.spoil.is_none() {
            rules.spoil = w.def.rock.first().map(|r| r.0.clone());
        }
        let seed = w.def.seed;
        let mut sh = match self.repeat {
            true => crate::shooter::Shooter::repeatable(&w.lib, &rules, self.home, &self.store, seed),
            false => crate::shooter::Shooter::new(&w.lib, &rules, self.home, &self.store, seed),
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
        self.weapon_models = self.models.len();
        self.models.extend(crate::combat::viewmodel::models());
        crate::combat::viewmodel::palette(&mut self.palette);
        // The bots' bodies come straight after the guns, because an activity
        // is handed one base index and counts on from it. They are built from
        // the library here and again in `Shooter::new`, which wants the rig:
        // `avatar::build` is pure in the library, so the two agree.
        let mut palette = std::mem::take(&mut self.palette);
        let av = crate::brain::avatar::build(self.library(), &mut palette);
        self.palette = palette;
        self.models.extend(av.models);
    }

    pub fn note(&mut self, msg: &str) {
        self.message = Some((msg.to_string(), 2.5));
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
            if let Some(turn) = self.year.advance(step, self.hour, sky.snow) {
                let p = self.player.pos;
                self.streamer.set_turn(turn, p);
            }
            if let Some(k) = self.year.wants_weather() {
                self.weather.nudge(k);
            }
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
                let eye = self.frame_params(1.0).cam_pos;
                self.activity = Some(Box::new(Screenshot::new(eye, self.player.yaw, self.player.pitch)));
            }
        }
        self.start_shooter();
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
        if std::mem::take(&mut actions.glide) && self.spawned && !self.player.fly {
            self.ride.glide_key(&mut self.player);
        }
        if std::mem::take(&mut actions.cam_toggle) {
            self.ride.toggle_chase();
        }
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
        let eye = self.eye(self.alpha);
        let fwd = self.player.forward();
        self.target = if self.spawned { self.store.raycast(eye, fwd, 6.0) } else { None };
        self.looking_at = if self.spawned { self.identify(eye, fwd) } else { None };
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
                self.save();
            }
        }
    }

    /// Names the creature or tree on the line of sight, whichever is nearer.
    fn identify(&self, eye: Vec3, fwd: Vec3) -> Option<String> {
        const RANGE: f32 = 60.0;
        let block = self.store.raycast(eye, fwd, RANGE);
        let block_t = block.map_or(RANGE, |(v, _)| {
            let c = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * 0.25;
            (c - eye).length()
        });
        if let Some((t, name)) = self.ents.identify(&self.models, eye, fwd, RANGE) {
            if t < block_t + 0.5 {
                return Some(name);
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
        Some(name.to_uppercase())
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
        for (k, when) in &w.when {
            let Some(flag) = w.shown.get(*k as usize) else {
                continue;
            };
            let (x0, z0, x1, z1) = w.placed[*k as usize].footprint(&w.shapes);
            let (x0, z0, x1, z1) = (x0 as f32 * 0.25, z0 as f32 * 0.25, x1 as f32 * 0.25, z1 as f32 * 0.25);
            let c = glam::Vec2::new((x0 + x1) * 0.5, (z0 + z1) * 0.5);
            let was = flag.load(std::sync::atomic::Ordering::Relaxed);
            let now = when.stands(self.hour, c.distance(glam::Vec2::new(p.x, p.z)), was);
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
            if undone > 0 {
                self.note("THE ARENA IS MENDED");
            }
        }
        if let Some(sh) = a.shooter_mut() {
            let cues = sh.combat.take_cues();
            self.combat_fx(&cues);
        }
        if let Some(m) = a.note() {
            self.note(&m);
        }
        self.activity = Some(a);
    }

    fn fixed_step(&mut self, input: &MoveInput, paused: bool) {
        // The moving parts first, then the activity, then the player: a
        // rider steps off a floor that has already moved this step.
        let player = self.spawned.then_some(&mut self.player);
        self.parts.step(STEP, &mut self.store, player, self.activity.as_deref_mut());
        if std::mem::take(&mut self.parts.placed) {
            // A platform has just been put where it lives: keep the grass
            // out of the room it sweeps, and mesh the ground under it again.
            let boxes = self.parts.platforms.keep_out();
            crate::mesh::sprites::set_keep_out(boxes.clone());
            for b in boxes {
                self.streamer.remesh_box(&self.store, Vec3::new(b[0], b[1] - 2.0, b[2]), Vec3::new(b[3], b[4], b[5]));
            }
        }
        self.with_activity(|a, cx| a.step(cx, input));
        if paused {
            return;
        }
        self.time += STEP;
        self.update_conditional();
        let slopes = self.streamer.smooth();
        // The EDIT page's camera owns the eye while it is open (editor/cam.rs),
        // so the body is left standing where it was and put back untouched
        // when the page closes.
        if self.spawned && !self.editing {
            let water = self.water_at(self.player.pos);
            let before = (self.player.body.on_ground, self.player.body.vel, self.player.pos);
            self.player.update(&mut self.ride, input, &self.store, self.sea, water, slopes, STEP);
            self.step_fx(before, water.0);
            self.dive_extras();
        } else {
            self.player.prev_pos = self.player.pos;
        }
        if !self.freeze_creatures {
            // How the animals read the player: where, how fast and how they
            // travel (creatures/startle.rs). Flying with the cheat on reads
            // as standing still, so nothing bolts from a camera in the sky.
            let vel = if self.player.fly { Vec3::ZERO } else { self.player.body.vel };
            let threat = crate::creatures::startle::Threat::new(self.player.pos, vel, self.ride.mode).crouched(self.player.crouch);
            let gen = self.streamer.generator();
            self.ents.hour = self.hour;
            self.ents.sky = self.weather.sky();
            self.ents.phase = self.year.phase();
            crate::render::daylight::set_declination(self.ents.phase.declination);
            let f = self.player.forward();
            self.ents.facing = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
            self.ents.update(STEP, &self.store, gen, &threat, slopes, &self.models);
            // Spray the animals threw up this step (creatures/swim.rs).
            self.fx.take(&mut self.ents.cues);
        }
        if self.spawned && !self.editing {
            self.hitch_step(input.focus_far && !self.camera.active);
        }
        if self.spawned && !self.player.fly {
            self.shoulder_past_animals();
        }
    }

    /// One step of the particles: what the player's body did this step
    /// (`before` is its footing, velocity and position going in), what the
    /// trees are dropping, and the pool moved on. Cosmetic only: nothing
    /// here is read back by the simulation.
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
                Cue::Sparks { at } => {
                    self.fx.burst("impact_sparks", at, Vec3::Y, 0, 2.0);
                    self.fx.burst("bore_sparks", at, Vec3::Y, 0, 1.5);
                }
                Cue::Burst { at, radius } => {
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
        for (_, look, me, crit, age) in self.ents.world.query::<(hecs::Entity, &Look, Option<&Creature>, Option<&Critter>, Option<&Age>)>().iter() {
            if (me.is_none() && crit.is_none()) || age.is_some_and(|a| a.dead()) {
                continue;
            }
            let Some(m) = self.models.get(look.model) else { continue };
            if m.height < BLOCKS_AT_M {
                continue;
            }
            // Cheap reject before the box test: most animals in the world
            // are nowhere near you.
            if (look.pos.x - self.player.pos.x).abs() > 4.0 || (look.pos.z - self.player.pos.z).abs() > 4.0 {
                continue;
            }
            by += crate::physics::separate(self.player.pos, hw, h, look.pos, m.half_w, m.height);
        }
        if by != Vec3::ZERO {
            crate::physics::nudge(&self.store, &mut self.player.pos, by, hw, h);
        }
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

    fn camera(&self) -> crate::rig::Camera {
        if let Some(c) = self.activity.as_ref().and_then(|a| a.camera(&self.player)) {
            c
        } else if self.player.fly {
            crate::rig::Camera::looking(self.player.eye(self.alpha), self.player.yaw, self.player.pitch)
        } else {
            self.ride.camera(&self.player, &self.store, self.alpha)
        }
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
        let Some(w) = self.generator().world.clone() else {
            self.lamps.sort_to(eye);
            return;
        };
        // How far into the evening it is, for lamps lit only after dark.
        let e = crate::world::clock::sun_elevation(self.hour);
        let dark = ((2.0 - e) / 6.0).clamp(0.0, 1.0);
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
        // The moving parts keep stepping in a paused world; nothing else does.
        let alpha = self.still_alpha.unwrap_or(self.alpha);
        self.ents.instances(alpha, &self.models, &mut self.inst);
        let pinned_from = self.inst.len();
        if let (Some(a), true) = (&self.activity, self.weapon_models != usize::MAX) {
            a.instances(&self.player, alpha, self.weapon_models, &self.models, &mut self.inst);
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
        self.ride.tokens(self.ride_models, &self.models, &mut self.inst);
        self.ride.moored_instance(self.time, self.ride_models, &self.models, &mut self.inst);
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
        let highlight = self.target.map(|(h, _)| {
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
            entities: &self.inst,
            plates: &self.plates,
            fx: &self.fx,
            lamps: &self.lamps,
            shadows: self.settings.shadows,
            style: self.settings.style,
            slopes: self.streamer.smooth(),
            speed_fx: cam.speed_fx,
            streaks: cam.streaks,
            hour: self.hour,
            sky: self.weather.sky(),
            season: self.year.phase(),
            snow_cover: {
                let lying = self.year.turn().snow as f32 * 0.5;
                let floor = self.generator().world.as_ref().map_or(0.0, |w| w.def.snow_cover);
                lying.max(floor).min(1.0)
            },
            lens: (self.camera.active && self.camera.raised && self.camera.book.is_none() && self.activity.is_none()).then(|| self.camera.lens.fx(self.camera.orient, aspect, true)),
        }
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
    /// how far off it is and which voxel it is. For the stats HUD.
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
        format!("{} #{m}{grain}  {block_t:.1}M  {} {} {}", material::name(m), v[0], v[1], v[2])
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
    fn build_stats(&self, hud: &mut Hud, s: f32, dim: [f32; 4]) {
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
        let wmax = lines.iter().map(|l| Hud::text_width(l, s)).fold(0.0, f32::max);
        hud.rect(8.0, 8.0, wmax + 16.0, line * lines.len() as f32 + 12.0, [0.0, 0.0, 0.0, 0.35]);
        for (i, l) in lines.iter().enumerate() {
            hud.label(16.0, 14.0 + i as f32 * line, s, dim, l);
        }
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
            if let Some((m, t)) = &self.message {
                let a = t.min(1.0);
                let mw = Hud::text_width(m, s * 1.5);
                hud.label((w * 0.5 - mw * 0.5).floor(), h * 0.3, s * 1.5, [1.0, 1.0, 1.0, a], m);
            }
            // An activity draws its own HUD and nothing else of the game's,
            // but the frame stats are not the game's: they are the player's
            // instrument, and an arena is exactly where they are wanted.
            self.build_stats(hud, s, dim);
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

        self.build_stats(hud, s, dim);
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
        if let Some(name) = &self.looking_at {
            hud.label(16.0, h - line * 1.5 - 12.0 - foot, s * 1.5, [1.0, 0.96, 0.86, 0.95], name);
        }
        self.build_area_name(hud, h - foot, s, line);

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
    fn build_area_name(&self, hud: &mut Hud, h: f32, s: f32, line: f32) {
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
        hud.label(16.0, h - line * 3.0 - 16.0, s * 1.5, [0.98, 0.95, 0.88, a], name);
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

    pub fn save(&mut self) {
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
        };
        let path = self.save_dir.join("progress.json");
        let tmp = path.with_extension("tmp");
        let ok = ok && std::fs::write(&tmp, serde_json::to_string_pretty(&p).unwrap()).is_ok() && std::fs::rename(&tmp, &path).is_ok();
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
            Mover::School | Mover::Breach | Mover::Buzz { .. } => continue,
            Mover::Flock { .. } | Mover::Circle { .. } | Mover::Flit | Mover::Cling => (true, false),
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

        let written = serde_json::to_value(Progress { version: PROGRESS_VERSION, ..Default::default() }).unwrap();
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
            .map(|(name, at, amount)| crate::fx::Cue { name, at, radius: 0.12, dir: Vec3::Y, amount })
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
        if !grab {
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
        let c = if left > 10.0 / crate::player::dive::BREATH { [0.75, 0.95, 1.0, 0.9] } else { [1.0, 0.55, 0.4, 0.95] };
        hud.rect(cx - bw * 0.5, by, bw * left, bh, c);
    }
}
