use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use crate::world::noise::{fbm2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// Land that sinks: a delta lobe starved of silt, a marsh settling, ground
/// let down by pumping. Inside a ragged region the ground drops by `drop`
/// metres (varying in patches `grain` m across), or down to `level` m under
/// the sea, easing out over `edge` m at its rim, so wherever it falls under the sea it floods. What stood high
/// stays relatively high, and old channels stay deeper. `islands` keeps a
/// share of the region as low knolls (hammocks, keys) that end up
/// `island_rise` m over the sea, and `floor` caps the water at that depth.
#[derive(Deserialize, Debug)]
pub struct Subside {
    at: Pos,
    radius: f32,
    /// How far the ground drops, metres: [least, most].
    #[serde(default)]
    drop: Option<[f32; 2]>,
    /// Or the depth of water it settles to, metres under the sea: [least,
    /// most]. Ground already deeper (an old channel) stays as it is.
    #[serde(default)]
    level: Option<[f32; 2]>,
    /// Size of the patches the drop varies over, metres.
    #[serde(default = "grain")]
    grain: f32,
    /// Width of the rim the drop eases out over, metres.
    #[serde(default = "edge")]
    edge: f32,
    /// How far the rim wanders in and out, as a share of the radius.
    #[serde(default = "ragged")]
    ragged: f32,
    /// The region is this many times longer than wide along `bearing`.
    #[serde(default = "one")]
    stretch: f32,
    #[serde(default)]
    bearing: f32,
    /// Share of the region (0..0.6) left standing as low islands.
    #[serde(default)]
    islands: f32,
    /// Typical island size, metres.
    #[serde(default = "island_size")]
    island_size: f32,
    /// Height of an island's top over the sea, metres.
    #[serde(default = "island_rise")]
    island_rise: f32,
    /// The deepest the ground may end up under the sea, metres (none: no cap).
    #[serde(default)]
    floor: Option<f32>,
}

fn grain() -> f32 {
    60.0
}
fn edge() -> f32 {
    40.0
}
fn ragged() -> f32 {
    0.25
}
fn one() -> f32 {
    1.0
}
fn island_size() -> f32 {
    28.0
}
fn island_rise() -> f32 {
    0.7
}

impl Process for Subside {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(&self.at);
        let b = self.bearing.to_radians();
        let axis = Vec2::new(b.sin(), -b.cos());
        let side = Vec2::new(-axis.y, axis.x);
        let radius = self.radius.max(5.0);
        let stretch = self.stretch.max(1.0);
        let ragged = self.ragged.clamp(0.0, 0.8);
        let reach = radius * stretch * (1.0 + ragged);
        let edge = (self.edge / radius).clamp(0.01, 0.9);
        let [lo, hi] = self.drop.or(self.level).unwrap_or([1.0, 1.0]);
        let to_level = self.drop.is_none() && self.level.is_some();
        let share = self.islands.clamp(0.0, 0.6);
        // fbm2 spreads roughly over -0.6..0.6: the threshold above which a
        // `share` of cells lie.
        let thr = 0.55 * (1.0 - 2.0 * share);
        let (grain, isz, rise, floor) = (self.grain.max(5.0), self.island_size.max(4.0), self.island_rise, self.floor);
        let noise_at = cx.noise_at;
        let mut sunk = 0usize;
        let mut wet = 0usize;
        each_near(&mut cx.f, c, reach, |f: &mut Fields, i, _, _| {
            let at = f.centre(i);
            let d = at - c;
            let p = at + noise_at;
            let q = Vec2::new(d.dot(axis) / stretch, d.dot(side));
            let a = q.y.atan2(q.x);
            let wobble = fbm2(a.cos() * 1.7 + 40.0, a.sin() * 1.7, 3, seed) + 0.5 * fbm2(p.x / (radius * 0.35), p.y / (radius * 0.35), 2, seed + 1);
            let r = q.length() / radius + wobble * ragged;
            let k = smoothstep(1.0, 1.0 - edge, r);
            if k <= 0.0 {
                return;
            }
            let dn = (fbm2(p.x / grain, p.y / grain, 3, seed + 2) * 0.5 + 0.5).clamp(0.0, 1.0);
            let amount = lo + (hi - lo) * dn;
            let mut h = if to_level { f.h[i] + ((f.sea - amount).min(f.h[i]) - f.h[i]) * k } else { f.h[i] - amount * k };
            if let Some(fl) = floor {
                h = h.max(f.sea - fl);
            }
            if share > 0.0 {
                let n = fbm2(p.x / isz + 13.0, p.y / isz, 3, seed + 3);
                let s = smoothstep(thr, thr + 0.12, n) * k;
                let top = f.sea + rise * (0.6 + 0.4 * smoothstep(thr, thr + 0.35, n));
                if s > 0.0 && top > h {
                    h += (top - h) * s;
                }
            }
            if h < f.h[i] {
                f.h[i] = h;
                f.cause[i] = cause;
                f.moist[i] = f.moist[i].max(0.85 * k);
                sunk += 1;
                if h < f.sea {
                    wet += 1;
                }
            }
        });
        let how = if to_level { "to" } else { "by" };
        format!("{sunk} m² sank {how} {lo}-{hi} m round {:.0},{:.0}, {wet} m² now under the sea", c.x, c.y)
    }
}
