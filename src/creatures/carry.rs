//! Prey in a predator's mouth.
//!
//! A coyote that gets within a metre of a rabbit has it: the rabbit stops
//! being an animal and becomes a limp body (anim/limp.rs) held in the
//! coyote's mouth by whatever part of it the mouth closed on (`grab`). Nothing here decides whether it is carried or
//! dragged. The body hangs from the mouth, so a rabbit swings clear of the
//! ground and a deer, longer than a cougar's mouth is high, lies along the
//! ground behind it and is hauled. It sways and trails as the carrier walks
//! because the mouth moves and the body follows it.
//!
//! A walker carries its kill a short way, puts it down and eats it
//! (carcass.rs, walk.rs `Mood::Eat`). A bird of prey takes its kill in its
//! talons by the back and lets go of it after a while; a roadrunner carries
//! a lizard in its bill. Whatever is let go is a carcass, and anything that
//! hunts that kind of animal may come and eat from it.

use super::age::Age;
use super::carcass::{self, Carcass};
use super::terrain::Terrain;
use super::walk::{Critter, Mood};
use super::{Creature, Look, Species};
use crate::anim::limp::Grip;
use crate::anim::{Anim, Role};
use crate::models::Model;
use glam::{Quat, Vec3};

/// How close a hunter has to get to take its prey.
pub const CATCH_M: f32 = 1.0;

/// The catch distance in use. `VG_CATCH_M=12` lets a hunter take prey across
/// the clearing and prints each kill, for testing without waiting on one.
pub fn catch_m() -> f32 {
    std::env::var("VG_CATCH_M").ok().and_then(|v| v.parse().ok()).unwrap_or(CATCH_M)
}
/// How long a walker carries a kill before it puts it down to eat.
pub const CARRY_S: f32 = 12.0;
/// The longest one animal spends eating at one sitting.
pub const EAT_S: f32 = 45.0;
/// How long after a meal before it hunts again.
pub const FULL_S: f32 = 180.0;
/// How far in front of the head joint the mouth is, in carrier heights.
const MUZZLE: f32 = 0.35;

/// A bird of prey with a kill in its talons, until clock time `until`.
pub struct Talons {
    pub until: f32,
}

/// A bird with a kill in its bill (a roadrunner with a lizard), until
/// clock time `until`: it has no walker's mood to say when it is done.
pub struct Beak {
    pub until: f32,
}

/// Prey being carried off by the entity that caught it.
pub struct Carried {
    pub by: hecs::Entity,
    /// Where it is held: the point of it the hunter took hold of.
    pub grip: Grip,
}

