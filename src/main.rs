//! Voxel adventure engine. Run with no arguments for the game window; see `--help`
//! for the headless screenshot and benchmark modes.

mod anim;
mod brain;
mod camera;
mod cli;
mod combat;
mod creatures;
mod editor;
mod entities;
mod form;
mod fx;
mod game;
mod games;
mod inherit;
mod input;
mod items;
mod launch;
mod library;
mod lookpage;
mod menu;
mod mesh;
mod models;
mod modes;
mod pacing;
mod physics;
mod platforms;
mod player;
mod portals;
mod race;
mod render;
mod ride;
mod rig;
mod screenshot;
mod shooter;
mod sound;
mod stream;
mod tools;
mod trace;
mod util;
mod variety;
mod vehicle;
mod world;
mod world_parts;
mod worldload;

use cli::{parse_args, Args};
use game::{Actions, Game};
use glam::Vec3;
use input::Action;
use menu::{Choice, Menu, Place, Prefs, Setting, Slides};
use pacing::Pacer;
use player::MoveInput;
use render::hud::Hud;
use render::{Gpu, Renderer};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tools::bench::Bench;
use tools::headless::run_headless;
use tools::inspect::{check_world, profile_gen, write_map};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

fn main() {
    let args = parse_args();
    if let Some(w) = &args.world {
        worldload::select(w);
    }
    if args.gazetteer || args.validate {
        std::process::exit(check_world(&args));
    }
    if let Some(path) = &args.seam {
        std::process::exit(tools::inspect::seam(path));
    }
    if let Some(path) = &args.map {
        if let Ok(l) = worldload::load(&worldload::selected()) {
            // VG_MAP_PX sets the image width; the fields are 1 m a cell, so the
            // world's size in metres is full resolution.
            let px = std::env::var("VG_MAP_PX").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);
            world::atlas::write(&l.world, path, px).expect("map file");
            println!("{}", world::chronicle::gazetteer(&l.world).lines().last().unwrap_or(""));
            return;
        }
        write_map(&args, path);
        return;
    }
    if args.duel.is_some() {
        tools::duel::run(&args);
        return;
    }
    if args.ctf.is_some() {
        tools::ctf::run(&args);
        return;
    }
    if args.league.is_some() {
        tools::league::run(&args);
        return;
    }
    if args.tourney.is_some() {
        tools::tourney::run(&args);
        return;
    }
    if args.profile_gen {
        profile_gen(&args);
        return;
    }
    if args.catalog.is_some() {
        tools::catalog::run(&args);
        return;
    }
    if args.sound_bake.is_some() {
        sound::bake::run(&args);
        return;
    }
    if args.headless {
        run_headless(args);
    } else {
        let event_loop = EventLoop::new().expect("event loop");
        event_loop.set_control_flow(ControlFlow::Poll);
        let mut app = App { args, run: None };
        event_loop.run_app(&mut app).expect("run");
    }
}

pub(crate) fn new_game(args: &Args) -> Result<Game, String> {
    // The launch screen's choices, or the world's own defaults when a world
    // is opened from the command line.
    let mut chosen = args.launch.clone().unwrap_or_else(|| launch::Chosen::for_world(&worldload::selected()));
    if let Some(g) = args.mode.as_deref().and_then(games::get) {
        chosen.set_game(g);
    }
    // A season asked for on the command line is part of the launch, not
    // something done to the world afterwards: the first columns are then
    // built in it and nothing has to be built twice.
    if let Some(s) = args.season.as_deref() {
        if crate::world::season::Season::parse(s).is_none() {
            return Err(format!("--season: no such season as {s}"));
        }
        chosen.season = Some(s.to_string());
    }
    let settings = crate::game::Settings {
        shadows: args.shadows,
        style: args.style.unwrap_or_default(),
        slopes: !args.blocky,
        show_stats: true,
        minimap: false,
    };
    let mut game = Game::new(args.save_dir.clone(), args.view, args.threads, args.fresh || args.headless, chosen, settings)?;
    if let Some(f) = args.full_detail {
        game.streamer.full_m = f.min(args.view);
    }
    game.add_ride_models();
    let (yaw, pitch) = args.look.map(|(y, p)| (y.to_radians(), p.to_radians())).unwrap_or((game.player.yaw, game.player.pitch));
    if let Some(p) = args.pos {
        game.set_camera(p, yaw, pitch, true);
        game.pin();
    } else if let Some((x, z)) = args.ground {
        let h = game.generator().height(x, z);
        game.set_camera(Vec3::new(x, h + 2.0, z), yaw, pitch, false);
    } else if args.look.is_some() {
        let p = game.player.pos;
        game.set_camera(p, yaw, pitch, false);
    }
    Ok(game)
}

/// The window, the GPU and whatever the window is showing: a menu, the
/// loading screen, or a world.
struct Running {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    gpu: Gpu,
    renderer: Renderer,
    gpu_name: String,
    /// The world being played; None on the title screen and while loading.
    game: Option<Game>,
    hud: Hud,
    input: MoveInput,
    actions: Actions,
    /// The mouse buttons as they are held, left and right. `Actions` is a
    /// list of things to do once, and whoever does one takes it out of the
    /// struct; a held trigger is a state, not an event, so it is kept here
    /// and written back into `actions` every frame. Without this a minigun
    /// fires one round per click however long the button is down.
    held: (bool, bool),
    /// Screenshots and camera photos being written to disk.
    writing: Vec<std::thread::JoinHandle<Result<PathBuf, String>>>,
    last: Instant,
    /// Turns the loop's intervals into the world's step (`pacing`).
    pacer: Pacer,
    /// `VOXEL_TRACE=1`: print each hitch's parts (`trace`).
    trace: bool,
    /// The clock for the menus' pictures and the loading bar.
    t0: Instant,
    grabbed: bool,
    bench: Option<Bench>,
    screen: Screen,
    menu: Menu,
    /// The mouse, in window pixels.
    cursor: (f32, f32),
    prefs: Prefs,
    /// Pictures behind the title screen.
    slides: Slides,
    loading: Option<Loading>,
    /// A short line under the pause menu ("SAVED") and when it was set.
    flash: Option<(String, f32)>,
    /// The colour the LOOK page's picker is on, and the row it was opened
    /// from, so closing it comes back to the same line.
    pick_hsv: [f32; 3],
    look_at: usize,
    /// Which part of the picker the mouse is dragging (0 the square, 1 the hue).
    drag: Option<u8>,
    /// True while the mouse is holding an open dropdown's scrollbar thumb.
    scrolling: bool,
    /// A tuning panel line being dragged on the pause screen: the line, the
    /// cursor's x when it was pressed, and the tune then.
    tune_drag: Option<(usize, f32, crate::vehicle::Spec)>,
    /// Open the EDIT page as soon as the world being loaded is in: what
    /// CREATURES AND PLANTS does from the title screen, where there is no
    /// world to stand a creature in yet.
    edit_on_load: bool,
    /// How far away whatever the cursor is over is, on the LOOK page.
    probe: Option<f32>,
    /// Shown under the title menu after a world failed to load.
    load_error: Option<String>,
    /// The EDIT page: the entry being edited and the studio it stands in
    /// (src/editor). Both are made when the page opens and dropped when it
    /// closes, so nothing of them is left in the world behind it.
    edit: Option<editor::Editor>,
    studio: Option<editor::studio::Studio>,
    /// The line the EDIT page was on when its colour picker, or a dropdown,
    /// opened.
    edit_at: usize,
    /// The EDIT page's camera is being dragged: Some(true) turning around the
    /// subject, Some(false) sliding the view across it.
    cam_drag: Option<bool>,
    /// Where the cursor was last frame while it drags the camera.
    cam_last: (f32, f32),
    /// The launch screen's line a dropdown was opened from, so closing it
    /// comes back to the same place, and how far its list was scrolled.
    launch_at: usize,
    launch_top: usize,
    /// Which world's pictures are behind the menus, if only one world's are
    /// (the launch screen shows the world on the line; the title screen
    /// shows them all).
    slides_world: Option<String>,
    /// The window is hidden (minimized, or covered on a compositor that says
    /// so). Nothing is drawn and the swapchain is left alone until it is back.
    hidden: bool,
    /// Whether the window has the keyboard. Out of focus the loop idles.
    focused: bool,
    quit: bool,
    /// The sound device and the listener's survey of the world (src/sound).
    sound: sound::Sound,
    ear: sound::bed::Ear,
    watch: sound::watch::Watch,
    /// The sound readout (F9): every number the ambience is made of.
    hearing: bool,
}

/// Where a screen goes back to: the title, the pause menu, or straight into
/// the game (the controls opened with H, the world list with L).
#[derive(Clone, Copy, PartialEq)]
enum Back {
    Title,
    Paused,
    Playing,
}

/// What the window is showing. Only Playing runs the game and holds the mouse.
#[derive(Clone, PartialEq)]
enum Screen {
    Title,
    /// The launch screen: which world, which way of playing it, and the
    /// options those two put on offer (src/launch.rs).
    Launch {
        back: Back,
        /// A list standing open on the world (0) or the game (1).
        open: Option<u8>,
    },
    Settings {
        back: Back,
    },
    /// The LOOK page: `page` 0 is the group list, 1-3 a group; `pick` is the
    /// swatch whose colour the picker is on.
    Look {
        page: u8,
        pick: Option<u8>,
        back: Back,
    },
    /// The EDIT page: `page` 0 is the front page, 1 up a group of the
    /// entry's fields; `pick` is the colour the picker is on.
    Edit {
        page: u8,
        pick: Option<u8>,
        /// The row a list is dropped open under (`editor::DROP_*`).
        open: Option<u16>,
        back: Back,
    },
    Help {
        back: Back,
    },
    Loading,
    Playing,
    /// The pause menu over the stopped world (Esc, or the window losing focus).
    Paused,
}

impl Screen {
    /// The menu screens over a world rather than over the title pictures.
    fn over_world(&self) -> bool {
        match self {
            Screen::Playing | Screen::Paused => true,
            Screen::Launch { back, .. } | Screen::Settings { back } | Screen::Look { back, .. } | Screen::Edit { back, .. } | Screen::Help { back } => *back != Back::Title,
            Screen::Title | Screen::Loading => false,
        }
    }
}

/// A world on its way in: built on a worker thread, then streamed around the
/// spawn point before play starts.
struct Loading {
    name: String,
    job: Option<std::thread::JoinHandle<Result<Game, String>>>,
    game: Option<Game>,
    /// When streaming started.
    streaming: Option<Instant>,
    progress: f32,
    slides: Slides,
    /// Use the command line's position and look (the first world from `--world`).
    from_args: bool,
}

/// Longest to wait for the view to fill before starting anyway.
const STREAM_WAIT: f32 = 30.0;

struct App {
    args: Args,
    run: Option<Running>,
}

