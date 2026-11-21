//! The gun in the player's hands: a voxel model held in the camera's frame.
//!
//! These are drawn as ordinary entity instances, not as a HUD overlay, so
//! they take the light of the room they are in and move with the view.
//! `src/shooter.rs` decides where to hold one and how it bobs, sways and
//! kicks; this file only builds the shapes.
//!
//! Model space: x right, y up, the muzzle toward -z, the pivot at the grip's
//! foot (16, 0, 16) as every model here has it. The minigun's barrels are a
//! model of their own with the pivot on their axis, so they can spin without
//! swinging the gun round with them.

use crate::models::{srgb, Grid, Model};
use glam::Vec3;

/// Palette slots 18..38 are the guns': 18..22 are the plastics, free below
/// the built-in models' colours, and the rest runs up to the bots'
/// (brain/avatar.rs takes 38..48).
///
/// Two of these are toy colours, which is not a joke at the arsenal's
/// expense: one weapon in it is a bright plastic thing among iron ones, and
/// that reads at a glance in a way no amount of grey would.
const PLASTIC: u8 = 18;
const PLASTIC_B: u8 = 19;
const FOAM: u8 = 20;
const PALE: u8 = 21;
const STEEL: u8 = 22;
const DARK: u8 = 23;
const BARREL: u8 = 24;
const OLIVE: u8 = 25;
const RUST: u8 = 26;
const BRASS: u8 = 27;
const GRIP: u8 = 28;
const HOT: u8 = 29;
/// The energy weapons' own colours. 32 is the look page's swatch, so the
/// block steps over it.
const PLASMA: u8 = 30;
const GOO: u8 = 31;
const PULSE: u8 = 33;
const EDGE: u8 = 34;
const BLACK: u8 = 35;
const COPPER: u8 = 36;
const GLASS: u8 = 37;

pub fn palette(p: &mut [[f32; 4]]) {
    p[PLASTIC as usize] = srgb(238, 104, 26, 0.0);
    p[PLASTIC_B as usize] = srgb(48, 126, 206, 0.0);
    p[FOAM as usize] = srgb(226, 216, 112, 0.0);
    p[PALE as usize] = srgb(226, 228, 230, 0.0);
    p[STEEL as usize] = srgb(118, 122, 126, 0.0);
    p[DARK as usize] = srgb(54, 56, 60, 0.0);
    p[BARREL as usize] = srgb(86, 90, 94, 0.0);
    p[OLIVE as usize] = srgb(72, 80, 60, 0.0);
    p[RUST as usize] = srgb(126, 76, 46, 0.0);
    p[BRASS as usize] = srgb(170, 140, 68, 0.0);
    p[GRIP as usize] = srgb(40, 38, 40, 0.0);
    // The muzzle flash is the only thing in the game that lights itself this
    // hard; it is on screen for three frames.
    p[HOT as usize] = srgb(255, 108, 18, 2.4);
    // The energy weapons light themselves, which is most of what tells them
    // apart from the iron ones in a dark hall.
    p[PLASMA as usize] = srgb(96, 158, 240, 0.55);
    p[GOO as usize] = srgb(132, 206, 44, 0.4);
    p[PULSE as usize] = srgb(150, 240, 170, 0.5);
    p[EDGE as usize] = srgb(196, 202, 208, 0.0);
    p[BLACK as usize] = srgb(28, 29, 32, 0.0);
    p[COPPER as usize] = srgb(168, 104, 56, 0.0);
    p[GLASS as usize] = srgb(104, 134, 148, 0.25);
}

/// Model voxels to the metre: 32 voxels is 0.7 m, which held at arm's
/// length fills about a third of the screen's width.
const VOX: f32 = 1.0 / 58.0;

/// The shapes a weapon file may ask for by name (`"view"`). The minigun is
/// first because it is the one with turning barrels, which `MINIGUN` names.
pub const SHAPES: [&str; 10] = ["minigun", "launcher", "scrap", "sidearm", "digger", "goo", "bore", "pulse", "skipjack", "sniper"];

/// The one shape with a barrel assembly of its own that turns.
pub const MINIGUN: usize = 0;

/// How high up its own grid the barrel assembly's pivot sits, in model
/// voxels: it is on the axis they spin about, not at the grip like every
/// other model here, so anybody placing one beside the gun has to lift it
/// by this much.
pub const BARREL_AXIS: f32 = 13.0;

/// Model voxels to the metre, for whoever places one.
pub const VOXEL: f32 = VOX;

/// What each shape is scaled by, in `SHAPES` order.
///
/// Every gun is drawn inside the same 32 cube, which is a convenience for
/// drawing them and a lie about the arsenal: a sidearm came out the size of
/// a rocket launcher, and ten weapons that are all the same box on screen
/// are ten weapons you cannot tell apart at a glance. The grid stays one
/// size; what leaves it does not.
const SCALE: [f32; SHAPES.len()] = [
    1.06, // minigun: a lump of a thing
    1.14, // launcher: the longest tube in the arsenal
    0.96, // scrap gun: a slab of a forge
    0.62, // sidearm: a stubby bullpup, still the smallest thing carried
    0.94, // digger
    0.94, // goo gun
    1.06, // bore
    1.0,  // pulse gun
    1.0,  // skipjack
    1.18, // sniper rifle: the longest thing anybody carries
];

