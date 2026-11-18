//! Speed tokens: a thin trail of rare boost pads that only shows while the
//! bike is out.
//!
//! Each 384 m cell of the world may hold one trail, and about half do: a
//! chain of eight to fourteen tokens stepping 26 m at a time along a heading
//! that wanders as it goes. A token's (x, z) comes from the world seed alone,
//! so a trail is in the same place every ride. Its height waits until the
//! ground under it has streamed in, and a spot that turns out to be steep,
//! walled in or under water is dropped rather than placed, which is what
//! keeps the trail somewhere a bike can actually get to. A token taken comes
//! back a minute later, so a good line can be ridden again.

use super::{ground_at, hits_hard, Ground};
use crate::models::{Instance, Model};
use crate::util::FxHashSet as HashSet;
use crate::world::noise::{hash2, Rng};
use crate::world::store::Store;
use glam::{Quat, Vec3};

/// Side of the cell that holds at most one trail, metres.
const CELL: f32 = 384.0;
/// Metres between tokens along a trail.
const STEP: f32 = 26.0;
/// Tokens are given a height once the rider is this close...
const PLACE: f32 = 170.0;
/// ...and forgotten again beyond this, so the trail costs nothing to leave.
const FORGET: f32 = 300.0;
/// Seconds before a token taken comes back.
const RETURN: f32 = 60.0;
/// The token's middle above the ground, metres (the model's own centre is
/// half a metre up, so this is how far its base is lifted).
pub const LIFT: f32 = 0.45;
/// Reach of a pickup, metres across and up.
const GRAB: (f32, f32) = (1.6, 1.5);

/// One token's place on the map, before the ground under it is known.
struct Spot {
    x: f32,
    z: f32,
    /// The ground here turned out to be no good: never place it.
    bad: bool,
    /// Standing in the world now (`Trail::live` holds it).
    live: bool,
}

/// A token standing in the world.
pub struct Token {
    pub pos: Vec3,
    /// Seconds until it comes back; 0 when it is there to be taken.
    pub taken: f32,
    /// Index into `Trail::spots`.
    spot: usize,
    /// Its own place in the bob and the spin, radians.
    phase: f32,
}

#[derive(Default)]
pub struct Trail {
    /// The world's seed; trails are the same on every ride of a world.
    pub seed: u32,
    /// Cells whose trail has been worked out.
    cells: HashSet<(i32, i32)>,
    spots: Vec<Spot>,
    live: Vec<Token>,
    t: f32,
}

impl Trail {
    /// Plan the trails around the rider, give heights to the ones in reach,
    /// and let taken ones come back. Call only while the bike is out.
    pub fn step(&mut self, store: &Store, at: Vec3, sea: f32, slopes: bool, dt: f32) {
        self.t += dt;
        self.plan(at);
        self.place(store, at, sea, slopes);
        for tk in &mut self.live {
            tk.taken = (tk.taken - dt).max(0.0);
        }
        let spots = &mut self.spots;
        self.live.retain(|tk| {
            let keep = (Vec3::new(tk.pos.x - at.x, 0.0, tk.pos.z - at.z)).length() < FORGET;
            if !keep {
                spots[tk.spot].live = false;
            }
            keep
        });
    }

    /// Take the token the rider has reached, if any.
    pub fn take(&mut self, at: Vec3) -> bool {
        let mut hit = None;
        for (i, tk) in self.live.iter().enumerate() {
            if tk.taken > 0.0 {
                continue;
            }
            let d = Vec3::new(tk.pos.x - at.x, 0.0, tk.pos.z - at.z).length();
            if d < GRAB.0 && (tk.pos.y + 0.5 - (at.y + 0.8)).abs() < GRAB.1 {
                hit = Some(i);
                break;
            }
        }
        match hit {
            Some(i) => {
                self.live[i].taken = RETURN;
                true
            }
            None => false,
        }
    }

    /// The tokens to draw: each bobs and spins, and one taken is gone.
    pub fn instances(&self, model: usize, md: &Model, out: &mut Vec<(usize, Instance)>) {
        for tk in &self.live {
            if tk.taken > 0.0 {
                continue;
            }
            let bob = (self.t * 1.6 + tk.phase).sin() * 0.1;
            let spin = self.t * 1.3 + tk.phase;
            let q = Quat::from_rotation_y(spin);
            out.push((
                model,
                Instance {
                    pos_yaw: [tk.pos.x, tk.pos.y + bob, tk.pos.z, 0.0],
                    info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                    tint: [1.0, 1.0, 1.0, 0.0],
                    rot: [q.x, q.y, q.z, q.w],
                    shift: [0.0; 4],
                },
            ));
        }
    }

    /// Work out the (x, z) of every token in the cells around the rider.
    fn plan(&mut self, at: Vec3) {
        let (pcx, pcz) = ((at.x / CELL).floor() as i32, (at.z / CELL).floor() as i32);
        for cz in pcz - 1..=pcz + 1 {
            for cx in pcx - 1..=pcx + 1 {
                if !self.cells.insert((cx, cz)) {
                    continue;
                }
                let h = hash2(cx, cz, self.seed ^ 0x7b01_55a3);
                let mut r = Rng::new(h);
                // Only about half the cells carry a trail at all.
                if r.unit() > 0.55 {
                    continue;
                }
                let mut x = (cx as f32 + 0.15 + 0.7 * r.unit()) * CELL;
                let mut z = (cz as f32 + 0.15 + 0.7 * r.unit()) * CELL;
                let mut head = r.unit() * std::f32::consts::TAU;
                let n = 8 + (r.unit() * 7.0) as i32;
                for _ in 0..n {
                    self.spots.push(Spot { x, z, bad: false, live: false });
                    head += r.signed() * 0.45;
                    x += head.cos() * STEP;
                    z += head.sin() * STEP;
                }
            }
        }
    }

