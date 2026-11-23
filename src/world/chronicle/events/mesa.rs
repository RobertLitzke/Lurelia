use crate::world::chronicle::geom::{dir_deg, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{fbm2, perlin2};
use glam::Vec2;
use serde::Deserialize;

/// A flat-topped block of rock standing `height` m over the ground around
/// it: a mesa, a butte, or with a small radius a spire. Sheer cliffs, then
/// a talus skirt `skirt` m wide that starts `talus` of the way up.
#[derive(Deserialize, Debug)]
pub struct Mesa {
    at: Pos,
    radius: f32,
    /// Metres over the ground round its foot.
    #[serde(default)]
    height: f32,
    /// Or: the flat top's height in metres above sea level, whatever the
    /// ground (towers that must match, to hang a bridge between).
    #[serde(default)]
    top: Option<f32>,
    /// How ragged the outline is (0 a circle).
    #[serde(default = "default_warp")]
    warp: f32,
    /// Stretch along a bearing: [bearing, factor] makes a long fin or ridge.
    #[serde(default)]
    stretch: Option<[f32; 2]>,
    #[serde(default)]
    skirt: Option<f32>,
    #[serde(default = "default_talus")]
    talus: f32,
    /// Surface material on the top.
    #[serde(default)]
    cap: Option<String>,
    #[serde(default)]
    poi: Option<String>,
    /// Ledges: 0 (the default) is one sheer wall. With n, the wall steps
    /// back n times on its way up, each ledge a few metres wide and wider
    /// in some places than others, the way hard beds stand out over soft
    /// ones. It also roughens the outline at the scale of a few metres.
    #[serde(default)]
    tiers: u32,
    /// [bearing, grade]: the beds and the top tilt down toward the bearing
    /// by `grade` (rise per metre), so the bands run at a slant across the face.
    #[serde(default)]
    dip: Option<[f32; 2]>,
}

fn default_warp() -> f32 {
    0.25
}
fn default_talus() -> f32 {
    0.3
}

impl Process for Mesa {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let cap_m = self.cap.as_ref().map(|m| cx.mat(m));
        let msg = self.raise(cx, c, cap_m);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause: cx.cause });
        }
        msg
    }
}

