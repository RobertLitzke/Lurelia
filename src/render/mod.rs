//! The renderer. Terrain is one big buffer of packed quads that the vertex shader
//! expands; each frame the CPU culls chunks and face directions, writes indirect
//! draw arguments, and the whole terrain goes out in one multi-draw call per pass.

pub mod alloc;
pub mod aurora;
pub mod daylight;
pub mod foliage;
pub mod font;
pub mod gpuprof;
pub mod grade;
pub mod horizon;
pub mod hud;
pub mod lamps;
pub mod occlusion;
pub mod overlay;
pub mod particles;
pub mod plates;
pub mod recipes;
pub mod shaders;
pub mod signs;
pub mod sprite_plants;
pub mod sprite_tex;
pub mod texopts;
pub mod texsets;
pub mod textures;
pub mod ui;

use crate::mesh::Mesh;
use crate::stream::{ChunkSink, GpuChunk, Streamer};
use crate::world::material;
use alloc::RangeAlloc;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
pub use grade::Grade;
use hud::{Hud, HudInst};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Instant;
use wgpu::util::DeviceExt;

/// Most terrain quads (16 bytes each) the buffer may hold: 1 GiB. The real size is
/// this or what the device allows, whichever is less (`quad_capacity`). 256 MB was
/// full at the spawn of a dense 6.4 km city world, and chunks loaded after that were dropped.
pub const MAX_QUADS: u32 = 64 * 1024 * 1024;

/// Quads the terrain buffer holds on this device.
fn quad_capacity(device: &wgpu::Device) -> u32 {
    let l = device.limits();
    let bytes = l.max_buffer_size.min(l.max_storage_buffer_binding_size as u64);
    (bytes / 16).min(MAX_QUADS as u64) as u32
}
pub const MAX_SLOTS: u32 = 65536;
pub const SHADOW_SIZE: u32 = 2048;
/// Cells across the mist map (see `set_mist`).
pub const MIST_N: u32 = 256;
pub const MAX_ENTITY_QUADS: u32 = 1 << 18;
pub const MAX_INSTANCES: u32 = 8192;
pub const MAX_HUD: u32 = 16384;
/// Side of the heightmap the water and mist read (set_world).
pub const HEIGHTS_N: u32 = 512;
/// Most river surface cells a world can draw (one quad per 1 m cell).
pub const RIVER_CAP: usize = 1 << 18;
/// Pieces of spray (mist puffs and droplets) per waterfall; vs_spray in
/// scene.wgsl splits them.
const SPRAY: u32 = 96;

/// One metre cell of river surface: its corner (x, z), 1 if it only draws
/// foam over the sea, and the index + 1 of the `Spray` of a waterfall
/// churning it (0 for none), then per corner (x0z0, x1z0, x1z1, x0z1) the
/// water level, the bed under it and the current. Built by
/// `world::rivers::surface`. Waterfall sheets use the same record another
/// way (`world::rivers::fall_quads`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug, Default)]
pub struct RiverQuad {
    pub at: [f32; 4],
    pub level: [f32; 4],
    pub bed: [f32; 4],
    pub flow_x: [f32; 4],
    pub flow_z: [f32; 4],
    /// Per corner, how far the tide may draw the water down (`Wet::ebb`);
    /// unused by a fall's sheet.
    pub ebb: [f32; 4],
}

/// Where a waterfall lands, for its spray (vs_spray) and for the pool it
/// churns white (fs_water): `at` is the splash point and the fall's width,
/// `dir` the way the water leaves the lip (x, z), the drop and how hard it
/// lands (0.3 to 1.5). Built by `world::rivers::spray_spots`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug, Default)]
pub struct Spray {
    pub at: [f32; 4],
    pub dir: [f32; 4],
}

/// Vertices along a side of the open sea's mesh when it has a swell
/// (`vs_water`, `SEA_GRID` in scene.wgsl is the same).
pub const SEA_GRID: u32 = 192;

/// Most waterfalls a world can throw spray from.
pub const SPRAY_CAP: usize = 4096;
const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_DRAWS: u64 = MAX_SLOTS as u64 * 6 + MAX_SLOTS as u64 * 9 + MAX_SLOTS as u64;

/// Quads (terrain and sprites) are drawn indexed, four vertices each, with one
/// shared index buffer of (0, 1, 2) (0, 2, 3) patterns and each draw's start as
/// its base vertex. Drawn as six separate vertices each, every corner but one
/// pair ran the vertex shader twice; terrain is bound by vertex work.
const QUAD_INDEX: u32 = 1 << 17;

/// One indirect indexed draw: index count, instances, first index, base vertex,
/// first instance.
type Draw = [u32; 5];

/// Put draws in near-to-far order, so the depth test can reject what is
/// behind them before shading. Sorting ten thousand of them outright cost
/// 0.34 ms a frame; counting them into buckets four metres deep gives the
/// same order as far as the depth test can tell, in two passes. The key each
/// draw carries is its squared distance from the camera.
fn near_to_far(src: &mut Vec<(f32, Draw, u32)>, scratch: &mut Vec<(f32, Draw, u32)>) {
    /// Buckets, and how deep each is in metres: 2 km of them.
    const N: usize = 512;
    const DEPTH: f32 = 4.0;
    if src.len() < 256 {
        src.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        return;
    }
    let at = |d2: f32| ((d2.sqrt() * (1.0 / DEPTH)) as usize).min(N - 1);
    let mut start = [0u32; N];
    for (d, _, _) in src.iter() {
        start[at(*d)] += 1;
    }
    let mut sum = 0;
    for c in start.iter_mut() {
        let n = *c;
        *c = sum;
        sum += n;
    }
    scratch.clear();
    scratch.resize(src.len(), (0.0, [0; 5], 0));
    for e in src.iter() {
        let b = at(e.0);
        scratch[start[b] as usize] = *e;
        start[b] += 1;
    }
    std::mem::swap(src, scratch);
}

/// The draw for `n` quads from quad `start` of the terrain buffer.
/// Whether whole columns are thrown out before their chunks are walked
/// (VOXEL_COLUMN_CULL=0 turns it off).
fn column_cull() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_COLUMN_CULL").map(|v| v != "0").unwrap_or(true))
}

fn qdraw(n: u32, start: u32) -> Draw {
    debug_assert!(n <= QUAD_INDEX);
    [n.min(QUAD_INDEX) * 6, 1, 0, start * 4, 0]
}

pub struct Gpu {
    /// Held so the instance outlives the adapter and surface made from it.
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub multi_draw: bool,
    /// Timestamp queries are on (VOXEL_GPU_PROFILE=1; see gpuprof.rs).
    pub timestamps: bool,
}

impl Gpu {
    pub fn instance(dx12: bool) -> wgpu::Instance {
        let backends = if dx12 { wgpu::Backends::DX12 } else { wgpu::Backends::VULKAN };
        wgpu::Instance::new(&wgpu::InstanceDescriptor { backends, ..Default::default() })
    }

    pub fn new(instance: wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Self {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
        }))
        .expect("no suitable GPU adapter (Vulkan); try --dx12");
        let feats = adapter.features();
        let multi_draw = feats.contains(wgpu::Features::MULTI_DRAW_INDIRECT);
        let mut required = wgpu::Features::empty();
        if multi_draw {
            required |= wgpu::Features::MULTI_DRAW_INDIRECT;
        }
        // Without this the device validates sample counts against WebGPU's
        // guaranteed set, [1, 4], and antialiasing has no 2x step.
        if feats.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES) {
            required |= wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        }
        let ts = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        let timestamps = std::env::var("VOXEL_GPU_PROFILE").is_ok_and(|v| v != "0") && feats.contains(ts);
        if timestamps {
            required |= ts;
        }
        let al = adapter.limits();
        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: al.max_storage_buffer_binding_size.min(1 << 30),
            max_buffer_size: al.max_buffer_size.min(1 << 30),
            max_storage_buffers_per_shader_stage: al.max_storage_buffers_per_shader_stage.min(16),
            // One block texture layer per library material face (plus the
            // built-ins); the library grows past the default 256.
            max_texture_array_layers: al.max_texture_array_layers.clamp(256, 2048),
            ..wgpu::Limits::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            required_features: required,
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .expect("failed to create device");
        // A lost device (a driver reset, a GPU hang) otherwise shows up only as
        // a panic in whatever call touched it next, with no reason given.
        device.set_device_lost_callback(|reason, msg| eprintln!("gpu device lost: {reason:?}: {msg}"));
        Self {
            instance,
            adapter,
            device,
            queue,
            multi_draw,
            timestamps,
        }
    }

    pub fn describe(&self) -> String {
        let i = self.adapter.get_info();
        format!("{} ({:?})", i.name, i.backend)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    view_proj: [f32; 16],
    inv_view_proj: [f32; 16],
    /// Cascade 0, then the older and newer map of cascades 1 and 2.
    shadow_mats: [[f32; 16]; 5],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    ground_color: [f32; 4],
    fog: [f32; 4],
    splits: [f32; 4],
    screen: [f32; 4],
    cam_fwd: [f32; 4],
    shadow_texel: [f32; 4],
    box_min: [f32; 4],
    box_max: [f32; 4],
    style: [f32; 4],
    sky_ambient: [f32; 4],
    /// rgb = light the air scatters toward the viewer near the sun, w = how much.
    glow: [f32; 4],
    /// xyz = toward the real sun (the key light may be the moon), w = night.
    air_sun: [f32; 4],
    /// x = biome mist multiplier, y = mist everywhere, z = mist height (m), w = 1 if any.
    mist: [f32; 4],
    /// Texture layers of the older and newer maps of cascades 1 and 2.
    shadow_layers: [f32; 4],
    /// How far each of cascades 1 and 2 has blended from its older map to its newer.
    shadow_blend: [f32; 4],
    /// Water colours, linear: world shallow, deep, then two alternates
    /// (shallow, deep); w = 1 where set. The mist map's b and a hold each
    /// alternate's share.
    water: [[f32; 4]; 6],
    /// x = how many lamps are in the lamp buffer this frame (render/lamps.rs).
    lamps: [f32; 4],
    /// The weather (world/weather.rs `Sky::uniform`): x = cloud cover added
    /// to the deck, y = rain, z = snow, w = the wind's multiplier.
    weather: [f32; 4],
    /// The season (world/season.rs `Phase::uniform`): xyz = the multiply on
    /// every green thing, w = how far the grass has gone over to straw.
    /// Summer is 1, 1, 1, 0 and moves nothing.
    season: [f32; 4],
    /// The northern lights (render/aurora.rs `uniform`): x = brightness,
    /// y = how far overhead, z = the night's layout. All zero on most worlds.
    aurora: [f32; 4],
    /// Under water (docs/UNDERWATER.md): x = the water surface over the
    /// camera (the sea's when it is not under), y = visibility in metres,
    /// z = sunlight falloff per metre of depth, w = 1 under inland water.
    under: [f32; 4],
    /// x = snow on upward faces, 0 to 1 (`FrameParams::snow_cover`);
    /// y = 1 for a map in space (`FrameParams::space`).
    snow: [f32; 4],
    /// The EDIT page's platform (`FrameParams::studio`): x, z of its middle,
    /// half its side with the bank, w = 1 while it is laid.
    studio: [f32; 4],
    /// What the weather has left on the ground (`FrameParams::damp`): x =
    /// wet, y = standing water, z = frost in shade, w = frost in sun.
    damp: [f32; 4],
    /// Lightning (`FrameParams::bolt`): where it comes to ground, its seed,
    /// how bright it is now.
    bolt: [f32; 4],
    /// The tide (world/tide.rs): x = how far the sea has fallen from high
    /// water, y = the range, z = high water, w = 1 with a tide.
    tide: [f32; 4],
    /// The surf (world/surf.rs): x = breaker height on a fair day, y = wave
    /// length, z = period, w = 1 with surf.
    surf: [f32; 4],
    /// The swell (world/surf.rs): x, z = the way it runs, z = crest to
    /// trough on a fair day, w = the longest wave's length; z = 0 for none.
    swell: [f32; 4],
}

/// The visual style: textures, palette, lighting and grade change together.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Softly detailed textures (render/texsets.rs) and a clear, neutral
    /// daylight. The fallback style.
    Classic,
    /// Near-flat colour, broad colour fields across the land, soft shadows, a
    /// warm fog ramp and a split-tone grade. The default on
    /// load since 2026-11-08; saves from before then call it "calm".
    #[default]
    #[serde(alias = "calm")]
    Saturated,
}

impl Style {
    pub fn next(self) -> Self {
        match self {
            Style::Classic => Style::Saturated,
            Style::Saturated => Style::Classic,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Style::Classic => "classic",
            Style::Saturated => "saturated",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "classic" => Some(Style::Classic),
            "saturated" | "calm" => Some(Style::Saturated),
            _ => None,
        }
    }
}

/// Sky, ground bounce, sun colour and fog for one style. The game loads them
/// from library/looks (set_look); `builtin` is the fallback.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub ground: [f32; 3],
    /// Sky fill for shadowed and indirect light; kept apart from the sky colours
    /// so the sky can be tuned without changing the ground.
    pub ambient: [f32; 3],
    pub sun_tint: Vec3,
    pub sun_gain: f32,
    pub fog_density: f32,
}

fn look(style: Style) -> Look {
    match style {
        Style::Classic => Look {
            zenith: [0.18, 0.37, 0.71],
            horizon: [0.66, 0.74, 0.84],
            ground: [0.23, 0.20, 0.15],
            ambient: [0.42, 0.555, 0.775],
            sun_tint: Vec3::ONE,
            sun_gain: 1.0,
            fog_density: 1.0 / 950.0,
        },
        Style::Saturated => Look {
            zenith: [0.13, 0.38, 0.78],
            horizon: [0.66, 0.78, 0.88],
            ground: [0.30, 0.22, 0.12],
            // A deep, slightly teal fill, not lifted blue-grey.
            ambient: [0.47, 0.61, 0.62],
            sun_tint: Vec3::new(1.0, 0.88, 0.66),
            sun_gain: 1.08,
            // A little more distance haze than first tuned.
            fog_density: 1.0 / 1050.0,
        },
    }
}

/// The look a style falls back on when no look file is read (the LOOK page
/// and the tests start from these).
#[allow(dead_code)] // the LOOK page tests start from these
pub fn builtin_look(style: Style) -> Look {
    look(style)
}

pub use crate::models::Instance;

/// Everything about the camera and scene the renderer needs for one frame.
/// Screen-space reflections in standing water (post.wgsl `mirror`):
/// VOXEL_SSR=0 turns them off for a comparison.
fn reflections_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VOXEL_SSR").map(|v| v != "0").unwrap_or(true))
}

