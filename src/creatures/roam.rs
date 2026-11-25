//! Herds going somewhere: the first reader of the world survey's graph
//! (notes/WORLD_SURVEY_PLAN.md) and of the live grid.
//!
//! Every couple of seconds each prey herd (a walking group whose species
//! hunts nothing) is given a trip: what it needs now, the survey anchor that
//! answers it best, and a route there along the links a large walker can
//! take. Thirsty at the ends of the day, it heads for a drink anchor; drowsy
//! outside its hours, for cover to bed in; after a bolt, for somewhere quiet
//! away from the trouble; otherwise for the next feed anchor, chosen by its
//! diet (graze for grazers, browse for browsers). Danger counts against a
//! goal and against every link on the way (the live grid's `threat`), and a
//! trip whose way grows dangerous is planned again.
//!
//! The herd is moved only by nudging: members that are standing about are
//! set walking now and then, and herd.rs pulls walkers toward the trip's next
//! waypoint alongside the boid rules. So a feeding herd drifts along its way
//! as it grazes, and a thirsty one walks to the water.
//!
//! Predators (a walking species that hunts) get trips too, a pack or a lone
//! hunter alike. Hungry in its hours, it hunts: it picks the nearest herd of
//! something it hunts (any herd with a mind here, recorded or not), heads
//! for the anchor nearest that herd, and once within `STALK_M` makes for the
//! herd itself. The chase and the kill are walk.rs's, when the two meet near
//! the player. With no quarry it knows of, it prowls the feed anchors,
//! where small prey lives.
//!
//! Herds learn where they are hunted (ANIMAL_SIM C4). Each mind keeps a
//! wariness, -0.3 (used to people) to 1 (badly spooked): a bolt with the
//! player near raises it, any other bolt a little, a member lost while
//! drawn or a kill out of sight more (`trouble`). It fades over about a game
//! day, and quiet time near the player wears it below nothing. It widens
//! every member's flee distance (walk.rs `Critter::wary`), and the places
//! trouble happened (the last few) count against goals and routes, the
//! warier the more.
//!
//! Packs keep to ground of their own: a home where the pack was first
//! thought about, out to `TERRITORY_M`, and no nearer another pack of its
//! kind's home than its own by more than `HESITATE_M` (the band where it
//! hesitates). A quarry that crosses into a neighbour's ground is let go.
//! A pack that has eaten lies up where it ate for `FED_S`, while the herd
//! it took from goes on, and it leaves that herd alone for `SPURN_S` after.
//! One that follows a quarry for `GIVE_UP_S` without a kill gives it up for
//! a while too, so a pack does not shadow one herd for ever.

use super::walk::{Critter, Mood, Sleep};
use super::{Creature, Look, Species};
use crate::anim::life::Act;
use crate::util::FxHashMap as HashMap;
use crate::world::live::{Live, LiveChan};
use crate::world::survey::graph::{Class, Graph, Kind};
use glam::Vec3;

/// Seconds between herd thinks.
const THINK_S: f32 = 2.0;
/// Within this of a waypoint, the herd's middle has passed it, metres.
const PASS_M: f32 = 14.0;
/// Within this of the goal anchor, the herd has arrived, metres.
const ARRIVE_M: f32 = 22.0;
/// Goals are sought this far away at most, metres in a straight line.
const LOOK_M: f32 = 700.0;
/// A start anchor must be this near the herd, metres.
const START_M: f32 = 160.0;
/// How strongly the live threat counts: a link through threat 1 costs this
/// many times its length.
const THREAT_COST: f32 = 4.0;
/// A trip is planned again when the threat on its way gets this high.
const REPLAN_THREAT: f32 = 0.5;
/// Seconds a herd keeps away from where it bolted before feeding again.
const SHAKEN_S: f32 = 60.0;
/// A drowsy herd looks for its bed this near, metres: at dusk it walks
/// some way to a bedding ground rather than lying where it fed. Not far:
/// a night goes by in about four minutes (world/clock.rs), at walking pace.
const REST_M: f32 = 200.0;
/// A herd not at its bed this long after setting off for it, seconds, lies
/// down where it is instead of walking half the night.
const BED_BY_S: f32 = 90.0;
/// What going back to last night's bed is worth, against a fresh site
/// (an anchor's worth runs 0.5 to about 1.5).
const BED_FAVOUR: f32 = 0.6;
/// Wariness: the most and the least, how long it takes to fade by about
/// two thirds (game clock seconds; a game day is about half an hour), and
/// how much quiet time near the player wears off each think.
const WARY_MAX: f32 = 1.0;
const WARY_MIN: f32 = -0.3;
const WARY_FADE_S: f32 = 1800.0;
const WARY_CALM: f32 = 0.002;
/// What raises it: a bolt with the player within `NEAR_PLAYER_M`, any other
/// bolt (a predator, the herd), a member lost while drawn (roster.rs), a
/// kill out of sight.
pub const WARY_BOLT_PLAYER: f32 = 0.12;
const WARY_BOLT: f32 = 0.05;
pub const WARY_LOSS: f32 = 0.35;
const WARY_KILLED: f32 = 0.15;
const NEAR_PLAYER_M: f32 = 80.0;
/// A trouble spot is remembered this long (game clock seconds) and counts
/// against ground within this of it, metres; a herd keeps the last few.
const TROUBLE_S: f32 = 2400.0;
const TROUBLE_M: f32 = 120.0;
const TROUBLES: usize = 4;

/// Newcomers settled since the program started, for `--eco-soak`.
pub static SETTLED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// A newcomer within this of the home it is making for has settled, metres.
const SETTLED_M: f32 = 120.0;
/// A herd that has gone to bed stays down until it is this little drowsy
/// (it went at 0.6), so dusk and dawn do not flicker.
const WAKE: f32 = 0.4;
/// Seconds between drinks once a herd has drunk.
const SLAKED_S: f32 = 900.0;
/// Seconds a pack lies up after a kill before it hunts again.
const FED_S: f32 = 1200.0;
/// A hunting pack picks a quarry this near, and makes straight for it (not
/// its anchor) once it is within `STALK_M`, metres.
const QUARRY_M: f32 = 600.0;
const STALK_M: f32 = 120.0;
/// A hunt's goal anchor is within this of the quarry; once the quarry is
/// further than this from it, the hunt is planned again, metres.
const NEAR_QUARRY_M: f32 = 150.0;
/// A pack not drawn stops this short of its quarry: what happens then is
/// for the player to see (or C3's encounters, not built), metres.
const SHADOW_M: f32 = 25.0;
/// How much faster a pack of two or more moves than a lone hunter while
/// hunting out of sight.
const PACK_PACE: f32 = 1.4;
/// A pack's ground: this far from its home at most, metres.
const TERRITORY_M: f32 = 450.0;
/// Ground this much nearer another pack's home than its own is theirs;
/// inside the band it is shared, metres.
const HESITATE_M: f32 = 40.0;
/// Seconds a pack leaves alone the herd it took a kill from.
const SPURN_S: f32 = 1800.0;
/// Seconds a pack follows one quarry without a kill before it gives up on
/// it (for `SPURN_S / 2`).
const GIVE_UP_S: f32 = 300.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Need {
    Drink,
    Feed,
    Rest,
    /// Away from where it bolted.
    Away,
    /// A predator after its quarry, or prowling for one.
    Hunt,
    /// A newcomer making for the ground it has come to take (roster.rs
    /// `replace`), slowly, a trip at a time.
    Home,
}

