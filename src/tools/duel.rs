//! A bench for weapons: two bots in an empty room, one gun each.
//!
//! The arsenal grew to ten guns in an afternoon and every number in every
//! weapon file was a guess. A live match on Slagdeck cannot settle any of
//! them: two bots on a map with a pit and three galleries spend most of a
//! round out of each other's sight, and what comes out is as much the map
//! as the gun. Worse, the map hands out all ten weapons, so the only thing
//! a match measures is which gun the bots *chose*.
//!
//! So: the `duelbox` world, which is one empty room, and one weapon per
//! fighter, forced. Two passes.
//!
//! - **The bench.** Both bots carry the same gun. This says how the weapon
//!   behaves -- how often it lands, what a round is worth when it does, how
//!   far away the fight happens, how fast anybody dies.
//! - **The table.** Every gun against every other gun, both ways round so
//!   the spawn is not what wins, scored on net kills. This says which gun
//!   is better, which is a different question and the one that matters.
//!
//! Both bots are the same person with the same aim, so skill cannot be the
//! difference. The player is parked outside the room: a third body standing
//! still in the middle of a weapons bench is a free kill that goes to
//! whichever gun reaches it.
//!
//! No window, no GPU, no wall clock in the loop: the same arguments give the
//! same table every time.
//!
//!     lurelia --world duelbox --duel 2 --duel-seconds 45

use crate::brain::person;
use crate::cli::Args;
use crate::player::{MoveInput, Player};
use crate::shooter::Shooter;
use crate::tools::tourney;
use crate::world::fields::BotDef;
use crate::{worldload, Vec3};

/// Every fighter on the bench is this person: the aim is the same on both
/// sides of every match, so what is left is the gun.
const SKILL: &str = "average";
/// Aim forced to the same value for everyone, the way the tournament's
/// fighting bracket does it.
const AIM: f32 = 0.5;
/// Metres below which a fight counts as nose to nose.
const NOSE_M: f32 = 6.0;

/// What one fighter did in one match.
#[derive(Default, Clone, Copy)]
struct Tally {
    kills: f32,
    deaths: f32,
    shots: f32,
    hits: f32,
    dealt: f32,
    /// Ticks spent aiming at somebody, and the ranges summed over them.
    range_sum: f32,
    range_n: f32,
    nose: f32,
    /// Steps each button was held, so a mode nobody ever picks is visible
    /// rather than hidden inside an average.
    left: f32,
    right: f32,
    /// Rounds sent round a corner off a wall.
    banks: f32,
    /// Seconds of match this came from, so rates can be per minute.
    secs: f32,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.kills += o.kills;
        self.deaths += o.deaths;
        self.shots += o.shots;
        self.hits += o.hits;
        self.dealt += o.dealt;
        self.range_sum += o.range_sum;
        self.range_n += o.range_n;
        self.nose += o.nose;
        self.left += o.left;
        self.right += o.right;
        self.banks += o.banks;
        self.secs += o.secs;
    }
    fn accuracy(&self) -> f32 {
        if self.shots > 0.0 {
            self.hits / self.shots
        } else {
            0.0
        }
    }
    /// Damage put on somebody per round fired: what one trigger pull is
    /// actually worth, which is the figure a weapon file is tuned on.
    fn per_shot(&self) -> f32 {
        if self.shots > 0.0 {
            self.dealt / self.shots
        } else {
            0.0
        }
    }
    fn per_min(&self, v: f32) -> f32 {
        if self.secs > 0.0 {
            v * 60.0 / self.secs
        } else {
            0.0
        }
    }
    fn range(&self) -> f32 {
        if self.range_n > 0.0 {
            self.range_sum / self.range_n
        } else {
            0.0
        }
    }
}

fn bot(name: &str) -> BotDef {
    BotDef {
        name: name.to_string(),
        skill: SKILL.to_string(),
        colour: None,
        bold: 0.0,
        steady: 0.0,
        without: Vec::new(),
        tune: Default::default(),
        team: None,
    }
}

