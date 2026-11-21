//! What the ear gets: the one file in `sound/` that knows about the world.
//!
//! Four things answer for a place's sound, and they are different in kind:
//!
//! - **The biome** says what the place sounds like with nothing happening in
//!   it: how much wind reaches the ground, how much grass there is to stir,
//!   what rain lands on. Sampled on rings round the listener, so a biome's
//!   edge crossfades over tens of metres instead of snapping at a line.
//! - **The weather** says what the day is doing to it (`Sky`): the wind, the
//!   rain, the hush of fog and snow.
//! - **The world's own data** says where the loud things are: rivers and
//!   lakes from the water fields, every waterfall, the sea. None of these is
//!   a point, so each is summed over a ring of samples into a direction and
//!   a spread: a river 50 m off comes out narrow, one you stand in, all round.
//! - **The voxels round the head** say what the wind has to rub against.
//!   Moving air is nearly silent; what is heard is what it gets past
//!   (Farnell, *Designing Sound*). So the leaves are the leaf voxels actually
//!   near the listener, and a roof, a cave or a canyon is found by rays.
//!
//! The survey runs ten times a second; the engine glides between surveys.

use super::{Emitter, Ground, Heard};
use crate::game::Game;
use crate::world::gen::VOXEL;
use crate::world::store::{Cell, Store};
use glam::{Vec2, Vec3};
use std::sync::Arc;

/// Seconds between surveys.
const EVERY: f32 = 0.1;

/// What one biome contributes to the bed, worked out once a world.
#[derive(Clone, Debug)]
struct Place {
    name: String,
    wind: f32,
    leaves: f32,
    grass: f32,
    ground: Ground,
}

/// What a survey found besides the numbers, for the F9 readout.
#[derive(Clone, Debug, Default)]
pub struct Why {
    pub biome: String,
    /// Share of the sky the listener can see, 0 to 1.
    pub open: f32,
    /// Share of the horizontal rays that met a wall, and how far off on
    /// average.
    pub walled: f32,
    pub wall_m: f32,
    /// Leaf voxels counted near the head, and above it.
    pub leaf_cells: u32,
    pub leaf_over: u32,
    /// How far the eye stands above the land round it, metres.
    pub rise: f32,
    /// How wet the leaves are, 0 to 1 (drips after rain).
    pub wet: f32,
}

/// The listener's state between surveys: the last answer, the biome table
/// for the world it is in, and how wet the leaves are, which outlasts the
/// rain that wet them.
#[derive(Default)]
pub struct Ear {
    since: f32,
    last: Heard,
    wet: f32,
    places: Vec<Place>,
    /// Which world `places` was made for.
    world: usize,
    pub why: Why,
}

impl Ear {
    /// The moment for `game`, `dt` seconds after the last call. Surveys at
    /// most every `EVERY` seconds and hands back the last survey between.
    pub fn hear(&mut self, game: &Game, dt: f32) -> Heard {
        self.since += dt;
        if self.since < EVERY && self.since >= 0.0 {
            return self.last;
        }
        let elapsed = self.since.max(EVERY);
        self.since = 0.0;
        self.last = self.survey(game, elapsed);
        self.last
    }

    /// Forget the last answer, so the next call surveys at once (a world was
    /// just opened, or the camera was put somewhere new).
    pub fn reset(&mut self) {
        self.since = -1.0;
    }

    fn places_for(&mut self, w: &Arc<crate::world::fields::World>) {
        let key = Arc::as_ptr(w) as usize;
        if self.world == key && !self.places.is_empty() {
            return;
        }
        self.world = key;
        self.places = w.lib.biomes.iter().map(|b| place(b, &w.lib)).collect();
    }

