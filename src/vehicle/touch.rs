//! Karts meeting each other.
//!
//! A kart's chassis spheres are sized for terrain and leave its wheels
//! outside them, so karts don't meet by them (`vehicle::meet`). To another
//! kart a kart is its footprint on the ground: a rectangle from its
//! wheelbase, track and tyres. Two footprints are tested as boxes in the
//! ground plane, pushed apart by mass (never into a wall: `clear`), and given an
//! impulse with a little bounce and some friction along the contact, so
//! karts rubbing side by side cost each other speed and a hit from behind
//! passes speed forward. Only the yaw is turned, and by no more than
//! `YAW_MAX` a contact: a tap on a rear corner turns a kart, it doesn't spin
//! it. Spinning is what weapons are for.

use super::Vehicle;
use crate::world::store::Store;
use glam::{Vec2, Vec3};

/// Restitution between karts: a little bounce.
const BOUNCE: f32 = 0.3;
/// Sliding friction along the contact.
const RUB: f32 = 0.3;
/// The most a contact turns a kart, rad/s.
const YAW_MAX: f32 = 1.5;
/// Karts further apart in height than this pass without touching, metres,
/// so one landing from a hop doesn't stand on another.
const HEIGHT_GAP: f32 = 0.8;
/// Bumpers: how far the footprint reaches past the tyres front and back.
const NOSE: f32 = 0.1;

/// A kart's footprint: its centre on the ground, its forward and right
/// (unit, on the ground), and half its width and length.
#[derive(Clone, Copy, Debug)]
pub struct Footprint {
    pub c: Vec2,
    pub f: Vec2,
    pub r: Vec2,
    pub half: Vec2,
}

impl Footprint {
    pub fn of(v: &Vehicle) -> Footprint {
        let s = &v.spec;
        let fwd = v.forward();
        let f = Vec2::new(fwd.x, fwd.z).normalize_or(Vec2::NEG_Y);
        let half = match s.bump_box {
            Some([w, l]) => Vec2::new(w * 0.5, l * 0.5),
            None => Vec2::new((s.track + s.tyre_width) * 0.5, s.wheelbase * 0.5 + s.wheel_radius + NOSE),
        };
        Footprint { c: Vec2::new(v.b.pos.x, v.b.pos.z), f, r: Vec2::new(-f.y, f.x), half }
    }

    fn corners(&self) -> [Vec2; 4] {
        let (x, z) = (self.r * self.half.x, self.f * self.half.y);
        [self.c + x + z, self.c - x + z, self.c - x - z, self.c + x - z]
    }

    /// Whether `p` is inside, with `pad` metres to spare.
    fn holds(&self, p: Vec2, pad: f32) -> bool {
        let d = p - self.c;
        d.dot(self.r).abs() <= self.half.x + pad && d.dot(self.f).abs() <= self.half.y + pad
    }

    /// Half its extent along `axis`.
    fn reach(&self, axis: Vec2) -> f32 {
        self.half.x * self.r.dot(axis).abs() + self.half.y * self.f.dot(axis).abs()
    }
}

/// Whether a kart's footprint is out of the rock: its corners and the
/// middles of its sides, at the height of its centre (above the kerbs and
/// the road's voxel steps, below the top of any wall), are all in open air.
pub fn clear(store: &Store, v: &Vehicle) -> bool {
    let f = Footprint::of(v);
    let y = (v.b.pos.y / crate::world::gen::VOXEL).floor() as i32;
    let (x, z) = (f.r * f.half.x, f.f * f.half.y);
    [f.c + x + z, f.c - x + z, f.c - x - z, f.c + x - z, f.c + x, f.c - x, f.c + z, f.c - z].iter().all(|p| {
        let q = (*p / crate::world::gen::VOXEL).floor();
        !store.solid(q.x as i32, y, q.y as i32)
    })
}

