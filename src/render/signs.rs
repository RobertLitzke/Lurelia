//! The prints on the ground (creatures/sign.rs), drawn as flat quads that
//! darken the ground they lie on (shader signs.wgsl). One storage buffer of
//! the prints near the camera, rewritten when the marks change or the camera
//! has moved on, and one instanced draw that multiplies into the frame
//! after the land is down, so a print takes the colour of whatever it is
//! pressed into, lit as that ground is lit. Its groups are laid out like the
//! particles' (scene, shadow, its own), so it rebinds only group 2.

use super::{PipeDesc, HDR};
use crate::creatures::sign::{Bed, Kind, Marks};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// One print as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GpuSign {
    /// Position xyz (on the drawn ground); w its length in metres.
    pub a: [f32; 4],
    /// Heading (radians, creature convention), wear 0 to 1, foot shape
    /// (sign.rs `Foot::index`), bed (0 snow, 1 mud, 2 sand, 3 earth).
    pub b: [f32; 4],
}

/// At most this many prints are drawn: the nearest, within `DRAW_M`.
pub const MAX_DRAWN: usize = 2048;
/// Prints further than this are too small to see.
pub const DRAW_M: f32 = 45.0;

pub struct Signs {
    pipe: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    packed: Vec<GpuSign>,
    n: u32,
    /// What was sent last: the marks' version and where the camera stood.
    sent: Option<(u64, Vec3)>,
}

fn bed_index(b: Bed) -> f32 {
    match b {
        Bed::Snow => 0.0,
        Bed::Mud => 1.0,
        Bed::Sand => 2.0,
        Bed::Earth => 3.0,
    }
}

impl Signs {
    pub fn new(device: &wgpu::Device, scene_bgl: &wgpu::BindGroupLayout, shadow_in_bgl: &wgpu::BindGroupLayout, msaa: u32) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("signs"),
            size: (MAX_DRAWN * std::mem::size_of::<GpuSign>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("signs"),
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
            label: Some("signs"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("signs"),
            bind_group_layouts: &[scene_bgl, shadow_in_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let module = super::shader(device, "signs", super::shaders::signs());
        // Multiply: what the frame already holds times what the print says.
        let multiply = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipe = super::pipeline(
            device,
            PipeDesc {
                label: "signs",
                layout: &layout,
                module: &module,
                vs: "vs_sign",
                fs: Some("fs_sign"),
                color: Some((HDR, Some(multiply))),
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull: None,
                depth: Some((false, wgpu::CompareFunction::GreaterEqual, wgpu::DepthBiasState::default())),
                samples: msaa,
            },
        );
        Signs {
            pipe,
            buf,
            bg,
            packed: Vec::new(),
            n: 0,
            sent: None,
        }
    }

    /// Send the prints near `eye`, when the marks have changed or the camera
    /// has moved a few metres since they were last sent.
    pub fn upload(&mut self, queue: &wgpu::Queue, marks: &Marks, eye: Vec3) {
        if let Some((v, at)) = self.sent {
            if v == marks.version && (at - eye).length_squared() < 16.0 {
                return;
            }
        }
        self.sent = Some((marks.version, eye));
        let reach = DRAW_M + 4.0;
        let mut near: Vec<(f32, GpuSign)> = marks
            .signs
            .iter()
            .filter_map(|s| {
                let Kind::Print(foot) = s.kind else {
                    return None;
                };
                let d = (s.pos - eye).length_squared();
                (d < reach * reach).then(|| {
                    (
                        d,
                        GpuSign {
                            a: [s.pos.x, s.pos.y, s.pos.z, s.size],
                            b: [s.yaw, s.wear, foot.index() as f32, bed_index(s.bed)],
                        },
                    )
                })
            })
            .collect();
        if near.len() > MAX_DRAWN {
            near.select_nth_unstable_by(MAX_DRAWN, |a, b| a.0.total_cmp(&b.0));
            near.truncate(MAX_DRAWN);
        }
        self.packed.clear();
        self.packed.extend(near.into_iter().map(|(_, g)| g));
        self.n = self.packed.len() as u32;
        if self.n > 0 {
            queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&self.packed));
        }
    }

    /// Call once the land is down and before anything stands on it.
    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.n > 0 {
            pass.set_pipeline(&self.pipe);
            pass.set_bind_group(2, &self.bg, &[]);
            pass.draw(0..6, 0..self.n);
        }
    }
}
