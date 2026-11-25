//! Realistic gun models (see mod.rs): the long guns, an anti-materiel
//! rifle, an assault rifle and a bolt-action rifle, three of each.
//!
//! These are drawn at 2.5 to 4.7 cm a voxel, so a barrel is a rod well under
//! a voxel thick and a receiver is two voxels wide (x 15..17, about the
//! centre line x = 16). Rods run along x = 16 and at y = k + 0.5 so that they
//! line up with the two-voxel bodies.

use super::c::*;
use super::Marks;
use crate::models::Grid;

/// A round part along the centre line, from (y0, z0) to (y1, z1).
fn bar(g: &mut Grid, y0: f32, z0: f32, y1: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y0, z0], [16.0, y1, z1], r, c);
}

/// A round part straight along z at height y.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    bar(g, y, z0, y, z1, r, c);
}

/// A two-voxel-wide body on the centre line, y0..y1 by z0..z1 (half-open).
fn slab(g: &mut Grid, y0: i32, y1: i32, z0: i32, z1: i32, c: u8) {
    g.fill([15, y0, z0], [17, y1, z1], c);
}

/// A body whose top and bottom vary along z: a stock, a sloped fore-end.
/// `bot` and `top` give the edges at each slice's middle.
fn profile(g: &mut Grid, z0: i32, z1: i32, bot: impl Fn(f32) -> f32, top: impl Fn(f32) -> f32, c: u8) {
    for z in z0..z1 {
        let m = z as f32 + 0.5;
        slab(g, bot(m).round() as i32, top(m).round() as i32, z, z + 1, c);
    }
}

/// Straight line between two (z, y) points, as a function of z.
fn lerp(z0: f32, y0: f32, z1: f32, y1: f32) -> impl Fn(f32) -> f32 {
    move |z| y0 + (y1 - y0) * ((z - z0) / (z1 - z0)).clamp(0.0, 1.0)
}

/// A muzzle brake: fat collars with narrower gaps between, the gaps reading
/// as the ports cut across it.
fn brake(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, ports: usize, c: u8) {
    let n = ports * 2 + 1;
    let step = (z1 - z0) / n as f32;
    for i in 0..n {
        let (a, b) = (z0 + step * i as f32, z0 + step * (i + 1) as f32);
        if i % 2 == 0 {
            tube(g, y, a, b, r, c);
        } else {
            tube(g, y, a, b, r * 0.55, BLACK);
        }
    }
}

/// A rifle scope on the centre line: objective bell at the front, a tube,
/// an eyepiece, an elevation turret on top and a windage turret to the right.
fn scope(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, bell: f32, c: u8) {
    let len = z1 - z0;
    tube(g, y, z0, z0 + len * 0.16, bell, c);
    bar(g, y, z0 + len * 0.16, y, z0 + len * 0.26, (r + bell) * 0.5, c);
    tube(g, y, z0 + len * 0.26, z1 - len * 0.16, r, c);
    tube(g, y, z1 - len * 0.16, z1, r * 1.3, c);
    tube(g, y, z0 - 0.15, z0, bell * 0.8, GLASS);
    let mid = z0 + len * 0.5;
    bar(g, y, mid, y + r + 1.0, mid, r * 0.8, c);
    g.rod([16.0, y, mid], [16.0 + r + 1.0, y, mid], r * 0.7, c);
}

/// Scope rings: a block from the rail up to the scope's tube.
fn ring(g: &mut Grid, y0: i32, y1: i32, z: i32, c: u8) {
    slab(g, y0, y1, z, z + 1, c);
}

/// A bolt handle out to the right with a ball on the end.
fn bolt(g: &mut Grid, y: f32, z: f32, len: f32, knob: f32) {
    g.rod([16.5, y, z], [16.5 + len, y - len * 0.5, z + 0.4], 0.3, STEEL);
    g.ellipsoid([16.7 + len, y - len * 0.5 - 0.1, z + 0.45], [knob, knob, knob], BLACK);
}