/// The window's monitor's refresh interval in seconds, if it reports one.
fn refresh_interval(window: &Window) -> Option<f32> {
    let mhz = window.current_monitor()?.refresh_rate_millihertz()?;
    (mhz > 0).then(|| 1000.0 / mhz as f32)
}

/// How many frames the swapchain may queue ahead of the display. Under vsync
/// each queued frame is another refresh between moving the mouse and seeing
/// it, and the GPU frame is a few milliseconds, so one is enough. Without
/// vsync it needs three: with two, mailbox frames on the RTX 3060 at 60 Hz
/// came in uneven pairs and averaged 120 to 190 fps; with three they ran an
/// even 260 (field route, 1080p, shadows on).
/// `VOXEL_FRAME_LATENCY=N` overrides it for comparisons.
fn frame_latency(mode: wgpu::PresentMode) -> u32 {
    if let Some(n) = std::env::var("VOXEL_FRAME_LATENCY").ok().and_then(|v| v.parse().ok()) {
        return n;
    }
    if mode == wgpu::PresentMode::Fifo {
        1
    } else {
        3
    }
}

fn present_mode(modes: &[wgpu::PresentMode], vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::Fifo
    } else if modes.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else if modes.contains(&wgpu::PresentMode::Immediate) {
        wgpu::PresentMode::Immediate
    } else {
        wgpu::PresentMode::Fifo
    }
}

impl Running {
    fn now(&self) -> f32 {
        self.t0.elapsed().as_secs_f32()
    }

    fn grab(&mut self, on: bool) {
        if on {
            let ok = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            self.window.set_cursor_visible(!ok);
            self.grabbed = ok;
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
            self.grabbed = false;
            self.input = MoveInput::default();
            self.held = (false, false);
            self.actions.dig = false;
            self.actions.place = false;
        }
    }

    /// Change screens: the mouse is held only while playing.
    fn set_screen(&mut self, s: Screen) {
        self.grab(s == Screen::Playing);
        if s != self.screen {
            self.menu = Menu::default();
        }
        self.screen = s;
    }

    fn save_prefs(&self, args: &Args) {
        self.prefs.save(&args.save_dir);
    }

    /// Copy the settings the in-game keys change (G, T, B, F3) into the prefs.
    fn prefs_from_game(&mut self, args: &Args) {
        if let Some(g) = &self.game {
            if self.prefs.settings != g.settings {
                self.prefs.settings = g.settings;
                self.save_prefs(args);
            }
        }
    }

    fn set_fullscreen(&mut self, on: bool) {
        self.window.set_fullscreen(if on { Some(Fullscreen::Borderless(None)) } else { None });
    }

    /// Put a changed setting into effect.
    fn apply(&mut self, s: Setting) {
        let p = self.prefs.clone();
        match s {
            Setting::Fullscreen => self.set_fullscreen(p.fullscreen),
            Setting::Vsync => {
                self.config.present_mode = present_mode(&self.present_modes, p.vsync);
                self.config.desired_maximum_frame_latency = frame_latency(self.config.present_mode);
                self.surface.configure(&self.gpu.device, &self.config);
            }
            Setting::View | Setting::Msaa => {}
            Setting::Sound | Setting::Volume | Setting::Ambience | Setting::Effects | Setting::Music => self.sound.set_levels(p.sound),
            Setting::Style | Setting::Shadows | Setting::Slopes | Setting::Stats => {
                if let Some(g) = &mut self.game {
                    let resmooth = g.settings.slopes != p.settings.slopes;
                    g.settings = p.settings;
                    if resmooth {
                        let pos = g.player.pos;
                        g.streamer.set_smooth(p.settings.slopes, pos);
                    }
                }
            }
        }
    }

    /// Start loading a world as the launch screen says: the current one is
    /// saved and let go first.
    fn start_load(&mut self, args: &Args, chosen: launch::Chosen, from_args: bool) {
        let name = chosen.world.clone();
        let name = name.as_str();
        if let Some(mut g) = self.game.take() {
            g.save();
        }
        self.bench = None;
        self.load_error = None;
        worldload::select(name);
        self.prefs.last.world = name.to_string();
        self.prefs.last.game = chosen.game.clone();
        self.prefs.last.opened(name);
        self.save_prefs(args);
        let mut a = args.clone();
        a.launch = Some(chosen);
        if !from_args {
            a.pos = None;
            a.ground = None;
            a.look = None;
            a.fresh = false;
        }
        a.style = Some(self.prefs.settings.style);
        a.shadows = self.prefs.settings.shadows;
        a.blocky = !self.prefs.settings.slopes;
        a.view = self.prefs.view;
        let job = std::thread::Builder::new().name("load world".into()).spawn(move || new_game(&a)).expect("load thread");
        let mut pics = menu::picture_paths(Some(name));
        if pics.is_empty() {
            pics = menu::picture_paths(None);
        }
        let seed = self.t0.elapsed().as_nanos() as u64;
        self.loading = Some(Loading {
            name: name.to_string(),
            job: Some(job),
            game: None,
            streaming: None,
            progress: 0.0,
            slides: Slides::new(pics, seed),
            from_args,
        });
        self.set_screen(Screen::Loading);
    }

    /// One frame of loading: take the built world, then stream until the view
    /// around the player is in.
    fn loading_step(&mut self, args: &Args) {
        let Some(l) = &mut self.loading else { return };
        if l.job.as_ref().is_some_and(|j| j.is_finished()) {
            match l.job.take().unwrap().join() {
                Ok(Ok(mut game)) => {
                    game.gpu_name = self.gpu_name.clone();
                    game.settings = self.prefs.settings;
                    show_world(&mut self.renderer, &game);
                    l.game = Some(game);
                    l.streaming = Some(Instant::now());
                }
                failed => {
                    let why = match failed {
                        Ok(Err(e)) => e,
                        _ => "the loader crashed".to_string(),
                    };
                    eprintln!("loading {} failed: {why}", l.name);
                    self.load_error = Some(format!("COULD NOT LOAD {} (DETAILS IN THE CONSOLE)", menu::world_title(&l.name)));
                    self.loading = None;
                    self.set_screen(Screen::Title);
                    return;
                }
            }
        }
        let (Some(g), Some(since)) = (&mut l.game, l.streaming) else {
            return;
        };
        let input = MoveInput::default();
        g.stream(&mut self.renderer.terrain, Duration::from_millis(12));
        g.tick(0.0, &input, &mut self.actions);
        let st = &g.streamer.stats;
        let p = st.columns as f32 / (st.columns + st.queued + st.busy).max(1) as f32;
        l.progress = l.progress.max(p.min(0.99));
        let waited = since.elapsed().as_secs_f32();
        if (g.spawned && g.streamer.idle() && waited > 0.2) || waited > STREAM_WAIT {
            let l = self.loading.take().unwrap();
            let game = l.game.unwrap();
            if args.bench {
                self.bench = Some(Bench::new(game.player.pos + Vec3::Y * 2.0, args.bench_route));
            }
            self.game = Some(game);
            self.last = Instant::now();
            // The window may not have focus (or the cursor may be far away):
            // a world opened from the command line waits on the pause menu.
            let play = !l.from_args || self.window.has_focus();
            self.set_screen(if play { Screen::Playing } else { Screen::Paused });
            // --edit opens the page on a named creature or plant as soon as
            // there is a world to stand it in.
            let want = args.edit.clone().or_else(|| std::mem::take(&mut self.edit_on_load).then(String::new));
            if let Some(want) = want {
                // NAME[:SECTION] -- the heading to open on its own, so a
                // screenshot can show one group rather than the whole form.
                let (want, sec) = match want.split_once(':') {
                    Some((n, rest)) => (n.to_string(), rest.split(':').next().and_then(|v| v.parse::<usize>().ok())),
                    None => (want, None),
                };
                let name = (!want.is_empty()).then_some(want);
                self.open_edit(name.as_deref());
                if let (Some(e), Some(sec)) = (self.edit.as_mut(), sec) {
                    if sec > 0 {
                        e.only_section(sec);
                    }
                }
                if self.edit.is_some() {
                    self.menu.at = 0;
                    self.set_screen(Screen::Edit {
                        page: 0,
                        pick: None,
                        open: None,
                        back: Back::Playing,
                    });
                }
            }
        }
    }

    /// The lines of the menu on screen now.
    fn items(&self) -> Vec<menu::Item> {
        match &self.screen {
            Screen::Title => menu::title_items(&self.prefs),
            Screen::Paused => menu::pause_items(),
            Screen::Launch { open: Some(which), .. } => menu::drop_items(&self.prefs.last, *which),
            Screen::Launch { .. } => {
                let here = self.game.as_ref().map(|_| worldload::selected());
                menu::launch_items(&self.prefs.last, here.as_deref())
            }
            Screen::Settings { .. } => menu::settings_items(&self.prefs, self.game.is_some(), self.renderer.msaa()),
            Screen::Look { page, pick, .. } => match pick {
                Some(code) => {
                    let g = self.renderer.grade_of(self.style_now());
                    lookpage::pick_items(self.pick_hsv, lookpage::stop_distance(g, *code))
                }
                None => {
                    let style = self.style_now();
                    lookpage::items(*page, &self.renderer.look_of(style), self.renderer.grade_of(style))
                }
            },
            Screen::Edit { pick, open, .. } => match (self.edit.as_ref(), pick, open) {
                (Some(_), Some(_), _) => lookpage::pick_items(self.pick_hsv, None),
                (Some(e), None, Some(which)) => match self.game.as_ref().and_then(|g| g.library()) {
                    Some(lib) => e.drop_items(*which, lib),
                    None => Vec::new(),
                },
                (Some(e), None, None) => e.items(),
                (None, _, _) => Vec::new(),
            },
            Screen::Help { .. } | Screen::Loading | Screen::Playing => Vec::new(),
        }
    }

    fn back_from(&self) -> Back {
        match &self.screen {
            Screen::Title => Back::Title,
            Screen::Playing => Back::Playing,
            Screen::Launch { back, .. } | Screen::Settings { back } | Screen::Look { back, .. } | Screen::Edit { back, .. } | Screen::Help { back } => *back,
            _ => Back::Paused,
        }
    }

    /// Shut an open dropdown, back onto the line it hangs from.
    fn close_drop(&mut self) {
        if let Screen::Launch { back, open: Some(_) } = self.screen {
            self.set_screen(Screen::Launch { back, open: None });
            self.menu.at = self.launch_at;
            self.menu.top = self.launch_top;
            self.menu.shown = 0;
        }
    }

