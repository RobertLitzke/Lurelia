//! What an event works on: the world fields, the lists it adds to, the state
//! earlier events left, and helpers to resolve names and place structures.

use crate::library::Library;
use crate::util::FxHashMap as HashMap;
use crate::world::fields::*;
use crate::world::noise::{hash32, smoothstep};
use crate::world::shape::{self, Shape};
use glam::Vec2;
use std::any::{Any, TypeId};
use std::path::Path;

/// One event kind's behaviour. Its params struct (read from chronicle.json)
/// implements this; `apply` changes the world and returns the gazetteer line.
pub trait Process: std::fmt::Debug {
    fn apply(&self, cx: &mut EventCtx) -> String;
}

/// State an event leaves for later ones (rivers by name, a settlement's
/// buildings, lava tubes), one value per type. Each event file defines its own
/// type, so a new kind of state needs no new field here.
#[derive(Default)]
pub struct State(HashMap<TypeId, Box<dyn Any>>);

impl State {
    /// The value of type `T`, made with `Default` the first time.
    pub fn get<T: Default + 'static>(&mut self) -> &mut T {
        self.0.entry(TypeId::of::<T>()).or_insert_with(|| Box::new(T::default())).downcast_mut().expect("state type")
    }
    /// Take the value of type `T` out, leaving a default.
    pub fn take<T: Default + 'static>(&mut self) -> T {
        std::mem::take(self.get::<T>())
    }
}

pub struct EventCtx<'a> {
    pub lib: &'a Library,
    /// The world folder, for events that read a file from it.
    pub dir: &'a Path,
    places: &'a HashMap<String, [f32; 2]>,
    /// Places events found on the land they shaped (`seat`, `glaciate`), by
    /// name: where, and the bearing it faces if it faces one. Looked up
    /// before `places`.
    pub seats: HashMap<String, (Vec2, Option<f32>)>,
    pub f: Fields,
    pub shapes: Vec<Shape>,
    pub placed: Vec<Placed>,
    pub items: Vec<Item>,
    pub pois: Vec<Poi>,
    pub areas: Vec<crate::world::places::Area>,
    pub signs: Vec<crate::world::places::Sign>,
    /// Point lights the world places (event `lamp`, render/lamps.rs).
    pub lamps: Vec<crate::world::fields::WorldLamp>,
    /// What breathes particles all the time (event `spring`).
    pub vents: Vec<crate::world::fields::WorldVent>,
    /// Placed structures that stand only some of the time.
    pub when: Vec<(u32, crate::world::fields::When)>,
    pub warnings: Vec<String>,
    pub state: State,
    shape_cache: HashMap<(usize, Vec<(String, String)>), usize>,
    /// The running event's year.
    pub year: i32,
    /// The running event's number (1 for the oldest), stamped into `f.cause`.
    pub cause: u16,
    /// The running event's seed.
    pub seed: u32,
    /// World.json `noise_at`: added to points before position noise is read.
    pub noise_at: Vec2,
    /// The world's race track, if it has one (world.json `race.track`), for
    /// placing things by lap distance (`Pos::Along`).
    pub track: Option<super::race_track::TrackLine>,
}

impl<'a> EventCtx<'a> {
    pub fn new(lib: &'a Library, places: &'a HashMap<String, [f32; 2]>, dir: &'a Path, f: Fields, warnings: Vec<String>) -> Self {
        EventCtx {
            lib,
            dir,
            places,
            seats: HashMap::default(),
            f,
            shapes: Vec::new(),
            placed: Vec::new(),
            items: Vec::new(),
            pois: Vec::new(),
            areas: Vec::new(),
            signs: Vec::new(),
            lamps: Vec::new(),
            vents: Vec::new(),
            when: Vec::new(),
            warnings,
            state: State::default(),
            shape_cache: HashMap::default(),
            year: 0,
            cause: 0,
            seed: 0,
            noise_at: Vec2::ZERO,
            track: None,
        }
    }

    /// A position: coordinates, a name (a place an event found, else one
    /// from the chronicle's `places`), an offset from a name, or a place
    /// beside the race track.
    pub fn pos(&mut self, p: &Pos) -> Vec2 {
        match p {
            Pos::At(a) => Vec2::new(a[0], a[1]),
            Pos::Named(n) => self.named(n),
            Pos::Along { along, left, right } => match &self.track {
                Some(t) => t.beside(*along, right - left),
                None => {
                    self.warnings.push(format!("along {along}: the world has no race track (world.json race.track)"));
                    Vec2::ZERO
                }
            },
            Pos::Rel { from, by, .. } => {
                let base = self.named(from);
                let by = Vec2::new(by[0], by[1]);
                // Quarter turns clockwise, seen from above (x east, y south).
                base + match self.turn(p) {
                    1 => Vec2::new(-by.y, by.x),
                    2 => -by,
                    3 => Vec2::new(by.y, -by.x),
                    _ => by,
                }
            }
        }
    }

