use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{densify, each_near, spline};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::falls::{Fall, FALL_MIN};
use crate::world::fields::{Fields, Pos, Wet};
use crate::world::noise::perlin2;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A river carved along `path` to the sea: a channel whose bed falls steadily
/// toward the mouth, banks sloping back to the ground, water in the middle.
#[derive(Deserialize, Debug)]
pub struct River {
    name: String,
    path: Vec<Pos>,
    width: f32,
    depth: f32,
    /// The river bed under the water.
    bed: String,
    /// Mud or sand along the waterline: the shallow edges of the bed and a
    /// ragged strip of bank above the water. Without it the bed material
    /// runs to the edge and the bank keeps its biome's ground.
    #[serde(default)]
    bank: Option<String>,
    /// Old worlds name the material the channel was once painted with; the
    /// water is drawn as a surface now, so it is read and ignored.
    #[serde(default)]
    #[allow(dead_code)]
    water: Option<String>,
    #[serde(default = "default_meander")]
    meander: f32,
    /// The deepest the bed may be carved, metres under the sea. The bed must
    /// fall a little at every step, which across flat country (a delta) adds
    /// up to a gorge; this stops it. None: no limit.
    #[serde(default)]
    floor: Option<f32>,
    /// Metres over which the river grows from a trickle to its full width
    /// and depth. 0, the default, starts it full-grown, which reads as a
    /// river from nowhere unless it runs out of a lake: give one that rises
    /// at a spring 150 to 300.
    #[serde(default)]
    rise: f32,
    /// The river comes in from beyond the world: carry it back from its
    /// first point to the nearest edge, so it does not begin a few metres
    /// inside the map.
    #[serde(default)]
    from_edge: bool,
    /// Bend the path into a smooth curve through its points before anything
    /// else. A path of found points (a `drainage` river's) is otherwise a row
    /// of straight stretches with a corner at each one.
    #[serde(default)]
    curve: bool,
}

fn default_meander() -> f32 {
    6.0
}

/// A carved river, kept so later events (avulsion, the final settle) know it.
#[derive(Clone)]
pub struct RiverRec {
    pub line: Vec<Vec2>,
    pub width: f32,
    pub depth: f32,
    pub bed: u8,
    /// Waterline material, 0 for none.
    pub bank: u8,
    pub seed: u32,
    /// Deepest bed under the sea (`River::floor`).
    pub floor: Option<f32>,
}

/// Every river so far, by name.
#[derive(Default)]
pub struct Rivers(pub HashMap<String, RiverRec>);

impl Process for River {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let River { name, path, width, depth, bed, bank, meander, floor, rise, from_edge, curve, .. } = self;
        let pts: Vec<Vec2> = path.iter().map(|p| cx.pos(p)).collect();
        let pts = if *curve { spline(&pts, false, 8.0) } else { pts };
        let line = densify(&pts, *meander, cx.seed);
        let bed_m = cx.mat(bed);
        let bank_m = bank.as_ref().map_or(0, |b| cx.mat(b));
        let line = if *from_edge { to_edge(&cx.f, &line) } else { line };
        let k = head(cx, name, &line, *width, *depth, bed_m, bank_m, *floor, *rise);
        let line = line[k..].to_vec();
        carve(cx, &line, *width, *depth, bed_m, *floor);
        let len = line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
        let seed = cx.seed;
        cx.state.get::<Rivers>().0.insert(name.clone(), RiverRec { line, width: *width, depth: *depth, bed: bed_m, bank: bank_m, seed, floor: *floor });
        format!("{name}: {len:.0} m long, {width} m wide")
    }
}

