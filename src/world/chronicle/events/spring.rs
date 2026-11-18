use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos, WorldVent};
use crate::world::noise::{hash2, perlin2, smoothstep, unit};
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A hot spring or a field of fumaroles: ground the heat keeps bare of snow,
/// crusted with sinter in low stepped terraces, and steam rising all the
/// time. Put a `lake` at the same place first for a pool to steam off; with
/// none it is dry vents breathing out of the ground.
///
/// `vents` steam vents are dealt across `radius` m round `at`, each breathing
/// `steam` puffs a second of the `fx` effect (default `steam`). The bare,
/// sinter-crusted ground reaches `ring` m beyond.
#[derive(Deserialize, Debug)]
pub struct Spring {
    at: Pos,
    radius: f32,
    #[serde(default = "default_ring")]
    ring: f32,
    /// The crust round it.
    sinter: String,
    /// Patches of this in the crust (the orange and green of the algae mats).
    #[serde(default)]
    stain: Option<String>,
    #[serde(default = "default_vents")]
    vents: u32,
    #[serde(default = "default_steam")]
    steam: f32,
    #[serde(default)]
    fx: Option<String>,
}

fn default_ring() -> f32 {
    10.0
}
fn default_vents() -> u32 {
    5
}
fn default_steam() -> f32 {
    6.0
}

impl Process for Spring {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let sinter = cx.mat(&self.sinter);
        let stain = self.stain.as_ref().map(|m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let (r, ring) = (self.radius.max(1.0), self.ring.max(0.0));
        let mut low = f32::MAX;
        let mut crusted = 0;
        each_near(&mut cx.f, c, r + ring, |f: &mut Fields, i, d, _| {
            let p = f.centre(i);
            // A ragged edge to the bare ground, the snow creeping in on the
            // cold side of each lobe.
            let edge = r + ring * (0.7 + 0.5 * perlin2(p.x * 0.15, p.y * 0.15, seed));
            if d > edge || f.cliff[i] > 0.5 {
                return;
            }
            low = low.min(f.h[i]);
            if f.water[i] != 0 {
                return;
            }
            // Low terraces of sinter, stepping down away from the pool.
            let k = 1.0 - smoothstep(r * 0.6, edge, d);
            let terrace = ((k * 3.0).floor() / 3.0) * 0.5;
            f.h[i] += terrace;
            let stained = stain.is_some() && perlin2(p.x * 0.22, p.y * 0.22, seed + 1) > 0.25 && d < r + ring * 0.6;
            f.surface[i] = if stained { stain.unwrap() } else { sinter };
            f.cause[i] = cause;
            crusted += 1;
        });
        let fx = self.fx.clone().unwrap_or_else(|| "steam".to_string());
        let y = if low < f32::MAX { low } else { cx.f.height(c.x, c.y) };
        for k in 0..self.vents {
            // The first vent at the heart, the rest scattered across it.
            let (a, s) = (unit(hash2(k as i32, 0, seed + 2)) * std::f32::consts::TAU, if k == 0 { 0.0 } else { r * unit(hash2(k as i32, 1, seed + 3)).sqrt() });
            let p = c + Vec2::new(a.cos(), a.sin()) * s;
            cx.vents.push(WorldVent {
                pos: Vec3::new(p.x, y + 0.2, p.y),
                fx: fx.clone(),
                rate: self.steam * if k == 0 { 1.6 } else { 0.6 + 0.6 * unit(hash2(k as i32, 2, seed + 4)) },
                spread: (r * 0.35).max(0.8),
            });
        }
        format!("spring r {r:.0} m at {:.0},{:.0}: {crusted} m2 of sinter, {} vents", c.x, c.y, self.vents)
    }
}
