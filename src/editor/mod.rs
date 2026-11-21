//! The EDIT page (F8): one creature or plant held in front of the camera,
//! regenerated on a key, with its library entry on the right where every
//! field can be stepped and the result written back to `library/`.
//!
//! The page keeps the entry as the JSON it was read from (`doc`) rather than
//! as a parsed `CreatureDef`, for three reasons: a field nobody has a knob
//! for yet is carried through untouched, EXPORT writes the file in the
//! library's own canonical form (`fmt`, the Rust side of tools/libfmt.py),
//! and a bad edit fails at the parse rather than halfway through a rebuild.
//! Every change reparses the whole entry and rebuilds the subject, so the
//! animal on screen is built by exactly the code the game uses.
//!
//! The subject lives at the centre of a studio: a grey checked platform laid
//! into the world as voxel edits (studio.rs), or, with the platform off, the
//! world's own ground wherever the player is standing.

pub mod cam;
pub mod form;
pub mod panel;
pub mod pose;
pub mod splice;
pub mod studio;
pub mod subject;

use crate::library::Library;
use crate::menu::{Choice, Item};
use serde_json::Value;
use std::path::PathBuf;

/// How much of the subject an edit invalidates. The page used to rebuild
/// everything for every keypress -- drop the animal, recompile its 32x32x32
/// shape, re-slice its rig, skeletonise it and spawn a fresh individual --
/// which is why it blinked and re-faced you each time a number was nudged.
/// Almost nothing needs all that.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Impact {
    /// A colour value: repaint the palette slots the model already points at.
    Paint,
    /// Behaviour, group, variety, rarity: build the species row again and put
    /// it in place. The model, the individual and its pose all stay.
    Row,
    /// The shape, the size, the ages, the rig: build the lot again.
    Whole,
}

/// What stepping the knob at `path` invalidates.
pub fn impact(kind: Kind, path: &str) -> Impact {
    if kind != Kind::Creature {
        // A plant is terrain and a material is a global table; both are
        // already made again from scratch whatever changes.
        return Impact::Whole;
    }
    if path.starts_with("colors.") {
        return Impact::Paint;
    }
    if path.starts_with("behaviour.") || path.starts_with("vary.") || path == "rarity" {
        return Impact::Row;
    }
    Impact::Whole
}

/// What is being edited. Both kinds live in `library/<kind>s/<name>.json`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Creature,
    Plant,
    Material,
    /// The deathmatch guns (src/combat). A weapon is all numbers: its shape
    /// is one of a handful built in Rust that the file picks by name.
    Weapon,
    /// The things the camera goes looking for (src/items.rs): a model built
    /// from ops, like a creature's, with nothing driving it.
    Collectible,
}

impl Kind {
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Creature => "creatures",
            Kind::Plant => "plants",
            Kind::Material => "materials",
            Kind::Weapon => "weapons",
            Kind::Collectible => "collectibles",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Creature => "Creature",
            Kind::Plant => "Plant",
            Kind::Material => "Material",
            Kind::Weapon => "Weapon",
            Kind::Collectible => "Find",
        }
    }

    pub fn other(self) -> Kind {
        match self {
            Kind::Creature => Kind::Plant,
            Kind::Plant => Kind::Material,
            Kind::Material => Kind::Weapon,
            Kind::Weapon => Kind::Collectible,
            Kind::Collectible => Kind::Creature,
        }
    }

    pub const ALL: [Kind; 5] = [Kind::Creature, Kind::Plant, Kind::Material, Kind::Weapon, Kind::Collectible];
}

pub use crate::form::{Num, Val};

/// Where a knob's value is in the entry: a dotted path (`behaviour.wary`),
/// and an element of the array there when the value is one of a pair
/// (`height` is [low, high]).
#[derive(Clone, Copy)]
pub struct Path {
    pub path: &'static str,
    pub at: Option<usize>,
}

/// One line of the page: a path into the entry and how it is stepped.
pub type Knob = crate::form::Knob<Path>;

const fn reach(path: &'static str) -> Path {
    Path { path, at: None }
}

const fn num(page: u8, label: &'static str, path: &'static str, lo: f32, hi: f32, step: f32, dp: u8, help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Num(Num::new(lo, hi, step, dp)),
        reach: reach(path),
    }
}

const fn pair(page: u8, label: &'static str, path: &'static str, at: usize, lo: f32, hi: f32, step: f32, dp: u8, help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Num(Num::new(lo, hi, step, dp)),
        reach: Path { path, at: Some(at) },
    }
}

const fn flag(page: u8, label: &'static str, path: &'static str, help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Flag,
        reach: reach(path),
    }
}

const fn pick(page: u8, label: &'static str, path: &'static str, names: &'static [&'static str], help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Pick(names),
        reach: reach(path),
    }
}

const fn colour(page: u8, label: &'static str, path: &'static str, help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Colour,
        reach: reach(path),
    }
}

const fn mat(page: u8, label: &'static str, path: &'static str, help: &'static str) -> Knob {
    Knob {
        page,
        label,
        help,
        val: Val::Mat,
        reach: reach(path),
    }
}

/// The parts of the day a creature may keep (creatures/diel.rs).
const PERIODS: [&str; 4] = ["diurnal", "nocturnal", "crepuscular", "any"];
/// What a creature leaves behind (creatures/age.rs).
const BONES: [&str; 2] = ["full", "none"];
/// Wingbeat patterns (anim/flight.rs).
const FLIGHTS: [&str; 6] = ["flap", "flap_glide", "bound", "soar", "sail", "hover"];

/// Creature pages: 1 body, 2 colours, 3 behaviour, 4 group, 5 variety.
pub const CREATURE_PAGES: [&str; 5] = ["BODY", "COLOURS", "BEHAVIOUR", "GROUP", "VARIETY"];
/// Plant pages: 1 size, 2 crown, 3 materials.
pub const PLANT_PAGES: [&str; 3] = ["SIZE", "CROWN", "MATERIALS"];
/// Material pages: 1 the texture, 2 the calm style's version of it, 3 what
/// the stuff does beyond its colours.
pub const MATERIAL_PAGES: [&str; 3] = ["TEXTURE", "CALM STYLE", "SURFACE"];

pub fn knobs(kind: Kind) -> &'static [Knob] {
    match kind {
        Kind::Creature => CREATURE_KNOBS,
        Kind::Plant => PLANT_KNOBS,
        Kind::Material => MATERIAL_KNOBS,
        Kind::Weapon => WEAPON_KNOBS,
        Kind::Collectible => COLLECTIBLE_KNOBS,
    }
}

pub fn pages(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Creature => &CREATURE_PAGES,
        Kind::Plant => &PLANT_PAGES,
        Kind::Material => &MATERIAL_PAGES,
        Kind::Weapon => &WEAPON_PAGES,
        Kind::Collectible => &COLLECTIBLE_PAGES,
    }
}

/// How a fire mode throws its damage (library::FireKind).
const FIRE_KINDS: [&str; 3] = ["hitscan", "shell", "beam"];

/// Weapon pages: 1 the gun, 2 and 3 its two fire modes, 4 how it looks.
pub const WEAPON_PAGES: [&str; 4] = ["WEAPON", "FIRE", "ALT FIRE", "LOOK"];
/// Collectible pages: 1 the body, 2 its colours.
pub const COLLECTIBLE_PAGES: [&str; 2] = ["BODY", "COLOURS"];