    fn survey(&mut self, game: &Game, dt: f32) -> Heard {
        let (eye, fwd) = game.listener();
        let _ = fwd;
        let sky = game.weather.sky();
        let store = &game.store;
        let world = game.generator().world.clone();
        let day = (sky.wind / 1.5).clamp(0.0, 1.0);

        // The biome, on rings round the listener.
        let mut bed = Place { name: String::new(), wind: 1.0, leaves: 1.0, grass: 0.0, ground: Ground::Soft };
        let mut ground_here = eye.y - 1.6;
        let mut rise = 0.0;
        if let Some(w) = &world {
            self.places_for(w);
            let f = &w.f;
            let at = |x: f32, z: f32| -> Option<&Place> {
                if x < 0.0 || z < 0.0 || x >= f.size || z >= f.size || f.height(x, z) < f.sea {
                    return None;
                }
                self.places.get(f.biome[f.idx(x, z)] as usize)
            };
            let (mut wind, mut leaves, mut grass, mut total) = (0.0, 0.0, 0.0, 0.0);
            let mut add = |p: Option<&Place>, k: f32| {
                // Over the sea there is no ground to rustle, but the wind
                // blows there as anywhere.
                let (pw, pl, pg) = p.map_or((1.1, 1.0, 0.0), |p| (p.wind, p.leaves, p.grass));
                wind += pw * k;
                leaves += pl * k;
                grass += pg * k;
                total += k;
            };
            add(at(eye.x, eye.z), 3.0);
            for (r, k) in [(12.0, 1.0), (30.0, 0.5)] {
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::TAU / 8.0;
                    add(at(eye.x + r * a.cos(), eye.z + r * a.sin()), k);
                }
            }
            bed.wind = wind / total;
            bed.leaves = leaves / total;
            bed.grass = grass / total;
            if let Some(p) = at(eye.x, eye.z) {
                bed.name = p.name.clone();
                bed.ground = p.ground;
            }
            ground_here = f.height(eye.x, eye.z);
            // How far the eye stands above the land round it: a ridge or a
            // summit, or the air high over everything.
            let mut sum = 0.0;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                sum += f.height(eye.x + 40.0 * a.cos(), eye.z + 40.0 * a.sin()).max(f.sea);
            }
            rise = eye.y - sum / 8.0;
        }

        let probe = Probe::take(store, eye);
        let open = probe.open;
        let walled = probe.walled;
        let (level, _) = game.water_at(eye);
        let under = if eye.y < level - 0.05 { 1.0 } else { 0.0 };
        let snow = (game.year.lying() / 0.3).clamp(0.0, 1.0);
        let above_ground = (eye.y - ground_here).max(0.0);

        // Wind: exposure to the sky, less inside walls, more the higher the
        // ear stands over the land.
        let exposure = ((rise / 25.0).max((above_ground - 3.0) / 60.0)).clamp(0.0, 1.0);
        let wind = (bed.wind * (0.3 + 0.7 * open) * (1.0 - 0.6 * walled) * (0.8 + 0.4 * exposure)).clamp(0.0, 1.2);
        let whistle = (open * exposure * (0.3 + 0.7 * day) * bed.wind.min(1.3)).clamp(0.0, 1.0);

        // Leaves: the crowns actually near the head, then the wood round it.
        let near = (probe.leaf_cells as f32 / 40.0).min(1.0);
        let far = (probe.leaf_far as f32 / 12.0).min(1.0);
        let leaves = ((near + 0.4 * far).min(1.0) * bed.leaves * (1.0 - 0.4 * sky.snow)).clamp(0.0, 1.5);
        // Grass about the feet, gone under snow and out of earshot from a
        // roof or the air.
        let feet = (1.0 - (above_ground - 1.8) / 3.0).clamp(0.0, 1.0);
        let grass = (bed.grass * feet * (1.0 - snow) * (1.0 - 0.7 * sky.snow) * (0.4 + 0.6 * open)).clamp(0.0, 1.0);

        // Rain, and the leaves that hold it.
        let over = (probe.leaf_over as f32 / 18.0).min(1.0);
        if sky.rain > 0.2 {
            self.wet = (self.wet + dt * sky.rain / 40.0).min(1.0);
        } else {
            self.wet = (self.wet - dt / 150.0).max(0.0);
        }
        let drip = self.wet.max(sky.rain * 0.6) * over;
        let ground = if under > 0.0 || game.water_at(eye - Vec3::Y * 1.6).0 > eye.y - 1.6 {
            Ground::Water
        } else if over > 0.4 {
            Ground::Leaves
        } else if open < 0.4 && walled > 0.5 {
            // Under a roof: rain on the roof.
            Ground::Stone
        } else {
            bed.ground
        };

        // The loud things.
        let (water, flow, surf) = world.as_ref().map_or((Emitter::default(), 0.0, Emitter::default()), |w| waters(&w.f, eye));
        let (falls, falls_size, spray) = world.as_ref().map_or((Emitter::default(), 0.0, 0.0), |w| fall(&w.f, eye));

        // The room: walls round the ear and sky over it.
        let room = (0.1 + 0.65 * walled * (1.0 - 0.5 * open) + 0.15 * near).clamp(0.0, 1.0);
        // In the open there is only a little air to ring, and it is short.
    let room_size = (0.15 * (1.0 - walled) + walled * ((probe.wall_m / 30.0) * 0.6 + 0.4 * (1.0 - open))).clamp(0.0, 1.0);
        let muffle = (0.45 * sky.snow + 0.5 * sky.mist.min(0.8) + 0.25 * snow + 0.15 * (1.0 - open) * walled).clamp(0.0, 1.0);

        self.why = Why {
            biome: bed.name.clone(),
            open,
            walled,
            wall_m: probe.wall_m,
            leaf_cells: probe.leaf_cells,
            leaf_over: probe.leaf_over,
            rise,
            wet: self.wet,
        };
        Heard {
            wind,
            gust: day,
            whistle,
            leaves,
            grass,
            rain: sky.rain,
            shelter: 1.0 - open,
            ground,
            drip,
            water,
            flow,
            falls,
            falls_size,
            spray,
            surf,
            under,
            muffle,
            room,
            room_size,
        }
    }

    /// The readout's lines (F9): every number and what set it.
    pub fn lines(&self, h: &Heard) -> Vec<String> {
        let w = &self.why;
        let em = |e: &Emitter| {
            if e.gain <= 0.001 {
                "-".to_string()
            } else {
                format!("{:.2} at {:03.0} spread {:.2}", e.gain, crate::util::bearing(Vec2::from(e.dir)), e.spread)
            }
        };
        vec![
            format!("BIOME {}", if w.biome.is_empty() { "-" } else { &w.biome }),
            format!("SKY OPEN {:.2}  WALLED {:.2} AT {:.0} M  RISE {:.0} M", w.open, w.walled, w.wall_m, w.rise),
            format!("WIND {:.2}  GUST {:.2}  WHISTLE {:.2}", h.wind, h.gust, h.whistle),
            format!("LEAVES {:.2} ({} CELLS, {} OVER)  GRASS {:.2}", h.leaves, w.leaf_cells, w.leaf_over, h.grass),
            format!("RAIN {:.2} ON {}  SHELTER {:.2}  DRIP {:.2}  WET {:.2}", h.rain, h.ground.name().to_uppercase(), h.shelter, h.drip, w.wet),
            format!("WATER {}  FLOW {:.2}", em(&h.water), h.flow),
            format!("FALLS {}  SIZE {:.2}  SPRAY {:.2}", em(&h.falls), h.falls_size, h.spray),
            format!("SURF {}", em(&h.surf)),
            format!("UNDER {:.0}  MUFFLE {:.2}  ROOM {:.2} SIZE {:.2}", h.under, h.muffle, h.room, h.room_size),
        ]
    }
}

