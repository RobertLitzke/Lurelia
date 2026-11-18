use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use serde::Deserialize;

/// A buried band (ash, char) laid across the land, thinning with distance.
#[derive(Deserialize, Debug)]
pub struct Ashfall {
    from: Pos,
    thickness: f32,
    reach: f32,
    depth: f32,
    mat: String,
}

impl Process for Ashfall {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Ashfall { from, thickness, reach, depth, mat } = self;
        let c = cx.pos(from);
        let m = cx.mat(mat);
        let f = &mut cx.f;
        let mut cells = 0;
        for i in 0..f.n * f.n {
            if f.h[i] < f.sea {
                continue;
            }
            let d = f.centre(i).distance(c);
            let t = thickness * (-d / reach).exp();
            if t >= 0.12 {
                f.add_layer(i, m, *depth, t.max(0.25), false);
                cells += 1;
            }
        }
        format!("{mat} band under {cells} m2")
    }
}
