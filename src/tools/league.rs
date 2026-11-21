//! A league: five bots at one difficulty, tuned differently, in one match.
//!
//! The tournament (`tools::tourney`) answers "is this change worth anything"
//! by differencing one bot against a control. That is the right instrument
//! for a margin and the wrong one for the complaint this exists for, which
//! is about what a bot looks like from the other end of a gun: it wanders
//! with no purpose, walks backwards for no reason, stands about, moves
//! sideways faster and more often than a person can, and ignores the floor.
//! None of those is a margin. Each is a rate, and a rate can be counted.
//!
//! So the league plays the game as played -- five bots, free for all, the
//! difficulty's own aim, on an imported deathmatch map -- and counts:
//!
//! - **objectives**: guns picked up (and how long after spawning), health
//!   taken while hurt, and whether a lost enemy is found again;
//! - **movement**: time walking backwards with nobody in sight, two-second
//!   stretches with nobody in sight spent going nowhere (`lost`, which is
//!   "stands there confused"), dodges and sidestep reversals a minute
//!   against what a player does, and time pressing no key at all.
//!
//! The entrants differ by `BotDef::tune` (`brain::duel::Tuning`), which is
//! the point of having five of them: a code change moves every row, and the
//! spread between the rows says which way each knob wants to go. Every
//! entrant plays every slot on every seed, so where it spawned cancels out.

use super::tourney::{open, Field};
use crate::cli::Args;
use crate::player::{MoveInput, Player};
use crate::shooter::Shooter;
use crate::world::fields::{BotDef, ShooterDef};
use crate::world::store::Store;
use crate::{worldload, Vec3};

/// The difficulty every entrant plays at.
const SKILL: &str = "skilled";

/// One entrant: a name and the numbers set on it.
struct Entrant {
    name: &'static str,
    tune: &'static [(&'static str, f32)],
}

/// The field. `house` is the shipping bot; each other row pulls one knob a
/// long way, so the table says which direction each knob wants to move.
const FIELD: &[Entrant] = &[
    Entrant { name: "house", tune: &[] },
    Entrant { name: "calm", tune: &[("dodge", 0.5), ("weave", 3.2)] },
    Entrant { name: "hunter", tune: &[("hunt", 6.0)] },
    Entrant { name: "scavenger", tune: &[("greed", 2.0)] },
    Entrant { name: "medic", tune: &[("nerve", 0.2)] },
];

/// A player's limits, for comparison: running pace, the boost over it, and
/// a dodge (`player.rs`). Anything over `DODGING_MS` is a dodge.
const RUN_MS: f32 = 8.3;
const BOOST_MS: f32 = 8.3 * 1.15;
const DODGING_MS: f32 = 10.6;
/// What a practised player does a minute in a deathmatch, near enough: a
/// dodge every ten seconds or so, a sidestep reversal every five.
const DODGE_PM_OK: f32 = 6.0;
const FLIP_PM_OK: f32 = 12.0;
/// A sidestep reversal is the other side key going down within this long of
/// the first coming up.
const FLIP_GAP_S: f32 = 0.3;
/// Window for "going nowhere", and how little it has to cover to count.
const LOST_WINDOW_S: f32 = 2.0;
const LOST_M: f32 = 1.5;
/// Below this share of health it is hurt and ought to want a health pad.
const HURT: f32 = 0.5;
/// A lost enemy found again inside this long counts as a pursuit that worked.
const CHASE_S: f32 = 6.0;
/// The squares a map is cut into for "rooms visited": 8 m across and 3 m
/// of height, so a floor above is another room.
const AREA_M: f32 = 8.0;
const AREA_UP_M: f32 = 3.0;
/// Pads nearer each other than this are one pickup spot.
const SPOT_M: f32 = 2.5;