/// Bipod legs splayed out from (y, z) to feet at (yf, zf).
fn bipod(g: &mut Grid, y: f32, z: f32, yf: f32, zf: f32, splay: f32, c: u8) {
    for s in [-1.0f32, 1.0] {
        g.rod([16.0 + s * 0.6, y, z], [16.0 + s * splay, yf, zf], 0.35, c);
        g.rod([16.0 + s * splay, yf + 0.2, zf], [16.0 + s * splay, yf - 0.3, zf - 0.3], 0.5, BLACK);
    }
    slab(g, y.floor() as i32, y.floor() as i32 + 1, z.floor() as i32 - 1, z.floor() as i32 + 1, c);
}

/// A trigger guard: a bar under the receiver from z0 to z1 at height y.
fn guard(g: &mut Grid, y: f32, z0: f32, z1: f32, top: f32, c: u8) {
    bar(g, top, z0, y, z0 + 0.4, 0.3, c);
    tube(g, y, z0 + 0.4, z1, 0.3, c);
    g.rod([16.0, 0.5 + y.floor() + 1.0, z0 + (z1 - z0) * 0.55], [16.0, y + 0.3, z0 + (z1 - z0) * 0.65], 0.25, STEEL);
}

// ---- anti-materiel rifles ----

/// Long-barrel semi-auto on a bipod: a big slotted brake, a long bare barrel,
/// a tall slab receiver with cooling holes at the front, a rail and scope, a
/// straight box magazine, and the stock run straight back from the receiver
/// to a thick pad with a monopod under it. About 4.7 cm a voxel.
pub fn amr_long(g: &mut Grid) -> Marks {
    let y = 16.5;
    brake(g, y, 0.0, 3.6, 1.1, 2, DARK);
    tube(g, y, 3.6, 12.0, 0.45, BARREL);
    // Upper receiver: the barrel runs into it; the front is a shroud with
    // round holes through it.
    slab(g, 15, 19, 11, 27, DARK);
    for z in [12, 14] {
        g.fill([15, 17, z], [17, 18, z + 1], BLACK);
    }
    // Lower receiver, set back under the upper.
    slab(g, 13, 15, 17, 25, BARREL);
    // Rail along the top, with notches.
    slab(g, 19, 20, 12, 26, BLACK);
    // Magazine: straight, ahead of the grip.
    slab(g, 10, 13, 18, 21, BLACK);
    slab(g, 10, 11, 18, 21, DARK);
    guard(g, 12.2, 21.0, 22.8, 13.0, BLACK);
    // Pistol grip.
    bar(g, 13.5, 22.6, 9.8, 23.8, 0.8, GRIP);
    // Stock: the receiver's line carried back, with a cheek piece, a
    // thick pad and a monopod.
    slab(g, 14, 18, 27, 30, DARK);
    slab(g, 18, 19, 26, 29, BLACK);
    slab(g, 12, 20, 30, 31, BLACK);
    bar(g, 13.5, 28.5, 10.0, 28.5, 0.35, DARK);
    tube(g, 9.9, 28.0, 29.0, 0.45, BLACK);
    bipod(g, 14.5, 12.5, 9.0, 8.0, 2.4, DARK);
    // Scope on two rings.
    ring(g, 20, 21, 15, BLACK);
    ring(g, 20, 21, 21, BLACK);
    scope(g, 21.5, 13.0, 24.0, 0.55, 0.9, BLACK);
    Marks { grip: [16.0, 9.8, 23.8], muzzle: [16.0, y, 0.0], butt: [16.0, 16.0, 31.0], fore: Some([16.0, 14.5, 14.0]) }
}



// ---- assault rifles ----



