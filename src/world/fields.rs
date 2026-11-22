//! World-scale fields: everything the chronicle's processes decide about a world,
//! computed once when the world loads and then read by the chunk generator from
//! any thread. Height, rock layers, biome, burn age, strangeness and what caused
//! each place live on a 1 m grid; trees, structures and buried items are lists
//! with a coarse spatial index.

use super::gen::Tree;
use super::shape::Shape;
use crate::library::Library;
use crate::util::FxHashMap as HashMap;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// Grid cell, metres.
pub const CELL: f32 = 1.0;
/// Spatial index cell for trees and structures, metres.
pub const BUCKET: f32 = 16.0;

/// How many threads a world build spreads its per-cell stages over.
/// VOXEL_BUILD_THREADS sets it; 1 runs them as one pass, which is how a
/// threaded stage is checked against itself.
pub fn build_threads() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("VOXEL_BUILD_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or_else(|| std::thread::available_parallelism().map(|t| t.get()).unwrap_or(4))
            .min(32)
    })
}

/// The seed scheme a world without a `seeds` line uses.
pub const SEEDS_NOW: u32 = 2;

/// A world's own settings file (`worlds/<name>/world.json`).
#[derive(Deserialize, Clone, Debug)]
pub struct WorldDef {
    pub name: String,
    pub size_m: f32,
    #[serde(default = "default_height")]
    pub height_m: f32,
    pub sea_level_m: f32,
    pub seed: u32,
    /// Which generation of seed keys this world was laid out with. Leaving it
    /// out means the current one, which keys an event by its kind and year
    /// and a biome's plant, feature or collectible by its name. `1` is the
    /// older scheme, which counts an event's place in the year-sorted list
    /// and a use's place in its biome's list: adding one in the middle moved
    /// everything after it. The worlds written before the change say `1` so
    /// their land stays where it was; a new world should leave this out.
    #[serde(default)]
    pub seeds: Option<u32>,
    /// Bedrock bands by weight, e.g. [["basalt", 0.6], ["tuff", 0.4]].
    #[serde(default)]
    pub rock: Vec<(String, f32)>,
    /// Where the player starts, metres, and the compass bearing they face.
    #[serde(default)]
    pub spawn: Option<[f32; 2]>,
    /// The spawn as a place the chronicle found (`Pos`), for a world whose
    /// land is generated: it wins over `spawn`, and `spawn_bearing` turns
    /// with it.
    #[serde(default)]
    pub spawn_at: Option<Pos>,
    /// Metres above the bare ground the spawn point stands on, for a world
    /// whose spawn is inside a building rather than on the land. The ground
    /// at the spawn is the land under the structure, so a deck raised on a
    /// plinth needs to say so or the player is pushed up through the
    /// building and comes out on its roof. It is also the height every bot
    /// nav node measures from (`NavNode::up`).
    #[serde(default)]
    pub spawn_up: f32,
    #[serde(default)]
    pub spawn_bearing: f32,
    /// The biome used where no other matches.
    pub default_biome: String,
    /// The biome used under the sea where no other matches.
    #[serde(default)]
    pub seabed_biome: Option<String>,
    /// The library biomes this world may use. Every library biome competes
    /// for every cell, so a world names its own; leaving it out means all of them.
    #[serde(default)]
    pub biomes: Option<Vec<String>>,
    /// Rock formations by height, oldest (lowest) first: [material, base height
    /// in metres]. Each runs up to the next one's base. When given, bare rock
    /// shows these horizontal layers instead of the random `rock` bands.
    #[serde(default)]
    pub strata: Vec<(String, f32)>,
    /// How far (m) each strata boundary wanders up and down across the world,
    /// each by its own amount, so bands pinch and swell instead of running
    /// level and even everywhere. 0 (the default) keeps them flat.
    #[serde(default)]
    pub strata_swell: f32,
    /// Strata (by material name) that are level lines such as old water
    /// marks rather than rock beds: they stay put when beds swell or fold.
    #[serde(default)]
    pub strata_level: Vec<String>,
    /// Materials the mesher draws with bevelled edges in this world only, as
    /// if their library entry said `"facet": true`: every corner is placed by
    /// surface nets, so square edges get a chamfer and a one-voxel step (a
    /// stair, the stepped side of a blast hole) becomes a slanted face. The
    /// arenas use it on their walls and decks. Applied by
    /// `worldload::library_for`; B / `--blocky` still turns it off.
    #[serde(default)]
    #[allow(dead_code)] // read from the raw JSON by worldload, before a WorldDef exists
    pub round_edges: Vec<String>,
    /// How the menus show the world, where the folder's name in capitals is
    /// not it ("CTF Box" for `ctfbox`).
    #[serde(default)]
    #[allow(dead_code)] // read from the raw JSON by launch::info, without loading the world
    pub title: Option<String>,
    /// A sentence on what the place is, shown under the launch screen's list
    /// with the world picked. Left out, the opening of story.md is used.
    #[serde(default)]
    #[allow(dead_code)] // launch::info
    pub blurb: Option<String>,
    /// `false` keeps the world off the menus' list (a bench, a test);
    /// `--world` still opens it.
    #[serde(default)]
    #[allow(dead_code)] // launch::info
    pub listed: Option<bool>,
    /// How fast abandoned buildings decay here (1 a wet climate; a desert
    /// keeps its timber, about 0.35). Multiplies their age for decay.
    #[serde(default = "default_one")]
    pub weathering: f32,
    /// How much moss old stonework grows (0 in a desert).
    #[serde(default = "default_one")]
    pub moss: f32,
    /// The one season this world is always in, where it has only one
    /// (world/season.rs). `seasons` says it better and is what a new world
    /// should use; this is the older way of saying `"seasons": ["autumn"]`.
    #[serde(default)]
    pub season: String,
    /// The parts of the year this world has, by name ("spring", "summer",
    /// "autumn", "winter"). The year turns from one to the next in that
    /// order, about every two days, and a world that names one season never
    /// turns at all. Leaving it out gives summer alone, which is every world
    /// the engine had before there were seasons.
    #[serde(default)]
    pub seasons: Vec<String>,
    /// The season a new game in this world opens in; the first its calendar
    /// has, going round from summer, if left out.
    #[serde(default)]
    pub start_season: Option<String>,
    /// The ways of playing this world allows, by name (src/modes.rs: walk,
    /// bike, glide, climb, rope, screenshot, camera). Leaving it out allows all.
    #[serde(default)]
    pub modes: Option<Vec<String>>,
    /// The games this world is built for, its own first: `["ctf", "team"]`
    /// (explore, deathmatch, team, ctf, race). The launch screen offers
    /// these and no others, and opens on the first. Left out, the world is
    /// one to explore, plus whatever its `shooter` and `race` blocks allow.
    /// Read by `launch::info`, before the world is loaded.
    #[serde(default)]
    #[allow(dead_code)]
    pub games: Vec<String>,
    /// The vehicles here, by library name (library/vehicles): `["sand_rail"]`.
    /// The first is the one U brings unless the launch screen picks another.
    /// A world with none has no driving, whatever `modes` says.
    #[serde(default)]
    pub vehicles: Vec<String>,
    /// A race here (`RaceDef`): the launch screen offers RACE, and starts it.
    #[serde(default)]
    pub race: Option<RaceDef>,
    /// Height (m) above which ground turns to snow; default 72. A warm world
    /// with tall towers sets it out of reach.
    #[serde(default)]
    pub snow_line_m: Option<f32>,
    /// The sun's path here (render/daylight.rs `SunPath`): the latitude and
    /// how the light falls. Latitude 42 if left out, the summer sun 62
    /// degrees up at noon.
    #[serde(default)]
    pub sun: Option<SunDef>,
    /// The glider's air here: `{"thermals": 0.2, "ridge_from": 180,
    /// "ridge_speed": 9}`. Thermals scale the engine's (1, the default);
    /// `ridge_from` is the bearing the ridge wind blows from, for a world
    /// whose cliffs the engine's own wind runs along rather than into.
    #[serde(default)]
    pub glide: Option<GlideDef>,
    /// Diamond dust: ice crystals sparkling in the air round you on clear
    /// days, as a rate (1 is a fair glitter; 0, the default, none).
    #[serde(default)]
    pub diamond_dust: f32,
    /// Snow on whatever faces up (tree tops, ledges, roofs) that never goes,
    /// 0 to 1, for a snowbound world; the season's own lying snow adds to it.
    #[serde(default)]
    pub snow_cover: f32,
    /// How far the grass has gone over to straw whatever the season, 0 to 1,
    /// for a dry country (a steppe arena); the season's own straw adds to it.
    #[serde(default)]
    pub straw: f32,
    /// How strong the northern lights get on the best night (0 = never;
    /// render/aurora.rs). Each night draws its own share of it.
    #[serde(default)]
    pub aurora: f32,
    /// How strongly the ground mirrors the sky at a grazing angle in the heat
    /// of the day (0 = never, the default; 1 = a salt pan at noon): the
    /// far flat goes to sky-coloured water and what stands on it floats over
    /// its own upside-down image (post.wgsl `mirage`).
    #[serde(default)]
    pub mirage: f32,
    /// The sea's colour here; the engine's own jade-teal if left out.
    /// Biomes may give their own (`water`), at most two per world.
    #[serde(default)]
    pub water: Option<crate::library::WaterColour>,
    /// What it is like to be under that water (docs/UNDERWATER.md): how far
    /// you can see and how fast the sunlight fades with depth.
    #[serde(default)]
    pub underwater: Underwater,
    /// The tide (world/tide.rs): `{"range": 2.5}` is two and a half metres
    /// between high and low water, high at six. `sea_level_m` is high water.
    /// Leaving it out is a still sea; a world with no open sea reaching the
    /// edge of the map has none whatever this says.
    #[serde(default)]
    pub tide: Option<crate::world::tide::TideDef>,
    /// Waves breaking on the shore (world/surf.rs): `{"height": 1.2}` is
    /// breakers a little over a metre high on a fair day. Leaving it out is a
    /// sea that only laps.
    #[serde(default)]
    pub surf: Option<crate::world::surf::SurfDef>,
    /// Which kinds of day this world gets, and how often: kind name
    /// ("sun", "heat", "clouds", "rain", "snow", "fog") to a weight
    /// (world/weather.rs). A kind left out of a block that has anything in it
    /// never happens here, so a desert lists everything but snow. Leaving the
    /// block out gives every kind at its own default.
    #[serde(default)]
    pub weather: std::collections::BTreeMap<String, f32>,
    /// The kind of day a new game in this world opens on; the commonest kind
    /// its climate allows if left out.
    #[serde(default)]
    pub start_weather: Option<String>,
    /// The hour a new game in this world starts at (world/clock.rs, 5.2 to
    /// 18.8); mid-afternoon if left out. Hemlow starts in the evening.
    #[serde(default)]
    pub start_hour: Option<f32>,
    /// A map floating in open space (Starfall): no ground under it, the
    /// clock held at the hour it opens on, and the sky drawn black with
    /// stars all the way round, below the horizon too, with no clouds.
    #[serde(default)]
    pub space: bool,
    /// How hard this world pulls and toward what (world/gravity.rs):
    /// `{"g": 0.3}` is a third of the engine's own, and under 0.6 a body in
    /// the air can be steered. Leaving it out is the engine's own, straight
    /// down.
    #[serde(default)]
    pub gravity: crate::world::gravity::Gravity,
    /// The rules of the shooter played here (src/shooter.rs). A world with
    /// this block starts in a gun's hands; one without never gets a gun.
    #[serde(default)]
    pub shooter: Option<ShooterDef>,
    /// Pieces of the world that move: lifts, doors, a ferry
    /// (src/platforms.rs).
    #[serde(default)]
    pub platforms: Vec<crate::platforms::PlatformDef>,
    /// Places that act on whoever steps into them: teleporters and jump
    /// pads (src/portals.rs).
    #[serde(default)]
    pub portals: Vec<crate::portals::PortalDef>,
    /// Added to every point before the land's noise is read (landmass,
    /// subside): a world trimmed after it was made, its features moved by
    /// minus this, keeps the same coast and islands under them.
    #[serde(default)]
    pub noise_at: [f32; 2],
    /// The worlds beside this one, which it is built to meet at its edges
    /// (`world/seam.rs`, notes/UBERWORLD_PLAN.md).
    #[serde(default)]
    pub neighbours: super::seam::Neighbours,
}

