//! Plant site fields (notes/PLANT_SIM_PLAN.md, P1): what each patch of
//! ground offers a plant, measured from the finished land before any plant
//! is placed. Biomes say which plants may grow somewhere; these say where
//! inside a biome each one would do well.
//!
//! One byte a cell per channel, on the world survey's 4 m grid
//! (`survey::side`, `survey::cell_index`), so a cell is the same ground in
//! both. 0 to 255 reads as 0 to 1 everywhere.
//!
//! - `wet`: water in the ground. The soft-or of three local terms (any one
//!   can make a cell wet), set off by the climate (`Fields::moist`):
//!   the water table (a cell standing little above the drainage its water
//!   runs to: open water, the sea or a channel the flow finds), convergence
//!   (the flow a cell collects from upslope against its own slope; pits
//!   under `FILL_M` are filled, deeper basins left as they are so a pan
//!   collects as a sink), and depth below the
//!   mean ground within 20 m, for divots and washes too small for the flow
//!   on a 4 m grid. Each term is kept as its own channel for the map.
//! - `soil`: acid and poor (0) to base-rich (1), from the biome (`soil` on
//!   its entry, 0.5 if left out), graded linearly over `BORDER_M` either side
//!   of a biome border. Each cell keeps its nearest other biome and the
//!   distance to the border.
//! - `sun`: the ground's warmth: slope and aspect against the sun's path at
//!   the equinox, 0.5 on flat ground.
//! - `exposure`: height over the ground round about, and facing into
//!   `world::WIND`.
//! - `cold`: height above the sea on one scale for every world (0.15 at sea
//!   level, 1 at `ALPINE_M`), shifted by latitude, colder on north faces and
//!   where cold air pools in hollows, milder by the sea.
//! - `rock`: steepness, cliff and bare stone at the surface, less where
//!   deposits (sand, ash, silt) lie deep over the rock.
//! - `disturbed`: recent burns, bare volcanic ground (cinders, ash, lava
//!   crust), paths and roads, built structures.
//! - `shade`: the land's own shade: the share of the day the sun is hidden
//!   (behind the land, or the ground turned from it), and how much sky is
//!   cut off. Crown shade from plants comes with placement (P3).
//!
//! Built once with the world, in under a second on a 1536 m world, split by
//! rows across the build's threads. `--site-report` prints each field's
//! spread by biome.

use super::fields::{build_threads, World, CELL};
use super::survey::{self, CELL_M};
use glam::{Vec2, Vec3};

/// Metres either side of a biome border over which soil is graded.
pub const BORDER_M: f32 = 15.0;
/// 1 m cells per site cell along a side.
const SUB: usize = (CELL_M / CELL) as usize;

/// The water table term reaches zero this far above the nearest drainage.
const TABLE_M: f32 = 5.0;
/// Cells (16 m^2 each) of upslope flow at which a valley floor counts as a
/// channel the water table stands on: about 1.3 ha.
const CHANNEL_CELLS: f32 = 800.0;
/// The topographic wetness index (ln(a / tan b)) the convergence term starts
/// at and how far above that it reaches 1.
const TWI_FROM: f32 = 7.0;
const TWI_SPAN: f32 = 6.0;
/// Radius (m) of the mean ground a hollow is measured against, and the depth
/// below it that reads fully wet.
const HOLLOW_R: f32 = 20.0;
const HOLLOW_M: f32 = 2.5;
/// Height above the sea (m) at which `cold` reaches 1 at latitude 42: the
/// game's mountains are low, and Kitehold's páramo starts at 172 m.
const ALPINE_M: f32 = 220.0;
/// How disturbed ground surfaced with cinders, ash or lava crust reads.
const VOLCANIC: f32 = 0.6;
/// How much of `wet` the climate alone gives the wettest climate.
const CLIMATE: f32 = 0.45;

macro_rules! chans {
    ($($v:ident = $name:literal, $about:literal;)*) => {
        /// One channel of the site grid.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum SiteChan { $($v),* }

        impl SiteChan {
            pub const ALL: &'static [SiteChan] = &[$(SiteChan::$v),*];

            pub fn name(self) -> &'static str {
                match self { $(SiteChan::$v => $name),* }
            }

            /// One line for the map's legend.
            pub fn about(self) -> &'static str {
                match self { $(SiteChan::$v => $about),* }
            }
        }
    };
}

chans! {
    Wet = "wet", "WATER IN THE GROUND (PLANT SITE)";
    WetTable = "wet_table", "WET: STANDS LITTLE ABOVE A RIVER, LAKE OR CHANNEL";
    WetFlow = "wet_flow", "WET: COLLECTS FLOW FROM UPSLOPE";
    WetHollow = "wet_hollow", "WET: BELOW THE GROUND WITHIN 20 M";
    Soil = "soil", "SOIL: ACID AND POOR (0) TO BASE-RICH (1)";
    Sun = "sun", "WARMTH OF THE GROUND BY SLOPE AND ASPECT (0.5 FLAT)";
    Exposure = "exposure", "EXPOSED: HIGH, OR FACING INTO THE WIND";
    Cold = "cold", "COLD: HEIGHT, NORTH FACES, FROST HOLLOWS, FAR FROM THE SEA";
    Rock = "rock", "STEEP, CLIFF, OR THIN GROUND OVER ROCK";
    Disturbed = "disturbed", "RECENT BURN, CINDERS AND LAVA, PATHS, ROADS, BUILDINGS";
    Shade = "shade", "THE LAND'S OWN SHADE OVER THE DAY";
}

impl SiteChan {
    pub fn parse(s: &str) -> Option<SiteChan> {
        SiteChan::ALL.iter().copied().find(|c| c.name() == s)
    }
}

/// A world's site fields.
pub struct Site {
    /// Cells along each side.
    pub n: usize,
    /// One byte a cell, per channel in `SiteChan::ALL` order.
    chans: Vec<Vec<u8>>,
    /// The biome each cell is counted in (its middle 1 m cell), by library
    /// index.
    pub biome: Vec<u8>,
    /// The nearest other biome to each cell, by library index, and how far
    /// the border with it is, metres (255: that far or further).
    pub other: Vec<u8>,
    pub border: Vec<u8>,
    /// Cells mostly under open water or the sea.
    pub water: Vec<bool>,
    /// How long each part took, ms, for the report.
    pub parts: Vec<(&'static str, f32)>,
    /// Which realms each cell's life comes from (`realm_map`).
    pub realms: super::realm_map::RealmMap,
}

impl Site {
    pub fn idx(&self, x: f32, z: f32) -> usize {
        survey::cell_index(self.n, x, z)
    }

    pub fn grid(&self, c: SiteChan) -> &[u8] {
        &self.chans[c as usize]
    }

