//! The routing engine: lines through a world and what is built along them.
//!
//! - `kit.rs` — a cross-section and what repeats along it, from `library/kits`.
//! - `ribbon.rs` — a 3D polyline plus a kit, drawn per voxel column.
//! - `cost.rs` — finding a line across the land.
//! - `plan.rs` — settlement layouts laid out on a network.
//!
//! The chronicle events `ribbon`, `route`, `network` and `village` are the
//! front doors; everything they share lives here.

pub mod cost;
pub mod kit;
pub mod plan;
pub mod ribbon;

use crate::world::chronicle::geom::{grade_strip, smoothed_ground, Strip};
use crate::world::chronicle::EventCtx;
use crate::world::fields::Fields;
use glam::{Vec2, Vec3};
use kit::{Ground, Kit};
use ribbon::{Pier, Ribbons, Seg};

/// Ground more than this far under the grade line is a gap the route spans.
const GAP: f32 = 3.0;

/// A deck standing this far over the ground leaves it to grow: a canopy
/// walk 20 m up should not strip the ferns off the forest floor under it.
/// Lower decks (a boardwalk, a jetty) still take their ground.
pub const HIGH_DECK: f32 = 6.0;

/// The kits a world has resolved so far, by name, so each is built once.
#[derive(Default)]
pub struct Kits(pub Vec<(String, usize)>);

/// Resolve a kit by name into the world's ribbon store, returning its index.
/// Its `span` kit is resolved with it, so a route can hand over mid-line.
pub fn kit_index(cx: &mut EventCtx, name: &str) -> Option<usize> {
    if let Some(&(_, k)) = cx.state.get::<Kits>().0.iter().find(|(n, _)| n == name) {
        return Some(k);
    }
    let Some(def) = cx.lib.kit(name).map(|i| cx.lib.kits[i].clone()) else {
        cx.warnings.push(format!("unknown kit {name}"));
        return None;
    };
    let mut warnings = Vec::new();
    let mut k = Kit::build(&def, cx.lib, &mut |w| warnings.push(w));
    cx.warnings.extend(warnings);
    // Resolve the span kit first: it lands at a lower index, and a kit never
    // names itself as its own span (that would not terminate).
    let span = match &def.span {
        Some(s) if s != name => kit_index(cx, s),
        _ => None,
    };
    k.span = span;
    let r = cx.state.get::<Ribbons>();
    r.kits.push(k);
    let ix = r.kits.len() - 1;
    cx.state.get::<Kits>().0.push((name.to_string(), ix));
    Some(ix)
}

/// What the chronicle may say about how a line should pick its way, over the
/// kit's own defaults. Every field is optional; what is left out is the kit's.
#[derive(serde::Deserialize, Debug, Clone, Default)]
pub struct Terms {
    /// The steepest grade the line holds before it starts paying.
    #[serde(default)]
    pub grade: Option<f32>,
    /// What crossing open water costs, as a multiple of distance; 0 refuses it.
    #[serde(default)]
    pub water: Option<f32>,
    /// How far it will go out of its way to avoid ground already taken.
    #[serde(default)]
    pub taken: Option<f32>,
    /// Search step, metres.
    #[serde(default)]
    pub step: Option<f32>,
    /// The deepest water the line will wade rather than bridge, metres.
    #[serde(default)]
    pub wade: Option<f32>,
    /// What a metre of climb costs a watercourse (`watercourse` sets this).
    #[serde(default)]
    pub downhill: Option<f32>,
    /// The most a watercourse may climb in one step, metres.
    #[serde(default)]
    pub max_climb: Option<f32>,
}

impl Terms {
    /// The search terms for a kit under this chronicle's wishes.
    pub fn over(&self, kit: &Kit) -> cost::Terms {
        let mut t = cost::Terms {
            max_span: kit.max_span,
            ..Default::default()
        };
        if let Some(g) = self.grade {
            t.max_grade = g;
        }
        if let Some(w) = self.water {
            t.water = (w > 0.0).then_some(w);
        }
        if let Some(k) = self.taken {
            t.taken = k;
        }
        if let Some(s) = self.step {
            t.step = s.clamp(1.0, 16.0);
        }
        if let Some(w) = self.wade {
            t.wade = w;
        }
        if let Some(h) = self.downhill {
            t.downhill = h;
        }
        if let Some(c) = self.max_climb {
            t.max_climb = c;
        }
        // A kit that cannot span anything will not cross water at all.
        if kit.span.is_none() && kit.ground == Ground::Grade && kit.max_span <= 0.0 {
            t.water = None;
        }
        t
    }
}

