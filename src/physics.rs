//! Box-versus-voxel-grid movement, shared by the player and every creature.
//! Moves are split into substeps shorter than a voxel and resolved one axis at a
//! time; a body on the ground climbs ledges up to `step_up` metres without jumping.

use crate::world::store::Store;
use glam::Vec3;

/// How far the drawn ground at the point (x, z) under `pos` sits above (or
/// below, negative) `pos.y`, in metres, where sloped ground is drawn. Bodies
/// collide with voxels, so this is what to add to where they are drawn for the
/// feet to touch the slope. Zero when there is no ground within half a metre.
///
/// Nothing over a moving platform. The slope is read off voxels, and a plate
/// between voxels has a voxel copy up to a quarter metre below its deck, so
/// a rider would be drawn sunk into the floor they are standing on.
pub fn drawn_ground_lift(store: &Store, pos: Vec3) -> f32 {
    use crate::world::gen::VPM;
    let feet = Vec3::new(pos.x, pos.y - 0.35, pos.z);
    if !store.platforms.is_empty() && store.platforms.iter().any(|s| s.meets(feet - Vec3::new(0.05, 0.0, 0.05), feet + Vec3::new(0.05, 0.7, 0.05), 0.0)) {
        return 0.0;
    }
    use crate::world::material::AIR;
    let at = |x: isize, y: isize, z: isize| store.get(x as i32, y as i32, z as i32).unwrap_or(AIR);
    match crate::mesh::ground_height_h(at, |x, z| store.ground_vox(x as i32, z as i32), pos.x * VPM, pos.z * VPM, pos.y * VPM) {
        Some(h) => (h / VPM - pos.y).clamp(-0.3, 0.3),
        None => 0.0,
    }
}

const V: f32 = 4.0;
const EPS: f32 = 1e-4;
/// How far a blocked move may set a body back from where it started, to put
/// it flush against what it hit. A voxel face is never further than this;
/// a platform face can be, if the plate has moved into the body, and then
/// the body is left for `free_body` rather than flung onto the far side.
const BACK: f32 = 0.3;

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub vel: Vec3,
    pub half_w: f32,
    pub height: f32,
    pub on_ground: bool,
}

impl Body {
    pub fn new(half_w: f32, height: f32) -> Self {
        Self {
            vel: Vec3::ZERO,
            half_w,
            height,
            on_ground: false,
        }
    }
}

/// How far to shove an upright body at `a` so it stops standing inside the
/// one at `b`, as a level vector. Zero when they are already clear.
///
/// Boxes, like everything else that collides in here, and pushed apart on
/// the shallower of the two axes so a body squeezing past a shoulder slides
/// along it rather than being thrown sideways. The vertical overlap is a
/// gate, not a direction: you can stand on a head, and two bodies a storey
/// apart are not touching however close their shadows are.
///
/// It returns the whole correction. A caller moving both bodies applies
/// half to each; a caller moving one (an animal is not going to give way to
/// you) applies all of it.
pub fn separate(a: Vec3, a_hw: f32, a_h: f32, b: Vec3, b_hw: f32, b_h: f32) -> Vec3 {
    if a.y + a_h <= b.y || b.y + b_h <= a.y {
        return Vec3::ZERO;
    }
    let reach = a_hw + b_hw;
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    let (ox, oz) = (reach - dx.abs(), reach - dz.abs());
    if ox <= 0.0 || oz <= 0.0 {
        return Vec3::ZERO;
    }
    // Two bodies exactly on top of each other have no shallower axis, so
    // pick one rather than dividing by nothing.
    if ox <= oz {
        Vec3::new(if dx < 0.0 { -ox } else { ox }, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, if dz < 0.0 { -oz } else { oz })
    }
}