/// A biome's part in the bed: its `sound` block where it has one, the rest
/// from its surface material.
fn place(b: &crate::library::BiomeDef, lib: &crate::library::Library) -> Place {
    let s = b.sound.clone().unwrap_or_default();
    let mat = lib.materials.iter().find(|m| m.name == b.surface);
    // Grass grows where the material says how, or (as the sprites have it)
    // where its name holds "grass" and it takes the defaults.
    let grows = mat.and_then(|m| match &m.grass {
        Some(g) => Some(((g.sparse + g.dense) * 0.5).clamp(0.0, 1.0)),
        None if m.name.to_lowercase().contains("grass") => Some(0.475),
        None => None,
    });
    let ground = s.ground.unwrap_or_else(|| ground_of(&b.surface, grows.is_some()));
    Place {
        name: b.name.clone(),
        wind: s.wind.unwrap_or(1.0).max(0.0),
        leaves: s.leaves.unwrap_or(1.0).max(0.0),
        grass: s.grass.or(grows).unwrap_or(0.0).clamp(0.0, 1.0),
        ground,
    }
}

/// What rain lands on, from the name of a surface material: grass where
/// grass grows, stone where the name says rock, soft everywhere else.
pub fn ground_of(surface: &str, grassy: bool) -> Ground {
    const STONE: [&str; 16] = ["rock", "stone", "basalt", "granite", "slab", "paving", "cobble", "brick", "concrete", "tile", "slate", "marble", "chalk", "limestone", "flint", "lava"];
    let s = surface.to_lowercase();
    if grassy {
        Ground::Grass
    } else if STONE.iter().any(|w| s.contains(w)) {
        Ground::Stone
    } else {
        Ground::Soft
    }
}