impl Terms {
    /// The search terms for a watercourse: a line that only ever falls. There
    /// is no kit, so the chronicle's own fields are all that steer it.
    pub fn over_water(&self) -> cost::Terms {
        let mut t = cost::Terms {
            step: 4.0,
            downhill: 60.0,
            max_climb: 0.4,
            cliff: 0.0,
            taken: 1.0,
            ..Default::default()
        };
        if let Some(h) = self.downhill {
            t.downhill = h;
        }
        if let Some(c) = self.max_climb {
            t.max_climb = c;
        }
        if let Some(s) = self.step {
            t.step = s.clamp(1.0, 16.0);
        }
        if let Some(k) = self.taken {
            t.taken = k;
        }
        t
    }
}

/// Find a line from `a` to `b` on a grid sampled from the world as it stands,
/// then tidy it: the points are dropped to the ones that matter and the
/// result is curved, ready for `lay`.
pub fn find_line(g: &cost::Grid, a: Vec2, b: Vec2, t: &cost::Terms) -> Option<Vec<Vec2>> {
    let raw = cost::find(g, a, b, t)?;
    let keep = cost::simplify(&raw, t.step * 0.4);
    Some(crate::world::chronicle::geom::spline(&keep, false, 1.0))
}

/// How a line was laid: what it did to the land and what it had to span.
pub struct Laid {
    pub length: f32,
    pub segs: usize,
    pub spans: Vec<(Vec2, Vec2, f32)>,
    pub piers: usize,
}

/// Lay a route: grade the land under it where the kit asks, then emit the
/// segments and supports that draw it. `line` is the centre line, about a
/// metre between points. `at` fixes the surface height above sea level
/// (a bored passage, a skywalk strung between two towers) instead of letting
/// it follow the land.
pub fn lay_at(cx: &mut EventCtx, line: &[Vec2], kit_ix: usize, smooth: f32, at: Option<f32>) -> Laid {
    let heights = at.map(|h| vec![cx.f.sea + h; line.len()]);
    lay_inner(cx, line, kit_ix, smooth, heights)
}

/// Lay a line whose surface height is given point by point, metres above
/// sea level as the world measures it (absolute): a walkway climbing from
/// one platform to another, a stair wound round a trunk. Deck kits only
/// make sense here; a graded kit would cut the land to the heights given.
pub fn lay_heights(cx: &mut EventCtx, line: &[Vec2], kit_ix: usize, heights: Vec<f32>) -> Laid {
    assert_eq!(line.len(), heights.len());
    lay_inner(cx, line, kit_ix, 0.0, Some(heights))
}