    fn go_back(&mut self) {
        // Closing the EDIT page drops the entry as it stands, so edits that
        // were never written are gone. Say so once and wait for a second
        // press: the page used to shut on the first Esc and take an
        // afternoon's tuning with it.
        if matches!(self.screen, Screen::Edit { page: 0, pick: None, open: None, .. }) {
            let asked = match self.edit.as_mut() {
                Some(e) if e.dirty && !e.asked_discard => {
                    e.asked_discard = true;
                    let m = "UNSAVED EDITS -- WRITE, OR CLOSE AGAIN TO DISCARD".to_string();
                    e.msg = Some(m.clone());
                    Some(m)
                }
                _ => None,
            };
            if let Some(m) = asked {
                self.note_flash(m);
                return;
            }
        }
        let to = match &self.screen {
            // An open list closes onto the line it hangs from, not the
            // screen behind it.
            Screen::Launch { open: Some(_), .. } => {
                self.close_drop();
                return;
            }
            Screen::Paused => Screen::Playing,
            Screen::Title => return,
            // The picker closes onto the line it was opened from; a group of
            // the LOOK page closes onto the group list.
            Screen::Look { page, pick: Some(_), back } => {
                let to = Screen::Look { page: *page, pick: None, back: *back };
                self.set_screen(to);
                self.menu.at = self.look_at;
                return;
            }
            Screen::Look { page, back, .. } if *page != 0 => Screen::Look { page: 0, pick: None, back: *back },
            // The EDIT page's picker closes onto the colour it opened from,
            // a group onto the front page, and the front page shuts the
            // studio down and puts the world back as it was.
            // A list closes onto the row it dropped from.
            Screen::Edit { page, pick, open: Some(_), back } => {
                let to = Screen::Edit {
                    page: *page,
                    pick: *pick,
                    open: None,
                    back: *back,
                };
                self.set_screen(to);
                self.menu.at = self.edit_at;
                self.menu.top = 0;
                self.menu.shown = 0;
                return;
            }
            Screen::Edit {
                page,
                pick: Some(_),
                open: None,
                back,
            } => {
                let to = Screen::Edit {
                    page: *page,
                    pick: None,
                    open: None,
                    back: *back,
                };
                self.set_screen(to);
                self.menu.at = self.edit_at;
                return;
            }
            Screen::Edit { page, back, .. } if *page != 0 => Screen::Edit {
                page: 0,
                pick: None,
                open: None,
                back: *back,
            },
            Screen::Edit { back, .. } => {
                let to = match *back {
                    Back::Title => Screen::Title,
                    Back::Paused => Screen::Paused,
                    Back::Playing => Screen::Playing,
                };
                self.close_edit();
                self.set_screen(to);
                return;
            }
            _ => match self.back_from() {
                Back::Title => Screen::Title,
                Back::Paused => Screen::Paused,
                Back::Playing => Screen::Playing,
            },
        };
        self.set_screen(to);
    }

    /// The style whose look the LOOK page edits: the world the player is in,
    /// or the one a world would load with.
    fn style_now(&self) -> render::Style {
        self.game.as_ref().map_or(self.prefs.settings.style, |g| g.settings.style)
    }

    /// The world on screen, if any: where its own look.json lives.
    fn world_dir(&self) -> Option<std::path::PathBuf> {
        let g = self.game.as_ref()?;
        g.generator().world.as_ref().map(|w| w.dir.clone())
    }

    /// The grading this world keeps for itself, if any.
    fn world_look(&self) -> Option<library::WorldLook> {
        let g = self.game.as_ref()?;
        g.generator().world.as_ref().and_then(|w| w.look.clone())
    }

    fn note_flash(&mut self, m: String) {
        self.flash = Some((m, self.t0.elapsed().as_secs_f32()));
    }

    /// Put the picker hue, saturation and value onto the colour it was opened
    /// for.
    fn apply_pick(&mut self) {
        if let Screen::Edit { page, pick: Some(i), .. } = self.screen {
            let c = lookpage::from_hsv(self.pick_hsv);
            if let Some(e) = self.edit.as_mut() {
                e.set_colour(page, i as usize, c);
            }
            return;
        }
        let Screen::Look { pick: Some(code), .. } = self.screen else {
            return;
        };
        let c = lookpage::from_hsv(self.pick_hsv);
        let style = self.style_now();
        let (look, grade) = self.renderer.look_and_grade_mut(style);
        lookpage::swatch_set(look, grade, code, c);
    }

    fn cursor_probe(&self) -> Option<f32> {
        let g = self.game.as_ref()?;
        lookpage::probe(g, self.cursor, self.config.width as f32, self.config.height as f32)
    }

    /// Which part of the picker the cursor is over.
    fn pick_region(&self) -> Option<u8> {
        if !matches!(self.screen, Screen::Look { pick: Some(_), .. } | Screen::Edit { pick: Some(_), .. }) {
            return None;
        }
        let (sq, hb, sl) = lookpage::picker_rects(self.config.width as f32, self.config.height as f32);
        let (x, y) = self.cursor;
        let inside = |r: [f32; 4]| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
        if inside(sq) {
            Some(0)
        } else if inside(hb) {
            Some(1)
        } else if inside(sl) && self.pick_has_distance() {
            Some(2)
        } else {
            None
        }
    }

    /// True when the picker is open on a stop that has a distance.
    fn pick_has_distance(&self) -> bool {
        let Screen::Look { pick: Some(code), .. } = self.screen else {
            return false;
        };
        lookpage::stop_distance(self.renderer.grade_of(self.style_now()), code).is_some()
    }

    /// Follow a stop that changed place, so a drag keeps hold of it.
    fn set_pick_code(&mut self, code: u8) {
        if let Screen::Look { page, back, pick: Some(old) } = self.screen {
            if old != code {
                self.screen = Screen::Look { page, back, pick: Some(code) };
            }
        }
    }

    /// Read a colour straight off the picker square or hue strip.
    fn pick_drag(&mut self, which: u8) {
        let (sq, hb, sl) = lookpage::picker_rects(self.config.width as f32, self.config.height as f32);
        let (x, y) = self.cursor;
        match which {
            0 => {
                self.pick_hsv[1] = ((x - sq[0]) / sq[2]).clamp(0.0, 1.0);
                self.pick_hsv[2] = (1.0 - (y - sq[1]) / sq[3]).clamp(0.0, 1.0) * lookpage::V_MAX;
            }
            1 => self.pick_hsv[0] = ((x - hb[0]) / hb[2]).clamp(0.0, 1.0) * 360.0,
            _ => {
                let Screen::Look { pick: Some(code), .. } = self.screen else {
                    return;
                };
                let style = self.style_now();
                let moved = {
                    let (_, grade) = self.renderer.look_and_grade_mut(style);
                    let far = lookpage::slider_far(grade);
                    let d = ((x - sl[0]) / sl[2]).clamp(0.0, 1.0) * far;
                    lookpage::set_stop_distance(grade, code, d)
                };
                self.set_pick_code(moved);
                return;
            }
        }
        self.apply_pick();
    }

