//! Woodpeckers (the `cling` movement): a bird that clings upright to a tree
//! trunk, drums at it in quick bursts, hitches up the trunk in short hops
//! and round it, then bounds off to another trunk 8 to 35 m away. It flies
//! to a trunk farther off when the player comes close. Trunks come from the
//! world's placed plants (plants.rs): anything at least 4 m tall with a
//! trunk, snags included.

use super::plants::Flora;
use super::terrain::Terrain;
use super::{Creature, Look, Species};
use crate::world::noise::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// A trunk: its base, radius and height.
#[derive(Clone, Copy, Debug)]
pub struct Trunk {
    pub base: Vec3,
    pub r: f32,
    pub top: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    /// On `trunk`, `h` metres up, on the side facing `angle`.
    Cling { trunk: Trunk, h: f32, angle: f32, timer: f32, hop: f32 },
    Fly {
        from: Vec3,
        to: Vec3,
        t: f32,
        dur: f32,
        trunk: Trunk,
        h: f32,
        angle: f32,
    },
}

pub struct Clinger {
    pub state: State,
    rng: Rng,
}

impl Clinger {
    pub fn clinging(&self) -> bool {
        matches!(self.state, State::Cling { .. })
    }
    pub fn landing(&self) -> bool {
        matches!(self.state, State::Fly { t, .. } if t > 0.8)
    }
}

/// A trunk `rmin..rmax` from `at`, leaning toward `toward` if given.
pub fn pick(flora: &Flora, at: Vec3, rmin: f32, rmax: f32, toward: Option<Vec3>, rng: &mut Rng) -> Option<Trunk> {
    let mut best: Option<(f32, Trunk)> = None;
    for p in flora.near(at.x, at.z, rmax, None) {
        if p.trunk < 0.12 || p.height < 4.0 {
            continue;
        }
        let d = Vec3::new(p.pos.x - at.x, 0.0, p.pos.z - at.z);
        if d.length() < rmin {
            continue;
        }
        let lean = toward.map_or(0.0, |w| d.normalize_or_zero().dot(w) * 2.0);
        let score = lean + rng.unit();
        if best.is_none_or(|b| score > b.0) {
            best = Some((
                score,
                Trunk {
                    base: p.pos,
                    r: p.trunk,
                    top: p.height,
                },
            ));
        }
    }
    best.map(|b| b.1)
}

/// Where the bird's pivot sits clinging to `tr` at height `h` on side `angle`.
fn spot(tr: &Trunk, h: f32, angle: f32, body: f32) -> Vec3 {
    // The trunk tapers to about 0.7 of its radius at the top.
    let r = tr.r * (1.0 - 0.3 * (h / tr.top.max(1.0)).min(1.0)) + 0.03 + body;
    tr.base + Vec3::new(angle.sin() * r, h, angle.cos() * r)
}

fn start_height(tr: &Trunk, rng: &mut Rng) -> f32 {
    1.2 + rng.unit() * (tr.top * 0.5).min(6.0)
}

/// `n` woodpeckers of species `s` on trunks round `at`; false if no trunk.
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, flora: &Flora, at: Vec3, n: u32, seed: u32) -> bool {
    let mut rng = Rng::new(seed | 1);
    let mut any = false;
    for _ in 0..n.max(1) {
        let Some(tr) = pick(flora, at, 0.0, 30.0, None, &mut rng) else {
            break;
        };
        let (h, angle) = (start_height(&tr, &mut rng), rng.unit() * TAU);
        let c = Clinger {
            state: State::Cling {
                trunk: tr,
                h,
                angle,
                timer: 3.0 + rng.unit() * 8.0,
                hop: 1.0,
            },
            rng: Rng::new(rng.state | 1),
        };
        world.spawn((c, Creature::of(s), Look::new(sp.model, spot(&tr, h, angle, 0.1), angle)));
        any = true;
    }
    any
}

/// Step every woodpecker.
pub fn update(world: &mut hecs::World, species: &[Species], flora: &Flora, dt: f32, threat: &super::startle::Threat, _t: &Terrain) {
    let player = threat.pos;
    for (c, look, me, morph) in world.query_mut::<(&mut Clinger, &mut Look, &Creature, Option<&crate::variety::Morph>)>() {
        look.begin_step();
        let sp = &species[me.species];
        let (pace, _) = crate::variety::pace(morph);
        let nerve = super::startle::Nerve::of(sp, morph, super::age::Stage::Adult);
        let body = 0.1;
        match c.state {
            State::Cling {
                trunk,
                mut h,
                mut angle,
                timer,
                mut hop,
            } => {
                let timer = timer - dt;
                hop -= dt;
                let near = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).length() < super::startle::reach(&nerve, threat, look.pos, 4.0);
                if near || timer <= 0.0 || h > trunk.top * 0.85 {
                    let away = Vec3::new(look.pos.x - player.x, 0.0, look.pos.z - player.z).normalize_or_zero();
                    let next = if near {
                        pick(flora, look.pos, 18.0, 40.0, Some(away), &mut c.rng)
                    } else {
                        pick(flora, look.pos, 8.0, 35.0, None, &mut c.rng)
                    };
                    if let Some(tr) = next {
                        let (h2, a2) = (start_height(&tr, &mut c.rng), c.rng.unit() * TAU);
                        let to = spot(&tr, h2, a2, body);
                        let speed = if near { sp.beh.flee_speed } else { sp.beh.walk_speed }.max(3.0) * pace;
                        let dur = ((to - look.pos).length() / speed).clamp(0.8, 8.0);
                        c.state = State::Fly {
                            from: look.pos,
                            to,
                            t: 0.0,
                            dur,
                            trunk: tr,
                            h: h2,
                            angle: a2,
                        };
                        continue;
                    }
                }
                if hop <= 0.0 {
                    // Hitch up the trunk, and now and then round it.
                    h += 0.15 + c.rng.unit() * 0.25;
                    angle += (c.rng.unit() - 0.5) * 0.7;
                    hop = 0.6 + c.rng.unit() * 1.8;
                }
                look.pos = spot(&trunk, h, angle, body);
                look.yaw = angle;
                c.state = State::Cling {
                    trunk,
                    h,
                    angle,
                    timer: timer.max(-1.0),
                    hop,
                };
            }
            State::Fly { from, to, t, dur, trunk, h, angle } => {
                let t = (t + dt / dur).min(1.0);
                let d = to - from;
                // Bounding: a dip in the middle of the flight.
                let dip = -0.25 * Vec3::new(d.x, 0.0, d.z).length().min(8.0) * (PI * t).sin();
                look.pos = from.lerp(to, t) + Vec3::Y * dip;
                if t < 0.9 && d.with_y(0.0).length() > 0.1 {
                    look.yaw = d.x.atan2(d.z) + PI;
                } else {
                    look.yaw = crate::util::turn_toward(look.yaw, angle, (dt * 10.0).min(1.0));
                }
                c.state = if t >= 1.0 {
                    State::Cling {
                        trunk,
                        h,
                        angle,
                        timer: 4.0 + c.rng.unit() * 10.0,
                        hop: 1.0,
                    }
                } else {
                    State::Fly { from, to, t, dur, trunk, h, angle }
                };
            }
        }
    }
}
