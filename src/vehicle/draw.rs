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
pub const KARTS: usize = 8;
/// The karts' big wheels and their drivers, one per colour like the bodies,
/// since the rims, helmets and suits wear the kart's colours.
pub const KART_WHEEL: usize = KART + KARTS;
pub const KART_DRIVER: usize = KART_WHEEL + KARTS;
/// The ATV: its body, knobbly wheels, swing arms, handlebars (with the
/// gloves on them for first person) and rider, in a colour block of its own.
pub const ATV: usize = KART_DRIVER + KARTS;
pub const ATV_WHEEL: usize = ATV + 1;
pub const ATV_ARM: usize = ATV + 2;
pub const ATV_BARS: usize = ATV + 3;
pub const ATV_BARS_HANDS: usize = ATV + 4;
pub const ATV_RIDER: usize = ATV + 5;
/// The karts' head and tail lamps, shared by every colour: drawn with the
/// body and glowing as the lights are on (`lights::head_glow`, `tail_glow`).
pub const KART_HEADLAMP: usize = ATV_RIDER + 1;
pub const KART_TAILLAMP: usize = ATV_RIDER + 2;

/// Each kart's body colour and its trim colour (stripes, gloves, hubs): the
/// player's red, then the rivals' blue, green, yellow, purple, orange, cyan
/// and pink.
type Rgb = (u8, u8, u8);
const KART_COLOURS: [(Rgb, Rgb); KARTS] = [
    ((214, 40, 46), (252, 202, 36)),
    ((36, 96, 214), (255, 142, 28)),
    ((38, 172, 76), (250, 232, 70)),
    ((242, 196, 36), (30, 30, 38)),
    ((136, 64, 200), (240, 240, 240)),
    ((240, 120, 30), (40, 40, 48)),
    ((40, 190, 200), (250, 250, 250)),
    ((232, 96, 160), (30, 30, 38)),
];

/// Added to a kart's colours while it flashes after taking damage.
const FLASH: f32 = 0.6;

/// Light up a kart's instances, for its blink after taking damage.
pub fn flash(out: &mut [(usize, crate::models::Instance)]) {
    for (_, inst) in out {
        for c in 0..3 {
            inst.shift[c] += FLASH;
        }
    }
}

/// Kart `i`'s body colour for the HUD: 0 the player's, then the rivals'.
pub fn kart_colour(i: usize) -> [f32; 4] {
    let ((r, g, b), _) = KART_COLOURS[i % KARTS];
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}
/// Palette slots the karts' blocks repaint: cream for stripes and helmets,
/// and the helmet visor.
const CREAM: u8 = 55;
const VISOR: u8 = 60;
/// The lamps' glass, in the lamp models' own block: warm white and red,
/// glowing (`srgb`'s fourth value) as much as the instance's gain allows.
const HEADLAMP: u8 = 62;
const TAILLAMP: u8 = 63;
/// The kart tyre's radius, metres. The physics wheel is smaller (spec), so a
/// drawn wheel's hub rides `KART_R - spec.wheel_radius` above the physics
/// hub and its tread still touches the ground.
const KART_R: f32 = 0.22;
/// Karts are drawn this much bigger than their physics, about the ground
/// under them, so they are easier to see and to hit (race/arms.rs makes
/// their hitboxes as much bigger, and kart.json their bump box and eye).
/// Their handling stays that of the smaller kart.
pub const KART_SIZE: f32 = 1.3;
const KART_W: f32 = 0.27;

/// Kart voxel, metres, and the body point at the kart grid's centre.
const KV: f32 = 0.06;
/// Kart driver voxel, and the body point at the driver grid's origin.
const KDV: f32 = 0.045;
const KD_Y0: f32 = -0.2;
const KD_Z0: f32 = 0.1;
const K_Y0: f32 = -0.24;
const K_Z0: f32 = -0.05;
/// Where the driver and steering wheel sit on a kart, relative to the
/// buggies' (DRIVER_AT, WHEEL_AT): lower, and a little back.
const KART_SEAT: Vec3 = Vec3::new(0.0, -0.4, 0.08);

