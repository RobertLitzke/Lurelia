//! Entity models: small voxel grids built in code and meshed by the terrain mesher.
//! Each model has its own voxel size, finer than the terrain's, so a rabbit still
//! has a face. Models face -Z; the pivot is the bottom centre.

use crate::mesh::Mesh;
use crate::world::gen::{pidx, M, MY, PAD3};
use crate::world::material::PALETTE_BASE;
use glam::Vec3;

// Palette indices (material = PALETTE_BASE + index).
const DEER: u8 = 0;
const DEER_BELLY: u8 = 1;
const DARK: u8 = 2;
const WHITE: u8 = 3;
const ANTLER: u8 = 4;
const RABBIT: u8 = 5;
const RABBIT_LIGHT: u8 = 6;
const PINK: u8 = 7;
const FOX: u8 = 8;
const FOX_WHITE: u8 = 9;
const FOX_BLACK: u8 = 10;
const GEM: u8 = 11;
const GEM_LIGHT: u8 = 12;
const GULL: u8 = 13;
const GULL_GREY: u8 = 14;
const BEAK: u8 = 15;
const WINGTIP: u8 = 16;

/// sRGB bytes and emissive strength to the palette's linear form.
pub fn srgb(r: u8, g: u8, b: u8, e: f32) -> [f32; 4] {
    let f = |c: u8| (c as f32 / 255.0).powf(2.2);
    [f(r), f(g), f(b), e]
}

/// The fixed block of the model palette (FIXED_SLOTS colours) with the
/// built-in models' colours filled in (linear RGB and emissive strength). The
/// ride's colours (ride::palette) are added by the game, and the world's
/// library models' blocks are appended after it.
pub fn palette() -> Vec<[f32; 4]> {
    let mut p = vec![[1.0, 0.0, 1.0, 0.0]; FIXED_SLOTS];
    p[DEER as usize] = srgb(150, 98, 60, 0.0);
    p[DEER_BELLY as usize] = srgb(210, 180, 140, 0.0);
    p[DARK as usize] = srgb(36, 28, 24, 0.0);
    p[WHITE as usize] = srgb(240, 236, 228, 0.0);
    p[ANTLER as usize] = srgb(214, 196, 160, 0.0);
    p[RABBIT as usize] = srgb(140, 118, 96, 0.0);
    p[RABBIT_LIGHT as usize] = srgb(210, 196, 176, 0.0);
    p[PINK as usize] = srgb(220, 150, 150, 0.0);
    p[FOX as usize] = srgb(214, 104, 34, 0.0);
    p[FOX_WHITE as usize] = srgb(240, 232, 220, 0.0);
    p[FOX_BLACK as usize] = srgb(44, 32, 28, 0.0);
    p[GEM as usize] = srgb(60, 220, 230, 1.6);
    p[GEM_LIGHT as usize] = srgb(190, 250, 255, 2.4);
    p[GULL as usize] = srgb(244, 244, 240, 0.0);
    p[GULL_GREY as usize] = srgb(150, 158, 168, 0.0);
    p[BEAK as usize] = srgb(240, 190, 40, 0.0);
    p[WINGTIP as usize] = srgb(30, 30, 34, 0.0);
    p
}

pub struct Model {
    pub mesh: Mesh,
    /// Model voxel edge, metres.
    pub voxel: f32,
    /// Pivot in model voxel units (bottom centre).
    pub pivot: Vec3,
    /// Half-width and height of the collision box, metres.
    pub half_w: f32,
    pub height: f32,
    /// How far the body runs either side of the feet along the model's own
    /// z, metres before any morph scale: (low z, high z). A model faces
    /// -Z, so `reach[0]` is the snout end and `reach[1]` the rump. The
    /// collision box is a square `half_w` across, which is right for a deer
    /// and a fifth of a triceratops; herds and crowds space by this instead
    /// (creatures/crowd.rs). `[-half_w, half_w]` unless the loader measured it.
    pub reach: [f32; 2],
    /// Library creatures with a rig: drawn as parts (src/anim).
    pub rig: Option<crate::anim::Rig>,
    /// The same rig, with each bone's part replaced by the bones inside it
    /// (anim/bones.rs): what a deceased one is drawn with.
    pub skeleton: Option<crate::anim::Rig>,
    /// Where this model's colours start in the palette: a voxel's colour is
    /// `palette[block + material - PALETTE_BASE]`. 0 for the built-in models,
    /// the ride, the guns and the bots, which use the fixed block; a library
    /// model has a block of its own (see FIXED_SLOTS).
    pub block: u32,
    /// The levels of detail of a library creature's model or part: its
    /// cells until `bake_all` runs, then the baked meshes. None for
    /// everything else, which is always drawn as built.
    pub lod: Option<Box<Lods>>,
}

