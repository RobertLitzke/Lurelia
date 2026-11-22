//! The vehicles as models: the chassis, and the parts that move on it --
//! wheels, the arms that swing on their pivots, coilovers that telescope,
//! the steering wheel and the driver. Each part is its own instance posed
//! from the physics, so what you see the suspension do is what it does.
//!
//! Colours are the ride models' (ride.rs, palette slots 48..64). The beach
//! buggy's frame and arms are blue: they read from a copy of that block with
//! the frame colour changed (`models`).

use super::{Spec, Vehicle};
use crate::models::{Grid, Instance, Model};
use crate::ride::{BOOT, FRAME, HELMET, JACKET, METAL, PACK, SKIN, SUIT_DARK, TROUSERS, TYRE};
use glam::{Quat, Vec3};

// Vehicle models, appended after the ride models in this order.
pub const RAIL: usize = 0;
pub const BEACH: usize = 1;
pub const WHEEL: usize = 2;
pub const ARM: usize = 3;
pub const COIL: usize = 4;
pub const SHAFT: usize = 5;
pub const STEER: usize = 6;
pub const DRIVER: usize = 7;
pub const STEER_HANDS: usize = 8;
/// The sand rail as seen from its own seat: no A-pillars, roof or dash bar,
/// which at arm's length fill the view.
pub const RAIL_FP: usize = 9;
/// The beach buggy from its own seat, and its blue suspension arms.
pub const BEACH_FP: usize = 10;
pub const BEACH_ARM: usize = 11;
/// Karts, one per colour: the player's red, then the rivals' (KART_COLOURS).
pub const KART: usize = 12;
pub const KARTS: usize = 4;

/// The karts' body colours after the first, which is the shared frame red.
const KART_COLOURS: [(u8, u8, u8); 3] = [(36, 96, 210), (40, 160, 70), (236, 196, 40)];

/// Kart voxel, metres, and the body point at the kart grid's centre.
const KV: f32 = 0.055;
const K_Y0: f32 = -0.24;
const K_Z0: f32 = -0.05;
/// Where the driver and steering wheel sit on a kart, relative to the
/// buggies' (DRIVER_AT, WHEEL_AT): lower, and a little back.
const KART_SEAT: Vec3 = Vec3::new(0.0, -0.4, 0.08);

/// Height of the beach buggy's front shock towers, and so the top of its
/// front coilovers, metres up the body.
const FRONT_TOWER: f32 = 0.64;

/// The beach buggy's frame colour.
const BUGGY_BLUE: (u8, u8, u8) = (30, 84, 200);

/// Chassis voxel, metres, and where the body frame's origin (the centre of
/// mass) sits in the chassis grid.
const CV: f32 = 0.11;
const C_Y0: f32 = -0.2;
const C_Z0: f32 = -0.15;
/// Arm length and the coilover's two halves, metres.
const ARM_LEN: f32 = 0.62;
const COIL_LEN: f32 = 0.34;
const SHAFT_LEN: f32 = 0.34;
/// Driver voxel, and where the driver model's pivot sits in the body frame.
const DV: f32 = 0.05;
const DRIVER_AT: Vec3 = Vec3::new(0.0, 0.15, -0.3);
/// The steering wheel's centre in the body frame, and its tilt back.
pub const WHEEL_AT: Vec3 = Vec3::new(0.0, 0.72, -0.44);
const WHEEL_TILT: f32 = 0.45;

/// Body metres to chassis grid coordinates.
fn b(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + x / CV, (y - C_Y0) / CV, 16.0 + (z - C_Z0) / CV]
}

/// Body metres to driver grid coordinates (relative to `DRIVER_AT`).
fn d(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + (x - DRIVER_AT.x) / DV, (y - DRIVER_AT.y) / DV, 16.0 + (z - DRIVER_AT.z) / DV]
}

fn pair(g: &mut Grid, a: [f32; 3], bb: [f32; 3], r: f32, c: u8) {
    g.rod(b(-a[0], a[1], a[2]), b(-bb[0], bb[1], bb[2]), r, c);
    g.rod(b(a[0], a[1], a[2]), b(bb[0], bb[1], bb[2]), r, c);
}

