//! Particles: short-lived bits of matter thrown up by something happening.
//! Dust off a landing, a splash, chips off a wall that was shot, smoke from a
//! muzzle, leaves coming down in autumn.
//!
//! The pool lives on the CPU (positions, velocities, ages) and is packed into
//! a storage buffer once a frame, the way render/plates.rs does it, then drawn
//! in two instanced draws (render/particles.rs, shaders/particles.wgsl): solid
//! bits that write depth, and soft bits that blend.
//!
//! What a burst looks like is a library entry under `library/fx/`, so a
//! new effect is a JSON file. Code only says where and when: `burst` for a
//! one-off, `trickle` for a steady stream, `hooks.rs` for the events the game
//! already has, `ambient.rs` for what drifts down on its own.
//!
//! Nothing here touches the simulation. The pool has its own random stream and
//! only reads the store, so a match, a bench or a seeded run is the same with
//! or without it.

pub mod ambient;
pub mod hooks;

use crate::render::particles::GpuParticle;
use crate::world::material::AIR;
use crate::world::store::Store;
use glam::Vec3;
use serde::Deserialize;

/// Most particles alive at once. A burst that would go over the cap is cut
/// short; nothing already flying is dropped for it.
pub const MAX_PARTICLES: usize = 8192;

/// How a particle is drawn.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    /// A soft round puff facing the camera: dust, smoke, mist, breath.
    #[default]
    Puff,
    /// A small solid square, turning: grit, chips of stone or wood.
    Chip,
    /// A line along its own flight: sparks, droplets.
    Streak,
    /// A flat oval that tumbles as it falls: a leaf, a petal.
    Leaf,
}

impl Shape {
    /// Solid shapes write depth and need no sorting; soft ones blend.
    pub fn solid(self) -> bool {
        matches!(self, Shape::Chip | Shape::Leaf)
    }

    fn code(self) -> u32 {
        match self {
            Shape::Puff => 0,
            Shape::Chip => 1,
            Shape::Streak => 2,
            Shape::Leaf => 3,
        }
    }
}

fn one() -> u32 {
    1
}
fn one_f() -> f32 {
    1.0
}
fn count_one() -> [u32; 2] {
    [1, 1]
}
fn white() -> [u8; 3] {
    [255, 255, 255]
}
fn cone() -> f32 {
    30.0
}
fn gravity() -> f32 {
    9.8
}
fn near_fade() -> f32 {
    1.2
}

/// One kind of particle and how a burst of it is thrown: a library entry
/// under `library/fx/`.
#[derive(Deserialize, Debug, Clone)]
pub struct FxDef {
    pub name: String,
    #[serde(default = "one")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    #[serde(default)]
    #[allow(dead_code)] // for an author reading the library
    pub notes: String,
    #[serde(default)]
    pub shape: Shape,
    /// How many a burst throws, fewest and most.
    #[serde(default = "count_one")]
    pub count: [u32; 2],
    /// Speed out along the burst's direction, m/s, slowest and fastest.
    #[serde(default)]
    pub speed: [f32; 2],
    /// How far from that direction a particle may leave, degrees: 0 is a
    /// jet, 90 a hemisphere, 180 everywhere.
    #[serde(default = "cone")]
    pub cone_deg: f32,
    /// Seconds each lives, shortest and longest.
    pub life: [f32; 2],
    /// Radius in metres at birth and at death.
    pub size: [f32; 2],
    /// Each particle's size is scaled by up to this much either way.
    #[serde(default)]
    pub size_jitter: f32,
    /// Downward pull, m/s². Negative rises (smoke, breath).
    #[serde(default = "gravity")]
    pub gravity: f32,
    /// How fast the air takes the speed off it, per second.
    #[serde(default)]
    pub drag: f32,
    /// How much it is carried by the wind, 0 to 1.
    #[serde(default)]
    pub wind: f32,
    /// Lands on the ground and lies there till its life is out, instead of
    /// falling through it.
    #[serde(default)]
    pub settle: bool,
    /// A sideways swing as it falls, m/s: how a leaf comes down.
    #[serde(default)]
    pub flutter: f32,
    /// Its colour, sRGB. With `material` this multiplies the material's own
    /// colour instead, so white is the material as it is.
    #[serde(default = "white")]
    pub color: [u8; 3],
    /// Take the colour of the material the burst came off (the caller says
    /// which): grit from the ground it was kicked out of, chips from the wall.
    #[serde(default)]
    pub material: bool,
    /// Its opacity at birth, 0 to 1.
    #[serde(default = "one_f")]
    pub alpha: f32,
    /// Gives its own light, a multiplier on its colour: sparks. Unlit.
    #[serde(default)]
    pub glow: f32,
    /// Gone within this many metres of the camera, so a puff at your feet or
    /// at the muzzle doesn't fill the view.
    #[serde(default = "near_fade")]
    pub near_fade: f32,
}