pub struct FrameParams<'a> {
    pub view: Mat4,
    pub proj: Mat4,
    pub cam_pos: Vec3,
    pub cam_fwd: Vec3,
    pub time: f32,
    pub underwater: bool,
    /// The open sea's surface now: high water (`set_world`) less the tide
    /// (world/tide.rs).
    pub sea: f32,
    /// The water surface over the camera (the sea level when it is dry).
    pub water_level: f32,
    pub highlight: Option<(Vec3, Vec3)>,
    /// Lines a mode draws into the world this frame (overlay.rs).
    pub overlay: &'a overlay::Overlay,
    /// Sign boards and lettering near the camera (plates.rs).
    pub plates: &'a plates::Plates,
    /// Prints and trampled grass (creatures/sign.rs, signs.rs).
    pub marks: &'a crate::creatures::sign::Marks,
    /// Dust, splashes, chips and falling leaves (src/fx, particles.rs).
    pub fx: &'a crate::fx::Fx,
    /// The point lights near the camera (lamps.rs), for roofed places.
    pub lamps: &'a lamps::Lamps,
    /// Entity instances, sorted by model index.
    pub entities: &'a [(usize, Instance)],
    pub shadows: bool,
    pub style: Style,
    /// Sloped ground and rounded trees: tilted quads can face away from their
    /// direction, so they are drawn whenever they might show.
    pub slopes: bool,
    /// Speed effect strength, 0 to 1: radial blur and vignette in post.
    pub speed_fx: f32,
    /// Speed streaks at the rim of the view, 0 to 1 (post.wgsl).
    pub streaks: f32,
    /// Time of day in hours (see daylight.rs).
    pub hour: f32,
    /// The weather (world/weather.rs): the light, the cloud deck, the haze,
    /// the wind and the falling rain or snow.
    pub sky: crate::world::weather::Sky,
    /// The season (world/season.rs): the palette, and the colour of every
    /// growing thing.
    pub season: crate::world::season::Phase,
    /// How much snow lies on whatever faces up (tree tops, ledges, roofs),
    /// 0 to 1: the season's lying snow, or a snowbound world's own floor.
    pub snow_cover: f32,
    /// What the weather has left on the ground (world/damp.rs `uniform`):
    /// wet, standing water, frost in shade, frost in sun.
    pub damp: [f32; 4],
    /// Mist added everywhere for the fog lying in the hollows after a clear
    /// night (world/damp.rs `mist`).
    pub ground_fog: f32,
    /// Lightning (world/lightning.rs `uniform`): where it comes to ground,
    /// its seed and how bright it is now; all zero between strikes.
    pub bolt: [f32; 4],
    /// The heat mirage, 0 to 1 (post.wgsl `mirage`).
    pub mirage: f32,
    /// The level of the ground the mirage lies over, metres: the ground
    /// under the camera.
    pub mirage_floor: f32,
    /// The camera's lens and viewfinder (src/camera), when it is up.
    pub lens: Option<LensFx>,
    /// A map floating in space (world.json `space`): the sky is black with
    /// stars all the way round, and has no clouds or mist.
    pub space: bool,
    /// Draw with the default grade rather than the style's own: the EDIT
    /// page's studio, so a world's look cannot tint what is being edited.
    pub plain_grade: bool,
    /// The EDIT page's platform while it is laid: the middle and half the
    /// side, bank included. It is cut down into the land and open to the sky,
    /// so the sun is not stopped at the land over it (scene.wgsl
    /// `under_land`).
    pub studio: Option<(f32, f32, f32)>,
}

/// The camera's lens for the post pass (post.wgsl `Lens`): the viewfinder in
/// uv, the focus distance, blur at infinity and its cap in pixels at 1080
/// tall, and the blur outside the frame (0 for a photo, which has no mask).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct LensFx {
    pub frame: [f32; 4],
    pub focus_m: f32,
    pub blur_inf: f32,
    pub blur_max: f32,
    pub mask: f32,
}

#[derive(Default, Clone, Debug)]
pub struct FrameStats {
    pub draws: u32,
    pub shadow_draws: u32,
    pub quads: u64,
    pub shadow_quads: u64,
    pub chunks_visible: u32,
    /// Grass, flower and leaf sprites drawn (see foliage.rs).
    pub sprites: u64,
    pub cpu_ms: f32,
    /// Of `cpu_ms`, the time spent deciding what to draw: the walk over the
    /// loaded chunks and the sort (render/drawlist.rs).
    pub drawlist_ms: f32,
}

pub struct Sun {
    pub dir: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

impl Default for Sun {
    fn default() -> Self {
        Self {
            dir: Vec3::new(-0.52, 0.6, 0.6).normalize(),
            color: Vec3::new(1.0, 0.89, 0.74),
            intensity: 2.3,
        }
    }
}

/// GPU side of the terrain: the quad buffer, its allocator and chunk slots.
pub struct TerrainBuffers {
    queue: wgpu::Queue,
    quad_buf: wgpu::Buffer,
    chunk_buf: wgpu::Buffer,
    alloc: RangeAlloc,
    free_slots: Vec<u32>,
    next_slot: u32,
    scratch: Vec<[u32; 4]>,
    pub full_warned: bool,
    /// Bumped on every upload or free, so cached shadow maps know to redraw.
    pub changes: u64,
}

impl TerrainBuffers {
    /// Forget every chunk (a new world): the old ones are never freed one by one.
    fn reset(&mut self) {
        self.alloc = RangeAlloc::new(self.alloc.capacity);
        self.free_slots.clear();
        self.next_slot = 0;
        self.full_warned = false;
        self.changes += 1;
    }

    pub fn used_quads(&self) -> u64 {
        self.alloc.used
    }
    pub fn slots_used(&self) -> u32 {
        self.next_slot - self.free_slots.len() as u32
    }
    /// Free ranges in the quad buffer; a rising count means fragmentation.
    pub fn free_ranges(&self) -> usize {
        self.alloc.fragments()
    }
}

impl ChunkSink for TerrainBuffers {
    fn upload(&mut self, origin: Vec3, scale: f32, mesh: &Mesh) -> Option<GpuChunk> {
        // Sprites go after the six face groups, in the same allocation.
        let n = (mesh.quads.len() + mesh.sprites.len()) as u32;
        let Some(offset) = self.alloc.alloc(n) else {
            if !self.full_warned {
                eprintln!("terrain quad buffer full ({} quads); dropping chunks", self.alloc.capacity);
                self.full_warned = true;
            }
            return None;
        };
        let slot = match self.free_slots.pop() {
            Some(s) => s,
            None if self.next_slot < MAX_SLOTS => {
                self.next_slot += 1;
                self.next_slot - 1
            }
            None => {
                self.alloc.free(offset, n);
                return None;
            }
        };
        self.changes += 1;
        self.scratch.clear();
        self.scratch.extend(
            mesh.quads
                .iter()
                .chain(&mesh.sprites)
                .map(|q| [q[0], (q[1] & 0xffff) | slot << crate::mesh::quad::SLOT_SHIFT, q[2], q[3]]),
        );
        self.queue.write_buffer(&self.quad_buf, offset as u64 * 16, bytemuck::cast_slice(&self.scratch));
        self.queue.write_buffer(&self.chunk_buf, slot as u64 * 16, bytemuck::cast_slice(&[origin.x, origin.y, origin.z, scale]));
        let size = 32.0 * scale;
        // Sloped and rounded corners move up to a voxel, so the box grows by one.
        let slack = Vec3::splat(scale);
        // Kelp rises from its holdfast to the surface, lies along it and
        // sways (sprites.rs STRAND): the box reaches as high as the longest
        // strand could, and as far to the sides, or a chunk on the bed would
        // be culled while its canopy is in view.
        let mut reach = Vec3::ZERO;
        for q in mesh.sprites.iter().filter(|q| (q[1] >> 8) & 255 == crate::mesh::sprites::STRAND) {
            let y = (((q[0] >> 20) & 2047) as f32 / 16.0 - 2.0) * scale;
            let len = ((q[2] >> 16) & 255) as f32 / 8.0;
            reach = reach.max(Vec3::new(len, y + len - size, len));
        }
        let (lo, hi) = (Vec3::new(reach.x, 0.0, reach.z), reach.max(Vec3::ZERO));
        Some(GpuChunk {
            cy: 0,
            slot,
            offset,
            dir_counts: mesh.dir_counts,
            tails: mesh.tails,
            rtails: mesh.rtails,
            sprites: mesh.sprites.len() as u32,
            sprite_fill: mesh.sprite_fill,
            sprite_grass: mesh.sprite_grass,
            sprite_patch: mesh.sprite_patch,
            sprite_stalk: mesh.sprite_stalk,
            min: origin - slack - lo,
            max: origin + Vec3::splat(size) + slack + hi,
        })
    }

    fn free(&mut self, c: &GpuChunk) {
        self.changes += 1;
        self.alloc.free(c.offset, c.quads());
        self.free_slots.push(c.slot);
    }
}

struct Targets {
    hdr_msaa: wgpu::TextureView,
    hdr: wgpu::TextureView,
    depth: wgpu::TextureView,
    post_bg: wgpu::BindGroup,
    /// Sun rays at half resolution (see post.wgsl fs_rays), and the bind
    /// group the rays pass reads the scene through.
    rays: wgpu::TextureView,
    rays_bg: wgpu::BindGroup,
}

/// What make_targets needs besides the size.
struct TargetLayouts {
    post_bgl: wgpu::BindGroupLayout,
    rays_bgl: wgpu::BindGroupLayout,
    post_buf: wgpu::Buffer,
    shaft_buf: wgpu::Buffer,
    lens_buf: wgpu::Buffer,
    grade_buf: wgpu::Buffer,
    grade_view: wgpu::TextureView,
    linear: wgpu::Sampler,
}

/// A shadow cascade is a square around the camera position, not its view
/// direction, so turning never invalidates it. With a fixed sun, the outer
/// cascades only need redrawing when the player has moved or terrain changed.
#[derive(Clone, Copy)]
struct CascadeCache {
    mat: Mat4,
    texel: f32,
    centre: Vec3,
    radius: f32,
    terrain_changes: u64,
    /// The sun direction the map was drawn for; the sun moves through the day.
    sun: Vec3,
    valid: bool,
    /// The light matrix before snapping, and the grid phase it was snapped to.
    raw: Mat4,
    phase: glam::Vec2,
}

impl Default for CascadeCache {
    fn default() -> Self {
        Self {
            mat: Mat4::IDENTITY,
            texel: 0.0,
            centre: Vec3::ZERO,
            radius: 0.0,
            terrain_changes: 0,
            sun: Vec3::ZERO,
            valid: false,
            raw: Mat4::IDENTITY,
            phase: glam::Vec2::ZERO,
        }
    }
}

/// Shadow map layers: cascade 0 has one, cascades 1 and 2 have three each
/// (see `Sliced`).
const SHADOW_LAYERS: u32 = 7;

/// Cascades 1 and 2 are drawn a slice of their chunks per frame, `SLICES[k]`
/// frames to a map, into the third of their three layers while the shader
/// blends between the other two (the last two maps finished). Shadows then
/// slide smoothly as the sun moves, there is no frame that redraws a whole
/// outer cascade (those were hitches: 1.5 and 3.3 million quads in the
/// forest), and the cost per frame is about a quarter of redrawing them every
/// other frame. The maps lag the sun by up to two cycles (0.13 and 0.4 s),
/// far too little to see.
const SLICES: [u32; 2] = [4, 12];
/// How far the camera may move from where the older map was centred (m)
/// before the cascade is redrawn whole; the maps reach this far past their
/// split so a moving player stays covered.
const SLICE_MARGIN: [f32; 2] = [6.0, 20.0];
/// Each map is shown for three slice cycles after it is placed, so an outer
/// cascade is centred where the camera will be in the middle of that life:
/// one and a half cycles ahead at its present speed. The camera then strays
/// half as far from the centre as it would centred on the spot, which doubles
/// the speed a flight can hold before the margin runs out and the cascade has
/// to be redrawn whole (a hitch of 1.5 to 4.4 million quads). Standing still
/// the lead is nothing, so shadows near a walking player are unchanged.
const LEAD_CYCLES: f32 = 1.5;
/// A step bigger than this is a jump, not movement (a teleport, a new world).
const JUMP: f32 = 5.0;

#[derive(Clone, Copy)]
struct Sliced {
    /// Layers being shown: the older and the newer finished map.
    shown: [usize; 2],
    /// The layer being drawn.
    drawing: usize,
    /// Slices of the drawing map done so far.
    slice: u32,
}

/// One shadow map draw this frame.
#[derive(Clone, Copy)]
struct ShadowJob {
    layer: usize,
    /// Draw only chunks whose slot falls in this slice (of `n`); None = all.
    slice: Option<(u32, u32)>,
    clear: bool,
    creatures: bool,
}

/// Which slice of a cycle a chunk belongs to (stable while it stays loaded).
fn chunk_slice(min: Vec3, n: u32) -> u32 {
    let (x, y, z) = ((min.x / 8.0).floor() as i64, (min.y / 8.0).floor() as i64, (min.z / 8.0).floor() as i64);
    let h = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791)) as u64;
    ((h ^ (h >> 17)) % n as u64) as u32
}

/// IEEE half-precision bits for `v` (rounded toward zero; plenty for mist).
fn f16_bits(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let e = ((b >> 23) & 0xff) as i32 - 127 + 15;
    if e <= 0 {
        return sign;
    }
    if e >= 31 {
        return sign | 0x7c00;
    }
    sign | ((e as u16) << 10) | ((b >> 13) & 0x3ff) as u16
}

/// How high a mist bank reaches above the ground under it (metres, e-folding).
const MIST_HEIGHT: f32 = 7.0;

/// The ground mist lies on, per mist-map cell: the land smoothed over about
/// 30 m (so banks fill hollows rather than tracing every bump), never below
/// the sea.
fn mist_ground(heights: &[f32], hn: usize, m: usize, sea: f32, size: f32) -> Vec<f32> {
    let mut g = vec![sea; m * m];
    for z in 0..m {
        for x in 0..m {
            let sx = ((x as f32 + 0.5) / m as f32 * hn as f32) as usize;
            let sz = ((z as f32 + 0.5) / m as f32 * hn as f32) as usize;
            if let Some(h) = heights.get(sz.min(hn - 1) * hn + sx.min(hn - 1)) {
                g[z * m + x] = h.max(sea);
            }
        }
    }
    let r = ((30.0 / (size / m as f32)) as i32).max(1);
    crate::util::blur(&mut g, m, r);
    g
}

/// Light-space matrix for one shadow cascade: a square of half-size `r`
/// around `centre`, looking along `-sun`. The texel grid is placed so that the
/// world origin falls at `phase` texels; see `shift_phase`.
fn cascade_matrix(sun: Vec3, centre: Vec3, r: f32, phase: glam::Vec2) -> (Mat4, Mat4, f32) {
    // Up is the axis of the sun's daily circle (daylight::sun_dir), so the sun
    // turns the light's view about its own up axis and the grid's rows never move.
    let path_axis = daylight::path_axis();
    let up = if sun.dot(path_axis).abs() < 0.95 {
        path_axis
    } else if sun.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    // look_to, not look_at: the view's rotation must not pick up rounding from
    // the camera position, or the grid phase below wanders as the player walks.
    let view = Mat4::look_to_rh(centre + sun * 300.0, -sun, up);
    let proj = Mat4::orthographic_rh(-r, r, -r, r, 0.0, 600.0);
    let raw = proj * view;
    // Snap so the grid lands on whole texels plus the phase: moving the camera
    // then slides the map by whole texels and its shadows hold still.
    let half = SHADOW_SIZE as f32 * 0.5;
    let o = (raw * Vec4::W).truncate().truncate() * half - phase;
    let d = (o.round() - o) / half;
    let snap = Mat4::from_translation(Vec3::new(d.x, d.y, 0.0));
    (snap * raw, raw, 2.0 * r / SHADOW_SIZE as f32)
}