impl WorldDef {
    /// True when this world was laid out with the first seed scheme, whose
    /// keys count a thing's place in a list (see `seeds`).
    pub fn seeds_v1(&self) -> bool {
        self.seeds.unwrap_or(SEEDS_NOW) < 2
    }

    /// The parts of the year this world has: whatever `seasons` names, else
    /// the single `season` where an older world states one, else summer.
    pub fn calendar(&self) -> crate::world::season::Calendar {
        use crate::world::season::{Calendar, Season};
        if !self.seasons.is_empty() {
            return Calendar::from_names(&self.seasons);
        }
        match Season::parse(&self.season) {
            Some(s) => Calendar::of(&[s]),
            None => Calendar::default(),
        }
    }
}

/// world.json `glide`.
#[derive(Deserialize, Clone, Debug)]
pub struct GlideDef {
    #[serde(default)]
    pub thermals: Option<f32>,
    #[serde(default)]
    pub ridge_from: Option<f32>,
    #[serde(default)]
    pub ridge_speed: Option<f32>,
}

impl GlideDef {
    pub fn air(&self) -> crate::ride::GlideAir {
        let mut a = crate::ride::GlideAir::default();
        if let Some(t) = self.thermals {
            a.thermals = t;
        }
        if let Some(b) = self.ridge_from {
            // Blowing from the bearing, so toward its opposite (0 north = -z).
            let r = b.to_radians();
            a.ridge = glam::Vec2::new(-r.sin(), r.cos());
        }
        if let Some(s) = self.ridge_speed {
            a.ridge_speed = s;
        }
        a
    }
}

/// world.json `sun`: how the sun crosses this world's sky.
#[derive(Deserialize, Clone, Debug)]
pub struct SunDef {
    /// Degrees north; 42 if left out, which puts the summer sun 62 degrees
    /// up at noon. Each season then moves the sun (world/season.rs).
    #[serde(default)]
    pub latitude: Option<f32>,
    /// The lowest sun lighting and shadows are cast from (degrees). A low
    /// sun raises it so the snow is lit and shadows stay a sensible length.
    #[serde(default)]
    pub lift_deg: Option<f32>,
    /// How much wider the shadow filter spreads (1 = as built).
    #[serde(default)]
    pub shadow_soft: Option<f32>,
    /// How far the light is turned cold, 0 to 1: a pale sun, blue shade and
    /// sky, no golden grade (render/daylight.rs `chill`).
    #[serde(default)]
    pub cold: f32,
}

impl SunDef {
    pub fn path(&self) -> crate::render::daylight::SunPath {
        let mut p = crate::render::daylight::SunPath::DEFAULT;
        if let Some(l) = self.latitude {
            p.latitude = l;
        }
        if let Some(l) = self.lift_deg {
            p.lift_deg = l;
        }
        if let Some(s) = self.shadow_soft {
            p.shadow_soft = s;
        }
        p.cold = self.cold;
        p
    }
}

