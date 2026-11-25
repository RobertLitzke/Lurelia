//! River water as the renderer draws it: one quad per metre cell of
//! `Fields::inland` (the channel and a margin of bank), each corner at the
//! mean water level, bed height and current of the cells that share it. The
//! surface runs on under the banks, so the shore is wherever the ground rises
//! through it, the same way the sea plane meets the land.
//!
//! Waterfalls (`Fields::falls`) go in the same buffer after the surface, in
//! the same 96-byte record read another way by the shader: the quads of each
//! falling sheet, then a short face of pouring water wherever the surface
//! steps down or would otherwise show an open side (`surface`). Where each
//! fall lands is a `Spray` of its own, for the spray it throws up and the
//! pool it churns.

use super::falls::{joins, past_lip, sheet, Fall, HOLD};
use super::fields::Fields;
use crate::render::{RiverQuad, Spray};
use glam::Vec2;
use std::collections::HashSet;

/// How far round a splash the pool is churned white, metres (fs_water in
/// scene.wgsl works it out the same way from the spray record).
fn churn_reach(fall: &Fall) -> f32 {
    fall.width * 0.6 + 2.5
}

/// Everything the renderer draws of the rivers: the surface, the falls'
/// sheets and steps, and where each fall lands. A surface cell near the foot
/// of a fall points at that fall's `Spray` (its index + 1, in `at.w`), so the
/// water there is churned white by distance from the splash. Cells at sea
/// level (round a fall into the sea) have `at.z` = 1: the sea's own surface
/// is under them, so they draw only the foam.
pub fn buffers(f: &Fields) -> (Vec<RiverQuad>, Vec<RiverQuad>, Vec<Spray>) {
    let (cells, steps) = surface(f);
    let mut sheets = fall_quads(f);
    sheets.extend(steps);
    (cells, sheets, spray_spots(f))
}

