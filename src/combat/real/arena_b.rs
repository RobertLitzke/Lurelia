//! Arena gun models, redrawn (see mod.rs): the bore, the arc gun and the
//! skipjack, three of each.
//!
//! Drawn at 2.6 to 3.4 cm a voxel. The arena's guns are chunkier than the
//! realistic set, so bodies are four voxels across (x 14..18) rather than
//! two, and round parts are rods on x = 16 at a half-voxel y so they come
//! out round.

use std::f32::consts::TAU;

use super::c::*;
use super::Marks;
use crate::models::Grid;

/// A round part straight along z on the centre line at height y.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y, z0], [16.0, y, z1], r, c);
}

/// A round part on the centre line from (y0, z0) to (y1, z1).
fn bar(g: &mut Grid, y0: f32, z0: f32, y1: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y0, z0], [16.0, y1, z1], r, c);
}

/// A cone along z, as short tubes stepping from radius r0 to r1.
fn cone(g: &mut Grid, y: f32, z0: f32, z1: f32, r0: f32, r1: f32, c: u8) {
    let n = ((z1 - z0).abs() * 2.0).ceil().max(1.0) as i32;
    for i in 0..n {
        let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let r = r0 + (r1 - r0) * (a + b) * 0.5;
        tube(g, y, z0 + (z1 - z0) * a, z0 + (z1 - z0) * b, r, c);
    }
}

/// `n` rods along z set round a circle of radius `at` about (16, y), the
/// first at angle `phase`: teeth, prongs, a cage of rails.
fn round(g: &mut Grid, y: f32, z0: f32, z1: f32, at: f32, n: usize, phase: f32, r: f32, c: u8) {
    for k in 0..n {
        let a = phase + k as f32 / n as f32 * TAU;
        let (x, yy) = (16.0 + at * a.cos(), y + at * a.sin());
        g.rod([x, yy, z0], [x, yy, z1], r, c);
    }
}

/// A box across x0..x1, y0..y1, z0..z1 (half-open).
fn boxy(g: &mut Grid, x: [i32; 2], y: [i32; 2], z: [i32; 2], c: u8) {
    g.fill([x[0], y[0], z[0]], [x[1], y[1], z[1]], c);
}



/// A trigger guard under the body from z0 to z1 at height y, its front
/// rising to `top`, and a trigger in it.
fn guard(g: &mut Grid, y: f32, z0: f32, z1: f32, top: f32, c: u8, trigger: u8) {
    bar(g, top, z0, y, z0 + 0.5, 0.45, c);
    bar(g, y, z0 + 0.5, y, z1, 0.45, c);
    bar(g, top, z0 + (z1 - z0) * 0.5, y + 0.8, z0 + (z1 - z0) * 0.6, 0.5, trigger);
}

// ---- bore ----