    /// The channel at a world point, 0 to 1, read bilinearly between cell
    /// middles.
    pub fn value(&self, c: SiteChan, x: f32, z: f32) -> f32 {
        let g = &self.chans[c as usize];
        let n = self.n;
        let fx = (x / CELL_M - 0.5).clamp(0.0, (n - 1) as f32);
        let fz = (z / CELL_M - 0.5).clamp(0.0, (n - 1) as f32);
        let (x0, z0) = (fx as usize, fz as usize);
        let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let at = |x: usize, z: usize| g[z * n + x] as f32 / 255.0;
        let a = at(x0, z0) * (1.0 - tx) + at(x1, z0) * tx;
        let b = at(x0, z1) * (1.0 - tx) + at(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
    }

    pub fn ms(&self) -> f32 {
        self.parts.iter().map(|p| p.1).sum()
    }

    /// Measure `w`'s ground. Reads the fields, the placed structures and the
    /// roads; nothing the plants make.
    pub fn build(w: &World) -> Site {
        let f = &w.f;
        let n1 = f.n;
        let n = survey::side(f.size);
        let mut parts = Vec::new();
        let mut t = std::time::Instant::now();
        let mut lap = |name: &'static str, parts: &mut Vec<(&'static str, f32)>| {
            parts.push((name, t.elapsed().as_secs_f32() * 1000.0));
            t = std::time::Instant::now();
        };

        // Each 4 m cell gathered from its 1 m cells.
        let hollow1 = hollow_1m(&f.h, n1);
        // Surface materials that are bare stone (a granite dome, a pavement).
        let stone: Vec<bool> = (0..=255u8).map(bare_stone).collect();
        let volcanic: Vec<bool> = (0..=255u8).map(volcanic_ground).collect();
        let mut cells = vec![Gather { level: f32::MIN, ..Gather::default() }; n * n];
        rows(n, &mut cells, |cx, cz, g| {
            let mut cnt = 0.0f32;
            let mut rocky = 0usize;
            let mut youngest = i32::MIN;
            for sz in 0..SUB {
                for sx in 0..SUB {
                    let (x, z) = (cx * SUB + sx, cz * SUB + sz);
                    if x >= n1 || z >= n1 {
                        continue;
                    }
                    let i = z * n1 + x;
                    cnt += 1.0;
                    g.h += f.h[i];
                    g.slope += f.slope[i];
                    g.cliff += f.cliff[i];
                    g.moist += f.moist[i];
                    g.coast += f.coast_dist[i];
                    g.hollow += hollow1[i].max(0.0);
                    let open = f.water[i] == 1 || f.h[i] < f.sea;
                    g.water += open as u8 as f32;
                    if open {
                        let level = if f.h[i] < f.sea { f.sea } else { f.inland.get(&(i as u32)).map_or(f.h[i], |w| w.level) };
                        g.level = g.level.max(level);
                    }
                    g.path += (f.water[i] == 3) as u8 as f32;
                    rocky += (f.slope[i] > 1.35 || (f.cliff[i] > 0.35 && f.slope[i] > 0.8)) as usize;
                    let top = if f.surface[i] != 0 { f.surface[i] } else { w.biome_mats.get(f.biome[i] as usize).map_or(0, |b| b.surface) };
                    g.bare += (stone[top as usize] && f.water[i] == 0) as u8 as f32;
                    g.volcanic += (volcanic[top as usize] && f.water[i] == 0) as u8 as f32;
                    youngest = youngest.max(f.burn_year[i]);
                    let loose: f32 = f.layers.get(i).map_or(0.0, |ls| ls.iter().filter(|l| l.mat != 0).map(|l| l.thick()).sum());
                    g.loose += loose;
                }
            }
            let c = cnt.max(1.0);
            for v in [&mut g.coast, &mut g.h, &mut g.slope, &mut g.cliff, &mut g.moist, &mut g.hollow, &mut g.water, &mut g.path, &mut g.loose, &mut g.bare, &mut g.volcanic] {
                *v /= c;
            }
            g.rocky = rocky as f32 / c;
            g.burn = youngest;
            let (mx, mz) = ((cx * SUB + SUB / 2).min(n1 - 1), (cz * SUB + SUB / 2).min(n1 - 1));
            g.biome = f.biome[mz * n1 + mx];
        });
        let h: Vec<f32> = cells.iter().map(|c| c.h).collect();
        let drain: Vec<bool> = cells.iter().map(|c| c.water >= 0.5).collect();
        // Drainage stands at its water's surface, not its bed.
        let surface: Vec<f32> = cells.iter().map(|c| if c.level > f32::MIN { c.level.max(c.h) } else { c.h }).collect();
        lap("gather", &mut parts);

        // Wet.
        let terms = wet_terms(&h, &surface, &drain, &cells.iter().map(|c| c.hollow).collect::<Vec<_>>(), n);
        lap("wet", &mut parts);
        let mut chans: Vec<Vec<u8>> = vec![vec![0u8; n * n]; SiteChan::ALL.len()];
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        for k in 0..n * n {
            let (a, b, c) = (terms.table[k], terms.flow[k], terms.hollow[k]);
            let climate = CLIMATE * cells[k].moist.clamp(0.0, 1.0);
            let wet = if drain[k] { 1.0 } else { soft_or(&[climate, a, b, c]) };
            chans[SiteChan::Wet as usize][k] = byte(wet);
            chans[SiteChan::WetTable as usize][k] = byte(a);
            chans[SiteChan::WetFlow as usize][k] = byte(b);
            chans[SiteChan::WetHollow as usize][k] = byte(c);
        }

        // Soil, graded across biome borders.
        let biome: Vec<u8> = cells.iter().map(|c| c.biome).collect();
        let (other, border_m) = borders(&biome, n);
        let soil_of = |b: u8| w.lib.biomes.get(b as usize).and_then(|d| d.soil).unwrap_or(0.5).clamp(0.0, 1.0);
        for k in 0..n * n {
            let own = soil_of(biome[k]);
            let s = if other[k] == biome[k] {
                own
            } else {
                let toward = 0.5 * (1.0 - border_m[k] / BORDER_M).clamp(0.0, 1.0);
                own + (soil_of(other[k]) - own) * toward
            };
            chans[SiteChan::Soil as usize][k] = byte(s);
        }
        lap("soil", &mut parts);

        // Sun, exposure and rock: from the 4 m heights and the gathered cells.
        let path = sun_path(w);
        let suns = sun_dirs(&path, 0.5);
        let flat: f32 = suns.iter().map(|s| s.y).sum::<f32>().max(1e-3);
        let far = box_mean(&h, n, (100.0 / CELL_M) as usize);
        let near = box_mean(&h, n, (HOLLOW_R / CELL_M) as usize);
        let wind = super::WIND;
        {
            let (sun_c, rest) = chans.split_at_mut(SiteChan::Exposure as usize);
            let sun_g = &mut sun_c[SiteChan::Sun as usize];
            let (exp_c, rest) = rest.split_at_mut(1);
            let rock_g = &mut rest[SiteChan::Rock as usize - SiteChan::Exposure as usize - 1];
            let exp_g = &mut exp_c[0];
            rows(n, sun_g, |x, z, out| {
                let nrm = normal(&h, n, x, z);
                let got: f32 = suns.iter().map(|s| nrm.dot(*s).max(0.0)).sum();
                // Flat ground 0.5; a face turned from the sun falls toward 0
                // with what it gets, one turned to it rises faster, since
                // even the best face gains only a fifth or so over flat.
                let r = got / flat;
                *out = byte(if r < 1.0 { 0.5 * r } else { 0.5 + (r - 1.0) * 2.5 });
            });
            rows(n, exp_g, |x, z, out| {
                let k = z * n + x;
                let (gx, gz) = grad(&h, n, x, z);
                let face = (gx * wind.x + gz * wind.y).clamp(-1.0, 1.0);
                *out = byte(0.5 + (h[k] - far[k]) / 40.0 + (h[k] - near[k]) / 16.0 + face * 0.3);
            });
            rows(n, rock_g, |x, z, out| {
                let c = &cells[z * n + x];
                let steep = super::noise::smoothstep(0.35, 1.35, c.slope);
                let cliff = (c.cliff / 0.35).clamp(0.0, 1.0);
                // The generator lays about a metre of dirt over rock
                // everywhere; deposits (sand, ash, silt) deepen it.
                let deep = (c.loose / 2.5).clamp(0.0, 1.0);
                *out = byte(soft_or(&[c.rocky, c.bare * 0.85, steep * 0.7, cliff * 0.8]) * (1.0 - 0.6 * deep));
            });
        }
        // Cold: height above the sea on one scale for every world, the
        // latitude, a north face, cold air pooled in a hollow, and the sea's
        // mildness near the shore.
        let latitude = w.def.sun.as_ref().and_then(|s| s.latitude).unwrap_or(42.0);
        for k in 0..n * n {
            let up = (h[k] - f.sea).max(0.0) / ALPINE_M;
            let lat = (latitude - 42.0) / 45.0 * 0.3;
            let north = (0.5 - chans[SiteChan::Sun as usize][k] as f32 / 255.0) * 0.2;
            let pool = ((far[k] - h[k]) / 15.0).clamp(0.0, 1.0) * 0.12;
            let shore = (1.0 - cells[k].coast / 150.0).clamp(0.0, 1.0) * 0.08;
            chans[SiteChan::Cold as usize][k] = byte(0.15 + 0.85 * up + lat + north + pool - shore);
        }
        lap("sun, exposure, rock, cold", &mut parts);

        // Disturbed: burns, paths and roads, things built.
        let mut path_seed: Vec<bool> = cells.iter().map(|c| c.path >= 0.25).collect();
        let seed_at = |seed: &mut [bool], x: f32, z: f32| {
            if x >= 0.0 && z >= 0.0 && x < f.size && z < f.size {
                seed[survey::cell_index(n, x, z)] = true;
            }
        };
        for s in &w.ribbons.segs {
            let len = ((s.b - s.a).length() / 2.0).ceil().max(1.0) as usize;
            for j in 0..=len {
                let p = s.a.lerp(s.b, j as f32 / len as f32);
                seed_at(&mut path_seed, p.x, p.z);
            }
        }
        for c in &w.circuits {
            for p in &c.points {
                seed_at(&mut path_seed, p.x, p.y);
            }
        }
        let mut built_seed = vec![false; n * n];
        for p in &w.placed {
            if !w.lib.structures.get(p.def).is_some_and(|d| d.decays) {
                continue;
            }
            let (x0, z0, x1, z1) = p.footprint(&w.shapes);
            let m = |v: i32| v as f32 * super::gen::VOXEL;
            let mut z = m(z0);
            while z <= m(z1) {
                let mut x = m(x0);
                while x <= m(x1) {
                    seed_at(&mut built_seed, x, z);
                    x += CELL_M * 0.5;
                }
                z += CELL_M * 0.5;
            }
        }
        let path_d = chamfer(&path_seed, n);
        let built_d = chamfer(&built_seed, n);
        for k in 0..n * n {
            let burn = if cells[k].burn == i32::MIN { 0.0 } else { (1.0 - (-cells[k].burn).max(0) as f32 / 80.0).clamp(0.0, 1.0) };
            let path = (1.0 - path_d[k] / 12.0).clamp(0.0, 1.0) * 0.9;
            let built = (1.0 - built_d[k] / 20.0).clamp(0.0, 1.0) * 0.8;
            // Cinders, ash and lava crust are open mineral ground, the
            // ground pioneers (fireweed) take after an eruption as after a fire.
            let volcanic = cells[k].volcanic * VOLCANIC;
            chans[SiteChan::Disturbed as usize][k] = byte(soft_or(&[burn, path, built, volcanic]));
        }
        lap("disturbed", &mut parts);

        // The land's shade, from the sun's path each hour and the sky.
        let hourly: Vec<Vec3> = sun_dirs(&path, 1.0).into_iter().filter(|s| s.y > 0.05).collect();
        land_shade(&h, n, &hourly, &mut chans[SiteChan::Shade as usize]);
        lap("shade", &mut parts);

        let border = border_m.iter().map(|&d| d.round().clamp(0.0, 255.0) as u8).collect();
        let realms = super::realm_map::RealmMap::build(w, &biome, n);
        lap("realms", &mut parts);
        Site { n, chans, biome, other, border, water: drain, parts, realms }
    }