/// A creature model's other meshes (docs/CREATURES.md "Model resolution and
/// decimation"). Smooth mode draws `near` up close and `far` beyond the
/// switch; voxel mode draws the model's own quads up close and `coarse`
/// beyond. The renderer picks per instance (render/mod.rs `lod_of`).
#[derive(Default)]
pub struct Lods {
    /// The 32^3 cells the model was meshed from, kept until the bake.
    pub cells: Vec<u8>,
    /// A rig's part: its smooth meshes come from its creature's `body`
    /// bake, not from its own cells, so it is one surface with the rest.
    pub of_body: bool,
    /// On a rigged creature's whole model: the bake that makes its parts'
    /// smooth meshes.
    pub body: Option<BodyBake>,
    /// The model merged 2x2x2 (majority colour), as quads in the model's own
    /// voxel units.
    pub coarse: Mesh,
    /// Decimated triangle meshes, packed vertices (mesh/decimate.rs): half
    /// and a quarter of the square model's triangles.
    pub near: Vec<crate::mesh::decimate::Vert>,
    pub far: Vec<crate::mesh::decimate::Vert>,
}

/// A rigged creature's whole body, to be baked as one surface and split
/// into its parts (mesh/decimate.rs `Body`).
pub struct BodyBake {
    pub cells: Vec<u8>,
    pub dims: [i32; 3],
    pub owner: Vec<u8>,
    pub parent: Vec<Option<usize>>,
    /// Bones baked as surfaces of their own (`BoneDef::loose`).
    pub loose: Vec<bool>,
    pub origins: Vec<[i32; 3]>,
    /// Each bone's part model, as an index into the models being baked.
    pub parts: Vec<usize>,
}

impl Model {
    /// Keep this model's cells for `bake_all`: a creature's model or part.
    pub fn for_lod(mut self, cells: &[u8]) -> Model {
        if !self.mesh.quads.is_empty() {
            self.lod = Some(Box::new(Lods { cells: cells.to_vec(), ..Default::default() }));
        }
        self
    }
}

enum Baked {
    /// Model k's coarse mesh, and its own near and far if it is not a part.
    One(usize, Mesh, Option<(Vec<crate::mesh::decimate::Vert>, Vec<crate::mesh::decimate::Vert>)>),
    /// Each part's near and far from a body bake.
    Parts(Vec<(usize, Vec<crate::mesh::decimate::Vert>, Vec<crate::mesh::decimate::Vert>)>),
}

/// Bake every model that kept its cells (`Model::for_lod`) and every rigged
/// creature's body, on every core: the smooth near and far meshes and the
/// 2x2x2 voxel mesh.
pub fn bake_all(models: &mut [Model]) {
    bake_with(models, Vec::new());
}

