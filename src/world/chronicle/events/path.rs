use crate::world::chronicle::geom::{densify, stamp_path};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use serde::Deserialize;

/// A footpath: surface material on a gently wandering line, no plants.
#[derive(Deserialize, Debug)]
pub struct Path {
    from: Pos,
    to: Pos,
    width: f32,
    mat: String,
}

impl Process for Path {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Path { from, to, width, mat } = self;
        let (a, b) = (cx.pos(from), cx.pos(to));
        let m = cx.mat(mat);
        let line = densify(&[a, b], 4.0, cx.seed);
        stamp_path(&mut cx.f, &line, *width, m);
        format!("path {:.0} m", a.distance(b))
    }
}
