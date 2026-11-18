use crate::world::chronicle::geom::densify;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos, Tube};
use super::lava_tube::Tubes;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A level tunnel through rock at `height` m above sea along `path`: a cave
/// passage through a tower or a ridge, open wherever the path leaves the rock.
/// `lining` names a material for the rock over its roof (a glowing ceiling);
/// `shafts` vertical chimneys up to the surface, evenly along it.
#[derive(Deserialize, Debug)]
pub struct Passage {
    path: Vec<Pos>,
    height: f32,
    radius: f32,
    #[serde(default)]
    lining: Option<String>,
    #[serde(default)]
    shafts: u32,
    /// Material for the rock all round the passage, floor and walls (a dark
    /// wet stone); the lining still covers the top of the roof.
    #[serde(default)]
    wall: Option<String>,
    /// How much the path wanders, metres (0: straight between the points).
    #[serde(default)]
    wander: f32,
    #[serde(default)]
    poi: Option<String>,
}

impl Process for Passage {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let lining = self.lining.as_ref().map_or(0, |m| cx.mat(m));
        let wall = self.wall.as_ref().map_or(0, |m| cx.mat(m));
        let line = if self.wander > 0.0 { densify(&pts, self.wander, seed) } else { pts.clone() };
        let y = cx.f.sea + self.height;
        let n = line.len();
        let mut inside = 0.0;
        let mut total = 0.0;
        for w in line.windows(2) {
            let d = w[0].distance(w[1]);
            total += d;
            let steps = (d / 2.0).ceil().max(1.0) as usize;
            for s in 0..steps {
                let p = w[0].lerp(w[1], (s as f32 + 0.5) / steps as f32);
                if cx.f.height(p.x, p.y) > y + self.radius {
                    inside += d / steps as f32;
                }
            }
        }
        let mut shafts = Vec::new();
        for s in 0..self.shafts {
            let k = ((s as f32 + 1.0) / (self.shafts as f32 + 1.0) * n as f32) as usize;
            let p = line[k.min(n - 1)];
            let top = cx.f.height(p.x, p.y) + 1.0;
            shafts.push(Tube { a: Vec3::new(p.x, y, p.y), b: Vec3::new(p.x, top, p.y), r: self.radius * 0.6, lining: 0, wall: 0 });
        }
        // Lined only under a real roof: where the rock over the tunnel is
        // thinner than the lining, it would show on the open ground.
        let fine = densify(&line, 0.0, seed);
        let fine: Vec<Vec2> = fine
            .windows(2)
            .flat_map(|w| {
                let steps = (w[0].distance(w[1]) / 2.0).ceil().max(1.0) as usize;
                (0..steps).map(move |s| w[0].lerp(w[1], s as f32 / steps as f32))
            })
            .chain(std::iter::once(line[n - 1]))
            .collect();
        let mut segs = Vec::new();
        for w in fine.windows(2) {
            // Rock over it and to both sides, or one wall of the tunnel is open.
            let d = (w[1] - w[0]).normalize_or_zero();
            let side = Vec2::new(-d.y, d.x) * (self.radius + 2.0);
            let mid = (w[0] + w[1]) * 0.5;
            let roof = [w[0], w[1], mid, mid + side, mid - side].iter().map(|p| cx.f.height(p.x, p.y)).fold(f32::MAX, f32::min);
            let lined = if roof > y + self.radius * 1.2 + 2.5 { lining } else { 0 };
            segs.push(Tube { a: Vec3::new(w[0].x, y, w[0].y), b: Vec3::new(w[1].x, y, w[1].y), r: self.radius, lining: lined, wall });
        }
        let tubes = &mut cx.state.get::<Tubes>().0;
        tubes.extend(segs);
        tubes.extend(shafts);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: line[n / 2], cause });
        }
        format!("passage {total:.0} m long at {:.0} m above sea, {inside:.0} m of it in rock, {} shafts", self.height, self.shafts)
    }
}