    fn named(&mut self, n: &str) -> Vec2 {
        if let Some((p, _)) = self.seats.get(n) {
            return *p;
        }
        match self.places.get(n) {
            Some(a) => Vec2::new(a[0], a[1]),
            None => {
                self.warnings.push(format!("unknown place {n}"));
                Vec2::ZERO
            }
        }
    }

    /// How many quarter turns clockwise a position's surroundings are turned
    /// from the way they were drawn (`Pos::Rel` with `as_if`): the offset,
    /// and anything laid out round it, turns by this.
    pub fn turn(&self, p: &Pos) -> u8 {
        let Pos::Rel { from, as_if: Some(drawn), .. } = p else {
            return 0;
        };
        match self.seats.get(from) {
            Some((_, Some(faces))) => (((faces - drawn).rem_euclid(360.0) + 45.0) / 90.0) as u8 % 4,
            _ => 0,
        }
    }

    /// Turn `[east, south]` metres round a position by its quarter turns.
    pub fn turned(&self, p: &Pos, d: [f32; 2]) -> [f32; 2] {
        match self.turn(p) {
            1 => [-d[1], d[0]],
            2 => [-d[0], -d[1]],
            3 => [d[1], -d[0]],
            _ => d,
        }
    }

    /// Name a place an event found, for later events to build on.
    pub fn seat(&mut self, name: &str, at: Vec2, faces: Option<f32>) {
        self.seats.insert(name.to_string(), (at, faces));
    }

    /// A material's id by name (0, with a warning, if the library lacks it).
    pub fn mat(&mut self, name: &str) -> u8 {
        self.lib.material(name).unwrap_or_else(|| {
            self.warnings.push(format!("unknown material {name}"));
            0
        })
    }

    /// Compile a structure with slot overrides (cached), level a pad under it,
    /// and add it to the placed list. Returns its index.
    pub fn place(&mut self, def: usize, p: Vec2, bearing: f32, slots: &HashMap<String, String>, age: f32, seed: u32) -> Option<usize> {
        self.place_on(def, p, bearing, slots, age, seed, None)
    }

