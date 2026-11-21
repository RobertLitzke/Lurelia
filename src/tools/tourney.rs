//! A tournament for bots, scored on how they move rather than on who wins.
//!
//! The problem this exists for: a bot change is a claim, and a live match is
//! the worst instrument for settling one. A match has two bots, a map, a
//! weapon table and a dozen random rolls in it, and the thing that comes out
//! is a kill count. So "the wall slide helps" and "the circling makes them
//! harder to hit" and "they get stuck on imported geometry" were all, until
//! this file, opinions with an anecdote attached. Two sessions of watching
//! one match at a time produced three confident conclusions and all three
//! were wrong (docs/BOTS.md, "The tournament").
//!
//! So: one bot, alone on a map, for a minute, scored on where it went.
//!
//! - **coverage**, the share of the navigation graph it came within reach of.
//!   The headline number, because a bot that has seen a tenth of the map is
//!   a bot that will not be where you are.
//! - **metres**, how far it actually travelled.
//! - **stalled**, the share of its time asking to move and not moving. This
//!   is the one that catches a bot pacing back and forth or grinding a wall,
//!   neither of which a distance figure notices.
//! - **jams**, the times it had to shove itself out of something.
//!
//! Nobody is shooting, so none of it is luck. Entrants differ by tuning and
//! by which parts of their movement are switched on (`duel::Style`), which
//! makes "what is the wall slide worth" a question with an answer: run the
//! same bot on the same seeds without it.
//!
//! The second bracket fights them, with every entrant's aim forced to the
//! same value so that aim cannot be what wins. It reports the engagement
//! range and the time spent inside four metres, because the degenerate
//! answer to "make them hit each other" is "walk up to them", and a score
//! that does not watch for it will find it.
//!
//! It runs with no window and no GPU: the world generates into a voxel store
//! on this thread, and the match is `Shooter::tick` on a fixed step. A
//! minute of match takes about a second, and the same arguments give the
//! same table every time, which is what makes a before-and-after comparison
//! worth anything.
//!
//! That last sentence was false for as long as this file has existed, and it
//! said so in these words. There was a wall clock in the loop: the spawn
//! points are drawn from a stream seeded off the system clock, deliberately,
//! so that two matches in a row do not open in the same corner of a map. So
//! every round put the bots somewhere else, and the fight bracket -- which
//! is a difference between two rounds -- was reading spawn luck. Rounds come
//! from `Shooter::repeatable` now, and `fight` opens by playing one round
//! twice and complaining if the two differ.
//!
//! **The lifts and the teleporters run.** The loop drives `Shooter::tick`
//! rather than `Game`, so for a long time the platforms and portals stood
//! still in here, and the tournament was measuring a bot on the part of the
//! map it could walk to. It steps them with the game's own step
//! (`world_parts::WorldParts`), for the same reason the whole file exists:
//! an arena whose upper floors are reached by lift cannot be scored on
//! coverage with the lifts switched off. The `rides`, `tele` and `took`
//! columns say whether the bot used any of it.

use crate::brain::person;
use crate::cli::Args;
use crate::player::{MoveInput, Player};
use crate::shooter::Shooter;
use crate::world::fields::{BotDef, ShooterDef};
use crate::world::store::Store;
use crate::{worldload, Vec3};
use std::time::Duration;

/// How close a bot has to get to a node to have been there: across the
/// floor, with height forgiven, which is how the engine itself counts
/// arriving at one (`nav::Path::advance`).
///
/// Measuring it as a plain distance in three dimensions was wrong and read
/// as a damning result: several of Slagdeck's nodes resolve a metre and a
/// half off the floor the bot actually walks on, so a bot standing on a node
/// was often three metres from it by that measure. Coverage came out at
/// 8-17% for every entrant while the bot was in fact crossing the whole
/// arena -- which is what `spread` and `lost` were added to catch, and did.
const TOUCHED_M: f32 = 3.0;
const TOUCHED_UP_M: f32 = 2.5;
/// Below this, in metres a second, a bot that wanted to move did not.
const STALLED_MS: f32 = 1.2;
/// Further than this from every node, a bot is somewhere the graph has
/// nothing to say about.
const LOST_M: f32 = 10.0;
/// Inside this, two fighters are nose to nose, which is the thing the fight
/// bracket exists to catch.
const NOSE_M: f32 = 4.0;

/// One competitor: a tuning, and which parts of its movement are on.
pub struct Entrant {
    pub name: &'static str,
    pub skill: &'static str,
    pub bold: f32,
    pub steady: f32,
    pub without: &'static [&'static str],
}

