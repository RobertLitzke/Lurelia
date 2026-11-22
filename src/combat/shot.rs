//! What a round does on its way out: the cone a pellet goes into, the ray
//! that finds the first thing in its way, the shell that flies and bounces,
//! and the damage either leaves behind.

use super::Scene;
use crate::anim::{ragdoll::Ragdoll, Anim};
use crate::creatures::age::Age;
use crate::creatures::walk::Critter;
use crate::creatures::{Creature, Look};
use crate::library::{FireDef, SplashDef};
use crate::physics::Body;
use crate::world::material::AIR;
use crate::world::store::Store;
use glam::Vec3;

/// A line left in the air after a ray: it fades over `of` seconds.
pub struct Tracer {
    pub a: Vec3,
    pub b: Vec3,
    pub left: f32,
    pub of: f32,
    pub tint: [f32; 3],
    /// How wide the ribbon is drawn, metres: a bullet leaves a thread and a
    /// beam a rope.
    pub width: f32,
}

/// A burst's ring, opening and dimming.
pub struct Burst {
    pub at: Vec3,
    pub radius: f32,
    pub left: f32,
    /// Fire or goo: the colour of the light it puts on the room.
    pub look: crate::library::SplashLook,
}

/// A body in flight: a rocket, a grenade, a scrap shell.
pub struct Shell {
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    pub gravity: f32,
    pub bounces: u32,
    /// Of its speed, what it keeps through a bounce (`FireDef::bounce_keep`).
    pub keep: f32,
    /// The share of its damage it does to whoever fired it
    /// (`FireDef::self_hit`), and whether it has bounced yet: until it has
    /// come off something it cannot be on its way back at them.
    pub self_hit: f32,
    pub bounced: bool,
    /// Seconds left before it goes off wherever it is.
    pub fuse: f32,
    /// Metres flown, so a stray shell is dropped rather than tracked forever.
    pub flown: f32,
    pub damage: f32,
    pub splash: Option<SplashDef>,
    pub tint: [f32; 3],
    /// Which little voxel body it is drawn as: an index into the weapon
    /// models (`viewmodel::SHELLS`). A shell is a model in the world, not a
    /// line on the overlay, so you can watch a rocket go and see a scrap
    /// shard spin past your ear.
    pub look: usize,
    /// Radians it has turned about its own long axis, for a grenade that
    /// tumbles rather than flying nose-first.
    pub spin: f32,
    /// Seconds a shell clings to what it lands on rather than going off
    /// there (`stick_s`). 0 is a shell that bursts on contact.
    pub stick_s: f32,
    /// True once it has landed and is waiting: it does not move, and it goes
    /// off when somebody comes near or when its fuse runs out.
    pub stuck: bool,
    /// Metres of sludge it lays down where it lands instead of hanging
    /// there as a model (`FireDef::goo_r`). 0 is every other shell.
    pub goo_r: f32,
    /// Metres of the map's spoil it lays down as a block where it lands
    /// (`FireDef::spoil_r`). 0 is every other shell.
    pub spoil: f32,
    /// What it breaks into where it lands, if anything: the little ones are
    /// thrown by `Combat::step`, which is the only place that can make more
    /// shells. The look is already an index into the shell models, so
    /// nothing here holds a string.
    pub cluster: Option<Cluster>,
    /// Which fighter fired it (`super::Scene::fighters`).
    pub by: usize,
    /// Radians a second it turns toward its owner's crosshair
    /// (`FireDef::guide`).
    pub guide: f32,
}

/// A `library::ClusterDef` with its look resolved, small enough to ride
/// along in every shell.
#[derive(Clone, Copy, Debug)]
pub struct Cluster {
    pub count: u32,
    pub damage: f32,
    pub speed: f32,
    pub gravity: f32,
    pub cone_deg: f32,
    pub bounces: u32,
    pub fuse_s: f32,
    pub splash: Option<SplashDef>,
    pub look: usize,
}

