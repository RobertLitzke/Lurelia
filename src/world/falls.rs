//! Waterfalls: where a river's water drops over a cliff. `settle_water`
//! (chronicle/events/river.rs) finds them and records each as a `Fall`;
//! `sheet` works out the shape of the falling water against the rock under
//! the lip. The renderer draws the sheet and throws spray and mist up where
//! it lands (world/rivers.rs).

use super::fields::Fields;
use glam::{Vec2, Vec3};

/// A river's drop of FALL_MIN or more over a short run.
#[derive(Clone, Debug)]
pub struct Fall {
    /// Middle of the lip's edge, at the water surface above it.
    pub lip: Vec3,
    /// Which way the water leaves the lip (x, z), unit length.
    pub dir: Vec2,
    /// Width of the falling water, metres.
    pub width: f32,
    /// The water surface it falls into: a pool, a lake or the sea.
    pub foot: f32,
    /// Speed over the lip, m/s.
    pub speed: f32,
    /// Where the middle of the sheet meets the water below (set by
    /// `settle_water` once the ground has its final shape).
    pub splash: Vec3,
}

/// The least drop that makes a waterfall rather than rapids, metres.
pub const FALL_MIN: f32 = 2.5;

const G: f32 = 9.8;
/// Rows of the sheet are this far apart in drop, metres.
const ROW: f32 = 0.5;
/// Columns across the sheet are this far apart, metres.
const COL: f32 = 0.5;
/// The sheet keeps this far off the rock behind it, metres.
const CLEAR: f32 = 0.3;

/// The falling water as a grid of points: `cols` columns across the lip,
/// each with the same number of rows down to the water below. A point is
/// its position and the seconds the water took to fall there from the lip
/// (what the streaks in it scroll by).
pub struct Sheet {
    /// Each column's offset across the lip (metres from the middle) and its
    /// points, top first.
    pub cols: Vec<(f32, Vec<(Vec3, f32)>)>,
    pub splash: Vec3,
}

/// The sheet of a fall. Water leaves the lip at `speed` and falls freely,
/// except that it keeps off the rock: where the cliff leans out, the sheet
/// slides down its face a hand's breadth off it, and under a bulge it may
/// pass into the rock (hidden there) rather than step out across it.
pub fn sheet(f: &Fields, fall: &Fall) -> Sheet {
    let dir = fall.dir;
    let across = Vec2::new(-dir.y, dir.x);
    let edge = Vec2::new(fall.lip.x, fall.lip.z) + dir * 0.5;
    let top = fall.lip.y - 0.03;
    let drop = (top - fall.foot).max(0.5);
    let rows = (drop / ROW).ceil() as usize + 1;
    let n = ((fall.width / COL).round() as usize).max(2);
    let mut cols = Vec::with_capacity(n + 1);
    for c in 0..=n {
        let u = (c as f32 / n as f32 - 0.5) * fall.width;
        let start = edge + across * u;
        // A row back over the river, so the sheet meets its surface; then the
        // water runs flat over any ledge left before the rock drops away
        // (the river's level may end short of the real edge).
        let mut e0 = 0.0;
        while e0 < 20.0 && f.height(start.x + dir.x * e0, start.y + dir.y * e0) > top - 1.5 {
            e0 += 0.25;
        }
        let base = start + dir * e0;
        let mut pts = vec![(Vec3::new(start.x - dir.x * 0.6, top, start.y - dir.y * 0.6), 0.0), (Vec3::new(base.x, top, base.y), 0.0)];
        let mut s_prev = 0.0f32;
        let mut landed: Option<(Vec3, f32)> = None;
        for r in 0..rows {
            if let Some(p) = landed {
                pts.push(p);
                continue;
            }
            let d = (r as f32 * ROW).min(drop + 0.1);
            let y = top - d;
            let secs = (2.0 * d / G).sqrt();
            // Out past any rock standing above this height, scanning from
            // the lip, and never back toward the cliff.
            let mut s = 0.0;
            while s < 25.0 && f.height(base.x + dir.x * s, base.y + dir.y * s) > y - 0.15 {
                s += 0.25;
            }
            let rock = if s > 0.0 { s + CLEAR } else { 0.0 };
            // Over a bulge below the lip the water slides down the rock rather
            // than jumping clear of it in one row (a flat plate in the air).
            let s = (fall.speed * secs).max(rock).max(s_prev).min(s_prev + 1.5);
            s_prev = s;
            let at = base + dir * s;
            let ground = f.height(at.x, at.y);
            let floor = fall.foot.max(ground);
            if y <= floor + 0.05 || r + 1 == rows {
                let p = (Vec3::new(at.x, floor - 0.1, at.y), secs);
                pts.push(p);
                landed = Some(p);
            } else {
                pts.push((Vec3::new(at.x, y, at.y), secs));
            }
        }
        cols.push((u, pts));
    }
    let mid = &cols[cols.len() / 2].1;
    let splash = mid.last().map_or(fall.lip, |p| p.0);
    Sheet { cols, splash }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 m cliff along x = 50: the sheet keeps off it and lands on the
    /// water at its foot.
    #[test]
    fn sheet_clears_the_cliff() {
        let mut f = Fields::new(128.0, 0.0);
        let n = f.n;
        for i in 0..n * n {
            let x = f.centre(i).x;
            f.h[i] = if x < 50.0 { 30.0 } else { 8.0 };
        }
        let fall = Fall {
            lip: Vec3::new(49.0, 30.5, 64.0),
            dir: Vec2::X,
            width: 4.0,
            foot: 10.0,
            speed: 1.2,
            splash: Vec3::ZERO,
        };
        let s = sheet(&f, &fall);
        for (_, pts) in &s.cols {
            for (p, _) in pts.iter().skip(1) {
                assert!(p.y >= f.height(p.x, p.z) - 0.2, "sheet in the rock at {p}");
            }
            assert!((pts.last().unwrap().0.y - 9.9).abs() < 0.01);
        }
        assert!(s.splash.x > 50.0 && s.splash.x < 54.0, "splash at {}", s.splash);
    }
}
