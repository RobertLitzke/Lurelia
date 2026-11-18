//! The look's grade: two art-directed colour ramps (1D gradient strips) and
//! the knobs the haze and the grade read.
//!
//! One strip runs by distance and colours the haze, the low sky and the
//! shadowed light, so ridges recede through a designed sequence of hues. The
//! other two run by luminance over the finished frame: the tint multiplies,
//! so each pixel keeps its own hue (this is the old split tone), while the
//! gradient map replaces the hue with the strip's at the pixel's own
//! brightness, which is the flat colour the calm look is built on.
//!
//! Everything here is live. The LOOK page (menu.rs) edits it, the renderer
//! uploads the strip and the knobs each frame, and EXPORT writes it back into
//! `library/looks/<style>.json`.

use serde::{Deserialize, Serialize};

/// Samples along each strip.
pub const STRIP_N: u32 = 256;
/// Rows: the distance ramp, the luminance tint, the gradient map.
pub const STRIP_ROWS: u32 = 3;
pub const ROW_RAMP: usize = 0;
pub const ROW_TINT: usize = 1;
pub const ROW_MAP: usize = 2;

/// One stop on a strip: where it sits (0..1) and its linear colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub at: f32,
    pub color: [f32; 3],
}

impl Stop {
    pub fn new(at: f32, color: [f32; 3]) -> Stop {
        Stop { at, color }
    }
}

fn ramp_default() -> Vec<Stop> {
    vec![Stop::new(0.0, [0.98, 0.82, 0.56]), Stop::new(0.5, [0.90, 0.82, 0.74]), Stop::new(1.0, [0.58, 0.72, 0.86])]
}

/// The tint ramp that was the split tone: teal in the shadows, gold in the
/// highlights, with the old smoothstep(0.1, 0.8) shape kept as stops.
fn tint_default() -> Vec<Stop> {
    vec![
        Stop::new(0.0, [0.92, 1.0, 1.0]),
        Stop::new(0.10, [0.92, 1.0, 1.0]),
        Stop::new(0.3333, [0.9589, 1.0, 0.9637]),
        Stop::new(0.5667, [1.0311, 1.0, 0.8963]),
        Stop::new(0.80, [1.07, 1.0, 0.86]),
        Stop::new(1.0, [1.07, 1.0, 0.86]),
    ]
}

/// The gradient map: a designed sequence of hues from the shadows to the
/// highlights. Only its hue and chroma are used, since each sample is put
/// back at the pixel's own brightness, so the stops can be bold.
fn map_default() -> Vec<Stop> {
    vec![
        Stop::new(0.0, [0.07, 0.13, 0.18]),
        Stop::new(0.35, [0.30, 0.25, 0.22]),
        Stop::new(0.70, [0.74, 0.50, 0.28]),
        Stop::new(1.0, [1.0, 0.93, 0.78]),
    ]
}