/// Move `p` by `by` unless that puts the body in a wall. A push out of
/// another body must never become a push into the world, which is the
/// commonest way a crowd of bodies posts one of its members through a
/// floor.
pub fn nudge(store: &Store, p: &mut Vec3, by: Vec3, half_w: f32, height: f32) {
    let want = *p + by;
    if !hits(store, want, half_w, height) {
        *p = want;
        return;
    }
    // One axis at a time: a corner that blocks both still lets a body slide
    // along the wall it is against.
    for axis in [Vec3::X, Vec3::Z] {
        let one = *p + by * axis;
        if one != *p && !hits(store, one, half_w, height) {
            *p = one;
            return;
        }
    }
}

/// The moving platforms a body box with feet at `p` is inside, measured
/// against where each plate really is rather than its voxels
/// (`store::Slab`).
fn plates_in<'a>(store: &'a Store, p: Vec3, half_w: f32, height: f32) -> impl Iterator<Item = &'a crate::world::store::Slab> + 'a {
    let lo = Vec3::new(p.x - half_w, p.y, p.z - half_w);
    let hi = Vec3::new(p.x + half_w, p.y + height, p.z + half_w);
    store.platforms.iter().filter(move |s| s.meets(lo, hi, EPS))
}

/// Does a box with feet at `p` overlap any voxel `solid` calls solid, or a
/// moving platform? Platforms are solid to every notion of solid there is:
/// a swimmer and a glider stand on a lift as a walker does.
fn overlaps(store: &Store, p: Vec3, half_w: f32, height: f32, solid: impl Fn(&Store, i32, i32, i32) -> bool) -> bool {
    if !store.platforms.is_empty() && plates_in(store, p, half_w, height).next().is_some() {
        return true;
    }
    let x0 = ((p.x - half_w) * V + EPS).floor() as i32;
    let x1 = ((p.x + half_w) * V - EPS).floor() as i32;
    let y0 = (p.y * V + EPS).floor() as i32;
    let y1 = ((p.y + height) * V - EPS).floor() as i32;
    let z0 = ((p.z - half_w) * V + EPS).floor() as i32;
    let z1 = ((p.z + half_w) * V - EPS).floor() as i32;
    // The feet in voxels: a slab in the bottom row is clear of a body
    // standing at or above its middle.
    let feet = p.y * V;
    for y in y0..=y1 {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if solid(store, x, y, z) {
                    if y == y0 && feet >= y as f32 + 0.5 - EPS && store.slab(x, y, z) {
                        continue;
                    }
                    return true;
                }
            }
        }
    }
    false
}

/// Does a box with feet at `p` overlap any solid voxel?
pub fn hits(store: &Store, p: Vec3, half_w: f32, height: f32) -> bool {
    overlaps(store, p, half_w, height, |s, x, y, z| s.solid(x, y, z))
}

/// `hits` for a body that can swim: water voxels are not in the way. An
/// imported map fills its water zones with solid voxels, so without this a
/// pool is a floor and a flooded tunnel is a block of rock.
pub fn hits_wet(store: &Store, p: Vec3, half_w: f32, height: f32) -> bool {
    overlaps(store, p, half_w, height, |s, x, y, z| !matches!(s.cell(x, y, z), crate::world::store::Cell::Water) && s.solid(x, y, z))
}

/// How far up a column of water voxels is searched for its surface, in
/// voxels: deeper than any pool an imported map holds.
const WATER_SCAN: i32 = 400;

/// The surface of the water `p` is inside, in metres, when `p` is inside
/// water voxels: the top of the column of them above it. None anywhere
/// else. This is what gives a voxel pool the same answer `game::water_over`
/// gives for the world's own water, which is a field and not voxels.
pub fn water_voxel_top(store: &Store, p: Vec3) -> Option<f32> {
    use crate::world::store::Cell;
    let (x, z) = ((p.x * V).floor() as i32, (p.z * V).floor() as i32);
    let y0 = (p.y * V).floor() as i32;
    if store.cell(x, y0, z) != Cell::Water {
        return None;
    }
    let mut y = y0;
    while y - y0 < WATER_SCAN && store.cell(x, y + 1, z) == Cell::Water {
        y += 1;
    }
    Some((y + 1) as f32 / V)
}

