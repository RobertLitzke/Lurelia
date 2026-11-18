//! The rules of a shooter: health, dying, coming back, who killed whom.
//! One `modes::Activity`, started by `Game` on any world whose `world.json`
//! carries a `"shooter"` block.
//!
//! Everything about the guns themselves is `src/combat`, which this calls and
//! never reaches inside; everything about what a bot decides is `src/brain`,
//! likewise; where a gun sits in the player's hands is `hold.rs` and the HUD
//! is `hud.rs`. A different kind of shooter -- one life, no respawn, a rifle
//! and a long valley -- is a second set of rules written here against the
//! same parts (docs/SHOOTER.md, docs/BOTS.md).
//!
//! The player is fighter 0 and the bots are 1.., in one list. Nothing below
//! this file knows which one is the human, so the match would run with no
//! player in it at all.

pub mod corpse;
pub mod gibs;
mod hold;
mod hud;
pub mod pickup;
pub mod round;

use crate::anim::figure;
use crate::brain::{self, avatar, duel, nav, person, Bot, Fighter, Foe, Me, Pilot, Score};
use crate::combat::weapon::{Arsenal, Trigger};
use crate::combat::{viewmodel, Combat, Hitbox, Scene};
use crate::creatures::terrain::{hurt_at, voxel_ground};
use crate::game::{Actions, STEP};
use crate::library::Library;
use crate::models::{Instance, Model};
use crate::modes::{Activity, ActivityCtx, ModeId};
use crate::player::{MoveInput, Player};
use crate::render::hud::Hud;
use crate::world::fields::{ShooterDef, SpawnPoint};
use crate::world::noise::Rng;
use crate::world::store::Store;
use glam::{Quat, Vec3};
use hold::Hold;

/// How fast the view settles back after recoil, degrees a second.
/// Damage a corpse takes for each piece shot off it: about a fifth of a
/// second of minigun, and a sixth of the pulse beam.
const CHIP_HP: f32 = 15.0;
/// How near a round has to pass to a part of a corpse to land on it, metres.
const CHIP_REACH: f32 = 0.4;
/// What a round that chips a piece off puts into it, as a blast's `fling`
/// (`gibs::burst` gives a chunk about a quarter of this).
const CHIP_FLING: f32 = 16.0;
/// How long the dead lie about, in respawn waits: they are cleared away
/// this long after going down, whether or not the bot is back.
const CORPSE_LIFE: f32 = 2.0;
const RECOVER: f32 = 34.0;
/// The most recoil may hold the view up, degrees.
const CLIMB_MAX: f32 = 14.0;
/// Seconds a hit marker stays on the crosshair.
pub(crate) const MARK_S: f32 = 0.3;
/// Seconds the killing-blow marker stays on the crosshair.
pub(crate) const KILL_MARK_S: f32 = 0.5;
/// Seconds the kill banner is up, and the most apart two kills may be to
/// count toward the same streak (DOUBLE KILL and on).
pub(crate) const BANNER_S: f32 = 1.6;
const STREAK_S: f32 = 3.0;
/// Seconds a line of the kill feed is shown for.
const FEED_S: f32 = 3.5;
/// How many times the nav graph is rebuilt while the world streams in, and
/// how many frames apart the attempts are. Frames and not seconds, because
/// what it is waiting for is the streamer, which is budgeted per frame: a
/// headless run goes many times faster than real time and would otherwise
/// give up after a second of it.
const NAV_TRIES: u32 = 90;
const NAV_RETRY_FRAMES: u32 = 15;
/// Spawn points drawn before the one with the most room round it wins.
const SPAWN_DRAWS: usize = 4;
/// Steps the match keeps trying to stand the player on a spawn point, for
/// the same reason as `NAV_TRIES`: the floor under one is not there until
/// the world has streamed in. A match told to leave the player where they
/// are (`pin`) sets it to zero and never looks again.
const PLACE_TRIES: u32 = 150;
/// Metres a second below which the blow that killed a body is not worth
/// throwing it with: it goes over where it stood, as a body that walked into
/// a pit does.
const FLUNG: f32 = 1.5;
/// How far above the world's own spawn floor a respawn looks for its floor.
/// Without this a roofed arena puts bodies on the roof.
const SPAWN_SLACK: f32 = 3.0;
/// How long a noise is worth walking toward, and how far a shot carries.
///
/// Three seconds because that is about how long the information is good
/// for: `HUNT_S` in `brain::duel` is the same number for the same reason,
/// and a bot that sets off toward a gun it heard five seconds ago arrives
/// somewhere nobody is. Forty metres covers Slagdeck end to end, which is
/// the point -- an arena where a firefight is private is an arena where two
/// bots can spend a minute not finding each other.
const EARSHOT_S: f32 = 3.0;
const SHOT_REACH_M: f32 = 40.0;

pub struct Shooter {
    rules: ShooterDef,
    pub combat: Combat,
    /// Where bots may walk (src/brain/nav.rs). Built from the world rather
    /// than at load: see `settle_nav`.
    nav: nav::Graph,
    /// Attempts left at resolving the graph against the real geometry, and
    /// the seconds until the next one. The voxels round a node have to be
    /// streamed in before the floor under it can be found, and at the moment
    /// a match starts only the ground by the spawn point is loaded.
    nav_tries: u32,
    nav_due: u32,
    /// The same cadence for the pads whose ground had not arrived.
    pad_due: u32,
    /// The player: fighter 0.
    pub(crate) me: Fighter,
    /// Everyone else, fighter 1 upward.
    pub(crate) bots: Vec<Bot>,
    /// Degrees of recoil the player's view is still holding.
    climb: f32,
    /// The player is looking down a sight this frame: the right button on a
    /// gun with a `zoom`. The view narrows (`Activity::zoom`) and the HUD
    /// draws the scope.
    pub(crate) sighted: bool,
    pub(crate) mark: f32,
    /// How big the hit marker is drawn, 0 to 1 by the damage that set it
    /// off, and whether that was a head shot.
    pub(crate) mark_size: f32,
    pub(crate) mark_head: bool,
    /// Seconds left of the killing-blow marker.
    pub(crate) mark_kill: f32,
    /// The kill banner above the crosshair: who, the streak it is part of,
    /// and seconds since it went up.
    pub(crate) banner: Option<(String, u32, f32)>,
    /// Kills in the current streak and seconds since the last of them.
    streak: (u32, f32),
    /// Chunks of bodies lying about (gibs.rs): what a blast broke a bot
    /// into, and the pieces shot off a corpse. Seconds since they came off,
    /// and seconds before they are cleared away.
    gibs: Vec<(f32, f32, Vec<gibs::Gib>)>,
    /// The dead, lying where they fell (corpse.rs). Each is its own thing:
    /// the bot it was has usually respawned long before it is cleared.
    pub(crate) corpses: Vec<corpse::Corpse>,
    /// The player's boost meter, mirrored off their body each tick so the
    /// HUD (hud.rs) can draw it without being handed a `Player`. How much is
    /// left, and whether it is being spent.
    pub(crate) boost: Option<(f32, bool)>,
    /// Spawn points are drawn from this, not taken in turn.
    spawn_rng: Rng,
    /// Steps left in which to stand the player on a spawn point, and where
    /// the world left them before that happened. A match begins with the
    /// player wherever `Game` put them, which is not a spawn point, so one
    /// is found for them -- again and again until the floor under it has
    /// streamed in, and not at all once they have walked off by themselves.
    me_to_the_field: u32,
    me_was: Option<Vec3>,
    /// A new match wants the arena back as it was built: the holes the last
    /// one blew in it belong to that match, not to the map.
    clean_world: bool,
    /// The world's own spawn, which is both the fallback spawn and the floor
    /// every nav height is measured from.
    home: (Vec3, f32),
    note: Option<String>,
    /// Seconds since the match began, which the brains measure by.
    now: f32,
    /// The two mouse buttons as they were held this frame (`frame`), for
    /// the steps to pull the trigger with.
    held: (bool, bool),
    /// Enter was pressed since the last step: another match, once the
    /// scoreboard has been up long enough (round.rs).
    again: bool,
    /// Who killed whom, newest last.
    pub(crate) feed: Vec<(String, f32)>,
    /// The gun in the player's hands (hold.rs).
    hold: Hold,
    /// Set once the graph has settled: every bot is re-placed on real ground.
    bots_to_the_field: bool,
    /// The match: the target score, the end of it and the scoreboard
    /// (round.rs).
    round: round::Round,
    /// What the bots are drawn as: the library's robot, rigged
    /// (`brain::avatar`). Only drawing and the muzzle read it.
    avatar: avatar::Avatar,
    /// What is lying about to be picked up (pickup.rs). Placed on the same
    /// clock as the nav graph, for the same reason: the floor under a pad is
    /// not there until the world has streamed in.
    pickups: pickup::Pickups,
    /// Scratch for the pads a bot would be glad of, so thinking does not
    /// allocate once per bot per frame.
    lures: Vec<brain::Lure>,
    /// What has been heard lately. Trimmed to the last `EARSHOT_S` every
    /// time a brain is handed it, so it stays a handful of entries.
    noises: Vec<brain::Noise>,
    /// The player is not in this match (`bench_player`): nothing targets
    /// them and nothing can hurt them. For the tournament, where the only
    /// player is a `Player` standing at the spawn doing nothing -- which is
    /// not a neutral thing to leave in a measurement. It is a free kill for
    /// whoever walks past it, so a quarter of the fight bracket's score was
    /// who found the statue first, and in the movement bracket a lone bot
    /// that spotted it stopped exploring and fought it for the rest of the
    /// round, on the deck floor, which is where the spawn is.
    bench: bool,
}

/// How tall a pickup's column of light stands on a pad of the middle size,
/// metres, and where it starts: clear of the blob turning at knee height,
/// and tall enough to clear a bot standing in front of it.
const BEACON_M: f32 = 2.4;
const BEACON_FOOT_M: f32 = 0.75;
/// How wide the column is drawn at its foot, metres.
const BEACON_W_M: f32 = 0.07;
/// How strong it is at its foot, before the fade up it.
const BEACON_A: f32 = 0.22;
/// How many bands it is drawn in: enough to fade smoothly, few enough that
/// thirty pads are a hundred lines and not a thousand.
const BEACON_BANDS: usize = 5;
/// Nearer than this and the column is not drawn, metres.
const BEACON_NEAR_M: f32 = 2.5;

/// A fighter as this world's rules have it: its guns, its health, the
/// armour it starts in and the armour it can wear. The two armours are
/// different numbers because of the pads -- an arena with armour on its
/// floor issues none at spawn and a capacity of `armour_max` to fill from
/// them, and one without issues what it wants worn.
/// The seed for the spawn stream, which is the one thing here the clock
/// gets a say in. Everything else a match draws is seeded from the world, so
/// that a headless run repeats; but a seed from the world is the same seed
/// every time you press play, and then every match on a map opens in the
/// same corner, which is the thing this is meant to stop.
fn spawn_seed(seed: u32) -> u32 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u32);
    crate::world::noise::hash32(seed ^ now) | 1
}

/// Stand a body at `pos` facing `yaw`, carrying nothing over from where it
/// was: no velocity, and no trail for the draw to interpolate along.
fn stand(player: &mut Player, pos: Vec3, yaw: f32) {
    player.pos = pos;
    player.prev_pos = pos;
    player.body.vel = Vec3::ZERO;
    player.yaw = yaw;
}

fn new_fighter(lib: &Library, rules: &ShooterDef) -> Fighter {
    let mut f = Fighter::new(arsenal(lib, rules), rules.health, rules.armour);
    f.armour_cap(rules.armour_max.unwrap_or(rules.armour));
    f.health_cap(rules.health_max.unwrap_or(crate::brain::HEALTH_MAX));
    f
}

impl Shooter {
    /// `home` is the world's spawn point and bearing: the fallback when the
    /// rules list no spawns of their own, and the floor every nav node's
    /// height is measured from.
    ///
    /// The spawn draw is seeded from the clock (`spawn_seed`), so two
    /// matches on one map do not open in the same corner. `repeatable` is
    /// the same match with that drawn from `seed` instead, for anything
    /// measuring.
    pub fn new(lib: &Library, rules: &ShooterDef, home: (Vec3, f32), store: &Store, seed: u32) -> Shooter {
        Shooter::make(lib, rules, home, store, seed, spawn_seed(seed))
    }

    /// A match that repeats: the spawn points come out of `seed` rather than
    /// off the clock, so the same arguments give the same match.
    ///
    /// This exists because they did not. The tournament and the weapon bench
    /// both run a round, change one thing and run it again, and both were
    /// built on the belief -- written down in `tools::tourney`'s own header
    /// -- that there is no wall clock anywhere in the loop. There was one,
    /// in `spawn_seed`, and it put the bots somewhere else every round: the
    /// same bot on the same seed dealt 64 damage and then 101. That is most
    /// of why the fight bracket could never separate anything, and it was
    /// found by having the bracket check whether a round repeats at all.
    pub fn repeatable(lib: &Library, rules: &ShooterDef, home: (Vec3, f32), store: &Store, seed: u32) -> Shooter {
        Shooter::make(lib, rules, home, store, seed, crate::world::noise::hash32(seed ^ 0x5bd1) | 1)
    }

