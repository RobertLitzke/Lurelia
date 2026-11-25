//! Who calls, and when: the scheduler between a species' `voice` (library
//! `VoiceDef`) and the call engine (sound/call.rs).
//!
//! It reads the creatures rather than being told by them, as sound/watch.rs
//! does for a match, so no mover knows about sound. Each step it looks at
//! every creature whose species has a voice and asks what has changed:
//!
//! - "idle": a timer per call per individual, drawn from `every_s`, that
//!   fires when it runs out if the hour, season, weather, sex and age all
//!   allow it and the animal is awake and not running. A songbird's timer
//!   runs up to four times faster around sunrise: the dawn chorus.
//! - "flee" and "chase": a walker's mood turning to either.
//! - "takeoff": a bird that was sitting (on a perch, the ground, a trunk or
//!   the water) flying up, whichever mover flies it.
//! - "warn": a snake that `warns` holding its ground, tail buzzing; it
//!   calls again as each call ends while the warning lasts.
//! - "display:KIND": a display starting, or with `at: "jaw"` each time the
//!   display opens the jaw (the `call` display's howls, one per jaw).
//!
//! A call with `chorus` sets others of its species near it answering a
//! second or few later; answers don't set off more answers. At most
//! `LIVE` calls sound at once, `PER_SPECIES` of any one species, so a herd
//! of forty cows is not forty moos.
//!
//! A call follows its animal: every step each live call is posted again from
//! where its caller now is, with the same key and seed, which moves the
//! sounding call rather than starting another (sound/fx.rs `Kind::Call`).

use super::age::{Age, Stage};
use super::sex::Sex;
use super::walk::{Critter, Mood};
use super::{Creature, Look, Species};
use crate::library::CallDef;
use crate::sound::call::Cry;
use crate::sound::dsp::Rng;
use glam::Vec3;
use std::collections::HashMap;

/// Calls sounding at once, at most, and of one species.
const LIVE: usize = 12;
const PER_SPECIES: usize = 4;
/// Further from the player than this, a creature is not listened to.
const HEARD_M: f32 = 1500.0;
/// Rain heavier than this quiets the calls that don't say `rain`.
const DOWNPOUR: f32 = 0.35;

/// One individual's state, for telling what changed since the last step.
struct Caller {
    /// Seconds until each of its species' idle calls is due, by call index.
    due: Vec<f32>,
    mood: Option<Mood>,
    show: Option<&'static str>,
    jaw: bool,
    perched: Option<bool>,
    /// Its own number, for its song and its seeds.
    seed: u32,
    /// Calls made, so each is a new seed.
    made: u32,
    /// Seen this step (callers not seen are dropped).
    seen: bool,
}

/// A call sounding, posted again each step from where its caller is.
struct Live {
    ent: hecs::Entity,
    species: usize,
    cry: Cry,
    size: f32,
    seed: u32,
    boost: f32,
    /// Seconds left to keep posting it.
    left: f32,
}

/// An answer waiting to be given.
struct Answer {
    ent: hecs::Entity,
    call: usize,
    wait: f32,
}

#[derive(Default)]
pub struct Voices {
    callers: HashMap<hecs::Entity, Caller>,
    live: Vec<Live>,
    answers: Vec<Answer>,
    rng: Option<Rng>,
    /// Calls started, for the headless stats.
    pub started: u32,
}

/// What the scheduler needs of the world this step.
pub struct Now<'a> {
    pub hour: f32,
    pub rain: f32,
    pub season: &'a str,
    pub player: Vec3,
}

/// What set a call off.
#[derive(Clone, Copy, PartialEq)]
enum Why {
    Idle,
    Event,
    Answer,
}