/// One match: bot 1 carries `west`, bot 2 carries `east`. Returns what each
/// of them did.
fn fight(f: &tourney::Field, west: &str, east: &str, seconds: f32, seed: u32) -> (Tally, Tally) {
    let mut rules = f.rules.clone();
    rules.bots = vec![bot("West"), bot("East")];
    rules.target_score = 0;
    rules.give_all = true;
    // On a map with pads, the ones for guns nobody in this match carries are
    // noise twice over: a bot walks to them and picks up nothing, and the
    // walk is time it was not fighting. Health and armour stay, because they
    // are part of what a map is; ammunition stays for the two guns in hand.
    rules.pickups.retain(|p| match p.kind.as_str() {
        "weapon" => false,
        "ammo" => p.weapon.as_deref().is_some_and(|w| w == west || w == east),
        _ => true,
    });
    // A bench, so the same arguments have to give the same bout
    // (`Shooter::repeatable`).
    let mut sh = Shooter::repeatable(&f.land.lib, &rules, f.home, &f.store, seed);
    sh.handicap(AIM);
    sh.arm(1, &f.land.lib, std::slice::from_ref(&west.to_string()), true);
    sh.arm(2, &f.land.lib, std::slice::from_ref(&east.to_string()), true);

    // The player is parked a long way off and put back there every tick.
    // A body standing still in the middle of the room is a free kill, and
    // it would go to whichever gun happened to reach it.
    let away = f.home.0 + Vec3::new(140.0, 0.0, 140.0);
    let mut player = Player::new(away, f.home.1);
    let mut ents = crate::entities::Entities::new(Default::default(), 1);
    let mut actions = crate::game::Actions::default();
    let input = MoveInput::default();
    let mut shot = false;
    let step = crate::game::STEP;
    let steps = (seconds / step) as usize;
    let mut side = [Tally::default(), Tally::default()];
    for _ in 0..steps {
        player.pos = away;
        player.prev_pos = away;
        let mut cx = crate::modes::ActivityCtx {
            player: &mut player,
            store: &f.store,
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
        for (k, t) in side.iter_mut().enumerate() {
            let Some(b) = sh.bots.get(k) else { continue };
            let (_, r, _, _) = b.brain.aiming_at();
            if r.is_finite() && r < 400.0 {
                t.range_sum += r;
                t.range_n += 1.0;
                if r < NOSE_M {
                    t.nose += 1.0;
                }
            }
        }
    }
    let scores = sh.scores();
    for (k, t) in side.iter_mut().enumerate() {
        if let Some((_, s)) = scores.get(k + 1) {
            t.kills = s.kills as f32;
            t.deaths = s.deaths as f32;
            t.shots = s.shots as f32;
            t.hits = s.hits as f32;
            t.dealt = s.dealt;
        }
        if let Some(b) = sh.bots.get(k) {
            let (l, r) = b.brain.triggers();
            t.left = l as f32;
            t.right = r as f32;
            t.banks = b.brain.banks() as f32;
        }
        t.secs = seconds;
    }
    (side[0], side[1])
}

pub fn run(args: &Args) {
    let field = match tourney::open(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("duel: {e}");
            std::process::exit(1);
        }
    };
    if person::level(SKILL).is_none() {
        eprintln!("duel: no difficulty called {SKILL}");
        std::process::exit(1);
    }
    let seconds = args.duel_seconds.unwrap_or(45.0);
    let rounds = args.duel.unwrap_or(2).max(1);
    // Every weapon the world hands out, in slot order, which is how the
    // arsenal is read everywhere else.
    let mut guns: Vec<String> = field.rules.weapons.clone();
    guns.sort_by_key(|n| field.land.lib.weapon(n).map_or(99, |i| field.land.lib.weapons[i].slot));
    if let Some(only) = args.duel_weapon.as_deref() {
        if !guns.iter().any(|g| g == only) {
            eprintln!("duel: {only} is not one of this world's weapons: {guns:?}");
            std::process::exit(1);
        }
    }
    println!("{}: {} guns, {rounds} rounds of {seconds:.0} s, both sides {SKILL} with aim {AIM}", worldload::selected(), guns.len());

    // ---- the bench: the same gun on both sides --------------------------
    println!("\n-- the bench: both bots carrying the same gun --\n");
    println!(
        "{:<17}{:>8}{:>9}{:>9}{:>10}{:>9}{:>8}{:>8}",
        "weapon", "kills/m", "acc", "dmg/shot", "dmg/min", "range", "right", "banked"
    );
    let mut bench: Vec<(String, Tally)> = Vec::new();
    for g in &guns {
        let mut t = Tally::default();
        for r in 0..rounds {
            let seed = field.seed ^ (r as u32 + 1).wrapping_mul(0x9e3779b9);
            let (a, b) = fight(&field, g, g, seconds, seed);
            t.add(&a);
            t.add(&b);
        }
        println!(
            "{:<17}{:>8.1}{:>8.0}%{:>9.1}{:>10.0}{:>8.1}m{:>7.0}%{:>8.0}",
            g,
            t.per_min(t.kills) * 0.5,
            t.accuracy() * 100.0,
            t.per_shot(),
            t.per_min(t.dealt) * 0.5,
            t.range(),
            if t.left + t.right > 0.0 { t.right / (t.left + t.right) * 100.0 } else { 0.0 },
            t.banks
        );
        bench.push((g.clone(), t));
    }

    // ---- the table: every gun against every other -----------------------
    //
    // Every pair is fought both ways round, because the west spawn and the
    // east spawn are not the same place and a gun that wins only from one
    // of them has not won. The matrix is filled first and printed after:
    // a half-built table read as it goes is how a sign ends up backwards.
    println!(
        "
-- the table: net kills, every gun against every other, both ways round --
"
    );
    let n = guns.len();
    let mut grid = vec![vec![f32::NAN; n]; n];
    let only = args.duel_weapon.clone();
    for i in 0..n {
        for j in (i + 1)..n {
            if only.as_deref().is_some_and(|o| o != guns[i] && o != guns[j]) {
                continue;
            }
            let (mut mine, mut theirs) = (0.0f32, 0.0f32);
            for r in 0..rounds {
                let seed = field.seed ^ ((i * 31 + j) as u32 + r + 1).wrapping_mul(0x85ebca6b);
                let (w, e) = fight(&field, &guns[i], &guns[j], seconds, seed);
                mine += w.kills;
                theirs += e.kills;
                let (w, e) = fight(&field, &guns[j], &guns[i], seconds, seed ^ 0x5bd1_e995);
                mine += e.kills;
                theirs += w.kills;
            }
            grid[i][j] = mine - theirs;
            grid[j][i] = theirs - mine;
        }
    }
    print!("{:<17}", "");
    for g in &guns {
        print!("{:>7}", short(g));
    }
    println!("{:>9}", "net");
    let mut net = vec![0.0f32; n];
    for i in 0..n {
        for j in 0..n {
            if grid[i][j].is_finite() {
                net[i] += grid[i][j];
            }
        }
    }
    for i in 0..n {
        print!("{:<17}", guns[i]);
        for j in 0..n {
            if i == j {
                print!("{:>7}", "--");
            } else if grid[i][j].is_finite() {
                print!("{:>+7.0}", grid[i][j]);
            } else {
                print!("{:>7}", ".");
            }
        }
        println!("{:>+9.0}", net[i]);
    }

    println!(
        "
-- the standing --
"
    );
    println!("{:<17}{:>8}{:>9}{:>10}{:>9}", "weapon", "net", "acc", "dmg/shot", "range");
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| net[*b].total_cmp(&net[*a]));
    for i in order {
        let t = &bench[i].1;
        println!("{:<17}{:>+8.0}{:>8.0}%{:>10.1}{:>8.1}m", guns[i], net[i], t.accuracy() * 100.0, t.per_shot(), t.range());
    }
}

/// A column heading narrow enough for ten of them: the first letters of the
/// weapon's name, which are what tells them apart.
fn short(name: &str) -> String {
    let mut s: String = name.split('_').map(|w| w.chars().next().unwrap_or('?')).collect();
    if s.len() < 2 {
        s = name.chars().take(4).collect();
    }
    s.to_uppercase()
}