    fn make(lib: &Library, rules: &ShooterDef, home: (Vec3, f32), store: &Store, seed: u32, spawns: u32) -> Shooter {
        let mut me = new_fighter(lib, rules);
        me.revive(rules.give_all);
        let mut sh = Shooter {
            rules: rules.clone(),
            combat: Combat::new(seed ^ 0x5110),
            nav: nav::Graph::default(),
            nav_tries: NAV_TRIES,
            nav_due: 0,
            pad_due: 0,
            me,
            bots: Vec::new(),
            climb: 0.0,
            sighted: false,
            mark: 0.0,
            mark_size: 0.0,
            mark_head: false,
            mark_kill: 0.0,
            banner: None,
            streak: (0, 0.0),
            gibs: Vec::new(),
            corpses: Vec::new(),
            boost: None,
            spawn_rng: Rng::new(spawns),
            me_to_the_field: PLACE_TRIES,
            me_was: None,
            clean_world: false,
            home,
            note: None,
            now: 0.0,
            held: (false, false),
            again: false,
            feed: Vec::new(),
            hold: Hold::default(),
            bots_to_the_field: false,
            bench: false,
            round: round::Round::new(rules.target_score),
            // The palette slots were filled when the models were built
            // (`Game::add_ride_models`); this call wants the rig, and
            // `build` is pure in the library, so the two agree.
            avatar: avatar::build(Some(lib), &mut crate::models::palette()),
            pickups: pickup::Pickups::default(),
            lures: Vec::new(),
            noises: Vec::new(),
        };
        let want = rules.bots.clone();
        let mut taken: Vec<Vec3> = Vec::new();
        for (i, def) in want.iter().enumerate() {
            let Some(base) = person::level(&def.skill) else {
                eprintln!("world.json shooter: no difficulty called {} (one of {:?})", def.skill, person::NAMES);
                continue;
            };
            let at = sh.next_spawn(&taken);
            let pos = sh.floor_at(store, at.at[0], at.at[1], at.ceiling, sh.home.0.y).0;
            taken.push(pos);
            let mut body = Player::new(pos, -at.bearing.to_radians());
            body.pitch = 0.0;
            // The shooter's rules are what put a body on the arena's
            // movement -- the boost, the dodges, three jumps -- and a bot
            // walks rather than rides (`Player::walk`), so it is set here
            // rather than read off a ride every step.
            body.arena = true;
            body.air_jumps = 3;
            // Its own colour out of the roster, unless the world insisted
            // on one. Nobody has to name a colour for a match to have every
            // bot in it told apart.
            let tint = def.colour.map_or_else(|| avatar::team_tint(i), |c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]);
            let mut fighter = new_fighter(lib, rules);
            fighter.revive(rules.give_all);
            sh.bots.push(Bot::new(
                &def.name,
                body,
                fighter,
                base.flavour(def.bold, def.steady),
                Box::new(duel::Duel::playing(
                    seed ^ 0x9e37 ^ (i as u32 + 1).wrapping_mul(0x85eb),
                    duel::Style::without(&def.without, |w| eprintln!("world.json shooter: bot {}: {w}", def.name)),
                )),
                tint,
                &sh.avatar,
                seed ^ (i as u32 + 1).wrapping_mul(0x27d4),
            ));
        }
        if !sh.bots.is_empty() && sh.rules.nav.nodes.is_empty() {
            eprintln!("world.json shooter: {} bots and no nav graph, so they will stand still", sh.bots.len());
        }
        sh.combat.spoil_mat = rules.spoil.as_deref().and_then(|n| lib.material(n)).unwrap_or(0);
        sh
    }

    /// The floor the world's own spawn point stands on: the datum every
    /// height in `world.json` is measured from, nav nodes and pads alike,
    /// and the same one `Game` hands the platforms.
    ///
    /// Not the floor under the player, which is where this used to look. By
    /// the time anything asks, the player is standing on one of the match's
    /// spawn points, and on a map with storeys that is some other floor: on
    /// one imported map it put the datum 4.75 m up and carried every node and pad with
    /// it, which is what the "wanted 31.2 m and found floor at 26.5 m"
    /// warnings were. The player's floor is kept as the fallback, because it
    /// is the one column certain to be loaded, and the retries are there for
    /// the spawn's own chunk arriving late.
    fn datum(&self, store: &Store, player: &Player) -> f32 {
        let home = self.home.0;
        voxel_ground(store, home.x, home.z, home.y + 1.0)
            .or_else(|| voxel_ground(store, player.pos.x, player.pos.z, player.pos.y + 1.0))
            .map_or(home.y, |(y, _)| y)
    }

    /// Try to resolve the nav graph against the voxels, and keep trying
    /// until every node finds its floor.
    ///
    /// A world states each node's height as metres above the floor its own
    /// spawn point stands on, and the real floor is whatever the structure
    /// that was built there actually left. Both of those need the chunk to
    /// be loaded, and at the moment a match starts only the ground round the
    /// spawn is. So this runs every half second until nothing is missing,
    /// and gives up after a few seconds rather than retrying forever on a
    /// graph whose nodes are simply in the wrong place.
    fn settle_nav(&mut self, store: &Store, player: &Player) {
        self.nav_due = self.nav_due.saturating_sub(1);
        if self.nav_tries == 0 || self.nav_due > 0 {
            return;
        }
        self.nav_due = NAV_RETRY_FRAMES;
        self.nav_tries -= 1;
        let last = self.nav_tries == 0;
        let base = self.datum(store, player);
        let mut missing = 0;
        let nav = nav::Graph::build(
            &self.rules.nav,
            base,
            |x, z, under| voxel_ground(store, x, z, under).map(|(y, _)| y),
            |at| hurt_at(store, at, 0.3) > 0.0,
            |w| {
                missing += 1;
                if last {
                    eprintln!("world.json shooter nav: {w}");
                }
            },
        );
        // Take the best graph going every time rather than holding out for
        // a perfect one: a node whose chunk is still loading keeps the
        // height the world stated, which is roughly right, and a bot with
        // most of a map is far better off than a bot with none of it.
        self.nav = nav;
        let settled = missing == 0 || last;
        if settled {
            self.nav_tries = 0;
        }
        // Only once the graph has settled, which is also when the ground
        // under the spawn points has streamed in. Placing them on the first
        // attempt drops every bot two metres at the start of a match,
        // because `floor_at` falls back to the world's spawn height when
        // the column it is asking about is not loaded yet.
        if settled {
            // The ground under the bots exists now, so put them on it: when
            // they were made it did not, and they were left standing at the
            // world's own spawn height.
            self.bots_to_the_field = true;
        }
        // The pads go down on the first pass, settled or not.
        //
        // They used to wait for the graph with the bots, and on Slagdeck the
        // graph never settles -- a handful of its nodes never find a floor --
        // so the pads were laid by the give-up branch ninety tries later.
        // That is twenty-two seconds of a match with nothing on the floor of
        // the arena at all, which is most of what "I never see a health pack"
        // was. A pad whose column has not loaded takes the height the world
        // stated, which is right to a voxel or two, and `settle_pads` asks
        // again for the ones that need it -- the machinery was already there
        // and had nothing to do, because nothing was ever placed early.
        if self.pickups.is_empty() && !self.rules.pickups.is_empty() {
            let defs = self.rules.pickups.clone();
            let mut bad = Vec::new();
            self.pickups = pickup::Pickups::new(&defs, |x, z, up| self.pad_floor(store, base, x, z, up), |w| bad.push(w));
            for w in bad {
                eprintln!("world.json shooter pickups: {w}");
            }
        }
    }

    pub fn alive(&self) -> bool {
        self.me.alive()
    }

    /// The nav graph, for the headless tools.
    /// Seconds of match the activity has seen, which is not wall clock.
    pub fn elapsed(&self) -> f32 {
        self.now
    }

    pub fn nav(&self) -> &nav::Graph {
        &self.nav
    }

    /// Everyone's name and score, player first. The one way to read the
    /// state of a match from outside: a score limit, a final scoreboard or a
    /// test wants this and not the fighters themselves.
    pub fn scores(&self) -> Vec<(String, Score)> {
        let mut out = vec![("YOU".to_string(), self.me.score)];
        for b in &self.bots {
            out.push((b.name.to_uppercase(), b.fighter.score));
        }
        out
    }

    /// What each bot is doing and where, for the headless tools: enough to
    /// tell a bot that is fighting from one stuck in a corner.
    /// A column of light over every pad still standing.
    ///
    /// Before this the only mark on a pickup was the blob turning over it,
    /// a quarter of a metre across, at knee height, on a deck fifty metres
    /// end to end -- and the blob was drawing an empty mesh, so there was
    /// no mark at all and a health pack was something you found by walking
    /// into it. The column is depth-tested, so a wall still hides one: it
    /// says where a thing is from across a room, not through the floor.
    ///
    /// It fades out with height rather than ending, because a column drawn
    /// to full height at full strength reads as a post you could walk into.
    fn beacons(&self, player: &Player, out: &mut crate::render::overlay::Overlay) {
        for p in &self.pickups.pads {
            if !p.ready() {
                continue;
            }
            // Standing on it, the column is in your face and says nothing
            // you do not already know.
            let flat = Vec3::new(p.at.x - player.pos.x, 0.0, p.at.z - player.pos.z).length();
            if flat < BEACON_NEAR_M {
                continue;
            }
            let tall = BEACON_M * p.size().map_or(1.0, pickup::Size::scale);
            let t = p.kind.tint();
            for i in 0..BEACON_BANDS {
                let up = |k: usize| p.at + Vec3::Y * (BEACON_FOOT_M + tall * k as f32 / BEACON_BANDS as f32);
                let fade = 1.0 - i as f32 / BEACON_BANDS as f32;
                out.line(up(i), up(i + 1), BEACON_W_M * fade.max(0.3), [t[0], t[1], t[2], BEACON_A * fade * fade]);
            }
        }
    }

    /// How many pads are standing, out of how many there are: the cheapest
    /// way to see from outside whether anybody is picking anything up.
    pub fn pads_standing(&self) -> (usize, usize) {
        (self.pickups.pads.iter().filter(|p| p.ready()).count(), self.pickups.pads.len())
    }

    /// Force every bot's aim to the same value, so a measurement of
    /// anything else is not really a measurement of aim (`tools::tourney`).
    /// Perception and judgement are left alone: it is the cone that
    /// dominates, and flattening the whole personality would leave nothing
    /// to tell entrants apart.
    /// Give one fighter a weapon table of its own -- 0 is the player, 1 up
    /// are the bots. The bench puts one gun against another, and a world
    /// hands the same arsenal to everybody, so this is the one way to ask
    /// what the skipjack does to the sniper rifle (`tools::duel`). The
    /// arsenal object survives every respawn, so one call holds for a
    /// whole match.
    pub fn arm(&mut self, who: usize, lib: &Library, weapons: &[String], full: bool) {
        let mut bad = Vec::new();
        let a = Arsenal::new(lib, weapons, full, |n| bad.push(n.to_string()));
        for n in &bad {
            eprintln!("arm: no weapon called {n}");
        }
        let f = if who == 0 {
            &mut self.me
        } else {
            match self.bots.get_mut(who - 1) {
                Some(b) => &mut b.fighter,
                None => return,
            }
        };
        f.arsenal = a;
        f.revive(full);
    }

    /// Take the player out of the match: nothing aims at them, nothing
    /// hurts them, and nothing they are hit by scores. For a measurement
    /// with no human in it (`tools::tourney`), where the player is a body
    /// standing at the spawn and would otherwise be both a free kill and a
    /// reason for a bot to stop walking the map.
    pub fn bench_player(&mut self) {
        self.bench = true;
    }

    pub fn handicap(&mut self, aim: f32) {
        for b in &mut self.bots {
            b.person.aim = aim;
        }
    }

    /// What is wrong with this world's nav graph, as a body standing at the
    /// spawn sees it (`nav::Graph::audit`). For the headless report: a map
    /// whose graph has one-way pockets in it plays like a map whose bots are
    /// broken, and there is nothing else that would say which it is.
    pub fn nav_audit(&self) -> serde_json::Value {
        let from = self.nav.nearest(self.home.0).unwrap_or(0);
        let (hurts, marooned, trapped) = self.nav.audit(from, crate::brain::nav::Able::default());
        serde_json::json!({
            "from": from,
            "hurts": hurts,
            "marooned": marooned,
            "trapped": trapped,
        })
    }

    pub fn bot_report(&self) -> Vec<serde_json::Value> {
        self.bots
            .iter()
            .map(|b| {
                serde_json::json!({
                    "name": b.name,
                    "doing": b.brain.doing(),
                    "health": b.fighter.health,
                    "at": [b.pos().x, b.pos().y, b.pos().z],
                    "alive": b.fighter.alive(),
                    "vel": [b.vel().x, b.vel().y, b.vel().z],
                    "on_ground": b.on_ground(),
                    "gun": b.fighter.arsenal.current().map(|g| g.def.display.clone()),
                    "decides": b.brain.worked().0,
                    "looks": b.brain.worked().1,
                    // How often it has had to shove itself out of something
                    // it was wedged in. The number to watch on a map nobody
                    // walked: on a hand-drawn arena it should stay near zero.
                    "jams": b.brain.jams(),
                    // Dodges taken. Four body steps each, so no sampled
                    // frame would ever catch one.
                    "dodges": b.brain.dodges(),
                    // Rounds on each button, and orbs it set off itself:
                    // whether a bot ever uses the right button is otherwise
                    // invisible from outside.
                    "left": b.brain.triggers().0,
                    "right": b.brain.triggers().1,
                    "banks": b.brain.banks(),
                    "armour": b.fighter.armour,
                    // The floor: what it wanted, what it set off for, and
                    // what it actually picked up.
                    "picked": b.fighter.picked,
                    "fetches": b.brain.scouting().fetches,
                    "gave_up": b.brain.scouting().gave_up,
                    "lures": b.brain.scouting().lures,
                    "want": b.brain.scouting().want,
                    "blocked": b.brain.scouting().blocked,
                    "reach": [b.brain.scouting().reach, b.brain.scouting().nodes],
                    "portal_routes": b.brain.scouting().portal_routes,
                    "lift_routes": b.brain.scouting().lift_routes,
                    "routes": b.brain.wandering(),
                    "buried": b.buried,
                    "buried_at": [b.buried_at.x, b.buried_at.y, b.buried_at.z],
                    "carrying": b.fighter.arsenal.guns.iter().filter(|g| g.have).count(),
                    "foe": b.brain.aiming_at().0,
                    "range": b.brain.aiming_at().1,
                    "known": b.brain.aiming_at().2,
                    "seen": b.brain.aiming_at().3,
                })
            })
            .collect()
    }

    /// The floor at a spawn. Looked for from just above the world's own
    /// spawn floor unless the point names its own ceiling: a roofed arena
    /// searched from the top of the world puts bodies on the roof.
    /// Also says whether it found ground: a column that has not streamed
    /// in yet gives `miss` back, and whoever asked may want to ask again.
    fn floor_at(&self, store: &Store, x: f32, z: f32, ceiling: Option<f32>, miss: f32) -> (Vec3, bool) {
        // The graph's own base once it has settled, because that is the
        // floor the geometry actually left rather than the one the world
        // file guessed at.
        let base = self.nav.at(0).map_or(self.home.0.y, |n| n.at.y);
        let under = ceiling.unwrap_or(base + SPAWN_SLACK);
        match voxel_ground(store, x, z, under) {
            Some((gy, _)) => (Vec3::new(x, gy + 0.1, z), true),
            None => (Vec3::new(x, miss, z), false),
        }
    }

    /// The floor under a pad. A pad states its own height, and that height
    /// -- not the world's spawn, which on a map with storeys can be a storey out -- is what
    /// it keeps while its column is still loading.
    fn pad_floor(&self, store: &Store, base: f32, x: f32, z: f32, up: Option<f32>) -> (Vec3, bool) {
        let stated = up.map(|u| base + u);
        self.floor_at(store, x, z, stated, stated.unwrap_or(self.home.0.y))
    }

    /// Ask again for the floor under any pad that had none when the graph
    /// settled. The arena is big and the pads are all over it, so on the
    /// frame the graph settles half their columns are still streaming;
    /// without this they keep the stated height for the whole match, which
    /// is a gun sunk in the floor wherever the world file rounded.
    fn settle_pads(&mut self, store: &Store, player: &Player) {
        if self.pickups.unplaced() == 0 {
            return;
        }
        self.pad_due = self.pad_due.saturating_sub(1);
        if self.pad_due > 0 {
            return;
        }
        self.pad_due = NAV_RETRY_FRAMES;
        let base = self.datum(store, player);
        // Out and back so the closure may read the rest of `self`.
        let mut pads = std::mem::take(&mut self.pickups);
        pads.settle(|x, z, up| self.pad_floor(store, base, x, z, up));
        self.pickups = pads;
    }

    /// Where the next body comes back: of a few spawn points drawn at
    /// random, the one standing farthest from everybody in `away_from`. The
    /// world's own spawn when the rules list none.
    ///
    /// It used to march down the list in turn, which hands out the same
    /// opening corner every match and, on a list that begins somewhere
    /// awkward, hands out that corner again and again.
    fn next_spawn(&mut self, away_from: &[Vec3]) -> SpawnPoint {
        if self.rules.spawns.is_empty() {
            return SpawnPoint {
                at: [self.home.0.x, self.home.0.z],
                bearing: self.home.1,
                ceiling: None,
            };
        }
        let mut best: Option<(f32, SpawnPoint)> = None;
        for _ in 0..SPAWN_DRAWS {
            let i = ((self.spawn_rng.unit() * self.rules.spawns.len() as f32) as usize).min(self.rules.spawns.len() - 1);
            let s = self.rules.spawns[i];
            let room = away_from.iter().map(|p| (p.x - s.at[0]).hypot(p.z - s.at[1])).fold(f32::INFINITY, f32::min);
            if best.map_or(true, |(r, _)| room > r) {
                best = Some((room, s));
            }
        }
        best.expect("SPAWN_DRAWS is not zero").1
    }

    /// Stand the player on a spawn point at the start of a match.
    ///
    /// `Game` starts them where the world says, or where `progress.json`
    /// remembers -- neither of which is a spawn point. A deathmatch opens on
    /// one, the way every other life in it begins.
    /// Leave the player where they are: they were put there on purpose
    /// (`--pos`, which is how the arena is measured and screenshotted), and
    /// a match that stood them on a spawn point instead would report on
    /// somewhere other than the place the caller asked about.
    pub fn pin(&mut self) {
        self.me_to_the_field = 0;
    }

    fn stand_me(&mut self, player: &mut Player, store: &Store) {
        if self.me_to_the_field == 0 || !self.me.alive() {
            return;
        }
        let was = *self.me_was.get_or_insert(player.pos);
        if (player.pos.x - was.x).hypot(player.pos.z - was.z) > 0.75 {
            self.me_to_the_field = 0; // up and playing: leave them where they are
            return;
        }
        self.me_to_the_field -= 1;
        let away: Vec<Vec3> = self.bots.iter().map(|b| b.pos()).collect();
        let s = self.next_spawn(&away);
        let (pos, found) = self.floor_at(store, s.at[0], s.at[1], s.ceiling, self.home.0.y);
        if !found {
            return; // that corner of the world is not in yet: try again
        }
        // The pitch is left alone. It is level at the start of a match
        // anyway, and this is a placement rather than a respawn.
        stand(player, pos, -s.bearing.to_radians());
        self.me_to_the_field = 0;
    }

    /// Everyone who can be shot, player first. Rebuilt every step, because
    /// it is what every shot this step is resolved against.
    fn hitboxes(&self, player: &Player) -> Vec<Hitbox> {
        let mut out = Vec::with_capacity(self.bots.len() + 1);
        out.push(Hitbox {
            foot: player.pos,
            height: player.body.height,
            half_w: player.body.half_w,
            alive: self.me.alive(),
        });
        for b in &self.bots {
            // Wide enough to cover where the bot was drawn as well as where
            // it now is.
            //
            // Bodies move on a fixed tick and are drawn between the last
            // tick and this one (`instances` interpolates), so the figure
            // you are shooting at is up to one tick behind the box the shot
            // is tested against. Standing still that is nothing. Strafing at
            // six metres a second it is ten centimetres against a body
            // thirty wide -- a third of it, gone, and worst on exactly the
            // bot that is already hardest to hit. The player keeps its true
            // box: a bot fires on the tick and has no lag to allow for, so
            // widening that one would only mean being shot by a bot that
            // missed.
            let lag = b.pos().distance(b.body.prev_pos).min(0.25);
            out.push(Hitbox {
                foot: b.pos(),
                height: b.height(),
                half_w: b.half_w() + lag,
                alive: b.fighter.alive(),
            });
        }
        out
    }

    /// Stop fighters standing inside each other.
    ///
    /// Bodies move through `Body::update`, which knows about the world and
    /// nothing else, and making it know about every other body would put a
    /// list of fighters inside the movement code the whole bot design rests
    /// on being free of. So they are allowed to overlap for an instant and
    /// then pushed apart here, which is a weaker guarantee than a solver and
    /// is the right one: two bodies shoving each other resolve over a few
    /// ticks instead of locking.
    ///
    /// The dead are left out. A corpse is scenery and walking through one is
    /// better than being walled in by it.
    fn unstack(&mut self, store: &Store, player: &mut Player) {
        let mut pos: Vec<Vec3> = Vec::with_capacity(self.bots.len() + 1);
        let mut size: Vec<(f32, f32, bool)> = Vec::with_capacity(self.bots.len() + 1);
        pos.push(player.pos);
        size.push((player.body.half_w, player.body.height, self.me.alive()));
        for b in &self.bots {
            pos.push(b.pos());
            size.push((b.half_w(), b.height(), b.fighter.alive()));
        }
        let mut shove = vec![Vec3::ZERO; pos.len()];
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                if !size[i].2 || !size[j].2 {
                    continue;
                }
                let push = crate::physics::separate(pos[i], size[i].0, size[i].1, pos[j], size[j].0, size[j].1);
                shove[i] += push * 0.5;
                shove[j] -= push * 0.5;
            }
        }
        for (i, by) in shove.into_iter().enumerate() {
            if by == Vec3::ZERO {
                continue;
            }
            let (hw, h, _) = size[i];
            if i == 0 {
                crate::physics::nudge(store, &mut player.pos, by, hw, h);
            } else {
                crate::physics::nudge(store, &mut self.bots[i - 1].body.pos, by, hw, h);
            }
        }
    }

    /// Hand out whatever anybody is standing on.
    ///
    /// After the bodies have moved and before anything is fired, so a bot
    /// that runs over an ammo pad has the rounds this step rather than next.
    fn collect(&mut self, player: &mut Player, dt: f32) {
        if self.pickups.is_empty() {
            return;
        }
        self.pickups.step(dt);
        if self.me.alive() {
            if let Some(note) = self.pickups.walk_over(player.pos, &mut self.me) {
                self.note = Some(note);
            }
        }
        for b in &mut self.bots {
            if b.fighter.alive() {
                // A bot takes its pickups in silence.
                self.pickups.walk_over(b.body.pos, &mut b.fighter);
            }
        }
    }

    /// The same list as a brain sees it.
    fn foes(&self, player: &Player) -> Vec<Foe> {
        let mut out = Vec::with_capacity(self.bots.len() + 1);
        out.push(Foe {
            who: 0,
            foot: player.pos,
            vel: player.body.vel,
            height: player.body.height,
            alive: self.me.alive() && !self.bench,
        });
        for (i, b) in self.bots.iter().enumerate() {
            out.push(Foe {
                who: i + 1,
                foot: b.pos(),
                vel: b.vel(),
                height: b.height(),
                alive: b.fighter.alive(),
            });
        }
        out
    }

    fn name_of(&self, who: usize) -> String {
        match who {
            0 => "YOU".to_string(),
            i => self.bots.get(i - 1).map_or_else(|| format!("#{i}"), |b| b.name.to_uppercase()),
        }
    }

    /// The player just killed fighter `who`: the killing-blow marker, and
    /// the banner over the crosshair with the streak it is part of.
    fn killed_one(&mut self, who: usize) {
        self.mark_kill = KILL_MARK_S;
        self.streak = if self.streak.0 > 0 && self.streak.1 < STREAK_S { (self.streak.0 + 1, 0.0) } else { (1, 0.0) };
        self.banner = Some((self.name_of(who), self.streak.0, 0.0));
    }

    /// Bot `i` has just been killed: sparks fly off its chest lamps, and if a
    /// burst did it the body comes apart, lamps and all. Done here, at the
    /// moment of death, because the pose it died in is the one the pieces
    /// start from.
    fn break_lamps(&mut self, i: usize, fling: Vec3, blast: bool) {
        let Some(b) = self.bots.get(i) else { return };
        let av = &self.avatar;
        let lamps = av.lamps(&b.figure, b.body.pos, b.body.yaw, 1.0);
        for l in lamps {
            self.combat.sparks(l + Vec3::Y * avatar::LAMP_RISE);
        }
        let pose = b.figure.at(1.0);
        let mut fk = Vec::new();
        crate::anim::forward(&av.rig, &pose, av.voxel, av.pivot, &mut fk);
        let yq = Quat::from_rotation_y(b.body.yaw);
        let seed = (i as u32 + 1).wrapping_mul(0x2545_f491) ^ b.fighter.score.deaths.wrapping_mul(0x9e37_79b9);
        let life = self.rules.respawn_s * CORPSE_LIFE;
        if !blast {
            // Down whole: a body of its own, which falls the way the blow
            // sends it and lies there after the bot has come back.
            let parents: Vec<Option<usize>> = av.rig.bones.iter().map(|b| b.parent).collect();
            let parts: Vec<(Vec3, Quat)> = fk.iter().map(|&(p, q)| (b.body.pos + yq * p, yq * q)).collect();
            self.corpses.push(corpse::Corpse::new(i, &parents, &parts, &lamps, fling, life, seed, STEP));
            if self.corpses.len() > corpse::MOST {
                self.corpses.remove(0);
            }
            return;
        }
        let mut parts: Vec<(gibs::Part, Vec3, Quat)> = fk.iter().enumerate().map(|(k, &(p, q))| (gibs::Part::Bone(k), b.body.pos + yq * p, yq * q)).collect();
        parts.extend(lamps.iter().map(|&l| (gibs::Part::Lamp, l, yq)));
        let chest = b.body.pos + Vec3::Y * b.height() * 0.55;
        let chunks = gibs::burst(&parts, chest, fling, seed);
        self.gibs.push((0.0, life, chunks));
    }

    /// Shoot pieces off the dead. Every round and beam this step
    /// (`Combat::take_strikes`) that passes through a corpse puts its damage
    /// towards the next piece, and every `CHIP_HP` of it knocks one off: the
    /// outermost part left nearest where the fire is landing (a hand before
    /// its forearm, a forearm before the arm), which flies off along the
    /// shot and lies where it lands. Hold a minigun or a pulse beam on one
    /// and it comes apart a bit at a time until nothing is left.
    ///
    /// The dead are not hitboxes, so nothing here stops a round: it goes on
    /// through what it chipped.
    fn chip(&mut self) {
        let strikes = self.combat.take_strikes();
        if strikes.is_empty() {
            return;
        }
        let av = &self.avatar;
        let parents: Vec<Option<usize>> = av.rig.bones.iter().map(|b| b.parent).collect();
        let mut thrown: Vec<(f32, f32, gibs::Gib)> = Vec::new();
        let mut sparks = Vec::new();
        for (n, c) in self.corpses.iter_mut().enumerate() {
            // Every part still on it, where it is now.
            let mut parts: Vec<(gibs::Part, u32, Vec3, Quat)> = c.parts(1.0).map(|(k, p, q)| (gibs::Part::Bone(k), k as u32, p, q)).collect();
            parts.extend(c.lamps(1.0).map(|(j, l)| (gibs::Part::Lamp, (corpse::LAMP_BIT + j) as u32, l, Quat::IDENTITY)));
            if parts.is_empty() {
                continue;
            }
            let (mut aim, mut along) = (None, Vec3::Y);
            for &(a, z, damage) in &strikes {
                let seg = z - a;
                let near = parts
                    .iter()
                    .map(|p| {
                        let t = ((p.2 - a).dot(seg) / seg.length_squared().max(1e-6)).clamp(0.0, 1.0);
                        let c = a + seg * t;
                        (c.distance(p.2), c)
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0));
                if let Some((d, at)) = near {
                    if d < CHIP_REACH {
                        c.owed += damage;
                        aim = Some(at);
                        along = seg.normalize_or(Vec3::Y);
                    }
                }
            }
            let Some(aim) = aim else { continue };
            while c.owed >= CHIP_HP && !parts.is_empty() {
                c.owed -= CHIP_HP;
                // The outermost parts: a part with nothing still hanging
                // off it, and the lamps, which hang off nothing.
                let pick = parts
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| match p.0 {
                        gibs::Part::Bone(k) => !c.holds(&parents, k),
                        gibs::Part::Lamp => true,
                    })
                    .min_by(|x, y| x.1 .2.distance(aim).total_cmp(&y.1 .2.distance(aim)))
                    .map(|(i, _)| i);
                let Some(i) = pick else { break };
                let (part, bit, at, rot) = parts.swap_remove(i);
                c.missing |= 1 << bit;
                if part == gibs::Part::Lamp {
                    sparks.push(at);
                }
                let seed = (n as u32 + 1).wrapping_mul(0x2545_f491) ^ c.missing.count_ones().wrapping_mul(0x9e37_79b9) ^ (c.bot as u32);
                // Thrown along the shot and out of the body, as a blast
                // throws a chunk out of a chest.
                let from = at - along * 0.3;
                thrown.extend(gibs::burst(&[(part, at, rot)], from, along * CHIP_FLING, seed).into_iter().map(|g| (c.age, c.left, g)));
            }
        }
        for at in sparks {
            self.combat.sparks(at);
        }
        for (age, left, g) in thrown {
            self.gibs.push((age, left, vec![g]));
        }
    }

    fn say(&mut self, line: String) {
        self.feed.push((line, FEED_S));
        while self.feed.len() > 4 {
            self.feed.remove(0);
        }
    }

    /// Fighter `who` is down, killed by `by`. `fling` is what the killing
    /// blow leaves in the body (`physics::tumble`); `blast` says a burst did
    /// it, and a bot a burst kills comes apart (gibs.rs).
    fn down(&mut self, who: usize, by: usize, player: &Player, fling: Vec3, blast: bool) {
        let at = if who == 0 { player.pos } else { self.bots.get(who - 1).map_or(Vec3::ZERO, |b| b.pos()) };
        if who > 0 {
            self.break_lamps(who - 1, fling, blast);
        }
        let respawn = self.rules.respawn_s;
        if who == 0 {
            self.me.die(at, respawn, fling);
            self.combat.clear();
            // No note: the HUD already puts YOU DIED across the middle of
            // the screen, and a second copy in the corner message is one
            // too many ways of saying it.
        } else if let Some(b) = self.bots.get_mut(who - 1) {
            b.fighter.die(at, respawn, fling);
        }
        // A fighter who walked into their own rocket does not get the frag.
        if by != who {
            if by == 0 {
                self.me.score.kills += 1;
            } else if let Some(k) = self.bots.get_mut(by - 1) {
                k.fighter.score.kills += 1;
            }
            let line = format!("{} KILLED {}", self.name_of(by), self.name_of(who));
            self.say(line);
            if by == 0 {
                self.killed_one(who);
            }
        } else {
            let line = format!("{} MANAGED IT ALONE", self.name_of(who));
            self.say(line);
        }
        // The match may have just been won (round.rs).
        let board = self.scores();
        self.round.after_kill(who, by, &board);
    }

    /// Put the match back to nothing and stand everyone at a spawn again.
    fn restart(&mut self, r: round::Reset, player: &mut Player, store: &Store) {
        if r.scores {
            self.me.score = Score::default();
            for b in &mut self.bots {
                b.fighter.score = Score::default();
            }
        }
        self.feed.clear();
        self.combat.clear();
        for who in 0..=self.bots.len() {
            self.come_back(who, player, store);
        }
        self.clean_world = true;
        self.note = Some("A NEW MATCH".into());
    }

    /// Put a body back at the next spawn point.
    fn come_back(&mut self, who: usize, player: &mut Player, store: &Store) {
        self.put_at_spawn(who, player, store, true);
    }

    /// Put a fighter on a fresh spawn. `heal` says whether this is a
    /// respawn or only a move.
    ///
    /// The two are not the same thing and used to be. A bot is placed once
    /// more when the nav graph finally settles, because until the world has
    /// streamed in there is no floor to stand it on -- and that placement
    /// was handing every bot a full heal at whatever point the fight had
    /// reached. On a small arena the graph settles before anyone has been
    /// shot and nothing shows; on a big one it lands ten or twenty seconds
    /// in, and a bot you have worked down to a quarter is suddenly whole
    /// again with its death count untouched.
    fn put_at_spawn(&mut self, who: usize, player: &mut Player, store: &Store, heal: bool) {
        // Away from whoever is still up, so a body does not come back in
        // the lap of the one that just shot it.
        let mut away = Vec::with_capacity(self.bots.len() + 1);
        if who != 0 && self.me.alive() {
            away.push(player.pos);
        }
        away.extend(self.bots.iter().enumerate().filter(|(i, b)| i + 1 != who && b.fighter.alive()).map(|(_, b)| b.pos()));
        let s = self.next_spawn(&away);
        let pos = self.floor_at(store, s.at[0], s.at[1], s.ceiling, self.home.0.y).0;
        let yaw = -s.bearing.to_radians();
        if who == 0 {
            stand(player, pos, yaw);
            player.pitch = 0.0;
            self.me_to_the_field = 0;
            if heal {
                self.me.revive(self.rules.give_all);
            }
            self.climb = 0.0;
        } else if let Some(b) = self.bots.get_mut(who - 1) {
            b.body.pos = pos;
            b.body.prev_pos = pos;
            b.body.body.vel = Vec3::ZERO;
            b.body.yaw = yaw;
            b.body.pitch = 0.0;
            b.pilot = Pilot { yaw, ..Default::default() };
            if heal {
                b.fighter.revive(self.rules.give_all);
                b.figure.revive();
            }
        }
    }

    /// Somebody fired at `at`. Bots within `SHOT_REACH_M` may walk toward
    /// it (`brain::duel`), which is the cheapest thing that makes two bots
    /// on a map find each other, and is what a player does.
    ///
    /// Only one entry per shooter at a time: a minigun would otherwise put
    /// six hundred noises a minute in here and every one of them says the
    /// same thing.
    fn heard(&mut self, at: Vec3, by: usize) {
        let when = self.now;
        if let Some(n) = self.noises.iter_mut().find(|n| n.by == by) {
            *n = brain::Noise { at, when, reach: SHOT_REACH_M, by };
        } else {
            self.noises.push(brain::Noise { at, when, reach: SHOT_REACH_M, by });
        }
    }

    /// Let every brain think, once per step. Thinking reads only shared
    /// state, so this cannot move anything.
    fn think(&mut self, store: &Store, foes: &[Foe], dt: f32) {
        let now = self.now;
        let nav = &self.nav;
        let pads = &self.pickups;
        let lures = &mut self.lures;
        self.noises.retain(|n| now - n.when < EARSHOT_S);
        let noises = &self.noises;
        for (i, b) in self.bots.iter_mut().enumerate() {
            if !b.fighter.alive() {
                continue;
            }
            // What is on the floor, scored for this bot: the pads answer the
            // question, so the brain never learns what any of them is.
            pads.lures(&b.fighter, lures);
            let cx = brain::Ctx {
                store,
                nav,
                now,
                fighters: foes,
                lures,
                noises,
            };
            let me = Me {
                who: i + 1,
                foot: b.body.pos,
                eye: b.body.eye(1.0),
                vel: b.body.body.vel,
                yaw: b.body.yaw,
                pitch: b.body.pitch,
                on_ground: b.body.body.on_ground,
                half_w: b.body.body.half_w,
                height: b.body.body.height,
                health: b.fighter.health,
                full: b.fighter.full_health,
                struck: b.fighter.struck.take(),
                guns: &b.fighter.arsenal,
                person: &b.person,
            };
            b.brain.think(&me, &cx, &mut b.pilot, dt);
            if let Some(slot) = b.pilot.want_slot.take() {
                b.fighter.arsenal.pick(slot);
            }
        }
    }

    /// Move the bots' bodies, on the same fixed step the player's body runs
    /// on and through the same function. This is the claim the whole bot
    /// design rests on: a bot can do what the player can do and no more,
    /// because there is only one piece of movement code.
    fn step_bodies(&mut self, store: &Store, sea: f32, land: Option<&crate::world::fields::World>, slopes: bool) {
        for b in &mut self.bots {
            if !b.fighter.alive() {
                let fall = (b.name.len() as u32).wrapping_mul(2654435761) ^ b.fighter.score.deaths.wrapping_mul(0x9e3779b9) | 1;
                if !b.figure.dead() {
                    // The step it goes down on. Nothing is holding it up
                    // any more, and what killed it is still in it: a
                    // shot tips it over where it stood, a rocket throws
                    // it off the gantry (`physics::tumble`).
                    b.body.pos = b.fighter.fell;
                    b.body.prev_pos = b.fighter.fell;
                    b.figure.die(&self.avatar.rig, self.avatar.voxel, fall);
                    let thrown = b.fighter.fling.length();
                    if thrown > FLUNG {
                        b.body.body.vel = b.fighter.fling;
                        b.body.body.on_ground = false;
                        b.figure.hurl(&self.avatar.rig, thrown, fall);
                    } else {
                        // Shot off its feet by nothing much: it drops
                        // where it stood.
                        b.body.body.vel = Vec3::ZERO;
                    }
                }
                b.body.prev_pos = b.body.pos;
                crate::physics::tumble(store, &mut b.body.pos, &mut b.body.body, STEP);
                // What the limbs are told: where the body is going, and
                // whether it is still in the air.
                let d = figure::Drive {
                    dt: STEP,
                    vel: b.body.body.vel,
                    yaw: b.body.yaw,
                    on_ground: b.body.body.on_ground,
                    ..Default::default()
                };
                b.figure.step(&self.avatar.rig, self.avatar.voxel, &d);
                continue;
            }
            b.body.yaw = b.pilot.yaw;
            b.body.pitch = b.pilot.pitch;
            // Water is not being buried in rock: a bot swims in a
            // pool rather than standing on it (physics::hits_wet).
            let inside = crate::physics::hits_wet(store, b.body.pos, b.body.body.half_w, b.body.body.height);
            if inside && !b.was_buried {
                b.buried += 1;
                b.buried_at = b.body.pos;
            }
            b.was_buried = inside;
            let water = crate::game::water_at(land, sea, store, b.body.pos);
            // A dodge the brain asked for is tapped out here, on the
            // body's own step: the keys that reach `walk` are the keys
            // a player's hand would be holding, and the body reads a
            // bot's double-tap by the rule it reads the player's.
            let keys = b.tap.step(b.pilot.dodge.take(), b.pilot.mv);
            b.body.walk(&keys, store, water, slopes, STEP);
            // Drawing only, and after the step, so the pose is of the
            // body as it now is. `Figure` never writes back.
            let d = figure::Drive {
                dt: STEP,
                vel: b.body.body.vel,
                yaw: b.body.yaw,
                pitch: b.body.pitch,
                on_ground: b.body.body.on_ground,
                crouch: b.body.crouch,
                sprint: b.pilot.mv.sprint,
                holding: b.fighter.arsenal.current().is_some(),
            };
            b.figure.step(&self.avatar.rig, self.avatar.voxel, &d);
        }
    }

    /// Every bot pulls its trigger.
    fn bots_fire(&mut self, store: &Store, ents: &mut crate::entities::Entities, models: &[Model], boxes: &[Hitbox], dt: f32) {
        for i in 0..self.bots.len() {
            if !self.bots[i].fighter.alive() {
                continue;
            }
            let trigger = self.bots[i].pilot.trigger;
            let shot = self.bots[i].fighter.arsenal.current_mut().map_or(Default::default(), |g| g.pull(trigger, dt));
            let mode = self.bots[i].fighter.arsenal.current().and_then(|g| g.fired_mode(&shot, trigger).cloned());
            // The spade, which is not a shot: a bot holding the Digger cuts
            // the wall in front of it, which is how one at the bottom of a
            // borer hole gets out of it.
            if shot.dig {
                let spade = self.bots[i].fighter.arsenal.current().and_then(|g| g.def.dig);
                if let Some(d) = spade {
                    let (eye, aim) = (self.bots[i].body.eye(1.0), self.bots[i].body.forward());
                    self.combat.bite(store, eye, aim, d.radius, d.range);
                }
            }
            let (Some(f), true) = (mode, shot.shots > 0 || shot.beam) else { continue };
            let eye = self.bots[i].body.eye(1.0);
            let shape = self.bots[i].fighter.arsenal.current().map_or(0, |g| viewmodel::shape(&g.def.view));
            let muzzle = self.bots[i].muzzle(&self.avatar, shape);
            self.bots[i].figure.fired(f.kick * shot.shots as f32);
            let aim = self.bots[i].body.forward();
            {
                let mut scene = Scene { store, ents, models, fighters: boxes };
                if shot.beam {
                    self.combat.hold_beam(&mut scene, eye, muzzle, aim, &f, i + 1, dt);
                }
                for _ in 0..shot.shots {
                    self.combat.fire(&mut scene, eye, muzzle, aim, &f, i + 1, shot.strength);
                }
            }
            self.bots[i].fighter.score.shots += shot.shots;
            self.heard(eye, i + 1);
        }
    }

    /// Hand out the damage and the pushes this step produced.
    /// What the ground does to whoever is standing on it: molten slag, lava,
    /// anything with `hurt` in its material file. It is a property of the
    /// stuff, so the arena's own pit bites exactly as an imported map's lava
    /// does, and no world carries a list of hazards.
    ///
    /// A death here belongs to whoever last hurt or shoved the fighter, if
    /// that was within `brain::BLAME_S`: rocketed into the slag is a kill.
    /// Otherwise it is credited to nobody, through `down(who, who, ..)`, the
    /// path for a fighter who managed it alone.
    fn scald(&mut self, store: &Store, player: &mut Player, dt: f32) {
        self.me.forget_blame(dt);
        for b in &mut self.bots {
            b.fighter.forget_blame(dt);
        }
        let mut fallen: Vec<usize> = Vec::new();
        if self.me.alive() && !self.bench {
            let dps = hurt_at(store, player.pos, player.body.half_w);
            if dps > 0.0 && self.me.take(dps * dt) {
                fallen.push(0);
            }
        }
        for (i, b) in self.bots.iter_mut().enumerate() {
            if !b.fighter.alive() {
                continue;
            }
            let dps = hurt_at(store, b.body.pos, b.body.body.half_w);
            if dps > 0.0 && b.fighter.take(dps * dt) {
                fallen.push(i + 1);
            }
        }
        for who in fallen {
            let f = if who == 0 { Some(&self.me) } else { self.bots.get(who - 1).map(|b| &b.fighter) };
            let by = f.map_or(who, |f| f.ground_killer(who));
            self.down(who, by, player, Vec3::ZERO, false);
        }
    }

    fn settle(&mut self, player: &mut Player) {
        let hits = self.combat.take_hits();
        for h in hits {
            if h.push.length_squared() > 0.0 {
                if h.who == 0 {
                    player.body.vel += h.push;
                    player.body.on_ground = false;
                } else if let Some(b) = self.bots.get_mut(h.who - 1) {
                    b.body.body.vel += h.push;
                    b.body.body.on_ground = false;
                }
            }
            if h.by == 0 && h.who != 0 && h.damage > 0.0 {
                // A bigger hit is a bigger marker, and one landing on top of
                // another keeps the bigger of the two.
                let size = (h.damage / 40.0).clamp(0.25, 1.0);
                let fresh = self.mark <= 0.0;
                self.mark_size = if fresh { size } else { self.mark_size.max(size) };
                self.mark_head = h.head || (!fresh && self.mark_head);
                self.mark = MARK_S + 0.15 * size;
            }
            // What landed on somebody else, credited to whoever fired it.
            // This is the one number that says whether the aim model is
            // doing what it is meant to: a stationary player cannot show it,
            // because a target that is not crossing the view is the case the
            // crossing term is defined not to touch (aim.rs).
            if self.bench && (h.who == 0 || h.by == 0) {
                continue;
            }
            if h.by != h.who && h.damage > 0.0 {
                let credit = if h.by == 0 {
                    Some(&mut self.me.score)
                } else {
                    self.bots.get_mut(h.by - 1).map(|b| &mut b.fighter.score)
                };
                if let Some(sc) = credit {
                    sc.hits += 1;
                    sc.dealt += h.damage;
                }
            }
            // Anybody else's blow, even one that only pushes: the ground may
            // finish what it started (`scald`).
            if h.by != h.who && (h.damage > 0.0 || h.push.length_squared() > 0.0) {
                let f = if h.who == 0 { Some(&mut self.me) } else { self.bots.get_mut(h.who - 1).map(|b| &mut b.fighter) };
                if let Some(f) = f {
                    f.blame = Some((h.by, crate::brain::BLAME_S));
                }
            }
            let killed = if h.who == 0 {
                self.me.take(h.damage)
            } else {
                self.bots.get_mut(h.who - 1).is_some_and(|b| {
                    // Who it was, for the brain. Not its own blast, or a
                    // rocket jump would read as somebody shooting at it.
                    if h.by != h.who {
                        b.fighter.struck = Some(h.by);
                    }
                    b.fighter.take(h.damage)
                })
            };
            if killed {
                self.down(h.who, h.by, player, h.fling, h.blast);
            }
        }
    }
}

