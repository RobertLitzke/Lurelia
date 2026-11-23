//! What a bot looks like: one of the library's robot builds, rigged,
//! animated and carrying its gun in its hand.
//!
//! The first bots were spheres: a sphere answers the question the first
//! bots existed to answer, which was whether they fight well. They do, so this is the second answer. A body with legs
//! says things a sphere cannot -- which way it is about to go, whether it is
//! strafing or backing off, whether it has its weapon up -- and all of those
//! are things you have to read off an opponent to fight it.
//!
//! Nothing about the shape is written here. The bodies are library creatures
//! (`BUILDS`, children of `robot`, which is a `humanoid` in different paint),
//! compiled the way the EDIT page compiles the entry it is showing: on
//! demand, into models of their own, so a bot looks the same in every world
//! whether or not that world's biomes have ever heard of it.
//! `src/anim/figure.rs` poses it. Each bot is handed one build for the match
//! (`Shooter::new`); every build shares the player's collision box, so the
//! build is looks and nothing else.
//!
//! The one thing drawn here that is not in the library is the lamp across
//! its chest, which carries its side's colour and its health. That is deliberate: health is state, and
//! state does not belong in a voxel model. It is also emissive, so it is
//! readable in an unlit corridor, where the old trick of tinting a near-white
//! shell by health would not be -- and a tint can only darken, which stops
//! meaning anything the moment a bot has colours of its own. The plate tint
//! is now what the world says it is: whose side this one is on.

use crate::anim::{figure::Figure, Rig};
use crate::anim::{Plan, Role};
use crate::library::Library;
use crate::models::{self, srgb, Grid, Model};
use glam::{Quat, Vec3};

/// The library creature the builds are children of, and what is drawn when
/// none of them is there.
pub const SPECIES: &str = "robot";
/// The bodies a bot may be drawn as: built for the shoulders, for the
/// middle, and thin and a little tall.
pub const BUILDS: [&str; 3] = ["robot_brawny", "robot_heavy", "robot_lean"];

/// Palette slots the avatar may fill: the guns take 18..38
/// (combat/viewmodel.rs) and the ride's start at 48. The robot names seven
/// colours, so ten is room to spare; the guns needed the slots below when
/// the arsenal grew to ten and they are drawn a hand from the camera, where
/// a wrong colour is unmissable. 37 was an overlap that quietly repainted
/// the sniper scope's glass, so this starts clear of it at 38.
pub const COLOURS: std::ops::Range<usize> = 38..48;
/// The lamp's own slot, which is not the creature's.
const LAMP: u8 = 47;

/// Models, in the order they are appended after the guns.
/// Only the tests ask for the whole body: the game draws the rig's parts.
#[cfg(test)]
pub const BODY_M: usize = 0;
pub const LAMP_M: usize = 1;
/// The lit blob a pickup pad stands in (src/shooter/pickup.rs). It lives
/// here because it is drawn in the lamp's palette slot and nothing else in
/// the engine owns an emissive shape.
pub const PAD_M: usize = 2;
/// The cross drawn over a health pad, in the same emissive slot: the one
/// mark on the floor of an arena that has to be read without reading a word.
pub const CROSS_M: usize = 3;
/// The breastplate drawn over an armour pad, in the same slot: blue against
/// the cross's green, and a different shape for anybody who sees neither.
pub const ARMOUR_M: usize = 4;
/// Capture the flag's flag, pole and cloth, and the ring its stand is marked
/// with (src/shooter/flag.rs). Lit, like the pickups, so the instance tint
/// is the side it belongs to.
pub const FLAG_M: usize = 5;
pub const STAND_M: usize = 6;
/// A kart race's shield (src/race/fight.rs): a hollow dome of light, lit in
/// the same slot, so the instance tint is its colour.
pub const BUBBLE_M: usize = 7;
/// How many models come before the rigs' parts.
#[cfg(test)]
pub const WHOLE: usize = 8;

/// The bots' bodies: the models to draw them with, and one `Body` per build.
pub struct Avatar {
    /// The first build's whole body, the lamp, the pickup and flag shapes,
    /// then each build's parts in turn. Bone model indices count from the
    /// first of these.
    pub models: Vec<Model>,
    /// At least one, always: a library with no robot gets a stub.
    pub bodies: Vec<Body>,
    /// Problems building it, for `--validate`.
    pub problems: Vec<String>,
}

/// One build: the rig to pose, the sizes every placement needs, and where
/// its lamp bars sit.
pub struct Body {
    /// The whole body, one model per bone: what a corpse falls as.
    pub rig: Rig,
    /// The same rig over the same voxels split in two, for a living bot:
    /// `paint` is the plates in the `PAINT` colour, which take the side's
    /// colour like a uniform, and `base` is everything else, which keeps
    /// the robot's own greys. Bone for bone the same as `rig`; only the
    /// models differ.
    pub base: Rig,
    pub paint: Rig,
    /// Model voxel edge in metres, and the pivot the parts are placed about.
    pub voxel: f32,
    pub pivot: Vec3,
    /// The middles of the front and back lamp bars, in voxels from the chest
    /// joint, found on the build's own voxels (`bars`).
    bars: [Vec3; 2],
}