/// The ATV's colours: green plastics (the frame slot), amber trim and
/// helmet, a brown jacket and dark trousers.
const ATV_COLOURS: [(u8, Rgb); 5] = [(FRAME, (62, 112, 50)), (HELMET, (232, 168, 34)), (JACKET, (96, 72, 50)), (TROUSERS, (52, 58, 70)), (VISOR, (18, 26, 52))];
/// The ATV tyre's radius and width, metres. As on the kart, a drawn hub
/// rides `ATV_R - spec.wheel_radius` above the physics hub.
const ATV_R: f32 = 0.3;
const ATV_W: f32 = 0.26;
/// ATV body voxel, metres, and the body height at the bottom of its grid.
const AV: f32 = 0.07;
const A_Y0: f32 = -0.45;
/// ATV rider voxel, and the body point at the rider grid's origin.
const RV: f32 = 0.045;
const R_Y0: f32 = -0.2;
const R_Z0: f32 = -0.1;
/// The handlebars' centre in the body frame; they turn about the body's up.
const ATV_BARS_AT: Vec3 = Vec3::new(0.0, 0.72, -0.48);
/// Half the handlebars' width, and where the hands hold them.
const ATV_BAR_HALF: f32 = 0.36;
const ATV_GRIP: f32 = 0.3;
/// The ATV's swing arms: length, and the pivot inboard on the frame.
const ATV_ARM_LEN: f32 = 0.34;
const ATV_PIVOT: (f32, f32) = (0.16, -0.12);

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
    boxfill(g, k, a, b, c);
}

/// Fill the box between two body points, mapped to grid coordinates by `to`.
fn boxfill(g: &mut Grid, to: fn(f32, f32, f32) -> [f32; 3], a: [f32; 3], b: [f32; 3], c: u8) {
    let (p, q) = (to(a[0], a[1], a[2]), to(b[0], b[1], b[2]));
    g.fill(
        [p[0].min(q[0]) as i32, p[1].min(q[1]) as i32, p[2].min(q[2]) as i32],
        [p[0].max(q[0]).ceil() as i32, p[1].max(q[1]).ceil() as i32, p[2].max(q[2]).ceil() as i32],
        c,
    );
}

/// Slices of a body along z from `z0` to `z1`, on the grid `to` maps body
/// metres onto with voxels `vox` metres: `f(t)` gives the slice's (x low,
/// x high, y low, y high) at `t` in 0..=1, and `mirror` adds its reflection
/// across the middle.
#[allow(clippy::too_many_arguments)]
fn slab(g: &mut Grid, to: fn(f32, f32, f32) -> [f32; 3], vox: f32, (z0, z1): (f32, f32), mirror: bool, c: u8, f: impl Fn(f32) -> (f32, f32, f32, f32)) {
    let n = (((z1 - z0).abs() / vox) * 1.5).ceil().max(1.0) as i32;
    for i in 0..n {
        let (ta, tb) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let (x0, x1, y0, y1) = f((ta + tb) * 0.5);
        let (za, zb) = (z0 + (z1 - z0) * ta, z0 + (z1 - z0) * tb);
        boxfill(g, to, [x0, y0, za], [x1, y1, zb], c);
        if mirror {
            boxfill(g, to, [-x0, y0, za], [-x1, y1, zb], c);
        }
    }
}

