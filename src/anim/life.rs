//! What a rigged creature is doing and how far through it: the activity
//! scheduler (a weighted table of what follows what), the posture level that
//! moves between standing, sitting and lying, stride phase and gait, where
//! the head is looking, and the ground tilt. `compose` turns that state into a
//! `Pose` with the motors.

use super::motors::{self, Gait};
use super::{Pose, Rig};
use crate::world::noise::Rng;
use glam::{Quat, Vec3};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Stand,
    Walk,
    Graze,
    Look,
    Sit,
    Lie,
    /// Off to the nearest water and head down at the edge (walk.rs finds the
    /// water; here it is a graze-like overlay once there).
    Drink,
}

/// Every activity, in the order of the enum (`Act as usize` indexes tables).
pub const ACTS: [Act; 7] = [Act::Stand, Act::Walk, Act::Graze, Act::Look, Act::Sit, Act::Lie, Act::Drink];

/// One number per activity, in ACTS order (an individual's habits).
pub type Habits = [f32; ACTS.len()];

/// Which default activity table a movement verb starts from
/// (creatures/locomotion.rs names one for each verb).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Routine {
    /// Grazers and anything else.
    Graze,
    Hop,
    Skulk,
    Still,
}

impl Act {
    pub fn name(self) -> &'static str {
        match self {
            Act::Stand => "stand",
            Act::Walk => "walk",
            Act::Graze => "graze",
            Act::Look => "look",
            Act::Sit => "sit",
            Act::Lie => "lie",
            Act::Drink => "drink",
        }
    }
    pub fn parse(s: &str) -> Option<Act> {
        ACTS.iter().copied().find(|a| a.name() == s)
    }
    fn level(self) -> f32 {
        match self {
            Act::Sit => motors::SIT,
            Act::Lie => motors::LIE,
            _ => motors::STAND,
        }
    }
}

/// One row of a species' activity table, as written in a rig's `activities`.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct ActDef {
    /// Seconds, low and high.
    pub time: [f32; 2],
    /// Activity name -> weight.
    #[serde(default)]
    pub next: HashMap<String, f32>,
}

#[derive(Clone, Debug)]
struct Entry {
    time: [f32; 2],
    next: Vec<(Act, f32)>,
}

#[derive(Clone, Debug)]
pub struct Table {
    entries: Vec<Entry>,
    pub has_sit: bool,
}

impl Table {
    fn entry(&self, a: Act) -> &Entry {
        &self.entries[a as usize]
    }
}

type Row<'a> = (Act, [f32; 2], &'a [(Act, f32)]);

/// A table from rows named by activity, in any order; every activity needs one.
fn rows(r: &[Row]) -> Vec<Entry> {
    let mut e: Vec<Option<Entry>> = vec![None; ACTS.len()];
    for &(a, time, next) in r {
        e[a as usize] = Some(Entry { time, next: next.to_vec() });
    }
    e.into_iter().zip(ACTS).map(|(x, a)| x.unwrap_or_else(|| panic!("activity table has no {} row", a.name()))).collect()
}

