//! Realistic gun models (see mod.rs): the grenade launchers, flamethrowers
//! and rocket tubes.

use super::c::*;
use super::Marks;
use crate::models::Grid;

/// A round tube along z on the centre line.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y, z0], [16.0, y, z1], r, c);
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

/// A grip on the centre line from where it meets the gun (`top`, y and z)
/// down to its foot.
fn grip(g: &mut Grid, top: [f32; 2], foot: [f32; 2], r: f32, c: u8) {
    g.rod([16.0, top[0], top[1]], [16.0, foot[0], foot[1]], r, c);
}



/// Six-round revolver: the fluted cylinder between two steel plates, a short
/// barrel off its top chamber, a vertical fore grip, a rail with a reflex
/// sight, a pistol grip and a skeleton stock.
pub fn gl_drum(g: &mut Grid) -> Marks {
    let (cy, by) = (15.5, 17.4);
    // Barrel, muzzle ring and the lug the fore grip hangs from.
    tube(g, by, 0.0, 9.5, 1.0, BARREL);
    tube(g, by, 0.0, 1.0, 1.2, DARK);
    g.fill([15, 14, 5], [17, 17, 8], DARK);
    grip(g, [14.5, 6.3], [10.5, 7.0], 1.05, GRIP);
    // Plates and the cylinder, with its six chambers bulging out of it.
    tube(g, cy, 9.0, 10.5, 3.1, STEEL);
    tube(g, cy, 10.5, 19.5, 2.5, DARK);
    for k in 0..6 {
        let a = std::f32::consts::PI / 6.0 + k as f32 * std::f32::consts::PI / 3.0;
        let (x, yy) = (16.0 + 1.9 * a.cos(), cy + 1.9 * a.sin());
        g.rod([x, yy, 10.5], [x, yy, 19.5], 1.0, STEEL);
    }
    tube(g, cy, 19.5, 21.0, 3.1, STEEL);
    // Top strap with a rail, and a reflex sight on it.
    g.fill([15, 18, 9], [17, 19, 22], DARK);
    g.fill([15, 19, 9], [17, 20, 22], BLACK);
    for z in (10..22).step_by(2) {
        g.fill([15, 19, z], [17, 20, z + 1], DARK);
    }
    g.fill([15, 20, 14], [17, 21, 18], BLACK);
    g.fill([15, 21, 14], [17, 23, 15], BLACK);
    g.fill([15, 21, 17], [17, 23, 18], BLACK);
    g.fill([15, 22, 15], [17, 23, 17], BLACK);
    g.fill([15, 21, 15], [17, 22, 17], GLASS);
    // Rear frame, trigger guard, trigger and grip.
    g.fill([15, 13, 21], [17, 19, 24], DARK);
    g.fill([15, 10, 19], [17, 11, 23], DARK);
    g.fill([15, 10, 19], [17, 13, 20], DARK);
    g.rod([16.0, 12.8, 20.8], [16.0, 11.3, 21.4], 0.55, STEEL);
    grip(g, [14.0, 22.3], [9.0, 24.3], 1.15, GRIP);
    // Skeleton stock: buffer tube, a strut down from the grip, the pad.
    tube(g, 17.0, 24.0, 29.5, 0.9, DARK);
    g.rod([16.0, 13.5, 24.0], [16.0, 12.5, 29.5], 0.7, DARK);
    g.fill([15, 11, 29], [17, 19, 30], DARK);
    g.fill([15, 11, 30], [17, 19, 31], BLACK);
    Marks { grip: [16.0, 9.0, 24.3], muzzle: [16.0, by, 0.0], butt: [16.0, 15.0, 31.0], fore: Some([16.0, 11.0, 6.9]) }
}