impl Need {
    pub fn name(self) -> &'static str {
        match self {
            Need::Drink => "drink",
            Need::Feed => "feed",
            Need::Rest => "rest",
            Need::Away => "away",
            Need::Hunt => "hunt",
            Need::Home => "home",
        }
    }
}

/// One herd's trip.
#[derive(Clone, Debug)]
pub struct Trip {
    pub need: Need,
    /// The goal anchor (index into the survey graph's anchors).
    pub goal: u32,
    /// The way there, waypoints on the ground plane (y 0).
    pub route: Vec<Vec3>,
    /// The waypoint the herd is making for.
    pub at: usize,
    /// When it set off, and when it arrived (game clock).
    pub planned: f32,
    pub arrived: Option<f32>,
    /// Seconds it stays once there.
    pub dwell: f32,
    /// The route's cost when planned, metres with the threat added.
    pub cost: f32,
    /// A hunt's quarry: the herd it is after.
    pub quarry: Option<u32>,
}

#[derive(Default)]
struct Mind {
    trip: Option<Trip>,
    /// When it last bolted, and when it last drank (game clock).
    bolted: Option<f32>,
    drank: Option<f32>,
    /// Where the herd's middle was at the last think.
    centre: Vec3,
    species: usize,
    members: usize,
    /// A predator's pack (its species hunts), and when it last ate.
    hunter: bool,
    ate: Option<f32>,
    /// A pack's home, the middle of its ground: where it was first thought
    /// about.
    home: Option<Vec3>,
    /// Herds it leaves alone, and until when.
    spurned: Vec<(u32, f32)>,
    /// The quarry it is following and since when.
    hunting: Option<(u32, f32)>,
    /// What it needed at the last think.
    need: Option<Need>,
    /// The anchor it last slept at, to go back to.
    bed: Option<u32>,
    /// Bedded where it stands, having nowhere better to go.
    rough: bool,
    /// The home it is making for, a newcomer's (`adopt`).
    settle: Option<Vec3>,
    /// How wary it is, -0.3 to 1 (`trouble`), and where trouble last
    /// happened to it, with when (game clock).
    wary: f32,
    trouble: Vec<(Vec3, f32)>,
    /// When its wariness last faded (game clock).
    faded: f32,
}

#[derive(Default)]
pub struct Roam {
    herds: HashMap<u32, Mind>,
    /// Every herd and pack with a mind, as of the last think: (group,
    /// middle, species), for hunters to pick a quarry from among the species
    /// they hunt.
    prey: Vec<(u32, Vec3, usize)>,
    /// Every pack's home: (group, home, species), for working out whose
    /// ground is whose.
    dens: Vec<(u32, Vec3, usize)>,
    since: f32,
    /// The large walker's links by anchor: (link, other end).
    adj: Vec<Vec<(u32, u32)>>,
    /// Which graph `adj` was built for (its link count; 0 for none).
    built_for: usize,
}

impl Roam {
    /// The next waypoint of every herd on its way somewhere, for herd.rs:
    /// for a pack stalking its quarry, the quarry's middle.
    pub fn waypoints(&self) -> HashMap<u32, Vec3> {
        self.herds
            .iter()
            .filter_map(|(g, m)| {
                let t = m.trip.as_ref()?;
                if let Some(q) = self.stalking(m) {
                    return Some((*g, q));
                }
                (t.arrived.is_none() && t.at < t.route.len()).then(|| (*g, t.route[t.at]))
            })
            .collect()
    }

    /// Where a hunting pack's quarry is, when it is near enough to make
    /// straight for.
    fn stalking(&self, m: &Mind) -> Option<Vec3> {
        let t = m.trip.as_ref().filter(|t| t.need == Need::Hunt)?;
        let q = self.prey.iter().find(|p| Some(p.0) == t.quarry)?.1;
        (flat(q - m.centre) < STALK_M).then_some(q)
    }

    /// A hunting pack close enough on its quarry to try for a kill out of
    /// sight (roster.rs `encounters`): the quarry and how far off its middle
    /// is, metres.
    pub fn striking(&self, g: u32) -> Option<(u32, f32)> {
        let m = self.herds.get(&g).filter(|m| m.hunter)?;
        let t = m.trip.as_ref().filter(|t| t.need == Need::Hunt)?;
        let q = self.prey.iter().find(|p| Some(p.0) == t.quarry)?;
        Some((q.0, flat(q.1 - m.centre)))
    }

    /// Pack `pack` made a kill out of sight at `at`, from herd `quarry`: the
    /// pack is fed and lies up there, leaving that herd alone a while, and
    /// the herd runs (it is shaken, `Need::Away`, as after a bolt).
    pub fn killed(&mut self, pack: u32, quarry: u32, at: Vec3, clock: f32) {
        if let Some(m) = self.herds.get_mut(&pack) {
            m.ate = Some(clock);
            m.spurned.push((quarry, clock + SPURN_S));
            m.hunting = None;
            m.trip = None;
            m.centre = Vec3::new(at.x, 0.0, at.z);
        }
        if let Some(m) = self.herds.get_mut(&quarry) {
            m.bolted = Some(clock);
            m.trip = None;
        }
        self.trouble(quarry, at, WARY_KILLED, clock);
    }

    /// Trouble came to herd `g` at `at`: it grows warier by `how` and
    /// remembers the spot.
    pub fn trouble(&mut self, g: u32, at: Vec3, how: f32, clock: f32) {
        let m = self.herds.entry(g).or_default();
        m.wary = (m.wary.max(0.0) + how).min(WARY_MAX);
        m.trouble.retain(|t| clock - t.1 < TROUBLE_S);
        if m.trouble.len() >= TROUBLES {
            m.trouble.remove(0);
        }
        m.trouble.push((Vec3::new(at.x, 0.0, at.z), clock));
        log(g, clock, &format!("herd: trouble at {:.0}, {:.0}, wary {:.2}", at.x, at.z, m.wary));
    }

    /// How wary herd `g` is (-0.3 to 1).
    pub fn wary_of(&self, g: u32) -> f32 {
        self.herds.get(&g).map_or(0.0, |m| m.wary)
    }

