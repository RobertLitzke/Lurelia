use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::fbm2;
use glam::Vec2;
use serde::Deserialize;

/// Earth heaped up by people: effigy mounds and ridges (a bird with spread
/// wings, the concentric half rings of Poverty Point). Each `part` is a ridge
/// along a segment, each `arc` one along part of a circle; both have a flat
/// crest `width` m across standing `top` m over the sea and grassy sides at
/// `slope` (metres of run per metre of rise). Where pieces overlap the higher
/// one wins, so a body, wings and a head join into one mound. Ground already
/// higher is left alone. With `mat` the raised ground is laid in that
/// material instead of grassing over: a granite breakwater, a shingle bank.
#[derive(Deserialize, Debug)]
pub struct Earthwork {
    #[serde(default)]
    parts: Vec<Part>,
    #[serde(default)]
    arcs: Vec<Arc>,
    /// Run per rise of the sides (1.8: about 30 degrees).
    #[serde(default = "slope")]
    slope: f32,
    /// `top` is over the ground round each cell instead of over the sea (low
    /// ridges on rolling ground).
    #[serde(default)]
    relative: bool,
    /// A name for the gazetteer's points of interest.
    #[serde(default)]
    poi: Option<String>,
    /// Surface material for everything it raises (a breakwater's granite);
    /// left out, the biome's own ground covers it.
    #[serde(default)]
    mat: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Part {
    from: Pos,
    to: Pos,
    width: f32,
    top: f32,
}

#[derive(Deserialize, Debug)]
struct Arc {
    centre: Pos,
    radius: f32,
    /// Compass bearings the arc runs between, clockwise from `from`.
    from: f32,
    to: f32,
    width: f32,
    top: f32,
}

fn slope() -> f32 {
    1.8
}

impl Process for Earthwork {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let s = self.slope.max(0.3);
        let relative = self.relative;
        let mat = self.mat.as_ref().map(|m| cx.mat(m));
        // Relative heights stand on the ground as it was before this event,
        // so ridges that meet don't stack.
        let ground = if relative { cx.f.h.clone() } else { Vec::new() };
        let mut raised = 0usize;
        let mut highest = f32::MIN;
        let parts: Vec<(Vec2, Vec2, f32, f32)> = self.parts.iter().map(|p| (cx.pos(&p.from), cx.pos(&p.to), p.width * 0.5, p.top)).collect();
        let arcs: Vec<(Vec2, f32, f32, f32, f32, f32)> = self
            .arcs
            .iter()
            .map(|a| {
                let span = (a.to - a.from).rem_euclid(360.0);
                (cx.pos(&a.centre), a.radius, a.from, span, a.width * 0.5, a.top)
            })
            .collect();
        let mut raise = |f: &mut Fields, i: usize, top: f32, d: f32, half: f32| {
            // A little unevenness on the crest and the sides: heaped by hand.
            let p = f.centre(i);
            let wob = fbm2(p.x * 0.08, p.y * 0.08, 2, seed) * 0.25;
            let base = if relative { ground[i] } else { f.sea };
            let h = base + top + wob - (d - half).max(0.0) / s;
            if h > f.h[i] + 0.02 {
                f.h[i] = h;
                f.cause[i] = cause;
                if let Some(m) = mat {
                    f.surface[i] = m;
                }
                raised += 1;
                highest = highest.max(h - f.sea);
            }
        };
        for &(a, b, half, top) in &parts {
            let reach = half + top * s + 2.0;
            let mid = (a + b) * 0.5;
            let r = a.distance(b) * 0.5 + reach;
            let ab = b - a;
            each_near(&mut cx.f, mid, r, |f: &mut Fields, i, _, _| {
                let p = f.centre(i);
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let d = p.distance(a + ab * t);
                if d < reach {
                    raise(f, i, top, d, half);
                }
            });
        }
        for &(c, radius, from, span, half, top) in &arcs {
            let reach = half + top * s + 2.0;
            each_near(&mut cx.f, c, radius + reach, |f: &mut Fields, i, _, _| {
                let p = f.centre(i);
                let v = p - c;
                // Compass bearing of the cell from the centre: 0 north (-z), 90 east.
                let b = v.x.atan2(-v.y).to_degrees().rem_euclid(360.0);
                if (b - from).rem_euclid(360.0) > span {
                    return;
                }
                let d = (v.length() - radius).abs();
                if d < reach {
                    raise(f, i, top, d, half);
                }
            });
        }
        if let (Some(name), Some(&(a, b, _, _))) = (&self.poi, parts.first()) {
            cx.pois.push(Poi { name: name.clone(), pos: (a + b) * 0.5, cause });
        }
        format!("{raised} m² heaped up, {:.1} m at the highest", highest.max(0.0))
    }
}
