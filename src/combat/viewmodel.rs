//! The gun in the player's hands: a voxel model held in the camera's frame.
//!
//! These are drawn as ordinary entity instances, not as a HUD overlay, so
//! they take the light of the room they are in and move with the view.
//! `src/shooter.rs` decides where to hold one and how it bobs, sways and
//! kicks; this file only builds the shapes.
//!
//! The guns themselves are drawn in `combat::real`, each at its own real
//! size; this file turns them into models, with the shells in flight, the
//! muzzle flash and the ammunition looks. Model space: x right, y up, the
//! muzzle toward -z, the pivot at the foot of the grip. The minigun's
//! barrels are a model of their own with the pivot on their axis, so they
//! can spin without swinging the gun round with them.

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
    super::real::palette(p);
}

/// The arena guns' frame: model voxels to the metre that a point handed out
/// by `muzzle`, `fore` and `spin_axis` is measured in, with the grip at
/// (16, 0, 16). Every gun has a voxel size of its own (`combat::real`), so
/// the points are put into this one frame for whoever places them.
const VOX: f32 = 1.0 / 58.0;

/// Every shape a weapon file may wear (`"view"`): one per `combat::real`
/// model, in the same order.
pub const SHAPE_COUNT: usize = super::real::REAL.len();

/// The model a shape index is.
pub fn real(shape: usize) -> Option<&'static super::real::Real> {
    super::real::REAL.get(shape)
}

/// Every shape's name, in order.
pub fn shape_names() -> &'static [&'static str] {
    static N: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    N.get_or_init(|| super::real::REAL.iter().map(|r| r.name).collect())
}

/// The models' marks and voxel sizes, worked out once: the muzzle is asked
/// for on every shot.
fn real_marks() -> &'static [(super::real::Marks, f32)] {
    static M: std::sync::OnceLock<Vec<(super::real::Marks, f32)>> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        super::real::REAL
            .iter()
            .map(|r| {
                let (_, m) = super::real::build(r);
                (m, super::real::voxel(r, &m))
            })
            .collect()
    })
}

/// A point of a model, put into the shared frame: the grid coordinates it
/// would have in a model with its grip at (16, 0, 16) and voxels of `VOX`.
/// Everything that places a point of a held gun (the muzzle, the flash, a
/// bot's hand, the minigun's barrels) works in that frame.
fn real_point(k: usize, p: [f32; 3]) -> Vec3 {
    let Some((m, vox)) = real_marks().get(k) else { return Vec3::new(16.0, 13.0, 1.0) };
    Vec3::new(16.0, 0.0, 16.0) + (Vec3::from(p) - Vec3::from(m.grip)) * (vox / VOX)
}

/// How a body carries a shape.
pub fn hands(shape: usize) -> super::real::Hands {
    real(shape).map_or(super::real::Hands::Shoulder, |r| r.hands)
}

/// Where the left hand goes on a shape held in two, in the shared frame;
/// None for a one-handed one.
pub fn fore(shape: usize) -> Option<Vec3> {
    real_marks().get(shape)?.0.fore.map(|f| real_point(shape, f))
}

/// The butt of a shape, in metres of the gun from the foot of its grip
/// (x right, y up, +z back toward the shoulder).
pub fn butt(shape: usize) -> Vec3 {
    match real_marks().get(shape) {
        Some((m, _)) => (real_point(shape, m.butt) - Vec3::new(16.0, 0.0, 16.0)) * VOX,
        None => Vec3::new(0.0, 0.1, 0.2),
    }
}

/// How a bot holds a shape, in metres of the gun from the foot of its grip.
pub fn grasp(shape: usize) -> crate::anim::figure::Grasp {
    use super::real::Hands;
    use crate::anim::figure::{Grasp, Style};
    let metres = |p: Vec3| (p - Vec3::new(16.0, 0.0, 16.0)) * VOX;
    let (Some(r), Some((m, _))) = (real(shape), real_marks().get(shape)) else { return Grasp::default() };
    let bore = metres(real_point(shape, m.muzzle)).y;
    let back = metres(real_point(shape, m.butt)).z.max(0.0);
    match r.hands {
        Hands::One => Grasp { style: Style::One, bore: 0.04, reach: 0.30, fore: None },
        Hands::Haft => Grasp { style: Style::Haft, bore: 0.04, reach: 0.30, fore: fore(shape).map(metres) },
        Hands::Pistol => Grasp { style: Style::Pistol, bore, reach: 0.34, fore: Some(Vec3::new(-0.035, 0.03, 0.01)) },
        Hands::Shoulder => Grasp { style: Style::Shoulder, bore, reach: back.max(0.12), fore: fore(shape).map(metres) },
        Hands::Tube => Grasp { style: Style::Tube, bore, reach: back.max(0.12) * 0.5, fore: fore(shape).map(metres) },
    }
}

/// Whether a shape has barrels that turn: the minigun's, which are the
/// `BARRELS` model.
pub fn spins(shape: usize) -> bool {
    real(shape).is_some_and(|r| r.name == super::real::SPINNER)
}

/// Where the turning barrels' axis is, in the shared frame: the point the
/// `BARRELS` model is placed at.
pub fn spin_axis() -> Vec3 {
    match super::real::REAL.iter().position(|r| r.name == super::real::SPINNER) {
        Some(k) => real_point(k, super::real::arena_c::MINIGUN_AXIS),
        None => Vec3::new(16.0, 13.0, 16.0),
    }
}

/// Model voxels to the metre of the shared frame, for whoever places a
/// point in it.
pub const VOXEL: f32 = VOX;