impl Shell {
    /// A shell stuck on a wall: it does not move. It goes off when somebody
    /// comes within reach of it, or when it has sat there long enough.
    fn wait(&mut self, scene: &Scene, dt: f32) -> Flight {
        self.fuse -= dt;
        self.spin += dt * 1.2;
        if self.fuse <= 0.0 {
            return Flight::Hit { at: self.pos, beast: None, man: None };
        }
        for man in scene.fighters.iter().filter(|m| m.alive) {
            let flat = Vec3::new(man.foot.x - self.pos.x, 0.0, man.foot.z - self.pos.z).length();
            let within = self.pos.y > man.foot.y - 0.4 && self.pos.y < man.foot.y + man.height + 0.4;
            if flat < STUCK_TOUCH + man.half_w && within {
                // Set off by proximity rather than struck: the burst is all
                // of it, so nobody takes the shell's own damage as well.
                return Flight::Hit { at: self.pos, beast: None, man: None };
            }
        }
        Flight::Flying
    }
}

pub enum Flight {
    Flying,
    /// It went off here, on `beast` if it struck an animal and on `man` if
    /// it struck a fighter. Either of those takes the shell's own `damage`
    /// on top of whatever its burst does, which is the whole of what a
    /// shell with no burst at all does to anybody.
    Hit {
        at: Vec3,
        beast: Option<hecs::Entity>,
        man: Option<usize>,
    },
    /// Out of range and forgotten, with no burst.
    Spent,
}

/// How much health a creature of this height has. Nothing in the library
/// gives animals health, and a shooter does not want a table of its own for
/// them: a squirrel goes down to a round or two, a bear takes a magazine.
fn health(height: f32) -> f32 {
    8.0 + 26.0 * height
}

/// Damage taken so far, put on a creature the first time it is hit.
pub struct Hurt {
    pub left: f32,
}

/// Turn `dir` by up to `half` radians, picking evenly inside the cone.
pub fn spread(dir: Vec3, half: f32, u1: f32, u2: f32) -> Vec3 {
    if half <= 0.0 {
        return dir;
    }
    let right = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(dir).normalize_or(Vec3::Y);
    // sqrt keeps the pellets even across the disc instead of bunched at the
    // middle, which is what makes a cone of scrap read as a wall of scrap.
    let r = half * u1.max(0.0).sqrt();
    let th = u2 * std::f32::consts::TAU;
    (dir + (right * th.cos() + up * th.sin()) * r.tan()).normalize_or(dir)
}

/// A ray's damage at `t` metres: full at the muzzle, `falloff` of itself at
/// the end of its range.
pub fn falloff(f: &FireDef, t: f32) -> f32 {
    let k = (t / f.range.max(0.01)).clamp(0.0, 1.0);
    f.damage * (1.0 - k * (1.0 - f.falloff))
}

/// Is there open air all the way from `a` to `b`? Splash needs it, so a
/// rocket on the far side of a wall is only a noise.
pub fn sees(store: &Store, a: Vec3, b: Vec3) -> bool {
    let v = b - a;
    let d = v.length();
    if d < 0.05 {
        return true;
    }
    store.ray(a, v / d, d - 0.04, |m| m != AIR).is_none()
}

/// `nearest_fighter` for something `pad` metres in radius rather than a
/// point: a body it passes within `pad` of is hit. A bullet is a point; a
/// half-metre block thrown past somebody's shoulder hits the shoulder.
pub fn nearest_fighter_wide(fighters: &[super::Hitbox], from: Vec3, dir: Vec3, max: f32, by: usize, pad: f32) -> Option<(f32, usize)> {
    where_fighter_wide(fighters, from, dir, max, by, pad).map(|(t, i, _)| (t, i))
}

/// The share of a body's height above which a hit counts as a head shot.
const HEAD_FROM: f32 = 0.82;

/// The same, saying as well whether the ray passed through the top of the
/// body. A head shot is read off the geometry rather than from a second
/// hitbox: the capsule already knows how high up itself the ray went by.
pub fn where_fighter(fighters: &[super::Hitbox], from: Vec3, dir: Vec3, max: f32, by: usize) -> Option<(f32, usize, bool)> {
    where_fighter_wide(fighters, from, dir, max, by, 0.0)
}

