//! Realistic gun models (see mod.rs): the pistols, shotguns and machine guns.
//!
//! The pistols are drawn at about 0.65-0.95 cm a voxel, so a slide is four
//! voxels across and a grip a dozen tall. The long guns are 3.5-4 cm a voxel:
//! a barrel is a rod of radius about half a voxel and a stock two voxels
//! across. Round parts are rods on voxel centres or boundaries (x = 16, y at
//! a half) so that their voxels come out two across and the tube drawn over
//! them is round.

use super::c::*;
use super::Marks;
use crate::models::Grid;

/// A slice of a long gun's furniture, two voxels across (x 15..17), one deep
/// at `z`, from `y0` up to `y1` rounded.
fn slab(g: &mut Grid, z: i32, y0: f32, y1: f32, c: u8) {
    g.fill([15, y0.round() as i32, z], [17, y1.round() as i32, z + 1], c);
}

/// A rod along z at x = 16.
fn tube(g: &mut Grid, y: f32, z0: f32, z1: f32, r: f32, c: u8) {
    g.rod([16.0, y, z0], [16.0, y, z1], r, c);
}

/// A small upright rod: a sight post, a trigger, a bead.
fn post(g: &mut Grid, x: f32, z: f32, y0: f32, y1: f32, r: f32, c: u8) {
    g.rod([x, y0, z], [x, y1, z], r, c);
}

/// Polymer service pistol, 19 cm: a black slide on a tan frame with a
/// stippled grip, an accessory rail under the dust cover and a squared
/// trigger guard.
pub fn pistol_polymer(g: &mut Grid) -> Marks {
    // Frame and dust cover, with rail lugs underneath.
    g.fill([14, 18, 3], [18, 21, 29], TAN);
    for z in [3, 5, 7] {
        g.fill([14, 17, z], [18, 18, z + 1], TAN);
    }
    g.fill([14, 17, 27], [18, 18, 30], TAN);
    // Trigger guard, squared at the front, and the trigger.
    g.fill([15, 13, 8], [17, 18, 9], TAN);
    g.fill([15, 13, 9], [17, 14, 19], TAN);
    g.fill([15, 16, 12], [17, 18, 13], BLACK);
    g.fill([15, 14, 13], [17, 16, 14], BLACK);
    // The grip, raked back, stippled on its sides.
    for y in 5..18 {
        let d = (18 - y) as f32;
        let zf = (17.0 + d * 0.3).round() as i32;
        let zb = (27.5 + d * 0.15).round() as i32;
        g.fill([14, y, zf], [18, y + 1, zb], TAN);
        if (7..16).contains(&y) {
            for z in zf + 1..zb - 1 {
                if (y + z) % 2 == 0 {
                    g.set(14, y, z, GRIP);
                    g.set(17, y, z, GRIP);
                }
            }
        }
    }
    // Magazine base plate.
    g.fill([14, 4, 21], [18, 5, 30], BLACK);
    // The slide, its nose stepped down, the bore, the ejection port on the
    // right with the barrel hood showing, and cocking serrations.
    g.fill([14, 21, 2], [18, 26, 30], DARK);
    g.fill([14, 21, 1], [18, 25, 2], DARK);
    g.fill([15, 22, 1], [17, 24, 2], BLACK);
    g.fill([17, 23, 12], [18, 25, 18], STEEL);
    for z in [24, 26, 28] {
        g.fill([14, 22, z], [15, 25, z + 1], BLACK);
        g.fill([17, 22, z], [18, 25, z + 1], BLACK);
    }
    // Sights, and the slide stop on the left.
    g.fill([15, 26, 3], [17, 27, 5], BLACK);
    g.fill([14, 26, 26], [15, 27, 28], BLACK);
    g.fill([17, 26, 26], [18, 27, 28], BLACK);
    g.fill([13, 20, 14], [14, 21, 18], BLACK);
    Marks { grip: [16.0, 4.0, 25.0], muzzle: [16.0, 23.0, 1.0], butt: [16.0, 23.0, 30.0], fore: None }
}