/// The fallback when the library has no `robot`: a blank body with no rig, so
/// a bot is a grey box rather than a crash.
fn stub() -> Avatar {
    let mut g = Grid::new();
    // The lamp slot, which is lit white: a bot with no body is a
    // glowing box, which is unmistakable rather than merely wrong.
    g.fill([12, 0, 13], [20, 30, 19], LAMP);
    let m = g.build(1.75 / 32.0, 0.3, 1.75);
    let body = Body {
        pivot: m.pivot,
        voxel: m.voxel,
        rig: empty_rig(),
        base: empty_rig(),
        paint: empty_rig(),
        bars: DEFAULT_BARS,
    };
    Avatar {
        models: shared(m),
        bodies: vec![body],
        problems: vec![format!("no library creature called {SPECIES}: bots have no body")],
    }
}

fn empty_rig() -> Rig {
    Rig {
        soft: None,
        plan: Some(Plan::Biped),
        bones: Vec::new(),
        activities: Default::default(),
        floor: 0.0,
    }
}

/// The colour a bot's uniform is painted over: the chest and shoulder
/// plates and the kneepads (`uniform` in library/creatures/robot.json, the
/// same white as the body until a side paints it). Limbs, head, waist and
/// joints keep their own colours, so a side reads as what it is wearing
/// rather than as a robot dipped in paint.
pub const PAINT: &str = "uniform";

/// Where the bars sit on the robot itself, for a body with no voxels to
/// find them on.
const DEFAULT_BARS: [Vec3; 2] = [Vec3::new(0.0, 4.5, -2.0), Vec3::new(0.0, 4.5, 2.0)];

/// The models every build shares, after the given whole body.
fn shared(whole: Model) -> Vec<Model> {
    vec![whole, lamp_model(), pad_model(), cross_model(), armour_model(), flag_model(), stand_model(), bubble_model()]
}

/// The lamp across a bot's chest, drawn as its own instance so its colour
/// can say something. Near white in the palette, because the instance tint
/// is what takes it from the side's colour to red.
///
/// A bar four voxels long, one high and one deep, its pivot moved to its
/// middle so it is placed where it is seen and turns about itself.
fn lamp_model() -> Model {
    let mut g = Grid::new();
    g.fill([14, 16, 16], [18, 17, 17], LAMP);
    let mut m = g.build(LAMP_VOX, 0.12, 0.1);
    m.pivot = Vec3::new(16.0, 16.5, 16.5);
    m
}

/// A cross over a health pad, so what is on the floor can be told from the
/// other end of a hall without walking into it.
///
/// Three bars rather than a flat cross on purpose: the pad turns on the
/// spot, and a flat cross spends a quarter of every turn edge-on, which
/// reads as a blinking fault rather than as a mark. An upright with an arm
/// each way shows a cross from any side.
fn cross_model() -> Model {
    let mut g = Grid::new();
    g.fill([15, 9, 15], [18, 25, 18], LAMP);
    g.fill([10, 15, 15], [23, 19, 18], LAMP);
    g.fill([15, 15, 10], [18, 19, 23], LAMP);
    g.build(LAMP_VOX, 0.14, 0.3)
}

/// A breastplate over an armour pad: a curved shell, the front half of an
/// egg, with a notch at the neck and one at each arm. Curved rather than
/// flat for the cross's reason: it turns, and a flat plate edge-on is a
/// line. From behind it is the inside of the same curve, still a shape.
fn armour_model() -> Model {
    let mut g = Grid::new();
    let (c, r) = ([16.0f32, 16.0, 17.0], [6.5f32, 8.0, 5.0]);
    for y in 0..32 {
        for z in 0..32 {
            for x in 0..32 {
                let p = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                let d = ((p[0] - c[0]) / r[0]).powi(2) + ((p[1] - c[1]) / r[1]).powi(2) + ((p[2] - c[2]) / r[2]).powi(2);
                // A shell a voxel and a half thick, the front half only.
                if !(0.62..=1.0).contains(&d) || p[2] > c[2] {
                    continue;
                }
                let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
                let neck = dy > 4.5 && dx.abs() < 2.5;
                let arms = dy > 1.5 && dx.abs() > 4.5;
                if !neck && !arms {
                    g.set(x, y, z, LAMP);
                }
            }
        }
    }
    g.build(LAMP_VOX, 0.14, 0.3)
}