    /// `dir` is -1 or +1 to step a value, and 0 to open or do the line.
    fn choose(&mut self, args: &Args, c: Choice, dir: i32) {
        let back = self.back_from();
        match c {
            Choice::Continue => {
                if !worldload::available().contains(&self.prefs.last.world) {
                    self.prefs.last.world = worldload::DEFAULT_WORLD.to_string();
                }
                let chosen = self.prefs.last.resolve();
                self.start_load(args, chosen, false);
            }
            Choice::Open => {
                // settings.json may name a game this world no longer offers.
                self.prefs.last.settle();
                self.set_screen(Screen::Launch { back, open: None })
            }
            // A click or Enter opens the list; the arrow keys still step the
            // value without it, which is quicker once you know the worlds.
            Choice::LaunchWorld | Choice::LaunchMode => {
                let world = c == Choice::LaunchWorld;
                if dir == 0 {
                    // The row the list hangs from: the game is the first.
                    let which = u8::from(world);
                    self.launch_at = self.menu.at;
                    self.launch_top = self.menu.top;
                    if let Screen::Launch { back, .. } = self.screen {
                        self.set_screen(Screen::Launch { back, open: Some(which) });
                    }
                    self.menu.at = menu::drop_at(&self.prefs.last, which);
                    // Open on the choice in force rather than at the top of
                    // a list it may be a long way down.
                    self.menu.top = self.menu.at.saturating_sub(2);
                } else if world {
                    self.prefs.last.step_world(dir);
                } else {
                    self.prefs.last.step_mode(dir);
                }
            }
            Choice::LaunchReset => self.prefs.last.reset(),
            Choice::ChooseWorld(i) => {
                let list = self.prefs.last.world_rows();
                if let Some(name) = list.get(i as usize).map(|r| r.world.clone()) {
                    self.prefs.last.set_world(&name);
                }
                self.close_drop();
            }
            Choice::ChooseMode(i) => {
                let list = launch::all_games();
                if let Some(g) = list.get(i as usize) {
                    self.prefs.last.set_game(&g.name);
                }
                self.close_drop();
            }
            Choice::LaunchOpt(i) => {
                let opts = launch::options(&self.prefs.last.world, self.prefs.last.def());
                if let Some(o) = opts.get(i as usize) {
                    self.prefs.last.step(o, dir);
                }
            }
            Choice::Start => {
                let chosen = self.prefs.last.resolve();
                // Already in that world, with the same rules? Just go back to it.
                if self.game.is_some() && chosen.world == worldload::selected() && self.game.as_ref().is_some_and(|g| g.launch.same_run(&chosen)) {
                    self.set_screen(Screen::Playing);
                } else {
                    self.start_load(args, chosen, false);
                }
            }
            Choice::Settings => self.set_screen(Screen::Settings { back }),
            Choice::Controls => self.set_screen(Screen::Help { back }),
            Choice::Quit => self.quit = true,
            Choice::Resume => self.set_screen(Screen::Playing),
            Choice::Save => {
                if let Some(g) = &mut self.game {
                    g.save();
                    self.flash = Some(("SAVED".to_string(), self.t0.elapsed().as_secs_f32()));
                }
            }
            Choice::MainMenu => {
                self.prefs_from_game(args);
                if let Some(mut g) = self.game.take() {
                    g.save();
                }
                self.bench = None;
                self.set_screen(Screen::Title);
            }
            Choice::Set(s) => {
                menu::step(&mut self.prefs, s, if dir == 0 { 1 } else { dir });
                self.apply(s);
                self.save_prefs(args);
            }
            Choice::Back => self.go_back(),
            Choice::Look(page) => {
                self.look_at = 0;
                self.set_screen(Screen::Look { page, pick: None, back });
            }
            Choice::Knob(i) => {
                let style = self.style_now();
                let (look, grade) = self.renderer.look_and_grade_mut(style);
                lookpage::step_knob(look, grade, i, if dir == 0 { 1 } else { dir });
            }
            Choice::Stop(code) => {
                let style = self.style_now();
                if dir == 0 {
                    let c = {
                        let (look, grade) = self.renderer.look_and_grade_mut(style);
                        lookpage::swatch_get(look, grade, code)
                    };
                    self.pick_hsv = lookpage::to_hsv(c);
                    self.look_at = self.menu.at;
                    if let Screen::Look { page, back, .. } = self.screen {
                        self.set_screen(Screen::Look { page, pick: Some(code), back });
                    }
                } else {
                    let (_, grade) = self.renderer.look_and_grade_mut(style);
                    let moved = lookpage::move_stop(grade, code, dir);
                    // The stop may have passed a neighbour; follow its row.
                    self.menu.at = (self.menu.at as i32 + moved as i32 - code as i32).max(0) as usize;
                }
            }
            Choice::AddStop(base) => {
                let style = self.style_now();
                let (_, grade) = self.renderer.look_and_grade_mut(style);
                let row = lookpage::strip_of(base);
                let gap = lookpage::widest_gap(grade, row);
                grade.add_stop(row, gap);
            }
            Choice::DelStop(base) => {
                let style = self.style_now();
                let (_, grade) = self.renderer.look_and_grade_mut(style);
                let row = lookpage::strip_of(base);
                let i = lookpage::middle_stop(grade, row);
                grade.remove_stop(row, i);
            }
            Choice::Pick(3) => {
                // The distance row steps the stop itself, in metres.
                let Screen::Look { pick: Some(code), .. } = self.screen else {
                    return;
                };
                if dir != 0 {
                    let style = self.style_now();
                    let moved = {
                        let (_, grade) = self.renderer.look_and_grade_mut(style);
                        lookpage::move_stop(grade, code, dir)
                    };
                    self.set_pick_code(moved);
                }
            }
            Choice::Pick(which) => {
                if dir != 0 {
                    lookpage::step_pick(&mut self.pick_hsv, which, dir);
                    self.apply_pick();
                }
            }
            Choice::PickDone => self.go_back(),
            Choice::Export => {
                let style = self.style_now();
                let look = self.renderer.look_of(style);
                let world = self.world_look();
                let m = match lookpage::export(style, &look, self.renderer.grade_of(style), world.as_ref()) {
                    Ok((p, 0)) => format!("WRITTEN TO {}", p.display()),
                    Ok((p, n)) => format!("WRITTEN TO {}; {n} LEFT TO THIS WORLD", p.display()),
                    Err(e) => format!("NOT WRITTEN: {e}"),
                };
                self.note_flash(m);
            }
            Choice::ExportWorld(which) => {
                let style = self.style_now();
                let look = self.renderer.look_of(style);
                let m = match self.world_dir() {
                    Some(dir) => match lookpage::export_world(&dir, style, &look, self.renderer.grade_of(style), which == 0) {
                        Ok(p) => format!("WRITTEN TO {}", p.display()),
                        Err(e) => format!("NOT WRITTEN: {e}"),
                    },
                    None => "NO WORLD TO WRITE TO".to_string(),
                };
                self.note_flash(m);
            }
            Choice::OpenEdit => {
                // The studio needs a world: it stands its subject on that
                // world's own ground, and the library it edits is the one
                // the world loaded. From the title screen there is none, so
                // the last world is opened and the page comes up on top of
                // it.
                if self.game.is_some() {
                    self.open_edit(None);
                    if self.edit.is_some() {
                        self.menu.at = 0;
                        self.set_screen(Screen::Edit {
                            page: 0,
                            pick: None,
                            open: None,
                            back: Back::Playing,
                        });
                    }
                } else {
                    if !worldload::available().contains(&self.prefs.last.world) {
                        self.prefs.last.world = worldload::DEFAULT_WORLD.to_string();
                    }
                    // Always as a world to walk, whatever was last played:
                    // the page opened in the middle of a deathmatch, with
                    // bots shooting at the studio.
                    let mut chosen = self.prefs.last.resolve();
                    chosen.set_game(games::base(launch::Mode::Explore));
                    self.edit_on_load = true;
                    self.start_load(args, chosen, false);
                }
            }
            Choice::EditPage(sec) => {
                // Opened from a menu with no page up, this is still "open the
                // EDIT page"; on the page itself it opens and shuts a heading.
                if self.edit.is_none() {
                    self.open_edit(None);
                    if self.edit.is_some() {
                        self.menu.at = 0;
                        self.set_screen(Screen::Edit {
                            page: 0,
                            pick: None,
                            open: None,
                            back,
                        });
                    }
                    return;
                }
                if let Some(e) = self.edit.as_mut() {
                    e.toggle_section(sec as usize);
                }
            }
            Choice::EditChoose(i) => {
                let Some(which) = (match self.screen {
                    Screen::Edit { open, .. } => open,
                    _ => None,
                }) else {
                    return;
                };
                if let Some(mut e) = self.edit.take() {
                    if let Some(lib) = self.game.as_ref().and_then(|g| g.library()) {
                        e.take(which, i as usize, lib);
                    }
                    self.edit = Some(e);
                }
                self.close_edit_drop();
            }
            Choice::EditKind if dir == 0 => self.open_edit_drop(editor::DROP_KIND),
            Choice::EditEntry if dir == 0 => self.open_edit_drop(editor::DROP_ENTRY),
            Choice::EditKnob(i) if dir == 0 && self.edit.as_ref().is_some_and(|e| e.has_drop(editor::DROP_KNOB + i as u16)) => self.open_edit_drop(editor::DROP_KNOB + i as u16),
            Choice::EditKind => {
                let Some(e) = self.edit.as_ref() else { return };
                let kind = e.kind.other();
                let Some(lib) = self.game.as_ref().and_then(|g| g.library()) else { return };
                // The library lives in the loaded world; take what the move
                // needs and let go of it.
                let mut e = self.edit.take().unwrap();
                e.open(kind, 0, lib);
                self.edit = Some(e);
            }
            Choice::EditEntry => {
                let Some(e) = self.edit.as_ref() else { return };
                let n = e.names.len();
                let step = if dir == 0 { 1 } else { dir };
                let at = (e.at as i32 + step).rem_euclid(n as i32) as usize;
                let kind = e.kind;
                let Some(lib) = self.game.as_ref().and_then(|g| g.library()) else { return };
                let mut e = self.edit.take().unwrap();
                e.open(kind, at, lib);
                self.edit = Some(e);
            }
            Choice::EditRegen => {
                if let Some(e) = self.edit.as_mut() {
                    e.regen();
                }
            }
            Choice::EditAge => {
                if let Some(e) = self.edit.as_mut() {
                    use crate::creatures::age::Stage;
                    const AGES: [Stage; 4] = [Stage::Juvenile, Stage::Adult, Stage::Aged, Stage::Dead];
                    let at = AGES.iter().position(|s| *s == e.stage).unwrap_or(1) as i32;
                    let step = if dir == 0 { 1 } else { dir };
                    e.stage = AGES[(at + step).rem_euclid(4) as usize];
                    // A different age is a different dressing of the same
                    // animal, not a different animal.
                    e.wants(editor::Impact::Row);
                }
            }
            Choice::EditPose if dir == 0 => self.open_edit_drop(editor::DROP_POSE),
            Choice::EditPose => {
                if let Some(e) = self.edit.as_mut() {
                    let all = e.poses();
                    if !all.is_empty() {
                        let at = all.iter().position(|p| *p == e.pose).unwrap_or(0) as i32;
                        e.pose = all[(at + dir).rem_euclid(all.len() as i32) as usize];
                        e.wants(editor::Impact::Row);
                    }
                }
            }
            Choice::EditGrid => {
                if let Some(e) = self.edit.as_mut() {
                    e.grid = !e.grid;
                    // The platform is part of the generated ground, so the
                    // plant or the block standing on it goes in again with it.
                    e.wants(editor::Impact::Whole);
                }
                self.edit_floor();
            }
            Choice::EditLight => {
                if let Some(e) = self.edit.as_mut() {
                    e.world_light = !e.world_light;
                    if let Some(g) = self.game.as_mut() {
                        g.studio_light = !e.world_light;
                    }
                    e.wants(editor::Impact::Row);
                }
            }
            Choice::EditKnob(i) => {
                let Some(mut e) = self.edit.take() else { return };
                if let Some(lib) = self.game.as_ref().and_then(|g| g.library()) {
                    e.step(i as usize, if dir == 0 { 1 } else { dir }, lib);
                }
                self.edit = Some(e);
            }
            Choice::EditColour(sec, i) => {
                let Some(e) = self.edit.as_ref() else { return };
                self.pick_hsv = lookpage::to_hsv(e.colour(sec, i as usize));
                self.edit_at = self.menu.at;
                if let Screen::Edit { back, .. } = self.screen {
                    // The heading the colour sits under is kept in `page`, so
                    // the picker knows which row it is painting.
                    self.set_screen(Screen::Edit {
                        page: sec,
                        pick: Some(i),
                        open: None,
                        back,
                    });
                }
            }
            Choice::EditExport => {
                let m = match self.edit.as_mut().map(|e| e.export()) {
                    Some(Ok(p)) => format!("WRITTEN TO {}", p.display()),
                    Some(Err(e)) => format!("NOT WRITTEN: {e}"),
                    None => "NOTHING OPEN".to_string(),
                };
                if let Some(e) = self.edit.as_mut() {
                    e.msg = Some(m.clone());
                }
                self.note_flash(m);
            }
            Choice::EditReload => {
                if let Some(e) = self.edit.as_mut() {
                    let m = match e.read() {
                        Ok(()) => "READ THE FILE AGAIN".to_string(),
                        Err(x) => format!("NOT READ: {x}"),
                    };
                    e.msg = Some(m);
                    e.wants(editor::Impact::Whole);
                }
            }
            Choice::ResetLook => {
                let style = self.style_now();
                let world = self.world_look();
                match lookpage::from_file(style, world.as_ref()) {
                    Some((l, g)) => {
                        self.renderer.set_look(style, l);
                        self.renderer.set_grade(style, g);
                        self.note_flash("READ THE LOOK FILE AGAIN".to_string());
                    }
                    None => self.note_flash("NO LOOK FILE TO READ".to_string()),
                }
            }
        }
    }

    /// Keep the EDIT page's highlight inside its scrolling body.
    fn edit_reveal(&mut self) {
        let at = self.menu.at;
        if let Some(e) = self.edit.as_mut() {
            e.reveal(at);
        }
    }

    /// The aspect the EDIT page's camera frames for.
    fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    /// Whether the cursor is out over the studio rather than on the entry's
    /// list, which takes the right 44 units of the screen
    /// (`form::BAND`). Drags and the wheel belong to the camera on
    /// one side of that line and to the list on the other.
    fn on_studio(&self) -> bool {
        let (w, h) = (self.config.width as f32, self.config.height as f32);
        matches!(self.screen, Screen::Edit { open: None, pick: None, .. }) && self.cursor.0 < w - 44.0 * menu::unit(w, h)
    }

