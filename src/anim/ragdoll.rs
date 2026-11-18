//! A body with nothing holding it up.
//!
//! Each bone is given an angular velocity and let go: gravity pulls whichever
//! way its own long axis is not already pointing down, the joint fights back a
//! little, and friction takes the motion out of it. Once everything has
//! stopped, the pose is held and costs nothing further.
//!
//! Three things use it. The deceased settle into it, so no two skeletons in a
//! field lie the same way. Prey in a predator's mouth keeps stepping, with the
//! carrier's own movement fed in, so it swings and jiggles as it is carried.
//! And an animal that sleeps on its side tips over into it and stays there.

use crate::anim::{Plan, Role};
use super::{bone_rot, Pose, Rig};
use crate::world::noise::Rng;
use glam::Vec3;

/// How far a joint of each role can be bent before it stops, radians.
fn limit(role: Role) -> f32 {
    match role {
        Role::Leg | Role::Arm => 1.2,
        Role::Neck | Role::Head => 1.0,
        Role::Tail => 1.5,
        // A serpent's body bends freely but not into a knot.
        Role::Spine => 0.6,
        Role::Wing => 1.3,
        Role::Ear => 0.8,
        Role::Jaw => 0.5,
        Role::Root => 1.7,
    }
}

/// How hard gravity turns a bone, and how fast the motion dies away.
const GRAVITY: f32 = 9.0;
const DAMPING: f32 = 3.2;
/// Below this much motion in every joint it is done moving.
const ASLEEP: f32 = 0.02;
/// How much gravity a joint that is still off the floor keeps once the body
/// is flat. Without it nothing at all pulls on the parts of a body that is
/// down, which is right for an arm lying on the floor and wrong for a leg
/// left pointing at the ceiling by a tumble, or a back arched by one: they
/// stayed there. Anything higher than `SAG_FROM` of hip height is not lying
/// on the floor, so it goes on falling. Kept to what is above hip height:
/// giving everything off the floor its weight back stands the body up again,
/// which is what switching gravity off when flat was for in the first place.
const SAG: f32 = 0.45;
const SAG_FROM: f32 = 1.0;
/// How hard the floor holds a limb that is lying on it. Gravity alone
/// cannot be turned down enough to keep a body down: however small it is
/// made, hanging straight is still the only place a bone stops, so the body
/// creeps upright given long enough -- and it is given twenty seconds. A
/// limb on the ground is held by friction, which is a threshold and not a
/// scale, so that is what this is.
const FLOOR_GRIP: f32 = 1.4;
/// How hard a thrown body's joints are drawn back toward their rest pose
/// once it is lying down, radians a second squared per radian of bend.
const RELAX: f32 = 9.0;
/// Steps a settle is allowed to take before it is called finished.
const MAX_STEPS: u32 = 600;
/// How far the body's own axis may be off horizontal and still count as
/// lying down: 0.34 is within twenty degrees. Above it a thrown body on the
/// floor keeps going over, because the floor cannot hold a corpse up on its
/// feet and one that comes down out of a tumble can land on them -- on its
/// head, too, which is why this is measured off the axis itself rather than
/// off the root's two angles. A body turned over in both of them at once
/// passed an `x`-or-`z` test while standing on a corner.
const DOWN: f32 = 0.34;
/// How hard the floor turns a body that is standing on it and should not be,
/// radians a second squared. This is a shove, not a nudge: against `DAMPING`
/// it settles at about five radians a second, so a body that comes down on
/// its feet is flat in a third of a second. The first try at 6 took a second
/// and a half, which is most of a respawn -- what you saw was a dead man
/// standing in the middle of the floor, which is what the whole thing was
/// meant to stop.
const TOPPLE: f32 = 18.0;
/// How high off the floor a lying body's lowest joint ends up, in hip
/// heights. A joint is the middle of a bone, not its underside, so a body
/// whose lowest joint sat exactly on the floor would be half a limb into it.
const LIE_REST: f32 = 0.16;

/// Which way a bone of this plan points when it is at rest: down the body for
/// most animals, back along the chain for a serpent (anim/spine.rs). Gravity's
/// pull across a bone is worked out from this, so a snake that lets go of a
/// branch flails instead of standing to attention.
fn rest_axis(rig: &Rig) -> Vec3 {
    if rig.plan == Some(Plan::Serpent) {
        Vec3::Z
    } else {
        Vec3::NEG_Y
    }
}