/// A racing kart, an original wedge: a low tray, a pointed nose cone with a
/// front wing, side pods with intakes and number plates, a bucket seat, the
/// engine out back with twin pipes, and a bumper. Body colour, a cream racing
/// stripe and trim-coloured accents. The ground is 0.21 m under the body
/// origin.
fn kart(g: &mut Grid) {
    // The tray, and frame rails down each side.
    kfill(g, [-0.34, -0.17, -0.8], [0.34, -0.12, 0.76], SUIT_DARK);
    for side in [-1.0f32, 1.0] {
        g.rod(k(side * 0.3, -0.12, -0.8), k(side * 0.3, -0.12, 0.76), 0.7, METAL);
    }
    // Nose cone: a wedge rising to the cowl, narrow at the tip.
    slab(g, k, KV, (-0.92, -0.3), true, FRAME, |t| (0.0, 0.14 + t * 0.2, -0.15, -0.07 + t * 0.25));
    // The racing stripe over the nose and cowl (the nose lamps are a model
    // of their own, `kart_lamps`).
    slab(g, k, KV, (-0.92, -0.3), false, CREAM, |t| (-0.045, 0.045, -0.08 + t * 0.25, -0.05 + t * 0.25));
    // Front wing ahead of the tyres, with trim end plates.
    kfill(g, [-0.64, -0.16, -1.0], [0.64, -0.12, -0.86], SUIT_DARK);
    for side in [-1.0f32, 1.0] {
        kfill(g, [side * 0.6, -0.16, -1.0], [side * 0.66, 0.0, -0.84], HELMET);
        g.rod(k(side * 0.12, -0.12, -0.9), k(side * 0.12, -0.12, -0.8), 0.6, METAL);
    }
    // The cowl over the pedals.
    g.ellipsoid(k(0.0, 0.1, -0.3), [3.0, 2.0, 2.4], FRAME);
    // Side pods: wedges between the wheels with a dark intake at the front,
    // a cream stripe along the top and a number plate on the outer face.
    for side in [-1.0f32, 1.0] {
        slab(g, k, KV, (-0.4, 0.2), false, FRAME, |t| (side * 0.3, side * 0.56, -0.15, t * 0.12));
        kfill(g, [side * 0.34, -0.1, -0.43], [side * 0.52, 0.0, -0.37], SUIT_DARK);
        slab(g, k, KV, (-0.34, 0.18), false, CREAM, |t| (side * 0.38, side * 0.46, t * 0.12 - 0.005, t * 0.12 + 0.03));
        kfill(g, [side * 0.56, -0.09, -0.26], [side * 0.585, 0.05, 0.08], CREAM);
        kfill(g, [side * 0.56, -0.04, -0.2], [side * 0.59, 0.0, 0.02], HELMET);
    }
    // The bucket seat and its tilted back, in body colour with a cream
    // stripe and a trim headrest.
    g.ellipsoid(k(0.0, -0.06, 0.34), [4.2, 1.2, 4.6], SUIT_DARK);
    g.rod(k(0.0, -0.04, 0.5), k(0.0, 0.5, 0.62), 3.0, FRAME);
    g.rod(k(0.0, -0.02, 0.43), k(0.0, 0.5, 0.55), 0.8, CREAM);
    g.ellipsoid(k(0.0, 0.5, 0.62), [3.0, 1.4, 1.2], HELMET);
    // The engine out back: block, head, air filter, twin pipes, in the open.
    g.ellipsoid(k(0.0, 0.06, 0.72), [3.6, 3.0, 2.0], METAL);
    g.ellipsoid(k(0.0, 0.22, 0.72), [2.6, 1.0, 1.6], SUIT_DARK);
    g.ellipsoid(k(0.0, 0.3, 0.7), [1.5, 1.2, 1.5], HELMET);
    for side in [-1.0f32, 1.0] {
        g.rod(k(side * 0.14, 0.02, 0.76), k(side * 0.2, 0.0, 0.9), 0.75, METAL);
        g.ellipsoid(k(side * 0.2, 0.0, 0.9), [0.9, 0.9, 0.7], SUIT_DARK);
    }
    // Rear bumper: a chunky dark bar with trim end caps, braced to the tray.
    kfill(g, [-0.5, -0.15, 0.82], [0.5, -0.05, 0.9], SUIT_DARK);
    for side in [-1.0f32, 1.0] {
        kfill(g, [side * 0.5, -0.15, 0.8], [side * 0.58, -0.02, 0.9], HELMET);
        g.rod(k(side * 0.28, -0.12, 0.7), k(side * 0.4, -0.1, 0.84), 0.7, METAL);
    }
    // The steering column up to the wheel.
    let w = WHEEL_AT + KART_SEAT;
    g.rod(k(0.0, 0.1, -0.32), k(w.x, w.y - 0.04, w.z - 0.05), 0.6, METAL);
}

/// The kart's lamps on the body's grid: the two nose lamps (`head`) or two
/// tail lamps on top of the rear bumper's back edge.
fn kart_lamps(g: &mut Grid, head: bool) {
    for side in [-1.0f32, 1.0] {
        if head {
            g.ellipsoid(k(side * 0.1, -0.02, -0.9), [1.1, 0.8, 0.8], HEADLAMP);
        } else {
            kfill(g, [side * 0.26, -0.05, 0.84], [side * 0.44, 0.05, 0.9], TAILLAMP);
        }
    }
}

/// Body metres to kart driver grid coordinates.
fn kd(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + x / KDV, (y - KD_Y0) / KDV, 16.0 + (z - KD_Z0) / KDV]
}

/// The kart's driver, leaning back in the bucket seat with both hands on the
/// wheel: a cream full-face helmet with a stripe, ears and a dark visor, a
/// suit in the kart's dark shade and trim-coloured gloves.
fn kart_driver(g: &mut Grid) {
    for side in [-1.0f32, 1.0] {
        // Thighs forward from the seat, shins down to boots on the pedals.
        g.rod(kd(side * 0.1, 0.04, 0.32), kd(side * 0.13, 0.16, 0.0), 2.0, TROUSERS);
        g.rod(kd(side * 0.13, 0.16, 0.0), kd(side * 0.12, -0.03, -0.38), 1.7, TROUSERS);
        g.ellipsoid(kd(side * 0.12, -0.06, -0.42), [1.7, 1.5, 2.6], TYRE);
        // Arms: out to an elbow, then in to the gloves at ten to two.
        let hand = [side * 0.1, 0.355, -0.33];
        g.rod(kd(side * 0.2, 0.44, 0.2), kd(side * 0.28, 0.3, 0.0), 1.6, JACKET);
        g.rod(kd(side * 0.28, 0.3, 0.0), kd(hand[0], hand[1], hand[2]), 1.4, JACKET);
        g.ellipsoid(kd(hand[0], hand[1], hand[2]), [1.5, 1.5, 1.5], HELMET);
    }
    // Torso, reclined, with a cream band across the chest.
    g.rod(kd(0.0, 0.1, 0.32), kd(0.0, 0.4, 0.24), 3.5, JACKET);
    g.ellipsoid(kd(0.0, 0.45, 0.22), [4.6, 2.0, 2.6], JACKET);
    g.rod(kd(-0.17, 0.34, 0.08), kd(0.17, 0.34, 0.08), 0.8, CREAM);
    // Helmet: cream shell, team stripe over the top, ears, visor, chin.
    g.ellipsoid(kd(0.0, 0.64, 0.26), [3.3, 3.3, 3.5], CREAM);
    g.rod(kd(0.0, 0.76, 0.14), kd(0.0, 0.74, 0.4), 0.9, FRAME);
    g.rod(kd(0.0, 0.66, 0.4), kd(0.0, 0.54, 0.4), 0.9, FRAME);
    for side in [-1.0f32, 1.0] {
        g.ellipsoid(kd(side * 0.148, 0.64, 0.26), [0.6, 1.7, 1.7], HELMET);
    }
    g.ellipsoid(kd(0.0, 0.66, 0.135), [2.4, 1.1, 1.0], VISOR);
    g.ellipsoid(kd(0.0, 0.56, 0.15), [1.5, 0.9, 1.0], CREAM);
}

