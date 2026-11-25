//! Every gun's model, drawn at its real size.
//!
//! Each one fills its 32 grid along its length as finely as the grid
//! allows, marks where its muzzle and its butt are, and is then scaled so
//! that the distance between those two marks is the gun's length in
//! centimetres. A pistol comes out a fifth the length of an anti-materiel
//! rifle because it is. The realistic kit's are in `short`, `long`, `heavy`
//! and `melee`; the arena's in `arena_a` to `arena_d`, painted gunmetal,
//! black, tan and silver with one or two colours of their own. A weapon
//! file names its model in `view`.
//!
//! Model space: x right, y up, the muzzle toward -z. The pivot is the
//! model's own `grip` mark, the foot of the pistol grip where the hand
//! closes on it, which is the point the player's view and a bot's hand both
//! hold a gun by.

use crate::models::Grid;

mod heavy;
mod long;
mod melee;
mod short;
mod arena_a;
mod arena_b;
pub(crate) mod arena_c;
mod arena_d;

/// Palette slots, the same as `combat::viewmodel`'s (18..38) plus the three
/// the realistic guns add: ash, walnut and a tan polymer (17, 62, 63, which
/// nothing else in the fixed block uses).
#[allow(dead_code)]
pub(crate) mod c {
    pub const PLASTIC: u8 = 18;
    pub const PLASTIC_B: u8 = 19;
    pub const FOAM: u8 = 20;
    pub const PALE: u8 = 21;
    pub const STEEL: u8 = 22;
    pub const DARK: u8 = 23;
    pub const BARREL: u8 = 24;
    pub const OLIVE: u8 = 25;
    pub const RUST: u8 = 26;
    pub const BRASS: u8 = 27;
    pub const GRIP: u8 = 28;
    pub const HOT: u8 = 29;
    pub const EDGE: u8 = 34;
    pub const BLACK: u8 = 35;
    pub const COPPER: u8 = 36;
    pub const GLASS: u8 = 37;
    /// Light wood: a shovel haft, an axe handle.
    pub const ASH: u8 = 17;
    /// Dark oiled wood: a rifle stock.
    pub const WALNUT: u8 = 62;
    /// Sand-coloured polymer furniture.
    pub const TAN: u8 = 63;
    /// The energy weapons' own lit colours, shared with the arena's old
    /// models: plasma blue, goo green, pulse mint.
    pub const PLASMA: u8 = 30;
    pub const GOO: u8 = 31;
    pub const PULSE: u8 = 33;
    /// The arena guns' paint (64..80): enamel colours a gun can be painted
    /// in, and four that light themselves.
    pub const RED: u8 = 64;
    pub const YELLOW: u8 = 65;
    pub const TEAL: u8 = 66;
    pub const PURPLE: u8 = 67;
    pub const MAGENTA: u8 = 68;
    pub const LIME: u8 = 69;
    pub const NAVY: u8 = 70;
    pub const WHITE: u8 = 71;
    pub const CHROME: u8 = 72;
    pub const ORANGE: u8 = 73;
    pub const GUNMETAL: u8 = 74;
    pub const MINT: u8 = 75;
    pub const GLOW_CYAN: u8 = 76;
    pub const GLOW_PINK: u8 = 77;
    pub const GLOW_RED: u8 = 78;
    pub const GLOW_YELLOW: u8 = 79;
}

pub fn palette(p: &mut [[f32; 4]]) {
    use crate::models::srgb;
    p[c::ASH as usize] = srgb(196, 160, 108, 0.0);
    p[c::WALNUT as usize] = srgb(96, 58, 34, 0.0);
    p[c::TAN as usize] = srgb(168, 144, 104, 0.0);
    p[c::RED as usize] = srgb(214, 44, 40, 0.0);
    p[c::YELLOW as usize] = srgb(244, 196, 40, 0.0);
    p[c::TEAL as usize] = srgb(30, 160, 150, 0.0);
    p[c::PURPLE as usize] = srgb(124, 70, 190, 0.0);
    p[c::MAGENTA as usize] = srgb(220, 60, 150, 0.0);
    p[c::LIME as usize] = srgb(130, 210, 50, 0.0);
    p[c::NAVY as usize] = srgb(34, 52, 110, 0.0);
    p[c::WHITE as usize] = srgb(236, 236, 232, 0.0);
    p[c::CHROME as usize] = srgb(196, 202, 210, 0.0);
    p[c::ORANGE as usize] = srgb(240, 120, 30, 0.0);
    p[c::GUNMETAL as usize] = srgb(70, 78, 92, 0.0);
    p[c::MINT as usize] = srgb(150, 230, 200, 0.0);
    p[c::GLOW_CYAN as usize] = srgb(80, 230, 255, 0.6);
    p[c::GLOW_PINK as usize] = srgb(255, 90, 200, 0.6);
    p[c::GLOW_RED as usize] = srgb(255, 60, 40, 0.7);
    p[c::GLOW_YELLOW as usize] = srgb(255, 220, 80, 0.6);
}

