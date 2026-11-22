//! What the launch screen holds: which world, which way of playing it, and
//! the options those two put on offer.
//!
//! The screen itself is drawn by `menu::draw_menu` from the items below, and
//! `main.rs` steps them; this file owns the choices and nothing about the
//! drawing. A choice is kept as text under a `"<world>/<mode>"` key, so
//! coming back to a world finds the settings it was last played with, and an
//! option that no longer exists is simply never asked for again.
//!
//! `Chosen` is the resolved launch: what the loader is handed and what the
//! game keeps. Everything that reads an option reads it off `Chosen`, never
//! off the text map.

use crate::modes::{Allowed, ModeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// A way of playing a world. Explore is the game as it has always been;
/// Fps is the arena deathmatch (src/shooter.rs), on a world that has rules
/// for it in its world.json.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Explore,
    #[serde(alias = "deathmatch")]
    Fps,
    /// Team deathmatch: the same arena, two sides.
    Team,
    /// Capture the flag, on a world whose shooter rules have a `ctf` block.
    Ctf,
    /// Laps against bots, on a world with a `race` block (src/race.rs).
    Race,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Explore
    }
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Explore => "explore",
            Mode::Fps => "fps",
            Mode::Team => "team",
            Mode::Ctf => "ctf",
            Mode::Race => "race",
        }
    }

    /// A match of some kind: the gun is out and the shooter's rules run.
    pub fn is_match(self) -> bool {
        matches!(self, Mode::Fps | Mode::Team | Mode::Ctf)
    }

    /// Opens on the world as built and its spawn, not where the last visit
    /// left off: a match, or a race.
    pub fn fresh_start(self) -> bool {
        self != Mode::Explore
    }

    /// How the launch screen shows it.
    pub fn title(self) -> &'static str {
        match self {
            Mode::Explore => "EXPLORE",
            Mode::Fps => "DEATHMATCH",
            Mode::Team => "TEAM DEATHMATCH",
            Mode::Ctf => "CAPTURE THE FLAG",
            Mode::Race => "RACE",
        }
    }

    /// The plain game of this kind in `library/games/` (src/games.rs).
    pub fn game_name(self) -> &'static str {
        match self {
            Mode::Fps => "deathmatch",
            m => m.name(),
        }
    }

    /// What a world.json has to have for this game to be played there, in
    /// words, for the complaint when it has not.
    pub fn needs(self) -> &'static str {
        match self {
            Mode::Explore => "nothing",
            Mode::Fps | Mode::Team => "a shooter block",
            Mode::Ctf => "a shooter block with a ctf block",
            Mode::Race => "a race block",
        }
    }
}

/// One line of options on the launch screen: a label and the values it steps
/// through. A switch is a two-value option, "Off" then "On".
pub struct Opt {
    pub id: String,
    pub label: String,
    pub help: &'static str,
    pub values: Vec<String>,
    pub default: usize,
}

impl Opt {
    fn switch(id: &str, label: &str, help: &'static str, on: bool) -> Opt {
        Opt {
            id: id.to_string(),
            label: label.to_string(),
            help,
            values: vec!["Off".into(), "On".into()],
            default: usize::from(on),
        }
    }

    fn pick(id: &str, label: &str, help: &'static str, values: &[&str], default: usize) -> Opt {
        Opt {
            id: id.to_string(),
            label: label.to_string(),
            help,
            values: values.iter().map(|v| v.to_string()).collect(),
            default,
        }
    }
}

/// The travel modes the launch screen can switch off, in the order it shows
/// them. Walking is not one of them: every other mode starts and ends on foot.
pub const TRAVEL: [ModeId; 6] = [ModeId::Bike, ModeId::Drive, ModeId::Glide, ModeId::Climb, ModeId::Rope, ModeId::Kayak];

/// The most bots the launch screen offers on a world that does not say
/// (`ShooterDef::max_bots`): about what a small arena holds.
pub const DEFAULT_MAX_BOTS: usize = 7;
/// Target scores the deathmatch options step through.
const TARGETS: [&str; 6] = ["5", "10", "15", "20", "30", "50"];
/// Captures and match lengths capture the flag steps through.
const CAPTURES: [&str; 4] = ["1", "3", "5", "7"];
const MINUTES: [&str; 5] = ["Off", "5", "10", "15", "20"];
const SIDES: [&str; 3] = ["Either", "Cyan", "Magenta"];
/// How the first value of the season and hour lines begins: the one that
/// leaves it to the world. What follows the word says what the world does.
const TURNING: &str = "Turning";
const WORLD: &str = "World";

/// "easy" -> "Easy": the menu's voice for a value that is a word.
fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn travel_help(m: ModeId) -> &'static str {
    match m {
        ModeId::Bike => "Whether the bike is in this world.",
        ModeId::Glide => "Whether the glider is in this world: flight and the wingsuit both.",
        ModeId::Climb => "Whether sheer rock can be climbed.",
        ModeId::Rope => "Whether the rope can be thrown.",
        ModeId::Kayak => "Whether the kayak is in this world.",
        ModeId::Drive => "Whether this world's vehicle is in it.",
        _ => "",
    }
}