/// What a world hands the shooter: the loadout, how much health, and what
/// happens when you die. The guns themselves are `library/weapons/`.
#[derive(Deserialize, Clone, Debug)]
pub struct ShooterDef {
    /// The most bots the launch screen offers for a match here; left out,
    /// `launch::DEFAULT_MAX_BOTS`. A big map says so: Starfall holds 15.
    /// Worth keeping within the spawn points, or bots share them.
    #[serde(default)]
    pub max_bots: Option<usize>,
    /// Library weapon names, in any order; each takes its own digit.
    pub weapons: Vec<String>,
    /// The material a weapon that throws spoil lays down (`FireDef::spoil_r`,
    /// the bore's right button). Left out, it is the world's first bedrock
    /// (`rock`): what a bore shows you when it has gone deep enough.
    #[serde(default)]
    pub spoil: Option<String>,
    #[serde(default = "default_health")]
    pub health: f32,
    /// The most health a fighter can have: health pads heal past `health`
    /// up to here, and a respawn hands back `health`. Left out, it is 200
    /// (`brain::HEALTH_MAX`).
    #[serde(default)]
    pub health_max: Option<f32>,
    /// Armour at spawn; it soaks up two thirds of what lands until it is gone.
    #[serde(default)]
    pub armour: f32,
    /// The most armour a fighter can wear, for a world whose floor has
    /// armour on it: a pad can take one up to here, and a respawn hands
    /// back `armour` rather than this. Left out, it is `armour`, so a world
    /// that issues armour and has no pads is unchanged.
    #[serde(default)]
    pub armour_max: Option<f32>,
    #[serde(default = "default_respawn")]
    pub respawn_s: f32,
    /// Where bodies come back, taken in turn. The world's own spawn point
    /// when the list is empty.
    #[serde(default)]
    pub spawns: Vec<SpawnPoint>,
    /// Hand out every weapon, full, at every spawn: what an arena does until
    /// there are pickups to stand on.
    #[serde(default)]
    pub give_all: bool,
    /// Ammunition pads hand rounds over for a gun you are not carrying, kept
    /// until you find the gun (and lost at a death), as older arena
    /// shooters do. On unless a world says `false`; off, a pad for a gun you have
    /// not got stays standing.
    #[serde(default = "default_true")]
    pub ammo_without_gun: bool,
    /// Kills that end the match; 0 plays on forever (src/shooter/round.rs).
    #[serde(default)]
    pub target_score: u32,
    /// Who else is in the match (src/brain).
    #[serde(default)]
    pub bots: Vec<BotDef>,
    /// Where bots may walk. Baked, because a graph worked out at run time is
    /// a graph nothing can check (src/brain/nav.rs).
    #[serde(default)]
    pub nav: NavDef,
    /// What is lying about to be picked up (src/shooter/pickup.rs).
    #[serde(default)]
    pub pickups: Vec<PickupDef>,
    /// A body that falls below this world height (m) dies: the edge of a
    /// map that floats in space (world.json `space`). None, the default,
    /// leaves falling to the floor.
    #[serde(default)]
    pub void_m: Option<f32>,
    /// How much of a teammate's damage lands, 0 to 1. Pushes always land, so
    /// a teammate's rocket still lifts you. Only read when there are teams.
    #[serde(default)]
    pub friendly_fire: f32,
    /// The player's side in a team match, set by the launch screen; None is
    /// the smaller side (src/shooter/teams.rs).
    #[serde(default)]
    pub player_team: Option<u8>,
    /// Play the bots and spawns as two sides even without a `ctf` block:
    /// team deathmatch. Set by the launch screen.
    #[serde(default)]
    pub teams: bool,
    /// Capture the flag (src/shooter/flag.rs). Present, the match is two
    /// teams and the flags decide it.
    #[serde(default)]
    pub ctf: Option<CtfDef>,
}

/// Capture the flag: where each team's flag stands and how a match is won.
#[derive(Deserialize, Clone, Debug)]
pub struct CtfDef {
    /// One per team, team 0 first.
    pub flags: Vec<FlagDef>,
    /// Captures that win the match.
    #[serde(default = "default_captures")]
    pub captures: u32,
    /// Seconds a match lasts; 0 plays to the capture count.
    #[serde(default)]
    pub time_s: f32,
    /// Seconds a dropped flag lies before it goes home on its own.
    #[serde(default = "default_flag_return")]
    pub return_s: f32,
    /// A capture needs your own flag at home.
    #[serde(default = "default_true")]
    pub need_home: bool,
}

/// Where a flag stands: its stand, like a pad, with the floor found under `up`.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct FlagDef {
    pub team: u8,
    pub at: [f32; 2],
    #[serde(default)]
    pub up: Option<f32>,
}

fn default_captures() -> u32 {
    3
}

fn default_flag_return() -> f32 {
    25.0
}

/// One thing standing on the floor waiting to be walked over.
#[derive(Deserialize, Clone, Debug)]
pub struct PickupDef {
    /// `health`, `armour`, `ammo` or `weapon`.
    pub kind: String,
    /// Metres, `[x, z]`, with the floor found under `up` the way a spawn is.
    pub at: [f32; 2],
    #[serde(default)]
    pub up: Option<f32>,
    /// How much: health or armour points, or rounds for an `ammo` pad. The
    /// kind's own default when left out.
    #[serde(default)]
    pub amount: Option<f32>,
    /// Which library weapon, for `ammo` and `weapon`.
    #[serde(default)]
    pub weapon: Option<String>,
    /// Seconds before it comes back. The kind's own default when left out --
    /// health soon, the rocket launcher slowly, which is most of what makes
    /// an arena a map rather than a room.
    #[serde(default)]
    pub respawn_s: Option<f32>,
}

impl PickupDef {
    /// The slab the shooter stamps under this pad: the kind's own colour for
    /// health and armour, the gun's for a weapon and for its ammunition.
    /// Named here rather than in the rules so `worldload::reach` can give
    /// every pad's material an id in a world that has them.
    pub fn pad_material(&self) -> Option<String> {
        match self.kind.as_str() {
            "health" => Some("pickup_pad_health".into()),
            "armour" | "armor" => Some("pickup_pad_armour".into()),
            "ammo" | "weapon" => self.weapon.as_ref().map(|w| format!("weapon_pad_{w}")),
            _ => None,
        }
    }
}

/// One bot in the match.
#[derive(Deserialize, Clone, Debug)]
pub struct BotDef {
    pub name: String,
    /// A row of `brain::person::LEVELS`, easiest first.
    #[serde(default = "default_skill")]
    pub skill: String,
    /// Its shell's colour when whole, sRGB bytes; health takes it to red.
    #[serde(default)]
    pub colour: Option<[u8; 3]>,
    /// Character on top of the difficulty: toward pressing the attack, and
    /// toward holding its aim. Both -1..1.
    #[serde(default)]
    pub bold: f32,
    #[serde(default)]
    pub steady: f32,
    /// Parts of its movement to switch off (`brain::duel::Style`). Empty is
    /// every part on, which is what a world wants; the tournament uses it to
    /// run the same bot with and without one thing.
    #[serde(default)]
    pub without: Vec<String>,
    /// Numbers to set on its decision layer (`brain::duel::Tuning`), by
    /// name. Empty is the shipping bot; the league uses it to field five
    /// differently tuned bots at one difficulty.
    #[serde(default)]
    pub tune: std::collections::BTreeMap<String, f32>,
    /// Its side in a team match, 0 or 1. None in a team match is dealt out
    /// to even the sides.
    #[serde(default)]
    pub team: Option<u8>,
}

/// A bot navigation graph: places, and what it takes to get between them.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct NavDef {
    #[serde(default)]
    pub nodes: Vec<NavNode>,
    #[serde(default)]
    pub links: Vec<NavLink>,
}

/// One place worth standing.
#[derive(Deserialize, Clone, Debug)]
pub struct NavNode {
    /// Metres, `[x, z]`.
    pub at: [f32; 2],
    /// Metres above the floor the world's own spawn point stands on. A world
    /// does not know what height its building ended up at, but it does know
    /// a walkway is four metres over the floor.
    #[serde(default)]
    pub up: f32,
    /// How enclosed it is, 0 open floor and 1 a corner to hide in.
    #[serde(default)]
    pub cover: f32,
    /// A name a brain can ask for ("rocket", "gantry").
    #[serde(default)]
    pub poi: Option<String>,
}

