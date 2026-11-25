//! A kill on the ground, and what eats it.
//!
//! A body a predator has finished carrying, or one a shot dropped, is a
//! carcass. It lies where it fell, limp (anim/limp.rs), with meat on it:
//! seconds of one animal eating, more on a deer than on a rabbit. Anything
//! that hunts its kind and is hungry comes and eats from it (walk.rs): the
//! one that killed it first, then whoever else finds it, and several at once
//! is fine, which is how a kill is shared. Each eater tugs at it now and then
//! as it feeds. When the meat is gone the bones are left, or nothing for an
//! animal too small to leave bones worth drawing; a carcass nobody eats goes
//! to bones on the usual clock (age.rs `CARCASS_S`).
//!
//! The body is stepped here until it is still, and once more when it turns
//! to bones, which lie on bone (`Bone::hull` of the skeleton) and so settle
//! lower than the body did.

use super::age::Age;
use super::terrain::{far_ground, voxel_ground, Terrain};
use super::Look;
use crate::anim::limp::{Grip, Limp};
use crate::anim::Anim;
use crate::models::Model;
use crate::world::store::Store;
use glam::{Quat, Vec3};

/// Seconds of eating on a carcass, per metre of the animal's height.
const MEAT_PER_M: f32 = 60.0;
/// How hard an eater pulls on what it is eating, metres a second.
const TUG: f32 = 1.4;
/// Seconds between tugs, on average.
const TUG_EVERY_S: f32 = 3.0;

pub struct Carcass {
    /// Seconds of one animal eating left on it.
    pub meat: f32,
    /// The middle of the body, world metres: where an eater heads for.
    pub at: Vec3,
    /// Gone to bones and lying as a skeleton.
    pub bones: bool,
}

impl Carcass {
    /// A fresh one, of an animal `height` metres tall, lying at `at`.
    pub fn fresh(height: f32, at: Vec3) -> Self {
        Carcass {
            meat: (height * MEAT_PER_M).clamp(8.0, 400.0),
            at,
            bones: false,
        }
    }
}

/// The ground under a world point: the voxels where they are loaded, the
/// generator's ground further out, and a long way down if neither.
pub fn ground_at(store: &Store, land: Option<&crate::world::fields::World>, p: Vec3) -> f32 {
    voxel_ground(store, p.x, p.z, p.y + 0.5)
        .map(|g| g.0)
        .or_else(|| far_ground(store, p.x, p.z))
        .or_else(|| land.map(|l| l.f.height(p.x, p.z)))
        .unwrap_or(p.y - 100.0)
}

/// What a limp body in the frame of `look` stands on: `ground_at`, turned
/// into that frame.
pub fn frame_ground<'a>(store: &'a Store, land: Option<&'a crate::world::fields::World>, at: Vec3, yaw: f32) -> impl Fn(Vec3) -> f32 + 'a {
    let yq = Quat::from_rotation_y(yaw);
    move |p: Vec3| ground_at(store, land, at + yq * p) - at.y
}

/// What a body of this model is limp with: its rig (the skeleton's once it
/// has gone to bones), voxel edge with its morph's size, and pivot.
pub fn frame_of<'m>(world: &hecs::World, e: hecs::Entity, models: &'m [Model], bones: bool) -> Option<(&'m crate::anim::Rig, f32, Vec3)> {
    let m = models.get(world.get::<&Look>(e).ok()?.model)?;
    let rig = if bones { m.skeleton.as_ref()? } else { m.rig.as_ref()? };
    let scale = world.get::<&crate::variety::Morph>(e).map_or(1.0, |v| v.scale);
    Some((rig, m.voxel * scale, m.pivot))
}

/// Make `e` a body: limp in the pose it was in, with nothing moving it but
/// what the caller does next.
pub fn go_limp(world: &mut hecs::World, e: hecs::Entity, models: &[Model]) -> bool {
    let Some((rig, voxel, pivot)) = frame_of(world, e, models, false) else {
        return false;
    };
    let rig = rig.clone();
    if world.get::<&Anim>(e).is_err() {
        let _ = world.insert_one(e, Anim::hold(crate::anim::Pose::rest(rig.bones.len())));
    }
    let Ok(mut anim) = world.get::<&mut Anim>(e) else { return false };
    // Nothing alive is left to move it.
    anim.life = None;
    anim.flight = None;
    anim.go_limp(&rig, voxel, pivot);
    anim.show_limp(&rig);
    anim.prev = anim.pose.clone();
    drop(anim);
    if let Ok(mut look) = world.get::<&mut Look>(e) {
        look.bob = 0.0;
    }
    true
}

