//! `--gun-sheet`: every weapon in the arsenal, each from several angles, in
//! one headless screenshot.
//!
//! A view model is only ever seen from one place -- down and to the right of
//! the camera, held at the grip, pointing away -- so the only part of it
//! anybody has ever looked at is its left flank and its muzzle. The sheet
//! stands all ten of them up side by side and turns each one through half a
//! revolution, which is how a gun whose far side is a slab of nothing, or
//! whose breech is a hole, shows itself.
//!
//! One row per weapon in slot order, one column per angle, broadside first,
//! each row named down the left. The camera backs up against a wall and the
//! sheet hangs a little in front of it, so the backdrop is one flat surface:
//! a sheet with a floor and a ceiling in it is a photograph of a room that
//! happens to have guns in it.
//!
//! It wants a frame wider than it is tall. The grid is about square, and the
//! slack either side is where the names go.

use super::viewmodel::{self, SHAPES};
use crate::entities::{Entities, Look};
use crate::library::Library;
use crate::render::hud::Hud;
use crate::world::material::AIR;
use crate::world::store::Store;
use glam::Vec3;

/// How far each column is turned from broadside, degrees. Broadside first
/// because it is the one that reads, then round the back; 90 is down the
/// barrel, which is a dot on purpose -- a gun that is all silhouette from
/// the front is a gun nobody will recognise coming at them.
pub const ANGLES: [f32; 6] = [0.0, 35.0, 70.0, 105.0, 140.0, 180.0];

/// Metres of air between the back of the sheet and the wall behind it.
const OFF_THE_WALL: f32 = 1.1;

/// How high up its own grid a gun is built, in model voxels. A model's
/// pivot is its bottom centre, so a gun spawned on a point is drawn above
/// it, and a grid of them sits higher than the grid of points it was laid
/// out on -- which puts every name beside the gap under its row. The
/// weapons are all built about here; it is the height `viewmodel::muzzle`
/// returns for every one of them.
const BUILT_AT: f32 = 13.0;

/// What the sheet was laid out as, so the names can be put beside it. The
/// grid is a flat plane square to the lens at a known distance, which
/// projects evenly: one number turns metres across it into pixels, and
/// nothing has to be put through a matrix.
pub struct Sheet {
    /// The name beside each row, top to bottom.
    pub rows: Vec<String>,
    d: f32,
    across: f32,
    down: f32,
    cols: usize,
}

/// How far in front of the camera the sheet hangs: far enough back that the
/// whole grid is in frame, with the width and the height each asking for a
/// distance and the larger one winning.
fn depth() -> f32 {
    let span = 32.0 * viewmodel::VOXEL;
    (span * 1.25 * ANGLES.len() as f32 * 0.62).max(span * 0.95 * SHAPES.len() as f32 * 0.72) + 0.7
}

/// Where to stand to shoot it: raised until the sheet is not half
/// underground, then backed up until the wall ahead is just behind where the
/// sheet will hang. A world with no wall in front of the camera, and a room
/// too low to rise in, each leave it where it was -- the sheet still works,
/// it is just photographed in a room.
pub fn place(store: &Store, eye: Vec3, yaw: f32) -> Vec3 {
    let d = depth();
    let lift = d * 0.5;
    let head = store.ray(eye, Vec3::Y, lift + 1.0, |m| m != AIR).map_or(lift, |h| (h.t - 1.0).max(0.0));
    let eye = eye + Vec3::Y * head;
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let want = d + OFF_THE_WALL;
    match store.ray(eye, fwd, 400.0, |m| m != AIR) {
        Some(h) if h.t > want => eye + fwd * (h.t - want),
        _ => eye,
    }
}