pub struct MoveResult {
    pub stepped: f32,
    pub blocked: bool,
}

/// Move `pos` (feet centre) by `delta`, sliding along walls. Updates `b.vel` and
/// `b.on_ground`.
pub fn move_body(store: &Store, pos: &mut Vec3, b: &mut Body, delta: Vec3, step_up: f32) -> MoveResult {
    move_body_with(store, pos, b, delta, step_up, hits)
}

/// `move_body` with its own notion of what is solid, so a mode can let some
/// materials through (the glider passes through leaves).
pub fn move_body_with(store: &Store, pos: &mut Vec3, b: &mut Body, delta: Vec3, step_up: f32, hits: impl Fn(&Store, Vec3, f32, f32) -> bool) -> MoveResult {
    let mut res = MoveResult { stepped: 0.0, blocked: false };
    let n = (delta.abs().max_element() / 0.2).ceil().max(1.0) as i32;
    let d = delta / n as f32;
    let (hw, h) = (b.half_w, b.height);
    let mut grounded = false;
    for _ in 0..n {
        // Vertical.
        if d.y != 0.0 {
            let mut p = *pos;
            p.y += d.y;
            if hits(store, p, hw, h) {
                // Where the body comes to rest: on the voxel face it hit, or
                // on the face of a platform it hit, which is wherever the
                // plate is and not on the grid. The nearest one that is
                // clear wins.
                let plates: Vec<f32> = plates_in(store, p, hw, h).map(|s| if d.y < 0.0 { s.hi_m.y } else { s.lo_m.y - h }).collect();
                let grid = if d.y < 0.0 { ((p.y * V).floor() + 1.0) / V } else { ((p.y + h) * V).floor() / V - EPS - h };
                // Falling, the middle of the voxel too, which is the top of
                // a slab (`Store::slab`); anything else rejects it below as
                // a stop that still hits.
                let half = (d.y < 0.0).then(|| ((p.y * V).floor() + 0.5) / V);
                let mut stops: Vec<f32> = std::iter::once(grid).chain(half).chain(plates).filter(|&y| if d.y < 0.0 { y >= p.y && y <= pos.y + BACK } else { y <= p.y && y >= pos.y - BACK }).collect();
                if d.y < 0.0 {
                    stops.sort_by(f32::total_cmp);
                } else {
                    stops.sort_by(|a, b| b.total_cmp(a));
                }
                if let Some(y) = stops.into_iter().find(|&y| !hits(store, Vec3::new(p.x, y, p.z), hw, h)) {
                    pos.y = y;
                }
                if d.y < 0.0 {
                    grounded = true;
                }
                b.vel.y = 0.0;
            } else {
                *pos = p;
            }
        }
        // Horizontal, one axis at a time.
        for axis in [0usize, 2] {
            let step = d[axis];
            if step == 0.0 {
                continue;
            }
            let mut p = *pos;
            p[axis] += step;
            if !hits(store, p, hw, h) {
                *pos = p;
                continue;
            }
            // Try climbing a ledge.
            let mut climbed = false;
            if (b.on_ground || grounded) && step_up > 0.0 {
                let max_k = (step_up * V * 2.0).round() as i32;
                // Every ledge in reach, by half voxels so the top of a slab
                // is one, and the deck of any platform in the way, which
                // sits wherever the plate has got to.
                let mut ups: Vec<f32> = (1..=max_k).map(|k| ((pos.y * V * 2.0 + EPS).floor() + k as f32) / (2.0 * V)).collect();
                ups.extend(plates_in(store, p, hw, h).map(|s| s.hi_m.y).filter(|&y| y > pos.y && y <= pos.y + step_up));
                ups.sort_by(f32::total_cmp);
                for up in ups {
                    let mut q = p;
                    q.y = up;
                    let mut head = *pos;
                    head.y = q.y;
                    if !hits(store, head, hw, h) && !hits(store, q, hw, h) {
                        res.stepped += q.y - pos.y;
                        *pos = q;
                        climbed = true;
                        break;
                    }
                }
            }
            if !climbed {
                // Slide up to the wall: the voxel face, or the side of a
                // platform, whichever is clear and furthest along.
                let grid = if step > 0.0 { ((p[axis] + hw) * V).floor() / V - hw - EPS } else { (((p[axis] - hw) * V).floor() + 1.0) / V + hw + EPS };
                let plates: Vec<f32> = plates_in(store, p, hw, h).map(|s| if step > 0.0 { s.lo_m[axis] - hw } else { s.hi_m[axis] + hw }).collect();
                let from = pos[axis];
                let mut stops: Vec<f32> = std::iter::once(grid).chain(plates).filter(|&x| if step > 0.0 { x >= from - BACK && x <= p[axis] } else { x <= from + BACK && x >= p[axis] }).collect();
                stops.sort_by(|a, b| if step > 0.0 { b.total_cmp(a) } else { a.total_cmp(b) });
                for x in stops {
                    let mut q = p;
                    q[axis] = x;
                    if !hits(store, q, hw, h) {
                        *pos = q;
                        break;
                    }
                }
                b.vel[axis] = 0.0;
                res.blocked = true;
            }
        }
    }
    // Ground probe: standing still on the ground still counts as grounded.
    if !grounded && b.vel.y <= 0.0 {
        let mut p = *pos;
        p.y -= 0.02;
        grounded = hits(store, p, hw, h);
    }
    b.on_ground = grounded;
    res
}

