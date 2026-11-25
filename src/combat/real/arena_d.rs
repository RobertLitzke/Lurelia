//! Arena gun models, redrawn (see mod.rs): the rocket launcher and the
//! sniper rifle, three of each.

use super::c::*;
use super::Marks;
use crate::models::Grid;

/// A round part along the centre line, from (y0, z0) to (y1, z1).
fn bar(g: &mut Grid, y0: f32, z0: f32, y1: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y0, z0], [16.0, y1, z1], r, c);
}

/// A round tube straight along z at height y.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    bar(g, y, z0, y, z1, r, c);
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

/// A two-voxel-wide body on the centre line, y0..y1 by z0..z1 (half-open).
fn slab(g: &mut Grid, y0: i32, y1: i32, z0: i32, z1: i32, c: u8) {
    g.fill([15, y0, z0], [17, y1, z1], c);
}



/// A trigger guard: a loop under the body from z0 to z1 at height y, with
/// the trigger inside it.
fn guard(g: &mut Grid, y: f32, z0: f32, z1: f32, top: f32, c: u8) {
    bar(g, top, z0, y, z0 + 0.4, 0.3, c);
    tube(g, y, z0 + 0.4, z1, 0.3, c);
    bar(g, y, z1, top, z1 + 0.2, 0.3, c);
    bar(g, top - 0.2, z0 + (z1 - z0) * 0.45, y + 0.5, z0 + (z1 - z0) * 0.6, 0.25, CHROME);
}


/// A bolt handle out to the right with a ball on the end.
fn bolt(g: &mut Grid, y: f32, z: f32, len: f32, knob: f32, c: u8) {
    g.rod([16.5, y, z], [16.5 + len, y - len * 0.5, z + 0.4], 0.3, CHROME);
    g.ellipsoid([16.7 + len, y - len * 0.5 - 0.1, z + 0.45], [knob, knob, knob], c);
}

/// A rifle scope on the centre line: objective bell at the front, a tube,
/// an eyepiece, turrets on top and to the right, a lens of `lens` colour.
fn scope(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, bell: f32, c: u8, trim: u8, lens: u8) {
    let len = z1 - z0;
    tube(g, y, z0, z0 + len * 0.18, bell, c);
    bar(g, y, z0 + len * 0.18, y, z0 + len * 0.28, (r + bell) * 0.5, c);
    tube(g, y, z0 + len * 0.28, z1 - len * 0.16, r, c);
    tube(g, y, z1 - len * 0.16, z1, r * 1.35, c);
    tube(g, y, z0 - 0.2, z0, bell * 0.75, lens);
    tube(g, y, z0 + len * 0.16, z0 + len * 0.2, bell + 0.1, trim);
    tube(g, y, z1 - len * 0.18, z1 - len * 0.14, r * 1.45, trim);
    let mid = z0 + len * 0.5;
    bar(g, y, mid, y + r + 0.9, mid, r * 0.75, trim);
    g.rod([16.0, y, mid], [16.0 + r + 0.9, y, mid], r * 0.65, trim);
}

// ---- rocket launchers ----