#[derive(Default, Clone, Copy)]
struct Tally {
    alive_s: f32,
    kills: f32,
    deaths: f32,
    dealt: f32,
    /// Guns it did not have, picked up; seconds from spawn to the first of
    /// them, summed over the lives that found one; lives in all.
    guns: f32,
    arm_s: f32,
    armed_lives: f32,
    lives: f32,
    /// Health taken while under `HURT`, armour taken, health taken at all.
    mends: f32,
    armour: f32,
    /// Hurt spells, and how they ended.
    hurt: f32,
    hurt_mended: f32,
    hurt_died: f32,
    mend_s: f32,
    /// Lost sight of the enemy; found them again inside `CHASE_S`.
    lost_foe: f32,
    found_foe: f32,
    /// Seconds on the ground moving with nobody in sight, and of those, going
    /// backwards; the same while somebody is in sight.
    calm_s: f32,
    calm_back_s: f32,
    fight_s: f32,
    fight_back_s: f32,
    /// Windows with nobody in sight, and the ones that went nowhere.
    windows: f32,
    lost_windows: f32,
    /// Executed dodges, sidestep reversals, seconds pressing nothing.
    dodges: f32,
    /// Dodges the brain asked for, as opposed to taken.
    meant: f32,
    /// Shells seen coming and stepped off the line of; rounds fired.
    evades: f32,
    shots: f32,
    flips: f32,
    idle_s: f32,
    /// Seconds moving on the ground, and flat metres covered in them.
    run_s: f32,
    run_m: f32,
    /// Seconds over the boost, not counting dodges.
    over_s: f32,
    acts: [f32; 6],
    /// Coverage, per bot per match: nav nodes come within reach of, of how
    /// many; 8 m squares of the graph stood in, of how many; distinct pads
    /// taken from, of how many. Summed over bots, so divide by `bots`.
    nodes: f32,
    nodes_of: f32,
    areas: f32,
    areas_of: f32,
    pads: f32,
    pads_of: f32,
    /// Pads that were worth something to this bot at some moment.
    wanted: f32,
    /// Pad spots stood on, whatever they gave.
    stood: f32,
    /// The same by spot: pads within `SPOT_M` of each other are one place.
    spots: f32,
    spots_took: f32,
    spots_stood: f32,
    /// Health, armour and guns carried, integrated over time alive.
    hp_s: f32,
    armour_s: f32,
    guns_s: f32,
    bots: f32,
    hurt_acts: [f32; 6],
    hurt_seen_s: f32,
    packs_up: f32,
    hurt_calm_s: f32,
    no_pack_s: f32,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        macro_rules! sum {
            ($($f:ident),*) => { $(self.$f += o.$f;)* };
        }
        sum!(nodes, nodes_of, areas, areas_of, pads, pads_of, wanted, stood, spots, spots_took, spots_stood, hp_s, armour_s, guns_s, bots, alive_s, kills, deaths, dealt, guns, arm_s, armed_lives, lives, mends, armour, hurt, hurt_mended, hurt_died, mend_s, lost_foe, found_foe, calm_s, calm_back_s, fight_s, fight_back_s, windows, lost_windows, dodges, meant, evades, shots, flips, idle_s, run_s, run_m, over_s);
        for (a, b) in self.acts.iter_mut().zip(o.acts) {
            *a += b;
        }
        for (a, b) in self.hurt_acts.iter_mut().zip(o.hurt_acts) {
            *a += b;
        }
        self.hurt_seen_s += o.hurt_seen_s;
        self.packs_up += o.packs_up;
        self.hurt_calm_s += o.hurt_calm_s;
        self.no_pack_s += o.no_pack_s;
    }

    fn per_min(&self, x: f32) -> f32 {
        x / (self.alive_s / 60.0).max(1e-3)
    }

    fn share(x: f32, of: f32) -> f32 {
        if of > 0.0 {
            x / of
        } else {
            0.0
        }
    }

    fn back(&self) -> f32 {
        Tally::share(self.calm_back_s, self.calm_s)
    }
    fn lost(&self) -> f32 {
        Tally::share(self.lost_windows, self.windows)
    }
    fn idle(&self) -> f32 {
        Tally::share(self.idle_s, self.alive_s)
    }
    fn dodge_pm(&self) -> f32 {
        self.per_min(self.dodges)
    }
    fn flip_pm(&self) -> f32 {
        self.per_min(self.flips)
    }

    /// The three halves of the score, each a minute of being alive.
    ///
    /// Fighting: a kill is worth more than a death costs, because a bot that
    /// never engages never dies either, and damage counts so that a
    /// near-kill is not nothing.
    fn fight_pts(&self) -> f32 {
        self.per_min(8.0 * self.kills - 6.0 * self.deaths + self.dealt / 25.0)
    }
    /// Objectives: arming up, mending when hurt, finding a lost enemy again.
    fn goal_pts(&self) -> f32 {
        self.per_min(3.0 * self.guns + 4.0 * self.mends + 1.5 * self.armour + 2.0 * self.found_foe)
    }
    /// Moving like a person: each percent of the bad kinds of time costs,
    /// and dodges and reversals past what a player does cost per extra one.
    fn move_pts(&self) -> f32 {
        -(0.4 * self.back() * 100.0
            + 0.5 * self.lost() * 100.0
            + 0.3 * self.idle() * 100.0
            + 1.0 * (self.dodge_pm() - DODGE_PM_OK).max(0.0)
            + 0.3 * (self.flip_pm() - FLIP_PM_OK).max(0.0))
    }
    fn score(&self) -> f32 {
        self.fight_pts() + self.goal_pts() + self.move_pts()
    }
}

