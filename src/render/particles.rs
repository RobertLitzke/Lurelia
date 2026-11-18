//! The particle draws (pool in src/fx, shader particles.wgsl). One storage
//! buffer rewritten each frame, the way plates.rs does it, and two instanced
//! draws over it: the solid particles, which write depth and go in with the
//! sign boards, and the soft ones, which blend and go in after the water.

use super::{PipeDesc, HDR};
use bytemuck::{Pod, Zeroable};

/// One particle as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GpuParticle {
    /// Position xyz; w the radius in metres.
    pub pos: [f32; 4],
    /// Velocity xyz; w its spin in radians.
    pub vel: [f32; 4],
    /// Linear RGB (a multiply on the material's colour where it has one); a
    /// is its opacity.
    pub color: [f32; 4],
    /// Shape, material (0 for none), seed, 1 where it gives its own light.
    pub info: [u32; 4],
}

pub struct Particles {
    solid: wgpu::RenderPipeline,
    soft: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    /// This frame's packed particles, kept to save the allocation.
    packed: Vec<GpuParticle>,
    solid_n: u32,
    n: u32,
}

impl Particles {
    pub fn new(device: &wgpu::Device, scene_bgl: &wgpu::BindGroupLayout, shadow_in_bgl: &wgpu::BindGroupLayout, msaa: u32) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles"),
            size: (crate::fx::MAX_PARTICLES * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
            entries: &[super::bgl_entry(
                0,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[scene_bgl, shadow_in_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let module = super::shader(device, "particles", super::shaders::particles());
        let ge = wgpu::CompareFunction::GreaterEqual;
        let nobias = wgpu::DepthBiasState::default();
        let make = |label, fs, write, blend| {
            super::pipeline(
                device,
                PipeDesc {
                    label,
                    layout: &layout,
                    module: &module,
                    vs: "vs_particle",
                    fs: Some(fs),
                    color: Some((HDR, blend)),
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth: Some((write, ge, nobias)),
                    samples: msaa,
                },
            )
        };
        let solid = make("particles solid", "fs_particle_solid", true, None);
        let soft = make("particles soft", "fs_particle_soft", false, Some(wgpu::BlendState::ALPHA_BLENDING));
        Particles {
            solid,
            soft,
            buf,
            bg,
            packed: Vec::new(),
            solid_n: 0,
            n: 0,
        }
    }

    /// Pack the pool and send it.
    pub fn upload(&mut self, queue: &wgpu::Queue, fx: &crate::fx::Fx, eye: glam::Vec3) {
        self.solid_n = fx.pack(eye, crate::game::STEP, &mut self.packed);
        self.n = self.packed.len() as u32;
        if self.n > 0 {
            queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&self.packed));
        }
    }

    /// Chips and leaves: call where solid things are drawn.
    pub fn draw_solid(&self, pass: &mut wgpu::RenderPass) {
        if self.solid_n > 0 {
            pass.set_pipeline(&self.solid);
            pass.set_bind_group(2, &self.bg, &[]);
            pass.draw(0..6, 0..self.solid_n);
        }
    }

    /// Puffs and streaks: call after the water.
    pub fn draw_soft(&self, pass: &mut wgpu::RenderPass) {
        if self.n > self.solid_n {
            pass.set_pipeline(&self.soft);
            pass.set_bind_group(2, &self.bg, &[]);
            pass.draw(0..6, self.solid_n..self.n);
        }
    }
}