/// Step a body box out of anything solid it has ended up inside, which
/// otherwise stops it moving at all: every move the collision tries is
/// blocked, so the body is stuck where it stands. Quarter-metre shells out
/// from where it is, sideways before up at the same distance, so a body
/// wedged in a wall steps out of it rather than climbing it: pushing up
/// first is what left bots standing waist-deep in the deck over a lift
/// shaft, because up is where the shaft wall's own floor is. A body that
/// has sunk into the ground is unaffected -- every sideways try at its own
/// height is solid too, so it still takes the lift at the same shell. Searches sideways out to `max` metres and
/// then straight up as far as it takes. `hits` is the caller's notion of
/// solid, as for `move_body_with`. False when there is nowhere free.
pub fn free_body(store: &Store, pos: &mut Vec3, hw: f32, h: f32, max: f32, hits: impl Fn(&Store, Vec3, f32, f32) -> bool) -> bool {
    if !hits(store, *pos, hw, h) {
        return true;
    }
    const Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const OUT: [(f32, f32); 8] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (Q, Q), (Q, -Q), (-Q, Q), (-Q, -Q)];
    let shells = (max * V).round().max(1.0) as i32;
    for k in 1..=shells {
        let d = k as f32 / V;
        for (dx, dz) in OUT {
            let q = *pos + Vec3::new(dx * d, 0.0, dz * d);
            if !hits(store, q, hw, h) {
                *pos = q;
                return true;
            }
        }
        if !hits(store, *pos + Vec3::Y * d, hw, h) {
            pos.y += d;
            return true;
        }
        for (dx, dz) in OUT {
            let q = *pos + Vec3::new(dx * d, d * 0.5, dz * d);
            if !hits(store, q, hw, h) {
                *pos = q;
                return true;
            }
        }
    }
    // Buried deep: the only way out is up.
    for k in shells + 1..(240.0 * V) as i32 {
        let q = *pos + Vec3::Y * (k as f32 / V);
        if !hits(store, q, hw, h) {
            *pos = q;
            return true;
        }
    }
    false
}