/// One way between two nodes. Walks and jumps go both ways; a drop goes down
/// only.
#[derive(Deserialize, Clone, Debug)]
pub struct NavLink {
    pub a: u16,
    pub b: u16,
    /// "walk", "jump" or "drop"; "walk" when left out.
    #[serde(default)]
    pub reach: String,
    /// What it costs, when the distance between the nodes is the wrong
    /// answer (a ladder, a long drop). The distance when left out.
    #[serde(default)]
    pub cost: f32,
    /// The platform this link rides (src/platforms.rs) or the portal it
    /// steps into (src/portals.rs), by name. A link that waits for a lift
    /// has to know which lift it is waiting for. Nothing reads them yet:
    /// routing treats every lift and portal link alike.
    #[serde(default)]
    #[allow(dead_code)]
    pub platform: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub portal: Option<String>,
}

/// One place a body comes back at.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct SpawnPoint {
    /// Metres, `[x, z]`. The floor is found under `ceiling`.
    pub at: [f32; 2],
    /// The compass bearing the player faces.
    #[serde(default)]
    pub bearing: f32,
    /// Height (m) to look for the floor below, for a spawn under a roof: the
    /// highest floor in the world at that spot when left out.
    #[serde(default)]
    pub ceiling: Option<f32>,
    /// The side that comes back here in a team match; None is either side.
    #[serde(default)]
    pub team: Option<u8>,
}

fn default_health() -> f32 {
    100.0
}

fn default_respawn() -> f32 {
    5.0
}

fn default_true() -> bool {
    true
}

fn default_skill() -> String {
    "average".to_string()
}

fn default_one() -> f32 {
    1.0
}

fn default_height() -> f32 {
    128.0
}

/// A place: coordinates in metres, a name (from the chronicle's `places`, or
/// one an event found: see `seat` and `glaciate`), or so far from a name.
///
/// The last is how anything built on land the generator shaped is placed:
/// `{"from": "fjord_head", "by": [30, -12]}` is 30 m east and 12 m north of
/// wherever the fjord's head came out. A found place may face a way (the
/// head of a fjord faces up its valley); with `as_if`, the offset is read as
/// drawn for a place facing that bearing and turned, in quarter turns, to
/// suit the way this one came out, so a village laid out for a shore to its
/// west still has the water at its feet.
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Pos {
    At([f32; 2]),
    Named(String),
    Rel {
        from: String,
        by: [f32; 2],
        #[serde(default)]
        as_if: Option<f32>,
    },
}

/// A buried band: material, depth of its top below the surface, thickness,
/// both in `LAYER_STEP`s. Only the top `BURIED_LIMIT` metres are kept: the
/// game is not about mining, so anything deeper is plain
/// rock. Packed this way a cell's layers take 9 bytes instead of 36, which on
/// a 1 km world is 27 MB less (61 MB on Redrock's 1.5 km).
#[derive(Clone, Copy, Default, Debug)]
pub struct Layer {
    pub mat: u8,
    depth: u8,
    thick: u8,
}

/// Depth below the surface past which nothing is kept or dug (metres).
pub const BURIED_LIMIT: f32 = 3.0;
const LAYER_STEP: f32 = 0.02;

impl Layer {
    /// A layer from metres, cut off at `BURIED_LIMIT`; None if nothing is left.
    pub fn new(mat: u8, depth: f32, thick: f32) -> Option<Self> {
        let top = depth.max(0.0);
        let bottom = (depth + thick).min(BURIED_LIMIT);
        if mat == 0 || bottom <= top {
            return None;
        }
        let d = (top / LAYER_STEP).round() as u8;
        let t = ((bottom - top) / LAYER_STEP).round().max(1.0) as u8;
        Some(Self { mat, depth: d, thick: t })
    }
    pub fn depth(&self) -> f32 {
        self.depth as f32 * LAYER_STEP
    }
    pub fn thick(&self) -> f32 {
        self.thick as f32 * LAYER_STEP
    }
}

pub const MAX_LAYERS: usize = 3;

/// When a placed structure stands (chronicle `place` `when`): the rest of
/// the time its plot is empty. Every condition given must hold.
#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// Hours of the day it stands, [from, to); past midnight wraps ([23, 3]).
    #[serde(default)]
    pub hours: Option<[f32; 2]>,
    /// Seen only from at least this far away, metres: it is gone by the
    /// time you get there, and back once you have gone.
    #[serde(default)]
    pub far: Option<f32>,
    /// Seen only from within this distance, metres.
    #[serde(default)]
    pub near: Option<f32>,
    /// true: stands only while the moon is out (the sun more than 5 degrees
    /// down, where the moonlight is half faded in), so it follows dusk and
    /// dawn through the year; false: only while it is not.
    #[serde(default)]
    pub moon: Option<bool>,
}

impl When {
    /// Whether it stands at `hour` with the player `dist` m away and the
    /// moon out or not (`moon_out`), given whether it stood a moment ago
    /// (`was`): distances switch with 20 m of slack so it doesn't flicker at
    /// the line.
    pub fn stands(&self, hour: f32, dist: f32, moon_out: bool, was: bool) -> bool {
        let slack = if was { 0.0 } else { 20.0 };
        let hours = self.hours.is_none_or(|[a, b]| if a <= b { hour >= a && hour < b } else { hour >= a || hour < b });
        let far = self.far.is_none_or(|f| dist > f + slack);
        let near = self.near.is_none_or(|n| dist < n - slack);
        let moon = self.moon.is_none_or(|m| m == moon_out);
        hours && far && near && moon
    }
}

#[cfg(test)]
mod when_tests {
    use super::When;

    #[test]
    fn hours_wrap_past_midnight_and_far_has_slack() {
        let w = When {
            hours: Some([23.0, 3.0]),
            far: Some(70.0),
            near: None,
            moon: None,
        };
        assert!(w.stands(1.0, 200.0, true, false) && w.stands(23.5, 200.0, true, false));
        assert!(!w.stands(12.0, 200.0, false, true));
        // Gone once you come within 70 m; back only past 90 m.
        assert!(!w.stands(1.0, 60.0, true, true));
        assert!(!w.stands(1.0, 80.0, true, false));
        assert!(w.stands(1.0, 95.0, true, false));
        assert!(w.stands(1.0, 80.0, true, true));
    }

    #[test]
    fn moon_follows_the_moon_not_the_clock() {
        let w = When {
            moon: Some(true),
            ..Default::default()
        };
        assert!(w.stands(13.0, 0.0, true, false), "the hour doesn't matter, only the moon");
        assert!(!w.stands(1.0, 0.0, false, true));
    }
}

/// A structure stamped into the world.
#[derive(Clone, Debug)]
pub struct Placed {
    /// Index into `World::shapes`.
    pub shape: usize,
    /// Library structure index (for names and tags).
    pub def: usize,
    /// World voxel of the rotated footprint's minimum corner at ground level.
    pub origin: [i32; 3],
    pub rot: u8,
    /// Years since abandonment, for decay (0: kept up, or never built to decay).
    pub age: f32,
    /// Event that put it here.
    #[allow(dead_code)] // provenance, for tools
    pub cause: u16,
    pub seed: u32,
}

impl Placed {
    pub fn footprint(&self, shapes: &[Shape]) -> (i32, i32, i32, i32) {
        let s = &shapes[self.shape];
        let (w, d) = super::shape::rotated_size(s.size, self.rot);
        (self.origin[0], self.origin[2], self.origin[0] + w, self.origin[2] + d)
    }
}

/// A pickup the world hides: skystone fragments, caches.
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: String,
    pub pos: Vec3,
    #[allow(dead_code)] // recorded by the events, not drawn yet
    pub size: f32,
    #[allow(dead_code)] // provenance, for tools
    pub cause: u16,
    /// Lies on the surface: the game sets it on the ground it finds there.
    pub surface: bool,
}

