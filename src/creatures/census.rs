//! Counting creatures for the headless stats, and naming the one under the
//! crosshair.

use super::age::{Age, Stage};
use super::circle::Gull;
use super::flit::Flit;
use super::flock::Flyer;
use super::terrain::Terrain;
use super::walk::Critter;
use super::{Creature, Look, Pathing, Species};
use glam::Vec3;
use std::collections::BTreeMap;

/// Creatures alive now by species.
pub fn census(world: &hecs::World, species: &[Species]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for c in world.query::<&Creature>().iter() {
        if c.species < species.len() {
            *out.entry(super::display_name(species, c.species).to_string()).or_insert(0) += 1;
        }
    }
    out
}

/// How many are alive toward each species' biome quota: a rare morph
/// counts toward the parent it stands in for.
pub fn quota(world: &hecs::World) -> crate::util::FxHashMap<usize, u32> {
    let mut have = crate::util::FxHashMap::default();
    // The far band has its own count (spawn.rs `Distant`).
    for c in world.query::<&Creature>().without::<&super::spawn::Distant>().iter() {
        *have.entry(c.spawned_for).or_insert(0) += 1;
    }
    have
}

/// Walkers, flocking birds and flitting birds (legacy animals included).
pub fn critter_count(world: &hecs::World) -> usize {
    world.query::<&Critter>().iter().count()
        + world.query::<&Flyer>().iter().count()
        + world.query::<&Flit>().iter().count()
        + world.query::<&super::runner::Runner>().iter().count()
        + world.query::<&super::hummer::Hummer>().iter().count()
}

/// Rigged creatures by current activity, and flitting birds by where they are.
pub fn activities(world: &hecs::World, land: Option<&crate::world::fields::World>) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for a in world.query::<&crate::anim::Anim>().iter() {
        if let Some(l) = &a.life {
            let posture = if l.level > 1.5 {
                " (down)"
            } else if l.level > 0.5 {
                " (sitting)"
            } else {
                ""
            };
            *out.entry(format!("{}{posture}", l.act.name())).or_insert(0) += 1;
        }
        if let Some(f) = &a.flight {
            let what = if f.fold > 0.5 {
                "bird perched"
            } else if f.flapping > 0.5 {
                "bird beating"
            } else {
                "bird gliding"
            };
            *out.entry(what.to_string()).or_insert(0) += 1;
        }
    }
    // Predators at their kills, and the kills with meat on them.
    for c in world.query::<&super::walk::Critter>().iter() {
        let what = match c.mood {
            super::walk::Mood::Feed => "carrying a kill",
            super::walk::Mood::Eat => "eating a kill",
            _ => continue,
        };
        *out.entry(what.to_string()).or_insert(0) += 1;
    }
    for c in world.query::<&super::carcass::Carcass>().iter() {
        if c.meat > 0.0 && !c.bones {
            *out.entry("carcass".to_string()).or_insert(0) += 1;
        }
    }
    // Running birds and hummingbirds: what each is doing.
    for (r, look) in world.query::<(&super::runner::Runner, &Look)>().iter() {
        let moving = (look.pos - look.prev_pos).length() > 1e-3;
        *out.entry(format!("runner {}", r.doing(moving))).or_insert(0) += 1;
    }
    for h in world.query::<&super::hummer::Hummer>().iter() {
        *out.entry(format!("hummingbird {}", h.doing())).or_insert(0) += 1;
    }
    // Snakes: what each is doing (creatures/snake.rs).
    for s in world.query::<&super::snake::Serpent>().iter() {
        *out.entry(format!("snake {}", s.doing())).or_insert(0) += 1;
    }
    for o in world.query::<&super::octopus::Octopus>().iter() {
        *out.entry(format!("octopus {}", o.doing())).or_insert(0) += 1;
    }
    // Flitting birds: on a shrub or tree top, on the ground, or flying.
    for (f, look) in world.query::<(&Flit, &Look)>().iter() {
        let rise = land.map_or(0.0, |l| look.pos.y - l.f.height(look.pos.x, look.pos.z));
        let what = if !f.perched() {
            "flit: flying"
        } else if rise > 0.5 {
            "flit: on a top"
        } else {
            "flit: on the ground"
        };
        *out.entry(what.to_string()).or_insert(0) += 1;
    }
    out
}

/// Birds sitting this step, and those sitting on water (which they should not).
pub fn count_sitting(world: &hecs::World, terrain: &Terrain, stats: &mut Pathing) {
    for (look, flit, fl) in world.query::<(&Look, Option<&Flit>, Option<&Flyer>)>().iter() {
        let sitting = flit.is_some_and(|f| f.perched()) || fl.is_some_and(|f| f.perched);
        if sitting {
            stats.birds_sitting += 1;
            let low = terrain.height(look.pos.x, look.pos.z).is_none_or(|g| look.pos.y - g < 0.6);
            stats.birds_on_water += (low && terrain.wet(look.pos.x, look.pos.z)) as u32;
        }
    }
}