/// A fat slick on a deep dish rim, axle along X, hub at the grid's centre,
/// radius `r` and width `w`, metres: smooth shoulders, a cream band on each
/// sidewall, a six-spoke rim and a trim-coloured hub cap.
fn kart_wheel(g: &mut Grid, r: f32, w: f32) {
    let v = r * 1.06 / 16.0;
    for y in 0..32 {
        for z in 0..32 {
            let (py, pz) = ((y as f32 + 0.5 - 16.0) * v, (z as f32 + 0.5 - 16.0) * v);
            let rho = (py * py + pz * pz).sqrt();
            let ang = pz.atan2(py);
            for x in 0..32 {
                let px = (x as f32 + 0.5 - 16.0) * v;
                let u = px.abs() / (w * 0.5);
                if u > 1.0 {
                    continue;
                }
                // Round shoulders: the tread curls in toward the sidewalls.
                let edge = r - 0.045 * u.powi(5) - 0.02 * u.powi(2);
                if rho <= edge && rho >= r * 0.6 {
                    let band = u > 0.86 && (0.66 * r..0.76 * r).contains(&rho);
                    g.set(x, y, z, if band { CREAM } else { TYRE });
                } else if rho < r * 0.6 && u < 0.9 {
                    // The rim: a face near the sidewall with six cut-outs.
                    let spoke = ((ang / (std::f32::consts::TAU / 6.0)).fract() - 0.5).abs() < 0.2 && (0.3 * r..0.52 * r).contains(&rho);
                    if rho < 0.2 * r && u > 0.55 {
                        g.set(x, y, z, HELMET);
                    } else if !spoke && (u > 0.7 || rho < 0.2 * r || rho > 0.5 * r) {
                        g.set(x, y, z, METAL);
                    }
                }
            }
        }
    }
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

/// Body metres to ATV grid coordinates.
fn a(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + x / AV, (y - A_Y0) / AV, 16.0 + z / AV]
}

/// Body metres to ATV rider grid coordinates.
fn ar(x: f32, y: f32, z: f32) -> [f32; 3] {
    [16.0 + x / RV, (y - R_Y0) / RV, 16.0 + (z - R_Z0) / RV]
}