    /// Lines for `--site-report`: per biome, each channel's spread over the
    /// biome's dry land (min, quartiles, max).
    pub fn report(&self, w: &World) -> Vec<String> {
        let mut out = vec![format!(
            "site fields: {} x {} cells of {CELL_M} m, built in {:.0} ms ({})",
            self.n,
            self.n,
            self.ms(),
            self.parts.iter().map(|(n, ms)| format!("{n} {ms:.0}")).collect::<Vec<_>>().join(", ")
        )];
        let mut present: Vec<(u8, usize)> = Vec::new();
        let dry: Vec<bool> = self.water.iter().map(|w| !w).collect();
        for k in 0..self.n * self.n {
            if !dry[k] {
                continue;
            }
            match present.iter_mut().find(|p| p.0 == self.biome[k]) {
                Some(p) => p.1 += 1,
                None => present.push((self.biome[k], 1)),
            }
        }
        present.sort_by(|a, b| b.1.cmp(&a.1));
        out.push(format!("{:<11} {:>5} {:>5} {:>5} {:>5} {:>5}", "", "min", "q1", "med", "q3", "max"));
        for (b, count) in present {
            let name = w.lib.biomes.get(b as usize).map_or("?", |d| d.name.as_str());
            let soil = w.lib.biomes.get(b as usize).and_then(|d| d.soil);
            let ha = count as f32 * CELL_M * CELL_M / 10_000.0;
            out.push(format!("{name}: {count} cells ({ha:.0} ha), soil {}", soil.map_or("unset (0.5)".to_string(), |s| format!("{s:.2}"))));
            for &c in SiteChan::ALL {
                let g = self.grid(c);
                let mut v: Vec<u8> = (0..g.len()).filter(|&k| dry[k] && self.biome[k] == b).map(|k| g[k]).collect();
                v.sort_unstable();
                let q = |p: f32| v[((v.len() - 1) as f32 * p).round() as usize] as f32 / 255.0;
                out.push(format!("  {:<9} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>5.2}", c.name(), q(0.0), q(0.25), q(0.5), q(0.75), q(1.0)));
            }
        }
        out
    }
}

impl Site {
    /// How lush the ground cover at (x, z) grows, 0 to 1, with 0.5 as
    /// ordinary ground (where grass and its colour stay as they always were):
    /// wetter ground, a north face or a sheltered dip lusher; dry ground, a
    /// south face, a wind-cut ridge or thin rocky ground drier. Read by the
    /// grass sprites (mesh/sprites.rs) and, as `lush_grid`, the ground's
    /// colour (scene.wgsl `site_lush`).
    pub fn lush(&self, x: f32, z: f32) -> f32 {
        let wet = ((self.value(SiteChan::Wet, x, z) - LUSH_WET) / 0.35).clamp(-1.0, 1.0);
        let north = (0.5 - self.value(SiteChan::Sun, x, z)) * 0.6;
        let shelter = (0.5 - self.value(SiteChan::Exposure, x, z)) * 0.4;
        let rock = self.value(SiteChan::Rock, x, z) * 0.5;
        (0.5 + 0.5 * (0.7 * wet + north + shelter - rock)).clamp(0.0, 1.0)
    }

