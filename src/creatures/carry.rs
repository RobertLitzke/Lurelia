//! Prey in a predator's mouth.
//!
//! There is no pounce and no eating. A coyote that gets within a metre of a
//! rabbit has it: the rabbit stops being an animal, becomes a limp body
//! (anim/ragdoll.rs) and is pinned to the coyote's jaw until the coyote drops
//! it. Something small enough is carried clear of the ground; something as big
//! as the carrier is dragged, feet trailing. Either way the body is a ragdoll
//! rather than a pose, so it swings as the carrier walks.
//!
//! What is dropped stays a body for a minute and is bones after that
//! (age::Age::fresh_kill), or, if the creature is too small to leave bones
//! worth drawing, it goes.

use crate::anim::Role;
use super::age::Age;
use super::terrain::Terrain;
use super::walk::{Critter, Mood};
use super::{Creature, Look, Species};
use crate::anim::{ragdoll::Ragdoll, Anim};
use crate::models::Model;
use glam::Vec3;

/// How close a hunter has to get to take its prey.
pub const CATCH_M: f32 = 1.0;

/// The catch distance in use. `VG_CATCH_M=12` lets a hunter take prey across
/// the clearing and prints each kill, for testing without waiting on one.
pub fn catch_m() -> f32 {
    std::env::var("VG_CATCH_M").ok().and_then(|v| v.parse().ok()).unwrap_or(CATCH_M)
}
/// How long it carries a kill before dropping it.
pub const CARRY_S: f32 = 30.0;
/// How long after a kill before it hunts again.
pub const FULL_S: f32 = 180.0;
/// Prey at least this much of the carrier's own height is dragged, not carried.
const DRAG_FROM: f32 = 0.55;

/// Whether this prey is dragged along the ground rather than held clear of
/// it: a rabbit is carried, a deer is dragged.
pub fn dragged(prey_h: f32, carrier_h: f32) -> bool {
    prey_h >= carrier_h * DRAG_FROM
}
/// How much the carrier's pace shakes what it is carrying.
const JOSTLE: f32 = 0.5;
/// How far in front of the head joint the mouth is, in carrier heights.
const MUZZLE: f32 = 0.35;

/// Prey being carried off by the entity that caught it.
pub struct Carried {
    pub by: hecs::Entity,
    /// Dragged along the ground rather than held clear of it.
    pub drag: bool,
    /// How far behind the mouth a dragged body trails, metres.
    pub slack: f32,
}

/// Turn each caught animal into a body in its hunter's mouth. Called from
/// walk.rs once its own pass is done, since this despawns components.
pub fn take(world: &mut hecs::World, species: &[Species], models: &[Model], kills: &[(hecs::Entity, hecs::Entity)], now: f32) {
    for &(hunter, victim) in kills {
        // Two hunters can reach the same rabbit on the same step.
        if world.get::<&Carried>(victim).is_ok() || world.get::<&Age>(victim).is_ok_and(|a| a.dead()) {
            continue;
        }
        let Ok(model) = world.get::<&Look>(victim).map(|l| l.model) else {
            continue;
        };
        let Some(prey_m) = models.get(model) else {
            continue;
        };
        let Some(rig) = prey_m.rig.as_ref() else {
            continue;
        };
        let carrier_h = world.get::<&Look>(hunter).ok().and_then(|l| models.get(l.model).map(|m| m.height)).unwrap_or(1.0);
        let drag = dragged(prey_m.height, carrier_h);
        let limp = Ragdoll::new(rig, prey_m.voxel, victim.id() ^ 0x5add);
        // It is a body now: no mover, no collision box, no family.
        let _ = world.remove_one::<Critter>(victim);
        let _ = world.remove_one::<crate::physics::Body>(victim);
        let _ = world.remove_one::<super::herd::Kin>(victim);
        let _ = world.remove_one::<Anim>(victim);
        let leaves_bones = world.get::<&Creature>(victim).map(|c| species[c.species].ages.bones).unwrap_or(false);
        let mut age = Age::fresh_kill(now);
        if !leaves_bones {
            // Nothing worth drawing: it is gone when the carrier is done.
            age.bones_at = f32::MAX;
            age.until = now + super::age::CARCASS_S;
        }
        if std::env::var_os("VG_CATCH_M").is_some() {
            let who = world.get::<&Creature>(victim).map(|c| species[c.species].name.clone()).unwrap_or_default();
            eprintln!("VG_CATCH: took a {who}, {}", if drag { "dragged" } else { "carried" });
        }
        let mut anim = Anim::hold(limp.pose.clone());
        anim.ragdoll = Some(Box::new(limp));
        let _ = world.insert(
            victim,
            (
                age,
                Carried {
                    by: hunter,
                    // Dragged by the head: the body trails back from the
                    // mouth, so its middle sits about half its length
                    // behind. Carried: slung across the mouth.
                    drag,
                    slack: prey_m.height * if drag { 0.5 } else { 0.9 },
                },
                anim,
            ),
        );
    }
}