/// Pump-action, 105 cm: a barrel over a magazine tube, a grooved walnut
/// pump on the tube, a steel receiver and a walnut stock with a curved
/// wrist.
pub fn sg_pump(g: &mut Grid) -> Marks {
    tube(g, 16.5, 1.0, 16.0, 0.5, BARREL);
    post(g, 16.0, 1.5, 17.0, 17.6, 0.25, BRASS);
    tube(g, 15.5, 3.0, 15.5, 0.42, DARK);
    tube(g, 15.5, 2.5, 3.0, 0.5, STEEL);
    // The pump, ridged.
    tube(g, 15.5, 6.0, 12.5, 0.8, WALNUT);
    for i in 0..4 {
        let z = 6.6 + i as f32 * 1.6;
        tube(g, 15.5, z, z + 0.6, 0.95, WALNUT);
    }
    // Receiver, port on the right, trigger plate below.
    g.fill([15, 15, 15], [17, 18, 22], DARK);
    g.fill([16, 16, 17], [17, 17, 19], STEEL);
    g.fill([15, 14, 17], [17, 15, 22], DARK);
    // Trigger guard and trigger.
    g.fill([15, 13, 18], [17, 14, 22], DARK);
    g.fill([15, 13, 18], [17, 15, 19], DARK);
    post(g, 16.0, 19.8, 13.7, 14.9, 0.25, STEEL);
    // Stock.
    for z in 22..30 {
        let t = (z - 22) as f32;
        let top = 18.0 - t * 0.1;
        let bot = if z < 24 { 14.0 } else { 14.0 - (t - 1.0) * 0.32 };
        slab(g, z, bot, top, WALNUT);
    }
    g.fill([15, 12, 30], [17, 17, 31], BLACK);
    Marks { grip: [16.0, 14.0, 23.0], muzzle: [16.0, 16.5, 1.0], butt: [16.0, 15.0, 31.0], fore: Some([16.0, 14.5, 9.0]) }
}




/// General-purpose MG with a perforated barrel jacket, 118 cm: a muzzle
/// booster, the jacket in line with the receiver and stock, a bipod folded
/// under it, a belt of brass hanging from the left, and black furniture.
pub fn mg_jacket(g: &mut Grid) -> Marks {
    // Muzzle booster.
    tube(g, 18.0, 1.0, 2.0, 0.75, DARK);
    tube(g, 18.0, 2.0, 3.5, 0.95, DARK);
    // The jacket, holes staggered along it.
    g.fill([15, 17, 3], [17, 19, 15], DARK);
    for z in 4..14 {
        let y = if z % 2 == 0 { 18 } else { 17 };
        g.fill([15, y, z], [17, y + 1, z + 1], BLACK);
    }
    post(g, 16.0, 4.5, 19.0, 20.4, 0.3, BLACK);
    // Bipod folded back under the jacket.
    g.fill([15, 16, 4], [17, 17, 5], DARK);
    for x in [15.5, 16.5] {
        g.rod([x, 16.5, 4.5], [x, 16.5, 12.5], 0.3, STEEL);
    }
    // Receiver, top cover and rear sight.
    g.fill([15, 16, 15], [17, 20, 23], DARK);
    g.fill([15, 20, 15], [17, 21, 21], DARK);
    g.fill([15, 21, 19], [17, 22, 20], BLACK);
    // Feed tray and the belt hanging out of the left side: cartridges
    // pointing forward, a link between each.
    g.fill([14, 18, 15], [15, 20, 19], DARK);
    for i in 0..8 {
        let y = 17 - i;
        let x = 13 - i / 4;
        if i % 2 == 0 {
            g.fill([x, y, 16], [x + 1, y + 1, 18], BRASS);
            g.set(x, y, 15, COPPER);
        } else {
            g.fill([x, y, 16], [x + 1, y + 1, 18], DARK);
        }
    }
    // Guard, trigger and grip.
    g.fill([15, 14, 16], [17, 15, 19], DARK);
    g.fill([15, 14, 16], [17, 16, 17], DARK);
    post(g, 16.0, 17.8, 14.8, 15.9, 0.22, STEEL);
    for y in 12..16 {
        let off = ((16 - y) as f32 * 0.45).round() as i32;
        g.fill([15, y, 19 + off], [17, y + 1, 21 + off], BLACK);
    }
    // Stock, its top in line with the receiver.
    for z in 23..30 {
        let t = (z - 23) as f32;
        slab(g, z, 16.5 - t * 0.2, 20.0 - t * 0.1, BLACK);
    }
    g.fill([15, 15, 30], [17, 19, 31], DARK);
    Marks { grip: [16.0, 12.0, 21.0], muzzle: [16.0, 18.0, 1.0], butt: [16.0, 17.0, 31.0], fore: Some([16.0, 16.5, 13.0]) }
}