/// The knobs and the strips, as one style's grade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    /// The distance strip, near to far.
    pub ramp: Vec<Stop>,
    /// The luminance tint, shadows to highlights; it multiplies.
    pub tint: Vec<Stop>,
    /// The gradient map, shadows to highlights; it replaces the hue.
    pub map: Vec<Stop>,
    /// How far the distance ramp reaches, in metres.
    pub ramp_reach: f32,
    /// The shape it runs on: 1 eases, so the strip is spent on the first few
    /// hundred metres and everything beyond crowds into its far end; 0 is
    /// straight, so a stop sits at the distance it says and ridges a few
    /// hundred metres apart land on different parts of the strip.
    pub ramp_curve: f32,
    /// Cut the view into this many slabs of distance, each taking one colour
    /// and one thickness of haze across the whole of it. 0 leaves the haze
    /// smooth; a handful of slabs is what turns ridges into flat cards, since
    /// a stepped ramp colour under a smoothly thickening haze still blends.
    pub ramp_bands: f32,
    /// How much each stop is rescaled to the horizon's brightness. 1 keeps
    /// only the stop's hue (the old behaviour); 0 lets the strip set the
    /// value too, which is what gives banded ridges.
    pub ramp_value: f32,
    /// How much of the haze colour the ramp supplies.
    pub haze_ramp: f32,
    /// Multiplier on the haze density where the ramp is in use.
    pub haze_density: f32,
    /// The height the haze thins over, in metres: the air at `haze_height`
    /// above the sea holds about a third of what the air at the sea does, and
    /// the thickness between two points is integrated along the ray. This is
    /// what stacks ridges as layers, since a valley floor lies under more air
    /// than the crest behind it. 0 gives air of one thickness everywhere.
    pub haze_height: f32,
    /// The far end of the ramp pulled into the low sky, and how far up.
    pub sky_ramp: f32,
    pub sky_reach: f32,
    /// The ramp pulled into shadowed and indirect light.
    pub shade_ramp: f32,
    /// Haze warmer looking into the sun, cooler looking away.
    pub sun_side: f32,
    /// A broad warm glow round the sun, in the sky and the haze.
    pub sun_glow: f32,
    /// Leaves darker toward the ground, lit toward the crown.
    pub crown_shade: f32,
    /// Exposure into the tone map.
    pub exposure: f32,
    /// The luminance strip as a tint: keeps each pixel's own hue.
    pub tone: f32,
    /// The luminance strip as a gradient map: replaces hue with the strip's.
    pub map_amount: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub vignette: f32,
    /// The time of day's own warm/cool shift (golden hour, moonlight).
    pub time_tint: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            ramp: ramp_default(),
            tint: tint_default(),
            map: map_default(),
            ramp_reach: 300.0,
            ramp_curve: 1.0,
            ramp_bands: 0.0,
            ramp_value: 1.0,
            haze_ramp: 0.85,
            haze_density: 1.5,
            haze_height: 150.0,
            sky_ramp: 0.0,
            sky_reach: 0.26,
            shade_ramp: 0.6,
            sun_side: 0.2,
            sun_glow: 0.0,
            crown_shade: 0.0,
            exposure: 1.0,
            tone: 1.0,
            map_amount: 0.0,
            saturation: 1.0,
            contrast: 0.25,
            vignette: 0.22,
            time_tint: 1.0,
        }
    }
}

/// Every knob by the name it has in a file, so a world's own look
/// (`library::WorldLook`) can name one and set it. The list is the knobs in
/// `uniform()`, which a test keeps it level with.
macro_rules! grade_fields {
    ($($name:ident),* $(,)?) => {
        pub const FIELD_NAMES: &[&str] = &[$(stringify!($name)),*];
        impl Grade {
            /// This knob's value, or None if nothing is called that.
            pub fn field(&self, name: &str) -> Option<f32> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }
            /// This knob, to write; None if nothing is called that.
            pub fn field_mut(&mut self, name: &str) -> Option<&mut f32> {
                match name {
                    $(stringify!($name) => Some(&mut self.$name),)*
                    _ => None,
                }
            }
        }
    };
}

grade_fields!(
    ramp_reach,
    ramp_curve,
    ramp_bands,
    ramp_value,
    haze_ramp,
    haze_density,
    haze_height,
    sky_ramp,
    sky_reach,
    shade_ramp,
    sun_side,
    sun_glow,
    crown_shade,
    exposure,
    tone,
    map_amount,
    saturation,
    contrast,
    vignette,
    time_tint,
);

impl Grade {
    /// The classic style's grade: no ramp, a flat warm tint, softer contrast
    /// and a deeper vignette, as the neutral daylight grade always was.
    pub fn classic() -> Grade {
        Grade {
            tint: vec![Stop::new(0.0, [1.04, 1.0, 0.95]), Stop::new(1.0, [1.04, 1.0, 0.95])],
            haze_ramp: 0.0,
            haze_density: 1.0,
            haze_height: 0.0,
            shade_ramp: 0.0,
            sun_side: 0.0,
            exposure: 1.05,
            saturation: 0.96,
            contrast: 0.0,
            vignette: 0.35,
            time_tint: 0.0,
            ..Grade::default()
        }
    }

