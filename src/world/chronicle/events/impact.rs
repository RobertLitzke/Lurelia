use crate::world::chronicle::geom::{dir_deg, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Item, Poi, Pos};
use crate::world::noise::{hash32, perlin2, smoothstep, unit};
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A crater with a raised rim, a glassy floor, a strewn field of fragments
/// downrange and a strangeness that fades with distance.
#[derive(Deserialize, Debug)]
pub struct Impact {
    at: Pos,
    radius: f32,
    depth: f32,
    rim: f32,
    /// The bearing the body came from; fragments fly the opposite way.
    from_bearing: f32,
    strewn_length: f32,
    strewn_width: f32,
    fragments: u32,
    floor: String,
    item: String,
    /// Surface material over each buried fragment, if any (a tell).
    #[serde(default)]
    tell: Option<String>,
    #[serde(default = "one")]
    strangeness: f32,
}

fn one() -> f32 {
    1.0
}

impl Process for Impact {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Impact { at, radius, depth, rim, from_bearing, strewn_length, strewn_width, fragments, floor, item, tell, strangeness } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(at);
        let floor_m = cx.mat(floor);
        let tell_m = tell.as_ref().map(|t| cx.mat(t));
        let f = &mut cx.f;
        let r = *radius;
        // The rim stands on the ground's mean height around the crater.
        let mut ring = (0.0, 0);
        each_near(f, c, r * 1.1, |f: &mut Fields, i, d, _| {
            if d > r * 0.9 {
                ring.0 += f.h[i];
                ring.1 += 1;
            }
        });
        let base = ring.0 / ring.1.max(1) as f32;
        let down = dir_deg(from_bearing + 180.0);
        let floor_h = base + rim - depth * 0.78;
        each_near(f, c, r * 2.2, |f: &mut Fields, i, d, p| {
            let t = d / r;
            let cosang = if d > 0.01 { (p - c).normalize().dot(down) } else { 0.0 };
            if t < 1.08 {
                f.occupied[i] = true;
            }
            if t < 1.0 {
                let bowl = base + rim - depth * (1.0 - t * t);
                f.h[i] = bowl.max(floor_h);
                if bowl <= floor_h + 0.01 {
                    f.surface[i] = floor_m;
                }
                f.cliff[i] = f.cliff[i].max(smoothstep(0.55, 0.85, t));
            } else {
                let lift = rim * (-((t - 1.0) / 0.38).powi(2)).exp() * (1.0 + 0.6 * cosang.max(0.0));
                f.h[i] += lift * smoothstep(2.2, 1.6, t);
            }
            f.cause[i] = cause;
        });
        // Strangeness: strongest at the crater, fading along the strewn field.
        let perp = Vec2::new(-down.y, down.x);
        for i in 0..f.n * f.n {
            let p = f.centre(i);
            let rel = p - c;
            let d = rel.length();
            let ragged = 1.0 + 0.45 * perlin2(p.x * 0.018, p.y * 0.018, seed) + 0.2 * perlin2(p.x * 0.06, p.y * 0.06, seed + 1);
            let near = (-(d - r).max(0.0) / (r * 1.6 * ragged.max(0.3))).exp();
            let along = rel.dot(down);
            let across = rel.dot(perp);
            let ell = if along > 0.0 {
                let a = along / (strewn_length + r);
                let b = across / (strewn_width * 0.5 * (0.6 + a));
                (1.0 - (a * a + b * b) / ragged.max(0.3)).max(0.0).powf(0.7) * 0.7
            } else {
                0.0
            };
            f.strange[i] = (f.strange[i] + strangeness * near.max(ell)).min(1.0);
        }
        // Fragments: more near the crater, the heaviest flying furthest.
        for k in 0..*fragments {
            let h0 = hash32(seed ^ (k.wrapping_mul(2654435761)));
            let a = unit(h0).sqrt();
            let along = r * 1.1 + a * strewn_length;
            let spread = strewn_width * 0.5 * (0.3 + a);
            let across = (unit(hash32(h0 ^ 5)) * 2.0 - 1.0) * spread;
            let p = c + down * along + perp * across;
            let g = f.height(p.x, p.y);
            if g < f.sea + 0.5 {
                continue;
            }
            let bury = 0.3 + unit(hash32(h0 ^ 9)) * 0.7;
            cx.items.push(Item { kind: item.clone(), pos: Vec3::new(p.x, g - bury, p.y), size: 0.4 + 1.2 * a, cause, surface: false });
            if let Some(t) = tell_m {
                each_near(f, p, 1.6, |f: &mut Fields, i, _, _| f.surface[i] = t);
            }
        }
        // The main mass, deeper, at the far end.
        let p = c + down * (r + strewn_length * 1.05);
        let g = f.height(p.x, p.y);
        if g > f.sea + 0.5 {
            cx.items.push(Item { kind: format!("{item}_mother"), pos: Vec3::new(p.x, g - 2.2, p.y), size: 3.0, cause, surface: false });
            cx.pois.push(Poi { name: "mother stone (buried)".into(), pos: p, cause });
        }
        cx.pois.push(Poi { name: "crater".into(), pos: c, cause });
        format!("crater r {r} m at {:.0},{:.0}, {} fragments downrange toward {:.0} deg", c.x, c.y, fragments, (from_bearing + 180.0) % 360.0)
    }
}
