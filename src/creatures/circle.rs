//! Circlers (the `Circle` mover: circle, flutter): gulls round the coast,
//! moths round a lamp. Each goes round a fixed centre; seabirds that `float`
//! now and then glide down to sit on the sea, drift, and fly back up, and
//! the ones that `plunge` (gannets) fold their wings and drop into it.
//!
//! A display (creatures/display: a crow's tumble, an owl on a post) takes a
//! circler off its circle for a while: it carries a `Show` and is put where
//! the display says, and when it ends it flies back up to its circle.

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
    /// In a display last step.
    shown: bool,
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
    /// Wings folded, falling from `from` into the sea after fish, `t`
    /// seconds into the fall.
    Plunge {
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
    /// On the water rather than in the air.
    pub fn sitting(&self) -> bool {
        matches!(self.rest, GullRest::Float { .. })
    }

    pub fn new(centre: Vec3, radius: f32, angle: f32, speed: f32) -> Self {
        Gull {
            centre,
            radius,
            angle,
            speed,
            rest: GullRest::Circling,
            next: 20.0 + (angle * 37.0).rem_euclid(40.0),
            shown: false,
        }
    }

    /// On the water, under it or plunging into it: wings folded each way.
    pub fn floating(&self) -> bool {
        matches!(self.rest, GullRest::Float { .. } | GullRest::Under { .. } | GullRest::Plunge { .. })
    }

    /// Going round its circle, free for a display (not still in one, or
    /// just out of one and not yet back up).
    pub fn circling(&self) -> bool {
        matches!(self.rest, GullRest::Circling) && !self.shown
    }

    pub fn landing(&self) -> bool {
        matches!(self.rest, GullRest::Down { t, .. } if t > 0.7)
    }
}

/// Over this height a circler holds its own level; under it the circle rides
/// over the ground (see `update`).
const LOW_FLY: f32 = 8.0;
/// How fast a low flyer climbs and comes down, metres a second.
const CLIMB: f32 = 8.0;
const SINK: f32 = 2.5;
/// How fast a plunging bird gathers speed, metres a second a second: more
/// than falling, since it drives down with its last wingbeats.
const PLUNGE_G: f32 = 14.0;
/// How fast it keeps going forward as it drops, metres a second.
const PLUNGE_ON: f32 = 3.0;
/// Least water under a plunge, metres: it goes in at speed.
const PLUNGE_ROOM: f32 = 2.5;

/// Height of a plunging bird `t` seconds after it folded at `from_y`.
fn plunge_y(from_y: f32, t: f32) -> f32 {
    from_y - 0.5 * PLUNGE_G * t * t
}

