//! Coming down for a perched display: where a flier settles, and getting
//! there, and the water's surface. Shared by the bird and fish displays.

use super::{heading, spot, Ctx, Drive, Hop, Member, Mover, Out};
use crate::world::noise::Rng;
use glam::Vec3;

/// How far a bird sits down in the water: its body's waterline, as circle.rs
/// floats them.
pub const FLOAT: f32 = 0.26;

/// Open water `rmin..rmax` from `around`, where a bird can sit on the sea.
pub fn water_spot(ctx: &Ctx, around: Vec3, rmin: f32, rmax: f32, rng: &mut Rng) -> Option<Vec3> {
    (0..16).find_map(|_| {
        let a = rng.unit() * std::f32::consts::TAU;
        let r = rmin + rng.unit() * (rmax - rmin);
        let (x, z) = (around.x + a.sin() * r, around.z + a.cos() * r);
        ctx.terrain.deep(x, z).then_some(Vec3::new(x, ctx.terrain.sea - FLOAT, z))
    })
}

/// Where member `m` settles: `on` is "water", "perch" (raised, a post, a
/// rock, a roof) or "ground". A flitting or perched bird that isn't asked
/// for water stays where it sits.
pub fn settle_at(m: &Member, on: &str, ctx: &Ctx, rng: &mut Rng) -> Option<Vec3> {
    if !matches!(m.mover, Mover::Circler { .. }) && on != "water" {
        return Some(m.pos);
    }
    match on {
        "water" => water_spot(ctx, m.pos, 5.0, 40.0, rng),
        "ground" => spot(ctx.terrain, m.pos, 6.0, 30.0, Some(m.facing()), false, rng),
        _ => spot(ctx.terrain, m.pos, 6.0, 30.0, Some(m.facing()), true, rng),
    }
}

/// A flight down to a spot and then sitting there.
pub struct Settle {
    pub at: Vec3,
    pub yaw: f32,
    hop: Option<Hop>,
}

impl Settle {
    pub fn new(m: &Member, at: Vec3, yaw: f32) -> Settle {
        let hop = (m.pos - at).length() > 0.3;
        Settle { at, yaw, hop: hop.then(|| Hop::new(m.pos, at, m.slow.max(4.0))) }
    }

    /// Flying there: the step's `Out`. None once it sits.
    pub fn flying(&mut self, m: &Member, dt: f32) -> Option<Out> {
        let h = self.hop.as_mut()?;
        let (pos, done) = h.step(dt);
        if done {
            self.hop = None;
        }
        Some(Out { drive: Drive::Place { pos, yaw: heading(m.pos, pos, m.yaw) }, ..Default::default() })
    }

    pub fn sit(&self) -> Drive {
        Drive::Sit { pos: self.at, yaw: self.yaw }
    }
}

/// The top of the water at (x, z): a river's surface where there is one,
/// else the sea.
pub fn water_top(ctx: &Ctx, x: f32, z: f32) -> f32 {
    ctx.terrain.inland_water(x, z).map_or(ctx.terrain.sea, |w| w.0)
}