/// Improvised: a length of steel pipe with a copper reducer for a nozzle and
/// a torch burner as the pilot, a propane bottle clamped upright under the
/// front as the fore grip, a hose looping from its valve to a ball valve,
/// a taped wooden handle and a plank for a stock.
pub fn flame_scrap(g: &mut Grid) -> Marks {
    let y = 17.5;
    tube(g, y, 1.5, 25.5, 0.65, STEEL);
    cone(g, y, 0.0, 1.5, 0.6, 0.95, COPPER);
    tube(g, y, 25.0, 26.0, 0.8, DARK);
    // Pilot: a little burner tube along the pipe's underside, alight.
    tube(g, 16.5, 0.8, 5.0, 0.55, BRASS);
    tube(g, 16.5, 0.2, 0.8, 0.6, HOT);
    // The bottle, upright, with its shoulder, its valve and two hose clamps
    // round the pipe.
    g.rod([16.0, 7.5, 9.5], [16.0, 14.5, 9.5], 2.0, PALE);
    g.ellipsoid([16.0, 14.5, 9.5], [2.0, 1.2, 2.0], PALE);
    g.rod([16.0, 7.2, 9.5], [16.0, 7.8, 9.5], 2.1, DARK);
    g.rod([16.0, 14.0, 9.5], [16.0, 17.5, 9.5], 0.8, BRASS);
    g.rod([16.0, 16.0, 9.5], [16.0, 16.6, 9.5], 0.9, BRASS);
    for z in [7.2, 10.2] {
        tube(g, y, z, z + 0.6, 0.85, BLACK);
    }
    // Hose from the bottle's valve round the right side to the ball valve.
    g.rod([16.3, 16.0, 9.5], [17.6, 15.5, 13.0], 0.55, BLACK);
    g.rod([17.6, 15.5, 13.0], [17.6, 16.5, 16.5], 0.55, BLACK);
    g.rod([17.6, 16.5, 16.5], [16.5, 18.0, 17.5], 0.55, BLACK);
    // Ball valve with its red handle as the trigger lever, a gate valve's
    // wheel up top.
    g.fill([15, 17, 17], [17, 19, 20], BRASS);
    g.rod([16.0, 17.0, 19.0], [16.0, 14.8, 20.5], 0.55, RUST);
    g.rod([16.0, 18.0, 13.5], [16.0, 20.3, 13.5], 0.6, BRASS);
    g.rod([16.0, 20.3, 13.5], [16.0, 20.9, 13.5], 1.2, RUST);
    // Handle, taped, and the plank stock lashed under the pipe's end.
    grip(g, [17.4, 21.6], [12.0, 23.4], 1.05, WALNUT);
    for t in [0.3, 0.6] {
        let (a, b) = ([16.0, 17.4 - 5.4 * t, 21.6 + 1.8 * t], [16.0, 17.4 - 5.4 * (t + 0.12), 21.6 + 1.8 * (t + 0.12)]);
        g.rod(a, b, 1.15, BLACK);
    }
    g.fill([15, 13, 24], [17, 18, 31], ASH);
    g.fill([15, 13, 26], [17, 19, 27], BLACK);
    g.fill([15, 13, 29], [17, 18, 30], BLACK);
    Marks { grip: [16.0, 12.0, 23.4], muzzle: [16.0, y, 0.0], butt: [16.0, 15.5, 31.0], fore: Some([16.0, 11.5, 9.5]) }
}

/// Reloadable tube with the bulbous warhead loaded: the finned rocket's
/// cone and body standing out of the muzzle, a slim steel tube with a
/// wooden heat guard round its middle, the optic on the left, two pistol
/// grips under the front half and the flared blast cone at the back.
pub fn rpg_warhead(g: &mut Grid) -> Marks {
    let y = 17.5;
    // Warhead: fuze, ogive, body, then the taper into the tube.
    tube(g, y, 0.0, 1.0, 0.45, BRASS);
    cone(g, y, 1.0, 3.5, 0.5, 1.1, OLIVE);
    tube(g, y, 3.5, 5.5, 1.1, OLIVE);
    cone(g, y, 5.5, 7.5, 1.05, 0.55, OLIVE);
    // The tube, muzzle ring, heat guard and its two steel bands.
    tube(g, y, 7.0, 27.0, 0.65, STEEL);
    tube(g, y, 7.0, 7.8, 0.8, DARK);
    tube(g, y, 11.0, 20.0, 0.95, WALNUT);
    tube(g, y, 10.6, 11.0, 1.0, DARK);
    tube(g, y, 20.0, 20.4, 1.0, DARK);
    // Blast cone.
    cone(g, y, 26.5, 31.0, 0.7, 1.2, DARK);
    // Sights: front post and rear leaf on top, the optic out on the left.
    g.fill([15, 18, 10], [17, 20, 11], DARK);
    g.fill([15, 18, 20], [17, 20, 21], DARK);
    g.fill([14, 18, 15], [15, 19, 19], DARK);
    g.rod([13.6, 19.6, 14.5], [13.6, 19.6, 19.5], 0.65, DARK);
    g.set(13, 19, 14, GLASS);
    // Trigger housing and grip under the front half, a second grip behind.
    tube(g, 16.9, 12.5, 16.5, 0.6, DARK);
    g.rod([16.0, 16.5, 13.4], [16.0, 15.4, 13.8], 0.55, STEEL);
    grip(g, [17.0, 15.2], [14.0, 16.0], 0.6, GRIP);
    grip(g, [17.0, 21.0], [14.2, 21.8], 0.6, GRIP);
    Marks { grip: [16.0, 14.0, 16.0], muzzle: [16.0, y, 0.0], butt: [16.0, y, 31.0], fore: Some([16.0, 17.0, 12.0]) }
}


