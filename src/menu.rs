//! The screens around the game: the title screen with screenshots cycling
//! behind the logo, the world list, settings, the loading screen and the pause
//! menu. They are drawn with the HUD; main.rs owns the window and decides what
//! each choice does. Settings that belong to the player rather than to a world
//! (fullscreen, style, view distance...) are kept in `settings.json` in the
//! save folder.

use crate::render::font::{hex, Face};
use crate::render::hud::{with_a, Hud, Ink, Type, BACKDROP_H, BACKDROP_W};
use crate::render::Renderer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

/// The game's name (the window title; the title screen shows the logo).
pub const TITLE: &str = "Lurelia";

/// The player's settings, shared by every world.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub fullscreen: bool,
    /// Style, shadows, sloped ground, the stats and the minimap: what a game
    /// is handed when it starts and changes in play (`game::Settings`).
    #[serde(flatten)]
    pub settings: crate::game::Settings,
    /// Antialiasing samples: 4, 2 or 1 (off). Applies when the game starts.
    #[serde(default = "default_msaa")]
    pub msaa: u32,
    /// Full-detail radius in metres; applies when a world loads.
    pub view: f32,
    pub vsync: bool,
    /// A launch choice this build cannot read takes its default and leaves
    /// the other settings alone. What RETURN opens: the world, the way of playing it and its options
    /// (src/launch.rs).
    #[serde(deserialize_with = "crate::util::or_default")]
    pub last: crate::launch::Launch,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            fullscreen: true,
            settings: crate::game::Settings::default(),
            msaa: default_msaa(),
            view: 128.0,
            vsync: false,
            last: crate::launch::Launch::default(),
        }
    }
}

fn default_msaa() -> u32 {
    4
}

/// The antialiasing sample counts SETTINGS steps through. 4x costs 0.4 to
/// 0.75 ms of GPU time a frame. 2x is not offered: on a 3060 it measured no
/// cheaper than 4x and looks worse, so the choice is off or 4x. A settings
/// file that still says 2 lands on 4 (see `step`).
pub const SAMPLES: [u32; 2] = [1, 4];

/// The view distances SETTINGS steps through.
pub const VIEWS: [f32; 5] = [64.0, 96.0, 128.0, 192.0, 256.0];

impl Prefs {
    fn path(save_dir: &Path) -> PathBuf {
        save_dir.join("settings.json")
    }

    pub fn load(save_dir: &Path) -> Prefs {
        // A file that is there and will not parse is set aside before the
        // defaults stand in for it: the next change to any setting saves, and
        // that used to be the end of every setting in the old file.
        let path = Self::path(save_dir);
        let mut p: Prefs = match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).unwrap_or_else(|e| {
                crate::util::set_aside(&path, format_args!("{e}; starting from the default settings"));
                Prefs::default()
            }),
            Err(_) => Prefs::default(),
        };
        // A file written when 2x was on offer: snap it to the nearest count
        // still offered, so the line reads what is running.
        if !SAMPLES.contains(&p.msaa) {
            p.msaa = if p.msaa <= 1 { 1 } else { 4 };
        }
        p
    }

    pub fn save(&self, save_dir: &Path) {
        let _ = std::fs::create_dir_all(save_dir);
        if let Ok(t) = serde_json::to_string_pretty(self) {
            // Written beside the file and moved over it, so a crash halfway
            // leaves the old settings rather than half of the new ones.
            let path = Self::path(save_dir);
            let tmp = path.with_extension("tmp");
            if let Err(e) = std::fs::write(&tmp, t).and_then(|_| std::fs::rename(&tmp, &path)) {
                eprintln!("settings: {e}");
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Fullscreen,
    Style,
    Shadows,
    Slopes,
    Msaa,
    View,
    Vsync,
    Stats,
}

/// What a menu line does when chosen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    /// Open the world RETURN names again, where it was left.
    Continue,
    /// The launch screen: the world, the mode and their options.
    Open,
    /// Step the launch screen's world, mode, or one of its option lines.
    LaunchWorld,
    LaunchMode,
    LaunchOpt(u8),
    /// Go, with what the launch screen says.
    Start,
    /// A line of an open dropdown: the world, or the game, at this index.
    ChooseWorld(u8),
    ChooseMode(u8),
    Settings,
    Controls,
    Quit,
    Resume,
    Save,
    MainMenu,
    Set(Setting),
    Back,
    // ---- the LOOK page (lookpage.rs) ----
    /// Open the LOOK page, or one of its groups (0 = the group list).
    Look(u8),
    /// Step one of lookpage::KNOBS.
    Knob(u8),
    /// A colour: open the picker on it (lookpage swatch code). Left and right
    /// move a gradient stop along its strip instead.
    Stop(u8),
    AddStop(u8),
    DelStop(u8),
    /// Hue, saturation or value inside the picker.
    Pick(u8),
    /// Close the picker, back to the group it was opened from.
    PickDone,
    /// Write the look back to library/looks/<style>.json.
    Export,
    /// Write what differs from the style file to this world's own
    /// look.json: 0 for every style, 1 for this style alone.
    ExportWorld(u8),
    /// Throw the edits away and read the look file again.
    ResetLook,
    // ---- the EDIT page (src/editor) ----
    /// Open the EDIT page from a menu that may have no world behind it:
    /// load one first if that is the case, then open the page on it.
    OpenEdit,
    /// Open the EDIT page, or one of its groups (0 = the front page).
    EditPage(u8),
    /// Creatures or plants.
    EditKind,
    /// Which entry of that kind.
    EditEntry,
    /// Roll another individual.
    EditRegen,
    /// Which age the subject is shown at.
    EditAge,
    /// What the subject is doing: a pose it holds, or loose in the world.
    EditPose,
    /// The grey platform, or the world's own ground.
    EditGrid,
    /// Step one of the entry's fields (`editor::knobs`).
    EditKnob(u8),
    /// Open the picker on one of the entry's colours: which heading it sits
    /// under, and which of that heading's colours it is.
    EditColour(u8, u8),
    /// Write the entry back to its file.
    EditExport,
    /// Read the entry's file again, losing the edits.
    EditReload,
    /// A line of a list the EDIT page has dropped open.
    EditChoose(u16),
}

#[derive(Clone)]
pub struct Item {
    pub label: String,
    /// A setting's value, in a column of its own.
    pub value: Option<String>,
    /// A word or two in italic after the label (the world CONTINUE opens).
    pub note: Option<String>,
    /// A colour chip before the value (a gradient stop, a light colour).
    pub swatch: Option<[f32; 3]>,
    /// A sentence or two about what this line does, shown while it is picked.
    pub help: &'static str,
    /// A line whose value is picked from a list that drops down under it,
    /// rather than stepped with the arrows.
    pub drop: bool,
    /// A line drawn as a button -- bigger, in a box, in a row with the other
    /// buttons under the list -- rather than as a row of the list.
    pub button: bool,
    pub choice: Choice,
}

impl Item {
    pub fn new(label: &str, choice: Choice) -> Item {
        Item {
            label: label.to_string(),
            value: None,
            note: None,
            swatch: None,
            help: "",
            drop: false,
            button: false,
            choice,
        }
    }

    /// A line the player opens a list on (the world, the game).
    pub fn drop(label: &str, choice: Choice, value: String) -> Item {
        Item {
            drop: true,
            ..Item::value(label, choice, value)
        }
    }

    /// GO, BACK: a button in a box under the list.
    pub fn button(label: &str, choice: Choice) -> Item {
        Item {
            button: true,
            ..Item::new(label, choice)
        }
    }

    pub fn value(label: &str, choice: Choice, value: String) -> Item {
        Item {
            value: Some(value),
            ..Item::new(label, choice)
        }
    }
}

/// How a world's folder name is shown.
pub fn world_title(name: &str) -> String {
    if name == "random" {
        "RANDOM ISLAND".to_string()
    } else {
        name.replace('_', " ").to_uppercase()
    }
}

pub fn title_items(prefs: &Prefs) -> Vec<Item> {
    vec![
        Item {
            note: Some(launch_words(&prefs.last)),
            help: "The world you were last in, as you left it.",
            ..Item::new("RETURN", Choice::Continue)
        },
        Item {
            help: "Pick a world, a way of playing it, and what is in it.",
            ..Item::new("OPEN WORLD", Choice::Open)
        },
        Item {
            help: "The studio: a creature or a plant on a platform, with its library entry beside it.",
            ..Item::new("CREATURES AND PLANTS", Choice::OpenEdit)
        },
        Item::new("SETTINGS", Choice::Settings),
        Item::new("CONTROLS", Choice::Controls),
        Item::new("QUIT", Choice::Quit),
    ]
}

pub fn pause_items() -> Vec<Item> {
    vec![
        Item::new("RESUME", Choice::Resume),
        Item::new("SAVE", Choice::Save),
        Item::new("SETTINGS", Choice::Settings),
        Item::new("LOOK AND COLOUR", Choice::Look(0)),
        Item::new("CREATURES AND PLANTS", Choice::OpenEdit),
        Item::new("CONTROLS", Choice::Controls),
        Item::new("BACK TO MAIN MENU", Choice::MainMenu),
        Item::new("EXIT GAME", Choice::Quit),
    ]
}

/// The launch screen: the world, the way of playing it, the options those
/// two put on offer, and GO. `here` is the world on screen now, if any, so a
/// line can say so.
pub fn launch_items(l: &crate::launch::Launch, here: Option<&str>) -> Vec<Item> {
    let mut out = vec![
        Item {
            note: (here == Some(l.world.as_str())).then(|| "here now".to_string()),
            help: "Which world to open.",
            ..Item::drop("WORLD", Choice::LaunchWorld, world_title(&l.world))
        },
        Item {
            help: mode_help(l.mode),
            ..Item::drop("GAME", Choice::LaunchMode, l.mode.title().to_string())
        },
    ];
    for (i, o) in crate::launch::options(&l.world, l.mode).iter().enumerate() {
        out.push(Item {
            help: o.help,
            ..Item::value(&o.label, Choice::LaunchOpt(i as u8), l.value(o))
        });
    }
    out.push(Item {
        help: "Open the world with these settings.",
        ..Item::button("GO", Choice::Start)
    });
    out.push(Item::button("BACK", Choice::Back));
    out
}

/// The lines of an open dropdown: every world, or every game this world
/// offers. `which` is 0 for the world and 1 for the game.
pub fn drop_items(l: &crate::launch::Launch, which: u8) -> Vec<Item> {
    if which == 0 {
        crate::worldload::available()
            .iter()
            .enumerate()
            .map(|(i, w)| Item::new(&world_title(w), Choice::ChooseWorld(i as u8)))
            .collect()
    } else {
        crate::launch::modes(&l.world)
            .iter()
            .enumerate()
            .map(|(i, m)| Item::new(m.title(), Choice::ChooseMode(i as u8)))
            .collect()
    }
}

/// Where a dropdown's list stands open now: the row it hangs under and the
/// line of that row that is picked.
pub fn drop_at(l: &crate::launch::Launch, which: u8) -> usize {
    if which == 0 {
        crate::worldload::available().iter().position(|w| *w == l.world).unwrap_or(0)
    } else {
        crate::launch::modes(&l.world).iter().position(|m| *m == l.mode).unwrap_or(0)
    }
}

fn mode_help(m: crate::launch::Mode) -> &'static str {
    match m {
        crate::launch::Mode::Explore => "The world to walk, ride, climb and photograph: the game as it has always been.",
        crate::launch::Mode::Fps => "An arena match: guns, respawns, and whoever gets the most kills.",
    }
}