/// Where the camera falls on a cascade's texel grid, relative to the world
/// origin (texels); only the matrix's rotation and scale matter.
fn grid_pos(raw: Mat4, p: Vec3) -> glam::Vec2 {
    raw.transform_vector3(p).truncate() * (SHADOW_SIZE as f32 * 0.5)
}

/// The grid phase for a new light matrix, chosen so the texel grid stays put
/// at the camera while the sun turns. Snapping to the world origin alone kept
/// the grid fixed to the world under movement, but every small step of the sun
/// swung it round the origin, hundreds of metres away: near the player the grid
/// slid most of a texel each frame and leaf shadows flickered. Pinning it at
/// the camera instead leaves only a slide of a few hundredths of a texel, at
/// the cascade's edge, while camera movement still adds nothing.
fn shift_phase(phase: glam::Vec2, old_raw: Mat4, new_raw: Mat4, cam: Vec3) -> glam::Vec2 {
    let p = phase + grid_pos(old_raw, cam) - grid_pos(new_raw, cam);
    p - p.floor()
}

pub struct ModelRange {
    pub offset: u32,
    pub count: u32,
}

pub struct Renderer {
    prof: Option<gpuprof::GpuProf>,
    /// Material id -> texture layers and flags, re-uploaded when a world
    /// installs its own ids (`matlib::generation`).
    mat_buf: wgpu::Buffer,
    mat_gen: u32,
    /// The light for each Style, by its index.
    looks: [Look; 2],
    /// Each style's gradient strips and grade knobs, and the one last sent to
    /// the GPU, so an edit on the LOOK page shows on the next frame.
    grades: [Grade; 2],
    grade_tex: wgpu::Texture,
    grade_sent: Option<(usize, Grade)>,
    heights_tex: wgpu::Texture,
    palette_buf: wgpu::Buffer,
    mist_tex: wgpu::Texture,
    horizon_tex: wgpu::Texture,
    mist_base: Vec<f32>,
    water: [[f32; 4]; 6],
    /// True once a world with misty biomes has set the mist map.
    mist_any: bool,
    air: daylight::Air,
    device: wgpu::Device,
    queue: wgpu::Queue,
    multi_draw: bool,
    pub width: u32,
    pub height: u32,
    msaa: u32,
    pub terrain: TerrainBuffers,
    pub sun: Sun,
    globals_buf: wgpu::Buffer,
    indirect_buf: wgpu::Buffer,
    cascade_buf: wgpu::Buffer,
    einst_buf: wgpu::Buffer,
    equad_buf: wgpu::Buffer,
    hud_buf: wgpu::Buffer,
    hud_screen_buf: wgpu::Buffer,
    map_tex: wgpu::Texture,
    backdrop_tex: [wgpu::Texture; 2],
    scene_bg: wgpu::BindGroup,
    shadow_in_bg: wgpu::BindGroup,
    cascade_bg: wgpu::BindGroup,
    hud_bg: wgpu::BindGroup,
    shadow_layer_views: Vec<wgpu::TextureView>,
    targets: Targets,
    p_sky: wgpu::RenderPipeline,
    p_terrain: wgpu::RenderPipeline,
    p_entity: wgpu::RenderPipeline,
    /// Entities drawn see-through (`models::GLASS`): blended, writing no depth.
    p_entity_glass: wgpu::RenderPipeline,
    p_water: wgpu::RenderPipeline,
    p_water_basin: wgpu::RenderPipeline,
    p_river: wgpu::RenderPipeline,
    p_fall: wgpu::RenderPipeline,
    p_spray: wgpu::RenderPipeline,
    river_buf: wgpu::Buffer,
    spray_buf: wgpu::Buffer,
    river_n: u32,
    /// Waterfall sheet quads and spray spots, after the river cells.
    fall_n: u32,
    spray_n: u32,
    p_box: wgpu::RenderPipeline,
    p_overlay: wgpu::RenderPipeline,
    p_overlay_top: wgpu::RenderPipeline,
    overlay_buf: wgpu::Buffer,
    overlay_lines: Vec<overlay::Line>,
    lamp_buf: wgpu::Buffer,
    lamp_n: u32,
    p_plate: wgpu::RenderPipeline,
    plate_buf: wgpu::Buffer,
    plate_n: u32,
    /// The HUD's font atlas and its sampler, kept so the scene bind group can
    /// be built again when the block texture array is replaced.
    font_atlas_view: wgpu::TextureView,
    font_samp: wgpu::Sampler,
    p_shadow_terrain: wgpu::RenderPipeline,
    p_shadow_entity: wgpu::RenderPipeline,
    p_post: wgpu::RenderPipeline,
    p_rays: wgpu::RenderPipeline,
    tl: TargetLayouts,
    p_hud: wgpu::RenderPipeline,
    foliage: foliage::Foliage,
    particles: particles::Particles,
    /// Prints on the ground (creatures/sign.rs, signs.rs).
    signs: signs::Signs,
    models: Vec<ModelRange>,
    draws: Vec<Draw>,
    /// Main-pass draws with their distance and their chunk's index in
    /// `cull_boxes`.
    main_list: Vec<(f32, Draw, u32)>,
    shadow_lists: [Vec<Draw>; 3],
    occlusion: occlusion::Occlusion,
    /// This frame's visible chunks' boxes (min, max), and for every draw the
    /// box it belongs to (occlusion::NO_CHUNK for shadow draws).
    cull_boxes: Vec<[f32; 8]>,
    cull_owner: Vec<u32>,
    sprite_list: Vec<(f32, Draw, u32)>,
    /// Scratch for `near_to_far`.
    draw_order: Vec<(f32, Draw, u32)>,
    quad_index: wgpu::Buffer,
    sliced: [Sliced; 2],
    /// The camera's drift between frames, smoothed, and where it was last
    /// frame: the outer cascades are centred ahead of a moving player (`LEAD`).
    cam_step: Vec3,
    last_cam: Option<Vec3>,
    /// How many times each outer cascade has had to be redrawn whole.
    cascade_redraws: [u64; 2],
    inst_scratch: Vec<Instance>,
    /// Entity draws: model, first instance, count, and whether they are glass.
    runs: Vec<(usize, u32, u32, bool)>,
    /// One per shadow map layer.
    cascades: [CascadeCache; SHADOW_LAYERS as usize],
    frame_no: u64,
    pub splits: [f32; 3],
    world_size: f32,
    /// High water: the sea level the world was built against.
    sea_level: f32,
    /// Metres between high and low water (`set_tide`); 0 for a still sea.
    tide_range: f32,
    /// The surf (`set_surf`, world/surf.rs): height, length, period, on.
    surf: [f32; 4],
    /// The swell (`set_surf`): heading x, z, height, length; height 0 for none.
    swell: [f32; 4],
    /// The world's underwater visibility (m) and light falloff (/m).
    under: [f32; 2],
    /// Sized to `matlib::layer_count` at the last install; a world's own ids
    /// can change the count (grain copies), and then it is made again.
    blocks: wgpu::Texture,
    blocks_samp: wgpu::Sampler,
    scene_bgl: wgpu::BindGroupLayout,
    post_buf: wgpu::Buffer,
    shaft_buf: wgpu::Buffer,
    lens_buf: wgpu::Buffer,
    style: Style,
}

/// What the scene bind group points at; it is made again when the block
/// texture array is replaced.
struct SceneRes<'a> {
    globals: &'a wgpu::Buffer,
    quads: &'a wgpu::Buffer,
    chunks: &'a wgpu::Buffer,
    mats: &'a wgpu::Buffer,
    blocks: &'a wgpu::TextureView,
    blocks_samp: &'a wgpu::Sampler,
    heights: &'a wgpu::TextureView,
    palette: &'a wgpu::Buffer,
    equads: &'a wgpu::Buffer,
    einst: &'a wgpu::Buffer,
    mist: &'a wgpu::TextureView,
    /// The land's skyline and sky openness (horizon.rs).
    horizon: &'a wgpu::TextureView,
    overlay: &'a wgpu::Buffer,
    lamps: &'a wgpu::Buffer,
    rivers: &'a wgpu::Buffer,
    /// Where the waterfalls land (`Spray`).
    sprays: &'a wgpu::Buffer,
    /// The look's gradient strips (grade.rs) and the knobs that go with them.
    grade_strip: &'a wgpu::TextureView,
    grade: &'a wgpu::Buffer,
    /// Sign boards and lettering (plates.rs), with the HUD's font atlas.
    plates: &'a wgpu::Buffer,
    font_atlas: &'a wgpu::TextureView,
    font_samp: &'a wgpu::Sampler,
}

fn scene_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, r: &SceneRes) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: r.globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: r.quads.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: r.chunks.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: r.mats.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(r.blocks),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(r.blocks_samp),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(r.heights),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: r.palette.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: r.equads.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: r.einst.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(r.mist),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: r.overlay.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: r.rivers.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 13,
                resource: wgpu::BindingResource::TextureView(r.grade_strip),
            },
            wgpu::BindGroupEntry {
                binding: 14,
                resource: r.grade.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 15,
                resource: r.plates.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 16,
                resource: wgpu::BindingResource::TextureView(r.font_atlas),
            },
            wgpu::BindGroupEntry {
                binding: 17,
                resource: wgpu::BindingResource::Sampler(r.font_samp),
            },
            wgpu::BindGroupEntry {
                binding: 18,
                resource: wgpu::BindingResource::TextureView(r.horizon),
            },
            wgpu::BindGroupEntry {
                binding: 19,
                resource: r.lamps.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 20,
                resource: r.sprays.as_entire_binding(),
            },
        ],
    })
}

/// The block texture array, one layer per `matlib::layer_count`.
fn make_blocks(device: &wgpu::Device, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blocks"),
        size: wgpu::Extent3d {
            width: textures::SIZE as u32,
            height: textures::SIZE as u32,
            depth_or_array_layers: layers.max(1),
        },
        mip_level_count: textures::MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn blocks_view(tex: &wgpu::Texture) -> wgpu::TextureView {
    tex.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

fn upload_blocks(queue: &wgpu::Queue, tex: &wgpu::Texture, style: Style) {
    let mips = textures::build(style == Style::Saturated);
    for (m, level) in mips.iter().enumerate() {
        let s = (textures::SIZE >> m) as u32;
        for (l, data) in level.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: m as u32,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: l as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * s),
                    rows_per_image: Some(s),
                },
                wgpu::Extent3d {
                    width: s,
                    height: s,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn shader(device: &wgpu::Device, label: &str, code: String) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(code)),
    })
}

/// Index into Mesh::tails of the tilted quads of group `d` that could face a
/// camera behind the face plane of the box (min, max). A quad leaning at most
/// angle a from the axis can face the camera only if the camera is behind the
/// plane by less than tan(a) times its distance, across the plane, to the far
/// side of the box.
fn lean_needed(d: usize, cam: Vec3, min: Vec3, max: Vec3) -> usize {
    let a = d / 2;
    let behind = if d % 2 == 0 { min[a] - cam[a] } else { cam[a] - max[a] };
    let far = |i: usize| (cam[i] - min[i]).abs().max((cam[i] - max[i]).abs());
    let (u, v) = ((a + 1) % 3, (a + 2) % 3);
    let across = (far(u).powi(2) + far(v).powi(2)).sqrt();
    crate::mesh::LEANS.iter().take_while(|(_, tan)| behind >= tan * across).count()
}

fn bgl_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn storage_ro() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn uniform(dynamic: bool) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: dynamic,
        min_binding_size: None,
    }
}

fn tex_ty(dim: wgpu::TextureViewDimension, filterable: bool) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable },
        view_dimension: dim,
        multisampled: false,
    }
}

struct PipeDesc<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    vs: &'a str,
    fs: Option<&'a str>,
    color: Option<(wgpu::TextureFormat, Option<wgpu::BlendState>)>,
    topology: wgpu::PrimitiveTopology,
    cull: Option<wgpu::Face>,
    depth: Option<(bool, wgpu::CompareFunction, wgpu::DepthBiasState)>,
    samples: u32,
}