/// The plan's and routine's defaults, overridden row by row by the rig.
pub fn table(plan: Option<super::Plan>, overrides: &HashMap<String, ActDef>, routine: Routine) -> Table {
    use Act::*;
    let mut e: Vec<Entry> = match (plan, routine) {
        // Crabs pick over the ground most of the time and rest flat.
        (Some(super::Plan::Crab), _) => rows(&[
            (Stand, [2.0, 6.0], &[(Walk, 4.0), (Graze, 5.0), (Look, 1.0), (Lie, 0.6)]),
            (Walk, [2.0, 6.0], &[(Stand, 2.0), (Graze, 3.0)]),
            (Graze, [4.0, 14.0], &[(Stand, 2.0), (Walk, 3.0)]),
            (Look, [1.5, 4.0], &[(Stand, 2.0), (Walk, 1.0)]),
            (Sit, [5.0, 15.0], &[(Stand, 1.0)]),
            (Lie, [15.0, 50.0], &[(Stand, 1.0)]),
            (Drink, [3.0, 6.0], &[(Stand, 1.0), (Graze, 1.0)]),
        ]),
        (Some(super::Plan::Sprawler), _) => rows(&[
            (Stand, [2.0, 6.0], &[(Walk, 3.0), (Lie, 3.0), (Look, 2.0)]),
            (Walk, [1.0, 4.0], &[(Stand, 3.0), (Lie, 1.0)]),
            (Graze, [2.0, 5.0], &[(Stand, 1.0)]),
            (Look, [1.5, 4.0], &[(Stand, 2.0), (Walk, 1.0)]),
            (Sit, [5.0, 15.0], &[(Stand, 1.0)]),
            // Basking flat on a warm rock.
            (Lie, [10.0, 40.0], &[(Stand, 1.0), (Look, 1.0)]),
            (Drink, [4.0, 8.0], &[(Stand, 1.0)]),
        ]),
        (_, Routine::Hop) => rows(&[
            (Stand, [1.5, 5.0], &[(Walk, 3.0), (Graze, 5.0), (Look, 2.0), (Sit, 1.0), (Lie, 0.3), (Drink, 0.3)]),
            (Walk, [1.0, 3.0], &[(Stand, 2.0), (Graze, 3.0)]),
            (Graze, [4.0, 12.0], &[(Stand, 2.0), (Walk, 3.0), (Look, 1.0)]),
            (Look, [1.5, 4.0], &[(Stand, 2.0), (Graze, 2.0), (Sit, 1.0)]),
            (Sit, [4.0, 14.0], &[(Stand, 1.0), (Graze, 1.0)]),
            (Lie, [15.0, 40.0], &[(Stand, 1.0)]),
            (Drink, [4.0, 8.0], &[(Stand, 1.0), (Graze, 1.0)]),
        ]),
        (_, Routine::Skulk) => rows(&[
            (Stand, [2.0, 5.0], &[(Walk, 5.0), (Look, 2.0), (Sit, 1.5), (Lie, 0.4), (Graze, 0.6), (Drink, 0.5)]),
            (Walk, [3.0, 10.0], &[(Stand, 3.0), (Look, 1.0), (Graze, 0.5)]),
            // Nosing at the ground.
            (Graze, [2.0, 6.0], &[(Stand, 2.0), (Walk, 2.0)]),
            (Look, [1.5, 4.0], &[(Stand, 2.0), (Walk, 2.0), (Sit, 1.0)]),
            (Sit, [5.0, 20.0], &[(Stand, 1.0), (Lie, 0.4), (Look, 0.5)]),
            (Lie, [20.0, 60.0], &[(Sit, 1.0), (Stand, 0.5)]),
            (Drink, [5.0, 10.0], &[(Stand, 1.0), (Walk, 1.0)]),
        ]),
        (_, Routine::Still) => rows(&[
            (Stand, [3.0, 8.0], &[(Look, 3.0), (Graze, 1.0), (Lie, 1.0)]),
            (Walk, [1.0, 2.0], &[(Stand, 1.0)]),
            (Graze, [3.0, 8.0], &[(Stand, 1.0)]),
            (Look, [2.0, 5.0], &[(Stand, 1.0)]),
            (Sit, [5.0, 15.0], &[(Stand, 1.0)]),
            (Lie, [20.0, 60.0], &[(Stand, 1.0)]),
            (Drink, [4.0, 8.0], &[(Stand, 1.0)]),
        ]),
        (_, Routine::Graze) => rows(&[
            (Stand, [2.0, 6.0], &[(Walk, 4.0), (Graze, 4.0), (Look, 2.0), (Lie, 0.4), (Drink, 0.7)]),
            (Walk, [3.0, 9.0], &[(Stand, 3.0), (Graze, 3.0), (Look, 1.0)]),
            (Graze, [5.0, 14.0], &[(Stand, 2.0), (Walk, 3.0), (Look, 1.0)]),
            (Look, [1.5, 4.0], &[(Stand, 2.0), (Graze, 2.0), (Walk, 1.0)]),
            (Sit, [5.0, 20.0], &[(Stand, 1.0)]),
            (Lie, [20.0, 60.0], &[(Stand, 1.0)]),
            (Drink, [6.0, 14.0], &[(Stand, 2.0), (Graze, 2.0), (Walk, 1.0)]),
        ]),
    };
    for (name, d) in overrides {
        let Some(a) = Act::parse(name) else { continue };
        e[a as usize] = Entry {
            time: d.time,
            next: d.next.iter().filter_map(|(n, w)| Act::parse(n).map(|a| (a, *w))).collect(),
        };
    }
    let has_sit = e.iter().any(|r| r.next.iter().any(|(a, w)| *a == Act::Sit && *w > 0.0));
    Table { entries: e, has_sit }
}