/// A tube-frame sand rail with a roll cage and the engine out back. `buggy`
/// draws the beach buggy's version: a braced roof, a bumper hoop ahead of
/// the nose, low side bars and a pair of lamps, and no number board.
fn rail(g: &mut Grid, seat: bool, buggy: bool) {
    let t = 0.45; // tube radius in voxels (5 cm)
                  // Lower rails, nose and tail.
    pair(g, [0.42, 0.12, -1.05], [0.45, 0.12, 0.65], t, FRAME);
    pair(g, [0.42, 0.12, -1.05], [0.2, 0.3, -1.8], t, FRAME);
    pair(g, [0.45, 0.12, 0.65], [0.36, 0.2, 1.45], t, FRAME);
    g.rod(b(-0.2, 0.3, -1.8), b(0.2, 0.3, -1.8), t, FRAME);
    g.rod(b(-0.36, 0.2, 1.45), b(0.36, 0.2, 1.45), t, FRAME);
    // Upper nose rails and the front shock towers. The buggy's are lower,
    // so the road shows over its nose from the seat.
    let (nose, tower) = if buggy { (0.48, FRONT_TOWER) } else { (0.62, 0.85) };
    pair(g, [0.2, 0.3, -1.8], [0.3, nose, -1.3], t, FRAME);
    pair(g, [0.3, nose, -1.3], [0.42, 0.62, -0.9], t, FRAME);
    pair(g, [0.42, 0.12, -1.25], [0.42, tower, -1.5], t, FRAME);
    g.rod(b(-0.42, tower, -1.5), b(0.42, tower, -1.5), t, FRAME);
    // The cage: A-pillars, roof, main hoop and its braces. From the seat
    // only the hoop behind the driver is kept.
    if !seat {
        pair(g, [0.42, 0.12, -0.9], [0.5, 1.45, -0.35], t, FRAME);
        pair(g, [0.5, 1.45, -0.35], [0.5, 1.45, 0.55], t, FRAME);
        g.rod(b(-0.5, 1.45, -0.35), b(0.5, 1.45, -0.35), t, FRAME);
    }
    pair(g, [0.55, 0.12, 0.6], [0.5, 1.45, 0.55], t, FRAME);
    g.rod(b(-0.5, 1.45, 0.55), b(0.5, 1.45, 0.55), t, FRAME);
    g.rod(b(-0.5, 1.45, 0.55), b(0.5, 0.75, 0.6), t * 0.8, FRAME);
    // Side bars at the driver's hip, and the dash bar.
    pair(g, [0.45, 0.62, -0.9], [0.55, 0.7, 0.6], if seat { t * 0.6 } else { t }, FRAME);
    if !seat {
        g.rod(b(-0.45, 0.72, -0.82), b(0.45, 0.72, -0.82), t, FRAME);
    }
    // The engine bay: the hoop down to the tail.
    pair(g, [0.5, 0.95, 0.58], [0.36, 0.55, 1.45], t, FRAME);
    g.rod(b(-0.36, 0.55, 1.45), b(0.36, 0.55, 1.45), t, FRAME);
    // Floor pan and seat.
    g.fill(
        [b(-0.4, 0.0, 0.0)[0] as i32, b(0.0, 0.08, 0.0)[1] as i32, b(0.0, 0.0, -0.95)[2] as i32],
        [b(0.4, 0.0, 0.0)[0] as i32 + 1, b(0.0, 0.16, 0.0)[1] as i32 + 1, b(0.0, 0.0, 0.6)[2] as i32],
        SUIT_DARK,
    );
    g.ellipsoid(b(0.0, 0.28, 0.12), [2.6, 1.2, 2.6], SUIT_DARK);
    g.rod(b(0.0, 0.3, 0.35), b(0.0, 1.0, 0.5), 2.2, SUIT_DARK);
    // The engine, its cooling fins and the exhaust.
    g.ellipsoid(b(0.0, 0.5, 1.0), [2.4, 2.2, 2.8], METAL);
    g.ellipsoid(b(0.0, 0.78, 1.02), [1.4, 0.8, 2.0], SUIT_DARK);
    g.rod(b(0.18, 0.45, 1.15), b(0.25, 0.65, 1.6), 0.7, METAL);
    // Steering column.
    g.rod(b(0.0, 0.45, -1.1), b(WHEEL_AT.x, WHEEL_AT.y - 0.04, WHEEL_AT.z - 0.05), 0.35, METAL);
    if buggy {
        // The bumper: a hoop ahead of the nose, braced back to it.
        pair(g, [0.2, 0.3, -1.8], [0.34, 0.28, -2.02], t, FRAME);
        g.rod(b(-0.34, 0.28, -2.02), b(0.34, 0.28, -2.02), t, FRAME);
        pair(g, [0.34, 0.28, -2.02], [0.26, 0.5, -1.98], t, FRAME);
        g.rod(b(-0.26, 0.5, -1.98), b(0.26, 0.5, -1.98), t, FRAME);
        // Side bars low along the cockpit, out from the lower rails.
        pair(g, [0.42, 0.18, -0.95], [0.66, 0.32, -0.75], t, FRAME);
        pair(g, [0.66, 0.32, -0.75], [0.68, 0.32, 0.45], t, FRAME);
        pair(g, [0.68, 0.32, 0.45], [0.47, 0.16, 0.6], t, FRAME);
        if !seat {
            // Lamps on the shock-tower bar, which from the seat sit in the view.
            for side in [-1.0f32, 1.0] {
                g.ellipsoid(b(side * 0.24, FRONT_TOWER + 0.07, -1.55), [0.9, 0.9, 0.7], METAL);
            }
            // The roof crossed both ways, and a bar across the A-pillars.
            g.rod(b(-0.5, 1.45, -0.35), b(0.5, 1.45, 0.55), t * 0.8, FRAME);
            g.rod(b(0.5, 1.45, -0.35), b(-0.5, 1.45, 0.55), t * 0.8, FRAME);
            g.rod(b(-0.46, 0.95, -0.72), b(0.46, 0.95, -0.72), t * 0.8, FRAME);
        }
    }
    // A number board on the nose, which from the seat only blocks the view.
    if !seat && !buggy {
        g.fill(
            [b(-0.18, 0.0, 0.0)[0] as i32, b(0.0, 0.34, 0.0)[1] as i32, b(0.0, 0.0, -1.78)[2] as i32],
            [b(0.18, 0.0, 0.0)[0] as i32, b(0.0, 0.58, 0.0)[1] as i32, b(0.0, 0.0, -1.68)[2] as i32],
            PACK,
        );
    }
}

