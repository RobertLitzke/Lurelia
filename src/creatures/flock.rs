//! Boids: creatures that move as a group. A library creature opts in with its
//! movement verb and an optional `group` block in its behaviour:
//!
//! - `flock`: a flying flock (starlings). The birds wheel round a slowly
//!   wandering point, now and then come down together on open ground or a
//!   tree, and burst up and away when the player comes within `wary`.
//! - `soar`: a lone bird (an eagle) circling wide and high, never landing.
//! - any walking verb with `group`: a family group (deer in twos and threes)
//!   that grazes together and bolts together (herd.rs).
//!
//! Each bird steers by the three classic rules over its flockmates within
//! `range`: separation (keep `spacing` apart), alignment (fly the way the
//! neighbours fly) and cohesion (stay with them), plus a pull toward the
//! flock's goal and a spring toward its flying height above the ground.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;
use serde::Deserialize;

/// How a species groups up, from its library behaviour.
#[derive(Deserialize, Clone, Debug)]
pub struct Group {
    /// Smallest and largest group spawned.
    #[serde(default = "one_one")]
    pub size: [u32; 2],
    /// Distance kept between neighbours, metres.
    #[serde(default = "spacing")]
    pub spacing: f32,
    /// How far a member sees its neighbours, metres.
    #[serde(default = "range")]
    pub range: f32,
    #[serde(default = "unit_w")]
    pub separation: f32,
    #[serde(default = "unit_w")]
    pub alignment: f32,
    #[serde(default = "unit_w")]
    pub cohesion: f32,
}

fn one_one() -> [u32; 2] {
    [1, 1]
}
fn spacing() -> f32 {
    1.5
}
fn range() -> f32 {
    8.0
}
fn unit_w() -> f32 {
    1.0
}

impl Default for Group {
    fn default() -> Self {
        Group {
            size: one_one(),
            spacing: spacing(),
            range: range(),
            separation: 1.0,
            alignment: 1.0,
            cohesion: 1.0,
        }
    }
}

/// The boids acceleration for one member at `p` moving at `v`, from its
/// neighbours' positions and velocities (itself excluded).
/// `others` may include the member itself; it is skipped by distance.
pub fn steer(p: Vec3, v: Vec3, others: impl IntoIterator<Item = (Vec3, Vec3)>, g: &Group) -> Vec3 {
    let (mut sep, mut sum_p, mut sum_v, mut n) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0);
    for (q, w) in others {
        let d = p - q;
        let dist = d.length();
        if dist > g.range || dist < 1e-4 {
            continue;
        }
        if dist < g.spacing * 1.5 {
            sep += d / (dist * dist) * g.spacing;
        }
        sum_p += q;
        sum_v += w;
        n += 1.0;
    }
    if n == 0.0 {
        return Vec3::ZERO;
    }
    let coh = (sum_p / n - p) * 0.6;
    let ali = (sum_v / n - v) * 1.2;
    sep * 6.0 * g.separation + ali * g.alignment + coh * g.cohesion
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Fly,
    Land,
    Perch,
    Soar,
}

/// One flock, kept as its own entity (no Look, so nothing is drawn for it).
pub struct Flock {
    pub species: usize,
    pub mode: Mode,
    /// Where the flock is heading; it wanders round `home`.
    pub goal: Vec3,
    pub home: Vec3,
    pub timer: f32,
    phase: f32,
    rng: Rng,
}

/// A member of a flock.
pub struct Flyer {
    pub flock: hecs::Entity,
    pub vel: Vec3,
    pub perched: bool,
}

impl Flock {
    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }
}

/// Spawn a flock of `n` (or a soaring bird, n = 1) of species `s` above `at`.
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, at: Vec3, n: u32, seed: u32) -> u32 {
    let soar = matches!(sp.verb().mover, super::locomotion::Mover::Flock { soar: true });
    let mut f = Flock {
        species: s,
        mode: if soar { Mode::Soar } else { Mode::Fly },
        goal: at,
        home: at,
        timer: 0.0,
        phase: 0.0,
        rng: Rng::new(seed | 1),
    };
    f.timer = 20.0 + f.rand() * 30.0;
    f.phase = f.rand() * std::f32::consts::TAU;
    let height = sp.beh.fly_height.max(3.0);
    let mut members = Vec::new();
    for _ in 0..n.max(1) {
        let off = Vec3::new(f.rand() - 0.5, f.rand() * 0.5, f.rand() - 0.5) * (2.0 + n as f32 * 0.25);
        let a = f.rand() * std::f32::consts::TAU;
        let pos = at + Vec3::Y * height + off;
        let vel = Vec3::new(a.sin(), 0.0, a.cos()) * sp.beh.walk_speed.max(2.0);
        members.push((pos, vel));
    }
    let fe = world.spawn((f,));
    for (pos, vel) in members {
        let yaw = vel.x.atan2(vel.z) + std::f32::consts::PI;
        world.spawn((Flyer { flock: fe, vel, perched: false }, Creature::of(s), Look::new(sp.model, pos, yaw)));
    }
    n.max(1)
}

