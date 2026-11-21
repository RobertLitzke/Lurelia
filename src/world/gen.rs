//! Terrain generation. Pure functions of a `WorldSpec`: any column or chunk can be
//! generated on any thread in any order and comes out the same, at any level of
//! detail, because coarse voxels sample the same functions at coarser spacing.

use super::canopy::Blob;
use super::material::*;
use super::matlib;
use super::noise::*;
use super::spec::WorldSpec;
use super::stamp::{Press, Stamp};
use crate::util::FxHashMap as HashMap;
use glam::Vec3;

/// Voxels per metre.
pub const VPM: f32 = 4.0;
/// Edge of one voxel, metres.
pub const VOXEL: f32 = 0.25;
/// Chunk edge in voxels (at its own level of detail).
pub const CHUNK: i32 = 32;
/// Neighbour data around a chunk for meshing. Sloped ground averages the
/// surfaces in a 4x4 block of columns around each corner, four levels up and
/// down (mesh::REACH), so the mesher needs two columns of margin across and
/// six levels of margin above and below.
pub const M: usize = 2;
pub const MY: usize = 6;
/// Padded chunk edge across and height.
pub const PAD: usize = 32 + 2 * M;
pub const PADY: usize = 32 + 2 * MY;
pub const PAD3: usize = PAD * PAD * PADY;
pub const CHUNK3: usize = 32 * 32 * 32;

#[inline]
pub fn pidx(x: usize, y: usize, z: usize) -> usize {
    (y * PAD + z) * PAD + x
}

#[inline]
pub fn cidx(x: usize, y: usize, z: usize) -> usize {
    (y * 32 + z) * 32 + x
}

/// A vertical stack of chunks at one level of detail. Level `lod` voxels are
/// `2^lod` base voxels across, so the column covers `32 << lod` base voxels.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ColKey {
    pub lod: u8,
    pub x: i32,
    pub z: i32,
}

impl ColKey {
    pub fn size_vox(&self) -> i32 {
        CHUNK << self.lod
    }
    pub fn size_m(&self) -> f32 {
        self.size_vox() as f32 * VOXEL
    }
    /// The column's footprint in metres, as (minimum, maximum) on x and z.
    pub fn footprint_m(&self) -> (glam::Vec2, glam::Vec2) {
        let w = self.size_m();
        let lo = glam::Vec2::new(self.x as f32 * w, self.z as f32 * w);
        (lo, lo + glam::Vec2::splat(w))
    }