/// A burst asked for by something that has no pool of its own to hand: an
/// animal breaking the water (creatures/swim.rs). The list lives on
/// `Entities`; `Game` drains it into the pool after each step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// The effect (`library/fx/`).
    pub name: &'static str,
    pub at: Vec3,
    /// Particles start anywhere in a level disc this wide round `at`, metres.
    pub radius: f32,
    pub dir: Vec3,
    pub amount: f32,
}

/// Cues kept waiting; a world nobody draws keeps only the latest.
const CUES_KEPT: usize = 256;

/// Add a cue to a waiting list, dropping the oldest past `CUES_KEPT`.
pub fn cue(list: &mut Vec<Cue>, c: Cue) {
    if list.len() >= CUES_KEPT {
        list.remove(0);
    }
    list.push(c);
}

/// One particle in flight.
#[derive(Clone, Copy, Debug)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    /// Its own scale on the def's size.
    scale: f32,
    spin: f32,
    spin_rate: f32,
    def: u16,
    /// The material whose colour it wears; 0 for its own.
    mat: u8,
    /// Lying on the ground.
    rest: bool,
    seed: u32,
}

/// The pool, the effects it knows, and its own random stream.
pub struct Fx {
    items: Vec<Particle>,
    defs: Vec<FxDef>,
    rng: u32,
    /// Fractional particles a trickle owes, per def, so a slow stream still
    /// comes out at its rate.
    owed: Vec<f32>,
    /// How far the frame is between two steps, for drawing.
    pub alpha: f32,
    /// The wind the pool is carried by, m/s, set before each step.
    pub wind: Vec3,
    /// The library the defs came from, so a new world's are picked up.
    lib_tag: usize,
    /// Kept between frames: the state `hooks` and `ambient` compare against.
    pub hooks: hooks::State,
    pub ambient: ambient::State,
    /// Off with the environment variable VOXEL_FX=0, for comparisons.
    pub on: bool,
}

