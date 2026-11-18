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
    Fps,
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
        }
    }

    /// How the launch screen shows it.
    pub fn title(self) -> &'static str {
        match self {
            Mode::Explore => "EXPLORE",
            Mode::Fps => "DEATHMATCH",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "explore" => Some(Mode::Explore),
            "fps" | "deathmatch" => Some(Mode::Fps),
            _ => None,
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
pub const TRAVEL: [ModeId; 5] = [ModeId::Bike, ModeId::Glide, ModeId::Climb, ModeId::Rope, ModeId::Kayak];

/// Bot counts and target scores the deathmatch options step through.
const BOTS: [&str; 8] = ["0", "1", "2", "3", "4", "5", "6", "7"];
const TARGETS: [&str; 6] = ["5", "10", "15", "20", "30", "50"];

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
        _ => "",
    }
}

/// The options a world and a mode put on offer, in the order they are shown:
/// the world's own first, then the mode's.
pub fn options(world: &str, mode: Mode) -> Vec<Opt> {
    let info = info(world);
    let mut out = vec![
        Opt::switch("animals", "ANIMALS", "Whether the world's creatures are in it. Off leaves the plants and the land alone.", true),
        Opt::switch("day_night", "DAY AND NIGHT", "Off holds the world at the hour it opens on, so the light never changes.", true),
        Opt::switch(
            "weather",
            "WEATHER",
            "Whether the sky changes: rain, snow, fog, heat and cloud come and go. Off keeps fair weather.",
            true,
        ),
    ];
    // The parts of the year this world has, where it has more than one: the
    // year turns on its own unless a season is picked, which holds it there.
    if info.seasons.len() > 1 {
        let mut values = vec!["Turning".to_string()];
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
    let mut hours = vec!["World".to_string()];
    hours.extend(crate::world::clock::PRESETS.iter().map(|(_, n)| capitalise(&n.to_lowercase())));
    let hours: Vec<&str> = hours.iter().map(|h| h.as_str()).collect();
    out.push(Opt::pick("hour", "TIME", "The hour the world opens at. World is the hour it was written to be seen in.", &hours, 0));
    for m in TRAVEL {
        if info.allowed.has(m) {
            out.push(Opt::switch(&format!("travel.{}", m.name()), &m.name().to_uppercase(), travel_help(m), true));
        }
    }
    if mode == Mode::Fps {
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
        let bots = if info.bots > 0 { info.bots.min(BOTS.len() - 1) } else { 3 };
        out.push(Opt::pick("bots", "BOTS", "How many of them are in the match.", &BOTS, bots));
        out.push(Opt::pick(
            "target",
            "TARGET SCORE",
            "Kills that win the match. The scoreboard comes up when someone gets there.",
            &TARGETS,
            2,
        ));
    }
    out
}

/// The modes a world offers: every world can be explored, and one whose
/// world.json carries a `"shooter"` block can be fought over.
pub fn modes(world: &str) -> Vec<Mode> {
    let mut out = vec![Mode::Explore];
    if info(world).shooter {
        out.push(Mode::Fps);
    }
    out
}

/// The launch screen's state, kept in settings.json: where RETURN goes, and
/// what each world and mode was last played with.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Launch {
    pub world: String,
    /// A mode this build does not know is Explore, not a reason to lose the
    /// rest of settings.json.
    #[serde(deserialize_with = "crate::util::or_default")]
    pub mode: Mode,
    /// Choices by `"<world>/<mode>"`, then by option id.
    pub kept: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Launch {
    fn default() -> Self {
        Launch {
            world: crate::worldload::DEFAULT_WORLD.to_string(),
            mode: Mode::Explore,
            kept: BTreeMap::new(),
        }
    }
}

impl Launch {
    fn key(&self) -> String {
        format!("{}/{}", self.world, self.mode.name())
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

    /// Step the world, wrapping round the worlds there are. The mode follows
    /// if the world it lands on does not offer the one that was picked.
    pub fn step_world(&mut self, dir: i32) {
        let list = crate::worldload::available();
        if list.is_empty() {
            return;
        }
        let n = list.len() as i32;
        let at = list.iter().position(|w| *w == self.world).unwrap_or(0) as i32;
        self.world = list[(at + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize].clone();
        if !modes(&self.world).contains(&self.mode) {
            self.mode = Mode::Explore;
        }
    }

    pub fn step_mode(&mut self, dir: i32) {
        let list = modes(&self.world);
        let n = list.len() as i32;
        let at = list.iter().position(|m| *m == self.mode).unwrap_or(0) as i32;
        self.mode = list[(at + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize];
    }

    /// Everything the loader and the game need, read off the options this
    /// world and mode offer now.
    pub fn resolve(&self) -> Chosen {
        let opts = options(&self.world, self.mode);
        let on = |id: &str| opts.iter().find(|o| o.id == id).is_some_and(|o| self.value(o) == "On");
        let num = |id: &str, fallback: u32| -> u32 { opts.iter().find(|o| o.id == id).and_then(|o| self.value(o).parse().ok()).unwrap_or(fallback) };
        // A pick whose first value means "leave it to the world".
        let pick = |id: &str| -> Option<String> { opts.iter().find(|o| o.id == id).map(|o| self.value(o)).filter(|v| v != "Turning" && v != "World") };
        let off = TRAVEL
            .into_iter()
            .filter(|m| opts.iter().any(|o| o.id == format!("travel.{}", m.name())) && !on(&format!("travel.{}", m.name())))
            .collect();
        Chosen {
            world: self.world.clone(),
            mode: self.mode,
            animals: on("animals"),
            day_night: on("day_night"),
            weather: on("weather"),
            off,
            difficulty: opts.iter().find(|o| o.id == "difficulty").map_or_else(|| "average".to_string(), |o| self.value(o).to_lowercase()),
            bots: num("bots", 3) as usize,
            target: num("target", 15),
            season: pick("season").and_then(|v| crate::world::season::Season::parse(&v.to_lowercase())).map(|s| s.name().to_string()),
            hour: pick("hour").and_then(|v| crate::world::clock::PRESETS.iter().find(|(_, n)| n.eq_ignore_ascii_case(&v)).map(|(h, _)| *h)),
        }
    }
}

/// A resolved launch: what the loading thread is handed, and what the running
/// game keeps so it can answer "is the clock running", "are there animals"
/// and "whose rules are these".
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub world: String,
    pub mode: Mode,
    pub animals: bool,
    pub day_night: bool,
    /// Whether the weather drifts (world/weather.rs). Off holds fair weather.
    pub weather: bool,
    /// Travel modes switched off for this run.
    pub off: Vec<ModeId>,
    pub difficulty: String,
    pub bots: usize,
    pub target: u32,
    /// The season to hold the world in (world/season.rs), by name. None lets
    /// the year turn on its own.
    pub season: Option<String>,
    /// The hour to open at. None takes the world's own.
    pub hour: Option<f32>,
}

/// Names and shell colours for the bots a match is asked for, in the order
/// they are handed out. Foundry words, since the first arena is a smelter.
pub const BOT_NAMES: [&str; 7] = ["DROSSEL", "KILN", "TUYERE", "SCORIA", "CINDER", "BOSH", "LADLE"];
const BOT_COLOURS: [[u8; 3]; 7] = [[196, 84, 64], [96, 150, 196], [206, 176, 88], [128, 196, 132], [186, 116, 196], [220, 140, 80], [120, 196, 196]];

impl Chosen {
    /// The defaults for a world opened without the launch screen (the command
    /// line, the headless tools): everything on, and the mode the world is
    /// built for, so `--world slagdeck` still arrives holding a gun.
    pub fn for_world(world: &str) -> Chosen {
        let info = info(world);
        Chosen {
            world: world.to_string(),
            mode: if info.shooter { Mode::Fps } else { Mode::Explore },
            animals: true,
            day_night: true,
            weather: true,
            off: Vec::new(),
            // The world's own roster, so a match started from the command
            // line is the one the world was written for.
            difficulty: info.skill.clone().unwrap_or_else(|| "average".to_string()),
            bots: if info.bots > 0 { info.bots } else { 3 },
            target: 15,
            season: None,
            hour: None,
        }
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
                // Alternating, so the roster has pressers and holders in it.
                bold: [0.0, 0.5, -0.4, 0.3, -0.2, 0.6, -0.5][i],
                steady: [0.0, -0.3, 0.5, 0.2, -0.5, 0.4, 0.3][i],
                without: Vec::new(),
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
        if self.mode != Mode::Fps {
            a = a.without(ModeId::Shooter);
        }
        a
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
}

#[derive(Deserialize)]
struct Peek {
    #[serde(default)]
    modes: Option<Vec<String>>,
    #[serde(default)]
    shooter: Option<PeekShooter>,
    #[serde(default)]
    seasons: Vec<String>,
    #[serde(default)]
    season: String,
}

#[derive(Deserialize)]
struct PeekShooter {
    #[serde(default)]
    bots: Vec<PeekBot>,
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
    let path = crate::worldload::world_dir(world).join("world.json");
    let peek: Option<Peek> = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok());
    let info = match peek {
        Some(p) => Info {
            allowed: match &p.modes {
                Some(names) => Allowed::from_names(names, |_| {}),
                None => Allowed::all(),
            },
            shooter: p.shooter.is_some(),
            seasons: peek_seasons(&p),
            bots: p.shooter.as_ref().map_or(0, |sh| sh.bots.len()),
            skill: p.shooter.as_ref().and_then(|sh| sh.bots.first().and_then(|b| b.skill.clone())),
        },
        // The random island, and a world whose world.json will not parse: the
        // loader is the one that reports that, so offer the usual game.
        None => Info {
            allowed: Allowed::all(),
            shooter: false,
            seasons: vec![crate::world::season::Season::Summer],
            bots: 0,
            skill: None,
        },
    };
    cache.insert(world.to_string(), info.clone());
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(world: &str, mode: Mode) -> Launch {
        Launch {
            world: world.to_string(),
            mode,
            kept: BTreeMap::new(),
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
            })
            .collect();
        assert!(c.world_roster(&world_bots));
        l.step(bots, 1);
        let c = l.resolve();
        assert_eq!(c.bots, info.bots + 1);
        assert!(!c.world_roster(&world_bots), "one more bot still counted as the world's roster");
        assert_eq!(c.roster().len(), info.bots + 1);
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