/// A dead bot's lamp `t` seconds after it went down: a white flare, then a
/// flickering fade to the dark of `lamp_tint(_, false)` over most of a
/// second. The flare is what says "that one is dead" from across the hall.
fn lamp_dying(t: f32) -> [f32; 3] {
    let dark = avatar::lamp_tint(0.0, false);
    if t < 0.08 {
        return [3.0, 2.8, 2.5];
    }
    let k = ((t - 0.08) / 0.8).clamp(0.0, 1.0);
    let flicker = if k < 1.0 { 0.75 + 0.25 * (t * 47.0).sin() } else { 1.0 };
    let hot = [2.2, 1.0, 0.3];
    [0, 1, 2].map(|c| dark[c] + (hot[c] - dark[c]) * (1.0 - k) * (1.0 - k) * flicker)
}

/// The weapons a world hands out, complaining about names it does not know.
fn arsenal(lib: &Library, rules: &ShooterDef) -> Arsenal {
    let mut bad = Vec::new();
    let a = Arsenal::new(lib, &rules.weapons, rules.give_all, |n| bad.push(n.to_string()));
    for n in &bad {
        eprintln!("world.json shooter: no weapon called {n}");
    }
    a
}

impl Activity for Shooter {
    fn id(&self) -> ModeId {
        ModeId::Shooter
    }