impl Default for Fx {
    fn default() -> Self {
        Fx {
            items: Vec::new(),
            defs: Vec::new(),
            rng: 0x9e37_79b9,
            owed: Vec::new(),
            alpha: 0.0,
            wind: Vec3::ZERO,
            lib_tag: 0,
            hooks: Default::default(),
            ambient: Default::default(),
            on: std::env::var("VOXEL_FX").map(|v| v != "0").unwrap_or(true),
        }
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Two directions square to `d` and to each other.
fn basis(d: Vec3) -> (Vec3, Vec3) {
    let up = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = d.cross(up).normalize_or(Vec3::X);
    (u, d.cross(u))
}

impl Fx {
    /// Take the effects from a library. A library already taken is left
    /// alone; a different one (a new world) replaces the defs and empties
    /// the pool, since its particles named the old list.
    pub fn use_library(&mut self, lib: &crate::library::Library) {
        let tag = lib as *const _ as usize;
        if tag == self.lib_tag {
            return;
        }
        self.lib_tag = tag;
        self.defs = lib.fx.clone();
        self.owed = vec![0.0; self.defs.len()];
        self.items.clear();
        self.ambient = ambient::State::of(lib);
    }

    #[cfg(test)]
    pub fn with_defs(defs: Vec<FxDef>) -> Fx {
        let owed = vec![0.0; defs.len()];
        Fx { defs, owed, on: true, ..Default::default() }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.name == name)
    }

    /// 0 to 1.
    fn roll(&mut self) -> f32 {
        // xorshift32: its own stream, never the simulation's.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn between(&mut self, r: [f32; 2]) -> f32 {
        r[0] + (r[1] - r[0]) * self.roll()
    }

    /// A direction within `half` radians of `d`.
    fn spread(&mut self, d: Vec3, half: f32) -> Vec3 {
        let (u, w) = basis(d);
        // Uniform over the cap: cos of the angle uniform between cos(half) and 1.
        let c = 1.0 - self.roll() * (1.0 - half.cos());
        let s = (1.0 - c * c).max(0.0).sqrt();
        let a = self.roll() * std::f32::consts::TAU;
        (d * c + (u * a.cos() + w * a.sin()) * s).normalize_or(d)
    }

    /// Throw a burst of the effect `name` from `at`, out along `dir`.
    /// `mat` is the material it came off, for an effect that wears one (0
    /// for none); `amount` scales the count (a harder landing throws more).
    /// An effect the library doesn't have does nothing.
    pub fn burst(&mut self, name: &str, at: Vec3, dir: Vec3, mat: u8, amount: f32) {
        self.burst_in(name, at, 0.0, dir, mat, amount);
    }

    /// `burst`, with each particle starting anywhere in a level disc
    /// `radius` metres round `at`: a breaching whale's splash is metres wide.
    pub fn burst_in(&mut self, name: &str, at: Vec3, radius: f32, dir: Vec3, mat: u8, amount: f32) {
        if !self.on {
            return;
        }
        let Some(i) = self.find(name) else { return };
        let [lo, hi] = self.defs[i].count;
        let n = ((lo as f32 + (hi.saturating_sub(lo) + 1) as f32 * self.roll()).floor() * amount.max(0.0)).round() as u32;
        for _ in 0..n {
            let mut from = at;
            if radius > 0.0 {
                let (a, r) = (self.roll() * std::f32::consts::TAU, radius * self.roll().sqrt());
                from += Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            }
            self.spawn(i, from, dir, mat);
        }
    }

    /// Throw every waiting cue.
    pub fn take(&mut self, cues: &mut Vec<Cue>) {
        for c in cues.drain(..) {
            self.burst_in(c.name, c.at, c.radius, c.dir, 0, c.amount);
        }
    }

    /// A steady stream of `name` at `rate` a second, for one step of `dt`:
    /// call every step for as long as it should run.
    pub fn trickle(&mut self, name: &str, at: Vec3, dir: Vec3, mat: u8, rate: f32, dt: f32) {
        if !self.on {
            return;
        }
        let Some(i) = self.find(name) else { return };
        self.owed[i] += rate.max(0.0) * dt;
        while self.owed[i] >= 1.0 {
            self.owed[i] -= 1.0;
            self.spawn(i, at, dir, mat);
        }
    }

    /// One particle of def `i`.
    fn spawn(&mut self, i: usize, at: Vec3, dir: Vec3, mat: u8) {
        if self.items.len() >= MAX_PARTICLES {
            return;
        }
        let d = self.defs[i].clone();
        let dir = dir.normalize_or(Vec3::Y);
        let out = self.spread(dir, d.cone_deg.to_radians());
        let speed = self.between(d.speed);
        let life = self.between(d.life).max(0.05);
        let scale = 1.0 + d.size_jitter * (self.roll() * 2.0 - 1.0);
        let spin = self.roll() * std::f32::consts::TAU;
        let spin_rate = (self.roll() * 2.0 - 1.0) * 6.0;
        let seed = (self.roll() * 16_777_216.0) as u32;
        self.items.push(Particle {
            pos: at,
            vel: out * speed,
            age: 0.0,
            life,
            scale,
            spin,
            spin_rate,
            def: i as u16,
            mat: if d.material { mat } else { 0 },
            rest: false,
            seed,
        });
    }

    /// Move everything on by `dt`: gravity, drag, the wind, a leaf's swing,
    /// landing where the def settles. The dead go.
    pub fn step(&mut self, dt: f32, store: &Store) {
        let wind = self.wind;
        let defs = &self.defs;
        self.items.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.life {
                return false;
            }
            if p.rest {
                return true;
            }
            let d = &defs[p.def as usize];
            p.vel += store.gravity.drop(p.pos, d.gravity) * dt;
            // Drag pulls the particle toward moving with the air: in still
            // air it slows to a stop, in wind it drifts downwind.
            let air = wind * d.wind;
            let k = (d.drag * dt).min(1.0);
            p.vel += (air - p.vel) * Vec3::new(k, if d.gravity.abs() > 0.0 { k * 0.5 } else { k }, k);
            let mut v = p.vel;
            if d.flutter > 0.0 {
                // The swing of a falling leaf: side to side across its fall,
                // a different rhythm for each.
                let ph = p.age * (1.6 + (p.seed & 255) as f32 / 255.0) + (p.seed >> 8) as f32 * 0.001;
                v += Vec3::new(ph.sin(), 0.0, (ph * 0.7).cos()) * d.flutter;
            }
            p.spin += p.spin_rate * dt;
            let next = p.pos + v * dt;
            if d.settle && v.y < 0.0 {
                let vx = [(next.x * 4.0).floor() as i32, (next.y * 4.0).floor() as i32, (next.z * 4.0).floor() as i32];
                if let Some(m) = store.get(vx[0], vx[1], vx[2]) {
                    if m != AIR && !crate::world::matlib::has(m, crate::world::matlib::flag::WATER) && !crate::world::matlib::has(m, crate::world::matlib::flag::FOLIAGE) {
                        p.pos = Vec3::new(next.x, (vx[1] + 1) as f32 / 4.0 + 0.01, next.z);
                        p.vel = Vec3::ZERO;
                        p.rest = true;
                        return true;
                    }
                }
            }
            p.pos = next;
            true
        });
    }

    /// What the renderer draws: the solid particles first, then the soft ones
    /// far to near, so they blend in order. Returns how many are solid.
    pub fn pack(&self, eye: Vec3, step: f32, out: &mut Vec<GpuParticle>) -> u32 {
        out.clear();
        if !self.on {
            return 0;
        }
        let mut soft: Vec<(f32, GpuParticle)> = Vec::new();
        let ahead = self.alpha * step;
        for p in &self.items {
            let d = &self.defs[p.def as usize];
            let t = p.age / p.life;
            let pos = if p.rest { p.pos } else { p.pos + p.vel * ahead };
            let size = (d.size[0] + (d.size[1] - d.size[0]) * t) * p.scale;
            // Puffs and streaks thin out over their whole life; solid bits
            // stay solid and shrink away at the end instead.
            let (a, size) = match d.shape {
                Shape::Puff => (d.alpha * (1.0 - t) * (1.0 - t) * (t * 12.0).min(1.0), size),
                Shape::Streak => (d.alpha * (1.0 - t), size),
                Shape::Chip | Shape::Leaf => (d.alpha, size * (1.0 - ((t - 0.8) / 0.2).clamp(0.0, 1.0))),
            };
            let dist = pos.distance(eye);
            let a = a * ((dist - d.near_fade * 0.25) / (d.near_fade * 0.75).max(1e-3)).clamp(0.0, 1.0);
            if a <= 0.004 || size <= 0.001 {
                continue;
            }
            let glow = d.glow.max(0.0);
            let rgb = d.color.map(srgb_to_linear);
            let k = if glow > 0.0 { glow } else { 1.0 };
            let g = GpuParticle {
                pos: [pos.x, pos.y, pos.z, size],
                vel: [p.vel.x, p.vel.y, p.vel.z, p.spin],
                color: [rgb[0] * k, rgb[1] * k, rgb[2] * k, a],
                info: [d.shape.code(), p.mat as u32, p.seed, (glow > 0.0) as u32],
            };
            if d.shape.solid() {
                out.push(g);
            } else {
                soft.push((dist, g));
            }
        }
        let solid = out.len() as u32;
        soft.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.extend(soft.into_iter().map(|x| x.1));
        out.truncate(MAX_PARTICLES);
        solid.min(out.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, shape: Shape) -> FxDef {
        serde_json::from_value(serde_json::json!({
            "name": name, "shape": shape_name(shape), "count": [4, 4], "speed": [1.0, 1.0],
            "life": [1.0, 1.0], "size": [0.1, 0.1], "settle": true
        }))
        .unwrap()
    }

    /// Four empty chunks of air, 16 m across, 8 m up.
    fn open() -> Store {
        let mut s = Store::default();
        for cx in 0..2 {
            for cz in 0..2 {
                s.chunks.insert((cx, 0, cz), crate::world::store::Voxels::Uniform(AIR));
            }
        }
        s
    }

    fn shape_name(s: Shape) -> &'static str {
        match s {
            Shape::Puff => "puff",
            Shape::Chip => "chip",
            Shape::Streak => "streak",
            Shape::Leaf => "leaf",
        }
    }

    #[test]
    fn a_burst_lives_its_life_and_goes() {
        let store = open();
        let mut fx = Fx::with_defs(vec![def("grit", Shape::Chip)]);
        fx.burst("grit", Vec3::new(8.0, 8.0, 8.0), Vec3::Y, 0, 1.0);
        assert_eq!(fx.len(), 4);
        fx.burst("nothing by that name", Vec3::ZERO, Vec3::Y, 0, 1.0);
        assert_eq!(fx.len(), 4);
        for _ in 0..30 {
            fx.step(1.0 / 60.0, &store);
        }
        assert_eq!(fx.len(), 4, "gone before its life was out");
        for _ in 0..40 {
            fx.step(1.0 / 60.0, &store);
        }
        assert_eq!(fx.len(), 0, "outlived its life");
    }

    #[test]
    fn solid_bits_pack_before_soft_ones_and_soft_ones_far_to_near() {
        let store = open();
        let mut fx = Fx::with_defs(vec![def("grit", Shape::Chip), def("dust", Shape::Puff)]);
        fx.burst("dust", Vec3::new(10.0, 8.0, 8.0), Vec3::Y, 0, 1.0);
        fx.burst("grit", Vec3::new(10.0, 8.0, 8.0), Vec3::Y, 0, 1.0);
        fx.burst("dust", Vec3::new(30.0, 8.0, 8.0), Vec3::Y, 0, 1.0);
        fx.step(0.1, &store);
        let mut out = Vec::new();
        let solid = fx.pack(Vec3::new(0.0, 8.0, 8.0), 1.0 / 60.0, &mut out);
        assert_eq!(solid, 4);
        assert_eq!(out.len(), 12);
        assert!(out[..4].iter().all(|g| g.info[0] == 1));
        let far: Vec<f32> = out[4..].iter().map(|g| g.pos[0]).collect();
        assert!(far.windows(2).all(|w| w[0] >= w[1]), "soft particles not far to near: {far:?}");
    }

    #[test]
    fn a_settling_bit_comes_to_rest_on_the_ground() {
        let mut store = open();
        for x in 0..64 {
            for z in 0..64 {
                assert!(store.set(x, 3, z, crate::world::material::STONE));
            }
        }
        let mut fx = Fx::with_defs(vec![def("grit", Shape::Chip)]);
        fx.burst("grit", Vec3::new(8.0, 2.0, 8.0), Vec3::Y, 0, 1.0);
        for _ in 0..50 {
            fx.step(1.0 / 60.0, &store);
        }
        assert!(fx.items.iter().all(|p| p.rest && (p.pos.y - 1.01).abs() < 1e-3), "{:?}", fx.items.iter().map(|p| p.pos).collect::<Vec<_>>());
    }
}
