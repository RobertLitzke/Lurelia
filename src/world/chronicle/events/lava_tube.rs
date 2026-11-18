use crate::world::chronicle::geom::densify;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos, Tube};
use crate::world::noise::smoothstep;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A lava tube: a tunnel `depth` m under the ground along `path`, opening at
/// both ends and through `skylights` collapsed holes in its roof.
#[derive(Deserialize, Debug)]
pub struct LavaTube {
    path: Vec<Pos>,
    radius: f32,
    depth: f32,
    #[serde(default)]
    skylights: u32,
}

/// Every tube segment so far; the world keeps them to carve the voxels.
#[derive(Default)]
pub struct Tubes(pub Vec<Tube>);

impl Process for LavaTube {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let LavaTube { path, radius, depth, skylights } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let pts: Vec<Vec2> = path.iter().map(|p| cx.pos(p)).collect();
        let line = densify(&pts, 4.0, seed);
        let f = &cx.f;
        let tubes = &mut cx.state.get::<Tubes>().0;
        let n = line.len();
        let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut along = 0.0;
        let mut centres = Vec::with_capacity(n);
        for (k, p) in line.iter().enumerate() {
            if k > 0 {
                along += p.distance(line[k - 1]);
            }
            // Rise to the surface at both ends: those are the mouths.
            let ends = smoothstep(0.0, 18.0, along) * smoothstep(total, total - 18.0, along);
            let g = f.height(p.x, p.y);
            let y = g - (depth * ends).max(radius * 0.35);
            centres.push(Vec3::new(p.x, y, p.y));
        }
        for w in centres.windows(2) {
            tubes.push(Tube { a: w[0], b: w[1], r: *radius, lining: 0, wall: 0 });
        }
        // Skylights: shafts where the roof fell in.
        for s in 0..*skylights {
            let k = ((s as f32 + 1.0) / (*skylights as f32 + 1.0) * n as f32) as usize;
            let c = centres[k.min(n - 1)];
            let top = f.height(c.x, c.z) + 1.0;
            tubes.push(Tube { a: c, b: Vec3::new(c.x, top, c.z), r: radius * 0.75, lining: 0, wall: 0 });
            cx.pois.push(Poi { name: "skylight into a lava tube".into(), pos: Vec2::new(c.x, c.z), cause });
        }
        cx.pois.push(Poi { name: "lava tube mouth".into(), pos: line[0], cause });
        cx.pois.push(Poi { name: "lava tube mouth".into(), pos: line[n - 1], cause });
        format!("lava tube {total:.0} m long, {skylights} skylights")
    }
}
