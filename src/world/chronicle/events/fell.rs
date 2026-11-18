use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::noise::{fbm2, hash2, perlin2, smoothstep, unit};
use glam::Vec2;
use serde::Deserialize;

/// What wind and frost make of high snowy ground, so a plateau is not one
/// white sheet. Applies to every open cell over `above` m (inside `radius`
/// of `at` if given); cliffs and anything an earlier event surfaced (a
/// glacier, a road) are left alone.
///
/// - `scour`: the share of ground the wind has blown bare, in patches, where
///   the dark rock shows through.
/// - `tors`: how many black rock knobs stand up through the snow (0 to 1, a
///   chance per 22 m square), up to about `tor_height` m.
/// - `polygons`: the share of ground cracked into frost polygons, a few
///   metres across, the cracks lined with gravel (patterned ground).
/// - `drifts`: sastrugi, the hard ridges wind carves in old snow, this many
///   metres high, lying across the wind from `wind` (a bearing it blows from).
#[derive(Deserialize, Debug)]
pub struct Fell {
    #[serde(default)]
    at: Option<Pos>,
    #[serde(default)]
    radius: Option<f32>,
    above: f32,
    #[serde(default)]
    wind: f32,
    #[serde(default)]
    scour: f32,
    #[serde(default)]
    rock: Option<String>,
    #[serde(default)]
    tors: f32,
    #[serde(default = "default_tor_height")]
    tor_height: f32,
    #[serde(default)]
    polygons: f32,
    #[serde(default)]
    gravel: Option<String>,
    #[serde(default)]
    drifts: f32,
}

fn default_tor_height() -> f32 {
    4.0
}

/// The size of the squares tors are dealt in, metres.
const TOR_CELL: f32 = 22.0;
/// The size of the squares frost polygons are seeded in, metres.
const POLY_CELL: f32 = 8.0;

impl Process for Fell {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let centre = self.at.as_ref().map(|p| cx.pos(p));
        let rock = self.rock.as_ref().map(|m| cx.mat(m));
        let gravel = self.gravel.as_ref().map(|m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let b = self.wind.to_radians();
        let wind = Vec2::new(b.sin(), -b.cos());
        let f = &mut cx.f;
        let (mut bare, mut tor_cells, mut cracks, mut ridged) = (0, 0, 0, 0);
        for i in 0..f.h.len() {
            let p = f.centre(i);
            if let (Some(c), Some(r)) = (centre, self.radius) {
                if p.distance(c) > r {
                    continue;
                }
            }
            if f.h[i] < self.above || f.cliff[i] > 0.5 || f.surface[i] != 0 || f.h[i] < f.sea + 1.0 {
                continue;
            }
            // Fade in over the first few metres above `above`.
            let fade = smoothstep(self.above, self.above + 6.0, f.h[i]);

            // Tors: the nearest knob dealt in the 3x3 squares round here.
            if self.tors > 0.0 {
                let (gx, gz) = ((p.x / TOR_CELL).floor() as i32, (p.y / TOR_CELL).floor() as i32);
                let mut lift = 0.0f32;
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let h = hash2(gx + dx, gz + dz, seed ^ 0x7012);
                        if unit(h) > self.tors {
                            continue;
                        }
                        let c = Vec2::new(
                            (gx + dx) as f32 * TOR_CELL + unit(hash2(gx + dx, gz + dz, seed + 1)) * TOR_CELL,
                            (gz + dz) as f32 * TOR_CELL + unit(hash2(gx + dx, gz + dz, seed + 2)) * TOR_CELL,
                        );
                        let r = 2.5 + 4.5 * unit(hash2(gx + dx, gz + dz, seed + 3));
                        // Jointed, so the outline is ragged rather than round.
                        let d = p.distance(c) * (1.0 + 0.3 * perlin2(p.x * 0.4, p.y * 0.4, seed + 4));
                        if d < r {
                            let ht = self.tor_height * (0.5 + unit(hash2(gx + dx, gz + dz, seed + 5)));
                            let k = 1.0 - (d / r).powi(2);
                            // Stacked blocks: the profile steps every metre or so.
                            let v = ht * k.powf(0.35);
                            lift = lift.max((v * 1.2).floor() / 1.2 + 0.4 * perlin2(p.x * 0.7, p.y * 0.7, seed + 6));
                        }
                    }
                }
                if lift > 0.3 {
                    f.h[i] += lift * fade;
                    f.cliff[i] = 1.0;
                    f.cause[i] = cause;
                    tor_cells += 1;
                    continue;
                }
            }

            // Wind-scoured patches: the snow blown off a rise, the rock bare.
            let n = fbm2(p.x * 0.018, p.y * 0.018, 3, seed + 7) + 0.25 * perlin2(p.x * 0.09, p.y * 0.09, seed + 8);
            let cut = 0.42 - 1.1 * self.scour;
            if self.scour > 0.0 && n * fade > cut {
                if let Some(m) = rock {
                    f.surface[i] = m;
                }
                f.h[i] -= 0.25;
                f.cause[i] = cause;
                bare += 1;
                continue;
            }

            // Frost polygons, in their own patches.
            if self.polygons > 0.0 && perlin2(p.x * 0.012, p.y * 0.012, seed + 9) * fade > 1.0 - 2.0 * self.polygons {
                if crack_at(p, seed + 10) < 0.45 {
                    f.h[i] -= 0.3;
                    if let Some(m) = gravel {
                        f.surface[i] = m;
                    }
                    f.cause[i] = cause;
                    cracks += 1;
                    continue;
                }
            }

            // Sastrugi: sharp-crested ridges across the wind, wavering.
            if self.drifts > 0.0 {
                let along = p.dot(wind);
                let phase = along / 4.5 * std::f32::consts::TAU + perlin2(p.x * 0.04, p.y * 0.04, seed + 11) * 4.0;
                let patch = smoothstep(-0.2, 0.4, perlin2(p.x * 0.02, p.y * 0.02, seed + 12));
                let r = smoothstep(0.45, 1.0, phase.sin()) * self.drifts * patch * fade;
                if r > 0.05 {
                    f.h[i] += r;
                    ridged += 1;
                }
            }
        }
        format!("fell: {bare} m2 scoured bare, {tor_cells} m2 of tors, {cracks} m2 of frost cracks, {ridged} m2 of sastrugi")
    }
}

/// How far `p` is from the edge between its two nearest frost-polygon
/// centres (F2 - F1 of a jittered grid), metres.
fn crack_at(p: Vec2, seed: u32) -> f32 {
    // Warped, so the cracks wander instead of lining up with the grid.
    let p = p + Vec2::new(perlin2(p.x * 0.11, p.y * 0.11, seed + 5), perlin2(p.x * 0.11, p.y * 0.11, seed + 6)) * 2.5;
    let (gx, gz) = ((p.x / POLY_CELL).floor() as i32, (p.y / POLY_CELL).floor() as i32);
    let (mut d1, mut d2) = (f32::MAX, f32::MAX);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (cx, cz) = (gx + dx, gz + dz);
            let c = Vec2::new(
                (cx as f32 + 0.05 + 0.9 * unit(hash2(cx, cz, seed))) * POLY_CELL,
                (cz as f32 + 0.05 + 0.9 * unit(hash2(cx, cz, seed + 1))) * POLY_CELL,
            );
            let d = p.distance(c);
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