impl Mesa {
    /// Raise the block centred on `c`, capped with material `cap`.
    fn raise(&self, cx: &mut EventCtx, c: Vec2, cap: Option<u8>) -> String {
        let (radius, warp, stretch, talus) = (self.radius, self.warp, self.stretch, self.talus);
        let mut height = self.height;
        let skirt = self.skirt.unwrap_or(radius * 0.8 + 4.0);
        let (seed, cause) = (cx.seed, cx.cause);
        let f = &mut cx.f;
        let (sdir, sfac) = match stretch {
            Some([b, k]) => (dir_deg(b), k.max(0.1)),
            None => (Vec2::Y, 1.0),
        };
        let sperp = Vec2::new(-sdir.y, sdir.x);
        // Elliptical distance in units of the radius, with a ragged outline.
        let edist = |p: Vec2| -> f32 {
            let r = p - c;
            let a = r.dot(sdir) / sfac;
            let b = r.dot(sperp);
            let d = (a * a + b * b).sqrt() / radius;
            let k = 1.6 / radius.max(4.0);
            // Small-scale wobble: a regular bump every 8 m reads as fluting
            // on a long face, so tiered mesas use broken fbm instead.
            let fine = if self.tiers > 0 {
                fbm2(p.x * 0.07, p.y * 0.07, 3, seed + 3) * warp * 0.3
            } else {
                perlin2(p.x * 0.12, p.y * 0.12, seed + 3) * warp * 0.25
            };
            let w = fbm2(p.x * k, p.y * k, 3, seed) * warp + fine;
            d * (1.0 - w)
        };
        let reach = radius * sfac.max(1.0) * (1.0 + warp * 2.0) + skirt + 2.0;
        // It stands on the ground just outside its cliff line: the low fifth of
        // it, so one dip (a wash, a coast) doesn't sink the whole mesa.
        let mut ring: Vec<f32> = Vec::new();
        each_near(f, c, reach, |f: &mut Fields, i, _, p| {
            let e = edist(p);
            if (1.0..1.25).contains(&e) {
                ring.push(f.h[i]);
            }
        });
        ring.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let foot = if ring.is_empty() { f.height(c.x, c.y) } else { ring[ring.len() / 5] };
        if let Some(t) = self.top {
            height = (f.sea + t - foot).max(1.0);
        }
        let top = foot + height;
        let skirt_e = skirt / radius;
        // Ledges: tier k's wall stands `inset[k]` m inside the outline and
        // rises to `lvl[k + 1]` above the foot. The heights are split
        // unevenly between the top of the talus and the top.
        let n = self.tiers as usize;
        let mut lvl = vec![height * talus];
        let mut inset = vec![0.0f32];
        if n > 0 {
            let hs = crate::world::noise::hash32(seed ^ (c.x as u32).wrapping_mul(73_856_093) ^ (c.y as u32).wrapping_mul(19_349_663));
            let mut r = hs;
            let mut rnd = || {
                r = crate::world::noise::hash32(r.wrapping_add(0x9e37_79b9));
                crate::world::noise::unit(r)
            };
            let parts: Vec<f32> = (0..=n).map(|_| 0.6 + rnd()).collect();
            let sum: f32 = parts.iter().sum();
            let mut acc = 0.0;
            for k in 0..n {
                acc += parts[k] / sum;
                lvl.push(height * (talus + (1.0 - talus) * acc));
                inset.push(inset[k] + (2.0 + 4.0 * rnd()).min(radius * 0.25));
            }
        }
        lvl.push(height);
        let (ddir, grade) = match self.dip {
            Some([b, g]) => (dir_deg(b), g),
            None => (Vec2::Y, 0.0),
        };
        let mut cells = 0;
        each_near(f, c, reach, |f: &mut Fields, i, _, p| {
            let e = edist(p);
            let tilt = -grade * (p - c).dot(ddir);
            let want = if e < 1.0 && n > 0 {
                // Metres inside the outline, against each ledge's own inset,
                // which widens and narrows along the face.
                let u = (1.0 - e) * radius;
                let mut lv = lvl[1];
                for k in (1..=n).rev() {
                    let wob = 0.55 + 0.9 * (perlin2(p.x * 0.05 + k as f32 * 7.3, p.y * 0.05, seed + 20 + k as u32) * 0.5 + 0.5);
                    if u >= inset[k] * wob {
                        lv = lvl[k + 1];
                        break;
                    }
                }
                foot + lv + tilt + perlin2(p.x * 0.07, p.y * 0.07, seed + 5) * 0.4
            } else if e < 1.0 {
                top + tilt + perlin2(p.x * 0.07, p.y * 0.07, seed + 5) * 0.6
            } else if e < 1.0 + skirt_e {
                let t = (e - 1.0) / skirt_e;
                foot + height * talus * (1.0 - t).powf(1.6)
            } else {
                return;
            };
            if want > f.h[i] {
                f.h[i] = want;
                f.cause[i] = cause;
                // Rock raised out of a lake's basin is dry ground (an island).
                if f.water[i] == 1 {
                    f.water[i] = 0;
                }
                cells += 1;
                if grade != 0.0 {
                    f.lift_mut()[i] += tilt;
                }
                if (e > 0.9 && e < 1.0 + 1.5 / radius) || n > 0 && e < 1.0 {
                    f.cliff[i] = 1.0;
                }
                if e < 0.92 {
                    if let Some(m) = cap {
                        f.surface[i] = m;
                    }
                }
            }
        });
        format!("{height:.1} m high from {foot:.0} m, r {radius} m ({cells} m2)")
    }
}