/// Body metres to kart grid coordinates.
fn k(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + x / KV, (y - K_Y0) / KV, 16.0 + (z - K_Z0) / KV]
}

/// Fill the box between two body points on the kart grid.
fn kfill(g: &mut Grid, a: [f32; 3], b: [f32; 3], c: u8) {
    let (p, q) = (k(a[0], a[1], a[2]), k(b[0], b[1], b[2]));
    g.fill(
        [p[0].min(q[0]) as i32, p[1].min(q[1]) as i32, p[2].min(q[2]) as i32],
        [p[0].max(q[0]).ceil() as i32, p[1].max(q[1]).ceil() as i32, p[2].max(q[2]).ceil() as i32],
        c,
    );
}

/// A racing kart: a flat tray on the ground, a nose cone and side pods in
/// the body colour, a bucket seat, the engine beside it on the right, and a
/// bumper bar behind. The ground is 0.21 m under the body origin.
fn kart(g: &mut Grid) {
    // The tray and its frame rails.
    kfill(g, [-0.32, -0.13, -0.62], [0.32, -0.1, 0.62], SUIT_DARK);
    for side in [-1.0f32, 1.0] {
        g.rod(k(side * 0.3, -0.1, -0.7), k(side * 0.3, -0.1, 0.62), 0.7, METAL);
    }
    // Nose cone: a wedge rising toward the steering column, and the bumper.
    for i in 0..=8 {
        let t = i as f32 / 8.0;
        let z = -0.85 + t * 0.5;
        let half = 0.22 + t * 0.12;
        let top = -0.04 + t * 0.2;
        kfill(g, [-half, -0.11, z], [half, top, z + 0.065], FRAME);
    }
    g.rod(k(-0.42, -0.08, -0.88), k(0.42, -0.08, -0.88), 0.8, METAL);
    // A number plate on the nose.
    kfill(g, [-0.12, 0.08, -0.5], [0.12, 0.2, -0.46], HELMET);
    // Side pods between the wheels.
    for side in [-1.0f32, 1.0] {
        kfill(g, [side * 0.34, -0.11, -0.25], [side * 0.56, 0.04, 0.3], FRAME);
        kfill(g, [side * 0.36, 0.04, -0.2], [side * 0.54, 0.07, 0.25], FRAME);
    }
    // The seat: a bucket, and its back.
    g.ellipsoid(k(0.0, -0.04, 0.12), [3.6, 1.6, 3.4], SUIT_DARK);
    g.rod(k(0.0, -0.02, 0.32), k(0.0, 0.32, 0.4), 3.2, SUIT_DARK);
    // The engine on the right, beside the seat, with its exhaust.
    g.ellipsoid(k(0.36, 0.08, 0.38), [2.2, 2.6, 2.6], METAL);
    g.ellipsoid(k(0.36, 0.3, 0.38), [1.6, 0.8, 1.8], SUIT_DARK);
    g.rod(k(0.36, 0.0, 0.5), k(0.3, 0.05, 0.75), 0.8, METAL);
    // Rear bumper bar across behind the wheels.
    g.rod(k(-0.62, -0.06, 0.74), k(0.62, -0.06, 0.74), 0.8, METAL);
    for side in [-1.0f32, 1.0] {
        g.rod(k(side * 0.3, -0.1, 0.58), k(side * 0.55, -0.06, 0.74), 0.7, METAL);
    }
    // The steering column up to the wheel.
    let w = WHEEL_AT + KART_SEAT;
    g.rod(k(0.0, 0.0, -0.55), k(w.x, w.y - 0.04, w.z - 0.05), 0.6, METAL);
}

