//! The capture-the-flag bench: two sides of bots, one run by the team brain
//! and one by something else, on a CTF world, with no window or GPU.
//!
//! The team brain earns its place only if it beats the simplest things a
//! side could do instead, so every row is a coach against a coach:
//!
//! - **plan v base**: the shipping brain against itself as it stood before
//!   the tournament (`brain::team_v0`). Until the plan changes this is the
//!   twin row: on a mirrored map with the sides swapped every seed it comes
//!   out level, and a row that does not is a map or a spawn that is not fair.
//! - **plan v fixed**: the other side gives each bot a job at the start,
//!   attack or defend, and never changes it.
//! - **plan v rush**: the other side sends everybody at the flag.
//!
//! Each seed is played twice with the coaches swapped between the sides, so
//! the half of the map a coach starts in is not what wins. No wall clock in
//! the loop: the same arguments give the same table.
//!
//!     lurelia --world ctfbox --ctf 4 --ctf-seconds 300 --ctf-size 3

use crate::brain::team::Coach;
use crate::cli::Args;
use crate::player::{MoveInput, Player};
use crate::shooter::flag::Event;
use crate::shooter::Shooter;
use crate::tools::tourney;
use crate::world::fields::BotDef;
use crate::{worldload, Vec3};

const NAMES: [&str; 8] = ["RILL", "WEIR", "SUMP", "FLUE", "GULLY", "LEAT", "DRAIN", "SLUE"];

/// What one coach's side did, summed over its matches.
#[derive(Default, Clone)]
struct Side {
    matches: f32,
    secs: f32,
    caps: f32,
    /// Matches won, drawn.
    won: f32,
    drawn: f32,
    grabs: f32,
    returns: f32,
    /// Carries that ended in a capture, and carries in all; seconds carried.
    scored: f32,
    carries: f32,
    carry_s: f32,
    /// How far from home its carriers were when they died, summed.
    died_from_home: f32,
    kills: f32,
    /// Seconds both flags were out at once, and seconds its carrier stood
    /// within 10 m of its own stand unable to score because its own flag
    /// was away.
    both_out: f32,
    stalled: f32,
    /// Steps its living bots spent on each job, by name.
    doing: std::collections::BTreeMap<&'static str, f32>,
}

impl Side {
    fn per_min(&self, v: f32) -> f32 {
        if self.secs > 0.0 {
            v * 60.0 / self.secs
        } else {
            0.0
        }
    }
}

fn coach_name(c: Coach) -> &'static str {
    match c {
        Coach::Plan => "plan",
        Coach::Fixed => "fixed",
        Coach::Rush => "rush",
        Coach::Base => "base",
    }
}

