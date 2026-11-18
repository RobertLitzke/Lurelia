//! Lines drawn into the world each frame by whatever needs them: a shooter's
//! tracers, a race's gates. A mode pushes lines during the
//! frame; the renderer draws them after the scene as ribbons that face the
//! camera, either hidden by the land (depth-tested) or on top of everything.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Most lines one frame can draw; the rest are dropped.
pub const MAX_LINES: usize = 4096;

/// One line as the shader reads it: ends (xyz) with the width in metres in
/// `a.w`, and a linear RGBA colour.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Line {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct Overlay {
    // No mode draws lines yet; Game::overlay and Activity::overlay are the hooks.
    depth_tested: Vec<Line>,
    on_top: Vec<Line>,
}

#[allow(dead_code)]
impl Overlay {
    /// A line the land and creatures can hide.
    pub fn line(&mut self, a: Vec3, b: Vec3, width: f32, color: [f32; 4]) {
        self.depth_tested.push(Line {
            a: [a.x, a.y, a.z, width],
            b: [b.x, b.y, b.z, 0.0],
            color,
        });
    }

    /// A line drawn over everything.
    pub fn line_on_top(&mut self, a: Vec3, b: Vec3, width: f32, color: [f32; 4]) {
        self.on_top.push(Line {
            a: [a.x, a.y, a.z, width],
            b: [b.x, b.y, b.z, 0.0],
            color,
        });
    }

    pub fn clear(&mut self) {
        self.depth_tested.clear();
        self.on_top.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.depth_tested.is_empty() && self.on_top.is_empty()
    }

    /// The lines to upload, depth-tested first, and how many of those there are.
    pub(super) fn packed(&self, out: &mut Vec<Line>) -> u32 {
        out.clear();
        out.extend_from_slice(&self.depth_tested);
        out.extend_from_slice(&self.on_top);
        out.truncate(MAX_LINES);
        self.depth_tested.len().min(MAX_LINES) as u32
    }
}