/// The options a world and a mode put on offer, in the order they are shown:
/// the world's own first, then the mode's.
pub fn options(world: &str, game: &crate::games::GameDef) -> Vec<Opt> {
    let mode = game.kind;
    let info = info(world);
    // A line that could change nothing is not offered: animals where no
    // biome of the world has any, the clock on a map in space (held at the
    // hour it opens on), the weather where the climate has one kind of day.
    let mut out = Vec::new();
    if info.species > 0 {
        out.push(Opt::switch(
            "animals",
            "ANIMALS",
            "Whether the world's creatures are in it. Off leaves the plants and the land alone.",
            true,
        ));
    }
    if !info.space {
        out.push(Opt::switch(
            "day_night",
            "DAY AND NIGHT",
            "Off holds the world at the hour it opens on, so the light never changes.",
            true,
        ));
    }
    if !info.space && info.weathers != 1 {
        out.push(Opt::switch(
            "weather",
            "WEATHER",
            "Whether the sky changes: rain, snow, fog, heat and cloud come and go. Off keeps fair weather.",
            true,
        ));
    }
    // The parts of the year this world has, where it has more than one: the
    // year turns on its own unless a season is picked, which holds it there.
    if info.seasons.len() > 1 {
        let mut values = vec![format!("{TURNING}, from {}", info.start_season.name())];
        values.extend(info.seasons.iter().map(|s| capitalise(s.name())));
        let values: Vec<&str> = values.iter().map(|v| v.as_str()).collect();
        out.push(Opt::pick(
            "season",
            "SEASON",
            "Which part of the year to open in. Turning lets the year go round on its own, about two days a season.",
            &values,
            0,
        ));
    }
    // The hour to open on, which the world's own is the first offer of.
    let own = info.start_hour.unwrap_or(crate::world::clock::START);
    let mut hours = vec![format!("{WORLD}, {:02}:{:02}", own.floor() as u32, ((own.fract() * 60.0).round() as u32).min(59))];
    hours.extend(crate::world::clock::PRESETS.iter().map(|(_, n)| capitalise(&n.to_lowercase())));
    let hours: Vec<&str> = hours.iter().map(|h| h.as_str()).collect();
    if !info.space {
        out.push(Opt::pick("hour", "TIME", "The hour the world opens at. World is the hour it was written to be seen in.", &hours, 0));
    }
    for m in TRAVEL {
        if info.allowed.has(m) {
            out.push(Opt::switch(&format!("travel.{}", m.name()), &m.name().to_uppercase(), travel_help(m), true));
        }
    }
    // Which vehicle U brings, where the world has more than one.
    if info.allowed.has(ModeId::Drive) && info.vehicles.len() > 1 {
        let names: Vec<String> = info.vehicles.iter().map(|v| capitalise(&v.replace('_', " "))).collect();
        let names: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        out.push(Opt::pick("vehicle", "VEHICLE", "Which of this world's vehicles U brings.", &names, 0));
    }
    if mode.is_match() {
        // The levels are shown in the menu's own voice (one capital), and
        // lowercased again on the way back to `brain::person::level`.
        let levels: Vec<String> = crate::brain::person::NAMES.iter().map(|n| capitalise(n)).collect();
        let levels: Vec<&str> = levels.iter().map(|s| s.as_str()).collect();
        // The world's own roster is what the lines start on, so leaving them
        // alone plays the match the world was written for.
        let at = info.skill.as_ref().and_then(|s| levels.iter().position(|l| l.eq_ignore_ascii_case(s))).unwrap_or(2);
        out.push(Opt::pick(
            "difficulty",
            "DIFFICULTY",
            "How well the bots shoot, from a novice to one that sees behind itself.",
            &levels,
            at,
        ));
        // Zero to as many as the world says it holds, and no more than there
        // are names for.
        let most = info.max_bots.min(BOT_NAMES.len());
        let counts: Vec<String> = (0..=most).map(|n| n.to_string()).collect();
        let counts: Vec<&str> = counts.iter().map(|s| s.as_str()).collect();
        let bots = if info.bots > 0 { info.bots.min(most) } else { 3.min(most) };
        out.push(Opt::pick("bots", "BOTS", "How many of them are in the match.", &counts, bots));
        if mode == Mode::Ctf {
            out.push(Opt::pick("captures", "CAPTURES", "Flags carried home that win the match.", &CAPTURES, game.captures.and_then(|c| CAPTURES.iter().position(|v| *v == c.to_string())).unwrap_or(1)));
            out.push(Opt::pick("minutes", "TIME LIMIT", "Minutes a match lasts; the side ahead when it runs out wins.", &MINUTES, 2));
        } else {
            out.push(Opt::pick(
                "target",
                "TARGET SCORE",
                if mode == Mode::Team {
                    "Kills between a side that win the match."
                } else {
                    "Kills that win the match. The scoreboard comes up when someone gets there."
                },
                &TARGETS,
                game.target.and_then(|t| TARGETS.iter().position(|v| *v == t.to_string())).unwrap_or(if mode == Mode::Team { 3 } else { 2 }),
            ));
        }
        if mode != Mode::Fps {
            out.push(Opt::pick("side", "YOUR SIDE", "Which side you play on. Either puts you on the shorter one.", &SIDES, 0));
        }
    }
    out
}

/// The games a world offers, the one it is built for first.
///
/// A world says so itself: `"games": ["ctf", "team"]` in its world.json,
/// each a name in `library/games/` (src/games.rs). A game it names and
/// cannot host (capture the flag with no flags) is left out;
/// `games_problems` is what reports that. A world that says nothing is one
/// to explore, and is offered the plain game of every kind its world.json
/// has the blocks for, as every world was before the list existed.
pub fn games(world: &str) -> Vec<&'static crate::games::GameDef> {
    let i = info(world);
    let listed: Vec<_> = i.games.iter().filter_map(|n| crate::games::get(n)).filter(|g| i.hosts(g.kind)).collect();
    if !listed.is_empty() {
        return listed;
    }
    crate::games::KINDS.into_iter().filter(|m| i.hosts(*m)).map(crate::games::base).collect()
}

/// Every game some world on the list offers: what the launch screen's GAME
/// line picks from. The plain games in the order of their kinds, each
/// followed by its variants.
pub fn all_games() -> Vec<&'static crate::games::GameDef> {
    let mut out: Vec<&'static crate::games::GameDef> = Vec::new();
    for w in crate::worldload::available() {
        for g in games(&w) {
            if !out.iter().any(|o| o.name == g.name) {
                out.push(g);
            }
        }
    }
    out.sort_by_key(|g| (crate::games::KINDS.iter().position(|k| *k == g.kind), g.name != g.kind.game_name(), g.name.clone()));
    out
}

/// The worlds on the list that offer a game, in the list's order: what the
/// launch screen's WORLD line picks from once the game is chosen.
pub fn worlds_for(game: &str) -> Vec<String> {
    crate::worldload::available().into_iter().filter(|w| games(w).iter().any(|g| g.name == game)).collect()
}