/// Core drill in site yellow: a copper crown of teeth round a plasma bit, a
/// finned black sleeve with a clamp-on front grip, a yellow motor box with
/// a plasma porthole on each side, a D-handle over it and a short loop stock.
pub fn bore(g: &mut Grid) -> Marks {
    let y = 16.5;
    // The crown: teeth standing out of its lip, hollow, the bit lit inside.
    tube(g, y, 1.0, 4.0, 2.7, COPPER);
    round(g, y, 0.0, 1.2, 2.3, 10, 0.0, 0.5, EDGE);
    tube(g, y, 1.0, 1.8, 1.9, BLACK);
    tube(g, y, 0.3, 3.0, 0.9, PLASMA);
    // Neck and the finned sleeve.
    cone(g, y, 4.0, 6.0, 2.0, 1.4, CHROME);
    tube(g, y, 6.0, 17.0, 1.6, BLACK);
    for z in [6.5, 8.0, 12.5, 14.0, 15.5] {
        tube(g, y, z, z + 0.7, 2.3, YELLOW);
    }
    // Clamp and the front grip off it.
    tube(g, y, 9.5, 11.5, 2.2, GUNMETAL);
    bar(g, 14.5, 10.5, 10.0, 11.3, 1.0, BLACK);
    tube(g, 10.0, 10.8, 11.8, 1.1, YELLOW);
    // Motor box, hazard band along its foot, and the plasma portholes.
    boxy(g, [14, 18], [13, 20], [16, 26], YELLOW);
    boxy(g, [14, 18], [13, 14], [16, 26], BLACK);
    for z in [17, 19, 21, 23, 25] {
        boxy(g, [14, 18], [13, 14], [z, z + 1], YELLOW);
    }
    g.rod([13.6, 17.0, 20.5], [18.4, 17.0, 20.5], 1.9, BLACK);
    g.rod([13.3, 17.0, 20.5], [18.7, 17.0, 20.5], 1.2, PLASMA);
    // Vent slits at the back of the box.
    for yy in [15, 17] {
        boxy(g, [13, 14], [yy, yy + 1], [23, 25], BLACK);
        boxy(g, [18, 19], [yy, yy + 1], [23, 25], BLACK);
    }
    // D-handle over the motor.
    bar(g, 20.0, 17.0, 23.0, 18.0, 0.6, BLACK);
    bar(g, 23.0, 18.0, 23.0, 23.5, 0.8, BLACK);
    bar(g, 23.0, 23.5, 20.0, 24.5, 0.6, BLACK);
    // Guard, trigger, grip.
    guard(g, 11.2, 21.0, 23.6, 13.0, BLACK, YELLOW);
    bar(g, 13.5, 23.2, 9.0, 25.0, 1.2, BLACK);
    tube(g, 9.3, 24.2, 25.7, 1.25, YELLOW);
    // Loop stock: a tube back from the box, a strut under, a yellow pad.
    tube(g, 18.5, 26.0, 30.0, 0.8, BLACK);
    bar(g, 13.5, 25.5, 12.5, 30.0, 0.6, BLACK);
    boxy(g, [14, 18], [11, 20], [30, 31], YELLOW);
    boxy(g, [14, 18], [11, 12], [30, 31], BLACK);
    Marks { grip: [16.0, 9.0, 25.0], muzzle: [16.0, y, 0.0], butt: [16.0, 15.5, 31.0], fore: Some([16.0, 10.0, 11.3]) }
}



// ---- arc gun ----


/// Tank gun in purple and mint: a fork of two chrome horns with lit balls
/// round a lit orb, a ribbed purple barrel, the mint tank both beams draw
/// from riding high on posts with a lit gauge down its side, a purple
/// receiver and a skeleton stock.
pub fn arc_gun(g: &mut Grid) -> Marks {
    let y = 15.5;
    // The fork and the orb between its horns.
    bar(g, y + 1.0, 5.0, y + 3.2, 1.0, 0.55, CHROME);
    bar(g, y - 1.0, 5.0, y - 3.2, 1.0, 0.55, CHROME);
    g.ellipsoid([16.0, y + 3.3, 0.9], [0.8, 0.8, 0.8], GLOW_CYAN);
    g.ellipsoid([16.0, y - 3.3, 0.9], [0.8, 0.8, 0.8], GLOW_CYAN);
    g.ellipsoid([16.0, y, 1.5], [1.1, 1.1, 1.3], PULSE);
    tube(g, y, 2.5, 5.5, 0.6, CHROME);
    // The barrel, ribbed, and a mint handguard under it.
    tube(g, y, 5.0, 15.0, 1.4, GUNMETAL);
    for z in [6.0, 7.5, 9.0, 10.5, 12.0] {
        tube(g, y, z, z + 0.6, 1.7, MINT);
    }
    tube(g, 13.5, 6.5, 13.5, 1.0, MINT);
    // The tank: posts up from the gun, mint drum, purple caps, the gauge.
    for z in [11.5, 19.5] {
        bar(g, y + 1.0, z, 20.0, z, 0.6, GUNMETAL);
    }
    tube(g, 21.5, 9.0, 22.0, 2.0, MINT);
    cone(g, 21.5, 7.5, 9.0, 1.0, 2.0, GUNMETAL);
    tube(g, 21.5, 22.0, 23.0, 2.1, GUNMETAL);
    g.rod([18.0, 21.5, 10.5], [18.0, 21.5, 20.5], 0.5, PULSE);
    for z in [13.0, 17.0] {
        tube(g, 21.5, z, z + 0.6, 2.15, GUNMETAL);
    }
    // Hoses from the tank's back cap down into the receiver.
    g.rod([15.5, 20.5, 23.0], [15.5, 18.5, 24.5], 0.5, BLACK);
    g.rod([16.5, 20.5, 23.0], [16.5, 18.5, 24.5], 0.5, BLACK);
    // Receiver, a lit vent row, guard and grip.
    boxy(g, [14, 18], [12, 19], [14, 26], GUNMETAL);
    boxy(g, [14, 18], [12, 13], [14, 26], MINT);
    for z in [15, 17] {
        boxy(g, [13, 19], [15, 17], [z, z + 1], GLOW_CYAN);
    }
    guard(g, 10.2, 19.0, 21.8, 12.0, MINT, GLOW_CYAN);
    bar(g, 12.5, 21.6, 8.0, 23.4, 1.15, BLACK);
    // Skeleton stock: a top tube, a lower strut and the mint pad.
    tube(g, 17.5, 26.0, 30.0, 0.8, GUNMETAL);
    bar(g, 13.0, 25.5, 12.0, 30.0, 0.7, GUNMETAL);
    boxy(g, [14, 18], [11, 19], [30, 31], MINT);
    Marks { grip: [16.0, 8.0, 23.4], muzzle: [16.0, y, 0.0], butt: [16.0, 15.0, 31.0], fore: Some([16.0, 12.5, 10.0]) }
}


