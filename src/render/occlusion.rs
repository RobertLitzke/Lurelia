//! Occlusion culling. Facing a forest, most chunks in the view are hidden
//! behind the nearest trees, and terrain is bound by vertex work, so drawing
//! them costs about as much as drawing what shows.
//!
//! At the end of each frame `build` turns the depth buffer into a pyramid of
//! farthest depths (shaders/occlusion.wgsl). The next frame, before the main
//! pass, `cull` tests every visible chunk's box against that pyramid, as seen
//! from the camera the pyramid was drawn with, and sets the instance count of
//! the hidden chunks' terrain and sprite draws to zero. Testing from the old
//! camera, a turn of the view can't hide anything wrongly: whatever that frame
//! didn't see counts as visible. Moving reveals a chunk one frame late at
//! worst, a sliver at an edge; a jump (teleport, a new world, a resize) skips
//! culling for a frame.
//!
//! Off with `VOXEL_OCCLUSION=0`.

use glam::{Mat4, Vec3};

/// The chunk index of a draw that is never culled (shadow draws).
pub const NO_CHUNK: u32 = u32::MAX;

/// Further than this between frames and the old pyramid isn't trusted.
const MAX_MOVE: f32 = 2.0;

pub struct Occlusion {
    pub on: bool,
    first: wgpu::ComputePipeline,
    down: wgpu::ComputePipeline,
    cull: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    boxes: wgpu::Buffer,
    owner: wgpu::Buffer,
    /// Pyramid levels, their bind groups, and the cull pass's bind group;
    /// remade with the screen size.
    sized: Option<Sized>,
    /// The camera of the frame the pyramid holds; None until one is built.
    built: Option<(Mat4, Vec3)>,
}

struct Sized {
    size: (u32, u32),
    levels: u32,
    first_bg: wgpu::BindGroup,
    down_bgs: Vec<wgpu::BindGroup>,
    cull_bg: wgpu::BindGroup,
    dims: Vec<(u32, u32)>,
}

fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

impl Occlusion {
    pub fn new(device: &wgpu::Device, msaa: bool, max_chunks: u64, max_draws: u64) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("occlusion"),
            source: wgpu::ShaderSource::Wgsl(super::shaders::occlusion(msaa).into()),
        });
        let pipe = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let buf = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let st = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        Self {
            on: std::env::var("VOXEL_OCCLUSION").map(|v| v != "0").unwrap_or(true),
            first: pipe("cs_first"),
            down: pipe("cs_down"),
            cull: pipe("cs_cull"),
            params: buf("cull params", 96, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
            boxes: buf("cull boxes", max_chunks * 32, st),
            owner: buf("cull owners", max_draws * 4, st),
            sized: None,
            built: None,
        }
    }

    /// Make the pyramid for a `w` x `h` screen reading `depth`, writing culls
    /// into `draws` (the indirect buffer). Call when the targets change.
    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32, depth: &wgpu::TextureView, draws: &wgpu::Buffer) {
        self.built = None;
        let (w0, h0) = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
        let levels = 32 - w0.max(h0).leading_zeros();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("occlusion pyramid"),
            size: wgpu::Extent3d {
                width: w0,
                height: h0,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mip = |l: u32| {
            tex.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: l,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mips: Vec<wgpu::TextureView> = (0..levels).map(mip).collect();
        // Mip sizes halve rounding down; the shader folds an odd last row
        // and column into the texel before them.
        let dims: Vec<(u32, u32)> = (0..levels).map(|l| ((w0 >> l).max(1), (h0 >> l).max(1))).collect();
        let first_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occlusion first"),
            layout: &self.first.get_bind_group_layout(0),
            entries: &[entry(0, wgpu::BindingResource::TextureView(depth)), entry(2, wgpu::BindingResource::TextureView(&mips[0]))],
        });
        let down_bgs = (1..levels as usize)
            .map(|l| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("occlusion down"),
                    layout: &self.down.get_bind_group_layout(0),
                    entries: &[entry(1, wgpu::BindingResource::TextureView(&mips[l - 1])), entry(2, wgpu::BindingResource::TextureView(&mips[l]))],
                })
            })
            .collect();
        let all = tex.create_view(&Default::default());
        let cull_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occlusion cull"),
            layout: &self.cull.get_bind_group_layout(0),
            entries: &[
                entry(3, self.params.as_entire_binding()),
                entry(4, wgpu::BindingResource::TextureView(&all)),
                entry(5, self.boxes.as_entire_binding()),
                entry(6, self.owner.as_entire_binding()),
                entry(7, draws.as_entire_binding()),
            ],
        });
        self.sized = Some(Sized {
            size: (w, h),
            levels,
            first_bg,
            down_bgs,
            cull_bg,
            dims,
        });
    }

    /// Forget the pyramid (a new world, say), so the next frame draws everything.
    pub fn reset(&mut self) {
        self.built = None;
    }

    /// Whether this frame, with the camera at `cam`, can be culled.
    pub fn ready(&self, cam: Vec3) -> bool {
        self.on && self.sized.is_some() && self.built.is_some_and(|(_, c)| (c - cam).length() <= MAX_MOVE)
    }

    /// Turn off the draws among the first `count` whose chunk (`owner`, an
    /// index into `boxes`, two corners each) the last frame's pyramid hides.
    /// Only call when `ready`.
    pub fn cull(&self, queue: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, boxes: &[[f32; 8]], owner: &[u32]) {
        let (Some(s), Some((vp, _))) = (&self.sized, self.built) else {
            return;
        };
        if owner.is_empty() || boxes.is_empty() {
            return;
        }
        let mut p = [0u8; 96];
        p[..64].copy_from_slice(bytemuck::cast_slice(&vp.to_cols_array()));
        p[64..80].copy_from_slice(bytemuck::cast_slice(&[s.size.0 as f32, s.size.1 as f32, s.levels as f32, 0.0]));
        p[80..84].copy_from_slice(&(owner.len() as u32).to_le_bytes());
        queue.write_buffer(&self.params, 0, &p);
        queue.write_buffer(&self.boxes, 0, bytemuck::cast_slice(boxes));
        queue.write_buffer(&self.owner, 0, bytemuck::cast_slice(owner));
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("occlusion cull"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.cull);
        pass.set_bind_group(0, &s.cull_bg, &[]);
        pass.dispatch_workgroups((owner.len() as u32).div_ceil(64), 1, 1);
    }

    /// Build the pyramid from this frame's depth, drawn with `view_proj` from `cam`.
    pub fn build(&mut self, enc: &mut wgpu::CommandEncoder, view_proj: Mat4, cam: Vec3) {
        let Some(s) = &self.sized else { return };
        if !self.on {
            return;
        }
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("occlusion pyramid"),
            timestamp_writes: None,
        });
        let groups = |(w, h): (u32, u32)| (w.div_ceil(8), h.div_ceil(8));
        pass.set_pipeline(&self.first);
        pass.set_bind_group(0, &s.first_bg, &[]);
        let (gx, gy) = groups(s.dims[0]);
        pass.dispatch_workgroups(gx, gy, 1);
        pass.set_pipeline(&self.down);
        for (l, bg) in s.down_bgs.iter().enumerate() {
            pass.set_bind_group(0, bg, &[]);
            let (gx, gy) = groups(s.dims[l + 1]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        drop(pass);
        self.built = Some((view_proj, cam));
    }
}
