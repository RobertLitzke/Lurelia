//! The games a world can be played as: `library/games/*.json`.
//!
//! A game is a `kind` (which code runs it: `launch::Mode`) and the rules it
//! is played by. A world lists the games it is built for by name
//! (world.json `games`), so two deathmatches with different rules can stand
//! on one map, and a map says nothing about rules it does not care about.
//!
//! Three things meet when a match starts, and each has one owner:
//!
//! - the **map** (world.json `shooter`, `race`): where things are. Spawns,
//!   the nav graph, pads, flag stands, the circuit. It also carries the
//!   rules the map was written with, which a game that says nothing keeps.
//! - the **game** (this file): how it is played. Any rule a game states
//!   replaces the map's.
//! - the **run** (`launch::Chosen`): what the player asked for this time.
//!   Bots, difficulty, target, side.
//!
//! `GameDef::shooter_rules` and `race_rules` are where the three are put
//! together, so `Game` starts an activity without knowing which game it is.
//!
//! A variant says `"like": "deathmatch"` and what it changes
//! (src/inherit.rs), the way a vehicle or a creature does. The keys are flat
//! for the same reason a vehicle's are: `like` replaces nested blocks whole.

use crate::launch::{Chosen, Mode};
use crate::world::fields::{RaceDef, ShooterDef};
use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;

/// One game. Every rule is optional: left out, the map's own stands.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GameDef {
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    introduced_by: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    notes: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    like: Option<String>,
    /// Which code runs it: explore, deathmatch, team, ctf or race.
    pub kind: Mode,
    /// How the launch screen shows it.
    pub title: String,
    /// A sentence for under the launch screen's list.
    #[serde(default)]
    pub help: String,

    // ---- a match (deathmatch, team, ctf) ----
    /// Library weapon names: the guns in this game. A variant names a few of
    /// the map's; a gun the map has no pads for is only ever handed out.
    #[serde(default)]
    pub weapons: Option<Vec<String>>,
    #[serde(default)]
    pub health: Option<f32>,
    #[serde(default)]
    pub health_max: Option<f32>,
    #[serde(default)]
    pub armour: Option<f32>,
    #[serde(default)]
    pub armour_max: Option<f32>,
    #[serde(default)]
    pub respawn_s: Option<f32>,
    /// Hand out every weapon, full, at every spawn.
    #[serde(default)]
    pub give_all: Option<bool>,
    #[serde(default)]
    pub ammo_without_gun: Option<bool>,
    #[serde(default)]
    pub friendly_fire: Option<f32>,
    /// `false` plays the map with nothing lying on its floor.
    #[serde(default)]
    pub pickups: Option<bool>,
    /// The target score the launch screen starts on.
    #[serde(default)]
    pub target: Option<u32>,

    // ---- capture the flag ----
    /// The captures the launch screen starts on.
    #[serde(default)]
    pub captures: Option<u32>,
    /// Seconds a dropped flag lies before it goes home on its own.
    #[serde(default)]
    pub return_s: Option<f32>,
    /// A capture needs your own flag at home.
    #[serde(default)]
    pub need_home: Option<bool>,

    // ---- a race ----
    #[serde(default)]
    pub laps: Option<u32>,
    #[serde(default)]
    pub rivals: Option<u32>,
}

impl GameDef {
    /// The game a kind is played as where no file says: nothing but a name,
    /// so every rule is the map's. What stands in when `library/games/` is
    /// missing one of the five.
    fn plain(kind: Mode) -> GameDef {
        GameDef {
            name: kind.game_name().to_string(),
            version: None,
            introduced_by: None,
            notes: None,
            like: None,
            kind,
            title: kind.title().to_string(),
            help: String::new(),
            weapons: None,
            health: None,
            health_max: None,
            armour: None,
            armour_max: None,
            respawn_s: None,
            give_all: None,
            ammo_without_gun: None,
            friendly_fire: None,
            pickups: None,
            target: None,
            captures: None,
            return_s: None,
            need_home: None,
            laps: None,
            rivals: None,
        }
    }

