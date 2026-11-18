use crate::world::chronicle::{EventCtx, Process};
use crate::world::noise::{fbm2, perlin2, smoothstep};
use serde::Deserialize;
use std::collections::VecDeque;

/// Weathers the tops of every cliff in the world: the lip is rounded into a
/// shoulder up to `depth` m deep that eases back `reach` m from the edge, and
/// here and there a gully is cut back into the top (`gullies`, 0 to 1). The
/// rounding varies along the edge, so no two stretches of rim match. Run it
/// after the landforms and before anything is built on them.
#[derive(Deserialize, Debug)]
pub struct ErodeRims {
    #[serde(default = "three")]
    depth: f32,
    #[serde(default = "ten")]
    reach: f32,
    #[serde(default = "gul")]
    gullies: f32,
    /// Only cliffs at least this high (m) count.
    #[serde(default = "four")]
    min_drop: f32,
    /// 0 (the default): gullies are broad, smooth scallops. 1: they are
    /// narrow notches along joints in a hard caprock, which keeps its flat
    /// edge between them (Redrock's mesas).
    #[serde(default)]
    joints: f32,
}

fn three() -> f32 {
    3.0
}
fn ten() -> f32 {
    10.0
}
fn gul() -> f32 {
    0.4
}
fn four() -> f32 {
    4.0
}

impl Process for ErodeRims {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let f = &mut cx.f;
        let n = f.n;
        let h0 = f.h.clone();
        // Distance (cells) back from the nearest lip, over the top only, and
        // the drop at that lip.
        let reach_cells = (self.reach * 1.8).ceil() as i32;
        let mut dist = vec![i32::MAX; n * n];
        let mut lip_h = vec![0.0f32; n * n];
        let mut drop_at = vec![0.0f32; n * n];
        let mut q = VecDeque::new();
        for z in 1..n - 1 {
            for x in 1..n - 1 {
                let i = z * n + x;
                if f.occupied[i] || h0[i] < f.sea {
                    continue;
                }
                // The steepest fall within 3 cells.
                let mut drop: f32 = 0.0;
                for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    for k in 1..=3 {
                        let (xx, zz) = (x as i32 + dx * k, z as i32 + dz * k);
                        if xx < 0 || zz < 0 || xx >= n as i32 || zz >= n as i32 {
                            break;
                        }
                        drop = drop.max(h0[i] - h0[zz as usize * n + xx as usize]);
                    }
                }
                if drop >= self.min_drop {
                    dist[i] = 0;
                    lip_h[i] = h0[i];
                    drop_at[i] = drop;
                    q.push_back(i);
                }
            }
        }
        while let Some(i) = q.pop_front() {
            let d = dist[i];
            if d >= reach_cells {
                continue;
            }
            let (x, z) = ((i % n) as i32, (i / n) as i32);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (xx, zz) = (x + dx, z + dz);
                if xx < 0 || zz < 0 || xx >= n as i32 || zz >= n as i32 {
                    continue;
                }
                let j = zz as usize * n + xx as usize;
                // Stay on the top: no further down than a metre below the lip.
                if dist[j] <= d + 1 || h0[j] < lip_h[i] - 1.0 || f.occupied[j] {
                    continue;
                }
                dist[j] = d + 1;
                lip_h[j] = lip_h[i];
                drop_at[j] = drop_at[i];
                q.push_back(j);
            }
        }
        let mut cells = 0;
        let mut deepest: f32 = 0.0;
        for i in 0..n * n {
            if dist[i] == i32::MAX {
                continue;
            }
            let p = f.centre(i);
            let d = dist[i] as f32;
            // How hard this stretch of rim has weathered.
            let k = (0.55 + 0.75 * (fbm2(p.x * 0.03, p.y * 0.03, 3, seed) * 0.5 + 0.5)).clamp(0.2, 1.4);
            // A hard caprock keeps a sharper lip: the rounding stays near the edge.
            let shoulder = (1.0 - d / self.reach).max(0.0).powf(1.6 + 1.4 * self.joints);
            let g = if self.joints > 0.0 {
                // Joints: a warped, finer noise thresholded hard, so the cuts
                // are narrow, uneven in spacing and steep-sided.
                let wx = p.x + perlin2(p.x * 0.02, p.y * 0.02, seed + 10) * 25.0;
                let wz = p.y + perlin2(p.x * 0.02, p.y * 0.02, seed + 11) * 25.0;
                let j = perlin2(wx * 0.13, wz * 0.13, seed + 9) * 0.5 + 0.5;
                let sparse = smoothstep(0.35, 0.6, perlin2(p.x * 0.015, p.y * 0.015, seed + 12) * 0.5 + 0.5);
                let notch = smoothstep(0.66, 0.74, j) * sparse;
                (notch * self.joints + smoothstep(0.35, 0.75, perlin2(p.x * 0.06, p.y * 0.06, seed + 9) * 0.5 + 0.5) * (1.0 - self.joints)) * self.gullies
            } else {
                smoothstep(0.35, 0.75, perlin2(p.x * 0.06, p.y * 0.06, seed + 9) * 0.5 + 0.5) * self.gullies
            };
            let gully = (1.0 - d / (self.reach * 1.8)).max(0.0) * g * 2.2;
            let cut = (self.depth * k * shoulder + self.depth * gully + perlin2(p.x * 0.4, p.y * 0.4, seed + 3) * 0.3 * shoulder)
                .min(drop_at[i] * 0.6)
                .max(0.0);
            if cut > 0.05 {
                f.h[i] -= cut;
                f.cause[i] = cause;
                cells += 1;
                deepest = deepest.max(cut);
            }
        }
        format!("rims weathered over {cells} m2, up to {deepest:.1} m down")
    }
}