/// "Glassfall, explore" for the note under RETURN.
pub fn launch_words(l: &crate::launch::Launch) -> String {
    format!("{}, {}", world_words(&l.world), l.mode.title().to_lowercase())
}

/// The antialiasing line's value. The renderer reads it when it is made, so a
/// choice that differs from what is running waits for the next start.
fn msaa_label(n: u32, now: u32) -> String {
    let v = if n <= 1 { "Off".to_string() } else { format!("{n}x") };
    if n == now {
        v
    } else {
        format!("{v}, next start")
    }
}

/// `msaa_now` is the sample count the renderer is actually running.
pub fn settings_items(p: &Prefs, in_world: bool, msaa_now: u32) -> Vec<Item> {
    let on = |b: bool| if b { "On" } else { "Off" }.to_string();
    let set = |label: &str, s: Setting, v: String| Item::value(label, Choice::Set(s), v);
    let view = if in_world { format!("{:.0} m, next load", p.view) } else { format!("{:.0} m", p.view) };
    vec![
        set("FULLSCREEN", Setting::Fullscreen, on(p.fullscreen)),
        set("STYLE", Setting::Style, soften(&p.settings.style.name().to_uppercase())),
        set("SHADOWS", Setting::Shadows, on(p.settings.shadows)),
        set("SLOPED GROUND", Setting::Slopes, on(p.settings.slopes)),
        set("ANTIALIASING", Setting::Msaa, msaa_label(p.msaa, msaa_now)),
        set("VIEW DISTANCE", Setting::View, view),
        set("VSYNC", Setting::Vsync, on(p.vsync)),
        set("PERFORMANCE STATS", Setting::Stats, on(p.settings.show_stats)),
        Item::new("BACK", Choice::Back),
    ]
}

/// Change a setting one step (`dir` +1 or -1). The caller applies it.
pub fn step(p: &mut Prefs, s: Setting, dir: i32) {
    match s {
        Setting::Fullscreen => p.fullscreen = !p.fullscreen,
        Setting::Style => p.settings.style = p.settings.style.next(),
        Setting::Shadows => p.settings.shadows = !p.settings.shadows,
        Setting::Slopes => p.settings.slopes = !p.settings.slopes,
        Setting::Msaa => {
            let n = SAMPLES.len() as i32;
            let at = SAMPLES.iter().position(|v| *v == p.msaa).unwrap_or(n as usize - 1) as i32;
            p.msaa = SAMPLES[(at + dir).rem_euclid(n) as usize];
        }
        Setting::Vsync => p.vsync = !p.vsync,
        Setting::Stats => p.settings.show_stats = !p.settings.show_stats,
        Setting::View => {
            let n = VIEWS.len() as i32;
            let at = VIEWS.iter().position(|v| (*v - p.view).abs() < 1.0).unwrap_or(2) as i32;
            p.view = VIEWS[(at + dir).rem_euclid(n) as usize];
        }
    }
}