/// Bullpup with an optic: a short barrel, an olive body with a vertical
/// fore grip joined to the pistol grip by one long guard, the magazine
/// behind the grip, and an integral optic on top. About 2.5 cm a voxel.
pub fn ar_bullpup(g: &mut Grid) -> Marks {
    let y = 16.5;
    brake(g, y, 0.0, 1.6, 0.6, 1, BLACK);
    tube(g, y, 1.6, 7.0, 0.45, BARREL);
    // Barrel shroud and the body, rounding up at the nose.
    tube(g, y, 6.0, 11.0, 1.4, OLIVE);
    profile(g, 10, 31, lerp(10.0, 15.0, 13.0, 12.0), lerp(10.0, 18.0, 17.0, 18.0), OLIVE);
    slab(g, 18, 19, 17, 28, OLIVE);
    slab(g, 11, 19, 30, 31, BLACK);
    // Ejection port and cocking slot.
    g.fill([16, 15, 20], [17, 17, 24], BLACK);
    g.fill([16, 17, 9], [17, 18, 17], BLACK);
    // Vertical fore grip, pistol grip, and the guard joining them.
    bar(g, 15.0, 8.6, 10.0, 8.2, 0.85, OLIVE);
    bar(g, 12.5, 14.0, 8.5, 15.0, 0.9, OLIVE);
    bar(g, 10.0, 8.4, 8.9, 14.6, 0.6, OLIVE);
    // Magazine behind the grip, curving a little.
    slab(g, 9, 12, 18, 21, BLACK);
    slab(g, 7, 9, 17, 20, BLACK);
    // Integral optic: a mount and a tube with glass at each end.
    slab(g, 18, 20, 10, 17, OLIVE);
    tube(g, 21.0, 9.5, 17.5, 0.95, OLIVE);
    tube(g, 21.0, 9.2, 9.5, 0.8, GLASS);
    tube(g, 21.0, 17.5, 17.8, 0.6, GLASS);
    // A small backup sight on top.
    bar(g, 21.8, 13.5, 22.8, 13.5, 0.35, BLACK);
    Marks { grip: [16.0, 8.5, 15.0], muzzle: [16.0, y, 0.0], butt: [16.0, 15.0, 31.0], fore: Some([16.0, 12.0, 8.4]) }
}

// ---- bolt-action rifles ----



/// Short scout rifle: a short barrel with an iron front sight, a long eye
/// relief scope mounted forward over the barrel, an olive synthetic stock
/// with a straight comb, a box magazine and a ghost-ring rear sight. About
/// 3.2 cm a voxel.
pub fn sniper_scout(g: &mut Grid) -> Marks {
    let y = 16.5;
    tube(g, y, 0.0, 1.2, 0.55, DARK);
    tube(g, y, 1.2, 14.0, 0.42, BARREL);
    bar(g, 16.8, 1.8, 18.3, 1.8, 0.3, DARK);
    // Stock: fore-end, action, a straight wrist and a butt.
    profile(g, 7, 17, lerp(7.0, 15.8, 10.0, 14.0), |_| 16.6, OLIVE);
    profile(g, 17, 22, |_| 13.0, |_| 16.6, OLIVE);
    profile(g, 22, 25, lerp(22.0, 12.6, 25.0, 11.4), |_| 16.6, OLIVE);
    profile(g, 25, 31, lerp(25.0, 11.6, 31.0, 11.4), |_| 17.2, OLIVE);
    slab(g, 11, 18, 30, 31, BLACK);
    // Action, bolt, ghost-ring rear sight.
    tube(g, y, 15.0, 22.5, 0.75, STEEL);
    bolt(g, y, 21.0, 2.0, 0.55);
    g.rod([16.0, 17.8, 22.0], [16.0, 18.8, 22.0], 0.6, BLACK);
    // Magazine, guard.
    slab(g, 11, 13, 18, 20, BLACK);
    guard(g, 11.7, 20.2, 21.6, 13.0, DARK);
    // Forward scope over the barrel, on a rail.
    slab(g, 17, 18, 6, 14, DARK);
    ring(g, 18, 19, 7, BLACK);
    ring(g, 18, 19, 12, BLACK);
    tube(g, 19.5, 4.5, 14.5, 0.45, BLACK);
    tube(g, 19.5, 4.0, 6.0, 0.65, BLACK);
    tube(g, 19.5, 13.0, 14.8, 0.6, BLACK);
    tube(g, 19.5, 3.85, 4.0, 0.5, GLASS);
    bar(g, 19.5, 9.5, 20.8, 9.5, 0.4, BLACK);
    // Sling swivels.
    bar(g, 14.0, 9.0, 13.2, 9.0, 0.3, STEEL);
    bar(g, 11.0, 28.5, 10.3, 28.5, 0.3, STEEL);
    Marks { grip: [16.0, 11.3, 23.8], muzzle: [16.0, y, 0.0], butt: [16.0, 14.0, 31.0], fore: Some([16.0, 14.5, 12.0]) }
}