/// `where_fighter` for something `pad` metres in radius.
fn where_fighter_wide(fighters: &[super::Hitbox], from: Vec3, dir: Vec3, max: f32, by: usize, pad: f32) -> Option<(f32, usize, bool)> {
    let mut best: Option<(f32, usize, bool)> = None;
    for (i, man) in fighters.iter().enumerate() {
        if i == by || !man.alive {
            continue;
        }
        let (lo, hi) = (man.height * 0.08 - pad, man.height * 0.96 + pad);
        let (t, miss, at) = ray_to_upright(from, dir, man.foot, lo, hi);
        if !(0.0..max).contains(&t) || miss > man.half_w + pad {
            continue;
        }
        let head = at > man.height * HEAD_FROM;
        if best.is_none_or(|(bt, _, _)| t < bt) {
            best = Some((t, i, head));
        }
    }
    best
}

/// Distance along the ray to the nearest animal within `max`, and which.
pub fn nearest_creature(scene: &Scene, from: Vec3, dir: Vec3, max: f32) -> Option<(f32, hecs::Entity)> {
    let mut best: Option<(f32, hecs::Entity)> = None;
    for (e, look, me, crit, age) in scene.ents.world.query::<(hecs::Entity, &Look, Option<&Creature>, Option<&Critter>, Option<&Age>)>().iter() {
        if me.is_none() && crit.is_none() {
            continue;
        }
        if age.is_some_and(|a| a.dead()) {
            continue;
        }
        let (h, hw) = scene.models.get(look.model).map_or((1.0, 0.3), |m| (m.height, m.half_w));
        let (lo, hi) = (h * 0.1, h * 0.95);
        let (t, miss, _) = ray_to_upright(from, dir, look.pos, lo, hi);
        if !(0.0..max).contains(&t) || miss > hw.max(0.18) {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, e));
        }
    }
    best
}

/// Where a ray passes closest to the upright capsule from `foot + lo` to
/// `foot + hi`: the distance along the ray, how far it misses by, and how
/// high up the body it went past, metres above the feet.
/// (The same measure `creatures::census` uses to name what is in the sights.)
fn ray_to_upright(origin: Vec3, dir: Vec3, foot: Vec3, lo: f32, hi: f32) -> (f32, f32, f32) {
    let w = foot - origin;
    let wd = w.dot(dir);
    let a = 1.0 - dir.y * dir.y;
    let b = w.y - wd * dir.y;
    let s = if a > 1e-4 { (-b / a).clamp(lo, hi) } else { (lo + hi) * 0.5 };
    let v = w + Vec3::Y * s;
    let t = v.dot(dir);
    (t, (v - dir * t).length(), s)
}

/// Take `dmg` off a creature; true if that killed it. `fling` is what the
/// blow would do to a body with nothing holding it up, metres a second,
/// which only matters if this is the blow that kills it. Anything that is
/// not a creature (a find lying on the ground) is left alone.
pub fn hurt(scene: &mut Scene, e: hecs::Entity, dmg: f32, fling: Vec3) -> bool {
    let world = &mut scene.ents.world;
    let Ok(model) = world.get::<&Look>(e).map(|l| l.model) else {
        return false;
    };
    if world.get::<&Creature>(e).is_err() && world.get::<&Critter>(e).is_err() {
        return false;
    }
    if world.get::<&Age>(e).is_ok_and(|a| a.dead()) {
        return false;
    }
    let full = health(scene.models.get(model).map_or(1.0, |m| m.height));
    let left = match world.get::<&Hurt>(e) {
        Ok(h) => h.left - dmg,
        Err(_) => full - dmg,
    };
    if let Ok(mut l) = world.get::<&mut Look>(e) {
        l.flash = 1.0;
    }
    if left > 0.0 {
        let _ = world.insert_one(e, Hurt { left });
        return false;
    }
    kill(scene, e, fling);
    true
}

/// How much lighter on its feet an animal is than a man, by height: the same
/// blast that moves a deer sends a rabbit flying. A guess at mass from the
/// one number every model has, which is all this needs to be.
fn carries(height: f32) -> f32 {
    (1.7 / height.max(0.15)).clamp(0.4, 4.0)
}