/// One match: `coaches[t]` runs team `t`. Adds what each side did to `out[t]`.
#[allow(clippy::too_many_arguments)]
fn play(f: &tourney::Field, coaches: [Coach; 2], tweaks: [u32; 2], skill: &str, size: usize, seconds: f32, seed: u32, out: [&mut Side; 2]) {
    let mut rules = f.rules.clone();
    rules.bots = (0..size * 2)
        .map(|i| BotDef {
            name: NAMES[i % NAMES.len()].to_string(),
            skill: skill.to_string(),
            colour: None,
            bold: 0.0,
            steady: 0.0,
            without: Vec::new(),
            tune: Default::default(),
            team: Some((i % 2) as u8),
        })
        .collect();
    rules.teams = true;
    // A long match is played to the clock, not stopped at the first side to
    // reach the capture count, so a stronger side shows by how much.
    if let Some(c) = rules.ctf.as_mut() {
        c.captures = 0;
        c.time_s = 0.0;
    }
    let mut sh = Shooter::repeatable(&f.land.lib, &rules, f.home, &f.store, seed);
    sh.bench_player();
    sh.coach(0, coaches[0]);
    sh.coach(1, coaches[1]);
    sh.tweak(0, tweaks[0]);
    sh.tweak(1, tweaks[1]);
    let away = f.home.0 + Vec3::new(200.0, 0.0, 200.0);
    let mut player = Player::new(away, f.home.1);
    let mut ents = crate::entities::Entities::new(Default::default(), 1);
    let mut actions = crate::game::Actions::default();
    let input = MoveInput::default();
    let mut shot = false;
    let step = crate::game::STEP;
    let steps = (seconds / step) as usize;
    let mut doing: [std::collections::BTreeMap<&'static str, f32>; 2] = Default::default();
    let mut both_out = 0.0f32;
    let mut stalled = [0.0f32; 2];
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
        for (team, what) in sh.doings() {
            if let Some(t) = team {
                *doing[t as usize].entry(what).or_default() += step;
            }
        }
        let (f0, f1) = (sh.flag_now(0, &player), sh.flag_now(1, &player));
        if let (Some((s0, _)), Some((s1, _))) = (f0, f1) {
            let home = crate::shooter::flag::State::Home;
            if s0 != home && s1 != home {
                both_out += step;
            }
            for t in 0..2usize {
                // Side t's carrier holds the other flag; is it home and stuck?
                let (theirs, ours) = if t == 0 { (f1, f0) } else { (f0, f1) };
                if let (Some((crate::shooter::flag::State::Carried(_), at)), Some((own, _))) = (theirs, ours) {
                    let stand = sh.flag_stand(t as u8).unwrap_or(Vec3::ZERO);
                    if own != home && Vec3::new(at.x - stand.x, 0.0, at.z - stand.z).length() < 10.0 {
                        stalled[t] += step;
                    }
                }
            }
        }
    }
    let caps = sh.captures();
    let stands: Vec<Vec3> = (0..2u8).map(|t| sh.flag_stand(t).unwrap_or(Vec3::ZERO)).collect();
    let sides = sh.sides();
    let scores = sh.scores();
    // Carries, from the flag log: a carry starts when a side picks the
    // other's flag up and ends in a capture or a drop.
    let mut open: [Option<(f32, usize)>; 2] = [None, None];
    let [a, b] = out;
    let mut acc = [a, b];
    // CTF_LOG=1: every flag event, with where it happened against the two
    // stands (x, z from each, and height over the stand), to see where a
    // map's carries end.
    if std::env::var_os("CTF_LOG").is_some() {
        eprintln!("seed {seed}: stands {:?} {:?}", stands[0], stands[1]);
        for &(t, e) in sh.flag_log() {
            let at = match e {
                Event::Dropped { at, .. } => Some(at),
                _ => None,
            };
            let rel = at.map(|p| {
                let d = |s: Vec3| Vec3::new(p.x - s.x, 0.0, p.z - s.z).length();
                format!(
                    "  {:.0} m from stand 0, {:.0} m from stand 1, {:+.1} m over the stands, at ({:.1}, {:.1}, {:.1})",
                    d(stands[0]),
                    d(stands[1]),
                    p.y - stands[0].y,
                    p.x,
                    p.y,
                    p.z
                )
            });
            eprintln!("  {t:6.1}s {e:?}{}", rel.unwrap_or_default());
        }
    }
    for &(t, e) in sh.flag_log() {
        let carrier_side = 1 - e.team() as usize;
        match e {
            Event::Taken { .. } | Event::PickedUp { .. } => {
                if let Event::Taken { by, .. } | Event::PickedUp { by, .. } = e {
                    open[carrier_side] = Some((t, by));
                }
                acc[carrier_side].grabs += 1.0;
                acc[carrier_side].carries += 1.0;
            }
            Event::Captured { .. } => {
                if let Some((t0, _)) = open[carrier_side].take() {
                    acc[carrier_side].carry_s += t - t0;
                }
                acc[carrier_side].scored += 1.0;
            }
            Event::Dropped { at, .. } => {
                if let Some((t0, _)) = open[carrier_side].take() {
                    acc[carrier_side].carry_s += t - t0;
                }
                let home = stands[carrier_side];
                acc[carrier_side].died_from_home += Vec3::new(at.x - home.x, 0.0, at.z - home.z).length();
            }
            Event::Returned { by: Some(_), team } => acc[team as usize].returns += 1.0,
            Event::Returned { .. } => {}
        }
    }
    for t in 0..2 {
        let s = &mut acc[t];
        s.matches += 1.0;
        s.secs += seconds;
        s.both_out += both_out;
        s.stalled += stalled[t];
        s.caps += caps[t] as f32;
        if caps[t] > caps[1 - t] {
            s.won += 1.0;
        } else if caps[t] == caps[1 - t] {
            s.drawn += 1.0;
        }
        s.kills += scores.iter().zip(&sides).filter(|(_, side)| **side == Some(t as u8)).map(|((_, sc), _)| sc.kills as f32).sum::<f32>();
        for (k, v) in &doing[t] {
            *s.doing.entry(k).or_default() += v;
        }
    }
}