/// One segment of a carved tunnel (lava tubes), metres.
#[derive(Clone, Copy, Debug)]
pub struct Tube {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
    /// Material lining the roof of the tunnel (0: bare rock).
    pub lining: u8,
    /// Material of the rock all round the tunnel, floor included, under the
    /// lining (0: the ground's own rock).
    pub wall: u8,
}

/// Radii a tower's outline is sampled at, round the compass.
pub const TOWER_ANGLES: usize = 32;
/// Height step of a tower's radius table, metres.
pub const TOWER_DY: f32 = 0.5;

/// A free-standing rock tower drawn as a volume rather than a height, so it
/// can narrow, bulge and overhang (tower karst, big hoodoos). The heightfield
/// holds its top; the chunk generator fills it voxel by voxel from this.
#[derive(Clone, Debug)]
pub struct Tower {
    pub c: Vec2,
    /// Height of the bottom row, metres.
    pub base: f32,
    /// Outline radius per row (every TOWER_DY m) and angle bucket.
    pub rows: Vec<[f32; TOWER_ANGLES]>,
    /// The axis's offset from `c` per row (lean and wander).
    pub off: Vec<Vec2>,
    /// A tilted top: the top may not rise above `top + tilt . (p - c)`.
    pub top: f32,
    pub tilt: Vec2,
    /// Widest reach from `c`, metres, for bounds.
    pub reach: f32,
    /// The ground under the tower before it rose, per 1 m cell of its
    /// bounds from (x0, z0), `w` cells wide; NaN where it raised nothing.
    pub x0: i32,
    pub z0: i32,
    pub w: i32,
    pub under: Vec<f32>,
    /// Its own rock (0: the world's strata), and a mantle (moss, vines) over
    /// `mantle_share` of its skin (0: none).
    pub rock: u8,
    pub mantle: u8,
    pub mantle_share: f32,
    /// Moss in streaks down its walls, over about `moss_share` of them and
    /// thickest under ledges (0: none).
    pub moss: u8,
    pub moss_share: f32,
    /// Soil on the tops of its ledges and bulges, wherever its skin faces up
    /// (0: bare rock).
    pub soil: u8,
}

impl Tower {
    pub fn inside(&self, p: Vec3) -> bool {
        self.margin(p).is_some()
    }

    /// How far inside the outline p is, metres, or None outside.
    pub fn margin(&self, p: Vec3) -> Option<f32> {
        let t = (p.y - self.base) / TOWER_DY;
        if t < 0.0 || t >= (self.rows.len() - 1) as f32 {
            return None;
        }
        let i = t as usize;
        let ft = t - i as f32;
        let off = self.off[i].lerp(self.off[i + 1], ft);
        let d = Vec2::new(p.x, p.z) - self.c - off;
        let dd = d.length_squared();
        if dd > self.reach * self.reach {
            return None;
        }
        let cut = self.top + self.tilt.dot(Vec2::new(p.x, p.z) - self.c) - p.y;
        if cut < 0.0 {
            return None;
        }
        let a = (d.y.atan2(d.x) / std::f32::consts::TAU).rem_euclid(1.0) * TOWER_ANGLES as f32;
        let a0 = a as usize % TOWER_ANGLES;
        let a1 = (a0 + 1) % TOWER_ANGLES;
        let fa = a - a.floor();
        let r0 = self.rows[i][a0] + (self.rows[i][a1] - self.rows[i][a0]) * fa;
        let r1 = self.rows[i + 1][a0] + (self.rows[i + 1][a1] - self.rows[i + 1][a0]) * fa;
        let r = r0 + (r1 - r0) * ft;
        if dd < r * r {
            Some((r - dd.sqrt()).min(cut))
        } else {
            None
        }
    }

    /// The ground this tower stands over at (x, z), if it raised that cell.
    pub fn under_at(&self, x: f32, z: f32) -> Option<f32> {
        let (ix, iz) = (x.floor() as i32 - self.x0, z.floor() as i32 - self.z0);
        if ix < 0 || iz < 0 || ix >= self.w || iz >= self.w {
            return None;
        }
        let u = self.under[(iz * self.w + ix) as usize];
        if u.is_nan() {
            None
        } else {
            Some(u)
        }
    }
}

/// Something the chronicle asks to grow at an exact spot (a banyan on a
/// tower's ledge, a vine curtain hanging from it): plant name, base, and the
/// way out from the rock.
#[derive(Clone, Debug)]
pub struct Growth {
    pub plant: String,
    pub at: Vec3,
    pub out: Vec2,
    /// The seed to grow it from. An event that needs to know the tree's
    /// shape at chronicle time (where a canopy platform goes on the trunk)
    /// grows it itself and passes the seed it used, so flora grows the same
    /// tree. None takes one from the growth's place in the list.
    pub seed: Option<u32>,
}

/// A point of interest the chronicle made, for the gazetteer and validation.
#[derive(Clone, Debug)]
pub struct Poi {
    pub name: String,
    pub pos: Vec2,
    #[allow(dead_code)] // provenance, for tools
    pub cause: u16,
}

pub struct Fields {
    pub n: usize,
    pub size: f32,
    pub sea: f32,
    pub h: Vec<f32>,
    pub cliff: Vec<f32>,
    pub moist: Vec<f32>,
    /// Year of the last fire, or i32::MIN.
    pub burn_year: Vec<i32>,
    pub strange: Vec<f32>,
    /// 0 none, 1 open water (a river or lake), 2 old river bed, 3 path.
    pub water: Vec<u8>,
    /// Surface material override (0 none): crater glass, river bed, paths.
    pub surface: Vec<u8>,
    pub layers: Vec<[Layer; MAX_LAYERS]>,
    /// Index into `regions` + 1, or 0.
    pub region: Vec<u8>,
    pub regions: Vec<String>,
    /// Event index + 1 that last changed the cell, or 0.
    pub cause: Vec<u16>,
    /// Structure footprints, so plants avoid them.
    pub occupied: Vec<bool>,
    // Derived after the chronicle runs.
    pub slope: Vec<f32>,
    pub coast_dist: Vec<f32>,
    pub river_dist: Vec<f32>,
    pub biome: Vec<u8>,
    /// River and lake water, sparse (they are a sliver of a world): cell
    /// index -> surface height and current. Set by `settle_water`; covers
    /// the water and a margin of bank, so the drawn surface meets the
    /// ground under it.
    pub inland: HashMap<u32, Wet>,
    /// Waterfalls: where a river drops over a cliff (`settle_water`).
    pub falls: Vec<super::falls::Fall>,
    /// How far the rock beds under each cell have been lifted (m) by folding
    /// (event `reef`): strata are read at height minus this. Empty, all zero,
    /// until an event folds something.
    pub lift: Vec<f32>,
    /// The tide (world/tide.rs): which cells are open sea and how far it has
    /// fallen now. Off unless the world asks for one (`WorldDef::tide`).
    pub tide: super::tide::Tide,
    /// Surf and swell (world/surf.rs): the shore grid and the swell's waves.
    /// Empty unless the world has surf (`WorldDef::surf`).
    pub shore: super::surf::Shore,
    /// Low water (m): `sea` less the tide's range, or `sea` with no tide.
    /// Set before the rivers settle, which run their channels on down to it.
    pub low_water: f32,
}

/// River water over one cell.
#[derive(Clone, Copy, Debug)]
pub struct Wet {
    /// Water surface, metres.
    pub level: f32,
    /// Current, metres a second, in x and z.
    pub flow: [f32; 2],
    /// How far the tide may draw this water down, metres: 0 for a lake, a
    /// river that ends in one, and a river above a waterfall; for a river
    /// running to the sea, its depth less a hand, so it never runs dry
    /// (`Tide::river_fall`).
    pub ebb: f32,
}