/// How two footprints overlap: the normal from `b` to `a`, the depth along
/// it, and the contact point. None when they don't.
pub fn overlap(a: &Footprint, b: &Footprint) -> Option<(Vec2, f32, Vec2)> {
    let d = a.c - b.c;
    let mut best: Option<(Vec2, f32)> = None;
    for axis in [a.f, a.r, b.f, b.r] {
        let pen = a.reach(axis) + b.reach(axis) - d.dot(axis).abs();
        if pen <= 0.0 {
            return None;
        }
        if best.is_none_or(|(_, p)| pen < p) {
            best = Some((if d.dot(axis) >= 0.0 { axis } else { -axis }, pen));
        }
    }
    let (n, pen) = best?;
    // The contact: the corners of each inside the other, averaged; with none
    // (edge on edge) the point between them on the line of centres.
    let inside: Vec<Vec2> = a.corners().into_iter().filter(|&p| b.holds(p, 0.02)).chain(b.corners().into_iter().filter(|&p| a.holds(p, 0.02))).collect();
    let p = if inside.is_empty() {
        b.c + n * (b.reach(n) - pen * 0.5)
    } else {
        inside.iter().copied().sum::<Vec2>() / inside.len() as f32
    };
    Some((n, pen, p))
}

/// Two karts touching: pushed apart and turned. Returns how hard they met,
/// m/s along the contact. With `store`, neither is pushed into a wall: a
/// kart that would no longer fit stays where it was and the other takes the
/// whole push.
pub fn karts(a: &mut Vehicle, b: &mut Vehicle, store: Option<&Store>) -> f32 {
    if (a.b.pos - b.b.pos).length() > 5.0 || (a.b.pos.y - b.b.pos.y).abs() > HEIGHT_GAP {
        return 0.0;
    }
    let (fa, fb) = (Footprint::of(a), Footprint::of(b));
    let Some((n2, pen, p2)) = overlap(&fa, &fb) else { return 0.0 };
    let n = Vec3::new(n2.x, 0.0, n2.y);
    let (ma, mb) = (a.b.mass, b.b.mass);
    let (mut sa, mut sb) = (mb / (ma + mb), ma / (ma + mb));
    let (pa, pb) = (a.b.pos, b.b.pos);
    a.b.pos += n * pen * sa;
    b.b.pos -= n * pen * sb;
    if let Some(store) = store {
        let (oka, okb) = (clear(store, a), clear(store, b));
        if !oka || !okb {
            a.b.pos = pa;
            b.b.pos = pb;
            (sa, sb) = match (oka, okb) {
                (false, true) => (0.0, 1.0),
                (true, false) => (1.0, 0.0),
                _ => (0.0, 0.0),
            };
            a.b.pos += n * pen * sa;
            b.b.pos -= n * pen * sb;
        }
    }

    // The impulse, at the contact on the plan and at the height between the
    // two centres, so it never lifts either kart.
    let p = Vec3::new(p2.x, (a.b.pos.y + b.b.pos.y) * 0.5, p2.y);
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let vr = flat(a.b.point_vel(p) - b.b.point_vel(p));
    let vn = vr.dot(n);
    if vn >= 0.0 {
        return 0.0;
    }
    let (ra, rb) = (p - a.b.pos, p - b.b.pos);
    // Inverse effective mass along a direction on the ground, with the yaw
    // only: karts are turned about their up, never rolled by a touch.
    let (ia, ib) = (1.0 / a.b.inertia.y, 1.0 / b.b.inertia.y);
    let k = |d: Vec3| {
        let (ta, tb) = (ra.cross(d).y, rb.cross(d).y);
        1.0 / ma + 1.0 / mb + ta * ta * ia + tb * tb * ib
    };
    let jn = -(1.0 + BOUNCE) * vn / k(n);
    let vt = vr - n * vn;
    let mut j = n * jn;
    if vt.length() > 1e-3 {
        let t = vt / vt.length();
        let jt = (vt.length() / k(t)).min(RUB * jn);
        j -= t * jt;
    }
    let turn = |r: Vec3, j: Vec3, inv: f32| (r.cross(j).y * inv).clamp(-YAW_MAX, YAW_MAX);
    a.b.vel += j / ma;
    b.b.vel -= j / mb;
    a.b.ang.y += turn(ra, j, ia);
    b.b.ang.y += turn(rb, -j, ib);
    a.asleep = false;
    b.asleep = false;
    -vn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::Spec;

    fn kart() -> Spec {
        crate::vehicle::Catalogue::load(&crate::library::data_root()).get("kart")
    }

    fn at(spec: &Spec, x: f32, z: f32, yaw: f32, vel: Vec3) -> Vehicle {
        let mut v = Vehicle::new(spec.clone(), Vec3::new(x, 0.0, z), yaw);
        v.b.vel = vel;
        v
    }

    /// A kart pinned against a wall by another is not pushed into it: the
    /// other kart takes the whole push.
    #[test]
    fn a_kart_against_a_wall_is_not_pushed_into_it() {
        use crate::world::material::STONE;
        use crate::world::store::Voxels;
        // Flat ground at 4 m, and a wall 3 m high from x = 3 m east.
        let mut store = Store::default();
        for cz in -2..2 {
            for cx in -2..2 {
                for cy in 0..4 {
                    store.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -64..64 {
            for z in -64..64 {
                let top = if x >= 12 { 28 } else { 16 };
                for y in 0..top {
                    store.set(x, y, z, STONE);
                }
            }
        }
        let s = kart();
        let half = Footprint::of(&at(&s, 0.0, 0.0, 0.0, Vec3::ZERO)).half.x;
        let w = half * 2.0;
        let put = |x: f32, vel: Vec3| {
            let mut v = Vehicle::new(s.clone(), Vec3::new(x, 4.0, 0.0), 0.0);
            v.b.vel = vel;
            v
        };
        let mut a = put(3.0 - half - 0.03, Vec3::ZERO);
        assert!(clear(&store, &a), "the kart by the wall should be clear of it to begin with");
        let mut b = put(3.0 - half - 0.03 - w + 0.2, Vec3::new(3.0, 0.0, 0.0));
        let (ax, bx) = (a.b.pos.x, b.b.pos.x);
        karts(&mut a, &mut b, Some(&store));
        assert!(clear(&store, &a), "pushed into the wall: x {} from {ax}", a.b.pos.x);
        assert!(b.b.pos.x < bx - 0.1, "b not pushed back: {} from {bx}", b.b.pos.x);
    }

    /// The footprint covers the wheels: two karts side by side touch when
    /// their tyres do, not 0.2 m later (or, with a `bump_box`, its sides:
    /// the kart is drawn bigger than its wheels).
    #[test]
    fn side_by_side_karts_touch_at_the_tyres() {
        let s = kart();
        let w = s.bump_box.map_or(s.track + s.tyre_width, |b| b[0]);
        let a = Footprint::of(&at(&s, 0.0, 0.0, 0.0, Vec3::ZERO));
        assert!(overlap(&a, &Footprint::of(&at(&s, w - 0.05, 0.0, 0.0, Vec3::ZERO))).is_some());
        assert!(overlap(&a, &Footprint::of(&at(&s, w + 0.05, 0.0, 0.0, Vec3::ZERO))).is_none());
    }

    /// A nose driven into the middle of another kart's side doesn't slip
    /// through: a footprint has no gaps along its side.
    #[test]
    fn a_nose_into_a_side_is_stopped() {
        let s = kart();
        // b faces north (-z) at the origin; a comes at its side from the
        // east, facing west, at 15 m/s.
        let mut a = at(&s, 1.6, 0.0, std::f32::consts::FRAC_PI_2, Vec3::new(-15.0, 0.0, 0.0));
        let mut b = at(&s, 0.0, 0.0, 0.0, Vec3::ZERO);
        let dt = 1.0 / 60.0;
        for _ in 0..30 {
            a.b.pos += a.b.vel * dt;
            b.b.pos += b.b.vel * dt;
            karts(&mut a, &mut b, None);
        }
        assert!(a.b.pos.x > b.b.pos.x, "went through: a {:?} b {:?}", a.b.pos, b.b.pos);
    }

    /// A rear-end passes speed forward: the front kart is sped up and the
    /// rear one slowed.
    #[test]
    fn a_rear_end_speeds_up_the_kart_in_front() {
        let s = kart();
        let mut front = at(&s, 0.0, 0.0, 0.0, Vec3::new(0.0, 0.0, -20.0));
        let len = Footprint::of(&front).half.y * 2.0;
        let mut back = at(&s, 0.0, len - 0.05, 0.0, Vec3::new(0.0, 0.0, -25.0));
        let hard = karts(&mut back, &mut front, None);
        assert!(hard > 4.0, "{hard}");
        assert!(-front.b.vel.z > 20.5, "front {}", -front.b.vel.z);
        assert!(-back.b.vel.z < 24.5, "back {}", -back.b.vel.z);
    }

    /// Two karts pressed together side by side at speed lose a little to
    /// the rubbing, neither gains, and neither is turned far.
    #[test]
    fn rubbing_side_by_side_costs_a_little_speed() {
        let s = kart();
        let w = s.track + s.tyre_width;
        let mut a = at(&s, 0.0, 0.0, 0.0, Vec3::new(0.0, 0.0, -25.0));
        let mut b = at(&s, w - 0.1, 0.0, 0.0, Vec3::new(0.0, 0.0, -25.0));
        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            // Each steers into the other a little.
            a.b.vel.x += 2.0 * dt;
            b.b.vel.x -= 2.0 * dt;
            a.b.pos += a.b.vel * dt;
            b.b.pos += b.b.vel * dt;
            karts(&mut a, &mut b, None);
            a.b.ang = Vec3::ZERO;
            b.b.ang = Vec3::ZERO;
        }
        for v in [&a, &b] {
            let fwd = -v.b.vel.z;
            assert!(fwd <= 25.01 && fwd > 23.0, "{fwd}");
        }
    }

    /// A corner tap turns a kart, and by no more than YAW_MAX.
    #[test]
    fn a_corner_tap_turns_but_does_not_spin() {
        let s = kart();
        let mut a = at(&s, 0.0, 0.0, 0.0, Vec3::new(0.0, 0.0, -20.0));
        let f = Footprint::of(&a);
        // b's nose clips a's rear right corner, b a little faster.
        let mut b = at(&s, f.half.x * 1.6, f.half.y * 2.0 - 0.1, 0.0, Vec3::new(0.0, 0.0, -26.0));
        karts(&mut a, &mut b, None);
        assert!(a.b.ang.y.abs() > 0.05, "not turned: {}", a.b.ang.y);
        assert!(a.b.ang.y.abs() <= YAW_MAX + 1e-4);
    }

    /// Head on, both are stopped and bounced back a little, and neither is
    /// lifted.
    #[test]
    fn head_on_stops_both_and_lifts_neither() {
        let s = kart();
        let mut a = at(&s, 0.0, 0.0, 0.0, Vec3::new(0.0, 0.0, -20.0));
        let len = Footprint::of(&a).half.y * 2.0;
        let mut b = at(&s, 0.0, -len + 0.05, std::f32::consts::PI, Vec3::new(0.0, 0.0, 20.0));
        karts(&mut a, &mut b, None);
        assert!(a.b.vel.z > 0.0 && a.b.vel.z < 20.0, "{}", a.b.vel.z);
        assert!(b.b.vel.z < 0.0, "{}", b.b.vel.z);
        assert_eq!(a.b.vel.y, 0.0);
        assert_eq!(b.b.vel.y, 0.0);
    }
}
