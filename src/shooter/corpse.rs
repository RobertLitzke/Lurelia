//! A dead bot's body: a ragdoll of points and sticks.
//!
//! The figure's own ragdoll (`anim::ragdoll`) turns joints about a body that
//! is one box at the feet, and the box is what falls. That is right for an
//! animal settling where it lay down and wrong for a body thrown by a shot:
//! the feet stay where the box is and the rest swings about them, so a
//! corpse drifted down like a balloon on a string. The chunks a blast
//! throws (`gibs.rs`) looked right because each one is its own body.
//!
//! So a corpse is the same idea held together. Every joint is a point that
//! falls, bounces and slides on its own, a stick of fixed length runs from
//! each joint to its parent's, and the joints that hang off one parent are
//! braced to each other and to the parent's parent, so a chest stays a
//! chest while an arm folds at the elbow. Each part is drawn at its joint,
//! turned to follow the stick it points along. A body shot in the legs goes
//! over backwards from the legs; one thrown by a rocket flies and lands
//! limbs first, and nothing is pinned.
//!
//! It is its own thing rather than a state of the bot: the bot comes back at
//! a spawn while its body is still lying where it fell.

use crate::world::material::AIR;
use crate::world::noise::Rng;
use crate::world::store::Store;
use glam::{Quat, Vec3};

/// Metres a second squared: what the dead fall under (`physics::tumble`).
const GRAVITY: f32 = 24.0;
/// How much of its speed a point keeps each step in the air, so a body
/// does not swing for ever.
const AIR_KEEP: f32 = 0.995;
/// Of its speed along the floor, what a point touching it keeps each step.
const FLOOR_KEEP: f32 = 0.55;
/// Of its speed into a surface, what comes back out.
const BOUNCE: f32 = 0.25;
/// Points are the middles of joints; a limb has thickness round them, so a
/// point stops this far off whatever it lands on.
const SKIN: f32 = 0.07;
/// Passes over the sticks each step. More is stiffer.
const PASSES: usize = 8;
/// How far past its end a part with nothing hanging off it reaches, metres:
/// the point its direction is read from.
const TIP: f32 = 0.16;
/// The most corpses kept at once; the oldest goes first.
pub const MOST: usize = 8;

struct Stick {
    a: usize,
    b: usize,
    len: f32,
    /// The part the stick belongs to: it goes slack once that part is shot
    /// off (`Corpse::missing`). None for a brace, which belongs to no part.
    part: Option<usize>,
}

pub struct Corpse {
    /// Whose body it was: its paint and its lamps.
    pub bot: usize,
    /// Seconds it has been down, and seconds before it is cleared away.
    pub age: f32,
    pub left: f32,
    /// The points, where they were a step ago (their speed is the
    /// difference), and where they were at the start of this step, which is
    /// what drawing between steps lerps from.
    pts: Vec<Vec3>,
    prev: Vec<Vec3>,
    was: Vec<Vec3>,
    sticks: Vec<Stick>,
    /// For each part: the point it points at, and that direction in its own
    /// frame, taken at the moment of death.
    aim: Vec<usize>,
    local: Vec<Vec3>,
    /// Each part's orientation, now and a step ago.
    rot: Vec<Quat>,
    prev_rot: Vec<Quat>,
    /// The chest lamps: which part each hangs off, and where in its frame.
    lamps: Vec<(usize, Vec3)>,
    /// Parts shot off it (bit `k` for part `k`, `LAMP_BIT + j` for lamp
    /// `j`) and the damage put towards the next piece (`Shooter::chip`).
    pub missing: u64,
    pub owed: f32,
}

/// Where the lamps start in `missing`: past any part.
pub const LAMP_BIT: usize = 48;

