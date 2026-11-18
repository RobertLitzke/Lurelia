use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use serde::Deserialize;

/// A deposit (silt, scree) that raises the ground and buries what was there.
#[derive(Deserialize, Debug)]
pub struct Deposit {
    at: Pos,
    radius: f32,
    thickness: f32,
    mat: String,
    /// "dome" eases the layer out to nothing at the rim with no kink, for a
    /// knoll or a low hill; the default thins as 1 - (d/r)^2 and ends in a
    /// slight step at the rim (a fan of silt or scree).
    #[serde(default)]
    shape: Option<String>,
    /// The deepest water the layer may build up out of, metres: ground under
    /// more water than this is left alone. A hand's depth by default, which
    /// keeps a fan of silt or scree to the shore it came off; a bank built up
    /// out of a lagoon floor three metres down asks for its own figure.
    #[serde(default = "default_depth")]
    depth: f32,
}

fn default_depth() -> f32 {
    1.0
}

impl Process for Deposit {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Deposit { at, radius, thickness, mat, shape, depth } = self;
        let dome = shape.as_deref() == Some("dome");
        let cause = cx.cause;
        let c = cx.pos(at);
        let m = cx.mat(mat);
        each_near(&mut cx.f, c, *radius, |f: &mut Fields, i, d, _| {
            let k = (1.0 - (d / radius).powi(2)).max(0.0);
            let t = thickness * if dome { k * k } else { k };
            if t > 0.1 && f.h[i] > f.sea - depth {
                f.add_layer(i, m, 0.0, t, true);
                f.cause[i] = cause;
            }
        });
        format!("{thickness} m of {mat} at {:.0},{:.0}", c.x, c.y)
    }
}