/// A list of lines with one highlighted. `hits` are the line rectangles from
/// the last draw, for the mouse.
#[derive(Default)]
pub struct Menu {
    pub at: usize,
    pub hits: Vec<[f32; 4]>,
    /// The left and right arrows of each line that has them, so clicking one
    /// steps the value instead of opening the line. Parallel to `hits`,
    /// zero-sized where there is no arrow.
    pub minus: Vec<[f32; 4]>,
    pub plus: Vec<[f32; 4]>,
    /// Where each dropdown line's value stands, so the list that opens can
    /// hang under it. Parallel to `hits`, zero-sized on every other line.
    pub drops: Vec<[f32; 4]>,
    /// An open dropdown's first visible line and how many it shows. The
    /// scroll is its own state rather than a function of the highlight,
    /// because a wheel moves the list without moving the choice.
    pub top: usize,
    pub shown: usize,
    /// The scrollbar's track and its thumb, from the last draw, for the
    /// mouse. Both zero-sized when the whole list fits.
    pub track: [f32; 4],
    pub thumb: [f32; 4],
}

impl Menu {
    /// The line under the mouse, if any.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.hits.iter().position(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
    }

    /// -1 on line `i`'s left arrow, 1 on its right arrow, 0 elsewhere.
    pub fn on_arrow(&self, i: usize, x: f32, y: f32) -> i32 {
        let on = |v: &Vec<[f32; 4]>| v.get(i).is_some_and(|r| r[2] > 0.0 && x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]);
        if on(&self.minus) {
            -1
        } else if on(&self.plus) {
            1
        } else {
            0
        }
    }

    /// Move the highlight down or up, wrapping round, and bring it into
    /// view if the list is scrolled away from it.
    pub fn nav(&mut self, n: usize, down: bool) {
        if n > 0 {
            self.at = if down { (self.at + 1) % n } else { (self.at + n - 1) % n };
        }
        self.reveal(n);
    }

    /// Scroll just far enough that the highlighted line is on screen.
    pub fn reveal(&mut self, n: usize) {
        if self.shown == 0 {
            return;
        }
        let max = n.saturating_sub(self.shown);
        if self.at < self.top {
            self.top = self.at;
        } else if self.at >= self.top + self.shown {
            self.top = self.at + 1 - self.shown;
        }
        self.top = self.top.min(max);
    }

    /// Roll an open list by `lines`, positive down.
    pub fn scroll(&mut self, n: usize, lines: i32) {
        let max = n.saturating_sub(self.shown);
        self.top = (self.top as i32 + lines).clamp(0, max as i32) as usize;
    }

    /// Is `(x, y)` on the scrollbar's thumb?
    pub fn on_thumb(&self, x: f32, y: f32) -> bool {
        let t = self.thumb;
        t[3] > 0.0 && x >= t[0] - 2.0 && x < t[0] + t[2] + 2.0 && y >= t[1] && y < t[1] + t[3]
    }

    /// Where a drag of the thumb to `y` puts the top of the list.
    pub fn drag_to(&mut self, n: usize, y: f32) {
        let (ty, th) = (self.track[1], self.track[3]);
        if th <= 0.0 || self.shown == 0 {
            return;
        }
        let max = n.saturating_sub(self.shown);
        let run = (th - self.thumb[3]).max(1.0);
        let f = ((y - self.thumb[3] * 0.5 - ty) / run).clamp(0.0, 1.0);
        self.top = (f * max as f32).round() as usize;
    }
}

/// Pixels per font pixel for the in-game text at this window height.
pub fn scale(h: f32) -> f32 {
    (h / 540.0).floor().clamp(2.0, 4.0)
}

/// The menus' unit: a hundredth of the width of the largest 16:9 area that
/// fits the window, so the layout keeps its proportions at any size.
pub fn unit(w: f32, h: f32) -> f32 {
    w.min(h * 16.0 / 9.0) / 100.0
}

/// The menu colours, from the logo: cream type, its coral-rose-mauve
/// gradient for the selection, apricot stars, a plum-black ground.
pub struct Pal {
    pub ink: [f32; 4],
    pub ink_value: [f32; 4],
    pub lit: [[f32; 4]; 3],
    pub note: [f32; 4],
    pub foot: [f32; 4],
    pub apricot: [f32; 4],
    pub coral: [f32; 4],
    pub rose: [f32; 4],
    pub mauve: [f32; 4],
    pub plum: [f32; 4],
}

pub fn pal() -> &'static Pal {
    static P: std::sync::OnceLock<Pal> = std::sync::OnceLock::new();
    P.get_or_init(|| Pal {
        ink: hex(0xffece4, 0.72),
        ink_value: hex(0xffece4, 0.88),
        lit: [hex(0xffc49a, 1.0), hex(0xff9db8, 1.0), hex(0xe4b0ec, 1.0)],
        note: hex(0xffd9c9, 0.95),
        foot: hex(0xffe1d6, 0.9),
        apricot: hex(0xffad74, 1.0),
        coral: hex(0xff6b4a, 1.0),
        rose: hex(0xe9668e, 1.0),
        mauve: hex(0xa66aa6, 1.0),
        plum: hex(0x1a0e18, 1.0),
    })
}