    pub fn parent(&self) -> ColKey {
        ColKey {
            lod: self.lod + 1,
            x: self.x.div_euclid(2),
            z: self.z.div_euclid(2),
        }
    }
    pub fn children(&self) -> [ColKey; 4] {
        let l = self.lod - 1;
        let (x, z) = (self.x * 2, self.z * 2);
        [
            ColKey { lod: l, x, z },
            ColKey { lod: l, x: x + 1, z },
            ColKey { lod: l, x, z: z + 1 },
            ColKey { lod: l, x: x + 1, z: z + 1 },
        ]
    }
    /// Horizontal distance in metres from a point to this column's footprint.
    pub fn dist_m(&self, px: f32, pz: f32) -> f32 {
        let s = self.size_m();
        let x0 = self.x as f32 * s;
        let z0 = self.z as f32 * s;
        let dx = (x0 - px).max(px - (x0 + s)).max(0.0);
        let dz = (z0 - pz).max(pz - (z0 + s)).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Column {
    /// Surface height, metres.
    pub h: f32,
    /// 0..1, how much this place is a cliff (terraces, rock, overhangs).
    pub cliff: f32,
    pub moist: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeKind {
    Broad,
    Birch,
    Pine,
    /// Hanging vine strands (flora.rs `place_vines`): each blob is a strand's
    /// top point and its length in metres.
    Vine,
    /// A skin of moss or lichen on a trunk or a rock face (flora.rs
    /// `place_crusts`): each `branches` entry is (centre, the way the patch
    /// faces, its radius) and `blobs` is empty.
    Crust,
    /// Shelf fungus standing out from a trunk, held the same way as `Crust`.
    Bracket,
}

#[derive(Clone, Debug)]
pub struct Tree {
    pub pos: Vec3,
    pub kind: TreeKind,
    pub trunk_h: f32,
    pub trunk_r: f32,
    pub blobs: Vec<super::canopy::Blob>,
    pub canopy_lo: f32,
    pub top: f32,
    pub radius: f32,
    pub leaf: u8,
    pub bark: u8,
    pub seed: u32,
    /// Vertical scale of leaf blobs (below 1 flattens crowns).
    pub squash: f32,
    /// Explicit limbs (start, end, radius) for branching trees; when present
    /// they replace the default spokes from the trunk top to each leaf blob.
    pub branches: Vec<(Vec3, Vec3, f32)>,
    /// Index of the library plant this tree grew from; NO_PLANT on the noise island.
    pub plant: u16,
    /// Root flare: extra trunk radius at the ground, metres (big old trees).
    pub flare: f32,
    /// This plant's autumn leaf material, 0 where it has none. A plant with
    /// one is deciduous: it wears it in autumn and loses its leaves in winter
    /// (world/season.rs, `flora::dress`). Everything evergreen has none and
    /// keeps its own leaf all year.
    pub autumn: u8,
    /// Its flower material, 0 where it has none: the blobs wearing it are its
    /// blossom, which is stripped out of its flowering season.
    pub flower: u8,
    /// Limbs thinner than this radius, metres, are sticks: drawn as one thin
    /// line of voxels and meshed as a smooth tube along their own line
    /// (stamp.rs, mesh/lean.rs), whatever the bark. 0 for none.
    pub sticks: f32,
}

pub const NO_PLANT: u16 = u16::MAX;

/// Player edits, keyed by base-level chunk, then by index within the chunk.
pub type EditMap = HashMap<(i32, i32, i32), HashMap<u16, u8>>;

pub struct Generator {
    pub spec: WorldSpec,
    pub world_vox: i32,
    pub height_vox: i32,
    seeds: [u32; 16],
    /// A world built from a chronicle: when present, terrain, materials, trees
    /// and structures come from its fields instead of the noise island.
    pub world: Option<std::sync::Arc<super::fields::World>>,
    /// Plants the EDIT page is showing (src/editor), drawn on top of whatever
    /// the world has there. The generator runs on the streamer's worker
    /// threads, so the list is shared rather than owned, and a change to it is
    /// followed by `Streamer::regen_area` over the ground it covers.
    pub studio: std::sync::Arc<std::sync::RwLock<Vec<Tree>>>,
    /// The EDIT page's studio floor, if it is laid (src/editor/studio.rs).
    pub studio_floor: std::sync::Arc<std::sync::RwLock<Option<StudioFloor>>>,
    /// Which generator the per-thread column-height cache holds heights for
    /// (see `column_h`). A clone shares it, since a clone generates the same
    /// land; a new world gets a new one and the cache is dropped.
    ground_epoch: u64,
    /// Where the world is in the year (world/season.rs `Turn`), packed into
    /// one word. The generator runs on the streamer's worker threads, so the
    /// turn is shared rather than owned, and `Streamer::set_turn` is what
    /// moves it and then has the loaded columns built again.
    turn: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

/// A grey grid platform the EDIT page stands its subject on. It is part of
/// the generated ground rather than a set of voxel edits, for two reasons:
/// nothing of it can end up in somebody's save, and the plant being previewed
/// is drawn after it, so the platform cannot cut the tree in half.
/// How far round the EDIT page's platform the ground is cut back, metres.
/// Inside the eight the studio regenerates beyond its edge (`studio.rs`).
pub const STUDIO_BANK_M: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
pub struct StudioFloor {
    /// The middle, in metres.
    pub x: f32,
    pub z: f32,
    /// Half the platform's side, metres.
    pub half_m: f32,
    /// The top of the platform, metres.
    pub top_m: f32,
    /// The two greys: the field and the grid lines.
    pub pale: u8,
    pub dark: u8,
    /// A block of one material standing on the platform, for looking at the
    /// stuff itself (the EDIT page's materials): its voxel id, 0 for none.
    pub block: u8,
    /// Where it stands, metres: the studio's own spot, not the middle of the
    /// platform, which is where whoever opened the page is standing.
    pub block_x: f32,
    pub block_z: f32,
    /// Half the block's side, and how tall it stands, metres.
    pub block_m: f32,
    pub block_h: f32,
}

impl StudioFloor {
    /// Whether (x, z) is in the block standing on the platform.
    pub fn in_block(&self, x: f32, z: f32) -> bool {
        self.block != 0 && (x - self.block_x).abs() <= self.block_m && (z - self.block_z).abs() <= self.block_m
    }


    /// How far (x, z) is outside the platform's square, metres; 0 on it.
    pub fn outside(&self, x: f32, z: f32) -> f32 {
        let dx = ((x - self.x).abs() - self.half_m).max(0.0);
        let dz = ((z - self.z).abs() - self.half_m).max(0.0);
        dx.hypot(dz)
    }

    /// The grey at (x, z): a line every two metres, a heavier one every ten,
    /// so the size of what stands on it can be read off the floor.
    pub fn mat(&self, x: f32, z: f32) -> u8 {
        let (vx, vz) = ((x / VOXEL).floor() as i32, (z / VOXEL).floor() as i32);
        let line = vx.rem_euclid(8) == 0 || vz.rem_euclid(8) == 0;
        let heavy = vx.rem_euclid(40) < 2 || vz.rem_euclid(40) < 2;
        if line || heavy {
            self.dark
        } else {
            self.pale
        }
    }
}

const S_COAST: usize = 0;
const S_WARP: usize = 1;
const S_HILL: usize = 2;
const S_RIDGE: usize = 3;
const S_RMASK: usize = 4;
const S_CLIFF: usize = 5;
const S_SEACLIFF: usize = 6;
const S_DETAIL: usize = 7;
const S_MOIST: usize = 8;
const S_CARVE: usize = 9;
const S_STRATA: usize = 10;
const S_TREE: usize = 11;
const S_FOREST: usize = 12;
const S_BEACH: usize = 13;
const S_LEAF: usize = 14;

/// Nanoseconds spent in each generator stage, summed over all threads:
/// column samples, tree lookup, voxel fill, tree drawing. For `--profile-gen`.
pub static GEN_NANOS: [std::sync::atomic::AtomicU64; 4] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

fn add_nanos(stage: usize, since: std::time::Instant) {
    GEN_NANOS[stage].fetch_add(since.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
}

/// Per-column surface data used while filling voxels.
#[derive(Clone, Copy, Default)]
struct ColInfo {
    /// Field cell index (chronicle worlds only).
    cell: u32,
    /// The cell whose biome colours the top voxel: `cell` well inside a
    /// biome, a neighbour a few metres off near an edge (`fringe_cell`).
    fringe: u32,
    h: f32,
    amp: f32,
    slope: f32,
    cliff: f32,
    beach: f32,
    strata_off: f32,
    /// A slow noise (about -1 to 1) the strata boundaries ride on when the
    /// world sets `strata_swell`; 0 otherwise.
    swell: f32,
    dirt: f32,
    /// The height above which this column is permanently snow (its snow
    /// line, give or take a little noise).
    snow: f32,
    /// Snow lying on this column because of the season rather than the
    /// height (world/season.rs): the ground takes it whatever it is made of.
    lying: bool,
    /// City street surface over this column (0 none): decided per column so
    /// kerbs are straight.
    street: u8,
    /// Under a volumetric tower: 1 if the column is within one's bounds, 2 if
    /// a tower raised it (then `under` is the ground beneath).
    tower: u8,
    under: f32,
    /// Over a tower raised column holding water (a lake on its top): the
    /// rock stops at the lake bed, the cell's height. Infinite elsewhere.
    cap: f32,
}

/// Buffers a worker thread reuses from job to job, so generation allocates almost
/// nothing. Large allocations on many threads at once contend for the process
/// heap, and the main thread then stalls behind them.
pub struct GenScratch {
    cols: Vec<Column>,
    info: Vec<ColInfo>,
    padded: Box<[u8; PAD3]>,
}

impl Default for GenScratch {
    fn default() -> Self {
        Self {
            cols: Vec::new(),
            info: Vec::new(),
            padded: vec![AIR; PAD3].into_boxed_slice().try_into().unwrap(),
        }
    }
}

#[cfg(test)]
pub struct ColumnGen {
    /// Padded voxel arrays for every chunk that has any surface, by chunk y.
    pub chunks: Vec<(i32, Box<[u8; PAD3]>)>,
    /// Chunks below this y are solid all the way through (no faces).
    pub solid_below: i32,
    /// Number of chunks in the column.
    pub count: i32,
}

/// Slots in the per-thread column-height cache (a power of two). 8192
/// columns is about a 180 m square of ground, which covers the player and
/// everything moving near them.
const GROUND_SLOTS: usize = 8192;

struct GroundCache {
    /// Which generator's heights these are (see `Generator::ground_epoch`).
    epoch: u64,
    keys: Vec<(i32, i32)>,
    vals: Vec<f32>,
}

impl GroundCache {
    fn clear(&mut self, epoch: u64) {
        self.epoch = epoch;
        self.keys.clear();
        self.keys.resize(GROUND_SLOTS, (i32::MIN, i32::MIN));
        self.vals.clear();
        self.vals.resize(GROUND_SLOTS, 0.0);
    }
}

thread_local! {
    static GROUND_CACHE: std::cell::RefCell<GroundCache> = std::cell::RefCell::new(GroundCache {
        epoch: 0,
        keys: Vec::new(),
        vals: Vec::new(),
    });
}

/// Hands out an epoch to each generator made, so a cache filled for one world
/// is thrown away rather than answering for the next.
fn next_ground_epoch() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Generator {
    pub fn new(spec: WorldSpec) -> Self {
        let mut seeds = [0u32; 16];
        for (i, s) in seeds.iter_mut().enumerate() {
            *s = hash32(spec.seed.wrapping_mul(0x9e37_79b9).wrapping_add((i as u32).wrapping_mul(0x85eb_ca6b)));
        }
        Self {
            world_vox: (spec.size_m * VPM) as i32,
            height_vox: (spec.height_m * VPM) as i32,
            spec,
            seeds,
            world: None,
            studio: Default::default(),
            studio_floor: Default::default(),
            ground_epoch: next_ground_epoch(),
            turn: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(super::season::Turn::default().pack())),
        }
    }

    /// Where the world is in the year, as the column being built sees it.
    pub fn turn(&self) -> super::season::Turn {
        super::season::Turn::unpack(self.turn.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Put the world in another part of the year. Columns already built keep
    /// the season they were built in until they are built again, which is
    /// what `Streamer::set_turn` sees to.
    pub fn set_turn(&self, t: super::season::Turn) {
        self.turn.store(t.pack(), std::sync::atomic::Ordering::Relaxed);
    }

    /// A generator for a chronicle-built world.
    pub fn with_world(world: std::sync::Arc<super::fields::World>) -> Self {
        let d = &world.def;
        let spec = WorldSpec {
            seed: d.seed,
            size_m: d.size_m,
            height_m: d.height_m,
            sea_level_m: d.sea_level_m,
            snow_line_m: d.snow_line_m.unwrap_or(72.0),
            ..WorldSpec::default()
        };
        let mut g = Self::new(spec);
        g.world = Some(world);
        g.ground_epoch = next_ground_epoch();
        g
    }

    /// Surface description at (x, z) metres.
    pub fn column(&self, x: f32, z: f32) -> Column {
        if let Some(w) = &self.world {
            let f = &w.f;
            let ripple = if w.city.is_empty() { 1.0 } else { 1.0 - f.occupied_share(x, z) };
            let h = f.height(x, z) + fbm2(x * 0.11, z * 0.11, 2, self.seeds[S_DETAIL]) * 0.12 * ripple;
            return Column {
                h,
                cliff: f.sample(&f.cliff, x, z),
                moist: f.sample(&f.moist, x, z),
            };
        }
        let s = &self.spec;
        let sd = &self.seeds;
        let w = s.size_m;
        let half = w * 0.5;

        // Coastline: an off-centre island, squashed so the sea is nearer on the east.
        let dx = (x - s.island_centre[0] * w) / half;
        let dz = (z - s.island_centre[1] * w) / half;
        let dxs = if dx > 0.0 { dx * s.east_squash } else { dx * 1.08 };
        let warp = fbm2(x * 0.0045, z * 0.0045, 4, sd[S_COAST]) * 0.30 + fbm2(x * 0.02, z * 0.02, 2, sd[S_COAST] + 1) * 0.04;
        let r = (dxs * dxs + dz * dz).sqrt() + warp;
        let soft = smoothstep(0.96, 0.6, r);
        let sea_cliff = smoothstep(0.1, 0.4, fbm2(x * 0.006 + 40.0, z * 0.006, 3, sd[S_SEACLIFF])) * s.cliff_amount.min(1.0);
        let sharp = smoothstep(0.84, 0.81, r);
        let land = soft + (sharp - soft).max(0.0) * sea_cliff;

        // Hills and ridges, domain-warped so they meander.
        let wx = x + 55.0 * fbm2(x * 0.003, z * 0.003, 3, sd[S_WARP]);
        let wz = z + 55.0 * fbm2(x * 0.003 + 100.0, z * 0.003, 3, sd[S_WARP] + 1);
        let hills = (fbm2(wx * 0.0055, wz * 0.0055, 5, sd[S_HILL]) * 0.5 + 0.5).clamp(0.0, 1.0).powf(1.4);
        let ridge_mask = smoothstep(0.38, 0.66, fbm2(x * 0.0035 + 11.0, z * 0.0035 - 7.0, 3, sd[S_RMASK]) * 0.5 + 0.5);
        let ridge = ridged2(wx * 0.008, wz * 0.008, 4, sd[S_RIDGE]);
        let detail = fbm2(x * 0.05, z * 0.05, 3, sd[S_DETAIL]) * 0.7;
        let inland = smoothstep(0.95, 0.42, r);
        let lift = (0.18 + 0.82 * inland) + (1.0 - (0.18 + 0.82 * inland)) * sea_cliff * 0.85;
        let mut land_h = 1.4 + (hills * s.hill_height_m + ridge * ridge_mask * s.ridge_height_m) * lift + detail;

        // Terraced cliffs: flatten into steps with steep risers.
        let cliff = smoothstep(0.44, 0.6, fbm2(x * 0.004 - 50.0, z * 0.004 + 20.0, 3, sd[S_CLIFF]) * 0.5 + 0.5) * s.cliff_amount;
        if cliff > 0.0 && s.terrace_step_m > 0.0 {
            let t = land_h / s.terrace_step_m;
            let f = t - t.floor();
            let terr = (t.floor() + smoothstep(0.68, 0.92, f)) * s.terrace_step_m;
            land_h += (terr - land_h) * cliff;
        }

        let h = s.sea_level_m - 10.0 + land * (10.0 + land_h);
        let moist = fbm2(x * 0.004 + 300.0, z * 0.004, 3, sd[S_MOIST]) * 0.5 + 0.5;
        Column {
            h,
            cliff: cliff.max(sea_cliff * sharp),
            moist,
        }
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        self.column(x, z).h
    }

    fn strata(&self, y: f32, off: f32) -> u8 {
        let band = ((y + off) / 1.1).floor() as i32;
        let r = unit(hash2(band, 17, self.seeds[S_STRATA]));
        if r < 0.55 {
            STONE
        } else if r < 0.8 {
            STONE_DARK
        } else {
            STONE_LIGHT
        }
    }

    // ---- trees -----------------------------------------------------------

    const TREE_CELL: f32 = 5.0;

    fn make_tree(&self, ci: i32, cj: i32) -> Option<Tree> {
        let sd = &self.seeds;
        let s = &self.spec;
        let h0 = hash2(ci, cj, sd[S_TREE]);
        let h1 = hash32(h0 ^ 0x51ed_270b);
        let h2 = hash32(h1 ^ 0x2545_f491);
        let tx = (ci as f32 + 0.15 + 0.7 * unit(h0)) * Self::TREE_CELL;
        let tz = (cj as f32 + 0.15 + 0.7 * unit(h1)) * Self::TREE_CELL;
        if tx < 2.0 || tz < 2.0 || tx > s.size_m - 2.0 || tz > s.size_m - 2.0 {
            return None;
        }
        let forest = fbm2(tx * 0.008 + 5.0, tz * 0.008, 3, sd[S_FOREST]) * 0.5 + 0.5;
        let mut p = (smoothstep(0.42, 0.62, forest) * 0.85 + 0.05) * s.tree_density;
        if unit(h2) > p {
            return None;
        }
        let col = self.column(tx, tz);
        if col.h < s.sea_level_m + 1.6 {
            return None;
        }
        let hx = self.height(tx + 0.75, tz);
        let hz = self.height(tx, tz + 0.75);
        let slope = ((hx - col.h).powi(2) + (hz - col.h).powi(2)).sqrt() / 0.75;
        if slope > 0.55 {
            return None;
        }
        if col.h > s.snow_line_m - 2.0 {
            p *= 0.2;
            if unit(hash32(h2)) > p {
                return None;
            }
        }

        let mut rng = h2;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            unit(rng)
        };
        let elev = col.h - s.sea_level_m;
        let pine_bias = smoothstep(16.0, 40.0, elev) + (1.0 - col.moist) * 0.3;
        let r = rnd();
        let kind = if r < pine_bias * 0.8 {
            TreeKind::Pine
        } else if r < pine_bias * 0.8 + 0.18 {
            TreeKind::Birch
        } else {
            TreeKind::Broad
        };
        let base = Vec3::new(tx, col.h - 0.1, tz);
        let seed = hash32(h0 ^ 0x1234_5678);
        let tree = match kind {
            TreeKind::Vine | TreeKind::Crust | TreeKind::Bracket => return None,
            TreeKind::Broad => {
                let trunk_h = 2.6 + 2.0 * rnd();
                let trunk_r = 0.24 + 0.14 * rnd();
                let top = base + Vec3::new(0.0, trunk_h + 1.1, 0.0);
                let main_r = 2.0 + 1.0 * rnd();
                let mut blobs = vec![Blob::ball(top, main_r)];
                let n = 3 + (rnd() * 2.0) as usize;
                let a0 = rnd() * std::f32::consts::TAU;
                for i in 0..n {
                    let a = a0 + i as f32 * std::f32::consts::TAU / n as f32 + rnd() * 0.6;
                    let d = 1.3 + 0.9 * rnd();
                    let c = top + Vec3::new(a.cos() * d, -0.6 + 1.3 * rnd(), a.sin() * d);
                    blobs.push(Blob::ball(c, 1.3 + 0.8 * rnd()));
                }
                let radius = blobs.iter().map(|b| b.reach(base, 1.0, 1.35).0).fold(0.0, f32::max);
                let top_y = blobs.iter().map(|b| b.reach(base, 1.0, 1.35).1).fold(0.0, f32::max);
                let leaf = if rnd() < 0.16 { LEAVES_AUTUMN } else { LEAVES };
                Tree {
                    flare: 0.0,
                    autumn: 0,
                    flower: 0,
                    sticks: 0.0,
                    pos: base,
                    kind,
                    trunk_h,
                    trunk_r,
                    blobs,
                    canopy_lo: trunk_h - 1.5,
                    top: top_y,
                    radius,
                    leaf,
                    bark: BARK,
                    seed,
                    squash: 1.0,
                    branches: Vec::new(),
                    plant: NO_PLANT,
                }
            }
            TreeKind::Birch => {
                let trunk_h = 4.0 + 2.2 * rnd();
                let trunk_r = 0.15 + 0.05 * rnd();
                let mut blobs = Vec::new();
                for i in 0..3 {
                    let y = trunk_h - 1.6 + i as f32 * 1.1;
                    let off = Vec3::new(rnd() - 0.5, 0.0, rnd() - 0.5) * 0.8;
                    blobs.push(Blob::ball(base + Vec3::new(0.0, y, 0.0) + off, 1.5 - i as f32 * 0.25 + rnd() * 0.3));
                }
                let top_y = blobs.iter().map(|b| b.reach(base, 1.0, 1.35).1).fold(0.0, f32::max);
                Tree {
                    flare: 0.0,
                    autumn: 0,
                    flower: 0,
                    sticks: 0.0,
                    pos: base,
                    kind,
                    trunk_h,
                    trunk_r,
                    blobs,
                    canopy_lo: trunk_h - 3.5,
                    top: top_y,
                    radius: 2.9,
                    leaf: BIRCH_LEAVES,
                    bark: BIRCH_BARK,
                    seed,
                    squash: 1.0,
                    branches: Vec::new(),
                    plant: NO_PLANT,
                }
            }
            TreeKind::Pine => {
                let height = 7.0 + 4.5 * rnd();
                let trunk_r = 0.22 + 0.1 * rnd();
                let rmax = 1.8 + 0.9 * rnd();
                let canopy_lo = 1.6 + rnd() * 0.8;
                // The same whorled crown the library conifers get (flora.rs).
                let (mut blobs, mut branches) = (Vec::new(), Vec::new());
                let trunk_h = crate::world::flora::conifer_crown(
                    &mut rnd,
                    base,
                    height,
                    trunk_r,
                    rmax,
                    canopy_lo,
                    crate::world::flora::CONIFER_TAPER,
                    crate::world::flora::Sweep::Level,
                    None,
                    &mut blobs,
                    &mut branches,
                );
                let radius = blobs.iter().map(|b| b.reach(base, 1.0, 1.35).0).fold(1.0, f32::max);
                let top = blobs.iter().map(|b| b.reach(base, 1.0, 1.35).1).fold(trunk_h, f32::max);
                Tree {
                    flare: 0.0,
                    autumn: 0,
                    flower: 0,
                    sticks: 0.0,
                    pos: base,
                    kind,
                    trunk_h,
                    trunk_r,
                    blobs,
                    canopy_lo,
                    top,
                    radius,
                    leaf: PINE,
                    bark: BARK,
                    seed,
                    squash: 1.0,
                    branches,
                    plant: NO_PLANT,
                }
            }
        };
        Some(tree)
    }

    /// Trees whose bounds may touch the box [x0, x1] x [z0, z1] metres.
    /// Every plant standing in this box, dressed for the part of the year the
    /// world is in (`flora::dress`). The plants themselves are grown once, in
    /// summer leaf, when the world is built; the year is put on here, so a
    /// column carries whatever season it was last built in and a turn of the
    /// year is a rebuild.
    pub fn trees_near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<Tree> {
        let mut out = self.trees_grown(x0, z0, x1, z1);
        let turn = self.turn();
        if let Some(w) = &self.world {
            for t in out.iter_mut() {
                super::flora::dress(t, &w.lib, turn);
            }
        }
        out
    }

    /// The plants near column `key` whose stems are thin tubes (canes,
    /// spurs: a `tube` bark), for `threads`.
    pub fn tube_trees(&self, key: ColKey) -> Vec<Tree> {
        let step = 1i32 << key.lod;
        let (x0, z0) = (key.x * CHUNK * step, key.z * CHUNK * step);
        let pad = |v: i32, k: i32| (v + k * step) as f32 * VOXEL;
        let mut out = self.trees_near(pad(x0, -(M as i32)), pad(z0, -(M as i32)), pad(x0, CHUNK + M as i32), pad(z0, CHUNK + M as i32));
        out.retain(|t| !t.branches.is_empty() && matches!(t.kind, TreeKind::Broad | TreeKind::Birch | TreeKind::Pine) && (matlib::tubes(t.bark) || t.sticks > 0.0));
        out
    }

    /// The threads (mesh/lean.rs) of chunk `cy` of column `key`: each leaning
    /// limb of `trees` (from `tube_trees`) that passes near it, in the padded
    /// chunk's voxel coordinates. The hull is the radius stamp.rs draws a
    /// tube limb to, so the thread covers exactly the voxels of its stem.
    pub fn threads(trees: &[Tree], key: ColKey, cy: i32) -> Vec<crate::mesh::Thread> {
        let step = 1i32 << key.lod;
        let vsize = VOXEL * step as f32;
        let base = Vec3::new((key.x * CHUNK - M as i32) as f32, (cy * CHUNK - MY as i32) as f32, (key.z * CHUNK - M as i32) as f32);
        let to = |p: Vec3| p / vsize - base;
        let (lo, hi) = (Vec3::splat(-1.0), Vec3::new(PAD as f32 + 1.0, PADY as f32 + 1.0, PAD as f32 + 1.0));
        let mut out = Vec::new();
        for t in trees {
            // A tube-barked plant's limbs are all threads, of any tube
            // material; another plant's sticks (limbs under `sticks`) are
            // threads of its bark, which its trunk is made of too (`lone`).
            let tube = matlib::tubes(t.bark);
            // A deciduous tree in leaf hides its twigs: their thin line of
            // voxels stays, but the tubes wait for the leaves to fall.
            let hidden = !tube && t.autumn != AIR && t.leaf != AIR;
            for &(a, b, r) in &t.branches {
                if !tube && (hidden || r >= t.sticks) {
                    continue;
                }
                let (pa, pb) = (to(a), to(b));
                let hull = r.max(0.62 * vsize) / vsize;
                let rv = r / vsize;
                if rv > crate::mesh::THREAD_MAX_R || pa.max(pb).cmplt(lo - hull).any() || pa.min(pb).cmpgt(hi + hull).any() {
                    continue;
                }
                // Straight up or along an axis: the voxel faces' own bevel
                // already draws it round.
                let d = (pb - pa).normalize_or_zero();
                if d.abs().max_element() > 0.995 {
                    continue;
                }
                out.push(crate::mesh::Thread { a: pa, b: pb, r: rv, hull, mat: if tube { 0 } else { t.bark }, lone: !tube });
            }
        }
        out
    }

    /// All the threads of chunk `cy` of column `key`: thin plant stems (from
    /// `tube_trees`) and thin route rails.
    pub fn chunk_threads(&self, trees: &[Tree], key: ColKey, cy: i32) -> Vec<crate::mesh::Thread> {
        let mut out = Self::threads(trees, key, cy);
        self.rail_threads(key, cy, &mut out);
        self.structure_threads(key, cy, &mut out);
        out
    }

    /// The `segment` pieces of the structures placed near chunk `cy` of
    /// column `key` (rigging, spokes, braces), turned and moved as the
    /// structure is (`draw_structures`), as threads.
    fn structure_threads(&self, key: ColKey, cy: i32, out: &mut Vec<crate::mesh::Thread>) {
        let Some(w) = &self.world else { return };
        let step = 1i32 << key.lod;
        let vsize = VOXEL * step as f32;
        let base = Vec3::new((key.x * CHUNK - M as i32) as f32, (cy * CHUNK - MY as i32) as f32, (key.z * CHUNK - M as i32) as f32);
        let (x0, z0) = ((base.x - 1.0) * vsize, (base.z - 1.0) * vsize);
        let (x1, z1) = ((base.x + PAD as f32 + 1.0) * vsize, (base.z + PAD as f32 + 1.0) * vsize);
        for k in w.placed_near(x0, z0, x1, z1) {
            if w.shown.get(k as usize).is_some_and(|s| !s.load(std::sync::atomic::Ordering::Relaxed)) {
                continue;
            }
            let p = &w.placed[k as usize];
            let s = &w.shapes[p.shape];
            if s.threads.is_empty() {
                continue;
            }
            let (sx, sz) = (s.size[0] as f32, s.size[2] as f32);
            // Local voxel units to the chunk's padded voxels: the quarter
            // turn of `shape::rotate`, taken on continuous coordinates.
            let to = |q: [f32; 3]| {
                let (rx, rz) = match p.rot & 3 {
                    0 => (q[0], q[2]),
                    1 => (sz - q[2], q[0]),
                    2 => (sx - q[0], sz - q[2]),
                    _ => (q[2], sx - q[0]),
                };
                let world = Vec3::new(p.origin[0] as f32 + rx, p.origin[1] as f32 + q[1], p.origin[2] as f32 + rz) / VPM;
                world / vsize - base
            };
            let scale = 1.0 / step as f32;
            for t in &s.threads {
                let (a, b) = (to(t.a), to(t.b));
                let hull = t.hull * scale + if step > 1 { 0.5 } else { 0.0 };
                let lo = a.min(b) - Vec3::splat(hull);
                let hi = a.max(b) + Vec3::splat(hull);
                if hi.x < 0.0 || hi.y < 0.0 || hi.z < 0.0 || lo.x > PAD as f32 || lo.y > PADY as f32 || lo.z > PAD as f32 {
                    continue;
                }
                let r = t.r * scale;
                let d = (b - a).normalize_or_zero();
                if r > crate::mesh::THREAD_MAX_R || d.abs().max_element() > 0.9999 {
                    continue;
                }
                out.push(crate::mesh::Thread { a, b, r, hull, mat: t.mat, lone: false });
            }
        }
    }

    /// The thin rails of the routes passing chunk `cy` of column `key` (wire
    /// fences, rope handlines, guard rails: rails no thicker than THIN_RAIL
    /// either way), as threads along their centrelines. A rail that follows
    /// the ground is sampled every half metre.
    fn rail_threads(&self, key: ColKey, cy: i32, out: &mut Vec<crate::mesh::Thread>) {
        use super::route::kit::{Ground, Side};
        const THIN_RAIL: f32 = 0.4;
        let Some(w) = &self.world else { return };
        if w.ribbons.is_empty() {
            return;
        }
        let step = 1i32 << key.lod;
        let vsize = VOXEL * step as f32;
        let base = Vec3::new((key.x * CHUNK - M as i32) as f32, (cy * CHUNK - MY as i32) as f32, (key.z * CHUNK - M as i32) as f32);
        let to = |p: Vec3| p / vsize - base;
        let (x0, z0) = ((base.x - 1.0) * vsize, (base.z - 1.0) * vsize);
        let (x1, z1) = ((base.x + PAD as f32 + 1.0) * vsize, (base.z + PAD as f32 + 1.0) * vsize);
        let (y0, y1) = ((base.y - 1.0) * vsize, (base.y + PADY as f32 + 1.0) * vsize);
        for id in w.ribbons.near(x0, z0, x1, z1) {
            let Some(s) = w.ribbons.segs.get(id as usize) else { continue };
            let kit = &w.ribbons.kits[s.kit as usize];
            let follow = kit.ground == Ground::Follow;
            if !follow && (s.a.y.min(s.b.y) + kit.rise + 1.0 < y0 || s.a.y.max(s.b.y) - kit.drop - 1.0 > y1) {
                continue;
            }
            let (a2, b2) = (glam::Vec2::new(s.a.x, s.a.z), glam::Vec2::new(s.b.x, s.b.z));
            let len = a2.distance(b2);
            if len < 1e-3 {
                continue;
            }
            let dir = (b2 - a2) / len;
            let left = glam::Vec2::new(dir.y, -dir.x);
            for r in &kit.rails {
                let tall = r.to - r.from;
                if r.mat == 0 || r.thick > THIN_RAIL || tall > THIN_RAIL {
                    continue;
                }
                // Where the rail sits across the line (kit.rs `band`).
                let mid = (kit.half + r.out).max(0.0) + r.thick * 0.5;
                let sides: &[f32] = match r.side {
                    Side::Centre => &[0.0],
                    Side::Left => &[1.0],
                    Side::Right => &[-1.0],
                    Side::Both => &[1.0, -1.0],
                };
                let up = (r.from + r.to) * 0.5;
                let rv = (r.thick.min(tall) * 0.5) / vsize;
                let hull = (r.thick.hypot(tall) * 0.5) / vsize + 0.75;
                for &sg in sides {
                    let off = left * sg * mid;
                    let n = if follow { (len / 0.5).ceil().max(1.0) as usize } else { 1 };
                    let at = |k: usize| {
                        let t = k as f32 / n as f32;
                        let p = a2 + (b2 - a2) * t + off;
                        let y = if follow { self.column(p.x, p.y).h } else { s.a.y + (s.b.y - s.a.y) * t };
                        Vec3::new(p.x, y + up, p.y)
                    };
                    let mut prev = to(at(0));
                    for k in 1..=n {
                        let next = to(at(k));
                        let lo = prev.min(next) - Vec3::splat(hull);
                        let hi = prev.max(next) + Vec3::splat(hull);
                        let d = (next - prev).normalize_or_zero();
                        let inside = hi.x >= 0.0 && hi.y >= 0.0 && hi.z >= 0.0 && lo.x <= PAD as f32 && lo.y <= PADY as f32 && lo.z <= PAD as f32;
                        // Dead straight along an axis: its voxels are already a
                        // clean bar. Anything off it, however slightly, steps.
                        if inside && d.abs().max_element() <= 0.9999 {
                            out.push(crate::mesh::Thread { a: prev, b: next, r: rv, hull, mat: r.mat, lone: false });
                        }
                        prev = next;
                    }
                }
            }
        }
    }

    fn trees_grown(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<Tree> {
        // The EDIT page's plant, if it stands in this box, drawn after the
        // world's own so it is never hidden by them.
        let studio = |mut out: Vec<Tree>| -> Vec<Tree> {
            if let Some(f) = *self.studio_floor.read().unwrap() {
                // Off the cut bank as well: its ground is gone from under it.
                out.retain(|t| f.outside(t.pos.x, t.pos.z) > STUDIO_BANK_M);
            }
            let list = self.studio.read().unwrap();
            if !list.is_empty() {
                out.extend(
                    list.iter()
                        .filter(|t| t.pos.x + t.radius >= x0 && t.pos.x - t.radius <= x1 && t.pos.z + t.radius >= z0 && t.pos.z - t.radius <= z1)
                        .cloned(),
                );
            }
            out
        };
        if let Some(w) = &self.world {
            return studio(w.trees_near(x0, z0, x1, z1));
        }
        let c = Self::TREE_CELL;
        let mut out = Vec::new();
        for cj in ((z0 / c).floor() as i32 - 1)..=((z1 / c).floor() as i32 + 1) {
            for ci in ((x0 / c).floor() as i32 - 1)..=((x1 / c).floor() as i32 + 1) {
                if let Some(t) = self.make_tree(ci, cj) {
                    if t.pos.x + t.radius >= x0 && t.pos.x - t.radius <= x1 && t.pos.z + t.radius >= z0 && t.pos.z - t.radius <= z1 {
                        out.push(t);
                    }
                }
            }
        }
        studio(out)
    }

    // ---- chunks ----------------------------------------------------------

    /// One column's height in voxels, through the cache: a column is a noise
    /// evaluation, and the drawn ground's lift (physics::drawn_ground_lift)
    /// asks for up to sixteen of them per body per step, almost all of them
    /// the same ones as the step before. Walking bodies hit about nine in
    /// ten of these.
    fn column_h(&self, x: i32, z: i32) -> f32 {
        GROUND_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.epoch != self.ground_epoch {
                c.clear(self.ground_epoch);
            }
            // Direct-mapped: one slot per column, the loser evicted. Two
            // columns a body straddles never collide, since the hash mixes
            // both axes.
            let h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841);
            let i = (h >> 13) as usize & (GROUND_SLOTS - 1);
            if c.keys[i] == (x, z) {
                return c.vals[i];
            }
            let v = self.column((x as f32 + 0.5) * VOXEL, (z as f32 + 0.5) * VOXEL).h;
            c.keys[i] = (x, z);
            c.vals[i] = v;
            v
        })
    }

    /// Generated ground height of the full-detail voxel column (x, z), in
    /// voxels: the coarse samples every second voxel and the interpolation
    /// between them exactly as `gen_column_with` makes them at lod 0.
    /// The lowest voxel row at this column that may be dug out, as a voxel
    /// height. Below the generated ground the world keeps no detail (see
    /// `fields::Layer`), so a hole into it is a hole out of the world, and
    /// nothing more than `BURIED_LIMIT` under the surface may go.
    ///
    /// A structure sunk into the ground is the exception, and an important
    /// one: the whole of an imported arena stands thirty metres below the
    /// plain it was dropped into, so the plain's rule would refuse every
    /// voxel of it. Those voxels were placed rather than generated, and the
    /// placement knows exactly how far down it goes, so inside a structure's
    /// own footprint the floor is the bottom of the structure.
    pub fn dig_floor_vox(&self, x: i32, z: i32) -> f32 {
        let mut floor = self.ground_vox(x, z) - super::fields::BURIED_LIMIT * VPM;
        let Some(w) = &self.world else { return floor };
        let (mx, mz) = (x as f32 / VPM, z as f32 / VPM);
        for k in w.placed_near(mx, mz, mx, mz) {
            let p = &w.placed[k as usize];
            let (x0, z0, x1, z1) = p.footprint(&w.shapes);
            if x >= x0 && x < x1 && z >= z0 && z < z1 {
                floor = floor.min((p.origin[1] - w.lib.structures[p.def].foundation) as f32);
            }
        }
        floor
    }

    /// The water surface over (x, z) in metres: a river's or a lake's where
    /// there is one, else the sea. What a floating sprite lies on, and the
    /// same rule `game::water_over` gives a body.
    pub fn water_m(&self, x: f32, z: f32) -> f32 {
        let sea = self.spec.sea_level_m;
        match self.world.as_ref().and_then(|w| w.f.wet(x, z)) {
            Some(wet) if wet.level > sea => wet.level,
            _ => sea,
        }
    }

    pub fn ground_vox(&self, x: i32, z: i32) -> f32 {
        let s = |x: i32, z: i32| self.column_h(x, z);
        let h = match (x & 1 == 0, z & 1 == 0) {
            (true, true) => s(x, z),
            (false, true) => 0.5 * (s(x - 1, z) + s(x + 1, z)),
            (true, false) => 0.5 * (s(x, z - 1) + s(x, z + 1)),
            (false, false) => 0.25 * (s(x - 1, z - 1) + s(x + 1, z - 1) + s(x - 1, z + 1) + s(x + 1, z + 1)),
        };
        h * VPM
    }

    /// Generate every chunk of a column that has any surface. Chunks above the
    /// returned set are air; chunks below `solid_below` are solid with no faces.
    #[cfg(test)]
    pub fn gen_column(&self, key: ColKey, edits: Option<&EditMap>) -> ColumnGen {
        let mut scratch = GenScratch::default();
        let mut chunks = Vec::new();
        let (solid_below, count) = self.gen_column_with(key, edits, &mut scratch, |cy, v, _| chunks.push((cy, Box::new(*v))));
        ColumnGen { chunks, solid_below, count }
    }

    /// Like `gen_column`, but hands each chunk's padded voxels to `emit` from a
    /// reused buffer. Returns (solid_below, chunk count).
    pub fn gen_column_with(&self, key: ColKey, edits: Option<&EditMap>, scratch: &mut GenScratch, mut emit: impl FnMut(i32, &[u8; PAD3], &[f32])) -> (i32, i32) {
        let GenScratch { cols, info, padded } = scratch;
        let step = 1i32 << key.lod;
        let vsize = VOXEL * step as f32;
        let x0 = key.x * CHUNK * step;
        let z0 = key.z * CHUNK * step;
        let count = ((self.height_vox + CHUNK * step - 1) / (CHUNK * step)).max(1);

        let t_cols = std::time::Instant::now();
        // Column samples on a 36x36 grid (padded grid plus one ring for slopes).
        const G: usize = PAD + 2;
        cols.clear();
        cols.resize(G * G, Column::default());
        // Grid index q holds the column at voxel offset q - M - 1 from the chunk.
        let sx = |q: i32, o: i32| ((o + (q - M as i32 - 1) * step) as f32 + step as f32 * 0.5) * VOXEL;
        if step == 1 {
            // Terrain features are metres across, so at full detail sample every
            // second voxel and interpolate. The coarse grid sits on even voxel
            // offsets, so neighbouring chunks agree along their shared padding.
            // Coarse sample c sits at grid index 2c - PAR, an even voxel offset.
            const PAR: usize = (M + 1) % 2;
            const H: usize = G / 2 + 2;
            let mut coarse = [Column::default(); H * H];
            for cz in 0..H {
                for cx in 0..H {
                    coarse[cz * H + cx] = self.column(sx((cx * 2) as i32 - PAR as i32, x0), sx((cz * 2) as i32 - PAR as i32, z0));
                }
            }
            let lerp = |a: Column, b: Column, t: f32| Column {
                h: a.h + (b.h - a.h) * t,
                cliff: a.cliff + (b.cliff - a.cliff) * t,
                moist: a.moist + (b.moist - a.moist) * t,
            };
            for qz in 0..G {
                let (cz, tz) = ((qz + PAR) / 2, ((qz + PAR) % 2) as f32 * 0.5);
                for qx in 0..G {
                    let (cx, tx) = ((qx + PAR) / 2, ((qx + PAR) % 2) as f32 * 0.5);
                    let at = |x: usize, z: usize| coarse[z.min(H - 1) * H + x.min(H - 1)];
                    let a = lerp(at(cx, cz), at(cx + 1, cz), tx);
                    let b = lerp(at(cx, cz + 1), at(cx + 1, cz + 1), tx);
                    cols[qz * G + qx] = lerp(a, b, tz);
                }
            }
        } else {
            for qz in 0..G {
                for qx in 0..G {
                    cols[qz * G + qx] = self.column(sx(qx as i32, x0), sx(qz as i32, z0));
                }
            }
        }
        let sd = &self.seeds;
        // Snow lying on the ground because of the season rather than the
        // height (world/season.rs): a covering lies in patches on anything
        // that is not steep, and a deep fall lies on everything short of a
        // rock face. The permanent snow line is left as it is and the rules
        // below it decide the rest, so beaches stay sand and cliffs stay rock.
        let lying = self.turn().snow;
        info.clear();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        // Volumetric towers whose bounds reach this chunk's columns.
        let towers: Vec<&super::fields::Tower> = {
            let (ax0, az0) = ((x0 - M as i32 * step) as f32 * VOXEL - 1.0, (z0 - M as i32 * step) as f32 * VOXEL - 1.0);
            let (ax1, az1) = ((x0 + (CHUNK + M as i32) * step) as f32 * VOXEL + 1.0, (z0 + (CHUNK + M as i32) * step) as f32 * VOXEL + 1.0);
            self.world
                .as_ref()
                .map(|w| {
                    w.towers
                        .iter()
                        .filter(|t| (t.x0 as f32) <= ax1 && ((t.x0 + t.w) as f32) >= ax0 && (t.z0 as f32) <= az1 && ((t.z0 + t.w) as f32) >= az0)
                        .collect()
                })
                .unwrap_or_default()
        };
        for pz in 0..PAD {
            for px in 0..PAD {
                let c = cols[(pz + 1) * G + px + 1];
                let gx = (cols[(pz + 1) * G + px + 2].h - cols[(pz + 1) * G + px].h) / (2.0 * vsize);
                let gz = (cols[(pz + 2) * G + px + 1].h - cols[pz * G + px + 1].h) / (2.0 * vsize);
                let slope = (gx * gx + gz * gz).sqrt();
                let amp = c.cliff * 1.8 * smoothstep(0.6, 1.5, slope);
                let (xm, zm) = (sx(px as i32 + 1, x0), sx(pz as i32 + 1, z0));
                let mut tower = 0u8;
                let mut under = f32::NAN;
                for t in &towers {
                    if xm >= t.x0 as f32 && xm < (t.x0 + t.w) as f32 && zm >= t.z0 as f32 && zm < (t.z0 + t.w) as f32 {
                        tower = tower.max(1);
                        if let Some(u) = t.under_at(xm, zm) {
                            tower = 2;
                            under = if under.is_nan() { u } else { under.min(u) };
                        }
                    }
                }
                // Near a tower the smoothed height ramps between its top and
                // the ground beside it: take the cell's own height instead.
                let (c, amp) = match (tower, self.world.as_ref()) {
                    (1.., Some(w)) => {
                        let ch = w.f.h[w.f.idx(xm, zm)];
                        if tower == 2 || (c.h - ch).abs() > 0.75 {
                            (Column { h: ch, ..c }, 0.0)
                        } else {
                            (c, amp)
                        }
                    }
                    _ => (c, amp),
                };
                lo = lo.min(c.h - amp);
                if tower == 2 {
                    lo = lo.min(under - 1.0);
                }
                if tower > 0 {
                    lo = lo.min(towers.iter().map(|t| t.base).fold(f32::MAX, f32::min));
                }
                hi = hi.max(c.h + amp);
                info.push(ColInfo {
                    cell: self.world.as_ref().map_or(0, |w| w.f.idx(xm, zm) as u32),
                    fringe: self.world.as_ref().map_or(0, |w| fringe_cell(w, xm, zm, sd[S_BEACH])),
                    h: c.h,
                    amp,
                    slope,
                    cliff: c.cliff,
                    beach: 0.9 + 0.6 * perlin2(xm * 0.08, zm * 0.08, sd[S_BEACH]),
                    strata_off: 1.4 * perlin2(xm * 0.03, zm * 0.03, sd[S_STRATA]),
                    swell: if self.world.as_ref().is_some_and(|w| w.def.strata_swell > 0.0) {
                        fbm2(xm * 0.005, zm * 0.005, 2, sd[S_STRATA] + 11) * 1.4 + perlin2(xm * 0.02, zm * 0.02, sd[S_STRATA] + 12) * 0.35
                    } else {
                        0.0
                    },
                    dirt: 1.0 + 0.5 * perlin2(xm * 0.11, zm * 0.11, sd[S_BEACH] + 3),
                    snow: self.spec.snow_line_m + 3.0 * perlin2(xm * 0.05, zm * 0.05, sd[S_BEACH] + 4),
                    lying: match lying {
                        0 => false,
                        // A covering lies in patches on anything that is not
                        // steep; a deep fall lies on everything short of a
                        // rock face.
                        1 => slope < 0.75 && perlin2(xm * 0.07, zm * 0.07, sd[S_BEACH] + 9) > -0.12,
                        _ => slope < 1.3,
                    },
                    street: self.world.as_ref().map_or(0, |w| w.city.street_at(xm, zm)),
                    tower,
                    under,
                    cap: match self.world.as_ref() {
                        Some(w) if tower == 2 && !w.f.inland.is_empty() && w.f.inland.contains_key(&(w.f.idx(xm, zm) as u32)) => c.h,
                        _ => f32::INFINITY,
                    },
                });
            }
        }

        let pad_x0 = (x0 - M as i32 * step) as f32 * VOXEL;
        let pad_z0 = (z0 - M as i32 * step) as f32 * VOXEL;
        let pad_x1 = (x0 + (CHUNK + M as i32) * step) as f32 * VOXEL;
        let pad_z1 = (z0 + (CHUNK + M as i32) * step) as f32 * VOXEL;
        add_nanos(0, t_cols);
        let t_trees = std::time::Instant::now();
        let trees = self.trees_near(pad_x0, pad_z0, pad_x1, pad_z1);
        add_nanos(1, t_trees);
        for t in &trees {
            hi = hi.max(t.pos.y + t.top);
        }
        let structs = self.world.as_ref().map(|w| w.placed_near(pad_x0, pad_z0, pad_x1, pad_z1)).unwrap_or_default();
        if let Some(w) = &self.world {
            for &k in &structs {
                let p = &w.placed[k as usize];
                let sh = &w.shapes[p.shape];
                let found = w.lib.structures[p.def].foundation;
                hi = hi.max((p.origin[1] + sh.top) as f32 * VOXEL);
                lo = lo.min((p.origin[1] - found) as f32 * VOXEL);
            }
        }
        let city_items = self.world.as_ref().map(|w| w.city.near(pad_x0, pad_z0, pad_x1, pad_z1)).unwrap_or_default();
        if let (Some(w), false) = (&self.world, city_items.is_empty()) {
            let (clo, chi) = w.city.y_range(&city_items);
            lo = lo.min(clo);
            hi = hi.max(chi);
        }
        let route_items = self.world.as_ref().map(|w| w.ribbons.near(pad_x0, pad_z0, pad_x1, pad_z1)).unwrap_or_default();
        if let (Some(w), false) = (&self.world, route_items.is_empty()) {
            let (rlo, rhi, follow) = w.ribbons.y_range(&route_items);
            lo = lo.min(rlo);
            hi = hi.max(rhi).max(hi + follow);
        }
        // Skeletons (event `skeleton`): bones standing over or buried under
        // these columns widen the chunks that must be built.
        let bone_items = match &self.world {
            Some(w) if !w.bones.is_empty() => {
                let (list, blo, bhi) = w.bones.near(pad_x0, pad_z0, pad_x1, pad_z1);
                if !list.is_empty() {
                    lo = lo.min(blo);
                    hi = hi.max(bhi);
                }
                list
            }
            _ => Vec::new(),
        };
        // Tunnels under this column: the chunks they pass through must be built.
        let tubes: Vec<super::fields::Tube> = self
            .world
            .as_ref()
            .map(|w| {
                w.tubes
                    .iter()
                    .filter(|t| {
                        let r = t.r * 1.2 + 1.0;
                        t.a.x.min(t.b.x) - r <= pad_x1 && t.a.x.max(t.b.x) + r >= pad_x0 && t.a.z.min(t.b.z) - r <= pad_z1 && t.a.z.max(t.b.z) + r >= pad_z0
                    })
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        for t in &tubes {
            lo = lo.min(t.a.y.min(t.b.y) - t.r - 0.5);
        }

        // Chunks holding an edit, or next to one (their padding reads it), are
        // built even where the column would otherwise skip them as all rock or
        // all air; otherwise a deep tunnel or a block placed above the terrain
        // is lost whenever the column is generated again.
        let mut edited = Vec::new();
        if let (0, Some(edits)) = (key.lod, edits) {
            for (&(ex, ey, ez), list) in edits.iter() {
                if (ex - key.x).abs() <= 1 && (ez - key.z).abs() <= 1 && !list.is_empty() {
                    for cy in ey - 1..=ey + 1 {
                        if (0..count).contains(&cy) {
                            if edited.len() <= cy as usize {
                                edited.resize(cy as usize + 1, false);
                            }
                            edited[cy as usize] = true;
                        }
                    }
                }
            }
        }
        let is_edited = |cy: i32| edited.get(cy as usize).copied().unwrap_or(false);

        let sea = self.spec.sea_level_m;
        let mut solid_below = 0;
        for cy in 0..count {
            let bottom_c = ((cy * CHUNK - MY as i32) * step) as f32 * VOXEL + vsize * 0.5;
            let top_c = ((cy * CHUNK + PADY as i32 - 1 - MY as i32) * step) as f32 * VOXEL + vsize * 0.5;
            if top_c < lo - 0.01 {
                solid_below = cy + 1;
                if !is_edited(cy) {
                    continue;
                }
            }
            if bottom_c > hi + 0.01 {
                if cy as usize >= edited.len() {
                    break;
                }
                if !is_edited(cy) {
                    continue;
                }
            }
            let t_fill = std::time::Instant::now();
            let v: &mut [u8; PAD3] = padded;
            v.fill(AIR);
            let mut any_solid = false;
            for pz in 0..PAD {
                let zm = sx(pz as i32 + 1, z0);
                for px in 0..PAD {
                    let xm = sx(px as i32 + 1, x0);
                    let c = &info[pz * PAD + px];
                    for py in 0..PADY {
                        let ym = ((cy * CHUNK + py as i32 - MY as i32) * step) as f32 * VOXEL + vsize * 0.5;
                        let depth = c.h - ym;
                        let in_tower = || towers.iter().any(|t| t.inside(Vec3::new(xm, ym, zm)));
                        let solid = if c.tower == 2 {
                            ym < c.under || (ym < c.cap && in_tower())
                        } else if depth > c.amp + 0.01 {
                            true
                        } else if depth < -c.amp - 0.01 {
                            false
                        } else {
                            depth + c.amp * perlin3(xm * 0.11, ym * 0.3, zm * 0.11, sd[S_CARVE]) > 0.0
                        } || (c.tower == 1 && in_tower());
                        if !solid {
                            continue;
                        }
                        any_solid = true;
                        // A tower is rock all over except the top metre of its crown.
                        // A tower is rock except for the turf on its crown: a
                        // couple of metres of soil, and over the shoulders of
                        // a dome or a knob, not only the flat part. Rain
                        // leaves karst tops green, and a bare white cap reads
                        // as a quarry.
                        let rocky = c.slope > 1.35 || (c.cliff > 0.35 && c.slope > 0.8) || depth < -0.01 || (c.tower == 2 && (depth > 2.0 || c.h < sea + 6.0 || c.slope > 1.2));
                        if let Some(w) = &self.world {
                            let mut m = field_material(w, c, ym, depth, vsize, rocky, sd[S_STRATA]);
                            // A tower's own rock and mantle.
                            if c.tower > 0 {
                                let p = Vec3::new(xm, ym, zm);
                                if let Some((t, margin)) = towers.iter().find_map(|t| t.margin(p).map(|g| (*t, g))) {
                                    if t.rock != 0 && rocky {
                                        m = t.rock;
                                    }
                                    if rocky && margin < 0.8 && (t.moss != 0 || t.soil != 0) {
                                        // Open above: the top of a ledge or bulge.
                                        let up = !t.inside(p + Vec3::Y * 0.5) && !t.inside(p + Vec3::Y * 1.0);
                                        let ss = sd[S_STRATA];
                                        if up && t.soil != 0 {
                                            // Anything with the sky over it
                                            // grows moss far more often than
                                            // it stays bare soil.
                                            m = if t.moss != 0 && perlin3(xm * 0.5, ym * 0.5, zm * 0.5, ss + 9) > -0.4 { t.moss } else { t.soil };
                                        } else if t.moss != 0 {
                                            // Streaks: long up and down, narrow across, and
                                            // more of them just below an overhang.
                                            let under = t.inside(p + Vec3::Y * 2.5) && !t.inside(p + glam::Vec3::new(0.0, 2.5, 0.0) + (p - Vec3::new(t.c.x, p.y, t.c.y)).normalize_or_zero() * 0.6);
                                            let n = (perlin3(xm * 0.08, ym * 0.05, zm * 0.08, ss + 18) * 0.55 + perlin3(xm * 0.5, ym * 0.09, zm * 0.5, ss + 8) * 0.45) * 0.5
                                                + 0.5
                                                + if under { 0.2 } else { 0.0 };
                                            if n > 1.0 - t.moss_share {
                                                m = t.moss;
                                            }
                                        }
                                    }
                                    // Irregular cover: broad drifts broken up by a finer,
                                    // up-and-down noise, so it hangs in ragged sheets.
                                    let cover = perlin3(xm * 0.06, ym * 0.04, zm * 0.06, sd[S_STRATA] + 7) * 0.65 + perlin3(xm * 0.3, ym * 0.11, zm * 0.3, sd[S_STRATA] + 17) * 0.35;
                                    if t.mantle != 0 && margin < 0.8 && cover * 0.5 + 0.5 < t.mantle_share {
                                        m = t.mantle;
                                    }
                                }
                            }
                            v[pidx(px, py, pz)] = m;
                            continue;
                        }
                        let m = if rocky {
                            self.strata(ym, c.strata_off)
                        } else if c.h < sea + c.beach {
                            if depth > 2.5 {
                                self.strata(ym, c.strata_off)
                            } else if c.h < sea - 4.0 && c.strata_off > 0.3 {
                                GRAVEL
                            } else {
                                SAND
                            }
                        } else if depth < vsize + 0.01 {
                            if c.h > c.snow || c.lying {
                                SNOW
                            } else if c.slope > 1.1 {
                                DIRT
                            } else {
                                GRASS
                            }
                        } else if depth < c.dirt {
                            DIRT
                        } else {
                            self.strata(ym, c.strata_off)
                        };
                        v[pidx(px, py, pz)] = m;
                    }
                }
            }

            add_nanos(2, t_fill);
            let t_draw = std::time::Instant::now();
            if let Some(w) = &self.world {
                let y0m = ((cy * CHUNK - MY as i32) * step) as f32 * VOXEL;
                carve_tubes(&tubes, v, Vec3::new(pad_x0, y0m, pad_z0), vsize, sd[S_CARVE]);
                if draw_structures(w, &structs, v, Vec3::new(pad_x0, y0m, pad_z0), vsize, info) {
                    any_solid = true;
                }
                if !city_items.is_empty() && super::city::draw(&w.city, &city_items, &mut v[..], Vec3::new(pad_x0, y0m, pad_z0), vsize, PAD, PADY, pidx, |px, pz| info[pz * PAD + px].h) {
                    any_solid = true;
                }
                if !route_items.is_empty()
                    && super::route::ribbon::draw(&w.ribbons, &route_items, &mut v[..], Vec3::new(pad_x0, y0m, pad_z0), vsize, PAD, PADY, pidx, |px, pz| {
                        info[pz * PAD + px].h
                    })
                {
                    any_solid = true;
                }
                if !bone_items.is_empty() && w.bones.draw(&bone_items, &mut v[..], Vec3::new(pad_x0, y0m, pad_z0), vsize, PAD, PADY, pidx, sea) {
                    any_solid = true;
                }
            }
            // The EDIT page's studio floor, over everything the world put
            // here and under the plant it is showing.
            // Full detail only: a coarse chunk samples the grid at half a metre or
            // worse, which comes out as pale patches with no lines in it.
            if let Some(f) = (step == 1).then(|| *self.studio_floor.read().unwrap()).flatten() {
                let fy0 = ((cy * CHUNK - MY as i32) * step) as f32 * VOXEL;
                for pz in 0..PAD {
                    for px in 0..PAD {
                        let xm = pad_x0 + (px as f32 + 0.5) * vsize;
                        let zm = pad_z0 + (pz as f32 + 0.5) * vsize;
                        // Round the platform the hill is cut back at one in
                        // one, so on a slope its uphill edge is a bank rather
                        // than a wall of cut ground standing over the floor.
                        let out = f.outside(xm, zm);
                        if out > STUDIO_BANK_M {
                            continue;
                        }
                        if out > 0.0 {
                            for py in 0..PADY {
                                let ym = fy0 + (py as f32 + 0.5) * vsize;
                                if ym > f.top_m + out {
                                    v[pidx(px, py, pz)] = AIR;
                                }
                            }
                            continue;
                        }
                        for py in 0..PADY {
                            let ym = fy0 + (py as f32 + 0.5) * vsize;
                            // Everything over the floor is cleared: clearing
                            // only five metres of it left the hill hanging
                            // over the uphill end of the platform on a slope.
                            if ym <= f.top_m - vsize {
                                continue;
                            }
                            let m = if ym > f.top_m {
                                // The block of material the page is showing
                                // stands in the cleared air above the floor.
                                if f.in_block(xm, zm) && ym <= f.top_m + f.block_h {
                                    f.block
                                } else {
                                    AIR
                                }
                            } else {
                                f.mat(xm, zm)
                            };
                            if m != AIR {
                                any_solid = true;
                            }
                            v[pidx(px, py, pz)] = m;
                        }
                    }
                }
            }
            // Trees, written only into air.
            let y0m = ((cy * CHUNK - MY as i32) * step) as f32 * VOXEL;
            let y1m = ((cy * CHUNK + CHUNK + MY as i32) * step) as f32 * VOXEL;
            let origin = Vec3::new(pad_x0, y0m, pad_z0);
            for t in &trees {
                if t.pos.x + t.radius < pad_x0 || t.pos.x - t.radius > pad_x1 || t.pos.z + t.radius < pad_z0 || t.pos.z - t.radius > pad_z1 || t.pos.y + t.top < y0m || t.pos.y - 0.8 > y1m {
                    continue;
                }
                let mut press = Press {
                    v,
                    o: origin,
                    vsize,
                    leaf_seed: self.seeds[S_LEAF],
                };
                if t.press(&mut press) {
                    any_solid = true;
                }
            }
            add_nanos(3, t_draw);
            if key.lod == 0 {
                if let Some(edits) = edits {
                    if apply_edits(v, key.x, cy, key.z, edits) {
                        any_solid = true;
                    }
                }
            }
            if any_solid {
                // Each padded column's true ground height in this chunk's voxels.
                let y0m = ((cy * CHUNK - MY as i32) * step) as f32 * VOXEL;
                let mut colh = [0f32; PAD * PAD];
                for (h, c) in colh.iter_mut().zip(info.iter()) {
                    *h = (c.h - y0m) / vsize;
                }
                emit(cy, v, &colh);
            }
        }
        (solid_below, count)
    }

    /// Heights on a regular grid covering the world, metres; used by the water shader
    /// and for picking a spawn point.
    /// The ground at (x, z), under any tower standing there (its foot, not its top).
    pub fn ground_under(&self, x: f32, z: f32) -> f32 {
        let h = self.height(x, z);
        self.world.as_ref().map_or(h, |w| w.towers.iter().filter_map(|t| t.under_at(x, z)).fold(h, f32::min))
    }

    pub fn height_grid(&self, n: usize) -> Vec<f32> {
        let cell = self.spec.size_m / n as f32;
        let mut out = vec![0.0f32; n * n];
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let rows_per = n.div_ceil(threads);
        std::thread::scope(|s| {
            for (ti, chunk) in out.chunks_mut(rows_per * n).enumerate() {
                s.spawn(move || {
                    for (i, h) in chunk.iter_mut().enumerate() {
                        let row = ti * rows_per + i / n;
                        let col = i % n;
                        *h = self.height((col as f32 + 0.5) * cell, (row as f32 + 0.5) * cell);
                    }
                });
            }
        });
        // Under a tower the ground is what it stands on, not its top: the
        // water beside it is as deep as the sea round it.
        if let Some(w) = &self.world {
            for t in &w.towers {
                let (x0, z0) = ((t.x0 as f32 / cell) as usize, (t.z0 as f32 / cell).max(0.0) as usize);
                let (x1, z1) = ((((t.x0 + t.w) as f32 / cell) as usize + 1).min(n), (((t.z0 + t.w) as f32 / cell) as usize + 1).min(n));
                for row in z0..z1 {
                    for col in x0.min(n)..x1 {
                        if let Some(u) = t.under_at((col as f32 + 0.5) * cell, (row as f32 + 0.5) * cell) {
                            let h = &mut out[row * n + col];
                            *h = h.min(u);
                        }
                    }
                }
            }
        }
        out
    }

    /// Water colours: the palette (world shallow/deep, then two alternates
    /// from biomes; w = 0 where unset) and, per cell of an n x n grid, how far
    /// toward each alternate the water there leans.
    pub fn water_grid(&self, n: usize) -> ([[f32; 4]; 6], Vec<[f32; 2]>) {
        let mut pal = [[0.0f32; 4]; 6];
        let mut share = vec![[0.0f32; 2]; n * n];
        let Some(w) = &self.world else {
            return (pal, share);
        };
        if let Some(c) = &w.def.water {
            let (s, d) = c.linear();
            pal[0] = s;
            pal[1] = d;
        }
        // The first two biomes the world uses that colour their water.
        let mut alts: Vec<u8> = Vec::new();
        let mut used = vec![false; w.lib.biomes.len()];
        for &b in &w.f.biome {
            used[b as usize] = true;
        }
        for (i, b) in w.lib.biomes.iter().enumerate() {
            if let (true, Some(c)) = (used[i], b.water) {
                if alts.len() < 2 {
                    let (s, d) = c.linear();
                    pal[2 + alts.len() * 2] = s;
                    pal[3 + alts.len() * 2] = d;
                    alts.push(i as u8);
                }
            }
        }
        if alts.is_empty() {
            return (pal, share);
        }
        let cell = self.spec.size_m / n as f32;
        let sea = self.spec.sea_level_m;
        let mut biome: Vec<u8> = (0..n * n).map(|i| w.f.biome[w.f.idx((i % n) as f32 * cell + cell * 0.5, (i / n) as f32 * cell + cell * 0.5)]).collect();
        // Inland water (a cenote, a lake: below sea level but not joined to
        // the open sea) takes the colour of the biome round its shore; the
        // cells under it are all seabed.
        let wet: Vec<bool> = (0..n * n)
            .map(|i| self.ground_under((i % n) as f32 * cell + cell * 0.5, (i / n) as f32 * cell + cell * 0.5) < sea)
            .collect();
        let mut comp = vec![u32::MAX; n * n];
        let mut next = 0u32;
        let nb = |i: usize| -> Vec<usize> {
            let (x, z) = ((i % n) as i32, (i / n) as i32);
            [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                .iter()
                .filter_map(|(dx, dz)| {
                    let (nx, nz) = (x + dx, z + dz);
                    (nx >= 0 && nz >= 0 && (nx as usize) < n && (nz as usize) < n).then(|| nz as usize * n + nx as usize)
                })
                .collect()
        };
        for start in 0..n * n {
            if !wet[start] || comp[start] != u32::MAX {
                continue;
            }
            let mut cells = Vec::new();
            let mut stack = vec![start];
            comp[start] = next;
            let mut open = false;
            let mut shore: std::collections::HashMap<u8, u32> = std::collections::HashMap::new();
            while let Some(i) = stack.pop() {
                cells.push(i);
                let (x, z) = (i % n, i / n);
                open |= x == 0 || z == 0 || x == n - 1 || z == n - 1;
                for j in nb(i) {
                    if wet[j] {
                        if comp[j] == u32::MAX {
                            comp[j] = next;
                            stack.push(j);
                        }
                    } else {
                        *shore.entry(biome[j]).or_default() += 1;
                    }
                }
            }
            next += 1;
            if !open {
                // Ties go to the lower biome index: a HashMap's order differs per
                // process, and the `--map` check needs the same world every run.
                if let Some((&bi, _)) = shore.iter().max_by_key(|&(&b, &c)| (c, std::cmp::Reverse(b))) {
                    for i in cells {
                        biome[i] = bi;
                    }
                }
            }
        }
        let mut a = vec![0.0f32; n * n];
        let mut b = vec![0.0f32; n * n];
        for z in 0..n {
            for x in 0..n {
                let bi = biome[z * n + x];
                if alts.first() == Some(&bi) {
                    a[z * n + x] = 1.0;
                }
                if alts.get(1) == Some(&bi) {
                    b[z * n + x] = 1.0;
                }
            }
        }
        let r = ((6.0 / cell) as i32).max(1);
        crate::util::blur(&mut a, n, r);
        crate::util::blur(&mut b, n, r);
        for i in 0..n * n {
            share[i] = [a[i], b[i]];
        }
        (pal, share)
    }

    /// How misty each of `n` x `n` cells over the world is (0..1), from its
    /// biome, blurred over about 40 m so banks thin out at their edges and
    /// spill a little past the shore. The noise island has no biomes: all 0.
    pub fn mist_grid(&self, n: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; n * n];
        let Some(w) = &self.world else { return out };
        let cell = self.spec.size_m / n as f32;
        for z in 0..n {
            for x in 0..n {
                let i = w.f.idx((x as f32 + 0.5) * cell, (z as f32 + 0.5) * cell);
                out[z * n + x] = w.lib.biomes.get(w.f.biome[i] as usize).map_or(0.0, |b| b.mist.clamp(0.0, 1.0));
            }
        }
        // Sea fog: over open water a misty coast's mist drifts out for a
        // hundred metres or so, so sea stacks stand in it rather than beside it.
        let mut wide = out.clone();
        crate::util::blur(&mut wide, n, ((80.0 / cell) as i32).max(1));
        let sea = self.spec.sea_level_m;
        // Only the open sea: water joined to the edge of the map. A flooded
        // sinkhole inland is below sea level too, and would fill with fog.
        let mut open = vec![false; n * n];
        let wet: Vec<bool> = (0..n * n)
            .map(|i| self.ground_under((i % n) as f32 * cell + cell * 0.5, (i / n) as f32 * cell + cell * 0.5) < sea)
            .collect();
        let mut stack: Vec<usize> = (0..n * n).filter(|&i| wet[i] && (i % n == 0 || i % n == n - 1 || i / n == 0 || i / n == n - 1)).collect();
        while let Some(i) = stack.pop() {
            if open[i] {
                continue;
            }
            open[i] = true;
            let (x, z) = (i % n, i / n);
            for (dx, dz) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                if nx >= 0 && nz >= 0 && (nx as usize) < n && (nz as usize) < n {
                    let j = nz as usize * n + nx as usize;
                    if wet[j] && !open[j] {
                        stack.push(j);
                    }
                }
            }
        }
        for i in 0..n * n {
            if open[i] {
                out[i] = out[i].max((wide[i] * 4.0).min(0.8));
            }
        }
        let r = ((20.0 / cell) as i32).max(1);
        crate::util::blur(&mut out, n, r);
        // Spray hangs round the foot of a waterfall, more for a bigger drop.
        for fall in &w.f.falls {
            let drop = fall.lip.y - fall.foot;
            let k = (drop / 25.0).clamp(0.25, 1.0);
            let reach = 8.0 + fall.width + drop * 0.3;
            for z in 0..n {
                for x in 0..n {
                    let p = glam::Vec2::new((x as f32 + 0.5) * cell, (z as f32 + 0.5) * cell);
                    let d = p.distance(glam::Vec2::new(fall.splash.x, fall.splash.z)) / reach;
                    if d < 2.0 {
                        out[z * n + x] = out[z * n + x].max(k * (-d * d * 2.0).exp());
                    }
                }
            }
        }
        out
    }

    /// A flat grassy spot near the east coast, looking out to sea.
    pub fn spawn_point(&self) -> (Vec3, f32) {
        if let Some(w) = &self.world {
            return (w.spawn, w.spawn_yaw);
        }
        let s = &self.spec;
        let w = s.size_m;
        let cz = s.island_centre[1] * w;
        let sea = s.sea_level_m;
        let mut best = (Vec3::new(w * 0.5, sea + 10.0, w * 0.5), f32::MAX);
        let mut x = s.island_centre[0] * w;
        // Walk east until the ground reaches the sea, remembering the last good spot.
        let mut last_land = x;
        while x < w {
            let h = self.height(x, cz);
            if h < sea + 1.0 {
                break;
            }
            last_land = x;
            x += 2.0;
        }
        for dz in (-60i32..=60).step_by(4) {
            for back in (20..=70).step_by(5) {
                let px = last_land - back as f32;
                let pz = cz + dz as f32;
                let h = self.height(px, pz);
                if h < sea + 3.0 {
                    continue;
                }
                let hx = self.height(px + 1.0, pz);
                let hz = self.height(px, pz + 1.0);
                let slope = ((hx - h).powi(2) + (hz - h).powi(2)).sqrt();
                if slope > 0.35 {
                    continue;
                }
                // No trees in the way, and the ground falls away toward the sea.
                let clear = self
                    .trees_near(px - 6.0, pz - 6.0, px + 14.0, pz + 6.0)
                    .iter()
                    .all(|t| ((t.pos.x - px).powi(2) + (t.pos.z - pz).powi(2)).sqrt() > t.radius + 2.5);
                if !clear {
                    continue;
                }
                let mut blocked = false;
                let mut x = px + 4.0;
                while x < last_land + 10.0 {
                    if self.height(x, pz) > h + 1.0 {
                        blocked = true;
                        break;
                    }
                    x += 3.0;
                }
                if blocked {
                    continue;
                }
                // Prefer flat ground with a little height and few trees in the view.
                let in_view = self
                    .trees_near(px + 4.0, pz - 12.0, px + 40.0, pz + 12.0)
                    .iter()
                    .filter(|t| t.pos.x > px + 2.0 && (t.pos.z - pz).abs() < (t.pos.x - px) * 0.6 + 2.0)
                    .count();
                let score = slope * 20.0 + (h - (sea + 9.0)).abs() * 0.4 + dz.abs() as f32 * 0.05 + in_view as f32 * 1.5;
                if score < best.1 {
                    best = (Vec3::new(px, h, pz), score);
                }
            }
        }
        // Yaw 0 looks along -Z in our camera; the sea is +X, so turn right.
        (best.0, -std::f32::consts::FRAC_PI_2)
    }
}

/// Biome edges feather, unless VOXEL_FRINGE=0 (for before-and-after shots).
fn fringe_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_FRINGE").map(|v| v != "0").unwrap_or(true))
}

/// The field cell whose biome colours the top of the column at (x, z): the
/// column's own cell well inside a biome, a neighbour a few metres off near
/// an edge. Two scales of noise push the lookup about, so a boundary between
/// strongly different ground breaks into interlocking patches a metre or two
/// across rather than one clean line. Grass and flowers grow from the surface
/// material, so they fringe along with it. Deep inside a biome every sample
/// lands on the same biome and nothing changes.
///
/// How far the lookup reaches is the biome's own `fringe`, so two shades of
/// grass can blend while a shore or a lava field keeps its edge: a biome with
/// `fringe` 0 neither spills nor is spilled into.
fn fringe_cell(w: &super::fields::World, x: f32, z: f32, seed: u32) -> u32 {
    let f = &w.f;
    let i = f.idx(x, z);
    if !fringe_on() {
        return i as u32;
    }
    let reach = w.biome_mats.get(f.biome[i] as usize).map_or(0.0, |b| b.fringe);
    if reach <= 0.0 {
        return i as u32;
    }
    // Two scales of smooth noise make the line ragged in clumps; a per-cell
    // draw on top of them speckles the cells within about half the reach of
    // it, so the two grounds interleave in a band instead of meeting along a
    // single (if wandering) line.
    let off = |s: u32| perlin2(x * 0.11, z * 0.11, s) * 0.6 + perlin2(x * 0.47, z * 0.47, s ^ 0x5bd1) * 0.4;
    let h = hash2((x / super::fields::CELL) as i32, (z / super::fields::CELL) as i32, seed + 9);
    let (a, r) = (unit(h) * std::f32::consts::TAU, unit(hash32(h)).sqrt() * reach * 0.55);
    let j = f.idx(x + off(seed + 7) * reach + a.cos() * r, z + off(seed + 8) * reach + a.sin() * r);
    // Never reach across the shore or a river bank, and never into a biome
    // that keeps a hard edge.
    let hard = w.biome_mats.get(f.biome[j] as usize).is_none_or(|b| b.fringe <= 0.0);
    if !hard && (f.h[j] < f.sea) == (f.h[i] < f.sea) && f.water[j] == f.water[i] {
        j as u32
    } else {
        i as u32
    }
}

/// The voxel material at depth `depth` below the surface of a field column.
fn field_material(w: &super::fields::World, c: &ColInfo, ym: f32, depth: f32, vsize: f32, rocky: bool, seed: u32) -> u8 {
    let f = &w.f;
    let i = c.cell as usize;
    let bm = w.biome_mats[f.biome[i] as usize];
    let rock = || {
        if bm.rock != 0 {
            bm.rock
        } else {
            w.rock_at(ym, f.lift_at(i), c.strata_off, c.swell, seed)
        }
    };
    if rocky {
        return rock();
    }
    if depth < vsize + 0.01 {
        // The top voxel takes its biome from the fringed cell, so biome edges
        // interlock instead of drawing a line.
        let bm = w.biome_mats[f.biome[c.fringe as usize] as usize];
        return if c.street != 0 {
            c.street
        } else if f.surface[i] != 0 {
            f.surface[i]
        } else if c.lying && c.h > f.sea + 0.5 {
            // Snow lying this season, on whatever this biome's ground is
            // (world/season.rs). Not on a street or a laid floor, which are
            // swept, and not on a lake bed.
            crate::world::material::SNOW
        } else if c.slope > 1.1 {
            bm.subsurface
        } else {
            bm.surface
        };
    }
    for l in f.layers.get(i).into_iter().flatten() {
        if l.mat != 0 && depth >= l.depth() && depth < l.depth() + l.thick() {
            return l.mat;
        }
    }
    if depth < c.dirt {
        bm.subsurface
    } else {
        rock()
    }
}

/// Hollow tunnels out of a padded chunk: round in section with a rough wall and a
/// flat floor a little below the centre, so they can be walked.
fn carve_tubes(tubes: &[super::fields::Tube], v: &mut [u8; PAD3], o: Vec3, vsize: f32, seed: u32) {
    for t in tubes {
        let r = t.r * 1.2;
        let lo = t.a.min(t.b) - Vec3::splat(r);
        let hi = t.a.max(t.b) + Vec3::splat(r);
        let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
        let (x0, x1) = (idx(lo.x, o.x).max(0), idx(hi.x, o.x).min(PAD as i32 - 1));
        let (y0, y1) = (idx(lo.y, o.y).max(0), idx(hi.y, o.y).min(PADY as i32 - 1));
        let (z0, z1) = (idx(lo.z, o.z).max(0), idx(hi.z, o.z).min(PAD as i32 - 1));
        if x0 > x1 || y0 > y1 || z0 > z1 {
            continue;
        }
        let ab = t.b - t.a;
        let len2 = ab.length_squared().max(1e-6);
        for py in y0..=y1 {
            for pz in z0..=z1 {
                for px in x0..=x1 {
                    let p = o + (Vec3::new(px as f32, py as f32, pz as f32) + 0.5) * vsize;
                    let k = ((p - t.a).dot(ab) / len2).clamp(0.0, 1.0);
                    let c = t.a + ab * k;
                    // The noise only moves the wall by up to 18%: skip it outside that.
                    let d2 = (p - c).length_squared();
                    if d2 >= r * r {
                        continue;
                    }
                    let wall = t.r * (1.0 + 0.18 * perlin3(p.x * 0.35, p.y * 0.35, p.z * 0.35, seed));
                    // Vertical segments (skylights) have no floor.
                    let floor_ok = ab.y.abs() > ab.x.abs() + ab.z.abs() || p.y > c.y - t.r * 0.55;
                    if (p - c).length_squared() < wall * wall && floor_ok {
                        v[pidx(px as usize, py as usize, pz as usize)] = AIR;
                    }
                }
            }
        }
    }
    // Linings: the rock left in a shell over the roof of a lined tunnel
    // takes the lining (glowing ceilings). After all carving, so a lining
    // never fills another tunnel.
    for t in tubes.iter().filter(|t| t.lining != 0 || t.wall != 0) {
        let r = t.r * 1.2 + 0.75;
        let lo = t.a.min(t.b) - Vec3::splat(r);
        let hi = t.a.max(t.b) + Vec3::splat(r);
        let idx = |m: f32, o: f32| ((m - o) / vsize).floor() as i32;
        let (x0, x1) = (idx(lo.x, o.x).max(0), idx(hi.x, o.x).min(PAD as i32 - 1));
        let (y0, y1) = (idx(lo.y, o.y).max(0), idx(hi.y, o.y).min(PADY as i32 - 1));
        let (z0, z1) = (idx(lo.z, o.z).max(0), idx(hi.z, o.z).min(PAD as i32 - 1));
        if x0 > x1 || y0 > y1 || z0 > z1 {
            continue;
        }
        let ab = t.b - t.a;
        let len2 = ab.length_squared().max(1e-6);
        for py in y0..=y1 {
            for pz in z0..=z1 {
                for px in x0..=x1 {
                    let i = pidx(px as usize, py as usize, pz as usize);
                    if v[i] == AIR {
                        continue;
                    }
                    let p = o + (Vec3::new(px as f32, py as f32, pz as f32) + 0.5) * vsize;
                    let k = ((p - t.a).dot(ab) / len2).clamp(0.0, 1.0);
                    let c = t.a + ab * k;
                    if (p - c).length_squared() >= r * r {
                        continue;
                    }
                    // The top of the roof takes the lining, the rest of the
                    // shell (sides and floor) the wall.
                    // With a wall the lining grows in patches (glow-worm
                    // colonies), not over the whole roof.
                    let patch = t.wall == 0 || perlin3(p.x * 0.22, p.y * 0.22, p.z * 0.22, 9101) > 0.05;
                    if t.lining != 0 && p.y >= c.y + t.r * 0.45 && patch {
                        v[i] = t.lining;
                    } else if t.wall != 0 {
                        v[i] = t.wall;
                    } else if t.lining != 0 && p.y >= c.y + t.r * 0.1 {
                        v[i] = t.lining;
                    }
                }
            }
        }
    }
}

/// How far a placed structure has decayed, 0 (kept up, or natural) to 1.
pub fn structure_wear(w: &super::fields::World, p: &super::fields::Placed) -> f32 {
    let age = p.age * w.def.weathering;
    if w.lib.structures[p.def].decays && age > 0.0 {
        1.0 - (-age / 45.0).exp()
    } else {
        0.0
    }
}

/// Whether the roof over local column (lx, lz) of shape `s` still stands at
/// `wear`. A roof caves in from one breach (somewhere along the ridge, or at
/// one end) and the hole grows outward with a ragged edge, so what is left is
/// one piece still resting on its walls, never scattered tiles.
pub fn roof_kept(s: &super::shape::Shape, seed: u32, lx: i32, lz: i32, wear: f32) -> bool {
    use super::shape::{decay_rate, PART_ROOF};
    let loss = wear * decay_rate(PART_ROOF) * 1.35;
    if loss <= 0.0 {
        return true;
    }
    let (sx, sz) = (s.size[0] as f32, s.size[2] as f32);
    let h = hash3(17, 3, 5, seed ^ 0x9d);
    let (u, v) = (unit(h), unit(crate::world::noise::hash32(h)));
    // Along the long axis anywhere (an end 40% of the time), across it near the middle.
    let along = if u < 0.2 {
        0.0
    } else if u < 0.4 {
        1.0
    } else {
        0.15 + 0.7 * v
    };
    let across = 0.35 + 0.3 * unit(crate::world::noise::hash32(h ^ 0x51));
    let (bx, bz) = if sx >= sz { (along * sx, across * sz) } else { (across * sx, along * sz) };
    let far = [(0.0, 0.0), (sx, 0.0), (0.0, sz), (sx, sz)]
        .iter()
        .map(|&(x, z): &(f32, f32)| ((x - bx).powi(2) + (z - bz).powi(2)).sqrt())
        .fold(1.0, f32::max);
    let (px, pz) = (lx as f32 + 0.5, lz as f32 + 0.5);
    let d = ((px - bx).powi(2) + (pz - bz).powi(2)).sqrt() / far;
    let edge = perlin2(px * 0.18, pz * 0.18, seed ^ 0x3a) * 0.12;
    d * 0.9 + 0.1 + edge >= loss
}

/// Whether a structure voxel of `part` at local (lx, ly, lz), world column
/// (wx, wz), of shape `s` has gone with `wear`. Roofs cave in from one breach
/// (`roof_kept`); glass and canvas fail in whole sections; everything else
/// crumbles from the top of its column (`col_top` voxels) down, ragged but
/// continuous along a wall, so nothing is left hanging, except under roof
/// that still stands, which keeps the walls under it whole. Shared by the
/// drawing and by the wall vines, which hang from what still stands.
#[allow(clippy::too_many_arguments)]
pub fn crumbled(s: &super::shape::Shape, seed: u32, part: u8, lx: i32, ly: i32, lz: i32, wx: i32, wz: i32, col_top: f32, wear: f32) -> bool {
    use super::shape::{decay_rate, PART_CLOTH, PART_FLOOR, PART_GLASS, PART_ROOF};
    match part {
        PART_ROOF => !roof_kept(s, seed, lx, lz, wear),
        PART_CLOTH | PART_GLASS => {
            let keep = (1.0 - wear * decay_rate(part) * 1.35).max(0.0);
            unit(hash3(lx.div_euclid(6), 0, lz.div_euclid(6), seed ^ 0x9d)) >= keep
        }
        PART_FLOOR => false,
        _ => {
            let col = (lz * s.size[0] + lx) as usize;
            if s.roof.get(col).is_some_and(|&r| r != 0) && roof_kept(s, seed, lx, lz, wear) {
                return false;
            }
            let n = (perlin2(wx as f32 * 0.16, wz as f32 * 0.16, seed) * 0.5 + 0.5).clamp(0.0, 1.0);
            let loss = wear * 0.4 * col_top * (0.2 + n);
            ly as f32 >= col_top - loss
        }
    }
}

/// Which voxels of placed structure `p` still stand after decay, one bit per
/// shape voxel (`Shape::idx` order), or None when nothing has decayed, then
/// how many voxels fell and how many were put back to hold others up.
/// Decay alone (`crumbled`) is decided voxel by voxel and can leave pieces
/// with nothing under them: a lintel over a doorway whose jambs went, an
/// upper storey over walls that crumbled, a gable end past a caved-in roof.
/// This is the gravity pass: whatever is not joined, face or edge, through
/// standing voxels to the ground falls and is gone. The ground is the
/// structure's own base row (it stands on its foundation) and any voxel the
/// terrain touches from below or beside.
pub fn standing_map(w: &super::fields::World, p: &super::fields::Placed) -> (Option<Vec<u64>>, usize, usize) {
    use super::shape::{rotate, CARVE};
    let wear = structure_wear(w, p);
    if wear <= 0.0 {
        return (None, 0, 0);
    }
    let s = &w.shapes[p.shape];
    let def = &w.lib.structures[p.def];
    let [sx, sy, sz] = s.size;
    let n = (sx * sy * sz).max(0) as usize;
    let base = if def.level { def.anchor[1] } else { 0 };
    let (x0, _, z0) = (p.origin[0], p.origin[1], p.origin[2]);
    // Decay alone: solid and not crumbled.
    let mut solid = vec![false; n];
    for ly in 0..sy.min(s.top) {
        for lz in 0..sz {
            for lx in 0..sx {
                let i = s.idx(lx, ly, lz).unwrap();
                let (m, part) = (s.mat[i], s.part[i]);
                if m == 0 || m == CARVE {
                    continue;
                }
                let (rx, rz) = rotate(lx, lz, s.size, p.rot);
                let col_top = (s.col_top[(lz * sx + lx) as usize] - base).max(1) as f32;
                solid[i] = !crumbled(s, p.seed, part, lx, ly - base, lz, x0 + rx, z0 + rz, col_top, wear);
            }
        }
    }
    // Terrain under each column of the footprint, in structure voxels above
    // the origin, as the highest of the column and its four neighbours, so a
    // wall dug into a slope rests on the slope beside it.
    let ground_at = |lx: i32, lz: i32| -> i32 {
        let (rx, rz) = rotate(lx, lz, s.size, p.rot);
        let h = |dx: i32, dz: i32| w.f.height((x0 + rx + dx) as f32 / VPM + 0.125, (z0 + rz + dz) as f32 / VPM + 0.125);
        let top = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().map(|&(dx, dz)| h(dx, dz)).fold(f32::MIN, f32::max);
        (top * VPM).floor() as i32 - p.origin[1]
    };
    let ground: Vec<i32> = (0..sz).flat_map(|lz| (0..sx).map(move |lx| (lx, lz))).map(|(lx, lz)| ground_at(lx, lz)).collect();
    // `VOXEL_GRAVITY=0` keeps decay alone, for comparing.
    if std::env::var("VOXEL_GRAVITY").is_ok_and(|v| v == "0") {
        let mut bits = vec![0u64; n.div_ceil(64)];
        for (i, _) in solid.iter().enumerate().filter(|(_, &k)| k) {
            bits[i / 64] |= 1 << (i % 64);
        }
        return (Some(bits), 0, 0);
    }
    // It rests on its lowest row (a hull afloat starts above 0), or on the
    // row its anchor sits at for a building dug in.
    let low = (0..sy)
        .find(|&y| (0..sx * sz).any(|c| s.mat[(y * sx * sz + c) as usize] != 0 && s.mat[(y * sx * sz + c) as usize] != CARVE))
        .unwrap_or(0);
    let rest = base.max(low);
    // Flood from the supported voxels through face and edge neighbours.
    let flood = |solid: &[bool], anchored: &dyn Fn(usize) -> bool| -> Vec<bool> {
        let mut kept = vec![false; n];
        let mut stack = Vec::new();
        for ly in 0..sy {
            for lz in 0..sz {
                for lx in 0..sx {
                    let i = s.idx(lx, ly, lz).unwrap();
                    if solid[i] && (ly <= rest || ly <= ground[(lz * sx + lx) as usize] || anchored(i)) {
                        kept[i] = true;
                        stack.push((lx, ly, lz));
                    }
                }
            }
        }
        while let Some((x, y, z)) = stack.pop() {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        // Faces and edges (18 neighbours): a stepped gable is
                        // joined only along edges.
                        let k = (dx as i32).abs() + (dy as i32).abs() + (dz as i32).abs();
                        if k == 0 || k == 3 {
                            continue;
                        }
                        let Some(j) = s.idx(x + dx, y + dy, z + dz) else {
                            continue;
                        };
                        if solid[j] && !kept[j] {
                            kept[j] = true;
                            stack.push((x + dx, y + dy, z + dz));
                        }
                    }
                }
            }
        }
        kept
    };
    // Some shapes are drawn with parts that touch nothing (a gable roof a
    // voxel or more above the beam it sits on, a hanging lantern): those
    // were meant to stand, so only what decay cut loose falls.
    let whole: Vec<bool> = s.mat.iter().map(|&m| m != 0 && m != CARVE).collect();
    let intact = flood(&whole, &|_| false);
    let mut solid = solid;
    let mut first = flood(&solid, &|i| !intact[i]);
    // Rebuild what holds up the loose pieces, so a ruin keeps the height it
    // had. First straight down: under each loose voxel, the decayed voxels
    // of the intact shape below it come back (the roof ring an upper storey
    // stands on, the wall under a gable end), down to something solid.
    let mut rebuilt = 0;
    for i in 0..n {
        if !solid[i] || first[i] {
            continue;
        }
        let mut j = i;
        while j >= (sx * sz) as usize {
            j -= (sx * sz) as usize;
            if !whole[j] || solid[j] {
                break;
            }
            solid[j] = true;
            rebuilt += 1;
        }
    }
    if rebuilt > 0 {
        first = flood(&solid, &|i| !intact[i]);
    }
    // Then what still hangs: the fewest voxels of the intact shape that join
    // it to what stands (a strip of roof, a jamb). A 0-1 search from
    // everything standing, through the intact shape: standing or loose
    // voxels cost nothing to cross, decayed ones 1.
    let mut dist = vec![u32::MAX; n];
    let mut from = vec![u32::MAX; n];
    let mut dq = std::collections::VecDeque::new();
    for i in 0..n {
        if first[i] {
            dist[i] = 0;
            dq.push_back(i);
        }
    }
    let at = |i: usize| -> (i32, i32, i32) {
        let i = i as i32;
        (i % sx, i / (sx * sz), (i / sx) % sz)
    };
    while let Some(i) = dq.pop_front() {
        let (x, y, z) = at(i);
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let k = (dx as i32).abs() + (dy as i32).abs() + (dz as i32).abs();
                    if k == 0 || k == 3 {
                        continue;
                    }
                    let Some(j) = s.idx(x + dx, y + dy, z + dz) else {
                        continue;
                    };
                    if !whole[j] {
                        continue;
                    }
                    let c = if solid[j] { 0 } else { 1 };
                    if dist[i] + c < dist[j] {
                        dist[j] = dist[i] + c;
                        from[j] = i as u32;
                        if c == 0 {
                            dq.push_front(j)
                        } else {
                            dq.push_back(j)
                        }
                    }
                }
            }
        }
    }
    // Each loose piece: put back its cheapest path if that costs no more
    // than the piece is big (a storey is rebuilt, a lintel falls).
    let mut seen = vec![false; n];
    for i0 in 0..n {
        if !solid[i0] || first[i0] || seen[i0] {
            continue;
        }
        let mut piece = vec![i0];
        seen[i0] = true;
        let mut k = 0;
        while k < piece.len() {
            let (x, y, z) = at(piece[k]);
            k += 1;
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let m = (dx as i32).abs() + (dy as i32).abs() + (dz as i32).abs();
                        if m == 0 || m == 3 {
                            continue;
                        }
                        let Some(j) = s.idx(x + dx, y + dy, z + dz) else {
                            continue;
                        };
                        if solid[j] && !first[j] && !seen[j] {
                            seen[j] = true;
                            piece.push(j);
                        }
                    }
                }
            }
        }
        let Some(&best) = piece.iter().min_by_key(|&&j| dist[j]) else {
            continue;
        };
        if dist[best] == u32::MAX || dist[best] as usize > piece.len() / 2 {
            continue;
        }
        let mut j = best;
        while from[j] != u32::MAX {
            if !solid[j] {
                solid[j] = true;
                rebuilt += 1;
            }
            j = from[j] as usize;
        }
    }
    let kept = flood(&solid, &|i| !intact[i]);
    let mut bits = vec![0u64; n.div_ceil(64)];
    for (i, _) in kept.iter().enumerate().filter(|(_, &k)| k) {
        bits[i / 64] |= 1 << (i % 64);
    }
    let fell = solid.iter().zip(&kept).filter(|(&a, &b)| a && !b).count();
    (Some(bits), fell, rebuilt)
}

