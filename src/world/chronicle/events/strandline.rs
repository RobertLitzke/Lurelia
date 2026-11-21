use crate::world::chronicle::{EventCtx, Process};
use crate::world::noise::{fbm2, smoothstep};
use serde::Deserialize;

/// The shore of a lake or sea that stood at `level` for a long time and is
/// gone: waves cut a bench into every slope at that height, with a low cliff
/// behind it, and the bench is still there, following the land's contour
/// round every hill and up every valley. It is the same height everywhere,
/// which is what makes it read as a shoreline.
///
/// Ground within `band` m of the level is drawn toward it: nearly flat at the
/// level itself, steepening to the old slope and past it at the band's upper
/// edge. `strength` (0 to 1) is how completely; it varies along the shore, so
/// the bench is broad on some headlands and gone on others. With several
/// `levels` the shore stood at each in turn.
#[derive(Deserialize, Debug)]
pub struct Strandline {
    /// Absolute heights (m) the water stood at.
    levels: Vec<f32>,
    #[serde(default = "default_band")]
    band: f32,
    #[serde(default = "default_strength")]
    strength: f32,
}

fn default_band() -> f32 {
    4.0
}
fn default_strength() -> f32 {
    0.8
}

impl Process for Strandline {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let band = self.band.max(0.5);
        let noise_at = cx.noise_at;
        let f = &mut cx.f;
        let mut cut = 0usize;
        for i in 0..f.n * f.n {
            let h = f.h[i];
            if h < f.sea {
                continue;
            }
            for (k, level) in self.levels.iter().enumerate() {
                let t = (h - level) / band;
                if t.abs() >= 1.0 {
                    continue;
                }
                let p = f.centre(i) + noise_at;
                // Broad here, faint there.
                let along = smoothstep(-0.35, 0.25, fbm2(p.x * 0.006, p.y * 0.006, 3, seed + k as u32));
                // Below the level a flat bench; above it the cliff the waves
                // cut back, which gives the ground it took back at its top.
                let shaped = if t < 0.0 { -(-t).powf(2.6) } else { t.powf(0.45) * t.abs().max(0.15).min(1.0).powf(0.2) };
                let to = level + shaped.clamp(-1.0, 1.0) * band;
                let w = self.strength.clamp(0.0, 1.0) * along * smoothstep(1.0, 0.8, t.abs());
                let nh = h + (to - h) * w;
                if (nh - h).abs() > 0.05 {
                    f.h[i] = nh;
                    f.cause[i] = cause;
                    cut += 1;
                }
                break;
            }
        }
        format!("{} old shorelines, {cut} m2 benched", self.levels.len())
    }
}