    /// Set herd `g`'s wariness, read back from a save (roster.rs).
    pub fn set_wary(&mut self, g: u32, w: f32) {
        self.herds.entry(g).or_default().wary = w.clamp(WARY_MIN, WARY_MAX);
    }

    /// Herd `g` is a newcomer come to take the ground round `home`: it makes
    /// for it and keeps it as its own once there.
    pub fn adopt(&mut self, g: u32, home: Vec3) {
        let m = self.herds.entry(g).or_default();
        m.settle = Some(Vec3::new(home.x, 0.0, home.z));
    }

    /// When pack `g` last ate (the game clock).
    pub fn ate_at(&self, g: u32) -> Option<f32> {
        self.herds.get(&g)?.ate
    }

    /// Where herd `g` is in its night, for its members (walk.rs `Sleep`).
    pub fn sleep_of(&self, g: u32) -> Sleep {
        self.herds.get(&g).map_or(Sleep::Free, sleep_state)
    }

    /// Seconds a drawn member of pack `g` should go before it hunts again:
    /// what is left of the pack's rest after its last meal.
    pub fn fed_left(&self, g: u32, clock: f32) -> f32 {
        self.herds.get(&g).and_then(|m| m.ate).map_or(0.0, |t| (FED_S - (clock - t)).max(0.0))
    }

    /// Every herd with a trip: its middle, its size and the trip, for the
    /// map and the dump.
    pub fn trips(&self) -> impl Iterator<Item = (u32, Vec3, usize, &Trip)> {
        self.herds.iter().filter_map(|(g, m)| m.trip.as_ref().map(|t| (*g, m.centre, m.members, t)))
    }

    /// The trip of the herd `group`, if it has one.
    pub fn trip(&self, group: u32) -> Option<&Trip> {
        self.herds.get(&group)?.trip.as_ref()
    }
}

/// One think's worth for every prey herd: what it needs, where that is, how
/// to get there. Runs every `THINK_S`.
#[allow(clippy::too_many_arguments)]
pub fn update(roam: &mut Roam, world: &mut hecs::World, species: &[Species], graph: Option<&Graph>, live: &Live, player: Vec3, hour: f32, clock: f32, dt: f32) {
    roam.since += dt;
    if roam.since < THINK_S {
        return;
    }
    roam.since = 0.0;
    let Some(graph) = graph else {
        roam.herds.clear();
        return;
    };
    roam.links_for(graph);

    // The herds as they stand: middle, size, species, whether one of them
    // is running, and whether one of them has just eaten.
    let mut seen: HashMap<u32, (Vec3, usize, usize, bool, bool)> = HashMap::default();
    for (kin, crit, look, c) in world.query::<(&super::herd::Kin, &Critter, &Look, &Creature)>().without::<&super::zoo::Kept>().iter() {
        if species.get(c.species).is_none() {
            continue;
        }
        let e = seen.entry(kin.group).or_insert((Vec3::ZERO, 0, c.species, false, false));
        e.0 += look.pos;
        e.1 += 1;
        e.3 |= crit.mood == Mood::Flee;
        e.4 |= matches!(crit.mood, Mood::Feed | Mood::Eat);
    }
    // A herd's mind goes with its animals, except a recorded herd's
    // (roster.rs), which carries on while it is not drawn.
    roam.herds.retain(|g, _| seen.contains_key(g) || super::roster::is_record(*g));
    // The prey herds, before anyone thinks: drawn ones where they stand, the
    // rest where their records have got to.
    for (&g, &(sum, n, sp, ..)) in &seen {
        if species[sp].hunts.is_empty() && n >= 2 {
            let m = roam.herds.entry(g).or_default();
            m.centre = Vec3::new(sum.x / n as f32, 0.0, sum.z / n as f32);
        }
    }
    // Hunters are quarry too, for whatever hunts them (a velociraptor pack
    // after compsognathus, which hunts lizards): a pack only ever picks a
    // species it `hunts`, so its own kind and other hunters stay off its list.
    roam.prey = roam.herds.iter().filter(|(_, m)| m.members > 0).map(|(g, m)| (*g, m.centre, m.species)).collect();
    roam.dens = roam.herds.iter().filter_map(|(g, m)| Some((*g, m.home.filter(|_| m.hunter)?, m.species))).collect();
    let adj = &roam.adj;
    let prey = &roam.prey;
    let dens = &roam.dens;
    let mut notes: Vec<(u32, String)> = Vec::new();
    for (&g, &(sum, n, sp, running, eating)) in &seen {
        let hunter = !species[sp].hunts.is_empty();
        // A lone hunter goes about on its own; a lone grazer is a straggler
        // left by the spawner and has no trips.
        if n < 2 && !hunter {
            continue;
        }
        let centre = Vec3::new(sum.x / n as f32, 0.0, sum.z / n as f32);
        let m = roam.herds.entry(g).or_default();
        m.centre = centre;
        m.species = sp;
        m.members = n;
        m.hunter = hunter;
        if eating {
            // A new meal: the herd it came from is left alone a while.
            if m.ate.is_none_or(|t| clock - t > 60.0) {
                if let Some(q) = m.trip.as_ref().and_then(|t| t.quarry) {
                    m.spurned.push((q, clock + SPURN_S));
                }
            }
            m.ate = Some(clock);
        }
        let near = flat(player - centre) < NEAR_PLAYER_M;
        if running {
            // A new bolt (not one already going): trouble, worse with the
            // player close.
            if m.bolted.is_none_or(|t| clock - t > 10.0) {
                let how = if near { WARY_BOLT_PLAYER } else { WARY_BOLT };
                roam_trouble(m, g, centre, how, clock);
            }
            m.bolted = Some(clock);
            m.trip = None;
            continue;
        }
        // Quiet time with the player close by: it gets used to them.
        if near && live.value(LiveChan::Threat, centre.x, centre.z) < 0.3 {
            m.wary = (m.wary - WARY_CALM).max(WARY_MIN);
        }
        if let Some(note) = think(m, g, &species[sp], centre, graph, adj, live, prey, dens, hour, clock) {
            log(g, clock, &note);
            notes.push((g, note));
        }
    }

    // Nudge: members standing about are set walking now and then while their
    // herd is on its way, more often for water or away from trouble than
    // while feeding along.
    let going: HashMap<u32, (Vec3, f32, bool)> = roam
        .herds
        .iter()
        .filter_map(|(g, m)| {
            let t = m.trip.as_ref()?;
            let to = *t.route.get(t.at)?;
            let p = if t.need == Need::Feed { 0.3 } else { 0.6 };
            t.arrived.is_none().then_some((*g, (to, p, t.need != Need::Feed)))
        })
        .collect();
    let mut going = going;
    for (g, m) in &roam.herds {
        if let Some(q) = roam.stalking(m) {
            going.insert(*g, (q, 0.6, true));
        }
    }
    // A pack that has eaten is full to the last member, so none of them
    // hunts again on its own clock (walk.rs `Critter::fed`) while it lies up.
    let full: HashMap<u32, f32> = roam.herds.keys().map(|g| (*g, roam.fed_left(*g, clock))).filter(|(_, f)| *f > 0.0).collect();
    let nights: HashMap<u32, Sleep> = roam.herds.iter().map(|(g, m)| (*g, sleep_state(m))).collect();
    let wariness: HashMap<u32, f32> = roam.herds.iter().map(|(g, m)| (*g, m.wary)).collect();
    for (kin, crit, look, anim, insp) in world
        .query_mut::<(&super::herd::Kin, &mut Critter, &Look, Option<&mut crate::anim::Anim>, Option<&mut super::inspect::Inspect>)>()
        .without::<&super::zoo::Kept>()
    {
        if let (Some(i), Some((_, l))) = (insp, notes.iter().find(|n| n.0 == kin.group)) {
            i.why(l.clone());
        }
        if let Some(&f) = full.get(&kin.group) {
            crit.fed = crit.fed.max(f);
        }
        crit.sleep = nights.get(&kin.group).copied().unwrap_or_default();
        crit.wary = wariness.get(&kin.group).copied().unwrap_or_default();
        let Some(&(to, p, urgent)) = going.get(&kin.group) else { continue };
        let mut life = anim.and_then(|a| a.life.as_mut());
        let lying = life.as_ref().is_some_and(|l| l.act == Act::Lie);
        // A herd bedded down gets up for anything but feeding along, a few
        // at a time: at dawn the herd rises over a minute or so, not at once.
        if crit.mood == Mood::Idle && (!lying || urgent) && crit.alarm < 0.2 && crit.rng.unit() < if lying { 0.15 } else { p } {
            let d = to - look.pos;
            crit.mood = Mood::Walk;
            crit.heading = d.x.atan2(d.z) + (crit.rng.unit() - 0.5) * 0.5;
            crit.timer = 3.0 + crit.rng.unit() * 4.0;
            if let Some(l) = life.as_mut() {
                l.act = Act::Walk;
            }
        }
    }
}