/// Whether the voxel at local (lx, ly, lz) of placed structure `k` stands
/// after decay and gravity (`standing_map`). Air and carved voxels too say
/// true: only the drawing decides what they do.
pub fn still_standing(w: &super::fields::World, k: usize, lx: i32, ly: i32, lz: i32) -> bool {
    let Some(Some(bits)) = w.standing.get(k) else {
        return true;
    };
    let Some(i) = w.shapes[w.placed[k].shape].idx(lx, ly, lz) else {
        return true;
    };
    let (m, _) = w.shapes[w.placed[k].shape].get(lx, ly, lz);
    m == 0 || m == super::shape::CARVE || bits[i / 64] >> (i % 64) & 1 == 1
}

/// Stamp the chronicle's structures into a padded chunk: rotated, decayed by
/// age (roofs and timber first, high before low), mossed, and buried where later
/// deposits raised the ground. `info` gives each padded column's surface height.
fn draw_structures(w: &super::fields::World, list: &[u32], v: &mut [u8; PAD3], o: Vec3, vsize: f32, info: &[ColInfo]) -> bool {
    use super::shape::{unrotate, CARVE, PART_STONE, PART_WALL};
    let mut any = false;
    let mossy = w.lib.material("mossy_stone");
    // Under the water a sunk structure crusts over with reef rock and coral
    // instead of moss (docs/UNDERWATER.md): whichever of these the world has.
    let crusts: Vec<u8> = ["reef_lime", "coral_rose", "coral_violet", "coral_gold", "coral_white"].iter().filter_map(|n| w.lib.material(n)).collect();
    let sea = w.f.sea;
    // Padded cells whose centres may fall in voxels lo..hi along one axis
    // (one cell of slack each way; the exact test is per cell below).
    let span = |lo: i32, hi: i32, o: f32, n: usize| {
        let a = ((lo as f32 / VPM - o) / vsize - 1.0).floor().max(0.0) as usize;
        let b = ((hi as f32 / VPM - o) / vsize + 1.0).ceil().clamp(0.0, n as f32) as usize;
        a..b.max(a)
    };
    for &k in list {
        // Not standing just now (`place` `when`).
        if w.shown.get(k as usize).is_some_and(|s| !s.load(std::sync::atomic::Ordering::Relaxed)) {
            continue;
        }
        let p = &w.placed[k as usize];
        let s = &w.shapes[p.shape];
        let def = &w.lib.structures[p.def];
        let (x0, z0, x1, z1) = p.footprint(&w.shapes);
        let found = def.foundation;
        let age = p.age * w.def.weathering;
        let wear = structure_wear(w, p);
        let moss = if def.decays && age > 0.0 { 0.5 * w.def.moss * (1.0 - (-age / 70.0).exp()) } else { 0.0 };
        let mseed = p.seed ^ 0x4d0;
        let ys = span(p.origin[1] - found, p.origin[1] + s.top, o.y, PADY);
        if ys.is_empty() {
            continue;
        }
        for pz in span(z0, z1, o.z, PAD) {
            for px in span(x0, x1, o.x, PAD) {
                let wx = ((o.x + (px as f32 + 0.5) * vsize) * VPM).floor() as i32;
                let wz = ((o.z + (pz as f32 + 0.5) * vsize) * VPM).floor() as i32;
                if wx < x0 || wx >= x1 || wz < z0 || wz >= z1 {
                    continue;
                }
                let (lx, lz) = unrotate(wx - x0, wz - z0, s.size, p.rot);
                let ground = info[pz * PAD + px].h;
                for py in ys.clone() {
                    let ym = o.y + (py as f32 + 0.5) * vsize;
                    let wy = (ym * VPM).floor() as i32;
                    let ly = wy - p.origin[1];
                    if ly >= s.top || ly < -found {
                        continue;
                    }
                    let i = pidx(px, py, pz);
                    if ly < 0 {
                        // Foundation under any solid footing.
                        let (m, _) = s.get(lx, 0, lz);
                        if m != AIR && m != CARVE && v[i] == AIR {
                            v[i] = m;
                            any = true;
                        }
                        continue;
                    }
                    let (mut m, part) = s.get(lx, ly, lz);
                    // A door on a structure that is kept up is a platform,
                    // not terrain (`platforms::doors_in`): the leaf stands
                    // in the opening and slides out of it, so the voxels
                    // must not be here as well. A ruin keeps its door,
                    // because a ruin's door does not open.
                    if p.age <= 0.0 && w.doors.contains(&m) {
                        continue;
                    }
                    if p.rot & 1 == 1 && m != CARVE {
                        // A quarter turn swaps which way logs lie.
                        m = matlib::grained(m, matlib::grain_axis(m).turned(p.rot));
                    }
                    if m == CARVE {
                        v[i] = AIR;
                        continue;
                    }
                    if m == AIR {
                        // Rubble from a fallen roof on the floor inside.
                        if wear > 0.3 && ly <= 2 && s.get(lx, 0, lz).0 != AIR && v[i] == AIR {
                            let r = s.roof[(lz * s.size[0] + lx) as usize];
                            let pile = unit(hash3(lx / 3, lz / 3, ly, p.seed ^ 0x77)) < wear * if ly == 1 { 0.45 } else { 0.15 };
                            if r != AIR && pile && !roof_kept(s, p.seed, lx, lz, wear) {
                                v[i] = r;
                                any = true;
                            }
                        }
                        continue;
                    }
                    // Buried by later deposits: leave the ground. A building's
                    // own underground part (below its anchor) is drawn anyway.
                    if ym < ground - 0.05 && v[i] != AIR && !(def.level && ly < def.anchor[1]) {
                        continue;
                    }
                    // Decayed, or fallen with what held it up (`standing_map`).
                    if wear > 0.0 && !still_standing(w, k as usize, lx, ly, lz) {
                        continue;
                    }
                    let col_top = (s.col_top[(lz * s.size[0] + lx) as usize] - if def.level { def.anchor[1] } else { 0 }).max(1) as f32;
                    // Moss in patches, heavier low down.
                    if let Some(mm) = mossy {
                        if moss > 0.0 && (part == PART_STONE || part == PART_WALL) {
                            let n = perlin3(wx as f32 * 0.13, wy as f32 * 0.2, wz as f32 * 0.13, mseed) * 0.5 + 0.5;
                            let low = 1.0 - ly as f32 / col_top;
                            if n + low * 0.35 > 1.1 - moss {
                                m = mm;
                            }
                        }
                    }
                    // Sunk: crusted in broad patches all over, wood and iron
                    // too -- reef rock, and on the thickest of it one coral
                    // colour for the whole wreck. (A colour per block of
                    // voxels read as confetti.)
                    if !crusts.is_empty() && p.age > 0.0 && ym < sea - 0.6 {
                        let n = perlin3(wx as f32 * 0.12, wy as f32 * 0.2, wz as f32 * 0.12, mseed ^ 0xc0) * 0.5 + 0.5;
                        if n > 0.55 {
                            let coral = if crusts.len() > 1 { crusts[1 + (mseed as usize) % (crusts.len() - 1)] } else { crusts[0] };
                            m = if n > 0.7 { coral } else { crusts[0] };
                        }
                    }
                    v[i] = m;
                    any = true;
                }
            }
        }
    }
    any
}