static WEAPON_KNOBS: &[Knob] = &[
    // ---- 1: the gun ----
    num(1, "SLOT", "slot", 1.0000, 9.0000, 1.0000, 0, "Which digit picks it up and selects it."),
    num(
        1,
        "AMMO MAX",
        "ammo_max",
        0.0000,
        1000.0000,
        5.0000,
        0,
        "Rounds the player may carry. 0 is a weapon that needs no ammunition at all and can never be emptied, which is what the Digger is.",
    ),
    num(1, "AMMO START", "ammo_start", 0.0000, 1000.0000, 5.0000, 0, "Rounds handed out with the weapon."),
    num(
        1,
        "MAG",
        "mag",
        0.0000,
        200.0000,
        1.0000,
        0,
        "Rounds it holds before it has to be reloaded. 0 is a weapon fed straight from its store.",
    ),
    num(1, "RELOAD", "reload_s", 0.0000, 6.0000, 0.0500, 2, "Seconds a reload takes. It does nothing without a magazine."),
    num(
        1,
        "ZOOM",
        "zoom",
        0.0000,
        1.0000,
        0.0100,
        2,
        "How far the right button narrows the view, as a share of the normal field of view: 0.33 is three times magnification. Above 0 the right button is a sight rather than a second fire mode.",
    ),
    // ---- 2: fire ----
    pick(
        2,
        "KIND",
        "fire.kind",
        &FIRE_KINDS,
        "How the mode throws its damage: a ray resolved the instant the trigger goes, a shell that flies and bursts, or a beam held on a target and paid for by the second.",
    ),
    num(2, "RPM", "fire.rpm", 0.0000, 3000.0000, 10.0000, 0, "Rounds a minute while the trigger is held."),
    num(2, "DAMAGE", "fire.damage", 0.0000, 500.0000, 1.0000, 0, "Damage a pellet does, or a shell on a direct hit."),
    num(
        2,
        "PELLETS",
        "fire.pellets",
        1.0000,
        40.0000,
        1.0000,
        0,
        "Rays a shot throws. Above one it is buckshot, and the spread is what makes it a shotgun rather than a rifle.",
    ),
    num(2, "SPREAD", "fire.spread_deg", 0.0000, 30.0000, 0.1000, 1, "Half-angle of the cone the pellets go into, in degrees."),
    num(2, "RANGE", "fire.range", 0.0000, 1000.0000, 5.0000, 0, "How far a ray reaches, in metres."),
    num(
        2,
        "FALLOFF",
        "fire.falloff",
        0.0000,
        1.0000,
        0.0200,
        2,
        "What share of the damage is left at the far end of the range. 1 keeps the lot.",
    ),
    num(
        2,
        "SPIN UP",
        "fire.spin_up",
        0.0000,
        5.0000,
        0.0500,
        2,
        "Seconds of held trigger before the full rate is reached. The minigun's wait, in place of a reload.",
    ),
    num(2, "SPEED", "fire.speed", 0.0000, 400.0000, 2.0000, 0, "Shells: how fast it flies, in metres a second."),
    num(
        2,
        "GRAVITY",
        "fire.gravity",
        0.0000,
        60.0000,
        0.5000,
        1,
        "Shells: how hard it drops, in metres a second squared. 0 flies flat.",
    ),
    num(2, "BOUNCES", "fire.bounces", 0.0000, 10.0000, 1.0000, 0, "Shells: how many times it may bounce before it goes off."),
    num(2, "FUSE", "fire.fuse_s", 0.0000, 20.0000, 0.1000, 1, "Shells: seconds after which it goes off wherever it is."),
    num(2, "PER SHOT", "fire.per_shot", 1.0000, 50.0000, 1.0000, 0, "Rounds spent on one shot."),
    num(2, "KICK", "fire.kick", 0.0000, 10.0000, 0.0500, 2, "Degrees the view climbs with each shot."),
    colour(2, "TRACER", "fire.tracer", "Tracer and shell colour."),
    num(2, "TRACER SECONDS", "fire.tracer_s", 0.0000, 1.0000, 0.0100, 2, "How long a tracer stays up."),
    num(
        2,
        "CHARGE",
        "fire.charge_s",
        0.0000,
        5.0000,
        0.0500,
        2,
        "Seconds of held trigger that build a full charge. 0 fires the instant the trigger goes down; above 0 holds the shot back until the trigger is let go.",
    ),
    num(
        2,
        "CHARGE MULT",
        "fire.charge_mult",
        1.0000,
        8.0000,
        0.1000,
        1,
        "What a full charge is worth: damage, burst and speed all scale between 1 at a tap and this at full.",
    ),
    num(
        2,
        "STICK",
        "fire.stick_s",
        0.0000,
        20.0000,
        0.1000,
        1,
        "Shells: seconds it clings to whatever it lands on instead of going off there.",
    ),
    num(
        2,
        "HEAD MULT",
        "fire.head_mult",
        1.0000,
        6.0000,
        0.0500,
        2,
        "What the damage is multiplied by for a hit in the top of the body. 1 is a weapon that does not care where it lands.",
    ),
    pick(
        2,
        "SHELL LOOK",
        "fire.look",
        &crate::combat::viewmodel::SHELLS,
        "Which little voxel body is drawn for a shell in flight. Left unset it follows from the mode: a bouncing shell is a grenade, one with a burst a rocket, one without a shard.",
    ),
    num(
        2,
        "SPLASH DAMAGE",
        "fire.splash.damage",
        0.0000,
        500.0000,
        1.0000,
        0,
        "Damage at the centre of the burst, falling to nothing at the rim.",
    ),
    num(2, "SPLASH RADIUS", "fire.splash.radius", 0.0000, 30.0000, 0.2000, 1, "How far the burst reaches, in metres."),
    num(
        2,
        "SPLASH PUSH",
        "fire.splash.push",
        0.0000,
        60.0000,
        0.5000,
        1,
        "Metres a second given to whoever is caught at the centre.",
    ),
    num(
        2,
        "SPLASH SELF DAMAGE",
        "fire.splash.self_damage",
        0.0000,
        1.0000,
        0.0500,
        2,
        "How much of its own burst the shooter takes: 0 none, 1 all of it.",
    ),
    num(
        2,
        "SPLASH SELF PUSH",
        "fire.splash.self_push",
        0.0000,
        2.0000,
        0.0500,
        2,
        "How much of its own push the shooter takes. Full push with less than full damage is what makes a rocket jump worth doing.",
    ),
    // ---- 3: alt fire ----
    pick(
        3,
        "KIND",
        "alt.kind",
        &FIRE_KINDS,
        "How the mode throws its damage: a ray resolved the instant the trigger goes, a shell that flies and bursts, or a beam held on a target and paid for by the second.",
    ),
    num(3, "RPM", "alt.rpm", 0.0000, 3000.0000, 10.0000, 0, "Rounds a minute while the trigger is held."),
    num(3, "DAMAGE", "alt.damage", 0.0000, 500.0000, 1.0000, 0, "Damage a pellet does, or a shell on a direct hit."),
    num(
        3,
        "PELLETS",
        "alt.pellets",
        1.0000,
        40.0000,
        1.0000,
        0,
        "Rays a shot throws. Above one it is buckshot, and the spread is what makes it a shotgun rather than a rifle.",
    ),
    num(3, "SPREAD", "alt.spread_deg", 0.0000, 30.0000, 0.1000, 1, "Half-angle of the cone the pellets go into, in degrees."),
    num(3, "RANGE", "alt.range", 0.0000, 1000.0000, 5.0000, 0, "How far a ray reaches, in metres."),
    num(
        3,
        "FALLOFF",
        "alt.falloff",
        0.0000,
        1.0000,
        0.0200,
        2,
        "What share of the damage is left at the far end of the range. 1 keeps the lot.",
    ),
    num(
        3,
        "SPIN UP",
        "alt.spin_up",
        0.0000,
        5.0000,
        0.0500,
        2,
        "Seconds of held trigger before the full rate is reached. The minigun's wait, in place of a reload.",
    ),
    num(3, "SPEED", "alt.speed", 0.0000, 400.0000, 2.0000, 0, "Shells: how fast it flies, in metres a second."),
    num(
        3,
        "GRAVITY",
        "alt.gravity",
        0.0000,
        60.0000,
        0.5000,
        1,
        "Shells: how hard it drops, in metres a second squared. 0 flies flat.",
    ),
    num(3, "BOUNCES", "alt.bounces", 0.0000, 10.0000, 1.0000, 0, "Shells: how many times it may bounce before it goes off."),
    num(3, "FUSE", "alt.fuse_s", 0.0000, 20.0000, 0.1000, 1, "Shells: seconds after which it goes off wherever it is."),
    num(3, "PER SHOT", "alt.per_shot", 1.0000, 50.0000, 1.0000, 0, "Rounds spent on one shot."),
    num(3, "KICK", "alt.kick", 0.0000, 10.0000, 0.0500, 2, "Degrees the view climbs with each shot."),
    colour(3, "TRACER", "alt.tracer", "Tracer and shell colour."),
    num(3, "TRACER SECONDS", "alt.tracer_s", 0.0000, 1.0000, 0.0100, 2, "How long a tracer stays up."),
    num(
        3,
        "CHARGE",
        "alt.charge_s",
        0.0000,
        5.0000,
        0.0500,
        2,
        "Seconds of held trigger that build a full charge. 0 fires the instant the trigger goes down; above 0 holds the shot back until the trigger is let go.",
    ),
    num(
        3,
        "CHARGE MULT",
        "alt.charge_mult",
        1.0000,
        8.0000,
        0.1000,
        1,
        "What a full charge is worth: damage, burst and speed all scale between 1 at a tap and this at full.",
    ),
    num(
        3,
        "STICK",
        "alt.stick_s",
        0.0000,
        20.0000,
        0.1000,
        1,
        "Shells: seconds it clings to whatever it lands on instead of going off there.",
    ),
    num(
        3,
        "HEAD MULT",
        "alt.head_mult",
        1.0000,
        6.0000,
        0.0500,
        2,
        "What the damage is multiplied by for a hit in the top of the body. 1 is a weapon that does not care where it lands.",
    ),
    pick(
        3,
        "SHELL LOOK",
        "alt.look",
        &crate::combat::viewmodel::SHELLS,
        "Which little voxel body is drawn for a shell in flight. Left unset it follows from the mode: a bouncing shell is a grenade, one with a burst a rocket, one without a shard.",
    ),
    num(
        3,
        "SPLASH DAMAGE",
        "alt.splash.damage",
        0.0000,
        500.0000,
        1.0000,
        0,
        "Damage at the centre of the burst, falling to nothing at the rim.",
    ),
    num(3, "SPLASH RADIUS", "alt.splash.radius", 0.0000, 30.0000, 0.2000, 1, "How far the burst reaches, in metres."),
    num(
        3,
        "SPLASH PUSH",
        "alt.splash.push",
        0.0000,
        60.0000,
        0.5000,
        1,
        "Metres a second given to whoever is caught at the centre.",
    ),
    num(
        3,
        "SPLASH SELF DAMAGE",
        "alt.splash.self_damage",
        0.0000,
        1.0000,
        0.0500,
        2,
        "How much of its own burst the shooter takes: 0 none, 1 all of it.",
    ),
    num(
        3,
        "SPLASH SELF PUSH",
        "alt.splash.self_push",
        0.0000,
        2.0000,
        0.0500,
        2,
        "How much of its own push the shooter takes. Full push with less than full damage is what makes a rocket jump worth doing.",
    ),
    // ---- 4: look ----
    pick(
        4,
        "VIEW",
        "view",
        &crate::combat::viewmodel::SHAPES,
        "Which built-in shape is drawn in the player's hands. The shapes are Rust, not data, so a weapon file picks one rather than describing one.",
    ),
    colour(4, "TINT", "tint", "The colour of the flat grip the HUD draws in the corner when the view-model is off."),
];

static COLLECTIBLE_KNOBS: &[Knob] = &[
    num(
        1,
        "VOXEL",
        "voxel",
        0.0020,
        0.2000,
        0.0010,
        4,
        "The model's voxel edge in metres. The model grid is 32 voxels on a side, so this sets how big the thing is overall.",
    ),
    flag(1, "SPIN", "spin", "Turns slowly in place where it lies, for something meant to catch the eye."),
];