/// A utility ATV, original: a tube frame and skid plate under a single-
/// cylinder engine, green plastics with a hood and arched fenders over all
/// four wheels, steel racks front and back, a long seat, footboards, a front
/// bumper with twin lamps, and the exhaust up the right side to a muffler
/// under the rear rack. The ground is 0.42 m under the body origin; the
/// front axle is 0.69 m ahead of it and the rear 0.56 m behind.
fn atv(g: &mut Grid) {
    let (front, rear) = (-0.69f32, 0.56f32);
    // Frame rails, the skid plate under them and the engine between.
    for side in [-1.0f32, 1.0] {
        g.rod(a(side * 0.2, -0.17, -0.98), a(side * 0.2, -0.17, 0.9), 0.6, METAL);
        g.rod(a(side * 0.2, -0.17, -0.98), a(side * 0.2, 0.12, -0.98), 0.6, METAL);
    }
    boxfill(g, a, [-0.2, -0.24, -0.62], [0.2, -0.18, 0.5], SUIT_DARK);
    g.ellipsoid(a(0.0, -0.04, 0.0), [0.2 / AV, 0.16 / AV, 0.3 / AV], METAL);
    g.ellipsoid(a(0.0, 0.1, -0.12), [0.13 / AV, 0.09 / AV, 0.13 / AV], SUIT_DARK);
    // Footboards, with a raised lip at the back.
    for side in [-1.0f32, 1.0] {
        boxfill(g, a, [side * 0.2, -0.17, -0.34], [side * 0.5, -0.11, 0.24], SUIT_DARK);
        boxfill(g, a, [side * 0.42, -0.11, 0.16], [side * 0.5, 0.0, 0.24], SUIT_DARK);
    }
    // Fenders: shells arched over each wheel, a little wider than the tyre,
    // and an inner skirt down to the frame.
    let arch = |t: f32| 0.2 + 0.12 * (std::f32::consts::PI * t).sin();
    for (z0, z1) in [(front - 0.38, front + 0.38), (rear - 0.38, rear + 0.4)] {
        slab(g, a, AV, (z0, z1), true, FRAME, move |t| (0.26, 0.68, arch(t) - 0.06, arch(t)));
        slab(g, a, AV, (z0 + 0.06, z1 - 0.06), true, FRAME, move |t| (0.26, 0.31, -0.02, arch(t)));
    }
    // The hood, rising from the bumper to the tank, and the rear body
    // under the seat.
    slab(g, a, AV, (-1.0, -0.3), true, FRAME, |t| (0.0, 0.27, 0.0, 0.16 + 0.12 * t));
    slab(g, a, AV, (0.0, 0.94), true, FRAME, |t| (0.0, 0.27, 0.02, 0.26 - 0.06 * t));
    // Tank and its cap, the seat along the top.
    g.ellipsoid(a(0.0, 0.3, -0.26), [0.17 / AV, 0.1 / AV, 0.2 / AV], FRAME);
    g.ellipsoid(a(0.0, 0.38, -0.3), [0.05 / AV, 0.03 / AV, 0.05 / AV], METAL);
    g.ellipsoid(a(0.0, 0.31, 0.24), [0.17 / AV, 0.075 / AV, 0.38 / AV], SUIT_DARK);
    // Steering column up to the bars.
    g.rod(a(0.0, 0.26, -0.36), a(0.0, ATV_BARS_AT.y - 0.03, ATV_BARS_AT.z), 0.7, METAL);
    // Racks: a rail each side over a grid of cross bars, on posts.
    for (z0, z1, y) in [(-1.0f32, -0.48f32, 0.36f32), (0.44, 0.98, 0.38)] {
        for side in [-1.0f32, 1.0] {
            g.rod(a(side * 0.44, y, z0), a(side * 0.44, y, z1), 0.55, SUIT_DARK);
            g.rod(a(side * 0.3, y - 0.12, (z0 + z1) * 0.5), a(side * 0.3, y, (z0 + z1) * 0.5), 0.5, SUIT_DARK);
        }
        for k in 0..4 {
            let z = z0 + (z1 - z0) * k as f32 / 3.0;
            g.rod(a(-0.44, y, z), a(0.44, y, z), 0.5, SUIT_DARK);
        }
    }
    // Front bumper and its lamps, and an amber tail lamp.
    for side in [-1.0f32, 1.0] {
        g.rod(a(side * 0.26, -0.08, -1.06), a(side * 0.26, 0.22, -1.06), 0.7, SUIT_DARK);
        g.rod(a(side * 0.26, -0.08, -1.06), a(side * 0.2, -0.17, -0.98), 0.7, SUIT_DARK);
        g.ellipsoid(a(side * 0.15, 0.2, -0.99), [0.065 / AV, 0.05 / AV, 0.035 / AV], CREAM);
    }
    for y in [-0.08f32, 0.08, 0.22] {
        g.rod(a(-0.26, y, -1.06), a(0.26, y, -1.06), 0.6, SUIT_DARK);
    }
    boxfill(g, a, [-0.1, 0.12, 0.94], [0.1, 0.18, 0.98], HELMET);
    // Exhaust up the right side to a muffler under the rear rack.
    g.rod(a(0.12, 0.02, -0.1), a(0.22, 0.06, 0.3), 0.75, METAL);
    g.rod(a(0.22, 0.06, 0.3), a(0.24, 0.16, 0.7), 0.75, METAL);
    g.ellipsoid(a(0.24, 0.17, 0.8), [0.06 / AV, 0.06 / AV, 0.12 / AV], SUIT_DARK);
    // Amber trim along each fender's outer edge.
    for z in [front, rear] {
        for side in [-1.0f32, 1.0] {
            g.rod(a(side * 0.68, 0.24, z - 0.3), a(side * 0.68, 0.24, z + 0.3), 0.5, HELMET);
        }
    }
}

