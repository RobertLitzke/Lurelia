use crate::world::chronicle::geom::{at_closed, claims, cumulative, each_near, grade_strip_except, smoothed_ground, spline, Nearest, Strip, WALL_THICK};
use crate::world::chronicle::{EventCtx, Process};
use crate::library::Op;
use crate::world::fields::{Circuit as Line, Placed, Poi, Pos};
use crate::world::shape;
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
/// running wide onto the verge. `level` gives the road's height outright at
/// a path point (metres, as the land's; null follows the land and `rise`),
/// eased between points like a rise. Where the road stands more than 2.5 m
/// over the ground it is a deck: a slab of `deck_mat` with kerbs and a
/// parapet each side, on pillars, so a road can cross over itself (two
/// stretches want 4.5 m between them where they cross). The centreline is kept (`World::circuits`)
/// for a race to follow: laps, the grid and the bots all go by it. `gate`
/// marks the start/finish line; a world names its materials in the event,
/// as for the road, so it reaches them. A race track built by
/// `tools/course/track_file.py` doesn't use this event: its road is frozen
/// in the world's `track.json` and stamped after the chronicle
/// (`chronicle/race_track.rs`), which shares the walls, decks, gate and
/// lamps here.
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
    #[serde(default)]
    gate: Gate,
    #[serde(default)]
    level: Vec<Option<f32>>,
    #[serde(default = "concrete")]
    deck_mat: String,
    /// Lamps along the circuit after dark, one every this many metres,
    /// alternating sides, over the walls (0 for none).
    #[serde(default)]
    lights: f32,
}

fn concrete() -> String {
    "concrete".into()
}

/// The gate's paint and the walls' material where the event names none.
const GATE_LIGHT: &str = "line_white";
const GATE_DARK: &str = "line_black";
const WALL_MAT: &str = "ice_wall";
/// The lamps' posts where nothing names them, and their glass heads.
pub(crate) const LAMP_POST: &str = "bridge_steel";
pub(crate) const LAMP_HEAD: &str = "lantern_glass";
/// The library materials a circuit uses without its event naming them
/// (`events::defaults_used`).
pub const DEFAULTS: [&str; 5] = [GATE_LIGHT, GATE_DARK, WALL_MAT, LAMP_POST, LAMP_HEAD];
/// The structures that name a gantry's and a deck piece's placement; their
/// voxels are built here.
const GATE_STRUCTURE: &str = "race_gate";
const DECK_STRUCTURE: &str = "race_deck";