static CREATURE_KNOBS: &[Knob] = &[
    // ---- 1: body ----
    num(
        1,
        "VOXEL",
        "voxel",
        0.01,
        0.4,
        0.0025,
        4,
        "The model's voxel edge in metres. The model grid is 32 voxels on a side, so this sets how big the animal is overall: everything else about its shape is in the ops.",
    ),
    num(
        1,
        "HALF WIDTH",
        "half_w",
        0.02,
        3.0,
        0.01,
        2,
        "Half the collision box's width in metres. It decides what the animal can squeeze past, not how it looks.",
    ),
    num(
        1,
        "HEIGHT",
        "height",
        0.05,
        8.0,
        0.05,
        2,
        "The collision box's height in metres. It also decides which ages the animal has and whether it leaves bones, unless the ages block says otherwise.",
    ),
    num(
        1,
        "JUVENILE SCALE",
        "ages.juvenile",
        0.0,
        1.0,
        0.02,
        2,
        "How big a juvenile is beside an adult. 0 means this animal has no young stage: every one spawns full grown.",
    ),
    flag(1, "AGED", "ages.aged", "Whether old individuals turn up, greyer, slower and stiffer than the rest."),
    pick(
        1,
        "BONES",
        "ages.bones",
        &BONES,
        "What is left when it dies: a skeleton (full) or nothing. Left unset it follows from the height -- half a metre and up leaves bones.",
    ),
    num(
        1,
        "RARITY",
        "rarity",
        0.0,
        1.0,
        0.005,
        3,
        "Above 0 this entry is a rare morph of the creature it is `like`: this share of that creature's spawns are this one instead.",
    ),
    // ---- 3: behaviour ----
    pick(
        3,
        "MOVEMENT",
        "behaviour.movement",
        &crate::creatures::locomotion::MOVEMENTS,
        "The movement verb that drives it (creatures/locomotion.rs): what kind of animal the engine treats it as.",
    ),
    num(3, "WALK SPEED", "behaviour.walk_speed", 0.1, 20.0, 0.1, 1, "Metres a second at its ordinary pace."),
    num(3, "FLEE SPEED", "behaviour.flee_speed", 0.1, 40.0, 0.1, 1, "Metres a second once it is running."),
    num(
        3,
        "WARY",
        "behaviour.wary",
        0.0,
        80.0,
        0.5,
        1,
        "The distance in metres at which it notices the player or a predator, and so the distance at which it bolts.",
    ),
    num(
        3,
        "FLIGHTINESS",
        "behaviour.flightiness",
        0.0,
        3.0,
        0.05,
        2,
        "How readily it bolts once you are inside its wary distance: 0 carries on as if nobody were there, 1 an ordinary wild animal, 2 a hare.",
    ),
    num(
        3,
        "JUVENILE FLIGHTINESS",
        "behaviour.juvenile_flightiness",
        0.0,
        3.0,
        0.05,
        2,
        "A juvenile's flightiness beside an adult's. Below 1 where the young stand and stare.",
    ),
    pick(
        3,
        "ACTIVE",
        "behaviour.active",
        &PERIODS,
        "The part of the day it keeps. Outside those hours few are abroad, and the ones that sleep where they stand are bedded down.",
    ),
    num(
        3,
        "OFF HOURS",
        "behaviour.off_hours",
        0.0,
        1.0,
        0.05,
        2,
        "The share of it still abroad outside its own hours. 0 means it is never seen then.",
    ),
    num(
        3,
        "SPAWN CHANCE",
        "behaviour.spawn_chance",
        0.0,
        1.0,
        0.02,
        2,
        "The chance, each half-second check while its biome wants one, that a missing one turns up. Below 1 makes it rare.",
    ),
    flag(3, "SWIMS", "behaviour.swims", "Crosses water too deep to wade by swimming instead of turning back at it."),
    num(
        3,
        "FLY HEIGHT",
        "behaviour.fly_height",
        0.0,
        200.0,
        1.0,
        0,
        "How high above the ground or sea it flies, for the flying movements.",
    ),
    pick(
        3,
        "FLIGHT",
        "behaviour.flight",
        &FLIGHTS,
        "The wingbeat pattern for a rigged bird. Unset, it follows from the movement.",
    ),
    num(3, "WINGBEAT", "behaviour.wingbeat", 0.0, 30.0, 0.25, 2, "Wingbeats a second. Unset, it follows from the wingspan."),
    flag(3, "FLOATS", "behaviour.floats", "A seabird that comes down off its circle now and then to sit on the sea."),
    flag(3, "AWASH", "behaviour.awash", "Cruises with its back and eyes out of the water rather than under it: a crocodile."),
    num(
        3,
        "SHALLOWS",
        "behaviour.shallows",
        0.0,
        20.0,
        0.2,
        1,
        "The least water in metres a swimmer keeps to, and so where it is found.",
    ),
    num(
        3,
        "LEAP",
        "behaviour.leap",
        0.0,
        10.0,
        0.1,
        2,
        "How often it leaps clear of the water: leaps a fish a minute for a school, or the chance a dive ends in a breach.",
    ),
    num(3, "BLINK", "behaviour.blink", 0.0, 10.0, 0.1, 2, "Flashes of its glowing colours a second. 0 glows steady."),
    flag(
        3,
        "SLEEPS ON SIDE",
        "behaviour.sleeps_on_side",
        "Lies down on its side and tips over to get there. Dogs, cats and seals do; deer and cattle don't.",
    ),
    flag(
        3,
        "WARNS",
        "behaviour.warns",
        "Holds its ground and warns -- coiled, head up -- while anything presses on it, instead of leaving.",
    ),
    // ---- 4: group ----
    pair(4, "GROUP LEAST", "behaviour.group.size", 0, 1.0, 60.0, 1.0, 0, "The fewest that travel together."),
    pair(4, "GROUP MOST", "behaviour.group.size", 1, 1.0, 400.0, 1.0, 0, "The most that travel together."),
    num(4, "SPACING", "behaviour.group.spacing", 0.2, 30.0, 0.1, 1, "How far apart in metres they keep from each other."),
    num(
        4,
        "RANGE",
        "behaviour.group.range",
        1.0,
        200.0,
        1.0,
        0,
        "How far in metres the group holds together before a straggler turns back.",
    ),
    num(4, "ALIGNMENT", "behaviour.group.alignment", 0.0, 3.0, 0.05, 2, "How strongly each one matches its neighbours' heading."),
    num(
        4,
        "COHESION",
        "behaviour.group.cohesion",
        0.0,
        3.0,
        0.05,
        2,
        "How strongly each one pulls back toward the middle of the group.",
    ),
    // ---- 5: variety ----
    num(
        5,
        "SIZE SPREAD",
        "vary.size",
        0.0,
        0.5,
        0.01,
        2,
        "How much individuals differ in body size, either side of the species' own.",
    ),
    num(5, "SHADE SPREAD", "vary.shade", 0.0, 0.5, 0.01, 2, "How much the coat's brightness differs between individuals."),
    num(5, "HUE SPREAD", "vary.hue", 0.0, 0.5, 0.01, 2, "How much warmer or cooler one individual's coat is than another's."),
    num(5, "SPEED SPREAD", "vary.speed", 0.0, 0.5, 0.01, 2, "How much individuals differ in walking and running speed."),
    num(
        5,
        "WARY SPREAD",
        "vary.wary",
        0.0,
        1.0,
        0.02,
        2,
        "How much individuals differ in the distance at which they notice you.",
    ),
    num(
        5,
        "HABITS SPREAD",
        "vary.habits",
        0.0,
        1.5,
        0.05,
        2,
        "How much their tastes differ: at 0.4 one deer grazes half again as often as another and lies down half as often.",
    ),
];

static MATERIAL_KNOBS: &[Knob] = &[
    // ---- 1: the texture ----
    pick(
        1,
        "PATTERN",
        "texture.pattern",
        &crate::render::recipes::PATTERNS,
        "The pattern verb that paints the tile (render/recipes.rs). The two colours below are what it paints with.",
    ),
    colour(1, "BASE", "texture.base", "The ground colour of the tile: what most of it is."),
    colour(1, "ALT", "texture.alt", "The second colour the pattern works in: the speck, the crack, the course of stone."),
    num(
        1,
        "SCALE",
        "texture.scale",
        1.0,
        32.0,
        1.0,
        0,
        "Pattern cells across a one-metre tile. Low is coarse and reads from far off; high turns to noise at a distance.",
    ),
    num(
        1,
        "CONTRAST",
        "texture.contrast",
        0.0,
        3.0,
        0.05,
        2,
        "How far apart the two colours are held. Below 1 pulls them together, above 1 pushes them apart.",
    ),
    num(
        1,
        "GLOW",
        "glow",
        0.0,
        1.5,
        0.05,
        2,
        "Light of its own, drawn bright in shade and at night: bioluminescent moss, a cave ceiling. 0 for ordinary stuff.",
    ),
    num(
        1,
        "HURT",
        "hurt",
        0.0,
        63.0,
        1.0,
        0,
        "Damage a second to anything standing in it: slag, lava, acid. Over about 20 is instant death rather than a thing to run across.",
    ),
    // ---- 2: the calm style ----
    pick(
        2,
        "CALM PATTERN",
        "texture.calm.pattern",
        &crate::render::recipes::PATTERNS,
        "A different pattern verb in the saturated style. Unset, it keeps the one above.",
    ),
    colour(
        2,
        "CALM BASE",
        "texture.calm.base",
        "The ground colour in the saturated style, where the palette is flatter and warmer.",
    ),
    colour(2, "CALM ALT", "texture.calm.alt", "The second colour in the saturated style."),
    num(
        2,
        "CALM SCALE",
        "texture.calm.scale",
        1.0,
        32.0,
        1.0,
        0,
        "Pattern cells a metre in the saturated style, where it differs.",
    ),
    num(
        2,
        "CALM CONTRAST",
        "texture.calm.contrast",
        0.0,
        3.0,
        0.05,
        2,
        "Scales the contrast above in the saturated style. That style already softens every pattern, so this is on top of it.",
    ),
    // ---- 3: what it does ----
    flag(3, "SMOOTH", "smooth", "Ground the mesher smooths into slopes rather than steps."),
    flag(
        3,
        "CRISP",
        "crisp",
        "Its border with other ground is drawn sharp rather than blended with noise: paving, road surfaces.",
    ),
    flag(3, "PASSABLE", "passable", "Bodies pass through it: hanging vines and other thin growth."),
    flag(
        3,
        "ROUND",
        "round",
        "Rock the mesher rounds like a tree crown: free-standing natural forms such as hoodoos, never ground the land is made of.",
    ),
    flag(3, "FACET", "facet", "Rounded rock drawn in flat facets rather than soft blobs. Implies ROUND."),
    flag(3, "WATER", "water", "Water drawn as solid voxels: walking creatures keep out of it."),
    flag(
        3,
        "UNDERWATER",
        "underwater",
        "Its detail sprites grow under the sea as well as over it: eelgrass and the other drowned meadows.",
    ),
    flag(3, "FLOATING", "floating", "Its sprites lie flat on the sea's surface above where they grow: lily pads."),
    flag(3, "GLOW SPECKS", "glow_specks", "Only the bright specks of its texture give light, not the rock between them."),
    flag(3, "FOLIAGE", "foliage", "Leaves or needles: the bike brushes through and a rope will not hold."),
    flag(3, "GLINTS", "glints", "Single texels flash in the sun, as glass does."),
];