/// The point of each shape the shot leaves from, in the shared frame.
pub fn muzzle(shape: usize) -> Vec3 {
    match real_marks().get(shape) {
        Some((m, _)) => real_point(shape, m.muzzle),
        None => Vec3::new(16.0, 13.0, 1.0),
    }
}

/// Index into `models()` of the shape called `name`, if there is one.
pub fn shape_of(name: &str) -> Option<usize> {
    super::real::REAL.iter().position(|r| r.name == name)
}

/// The same, falling back to the first shape, for a weapon file that names
/// none: better a gun in hand than nothing at all.
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

/// One per shape, then the minigun's turning barrels, the muzzle flash and
/// one per name in `SHELLS`.
pub const BARRELS: usize = SHAPE_COUNT;
pub const FLASH: usize = SHAPE_COUNT + 1;
/// The first of the shell bodies.
pub const SHELL: usize = SHAPE_COUNT + 2;
/// What a pad of rounds is drawn as, one look per kind of ammunition, so a
/// pad of rockets can be told from a pad of bullets across a hall without
/// reading anything. Guns that share a store (`ammo_type`) share a look.
pub const AMMOS: [&str; 12] = ["bullets", "rockets", "scrap", "goo", "bore", "cells", "discs", "sniper", "shells", "grenades", "fuel", "heavy"];
/// The first of the ammunition looks.
pub const AMMO: usize = SHELL + SHELLS.len();
/// How many models `models()` returns, so whatever is appended after them
/// knows where it starts.
pub const COUNT: usize = AMMO + AMMOS.len();

/// Ammunition looks are a little finer than the guns: a can of bullets is a
/// hand across, and it is held up on its pad rather than at arm's length.
const AMMO_VOX: f32 = 1.0 / 44.0;

/// Which look a pad of a weapon's rounds is drawn as: its `ammo_type` where
/// that names one, else read off which gun it is.
pub fn ammo_look(w: &crate::library::WeaponDef) -> usize {
    let by_type = w.ammo_type.as_deref().and_then(|t| AMMOS.iter().position(|a| *a == t));
    let by_gun = match w.name.as_str() {
        "rpg" | "rocket_launcher" => "rockets",
        "grenade_launcher" => "grenades",
        "flamethrower" => "fuel",
        "shotgun" => "shells",
        "bolt_rifle" | "sniper_rifle" => "sniper",
        "anti_materiel_rifle" => "heavy",
        "scrap_gun" => "scrap",
        "goo_gun" => "goo",
        "bore" => "bore",
        "arc_gun" => "cells",
        "skipjack" => "discs",
        _ => "bullets",
    };
    AMMO + by_type.unwrap_or_else(|| AMMOS.iter().position(|a| *a == by_gun).unwrap_or(0))
}

/// Shell bodies are a tenth the guns' scale: a rocket is about 0.5 m long
/// and a scrap shard a hand's width, which is what they have to be for one
/// seen edge-on at thirty metres to read as a speck and not a crate.
const SHELL_VOX: f32 = 1.0 / 56.0;

pub fn models() -> Vec<Model> {
    let mut out = Vec::new();
    // Every gun at its own voxel size, held by its grip.
    for (r, (m, vox)) in super::real::REAL.iter().zip(real_marks()) {
        let (g, _) = super::real::build(r);
        let mut model = g.build(*vox, 0.2, 0.5);
        model.pivot = Vec3::from(m.grip);
        out.push(model);
    }
    // The minigun's barrels, in its grid and at its voxel size, turning
    // about their own axis.
    let mut b = Grid::new();
    super::real::arena_c::minigun_barrels(&mut b);
    let vox = super::real::REAL.iter().zip(real_marks()).find(|(r, _)| r.name == super::real::SPINNER).map_or(VOX, |(_, (_, v))| *v);
    let mut m = b.build(vox, 0.2, 0.5);
    m.pivot = Vec3::from(super::real::arena_c::MINIGUN_AXIS);
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
        // A cell for the arc gun: a dark can with lit windows and copper
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
        // Shotgun shells: a box of red hulls with brass heads, a few
        // standing loose on the lid.
        "shells" => {
            g.fill([9, 0, 11], [23, 7, 21], DARK);
            for (cx, cz) in [(12.0f32, 14.0f32), (16.0, 14.0), (20.0, 14.0), (14.0, 18.5), (18.0, 18.5)] {
                post(g, cx, cz, 7, 9, 1.8, BRASS);
                post(g, cx, cz, 9, 16, 1.6, RUST);
            }
        }
        // Forty-millimetre grenades: four fat rounds in a bandolier strip.
        "grenades" => {
            g.fill([7, 0, 13], [25, 3, 19], OLIVE);
            for cx in [10.0f32, 14.0, 18.0, 22.0] {
                post(g, cx, 16.0, 3, 9, 1.9, BRASS);
                post(g, cx, 16.0, 9, 13, 1.9, OLIVE);
                post(g, cx, 16.0, 13, 14, 1.2, BRASS);
            }
        }
        // Fuel: a red jerrycan with its spout up.
        "fuel" => {
            g.fill([10, 0, 12], [22, 18, 20], RUST);
            g.fill([10, 6, 11], [22, 7, 21], DARK);
            g.fill([12, 18, 13], [16, 20, 19], DARK);
            post(g, 19.0, 16.0, 18, 22, 1.3, STEEL);
            post(g, 19.0, 16.0, 22, 23, 0.8, HOT);
        }
        // Fifty-calibre rounds: three long ones on a link, laid down.
        "heavy" => {
            for cx in [11.0f32, 16.0, 21.0] {
                rod(g, cx, 3.0, 6, 22, 2.2, BRASS);
                rod(g, cx, 3.0, 22, 28, 1.6, COPPER);
            }
            g.fill([9, 0, 12], [23, 2, 16], DARK);
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