fn pipeline(device: &wgpu::Device, d: PipeDesc) -> wgpu::RenderPipeline {
    let targets = [d.color.map(|(format, blend)| wgpu::ColorTargetState {
        format,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(d.label),
        layout: Some(d.layout),
        vertex: wgpu::VertexState {
            module: d.module,
            entry_point: Some(d.vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: d.topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: d.cull,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: d.depth.map(|(write, compare, bias)| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: write,
            depth_compare: compare,
            stencil: Default::default(),
            bias,
        }),
        multisample: wgpu::MultisampleState {
            count: d.samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: d.fs.map(|fs| wgpu::FragmentState {
            module: d.module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: if d.color.is_some() { &targets } else { &[] },
        }),
        multiview: None,
        cache: None,
    })
}

impl Renderer {
    /// The device resources, with no world yet: `set_world` gives it one.
    /// `want_msaa` is the wanted sample count (1, 2 or 4); the highest the
    /// adapter supports at or below it is used.
    pub fn new(gpu: &Gpu, out_format: wgpu::TextureFormat, width: u32, height: u32, want_msaa: u32) -> Self {
        let (heights_n, world_size, sea_level) = (HEIGHTS_N, 512.0, 0.0);
        let heights = vec![0.0f32; (HEIGHTS_N * HEIGHTS_N) as usize];
        let device = gpu.device.clone();
        let queue = gpu.queue.clone();
        // 2 samples need the adapter's own format features (see Gpu::new);
        // where they are missing the choice falls back to the next lower count.
        let flags = gpu.adapter.get_texture_format_features(HDR).flags;
        let fine = |n: u32| n == 1 || (flags.sample_count_supported(n) && (n == 4 || device.features().contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)));
        let msaa = [4, 2, 1].into_iter().find(|n| *n <= want_msaa.clamp(1, 4) && fine(*n)).unwrap_or(1);

        let buf = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let st = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let quad_cap = quad_capacity(&device);
        let quad_buf = buf("terrain quads", quad_cap as u64 * 16, st);
        let chunk_buf = buf("chunk origins", MAX_SLOTS as u64 * 16, st);
        let globals_buf = buf("globals", std::mem::size_of::<Globals>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        // STORAGE: the occlusion pass turns off hidden chunks' draws in place.
        let indirect_buf = buf("indirect", MAX_DRAWS * 20, wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE);
        let quad_index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad index"),
            contents: bytemuck::cast_slice(&(0..QUAD_INDEX).flat_map(|k| [4 * k, 4 * k + 1, 4 * k + 2, 4 * k, 4 * k + 2, 4 * k + 3]).collect::<Vec<u32>>()),
            usage: wgpu::BufferUsages::INDEX,
        });
        let cascade_buf = buf("cascades", 256 * SHADOW_LAYERS as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        let einst_buf = buf("entity instances", MAX_INSTANCES as u64 * std::mem::size_of::<Instance>() as u64, st);
        let overlay_buf = buf("overlay lines", (overlay::MAX_LINES * std::mem::size_of::<overlay::Line>()) as u64, st);
        let lamp_buf = buf("lamps", (lamps::MAX * std::mem::size_of::<lamps::Lamp>()) as u64, st);
        let plate_buf = buf("sign plates", (plates::MAX_PLATES * std::mem::size_of::<plates::Plate>()) as u64, st);
        let river_buf = buf("river surface", (RIVER_CAP * std::mem::size_of::<RiverQuad>()) as u64, st);
        let spray_buf = buf("waterfall spray", (SPRAY_CAP * std::mem::size_of::<Spray>()) as u64, st);
        let equad_buf = buf("entity quads", MAX_ENTITY_QUADS as u64 * 16, st);
        let hud_buf = buf("hud", MAX_HUD as u64 * 48, st);
        let hud_screen_buf = buf("hud screen", 16, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);

        // Material table: (top, side, bottom) texture layer per material.
        let mat_gen = crate::world::matlib::generation();
        let mat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("materials"),
            contents: bytemuck::cast_slice(&material_rows()),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        // Filled by set_models.
        let palette_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("palette"),
            contents: bytemuck::cast_slice(&crate::models::palette()),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Block textures.
        let tex = make_blocks(&device, crate::world::matlib::layer_count());
        upload_blocks(&queue, &tex, Style::Classic);
        let tex_view = blocks_view(&tex);
        let samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blocks"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        // Heightmap for the water shader.
        let hm = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("heightmap"),
                size: wgpu::Extent3d {
                    width: heights_n,
                    height: heights_n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // r = the ground, g = 1 over a hollow below sea level the
                // open sea cannot reach (world/tide.rs), b = metres seaward
                // of the high-water coast, a = how open the sea is there
                // (world/surf.rs). See `set_world`.
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&heights.iter().flat_map(|h| [*h, 0.0, 0.0, 0.0]).collect::<Vec<f32>>()),
        );
        let hm_view = hm.create_view(&Default::default());

        // Mist map: r = how misty the biomes are around each cell (set_mist),
        // g = the ground the mist lies on, smoothed, so a bank follows the land.
        let mist_base = mist_ground(&heights, heights_n as usize, MIST_N as usize, sea_level, world_size);
        let mist_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mist"),
            size: wgpu::Extent3d {
                width: MIST_N,
                height: MIST_N,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mist_view = mist_tex.create_view(&Default::default());

        // The land's skyline per compass direction, and how much sky each
        // cell of the world can see (horizon.rs). Baked in set_world; until
        // then the world is flat and open.
        let horizon_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("horizon"),
                size: wgpu::Extent3d {
                    width: horizon::HORIZON_N,
                    height: horizon::HORIZON_N,
                    depth_or_array_layers: horizon::LAYERS,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &horizon::blank(),
        );
        let horizon_view = horizon_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        // Shadow map.
        let shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: SHADOW_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let shadow_layer_views = (0..SHADOW_LAYERS)
            .map(|i| {
                shadow.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        // HUD type: the distance-field font atlas and the logo, both with mips.
        let (atlas_px, aw, ah, alevels) = font::atlas_mips();
        let atlas = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("font"),
                size: wgpu::Extent3d {
                    width: aw,
                    height: ah,
                    depth_or_array_layers: 1,
                },
                mip_level_count: alevels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas_px,
        );
        let atlas_view = atlas.create_view(&Default::default());
        let (logo_px, lw, lh, llevels) = font::logo_mips();
        let logo_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("logo"),
                size: wgpu::Extent3d {
                    width: lw,
                    height: lh,
                    depth_or_array_layers: 1,
                },
                mip_level_count: llevels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Premultiplied sRGB values; hud.wgsl converts after filtering.
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &logo_px,
        );
        let logo_view = logo_tex.create_view(&Default::default());
        let mip_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hud mips"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // The scrapbook page, drawn on the CPU and shown by the HUD as one quad.
        let map_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("page"),
            size: wgpu::Extent3d {
                width: hud::IMAGE_PX as u32,
                height: hud::IMAGE_PX as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let map_view = map_tex.create_view(&Default::default());
        // Two full-screen pictures for the menus (a fading-out and a fading-in
        // screenshot), drawn by the HUD with a linear sampler.
        let backdrop = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("backdrop"),
                size: wgpu::Extent3d {
                    width: hud::BACKDROP_W,
                    height: hud::BACKDROP_H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let backdrop_tex = [backdrop(), backdrop()];
        let backdrop_views: Vec<wgpu::TextureView> = backdrop_tex.iter().map(|t| t.create_view(&Default::default())).collect();
        let backdrop_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("backdrop"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        use wgpu::TextureViewDimension as Dim;
        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                bgl_entry(0, uniform(false)),
                bgl_entry(1, storage_ro()),
                bgl_entry(2, storage_ro()),
                bgl_entry(3, storage_ro()),
                bgl_entry(4, tex_ty(Dim::D2Array, true)),
                bgl_entry(5, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                bgl_entry(6, tex_ty(Dim::D2, false)),
                bgl_entry(7, storage_ro()),
                bgl_entry(8, storage_ro()),
                bgl_entry(9, storage_ro()),
                bgl_entry(10, tex_ty(Dim::D2, true)),
                bgl_entry(11, storage_ro()),
                bgl_entry(12, storage_ro()),
                bgl_entry(13, tex_ty(Dim::D2, true)),
                bgl_entry(14, uniform(false)),
                bgl_entry(15, storage_ro()),
                bgl_entry(16, tex_ty(Dim::D2, true)),
                bgl_entry(17, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                bgl_entry(18, tex_ty(Dim::D2Array, true)),
                bgl_entry(19, storage_ro()),
                bgl_entry(20, storage_ro()),
            ],
        });
        let shadow_in_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow in"),
            entries: &[
                bgl_entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: Dim::D2Array,
                        multisampled: false,
                    },
                ),
                bgl_entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
            ],
        });
        let cascade_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cascade"),
            entries: &[bgl_entry(0, uniform(true))],
        });
        let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                bgl_entry(0, tex_ty(Dim::D2, false)),
                bgl_entry(1, uniform(false)),
                bgl_entry(2, uniform(false)),
                bgl_entry(
                    3,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: Dim::D2,
                        multisampled: msaa > 1,
                    },
                ),
                bgl_entry(4, tex_ty(Dim::D2, true)),
                bgl_entry(5, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                bgl_entry(6, uniform(false)),
                bgl_entry(7, tex_ty(Dim::D2, true)),
                bgl_entry(8, uniform(false)),
            ],
        });
        let rays_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rays"),
            entries: &[bgl_entry(0, tex_ty(Dim::D2, false)), bgl_entry(2, uniform(false))],
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Sun rays in post: the sun's screen position (uv), strength, and 0.
        let shaft_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shafts"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let post_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post"),
            // Six vec4s and two matrices: the style and speed, the streaks,
            // the weather, the camera's heading, the mirage and reflections,
            // the camera; then the view-projection and its inverse
            // (post.wgsl `Post`).
            size: 96 + 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lens_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lens"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        lens_buf.slice(..).get_mapped_range_mut().fill(0);
        lens_buf.unmap();
        // The look's gradient strips: row 0 by distance (the haze, the low sky
        // and the shadowed light), row 1 by luminance (the grade). Both the
        // scene and the post pass read them.
        let grade_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("grade strip"),
            size: wgpu::Extent3d {
                width: grade::STRIP_N,
                height: grade::STRIP_ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let grade_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grade"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let hud_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud"),
            entries: &[
                bgl_entry(0, uniform(false)),
                bgl_entry(1, storage_ro()),
                bgl_entry(2, tex_ty(Dim::D2, true)),
                bgl_entry(3, tex_ty(Dim::D2, false)),
                bgl_entry(4, tex_ty(Dim::D2, true)),
                bgl_entry(5, tex_ty(Dim::D2, true)),
                bgl_entry(6, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                bgl_entry(7, tex_ty(Dim::D2, true)),
                bgl_entry(8, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });

        let scene_bg = scene_bind_group(
            &device,
            &scene_bgl,
            &SceneRes {
                globals: &globals_buf,
                quads: &quad_buf,
                chunks: &chunk_buf,
                mats: &mat_buf,
                blocks: &tex_view,
                blocks_samp: &samp,
                heights: &hm_view,
                palette: &palette_buf,
                equads: &equad_buf,
                einst: &einst_buf,
                mist: &mist_view,
                horizon: &horizon_view,
                overlay: &overlay_buf,
                lamps: &lamp_buf,
                rivers: &river_buf,
                sprays: &spray_buf,
                plates: &plate_buf,
                font_atlas: &atlas_view,
                font_samp: &mip_samp,
                grade_strip: &grade_tex.create_view(&Default::default()),
                grade: &grade_buf,
            },
        );
        let shadow_in_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow in"),
            layout: &shadow_in_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_samp),
                },
            ],
        });
        let cascade_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cascade"),
            layout: &cascade_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &cascade_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(64),
                }),
            }],
        });
        let hud_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud"),
            layout: &hud_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: hud_screen_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: hud_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&backdrop_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&backdrop_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&backdrop_samp),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&logo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&mip_samp),
                },
            ],
        });

        let scene_mod = shader(&device, "scene", shaders::scene());
        let shadow_mod = shader(&device, "shadow", shaders::shadow());
        let post_mod = shader(&device, "post", shaders::post(msaa > 1));
        let hud_mod = shader(&device, "hud", shaders::hud());

        let layout = |bgls: &[&wgpu::BindGroupLayout]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: bgls,
                push_constant_ranges: &[],
            })
        };
        let scene_layout = layout(&[&scene_bgl, &shadow_in_bgl]);
        let foliage = foliage::Foliage::new(&device, &queue, &scene_bgl, &shadow_in_bgl, msaa);
        let particles = particles::Particles::new(&device, &scene_bgl, &shadow_in_bgl, msaa);
        let signs = signs::Signs::new(&device, &scene_bgl, &shadow_in_bgl, msaa);
        let shadow_layout = layout(&[&scene_bgl, &cascade_bgl]);
        let post_layout = layout(&[&post_bgl]);
        let hud_layout = layout(&[&hud_bgl]);

        let nobias = wgpu::DepthBiasState::default();
        let ge = wgpu::CompareFunction::GreaterEqual;
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let scene = |label, vs, fs, cull, depth, blend, topology| {
            pipeline(
                &device,
                PipeDesc {
                    label,
                    layout: &scene_layout,
                    module: &scene_mod,
                    vs,
                    fs: Some(fs),
                    color: Some((HDR, blend)),
                    topology,
                    cull,
                    depth: Some(depth),
                    samples: msaa,
                },
            )
        };
        // The sky is drawn after the land, sprites and creatures, only where the
        // depth is still clear (reversed depth: 0 is infinitely far). Drawn first
        // it shaded every pixel, clouds and all, mostly to be painted over.
        let p_sky = scene("sky", "vs_fullscreen", "fs_sky", None, (false, ge, nobias), None, tri);
        let p_terrain = scene("terrain", "vs_terrain", "fs_terrain", Some(wgpu::Face::Back), (true, ge, nobias), None, tri);
        let p_entity = scene("entity", "vs_entity", "fs_entity", Some(wgpu::Face::Back), (true, ge, nobias), None, tri);
        let p_entity_glass = scene("entity glass", "vs_entity", "fs_entity_glass", Some(wgpu::Face::Back), (false, ge, nobias), blend, tri);
        // Water blends its colour over the bed as usual, but adds its
        // coverage to the bed's alpha rather than blending it: alpha over 1
        // is what marks a mirror for the reflections in post (post.wgsl
        // `mirror`).
        let water_blend = Some(wgpu::BlendState {
            color: wgpu::BlendState::ALPHA_BLENDING.color,
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let p_water = scene("water", "vs_water", "fs_water", None, (false, ge, nobias), water_blend, tri);
        // The high-water plane over hollows the open sea cannot reach (with a
        // tide) writes its depth, so the reflections in post.wgsl take it as
        // a pool at its own surface rather than the open sea's level.
        let p_water_basin = scene("water basin", "vs_water", "fs_water", None, (true, ge, nobias), water_blend, tri);
        let p_river = scene("river", "vs_river", "fs_water", None, (false, ge, nobias), water_blend, tri);
        let p_fall = scene("waterfall", "vs_fall", "fs_fall", None, (false, ge, nobias), blend, tri);
        let p_spray = scene("spray", "vs_spray", "fs_spray", None, (false, ge, nobias), blend, tri);
        let p_box = scene("box", "vs_box", "fs_box", None, (false, ge, nobias), blend, wgpu::PrimitiveTopology::LineList);
        let p_overlay = scene("overlay", "vs_overlay", "fs_overlay", None, (false, ge, nobias), blend, tri);
        let p_overlay_top = scene("overlay on top", "vs_overlay", "fs_overlay", None, (false, wgpu::CompareFunction::Always, nobias), blend, tri);
        // Sign boards and letters: depth-tested against the world but writing
        // no depth, so the letters sit over the board they stand on.
        let p_plate = scene("plates", "vs_plate", "fs_plate", None, (true, ge, nobias), blend, tri);
        // Measured 2026-11-21 against none at all: it moves shadow edges by a
        // few pixels at most, and without it leaf crowns speckle.
        let sbias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.5,
            clamp: 0.0,
        };
        let shadow_pipe = |label, vs, fs, cull| {
            pipeline(
                &device,
                PipeDesc {
                    label,
                    layout: &shadow_layout,
                    module: &shadow_mod,
                    vs,
                    fs,
                    color: None,
                    topology: tri,
                    cull,
                    depth: Some((true, wgpu::CompareFunction::LessEqual, sbias)),
                    samples: 1,
                },
            )
        };
        // Terrain draws only faces turned away from the sun (see back_dirs), so
        // culling faces that point at the light changes nothing for flat ground
        // and keeps sunlit slopes out of the map.
        let p_shadow_terrain = shadow_pipe("shadow terrain", "vs_shadow_terrain", None, Some(wgpu::Face::Front));
        // Entities too cast from the faces turned from the light: their
        // models are closed (the scene pass culls their back faces), so a lit
        // face is compared against the far side of its own body and can take
        // the tight shadow bias (scene.wgsl SHADOW_TIGHT).
        let p_shadow_entity = shadow_pipe("shadow entity", "vs_shadow_entity", Some("fs_shadow_entity"), if std::env::var("VOXEL_SHADOW_BIAS").is_ok_and(|v| v == "legacy") { None } else { Some(wgpu::Face::Front) });
        let p_rays = pipeline(
            &device,
            PipeDesc {
                label: "rays",
                layout: &layout(&[&rays_bgl]),
                module: &post_mod,
                vs: "vs_post",
                fs: Some("fs_rays"),
                color: Some((HDR, None)),
                topology: tri,
                cull: None,
                depth: None,
                samples: 1,
            },
        );
        let p_post = pipeline(
            &device,
            PipeDesc {
                label: "post",
                layout: &post_layout,
                module: &post_mod,
                vs: "vs_post",
                fs: Some("fs_post"),
                color: Some((out_format, None)),
                topology: tri,
                cull: None,
                depth: None,
                samples: 1,
            },
        );
        let p_hud = pipeline(
            &device,
            PipeDesc {
                label: "hud",
                layout: &hud_layout,
                module: &hud_mod,
                vs: "vs_hud",
                fs: Some("fs_hud"),
                color: Some((out_format, blend)),
                topology: tri,
                cull: None,
                depth: None,
                samples: 1,
            },
        );

        let tl = TargetLayouts {
            post_bgl: post_bgl.clone(),
            rays_bgl,
            post_buf: post_buf.clone(),
            shaft_buf: shaft_buf.clone(),
            lens_buf: lens_buf.clone(),
            grade_buf: grade_buf.clone(),
            grade_view: grade_tex.create_view(&Default::default()),
            linear,
        };
        let targets = Self::make_targets(&device, &tl, width, height, msaa);
        let mut occlusion = occlusion::Occlusion::new(&device, msaa > 1, MAX_SLOTS as u64, MAX_DRAWS);
        occlusion.resize(&device, width, height, &targets.depth, &indirect_buf);
        Self {
            heights_tex: hm,
            palette_buf,
            terrain: TerrainBuffers {
                queue: queue.clone(),
                quad_buf,
                chunk_buf,
                alloc: RangeAlloc::new(quad_cap),
                free_slots: Vec::new(),
                next_slot: 0,
                scratch: Vec::new(),
                full_warned: false,
                changes: 0,
            },
            prof: if gpu.timestamps { Some(gpuprof::GpuProf::new(&device, &queue)) } else { None },
            mist_tex,
            horizon_tex,
            mist_base,
            water: [[0.0; 4]; 6],
            mist_any: false,
            air: daylight::apply(look(Style::Classic), &Sun::default(), crate::world::clock::START).2,
            looks: [look(Style::Classic), look(Style::Saturated)],
            grades: {
                let env = std::env::var("VOXEL_GRADE").unwrap_or_default();
                let mut classic = Grade::classic();
                let mut saturated = Grade::default();
                classic.apply_env(&env);
                saturated.apply_env(&env);
                [classic, saturated]
            },
            grade_tex,
            grade_sent: None,
            device,
            queue,
            multi_draw: gpu.multi_draw,
            width,
            height,
            msaa,
            sun: Sun::default(),
            globals_buf,
            indirect_buf,
            cascade_buf,
            einst_buf,
            equad_buf,
            hud_buf,
            hud_screen_buf,
            map_tex,
            backdrop_tex,
            scene_bg,
            shadow_in_bg,
            cascade_bg,
            hud_bg,
            shadow_layer_views,
            targets,
            p_sky,
            p_terrain,
            p_entity,
            p_entity_glass,
            p_water,
            p_water_basin,
            p_river,
            p_fall,
            p_spray,
            river_buf,
            spray_buf,
            river_n: 0,
            fall_n: 0,
            spray_n: 0,
            p_box,
            p_overlay,
            p_overlay_top,
            overlay_buf,
            overlay_lines: Vec::new(),
            lamp_buf,
            lamp_n: 0,
            p_plate,
            plate_buf,
            plate_n: 0,
            font_atlas_view: atlas_view,
            font_samp: mip_samp,
            p_shadow_terrain,
            p_shadow_entity,
            p_post,
            p_rays,
            tl,
            p_hud,
            foliage,
            particles,
            signs,
            models: Vec::new(),
            draws: Vec::new(),
            quad_index,
            main_list: Vec::new(),
            sprite_list: Vec::new(),
            shadow_lists: Default::default(),
            occlusion,
            cull_boxes: Vec::new(),
            cull_owner: Vec::new(),
            inst_scratch: Vec::new(),
            runs: Vec::new(),
            cascades: [CascadeCache::default(); SHADOW_LAYERS as usize],
            sliced: [Sliced { shown: [1, 2], drawing: 3, slice: 0 }, Sliced { shown: [4, 5], drawing: 6, slice: 0 }],
            cam_step: Vec3::ZERO,
            last_cam: None,
            cascade_redraws: [0; 2],
            draw_order: Vec::new(),
            frame_no: 0,
            splits: [16.0, 50.0, 160.0],
            world_size,
            sea_level,
            tide_range: 0.0,
            surf: [0.0; 4],
            swell: [0.0; 4],
            under: [14.0, 0.16],
            blocks: tex,
            blocks_samp: samp,
            scene_bgl,
            post_buf,
            shaft_buf,
            lens_buf,
            style: Style::Classic,
            mat_buf: mat_buf.clone(),
            mat_gen,
        }
    }

    fn make_targets(device: &wgpu::Device, tl: &TargetLayouts, w: u32, h: u32, msaa: u32) -> Targets {
        let tex = |label: &str, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w.max(1),
                        height: h.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let ra = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let hdr = tex("hdr", HDR, 1, ra | wgpu::TextureUsages::TEXTURE_BINDING);
        let hdr_msaa = if msaa > 1 { tex("hdr msaa", HDR, msaa, ra) } else { hdr.clone() };
        let depth = tex("depth", DEPTH, msaa, ra | wgpu::TextureUsages::TEXTURE_BINDING);
        let rays = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("rays"),
                size: wgpu::Extent3d {
                    width: w.div_ceil(2).max(1),
                    height: h.div_ceil(2).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR,
                usage: ra | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let post_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &tl.post_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tl.post_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: tl.shaft_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&rays),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&tl.linear),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: tl.lens_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&tl.grade_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: tl.grade_buf.as_entire_binding(),
                },
            ],
        });
        let rays_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rays"),
            layout: &tl.rays_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: tl.shaft_buf.as_entire_binding(),
                },
            ],
        });
        Targets {
            hdr_msaa,
            hdr,
            depth,
            post_bg,
            rays,
            rays_bg,
        }
    }

    /// Replace the page image (sRGB RGBA, IMAGE_PX square).
    pub fn upload_map(&self, rgba: &[u8]) {
        let n = hud::IMAGE_PX as u32;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.map_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(n * 4),
                rows_per_image: Some(n),
            },
            wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Replace backdrop picture `slot` (0 or 1): sRGB RGBA, BACKDROP_W x BACKDROP_H.
    pub fn upload_backdrop(&self, slot: usize, rgba: &[u8]) {
        let (w, h) = (hud::BACKDROP_W, hud::BACKDROP_H);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.backdrop_tex[slot],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Upload the HUD's items and the screen size; returns how many to draw.
    fn upload_hud(&self, hud: &Hud) -> u32 {
        let n_hud = hud.items.len().min(MAX_HUD as usize);
        if n_hud > 0 {
            self.queue.write_buffer(&self.hud_buf, 0, bytemuck::cast_slice::<HudInst, u8>(&hud.items[..n_hud]));
        }
        self.queue
            .write_buffer(&self.hud_screen_buf, 0, bytemuck::cast_slice(&[self.width as f32, self.height as f32, hud::IMAGE_PX as f32, 0.0]));
        n_hud as u32
    }

    /// Draw only the HUD over black: the menus and the loading screen, when
    /// there is no world to draw behind them.
    pub fn render_ui(&mut self, hud: &Hud, target: &wgpu::TextureView) {
        let n_hud = self.upload_hud(hud);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if n_hud > 0 {
                pass.set_pipeline(&self.p_hud);
                pass.set_bind_group(0, &self.hud_bg, &[]);
                pass.draw(0..6, 0..n_hud);
            }
        }
        self.queue.submit(Some(enc.finish()));
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w == self.width && h == self.height) {
            return;
        }
        self.width = w;
        self.height = h;
        self.targets = Self::make_targets(&self.device, &self.tl, w, h, self.msaa);
        self.occlusion.resize(&self.device, w, h, &self.targets.depth, &self.indirect_buf);
    }

    pub fn msaa(&self) -> u32 {
        self.msaa
    }

    /// How many times each outer shadow cascade has had to be redrawn whole
    /// since startup (a hitch of a million quads or more; see `SLICES`).
    pub fn cascade_redraws(&self) -> [u64; 2] {
        self.cascade_redraws
    }

    /// Upload entity models (each at most 32^3 model voxels) once at startup.
    /// Set how misty the land is: `mist` is `n` x `n` cells over the world,
    /// 0 (clear) to 1 (thick banks), from the biomes (Generator::mist_grid).
    /// Show a world: its heightmap (HEIGHTS_N square, metres, for the water and
    /// the mist), size and sea level (high water). `shore` is HEIGHTS_N square
    /// too, or empty: per texel 1 where the ground is a hollow below sea level
    /// the open sea cannot reach (`Fields::basin_grid`), metres seaward of the
    /// coast and how open the sea is (`surf::shore_grid`). Every terrain chunk
    /// of the last world is dropped; call set_tide, set_surf, set_mist and
    /// set_models after.
    pub fn set_world(&mut self, heights: &[f32], shore: &[[f32; 3]], world_size: f32, sea_level: f32) {
        let n = HEIGHTS_N;
        assert_eq!(heights.len(), (n * n) as usize, "heightmap must be HEIGHTS_N square");
        let texels: Vec<f32> = heights
            .iter()
            .enumerate()
            .flat_map(|(i, h)| {
                let [b, s, o] = shore.get(i).copied().unwrap_or([0.0; 3]);
                [*h, b, s, o]
            })
            .collect();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.heights_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&texels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16 * n),
                rows_per_image: Some(n),
            },
            wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: 1,
            },
        );
        self.mist_base = mist_ground(heights, n as usize, MIST_N as usize, sea_level, world_size);
        // The skyline and sky openness of this land (horizon.rs): what
        // shadows the distance, where the cascades cannot reach.
        let t = std::time::Instant::now();
        let hz = horizon::bake(heights, n as usize, world_size);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.horizon_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &hz,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(horizon::HORIZON_N),
                rows_per_image: Some(horizon::HORIZON_N),
            },
            wgpu::Extent3d {
                width: horizon::HORIZON_N,
                height: horizon::HORIZON_N,
                depth_or_array_layers: horizon::LAYERS,
            },
        );
        if std::env::var("VOXEL_TIMING").is_ok() {
            eprintln!("horizon map baked in {:.0} ms", t.elapsed().as_secs_f32() * 1000.0);
        }
        self.occlusion.reset();
        self.world_size = world_size;
        self.sea_level = sea_level;
        self.tide_range = 0.0;
        self.surf = [0.0; 4];
        self.swell = [0.0; 4];
        self.terrain.reset();
        self.river_n = 0;
        self.fall_n = 0;
        self.spray_n = 0;
    }

    /// The rivers' water surface (`world::rivers::surface`) and the
    /// waterfalls' sheets, which share a buffer, and where each fall lands;
    /// empty for none.
    pub fn set_rivers(&mut self, quads: &[RiverQuad], falls: &[RiverQuad], spray: &[Spray]) {
        let all: Vec<RiverQuad> = quads.iter().chain(falls).copied().collect();
        if all.len() > RIVER_CAP {
            eprintln!("rivers: {} surface cells and fall pieces, drawing none past {RIVER_CAP}", all.len());
        }
        let n = all.len().min(RIVER_CAP);
        if n > 0 {
            self.queue.write_buffer(&self.river_buf, 0, bytemuck::cast_slice(&all[..n]));
        }
        self.river_n = quads.len().min(n) as u32;
        self.fall_n = falls.len().min(n - self.river_n as usize) as u32;
        self.spray_n = spray.len().min(SPRAY_CAP) as u32;
        if self.spray_n > 0 {
            self.queue.write_buffer(&self.spray_buf, 0, bytemuck::cast_slice(&spray[..self.spray_n as usize]));
        }
    }

    /// Replace the light a style is drawn in.
    pub fn set_look(&mut self, style: Style, look: Look) {
        self.looks[style as usize] = look;
    }

    pub fn look_of(&self, style: Style) -> Look {
        self.looks[style as usize]
    }

    /// The light and the grade of a style, to edit in place (the LOOK page).
    /// Both are uploaded on the next frame.
    pub fn look_and_grade_mut(&mut self, style: Style) -> (&mut Look, &mut Grade) {
        let i = style as usize;
        (&mut self.looks[i], &mut self.grades[i])
    }

    /// Replace a style's grade (its gradient strips and knobs).
    pub fn set_grade(&mut self, style: Style, grade: Grade) {
        self.grades[style as usize] = grade;
    }

    pub fn grade_of(&self, style: Style) -> &Grade {
        &self.grades[style as usize]
    }

    /// Send this style's grade: the knobs every frame (they are a few bytes),
    /// the strip only when its stops changed.
    /// Send the style's grade, with the weather's and the season's own
    /// exposure and saturation over it (world/weather.rs, world/season.rs): a
    /// wet day is darker and greyer, a hot one brighter and bolder, winter
    /// paler, and fair summer weather multiplies by one. Only the copy on the
    /// GPU is touched, so the LOOK page still edits the style's own.
    fn upload_grade(&mut self, style: Style, sky: &crate::world::weather::Sky, season: &crate::world::season::Phase, plain: bool) {
        let i = style as usize;
        let g = if plain { Grade::default() } else { self.grades[i].clone() };
        let mut u = g.uniform();
        u[10] *= (sky.brightness * season.brightness).max(0.0);
        u[13] *= (sky.saturation * season.saturation).max(0.0);
        self.queue.write_buffer(&self.tl.grade_buf, 0, bytemuck::cast_slice(&u));
        if self.grade_sent.as_ref().is_some_and(|(s, sent)| *s == i && sent.ramp == g.ramp && sent.map == g.map) {
            return;
        }
        let px: Vec<u16> = g.strip().iter().flat_map(|c| c.iter().map(|v| f16_bits(*v)).collect::<Vec<_>>()).collect();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.grade_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&px),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(grade::STRIP_N * 8),
                rows_per_image: Some(grade::STRIP_ROWS),
            },
            wgpu::Extent3d {
                width: grade::STRIP_N,
                height: grade::STRIP_ROWS,
                depth_or_array_layers: 1,
            },
        );
        self.grade_sent = Some((i, g));
    }

    /// Water colours (Generator::water_grid) and each alternate's share per
    /// cell of an n x n grid go in with the mist, in the mist map's b and a.
    /// The tide's range in metres (world/tide.rs); 0, the default after
    /// `set_world`, is a still sea.
    pub fn set_tide(&mut self, range: f32) {
        self.tide_range = range.max(0.0);
    }

    /// The surf breaking on this world's shores and the swell on its open
    /// sea (world/surf.rs); none after `set_world`.
    pub fn set_surf(&mut self, s: [f32; 4], swell: [f32; 4]) {
        self.surf = s;
        self.swell = swell;
    }

    fn swell_on(&self) -> bool {
        self.swell[2] > 0.0
    }

    fn tide_on(&self) -> bool {
        self.tide_range > 0.0
    }

    /// The world's underwater visibility and light falloff.
    pub fn set_underwater(&mut self, u: crate::world::fields::Underwater) {
        self.under = [u.visibility.max(1.0), u.light.max(0.0)];
    }

    pub fn set_mist(&mut self, mist: &[f32], n: usize, water: ([[f32; 4]; 6], Vec<[f32; 2]>)) {
        self.water = water.0;
        let shares = water.1;
        let m = MIST_N as usize;
        let mut px = vec![0u16; m * m * 4];
        for z in 0..m {
            for x in 0..m {
                let sx = ((x as f32 + 0.5) / m as f32 * n as f32) as usize;
                let sz = ((z as f32 + 0.5) / m as f32 * n as f32) as usize;
                let v = mist.get(sz.min(n - 1) * n + sx.min(n - 1)).copied().unwrap_or(0.0);
                let i = (z * m + x) * 4;
                px[i] = f16_bits(v);
                px[i + 1] = f16_bits(self.mist_base[z * m + x]);
                if let Some(s) = shares.get(sz.min(n - 1) * n + sx.min(n - 1)) {
                    px[i + 2] = f16_bits(s[0]);
                    px[i + 3] = f16_bits(s[1]);
                }
            }
        }
        self.mist_any = mist.iter().any(|v| *v > 0.001);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.mist_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&px),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(m as u32 * 8),
                rows_per_image: Some(m as u32),
            },
            wgpu::Extent3d {
                width: MIST_N,
                height: MIST_N,
                depth_or_array_layers: 1,
            },
        );
    }

    /// A new material table is installed: its recipes may differ and their
    /// number may too, so build the block layers again for the current style,
    /// in a new array (and scene bind group) when the count changed.
    fn reload_blocks(&mut self) {
        let layers = crate::world::matlib::layer_count().max(1);
        if layers != self.blocks.depth_or_array_layers() {
            self.blocks = make_blocks(&self.device, layers);
            self.rebuild_scene_bg();
        }
        upload_blocks(&self.queue, &self.blocks, self.style);
    }

    /// Make the scene bind group again, after one of the resources in it
    /// was replaced (the block array, the palette).
    fn rebuild_scene_bg(&mut self) {
        {
            let blocks = blocks_view(&self.blocks);
            let heights = self.heights_tex.create_view(&Default::default());
            let mist = self.mist_tex.create_view(&Default::default());
            let horizon = self.horizon_tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            self.scene_bg = scene_bind_group(
                &self.device,
                &self.scene_bgl,
                &SceneRes {
                    globals: &self.globals_buf,
                    quads: &self.terrain.quad_buf,
                    chunks: &self.terrain.chunk_buf,
                    mats: &self.mat_buf,
                    blocks: &blocks,
                    blocks_samp: &self.blocks_samp,
                    heights: &heights,
                    palette: &self.palette_buf,
                    equads: &self.equad_buf,
                    einst: &self.einst_buf,
                    mist: &mist,
                    horizon: &horizon,
                    overlay: &self.overlay_buf,
                    lamps: &self.lamp_buf,
                    rivers: &self.river_buf,
                    sprays: &self.spray_buf,
                    plates: &self.plate_buf,
                    font_atlas: &self.font_atlas_view,
                    font_samp: &self.font_samp,
                    grade_strip: &self.grade_tex.create_view(&Default::default()),
                    grade: &self.tl.grade_buf,
                },
            );
        }
    }

    /// Upload the entity palette alone: the fixed block and every model's
    /// block (models::FIXED_SLOTS). The buffer grows to fit, so a world has
    /// no limit on its colours. The EDIT page paints a creature's colours
    /// without rebuilding a thing, and re-sending every mesh for that would
    /// be the one expensive part left.
    pub fn set_palette(&mut self, palette: &[[f32; 4]]) {
        let bytes = std::mem::size_of_val(palette) as u64;
        if bytes > self.palette_buf.size() {
            // Room to spare, so the EDIT page adding a colour or two does not
            // make a new buffer each time.
            self.palette_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("palette"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.rebuild_scene_bg();
        }
        self.queue.write_buffer(&self.palette_buf, 0, bytemuck::cast_slice(palette));
    }

    /// Upload the entity models and their palette. Each model's palette
    /// block goes into the top half of its quads' second word, which an
    /// entity quad leaves free (mesh/quad.rs) and `vs_entity` adds to the
    /// voxel's slot.
    pub fn set_models(&mut self, models: &[crate::models::Model], palette: &[[f32; 4]]) {
        self.set_palette(palette);
        let mut all: Vec<[u32; 4]> = Vec::new();
        self.models.clear();
        for m in models {
            assert!(m.block <= 0xffff, "palette block {} past what a quad holds", m.block);
            self.models.push(ModelRange {
                offset: all.len() as u32,
                count: m.mesh.quads.len() as u32,
            });
            all.extend(m.mesh.quads.iter().map(|q| [q[0], (q[1] & 0xffff) | m.block << crate::mesh::quad::SLOT_SHIFT, q[2], q[3]]));
        }
        assert!(all.len() as u32 <= MAX_ENTITY_QUADS, "{} entity quads, past MAX_ENTITY_QUADS", all.len());
        self.queue.write_buffer(&self.equad_buf, 0, bytemuck::cast_slice(&all));
    }

    pub fn render(&mut self, streamer: &Streamer, p: &FrameParams, hud: &Hud, target: &wgpu::TextureView) -> FrameStats {
        let t0 = Instant::now();
        let mut stats = FrameStats::default();
        let view_proj = p.proj * p.view;
        let shadows = p.shadows;
        if crate::world::matlib::generation() != self.mat_gen {
            self.mat_gen = crate::world::matlib::generation();
            self.queue.write_buffer(&self.mat_buf, 0, bytemuck::cast_slice(&material_rows()));
            self.reload_blocks();
        }
        if p.style != self.style {
            upload_blocks(&self.queue, &self.blocks, p.style);
            self.style = p.style;
        }
        self.upload_grade(self.style, &p.sky, &p.season, p.plain_grade);
        // 1 for the saturated style's soft path (shaders still call it calm).
        let calm = if self.style == Style::Saturated { 1.0f32 } else { 0.0 };
        // z: the projection's near distance, to turn reversed depth into metres.
        let near = p.proj.w_axis.z;
        // This frame's sun first: the grade and the rays read it.
        let (mut lk, mut sun, mut air) = daylight::apply(self.looks[self.style as usize], &Sun::default(), p.hour);
        if self.style == Style::Saturated {
            daylight::palette_study(&mut lk, &mut sun, &mut air);
        }
        // The weather, over whatever the hour decided (world/weather.rs),
        // and the season's own warm or cool over that (world/season.rs).
        daylight::weather(&mut lk, &mut sun, &mut air, &p.sky);
        // A lightning flash lights the whole sky, and so the land under it,
        // a cold blue-white for as long as the stroke lasts.
        let flash = p.bolt[3].clamp(0.0, 1.0);
        if flash > 0.0 {
            for (c, k) in [(&mut lk.zenith, 0.45), (&mut lk.horizon, 0.6), (&mut lk.ambient, 0.4)] {
                c[0] += 0.7 * k * flash;
                c[1] += 0.76 * k * flash;
                c[2] += 1.0 * k * flash;
            }
        }
        // Fog lying in the hollows after a clear, still night (world/damp.rs).
        air.mist_floor += p.ground_fog;
        air.warmth += p.season.warmth;
        self.sun = sun;
        self.air = air;
        // post.wgsl `Post`: the style, then the streaks and a wrapped clock
        // for them (they move along their rays).
        self.queue.write_buffer(
            &self.post_buf,
            0,
            bytemuck::cast_slice(&[
                calm,
                p.speed_fx.clamp(0.0, 1.0),
                near,
                self.air.warmth,
                p.streaks.clamp(0.0, 1.0),
                p.time.rem_euclid(64.0),
                p.mirage_floor,
                0.0,
                // The weather's falling stuff, where the camera is looking so
                // it sweeps past as you turn, and the horizon colour the rain
                // is lit by (post.wgsl `precipitation`).
                p.sky.rain.clamp(0.0, 1.0),
                p.sky.snow.clamp(0.0, 1.0),
                p.sky.wind.max(0.0),
                p.cam_fwd.z.atan2(p.cam_fwd.x),
                p.cam_fwd.y,
                lk.horizon[0],
                lk.horizon[1],
                lk.horizon[2],
                // The mirage and the reflections (post.wgsl `mirage`, `mirror`).
                p.cam_fwd.y.clamp(-1.0, 1.0).asin(),
                1.0 / p.proj.y_axis.y.max(1e-4),
                if p.underwater { 0.0 } else { p.mirage.clamp(0.0, 1.0) },
                if p.underwater || !reflections_on() { 0.0 } else { 1.0 },
                p.cam_pos.x,
                p.cam_pos.y,
                p.cam_pos.z,
                p.sea,
            ]),
        );
        let vp = p.proj * p.view;
        self.queue.write_buffer(&self.post_buf, 96, bytemuck::cast_slice(&vp.to_cols_array()));
        self.queue.write_buffer(&self.post_buf, 160, bytemuck::cast_slice(&vp.inverse().to_cols_array()));
        let l = p.lens.unwrap_or_default();
        let lens = [l.frame[0], l.frame[1], l.frame[2], l.frame[3], l.focus_m, l.blur_inf, l.blur_max, l.mask];
        self.queue.write_buffer(&self.lens_buf, 0, bytemuck::cast_slice(&lens));
        let shafts = foliage::sun_shafts(p.proj * p.view, p.cam_pos, self.sun.dir, self.sun.intensity, p.time, p.underwater, self.air.night);
        self.queue.write_buffer(&self.shaft_buf, 0, bytemuck::cast_slice(&shafts));

        // Shadow maps: decide what to draw this frame.
        self.frame_no += 1;
        // The camera's drift between frames, smoothed over about five; the
        // outer cascades are centred ahead of it (see LEAD_CYCLES).
        let step = self.last_cam.map_or(Vec3::ZERO, |l| p.cam_pos - l);
        self.last_cam = Some(p.cam_pos);
        self.cam_step = if step.length() > JUMP { Vec3::ZERO } else { self.cam_step * 0.8 + step * 0.2 };
        // Below a centimetre a frame the player is standing still, and the
        // cascades should sit on them exactly as they did before the lead.
        if self.cam_step.length() < 0.01 {
            self.cam_step = Vec3::ZERO;
        }
        let mut jobs: Vec<ShadowJob> = Vec::with_capacity(3);
        let mut blend = [0.0f32; 2];
        if shadows {
            let changes = self.terrain.changes;
            let cam = p.cam_pos;
            let sun_dir = self.sun.dir;
            // Fill layer `l` for cascade radius `r`, continuing the grid phase of
            // layer `from` so the texel grid stays put (see shift_phase).
            let place = |cascades: &mut [CascadeCache], l: usize, from: usize, r: f32, centre: Vec3| {
                let f = cascades[from];
                let phase = if f.valid {
                    let (_, raw, _) = cascade_matrix(sun_dir, centre, r, f.phase);
                    shift_phase(f.phase, f.raw, raw, cam)
                } else {
                    glam::Vec2::ZERO
                };
                let (m, raw, t) = cascade_matrix(sun_dir, centre, r, phase);
                cascades[l] = CascadeCache {
                    mat: m,
                    texel: t,
                    centre,
                    radius: r,
                    sun: sun_dir,
                    terrain_changes: changes,
                    valid: true,
                    raw,
                    phase,
                };
                m
            };
            let write = |l: usize, m: Mat4| {
                let mut bytes = [0u8; 256];
                bytes[..64].copy_from_slice(bytemuck::cast_slice(&m.to_cols_array()));
                self.queue.write_buffer(&self.cascade_buf, l as u64 * 256, &bytes);
            };
            // Cascade 0, around the player, follows everything every frame.
            let r0 = (self.splits[0] * 4.0).ceil() / 4.0;
            let m = place(&mut self.cascades, 0, 0, r0, cam);
            write(0, m);
            jobs.push(ShadowJob {
                layer: 0,
                slice: None,
                clear: true,
                creatures: true,
            });
            for k in 0..2 {
                let i = k + 1;
                let n = SLICES[k];
                let mut st = self.sliced[k];
                let r = ((self.splits[i] + SLICE_MARGIN[k]) * 4.0).ceil() / 4.0;
                // Where the camera will be in the middle of the next map's
                // life. Capped at the margin: any further and the map would
                // stop reaching back to its own split (see LEAD_CYCLES).
                let centre = cam + (self.cam_step * (LEAD_CYCLES * n as f32)).clamp_length_max(SLICE_MARGIN[k]);
                let (a, b) = (self.cascades[st.shown[0]], self.cascades[st.shown[1]]);
                let reset = !a.valid || !b.valid || b.radius != r || (cam - a.centre).length() > SLICE_MARGIN[k] || a.sun.dot(sun_dir) < 0.9998;
                if reset {
                    // Draw it whole now and show that map until the next one.
                    self.cascade_redraws[k] += 1;
                    let l = st.drawing;
                    let m = place(&mut self.cascades, l, st.shown[1], r, centre);
                    write(l, m);
                    jobs.push(ShadowJob {
                        layer: l,
                        slice: None,
                        clear: true,
                        creatures: i == 1,
                    });
                    let base = 1 + 3 * k;
                    st = Sliced {
                        shown: [l, l],
                        drawing: if l == base { base + 1 } else { base },
                        slice: 0,
                    };
                } else {
                    // Keep cycling while anything differs from the newest map
                    // (creatures move, so cascade 1 always does), and once more
                    // after it stops so both shown maps agree.
                    let differs = |c: &CascadeCache| c.sun != sun_dir || c.terrain_changes != changes || (cam - c.centre).length() > 0.5;
                    let unequal = a.sun != b.sun || a.centre != b.centre || a.terrain_changes != b.terrain_changes;
                    if st.slice > 0 || i == 1 || differs(&b) || unequal {
                        let l = st.drawing;
                        if st.slice == 0 {
                            let m = place(&mut self.cascades, l, st.shown[1], r, centre);
                            write(l, m);
                        }
                        jobs.push(ShadowJob {
                            layer: l,
                            slice: Some((st.slice, n)),
                            clear: st.slice == 0,
                            creatures: i == 1 && st.slice == n - 1,
                        });
                        st.slice += 1;
                        if st.slice == n {
                            let base = 1 + 3 * k;
                            let shown = [st.shown[1], l];
                            let free = (base..base + 3).find(|x| !shown.contains(x)).unwrap_or(base);
                            st = Sliced { shown, drawing: free, slice: 0 };
                        }
                    }
                }
                blend[k] = st.slice as f32 / n as f32;
                self.sliced[k] = st;
            }
        } else {
            for c in &mut self.cascades {
                c.valid = false;
            }
        }
        let (s1, s2) = (self.sliced[0], self.sliced[1]);
        let shadow_mats = [
            self.cascades[0].mat,
            self.cascades[s1.shown[0]].mat,
            self.cascades[s1.shown[1]].mat,
            self.cascades[s2.shown[0]].mat,
            self.cascades[s2.shown[1]].mat,
        ];
        let texels = [self.cascades[0].texel, self.cascades[s1.shown[1]].texel, self.cascades[s2.shown[1]].texel, daylight::path().shadow_soft];
        let shadow_layers = [s1.shown[0] as f32, s1.shown[1] as f32, s2.shown[0] as f32, s2.shown[1] as f32];
        let job_spheres: Vec<(Vec3, f32, Option<(u32, u32)>)> = jobs.iter().map(|j| (self.cascades[j.layer].centre, self.cascades[j.layer].radius, j.slice)).collect();

        // Cull and build indirect draws.
        let t_dl = Instant::now();
        let planes = frustum_planes(view_proj);
        let cam = p.cam_pos;
        let sun = self.sun.dir;
        let back_dirs: [bool; 6] = std::array::from_fn(|d| crate::mesh::dir_normal(d).dot(sun) < 0.0);
        // For the other directions, the tilted quads that could still face away
        // from the sun: a class leaning at most b can when the axis is more than
        // 90 - b degrees from the sun.
        let shadow_lean: [usize; 6] = std::array::from_fn(|d| {
            let a = crate::mesh::dir_normal(d).dot(sun).clamp(-1.0, 1.0).acos();
            crate::mesh::LEANS.iter().take_while(|(cos, _)| a + cos.acos() <= std::f32::consts::FRAC_PI_2).count()
        });
        self.draws.clear();
        // Main-pass draws carry their distance so they can go out front to back,
        // letting the depth test reject hidden fragments before shading.
        let mut main = std::mem::take(&mut self.main_list);
        let mut sprite_list = std::mem::take(&mut self.sprite_list);
        sprite_list.clear();
        let mut boxes = std::mem::take(&mut self.cull_boxes);
        boxes.clear();
        let mut shadow_lists = std::mem::take(&mut self.shadow_lists);
        main.clear();
        for l in &mut shadow_lists {
            l.clear();
        }
        // A column's whole footprint is tested before any of its chunks:
        // every chunk in it stands in the same 8 m (or coarser) square, so
        // one plane test throws out the columns to the side and behind
        // instead of one per chunk. At a 256 m radius that is about 7,800
        // columns walked for 3,400 visible chunks, and most of the walk used
        // to be spent proving the same column invisible two or three times.
        // The box is as tall as the world, which is conservative: a column
        // kept by it is still culled chunk by chunk below.
        let col_hi = streamer.world_height_m();
        // Under water nothing past about five visibilities shows (the haze
        // has hidden 99% of it), so those chunks are not drawn at all; this
        // is what pays for dense coral. Shadows still use the full test.
        let far = if p.underwater { self.under[0] * 5.0 } else { f32::INFINITY };
        let near_enough = |min: Vec3, max: Vec3| cam.clamp(min, max).distance(cam) <= far;
        for (key, list) in &streamer.loaded {
            if !streamer.is_drawn(key) {
                continue;
            }
            let (c0, c1) = key.footprint_m();
            // The same slack a chunk box gets: a sloped or rounded corner
            // moves up to a voxel, so geometry can hang a voxel outside the
            // footprint (TerrainBuffers::upload).
            let slack = key.size_m() / 32.0;
            let cmin = Vec3::new(c0.x - slack, -slack, c0.y - slack);
            let cmax = Vec3::new(c1.x + slack, col_hi + slack, c1.y + slack);
            let col_seen = aabb_in_frustum(&planes, cmin, cmax) && near_enough(cmin, cmax);
            // Shadow casters need the same test against each job's sphere, or
            // a column behind the camera would be walked for its shadows
            // alone. A sphere test against the whole footprint is cheap and
            // the chunk loop below repeats it exactly.
            let col_casts = shadows
                && job_spheres.iter().any(|(sc, sr, _)| {
                    let centre = (cmin + cmax) * 0.5;
                    let rc = (cmax - cmin).length() * 0.5;
                    let v = centre - *sc;
                    let along = v.dot(sun);
                    let perp = (v - sun * along).length();
                    perp <= sr * 1.42 + rc && along >= -300.0 - rc
                });
            // VOXEL_COLUMN_CULL=0 keeps every column, for checking that the
            // test changes nothing but the time it takes.
            let (col_seen, col_casts) = if column_cull() { (col_seen, col_casts) } else { (true, shadows) };
            if !col_seen && !col_casts {
                continue;
            }
            for c in list {
                let visible = col_seen && aabb_in_frustum(&planes, c.min, c.max) && near_enough(c.min, c.max);
                if visible {
                    stats.chunks_visible += 1;
                }
                let mut off = c.offset;
                let mut dir_off = [0u32; 6];
                for d in 0..6 {
                    dir_off[d] = off;
                    off += c.dir_counts[d];
                }
                if visible {
                    let dist = ((c.min + c.max) * 0.5 - cam).length_squared();
                    let bi = boxes.len() as u32;
                    boxes.push([c.min.x, c.min.y, c.min.z, 0.0, c.max.x, c.max.y, c.max.z, 0.0]);
                    // Sprites sit after the six face groups, where `off` now points.
                    // Fill tufts come first, then the other grass: further off a
                    // chunk skips the runs that have shrunk to nothing by then.
                    // The far grass patches come last: near chunks leave them off,
                    // and past the leaves a chunk draws only them.
                    let far = foliage::FAR.max(foliage::PATCH_FAR).max(foliage::STALK_FAR);
                    if self.foliage.on && c.sprites > 0 && dist < far * far {
                        let ranges = self.foliage.ranges;
                        let skip = if !ranges || dist < foliage::NEAR * foliage::NEAR {
                            0
                        } else if dist < foliage::GRASS_FAR * foliage::GRASS_FAR {
                            c.sprite_fill
                        } else if dist < foliage::FAR * foliage::FAR {
                            c.sprite_fill + c.sprite_grass
                        } else {
                            // Past the leaves: the stalks and the far patches.
                            c.sprites - c.sprite_patch - c.sprite_stalk
                        };
                        // The far cover is left off only when the whole chunk
                        // is nearer than where it grows in. Judged by the
                        // centre, a coarse chunk (16 to 256 m across) still
                        // standing in under the camera while the full detail
                        // loads dropped all its cover, out to its far side:
                        // bare ground near, cover further off.
                        let reach = (cam - c.min).abs().max((cam - c.max).abs()).length();
                        let end = if ranges && reach < foliage::PATCH_FADE[0] { c.sprites - c.sprite_patch } else { c.sprites };
                        let n = end.saturating_sub(skip);
                        if n > 0 {
                            sprite_list.push((dist, qdraw(n, off + skip), bi));
                            stats.sprites += n as u64;
                        }
                    }
                    for d in 0..6 {
                        let n = c.dir_counts[d];
                        if n == 0 {
                            continue;
                        }
                        let faces = match d {
                            0 => cam.x > c.min.x,
                            1 => cam.x < c.max.x,
                            2 => cam.y > c.min.y,
                            3 => cam.y < c.max.y,
                            4 => cam.z > c.min.z,
                            _ => cam.z < c.max.z,
                        };
                        if faces {
                            main.push((dist, qdraw(n, dir_off[d]), bi));
                            stats.quads += n as u64;
                        } else if p.slopes && c.tails[0][d] + c.rtails[0][d] > 0 {
                            // Tilted quads (sloped ground, rounded trees) can face a
                            // camera on the far side of the group's plane, e.g. a
                            // hillside above the eye seen from below, as far behind
                            // as their lean allows. Sloped quads end the group;
                            // rounded ones end just before them (see Mesh::tails).
                            let lean = lean_needed(d, cam, c.min, c.max);
                            let (t, r) = (c.tails[lean][d], c.rtails[lean][d]);
                            let sloped_start = dir_off[d] + n - c.tails[0][d];
                            for (start, k) in [(dir_off[d] + n - t, t), (sloped_start - r, r)] {
                                if k > 0 {
                                    main.push((dist, qdraw(k, start), bi));
                                    stats.quads += k as u64;
                                }
                            }
                        }
                    }
                }
                if col_casts {
                    let centre = (c.min + c.max) * 0.5;
                    let rc = (c.max - c.min).length() * 0.5;
                    for (ci, (sc, sr, slice)) in job_spheres.iter().enumerate() {
                        let v = centre - *sc;
                        let along = v.dot(sun);
                        let perp = (v - sun * along).length();
                        if perp > sr * 1.42 + rc || along < -300.0 - rc {
                            continue;
                        }
                        // The slice hash costs a division, so only for the
                        // chunks the cascade reaches at all.
                        if let Some((k, n)) = slice {
                            if chunk_slice(c.min, *n) != *k {
                                continue;
                            }
                        }
                        for d in 0..6 {
                            // Directions facing away from the sun cast shadows; of
                            // the others, only sloped quads (kept at the end of the
                            // group) can, and the pipeline culls the ones that face
                            // the sun. Rounded trees are drawn as their square voxel
                            // faces here (see the shadow shader), so theirs come
                            // only from the directions facing away.
                            let n = c.dir_counts[d];
                            let (start, n) = if back_dirs[d] {
                                (dir_off[d], n)
                            } else if p.slopes {
                                let t = c.tails[shadow_lean[d]][d];
                                (dir_off[d] + n - t, t)
                            } else {
                                (0, 0)
                            };
                            if n > 0 {
                                shadow_lists[ci].push(qdraw(n, start));
                                stats.shadow_quads += n as u64;
                            }
                        }
                    }
                }
            }
        }
        let mut order = std::mem::take(&mut self.draw_order);
        near_to_far(&mut main, &mut order);
        self.draws.extend(main.iter().map(|(_, d, _)| *d));
        let main_count = self.draws.len() as u32;
        let mut shadow_ranges = [(0u32, 0u32); 3];
        for (i, l) in shadow_lists.iter().enumerate() {
            shadow_ranges[i] = (self.draws.len() as u32, l.len() as u32);
            self.draws.extend_from_slice(l);
        }
        self.shadow_lists = shadow_lists;
        stats.draws = main_count;
        stats.shadow_draws = self.draws.len() as u32 - main_count;
        near_to_far(&mut sprite_list, &mut order);
        self.draw_order = order;
        let sprite_range = (self.draws.len() as u32, sprite_list.len() as u32);
        self.draws.extend(sprite_list.iter().map(|(_, d, _)| *d));
        let n_draws = (self.draws.len() as u64).min(MAX_DRAWS) as usize;
        if n_draws > 0 {
            self.queue.write_buffer(&self.indirect_buf, 0, bytemuck::cast_slice(&self.draws[..n_draws]));
        }
        stats.drawlist_ms = t_dl.elapsed().as_secs_f32() * 1000.0;
        let culling = self.occlusion.ready(cam);
        let mut owner = std::mem::take(&mut self.cull_owner);
        owner.clear();
        if culling {
            owner.extend(main.iter().map(|m| m.2));
            owner.resize(sprite_range.0 as usize, occlusion::NO_CHUNK);
            owner.extend(sprite_list.iter().map(|m| m.2));
            owner.truncate(n_draws);
        }

        // Globals.
        let inv = view_proj.inverse();
        let (bmin, bmax, bon) = match p.highlight {
            Some((a, b)) => (a, b, 1.0),
            None => (Vec3::ZERO, Vec3::ZERO, 0.0),
        };
        let s = &self.sun;
        let overlay_tested = p.overlay.packed(&mut self.overlay_lines);
        if !self.overlay_lines.is_empty() {
            self.queue.write_buffer(&self.overlay_buf, 0, bytemuck::cast_slice(&self.overlay_lines));
        }
        self.lamp_n = p.lamps.len().min(lamps::MAX) as u32;
        if self.lamp_n > 0 {
            self.queue.write_buffer(&self.lamp_buf, 0, bytemuck::cast_slice(&p.lamps.packed()[..self.lamp_n as usize]));
        }
        self.plate_n = p.plates.len() as u32;
        if self.plate_n > 0 {
            self.queue.write_buffer(&self.plate_buf, 0, bytemuck::cast_slice(p.plates.packed()));
        }
        self.particles.upload(&self.queue, p.fx, p.cam_pos);
        self.signs.upload(&self.queue, p.marks, p.cam_pos);
        self.foliage.upload_trample(&self.queue, p.marks, p.cam_pos);
        let au = aurora::uniform(p.hour);
        let sea_now = if self.tide_on() { p.sea } else { self.sea_level };
        let globals = Globals {
            view_proj: view_proj.to_cols_array(),
            inv_view_proj: inv.to_cols_array(),
            shadow_mats: shadow_mats.map(|m| m.to_cols_array()),
            cam_pos: [cam.x, cam.y, cam.z, p.time],
            sun_dir: [s.dir.x, s.dir.y, s.dir.z, if shadows { 1.0 } else { 0.0 }],
            sun_color: [s.color.x * lk.sun_tint.x, s.color.y * lk.sun_tint.y, s.color.z * lk.sun_tint.z, s.intensity * lk.sun_gain],
            sky_zenith: [lk.zenith[0], lk.zenith[1], lk.zenith[2], 0.0],
            sky_horizon: [lk.horizon[0], lk.horizon[1], lk.horizon[2], 0.0],
            ground_color: [lk.ground[0], lk.ground[1], lk.ground[2], 0.0],
            fog: [lk.fog_density, sea_now, if p.underwater { 1.0 } else { 0.0 }, self.world_size],
            splits: [self.splits[0], self.splits[1], self.splits[2], 0.0],
            screen: [self.width as f32, self.height as f32, 1.0 / self.width as f32, 1.0 / self.height as f32],
            cam_fwd: [p.cam_fwd.x, p.cam_fwd.y, p.cam_fwd.z, 0.0],
            shadow_texel: texels,
            box_min: [bmin.x, bmin.y, bmin.z, bon],
            box_max: [bmax.x, bmax.y, bmax.z, 0.0],
            style: [calm, 0.0, 0.0, 0.0],
            // The northern lights put a faint green into the sky's fill.
            sky_ambient: [lk.ambient[0] + 0.006 * au[0], lk.ambient[1] + 0.03 * au[0], lk.ambient[2] + 0.014 * au[0], 0.0],
            glow: [self.air.glow.x, self.air.glow.y, self.air.glow.z, self.air.inscatter],
            air_sun: [self.air.sun.x, self.air.sun.y, self.air.sun.z, self.air.night],
            mist: [self.air.mist_scale, self.air.mist_floor, MIST_HEIGHT, if self.mist_any { 1.0 } else { 0.0 }],
            shadow_layers,
            shadow_blend: [blend[0], blend[1], 0.0, 0.0],
            water: self.water,
            lamps: [self.lamp_n as f32, 0.0, 0.0, 0.0],
            weather: p.sky.uniform(),
            season: p.season.uniform(),
            aurora: au,
            under: [p.water_level, self.under[0], self.under[1], if p.water_level > sea_now + 0.05 { 1.0 } else { 0.0 }],
            snow: [p.snow_cover, if p.space { 1.0 } else { 0.0 }, 0.0, 0.0],
            studio: p.studio.map_or([0.0; 4], |(x, z, half)| [x, z, half, 1.0]),
            damp: if p.space { [0.0; 4] } else { p.damp },
            bolt: p.bolt,
            tide: [self.sea_level - sea_now, self.tide_range, self.sea_level, if self.tide_on() { 1.0 } else { 0.0 }],
            surf: if p.space { [0.0; 4] } else { self.surf },
            swell: if p.space { [0.0; 4] } else { self.swell },
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        // Entities.
        let n_inst = p.entities.len().min(MAX_INSTANCES as usize);
        if n_inst > 0 {
            self.inst_scratch.clear();
            self.inst_scratch.extend(p.entities[..n_inst].iter().map(|(_, i)| *i));
            self.queue.write_buffer(&self.einst_buf, 0, bytemuck::cast_slice(&self.inst_scratch));
        }
        let mut runs = std::mem::take(&mut self.runs);
        runs.clear();
        for (i, (m, inst)) in p.entities[..n_inst].iter().enumerate() {
            let glass = inst.shift[3] > crate::models::GLASS - 0.5;
            match runs.last_mut() {
                Some(r) if r.0 == *m && r.3 == glass => r.2 += 1,
                _ => runs.push((*m, i as u32, 1, glass)),
            }
        }

        // HUD.
        let n_hud = self.upload_hud(hud);

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        if let Some(pr) = &self.prof {
            pr.at_encoder(&mut enc, 0);
        }

        if shadows {
            for (ci, job) in jobs.iter().enumerate() {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadow_layer_views[job.layer],
                        depth_ops: Some(wgpu::Operations {
                            load: if job.clear { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load },
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_bind_group(0, &self.scene_bg, &[]);
                pass.set_bind_group(1, &self.cascade_bg, &[(job.layer * 256) as u32]);
                let (start, count) = shadow_ranges[ci];
                if count > 0 {
                    pass.set_pipeline(&self.p_shadow_terrain);
                    self.multi_draw(&mut pass, start, count);
                }
                // Creatures cast shadows in cascades 0 and 1 (on the last slice of 1).
                if !runs.is_empty() && job.creatures {
                    pass.set_pipeline(&self.p_shadow_entity);
                    for &(m, first, n, _) in &runs {
                        let r = &self.models[m];
                        pass.draw(r.offset * 6..(r.offset + r.count) * 6, first..first + n);
                    }
                }
            }
        }

        if let Some(pr) = &self.prof {
            pr.at_encoder(&mut enc, 1);
        }
        if culling {
            self.occlusion.cull(&self.queue, &mut enc, &boxes, &owner);
        }
        self.main_list = main;
        self.sprite_list = sprite_list;
        self.cull_boxes = boxes;
        self.cull_owner = owner;
        {
            let resolve = if self.msaa > 1 { Some(&self.targets.hdr) } else { None };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_msaa,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: if self.msaa > 1 { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    // Kept for post.wgsl's distance blur.
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.scene_bg, &[]);
            pass.set_bind_group(1, &self.shadow_in_bg, &[]);
            let prof = self.prof.as_ref();
            if main_count > 0 {
                pass.set_pipeline(&self.p_terrain);
                self.multi_draw(&mut pass, 0, main_count);
            }
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 2);
            }
            if !runs.is_empty() {
                pass.set_pipeline(&self.p_entity);
                for &(m, first, n, _) in runs.iter().filter(|r| !r.3) {
                    let r = &self.models[m];
                    pass.draw(r.offset * 6..(r.offset + r.count) * 6, first..first + n);
                }
            }
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 3);
            }
            if sprite_range.1 > 0 {
                pass.set_pipeline(&self.foliage.pipeline);
                pass.set_bind_group(2, &self.foliage.bg, &[]);
                self.multi_draw(&mut pass, sprite_range.0, sprite_range.1);
            }
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 4);
            }
            // Prints on the ground, multiplied into the land before anything
            // that stands on it is drawn over them.
            self.signs.draw(&mut pass);
            // Sign boards and their letters: solid things in the world, so
            // they go in before the sky and the water, writing depth (the
            // letters' clear pixels discard, so they write none).
            if self.plate_n > 0 {
                pass.set_pipeline(&self.p_plate);
                pass.draw(0..6, 0..self.plate_n);
            }
            // Chips and leaves are solid too, and write depth.
            self.particles.draw_solid(&mut pass);
            pass.set_pipeline(&self.p_sky);
            pass.draw(0..3, 0..1);
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 5);
            }
            if self.foliage.on {
                pass.set_pipeline(&self.foliage.motes);
                pass.set_bind_group(2, &self.foliage.bg, &[]);
                pass.draw(0..6, 0..foliage::MOTES);
            }
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 6);
            }
            pass.set_pipeline(&self.p_water);
            // The open sea: a mesh round the camera that the swell moves, or
            // one quad where there is none (vs_water). With a tide, a second
            // plane at high water for the hollows the open sea cannot reach.
            let sea_verts = if self.swell_on() { 6 * (SEA_GRID - 1) * (SEA_GRID - 1) } else { 6 };
            pass.draw(0..sea_verts, 0..1);
            if self.tide_on() {
                pass.set_pipeline(&self.p_water_basin);
                pass.draw(0..6, 1..2);
            }
            if self.river_n > 0 {
                pass.set_pipeline(&self.p_river);
                pass.draw(0..6 * self.river_n, 0..1);
            }
            if self.fall_n > 0 {
                let a = self.river_n;
                pass.set_pipeline(&self.p_fall);
                pass.draw(6 * a..6 * (a + self.fall_n), 0..1);
            }
            if self.spray_n > 0 {
                // Instance i is spray piece i % SPRAY of waterfall i / SPRAY.
                pass.set_pipeline(&self.p_spray);
                pass.draw(0..6, 0..self.spray_n * SPRAY);
            }
            // Dust, smoke and splashes: blended over the water, far to near.
            self.particles.draw_soft(&mut pass);
            // See-through entities (a pickup turning over its pad): over
            // everything solid, writing no depth, in no particular order.
            if runs.iter().any(|r| r.3) {
                pass.set_pipeline(&self.p_entity_glass);
                for &(m, first, n, _) in runs.iter().filter(|r| r.3) {
                    let r = &self.models[m];
                    pass.draw(r.offset * 6..(r.offset + r.count) * 6, first..first + n);
                }
            }
            if let Some(pr) = prof {
                pr.in_pass(&mut pass, 7);
            }
            if p.highlight.is_some() {
                pass.set_pipeline(&self.p_box);
                pass.draw(0..24, 0..1);
            }
            let n = self.overlay_lines.len() as u32;
            if overlay_tested > 0 {
                pass.set_pipeline(&self.p_overlay);
                pass.draw(0..6, 0..overlay_tested);
            }
            if n > overlay_tested {
                pass.set_pipeline(&self.p_overlay_top);
                pass.draw(0..6, overlay_tested..n);
            }
        }
        if shafts[2] > 0.001 {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rays"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.rays,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.p_rays);
            pass.set_bind_group(0, &self.targets.rays_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.p_post);
            pass.set_bind_group(0, &self.targets.post_bg, &[]);
            pass.draw(0..3, 0..1);
            if n_hud > 0 {
                pass.set_pipeline(&self.p_hud);
                pass.set_bind_group(0, &self.hud_bg, &[]);
                pass.draw(0..6, 0..n_hud);
            }
        }
        // For the next frame's culling.
        self.occlusion.build(&mut enc, view_proj, cam);
        if let Some(pr) = &self.prof {
            pr.at_encoder(&mut enc, 8);
            pr.resolve(&mut enc);
        }
        self.queue.submit(Some(enc.finish()));
        if let Some(pr) = &mut self.prof {
            pr.collect(&self.device);
        }
        self.runs = runs;
        stats.cpu_ms = t0.elapsed().as_secs_f32() * 1000.0;
        stats
    }

    fn multi_draw(&self, pass: &mut wgpu::RenderPass<'_>, start: u32, count: u32) {
        pass.set_index_buffer(self.quad_index.slice(..), wgpu::IndexFormat::Uint32);
        if self.multi_draw {
            pass.multi_draw_indexed_indirect(&self.indirect_buf, start as u64 * 20, count);
        } else {
            for i in 0..count {
                pass.draw_indexed_indirect(&self.indirect_buf, (start + i) as u64 * 20);
            }
        }
    }
}