/// The kinds of game a world offers, in the order `games` has them.
#[cfg(test)]
pub fn modes(world: &str) -> Vec<Mode> {
    let mut out = Vec::new();
    for g in games(world) {
        if !out.contains(&g.kind) {
            out.push(g.kind);
        }
    }
    out
}

/// The game a world opens in when nobody has said: the first it offers.
pub fn default_game(world: &str) -> &'static crate::games::GameDef {
    games(world).first().copied().unwrap_or_else(|| crate::games::base(Mode::Explore))
}

/// What is wrong with a world.json's `games` list: a name that is no game,
/// or a game the world has no block for.
pub fn games_problems(world: &str) -> Vec<String> {
    let i = info(world);
    let mut out = Vec::new();
    for n in &i.games {
        match crate::games::get(n) {
            None => out.push(format!("world.json games: no game called {n} in library/games")),
            Some(g) if !i.hosts(g.kind) => out.push(format!("world.json games: {n} needs {}", g.kind.needs())),
            Some(_) => {}
        }
    }
    out
}

/// The launch screen's state, kept in settings.json: where RETURN goes, and
/// what each world and mode was last played with.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Launch {
    pub world: String,
    /// The game, by its name in `library/games/`. Older settings called it
    /// `mode`. One this build does not know, or this world does not offer,
    /// is read as the world's own (`Launch::def`), not a reason to lose the
    /// rest of settings.json.
    #[serde(alias = "mode", deserialize_with = "crate::util::or_default")]
    pub game: String,
    /// Choices by `"<world>/<game>"`, then by option id.
    pub kept: BTreeMap<String, BTreeMap<String, String>>,
    /// The worlds last opened, newest first: the top of the world list.
    pub recent: Vec<String>,
    /// The world each game was last played in, by the game's name: where
    /// picking that game goes back to.
    pub maps: BTreeMap<String, String>,
}

impl Default for Launch {
    fn default() -> Self {
        Launch {
            world: crate::worldload::DEFAULT_WORLD.to_string(),
            game: Mode::Explore.game_name().to_string(),
            kept: BTreeMap::new(),
            recent: Vec::new(),
            maps: BTreeMap::new(),
        }
    }
}

/// How many worlds the top of the list remembers.
const RECENT: usize = 3;

/// One line of the world list: a world, and the heading it is the first
/// line under, if it is.
pub struct WorldRow {
    pub world: String,
    pub head: Option<&'static str>,
}

impl Launch {
    /// A world was opened: it goes to the front of the recent ones.
    pub fn opened(&mut self, world: &str) {
        self.recent.retain(|w| w != world);
        self.recent.insert(0, world.to_string());
        self.recent.truncate(RECENT);
        self.maps.insert(self.def().name.clone(), world.to_string());
    }

    /// The world list as the dropdown shows it: the worlds that offer the
    /// game in force, and no others. The last few opened first, where the
    /// list is long enough for that to save a search, then each under its
    /// kind. A recent world is in the list twice.
    pub fn world_rows(&self) -> Vec<WorldRow> {
        let all = worlds_for(&self.def().name);
        let mut out = Vec::new();
        let recent: Vec<&String> = self.recent.iter().filter(|w| all.contains(w)).collect();
        if all.len() > 8 && recent.len() > 1 {
            for (i, w) in recent.iter().enumerate() {
                out.push(WorldRow {
                    world: (*w).clone(),
                    head: (i == 0).then_some("recent"),
                });
            }
        }
        let mut last = None;
        for w in all {
            let kind = info(&w).kind();
            out.push(WorldRow {
                head: (last != Some(kind)).then_some(kind.title()),
                world: w,
            });
            last = Some(kind);
        }
        out
    }

    /// Whether an option stands somewhere other than its default.
    pub fn changed(&self, o: &Opt) -> bool {
        self.index(o) != o.default
    }

    /// Forget what this world and mode were last played with.
    pub fn reset(&mut self) {
        let key = self.key();
        self.kept.remove(&key);
    }

    fn key(&self) -> String {
        format!("{}/{}", self.world, self.def().name)
    }

    /// Where an option's value is now: what was kept, or its default.
    pub fn index(&self, o: &Opt) -> usize {
        self.kept
            .get(&self.key())
            .and_then(|m| m.get(&o.id))
            .and_then(|v| o.values.iter().position(|x| x == v))
            .unwrap_or(o.default)
    }

    pub fn value(&self, o: &Opt) -> String {
        o.values.get(self.index(o)).cloned().unwrap_or_default()
    }

