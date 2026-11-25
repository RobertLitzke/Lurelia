use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::noise::{hash2, perlin2, smoothstep, unit};
use glam::Vec2;
use serde::Deserialize;

/// The surface of an old lava flow, so a field of it is not one flat sheet:
/// what a flow leaves as it cools, on every cell of `region` (cells an
/// earlier event surfaced, cliffs and kipukas painted as other regions are
/// left alone). The flow runs away from `from`, its vent.
///
/// - `lobes`: the flow came as overlapping tongues, each a few tens of
///   metres across (`lobe_size`), its toe a step this many metres high and
///   its top a little domed; neighbouring lobes stand at different heights.
/// - `ropes`: ropy pahoehoe, low folds `rope_spacing` m apart curving round
///   the front of each lobe, convex downstream, this many metres high.
/// - `ridges`: pressure ridges, long low walls pushed up where the crust
///   buckled, running across the flow and split along their crests, this
///   many metres high, in patches, rubble (`aa_mat`) along the split.
/// - `aa`: the share of the field that is rough aa clinker, in tongues,
///   surfaced in `aa_mat` with a jagged top.
/// - `cracks`: the share of ground cut by cooling cracks, metres-wide
///   polygons whose cracks fill with `crack_mat` (soil, where ferns take).
#[derive(Deserialize, Debug)]
pub struct Pahoehoe {
    region: String,
    from: Pos,
    #[serde(default = "d_lobes")]
    lobes: f32,
    #[serde(default = "d_lobe_size")]
    lobe_size: f32,
    #[serde(default = "d_ropes")]
    ropes: f32,
    #[serde(default = "d_rope_spacing")]
    rope_spacing: f32,
    #[serde(default)]
    ridges: f32,
    #[serde(default)]
    aa: f32,
    #[serde(default)]
    aa_mat: Option<String>,
    #[serde(default)]
    cracks: f32,
    #[serde(default)]
    crack_mat: Option<String>,
}

fn d_lobes() -> f32 {
    1.2
}
fn d_lobe_size() -> f32 {
    34.0
}
fn d_ropes() -> f32 {
    0.3
}
fn d_rope_spacing() -> f32 {
    2.4
}