/// The point of each shape's grid the shot leaves from.
pub fn muzzle(shape: usize) -> Vec3 {
    match SHAPES.get(shape).copied().unwrap_or("minigun") {
        "scrap" => Vec3::new(16.0, 12.0, 1.0),
        "sidearm" => Vec3::new(16.0, 14.0, 0.0),
        // The Digger lays its blast on whatever the nozzle is against.
        "digger" => Vec3::new(16.0, 14.0, 1.0),
        "goo" => Vec3::new(16.0, 12.0, 2.0),
        "bore" | "sniper" => Vec3::new(16.0, 14.0, 0.0),
        "pulse" => Vec3::new(16.0, 13.0, 2.0),
        "skipjack" => Vec3::new(16.0, 14.0, 2.0),
        _ => Vec3::new(16.0, 13.0, 1.0),
    }
}

/// Index into `models()` of the shape called `name`, if there is one.
pub fn shape_of(name: &str) -> Option<usize> {
    SHAPES.iter().position(|s| *s == name)
}

/// The same, falling back to the first shape, for a weapon file that names
/// none: better a generic gun in hand than nothing at all.
pub fn shape(name: &str) -> usize {
    shape_of(name).unwrap_or(0)
}

/// The little bodies drawn for a shell in flight (`"look"` in a fire mode).
pub const SHELLS: [&str; 7] = ["rocket", "grenade", "shard", "goo", "disc", "bolt", "block"];

/// Index into `models()` of the shell body called `name`, if there is one.
pub fn shell_of(name: &str) -> Option<usize> {
    SHELLS.iter().position(|s| *s == name).map(|i| SHELL + i)
}

/// Which body a fire mode's shells are drawn as. A mode may name one; left
/// out, it is read off what the shell does, which is right nearly always: a
/// bouncing shell is a grenade, one that bursts is a rocket, and one that
/// does neither is a chip of scrap.
pub fn shell_look(f: &crate::library::FireDef) -> usize {
    if let Some(i) = shell_of(&f.look) {
        return i;
    }
    match (f.bounces > 0, f.splash.is_some()) {
        (true, _) => SHELL + 1,
        (false, true) => SHELL,
        (false, false) => SHELL + 2,
    }
}

/// One per name in `SHAPES`, then the spinning barrels, the muzzle flash and
/// one per name in `SHELLS`.
pub const BARRELS: usize = SHAPES.len();
pub const FLASH: usize = SHAPES.len() + 1;
/// The first of the shell bodies.
pub const SHELL: usize = SHAPES.len() + 2;
/// What a pad of rounds is drawn as, one look per kind of ammunition, so a
/// pad of rockets can be told from a pad of bullets across a hall without
/// reading anything. Guns that share a store (`ammo_type`) share a look.
pub const AMMOS: [&str; 8] = ["bullets", "rockets", "scrap", "goo", "bore", "cells", "discs", "sniper"];
/// The first of the ammunition looks.
pub const AMMO: usize = SHELL + SHELLS.len();
/// How many models `models()` returns, so whatever is appended after them
/// knows where it starts.
pub const COUNT: usize = AMMO + AMMOS.len();

/// Ammunition looks are a little finer than the guns: a can of bullets is a
/// hand across, and it is held up on its pad rather than at arm's length.
const AMMO_VOX: f32 = 1.0 / 44.0;

/// Which look a pad of a weapon's rounds is drawn as: its `ammo_type` where
/// that names one, else read off the gun's own shape.
pub fn ammo_look(w: &crate::library::WeaponDef) -> usize {
    let by_type = w.ammo_type.as_deref().and_then(|t| AMMOS.iter().position(|a| *a == t));
    let by_view = match w.view.as_str() {
        "launcher" => "rockets",
        "scrap" => "scrap",
        "goo" => "goo",
        "bore" => "bore",
        "pulse" => "cells",
        "skipjack" => "discs",
        "sniper" => "sniper",
        _ => "bullets",
    };
    AMMO + by_type.unwrap_or_else(|| AMMOS.iter().position(|a| *a == by_view).unwrap_or(0))
}

/// Shell bodies are a tenth the guns' scale: a rocket is about 0.5 m long
/// and a scrap shard a hand's width, which is what they have to be for one
/// seen edge-on at thirty metres to read as a speck and not a crate.
const SHELL_VOX: f32 = 1.0 / 56.0;

/// Draw one shape by name into a grid. Named rather than inline so that
/// anything measuring the arsenal works from the same shapes the game does.
fn build_shape(name: &str, g: &mut Grid) {
    match name {
        "minigun" => minigun(g),
        "launcher" => launcher(g),
        "sidearm" => sidearm(g),
        "digger" => digger(g),
        "goo" => goo_gun(g),
        "bore" => bore(g),
        "pulse" => pulse(g),
        "skipjack" => skipjack(g),
        "sniper" => sniper(g),
        _ => scrap(g),
    }
}

pub fn models() -> Vec<Model> {
    let mut out = Vec::new();
    for (i, name) in SHAPES.iter().enumerate() {
        let mut g = Grid::new();
        build_shape(name, &mut g);
        out.push(g.build(VOX * SCALE[i], 0.2, 0.5));
    }
    let mut b = Grid::new();
    barrels(&mut b);
    let mut m = b.build(VOX, 0.2, 0.5);
    // Its own axis, so a spin about z turns the barrels and nothing else.
    m.pivot = Vec3::new(16.0, BARREL_AXIS, 16.0);
    out.push(m);
    let mut f = Grid::new();
    flash(&mut f);
    // Half again the guns' scale: a flash the size of a breech is a speck on
    // screen, and one much bigger than this is all you can see.
    let mut m = f.build(VOX * 1.55, 0.2, 0.5);
    m.pivot = Vec3::new(16.0, 16.0, 16.0);
    out.push(m);
    for name in SHELLS {
        let mut g = Grid::new();
        match name {
            "rocket" => rocket(&mut g),
            "grenade" => grenade(&mut g),
            "goo" => goo_blob(&mut g),
            "disc" => disc(&mut g),
            "bolt" => bolt(&mut g),
            "block" => block(&mut g),
            _ => shard(&mut g),
        }
        let mut m = g.build(SHELL_VOX, 0.1, 0.2);
        // Middle of the body, so a shell turns about itself in flight.
        m.pivot = Vec3::new(16.0, 16.0, 16.0);
        out.push(m);
    }
    for name in AMMOS {
        let mut g = Grid::new();
        ammo(name, &mut g);
        // The pivot is the bottom middle of the grid, so a look is placed by
        // the point it stands on.
        out.push(g.build(AMMO_VOX, 0.2, 0.4));
    }
    out
}