/// The largest axis distance (L-infinity) from `p` to the nearest point of
/// segment a-b: under half a voxel means the segment passes through p's voxel.
pub(crate) fn seg_box_dist(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    (p - (a + ab * t)).abs().max_element()
}

pub(crate) fn seg_dist2(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (a + ab * t - p).length_squared()
}

/// Write edits from this chunk and its 26 neighbours into a padded array.
/// Returns true if any edit made a voxel solid.
pub fn apply_edits(v: &mut [u8; PAD3], cx: i32, cy: i32, cz: i32, edits: &EditMap) -> bool {
    let mut any = false;
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let Some(list) = edits.get(&(cx + dx, cy + dy, cz + dz)) else {
                    continue;
                };
                for (&i, &m) in list {
                    let i = i as i32;
                    let (lx, lz, ly) = (i % 32, (i / 32) % 32, i / 1024);
                    let px = lx + dx * 32 + M as i32;
                    let py = ly + dy * 32 + MY as i32;
                    let pz = lz + dz * 32 + M as i32;
                    if (0..PAD as i32).contains(&px) && (0..PADY as i32).contains(&py) && (0..PAD as i32).contains(&pz) {
                        v[pidx(px as usize, py as usize, pz as usize)] = m;
                        any |= m != AIR;
                    }
                }
            }
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An edit deep under the ground or high above it sits in a chunk the
    /// column would otherwise skip as all rock or all air; it must still be
    /// built, or the edit is lost each time the column is generated.
    #[test]
    fn edits_survive_in_skipped_chunks() {
        let g = Generator::new(WorldSpec::default());
        let (spawn, _) = g.spawn_point();
        let key = ColKey {
            lod: 0,
            x: (spawn.x * VPM) as i32 >> 5,
            z: (spawn.z * VPM) as i32 >> 5,
        };
        let plain = g.gen_column(key, None);
        assert!(plain.solid_below > 0, "spawn column should have solid chunks below its surface");
        let top_cy = plain.count - 1;
        assert!(plain.chunks.iter().all(|c| c.0 != 0 && c.0 != top_cy), "test needs skipped chunks at the bottom and top");
        // A dug voxel in the bottom chunk and a placed one in the top chunk.
        let mut edits = EditMap::default();
        edits.entry((key.x, 0, key.z)).or_default().insert((5 * 1024 + 5 * 32 + 5) as u16, AIR);
        edits.entry((key.x, top_cy, key.z)).or_default().insert((20 * 1024 + 5 * 32 + 5) as u16, STONE);
        let col = g.gen_column(key, Some(&edits));
        let at = |cy: i32, y: usize| col.chunks.iter().find(|c| c.0 == cy).map(|c| c.1[pidx(5 + M, y + MY, 5 + M)]);
        assert_eq!(at(0, 5), Some(AIR), "the dug voxel deep underground");
        assert_eq!(at(top_cy, 20), Some(STONE), "the placed voxel high above the terrain");
        assert_eq!(col.solid_below, plain.solid_below);
    }

    #[test]
    fn world_has_sea_land_and_hills() {
        let g = Generator::new(WorldSpec::default());
        let hs = g.height_grid(128);
        let sea = g.spec.sea_level_m;
        let below = hs.iter().filter(|&&h| h < sea).count() as f32 / hs.len() as f32;
        let max = hs.iter().cloned().fold(f32::MIN, f32::max);
        assert!(below > 0.2 && below < 0.75, "sea fraction {below}");
        assert!(max > sea + 25.0, "highest point {max}");
    }

    #[test]
    fn lod_columns_are_consistent() {
        let g = Generator::new(WorldSpec::default());
        let (p, _) = g.spawn_point();
        let key = ColKey {
            lod: 0,
            x: (p.x * VPM) as i32 / 32,
            z: (p.z * VPM) as i32 / 32,
        };
        let cg = g.gen_column(key, None);
        assert!(!cg.chunks.is_empty());
        assert!(cg.solid_below > 0);
        assert_eq!(cg.count, 16);
    }
}