static PLANT_KNOBS: &[Knob] = &[
    // ---- 1: size ----
    pick(1, "FORM", "form", crate::world::flora::Form::NAMES, "The growth form verb that draws it (world/flora.rs)."),
    pair(1, "HEIGHT LOW", "height", 0, 0.1, 80.0, 0.1, 1, "The shortest this plant grows, in metres."),
    pair(1, "HEIGHT HIGH", "height", 1, 0.1, 80.0, 0.1, 1, "The tallest this plant grows, in metres."),
    pair(1, "TRUNK LOW", "trunk_radius", 0, 0.0, 6.0, 0.01, 2, "The thinnest trunk, in metres of radius."),
    pair(1, "TRUNK HIGH", "trunk_radius", 1, 0.0, 6.0, 0.01, 2, "The thickest trunk, in metres of radius."),
    num(
        1,
        "BUTTRESS",
        "buttress",
        0.0,
        4.0,
        0.05,
        2,
        "Root flare: extra trunk radius at the ground in metres, in lobes like buttress roots. 0 for none.",
    ),
    num(1, "LEAN", "lean", 0.0, 1.2, 0.02, 2, "The largest lean of the trunk, in radians."),
    num(1, "MAX SLOPE", "max_slope", 0.0, 2.0, 0.05, 2, "The steepest ground it will grow on."),
    num(
        1,
        "RARITY",
        "rarity",
        0.0,
        1.0,
        0.005,
        3,
        "Above 0 this entry is a rare variety of the plant it is `like`: this share of that plant's places grow this one instead.",
    ),
    // ---- 2: crown ----
    pair(2, "CROWN LOW", "crown_radius", 0, 0.1, 30.0, 0.1, 1, "The narrowest crown, in metres of radius."),
    pair(2, "CROWN HIGH", "crown_radius", 1, 0.1, 30.0, 0.1, 1, "The widest crown, in metres of radius."),
    num(2, "SQUASH", "squash", 0.1, 2.0, 0.05, 2, "Vertical scale of the leaf blobs. Below 1 flattens the crown."),
    num(
        2,
        "CROWN BASE",
        "crown_base",
        0.0,
        0.95,
        0.02,
        2,
        "Conifers: where the needles start, as a share of the height. Ponderosa pines carry theirs high on a bare trunk.",
    ),
    num(
        2,
        "SPREAD",
        "spread",
        0.0,
        1.0,
        0.05,
        2,
        "Branching forms: how far the first limbs reach out sideways and how many there are. 1 gives a wide old crown.",
    ),
    num(
        2,
        "DOME",
        "dome",
        0.0,
        1.0,
        0.05,
        2,
        "Branching forms: how far the tip clusters are filled out into one domed shell of foliage over the limbs. 0 leaves separate clumps.",
    ),
    num(
        2,
        "LIMB",
        "limb",
        0.0,
        1.0,
        0.02,
        2,
        "Branching forms: a first limb's radius as a share of the trunk's. More for thick-limbed trees like cottonwoods.",
    ),
    num(
        2,
        "FILL",
        "fill",
        0.0,
        1.0,
        0.05,
        2,
        "Canopy `lobes`: how often the notch between two neighbouring lobes is bridged. 0 leaves every notch open.",
    ),
    pick(2, "CURVE", "curve", crate::world::flora::Curve::NAMES, "Branching forms: the shape a limb takes."),
    pick(2, "CANOPY", "canopy", crate::world::flora::Canopy::NAMES, "Branching forms: how the foliage is massed."),
    num(
        2,
        "CANE LEAN",
        "cane_lean",
        0.0,
        1.4,
        0.02,
        2,
        "Form `canes`: the most a cane leans from upright at its root, in radians. An ocotillo's fan is about 0.54.",
    ),
    num(
        2,
        "CANE BEND",
        "cane_bend",
        0.0,
        0.3,
        0.01,
        2,
        "Canes with tufts: how far a cane arches toward its tip. 0 stands straight; a buckthorn is about 0.07.",
    ),
    num(2, "CANE LEAVES", "cane_leaves", 0.0, 1.0, 0.05, 2, "Canes with tufts: the chance of a leaf tuft at each upper joint. Low for near-leafless switches."),
    num(2, "CANE SPURS", "cane_spurs", 0.0, 1.0, 0.05, 2, "Canes with tufts: the chance of a side spur on each segment. 0 for clean stems."),
    // ---- 3: materials ----
    mat(3, "BARK", "bark", "The material the trunk and limbs are drawn in."),
    mat(3, "LEAF", "leaf", "The material the foliage is drawn in."),
    mat(3, "AUTUMN", "autumn", "The leaf material in a world whose season is autumn."),
    mat(3, "FLOWER", "flower", "The material its flowers are drawn in, where the form has them."),
];

/// What a dropdown on the page is open on: the kind, the entry, or one of
/// the knobs (its index, after `DROP_KNOB`).
pub const DROP_KIND: u16 = 0;
pub const DROP_ENTRY: u16 = 1;
pub const DROP_POSE: u16 = 2;
pub const DROP_KNOB: u16 = 3;

/// A library name as the page shows it: `cave_bear` reads as CAVE BEAR, and
/// the menu's own small caps do the rest.
pub fn shown_name(name: &str) -> String {
    name.replace('_', " ")
}

/// The EDIT page's state: which entry, the entry as JSON, and how the subject
/// in front of the camera is being shown.
pub struct Editor {
    pub kind: Kind,
    /// Every entry name of this kind, in library order.
    pub names: Vec<String>,
    /// Which of `names` is loaded.
    pub at: usize,
    /// The entry as read from its file, with every edit applied.
    pub doc: Value,
    /// The entry's top-level keys in the order its file had them, so EXPORT
    /// keeps the order for keys libfmt has no fixed place for.
    order: Vec<String>,
    /// The file exactly as it was read. WRITE THE FILE splices the edited
    /// values into this rather than writing the parsed entry out again:
    /// that is what keeps a hand-written entry's own key order and layout
    /// intact, so a one-knob edit shows up as a one-line diff.
    text: String,
    /// The paths edited since the file was read, in the order they were
    /// first touched.
    edited: Vec<String>,
    /// A library of this world's materials, the page's own copy, loaded the
    /// first time a material is edited. The materials the engine draws with
    /// are a global table installed from a library (`matlib::install`), so
    /// changing one live means installing a whole library again -- this one,
    /// with the edited material in it.
    pub mats: Option<crate::library::Library>,
    /// The entry's parent chain, merged (src/inherit.rs). An entry with
    /// `like` is only half an entry on its own -- an alligator's file is its
    /// colours and nothing else -- so the page reads every value through the
    /// chain and writes only to the file in front of it.
    base: Value,
    /// The roll that decides this individual: its size, coat, habits and, for
    /// a plant, its whole shape. REGENERATE moves it on.
    pub seed: u32,
    /// Which age the subject is shown at (creatures/age.rs).
    pub stage: crate::creatures::age::Stage,
    /// What the subject is doing: a pose it holds, or LIVE, which turns it
    /// loose in the world (editor/pose.rs).
    pub pose: pose::Pose,
    /// The grey platform is laid in; off, the subject stands on the world's
    /// own ground.
    pub grid: bool,
    /// Lit by the world's own hour, weather and grade rather than the
    /// studio's fixed sun (`Game::studio_light`).
    pub world_light: bool,
    /// What the edits made since the last apply invalidate, if anything.
    /// Edits are collected here and applied once a frame rather than on each
    /// keypress, so holding an arrow down does not build sixty models a
    /// second.
    pub pending: Option<Impact>,
    /// Which headings of the form are shut, a bit to a heading
    /// (editor/form.rs). All open to begin with: the point of one scroll is
    /// that the whole entry is in it.
    pub closed: u32,
    /// The scrolling body's first visible row and how many it shows, kept
    /// here rather than on the `Menu` because a list dropped open over the
    /// panel draws into a menu of its own.
    pub top: usize,
    pub shown: usize,
    /// What the last action said, shown at the foot of the page.
    pub msg: Option<String>,
    /// The entry has edits that are not written to its file.
    pub dirty: bool,
    /// CLOSE has been pressed once on a dirty entry and said so. A second
    /// press discards the edits; any further edit clears it, so the warning
    /// comes again.
    pub asked_discard: bool,
}

impl Editor {
    /// Open the page on `name`, or on the first entry of its kind.
    pub fn new(lib: &Library, kind: Kind, name: Option<&str>) -> Result<Editor, String> {
        let names: Vec<String> = match kind {
            Kind::Creature => lib.creatures.iter().map(|c| c.name.clone()).collect(),
            Kind::Plant => lib.plants.iter().map(|p| p.name.clone()).collect(),
            // A plant's coats generate materials of their own at load
            // (`coat_of`); they are not files, so the page does not offer
            // them.
            Kind::Material => lib.materials.iter().filter(|m| m.coat_of.is_none()).map(|m| m.name.clone()).collect(),
            Kind::Weapon => lib.weapons.iter().map(|w| w.name.clone()).collect(),
            Kind::Collectible => lib.collectibles.iter().map(|c| c.name.clone()).collect(),
        };
        if names.is_empty() {
            return Err(format!("the library has no {}", kind.folder()));
        }
        let at = name.and_then(|n| names.iter().position(|x| x == n)).unwrap_or(0);
        let mut e = Editor {
            kind,
            names,
            at,
            doc: Value::Null,
            order: Vec::new(),
            text: String::new(),
            edited: Vec::new(),
            base: Value::Null,
            mats: None,
            seed: 0x51ed_9a3b,
            stage: crate::creatures::age::Stage::Adult,
            pose: pose::Pose::Rest,
            grid: true,
            world_light: false,
            pending: None,
            closed: 0,
            top: 0,
            shown: 0,
            msg: None,
            dirty: false,
            asked_discard: false,
        };
        e.read()?;
        Ok(e)
    }

    pub fn name(&self) -> &str {
        &self.names[self.at]
    }

    pub fn path(&self) -> PathBuf {
        crate::library::data_root().join("library").join(self.kind.folder()).join(format!("{}.json", self.name()))
    }

    /// Read the entry's file into `doc`, losing any unexported edits.
    pub fn read(&mut self) -> Result<(), String> {
        let path = self.path();
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.doc = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.order = key_order(&text);
        self.text = text;
        self.edited.clear();
        self.base = self.parents();
        self.dirty = false;
        self.asked_discard = false;
        Ok(())
    }

    /// Move to another entry of the same kind, or another kind.
    pub fn open(&mut self, kind: Kind, at: usize, lib: &Library) {
        if kind != self.kind {
            let name = self.name().to_string();
            match Editor::new(lib, kind, Some(&name)) {
                Ok(e) => {
                    self.kind = e.kind;
                    self.names = e.names;
                    self.at = e.at;
                    self.doc = e.doc;
                    self.order = e.order;
                    self.text = e.text;
                    self.edited = e.edited;
                    self.base = e.base;
                    self.msg = None;
                    self.dirty = false;
                    self.asked_discard = false;
                    self.wants(Impact::Whole);
                }
                Err(e) => self.msg = Some(e),
            }
            return;
        }
        self.at = at.min(self.names.len() - 1);
        self.wants(Impact::Whole);
        if let Err(e) = self.read() {
            self.msg = Some(e);
        } else {
            self.msg = None;
        }
    }

    /// The entry's parents merged, deepest first, or null where it has none.
    /// Read from their files, since the loader keeps parsed entries rather
    /// than the JSON they came from.
    fn parents(&self) -> Value {
        let folder = crate::library::data_root().join("library").join(self.kind.folder());
        let mut chain: Vec<Value> = Vec::new();
        let mut at = self.doc.clone();
        for _ in 0..8 {
            let Some(name) = at.get("like").and_then(|v| v.as_str()) else { break };
            let Ok(text) = std::fs::read_to_string(folder.join(format!("{name}.json"))) else { break };
            let Ok(v): Result<Value, _> = serde_json::from_str(&text) else { break };
            at = v.clone();
            chain.push(v);
        }
        let mut out = Value::Null;
        for v in chain.into_iter().rev() {
            out = if out.is_null() { v } else { crate::inherit::over(&out, &v) };
        }
        out
    }

    /// The entry as the engine reads it: the file over its parents.
    pub fn resolved(&self) -> Value {
        if self.base.is_null() {
            self.doc.clone()
        } else {
            crate::inherit::over(&self.base, &self.doc)
        }
    }

