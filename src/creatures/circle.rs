//! Circlers (the `Circle` mover: circle, flutter): gulls round the coast,
//! moths round a lamp. Each goes round a fixed centre; seabirds that `float`
//! now and then glide down to sit on the sea, drift, and fly back up.

use super::terrain::Terrain;
use super::{Creature, Look, Pathing, Species};
use glam::Vec3;

pub struct Gull {
    pub centre: Vec3,
    pub radius: f32,
    pub angle: f32,
    pub speed: f32,
    /// Seabirds that float: what it is doing off the circle.
    rest: GullRest,
    /// Seconds until it next thinks of coming down.
    next: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum GullRest {
    Circling,
    /// Gliding from `from` down to the sea at `to`.
    Down {
        from: Vec3,
        to: Vec3,
        t: f32,
        dur: f32,
    },
    /// Sitting on the water for `left` seconds, drifting.
    Float {
        left: f32,
    },
    /// Back up to the circle.
    Up {
        from: Vec3,
        t: f32,
    },
    /// Under the water after fish, `depth` metres at the bottom of the dive.
    Under {
        t: f32,
        dur: f32,
        depth: f32,
    },
}

impl Gull {
    pub fn new(centre: Vec3, radius: f32, angle: f32, speed: f32) -> Self {
        Gull {
            centre,
            radius,
            angle,
            speed,
            rest: GullRest::Circling,
            next: 20.0 + (angle * 37.0).rem_euclid(40.0),
        }
    }

    /// On the water or under it: wings folded either way.
    pub fn floating(&self) -> bool {
        matches!(self.rest, GullRest::Float { .. } | GullRest::Under { .. })
    }

