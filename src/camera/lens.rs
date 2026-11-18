//! The camera's optics, kept simple: a frame shape, a focal length, an
//! aperture and a focus distance. Blur is the thin-lens circle of confusion
//! on a 24 mm tall sensor, exaggerated so depth of field reads at game
//! distances. The post shader (post.wgsl `defocus`) and the photo checks
//! (tasks.rs) use the same numbers, so what looks sharp counts as sharp.

use serde::{Deserialize, Serialize};

/// The frame's shape. Right click cycles through them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orient {
    #[default]
    Landscape,
    Portrait,
    Square,
}

impl Orient {
    pub fn next(self) -> Self {
        match self {
            Orient::Landscape => Orient::Portrait,
            Orient::Portrait => Orient::Square,
            Orient::Square => Orient::Landscape,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Orient::Landscape => "LANDSCAPE",
            Orient::Portrait => "PORTRAIT",
            Orient::Square => "SQUARE",
        }
    }

    /// Width over height.
    pub fn aspect(self) -> f32 {
        match self {
            Orient::Landscape => 1.5,
            Orient::Portrait => 2.0 / 3.0,
            Orient::Square => 1.0,
        }
    }
}

/// The viewfinder on a screen of `aspect` (width over height), in uv:
/// (x0, y0, x1, y1), centred, as large as fits in 86% of the width and 82%
/// of the height.
pub fn frame_uv(orient: Orient, aspect: f32) -> [f32; 4] {
    let a = orient.aspect();
    // In units of the screen height.
    let mut fh = 0.82;
    let mut fw = fh * a;
    if fw > 0.86 * aspect {
        fw = 0.86 * aspect;
        fh = fw / a;
    }
    let (hw, hh) = (fw / aspect * 0.5, fh * 0.5);
    [0.5 - hw, 0.5 - hh, 0.5 + hw, 0.5 + hh]
}

/// The viewfinder in whole pixels: (x, y, w, h).
pub fn frame_px(orient: Orient, w: u32, h: u32) -> [u32; 4] {
    let f = frame_uv(orient, w as f32 / h.max(1) as f32);
    let x0 = (f[0] * w as f32).round() as u32;
    let y0 = (f[1] * h as f32).round() as u32;
    let x1 = ((f[2] * w as f32).round() as u32).min(w);
    let y1 = ((f[3] * h as f32).round() as u32).min(h);
    [x0, y0, x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1)]
}

pub const APERTURES: [f32; 7] = [1.8, 2.8, 4.0, 5.6, 8.0, 11.0, 16.0];
pub const FOCAL_MIN: f32 = 24.0;
pub const FOCAL_MAX: f32 = 200.0;
pub const FOCUS_MIN: f32 = 0.6;
pub const FOCUS_MAX: f32 = 400.0;
/// Pixels per millimetre of sensor at 1080 pixels tall.
const PX_PER_MM: f32 = 1080.0 / 24.0;
/// Real depth of field is deep at these distances; this makes it visible.
const EXAGGERATE: f32 = 4.0;
/// The most blur, in pixels at 1080 tall.
pub const BLUR_MAX: f32 = 14.0;
/// Blur outside the viewfinder, in pixels at 1080 tall.
pub const MASK_BLUR: f32 = 6.0;
/// At most this much blur (pixels at 1080 tall) still counts as sharp.
pub const SHARP: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Lens {
    pub focal_mm: f32,
    /// Index into APERTURES.
    pub stop: usize,
    pub focus_m: f32,
}

impl Default for Lens {
    fn default() -> Self {
        Lens {
            focal_mm: 50.0,
            stop: 3,
            focus_m: 10.0,
        }
    }
}

impl Lens {
    pub fn f_number(&self) -> f32 {
        APERTURES[self.stop.min(APERTURES.len() - 1)]
    }

    /// Vertical field of view, radians (a 24 mm tall sensor).
    pub fn fov(&self) -> f32 {
        2.0 * (12.0 / self.focal_mm).atan()
    }

    /// Blur of a point at infinity, pixels at 1080 tall. Blur at distance d
    /// is this times |1 - focus / d|.
    pub fn blur_inf(&self) -> f32 {
        let f = self.focal_mm;
        let s = (self.focus_m * 1000.0).max(f * 1.5);
        EXAGGERATE * PX_PER_MM * f * f / (self.f_number() * (s - f))
    }