pub fn frustum_planes(m: Mat4) -> [Vec4; 5] {
    let r0 = m.row(0);
    let r1 = m.row(1);
    let r2 = m.row(2);
    let r3 = m.row(3);
    let n = |p: Vec4| p / p.truncate().length();
    // Left, right, bottom, top, and near (reversed depth puts near at z = w).
    [n(r3 + r0), n(r3 - r0), n(r3 + r1), n(r3 - r1), n(r3 - r2)]
}

/// Does a ball of radius `r` at `c` reach into the frustum?
pub fn sphere_in_frustum(planes: &[Vec4; 5], c: Vec3, r: f32) -> bool {
    planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r)
}

fn aabb_in_frustum(planes: &[Vec4; 5], min: Vec3, max: Vec3) -> bool {
    for p in planes {
        let v = Vec3::new(if p.x >= 0.0 { max.x } else { min.x }, if p.y >= 0.0 { max.y } else { min.y }, if p.z >= 0.0 { max.z } else { min.z });
        if p.truncate().dot(v) + p.w < 0.0 {
            return false;
        }
    }
    true
}

/// Read back a texture (RGBA8) for screenshots.
pub fn read_texture(gpu: &Gpu, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let row = (w * 4).div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(enc.finish()));
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map failed"));
    gpu.device.poll(wgpu::PollType::Wait).expect("poll failed");
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let s = (y * row) as usize;
        out.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    out
}

