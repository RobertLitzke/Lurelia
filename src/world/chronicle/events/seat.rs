use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos};
use glam::Vec2;
use serde::Deserialize;

/// Find a place on the land as it came out, and name it for later events to
/// build on (`{"from": name, "by": [..]}`, see `Pos`). On land the generator
/// shaped, fixed coordinates stop meaning anything the moment a setting
/// changes; a rule keeps the hot pool on its shelf and the wreck in its cove.
///
/// It looks within `within` m of `near` (itself any `Pos`) for the spot that
/// best fits `rule`, preferring nearer ones a little:
///
/// - `flat`: the most level ground, `size` m across.
/// - `high`: the highest ground. `low`: the lowest.
/// - `shore`: dry land at the water's edge, the lower the ground behind it
///   the better. Faces inland.
/// - `cove`: sea with land close round it. Faces open water.
/// - `sea`: open water, clear of land for `size` m.
///
/// `elev` ([least, most], metres above the sea) limits where it may look.
#[derive(Deserialize, Debug)]
pub struct Seat {
    name: String,
    near: Pos,
    within: f32,
    rule: String,
    #[serde(default)]
    elev: Option<[f32; 2]>,
    #[serde(default = "default_size")]
    size: f32,
}

fn default_size() -> f32 {
    12.0
}

/// Sixteen points round a ring and its middle.
fn ring(f: &Fields, p: Vec2, r: f32, mut g: impl FnMut(Vec2, f32)) {
    g(p, f.height(p.x, p.y));
    for k in 0..16 {
        let a = k as f32 * std::f32::consts::TAU / 16.0;
        let rr = if k % 2 == 0 { r } else { r * 0.55 };
        let q = p + Vec2::new(a.cos(), a.sin()) * rr;
        if q.x < 0.0 || q.y < 0.0 || q.x >= f.size || q.y >= f.size {
            continue;
        }
        g(q, f.height(q.x, q.y));
    }
}

impl Process for Seat {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let near = cx.pos(&self.near);
        let f = &cx.f;
        let sea = f.sea;
        let r = self.within.max(4.0);
        let size = self.size.max(2.0);
        let mut best: Option<(f32, Vec2, Option<f32>)> = None;
        let steps = (r / 3.0).ceil() as i32;
        for dz in -steps..=steps {
            for dx in -steps..=steps {
                let p = near + Vec2::new(dx as f32, dz as f32) * 3.0;
                let d = p.distance(near);
                // Nothing is seated on the rim of the world.
                let rim = 40.0f32.min(f.size * 0.04);
                if d > r || p.x < rim || p.y < rim || p.x >= f.size - rim || p.y >= f.size - rim {
                    continue;
                }
                let h = f.height(p.x, p.y);
                if let Some([lo, hi]) = self.elev {
                    if h - sea < lo || h - sea > hi {
                        continue;
                    }
                }
                // Lower is better.
                let mut faces = None;
                let score = match self.rule.as_str() {
                    "flat" => {
                        // Dry, which a sheet of sea ice a hand over the water is.
                        if h < sea + 0.05 {
                            continue;
                        }
                        let (mut lo, mut hi) = (h, h);
                        ring(f, p, size * 0.5, |_, q| {
                            lo = lo.min(q);
                            hi = hi.max(q);
                        });
                        (hi - lo) + d * 0.02
                    }
                    "high" => -h + d * 0.02,
                    "low" => h + d * 0.02,
                    "shore" | "cove" | "sea" => {
                        let (mut wet, mut all) = (0.0, 0.0);
                        let mut to_sea = Vec2::ZERO;
                        let mut tallest = h;
                        let reach = if self.rule == "cove" { size * 2.5 } else { size };
                        ring(f, p, reach, |q, hq| {
                            all += 1.0;
                            tallest = tallest.max(hq);
                            if hq < sea {
                                wet += 1.0;
                                to_sea += q - p;
                            }
                        });
                        let share = wet / all;
                        match self.rule.as_str() {
                            "shore" => {
                                if h < sea + 0.4 || h > sea + 5.0 || share < 0.15 {
                                    continue;
                                }
                                faces = Some(crate::util::bearing(-to_sea).to_degrees().rem_euclid(360.0));
                                // A landing wants ground behind it, not the
                                // foot of a cliff.
                                d + (tallest - sea).min(90.0) * 3.0
                            }
                            "cove" => {
                                // Sea underfoot and close by, land in the
                                // wider ring.
                                let mut close = true;
                                ring(f, p, size * 0.6, |_, hq| close &= hq < sea);
                                if h > sea - 0.5 || !close || share > 0.8 {
                                    continue;
                                }
                                faces = Some(crate::util::bearing(to_sea).to_degrees().rem_euclid(360.0));
                                share * 40.0 + d * 0.03
                            }
                            _ => {
                                if h > sea - 1.0 || share < 0.99 {
                                    continue;
                                }
                                d
                            }
                        }
                    }
                    other => {
                        cx.warnings.push(format!("seat {}: no rule called {other}", self.name));
                        return "skipped".into();
                    }
                };
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, p, faces));
                }
            }
        }
        match best {
            Some((_, p, faces)) => {
                cx.seat(&self.name, p, faces);
                format!("{} ({}) at {:.0},{:.0}, {:.0} m from where it was asked for", self.name, self.rule, p.x, p.y, p.distance(near))
            }
            None => {
                cx.warnings.push(format!("seat {}: nowhere within {r:.0} m fits rule {}", self.name, self.rule));
                cx.seat(&self.name, near, None);
                format!("{}: nothing fits, left where it was asked for", self.name)
            }
        }
    }
}