    /// `lush` over a world `size` metres across, `n` cells square, as bytes.
    pub fn lush_grid(&self, size: f32, n: usize) -> Vec<u8> {
        let cell = size / n as f32;
        (0..n * n).map(|k| (self.lush(((k % n) as f32 + 0.5) * cell, ((k / n) as f32 + 0.5) * cell) * 255.0).round() as u8).collect()
    }
}

/// How strongly the grass sprites follow the site, as last applied: the
/// LOOK page's SITE LUSHNESS (render/grade.rs `site_lush`), f32 bits.
static STRENGTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);
/// The strength asked for and since when, while it waits to settle.
static ASKED: std::sync::Mutex<Option<(f32, std::time::Instant)>> = std::sync::Mutex::new(None);

pub fn lush_strength() -> f32 {
    f32::from_bits(STRENGTH.load(std::sync::atomic::Ordering::Relaxed))
}

/// Called each frame with the strength the look asks for. True once it has
/// held still for half a second at a new value: the caller then builds the
/// land again so the grass takes it (a drag of the slider builds it once).
pub fn settle_strength(want: f32) -> bool {
    if (want - lush_strength()).abs() < 1e-4 {
        *ASKED.lock().unwrap() = None;
        return false;
    }
    let mut asked = ASKED.lock().unwrap();
    match *asked {
        Some((v, since)) if (v - want).abs() < 1e-4 => {
            if since.elapsed().as_secs_f32() < 0.5 {
                return false;
            }
            STRENGTH.store(want.to_bits(), std::sync::atomic::Ordering::Relaxed);
            *asked = None;
            true
        }
        _ => {
            *asked = Some((want, std::time::Instant::now()));
            false
        }
    }
}

impl Site {
    /// `lush` scaled by the strength the look asks for (`lush_strength`),
    /// for the grass sprites.
    pub fn lush_scaled(&self, x: f32, z: f32) -> f32 {
        (0.5 + (self.lush(x, z) - 0.5) * lush_strength()).clamp(0.0, 1.0)
    }
}

/// Whether ground cover follows the site (`Site::lush`); `VG_LUSH=0` turns
/// it off, for comparing with the ground as it was.
pub fn lush_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VG_LUSH").map_or(true, |v| v.trim() != "0"))
}

/// The `wet` of ordinary ground, where `lush` neither leans wet nor dry.
const LUSH_WET: f32 = 0.32;

/// How far outside a plant's range (`SitePrefs`) a field may go before the
/// plant has almost nothing of its suitability left.
pub const SOFT: f32 = 0.15;
/// What is left of suitability far outside a range: rarely, not never.
const FLOOR: f32 = 0.03;

/// How well a value suits a soft range `[lo, hi]`: 1 inside, falling
/// linearly to `FLOOR` over `SOFT` outside.
pub fn fit(r: [f32; 2], v: f32) -> f32 {
    let out = if v < r[0] { r[0] - v } else if v > r[1] { v - r[1] } else { 0.0 };
    (1.0 - out / SOFT).max(FLOOR)
}

impl Site {
    /// How well the ground at (x, z) suits a plant with these preferences,
    /// 0 to 1: the product of each field's fit. `shade` is the shade there
    /// now (the land's, with the crowns placed so far).
    pub fn suits(&self, prefs: &crate::library::SitePrefs, x: f32, z: f32, shade: f32) -> f32 {
        let mut s = 1.0;
        for (c, r) in [
            (SiteChan::Wet, prefs.wet),
            (SiteChan::Soil, prefs.soil),
            (SiteChan::Sun, prefs.sun),
            (SiteChan::Exposure, prefs.exposure),
            (SiteChan::Cold, prefs.cold),
            (SiteChan::Rock, prefs.rock),
            (SiteChan::Disturbed, prefs.disturbed),
        ] {
            if let Some(r) = r {
                s *= fit(r, self.value(c, x, z));
            }
        }
        if let Some(r) = prefs.shade {
            s *= fit(r, shade);
        }
        s
    }
}