fn lay_inner(cx: &mut EventCtx, line: &[Vec2], kit_ix: usize, smooth: f32, heights: Option<Vec<f32>>) -> Laid {
    if line.len() < 2 {
        return Laid {
            length: 0.0,
            segs: 0,
            spans: Vec::new(),
            piers: 0,
        };
    }
    let at = heights.is_some();
    let kit = cx.state.get::<Ribbons>().kits[kit_ix].clone();
    let length: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    // The height the surface runs at, point by point.
    let base: Vec<f32> = match (heights, kit.ground) {
        (Some(h), _) => h,
        _ => match kit.ground {
            Ground::Grade => smoothed_ground(&cx.f, line, smooth, false),
            Ground::Deck => {
                let sea = cx.f.sea;
                smoothed_ground(&cx.f, line, smooth.min(24.0), false).iter().map(|h| h.max(sea) + kit.lift).collect()
            }
            _ => line.iter().map(|p| cx.f.height(p.x, p.y)).collect(),
        },
    };
    // Where the land falls away from the line: those stretches fly.
    let airborne: Vec<bool> = match kit.ground {
        Ground::Bore | Ground::None | Ground::Follow if at => vec![false; line.len()],
        Ground::Grade => line.iter().enumerate().map(|(k, p)| cx.f.height(p.x, p.y) < base[k] - GAP).collect(),
        Ground::Deck => line.iter().enumerate().map(|(k, p)| cx.f.height(p.x, p.y) < base[k] - 0.5).collect(),
        _ => vec![false; line.len()],
    };
    // A line that sits on the ground stops at the waterline: a fence does not
    // wade out to sea, and a wall round a village stops at the beach.
    let skip: Vec<bool> = if kit.ground == Ground::Follow {
        let sea = cx.f.sea;
        line.iter().map(|p| cx.f.height(p.x, p.y) < sea + 0.3 || cx.f.water[cx.f.idx(p.x, p.y)] == 1).collect()
    } else {
        vec![false; line.len()]
    };
    let mut spans = Vec::new();
    if kit.ground == Ground::Grade {
        let off = vec![0.0; line.len()];
        let strip = Strip {
            width: kit.half * 2.0,
            mat: kit.mat,
            shoulder: kit.shoulder,
            margin: kit.margin,
        };
        spans = grade_strip(cx, line, &base, &off, &[], &strip);
    } else if kit.ground == Ground::Deck || kit.ground == Ground::Follow {
        // Nothing is cut, but the ground under a deck or a fence is still
        // taken: plants and later events keep off it. Not under a high
        // deck, which the ground grows on under (flora.rs keeps the plants
        // that would reach it out).
        let low: Vec<bool> = if kit.ground == Ground::Deck {
            line.iter().zip(&base).map(|(p, &y)| y - cx.f.height(p.x, p.y) < HIGH_DECK).collect()
        } else {
            vec![true; line.len()]
        };
        take(&mut cx.f, line, &low, kit.reach);
    }
    // Sag an airborne run toward its middle, so a rope bridge hangs.
    let base = sagged(&base, &airborne, kit.sag);
    let segs = emit(cx, line, &base, &airborne, &skip, kit_ix, &kit);
    let piers = piers(cx, line, &base, &airborne, &kit);
    if kit.half > 0.8 && kit.ground != Ground::Bore {
        cx.state.get::<RoadLines>().0.push((line.to_vec(), kit.half));
    }
    Laid { length, segs, spans, piers }
}

/// Every road laid so far that carries traffic (a surface wider than a
/// fence), as its centre line and half-width: what a later `town` or
/// `fields` event hangs its fields off and keeps its walls clear of.
#[derive(Default)]
pub struct RoadLines(pub Vec<(Vec<Vec2>, f32)>);

/// Lay a route that follows the land.
pub fn lay(cx: &mut EventCtx, line: &[Vec2], kit_ix: usize, smooth: f32) -> Laid {
    lay_at(cx, line, kit_ix, smooth, None)
}

/// Mark the ground within `r` m of the line as taken, along every piece
/// whose two ends are both `low`.
fn take(f: &mut Fields, line: &[Vec2], low: &[bool], r: f32) {
    for (k, w) in line.windows(2).enumerate() {
        if !(low[k] && low[k + 1]) {
            continue;
        }
        let steps = (w[0].distance(w[1]) / 0.8).ceil().max(1.0) as i32;
        for k in 0..=steps {
            let p = w[0].lerp(w[1], k as f32 / steps as f32);
            crate::world::chronicle::geom::each_near(f, p, r, |f: &mut Fields, i, _, _| {
                f.occupied[i] = true;
            });
        }
    }
}

/// Drop each airborne run toward its middle by up to `sag` m.
fn sagged(base: &[f32], airborne: &[bool], sag: f32) -> Vec<f32> {
    let mut out = base.to_vec();
    if sag <= 0.0 {
        return out;
    }
    let mut k = 0;
    while k < airborne.len() {
        if !airborne[k] {
            k += 1;
            continue;
        }
        let a = k;
        while k < airborne.len() && airborne[k] {
            k += 1;
        }
        let n = (k - a) as f32;
        for (j, h) in out[a..k].iter_mut().enumerate() {
            let t = (j as f32 + 0.5) / n;
            *h -= sag * (4.0 * t * (1.0 - t));
        }
    }
    out
}