/// The field. Deliberately small and deliberately boring: one of each
/// difficulty to see whether the table means anything for movement, then the
/// shipping bot with one thing switched off at a time, which is the only way
/// to find out what each part is worth.
pub const FIELD: &[Entrant] = &[
    Entrant {
        name: "novice",
        skill: "novice",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "easy",
        skill: "easy",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "average",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "skilled",
        skill: "skilled",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "godlike",
        skill: "godlike",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "easy bold",
        skill: "easy",
        bold: 0.8,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "easy timid",
        skill: "easy",
        bold: -0.8,
        steady: 0.0,
        without: &[],
    },
    Entrant {
        name: "no wall slide",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &["wall_slide"],
    },
    Entrant {
        name: "no unjam",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &["unjam"],
    },
    Entrant {
        name: "no circling",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &["circle"],
    },
    Entrant {
        name: "no fetching",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &["fetch"],
    },
    Entrant {
        name: "no hearing",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &["hearing"],
    },
    // The noise floor, and the most useful row in the table: an entrant
    // identical to the control in every respect. Its `vs house` is the
    // difference between two runs of the same bot, so whatever it prints is
    // what this instrument cannot tell apart from nothing. Any entrant
    // whose margin is inside the twin's is a result that has not happened.
    //
    // The control's own row is 0 +-0 by construction -- it is differenced
    // against itself -- so it cannot serve this purpose, and for seven
    // rounds there was nothing in the table saying how big a difference had
    // to be before it was one.
    Entrant {
        name: "twin",
        skill: "average",
        bold: 0.0,
        steady: 0.0,
        without: &[],
    },
];

impl Entrant {
    fn bot(&self, name: &str) -> BotDef {
        BotDef {
            name: name.to_string(),
            skill: self.skill.to_string(),
            colour: None,
            bold: self.bold,
            steady: self.steady,
            without: self.without.iter().map(|s| s.to_string()).collect(),
            tune: Default::default(),
            team: None,
        }
    }
}

/// What one round of one bot came to.
#[derive(Default, Clone, Copy)]
struct Round {
    coverage: f32,
    metres: f32,
    stalled: f32,
    jams: f32,
    asked: f32,
    failed: f32,
    /// Times it set off toward something it heard.
    heards: f32,
    /// The diagonal of the box its positions fit in: a sanity check on
    /// coverage, since a bot confined to a corner and a bot crossing the map
    /// can travel the same number of metres.
    spread: f32,
    /// How many metres of height it covered. Slagdeck is a hall with a
    /// walkway ring four metres up, galleries above that and a pit below,
    /// so a bot that never leaves the deck floor reads as a couple of
    /// metres here however far it walks.
    climb: f32,
    /// The share of its time not asking to move at all.
    ///
    /// Not the same as `stalled`, which only counts ticks where a direction
    /// is being pressed: a bot leaning on a wall is stalled, and a bot that
    /// has simply stopped deciding to go anywhere is invisible to it. That
    /// is the one that shows in play as "sometimes the bots just stand
    /// still", so it needs a column of its own.
    idle: f32,
    /// The share of its time spent further than `LOST_M` from any node at
    /// all -- somewhere the graph does not describe, which on a map that is
    /// a building in a landscape means it has walked out of the building.
    lost: f32,
    /// Lifts ridden, teleporters stepped into and pads picked up: whether
    /// the bot used the map it was given, which on an imported arena is most
    /// of what "the bots barely use the floor" was about.
    rides: f32,
    tele: f32,
    took: f32,
    /// Fight bracket only. `dealt` and `taken` are damage, which is the
    /// believable half of this bracket: a sixty-second duel produces about
    /// one kill, and one kill cannot be averaged into an answer. Damage is
    /// a hundred-odd points landed in a few dozen pieces, so it says the
    /// same thing with a tenth of the spread.
    kills: f32,
    deaths: f32,
    dealt: f32,
    taken: f32,
    accuracy: f32,
    range: f32,
    nose: f32,
    /// The share of its time in each act, in `Act`'s own order: roam, hunt,
    /// fight, close, break, falling. The movement bracket cannot see a
    /// fight behaviour at all, and the fight bracket was reporting only
    /// damage, so "the bots are aimless" and "they stand about" had no
    /// column anywhere. This is that column: a bot that spends half a duel
    /// hunting somebody it cannot route to looks fine on damage and is
    /// exactly what reads as aimless in play.
    acts: [f32; 6],
}

/// `Act::name` in `Act`'s own order, for the histogram's headings.
const ACTS: [&str; 6] = ["roam", "hunt", "fight", "close", "break", "fall"];

fn act_index(name: &str) -> Option<usize> {
    ACTS.iter().position(|&a| a == name)
}