    /// Step one option, wrapping round.
    pub fn step(&mut self, o: &Opt, dir: i32) {
        let n = o.values.len() as i32;
        if n == 0 {
            return;
        }
        let at = (self.index(o) as i32 + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize;
        let key = self.key();
        self.kept.entry(key).or_default().insert(o.id.clone(), o.values[at].clone());
    }

    /// Step the world, wrapping round the worlds that offer the game in
    /// force.
    pub fn step_world(&mut self, dir: i32) {
        let list = worlds_for(&self.def().name);
        if list.is_empty() {
            return;
        }
        let n = list.len() as i32;
        let at = list.iter().position(|w| *w == self.world).unwrap_or(0) as i32;
        let next = list[(at + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize].clone();
        self.set_world(&next);
    }

    /// Pick a world. The game stays if this world offers it, and is the
    /// world's own first choice if not, so an arena never opens as a place
    /// to explore because the last world was one.
    pub fn set_world(&mut self, world: &str) {
        self.world = world.to_string();
        self.settle();
    }

    /// Put the game on one this world offers. settings.json may name a game
    /// the world has stopped offering since it was written.
    pub fn settle(&mut self) {
        self.game = self.def().name.clone();
    }

    /// The game in force: the one named, where this world offers it, and
    /// the world's own first where it does not.
    pub fn def(&self) -> &'static crate::games::GameDef {
        let named = crate::games::get(&self.game).map(|g| g.name.as_str());
        games(&self.world).into_iter().find(|g| Some(g.name.as_str()) == named).unwrap_or_else(|| default_game(&self.world))
    }

    /// Pick a game. The world stays if it offers the game; if not, it is
    /// the world this game was last played in, or the first that offers it.
    pub fn set_game(&mut self, name: &str) {
        self.game = name.to_string();
        if !games(&self.world).iter().any(|g| g.name == name) {
            let list = worlds_for(name);
            let last = self.maps.get(name).filter(|w| list.contains(w)).cloned();
            if let Some(w) = last.or_else(|| list.first().cloned()) {
                self.world = w;
            }
        }
        self.settle();
    }

    /// Step the game, wrapping round every game some world offers. The
    /// world follows where it has to (`set_game`).
    pub fn step_mode(&mut self, dir: i32) {
        let list = all_games();
        let n = list.len() as i32;
        if n == 0 {
            return;
        }
        let now = self.def();
        let at = list.iter().position(|g| g.name == now.name).unwrap_or(0) as i32;
        let next = list[(at + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize].name.clone();
        self.set_game(&next);
    }

    /// Everything the loader and the game need, read off the options this
    /// world and mode offer now.
    pub fn resolve(&self) -> Chosen {
        // A game the world offers, whatever settings.json kept.
        let game = self.def();
        let opts = options(&self.world, game);
        let on = |id: &str| opts.iter().find(|o| o.id == id).is_some_and(|o| self.value(o) == "On");
        // A line that was not offered has nothing to switch off, except the
        // clock of a map in space, which is held.
        let on_or = |id: &str, absent: bool| if opts.iter().any(|o| o.id == id) { on(id) } else { absent };
        let num = |id: &str, fallback: u32| -> u32 { opts.iter().find(|o| o.id == id).and_then(|o| self.value(o).parse().ok()).unwrap_or(fallback) };
        // A pick whose first value means "leave it to the world".
        let pick = |id: &str| -> Option<String> { opts.iter().find(|o| o.id == id).map(|o| self.value(o)).filter(|v| !v.starts_with(TURNING) && !v.starts_with(WORLD)) };
        let off = TRAVEL
            .into_iter()
            .filter(|m| opts.iter().any(|o| o.id == format!("travel.{}", m.name())) && !on(&format!("travel.{}", m.name())))
            .collect();
        Chosen {
            world: self.world.clone(),
            mode: game.kind,
            game: game.name.clone(),
            animals: on_or("animals", true),
            day_night: on_or("day_night", false),
            weather: on_or("weather", true),
            off,
            difficulty: opts.iter().find(|o| o.id == "difficulty").map_or_else(|| "average".to_string(), |o| self.value(o).to_lowercase()),
            bots: num("bots", 3) as usize,
            target: num("target", 15),
            captures: num("captures", 3),
            minutes: num("minutes", 0),
            side: opts
                .iter()
                .find(|o| o.id == "side")
                .and_then(|o| SIDES.iter().position(|v| *v == self.value(o)))
                .and_then(|k| k.checked_sub(1))
                .map(|k| k as u8),
            season: pick("season").and_then(|v| crate::world::season::Season::parse(&v.to_lowercase())).map(|s| s.name().to_string()),
            hour: pick("hour").and_then(|v| crate::world::clock::PRESETS.iter().find(|(_, n)| n.eq_ignore_ascii_case(&v)).map(|(h, _)| *h)),
            vehicle: opts.iter().find(|o| o.id == "vehicle").map(|o| self.value(o).to_lowercase().replace(' ', "_")),
        }
    }
}

/// A resolved launch: what the loading thread is handed, and what the running
/// game keeps so it can answer "is the clock running", "are there animals"
/// and "whose rules are these".
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub world: String,
    /// The kind of game: which code runs it.
    pub mode: Mode,
    /// The game, by its name in `library/games/`: whose rules these are.
    pub game: String,
    pub animals: bool,
    pub day_night: bool,
    /// Whether the weather drifts (world/weather.rs). Off holds fair weather.
    pub weather: bool,
    /// Travel modes switched off for this run.
    pub off: Vec<ModeId>,
    pub difficulty: String,
    pub bots: usize,
    pub target: u32,
    /// Capture the flag: captures that win, and minutes a match lasts (0
    /// for no limit).
    pub captures: u32,
    pub minutes: u32,
    /// The player's side in a team match; None for the shorter one.
    pub side: Option<u8>,
    /// The season to hold the world in (world/season.rs), by name. None lets
    /// the year turn on its own.
    pub season: Option<String>,
    /// The hour to open at. None takes the world's own.
    pub hour: Option<f32>,
    /// The vehicle U brings, by library name. None takes the world's first.
    pub vehicle: Option<String>,
}

/// Names and shell colours for the bots a match is asked for, in the order
/// they are handed out. Foundry words, since the first arena is a smelter.
/// As many as the biggest map's `max_bots`.
pub const BOT_NAMES: [&str; 15] = [
    "DROSSEL", "KILN", "TUYERE", "SCORIA", "CINDER", "BOSH", "LADLE", "FLUX", "INGOT", "CUPOLA", "SPRUE", "BELLOWS", "TAPHOLE", "GANGUE", "CRUCIBLE",
];
const BOT_COLOURS: [[u8; 3]; 15] = [
    [196, 84, 64],
    [96, 150, 196],
    [206, 176, 88],
    [128, 196, 132],
    [186, 116, 196],
    [220, 140, 80],
    [120, 196, 196],
    [232, 120, 150],
    [150, 120, 220],
    [176, 210, 96],
    [90, 120, 170],
    [210, 210, 210],
    [170, 110, 80],
    [100, 196, 160],
    [230, 180, 200],
];
/// Each bot's lean toward pressing (`bold`) and holding steady, alternating
/// so the roster has pressers and holders in it.
const BOT_BOLD: [f32; 15] = [0.0, 0.5, -0.4, 0.3, -0.2, 0.6, -0.5, 0.2, -0.3, 0.4, -0.1, 0.1, -0.6, 0.45, -0.25];
const BOT_STEADY: [f32; 15] = [0.0, -0.3, 0.5, 0.2, -0.5, 0.4, 0.3, -0.2, 0.1, -0.4, 0.5, -0.1, 0.2, -0.35, 0.35];

impl Chosen {
    /// The defaults for a world opened without the launch screen (the command
    /// line, the headless tools): everything on, and the mode the world is
    /// built for, so `--world slagdeck` still arrives holding a gun.
    pub fn for_world(world: &str) -> Chosen {
        let info = info(world);
        // The game the world is built for: the first of its `games`. A world
        // with no list is one made before there was one, where the blocks
        // it has say what it is for.
        let game = if !info.games.is_empty() {
            default_game(world)
        } else {
            crate::games::base(if info.race {
                Mode::Race
            } else if info.ctf {
                Mode::Ctf
            } else if info.shooter {
                Mode::Fps
            } else {
                Mode::Explore
            })
        };
        Chosen {
            world: world.to_string(),
            mode: game.kind,
            game: game.name.clone(),
            animals: true,
            day_night: true,
            weather: true,
            off: Vec::new(),
            // The world's own roster, so a match started from the command
            // line is the one the world was written for.
            difficulty: info.skill.clone().unwrap_or_else(|| "average".to_string()),
            bots: if info.bots > 0 { info.bots } else { 3 },
            target: game.target.unwrap_or(15),
            // Nothing: the world's own capture count and clock.
            captures: 0,
            minutes: 0,
            side: None,
            season: None,
            hour: None,
            vehicle: None,
        }
    }