    /// The keys, once a frame: the trigger is held and kept for the steps,
    /// and a press (a weapon, a reload, another match) is taken here so it
    /// lands once however many steps the frame holds.
    fn frame(&mut self, _cx: &mut ActivityCtx, _input: &MoveInput, actions: &mut Actions, _dt: f32) {
        // The mouse buttons are the trigger here, so nothing digs the arena
        // out from under itself.
        self.held = (std::mem::take(&mut actions.dig), std::mem::take(&mut actions.place));
        actions.tilt_left = false;
        self.again |= std::mem::take(&mut actions.accept);
        if !self.me.alive() {
            actions.select = None;
            actions.brush_delta = 0;
            actions.reload = false;
            return;
        }
        if let Some(i) = actions.select.take() {
            // Nine digits and then 0, which is the tenth slot the way
            // every arena shooter has had it.
            let slot = i as u32 + 1;
            if self.me.arsenal.pick(slot) {
                if let Some(g) = self.me.arsenal.current() {
                    self.note = Some(g.def.display.to_uppercase());
                }
            }
        }
        // Topping up a magazine that is merely low. A gun that runs dry
        // reloads itself (combat/weapon.rs); this is the key for doing it
        // before you have to.
        if std::mem::take(&mut actions.reload) && self.me.arsenal.reload() {
            self.note = Some("RELOADING".into());
        }
        if actions.brush_delta != 0 {
            self.me.arsenal.cycle(-actions.brush_delta);
            actions.brush_delta = 0;
            if let Some(g) = self.me.arsenal.current() {
                self.note = Some(g.def.display.to_uppercase());
            }
        }
    }

