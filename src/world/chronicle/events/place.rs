use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::material::{AIR, KEEP_AIR};
use crate::world::route;
use crate::world::shape;
use glam::Vec2;
use serde::Deserialize;

/// One structure: a landing, a shrine, an anomaly.
#[derive(Deserialize, Debug)]
pub struct Place {
    structure: String,
    at: Pos,
    #[serde(default)]
    bearing: Option<f32>,
    #[serde(default)]
    face: Option<Pos>,
    #[serde(default)]
    slots: HashMap<String, String>,
    /// Years of neglect for decay.
    #[serde(default)]
    age: f32,
    #[serde(default)]
    poi: Option<String>,
    /// Hang it at this height above sea level (its anchor voxel there),
    /// whatever the ground: a rope bridge strung between two rock walls.
    #[serde(default)]
    height: Option<f32>,
    /// When it stands: hours of the day, only from afar, only close by
    /// (fields::When). The rest of the time its plot is empty.
    #[serde(default)]
    when: Option<crate::world::fields::When>,
}

impl Process for Place {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Place { structure, at, bearing, face, slots, age, poi, height, when } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let p = cx.pos(at);
        let Some(d) = cx.lib.structure(structure) else {
            cx.warnings.push(format!("place: unknown structure {structure}"));
            return "skipped".into();
        };
        let b = match (bearing, face, at) {
            (Some(b), _, _) => *b + 90.0 * cx.turn(at) as f32,
            (None, Some(t), _) => crate::util::bearing(cx.pos(t) - p).to_degrees(),
            // Beside the race track: facing the road.
            (None, None, Pos::Along { along, .. }) => match &cx.track {
                Some(t) => crate::util::bearing(t.at(*along).0 - p).to_degrees(),
                None => 0.0,
            },
            _ => 0.0,
        };
        // A levelled structure given a height has its pad levelled there.
        let sd = &cx.lib.structures[d];
        let ground = height.filter(|_| sd.level && sd.base.is_none()).map(|h| cx.f.sea + h);
        let k = cx.place_on(d, p, b, slots, *age, seed, ground);
        if let (Some(k), Some(h)) = (k, height) {
            let lift = cx.lib.structures[d].anchor[1];
            cx.placed[k].origin[1] = ((cx.f.sea + h) * 4.0).round() as i32 - lift;
        }
        // A bridge meets the land and the road at both ends.
        let sd = &cx.lib.structures[d];
        let fit = match k {
            Some(k) if sd.base.as_deref() == Some("rim") && sd.tags.iter().any(|t| t == "bridge") => fit_bridge(cx, k, height.is_none()),
            _ => String::new(),
        };
        if let (Some(k), Some(w)) = (k, when) {
            cx.when.push((k as u32, w.clone()));
        }
        cx.pois.push(Poi { name: poi.clone().unwrap_or(structure.clone()), pos: p, cause });
        format!("{structure} at {:.0},{:.0} facing {b:.0} deg{fit}", p.x, p.y)
    }
}

/// One end of a bridge: where its tip is, which way is off the bridge, and
/// the voxel row whose bottom is its walking surface (above the origin).
struct Tip {
    at: Vec2,
    out: Vec2,
    row: i32,
}