/// The surface cells, and the faces of water pouring over each step in the
/// surface and over each side where nothing holds the water in.
fn surface(f: &Fields) -> (Vec<RiverQuad>, Vec<RiverQuad>) {
    let n = f.n as i32;
    let level = |i: u32| f.inland[&i].level;
    let at_sea = |i: u32| level(i) <= f.sea + 0.02;
    let chan = |i: u32| f.water[i as usize] == 1;
    let mut drawn: HashSet<u32> = f.inland.iter().filter(|(i, w)| past_lip(&f.falls, f.centre(**i as usize), w.level) != Some(true)).map(|(i, _)| *i).collect();
    // The cell across side k of cell i (-x, +x, -z, +z); None past the map.
    let side = |i: u32, k: usize| -> Option<u32> {
        let (x, z) = ((i as i32) % n + [-1, 1, 0, 0][k], (i as i32) / n + [0, 0, -1, 1][k]);
        (x >= 0 && z >= 0 && x < n && z < n).then_some((z * n + x) as u32)
    };
    // Is cell i's water held on side k: by water of its own surface, or by
    // ground standing up to it?
    let held = |drawn: &HashSet<u32>, i: u32, k: usize| match side(i, k) {
        None => true,
        Some(j) => (drawn.contains(&j) && joins(level(j), level(i), chan(i) && chan(j))) || f.h[j as usize] >= level(i) - HOLD,
    };
    // Water standing over its own ground. Water under it is hidden, and so
    // are its sides.
    let shows = |i: u32| f.h[i as usize] < level(i) - 0.05;
    // Water beside a channel (the margin that runs on under the banks) that
    // lies out over lower ground would hang in the air with its side open:
    // where the ground beside a river falls away (down a cascade, past a
    // lip) it is left out, and the channel pours over that side instead.
    // Water under its own ground beside such a drop is left out too: the
    // smoothed ground rounds off the edge of the drop and would show the
    // edge of the water as a thin line.
    for _ in 0..12 {
        let gone: Vec<u32> = drawn
            .iter()
            .copied()
            .filter(|&i| (f.water[i as usize] == 0 || !shows(i)) && !at_sea(i) && (0..4).any(|k| !held(&drawn, i, k)))
            .collect();
        if gone.is_empty() {
            break;
        }
        for i in gone {
            drawn.remove(&i);
        }
    }
    // (level, bed, flow x, flow z, ebb) at the corner (x, z) of the cell grid
    // for the water of level `own` there. The drawn cells round the corner
    // fall into groups, each level joined to the next (`joins`), and a cell
    // takes its own group's mean: every cell of a group puts the corner in the same
    // place, so the surface has no cracks, and groups either side of a step
    // keep their own heights.
    let corner = |x: i32, z: i32, own: f32| -> [f32; 5] {
        let mut wet: Vec<(f32, f32, f32, f32, bool)> = Vec::with_capacity(4);
        let (mut bed, mut cells) = (0.0, 0.0f32);
        for (dx, dz) in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
            let (cx, cz) = (x + dx, z + dz);
            if cx < 0 || cz < 0 || cx >= n || cz >= n {
                continue;
            }
            let i = (cz * n + cx) as usize;
            bed += f.h[i];
            cells += 1.0;
            if drawn.contains(&(i as u32)) {
                let w = &f.inland[&(i as u32)];
                wet.push((w.level, w.flow[0], w.flow[1], w.ebb, chan(i as u32)));
            }
        }
        wet.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut lo, mut hi) = (0, 0);
        for k in 0..wet.len() {
            if k > 0 && !joins(wet[k].0, wet[k - 1].0, wet[k].4 && wet[k - 1].4) {
                if wet[k].0 > own + 1e-4 {
                    break;
                }
                lo = k;
            }
            hi = k + 1;
        }
        let bed = bed / cells.max(1.0);
        let group = &wet[lo..hi.max(lo)];
        if group.is_empty() {
            return [own, bed, 0.0, 0.0, 0.0];
        }
        let m = group.len() as f32;
        let sum = group.iter().fold([0.0f32; 4], |s, w| [s[0] + w.0, s[1] + w.1, s[2] + w.2, s[3] + w.3]);
        [sum[0] / m, bed, sum[1] / m, sum[2] / m, sum[3] / m]
    };
    let corners = |i: u32| {
        let (x, z) = ((i as i32) % n, (i as i32) / n);
        let own = level(i);
        [corner(x, z, own), corner(x + 1, z, own), corner(x + 1, z + 1, own), corner(x, z + 1, own)]
    };
    let mut keys: Vec<u32> = drawn.iter().copied().collect();
    keys.sort_unstable();
    let cell = super::fields::CELL;
    let mut quads = Vec::with_capacity(keys.len());
    let mut steps = Vec::new();
    for &i in &keys {
        let (x, z) = ((i as i32) % n, (i as i32) / n);
        let own = level(i);
        let c = corners(i);
        // The fall whose foot churns this cell, by its index in f.falls + 1.
        let p = f.centre(i as usize);
        let churn = f
            .falls
            .iter()
            .position(|fl| (own - fl.foot).abs() < 0.3 && p.distance(Vec2::new(fl.splash.x, fl.splash.z)) < churn_reach(fl) + 1.5)
            .map_or(0.0, |k| k as f32 + 1.0);
        quads.push(RiverQuad {
            at: [x as f32 * cell, z as f32 * cell, if at_sea(i) { 1.0 } else { 0.0 }, churn],
            level: c.map(|c| c[0]),
            bed: c.map(|c| c[1]),
            flow_x: c.map(|c| c[2]),
            flow_z: c.map(|c| c[3]),
            ebb: c.map(|c| c[4]),
        });
        if at_sea(i) || !shows(i) {
            continue;
        }
        // Each side where this water is not held pours over its edge, down to
        // the water or the ground beyond.
        for k in 0..4 {
            let Some(j) = side(i, k) else { continue };
            // The edge's two corners, by their index in `corners` of this
            // cell and of the one across it.
            let (e0, e1) = [(0, 3), (1, 2), (0, 1), (3, 2)][k];
            let (o0, o1) = [(1, 2), (0, 3), (3, 2), (0, 1)][k];
            let top = [c[e0][0], c[e1][0]];
            let ground = f.h[j as usize];
            let bottom = if drawn.contains(&j) {
                let lj = level(j);
                if lj > own || (lj == own && j > i) {
                    // The higher water draws the step.
                    continue;
                }
                let cj = corners(j);
                [cj[o0][0].max(ground), cj[o1][0].max(ground)]
            } else {
                [ground; 2]
            };
            if (top[0] - bottom[0]).max(top[1] - bottom[1]) < HOLD {
                continue;
            }
            // Over a fall's lip its own sheet is the falling water.
            if past_lip(&f.falls, f.centre(j as usize), own) != Some(true) {
                steps.push(step_face(i, k, n, top, bottom));
            }
        }
    }
    (quads, steps)
}

/// Water pouring over side `k` of cell `i`, from `top` at the edge's two
/// corners down to `bottom`, as a record like the sheets' (`fall_quads`):
/// the offset across is the distance along the edge over 2.5 m, and
/// `ebb[0]` = 1 marks a step, whose ends are not ragged.
fn step_face(i: u32, k: usize, n: i32, top: [f32; 2], bottom: [f32; 2]) -> RiverQuad {
    let cell = super::fields::CELL;
    let (x, z) = (((i as i32) % n) as f32 * cell, ((i as i32) / n) as f32 * cell);
    // The edge's ends, a little out from the rock face under it.
    let off = 0.04;
    let (a, b) = match k {
        0 => (Vec2::new(x - off, z), Vec2::new(x - off, z + cell)),
        1 => (Vec2::new(x + cell + off, z), Vec2::new(x + cell + off, z + cell)),
        2 => (Vec2::new(x, z - off), Vec2::new(x + cell, z - off)),
        _ => (Vec2::new(x, z + cell + off), Vec2::new(x + cell, z + cell + off)),
    };
    let secs = |d: f32| (2.0 * d.max(0.0) / 9.8).sqrt();
    let (ua, ub) = if k < 2 { (a.y, b.y) } else { (a.x, b.x) };
    // Down a little into the water below, so the two meet.
    let lo = [bottom[0].min(top[0]) - 0.08, bottom[1].min(top[1]) - 0.08];
    RiverQuad {
        at: [a.x, top[0] - 0.03, a.y, 0.0],
        level: [b.x, top[1] - 0.03, b.y, 0.0],
        bed: [b.x, lo[1], b.y, secs(top[1] - lo[1])],
        flow_x: [a.x, lo[0], a.y, secs(top[0] - lo[0])],
        flow_z: [ua / 2.5, ub / 2.5, ub / 2.5, ua / 2.5],
        ebb: [1.0, 0.0, 0.0, 0.0],
    }
}