/// What the loop keeps about one bot between steps.
#[derive(Default)]
struct Watch {
    alive: bool,
    health: f32,
    armour: f32,
    guns: usize,
    life_t: f32,
    armed: bool,
    hurt_since: Option<f32>,
    seen: bool,
    lost_at: Option<(usize, f32)>,
    side: i8,
    side_up: f32,
    side_from: f32,
    dodging: bool,
    win_t: f32,
    win_from: Vec3,
    win_seen: bool,
}

fn bot_def(e: &Entrant, slot: usize) -> BotDef {
    BotDef {
        name: format!("{} {}", e.name, slot),
        skill: SKILL.into(),
        colour: None,
        bold: 0.0,
        steady: 0.0,
        // LEAGUE_WITHOUT=evade,dodge switches parts off for every entrant: the
        // way to value one, since in a mirror match it moves the whole field.
        // LEAGUE_WITHOUT_FOR=house limits that to one entrant: the paired test.
        without: if std::env::var("LEAGUE_WITHOUT_FOR").map_or(true, |w| w == e.name) {
            std::env::var("LEAGUE_WITHOUT").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
        } else {
            Vec::new()
        },
        tune: e.tune.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        team: None,
    }
}

fn act_index(name: &str) -> Option<usize> {
    ["roam", "hunt", "fight", "close", "break", "fall"].iter().position(|&a| a == name)
}