/// Turn the fine line into as few segments as will still follow it: a run is
/// extended while it stays within 0.3 m of the points it covers, in plan and
/// in height, and does not change between ground and air.
fn emit(cx: &mut EventCtx, line: &[Vec2], base: &[f32], airborne: &[bool], skip: &[bool], kit_ix: usize, kit: &Kit) -> usize {
    let span_ix = kit.span.unwrap_or(kit_ix);
    let decked_kit = |fly: bool| if fly { span_ix } else { kit_ix };
    let mut out: Vec<Seg> = Vec::new();
    let mut s0 = 0.0f32;
    let mut a = 0usize;
    let mut k = 1usize;
    while a + 1 < line.len() {
        if skip[a] {
            s0 += line[a].distance(line[a + 1]);
            a += 1;
            k = a + 1;
            continue;
        }
        let fly = airborne[a];
        // Extend while the straight line from `a` still describes the points.
        while k + 1 < line.len() {
            let n = k + 1;
            if airborne[n] != fly || skip[n] || line[a].distance(line[n]) > 32.0 {
                break;
            }
            let (pa, pb) = (line[a], line[n]);
            let (ya, yb) = (base[a], base[n]);
            let ab = pb - pa;
            let l2 = ab.length_squared().max(1e-6);
            let ok = (a..=n).all(|j| {
                let t = ((line[j] - pa).dot(ab) / l2).clamp(0.0, 1.0);
                line[j].distance(pa + ab * t) < 0.3 && (base[j] - (ya + (yb - ya) * t)).abs() < 0.3
            });
            if !ok {
                break;
            }
            k = n;
        }
        let len = line[a].distance(line[k]);
        out.push(Seg {
            a: Vec3::new(line[a].x, base[a], line[a].y),
            b: Vec3::new(line[k].x, base[k], line[k].y),
            kit: decked_kit(fly) as u16,
            s0,
            // A ground run of a decked kit still needs its slab: only a
            // graded line lets the land be the surface.
            decked: fly || kit.ground != Ground::Grade,
        });
        s0 += len;
        a = k;
        k = a + 1;
    }
    let n = out.len();
    cx.state.get::<Ribbons>().segs.extend(out);
    n
}

/// Supports under the stretches that stand clear of the ground.
fn piers(cx: &mut EventCtx, line: &[Vec2], base: &[f32], airborne: &[bool], kit: &Kit) -> usize {
    let Some(p) = kit.piers else { return 0 };
    let mut out = Vec::new();
    let mut since = p.every;
    let mut walked = 0.0f32;
    for k in 0..line.len() {
        if k > 0 {
            let d = line[k - 1].distance(line[k]);
            walked += d;
            since += d;
        }
        if !airborne[k] || since < p.every {
            continue;
        }
        let g = cx.f.height(line[k].x, line[k].y);
        if base[k] - g < p.min_drop {
            continue;
        }
        out.push(Pier {
            at: line[k],
            r: p.radius,
            top: base[k] - kit.drop,
            mat: p.mat,
        });
        since = 0.0;
    }
    let _ = walked;
    let n = out.len();
    cx.state.get::<Ribbons>().piers.extend(out);
    n
}

/// An apron where lines meet: the surface stamped over a disc so the corners
/// of two carriageways join instead of crossing.
pub fn junction(cx: &mut EventCtx, at: Vec2, r: f32, mat: u8) {
    let base = cx.f.height(at.x, at.y);
    let cause = cx.cause;
    crate::world::chronicle::geom::each_near(&mut cx.f, at, r + 3.0, |f: &mut Fields, i, d, _| {
        if f.water[i] == 1 {
            return;
        }
        if d <= r {
            f.h[i] = base;
            f.surface[i] = mat;
            f.occupied[i] = true;
            f.water[i] = 3;
            f.cliff[i] = 0.0;
            f.cause[i] = cause;
        } else {
            let t = crate::world::noise::smoothstep(r + 3.0, r, d);
            f.h[i] += (base - f.h[i]) * t;
            f.cliff[i] *= 1.0 - t;
        }
    });
}