/// What the voxels round the head say.
#[derive(Debug, Default)]
struct Probe {
    /// Leaf voxels counted in a box round the head, those above it, and those
    /// in the wood a dozen metres off.
    leaf_cells: u32,
    leaf_over: u32,
    leaf_far: u32,
    /// Share of the sky seen up five rays, 0 to 1.
    open: f32,
    /// Share of eight level rays that met something solid within 30 m, and
    /// how far off they met it on average (30 where none did).
    walled: f32,
    wall_m: f32,
}

impl Probe {
    fn take(store: &Store, eye: Vec3) -> Probe {
        let cell = |p: Vec3| -> Cell {
            let v = (p / VOXEL).floor();
            store.cell(v.x as i32, v.y as i32, v.z as i32)
        };
        let mut p = Probe::default();
        // 150 cells in a 6 m box from just under the head to 6 m over it.
        for xi in -2..=2 {
            for zi in -2..=2 {
                for yi in -1..=4 {
                    let at = eye + Vec3::new(xi as f32 * 1.5, yi as f32 * 1.5, zi as f32 * 1.5);
                    if cell(at) == Cell::Foliage {
                        p.leaf_cells += 1;
                        if yi >= 1 {
                            p.leaf_over += 1;
                        }
                    }
                }
            }
        }
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            for y in [-1.0, 2.0, 5.0, 8.0] {
                if cell(eye + Vec3::new(12.0 * a.cos(), y, 12.0 * a.sin())) == Cell::Foliage {
                    p.leaf_far += 1;
                }
            }
        }
        // The sky: straight up and four rays leaning 35 degrees off it.
        // Leaves let some of it through; anything solid shuts it.
        let lean = 35f32.to_radians();
        let mut seen = 0.0;
        let ups = [Vec3::Y, Vec3::new(lean.sin(), lean.cos(), 0.0), Vec3::new(-lean.sin(), lean.cos(), 0.0), Vec3::new(0.0, lean.cos(), lean.sin()), Vec3::new(0.0, lean.cos(), -lean.sin())];
        for d in ups {
            let mut shut = 0.0f32;
            for s in 1..=32 {
                match cell(eye + d * s as f32) {
                    Cell::Solid | Cell::Water => {
                        shut = 1.0;
                        break;
                    }
                    Cell::Foliage => shut += 0.25,
                    _ => {}
                }
                if shut >= 1.0 {
                    break;
                }
            }
            seen += 1.0 - shut.min(1.0);
        }
        p.open = seen / ups.len() as f32;
        // Walls: eight level rays from the ear.
        let (mut hits, mut dist) = (0u32, 0.0f32);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            for s in 1..=30 {
                if cell(eye + d * s as f32) == Cell::Solid {
                    hits += 1;
                    dist += s as f32;
                    break;
                }
            }
        }
        p.walled = hits as f32 / 8.0;
        p.wall_m = if hits > 0 { dist / hits as f32 } else { 30.0 };
        p
    }
}