/// Keep a walking body on the ground when it walks off a ledge no taller than
/// `max` metres: drop it straight onto the lower ground instead of letting it fall.
/// Returns the distance dropped (0 when there is no ground that close).
/// `hits` is the caller's notion of solid, as for `move_body_with`.
pub fn snap_down(store: &Store, pos: &mut Vec3, b: &mut Body, max: f32, hits: impl Fn(&Store, Vec3, f32, f32) -> bool) -> f32 {
    // By half voxels, so stepping off a slab comes down onto the floor
    // beside it and stepping off a ledge comes down onto a slab.
    let base = (pos.y * V * 2.0).round() / (2.0 * V);
    let max_k = (max * V * 2.0).round() as i32;
    for k in 1..=max_k {
        let mut q = *pos;
        q.y = base - k as f32 / (2.0 * V);
        if hits(store, q, b.half_w, b.height) {
            return 0.0;
        }
        let mut probe = q;
        probe.y -= 0.02;
        if hits(store, probe, b.half_w, b.height) {
            let dropped = pos.y - q.y;
            *pos = q;
            b.vel.y = 0.0;
            b.on_ground = true;
            return dropped;
        }
    }
    0.0
}

/// What a body does once nobody is driving it: a dead fighter, a shot
/// animal. It keeps the box it had and gravity and the walls do the rest --
/// it falls off the gantry it was shot on, it is thrown across the room by a
/// rocket, it hits the wall and comes off it, and it slides to a stop. The
/// pose is not this function's business: the limbs are a ragdoll
/// (`anim::ragdoll`), driven by the same velocity.
/// It falls under the world's gravity, as a living body does.

/// How much of the speed into a surface comes back out of it. A body is not
/// a ball: it hits, it gives, and most of the speed stays in the floor.
const BOUNCE: f32 = 0.3;
/// Below this much speed into a surface, a bounce is not worth having: the
/// body simply stops against it, metres a second.
const KEEP: f32 = 2.0;
/// How fast the ground takes the slide out of a body, a second.
const DRAG: f32 = 5.0;
/// What is left of the speed along the floor after a body has come down on
/// it. Without this a thrown body skips: nothing slows a bounce but the
/// ground friction, and a body that is bouncing is never on the ground long
/// enough to feel it, so it crosses the whole arena in hops. A man is not a
/// skipped stone -- he lands, and most of the way he was going stays where
/// he landed.
const THUMP: f32 = 0.55;
/// Under this it has stopped moving, metres a second.
const DEAD_STILL: f32 = 0.25;
/// Terminal speed, so a body dropped down a shaft stays simulable.
const FASTEST: f32 = 50.0;