/// What the creature's body is doing this step, from entities.rs.
pub struct Input<'a> {
    pub dt: f32,
    /// Horizontal speed over the ground, m/s.
    pub speed: f32,
    /// The behaviour wants to move (walking, fleeing, chasing, rejoining).
    pub moving: bool,
    /// Fleeing.
    pub alarm: bool,
    pub on_ground: bool,
    pub pos: Vec3,
    pub yaw: f32,
    pub player: Vec3,
    /// Distance at which the player is noticed.
    pub wary: f32,
    /// Ground height under (x, z), if loaded.
    pub ground: &'a dyn Fn(f32, f32) -> Option<f32>,
    /// The water surface where it stands (a river, lake or the sea), if it is
    /// standing in or at water. A head going down stops here.
    pub water: Option<f32>,
    /// Drawn into its shell (the `plod` verb when the player is close).
    pub hide: bool,
    /// The model's pivot (model voxels) and this animal's size, for a tail
    /// dragged through the world (trail.rs).
    pub pivot: Vec3,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct Life {
    pub table: Table,
    pub act: Act,
    /// This individual's taste for each activity (variety.rs), in ACTS order.
    pub habits: Habits,
    /// Posture level now (0 stand, 1 sit, 2 lie).
    pub level: f32,
    pub phase: f32,
    pub gait: Gait,
    /// The gait being left and how far the change has gone (0..1).
    pub from_gait: Gait,
    pub gait_mix: f32,
    pub gait_w: f32,
    pub graze_w: f32,
    /// Head yaw and pitch now, and wanted.
    pub look: (f32, f32),
    look_want: (f32, f32),
    look_at: Option<Vec3>,
    look_timer: f32,
    pub alert: f32,
    pub alarm: f32,
    pub tilt: f32,
    /// 0 out, 1 drawn into its shell.
    pub hide: f32,
    pub t: f32,
    pub flick: f32,
    flick_timer: f32,
    seed: f32,
    rng: Rng,
    /// How freely it moves: 1 as usual, less for an old animal
    /// (variety::Morph::supple).
    pub supple: f32,
    /// Lies down on its side rather than upright (library `sleeps_on_side`),
    /// and which side (-1 or 1).
    pub on_side: bool,
    pub side: f32,
    /// The pose it falls into on its side, worked out once (anim/limp.rs)
    /// the first time it lies down, since settling a limp body is not something
    /// to do every frame.
    side_pose: Option<Pose>,
    /// A long tail dragged behind and a long neck that browses (trail.rs),
    /// found the first step.
    tail: Option<Box<super::trail::Tail>>,
    neck: Option<Box<super::trail::Neck>>,
    rigged: bool,
    /// The heading a step ago, and how fast it is turning, radians a second.
    last_yaw: Option<f32>,
    turn: f32,
}