impl Voices {
    pub fn update(&mut self, world: &hecs::World, species: &[Species], models: &[crate::models::Model], dt: f32, now: &Now) {
        if !species.iter().any(|s| s.beh.voice.is_some()) {
            return;
        }
        let mut rng = self.rng.take().unwrap_or_else(|| Rng::new(0xCA11));
        for c in self.callers.values_mut() {
            c.seen = false;
        }
        let mut fire: Vec<(hecs::Entity, usize, usize, Why)> = Vec::new();
        for (e, cr, look) in world.query::<(hecs::Entity, &Creature, &Look)>().iter() {
            let sp = &species[cr.species];
            let Some(voice) = sp.beh.voice.as_ref() else { continue };
            if voice.calls.is_empty() || (look.pos - now.player).length() > HEARD_M {
                continue;
            }
            let ent = world.entity(e).ok();
            let stage = ent.as_ref().and_then(|x| x.get::<&Age>().map(|a| a.stage)).unwrap_or(Stage::Adult);
            if stage == Stage::Dead {
                continue;
            }
            let mood = ent.as_ref().and_then(|x| x.get::<&Critter>().map(|c| c.mood));
            let show = ent.as_ref().and_then(|x| x.get::<&super::display::Show>().map(|s| (s.kind, s.out.pose.jaw > 0.05)));
            let perched = ent.as_ref().and_then(sitting);
            let warning = ent.as_ref().and_then(|x| x.get::<&super::snake::Serpent>().map(|s| s.rattle > 0.5)).unwrap_or(false);
            let caller = self.callers.entry(e).or_insert_with(|| {
                let seed = hash(e.to_bits().get());
                let mut r = Rng::new(seed);
                Caller {
                    // Spread the first calls over the whole wait, so a herd
                    // that has just come into range doesn't call as one.
                    due: voice.calls.iter().map(|c| every(c)[1] * r.unit()).collect(),
                    mood,
                    show: show.map(|s| s.0),
                    jaw: show.is_some_and(|s| s.1),
                    perched,
                    seed,
                    made: 0,
                    seen: true,
                }
            });
            caller.seen = true;
            let running = matches!(mood, Some(Mood::Flee | Mood::Chase));
            let awake = sp.beh.period().activity(now.hour) > 0.3;
            for (i, c) in voice.calls.iter().enumerate() {
                let on = c.on.as_str();
                let event = match on {
                    "idle" => {
                        let Some(due) = caller.due.get_mut(i) else { continue };
                        let mut rate = 1.0;
                        if Cry::parse(&c.cry) == Some(Cry::Song) {
                            rate += 3.0 * dawn(now.hour);
                        }
                        *due -= dt * rate;
                        if *due > 0.0 {
                            continue;
                        }
                        let [lo, hi] = every(c);
                        *due = lo + (hi - lo) * rng.unit();
                        // An hour it names wakes it whatever its period.
                        if running || (c.hours.is_none() && !awake) || (!c.rain && now.rain > DOWNPOUR) {
                            continue;
                        }
                        Why::Idle
                    }
                    "flee" => {
                        if !(mood == Some(Mood::Flee) && caller.mood != Some(Mood::Flee)) {
                            continue;
                        }
                        Why::Event
                    }
                    "chase" => {
                        if !(mood == Some(Mood::Chase) && caller.mood != Some(Mood::Chase)) {
                            continue;
                        }
                        Why::Event
                    }
                    "warn" => {
                        if !warning {
                            continue;
                        }
                        Why::Event
                    }
                    "takeoff" => {
                        if !(perched == Some(false) && caller.perched == Some(true)) {
                            continue;
                        }
                        Why::Event
                    }
                    d if d.starts_with("display:") => {
                        let kind = &d["display:".len()..];
                        let Some((now_kind, jaw)) = show else { continue };
                        if now_kind != kind {
                            continue;
                        }
                        let fired = if c.at.as_deref() == Some("jaw") { jaw && !caller.jaw } else { caller.show != Some(kind) };
                        if !fired {
                            continue;
                        }
                        Why::Event
                    }
                    _ => continue,
                };
                if !allowed(c, now, stage, ent.as_ref().and_then(|x| x.get::<&Sex>().map(|s| *s))) {
                    continue;
                }
                if rng.unit() > c.chance.unwrap_or(1.0) {
                    continue;
                }
                fire.push((e, cr.species, i, event));
            }
            caller.mood = mood;
            caller.show = show.map(|s| s.0);
            caller.jaw = show.is_some_and(|s| s.1);
            caller.perched = perched;
        }
        self.callers.retain(|_, c| c.seen);

        // Answers coming due.
        let mut due = Vec::new();
        self.answers.retain_mut(|a| {
            a.wait -= dt;
            if a.wait <= 0.0 {
                due.push((a.ent, a.call));
                false
            } else {
                true
            }
        });
        for (e, i) in due {
            let Ok(cr) = world.get::<&Creature>(e) else { continue };
            let running = world.get::<&Critter>(e).is_ok_and(|c| matches!(c.mood, Mood::Flee | Mood::Chase));
            if !running {
                fire.push((e, cr.species, i, Why::Answer));
            }
        }

        for (e, s, i, why) in fire {
            self.start(world, species, models, e, s, i, why, now, &mut rng);
        }

        // Every live call again from where its caller is now.
        self.live.retain_mut(|l| {
            l.left -= dt;
            if l.left <= 0.0 {
                return false;
            }
            let Ok(look) = world.get::<&Look>(l.ent) else { return false };
            let at = mouth(&look, models);
            crate::sound::emit(crate::sound::Noise::call(at, l.cry, l.size, l.seed, l.species as u16, key(l.ent), l.boost));
            true
        });
        self.rng = Some(rng);
    }

