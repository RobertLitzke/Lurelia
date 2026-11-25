//! A plan drawn flat, one pixel a metre: districts tinted, streets grey,
//! buildings red, walls brown. For tuning the generator without rendering
//! the world (`VG_TOWN_SKETCH=dir` writes one per `town` event).

use super::graph::seg_foot;
use super::{rank, Edge, Ground, Plan, Style};
use glam::Vec2;

pub fn write(plan: &Plan, ground: &dyn Ground, reach: f32, path: &std::path::Path) -> std::io::Result<()> {
    let c = plan.districts.c;
    let n = (reach * 2.0) as usize;
    let lo = c - Vec2::splat(reach);
    let mut px = vec![0u8; n * n * 4];
    let put = |px: &mut Vec<u8>, x: i64, z: i64, col: [u8; 3]| {
        if x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n {
            let i = (z as usize * n + x as usize) * 4;
            px[i..i + 3].copy_from_slice(&col);
            px[i + 3] = 255;
        }
    };
    for z in 0..n {
        for x in 0..n {
            let p = lo + Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let col = if ground.wet(p) {
                [90, 140, 200]
            } else {
                match plan.districts.at(p).map(|d| plan.districts.style(d)) {
                    Some(Style::Maze) => [232, 214, 190],
                    Some(Style::Radial) => [226, 206, 226],
                    Some(Style::Grid) => [214, 222, 236],
                    Some(Style::Suburb) => [214, 236, 206],
                    Some(Style::Village) => [236, 232, 200],
                    Some(Style::Ribbon) => [236, 226, 210],
                    None => [196, 214, 170],
                }
            };
            put(&mut px, x as i64, z as i64, col);
        }
    }
    let fill_disc = |px: &mut Vec<u8>, p: Vec2, r: f32, col: [u8; 3]| {
        let q = p - lo;
        for dz in -(r as i64)..=(r as i64) {
            for dx in -(r as i64)..=(r as i64) {
                if ((dx * dx + dz * dz) as f32) <= r * r {
                    put(px, q.x as i64 + dx, q.y as i64 + dz, col);
                }
            }
        }
    };
    for &(p, r) in &plan.parks {
        fill_disc(&mut px, p, r, [150, 200, 130]);
    }
    for sq in &plan.plazas {
        let reach = sq.r.max(sq.w) + 1.0;
        for dz in -(reach as i64)..=(reach as i64) {
            for dx in -(reach as i64)..=(reach as i64) {
                let p = sq.c + Vec2::new(dx as f32, dz as f32);
                if sq.contains(p, 0.0) {
                    let q = p - lo;
                    put(&mut px, q.x as i64, q.y as i64, [200, 190, 170]);
                }
            }
        }
    }
    for &(a, b) in &plan.piers {
        let n = (a.distance(b) / 0.5) as usize;
        for k in 0..=n {
            let q = a.lerp(b, k as f32 / n.max(1) as f32) - lo;
            for d in -2..=2 {
                put(&mut px, q.x as i64 + d, q.y as i64, [120, 90, 60]);
            }
        }
    }
    let line = |px: &mut Vec<u8>, a: Vec2, b: Vec2, r: f32, col: [u8; 3]| {
        let (l, h) = (a.min(b) - Vec2::splat(r + 1.0) - lo, a.max(b) + Vec2::splat(r + 1.0) - lo);
        for z in l.y.max(0.0) as i64..h.y.min(n as f32) as i64 {
            for x in l.x.max(0.0) as i64..h.x.min(n as f32) as i64 {
                let p = lo + Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
                if seg_foot(a, b, p).0 <= r {
                    put(px, x, z, col);
                }
            }
        }
    };
    for w in plan.ways.iter().filter(|w| w.drawn) {
        for s in w.pts.windows(2) {
            line(&mut px, s[0], s[1], w.half + w.walk, [200, 200, 200]);
        }
    }
    for w in plan.ways.iter().filter(|w| w.drawn) {
        let col = match w.rank {
            rank::MAIN => [70, 70, 80],
            rank::RING => [90, 90, 110],
            rank::COUNTRY => [150, 130, 100],
            rank::QUAY => [80, 110, 140],
            _ => [120, 120, 120],
        };
        for s in w.pts.windows(2) {
            line(&mut px, s[0], s[1], w.half, col);
        }
    }
    for b in &plan.buildings {
        let poly = &b.poly;
        let (l, h) = poly.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(l, h), &p| (l.min(p), h.max(p)));
        for z in (l.y - lo.y) as i64..=(h.y - lo.y) as i64 {
            for x in (l.x - lo.x) as i64..=(h.x - lo.x) as i64 {
                let p = lo + Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
                if super::extent::in_poly(poly, p) {
                    put(&mut px, x, z, [190, 70, 60]);
                }
            }
        }
        for k in 0..poly.len() {
            let (a, bb) = (poly[k], poly[(k + 1) % poly.len()]);
            line(&mut px, a, bb, 0.4, if k == b.front { [255, 230, 80] } else { [110, 40, 35] });
        }
    }
    for p in &plan.paths {
        for s in p.windows(2) {
            line(&mut px, s[0], s[1], 0.7, [175, 140, 95]);
        }
    }
    // Library structures: their turned footprints in purple.
    for s in &plan.sites {
        let h = s.size * 0.5;
        for z in (s.at.y - h.y - lo.y) as i64..(s.at.y + h.y - lo.y) as i64 {
            for x in (s.at.x - h.x - lo.x) as i64..(s.at.x + h.x - lo.x) as i64 {
                put(&mut px, x, z, [120, 60, 150]);
            }
        }
    }
    for b in &plan.boundaries {
        let col = match b.edge {
            Edge::TownWall => [60, 50, 40],
            Edge::FieldWall => [130, 120, 105],
            Edge::Hedge => [60, 100, 50],
            _ => [140, 95, 50],
        };
        for s in b.pts.windows(2) {
            line(&mut px, s[0], s[1], if b.edge == Edge::TownWall { 1.2 } else { 0.5 }, col);
        }
    }
    for &(p, _) in &plan.trees {
        fill_disc(&mut px, p, 1.5, [40, 120, 50]);
    }
    crate::util::save_png(path, n as u32, n as u32, &px)
}