/// What herd `g` (its mind `m`, of species `s`, its middle at `centre`)
/// needs now and whether to plan its trip again; advances its trip past the
/// waypoints it has reached. A note for the inspector when something changed.
#[allow(clippy::too_many_arguments)]
fn think(m: &mut Mind, g: u32, s: &Species, centre: Vec3, graph: &Graph, adj: &[Vec<(u32, u32)>], live: &Live, prey: &[(u32, Vec3, usize)], dens: &[(u32, Vec3, usize)], hour: f32, clock: f32) -> Option<String> {
    // Wariness fades back toward nothing; old trouble is forgotten.
    let since = (clock - m.faded).clamp(0.0, 600.0);
    m.faded = clock;
    m.wary *= (-since / WARY_FADE_S).exp();
    m.trouble.retain(|t| clock - t.1 < TROUBLE_S);
    let thirst = super::walk::thirst(hour);
    let drowsy = super::diel::drowsy(s.beh.period(), hour);
    let shaken = m.bolted.is_some_and(|t| clock - t < SHAKEN_S);
    let slaked = m.drank.is_some_and(|t| clock - t < SLAKED_S);
    let fed = m.ate.is_some_and(|t| clock - t < FED_S);
    // Water first at the ends of the day, then bed down. A predator that
    // has eaten lies up before anything but running; out of its hours it
    // beds down; otherwise it hunts.
    let need = if shaken {
        Need::Away
    } else if m.hunter && fed {
        Need::Rest
    } else if m.settle.is_some_and(|h| flat(h - centre) > SETTLED_M) {
        // A newcomer in country it does not know keeps going, day and
        // night, until it is on its new ground.
        Need::Home
    } else if thirst >= 0.75 && !slaked {
        Need::Drink
    } else if drowsy > 0.6 || (drowsy > WAKE && m.need == Some(Need::Rest)) {
        Need::Rest
    } else if m.hunter {
        Need::Hunt
    } else {
        Need::Feed
    };
    // Arrived: the ground is its own now.
    let settled = m.settle.filter(|h| flat(*h - centre) <= SETTLED_M);
    if let Some(h) = settled {
        m.settle = None;
        m.home = Some(h);
        log(g, clock, &format!("{}: settled into its new ground at {:.0}, {:.0}", super::roster::kind(s), h.x, h.z));
        SETTLED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if m.need != Some(need) {
        m.rough = false;
    }
    m.need = Some(need);
    let home = *m.home.get_or_insert(centre);
    let who = super::roster::kind(s);
    let mut note = settled.map(|_| format!("{who}: settled into its new ground"));
    // Full: it lies up where it ate, and goes nowhere until it is thirsty,
    // out of its hours or hungry again.
    if m.hunter && fed && need == Need::Rest {
        if m.trip.take().is_some() {
            note = Some(format!("{who}: fed, lying up where it ate"));
        }
        m.hunting = None;
        return note;
    }
    // A pack's own ground: near its home, and not nearer another's (of its
    // kind) by more than the band where it hesitates.
    let me = m.species;
    let ours = |p: Vec3| on_ground(p, g, home, me, dens);
    m.spurned.retain(|sp| sp.1 > clock);
    // Followed too long without a kill: give it up for a while.
    if let Some((q, since)) = m.hunting {
        if clock - since > GIVE_UP_S {
            m.spurned.push((q, clock + SPURN_S * 0.5));
            m.hunting = None;
            note = Some(format!("{who}: gave up on herd {q:x} after {:.0} s", clock - since));
        }
    }
    let spurned = &m.spurned;
    let left = |q: u32| spurned.iter().any(|sp| sp.0 == q);
    // The quarry: the one it is after while that herd is still about and on
    // its ground, else the nearest herd of something it hunts there.
    let was = m.trip.as_ref().and_then(|t| t.quarry);
    let kept = was.and_then(|q| prey.iter().find(|p| p.0 == q)).filter(|p| !left(p.0) && flat(p.1 - centre) < QUARRY_M * 1.2 && ours(p.1));
    if let (Some(q), None, Need::Hunt) = (was, kept, need) {
        if note.is_none() && prey.iter().find(|p| p.0 == q).is_some_and(|p| !ours(p.1)) {
            note = Some(format!("{who}: let herd {q:x} go, it crossed off its ground"));
        }
    }
    let quarry = if need != Need::Hunt {
        None
    } else {
        kept.or_else(|| {
            prey.iter()
                .filter(|p| s.hunts.contains(&p.2) && !left(p.0) && flat(p.1 - centre) < QUARRY_M && ours(p.1))
                .min_by(|a, b| flat(a.1 - centre).total_cmp(&flat(b.1 - centre)))
        })
    };
    m.hunting = match (quarry, m.hunting) {
        (Some(q), Some(h)) if h.0 == q.0 => Some(h),
        (Some(q), _) => Some((q.0, clock)),
        (None, _) => None,
    };
    // Moving along: pass waypoints, arrive, stay a while.
    if let Some(t) = m.trip.as_mut() {
        while t.at < t.route.len() && flat(t.route[t.at] - centre) < PASS_M {
            t.at += 1;
        }
        let goal = &graph.anchors[t.goal as usize];
        if t.arrived.is_none() && flat(Vec3::new(goal.pos[0], 0.0, goal.pos[2]) - centre) < ARRIVE_M {
            t.arrived = Some(clock);
            t.at = t.route.len();
            if t.need == Need::Drink {
                m.drank = Some(clock);
            }
            note = Some(if t.need == Need::Rest {
                m.bed = Some(t.goal);
                format!("{who}: bedded down at the {} anchor", goal.kind.name())
            } else {
                format!("{who}: at the {} anchor, staying {:.0} s", goal.kind.name(), t.dwell)
            });
        }
    }
    // Too long getting to bed: it lies where it is.
    if need == Need::Rest && m.trip.as_ref().is_some_and(|t| t.need == Need::Rest && t.arrived.is_none() && clock - t.planned > BED_BY_S) {
        m.trip = None;
        m.rough = true;
        note = Some(format!("{who}: not at its bed after {BED_BY_S:.0} s, lying where it is"));
    }
    // Plan again when there is no trip, the need changed, the stay is over,
    // the way has grown dangerous, or it has taken far too long.
    let why = match m.trip.as_ref() {
        // Bedded where it stood: it stays there till morning.
        None if need == Need::Rest && m.rough => None,
        None => Some("no trip"),
        Some(t) if t.need != need && !(t.need == Need::Drink && t.arrived.is_none() && need == Need::Feed) => Some("need changed"),
        Some(t) if t.arrived.is_some_and(|a| clock - a > t.dwell) => Some("stay over"),
        Some(t) if !m.hunter && t.arrived.is_none() && way_threat(t, live) > REPLAN_THREAT => Some("danger on the way"),
        Some(t) if t.arrived.is_none() && clock - t.planned > 300.0 + t.cost / 0.3 => Some("taking too long"),
        Some(t) if t.need == Need::Hunt && t.quarry != quarry.map(|q| q.0) => Some("new quarry"),
        Some(t) if t.need == Need::Hunt && quarry.is_some_and(|q| { let a = &graph.anchors[t.goal as usize]; flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - q.1) > NEAR_QUARRY_M }) => Some("quarry moved"),
        Some(t) if t.need == Need::Home && t.arrived.is_some() => Some("on toward its new ground"),
        _ => None,
    };
    if let Some(why) = why {
        // A hunt may go back to the anchor it left: the quarry is what it
        // follows.
        // A bed may be the one it left (last night's), and a hunt goes where
        // its quarry is.
        let skip = m.trip.as_ref().filter(|_| matches!(need, Need::Feed | Need::Drink | Need::Away)).map(|t| t.goal);
        let grazer = s.beh.movement == "graze";
        // A pack hunts, prowls and lies up on its own ground; water it takes
        // where it finds it.
        let within = (m.hunter && matches!(need, Need::Hunt | Need::Rest)).then_some((home, TERRITORY_M));
        let fear = if m.hunter { 0.0 } else { 1.0 };
        let bed = Bed { open: open_bedder(s), last: m.bed };
        let avoid: Vec<(Vec3, f32)> = m.trouble.iter().map(|t| (t.0, 0.5 + m.wary.max(0.0))).collect();
        // A newcomer aims at its new home as a hunt aims at its quarry.
        let toward = if need == Need::Home { m.settle } else { quarry.map(|q| q.1) };
        m.trip = plan(graph, adj, live, grazer, need, centre, skip, g, clock, toward, within, fear, bed, &avoid);
        // Nowhere to bed: it lies where it is.
        m.rough = need == Need::Rest && m.trip.is_none();
        if let Some(t) = m.trip.as_mut() {
            t.quarry = quarry.map(|q| q.0);
        }
        // No way nearer its new home on the links (it came in on a scrap of
        // network by the edge): it goes straight across country.
        if let (Need::Home, None, Some(h)) = (need, m.trip.as_ref(), m.settle) {
            m.trip = straight(graph, centre, h, clock);
        }
        // No water it can get to: it goes without, as if it had drunk, and
        // gets on with the next need instead of asking again every think.
        if need == Need::Drink && m.trip.is_none() {
            m.drank = Some(clock);
        }
        let after = quarry.map_or(String::new(), |q| format!(" after herd {:x} {:.0} m off", q.0, flat(q.1 - centre)));
        note = Some(match &m.trip {
            Some(t) => {
                let a = &graph.anchors[t.goal as usize];
                format!("{who}: {}{after} ({why}): {} anchor {:.0} m by {} waypoints", need.name(), a.kind.name(), t.cost, t.route.len())
            }
            None if need == Need::Rest => format!("{who}: rest ({why}): no bed in reach, lying where it is"),
            None => format!("{who}: {}{after} ({why}): nowhere to go", need.name()),
        });
    }
    note
}

