use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A long rock face: the ground on the right of the line from `from` to `to`
/// (facing along it) stands `height` m above the left, with a cliff between.
///
/// Without `reach` only cells within a circle round the line's middle
/// (half its length + 4 widths + 40 m) change, so on the raised side the
/// rise ends in a circular step. With `reach` the whole raised side within
/// that many metres of the line rises, fading out over 40 m beyond it: use
/// it for long cliff lines that cross a world.
#[derive(Deserialize, Debug)]
pub struct Escarpment {
    from: Pos,
    to: Pos,
    height: f32,
    #[serde(default = "default_width")]
    width: f32,
    #[serde(default)]
    reach: Option<f32>,
}

fn default_width() -> f32 {
    10.0
}

impl Process for Escarpment {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Escarpment { from, to, height, width, reach } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let (a, b) = (cx.pos(from), cx.pos(to));
        let f = &mut cx.f;
        let len = a.distance(b).max(1.0);
        let d = (b - a) / len;
        let right = Vec2::new(-d.y, d.x);
        let mid = (a + b) * 0.5;
        let r = match reach {
            Some(_) => f.size * 2.0,
            None => len * 0.5 + width * 4.0 + 40.0,
        };
        each_near(f, mid, r, |f: &mut Fields, i, _, p| {
            let rel = p - a;
            let along = rel.dot(d);
            let fade = match reach {
                Some(k) => smoothstep(k + 40.0, *k, rel.dot(right)),
                None => 1.0,
            };
            if fade <= 0.0 {
                return;
            }
            // Fade out past the ends, and let the face wander a little.
            let fade = fade * smoothstep(-40.0, 0.0, along) * smoothstep(len + 40.0, len, along);
            if fade <= 0.0 || f.h[i] < f.sea - 1.0 {
                return;
            }
            let wob = perlin2(along * 0.02, 5.0, seed) * width * 1.5;
            let s = rel.dot(right) - wob;
            let t = smoothstep(-width * 0.5, width * 0.5, s);
            let step = smoothstep(0.35, 0.65, t);
            f.h[i] += height * fade * (step * 0.85 + t * 0.15);
            if (s.abs() < width * 0.45) && fade > 0.3 {
                f.cliff[i] = f.cliff[i].max(fade);
            }
            f.cause[i] = cause;
        });
        format!("escarpment {height} m high, {len:.0} m long")
    }
}
