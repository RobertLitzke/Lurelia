use crate::world::chronicle::geom::{dir_deg, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::perlin2;
use glam::Vec2;
use serde::Deserialize;

/// A fire burning further downwind than up; sets each burned cell's burn year
/// (biome rules read burn age) and can leave a buried char layer.
#[derive(Deserialize, Debug)]
pub struct Fire {
    at: Pos,
    radius: f32,
    /// Bearing the wind blew toward.
    wind_bearing: f32,
    #[serde(default = "one")]
    stretch: f32,
    #[serde(default)]
    char: Option<String>,
}

fn one() -> f32 {
    1.0
}

impl Process for Fire {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Fire { at, radius, wind_bearing, stretch, char } = self;
        let (seed, cause, year) = (cx.seed, cx.cause, cx.year);
        let c = cx.pos(at);
        let wd = dir_deg(*wind_bearing);
        let cm = char.as_ref().map(|m| cx.mat(m));
        let mut burned = 0;
        each_near(&mut cx.f, c, radius * (1.0 + stretch) * 1.3, |f: &mut Fields, i, _, p| {
            if f.h[i] < f.sea + 0.3 || f.water[i] == 1 {
                return;
            }
            let rel = p - c;
            let along = rel.dot(wd);
            let across = rel.dot(Vec2::new(-wd.y, wd.x));
            // Burns further downwind, less upwind.
            let reach = if along > 0.0 { radius * (1.0 + stretch) } else { radius * 0.6 };
            let e = (along / reach).powi(2) + (across / radius).powi(2);
            let ragged = perlin2(p.x * 0.03, p.y * 0.03, seed) * 0.35;
            if e < 1.0 + ragged {
                f.burn_year[i] = year;
                if let Some(m) = cm {
                    f.add_layer(i, m, 0.25, 0.25, false);
                }
                f.cause[i] = cause;
                burned += 1;
            }
        });
        format!("{burned} m2 burned, wind toward {wind_bearing} deg")
    }
}