    /// The rules a match here is played by: the map's, with what this game
    /// states over them, and what this run asked for over that.
    ///
    /// `bedrock` is the world's first rock, which a bore throws up as spoil
    /// where the map names none.
    pub fn shooter_rules(&self, map: &ShooterDef, run: &Chosen, bedrock: Option<&str>) -> ShooterDef {
        let mut r = map.clone();
        // The game over the map.
        if let Some(w) = &self.weapons {
            r.weapons = w.clone();
        }
        if let Some(v) = self.health {
            r.health = v;
        }
        if let Some(v) = self.health_max {
            r.health_max = Some(v);
        }
        if let Some(v) = self.armour {
            r.armour = v;
        }
        if let Some(v) = self.armour_max {
            r.armour_max = Some(v);
        }
        if let Some(v) = self.respawn_s {
            r.respawn_s = v;
        }
        if let Some(v) = self.give_all {
            r.give_all = v;
        }
        if let Some(v) = self.ammo_without_gun {
            r.ammo_without_gun = v;
        }
        if let Some(v) = self.friendly_fire {
            r.friendly_fire = v;
        }
        if self.pickups == Some(false) {
            r.pickups.clear();
        }
        // The run over both. The launch screen says how many bots there are
        // and how well they shoot; asked for exactly what the map's own
        // rules name, the map's own bots are played, names and characters
        // and all.
        if !run.world_roster(&r.bots) {
            r.bots = run.roster();
        }
        r.target_score = run.target;
        // The kind decides what sort of match the map is played as: a map
        // with flags still plays a deathmatch when that is the game.
        r.teams = matches!(self.kind, Mode::Team | Mode::Ctf);
        r.player_team = run.side;
        match (self.kind, r.ctf.as_mut()) {
            (Mode::Ctf, Some(c)) => {
                if let Some(v) = self.return_s {
                    c.return_s = v;
                }
                if let Some(v) = self.need_home {
                    c.need_home = v;
                }
                // What the launch screen asked for; nothing asked for (the
                // command line) keeps the map's own.
                if run.captures > 0 {
                    c.captures = run.captures;
                }
                if run.minutes > 0 {
                    c.time_s = run.minutes as f32 * 60.0;
                }
            }
            (Mode::Ctf, None) => {}
            _ => r.ctf = None,
        }
        if r.spoil.is_none() {
            r.spoil = bedrock.map(str::to_string);
        }
        r
    }

    /// The race here as this game runs it.
    pub fn race_rules(&self, map: &RaceDef) -> RaceDef {
        let mut r = map.clone();
        if let Some(v) = self.laps {
            r.laps = v;
        }
        if let Some(v) = self.rivals {
            r.rivals = v;
        }
        r
    }
}

/// Every game in the library, resolved.
pub struct Catalogue {
    pub games: Vec<GameDef>,
    pub errors: Vec<String>,
}

/// The five kinds, in the order a world with no `games` list is offered
/// them.
pub const KINDS: [Mode; 5] = [Mode::Explore, Mode::Fps, Mode::Team, Mode::Ctf, Mode::Race];

fn read(dir: &Path) -> Catalogue {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut files: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();
        for p in files {
            let file = p.display().to_string();
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()))
            {
                Ok(v) => entries.push((file, v)),
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
    }
    let mut games: Vec<GameDef> = Vec::new();
    for (file, v) in crate::inherit::resolve(entries, &mut errors) {
        match serde_json::from_value::<GameDef>(v) {
            Ok(g) => games.push(g),
            Err(e) => errors.push(format!("{file}: {e}")),
        }
    }
    // The five every world is offered by kind are always there, so a broken
    // or missing file costs a game its rules and help, not the game.
    for kind in KINDS {
        if !games.iter().any(|g| g.name == kind.game_name()) {
            errors.push(format!("library/games: no {}.json, so it is played by the map's own rules", kind.game_name()));
            games.push(GameDef::plain(kind));
        }
    }
    Catalogue { games, errors }
}

