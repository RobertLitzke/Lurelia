use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::noise::{fbm2, ridged2, smoothstep};
use serde::Deserialize;

/// The island itself: a warped coast around `centre`, rolling hills inside.
#[derive(Deserialize, Debug)]
pub struct Landmass {
    centre: Pos,
    radius: f32,
    #[serde(default = "half")]
    roughness: f32,
    #[serde(default)]
    hills: f32,
    #[serde(default)]
    ridges: f32,
    /// Depth of the sea floor off the coast, metres (a shallow shelf: 3).
    #[serde(default = "deep")]
    shelf: f32,
    /// Over how many metres from the world's edge a shallow shelf falls away
    /// to the deep sea (default 140).
    #[serde(default = "fall")]
    edge_fall: f32,
}

fn fall() -> f32 {
    140.0
}

fn deep() -> f32 {
    14.0
}

fn half() -> f32 {
    0.5
}

impl Process for Landmass {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Landmass { centre, radius, roughness, hills, ridges, shelf, edge_fall } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(centre);
        let noise_at = cx.noise_at;
        let f = &mut cx.f;
        let sea = f.sea;
        for i in 0..f.n * f.n {
            let at = f.centre(i);
            let d = (at - c) / *radius;
            let p = at + noise_at;
            let warp = fbm2(p.x * 0.0032, p.y * 0.0032, 4, seed) * 0.38 * roughness + fbm2(p.x * 0.015, p.y * 0.015, 2, seed + 1) * 0.05;
            let r = d.length() + warp;
            let land = smoothstep(1.0, 0.72, r);
            let wx = p.x + 60.0 * fbm2(p.x * 0.003, p.y * 0.003, 3, seed + 2);
            let wz = p.y + 60.0 * fbm2(p.x * 0.003 + 50.0, p.y * 0.003, 3, seed + 3);
            let hv = (fbm2(wx * 0.005, wz * 0.005, 5, seed + 4) * 0.5 + 0.5).clamp(0.0, 1.0).powf(1.3) * hills;
            let rv = ridged2(wx * 0.007, wz * 0.007, 4, seed + 5) * ridges * smoothstep(0.4, 0.7, fbm2(p.x * 0.003, p.y * 0.003 - 9.0, 2, seed + 6) * 0.5 + 0.5);
            let inland = smoothstep(1.0, 0.45, r);
            // A shallow shelf still falls away to the deep sea before the
            // edge of the world, or the edge shows as a straight drop-off.
            let edge = at.x.min(at.y).min(f.size - at.x).min(f.size - at.y);
            let shelf = if *shelf < 14.0 { 14.0 + (shelf - 14.0) * smoothstep(20.0f32.min(edge_fall * 0.5), edge_fall.max(10.0), edge) } else { *shelf };
            f.h[i] = sea - shelf + land * (shelf + 1.2 + (hv + rv) * inland + fbm2(p.x * 0.04, p.y * 0.04, 2, seed + 7) * 0.6);
            f.moist[i] = fbm2(p.x * 0.004 + 300.0, p.y * 0.004, 3, seed + 8) * 0.5 + 0.5;
            f.cause[i] = cause;
        }
        format!("island r {radius} m around {:.0},{:.0}", c.x, c.y)
    }
}