pub struct Ragdoll {
    pub pose: Pose,
    /// Pitch and roll rate per bone, radians a second.
    vel: Vec<(f32, f32)>,
    /// Hip height in metres: the scale the resting height is measured in.
    leg_m: f32,
    /// Kept so the body can be measured against the floor as it falls.
    voxel: f32,
    /// Scratch for that measurement, so settling does not allocate 600 times.
    fk: Vec<(Vec3, glam::Quat)>,
    /// How high each joint was above the floor at the last measurement,
    /// metres, which is how a limb that is still in the air is told from one
    /// lying on the ground.
    up: Vec<f32>,
    settled: bool,
    /// In the air: thrown by a blast and not down yet. Nothing holds it up,
    /// nothing lies on the floor, and the root turns past its joint stops.
    airborne: bool,
    /// Which way it goes over when it does land: whether the fall is about
    /// x, and which way. Fixed at birth so one body always flops the same
    /// way however many times it bounces.
    fall: (bool, f32),
    /// Thrown rather than dropped. A body that was dropped is lying down
    /// from the first step (`new`) and has nothing left to do about it; one
    /// that was thrown left its feet upright and can come down on them.
    thrown: bool,
}

impl Ragdoll {
    /// Limp from this moment, with a small shove so no two fall alike.
    pub fn new(rig: &Rig, voxel: f32, seed: u32) -> Self {
        let mut rng = Rng::new(seed | 1);
        let n = rig.bones.len();
        let mut pose = Pose::rest(n);
        // The body itself goes over onto a side, and takes everything with it.
        // A serpent has no side to go over onto: it is already down.
        let serpent = rig.plan == Some(Plan::Serpent);
        if !serpent {
            // Over onto a side, or forwards, or backwards. It used to be a
            // side only, which for a four-legged animal is the whole
            // repertoire but for a person is a tell: nobody who has been
            // shot falls sideways every time. The grounding below works off
            // the pose rather than off the roll, so any of the four lands
            // the body correctly.
            let how = (seed >> 1) & 3;
            let far = 1.2 + rng.unit() * 0.4;
            let lean = rng.signed() * 0.25;
            pose.bones[0] = match how {
                0 => Vec3::new(lean, 0.0, far),
                1 => Vec3::new(lean, 0.0, -far),
                2 => Vec3::new(far, 0.0, lean),
                _ => Vec3::new(-far, 0.0, lean),
            };
        }
        let vel = (0..n).map(|_| (rng.signed() * 1.5, rng.signed() * 1.5)).collect();
        let mut r = Self {
            pose,
            vel,
            leg_m: if serpent { 0.0 } else { super::motors::hip(rig) * voxel },
            voxel,
            fk: Vec::new(),
            up: Vec::new(),
            settled: false,
            airborne: false,
            fall: ((seed >> 3) & 1 == 0, if (seed >> 4) & 1 == 0 { 1.0 } else { -1.0 }),
            thrown: false,
        };
        r.rest_on_ground(rig);
        r
    }

    /// Thrown: a blast has taken the body off its feet at `speed` metres a
    /// second. It leaves the ground in the attitude it was standing in and
    /// tumbles from there, instead of going straight over onto a side, and
    /// nothing brings it down until `aloft(false)` says it has landed.
    pub fn hurl(&mut self, rig: &Rig, speed: f32, seed: u32) {
        if self.leg_m <= 0.0 {
            // A serpent has no feet to be taken off.
            return;
        }
        let mut rng = Rng::new(seed | 1);
        // How fast the body itself turns: a shove rolls it over, a rocket
        // spins it. Capped, or a point-blank burst turns it into a drill.
        let rate = (speed * 0.5).clamp(1.5, 9.0);
        // Some of the turn in each axis and all of it in neither, so no two
        // bodies tumble the same way, but every one of them tumbles: a
        // signed roll straight off the generator can come back at a
        // hundredth of the rate and the body sails flat across the room.
        let spin = |r: &mut Rng| ((0.45 + 0.55 * r.unit()) * rate) * if r.unit() < 0.5 { -1.0 } else { 1.0 };
        self.vel[0] = (spin(&mut rng), spin(&mut rng));
        // The limbs flail, the harder the faster it is going, but in
        // proportion to what each joint can do: a spine bends 0.6 radians
        // and an arm twice that, so one flail for all of them pinned every
        // back at its stop. And gently. A body that is down does not get its
        // joints back: whatever the flail bends stays bent, because a flat
        // body's limbs are held by the floor rather than by gravity, so a
        // hard flail does not read as a tumble, it leaves a corpse arched
        // like a bridge with its head thrown back. The tumble is the whole
        // body turning (`vel[0]`); this is only the limbs trailing.
        let flail = (speed * 0.1).clamp(0.4, 2.0);
        for (i, v) in self.vel.iter_mut().enumerate().skip(1) {
            let room = limit(rig.bones[i].role) / 1.2;
            v.0 += rng.signed() * flail * room;
            v.1 += rng.signed() * flail * room;
        }
        self.pose.bones[0] = Vec3::new(rng.signed() * 0.3, 0.0, rng.signed() * 0.3);
        self.pose.root.y = 0.0;
        self.airborne = true;
        self.thrown = true;
        self.settled = false;
    }