/// How fast a herd that is not drawn moves along its trip, metres a second:
/// a feeding herd drifts as it grazes, one after water or away from trouble
/// walks.
fn unseen_speed(need: Need) -> f32 {
    match need {
        Need::Feed => 0.25,
        Need::Rest => 0.4,
        Need::Drink | Need::Away | Need::Hunt => 0.8,
        // Slowly: it is a long way through country it does not know.
        Need::Home => 0.35,
    }
}

impl Roam {
    /// The large walker's links by anchor, built again when the graph changes.
    fn links_for(&mut self, graph: &Graph) {
        if self.built_for != graph.links.len() || self.adj.len() != graph.anchors.len() {
            self.adj = vec![Vec::new(); graph.anchors.len()];
            for (i, l) in graph.links.iter().enumerate().filter(|(_, l)| l.class == Class::Large) {
                self.adj[l.a as usize].push((i as u32, l.b));
                self.adj[l.b as usize].push((i as u32, l.a));
            }
            self.built_for = graph.links.len();
        }
    }

    /// A recorded herd that is not drawn (roster.rs), for `dt` seconds: it
    /// thinks as a drawn one does and its middle moves along its trip.
    #[allow(clippy::too_many_arguments)]
    pub fn unseen(&mut self, g: u32, s: usize, species: &[Species], members: usize, centre: &mut Vec3, graph: &Graph, live: &Live, hour: f32, clock: f32, dt: f32) {
        self.links_for(graph);
        let m = self.herds.entry(g).or_default();
        m.species = s;
        m.members = members;
        m.centre = *centre;
        m.hunter = !species[s].hunts.is_empty();
        if let Some(note) = think(m, g, &species[s], *centre, graph, &self.adj, live, &self.prey, &self.dens, hour, clock) {
            log(g, clock, &note);
        }
        // A pack near its quarry shadows it, short of it.
        let m = &self.herds[&g];
        if let Some(q) = self.stalking(m) {
            let to = q - *centre;
            let d = flat(to);
            let pace = if super::roster::packs(&species[s]) { PACK_PACE } else { 1.0 };
            let step = (unseen_speed(Need::Hunt) * pace * dt).min((d - SHADOW_M).max(0.0));
            if d > 0.1 {
                *centre += Vec3::new(to.x, 0.0, to.z) / d * step;
            }
            self.herds.get_mut(&g).unwrap().centre = *centre;
            return;
        }
        let m = self.herds.get_mut(&g).unwrap();
        let Some(t) = m.trip.as_ref().filter(|t| t.arrived.is_none()) else { return };
        // A pack hunts as one and covers ground faster than a lone hunter.
        let pace = if t.need == Need::Hunt && super::roster::packs(&species[s]) { PACK_PACE } else { 1.0 };
        let mut step = unseen_speed(t.need) * pace * dt;
        let mut at = t.at;
        while step > 0.0 && at < t.route.len() {
            let to = t.route[at] - *centre;
            let d = flat(to);
            if d <= step {
                *centre = Vec3::new(t.route[at].x, 0.0, t.route[at].z);
                step -= d;
                at += 1;
            } else {
                *centre += Vec3::new(to.x, 0.0, to.z) / d * step;
                step = 0.0;
            }
        }
        m.centre = *centre;
    }

