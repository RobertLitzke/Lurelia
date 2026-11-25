//! Arena gun models, redrawn (see mod.rs): the digger (a pneumatic blast hammer), the sidearm and the
//! goo gun, three options each.
//!
//! The digger is drawn at about 2.3 cm a voxel, the sidearm at 1.6 and the
//! goo gun at 2.8. Bodies are four voxels across (x 14..18) or two (15..17)
//! about the centre line x = 16; round parts are rods on x = 16 at a
//! half-voxel height so their voxels come out two across.

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

/// A rod straight across x at (y, z): a cross handle, a round lying sideways.
fn across(g: &mut Grid, y: f32, z: f32, x0: f32, x1: f32, r: f32, c: u8) {
    g.rod([x0, y, z], [x1, y, z], r, c);
}

/// A box, x0..x1 by y0..y1 by z0..z1 (half-open).
fn block(g: &mut Grid, x: [i32; 2], y: [i32; 2], z: [i32; 2], c: u8) {
    g.fill([x[0], y[0], z[0]], [x[1], y[1], z[1]], c);
}

/// A body x0..x1 across whose bottom and top vary along z: a stock, a sloped
/// nose. `bot` and `top` give the edges at each slice's middle.
fn profile(g: &mut Grid, x: [i32; 2], z0: i32, z1: i32, bot: impl Fn(f32) -> f32, top: impl Fn(f32) -> f32, c: u8) {
    for z in z0..z1 {
        let m = z as f32 + 0.5;
        g.fill([x[0], bot(m).round() as i32, z], [x[1], top(m).round() as i32, z + 1], c);
    }
}

/// Straight line between two (z, y) points, as a function of z.
fn lerp(z0: f32, y0: f32, z1: f32, y1: f32) -> impl Fn(f32) -> f32 {
    move |z| y0 + (y1 - y0) * ((z - z0) / (z1 - z0)).clamp(0.0, 1.0)
}

/// A pistol grip raked back, x across, from `top` down to `bot`: at the top
/// its front is at `zf` and its back at `zb`, both moving back `rake` a
/// voxel for each voxel down. Every other row of its sides is `tex`, for a
/// finger grooves. Returns the grip mark, the bottom centre.
#[allow(clippy::too_many_arguments)]
fn pistol_grip(g: &mut Grid, x: [i32; 2], top: i32, bot: i32, zf: f32, zb: f32, rake: f32, c: u8, tex: u8) -> [f32; 3] {
    for y in bot..top {
        let d = (top - y) as f32;
        let (a, b) = ((zf + d * rake).round() as i32, (zb + d * rake).round() as i32);
        g.fill([x[0], y, a], [x[1], y + 1, b], c);
        if y > bot + 1 && y < top - 1 && (top - y) % 2 == 0 {
            g.fill([x[0], y, a], [x[1], y + 1, b], tex);
        }
    }
    g.fill([x[0], bot, (zf + (top - bot) as f32 * rake).round() as i32 - 1], [x[1], bot + 1, (zb + (top - bot) as f32 * rake).round() as i32 + 1], c);
    [16.0, bot as f32, (zf + zb) * 0.5 + (top - bot) as f32 * rake]
}

/// A trigger guard of rods: down from the body at z0, along the bottom at
/// height y to the grip at z1, with a trigger hanging inside it.
fn guard(g: &mut Grid, top: f32, y: f32, z0: f32, z1: f32, c: u8, trigger: u8) {
    bar(g, top, z0, y, z0 + 0.6, 0.5, c);
    tube(g, y, z0 + 0.6, z1, 0.5, c);
    bar(g, top - 0.2, z0 + (z1 - z0) * 0.45, y + 1.0, z0 + (z1 - z0) * 0.62, 0.5, trigger);
}

/// A spade lying flat under a nozzle, its face up, from the cutting edge at
/// z = 0 back: `half` gives its half width at each slice. The sides stand a
/// voxel proud of the middle, so it reads dished; the front slice is `edge`.
fn spade(g: &mut Grid, y: i32, half: &[i32], c: u8, edge: u8) {
    for (z, &h) in half.iter().enumerate() {
        let z = z as i32;
        if h == 0 {
            continue;
        }
        let col = if z == 0 { edge } else { c };
        g.fill([16 - h, y, z], [16 + h, y + 1, z + 1], col);
        if h > 1 {
            for x in [16 - h, 15 + h] {
                g.fill([x, y + 1, z], [x + 1, y + 2, z + 1], if z == 0 { edge } else { c });
            }
        }
    }
}

// ---- the digger: a pneumatic blast hammer with a spade under its head ----