/// An upright round post: `rod` stood on end, for the things that stand.
fn post(g: &mut Grid, cx: f32, cz: f32, y0: i32, y1: i32, r: f32, c: u8) {
    for y in y0..y1 {
        for z in (cz - r).floor() as i32..=(cz + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                let (dx, dz) = (x as f32 + 0.5 - cx, z as f32 + 0.5 - cz);
                if dx * dx + dz * dz <= r * r {
                    g.set(x, y, z, c);
                }
            }
        }
    }
}

/// One ammunition look by name (`AMMOS`), standing on the bottom of the
/// grid. Each is the thing its gun eats, in the gun's own colours, with
/// something in it that lights itself so it reads at the end of a dark hall.
fn ammo(name: &str, g: &mut Grid) {
    match name {
        // Rockets: two of them stood on their fins, nose up.
        "rockets" => {
            for cx in [12.5f32, 19.5] {
                post(g, cx, 16.0, 3, 20, 2.6, OLIVE);
                post(g, cx, 16.0, 11, 13, 3.0, DARK);
                g.ellipsoid([cx, 21.0, 16.0], [2.4, 3.6, 2.4], RUST);
                post(g, cx, 16.0, 0, 3, 1.8, HOT);
                for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let (x, z) = ((cx + dx * 3.2) as i32, (16.0 + dz * 3.2) as i32);
                    g.fill([x, 2, z], [x + 1, 7, z + 1], STEEL);
                }
            }
        }
        // Scrap: a hopper of rust with hot chips heaped over its rim.
        "scrap" => {
            g.fill([9, 0, 11], [23, 9, 21], RUST);
            g.fill([10, 9, 12], [22, 10, 20], DARK);
            g.fill([9, 3, 10], [23, 5, 22], STEEL);
            g.fill([11, 10, 13], [15, 13, 16], HOT);
            g.fill([16, 10, 14], [20, 14, 18], BRASS);
            g.fill([13, 10, 17], [18, 12, 20], HOT);
            g.fill([18, 12, 12], [21, 15, 15], RUST);
        }
        // Sludge: a glass jar of it, lidded.
        "goo" => {
            post(g, 16.0, 16.0, 0, 16, 5.5, GLASS);
            post(g, 16.0, 16.0, 1, 13, 4.6, GOO);
            post(g, 16.0, 16.0, 16, 19, 4.4, DARK);
            post(g, 16.0, 16.0, 19, 20, 2.4, STEEL);
        }
        // The bore's feed: a core of cutting light in a steel cage.
        "bore" => {
            post(g, 16.0, 16.0, 0, 3, 5.0, DARK);
            post(g, 16.0, 16.0, 3, 19, 3.0, PLASMA);
            post(g, 16.0, 16.0, 19, 22, 5.0, DARK);
            for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                post(g, 16.0 + dx * 4.0, 16.0 + dz * 4.0, 3, 19, 0.9, STEEL);
            }
        }
        // A cell for the pulse gun: a dark can with lit windows and copper
        // at its ends.
        "cells" => {
            post(g, 16.0, 16.0, 0, 18, 4.5, DARK);
            for y in [4, 9, 14] {
                post(g, 16.0, 16.0, y, y + 2, 4.9, PULSE);
            }
            post(g, 16.0, 16.0, 18, 20, 2.6, COPPER);
            post(g, 16.0, 16.0, 20, 22, 1.4, COPPER);
        }
        // Razor discs, a stack of them on a bright plastic spindle.
        "discs" => {
            for k in 0..4 {
                let y = 2 + 3 * k;
                for z in 4..28 {
                    for x in 4..28 {
                        let (dx, dz) = (x as f32 + 0.5 - 16.0, z as f32 + 0.5 - 16.0);
                        let r = (dx * dx + dz * dz).sqrt();
                        if (3.0..=9.0).contains(&r) {
                            g.fill([x, y, z], [x + 1, y + 2, z + 1], if r > 7.5 { EDGE } else { STEEL });
                        }
                    }
                }
            }
            post(g, 16.0, 16.0, 0, 16, 2.2, PLASTIC);
            post(g, 16.0, 16.0, 16, 18, 3.0, PLASTIC_B);
        }
        // Sniper rounds: three long ones stood in a clip.
        "sniper" => {
            g.fill([9, 0, 13], [23, 4, 19], BLACK);
            for cx in [11.5f32, 16.0, 20.5] {
                post(g, cx, 16.0, 4, 18, 1.6, BRASS);
                post(g, cx, 16.0, 18, 24, 1.2, COPPER);
                post(g, cx, 16.0, 24, 26, 0.7, COPPER);
            }
            post(g, 16.0, 16.0, 2, 3, 6.0, PLASMA);
        }
        // Bullets: an ammunition can with a row of rounds standing out of it.
        _ => {
            g.fill([9, 0, 12], [23, 9, 20], OLIVE);
            g.fill([9, 7, 12], [23, 9, 20], DARK);
            g.fill([8, 3, 11], [24, 4, 21], BRASS);
            for cx in [11.0f32, 14.0, 17.0, 20.0] {
                post(g, cx + 0.5, 16.0, 9, 15, 1.3, BRASS);
                post(g, cx + 0.5, 16.0, 15, 18, 0.9, COPPER);
            }
            g.fill([10, 9, 15], [22, 10, 17], HOT);
        }
    }
}

