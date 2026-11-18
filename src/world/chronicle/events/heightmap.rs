use crate::world::chronicle::{EventCtx, Process};
use serde::Deserialize;

/// The ground from a height grid file in the world folder: f32
/// little-endian, one value per 1 m cell, rows north to south, in world metres.
#[derive(Deserialize, Debug)]
pub struct Heightmap {
    file: String,
}

impl Process for Heightmap {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let file = &self.file;
        let cause = cx.cause;
        let bytes = match std::fs::read(cx.dir.join(file)) {
            Ok(b) => b,
            Err(e) => {
                cx.warnings.push(format!("heightmap {file}: {e}"));
                return "heightmap missing".into();
            }
        };
        let f = &mut cx.f;
        let nn = f.n * f.n;
        if bytes.len() != nn * 4 {
            cx.warnings.push(format!("heightmap {file}: {} bytes, expected {} for {}x{}", bytes.len(), nn * 4, f.n, f.n));
            return "heightmap wrong size".into();
        }
        for i in 0..nn {
            f.h[i] = f32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);
            f.cause[i] = cause;
        }
        let (lo, hi) = f.h.iter().fold((f32::MAX, f32::MIN), |a, &h| (a.0.min(h), a.1.max(h)));
        format!("ground from {file}, {lo:.1}..{hi:.1} m")
    }
}