#[cfg(test)]
mod ground_cache_tests {
    use super::*;

    /// The column-height cache must answer exactly what a fresh evaluation
    /// would, for any column: the slot is a hash of both axes, so a wrong
    /// mix would hand one column another's ground and bodies would stand in
    /// the air a few metres away.
    #[test]
    fn cached_column_heights_match_the_real_ones() {
        let gen = Generator::new(crate::world::spec::WorldSpec::default());
        // Enough columns to evict each slot many times over, and in an order
        // that revisits them.
        for pass in 0..3 {
            for k in 0..40_000i32 {
                let (x, z) = ((k * 37) % 5_000 - 2_500, (k * 101) % 5_000 - 2_500);
                let want = gen.column((x as f32 + 0.5) * VOXEL, (z as f32 + 0.5) * VOXEL).h;
                let got = gen.column_h(x, z);
                assert_eq!(got.to_bits(), want.to_bits(), "pass {pass} at {x},{z}");
            }
        }
    }

    /// A second world's cache does not answer for the first: each generator
    /// gets its own epoch.
    #[test]
    fn another_world_does_not_inherit_the_cache() {
        let a = Generator::new(crate::world::spec::WorldSpec { seed: 1, ..Default::default() });
        let b = Generator::new(crate::world::spec::WorldSpec { seed: 2, ..Default::default() });
        let mut differ = 0;
        for k in 0..200i32 {
            let (x, z) = (k * 13, k * 7);
            let ha = a.column_h(x, z);
            let hb = b.column_h(x, z);
            assert_eq!(hb.to_bits(), b.column((x as f32 + 0.5) * VOXEL, (z as f32 + 0.5) * VOXEL).h.to_bits());
            assert_eq!(a.column_h(x, z).to_bits(), ha.to_bits());
            differ += (ha != hb) as u32;
        }
        // Half of them are flat sea in both worlds; what matters is that the
        // heights that do differ are each world's own.
        assert!(differ > 50, "two seeds should mostly differ, {differ} of 200 did");
    }
}