    /// `place`, levelling the pad of a levelled structure at `ground` m
    /// (absolute) instead of the mean ground under it: a gate tower whose
    /// floor must meet a bridge deck.
    #[allow(clippy::too_many_arguments)]
    pub fn place_on(&mut self, def: usize, p: Vec2, bearing: f32, slots: &HashMap<String, String>, age: f32, seed: u32, ground: Option<f32>) -> Option<usize> {
        let (lib, cause) = (self.lib, self.cause);
        let sd = &lib.structures[def];
        let mut over: Vec<(String, String)> = slots.iter().filter(|(k, _)| sd.slots.contains_key(*k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        over.sort();
        let key = (def, over.clone());
        let sh = match self.shape_cache.get(&key) {
            Some(&s) => s,
            None => {
                let s = compile_structure(lib, def, &over);
                self.shapes.push(s);
                self.shape_cache.insert(key, self.shapes.len() - 1);
                self.shapes.len() - 1
            }
        };
        let f = &mut self.f;
        // Front (local -Z) faces the bearing, quantised to quarter turns.
        let rot = (((bearing.rem_euclid(360.0) + 45.0) / 90.0) as u8) % 4;
        let size = self.shapes[sh].size;
        let (w, d) = shape::rotated_size(size, rot);
        let (ax, az) = shape::rotate(sd.anchor[0], sd.anchor[2], size, rot);
        let vx = (p.x * 4.0).floor() as i32 - ax;
        let vz = (p.y * 4.0).floor() as i32 - az;
        // Ground: the mean over the footprint, levelled with a feathered margin.
        let (x0, z0) = (vx as f32 * 0.25, vz as f32 * 0.25);
        let (x1, z1) = (x0 + w as f32 * 0.25, z0 + d as f32 * 0.25);
        let mut sum = (0.0, 0);
        for z in (z0 as i32)..=(z1 as i32) {
            for x in (x0 as i32)..=(x1 as i32) {
                sum.0 += f.height(x as f32 + 0.5, z as f32 + 0.5);
                sum.1 += 1;
            }
        }
        let mut g = ground.unwrap_or(sum.0 / sum.1.max(1) as f32);
        let mut lift = 0;
        if sd.base.as_deref() == Some("sea") {
            g = f.sea - 1.0;
        } else if sd.base.as_deref() == Some("bed") {
            // A wreck on the sea floor (docs/UNDERWATER.md): down on the
            // lowest bed under it and half a metre into the sand, so it lies
            // sunk rather than resting on its keel; nothing grows through it.
            let mut lo = f32::MAX;
            for z in (z0 as i32)..=(z1 as i32) {
                for x in (x0 as i32)..=(x1 as i32) {
                    lo = lo.min(f.height(x as f32 + 0.5, z as f32 + 0.5));
                }
            }
            g = (lo - 0.5).max(0.25);
            for z in (z0 as i32)..=(z1 as i32) {
                for x in (x0 as i32)..=(x1 as i32) {
                    if x >= 0 && z >= 0 && (x as usize) < f.n && (z as usize) < f.n {
                        f.occupied[z as usize * f.n + x as usize] = true;
                    }
                }
            }
        } else if sd.base.as_deref() == Some("rim") {
            // Spans a gap: the anchor voxel's height sits on the highest ground
            // under the footprint, and nothing is levelled.
            let mut hi = f32::MIN;
            for z in (z0 as i32)..=(z1 as i32) {
                for x in (x0 as i32)..=(x1 as i32) {
                    hi = hi.max(f.height(x as f32 + 0.5, z as f32 + 0.5));
                }
            }
            g = hi;
            lift = sd.anchor[1];
        } else if !sd.level {
            // Natural features settle into the ground instead.
            let mut lo = f32::MAX;
            for z in (z0 as i32)..=(z1 as i32) {
                for x in (x0 as i32)..=(x1 as i32) {
                    lo = lo.min(f.height(x as f32 + 0.5, z as f32 + 0.5));
                }
            }
            g = lo - 0.25;
            // Nothing grows through it.
            for z in (z0 as i32)..=(z1 as i32) {
                for x in (x0 as i32)..=(x1 as i32) {
                    if x >= 0 && z >= 0 && (x as usize) < f.n && (z as usize) < f.n {
                        f.occupied[z as usize * f.n + x as usize] = true;
                    }
                }
            }
        } else {
            let margin = 3.0;
            let n = f.n as i32;
            for z in ((z0 - margin) as i32).max(0)..=((z1 + margin) as i32).min(n - 1) {
                for x in ((x0 - margin) as i32).max(0)..=((x1 + margin) as i32).min(n - 1) {
                    let (cx, cz) = (x as f32 + 0.5, z as f32 + 0.5);
                    let dx = (x0 - cx).max(cx - x1).max(0.0);
                    let dz = (z0 - cz).max(cz - z1).max(0.0);
                    let out = (dx * dx + dz * dz).sqrt();
                    let t = smoothstep(margin, 0.0, out);
                    let i = z as usize * f.n + x as usize;
                    f.h[i] += (g - f.h[i]) * t;
                    // The pad and a metre around it are taken: nothing grows or
                    // carves there later.
                    if out <= 1.2 {
                        f.occupied[i] = true;
                    }
                    if out <= 0.0 {
                        f.cause[i] = cause;
                        if f.surface[i] != 0 && f.water[i] == 0 {
                            f.surface[i] = 0;
                        }
                    }
                }
            }
        }
        // A levelled building whose anchor sits above its bottom (a mine
        // shaft under a headframe) reaches that far down into the ground.
        if sd.level && sd.base.is_none() {
            lift = sd.anchor[1];
        }
        let gy = (g * 4.0).round() as i32 - lift;
        self.placed.push(Placed {
            shape: sh,
            def,
            origin: [vx, gy, vz],
            rot,
            age,
            cause,
            seed: hash32(seed ^ 0x51ed_270b),
        });
        Some(self.placed.len() - 1)
    }
}

pub fn compile_structure(lib: &Library, def: usize, over: &[(String, String)]) -> Shape {
    let sd = &lib.structures[def];
    let resolve = |n: &str| -> Option<u8> {
        if n == "carve" {
            return Some(shape::CARVE);
        }
        let name = over.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str()).or_else(|| sd.slots.get(n).map(|s| s.as_str())).unwrap_or(n);
        lib.material(name)
    };
    shape::compile(&sd.ops, sd.size, &resolve)
}