    /// Off the ground, or back on it.
    pub fn aloft(&mut self, flying: bool) {
        if flying == self.airborne {
            return;
        }
        self.airborne = flying;
        self.settled = false;
        if flying {
            return;
        }
        // Landed. Whatever whole turns the tumble made are nothing to a body
        // on the floor, so the root comes back into the half turn either
        // side of upright and then into what a joint can hold, and most of
        // the spin is left in the ground.
        let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let lim = limit(Role::Root);
        let e = &mut self.pose.bones[0];
        e.x = wrap(e.x).clamp(-lim, lim);
        e.z = wrap(e.z).clamp(-lim, lim);
        self.vel[0].0 *= 0.3;
        self.vel[0].1 *= 0.3;
    }

    /// The torque that puts a body that is standing on the floor down on it.
    /// Zero once it is lying, and zero in the air, where there is no floor
    /// to be held up by.
    ///
    /// It turns the body's own axis toward horizontal, whichever way it is
    /// already leaning -- which also deals with one that came down on its
    /// head, since that axis has as far to go as a body on its feet. Only a
    /// body standing dead straight has no lean to follow, and that one falls
    /// the way it was always going to.
    fn topple(&self, rig: &Rig) -> (f32, f32) {
        if !self.thrown || self.airborne {
            return (0.0, 0.0);
        }
        let dir = self.direction(rig, 0);
        if dir.y.abs() < DOWN {
            return (0.0, 0.0);
        }
        let tilt = (dir.x * dir.x + dir.z * dir.z).sqrt();
        if tilt < 0.1 {
            return match self.fall {
                (true, s) => (s * TOPPLE, 0.0),
                (false, s) => (0.0, s * TOPPLE),
            };
        }
        // Gravity's own torque turns a bone until it hangs; this is that
        // turned around, so the axis leaves vertical instead of finding it,
        // and signed so a body on its head comes down the same way.
        let g = if dir.y < 0.0 { -TOPPLE } else { TOPPLE };
        (g * dir.z / tilt, -g * dir.x / tilt)
    }

    /// Bring the body down onto the floor it fell on.
    ///
    /// Measured off the pose, not off a constant. The constant this replaced
    /// was a fraction of hip height keyed on the roll of the root bone, and
    /// it was right for the shape it was written for: a quadruped's root is
    /// in the middle of a horizontal barrel, so rolling it alone leaves the
    /// animal standing at hip height and most of that height has to come
    /// off. A biped's root is a pelvis on top of two legs that go on hanging
    /// below it, so the same figure put the body half a metre into the
    /// ground -- measured at 0.57 m on the bots, which is what "the ragdolls
    /// sink" was. Keying on roll also meant a body that went over by pitch
    /// got no drop at all and stood there.
    ///
    /// What every shape agrees on is where the body ends up, so that is what
    /// this says: the lowest joint in it rests just above the floor.
    fn rest_on_ground(&mut self, rig: &Rig) {
        if self.leg_m <= 0.0 {
            // A serpent is already lying down and has no hip to measure in.
            return;
        }
        self.pose.root.y = 0.0;
        // Only the y of this is read, and y does not depend on the pivot, so
        // there is no pivot to be wrong about here.
        super::forward(rig, &self.pose, self.voxel, Vec3::ZERO, &mut self.fk);
        let low = self.fk.iter().map(|(p, _)| p.y).fold(f32::MAX, f32::min);
        // Never upward: a body that is still standing is not lifted off the
        // floor by being dead.
        self.pose.root.y = (LIE_REST * self.leg_m - low).min(0.0);
        self.up.clear();
        self.up.extend(self.fk.iter().map(|(p, _)| p.y + self.pose.root.y));
    }

    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Let it start moving again: it has just been dropped.
    pub fn wake(&mut self) {
        self.settled = false;
    }