/// A paddle-and-knob sand tyre on a dished rim, axle along X, hub at the
/// grid's centre. `r` is the tyre's radius, `w` its width, metres.
fn wheel(g: &mut Grid, r: f32, w: f32) {
    let v = r * 1.06 / 16.0;
    for y in 0..32 {
        for z in 0..32 {
            let (py, pz) = ((y as f32 + 0.5 - 16.0) * v, (z as f32 + 0.5 - 16.0) * v);
            let rho = (py * py + pz * pz).sqrt();
            let ang = pz.atan2(py);
            for x in 0..32 {
                let px = (x as f32 + 0.5 - 16.0) * v;
                if px.abs() > w * 0.5 {
                    continue;
                }
                // Rounded shoulders: the outer edge falls away near the sides.
                let edge = r - 0.05 * ((px.abs() / (w * 0.5)).powi(4));
                // Knobs every 22.5 degrees stand proud of the carcass.
                let knob = ((ang / (std::f32::consts::TAU / 16.0)).fract() - 0.5).abs() < 0.22;
                let outer = if knob { edge } else { edge - 0.03 };
                if rho <= outer && rho >= r * 0.6 {
                    g.set(x, y, z, TYRE);
                } else if rho < r * 0.6 && px.abs() < w * 0.42 {
                    // The rim: a dish with five holes, its face near the
                    // tyre's sidewall so it catches the light.
                    let hole = ((ang / (std::f32::consts::TAU / 5.0)).fract() - 0.5).abs() < 0.18 && (0.25 * r..0.47 * r).contains(&rho);
                    if !hole && (px.abs() > w * 0.32 || rho < 0.15 * r || rho > 0.52 * r) {
                        g.set(x, y, z, METAL);
                    }
                }
            }
        }
    }
}