pub fn run(args: &Args) {
    let field = match tourney::open(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ctf: {e}");
            std::process::exit(1);
        }
    };
    if field.rules.ctf.is_none() {
        eprintln!("ctf: {} has no ctf block in its shooter rules", worldload::selected());
        std::process::exit(1);
    }
    let seconds = args.ctf_seconds.unwrap_or(300.0);
    let seeds = args.ctf.unwrap_or(2).max(1);
    let size = args.ctf_size.unwrap_or(3).clamp(1, 4) as usize;
    let skill = args.ctf_skill.clone().unwrap_or_else(|| "average".to_string());
    if crate::brain::person::level(&skill).is_none() {
        eprintln!("ctf: no difficulty called {skill}");
        std::process::exit(1);
    }
    println!("{}: {size} a side, {skill}, {seeds} seeds x both sides, {seconds:.0} s a match", worldload::selected());
    println!(
        "\n{:<15}{:>7}{:>7}{:>7}{:>8}{:>8}{:>9}{:>9}{:>9}{:>8}{:>7}{:>8}   jobs",
        "coach v coach", "won", "drawn", "caps/m", "grabs/m", "scored", "carry s", "fell at", "rets/m", "kills/m", "both", "stalled"
    );
    // CTF_AB=attack1,carry2: one row only, the plan with those role changes
    // (`brain::TWEAKS`) against the plan without them. Same coach both sides,
    // so who does which job is dealt the same way and only how the jobs are
    // played differs. CTF_AB=none is the twin check: it must come out level.
    let ab = std::env::var("CTF_AB").ok().map(|v| {
        let (mask, bad) = crate::brain::tweak_mask(&v);
        if !bad.is_empty() && v != "none" {
            eprintln!("ctf: no role change called {}", bad.join(", "));
            std::process::exit(1);
        }
        mask
    });
    let others: &[Coach] = if ab.is_some() { &[Coach::Plan] } else { &[Coach::Base, Coach::Fixed, Coach::Rush] };
    let mask = ab.unwrap_or(0);
    // CTF_SEED0=N starts the seeds at N, for a check on matches a variant was
    // not chosen on.
    let first: u32 = std::env::var("CTF_SEED0").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    for &other in others {
        let mut us = Side::default();
        let mut them = Side::default();
        for r in first..first + seeds {
            let seed = field.seed ^ (r as u32 + 1).wrapping_mul(0x9e37_79b9);
            play(&field, [Coach::Plan, other], [mask, 0], &skill, size, seconds, seed, [&mut us, &mut them]);
            play(&field, [other, Coach::Plan], [0, mask], &skill, size, seconds, seed, [&mut them, &mut us]);
        }
        let ours = if ab.is_some() { "ab v plan".to_string() } else { format!("plan v {}", coach_name(other)) };
        for (label, s) in [(ours, &us), (format!("  {} side", coach_name(other)), &them)] {
            let total: f32 = s.doing.values().sum::<f32>().max(1e-3);
            let jobs: Vec<String> = s.doing.iter().map(|(k, v)| format!("{k} {:.0}%", v * 100.0 / total)).collect();
            println!(
                "{:<15}{:>7.0}{:>7.0}{:>7.2}{:>8.2}{:>7.0}%{:>9.1}{:>8.0}m{:>9.2}{:>8.1}{:>6.0}%{:>7.0}%   {}",
                label,
                s.won,
                s.drawn,
                s.per_min(s.caps),
                s.per_min(s.grabs),
                if s.carries > 0.0 { s.scored * 100.0 / s.carries } else { 0.0 },
                if s.carries > 0.0 { s.carry_s / s.carries } else { 0.0 },
                if s.carries > s.scored { s.died_from_home / (s.carries - s.scored) } else { 0.0 },
                s.per_min(s.returns),
                s.per_min(s.kills),
                s.both_out * 100.0 / s.secs.max(1.0),
                s.stalled * 100.0 / s.secs.max(1.0),
                jobs.join(", ")
            );
        }
    }
}