/// Lines for `--site-report`: per plant with a site block, how many stand in
/// this world and how well their ground suits them (every field but shade,
/// since the crown shade they grew under is not kept): the mean
/// suitability, the share that stands where it is at least half, and each
/// ranged field's median at their feet against the range.
pub fn plant_report(w: &World) -> Vec<String> {
    let Some(site) = w.site.as_ref() else { return Vec::new() };
    let mut by: Vec<Vec<usize>> = vec![Vec::new(); w.lib.plants.len()];
    for (k, t) in w.trees.iter().enumerate() {
        if let Some(v) = by.get_mut(t.plant as usize) {
            v.push(k);
        }
    }
    let placing = if crate::world::flora::placement_v2(w) { "v2 (by site)" } else { "v1 (by list)" };
    let mut out = vec![format!("plants, placement {placing}: count, mean fit, share fitting at least half, median of each ranged field [range] and the share where that field alone fits under half")];
    let mut rows: Vec<(usize, String)> = Vec::new();
    let (mut all, mut good, mut sum) = (0usize, 0usize, 0f32);
    for (pi, stems) in by.iter().enumerate() {
        let plant = &w.lib.plants[pi];
        let Some(prefs) = plant.site.as_ref() else { continue };
        if stems.is_empty() {
            continue;
        }
        let no_shade = crate::library::SitePrefs { shade: None, ..prefs.clone() };
        let fits: Vec<f32> = stems.iter().map(|&k| site.suits(&no_shade, w.trees[k].pos.x, w.trees[k].pos.z, 0.0)).collect();
        let mean = fits.iter().sum::<f32>() / fits.len() as f32;
        let half = fits.iter().filter(|&&f| f >= 0.5).count();
        all += fits.len();
        good += half;
        sum += fits.iter().sum::<f32>();
        let mut cols = Vec::new();
        for (c, r) in [
            (SiteChan::Wet, prefs.wet),
            (SiteChan::Soil, prefs.soil),
            (SiteChan::Sun, prefs.sun),
            (SiteChan::Exposure, prefs.exposure),
            (SiteChan::Cold, prefs.cold),
            (SiteChan::Rock, prefs.rock),
            (SiteChan::Disturbed, prefs.disturbed),
        ] {
            if let Some([lo, hi]) = r {
                let mut v: Vec<f32> = stems.iter().map(|&k| site.value(c, w.trees[k].pos.x, w.trees[k].pos.z)).collect();
                // The share standing where this field alone fits under half.
                let off = v.iter().filter(|&&x| fit([lo, hi], x) < 0.5).count() as f32 * 100.0 / v.len() as f32;
                v.sort_by(f32::total_cmp);
                cols.push(format!("{} {:.2} [{lo:.2}-{hi:.2}] {off:.0}% out", c.name(), v[v.len() / 2]));
            }
        }
        rows.push((
            stems.len(),
            format!("  {:<18} {:>6} {:>5.2} {:>4.0}%  {}", plant.name, stems.len(), mean, half as f32 * 100.0 / stems.len() as f32, cols.join("  ")),
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    out.extend(rows.into_iter().map(|r| r.1));
    if all > 0 {
        out.push(format!("  all plants with a site block: {all}, mean fit {:.2}, {:.0}% fitting at least half", sum / all as f32, good as f32 * 100.0 / all as f32));
    }
    // Every plant (vines, crusts and stalks aside) by the biome it stands in.
    let mut tally: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for t in &w.trees {
        let Some(plant) = w.lib.plants.get(t.plant as usize) else { continue };
        let b = w.f.biome[w.f.idx(t.pos.x, t.pos.z)] as usize;
        let biome = w.lib.biomes.get(b).map_or("?", |d| d.name.as_str()).to_string();
        *tally.entry((biome, plant.name.clone())).or_default() += 1;
    }
    out.push("by biome: biome, plant, count".to_string());
    for ((b, p), c) in tally {
        out.push(format!("  biome {b} {p} {c}"));
    }
    out
}

/// `--site-report`: build the world, print its site fields by biome.
pub fn command(name: &str) -> i32 {
    let dir = crate::worldload::world_dir(name);
    let (edges, _) = crate::worldload::edges_for(name);
    let (lib, _) = crate::worldload::library_for(name);
    let w = match super::chronicle::build_world_with(&dir, lib, &edges) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("world {name}: {e}");
            return 1;
        }
    };
    let Some(site) = w.site.as_ref() else {
        eprintln!("world {name}: no site fields");
        return 1;
    };
    println!("world {name}");
    for l in site.report(&w) {
        println!("{l}");
    }
    for l in plant_report(&w) {
        println!("{l}");
    }
    for l in w.log.iter().filter(|l| l.starts_with("plants v2")) {
        println!("{l}");
    }
    0
}

/// What a 4 m cell gathers from its 1 m cells (means, unless said).
#[derive(Clone, Copy, Default)]
struct Gather {
    h: f32,
    slope: f32,
    cliff: f32,
    moist: f32,
    /// Distance to the sea, metres.
    coast: f32,
    /// Depth below the mean ground within `HOLLOW_R`, metres (0 where above).
    hollow: f32,
    /// Share under open water or the sea.
    water: f32,
    /// The highest water surface over it, metres (f32::MIN: none).
    level: f32,
    /// Share that is path.
    path: f32,
    /// Loose deposits over the rock, metres.
    loose: f32,
    /// Share of 1 m cells the generator draws as bare rock.
    rocky: f32,
    /// Share whose surface material is stone.
    bare: f32,
    /// Share whose surface is cinders, ash or lava crust.
    volcanic: f32,
    /// The latest fire's year (i32::MIN: none).
    burn: i32,
    biome: u8,
}

/// Whether material `m` is bare rock by its name: a granite dome, a basalt
/// flow. Built surfaces (paving, cobbles) are not counted.
fn bare_stone(m: u8) -> bool {
    const ROCK: [&str; 14] = ["granite", "basalt", "bedrock", "slate", "marble", "limestone", "sandstone", "obsidian", "tuff", "gneiss", "schist", "lava", "rock", "stone"];
    const BUILT: [&str; 11] = ["cobble", "brick", "pav", "flag", "block", "carved", "tile", "temple", "weapon", "wall", "field"];
    let Some(n) = super::matlib::key(m).or(super::matlib::name(m)) else { return false };
    ROCK.iter().any(|w| n.contains(w)) && !BUILT.iter().any(|w| n.contains(w))
}

/// Whether material `m` is volcanic ground by its name: cinders, ash,
/// lava crust, pahoehoe, scoria.
fn volcanic_ground(m: u8) -> bool {
    const WORDS: [&str; 7] = ["cinder", "cinders", "ash", "lava", "pahoehoe", "scoria", "tephra"];
    let Some(n) = super::matlib::key(m).or(super::matlib::name(m)) else { return false };
    n.split(|c: char| c == '_' || c == '-').any(|w| WORDS.contains(&w))
}

fn soft_or(v: &[f32]) -> f32 {
    1.0 - v.iter().fold(1.0, |a, &x| a * (1.0 - x.clamp(0.0, 1.0)))
}

/// Run `f(x, z, cell)` over a grid `n` wide, split by rows across the build's
/// threads.
fn rows<T: Send>(n: usize, out: &mut [T], f: impl Fn(usize, usize, &mut T) + Sync) {
    let per = n.div_ceil(build_threads()).max(1);
    let f = &f;
    std::thread::scope(|sc| {
        for (b, chunk) in out.chunks_mut(per * n).enumerate() {
            sc.spawn(move || {
                for (k, v) in chunk.iter_mut().enumerate() {
                    let i = b * per * n + k;
                    f(i % n, i / n, v);
                }
            });
        }
    });
}

/// Mean of `v` over a square of radius `r` cells round each cell (clipped at
/// the edge), from a summed-area table.
fn box_mean(v: &[f32], n: usize, r: usize) -> Vec<f32> {
    let w = n + 1;
    let mut sat = vec![0f64; w * w];
    for z in 0..n {
        let mut row = 0f64;
        for x in 0..n {
            row += v[z * n + x] as f64;
            sat[(z + 1) * w + x + 1] = sat[z * w + x + 1] + row;
        }
    }
    let mut out = vec![0f32; n * n];
    rows(n, &mut out, |x, z, o| {
        let (x0, z0) = (x.saturating_sub(r), z.saturating_sub(r));
        let (x1, z1) = ((x + r + 1).min(n), (z + r + 1).min(n));
        let s = sat[z1 * w + x1] - sat[z0 * w + x1] - sat[z1 * w + x0] + sat[z0 * w + x0];
        *o = (s / ((x1 - x0) * (z1 - z0)) as f64) as f32;
    });
    out
}

/// Each 1 m cell's depth below the mean ground within `HOLLOW_R`.
fn hollow_1m(h: &[f32], n: usize) -> Vec<f32> {
    let mean = box_mean(h, n, (HOLLOW_R / CELL) as usize);
    mean.iter().zip(h).map(|(m, h)| m - h).collect()
}

/// Height gradient (rise per metre) at a cell of the 4 m grid.
fn grad(h: &[f32], n: usize, x: usize, z: usize) -> (f32, f32) {
    let at = |x: usize, z: usize| h[z.min(n - 1) * n + x.min(n - 1)];
    let (xl, xr) = (x.saturating_sub(1), (x + 1).min(n - 1));
    let (zl, zr) = (z.saturating_sub(1), (z + 1).min(n - 1));
    let gx = (at(xr, z) - at(xl, z)) / (((xr - xl).max(1)) as f32 * CELL_M);
    let gz = (at(x, zr) - at(x, zl)) / (((zr - zl).max(1)) as f32 * CELL_M);
    (gx, gz)
}

fn normal(h: &[f32], n: usize, x: usize, z: usize) -> Vec3 {
    let (gx, gz) = grad(h, n, x, z);
    Vec3::new(-gx, 1.0, -gz).normalize()
}

/// The three wet terms per cell, 0 to 1.
struct WetTerms {
    table: Vec<f32>,
    flow: Vec<f32>,
    hollow: Vec<f32>,
    /// Upslope cells drained through each cell (itself included).
    #[allow(dead_code)] // for tests and later readers
    acc: Vec<f32>,
}

/// `h` is the 4 m ground, `surface` the water's surface over it where there
/// is water (else the ground), `drain` the cells under open water or the
/// sea, `hollow` each cell's mean depth below the ground within 20 m.
fn wet_terms(h: &[f32], surface: &[f32], drain: &[bool], hollow: &[f32], n: usize) -> WetTerms {
    let fl = flood(h, drain, n);
    // The flow runs over the ground with its shallow pits filled (the
    // noise's dimples would stop every wash) and its deep ones kept.
    let ground = keep_deep_pits(h, &fl.filled, n);
    let acc = accumulate(&ground, drain, n);
    // Drainage the water table stands on: open water and the channels the
    // flow finds. Each cell's is the first one down the way its water runs
    // out (over the filled ground, so out of a pit too).
    let seed: Vec<bool> = (0..n * n).map(|k| drain[k] || acc[k] >= CHANNEL_CELLS).collect();
    let mut base = vec![f32::NAN; n * n];
    for &k in &fl.order {
        let k = k as usize;
        base[k] = if seed[k] { surface[k] } else if fl.parent[k] != u32::MAX { base[fl.parent[k] as usize] } else { f32::NAN };
    }
    let table: Vec<f32> = (0..n * n)
        .map(|k| if base[k].is_nan() { 0.0 } else { (1.0 - (h[k] - base[k]).max(0.0) / TABLE_M).clamp(0.0, 1.0).powi(2) })
        .collect();
    let mut flow = vec![0f32; n * n];
    rows(n, &mut flow, |x, z, o| {
        let k = z * n + x;
        let (gx, gz) = grad(&ground, n, x, z);
        let tan_b = (gx * gx + gz * gz).sqrt().max(0.005);
        let twi = (acc[k] * CELL_M / tan_b).ln();
        *o = ((twi - TWI_FROM) / TWI_SPAN).clamp(0.0, 1.0);
    });
    let hollow = hollow.iter().map(|d| (d / HOLLOW_M).clamp(0.0, 1.0)).collect();
    WetTerms { table, flow, hollow, acc }
}

/// Pits shallower than this (m) are filled before the flow is run.
const FILL_M: f32 = 0.6;
/// The rise a filled flat is given per cell, so water crosses it.
const FLAT_EPS: f32 = 1e-3;

struct Flood {
    /// The ground with every pit filled to its spill, with a slight fall
    /// across flats.
    filled: Vec<f32>,
    /// The cell each cell's water runs out through (u32::MAX at the edge
    /// and at open water).
    parent: Vec<u32>,
    /// Cells in the order the flood reached them, from the outlets up.
    order: Vec<u32>,
}

/// Priority flood from the world's edge and open water inward.
fn flood(h: &[f32], drain: &[bool], n: usize) -> Flood {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;
    #[derive(PartialEq)]
    struct Item(f32, u32);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Item {
        fn cmp(&self, o: &Self) -> Ordering {
            o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
        }
    }
    let mut filled = h.to_vec();
    let mut parent = vec![u32::MAX; n * n];
    let mut done = vec![false; n * n];
    let mut order = Vec::with_capacity(n * n);
    let mut heap = BinaryHeap::new();
    for k in 0..n * n {
        let (x, z) = (k % n, k / n);
        if drain[k] || x == 0 || z == 0 || x == n - 1 || z == n - 1 {
            done[k] = true;
            heap.push(Item(h[k], k as u32));
        }
    }
    while let Some(Item(v, k)) = heap.pop() {
        order.push(k);
        let (x, z) = ((k as usize % n) as i32, (k as usize / n) as i32);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                continue;
            }
            let j = nz as usize * n + nx as usize;
            if done[j] {
                continue;
            }
            done[j] = true;
            filled[j] = h[j].max(v + FLAT_EPS);
            parent[j] = k;
            heap.push(Item(filled[j], j as u32));
        }
    }
    Flood { filled, parent, order }
}

