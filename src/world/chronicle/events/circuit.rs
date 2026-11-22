use crate::world::chronicle::geom::{each_near, grade_strip, smoothed_ground, spline, Strip};
use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Circuit as Line, Poi, Pos};
use glam::Vec2;
use serde::Deserialize;

/// A closed paved racing circuit through `path`: graded smooth for a kart
/// (short springs want no voxel steps), banked a little on the outside of
/// its turns, with kerbs of `kerb` along both edges. `rise` lifts the road
/// above the smoothed ground at each path point, metres, eased between
/// them: embankments and cuttings for climbs and drops. Keep a rise under
/// 3 m above the ground or grading leaves a gap for a bridge that nothing
/// fills. `wall` stands a wall that many metres high along both sides,
/// outside the kerbs, in `wall_mat`: a course in a canyon of ice, with no
/// running wide onto the verge. The centreline is kept (`World::circuits`)
/// for a race to follow: laps, the grid and the bots all go by it.
#[derive(Deserialize, Debug)]
pub struct Circuit {
    name: String,
    path: Vec<Pos>,
    width: f32,
    mat: String,
    kerb: String,
    #[serde(default)]
    rise: Vec<f32>,
    #[serde(default = "one")]
    bank: f32,
    #[serde(default)]
    wall: f32,
    #[serde(default)]
    wall_mat: Option<String>,
}

fn one() -> f32 {
    1.0
}

impl Process for Circuit {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        let line = spline(&pts, true, 1.0);
        let n = line.len();
        let m = cx.mat(&self.mat);
        let kerb = cx.mat(&self.kerb);
        let base = smoothed_ground(&cx.f, &line, 40.0, true);

        // The rise at each path point, carried to the line by where each
        // point falls on it, eased between them.
        let mut off = vec![0.0f32; n];
        if !self.rise.is_empty() {
            let at: Vec<usize> = pts
                .iter()
                .map(|p| (0..n).min_by(|&a, &b| line[a].distance(*p).total_cmp(&line[b].distance(*p))).unwrap_or(0))
                .collect();
            let k = pts.len();
            for j in 0..k {
                let (i0, i1) = (at[j], at[(j + 1) % k]);
                let (r0, r1) = (self.rise.get(j).copied().unwrap_or(0.0), self.rise.get((j + 1) % k).copied().unwrap_or(0.0));
                let span = (i1 + n - i0) % n;
                for s in 0..=span {
                    let t = if span == 0 { 0.0 } else { s as f32 / span as f32 };
                    let e = t * t * (3.0 - 2.0 * t);
                    off[(i0 + s) % n] = r0 + (r1 - r0) * e;
                }
            }
        }

        // Banking: the outside of each turn lifted, by how hard it turns.
        let turn: Vec<f32> = (0..n)
            .map(|i| {
                let (a, b, c) = (line[(i + n - 4) % n], line[i], line[(i + 4) % n]);
                let (d1, d2) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
                -d1.perp_dot(d2).atan2(d1.dot(d2)) / ((a.distance(b) + b.distance(c)) * 0.5).max(0.5)
            })
            .collect();
        let bank: Vec<f32> = (0..n)
            .map(|i| {
                let t = (0..13).map(|j| turn[(i + n + j - 6) % n]).sum::<f32>() / 13.0;
                -(t * 4.0).clamp(-0.18, 0.18) * self.bank
            })
            .collect();

        // A wide margin: the ground beside the road is eased back to the
        // land over 12 m, so a car that runs wide meets a slope, not a wall.
        let gaps = grade_strip(cx, &line, &base, &off, &bank, &Strip { width: self.width, mat: m, shoulder: kerb, margin: 12.0 });
        if !gaps.is_empty() {
            cx.warnings.push(format!("circuit {}: {} stretch(es) over 3 m above the ground leave a gap", self.name, gaps.len()));
        }
        if self.wall > 0.0 {
            let wm = cx.mat(self.wall_mat.as_deref().unwrap_or("ice_wall"));
            build_walls(cx, &line, &base, &off, self.width * 0.5, self.wall, wm);
        }
        cx.pois.push(Poi { name: self.name.clone(), pos: line[0], cause: cx.cause });
        let length: f32 = (0..n).map(|i| line[i].distance(line[(i + 1) % n])).sum();
        let climb = off.iter().zip(&base).map(|(o, b)| o + b).fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
        let heights: Vec<f32> = off.iter().zip(&base).map(|(o, b)| o + b).collect();
        // The steepest grade over 10 m, either way: a kart slows hard up much
        // over 15% and goes light over the top of one.
        let step = 10usize.min(n / 4).max(1);
        let (mut worst, mut at) = (0.0f32, 0usize);
        for i in 0..n {
            let j = (i + step) % n;
            let run = (0..step).map(|k| line[(i + k) % n].distance(line[(i + k + 1) % n])).sum::<f32>().max(0.5);
            let g = (heights[j] - heights[i]) / run;
            if g.abs() > worst.abs() {
                (worst, at) = (g, i);
            }
        }
        let along: f32 = (0..at).map(|i| line[i].distance(line[i + 1])).sum();
        if worst.abs() > 0.15 {
            cx.warnings.push(format!(
                "circuit {}: a {:.0}% grade {} m round, at {:.0},{:.0}: karts slow hard and go light over the top; ease the hills or RISE there",
                self.name,
                worst.abs() * 100.0,
                along.round(),
                line[at].x,
                line[at].y
            ));
        }
        cx.state.get::<Circuits>().0.push(Line { name: self.name.clone(), points: line, heights, width: self.width });
        format!(
            "circuit {}: {:.0} m round, {:.0} m from its lowest to highest, steepest grade {:.0}% {} {:.0} m round",
            self.name,
            length,
            climb.1 - climb.0,
            worst.abs() * 100.0,
            if worst > 0.0 { "up" } else { "down" },
            along
        )
    }
}

/// A wall `height` m high along both sides of the road, a metre thick, just
/// outside the kerb (which `grade_strip` lays from `half` to `half + 1`). Each
/// cell goes by the nearest point of the whole line, so where the track
/// passes close to itself the wall never lands on the other stretch's road.
fn build_walls(cx: &mut EventCtx, line: &[Vec2], base: &[f32], off: &[f32], half: f32, height: f32, m: u8) {
    let cause = cx.cause;
    let f = &mut cx.f;
    let mut best: HashMap<usize, (f32, usize)> = HashMap::default();
    for (k, p) in line.iter().enumerate() {
        each_near(f, *p, half + 2.5, |_f, i, d, _q| {
            let e = best.entry(i).or_insert((f32::MAX, 0));
            if d < e.0 {
                *e = (d, k);
            }
        });
    }
    for (i, (d, k)) in best {
        if d >= half + 1.0 && d < half + 2.2 {
            f.h[i] = base[k] + off[k] + height;
            f.surface[i] = m;
            f.occupied[i] = true;
            f.cliff[i] = 1.0;
            f.cause[i] = cause;
        }
    }
}

/// The circuits the chronicle laid, for `build_world` to keep.
#[derive(Default)]
pub struct Circuits(pub Vec<Line>);