/// Turn each caught animal into a body in its hunter's mouth. Called once
/// the movers' own passes are done, since this removes components.
pub fn take(world: &mut hecs::World, species: &[Species], models: &[Model], kills: &[(hecs::Entity, hecs::Entity)], now: f32) {
    for &(hunter, victim) in kills {
        // Two hunters can reach the same rabbit on the same step.
        if world.get::<&Carried>(victim).is_ok() || world.get::<&Age>(victim).is_ok_and(|a| a.dead()) {
            continue;
        }
        let Ok((model, at)) = world.get::<&Look>(victim).map(|l| (l.model, l.pos)) else {
            continue;
        };
        let Some(prey_m) = models.get(model) else {
            continue;
        };
        if prey_m.rig.is_none() {
            continue;
        }
        // A bird of prey carries what it takes in its talons, whatever its
        // size, and lets go of it after a while.
        let flier = world.get::<&super::flock::Flyer>(hunter).is_ok();
        // It is a body now: no mover, no collision box, no family, and none
        // of the other movers -- it is not flying, floating, swimming or
        // hopping any more.
        let _ = world.remove_one::<Critter>(victim);
        let _ = world.remove_one::<crate::physics::Body>(victim);
        let _ = world.remove_one::<super::herd::Kin>(victim);
        let _ = world.remove_one::<super::flock::Flyer>(victim);
        let _ = world.remove_one::<super::circle::Gull>(victim);
        let _ = world.remove_one::<super::fowl::Paddler>(victim);
        let _ = world.remove_one::<super::swim::Fish>(victim);
        let _ = world.remove_one::<super::flit::Flit>(victim);
        let _ = world.remove_one::<super::runner::Runner>(victim);
        let _ = world.remove_one::<super::hummer::Hummer>(victim);
        if !carcass::go_limp(world, victim, models) {
            continue;
        }
        if flier {
            let _ = world.insert_one(hunter, Talons { until: now + CARRY_S * 4.0 });
        }
        let leaves_bones = world.get::<&Creature>(victim).map(|c| species[c.species].ages.bones).unwrap_or(false);
        let mut age = Age::fresh_kill(now);
        if !leaves_bones {
            // Nothing worth drawing: it is gone once it is eaten or rotted.
            age.bones_at = f32::MAX;
            age.until = now + super::age::CARCASS_S;
        }
        if std::env::var_os("VG_CATCH_M").is_some() {
            let who = world.get::<&Creature>(victim).map(|c| species[c.species].name.clone()).unwrap_or_default();
            let by = world.get::<&Creature>(hunter).map(|c| species[c.species].name.clone()).unwrap_or_default();
            eprintln!("VG_CATCH: {by} took a {who}");
        }
        let grip = grab(world, hunter, victim, models, flier);
        let _ = world.insert(victim, (age, Carried { by: hunter, grip }, Carcass::fresh(prey_m.height, at)));
    }
}

/// Where on the prey the hunter takes hold: the point of it nearest the
/// hunter's mouth (or, for talons, the bird's feet) as the catch is made,
/// so a kill taken from behind is carried by the rump and one taken from
/// the side by the back or the neck. Only the neck, head and trunk are
/// taken, never a leg, a tail or an ear; talons take the trunk.
fn grab(world: &hecs::World, hunter: hecs::Entity, victim: hecs::Entity, models: &[Model], talons: bool) -> Grip {
    let fallback = if talons { Grip::Back } else { Grip::Scruff };
    let Ok(cl) = world.get::<&Look>(hunter) else { return fallback };
    let mut fk = Vec::new();
    let reach = match talons {
        true => Some(cl.pos),
        false => mouth(world, hunter, &cl, models, &mut fk),
    };
    let (Some(reach), Ok(vl), Ok(anim)) = (reach, world.get::<&Look>(victim), world.get::<&Anim>(victim)) else {
        return fallback;
    };
    let Some(l) = anim.limp.as_ref() else { return fallback };
    let local = Quat::from_rotation_y(-vl.yaw) * (reach - vl.pos);
    let roles: &[Role] = if talons { &[Role::Root, Role::Spine] } else { &[Role::Neck, Role::Head, Role::Root, Role::Spine] };
    l.nearest(local, roles)
}

/// Where the carrier's mouth is in the world, as it is drawn: a muzzle's
/// length in front of its head joint along the way the head faces.
fn mouth(world: &hecs::World, by: hecs::Entity, cl: &Look, models: &[Model], fk: &mut Vec<(Vec3, Quat)>) -> Option<Vec3> {
    let cm = models.get(cl.model)?;
    let Some(rig) = cm.rig.as_ref() else {
        return Some(cl.pos + Vec3::Y * cm.height * 0.7);
    };
    let scale = world.get::<&crate::variety::Morph>(by).map_or(1.0, |m| m.scale);
    let pose = world.get::<&Anim>(by).map(|a| a.pose.clone()).unwrap_or_else(|_| crate::anim::Pose::rest(rig.bones.len()));
    crate::anim::forward(rig, &pose, cm.voxel * scale, cm.pivot, fk);
    let head = rig.bones.iter().position(|b| b.role == Role::Jaw).or_else(|| rig.bones.iter().position(|b| b.role == Role::Head));
    // A head bone's joint is at the neck, so the mouth is a muzzle's length
    // in front of it, along the head's own facing (a model faces -Z).
    let local = head.and_then(|h| fk.get(h)).map_or(Vec3::Y * cm.height * scale * 0.7, |&(p, q)| p + q * Vec3::NEG_Z * (cm.height * scale * MUZZLE));
    Some(cl.pos + Vec3::Y * (cl.bob + cl.lift) + Quat::from_rotation_y(cl.yaw) * local)
}