/// `filled` where a pit is shallower than `FILL_M`, the ground itself in a
/// deeper one, so a pan keeps what runs into it.
fn keep_deep_pits(h: &[f32], filled: &[f32], n: usize) -> Vec<f32> {
    let raised = |k: usize| filled[k] - h[k] > FLAT_EPS * 0.5;
    let mut out = filled.to_vec();
    let mut seen = vec![false; n * n];
    let mut stack = Vec::new();
    let mut members = Vec::new();
    for s in 0..n * n {
        if seen[s] || !raised(s) {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        members.clear();
        let mut deepest = 0f32;
        while let Some(k) = stack.pop() {
            members.push(k);
            deepest = deepest.max(filled[k] - h[k]);
            let (x, z) = ((k % n) as i32, (k / n) as i32);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                    continue;
                }
                let j = nz as usize * n + nx as usize;
                if !seen[j] && raised(j) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if deepest > FILL_M {
            for &k in &members {
                out[k] = h[k];
            }
        }
    }
    out
}

/// Flow accumulated through each cell, in cells, spread to every lower
/// neighbour by its drop (multiple flow directions). Pits are not filled:
/// what runs into one stays there. Open water takes what reaches it.
fn accumulate(h: &[f32], drain: &[bool], n: usize) -> Vec<f32> {
    let mut order: Vec<u32> = (0..(n * n) as u32).collect();
    order.sort_unstable_by(|&a, &b| h[b as usize].total_cmp(&h[a as usize]));
    let mut acc = vec![1f32; n * n];
    const NB: [(i32, i32, f32); 8] = [
        (1, 0, 1.0),
        (-1, 0, 1.0),
        (0, 1, 1.0),
        (0, -1, 1.0),
        (1, 1, std::f32::consts::SQRT_2),
        (1, -1, std::f32::consts::SQRT_2),
        (-1, 1, std::f32::consts::SQRT_2),
        (-1, -1, std::f32::consts::SQRT_2),
    ];
    for &k in &order {
        let k = k as usize;
        if drain[k] {
            continue;
        }
        let (x, z) = ((k % n) as i32, (k / n) as i32);
        let mut w = [0f32; 8];
        let mut sum = 0.0;
        for (j, &(dx, dz, d)) in NB.iter().enumerate() {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                continue;
            }
            let drop = h[k] - h[nz as usize * n + nx as usize];
            if drop > 0.0 {
                w[j] = (drop / d).powf(1.1);
                sum += w[j];
            }
        }
        if sum <= 0.0 {
            continue;
        }
        let a = acc[k];
        for (j, &(dx, dz, _)) in NB.iter().enumerate() {
            if w[j] > 0.0 {
                acc[(z + dz) as usize * n + (x + dx) as usize] += a * w[j] / sum;
            }
        }
    }
    acc
}