impl Corpse {
    /// A body going down. `parents` is each part's parent in the rig,
    /// `parts` each part's joint and orientation in the world at the moment
    /// it died, `lamps` where its lamps were, `vel` what the killing blow
    /// left in it, `dt` the step it will be moved on.
    pub fn new(bot: usize, parents: &[Option<usize>], parts: &[(Vec3, Quat)], lamps: &[Vec3], vel: Vec3, life: f32, seed: u32, dt: f32) -> Corpse {
        let n = parts.len();
        let mut pts: Vec<Vec3> = parts.iter().map(|p| p.0).collect();
        let kids: Vec<Vec<usize>> = (0..n).map(|k| (0..n).filter(|&c| parents[c] == Some(k)).collect()).collect();
        let mut sticks = Vec::new();
        let mut aim = vec![0; n];
        for k in 0..n {
            if let Some(p) = parents[k] {
                sticks.push(Stick { a: k, b: p, len: pts[k].distance(pts[p]), part: Some(k) });
            }
            // What the part points at: its furthest child, or a tip of its
            // own past its end along the way its parent points into it.
            match kids[k].iter().copied().max_by(|&a, &b| pts[a].distance(pts[k]).total_cmp(&pts[b].distance(pts[k]))) {
                Some(c) if pts[c].distance(pts[k]) > 0.02 => aim[k] = c,
                _ => {
                    let from = parents[k].map_or(pts[k] + Vec3::Y, |p| pts[p]);
                    let dir = (pts[k] - from).normalize_or(Vec3::NEG_Y);
                    pts.push(pts[k] + dir * TIP);
                    let t = pts.len() - 1;
                    sticks.push(Stick { a: k, b: t, len: TIP, part: Some(k) });
                    aim[k] = t;
                }
            }
        }
        // Braces: the joints hanging off one parent hold their places
        // round it, and to the parent's own parent, so a torso keeps its
        // shape. A parent with one child (an elbow) is left free to bend.
        for k in 0..n {
            if kids[k].len() < 2 {
                continue;
            }
            for (i, &a) in kids[k].iter().enumerate() {
                for &b in &kids[k][i + 1..] {
                    sticks.push(Stick { a, b, len: pts[a].distance(pts[b]), part: None });
                }
                if let Some(g) = parents[k] {
                    sticks.push(Stick { a, b: g, len: pts[a].distance(pts[g]), part: None });
                }
            }
        }
        let local = (0..n).map(|k| parts[k].1.inverse() * (pts[aim[k]] - pts[k]).normalize_or(Vec3::Y)).collect();
        let rot: Vec<Quat> = parts.iter().map(|p| p.1).collect();
        // Every point given the blow, and a little of its own so no two
        // bodies go down the same way.
        let mut rng = Rng::new(seed | 1);
        let prev = pts
            .iter()
            .map(|&p| {
                let wobble = Vec3::new(rng.unit() - 0.5, rng.unit() * 0.5, rng.unit() - 0.5) * 1.2;
                p - (vel + wobble) * dt
            })
            .collect();
        let lamps = lamps
            .iter()
            .map(|&l| {
                let k = (0..n).min_by(|&a, &b| parts[a].0.distance(l).total_cmp(&parts[b].0.distance(l))).unwrap_or(0);
                (k, parts[k].1.inverse() * (l - parts[k].0))
            })
            .collect();
        Corpse {
            bot,
            age: 0.0,
            left: life,
            was: pts.clone(),
            pts,
            prev,
            sticks,
            aim,
            local,
            prev_rot: rot.clone(),
            rot,
            lamps,
            missing: 0,
            owed: 0.0,
        }
    }

    /// One step: fall, hit the world a point at a time, pull the sticks
    /// back to length, and turn every part to follow its stick.
    pub fn step(&mut self, store: &Store, dt: f32) {
        self.age += dt;
        self.left -= dt;
        self.was.clone_from(&self.pts);
        self.prev_rot.clone_from(&self.rot);
        let solid = |p: Vec3| {
            let v = (p * 4.0).floor();
            store.get(v.x as i32, v.y as i32, v.z as i32).is_some_and(|m| m != AIR)
        };
        for i in 0..self.pts.len() {
            let p = self.pts[i];
            let mut v = (p - self.prev[i]) * AIR_KEEP;
            v.y -= GRAVITY * dt * dt;
            let mut q = p;
            // One axis at a time, down first, so a point that lands slides
            // rather than sticking where it touched.
            for axis in [1usize, 0, 2] {
                let mut next = q;
                next[axis] += v[axis];
                let probe = if axis == 1 { next - Vec3::Y * SKIN } else { next };
                if solid(probe) {
                    if axis == 1 && v.y < 0.0 {
                        v.x *= FLOOR_KEEP;
                        v.z *= FLOOR_KEEP;
                    }
                    v[axis] = -v[axis] * BOUNCE;
                } else {
                    q = next;
                }
            }
            self.prev[i] = q - v;
            self.pts[i] = q;
        }
        for _ in 0..PASSES {
            for s in &self.sticks {
                if s.part.is_some_and(|k| self.missing & (1 << k) != 0) {
                    continue;
                }
                let d = self.pts[s.b] - self.pts[s.a];
                let len = d.length();
                if len < 1e-5 {
                    continue;
                }
                let fix = d * ((len - s.len) / len * 0.5);
                // A correction that would push a point into the world is
                // not made: the floor wins over the stick.
                let (na, nb) = (self.pts[s.a] + fix, self.pts[s.b] - fix);
                if !solid(na - Vec3::Y * SKIN) {
                    self.pts[s.a] = na;
                }
                if !solid(nb - Vec3::Y * SKIN) {
                    self.pts[s.b] = nb;
                }
            }
        }
        for k in 0..self.rot.len() {
            let want = (self.pts[self.aim[k]] - self.pts[k]).normalize_or_zero();
            if want == Vec3::ZERO {
                continue;
            }
            let now = self.rot[k] * self.local[k];
            self.rot[k] = (Quat::from_rotation_arc(now, want) * self.rot[k]).normalize();
        }
    }