/// "GLASSFALL" or "ESC TO RESUME" as a sentence, for the italic face. Text
/// that already has lower case is left alone.
pub fn soften(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    let lower = s.to_lowercase();
    let mut c = lower.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A world's folder name in words, for notes: "Glassfall", "Random island".
pub fn world_words(name: &str) -> String {
    soften(&world_title(name))
}

/// Menu line type: Cinzel, and its bold for the selected line.
fn line_type(u: f32, lit: bool) -> Type {
    Type::new(if lit { Face::CapsBold } else { Face::Caps }, 1.62 * u, 0.15)
}

fn italic(u: f32, size: f32) -> Type {
    Type::new(Face::Italic, size * u, 0.01)
}

/// The selection mark in the gutter left of a line: a hairline shading from
/// coral to rose, ending in a star. `mid` is the middle of the capitals.
fn mark(hud: &mut Hud, x: f32, mid: f32, u: f32) {
    let p = pal();
    let r = 0.625 * u;
    hud.star(x - 1.0 * u - r, mid, 2.0 * r, p.apricot);
    let t = (0.09 * u).max(1.0);
    let x1 = x - 2.7 * u;
    hud.grad_h(x1 - 3.0 * u, (mid - t * 0.5).round(), 3.0 * u, t, with_a(p.coral, 0.0), with_a(p.rose, 0.9));
}

/// A small triangle pointing down, for a line whose value opens a list.
/// `mid` is the middle of the capitals, `x` its left edge.
pub(crate) fn caret(hud: &mut Hud, x: f32, mid: f32, u: f32, c: [f32; 4]) {
    let t = (0.22 * u).max(1.0);
    let top = (mid - t * 1.5).round();
    for i in 0..3 {
        let wide = (1.5 - 0.5 * i as f32) * u;
        hud.rect((x + (1.5 * u - wide) * 0.5).round(), top + i as f32 * t, wide, t, c);
    }
}

/// How wide a caret is, with the space before it.
pub(crate) fn caret_w(u: f32) -> f32 {
    2.6 * u
}

/// The plum shade the left-hand menus sit on.
fn scrim_left(hud: &mut Hud, w: f32, h: f32) {
    let p = pal();
    hud.grad_h(0.0, 0.0, w * 0.40, h, with_a(p.plum, 0.70), with_a(p.plum, 0.34));
    hud.grad_h(w * 0.40, 0.0, w * 0.24, h, with_a(p.plum, 0.34), with_a(p.plum, 0.0));
}

/// A heading in the logo's gradient.
fn heading_type(u: f32) -> Type {
    Type::new(Face::CapsBold, 3.4 * u, 0.14)
}

/// Where a menu sits: down the left side (title screen and the screens it
/// opens), or centred over the dimmed world (pause menu and its screens).
#[derive(Clone, Copy, PartialEq)]
pub enum Place {
    Left,
    Centre,
}

/// Draw a menu with a heading and a footer line, and record its hit boxes.
pub fn draw_menu(hud: &mut Hud, menu: &mut Menu, heading: &str, items: &[Item], foot: &str, place: Place, w: f32, h: f32) {
    let u = unit(w, h);
    let p = pal();
    let t = line_type(u, false);
    let size = t.size;
    let cap = t.cap();
    let pitch = (1.95 * size).round();
    let vt = italic(u, 2.05);
    let nt = italic(u, 1.7);
    let ht = heading_type(u);
    // Buttons stand in a row of their own under the list, so the table is
    // measured from the list's own lines.
    let rows: Vec<&Item> = items.iter().filter(|i| !i.button).collect();
    let tabled = rows.iter().any(|i| i.value.is_some());
    let label_w = rows.iter().map(|i| Hud::measure(line_type(u, true), &i.label)).fold(0.0, f32::max);
    let value_w = rows
        .iter()
        .filter_map(|i| i.value.as_ref().map(|v| (i.drop, v)))
        .map(|(drop, v)| {
            if drop {
                Hud::measure(vt, v) + caret_w(u)
            } else {
                Hud::measure(vt, &format!("\u{2039}  {v}  \u{203a}"))
            }
        })
        .fold(0.0, f32::max);
    let col_gap = 9.0 * u;
    let table_w = label_w + if tabled { col_gap + value_w } else { 0.0 };
    let cx = (w * 0.5).floor();

    // The buttons stand in a row under the list and must be wholly on screen
    // whatever the list does, so the room the list has is measured from what
    // is left over once they and the footer have theirs.
    let bt = Type::new(Face::CapsBold, 2.15 * u, 0.14);
    let (bpx, bpy) = (3.2 * u, 1.5 * u);
    let bh = (bt.cap() + bpy * 2.0).round();
    let button_space = if items.iter().any(|i| i.button) { bh + 3.2 * u } else { 0.0 };
    let head_h = if heading.is_empty() { 0.0 } else { 6.2 * u };
    let left_top = if heading.is_empty() { 29.0 * u } else { 8.0 * u + ht.cap() + 1.6 * u };
    let room = match place {
        Place::Left => h - 6.0 * u - button_space - left_top,
        Place::Centre => h - 9.0 * u - button_space - head_h - 4.0 * u,
    };
    // A list too long for the window is scrolled instead of running off the
    // bottom of the screen (`Menu::top`, the wheel and the bar below).
    let shown = rows.len().min((room / pitch).floor().max(3.0) as usize);
    menu.shown = shown;
    menu.top = menu.top.min(rows.len() - shown);
    let first = menu.top;

    // Heading and where the list starts.
    let list_top = match place {
        Place::Left => {
            scrim_left(hud, w, h);
            if !heading.is_empty() {
                hud.set_lit(ht, 7.0 * u, 8.0 * u + ht.cap(), Ink::Grad(&p.lit), 0.8, heading);
            }
            left_top
        }
        Place::Centre => {
            hud.rect(0.0, 0.0, w, h, with_a(p.plum, 0.6));
            let total = head_h + pitch * shown as f32 + button_space;
            let y0 = ((h - total) * 0.46).max(2.0 * u).floor();
            if !heading.is_empty() {
                let hw = Hud::measure(ht, heading);
                hud.set_lit(ht, cx - hw * 0.5, y0 + ht.cap(), Ink::Grad(&p.lit), 0.8, heading);
                // A rule shading in and out, with a star in the middle.
                let ry = (y0 + 4.6 * u).round();
                let t1 = (0.09 * u).max(1.0);
                hud.grad_h(cx - 7.0 * u, ry, 7.0 * u, t1, with_a(p.apricot, 0.0), with_a(p.apricot, 0.9));
                hud.grad_h(cx, ry, 7.0 * u, t1, with_a(p.apricot, 0.9), with_a(p.apricot, 0.0));
                hud.star(cx, ry + t1 * 0.5, 1.2 * u, p.apricot);
            }
            y0 + head_h
        }
    };

    menu.at = menu.at.min(items.len().saturating_sub(1));
    menu.hits.clear();
    menu.minus.clear();
    menu.plus.clear();
    menu.drops.clear();
    let x_left = match place {
        Place::Left => 7.0 * u,
        Place::Centre => (cx - table_w * 0.5).floor(),
    };
    for (i, it) in rows.iter().enumerate() {
        // Scrolled out of the window: no line, and a hit box of no size, so
        // the boxes stay parallel to the items for whoever is clicking.
        if i < first || i >= first + shown {
            menu.hits.push([0.0; 4]);
            menu.minus.push([0.0; 4]);
            menu.plus.push([0.0; 4]);
            menu.drops.push([0.0; 4]);
            continue;
        }
        let lit = i == menu.at;
        let ry = (list_top + (i - first) as f32 * pitch).floor();
        let base = (ry + 0.62 * size + cap).round();
        let mid = base - cap * 0.5;
        let lt = line_type(u, lit);
        let ink = if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink) };
        let halo = if lit { 0.9 } else { 0.7 };
        let lw = Hud::measure(lt, &it.label);
        if place == Place::Centre && !tabled {
            // A centred list: stars either side of the selection.
            let x = (cx - lw * 0.5).floor();
            hud.set_lit(lt, x, base, ink, halo, &it.label);
            if lit {
                hud.star(x - 1.1 * u - 0.625 * u, mid, 1.25 * u, p.apricot);
                hud.star(x + lw + 1.1 * u + 0.625 * u, mid, 1.25 * u, p.apricot);
            }
            menu.hits.push([cx - label_w * 0.5 - 4.0 * u, ry, label_w + 8.0 * u, pitch]);
            menu.minus.push([0.0; 4]);
            menu.plus.push([0.0; 4]);
            menu.drops.push([0.0; 4]);
            continue;
        }
        let x = x_left;
        hud.set_lit(lt, x, base, ink, halo, &it.label);
        if lit {
            mark(hud, x, mid, u);
        }
        let mut right = x + lw;
        if let Some(n) = &it.note {
            let nx = x + lw + 1.1 * u;
            right = nx + hud.set_lit(nt, nx, base, Ink::Flat(p.note), 0.6, n);
        }
        let mut minus = [0.0; 4];
        let mut plus = [0.0; 4];
        let mut drop = [0.0; 4];
        if let (Some(v), true) = (&it.value, it.drop) {
            // A dropdown: the value as it stands, with a caret after it to
            // say a list opens here. No arrows, so a click anywhere on the
            // line opens it.
            let vx = x + label_w + col_gap;
            let ink = if lit { p.apricot } else { p.ink_value };
            let end = vx + hud.set_lit(vt, vx, base, Ink::Flat(ink), 0.6, v);
            caret(hud, end + 1.1 * u, mid, u, with_a(ink, if lit { 0.9 } else { 0.6 }));
            right = end + caret_w(u);
            drop = [vx, ry, (right - vx).max(value_w), pitch];
        } else if let Some(v) = &it.value {
            let vx = x + label_w + col_gap;
            // Unselected values start where the selected one's text does,
            // after its left arrow, so nothing jumps as the selection moves.
            right = if lit {
                let aw = Hud::measure(vt, "\u{2039}  ") + 1.2 * u;
                minus = [vx - 0.6 * u, ry, aw, pitch];
                let end = vx + hud.set_lit(vt, vx, base, Ink::Flat(p.apricot), 0.6, &format!("\u{2039}  {v}  \u{203a}"));
                plus = [end - aw + 0.6 * u, ry, aw, pitch];
                end
            } else {
                let vx = vx + Hud::measure(vt, "\u{2039}  ") + vt.track * vt.size;
                vx + hud.set_lit(vt, vx, base, Ink::Flat(p.ink_value), 0.6, v)
            };
        }
        menu.hits.push([x - 6.0 * u, ry, (right - x).max(table_w) + 7.0 * u, pitch]);
        menu.minus.push(minus);
        menu.plus.push(plus);
        menu.drops.push(drop);
    }

    // The buttons: a row under the list, each in a box of its own, bigger
    // than a line so the eye lands on them last.
    let buttons: Vec<(usize, &Item)> = items.iter().enumerate().filter(|(_, i)| i.button).collect();
    if !buttons.is_empty() {
        let (px, py) = (bpx, bpy);
        let gap = 2.2 * u;
        let widths: Vec<f32> = buttons.iter().map(|(_, it)| (Hud::measure(bt, &it.label) + px * 2.0).max(16.0 * u)).collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (buttons.len() as f32 - 1.0);
        // On the left-hand pages the buttons sit on the bottom of the
        // screen, where the eye ends up, rather than wandering up and down
        // with the length of the list. A centred menu keeps them under it.
        let by = match place {
            Place::Left => (h - 6.0 * u - bh).floor(),
            Place::Centre => (list_top + shown as f32 * pitch + 1.6 * u).floor(),
        };
        let mut bx = match place {
            Place::Left => x_left,
            Place::Centre => (cx - total * 0.5).floor(),
        };
        for ((i, it), bw) in buttons.iter().zip(&widths) {
            let lit = *i == menu.at;
            let line = (0.1 * u).max(1.0);
            hud.rect(bx, by, *bw, bh, with_a(p.plum, if lit { 0.82 } else { 0.5 }));
            // The border: the logo's gradient along the top and bottom when
            // the button is the one picked, a quiet hairline when it is not.
            let edge = if lit { with_a(p.apricot, 0.95) } else { with_a(p.ink, 0.35) };
            hud.rect(bx, by, *bw, line, edge);
            hud.rect(bx, by + bh - line, *bw, line, edge);
            hud.rect(bx, by, line, bh, edge);
            hud.rect(bx + *bw - line, by, line, bh, edge);
            let lw = Hud::measure(bt, &it.label);
            let ink = if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink) };
            hud.set_lit(bt, (bx + (*bw - lw) * 0.5).floor(), by + py + bt.cap(), ink, if lit { 0.9 } else { 0.7 }, &it.label);
            menu.hits.push([bx, by, *bw, bh]);
            menu.minus.push([0.0; 4]);
            menu.plus.push([0.0; 4]);
            menu.drops.push([0.0; 4]);
            bx += bw + gap;
        }
    }

    // A bar beside the list when it runs past its window: it says how much
    // more there is and gives the wheel and the mouse something to drive
    // (`Menu::scroll`, `Menu::drag_to`).
    menu.track = [0.0; 4];
    menu.thumb = [0.0; 4];
    if rows.len() > shown {
        let bw = (0.55 * u).max(3.0);
        let bx = match place {
            Place::Left => x_left + table_w + 1.5 * u,
            Place::Centre => (cx + table_w.max(label_w) * 0.5 + 3.0 * u).floor(),
        };
        let by = list_top;
        let bar_h = pitch * shown as f32;
        hud.rect(bx, by, bw, bar_h, with_a(p.apricot, 0.16));
        let max = rows.len() - shown;
        let th = (bar_h * shown as f32 / rows.len() as f32).max(2.0 * u);
        let ty = by + (bar_h - th) * first as f32 / max as f32;
        hud.rect(bx, ty, bw, th, with_a(p.apricot, 0.7));
        menu.track = [bx, by, bw, bar_h];
        menu.thumb = [bx, ty, bw, th];
    }

    if !foot.is_empty() {
        let text = soften(foot);
        let fw = Hud::measure(nt, &text);
        match place {
            Place::Left => {
                let x = w - 7.0 * u - fw;
                let base = h - 4.6 * u;
                hud.set_lit(nt, x, base, Ink::Flat(p.foot), 0.7, &text);
                hud.star(x - 1.0 * u - 0.6 * u, base - nt.size * 0.28, 1.2 * u, p.apricot);
            }
            Place::Centre => {
                hud.set_lit(nt, cx - fw * 0.5, h - 5.0 * u, Ink::Flat(p.note), 0.7, &text);
            }
        }
    }
}