    /// A key the EDIT page's camera takes. The page is a studio you stand in
    /// rather than a menu, so WASD turns and zooms the view; the arrows are
    /// what walk the list.
    fn edit_cam_key(&mut self, code: KeyCode) -> bool {
        if !matches!(self.screen, Screen::Edit { open: None, pick: None, .. }) {
            return false;
        }
        let aspect = self.aspect();
        let Some(st) = self.studio.as_mut() else { return false };
        use editor::cam::KEY_TURN;
        match code {
            KeyCode::KeyA => st.cam.turn(-KEY_TURN, 0.0),
            KeyCode::KeyD => st.cam.turn(KEY_TURN, 0.0),
            KeyCode::KeyW => st.cam.dolly(1.0),
            KeyCode::KeyS => st.cam.dolly(-1.0),
            KeyCode::KeyQ => st.cam.turn(0.0, KEY_TURN),
            KeyCode::KeyE => st.cam.turn(0.0, -KEY_TURN),
            KeyCode::KeyF => st.reframe(aspect),
            KeyCode::KeyT => st.cam.spinning = !st.cam.spinning,
            _ => return false,
        }
        true
    }

    /// A key on a menu screen.
    fn menu_key(&mut self, args: &Args, code: KeyCode) {
        if self.edit_cam_key(code) {
            return;
        }
        if matches!(self.screen, Screen::Help { .. }) {
            if matches!(code, KeyCode::Escape | KeyCode::KeyH | KeyCode::F1 | KeyCode::Enter | KeyCode::Space) {
                self.go_back();
            }
            return;
        }
        let items = self.items();
        let n = items.len();
        let at = self.menu.at.min(n.saturating_sub(1));
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => {
                self.menu.nav(n, false);
                self.edit_reveal();
            }
            KeyCode::KeyS | KeyCode::ArrowDown => {
                self.menu.nav(n, true);
                self.edit_reveal();
            }
            // The ends of a list, and a screenful at a time: the world list
            // is long enough to want them.
            KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown if n > 0 => {
                let page = self.menu.shown.saturating_sub(1).max(4);
                self.menu.at = match code {
                    KeyCode::Home => 0,
                    KeyCode::End => n - 1,
                    KeyCode::PageUp => at.saturating_sub(page),
                    _ => (at + page).min(n - 1),
                };
                self.menu.reveal(n);
                self.edit_reveal();
            }
            KeyCode::KeyA | KeyCode::ArrowLeft | KeyCode::KeyD | KeyCode::ArrowRight => {
                if let Some(it) = items.get(at) {
                    if matches!(
                        it.choice,
                        Choice::Set(_)
                            | Choice::Knob(_)
                            | Choice::Stop(_)
                            | Choice::Pick(_)
                            | Choice::LaunchWorld
                            | Choice::LaunchMode
                            | Choice::LaunchOpt(_)
                            | Choice::EditKnob(_)
                            | Choice::EditKind
                            | Choice::EditEntry
                            | Choice::EditAge
                            | Choice::EditPose
                            | Choice::EditGrid
                            | Choice::EditLight
                    ) {
                        let dir = if matches!(code, KeyCode::KeyA | KeyCode::ArrowLeft) { -1 } else { 1 };
                        self.choose(args, it.choice, dir);
                    }
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(it) = items.get(at) {
                    self.choose(args, it.choice, 0);
                }
            }
            KeyCode::Escape => self.go_back(),
            KeyCode::KeyL if matches!(self.screen, Screen::Launch { back, open: None } if back != Back::Title) => self.go_back(),
            _ => {}
        }
    }

    /// A click on a menu screen: choose the line under the mouse (a right
    /// click steps a setting back).
    fn menu_click(&mut self, args: &Args, right: bool) {
        if matches!(self.screen, Screen::Help { .. }) {
            self.go_back();
            return;
        }
        let Some(i) = self.menu.hit(self.cursor.0, self.cursor.1) else {
            // A click off an open list shuts it, as a dropdown should.
            self.close_drop();
            return;
        };
        let items = self.items();
        if let Some(it) = items.get(i) {
            self.menu.at = i;
            let steps = matches!(
                it.choice,
                Choice::Set(_)
                    | Choice::Knob(_)
                    | Choice::Stop(_)
                    | Choice::Pick(_)
                    | Choice::LaunchWorld
                    | Choice::LaunchMode
                    | Choice::LaunchOpt(_)
                    | Choice::EditKnob(_)
                    | Choice::EditKind
                    | Choice::EditEntry
                    | Choice::EditAge
                    | Choice::EditPose
                    | Choice::EditGrid
                    | Choice::EditLight
            );
            if right && !steps {
                return;
            }
            // A left click opens or does the line; on a number it nudges it
            // up, unless it landed on the left arrow, and a right click always
            // nudges down.
            let arrow = self.menu.on_arrow(i, self.cursor.0, self.cursor.1);
            let dir = if right || arrow < 0 {
                -1
            } else if arrow > 0 {
                1
            } else if matches!(it.choice, Choice::Set(_) | Choice::Knob(_) | Choice::Pick(_) | Choice::LaunchOpt(_) | Choice::EditKnob(_)) {
                1
            } else {
                0
            };
            self.choose(args, it.choice, dir);
        }
    }

    /// What on the tuning panel the cursor is over, on the pause screen.
    fn tune_hover(&self) -> Option<game::TuneHit> {
        let g = self.game.as_ref()?;
        g.tune_hit(self.config.width as f32, self.config.height as f32, self.cursor.0, self.cursor.1)
    }

    /// A mouse press on the pause screen's tuning panel: left on a line
    /// starts a drag (a choice flips), right puts the line back to the
    /// file's value, a click on the bottom line saves. False when the press
    /// was not on the panel.
    fn tune_press(&mut self, button: MouseButton) -> bool {
        let Some(hit) = self.tune_hover() else { return false };
        let t = self.t0.elapsed().as_secs_f32();
        let x = self.cursor.0;
        let Some(g) = self.game.as_mut() else { return false };
        match (hit, button) {
            (game::TuneHit::Line(i), MouseButton::Left) => {
                let choice = matches!(crate::vehicle::tune::KNOBS[i].key, "steering" | "driven");
                if choice {
                    g.ride.tune_step(i, 1.0);
                } else if let Some(base) = g.ride.tune_spec() {
                    g.ride.tune_apply(i, &base, 0.0);
                    self.tune_drag = Some((i, x, base));
                }
            }
            (game::TuneHit::Line(i), MouseButton::Right) => g.ride.tune_reset(i),
            (game::TuneHit::Save, MouseButton::Left) => {
                g.ride.tune_save();
                if let Some(m) = g.ride.note.take() {
                    self.flash = Some((m, t));
                }
            }
            _ => return false,
        }
        true
    }

