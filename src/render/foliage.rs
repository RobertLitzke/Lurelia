//! The sprite pass: grass, flowers and leaf clusters (records from
//! mesh/sprites.rs, textures from sprite_tex.rs, shader foliage.wgsl). Drawn in
//! the main pass after the terrain, with depth writes and alpha to coverage
//! under MSAA, so no sorting is needed. Sprites cast no shadows.

use super::{sprite_tex, DEPTH, HDR};

/// Distances (m) over which leaf clusters, grass and fill tufts (the extra
/// grass near the camera) shrink away in foliage.wgsl.
pub const LEAF_FADE: [f32; 2] = [70.0, 92.0];
pub const GRASS_FADE: [f32; 2] = [32.0, 46.0];
/// The far cover (sprites.rs): broad cards of grass that grow in over these
/// two distances, as the tufts shrink away, and then stay. They are drawn at
/// every level of detail, so they carry the ground's cover out to the edge of
/// the world; nothing shrinks them again.
pub const PATCH_FADE: [f32; 2] = [26.0, 40.0];
pub const FILL_FADE: [f32; 2] = [27.0, 42.0];
/// Flower stalks (agave, yucca) stay full size until here, the edge of the
/// full-detail chunks that carry sprites (the default view of 128 m), so they
/// don't grow out of the ground as the player walks up.
pub const STALK_FADE: [f32; 2] = [112.0, 124.0];
/// A chunk's centre is within 7 m of any sprite in it.
const CHUNK_SLACK: f32 = 7.0;
/// Chunks whose centre is further than this (m) draw no sprites: leaf
/// clusters have shrunk to nothing by then.
pub const FAR: f32 = LEAF_FADE[1] + CHUNK_SLACK;
/// Chunks further than this draw no flower stalks.
pub const STALK_FAR: f32 = STALK_FADE[1] + CHUNK_SLACK;
/// Chunks further than this draw no grass.
pub const GRASS_FAR: f32 = GRASS_FADE[1] + CHUNK_SLACK;
/// Chunks further than this draw no fill tufts.
pub const NEAR: f32 = FILL_FADE[1] + CHUNK_SLACK;
/// Chunks nearer than this draw no far cover. There is no far limit: the
/// cover is what the distance is made of, so every loaded column draws it.
pub const PATCH_NEAR: f32 = PATCH_FADE[0] - CHUNK_SLACK;
pub const PATCH_FAR: f32 = 1.0e6;

pub struct Foliage {
    pub pipeline: wgpu::RenderPipeline,
    pub motes: wgpu::RenderPipeline,
    pub bg: wgpu::BindGroup,
    /// Off with the environment variable VOXEL_FOLIAGE=0, for comparisons.
    pub on: bool,
    /// Draw fill tufts and grass only as far as they show (NEAR, GRASS_FAR).
    /// VOXEL_SPRITE_RANGES=0 draws every sprite out to FAR, for comparisons.
    pub ranges: bool,
}

impl Foliage {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene_bgl: &wgpu::BindGroupLayout, shadow_in_bgl: &wgpu::BindGroupLayout, msaa: u32) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sprites"),
            size: wgpu::Extent3d {
                width: sprite_tex::SIZE as u32,
                height: sprite_tex::SIZE as u32,
                depth_or_array_layers: crate::mesh::sprites::layer::COUNT,
            },
            mip_level_count: sprite_tex::MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (m, level) in sprite_tex::build().iter().enumerate() {
            let s = (sprite_tex::SIZE >> m) as u32;
            for (l, data) in level.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
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
        let view = tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprites"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprites"),
            entries: &[
                super::bgl_entry(0, super::tex_ty(wgpu::TextureViewDimension::D2Array, true)),
                super::bgl_entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprites"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&samp),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprites"),
            bind_group_layouts: &[scene_bgl, shadow_in_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let module = super::shader(device, "foliage", super::shaders::foliage());
        let a2c = msaa > 1;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprites"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_sprite"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: msaa,
                mask: !0,
                alpha_to_coverage_enabled: a2c,
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(if a2c { "fs_sprite" } else { "fs_sprite_cut" }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        // Motes: additive points of light, tested against depth but not written.
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let motes = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("motes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_mote"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: msaa,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_mote"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: Some(wgpu::BlendState { color: add, alpha: keep }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let on = std::env::var("VOXEL_FOLIAGE").map(|v| v != "0").unwrap_or(true);
        let ranges = std::env::var("VOXEL_SPRITE_RANGES").map(|v| v != "0").unwrap_or(true);
        Self { pipeline, motes, bg, on, ranges }
    }
}

/// Motes drawn each frame (see vs_mote in foliage.wgsl); most are culled in
/// the vertex shader for being in shade or out in the open.
pub const MOTES: u32 = 2500;

/// Uniform for the sun rays in post.wgsl: the sun's position on screen (uv),
/// the rays' strength (0 when the sun is behind the camera, down, or the
/// rays are resting), and `night` (1 when the light is the moon). Rays come
/// and go over a minute or two, and are stronger with the sun low; through
/// golden hour they never rest.
pub fn sun_shafts(view_proj: glam::Mat4, cam: glam::Vec3, sun: glam::Vec3, intensity: f32, t: f32, underwater: bool, night: f32) -> [f32; 4] {
    if underwater || intensity < 0.05 {
        return [0.0; 4];
    }
    let c = view_proj * (cam + sun * 1000.0).extend(1.0);
    if c.w <= 0.0 {
        return [0.0; 4];
    }
    let (u, v) = (c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
    if !(-0.4..1.4).contains(&u) || !(-0.4..1.4).contains(&v) {
        return [0.0; 4];
    }
    let s = |e0: f32, e1: f32, x: f32| {
        let k = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    };
    let swell = 0.5 + 0.3 * (t * 0.037).sin() + 0.2 * (t * 0.091 + 1.3).sin();
    let low = 1.0 + (1.0 - sun.y).max(0.0) * 0.6;
    let gold = s(0.5, 0.15, sun.y) * (1.0 - night);
    let k = 0.3 * s(0.35, 0.65, swell).max(gold * 0.85) * low * (intensity / 2.3).min(1.0) * (1.0 + gold * 0.6);
    [u, v, k, night]
}