/// A rocket as it flies: a body, a nose, four fins and a flame behind, laid
/// down the z axis with the nose toward -z, the way a weapon's own model is.
fn rocket(g: &mut Grid) {
    rod(g, 16.0, 16.0, 14, 26, 2.2, OLIVE);
    rod(g, 16.0, 16.0, 11, 15, 1.5, DARK);
    g.ellipsoid([16.0, 16.0, 12.0], [1.6, 1.6, 2.4], RUST);
    // A band round the middle and the fins at the tail.
    rod(g, 16.0, 16.0, 19, 21, 2.6, DARK);
    for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        for k in 0..4 {
            let t = 2.0 + k as f32;
            let (x, y) = (16.0 + dx * t, 16.0 + dy * t);
            g.fill([x as i32, y as i32, 23], [x as i32 + 1, y as i32 + 1, 27], STEEL);
        }
    }
    // The motor burning, which is the only part of it visible at distance.
    rod(g, 16.0, 16.0, 26, 29, 1.8, HOT);
    g.ellipsoid([16.0, 16.0, 30.0], [1.3, 1.3, 1.8], HOT);
}

/// A grenade: a ribbed can with a band round it, so it reads as it tumbles.
fn grenade(g: &mut Grid) {
    rod(g, 16.0, 16.0, 13, 20, 2.6, DARK);
    g.ellipsoid([16.0, 16.0, 13.0], [2.4, 2.4, 1.6], DARK);
    g.ellipsoid([16.0, 16.0, 20.0], [2.4, 2.4, 1.6], DARK);
    rod(g, 16.0, 16.0, 15, 18, 3.0, BRASS);
    // The lever down one side.
    g.fill([18, 15, 12], [20, 17, 14], STEEL);
}

/// A scrap shard: a chip of hot scrap, no two faces alike.
fn shard(g: &mut Grid) {
    g.fill([14, 15, 14], [19, 18, 18], RUST);
    g.fill([15, 14, 13], [18, 16, 20], HOT);
    g.fill([13, 16, 16], [16, 19, 19], BRASS);
}

/// A glob of sludge: a lumpy ball, brightest at its middle. Nothing about
/// it points anywhere, because it tumbles and then sits on a wall.
fn goo_blob(g: &mut Grid) {
    g.ellipsoid([16.0, 16.0, 16.0], [3.0, 2.8, 3.0], GOO);
    g.ellipsoid([14.4, 17.2, 15.0], [1.6, 1.6, 1.6], GOO);
    g.ellipsoid([17.6, 15.0, 17.2], [1.4, 1.4, 1.4], GOO);
    g.ellipsoid([16.0, 16.0, 16.0], [1.4, 1.4, 1.4], HOT);
}

/// A razor disc, flat across its flight: a ring of blade with a hub, laid in
/// the x-y plane so it flies edge-on and spins about its own axis.
fn disc(g: &mut Grid) {
    for y in 2..30 {
        for x in 2..30 {
            let (dx, dy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            let r = (dx * dx + dy * dy).sqrt();
            if r > 4.0 && r <= 7.0 {
                g.fill([x, y, 15], [x + 1, y + 1, 17], EDGE);
            } else if r <= 2.2 {
                g.fill([x, y, 14], [x + 1, y + 1, 18], STEEL);
            }
        }
    }
}

/// A block of spoil: half a metre of stone, the size of what it lays down
/// where it lands, with darker edges so it reads as a block and not a box
/// of light.
fn block(g: &mut Grid) {
    g.fill([2, 2, 2], [30, 30, 30], DARK);
    g.fill([3, 3, 2], [29, 29, 30], STEEL);
    g.fill([2, 3, 3], [30, 29, 29], STEEL);
    g.fill([3, 2, 3], [29, 30, 29], STEEL);
}

/// A bolt of plasma: a short bright dart with a tail.
fn bolt(g: &mut Grid) {
    rod(g, 16.0, 16.0, 14, 19, 1.5, PULSE);
    g.ellipsoid([16.0, 16.0, 14.0], [1.9, 1.9, 1.9], PULSE);
    g.ellipsoid([16.0, 16.0, 20.0], [1.2, 1.2, 2.2], PLASMA);
}

/// A round rod down the z axis: the shape nearly everything on a gun is.
/// Up to 3 voxels across it is drawn as a smooth tube (`Grid::rod`, threads
/// in mesh/lean.rs): barrels, prongs, teeth, plungers. Fatter ones (drums,
/// tanks, sleeves) stay a circle of voxels, which reads as round at that size.
fn rod(g: &mut Grid, cx: f32, cy: f32, z0: i32, z1: i32, r: f32, c: u8) {
    if r <= 3.0 {
        g.rod([cx, cy, z0 as f32], [cx, cy, z1 as f32], r, c);
        return;
    }
    let lo = (cx - r).floor() as i32;
    let hi = (cx + r).ceil() as i32;
    let ly = (cy - r).floor() as i32;
    let hy = (cy + r).ceil() as i32;
    for z in z0..z1 {
        for y in ly..=hy {
            for x in lo..=hi {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    g.set(x, y, z, c);
                }
            }
        }
    }
}

