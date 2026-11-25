//! Arena gun models, redrawn (see mod.rs): the minigun and the scrap gun.
//!
//! Both are wide guns. The minigun's six barrels sit on a ring about four
//! voxels across round the centre line; the scrap gun is a furnace with a
//! muzzle as wide as the cone it throws. The rest follows the realistic
//! set: furniture two voxels across on x = 16, round parts as rods.

use super::c::*;
use super::Marks;
use crate::models::Grid;
use std::f32::consts::TAU;

/// A rod along z on the centre line.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y, z0], [16.0, y, z1], r, c);
}

/// A grip on the centre line from where it meets the gun down to its foot.
fn grip(g: &mut Grid, top: [f32; 2], foot: [f32; 2], r: f32, c: u8) {
    g.rod([16.0, top[0], top[1]], [16.0, foot[0], foot[1]], r, c);
}

/// A trigger guard under the body from z0 back to z1, its bottom at `y`,
/// the body's underside at `top`, and the trigger hanging in it.
fn guard(g: &mut Grid, y: i32, top: i32, z0: i32, z1: i32, c: u8) {
    g.fill([15, y, z0], [17, y + 1, z1], c);
    g.fill([15, y, z0], [17, top, z0 + 1], c);
    let zt = (z0 + z1) as f32 * 0.5;
    g.rod([16.0, top as f32, zt], [16.0, y as f32 + 1.3, zt + 0.6], 0.55, CHROME);
}

/// Six barrels on a ring round (16, y) from z0 to z1, with the spindle
/// through the middle and the bores showing at the front.
fn barrels(g: &mut Grid, y: f32, ring: f32, z0: f32, z1: f32, r: f32, c: u8) {
    tube(g, y, z0 + 0.5, z1, 0.9, BLACK);
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU + TAU / 12.0;
        let (x, yy) = (16.0 + ring * a.cos(), y + ring * a.sin());
        g.rod([x, yy, z0], [x, yy, z1], r, c);
        g.rod([x, yy, z0], [x, yy, z0 + 0.4], r * 0.55, BLACK);
    }
}

/// Where the minigun's barrels turn about, in its grid: the pivot of
/// `minigun_barrels`.
pub const MINIGUN_AXIS: [f32; 3] = [16.0, 15.0, 16.0];

/// The minigun's six barrels with their clamps, drawn in the minigun's own
/// grid so they sit where they belong, as a model of their own that spins
/// about `MINIGUN_AXIS`.
pub fn minigun_barrels(g: &mut Grid) {
    let y = MINIGUN_AXIS[1];
    barrels(g, y, 2.1, 0.0, 19.0, 0.75, CHROME);
    // Clamps: a flash ring at the front and one halfway.
    tube(g, y, 1.0, 2.2, 3.15, YELLOW);
    tube(g, y, 1.2, 2.0, 3.3, BLACK);
    tube(g, y, 9.0, 10.2, 3.15, YELLOW);
}