/// The ATV's handlebars, across X about the pivot at the grid's centre:
/// a riser, a bar swept back to rubber grips, brake levers and a lamp pod.
/// With `hands` the gloves are on the grips, cuffs back toward the rider.
fn atv_bars(g: &mut Grid, hands: bool) {
    let v = 0.03;
    let p = |x: f32, y: f32, z: f32| [16.0 + x / v, 16.0 + y / v, 16.0 + z / v];
    g.rod(p(0.0, -0.03, 0.0), p(0.0, 0.03, 0.02), 1.2, METAL);
    for side in [-1.0f32, 1.0] {
        g.rod(p(0.0, 0.03, 0.02), p(side * 0.2, 0.04, 0.04), 0.7, METAL);
        g.rod(p(side * 0.2, 0.04, 0.04), p(side * (ATV_BAR_HALF - 0.1), 0.06, 0.07), 0.7, METAL);
        g.rod(p(side * (ATV_BAR_HALF - 0.1), 0.06, 0.07), p(side * ATV_BAR_HALF, 0.06, 0.08), 1.2, TYRE);
        g.rod(p(side * 0.2, 0.05, 0.02), p(side * 0.3, 0.05, -0.02), 0.4, SUIT_DARK);
        if hands {
            g.ellipsoid(p(side * ATV_GRIP, 0.07, 0.07), [1.6, 1.6, 1.8], BOOT);
            g.rod(p(side * ATV_GRIP, 0.05, 0.1), p(side * (ATV_GRIP + 0.04), -0.12, 0.42), 1.1, JACKET);
        }
    }
    g.ellipsoid(p(0.0, 0.04, -0.05), [2.4, 1.4, 1.4], FRAME);
}