/// `bake_all`, and these bodies as well: rigs whose whole model is not in
/// `models` (the bots' builds, brain/avatar.rs), their `parts` indices into
/// `models`.
pub fn bake_with(models: &mut [Model], extra: Vec<BodyBake>) {
    #[derive(Clone, Copy)]
    enum Job {
        One(usize),
        Body(usize),
        Extra(usize),
    }
    let mut jobs: Vec<Job> = Vec::new();
    for (k, m) in models.iter().enumerate() {
        if let Some(l) = &m.lod {
            if !l.cells.is_empty() {
                jobs.push(Job::One(k));
            }
            if l.body.is_some() {
                jobs.push(Job::Body(k));
            }
        }
    }
    jobs.extend((0..extra.len()).map(Job::Extra));
    if jobs.is_empty() {
        return;
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::with_capacity(jobs.len()));
    {
        let models: &[Model] = models;
        std::thread::scope(|sc| {
            for _ in 0..threads.min(jobs.len()) {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&job) = jobs.get(i) else { break };
                    let r = match job {
                        Job::One(k) => bake_one(&models[k], k),
                        Job::Body(k) => bake_body(models, models[k].lod.as_ref().and_then(|l| l.body.as_ref()).expect("a body to bake")),
                        Job::Extra(e) => bake_body(models, &extra[e]),
                    };
                    done.lock().unwrap().push(r);
                });
            }
        });
    }
    for r in done.into_inner().unwrap() {
        match r {
            Baked::One(k, coarse, own) => {
                if let Some(l) = models[k].lod.as_mut() {
                    l.cells = Vec::new();
                    l.coarse = coarse;
                    if let Some((near, far)) = own {
                        l.near = near;
                        l.far = far;
                    }
                }
            }
            Baked::Parts(parts) => {
                for (k, near, far) in parts {
                    if let Some(l) = models.get_mut(k).and_then(|m| m.lod.as_mut()) {
                        l.near = near;
                        l.far = far;
                    }
                }
            }
        }
    }
    for m in models.iter_mut() {
        if let Some(l) = m.lod.as_mut() {
            l.body = None;
        }
    }
}

/// The square model draws two triangles a quad; the near mesh has half
/// that, the far a quarter.
fn bake_one(m: &Model, k: usize) -> Baked {
    let l = m.lod.as_ref().expect("a model to bake");
    let own = (!l.of_body).then(|| {
        let tris = 2 * m.mesh.quads.len();
        crate::mesh::decimate::bake(&l.cells, (tris / 2).max(12), (tris / 4).max(8))
    });
    Baked::One(k, coarse(&l.cells), own)
}

/// A rigged creature baked as one surface, the budget the sum of its parts'.
fn bake_body(models: &[Model], b: &BodyBake) -> Baked {
    let tris: usize = b.parts.iter().map(|&p| 2 * models.get(p).map_or(0, |m| m.mesh.quads.len())).sum();
    let body = crate::mesh::decimate::Body { cells: &b.cells, dims: b.dims, owner: Some(&b.owner), parent: &b.parent, loose: &b.loose, origins: &b.origins };
    let meshes = crate::mesh::decimate::bake_body(&body, (tris / 2).max(12), (tris / 4).max(8));
    Baked::Parts(b.parts.iter().zip(meshes).map(|(&p, (near, far))| (p, near, far)).collect())
}

/// The cells merged 2x2x2: a block is solid when a quarter of it is, and
/// takes its commonest colour. Meshed at half size, then put back in the
/// model's own voxel units (each coordinate and size doubled).
fn coarse(cells: &[u8]) -> Mesh {
    let mut g = Grid::new();
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                let mut seen = [0u8; 8];
                let mut n = 0;
                for k in 0..8 {
                    let (dx, dy, dz) = (k & 1, k >> 1 & 1, k >> 2 & 1);
                    let c = cells[(((2 * y + dy) * 32 + 2 * z + dz) * 32 + 2 * x + dx) as usize];
                    seen[k as usize] = c;
                    n += (c != 0) as u32;
                }
                if n >= 2 {
                    let best = seen.iter().filter(|&&c| c != 0).max_by_key(|&&c| seen.iter().filter(|&&d| d == c).count()).copied().unwrap_or(0);
                    g.set(x, y, z, best - 1);
                }
            }
        }
    }
    let mut mesh = Mesh::default();
    crate::mesh::mesh_model_t(&g.v, &[], &mut mesh);
    for q in mesh.quads.iter_mut() {
        let [px, py, pz] = crate::mesh::quad::pos(q);
        let d = crate::mesh::quad::dir(q);
        let (w, h) = crate::mesh::quad::size(q);
        // A face on the positive side of its voxel sits a voxel further out
        // (the shader adds the normal), so it lands at 2c + 2 from 2c + 1.
        let along = d / 2;
        let positive = d % 2 == 0;
        let mut p = [2 * px, 2 * py, 2 * pz];
        if positive {
            p[along] += 1;
        }
        let keep = q[0] & !(0x1ff_ffff);
        q[0] = keep | crate::mesh::quad::word0(p, 2 * w, 2 * h, d, false) & 0x1ff_ffff;
    }
    mesh
}

