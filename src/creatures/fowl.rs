//! Waterfowl (the `paddle` movement): geese, ducks and swans that float on
//! open water near the shore in loose groups and paddle slowly about. When
//! the player comes close they take off together and fly low to other water
//! 40 to 90 m away, coming down with a splash. Now and then one flies off on
//! its own to another patch of water. They also come ashore: a bird
//! paddles to a low bank or beach close by, walks out and grazes along the
//! water's edge for half a minute or so, then walks back in (or flies, if
//! startled).
//!
//! Water is the sea (in Redrock the reservoir) or a river. Each bird
//! carries the level of the water it is on; on a river the current carries
//! it slowly downstream while it paddles about, and the level follows the
//! river as it goes.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::models::Model;
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub enum State {
    Float,
    /// On land by the water, grazing; `back` once it is walking back in.
    Ashore {
        timer: f32,
        back: bool,
    },
    /// A flight along the leader's line `from` -> `to`, at `off` from it
    /// (its place in the V; zero for the leader), joining up from its own
    /// starting point `own` over the first quarter of the way.
    Fly {
        from: Vec3,
        to: Vec3,
        t: f32,
        dur: f32,
        own: Vec3,
        off: Vec3,
    },
}

/// A flock that took off this step: who follows whom.
struct Party {
    species: usize,
    at: Vec3,
    from: Vec3,
    to: Vec3,
    level: f32,
    dur: f32,
    n: u32,
}

/// Take off for water `rmin..rmax` away (away from `away` when startled).
/// A startled bird within 12 m of a flock-mate that already took off this
/// step follows it, taking the next place in a V behind the leader. False
/// if there is no water to go to.
#[allow(clippy::too_many_arguments)]
fn take_off(parties: &mut Vec<Party>, s: usize, p: &mut Paddler, pos: Vec3, away: Option<Vec3>, (rmin, rmax): (f32, f32), t: &Terrain, speed: f32, sink_m: f32, spacing: f32) -> bool {
    if away.is_some() {
        if let Some(party) = parties.iter_mut().find(|q| q.species == s && (q.at - pos).with_y(0.0).length() < 12.0) {
            party.n += 1;
            let rank = party.n - 1;
            let fwd = (party.to - party.from).with_y(0.0).normalize_or_zero();
            let right = Vec3::new(-fwd.z, 0.0, fwd.x);
            let row = rank.div_ceil(2) as f32;
            let side = if rank % 2 == 1 { 1.0 } else { -1.0 };
            let mut off = (-fwd * row + right * side * row * 0.8) * spacing + Vec3::Y * 0.12 * row;
            // Where the V would come down on land, close it up.
            let end = party.to + off;
            if !on_water(t, end.x, end.z, party.level, 0.4) {
                off *= 0.25;
            }
            p.level = party.level;
            p.state = State::Fly {
                from: party.from,
                to: party.to,
                t: 0.0,
                dur: party.dur,
                own: pos,
                off,
            };
            p.home = Vec3::new(party.to.x + off.x, p.level, party.to.z + off.z);
            return true;
        }
    }
    let Some(to) = water_near(t, pos, rmin, rmax, away, &mut p.rng) else {
        return false;
    };
    p.level = to.y;
    let to = Vec3::new(to.x, to.y - sink_m, to.z);
    let dur = ((to - pos).with_y(0.0).length() / speed).max(3.0);
    p.state = State::Fly {
        from: pos,
        to,
        t: 0.0,
        dur,
        own: pos,
        off: Vec3::ZERO,
    };
    p.home = Vec3::new(to.x, p.level, to.z);
    if away.is_some() {
        parties.push(Party {
            species: s,
            at: pos,
            from: pos,
            to,
            level: p.level,
            dur,
            n: 1,
        });
    }
    true
}

pub struct Paddler {
    pub state: State,
    /// Height of the water surface it floats on (sea level or a river's).
    level: f32,
    home: Vec3,
    heading: f32,
    timer: f32,
    bob: f32,
    rng: Rng,
}