/// One match of the whole field; returns each entrant's tally, by entrant.
fn play(f: &Field, base: &ShooterDef, store: &mut Store, order: &[usize], seconds: f32, seed: u32) -> Vec<Tally> {
    let mut rules = base.clone();
    // LEAGUE_STASH=0 plays the old rule, where ammunition for a gun not
    // carried is refused.
    if std::env::var("LEAGUE_STASH").is_ok_and(|v| v == "0") {
        rules.ammo_without_gun = false;
    }
    rules.bots = order.iter().enumerate().map(|(slot, &e)| bot_def(&FIELD[e], slot)).collect();
    rules.target_score = 0;
    let mut sh = Shooter::repeatable(&f.land.lib, &rules, f.home, store, seed);
    let plats = crate::platforms::Platforms::new(&f.platform_defs, &f.platforms, 0);
    let gates = crate::portals::Portals::new(&f.land.def.portals, |_| {});
    let mut parts = crate::world_parts::WorldParts::new(plats, gates, f.home.0);
    sh.bench_player();
    let mut player = Player::new(f.home.0, f.home.1);
    let mut ents = crate::entities::Entities::new(Default::default(), 1);
    let mut actions = crate::game::Actions::default();
    let input = MoveInput::default();
    let mut shot = false;
    let step = crate::game::STEP;
    let steps = (seconds / step) as usize;
    let n = order.len();
    let mut tally = vec![Tally::default(); n];
    let mut w: Vec<Watch> = (0..n).map(|_| Watch::default()).collect();
    let mut t = 0.0f32;
    let mut seen_nodes: Vec<std::collections::HashSet<usize>> = vec![Default::default(); n];
    let mut seen_areas: Vec<std::collections::HashSet<(i32, i32, i32)>> = vec![Default::default(); n];
    let mut took_pads: Vec<std::collections::HashSet<usize>> = vec![Default::default(); n];
    let mut pad_was: Vec<bool> = Vec::new();
    let mut wanted: Vec<std::collections::HashSet<usize>> = vec![Default::default(); n];
    let mut stood: Vec<std::collections::HashSet<usize>> = vec![Default::default(); n];
    let area = |p: Vec3| ((p.x / AREA_M).floor() as i32, (p.y / AREA_UP_M).floor() as i32, (p.z / AREA_M).floor() as i32);
    for _ in 0..steps {
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
            edits: &mut Vec::new(),
        };
        crate::modes::Activity::frame(&mut sh, &mut cx, &input, &mut actions, step);
        crate::modes::Activity::step(&mut sh, cx, &input);
        t += step;
        // The graph takes a moment to settle and the bots are stood on real
        // ground once it has; nothing before that is play.
        if sh.nav().is_empty() {
            continue;
        }
        // Pads that went this step, to whoever is standing on them.
        let ready: Vec<bool> = sh.pads().iter().map(|p| p.ready()).collect();
        if pad_was.len() == ready.len() {
            for (pi, p) in sh.pads().iter().enumerate() {
                if pad_was[pi] && !ready[pi] {
                    let by = sh.bots.iter().enumerate().filter(|(_, b)| b.fighter.alive()).min_by(|a, b| a.1.pos().distance(p.at).total_cmp(&b.1.pos().distance(p.at)));
                    if let Some((bi, b)) = by {
                        if b.pos().distance(p.at) < 3.0 {
                            took_pads[bi].insert(pi);
                        }
                    }
                }
            }
        }
        pad_was = ready;
        if (t * 60.0) as u32 % 30 == 0 {
            for (i, b) in sh.bots.iter().enumerate() {
                if b.fighter.alive() {
                    for (pi, p) in sh.pads().iter().enumerate() {
                        if p.good(&b.fighter) > 0.0 {
                            wanted[i].insert(pi);
                        }
                    }
                }
            }
        }
        for (i, b) in sh.bots.iter().enumerate() {
            if b.fighter.alive() {
                for (pi, p) in sh.pads().iter().enumerate() {
                    if Vec3::new(p.at.x - b.pos().x, 0.0, p.at.z - b.pos().z).length() < 1.3 && (p.at.y - b.pos().y).abs() < 1.5 {
                        stood[i].insert(pi);
                    }
                }
            }
        }
        for (i, b) in sh.bots.iter().enumerate() {
            if b.fighter.alive() {
                let at = b.pos();
                for (ni, nd) in sh.nav().nodes.iter().enumerate() {
                    if Vec3::new(nd.at.x - at.x, 0.0, nd.at.z - at.z).length() < 3.0 && (nd.at.y - at.y).abs() < 2.5 {
                        seen_nodes[i].insert(ni);
                        seen_areas[i].insert(area(nd.at));
                    }
                }
            }
            let (s, k) = (&mut tally[i], &mut w[i]);
            let alive = b.fighter.alive();
            let health = b.fighter.health;
            let armour = b.fighter.armour;
            let guns = b.fighter.arsenal.guns.iter().filter(|g| g.have).count();
            let (foe, _, _, seen_n) = b.brain.aiming_at();
            let seen = seen_n > 0;
            let full = b.fighter.full_health.max(1.0);
            if alive && !k.alive {
                // A life begins.
                s.lives += 1.0;
                k.life_t = t;
                k.armed = false;
                k.hurt_since = None;
                k.lost_at = None;
                k.win_t = 0.0;
                k.win_from = b.pos();
                k.win_seen = false;
            } else if !alive && k.alive {
                if std::env::var("LEAGUE_ARM").is_ok() {
                    eprintln!("died armed{} after {:.1} s guns {} {}", k.armed as u8, t - k.life_t, guns, b.brain.note());
                }
                if let Some(_since) = k.hurt_since.take() {
                    s.hurt_died += 1.0;
                }
            } else if alive {
                if guns > k.guns {
                    s.guns += (guns - k.guns) as f32;
                    if !k.armed {
                        k.armed = true;
                        s.armed_lives += 1.0;
                        s.arm_s += t - k.life_t;
                    }
                }
                if health > k.health + 0.5 {
                    if k.health < full * HURT {
                        s.mends += 1.0;
                    }
                    // A spell is mended by the first pack taken in it: the
                    // objective is going and getting health, and a 25-point
                    // pack from 15% does not reach half.
                    if let Some(since) = k.hurt_since.take() {
                        s.hurt_mended += 1.0;
                        s.mend_s += t - since;
                    }
                }
                if armour > k.armour + 0.5 {
                    s.armour += 1.0;
                }
            }
            k.alive = alive;
            k.health = health;
            k.armour = armour;
            k.guns = guns;
            if !alive {
                continue;
            }
            s.alive_s += step;
            s.hp_s += health * step;
            s.armour_s += armour * step;
            s.guns_s += guns as f32 * step;
            if health < full * HURT && k.hurt_since.is_none() {
                k.hurt_since = Some(t);
                s.hurt += 1.0;
            }
            // Pursuit: sight of the enemy lost, then found again.
            if k.seen && !seen {
                if let Some(f) = foe {
                    k.lost_at = Some((f, t));
                    s.lost_foe += 1.0;
                }
            }
            if let Some((who, at)) = k.lost_at {
                if seen && foe == Some(who) {
                    s.found_foe += 1.0;
                    k.lost_at = None;
                } else if t - at > CHASE_S {
                    k.lost_at = None;
                }
            }
            k.seen = seen;
            if let Some(a) = act_index(b.brain.doing()) {
                s.acts[a] += step;
                if k.hurt_since.is_some() && std::env::var("LEAGUE_HURT").is_ok() && !seen && ((t * 60.0) as u32) % 30 == 0 {
                    let sc = b.brain.scouting();
                    eprintln!("hurt {} hp {:.0} want {:.2} blocked {} lures {} reach {}/{} {}", b.brain.doing(), health, sc.want, sc.blocked, sc.lures, sc.reach, sc.nodes, b.brain.note());
                }
                if k.hurt_since.is_some() && !seen {
                    let up = sh.pads().iter().filter(|p| p.kind == crate::shooter::pickup::Kind::Health && p.ready()).count();
                    s.packs_up += up as f32 * step;
                    s.hurt_calm_s += step;
                    if up == 0 {
                        s.no_pack_s += step;
                    }
                }
                if k.hurt_since.is_some() {
                    s.hurt_acts[a] += step;
                    if seen {
                        s.hurt_seen_s += step;
                    }
                }
            }
            // Movement.
            let v = b.vel();
            let flat = Vec3::new(v.x, 0.0, v.z);
            let speed = flat.length();
            let mv = b.pilot.mv;
            let keys = mv.forward || mv.back || mv.left || mv.right;
            if !keys {
                s.idle_s += step;
                if std::env::var("LEAGUE_IDLE").is_ok() {
                    eprintln!("idle {} seen{} ground{} spd {:.1} {}", b.brain.doing(), seen as u8, b.body.body.on_ground as u8, speed, b.brain.note());
                }
            }
            let dodging = speed > DODGING_MS;
            if dodging && !k.dodging {
                s.dodges += 1.0;
            }
            k.dodging = dodging;
            let on_ground = b.body.body.on_ground;
            if on_ground && speed > 2.0 {
                let facing = Vec3::new(-b.body.yaw.sin(), 0.0, -b.body.yaw.cos());
                let back = flat.dot(facing) / speed < -0.5;
                if back && !seen && std::env::var("LEAGUE_DIAG").is_ok() && i == 0 {
                    let f = facing;
                    let r = Vec3::new(-f.z, 0.0, f.x);
                    eprintln!(
                        "t {t:6.2} {:5} keys f{} b{} l{} r{} spr{} vel f{:+.1} r{:+.1} ground{} hp {:.0}",
                        b.brain.doing(), mv.forward as u8, mv.back as u8, mv.left as u8, mv.right as u8, mv.sprint as u8,
                        flat.dot(f), flat.dot(r), on_ground as u8, health
                    );
                }
                if seen {
                    s.fight_s += step;
                    if back {
                        s.fight_back_s += step;
                    }
                } else {
                    s.calm_s += step;
                    if back {
                        s.calm_back_s += step;
                    }
                }
                if keys {
                    s.run_s += step;
                    s.run_m += speed * step;
                    if speed > BOOST_MS + 0.3 && !dodging {
                        s.over_s += step;
                    }
                }
            }
            let side: i8 = (mv.right as i8) - (mv.left as i8);
            if side != 0 {
                if k.side != 0 && side != k.side && t - k.side_up < FLIP_GAP_S {
                    s.flips += 1.0;
                    if std::env::var("LEAGUE_GAP").is_ok() && i == 0 {
                        eprintln!("gap {:.2} held {:.2}", t - k.side_up, t - k.side_from);
                    }
                    if std::env::var("LEAGUE_FLIP").is_ok() && i == 0 {
                        eprintln!("flip {} seen{} f{} b{} hp{:.0} at {:.1},{:.1},{:.1} {}", b.brain.doing(), seen as u8, mv.forward as u8, mv.back as u8, health, b.pos().x, b.pos().y, b.pos().z, b.brain.note());
                    }
                }
                if side != k.side {
                    k.side_from = t;
                }
                k.side = side;
                k.side_up = t;
            }
            // Going nowhere with nobody about.
            k.win_t += step;
            k.win_seen |= seen;
            if k.win_t >= LOST_WINDOW_S {
                if !k.win_seen {
                    s.windows += 1.0;
                    if b.pos().distance(k.win_from) < LOST_M {
                        s.lost_windows += 1.0;
                        if std::env::var("LEAGUE_LOST").is_ok() {
                            eprintln!("lost {} at {:.1},{:.1},{:.1} keys{} idle-ish speed {:.1} jams {} {}", b.brain.doing(), b.pos().x, b.pos().y, b.pos().z, keys as u8, speed, b.brain.jams(), b.brain.note());
                            if std::env::var("LEAGUE_NODES").is_ok() {
                                for (j, nd) in sh.nav().nodes.iter().enumerate() {
                                    if Vec3::new(nd.at.x - b.pos().x, 0.0, nd.at.z - b.pos().z).length() < 4.0 {
                                        eprintln!("   node {j} at {:.1},{:.1},{:.1}", nd.at.x, nd.at.y, nd.at.z);
                                    }
                                }
                            }
                        }
                    }
                }
                k.win_t = 0.0;
                k.win_from = b.pos();
                k.win_seen = false;
            }
        }
    }
    if std::env::var("LEAGUE_PADS").is_ok() {
        for (pi, p) in sh.pads().iter().enumerate() {
            let by = stood.iter().filter(|s| s.contains(&pi)).count();
            let near = sh.nav().nodes.iter().map(|nd| (nd.at.distance(p.at), nd.at.y)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap_or((f32::MAX, 0.0));
            eprintln!("pad {pi} {:?} at {:.1},{:.1},{:.1} stood_by {by} node {:.1} m (dy {:.1})", p.kind, p.at.x, p.at.y, p.at.z, near.0, p.at.y - near.1);
        }
    }
    // Pads in a row are one place: join each to the first pad within reach.
    let pads = sh.pads();
    let mut spot_of: Vec<usize> = (0..pads.len()).collect();
    for a in 0..pads.len() {
        for b in 0..a {
            if pads[a].at.distance(pads[b].at) < SPOT_M {
                spot_of[a] = spot_of[b];
                break;
            }
        }
    }
    let spot_count = spot_of.iter().collect::<std::collections::HashSet<_>>().len();
    let all_areas: std::collections::HashSet<(i32, i32, i32)> = sh.nav().nodes.iter().map(|nd| area(nd.at)).collect();
    for i in 0..n {
        tally[i].nodes = seen_nodes[i].len() as f32;
        tally[i].nodes_of = sh.nav().len() as f32;
        tally[i].areas = seen_areas[i].len() as f32;
        tally[i].areas_of = all_areas.len() as f32;
        tally[i].pads = took_pads[i].len() as f32;
        tally[i].pads_of = sh.pads().len() as f32;
        tally[i].wanted = wanted[i].len() as f32;
        tally[i].stood = stood[i].len() as f32;
        tally[i].spots = spot_count as f32;
        tally[i].spots_took = took_pads[i].iter().map(|&p| spot_of[p]).collect::<std::collections::HashSet<_>>().len() as f32;
        tally[i].spots_stood = stood[i].iter().map(|&p| spot_of[p]).collect::<std::collections::HashSet<_>>().len() as f32;
        tally[i].bots = 1.0;
    }
    let scores = sh.scores();
    for (i, b) in sh.bots.iter().enumerate() {
        let sc = scores.get(i + 1).map(|(_, s)| *s).unwrap_or_default();
        tally[i].kills = sc.kills as f32;
        tally[i].deaths = sc.deaths as f32;
        tally[i].dealt = b.fighter.score.dealt;
        tally[i].meant = b.brain.dodges() as f32;
        tally[i].evades = b.brain.evades() as f32;
        tally[i].shots = sc.shots as f32;
    }
    // Back into entrant order.
    let mut out = vec![Tally::default(); FIELD.len()];
    for (slot, &e) in order.iter().enumerate() {
        out[e].add(&tally[slot]);
    }
    out
}