/// The shape a pickup pad glows as: an octahedron, which is the one solid
/// that reads as a thing-to-collect from any angle and does not read as a
/// crate. Lit in the lamp's slot, so the instance tint is what says whether
/// it is health, armour, rounds or a gun.
fn pad_model() -> Model {
    let mut g = Grid::new();
    // Two cones base to base. Four voxels across at the waist, which at this
    // voxel edge is about a quarter of a metre: big enough to see down a
    // hall, small enough not to hide what is behind it.
    for k in 0..=7i32 {
        let r = 4 - (k - 4).abs().min(4);
        if r <= 0 {
            continue;
        }
        g.fill([16 - r, 12 + k, 16 - r], [16 + r, 13 + k, 16 + r], LAMP);
    }
    g.build(LAMP_VOX, 0.14, 0.3)
}

/// A flag: a thin pole the height of a body and a cloth off the top of it,
/// kinked twice so it reads as cloth and not as a sign. Its voxel edge is
/// the lamp's, so the 32-cube is 1.75 m, a pole about as tall as a fighter:
/// it sticks up over a carrier's shoulders and over a low wall.
fn flag_model() -> Model {
    let mut g = Grid::new();
    g.fill([15, 0, 15], [17, 32, 17], LAMP);
    // The cloth, in three panels each a voxel further back than the last.
    for (k, x0) in [17, 21, 25].iter().enumerate() {
        let z = 15 + k as i32 % 2;
        g.fill([*x0, 21, z], [x0 + 4, 31, z + 1], LAMP);
    }
    g.build(LAMP_VOX, 0.14, 1.75)
}

/// The ring a flag's stand is marked with on the floor: lit, flat, a metre
/// and a half across, so a base reads as a place even with its flag gone.
fn stand_model() -> Model {
    let mut g = Grid::new();
    for z in 0..32 {
        for x in 0..32 {
            let d = ((x as f32 + 0.5 - 16.0).powi(2) + (z as f32 + 0.5 - 16.0).powi(2)).sqrt();
            if (11.0..14.0).contains(&d) {
                g.set(x, 0, z, LAMP);
            }
        }
    }
    g.build(LAMP_VOX, 0.7, 0.1)
}

/// A kart's shield: the shell of an egg a little wider than it is tall, a
/// voxel and a half thick, its middle 14 voxels up, open at the bottom
/// where the road cuts it. Drawn see-through, so it is the rim that shows.
fn bubble_model() -> Model {
    let mut g = Grid::new();
    let (c, r) = ([16.0f32, 14.0, 16.0], [15.0f32, 13.0, 15.0]);
    for y in 1..32 {
        for z in 0..32 {
            for x in 0..32 {
                let p = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                let d = (((p[0] - c[0]) / r[0]).powi(2) + ((p[1] - c[1]) / r[1]).powi(2) + ((p[2] - c[2]) / r[2]).powi(2)).sqrt();
                if (0.9..=1.0).contains(&d) {
                    g.set(x, y, z, LAMP);
                }
            }
        }
    }
    g.build(LAMP_VOX, 0.8, 1.5)
}

/// How far the pad's blob floats above the floor it stands on, metres: it
/// turns on the spot there rather than lying in the dust.
pub const PAD_RISE: f32 = 0.45;

/// The lamp's voxel edge: the same as the robot's own, so the bar is as
/// thick as the band it lies in.
pub const LAMP_VOX: f32 = 1.75 / 32.0;

/// The height of the lamp band's middle, in model voxels. It is the same in
/// every build (library/creatures/robot.json): the band is a dark strip
/// two voxels high across y 24 and 25.
const BAR_Y: i32 = 25;