    fn key(&mut self, args: &Args, code: KeyCode, down: bool) {
        if down && code == KeyCode::F9 {
            self.hearing = !self.hearing;
            return;
        }
        if down && code == KeyCode::F11 {
            self.prefs.fullscreen = self.window.fullscreen().is_none();
            self.set_fullscreen(self.prefs.fullscreen);
            self.save_prefs(args);
            return;
        }
        match &self.screen {
            Screen::Playing => {}
            Screen::Loading => return,
            _ => {
                if down {
                    self.menu_key(args, code);
                }
                return;
            }
        }
        // The vehicle tuning panel, open with the stats while driving.
        if down {
            if let Some(g) = self.game.as_mut().filter(|g| g.settings.show_stats) {
                if g.ride.tune_key(code) {
                    return;
                }
            }
        }
        // Enter is not a binding: it is "go on" wherever something is
        // waiting, which today means playing another match (shooter/round.rs).
        if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) {
            self.actions.accept = true;
            return;
        }
        let action = input::action(code);
        if let Some(a) = action {
            if input::hold(&mut self.input, a, down) {
                return;
            }
        }
        if !down {
            return;
        }
        let Some(a) = action else { return };
        let Some(g) = &mut self.game else { return };
        match a {
            Action::Help => self.set_screen(Screen::Help { back: Back::Playing }),
            Action::Worlds => self.choose(args, Choice::Open, 0),
            Action::Escape => {
                if g.in_screenshot() {
                    self.actions.screenshot_toggle = true;
                } else if g.camera.wants_escape() {
                    self.actions.back = true;
                } else {
                    self.set_screen(Screen::Paused);
                }
            }
            Action::Glide => self.actions.glide = true,
            Action::FreeCamera => self.actions.toggle_fly = true,
            Action::Screenshot => self.actions.screenshot_toggle = true,
            Action::TakeScreenshot => self.actions.screenshot_take = true,
            Action::Photography => self.actions.photography = true,
            Action::Logbook => self.actions.logbook = true,
            Action::Frame => self.actions.frame = true,
            Action::Reload => self.actions.reload = true,
            Action::TiltRight => self.actions.tilt_right = true,
            Action::TiltLeft => self.actions.tilt_left = true,
            Action::Bike => self.actions.bike_toggle = true,
            Action::Kayak => self.actions.kayak_toggle = true,
            Action::Vehicle => self.actions.vehicle_toggle = true,
            Action::Camera => self.actions.cam_toggle = true,
            Action::Select(n) => self.actions.select = Some(n as usize),
            Action::Shadows => {
                g.settings.shadows = !g.settings.shadows;
                g.note(if g.settings.shadows { "SHADOWS ON" } else { "SHADOWS OFF" });
            }
            Action::Slopes => {
                let on = !g.settings.slopes;
                g.settings.slopes = on;
                let p = g.player.pos;
                g.streamer.set_smooth(on, p);
                g.note(if on { "SLOPES: ON" } else { "SLOPES: OFF" });
            }
            Action::NextHour => {
                let (h, name) = crate::world::clock::next_preset(g.hour);
                g.hour = h;
                g.note(name);
            }
            Action::NextWeather => {
                let k = g.weather.next_kind();
                g.weather.set(k);
                let m = k.name().to_uppercase();
                g.note(&m);
            }
            Action::NextSeason => {
                // Round the year to the next season this world has, arriving
                // in it at once rather than turning into it over half a
                // season: a key that takes a while to show what it did is
                // worse than no key.
                let s = g.year.calendar().after(g.year.season);
                g.year.set(s);
                let turn = g.year.turn();
                let p = g.player.pos;
                g.streamer.set_turn(turn, p);
                let m = s.name().to_uppercase();
                g.note(&m);
            }
            Action::Style => {
                g.settings.style = g.settings.style.next();
                let m = format!("STYLE: {}", g.settings.style.name().to_uppercase());
                g.note(&m);
            }
            Action::Stats => g.settings.show_stats = !g.settings.show_stats,
            Action::Minimap => g.settings.minimap = !g.settings.minimap,
            Action::Look => self.set_screen(Screen::Look {
                page: 0,
                pick: None,
                back: Back::Playing,
            }),
            Action::Edit => {
                self.open_edit(None);
                if self.edit.is_some() {
                    self.menu.at = 0;
                    self.set_screen(Screen::Edit {
                        page: 0,
                        pick: None,
                        open: None,
                        back: Back::Playing,
                    });
                }
            }
            Action::Save => g.save(),
            // F11 is handled above, on every screen.
            Action::Fullscreen => {}
            // Held actions returned above.
            Action::Forward | Action::Back | Action::Left | Action::Right | Action::Jump | Action::Sprint | Action::Down | Action::Hook | Action::LookAround => {}
        }
        if matches!(a, Action::Shadows | Action::Slopes | Action::Style | Action::Stats | Action::Minimap) {
            self.prefs_from_game(args);
        }
    }

    /// Open the EDIT page on `name` (a creature or a plant), with a studio
    /// in front of the player for whatever it turns out to be.
    fn open_edit(&mut self, name: Option<&str>) {
        if self.edit.is_some() {
            return;
        }
        let Some(game) = self.game.as_mut() else {
            self.note_flash("NO WORLD TO EDIT IN".to_string());
            return;
        };
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        match editor::open(game, name, aspect) {
            Ok((page, studio)) => {
                self.edit = Some(page);
                self.studio = Some(studio);
            }
            Err(x) => self.note_flash(format!("NOT OPENED: {}", x.to_uppercase())),
        }
    }

    /// Drop a list open under one of the page's rows, on the choice in
    /// force rather than at the top of a list it may be a long way down.
    fn open_edit_drop(&mut self, which: u16) {
        let Some(e) = self.edit.as_ref() else { return };
        let Some(lib) = self.game.as_ref().and_then(|g| g.library()) else { return };
        if e.options(which, lib).len() < 2 {
            return;
        }
        let at = e.drop_at(which, lib);
        self.edit_at = self.menu.at;
        if let Screen::Edit { page, pick, back, .. } = self.screen {
            self.set_screen(Screen::Edit { page, pick, open: Some(which), back });
        }
        self.menu.at = at;
        self.menu.top = at.saturating_sub(2);
    }

    /// Close whatever list the page has open, back onto its row.
    fn close_edit_drop(&mut self) {
        if let Screen::Edit { page, pick, open: Some(_), back } = self.screen {
            self.set_screen(Screen::Edit { page, pick, open: None, back });
            self.menu.at = self.edit_at;
            self.menu.top = 0;
            self.menu.shown = 0;
        }
    }

    /// Shut the studio down and put the world back as it was.
    fn close_edit(&mut self) {
        if let (Some(st), Some(game)) = (&mut self.studio, &mut self.game) {
            st.close(game);
            game.persist = st.persist_was;
        }
        self.studio = None;
        self.edit = None;
    }

    /// Rebuild the subject from the entry as it stands.
    fn edit_floor(&mut self) {
        if let (Some(e), Some(st), Some(game)) = (&self.edit, &mut self.studio, &mut self.game) {
            editor::floor(e, st, game);
        }
    }

    /// The page's retries while the ground under the studio streams in.
    fn edit_tick(&mut self, dt: f32) {
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        if let (Some(e), Some(st), Some(game)) = (&mut self.edit, &mut self.studio, &mut self.game) {
            editor::tick(e, st, game, aspect, dt);
        }
    }

    /// Put one world's pictures behind the menus, or every world's. Nothing
    /// happens while it is already showing the right set, and a world with
    /// no pictures of its own falls back to all of them rather than to a
    /// black screen.
    fn show_pictures(&mut self, world: Option<&str>, t: f32) {
        if self.slides_world.as_deref() == world {
            return;
        }
        self.slides_world = world.map(str::to_string);
        let mut pics = menu::picture_paths(world);
        if pics.is_empty() {
            pics = menu::picture_paths(None);
        }
        let seed = self.t0.elapsed().as_nanos() as u64;
        self.slides.retarget(pics, seed, t);
    }

    /// Draw the menus and the loading screen into the HUD.
    fn draw_screen(&mut self, w: f32, h: f32) {
        let t = self.now();
        let items = self.items();
        let over_world = self.screen.over_world();
        if !over_world && self.screen != Screen::Loading {
            // The launch screen shows the world on the line, the way the
            // loading screen does; everywhere else shows them all.
            let world = match self.screen {
                Screen::Launch { .. } => Some(self.prefs.last.world.clone()),
                _ => None,
            };
            self.show_pictures(world.as_deref(), t);
            self.slides.update(&self.renderer, t);
            self.slides.draw(&mut self.hud, w, h, t);
        }
        let s = menu::scale(h);
        let place = if over_world { Place::Centre } else { Place::Left };
        match &self.screen {
            Screen::Playing => {}
            Screen::Loading => {
                if let Some(l) = &mut self.loading {
                    l.slides.update(&self.renderer, t);
                    l.slides.draw(&mut self.hud, w, h, t);
                    let (phase, progress) = match l.streaming {
                        None => ("BUILDING THE LAND".to_string(), None),
                        Some(_) => ("GROWING THE VIEW".to_string(), Some(l.progress)),
                    };
                    menu::draw_loading(&mut self.hud, &l.name, &phase, progress, w, h, t);
                }
            }
            Screen::Title => {
                let foot = self.load_error.as_deref().unwrap_or("");
                menu::draw_menu(&mut self.hud, &mut self.menu, "", &items, foot, place, w, h);
                menu::draw_logo(&mut self.hud, w, h);
            }
            Screen::Paused => {
                let foot = match &self.flash {
                    Some((m, at)) if t - at < 2.0 => m.as_str(),
                    _ => "Esc to resume",
                };
                menu::draw_menu(&mut self.hud, &mut self.menu, "PAUSED", &items, foot, place, w, h);
                if let Some(g) = &self.game {
                    g.build_tune(&mut self.hud, w, h, true);
                }
            }
            Screen::Launch { open, .. } => {
                let open = *open;
                // With a list open, the screen behind it is drawn as it was
                // (the line it hangs from still picked) into a menu of its
                // own, so the hit boxes belong to the list.
                let here = self.game.as_ref().map(|_| worldload::selected());
                let base_items = menu::launch_items(&self.prefs.last, here.as_deref());
                let at = if open.is_some() { self.launch_at } else { self.menu.at };
                // The world list open: the line under the highlight says
                // what that world is, so the list can be read down.
                let foot = match open {
                    Some(1) => items.get(self.menu.at).map(|i| i.help),
                    _ => base_items.get(at).map(|i| i.help),
                }
                .filter(|h| !h.is_empty())
                .unwrap_or("Left and right to change, Enter to go");
                let mut behind = Menu {
                    at,
                    top: self.launch_top,
                    ..Menu::default()
                };
                let menu = if open.is_some() { &mut behind } else { &mut self.menu };
                menu::draw_menu(&mut self.hud, menu, "OPEN WORLD", &base_items, foot, place, w, h);
                if let Some(which) = open {
                    let anchor = behind.drops.get(which as usize).copied().unwrap_or([0.0; 4]);
                    let in_force = menu::drop_at(&self.prefs.last, which);
                    menu::dim(&mut self.hud, w, h);
                    menu::draw_dropdown(&mut self.hud, &mut self.menu, &items, anchor, in_force, w, h);
                }
            }
            Screen::Settings { .. } => {
                menu::draw_menu(&mut self.hud, &mut self.menu, "SETTINGS", &items, "Left and right, or a click, to change", place, w, h);
            }
            Screen::Look { page, pick, .. } => {
                let (page, pick) = (*page, *pick);
                // The cursor's range, out over the world beside the list.
                self.probe = if self.cursor.0 > menu::unit(w, h) * 44.0 { self.cursor_probe() } else { None };
                match pick {
                    Some(code) => {
                        let name = lookpage::swatch_name(code);
                        let foot = "Drag in the square, the hue bar or the distance bar; Esc when it is right";
                        let bar = {
                            let g = self.renderer.grade_of(self.style_now());
                            lookpage::stop_distance(g, code).map(|at| lookpage::DistBar::new(g, at, self.probe))
                        };
                        lookpage::draw(&mut self.hud, &mut self.menu, &name, &items, foot, w, h);
                        lookpage::draw_picker(&mut self.hud, self.pick_hsv, &name, bar, w, h);
                    }
                    None => {
                        let fresh = self.flash.as_ref().filter(|(_, at)| t - at < 4.0).map(|(m, _)| m.clone());
                        let foot = fresh.as_deref().unwrap_or_else(|| lookpage::page_foot(page));
                        lookpage::draw(&mut self.hud, &mut self.menu, lookpage::page_title(page), &items, foot, w, h);
                    }
                }
                if let (Some(d), Some(g)) = (self.probe, self.game.as_ref()) {
                    let grade = self.renderer.grade_of(g.settings.style);
                    lookpage::draw_probe(&mut self.hud, grade, d, self.cursor, w, h);
                }
            }
            Screen::Edit { page, pick, open, .. } => {
                let (page, pick, open) = (*page, *pick, *open);
                let heading = self.edit.as_ref().map(|e| e.title()).unwrap_or_default();
                let foot = self.edit.as_ref().map(|e| e.foot()).unwrap_or_default();
                // With a list open the panel behind it is drawn as it was,
                // into a menu of its own, so the hit boxes belong to the list.
                let mut behind = Menu {
                    at: if open.is_some() { self.edit_at } else { self.menu.at },
                    ..Menu::default()
                };
                if let Some(e) = self.edit.as_mut() {
                    let rows = e.rows();
                    let menu = if open.is_some() { &mut behind } else { &mut self.menu };
                    editor::panel::draw(&mut self.hud, menu, e, &rows, &heading, &foot, w, h);
                }
                if let Some(which) = open {
                    let anchor = behind.drops.get(self.edit_at).copied().unwrap_or([0.0; 4]);
                    let in_force = self.edit.as_ref().zip(self.game.as_ref().and_then(|g| g.library())).map_or(0, |(e, lib)| e.drop_at(which, lib));
                    menu::draw_dropdown(&mut self.hud, &mut self.menu, &items, anchor, in_force, w, h);
                }
                if let Some(i) = pick {
                    let name = self.edit.as_ref().map(|e| e.colour_label(page, i as usize)).unwrap_or_default();
                    lookpage::draw_picker(&mut self.hud, self.pick_hsv, &name, None, w, h);
                }
            }
            Screen::Help { back } => {
                let allowed = self.game.as_ref().map_or(modes::Allowed::all(), |g| g.ride.allowed);
                let foot = if *back == Back::Playing { "H, Esc or a click to resume" } else { "H, Esc or a click to go back" };
                game::draw_help(&mut self.hud, allowed, w, h, s, foot);
            }
        }
    }

    fn frame(&mut self, args: &Args) {
        // A hidden or zero-sized window has no swapchain worth drawing to:
        // acquiring one anyway is how clicking away used to end the run.
        let wsize = self.window.inner_size();
        if self.hidden || wsize.width == 0 || wsize.height == 0 {
            self.last = Instant::now();
            return;
        }
        let now = Instant::now();
        // The clamp keeps one long stall (a world loading, the window coming
        // back) from throwing the simulation forward in a single step. The
        // benchmark reports `raw`, not the clamped step: clamping it reported
        // a 300 ms hitch as 100 ms, so max_ms and p99 were capped at the
        // clamp and the worst frames of a run were invisible. Under vsync the
        // step is evened out to the refresh (`pacing`), because the loop's own
        // intervals bunch up while the display still shows one frame a refresh.
        // Parts left from a frame that ended early belong to no frame.
        trace::take();
        let loading = self.screen == Screen::Loading;
        let raw = (now - self.last).as_secs_f32();
        let dt = self.pacer.step(raw, self.config.present_mode == wgpu::PresentMode::Fifo);
        self.last = now;
        if self.screen == Screen::Loading {
            self.loading_step(args);
        }
        let (w, h) = (self.config.width, self.config.height);
        let in_world = self.game.is_some() && self.screen.over_world();
        self.hud.clear();
        if in_world {
            let g = self.game.as_mut().unwrap();
            if let Some(b) = &mut self.bench {
                let (p, yaw, pitch) = b.camera(g);
                g.set_camera(p, yaw, pitch, true);
                b.t += dt;
                b.times.push(raw * 1000.0);
                b.steps.push(dt * 1000.0);
                let st = &g.streamer.stats;
                if st.queued + st.busy > 0 {
                    b.loading_frames += 1;
                }
                b.max_queue = b.max_queue.max(st.queued + st.busy);
            }
            // Only Playing runs the world, so the bike and glider don't carry on
            // behind a menu. A benchmark flight keeps the world running.
            // The EDIT page keeps the world running, so the animal in the
            // studio goes on grazing while its entry is edited.
            if self.screen == Screen::Playing || matches!(self.screen, Screen::Edit { .. }) || self.bench.is_some() {
                // A held button is put back every frame, whoever took it out
                // of `actions` last frame (see `held`).
                if self.screen == Screen::Playing {
                    self.actions.dig |= self.held.0;
                    self.actions.place |= self.held.1;
                }
                let tr = trace::start();
                g.tick(dt, &self.input, &mut self.actions);
                trace::end("tick", tr);
            }
            let tr = trace::start();
            g.stream(&mut self.renderer.terrain, Duration::from_millis(3));
            trace::end("stream", tr);
            // The EDIT page rebuilds its subject's models as it is edited.
            let tr = trace::start();
            upload_models(&mut self.renderer, g);
            trace::end("models", tr);
            if self.screen == Screen::Playing {
                let tr = trace::start();
                g.build_hud(&mut self.hud, w as f32, h as f32);
                trace::end("hud", tr);
            }
            if let Some(img) = g.take_map_image() {
                self.renderer.upload_map(&img);
            }
        }
        if matches!(self.screen, Screen::Edit { .. }) {
            self.edit_tick(dt);
        }
        let tr = trace::start();
        self.hear(dt, w as f32, h as f32);
        trace::end("sound", tr);
        self.draw_screen(w as f32, h as f32);
        // Waiting here and in `present` is the display, not work: under vsync
        // it is most of a quick frame.
        let acquire = Instant::now();
        let surface = self.surface.get_current_texture();
        let mut wait = acquire.elapsed().as_secs_f32();
        let frame = match surface {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.config.width = wsize.width;
                self.config.height = wsize.height;
                self.surface.configure(&self.gpu.device, &self.config);
                self.renderer.resize(wsize.width, wsize.height);
                return;
            }
            Err(e) => {
                eprintln!("surface: {e:?}");
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });
        let tr = trace::start();
        match (&mut self.game, in_world) {
            (Some(g), true) => {
                let aspect = w as f32 / h.max(1) as f32;
                g.prepare_frame(aspect);
                let params = g.frame_params(aspect);
                let stats = self.renderer.render(&g.streamer, &params, &self.hud, &view);
                g.last_render = stats;
                let format = self.config.format.add_srgb_suffix();
                if g.take_screenshot_request() {
                    let path = screenshot::next_path(std::path::Path::new("screenshots"), &worldload::selected());
                    self.writing.push(screenshot::capture(&self.gpu, &mut self.renderer, g, format, w, h, path.clone()));
                    g.screenshot_saved(&path.display().to_string());
                }
                if let Some(plan) = g.take_shot(w, h) {
                    let (px, cw, ch) = screenshot::develop(&self.gpu, &mut self.renderer, g, format, w, h, &plan);
                    g.developed(px, cw, ch);
                }
                self.writing.append(&mut g.files);
            }
            _ => self.renderer.render_ui(&self.hud, &view),
        }
        trace::end("render", tr);
        let mut i = 0;
        while i < self.writing.len() {
            if self.writing[i].is_finished() {
                if let Ok(Err(e)) = self.writing.swap_remove(i).join() {
                    if let Some(g) = &mut self.game {
                        g.note(&format!("NOT SAVED: {e}"));
                    }
                }
            } else {
                i += 1;
            }
        }
        self.window.pre_present_notify();
        let presenting = Instant::now();
        frame.present();
        wait += presenting.elapsed().as_secs_f32();
        let work = now.elapsed().as_secs_f32() - wait;
        // A loading frame is slow on purpose.
        if loading {
            trace::take();
        } else if let Some(line) = trace::frame_done(work * 1000.0, wait * 1000.0) {
            if self.trace {
                if let Some(g) = &self.game {
                    let st = &g.streamer.stats;
                    eprintln!(
                        "{line} | stream: accept {:.1} x{} plan {:.1} vis {:.1} unload {:.1} x{} | queued {} busy {}",
                        st.accept_ms, st.accepted, st.plan_ms, st.vis_ms, st.unload_ms, st.unloaded, st.queued, st.busy
                    );
                } else {
                    eprintln!("{line}");
                }
            }
        }
    }
}