    pub fn landing(&self) -> bool {
        matches!(self.rest, GullRest::Down { t, .. } if t > 0.7)
    }
}

/// Over this height a circler holds its own level; under it the circle rides
/// over the ground (see `update`).
const LOW_FLY: f32 = 8.0;

/// Step every circler. Legacy gulls have no `Creature` and never float.
pub fn update(world: &mut hecs::World, species: &[Species], terrain: &Terrain, dt: f32, threat: &super::startle::Threat, stats: &mut Pathing) {
    let player = threat.pos;
    let sea = terrain.sea;
    for (gull, look, me, morph) in world.query_mut::<(&mut Gull, &mut Look, Option<&Creature>, Option<&crate::variety::Morph>)>() {
        look.begin_step();
        gull.angle += gull.speed * dt / gull.radius;
        let (s, c) = gull.angle.sin_cos();
        let on_circle = gull.centre + Vec3::new(c * gull.radius, (gull.angle * 0.7).sin() * 1.5, s * gull.radius);
        // Tangent direction, banking ignored.
        let t = Vec3::new(-s, 0.0, c) * gull.speed.signum();
        let circle_yaw = t.x.atan2(t.z) + std::f32::consts::PI;
        let sp = me.map(|c| &species[c.species]);
        let floats = sp.is_some_and(|s| s.beh.floats);
        let wary = match sp {
            Some(s) => super::startle::reach(&super::startle::Nerve::of(s, morph, super::age::Stage::Adult), threat, look.pos, 6.0),
            None => 8.0 * crate::variety::pace(morph).1,
        };
        let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < wary;
        // Each gull's own clock for drift and bobbing.
        let h = gull.centre.x * 0.37 + gull.radius;
        match gull.rest {
            GullRest::Circling => {
                look.pos = on_circle;
                // A low flyer (a skate over the grass, a moth, an owl
                // quartering a meadow) keeps its height over the ground under
                // it, not over the point it started from: otherwise a rise
                // walks up into the circle and the creature disappears into
                // the hill. Over water it keeps clear of the surface instead.
                let low = sp.map_or(0.0, |s| s.beh.fly_height);
                if (0.0..=LOW_FLY).contains(&low) && low > 0.0 {
                    let bob = (on_circle.y - gull.centre.y) * 0.4;
                    match terrain.height(look.pos.x, look.pos.z) {
                        Some(g) if g > sea => look.pos.y = g + low + bob,
                        _ => look.pos.y = look.pos.y.max(sea + low),
                    }
                }
                look.yaw = circle_yaw;
                gull.next -= dt;
                if floats && gull.next <= 0.0 {
                    // Come down a little way ahead, if that is open water.
                    let ahead = gull.angle + gull.speed.signum() * 0.6;
                    let to = gull.centre + Vec3::new(ahead.cos() * gull.radius, 0.0, ahead.sin() * gull.radius);
                    if terrain.deep(to.x, to.z) && !near {
                        let to = Vec3::new(to.x, sea - 0.26, to.z);
                        let dur = ((to - on_circle).length() / 5.0).clamp(2.0, 6.0);
                        gull.rest = GullRest::Down { from: on_circle, to, t: 0.0, dur };
                    } else {
                        gull.next = 5.0 + (h * 13.0).rem_euclid(10.0);
                    }
                }
            }
            GullRest::Down { from, to, t, dur } => {
                let t = (t + dt / dur).min(1.0);
                let e = t * t * (3.0 - 2.0 * t);
                look.pos = from.lerp(to, e) + Vec3::Y * (1.0 - t) * t * 2.0;
                let d = to - from;
                look.yaw = d.x.atan2(d.z) + std::f32::consts::PI;
                gull.rest = if t >= 1.0 {
                    GullRest::Float {
                        left: 15.0 + (h * 7.0).rem_euclid(30.0),
                    }
                } else {
                    GullRest::Down { from, to, t, dur }
                };
            }
            GullRest::Float { left } => {
                let left = left - dt;
                // Drift slowly the way it faces, turning now and then, bobbing.
                look.yaw += (gull.angle * 0.9 + h).sin() * 0.25 * dt;
                let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
                let next = look.pos + fwd * 0.12 * dt;
                if terrain.deep(next.x, next.z) {
                    look.pos = next;
                }
                look.pos.y = sea - 0.26 + 0.03 * (gull.angle * 4.0 + h).sin();
                gull.rest = if left <= 0.0 || near { GullRest::Up { from: look.pos, t: 0.0 } } else { GullRest::Float { left } };
                // A diver goes under every few seconds while it sits, where
                // there is the water for it.
                if let (Some(max), GullRest::Float { .. }) = (sp.and_then(|s| s.beh.dive_depth), gull.rest) {
                    gull.next -= dt;
                    let room = sea - terrain.bed(look.pos.x, look.pos.z) - 0.6;
                    if gull.next <= 0.0 {
                        gull.next = 6.0 + (h * 17.0 + left).rem_euclid(8.0);
                        if room > 1.0 {
                            let dur = 12.0 + (h * 5.0 + left).rem_euclid(10.0);
                            gull.rest = GullRest::Under { t: 0.0, dur, depth: max.min(room) };
                        }
                    }
                }
            }
            GullRest::Under { t, dur, depth } => {
                // Down in a curve, weaving after fish, and up again further on.
                let t = (t + dt / dur).min(1.0);
                look.yaw += (gull.angle * 1.7 + h + t * 9.0).sin() * 0.7 * dt;
                let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
                let next = look.pos + fwd * 1.5 * dt;
                if terrain.deep(next.x, next.z) {
                    look.pos.x = next.x;
                    look.pos.z = next.z;
                } else {
                    look.yaw += std::f32::consts::PI * dt;
                }
                let dive = (t * std::f32::consts::PI).sin().sqrt();
                look.pos.y = (sea - 0.26 - depth * dive).max(terrain.bed(look.pos.x, look.pos.z) + 0.3);
                gull.rest = if t >= 1.0 { GullRest::Float { left: 6.0 + (h * 3.0).rem_euclid(14.0) } } else { GullRest::Under { t, dur, depth } };
            }
            GullRest::Up { from, t } => {
                let t = (t + dt / 3.0).min(1.0);
                let e = t * t;
                look.pos = from.lerp(on_circle, e);
                let d = on_circle - look.pos;
                look.yaw = if t > 0.8 { circle_yaw } else { d.x.atan2(d.z) + std::f32::consts::PI };
                if t >= 1.0 {
                    gull.rest = GullRest::Circling;
                    gull.next = 30.0 + (h * 11.0).rem_euclid(60.0);
                } else {
                    gull.rest = GullRest::Up { from, t };
                }
            }
        }
        if look.prev_pos == Vec3::ZERO {
            look.begin_step();
        }
        stats.gull_steps += 1;
        stats.gull_floating += gull.floating() as u32;
    }
}