/// The two nearest of a jittered grid of points `cell` m apart, warped so
/// the edges wander: (nearest point's grid key, F1, F2).
fn cells(p: Vec2, cell: f32, seed: u32) -> ((i32, i32), Vec2, f32, f32) {
    let w = p + Vec2::new(perlin2(p.x * 0.03, p.y * 0.03, seed + 5), perlin2(p.x * 0.03, p.y * 0.03, seed + 6)) * (cell * 0.25);
    let (gx, gz) = ((w.x / cell).floor() as i32, (w.y / cell).floor() as i32);
    let (mut d1, mut d2, mut k1, mut c1) = (f32::MAX, f32::MAX, (0, 0), Vec2::ZERO);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (cx, cz) = (gx + dx, gz + dz);
            let c = Vec2::new((cx as f32 + 0.1 + 0.8 * unit(hash2(cx, cz, seed))) * cell, (cz as f32 + 0.1 + 0.8 * unit(hash2(cx, cz, seed + 1))) * cell);
            let d = w.distance(c);
            if d < d1 {
                d2 = d1;
                d1 = d;
                k1 = (cx, cz);
                c1 = c;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (k1, c1, d1, d2)
}

impl Process for Pahoehoe {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let vent = cx.pos(&self.from);
        let aa_mat = self.aa_mat.as_ref().map(|m| cx.mat(m));
        let crack_mat = self.crack_mat.as_ref().map(|m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let Some(rid) = cx.f.regions.iter().position(|r| r == &self.region) else {
            cx.warnings.push(format!("pahoehoe: no region called {}", self.region));
            return "skipped".into();
        };
        let rid = rid as u8 + 1;
        let f = &mut cx.f;
        let (mut n, mut ropy, mut ridged, mut aa, mut cracked) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for i in 0..f.h.len() {
            if f.region[i] != rid || f.cliff[i] > 0.5 || f.surface[i] != 0 || f.water[i] != 0 {
                continue;
            }
            let p = f.centre(i);
            n += 1;
            let down = (p - vent).normalize_or_zero();
            let mut lift = 0.0f32;

            // Lobes: a step at each toe, a domed top, each its own height.
            let (key, centre, d1, d2) = cells(p, self.lobe_size, seed ^ 0x10be);
            let edge = (d2 - d1) * 0.5;
            let own = (unit(hash2(key.0, key.1, seed + 7)) - 0.5) * self.lobes;
            let toe = smoothstep(0.0, 3.5, edge);
            let dome = (1.0 - (d1 / (self.lobe_size * 0.7)).min(1.0)).powf(0.6) * 0.5;
            lift += self.lobes * (0.6 * toe + 0.4 * dome) + own * toe;

            // Ropes: folds curving round the lobe's front, convex downstream.
            if self.ropes > 0.0 {
                let up = centre - down * (self.lobe_size * 0.55);
                let r = p.distance(up) + perlin2(p.x * 0.08, p.y * 0.08, seed + 8) * 1.4;
                let fold = smoothstep(0.35, 1.0, (r / self.rope_spacing * std::f32::consts::TAU).sin());
                let patch = smoothstep(-0.25, 0.35, perlin2(p.x * 0.025, p.y * 0.025, seed + 9));
                let v = fold * self.ropes * patch * toe;
                if v > 0.04 {
                    ropy += 1;
                }
                lift += v;
            }

            // Pressure ridges: across the flow, long, split on the crest.
            if self.ridges > 0.0 {
                let across = Vec2::new(-down.y, down.x);
                let q = Vec2::new(p.dot(down) * 0.018, p.dot(across) * 0.005);
                let line = perlin2(q.x + perlin2(p.x * 0.01, p.y * 0.01, seed + 10) * 0.6, q.y, seed + 11).abs();
                let patch = smoothstep(0.15, 0.5, perlin2(p.x * 0.006, p.y * 0.006, seed + 12));
                let k = (1.0 - line / 0.07).max(0.0) * patch;
                if k > 0.0 {
                    let crest = if line < 0.012 { 0.55 } else { 1.0 };
                    lift += self.ridges * k.powf(0.7) * crest;
                    if line < 0.012 && patch > 0.5 {
                        if let Some(m) = aa_mat {
                            f.surface[i] = m;
                        }
                    }
                    ridged += 1;
                }
            }

            // Aa: rough clinker in tongues, a jagged top.
            if self.aa > 0.0 {
                let a = perlin2(p.x * 0.012 + down.x * 0.4, p.y * 0.012 + down.y * 0.4, seed + 13);
                if a > 1.0 - 2.0 * self.aa {
                    lift += 0.6 + 0.5 * perlin2(p.x * 0.5, p.y * 0.5, seed + 14) + 0.45 * perlin2(p.x * 1.9, p.y * 1.9, seed + 15) + 0.35 * (unit(hash2(p.x as i32, p.y as i32, seed + 17)) - 0.5);
                    if let Some(m) = aa_mat {
                        f.surface[i] = m;
                    }
                    aa += 1;
                }
            }

            // Cooling cracks, soil-filled.
            if self.cracks > 0.0 && perlin2(p.x * 0.015, p.y * 0.015, seed + 16) > 1.0 - 2.0 * self.cracks {
                let (_, _, c1, c2) = cells(p, 5.0, seed ^ 0xc7ac);
                if (c2 - c1) * 0.5 < 0.28 {
                    lift -= 0.35;
                    if let Some(m) = crack_mat {
                        f.surface[i] = m;
                    }
                    cracked += 1;
                }
            }

            f.h[i] += lift;
            f.cause[i] = cause;
        }
        format!("pahoehoe {}: {n} m2, ropes on {ropy}, pressure ridges on {ridged}, aa on {aa}, cracks on {cracked}", self.region)
    }
}