/// One step of every carcass that nothing is holding or throwing: still
/// falling or sliding, pulled about by what is eating it, or just turned to
/// bones.
pub fn update(world: &mut hecs::World, models: &[Model], terrain: &Terrain, dt: f32, now: f32) {
    let land = terrain.land;
    let store = terrain.store;
    let mut gone_to_bones = Vec::new();
    for (e, look, anim, c, age, held, thrown) in world
        .query::<(
            hecs::Entity,
            &mut Look,
            &mut Anim,
            &mut Carcass,
            &Age,
            Option<&super::carry::Carried>,
            Option<&super::flung::Flung>,
        )>()
        .iter()
    {
        // Something else is moving it.
        if held.is_some() || thrown.is_some() {
            continue;
        }
        if age.boned(now) && !c.bones {
            gone_to_bones.push(e);
            continue;
        }
        let Some(l) = anim.limp.as_ref() else { continue };
        if l.settled() {
            look.begin_step();
            continue;
        }
        let Some(rig) = models.get(look.model).and_then(|m| if c.bones { m.skeleton.as_ref() } else { m.rig.as_ref() }) else {
            continue;
        };
        look.begin_step();
        let ground = frame_ground(store, land, look.pos, look.yaw);
        anim.step_limp(rig, dt, &ground, Vec3::ZERO);
        if let Some(l) = anim.limp.as_ref() {
            c.at = look.pos + Quat::from_rotation_y(look.yaw) * l.middle();
            if l.settled() {
                look.lift = crate::physics::drawn_ground_lift(store, look.pos);
                look.prev_lift = look.lift;
            }
        }
    }
    // Picked clean or rotted down: the bones are a body of their own, and
    // they lie lower than the animal did, on bone instead of flesh.
    for e in gone_to_bones {
        if let Ok(mut c) = world.get::<&mut Carcass>(e) {
            c.bones = true;
        }
        let Some((rig, voxel, pivot)) = frame_of(world, e, models, true) else { continue };
        let (at, yaw) = match world.get::<&Look>(e) {
            Ok(l) => (l.pos, l.yaw),
            Err(_) => continue,
        };
        let Ok(mut anim) = world.get::<&mut Anim>(e) else { continue };
        let mut limp = Limp::new(rig, voxel, pivot, &anim.pose);
        limp.run_out(&frame_ground(store, land, at, yaw));
        anim.limp = Some(Box::new(limp));
        anim.show_limp(rig);
        anim.prev = anim.pose.clone();
    }
}

/// `eater` takes `dt` seconds of eating off carcass `e`. It tugs at the
/// body now and then (`roll` is a number in 0..1 from the eater's own
/// dice), and when the meat runs out the carcass goes to bones, or goes.
pub fn bite(world: &mut hecs::World, models: &[Model], e: hecs::Entity, eater: Vec3, dt: f32, now: f32, roll: f32) {
    let Ok(mut c) = world.get::<&mut Carcass>(e) else { return };
    if c.bones {
        return;
    }
    c.meat -= dt;
    let empty = c.meat <= 0.0;
    let middle = c.at;
    drop(c);
    if let Ok(mut a) = world.get::<&mut Age>(e) {
        let leaves = a.bones_at < f32::MAX;
        match (empty, leaves) {
            // Picked clean.
            (true, true) => a.bones_at = a.bones_at.min(now),
            (true, false) => a.until = a.until.min(now),
            // Something is eating it: it does not rot out from under them.
            (false, true) => a.bones_at = a.bones_at.max(now + 5.0),
            (false, false) => a.until = a.until.max(now + 5.0),
        }
    }
    if empty {
        return;
    }
    if roll > dt / TUG_EVERY_S {
        return;
    }
    // A tug: the near end of it pulled toward the eater and a little up.
    let Some((rig, ..)) = frame_of(world, e, models, false) else { return };
    let rig = rig.clone();
    let Ok(yaw) = world.get::<&Look>(e).map(|l| l.yaw) else { return };
    let Ok(mut anim) = world.get::<&mut Anim>(e) else { return };
    let Some(l) = anim.limp.as_mut() else { return };
    let to = Vec3::new(eater.x - middle.x, 0.0, eater.z - middle.z).normalize_or_zero();
    let local = Quat::from_rotation_y(-yaw) * (to * TUG + Vec3::Y * TUG * 0.5);
    l.tug(&rig, Grip::Scruff, local);
}