/// A dropdown standing open under the line it belongs to: a plum panel of
/// lines, the one in force marked. `anchor` is that line's value rectangle
/// (`Menu::drops`), and the hit boxes recorded are the panel's own, so the
/// mouse works on the list while it is open.
pub fn draw_dropdown(hud: &mut Hud, menu: &mut Menu, items: &[Item], anchor: [f32; 4], in_force: usize, w: f32, h: f32) {
    let u = unit(w, h);
    let p = pal();
    let lt = Type::new(Face::Caps, 1.5 * u, 0.12);
    let pitch = (2.3 * lt.size).round();
    let pad = 1.4 * u;
    let wide = items.iter().map(|i| Hud::measure(lt, &i.label)).fold(anchor[2], f32::max) + pad * 2.0 + 2.0 * u;
    // Kept on screen: a row near the right edge (the EDIT page's lists) drops
    // a panel that would otherwise hang off it.
    let x = (anchor[0] - pad).min(w - wide - 1.5 * u).max(1.5 * u);
    let y = (anchor[1] + anchor[3] * 0.92).floor();
    // A long list is windowed rather than stretched over the whole screen:
    // as many lines as fit under the one it hangs from, scrolled to keep the
    // line being picked in view.
    let room = ((h - 6.0 * u - y - pad * 2.0) / pitch).floor().max(4.0) as usize;
    let shown = items.len().min(room);
    menu.shown = shown;
    menu.top = menu.top.min(items.len() - shown);
    let first = menu.top;
    let tall = pitch * shown as f32 + pad * 2.0;
    let line = (0.1 * u).max(1.0);
    // Solid: a list that hangs over the EDIT page's panel would otherwise
    // read with that panel's own labels showing through it.
    hud.rect(x, y, wide, tall, with_a(p.plum, 1.0));
    hud.rect(x, y, wide, line, with_a(p.apricot, 0.8));
    hud.rect(x, y + tall - line, wide, line, with_a(p.apricot, 0.5));
    hud.rect(x, y, line, tall, with_a(p.apricot, 0.5));
    hud.rect(x + wide - line, y, line, tall, with_a(p.apricot, 0.5));
    menu.hits.clear();
    menu.minus.clear();
    menu.plus.clear();
    menu.drops.clear();
    for (i, it) in items.iter().enumerate() {
        // Outside the window: no line, and a hit box of no size, so the
        // boxes stay parallel to the items whoever is clicking.
        if i < first || i >= first + shown {
            menu.hits.push([0.0; 4]);
            menu.minus.push([0.0; 4]);
            menu.plus.push([0.0; 4]);
            menu.drops.push([0.0; 4]);
            continue;
        }
        let ry = (y + pad + (i - first) as f32 * pitch).floor();
        let lit = i == menu.at;
        let base = (ry + pitch * 0.74).round();
        if lit {
            hud.grad_h(x + line, ry, wide - line * 2.0, pitch, with_a(p.rose, 0.45), with_a(p.mauve, 0.12));
        }
        let ink = if lit {
            Ink::Grad(&p.lit)
        } else if i == in_force {
            Ink::Flat(p.ink_value)
        } else {
            Ink::Flat(p.ink)
        };
        hud.set_lit(lt, x + pad + 1.0 * u, base, ink, if lit { 0.9 } else { 0.7 }, &it.label);
        // The one in force keeps a star, so the list says where it stands.
        if i == in_force {
            hud.star(x + wide - pad - 0.8 * u, base - lt.cap() * 0.5, 1.2 * u, p.apricot);
        }
        menu.hits.push([x, ry, wide, pitch]);
        menu.minus.push([0.0; 4]);
        menu.plus.push([0.0; 4]);
        menu.drops.push([0.0; 4]);
    }
    // A scrollbar down the right edge when the list runs past the window.
    //
    // What was here was a caret at the bottom of the panel, which reads as
    // a button and is not one: it says there is more below without saying
    // how much more or giving you any way to get at it except by walking
    // the highlight down. A bar says both, and the wheel and the thumb both
    // drive it (main.rs).
    menu.track = [0.0; 4];
    menu.thumb = [0.0; 4];
    if items.len() > shown {
        let bw = (0.55 * u).max(3.0);
        let bx = x + wide - line - bw - 0.35 * u;
        let by = y + pad * 0.5;
        let bh = tall - pad;
        hud.rect(bx, by, bw, bh, with_a(p.apricot, 0.16));
        let max = items.len() - shown;
        let th = (bh * shown as f32 / items.len() as f32).max(2.0 * u);
        let ty = by + (bh - th) * first as f32 / max as f32;
        hud.rect(bx, ty, bw, th, with_a(p.apricot, 0.7));
        menu.track = [bx, by, bw, bh];
        menu.thumb = [bx, ty, bw, th];
    }
}