    /// Play this run as another game.
    pub fn set_game(&mut self, g: &crate::games::GameDef) {
        self.mode = g.kind;
        self.game = g.name.clone();
    }

    /// The game this run is played as.
    pub fn def(&self) -> &'static crate::games::GameDef {
        crate::games::get(&self.game).filter(|g| g.kind == self.mode).unwrap_or_else(|| crate::games::base(self.mode))
    }

    /// Two launches that would open the same run: the same world, the same
    /// mode and the same options. The launch screen uses it to go back to the
    /// world on screen rather than load it again.
    pub fn same_run(&self, o: &Chosen) -> bool {
        self == o
    }

    /// True when this launch asks for exactly the roster the world's own
    /// rules name, so the world's bots (their names, colours and characters)
    /// can be played as written instead of being replaced.
    pub fn world_roster(&self, world_bots: &[crate::world::fields::BotDef]) -> bool {
        !world_bots.is_empty() && world_bots.len() == self.bots && world_bots.iter().all(|b| b.skill.eq_ignore_ascii_case(&self.difficulty))
    }

    /// The bots a match is asked for, as a world's own `bots` list would
    /// have said it: one difficulty across the roster, each with a name, a
    /// colour and a little character of its own, so two bots of the same
    /// skill do not play identically.
    pub fn roster(&self) -> Vec<crate::world::fields::BotDef> {
        (0..self.bots.min(BOT_NAMES.len()))
            .map(|i| crate::world::fields::BotDef {
                name: BOT_NAMES[i].to_string(),
                skill: self.difficulty.clone(),
                colour: Some(BOT_COLOURS[i]),
                bold: BOT_BOLD[i],
                steady: BOT_STEADY[i],
                without: Vec::new(),
                tune: Default::default(),
                team: None,
            })
            .collect()
    }

    /// The modes this run allows: what the world allows, less what the launch
    /// screen switched off, less the shooter when this is not a match.
    pub fn allowed(&self, world: Allowed) -> Allowed {
        let mut a = world;
        for m in &self.off {
            a = a.without(*m);
        }
        if !self.mode.is_match() {
            a = a.without(ModeId::Shooter);
        }
        if self.mode != Mode::Race {
            a = a.without(ModeId::Race);
        }
        a
    }
}

/// The kinds of world the list sorts them under.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    /// A place to explore, shipped in `worlds/`.
    World,
    /// A map with shooter rules.
    Arena,
    /// Everything in `local/worlds/` that is not an arena: tests, imports,
    /// the random island.
    Workshop,
}

impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Kind::World => "worlds",
            Kind::Arena => "arenas",
            Kind::Workshop => "workshop",
        }
    }
}

/// What the launch screen needs to know about a world without loading it:
/// which modes its world.json allows, and whether it has shooter rules.
#[derive(Clone)]
pub struct Info {
    pub allowed: Allowed,
    pub shooter: bool,
    /// The parts of the year this world has, in the order the year goes
    /// round (world/season.rs).
    pub seasons: Vec<crate::world::season::Season>,
    /// How many bots the world's own rules ask for, and at what skill: the
    /// defaults the launch screen offers, so leaving the lines alone plays
    /// the match the world was written for.
    pub bots: usize,
    pub skill: Option<String>,
    /// The most bots the launch screen offers (`ShooterDef::max_bots`).
    pub max_bots: usize,
    /// Whether its shooter rules carry a `ctf` block.
    pub ctf: bool,
    /// The name it is shown by (world.json `title`), where the folder's own
    /// name in capitals is not it.
    pub title: Option<String>,
    /// What the screen says about it under the list: a sentence on what the
    /// place is (world.json `blurb`, else the opening of its story.md), then
    /// a line of facts. Leaked once per world, so a menu line can hold it.
    pub foot: &'static str,
    /// world.json `"listed": false` keeps a world off the list (a bench, a
    /// test); `--world` still opens it.
    pub listed: bool,
    /// Whether it lives in `local/worlds/`.
    pub local: bool,
    /// How many species its biomes name between them.
    pub species: usize,
    /// How many kinds of day its climate has; 0 where it does not say.
    pub weathers: usize,
    /// A map floating in space: the clock is held and the sky never changes.
    pub space: bool,
    /// The hour it opens at, where it says.
    pub start_hour: Option<f32>,
    /// The season a new game opens in.
    pub start_season: crate::world::season::Season,
    /// Its vehicles (world.json `vehicles`).
    pub vehicles: Vec<String>,
    /// Whether it has a race (world.json `race`).
    pub race: bool,
    /// The games it says it is for (world.json `games`), by their names in
    /// `library/games/`, its own first. Empty where it does not say.
    pub games: Vec<String>,
}

impl Info {
    /// Whether this world has what a game needs (`Mode::needs`).
    pub fn hosts(&self, m: Mode) -> bool {
        match m {
            Mode::Explore => true,
            Mode::Fps | Mode::Team => self.shooter,
            Mode::Ctf => self.ctf,
            Mode::Race => self.race,
        }
    }

