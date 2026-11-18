use super::river::{carve, RiverRec, Rivers};
use crate::world::chronicle::geom::{densify, each_near};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use glam::Vec2;
use serde::Deserialize;

/// The river breaks out at `breach` and runs a new course; the old bed below
/// the breach dries and half fills.
#[derive(Deserialize, Debug)]
pub struct Avulsion {
    river: String,
    breach: Pos,
    path: Vec<Pos>,
    old_bed: String,
    #[serde(default)]
    silt: Option<String>,
    #[serde(default)]
    silt_thick: f32,
}

impl Process for Avulsion {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Avulsion { river, breach, path, old_bed, silt, silt_thick } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let Some(rec) = cx.state.get::<Rivers>().0.get(river).cloned() else {
            cx.warnings.push(format!("avulsion: unknown river {river}"));
            return "skipped".into();
        };
        let line = rec.line.clone();
        let b = cx.pos(breach);
        let k = (0..line.len()).min_by(|&a, &c| line[a].distance(b).partial_cmp(&line[c].distance(b)).unwrap()).unwrap_or(0);
        let old_m = cx.mat(old_bed);
        // The old course below the breach dries and half fills.
        let old: Vec<Vec2> = line[k..].to_vec();
        for p in &old {
            each_near(&mut cx.f, *p, 6.0, |f: &mut Fields, i, d, _| {
                if f.water[i] == 1 {
                    f.water[i] = 2;
                    f.surface[i] = old_m;
                    f.h[i] += 0.9 * (1.0 - d / 6.0);
                    f.cause[i] = cause;
                }
            });
        }
        let mut pts = vec![line[k]];
        pts.extend(path.iter().map(|p| cx.pos(p)));
        let new_line = densify(&pts, 5.0, seed);
        carve(cx, &new_line, rec.width, rec.depth, rec.bed, rec.floor);
        if let (Some(s), true) = (silt, *silt_thick > 0.0) {
            let m = cx.mat(s);
            each_near(&mut cx.f, b, 90.0, |f: &mut Fields, i, d, _| {
                let t = silt_thick * (1.0 - d / 90.0);
                if f.h[i] > f.sea && t > 0.1 {
                    f.add_layer(i, m, 0.0, t, true);
                }
            });
        }
        let mut full = line[..=k].to_vec();
        full.extend(new_line.iter().skip(1));
        cx.state.get::<Rivers>().0.insert(river.clone(), RiverRec { line: full, ..rec });
        cx.pois.push(Poi { name: format!("{river} breach"), pos: b, cause });
        format!("{river} breaks out at {:.0},{:.0}; {} m of old bed dries", b.x, b.y, old.len() * 2)
    }
}