/// Chamfer distance in metres to the nearest seed cell (f32::MAX: none).
fn chamfer(seed: &[bool], n: usize) -> Vec<f32> {
    let mut d: Vec<f32> = seed.iter().map(|&s| if s { 0.0 } else { f32::MAX }).collect();
    let (a, b) = (CELL_M, CELL_M * std::f32::consts::SQRT_2);
    let relax = |d: &mut [f32], k: usize, j: usize, step: f32| {
        if d[j] != f32::MAX && d[j] + step < d[k] {
            d[k] = d[j] + step;
        }
    };
    for z in 0..n {
        for x in 0..n {
            let k = z * n + x;
            if x > 0 {
                relax(&mut d, k, k - 1, a);
            }
            if z > 0 {
                relax(&mut d, k, k - n, a);
                if x > 0 {
                    relax(&mut d, k, k - n - 1, b);
                }
                if x + 1 < n {
                    relax(&mut d, k, k - n + 1, b);
                }
            }
        }
    }
    for z in (0..n).rev() {
        for x in (0..n).rev() {
            let k = z * n + x;
            if x + 1 < n {
                relax(&mut d, k, k + 1, a);
            }
            if z + 1 < n {
                relax(&mut d, k, k + n, a);
                if x + 1 < n {
                    relax(&mut d, k, k + n + 1, b);
                }
                if x > 0 {
                    relax(&mut d, k, k + n - 1, b);
                }
            }
        }
    }
    d
}

