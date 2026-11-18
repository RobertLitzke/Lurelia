use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A reef: a fold where the rock beds bend up across a strip `width` m wide
/// along the line `from` -> `to`, rising `height` m toward the right of the
/// line. Erosion has stripped the high side, so what stands is a tilted
/// ridge: a long dip slope climbing with the beds on the left, a crest, and
/// a steep scarp on the right where the tilted bands show in section. The
/// beds stay lifted under the right side (`Fields::lift`), so the rock there
/// is older than at the same height elsewhere.
///
/// `cracks` notches are cut through the crest, each about `crack_width` m
/// wide, their floors climbing from the scarp foot at `grade` (rise per
/// metre) until they meet the dip slope: the ways a walker gets to the top.
/// The fold dies out over `taper` m at each end.
#[derive(Deserialize, Debug)]
pub struct Reef {
    from: Pos,
    to: Pos,
    height: f32,
    #[serde(default = "default_width")]
    width: f32,
    /// Where the crest stands across the fold, 0 (left edge) to 1 (right edge).
    #[serde(default = "default_crest")]
    crest: f32,
    /// Horizontal run of the scarp face, metres.
    #[serde(default = "default_scarp")]
    scarp: f32,
    #[serde(default)]
    cracks: u32,
    #[serde(default = "default_crack_width")]
    crack_width: f32,
    #[serde(default = "default_grade")]
    grade: f32,
    #[serde(default = "default_taper")]
    taper: f32,
    /// Surface material on the crack floors (a sandy wash).
    #[serde(default)]
    floor: Option<String>,
    #[serde(default)]
    poi: Option<String>,
}

fn default_width() -> f32 {
    120.0
}
fn default_crest() -> f32 {
    0.6
}
fn default_scarp() -> f32 {
    7.0
}
fn default_crack_width() -> f32 {
    4.0
}
fn default_grade() -> f32 {
    0.45
}
fn default_taper() -> f32 {
    90.0
}

impl Process for Reef {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let (a, b) = (cx.pos(&self.from), cx.pos(&self.to));
        let floor_m = self.floor.as_ref().map(|m| cx.mat(m));
        let f = &mut cx.f;
        let len = a.distance(b).max(1.0);
        let d = (b - a) / len;
        let right = Vec2::new(-d.y, d.x);
        let mid = (a + b) * 0.5;
        let half = self.width * 0.5;
        let s_crest = -half + self.width * self.crest.clamp(0.1, 0.95);
        // Cracks spaced along the middle of the reef, nudged off even spacing.
        let cracks: Vec<f32> = (0..self.cracks)
            .map(|k| {
                let t = (k as f32 + 0.5) / self.cracks as f32;
                let j = perlin2(k as f32 * 3.1, 0.5, seed + 4) * 0.35 / self.cracks as f32;
                self.taper * 0.6 + (len - self.taper * 1.2) * (t + j).clamp(0.02, 0.98)
            })
            .collect();
        let reach = len * 0.5 + self.width + self.taper;
        let (mut cells, mut cut) = (0, 0);
        let mut crest_top: f32 = 0.0;
        // The ground before the fold, so the scarp foot is read from the plain.
        let h0 = f.h.clone();
        f.lift_mut();
        each_near(f, mid, reach, |f: &mut Fields, i, _, p| {
            let rel = p - a;
            let along = rel.dot(d);
            let fade = smoothstep(-self.taper * 0.3, self.taper, along) * smoothstep(len + self.taper * 0.3, len - self.taper, along);
            if fade <= 0.0 || h0[i] < f.sea {
                return;
            }
            // The crest line and the fold wander a little along the strike.
            let wob = perlin2(along * 0.012, 3.0, seed) * self.width * 0.12 + perlin2(along * 0.05, 7.0, seed + 1) * 2.5;
            let s = rel.dot(right) - wob;
            if s < -half - 2.0 {
                return;
            }
            let height = self.height * fade;
            // Beds: a straight dip across the limb, eased at both hinges.
            let u = ((s + half) / self.width).clamp(0.0, 1.0);
            let bed = height * (u * 0.9 + 0.1 * smoothstep(0.0, 1.0, u));
            // Past the fold the beds stay up, then the lift eases off well
            // beyond the scarp, where nothing cuts deep enough to show it.
            f.lift[i] += bed * smoothstep(half + 70.0, half + 20.0, s);
            // Ground: up the dip slope with the beds, then down the scarp.
            let crest_h = height * ((s_crest + half) / self.width * 0.9 + 0.1 * smoothstep(0.0, 1.0, (s_crest + half) / self.width));
            let scarp = self.scarp * (0.8 + 0.4 * (perlin2(along * 0.04, 1.0, seed + 2) * 0.5 + 0.5));
            let raise = if s <= s_crest {
                // A little rounding at the very crest, and gentle ribs down the slope.
                bed - smoothstep(s_crest - 3.0, s_crest, s) * 0.6 + perlin2(along * 0.09, s * 0.03, seed + 3) * 0.3 * u
            } else {
                // The scarp: steep, ledgy (a step where the beds are hard), then
                // a short apron of fallen blocks.
                let t = ((s - s_crest) / scarp).clamp(0.0, 1.0);
                let q = (t * 3.0).min(2.999);
                let fall = (q.floor() + smoothstep(0.45, 1.0, q.fract())) / 3.0;
                let face = crest_h * (1.0 - fall);
                let apron = crest_h * 0.18 * (1.0 - smoothstep(s_crest + scarp, s_crest + scarp + crest_h * 0.6 + 4.0, s));
                face.max(apron).max(0.0)
            };
            if raise <= 0.0 {
                return;
            }
            let mut want = h0[i] + raise;
            // The cracks: notches whose floor climbs from the scarp foot.
            for &c in &cracks {
                let w = self.crack_width * (0.8 + 0.5 * (perlin2(s * 0.08, c * 0.1, seed + 5) * 0.5 + 0.5));
                let off = (along - c - perlin2(s * 0.05, c, seed + 6) * 3.0).abs();
                // A fan at the bottom, so the way in is easy to find.
                let fan = smoothstep(s_crest + 2.0, s_crest + scarp + 6.0, s) * 4.0;
                if off < w * 0.5 + fan {
                    let floor = h0[i] + (s_crest + scarp + 3.0 - s).max(0.0) * self.grade;
                    if floor < want {
                        want = floor;
                        cut += 1;
                        if let Some(m) = floor_m {
                            f.surface[i] = m;
                        }
                    }
                } else if off < w * 0.5 + fan + 1.2 && want > h0[i] + (s_crest + scarp + 3.0 - s).max(0.0) * self.grade + 1.5 {
                    f.cliff[i] = 1.0;
                }
            }
            if want > f.h[i] {
                f.h[i] = want;
                f.cause[i] = cause;
                cells += 1;
                if s > s_crest - 1.0 && s < s_crest + scarp {
                    f.cliff[i] = 1.0;
                }
                crest_top = crest_top.max(raise);
            }
        });
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: mid, cause });
        }
        format!("reef {len:.0} m long, crest up to {crest_top:.0} m, {} cracks ({cut} m2), {cells} m2 raised", self.cracks)
    }
}