impl Running {
    /// This frame's sound: survey the world round the listener and hand the
    /// moment to the device, with the ambience let through in full while
    /// playing, turned down behind a menu, and gone with no world or no
    /// focus. With the readout on (F9), draw what the ambience is made of.
    fn hear(&mut self, dt: f32, w: f32, h: f32) {
        let in_world = self.game.is_some() && self.screen.over_world();
        let duck = if !self.focused || !in_world {
            0.0
        } else if self.screen == Screen::Playing || matches!(self.screen, Screen::Edit { .. }) {
            1.0
        } else {
            0.35
        };
        let (heard, facing) = match (&self.game, in_world) {
            (Some(g), true) => {
                let (eye, fwd) = g.listener();
                // The match and the feet are only watched while the world
                // runs; behind a menu nothing moves to be heard.
                if duck >= 1.0 {
                    self.watch.look(g, dt, eye);
                }
                let noises = sound::take_noises();
                if duck >= 1.0 {
                    self.sound.play(&noises, eye, [fwd.x, fwd.z]);
                }
                let mut heard = self.ear.hear(g, dt);
                if let Some(car) = g.ride.car() {
                    let s = &car.spec;
                    heard.motor = if car.stalled { 0.0 } else { 0.45 + 0.55 * car.load };
                    heard.motor_hz = s.idle_hz + (s.top_hz - s.idle_hz) * car.rpm;
                    heard.motor_load = car.load;
                    heard.scrub = car.wheels.iter().filter(|w| w.down).map(|w| (w.slide / 6.0 + w.spin * 0.4).min(1.0)).fold(0.0, f32::max);
                }
                (heard, [fwd.x, fwd.z])
            }
            _ => {
                self.ear.reset();
                sound::take_noises();
                (sound::Heard::default(), [0.0, -1.0])
            }
        };
        self.sound.update(heard, facing, duck);
        if self.hearing && in_world {
            let s = (h / 1080.0).max(0.6) * 15.0;
            let mut y = h * 0.18;
            let ink = [0.92, 0.95, 1.0, 0.95];
            let head = if self.sound.playing() {
                format!("SOUND {} AT {} HZ", self.sound.device.to_uppercase(), self.sound.rate)
            } else {
                "SOUND OFF".to_string()
            };
            self.hud.rect(w - s * 34.0, y - s * 1.2, s * 33.0, s * 1.5 * 11.0 + s, [0.0, 0.0, 0.0, 0.55]);
            for line in std::iter::once(head).chain(self.ear.lines(&heard)) {
                self.hud.text(w - s * 33.0, y, s, ink, &line);
                y += s * 1.5;
            }
        }
    }
}

/// Hand the renderer the models again after the EDIT page rebuilt one. A
/// frame drawn before this would reach past the end of the renderer's model
/// list for the subject's own model, so it runs before anything is drawn.
pub(crate) fn upload_models(renderer: &mut Renderer, game: &mut Game) {
    if std::mem::take(&mut game.palette_dirty) && !game.models_dirty {
        renderer.set_palette(&game.palette);
    }
    if !game.models_dirty {
        return;
    }
    game.models_dirty = false;
    renderer.set_models(&game.models, &game.palette);
}