impl Paddler {
    /// Floating or standing: wings folded.
    pub fn floating(&self) -> bool {
        matches!(self.state, State::Float | State::Ashore { .. })
    }
    /// The last part of a flight: flare and drop the feet.
    pub fn landing(&self) -> bool {
        matches!(self.state, State::Fly { t, .. } if t > 0.8)
    }
}

/// Water depth at (x, z); nothing past the world's edge.
fn depth(t: &Terrain, x: f32, z: f32) -> f32 {
    if x < 10.0 || z < 10.0 || x > t.size() - 10.0 || z > t.size() - 10.0 {
        return 0.0;
    }
    t.sea - t.bed(x, z)
}

/// The surface of water a bird can float on at (x, z): the sea at least
/// `min_depth` deep, or a river at least half a metre deep with water on
/// every side.
fn level(t: &Terrain, x: f32, z: f32, min_depth: f32) -> Option<f32> {
    if depth(t, x, z) >= min_depth {
        return Some(t.sea);
    }
    let (l, d) = t.inland_water(x, z)?;
    let around = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
        .iter()
        .all(|(dx, dz)| t.inland_water(x + dx, z + dz).is_some_and(|w| w.1 > 0.15));
    (d >= min_depth.max(0.5) && around).then_some(l)
}

/// Floatable water, `rmin..rmax` from `at`, leaning toward `toward` if
/// given: the sea 0.6 to 8 m deep within 40 m of land, or a river. Its
/// surface height is the y of the result.
pub fn water_near(t: &Terrain, at: Vec3, rmin: f32, rmax: f32, toward: Option<Vec3>, rng: &mut Rng) -> Option<Vec3> {
    for _ in 0..24 {
        let a = match toward {
            Some(d) => d.x.atan2(d.z) + (rng.unit() - 0.5) * 1.8,
            None => rng.unit() * TAU,
        };
        let r = rmin + rng.unit() * (rmax - rmin);
        let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
        let d = depth(t, x, z);
        if (0.6..8.0).contains(&d) && t.coast(x, z).is_none_or(|c| c > -40.0) {
            return Some(Vec3::new(x, t.sea, z));
        }
        if d <= 0.0 {
            if let Some(y) = level(t, x, z, 0.6) {
                return Some(Vec3::new(x, y, z));
            }
        }
    }
    None
}

/// Low open ground a waterbird can stand on at (x, z), beside water whose
/// surface is at `level`: its height.
fn bank(t: &Terrain, x: f32, z: f32, level: f32) -> Option<f32> {
    let (gy, m) = t.ground(x, z, level + 4.0)?;
    (gy > level + 0.15 && gy < level + 3.0 && !crate::mesh::sprites::leafy(m) && !super::terrain::water_voxel(m)).then_some(gy)
}

/// On the water it is floating on (within a step of `level`).
fn on_water(t: &Terrain, x: f32, z: f32, level: f32, min_depth: f32) -> bool {
    self::level(t, x, z, min_depth).is_some_and(|l| (l - level).abs() < 0.3)
}

/// How far below the surface the model's pivot sits so the bird floats
/// with its feet under water and its body half in (bird rig: feet at the
/// pivot when perched, belly about 2.5 voxels up).
fn sink(m: &Model, scale: f32) -> f32 {
    m.voxel * scale * 2.6
}

/// `n` birds of species `s` on the water at `at`.
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, models: &[Model], t: &Terrain, at: Vec3, n: u32, seed: u32) {
    let mut rng = Rng::new(seed | 1);
    let heading = rng.unit() * TAU;
    for k in 0..n.max(1) {
        let off = Vec3::new(rng.unit() - 0.5, 0.0, rng.unit() - 0.5) * (1.5 + n as f32 * 0.6);
        // A narrow river may not hold the whole group spread out.
        let off = if on_water(t, at.x + off.x, at.z + off.z, at.y, 0.6) { off } else { off * 0.1 };
        let pos = at + off - Vec3::Y * sink(&models[sp.model], 1.0);
        let p = Paddler {
            state: State::Float,
            level: at.y,
            home: at,
            heading: heading + (rng.unit() - 0.5) * 0.8,
            timer: 20.0 + rng.unit() * 60.0,
            bob: rng.unit() * TAU,
            rng: Rng::new(crate::world::noise::hash32(seed ^ k.wrapping_mul(0x9e37_79b9)) | 1),
        };
        world.spawn((p, Creature::of(s), Look::new(sp.model, pos, p_yaw(heading))));
    }
}

