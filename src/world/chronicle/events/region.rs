use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::perlin2;
use serde::Deserialize;

/// Paints a named region for biome rules: a ragged disc, or with `poly` the
/// inside of that outline exactly (a walled field).
#[derive(Deserialize, Debug)]
pub struct Region {
    name: String,
    #[serde(default)]
    at: Option<Pos>,
    #[serde(default)]
    radius: f32,
    #[serde(default)]
    poly: Vec<Pos>,
    /// In place of `poly`: a band beside the race track
    /// (`race_track::AlongBand`).
    #[serde(default)]
    along: Option<crate::world::chronicle::race_track::AlongBand>,
}

impl Process for Region {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Region { name, at, radius, poly, along } = self;
        let seed = cx.seed;
        let rid = cx.f.region_id(name);
        let pts: Vec<glam::Vec2> = match along {
            Some(b) => crate::world::chronicle::race_track::along_band(cx, b),
            None => poly.iter().map(|p| cx.pos(p)).collect(),
        };
        if pts.len() >= 3 {
            let (mut lo, mut hi) = (pts[0], pts[0]);
            for p in &pts {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            let f = &mut cx.f;
            let mut cells = 0usize;
            for i in 0..f.n * f.n {
                let p = f.centre(i);
                if p.x < lo.x || p.y < lo.y || p.x > hi.x || p.y > hi.y {
                    continue;
                }
                // Even-odd crossing count.
                let mut inside = false;
                let mut j = pts.len() - 1;
                for k in 0..pts.len() {
                    let (a, b) = (pts[k], pts[j]);
                    if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                        inside = !inside;
                    }
                    j = k;
                }
                if inside {
                    f.region[i] = rid;
                    cells += 1;
                }
            }
            return format!("region {name}: {cells} m2 inside its outline");
        }
        let Some(at) = at else {
            return format!("region {name}: no `at` and no `poly`, skipped");
        };
        let (c, radius) = (cx.pos(at), radius);
        each_near(&mut cx.f, c, radius * 1.2, |f: &mut Fields, i, d, p| {
            if d < radius * (1.0 + 0.2 * perlin2(p.x * 0.05, p.y * 0.05, seed)) {
                f.region[i] = rid;
            }
        });
        format!("region {name}")
    }
}