/// The Lurelia logo, top left, its L on the menu's left edge.
pub fn draw_logo(hud: &mut Hud, w: f32, h: f32) {
    let u = unit(w, h);
    let lw = 36.0 * u;
    // The L's stem sits 7.3% in from the logo's own left edge.
    hud.logo((7.0 * u - lw * 0.073).round(), (5.0 * u).round(), lw, 1.0);
}

/// The controls screen: one box per mode (and one for the world), packed
/// into two columns, keys right-aligned in capitals and what they do in
/// italic, over the dimmed world or the title pictures.
pub fn draw_controls(hud: &mut Hud, boxes: &[(&str, Vec<(String, &str)>)], w: f32, h: f32, foot: &str) {
    let u = unit(w, h);
    let p = pal();
    hud.rect(0.0, 0.0, w, h, with_a(p.plum, 0.62));
    let ht = heading_type(u);
    let cx = (w * 0.5).floor();
    let title = "CONTROLS";
    let tw = Hud::measure(ht, title);
    let y0 = (2.0 * u).floor();
    hud.set_lit(ht, cx - tw * 0.5, y0 + ht.cap(), Ink::Grad(&p.lit), 0.8, title);
    let ry = (y0 + 4.6 * u).round();
    let t1 = (0.09 * u).max(1.0);
    hud.grad_h(cx - 7.0 * u, ry, 7.0 * u, t1, with_a(p.apricot, 0.0), with_a(p.apricot, 0.9));
    hud.grad_h(cx, ry, 7.0 * u, t1, with_a(p.apricot, 0.9), with_a(p.apricot, 0.0));
    hud.star(cx, ry + t1 * 0.5, 1.2 * u, p.apricot);

    let top = ry + 3.0 * u;
    let avail = (h - top - 6.0 * u).max(10.0 * u);
    // Two columns, each box in the shorter one so the page stays even.
    let gutter = 2.4 * u;
    let margin = 4.0 * u;
    let pad = 1.4 * u;
    // Lay the boxes out at scale `g`; the whole page shrinks to fit if the
    // longer column runs past the bottom.
    // The first box is the world, and it is always at the head of the left
    // column: the modes flow after it, down that column and then the right.
    let plan = |g: f32| -> (Vec<(usize, f32, f32)>, f32) {
        let (pitch, head) = (2.0 * u * g, 3.0 * u * g);
        let mut col_h = [0.0f32; 2];
        let mut out = Vec::new();
        for (i, (_, rows)) in boxes.iter().enumerate() {
            let bh = pad * g * 2.0 + head + rows.len() as f32 * pitch;
            let c = if i == 0 { 0 } else { usize::from(col_h[1] < col_h[0]) };
            out.push((c, col_h[c], bh));
            col_h[c] += bh + 1.8 * u * g;
        }
        (out, col_h[0].max(col_h[1]))
    };
    let (_, full) = plan(1.0);
    let g = (avail / full.max(1.0)).min(1.0);
    let (laid, _) = plan(g);
    let (pitch, head, pad) = (2.0 * u * g, 3.0 * u * g, pad * g);
    let bt = Type::new(Face::CapsBold, 1.35 * u * g, 0.14);
    let kt = Type::new(Face::Caps, 1.12 * u * g, 0.1);
    let at = Type::new(Face::Italic, 1.5 * u * g, 0.01);
    let gap = 1.2 * u * g;
    // Each box sets its own key column, and every box is as wide as the
    // widest needs, so the two columns line up without a gulf of space
    // inside the short ones.
    let keys: Vec<f32> = boxes.iter().map(|(_, rows)| rows.iter().map(|r| Hud::measure(kt, &r.0)).fold(0.0, f32::max)).collect();
    let col_w = boxes
        .iter()
        .zip(&keys)
        .map(|((title, rows), kw)| {
            let aw = rows.iter().map(|r| Hud::measure(at, &soften(r.1))).fold(0.0, f32::max);
            (pad * 2.0 + kw + gap + aw).max(pad * 2.0 + Hud::measure(bt, title))
        })
        .fold(0.0, f32::max)
        .min((w - margin * 2.0 - gutter) * 0.5);
    for (((c, y, bh), (title, rows)), key_w) in laid.iter().zip(boxes).zip(&keys) {
        let x = (w * 0.5 - col_w - gutter * 0.5 + *c as f32 * (col_w + gutter)).floor();
        let y = (top + y).floor();
        hud.rect(x, y, col_w, *bh, with_a(p.plum, 0.5));
        let line = (0.07 * u).max(1.0);
        hud.rect(x, y, col_w, line, with_a(p.apricot, 0.5));
        hud.set_lit(bt, x + pad, y + pad + bt.cap(), Ink::Flat(p.apricot), 0.7, title);
        let x_mid = (x + pad + *key_w).floor();
        for (i, (k, text)) in rows.iter().enumerate() {
            let base = (y + pad + head + i as f32 * pitch + pitch * 0.72).round();
            hud.set_lit(kt, x_mid - Hud::measure(kt, k), base, Ink::Flat(p.ink_value), 0.6, k);
            hud.set_lit(at, x_mid + gap, base, Ink::Flat(p.note), 0.6, &soften(text));
        }
    }
    let nt = italic(u, 1.7);
    let text = soften(foot);
    let fw = Hud::measure(nt, &text);
    hud.set_lit(nt, cx - fw * 0.5, h - 3.0 * u, Ink::Flat(p.foot), 0.7, &text);
}

