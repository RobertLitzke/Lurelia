use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::noise::{hash32, unit};
use glam::Vec2;
use serde::Deserialize;

/// Tall landmarks: `count` of the listed structures (picked at random)
/// within `radius` m of `at`, at least `spacing` m apart, each turned at
/// random. One makes a single landmark; dozens make a hoodoo forest, which
/// should be very rare. Structures with base "sea" stand only in water
/// 1.5 to 16 m deep; the rest on dry, unoccupied ground. One point of
/// interest at the middle, named `poi`.
#[derive(Deserialize, Debug)]
pub struct Spires {
    structures: Vec<String>,
    at: Pos,
    #[serde(default)]
    radius: f32,
    #[serde(default = "one")]
    count: u32,
    #[serde(default = "default_spacing")]
    spacing: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn one() -> u32 {
    1
}
fn default_spacing() -> f32 {
    8.0
}

impl Process for Spires {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Spires { structures, at, radius, count, spacing, poi } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(at);
        let mut defs = Vec::new();
        for s in structures {
            match cx.lib.structure(s) {
                Some(d) => defs.push(d),
                None => cx.warnings.push(format!("spires: unknown structure {s}")),
            }
        }
        if defs.is_empty() {
            return "skipped".into();
        }
        let mut rng = seed | 1;
        let mut next = move || {
            rng = hash32(rng.wrapping_add(0x9e37_79b9));
            unit(rng)
        };
        let mut done: Vec<Vec2> = Vec::new();
        let mut tries = 0;
        while (done.len() as u32) < *count && tries < count * 40 + 40 {
            tries += 1;
            let d = defs[(next() * defs.len() as f32) as usize % defs.len()];
            let a = next() * std::f32::consts::TAU;
            let r = if *radius < 1.0 { 0.0 } else { radius * next().sqrt() };
            let p = c + Vec2::new(a.cos(), a.sin()) * r;
            let f = &cx.f;
            if p.x < 12.0 || p.y < 12.0 || p.x > f.size - 12.0 || p.y > f.size - 12.0 {
                continue;
            }
            if done.iter().any(|q| q.distance(p) < *spacing) {
                continue;
            }
            let i = f.idx(p.x, p.y);
            let at_sea = cx.lib.structures[d].base.as_deref() == Some("sea");
            let ok = if at_sea {
                f.h[i] < f.sea - 1.5 && f.h[i] > f.sea - 16.0
            } else {
                f.water[i] == 0 && !f.occupied[i] && f.h[i] > f.sea + 1.0
            };
            if !ok {
                continue;
            }
            let bearing = next() * 360.0;
            cx.place(d, p, bearing, &HashMap::default(), 0.0, seed ^ (done.len() as u32).wrapping_mul(0x51ed));
            done.push(p);
        }
        if done.is_empty() {
            cx.warnings.push(format!("spires: no site for {} near {:.0},{:.0}", structures.join("/"), c.x, c.y));
            return "none placed".into();
        }
        let mid = done.iter().fold(Vec2::ZERO, |a, p| a + *p) / done.len() as f32;
        cx.pois.push(Poi { name: poi.clone().unwrap_or(structures[0].clone()), pos: mid, cause });
        format!("{} of {} round {:.0},{:.0}", done.len(), structures.join("/"), mid.x, mid.y)
    }
}