impl Fields {
    /// How far the beds under cell `i` were lifted (0 where nothing folded).
    pub fn lift_at(&self, i: usize) -> f32 {
        self.lift.get(i).copied().unwrap_or(0.0)
    }

    /// The lift field, allocated on first use.
    pub fn lift_mut(&mut self) -> &mut Vec<f32> {
        if self.lift.is_empty() {
            self.lift = vec![0.0; self.n * self.n];
        }
        &mut self.lift
    }

    /// Bytes held by the per-cell arrays (not the region names).
    pub fn bytes(&self) -> usize {
        let f = |n: usize, b: usize| n * b;
        f(
            self.h.len() + self.cliff.len() + self.moist.len() + self.strange.len() + self.slope.len() + self.coast_dist.len() + self.river_dist.len(),
            4,
        ) + f(self.burn_year.len(), 4)
            + f(self.water.len() + self.surface.len() + self.region.len() + self.occupied.len() + self.biome.len(), 1)
            + f(self.cause.len(), 2)
            + f(self.layers.len(), std::mem::size_of::<[Layer; MAX_LAYERS]>())
    }

    /// Free the fields only the build reads (burn year, region, cause, slope,
    /// strangeness): over half a gigabyte on a 6.4 km world. The game calls
    /// this; the map and gazetteer tools keep them.
    pub fn drop_build_only(&mut self) {
        self.burn_year = Vec::new();
        self.region = Vec::new();
        self.cause = Vec::new();
        self.slope = Vec::new();
        self.strange = Vec::new();
    }

    pub fn new(size: f32, sea: f32) -> Self {
        let n = (size / CELL).round() as usize;
        let nn = n * n;
        Self {
            n,
            size,
            sea,
            h: vec![sea - 12.0; nn],
            cliff: vec![0.0; nn],
            moist: vec![0.5; nn],
            burn_year: vec![i32::MIN; nn],
            strange: vec![0.0; nn],
            water: vec![0; nn],
            surface: vec![0; nn],
            // Allocated on the first buried layer: cities never lay any, and at
            // 36 bytes a cell it is half the fields of a large world.
            layers: Vec::new(),
            region: vec![0; nn],
            regions: Vec::new(),
            cause: vec![0; nn],
            occupied: vec![false; nn],
            slope: vec![0.0; nn],
            coast_dist: vec![0.0; nn],
            river_dist: vec![1e9; nn],
            biome: vec![0; nn],
            inland: HashMap::default(),
            falls: Vec::new(),
            lift: Vec::new(),
            tide: Default::default(),
            shore: Default::default(),
            low_water: sea,
        }
    }

    /// Cell index of a world point, clamped to the grid.
    #[inline]
    pub fn idx(&self, x: f32, z: f32) -> usize {
        let cx = ((x / CELL) as i32).clamp(0, self.n as i32 - 1) as usize;
        let cz = ((z / CELL) as i32).clamp(0, self.n as i32 - 1) as usize;
        cz * self.n + cx
    }

    #[inline]
    pub fn centre(&self, i: usize) -> Vec2 {
        Vec2::new(((i % self.n) as f32 + 0.5) * CELL, ((i / self.n) as f32 + 0.5) * CELL)
    }

    /// Bilinear sample of a per-cell field.
    pub fn sample(&self, f: &[f32], x: f32, z: f32) -> f32 {
        let n = self.n;
        let fx = (x / CELL - 0.5).clamp(0.0, (n - 1) as f32);
        let fz = (z / CELL - 0.5).clamp(0.0, (n - 1) as f32);
        let (x0, z0) = (fx as usize, fz as usize);
        let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let a = f[z0 * n + x0] * (1.0 - tx) + f[z0 * n + x1] * tx;
        let b = f[z1 * n + x0] * (1.0 - tx) + f[z1 * n + x1] * tx;
        a * (1.0 - tz) + b * tz
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        self.sample(&self.h, x, z)
    }

    /// River water over the cell at (x, z), if any. The entry may sit below
    /// the ground on the bank margin: ask `river_depth` for whether it is wet.
    pub fn wet(&self, x: f32, z: f32) -> Option<Wet> {
        if x < 0.0 || z < 0.0 || x >= self.size || z >= self.size || self.inland.is_empty() {
            return None;
        }
        self.inland.get(&(self.idx(x, z) as u32)).copied()
    }

    /// River water over the cell at (x, z) as it stands now, the tide drawn
    /// out of it near the sea (`wet` is the water at high tide, which is what
    /// the generator builds against).
    pub fn wet_now(&self, x: f32, z: f32) -> Option<Wet> {
        let mut w = self.wet(x, z)?;
        w.level -= self.tide.river_fall(w.level, self.sea, w.ebb);
        Some(w)
    }

    /// The sea's surface now: high water less what the tide has drawn out.
    pub fn sea_now(&self) -> f32 {
        self.sea - self.tide.drop()
    }

    /// The sea's surface at (x, z) now: `sea_now` with the swell on it, or
    /// high water over a hollow the open sea cannot reach.
    pub fn sea_at(&self, x: f32, z: f32) -> f32 {
        if self.basin(x, z) {
            return self.sea;
        }
        let level = self.sea_now();
        level + self.shore.swell_at(x, z, level - self.height(x, z), self.size)
    }

    /// True for ground below the sea's level that the open sea cannot reach
    /// (a hollow, a sunken pan): the water there stays at high water whatever
    /// the tide does. Always false without a tide.
    pub fn basin(&self, x: f32, z: f32) -> bool {
        if !self.tide.on() || x < 0.0 || z < 0.0 || x >= self.size || z >= self.size {
            return false;
        }
        let i = self.idx(x, z);
        self.h[i] < self.sea && !self.tide.ocean(i)
    }

    /// Mark the open sea and start the tide (`WorldDef::tide`), after the
    /// chronicle has given the land its last shape.
    pub fn start_tide(&mut self, range: f32) {
        let ocean = super::tide::ocean_cells(self.n, &self.h, self.sea);
        self.tide = super::tide::Tide::new(range, &ocean);
    }

