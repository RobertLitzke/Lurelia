//! Rigged birds: wings, tail, legs and attitude from how each is flying
//! (src/anim/flight.rs). Flight paths are flock.rs, flit.rs and circle.rs.

use crate::anim::Plan;
use super::circle::Gull;
use super::flit::Flit;
use super::flock::{Flock, Flyer, Mode};
use super::fowl::Paddler;
use super::woodpecker::Clinger;
use super::{Creature, Look, Species};
use crate::anim::flight::{beat_for, Flight, Input, Pattern};
use crate::anim::Anim;
use crate::models::Model;
use crate::util::FxHashMap as HashMap;
use glam::Vec3;

pub fn animate(world: &mut hecs::World, species: &[Species], dt: f32, models: &[Model]) {
    let mut new: Vec<(hecs::Entity, Anim)> = Vec::new();
    for (e, look, me, anim) in world.query::<(hecs::Entity, &Look, &Creature, Option<&Anim>)>().iter() {
        if anim.is_some() {
            continue;
        }
        let verb = species[me.species].verb();
        if !verb.flies() {
            continue;
        }
        let m = &models[look.model];
        let Some(rig) = m.rig.as_ref().filter(|r| r.plan == Some(Plan::Bird) || r.plan == Some(Plan::Ray)) else {
            continue;
        };
        let beh = &species[me.species].beh;
        let pattern = beh.flight.as_deref().and_then(Pattern::parse).unwrap_or(verb.flight);
        let beat = beh.wingbeat.unwrap_or_else(|| beat_for(rig, m.voxel));
        let bits = e.to_bits().get();
        new.push((
            e,
            Anim::flying(rig, Flight::new(pattern, beat, crate::world::noise::hash32(bits as u32 ^ (bits >> 32) as u32 ^ 0xb1d5))),
        ));
    }
    for (e, a) in new {
        let _ = world.insert_one(e, a);
    }
    // Flocks coming down: their members flare near the landing spot.
    let landing: HashMap<hecs::Entity, Vec3> = world.query::<(hecs::Entity, &Flock)>().iter().filter(|(_, f)| f.mode == Mode::Land).map(|(e, f)| (e, f.goal)).collect();
    for (look, anim, fl, flit, gull, (pad, cl)) in world.query_mut::<(&Look, &mut Anim, Option<&Flyer>, Option<&Flit>, Option<&Gull>, (Option<&Paddler>, Option<&Clinger>))>() {
        if anim.flight.is_none() {
            continue;
        }
        let Some(rig) = models[look.model].rig.as_ref() else {
            continue;
        };
        let vel = (look.pos - look.prev_pos) / dt.max(1e-4);
        let (perched, near_goal) = match (fl, flit, pad, cl) {
            (Some(f), ..) => (f.perched, landing.get(&f.flock).is_some_and(|g| (*g - look.pos).length() < 5.0)),
            (_, Some(f), ..) => (f.perched(), f.landing()),
            (_, _, Some(p), _) => (p.floating(), p.landing()),
            (_, _, _, Some(c)) => (c.clinging(), c.landing()),
            _ => gull.map_or((false, false), |g| (g.floating(), g.landing())),
        };
        let cling = cl.is_some_and(|c| c.clinging());
        anim.step_flight(
            rig,
            models[look.model].voxel,
            &Input {
                dt,
                vel,
                yaw: look.yaw,
                perched,
                landing: near_goal,
                cling,
            },
        );
    }
}