    /// Forget herd `g` (a record that is gone).
    pub fn forget(&mut self, g: u32) {
        self.herds.remove(&g);
    }
}

/// A trip straight across country from `from` to `to`, waypoints every 40 m,
/// its goal the anchor nearest `to`: a newcomer's way home where the links
/// do not go.
fn straight(graph: &Graph, from: Vec3, to: Vec3, clock: f32) -> Option<Trip> {
    let goal = graph.anchors.iter().enumerate().min_by(|a, b| flat(Vec3::new(a.1.pos[0], 0.0, a.1.pos[2]) - to).total_cmp(&flat(Vec3::new(b.1.pos[0], 0.0, b.1.pos[2]) - to)))?.0;
    let d = flat(to - from);
    let n = (d / 40.0).ceil().max(1.0) as usize;
    let route = (1..=n).map(|k| from + (to - from) * (k as f32 / n as f32)).map(|p| Vec3::new(p.x, 0.0, p.z)).collect();
    Some(Trip { need: Need::Home, goal: goal as u32, route, at: 0, planned: clock, arrived: None, dwell: 20.0, cost: d, quarry: None })
}

/// `Roam::trouble` on a mind already borrowed.
fn roam_trouble(m: &mut Mind, g: u32, at: Vec3, how: f32, clock: f32) {
    m.wary = (m.wary.max(0.0) + how).min(WARY_MAX);
    m.trouble.retain(|t| clock - t.1 < TROUBLE_S);
    if m.trouble.len() >= TROUBLES {
        m.trouble.remove(0);
    }
    m.trouble.push((Vec3::new(at.x, 0.0, at.z), clock));
    log(g, clock, &format!("herd: bolted at {:.0}, {:.0}, wary {:.2}", at.x, at.z, m.wary));
}

/// What a herd wants of a bed (`plan`).
#[derive(Clone, Copy, Default)]
struct Bed {
    /// It sleeps in the open with a long view (horses, bison, pronghorn),
    /// not in cover (deer, elk, the hunters).
    open: bool,
    /// The anchor it slept at last.
    last: Option<u32>,
}

/// Whether species `s` beds in the open: a grazer that keeps the day's
/// hours (a crepuscular deer lies up in cover).
fn open_bedder(s: &Species) -> bool {
    s.beh.movement == "graze" && s.hunts.is_empty() && s.beh.period() != super::diel::Period::Crepuscular
}

/// Where a herd is in its night: on its way to bed, bedded (at its bed,
/// lying where it stands, or a pack lying up after a kill), or neither.
fn sleep_state(m: &Mind) -> Sleep {
    match (m.need, m.trip.as_ref()) {
        (Some(Need::Rest), Some(t)) if t.arrived.is_none() => Sleep::Going,
        (Some(Need::Rest), _) => Sleep::Bedded,
        _ => Sleep::Free,
    }
}

/// Whether `p` is on the ground of pack `g` (home `home`, species `sp`):
/// within `TERRITORY_M` of home, and not nearer another pack of its kind's
/// home by more than `HESITATE_M`.
fn on_ground(p: Vec3, g: u32, home: Vec3, sp: usize, dens: &[(u32, Vec3, usize)]) -> bool {
    let d = flat(p - home);
    d < TERRITORY_M && !dens.iter().any(|o| o.0 != g && o.2 == sp && flat(p - o.1) + HESITATE_M < d)
}

/// Dev hook: `VG_ROAM_LOG=1` prints every herd's and pack's decisions,
/// drawn or not, to stderr.
fn log(g: u32, clock: f32, note: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("VG_ROAM_LOG").is_some()) {
        eprintln!("roam {clock:.0} {g:x} {note}");
    }
}

fn flat(v: Vec3) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

/// The highest threat on the rest of a trip's way.
fn way_threat(t: &Trip, live: &Live) -> f32 {
    t.route[t.at.min(t.route.len())..].iter().map(|p| live.value(LiveChan::Threat, p.x, p.z)).fold(0.0, f32::max)
}