/// The creature under a ray, nearest first: (distance, display name).
/// The name is built only for the winner; this runs every frame.
pub fn identify(world: &hecs::World, species: &[Species], models: &[crate::models::Model], origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, String)> {
    identify_one(world, species, models, origin, dir, max).map(|(t, name, _)| (t, name))
}

/// `identify`, with the entity it found.
pub fn identify_one(world: &hecs::World, species: &[Species], models: &[crate::models::Model], origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, String, hecs::Entity)> {
    enum Who {
        Species(usize),
        /// The random island's deer, rabbit or fox, by the model it wears.
        Legacy(usize),
        Gull,
    }
    let mut best: Option<(f32, Who, Stage, hecs::Entity)> = None;
    for (e, look, me, crit, gull, age) in world.query::<(hecs::Entity, &Look, Option<&Creature>, Option<&Critter>, Option<&Gull>, Option<&Age>)>().iter() {
        let who = match (me, crit, gull) {
            (Some(c), _, _) => Who::Species(c.species),
            (_, Some(_), _) => Who::Legacy(look.model),
            (_, _, Some(_)) => Who::Gull,
            _ => continue,
        };
        if matches!(who, Who::Species(s) if s >= species.len()) {
            continue;
        }
        let stage = age.map_or(Stage::Adult, |a| a.stage);
        // The ray is tested against the whole animal, not a point at its
        // feet: aim at a giraffe's neck and it is a giraffe.
        let (h, hw) = models.get(look.model).map_or((1.0, 0.5), |m| (m.height, m.half_w));
        // Something lying on the ground is a low mound; something standing
        // is a column from its knees to its head.
        let (lo, hi) = if stage == Stage::Dead { (0.0, (h * 0.25).min(0.4)) } else { (h * 0.2, h * 0.92) };
        let Some((t, d)) = ray_to_upright(origin, dir, look.pos, lo, hi) else {
            continue;
        };
        if !(0.0..max).contains(&t) {
            continue;
        }
        // A little more forgiving with distance, so far animals can be named.
        if d < (hw + 0.25).max(0.6) + t * 0.01 && best.as_ref().map_or(true, |b| t < b.0) {
            best = Some((t, who, stage, e));
        }
    }
    let (t, who, stage, e) = best?;
    let name = match who {
        Who::Species(s) => super::display_name(species, s).to_string(),
        Who::Legacy(m) => super::spawn::legacy(m).map_or("animal", |l| l.name).to_string(),
        Who::Gull => "GULL".to_string(),
    };
    Some((t, format!("{}{}", name.replace('_', " "), stage.label()).to_uppercase(), e))
}

/// One line on an individual for the crosshair readout: its sex and age, and
/// what it is doing -- the display it is in and how far through, or else
/// its activity or its mover's state.
pub fn doing(world: &hecs::World, e: hecs::Entity) -> String {
    let Ok(ent) = world.entity(e) else { return String::new() };
    let mut out = Vec::new();
    if let Some(s) = ent.get::<&super::sex::Sex>() {
        out.push(s.name().to_string());
    }
    out.push(ent.get::<&Age>().map_or(Stage::Adult, |a| a.stage).name().to_string());
    let what = if let Some(show) = ent.get::<&super::display::Show>() {
        format!("{}: {}", show.kind, show.phase)
    } else if let Some(h) = ent.get::<&super::hummer::Hummer>() {
        h.doing().to_string()
    } else {
        let mood = ent.get::<&Critter>().map(|c| c.mood);
        let act = ent.get::<&crate::anim::Anim>().and_then(|a| a.life.as_ref().map(|l| l.act.name()));
        match (mood, act) {
            (Some(super::walk::Mood::Flee), _) => "fleeing".to_string(),
            (Some(super::walk::Mood::Chase), _) => "hunting".to_string(),
            (Some(super::walk::Mood::Feed), _) => "carrying a kill".to_string(),
            (Some(super::walk::Mood::Eat), _) => "eating a kill".to_string(),
            (_, Some(a)) => a.to_string(),
            (Some(super::walk::Mood::Walk), None) => "walk".to_string(),
            (Some(super::walk::Mood::Idle), None) => "stand".to_string(),
            (None, None) => String::new(),
        }
    };
    if !what.is_empty() {
        out.push(what);
    }
    out.join("  ").to_uppercase()
}

/// Closest approach of a ray to the upright segment from `foot + lo` to
/// `foot + hi`: how far along the ray it happens, and how far off it passes.
fn ray_to_upright(origin: Vec3, dir: Vec3, foot: Vec3, lo: f32, hi: f32) -> Option<(f32, f32)> {
    let w = foot - origin;
    let wd = w.dot(dir);
    // |w + sY|^2 - ((w + sY).dir)^2 as a quadratic in the height s.
    let a = 1.0 - dir.y * dir.y;
    let b = w.y - wd * dir.y;
    let s = if a > 1e-4 { (-b / a).clamp(lo, hi) } else { (lo + hi) * 0.5 };
    let v = w + Vec3::Y * s;
    let t = v.dot(dir);
    Some((t, (v - dir * t).length()))
}
