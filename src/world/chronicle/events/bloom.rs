use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use serde::Deserialize;

/// Extra strangeness around a point.
#[derive(Deserialize, Debug)]
pub struct Bloom {
    at: Pos,
    radius: f32,
    #[serde(default = "one")]
    strength: f32,
}

fn one() -> f32 {
    1.0
}

impl Process for Bloom {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Bloom { at, radius, strength } = self;
        let c = cx.pos(at);
        each_near(&mut cx.f, c, radius * 2.5, |f: &mut Fields, i, d, _| {
            f.strange[i] = (f.strange[i] + strength * (-d / radius).exp()).min(1.0);
        });
        format!("strangeness around {:.0},{:.0}", c.x, c.y)
    }
}