#[cfg(test)]
mod drift_tests {
    use super::*;

    /// What common.wgsl's `unpack_mul` does, so the two stay a pair.
    fn unpack_mul(v: u32) -> [f32; 3] {
        [v & 255, (v >> 8) & 255, (v >> 16) & 255].map(|b| b as f32 * (2.0 / 255.0))
    }

    #[test]
    fn a_colour_multiply_survives_the_trip_to_the_shader() {
        for c in [[1.0, 1.0, 1.0], [0.0, 0.5, 2.0], [1.137, 1.027, 0.948], [0.86, 1.0, 0.84]] {
            let got = unpack_mul(pack_mul(c));
            for i in 0..3 {
                // Three bytes over a range of 2, so a step of 2/255 and
                // half of that at worst (plus a hair, for the rounding).
                assert!((got[i] - c[i]).abs() <= 1.0 / 255.0 + 1e-6, "{c:?} -> {got:?}");
            }
        }
        // Out of range is clamped rather than wrapped, which would be a
        // wildly wrong colour rather than a slightly wrong one.
        assert_eq!(unpack_mul(pack_mul([-1.0, 9.0, 2.0])), [0.0, 2.0, 2.0]);
    }

    /// The buffer the shaders index: the material table, then a drift row
    /// per material per style.
    #[test]
    fn the_material_buffer_carries_a_drift_row_for_every_material() {
        let rows = material_rows();
        assert_eq!(rows.len(), 768);
        // Nothing is declared in a bare table, so every drift row is off and
        // the shader keeps the ramp each tint class has always had.
        assert!(rows[256..].iter().all(|r| r[2] == 0));
    }
}

