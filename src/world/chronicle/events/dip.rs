use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// Tilted beds: the strata over a region, lifted by earth movement so they
/// run at a slant instead of dead level. Only the beds move (`Fields::lift`,
/// which the rock colours read); the ground stays where it is. Anything cut
/// after it that follows the beds (`canyon` benches with `irregular`, `trough`
/// ledges) steps along the slant too, so a cliff shows its bands and ledges
/// running down across the face.
///
/// The beds fall toward `bearing` by `grade` (drop per metre: 0.05 is about
/// 3 degrees), measured from `at`, and the tilt fades out over the last
/// third of `radius` so the region meets level country without a seam.
/// `warp` bends the plane in long gentle folds (metres of wander).
#[derive(Deserialize, Debug)]
pub struct Dip {
    at: Pos,
    radius: f32,
    bearing: f32,
    grade: f32,
    #[serde(default)]
    warp: f32,
}

impl Process for Dip {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let seed = cx.seed;
        let b = self.bearing.to_radians();
        // Toward the bearing (0 north, -z), the beds go down.
        let down = Vec2::new(b.sin(), -b.cos());
        let (r, grade, warp) = (self.radius.max(1.0), self.grade, self.warp);
        let mut most = 0.0f32;
        let mut cells = 0;
        each_near(&mut cx.f, c, r, |f: &mut Fields, i, d, _| {
            let p = f.centre(i);
            let fade = 1.0 - smoothstep(r * 0.66, r, d);
            let lift = (-(p - c).dot(down) * grade + warp * perlin2(p.x * 0.004, p.y * 0.004, seed)) * fade;
            f.lift_mut()[i] += lift;
            most = most.max(lift.abs());
            cells += 1;
        });
        format!("dip toward {:.0} at {:.3}, beds moved up to {most:.0} m over {cells} m2", self.bearing, grade)
    }
}
