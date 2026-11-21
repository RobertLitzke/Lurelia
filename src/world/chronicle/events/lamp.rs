use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Pos, WorldLamp};
use glam::Vec3;
use serde::Deserialize;

/// Lamps: point lights that let a roofed place be seen. Every other light in
/// the engine comes from the sky, so a room with a lid on it is black until
/// one of these is hung in it (see `render/lamps.rs`).
///
/// A lamp is light only. Put the fitting it hangs from in the structure, or
/// give its voxels a material with `glow` so the lamp itself looks lit.
#[derive(Deserialize, Debug)]
pub struct LampEvent {
    /// Where the lamps are measured from.
    at: Pos,
    /// Each lamp as `[east, south, up]` metres from `at`, up being above the
    /// ground there. One event holds a whole building's lighting, because
    /// lamps come in rows and a row should be one line in the chronicle.
    points: Vec<[f32; 3]>,
    /// Measure `up` from this height above the sea instead of from the
    /// ground: lamps in a cave, where the ground overhead is not the floor.
    #[serde(default)]
    level: Option<f32>,
    /// How far the light reaches, metres.
    #[serde(default = "default_reach")]
    reach: f32,
    /// Colour, sRGB bytes.
    #[serde(default = "default_colour")]
    colour: [f32; 3],
    /// Brightness at the lamp. 1 is about as bright as open daylight on a
    /// grey day; a working light in a shed is 1.5 to 2.5.
    #[serde(default = "default_strength")]
    strength: f32,
    /// Lit only from dusk to dawn: street lamps, a market's strings of
    /// lights. Off, the lamp burns day and night, as one in a cellar does.
    #[serde(default)]
    after_dark: bool,
}

fn default_reach() -> f32 {
    14.0
}

fn default_colour() -> [f32; 3] {
    [255.0, 236.0, 198.0]
}

fn default_strength() -> f32 {
    1.8
}

impl Process for LampEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let LampEvent { at, points, reach, colour, strength, after_dark, level } = self;
        let p = cx.pos(at);
        let ground = level.map_or(cx.f.height(p.x, p.y), |l| cx.f.sea + l);
        let srgb = |v: f32| {
            let c = (v / 255.0).clamp(0.0, 1.0);
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let colour = [srgb(colour[0]), srgb(colour[1]), srgb(colour[2])];
        for d in points {
            let t = cx.turned(at, [d[0], d[1]]);
            cx.lamps.push(WorldLamp {
                pos: Vec3::new(p.x + t[0], ground + d[2], p.y + t[1]),
                reach: *reach,
                colour,
                strength: *strength,
                after_dark: *after_dark,
            });
        }
        format!("{} lamps round {:.0},{:.0}, reach {reach} m", points.len(), p.x, p.y)
    }
}