fn p_yaw(heading: f32) -> f32 {
    heading + PI
}

/// Step every paddling bird.
pub fn update(world: &mut hecs::World, species: &[Species], models: &[Model], dt: f32, threat: &super::startle::Threat, t: &Terrain) {
    let player = threat.pos;
    // Where each species' birds are, for flushing together.
    let flushed: Vec<Vec3> = world
        .query::<(&Paddler, &Look, &Creature)>()
        .iter()
        .filter(|(p, l, c)| {
            let nerve = super::startle::Nerve::of(&species[c.species], None, super::age::Stage::Adult);
            !matches!(p.state, State::Fly { .. }) && Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length() < super::startle::reach(&nerve, threat, l.pos, 4.0)
        })
        .map(|(_, l, _)| l.pos)
        .collect();
    let mut parties: Vec<Party> = Vec::new();
    for (p, look, me, morph) in world.query_mut::<(&mut Paddler, &mut Look, &Creature, Option<&crate::variety::Morph>)>() {
        look.begin_step();
        let sp = &species[me.species];
        let (pace, _) = crate::variety::pace(morph);
        let scale = morph.map_or(1.0, |m| m.scale);
        let sink_m = sink(&models[look.model], scale);
        let float_y = p.level - sink_m;
        let speed = sp.beh.flee_speed.max(6.0) * pace;
        let spacing = models[look.model].half_w.max(0.15) * scale * 5.0;
        match p.state {
            State::Float => {
                p.timer -= dt;
                p.bob += dt * 1.7;
                // Startled, or a flock-mate within 6 m was: off together.
                let near = flushed.iter().any(|f| (*f - look.pos).with_y(0.0).length() < 6.0);
                if near || p.timer <= 0.0 {
                    let away = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero();
                    let (range, dir) = if near { ((40.0, 90.0), Some(away)) } else { ((25.0, 60.0), None) };
                    if take_off(&mut parties, me.species, p, look.pos, dir, range, t, speed, sink_m, spacing) {
                        continue;
                    }
                    p.timer = 15.0 + p.rng.unit() * 40.0;
                }
                // Now and then head for a bank close by and walk out.
                if p.rng.unit() < dt / 40.0 {
                    for k in 0..12 {
                        let a = k as f32 * TAU / 12.0;
                        let q = look.pos + Vec3::new(a.sin(), 0.0, a.cos()) * 5.0;
                        if bank(t, q.x, q.z, p.level).is_some() {
                            p.heading = a;
                            p.state = State::Ashore {
                                timer: 25.0 + p.rng.unit() * 40.0,
                                back: false,
                            };
                            break;
                        }
                    }
                    if matches!(p.state, State::Ashore { .. }) {
                        continue;
                    }
                }
                // Paddle: a slow wander, back toward home when far, away
                // from shallows and the shore.
                p.heading += (p.rng.unit() - 0.5) * 1.2 * dt;
                let to_home = p.home - look.pos;
                if to_home.with_y(0.0).length() > 18.0 {
                    let want = to_home.x.atan2(to_home.z);
                    p.heading += crate::util::wrap_angle(want - p.heading).clamp(-0.6, 0.6) * dt;
                }
                let fwd = Vec3::new(p.heading.sin(), 0.0, p.heading.cos());
                let ahead = look.pos + fwd * 1.5;
                if !on_water(t, ahead.x, ahead.z, p.level, 0.5) {
                    p.heading += PI * 0.6 * dt * 4.0;
                } else {
                    look.pos += fwd * sp.beh.walk_speed.max(0.1) * 0.35 * pace * dt;
                }
                // On a river the current carries it; it paddles back up
                // toward home when it has drifted far.
                let cur = t.current(look.pos.x, look.pos.z);
                if cur != glam::Vec2::ZERO {
                    let next = look.pos + Vec3::new(cur.x, 0.0, cur.y) * 0.5 * dt;
                    if let Some(l) = level(t, next.x, next.z, 0.5) {
                        look.pos.x = next.x;
                        look.pos.z = next.z;
                        p.level = l;
                    }
                }
                let float_y = p.level - sink_m;
                look.pos.y = float_y + 0.02 * p.bob.sin();
                look.yaw = crate::util::turn_toward(look.yaw, p_yaw(p.heading), (dt * 3.0).min(1.0));
            }
            State::Ashore { timer, back } => {
                let timer = timer - dt;
                let near = flushed.iter().any(|f| (*f - look.pos).with_y(0.0).length() < 6.0);
                if near {
                    // Startled on land: straight up and off to water.
                    let away = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero();
                    if take_off(&mut parties, me.species, p, look.pos, Some(away), (30.0, 80.0), t, speed, sink_m, spacing) {
                        continue;
                    }
                }
                let back = back || timer <= 0.0;
                // Into the water or a step at a time along the bank.
                let here_wet = on_water(t, look.pos.x, look.pos.z, p.level, 0.35);
                if back && here_wet {
                    p.state = State::Float;
                    p.timer = 20.0 + p.rng.unit() * 40.0;
                    look.pos.y = float_y;
                    continue;
                }
                if back {
                    // Toward the water: the lowest ground round about.
                    let mut best = (t.shore_dist(look.pos.x, look.pos.z).unwrap_or(0.0), p.heading);
                    for k in 0..8 {
                        let a = k as f32 * TAU / 8.0;
                        let q = look.pos + Vec3::new(a.sin(), 0.0, a.cos()) * 2.0;
                        if let Some(c) = t.shore_dist(q.x, q.z) {
                            if c < best.0 {
                                best = (c, a);
                            }
                        }
                    }
                    p.heading = best.1;
                } else {
                    p.heading += (p.rng.unit() - 0.5) * 2.0 * dt;
                }
                let fwd = Vec3::new(p.heading.sin(), 0.0, p.heading.cos());
                // Grazing birds walk in short spells.
                let walking = back || (p.bob * 0.3).sin() > 0.2;
                p.bob += dt * 1.7;
                let next = look.pos + fwd * if walking { 0.35 * pace * dt } else { 0.0 };
                let on_bank = bank(t, look.pos.x, look.pos.z, p.level).is_some();
                match bank(t, next.x, next.z, p.level) {
                    // Once out, keep to the land until it means to go back.
                    None if !back && on_bank => p.heading += PI * 0.5,
                    Some(gy) => look.pos = Vec3::new(next.x, gy, next.z),
                    None if back || level(t, next.x, next.z, 0.0).is_some() => look.pos = Vec3::new(next.x, (p.level - 0.1).max(look.pos.y - 0.05), next.z),
                    None => p.heading += PI * 0.5,
                }
                // Stay within a few metres of the water.
                if !back && t.shore_dist(look.pos.x, look.pos.z).is_some_and(|c| c > 8.0) {
                    p.heading += PI;
                }
                look.yaw = crate::util::turn_toward(look.yaw, p_yaw(p.heading), (dt * 3.0).min(1.0));
                p.state = State::Ashore { timer, back };
            }
            State::Fly { from, to, t: tt, dur, own, off } => {
                let tt = (tt + dt / dur).min(1.0);
                let flat = from.lerp(to, tt) + off;
                // Low over the water, up over anything in the way.
                let arc = (PI * tt).sin();
                let under = t.surface(flat.x, flat.z).unwrap_or(t.sea);
                let y = flat.y + arc * (sp.beh.fly_height.max(3.0) + (under - flat.y).max(0.0));
                // Into its place in the V over the first quarter.
                let j = (tt / 0.25).min(1.0);
                let join = j * j * (3.0 - 2.0 * j);
                look.pos = Vec3::new(flat.x, y, flat.z) + (own - (from + off)) * (1.0 - join);
                let d = to - from;
                look.yaw = crate::util::turn_toward(look.yaw, d.x.atan2(d.z) + PI, (dt * 4.0).min(1.0));
                if tt >= 1.0 {
                    p.state = State::Float;
                    p.heading = d.x.atan2(d.z);
                    p.timer = 30.0 + p.rng.unit() * 60.0;
                } else {
                    p.state = State::Fly { from, to, t: tt, dur, own, off };
                }
            }
        }
    }
}