/// Move every carried body with its carrier, and let go of the ones whose
/// carrier has finished with them or has gone.
pub fn update(world: &mut hecs::World, models: &[Model], dt: f32, now: f32, terrain: &Terrain) {
    // Where each carried body is held, in the world.
    let mut moves: Vec<(hecs::Entity, Vec3, Grip)> = Vec::new();
    let mut drops: Vec<hecs::Entity> = Vec::new();
    let mut fk = Vec::new();
    for (e, c, _) in world.query::<(hecs::Entity, &Carried, &Look)>().iter() {
        let talons = world.get::<&Talons>(c.by).ok().map(|t| t.until);
        let beak = world.get::<&Beak>(c.by).ok().map(|b| b.until);
        let done = match (talons, beak) {
            (Some(until), _) | (None, Some(until)) => now > until,
            (None, None) => world.get::<&Critter>(c.by).map(|cr| cr.mood != Mood::Feed).unwrap_or(true),
        };
        let Ok(cl) = world.get::<&Look>(c.by) else {
            drops.push(e);
            continue;
        };
        if done {
            drops.push(e);
            continue;
        }
        let held = match talons {
            // In the talons, under the bird.
            Some(_) => Some(cl.pos),
            None => mouth(world, c.by, &cl, models, &mut fk),
        };
        if let Some(at) = held {
            moves.push((e, at, c.grip));
        }
    }

    for (e, held, grip) in moves {
        let Some((rig, ..)) = carcass::frame_of(world, e, models, false) else { continue };
        let Ok(mut look) = world.get::<&mut Look>(e) else { continue };
        let Ok(mut anim) = world.get::<&mut Anim>(e) else { continue };
        look.begin_step();
        look.lift = 0.0;
        look.prev_lift = 0.0;
        let (at, yaw) = (look.pos, look.yaw);
        let yq = Quat::from_rotation_y(yaw);
        let ground = carcass::frame_ground(terrain.store, terrain.land, at, yaw);
        let Some(limp) = anim.limp.as_mut() else { continue };
        limp.hold(rig, Some((grip, yq.inverse() * (held - at))));
        limp.step(dt, &ground, Vec3::ZERO);
        // The frame follows the hand that holds it, standing on the ground
        // under it, so the body's place in the world is where it is.
        let g = limp.grip_at(rig, grip);
        let by = Vec3::new(g.x, ground(g), g.z);
        limp.shift(by);
        let middle = at + yq * (by + limp.middle());
        look.pos = at + yq * by;
        anim.show_limp(rig);
        drop((look, anim));
        if let Ok(mut c) = world.get::<&mut Carcass>(e) {
            c.at = middle;
        }
    }

    for e in drops {
        if let Ok(by) = world.get::<&Carried>(e).map(|c| c.by) {
            let _ = world.remove_one::<Talons>(by);
            let _ = world.remove_one::<Beak>(by);
        }
        let _ = world.remove_one::<Carried>(e);
        // Let go: it falls from there and lies where it lands (carcass.rs
        // steps it until it is still).
        let Some((rig, ..)) = carcass::frame_of(world, e, models, false) else { continue };
        if let Ok(mut anim) = world.get::<&mut Anim>(e) {
            if let Some(l) = anim.limp.as_mut() {
                l.hold(rig, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kill is let go and eaten well before its bones are gone, so nothing
    /// is still being carried or eaten when it is due to be cleared away.
    #[test]
    fn a_kill_outlasts_being_carried() {
        let age = Age::fresh_kill(0.0);
        assert!(age.until > CARRY_S * 4.0 + EAT_S, "{age:?}");
        assert!(FULL_S > CARRY_S + EAT_S, "a hunter would take the next one before it had eaten this one");
    }
}
