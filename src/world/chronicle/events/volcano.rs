use crate::world::chronicle::geom::{dir_deg, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{hash32, perlin2, ridged2, smoothstep, unit};
use glam::Vec2;
use serde::Deserialize;

/// A volcanic cone with a summit crater and lava tongues that end in sea cliffs.
#[derive(Deserialize, Debug)]
pub struct Volcano {
    at: Pos,
    radius: f32,
    height: f32,
    #[serde(default)]
    crater_radius: f32,
    #[serde(default)]
    crater_depth: f32,
    #[serde(default)]
    flows: u32,
}

impl Process for Volcano {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Volcano { at, radius, height, crater_radius, crater_depth, flows } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(at);
        let f = &mut cx.f;
        let sea = f.sea;
        let mut summit = 0.0f32;
        each_near(f, c, radius * 1.3, |f: &mut Fields, i, d, p| {
            let t = d / radius;
            let prof = (1.0 - smoothstep(0.0, 1.15, t)).powf(1.5);
            let ang = (p.y - c.y).atan2(p.x - c.x);
            // Radial gullies: ridged noise around the cone.
            let gully = ridged2(ang.cos() * 3.0 + 10.0, ang.sin() * 3.0 + t * 2.5, 3, seed) * smoothstep(0.15, 0.5, t) * (1.0 - smoothstep(0.8, 1.1, t));
            let add = height * prof * (1.0 - 0.12 * gully);
            f.h[i] += add;
            f.cause[i] = cause;
        });
        // Summit crater.
        if *crater_radius > 0.0 {
            each_near(f, c, crater_radius * 1.2, |f: &mut Fields, i, _, _| summit = summit.max(f.h[i]));
            each_near(f, c, *crater_radius, |f: &mut Fields, i, d, _| {
                let t = d / crater_radius;
                f.h[i] = f.h[i].min(summit - crater_depth * (1.0 - t * t));
                f.cliff[i] = f.cliff[i].max(smoothstep(0.6, 0.9, t));
            });
        }
        // Lava tongues, ending in cliffs where they meet the sea.
        for k in 0..*flows {
            let az = unit(hash32(seed + k * 77)) * 360.0;
            let dv = dir_deg(az);
            let w = 16.0 + unit(hash32(seed + k * 91)) * 14.0;
            let len = radius * (0.95 + unit(hash32(seed + k * 13)) * 0.3);
            let perp = Vec2::new(-dv.y, dv.x);
            each_near(f, c, len + w * 2.5, |f: &mut Fields, i, _, p| {
                let rel = p - c;
                let along = rel.dot(dv);
                if along < radius * 0.25 || along > len {
                    return;
                }
                // The tongue wanders and swells rather than running straight.
                let mid = perlin2(along * 0.012, k as f32 * 7.1, seed) * w * 1.6;
                let across = (rel.dot(perp) - mid).abs();
                let ww = w * (0.8 + 0.5 * perlin2(along * 0.03, k as f32 * 3.3 + 9.0, seed).abs()) * (1.0 - 0.35 * along / len);
                if across > ww {
                    return;
                }
                let end = smoothstep(len, len * 0.85, along);
                let lobe = (1.0 - (across / ww).powi(2)).powf(1.5) * 3.0 * end;
                if f.h[i] < sea - 0.5 {
                    return;
                }
                f.h[i] += lobe;
                // Where a tongue reaches the shore it ends in a cliff.
                if f.h[i] < sea + 7.0 && along > radius * 0.75 {
                    f.h[i] = f.h[i].max(sea + 5.0 + lobe);
                    f.cliff[i] = 1.0;
                }
                f.cause[i] = cause;
            });
        }
        cx.pois.push(Poi { name: "volcano summit".into(), pos: c, cause });
        format!("cone {height} m high, r {radius} m, {flows} flows")
    }
}