/// The start/finish line: a chequer painted across the road, grid boxes
/// behind it, and a gantry over it, two posts outside the kerbs and a
/// chequered beam across, `height` metres clear above the road.
#[derive(Deserialize, Debug)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Gate {
    pub(crate) on: bool,
    pub(crate) light: String,
    pub(crate) dark: String,
    pub(crate) post: String,
    pub(crate) height: f32,
    /// Grid boxes painted behind the line, one per kart, two abreast 6 m
    /// apart, where the race puts them (`race::Race::slot`).
    pub(crate) grid: u32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate { on: false, light: GATE_LIGHT.into(), dark: GATE_DARK.into(), post: LAMP_POST.into(), height: 5.0, grid: 8 }
    }
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

        // The ground as it was, before the road: what the decks stand over.
        let raw: Vec<f32> = line.iter().map(|p| cx.f.height(p.x, p.y)).collect();
        // The rise at each path point, carried to the line by where each
        // point falls on it, eased between them. A point with a `level` has
        // the rise that puts the road at that height.
        let mut off = vec![0.0f32; n];
        // Line points between two path points that both have a level: flat
        // across as well as along (no banking), so their height is exactly
        // what was asked and a shape built to it (a tunnel's hollow) fits.
        let mut flat = vec![false; n];
        if !self.rise.is_empty() || self.level.iter().any(|l| l.is_some()) {
            // Where each path point falls on the line, searched in order a
            // little way on from the last, so a point by a crossing is found
            // on its own stretch and not the one across it.
            let mut at: Vec<usize> = Vec::with_capacity(pts.len());
            for (j, p) in pts.iter().enumerate() {
                let from = at.last().copied().unwrap_or(0);
                let reach = if j == 0 { 1 } else { (pts[j - 1].distance(*p) * 2.0) as usize + 10 };
                let best = (from..from + reach).min_by(|&a, &b| line[a % n].distance(*p).total_cmp(&line[b % n].distance(*p))).unwrap_or(from);
                at.push(best % n);
            }
            let k = pts.len();
            // The road's height at line point `i` as path point `j` asks for
            // it: its level outright, or the ground there and its rise. Two
            // levelled points ease from one level to the other exactly, so a
            // stretch between them at one level is flat.
            let want = |j: usize, i: usize| match self.level.get(j).copied().flatten() {
                Some(y) => y,
                None => base[i] + self.rise.get(j).copied().unwrap_or(0.0),
            };
            for j in 0..k {
                let (i0, i1) = (at[j], at[(j + 1) % k]);
                let span = (i1 + n - i0) % n;
                for s in 0..=span {
                    let i = (i0 + s) % n;
                    let t = if span == 0 { 0.0 } else { s as f32 / span as f32 };
                    let e = t * t * (3.0 - 2.0 * t);
                    let (y0, y1) = (want(j, i), want((j + 1) % k, i));
                    off[i] = y0 + (y1 - y0) * e - base[i];
                    flat[i] = self.level.get(j).copied().flatten().is_some() && self.level.get((j + 1) % k).copied().flatten().is_some();
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
        // The banking eases out over 10 m either side of a flat stretch.
        let unflat: Vec<f32> = (0..n).map(|i| (0..21).filter(|&j| flat[(i + n + j - 10) % n]).count() as f32 / 21.0).map(|f| 1.0 - (f * 2.0).min(1.0)).collect();
        let bank: Vec<f32> = (0..n)
            .map(|i| {
                let t = (0..13).map(|j| turn[(i + n + j - 6) % n]).sum::<f32>() / 13.0;
                -(t * 4.0).clamp(-0.18, 0.18) * self.bank * if flat[i] { 0.0 } else { unflat[i] }
            })
            .collect();

        // A wide margin: the ground beside the road is eased back to the
        // land over 12 m, so a car that runs wide meets a slope, not a wall.
        let road_y: Vec<f32> = off.iter().zip(&base).map(|(o, b)| o + b).collect();
        let deck = deck_mask(&road_y, &raw);
        let halfs = vec![self.width * 0.5; n];
        let walls = vec![self.wall; n];
        grade_strip_except(cx, &line, &base, &off, &bank, &Strip { width: self.width, mat: m, shoulder: kerb, margin: 12.0 }, &deck);
        if self.wall > 0.0 {
            let wm = cx.mat(self.wall_mat.as_deref().unwrap_or(WALL_MAT));
            build_walls(cx, &line, &road_y, &halfs, &walls, wm, &deck);
        }
        let decks = if deck.iter().any(|&d| d) {
            let parapet = self.wall_mat.clone().unwrap_or_else(|| self.deck_mat.clone());
            lay_decks(cx, &line, &road_y, &raw, &deck, &halfs, &walls, [&self.mat, &self.kerb, &self.deck_mat, &parapet])
        } else {
            0
        };
        for (p, gap) in tight_crossings(&line, &road_y) {
            cx.warnings.push(format!(
                "circuit {}: two stretches cross at {:.0},{:.0} with {gap:.1} m between them; they need {CROSS_CLEAR} m (raise one with `level`)",
                self.name, p.x, p.y
            ));
        }
        let gate = if self.gate.on {
            lay_gate(cx, &line, &road_y, self.width, m, &self.gate)
        } else {
            String::new()
        };
        if self.lights > 0.0 {
            light_road(cx, &line, &road_y, &halfs, self.lights, &self.gate.post);
        }
        cx.pois.push(Poi { name: self.name.clone(), pos: line[0], cause: cx.cause });
        let length: f32 = (0..n).map(|i| line[i].distance(line[(i + 1) % n])).sum();
        let climb = road_y.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        // The steepest grade over 10 m, either way: a kart slows hard up much
        // over 15% and goes light over the top of one.
        let step = 10usize.min(n / 4).max(1);
        let (mut worst, mut at) = (0.0f32, 0usize);
        for i in 0..n {
            let j = (i + step) % n;
            let run = (0..step).map(|k| line[(i + k) % n].distance(line[(i + k + 1) % n])).sum::<f32>().max(0.5);
            let g = (road_y[j] - road_y[i]) / run;
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
        let runs = if decks > 0 { format!(" ({})", deck_runs(&line, &deck)) } else { String::new() };
        cx.state.get::<Circuits>().0.push(Line { name: self.name.clone(), points: line, heights: road_y, width: self.width, ..Default::default() });
        format!(
            "circuit {}: {:.0} m round, {:.0} m from its lowest to highest, steepest grade {:.0}% {} {:.0} m round{gate}{}",
            self.name,
            length,
            climb.1 - climb.0,
            worst.abs() * 100.0,
            if worst > 0.0 { "up" } else { "down" },
            along,
            if decks > 0 { format!(", {decks} m on decks{runs}") } else { String::new() }
        )
    }
}

/// A wall `height[k]` m high along both sides of the road (none where it is
/// 0), `WALL_THICK` thick, just outside the kerb (which `grade_strip` lays from
/// `half[k]` to `half[k] + 1`), standing on the road's height `y[k]`. Each
/// cell goes by the nearest point of the whole line, so where the track
/// passes close to itself the wall never lands on the other stretch's road.
pub(crate) fn build_walls(cx: &mut EventCtx, line: &[Vec2], y: &[f32], half: &[f32], height: &[f32], m: u8, deck: &[bool]) {
    let cause = cx.cause;
    let f = &mut cx.f;
    // A cell nearest a deck gets no wall (the deck has its parapets) unless
    // it is under the deck and another stretch of road has it (`claims`).
    for c in claims(f, line, true, |k| half[k] + 2.5, |k| deck[k], |k| y[k]) {
        let (i, Some(Nearest { d, k, .. })) = (c.cell, c.owner) else { continue };
        if height[k] > 0.0 && d >= half[k] + 1.0 && d < half[k] + (1.0 + WALL_THICK) {
            f.h[i] = y[k] + height[k];
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

/// Paint the chequer and the grid boxes into the road (cells of road `road`
/// only, so the kerbs keep their colours), and build the gantry over the
/// line as voxels. Returns a note for the event's line.
pub(crate) fn lay_gate(cx: &mut EventCtx, line: &[Vec2], heights: &[f32], width: f32, road: u8, g: &Gate) -> String {
    let (along, len) = cumulative(line, true);
    let half = width * 0.5;
    let (light, dark) = (cx.mat(&g.light), cx.mat(&g.dark));
    let c = line[0];
    let d = (line[1] - line[0]).normalize_or(Vec2::X);
    // Across the road in the race's own sense (`Race::slot`), so the grid
    // boxes land under the karts.
    let side = Vec2::new(-d.y, d.x);
    let f = &mut cx.f;

    // The chequer: two rows of metre squares across the road on the line.
    each_near(f, c, half + 2.0, |f, i, _, q| {
        let (a, x) = ((q - c).dot(d), (q - c).dot(side));
        if a.abs() < 1.0 && x.abs() < half && f.surface[i] == road {
            let (row, col) = ((a + 1.0).floor() as i32, (x + half).floor() as i32);
            f.surface[i] = if (row + col).rem_euclid(2) == 0 { light } else { dark };
        }
    });
    // A white bar across the front of each grid box.
    for slot in 0..g.grid {
        let s = -6.0 * (slot as f32 + 1.0);
        let lane = if slot % 2 == 0 { -2.2 } else { 2.2 };
        let (p, dd) = at_closed(line, &along, len, s + 1.2);
        let sd = Vec2::new(-dd.y, dd.x);
        let mid = p + sd * lane;
        each_near(f, mid, 1.6, |f, i, _, q| {
            let (a, x) = ((q - mid).dot(dd), (q - mid).dot(sd));
            if a.abs() < 0.5 && x.abs() < 1.0 && f.surface[i] == road {
                f.surface[i] = light;
            }
        });
    }

    // The gantry, in world metres: posts 0.75 m square just outside the
    // kerbs, the beam a metre deep and half a metre thick across the top,
    // chequered in half-metre squares.
    let Some(def) = cx.lib.structure(GATE_STRUCTURE) else {
        cx.warnings.push("circuit: the library has no structure called race_gate, so no gantry".into());
        return ", chequer and grid painted".into();
    };
    let h0 = heights[0];
    let post_at = half + 1.6;
    let beam_half = half + 2.0;
    let beam_lo = h0 + g.height;
    let beam_hi = beam_lo + 1.0;
    let foot = [-1.0f32, 1.0].map(|k| {
        let q = c + side * post_at * k;
        cx.f.height(q.x, q.y).min(h0) - 0.5
    });
    let reach = beam_half + 0.8;
    let corners = [c + side * reach + d, c + side * reach - d, c - side * reach + d, c - side * reach - d];
    let (lo, hi) = corners.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let o = [(lo.x / V).floor() as i32, (foot[0].min(foot[1]) / V).floor() as i32, (lo.y / V).floor() as i32];
    let size = [((hi.x / V).ceil() as i32 - o[0]).max(1), ((beam_hi / V).ceil() as i32 - o[1] + 1).max(1), ((hi.y / V).ceil() as i32 - o[2]).max(1)];
    let mut ops = Vec::new();
    for lz in 0..size[2] {
        for lx in 0..size[0] {
            let q = Vec2::new((o[0] + lx) as f32 + 0.5, (o[2] + lz) as f32 + 0.5) * V;
            let (a, x) = ((q - c).dot(d), (q - c).dot(side));
            for ly in 0..size[1] {
                let y = ((o[1] + ly) as f32 + 0.5) * V;
                let post = (x.abs() - post_at).abs() < 0.375 && a.abs() < 0.375 && y >= foot[if x < 0.0 { 0 } else { 1 }] && y < beam_lo;
                let beam = x.abs() < beam_half && a.abs() < 0.25 && (beam_lo..beam_hi).contains(&y);
                let mat = if post {
                    &g.post
                } else if beam {
                    let (col, row) = (((x + beam_half) / 0.5).floor() as i32, ((y - beam_lo) / 0.5).floor() as i32);
                    if (col + row).rem_euclid(2) == 0 {
                        &g.light
                    } else {
                        &g.dark
                    }
                } else {
                    continue;
                };
                ops.push(Op::Set { at: [lx, ly, lz], mat: mat.clone(), part: None });
            }
        }
    }
    let lib = cx.lib;
    let compiled = shape::compile(&ops, size, &|m: &str| lib.material(m));
    let (cause, seed) = (cx.cause, cx.seed);
    cx.shapes.push(compiled);
    cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin: o, rot: 0, age: 0.0, cause, seed });
    // Nothing grows round the posts.
    for k in [-1.0f32, 1.0] {
        each_near(&mut cx.f, c + side * post_at * k, 1.0, |f, i, _, _| f.occupied[i] = true);
    }
    format!(", a gantry {:.0} m clear over the line", g.height)
}

/// The road's height above which a stretch is a deck on pillars rather than
/// an embankment: a little under the 3 m `grade_strip` will fill, so the deck
/// takes over just before the fill would leave a gap.
const DECK_ABOVE: f32 = 2.5;
/// Two stretches of road that cross want at least this between their
/// surfaces: a kart, the deck, and clearance.
pub(crate) const CROSS_CLEAR: f32 = 4.5;
/// A stretch of road built as voxels (a deck, a tunnel) is built in pieces
/// of this many line points (about a metre each), each its own structure,
/// so no one shape grows huge.
const PIECE: usize = 40;

/// The runs of line points `on` marks, round the loop, cut into pieces of
/// `PIECE` points. A run across the start line is one run.
pub(crate) fn pieces(n: usize, on: impl Fn(usize) -> bool) -> Vec<Vec<usize>> {
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    // Start the walk off any run.
    let first = (0..n).find(|&k| !on(k)).unwrap_or(0);
    for step in 0..n {
        let k = (first + step) % n;
        if on(k) {
            cur.push(k);
            if cur.len() >= PIECE {
                pieces.push(std::mem::take(&mut cur));
            }
        } else if !cur.is_empty() {
            pieces.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    pieces
}

/// The points a piece answers for and two more at each end, so the seam
/// between pieces is shared.
pub(crate) fn with_seams(piece: &[usize], n: usize) -> impl Iterator<Item = usize> + '_ {
    (-2i64..piece.len() as i64 + 2).map(move |j| (piece[0] as i64 + j).rem_euclid(n as i64) as usize)
}

/// Which of the points `near` is nearest `q`.
pub(crate) fn nearest_of(near: &[usize], line: &[Vec2], q: Vec2) -> Option<usize> {
    near.iter().copied().min_by(|&a, &b| line[a].distance_squared(q).total_cmp(&line[b].distance_squared(q)))
}

/// A box of voxels filled column by column, then placed as one structure.
pub(crate) struct Columns {
    /// The box's low corner, in world voxels, and its size.
    pub(crate) o: [i32; 3],
    pub(crate) size: [i32; 3],
    ops: Vec<Op>,
}

impl Columns {
    /// The box that holds `lo` to `hi` on the plan and `ylo` to `yhi` in
    /// height, metres, and `below` voxels more underneath.
    pub(crate) fn new(lo: Vec2, hi: Vec2, ylo: f32, yhi: f32, below: i32) -> Columns {
        let o = [(lo.x / V).floor() as i32, (ylo / V).floor() as i32 - below, (lo.y / V).floor() as i32];
        let size = [((hi.x / V).ceil() as i32 - o[0]).max(1), ((yhi / V).ceil() as i32 - o[1]).max(1), ((hi.y / V).ceil() as i32 - o[2]).max(1)];
        Columns { o, size, ops: Vec::new() }
    }

    /// The middle of column (`lx`, `lz`) on the plan, metres.
    pub(crate) fn centre(&self, lx: i32, lz: i32) -> Vec2 {
        Vec2::new((self.o[0] + lx) as f32 + 0.5, (self.o[2] + lz) as f32 + 0.5) * V
    }

    /// Fill column (`lx`, `lz`) from `y0` to `y1` metres with `mat`. Voxel
    /// rows go by rounding, so a surface is within half a voxel of the
    /// graded road it runs on from (a kart rolls onto a step of 0.1 m and
    /// meets anything taller as a kerb).
    pub(crate) fn column(&mut self, lx: i32, lz: i32, y0: f32, y1: f32, mat: &str) {
        let (a, b) = (((y0 / V).round() as i32 - self.o[1]).max(0), ((y1 / V).round() as i32 - self.o[1]).min(self.size[1]));
        if b > a {
            self.ops.push(Op::Box { from: [lx, a, lz], to: [lx + 1, b, lz + 1], mat: mat.to_string(), part: None });
        }
    }

    /// Compile the box and place it where it was built, as structure `def`.
    pub(crate) fn place(self, cx: &mut EventCtx, def: usize, resolve: &dyn Fn(&str) -> Option<u8>) {
        let (cause, seed) = (cx.cause, cx.seed);
        cx.shapes.push(shape::compile(&self.ops, self.size, resolve));
        cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin: self.o, rot: 0, age: 0.0, cause, seed });
    }
}

const V: f32 = crate::world::gen::VOXEL;
/// Pillars stand this many line points apart.
const PILLAR_EVERY: usize = 12;

/// Which line points are on a deck: the road more than `DECK_ABOVE` over the
/// ground as it was before the road (`raw`). Runs shorter than three points
/// are dropped, and each run reaches two points further at both ends so the
/// deck overlaps the embankment it rises off. The line is a loop: a run
/// across the start line is one run.
fn deck_mask(heights: &[f32], raw: &[f32]) -> Vec<bool> {
    let n = heights.len();
    let high: Vec<bool> = (0..n).map(|k| heights[k] - raw[k] > DECK_ABOVE).collect();
    // Walk once round from a point off any run, so no run is met in two halves.
    let Some(first) = (0..n).find(|&k| !high[k]) else {
        return vec![n >= 3; n];
    };
    let mut out = vec![false; n];
    let mut run = 0;
    for step in 1..=n {
        let k = (first + step) % n;
        if high[k] {
            run += 1;
            continue;
        }
        // `k` is the first point past a run of `run` points.
        if run >= 3 {
            for j in 0..run + 4 {
                out[(k + 2 * n - run - 2 + j) % n] = true;
            }
        }
        run = 0;
    }
    out
}

/// Where two stretches of the line cross on the plan with less than
/// `CROSS_CLEAR` between their surfaces: (where, the height between them).
pub(crate) fn tight_crossings(line: &[Vec2], heights: &[f32]) -> Vec<(Vec2, f32)> {
    let n = line.len();
    let mut out: Vec<(Vec2, f32)> = Vec::new();
    for i in 0..n {
        let (a, b) = (line[i], line[(i + 1) % n]);
        for j in i + 30..n {
            if n - (j - i) < 30 {
                continue;
            }
            let (c, d) = (line[j], line[(j + 1) % n]);
            if let Some(p) = crate::world::chronicle::geom::segments_cross(a, b, c, d) {
                let gap = (heights[i] - heights[j]).abs();
                if gap < CROSS_CLEAR && out.iter().all(|(q, _)| q.distance(p) > 10.0) {
                    out.push((p, gap));
                }
            }
        }
    }
    out
}

/// The decks: for each stretch on `deck`, a slab of road 0.75 m thick with
/// its kerbs, a parapet either side (the walls' height, or 1 m), and pillars
/// down to the ground every `PILLAR_EVERY` m, built as voxels in pieces. No
/// pillar stands on another stretch of road. Returns how many metres of deck.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lay_decks(cx: &mut EventCtx, line: &[Vec2], heights: &[f32], raw: &[f32], deck: &[bool], halfs: &[f32], walls: &[f32], mats: [&str; 4]) -> usize {
    let Some(def) = cx.lib.structure(DECK_STRUCTURE) else {
        cx.warnings.push("circuit: the library has no structure called race_deck, so no decks".into());
        return 0;
    };
    let [road, kerb, slab, parapet] = mats;
    let n = line.len();
    let rail_at = |k: usize| if walls[k] > 0.0 { walls[k] } else { 1.0 };
    let dir = |k: usize| (line[(k + 1) % n] - line[(k + n - 1) % n]).normalize_or(Vec2::X);
    let lib = cx.lib;
    let mut metres = 0;
    for piece in pieces(n, |k| deck[k]) {
        metres += piece.len();
        let near: Vec<usize> = with_seams(&piece, n).collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        let (mut ylo, mut yhi) = (f32::MAX, f32::MIN);
        for &k in &near {
            let reach = halfs[k] + 2.0;
            lo = lo.min(line[k] - Vec2::splat(reach));
            hi = hi.max(line[k] + Vec2::splat(reach));
            ylo = ylo.min(raw[k].min(heights[k] - 1.0));
            yhi = yhi.max(heights[k] + rail_at(k) + 0.5);
        }
        let mut cols = Columns::new(lo, hi, ylo, yhi, 1);
        // Pillars: line points on the stride, not over another stretch of
        // road lower down.
        let pillars: Vec<(Vec2, f32, usize)> = piece
            .iter()
            .filter(|&&k| k % PILLAR_EVERY == 0)
            .flat_map(|&k| {
                let side = Vec2::new(-dir(k).y, dir(k).x);
                [-1.0f32, 1.0].map(|s| (line[k] + side * s * (halfs[k] - 1.5), heights[k], k))
            })
            .filter(|&(p, top, k)| {
                !(0..n).any(|j| {
                    let apart = (j as i64 - k as i64).rem_euclid(n as i64).min((k as i64 - j as i64).rem_euclid(n as i64));
                    apart > 30 && line[j].distance(p) < halfs[j] + 2.5 && heights[j] < top - 2.0
                })
            })
            .collect();
        for lz in 0..cols.size[2] {
            for lx in 0..cols.size[0] {
                let q = cols.centre(lx, lz);
                // The deck: by the nearest of the piece's points.
                if let Some(k) = nearest_of(&near, line, q) {
                    let side = Vec2::new(-dir(k).y, dir(k).x);
                    let x = (q - line[k]).dot(side).abs();
                    let along = (q - line[k]).dot(dir(k)).abs();
                    let ry = heights[k];
                    let half = halfs[k];
                    if along <= 0.75 {
                        if x < half + 1.0 {
                            cols.column(lx, lz, ry - 0.75, ry - V, slab);
                            cols.column(lx, lz, ry - V, ry, if x < half { road } else { kerb });
                        } else if x < half + 1.5 {
                            cols.column(lx, lz, ry - 0.75, ry + rail_at(k), parapet);
                        }
                    }
                }
                for (p, top, _) in &pillars {
                    if (q - *p).abs().max_element() < 0.5 {
                        let foot = cx.f.height(p.x, p.y) - 0.5;
                        cols.column(lx, lz, foot, top - 0.75, slab);
                    }
                }
            }
        }
        cols.place(cx, def, &|m: &str| lib.material(m));
        // Nothing grows under the deck's edges or round the pillars.
        for (p, _, _) in &pillars {
            each_near(&mut cx.f, *p, 1.0, |f, i, _, _| f.occupied[i] = true);
        }
    }
    metres
}

/// The decks as "x,z +N m" for each run, for the event's line.
fn deck_runs(line: &[Vec2], deck: &[bool]) -> String {
    let n = line.len();
    let first = (0..n).find(|&k| !deck[k]).unwrap_or(0);
    let mut out = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for step in 0..=n {
        let k = (first + step) % n;
        match (deck[k] && step < n, run) {
            (true, None) => run = Some((k, 1)),
            (true, Some((a, len))) => run = Some((a, len + 1)),
            (false, Some((a, len))) => {
                out.push(format!("{:.0},{:.0} +{len} m", line[a].x, line[a].y));
                run = None;
            }
            (false, None) => {}
        }
    }
    out.join("; ")
}

/// Lamps along the road, lit from dusk to dawn: one every `every` metres,
/// alternating sides, warm white, each on a post in `post` just outside the
/// kerb (where the gantry's posts stand) with an arm out over the kerb and
/// a glass head (`LAMP_HEAD`, lit from dusk) under its end.
///
/// A lamp gets no post where the ground under it is more than `POST_DROP`
/// below the road (a deck or an embankment: the post would hang in the
/// air or stand a long way down) or above it (a cutting or a tunnel, where
/// it would stand in the rock). The light is hung there all the same.
pub(crate) fn light_road(cx: &mut EventCtx, line: &[Vec2], heights: &[f32], halfs: &[f32], every: f32, post: &str) {
    const HIGH: f32 = 6.0;
    const ARM: f32 = 1.4;
    const POST_DROP: f32 = 2.0;
    let n = line.len();
    let mut since = every * 0.5;
    let mut side = 1.0f32;
    let def = cx.lib.structure(GATE_STRUCTURE);
    let mut posts: Vec<(Vec2, Vec<([i32; 3], String)>)> = Vec::new();
    for k in 0..n {
        since += line[k].distance(line[(k + 1) % n]);
        if since < every {
            continue;
        }
        since = 0.0;
        let d = (line[(k + 1) % n] - line[(k + n - 1) % n]).normalize_or(Vec2::X);
        let out = Vec2::new(-d.y, d.x) * side;
        side = -side;
        let p = line[k] + out * (halfs[k] + 1.6);
        let head = p - out * ARM;
        let road = heights[k];
        cx.lamps.push(crate::world::fields::WorldLamp {
            pos: glam::Vec3::new(head.x, road + HIGH - 0.6, head.y),
            reach: 22.0,
            colour: [1.0, 0.86, 0.62],
            strength: 1.4,
            after_dark: true,
        });
        let ground = cx.f.height(p.x, p.y);
        if def.is_none() || (ground - road).abs() > POST_DROP {
            continue;
        }
        let mut ops = Vec::new();
        // In world voxels: the post 0.5 m square from below the ground to
        // the top, the arm a voxel deep along its top toward the road, and
        // the head two voxels square under the arm's end.
        let vox = |x: f32| (x / V).floor() as i32;
        let (top, foot) = (vox(road + HIGH), vox(ground.min(road) - 0.5));
        let (px, pz) = (vox(p.x), vox(p.y));
        for y in foot..top {
            for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                ops.push(([px + dx, y, pz + dz], post.to_string()));
            }
        }
        let steps = (ARM / V).ceil() as i32 + 1;
        for s in 0..=steps {
            let q = p - out * (ARM * s as f32 / steps as f32);
            ops.push(([vox(q.x), top - 1, vox(q.y)], post.to_string()));
            ops.push(([vox(q.x) + 1, top - 1, vox(q.y)], post.to_string()));
        }
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            ops.push(([vox(head.x) + dx, top - 2, vox(head.y) + dz], LAMP_HEAD.to_string()));
        }
        posts.push((p, ops));
    }
    let Some(def) = def else { return };
    // One shape per post, so a whole circuit's lamps are not one box the
    // size of the course.
    let lib = cx.lib;
    for (p, set) in posts {
        // Nothing grows round the post.
        each_near(&mut cx.f, p, 0.8, |f, i, _, _| f.occupied[i] = true);
        let lo = set.iter().fold([i32::MAX; 3], |m, (a, _)| [m[0].min(a[0]), m[1].min(a[1]), m[2].min(a[2])]);
        let hi = set.iter().fold([i32::MIN; 3], |m, (a, _)| [m[0].max(a[0]), m[1].max(a[1]), m[2].max(a[2])]);
        let size = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];
        let mine: Vec<Op> = set.into_iter().map(|(a, mat)| Op::Set { at: [a[0] - lo[0], a[1] - lo[1], a[2] - lo[2]], mat, part: None }).collect();
        let compiled = shape::compile(&mine, size, &|m: &str| lib.material(m));
        let (cause, seed) = (cx.cause, cx.seed);
        cx.shapes.push(compiled);
        cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin: lo, rot: 0, age: 0.0, cause, seed });
    }
}