    /// The colours this page paints: a creature's own slots, sorted as the
    /// model builder sorts them, or the colour knobs of whatever page this
    /// is. Each is a label and the path it writes to.
    pub fn colour_rows(&self, page: u8) -> Vec<(String, String)> {
        if pages(self.kind).get(page as usize - 1) == Some(&"COLOURS") {
            let full = self.resolved();
            let Some(c) = full.get("colors").and_then(|v| v.as_object()) else {
                return Vec::new();
            };
            let mut names: Vec<String> = c.keys().cloned().collect();
            names.sort();
            return names.iter().map(|n| (n.to_uppercase(), format!("colors.{n}"))).collect();
        }
        knobs(self.kind)
            .iter()
            .filter(|k| k.page == page && matches!(k.val, Val::Colour))
            .map(|k| (k.label.to_string(), k.reach.path.to_string()))
            .collect()
    }

    /// What 1.0 means in this kind's colour values. A creature, a find and a
    /// material paint in 0..255 sRGB; a weapon's tracer and tint are the
    /// linear 0..1 the renderer wants, because they are mixed into light
    /// rather than laid on a voxel.
    pub fn colour_max(&self) -> f64 {
        if self.kind == Kind::Weapon {
            1.0
        } else {
            255.0
        }
    }

    /// One colour row's sRGB, 0..1 per channel.
    pub fn colour(&self, page: u8, i: usize) -> [f32; 3] {
        let rows = self.colour_rows(page);
        let Some((_, path)) = rows.get(i) else { return [0.0; 3] };
        let full = self.resolved();
        let v = get(&full, path).and_then(|v| v.as_array());
        let max = self.colour_max() as f32;
        let at = |k: usize| v.and_then(|a| a.get(k)).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 / max;
        [at(0), at(1), at(2)]
    }

    /// What that row is called, for the picker's heading.
    pub fn colour_label(&self, page: u8, i: usize) -> String {
        self.colour_rows(page).get(i).map(|(l, _)| l.clone()).unwrap_or_default()
    }

    /// Paint one colour row, keeping any fourth (emissive) value it had.
    pub fn set_colour(&mut self, page: u8, i: usize, c: [f32; 3]) {
        let rows = self.colour_rows(page);
        let Some((_, path)) = rows.get(i).cloned() else { return };
        let glow = get(&self.resolved(), &path).and_then(|v| v.as_array()).and_then(|a| a.get(3)).and_then(|x| x.as_f64());
        let max = self.colour_max();
        let dp = if max > 1.5 { 0 } else { 3 };
        let mut out: Vec<Value> = c.iter().map(|v| round_to(*v * max as f32, dp).into()).collect();
        if let Some(g) = glow {
            out.push(g.into());
        }
        // The colour goes into the file in front of us, even when the slot
        // it paints came from a parent.
        set(&mut self.doc, &path, Value::Array(out));
        self.touched(&path);
    }

    /// Step knob `i` of the whole table by `dir`, or toggle it.
    pub fn step(&mut self, i: usize, dir: i32, lib: &Library) {
        let Some(k) = knobs(self.kind).get(i) else { return };
        let full = self.resolved();
        let now = get(&full, k.reach.path);
        let next = match k.val {
            Val::Num(n) => {
                let here = match k.reach.at {
                    Some(j) => now.and_then(|v| v.as_array()).and_then(|a| a.get(j)).and_then(|x| x.as_f64()).map(|v| v as f32),
                    None => now.and_then(|v| v.as_f64()).map(|v| v as f32),
                };
                // A number that is not there starts from the low end, so one
                // press turns an unset field on.
                let v = here.unwrap_or(n.lo);
                let v = if here.is_none() { n.lo } else { n.nudge(v, dir) };
                let v = Value::from(round_to(v, n.dp));
                match k.reach.at {
                    Some(j) => {
                        let mut a: Vec<Value> = now.and_then(|v| v.as_array()).cloned().unwrap_or_else(|| vec![Value::from(n.lo), Value::from(n.lo)]);
                        while a.len() <= j {
                            a.push(Value::from(n.lo));
                        }
                        a[j] = v;
                        Value::Array(a)
                    }
                    None => v,
                }
            }
            Val::Flag => Value::Bool(!now.and_then(|v| v.as_bool()).unwrap_or(false)),
            Val::Pick(names) => {
                let here = now.and_then(|v| v.as_str());
                Value::String(cycle(names, here, dir).to_string())
            }
            Val::Mat => {
                let all: Vec<&str> = lib.materials.iter().map(|m| m.name.as_str()).collect();
                let here = now.and_then(|v| v.as_str());
                Value::String(cycle(&all, here, dir).to_string())
            }
            // A colour is painted in the picker, not stepped; left and right
            // walk its brightness, which is the one thing worth a key.
            Val::Colour => {
                let max = self.colour_max();
                let v = now.and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let at = |k: usize| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                let f = if dir > 0 { 1.06 } else { 1.0 / 1.06 };
                let dp = if max > 1.5 { 0 } else { 3 };
                Value::Array((0..3).map(|k| Value::from(round_to((at(k) * f).clamp(0.0, max) as f32, dp))).collect())
            }
        };
        set(&mut self.doc, k.reach.path, next);
        self.touched(k.reach.path);
    }

    /// Note that a path has been edited, for the splice at export, and what
    /// rebuilding it costs.
    fn touched(&mut self, path: &str) {
        if !self.edited.iter().any(|p| p == path) {
            self.edited.push(path.to_string());
        }
        self.dirty = true;
        self.asked_discard = false;
        self.wants(impact(self.kind, path));
    }

    /// Ask for a rebuild at least this deep before the next frame is drawn.
    pub fn wants(&mut self, how: Impact) {
        self.pending = Some(match self.pending {
            Some(was) => was.max(how),
            None => how,
        });
    }

    /// The value a knob shows.
    pub fn shown(&self, i: usize) -> String {
        let Some(k) = knobs(self.kind).get(i) else { return String::new() };
        let full = self.resolved();
        let now = get(&full, k.reach.path);
        match k.val {
            Val::Num(n) => {
                let here = match k.reach.at {
                    Some(j) => now.and_then(|v| v.as_array()).and_then(|a| a.get(j)).and_then(|x| x.as_f64()),
                    None => now.and_then(|v| v.as_f64()),
                };
                match here {
                    Some(v) => n.text(v as f32),
                    None => "--".to_string(),
                }
            }
            Val::Flag => match now.and_then(|v| v.as_bool()) {
                Some(true) => "On".to_string(),
                Some(false) => "Off".to_string(),
                None => "--".to_string(),
            },
            Val::Pick(_) | Val::Mat => now.and_then(|v| v.as_str()).map(shown_name).unwrap_or_else(|| "--".to_string()),
            Val::Colour => match now.and_then(|v| v.as_array()) {
                Some(a) if a.len() >= 3 => {
                    let max = self.colour_max();
                    let b = |k: usize| (a[k].as_f64().unwrap_or(0.0) / max * 255.0).round().clamp(0.0, 255.0) as u32;
                    format!("#{:02X}{:02X}{:02X}", b(0), b(1), b(2))
                }
                _ => "--".to_string(),
            },
        }
    }

    /// The names a row offers, for the list that drops under it.
    pub fn options(&self, which: u16, lib: &Library) -> Vec<String> {
        match which {
            DROP_KIND => Kind::ALL.iter().map(|k| k.label().to_string()).collect(),
            DROP_ENTRY => self.names.clone(),
            DROP_POSE => self.poses().iter().map(|p| p.name().to_string()).collect(),
            w => match knobs(self.kind).get((w - DROP_KNOB) as usize).map(|k| k.val) {
                Some(Val::Pick(names)) => names.iter().map(|x| x.to_string()).collect(),
                Some(Val::Mat) => lib.materials.iter().map(|m| m.name.clone()).collect(),
                _ => Vec::new(),
            },
        }
    }

    /// Whether row `which` has a list to drop at all.
    pub fn has_drop(&self, which: u16) -> bool {
        matches!(which, DROP_KIND | DROP_ENTRY | DROP_POSE) || matches!(knobs(self.kind).get((which.saturating_sub(DROP_KNOB)) as usize).map(|k| k.val), Some(Val::Pick(_)) | Some(Val::Mat))
    }

    /// The lines of that list.
    pub fn drop_items(&self, which: u16, lib: &Library) -> Vec<Item> {
        self.options(which, lib)
            .iter()
            .enumerate()
            .map(|(i, name)| Item::new(&shown_name(name), Choice::EditChoose(i as u16)))
            .collect()
    }

    /// Which line of that list is the one in force, so it opens on it.
    pub fn drop_at(&self, which: u16, lib: &Library) -> usize {
        // The raw value, not the one on screen: a name with an underscore
        // in it reads as two words on the page and would never match.
        let here = match which {
            DROP_KIND => self.kind.label().to_string(),
            DROP_ENTRY => self.name().to_string(),
            DROP_POSE => self.pose.name().to_string(),
            w => {
                let full = self.resolved();
                knobs(self.kind)
                    .get((w - DROP_KNOB) as usize)
                    .and_then(|k| get(&full, k.reach.path))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            }
        };
        self.options(which, lib).iter().position(|x| *x == here).unwrap_or(0)
    }

    /// Take line `i` of the list open on row `which`. Returns true when the
    /// entry itself changed, so the caller knows to rebuild.
    pub fn take(&mut self, which: u16, i: usize, lib: &Library) {
        let Some(name) = self.options(which, lib).get(i).cloned() else { return };
        match which {
            DROP_KIND => {
                let kind = Kind::ALL.iter().copied().find(|k| k.label() == name).unwrap_or(Kind::Creature);
                if kind != self.kind {
                    self.open(kind, 0, lib);
                }
            }
            DROP_ENTRY => self.open(self.kind, i, lib),
            DROP_POSE => {
                if let Some(p) = self.poses().into_iter().find(|p| p.name() == name) {
                    self.pose = p;
                    self.wants(Impact::Row);
                }
            }
            w => {
                let Some(k) = knobs(self.kind).get((w - DROP_KNOB) as usize) else { return };
                set(&mut self.doc, k.reach.path, Value::String(name));
                self.touched(k.reach.path);
            }
        }
    }

    /// Every row of the panel, in the order the keyboard walks them
    /// (editor/form.rs builds them).
    pub fn items(&self) -> Vec<Item> {
        self.rows().into_iter().map(|r| r.item).collect()
    }

    /// Keep the highlight inside the scrolling body, bringing it into view.
    pub fn reveal(&mut self, at: usize) {
        let (lo, hi) = self.body_range();
        if self.shown == 0 || at < lo || at >= hi {
            return;
        }
        let n = hi - lo;
        let k = at - lo;
        if k < self.top {
            self.top = k;
        } else if k >= self.top + self.shown {
            self.top = k + 1 - self.shown;
        }
        self.top = self.top.min(n.saturating_sub(self.shown));
    }

    /// Roll the body by `lines`, positive down.
    pub fn scroll(&mut self, lines: i32) {
        let (lo, hi) = self.body_range();
        let max = (hi - lo).saturating_sub(self.shown);
        self.top = (self.top as i32 + lines).clamp(0, max as i32) as usize;
    }

