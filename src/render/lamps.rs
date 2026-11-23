//! Lamps: small point lights that let a roofed room be seen.
//!
//! All the light in this engine comes from the sky -- the sun, the hemisphere
//! fill, and a baked sky-openness map that darkens anything standing under
//! something else (`sky_openness` in the shaders). That is right for a world
//! you walk across and useless the moment a world puts a roof on, because a
//! roofed room is then pitch black. A lamp is the other half: a position, a
//! reach and a colour, shaded with distance falloff and no shadow of its own.
//!
//! A world places them with the `lamp` chronicle event; `Game::prepare_frame`
//! sends the nearest `MAX` to the camera and the scene shader loops over
//! them. There is no clustering: an arena has a few dozen, and the loop is
//! cheap next to the shadow lookups already in `light()`.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// The most lamps one frame may light with. The rest, which are further
/// away, are dropped.
///
/// It is set high enough that a whole arena is lit at once: an imported
/// map hangs 150-odd lamps and dropping to the nearest few dozen made a room
/// brighten as the player walked into it, which is the one thing a light
/// must never do. The loop in `lamp_light` leaves early for any lamp that
/// cannot reach the pixel, so the ones out of the room cost a length and a
/// compare.
pub const MAX: usize = 256;

/// One lamp as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Lamp {
    /// Position, with the reach in metres in `w`.
    pub pos: [f32; 4],
    /// Linear colour, with the strength in `w`.
    pub color: [f32; 4],
    /// A spot's direction (unit), with the cosine of its cone's half-angle
    /// in `w`; `w` of `POINT` is a lamp that lights all round.
    pub dir: [f32; 4],
}

/// `Lamp::dir`'s `w` for a lamp that lights all round (no cosine is this low).
pub const POINT: f32 = -2.0;

/// The lamps lighting this frame, nearest first.
#[derive(Default)]
pub struct Lamps {
    list: Vec<Lamp>,
}

impl Lamps {
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Add a lamp if it can reach `eye`'s neighbourhood. `strength` is how
    /// bright it is at the lamp, before falloff.
    pub fn push(&mut self, pos: Vec3, reach: f32, color: [f32; 3], strength: f32) {
        self.list.push(Lamp {
            pos: [pos.x, pos.y, pos.z, reach],
            color: [color[0], color[1], color[2], strength],
            dir: [0.0, 0.0, 0.0, POINT],
        });
    }

    /// A spot: a lamp that lights only within `half` radians of `dir`, with
    /// a soft edge (a kart's headlights).
    pub fn push_spot(&mut self, pos: Vec3, dir: Vec3, half: f32, reach: f32, color: [f32; 3], strength: f32) {
        let d = dir.normalize_or(Vec3::NEG_Z);
        self.list.push(Lamp {
            pos: [pos.x, pos.y, pos.z, reach],
            color: [color[0], color[1], color[2], strength],
            dir: [d.x, d.y, d.z, half.cos()],
        });
    }

    /// Keep the `MAX` nearest to `eye` and put them in order, so dropping the
    /// rest drops the ones that matter least.
    pub fn sort_to(&mut self, eye: Vec3) {
        if self.list.len() > MAX {
            self.list.sort_by(|a, b| {
                let d = |l: &Lamp| (Vec3::new(l.pos[0], l.pos[1], l.pos[2]) - eye).length_squared();
                d(a).total_cmp(&d(b))
            });
            self.list.truncate(MAX);
        }
    }

    pub fn packed(&self) -> &[Lamp] {
        &self.list
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
}