/// Step every circler. Legacy gulls have no `Creature` and never float.
pub fn update(
    world: &mut hecs::World,
    species: &[Species],
    terrain: &Terrain,
    dt: f32,
    threat: &super::startle::Threat,
    stats: &mut Pathing,
    cues: &mut Vec<crate::fx::Cue>,
) {
    let player = threat.pos;
    let sea = terrain.sea;
    for (gull, look, me, morph, show) in world.query_mut::<(&mut Gull, &mut Look, Option<&Creature>, Option<&crate::variety::Morph>, Option<&super::display::Show>)>() {
        look.begin_step();
        gull.angle += gull.speed * dt / gull.radius;
        // In a display: where it says. Afterwards, back up to the circle.
        // Only a display that puts it somewhere takes it over; one that
        // only sets its glow (fireflies flashing together) leaves it flying.
        let show = show.filter(|s| matches!(s.out.drive, super::display::Drive::Place { .. } | super::display::Drive::Sit { .. }));
        if let Some(s) = show {
            if let super::display::Drive::Place { pos, yaw } | super::display::Drive::Sit { pos, yaw } = s.out.drive {
                look.pos = pos;
                look.yaw = yaw;
            }
            gull.rest = GullRest::Circling;
            gull.shown = true;
            continue;
        }
        if std::mem::take(&mut gull.shown) {
            gull.rest = GullRest::Up { from: look.pos, t: 0.0 };
        }
        let (s, c) = gull.angle.sin_cos();
        let on_circle = gull.centre + Vec3::new(c * gull.radius, (gull.angle * 0.7).sin() * 1.5, s * gull.radius);
        // Tangent direction, banking ignored.
        let t = Vec3::new(-s, 0.0, c) * gull.speed.signum();
        let circle_yaw = t.x.atan2(t.z) + std::f32::consts::PI;
        let sp = me.map(|c| &species[c.species]);
        let floats = sp.is_some_and(|s| s.beh.floats);
        let plunges = floats && sp.is_some_and(|s| s.beh.plunge);
        let wary = match sp {
            Some(s) => super::startle::reach(&super::startle::Nerve::of(s, morph, super::age::Stage::Adult), threat, look.pos, 6.0),
            None => 8.0 * crate::variety::pace(morph).1,
        };
        let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < wary;
        // Each gull's own clock for drift and bobbing.
        let h = gull.centre.x * 0.37 + gull.radius;
        match gull.rest {
            GullRest::Circling => {
                let was = look.pos.y;
                look.pos = on_circle;
                // A low flyer (a skate over the grass, a moth, an owl
                // quartering a meadow) keeps its height over the ground under
                // it, not over the point it started from: otherwise a rise
                // walks up into the circle and the creature disappears into
                // the hill. Over water it keeps clear of the surface instead.
                // It reads the ground a few seconds ahead so it climbs before
                // a rise, and changes height at a bird's rate: set straight to
                // the ground's height, a cormorant leaving a cliff top dropped
                // the cliff in one step.
                let low = sp.map_or(0.0, |s| s.beh.fly_height);
                if (0.0..=LOW_FLY).contains(&low) && low > 0.0 {
                    let bob = (on_circle.y - gull.centre.y) * 0.4;
                    let step = t * gull.speed.abs();
                    look.pos.y = low_flight_y(was, look.pos, step, low, bob, on_circle.y, sea, |x, z| terrain.height(x, z), dt);
                }
                look.yaw = circle_yaw;
                gull.next -= dt;
                if plunges && gull.next <= 0.0 {
                    // Straight down from where it is, if there is the water
                    // for it; otherwise look again in a few seconds.
                    let room = sea - terrain.bed(look.pos.x, look.pos.z);
                    if terrain.deep(look.pos.x, look.pos.z) && room > PLUNGE_ROOM && !near {
                        gull.rest = GullRest::Plunge { from: look.pos, t: 0.0 };
                    } else {
                        gull.next = 3.0 + (h * 13.0).rem_euclid(5.0);
                    }
                } else if floats && gull.next <= 0.0 {
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
            GullRest::Plunge { from, t } => {
                let t = t + dt;
                let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
                let y = plunge_y(from.y, t);
                look.pos = Vec3::new(from.x, y, from.z) + fwd * PLUNGE_ON * t;
                let surface = sea - 0.26;
                gull.rest = if y <= surface {
                    look.pos.y = surface;
                    super::swim::splash_at(cues, Vec3::new(look.pos.x, sea, look.pos.z), 0.6);
                    let room = sea - terrain.bed(look.pos.x, look.pos.z) - 0.6;
                    let max = sp.and_then(|s| s.beh.dive_depth).unwrap_or(3.0);
                    GullRest::Under {
                        t: 0.0,
                        dur: 3.0 + (h * 5.0).rem_euclid(3.0),
                        depth: max.min(room).max(0.5),
                    }
                } else {
                    GullRest::Plunge { from, t }
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
                gull.rest = if t >= 1.0 {
                    GullRest::Float {
                        left: 6.0 + (h * 3.0).rem_euclid(14.0),
                    }
                } else {
                    GullRest::Under { t, dur, depth }
                };
            }
            GullRest::Up { from, t } => {
                let t = (t + dt / 3.0).min(1.0);
                let e = t * t;
                look.pos = from.lerp(on_circle, e);
                let d = on_circle - look.pos;
                look.yaw = if t > 0.8 { circle_yaw } else { d.x.atan2(d.z) + std::f32::consts::PI };
                if t >= 1.0 {
                    gull.rest = GullRest::Circling;
                    gull.next = if plunges { 12.0 + (h * 11.0).rem_euclid(20.0) } else { 30.0 + (h * 11.0).rem_euclid(60.0) };
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

/// The height a low flyer at `pos` flies at this step, from the height it
/// was at: `low` over the ground (or the sea), reading the ground up to five
/// seconds ahead along `step` (its velocity) so it climbs before a rise, and
/// moving at most `CLIMB` up or `SINK` down a second. Never under half a
/// metre over the ground beneath it.
#[allow(clippy::too_many_arguments)]
fn low_flight_y(was: f32, pos: Vec3, step: Vec3, low: f32, bob: f32, circle_y: f32, sea: f32, ground: impl Fn(f32, f32) -> Option<f32>, dt: f32) -> f32 {
    let over = |p: Vec3| match ground(p.x, p.z) {
        Some(g) if g > sea => g + low,
        _ => sea + low,
    };
    let ahead = (1..=5).map(|k| over(pos + step * k as f32)).fold(over(pos), f32::max);
    let want = if ground(pos.x, pos.z).is_some_and(|g| g > sea) { ahead + bob } else { ahead.max(circle_y) };
    let y = if want > was { (was + CLIMB * dt).min(want) } else { (was - SINK * dt).max(want) };
    y.max(over(pos) - low + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 30 m cliff top at x < 0 over the sea (level 0). Flying off it at
    /// 10 m/s, a low flyer glides down, and flying in towards it, it has
    /// climbed clear before it gets there.
    #[test]
    fn a_low_flyer_glides_off_a_cliff_and_climbs_before_one() {
        let ground = |x: f32, _: f32| Some(if x < 0.0 { 30.0 } else { -5.0 });
        let dt = 1.0 / 60.0;
        let (low, sea) = (5.0, 0.0);
        let mut pos = Vec3::new(-20.0, 35.0, 0.0);
        let step = Vec3::new(10.0, 0.0, 0.0);
        let mut worst_drop = 0.0f32;
        for _ in 0..1200 {
            let was = pos.y;
            pos += step * dt;
            pos.y = low_flight_y(was, pos, step, low, 0.0, 0.0, sea, ground, dt);
            worst_drop = worst_drop.max(was - pos.y);
        }
        assert!(worst_drop <= SINK * dt + 1e-4, "dropped {worst_drop} m in one step");
        assert!((pos.y - (sea + low)).abs() < 0.01, "should end {low} m over the sea, is at {}", pos.y);

        let mut pos = Vec3::new(40.0, sea + low, 0.0);
        let step = -step;
        // Stop short of the edge: over the top the floor rule would lift it anyway.
        while pos.x > 0.3 {
            let was = pos.y;
            pos += step * dt;
            pos.y = low_flight_y(was, pos, step, low, 0.0, 0.0, sea, ground, dt);
        }
        assert!(pos.y >= 30.5, "reached the cliff edge at {} m, under its top", pos.y);
    }

    /// A gannet folding 25 m up hits the water in about two seconds, going
    /// faster all the way down.
    #[test]
    fn a_plunge_gathers_speed_and_reaches_the_sea_quickly() {
        let (top, sea) = (25.0, 0.0);
        let dt = 1.0 / 60.0;
        let (mut t, mut last_drop) = (0.0f32, 0.0f32);
        while plunge_y(top, t) > sea {
            let drop = plunge_y(top, t) - plunge_y(top, t + dt);
            assert!(drop >= last_drop, "slowed down at {t:.2} s");
            last_drop = drop;
            t += dt;
        }
        assert!((1.5..2.5).contains(&t), "took {t:.2} s to fall {top} m");
    }
}