/// The best goal for `need` and the way there, or None.
#[allow(clippy::too_many_arguments)]
fn plan(graph: &Graph, adj: &[Vec<(u32, u32)>], live: &Live, grazer: bool, need: Need, centre: Vec3, skip: Option<u32>, group: u32, clock: f32, quarry: Option<Vec3>, within: Option<(Vec3, f32)>, fear: f32, bed: Bed, avoid: &[(Vec3, f32)]) -> Option<Trip> {
    // The live threat, scaled by how much it matters to this body (a
    // predator's, `fear` 0, is mostly prey bolting, which it made), and the
    // places trouble came to it lately, by how wary it is.
    let threat_at = |x: f32, z: f32| {
        let remembered: f32 = avoid.iter().map(|(p, w)| w * (1.0 - flat(Vec3::new(x, 0.0, z) - *p) / TROUBLE_M).max(0.0)).sum();
        live.value(LiveChan::Threat, x, z) * fear + remembered
    };
    // Where a body walks from: the nearest anchor with large links.
    let start = graph
        .anchors
        .iter()
        .enumerate()
        .filter(|(i, _)| !adj[*i].is_empty())
        .map(|(i, a)| (i, flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - centre)))
        .filter(|(_, d)| *d <= START_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    // Dijkstra over the large links, each costing its length, more where it
    // runs through danger.
    let n = graph.anchors.len();
    let mut cost = vec![f32::INFINITY; n];
    let mut via: Vec<Option<(u32, u32)>> = vec![None; n];
    let mut heap = std::collections::BinaryHeap::new();
    cost[start.0] = start.1;
    heap.push(std::cmp::Reverse((Ord32(start.1), start.0)));
    while let Some(std::cmp::Reverse((Ord32(c), a))) = heap.pop() {
        if c > cost[a] || c > LOOK_M * 2.5 {
            continue;
        }
        for &(li, b) in &adj[a] {
            let l = &graph.links[li as usize];
            let mid = l.path[l.path.len() / 2];
            let danger = threat_at(mid[0], mid[1]).max(threat_at(graph.anchors[b as usize].pos[0], graph.anchors[b as usize].pos[2]));
            let nc = c + l.length * (1.0 + THREAT_COST * danger.min(2.0));
            if nc < cost[b as usize] {
                cost[b as usize] = nc;
                via[b as usize] = Some((li, a as u32));
                heap.push(std::cmp::Reverse((Ord32(nc), b as usize)));
            }
        }
    }
    let jitter = |i: usize| (crate::world::noise::hash32(i as u32 ^ group.wrapping_mul(2_654_435_761) ^ (clock / 60.0) as u32) & 0xffff) as f32 / 65535.0 * 0.15;
    let mut best: Option<(f32, usize)> = None;
    for (i, a) in graph.anchors.iter().enumerate() {
        if Some(i as u32) == skip || !cost[i].is_finite() {
            continue;
        }
        let d = flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - centre);
        if d > LOOK_M || within.is_some_and(|(h, r)| flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - h) > r) {
            continue;
        }
        let worth = match need {
            Need::Drink if a.kind == Kind::Drink => 1.0,
            Need::Feed if a.kind == Kind::Feed => {
                if grazer {
                    a.graze
                } else {
                    a.graze.max(a.browse)
                }
            }
            // A bed: open ground with a long view for a grazer of the open
            // country, cover for the rest; last night's is worth more.
            Need::Rest if !a.roofed && matches!(a.kind, Kind::Refuge | Kind::Feed | Kind::Focal | Kind::Vantage) => {
                let site = if bed.open {
                    if a.open < 0.5 {
                        continue;
                    }
                    0.5 + 0.5 * a.open + 0.5 * (a.sightline / 120.0).min(1.0)
                } else {
                    if a.kind != Kind::Refuge && a.cover < 0.35 {
                        continue;
                    }
                    0.5 + a.cover
                };
                site + if bed.last == Some(i as u32) { BED_FAVOUR } else { 0.0 }
            }
            // Away: quiet ground with food, the further the better.
            Need::Away if matches!(a.kind, Kind::Feed | Kind::Refuge) && !a.roofed => 0.5 + d / LOOK_M,
            // A hunt: the anchor nearest the quarry, whatever its kind.
            Need::Hunt if quarry.is_some() && !a.roofed => {
                let q = quarry.unwrap_or_default();
                let near = flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - q);
                if near > NEAR_QUARRY_M {
                    continue;
                }
                2.0 - near / NEAR_QUARRY_M * 1.5
            }
            // A newcomer: the anchor nearest its new home that it can see
            // from here, so it closes on it a trip at a time.
            Need::Home if !a.roofed => {
                let home = quarry.unwrap_or(centre);
                let left = flat(Vec3::new(a.pos[0], 0.0, a.pos[2]) - home);
                // Only somewhere that gets it nearer.
                if left > flat(centre - home) - 30.0 {
                    continue;
                }
                2.0 - left / LOOK_M
            }
            // No quarry known: prowl the feed anchors, where small prey is.
            Need::Hunt if quarry.is_none() && a.kind == Kind::Feed => a.graze.max(a.browse),
            _ => continue,
        };
        // Nothing worth walking to right beside the herd for food, and a
        // drowsy herd beds in cover close by or where it stands.
        if (matches!(need, Need::Feed) || (need == Need::Hunt && quarry.is_none())) && d < 40.0 || (need == Need::Rest && d > REST_M) {
            continue;
        }
        let score = worth - cost[i] / 600.0 - 2.0 * threat_at(a.pos[0], a.pos[2]) + jitter(i);
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, i));
        }
    }
    let (_, goal) = best?;
    // The way: link paths from the start to the goal, each the right way
    // round, as waypoints.
    let mut legs: Vec<(u32, u32)> = Vec::new();
    let mut at = goal;
    while let Some((li, from)) = via[at] {
        legs.push((li, at as u32));
        at = from as usize;
    }
    legs.reverse();
    let sa = &graph.anchors[start.0];
    let mut route = vec![Vec3::new(sa.pos[0], 0.0, sa.pos[2])];
    for (li, to) in legs {
        let l = &graph.links[li as usize];
        let forward = l.b == to;
        let pts: Vec<[f32; 2]> = if forward { l.path.clone() } else { l.path.iter().rev().copied().collect() };
        route.extend(pts.into_iter().skip(1).map(|p| Vec3::new(p[0], 0.0, p[1])));
    }
    let ga = &graph.anchors[goal];
    route.push(Vec3::new(ga.pos[0], 0.0, ga.pos[2]));
    // Waypoints closer than 10 m to the one before are dropped.
    let mut thin: Vec<Vec3> = Vec::with_capacity(route.len());
    for p in route {
        if thin.last().is_none_or(|q| flat(p - *q) >= 10.0) {
            thin.push(p);
        }
    }
    // Skip what is already behind the herd: from the waypoint nearest to it.
    let first = thin.iter().enumerate().min_by(|a, b| flat(*a.1 - centre).total_cmp(&flat(*b.1 - centre))).map_or(0, |(i, _)| i);
    let dwell = match need {
        Need::Drink => 90.0,
        Need::Feed => 240.0 + (goal as f32 * 37.0) % 240.0,
        // A bed is kept till the herd wakes (`WAKE`), not for a time.
        Need::Rest => f32::INFINITY,
        Need::Away => 30.0,
        Need::Hunt => 45.0,
        Need::Home => 20.0,
    };
    Some(Trip { need, goal: goal as u32, route: thin, at: first, planned: clock, arrived: None, dwell, cost: cost[goal], quarry: None })
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Ord32(f32);
impl Eq for Ord32 {}
impl Ord for Ord32 {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::survey::graph::{Anchor, Link};

    fn anchor(kind: Kind, x: f32, z: f32, graze: f32) -> Anchor {
        Anchor {
            kind,
            pos: [x, 0.0, z],
            facing: [1.0, 0.0],
            zone: 0,
            above_floor: 0.0,
            open: 1.0,
            sightline: 100.0,
            ways_in: 8,
            flat: 10.0,
            cover: 0.0,
            roofed: false,
            drink: 100.0,
            graze,
            browse: 0.0,
        }
    }