impl Round {
    fn add(&mut self, o: &Round) {
        self.coverage += o.coverage;
        self.metres += o.metres;
        self.stalled += o.stalled;
        self.jams += o.jams;
        self.asked += o.asked;
        self.failed += o.failed;
        self.heards += o.heards;
        self.spread += o.spread;
        self.lost += o.lost;
        self.idle += o.idle;
        self.climb += o.climb;
        self.rides += o.rides;
        self.tele += o.tele;
        self.took += o.took;
        self.kills += o.kills;
        self.deaths += o.deaths;
        self.dealt += o.dealt;
        self.taken += o.taken;
        self.accuracy += o.accuracy;
        self.range += o.range;
        self.nose += o.nose;
        for (a, b) in self.acts.iter_mut().zip(o.acts.iter()) {
            *a += *b;
        }
    }

    fn over(&self, n: f32) -> Round {
        Round {
            coverage: self.coverage / n,
            metres: self.metres / n,
            stalled: self.stalled / n,
            jams: self.jams / n,
            asked: self.asked / n,
            failed: self.failed / n,
            heards: self.heards / n,
            spread: self.spread / n,
            lost: self.lost / n,
            idle: self.idle / n,
            climb: self.climb / n,
            rides: self.rides / n,
            tele: self.tele / n,
            took: self.took / n,
            kills: self.kills / n,
            deaths: self.deaths / n,
            dealt: self.dealt / n,
            taken: self.taken / n,
            accuracy: self.accuracy / n,
            range: self.range / n,
            nose: self.nose / n,
            acts: self.acts.map(|a| a / n),
        }
    }

    /// One number to sort by, and it is coverage with the two failure modes
    /// taken off it. A bot that covers the map but spends a third of its
    /// time grinding a wall has not solved the problem; nor has one that
    /// covers it by jamming and shoving its way around.
    fn score(&self) -> f32 {
        self.coverage * 100.0 - self.stalled * 60.0 - self.idle * 60.0 - self.jams * 1.5
    }
}

/// The world, loaded and generated, with nobody playing it yet.
pub struct Field {
    pub store: Store,
    pub rules: ShooterDef,
    pub home: (Vec3, f32),
    /// The world itself, which also owns the library every match reads.
    pub land: std::sync::Arc<crate::world::fields::World>,
    /// The platforms as the game builds them (the world's own and its
    /// generated doors) and their built boxes, in the same order, so a round
    /// can run the lifts.
    pub platform_defs: Vec<crate::platforms::PlatformDef>,
    pub platforms: Vec<crate::platforms::Built>,
    pub seed: u32,
}