    /// The `VOXEL_GRADE` study switches: each variant number
    /// sets the knobs that variant stood for. "legacy" turns the lot off.
    pub fn apply_env(&mut self, v: &str) {
        if v.is_empty() {
            return;
        }
        if v.split(',').any(|t| t.trim() == "legacy") {
            self.haze_ramp = 0.0;
            self.haze_density = 1.0;
            self.sky_ramp = 0.0;
            self.shade_ramp = 0.0;
            self.sun_side = 0.0;
            self.sun_glow = 0.0;
            self.crown_shade = 0.0;
            return;
        }
        let n = |key: &str| -> Option<u32> { v.split(',').find_map(|t| t.trim().strip_prefix(key).and_then(|r| r.parse().ok())) };
        if let Some(k) = n("fog") {
            self.haze_ramp = if k == 0 { 0.0 } else { 0.85 };
            self.haze_density = match k {
                0 => 1.0,
                1 => 1.5,
                _ => 2.4,
            };
            if k >= 2 {
                self.ramp = vec![Stop::new(0.0, [1.0, 0.72, 0.42]), Stop::new(0.5, [0.96, 0.64, 0.62]), Stop::new(1.0, [0.58, 0.54, 0.80])];
            }
        }
        if let Some(k) = n("sun") {
            self.sun_side = [0.0, 0.2, 0.38][(k as usize).min(2)];
        }
        if let Some(k) = n("sky") {
            self.sky_ramp = [0.0, 0.75, 0.9][(k as usize).min(2)];
            self.sky_reach = if k >= 2 { 0.4 } else { 0.26 };
        }
        if let Some(k) = n("glow") {
            self.sun_glow = [0.0, 1.0, 2.0][(k as usize).min(2)];
        }
        if let Some(k) = n("tree") {
            self.crown_shade = [0.0, 1.0, 1.6][(k as usize).min(2)];
        }
        if let Some(k) = n("shade") {
            self.shade_ramp = [0.0, 0.6, 0.95][(k as usize).min(2)];
        }
    }

    /// The strip the shaders sample: STRIP_ROWS rows of STRIP_N linear RGBA
    /// samples, each row linearly interpolated between its stops.
    pub fn strip(&self) -> Vec<[f32; 4]> {
        let mut out = vec![[0.0; 4]; (STRIP_N * STRIP_ROWS) as usize];
        for (row, stops) in [&self.ramp, &self.tint, &self.map].into_iter().enumerate() {
            for i in 0..STRIP_N {
                let t = i as f32 / (STRIP_N - 1) as f32;
                let c = sample(stops, t);
                out[row * STRIP_N as usize + i as usize] = [c[0], c[1], c[2], 1.0];
            }
        }
        out
    }

    /// The knobs, as the five vec4s of the shaders' Grade uniform.
    pub fn uniform(&self) -> [f32; 20] {
        [
            self.ramp_reach,
            self.ramp_value,
            self.haze_ramp,
            self.haze_density,
            self.sky_ramp,
            self.sky_reach,
            self.shade_ramp,
            self.sun_side,
            self.sun_glow,
            self.crown_shade,
            self.exposure,
            self.tone,
            self.map_amount,
            self.saturation,
            self.contrast,
            self.vignette,
            self.time_tint,
            self.ramp_curve,
            self.ramp_bands,
            self.haze_height,
        ]
    }

    /// Where on the strip a point `dist` metres away sits, as the shader
    /// works it out (flat_ramp_t in common.wgsl).
    pub fn ramp_t(&self, dist: f32) -> f32 {
        let r = self.ramp_reach.max(1.0);
        let straight = (dist / r).clamp(0.0, 1.0);
        let eased = 1.0 - (-dist / r).exp();
        straight + (eased - straight) * self.ramp_curve.clamp(0.0, 1.0)
    }