/// Build the bots' bodies from the library, and fill in the palette slots
/// they need. Pure in the library, so the renderer and the rules may each
/// call it and get the same models in the same order.
pub fn build(lib: Option<&Library>, palette: &mut [[f32; 4]]) -> Avatar {
    // Lit, but not so lit that the colour goes out of it: the lamp's whole
    // job is to be amber at full health and red near the end, and anything
    // much past half an exposure of glow reads white whatever it is tinted.
    palette[LAMP as usize] = srgb(255, 255, 255, 0.5);
    let creatures = lib.map(|l| l.creatures.as_slice()).unwrap_or(&[]);
    let mut defs: Vec<_> = BUILDS.iter().filter_map(|n| creatures.iter().find(|c| c.name == *n)).collect();
    let mut problems = Vec::new();
    if defs.len() < BUILDS.len() {
        let missing: Vec<&str> = BUILDS.iter().copied().filter(|n| !creatures.iter().any(|c| c.name == *n)).collect();
        problems.push(format!("bot builds missing from the library: {}", missing.join(", ")));
    }
    if defs.is_empty() {
        match creatures.iter().find(|c| c.name == SPECIES) {
            Some(d) => defs.push(d),
            None => return stub(),
        }
    }
    // Each colour the first build names takes a slot of its own, in a fixed
    // order, so the models and the palette cannot disagree. Every build
    // inherits the robot's paint, so they share the slots.
    let first = defs[0];
    let mut names: Vec<&String> = first.colors.keys().collect();
    names.sort();
    if names.len() > COLOURS.len() {
        problems.push(format!("{}: {} colours, only {} slots", first.name, names.len(), COLOURS.len()));
    }
    for (k, name) in names.iter().enumerate().take(COLOURS.len()) {
        palette[COLOURS.start + k] = models::linear_colour(&first.colors[*name]);
    }
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        let k = names.iter().position(|x| x.as_str() == n)?;
        Some((COLOURS.start + k.min(COLOURS.len() - 1)) as u8 + 1)
    };
    let mut models_out: Vec<Model> = Vec::new();
    let mut bodies = Vec::new();
    for def in defs {
        if def.colors.len() != first.colors.len() || def.colors.keys().any(|k| !first.colors.contains_key(k)) {
            problems.push(format!("{}: colours differ from {}'s, which the builds share", def.name, first.name));
        }
        let s = crate::world::shape::compile(&def.ops, [32, 32, 32], &resolve);
        if s.solid == 0 {
            problems.push(format!("{} has no voxels", def.name));
            continue;
        }
        let whole = models::from_cells(&s.mat, def.voxel, def.half_w, def.height);
        let (pivot, voxel) = (whole.pivot, whole.voxel);
        if models_out.is_empty() {
            models_out = shared(whole);
        }
        let Some(rig_def) = def.rig.as_ref() else {
            problems.push(format!("{} has no rig: bots will not move", def.name));
            continue;
        };
        let colour_ok = |n: &str| resolve(n).is_some();
        let sliced = crate::anim::slice::slice(rig_def, &def.ops, &s.mat, crate::anim::slice::Dims::CUBE, &colour_ok, def.voxel, def.half_w, def.height);
        let mut rig = sliced.rig;
        rig.offset_models(models_out.len());
        models_out.extend(sliced.parts);
        // The uniform and the rest, sliced the same way so every voxel
        // goes to the same bone it does in the whole body.
        let paint_slot = resolve(PAINT).unwrap_or(0);
        let mut halves = Vec::new();
        for painted in [false, true] {
            let mat: Vec<u8> = s.mat.iter().map(|&m| if (m == paint_slot) == painted { m } else { 0 }).collect();
            let half = crate::anim::slice::slice(rig_def, &def.ops, &mat, crate::anim::slice::Dims::CUBE, &colour_ok, def.voxel, def.half_w, def.height);
            let mut r = half.rig;
            r.offset_models(models_out.len());
            models_out.extend(half.parts);
            halves.push(r);
        }
        let paint = halves.pop().unwrap_or_else(empty_rig);
        let base = halves.pop().unwrap_or_else(empty_rig);
        let chest = rig_def.bones.iter().filter(|b| b.role == Role::Spine).next_back().and_then(|b| b.joint);
        let bars = match chest {
            Some(j) => bars(&s, Vec3::from(j)),
            None => DEFAULT_BARS,
        };
        bodies.push(Body { rig, base, paint, voxel, pivot, bars });
    }
    // A figure is posed on one rig and a corpse falls on it, so every build
    // has to have the same bones in the same order.
    if let Some(b0) = bodies.first() {
        for (k, b) in bodies.iter().enumerate().skip(1) {
            let same = b.rig.bones.len() == b0.rig.bones.len() && b.rig.bones.iter().zip(&b0.rig.bones).all(|(x, y)| x.name == y.name && x.parent == y.parent);
            if !same {
                problems.push(format!("bot build {k} has different bones from the first"));
            }
        }
    }
    if bodies.is_empty() {
        let mut av = stub();
        av.problems = problems;
        return av;
    }
    Avatar { models: models_out, bodies, problems }
}

/// The lamp bars' middles, in voxels from the chest joint: across the band
/// at `BAR_Y`, half sunk into the front and the back of the chest wherever
/// this build's chest happens to end.
fn bars(s: &crate::world::shape::Shape, chest: Vec3) -> [Vec3; 2] {
    let solid = |z: i32| s.get(16, BAR_Y, z).0 != 0 && s.get(16, BAR_Y - 1, z).0 != 0;
    let front = (0..32).find(|&z| solid(z));
    let back = (0..32).rev().find(|&z| solid(z));
    match (front, back) {
        (Some(f), Some(b)) => {
            let y = BAR_Y as f32 - chest.y;
            [Vec3::new(16.0 - chest.x, y, f as f32 - chest.z), Vec3::new(16.0 - chest.x, y, (b + 1) as f32 - chest.z)]
        }
        _ => DEFAULT_BARS,
    }
}