// ---------------------------------------------------------------------------
// Pictures behind the menus.

/// Seconds each picture is shown, and of the cross-fade into the next.
const HOLD: f32 = 9.0;
const FADE: f32 = 1.6;

/// Screenshots cycling behind the menus, cross-faded, with a slow push in.
/// Pictures are decoded and cropped to 16:9 on a background thread.
pub struct Slides {
    paths: Vec<PathBuf>,
    next: usize,
    loader: Option<JoinHandle<Option<Vec<u8>>>>,
    /// The slot showing now (0 or 1) and when it came in; None before the first.
    cur: Option<(usize, f32)>,
    /// The slot fading out, if any.
    prev: Option<usize>,
}

/// Every picture the menus can show: the player's screenshots and camera
/// photos, then each world's bundled pictures in `<world folder>/pictures/`.
/// `world` keeps only that world's.
pub fn picture_paths(world: Option<&str>) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("screenshots"), PathBuf::from("photos")];
    match world {
        Some(w) => dirs.push(crate::worldload::world_dir(w).join("pictures")),
        None => dirs.extend(crate::worldload::available().iter().map(|w| crate::worldload::world_dir(w).join("pictures"))),
    }
    let mut out = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        let bundled = i >= 2;
        let mut v: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
            .filter(|p| {
                // A world's own folder holds only its pictures, whatever their names;
                // the player's are named world-0001.png.
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                bundled || world.map_or(true, |w| stem == w || stem.strip_prefix(w).is_some_and(|r| r.starts_with('-')))
            })
            .collect();
        v.sort();
        out.extend(v);
    }
    out
}

impl Slides {
    pub fn new(mut paths: Vec<PathBuf>, seed: u64) -> Slides {
        // A different order each run: a small shuffle from the seed.
        let mut x = seed | 1;
        for i in (1..paths.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            paths.swap(i, (x % (i as u64 + 1)) as usize);
        }
        let mut s = Slides {
            paths,
            next: 0,
            loader: None,
            cur: None,
            prev: None,
        };
        s.start_next();
        s
    }

    fn start_next(&mut self) {
        if self.paths.is_empty() || self.loader.is_some() {
            return;
        }
        let p = self.paths[self.next % self.paths.len()].clone();
        self.next += 1;
        self.loader = Some(std::thread::spawn(move || load_picture(&p)));
    }

    /// Bring in the next picture when it's time and ready. `t` is seconds
    /// since some fixed start.
    pub fn update(&mut self, renderer: &Renderer, t: f32) {
        let due = match self.cur {
            None => true,
            Some((_, since)) => t - since >= HOLD && self.paths.len() > 1,
        };
        if !due || !self.loader.as_ref().is_some_and(|l| l.is_finished()) {
            return;
        }
        let got = self.loader.take().and_then(|l| l.join().ok()).flatten();
        if let Some(px) = got {
            let slot = self.cur.map_or(0, |(s, _)| 1 - s);
            renderer.upload_backdrop(slot, &px);
            self.prev = self.cur.map(|(s, _)| s);
            self.cur = Some((slot, t));
        }
        if self.paths.len() > 1 || self.cur.is_none() {
            self.start_next();
        }
    }

    /// Show a different set of pictures (the launch screen, when the world
    /// on the line changes). The one on screen stays until the first of the
    /// new set is decoded, so stepping through the worlds crossfades rather
    /// than blinking to black.
    pub fn retarget(&mut self, paths: Vec<PathBuf>, seed: u64, t: f32) {
        let keep = self.cur.map(|(slot, _)| (slot, t - HOLD));
        *self = Slides::new(paths, seed);
        self.cur = keep;
    }

    /// Wait for the first picture (for headless screenshots of the menus).
    pub fn wait_first(&mut self, renderer: &Renderer) {
        if let Some(l) = self.loader.take() {
            if let Some(px) = l.join().ok().flatten() {
                renderer.upload_backdrop(0, &px);
                self.cur = Some((0, -100.0));
            }
        }
    }