/// Build the world and fill a voxel store around the spawn, with no window
/// and no GPU.
///
/// `Streamer` meshes every column it generates and offers the mesh to a
/// sink; the sink here throws them away, which is the whole of what makes
/// this headless. The voxels still land in the store, which is all a match
/// needs: nothing in `Shooter` or `brain` reads a mesh.
pub fn open(args: &Args) -> Result<Field, String> {
    struct NoSink;
    impl crate::stream::ChunkSink for NoSink {
        fn upload(&mut self, _origin: Vec3, _scale: f32, _mesh: &crate::mesh::Mesh) -> Option<crate::stream::GpuChunk> {
            None
        }
        fn free(&mut self, _c: &crate::stream::GpuChunk) {}
    }
    let name = worldload::selected();
    let loaded = worldload::load(&name)?;
    let world = loaded.world.clone();
    let rules = world.def.shooter.clone().ok_or_else(|| format!("world {name} has no shooter block, so there is no match to run"))?;
    let gen = crate::world::gen::Generator::with_world(world.clone());
    let (spawn, yaw) = gen.spawn_point();
    let height_vox = gen.height_vox;
    let mut store = Store { height_vox, gravity: world.def.gravity, ..Store::default() };
    let mut streamer = crate::stream::Streamer::new(gen, Default::default(), args.view.max(128.0), 4);
    streamer.set_smooth(false, Vec3::ZERO);
    // The whole arena, as `Game` streams it: from the spawn alone the far
    // base of a wide map (Face, Starfall) never loads and its nav nodes
    // settle on nothing.
    streamer.full_box = crate::stream::arena_box(&rules.nav);
    let mut sink = NoSink;
    let began = std::time::Instant::now();
    // The arena has to be all the way in before a bot walks it, or the nav
    // graph settles against holes and every bot stands inside a wall.
    //
    // Columns are generated on worker threads, so this has to wait for wall
    // clock rather than spin: an earlier version called `update` six hundred
    // times in under a second, finished with an empty store, and produced a
    // tournament in which every entrant covered 2% of the map and stalled
    // 100% of the time -- a table of plausible-looking numbers measuring
    // bots wedged in solid rock.
    loop {
        streamer.update(spawn, Vec3::NEG_Z, &mut sink, &mut store, Duration::from_millis(20));
        let (full, _) = store.census();
        if streamer.idle() && full > 0 {
            break;
        }
        if began.elapsed().as_secs_f32() > 120.0 {
            return Err(format!("the world did not finish streaming in two minutes ({full} chunks in)"));
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let seed = world.def.seed;
    Ok(Field {
        store,
        rules,
        home: (spawn, yaw),
        land: world,
        platform_defs: loaded.platform_defs,
        platforms: loaded.platforms,
        seed,
    })
}

/// Run one match and measure it. `bots` is the roster; `watch` is which bot
/// the numbers are about.
fn play(f: &Field, store: &mut Store, bots: Vec<BotDef>, watch: usize, seconds: f32, seed: u32, aim: Option<f32>) -> Round {
    let mut rules = f.rules.clone();
    rules.bots = bots;
    // Nobody is picking anything up in a movement round, but the pads still
    // stand there and a fetching bot still wants them, which is the point.
    rules.target_score = 0;
    let mut sh = Shooter::repeatable(&f.land.lib, &rules, f.home, store, seed);
    // The moving parts of the world, which `Game` owns and this loop has to
    // run itself. Fresh every round: a plate settles once and stays where
    // the last round left it otherwise, and a round is meant to be a
    // repeat, not a continuation.
    let plats = crate::platforms::Platforms::new(&f.platform_defs, &f.platforms, 0);
    let gates = crate::portals::Portals::new(&f.land.def.portals, |_| {});
    let mut parts = crate::world_parts::WorldParts::new(plats, gates, f.home.0);
    if let Some(a) = aim {
        sh.handicap(a);
    }
    // No human in a measurement of bots. The player below stands at the
    // spawn and never moves, which is not nothing: it is a free kill, and a
    // reason for a lone bot to stop walking the map and fight the statue.
    // VOXEL_TOURNEY_PLAYER=1 leaves them in, which is how the difference was
    // measured rather than assumed (docs/BOTS.md, "The tournament").
    if std::env::var("VOXEL_TOURNEY_PLAYER").map(|v| v == "0").unwrap_or(true) {
        sh.bench_player();
    }
    // The player stands at the spawn and does nothing. A tournament is about
    // the bots; a human in it would be the variable.
    let mut player = Player::new(f.home.0, f.home.1);
    let mut ents = crate::entities::Entities::new(Default::default(), 1);
    let mut actions = crate::game::Actions::default();
    let input = MoveInput::default();
    let mut shot = false;
    let step = crate::game::STEP;
    let steps = (seconds / step) as usize;

    // A set rather than a vector of flags, because the graph does not exist
    // yet: it is resolved against the real voxels over the first few ticks
    // of the match (`Shooter::settle_nav`), so at this point it has no
    // nodes at all and will have fifty-odd shortly.
    let mut touched: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut last = sh.bots.get(watch).map_or(Vec3::ZERO, |b| b.pos());
    let (mut metres, mut stalled, mut moving_ticks) = (0.0f32, 0.0f32, 0.0f32);
    let (mut range_sum, mut range_n, mut nose) = (0.0f32, 0.0f32, 0.0f32);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let (mut lost, mut ticks, mut idle) = (0.0f32, 0.0f32, 0.0f32);
    let mut acts = [0.0f32; 6];
    let mut edits = Vec::new();
    // What the voxels were before, so the next round starts on the same
    // ground: the store is shared, and a round that found the platforms
    // already down would be a different round from the first.
    let mut was: Vec<([i32; 3], u8)> = Vec::new();
    for _ in 0..steps {
        // The player is benched, so only the bots ride.
        parts.step(step, store, None, Some(&mut sh));
        let mut cx = crate::modes::ActivityCtx {
            player: &mut player,
            store,
            ents: &mut ents,
            models: &[],
            sea: 0.0,
            land: Some(&f.land),
            slopes: false,
            screenshot_request: &mut shot,
            edits: &mut edits,
        };
        crate::modes::Activity::frame(&mut sh, &mut cx, &input, &mut actions, step);
        crate::modes::Activity::step(&mut sh, cx, &input);
        // What the round changed in the voxels (the pickup platforms),
        // which `Game` hands to the streamer and this has to lay itself.
        for ([x, y, z], m) in edits.drain(..) {
            was.push(([x, y, z], store.get(x, y, z).unwrap_or(crate::world::material::AIR)));
            store.set(x, y, z, m);
        }
        let Some(b) = sh.bots.get(watch) else { break };
        let at = b.pos();
        lo = lo.min(at);
        hi = hi.max(at);
        let went = at.distance(last);
        last = at;
        metres += went;
        // Wanting to move and not moving. Pressing a key is the test, not
        // having a destination: a bot standing still on purpose is not
        // stalled, and a bot leaning on a wall is, whatever it believes.
        let keys = b.pilot.mv.forward || b.pilot.mv.back || b.pilot.mv.left || b.pilot.mv.right;
        // A passenger on a lift presses nothing and is not idle: the lift
        // is the move it chose.
        if !keys && !parts.platforms.riding(at, b.body.body.half_w) {
            idle += 1.0;
        }
        if keys {
            moving_ticks += 1.0;
            if went / step < STALLED_MS {
                stalled += 1.0;
            }
        }
        let mut nearest = f32::MAX;
        for (i, n) in sh.nav().nodes.iter().enumerate() {
            let flat = Vec3::new(n.at.x - at.x, 0.0, n.at.z - at.z).length();
            nearest = nearest.min(flat);
            if flat < TOUCHED_M && (n.at.y - at.y).abs() < TOUCHED_UP_M {
                touched.insert(i);
            }
        }
        if !sh.nav().is_empty() {
            ticks += 1.0;
            if nearest > LOST_M {
                lost += 1.0;
            }
        }
        if let Some(a) = act_index(b.brain.doing()) {
            acts[a] += 1.0;
        }
        // Only while it is actually fighting. Counting every tick with a
        // finite range in it averaged the distance to a stale memory while
        // roaming into the engagement range, which is how this column came
        // to read 25 m on an arena whose sightlines are about ten and whose
        // bots want to be inside sixteen (`Duel::band`). Three rounds were
        // read against that number before anybody checked what it was.
        let fighting = matches!(b.brain.doing(), "fight" | "close");
        let (_, r, _, _) = b.brain.aiming_at();
        if fighting && r.is_finite() && r < 400.0 {
            range_sum += r;
            range_n += 1.0;
            if r < NOSE_M {
                nose += 1.0;
            }
        }
    }
    for ([x, y, z], m) in was.into_iter().rev() {
        store.set(x, y, z, m);
    }
    let score = sh.scores();
    let mine = score.get(watch + 1).map(|(_, s)| *s).unwrap_or_default();
    Round {
        coverage: if sh.nav().is_empty() { 0.0 } else { touched.len() as f32 / sh.nav().len() as f32 },
        metres,
        stalled: if moving_ticks > 0.0 { stalled / moving_ticks } else { 0.0 },
        jams: sh.bots.get(watch).map_or(0.0, |b| b.brain.jams() as f32),
        asked: sh.bots.get(watch).map_or(0.0, |b| b.brain.wandering().0 as f32),
        failed: sh.bots.get(watch).map_or(0.0, |b| b.brain.wandering().1 as f32),
        heards: sh.bots.get(watch).map_or(0.0, |b| b.brain.heards() as f32),
        rides: parts.platforms.list.iter().map(|p| p.rides as f32).sum(),
        tele: parts.portals.list.iter().map(|p| p.used as f32).sum(),
        took: sh.bots.get(watch).map_or(0.0, |b| b.fighter.picked as f32),
        spread: Vec3::new(hi.x - lo.x, 0.0, hi.z - lo.z).length(),
        climb: hi.y - lo.y,
        lost: if ticks > 0.0 { lost / ticks } else { 0.0 },
        idle: idle / steps.max(1) as f32,
        kills: mine.kills as f32,
        deaths: mine.deaths as f32,
        dealt: sh.bots.get(watch).map_or(0.0, |b| b.fighter.score.dealt),
        taken: sh.bots.iter().enumerate().filter(|(i, _)| *i != watch).map(|(_, b)| b.fighter.score.dealt).sum(),
        accuracy: if mine.shots > 0 { mine.hits as f32 / mine.shots as f32 } else { 0.0 },
        range: if range_n > 0.0 { range_sum / range_n } else { 0.0 },
        nose: if range_n > 0.0 { nose / range_n } else { 0.0 },
        acts: acts.map(|a| a / steps.max(1) as f32),
    }
}

pub fn run(args: &Args) {
    let mut field = match open(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("tournament: {e}");
            std::process::exit(1);
        }
    };
    // The rounds run one after another, so one store serves them all and the
    // lifts can write their slabs into it.
    let mut store = std::mem::take(&mut field.store);
    let seconds = args.tourney_seconds.unwrap_or(60.0);
    let rounds = args.tourney.unwrap_or(3).max(1);
    println!(
        "{}: {} nav nodes stated, {} pads; {} entrants, {rounds} rounds of {seconds:.0} s each",
        worldload::selected(),
        field.rules.nav.nodes.len(),
        field.rules.pickups.len(),
        FIELD.len()
    );

    println!("\n-- moving: one bot alone on the map, nobody shooting --\n");
    println!(
        "{:<15}{:>9}{:>9}{:>9}{:>8}{:>7}{:>7}{:>7}{:>8}{:>8}{:>8}{:>9}",
        "entrant", "cover", "spread", "climb", "idle", "jams", "rides", "tele", "took", "asked", "failed", "score"
    );
    let mut moving: Vec<(usize, Round)> = Vec::new();
    for (i, e) in FIELD.iter().enumerate() {
        let mut total = Round::default();
        for r in 0..rounds {
            let seed = field.seed ^ (r + 1).wrapping_mul(0x9e3779b9);
            total.add(&play(&field, &mut store, vec![e.bot(e.name)], 0, seconds, seed, None));
        }
        moving.push((i, total.over(rounds as f32)));
    }
    moving.sort_by(|a, b| b.1.score().total_cmp(&a.1.score()));
    for (i, r) in &moving {
        println!(
            "{:<15}{:>8.0}%{:>8.0}m{:>7.1}m{:>7.0}%{:>7.1}{:>7.1}{:>7.1}{:>8.1}{:>8.0}{:>8.0}{:>9.1}",
            FIELD[*i].name,
            r.coverage * 100.0,
            r.spread,
            r.climb,
            r.idle * 100.0,
            r.jams,
            r.rides,
            r.tele,
            r.took,
            r.asked,
            r.failed,
            r.score()
        );
    }

    fight(&field, &mut store, seconds, rounds);
    melee(&field, &mut store, seconds, rounds);
    let _ = person::NAMES;
}

/// Three bots on the map at once, which is the game being played.
///
/// The first two brackets are one bot and two. A player on Slagdeck usually
/// faces three, and whether the bots look aimless or stand about is a
/// question about that game, not about a duel. The difference is not a
/// detail: a duel keeps a contact alive nearly all the time, so a bot in one
/// is rarely in the state where roaming, fetching and hearing decide
/// anything. Three bots spend most of a match not knowing where anybody is,
/// which is exactly where this session's changes live.
///
/// Scored per entrant by substituting it for one of the three and reporting
/// what that bot did, against the shipping bot in the same slot on the same
/// seed. Aim is forced flat here too, for the same reason as the duel.
fn melee(field: &Field, store: &mut Store, seconds: f32, rounds: u32) {
    println!(
        "
-- three bots at once: the game as played, same flat aim --
"
    );
    let flat = 0.4;
    let per = seconds / 60.0;
    // Each entrant in each of the three slots. A bot's brain is seeded off
    // its slot (`Shooter::make`), and a slot also decides which spawn it
    // draws, so slot 0 is not a neutral place to stand: running all three
    // triples the sample and removes the question at the same time.
    let three = |store: &mut Store, e: &Entrant, seed: u32, slot: usize| -> Round {
        let house = FIELD[2].bot("house");
        let mut bots = vec![house.clone(), house.clone(), house];
        bots[slot] = e.bot(e.name);
        play(field, store, bots, slot, seconds, seed, Some(flat))
    };
    let seeds: Vec<u32> = (0..rounds).map(|r| field.seed ^ (r + 1).wrapping_mul(0xc2b2ae35)).collect();

    let mut base: Vec<f32> = Vec::new();
    for &seed in &seeds {
        for slot in 0..3 {
            let r = three(store, &FIELD[2], seed, slot);
            base.push((r.dealt - r.taken) / per);
        }
    }

    let mut table: Vec<(usize, Round, f32, f32)> = Vec::new();
    for (i, e) in FIELD.iter().enumerate() {
        let mut total = Round::default();
        let mut paired: Vec<f32> = Vec::new();
        for (k, &seed) in seeds.iter().enumerate() {
            for slot in 0..3 {
                let r = three(store, e, seed, slot);
                paired.push((r.dealt - r.taken) / per - base[k * 3 + slot]);
                total.add(&r);
            }
        }
        let (mean, err) = spread(&paired);
        table.push((i, total.over(paired.len() as f32), mean, err));
    }
    table.sort_by(|a, b| b.2.total_cmp(&a.2));
    println!(
        "{:<15}{:>12}{:>7}{:>7}{:>7}{:>8}{:>6}{:>6}{:>6}{:>7}   {}",
        "entrant", "vs house", "dealt", "taken", "kills", "range", "idle", "stuck", "took", "heard", "roam/hunt/fight/close/break"
    );
    for (i, r, mean, err) in &table {
        println!(
            "{:<15}{:>7.0} +-{:<3.0}{:>7.0}{:>7.0}{:>7.1}{:>7.1}m{:>5.0}%{:>5.0}%{:>6.1}{:>7.1}   {:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}/{:>2.0} {}",
            FIELD[*i].name,
            mean,
            err,
            r.dealt,
            r.taken,
            r.kills,
            r.range,
            r.idle * 100.0,
            r.stalled * 100.0,
            r.took,
            r.heards,
            r.acts[0] * 100.0,
            r.acts[1] * 100.0,
            r.acts[2] * 100.0,
            r.acts[3] * 100.0,
            r.acts[4] * 100.0,
            if mean.abs() > 2.0 * err { "<-" } else { "" }
        );
    }
    println!(
        "
One entrant against two of the shipping bot, {} matches each ({} seeds x all
three slots), differenced against the shipping bot on that same seed and
slot. `taken` is everybody else's damage, so with two opponents it runs about
twice the duel's.

`twin` is the control under another name, and its row is the instrument
checking itself: a match here is fully deterministic, so the twin prints
0 +-0 to every digit. If it ever prints anything else, something has got a
wall clock into the loop again (docs/BOTS.md) and nothing else in this table
is a difference. It also fixes how the bars should be read -- they are not
run-to-run noise, of which there is none, but how much a result depends on
which spawns a seed drew.",
        seeds.len() * 3,
        seeds.len()
    );
}

/// Mean and standard error of a sample. The standard error is the whole
/// point of this file's second half: a bracket that prints 1.5 against 0.5
/// and says nothing about the spread invites the one conclusion the spread
/// would have forbidden, which is how the old fight table spent two
/// sessions being quoted and being wrong.
fn spread(xs: &[f32]) -> (f32, f32) {
    let n = xs.len() as f32;
    if n < 1.0 {
        return (0.0, f32::INFINITY);
    }
    let mean = xs.iter().sum::<f32>() / n;
    if n < 2.0 {
        return (mean, f32::INFINITY);
    }
    let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / (n - 1.0);
    (mean, (var / n).sqrt())
}

/// The fight bracket: every entrant against the shipping bot, scored on
/// damage and reported with its error.
///
/// Four things make it mean something that the kills column did not.
///
/// **Damage, not kills.** A minute of duel is about one kill. Averaging six
/// of those gives a number whose own noise is wider than any difference
/// worth finding, and the table printed it to one decimal place anyway.
/// Damage dealt less damage taken is the same quantity in points instead of
/// in whole bodies, and it moves in a few dozen pieces a round rather than
/// one.
///
/// **Both sides of the map.** Each seed is played twice, the entrant in the
/// first spawn slot and then in the second, because on an arena the spawns
/// are not equivalent and part of the old spread was which one an entrant
/// happened to draw.
///
/// **Nobody is shooting the player.** The tournament's player stands at the
/// spawn and never moves, so the two bots were partly being scored on who
/// found the statue first (`Shooter::bench_player`).
///
/// There is no accuracy column, and that is deliberate. `Score::hits`
/// counts damage events rather than rounds that connected -- a scrap shell
/// throws nine bodies and a held beam lands one every tick -- so hits over
/// shots came out at 53 for the leader of a give-all match and 2251 for one
/// holding a beam. Damage is the number that survives a mixed arsenal.
///
/// **A control.** The shipping bot plays itself on every seed, which is the
/// margin that a difference of nothing produces on this map. An entrant has
/// been separated from it only where its margin clears the control by more
/// than the two errors together, and the table says which those are rather
/// than leaving it to be eyeballed off a sorted list.
fn fight(field: &Field, store: &mut Store, seconds: f32, rounds: u32) {
    println!("\n-- fighting: the same aim for everyone, so aim is not what wins --\n");
    let flat = 0.4;
    let per = seconds / 60.0;
    let duel = |store: &mut Store, e: &Entrant, seed: u32, side: usize| -> Round {
        let house = FIELD[2].bot("house");
        let mine = e.bot(e.name);
        let bots = if side == 0 { vec![mine, house] } else { vec![house, mine] };
        play(field, store, bots, side, seconds, seed, Some(flat))
    };
    let seeds: Vec<u32> = (0..rounds).map(|r| field.seed ^ (r + 1).wrapping_mul(0x85ebca6b)).collect();

    // Does a round repeat? Everything here assumes it does: the same
    // entrant on the same seed on the same side is meant to be the same
    // match, and a bracket built out of rounds that quietly differ is a
    // bracket measuring its own drift. Cheap to check, so checked.
    let (a, b) = (duel(store, &FIELD[2], seeds[0], 0), duel(store, &FIELD[2], seeds[0], 0));
    if (a.dealt - b.dealt).abs() > 0.01 || (a.taken - b.taken).abs() > 0.01 {
        println!(
            "WARNING: a round does not repeat -- the same bot on the same seed dealt\n{:.0} then {:.0} and took {:.0} then {:.0}. Everything below is drift as much\nas difference.",
            a.dealt, b.dealt, a.taken, b.taken
        );
    }

    // The control first, so every entrant can be differenced against it on
    // the same seed and the same side of the map.
    let mut base: Vec<f32> = Vec::new();
    for &seed in &seeds {
        for side in 0..2 {
            let r = duel(store, &FIELD[2], seed, side);
            base.push((r.dealt - r.taken) / per);
        }
    }
    let (b_mean, b_err) = spread(&base);

    let mut table: Vec<(usize, Round, f32, f32, f32)> = Vec::new();
    let mut loose = 0.0f32;
    for (i, e) in FIELD.iter().enumerate() {
        let mut total = Round::default();
        let mut margins: Vec<f32> = Vec::new();
        let mut paired: Vec<f32> = Vec::new();
        for (k, &seed) in seeds.iter().enumerate() {
            for side in 0..2 {
                let r = duel(store, e, seed, side);
                let m = (r.dealt - r.taken) / per;
                margins.push(m);
                paired.push(m - base[k * 2 + side]);
                total.add(&r);
            }
        }
        let (_, err) = spread(&margins);
        let (mean, perr) = spread(&paired);
        loose += err;
        table.push((i, total.over(margins.len() as f32), mean, perr, err));
    }
    loose /= FIELD.len() as f32;
    let tight = table.iter().map(|t| t.3).sum::<f32>() / FIELD.len() as f32;
    table.sort_by(|a, b| b.2.total_cmp(&a.2));
    println!(
        "{:<15}{:>12}{:>7}{:>7}{:>7}{:>8}{:>6}{:>6}{:>6}{:>6}{:>7}   {}",
        "entrant", "vs house", "dealt", "taken", "kills", "range", "nose", "idle", "stuck", "took", "heard", "roam/hunt/fight/close/break"
    );
    for (i, r, mean, err, _) in &table {
        // Two standard errors of a difference that is paired seed by seed:
        // the conventional bar, and the only difference this instrument is
        // entitled to call one.
        let apart = mean.abs() > 2.0 * err;
        println!(
            "{:<15}{:>7.0} +-{:<3.0}{:>7.0}{:>7.0}{:>7.1}{:>7.1}m{:>5.0}%{:>5.0}%{:>5.0}%{:>6.1}{:>7.1}   {:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}/{:>2.0} {}",
            FIELD[*i].name,
            mean,
            err,
            r.dealt,
            r.taken,
            r.kills,
            r.range,
            r.nose * 100.0,
            r.idle * 100.0,
            r.stalled * 100.0,
            r.took,
            r.heards,
            r.acts[0] * 100.0,
            r.acts[1] * 100.0,
            r.acts[2] * 100.0,
            r.acts[3] * 100.0,
            r.acts[4] * 100.0,
            if apart { "<-" } else { "" }
        );
    }
    println!(
        "\nvs house is damage dealt less damage taken, a minute, against the shipping\nbot, less what the shipping bot itself managed on that same seed and side --\n{} duels an entrant ({} seeds x both spawn slots), and the control's own\nmargin was {:+.0} +-{:.0}. Differencing seed by seed is what makes the errors\nbelievable rather than merely small: paired they average +-{:.0} a minute here\nagainst +-{:.0} taken flat. An arrow marks an entrant two errors clear of the\nshipping bot; everything unmarked is this instrument saying it cannot tell.",
        seeds.len() * 2,
        seeds.len(),
        b_mean,
        b_err,
        tight,
        loose
    );
    // What it would take to believe the leader, which is the question the
    // old table answered by implication and got wrong. A difference counts
    // at two standard errors, and an error shrinks as the root of the
    // count, so the duels wanted are the square of how many errors short
    // the leader is.
    if let Some((i, _, mean, err, _)) = table.first() {
        let n = (seeds.len() * 2) as f32;
        let sd = err * n.sqrt();
        let need = if mean.abs() > 0.0 { 4.0 * (sd / mean) * (sd / mean) } else { f32::INFINITY };
        println!(
            "\nTelling {} from the shipping bot at this spread takes about {:.0} duels.\nYou ran {:.0}.",
            FIELD[*i].name,
            need.min(99999.0).ceil(),
            n
        );
    }
    println!(
        "
idle is the share of the match pressing no direction at all (riding a lift
does not count), and stuck the share of the pressed ticks that went nowhere:
between them they are what
\"the bots sometimes just stand there\" looks like from inside. The act shares
are where the time went -- a duel in which neither bot spends most of it in
fight or close is a duel they are not really having."
    );
    println!(
        "\nnose is the share of the fight spent inside {NOSE_M:.0} m. A tactic that wins by\nwalking up to the other bot is not a tactic, so a high one is a warning\nabout the entrant above it, not a badge."
    );
}