/// Cut a channel along a line: the bed falls steadily toward the mouth and the
/// banks slope back to the ground. This is the river as later events see it
/// (a village sits beside it, a flood breaks out of it); `settle_rivers`
/// gives it its final shape and its water after every event.
pub fn carve(cx: &mut EventCtx, line: &[Vec2], width: f32, depth: f32, bed_m: u8, floor: Option<f32>) {
    let cause = cx.cause;
    let f = &mut cx.f;
    if line.len() < 2 {
        return;
    }
    // Bed height along the line, never rising downstream.
    let mut bed: Vec<f32> = line.iter().map(|p| f.height(p.x, p.y) - depth).collect();
    for k in 1..bed.len() {
        bed[k] = bed[k].min(bed[k - 1] - 0.015);
    }
    if let Some(fl) = floor {
        let lowest = f.sea - fl;
        for b in bed.iter_mut() {
            *b = b.max(lowest);
        }
    }
    let reach = width * 3.0;
    let n = f.n;
    let mut best = vec![(f32::MAX, 0.0f32); 0];
    best.resize(n * n, (f32::MAX, 0.0));
    let mut touched = Vec::new();
    for (k, p) in line.iter().enumerate() {
        each_near(f, *p, reach, |_f: &mut Fields, i, d, _| {
            if best[i].0 == f32::MAX {
                touched.push(i);
            }
            if d < best[i].0 {
                best[i] = (d, bed[k]);
            }
        });
    }
    let half = width * 0.5;
    for i in touched {
        let (d, b) = best[i];
        let target = if d < half { b } else { b + (d - half) * 0.5 };
        if target < f.h[i] {
            f.h[i] = target;
            f.cause[i] = cause;
        }
        if d < half {
            f.water[i] = 1;
            f.surface[i] = bed_m;
        }
    }
}

/// A line carried back from its first point to the nearest edge of the world
/// and a little past it.
pub fn to_edge(f: &Fields, line: &[Vec2]) -> Vec<Vec2> {
    let Some(&p) = line.first() else {
        return line.to_vec();
    };
    let out = [Vec2::new(-6.0, p.y), Vec2::new(f.size + 6.0, p.y), Vec2::new(p.x, -6.0), Vec2::new(p.x, f.size + 6.0)];
    let far = out.into_iter().min_by(|a, b| a.distance(p).partial_cmp(&b.distance(p)).unwrap()).unwrap();
    let steps = (far.distance(p) / 3.0).ceil().max(1.0) as usize;
    let mut v: Vec<Vec2> = (0..steps).map(|s| far.lerp(p, s as f32 / steps as f32)).collect();
    v.extend_from_slice(line);
    v
}

/// The head of a river that rises on the map: its first `rise` metres cut in
/// three stretches, each wider and deeper than the last, so it starts as a
/// trickle and is a river by the end of them. Returns where along `line` the
/// full-grown river takes over (0 with no `rise`).
#[allow(clippy::too_many_arguments)]
pub fn head(cx: &mut EventCtx, name: &str, line: &[Vec2], width: f32, depth: f32, bed: u8, bank: u8, floor: Option<f32>, rise: f32) -> usize {
    if rise <= 0.0 || line.len() < 12 {
        return 0;
    }
    let total: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    let rise = rise.min(total * 0.45);
    let at = |metres: f32| -> usize {
        let mut left = metres;
        for (k, w) in line.windows(2).enumerate() {
            left -= w[0].distance(w[1]);
            if left <= 0.0 {
                return k + 1;
            }
        }
        line.len() - 1
    };
    let cuts = [0, at(rise / 3.0), at(rise * 2.0 / 3.0), at(rise)];
    let seed = cx.seed;
    for (k, (share_w, share_d)) in [(0.3f32, 0.45f32), (0.55, 0.65), (0.8, 0.85)].into_iter().enumerate() {
        let piece = &line[cuts[k]..=cuts[k + 1]];
        if piece.len() < 2 {
            continue;
        }
        let (w, d) = ((width * share_w).max(1.6), depth * share_d);
        carve(cx, piece, w, d, bed, floor);
        cx.state.get::<Rivers>().0.insert(format!("{name}~{k}"), RiverRec { line: piece.to_vec(), width: w, depth: d, bed, bank, seed: seed.wrapping_add(k as u32 + 1), floor });
    }
    cuts[3]
}

/// Points along `line` about a metre apart.
fn resample(line: &[Vec2]) -> Vec<Vec2> {
    let mut out = vec![line[0]];
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        let steps = len.ceil().max(1.0) as i32;
        for s in 1..=steps {
            out.push(w[0].lerp(w[1], s as f32 / steps as f32));
        }
    }
    out
}

/// Water depth in the middle of a river of carved depth `depth`: deep enough
/// to swim in, never a gorge full of water.
pub fn mid_depth(depth: f32) -> f32 {
    (depth * 0.8).clamp(1.4, 2.4)
}

