//! Walkers keeping out of each other.
//!
//! The collision box every walker has (`physics::Body`) is a square `half_w`
//! across: the right size for a deer and a fifth of a triceratops, and it
//! keeps a body out of the ground and the walls, not out of other animals.
//!
//! Between animals each walker is a capsule on the flat instead: a line from
//! snout to rump along its heading (`Model::reach`, measured from its voxels
//! when the library loads it), `half_w` thick, between its feet and the top
//! of its box. Once a step, after everyone has moved, any two capsules that
//! overlap are pushed apart along the line between their nearest points,
//! the lighter animal giving way more, and never into a wall
//! (`physics::nudge`). herd.rs steers by the same shapes, so members space
//! by the gap between their bodies rather than between their feet.

use super::walk::Critter;
use super::Look;
use crate::models::Model;
use crate::physics::Body;
use crate::world::store::Store;
use glam::{Vec2, Vec3};

/// The furthest one step's correction moves a body, metres a second, for a
/// shallow overlap: two animals leaning into each other ease apart rather
/// than jumping. A deep one (two running into each other at a bolt's
/// speed) closes `DEEP_SHARE` of itself each step, whatever that is.
const EASE_MS: f32 = 3.0;
const DEEP_SHARE: f32 = 0.5;

/// The share of the measured length a capsule spans. A tail tip or a horn
/// may brush a neighbour; the bulk of the body may not.
const SPAN: f32 = 0.9;

/// One walker as the crowd sees it: a capsule on the flat.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    /// The two ends of its spine, on the flat (x, z).
    pub a: Vec2,
    pub b: Vec2,
    /// How thick it is either side of the spine.
    pub r: f32,
    /// Feet and the top of the box.
    pub y0: f32,
    pub y1: f32,
}

impl Shape {
    /// The capsule of a body with feet at `pos` facing `yaw`, of a model
    /// that reaches `reach` along its own z either side of its feet
    /// (`Model::reach`: the snout end first), scaled by `scale`. `a` is the
    /// snout end of the spine and `b` the rump end.
    pub fn of(pos: Vec3, yaw: f32, half_w: f32, height: f32, reach: [f32; 2], scale: f32) -> Shape {
        let r = half_w * scale;
        let (lo, hi) = (reach[0] * scale * SPAN, reach[1] * scale * SPAN);
        // The spine stops a radius short of each end, so the round of the
        // capsule is the snout and the rump.
        let (lo, hi) = if hi - lo > 2.0 * r { (lo + r, hi - r) } else { ((lo + hi) * 0.5, (lo + hi) * 0.5) };
        // A model's own +z, turned by yaw (anim `push_parts_at`).
        let along = Vec2::new(yaw.sin(), yaw.cos());
        let at = Vec2::new(pos.x, pos.z);
        Shape {
            a: at + along * lo,
            b: at + along * hi,
            r,
            y0: pos.y,
            y1: pos.y + height * scale,
        }
    }

    /// A walker's capsule: its model's reach (its box, for a model that is
    /// not in `models`) at its morph's scale.
    pub fn of_walker(look: &Look, body: &Body, models: &[Model], morph: Option<&crate::variety::Morph>) -> Shape {
        let reach = models.get(look.model).map_or([-body.half_w, body.half_w], |m| m.reach);
        Shape::of(look.pos, look.yaw, body.half_w, body.height, reach, morph.map_or(1.0, |m| m.scale))
    }

    /// The middle of its spine.
    pub fn mid(&self) -> Vec2 {
        (self.a + self.b) * 0.5
    }

    /// How far the point `q` is from its spine.
    pub fn spine_dist(&self, q: Vec2) -> f32 {
        let (p, _) = closest(self.a, self.b, q, q);
        (p - q).length()
    }

    /// How far from its middle any part of it reaches.
    pub fn bound(&self) -> f32 {
        (self.b - self.a).length() * 0.5 + self.r
    }

    /// The nearest points of the two spines, this one's first.
    pub fn nearest(&self, o: &Shape) -> (Vec2, Vec2) {
        closest(self.a, self.b, o.a, o.b)
    }

    /// The way out of `o` and how far, when the two overlap: the push on
    /// this one alone. None when they are clear, or a storey apart.
    pub fn push_from(&self, o: &Shape) -> Option<Vec2> {
        if self.y1 <= o.y0 || o.y1 <= self.y0 {
            return None;
        }
        let (p, q) = self.nearest(o);
        let d = p - q;
        let len = d.length();
        let over = self.r + o.r - len;
        if over <= 0.0 {
            return None;
        }
        // Spines that cross have no way out between their nearest points:
        // go apart along the line between their middles instead.
        let n = if len > 1e-3 { d / len } else { (self.mid() - o.mid()).try_normalize().unwrap_or(Vec2::X) };
        Some(n * over)
    }
}