/// The painted vulcan: six chrome barrels in yellow clamps out of a red
/// enamel receiver, a navy carry handle arched over the top, the drum
/// slung low on the right with its brass belt climbing into the feed, and a
/// lit status window on the motor housing.
pub fn minigun(g: &mut Grid) -> Marks {
    let y = MINIGUN_AXIS[1];
    // The barrels and their clamps are `minigun_barrels`, a model of their
    // own that turns. The barrel cluster's root, then the receiver.
    tube(g, y, 17.0, 20.0, 3.5, BLACK);
    tube(g, y, 20.0, 28.5, 3.8, GUNMETAL);
    g.fill([12, 11, 20], [20, 15, 28], GUNMETAL);
    // White hazard bands at both ends of the receiver.
    tube(g, y, 20.0, 20.8, 3.95, TAN);
    tube(g, y, 27.7, 28.5, 3.95, TAN);
    // Motor at the back, its vents lit, then the butt cap and pad.
    tube(g, y, 28.5, 30.0, 3.0, BLACK);
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + TAU / 8.0;
        let (x, yy) = (16.0 + 2.6 * a.cos(), y + 2.6 * a.sin());
        g.rod([x, yy, 28.7], [x, yy, 29.8], 0.6, GLOW_YELLOW);
    }
    tube(g, y, 30.0, 31.0, 2.4, BLACK);
    // Carry handle over the receiver: two yellow posts, a black bar.
    g.fill([15, 18, 13], [17, 22, 15], YELLOW);
    g.fill([15, 18, 24], [17, 22, 26], YELLOW);
    g.rod([16.0, 21.5, 13.0], [16.0, 21.5, 26.0], 0.95, BLACK);
    // Status window on the left of the motor housing.
    g.fill([11, 15, 22], [12, 17, 26], GLOW_YELLOW);
    // The drum on the right, its black end caps and a red band, and the
    // belt of brass links up into the feed.
    g.rod([23.0, 9.0, 16.0], [23.0, 9.0, 24.0], 3.2, YELLOW);
    g.rod([23.0, 9.0, 15.6], [23.0, 9.0, 16.4], 3.35, BLACK);
    g.rod([23.0, 9.0, 23.6], [23.0, 9.0, 24.4], 3.35, BLACK);
    g.rod([23.0, 9.0, 19.4], [23.0, 9.0, 20.6], 3.3, GUNMETAL);
    for (i, yy) in (12..16).enumerate() {
        let x = 23 - i as i32 / 2;
        g.fill([x - 1, yy, 18], [x + 1, yy + 1, 23], BRASS);
        g.fill([x - 1, yy, 18 + (i as i32 % 2) * 2], [x + 1, yy + 1, 19 + (i as i32 % 2) * 2], COPPER);
    }
    g.fill([19, 14, 19], [21, 17, 22], BRASS);
    // Front grip under the barrels' root, and the pistol grip at the back.
    grip(g, [11.5, 17.6], [6.0, 17.0], 1.15, BLACK);
    tube(g, 6.0, 16.0, 18.0, 1.3, YELLOW);
    guard(g, 8, 11, 22, 26, BLACK);
    grip(g, [11.0, 26.0], [4.5, 27.8], 1.2, BLACK);
    g.rod([16.0, 4.5, 26.6], [16.0, 4.5, 28.8], 1.35, GUNMETAL);
    Marks { grip: [16.0, 4.5, 27.8], muzzle: [16.0, y, 0.0], butt: [16.0, y, 31.0], fore: Some([16.0, 6.0, 17.0]) }
}



/// The forge slab: black iron with a letterbox muzzle flaring as wide as the
/// spread, a glowing throat, hot slits down both sides, red enamel cooling
/// fins over the top, an orange firebox at the back with a yellow door and
/// a lit grate, a raked grip and a red-padded stock.
pub fn scrap_gun(g: &mut Grid) -> Marks {
    // Flared muzzle: each slice forward a little wider and taller.
    for z in 0..5 {
        let w = 8 - z / 2;
        let h = 3 - z / 3;
        g.fill([16 - w, 15 - h, z], [16 + w, 16 + h, z + 1], BLACK);
    }
    g.fill([9, 14, 0], [23, 17, 1], HOT);
    g.fill([8, 17, 0], [24, 19, 1], RUST);
    g.fill([8, 12, 0], [24, 13, 1], RUST);
    // The body.
    g.fill([11, 11, 5], [21, 19, 22], BLACK);
    for z in (6..21).step_by(3) {
        g.fill([10, 13, z], [11, 17, z + 1], HOT);
        g.fill([21, 13, z], [22, 17, z + 1], HOT);
    }
    // Rivets along the bottom edge.
    for z in (6..22).step_by(3) {
        g.set(10, 11, z, BRASS);
        g.set(21, 11, z, BRASS);
    }
    for z in (6..21).step_by(2) {
        g.fill([12, 19, z], [20, 21, z + 1], RUST);
    }
    g.fill([15, 19, 5], [17, 20, 21], BLACK);
    // Firebox: orange enamel, a yellow door on the left with a lit grate.
    g.fill([12, 10, 22], [20, 19, 27], RUST);
    g.fill([11, 12, 23], [12, 18, 26], BRASS);
    g.fill([11, 13, 24], [12, 17, 25], GLOW_RED);
    g.fill([14, 19, 23], [18, 21, 26], BLACK);
    g.fill([15, 21, 24], [17, 22, 25], HOT);
    // Stock: iron, red pad.
    for z in 27..30 {
        let b = 12 - (z - 27) / 2;
        g.fill([15, b, z], [17, 18, z + 1], BLACK);
    }
    g.fill([14, 10, 30], [18, 19, 31], RUST);
    // Fore grip under the front and the raked pistol grip.
    grip(g, [11.0, 8.5], [5.5, 8.0], 1.15, BLACK);
    guard(g, 8, 11, 17, 21, BLACK);
    grip(g, [11.0, 21.5], [4.0, 23.8], 1.2, BLACK);
    g.rod([16.0, 4.0, 22.6], [16.0, 4.0, 24.8], 1.35, RUST);
    Marks { grip: [16.0, 4.0, 23.8], muzzle: [16.0, 15.5, 0.0], butt: [16.0, 14.5, 31.0], fore: Some([16.0, 5.5, 8.0]) }
}