/// Blast-nozzle ram: a flared chrome blast bell glowing red in its throat on
/// a short ram with a warning collar, a fat purple piston drum with a white
/// band and lit exhaust slits, an orange carry handle over it, an orange
/// pressure ball for a butt with a gauge on each side, and a narrow pointed
/// navy spade under the bell.
pub fn digger(g: &mut Grid) -> Marks {
    let y = 16.5;
    // The bell and the ram behind it.
    tube(g, y, 0.0, 0.9, 1.4, GLOW_RED);
    tube(g, y, 0.4, 1.6, 2.4, CHROME);
    tube(g, y, 1.6, 3.0, 1.8, CHROME);
    tube(g, y, 3.0, 4.0, 2.0, BLACK);
    tube(g, y, 4.0, 6.0, 1.1, CHROME);
    tube(g, y, 4.8, 5.4, 1.3, ORANGE);
    // The piston drum, its rims, band and exhaust slits.
    tube(g, y, 7.0, 16.0, 3.0, GUNMETAL);
    tube(g, y, 6.0, 7.0, 3.2, BLACK);
    tube(g, y, 16.0, 17.0, 3.2, BLACK);
    tube(g, y, 10.0, 11.0, 3.1, TAN);
    for x in [12, 19] {
        block(g, [x, x + 1], [16, 17], [12, 15], GLOW_YELLOW);
    }
    // Carry handle over the drum.
    for z in [8.5, 14.5] {
        bar(g, 19.0, z, 22.5, z, 0.75, BLACK);
    }
    tube(g, 22.5, 8.0, 15.0, 0.9, ORANGE);
    // Neck back to the pressure ball, its band and gauges, and the pad.
    tube(g, 15.5, 17.0, 22.5, 1.2, GUNMETAL);
    g.ellipsoid([16.0, 15.5, 26.5], [3.0, 3.0, 4.3], ORANGE);
    tube(g, 15.5, 26.0, 27.0, 3.1, BLACK);
    for x in [12, 19] {
        block(g, [x, x + 1], [16, 18], [24, 26], TAN);
        g.set(x, 17, 25, ORANGE);
    }
    block(g, [14, 18], [13, 18], [30, 31], BLACK);
    // Grip, guard, and a front grip under the drum.
    block(g, [15, 17], [13, 15], [17, 21], GUNMETAL);
    let grip = pistol_grip(g, [15, 17], 13, 8, 17.5, 20.5, 0.3, BLACK, GUNMETAL);
    guard(g, 13.2, 10.5, 15.0, 18.5, ORANGE, CHROME);
    bar(g, 13.5, 9.0, 9.5, 9.3, 1.0, BLACK);
    tube(g, 9.3, 8.4, 10.1, 1.1, ORANGE);
    // The spade under the bell, welded back to the drum.
    spade(g, 12, &[1, 2, 2, 3, 3, 3, 2, 2], BLACK, CHROME);
    bar(g, 13.0, 6.0, 14.0, 8.0, 0.8, GUNMETAL);
    Marks { grip, muzzle: [16.0, y, 0.0], butt: [16.0, 15.5, 31.0], fore: Some([16.0, 10.0, 9.2]) }
}


// ---- the sidearm: a stubby bullpup, its magazine along the top ----

/// Rounds lying across a top magazine from z0 to z1, every other voxel, at
/// height y: brass cases with copper noses to the left, showing through
/// the magazine's sides. `shell` fills the gaps between them.
fn rounds(g: &mut Grid, y: i32, z0: i32, z1: i32, shell: u8) {
    for z in z0..z1 {
        if (z - z0) % 2 == 0 {
            block(g, [14, 18], [y, y + 1], [z, z + 1], BRASS);
            g.set(14, y, z, COPPER);
        } else {
            block(g, [14, 18], [y, y + 1], [z, z + 1], shell);
        }
    }
}