    /// One fixed step. `shake` is the carrier's acceleration in the creature's
    /// own frame, metres a second squared, for prey being carried; zero for
    /// anything lying still.
    pub fn step(&mut self, rig: &Rig, dt: f32, shake: Vec3) {
        if self.settled && shake.length_squared() < 0.01 {
            return;
        }
        // How far over the body is, 0 upright and 1 flat on the floor.
        //
        // Once it is down, the floor is holding the limbs up, and that has
        // to be said somewhere or the body stands back up. Gravity here
        // turns every bone until it hangs, which is right for a carcass
        // swinging from a mouth and wrong for one on the ground: a pelvis
        // rolled ninety degrees leaves its legs pointing sideways, gravity
        // swings them down again, and the body ends up standing with a
        // twisted hip. It then has to be shoved into the floor to look
        // dead, which is what the old fixed drop was doing and why corpses
        // sank. A body on its back has its arms out beside it.
        //
        // A body in the air is held up by nothing and lying on nothing, so
        // it is never flat however far over it has turned.
        let flat = match self.airborne {
            true => 0.0,
            false => (self.pose.bones[0].x.abs().max(self.pose.bones[0].z.abs()) / std::f32::consts::FRAC_PI_2).clamp(0.0, 1.0),
        };
        let held_up = 1.0 - flat;
        let over = self.topple(rig);
        let mut moving = false;
        for i in 0..rig.bones.len() {
            // A body turning through the air has no joint stop to reach: the
            // root goes right round if the blast was hard enough.
            let lim = match i == 0 && self.airborne {
                true => f32::MAX,
                false => limit(rig.bones[i].role),
            };
            // The bone's own direction, by the pose so far. A bone at rest
            // points down, so gravity's pull across it is strongest when it
            // is horizontal and nothing at all when it already hangs.
            // Turning about x by +a tips it toward -z, about z by +a toward
            // +x, which gives the two torques below.
            let dir = self.direction(rig, i);
            // The body is held up by the ground, not hanging from a joint:
            // it keeps the roll it was given and only loses its wobble.
            // A limb still in the air keeps some of its weight however far
            // over the body is (`SAG`).
            let held = match self.up.get(i) {
                Some(&y) if y > SAG_FROM * self.leg_m => held_up.max(SAG),
                _ => held_up,
            };
            // Nothing weighs anything in free fall: a body in the air moves
            // its limbs by what it is doing, not by gravity. Pulling on them
            // all the way down pinned every joint at its stop by the time it
            // landed, and a corpse arched backwards with its arms out is
            // what "they float and stay rigid" looked like from the floor.
            let g = if i == 0 || self.airborne { 0.0 } else { GRAVITY * held };
            let mut torque_p = g * dir.z;
            let mut torque_r = -g * dir.x;
            // The floor putting a body that is still on its feet down.
            if i == 0 {
                torque_p += over.0;
                torque_r += over.1;
            }
            // Lying on the floor and not being pulled hard enough to slide:
            // it stays where it fell.
            if torque_p.hypot(torque_r) < FLOOR_GRIP * flat {
                torque_p = 0.0;
                torque_r = 0.0;
            }
            // A thrown body relaxing as it comes to rest. A flat body's
            // limbs are held by the floor and not by gravity, so whatever a
            // tumble bent stays bent -- and a corpse propped on two bent
            // legs with its head thrown back is what a throw left behind.
            // This is the only thing that straightens it: toward the rest
            // pose, the more of it the flatter the body is, so a body that
            // was dropped rather than thrown never feels it (nothing is
            // bent) and one still turning in the air is not tidied up
            // mid-flight.
            if i > 0 && self.thrown && !self.airborne {
                let e = self.pose.bones[i];
                torque_p -= e.x * RELAX * flat;
                torque_r -= e.z * RELAX * flat;
            }
            let (vp, vr) = &mut self.vel[i];
            *vp += (torque_p + shake.z * 0.4) * dt;
            *vr += (torque_r + shake.x * 0.4) * dt;
            // A body turning in the air has nothing to turn against: only
            // the limbs, which are swinging on their joints, are damped.
            // Damping the body as well was why a thrown corpse crossed the
            // room facing the way it set off, which is not a tumble, it is
            // a thrown plank.
            if !(i == 0 && self.airborne) {
                *vp -= *vp * DAMPING * dt;
                *vr -= *vr * DAMPING * dt;
            }
            let e = &mut self.pose.bones[i];
            e.x = (e.x + *vp * dt).clamp(-lim, lim);
            e.z = (e.z + *vr * dt).clamp(-lim, lim);
            // A joint at its stop has nowhere left to go.
            if e.x.abs() >= lim - 1e-4 {
                *vp = 0.0;
            }
            if e.z.abs() >= lim - 1e-4 {
                *vr = 0.0;
            }
            moving |= vp.abs() > ASLEEP || vr.abs() > ASLEEP;
        }
        // Nothing to come down onto while it is still flying: the body turns
        // about where its hips were and the floor is found when it lands.
        if !self.airborne {
            self.rest_on_ground(rig);
        }
        self.settled = !moving && !self.airborne;
    }