    /// Per texel of an `m` x `m` grid over the world, 1 where any cell under
    /// it is a `basin`, for the renderer's sea (heightmap g): the open sea is
    /// drawn at the tide's level, a basin at high water.
    pub fn basin_grid(&self, m: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; m * m];
        if !self.tide.on() {
            return out;
        }
        let n = self.n;
        for i in 0..n * n {
            if self.h[i] < self.sea && !self.tide.ocean(i) {
                let (x, z) = (i % n, i / n);
                out[(z * m / n).min(m - 1) * m + (x * m / n).min(m - 1)] = 1.0;
            }
        }
        out
    }

    /// Depth of river water at (x, z) over the bare ground, metres (0 where dry).
    pub fn river_depth(&self, x: f32, z: f32) -> f32 {
        self.wet(x, z).map_or(0.0, |w| (w.level - self.height(x, z)).max(0.0))
    }

    /// The river's current at (x, z), metres a second (zero off rivers).
    pub fn current(&self, x: f32, z: f32) -> Vec2 {
        self.wet(x, z).map_or(Vec2::ZERO, |w| Vec2::from(w.flow))
    }

    /// Bilinear share (0..1) of occupied cells around a point.
    pub fn occupied_share(&self, x: f32, z: f32) -> f32 {
        let n = self.n;
        let fx = (x / CELL - 0.5).clamp(0.0, (n - 1) as f32);
        let fz = (z / CELL - 0.5).clamp(0.0, (n - 1) as f32);
        let (x0, z0) = (fx as usize, fz as usize);
        let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let o = |x: usize, z: usize| if self.occupied[z * n + x] { 1.0 } else { 0.0 };
        let a = o(x0, z0) * (1.0 - tx) + o(x1, z0) * tx;
        let b = o(x0, z1) * (1.0 - tx) + o(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
    }

    pub fn region_id(&mut self, name: &str) -> u8 {
        if let Some(i) = self.regions.iter().position(|r| r == name) {
            return i as u8 + 1;
        }
        self.regions.push(name.to_string());
        self.regions.len() as u8
    }

    /// Push every buried layer down and add a new one on top (a deposit of
    /// `thick` metres that raises the ground), or a band `depth` below the surface.
    pub fn add_layer(&mut self, i: usize, mat: u8, depth: f32, thick: f32, raise: bool) {
        if thick <= 0.0 {
            return;
        }
        if self.layers.is_empty() {
            self.layers = vec![[Layer::default(); MAX_LAYERS]; self.n * self.n];
        }
        let ls = &mut self.layers[i];
        if raise {
            for l in ls.iter_mut() {
                if l.mat != 0 {
                    *l = Layer::new(l.mat, l.depth() + thick, l.thick()).unwrap_or_default();
                }
            }
            self.h[i] += thick;
        }
        let Some(new) = Layer::new(mat, depth, thick) else {
            return;
        };
        // Replace the empty or the deepest slot.
        let slot = ls.iter().position(|l| l.mat == 0).unwrap_or_else(|| {
            let mut d = 0;
            for k in 1..MAX_LAYERS {
                if ls[k].depth > ls[d].depth {
                    d = k;
                }
            }
            d
        });
        ls[slot] = new;
    }

    /// Slope, distance to the coast and to flowing water, after the chronicle.
    pub fn derive(&mut self) {
        let n = self.n;
        // Slope is per cell from the heights around it: split by rows.
        let threads = build_threads();
        let rows_per = n.div_ceil(threads).max(1);
        let mut slope = std::mem::take(&mut self.slope);
        slope.clear();
        slope.resize(n * n, 0.0);
        let h = &self.h;
        std::thread::scope(|sc| {
            for (band, out) in slope.chunks_mut(rows_per * n).enumerate() {
                sc.spawn(move || {
                    let at = |x: usize, z: usize| h[z.min(n - 1) * n + x.min(n - 1)];
                    for (k, sl) in out.iter_mut().enumerate() {
                        let (z, x) = (band * rows_per + k / n, k % n);
                        let gx = (at(x + 1, z) - at(x.saturating_sub(1), z)) / (2.0 * CELL);
                        let gz = (at(x, z + 1) - at(x, z.saturating_sub(1))) / (2.0 * CELL);
                        *sl = (gx * gx + gz * gz).sqrt();
                    }
                });
            }
        });
        self.slope = slope;
        // The three distance transforms are sweeps, which cannot be split by
        // rows, but they do not read each other: run them side by side. On a
        // 5.8 km world they are most of this stage.
        let land: Vec<bool> = self.h.iter().map(|&h| h >= self.sea).collect();
        let water = &self.water;
        let (land_ref, (to_sea, to_land, river)) = (&land, {
            let land = &land;
            std::thread::scope(|sc| {
                let a = sc.spawn(move || distance(n, |i| !land[i]));
                let b = sc.spawn(move || distance(n, |i| land[i]));
                let c = sc.spawn(move || distance(n, |i| water[i] == 1));
                (a.join().expect("to_sea"), b.join().expect("to_land"), c.join().expect("river_dist"))
            })
        });
        for i in 0..n * n {
            self.coast_dist[i] = if land_ref[i] { to_sea[i] } else { -to_land[i] };
        }
        self.river_dist = river;
    }
}

/// Chamfer distance transform (metres) to the nearest cell where `seed` holds.
pub fn distance(n: usize, seed: impl Fn(usize) -> bool) -> Vec<f32> {
    let big = 1e9f32;
    let mut d: Vec<f32> = (0..n * n).map(|i| if seed(i) { 0.0 } else { big }).collect();
    let (a, b) = (CELL, CELL * std::f32::consts::SQRT_2);
    for z in 0..n {
        for x in 0..n {
            let i = z * n + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + a);
            }
            if z > 0 {
                v = v.min(d[i - n] + a);
                if x > 0 {
                    v = v.min(d[i - n - 1] + b);
                }
                if x + 1 < n {
                    v = v.min(d[i - n + 1] + b);
                }
            }
            d[i] = v;
        }
    }
    for z in (0..n).rev() {
        for x in (0..n).rev() {
            let i = z * n + x;
            let mut v = d[i];
            if x + 1 < n {
                v = v.min(d[i + 1] + a);
            }
            if z + 1 < n {
                v = v.min(d[i + n] + a);
                if x + 1 < n {
                    v = v.min(d[i + n + 1] + b);
                }
                if x > 0 {
                    v = v.min(d[i + n - 1] + b);
                }
            }
            d[i] = v;
        }
    }
    d
}

/// One point light in the world, in metres and linear colour.
#[derive(Clone, Copy, Debug)]
pub struct WorldLamp {
    pub pos: Vec3,
    pub reach: f32,
    pub colour: [f32; 3],
    pub strength: f32,
    /// Lit only from dusk to dawn (a street lamp, a market's strings).
    pub after_dark: bool,
}

/// A place that breathes particles all the time (event `spring`, a chimney):
/// the game trickles `fx` from it while the camera is near.
#[derive(Clone, Debug)]
pub struct WorldVent {
    pub pos: Vec3,
    /// The effect, a `library/fx/` name.
    pub fx: String,
    /// Particles a second.
    pub rate: f32,
    /// How far across the vent is, metres: each particle starts somewhere in it.
    pub spread: f32,
}

/// Per-biome material ids, resolved once.
#[derive(Clone, Copy, Default)]
pub struct BiomeMats {
    pub surface: u8,
    pub subsurface: u8,
    pub rock: u8,
    /// How far this biome's ground reaches across its edges, metres (0: a
    /// hard edge, and no other biome reaches into it either).
    pub fringe: f32,
}

/// A racing circuit's centreline, a point a metre or so apart, closed, in
/// the direction a race runs; and how wide the road is.
#[derive(Clone, Debug)]
pub struct Circuit {
    pub name: String,
    pub points: Vec<glam::Vec2>,
    /// The road's height at each point, metres.
    pub heights: Vec<f32>,
    pub width: f32,
}

/// A race in this world (world.json `race`): laps of a circuit against
/// bots, everyone in the same vehicle.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RaceDef {
    /// The `circuit` event's name.
    pub circuit: String,
    #[serde(default = "three")]
    pub laps: u32,
    /// Bots racing the player, 1 to 3.
    #[serde(default = "three")]
    pub rivals: u32,
    /// The library vehicle everyone drives.
    pub vehicle: String,
}

fn three() -> u32 {
    3
}