// ---- skipjack ----

/// A drum of discs on its side, axis across x from x0 to x1 at (y, z): blue
/// shell, the silver edges of the stacked discs round it, a yellow hub.
fn drum(g: &mut Grid, x0: f32, x1: f32, y: f32, z: f32, r: f32) {
    g.rod([x0, y, z], [x1, y, z], r, PLASTIC_B);
    let n = ((x1 - x0) / 0.8) as i32;
    for i in 0..n {
        let x = x0 + 0.3 + i as f32 * 0.8;
        g.rod([x, y, z], [x + 0.35, y, z], r + 0.25, EDGE);
    }
    g.rod([x0 - 0.3, y, z], [x1 + 0.3, y, z], r * 0.35, FOAM);
}



/// Compact drum-gun: a flared blue bell muzzle, a rounded orange shell with
/// a yellow stripe, a big blue drum of discs hung under the front ahead of
/// the grip, a stubby front grip ahead of that, and a blue wire stock.
pub fn skipjack(g: &mut Grid) -> Marks {
    let y = 17.5;
    // Bell muzzle and its slot.
    cone(g, y, 0.0, 4.0, 2.6, 1.3, GUNMETAL);
    boxy(g, [14, 18], [17, 18], [0, 1], BLACK);
    tube(g, y, 4.0, 8.0, 1.3, PLASTIC);
    tube(g, y, 5.0, 5.8, 1.6, TAN);
    // The shell: a rounded orange body, a yellow stripe, a blue top rib.
    g.ellipsoid([16.0, y, 16.5], [2.6, 3.0, 9.0], PLASTIC);
    tube(g, y, 12.0, 21.0, 2.7, PLASTIC);
    boxy(g, [13, 19], [17, 18], [10, 23], TAN);
    boxy(g, [15, 17], [20, 21], [10, 23], GUNMETAL);
    boxy(g, [15, 17], [21, 22], [20, 22], GUNMETAL);
    // Drum under the front.
    drum(g, 13.2, 18.8, 12.5, 14.0, 3.2);
    // Stubby front grip ahead of the drum.
    bar(g, 15.0, 8.5, 11.0, 9.0, 1.1, GUNMETAL);
    // Guard, trigger, grip.
    guard(g, 12.0, 19.0, 21.5, 14.5, GUNMETAL, TAN);
    bar(g, 14.5, 21.5, 8.0, 23.5, 1.4, PLASTIC);
    bar(g, 10.5, 22.3, 9.4, 22.7, 1.5, GUNMETAL);
    // Wire stock: two blue rods, a blue pad.
    tube(g, 19.5, 24.0, 30.0, 0.6, GUNMETAL);
    bar(g, 15.0, 24.0, 13.0, 30.0, 0.6, GUNMETAL);
    boxy(g, [14, 18], [12, 21], [30, 31], GUNMETAL);
    Marks { grip: [16.0, 8.0, 23.5], muzzle: [16.0, y, 0.0], butt: [16.0, 16.5, 31.0], fore: Some([16.0, 11.0, 9.0]) }
}