impl Life {
    pub fn new(table: Table, seed: u32) -> Self {
        let mut l = Self {
            table,
            act: Act::Stand,
            habits: [1.0; ACTS.len()],
            level: 0.0,
            phase: 0.0,
            gait: Gait::Walk,
            from_gait: Gait::Walk,
            gait_mix: 1.0,
            gait_w: 0.0,
            graze_w: 0.0,
            look: (0.0, 0.0),
            look_want: (0.0, 0.0),
            look_at: None,
            look_timer: 0.0,
            alert: 0.0,
            alarm: 0.0,
            tilt: 0.0,
            hide: 0.0,
            t: 0.0,
            flick: 0.0,
            flick_timer: 2.0,
            seed: 0.0,
            rng: Rng::walker(seed | 1),
            supple: 1.0,
            on_side: false,
            side: 1.0,
            side_pose: None,
            tail: None,
            neck: None,
            rigged: false,
            last_yaw: None,
            turn: 0.0,
        };
        l.seed = l.rand() * 10.0;
        l.phase = l.rand();
        l.t = l.rand() * 100.0;
        l
    }

    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }

    /// The current activity is over: pick the next. Returns whether it walks,
    /// how long it lasts, and a turn of heading for a walk.
    pub fn choose(&mut self, was_moving: bool) -> (bool, f32, f32) {
        let from = if was_moving { Act::Walk } else { self.act };
        let e = self.table.entry(from).clone();
        let habits = self.habits;
        let weight = |a: Act, w: f32| w.max(0.0) * habits[a as usize];
        let total: f32 = e.next.iter().map(|x| weight(x.0, x.1)).sum();
        let mut pick = Act::Stand;
        if total > 0.0 {
            let mut r = self.rand() * total;
            for (a, w) in &e.next {
                r -= weight(*a, *w);
                if r <= 0.0 {
                    pick = *a;
                    break;
                }
            }
        }
        self.act = pick;
        let t = self.table.entry(pick).time;
        let dur = t[0] + self.rand() * (t[1] - t[0]).max(0.0);
        let turn = if pick == Act::Walk { (self.rand() - 0.5) * 2.5 } else { 0.0 };
        (pick == Act::Walk, dur, turn)
    }

    /// 1 when standing and free to move, 0 while still down or getting up.
    pub fn mobility(&self) -> f32 {
        if self.level < 0.15 && self.hide < 0.1 {
            1.0
        } else {
            0.0
        }
    }

    pub fn step(&mut self, rig: &Rig, voxel: f32, inp: &Input) {
        let dt = inp.dt;
        self.t += dt;
        if inp.moving && self.act != Act::Walk && self.act != Act::Drink {
            self.act = Act::Walk;
        } else if !inp.moving && self.act == Act::Walk {
            self.act = Act::Stand;
        }
        let k = |rate: f32| (dt * rate).min(1.0);
        self.alarm += (if inp.alarm { 1.0 } else { 0.0 } - self.alarm) * k(4.0);
        // In fast, out slowly.
        self.hide += (if inp.hide { 1.0 } else { 0.0 } - self.hide) * k(if inp.hide { 5.0 } else { 0.8 });

        // Posture: up and down through the levels; scrambling up when alarmed.
        let want = if self.table.has_sit || self.act != Act::Sit { self.act.level() } else { motors::STAND };
        let rate = if self.alarm > 0.3 || inp.moving { 4.5 } else { 1.4 };
        let d = want - self.level;
        self.level += d.signum() * d.abs().min(rate * dt);

        // Stride.
        let leg_m = motors::hip(rig) * voxel;
        let g = motors::gait_for(rig.plan, inp.speed, leg_m);
        if self.gait_w < 0.05 {
            self.gait = g;
            self.gait_mix = 1.0;
        }
        self.phase += inp.speed * dt / (self.gait.cycle_for(rig.plan) * leg_m).max(0.05);
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            if g != self.gait {
                self.from_gait = self.gait;
                self.gait = g;
                self.gait_mix = 0.0;
            }
        }
        self.gait_mix = (self.gait_mix + dt / 0.3).min(1.0);
        let moving = (inp.speed / 0.25).clamp(0.0, 1.0);
        self.gait_w += (moving - self.gait_w) * k(6.0);
        // Drinking: head down once it has stopped at the water. The muzzle
        // stops at the surface rather than at the bed, so an animal standing
        // in the shallows dips its mouth in instead of putting its whole head
        // under; one standing in water deeper than its head doesn't reach
        // down at all.
        let at_water = self.act == Act::Drink && !inp.moving;
        let head_m = (motors::hip(rig) * voxel * 1.6).max(0.25);
        let depth = inp.water.map_or(0.0, |y| (y - inp.pos.y).max(0.0));
        let reach = (1.0 - depth / head_m).clamp(0.0, 1.0);
        let grazing = if (self.act == Act::Graze || at_water) && self.level < 0.1 && self.alert < 0.5 {
            reach
        } else {
            0.0
        };
        self.graze_w += (grazing - self.graze_w) * k(2.0);

        // Where to look.
        let yq = Quat::from_rotation_y(inp.yaw);
        let to_player = inp.player - inp.pos;
        let noticed = Vec3::new(to_player.x, 0.0, to_player.z).length() < inp.wary * 1.6 && !inp.alarm;
        self.alert += (if noticed { 1.0 } else { 0.0 } - self.alert) * k(3.0);
        self.look_timer -= dt;
        if noticed {
            self.look_at = Some(inp.player + Vec3::Y * 1.5);
        } else if self.look_timer <= 0.0 {
            self.look_timer = 1.0 + self.rand() * 3.0;
            self.look_at = None;
            let (r0, r1, r2) = (self.rand(), self.rand(), self.rand());
            self.look_want = match self.act {
                Act::Look => ((r0 - 0.5) * 2.4, (r1 - 0.3) * 0.5),
                Act::Walk => ((r0 - 0.5) * 0.4, (r1 - 0.5) * 0.2),
                _ if r2 < 0.5 => (0.0, 0.0),
                _ => ((r0 - 0.5) * 1.4, (r1 - 0.5) * 0.4),
            };
        }
        if let Some(p) = self.look_at {
            // Into the creature's frame (it faces -Z).
            let l = yq.inverse() * (p - inp.pos - Vec3::Y * motors::hip(rig) * voxel * 1.6);
            let yaw = (-l.x).atan2(-l.z);
            let pitch = l.y.atan2(Vec3::new(l.x, 0.0, l.z).length());
            self.look_want = (yaw.clamp(-1.4, 1.4), pitch.clamp(-0.5, 0.6));
        }
        let lr = if self.alert > 0.5 { 8.0 } else { 3.5 };
        self.look.0 += (self.look_want.0 - self.look.0) * k(lr);
        self.look.1 += (self.look_want.1 - self.look.1) * k(lr);

        // Ears.
        self.flick_timer -= dt;
        if self.flick_timer <= 0.0 {
            self.flick_timer = 1.5 + self.rand() * 5.0;
            self.flick = 1.0;
        }
        self.flick = (self.flick - dt * 4.0).max(0.0);

        // Lean with the ground between the front and hind feet.
        let zs: Vec<(bool, f32)> = (0..rig.bones.len())
            .filter(|&i| motors::upper_leg(rig, i))
            .map(|i| (rig.bones[i].pair == 0, rig.bones[i].joint.z))
            .collect();
        let mean = |front: bool| -> Option<f32> {
            let v: Vec<f32> = zs.iter().filter(|x| x.0 == front).map(|x| x.1).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
        };
        let mut tilt_want = 0.0;
        if let (Some(zf), Some(zb)) = (mean(true), mean(false)) {
            if inp.on_ground && zb > zf {
                let f = inp.pos + yq * Vec3::new(0.0, 0.0, (zf - 16.0) * voxel);
                let b = inp.pos + yq * Vec3::new(0.0, 0.0, (zb - 16.0) * voxel);
                if let (Some(hf), Some(hb)) = ((inp.ground)(f.x, f.z), (inp.ground)(b.x, b.z)) {
                    tilt_want = ((hf - hb) / ((zb - zf) * voxel)).atan().clamp(-0.35, 0.35);
                }
            }
        }
        self.tilt += (tilt_want - self.tilt) * k(5.0);

        // Lying on its side is a limp body let down onto the ground, worked
        // out the first time this one lies down and kept.
        if self.on_side && self.level > 1.0 && self.side_pose.is_none() {
            self.side_pose = Some(super::limp::on_side(rig, voxel, inp.pivot, self.side, (self.seed * 1000.0) as u32 | 1));
        }

        // A long tail and neck bending out of line (trail.rs).
        if !self.rigged {
            self.rigged = true;
            self.tail = super::trail::Tail::new(rig).map(Box::new);
            self.neck = super::trail::Neck::new(rig).map(Box::new);
        }
        let turn_now = self.last_yaw.map_or(0.0, |y| {
            let d = crate::util::wrap_angle(inp.yaw - y);
            if dt > 0.0 { d / dt } else { 0.0 }
        });
        self.last_yaw = Some(inp.yaw);
        self.turn += (turn_now - self.turn) * k(3.0);
        if let Some(t) = self.tail.as_mut() {
            t.step(rig, voxel * inp.scale.max(0.05), inp.pivot, inp.pos, inp.yaw, dt);
        }
        if self.neck.is_some() {
            let r = self.rand();
            let (graze, turn, walk) = (self.graze_w, self.turn, self.gait_w);
            if let Some(n) = self.neck.as_mut() {
                n.step(dt, graze, turn, walk, r);
            }
        }
    }

    pub fn compose(&self, rig: &Rig, voxel: f32) -> Pose {
        let leg_m = motors::hip(rig) * voxel;
        let mut p = motors::posture(rig, self.level, self.table.has_sit, leg_m);
        let up = 1.0 - self.level.min(1.0);
        let w = self.gait_w * up * self.supple;
        motors::gait(rig, &mut p, self.gait, self.phase, w * self.gait_mix, leg_m);
        motors::gait(rig, &mut p, self.from_gait, self.phase, w * (1.0 - self.gait_mix), leg_m);
        motors::graze(rig, &mut p, self.graze_w, self.t);
        motors::claws(rig, &mut p, self.t, self.gait_w * up, self.graze_w, self.alarm);
        let free = 1.0 - self.graze_w;
        motors::look(rig, &mut p, self.look.0 * free, self.look.1 * free + 0.2 * self.alert * free);
        if let Some(t) = &self.tail {
            t.apply(rig, &mut p, up);
        }
        if let Some(n) = &self.neck {
            n.apply(&mut p);
        }
        motors::tail(rig, &mut p, self.t, self.seed, self.phase, self.gait_w, self.alarm);
        motors::ears(rig, &mut p, (self.flick * std::f32::consts::PI).sin() * self.supple, self.alert * self.supple);
        motors::breathe(&mut p, self.t, 2.0 + 3.0 * self.alarm, leg_m);
        p.bones[0].x += self.tilt;
        if rig.soft.is_some() {
            p.sag = super::flesh::sag(self.level);
        }
        motors::fold_wings(rig, &mut p);
        motors::retract(rig, &mut p, self.hide, leg_m);
        // All the way down, an animal that sleeps on its side is over.
        if let Some(side) = self.side_pose.as_ref() {
            let t = (self.level - 1.0).clamp(0.0, 1.0);
            if t > 0.0 {
                return p.lerp(side, t);
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;
    use crate::world::shape;

    /// The deer's rig, sliced from the library as worldload does it.
    fn deer() -> (Rig, f32, Routine) {
        let (lib, _) = Library::load(&crate::library::data_root());
        let c = lib.creatures.iter().find(|c| c.name == "deer").expect("deer");
        let names: Vec<&String> = c.colors.keys().collect();
        let resolve = |n: &str| {
            if n == "air" {
                Some(0)
            } else {
                names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
            }
        };
        let whole = shape::compile(&c.ops, c.grid, &resolve).mat;
        let s = super::super::slice::slice(c.rig.as_ref().expect("deer rig"), &c.ops, &whole, crate::anim::slice::Dims(c.grid), &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
        (s.rig, c.voxel, crate::creatures::locomotion::verb(&c.behaviour.movement).routine)
    }

    /// Every default table has a row for each activity, and `Act as usize`
    /// matches ACTS.
    #[test]
    fn default_tables_are_complete() {
        for (i, a) in ACTS.iter().enumerate() {
            assert_eq!(*a as usize, i);
        }
        for plan in [Some(super::super::Plan::Quadruped), Some(super::super::Plan::Sprawler)] {
            for r in [Routine::Graze, Routine::Hop, Routine::Skulk, Routine::Still] {
                assert_eq!(table(plan, &HashMap::new(), r).entries.len(), ACTS.len());
            }
        }
    }

    /// Ten simulated minutes of a deer on flat ground, driven the way
    /// entities.rs drives it: it spends time in several activities, gets up
    /// before walking, and no joint jumps between steps.
    #[test]
    fn a_deer_lives_smoothly() {
        let (rig, voxel, routine) = deer();
        let mut life = Life::new(rig.table(routine), 99);
        let dt = 1.0 / 60.0;
        let (mut timer, mut walking, mut speed) = (0.0f32, false, 0.0f32);
        let mut time_in: HashMap<&'static str, f32> = HashMap::new();
        let mut prev = life.compose(&rig, voxel);
        let mut worst = (0.0f32, String::new());
        let ground = |_: f32, _: f32| Some(0.0);
        for step in 0..(600 * 60) {
            timer -= dt;
            if timer <= 0.0 {
                let (w, dur, _) = life.choose(walking);
                walking = w;
                timer = dur;
            }
            // A three-second bolt every two minutes.
            let fleeing = step > 3600 && (step / 60) % 120 < 3;
            let want = if fleeing {
                7.5
            } else if walking {
                1.3
            } else {
                0.0
            } * life.mobility();
            speed += (want - speed) * (dt * 10.0).min(1.0);
            let inp = Input {
                dt,
                speed,
                moving: walking || fleeing,
                alarm: fleeing,
                on_ground: true,
                pos: Vec3::ZERO,
                yaw: 0.0,
                player: Vec3::new(500.0, 0.0, 0.0),
                wary: 10.0,
                ground: &ground,
                water: None,
                hide: false, pivot: Vec3::ZERO, scale: 1.0,
            };
            life.step(&rig, voxel, &inp);
            if (walking || fleeing) && life.level > 0.2 {
                assert!(life.mobility() == 0.0, "moved before getting up");
            }
            *time_in.entry(life.act.name()).or_insert(0.0) += dt;
            let pose = life.compose(&rig, voxel);
            for (i, (a, b)) in pose.bones.iter().zip(&prev.bones).enumerate() {
                let d = (*a - *b).abs().max_element();
                if d > worst.0 {
                    worst = (d, format!("{} at step {step} ({})", rig.bones[i].name, life.act.name()));
                }
            }
            prev = pose;
        }
        let mut mix: Vec<_> = time_in.iter().map(|(k, v)| (*k, (v / 6.0).round())).collect();
        mix.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("percent of time: {mix:?}; largest one-step change {:.3} rad, {}", worst.0, worst.1);
        assert!(time_in.len() >= 4, "only {mix:?}");
        assert!(worst.0 < 0.3, "a joint jumped {:.3} rad: {}", worst.0, worst.1);
    }

    /// Drinking in the shallows dips the mouth to the surface: standing in
    /// water the head goes down less far than on dry land, and in water
    /// deeper than its head it doesn't reach down at all.
    #[test]
    fn a_drinker_stops_at_the_surface() {
        let (rig, voxel, _) = deer();
        let dip = |water: Option<f32>| {
            let mut life = Life::new(table(None, &HashMap::new(), Routine::Graze), 5);
            life.act = Act::Drink;
            let ground = |_: f32, _: f32| Some(0.0);
            for _ in 0..240 {
                life.act = Act::Drink;
                life.step(
                    &rig,
                    voxel,
                    &Input {
                        dt: 1.0 / 60.0,
                        speed: 0.0,
                        moving: false,
                        alarm: false,
                        on_ground: true,
                        pos: Vec3::ZERO,
                        yaw: 0.0,
                        player: Vec3::new(500.0, 0.0, 0.0),
                        wary: 10.0,
                        ground: &ground,
                        water,
                        hide: false, pivot: Vec3::ZERO, scale: 1.0,
                    },
                );
            }
            life.graze_w
        };
        let dry = dip(None);
        let shallow = dip(Some(0.35));
        let deep = dip(Some(5.0));
        assert!(dry > 0.9, "head right down at the edge: {dry}");
        assert!(shallow < dry * 0.85 && shallow > 0.05, "a dip, not a ducking: {shallow} against {dry}");
        assert!(deep < 0.02, "out of its depth, the head stays up: {deep}");
    }
}