    pub fn kind(&self) -> Kind {
        if self.shooter {
            Kind::Arena
        } else if self.local {
            Kind::Workshop
        } else {
            Kind::World
        }
    }
}

#[derive(Deserialize)]
struct Peek {
    #[serde(default)]
    modes: Option<Vec<String>>,
    #[serde(default)]
    games: Vec<String>,
    #[serde(default)]
    vehicles: Vec<String>,
    #[serde(default)]
    shooter: Option<PeekShooter>,
    #[serde(default)]
    race: Option<serde_json::Value>,
    #[serde(default)]
    seasons: Vec<String>,
    #[serde(default)]
    season: String,
    #[serde(default)]
    start_season: Option<String>,
    #[serde(default)]
    start_hour: Option<f32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    blurb: Option<String>,
    #[serde(default)]
    listed: Option<bool>,
    #[serde(default)]
    space: bool,
    #[serde(default)]
    size_m: Option<f32>,
    #[serde(default)]
    weather: BTreeMap<String, f32>,
    #[serde(default)]
    biomes: Vec<String>,
    #[serde(default)]
    default_biome: Option<String>,
    #[serde(default)]
    seabed_biome: Option<String>,
}

/// How many biomes a world.json names, and how many species those biomes
/// name between them, read from the library's biome files.
fn peek_life(p: &Peek) -> (usize, usize) {
    #[derive(Deserialize)]
    struct B {
        #[serde(default)]
        creatures: Vec<C>,
    }
    #[derive(Deserialize)]
    struct C {
        creature: String,
    }
    let dir = crate::library::data_root().join("library/biomes");
    let biomes: std::collections::BTreeSet<&String> = p.biomes.iter().chain(&p.default_biome).chain(&p.seabed_biome).collect();
    let mut species = std::collections::BTreeSet::new();
    for b in &biomes {
        let read: Option<B> = std::fs::read_to_string(dir.join(format!("{b}.json"))).ok().and_then(|t| serde_json::from_str(&t).ok());
        species.extend(read.into_iter().flat_map(|b| b.creatures).map(|c| c.creature));
    }
    (biomes.len(), species.len())
}

/// The first sentence of a world's story.md, for a world whose world.json
/// has no `blurb`. Headings are skipped, and so is a story that is a note
/// about how the map was built.
fn story_opening(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("story.md")).ok()?;
    let para: Vec<&str> = text.lines().map(str::trim).skip_while(|l| l.is_empty() || l.starts_with('#')).take_while(|l| !l.is_empty()).collect();
    let para = para.join(" ");
    if para.is_empty() || para.contains('`') {
        return None;
    }
    let end = para.find(". ").map_or(para.len(), |i| i + 1);
    Some(para[..end].to_string())
}

fn count_word(n: usize) -> String {
    const WORDS: [&str; 5] = ["no", "one", "two", "three", "four"];
    WORDS.get(n).map_or_else(|| n.to_string(), |w| w.to_string())
}

/// "1.5 km across, 17 biomes, 43 species, four seasons".
fn facts(p: &Peek, biomes: usize, species: usize, seasons: &[crate::world::season::Season], max_bots: usize) -> String {
    let mut out = Vec::new();
    if let Some(m) = p.size_m.filter(|m| *m > 0.0) {
        out.push(if m >= 1000.0 {
            let km = format!("{:.1}", m / 1000.0);
            format!("{} km across", km.trim_end_matches(".0"))
        } else {
            format!("{m:.0} m across")
        });
    }
    if p.shooter.is_some() {
        out.push(format!("up to {max_bots} bots"));
        if species > 0 {
            out.push(format!("{species} species"));
        }
    } else {
        out.push(format!("{biomes} biome{}", if biomes == 1 { "" } else { "s" }));
        out.push(format!("{species} species"));
        out.push(match seasons {
            [one] => format!("always {}", one.name()),
            many => format!("{} seasons", count_word(many.len())),
        });
    }
    let mut c = out.join(", ");
    if let Some(f) = c.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    c
}

#[derive(Deserialize)]
struct PeekShooter {
    #[serde(default)]
    bots: Vec<PeekBot>,
    #[serde(default)]
    ctf: Option<serde_json::Value>,
    #[serde(default)]
    max_bots: Option<usize>,
}

#[derive(Deserialize)]
struct PeekBot {
    #[serde(default)]
    skill: Option<String>,
}