/// The ATV's rider, astride the seat, knees out, boots on the footboards,
/// hands on the grips: amber helmet with a dark visor, jacket and trousers
/// in the ATV's block, dark boots and gloves.
fn atv_rider(g: &mut Grid) {
    let bars = ATV_BARS_AT;
    for side in [-1.0f32, 1.0] {
        g.rod(ar(side * 0.11, 0.42, 0.2), ar(side * 0.26, 0.46, -0.14), 2.3, TROUSERS);
        g.rod(ar(side * 0.26, 0.46, -0.14), ar(side * 0.32, -0.02, -0.04), 1.9, TROUSERS);
        g.ellipsoid(ar(side * 0.32, -0.06, -0.08), [1.7, 1.6, 2.8], BOOT);
        let hand = [side * ATV_GRIP, bars.y + 0.07, bars.z + 0.07];
        g.rod(ar(side * 0.19, 0.86, 0.1), ar(side * 0.3, 0.7, -0.1), 1.6, JACKET);
        g.rod(ar(side * 0.3, 0.7, -0.1), ar(hand[0], hand[1], hand[2]), 1.4, JACKET);
        g.ellipsoid(ar(hand[0], hand[1], hand[2]), [1.4, 1.4, 1.5], BOOT);
    }
    // Hips on the seat and the torso leaning a little forward.
    g.ellipsoid(ar(0.0, 0.44, 0.22), [3.6, 2.2, 3.0], TROUSERS);
    g.rod(ar(0.0, 0.46, 0.22), ar(0.0, 0.84, 0.1), 3.6, JACKET);
    g.ellipsoid(ar(0.0, 0.82, 0.1), [4.4, 2.6, 2.8], JACKET);
    // Helmet, visor and chin bar.
    g.ellipsoid(ar(0.0, 1.06, 0.06), [3.2, 3.3, 3.5], HELMET);
    g.ellipsoid(ar(0.0, 1.07, -0.075), [2.4, 1.1, 1.0], VISOR);
    g.ellipsoid(ar(0.0, 0.97, -0.07), [1.6, 1.0, 1.0], HELMET);
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
    // The karts, each with a block of its own: the frame colour is the
    // body, with the trim, cream, suit and visor colours beside it.
    let mut blocks = [0u32; KARTS];
    for (i, block) in blocks.iter_mut().enumerate() {
        let start = palette.len() as u32;
        let shared: Vec<[f32; 4]> = palette.iter().take(crate::models::FIXED_SLOTS).copied().collect();
        palette.extend(shared);
        let ((r, g, b), (tr, tg, tb)) = KART_COLOURS[i];
        let dark = |c: u8| (c as f32 * 0.5) as u8;
        for (slot, (r, g, b)) in [
            (FRAME, (r, g, b)),
            (HELMET, (tr, tg, tb)),
            (CREAM, (240, 234, 220)),
            (JACKET, (dark(r), dark(g), dark(b))),
            (TROUSERS, (34, 34, 42)),
            (VISOR, (18, 26, 52)),
        ] {
            if let Some(c) = palette.get_mut(start as usize + slot as usize) {
                *c = crate::models::srgb(r, g, b, 0.0);
            }
        }
        *block = start;
    }
    for &block in &blocks {
        let mut g = Grid::new();
        kart(&mut g);
        let mut m = g.build(KV, 0.7, 1.0);
        at(&mut m, k(0.0, 0.0, 0.0));
        m.block = block;
        out.push(m);
    }
    for &block in &blocks {
        let mut g = Grid::new();
        kart_wheel(&mut g, KART_R, KART_W);
        let mut m = g.build(KART_R * 1.06 / 16.0, 0.4, 0.8);
        at(&mut m, [16.0, 16.0, 16.0]);
        m.block = block;
        out.push(m);
    }
    for &block in &blocks {
        let mut g = Grid::new();
        kart_driver(&mut g);
        let mut m = g.build(KDV, 0.4, 1.3);
        at(&mut m, kd(0.0, 0.0, 0.0));
        m.block = block;
        out.push(m);
    }
    // The ATV, in a block of its own colours.
    let block = palette.len() as u32;
    let shared: Vec<[f32; 4]> = palette.iter().take(crate::models::FIXED_SLOTS).copied().collect();
    palette.extend(shared);
    for (slot, (r, g, b)) in ATV_COLOURS {
        if let Some(c) = palette.get_mut(block as usize + slot as usize) {
            *c = crate::models::srgb(r, g, b, 0.0);
        }
    }
    let mut g = Grid::new();
    atv(&mut g);
    let mut m = g.build(AV, 0.75, 1.3);
    at(&mut m, a(0.0, 0.0, 0.0));
    m.block = block;
    out.push(m);
    let mut g = Grid::new();
    wheel(&mut g, ATV_R, ATV_W);
    let mut m = g.build(ATV_R * 1.06 / 16.0, 0.4, 0.8);
    at(&mut m, [16.0, 16.0, 16.0]);
    m.block = block;
    out.push(m);
    let mut g = Grid::new();
    bar(&mut g, ATV_ARM_LEN, 0.02, 1.2, SUIT_DARK);
    let mut m = g.build(0.02, 0.1, 0.1);
    at(&mut m, [4.0, 16.0, 16.0]);
    m.block = block;
    out.push(m);
    for hands in [false, true] {
        let mut g = Grid::new();
        atv_bars(&mut g, hands);
        let mut m = g.build(0.03, 0.4, 0.3);
        at(&mut m, [16.0, 16.0, 16.0]);
        m.block = block;
        out.push(m);
    }
    let mut g = Grid::new();
    atv_rider(&mut g);
    let mut m = g.build(RV, 0.4, 1.3);
    at(&mut m, ar(0.0, 0.0, 0.0));
    m.block = block;
    out.push(m);
    // The karts' lamps, in a block with the lamp glass in it.
    let block = palette.len() as u32;
    let shared: Vec<[f32; 4]> = palette.iter().take(crate::models::FIXED_SLOTS).copied().collect();
    palette.extend(shared);
    for (slot, (r, g, b), e) in [(HEADLAMP, (240, 234, 214), 2.6), (TAILLAMP, (230, 34, 24), 3.2)] {
        if let Some(c) = palette.get_mut(block as usize + slot as usize) {
            *c = crate::models::srgb(r, g, b, e);
        }
    }
    for head in [true, false] {
        let mut g = Grid::new();
        kart_lamps(&mut g, head);
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
    if v.spec.model == "atv" {
        atv_instances(v, alpha, base, models, (driver, hands), (pos, rot, shown), out);
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

/// Where a point on the car (world, this step's pose) is drawn. Karts are
/// drawn `KART_SIZE` bigger about the ground under their middle, so a
/// kart's tyre track has to be laid where its drawn tyres are, not its
/// physics wheels; other vehicles are drawn at their physics.
pub fn drawn_at(v: &Vehicle, p: Vec3) -> Vec3 {
    if v.spec.model != "kart" {
        return p;
    }
    let n = v.wheels.len().max(1) as f32;
    let ground = v.wheels.iter().map(|w| w.mount.y - w.t).sum::<f32>() / n - v.spec.wheel_radius;
    let anchor = v.pos() + v.up() * ground;
    anchor + (p - anchor) * KART_SIZE
}

/// A kart's instances: body, wheels on their hubs (no arms or coilovers to
/// show), steering wheel and driver in the kart's lower seat.
#[allow(clippy::too_many_arguments)]
fn kart_instances(v: &Vehicle, alpha: f32, base: usize, models: &[Model], (driver, hands): (bool, bool), colour: usize, (pos, rot, shown): (Vec3, Quat, Quat), out: &mut Vec<(usize, Instance)>) {
    let first = out.len();
    put(out, models, base + KART + colour, pos, shown, 0.0);
    let up = rot * Vec3::Y;
    let steer = v.prev_steer + (v.steer - v.prev_steer) * alpha;
    for w in &v.wheels {
        let t = w.prev_t + (w.t - w.prev_t) * alpha;
        let hub = pos + rot * w.mount - up * t;
        let turn = if w.rear { 0.0 } else { steer };
        let roll = w.prev_roll + wrap(w.roll - w.prev_roll) * alpha;
        // The drawn tyre is bigger than the physics wheel: raise it so its
        // tread still meets the ground, and turn it slower to match.
        let hub = hub + up * (KART_R - v.spec.wheel_radius);
        let roll = roll * v.spec.wheel_radius / KART_R;
        put(out, models, base + KART_WHEEL + colour, hub, rot * Quat::from_rotation_y(turn) * Quat::from_rotation_x(-roll), 0.0);
    }
    let wheel_q = shown * Quat::from_rotation_x(WHEEL_TILT + 0.3) * Quat::from_rotation_z(steer * 4.0);
    let at = pos + shown * (WHEEL_AT + KART_SEAT);
    if hands {
        put(out, models, base + STEER_HANDS, at, wheel_q, crate::ride::NO_SHADOW);
    } else {
        put(out, models, base + STEER, at, wheel_q, 0.0);
    }
    if driver {
        put(out, models, base + KART_DRIVER + colour, pos, shown, if hands { crate::ride::SHADOW_ONLY } else { 0.0 });
    }
    // The lamps, lit as the lights are (lights.rs): the glow's gain is
    // carried less one.
    let lit = super::lights::lit();
    for (model, gain) in [(KART_HEADLAMP, super::lights::head_glow(lit)), (KART_TAILLAMP, super::lights::tail_glow(lit, v.braking))] {
        put(out, models, base + model, pos, shown, crate::ride::NO_SHADOW);
        if let Some((_, inst)) = out.last_mut() {
            inst.glow[0] = gain - 1.0;
        }
    }
    // All of it bigger, about the ground under the kart's middle (its
    // wheels' contact on average), so the tyres still meet the road.
    let n = v.wheels.len().max(1) as f32;
    let ground = v.wheels.iter().map(|w| w.mount.y - (w.prev_t + (w.t - w.prev_t) * alpha)).sum::<f32>() / n - v.spec.wheel_radius;
    let anchor = pos + up * ground;
    for (_, inst) in &mut out[first..] {
        let p = anchor + (Vec3::new(inst.pos_yaw[0], inst.pos_yaw[1], inst.pos_yaw[2]) - anchor) * KART_SIZE;
        inst.pos_yaw[0] = p.x;
        inst.pos_yaw[1] = p.y;
        inst.pos_yaw[2] = p.z;
        inst.info[0] *= KART_SIZE;
    }
}

/// The ATV's instances: body, wheels on their hubs, a swing arm and a
/// coilover to each, the handlebars turning with the front wheels, and the
/// rider.
fn atv_instances(v: &Vehicle, alpha: f32, base: usize, models: &[Model], (driver, hands): (bool, bool), (pos, rot, shown): (Vec3, Quat, Quat), out: &mut Vec<(usize, Instance)>) {
    put(out, models, base + ATV, pos, shown, 0.0);
    let up = rot * Vec3::Y;
    let steer = v.prev_steer + (v.steer - v.prev_steer) * alpha;
    for w in &v.wheels {
        let t = w.prev_t + (w.t - w.prev_t) * alpha;
        let hub = pos + rot * w.mount - up * t;
        let side = if w.left { -1.0 } else { 1.0 };
        let turn = if w.rear { 0.0 } else { steer };
        let roll = w.prev_roll + wrap(w.roll - w.prev_roll) * alpha;
        let hub = hub + up * (ATV_R - v.spec.wheel_radius);
        let roll = roll * v.spec.wheel_radius / ATV_R;
        put(out, models, base + ATV_WHEEL, hub, rot * Quat::from_rotation_y(turn) * Quat::from_rotation_x(-roll), 0.0);
        // The arm from its pivot on the frame out to the hub, and the
        // coilover from under the fender down onto it.
        let pivot = pos + shown * Vec3::new(side * ATV_PIVOT.0, ATV_PIVOT.1, w.mount.z);
        let to_hub = (hub - pivot).normalize_or(rot * Vec3::X * side);
        put(out, models, base + ATV_ARM, pivot, Quat::from_rotation_arc(Vec3::X, to_hub), 0.0);
        let low = pivot + to_hub * (ATV_ARM_LEN * 0.75);
        let top = pos + shown * Vec3::new(side * 0.3, 0.12, w.mount.z + 0.04);
        let axis = low - top;
        put(out, models, base + COIL, top, Quat::from_rotation_arc(Vec3::X, axis.normalize_or(-up)), 0.0);
        put(out, models, base + SHAFT, low, Quat::from_rotation_arc(Vec3::X, (-axis).normalize_or(up)), 0.0);
    }
    let bars_q = shown * Quat::from_rotation_y(steer);
    let at = pos + shown * ATV_BARS_AT;
    if hands {
        put(out, models, base + ATV_BARS_HANDS, at, bars_q, crate::ride::NO_SHADOW);
    } else {
        put(out, models, base + ATV_BARS, at, bars_q, 0.0);
    }
    if driver {
        put(out, models, base + ATV_RIDER, pos, shown, if hands { crate::ride::SHADOW_ONLY } else { 0.0 });
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