    /// Each part still on it, `alpha` of the way through the step: which
    /// part, its joint and its orientation.
    pub fn parts(&self, alpha: f32) -> impl Iterator<Item = (usize, Vec3, Quat)> + '_ {
        (0..self.rot.len()).filter(move |k| self.missing & (1 << k) == 0).map(move |k| (k, self.was[k].lerp(self.pts[k], alpha), self.prev_rot[k].slerp(self.rot[k], alpha)))
    }

    /// Each lamp still on it: which, and where.
    pub fn lamps(&self, alpha: f32) -> impl Iterator<Item = (usize, Vec3)> + '_ {
        self.lamps.iter().enumerate().filter(move |(j, _)| self.missing & (1 << (LAMP_BIT + j)) == 0).map(move |(j, &(k, off))| {
            let at = self.was[k].lerp(self.pts[k], alpha);
            let rot = self.prev_rot[k].slerp(self.rot[k], alpha);
            (j, at + rot * off)
        })
    }

    /// Whether part `k` still has a part hanging off it.
    pub fn holds(&self, parents: &[Option<usize>], k: usize) -> bool {
        parents.iter().enumerate().any(|(c, p)| *p == Some(k) && self.missing & (1 << c) == 0)
    }

    /// The lowest point of it, for tests.
    #[cfg(test)]
    pub fn lowest(&self) -> f32 {
        self.pts.iter().map(|p| p.y).fold(f32::MAX, f32::min)
    }

    /// The highest point, for tests.
    #[cfg(test)]
    pub fn highest(&self) -> f32 {
        self.pts.iter().map(|p| p.y).fold(f32::MIN, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    fn floor() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -40..40 {
            for z in -40..40 {
                for y in 0..4 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    /// A stick figure standing on the floor: hips, chest, head, two legs
    /// of two parts, two arms of two parts.
    fn figure() -> (Vec<Option<usize>>, Vec<(Vec3, Quat)>) {
        let at = |x: f32, y: f32| (Vec3::new(x, 1.0 + y, 0.0), Quat::IDENTITY);
        let parents = vec![None, Some(0), Some(1), Some(0), Some(3), Some(0), Some(5), Some(1), Some(7), Some(1), Some(9)];
        let parts = vec![at(0.0, 0.9), at(0.0, 1.3), at(0.0, 1.65), at(-0.12, 0.85), at(-0.12, 0.45), at(0.12, 0.85), at(0.12, 0.45), at(-0.25, 1.45), at(-0.3, 1.15), at(0.25, 1.45), at(0.3, 1.15)];
        (parents, parts)
    }

    /// Dropped where it stood with nothing in it, a body goes over and ends
    /// lying on the floor: everything near the floor, nothing through it,
    /// nothing still standing up.
    #[test]
    fn a_body_falls_down_and_lies_on_the_floor() {
        let store = floor();
        let (parents, parts) = figure();
        let mut c = Corpse::new(1, &parents, &parts, &[], Vec3::new(1.5, 0.0, 0.0), 10.0, 7, 1.0 / 60.0);
        for _ in 0..360 {
            c.step(&store, 1.0 / 60.0);
        }
        assert!(c.lowest() > 0.95, "a point went through the floor at 1 m: {}", c.lowest());
        assert!(c.highest() < 1.5, "it is still standing: its top is at {}", c.highest());
    }

    /// Thrown, it flies as a whole: the feet leave the floor with the rest
    /// rather than staying where it stood while the body swings round them.
    #[test]
    fn a_thrown_body_takes_its_feet_with_it() {
        let store = floor();
        let (parents, parts) = figure();
        let feet = parts[4].0;
        let mut c = Corpse::new(1, &parents, &parts, &[], Vec3::new(8.0, 6.0, 0.0), 10.0, 7, 1.0 / 60.0);
        for _ in 0..20 {
            c.step(&store, 1.0 / 60.0);
        }
        let (_, foot, _) = c.parts(1.0).find(|p| p.0 == 4).unwrap();
        assert!(foot.x - feet.x > 1.5, "the feet stayed put: they moved {:.2} m", foot.x - feet.x);
        assert!(foot.y > feet.y + 0.3, "the feet never left the floor");
        // And its parts keep their lengths.
        let (_, hip, _) = c.parts(1.0).find(|p| p.0 == 3).unwrap();
        assert!((hip.distance(foot) - 0.4).abs() < 0.05, "the shin is {:.2} m long, not 0.4", hip.distance(foot));
    }
}