    fn step(&mut self, cx: ActivityCtx, input: &MoveInput) {
        let dt = STEP;
        // Taken apart, because moving a second body needs the world while
        // the player is already borrowed.
        let ActivityCtx {
            player,
            store,
            ents,
            models,
            sea,
            land,
            slopes,
            screenshot_request: _,
            edits,
        } = cx;
        self.now += dt;
        let (fire, alt) = self.held;
        let kills_before = self.combat.kills;

        // --- the bots: think, move, shoot ---------------------------------
        self.settle_nav(store, player);
        self.settle_pads(store, player);
        self.stand_me(player, store);
        if std::mem::take(&mut self.bots_to_the_field) {
            // Standing them on ground that has finally arrived. Not a
            // respawn: nobody gets their health back for this.
            for i in 1..=self.bots.len() {
                self.put_at_spawn(i, player, store, false);
            }
        }
        let foes = self.foes(player);
        self.think(store, &foes, dt);
        self.step_bodies(store, sea, land, slopes);
        self.unstack(store, player);
        self.collect(player, dt);
        let boxes = self.hitboxes(player);
        self.bots_fire(store, ents, models, &boxes, dt);
        for b in &mut self.bots {
            b.fighter.fade(dt);
            if !b.fighter.alive() {
                b.fighter.respawn -= dt;
            }
        }
        for i in 1..=self.bots.len() {
            if self.bots[i - 1].fighter.respawn <= 0.0 && self.bots[i - 1].fighter.health <= 0.0 {
                self.come_back(i, player, store);
            }
        }

        // --- the player ---------------------------------------------------
        if !self.me.alive() {
            // The body lies where it fell until the clock runs out.
            player.pos = self.me.fell;
            player.prev_pos = self.me.fell;
            player.body.vel = Vec3::ZERO;
            self.me.respawn -= dt;
            if self.me.respawn <= 0.0 {
                self.come_back(0, player, store);
            }
        } else {
            let trigger = match (fire, alt) {
                (true, _) => Trigger::Fire,
                (_, true) => Trigger::Alt,
                _ => Trigger::Off,
            };
            let eye = player.eye(1.0);
            let aim = player.forward();
            // The sight is held, not fired, and holding it is not pressing a
            // trigger: it is read off the right button itself rather than
            // off `trigger`, which the left button wins. Reading it off
            // `trigger` meant the view snapped back to full width for every
            // frame the trigger was down, so a sniper could line a shot up
            // down the sight and never take one down it.
            self.sighted = alt && self.me.arsenal.current().is_some_and(|g| g.def.zoom > 0.0);
            let shot = self.me.arsenal.current_mut().map_or(Default::default(), |g| g.pull(trigger, dt));
            let mode = self.me.arsenal.current().and_then(|g| g.fired_mode(&shot, trigger).cloned());
            // The spade on the end of the Digger, which bites on its own
            // clock rather than the blast's (`library::DigDef`). It is the
            // one thing in a match that cuts the arena on purpose.
            if let (true, Some(d)) = (shot.dig, self.me.arsenal.current().and_then(|g| g.def.dig)) {
                self.combat.bite(store, eye, aim, d.radius, d.range);
            }
            if let (Some(f), true) = (mode, shot.shots > 0 || shot.beam) {
                let shape = self.me.arsenal.current().map_or(0, |g| viewmodel::shape(&g.def.view));
                let muzzle = hold::muzzle(&self.hold, player, 1.0, shape);
                {
                    let mut scene = Scene {
                        store,
                        ents,
                        models,
                        fighters: &boxes,
                    };
                    if shot.beam {
                        self.combat.hold_beam(&mut scene, eye, muzzle, aim, &f, 0, dt);
                    }
                    for _ in 0..shot.shots {
                        self.combat.fire(&mut scene, eye, muzzle, aim, &f, 0, shot.strength);
                    }
                }
                self.me.score.shots += shot.shots;
                if shot.shots > 0 || shot.beam {
                    self.heard(eye, 0);
                }
                if shot.shots > 0 {
                    self.hold.fired(f.kick * shot.strength, shot.shots);
                    let climb = f.kick * shot.strength * shot.shots as f32;
                    self.climb = (self.climb + climb).min(CLIMB_MAX);
                    player.pitch = (player.pitch + climb.to_radians()).min(1.5);
                }
            }
            // The view settles back down.
            if self.climb > 0.0 {
                let back = (RECOVER * dt).min(self.climb);
                self.climb -= back;
                player.pitch -= back.to_radians();
            }
        }

        // --- everything in the air ----------------------------------------
        {
            let mut scene = Scene {
                store,
                ents,
                models,
                fighters: &boxes,
            };
            self.combat.step(&mut scene, dt);
        }
        // Whatever a bore ate this tick goes to `Game`, which owns the
        // store and the streamer that has to rebuild what changed.
        edits.append(&mut self.combat.take_edits());
        self.settle(player);
        self.chip();
        self.scald(cx.store, player, dt);
        self.me.fade(dt);
        if self.combat.kills > kills_before {
            self.mark = MARK_S;
        }
        self.mark = (self.mark - dt).max(0.0);
        self.mark_kill = (self.mark_kill - dt).max(0.0);
        self.streak.1 += dt;
        if let Some((_, _, age)) = &mut self.banner {
            *age += dt;
            if *age > BANNER_S {
                self.banner = None;
            }
        }
        // The dead and the pieces of them, until each is cleared away.
        self.gibs.retain(|g| g.1 > 0.0);
        for (age, left, g) in &mut self.gibs {
            *age += dt;
            *left -= dt;
            gibs::step(g, cx.store, dt);
        }
        self.corpses.retain(|c| c.left > 0.0);
        for c in &mut self.corpses {
            c.step(cx.store, dt);
        }
        self.feed.retain_mut(|(_, t)| {
            *t -= dt;
            *t > 0.0
        });
        let spin = self.me.arsenal.current().map_or(0.0, |g| g.spin);
        self.hold.step(player, input.sprint, spin, dt);
        self.boost = player.boost_meter();
        // The match itself: is it won, and is another one wanted?
        let board = self.scores();
        let mut keys = Actions {
            accept: std::mem::take(&mut self.again),
            ..Default::default()
        };
        if let Some(r) = self.round.step(dt, &board, &mut keys) {
            self.restart(r, player, store);
        }
    }