    pub fn title(&self) -> String {
        shown_name(self.name()).to_uppercase()
    }

    /// The line under the list: what the last action said, or the keys.
    pub fn foot(&self) -> String {
        match &self.msg {
            Some(m) => m.clone(),
            None => "drag to turn, wheel zooms, F frames, T spins, Esc closes".to_string(),
        }
    }

    /// The entry parsed as the engine reads it. Any unknown key or bad value
    /// comes back as the error, with nothing changed on screen.
    pub fn creature(&self) -> Result<crate::library::CreatureDef, String> {
        let mut unknown = Vec::new();
        let def: crate::library::CreatureDef = crate::library::parse_known(self.resolved(), |k| unknown.push(k)).map_err(|e| e.to_string())?;
        if !unknown.is_empty() {
            return Err(format!("unknown key {}", unknown.join(", ")));
        }
        Ok(def)
    }

    pub fn material(&self) -> Result<crate::library::MaterialDef, String> {
        let mut unknown = Vec::new();
        let def: crate::library::MaterialDef = crate::library::parse_known(self.resolved(), |k| unknown.push(k)).map_err(|e| e.to_string())?;
        if !unknown.is_empty() {
            return Err(format!("unknown key {}", unknown.join(", ")));
        }
        Ok(def)
    }

    pub fn weapon(&self) -> Result<crate::library::WeaponDef, String> {
        let mut unknown = Vec::new();
        let def: crate::library::WeaponDef = crate::library::parse_known(self.resolved(), |k| unknown.push(k)).map_err(|e| e.to_string())?;
        if !unknown.is_empty() {
            return Err(format!("unknown key {}", unknown.join(", ")));
        }
        Ok(def)
    }

    pub fn collectible(&self) -> Result<crate::library::CollectibleDef, String> {
        let mut unknown = Vec::new();
        let def: crate::library::CollectibleDef = crate::library::parse_known(self.resolved(), |k| unknown.push(k)).map_err(|e| e.to_string())?;
        if !unknown.is_empty() {
            return Err(format!("unknown key {}", unknown.join(", ")));
        }
        Ok(def)
    }

    pub fn plant(&self) -> Result<crate::library::PlantDef, String> {
        let mut unknown = Vec::new();
        let def: crate::library::PlantDef = crate::library::parse_known(self.resolved(), |k| unknown.push(k)).map_err(|e| e.to_string())?;
        if !unknown.is_empty() {
            return Err(format!("unknown key {}", unknown.join(", ")));
        }
        Ok(def)
    }

    /// The poses this entry can be shown in: what its movement verb and its
    /// rig can actually do (editor/pose.rs).
    pub fn poses(&self) -> Vec<pose::Pose> {
        if self.kind != Kind::Creature {
            return Vec::new();
        }
        match self.creature() {
            Ok(def) => pose::choices(crate::creatures::locomotion::verb(&def.behaviour.movement), def.rig.is_some()),
            // An entry that will not parse still shows the two poses that
            // need nothing of it.
            Err(_) => vec![pose::Pose::Live, pose::Pose::Rest],
        }
    }

    /// The pose to show this entry in: the one the page asks for, or the
    /// nearest its rig can take.
    pub fn pose_for(&self, def: &crate::library::CreatureDef) -> pose::Pose {
        pose::nearest(self.pose, crate::creatures::locomotion::verb(&def.behaviour.movement), def.rig.is_some())
    }

    /// Next roll for the individual in front of the camera. For an animal
    /// that is another dressing of the same model; a plant's whole shape is
    /// drawn from its seed, so that one is built again.
    pub fn regen(&mut self) {
        self.seed = crate::world::noise::hash32(self.seed ^ 0x9e37_79b9);
        self.wants(if self.kind == Kind::Creature { Impact::Row } else { Impact::Whole });
    }

    /// Write the entry back to its file in the library's canonical form.
    /// The file as it would be written: the text it was read from with each
    /// edited value spliced in. It falls back to writing the whole entry out
    /// in canonical form if there is no text to splice into.
    pub fn written(&self) -> Result<String, String> {
        if self.text.is_empty() {
            return Ok(fmt::render(&self.doc, self.kind.folder(), &self.order));
        }
        let mut out = self.text.clone();
        for path in &self.edited {
            out = splice::put(&out, path, &self.doc, self.kind.folder()).map_err(|e| format!("{path}: {e}"))?;
        }
        Ok(out)
    }

    pub fn export(&mut self) -> Result<PathBuf, String> {
        // Never write something the engine cannot read back.
        match self.kind {
            Kind::Creature => self.creature().map(|_| ())?,
            Kind::Plant => self.plant().map(|_| ())?,
            Kind::Material => self.material().map(|_| ())?,
            Kind::Weapon => self.weapon().map(|_| ())?,
            Kind::Collectible => self.collectible().map(|_| ())?,
        }
        let text = self.written()?;
        let path = self.path();
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.dirty = false;
        self.asked_discard = false;
        Ok(path)
    }
}

/// What each line of the front page does.
pub(crate) fn front_help(c: Choice) -> &'static str {
    match c {
        Choice::EditKind => "What the page is editing: creatures, plants, materials, the deathmatch guns, or the things the camera goes looking for. It stays on the entry you were on when two kinds have one of that name.",
        Choice::EditEntry => "Which entry of this kind is being edited. Left and right walk the library in its own order.",
        Choice::EditRegen => "Roll another individual: another size, coat and set of habits, and for a plant another whole shape.",
        Choice::EditAge => "Which age the one in front of you is shown at. Which ages a creature has follows from its height unless its ages block says otherwise.",
        Choice::EditPose => "What the subject is doing. Every pose but LIVE holds it still at the studio spot with no mind of its own, so it cannot walk out of the view; LIVE turns it loose in the world to graze and bolt as it would.",
        Choice::EditGrid => "The grey checked platform, or the world's own ground where you are standing: the same animal in the studio and in its biome.",
        Choice::EditLight => "The studio's own light, a late-morning sun in a clear sky with no grade, so a colour reads as the colour it is; or the world's hour, weather and grade, to see it where it lives.",
        Choice::EditExport => "Write the entry back to its file in library/, in the form tools/libfmt.py keeps.",
        Choice::EditReload => "Throw the edits away and read the file again.",
        _ => "",
    }
}

/// Step through a list of names, starting at the first when nothing is set.
fn cycle(names: &[&str], here: Option<&str>, dir: i32) -> String {
    if names.is_empty() {
        return String::new();
    }
    let n = names.len() as i32;
    let at = match here.and_then(|h| names.iter().position(|x| *x == h)) {
        Some(k) => k as i32 + dir.signum(),
        None => 0,
    };
    names[at.rem_euclid(n) as usize].to_string()
}

pub(crate) fn hexed(c: [f32; 3]) -> String {
    let b = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

/// Round to `dp` decimals, so a stepped value reads and writes as a short
/// number rather than carrying the float noise of repeated adds.
fn round_to(v: f32, dp: u8) -> f64 {
    let m = 10f64.powi(dp as i32);
    ((v as f64) * m).round() / m
}

/// The value at a dotted path, if the entry has it.
pub fn get<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    let mut at = doc;
    for key in path.split('.') {
        at = at.get(key)?;
    }
    Some(at)
}

/// Put a value at a dotted path, making the objects along the way.
pub fn set(doc: &mut Value, path: &str, v: Value) {
    let keys: Vec<&str> = path.split('.').collect();
    let mut at = doc;
    for key in &keys[..keys.len() - 1] {
        if !at.get(*key).map_or(false, |x| x.is_object()) {
            if let Some(m) = at.as_object_mut() {
                m.insert((*key).to_string(), Value::Object(Default::default()));
            }
        }
        match at.get_mut(*key) {
            Some(next) => at = next,
            None => return,
        }
    }
    if let Some(m) = at.as_object_mut() {
        m.insert(keys[keys.len() - 1].to_string(), v);
    }
}

/// The top-level keys of a library entry in the order the file has them.
/// serde_json sorts an object's keys, so the file's own order has to be read
/// from its text to be kept.
fn key_order(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut in_str = false;
    let mut start = 0usize;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
                // A string that closes at depth 1 and is followed by a colon
                // is one of the entry's own keys.
                if depth == 1 {
                    let mut j = i + 1;
                    while j < b.len() && (b[j] as char).is_whitespace() {
                        j += 1;
                    }
                    if j < b.len() && b[j] == b':' {
                        out.push(text[start + 1..i].to_string());
                    }
                }
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                start = i;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    out
}

/// The library's canonical JSON form, the Rust side of tools/libfmt.py:
/// identity keys first, then the kind's own key order, then whatever is left
/// in the order the file had it; a value stays on one line while it fits in
/// 118 columns, and is broken one item to a line when it does not.
pub mod fmt {
    use serde_json::Value;

    pub const WIDTH: usize = 118;
    const FIRST: [&str; 4] = ["name", "version", "introduced_by", "notes"];