/// One step of a body with nobody in it. True while it is still moving:
/// false is a body that has come to rest and can be left alone. The caller
/// keeps `prev` for drawing, since what it is drawn from differs.
pub fn tumble(store: &Store, pos: &mut Vec3, body: &mut Body, dt: f32) -> bool {
    if body.vel.length_squared() < DEAD_STILL * DEAD_STILL && body.on_ground {
        body.vel = Vec3::ZERO;
        return false;
    }
    body.vel += store.gravity.pull(*pos) * dt;
    body.vel.y = body.vel.y.max(-FASTEST);
    let was = body.vel;
    // No step up: a corpse does not climb the stair it is sliding into.
    move_body(store, pos, body, body.vel * dt, 0.0);
    // `move_body` zeroes the component on the axis it stopped on, so what it
    // zeroed is what the body hit.
    for a in 0..3 {
        if was[a] == 0.0 || body.vel[a] != 0.0 {
            continue;
        }
        if was[a].abs() > KEEP {
            body.vel[a] = -was[a] * BOUNCE;
        }
        // Come down on the floor: the landing takes the run out of it.
        if a == 1 && was.y < 0.0 {
            body.vel.x *= THUMP;
            body.vel.z *= THUMP;
        }
    }
    if body.on_ground {
        // Friction, on the level only: what holds a body on a slope is the
        // slope's own voxels, which it is already resting against.
        let keep = (1.0 - DRAG * dt).max(0.0);
        body.vel.x *= keep;
        body.vel.z *= keep;
    }
    // Thrown into a wall, or caught by the world streaming in around it.
    if hits(store, *pos, body.half_w, body.height) {
        free_body(store, pos, body.half_w, body.height, 1.5, hits);
        body.vel = Vec3::ZERO;
    }
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::material::STONE;
    use crate::world::store::Voxels;

    /// Any material the library flags as water, for a pool to be built out
    /// of. The engine goes by the flag and not by a name, and so does this.
    /// The lock comes back with the id and has to be held for as long as the
    /// id is used: there is one material table for the process, and another
    /// test installing another world turns this number into some other
    /// material (`matlib::table_lock`).
    pub(crate) fn water_material() -> (u8, std::sync::MutexGuard<'static, ()>) {
        let table = crate::world::matlib::table_lock();
        let (lib, _) = crate::library::Library::load(&crate::library::data_root());
        crate::world::matlib::install(&lib);
        let m = (17u8..=254)
            .find(|&m| crate::world::matlib::has(m, crate::world::matlib::flag::WATER))
            .expect("the library has a water material in it");
        (m, table)
    }

    /// Any material the library flags as a slab, with the table lock held
    /// for as long as the id is used (see `water_material`).
    pub(crate) fn slab_material() -> (u8, std::sync::MutexGuard<'static, ()>) {
        let table = crate::world::matlib::table_lock();
        let (mut lib, _) = crate::library::Library::load(&crate::library::data_root());
        // The library holds more materials than a world has ids for, so
        // reach the one a pad stands on rather than hoping it got one.
        lib.assign_ids(Some(["pickup_pad_health".to_string()].into_iter().collect()));
        crate::world::matlib::install(&lib);
        let m = (17u8..=254).find(|&m| crate::world::matlib::slab(m)).expect("the library has a slab material in it");
        (m, table)
    }

    fn floor_store() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                s.chunks.insert((cx, 0, cz), Voxels::Uniform(STONE));
                s.chunks.insert((cx, 1, cz), Voxels::Uniform(0));
                s.chunks.insert((cx, 2, cz), Voxels::Uniform(0));
            }
        }
        s
    }

    /// A body wedged in a wall steps out of it sideways rather than
    /// climbing it. Pushing up first is what put bots inside the deck over
    /// a lift shaft: the wall they were clipping had its own floor above,
    /// so "up" was a storey of solid rock and the shove went looking for
    /// the top of it.
    #[test]
    fn a_wedged_body_steps_out_rather_than_up() {
        let mut s = floor_store();
        // A wall from the floor up, east of x = 1: thick enough that there
        // is no way through it, and open air to the west.
        for z in -8..8 {
            for x in 4..40 {
                for y in 32..48 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        // Feet on the floor with the box half inside the wall.
        let mut pos = Vec3::new(1.1, 8.0, 0.0);
        assert!(hits(&s, pos, 0.3, 1.75), "the body did not start wedged");
        assert!(free_body(&s, &mut pos, 0.3, 1.75, 2.0, hits), "it could not get out");
        assert!(!hits(&s, pos, 0.3, 1.75));
        assert!(pos.x < 1.1, "it did not come out westward: {pos:?}");
        assert!((pos.y - 8.0).abs() < 1e-3, "it climbed the wall instead of stepping out: {pos:?}");
        // And a body that has sunk into the ground still goes up, because
        // every way out sideways is ground too.
        let mut sunk = Vec3::new(-4.0, 7.6, 0.0);
        assert!(hits(&s, sunk, 0.3, 1.75));
        assert!(free_body(&s, &mut sunk, 0.3, 1.75, 2.0, hits));
        assert!(sunk.y >= 8.0, "it did not step up out of the floor: {sunk:?}");
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let s = floor_store();
        let mut pos = Vec3::new(1.0, 12.0, 1.0);
        let mut b = Body::new(0.3, 1.75);
        for _ in 0..200 {
            b.vel.y -= 24.0 / 60.0;
            let d = b.vel / 60.0;
            move_body(&s, &mut pos, &mut b, d, 0.5);
        }
        assert!((pos.y - 8.0).abs() < 1e-3, "y = {}", pos.y);
        assert!(b.on_ground);
    }

    /// A pool of water voxels: a swimmer goes into it, everything else
    /// stands on it, and a point inside it knows the surface over it. This
    /// is the only water the voxels hold -- the world's own is a field
    /// (`game::water_over`) -- and it is how an imported map's water zone
    /// becomes water rather than a floor.
    #[test]
    fn water_voxels_let_a_swimmer_through_and_hold_everyone_else() {
        let (water, _table) = water_material();
        let mut s = floor_store();
        // A tank two metres deep sunk into the floor, filled to its brim:
        // voxels 32..39 are water, so the surface is at 10 m.
        for z in -4..4 {
            for x in 0..8 {
                for y in 32..40 {
                    s.set(x, y, z, water);
                }
            }
        }
        let feet = Vec3::new(1.0, 8.0, 0.0);
        assert!(s.solid(4, 34, 0), "water is still solid to a walker");
        assert!(hits(&s, feet, 0.3, 1.75), "a walker should stand on the pool");
        assert!(!hits_wet(&s, feet, 0.3, 1.75), "a swimmer should go into the pool");
        assert_eq!(water_voxel_top(&s, feet), Some(10.0));
        assert_eq!(water_voxel_top(&s, Vec3::new(1.0, 9.9, 0.0)), Some(10.0), "and from just under the surface");
        assert_eq!(water_voxel_top(&s, Vec3::new(1.0, 10.1, 0.0)), None, "nothing above the surface is in water");
        assert_eq!(water_voxel_top(&s, Vec3::new(-1.0, 8.0, 0.0)), None, "nor is the rock beside it");
    }

    /// A slab is stood on at its middle: a body walks up onto one without
    /// jumping, stands an eighth of a metre up, comes down onto it from a
    /// fall, and steps back off it onto the floor.
    #[test]
    fn a_slab_is_half_a_voxel_high() {
        let (slab, _table) = slab_material();
        let mut s = floor_store();
        for z in -8..8 {
            for x in 8..16 {
                s.set(x, 32, z, slab);
            }
        }
        assert!(s.slab(10, 32, 0) && s.solid(10, 32, 0));
        let mut pos = Vec3::new(1.0, 8.0, 0.0);
        let mut b = Body::new(0.3, 1.75);
        b.on_ground = true;
        for _ in 0..30 {
            b.vel.y -= 24.0 / 60.0;
            let d = Vec3::new(4.0 / 60.0, b.vel.y / 60.0, 0.0);
            move_body(&s, &mut pos, &mut b, d, 0.5);
        }
        assert!(pos.x > 2.5, "it did not get onto the slab: x = {}", pos.x);
        assert!((pos.y - 8.125).abs() < 1e-3, "it is not standing on the slab's top: y = {}", pos.y);
        assert!(b.on_ground);
        // Dropped onto it from above.
        let mut fall = Vec3::new(3.0, 10.0, 0.0);
        let mut b = Body::new(0.3, 1.75);
        for _ in 0..200 {
            b.vel.y -= 24.0 / 60.0;
            let d = b.vel / 60.0;
            move_body(&s, &mut fall, &mut b, d, 0.5);
        }
        assert!((fall.y - 8.125).abs() < 1e-3, "it fell to y = {}", fall.y);
        // Off the edge, the floor is an eighth of a metre down.
        let mut off = Vec3::new(1.5, 8.125, 0.0);
        let mut b = Body::new(0.3, 1.75);
        assert!((snap_down(&s, &mut off, &mut b, 0.5, hits) - 0.125).abs() < 1e-4, "y = {}", off.y);
        // And the ground under a point on it is its top.
        let (y, _) = crate::creatures::terrain::voxel_ground(&s, 3.0, 0.0, 12.0).unwrap();
        assert!((y - 8.125).abs() < 1e-4, "voxel_ground says {y}");
    }

    #[test]
    fn steps_up_a_two_voxel_ledge() {
        let mut s = floor_store();
        for z in -8..8 {
            for x in 8..16 {
                s.set(x, 32, z, STONE);
                s.set(x, 33, z, STONE);
            }
        }
        let mut pos = Vec3::new(1.0, 8.0, 0.0);
        let mut b = Body::new(0.3, 1.75);
        b.on_ground = true;
        // 30 steps at 4 m/s ends at x = 3, on top of the ledge (which spans x 2..4).
        for _ in 0..30 {
            b.vel.y -= 24.0 / 60.0;
            let d = Vec3::new(4.0 / 60.0, b.vel.y / 60.0, 0.0);
            move_body(&s, &mut pos, &mut b, d, 0.5);
        }
        assert!(pos.x > 2.5, "x = {}", pos.x);
        assert!((pos.y - 8.5).abs() < 1e-3, "y = {}", pos.y);
    }

    #[test]
    fn snaps_down_a_one_voxel_drop_but_not_a_cliff() {
        let mut s = floor_store();
        // Floor top at y = 8; a one-voxel shelf at x 0..2 (voxels 0..8).
        for z in -8..8 {
            for x in -8..8 {
                s.set(x, 32, z, STONE);
            }
        }
        let mut b = Body::new(0.3, 1.75);
        let mut pos = Vec3::new(2.5, 8.25, 0.0);
        assert!((snap_down(&s, &mut pos, &mut b, 0.5, hits) - 0.25).abs() < 1e-4);
        assert!((pos.y - 8.0).abs() < 1e-4 && b.on_ground);
        let mut high = Vec3::new(2.5, 9.0, 0.0);
        assert_eq!(snap_down(&s, &mut high, &mut b, 0.5, hits), 0.0);
        assert_eq!(high.y, 9.0);
    }

    /// Two bodies standing in each other are pushed apart on the shallower
    /// axis, and two a storey apart are not touching however close their
    /// shadows are.
    #[test]
    fn bodies_do_not_stand_inside_each_other() {
        let (hw, h) = (0.3, 1.75);
        // Nose to nose, a little more overlapped across x than across z.
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(0.45, 0.0, 0.1);
        let push = separate(a, hw, h, b, hw, h);
        assert!(push.x < 0.0 && push.z == 0.0, "pushed the wrong way: {push:?}");
        assert!((a + push).distance(b) >= hw * 2.0 - 1e-3, "still overlapping");
        // Clear of each other already.
        assert_eq!(separate(a, hw, h, Vec3::new(2.0, 0.0, 0.0), hw, h), Vec3::ZERO);
        // One standing on the other's head.
        assert_eq!(separate(a, hw, h, Vec3::new(0.1, 1.8, 0.0), hw, h), Vec3::ZERO);
        // Exactly on top of each other: it still picks a direction.
        assert_ne!(separate(a, hw, h, a, hw, h), Vec3::ZERO);
    }

    /// A shove out of another body never becomes a shove into the world.
    #[test]
    fn a_nudge_stops_at_a_wall() {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -30..30 {
            for z in -30..30 {
                for y in 0..8 {
                    s.set(x, y, z, crate::world::material::STONE);
                }
            }
        }
        // A wall at x = 2 m.
        for z in -30..30 {
            for y in 8..24 {
                s.set(8, y, z, crate::world::material::STONE);
            }
        }
        let (hw, h) = (0.3, 1.75);
        let mut p = Vec3::new(1.0, 2.0, 0.0);
        nudge(&s, &mut p, Vec3::new(3.0, 0.0, 0.0), hw, h);
        assert!(!hits(&s, p, hw, h), "it was pushed into the wall");
        // The same shove with a free way out along z takes the z.
        let mut q = Vec3::new(1.0, 2.0, 0.0);
        nudge(&s, &mut q, Vec3::new(3.0, 0.0, 1.0), hw, h);
        assert!(!hits(&s, q, hw, h));
        assert!(q.z > 0.5, "it refused the way that was open: {q:?}");
    }
}
