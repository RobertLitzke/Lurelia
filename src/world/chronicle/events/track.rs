use crate::world::chronicle::geom::{grade_strip, smoothed_ground, spline, Strip};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use glam::Vec2;
use serde::Deserialize;

/// A closed dirt-bike loop through `path`: graded track, berms banked on the
/// outside of turns, and jumps built on the straights by their length
/// (rollers, a double, a tabletop). `scale` multiplies the feature heights.
#[derive(Deserialize, Debug)]
pub struct Track {
    path: Vec<Pos>,
    width: f32,
    mat: String,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn one() -> f32 {
    1.0
}

impl Process for Track {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Track { path, width, mat, scale, poi } = self;
        let pts: Vec<Vec2> = path.iter().map(|p| cx.pos(p)).collect();
        let line = spline(&pts, true, 1.0);
        let m = cx.mat(mat);
        let base = smoothed_ground(&cx.f, &line, 30.0, true);
        let (off, bank, built) = track_features(&line, *scale);
        grade_strip(cx, &line, &base, &off, &bank, &Strip { width: *width, mat: m, shoulder: m, margin: 4.0 });
        if let Some(name) = poi {
            cx.pois.push(Poi { name: name.clone(), pos: line[0], cause: cx.cause });
        }
        format!("track {} m loop: {built}", line.len())
    }
}

/// Heights and banking for a dirt-bike loop: turns get berms, straights get
/// jumps sized to their length. Returns (height offset, bank per metre across,
/// what was built).
fn track_features(line: &[Vec2], scale: f32) -> (Vec<f32>, Vec<f32>, String) {
    let n = line.len();
    let mut off = vec![0.0f32; n];
    let mut bank = vec![0.0f32; n];
    // Signed turn rate (radians per metre, positive = turning left) over 6 m.
    let turn: Vec<f32> = (0..n)
        .map(|k| {
            let a = line[(k + n - 3) % n];
            let b = line[k];
            let c = line[(k + 3) % n];
            let (d1, d2) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
            // x east, z south: a left turn is a negative cross product.
            let ang = d1.perp_dot(d2).atan2(d1.dot(d2));
            -ang / ((a.distance(b) + b.distance(c)) * 0.5).max(0.5)
        })
        .collect();
    let turning: Vec<bool> = (0..n).map(|k| (0..9).any(|j| turn[(k + n + j - 4) % n].abs() > 0.035)).collect();
    // Berms: the outside of each turn rises.
    for k in 0..n {
        if turning[k] {
            let mut t = 0.0f32;
            for j in 0..11 {
                t += turn[(k + n + j - 5) % n];
            }
            let t = t / 11.0;
            // Turning left lifts the right side: a negative bank.
            bank[k] = -(t * 9.0).clamp(-0.35, 0.35) * scale;
        }
    }
    // Straights: runs of samples between turns.
    let start = (0..n).find(|&k| turning[k]).unwrap_or(0);
    let mut built = Vec::new();
    let mut k = 0;
    while k < n {
        let i0 = (start + k) % n;
        if turning[i0] {
            k += 1;
            continue;
        }
        let mut len = 0;
        while k + len < n && !turning[(start + k + len) % n] {
            len += 1;
        }
        let l = len as f32;
        let mut put = |from: f32, span: f32, prof: &dyn Fn(f32) -> f32| {
            let mut j = 0.0;
            while j < span {
                if from + j < l {
                    let idx = (i0 + (from + j) as usize) % n;
                    off[idx] = off[idx].max(prof(j) * scale);
                }
                j += 1.0;
            }
        };
        if l >= 46.0 {
            // A tabletop: up 5 m to 1.8 m, 9 m flat, down 7 m.
            put((l - 21.0) * 0.5, 21.0, &|x| if x < 5.0 { 1.8 * x / 5.0 } else if x < 14.0 { 1.8 } else { 1.8 * (21.0 - x) / 7.0 });
            built.push("tabletop");
        } else if l >= 30.0 {
            // A double: kicker to 1.6 m, a 7 m gap, a landing ramp down 7 m.
            put((l - 22.0) * 0.5, 22.0, &|x| if x < 4.5 { 1.6 * x / 4.5 } else if x < 6.0 { 1.6 } else if x < 13.0 { 0.0 } else if x < 15.0 { 1.4 } else { 1.4 * (22.0 - x) / 7.0 });
            built.push("double");
        } else if l >= 16.0 {
            // Rollers: smooth humps 7 m apart.
            put(4.0, l - 8.0, &|x| 0.7 * (0.5 - 0.5 * (x / 7.0 * std::f32::consts::TAU).cos()));
            built.push("rollers");
        }
        k += len;
    }
    let berms = (0..n).filter(|&k| turning[k] && !turning[(k + n - 1) % n]).count();
    (off, bank, format!("{berms} bermed turns, {}", if built.is_empty() { "no jumps".to_string() } else { built.join(", ") }))
}