/// After every event: later deposits and building pads can lift a river bed,
/// leaving pools and dry steps. Give each river its final course: a water
/// level that only ever falls toward the sea, a rounded bed under it, banks
/// that rise gently out of the water with a ragged waterline, and the water
/// itself (level and current) in `f.river`.
///
/// Lakes are settled first (`settle_water`): a river that reaches one ends
/// there, at the lake's level.
fn settle_rivers(f: &mut Fields, rivers: &Rivers, owner: &mut HashMap<u32, f32>) {
    // Where two rivers' margins meet, the nearer line owns the cell.
    let mut names: Vec<&String> = rivers.0.keys().collect();
    names.sort();
    for name in names {
        settle_one(f, &rivers.0[name], owner);
    }
}

/// After every event: give lakes, then rivers, their final shape and their
/// water (`f.inland`).
pub fn settle_water(f: &mut Fields, lakes: &super::lake::Lakes, rivers: &Rivers) {
    f.inland.clear();
    f.falls.clear();
    let mut owner: HashMap<u32, f32> = HashMap::default();
    let mut names: Vec<&String> = lakes.0.keys().collect();
    names.sort();
    for name in names {
        super::lake::shape(f, &lakes.0[name], &mut owner, true);
    }
    settle_rivers(f, rivers, &mut owner);
    // Where each fall lands, now the ground has its last shape, and the
    // water there pushed outward from it.
    for k in 0..f.falls.len() {
        let splash = crate::world::falls::sheet(f, &f.falls[k]).splash;
        f.falls[k].splash = splash;
        let fall = f.falls[k].clone();
        let r = fall.width * 0.8 + 4.0;
        let at = Vec2::new(splash.x, splash.z);
        if fall.foot <= f.sea + 0.02 {
            // Landing in the sea (or a pool at sea level, which the sea's own
            // surface draws): a patch of entries at sea level, drawn as foam
            // only, for the churn and the push away from the fall.
            each_near(f, at, r, |f: &mut Fields, i, _, _| {
                if f.h[i] < fall.foot {
                    f.inland.entry(i as u32).or_insert(Wet { level: fall.foot, flow: [0.0, 0.0], ebb: 1.0e4 });
                }
            });
        }
        each_near(f, at, r, |f: &mut Fields, i, d, c| {
            if let Some(w) = f.inland.get_mut(&(i as u32)) {
                if (w.level - fall.foot).abs() < 0.3 {
                    let out = (c - at).normalize_or(fall.dir) * 1.2 * (1.0 - d / r);
                    w.flow = [w.flow[0] + out.x, w.flow[1] + out.y];
                }
            }
        });
    }
}

/// Carry a river's line on past its end, a metre at a time, down to low water
/// (`low`): straight on at first, leaning downhill as the flats fall. Only a
/// river that ends at the sea: one ending in a lake, or still up on the land,
/// is left alone.
fn extend_to_low(f: &Fields, line: &mut Vec<Vec2>, low: f32, owner: &HashMap<u32, f32>) {
    let m = line.len();
    if m < 2 {
        return;
    }
    let end = line[m - 1];
    let in_lake = owner.get(&(f.idx(end.x, end.y) as u32)).is_some_and(|o| *o == f32::NEG_INFINITY);
    if in_lake || f.height(end.x, end.y) > f.sea + 1.0 {
        return;
    }
    let mut dir = (end - line[m - 2]).normalize_or(Vec2::X);
    let mut p = end;
    for _ in 0..800 {
        if f.height(p.x, p.y) < low - 0.3 {
            break;
        }
        let e = 2.0;
        let g = Vec2::new(f.height(p.x + e, p.y) - f.height(p.x - e, p.y), f.height(p.x, p.y + e) - f.height(p.x, p.y - e));
        dir = (dir * 0.85 - g.normalize_or_zero() * 0.15).normalize_or(dir);
        p += dir;
        if p.x < 1.0 || p.y < 1.0 || p.x > f.size - 1.0 || p.y > f.size - 1.0 {
            break;
        }
        line.push(p);
    }
}

/// How far past the water's edge the river's water entries reach, metres:
/// the drawn surface runs this far under the bank so the shore is wherever
/// the ground rises through it.
pub const MARGIN: f32 = 2.5;