    /// The world holds still behind the final scoreboard, so the bots stop
    /// fighting over a match that is already decided.
    fn wants_world_clean(&mut self) -> bool {
        std::mem::take(&mut self.clean_world)
    }

    fn pauses_world(&self) -> bool {
        self.round.frozen()
    }

    fn note(&mut self) -> Option<String> {
        self.note.take()
    }

    fn shooter_mut(&mut self) -> Option<&mut Shooter> {
        Some(self)
    }

    fn shooter(&self) -> Option<&Shooter> {
        Some(self)
    }

    /// Down the sniper's sight the view narrows to the gun's own `zoom`.
    fn zoom(&self) -> f32 {
        match self.sighted {
            true => self.me.arsenal.current().map_or(1.0, |g| g.def.zoom.clamp(0.05, 1.0)),
            false => 1.0,
        }
    }

    fn bodies(&mut self, each: &mut dyn FnMut(&mut crate::player::Player)) {
        for b in &mut self.bots {
            each(&mut b.body);
        }
    }

    /// Everything solid this activity draws: the shells in the air, every
    /// bot's shell and gun, and the gun in the player's own hands.
    fn instances(&self, player: &Player, alpha: f32, base: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        let shells = base + viewmodel::COUNT;
        // Everything in the air, whoever fired it. A shell is laid nose
        // toward model -z like a gun is, so the yaw that points it along its
        // flight is the yaw that would point a gun there, and the pitch
        // rides in `rot` with the tumble rolled in first.
        let eye = player.eye(alpha);
        for s in &self.combat.shells {
            let Some(md) = models.get(base + s.look) else { continue };
            // A shell in the lens fills the screen; one leaving the muzzle
            // is about a third of a metre away, so this only cuts the frame
            // it is born in.
            if (s.pos - eye).length() < 0.22 {
                continue;
            }
            let d = s.vel.normalize_or(Vec3::NEG_Z);
            let yaw = (-d.x).atan2(-d.z);
            let rot = Quat::from_rotation_x(d.y.asin()) * Quat::from_rotation_z(s.spin);
            out.push((
                base + s.look,
                Instance {
                    pos_yaw: [s.pos.x, s.pos.y, s.pos.z, yaw],
                    info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                    tint: [1.0, 1.0, 1.0, 0.0],
                    rot: [rot.x, rot.y, rot.z, rot.w],
                    shift: [0.0; 4],
                },
            ));
        }
        // Every bot, living and lately dead: a body that vanished the
        // instant it was killed would take the one piece of feedback a kill
        // gives you with it. Drawing a corpse is not the same as being able
        // to shoot one -- `hitboxes` still says only the living are there.
        let mut fk = Vec::new();
        let av = &self.avatar;
        for b in &self.bots {
            let alive = b.fighter.alive();
            if !alive && b.fighter.respawn <= 0.0 {
                continue;
            }
            // The dead are drawn as their corpses and chunks, below.
            if !alive {
                continue;
            }
            let lit = avatar::lamp_tint(b.whole(), alive);
            let at = b.body.prev_pos.lerp(b.body.pos, alpha);
            let pose = b.figure.at(alpha);
            // A corpse goes grey all over and stops flashing: the paint is
            // the side it was on, and it is not on one any more. The flash
            // has to go with it or a bot killed by the shot that was already
            // in the air strobes white while it falls.
            let tint = [b.tint[0], b.tint[1], b.tint[2], b.fighter.hurt / brain::HURT_S];
            crate::anim::push_parts_at(&av.rig, shells, &pose, models, av.voxel, 1.0, 1.0, av.pivot, at, b.body.yaw, tint, [0.0; 3], &mut fk, out);
            // Its lamps, front and back: how much of it is left.
            for lamp in av.lamps(&b.figure, at, b.body.yaw, alpha) {
                out.extend(hold::placed(shells + avatar::LAMP_M, models, lamp, b.body.yaw, Quat::IDENTITY, lit));
            }
            let (gat, gyaw, grot) = b.grip(av, alpha);
            if let Some(g) = b.fighter.arsenal.current() {
                out.extend(hold::placed(base + viewmodel::shape(&g.def.view), models, gat, gyaw, grot, [1.0; 3]));
            }
        }
        // The dead: grey all over, their lamps flaring and guttering out.
        for c in &self.corpses {
            let lit = lamp_dying(c.age);
            for (k, p, q) in c.parts(alpha) {
                out.extend(hold::placed(shells + av.rig.bones[k].model, models, p, 0.0, q, avatar::DEAD_TINT));
            }
            for (_, l) in c.lamps(alpha) {
                out.extend(hold::placed(shells + avatar::LAMP_M, models, l, 0.0, Quat::IDENTITY, lit));
            }
        }
        for (age, _, chunks) in &self.gibs {
            let lit = lamp_dying(*age);
            for g in chunks {
                let p = g.prev.lerp(g.pos, alpha);
                let q = g.prev_rot.slerp(g.rot, alpha);
                let (model, tint) = match g.part {
                    gibs::Part::Bone(k) => (shells + av.rig.bones[k].model, avatar::DEAD_TINT),
                    gibs::Part::Lamp => (shells + avatar::LAMP_M, lit),
                };
                out.extend(hold::placed(model, models, p, 0.0, q, tint));
            }
        }
        // Whatever is lying about: a lit blob turning on the spot, with a
        // weapon pad's gun turning above it. The blob is drawn in the lamp's
        // emissive slot, so it reads at the end of an unlit corridor, which
        // is where a health pack most needs to be seen from.
        for p in &self.pickups.pads {
            if !p.ready() {
                continue;
            }
            let spin = self.now * 1.5;
            let bob = (self.now * 2.0 + p.at.x + p.at.z).sin() * 0.06;
            let size = p.size().map_or(1.0, pickup::Size::scale);
            let gun_pad = p.kind == pickup::Kind::Weapon;
            // A weapon pad's blob is a glow the gun turns over, not a blob
            // with a gun under it. The model's pivot is the bottom of its
            // grid and the blob sits two thirds of the way up it, so a pad
            // placed at standing height puts the blob over everything drawn
            // with it -- which on a weapon pad is a white lump hanging above
            // the gun. Dropped to the deck and drawn small, it is the pool
            // of light the comment on `Kind::tint` always claimed it was.
            let at = p.at + Vec3::Y * (if gun_pad { 0.0 } else { avatar::PAD_RISE } + bob);
            let blob = if gun_pad { 0.45 } else { size };
            out.extend(hold::sized(shells + avatar::PAD_M, models, at, spin, Quat::IDENTITY, p.kind.tint(), blob));
            // A health pad also carries a cross, the size of what it gives.
            // Nothing else on the floor says what it is without a word on
            // it: a weapon pad has its own gun drawn over it, and a health
            // pack had a blob the same shape as the armour next to it.
            if p.size().is_some() {
                let up = at + Vec3::Y * (0.21 + 0.17 * size);
                out.extend(hold::sized(shells + avatar::CROSS_M, models, up, spin, Quat::IDENTITY, p.kind.tint(), size * 0.55));
            }
            // The gun itself, so you can tell a rocket launcher from a flak
            // hopper without reading anything.
            if gun_pad {
                if let Some(name) = p.weapon.as_deref() {
                    if let Some(g) = self.me.arsenal.guns.iter().find(|g| g.def.name == name) {
                        let shape = viewmodel::shape(&g.def.view);
                        let up = at + Vec3::Y * (avatar::PAD_RISE + 0.16);
                        out.extend(hold::placed(base + shape, models, up, spin, Quat::from_rotation_x(0.35), [1.0; 3]));
                    }
                }
            }
        }
        if !self.alive() {
            return;
        }
        let Some(g) = self.me.arsenal.current() else { return };
        let shape = viewmodel::shape(&g.def.view);
        let grip = hold::grip(&self.hold, player, alpha);
        out.extend(grip.instance(base + shape, models, grip.pos, Quat::IDENTITY));
        if shape == viewmodel::MINIGUN {
            // The barrels turn about their own axis, which is where their
            // model's pivot is.
            let at = grip.at(Vec3::new(16.0, 13.0, 16.0));
            out.extend(grip.instance(base + viewmodel::BARRELS, models, at, Quat::from_rotation_z(self.hold.spin)));
        }
        if self.hold.flash > 0.0 {
            let at = grip.at(viewmodel::muzzle(shape) - Vec3::Z * 4.0);
            out.extend(grip.instance(base + viewmodel::FLASH, models, at, Quat::IDENTITY));
        }
    }

    fn overlay(&self, player: &Player, out: &mut crate::render::overlay::Overlay) {
        self.combat.draw(player.eye(1.0), out);
        self.beacons(player, out);
    }