pub(crate) struct Grid {
    v: Box<[u8; PAD3]>,
    /// Rods drawn as round tubes along their lines (`rod`).
    threads: Vec<crate::mesh::Thread>,
}

impl Grid {
    pub(crate) fn new() -> Self {
        Self {
            v: vec![0u8; PAD3].into_boxed_slice().try_into().unwrap(),
            threads: Vec::new(),
        }
    }
    /// A rod from a to b (grid voxel coordinates, a voxel's centre at +0.5),
    /// `r` voxels thick: its voxels, as `ride::seg` fills them, and a thread
    /// along it, so it is meshed as a smooth round tube (mesh/lean.rs) at any
    /// angle, along an axis too.
    pub(crate) fn rod(&mut self, a: [f32; 3], b: [f32; 3], r: f32, c: u8) {
        let (va, vb) = (Vec3::from(a), Vec3::from(b));
        let ab = vb - va;
        let hull = r.max(0.5);
        let lo = va.min(vb) - Vec3::splat(hull + 1.0);
        let hi = va.max(vb) + Vec3::splat(hull + 1.0);
        for y in lo.y.floor() as i32..=hi.y.ceil() as i32 {
            for z in lo.z.floor() as i32..=hi.z.ceil() as i32 {
                for x in lo.x.floor() as i32..=hi.x.ceil() as i32 {
                    let pt = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    // Flat ends at a and b, as the tube is drawn: no
                    // rounded cap reaching half a voxel past them.
                    let t = (pt - va).dot(ab) / ab.length_squared().max(1e-6);
                    if (0.0..=1.0).contains(&t) && (va + ab * t - pt).length() <= hull {
                        self.set(x, y, z, c);
                    }
                }
            }
        }
        let off = Vec3::new(M as f32, MY as f32, M as f32);
        self.threads.push(crate::mesh::Thread {
            a: va + off,
            b: vb + off,
            r,
            hull,
            mat: PALETTE_BASE + c,
            lone: false,
            owned: false,
        });
    }
    pub(crate) fn set(&mut self, x: i32, y: i32, z: i32, c: u8) {
        if (0..32).contains(&x) && (0..32).contains(&y) && (0..32).contains(&z) {
            self.v[pidx(x as usize + M, y as usize + MY, z as usize + M)] = PALETTE_BASE + c;
        }
    }
    pub(crate) fn fill(&mut self, a: [i32; 3], b: [i32; 3], c: u8) {
        for y in a[1]..b[1] {
            for z in a[2]..b[2] {
                for x in a[0]..b[0] {
                    self.set(x, y, z, c);
                }
            }
        }
    }
    pub(crate) fn ellipsoid(&mut self, centre: [f32; 3], r: [f32; 3], c: u8) {
        for y in 0..32 {
            for z in 0..32 {
                for x in 0..32 {
                    let d = ((x as f32 + 0.5 - centre[0]) / r[0]).powi(2) + ((y as f32 + 0.5 - centre[1]) / r[1]).powi(2) + ((z as f32 + 0.5 - centre[2]) / r[2]).powi(2);
                    if d <= 1.0 {
                        self.set(x, y, z, c);
                    }
                }
            }
        }
    }
    /// Recolour solid voxels below a height (bellies, socks).
    fn recolour_below(&mut self, y_max: i32, from: u8, to: u8) {
        for y in 0..y_max.min(32) {
            for z in 0..32 {
                for x in 0..32 {
                    let i = pidx(x + M, y as usize + MY, z + M);
                    if self.v[i] == PALETTE_BASE + from {
                        self.v[i] = PALETTE_BASE + to;
                    }
                }
            }
        }
    }
    /// The box the filled voxels actually occupy, in grid coordinates:
    /// `None` for an empty grid. What a shape is as opposed to what it was
    /// drawn inside, which is the only way to ask whether two models have
    /// the same silhouette.
    #[cfg(test)]
    pub(crate) fn bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
        for z in 0..32 {
            for y in 0..32 {
                for x in 0..32 {
                    if self.v[pidx(x as usize + M, y as usize + MY, z as usize + M)] != 0 {
                        for (k, v) in [x, y, z].into_iter().enumerate() {
                            lo[k] = lo[k].min(v);
                            hi[k] = hi[k].max(v);
                        }
                    }
                }
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    pub(crate) fn build(self, voxel: f32, half_w: f32, height: f32) -> Model {
        let mut mesh = Mesh::default();
        crate::mesh::mesh_model_t(&self.v, &self.threads, &mut mesh);
        Model {
            mesh,
            voxel,
            pivot: Vec3::new(16.0, 0.0, 16.0),
            half_w,
            height,
            reach: [-half_w, half_w],
            rig: None,
            skeleton: None,
            block: 0,
            lod: None,
        }
    }
}

pub const DEER_M: usize = 0;
pub const RABBIT_M: usize = 1;
pub const FOX_M: usize = 2;
pub const GEM_M: usize = 3;
pub const GULL_M: usize = 4;

/// How many colours one model can use. A model voxel's material is
/// PALETTE_BASE + slot, so 192 slots fit in a u8; the slot counts from the
/// model's own `block` in the palette.
pub const PALETTE_SLOTS: usize = 192;

/// The palette's first block, shared by every model whose `block` is 0:
/// 0..17 are the built-in models' colours, 18..38 the guns
/// (combat/viewmodel.rs), 38..48 the bots (brain/avatar.rs), 48..64 the
/// ride (ride.rs) and 64..80 the arena guns' paint (combat/real). Each
/// library creature, collectible and platform model gets a block of its own
/// after this one, so a world has no limit on its colours, only each model
/// (PALETTE_SLOTS).
pub const FIXED_SLOTS: usize = 80;

/// The slot (plus 1, as `from_cells` wants it) of colour `lin` in the block
/// that starts at `colours[start]`, appending it if the block lacks it. A
/// slot past PALETTE_SLOTS is clamped to the last one; `block_problem` says
/// when that happened.
pub fn block_slot(colours: &mut Vec<[f32; 4]>, start: usize, lin: [f32; 4]) -> u8 {
    let k = colours[start..].iter().position(|x| x == &lin).unwrap_or_else(|| {
        colours.push(lin);
        colours.len() - 1 - start
    });
    k.min(PALETTE_SLOTS - 1) as u8 + 1
}

/// The complaint for a block of `n` colours, if one model cannot hold them.
pub fn block_problem(name: &str, n: usize) -> Option<String> {
    (n > PALETTE_SLOTS).then(|| format!("{name}: {n} colours, and one model can use {PALETTE_SLOTS} at most"))
}

/// sRGB bytes (and optional emissive strength) to the palette's linear form.
pub fn linear_colour(c: &[f32]) -> [f32; 4] {
    let f = |v: f32| (v / 255.0).clamp(0.0, 1.0).powf(2.2);
    [f(c[0]), f(c.get(1).copied().unwrap_or(0.0)), f(c.get(2).copied().unwrap_or(0.0)), c.get(3).copied().unwrap_or(0.0)]
}

/// A model from a 32^3 grid of palette slots + 1 (0 is empty), x fastest then z then y.
pub fn from_cells(cells: &[u8], voxel: f32, half_w: f32, height: f32) -> Model {
    let mut g = Grid::new();
    for y in 0..32 {
        for z in 0..32 {
            for x in 0..32 {
                let c = cells[((y * 32 + z) * 32 + x) as usize];
                if c != 0 {
                    g.set(x, y, z, c - 1);
                }
            }
        }
    }
    g.build(voxel, half_w, height)
}

/// The whole model of a creature's grid of any size (`anim::slice::Dims`). A
/// 32^3 grid is meshed as `from_cells` does it. A bigger one has no mesh of
/// its own, since a mesh holds 32^3 at most: it is drawn by its rig's parts
/// alone, each meshed in a window of its own, and keeps only its voxel size
/// and its pivot at the grid's bottom centre, which the parts are posed from.
pub fn from_grid(cells: &[u8], dims: crate::anim::slice::Dims, voxel: f32, half_w: f32, height: f32) -> Model {
    if dims == crate::anim::slice::Dims::CUBE {
        return from_cells(cells, voxel, half_w, height);
    }
    let mut m = Grid::new().build(voxel, half_w, height);
    m.pivot = dims.pivot();
    m
}

pub fn build_all() -> Vec<Model> {
    vec![deer(), rabbit(), fox(), gem(), gull()]
}

fn deer() -> Model {
    let mut g = Grid::new();
    g.ellipsoid([16.0, 15.0, 17.0], [4.2, 4.0, 8.5], DEER);
    g.recolour_below(13, DEER, DEER_BELLY);
    for (x, z) in [(13, 11), (18, 11), (13, 22), (18, 22)] {
        g.fill([x, 2, z], [x + 2, 13, z + 2], DEER);
        g.fill([x, 0, z], [x + 2, 2, z + 2], DARK);
    }
    // Neck and head.
    g.fill([15, 16, 8], [18, 23, 11], DEER);
    g.fill([14, 21, 3], [19, 26, 9], DEER);
    g.fill([15, 21, 2], [18, 24, 3], DARK);
    g.set(14, 24, 5, DARK);
    g.set(18, 24, 5, DARK);
    // Ears and antlers.
    g.fill([12, 25, 7], [14, 27, 8], DEER);
    g.fill([19, 25, 7], [21, 27, 8], DEER);
    for (x, dx) in [(15, -1), (17, 1)] {
        g.fill([x, 26, 6], [x + 1, 30, 7], ANTLER);
        g.set(x + dx, 29, 6, ANTLER);
        g.set(x + dx * 2, 30, 6, ANTLER);
        g.set(x, 28, 5, ANTLER);
    }
    g.fill([15, 15, 25], [18, 18, 27], WHITE);
    g.build(1.0 / 16.0, 0.28, 1.4)
}

fn rabbit() -> Model {
    let mut g = Grid::new();
    g.ellipsoid([16.0, 7.5, 17.5], [4.0, 4.5, 6.0], RABBIT);
    g.recolour_below(5, RABBIT, RABBIT_LIGHT);
    g.ellipsoid([16.0, 11.5, 11.0], [3.0, 3.0, 3.2], RABBIT);
    g.fill([14, 13, 11], [15, 21, 12], RABBIT);
    g.fill([17, 13, 11], [18, 21, 12], RABBIT);
    g.fill([14, 14, 10], [15, 20, 11], PINK);
    g.fill([17, 14, 10], [18, 20, 11], PINK);
    g.set(13, 12, 9, DARK);
    g.set(18, 12, 9, DARK);
    g.set(16, 11, 7, PINK);
    g.ellipsoid([16.0, 8.5, 24.0], [1.8, 1.8, 1.8], WHITE);
    g.fill([13, 0, 20], [15, 3, 24], RABBIT);
    g.fill([17, 0, 20], [19, 3, 24], RABBIT);
    g.build(1.0 / 32.0, 0.12, 0.35)
}

fn fox() -> Model {
    let mut g = Grid::new();
    g.ellipsoid([16.0, 10.0, 15.0], [3.2, 3.4, 7.0], FOX);
    g.recolour_below(8, FOX, FOX_WHITE);
    for (x, z) in [(14, 10), (17, 10), (14, 19), (17, 19)] {
        g.fill([x, 0, z], [x + 1, 8, z + 1], FOX_BLACK);
    }
    // Head and snout.
    g.ellipsoid([16.0, 13.5, 7.0], [3.0, 2.6, 3.0], FOX);
    g.fill([15, 11, 2], [18, 14, 5], FOX);
    g.fill([15, 11, 2], [18, 12, 5], FOX_WHITE);
    g.set(16, 13, 1, FOX_BLACK);
    g.set(14, 14, 5, FOX_BLACK);
    g.set(18, 14, 5, FOX_BLACK);
    g.fill([13, 15, 7], [15, 19, 8], FOX);
    g.fill([17, 15, 7], [19, 19, 8], FOX);
    g.set(13, 18, 7, FOX_BLACK);
    g.set(18, 18, 7, FOX_BLACK);
    // Tail.
    g.ellipsoid([16.0, 10.0, 25.5], [2.2, 2.2, 4.5], FOX);
    g.ellipsoid([16.0, 10.0, 29.5], [1.6, 1.6, 1.8], FOX_WHITE);
    g.build(1.0 / 20.0, 0.18, 0.75)
}

fn gem() -> Model {
    let mut g = Grid::new();
    for y in 0..20 {
        for z in 0..32 {
            for x in 0..32 {
                let d = (x as f32 + 0.5 - 16.0).abs() + (z as f32 + 0.5 - 16.0).abs() + (y as f32 + 0.5 - 10.0).abs() * 0.7;
                if d <= 7.0 {
                    g.set(x, y, z, if d < 3.5 || y > 12 { GEM_LIGHT } else { GEM });
                }
            }
        }
    }
    g.build(1.0 / 40.0, 0.2, 0.5)
}

fn gull() -> Model {
    let mut g = Grid::new();
    g.ellipsoid([16.0, 8.0, 16.0], [2.4, 2.4, 6.0], GULL);
    g.ellipsoid([16.0, 9.5, 10.0], [2.0, 2.0, 2.2], GULL);
    g.fill([15, 9, 6], [17, 10, 8], BEAK);
    g.set(14, 10, 9, WINGTIP);
    g.set(18, 10, 9, WINGTIP);
    // Wings: flat, tapering, dark tips.
    for x in 0..32i32 {
        let off = (x - 16).abs();
        if off < 2 {
            continue;
        }
        let depth = (6 - off / 3).max(2);
        let c = if off > 12 { WINGTIP } else { GULL_GREY };
        g.fill([x, 9, 14 - depth / 2], [x + 1, 10, 14 + depth / 2 + 1], c);
    }
    g.fill([15, 8, 21], [18, 9, 25], GULL_GREY);
    g.build(1.0 / 24.0, 0.3, 0.3)
}

#[cfg(test)]
mod tests {
    #[test]
    fn models_mesh() {
        for m in super::build_all() {
            assert!(!m.mesh.quads.is_empty());
        }
    }
}

/// One drawn copy of a model: the renderer uploads these as they are.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct Instance {
    pub pos_yaw: [f32; 4],
    pub info: [f32; 4],
    pub tint: [f32; 4],
    /// Rotation quaternion (x, y, z, w) applied in model space before the yaw;
    /// all zeros means none.
    pub rot: [f32; 4],
    /// Added to the colour after `tint` multiplies it, so a coat can repaint
    /// and not only darken (variety::Repaint). Zero leaves the model alone.
    /// The fourth value flags the shadow: 1 casts a shadow but is never
    /// drawn, 2 is drawn but casts none (ride.rs `SHADOW_ONLY`/`NO_SHADOW`).
    /// `GLASS` and up is drawn see-through and casts none (`glass`).
    /// Below 0 the shadow is cast thinned, -w of it (a bird high over the
    /// ground, entities.rs `shadow_fade`).
    pub shift: [f32; 4],
    /// A soft body's flesh (anim/flesh.rs `Soft::instance`); all zero on
    /// anything rigid, which the shaders skip.
    pub flesh: [[f32; 4]; 3],
    /// `[0]`: how much brighter than its palette says this copy's glowing
    /// colours burn, as a gain less one, so all zeros draws it as built. A
    /// named coat sets it (variety::Repaint `glow`): the lantern moth is the
    /// glassmoth's model with its wings burning a third brighter. It only
    /// scales a glow the model has; nothing here lights a colour that does
    /// not glow.
    ///
    /// `[1..]`: on a rig's part, the parent bone's rotation as the x, y, z
    /// of a quaternion whose w is kept at or above zero, for the seam of
    /// its smooth mesh, which follows the parent (anim/mod.rs
    /// `push_parts_at`, mesh/decimate.rs). Zero is no rotation.
    pub glow: [f32; 4],
    /// `[0]`: how much of it has dissolved away, 0 drawn whole to 1 gone: a
    /// creature appearing or at the far edge of where it is kept
    /// (entities.rs `instances`). Above 0 it draws in the dissolving
    /// pipeline (scene.wgsl `fs_entity_fade`). The rest is unused.
    pub fade: [f32; 4],
}

/// `Instance::shift[3]` at and above this draws the instance see-through:
/// blended over what is behind it, writing no depth and casting no shadow.
/// What is over `GLASS` is its opacity, 0 to 1.
pub const GLASS: f32 = 3.0;

impl Instance {
    /// Draw this see-through, `alpha` of it opaque (more at grazing angles,
    /// where the edges hold the shape: scene.wgsl `fs_entity_glass`).
    pub fn glass(mut self, alpha: f32) -> Instance {
        self.shift[3] = GLASS + alpha.clamp(0.0, 1.0);
        self
    }
}