/// The gearbox, drum and grip; the barrels are a model of their own.
fn minigun(g: &mut Grid) {
    // A short receiver at the back: a minigun is mostly barrel, and the one
    // that was here was mostly box.
    g.fill([13, 8, 19], [20, 17, 28], STEEL);
    g.fill([12, 10, 21], [21, 15, 27], DARK);
    // Ammunition drum on the right, and the belt into the breech.
    rod(g, 22.5, 11.0, 20, 28, 4.2, DARK);
    g.fill([19, 10, 21], [22, 13, 25], BRASS);
    // Top rail and rear sight.
    g.fill([14, 17, 20], [19, 19, 27], DARK);
    g.fill([15, 19, 24], [18, 21, 26], DARK);
    // Grip and trigger guard.
    g.fill([14, 0, 24], [19, 8, 28], GRIP);
    g.fill([14, 6, 22], [19, 8, 25], DARK);
    // Front handguard at the root of the barrels, where a hand holds it
    // clear of what is turning.
    g.fill([13, 8, 16], [20, 12, 20], DARK);
    g.fill([14, 3, 16], [19, 9, 20], GRIP);
}

/// Six barrels on a ring, in their own model so they can turn.
fn barrels(g: &mut Grid) {
    for k in 0..6 {
        let a = k as f32 / 6.0 * std::f32::consts::TAU;
        rod(g, 16.0 + 3.0 * a.cos(), 13.0 + 3.0 * a.sin(), 0, 19, 1.0, BARREL);
    }
    // The plate they are held in, front and back.
    rod(g, 16.0, 13.0, 0, 2, 4.2, STEEL);
    rod(g, 16.0, 13.0, 16, 19, 4.4, STEEL);
}

fn launcher(g: &mut Grid) {
    // The tube, with a flare at the muzzle and a vent at the back.
    rod(g, 16.0, 13.0, 2, 25, 4.0, OLIVE);
    rod(g, 16.0, 13.0, 1, 4, 5.2, DARK);
    rod(g, 16.0, 13.0, 25, 29, 5.0, DARK);
    rod(g, 16.0, 13.0, 3, 28, 2.6, GRIP);
    // Shoulder body, optical sight on a bracket, and the grip.
    g.fill([12, 6, 20], [21, 12, 29], OLIVE);
    g.fill([14, 17, 9], [19, 19, 15], DARK);
    g.fill([14, 19, 10], [19, 22, 14], STEEL);
    g.fill([15, 15, 11], [18, 18, 13], DARK);
    g.fill([14, 0, 21], [19, 8, 25], GRIP);
    g.fill([14, 6, 19], [19, 8, 22], DARK);
    // A forward hand grip on the tube.
    g.fill([14, 3, 11], [19, 9, 15], GRIP);
}

/// The scrap gun: a forge. A slab of black iron with a letterbox muzzle as
/// wide as the spread it throws, a grille of glowing slits down both sides,
/// cooling fins across the top and a firebox at the back. Under it a raked
/// grip in the P90's manner, with the trigger guard as one loop in front of
/// it, so the hand sits back under the firebox and the gun reads as held
/// rather than as a box with a handle bolted to it.
fn scrap(g: &mut Grid) {
    g.fill([8, 10, 0], [25, 17, 3], DARK);
    g.fill([10, 12, 0], [23, 15, 1], HOT);
    g.fill([10, 8, 3], [23, 18, 22], BLACK);
    for z in (5..20).step_by(3) {
        g.fill([9, 11, z], [10, 15, z + 1], HOT);
        g.fill([23, 11, z], [24, 15, z + 1], HOT);
    }
    for z in (4..21).step_by(2) {
        g.fill([11, 18, z], [22, 20, z + 1], DARK);
    }
    g.fill([12, 7, 22], [21, 16, 28], RUST);
    g.fill([14, 10, 28], [19, 12, 29], HOT);
    // The grip, raked back: each row down sits half a voxel further back.
    for y in 0..8 {
        let z0 = 18 + (8 - y) / 2;
        g.fill([14, y, z0], [19, y + 1, z0 + 4], GRIP);
    }
    // The trigger guard: a bar down from the body, a bar back along the
    // bottom to the grip, and the trigger hanging in the loop.
    g.fill([14, 2, 11], [19, 8, 13], DARK);
    g.fill([14, 2, 11], [19, 4, 21], DARK);
    g.fill([15, 5, 16], [17, 8, 17], STEEL);
}

