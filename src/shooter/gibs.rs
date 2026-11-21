//! A bot a blast killed comes apart: every part of its body, and the chest
//! lamp, flies off on its own, bounces on the floor and settles there until
//! the bot respawns.
//!
//! The parts are the same rigid voxel models the living body is drawn with
//! (`anim::push_parts_at`), so nothing new is modelled. Each one starts
//! where it was in the pose the bot died in, thrown outward from the middle
//! of the body and along whatever killed it, and tumbles as it goes.

use crate::world::material::AIR;
use crate::world::store::Store;
use glam::{Quat, Vec3};

/// Which part a chunk is: a bone of the bot's rig, or the chest lamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    Bone(usize),
    Lamp,
}

/// One flying piece.
#[derive(Clone, Debug)]
pub struct Gib {
    pub part: Part,
    /// The part's pivot in the world, and where it was last step.
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    /// Whole orientation in the world, yaw included.
    pub rot: Quat,
    pub prev_rot: Quat,
    /// Radians a second about each axis.
    pub spin: Vec3,
}

/// Of the speed into the floor, what comes back out of a bounce.
const BOUNCE: f32 = 0.38;
/// Of the speed along the floor, what a bounce keeps.
const SCRUB: f32 = 0.72;
/// Slower than this on the floor and a chunk stops.
const REST: f32 = 0.6;
/// Fastest a chunk is thrown, metres a second.
const FASTEST: f32 = 11.0;

/// A cheap repeatable number in 0..1 off two integers.
fn roll(a: u32, b: u32) -> f32 {
    let h = crate::world::noise::hash32(a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b));
    (h & 0xffff) as f32 / 65535.0
}

/// Break a body into chunks. `parts` is each part's pivot and orientation
/// in the world, `chest` the middle of the body, `fling` what the killing
/// blow put into it, `seed` anything that differs from death to death.
pub fn burst(parts: &[(Part, Vec3, Quat)], chest: Vec3, fling: Vec3, seed: u32) -> Vec<Gib> {
    parts
        .iter()
        .enumerate()
        .map(|(i, &(part, pos, rot))| {
            let r = |k: u32| roll(seed, i as u32 * 7 + k);
            let out = (pos - chest).normalize_or(Vec3::Y);
            let sideways = Vec3::new(r(0) - 0.5, 0.0, r(1) - 0.5) * 4.0;
            // Most of a blast's fling is for throwing a whole corpse, which
            // is heavy; a chunk given all of it hits the ceiling.
            let vel = fling * (0.18 + 0.14 * r(2)) + out * (2.0 + 2.5 * r(3)) + Vec3::Y * (1.5 + 3.0 * r(4)) + sideways * 0.6;
            let vel = vel.clamp_length_max(FASTEST);
            let spin = Vec3::new(r(5) - 0.5, r(6) - 0.5, r(7) - 0.5) * 22.0;
            Gib { part, pos, prev: pos, vel, rot, prev_rot: rot, spin }
        })
        .collect()
}

fn solid(store: &Store, p: Vec3) -> bool {
    let v = [(p.x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (p.z * 4.0).floor() as i32];
    store.get(v[0], v[1], v[2]).is_some_and(|m| m != AIR)
}

/// One step for every chunk: fall, tumble, bounce off whatever it meets one
/// axis at a time, and stop once it has nothing left in it.
pub fn step(gibs: &mut [Gib], store: &Store, dt: f32) {
    for g in gibs {
        g.prev = g.pos;
        g.prev_rot = g.rot;
        if g.vel == Vec3::ZERO && g.spin == Vec3::ZERO {
            continue;
        }
        g.vel += store.gravity.pull(g.pos) * dt;
        let mut landed = false;
        // One axis at a time, so a chunk that hits a wall slides down it
        // rather than sticking where it touched.
        for axis in [1usize, 0, 2] {
            let mut next = g.pos;
            next[axis] += g.vel[axis] * dt;
            if solid(store, next) {
                if axis == 1 && g.vel.y < 0.0 {
                    landed = true;
                }
                g.vel[axis] = -g.vel[axis] * BOUNCE;
            } else {
                g.pos = next;
            }
        }
        if landed {
            g.vel.x *= SCRUB;
            g.vel.z *= SCRUB;
            g.spin *= SCRUB;
            if g.vel.length() < REST {
                g.vel = Vec3::ZERO;
                g.spin = Vec3::ZERO;
            }
        }
        let turn = g.spin * dt;
        if turn.length_squared() > 0.0 {
            g.rot = (Quat::from_scaled_axis(turn) * g.rot).normalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    fn floor() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                s.chunks.insert((cx, 0, cz), Voxels::Uniform(0));
            }
        }
        for x in -40..40 {
            for z in -40..40 {
                for y in 0..4 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    /// Chunks fly apart, come down on the floor rather than through it, and
    /// stop there.
    #[test]
    fn chunks_scatter_land_and_stop() {
        let store = floor();
        let chest = Vec3::new(0.0, 2.0, 0.0);
        let parts: Vec<(Part, Vec3, Quat)> = (0..10)
            .map(|i| {
                let a = i as f32 * 0.63;
                (Part::Bone(i), chest + Vec3::new(a.cos() * 0.3, (i as f32 - 5.0) * 0.12, a.sin() * 0.3), Quat::IDENTITY)
            })
            .collect();
        let mut gibs = burst(&parts, chest, Vec3::new(6.0, 4.0, 0.0), 7);
        for _ in 0..600 {
            step(&mut gibs, &store, 1.0 / 60.0);
        }
        let spread = gibs.iter().map(|g| (g.pos - chest).with_y(0.0).length()).fold(0.0f32, f32::max);
        assert!(spread > 1.5, "the chunks stayed in a heap: {spread:.2} m");
        for g in &gibs {
            assert!(g.pos.y >= 0.99 && g.pos.y < 1.3, "a chunk is at {} m, not on the floor at 1 m", g.pos.y);
            assert_eq!(g.vel, Vec3::ZERO, "a chunk is still moving after ten seconds");
        }
        // Thrown along the blow: most of them went the way it pushed.
        let ahead = gibs.iter().filter(|g| g.pos.x > chest.x).count();
        assert!(ahead >= 7, "only {ahead} of 10 went the way the blast threw them");
    }
}
