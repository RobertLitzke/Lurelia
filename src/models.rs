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

/// The model palette with the built-in models' colours filled in (linear RGB
/// and emissive strength). The ride's colours (ride::palette) and the world's
/// library colours (LIBRARY_COLOURS) are added by the game.
pub fn palette() -> Vec<[f32; 4]> {
    let mut p = vec![[1.0, 0.0, 1.0, 0.0]; PALETTE_SLOTS];
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
    /// Library creatures with a rig: drawn as parts (src/anim).
    pub rig: Option<crate::anim::Rig>,
    /// The same rig, with each bone's part replaced by the bones inside it
    /// (anim/bones.rs): what a deceased one is drawn with.
    pub skeleton: Option<crate::anim::Rig>,
}

pub(crate) struct Grid {
    v: Box<[u8; PAD3]>,
}

impl Grid {
    pub(crate) fn new() -> Self {
        Self {
            v: vec![0u8; PAD3].into_boxed_slice().try_into().unwrap(),
        }
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
        crate::mesh::mesh_model(&self.v, &mut mesh);
        Model {
            mesh,
            voxel,
            pivot: Vec3::new(16.0, 0.0, 16.0),
            half_w,
            height,
            rig: None,
            skeleton: None,
        }
    }
}

pub const DEER_M: usize = 0;
pub const RABBIT_M: usize = 1;
pub const FOX_M: usize = 2;
pub const GEM_M: usize = 3;
pub const GULL_M: usize = 4;

/// Model palette slots. A model voxel's material is PALETTE_BASE + slot, so
/// 192 fit in a u8. Slots 0..17 are the built-in models' colours, 17..48 are
/// free, 48..64 belong to the ride models (ride.rs), and the rest hold the
/// loaded world's library creature and collectible colours, first seen first.
pub const PALETTE_SLOTS: usize = 192;
pub const LIBRARY_COLOURS: std::ops::Range<usize> = 64..PALETTE_SLOTS;

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
    pub shift: [f32; 4],
}