/// The rules with one gun in them: everybody spawns holding it, and every
/// gun or ammunition pad on the map hands out rounds for it instead, so the
/// map keeps its supply in the same places. Health and armour are untouched.
fn one_gun(rules: &ShooterDef, gun: &str) -> ShooterDef {
    let mut r = rules.clone();
    r.weapons = vec![gun.to_string()];
    r.give_all = true;
    for p in &mut r.pickups {
        if p.kind == "weapon" || p.kind == "ammo" {
            p.kind = "ammo".into();
            p.weapon = Some(gun.to_string());
            p.amount = None;
        }
    }
    r
}

/// Every entrant on every seed and slot, summed by entrant.
fn season(field: &Field, rules: &ShooterDef, store: &mut Store, seeds: u32, seconds: f32) -> Vec<Tally> {
    let n = FIELD.len();
    let mut total = vec![Tally::default(); n];
    for k in 0..seeds {
        let seed = field.seed ^ (k + 1).wrapping_mul(0x9e3779b9);
        for r in 0..n {
            let order: Vec<usize> = (0..n).map(|j| (j + r) % n).collect();
            for (e, t) in play(field, rules, store, &order, seconds, seed).iter().enumerate() {
                total[e].add(t);
            }
        }
    }
    total
}

/// One gun at a time: how well the bots use each, the field summed.
fn by_gun(field: &Field, store: &mut Store, which: &str, seeds: u32, seconds: f32) {
    let guns: Vec<String> = if which == "all" { field.rules.weapons.clone() } else { vec![which.to_string()] };
    println!(
        "{}: one gun at a time, {} {SKILL} bots, {seeds} seeds x {} slot rotations, {seconds:.0} s a match\n",
        worldload::selected(),
        FIELD.len(),
        FIELD.len()
    );
    println!(
        "{:<16}{:>7}{:>7}{:>8}{:>8}{:>8}{:>8}{:>7}{:>7}{:>8}",
        "gun", "score", "K/m", "dmg/m", "self/m", "shots/m", "dmg/sh", "evd/m", "lost%", "flips/m"
    );
    for g in &guns {
        let rules = one_gun(&field.rules, g);
        let mut all = Tally::default();
        let rows = season(field, &rules, store, seeds, seconds);
        if guns.len() == 1 {
            for (e, t) in rows.iter().enumerate() {
                println!("  {:<11} K/m {:.2}  D/m {:.2}  dmg/m {:.0}  evd/m {:.1}", FIELD[e].name, t.per_min(t.kills), t.per_min(t.deaths), t.per_min(t.dealt), t.per_min(t.evades));
            }
        }
        for t in rows {
            all.add(&t);
        }
        println!(
            "{:<16}{:>7.1}{:>7.2}{:>8.0}{:>8.2}{:>8.1}{:>8.1}{:>7.1}{:>6.1}%{:>8.1}",
            g,
            all.score(),
            all.per_min(all.kills),
            all.per_min(all.dealt),
            all.per_min((all.deaths - all.kills).max(0.0)),
            all.per_min(all.shots),
            Tally::share(all.dealt, all.shots),
            all.per_min(all.evades),
            all.lost() * 100.0,
            all.flip_pm(),
        );
    }
    println!(
        "\nThe whole field summed: per bot-minute alive. self/m is deaths nobody was\ncredited with -- mostly a bot's own splash or the floor. dmg/sh is damage a\nround fired; evd/m shells seen coming and stepped off the line of."
    );
}

