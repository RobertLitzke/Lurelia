use crate::world::chronicle::geom::{dir_deg, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{fbm2, perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A dune field: wind-blown sand heaped into ridges across the wind, gentle
/// on the windward side and steep (the slip face) on the lee, their crests
/// wandering and breaking up. `wind_bearing` is the way the wind blows
/// toward. The field fades out over its last third, raggedly, and its sand
/// becomes the surface (`mat`) and a region (`name`) a biome can claim.
#[derive(Deserialize, Debug)]
pub struct Dunes {
    at: Pos,
    radius: f32,
    /// Crest-to-crest spacing, metres.
    #[serde(default = "default_spacing")]
    spacing: f32,
    /// Crest height over the troughs, metres.
    #[serde(default = "default_height")]
    height: f32,
    #[serde(default)]
    wind_bearing: f32,
    /// Sand under the whole field, a few metres deep.
    #[serde(default)]
    mat: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    poi: Option<String>,
}

fn default_spacing() -> f32 {
    40.0
}
fn default_height() -> f32 {
    6.0
}

impl Process for Dunes {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(&self.at);
        let mat = self.mat.as_ref().map(|m| cx.mat(m));
        let region = self.name.as_ref().map(|n| cx.f.region_id(n));
        let f = &mut cx.f;
        let wind = dir_deg(self.wind_bearing);
        let across = Vec2::new(-wind.y, wind.x);
        let (sp, ht, r) = (self.spacing, self.height, self.radius);
        let mut cells = 0;
        let mut top: f32 = 0.0;
        each_near(f, c, r * 1.3, |f: &mut Fields, i, d, p| {
            // A ragged edge: the field thins out over its outer third.
            let edge = d / r + fbm2(p.x * 0.012, p.y * 0.012, 3, seed) * 0.35;
            let fade = smoothstep(1.0, 0.65, edge);
            if fade <= 0.0 || f.h[i] < f.sea + 0.5 {
                return;
            }
            let q = p - c;
            let a = q.dot(across);
            // Crests bow downwind and wander; each ridge has its own height.
            let bow = perlin2(a * 0.012, 0.0, seed + 1) * sp * 0.9 + perlin2(a * 0.04, 3.0, seed + 2) * sp * 0.2;
            let w = (q.dot(wind) + bow) / sp;
            let k = w.floor();
            let t = w - k;
            // Windward 75% of the spacing rises gently, the last 25% falls steeply.
            let prof = if t < 0.75 { smoothstep(0.0, 0.75, t).powf(1.3) } else { 1.0 - smoothstep(0.75, 1.0, t).powf(0.7) };
            let ridge = 0.6 + 0.4 * (perlin2(k * 1.7, a * 0.02, seed + 3) * 0.5 + 0.5);
            // Crests break into separate dunes here and there.
            let breaks = smoothstep(0.2, 0.55, perlin2(a * 0.03, k * 0.9, seed + 4) * 0.5 + 0.5);
            let raise = (ht * prof * ridge * (0.35 + 0.65 * breaks) + ht * 0.25) * fade;
            if raise <= 0.05 {
                return;
            }
            f.h[i] += raise;
            f.cause[i] = cause;
            f.cliff[i] = 0.0;
            if let Some(m) = mat {
                if fade > 0.3 {
                    f.surface[i] = m;
                }
            }
            if let Some(rg) = region {
                if fade > 0.15 {
                    f.region[i] = rg;
                }
            }
            cells += 1;
            top = top.max(raise);
        });
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        }
        format!("dune field r {r} m, crests up to {top:.1} m, {cells} m2")
    }
}
