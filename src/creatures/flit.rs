//! Flitting: small birds (the `flit` movement) that sit on top of a shrub or
//! tree for a few seconds, then hop through the air in a short arc to another
//! top nearby, drop to the ground now and then to peck, and burst away to a
//! farther perch when the player comes close. Each bird moves on its own; a
//! group shares a home it drifts back toward.
//!
//! A display (creatures/display: a lark's song flight, a jay caching) takes
//! a bird over for a while: it carries a `Show` and is put where the display
//! says, and when it ends it carries on from there.

use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;

#[derive(Clone, Debug)]
pub enum State {
    Sit { timer: f32 },
    Fly { from: Vec3, to: Vec3, t: f32, dur: f32 },
}

pub struct Flit {
    pub state: State,
    pub home: Vec3,
    rng: Rng,
    /// In a display last step: whether it was sitting.
    shown: Option<bool>,
}

impl Flit {
    pub fn perched(&self) -> bool {
        matches!(self.state, State::Sit { .. })
    }
    /// The last part of a hop: wings flared, feet forward.
    pub fn landing(&self) -> bool {
        matches!(self.state, State::Fly { t, .. } if t > 0.75)
    }
    fn rand(&mut self) -> f32 {
        self.rng.unit()
    }
}

/// `n` birds of species `s` around ground point `at`.
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, at: Vec3, n: u32, seed: u32) {
    for k in 0..n.max(1) {
        let mut f = Flit {
            state: State::Sit { timer: 0.0 },
            home: at,
            shown: None,
            rng: Rng::new(crate::world::noise::hash32(seed ^ k.wrapping_mul(0x85eb_ca6b)) | 1),
        };
        f.state = State::Sit { timer: 0.2 + f.rand() * 2.0 };
        let off = Vec3::new(f.rand() - 0.5, 0.0, f.rand() - 0.5) * 3.0;
        let pos = at + off;
        let yaw = f.rand() * std::f32::consts::TAU;
        world.spawn((f, Creature::of(s), Look::new(sp.model, pos, yaw)));
    }
}

/// Step every flitting bird. Perches are `Terrain::perch` (leaves
/// included); never on wet ground or below the sea.
pub fn update(world: &mut hecs::World, species: &[Species], dt: f32, threat: &super::startle::Threat, ter: &Terrain) {
    let player = threat.pos;
    for (f, look, me, morph, show) in world.query_mut::<(&mut Flit, &mut Look, &Creature, Option<&crate::variety::Morph>, Option<&super::display::Show>)>() {
        look.begin_step();
        let sp = &species[me.species];
        if let Some(s) = show {
            use super::display::Drive;
            match s.out.drive {
                Drive::Place { pos, yaw } => {
                    (look.pos, look.yaw) = (pos, yaw);
                    f.shown = Some(false);
                }
                Drive::Sit { pos, yaw } => {
                    (look.pos, look.yaw) = (pos, yaw);
                    f.shown = Some(true);
                }
                _ => {}
            }
            continue;
        }
        // Out of a display: sit on where it was put, or if it was left in
        // the air, fly to a perch.
        if let Some(sat) = f.shown.take() {
            f.state = State::Sit { timer: 0.8 + f.rand() * 2.0 };
            if !sat {
                let to = choose(f, look.pos, None, 3.0, 14.0, ter).or_else(|| ground_spot(f, look.pos, ter));
                if let Some(to) = to {
                    let dur = ((to - look.pos).length() / sp.beh.walk_speed.max(2.0)).clamp(0.35, 4.0);
                    f.state = State::Fly { from: look.pos, to, t: 0.0, dur };
                }
            }
        }
        let (pace, _) = crate::variety::pace(morph);
        let hop_speed = sp.beh.walk_speed.max(2.0) * pace;
        let flee_speed = sp.beh.flee_speed.max(hop_speed) * pace;
        let nerve = super::startle::Nerve::of(sp, morph, super::age::Stage::Adult);
        let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < super::startle::reach(&nerve, threat, look.pos, 3.0);
        match f.state.clone() {
            State::Sit { timer } => {
                let timer = timer - dt;
                if near || timer <= 0.0 {
                    // Where next: away from the player, or a nearby top,
                    // drifting back toward home when far from it.
                    let (to, speed) = if near {
                        let away = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero();
                        (choose(f, look.pos, Some(away), 12.0, 24.0, ter), flee_speed)
                    } else if f.rand() < 0.2 {
                        (ground_spot(f, look.pos, ter), hop_speed)
                    } else {
                        let back = f.home - look.pos;
                        let bias = (Vec3::new(back.x, 0.0, back.z).length() > 20.0).then(|| Vec3::new(back.x, 0.0, back.z).normalize_or_zero());
                        (choose(f, look.pos, bias, 3.0, 14.0, ter), hop_speed)
                    };
                    match to {
                        Some(to) if (to - look.pos).length() > 0.5 => {
                            let dur = ((to - look.pos).length() / speed).clamp(0.35, 4.0);
                            f.state = State::Fly { from: look.pos, to, t: 0.0, dur };
                        }
                        _ => f.state = State::Sit { timer: 1.0 + f.rand() * 2.0 },
                    }
                } else {
                    f.state = State::Sit { timer };
                    // A hop round to face somewhere else now and then.
                    if f.rand() < dt * 0.3 {
                        look.yaw += (f.rand() - 0.5) * 2.5;
                    }
                }
            }
            State::Fly { from, to, t, dur } => {
                let t = (t + dt / dur).min(1.0);
                let d = to - from;
                let arc = 0.3 + 0.18 * Vec3::new(d.x, 0.0, d.z).length();
                look.pos = from.lerp(to, t) + Vec3::Y * arc * (std::f32::consts::PI * t).sin();
                let flat = Vec3::new(d.x, 0.0, d.z);
                if flat.length() > 0.1 {
                    look.yaw = flat.x.atan2(flat.z) + std::f32::consts::PI;
                }
                if t >= 1.0 {
                    look.pos = to;
                    let on_ground = ter.height(to.x, to.z).is_some_and(|g| to.y - g < 0.3);
                    let timer = if on_ground { 1.5 + f.rand() * 3.0 } else { 2.5 + f.rand() * 7.0 };
                    f.state = State::Sit { timer };
                } else {
                    f.state = State::Fly { from, to, t, dur };
                }
            }
        }
    }
}