/// Launch pod: a square teal and white box with the round bore cut in its
/// face and an orange rocket nose in it, glowing guidance strips down both
/// sides, a big carry handle with the seeker on it, a hooded rear with a
/// pink-lit exhaust grille, and a chunky grip and fore grip underneath.
pub fn rocket_launcher(g: &mut Grid) -> Marks {
    let y = 17.5;
    // The box: teal lower half, white upper, a navy face plate.
    g.fill([13, 15, 2], [19, 18, 27], TAN);
    g.fill([13, 18, 2], [19, 20, 27], TAN);
    g.fill([12, 14, 1], [20, 21, 3], GUNMETAL);
    // Bore and the rocket in it: orange cone, then a yellow ring.
    tube(g, y, 0.6, 3.2, 1.6, BLACK);
    cone(g, y, 0.0, 2.6, 0.35, 1.3, ORANGE);
    tube(g, y, 2.6, 3.4, 1.35, ORANGE);
    // Guidance strips down both sides.
    g.fill([12, 17, 5], [13, 18, 25], GLOW_CYAN);
    g.fill([19, 17, 5], [20, 18, 25], GLOW_CYAN);
    // Side bands.
    for z in [9, 17] {
        g.fill([12, 15, z], [20, 20, z + 1], GUNMETAL);
    }
    // Hooded rear with the exhaust grille.
    g.fill([12, 14, 26], [20, 21, 31], GUNMETAL);
    g.fill([13, 15, 30], [19, 20, 31], BLACK);
    for yy in [15, 17, 19] {
        g.fill([13, yy, 30], [19, yy + 1, 31], GLOW_CYAN);
    }
    // Carry handle with the seeker sitting on it.
    g.fill([15, 20, 7], [17, 23, 8], GUNMETAL);
    g.fill([15, 20, 18], [17, 23, 19], GUNMETAL);
    g.fill([15, 22, 7], [17, 23, 19], GUNMETAL);
    g.fill([14, 23, 10], [18, 25, 15], ORANGE);
    g.fill([15, 23, 9], [17, 25, 10], GLOW_CYAN);
    // Fore grip, trigger guard, grip and a shoulder pad.
    bar(g, 14.5, 7.0, 11.0, 7.6, 0.85, ORANGE);
    slab(g, 13, 15, 14, 19, GUNMETAL);
    guard(g, 11.6, 14.2, 16.4, 13.0, GUNMETAL);
    bar(g, 13.5, 17.2, 9.6, 18.3, 0.9, BLACK);
    slab(g, 13, 15, 21, 26, ORANGE);
    Marks { grip: [16.0, 9.6, 18.3], muzzle: [16.0, y, 0.0], butt: [16.0, y, 31.0], fore: Some([16.0, 11.0, 7.6]) }
}


// ---- sniper rifles ----



/// Bullpup lance: a long square magenta shroud over the barrel with mint
/// vents, a slim chrome needle barrel out the front, the grip well forward
/// with the action and a pink-lit charge cell behind it in the stock, a
/// tall purple scope on a raised rail, and a squared mint butt.
pub fn sniper_rifle(g: &mut Grid) -> Marks {
    let y = 16.5;
    // Needle barrel and a cone flash hider.
    cone(g, y, 0.0, 1.5, 0.6, 0.4, CHROME);
    tube(g, y, 1.5, 9.0, 0.4, CHROME);
    // Shroud: magenta with a mint vent stripe and lit slits.
    g.fill([15, 14, 9], [17, 19, 21], TAN);
    g.fill([14, 15, 10], [18, 18, 20], TAN);
    for z in [11, 13, 15, 17] {
        g.fill([14, 16, z], [18, 17, z + 1], MINT);
    }
    g.fill([14, 15, 9], [18, 18, 10], GUNMETAL);
    // Grip forward, guard, the body running back over it.
    guard(g, 11.6, 16.6, 18.8, 14.0, GUNMETAL);
    bar(g, 14.0, 19.0, 9.6, 20.0, 0.85, GUNMETAL);
    // Stock body: magenta upper, the magazine behind the grip, the cell.
    g.fill([15, 13, 21], [17, 19, 31], TAN);
    g.fill([14, 14, 21], [18, 18, 27], TAN);
    slab(g, 10, 13, 22, 25, GUNMETAL);
    g.fill([14, 15, 23], [18, 17, 26], GLOW_CYAN);
    bolt(g, y, 26.0, 1.6, 0.5, MINT);
    // Cheek riser and mint butt.
    slab(g, 19, 20, 24, 30, MINT);
    g.fill([14, 11, 29], [18, 20, 31], MINT);
    slab(g, 13, 14, 26, 29, MINT);
    // Rail and a tall scope.
    slab(g, 19, 20, 10, 23, GUNMETAL);
    slab(g, 20, 23, 12, 13, GUNMETAL);
    slab(g, 20, 23, 20, 21, GUNMETAL);
    scope(g, 23.5, 8.0, 25.0, 0.65, 1.2, BLACK, MINT, GLOW_CYAN);
    Marks { grip: [16.0, 9.6, 20.0], muzzle: [16.0, y, 0.0], butt: [16.0, 15.5, 31.0], fore: Some([16.0, 14.0, 12.0]) }
}
