use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{densify, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::smoothstep;
use glam::Vec2;
use serde::Deserialize;

/// A bayou: a slow channel of sea water `depth` m deep through low ground,
/// joining one stretch of open water to another (a crevasse, a slough, a
/// cut). Unlike a `river` it has no source and no current, and unlike a
/// `vale` its floor is set under the sea, so it floods end to end. Ground
/// already lower stays as it is.
#[derive(Deserialize, Debug)]
pub struct Bayou {
    path: Vec<Pos>,
    /// Width of the channel's flat floor, metres.
    width: f32,
    /// Water depth over the floor, metres.
    depth: f32,
    /// Run of each bank, metres, from the floor's edge up to the old ground.
    #[serde(default = "bank")]
    bank: f32,
    #[serde(default = "meander")]
    meander: f32,
}

fn bank() -> f32 {
    5.0
}
fn meander() -> f32 {
    8.0
}

impl Process for Bayou {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let line = densify(&pts, self.meander, cx.seed);
        if line.len() < 2 {
            return "skipped".into();
        }
        let cause = cx.cause;
        let f = &mut cx.f;
        let half = self.width.max(1.0) * 0.5;
        let reach = half + self.bank.max(0.5);
        // Each cell's distance to the line.
        let mut near: HashMap<usize, f32> = HashMap::default();
        for p in &line {
            each_near(f, *p, reach + 1.0, |_f: &mut Fields, i, d, _| {
                let e = near.entry(i).or_insert(f32::MAX);
                *e = e.min(d);
            });
        }
        let floor = f.sea - self.depth.max(0.2);
        let mut cut = 0;
        for (i, d) in near {
            let k = smoothstep(reach, half, d);
            let h = f.h[i] + (floor - f.h[i]) * k;
            if h < f.h[i] - 0.01 {
                f.h[i] = h;
                f.cause[i] = cause;
                f.moist[i] = f.moist[i].max(0.8);
                cut += 1;
            }
        }
        let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
        format!("bayou {len:.0} m long, {:.1} m deep, {cut} m² cut", self.depth)
    }
}