/// A loaded, built world: its settings, the library, the fields and the lists
/// the generator stamps.
pub struct World {
    pub def: WorldDef,
    /// This world's own grading over the style's look (`look.json`), if it
    /// has one; applied in `main::show_world`.
    pub look: Option<crate::library::WorldLook>,
    #[allow(dead_code)] // where the world was loaded from, for tools
    pub dir: std::path::PathBuf,
    pub lib: Library,
    pub f: Fields,
    pub shapes: Vec<Shape>,
    pub placed: Vec<Placed>,
    /// Material ids that are a door leaf. A door on a structure being kept
    /// up is drawn as a platform rather than pressed into the terrain
    /// (`platforms::doors_in`), so the generator skips these.
    pub doors: Vec<u8>,
    pub placed_grid: HashMap<(i32, i32), Vec<u32>>,
    pub trees: Vec<Tree>,
    pub tree_grid: HashMap<(i32, i32), Vec<u32>>,
    pub items: Vec<Item>,
    /// One per item: a key that names it whatever else changes in the
    /// chronicle (`chronicle::item_keys`), which is what the save remembers
    /// a find by.
    pub item_keys: Vec<u32>,
    pub pois: Vec<Poi>,
    /// Named stretches of ground, and the signs that say so (world/places.rs).
    pub areas: Vec<super::places::Area>,
    pub signs: Vec<super::places::Sign>,
    /// Point lights this world places (event `lamp`): the only thing that
    /// lights anything with a roof over it. See `render/lamps.rs`.
    pub lamps: Vec<WorldLamp>,
    /// Steam, smoke: what breathes particles all the time (`WorldVent`).
    pub vents: Vec<WorldVent>,
    pub biome_mats: Vec<BiomeMats>,
    /// Bedrock bands: (material, cumulative weight).
    pub rock: Vec<(u8, f32)>,
    /// Rock formations: (material, base height), sorted by height.
    pub strata: Vec<(u8, f32)>,
    /// Strata materials that are level lines (old water marks), not beds:
    /// neither folding nor `strata_swell` moves them.
    pub strata_level: Vec<u8>,
    pub tubes: Vec<Tube>,
    /// Volumetric rock towers (event `tower`).
    pub towers: Vec<Tower>,
    /// Skeletons drawn as volumes (event `skeleton`).
    pub bones: super::bones::Bones,
    /// Plants the chronicle placed by hand (ledges).
    pub growths: Vec<Growth>,
    /// Placed structures that stand only some of the time (`place` `when`),
    /// by index into `placed`.
    pub when: Vec<(u32, When)>,
    /// Whether each placed structure is drawn now; the game flips these for
    /// the ones in `when` and regenerates the ground they stand on.
    pub shown: Vec<std::sync::atomic::AtomicBool>,
    /// Per placed structure, which of its voxels still stand after decay and
    /// gravity (`gen::standing_map`); None where nothing decayed.
    pub standing: Vec<Option<Vec<u64>>>,
    /// Buildings, decks and piers imported from map data (city worlds).
    pub city: super::city::City,
    /// Routed lines and what is built along them (world/route).
    pub ribbons: super::route::ribbon::Ribbons,
    /// Racing circuits (event `circuit`): their centrelines, for a race.
    pub circuits: Vec<Circuit>,
    pub spawn: Vec3,
    pub spawn_yaw: f32,
    /// What each event did, in order, for the gazetteer and `--validate`.
    pub log: Vec<String>,
    pub warnings: Vec<String>,
    pub build_ms: f32,
}

impl World {
    pub fn index(&mut self) {
        self.city.index();
        self.placed_grid.clear();
        for (k, p) in self.placed.iter().enumerate() {
            let (x0, z0, x1, z1) = p.footprint(&self.shapes);
            let to_b = |v: i32| (v as f32 * 0.25 / BUCKET).floor() as i32;
            for bz in to_b(z0)..=to_b(z1) {
                for bx in to_b(x0)..=to_b(x1) {
                    self.placed_grid.entry((bx, bz)).or_default().push(k as u32);
                }
            }
        }
        self.tree_grid.clear();
        for (k, t) in self.trees.iter().enumerate() {
            let b = |v: f32| (v / BUCKET).floor() as i32;
            for bz in b(t.pos.z - t.radius)..=b(t.pos.z + t.radius) {
                for bx in b(t.pos.x - t.radius)..=b(t.pos.x + t.radius) {
                    self.tree_grid.entry((bx, bz)).or_default().push(k as u32);
                }
            }
        }
    }

    /// Memory held by the grown plants (`trees`), their blobs and limbs, bytes.
    pub fn tree_bytes(&self) -> usize {
        let blob = std::mem::size_of::<super::canopy::Blob>();
        let limb = std::mem::size_of::<(Vec3, Vec3, f32)>();
        self.trees.capacity() * std::mem::size_of::<Tree>() + self.trees.iter().map(|t| t.blobs.capacity() * blob + t.branches.capacity() * limb).sum::<usize>()
    }

    /// Trees whose bounds may touch the box, metres, in a fixed order. Trees
    /// are drawn only into air, so where two overlap the first one wins; the
    /// order must not depend on which chunk asks, or the shared margin of two
    /// neighbouring chunks disagrees.
    pub fn trees_near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<Tree> {
        let mut ks = Vec::new();
        let b = |v: f32| (v / BUCKET).floor() as i32;
        for bz in b(z0)..=b(z1) {
            for bx in b(x0)..=b(x1) {
                if let Some(list) = self.tree_grid.get(&(bx, bz)) {
                    ks.extend_from_slice(list);
                }
            }
        }
        ks.sort_unstable();
        ks.dedup();
        ks.into_iter()
            .map(|k| &self.trees[k as usize])
            .filter(|t| t.pos.x + t.radius >= x0 && t.pos.x - t.radius <= x1 && t.pos.z + t.radius >= z0 && t.pos.z - t.radius <= z1)
            .cloned()
            .collect()
    }

    /// Structures whose footprint may touch the box, metres.
    pub fn placed_near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let b = |v: f32| (v / BUCKET).floor() as i32;
        for bz in b(z0)..=b(z1) {
            for bx in b(x0)..=b(x1) {
                if let Some(list) = self.placed_grid.get(&(bx, bz)) {
                    out.extend(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Bedrock material at a height, banded. `lift` is how far folding raised
    /// the beds here (`Fields::lift_at`); `swell` the slow noise the band
    /// boundaries ride when the world sets `strata_swell`.
    pub fn rock_at(&self, y: f32, lift: f32, off: f32, swell: f32, seed: u32) -> u8 {
        if !self.strata.is_empty() {
            let y_level = y;
            let y = y - lift + off * 0.6;
            let sw = self.def.strata_swell;
            let mut m = self.strata[0].0;
            for (k, &(mat, base)) in self.strata.iter().enumerate() {
                if self.strata_level.contains(&mat) {
                    // A water line: level wherever it is, whatever the beds do.
                    let top = self.strata.get(k + 1).map_or(f32::MAX, |s| s.1);
                    if y_level >= base && y_level < top {
                        return mat;
                    }
                    continue;
                }
                // Each boundary rides one slow noise (`swell`) by its own amount
                // (-1 to 1 of `strata_swell`), so neighbouring bands thicken
                // and thin against each other.
                let base = if sw > 0.0 {
                    base - swell * sw * (super::noise::unit(super::noise::hash2(k as i32, 71, seed)) * 2.0 - 1.0)
                } else {
                    base
                };
                if y >= base {
                    m = mat;
                }
            }
            return m;
        }
        if self.rock.is_empty() {
            return super::material::STONE;
        }
        let band = ((y + off) / 1.3).floor() as i32;
        let r = super::noise::unit(super::noise::hash2(band, 23, seed));
        for &(m, w) in &self.rock {
            if r <= w {
                return m;
            }
        }
        self.rock.last().unwrap().0
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    #[test]
    fn buried_layers_stop_at_the_limit() {
        let mut f = Fields::new(16.0, 0.0);
        f.add_layer(0, 7, 0.5, 1.0, false);
        let l = f.layers[0][0];
        assert!((l.depth() - 0.5).abs() < 0.011 && (l.thick() - 1.0).abs() < 0.011);
        // A deposit on top pushes it down; past the limit it is cut, then gone.
        f.add_layer(0, 9, 0.0, 2.0, true);
        let old = f.layers[0].iter().find(|l| l.mat == 7).unwrap();
        assert!((old.depth() - 2.5).abs() < 0.011 && (old.depth() + old.thick() - BURIED_LIMIT).abs() < 0.011);
        f.add_layer(0, 11, 0.0, 1.0, true);
        assert!(f.layers[0].iter().all(|l| l.mat != 7));
        assert!(Layer::new(5, 3.5, 1.0).is_none());
    }
}

/// Under the water of a world (world.json `underwater`).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Underwater {
    /// Metres of water that hide about two thirds of what is behind them
    /// (the e-folding distance of the green channel; red fades sooner).
    pub visibility: f32,
    /// How fast direct sunlight fades with depth, per metre.
    pub light: f32,
}

impl Default for Underwater {
    fn default() -> Self {
        Underwater { visibility: 14.0, light: 0.16 }
    }
}