/// Running water and the sea, each summed over rings of samples into one
/// emitter: how loud, from which way, how widely spread.
fn waters(f: &crate::world::fields::Fields, eye: Vec3) -> (Emitter, f32, Emitter) {
    let here = Vec2::new(eye.x, eye.z);
    let mut river = Sum::default();
    let mut flow = 0.0;
    let mut sea = Sum::default();
    let inside = |p: Vec2| p.x >= 0.0 && p.y >= 0.0 && p.x < f.size && p.y < f.size;
    // Rivers and lakes: close rings, because a stream is heard from tens of
    // metres, not hundreds.
    let mut sample = |p: Vec2, r: f32| {
        if !inside(p) {
            return;
        }
        let depth = f.river_depth(p.x, p.y);
        if depth <= 0.05 {
            return;
        }
        let speed = f.current(p.x, p.y).length();
        // A lake laps; a river runs.
        let w = if speed < 0.05 { 0.02 } else { 0.15 + (speed / 2.0).min(1.0) } * depth.min(1.5).sqrt();
        let c = w / (1.0 + (r / 8.0).powi(2)) * (1.0 + r / 8.0);
        river.add(p - here, c);
        flow += c * (speed / 2.5).min(1.0);
    };
    sample(here, 0.0);
    for r in [3.0, 8.0, 16.0, 28.0, 45.0, 70.0] {
        for i in 0..12 {
            let a = (i as f32 + 0.5 * (r as i32 % 2) as f32) * std::f32::consts::TAU / 12.0;
            sample(here + r * Vec2::new(a.cos(), a.sin()), r);
        }
    }
    // The sea: heard a long way inland, loudest where it breaks in the
    // shallows rather than out in deep water.
    for r in [5.0, 15.0, 30.0, 60.0, 100.0, 160.0, 250.0] {
        for i in 0..16 {
            let a = i as f32 * std::f32::consts::TAU / 16.0;
            let p = here + r * Vec2::new(a.cos(), a.sin());
            // Off the map is open sea.
            let depth = if inside(p) { f.sea - f.height(p.x, p.y) } else { 10.0 };
            if depth <= 0.0 || (inside(p) && f.wet(p.x, p.y).is_some()) {
                continue;
            }
            // Deep water hardly counts: the sound is where it breaks.
            let w = if depth < 2.5 { 1.0 } else { 0.03 };
            sea.add(p - here, w / (1.0 + (r / 40.0).powi(2)) * (1.0 + r / 40.0));
        }
    }
    let flow = if river.w > 0.0 { flow / river.w } else { 0.0 };
    // A creek a few metres wide fills half of the near rings, which at 2.5
    // was already all but full loudness: as loud as a river. At 6 a creek
    // on its bank comes out near half, and it takes a broad river to fill it.
    (river.emitter(6.0), flow, sea.emitter(3.0))
}

/// A sum of samples, each a direction and a weight.
#[derive(Default)]
struct Sum {
    v: Vec2,
    w: f32,
}

impl Sum {
    fn add(&mut self, d: Vec2, w: f32) {
        self.v += d.normalize_or_zero() * w;
        self.w += w;
    }