/// Step every flock. Birds keep above `terrain.surface` (trees included, so
/// they can perch in them) and never come down where `terrain.wet`.
pub fn update(world: &mut hecs::World, species: &[Species], groups: &[Group], dt: f32, threat: &super::startle::Threat, terrain: &Terrain) {
    let player = threat.pos;
    let ground = |x: f32, z: f32| terrain.surface(x, z);
    let wet = |x: f32, z: f32| terrain.wet(x, z);
    // Members by flock.
    let mut by_flock: crate::util::FxHashMap<hecs::Entity, Vec<(Vec3, Vec3, bool)>> = Default::default();
    for (fl, look) in world.query::<(&Flyer, &Look)>().iter() {
        by_flock.entry(fl.flock).or_default().push((look.pos, fl.vel, fl.perched));
    }
    // Flocks with no members left are gone.
    let empty: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Flock)>().iter().map(|(e, _)| e).filter(|e| !by_flock.contains_key(e)).collect();
    for e in empty {
        let _ = world.despawn(e);
    }
    // Flock decisions: wander, land, flush.
    let mut flushed: Vec<hecs::Entity> = Vec::new();
    for (fe, f) in world.query_mut::<(hecs::Entity, &mut Flock)>() {
        let Some(m) = by_flock.get(&fe) else { continue };
        let sp = &species[f.species];
        let n = m.len() as f32;
        let centre = m.iter().fold(Vec3::ZERO, |a, x| a + x.0) / n;
        // The whole flock bursts together, so its flightiness moves the
        // radius rather than building alarm in each bird.
        let nerve = super::startle::Nerve::of(sp, None, super::age::Stage::Adult);
        let near_player = m.iter().any(|x| (x.0 - player).length() < super::startle::reach(&nerve, threat, x.0, 6.0));
        f.timer -= dt;
        f.phase += dt * if f.mode == Mode::Soar { 0.12 } else { 0.18 };
        match f.mode {
            Mode::Soar => {
                // Wide, slow circles round a home that drifts.
                let r = 45.0;
                f.goal = f.home + Vec3::new(f.phase.cos() * r, 0.0, f.phase.sin() * r);
                if f.timer <= 0.0 {
                    let (dx, dz) = (f.rand() - 0.5, f.rand() - 0.5);
                    f.home += Vec3::new(dx, 0.0, dz) * 60.0;
                    f.timer = 40.0 + f.rand() * 40.0;
                }
            }
            Mode::Fly => {
                let r = 12.0 + n.sqrt() * 2.0;
                f.goal = f.home + Vec3::new(f.phase.cos() * r, 0.0, (f.phase * 1.3).sin() * r * 0.7);
                if f.timer <= 0.0 {
                    if f.rand() < 0.5 && !near_player {
                        // Come down somewhere near.
                        let a = f.rand() * std::f32::consts::TAU;
                        let p = centre + Vec3::new(a.cos(), 0.0, a.sin()) * (10.0 + f.rand() * 20.0);
                        if let (Some(gy), false) = (ground(p.x, p.z), wet(p.x, p.z)) {
                            f.goal = Vec3::new(p.x, gy, p.z);
                            f.mode = Mode::Land;
                            f.timer = 20.0;
                        }
                    } else {
                        let (dx, dz) = (f.rand() - 0.5, f.rand() - 0.5);
                        f.home = centre + Vec3::new(dx, 0.0, dz) * 50.0;
                        f.timer = 15.0 + f.rand() * 25.0;
                    }
                }
            }
            Mode::Land => {
                if m.iter().all(|x| x.2) {
                    f.mode = Mode::Perch;
                    f.timer = 12.0 + f.rand() * 30.0;
                } else if f.timer <= 0.0 || near_player {
                    flushed.push(fe);
                }
            }
            Mode::Perch => {
                if f.timer <= 0.0 || near_player {
                    flushed.push(fe);
                }
            }
        }
        if flushed.last() == Some(&fe) {
            // Off and away from the player, then wheel round somewhere new.
            let away = Vec3::new(centre.x - player.x, 0.0, centre.z - player.z).normalize_or_zero();
            f.home = centre + away * 40.0;
            f.goal = f.home;
            f.mode = Mode::Fly;
            f.timer = 20.0 + f.rand() * 25.0;
        }
    }
    let flocks: std::collections::HashMap<hecs::Entity, (Mode, Vec3, usize)> = world.query::<(hecs::Entity, &Flock)>().iter().map(|(e, f)| (e, (f.mode, f.goal, f.species))).collect();
    // Members.
    for (fl, look, morph) in world.query_mut::<(&mut Flyer, &mut Look, Option<&crate::variety::Morph>)>() {
        look.begin_step();
        let (pace, _) = crate::variety::pace(morph);
        let Some(&(mode, goal, s)) = flocks.get(&fl.flock) else {
            continue;
        };
        let sp = &species[s];
        let g = &groups[s];
        let cruise = sp.beh.walk_speed.max(2.0) * pace;
        let top = sp.beh.flee_speed.max(cruise * 1.2) * pace;
        let p = look.pos;
        if flushed.contains(&fl.flock) && fl.perched {
            fl.perched = false;
            let away = Vec3::new(p.x - player.x, 0.0, p.z - player.z).normalize_or_zero();
            let h = crate::world::noise::hash3(p.x as i32 * 13, p.z as i32 * 7, (p.y * 10.0) as i32, 5);
            let jitter = Vec3::new(crate::world::noise::unit(h) - 0.5, 0.0, crate::world::noise::unit(h >> 8) - 0.5);
            fl.vel = (away + jitter + Vec3::Y * 1.2).normalize_or_zero() * top;
        }
        if fl.perched {
            continue;
        }
        let members = by_flock.get(&fl.flock).map_or(&[][..], |v| v.as_slice());
        let others = members.len().saturating_sub(1);
        let floor = ground(p.x, p.z);
        let mut acc = steer(p, fl.vel, members.iter().map(|x| (x.0, x.1)), g);
        let to_goal = goal - p;
        match mode {
            Mode::Land => {
                // Glide down onto the spot, slowing as it nears.
                let want = to_goal.normalize_or_zero() * (cruise * (to_goal.length() / 12.0).clamp(0.3, 1.0));
                acc += (want - fl.vel) * 1.5;
                if let Some(gy) = floor {
                    if p.y - gy < 0.35 && Vec3::new(to_goal.x, 0.0, to_goal.z).length() < 6.0 + others as f32 * 0.15 && !wet(p.x, p.z) {
                        fl.perched = true;
                        fl.vel = Vec3::ZERO;
                        look.pos.y = gy;
                        continue;
                    }
                }
            }
            _ => {
                let flat = Vec3::new(to_goal.x, 0.0, to_goal.z);
                acc += (flat.normalize_or_zero() * cruise - Vec3::new(fl.vel.x, 0.0, fl.vel.z)) * 0.5;
                if let Some(gy) = floor {
                    let want_y = gy + sp.beh.fly_height.max(3.0);
                    acc.y += (want_y - p.y) * 0.6 - fl.vel.y * 0.8;
                }
            }
        }
        if let Some(gy) = floor {
            if p.y < gy + 1.5 && mode != Mode::Land {
                acc.y += 25.0;
            }
        }
        fl.vel += acc * dt;
        let sp_len = fl.vel.length();
        let lo = if mode == Mode::Land { 0.5 } else { cruise * 0.6 };
        if sp_len > top {
            fl.vel *= top / sp_len;
        } else if sp_len < lo && sp_len > 1e-4 {
            fl.vel *= lo / sp_len;
        }
        look.pos += fl.vel * dt;
        if let Some(gy) = floor {
            look.pos.y = look.pos.y.max(gy + 0.05);
        }
        let flat = Vec3::new(fl.vel.x, 0.0, fl.vel.z);
        if flat.length() > 0.3 {
            let want = flat.x.atan2(flat.z) + std::f32::consts::PI;
            look.yaw = crate::util::turn_toward(look.yaw, want, (dt * 6.0).min(1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_too_close_push_apart_and_far_ones_pull_in() {
        let g = Group {
            spacing: 1.0,
            range: 10.0,
            ..Default::default()
        };
        let close = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(0.5, 0.0, 0.0), Vec3::ZERO)], &g);
        assert!(close.x < 0.0, "{close}");
        let far = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(6.0, 0.0, 0.0), Vec3::ZERO)], &g);
        assert!(far.x > 0.0, "{far}");
        let out_of_range = steer(Vec3::ZERO, Vec3::ZERO, [(Vec3::new(20.0, 0.0, 0.0), Vec3::ZERO)], &g);
        assert_eq!(out_of_range, Vec3::ZERO);
    }

    #[test]
    fn members_turn_to_fly_with_their_neighbours() {
        let g = Group::default();
        let v = Vec3::new(5.0, 0.0, 0.0);
        let others = [(Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 5.0)), (Vec3::new(-2.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 5.0))];
        let a = steer(Vec3::ZERO, v, others, &g);
        assert!(a.z > 0.0 && a.x < 0.0, "{a}");
    }
}
