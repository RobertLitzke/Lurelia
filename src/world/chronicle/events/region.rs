use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::perlin2;
use serde::Deserialize;

/// Paints a named region (a ragged disc) for biome rules.
#[derive(Deserialize, Debug)]
pub struct Region {
    name: String,
    at: Pos,
    radius: f32,
}

impl Process for Region {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Region { name, at, radius } = self;
        let seed = cx.seed;
        let c = cx.pos(at);
        let rid = cx.f.region_id(name);
        each_near(&mut cx.f, c, radius * 1.2, |f: &mut Fields, i, d, p| {
            if d < radius * (1.0 + 0.2 * perlin2(p.x * 0.05, p.y * 0.05, seed)) {
                f.region[i] = rid;
            }
        });
        format!("region {name}")
    }
}