impl Avatar {
    /// Build `k`, wrapping round however many there are.
    pub fn body(&self, k: usize) -> &Body {
        &self.bodies[k % self.bodies.len()]
    }
}

/// A lamp bar in the world: its middle and the way it is turned. The lamp
/// model's pivot is its middle, so this is also where to place it.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub at: Vec3,
    pub rot: Quat,
}

impl Body {
    /// A figure posed on this rig.
    pub fn figure(&self, seed: u32) -> Figure {
        Figure::new(&self.rig, seed)
    }

    /// The lamp bars, front then back, where the pose puts them.
    ///
    /// High on the chest, across the collar, and again between the shoulder
    /// blades. Not in the middle of the chest, which is where the lamps
    /// started and where nothing can be seen: a weapon held in both hands
    /// lies across the middle of the chest and the supporting arm crosses
    /// what is left, so the only part of a fighter's torso that is reliably
    /// in view is above the arms. Front and back means one faces you
    /// whichever way the bot is turned.
    pub fn lamps(&self, fig: &Figure, at: Vec3, yaw: f32, alpha: f32) -> [Bar; 2] {
        let pose = fig.at(alpha);
        let mut fk = Vec::new();
        crate::anim::forward(&self.rig, &pose, self.voxel, self.pivot, &mut fk);
        // The highest bone of the trunk: the chest, where there is one.
        let chest = (0..self.rig.bones.len()).filter(|&i| self.rig.bones[i].role == Role::Spine).next_back();
        let yq = Quat::from_rotation_y(yaw);
        let place = |off: Vec3| match chest.and_then(|i| fk.get(i)) {
            Some(&(p, q)) => Bar {
                at: at + yq * (p + q * (off * self.voxel)),
                rot: yq * q,
            },
            None => Bar {
                at: at + Vec3::Y * 1.35 + yq * Vec3::new(off.x, 0.0, off.z) * self.voxel,
                rot: yq,
            },
        };
        self.bars.map(place)
    }
}

/// The colours a roster of bots is handed out of, in order, sRGB bytes.
///
/// One per bot and then round again, so a match of any size has every bot
/// told apart by its plates without anyone naming a colour. Three rules
/// shaped the list and all three are about reading a fight rather than about
/// taste:
///
/// - Nothing in the amber band, roughly hue 20 to 55. That is the health
///   lamp's own colour, and a bot whose plates are amber is a bot whose
///   health cannot be read at a glance. It is also what `tools/warm.py`
///   relies on to count a lamp's pixels without counting plate.
/// - Neighbours in the list are far apart in hue, because a two-bot match
///   takes the first two and a three-bot match the first three. The list
///   gets closer together as it goes on, which is unavoidable past about a
///   dozen, and by then nobody is telling twenty bots apart by colour alone.
/// - Nothing so dark that the shell stops reading as a shell in a dim
///   corridor, and nothing so pale it reads as the unpainted grey a corpse
///   turns.
///
/// The first is the cold cyan Slagdeck used to name by hand, kept there on
/// purpose: lamps are measured against a cold-plated bot (`docs/BOTS.md`,
/// "Watching bots headlessly"), and that method wants slot zero to stay cold.
pub const TEAM_COLOURS: [[u8; 3]; 20] = [
    [120, 196, 214], // cyan
    [215, 219, 96],  // citron
    [201, 73, 204],  // magenta
    [73, 204, 119],  // green
    [87, 73, 204],   // indigo
    [142, 204, 73],  // olive
    [131, 235, 145], // mint
    [148, 112, 224], // violet
    [210, 131, 235], // orchid
    [131, 234, 235], // ice
    [96, 219, 193],  // teal
    [73, 174, 204],  // sky
    [107, 219, 96],  // leaf
    [224, 112, 202], // fuchsia
    [96, 138, 219],  // blue
    [209, 235, 131], // pale olive
    [147, 224, 112], // lime
    [131, 143, 235], // periwinkle
    [112, 224, 176], // jade
    [163, 96, 219],  // purple
];

/// The `i`th bot's plate colour, as the renderer wants it.
pub fn team_tint(i: usize) -> [f32; 3] {
    let c = TEAM_COLOURS[i % TEAM_COLOURS.len()];
    [0, 1, 2].map(|k| c[k] as f32 / 255.0)
}

/// A side's colour as the plates are painted with it. The roster's colours
/// are sRGB bytes, and the instance tint multiplies in linear light, so a
/// colour handed over as bytes over 255 had every weak channel lifted (73
/// drew like 148) and every bot came out pastel. Taken into linear light,
/// most of the way, then brought back up so its strongest channel is as bright as it was:
/// the hue and the saturation of the colour as written, at the brightness
/// it had. The fourth channel (the hurt flash) passes through.
pub fn plate(t: [f32; 4]) -> [f32; 4] {
    let lin = [t[0], t[1], t[2]].map(|c| c.max(0.0).powf(PLATE_GAMMA));
    let top = lin.iter().copied().fold(0.0f32, f32::max);
    let want = t[0].max(t[1]).max(t[2]);
    let k = if top > 1e-4 { want / top } else { 0.0 };
    [lin[0] * k, lin[1] * k, lin[2] * k, t[3]]
}