/// The sidearm: a bullpup PDW in the P90's shape. A stubby barrel out of a
/// sloped nose, the smoky magazine lying along the top with the rounds
/// showing through it, a ring sight on the magazine's front, a loop at the
/// front for the trigger hand's fingers, and the thumbhole behind the grip
/// that makes the stock and the grip one piece.
fn sidearm(g: &mut Grid) {
    // Upper body, nose stepped down toward the muzzle.
    g.fill([13, 11, 6], [19, 17, 30], DARK);
    g.fill([13, 12, 4], [19, 16, 6], DARK);
    g.fill([14, 13, 3], [18, 15, 4], BLACK);
    // The barrel and its flash hider, short: most of it is inside the body.
    rod(g, 16.0, 14.0, 0, 3, 1.0, BARREL);
    rod(g, 16.0, 14.0, 0, 1, 1.4, BLACK);
    // The magazine along the top: smoky and lit a little from inside, with
    // a stripe of brass down the middle where the rounds lie across it.
    g.fill([13, 17, 7], [19, 19, 28], GLASS);
    g.fill([15, 18, 8], [17, 19, 27], BRASS);
    g.fill([13, 17, 27], [19, 19, 29], BLACK);
    // The ring sight on its block at the front of the magazine.
    g.fill([14, 19, 8], [18, 20, 13], BLACK);
    g.fill([14, 20, 9], [18, 23, 10], DARK);
    g.fill([14, 20, 12], [18, 23, 13], DARK);
    g.fill([14, 22, 10], [18, 23, 12], DARK);
    g.fill([15, 20, 10], [17, 22, 12], GLASS);
    // Lower body: the front loop, the grip, the thumbhole and the butt, all
    // one moulding, built round the two holes rather than cut from a block.
    // The front loop: a bar down the front, a bar along the bottom, a strip
    // under the body, and the trigger hanging in the hole between them.
    g.fill([13, 4, 5], [19, 11, 7], BLACK);
    g.fill([13, 2, 7], [19, 4, 14], BLACK);
    g.fill([13, 10, 7], [19, 11, 14], BLACK);
    g.fill([13, 2, 13], [19, 11, 14], BLACK);
    g.fill([15, 7, 11], [17, 10, 12], STEEL);
    // The grip, under where the trigger hand sits.
    g.fill([13, 0, 14], [19, 11, 19], GRIP);
    // The thumbhole, right through: a bar below it and one above.
    g.fill([13, 0, 19], [19, 4, 23], BLACK);
    g.fill([13, 10, 19], [19, 11, 23], BLACK);
    // The butt, with a rubber plate on the back.
    g.fill([13, 0, 23], [19, 11, 29], BLACK);
    g.fill([13, 0, 29], [19, 11, 30], GRIP);
    // Charging handle and the ejection slot under the body.
    g.fill([12, 13, 9], [13, 15, 11], STEEL);
    g.fill([15, 1, 20], [17, 2, 23], DARK);
}

/// The Digger: a cartridge in a skeleton frame, a nozzle at the front, a
/// plunger rod out the back, a squeeze handle, and a spade welded under the
/// nozzle. It is a tool off a shelf in the works with the cartridge swapped
/// for something that goes off, and it should look like one -- the joke only
/// lands if the thing is plainly a caulk gun and plainly lethal.
fn digger(g: &mut Grid) {
    // The nozzle: a cone, stepped, with a bead of whatever it lays sitting
    // at the tip. The bead is the one bright thing on it.
    rod(g, 16.0, 14.0, 0, 3, 1.6, PLASTIC);
    rod(g, 16.0, 14.0, 3, 6, 2.6, PLASTIC);
    rod(g, 16.0, 14.0, 6, 8, 3.4, DARK);
    g.ellipsoid([16.0, 14.0, 0.5], [1.5, 1.5, 1.5], FOAM);
    // The cartridge, a fat tube most of the length of it, with a label band
    // and a crimp at the back.
    rod(g, 16.0, 14.0, 8, 25, 4.0, PALE);
    rod(g, 16.0, 14.0, 12, 17, 4.3, PLASTIC);
    rod(g, 16.0, 14.0, 24, 26, 3.4, DARK);
    // The frame: two side bars down the length, a half-ring holding the
    // cartridge at the front and a back plate the plunger comes through.
    for x in [11, 20] {
        g.fill([x, 9, 7], [x + 1, 12, 27], STEEL);
    }
    g.fill([11, 9, 7], [21, 12, 9], STEEL);
    g.fill([12, 8, 25], [20, 20, 27], DARK);
    // The plunger rod, out the back with its disc of a handle.
    rod(g, 16.0, 14.0, 26, 31, 0.9, STEEL);
    g.fill([13, 11, 30], [20, 18, 31], STEEL);
    // The squeeze handle and its trigger, and the bar the front hand takes.
    g.fill([14, 0, 24], [19, 9, 28], GRIP);
    g.fill([14, 7, 21], [19, 9, 25], DARK);
    g.fill([11, 4, 9], [21, 9, 12], GRIP);
    // The spade: a blade under the nozzle, lying flat the way an entrenching
    // tool's does, so it shows its face from the side rather than its edge.
    // It is widest at the cutting edge and narrows back to the socket, which
    // is the only thing that makes it read as a spade rather than a tongue.
    for (z, half) in [(0, 5), (1, 6), (2, 6), (3, 6), (4, 5), (5, 5), (6, 4), (7, 3)] {
        let c = if z < 2 { EDGE } else { STEEL };
        g.fill([16 - half, 7, z], [17 + half, 9, z + 1], c);
        // Dished: the sides stand a voxel proud of the middle.
        for x in [16 - half, 16 + half] {
            g.fill([x, 9, z], [x + 1, 10, z + 1], c);
        }
    }
    // The shank that holds it on, back and up into the frame under the
    // nozzle, and the collar round the cartridge it is clamped to.
    g.fill([14, 8, 6], [19, 12, 11], DARK);
    g.fill([12, 7, 10], [21, 13, 12], STEEL);
}

/// The goo gun: a tank of sludge over a short wide nozzle, with the stuff
/// visible in a sight glass down the side.
fn goo_gun(g: &mut Grid) {
    // Nozzle: wide, short, and open at the end.
    rod(g, 16.0, 12.0, 2, 13, 4.4, OLIVE);
    rod(g, 16.0, 12.0, 1, 3, 5.6, DARK);
    rod(g, 16.0, 12.0, 2, 12, 3.0, GOO);
    // Body.
    g.fill([12, 6, 12], [21, 17, 27], OLIVE);
    g.fill([11, 8, 14], [22, 15, 25], DARK);
    // The tank on top, strapped down, with a window of sludge along it.
    rod(g, 16.0, 20.0, 13, 27, 4.2, STEEL);
    g.fill([11, 19, 15], [12, 22, 25], GOO);
    g.fill([21, 19, 15], [22, 22, 25], GOO);
    for z in [15, 19, 23] {
        rod(g, 16.0, 20.0, z, z + 1, 4.6, DARK);
    }
    // A sight glass down each side, with the sludge behind it: the gun is
    // held to the right of the screen, so only its left side is ever seen.
    g.fill([21, 9, 16], [22, 15, 24], GOO);
    g.fill([11, 9, 16], [12, 15, 24], GOO);
    g.fill([21, 15, 16], [22, 16, 24], GLASS);
    g.fill([11, 15, 16], [12, 16, 24], GLASS);
    g.fill([14, 14, 11], [19, 18, 14], COPPER);
    // Grip and fore grip.
    g.fill([14, 0, 21], [19, 8, 25], GRIP);
    g.fill([14, 6, 19], [19, 8, 22], DARK);
    g.fill([13, 2, 8], [20, 8, 12], GRIP);
}