/// A world's `Info`, read from its world.json the first time it is asked for.
/// The launch screen asks every frame, so this is cached; nothing it reads
/// changes while the game runs.
pub fn info(world: &str) -> Info {
    static CACHE: Mutex<Option<BTreeMap<String, Info>>> = Mutex::new(None);
    let mut lock = CACHE.lock().unwrap();
    let cache = lock.get_or_insert_with(BTreeMap::new);
    if let Some(i) = cache.get(world) {
        return i.clone();
    }
    let dir = crate::worldload::world_dir(world);
    let local = dir.strip_prefix(crate::library::data_root()).unwrap_or(&dir).starts_with("local");
    let peek: Option<Peek> = std::fs::read_to_string(dir.join("world.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let info = match peek {
        Some(p) => {
            use crate::world::season::Season;
            let seasons = peek_seasons(&p);
            let (biomes, species) = peek_life(&p);
            let max_bots = p.shooter.as_ref().and_then(|sh| sh.max_bots).unwrap_or(DEFAULT_MAX_BOTS);
            let blurb = p.blurb.clone().or_else(|| story_opening(&dir)).unwrap_or_default();
            let facts = facts(&p, biomes, species, &seasons, max_bots);
            let start_season = p
                .start_season
                .as_deref()
                .and_then(Season::parse)
                .filter(|s| seasons.contains(s))
                .or_else(|| seasons.contains(&Season::Summer).then_some(Season::Summer))
                .or_else(|| seasons.first().copied())
                .unwrap_or(Season::Summer);
            Info {
                title: p.title.clone(),
                foot: leak(if blurb.is_empty() { facts } else { format!("{blurb}\n{facts}") }),
                listed: p.listed.unwrap_or(true),
                local,
                species,
                weathers: p.weather.values().filter(|w| **w > 0.0).count(),
                space: p.space,
                start_hour: p.start_hour,
                start_season,
                max_bots,
                allowed: {
                    let a = match &p.modes {
                        Some(names) => Allowed::from_names(names, |_| {}),
                        None => Allowed::all(),
                    };
                    if p.vehicles.is_empty() {
                        a.without(ModeId::Drive)
                    } else {
                        a
                    }
                },
                vehicles: p.vehicles.clone(),
                race: p.race.is_some(),
                games: p.games.clone(),
                shooter: p.shooter.is_some(),
                seasons,
                bots: p.shooter.as_ref().map_or(0, |sh| sh.bots.len()),
                skill: p.shooter.as_ref().and_then(|sh| sh.bots.first().and_then(|b| b.skill.clone())),
                ctf: p.shooter.as_ref().is_some_and(|sh| sh.ctf.is_some()),
            }
        }
        // The random island, and a world whose world.json will not parse: the
        // loader is the one that reports that, so offer the usual game.
        None => Info {
            allowed: Allowed::all().without(ModeId::Drive),
            vehicles: Vec::new(),
            race: false,
            games: Vec::new(),
            shooter: false,
            seasons: vec![crate::world::season::Season::Summer],
            bots: 0,
            skill: None,
            max_bots: DEFAULT_MAX_BOTS,
            ctf: false,
            title: None,
            foot: if world == "random" {
                "An island made from a seed: different every time one is asked for."
            } else {
                ""
            },
            listed: true,
            local: true,
            // Not counted, and not none: the island has its own animals.
            species: 1,
            weathers: 0,
            space: false,
            start_hour: None,
            start_season: crate::world::season::Season::Summer,
        },
    };
    cache.insert(world.to_string(), info.clone());
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The options of a kind's plain game.
    fn options(world: &str, mode: Mode) -> Vec<Opt> {
        super::options(world, crate::games::base(mode))
    }

    fn launch(world: &str, mode: Mode) -> Launch {
        Launch {
            world: world.to_string(),
            game: mode.game_name().to_string(),
            kept: BTreeMap::new(),
            recent: Vec::new(),
            maps: BTreeMap::new(),
        }
    }

    #[test]
    fn a_kept_choice_belongs_to_one_world_and_mode() {
        let mut l = launch("glassfall", Mode::Explore);
        let opts = options("glassfall", Mode::Explore);
        let animals = opts.iter().find(|o| o.id == "animals").expect("animals");
        assert_eq!(l.value(animals), "On");
        l.step(animals, 1);
        assert_eq!(l.value(animals), "Off");
        assert!(!l.resolve().animals);
        // Another world keeps its own answer, and the first one remembers.
        l.world = "redrock".to_string();
        assert_eq!(l.value(animals), "On");
        l.world = "glassfall".to_string();
        assert_eq!(l.value(animals), "Off");
    }

    #[test]
    fn deathmatch_options_only_come_with_the_mode() {
        let ids = |m| options("slagdeck", m).into_iter().map(|o| o.id).collect::<Vec<_>>();
        assert!(!ids(Mode::Explore).contains(&"bots".to_string()));
        assert!(ids(Mode::Fps).contains(&"bots".to_string()));
        // Slagdeck allows walking, the shooter and screenshots: no travel lines.
        assert!(!ids(Mode::Fps).iter().any(|i| i.starts_with("travel.")));
    }

    #[test]
    fn explore_takes_the_gun_away_and_a_switch_takes_a_mode_away() {
        let world = Allowed::all();
        let mut c = Chosen::for_world("slagdeck");
        assert_eq!(c.mode, Mode::Fps, "slagdeck has shooter rules");
        assert!(c.allowed(world).has(ModeId::Shooter));
        c.mode = Mode::Explore;
        assert!(!c.allowed(world).has(ModeId::Shooter));
        c.off = vec![ModeId::Kayak];
        let a = c.allowed(world);
        assert!(!a.has(ModeId::Kayak) && a.has(ModeId::Bike) && a.has(ModeId::Walk));
    }

    /// The bot lines start on the world's own roster, and stepping one takes
    /// the match away from it.
    #[test]
    fn the_numbers_come_back_as_numbers() {
        let world = "slagdeck";
        let info = info(world);
        let mut l = launch(world, Mode::Fps);
        let opts = options(world, Mode::Fps);
        let bots = opts.iter().find(|o| o.id == "bots").expect("bots");
        let c = l.resolve();
        assert_eq!(c.bots, info.bots, "the line did not start on the world's own roster");
        assert_eq!(Some(c.difficulty.clone()), info.skill.clone());
        assert_eq!(c.target, 15);
        // Asked for what the world asks for, the world's own bots are played.
        let world_bots: Vec<crate::world::fields::BotDef> = (0..info.bots)
            .map(|i| crate::world::fields::BotDef {
                name: format!("B{i}"),
                skill: info.skill.clone().unwrap_or_else(|| "easy".into()),
                colour: None,
                bold: 0.0,
                steady: 0.0,
                without: Vec::new(),
                tune: Default::default(),
                team: None,
            })
            .collect();
        assert!(c.world_roster(&world_bots));
        l.step(bots, 1);
        let c = l.resolve();
        assert_eq!(c.bots, info.bots + 1);
        assert!(!c.world_roster(&world_bots), "one more bot still counted as the world's roster");
        assert_eq!(c.roster().len(), info.bots + 1);
    }

    /// A world that says it holds more bots is offered more; one that says
    /// nothing stops at the default. Every bot offered has a name, a colour
    /// and a character of its own.
    #[test]
    fn a_big_map_offers_more_bots() {
        let most = |world: &str, mode| options(world, mode).into_iter().find(|o| o.id == "bots").map(|o| o.values.last().unwrap().parse::<usize>().unwrap());
        assert_eq!(info("starfall").max_bots, 15);
        assert_eq!(most("starfall", Mode::Ctf), Some(15));
        assert_eq!(most("slagdeck", Mode::Fps), Some(DEFAULT_MAX_BOTS));
        let mut c = Chosen::for_world("starfall");
        c.bots = 15;
        let roster = c.roster();
        assert_eq!(roster.len(), 15);
        let names: std::collections::BTreeSet<&str> = roster.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names.len(), 15, "two bots share a name");
    }

    /// Capture the flag is offered where the world has flags and nowhere
    /// else, and the command line plays it there by default.
    #[test]
    fn capture_the_flag_comes_with_a_world_that_has_flags() {
        assert!(modes("sluice").contains(&Mode::Ctf), "sluice has a ctf block and no CAPTURE THE FLAG: {:?}", modes("sluice"));
        assert!(modes("sluice").contains(&Mode::Team));
        assert!(!modes("porch").contains(&Mode::Ctf), "an arena with no flags offered capture the flag");
        assert_eq!(Chosen::for_world("sluice").mode, Mode::Ctf);
    }

    /// A world offers the games it lists and no others, its own first: a
    /// capture the flag map has no free-for-all, and an arena is not a place
    /// to explore.
    #[test]
    fn a_world_offers_the_games_it_lists() {
        assert_eq!(modes("sluice"), [Mode::Ctf, Mode::Team]);
        assert_eq!(modes("porch"), [Mode::Fps, Mode::Team]);
        assert_eq!(modes("kestrel"), [Mode::Race]);
        assert_eq!(modes("glassfall"), [Mode::Explore], "a world with no list and no blocks is one to explore");
    }

    /// Picking a world keeps the game where the world offers it and takes
    /// the world's own where it does not, and a game settings.json kept that
    /// the world has since dropped is not played.
    #[test]
    fn the_game_follows_the_world() {
        let mut l = launch("glassfall", Mode::Explore);
        l.set_world("sluice");
        assert_eq!(l.game, "ctf", "an arena opened as a place to explore");
        l.game = "team".to_string();
        l.set_world("porch");
        assert_eq!(l.game, "team", "a game both worlds offer was dropped");
        l.set_world("glassfall");
        assert_eq!(l.game, "explore");
        assert_eq!(launch("sluice", Mode::Fps).resolve().mode, Mode::Ctf);
        // What older settings called the game still names it.
        let mut old = launch("porch", Mode::Explore);
        old.game = "fps".to_string();
        assert_eq!(old.resolve().game, "deathmatch");
    }

    /// The game is picked first: the world list is then the worlds that
    /// offer it, and picking a game the world on screen does not offer
    /// moves to one that does, the last played in if there is one.
    #[test]
    fn the_world_list_follows_the_game() {
        let has = |game: &str, w: &str| worlds_for(game).contains(&w.to_string());
        assert!(has("ctf", "megalith") && has("deathmatch", "porch") && has("race", "kestrel") && has("explore", "glassfall"));
        assert!(worlds_for("ctf").iter().all(|w| info(w).ctf), "a world with no flags is on the capture the flag list");
        assert!(!has("explore", "porch") && !has("explore", "kestrel"), "an arena or a circuit is on the explore list");
        assert!(!has("deathmatch", "megalith"), "a flag map is on the deathmatch list");
        let mut l = launch("glassfall", Mode::Explore);
        l.set_game("ctf");
        assert_eq!(l.def().name, "ctf");
        assert!(info(&l.world).ctf, "capture the flag was left on {}", l.world);
        assert!(l.world_rows().iter().all(|r| info(&r.world).ctf));
        // From a flag map that lists its games, so deathmatch is not one.
        l.world = "megalith".to_string();
        l.maps.insert("deathmatch".to_string(), "transmission".to_string());
        l.set_game("deathmatch");
        assert_eq!(l.world, "transmission", "the world this game was last played in was forgotten");
        l.set_game("team");
        assert_eq!(l.world, "transmission", "a world that offers the game was left");
        // The plain games in the order of their kinds, a variant after its own.
        let names: Vec<&str> = all_games().iter().map(|g| g.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).unwrap_or_else(|| panic!("no {n} in {names:?}"));
        assert!(at("explore") < at("deathmatch") && at("deathmatch") < at("instagib") && at("instagib") < at("team") && at("team") < at("ctf"));
    }

    /// A variant is offered beside the game it is like, keeps its own
    /// choices, and is what the run is played as.
    #[test]
    fn a_map_offers_two_deathmatches() {
        let names: Vec<&str> = games("slagdeck").iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["deathmatch", "team", "instagib"]);
        assert_eq!(modes("slagdeck"), [Mode::Fps, Mode::Team]);
        let mut l = launch("slagdeck", Mode::Fps);
        l.game = "instagib".to_string();
        let c = l.resolve();
        assert_eq!((c.mode, c.game.as_str()), (Mode::Fps, "instagib"));
        assert_eq!(c.def().name, "instagib");
        let bots = super::options("slagdeck", l.def()).into_iter().find(|o| o.id == "bots").expect("bots");
        l.step(&bots, 1);
        assert!(l.kept.contains_key("slagdeck/instagib"));
        assert!(!l.kept.contains_key("slagdeck/deathmatch"));
    }

    /// Every world's `games` list names games, and games the world can host.
    #[test]
    fn every_listed_game_can_be_played() {
        let root = crate::library::data_root();
        for d in crate::worldload::WORLD_DIRS {
            let Ok(rd) = std::fs::read_dir(root.join(d)) else { continue };
            for w in rd.filter_map(|e| e.ok()).filter(|e| e.path().join("world.json").exists()).filter_map(|e| e.file_name().into_string().ok()) {
                assert_eq!(games_problems(&w), Vec::<String>::new(), "{w}");
            }
        }
    }
}

/// The parts of the year a world.json names, without loading the world: its
/// `seasons`, or the single `season` an older world states, or summer
/// (world/season.rs `Calendar`).
fn peek_seasons(p: &Peek) -> Vec<crate::world::season::Season> {
    use crate::world::season::{Calendar, Season};
    if !p.seasons.is_empty() {
        return Calendar::from_names(&p.seasons).list();
    }
    match Season::parse(&p.season) {
        Some(s) => vec![s],
        None => Calendar::default().list(),
    }
}