    fn link(a: u32, b: u32, pa: [f32; 2], pb: [f32; 2]) -> Link {
        let length = ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt();
        Link { a, b, class: Class::Large, length, narrowest: 50.0, cover: 0.0, pinch: false, path: vec![pa, [(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5], pb], narrow_at: None }
    }

    /// A herd at the start with two feed anchors as good as each other, one
    /// east and one west, each at the end of its own link.
    fn two_ways() -> (Graph, Vec<Vec<(u32, u32)>>) {
        let anchors = vec![anchor(Kind::Focal, 200.0, 200.0, 0.0), anchor(Kind::Feed, 350.0, 200.0, 0.9), anchor(Kind::Feed, 50.0, 200.0, 0.9)];
        let links = vec![link(0, 1, [200.0, 200.0], [350.0, 200.0]), link(0, 2, [200.0, 200.0], [50.0, 200.0])];
        let mut adj = vec![Vec::new(); 3];
        for (i, l) in links.iter().enumerate() {
            adj[l.a as usize].push((i as u32, l.b));
            adj[l.b as usize].push((i as u32, l.a));
        }
        (Graph { zones: Vec::new(), zone_of: Vec::new(), anchors, links }, adj)
    }

    #[test]
    fn a_herd_feeds_away_from_danger() {
        let (g, adj) = two_ways();
        let centre = Vec3::new(200.0, 0.0, 200.0);
        for (danger_x, want) in [(350.0, 2), (50.0, 1)] {
            let mut live = Live::new(400.0);
            // Danger over one feed anchor and the way to it.
            for x in [danger_x, (danger_x + 200.0) * 0.5] {
                live.stamp(LiveChan::Threat, Vec3::new(x, 0.0, 200.0), 1.0, 40.0);
            }
            assert!(live.due(crate::world::live::TICK_S));
            live.tick(glam::Vec2::ZERO);
            let t = plan(&g, &adj, &live, true, Need::Feed, centre, None, 7, 0.0, None, None, 1.0, Bed::default(), &[]).expect("a trip");
            assert_eq!(t.goal, want, "danger at x {danger_x}");
            assert!(t.route.len() >= 2);
        }
    }

    #[test]
    fn a_herd_beds_by_its_kind_and_goes_back_to_last_nights_bed() {
        let (mut g, adj) = two_ways();
        // Where it stands: shut in, no bed. East: open grass with a long
        // view. West: a thicket.
        g.anchors[0].open = 0.3;
        g.anchors[1].sightline = 150.0;
        g.anchors[2].kind = Kind::Refuge;
        g.anchors[2].open = 0.2;
        g.anchors[2].cover = 0.8;
        let live = Live::new(400.0);
        let centre = Vec3::new(200.0, 0.0, 200.0);
        let bed_for = |open: bool, last: Option<u32>, g: &Graph| plan(g, &adj, &live, true, Need::Rest, centre, None, 7, 0.0, None, None, 1.0, Bed { open, last }, &[]).expect("a bed").goal;
        assert_eq!(bed_for(true, None, &g), 1, "a horse beds in the open");
        assert_eq!(bed_for(false, None, &g), 2, "a deer beds in cover");
        // Two open beds alike: last night's wins.
        g.anchors[2] = g.anchors[1].clone();
        g.anchors[2].pos = [50.0, 0.0, 200.0];
        assert_eq!(bed_for(true, Some(2), &g), 2);
        assert_eq!(bed_for(true, Some(1), &g), 1);
        let t = plan(&g, &adj, &live, true, Need::Rest, centre, None, 7, 0.0, None, None, 1.0, Bed { open: true, last: None }, &[]).unwrap();
        assert!(t.dwell.is_infinite(), "a bed is kept till morning");
    }

    #[test]
    fn a_wary_herd_feeds_away_from_where_trouble_was() {
        let (g, adj) = two_ways();
        let live = Live::new(400.0);
        let centre = Vec3::new(200.0, 0.0, 200.0);
        for (spot_x, want) in [(350.0, 2), (50.0, 1)] {
            let avoid = [(Vec3::new(spot_x, 0.0, 200.0), 1.2)];
            let t = plan(&g, &adj, &live, true, Need::Feed, centre, None, 7, 0.0, None, None, 1.0, Bed::default(), &avoid).expect("a trip");
            assert_eq!(t.goal, want, "trouble at x {spot_x}");
        }
    }

    #[test]
    fn a_pack_keeps_to_its_own_ground() {
        // Two lion prides 400 m apart on x, and a hyena clan between them.
        let (a, b) = (Vec3::new(100.0, 0.0, 0.0), Vec3::new(500.0, 0.0, 0.0));
        let dens = [(1, a, 7), (2, b, 7), (3, Vec3::new(300.0, 0.0, 0.0), 8)];
        let at = |x: f32| Vec3::new(x, 0.0, 0.0);
        // Its own side, and the shared band either side of the middle.
        assert!(on_ground(at(150.0), 1, a, 7, &dens));
        assert!(on_ground(at(310.0), 1, a, 7, &dens));
        assert!(on_ground(at(290.0), 2, b, 7, &dens));
        // Over the band: the neighbour's. The hyenas' home doesn't count.
        assert!(!on_ground(at(330.0), 1, a, 7, &dens));
        assert!(!on_ground(at(260.0), 2, b, 7, &dens));
        // Too far from home, with no neighbour that way.
        assert!(!on_ground(at(-400.0), 1, a, 7, &dens));
    }

    #[test]
    fn a_pack_hunts_toward_its_quarry() {
        let (g, adj) = two_ways();
        let live = Live::new(400.0);
        let centre = Vec3::new(200.0, 0.0, 200.0);
        for (qx, want) in [(340.0, 1), (60.0, 2)] {
            let t = plan(&g, &adj, &live, false, Need::Hunt, centre, None, 9, 0.0, Some(Vec3::new(qx, 0.0, 210.0)), None, 0.0, Bed::default(), &[]).expect("a hunt");
            assert_eq!(t.goal, want, "quarry at x {qx}");
        }
    }

    #[test]
    fn a_herd_drinks_at_a_drink_anchor_and_skips_the_one_it_left() {
        let (mut g, adj) = two_ways();
        g.anchors[1].kind = Kind::Drink;
        g.anchors[2].kind = Kind::Drink;
        let live = Live::new(400.0);
        let centre = Vec3::new(200.0, 0.0, 200.0);
        let first = plan(&g, &adj, &live, true, Need::Drink, centre, None, 7, 0.0, None, None, 1.0, Bed::default(), &[]).expect("a trip").goal;
        let second = plan(&g, &adj, &live, true, Need::Drink, centre, Some(first), 7, 0.0, None, None, 1.0, Bed::default(), &[]).expect("a trip").goal;
        assert!(first != 0 && second != 0 && first != second);
    }
}