/// Yellow and purple box of a bullpup with an open-sided top magazine:
/// the rounds' brass shows the length of it under a purple top plate, a
/// ventilated purple shroud ending in a cyan-lit muzzle ring, an angled
/// fore grip, an aperture rear sight and a cyan round counter.
pub fn sidearm(g: &mut Grid) -> Marks {
    let y = 14.5;
    // Muzzle ring and the ventilated shroud.
    tube(g, y, 0.0, 1.0, 1.4, GLOW_CYAN);
    tube(g, y, 0.0, 1.2, 0.6, BLACK);
    tube(g, y, 1.0, 7.0, 1.3, GUNMETAL);
    for z in [2, 4] {
        for x in [14, 17] {
            g.set(x, 14, z, BLACK);
        }
    }
    // Upper body, slanted at the nose.
    block(g, [14, 18], [11, 15], [6, 31], TAN);
    block(g, [14, 18], [15, 17], [7, 31], TAN);
    // Magazine: feed lips down the middle, the rounds across it, the top
    // plate on a post at each end.
    block(g, [15, 17], [17, 19], [8, 28], BLACK);
    rounds(g, 17, 9, 27, BLACK);
    block(g, [14, 18], [17, 20], [8, 9], GUNMETAL);
    block(g, [14, 18], [17, 20], [27, 28], GUNMETAL);
    block(g, [14, 18], [19, 20], [8, 28], GUNMETAL);
    block(g, [15, 17], [20, 21], [12, 15], GLOW_CYAN);
    // Sights: a front post and a rear aperture.
    block(g, [15, 17], [20, 22], [9, 10], GUNMETAL);
    block(g, [14, 18], [20, 23], [25, 27], GUNMETAL);
    block(g, [15, 17], [21, 22], [25, 27], BLACK);
    // Angled fore grip under the nose.
    bar(g, 11.0, 7.5, 5.5, 6.0, 1.1, GUNMETAL);
    tube(g, 5.0, 5.0, 7.0, 0.9, BLACK);
    // Lower: the pistol grip, its guard, and the stock stepping down to a
    // black pad.
    let grip = pistol_grip(g, [14, 18], 11, 5, 15.0, 18.0, 0.2, GUNMETAL, BLACK);
    guard(g, 11.0, 8.0, 11.0, 15.5, GUNMETAL, CHROME);
    profile(g, [14, 18], 19, 30, lerp(19.0, 11.0, 26.0, 7.0), |_| 11.0, TAN);
    block(g, [14, 18], [7, 17], [30, 31], BLACK);
    block(g, [14, 18], [10, 11], [19, 30], GUNMETAL);
    block(g, [13, 14], [13, 15], [8, 11], BLACK);
    Marks { grip, muzzle: [16.0, y, 0.0], butt: [16.0, 12.0, 31.0], fore: Some([16.0, 5.5, 6.0]) }
}

// ---- the goo gun: a tank of glowing sludge on a launcher ----

/// Jar launcher: a purple body with lime bands and a lime flared muzzle
/// dripping goo, a caged jar of glowing goo standing upright on the
/// receiver under a red lid, a ribbed pump under the barrel, and a purple
/// skeleton stock.
pub fn goo_gun(g: &mut Grid) -> Marks {
    let y = 15.5;
    // Muzzle: a lime bell with goo in its mouth and a drip off the lip.
    tube(g, y, 0.0, 0.8, 1.7, GOO);
    tube(g, y, 0.5, 1.6, 2.6, LIME);
    tube(g, y, 1.6, 2.6, 2.0, LIME);
    g.ellipsoid([16.0, 12.6, 1.0], [0.8, 1.3, 0.8], GOO);
    // Barrel, banded.
    tube(g, y, 2.6, 12.0, 1.5, GUNMETAL);
    for z in [5.0, 8.5] {
        tube(g, y, z, z + 1.0, 1.75, LIME);
    }
    // Pump under the barrel, ribbed.
    tube(g, 12.5, 4.0, 10.5, 1.1, BLACK);
    for i in 0..4 {
        let z = 4.6 + i as f32 * 1.6;
        tube(g, 12.5, z, z + 0.7, 1.3, BLACK);
    }
    // Receiver, with a lime stripe.
    block(g, [14, 18], [12, 18], [11, 23], GUNMETAL);
    block(g, [14, 18], [12, 13], [11, 23], LIME);
    // The jar: goo standing in a cage of chrome bars between a red collar
    // and a red lid with a wheel on it.
    g.rod([16.0, 18.0, 16.5], [16.0, 23.5, 16.5], 2.5, GOO);
    g.rod([16.0, 17.6, 16.5], [16.0, 19.0, 16.5], 2.9, BLACK);
    g.rod([16.0, 23.3, 16.5], [16.0, 24.7, 16.5], 2.9, BLACK);
    g.rod([16.0, 24.7, 16.5], [16.0, 25.5, 16.5], 1.2, BLACK);
    across(g, 25.5, 16.5, 13.5, 18.5, 0.5, BLACK);
    for (x, z) in [(13.5, 14.5), (18.5, 14.5), (13.5, 18.5), (18.5, 18.5)] {
        g.rod([x, 19.0, z], [x, 23.3, z], 0.5, CHROME);
    }
    // Grip and guard.
    let grip = pistol_grip(g, [15, 17], 12, 7, 19.5, 22.5, 0.3, BLACK, GUNMETAL);
    guard(g, 12.0, 10.0, 16.0, 20.0, LIME, CHROME);
    // Skeleton stock: a top bar, a lower bar, and the butt plate.
    bar(g, 16.5, 23.0, 16.0, 30.0, 0.85, GUNMETAL);
    bar(g, 12.5, 23.0, 10.5, 30.0, 0.85, GUNMETAL);
    bar(g, 15.8, 26.0, 11.2, 27.0, 0.6, LIME);
    block(g, [15, 17], [9, 18], [30, 31], BLACK);
    Marks { grip, muzzle: [16.0, y, 0.0], butt: [16.0, 13.5, 31.0], fore: Some([16.0, 12.5, 7.0]) }
}