pub fn run(args: &Args) {
    let mut field = match open(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("league: {e}");
            std::process::exit(1);
        }
    };
    let mut store = std::mem::take(&mut field.store);
    let seconds = args.tourney_seconds.unwrap_or(90.0);
    let seeds = args.league.unwrap_or(2).max(1);
    if let Some(w) = &args.league_weapon {
        by_gun(&field, &mut store, w, seeds, seconds);
        return;
    }
    let n = FIELD.len();
    println!(
        "{}: {} nav nodes, {} pads; {} {SKILL} bots, {seeds} seeds x {n} slot rotations, {seconds:.0} s a match",
        worldload::selected(),
        field.rules.nav.nodes.len(),
        field.rules.pickups.len(),
        n
    );
    let began = std::time::Instant::now();
    let rules = field.rules.clone();
    let total = season(&field, &rules, &mut store, seeds, seconds);
    let mut rows: Vec<(usize, Tally)> = total.into_iter().enumerate().collect();
    rows.sort_by(|a, b| b.1.score().total_cmp(&a.1.score()));

    println!("\n-- score a minute alive: fight + objectives + movement --\n");
    println!("{:<11}{:>7}{:>7}{:>7}{:>7}   {:>5}{:>5}{:>7}", "entrant", "score", "fight", "goals", "move", "K/m", "D/m", "dmg/m");
    for (e, t) in &rows {
        println!(
            "{:<11}{:>7.1}{:>7.1}{:>7.1}{:>7.1}   {:>5.2}{:>5.2}{:>7.0}",
            FIELD[*e].name,
            t.score(),
            t.fight_pts(),
            t.goal_pts(),
            t.move_pts(),
            t.per_min(t.kills),
            t.per_min(t.deaths),
            t.per_min(t.dealt)
        );
    }
    println!("\n-- coverage, one bot in one match of {seconds:.0} s --\n");
    println!("{:<11}{:>9}{:>9}{:>9}{:>9}{:>9}", "entrant", "nodes", "areas", "pads", "usable", "stood");
    let mut all = Tally::default();
    for (e, t) in &rows {
        all.add(t);
        println!(
            "{:<11}{:>8.0}%{:>8.0}%{:>8.0}%{:>8.0}%{:>8.0}%",
            FIELD[*e].name,
            Tally::share(t.nodes, t.nodes_of) * 100.0,
            Tally::share(t.areas, t.areas_of) * 100.0,
            Tally::share(t.pads, t.pads_of) * 100.0,
            Tally::share(t.wanted, t.pads_of) * 100.0,
            Tally::share(t.stood, t.pads_of) * 100.0
        );
    }
    println!(
        "{:<11}{:>8.0}%{:>8.0}%{:>8.0}%{:>8.0}%{:>8.0}%   of {:.0} nodes, {:.0} areas, {:.0} pads",
        "all",
        Tally::share(all.nodes, all.nodes_of) * 100.0,
        Tally::share(all.areas, all.areas_of) * 100.0,
        Tally::share(all.pads, all.pads_of) * 100.0,
        Tally::share(all.wanted, all.pads_of) * 100.0,
        Tally::share(all.stood, all.pads_of) * 100.0,
        all.nodes_of / all.bots.max(1.0),
        all.areas_of / all.bots.max(1.0),
        all.pads_of / all.bots.max(1.0)
    );
    println!(
        "by spot (pads within {SPOT_M} m are one): took from {:.0}%, stood on {:.0}% of {:.0}
stock while alive: health {:.0}, armour {:.0}, guns carried {:.1}",
        Tally::share(all.spots_took, all.spots) * 100.0,
        Tally::share(all.spots_stood, all.spots) * 100.0,
        all.spots / all.bots.max(1.0),
        Tally::share(all.hp_s, all.alive_s),
        Tally::share(all.armour_s, all.alive_s),
        Tally::share(all.guns_s, all.alive_s)
    );
    println!("\n-- objectives --\n");
    println!(
        "{:<11}{:>7}{:>8}{:>8}{:>9}{:>8}{:>8}{:>9}{:>8}",
        "entrant", "guns/m", "arm s", "armed%", "mends/m", "hurt", "mended%", "mend s", "refind%"
    );
    for (e, t) in &rows {
        println!(
            "{:<11}{:>7.2}{:>8.1}{:>7.0}%{:>9.2}{:>8.0}{:>7.0}%{:>9.1}{:>7.0}%",
            FIELD[*e].name,
            t.per_min(t.guns),
            Tally::share(t.arm_s, t.armed_lives),
            Tally::share(t.armed_lives, t.lives) * 100.0,
            t.per_min(t.mends),
            t.hurt,
            Tally::share(t.hurt_mended, t.hurt) * 100.0,
            Tally::share(t.mend_s, t.hurt_mended),
            Tally::share(t.found_foe, t.lost_foe) * 100.0,
        );
    }
    println!("\n-- movement (a player runs {RUN_MS} m/s, boosts to {BOOST_MS:.1}, dodges at 12.2) --\n");
    println!(
        "{:<11}{:>7}{:>8}{:>7}{:>7}{:>8}{:>6}{:>8}{:>7}{:>7}   {}",
        "entrant", "back%", "fback%", "lost%", "idle%", "dodge/m", "meant", "flips/m", "m/s", "over%", "roam/hunt/fight/close/break"
    );
    for (e, t) in &rows {
        let a = t.acts.map(|x| Tally::share(x, t.alive_s) * 100.0);
        println!(
            "{:<11}{:>6.1}%{:>7.1}%{:>6.1}%{:>6.1}%{:>8.1}{:>6.1}{:>8.1}{:>7.2}{:>6.1}%   {:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}",
            FIELD[*e].name,
            t.back() * 100.0,
            Tally::share(t.fight_back_s, t.fight_s) * 100.0,
            t.lost() * 100.0,
            t.idle() * 100.0,
            t.dodge_pm(),
            t.per_min(t.meant),
            t.flip_pm(),
            Tally::share(t.run_m, t.run_s),
            Tally::share(t.over_s, t.run_s) * 100.0,
            a[0],
            a[1],
            a[2],
            a[3],
            a[4]
        );
    }
    println!("\n-- while hurt (under {:.0}%) --\n", HURT * 100.0);
    println!("{:<11}{:>8}{:>8}{:>7}{:>8}   {}", "entrant", "hurt s", "seen%", "packs", "none%", "roam/hunt/fight/close/break");
    for (e, t) in &rows {
        let all: f32 = t.hurt_acts.iter().sum();
        let a = t.hurt_acts.map(|x| Tally::share(x, all) * 100.0);
        println!(
            "{:<11}{:>8.1}{:>7.0}%{:>7.1}{:>7.0}%   {:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}/{:>2.0}",
            FIELD[*e].name,
            Tally::share(all, t.hurt),
            Tally::share(t.hurt_seen_s, all) * 100.0,
            Tally::share(t.packs_up, t.hurt_calm_s),
            Tally::share(t.no_pack_s, t.hurt_calm_s) * 100.0,
            a[0],
            a[1],
            a[2],
            a[3],
            a[4]
        );
    }
    println!(
        "\nback% is time moving backwards with nobody in sight (fback% the same in a
fight, where backpedalling while shooting is fair). lost% is two-second
stretches with nobody in sight that covered under {LOST_M} m. over% is time
on the ground faster than a boosting player, dodges aside. {:.0} s.",
        began.elapsed().as_secs_f32()
    );
}