/// Hand the renderer everything it needs from the loaded world: heightmap,
/// size, sea level, mist, models and their palette.
pub(crate) fn show_world(renderer: &mut Renderer, game: &Game) {
    let gen = game.generator();
    let heights = gen.height_grid(render::HEIGHTS_N as usize);
    let shore = gen.world.as_ref().map_or_else(Vec::new, |w| world::surf::shore_texels(&w.f, render::HEIGHTS_N as usize));
    renderer.set_world(&heights, &shore, gen.spec.size_m, gen.spec.sea_level_m);
    if let Some(w) = gen.world.as_ref().filter(|w| w.f.tide.on()) {
        renderer.set_tide(w.f.tide.range);
    }
    if let Some(w) = gen.world.as_ref() {
        if let Some(s) = w.def.surf {
            renderer.set_surf(s.uniform(), w.f.shore.uniform());
        }
    }
    renderer.set_mist(&gen.mist_grid(256), 256, gen.water_grid(256));
    renderer.set_underwater(gen.world.as_ref().map(|w| w.def.underwater).unwrap_or_default());
    if let Some(w) = gen.world.as_ref() {
        let (water, sheets, spots) = world::rivers::buffers(&w.f);
        renderer.set_rivers(&water, &sheets, &spots);
    } else {
        renderer.set_rivers(&[], &[], &[]);
    }
    renderer.set_models(&game.models, &game.palette);
    let (looks, errors) = library::load_looks(&library::data_root());
    for e in errors {
        eprintln!("looks: {e}");
    }
    let world_look = gen.world.as_ref().and_then(|w| w.look.clone());
    for d in looks {
        let Some(style) = render::Style::parse(&d.name) else {
            eprintln!("looks: {} is not a style", d.name);
            continue;
        };
        let (look, grade) = lookpage::of_style(&d, world_look.as_ref(), style);
        renderer.set_grade(style, grade);
        renderer.set_look(style, look);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.run.is_some() {
            return;
        }
        let a = &self.args;
        let mut prefs = Prefs::load(&a.save_dir);
        // Flags on the command line win over the saved settings.
        if let Some(s) = a.style {
            prefs.settings.style = s;
        }
        if !a.shadows {
            prefs.settings.shadows = false;
        }
        if a.blocky {
            prefs.settings.slopes = false;
        }
        if a.vsync {
            prefs.vsync = true;
        }
        if a.view_set {
            prefs.view = a.view;
        }
        // A maximized window, or borderless fullscreen when that is the setting.
        let mut attrs = Window::default_attributes()
            .with_title(menu::TITLE)
            .with_inner_size(winit::dpi::PhysicalSize::new(a.size.0, a.size.1))
            .with_maximized(true)
            .with_visible(false);
        if prefs.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let instance = Gpu::instance(a.dx12);
        let surface = instance.create_surface(window.clone()).expect("surface");
        let gpu = Gpu::new(instance, Some(&surface));
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let present = present_mode(&caps.present_modes, prefs.vsync);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: present,
            desired_maximum_frame_latency: frame_latency(present),
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![format.add_srgb_suffix()],
        };
        surface.configure(&gpu.device, &config);
        // The renderer reads the library's materials (colours, texture layers,
        // flags) when it is made, so install them before any world loads. The
        // library is shared by every world and installed once.
        let _ = worldload::library();
        let renderer = Renderer::new(&gpu, config.format.add_srgb_suffix(), config.width, config.height, self.args.msaa.unwrap_or(prefs.msaa));
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let sound = sound::Sound::new(prefs.sound);
        let refresh = refresh_interval(&window);
        let mut run = Running {
            window,
            surface,
            config,
            present_modes: caps.present_modes.clone(),
            gpu_name: format!("{}  {:?}", gpu.describe(), present),
            gpu,
            renderer,
            game: None,
            hud: Hud::default(),
            input: MoveInput::default(),
            writing: Vec::new(),
            actions: Actions::default(),
            held: (false, false),
            last: Instant::now(),
            pacer: Pacer::new(refresh, std::env::var("VOXEL_PACING").is_ok_and(|v| v == "0")),
            trace: std::env::var("VOXEL_TRACE").is_ok(),
            t0: Instant::now(),
            grabbed: false,
            bench: None,
            screen: Screen::Title,
            menu: Menu::default(),
            cursor: (0.0, 0.0),
            prefs,
            slides: Slides::new(menu::picture_paths(None), seed),
            loading: None,
            flash: None,
            pick_hsv: [0.0; 3],
            look_at: 0,
            drag: None,
            scrolling: false,
            tune_drag: None,
            edit_on_load: false,
            probe: None,
            load_error: None,
            edit: None,
            studio: None,
            edit_at: 0,
            cam_drag: None,
            cam_last: (0.0, 0.0),
            launch_at: 0,
            launch_top: 0,
            slides_world: None,
            hidden: false,
            focused: true,
            quit: false,
            sound,
            ear: sound::bed::Ear::default(),
            watch: sound::watch::Watch::default(),
            hearing: false,
        };
        // `--world` (or `--bench`) goes straight into a world; otherwise the title screen.
        if self.args.world.is_some() || self.args.bench {
            let name = worldload::selected();
            // The command line has no launch screen behind it: the world's
            // own defaults, which is the mode it is built for.
            run.start_load(&self.args, launch::Chosen::for_world(&name), true);
        }
        // The window was made hidden so Windows never shows its blank white
        // client area while the GPU starts. Draw the first frame now (a hidden
        // window gets no redraw events) and only then show it.
        run.frame(&self.args);
        run.window.set_visible(true);
        self.run = Some(run);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = &mut self.run else { return };
        let args = &self.args;
        match event {
            WindowEvent::CloseRequested => r.quit = true,
            WindowEvent::Resized(s) => {
                if s.width > 0 && s.height > 0 {
                    r.hidden = false;
                    r.config.width = s.width;
                    r.config.height = s.height;
                    r.surface.configure(&r.gpu.device, &r.config);
                    r.renderer.resize(s.width, s.height);
                } else {
                    // Minimized: the swapchain stays as it is until it is back.
                    r.hidden = true;
                }
            }
            WindowEvent::Occluded(o) => r.hidden = o,
            // It may be on another monitor now, at another refresh rate.
            WindowEvent::Moved(_) => r.pacer.refresh = refresh_interval(&r.window),
            WindowEvent::Focused(on) => {
                r.focused = on;
                if !on && r.screen == Screen::Playing {
                    r.set_screen(Screen::Paused);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    if !event.repeat || !pressed {
                        r.key(args, code, pressed);
                    } else if r.screen != Screen::Playing
                        && matches!(
                            code,
                            KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::KeyW | KeyCode::KeyS | KeyCode::KeyA | KeyCode::KeyD
                        )
                    {
                        // Held arrows keep moving through a menu, and keep
                        // winding a value up or down.
                        r.key(args, code, true);
                    } else if r.screen == Screen::Playing && crate::vehicle::tune::is_key(code) && code != KeyCode::KeyP {
                        r.key(args, code, true);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                r.cursor = (position.x as f32, position.y as f32);
                if let Some((i, x0, base)) = &r.tune_drag {
                    // A step for every 8 pixels across.
                    let steps = ((r.cursor.0 - x0) / 8.0).round();
                    let (i, base) = (*i, base.clone());
                    if let Some(g) = r.game.as_mut() {
                        g.ride.tune_apply(i, &base, steps);
                    }
                } else if let Some(orbit) = r.cam_drag {
                    let (dx, dy) = (r.cursor.0 - r.cam_last.0, r.cursor.1 - r.cam_last.1);
                    r.cam_last = r.cursor;
                    let h = r.config.height as f32;
                    if let Some(st) = r.studio.as_mut() {
                        if orbit {
                            st.cam.orbit(dx, dy);
                        } else {
                            st.cam.pan(dx, dy, game::FOV_Y, h);
                        }
                    }
                } else if r.scrolling {
                    let n = r.items().len();
                    r.menu.drag_to(n, r.cursor.1);
                } else if let Some(which) = r.drag {
                    r.pick_drag(which);
                } else if r.screen != Screen::Playing {
                    if let Some(i) = r.menu.hit(r.cursor.0, r.cursor.1) {
                        r.menu.at = i;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                match r.screen {
                    Screen::Playing => match button {
                        MouseButton::Left => {
                            r.held.0 = down;
                            r.actions.dig = down;
                        }
                        MouseButton::Right => {
                            r.held.1 = down;
                            r.actions.place = down;
                        }
                        MouseButton::Back | MouseButton::Forward => r.input.hook = down,
                        MouseButton::Middle if down => r.actions.autofocus = true,
                        _ => {}
                    },
                    Screen::Loading => {}
                    _ => {
                        if !down {
                            r.drag = None;
                            r.scrolling = false;
                            r.cam_drag = None;
                            r.tune_drag = None;
                        } else if r.screen == Screen::Paused && r.tune_press(button) {
                        } else if r.on_studio() && matches!(button, MouseButton::Left | MouseButton::Right | MouseButton::Middle) {
                            // Out over the studio the mouse turns the camera:
                            // left drags it round the subject, the others
                            // slide the view across it.
                            r.cam_drag = Some(button == MouseButton::Left);
                            r.cam_last = r.cursor;
                        } else if r.menu.on_thumb(r.cursor.0, r.cursor.1) {
                            r.scrolling = true;
                            let n = r.items().len();
                            r.menu.drag_to(n, r.cursor.1);
                        } else if matches!(button, MouseButton::Left | MouseButton::Right) {
                            // In the picker, a press inside the square or the
                            // hue strip starts a drag.
                            match r.pick_region() {
                                Some(which) => {
                                    r.drag = Some(which);
                                    r.pick_drag(which);
                                }
                                None => r.menu_click(args, button == MouseButton::Right),
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                let tune = if r.screen == Screen::Paused { r.tune_hover() } else { None };
                if let (Some(game::TuneHit::Line(i)), true) = (tune, d != 0.0) {
                    if let Some(g) = r.game.as_mut() {
                        g.ride.tune_step(i, d.signum());
                    }
                } else if d != 0.0 && r.screen == Screen::Playing {
                    r.actions.brush_delta = if d > 0.0 { 1 } else { -1 };
                } else if d != 0.0 && matches!(r.screen, Screen::Edit { open: None, pick: None, .. }) && !r.on_studio() {
                    if let Some(e) = r.edit.as_mut() {
                        e.scroll(if d > 0.0 { -2 } else { 2 });
                    }
                } else if d != 0.0 && r.on_studio() {
                    if let Some(st) = r.studio.as_mut() {
                        st.cam.dolly(if d > 0.0 { 1.0 } else { -1.0 });
                    }
                } else if d != 0.0 && r.menu.shown > 0 {
                    // An open list rolls under the wheel without the choice
                    // moving with it, which is what a list does everywhere
                    // else and what the caret at the bottom could not do.
                    let n = r.items().len();
                    r.menu.scroll(n, if d > 0.0 { -1 } else { 1 });
                }
            }
            WindowEvent::RedrawRequested => {
                r.frame(args);
                if let Some(b) = &r.bench {
                    if b.t >= b.duration {
                        println!("{}", serde_json::to_string_pretty(&b.report()).unwrap());
                        r.quit = true;
                    }
                }
            }
            _ => {}
        }
        if r.quit {
            r.prefs_from_game(args);
            if let Some(g) = &mut r.game {
                g.save();
            }
            el.exit();
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        let Some(r) = &mut self.run else { return };
        if let DeviceEvent::MouseMotion { delta } = event {
            if let (true, Some(g)) = (r.grabbed, &mut r.game) {
                // Slower through the sighting compass, in step with its narrower view.
                let s = 0.0022 * g.fov() / game::FOV_Y;
                g.player.yaw -= delta.0 as f32 * s;
                g.player.pitch = (g.player.pitch - delta.1 as f32 * s).clamp(-1.55, 1.55);
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(r) = &self.run else { return };
        // With vsync off nothing paces the loop, so behind a menu or out of
        // focus it submitted frames as fast as the CPU could build them --
        // thousands a second, the GPU falling further behind each one. Idle at
        // 60 instead. Loading and a benchmark still run flat out.
        let idle = (!r.focused || r.screen != Screen::Playing) && r.screen != Screen::Loading && r.bench.is_none();
        if idle {
            let next = r.last + Duration::from_millis(16);
            if Instant::now() < next {
                el.set_control_flow(ControlFlow::WaitUntil(next));
                return;
            }
        }
        el.set_control_flow(ControlFlow::Poll);
        r.window.request_redraw();
    }
}