    /// How far away a stop at `t` falls, in metres; None once the curve only
    /// reaches it at infinity.
    pub fn ramp_distance(&self, t: f32) -> Option<f32> {
        if t >= 1.0 && self.ramp_curve > 0.0 {
            return None;
        }
        // ramp_t rises with distance, so a bisection finds the inverse of
        // whatever mix of the two shapes is in use.
        let (mut lo, mut hi) = (0.0f32, self.ramp_reach.max(1.0) * 40.0);
        if self.ramp_t(hi) < t {
            return None;
        }
        for _ in 0..40 {
            let mid = (lo + hi) * 0.5;
            if self.ramp_t(mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some((lo + hi) * 0.5)
    }

    pub fn stops(&self, row: usize) -> &Vec<Stop> {
        match row {
            ROW_MAP => &self.map,
            ROW_TINT => &self.tint,
            _ => &self.ramp,
        }
    }

    pub fn stops_mut(&mut self, row: usize) -> &mut Vec<Stop> {
        match row {
            ROW_MAP => &mut self.map,
            ROW_TINT => &mut self.tint,
            _ => &mut self.ramp,
        }
    }

    /// Put a stop in halfway between stop `i` and the next (or the end), and
    /// return where it landed.
    pub fn add_stop(&mut self, row: usize, i: usize) -> usize {
        let s = self.stops_mut(row);
        if s.len() >= 8 {
            return i;
        }
        let a = s.get(i).copied().unwrap_or(Stop::new(0.0, [1.0; 3]));
        let b = s.get(i + 1).copied().unwrap_or(Stop::new(1.0, a.color));
        let mid = Stop::new((a.at + b.at) * 0.5, std::array::from_fn(|k| (a.color[k] + b.color[k]) * 0.5));
        s.insert(i + 1, mid);
        i + 1
    }

    /// Take stop `i` out; the first and last are kept.
    pub fn remove_stop(&mut self, row: usize, i: usize) {
        let s = self.stops_mut(row);
        if s.len() > 2 && i < s.len() {
            s.remove(i);
        }
    }

    /// Keep the stops in order after a position was moved.
    pub fn sort(&mut self) {
        for row in [ROW_RAMP, ROW_TINT, ROW_MAP] {
            self.stops_mut(row).sort_by(|a, b| a.at.total_cmp(&b.at));
        }
    }
}

/// The colour at `t` along a strip's stops (linear between them, flat past
/// the ends).
pub fn sample(stops: &[Stop], t: f32) -> [f32; 3] {
    match stops.len() {
        0 => [1.0; 3],
        1 => stops[0].color,
        _ => {
            if t <= stops[0].at {
                return stops[0].color;
            }
            let last = stops[stops.len() - 1];
            if t >= last.at {
                return last.color;
            }
            for w in stops.windows(2) {
                let (a, b) = (w[0], w[1]);
                if t >= a.at && t <= b.at {
                    let f = if (b.at - a.at).abs() < 1e-6 { 0.0 } else { (t - a.at) / (b.at - a.at) };
                    return std::array::from_fn(|k| a.color[k] + (b.color[k] - a.color[k]) * f);
                }
            }
            last.color
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every knob the shaders read has a name a world can set it by: the
    /// uniform is exactly the knobs, so the two lists are the same length,
    /// and each name reads and writes its own value.
    #[test]
    fn every_knob_has_a_name_that_reads_and_writes_it() {
        let g = Grade::default();
        assert_eq!(FIELD_NAMES.len(), g.uniform().len(), "a knob was added to the uniform without a name");
        for (i, name) in FIELD_NAMES.iter().enumerate() {
            let mut g = Grade::default();
            let was = g.field(name).unwrap_or_else(|| panic!("{name} reads"));
            *g.field_mut(name).unwrap() = was + 1.0;
            assert_eq!(g.field(name), Some(was + 1.0), "{name} writes");
            // Only that one knob moved.
            let before = Grade::default().uniform();
            let after = g.uniform();
            let moved: Vec<usize> = (0..after.len()).filter(|&k| (after[k] - before[k]).abs() > 1e-9).collect();
            assert_eq!(moved.len(), 1, "{name} (name {i}) moved {moved:?}");
        }
        assert!(Grade::default().field("no_such_knob").is_none());
    }

    #[test]
    fn strip_is_the_right_size_and_ends_on_the_stops() {
        let g = Grade::default();
        let s = g.strip();
        assert_eq!(s.len() as u32, STRIP_N * STRIP_ROWS);
        let first = g.ramp[0].color;
        assert!((s[0][0] - first[0]).abs() < 1e-5, "{:?} vs {first:?}", s[0]);
        let last = g.ramp[g.ramp.len() - 1].color;
        let i = STRIP_N as usize - 1;
        assert!((s[i][2] - last[2]).abs() < 1e-5);
    }

    /// The default tint strip still matches the split tone it replaced:
    /// smoothstep(0.1, 0.8) between the teal and the gold tint. Six stops
    /// with straight lines between them track that curve to within 0.009 of a
    /// multiplier, well under a percent, so the frame comes out the same.
    #[test]
    fn map_default_tracks_the_old_split_tone() {
        let g = Grade::default();
        let lo = [0.92f32, 1.0, 1.0];
        let hi = [1.07f32, 1.0, 0.86];
        for i in 0..=20 {
            let l = i as f32 / 20.0;
            let t = ((l - 0.1) / 0.7).clamp(0.0, 1.0);
            let k = t * t * (3.0 - 2.0 * t);
            let want: [f32; 3] = std::array::from_fn(|j| lo[j] + (hi[j] - lo[j]) * k);
            let got = sample(&g.tint, l);
            for j in 0..3 {
                assert!((got[j] - want[j]).abs() < 0.009, "at {l}: {got:?} vs {want:?}");
            }
        }
    }

    /// A stop reads back at the distance the shader puts it, on either shape.
    #[test]
    fn stop_distances_invert_the_ramp_curve() {
        for curve in [0.0, 0.5, 1.0] {
            let g = Grade {
                ramp_curve: curve,
                ramp_reach: 420.0,
                ..Grade::default()
            };
            for t in [0.05f32, 0.25, 0.5, 0.9] {
                let d = g.ramp_distance(t).expect("a stop short of the far end has a distance");
                assert!((g.ramp_t(d) - t).abs() < 1e-3, "curve {curve}, t {t}: {d} m reads back as {}", g.ramp_t(d));
            }
        }
        // Straight: the far end is exactly the reach; eased: it never arrives.
        let straight = Grade {
            ramp_curve: 0.0,
            ramp_reach: 420.0,
            ..Grade::default()
        };
        assert!((straight.ramp_distance(1.0).unwrap() - 420.0).abs() < 1.0);
        assert_eq!(Grade::default().ramp_distance(1.0), None);
    }

    #[test]
    fn legacy_turns_the_studies_off() {
        let mut g = Grade::default();
        g.apply_env("legacy");
        assert_eq!(g.haze_ramp, 0.0);
        assert_eq!(g.shade_ramp, 0.0);
        assert_eq!(g.sun_side, 0.0);
    }

    #[test]
    fn stops_can_be_added_and_taken_out() {
        let mut g = Grade::default();
        let n = g.ramp.len();
        let at = g.add_stop(ROW_RAMP, 0);
        assert_eq!(at, 1);
        assert_eq!(g.ramp.len(), n + 1);
        assert!((g.ramp[1].at - 0.25).abs() < 1e-6);
        g.remove_stop(ROW_RAMP, 1);
        assert_eq!(g.ramp.len(), n);
        for _ in 0..5 {
            g.remove_stop(ROW_RAMP, 0);
        }
        assert_eq!(g.ramp.len(), 2, "the first and last stops stay");
    }
}
