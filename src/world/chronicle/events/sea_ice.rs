use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{hash2, perlin2, smoothstep, unit};
use glam::Vec2;
use serde::Deserialize;

/// Sea ice in a sheltered bay: a sheet over the water `above` m proud of the
/// sea, that you can walk out on. Old white ice in the middle with snow
/// lying on it in drifts, younger grey ice toward the open water, and at the
/// edge broken floes with dark water between. `leads` (0 to 1) opens long
/// cracks of black water across the sheet.
///
/// The sheet only covers what was under the sea, within about `radius` m of
/// `at`; the shore stays as it was.
#[derive(Deserialize, Debug)]
pub struct SeaIce {
    at: Pos,
    radius: f32,
    ice: String,
    #[serde(default)]
    young: Option<String>,
    #[serde(default)]
    snow: Option<String>,
    #[serde(default = "default_above")]
    above: f32,
    #[serde(default)]
    leads: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn default_above() -> f32 {
    0.3
}

/// Floes are dealt in squares this size, metres.
const FLOE: f32 = 5.0;
/// Leads run along the edges of cells this size.
const LEAD: f32 = 28.0;

impl Process for SeaIce {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let ice = cx.mat(&self.ice);
        let young = self.young.as_ref().map(|m| cx.mat(m)).unwrap_or(ice);
        let snow = self.snow.as_ref().map(|m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let (r, above, leads) = (self.radius.max(4.0), self.above, self.leads);
        let mut sheet = 0;
        let mut open = 0;
        each_near(&mut cx.f, c, r * 1.35, |f: &mut Fields, i, d, _| {
            if f.h[i] >= f.sea {
                return;
            }
            let p = f.centre(i);
            let v = p - c;
            // A ragged edge: the ice reaches further into some corners.
            let a = v.y.atan2(v.x);
            let reach = r * (1.0 + 0.22 * perlin2(a.cos() * 2.0 + 7.0, a.sin() * 2.0, seed) + 0.1 * perlin2(p.x * 0.08, p.y * 0.08, seed + 1));
            if d > reach {
                return;
            }
            let k = d / reach;
            // The broken edge: floes, about half of them gone.
            if k > 0.78 {
                let (gx, gz) = ((p.x / FLOE).floor() as i32, (p.y / FLOE).floor() as i32);
                let keep = 1.0 - smoothstep(0.78, 1.0, k);
                if unit(hash2(gx, gz, seed + 2)) > keep * 0.9 || edge(p, FLOE, seed + 3) < 0.35 {
                    open += 1;
                    return;
                }
            }
            // Leads: long cracks along the edges of big cells, in patches.
            if leads > 0.0 && edge(p, LEAD, seed + 4) < 0.6 && perlin2(p.x * 0.015, p.y * 0.015, seed + 5) > 1.0 - 2.0 * leads {
                open += 1;
                return;
            }
            f.h[i] = f.sea + above + 0.05 * perlin2(p.x * 0.3, p.y * 0.3, seed + 6);
            f.cliff[i] = 0.0;
            f.cause[i] = cause;
            let drift = perlin2(p.x * 0.05, p.y * 0.05, seed + 7) + 0.3 * perlin2(p.x * 0.2, p.y * 0.2, seed + 8);
            f.surface[i] = match snow {
                Some(s) if k < 0.6 && drift > 0.05 => s,
                _ if k > 0.62 + 0.08 * perlin2(p.x * 0.1, p.y * 0.1, seed + 9) => young,
                _ => ice,
            };
            sheet += 1;
        });
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        }
        format!("sea ice r {r:.0} m: {sheet} m2 of ice, {open} m2 left open")
    }
}

/// How far `p` is from the edge between its two nearest centres of a
/// jittered grid of `cell` m squares (F2 - F1, halved), metres.
fn edge(p: Vec2, cell: f32, seed: u32) -> f32 {
    let p = p + Vec2::new(perlin2(p.x * 0.09, p.y * 0.09, seed + 1), perlin2(p.x * 0.09, p.y * 0.09, seed + 2)) * cell * 0.25;
    let (gx, gz) = ((p.x / cell).floor() as i32, (p.y / cell).floor() as i32);
    let (mut d1, mut d2) = (f32::MAX, f32::MAX);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (x, z) = (gx + dx, gz + dz);
            let q = Vec2::new((x as f32 + 0.05 + 0.9 * unit(hash2(x, z, seed))) * cell, (z as f32 + 0.05 + 0.9 * unit(hash2(x, z, seed + 7))) * cell);
            let d = p.distance(q);
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d2 - d1) * 0.5
}
