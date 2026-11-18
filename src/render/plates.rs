//! Flat plates drawn into the world each frame: the boards of signs and the
//! letters on them. A plate is a rectangle at any angle, either a flat colour
//! (a board, its frame, a post cap) or one glyph of the HUD's font atlas, so a
//! sign's lettering is as sharp as the type in the menus however close you
//! stand. Nothing here is in the voxels: the game pushes the plates of the
//! signs near the camera every frame and the renderer draws them after the
//! land, lit and fogged like everything else.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Most plates one frame can draw; the rest are dropped.
pub const MAX_PLATES: usize = 8192;

/// `pos.w`: a flat colour.
pub const FLAT: f32 = 0.0;
/// `pos.w`: one glyph from the font atlas, its cell in `cell`.
pub const GLYPH: f32 = 1.0;

/// One rectangle as the shader reads it: its centre, the half-extent along the
/// plate's width and height (so the two vectors give its facing), a linear
/// RGBA colour, and the atlas cell for a glyph.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct Plate {
    /// Centre xyz; w is FLAT or GLYPH.
    pub pos: [f32; 4],
    /// Half the plate's width, along its own u axis.
    pub right: [f32; 4],
    /// Half the plate's height, along its own v axis.
    pub up: [f32; 4],
    pub color: [f32; 4],
    /// Atlas cell x, y, width, height in atlas pixels.
    pub cell: [u32; 4],
}

#[derive(Default)]
pub struct Plates {
    items: Vec<Plate>,
}

impl Plates {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// A rectangle of flat colour centred on `at`, `right` and `up` being half
    /// its width and height as vectors.
    pub fn flat(&mut self, at: Vec3, right: Vec3, up: Vec3, color: [f32; 4]) {
        self.push(at, right, up, color, FLAT, [0; 4]);
    }

    /// One glyph: the atlas cell is x, y, width, height in atlas pixels.
    pub fn glyph(&mut self, at: Vec3, right: Vec3, up: Vec3, color: [f32; 4], cell: [u32; 4]) {
        self.push(at, right, up, color, GLYPH, cell);
    }

    fn push(&mut self, at: Vec3, right: Vec3, up: Vec3, color: [f32; 4], kind: f32, cell: [u32; 4]) {
        if self.items.len() >= MAX_PLATES {
            return;
        }
        self.items.push(Plate {
            pos: [at.x, at.y, at.z, kind],
            right: [right.x, right.y, right.z, 0.0],
            up: [up.x, up.y, up.z, 0.0],
            color,
            cell,
        });
    }

    pub(crate) fn packed(&self) -> &[Plate] {
        &self.items
    }
}