/// The nearest points of segments p1-q1 and p2-q2 (Ericson, Real-Time
/// Collision Detection, 5.1.9).
fn closest(p1: Vec2, q1: Vec2, p2: Vec2, q2: Vec2) -> (Vec2, Vec2) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    const EPS: f32 = 1e-6;
    let (s, t) = if a <= EPS && e <= EPS {
        (0.0, 0.0)
    } else if a <= EPS {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > EPS { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p1 + d1 * s, p2 + d2 * t)
}

/// How far a model's body runs along its own z either side of its feet,
/// metres, from its voxels: the first and last z slices holding more than a
/// couple of voxels (so the tip of a tail or a single claw does not count).
/// The low end is the snout, as a model faces -Z. Never shorter than the box.
pub fn measure_reach(cells: &[u8], dims: crate::anim::slice::Dims, voxel: f32, half_w: f32) -> [f32; 2] {
    let [nx, ny, nz] = dims.0;
    let mut per_z = vec![0u32; nz.max(0) as usize];
    for y in 0..ny {
        for z in 0..nz {
            for x in 0..nx {
                if cells.get(dims.idx(x, y, z)).is_some_and(|&c| c != 0) {
                    per_z[z as usize] += 1;
                }
            }
        }
    }
    let solid = |n: &u32| *n > 2;
    let (Some(lo), Some(hi)) = (per_z.iter().position(solid), per_z.iter().rposition(solid)) else {
        return [-half_w, half_w];
    };
    let pivot = dims.pivot().z;
    let lo = (lo as f32 - pivot) * voxel;
    let hi = (hi as f32 + 1.0 - pivot) * voxel;
    [lo.min(-half_w), hi.max(half_w)]
}

/// The reach of a body whose long neck bends or whose tail is dragged
/// (anim/trail.rs `flexible`, `dragged_tail`): a neck or tail that swings
/// out of a neighbour's way doesn't keep animals apart, so the capsule runs
/// from the root of the neck to the root of the tail, half a bone out past
/// each to keep the chest and the rump.
pub fn trunk_reach(reach: [f32; 2], rig: &crate::anim::Rig, pivot_z: f32, voxel: f32, half_w: f32) -> [f32; 2] {
    use crate::anim::{trail, Role};
    let mut out = reach;
    // The model faces -Z: the neck is at the low end, the tail at the high.
    if let Some(ch) = trail::flexible(rig, Role::Neck) {
        let z = rig.bones[ch.bones[0]].joint.z - ch.d[0] * 0.5;
        out[0] = out[0].max((z - pivot_z) * voxel);
    }
    if let Some(ch) = trail::dragged_tail(rig) {
        let z = rig.bones[ch.bones[0]].joint.z + ch.d[0] * 0.5;
        out[1] = out[1].min((z - pivot_z) * voxel);
    }
    if out[1] - out[0] < 2.0 * half_w {
        let mid = (out[0] + out[1]) * 0.5;
        out = [mid - half_w, mid + half_w];
    }
    out
}

/// Every walker on its feet, as the crowd sees it.
pub fn shapes(world: &hecs::World, models: &[Model]) -> Vec<(hecs::Entity, Shape, f32)> {
    world
        .query::<(hecs::Entity, &Critter, &Body, &Look, Option<&crate::variety::Morph>)>()
        .iter()
        .filter(|(_, c, _, _, _)| c.air.is_none())
        .map(|(e, _, b, l, m)| {
            let s = Shape::of_walker(l, b, models, m);
            // Heavier the bigger it is: width, length and height together.
            let mass = (s.r * 2.0) * (s.bound() * 2.0) * (s.y1 - s.y0).max(0.1);
            (e, s, mass)
        })
        .collect()
}

