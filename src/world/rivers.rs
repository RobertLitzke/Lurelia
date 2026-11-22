//! River water as the renderer draws it: one quad per metre cell of
//! `Fields::inland` (the channel and a margin of bank), each corner at the
//! mean water level, bed height and current of the cells that share it. The
//! surface runs on under the banks, so the shore is wherever the ground rises
//! through it, the same way the sea plane meets the land.
//!
//! Waterfalls (`Fields::falls`) go in the same buffer after the surface, in
//! the same 96-byte record read another way by the shader: the quads of each
//! falling sheet. Where each one lands is a `Spray` of its own, for the spray
//! it throws up and the pool it churns.

use super::falls::{sheet, Fall};
use super::fields::Fields;
use crate::render::{RiverQuad, Spray};

/// How far round a splash the pool is churned white, metres (fs_water in
/// scene.wgsl works it out the same way from the spray record).
fn churn_reach(fall: &Fall) -> f32 {
    fall.width * 0.6 + 2.5
}

/// Everything the renderer draws of the rivers: the surface, the falls'
/// sheets, and where each fall lands. A surface cell near the foot of a fall
/// points at that fall's `Spray` (its index + 1, in `at.w`), so the water
/// there is churned white by distance from the splash. Cells at sea level
/// (round a fall into the sea) have `at.z` = 1: the sea's own surface is
/// under them, so they draw only the foam.
pub fn buffers(f: &Fields) -> (Vec<RiverQuad>, Vec<RiverQuad>, Vec<Spray>) {
    (surface(f), fall_quads(f), spray_spots(f))
}

fn surface(f: &Fields) -> Vec<RiverQuad> {
    let n = f.n as i32;
    // (level, bed, flow x, flow z, ebb) at the corner (x, z) of the cell grid,
    // from the cells round it whose water is near `own` (never averaged
    // across a waterfall's drop).
    let corner = |x: i32, z: i32, own: f32, own_ebb: f32| -> [f32; 5] {
        let (mut lv, mut fx, mut fz, mut wet, mut eb) = (0.0, 0.0, 0.0, 0.0f32, 0.0f32);
        let (mut bed, mut cells) = (0.0, 0.0f32);
        for (dx, dz) in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
            let (cx, cz) = (x + dx, z + dz);
            if cx < 0 || cz < 0 || cx >= n || cz >= n {
                continue;
            }
            let i = (cz * n + cx) as usize;
            bed += f.h[i];
            cells += 1.0;
            if let Some(w) = f.inland.get(&(i as u32)) {
                if (w.level - own).abs() < 0.6 {
                    lv += w.level;
                    fx += w.flow[0];
                    fz += w.flow[1];
                    eb += w.ebb;
                    wet += 1.0;
                }
            }
        }
        if wet == 0.0 {
            return [own, bed / cells.max(1.0), 0.0, 0.0, own_ebb];
        }
        [lv / wet, bed / cells.max(1.0), fx / wet, fz / wet, eb / wet]
    };
    let mut keys: Vec<u32> = f.inland.keys().copied().collect();
    keys.sort_unstable();
    // Water at a fall's upper level past its lip's edge would hang in the
    // air over the drop.
    let past_lip = |i: u32, level: f32| {
        let p = f.centre(i as usize);
        f.falls.iter().any(|fl| {
            let d = p - glam::Vec2::new(fl.lip.x, fl.lip.z);
            (level - fl.lip.y).abs() < 0.3 && d.dot(fl.dir) > 0.7 && d.perp_dot(fl.dir).abs() < fl.width * 0.6 + 3.0
        })
    };
    keys.retain(|i| !past_lip(*i, f.inland[i].level));
    keys.iter()
        .map(|&i| {
            let (x, z) = ((i as i32) % n, (i as i32) / n);
            let (own, own_ebb) = (f.inland[&i].level, f.inland[&i].ebb);
            let c = [corner(x, z, own, own_ebb), corner(x + 1, z, own, own_ebb), corner(x + 1, z + 1, own, own_ebb), corner(x, z + 1, own, own_ebb)];
            let cell = super::fields::CELL;
            // The fall whose foot churns this cell, by its index in f.falls + 1.
            let p = f.centre(i as usize);
            let churn = f
                .falls
                .iter()
                .position(|fl| (own - fl.foot).abs() < 0.3 && p.distance(glam::Vec2::new(fl.splash.x, fl.splash.z)) < churn_reach(fl) + 1.5)
                .map_or(0.0, |k| k as f32 + 1.0);
            RiverQuad {
                at: [x as f32 * cell, z as f32 * cell, if own <= f.sea + 0.02 { 1.0 } else { 0.0 }, churn],
                level: c.map(|c| c[0]),
                bed: c.map(|c| c[1]),
                flow_x: c.map(|c| c[2]),
                flow_z: c.map(|c| c[3]),
                ebb: c.map(|c| c[4]),
            }
        })
        .collect()
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