fn settle_one(f: &mut Fields, rec: &RiverRec, owner: &mut HashMap<u32, f32>) {
    if rec.line.len() < 2 {
        return;
    }
    let mut line = resample(&rec.line);
    let sea = f.sea;
    let low = f.low_water;
    // With a tide, a river that reaches the sea runs on down to low water.
    if low < sea - 0.01 {
        extend_to_low(f, &mut line, low, owner);
    }
    let line = line;
    let m = line.len();
    let half = rec.width * 0.5;
    let deep = mid_depth(rec.depth);
    let dirs: Vec<Vec2> = (0..m).map(|k| (line[(k + 1).min(m - 1)] - line[k.saturating_sub(1)]).normalize_or_zero()).collect();

    // Lowest ground in the channel and on the banks just outside it, in a
    // band straight across the river at each station (a circle would reach
    // over a cliff edge ahead and drop the water before the lip).
    let mut bed = Vec::with_capacity(m);
    let mut rim = Vec::with_capacity(m);
    for (k, p) in line.iter().enumerate() {
        let (mut lo, mut r) = (f32::MAX, f32::MAX);
        let dir = dirs[k];
        each_near(f, *p, half + 3.5, |f: &mut Fields, i, d, c| {
            let along = (c - *p).dot(dir).abs();
            if d < half && along < 0.8 {
                lo = lo.min(f.h[i]);
            } else if d >= half + 1.5 && along < 1.5 && !f.occupied[i] {
                r = r.min(f.h[i]);
            }
        });
        let lo = if lo == f32::MAX { f.height(p.x, p.y) - rec.depth } else { lo };
        bed.push(lo);
        rim.push(if r == f32::MAX { lo + deep + 0.4 } else { r });
    }
    // The water stands `deep` over the bed, a hand below the lower bank, and
    // never rises downstream.
    let mut level: Vec<f32> = (0..m).map(|k| (bed[k] + deep).min(rim[k] - 0.35)).collect();
    for k in 1..m {
        level[k] = level[k].min(level[k - 1] - 0.004);
    }
    // A river may run out of a lake (its source), and ends where it reaches
    // a lake or where its banks drop to the sea: below that it is a channel
    // in the lake or sea floor, under their own surface. It never runs
    // higher than its source or lower than where it ends.
    let lake_at = |k: usize| -> Option<f32> {
        let i = f.idx(line[k].x, line[k].y) as u32;
        owner.get(&i).filter(|o| **o == f32::NEG_INFINITY).and_then(|_| f.inland.get(&i)).map(|w| w.level)
    };
    let source = lake_at(0);
    let start = if source.is_some() { (0..m).find(|&k| lake_at(k).is_none()).unwrap_or(m) } else { 0 };
    let ((mouth, floor), to_sea) = match (start..m).find_map(|k| lake_at(k).map(|l| (k, l))) {
        Some((k, l)) => ((k, l), false),
        None => (((start..m).find(|&k| rim[k] < sea + 0.3).unwrap_or(m), sea), true),
    };
    for (k, l) in level.iter_mut().enumerate() {
        if let Some(src) = source {
            *l = if k < start { src } else { l.min(src) };
        }
        *l = if k >= mouth { floor } else { l.max(floor) };
    }
    // Waterfalls: a drop of FALL_MIN or more over a short, steep run. The
    // water above stands level to the lip and the water below starts fresh
    // at the foot; between them it is in the air (drawn as a sheet).
    // Steep runs a few metres apart are one fall (no ledge pool between).
    let steep = |a: usize, e: usize| level[a] - level[e] >= 0.6 * (e - a) as f32;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut k = start.saturating_sub(1);
    while k + 1 < m {
        if level[k] - level[k + 1] > 0.5 {
            let mut e = k + 1;
            while e + 1 < m && level[e] - level[e + 1] > 0.25 {
                e += 1;
            }
            match runs.last_mut() {
                Some(last) if k <= last.1 + 4 && steep(last.0, e) => last.1 = e,
                _ => runs.push((k, e)),
            }
            k = e;
        } else {
            k += 1;
        }
    }
    // A fall drops FALL_MIN or more and is steep (a long steep run is rapids).
    let falls: Vec<(usize, usize)> = runs.into_iter().filter(|&(a, e)| level[a] - level[e] >= FALL_MIN && steep(a, e)).collect();
    // Stretches between falls, and the stations in the air (NONE).
    const NONE: usize = usize::MAX;
    let mut seg = vec![0usize; m];
    let (mut sid, mut fi) = (0, 0);
    for (k, s) in seg.iter_mut().enumerate() {
        if fi < falls.len() && k > falls[fi].0 {
            if k < falls[fi].1 {
                *s = NONE;
                continue;
            }
            sid += 1;
            fi += 1;
        }
        *s = sid;
    }
    let mut span = vec![(usize::MAX, 0usize); sid + 1];
    for (k, &s) in seg.iter().enumerate() {
        if s != NONE {
            span[s].0 = span[s].0.min(k);
            span[s].1 = span[s].1.max(k);
        }
    }
    // Smoothed within each stretch, so a bump in the bank is not a step.
    let w = 8usize;
    let raw = level.clone();
    for k in 0..m {
        let s = seg[k];
        if s == NONE {
            continue;
        }
        let (a, b) = (k.saturating_sub(w).max(span[s].0), (k + w).min(span[s].1));
        let smooth = raw[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
        level[k] = if k >= mouth { floor } else { smooth.min(raw[k]).max(floor) };
        if k > span[s].0 {
            level[k] = level[k].min(level[k - 1]);
        }
    }
    // Pools and rapids: the water stands level in pools and drops a
    // hand's breadth or so at a time over a short run of rapids, rather
    // than sloping evenly like a ramp. Each stretch steps down to its own
    // end (a lip, a lake, the sea).
    let step = 0.3;
    let pooled: Vec<f32> = (0..m)
        .map(|k| {
            let l = level[k];
            if seg[k] == NONE {
                return l;
            }
            let end = level[span[seg[k]].1];
            if l <= end + 0.01 { l } else { (end + ((l - end) / step).ceil() * step).min(l + step) }
        })
        .collect();
    for k in 0..m {
        let s = seg[k];
        if s == NONE {
            continue;
        }
        let (a, b) = (k.saturating_sub(2).max(span[s].0), (k + 2).min(span[s].1));
        level[k] = pooled[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
        if k > span[s].0 {
            level[k] = level[k].min(level[k - 1]);
        }
    }
    // In the air the level runs straight from lip to foot (for the current).
    for &(a, b) in &falls {
        for k in a + 1..b {
            level[k] = level[a] + (level[b] - level[a]) * (k - a) as f32 / (b - a) as f32;
        }
    }
    // The bed shallows toward a lip: the water runs out over a ledge.
    let mut deep_at = vec![deep; m];
    for &(a, _) in &falls {
        for j in 0..5usize.min(a + 1) {
            deep_at[a - j] = deep_at[a - j].min(deep * (0.25 + 0.75 * j as f32 / 5.0));
        }
    }
    // With a tide, the channel past the mouth runs on across the flats it
    // uncovers, cut a little into them and falling with them, down to low
    // water: at high water the sea covers it, and as the tide goes out the
    // river follows it down instead of stopping at the old mouth. Without a
    // tide nothing past the mouth changes.
    let tidal = to_sea && low < sea - 0.01;
    if tidal {
        for k in mouth..m {
            level[k] = floor.min(rim[k] - 0.3 + deep_at[k]);
            if k > mouth {
                level[k] = level[k].min(level[k - 1]);
            }
        }
    }
    // Current along the line: faster where the water falls faster.
    let flow: Vec<Vec2> = (0..m)
        .map(|k| {
            let (a, b) = (k.saturating_sub(2), (k + 2).min(m - 1));
            let dir = (line[b] - line[a]).normalize_or_zero();
            let fall = (level[a] - level[b]) / (b - a).max(1) as f32;
            dir * (0.3 + fall * 12.0).clamp(0.3, 1.6)
        })
        .collect();
    for &(a, b) in &falls {
        let dir = (line[(a + 2).min(m - 1)] - line[a]).normalize_or(dirs[a]);
        f.falls.push(Fall {
            lip: Vec3::new(line[a].x, level[a], line[a].y),
            dir,
            width: rec.width * 0.85,
            foot: level[b],
            speed: flow[a].length().max(0.8),
            splash: Vec3::new(line[b].x, level[b], line[b].y),
        });
    }

    // How far the tide may draw the water down at each station
    // (`Wet::ebb`): a river that runs to the sea, below its last waterfall,
    // down to a hand over its bed. A river ending in a lake, and the water
    // above a fall, keep their level.
    // Past the mouth the water stands at the sea's level and drains to a hand
    // over the channel's bed (`level` there is the bed plus `deep_at`).
    let tidal_from = falls.iter().filter(|f| f.1 <= mouth).map(|f| f.1).max().unwrap_or(0);
    let ebb = |k: usize| {
        if tidal && k >= mouth {
            (floor - level[k] + deep_at[k] - 0.1).max(0.0)
        } else if to_sea && k >= tidal_from {
            (deep_at[k] - 0.1).max(0.0)
        } else {
            0.0
        }
    };

    // Nearest station of every cell within reach.
    let reach = half + 6.0;
    let mut best: HashMap<usize, (f32, usize)> = HashMap::default();
    for (k, p) in line.iter().enumerate() {
        each_near(f, *p, reach, |_f: &mut Fields, i, d, _| {
            let e = best.entry(i).or_insert((f32::MAX, 0));
            if d < e.0 {
                *e = (d, k);
            }
        });
    }
    let seed = rec.seed;
    let mut cells: Vec<(usize, (f32, usize))> = best.into_iter().collect();
    cells.sort_unstable_by_key(|c| c.0);
    for (i, (d, k)) in cells {
        // A lake's own cells are the lake's.
        if owner.get(&(i as u32)).is_some_and(|o| *o == f32::NEG_INFINITY) {
            continue;
        }
        if k >= mouth && floor > sea {
            // Past the mouth, in the lake: leave its bed alone.
            continue;
        }
        if seg[k] == NONE {
            // Under a waterfall: the rock face stays as it is.
            continue;
        }
        let p = f.centre(i);
        // A ragged edge: the river is a little wider or narrower from metre to metre.
        let d = d + perlin2(p.x * 0.11, p.y * 0.11, seed) * 0.9 + perlin2(p.x * 0.4, p.y * 0.4, seed ^ 0x51) * 0.25;
        let l = level[k];
        let s = d / half;
        let occupied = f.occupied[i];
        if s < 1.0 {
            // A rounded bed, shallowing to a few centimetres at the edge.
            let t = l - 0.06 - (deep_at[k] - 0.06) * (1.0 - s * s).max(0.0).powf(0.7);
            // Past the edge of a lip the bed is never built up (it would be a
            // shelf out over the drop) and the rock keeps its own colour (a
            // tower's rim overhangs the h field's edge).
            let beyond = falls.iter().any(|&(a, _)| a == k) && (p - line[k]).dot(dirs[k]) > 0.3;
            f.h[i] = if beyond { f.h[i].min(t) } else { t };
            f.water[i] = 1;
            let edge = 0.72 + 0.14 * perlin2(p.x * 0.3, p.y * 0.3, seed ^ 0x77);
            if !beyond {
                f.surface[i] = if rec.bank != 0 && s > edge { rec.bank } else { rec.bed };
            }
        } else {
            let out = d - half;
            let bank = l - 0.06 + out * 0.5;
            if !occupied {
                if f.h[i] > bank {
                    f.h[i] = bank;
                } else if out < MARGIN && l > sea + 0.02 {
                    // A low lip where the land beside the river lies below its
                    // water, so the water never shows past the bank.
                    f.h[i] = f.h[i].max(bank.min(l + 0.15));
                }
            }
            if f.water[i] == 1 {
                f.water[i] = 0;
            }
            let fringe = 0.25 + 0.9 * (perlin2(p.x * 0.17, p.y * 0.17, seed ^ 0x33) * 0.5 + 0.5);
            if rec.bank != 0 && out < fringe && !occupied {
                f.surface[i] = rec.bank;
            } else if f.surface[i] == rec.bed || (rec.bank != 0 && f.surface[i] == rec.bank) {
                f.surface[i] = 0;
            }
        }
        if d < half + MARGIN && l > sea + 0.02 && k < mouth {
            let key = i as u32;
            if owner.get(&key).is_none_or(|&o| d < o) {
                owner.insert(key, d);
                let v = if s < 1.0 { flow[k] * (1.0 - 0.6 * s * s) } else { Vec2::ZERO };
                f.inland.insert(key, Wet { level: l, flow: [v.x, v.y], ebb: ebb(k) });
            }
        } else if tidal && k >= mouth && d < half + MARGIN && rim[k] > low {
            // The channel on past the mouth, down to low water. At high water
            // the sea covers it and these entries, at the sea's level, draw
            // nothing (rivers.rs marks them); as the tide goes out they drain
            // to a hand over the bed and the river runs on down them.
            let key = i as u32;
            if owner.get(&key).is_none_or(|&o| d < o) {
                owner.insert(key, d);
                f.inland.insert(key, Wet { level: floor, flow: [0.0, 0.0], ebb: ebb(k) });
            }
        }
    }
}