    #[allow(clippy::too_many_arguments)]
    fn start(&mut self, world: &hecs::World, species: &[Species], models: &[crate::models::Model], e: hecs::Entity, s: usize, i: usize, why: Why, now: &Now, rng: &mut Rng) {
        let Some(voice) = species[s].beh.voice.as_ref() else { return };
        let Some(c) = voice.calls.get(i) else { return };
        let Some(cry) = Cry::parse(&c.cry) else { return };
        // One call at a time from any one animal, and the budget.
        if self.live.iter().any(|l| l.ent == e) {
            return;
        }
        let same = self.live.iter().filter(|l| l.species == s).count();
        let cap = if why == Why::Answer { PER_SPECIES + 2 } else { PER_SPECIES };
        if self.live.len() >= LIVE || same >= cap {
            return;
        }
        let Ok(look) = world.get::<&Look>(e) else { return };
        let at = mouth(&look, models);
        let Some(caller) = self.callers.get_mut(&e) else { return };
        let stage = world.get::<&Age>(e).map(|a| a.stage).unwrap_or(Stage::Adult);
        let young = match stage {
            Stage::Juvenile => 0.6,
            Stage::Aged => 1.05,
            _ => 1.0,
        };
        // Individuals differ a little in size of voice too.
        let own = 0.92 + 0.16 * ((caller.seed >> 8) & 0xff) as f32 / 255.0;
        let size = voice.size.unwrap_or(1.0) * c.size.unwrap_or(1.0) * young * own;
        // A songbird sings its own song; every other call is new each time.
        let seed = if cry == Cry::Song { caller.seed } else { hash(caller.seed ^ caller.made.wrapping_mul(0x9E37_79B9)) };
        caller.made += 1;
        let len = cry.pattern(size, seed).len;
        let travel = (at - now.player).length() / 343.0;
        // Stop posting a little before the engine lets it go, so a late post
        // never starts it over.
        let left = (len + travel - 0.2).max(0.1);
        crate::sound::emit(crate::sound::Noise::call(at, cry, size, seed, s as u16, key(e), c.boost));
        self.live.push(Live {
            ent: e,
            species: s,
            cry,
            size,
            seed,
            boost: c.boost,
            left,
        });
        self.started += 1;
        if why == Why::Answer || c.chorus <= 0.0 {
            return;
        }
        let reach = c.chorus_m.unwrap_or(300.0);
        let mut n = 0;
        for (o, cr, ol) in world.query::<(hecs::Entity, &Creature, &Look)>().iter() {
            if o == e || cr.species != s || (ol.pos - look.pos).length() > reach || n >= 5 {
                continue;
            }
            if world.get::<&Age>(o).is_ok_and(|a| a.stage == Stage::Dead) {
                continue;
            }
            if rng.unit() < c.chorus {
                n += 1;
                self.answers.push(Answer {
                    ent: o,
                    call: i,
                    wait: (0.8 + 3.2 * rng.unit()) * (1.0 + 0.3 * n as f32),
                });
            }
        }
    }
}

/// Whether a bird is sitting rather than flying, for whichever mover flies
/// it; None for anything that doesn't fly.
fn sitting(e: &hecs::EntityRef) -> Option<bool> {
    if let Some(f) = e.get::<&super::flit::Flit>() {
        return Some(f.perched());
    }
    if let Some(f) = e.get::<&super::flock::Flyer>() {
        return Some(f.perched);
    }
    if let Some(p) = e.get::<&super::fowl::Paddler>() {
        return Some(!matches!(p.state, super::fowl::State::Fly { .. }));
    }
    if let Some(c) = e.get::<&super::woodpecker::Clinger>() {
        return Some(matches!(c.state, super::woodpecker::State::Cling { .. }));
    }
    if let Some(r) = e.get::<&super::runner::Runner>() {
        return Some(r.grounded());
    }
    if let Some(h) = e.get::<&super::hummer::Hummer>() {
        return Some(h.perched());
    }
    if let Some(g) = e.get::<&super::circle::Gull>() {
        return Some(g.sitting());
    }
    None
}

/// Where a call comes from: near the top of the animal.
fn mouth(look: &Look, models: &[crate::models::Model]) -> Vec3 {
    let h = models.get(look.model).map_or(0.5, |m| m.height);
    look.pos + Vec3::Y * h * 0.8
}

/// The call's key in the sound engine: which animal it is.
fn key(e: hecs::Entity) -> u32 {
    let b = e.to_bits().get();
    ((b ^ (b >> 32)) as u32) | 0x8000_0000
}

fn hash(x: impl Into<u64>) -> u32 {
    let mut z = x.into().wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as u32
}

fn every(c: &CallDef) -> [f32; 2] {
    let [lo, hi] = c.every_s.unwrap_or([60.0, 240.0]);
    [lo.max(1.0), hi.max(lo.max(1.0))]
}