    /// The per-kind key order, kept the same as tools/libfmt.py's ORDER.
    fn order(kind: &str) -> &'static [&'static str] {
        match kind {
            "materials" => &["id", "smooth", "crisp", "passable", "water", "glow", "glints", "foliage", "tint", "drift", "texture", "top", "bottom"],
            "plants" => &[
                "form",
                "height",
                "trunk_radius",
                "buttress",
                "crown_radius",
                "bark",
                "leaf",
                "leaf_alt",
                "morphs",
                "morph_blob_m",
                "squash",
                "curve",
                "canopy",
                "leader",
                "fill",
                "spread",
                "dome",
                "limb",
                "max_slope",
            ],
            "creatures" => &["voxel", "half_w", "height", "colors", "morphs", "behaviour", "in_water", "ops"],
            "collectibles" => &["display", "voxel", "spin", "colors", "ops"],
            _ => &[],
        }
    }

    /// One value on a single line, with the separators python's json uses.
    fn one_line(v: &Value) -> String {
        match v {
            Value::Object(m) => {
                let items: Vec<String> = m.iter().map(|(k, x)| format!("{}: {}", Value::String(k.clone()), one_line(x))).collect();
                format!("{{{}}}", items.join(", "))
            }
            Value::Array(a) => {
                let items: Vec<String> = a.iter().map(one_line).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Number(n) => trim(n.as_f64().unwrap_or(0.0), n.is_f64()),
            other => other.to_string(),
        }
    }

    /// Six significant digits, as libfmt's `tidy` keeps, and no trailing
    /// ".0" on a whole number (python writes 3.0 for a float, so a value
    /// that came in as an integer must stay one).
    fn trim(v: f64, float: bool) -> String {
        if !float {
            return format!("{}", v as i64);
        }
        let s = format!("{:.6e}", v);
        let r: f64 = s.parse().unwrap_or(v);
        let mut out = format!("{}", r);
        if !out.contains('.') && !out.contains('e') {
            out.push_str(".0");
        }
        out
    }

    /// A value at `indent` spaces: on one line while it fits.
    pub fn value(v: &Value, indent: usize) -> String {
        let s = one_line(v);
        let flat = match v {
            Value::Object(m) => m.is_empty(),
            Value::Array(a) => a.is_empty(),
            _ => true,
        };
        if s.len() + indent <= WIDTH || flat {
            return s;
        }
        let pad = " ".repeat(indent + 2);
        match v {
            Value::Array(a) => {
                let items: Vec<String> = a.iter().map(|x| format!("{pad}{}", value(x, indent + 2))).collect();
                format!("[\n{}\n{}]", items.join(",\n"), " ".repeat(indent))
            }
            Value::Object(m) => {
                let items: Vec<String> = m.iter().map(|(k, x)| format!("{pad}{}: {}", Value::String(k.clone()), value(x, indent + 2))).collect();
                format!("{{\n{}\n{}}}", items.join(",\n"), " ".repeat(indent))
            }
            _ => s,
        }
    }

    /// The keys that come after `key` in this kind's canonical order, so a
    /// new one can be written in before the first of them the file has.
    pub fn after(kind: &str, key: &str) -> Vec<&'static str> {
        let all: Vec<&'static str> = FIRST.iter().chain(order(kind).iter()).copied().collect();
        match all.iter().position(|k| *k == key) {
            Some(i) => all[i + 1..].to_vec(),
            // A key libfmt has no place for goes at the end of the entry.
            None => Vec::new(),
        }
    }

    /// The whole entry, ready to write: one key a line, canonical order.
    pub fn render(doc: &Value, kind: &str, was: &[String]) -> String {
        let Some(m) = doc.as_object() else {
            return format!("{doc}\n");
        };
        let mut keys: Vec<String> = Vec::new();
        for k in FIRST.iter().chain(order(kind).iter()) {
            if m.contains_key(*k) {
                keys.push((*k).to_string());
            }
        }
        // Then the keys libfmt has no place for, in the order the file had
        // them, and anything new after those.
        for k in was.iter().chain(m.keys()) {
            if m.contains_key(k) && !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        let lines: Vec<String> = keys.iter().map(|k| format!("  {}: {}", Value::String(k.clone()), value(&m[k], 2))).collect();
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }
}

// ---- the page over a running game ----------------------------------------
//
// The window (src/main.rs) and the headless screenshot run (tools/headless.rs)
// both drive the page through these, so what a screenshot shows is what the
// window does.

use crate::game::Game;
use studio::Studio;

/// Open the page on `name` (a creature or a plant), with a studio in front of
/// the player. The game stops saving while the page is open: a studio floor
/// in the middle of someone's world would be a poor thing to find saved.
pub fn open(game: &mut Game, name: Option<&str>, aspect: f32) -> Result<(Editor, Studio), String> {
    let lib = game.library().ok_or("this world has no library")?;
    let kind = match name {
        Some(n) if lib.creature(n).is_some() => Kind::Creature,
        Some(n) if lib.plant(n).is_some() => Kind::Plant,
        Some(n) if lib.materials.iter().any(|m| m.name == n) => Kind::Material,
        Some(n) if lib.weapons.iter().any(|w| w.name == n) => Kind::Weapon,
        Some(n) if lib.collectibles.iter().any(|c| c.name == n) => Kind::Collectible,
        Some(n) => return Err(format!("no creature, plant, material, weapon or find called {n}")),
        None => Kind::Creature,
    };
    let mut page = Editor::new(lib, kind, name)?;
    // The studio is anchored to where the player is standing, so it is made
    // before the page takes the camera off them.
    let mut st = Studio::new(game);
    st.persist_was = game.persist;
    game.persist = false;
    // A match is no place for a studio: the gun goes away, the rules are
    // parked and the player's body is frozen until the page closes.
    game.set_editing(true);
    game.studio_light = !page.world_light;
    floor(&page, &mut st, game);
    apply(&mut page, &mut st, game, aspect);
    st.reframe(aspect);
    st.drive_camera(game, aspect, 0.0);
    Ok((page, st))
}

/// Build the entry as it stands now and put it in the studio. Every edit goes
/// through here, so what is on screen is always what the file would say if it
/// were written this instant.
/// Do whatever the page has asked for since the last frame, and nothing
/// more. Called once a frame from `tick`, so holding an arrow down coalesces
/// into one rebuild rather than one per keypress.
pub fn settle(page: &mut Editor, st: &mut Studio, game: &mut Game, aspect: f32) {
    let Some(how) = page.pending.take() else { return };
    match how {
        Impact::Paint if repaint(page, st, game) => {}
        Impact::Row if restep(page, st, game) => {}
        // Either the cheap path did not fit this edit (a colour that moved
        // every slot after it, a page with nothing standing yet) or the edit
        // really does change the animal.
        _ => apply(page, st, game, aspect),
    }
}

/// A colour edit: write the new colours into the palette slots the model
/// already points at. False where the slots have moved, which happens when
/// two colours stop or start being identical -- then the model must be built
/// again, because its voxels name the slots.
fn repaint(page: &mut Editor, st: &mut Studio, game: &mut Game) -> bool {
    if page.kind != Kind::Creature || st.species.is_none() {
        return false;
    }
    let Ok(def) = page.creature() else { return false };
    let paint = subject::paint_of(&def);
    if paint.slots() != st.paint {
        return false;
    }
    st.paint_block(game, &paint.colours);
    page.msg = None;
    true
}

/// A behaviour, group or variety edit, a new age, pose or roll: build the
/// species row again and put it in place, then dress and pose the individual
/// that is already standing there. Nothing is despawned and no model is
/// built, which is what stops the animal blinking.
fn restep(page: &mut Editor, st: &mut Studio, game: &mut Game) -> bool {
    let Some(s) = st.species else { return false };
    let Some(world) = game.generator().world.clone() else { return false };
    let def = match page.creature() {
        Ok(d) => d,
        // A half-typed entry is the page's to report, not a reason to throw
        // the subject away.
        Err(x) => {
            page.msg = Some(x);
            return true;
        }
    };
    let model = game.ents.species[s].model;
    let bones = game.ents.species[s].ages.bones;
    let was_verb = game.ents.species[s].beh.movement.clone();
    game.ents.species[s] = subject::row(&def, &world.lib, model, s, bones);
    page.msg = None;
    let want = page.pose_for(&def);
    // A LIVE subject carries the components its mover gave it (creatures/
    // spawn.rs), so a new movement verb means a new animal; a posed one has
    // no mover at all and only wants its rig driven differently.
    if want == pose::Pose::Live && was_verb != def.behaviour.movement {
        st.spawn(game, page.stage, page.seed, want);
    } else {
        st.restyle(game, page.stage, page.seed, want);
    }
    true
}

pub fn apply(page: &mut Editor, st: &mut Studio, game: &mut Game, aspect: f32) {
    // The library lives in the world behind an Arc; hold the Arc so the game
    // itself can be changed while the library is read.
    let Some(world) = game.generator().world.clone() else {
        page.msg = Some("this world has no library".to_string());
        return;
    };
    let lib = &world.lib;
    page.msg = None;
    page.pending = None;
    match page.kind {
        Kind::Creature => {
            st.clear_plant(game);
            match page.creature() {
                Err(x) => page.msg = Some(x),
                Ok(def) => {
                    st.drop_subject(game);
                    let want = page.pose_for(&def);
                    match subject::creature(&def, lib, game.models.len(), st.block(), game.ents.species.len()) {
                        Err(x) => page.msg = Some(x),
                        Ok(built) => {
                            st.size(def.height, built.span, aspect);
                            let problems = st.load_creature(game, built);
                            page.msg = problems.first().cloned();
                            st.spawn(game, page.stage, page.seed, want);
                        }
                    }
                }
            }
        }
        Kind::Material => {
            st.clear_plant(game);
            st.drop_subject(game);
            match page.material() {
                Err(x) => page.msg = Some(x),
                Ok(def) => {
                    let name = def.name.clone();
                    match install_material(page, &def) {
                        Some(id) => {
                            st.size(3.0, 3.0, aspect);
                            st.show_material(game, id);
                        }
                        None => page.msg = Some(format!("no voxel id left for {name} in this world")),
                    }
                }
            }
        }
        Kind::Weapon => {
            st.clear_plant(game);
            match page.weapon() {
                Err(x) => page.msg = Some(x),
                Ok(def) => {
                    // A gun's shape is Rust, not data: the file picks one of
                    // `combat::viewmodel::SHAPES` by name, so the page stands
                    // that one up rather than building anything.
                    let which = crate::combat::viewmodel::shape_of(&def.view);
                    if which.is_none() && !def.view.is_empty() {
                        page.msg = Some(format!("no shape called {}: the shapes are {}", def.view, crate::combat::viewmodel::SHAPES.join(", ")));
                    }
                    let at = which.unwrap_or(0);
                    let all = crate::combat::viewmodel::models();
                    let mut parts: Vec<crate::models::Model> = Vec::new();
                    for (k, m) in all.into_iter().enumerate() {
                        // The gun, and for the minigun the barrels that turn
                        // on an axis of their own.
                        if k == at || (at == crate::combat::viewmodel::MINIGUN && k == crate::combat::viewmodel::BARRELS) {
                            parts.push(m);
                        }
                    }
                    // A gun's model carries a collision box it never uses, so
                    // its grid is the only honest measure: 32 voxels long and
                    // about half that tall.
                    let span = 32.0 * crate::combat::viewmodel::VOXEL;
                    // The guns have palette slots of their own, fixed and
                    // outside the library's: a world that is not a match has
                    // never written them.
                    crate::combat::viewmodel::palette(&mut game.palette);
                    game.palette_dirty = true;
                    st.size(span * 0.5, span, aspect);
                    // Broadside: a gun seen down its own barrel is a dot.
                    st.load_prop(game, parts, &[], std::f32::consts::FRAC_PI_2);
                }
            }
        }
        Kind::Collectible => {
            st.clear_plant(game);
            match page.collectible() {
                Err(x) => page.msg = Some(x),
                Ok(def) => match subject::collectible(&def, st.block()) {
                    Err(x) => page.msg = Some(x),
                    Ok(built) => {
                        st.size(built.tall, built.span, aspect);
                        st.load_prop(game, built.models, &built.colours, 0.0);
                        page.msg = built.problems.first().cloned();
                    }
                },
            }
        }
        Kind::Plant => {
            st.drop_subject(game);
            match page.plant() {
                Err(x) => page.msg = Some(x),
                Ok(def) => {
                    st.size(def.height[1], def.height[1], aspect);
                    // A plant is drawn in materials the world has voxel ids
                    // for, and a world only has ids for what its own biomes
                    // reach: another world's tree comes out in the default
                    // bark with no leaves at all, which is worth saying
                    // rather than leaving as a puzzle.
                    let missing: Vec<&str> = [("bark", Some(&def.bark)), ("leaf", def.leaf.as_ref())]
                        .into_iter()
                        .filter(|(_, m)| m.is_some_and(|m| lib.material(m).is_none()))
                        .map(|(slot, _)| slot)
                        .collect();
                    if !missing.is_empty() {
                        let world = crate::worldload::selected();
                        page.msg = Some(format!("{world} has no {} for {}: open a world that grows it", missing.join(" or "), def.name));
                    }
                    let pi = lib.plant(&def.name).unwrap_or(0);
                    let mut base = st.at;
                    if let Some(y) = st.ground(game) {
                        base.y = y;
                    }
                    let tree = subject::plant(&def, lib, pi, base, page.seed);
                    st.load_plant(game, tree);
                }
            }
        }
    }
}