/// The ground just off a bridge tip: mean height over the 0.5 to 3 m beyond
/// it, across the deck's width, of cells that are not water. None if all are.
fn ground_off(f: &Fields, t: &Tip, hw: f32) -> Option<f32> {
    let side = Vec2::new(-t.out.y, t.out.x);
    let (mut sum, mut n) = (0.0, 0);
    for s in [0.5, 1.5, 2.5, 3.0] {
        for j in -2..=2 {
            let p = t.at + t.out * s + side * (hw * j as f32 / 2.0);
            if !route::wet(f, f.idx(p.x, p.y)) {
                sum += f.height(p.x, p.y);
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum / n as f32)
}

/// The tips of placed structure `k`, a bridge whose long axis is local X or
/// Z (whichever is longer) with its deck centred on the anchor across it;
/// and the deck's half width, metres.
fn tips(cx: &EventCtx, k: usize) -> (Vec<Tip>, f32) {
    let pl = &cx.placed[k];
    let sd = &cx.lib.structures[pl.def];
    let sh = &cx.shapes[pl.shape];
    let size = sh.size;
    let along_z = size[2] >= size[0];
    let len = if along_z { size[2] } else { size[0] };
    let across = if along_z { sd.anchor[0] } else { sd.anchor[2] };
    let local = |a: i32, c: i32| if along_z { (c, a) } else { (a, c) };
    let world = |x: i32, z: i32| {
        let (rx, rz) = shape::rotate(x, z, size, pl.rot);
        Vec2::new((pl.origin[0] + rx) as f32 + 0.5, (pl.origin[2] + rz) as f32 + 0.5) * 0.25
    };
    let solid = |x: i32, y: i32, z: i32| !matches!(sh.get(x, y, z).0, AIR | KEEP_AIR);
    let (x0, z0) = local(sd.anchor[if along_z { 2 } else { 0 }], across);
    let centre = world(x0, z0);
    let mut out = Vec::new();
    let mut hw = 0.0f32;
    for a in [1, len - 2] {
        let (x, z) = local(a, across);
        // The walking surface: the first air above the first solid, from
        // the bottom of the column up.
        let mut seen = false;
        let mut row = None;
        for y in 0..size[1] {
            if solid(x, y, z) {
                seen = true;
            } else if seen {
                row = Some(y);
                break;
            }
        }
        let Some(row) = row else { continue };
        // The deck's width at the tip: solid in the row under the surface,
        // outward from the anchor each way.
        for dir in [-1, 1] {
            let mut c = across;
            loop {
                let (x, z) = local(a, c + dir);
                if !solid(x, row - 1, z) || (c - across).abs() > 32 {
                    break;
                }
                c += dir;
            }
            hw = hw.max(((c - across).abs() as f32 + 0.5) * 0.25);
        }
        let at = world(x, z);
        out.push(Tip { at, out: (at - centre).normalize_or_zero(), row });
    }
    (out, hw.clamp(1.0, 8.0))
}

/// Seat a bridge on its banks. Without a given height its deck is set to
/// the mean of the ground just off its two tips (the highest ground under
/// the whole footprint, the `rim` default, leaves a lip at the lower end).
/// Then the ground at each end is cut or filled to the deck and eased back
/// over a ramp, and the nearest graded road on that bank within 45 m is
/// carried on to the tip with its own kit. An end whose ground is more than
/// 5 m off the deck (a cliff, a canyon rim) is left alone.
fn fit_bridge(cx: &mut EventCtx, k: usize, free: bool) -> String {
    let (tips, hw) = tips(cx, k);
    if tips.is_empty() {
        return String::new();
    }
    let mut note = String::new();
    if free {
        let gs: Vec<(f32, i32)> = tips.iter().filter_map(|t| ground_off(&cx.f, t, hw).map(|g| (g, t.row))).collect();
        if !gs.is_empty() {
            let want = gs.iter().map(|(g, _)| g).sum::<f32>() / gs.len() as f32;
            let row = gs.iter().map(|(_, r)| *r as f32).sum::<f32>() / gs.len() as f32;
            let was = cx.placed[k].origin[1];
            cx.placed[k].origin[1] = (want * 4.0 - row).round() as i32;
            note += &format!("; deck {:+.2} m", (cx.placed[k].origin[1] - was) as f32 * 0.25);
        }
    }
    let oy = cx.placed[k].origin[1];
    for t in &tips {
        let top = (oy + t.row) as f32 * 0.25;
        let Some(g) = ground_off(&cx.f, t, hw) else {
            // Free-standing in the water at a chain's inner end is fine;
            // a bridge set on the ground should land on both banks.
            if free {
                let name = &cx.lib.structures[cx.placed[k].def].name;
                cx.warnings.push(format!("bridge {name} at {:.0},{:.0}: an end stands in the water", t.at.x, t.at.y));
            }
            continue;
        };
        if (g - top).abs() > 5.0 || !route::grade_landing(cx, t.at, t.out, top, hw) {
            note += "; an end left at a drop";
            continue;
        }
        if let Some(d) = route::join_road(cx, t.at, t.out, top) {
            note += &format!("; road joined over {d:.0} m");
        }
    }
    // The deck and a landing 8 m off each end are taken, so a town builds
    // no house across the head of the bridge and nothing grows on it.
    for t in &tips {
        cx.state.get::<route::BridgeHeads>().0.push(route::BridgeHead { tip: t.at, out: t.out, half: hw });
        let side = Vec2::new(-t.out.y, t.out.x);
        let far = tips.iter().map(|u| (u.at - t.at).dot(-t.out)).fold(0.0f32, f32::max);
        crate::world::chronicle::geom::each_near(&mut cx.f, t.at, far + 10.0, |f, i, _, p| {
            let v = p - t.at;
            let s = v.dot(t.out);
            if v.dot(side).abs() <= hw + 1.0 && s <= 8.0 && s >= -far * 0.5 - 0.5 {
                f.occupied[i] = true;
            }
        });
    }
    note
}
