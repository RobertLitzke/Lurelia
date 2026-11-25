//! Realistic gun models (see mod.rs): the melee weapons, shovels and knives.
//!
//! Laid along z like a gun: the blade's tip at z near 0, the pommel or
//! the shovel's handle at z near 31. A blade is flat: one voxel across x,
//! its depth in y.

use super::c::*;
use super::Marks;
use crate::models::Grid;


/// Smoothstep from a to b as t goes 0..1.
fn ease(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    a + (b - a) * t * t * (3.0 - 2.0 * t)
}

/// The blade's single plane across x, the voxel column x = 15.
const KX: i32 = 15;

/// Fill a flat blade between `bottom(z)` and `top(z)` (voxel y, inclusive of
/// the row whose centre is inside), its lowest row the sharpened edge.
fn blade(g: &mut Grid, z0: i32, z1: i32, body: u8, edge: u8, top: impl Fn(f32) -> f32, bottom: impl Fn(f32) -> f32) {
    for z in z0..z1 {
        let zc = z as f32 + 0.5;
        let (lo, hi) = (bottom(zc), top(zc));
        if hi <= lo {
            continue;
        }
        let y0 = lo.round() as i32;
        let y1 = (hi.round() as i32).max(y0 + 1);
        for y in y0..y1 {
            g.set(KX, y, z, if y == y0 { edge } else { body });
        }
    }
}



pub fn knife_machete(g: &mut Grid) -> Marks {
    // 60 cm, ~2 cm voxels: a long thin blade widening toward a clipped,
    // angled tip, black-coated with a bright edge, no guard, a polymer
    // handle with a lanyard hole.
    let spine = |_z: f32| 18.0;
    let bottom = |z: f32| {
        if z < 4.0 {
            // The tip: the edge sweeps up to a point at the spine.
            ease(17.2, 14.0, (z - 0.5) / 3.5)
        } else {
            // 4 voxels (8 cm) deep near the tip, 2.5 (5 cm) at the handle.
            ease(14.0, 15.5, (z - 4.0) / 18.0)
        }
    };
    blade(g, 0, 23, DARK, EDGE, spine, bottom);
    // A ground bevel line above the edge, lighter.
    for z in 3..22 {
        let y = bottom(z as f32 + 0.5).round() as i32 + 1;
        if y < 17 {
            g.set(KX, y, z, BARREL);
        }
    }
    // The tang's bolster where the blade meets the handle.
    g.fill([15, 15, 23], [17, 19, 24], DARK);
    // Handle: black polymer, a hook at the butt, finger swells below.
    for z in 24..31 {
        let zc = z as f32;
        let swell = if z == 25 || z == 27 { 0.2 } else { 0.0 };
        g.rod([16.0, 16.8, zc], [16.0, 16.8, zc + 1.0], 1.05 + swell, GRIP);
    }
    // The hook at the butt, pointing down.
    g.fill([15, 14, 30], [17, 16, 31], GRIP);
    Marks { grip: [16.0, 16.8, 27.0], muzzle: [15.5, 17.5, 0.0], butt: [16.0, 16.8, 31.0], fore: None }
}

/// A shovel's blade standing on edge in the column x = `x`: for each z the
/// rows whose centres lie within `half(z)` of `mid(z)`. The front voxel of
/// every row is the worn, bright cutting edge. With `dish` the outer rows
/// also turn one voxel toward +x, the blade's hollow.
#[allow(clippy::too_many_arguments)]
fn plate(g: &mut Grid, x: i32, z0: i32, z1: i32, body: u8, edge: u8, dish: bool, mid: impl Fn(f32) -> f32, half: impl Fn(f32) -> f32) {
    let mut seen = [false; 32];
    for z in z0..z1 {
        let zc = z as f32 + 0.5;
        let (m, h) = (mid(zc), half(zc));
        if h <= 0.0 {
            continue;
        }
        let y0 = (m - h - 0.5).ceil() as i32;
        let y1 = (m + h - 0.5).floor() as i32;
        for y in y0.max(0)..=y1.min(31) {
            let c = if seen[y as usize] { body } else { edge };
            seen[y as usize] = true;
            g.set(x, y, z, c);
            if dish && (y == y0 || y == y1) && y1 - y0 >= 3 {
                g.set(x + 1, y, z, c);
            }
        }
    }
}

/// The shovels' haft runs down the blade's column.
const HX: f32 = 15.5;
const HY: f32 = 15.5;


pub fn shovel_spade(g: &mut Grid) -> Marks {
    // 100 cm, ~3.3 cm voxels: an olive-drab army spade, a short blade
    // 16 cm wide with a pointed tip and square shoulders, a rolled back
    // edge, straps riveted up the haft, a short ash haft and a pressed
    // steel D-handle with a wooden crossbar.
    let half = |z: f32| if z < 3.5 { 0.3 + 2.1 * (z / 3.5) } else { 2.4 };
    plate(g, 15, 0, 8, OLIVE, EDGE, false, |_| HY, half);
    // The rolled back edge across the shoulders.
    g.fill([14, 13, 8], [17, 18, 9], OLIVE);
    // Straps up the haft from the blade, riveted.
    g.rod([HX, HY, 8.0], [HX, HY, 12.0], 1.05, OLIVE);
    g.set(14, 15, 9, STEEL);
    g.set(14, 15, 11, STEEL);
    g.set(16, 15, 10, STEEL);
    // The haft.
    g.rod([HX, HY, 12.0], [HX, HY, 24.5], 0.6, ASH);
    // The D-handle: a steel ferrule, two steel arms splaying out to a
    // wooden crossbar 13 cm wide.
    g.rod([HX, HY, 23.5], [HX, HY, 25.5], 0.85, STEEL);
    for s in [-1.0, 1.0] {
        let y = HY + 2.1 * s;
        g.rod([HX, HY, 25.0], [HX, y, 28.0], 0.55, STEEL);
        g.rod([HX, y, 28.0], [HX, y, 30.8], 0.55, STEEL);
    }
    g.rod([HX, HY - 2.1, 30.3], [HX, HY + 2.1, 30.3], 0.6, ASH);
    Marks { grip: [HX, HY, 30.3], muzzle: [HX, HY, 0.5], butt: [HX, HY, 31.0], fore: Some([HX, HY, 16.5]) }
}