/// For each cell, the nearest other biome and the distance to the border
/// with it (metres; the cell's own biome and f32::MAX where it has none).
fn borders(biome: &[u8], n: usize) -> (Vec<u8>, Vec<f32>) {
    let mut present: Vec<u8> = biome.to_vec();
    present.sort_unstable();
    present.dedup();
    let dists: Vec<Vec<f32>> = std::thread::scope(|sc| {
        let hs: Vec<_> = present
            .iter()
            .map(|&b| sc.spawn(move || chamfer(&biome.iter().map(|&c| c == b).collect::<Vec<_>>(), n)))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut other = biome.to_vec();
    let mut dist = vec![f32::MAX; n * n];
    for k in 0..n * n {
        for (j, &b) in present.iter().enumerate() {
            if b != biome[k] && dists[j][k] < dist[k] {
                dist[k] = dists[j][k];
                other[k] = b;
            }
        }
        // The border runs halfway between this cell and the other biome's
        // nearest cell.
        if dist[k] != f32::MAX {
            dist[k] = (dist[k] - CELL_M * 0.5).max(0.0);
        }
    }
    (other, dist)
}

/// The sun's path this world sets, at the equinox: the year's mean.
pub(crate) fn sun_path(w: &World) -> crate::render::daylight::SunPath {
    let mut p = w.def.sun.as_ref().map_or(crate::render::daylight::SunPath::DEFAULT, |s| s.path());
    p.declination = 0.0;
    p
}

/// Directions toward the sun every `step` hours while it is up.
fn sun_dirs(p: &crate::render::daylight::SunPath, step: f32) -> Vec<Vec3> {
    let mut out = Vec::new();
    let mut hour = step * 0.5;
    while hour < 24.0 {
        let d = p.dir(hour);
        if d.y > 0.0 {
            out.push(d);
        }
        hour += step;
    }
    out
}

/// Bilinear height on the 4 m grid at a point in cell units, None off it.
fn height_at(h: &[f32], n: usize, x: f32, z: f32) -> Option<f32> {
    if x < 0.0 || z < 0.0 || x > (n - 1) as f32 || z > (n - 1) as f32 {
        return None;
    }
    let (x0, z0) = (x as usize, z as usize);
    let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
    let (tx, tz) = (x - x0 as f32, z - z0 as f32);
    let a = h[z0 * n + x0] * (1.0 - tx) + h[z0 * n + x1] * tx;
    let b = h[z1 * n + x0] * (1.0 - tx) + h[z1 * n + x1] * tx;
    Some(a * (1.0 - tz) + b * tz)
}

/// The steepest rise (tan of the angle) seen from cell (x, z) along `dir`
/// within `reach` metres, from `eye` metres over the ground; stops early once
/// past `stop`.
fn horizon(h: &[f32], n: usize, x: usize, z: usize, dir: Vec2, reach: f32, eye: f32, stop: f32) -> f32 {
    let h0 = h[z * n + x] + eye;
    let mut t = CELL_M;
    let mut best = f32::MIN;
    while t < reach {
        let (px, pz) = (x as f32 + dir.x * t / CELL_M, z as f32 + dir.y * t / CELL_M);
        let Some(hs) = height_at(h, n, px, pz) else { break };
        best = best.max((hs - h0) / t);
        if best > stop {
            break;
        }
        t += (t * 0.15).max(CELL_M);
    }
    best
}

/// The land's shade: how much of the day's direct sun (`suns`, hourly) a
/// cell misses, three parts, and how much sky the land round it cuts off,
/// one part. Sun is counted by how squarely it strikes the ground against
/// flat open ground, capped there, so a north face the sun only grazes
/// reads shaded though the sun is above its horizon.
fn land_shade(h: &[f32], n: usize, suns: &[Vec3], out: &mut [u8]) {
    let sky: Vec<Vec2> = (0..16).map(|i| Vec2::from_angle(i as f32 / 16.0 * std::f32::consts::TAU)).collect();
    rows(n, out, |x, z, o| {
        let nrm = normal(h, n, x, z);
        let mut lit = 0.0f32;
        let flat: f32 = suns.iter().map(|s| s.y).sum::<f32>().max(1e-3);
        for s in suns {
            let strike = nrm.dot(*s);
            if strike <= 0.0 {
                continue;
            }
            let across = Vec2::new(s.x, s.z);
            let rise = s.y / across.length().max(1e-4);
            if horizon(h, n, x, z, across.normalize_or_zero(), 600.0, 1.0, rise) <= rise {
                lit += strike;
            }
        }
        let lit = (lit / flat).min(1.0);
        let open: f32 = sky.iter().map(|d| 1.0 - horizon(h, n, x, z, *d, 200.0, 1.0, f32::MAX).max(0.0).atan().sin()).sum::<f32>() / sky.len() as f32;
        *o = ((1.0 - (0.75 * lit + 0.25 * open)).clamp(0.0, 1.0) * 255.0).round() as u8;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 48;

    /// A plateau 60 m up, sloping down to a river along x = 0.
    fn plateau() -> Vec<f32> {
        (0..N * N).map(|k| (((k % N) as f32 - 4.0) * 3.0).clamp(0.0, 60.0)).collect()
    }

    fn river() -> Vec<bool> {
        (0..N * N).map(|k| k % N == 0).collect()
    }

    fn wet_of(h: &[f32], drain: &[bool]) -> Vec<f32> {
        let hollow1: Vec<f32> = hollow_cells(h);
        let t = wet_terms(h, h, drain, &hollow1, N);
        (0..N * N).map(|k| soft_or(&[t.table[k], t.flow[k], t.hollow[k]])).collect()
    }

    /// The hollow input as `build` gathers it, worked on the 4 m grid.
    fn hollow_cells(h: &[f32]) -> Vec<f32> {
        let mean = box_mean(h, N, (HOLLOW_R / CELL_M) as usize);
        mean.iter().zip(h).map(|(m, h)| (m - h).max(0.0)).collect()
    }

    #[test]
    fn a_closed_pit_on_a_plateau_reads_wet() {
        let mut h = plateau();
        let (px, pz) = (34, 24);
        for dz in -2i32..=2 {
            for dx in -2i32..=2 {
                let d = ((dx * dx + dz * dz) as f32).sqrt();
                h[(pz + dz) as usize * N + (px + dx) as usize] -= (2.0 - d * 0.6).max(0.0);
            }
        }
        let wet = wet_of(&h, &river());
        let pit = wet[pz as usize * N + px as usize];
        let plain = wet[pz as usize * N + 42];
        assert!(pit > 0.5, "pit {pit}");
        assert!(plain < 0.1, "plain {plain}");
    }

    #[test]
    fn the_river_bank_is_wet_and_the_plateau_is_not() {
        let h = plateau();
        let wet = wet_of(&h, &river());
        assert!(wet[20 * N + 2] > 0.7, "bank {}", wet[20 * N + 2]);
        // A dead flat plateau drains slowly across its flats, so it reads a
        // little wet along the lines its water takes; never like the bank.
        assert!(wet[20 * N + 40] < 0.3, "plateau {}", wet[20 * N + 40]);
    }

    #[test]
    fn a_bank_reads_against_the_water_not_the_bed() {
        // A river 2 m deep with its banks half a metre above the water.
        let h: Vec<f32> = (0..N * N).map(|k| if k % N < 3 { -2.0 } else { 0.5 + (k % N - 3) as f32 * 0.5 }).collect();
        let surface: Vec<f32> = (0..N * N).map(|k| if k % N < 3 { 0.0 } else { h[k] }).collect();
        let drain: Vec<bool> = (0..N * N).map(|k| k % N < 3).collect();
        let t = wet_terms(&h, &surface, &drain, &vec![0.0; N * N], N);
        assert!(t.table[20 * N + 3] > 0.6, "bank {}", t.table[20 * N + 3]);
    }

    #[test]
    fn a_wash_collects_flow_down_its_floor() {
        // A valley down z with a shallow cut along its floor.
        let mut h: Vec<f32> = (0..N * N).map(|k| 40.0 - (k / N) as f32 * 0.4 + ((k % N) as f32 - 24.0).abs() * 0.3).collect();
        for z in 0..N {
            for dx in -1i32..=1 {
                h[z * N + (24 + dx) as usize] -= 1.0 - dx.abs() as f32 * 0.5;
            }
        }
        let t = wet_terms(&h, &h, &vec![false; N * N], &hollow_cells(&h), N);
        let (wash, side) = (40 * N + 24, 40 * N + 10);
        assert!(t.acc[wash] > 10.0 * t.acc[side], "acc {} vs {}", t.acc[wash], t.acc[side]);
        assert!(t.flow[wash] > t.flow[side] + 0.2, "flow {} vs {}", t.flow[wash], t.flow[side]);
        assert!(t.hollow[wash] > 0.3);
    }

    #[test]
    fn a_shallow_dimple_does_not_stop_a_wash_but_a_deep_pit_does() {
        let valley = |dimple: f32| {
            let mut h: Vec<f32> = (0..N * N).map(|k| 40.0 - (k / N) as f32 * 0.2 + ((k % N) as f32 - 24.0).abs() * 0.3).collect();
            h[20 * N + 24] -= dimple;
            let t = wet_terms(&h, &h, &vec![false; N * N], &vec![0.0; N * N], N);
            t.acc[40 * N + 24]
        };
        let (none, shallow, deep) = (valley(0.0), valley(0.3), valley(3.0));
        assert!(shallow > none * 0.9, "shallow {shallow} vs {none}");
        assert!(deep < none * 0.7, "deep {deep} vs {none}");
    }

    #[test]
    fn fit_is_full_inside_and_falls_off_outside() {
        assert_eq!(fit([0.4, 0.8], 0.6), 1.0);
        assert!((fit([0.4, 0.8], 0.4 - SOFT * 0.5) - 0.5).abs() < 1e-5);
        assert_eq!(fit([0.4, 0.8], 0.0), FLOOR);
        assert_eq!(fit([0.4, 0.8], 1.0), FLOOR);
    }

    #[test]
    fn soil_border_distance() {
        let biome: Vec<u8> = (0..N * N).map(|k| if k % N < 20 { 1 } else { 2 }).collect();
        let (other, d) = borders(&biome, N);
        assert_eq!(other[5 * N + 19], 2);
        assert_eq!(other[5 * N + 20], 1);
        assert_eq!(d[5 * N + 19], CELL_M * 0.5);
        assert_eq!(d[5 * N + 20], CELL_M * 0.5);
        assert!((d[5 * N + 10] - (10.0 * CELL_M - 2.0)).abs() < 0.01);
    }

    #[test]
    fn a_north_face_is_shaded_and_a_south_face_is_not() {
        // A ridge along x: north of it (low z) faces north.
        let h: Vec<f32> = (0..N * N).map(|k| 30.0 - ((k / N) as f32 - 24.0).abs() * 2.5).collect();
        let p = crate::render::daylight::SunPath { declination: 0.0, ..crate::render::daylight::SunPath::DEFAULT };
        let suns: Vec<Vec3> = sun_dirs(&p, 1.0).into_iter().filter(|s| s.y > 0.05).collect();
        let mut out = vec![0u8; N * N];
        land_shade(&h, N, &suns, &mut out);
        let (north, south) = (out[18 * N + 24], out[30 * N + 24]);
        assert!(north > south + 60, "north {north} south {south}");
    }
}