/// It is a body now: no mover, no collision box, limp on the ground. The same
/// recipe `creatures::carry` uses for prey, without a carrier. A blow with
/// anything behind it also throws the carcass (`creatures::flung`).
fn kill(scene: &mut Scene, e: hecs::Entity, fling: Vec3) {
    let now = scene.ents.clock;
    let height = scene.models.get(scene.ents.world.get::<&Look>(e).map(|l| l.model).unwrap_or(usize::MAX)).map_or(1.0, |m| m.height);
    let world = &mut scene.ents.world;
    let model = world.get::<&Look>(e).map(|l| l.model).unwrap_or(usize::MAX);
    let rig = scene.models.get(model).and_then(|m| m.rig.as_ref());
    let _ = world.remove_one::<Critter>(e);
    let box_of = world.remove_one::<Body>(e).ok();
    let _ = world.remove_one::<crate::creatures::herd::Kin>(e);
    let _ = world.remove_one::<Hurt>(e);
    let thrown = fling * carries(height);
    match rig {
        Some(rig) => {
            let mut limp = Ragdoll::new(rig, scene.models[model].voxel, e.id() ^ 0x5add);
            let speed = thrown.length();
            if speed > crate::creatures::flung::FLUNG {
                limp.hurl(rig, speed, e.id() ^ 0x5add);
            }
            let mut anim = Anim::hold(limp.pose.clone());
            anim.ragdoll = Some(Box::new(limp));
            let _ = world.insert(e, (Age::fresh_kill(now), anim));
        }
        // Nothing rigged to go limp (an insect, a bird drawn as one piece):
        // it simply stops and fades with the rest of the carcasses. It still
        // falls: a bird shot out of the air used to hang there.
        None => {
            let _ = world.remove_one::<Anim>(e);
            let _ = world.insert_one(e, Age::fresh_kill(now));
        }
    }
    // Falling is not only for the thrown: a body killed in the air or on a
    // ledge has to come down whatever killed it.
    let mut b = box_of.unwrap_or_else(|| Body::new((height * 0.3).clamp(0.15, 0.6), height.max(0.2)));
    b.vel = thrown;
    b.on_ground = false;
    let _ = world.insert_one(e, crate::creatures::flung::Flung { body: b, quiet: 0.0 });
}

/// How long a step the flight predictor walks. Coarse on purpose: it is
/// asking whether a wall sends a disc round a corner, not reproducing the
/// round to the centimetre, and it is run on a handful of headings at once.
const PREDICT_STEP: f32 = 1.0 / 20.0;

/// Fly a shell of `f` out of `from` along `dir` through the world alone --
/// no bodies, no bursts -- and report how near it passes `at`, in metres.
///
/// This is what lets a bot shoot round a corner. A weapon file that says
/// `bounces` is a weapon whose rounds come off walls, and the only way to
/// use that on purpose is to ask where a round would end up before firing
/// it. Nothing here knows which weapon it is holding: it reads `speed`,
/// `gravity`, `bounces` and `fuse_s` out of the mode and walks.
///
/// `at` and `going` are where the target is and how fast: the walk carries
/// the target forward on its own clock, so the answer already has the lead
/// in it. A disc that arrives half a second later arrives where they will
/// be, not where they were when the trigger went down.
///
/// `f32::MAX` if it never comes near at all.
pub fn bank(store: &Store, from: Vec3, dir: Vec3, f: &FireDef, at: Vec3, going: Vec3) -> f32 {
    fly(store, from, dir, f, at, going, 0)
}

/// How near a round of `f` fired from `from` along `dir` comes back past
/// `at` once it has come off a wall: the same walk as `bank`, counting only
/// the flight after the first bounce. What a bot asks before throwing a disc
/// it would walk into; the single-gun league found every skipjack death
/// nobody was credited with was the thrower's own disc.
pub fn comes_back(store: &Store, from: Vec3, dir: Vec3, f: &FireDef, at: Vec3, going: Vec3) -> f32 {
    fly(store, from, dir, f, at, going, 1)
}