/// A rod along +X from the pivot, `len` metres in a grid of `vox` voxels.
fn bar(g: &mut Grid, len: f32, vox: f32, r: f32, c: u8) {
    g.rod([4.0, 16.0, 16.0], [4.0 + len / vox, 16.0, 16.0], r, c);
}

/// The steering wheel, round in the XY plane about the pivot, and in first
/// person the gloves on its rim at ten to two.
fn steering(g: &mut Grid, hands: bool) {
    let n = 20;
    for k in 0..n {
        let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
        g.rod([16.0 + 5.5 * a0.cos(), 16.0 + 5.5 * a0.sin(), 16.0], [16.0 + 5.5 * a1.cos(), 16.0 + 5.5 * a1.sin(), 16.0], 0.55, TYRE);
    }
    for a in [0.0f32, std::f32::consts::PI, -std::f32::consts::FRAC_PI_2] {
        g.rod([16.0, 16.0, 16.0], [16.0 + 5.0 * a.cos(), 16.0 + 5.0 * a.sin(), 16.0], 0.4, METAL);
    }
    g.ellipsoid([16.0, 16.0, 16.5], [1.0, 1.0, 0.8], METAL);
    if hands {
        for side in [-1.0f32, 1.0] {
            let a = std::f32::consts::FRAC_PI_2 - side * 1.0;
            let (x, y) = (16.0 + 5.5 * a.cos(), 16.0 + 5.5 * a.sin());
            g.ellipsoid([x, y, 16.5], [1.2, 1.4, 1.3], BOOT);
            // The cuff, back toward the driver and down out of the view.
            g.rod([x, y - 0.5, 17.5], [x + side * 2.0, y - 6.0, 31.0], 0.9, JACKET);
        }
    }
}

/// The driver sitting in, hands on the wheel. Drawn in the chase camera only.
fn driver(g: &mut Grid) {
    for side in [-1.0f32, 1.0] {
        // Thighs forward from the seat, shins down to the pedals.
        g.rod(d(side * 0.12, 0.36, 0.15), d(side * 0.14, 0.55, -0.45), 2.2, TROUSERS);
        g.rod(d(side * 0.14, 0.55, -0.45), d(side * 0.14, 0.25, -0.95), 1.9, TROUSERS);
        g.ellipsoid(d(side * 0.14, 0.22, -1.02), [1.6, 1.6, 2.4], BOOT);
        // Arms from the shoulders to the wheel.
        let hand = Vec3::new(side * 0.13, WHEEL_AT.y + 0.04, WHEEL_AT.z + 0.04);
        g.rod(d(side * 0.2, 1.02, 0.28), d(side * 0.2, 0.8, 0.0), 1.6, JACKET);
        g.rod(d(side * 0.2, 0.8, 0.0), d(hand.x, hand.y, hand.z), 1.4, JACKET);
        g.ellipsoid(d(hand.x, hand.y, hand.z), [1.4, 1.4, 1.4], BOOT);
    }
    g.rod(d(0.0, 0.38, 0.28), d(0.0, 1.0, 0.32), 4.2, JACKET);
    g.ellipsoid(d(0.0, 1.17, 0.27), [2.4, 2.7, 2.6], SKIN);
    g.ellipsoid(d(0.0, 1.22, 0.29), [2.9, 2.9, 3.0], HELMET);
    g.fill(
        [d(0.0, 0.0, 0.0)[0] as i32 - 2, d(0.0, 1.15, 0.0)[1] as i32, d(0.0, 0.0, 0.17)[2] as i32 - 2],
        [d(0.0, 0.0, 0.0)[0] as i32 + 3, d(0.0, 1.24, 0.0)[1] as i32, d(0.0, 0.0, 0.17)[2] as i32],
        SUIT_DARK,
    );
}