    /// Loudness rises toward 1 as the total weight passes `k`.
    fn emitter(&self, k: f32) -> Emitter {
        if self.w <= 1e-4 {
            return Emitter::default();
        }
        let len = self.v.length();
        let dir = if len > 1e-4 { self.v / len } else { Vec2::ZERO };
        Emitter { gain: 1.0 - (-self.w / k).exp(), dir: dir.into(), spread: (1.0 - len / self.w).clamp(0.0, 1.0) }
    }
}

/// The loudest waterfall: how loud, how big a drop, and the spray if the
/// ear is close enough to its foot.
fn fall(f: &crate::world::fields::Fields, eye: Vec3) -> (Emitter, f32, f32) {
    let mut best = (Emitter::default(), 0.0, 0.0);
    for fl in &f.falls {
        let to = fl.splash - eye;
        let d = to.length();
        let drop = fl.lip.y - fl.foot;
        let size = (drop / 25.0).clamp(0.2, 1.5);
        let gain = ((0.5 + 0.5 * size) * (1.0 + fl.width / 10.0) / (1.0 + (d / (10.0 + fl.width)).powi(2))).min(1.0);
        if gain > best.0.gain {
            let dir = Vec2::new(to.x, to.z).normalize_or_zero();
            let spread = (fl.width / (d + 1.0)).clamp(0.0, 1.0) * 0.8;
            let spray = (1.0 - d / (15.0 + fl.width)).clamp(0.0, 1.0);
            best = (Emitter { gain, dir: dir.into(), spread }, size, spray);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_name_their_ground() {
        assert_eq!(ground_of("meadow_turf", true), Ground::Grass);
        assert_eq!(ground_of("navajo_sandstone", false), Ground::Stone);
        assert_eq!(ground_of("red_sand", false), Ground::Soft);
        assert_eq!(ground_of("Cobbles", false), Ground::Stone);
    }

    /// A source to one side is narrow and points at it; samples all round
    /// cancel into a wide one with no direction to speak of.
    #[test]
    fn a_sum_has_a_direction_and_a_spread() {
        let mut one = Sum::default();
        one.add(Vec2::new(10.0, 0.0), 1.0);
        one.add(Vec2::new(10.0, 1.0), 1.0);
        let e = one.emitter(2.5);
        assert!(e.dir[0] > 0.99 && e.spread < 0.01, "{e:?}");
        let mut round = Sum::default();
        for i in 0..12 {
            let a = i as f32 * std::f32::consts::TAU / 12.0;
            round.add(Vec2::new(a.cos(), a.sin()), 1.0);
        }
        let e = round.emitter(2.5);
        assert!(e.spread > 0.99, "{e:?}");
        assert!(e.gain > 0.98);
        assert_eq!(Sum::default().emitter(1.0), Emitter::default());
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    /// What the water adds up to along a world's rivers: for each river
    /// cell sampled, its depth and current, then the emitter and flow heard
    /// standing on it and a few metres off it. `LURELIA_WATER_PROBE=world`.
    #[test]
    #[ignore]
    fn probe_the_water() {
        let Ok(name) = std::env::var("LURELIA_WATER_PROBE") else { return };
        let w = crate::worldload::load(&name).unwrap().world;
        let f = &w.f;
        let mut n = 0;
        let mut z = 0.0;
        while z < f.size && n < 14 {
            let mut x = 0.0;
            while x < f.size && n < 14 {
                let d = f.river_depth(x, z);
                let s = f.current(x, z).length();
                if d > 0.3 && s > 0.05 {
                    print!("at {x:5.0},{z:5.0} depth {d:4.2} speed {s:4.2} |");
                    for off in [0.0, 6.0, 15.0, 30.0] {
                        let eye = Vec3::new(x + off, f.height(x, z) + 1.6, z);
                        let (e, flow, _) = waters(f, eye);
                        print!(" {off:2.0}m g {:.2} fl {:.2} |", e.gain, flow);
                    }
                    println!();
                    n += 1;
                    z += 120.0;
                    break;
                }
                x += 2.0;
            }
            z += 4.0;
        }
    }
}