/// Stand the sheet up in front of a camera at `eye` looking along `yaw`.
/// `base` is where the weapon models start in the model list
/// (`Game::weapon_models`), and `names` is what to call the rows.
/// What to call each row: the weapon wearing that shape, or the shape
/// itself where no weapon in this library wears it -- which is also how an
/// unused shape announces itself. Taken before the sheet is built, because
/// the library and the entities are the same `Game`.
pub fn names(lib: Option<&Library>) -> Vec<String> {
    SHAPES
        .iter()
        .map(|shape| {
            lib.and_then(|l| l.weapons.iter().find(|w| w.view == **shape))
                .map_or_else(|| shape.to_uppercase(), |w| w.display.to_uppercase())
        })
        .collect()
}

pub fn spawn(ents: &mut Entities, names: Vec<String>, base: usize, eye: Vec3, yaw: f32) -> Sheet {
    let span = 32.0 * viewmodel::VOXEL;
    let (across, down) = (span * 1.25, span * 0.95);
    let (rows, cols) = (SHAPES.len(), ANGLES.len());
    let d = depth();
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    // Broadside: a gun seen down its own barrel is a dot, so that is the
    // angle the sheet starts from rather than the one it shows first.
    let broadside = yaw + std::f32::consts::FRAC_PI_2;
    for r in 0..rows {
        for (c, turn) in ANGLES.iter().enumerate() {
            let at = eye + fwd * d + right * ((c as f32 - (cols as f32 - 1.0) * 0.5) * across) + Vec3::Y * (((rows as f32 - 1.0) * 0.5 - r as f32) * down - BUILT_AT * viewmodel::VOXEL);
            ents.world.spawn((Look::new(base + r, at, broadside + turn.to_radians()),));
            // The minigun's barrels are a model of their own on an axis of
            // their own, and a minigun drawn without them is a box with a
            // hole in it.
            if r == viewmodel::MINIGUN {
                // The barrels carry a pivot of their own, up on their axis
                // so that they can spin without swinging the gun round with
                // them, so they have to be lifted by it to sit in the body
                // they turn inside.
                let axis = Vec3::Y * viewmodel::BARREL_AXIS * viewmodel::VOXEL;
                ents.world.spawn((Look::new(base + viewmodel::BARRELS, at + axis, broadside + turn.to_radians()),));
            }
        }
    }
    Sheet { rows: names, d, across, down, cols }
}

impl Sheet {
    /// The names, down the left, and the angles across the top.
    pub fn label(&self, hud: &mut Hud, w: f32, h: f32, fov: f32) {
        let k = h * 0.5 / ((fov * 0.5).tan() * self.d);
        let (cx, cy) = (w * 0.5, h * 0.5);
        // `Hud::text` takes a scale, not a pixel height: the old font's
        // pixels per pixel, about thirteen screen pixels tall each.
        let size = (h / 640.0).max(1.1);
        let ink = [0.96, 0.97, 0.95, 0.95];
        let faint = [0.96, 0.97, 0.95, 0.5];
        let rows = self.rows.len() as f32;
        let left = cx - ((self.cols as f32 - 1.0) * 0.5 * self.across + self.across * 0.6) * k;
        for (r, name) in self.rows.iter().enumerate() {
            let y = cy - ((rows - 1.0) * 0.5 - r as f32) * self.down * k;
            hud.label(left - Hud::text_width(name, size), y - size * 6.5, size, ink, name);
        }
        let top = cy - ((rows - 1.0) * 0.5 * self.down + self.down * 0.8) * k;
        for (c, turn) in ANGLES.iter().enumerate() {
            let x = cx + (c as f32 - (self.cols as f32 - 1.0) * 0.5) * self.across * k;
            let text = format!("{turn:.0}");
            hud.label(x - Hud::text_width(&text, size) * 0.5, top, size, faint, &text);
        }
    }

    /// What it laid out, for the console.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "gun sheet: {} weapons down, {} angles across ({})",
            self.rows.len(),
            self.cols,
            ANGLES.iter().map(|a| format!("{a:.0}")).collect::<Vec<_>>().join(", ")
        )];
        for (r, name) in self.rows.iter().enumerate() {
            out.push(format!("  row {}: {name}", r + 1));
        }
        out
    }
}
