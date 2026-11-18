//! A carcass with nothing holding it up.
//!
//! A shot animal stops being an animal: `combat::shot::kill` takes its mover
//! away and gives it a limp ragdoll, and what is left is a thing with mass.
//! This is what happens to that thing -- it falls, a blast throws it, it hits
//! the wall and comes off it, and it slides to a stop (`physics::tumble`),
//! with the body's own acceleration fed to the ragdoll as `shake` so the
//! limbs trail and a landing throws them forward.
//!
//! Before this, a carcass stayed exactly where the animal was standing when
//! it died, so a bird shot out of the air hung there and a goat shot on a
//! ledge stood on nothing.

use super::Look;
use crate::models::Model;
use crate::physics::{self, Body};
use crate::world::store::Store;
use glam::Vec3;

/// Metres a second below which a blow is not worth throwing a body with: it
/// drops where it stood.
pub const FLUNG: f32 = 1.5;
/// Seconds of stillness before a carcass is left alone. Not nothing, because
/// a body that has stopped against a ledge may still be given a shove by the
/// next burst.
const QUIET_S: f32 = 0.4;

/// A carcass still moving: the box it fell with, and how long it has been
/// still. Dropped from the entity once it has settled.
pub struct Flung {
    pub body: Body,
    pub quiet: f32,
}

/// Fall, bounce, slide and settle every carcass that is still moving.
pub fn update(world: &mut hecs::World, models: &[Model], store: &Store, dt: f32) {
    let mut done: Vec<hecs::Entity> = Vec::new();
    for (e, look, flung, anim, held) in world
        .query::<(hecs::Entity, &mut Look, &mut Flung, Option<&mut crate::anim::Anim>, Option<&super::carry::Carried>)>()
        .iter()
    {
        // Something else is holding it: a hunter has it in its mouth, and
        // carry.rs puts it where the mouth is (`creatures::carry`).
        if held.is_some() {
            done.push(e);
            continue;
        }
        look.begin_step();
        let was = flung.body.vel;
        let moving = physics::tumble(store, &mut look.pos, &mut flung.body, dt);
        flung.quiet = if moving { 0.0 } else { flung.quiet + dt };
        if flung.quiet > QUIET_S {
            done.push(e);
        }
        // The lift that puts feet on drawn, smoothed ground is set by the
        // mover, and a carcass has none: left as it was, a body that died on
        // a slope and slid onto the flat kept the slope's lift and hovered.
        look.lift = match flung.body.on_ground {
            true => physics::drawn_ground_lift(store, look.pos),
            false => 0.0,
        };
        let Some(anim) = anim else { continue };
        let Some(rig) = models.get(look.model).and_then(|m| m.rig.as_ref()) else {
            continue;
        };
        if let Some(r) = anim.ragdoll.as_mut() {
            r.aloft(!flung.body.on_ground);
        }
        // What the body is doing to the limbs hanging off it, in its own
        // frame: hitting the ground is what throws them forward.
        let acc = (flung.body.vel - was) / dt.max(1e-4);
        let (sy, cy) = look.yaw.sin_cos();
        let shake = Vec3::new(acc.x * cy - acc.z * sy, acc.y, -acc.x * sy - acc.z * cy).clamp_length_max(crate::anim::figure::SHAKE_MOST);
        anim.step_ragdoll(rig, dt, shake);
    }
    for e in done {
        let _ = world.remove_one::<Flung>(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    /// A floor of stone at y = 2, with air above it.
    fn floor() -> Store {
        let mut s = Store::default();
        for cz in -2..=2 {
            for cy in 0..=1 {
                for cx in -2..=2 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -40..40 {
            for z in -40..40 {
                for y in 0..8 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    /// A carcass thrown by a blast lands on the floor and stops there. Before
    /// this it stayed exactly where the animal was standing, so one shot off
    /// a ledge or out of the air hung in the sky.
    #[test]
    fn a_shot_carcass_falls_and_comes_to_rest() {
        let store = floor();
        let mut world = hecs::World::new();
        let mut body = Body::new(0.3, 0.8);
        body.vel = Vec3::new(7.0, 4.0, 0.0);
        let from = Vec3::new(0.0, 6.0, 0.0);
        let e = world.spawn((Look::new(0, from, 0.0), Flung { body, quiet: 0.0 }));
        for _ in 0..200 {
            update(&mut world, &[], &store, 1.0 / 30.0);
        }
        let at = world.get::<&Look>(e).map(|l| l.pos).unwrap();
        assert!((at.y - 2.0).abs() < 0.2, "it came to rest at y {:.2}, not on the floor at 2", at.y);
        assert!(at.x > 2.0, "the blast moved it only {:.1} m", at.x);
        assert!(world.get::<&Flung>(e).is_err(), "it is still being stepped after it stopped");
    }
}
