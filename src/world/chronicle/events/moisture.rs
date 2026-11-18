use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use serde::Deserialize;

/// Wetter (or, with a negative `amount`, drier) ground round a point.
#[derive(Deserialize, Debug)]
pub struct Moisture {
    at: Pos,
    radius: f32,
    amount: f32,
}

impl Process for Moisture {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Moisture { at, radius, amount } = self;
        let c = cx.pos(at);
        each_near(&mut cx.f, c, *radius, |f: &mut Fields, i, d, _| f.moist[i] = (f.moist[i] + amount * (1.0 - d / radius)).clamp(0.0, 1.0));
        "moisture".into()
    }
}