/// `bank` and `comes_back`: nearest pass at `at`, from the `after`th bounce on.
fn fly(store: &Store, from: Vec3, dir: Vec3, f: &FireDef, at: Vec3, going: Vec3, after: u32) -> f32 {
    let mut came_off = 0u32;
    let mut pos = from;
    let mut vel = dir.normalize_or(Vec3::NEG_Z) * f.speed.max(0.01);
    let mut bounces = f.bounces;
    let mut left = if f.fuse_s > 0.0 { f.fuse_s } else { 3.0 };
    let mut flown = 0.0f32;
    let mut best = f32::MAX;
    let mut gone = 0.0f32;
    while left > 0.0 && flown < super::SHELL_RANGE {
        let dt = PREDICT_STEP.min(left);
        left -= dt;
        gone += dt;
        // Where they will be when the round gets this far.
        let at = at + going * gone.min(1.2);
        vel += store.gravity.drop(pos, f.gravity) * dt;
        let delta = vel * dt;
        let d = delta.length();
        if d < 1e-4 {
            break;
        }
        let dir = delta / d;
        match store.ray(pos, dir, d, |m| m != AIR) {
            None => {
                if came_off >= after {
                    best = best.min(point_to_segment(at, pos, pos + delta));
                }
                pos += delta;
                flown += d;
            }
            Some(h) => {
                let to_face = (h.t - 0.06).max(0.0);
                let face = pos + dir * to_face;
                if came_off >= after {
                    best = best.min(point_to_segment(at, pos, face));
                }
                came_off += 1;
                pos = face;
                flown += to_face;
                if bounces == 0 {
                    break;
                }
                bounces -= 1;
                vel = (vel - 2.0 * vel.dot(h.normal) * h.normal) * f.bounce_keep;
            }
        }
    }
    best
}