/// The bore: a rock drill with the feed taken off it. A heavy barrel
/// sleeve, a cutting head of copper teeth at the muzzle, a cooling jacket
/// with fins, and a bit that glows because of what it has been doing.
fn bore(g: &mut Grid) {
    // The cutting head: a ring of teeth round a hot bit.
    rod(g, 16.0, 14.0, 0, 4, 4.4, COPPER);
    rod(g, 16.0, 14.0, 0, 5, 2.6, DARK);
    rod(g, 16.0, 14.0, 0, 2, 1.4, HOT);
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        rod(g, 16.0 + 3.6 * a.cos(), 14.0 + 3.6 * a.sin(), 0, 3, 0.8, EDGE);
    }
    // The barrel sleeve, with cooling fins down it.
    rod(g, 16.0, 14.0, 4, 20, 3.0, STEEL);
    for z in [7, 10, 13, 16] {
        rod(g, 16.0, 14.0, z, z + 1, 4.2, RUST);
    }
    // The body, the motor housing over it, and a vent.
    g.fill([13, 9, 18], [20, 19, 28], DARK);
    rod(g, 16.0, 19.0, 19, 27, 2.6, STEEL);
    for z in [21, 24] {
        g.fill([12, 18, z], [21, 22, z + 1], BLACK);
    }
    // Grip, trigger, and the bar the front hand takes on a drill this heavy.
    g.fill([14, 0, 23], [19, 9, 27], GRIP);
    g.fill([14, 7, 21], [19, 9, 24], DARK);
    g.fill([11, 3, 13], [21, 9, 17], GRIP);
}

/// The pulse gun: three prongs round an emitter, which is where the beam
/// strikes from, and a coil pack behind them.
fn pulse(g: &mut Grid) {
    // The emitter: a small glowing eye held between three prongs.
    g.ellipsoid([16.0, 13.0, 6.0], [1.8, 1.8, 1.8], PULSE);
    for (dx, dy) in [(0.0f32, 1.0f32), (-0.87, -0.5), (0.87, -0.5)] {
        let (x, y) = (16.0 + dx * 4.0, 13.0 + dy * 4.0);
        rod(g, x, y, 2, 14, 1.3, EDGE);
        g.fill([x as i32 - 1, y as i32 - 1, 1], [x as i32 + 1, y as i32 + 1, 2], PULSE);
    }
    // The body and the coil pack over it.
    rod(g, 16.0, 13.0, 12, 24, 4.0, STEEL);
    g.fill([12, 7, 18], [21, 18, 28], DARK);
    for z in (13..23).step_by(3) {
        rod(g, 16.0, 19.0, z, z + 2, 3.2, COPPER);
    }
    rod(g, 16.0, 19.0, 13, 23, 1.2, PULSE);
    // Grip and fore grip.
    g.fill([14, 0, 22], [19, 8, 26], GRIP);
    g.fill([14, 6, 20], [19, 8, 23], DARK);
    g.fill([13, 3, 13], [20, 9, 17], GRIP);
}

/// The skipjack: a toy. Orange shell, blue furniture, a drum of discs on
/// its side and a muzzle three times wider than the slot a disc comes out
/// of. Everything on it is a size too big, which is what makes a thing read
/// as plastic rather than as machined steel -- an iron gun is all thin
/// edges, and this has none.
fn skipjack(g: &mut Grid) {
    // The muzzle: a fat collar round a slot, with a pale ring at its lip.
    rod(g, 16.0, 14.0, 0, 9, 4.2, PLASTIC);
    rod(g, 16.0, 14.0, 0, 2, 4.8, PALE);
    g.fill([12, 12, 0], [21, 16, 8], BLACK);
    g.fill([13, 12, 0], [20, 16, 2], EDGE);
    // The shell, in two colours with a seam between them, because a toy is
    // moulded in halves and never lets you forget it.
    g.fill([13, 8, 8], [20, 21, 27], PLASTIC);
    g.fill([12, 11, 11], [21, 18, 24], PLASTIC_B);
    g.fill([12, 17, 11], [21, 18, 24], PALE);
    // The drum of discs, out on the right with the top one showing.
    rod(g, 22.0, 14.0, 11, 22, 5.2, PLASTIC_B);
    rod(g, 22.0, 14.0, 13, 20, 3.6, FOAM);
    rod(g, 22.0, 14.0, 15, 18, 1.4, PLASTIC);
    // A cocking handle along the top and a sight that is pure decoration.
    g.fill([15, 21, 16], [18, 24, 23], PALE);
    g.fill([15, 21, 9], [18, 23, 12], PLASTIC_B);
    // The grip, with a pale band, and a trigger guard a size too big.
    g.fill([14, 0, 21], [19, 11, 27], PLASTIC);
    g.fill([14, 3, 21], [19, 6, 27], PLASTIC_B);
    g.fill([13, 7, 16], [20, 10, 22], PLASTIC);
    g.fill([14, 9, 19], [19, 11, 22], BLACK);
}