/// How a body carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hands {
    /// In the right hand only: a knife.
    One,
    /// Both hands on a long haft, the left on the `fore` mark, and swung:
    /// a shovel.
    Haft,
    /// Both hands round the grip, arms out: a pistol.
    Pistol,
    /// Stock to the shoulder, the left hand forward on the `fore` mark.
    Shoulder,
    /// Over the shoulder: a rocket tube, held the same way but higher.
    Tube,
}

/// Points of a model, in its grid's voxel coordinates (a voxel's centre at
/// +0.5).
#[derive(Clone, Copy, Debug)]
pub struct Marks {
    /// The foot of the grip: where the right hand closes, and the pivot.
    pub grip: [f32; 3],
    /// Where the shot leaves: the end of the barrel. A blade's tip.
    pub muzzle: [f32; 3],
    /// The far end from the muzzle: the butt of the stock, the back of the
    /// grip, a shovel's D-handle. Muzzle to butt is the gun's real length.
    pub butt: [f32; 3],
    /// Where the left hand goes, for a gun held in two.
    pub fore: Option<[f32; 3]>,
}

/// One model of one gun.
pub struct Real {
    /// The shape's name, which a weapon file's `view` names.
    pub name: &'static str,
    /// The weapon it is a model of.
    #[allow(dead_code)] // read by the tests and by people
    pub gun: &'static str,
    /// A few words on what it is, for people choosing between the three.
    #[allow(dead_code)]
    pub what: &'static str,
    /// Muzzle to butt, centimetres.
    pub cm: f32,
    pub hands: Hands,
    pub build: fn(&mut Grid) -> Marks,
}

/// The model whose barrels turn: drawn without them, and they are a model
/// of their own (`arena_c::minigun_barrels`, `viewmodel::BARRELS`).
pub const SPINNER: &str = "minigun";

/// Every model's name, in `REAL`'s order, for whatever needs the list as a
/// constant (the EDIT page's knobs). A test keeps the two in step.
pub const NAMES: [&str; 21] = ["shovel_spade", "knife_machete", "pistol_polymer", "gl_drum", "amr_long", "flame_scrap", "ar_bullpup", "mg_jacket", "sg_pump", "rpg_warhead", "sniper_scout", "digger", "sidearm", "goo_gun", "bore", "arc_gun", "skipjack", "minigun", "scrap_gun", "rocket_launcher", "sniper_rifle"];

/// Every gun's model: the realistic kit's, then the arena's.
pub const REAL: &[Real] = &[
    // melee
    Real { name: "shovel_spade", gun: "shovel", what: "army trench spade, D-handle", cm: 100.0, hands: Hands::Haft, build: melee::shovel_spade },
    Real { name: "knife_machete", gun: "knife", what: "machete", cm: 60.0, hands: Hands::One, build: melee::knife_machete },
    // sidearm
    Real { name: "pistol_polymer", gun: "pistol", what: "polymer service pistol", cm: 19.0, hands: Hands::Pistol, build: short::pistol_polymer },
    // lobber
    Real { name: "gl_drum", gun: "grenade_launcher", what: "six-round revolving drum", cm: 78.0, hands: Hands::Shoulder, build: heavy::gl_drum },
    // breacher
    Real { name: "amr_long", gun: "anti_materiel_rifle", what: "long-barrel semi-auto on a bipod", cm: 145.0, hands: Hands::Shoulder, build: long::amr_long },
    // stream
    Real { name: "flame_scrap", gun: "flamethrower", what: "improvised: pipe and propane bottle", cm: 82.0, hands: Hands::Shoulder, build: heavy::flame_scrap },
    // midrange
    Real { name: "ar_bullpup", gun: "assault_rifle", what: "bullpup with an optic", cm: 76.0, hands: Hands::Shoulder, build: long::ar_bullpup },
    // heavy
    Real { name: "mg_jacket", gun: "machine_gun", what: "perforated barrel jacket, belt", cm: 118.0, hands: Hands::Shoulder, build: short::mg_jacket },
    // spread
    Real { name: "sg_pump", gun: "shotgun", what: "pump-action, wood", cm: 105.0, hands: Hands::Shoulder, build: short::sg_pump },
    // explosive
    Real { name: "rpg_warhead", gun: "rpg", what: "reloadable tube, bulbous warhead", cm: 120.0, hands: Hands::Tube, build: heavy::rpg_warhead },
    // marksman
    Real { name: "sniper_scout", gun: "bolt_rifle", what: "short scout rifle", cm: 100.0, hands: Hands::Shoulder, build: long::sniper_scout },
    // The arena's guns, redrawn: three options each (src/combat/real/arena_*.rs).
    Real { name: "digger", gun: "digger", what: "blast hammer: piston drum and flared blast bell", cm: 70.0, hands: Hands::Shoulder, build: arena_a::digger },
    Real { name: "sidearm", gun: "sidearm", what: "boxy bullpup, open-sided magazine on top", cm: 48.0, hands: Hands::Shoulder, build: arena_a::sidearm },
    Real { name: "goo_gun", gun: "goo_gun", what: "launcher with a caged jar of goo", cm: 85.0, hands: Hands::Shoulder, build: arena_a::goo_gun },
    Real { name: "bore", gun: "bore", what: "core drill with a plasma bit", cm: 105.0, hands: Hands::Shoulder, build: arena_b::bore },
    Real { name: "arc_gun", gun: "arc_gun", what: "tank gun, chrome horns round an orb", cm: 90.0, hands: Hands::Shoulder, build: arena_b::arc_gun },
    Real { name: "skipjack", gun: "skipjack", what: "compact disc gun, drum under the front", cm: 80.0, hands: Hands::Shoulder, build: arena_b::skipjack },
    Real { name: "minigun", gun: "minigun", what: "six chrome barrels, drum on the right", cm: 115.0, hands: Hands::Shoulder, build: arena_c::minigun },
    Real { name: "scrap_gun", gun: "scrap_gun", what: "iron forge slab with a firebox", cm: 92.0, hands: Hands::Shoulder, build: arena_c::scrap_gun },
    Real { name: "rocket_launcher", gun: "rocket_launcher", what: "square launch pod", cm: 125.0, hands: Hands::Tube, build: arena_d::rocket_launcher },
    Real { name: "sniper_rifle", gun: "sniper_rifle", what: "bullpup charge lance", cm: 135.0, hands: Hands::Shoulder, build: arena_d::sniper_rifle },
];