/// How near `p` comes to the segment from `a` to `b`.
fn point_to_segment(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-8 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// How near a body has to come to a shell stuck on the wall before it goes
/// off, metres.
const STUCK_TOUCH: f32 = 1.1;

impl Shell {
    /// Move it for one step, stopping at the first thing in the way. A
    /// bounce costs it most of its speed; the last bounce goes off instead.
    pub fn step(&mut self, scene: &Scene, dt: f32) -> Flight {
        if self.stuck {
            return self.wait(scene, dt);
        }
        self.fuse -= dt;
        self.spin += dt * 9.0;
        self.vel += scene.store.gravity.drop(self.pos, self.gravity) * dt;
        let mut left = dt;
        // At most a few bounces inside one step, so a grenade in a corner
        // cannot spin here forever.
        for _ in 0..4 {
            let delta = self.vel * left;
            let d = delta.length();
            if d < 1e-5 {
                break;
            }
            let dir = delta / d;
            let wall = scene.store.ray(self.pos, dir, d, |m| m != AIR);
            let mut reach = wall.as_ref().map_or(d, |h| h.t);
            // A body in the way goes off on the body, not behind it.
            // Its owner is in the way too, once it has bounced and if it
            // hurts them at all.
            let skip = if self.self_hit > 0.0 && self.bounced { usize::MAX } else { self.by };
            // A block of spoil is as wide as it is drawn.
            if let Some((t, who)) = nearest_fighter_wide(scene.fighters, self.pos, dir, reach, skip, self.spoil * 0.5) {
                self.prev = self.pos;
                self.pos += dir * t;
                return Flight::Hit {
                    at: self.pos,
                    beast: None,
                    man: Some(who),
                };
            }
            reach = reach.min(d);
            if let Some((t, beast)) = nearest_creature(scene, self.pos, dir, reach) {
                self.prev = self.pos;
                self.pos += dir * t;
                return Flight::Hit {
                    at: self.pos,
                    beast: Some(beast),
                    man: None,
                };
            }
            match wall {
                None => {
                    self.prev = self.pos;
                    self.pos += delta;
                    self.flown += d;
                    break;
                }
                Some(h) => {
                    // Stop a finger short of the face, so the burst is in the
                    // air and can see out of the corner it went off in.
                    let to_face = (h.t - 0.06).max(0.0);
                    self.prev = self.pos;
                    self.pos += dir * to_face;
                    self.flown += to_face;
                    if self.bounces == 0 {
                        // Sludge that lays voxels does not hang on the
                        // face: it lands, and the rules turn it into matter
                        // (`combat::sludge`). One without a `goo_r` clings
                        // to the face it struck and waits there.
                        if self.stick_s > 0.0 && self.goo_r <= 0.0 {
                            self.stuck = true;
                            self.vel = Vec3::ZERO;
                            self.gravity = 0.0;
                            self.fuse = self.stick_s;
                            return Flight::Flying;
                        }
                        return Flight::Hit { at: self.pos, beast: None, man: None };
                    }
                    self.bounces -= 1;
                    self.bounced = true;
                    crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Bounce, self.pos, (self.vel.length() / 40.0).clamp(0.2, 0.9)));
                    let n = h.normal;
                    self.vel = (self.vel - 2.0 * self.vel.dot(n) * n) * self.keep;
                    left -= to_face / self.vel.length().max(0.01);
                    if left <= 0.0 {
                        break;
                    }
                }
            }
        }
        if self.fuse <= 0.0 {
            return Flight::Hit { at: self.pos, beast: None, man: None };
        }
        if self.flown > super::SHELL_RANGE {
            return Flight::Spent;
        }
        Flight::Flying
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pellet of a wide cone stays inside it, and they are not all on
    /// the same line.
    #[test]
    fn a_cone_stays_a_cone() {
        let dir = Vec3::new(0.3, -0.2, -1.0).normalize();
        let half = 8f32.to_radians();
        let mut worst: f32 = 0.0;
        let mut spread_seen: f32 = 0.0;
        let mut rng = 12345u32;
        for _ in 0..200 {
            rng = crate::world::noise::hash32(rng);
            let a = crate::world::noise::unit(rng);
            rng = crate::world::noise::hash32(rng);
            let b = crate::world::noise::unit(rng);
            let p = spread(dir, half, a, b);
            assert!((p.length() - 1.0).abs() < 1e-4);
            let ang = p.dot(dir).clamp(-1.0, 1.0).acos();
            worst = worst.max(ang);
            spread_seen = spread_seen.max(ang);
        }
        assert!(worst <= half + 1e-3, "a pellet went {} deg out", worst.to_degrees());
        assert!(spread_seen > half * 0.8, "every pellet came out near the middle");
    }

    /// Damage falls off along the ray and stops at the floor the file sets.
    #[test]
    fn range_takes_the_sting_out() {
        let f = FireDef {
            bore_r: 0.0,
            kind: crate::library::FireKind::Hitscan,
            rpm: 60.0,
            damage: 10.0,
            pellets: 1,
            spread_deg: 0.0,
            range: 100.0,
            falloff: 0.25,
            spin_up: 0.0,
            speed: 0.0,
            gravity: 0.0,
            bounces: 0,
            bounce_keep: 0.45,
            self_hit: 0.0,
            fuse_s: 0.0,
            splash: None,
            per_shot: 1,
            kick: 0.0,
            charge_s: 0.0,
            charge_mult: 1.0,
            stick_s: 0.0,
            goo_r: 0.0,
            spoil_r: 0.0,
            burst: 0,
            burst_gap_s: 0.0,
            burst_spread_deg: 0.0,
            pierce_m: 0.0,
            guide: 0.0,
            ring: false,
            twist: 0.0,
            arc_jumps: 0,
            arc_m: 0.0,
            arc_keep: 0.5,
            bend_m: 0.0,
            cluster: None,
            head_mult: 1.0,
            tracer: [1.0; 3],
            tracer_s: 0.05,
            look: String::new(),
        };
        assert!((falloff(&f, 0.0) - 10.0).abs() < 1e-4);
        assert!((falloff(&f, 50.0) - 6.25).abs() < 1e-4);
        assert!((falloff(&f, 100.0) - 2.5).abs() < 1e-4);
        assert!((falloff(&f, 400.0) - 2.5).abs() < 1e-4);
    }
}