/// The sniper rifle: a long barrel, a heavy scope on rings, a bolt out to
/// the right, and a stock that reaches back past the eye.
fn sniper(g: &mut Grid) {
    // A long thin barrel is most of what makes a rifle read as one, so it
    // runs most of the model and nothing else is allowed to be as thick.
    rod(g, 16.0, 14.0, 0, 20, 1.7, BARREL);
    // Muzzle brake: a fatter collar with two slots cut across it.
    rod(g, 16.0, 14.0, 0, 3, 2.4, DARK);
    for z in [1, 2] {
        g.fill([13, 15, z], [19, 17, z + 1], BLACK);
    }
    // Slim receiver, five voxels wide against the minigun's nine, and in
    // steel rather than black: a rifle that is all one dark mass at arm's
    // length is a dark mass, not a rifle.
    g.fill([14, 11, 18], [19, 17, 25], STEEL);
    g.fill([14, 16, 17], [19, 18, 26], DARK);
    // The stock: a wrist that drops behind the grip, a comb to lay a cheek
    // on, and a butt plate. This is the shape an eye reads as a rifle
    // rather than as a pipe with a handle on it.
    g.fill([14, 9, 24], [19, 16, 28], OLIVE);
    g.fill([14, 12, 27], [19, 17, 31], OLIVE);
    g.fill([14, 10, 30], [19, 17, 31], DARK);
    // Fore-end: wood under the barrel where the front hand goes, stopping
    // well short of the muzzle so the barrel is seen to be bare.
    g.fill([14, 10, 12], [19, 14, 19], OLIVE);
    // The bolt handle, out to the right where it can be seen.
    g.fill([19, 13, 21], [23, 15, 23], STEEL);
    g.ellipsoid([23.0, 14.0, 22.0], [1.4, 1.4, 1.4], EDGE);
    // The scope: a tube on two low rings, an objective bell at the front
    // and glass at the eye. Low on purpose -- a scope on tall rings makes
    // the silhouette square, and square is what everything in here was.
    rod(g, 16.0, 19.5, 10, 26, 1.7, BLACK);
    rod(g, 16.0, 19.5, 10, 13, 2.4, BLACK);
    g.fill([14, 17, 13], [19, 19, 15], STEEL);
    g.fill([14, 17, 22], [19, 19, 24], STEEL);
    g.fill([14, 18, 25], [19, 21, 26], GLASS);
    // Pistol grip and the magazine ahead of it.
    g.fill([14, 4, 23], [19, 12, 27], GRIP);
    g.fill([14, 10, 21], [19, 12, 24], DARK);
    g.fill([14, 7, 19], [19, 12, 22], BRASS);
    // A sling swivel under the fore-end, which is all the furniture it
    // needs: a bipod read as legs on a spider at this size.
    g.fill([15, 8, 15], [18, 10, 16], STEEL);
}

/// A rough star of light, a few frames long.
fn flash(g: &mut Grid) {
    g.ellipsoid([16.0, 16.0, 16.0], [3.2, 3.2, 2.6], HOT);
    for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (-0.7, -0.7)] {
        for k in 1..7 {
            let t = k as f32;
            g.ellipsoid([16.0 + dx * t, 16.0 + dy * t, 16.0], [1.6 - t * 0.2, 1.6 - t * 0.2, 1.4 - t * 0.15], HOT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every weapon fills a different box. Ten guns drawn in the same 32
    /// cube come out the same size and shape unless somebody is watching:
    /// a sidearm has to be small, a rifle long and thin, a minigun deep.
    /// This prints the measurements and fails when the arsenal has gone
    /// square again.
    #[test]
    fn the_guns_are_not_all_the_same_box() {
        let mut shape = Vec::new();
        for (i, name) in SHAPES.iter().enumerate() {
            let mut g = Grid::new();
            build_shape(name, &mut g);
            // The minigun's barrels are a model of their own, and a minigun
            // measured without them is a stub: they go in the same grid
            // here, which is where they sit in the world anyway.
            if i == MINIGUN {
                barrels(&mut g);
            }
            let (lo, hi) = g.bounds().expect("a shape with nothing in it");
            // Centimetres, which is what reaches the screen: the grid is one
            // size for every gun and the scale is not.
            let k = VOX * SCALE[i] * 100.0;
            let (w, t, l) = ((hi[0] - lo[0] + 1) as f32 * k, (hi[1] - lo[1] + 1) as f32 * k, (hi[2] - lo[2] + 1) as f32 * k);
            println!("{name:10} {w:5.1} wide {t:5.1} tall {l:5.1} long cm   long/tall {:.2}  bulk {:.0}", l / t, w * t * l);
            shape.push((*name, w, t, l));
        }
        // Length against height: a rifle is a line, the Digger is a tube,
        // a pistol is a block.
        let slim = shape.iter().map(|(_, _, t, l)| l / t).fold(0.0f32, f32::max);
        let squat = shape.iter().map(|(_, _, t, l)| l / t).fold(f32::MAX, f32::min);
        assert!(slim > 1.7, "the longest gun is only {slim:.2} times its own height: they are all boxes");
        assert!(slim > squat * 1.6, "every gun has the same proportions: {squat:.2} to {slim:.2}");
        // And the longest is well clear of the shortest, which a single
        // scale for the whole arsenal is exactly what prevents.
        let long: Vec<f32> = shape.iter().map(|(_, _, _, l)| *l).collect();
        let big = long.iter().cloned().fold(0.0f32, f32::max);
        let small = long.iter().cloned().fold(f32::MAX, f32::min);
        assert!(big > small * 1.9, "the longest gun is only {:.1} times the shortest", big / small);
    }
}