#[cfg(test)]
mod shadow_grid_tests {
    use super::*;
    use glam::Vec2;

    /// Texel coordinate of `p` in a cascade's map.
    fn texel(m: Mat4, p: Vec3) -> Vec2 {
        ((m * p.extend(1.0)).truncate().truncate() + Vec2::ONE) * (SHADOW_SIZE as f32 * 0.5)
    }

    /// The texel grid near the player should barely slide as the sun moves
    /// (that slide was the leaf-shadow flicker), and not at all as they walk.
    #[test]
    fn the_shadow_grid_holds_still_near_the_camera() {
        let r = 16.0;
        let cam = Vec3::new(812.0, 41.0, 641.0);
        let leaf = cam + Vec3::new(9.0, 6.0, -7.0);
        // One frame of sun movement at 60 fps on the default clock, at golden hour.
        let h0 = 17.1;
        let dh = crate::world::clock::rate(h0) / 60.0 / 60.0;
        let (s0, s1) = (daylight::sun_dir(h0), daylight::sun_dir(h0 + dh));
        let (m0, raw0, _) = cascade_matrix(s0, cam, r, Vec2::ZERO);
        let phase = shift_phase(Vec2::ZERO, raw0, cascade_matrix(s1, cam, r, Vec2::ZERO).1, cam);
        let (m1, _, _) = cascade_matrix(s1, cam, r, phase);
        // Where the leaf sits within its texel; the true shadow moves a little
        // too, so allow a few hundredths.
        let slide = texel(m1, leaf) - texel(m0, leaf);
        let frac = slide - slide.round();
        assert!(frac.abs().max_element() < 0.05, "grid slid {frac:?} texels in one frame");
        // Snapping to the world origin alone, as before, slid it much further.
        let old0 = cascade_matrix(s0, cam, r, Vec2::ZERO).0;
        let old1 = cascade_matrix(s1, cam, r, Vec2::ZERO).0;
        let old = texel(old1, leaf) - texel(old0, leaf);

        assert!((old - old.round()).abs().max_element() > frac.abs().max_element() * 4.0, "old {old:?} new {frac:?}");

        // Walking with the sun still: every point keeps its place in its texel.
        let walked = cam + Vec3::new(3.3, 0.4, -1.7);
        let p2 = shift_phase(phase, cascade_matrix(s1, cam, r, phase).1, cascade_matrix(s1, walked, r, phase).1, walked);
        let (m2, _, _) = cascade_matrix(s1, walked, r, p2);
        let d = texel(m2, leaf) - texel(m1, leaf);
        assert!((d - d.round()).abs().max_element() < 0.01, "walking moved the grid {d:?}");
    }
}