/// Push apart any two walkers standing in each other. Returns how many
/// pairs overlapped, for the headless stats.
pub fn update(world: &mut hecs::World, models: &[Model], store: &Store, dt: f32) -> u32 {
    let mut all = shapes(world, models);
    if all.len() < 2 {
        return 0;
    }
    // Sweep along x: sorted by where each one's reach starts, a pair can
    // only touch while the second starts before the first ends.
    all.sort_by(|a, b| (a.1.mid().x - a.1.bound()).total_cmp(&(b.1.mid().x - b.1.bound())));
    let mut by = vec![Vec2::ZERO; all.len()];
    let mut pairs = 0;
    let mut deepest = 0.0f32;
    for i in 0..all.len() {
        let (_, si, mi) = all[i];
        let end = si.mid().x + si.bound();
        for j in i + 1..all.len() {
            let (_, sj, mj) = all[j];
            if sj.mid().x - sj.bound() > end {
                break;
            }
            if (si.mid().y - sj.mid().y).abs() > si.bound() + sj.bound() {
                continue;
            }
            let Some(push) = si.push_from(&sj) else { continue };
            pairs += 1;
            deepest = deepest.max(push.length());
            // The lighter one gives way: a calf against a bull moves
            // nearly all of the difference.
            let share = mj / (mi + mj);
            by[i] += push * share;
            by[j] -= push * (1.0 - share);
        }
    }
    // Dev hook: VG_CROWD=1 prints, every 60th call (once a second at the
    // game's 60 Hz step), how many walkers are in each other and how deep
    // the worst of them is; VG_CROWD=still prints the same without pushing
    // anyone.
    static LOG: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    static TICK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let log = *LOG.get_or_init(|| std::env::var("VG_CROWD").ok().map(|v| v == "still"));
    if log.is_some() && TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
        // And the fastest any big one is turning, for spotting a spin.
        let spin = world
            .query::<(&Look, &Body)>()
            .iter()
            .filter(|(_, b)| super::trample::big(b.half_w, b.height))
            .map(|(l, _)| crate::util::wrap_angle(l.yaw - l.prev_yaw).abs() / dt.max(1e-4))
            .fold(0.0f32, f32::max);
        eprintln!("crowd: {} walkers, {pairs} overlapping, deepest {deepest:.2} m, fastest big turn {spin:.2} rad/s", all.len());
    }
    if log == Some(true) {
        return pairs;
    }
    let cap = EASE_MS * dt;
    for (k, (e, _, _)) in all.iter().enumerate() {
        let d = by[k];
        if d == Vec2::ZERO {
            continue;
        }
        let cap = cap.max(d.length() * DEEP_SHARE);
        let d = if d.length() > cap { d.normalize() * cap } else { d };
        if let Ok((look, body)) = world.query_one_mut::<(&mut Look, &Body)>(*e) {
            // Out past full detail there are no voxels to keep it out of
            // (walk.rs `far`): it moves over the generator's ground.
            match super::terrain::far_ground(store, look.pos.x + d.x, look.pos.z + d.y) {
                Some(gy) => look.pos = Vec3::new(look.pos.x + d.x, gy, look.pos.z + d.y),
                None => crate::physics::nudge(store, &mut look.pos, Vec3::new(d.x, 0.0, d.y), body.half_w, body.height),
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 9 m animal standing nose to tail behind another, feet 5 m apart:
    /// square boxes 2.2 m across say they are clear, the capsules say the
    /// horns are in the rump.
    #[test]
    fn long_bodies_touch_end_to_end_where_boxes_do_not() {
        let reach = [-4.5, 4.5];
        let a = Shape::of(Vec3::ZERO, 0.0, 1.1, 3.0, reach, 1.0);
        let b = Shape::of(Vec3::new(0.0, 0.0, 5.0), 0.0, 1.1, 3.0, reach, 1.0);
        assert_eq!(crate::physics::separate(Vec3::ZERO, 1.1, 3.0, Vec3::new(0.0, 0.0, 5.0), 1.1, 3.0), Vec3::ZERO);
        let push = a.push_from(&b).expect("end to end they overlap");
        assert!(push.y < 0.0, "pushed back along its own line, got {push}");
        // Side by side 5 m apart they are clear.
        let c = Shape::of(Vec3::new(5.0, 0.0, 0.0), 0.0, 1.1, 3.0, reach, 1.0);
        assert!(a.push_from(&c).is_none());
    }

    #[test]
    fn crossing_spines_still_come_apart() {
        let reach = [-3.0, 3.0];
        let a = Shape::of(Vec3::ZERO, 0.0, 1.0, 3.0, reach, 1.0);
        let b = Shape::of(Vec3::new(0.2, 0.0, 0.0), std::f32::consts::FRAC_PI_2, 1.0, 3.0, reach, 1.0);
        let push = a.push_from(&b).expect("crossed");
        assert!(push.length() > 0.5);
    }

    #[test]
    fn a_storey_apart_is_not_touching() {
        let a = Shape::of(Vec3::ZERO, 0.0, 1.0, 2.0, [-2.0, 2.0], 1.0);
        let b = Shape::of(Vec3::new(0.0, 5.0, 0.0), 0.0, 1.0, 2.0, [-2.0, 2.0], 1.0);
        assert!(a.push_from(&b).is_none());
    }

    #[test]
    fn reach_is_measured_from_the_body_not_the_grid() {
        use crate::anim::slice::Dims;
        let dims = Dims([8, 4, 40]);
        let mut cells = vec![0u8; dims.len()];
        // A body from z 10 to 29 three voxels thick, and a one-voxel tail
        // tip at z 5 that does not count.
        for z in 10..30 {
            for x in 3..6 {
                cells[dims.idx(x, 1, z)] = 1;
            }
        }
        cells[dims.idx(4, 1, 5)] = 1;
        let r = measure_reach(&cells, dims, 0.1, 0.2);
        assert!((r[0] - -1.0).abs() < 1e-4 && (r[1] - 1.0).abs() < 1e-4, "{r:?}");
    }
}