    /// Give a height to the spots the rider has come near, and drop the ones
    /// whose ground is steep, under water or built over.
    fn place(&mut self, store: &Store, at: Vec3, sea: f32, slopes: bool) {
        for (i, s) in self.spots.iter_mut().enumerate() {
            if s.bad || s.live || Vec3::new(s.x - at.x, 0.0, s.z - at.z).length() > PLACE {
                continue;
            }
            // Wait for the ground: unloaded is not the same as no good.
            if !store.loaded((s.x * 4.0).floor() as i32, (at.y * 4.0).floor() as i32, (s.z * 4.0).floor() as i32) {
                continue;
            }
            let top = at.y + 24.0;
            let Ground::At(h, _) = ground_at(store, s.x, s.z, top, 80.0, slopes) else {
                s.bad = true;
                continue;
            };
            // Somewhere a bike can take it: dry, not a cliff, nothing in the way.
            let rough = [(2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)]
                .iter()
                .any(|(dx, dz)| match ground_at(store, s.x + dx, s.z + dz, top, 80.0, slopes) {
                    Ground::At(n, _) => (n - h).abs() > 1.6,
                    _ => true,
                });
            if h < sea + 0.6 || rough || hits_hard(store, Vec3::new(s.x, h + 1.1, s.z), 0.5, 1.6) {
                s.bad = true;
                continue;
            }
            s.live = true;
            self.live.push(Token {
                pos: Vec3::new(s.x, h + LIFT, s.z),
                taken: 0.0,
                spot: i,
                phase: (i as f32) * 2.399,
            });
        }
    }

    /// Put one token at (x, z), skipping the seeded plan (tests).
    #[cfg(test)]
    fn plant(&mut self, x: f32, z: f32) {
        self.spots.push(Spot { x, z, bad: false, live: false });
    }

    /// Tokens standing now (tests).
    #[cfg(test)]
    fn standing(&self) -> usize {
        self.live.iter().filter(|t| t.taken <= 0.0).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testutil::land;

    /// The trails are sparse, and each is one chain: every token is a step
    /// from the one before it, or starts a new trail well away.
    #[test]
    fn trails_are_sparse_chains() {
        let mut t = Trail { seed: 7, ..Default::default() };
        for (x, z) in [(0.0, 0.0), (1200.0, 0.0), (0.0, 1200.0), (1200.0, 1200.0)] {
            t.plan(Vec3::new(x, 20.0, z));
        }
        // 36 cells planned, about half with a trail of 8 to 14 tokens.
        assert!(t.cells.len() == 36, "cells {}", t.cells.len());
        assert!((80..260).contains(&t.spots.len()), "tokens {}", t.spots.len());
        let mut chain = 1;
        for w in t.spots.windows(2) {
            let d = ((w[1].x - w[0].x).powi(2) + (w[1].z - w[0].z).powi(2)).sqrt();
            if (d - STEP).abs() < 0.01 {
                chain += 1;
            } else {
                assert!(d > 50.0, "tokens {d} m apart: neither a step nor a new trail");
                assert!((8..=14).contains(&chain), "a trail of {chain} tokens");
                chain = 1;
            }
        }
    }

    /// A token on open ground stands up, is taken by riding into it, and
    /// comes back a minute later. Cliffs and water take no token at all.
    #[test]
    fn a_token_stands_on_open_ground() {
        let s = land(|_| 20.0);
        let mut t = Trail { seed: 1, ..Default::default() };
        t.plant(40.0, 0.0);
        let at = Vec3::new(40.0, 20.0, 0.0);
        t.step(&s, at, -100.0, true, 1.0 / 60.0);
        assert_eq!(t.standing(), 1, "should have stood up on the flat");
        assert!(t.take(at), "riding into it should take it");
        assert_eq!(t.standing(), 0);
        for _ in 0..61 {
            t.step(&s, at, -100.0, true, 1.0);
        }
        assert_eq!(t.standing(), 1, "should come back after a minute");
    }

    #[test]
    fn a_cliff_edge_takes_no_token() {
        let s = land(|x| if x < 60.0 { 20.0 } else { 40.0 });
        let mut t = Trail { seed: 1, ..Default::default() };
        t.plant(60.0, 0.0);
        t.step(&s, Vec3::new(58.0, 20.0, 0.0), -100.0, true, 1.0 / 60.0);
        assert_eq!(t.standing(), 0, "a step of 20 m is not a place for a token");
    }

    #[test]
    fn under_water_takes_no_token() {
        let s = land(|_| 20.0);
        let mut t = Trail { seed: 1, ..Default::default() };
        t.plant(40.0, 0.0);
        t.step(&s, Vec3::new(40.0, 20.0, 0.0), 24.0, true, 1.0 / 60.0);
        assert_eq!(t.standing(), 0, "the sea is over it");
    }
}