/// (top, side, bottom) texture layer and shader flags for every material id,
/// from the installed table.
fn material_rows() -> Vec<[u32; 4]> {
    // Rows 0..256 are the material table the shaders index by id. Rows
    // 256..512 and 512..768 hold each material's declared colour field
    // (`drift`) for the calm and classic styles, as the multiply that takes
    // its own texture to each end: x and y the two ends packed as three
    // bytes, z non-zero where one was declared. They ride the same buffer
    // rather than taking a binding of their own.
    let faces = (0..256usize).map(|m| {
        let f = material::faces(m as u8);
        // w: the material's shader flags (matlib::flag, M_* in WGSL).
        [
            f[0],
            f[1],
            f[2],
            crate::world::matlib::flags(m as u8)
                & (0xff
                    | crate::world::matlib::flag::FOLIAGE
                    | crate::world::matlib::flag::GROWS
                    | crate::world::matlib::flag::GLOW_SPECKS
                    | crate::world::matlib::flag::NIGHT_GLOW
                    | crate::world::matlib::flag::FLOODS
                    | 63 << crate::world::matlib::flag::GLOW_SHIFT),
        ]
    });
    let drifts = (0..2usize).flat_map(|style| {
        (0..256usize).map(move |m| match crate::world::matlib::drift(m as u8, style) {
            Some([lo, hi]) => [pack_mul(lo), pack_mul(hi), 1, 0],
            None => [0; 4],
        })
    });
    faces.chain(drifts).collect()
}

/// A colour multiply (0 to 2 a channel) as three bytes, for `material_rows`.
/// The shader unpacks it with the same 2/255 step.
fn pack_mul(c: [f32; 3]) -> u32 {
    let b = |v: f32| ((v * 0.5).clamp(0.0, 1.0) * 255.0).round() as u32;
    b(c[0]) | (b(c[1]) << 8) | (b(c[2]) << 16)
}