/// Install the edited material so the world is drawn with it: the texture
/// table is global, built from a library, so the page keeps a library of its
/// own and installs that. The renderer notices the new generation and
/// re-uploads the tiles by itself (render/mod.rs). Returns the voxel id the
/// material has in this world, which is what the studio shows it with.
fn install_material(page: &mut Editor, def: &crate::library::MaterialDef) -> Option<u8> {
    if page.mats.is_none() {
        let (lib, errors) = crate::worldload::library_for(&crate::worldload::selected());
        for e in errors.iter().take(1) {
            eprintln!("edit: {e}");
        }
        page.mats = Some(lib);
    }
    let lib = page.mats.as_mut()?;
    let taken: Vec<u8> = lib.materials.iter().map(|m| m.id).collect();
    let at = lib.materials.iter().position(|m| m.name == def.name)?;
    // A material this world never reaches has no voxel id, so nothing could
    // be drawn with it. The page gives it one of the ids the world left
    // free, which no chunk can be using.
    let id = match lib.materials[at].id {
        0 => (17u8..254).find(|i| !taken.contains(i))?,
        id => id,
    };
    let mut def = def.clone();
    def.id = id;
    def.file_id = lib.materials[at].file_id;
    lib.materials[at] = def;
    crate::world::matlib::install(lib);
    Some(id)
}

/// Lay the grey platform or take it away, to match the page.
pub fn floor(page: &Editor, st: &mut Studio, game: &mut Game) {
    if page.grid {
        st.floor_on(game);
    } else {
        st.floor_off(game);
    }
}

/// Each frame the page is open: the ground under the studio may only just
/// have streamed in, so the platform and the animal are tried again until
/// they take.
pub fn tick(page: &mut Editor, st: &mut Studio, game: &mut Game, aspect: f32, dt: f32) {
    settle(page, st, game, aspect);
    if page.grid && !st.floored() {
        // The platform waits on ground that has streamed in. If it never
        // comes, say so: a page with no floor and nothing standing on it
        // gives no clue that anything is wrong.
        if !st.floor_on(game) {
            st.floor_tries += 1;
            if st.floor_tries == 90 {
                page.msg = Some("no ground for the studio here: walk somewhere open and press F8 again".to_string());
            }
        } else {
            st.floor_tries = 0;
        }
    }
    // LIVE is the one pose that can run off: an animal that flees the player
    // bolts the moment the page opens, so it is brought back. Everything
    // else is posed and stays where it was put.
    let showing = st.showing;
    if page.kind == Kind::Creature && !st.waiting {
        st.hold(game, page.stage, page.seed, showing);
    }
    if st.waiting {
        st.spawn(game, page.stage, page.seed, showing);
        // A spot that will not take it is worth saying: a swimmer wants
        // water and a woodpecker wants a tree, and the studio has neither.
        if st.tries == 60 {
            page.msg = Some(format!("no room for {} here: it may want water, a tree or deeper ground", page.name()));
        }
    }
    st.pose_tick(game, showing, dt);
    st.drive_camera(game, aspect, dt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_costs_only_what_it_changes() {
        assert_eq!(impact(Kind::Creature, "colors.coat"), Impact::Paint);
        assert_eq!(impact(Kind::Creature, "behaviour.walk_speed"), Impact::Row);
        assert_eq!(impact(Kind::Creature, "behaviour.group.size"), Impact::Row);
        assert_eq!(impact(Kind::Creature, "vary.size"), Impact::Row);
        assert_eq!(impact(Kind::Creature, "rarity"), Impact::Row);
        // These reach the model, so there is no cheap way to show them.
        assert_eq!(impact(Kind::Creature, "voxel"), Impact::Whole);
        assert_eq!(impact(Kind::Creature, "height"), Impact::Whole);
        assert_eq!(impact(Kind::Creature, "ages.juvenile"), Impact::Whole);
        // A plant is terrain and a material a global table: both are made
        // again whatever changes.
        assert_eq!(impact(Kind::Plant, "height"), Impact::Whole);
        assert_eq!(impact(Kind::Material, "base"), Impact::Whole);
        // Nothing a creature's behaviour knobs touch should cost a rebuild.
        for k in knobs(Kind::Creature) {
            if k.reach.path.starts_with("behaviour.") {
                assert_eq!(impact(Kind::Creature, k.reach.path), Impact::Row, "{}", k.reach.path);
            }
        }
    }

    #[test]
    fn a_dotted_path_reads_and_writes_through_missing_objects() {
        let mut v: Value = serde_json::json!({"behaviour": {"wary": 10}});
        assert_eq!(get(&v, "behaviour.wary").and_then(|x| x.as_f64()), Some(10.0));
        set(&mut v, "behaviour.group.spacing", Value::from(2.5));
        assert_eq!(get(&v, "behaviour.group.spacing").and_then(|x| x.as_f64()), Some(2.5));
        assert!(get(&v, "behaviour.nothing").is_none());
    }

    #[test]
    fn the_file_s_own_key_order_is_read_from_its_text() {
        let text = "{\n  \"name\": \"deer\",\n  \"colors\": {\"coat\": [1, 2, 3]},\n  \"ops\": [{\"op\": \"set\"}]\n}\n";
        assert_eq!(key_order(text), vec!["name", "colors", "ops"]);
    }

    #[test]
    fn stepping_a_knob_that_is_not_there_turns_it_on_at_the_low_end() {
        let lib = Library::default();
        let mut e = Editor {
            kind: Kind::Creature,
            names: vec!["x".into()],
            at: 0,
            doc: serde_json::json!({"name": "x"}),
            order: vec!["name".into()],
            text: String::new(),
            edited: Vec::new(),
            base: Value::Null,
            mats: None,
            seed: 1,
            stage: crate::creatures::age::Stage::Adult,
            pose: pose::Pose::Rest,
            grid: true,
            world_light: false,
            pending: None,
            closed: 0,
            top: 0,
            shown: 0,
            msg: None,
            dirty: false,
            asked_discard: false,
        };
        let i = knobs(Kind::Creature).iter().position(|k| k.reach.path == "behaviour.wary").unwrap();
        e.step(i, 1, &lib);
        assert_eq!(get(&e.doc, "behaviour.wary").and_then(|v| v.as_f64()), Some(0.0));
        e.step(i, 1, &lib);
        assert_eq!(get(&e.doc, "behaviour.wary").and_then(|v| v.as_f64()), Some(0.5));
    }

    /// Writing an entry nobody has touched gives its file back byte for
    /// byte, over the whole library. That is what makes WRITE THE FILE safe
    /// on entries somebody wrote by hand.
    #[test]
    fn writing_an_untouched_entry_gives_the_file_back() {
        let root = crate::library::data_root().join("library");
        let mut checked = 0;
        let mut wrong: Vec<String> = Vec::new();
        for kind in Kind::ALL {
            let Ok(dir) = std::fs::read_dir(root.join(kind.folder())) else { continue };
            for f in dir.flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let mut e = Editor {
                    kind,
                    names: vec![String::new()],
                    at: 0,
                    doc: serde_json::from_str(&text).unwrap(),
                    order: key_order(&text),
                    text: text.clone(),
                    edited: Vec::new(),
                    base: Value::Null,
                    mats: None,
                    seed: 1,
                    stage: crate::creatures::age::Stage::Adult,
                    pose: pose::Pose::Rest,
                    grid: true,
                    world_light: false,
                    pending: None,
                    closed: 0,
                    top: 0,
                    shown: 0,
                    msg: None,
                    dirty: false,
                    asked_discard: false,
                };
                checked += 1;
                let out = e.written().unwrap();
                if out != text {
                    wrong.push(path.display().to_string());
                }
                // And one edit changes one line, not the file around it.
                e.doc = serde_json::from_str(&text).unwrap();
                e.touched("name");
                let out = e.written().unwrap();
                if out != text {
                    wrong.push(format!("{} (rewritten by one edit)", path.display()));
                }
            }
        }
        assert!(checked > 100, "only {checked} entries read");
        assert!(wrong.is_empty(), "{} entries would be rewritten, e.g. {:?}", wrong.len(), &wrong[..wrong.len().min(3)]);
    }

    /// A real entry, a real knob, and the diff it would write: one line.
    #[test]
    fn stepping_a_knob_on_a_real_entry_rewrites_one_line() {
        let lib = Library::default();
        let path = crate::library::data_root().join("library").join("creatures").join("deer.json");
        let text = std::fs::read_to_string(&path).expect("deer.json");
        let mut e = Editor {
            kind: Kind::Creature,
            names: vec!["deer".into()],
            at: 0,
            doc: serde_json::from_str(&text).unwrap(),
            order: key_order(&text),
            text: text.clone(),
            edited: Vec::new(),
            base: Value::Null,
            mats: None,
            seed: 1,
            stage: crate::creatures::age::Stage::Adult,
            pose: pose::Pose::Rest,
            grid: true,
            world_light: false,
            pending: None,
            closed: 0,
            top: 0,
            shown: 0,
            msg: None,
            dirty: false,
            asked_discard: false,
        };
        let i = knobs(Kind::Creature).iter().position(|k| k.reach.path == "behaviour.wary").unwrap();
        e.step(i, 1, &lib);
        let out = e.written().unwrap();
        let before: Vec<&str> = text.lines().collect();
        let after: Vec<&str> = out.lines().collect();
        assert_eq!(before.len(), after.len(), "the line count changed");
        let differ: Vec<usize> = (0..before.len()).filter(|k| before[*k] != after[*k]).collect();
        assert_eq!(differ.len(), 1, "{differ:?} lines differ");
        assert!(after[differ[0]].contains("\"wary\": 10.5"), "{}", after[differ[0]]);
        // And it is still an entry the engine reads.
        assert!(e.creature().is_ok());
    }

    #[test]
    fn canonical_form_puts_the_identity_keys_first_and_keeps_the_rest_in_place() {
        let doc: Value = serde_json::json!({"ops": [], "name": "x", "rig": {"plan": "quadruped"}, "voxel": 0.0625});
        let out = fmt::render(&doc, "creatures", &["rig".to_string()]);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[1].starts_with("  \"name\""), "{out}");
        assert!(lines[2].starts_with("  \"voxel\""), "{out}");
        assert!(lines[3].starts_with("  \"ops\""), "{out}");
        assert!(lines[4].starts_with("  \"rig\""), "{out}");
    }
}