    /// Blur of a point `d` metres away, pixels at 1080 tall.
    pub fn blur(&self, d: f32) -> f32 {
        (self.blur_inf() * (1.0 - self.focus_m / d.max(0.01)).abs()).min(BLUR_MAX)
    }

    pub fn sharp(&self, d: f32) -> bool {
        self.blur(d) <= SHARP
    }

    /// Autofocus: rack toward `target` metres, evenly in log distance; it
    /// settles in a fraction of a second.
    pub fn focus_toward(&mut self, target: f32, dt: f32) {
        let t = target.clamp(FOCUS_MIN, FOCUS_MAX).ln();
        let k = 1.0 - (-dt * 12.0).exp();
        let now = self.focus_m.ln();
        self.focus_m = (now + (t - now) * k).exp().clamp(FOCUS_MIN, FOCUS_MAX);
    }

    /// Manual focus held for `dt` seconds: `dir` +1 further, -1 nearer, at
    /// three times the distance per second.
    pub fn focus_hold(&mut self, dir: f32, dt: f32) {
        self.focus_m = (self.focus_m * 3f32.powf(dir * dt)).clamp(FOCUS_MIN, FOCUS_MAX);
    }

    /// Wheel notches of manual focus: positive further off, a sixth of a
    /// stop of distance each, so a full turn covers a lot of ground.
    pub fn focus_by(&mut self, steps: i32) {
        self.focus_m = (self.focus_m * 1.18f32.powi(steps)).clamp(FOCUS_MIN, FOCUS_MAX);
    }

    /// Steps: positive closes the aperture (more in focus).
    pub fn stop_by(&mut self, steps: i32) {
        self.stop = (self.stop as i32 + steps).clamp(0, APERTURES.len() as i32 - 1) as usize;
    }

    /// Steps: positive zooms in.
    pub fn zoom_by(&mut self, steps: i32) {
        self.focal_mm = (self.focal_mm * 1.15f32.powi(steps)).clamp(FOCAL_MIN, FOCAL_MAX).round();
    }

    /// The post pass's lens for a screen of `aspect`; `mask` greys and blurs
    /// outside the frame (the viewfinder; a photo has none).
    pub fn fx(&self, orient: Orient, aspect: f32, mask: bool) -> crate::render::LensFx {
        crate::render::LensFx {
            frame: frame_uv(orient, aspect),
            focus_m: self.focus_m,
            blur_inf: self.blur_inf(),
            blur_max: BLUR_MAX,
            mask: if mask { MASK_BLUR } else { 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame keeps its shape and fits the screen.
    #[test]
    fn frames_fit_and_keep_their_shape() {
        for o in [Orient::Landscape, Orient::Portrait, Orient::Square] {
            for (w, h) in [(1600u32, 900u32), (900, 1600), (1024, 768)] {
                let [x, y, fw, fh] = frame_px(o, w, h);
                assert!(x + fw <= w && y + fh <= h, "{o:?} at {w}x{h}");
                let a = fw as f32 / fh as f32;
                assert!((a - o.aspect()).abs() < 0.02, "{o:?} at {w}x{h}: aspect {a}");
            }
        }
        assert_eq!(Orient::Landscape.next().next().next(), Orient::Landscape);
    }

    /// The focus distance is sharp; wide apertures and long lenses blur more.
    #[test]
    fn depth_of_field_behaves() {
        let l = Lens {
            focal_mm: 50.0,
            stop: 1,
            focus_m: 10.0,
        };
        assert!(l.blur(10.0) < 0.01);
        assert!(l.sharp(10.3));
        assert!(!l.sharp(40.0), "f/2.8 at 50 mm should blur 40 m: {}", l.blur(40.0));
        let closed = Lens { stop: 6, ..l };
        assert!(closed.sharp(40.0) || closed.blur(40.0) < l.blur(40.0));
        let long = Lens { focal_mm: 135.0, ..l };
        assert!(long.blur(20.0) > l.blur(20.0));
        // Near blur grows faster than far blur.
        assert!(l.blur(5.0) > l.blur(20.0));
        assert!(l.blur(0.1) <= BLUR_MAX);
    }

    /// Autofocus settles on the target within half a second, from either side.
    #[test]
    fn autofocus_settles() {
        for (from, to) in [(10.0, 80.0), (200.0, 2.0)] {
            let mut l = Lens { focus_m: from, ..Lens::default() };
            for _ in 0..30 {
                l.focus_toward(to, 1.0 / 60.0);
            }
            assert!(l.sharp(to), "{from} -> {to}: focus {}", l.focus_m);
        }
    }
}
