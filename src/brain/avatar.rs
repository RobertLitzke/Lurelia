//! What a bot looks like: the library's `robot`, rigged, animated and
//! carrying its gun in its hand.
//!
//! The first bots were spheres: a sphere answers the question the first
//! bots existed to answer, which was whether they fight well. They do, so this is the second answer. A body with legs
//! says things a sphere cannot -- which way it is about to go, whether it is
//! strafing or backing off, whether it has its weapon up -- and all of those
//! are things you have to read off an opponent to fight it.
//!
//! Nothing about the shape is written here. The body is a library creature
//! (`robot`, which is a `humanoid` in different paint), compiled the way the
//! EDIT page compiles the entry it is showing: on demand, into models of its
//! own, so a bot looks the same in every world whether or not that world's
//! biomes have ever heard of it. `src/anim/figure.rs` poses it.
//!
//! The one thing drawn here that is not in the library is the lamp in its
//! chest, which carries its health. That is deliberate: health is state, and
//! state does not belong in a voxel model. It is also emissive, so it is
//! readable in an unlit corridor, where the old trick of tinting a near-white
//! shell by health would not be -- and a tint can only darken, which stops
//! meaning anything the moment a bot has colours of its own. The plate tint
//! is now what the world says it is: whose side this one is on.

use crate::anim::{Plan, Role};
use crate::anim::{figure::Figure, Rig};
use crate::library::Library;
use crate::models::{self, srgb, Grid, Model};
use glam::Vec3;

/// The library creature the bots are drawn as.
pub const SPECIES: &str = "robot";

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
pub const BODY_M: usize = 0;
pub const LAMP_M: usize = 1;
/// The lit blob a pickup pad stands in (src/shooter/pickup.rs). It lives
/// here because it is drawn in the lamp's palette slot and nothing else in
/// the engine owns an emissive shape.
pub const PAD_M: usize = 2;
/// The cross drawn over a health pad, in the same emissive slot: the one
/// mark on the floor of an arena that has to be read without reading a word.
pub const CROSS_M: usize = 3;
/// How many models come before the rig's parts.
pub const WHOLE: usize = 4;

/// A bot's body: the models to draw it with, the rig to pose, and the sizes
/// every placement needs.
pub struct Avatar {
    /// The whole body, the lamp, then one model per bone. Bone model indices
    /// count from the first of these.
    pub models: Vec<Model>,
    pub rig: Rig,
    /// Model voxel edge in metres, and the pivot the parts are placed about.
    pub voxel: f32,
    pub pivot: Vec3,
    /// Problems building it, for `--validate`.
    pub problems: Vec<String>,
}

/// The fallback when the library has no `robot`: a blank body with no rig, so
/// a bot is a grey box rather than a crash.
fn stub() -> Avatar {
    let mut g = Grid::new();
    // The lamp slot, which is lit white: a bot with no body is a
    // glowing box, which is unmistakable rather than merely wrong.
    g.fill([12, 0, 13], [20, 30, 19], LAMP);
    let m = g.build(1.75 / 32.0, 0.3, 1.75);
    Avatar {
        pivot: m.pivot,
        voxel: m.voxel,
        models: vec![m, lamp_model(), pad_model(), cross_model()],
        rig: Rig {
            plan: Some(Plan::Biped),
            bones: Vec::new(),
            activities: Default::default(),
            floor: 0.0,
        },
        problems: vec![format!("no library creature called {SPECIES}: bots have no body")],
    }
}