/// The falling sheets as quads: each record holds the four corners as
/// (x, y, z, seconds since the lip) in `at`, `level`, `bed` and `flow_x`,
/// and in `flow_z` each corner's offset across the sheet over its half
/// width (-1 to 1).
fn fall_quads(f: &Fields) -> Vec<RiverQuad> {
    let mut out = Vec::new();
    for fall in &f.falls {
        let s = sheet(f, fall);
        let half = fall.width * 0.5;
        for w in s.cols.windows(2) {
            let ((u0, a), (u1, b)) = (&w[0], &w[1]);
            for r in 0..a.len().min(b.len()) - 1 {
                let v = |p: &(glam::Vec3, f32)| [p.0.x, p.0.y, p.0.z, p.1];
                out.push(RiverQuad {
                    at: v(&a[r]),
                    level: v(&b[r]),
                    bed: v(&b[r + 1]),
                    flow_x: v(&a[r + 1]),
                    flow_z: [u0 / half, u1 / half, u1 / half, u0 / half],
                    ebb: [0.0; 4],
                });
            }
        }
    }
    out
}

/// Where each fall lands (`Spray`), in the order of `Fields::falls`.
fn spray_spots(f: &Fields) -> Vec<Spray> {
    f.falls
        .iter()
        .map(|fall| {
            let drop = fall.lip.y - fall.foot;
            Spray {
                at: [fall.splash.x, fall.splash.y, fall.splash.z, fall.width],
                dir: [fall.dir.x, fall.dir.y, drop, (drop / 15.0).clamp(0.3, 1.5)],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::falls::{joins, CHUTE};
    use crate::world::fields::Wet;

    fn wet(f: &mut Fields, x: usize, z: usize, level: f32, ground: f32, channel: bool) {
        let i = z * f.n + x;
        f.h[i] = ground;
        f.water[i] = channel as u8;
        f.inland.insert(i as u32, Wet { level, flow: [0.0; 2], ebb: 0.0 });
    }

    /// A channel whose cells stand 0.5 and 1 m apart in a pattern: every two
    /// cells that are one surface put the corner they share at the same
    /// height, so the surface has no cracks.
    #[test]
    fn joined_cells_share_their_corners() {
        let mut f = Fields::new(32.0, 0.0);
        f.h.iter_mut().for_each(|h| *h = 20.0);
        let lv = |x: usize, z: usize| 10.0 + 0.5 * ((x + 2 * z) % 3) as f32;
        for z in 4..10 {
            for x in 4..12 {
                wet(&mut f, x, z, lv(x, z), 9.0, true);
            }
        }
        let (quads, _) = surface(&f);
        let mut at: std::collections::HashMap<(i32, i32), Vec<(f32, f32)>> = Default::default();
        for q in &quads {
            let (x, z) = (q.at[0] as i32, q.at[1] as i32);
            let own = lv(x as usize, z as usize);
            for (c, (dx, dz)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                at.entry((x + dx, z + dz)).or_default().push((own, q.level[c]));
            }
        }
        for (p, v) in at {
            for a in &v {
                for b in &v {
                    if joins(a.0, b.0, true) {
                        assert!((a.1 - b.1).abs() < 1e-4, "crack at {p:?}: {v:?}");
                    }
                }
            }
        }
    }

    /// Two pools in one channel, the lower more than a chute below the
    /// upper: the step between them is drawn as water pouring over it, once
    /// per cell along the edge, from the upper pool's surface down into the
    /// lower one.
    #[test]
    fn a_step_between_pools_pours() {
        let mut f = Fields::new(32.0, 0.0);
        f.h.iter_mut().for_each(|h| *h = 20.0);
        let drop = CHUTE + 0.5;
        for z in 4..8 {
            for x in 4..12 {
                let (level, ground) = if x < 8 { (10.0, 8.0) } else { (10.0 - drop, 7.0) };
                wet(&mut f, x, z, level, ground, true);
            }
        }
        let (_, steps) = surface(&f);
        assert_eq!(steps.len(), 4, "one face per row");
        for s in &steps {
            assert!((s.at[0] - 8.04).abs() < 1e-3 && s.ebb[0] == 1.0);
            assert!((s.at[1] - 9.97).abs() < 1e-3, "top at the upper surface: {}", s.at[1]);
            assert!(s.bed[1] < 10.0 - drop, "bottom into the lower pool: {}", s.bed[1]);
        }
    }
}