/// The best perch among a dozen points `rmin..rmax` away: the one standing
/// highest above the bare ground (a shrub or tree top), else the ground.
fn choose(f: &mut Flit, at: Vec3, toward: Option<Vec3>, rmin: f32, rmax: f32, t: &Terrain) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for _ in 0..12 {
        let a = match toward {
            Some(dir) => dir.x.atan2(dir.z) + (f.rand() - 0.5) * 1.6,
            None => f.rand() * std::f32::consts::TAU,
        };
        let r = rmin + f.rand() * (rmax - rmin);
        let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
        // Under a roof, from under the roof: from the sky it would perch
        // on the land laid over the room (#377).
        let from = match t.roof(at) {
            Some(roof) if t.store.cell((x * 4.0).floor() as i32, ((roof - 0.05) * 4.0).floor() as i32, (z * 4.0).floor() as i32) != crate::world::store::Cell::Open => continue,
            Some(roof) => roof - 0.05,
            None => at.y + 30.0,
        };
        let Some(y) = t.perch(x, z, from) else {
            continue;
        };
        if y < t.sea + 0.5 {
            continue;
        }
        let rise = y - t.height(x, z).unwrap_or(y);
        // A branch over a river will do; the water itself won't.
        if rise < 0.5 && t.wet(x, z) {
            continue;
        }
        // Shrubs and small trees (about 3 m) first, tall crowns least.
        let score = if rise > 0.5 { 4.0 - (rise - 3.0).abs() * 0.4 } else { 0.0 } + f.rand();
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, Vec3::new(x, y, z)));
        }
    }
    best.map(|b| b.1)
}

/// A spot on the ground a couple of metres away, to peck at.
fn ground_spot(f: &mut Flit, at: Vec3, t: &Terrain) -> Option<Vec3> {
    let a = f.rand() * std::f32::consts::TAU;
    let r = 1.5 + f.rand() * 4.0;
    let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
    // From just above the bare ground, so a canopy overhead doesn't count.
    let g = match t.roof(at) {
        Some(_) => super::terrain::ground_near(t.store, x, z, at)?.0,
        None => t.height(x, z)?,
    };
    let y = t.perch(x, z, g + 1.5).unwrap_or(g);
    (y > t.sea + 0.5 && !t.wet(x, z)).then_some(Vec3::new(x, y, z))
}