/// The vehicle models, in the order of the constants above. `spec` sizes
/// the wheel.
/// `palette` gains the beach buggy's colour block: the shared block with
/// the frame colour made blue.
pub fn models(spec: &Spec, palette: &mut Vec<[f32; 4]>) -> Vec<Model> {
    let at = |m: &mut Model, p: [f32; 3]| m.pivot = Vec3::from(p);
    let blue = palette.len() as u32;
    let shared: Vec<[f32; 4]> = palette.iter().take(crate::models::FIXED_SLOTS).copied().collect();
    palette.extend(shared);
    if let Some(c) = palette.get_mut(blue as usize + FRAME as usize) {
        *c = crate::models::srgb(BUGGY_BLUE.0, BUGGY_BLUE.1, BUGGY_BLUE.2, 0.0);
    }
    let mut out = Vec::new();
    let mut g = Grid::new();
    rail(&mut g, false, false);
    let mut m = g.build(CV, 1.0, 1.7);
    at(&mut m, b(0.0, 0.0, 0.0));
    out.push(m);
    let mut g = Grid::new();
    rail(&mut g, false, true);
    let mut m = g.build(CV, 1.0, 1.7);
    at(&mut m, b(0.0, 0.0, 0.0));
    m.block = blue;
    out.push(m);
    let mut g = Grid::new();
    wheel(&mut g, spec.wheel_radius, spec.tyre_width);
    let wv = spec.wheel_radius * 1.06 / 16.0;
    let mut m = g.build(wv, 0.4, 0.8);
    at(&mut m, [16.0, 16.0, 16.0]);
    out.push(m);
    for (len, vox, r, c) in [(ARM_LEN, 0.025, 0.9, FRAME), (COIL_LEN, 0.016, 2.6, PACK), (SHAFT_LEN, 0.016, 1.0, METAL)] {
        let mut g = Grid::new();
        bar(&mut g, len, vox, r, c);
        if c == PACK {
            // Coils round the body of the damper.
            let turns = 7;
            for k in 0..turns * 8 {
                let (a0, a1) = (k as f32 / 8.0 * std::f32::consts::TAU, (k + 1) as f32 / 8.0 * std::f32::consts::TAU);
                let x0 = 4.0 + (k as f32 / (turns * 8) as f32) * len / vox;
                let x1 = 4.0 + ((k + 1) as f32 / (turns * 8) as f32) * len / vox;
                g.rod([x0, 16.0 + 4.2 * a0.cos(), 16.0 + 4.2 * a0.sin()], [x1, 16.0 + 4.2 * a1.cos(), 16.0 + 4.2 * a1.sin()], 0.8, HELMET);
            }
        }
        let mut m = g.build(vox, 0.1, 0.1);
        at(&mut m, [4.0, 16.0, 16.0]);
        out.push(m);
    }
    let mut g = Grid::new();
    steering(&mut g, false);
    let mut m = g.build(0.02, 0.2, 0.3);
    at(&mut m, [16.0, 16.0, 16.0]);
    out.push(m);
    let mut g = Grid::new();
    driver(&mut g);
    let mut m = g.build(DV, 0.4, 1.3);
    at(&mut m, [16.0, 0.0, 16.0]);
    out.push(m);
    let mut g = Grid::new();
    steering(&mut g, true);
    let mut m = g.build(0.02, 0.2, 0.3);
    at(&mut m, [16.0, 16.0, 16.0]);
    out.push(m);
    let mut g = Grid::new();
    rail(&mut g, true, false);
    let mut m = g.build(CV, 1.0, 1.7);
    at(&mut m, b(0.0, 0.0, 0.0));
    out.push(m);
    let mut g = Grid::new();
    rail(&mut g, true, true);
    let mut m = g.build(CV, 1.0, 1.7);
    at(&mut m, b(0.0, 0.0, 0.0));
    m.block = blue;
    out.push(m);
    let mut g = Grid::new();
    bar(&mut g, ARM_LEN, 0.025, 0.9, FRAME);
    let mut m = g.build(0.025, 0.1, 0.1);
    at(&mut m, [4.0, 16.0, 16.0]);
    m.block = blue;
    out.push(m);
    // The karts: red in the shared block, the rivals each in a block of
    // their own with the frame colour changed.
    for i in 0..KARTS {
        let block = if i == 0 {
            0
        } else {
            let start = palette.len() as u32;
            let shared: Vec<[f32; 4]> = palette.iter().take(crate::models::FIXED_SLOTS).copied().collect();
            palette.extend(shared);
            let (r, g, b) = KART_COLOURS[i - 1];
            if let Some(c) = palette.get_mut(start as usize + FRAME as usize) {
                *c = crate::models::srgb(r, g, b, 0.0);
            }
            start
        };
        let mut g = Grid::new();
        kart(&mut g);
        let mut m = g.build(KV, 0.7, 1.0);
        at(&mut m, k(0.0, 0.0, 0.0));
        m.block = block;
        out.push(m);
    }
    out
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn put(out: &mut Vec<(usize, Instance)>, models: &[Model], i: usize, pos: Vec3, q: Quat, shadow: f32) {
    let Some(md) = models.get(i) else { return };
    out.push((
        i,
        Instance {
            pos_yaw: [pos.x, pos.y, pos.z, 0.0],
            info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
            tint: [1.0, 1.0, 1.0, 0.0],
            rot: [q.x, q.y, q.z, q.w],
            shift: [0.0, 0.0, 0.0, shadow],
            flesh: [[0.0; 4]; 3],
            glow: [0.0; 4],
        },
    ));
}

/// The car's instances, `alpha` of the way through the step. `driver` draws
/// the driver (the chase camera, or a parked car's empty seat when false);
/// `hands` draws the first-person gloves on the wheel.
pub fn instances(v: &Vehicle, alpha: f32, base: usize, models: &[Model], driver: bool, hands: bool, out: &mut Vec<(usize, Instance)>) {
    instances_in(v, alpha, base, models, driver, hands, 0, out);
}

/// `instances`, for a kart in colour `colour` (0 the player's red, then
/// KART_COLOURS); other vehicles ignore it.
#[allow(clippy::too_many_arguments)]
pub fn instances_in(v: &Vehicle, alpha: f32, base: usize, models: &[Model], driver: bool, hands: bool, colour: usize, out: &mut Vec<(usize, Instance)>) {
    let (pos, rot) = v.pose(alpha);
    let (vr, vp) = v.visual_lean(alpha);
    let shown = rot * Quat::from_rotation_z(vr) * Quat::from_rotation_x(vp);
    if v.spec.model == "kart" {
        kart_instances(v, alpha, base, models, (driver, hands), colour % KARTS, (pos, rot, shown), out);
        return;
    }
    let buggy = v.spec.model == "beach_buggy";
    let (body, seat, arm) = if buggy { (BEACH, BEACH_FP, BEACH_ARM) } else { (RAIL, RAIL_FP, ARM) };
    if hands {
        // From the seat: the open frame in view, the whole one in the shadow.
        put(out, models, base + seat, pos, shown, crate::ride::NO_SHADOW);
        put(out, models, base + body, pos, shown, crate::ride::SHADOW_ONLY);
    } else {
        put(out, models, base + body, pos, shown, 0.0);
    }
    let up = rot * Vec3::Y;
    let steer = v.prev_steer + (v.steer - v.prev_steer) * alpha;
    let s = &v.spec;
    for w in &v.wheels {
        let t = w.prev_t + (w.t - w.prev_t) * alpha;
        let mount = pos + rot * w.mount;
        let hub = mount - up * t;
        let side = if w.left { -1.0 } else { 1.0 };
        let turn = if w.rear { 0.0 } else { steer };
        let roll = w.prev_roll + wrap(w.roll - w.prev_roll) * alpha;
        let q = rot * Quat::from_rotation_y(turn) * Quat::from_rotation_x(-roll);
        put(out, models, base + WHEEL, hub, q, 0.0);
        // The arm from its pivot on the frame out to the hub, and the
        // coilover from its top mount down onto the arm.
        let (pivot, top) = if w.rear {
            (Vec3::new(side * 0.5, 0.25, w.mount.z - ARM_LEN + 0.05), Vec3::new(side * 0.5, 0.9, w.mount.z - 0.25))
        } else {
            let top_y = if buggy { FRONT_TOWER - 0.08 } else { 0.75 };
            (Vec3::new(side * 0.28, w.mount.y - s.travel * 0.55, w.mount.z), Vec3::new(side * 0.42, top_y, w.mount.z))
        };
        let pivot = pos + shown * pivot;
        let to_hub = (hub - pivot).normalize_or(rot * Vec3::X * side);
        let arm_q = Quat::from_rotation_arc(Vec3::X, to_hub);
        put(out, models, base + arm, pivot, arm_q, 0.0);
        let low = pivot + to_hub * (ARM_LEN * 0.8);
        let top = pos + shown * top;
        let axis = low - top;
        let q = Quat::from_rotation_arc(Vec3::X, axis.normalize_or(-up));
        put(out, models, base + COIL, top, q, 0.0);
        let q = Quat::from_rotation_arc(Vec3::X, (-axis).normalize_or(up));
        put(out, models, base + SHAFT, low, q, 0.0);
    }
    // The steering wheel turns several times the road wheels' angle.
    let wheel_q = shown * Quat::from_rotation_x(WHEEL_TILT) * Quat::from_rotation_z(steer * 5.0);
    let at = pos + shown * WHEEL_AT;
    if hands {
        put(out, models, base + STEER_HANDS, at, wheel_q, crate::ride::NO_SHADOW);
    } else {
        put(out, models, base + STEER, at, wheel_q, 0.0);
    }
    if driver {
        put(out, models, base + DRIVER, pos + shown * DRIVER_AT, shown, if hands { crate::ride::SHADOW_ONLY } else { 0.0 });
    }
}

/// A kart's instances: body, wheels on their hubs (no arms or coilovers to
/// show), steering wheel and driver in the kart's lower seat.
#[allow(clippy::too_many_arguments)]
fn kart_instances(v: &Vehicle, alpha: f32, base: usize, models: &[Model], (driver, hands): (bool, bool), colour: usize, (pos, rot, shown): (Vec3, Quat, Quat), out: &mut Vec<(usize, Instance)>) {
    put(out, models, base + KART + colour, pos, shown, 0.0);
    let up = rot * Vec3::Y;
    let steer = v.prev_steer + (v.steer - v.prev_steer) * alpha;
    for w in &v.wheels {
        let t = w.prev_t + (w.t - w.prev_t) * alpha;
        let hub = pos + rot * w.mount - up * t;
        let turn = if w.rear { 0.0 } else { steer };
        let roll = w.prev_roll + wrap(w.roll - w.prev_roll) * alpha;
        put(out, models, base + WHEEL, hub, rot * Quat::from_rotation_y(turn) * Quat::from_rotation_x(-roll), 0.0);
    }
    let wheel_q = shown * Quat::from_rotation_x(WHEEL_TILT + 0.3) * Quat::from_rotation_z(steer * 4.0);
    let at = pos + shown * (WHEEL_AT + KART_SEAT);
    if hands {
        put(out, models, base + STEER_HANDS, at, wheel_q, crate::ride::NO_SHADOW);
    } else {
        put(out, models, base + STEER, at, wheel_q, 0.0);
    }
    if driver {
        put(out, models, base + DRIVER, pos + shown * (DRIVER_AT + KART_SEAT), shown, if hands { crate::ride::SHADOW_ONLY } else { 0.0 });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vehicle_models_mesh() {
        let mut palette = vec![[0.0; 4]; crate::models::FIXED_SLOTS];
        for (i, m) in super::models(&super::Spec::default(), &mut palette).iter().enumerate() {
            assert!(!m.mesh.quads.is_empty(), "model {i} is empty");
        }
    }
}