/// The library's games, read once: the launch screen asks every frame, and
/// nothing here changes while the game runs.
pub fn catalogue() -> &'static Catalogue {
    static C: OnceLock<Catalogue> = OnceLock::new();
    C.get_or_init(|| {
        let c = read(&crate::library::data_root().join("library").join("games"));
        for e in &c.errors {
            eprintln!("games: {e}");
        }
        c
    })
}

/// A game by name. `fps` and `tdm` are the names older settings and the
/// command line used.
pub fn get(name: &str) -> Option<&'static GameDef> {
    let name = match name {
        "fps" => "deathmatch",
        "tdm" => "team",
        n => n,
    };
    catalogue().games.iter().find(|g| g.name == name)
}

/// The plain game of a kind: `deathmatch` for `Mode::Fps`.
pub fn base(kind: Mode) -> &'static GameDef {
    get(kind.game_name()).expect("read() adds the five plain games")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The library's files parse, the five plain games are files, and every
    /// gun a game names is in the library.
    #[test]
    fn the_library_games_parse() {
        let c = catalogue();
        assert_eq!(c.errors, Vec::<String>::new());
        let weapons = crate::library::data_root().join("library").join("weapons");
        for g in &c.games {
            assert!(!g.title.is_empty(), "{} has no title", g.name);
            for w in g.weapons.iter().flatten() {
                assert!(weapons.join(format!("{w}.json")).exists(), "{}: no weapon called {w}", g.name);
            }
        }
        for kind in KINDS {
            assert_eq!(base(kind).kind, kind);
        }
        assert_eq!(get("fps").map(|g| g.name.as_str()), Some("deathmatch"));
    }

    fn map() -> ShooterDef {
        serde_json::from_value(serde_json::json!({
            "weapons": ["sidearm", "rocket_launcher"],
            "health": 100,
            "pickups": [{"kind": "health", "at": [1.0, 1.0]}],
            "ctf": {"flags": [{"team": 0, "at": [0.0, 0.0]}, {"team": 1, "at": [9.0, 9.0]}], "captures": 5}
        }))
        .unwrap()
    }

    fn game(v: serde_json::Value) -> GameDef {
        serde_json::from_value(v).unwrap()
    }

    /// A rule the game states replaces the map's, and one it leaves out is
    /// the map's still.
    #[test]
    fn a_game_states_rules_over_the_maps() {
        let run = Chosen::for_world("slagdeck");
        let plain = game(serde_json::json!({"name": "d", "kind": "deathmatch", "title": "D"}));
        let r = plain.shooter_rules(&map(), &run, Some("basalt"));
        assert_eq!(r.weapons, ["sidearm", "rocket_launcher"]);
        assert_eq!((r.health, r.pickups.len()), (100.0, 1));
        assert!(r.ctf.is_none() && !r.teams, "a deathmatch on a flag map kept its flags");
        assert_eq!(r.spoil.as_deref(), Some("basalt"));
        let one = game(serde_json::json!({"name": "i", "kind": "deathmatch", "title": "I", "weapons": ["sidearm"], "health": 40, "pickups": false}));
        let r = one.shooter_rules(&map(), &run, None);
        assert_eq!(r.weapons, ["sidearm"]);
        assert_eq!((r.health, r.pickups.len()), (40.0, 0));
    }

    /// Capture the flag keeps the map's flags and count unless the run asks
    /// for another, and the kind is what makes it two sides.
    #[test]
    fn the_kind_decides_the_match() {
        let mut run = Chosen::for_world("slagdeck");
        let ctf = game(serde_json::json!({"name": "c", "kind": "ctf", "title": "C", "return_s": 10}));
        let r = ctf.shooter_rules(&map(), &run, None);
        let c = r.ctf.as_ref().expect("flags");
        assert!(r.teams);
        assert_eq!((c.captures, c.return_s), (5, 10.0));
        run.captures = 7;
        assert_eq!(ctf.shooter_rules(&map(), &run, None).ctf.unwrap().captures, 7);
        let team = game(serde_json::json!({"name": "t", "kind": "team", "title": "T"}));
        let r = team.shooter_rules(&map(), &run, None);
        assert!(r.teams && r.ctf.is_none());
    }
}