    fn hud(&self, h: &mut Hud, w: f32, ht: f32, s: f32, _fov: f32) {
        hud::draw(self, h, w, ht, s);
        // Over everything, and only once the match is over.
        self.round.draw(h, w, ht, s, &self.scores());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::{BotDef, NavDef, NavLink, NavNode};
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    /// A flat slab to fight on.
    ///
    /// The chunk range has to reach past the spawn points, not just past the
    /// voxels the loop below writes: `Store::set` outside an inserted chunk
    /// is dropped, and everything outside the store reads as solid, so a
    /// body placed there cannot move in any direction. That is what it looks
    /// like when a bot presses forward on open ground for thirty seconds
    /// with its velocity pinned at exactly zero, which is what this fixture
    /// did to a bot spawning at x = -14 while its floor stopped at -8.
    fn floor() -> Store {
        let mut s = Store::default();
        for cz in -3..=3 {
            for cy in 0..=1 {
                for cx in -3..=3 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -60..60 {
            for z in -60..60 {
                for y in 0..8 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    fn rules() -> ShooterDef {
        ShooterDef {
            weapons: vec!["rocket_launcher".into()],
            spoil: None,
            health: 100.0,
            armour: 0.0,
            armour_max: None,
            health_max: None,
            respawn_s: 1.0,
            spawns: vec![SpawnPoint {
                at: [2.0, 3.0],
                bearing: 90.0,
                ceiling: None,
            }],
            give_all: true,
            bots: Vec::new(),
            nav: NavDef::default(),
            pickups: Vec::new(),
            // No target: these tests fight a match that never ends.
            target_score: 0,
        }
    }

    /// Two bots on a flat floor with a line of nav nodes to walk.
    fn with_bots() -> ShooterDef {
        let mut r = rules();
        r.weapons = vec!["minigun".into(), "rocket_launcher".into(), "scrap_gun".into()];
        r.respawn_s = 3.0;
        r.spawns = (0..4)
            .map(|i| SpawnPoint {
                at: [i as f32 * 9.0 - 14.0, 3.0],
                bearing: 90.0,
                ceiling: None,
            })
            .collect();
        r.bots = ["rig", "slag"]
            .iter()
            .map(|n| BotDef {
                name: (*n).into(),
                skill: "easy".into(),
                colour: None,
                bold: 0.0,
                steady: 0.0,
                without: Vec::new(),
            })
            .collect();
        r.nav = NavDef {
            nodes: (0..5)
                .map(|i| NavNode {
                    at: [i as f32 * 8.0 - 16.0, 0.0],
                    up: 0.0,
                    cover: 0.0,
                    poi: None,
                })
                .collect(),
            links: (0..4)
                .map(|i| NavLink {
                    a: i,
                    b: i + 1,
                    reach: "walk".into(),
                    cost: 0.0,
                    platform: None,
                    portal: None,
                })
                .collect(),
        };
        r
    }

    fn run(sh: &mut Shooter, player: &mut Player, store: &Store, steps: usize, mut each: impl FnMut(usize, &mut Actions)) {
        let input = MoveInput::default();
        let mut actions = Actions::default();
        let mut ents = crate::entities::Entities::new(Default::default(), 1);
        for i in 0..steps {
            actions.dig = false;
            each(i, &mut actions);
            let cx = ActivityCtx {
                player,
                store,
                ents: &mut ents,
                models: &[],
                sea: 0.0,
                land: None,
                slopes: false,
                screenshot_request: &mut false,
                edits: &mut Vec::new(),
            };
            let mut cx = cx;
            sh.frame(&mut cx, &input, &mut actions, 1.0 / 60.0);
            sh.step(cx, &input);
        }
    }

    /// A bot dodges, and only when its style allows it. That it dodges by
    /// tapping the keys rather than by being handed a move the player has
    /// not got is `player::tests::a_tapper_dodges_the_body_the_way_a_player_does`;
    /// this is the half that says a brain ever asks.
    #[test]
    fn a_bot_breaks_the_circle_with_a_dodge() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let play = |without: &[&str]| {
            let mut def = with_bots();
            // Keen enough to use it: `tactics` is what says how willing a
            // bot is to strafe, dodge and jump, and an easy one is 0.2.
            for b in &mut def.bots {
                b.skill = "godlike".into();
                b.without = without.iter().map(|s| (*s).to_string()).collect();
            }
            let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 23);
            let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
            sh.bench_player();
            run(&mut sh, &mut player, &store, 2400, |_, _| {});
            sh.bots.iter().map(|b| b.brain.dodges()).sum::<u32>()
        };
        assert_eq!(play(&["dodge"]), 0, "it dodged with the switch off");
        assert!(play(&[]) > 0, "two godlike bots fought for eighty seconds and never dodged");
    }

    /// Squeezing the Digger at the floor cuts a hole in it, and the voxels
    /// reach `Game` through the activity's edit queue. This is the whole of
    /// how a body gets out of a borer hole, so it is worth a test that would
    /// notice the queue being dropped on the floor.
    #[test]
    fn the_digger_cuts_the_floor_and_hands_the_voxels_up() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let mut def = rules();
        def.weapons = vec!["digger".into()];
        def.bots.clear();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 11);
        let mut player = Player::new(Vec3::new(0.0, 2.2, 0.0), 0.0);
        // Straight down at the floor, and the left button held.
        player.pitch = -1.5;
        let mut out: Vec<([i32; 3], u8)> = Vec::new();
        let input = MoveInput::default();
        let mut actions = Actions::default();
        let mut ents = crate::entities::Entities::new(Default::default(), 1);
        for _ in 0..30 {
            actions.dig = true;
            let cx = ActivityCtx {
                player: &mut player,
                store: &store,
                ents: &mut ents,
                models: &[],
                sea: 0.0,
                land: None,
                slopes: false,
                screenshot_request: &mut false,
                edits: &mut out,
            };
            let mut cx = cx;
            sh.frame(&mut cx, &input, &mut actions, 1.0 / 60.0);
            sh.step(cx, &input);
        }
        // Half a second at 430 bites a minute is three or four bites, and
        // every one of them takes air out of the floor under the player.
        assert!(!out.is_empty(), "the Digger cut nothing at all");
        assert!(out.iter().all(|(_, m)| *m == crate::world::material::AIR), "the Digger put something back");
        let lowest = out.iter().map(|(v, _)| v[1]).min().unwrap();
        assert!(lowest < 8, "it cut above the floor's surface, at voxel {lowest}");

        // A gun with no spade on it leaves the floor alone, however long the
        // button is held. Not the rocket launcher: its burst takes a few
        // voxels out of whatever it lands on (`SplashDef::crater`).
        let mut def = rules();
        def.weapons = vec!["sidearm".into()];
        def.bots.clear();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 11);
        let mut player = Player::new(Vec3::new(0.0, 2.2, 0.0), 0.0);
        player.pitch = -1.5;
        let mut none: Vec<([i32; 3], u8)> = Vec::new();
        for _ in 0..30 {
            actions.dig = true;
            let cx = ActivityCtx {
                player: &mut player,
                store: &store,
                ents: &mut ents,
                models: &[],
                sea: 0.0,
                land: None,
                slopes: false,
                screenshot_request: &mut false,
                edits: &mut none,
            };
            let mut cx = cx;
            sh.frame(&mut cx, &input, &mut actions, 1.0 / 60.0);
            sh.step(cx, &input);
        }
        assert!(none.is_empty(), "the sidearm dug {} voxels", none.len());
    }

    /// Firing a rocket into the floor hurts but does not kill, and the push
    /// is upward: a rocket jump leaves you alive and in the air.
    #[test]
    fn a_rocket_jump_hurts_and_lifts() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let mut sh = Shooter::repeatable(&lib, &rules(), (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 3);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        player.body.on_ground = true;
        player.pitch = -1.4;
        run(&mut sh, &mut player, &store, 40, |i, a| a.dig = i == 0);
        assert_eq!(sh.me.score.shots, 1, "the rocket never left");
        assert!(sh.me.health < 100.0 && sh.me.health > 40.0, "health {}", sh.me.health);
        assert!(player.body.vel.y > 5.0, "it did not lift: {}", player.body.vel.y);
        assert!(sh.alive());
    }

    /// A new match asks for the arena back. The craters the last one blew in
    /// it belong to that match and not to the map, and `Game` is the only
    /// one that can mend them, since putting a column back needs the
    /// streamer and the shooter is handed the world to read.
    #[test]
    fn a_new_match_asks_for_the_arena_back() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let mut sh = Shooter::repeatable(&lib, &rules(), (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 3);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        assert!(!sh.wants_world_clean(), "a match that has not restarted wants nothing mended");
        sh.restart(round::Reset { scores: true }, &mut player, &store);
        assert!(sh.wants_world_clean(), "the new match was left the craters of the last one");
        assert!(!sh.wants_world_clean(), "the world was mended twice");
    }

    /// Dying holds the body still and brings it back at a spawn point, whole.
    #[test]
    fn dying_brings_you_back_at_a_spawn() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let mut sh = Shooter::repeatable(&lib, &rules(), (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 3);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        assert!(sh.me.take(140.0));
        sh.down(0, 0, &player, Vec3::ZERO, false);
        assert!(!sh.alive() && sh.me.score.deaths == 1);
        run(&mut sh, &mut player, &store, 90, |_, _| {});
        assert!(sh.alive(), "never came back");
        assert_eq!(sh.me.health, 100.0);
        assert!((player.pos.x - 2.0).abs() < 0.01 && (player.pos.z - 3.0).abs() < 0.01, "came back at {:?}", player.pos);
    }

    /// A corpse held under fire comes apart a piece at a time, outermost
    /// pieces first, until nothing of it is left standing; fire that passes
    /// nowhere near it takes nothing off.
    #[test]
    fn a_corpse_is_shot_apart_bit_by_bit() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let def = with_bots();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 7);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        run(&mut sh, &mut player, &store, 30, |_, _| {});
        sh.down(1, 0, &player, Vec3::ZERO, false);
        let body = sh.bots[0].body.pos + Vec3::Y * 0.6;
        let total = sh.avatar.rig.bones.len() + 3;
        // Fire a metre wide of it: nothing.
        for _ in 0..20 {
            sh.combat.strike(body + Vec3::new(-5.0, 0.0, 1.5), body + Vec3::new(5.0, 0.0, 1.5), 4.0);
            sh.chip();
        }
        assert_eq!(sh.corpses.len(), 1, "the kill left no corpse");
        assert_eq!(sh.corpses[0].missing, 0, "fire that missed took a piece off");
        // A minigun's worth, a round a step, straight through it.
        let mut counts = Vec::new();
        for _ in 0..40 {
            sh.combat.strike(body - Vec3::X * 5.0, body + Vec3::X * 5.0, 4.0);
            sh.chip();
            counts.push(sh.corpses[0].missing.count_ones());
        }
        // One at a time, not all at once.
        assert!(counts.windows(2).all(|w| w[1] - w[0] <= 1), "more than one piece came off a round: {counts:?}");
        assert!(counts[3] >= 1, "four rounds took nothing off");
        let gone = *counts.last().unwrap() as usize;
        assert!(gone >= 10, "only {gone} pieces came off after forty rounds");
        let chunks: usize = sh.gibs.iter().map(|g| g.2.len()).sum();
        assert_eq!(chunks, gone, "every piece that came off should be a chunk");
        // Keep at it and nothing is left.
        for _ in 0..200 {
            sh.combat.strike(body - Vec3::X * 5.0, body + Vec3::X * 5.0, 4.0);
            sh.chip();
        }
        let left = total - sh.corpses[0].missing.count_ones() as usize;
        assert!(left <= 2, "{left} parts were still on it after a long burst");
    }

    /// A match with a target score ends when somebody reaches it, the world
    /// stops behind the scoreboard, and Enter starts another one with the
    /// scores back to nothing (round.rs).
    #[test]
    fn a_match_ends_at_the_target_and_plays_again() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let mut def = with_bots();
        def.target_score = 2;
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 7);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        sh.down(1, 0, &player, Vec3::ZERO, false);
        assert!(!sh.round.frozen(), "one kill of two ended the match");
        sh.down(2, 0, &player, Vec3::ZERO, false);
        assert!(sh.round.frozen() && sh.pauses_world(), "the second kill did not end it");
        assert_eq!(sh.me.score.kills, 2);
        // Enter, once the scoreboard has had its moment.
        run(&mut sh, &mut player, &store, 120, |i, a| a.accept = i == 90);
        assert!(!sh.round.frozen(), "it never started again");
        assert_eq!(sh.me.score.kills, 0, "the new match kept the old score");
        assert!(sh.alive() && sh.bots.iter().all(|b| b.fighter.alive()), "somebody was left dead");
    }

    /// A kill puts up the banner and the killing-blow marker, a second one
    /// soon after is a DOUBLE KILL, and a bot a blast kills is drawn as
    /// chunks -- every part of its body and its lamps -- until it is back.
    #[test]
    fn a_kill_is_announced_and_a_blast_breaks_the_body() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let def = with_bots();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 7);
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        run(&mut sh, &mut player, &store, 30, |_, _| {});
        sh.down(1, 0, &player, Vec3::new(4.0, 3.0, 0.0), true);
        assert!(sh.mark_kill > 0.0, "no killing-blow marker");
        let (name, streak, _) = sh.banner.clone().expect("no banner");
        assert_eq!((name, streak), (sh.bots[0].name.to_uppercase(), 1));
        assert!(sh.corpses.is_empty(), "a blast kill left a whole corpse");
        let chunks = sh.gibs.first().map(|g| g.2.len()).expect("a blast kill left the body whole");
        assert_eq!(chunks, sh.avatar.rig.bones.len() + 3, "not every part came off");
        // Somebody else's kill does not break the streak; a second of the
        // player's own inside STREAK_S is a double.
        run(&mut sh, &mut player, &store, 30, |_, _| {});
        sh.down(2, 0, &player, Vec3::ZERO, false);
        assert_eq!(sh.banner.as_ref().map(|b| b.1), Some(2), "the second kill was not a double");
        assert_eq!(sh.gibs.len(), 1, "a kill with no blast broke the body up");
        assert_eq!(sh.corpses.len(), 1, "a kill with no blast left no corpse");
        // Back from the dead, with the chunks still lying where they fell:
        // the dead last twice as long as the wait to respawn.
        for _ in 0..60 {
            if sh.bots[0].fighter.alive() {
                break;
            }
            run(&mut sh, &mut player, &store, 10, |_, _| {});
        }
        assert!(sh.bots[0].fighter.alive(), "the bot never came back");
        assert!(!sh.gibs.is_empty(), "the chunks went with the bot's respawn");
        for _ in 0..60 {
            if sh.gibs.is_empty() {
                break;
            }
            run(&mut sh, &mut player, &store, 10, |_, _| {});
        }
        assert!(sh.gibs.is_empty() && sh.corpses.is_empty(), "the dead were never cleared away");
        assert!(sh.banner.is_none(), "the banner never went away");
    }

    /// Two easy bots on a floor with the player in the open: they move, they
    /// find each other, somebody takes damage, and nobody is left stuck.
    #[test]
    /// Every test here builds the match with `Shooter::repeatable`, not
    /// `Shooter::new`, and that is not tidiness. `new` draws its spawn
    /// points off the system clock on purpose, so these matches started
    /// somewhere different every run: this one was red in the full suite and
    /// green on its own for weeks, and the material table got the blame.
    fn two_bots_fight() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let def = with_bots();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 11);
        assert_eq!(sh.bots.len(), 2);
        // The graph is resolved against the voxels by ticking, not at load,
        // so it is empty until the match has run (`settle_nav`).
        assert!(sh.nav.is_empty(), "the graph was built before the world was there");
        let started: Vec<Vec3> = sh.bots.iter().map(|b| b.pos()).collect();
        let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        // Thirty seconds of match, in seconds, so how far each bot ever got
        // from its spawn can be watched rather than only where it ended up.
        let mut far = vec![0.0f32; sh.bots.len()];
        for _ in 0..30 {
            run(&mut sh, &mut player, &store, 60, |_, _| {});
            for (k, b) in sh.bots.iter().enumerate() {
                far[k] = far[k].max(b.pos().distance(started[k]));
            }
        }
        assert_eq!(sh.nav.len(), 5, "the graph never settled");
        // Every bot got somewhere, not just one of them, and the measure is
        // how far it ever got from where it started rather than where it
        // finished: a bot circling a fight ends up back near where it began
        // and has not been stuck for a moment.
        for (k, b) in sh.bots.iter().enumerate() {
            assert!(far[k] > 4.0, "{} never got further than {:.1} m from its spawn", b.name, far[k]);
        }
        let shots: u32 = sh.bots.iter().map(|b| b.fighter.score.shots).sum();
        assert!(shots > 0, "neither bot fired a round in thirty seconds");
        let frags: u32 = sh.scores().iter().map(|(_, s)| s.kills).sum();
        let hurt = sh.me.health < 100.0 || sh.bots.iter().any(|b| b.fighter.health < 100.0) || frags > 0;
        assert!(hurt, "thirty seconds and nobody was so much as scratched");
        for b in &sh.bots {
            assert!(b.fighter.alive() || b.fighter.respawn > 0.0, "{} is stuck dead", b.name);
        }
        assert_eq!(sh.scores().len(), 3, "the scoreboard lost somebody");
    }

    /// A bot that has been killed is still drawn, lying where it fell, for
    /// as long as it takes to come back.
    ///
    /// This is worth a test rather than a look, because the look is the
    /// hard part: catching a bot's death in front of a headless camera took
    /// a probe run, a bisection and a fixed firing window, and the body was
    /// behind a wall when it was finally found. A body that quietly stopped
    /// being drawn would take the one piece of feedback a kill gives you
    /// with it, and nothing else in the suite would notice.
    #[test]
    fn a_dead_bot_is_still_drawn() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let def = with_bots();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 11);
        let player = Player::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        // Models enough to index: the guns, then the bodies, which is the
        // order `Game::add_ride_models` appends them in.
        let mut models = crate::combat::viewmodel::models();
        let parts = avatar::build(Some(&lib), &mut crate::models::palette()).models.len();
        models.extend(avatar::build(Some(&lib), &mut crate::models::palette()).models);
        let drawn = |sh: &Shooter| -> Vec<usize> {
            let mut out = Vec::new();
            sh.instances(&player, 1.0, 0, &models, &mut out);
            let body = crate::combat::viewmodel::COUNT;
            out.iter().map(|(m, _)| *m).filter(|m| (body..body + parts).contains(m)).collect()
        };
        let whole = drawn(&sh);
        assert!(whole.len() > 20, "two living bots drew only {} parts", whole.len());

        let fell = sh.bots[0].pos();
        sh.down(1, 0, &player, Vec3::ZERO, false);
        let dead = drawn(&sh);
        // Exactly as many parts: a body that has fallen is drawn limb for
        // limb like a standing one, lamp included. Only the gun goes.
        assert_eq!(dead.len(), whole.len(), "the dead bot stopped being drawn");
        // A second on, it is down where it fell rather than standing.
        for _ in 0..60 {
            sh.corpses[0].step(&store, 1.0 / 60.0);
        }
        let c = &sh.corpses[0];
        let top = c.parts(1.0).map(|p| p.1.y).fold(f32::MIN, f32::max);
        assert!(top - fell.y < 0.9, "the body is still upright: its top is {:.2} m up", top - fell.y);
        assert!(c.parts(1.0).all(|p| p.1.with_y(0.0).distance(fell.with_y(0.0)) < 2.5), "the body wandered off where it fell");
        // Shooting one is another matter: `hitboxes` says only the living
        // are there, so a corpse cannot be hit or farmed for frags.
        let boxes = sh.hitboxes(&player);
        assert!(!boxes[1].alive, "a corpse is still a target");
    }

    /// A body still has in it whatever killed it. A rocket throws a corpse
    /// away from where it died; it comes down, slides and stops.
    ///
    /// The bug this is against is the one you see rather than measure: a
    /// body used to be pinned at the point of death, so a bot shot in a
    /// jump, off a gantry or on a stair hung in the air in the pose it was
    /// running in until it respawned.
    #[test]
    fn a_blast_throws_the_body() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let def = with_bots();
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 11);
        let fell = sh.bots[0].pos();
        // Thrown the way a rocket going off beside it would throw it, and
        // thrown inward, since the test floor runs out before the arena's
        // does.
        let away = if fell.x > 0.0 { -9.0 } else { 9.0 };
        sh.bots[0].fighter.die(fell, 30.0, Vec3::new(away, 7.0, 0.0));
        for _ in 0..6 {
            sh.step_bodies(&store, 0.0, None, false);
        }
        let up = sh.bots[0].pos();
        assert!(up.y > fell.y + 0.3, "the body never left the floor: {:.2} m up", up.y - fell.y);

        for _ in 0..150 {
            sh.step_bodies(&store, 0.0, None, false);
        }
        let rest = sh.bots[0].pos();
        let went = rest.distance(fell);
        assert!(went > 3.0, "the blast moved the body only {went:.1} m");
        assert!((rest.y - fell.y).abs() < 0.3, "it came to rest {:.2} m off the floor it fell on", rest.y - fell.y);
        let left = sh.bots[0].vel().length();
        assert!(left < 0.6, "it is still going at {left:.1} m/s");
        // And it is lying on the floor rather than standing where it slid to.
        let pose = sh.bots[0].figure.at(1.0);
        let over = pose.bones[0].x.abs().max(pose.bones[0].z.abs());
        assert!(over > 0.8, "the body landed on its feet: {:?}", pose.bones[0]);
    }

    /// A bot is a body like the player's: given the same keys from the same
    /// place, the two end up in the same spot. This is the claim the whole
    /// design rests on, so it is worth a test that would notice if a bot
    /// ever started moving itself.
    #[test]
    fn a_bot_moves_exactly_as_the_player_does() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let store = floor();
        let mut def = with_bots();
        def.bots.truncate(1);
        let mut sh = Shooter::repeatable(&lib, &def, (Vec3::new(0.0, 2.0, 0.0), 0.0), &store, 5);
        let start = sh.bots[0].pos();
        let mut mine = Player::new(start, 0.0);
        mine.arena = true;
        mine.air_jumps = 3;
        let keys = MoveInput {
            forward: true,
            jump: true,
            ..Default::default()
        };
        sh.bots[0].pilot.mv = keys;
        sh.bots[0].pilot.yaw = 0.0;
        for _ in 0..120 {
            mine.walk(&keys, &store, (0.0, glam::Vec2::ZERO), false, STEP);
            // The bot's body only, with the brain left out of it.
            let b = &mut sh.bots[0];
            b.body.yaw = b.pilot.yaw;
            b.body.walk(&keys, &store, (0.0, glam::Vec2::ZERO), false, STEP);
        }
        let d = sh.bots[0].pos().distance(mine.pos);
        assert!(d < 1e-3, "the bot ended {d} m from where the player did");
    }

    /// `Shooter::repeatable` means it. Every measurement this engine makes of
    /// a match -- the tournament, the weapon bench, a headless report -- is a
    /// difference between two runs, and all of them were quietly reading
    /// spawn luck instead, because `Shooter::new` draws its spawn points off
    /// the system clock. Thirty seconds of the same two bots, twice, has to
    /// come out in the same place.
    #[test]
    fn a_measured_match_repeats() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let def = with_bots();
        let home = (Vec3::new(0.0, 2.0, 0.0), 0.0);
        let play = || {
            let mut sh = Shooter::repeatable(&lib, &def, home, &store, 23);
            let mut player = Player::new(home.0, home.1);
            run(&mut sh, &mut player, &store, 1800, |_, _| {});
            let ends: Vec<Vec3> = sh.bots.iter().map(|b| b.pos()).collect();
            let hurt: Vec<f32> = sh.bots.iter().map(|b| b.fighter.score.dealt).collect();
            (ends, hurt)
        };
        let (a_ends, a_hurt) = play();
        let (b_ends, b_hurt) = play();
        assert_eq!(a_ends, b_ends, "the bots ended somewhere else the second time");
        assert_eq!(a_hurt, b_hurt, "the same match dealt {a_hurt:?} and then {b_hurt:?}");
        // And something happened, or this would pass on two empty matches.
        assert!(a_hurt.iter().sum::<f32>() > 0.0, "thirty seconds and no damage at all");
    }

    /// A benched player is not in the match: nothing shoots them and nothing
    /// they are hit by scores. The tournament needs it, because a body
    /// standing at the spawn for a whole round is a free kill.
    #[test]
    fn a_benched_player_is_not_a_target() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let store = floor();
        let def = with_bots();
        let home = (Vec3::new(0.0, 2.0, 0.0), 0.0);
        let mut sh = Shooter::repeatable(&lib, &def, home, &store, 23);
        sh.bench_player();
        let mut player = Player::new(home.0, home.1);
        run(&mut sh, &mut player, &store, 1800, |_, _| {});
        assert_eq!(sh.me.health, sh.me.full_health, "the benched player was hurt");
        assert_eq!(sh.me.score.deaths, 0, "the benched player died");
        let frags: u32 = sh.bots.iter().map(|b| b.fighter.score.kills).sum();
        let dealt: f32 = sh.bots.iter().map(|b| b.fighter.score.dealt).sum();
        // The bots fought each other, so the match ran; they just cannot
        // have been scoring off the statue, which is what the two
        // assertions above say. This one is only a bound on two bots
        // trading on an open floor with nothing to hide behind. It was 2
        // when a body walked at 5 m/s with one jump; on the arena's pace,
        // with three jumps and a dodge, they close and trade faster.
        assert!(dealt > 0.0, "thirty seconds and no damage at all");
        assert!(frags <= 6, "{frags} kills between two bots in thirty seconds");
    }
}