/// The lamp in a bot's chest, drawn as its own instance so its colour can say
/// something. Near white in the palette, because the instance tint is what
/// takes it from amber to red.
fn lamp_model() -> Model {
    let mut g = Grid::new();
    g.ellipsoid([16.0, 16.0, 16.0], [1.25, 1.25, 1.25], LAMP);
    g.build(LAMP_VOX, 0.1, 0.2)
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

/// How far the pad's blob floats above the floor it stands on, metres: it
/// turns on the spot there rather than lying in the dust.
pub const PAD_RISE: f32 = 0.45;

/// The lamp's voxel edge, and how far its middle sits above the point it is
/// placed at: every model's pivot is the bottom centre of its 32-cube
/// (`Grid::build`), and the lamp is a blob in the middle of one.
const LAMP_VOX: f32 = 1.75 / 32.0;
pub const LAMP_RISE: f32 = 16.0 * LAMP_VOX;

/// Build the bots' body from the library, and fill in the palette slots it
/// needs. Pure in the library, so the renderer and the rules may each call it
/// and get the same models in the same order.
pub fn build(lib: Option<&Library>, palette: &mut [[f32; 4]]) -> Avatar {
    // Lit, but not so lit that the colour goes out of it: the lamp's whole
    // job is to be amber at full health and red near the end, and anything
    // much past half an exposure of glow reads white whatever it is tinted.
    palette[LAMP as usize] = srgb(255, 255, 255, 0.5);
    let Some(def) = lib.and_then(|l| l.creatures.iter().find(|c| c.name == SPECIES)) else {
        return stub();
    };
    let mut problems = Vec::new();
    // Each colour the entry names takes a slot of its own, in a fixed order,
    // so the models and the palette cannot disagree.
    let mut names: Vec<&String> = def.colors.keys().collect();
    names.sort();
    if names.len() > COLOURS.len() {
        problems.push(format!("{SPECIES}: {} colours, only {} slots", names.len(), COLOURS.len()));
    }
    for (k, name) in names.iter().enumerate().take(COLOURS.len()) {
        palette[COLOURS.start + k] = models::linear_colour(&def.colors[*name]);
    }
    let resolve = |n: &str| -> Option<u8> {
        if n == "air" {
            return Some(0);
        }
        let k = names.iter().position(|x| x.as_str() == n)?;
        Some((COLOURS.start + k.min(COLOURS.len() - 1)) as u8 + 1)
    };
    let s = crate::world::shape::compile(&def.ops, [32, 32, 32], &resolve);
    if s.solid == 0 {
        return stub();
    }
    let whole = models::from_cells(&s.mat, def.voxel, def.half_w, def.height);
    let (pivot, voxel) = (whole.pivot, whole.voxel);
    let Some(rig_def) = def.rig.as_ref() else {
        problems.push(format!("{SPECIES} has no rig: bots will not move"));
        return Avatar {
            models: vec![whole, lamp_model(), pad_model(), cross_model()],
            rig: stub().rig,
            voxel,
            pivot,
            problems,
        };
    };
    let sliced = crate::anim::slice::slice(rig_def, &def.ops, &s.mat, &|n| resolve(n).is_some(), def.voxel, def.half_w, def.height);
    let mut rig = sliced.rig;
    rig.offset_models(WHOLE);
    let mut out = vec![whole, lamp_model(), pad_model(), cross_model()];
    out.extend(sliced.parts);
    Avatar {
        models: out,
        rig,
        voxel,
        pivot,
        problems,
    }
}

impl Avatar {
    /// A figure posed on this rig.
    pub fn figure(&self, seed: u32) -> Figure {
        Figure::new(&self.rig, seed)
    }

    /// Where the chest lamp sits in the world, by the pose it is drawn in.
    /// Where to place the lamp model, which is not where the lamp appears:
    /// every model's pivot is the bottom centre of its 32-cube, so the lit
    /// blob draws `LAMP_RISE` above the point this gives back.
    pub fn lamps(&self, fig: &Figure, at: Vec3, yaw: f32, alpha: f32) -> [Vec3; 3] {
        let pose = fig.at(alpha);
        let mut fk = Vec::new();
        crate::anim::forward(&self.rig, &pose, self.voxel, self.pivot, &mut fk);
        // The highest bone of the trunk: the chest, where there is one.
        let chest = (0..self.rig.bones.len()).filter(|&i| self.rig.bones[i].role == Role::Spine).next_back();
        let place = |off: Vec3| match chest.and_then(|i| fk.get(i)) {
            Some(&(p, q)) => at + glam::Quat::from_rotation_y(yaw) * (p + q * (off * self.voxel)) - Vec3::Y * LAMP_RISE,
            None => at + Vec3::Y * (1.25 - LAMP_RISE) + Vec3::new(off.x, 0.0, off.z) * self.voxel,
        };
        // High on the collar, one each side, and one between the shoulder
        // blades. Not in the middle of the chest, which is where this
        // started and where nothing can be seen: a weapon held in both hands
        // lies across the middle of the chest and the supporting arm crosses
        // what is left, so the only parts of a fighter's torso that are
        // reliably in view are above the arms. Three of them means one is
        // facing you whichever way the bot is turned.
        // Offsets are voxels from the chest joint, and match the recesses in
        // library/creatures/robot.json: move one and move the other.
        [place(Vec3::new(-2.2, 4.8, -2.4)), place(Vec3::new(2.2, 4.8, -2.4)), place(Vec3::new(0.0, 4.2, 2.4))]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn built() -> Avatar {
        let (lib, errors) = Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let mut p = models::palette();
        build(Some(&lib), &mut p)
    }

    /// The bots have a body with a bone for every part of it, and it is the
    /// size the player's own collision box is.
    #[test]
    fn the_robot_builds_from_the_library() {
        let av = built();
        assert!(av.problems.is_empty(), "{:?}", av.problems);
        assert_eq!(av.rig.plan, Some(Plan::Biped));
        assert_eq!(av.models.len(), WHOLE + av.rig.bones.len());
        for b in &av.rig.bones {
            let m = &av.models[b.model];
            assert!(!m.mesh.quads.is_empty(), "bone {} owns no voxels", b.name);
        }
        let h = av.models[BODY_M].height;
        assert!((h - 1.75).abs() < 0.01, "a bot is {h} m tall");
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
        for (i, m) in av.models.iter().enumerate() {
            let what = match i {
                BODY_M => "the body",
                LAMP_M => "the chest lamp",
                PAD_M => "the pickup blob",
                CROSS_M => "the health cross",
                _ => "a bone",
            };
            assert!(!m.mesh.quads.is_empty(), "{what} (model {i}) has no voxels in it");
        }
        // And the cross is a cross: wider than it is thick on both level
        // axes, so it does not read as a post from the side.
        let c = &av.models[CROSS_M];
        assert!(c.mesh.quads.len() > 6, "the cross is a single box: {} quads", c.mesh.quads.len());
    }

    /// The three lamps sit where they were meant to: two on the front of the
    /// collar, one between the shoulder blades, all of them on the body.
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
        let fig = av.figure(1);
        let up = Vec3::Y * LAMP_RISE;
        let [left, right, back] = av.lamps(&fig, Vec3::ZERO, 0.0, 1.0).map(|p| p + up);
        for (what, p) in [("left", left), ("right", right), ("back", back)] {
            assert!((1.2..1.7).contains(&p.y), "the {what} lamp is at {} m, not on a chest", p.y);
            assert!(p.x.abs() < 0.32, "the {what} lamp is {} m off the middle, wider than the body", p.x);
        }
        assert!(left.z < -0.1 && right.z < -0.1, "the collar lamps are not on the front: {left:?} {right:?}");
        assert!(back.z > 0.1, "the back lamp is not behind the body: {back:?}");
        assert!(left.x < -0.1 && right.x > 0.1, "the collar lamps are not a pair either side of the middle");
        assert!((left.x + right.x).abs() < 1e-3, "the collar lamps are not symmetric");
        // High enough to clear a weapon held in both hands, which is what
        // put them on the collar in the first place.
        let hand = crate::anim::figure::hand_bone(&av.rig).expect("a hand");
        let mut fk = Vec::new();
        crate::anim::forward(&av.rig, &fig.at(1.0), av.voxel, av.pivot, &mut fk);
        assert!(left.y > fk[hand].0.y + 0.1, "the lamps are no higher than the gun hand: {} vs {}", left.y, fk[hand].0.y);
        // And the comment above `lamps` is true: the drawn blob is a lamp's
        // height above the point it is placed at, not at it.
        assert!(av.lamps(&fig, Vec3::ZERO, 0.0, 1.0)[0].y < left.y - 0.5);
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