    /// Draw the pictures over the whole window (black when there are none).
    pub fn draw(&self, hud: &mut Hud, w: f32, h: f32, t: f32) {
        hud.rect(0.0, 0.0, w, h, [0.03, 0.03, 0.04, 1.0]);
        let Some((slot, since)) = self.cur else {
            return;
        };
        // Cover the window at 16:9, pushing in 6% over a picture's life.
        let cover = |hud: &mut Hud, slot: usize, age: f32, alpha: f32| {
            let zoom = 1.0 + 0.06 * (age / (HOLD + FADE)).clamp(0.0, 1.0);
            let aspect = BACKDROP_W as f32 / BACKDROP_H as f32;
            let (mut bw, mut bh) = (w, w / aspect);
            if bh < h {
                bh = h;
                bw = h * aspect;
            }
            bw *= zoom;
            bh *= zoom;
            hud.backdrop(slot, (w - bw) * 0.5, (h - bh) * 0.5, bw, bh, alpha);
        };
        let age = t - since;
        let fade = (age / FADE).clamp(0.0, 1.0);
        if let (Some(p), true) = (self.prev, fade < 1.0) {
            cover(hud, p, age + HOLD, 1.0);
        }
        cover(hud, slot, age, fade);
    }
}

/// Decode a PNG and crop it to fill BACKDROP_W x BACKDROP_H, as sRGB RGBA.
pub fn load_picture(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (sw, sh) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    let px = |x: usize, y: usize| -> [u8; 3] {
        let i = (y * sw + x) * ch;
        if ch >= 3 {
            [buf[i], buf[i + 1], buf[i + 2]]
        } else {
            [buf[i]; 3]
        }
    };
    let (dw, dh) = (BACKDROP_W as usize, BACKDROP_H as usize);
    // The largest 16:9 window of the source, centred.
    let (cw, chh) = if sw * dh > sh * dw { (sh * dw / dh, sh) } else { (sw, sw * dh / dw) };
    let (ox, oy) = ((sw - cw) / 2, (sh - chh) / 2);
    let f = cw as f32 / dw as f32;
    // Supersample n x n points per output pixel when shrinking.
    let n = (f.ceil() as usize).clamp(1, 4);
    let mut out = vec![255u8; dw * dh * 4];
    for y in 0..dh {
        for x in 0..dw {
            let mut acc = [0u32; 3];
            for j in 0..n {
                for i in 0..n {
                    let sx = ox + (((x as f32 + (i as f32 + 0.5) / n as f32) * f) as usize).min(cw - 1);
                    let sy = oy + (((y as f32 + (j as f32 + 0.5) / n as f32) * f) as usize).min(chh - 1);
                    let p = px(sx, sy);
                    acc[0] += p[0] as u32;
                    acc[1] += p[1] as u32;
                    acc[2] += p[2] as u32;
                }
            }
            let k = (n * n) as u32;
            let o = (y * dw + x) * 4;
            out[o] = (acc[0] / k) as u8;
            out[o + 1] = (acc[1] / k) as u8;
            out[o + 2] = (acc[2] / k) as u8;
        }
    }
    Some(out)
}

/// The loading screen: the world's name, what's happening, and a hairline
/// that fills as the view comes in (or sweeps while the land is being built).
pub fn draw_loading(hud: &mut Hud, world: &str, phase: &str, progress: Option<f32>, w: f32, h: f32, t: f32) {
    let u = unit(w, h);
    let p = pal();
    hud.grad_v(0.0, h * 0.45, w, h * 0.55, with_a(p.plum, 0.0), with_a(p.plum, 0.8));
    let x = 7.0 * u;
    let nt = Type::new(Face::CapsBold, 4.3 * u, 0.12);
    let base = (h - 13.0 * u).round();
    hud.set_lit(nt, x, base, Ink::Flat(hex(0xfff1ea, 1.0)), 0.8, &world_title(world));
    let by = (base + 2.6 * u).round();
    let bw = w - 14.0 * u;
    let bh = (0.12 * u).max(1.0);
    hud.rect(x, by, bw, bh, [1.0, 1.0, 1.0, 0.16]);
    let (a, b) = match progress {
        Some(f) => (x, x + bw * f.clamp(0.0, 1.0)),
        None => {
            let seg = bw * 0.18;
            let sx = x + (bw + seg) * (t * 0.6).fract() - seg;
            (sx.max(x), (sx + seg).min(x + bw))
        }
    };
    if b > a {
        hud.grad_h(a, by, b - a, bh, p.coral, p.mauve);
        hud.star(b, by + bh * 0.5, 1.1 * u, p.apricot);
    }
    hud.set_lit(italic(u, 1.7), x, by + 3.6 * u, Ink::Flat(p.note), 0.7, &soften(phase));
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    /// The game's settings sit flat in settings.json, as they did before
    /// `game::Settings` was split out: a file from then reads into it, and
    /// what is written back has the same keys at the top.
    #[test]
    fn game_settings_read_and_write_flat() {
        let p: Prefs = serde_json::from_str(r#"{"view":96.0,"slopes":false,"show_stats":true,"minimap":true,"shadows":false}"#).unwrap();
        let s = p.settings;
        assert_eq!((s.slopes, s.show_stats, s.minimap, s.shadows, p.view), (false, true, true, false, 96.0));
        let back = serde_json::to_value(&p).unwrap();
        assert_eq!(back["slopes"], false);
        assert_eq!(back["minimap"], true);
        assert!(back.get("settings").is_none(), "the settings were nested rather than flat");
    }

    /// settings.json: a style or a game mode this build does not know costs
    /// that one choice, not every setting in the file; and a file that will
    /// not parse at all is copied aside before the defaults replace it.
    #[test]
    fn settings_survive_what_they_cannot_read() {
        let dir = std::env::temp_dir().join(format!("voxel_settings_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let bad = dir.join("settings.json.bad");

        std::fs::write(&path, r#"{"view":256.0,"vsync":true,"style":"sepia","last":{"world":"hemlow","mode":"capture the flag","kept":{"hemlow/explore":{"hour":"dawn"}}}}"#).unwrap();
        let p = Prefs::load(&dir);
        assert_eq!((p.view, p.vsync), (256.0, true), "an unknown style took the other settings with it");
        assert!(p.settings.style == crate::render::Style::default());
        assert_eq!(p.last.world, "hemlow");
        assert!(p.last.mode == crate::launch::Mode::Explore);
        assert_eq!(p.last.kept.len(), 1, "an unknown mode took the kept choices with it");
        assert!(!bad.exists());

        std::fs::write(&path, "view = 256").unwrap();
        let p = Prefs::load(&dir);
        assert_eq!(p.view, Prefs::default().view);
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), "view = 256");

        // Saving goes through a temporary file and leaves none behind.
        p.save(&dir);
        assert!(!dir.join("settings.tmp").exists());
        assert_eq!(Prefs::load(&dir).view, p.view);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Antialiasing is off or 4x: 2x measured no cheaper than 4x on a 3060
    /// and looks worse, so it is not on offer. A settings file written when
    /// it was lands on 4 rather than on a value no line can show.
    #[test]
    fn antialiasing_is_off_or_four() {
        assert_eq!(SAMPLES, [1, 4]);
        let mut p = Prefs { msaa: 1, ..Default::default() };
        step(&mut p, Setting::Msaa, 1);
        assert_eq!(p.msaa, 4);
        step(&mut p, Setting::Msaa, 1);
        assert_eq!(p.msaa, 1, "it steps round");
        step(&mut p, Setting::Msaa, -1);
        assert_eq!(p.msaa, 4, "and back the other way");
    }
}