/// Model voxels to the metre for one of these, so that muzzle to butt is
/// its `cm`.
pub fn voxel(r: &Real, m: &Marks) -> f32 {
    let d = glam::Vec3::from(m.muzzle).distance(glam::Vec3::from(m.butt)).max(1.0);
    r.cm / 100.0 / d
}

/// Build one, with its marks.
pub fn build(r: &Real) -> (Grid, Marks) {
    let mut g = Grid::new();
    let m = (r.build)(&mut g);
    (g, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// Every model draws something, its marks are inside its grid and on
    /// it, and what comes out is the length it says: a pistol is a fifth of
    /// an anti-materiel rifle, not the same box.
    #[test]
    fn the_real_guns_are_their_real_size() {
        let mut long = Vec::new();
        for r in REAL {
            let (mut g, m) = build(r);
            // The minigun is measured with its barrels, which turn as a
            // model of their own but are part of how long it is.
            if r.name == SPINNER {
                arena_c::minigun_barrels(&mut g);
            }
            let (lo, hi) = g.bounds().unwrap_or_else(|| panic!("{} draws nothing", r.name));
            let k = voxel(r, &m) * 100.0;
            let (w, t, l) = ((hi[0] - lo[0] + 1) as f32 * k, (hi[1] - lo[1] + 1) as f32 * k, (hi[2] - lo[2] + 1) as f32 * k);
            println!("{:16} {:5.1} wide {:5.1} tall {:6.1} long cm   voxel {:.2} cm", r.name, w, t, l, k);
            for (what, p) in [("grip", Some(m.grip)), ("muzzle", Some(m.muzzle)), ("butt", Some(m.butt)), ("fore", m.fore)] {
                let Some(p) = p else { continue };
                assert!(p.iter().all(|v| (-1.0..=33.0).contains(v)), "{}: {what} {p:?} is off the grid", r.name);
            }
            // Fills its grid along its length, or the voxels are coarser than
            // they need be.
            let along = Vec3::from(m.muzzle).distance(Vec3::from(m.butt));
            assert!(along >= 24.0, "{}: muzzle to butt is only {along:.0} voxels of 32", r.name);
            assert!((l.max(t) - r.cm).abs() < r.cm * 0.3, "{}: comes out {l:.0} cm long against {} cm", r.name, r.cm);
            assert_eq!(r.hands != Hands::One, m.fore.is_some() || r.hands == Hands::Pistol, "{}: a two-handed gun needs a fore mark", r.name);
            long.push((r.gun, l.max(t)));
        }
        let of = |gun: &str| long.iter().filter(|(g, _)| *g == gun).map(|(_, l)| *l).fold(0.0f32, f32::max);
        assert!(of("anti_materiel_rifle") > of("pistol") * 4.0);
        assert!(of("assault_rifle") > of("pistol") * 3.0);
        assert_eq!(NAMES.to_vec(), REAL.iter().map(|r| r.name).collect::<Vec<_>>(), "NAMES is out of step with REAL");
        // One model a gun.
        for r in REAL {
            assert_eq!(REAL.iter().filter(|o| o.gun == r.gun).count(), 1, "{}", r.gun);
        }
    }
}