/// How far into linear light `plate` takes a colour: 2.2 is the whole way
/// and read as garish; 1.0 leaves the old pastel.
const PLATE_GAMMA: f32 = 1.7;

/// What a corpse's plates are washed with: the paint is gone and what is
/// left is the metal. A dead bot keeps its shape and loses its side, which
/// is the point -- a body on the floor is scenery, and reading a team colour
/// off it during a fight is reading a bot that is not in the fight.
pub const DEAD_TINT: [f32; 3] = [0.34, 0.34, 0.35];

/// The lamp's colour for a share of health: amber whole, red nearly out, and
/// dark once the body is down.
pub fn lamp_tint(whole: f32, alive: bool) -> [f32; 3] {
    if !alive {
        return [0.12, 0.10, 0.10];
    }
    let t = whole.clamp(0.0, 1.0);
    // Further toward orange than the lamp is meant to look, because the hall
    // light lands on it too and washes it back toward white: at [1, 0.50,
    // 0.12] it rendered as cream, which is a colour nothing can be read off.
    let low = [1.0, 0.05, 0.03];
    let full = [1.0, 0.30, 0.04];
    [0, 1, 2].map(|i| low[i] + (full[i] - low[i]) * t)
}

/// The chest lamps of a living bot: its side's colour (a team's, or in a
/// free-for-all its own), pushed brighter and more saturated than the plate
/// so it reads as lit, and running to red as the bot is hurt. Amber for
/// everybody read as pink under the hall lights and said nothing about whose
/// side a bot was on.
pub fn lamp_colour(side: [f32; 3], whole: f32) -> [f32; 3] {
    let top = side.iter().copied().fold(0.0f32, f32::max).max(0.01);
    let bright = side.map(|c| c / top);
    let mean = (bright[0] + bright[1] + bright[2]) / 3.0;
    // Hard toward the pure hue and a little under full: a lamp lit in two
    // channels at once (cyan) glows past its colour to white otherwise.
    let vivid = bright.map(|c| (mean + (c - mean) * 2.4).clamp(0.0, 1.0) * 0.8);
    // Most of the way to red only once the bot is badly hurt, so a side can
    // still be read off a bot that has taken a hit or two.
    let hurt = (1.0 - whole.clamp(0.0, 1.0)).powf(1.6);
    let low = lamp_tint(0.0, true);
    [0, 1, 2].map(|i| vivid[i] + (low[i] - vivid[i]) * hurt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole bot's lamps are its side's colour, more intense than its
    /// plate; a nearly dead one's are red whatever its side.
    #[test]
    fn the_lamp_is_the_side_and_reddens_with_hurt() {
        for side in [team_tint(0), team_tint(2), team_tint(5)] {
            let lit = lamp_colour(side, 1.0);
            let spread = |c: [f32; 3]| c.iter().copied().fold(0.0f32, f32::max) - c.iter().copied().fold(1.0f32, f32::min);
            assert!(spread(lit) > spread(side), "{lit:?} is no more intense than {side:?}");
            // The side's strongest channel stays the lamp's strongest, and its
            // weakest the lamp's weakest.
            let (hi, lo) = (0..3).fold((0, 0), |(h, l), k| (if side[k] > side[h] { k } else { h }, if side[k] < side[l] { k } else { l }));
            assert!(lit[hi] >= lit.iter().copied().fold(0.0, f32::max) - 1e-4, "{lit:?} is not the colour of {side:?}");
            assert!(lit[lo] <= lit.iter().copied().fold(1.0, f32::min) + 1e-4, "{lit:?} is not the colour of {side:?}");
            let low = lamp_colour(side, 0.05);
            assert!(low[0] > 0.8 && low[1] < 0.25 && low[2] < 0.25, "a dying {side:?} bot's lamp is {low:?}");
        }
    }

    fn built() -> Avatar {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let mut p = models::palette();
        build(Some(&lib), &mut p)
    }

    /// The bots have three builds, each with a bone for every part of it
    /// and the same bones as the others, none taller than a little over the
    /// player's own collision box.
    #[test]
    fn the_robot_builds_from_the_library() {
        let av = built();
        assert!(av.problems.is_empty(), "{:?}", av.problems);
        assert_eq!(av.bodies.len(), BUILDS.len());
        let mut parts = WHOLE;
        for body in &av.bodies {
            assert_eq!(body.rig.plan, Some(Plan::Biped));
            if std::env::var("VG_RIG_DUMP").is_ok() {
                let mut fk = Vec::new();
                crate::anim::forward(&body.rig, &body.figure(1).at(1.0), body.voxel, body.pivot, &mut fk);
                for (i, b) in body.rig.bones.iter().enumerate() {
                    eprintln!("RIG {i} {} {:?} parent {:?} at {:.2?}", b.name, b.role, b.parent, fk[i].0);
                }
            }
            for b in &body.rig.bones {
                let m = &av.models[b.model];
                assert!(!m.mesh.quads.is_empty(), "bone {} owns no voxels", b.name);
            }
            // The whole body, then the same bones split into the rest and
            // the uniform.
            parts += body.rig.bones.len() * 3;
        }
        assert_eq!(av.models.len(), parts);
        let h = av.models[BODY_M].height;
        assert!((h - 1.75).abs() < 0.01, "a bot is {h} m tall");
    }

    /// The builds differ where they were meant to: the brawny one widest
    /// across the chest, the heavy one widest at the belly, the lean one the
    /// narrowest at the ribs and the tallest.
    #[test]
    fn the_builds_differ_where_they_should() {
        let (lib, _) = Library::load(&crate::library::data_root());
        let mut p = models::palette();
        build(Some(&lib), &mut p);
        let measure = |name: &str| {
            let def = lib.creatures.iter().find(|c| c.name == name).expect(name);
            let s = crate::world::shape::compile(&def.ops, [32, 32, 32], &|n: &str| Some(if n == "air" { 0 } else { 1 }));
            // Solid voxels across row y at z, in metres, arms and all.
            let across = |y: i32| (0..32).filter(|&x| (0..32).any(|z| s.get(x, y, z).0 != 0)).count() as f32 * def.voxel;
            let deep = |y: i32| (0..32).filter(|&z| s.get(16, y, z).0 != 0).count() as f32 * def.voxel;
            (across(23), deep(18), deep(22), s.top as f32 * def.voxel)
        };
        let [brawny, heavy, lean] = BUILDS.map(measure);
        assert!(
            brawny.0 > heavy.0 && brawny.0 > lean.0,
            "the brawny build is not the widest across the chest: {brawny:?} {heavy:?} {lean:?}"
        );
        assert!(heavy.1 > brawny.1 && heavy.1 > lean.1, "the heavy build has no belly: {brawny:?} {heavy:?} {lean:?}");
        assert!(lean.2 < brawny.2 && lean.2 < heavy.2, "the lean build is not the thinnest at the ribs: {brawny:?} {heavy:?} {lean:?}");
        assert!(lean.3 > brawny.3 + 0.03 && lean.3 < 1.85, "the lean build is {} m tall against {}", lean.3, brawny.3);
    }

    /// Every model the avatar hands out has voxels in it.
    ///
    /// This is here because the pickup blob did not. `Grid::fill` is
    /// exclusive on the far corner, and the blob was built as eight
    /// horizontal slices each given the same y at both ends, so every one of
    /// them filled nothing: the model was an empty mesh, the renderer drew
    /// it without complaint, and the only mark on a health pack anywhere in
    /// the arena was the gun a weapon pad happened to draw above it. A
    /// hand-built shape that comes out empty is silent in a way a library
    /// one is not, because nothing validates it.
    #[test]
    fn nothing_the_avatar_draws_is_empty() {
        let av = built();
        // The split halves are left out: a bone with no uniform on it has an
        // empty uniform model, which is right.
        let halves: Vec<usize> = av.bodies.iter().flat_map(|b| b.base.bones.iter().chain(&b.paint.bones).map(|x| x.model)).collect();
        for (i, m) in av.models.iter().enumerate() {
            if halves.contains(&i) {
                continue;
            }
            let what = match i {
                BODY_M => "the body",
                LAMP_M => "the chest lamp",
                PAD_M => "the pickup blob",
                CROSS_M => "the health cross",
                ARMOUR_M => "the armour plate",
                FLAG_M => "the flag",
                STAND_M => "the flag stand",
                BUBBLE_M => "the shield bubble",
                _ => "a bone",
            };
            assert!(!m.mesh.quads.is_empty(), "{what} (model {i}) has no voxels in it");
        }
        // And the cross is a cross: wider than it is thick on both level
        // axes, so it does not read as a post from the side.
        let c = &av.models[CROSS_M];
        assert!(c.mesh.quads.len() > 6, "the cross is a single box: {} quads", c.mesh.quads.len());
    }

    /// The two lamp bars sit where they were meant to on every build: one
    /// across the front of the chest, one between the shoulder blades, both
    /// half sunk into the body and above the gun hand.
    ///
    /// Placement is worth a test because it was got wrong twice by eye, and
    /// because the failure is silent: a lamp a few voxels out is simply
    /// inside the chest, drawing nothing anyone can see, and only a close
    /// front-on screenshot of a bot that happens to be standing in the light
    /// would ever catch it. The model faces -z, so the front of the body is
    /// negative z.
    #[test]
    fn the_lamps_sit_on_the_body() {
        let av = built();
        for (k, body) in av.bodies.iter().enumerate() {
            let fig = body.figure(1);
            let [front, back] = body.lamps(&fig, Vec3::ZERO, 0.0, 1.0);
            for (what, b) in [("front", front), ("back", back)] {
                assert!((1.2..1.6).contains(&b.at.y), "build {k}: the {what} bar is at {} m, not on a chest", b.at.y);
                assert!(b.at.x.abs() < 0.03, "build {k}: the {what} bar is {} m off the middle", b.at.x);
            }
            assert!(front.at.z < -0.08, "build {k}: the front bar is not on the front: {front:?}");
            assert!(back.at.z > 0.04, "build {k}: the back bar is not behind the body: {back:?}");
            // High enough to clear a weapon held in both hands, which is what
            // put the lamps up there in the first place.
            let hand = crate::anim::figure::hand_bone(&body.rig).expect("a hand");
            let mut fk = Vec::new();
            crate::anim::forward(&body.rig, &fig.at(1.0), body.voxel, body.pivot, &mut fk);
            assert!(front.at.y > fk[hand].0.y + 0.1, "build {k}: the bar is no higher than the gun hand: {} vs {}", front.at.y, fk[hand].0.y);
        }
    }

    /// Health reads off the lamp, and the plate tint is left alone for the
    /// team colour.
    ///
    /// The test is on how far round the lamp goes, not on how much green it
    /// loses. An earlier version asserted a flat 0.3 of green between whole
    /// and hurt, which failed the day the whole lamp was deepened toward
    /// orange -- and deepening it was the fix for a real problem, since the
    /// hall light lands on the lamp too and a paler amber rendered as cream.
    /// What has to hold is that the two are a long way apart in hue, which
    /// is what a player reads, and a flat difference in one channel is a
    /// clumsy way of saying so.
    #[test]
    fn the_lamp_carries_the_health() {
        let (whole, hurt, down) = (lamp_tint(1.0, true), lamp_tint(0.1, true), lamp_tint(0.0, false));
        let warmth = |c: [f32; 3]| c[1] / c[0].max(1e-3);
        assert!(warmth(whole) > warmth(hurt) * 2.5, "a hurt lamp is barely redder than a whole one: {whole:?} {hurt:?}");
        assert!(whole[0] > whole[1] * 1.8, "a whole lamp is not amber enough to tell from white: {whole:?}");
        assert!(hurt[0] > 0.5 && hurt[1] < 0.15, "a hurt lamp is not red: {hurt:?}");
        assert!(down.iter().all(|&c| c < 0.2), "a dead bot's lamp is still lit");
        // Health runs the whole way, with no step in it.
        let mut last = warmth(lamp_tint(0.0, true));
        for i in 1..=10 {
            let w = warmth(lamp_tint(i as f32 / 10.0, true));
            assert!(w > last, "the lamp stops changing at {}/10 health", i);
            last = w;
        }
    }

    /// The roster hands out twenty colours that can be told apart, none of
    /// them the health lamp's amber, and it comes round rather than running
    /// out.
    #[test]
    fn every_bot_gets_its_own_colour() {
        for (i, c) in TEAM_COLOURS.iter().enumerate() {
            let (r, g, b) = (c[0] as i32, c[1] as i32, c[2] as i32);
            // Amber is red over both the others by a margin (tools/warm.py
            // counts exactly this). A plate that passes for a lamp is a
            // plate you cannot read health off.
            assert!(!(r - b > 55 && r - g > 22), "colour {i} {c:?} is in the lamp's amber band");
            assert!(r.max(g).max(b) > 150, "colour {i} {c:?} is too dark to read in a corridor");
            assert!(r.min(g).min(b) < 200, "colour {i} {c:?} is too pale to tell from a corpse");
        }
        // The first few are what a small match uses, so they have to be a
        // long way apart from each other.
        for i in 0..4 {
            for j in (i + 1)..4 {
                let d: i32 = (0..3).map(|k| (TEAM_COLOURS[i][k] as i32 - TEAM_COLOURS[j][k] as i32).pow(2)).sum();
                assert!(d > 8000, "colours {i} and {j} are too close together: {d}");
            }
        }
        assert_eq!(team_tint(0), team_tint(TEAM_COLOURS.len()), "the roster did not come round");
        // And slot zero stays cold, which is what docs/BOTS.md measures
        // lamps against.
        assert!(TEAM_COLOURS[0][2] > TEAM_COLOURS[0][0] + 60, "slot zero stopped being cold");
    }
}