/// How far into the dawn chorus `hour` is, 0 to 1: from first light to an
/// hour or so after sunrise, morning only.
fn dawn(hour: f32) -> f32 {
    if hour > 12.0 {
        return 0.0;
    }
    let e = crate::world::clock::sun_elevation(hour);
    let up = ((e + 10.0) / 6.0).clamp(0.0, 1.0);
    let down = (1.0 - (e - 4.0) / 8.0).clamp(0.0, 1.0);
    up.min(down)
}

/// Whether the hours a call names include `hour`.
pub fn in_hours(hours: &str, hour: f32) -> bool {
    let e = crate::world::clock::sun_elevation(hour);
    let twilight = (-14.0..8.0).contains(&e);
    match hours {
        "day" => e > -4.0,
        "night" => e < -8.0,
        "dawn" => twilight && hour < 12.0,
        "dusk" => twilight && hour >= 12.0,
        "twilight" => twilight,
        _ => true,
    }
}

/// The hour, season, sex and age gates on a call.
fn allowed(c: &CallDef, now: &Now, stage: Stage, sex: Option<Sex>) -> bool {
    if c.hours.as_deref().is_some_and(|h| !in_hours(h, now.hour)) {
        return false;
    }
    if !c.season.is_empty() && !c.season.iter().any(|s| s == now.season) {
        return false;
    }
    if let Some(want) = c.sex.as_deref() {
        if sex.is_none_or(|s| s.name() != want) {
            return false;
        }
    }
    if c.age.as_deref().is_some_and(|a| a != stage.name()) {
        return false;
    }
    true
}

/// What can go wrong in a voice, for the library check.
pub fn validate(name: &str, v: &crate::library::VoiceDef, displays: &[crate::library::DisplayDef], out: &mut Vec<String>) {
    const ONS: [&str; 5] = ["idle", "flee", "chase", "takeoff", "warn"];
    const HOURS: [&str; 5] = ["day", "night", "dawn", "dusk", "twilight"];
    const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];
    for c in &v.calls {
        if Cry::parse(&c.cry).is_none() {
            let names: Vec<&str> = crate::sound::call::CRIES.iter().map(|c| c.name()).collect();
            out.push(format!("creature {name}: voice: unknown cry {}, not one of {names:?}", c.cry));
        }
        match c.on.strip_prefix("display:") {
            Some(kind) => {
                if !displays.iter().any(|d| d.kind == kind) {
                    out.push(format!("creature {name}: voice: on {} but it has no {kind} display", c.on));
                }
            }
            None if !ONS.contains(&c.on.as_str()) => out.push(format!("creature {name}: voice: unknown on {}, not one of {ONS:?} or display:KIND", c.on)),
            None => {}
        }
        if let Some(a) = c.at.as_deref() {
            if !c.on.starts_with("display:") || !["start", "jaw"].contains(&a) {
                out.push(format!("creature {name}: voice: at {a} is for a display, \"start\" or \"jaw\""));
            }
        }
        if let Some(h) = c.hours.as_deref() {
            if !HOURS.contains(&h) {
                out.push(format!("creature {name}: voice: unknown hours {h}, not one of {HOURS:?}"));
            }
        }
        for s in &c.season {
            if !SEASONS.contains(&s.as_str()) {
                out.push(format!("creature {name}: voice: unknown season {s}"));
            }
        }
        if c.sex.as_deref().is_some_and(|s| s != "male" && s != "female") {
            out.push(format!("creature {name}: voice: sex is \"male\" or \"female\""));
        }
        if c.age.as_deref().is_some_and(|a| !["juvenile", "adult", "aged"].contains(&a)) {
            out.push(format!("creature {name}: voice: age is \"juvenile\", \"adult\" or \"aged\""));
        }
        if !(0.0..=1.0).contains(&c.chorus) {
            out.push(format!("creature {name}: voice: chorus is a chance, 0 to 1"));
        }
        if c.every_s.is_some_and(|[lo, hi]| lo <= 0.0 || hi < lo) {
            out.push(format!("creature {name}: voice: every_s is [low, high] seconds"));
        }
        if !(-12.0..=18.0).contains(&c.boost) {
            out.push(format!("creature {name}: voice: boost {} dB is out of reason", c.boost));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_split_the_day() {
        assert!(in_hours("day", 12.0));
        assert!(!in_hours("night", 12.0));
        assert!(in_hours("night", 1.0));
        assert!(!in_hours("dusk", 6.0));
    }

    #[test]
    fn the_dawn_chorus_is_in_the_morning() {
        let peak = (40..100).map(|t| dawn(t as f32 * 0.1)).fold(0.0, f32::max);
        assert!(peak > 0.9);
        assert_eq!(dawn(13.0), 0.0);
        assert_eq!(dawn(1.0), 0.0);
    }
}