    /// Which way bone `i` points in the creature's frame, by the pose so far.
    fn direction(&self, rig: &Rig, i: usize) -> Vec3 {
        let mut q = bone_rot(self.pose.bones[i]);
        let mut p = rig.bones[i].parent;
        let mut guard = 0;
        while let Some(k) = p {
            q = bone_rot(self.pose.bones[k]) * q;
            p = rig.bones[k].parent;
            guard += 1;
            if guard > 8 {
                break;
            }
        }
        (q * rest_axis(rig)).normalize_or_zero()
    }
}

/// A ragdoll run to a standstill: what bones that have been lying in a field
/// look like. Deterministic in `seed`, so the same skeleton always lies the
/// same way.
pub fn settled(rig: &Rig, voxel: f32, seed: u32) -> Pose {
    let mut r = Ragdoll::new(rig, voxel, seed);
    for _ in 0..MAX_STEPS {
        r.step(rig, 1.0 / 30.0, Vec3::ZERO);
        if r.settled() {
            break;
        }
    }
    r.pose
}

/// Lying on one side: the pose an animal that sleeps that way tips into.
/// `side` is -1 or 1.
pub fn on_side(rig: &Rig, voxel: f32, side: f32, seed: u32) -> Pose {
    let mut r = Ragdoll::new(rig, voxel, seed);
    r.pose.bones[0].z = side * 1.35;
    for _ in 0..MAX_STEPS {
        r.step(rig, 1.0 / 30.0, Vec3::ZERO);
        if r.settled() {
            break;
        }
    }
    // Keep the body where it was put; only the limbs were allowed to fall.
    r.pose.bones[0].z = side * 1.35;
    r.rest_on_ground(rig);
    r.pose
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;

    fn rigs() -> Vec<(String, Rig)> {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        lib.creatures
            .iter()
            .filter_map(|c| {
                let def = c.rig.as_ref()?;
                let names: Vec<&String> = c.colors.keys().collect();
                let resolve = |n: &str| {
                    if n == "air" {
                        Some(0u8)
                    } else {
                        names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
                    }
                };
                let whole = crate::world::shape::compile(&c.ops, [32, 32, 32], &resolve).mat;
                let s = crate::anim::slice::slice(def, &c.ops, &whole, &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
                Some((c.name.clone(), s.rig))
            })
            .collect()
    }

    #[test]
    fn every_creature_settles_and_stays_within_its_joints() {
        let all = rigs();
        assert!(all.len() > 20, "only {} rigs", all.len());
        for (name, rig) in &all {
            let mut r = Ragdoll::new(rig, 0.05, 0x1234);
            let mut steps = 0;
            while !r.settled() && steps < MAX_STEPS {
                r.step(rig, 1.0 / 30.0, Vec3::ZERO);
                steps += 1;
            }
            assert!(r.settled(), "{name}: still moving after {steps} steps");
            for (i, b) in rig.bones.iter().enumerate() {
                let lim = limit(b.role) + 1e-4;
                let e = r.pose.bones[i];
                assert!(e.x.abs() <= lim && e.z.abs() <= lim, "{name}: {} bent past its joint: {e:?}", b.name);
                assert!(e.x.is_finite() && e.z.is_finite(), "{name}: {} went to NaN", b.name);
            }
        }
    }

    #[test]
    fn no_two_settle_alike() {
        let (name, rig) = rigs().into_iter().find(|(n, _)| n == "deer" || n == "coyote").expect("a quadruped");
        let a = settled(&rig, 0.05, 1);
        let b = settled(&rig, 0.05, 2);
        assert_ne!(a.bones, b.bones, "{name}: two seeds settled identically");
        // And the same seed always gives the same pose.
        assert_eq!(settled(&rig, 0.05, 1).bones, a.bones);
    }

    #[test]
    fn being_carried_keeps_it_moving() {
        let (_, rig) = rigs().into_iter().find(|(n, _)| n == "rabbit").expect("a rabbit");
        let mut r = Ragdoll::new(&rig, 0.03, 7);
        for _ in 0..MAX_STEPS {
            r.step(&rig, 1.0 / 30.0, Vec3::ZERO);
        }
        assert!(r.settled());
        let before = r.pose.bones.clone();
        for _ in 0..30 {
            r.step(&rig, 1.0 / 30.0, Vec3::new(3.0, 0.0, 2.0));
        }
        assert_ne!(before, r.pose.bones, "a shake did not move it");
    }
}