/// Move every carried body with its carrier, and drop the ones whose carrier
/// has finished with them or has gone.
pub fn update(world: &mut hecs::World, models: &[Model], dt: f32, now: f32, terrain: &Terrain) {
    // Where each carrier's mouth is, and how fast it is going.
    let mut moves: Vec<(hecs::Entity, Vec3, f32, f32)> = Vec::new();
    let mut drops: Vec<hecs::Entity> = Vec::new();
    let mut fk = Vec::new();
    for (e, c, _) in world.query::<(hecs::Entity, &Carried, &Look)>().iter() {
        let done = world.get::<&Critter>(c.by).map(|cr| cr.mood != Mood::Feed).unwrap_or(true);
        let Ok(cl) = world.get::<&Look>(c.by) else {
            drops.push(e);
            continue;
        };
        if done {
            drops.push(e);
            continue;
        }
        let Some(cm) = models.get(cl.model) else {
            continue;
        };
        let speed = (cl.pos - cl.prev_pos).length() / dt.max(1e-3);
        // The mouth: the head or jaw joint of the carrier, as it is drawn.
        let mouth = match cm.rig.as_ref() {
            Some(rig) => {
                let pose = world.get::<&Anim>(c.by).map(|a| a.pose.clone()).unwrap_or_else(|_| crate::anim::Pose::rest(rig.bones.len()));
                crate::anim::forward(rig, &pose, cm.voxel, cm.pivot, &mut fk);
                let head = rig.bones.iter().position(|b| b.role == Role::Jaw).or_else(|| rig.bones.iter().position(|b| b.role == Role::Head));
                // A head bone's joint is at the neck, so the mouth is a
                // muzzle's length in front of it, along the head's own facing
                // (a model faces -Z).
                let local = head.and_then(|h| fk.get(h)).map_or(Vec3::Y * cm.height * 0.7, |&(p, q)| p + q * Vec3::NEG_Z * (cm.height * MUZZLE));
                let yq = glam::Quat::from_rotation_y(cl.yaw);
                cl.pos + yq * local
            }
            None => cl.pos + Vec3::Y * cm.height * 0.7,
        };
        moves.push((e, mouth, cl.yaw, speed));
    }

    for (e, mouth, yaw, speed) in moves {
        let (drag, slack) = {
            let Ok(c) = world.get::<&Carried>(e) else {
                continue;
            };
            (c.drag, c.slack)
        };
        let Ok(mut look) = world.get::<&mut Look>(e) else {
            continue;
        };
        look.begin_step();
        // Carried: hanging across the mouth, head to one side. Dragged: feet
        // on the ground, trailing behind the mouth.
        // A model faces -Z, so the carrier's own back is +Z of its yaw.
        let behind = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * slack;
        look.pos = if drag {
            let at = mouth + behind;
            let g = terrain.height(at.x, at.z).unwrap_or(at.y);
            Vec3::new(at.x, g, at.z)
        } else {
            // Slung across the mouth, hanging under it.
            mouth - Vec3::Y * slack * 0.45
        };
        // Dragged head first, in line with the carrier; carried across it.
        look.yaw = if drag { yaw } else { yaw + std::f32::consts::FRAC_PI_2 };
        let model = look.model;
        drop(look);
        // Stepping the ragdoll with the carrier's pace is the jiggle.
        let Some(rig) = models.get(model).and_then(|m| m.rig.as_ref()) else {
            continue;
        };
        let Ok(mut anim) = world.get::<&mut Anim>(e) else {
            continue;
        };
        let shake = Vec3::new((now * 7.0).sin(), 0.0, (now * 5.0).cos()) * speed * JOSTLE;
        anim.step_ragdoll(rig, dt, shake);
    }

    for e in drops {
        let _ = world.remove_one::<Carried>(e);
        // It lies where it was let go. Nothing steps the ragdoll of a body
        // that is not in a mouth, so run it to a standstill here: waking it
        // and walking away left the carcass frozen in the pose it was
        // carried in, standing on nothing.
        let rig = world.get::<&Look>(e).ok().and_then(|l| models.get(l.model)).and_then(|m| m.rig.clone());
        if let (Some(rig), Ok(mut anim)) = (rig, world.get::<&mut Anim>(e)) {
            if let Some(r) = anim.ragdoll.as_mut() {
                r.wake();
                for _ in 0..600 {
                    r.step(&rig, 1.0 / 30.0, Vec3::ZERO);
                    if r.settled() {
                        break;
                    }
                }
            }
            anim.step_ragdoll(&rig, 1.0 / 30.0, Vec3::ZERO);
        }
        let ground = ground_under(world, e, terrain);
        if let (Ok(mut look), Some(g)) = (world.get::<&mut Look>(e), ground) {
            look.pos.y = g;
            look.prev_pos.y = g;
        }
    }
}

fn ground_under(world: &hecs::World, e: hecs::Entity, terrain: &Terrain) -> Option<f32> {
    let p = world.get::<&Look>(e).ok()?.pos;
    terrain.height(p.x, p.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;

    /// The heights the library actually gives these animals decide which of
    /// them is slung in a mouth and which is hauled.
    #[test]
    fn a_rabbit_is_carried_and_a_deer_is_dragged() {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let h = |n: &str| lib.creatures.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {n}")).height;
        let (coyote, cougar) = (h("coyote"), h("cougar"));
        assert!(!dragged(h("rabbit"), coyote), "a coyote carries a rabbit");
        assert!(!dragged(h("lizard"), coyote), "a coyote carries a lizard");
        assert!(dragged(h("deer"), cougar), "a cougar drags a deer");
        assert!(dragged(h("pronghorn"), cougar), "a cougar drags a pronghorn");
    }

    /// A kill is let go well before its bones are gone, so nothing is still
    /// being carried when it is due to be cleared away.
    #[test]
    fn a_kill_outlasts_being_carried() {
        let age = Age::fresh_kill(0.0);
        assert!(age.until > CARRY_S * 2.0, "{age:?}");
        assert!(FULL_S > CARRY_S, "a hunter would take the next one before dropping this one");
    }
}
