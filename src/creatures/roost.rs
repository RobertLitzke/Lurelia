//! Roosts: holes that empty at dusk. A `roost` event marks a cave mouth or a
//! skylight with the species that sleep in it. In its window of the evening,
//! while the player is near enough to see, the roost lets them out one at a
//! time from below the ground: each comes up in a tight spiral out of the
//! hole and widens into the circle it will keep, so the whole colony pours
//! out of one place over half a minute instead of fading in round the map.
//! Only circlers (`circle`, `flutter`, `drift`, `undulate`) can come out of a roost; they are
//! ordinary creatures once they are up.

use super::circle::Gull;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;

/// A roost as the chronicle left it.
#[derive(Clone, Debug)]
pub struct RoostDef {
    /// The ground at the mouth, metres.
    pub at: Vec3,
    /// Metres under `at` they come up from.
    pub depth: f32,
    /// Species indices into the world's species list are resolved at load;
    /// these are the names.
    pub species: Vec<String>,
    pub count: u32,
    /// Game hours the colony goes out between.
    pub hours: [f32; 2],
}

/// How near the player must be for a roost to empty, metres.
const SEEN_M: f32 = 280.0;

/// What a roost has done this evening.
#[derive(Clone, Default, Debug)]
pub struct RoostRun {
    out: u32,
    next: f32,
}

/// One coming up out of the hole, from a tight circle low down to its own
/// wide one high up.
pub struct Rising {
    t: f32,
    dur: f32,
    from: Vec3,
    to: Vec3,
    r0: f32,
    r1: f32,
}

fn in_window(hour: f32, h: [f32; 2]) -> bool {
    if h[0] <= h[1] {
        hour >= h[0] && hour <= h[1]
    } else {
        hour >= h[0] || hour <= h[1]
    }
}

/// Let out what is due, and reset a roost once its evening is over.
pub fn release(world: &mut hecs::World, species: &[Species], roosts: &[RoostDef], runs: &mut Vec<RoostRun>, hour: f32, dt: f32, player: Vec3, rng: &mut Rng) {
    runs.resize(roosts.len(), RoostRun::default());
    for (r, run) in roosts.iter().zip(runs.iter_mut()) {
        if !in_window(hour, r.hours) {
            run.out = 0;
            run.next = 0.0;
            continue;
        }
        if run.out >= r.count || r.at.distance(player) > SEEN_M {
            continue;
        }
        run.next -= dt;
        if run.next > 0.0 {
            continue;
        }
        run.next = 0.15 + rng.unit() * 0.6;
        let names: Vec<usize> = r.species.iter().filter_map(|n| species.iter().position(|s| &s.name == n)).collect();
        if names.is_empty() {
            run.out = r.count;
            continue;
        }
        let s = names[(rng.unit() * names.len() as f32) as usize % names.len()];
        let sp = &species[s];
        let from = r.at - Vec3::Y * r.depth;
        let up = sp.beh.fly_height.max(2.0) + 6.0 + rng.unit() * 14.0;
        let ang = rng.unit() * std::f32::consts::TAU;
        let drift = 10.0 + rng.unit() * 40.0;
        let to = r.at + Vec3::new(ang.sin() * drift, up, -ang.cos() * drift);
        let speed = (2.2 + rng.unit() * 1.6) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        let gull = Gull::new(from, 0.8, rng.unit() * std::f32::consts::TAU, speed);
        world.spawn((
            gull,
            Creature::of(s),
            Look::new(sp.model, from, 0.0),
            Rising { t: 0.0, dur: 9.0 + rng.unit() * 8.0, from, to, r0: 0.8, r1: 6.0 + rng.unit() * 10.0 },
        ));
        run.out += 1;
    }
}

/// Carry each rising circler up its spiral; it is an ordinary circler once
/// it gets there.
pub fn rise(world: &mut hecs::World, dt: f32) {
    let mut done = Vec::new();
    for (e, gull, rising) in world.query_mut::<(hecs::Entity, &mut Gull, &mut Rising)>() {
        rising.t = (rising.t + dt / rising.dur).min(1.0);
        let k = rising.t * rising.t * (3.0 - 2.0 * rising.t);
        // Straight up out of the hole first, then out and away.
        let lift = (rising.t * 2.2).min(1.0);
        let lift = lift * lift * (3.0 - 2.0 * lift);
        let y = rising.from.y + (rising.to.y - rising.from.y) * lift;
        let flat = rising.from.lerp(rising.to, k);
        gull.centre = Vec3::new(flat.x, y, flat.z);
        gull.radius = rising.r0 + (rising.r1 - rising.r0) * k;
        if rising.t >= 1.0 {
            done.push(e);
        }
    }
    for e in done {
        let _ = world.remove_one::<Rising>(e);
    }
}
