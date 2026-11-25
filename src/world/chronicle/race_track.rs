//! A race track from a world's `track.json` (world.json `race.track`),
//! written by `tools/course/track_file.py` (`tools/course/FORMAT.md`).
//!
//! The file is the road frozen: a sample a metre round the lap with the
//! road's height, bank, width, walls and run-off at each, and the race
//! furniture. `build_world_with` stamps it after the chronicle and its water
//! settling, so the road comes out the same whatever the world's own events
//! did round it and however the generator changes: nothing here reads a
//! height from the ground except the foot of a pillar. The road takes its
//! cells outright, the run-off beside it is levelled to the road's edge, the
//! ground past that is eased back to the world's over `MARGIN`, and any
//! structure the world stood in the corridor is taken out again.

use super::events::circuit::{build_walls, lay_decks, lay_gate, light_road, nearest_of, pieces, tight_crossings, with_seams, Circuits, Columns, Gate, CROSS_CLEAR};
use super::geom::{at_closed, claims, cumulative, each_near, Nearest, WALL_THICK};
use super::EventCtx;
use crate::util::FxHashMap as HashMap;
use crate::world::fields::{Circuit, ItemRow, Pad, Placed, Poi};
use crate::world::noise::smoothstep;
use glam::Vec2;
use serde::Deserialize;

/// The ground past the run-off is eased back to the world's over this many
/// metres.
const MARGIN: f32 = 12.0;
/// A structure the world places counts as standing in the corridor if its
/// foot is less than this far over the road.
const HEADROOM: f32 = 6.0;
/// The ground under a deck is kept at least this far below the road on it:
/// the slab is 0.75 m thick.
const UNDER_DECK: f32 = 1.0;
/// The grade past which a kart slows hard and goes light over the top.
const STEEP: f32 = 0.15;

#[derive(Deserialize, Debug)]
pub struct TrackFile {
    pub name: String,
    /// `[x, z, y, bank, width, wall, runoff, flags]` a metre apart round the
    /// lap from the start line; flags 1 a deck, 2 covered (a shape over it:
    /// no ground eased beside it), 4 in a tunnel.
    pub line: Vec<[f32; 8]>,
    pub mats: Mats,
    #[serde(default)]
    pub gate: Option<TrackGate>,
    #[serde(default)]
    pub lights: f32,
    /// `[from, to, material]` stretches of road of another material.
    #[serde(default)]
    pub surfaces: Vec<(f32, f32, String)>,
    /// `[from, to, material]` stretches of run-off of a material.
    #[serde(default)]
    pub verges: Vec<(f32, f32, String)>,
    #[serde(default)]
    pub items: Vec<ItemRow>,
    #[serde(default)]
    pub pads: Vec<Pad>,
    #[serde(default)]
    pub jumps: Vec<Jump>,
    #[serde(default)]
    pub checkpoints: Vec<f32>,
    #[serde(default)]
    pub tunnels: Vec<Tunnel>,
    #[serde(default)]
    pub shapes: Vec<TrackShape>,
    #[serde(default)]
    pub structures: Vec<TrackStructure>,
    #[serde(default)]
    pub zones: Vec<crate::world::fields::Zone>,
    #[serde(default)]
    pub smash: Vec<crate::world::fields::Smash>,
}

#[derive(Deserialize, Debug)]
pub struct Mats {
    pub road: String,
    pub kerb: String,
    pub wall: String,
    pub deck: String,
    #[serde(default)]
    pub verge: Option<String>,
    /// What the boost pads are painted in.
    #[serde(default = "boost_pad")]
    pub pad: String,
}

/// What the pads are painted in and a tunnel is lit by where the track file
/// names neither.
const PAD_MAT: &str = "boost_pad";
const LAMP_MAT: &str = "works_lamp";
/// The library materials a track uses without its file naming them, reached
/// by every world with a track (`worldload::reach`).
pub const DEFAULTS: [&str; 4] = [PAD_MAT, LAMP_MAT, super::events::circuit::LAMP_POST, super::events::circuit::LAMP_HEAD];
/// The structures that name the placement of a shape and of a tunnel piece;
/// their voxels are built here.
const SHAPE_STRUCTURE: &str = "track_shape";
const TUNNEL_STRUCTURE: &str = "race_tunnel";

fn boost_pad() -> String {
    PAD_MAT.into()
}

#[derive(Deserialize, Debug)]
pub struct TrackGate {
    pub light: String,
    pub dark: String,
    pub post: String,
    #[serde(default = "eight")]
    pub grid: u32,
}

fn eight() -> u32 {
    8
}

#[derive(Deserialize, Debug)]
pub struct Jump {
    pub s: f32,
    /// In the road's heights already along the centreline; used again to
    /// slant a skewed lip across the road.
    pub height: f32,
    pub len: f32,
    /// Degrees the lip is turned from square across the road, the right
    /// side further along: a jump laid along a dune crest that crosses the
    /// road at a slant, so one side of a kart takes off first.
    #[serde(default)]
    pub skew: f32,
}

/// A stretch of road bored through whatever land is there: a hollow
/// `height` m high over the road and its kerbs, lined in `mat` a metre thick
/// (walls, roof, and a slab under the road), lit inside. The land over it is
/// the world's; where it is lower than the roof, the lining stands out of it
/// as a gallery.
#[derive(Deserialize, Debug)]
pub struct Tunnel {
    pub from: f32,
    pub to: f32,
    #[serde(default = "six")]
    pub height: f32,
    pub mat: String,
    /// The frame round each portal.
    #[serde(default = "concrete")]
    pub portal: String,
    /// The light panels in the roof.
    #[serde(default = "works_lamp")]
    pub light: String,
}

fn works_lamp() -> String {
    LAMP_MAT.into()
}

fn concrete() -> String {
    "concrete".into()
}

fn six() -> f32 {
    6.0
}

/// An `add` or `carve` shape: boxes of voxels from `at` (world voxels).
#[derive(Deserialize, Debug)]
pub struct TrackShape {
    pub mat: String,
    pub at: [i32; 3],
    pub size: [i32; 3],
    pub boxes: Vec<[i32; 6]>,
    #[serde(default)]
    pub line: u32,
}

#[derive(Deserialize, Debug)]
pub struct TrackStructure {
    pub name: String,
    pub at: [f32; 2],
    #[serde(default)]
    pub bearing: Option<f32>,
    /// Its anchor at this height, metres; on the ground without one.
    #[serde(default)]
    pub y: Option<f32>,
}

/// Read a world's track file.
pub fn load(dir: &std::path::Path, file: &str) -> Result<TrackFile, String> {
    let text = std::fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
    let t: TrackFile = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
    if t.line.len() < 8 {
        return Err(format!("{file}: a line of {} points is no track", t.line.len()));
    }
    Ok(t)
}

/// The centreline alone, for placing things by lap distance (`Pos::Along`).
#[derive(Clone, Debug, Default)]
pub struct TrackLine {
    pub points: Vec<Vec2>,
    pub along: Vec<f32>,
    pub length: f32,
}

impl TrackLine {
    pub fn of(t: &TrackFile) -> TrackLine {
        let points: Vec<Vec2> = t.line.iter().map(|r| Vec2::new(r[0], r[1])).collect();
        let (along, length) = cumulative(&points, true);
        TrackLine { points, along, length }
    }

    /// The point `s` metres round and the way the race runs there.
    pub fn at(&self, s: f32) -> (Vec2, Vec2) {
        at_closed(&self.points, &self.along, self.length, s)
    }

    /// The point `s` metres round, `right` metres right of the centreline
    /// (negative: left), as the race measures `off`.
    pub fn beside(&self, s: f32, right: f32) -> Vec2 {
        let (p, d) = self.at(s);
        p + Vec2::new(-d.y, d.x) * right
    }

    /// Points `step` metres apart from `from` to `to` round the lap (going
    /// the way the race runs, past the line if `to` is before `from`),
    /// `right` metres off the centreline.
    pub fn path(&self, from: f32, to: f32, right: f32, step: f32) -> Vec<Vec2> {
        let span = (to - from).rem_euclid(self.length);
        let span = if span == 0.0 { self.length } else { span };
        let k = (span / step.max(0.5)).ceil().max(1.0) as usize;
        (0..=k).map(|i| self.beside(from + span * i as f32 / k as f32, right)).collect()
    }
}

/// A line beside the race track (an event's `along` in place of a `path`):
/// from `from` to `to` metres round the lap, `left` or `right` metres off the
/// centreline, a point every `step` metres.
#[derive(Deserialize, Debug, Clone)]
pub struct AlongRun {
    pub from: f32,
    pub to: f32,
    #[serde(default)]
    pub left: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default = "five")]
    pub step: f32,
}

fn five() -> f32 {
    5.0
}

/// A band beside the race track (an event's `along` in place of a `poly`):
/// from `from` to `to` metres round the lap, between `[near, far]` metres
/// off the centreline on the `left` or the `right`.
#[derive(Deserialize, Debug, Clone)]
pub struct AlongBand {
    pub from: f32,
    pub to: f32,
    #[serde(default)]
    pub left: Option<[f32; 2]>,
    #[serde(default)]
    pub right: Option<[f32; 2]>,
}

/// The points of an `AlongRun`, or none (with a warning) in a world with no
/// race track.
pub fn along_run(cx: &mut EventCtx, a: &AlongRun) -> Vec<Vec2> {
    match &cx.track {
        Some(t) => t.path(a.from, a.to, a.right - a.left, a.step),
        None => {
            cx.warnings.push("along: the world has no race track (world.json race.track)".into());
            Vec::new()
        }
    }
}

/// The outline of an `AlongBand`: out along its near edge and back along
/// its far one.
pub fn along_band(cx: &mut EventCtx, b: &AlongBand) -> Vec<Vec2> {
    let Some(t) = &cx.track else {
        cx.warnings.push("along: the world has no race track (world.json race.track)".into());
        return Vec::new();
    };
    let (near, far) = match (b.left, b.right) {
        (Some([n, f]), _) => (-n, -f),
        (None, Some([n, f])) => (n, f),
        (None, None) => {
            cx.warnings.push("along: a band needs left or right [near, far]".into());
            return Vec::new();
        }
    };
    let mut pts = t.path(b.from, b.to, near, 5.0);
    let mut back = t.path(b.from, b.to, far, 5.0);
    back.reverse();
    pts.extend(back);
    pts
}

/// The material a track file names to cut voxels away.
const CARVE: &str = "carve";

/// A track's material by name: `CARVE`, or one of the library's.
fn or_carve(lib: &crate::library::Library, m: &str) -> Option<u8> {
    if m == CARVE {
        Some(crate::world::shape::CARVE)
    } else {
        lib.material(m)
    }
}

/// Whether lap distance `s` is in the stretch `from..to`, which may wrap
/// past the line.
fn in_run(s: f32, from: f32, to: f32) -> bool {
    if from <= to {
        s >= from && s < to
    } else {
        s >= from || s < to
    }
}

/// Stamp the track into the world, after the chronicle. `world_placed` is
/// how many structures the world's own events placed: those standing in the
/// corridor are taken out. Returns the line for the build log.
pub fn stamp(cx: &mut EventCtx, t: &TrackFile, world_placed: usize) -> String {
    let n = t.line.len();
    // The centreline, as `build_world_with` read it for the events.
    let TrackLine { points: line, along, length } = cx.track.clone().unwrap_or_else(|| TrackLine::of(t));
    let y: Vec<f32> = t.line.iter().map(|r| r[2]).collect();
    let bank: Vec<f32> = t.line.iter().map(|r| r[3]).collect();
    let half: Vec<f32> = t.line.iter().map(|r| r[4] * 0.5).collect();
    let wall: Vec<f32> = t.line.iter().map(|r| r[5]).collect();
    let runoff: Vec<f32> = t.line.iter().map(|r| r[6]).collect();
    let deck: Vec<bool> = t.line.iter().map(|r| r[7] as u32 & 1 != 0).collect();
    let covered: Vec<bool> = t.line.iter().map(|r| r[7] as u32 & 2 != 0).collect();
    let tunnel: Vec<bool> = t.line.iter().map(|r| r[7] as u32 & 4 != 0).collect();

    let road_m = cx.mat(&t.mats.road);
    let kerb = cx.mat(&t.mats.kerb);
    let wall_m = cx.mat(&t.mats.wall);
    let mut road = vec![road_m; n];
    for (from, to, m) in &t.surfaces {
        let id = cx.mat(m);
        for k in 0..n {
            if in_run(along[k], *from, *to) {
                road[k] = id;
            }
        }
    }
    let base_verge = t.mats.verge.as_ref().map(|m| cx.mat(m));
    let mut verge = vec![base_verge; n];
    for (from, to, m) in &t.verges {
        let id = cx.mat(m);
        for k in 0..n {
            if in_run(along[k], *from, *to) {
                verge[k] = Some(id);
            }
        }
    }
    // The ground under each point before the road: where a pillar stands.
    let raw: Vec<f32> = line.iter().map(|p| cx.f.height(p.x, p.y)).collect();

    let mut corridor = grade_road(cx, &Road { line: &line, y: &y, bank: &bank, half: &half, wall: &wall, runoff: &runoff, road: &road, kerb, verge: &verge, deck: &deck, covered: &covered, tunnel: &tunnel, along: &along, length, jumps: &t.jumps });
    let bored = bore(cx, &line, &along, length, &y, &half, &tunnel, &t.tunnels, [&t.mats.road, &t.mats.kerb], &mut corridor);
    if wall.iter().any(|&w| w > 0.0) {
        build_walls(cx, &line, &y, &half, &wall, wall_m, &deck);
    }
    let decks = if deck.iter().any(|&d| d) {
        let parapet = t.mats.wall.clone();
        // A deck's top is the road's material there: a `surface` stretch
        // over a pier is planks on the deck as on the land.
        let mut roads = vec![t.mats.road.as_str(); n];
        for (from, to, m) in &t.surfaces {
            for k in 0..n {
                if in_run(along[k], *from, *to) {
                    roads[k] = m.as_str();
                }
            }
        }
        lay_decks(cx, &line, &y, &raw, &deck, &half, &wall, &roads, [&t.mats.kerb, &t.mats.deck, &parapet])
    } else {
        0
    };
    paint_pads(cx, &line, &along, length, &half, &t.pads, &t.mats.pad);

    // The shapes over and round the road: a tunnel's block and its hollow,
    // arches, a grandstand.
    let mut shapes = 0;
    match cx.lib.structure(SHAPE_STRUCTURE) {
        Some(def) => {
            let lib = cx.lib;
            let (cause, seed) = (cx.cause, cx.seed);
            for s in &t.shapes {
                let ops: Vec<crate::library::Op> = s.boxes.iter().map(|b| crate::library::Op::Box { from: [b[0], b[1], b[2]], to: [b[3], b[4], b[5]], mat: s.mat.clone(), part: None }).collect();
                if s.mat != CARVE && lib.material(&s.mat).is_none() {
                    cx.warnings.push(format!("track {}: shape from line {} is of {}, which the library lacks", t.name, s.line, s.mat));
                }
                cx.shapes.push(crate::world::shape::compile(&ops, s.size, &|m| or_carve(lib, m)));
                cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin: s.at, rot: 0, age: 0.0, cause, seed });
                shapes += 1;
            }
            // The props a kart smashes, in the passable copy of their
            // material (`Library::ghost_twins`, worldload.rs): the race
            // takes them away as a kart reaches them, so none is ever hit.
            for s in &t.smash {
                let Some(lo) = s.boxes.iter().map(|b| [b[0], b[1], b[2]]).reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]) else { continue };
                let hi = s.boxes.iter().map(|b| [b[3], b[4], b[5]]).fold(lo, |a, b| [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]);
                let mat = crate::library::ghost(&s.mat);
                if lib.material(&mat).is_none() {
                    cx.warnings.push(format!("track {}: no passable {} for a smashable prop", t.name, s.mat));
                    continue;
                }
                let ops: Vec<crate::library::Op> = s
                    .boxes
                    .iter()
                    .map(|b| crate::library::Op::Box { from: [b[0] - lo[0], b[1] - lo[1], b[2] - lo[2]], to: [b[3] - lo[0], b[4] - lo[1], b[5] - lo[2]], mat: mat.clone(), part: None })
                    .collect();
                let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
                cx.shapes.push(crate::world::shape::compile(&ops, size, &|m| or_carve(lib, m)));
                cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin: lo, rot: 0, age: 0.0, cause, seed });
            }
        }
        None if !t.shapes.is_empty() => cx.warnings.push("track: the library has no structure called track_shape, so no shapes".into()),
        None => {}
    }

    let gate = match &t.gate {
        Some(g) => {
            let g = Gate { on: true, light: g.light.clone(), dark: g.dark.clone(), post: g.post.clone(), grid: g.grid, ..Gate::default() };
            lay_gate(cx, &line, &y, half[0] * 2.0, road[0], &g)
        }
        None => String::new(),
    };
    if t.lights > 0.0 {
        let post = t.gate.as_ref().map_or(super::events::circuit::LAMP_POST, |g| g.post.as_str());
        light_road(cx, &line, &y, &half, t.lights, post);
    }

    // The track's own library structures, placed last.
    for s in &t.structures {
        let Some(def) = cx.lib.structure(&s.name) else {
            cx.warnings.push(format!("track {}: unknown structure {}", t.name, s.name));
            continue;
        };
        let seed = cx.seed;
        let k = cx.place(def, Vec2::new(s.at[0], s.at[1]), s.bearing.unwrap_or(0.0), &HashMap::default(), 0.0, seed);
        if let (Some(k), Some(h)) = (k, s.y) {
            let lift = cx.lib.structures[def].anchor[1];
            cx.placed[k].origin[1] = (h / crate::world::gen::VOXEL).round() as i32 - lift;
        }
    }

    let dropped = clear_corridor(cx, &corridor, world_placed);

    // What --validate reports: steep grades away from the jumps, and two
    // stretches crossing too close one over the other.
    let mut lip = vec![false; n];
    for j in &t.jumps {
        for (k, l) in lip.iter_mut().enumerate() {
            *l |= (along[k] - (j.s - j.len - 2.0)).rem_euclid(length) < j.len + 14.0;
        }
    }
    // A flooded channel's banks are steep on purpose too.
    for z in t.zones.iter().filter(|z| z.kind == "flood" || z.kind == "landing") {
        for (k, l) in lip.iter_mut().enumerate() {
            *l |= z.on(along[k]);
        }
    }
    let step = 10usize.min(n / 4).max(1);
    let (mut worst, mut at) = (0.0f32, 0usize);
    for i in 0..n {
        let j = (i + step) % n;
        let run = (0..step).map(|k| line[(i + k) % n].distance(line[(i + k + 1) % n])).sum::<f32>().max(0.5);
        let g = (y[j] - y[i]) / run;
        if !lip[i] && !lip[j] && g.abs() > worst.abs() {
            (worst, at) = (g, i);
        }
    }
    if worst.abs() > STEEP {
        cx.warnings.push(format!(
            "track {}: a {:.0}% grade {:.0} m round, at {:.0},{:.0}: karts slow hard and go light over the top",
            t.name,
            worst.abs() * 100.0,
            along[at],
            line[at].x,
            line[at].y
        ));
    }
    for (p, gap) in tight_crossings(&line, &y) {
        cx.warnings.push(format!("track {}: two stretches cross at {:.0},{:.0} with {gap:.1} m between them; they need {CROSS_CLEAR} m", t.name, p.x, p.y));
    }
    for d in &dropped {
        cx.warnings.push(format!("track {}: {d} stood on the track and was taken out", t.name));
    }

    cx.pois.push(Poi { name: t.name.clone(), pos: line[0], cause: cx.cause });
    let width = half.iter().copied().fold(0.0f32, f32::max) * 2.0;
    let widths: Vec<f32> = half.iter().map(|h| h * 2.0).collect();
    let checkpoints = t.checkpoints.iter().map(|c| c.rem_euclid(length)).collect();
    cx.state.get::<Circuits>().0.push(Circuit {
        name: t.name.clone(),
        points: line,
        heights: y.clone(),
        width,
        widths,
        items: t.items.clone(),
        pads: t.pads.clone(),
        checkpoints,
        jumps: t.jumps.iter().map(|j| ((j.s - j.len).rem_euclid(length), (j.s + JUMP_FLIGHT).rem_euclid(length))).collect(),
        gate: t.gate.as_ref().map(|_| Gate::default().height),
        zones: t.zones.clone(),
        smash: t.smash.clone(),
        walls: if wall.iter().any(|&w| w > 0.0) { wall.clone() } else { Vec::new() },
    });
    let (lo, hi) = y.iter().fold((f32::MAX, f32::MIN), |(a, b), &h| (a.min(h), b.max(h)));
    format!(
        "track {}: {length:.0} m round, {:.0} m from its lowest to highest, steepest grade {:.0}%{gate}{}{}{}{}{}",
        t.name,
        hi - lo,
        worst.abs() * 100.0,
        if decks > 0 { format!(", {decks} m on decks") } else { String::new() },
        if shapes > 0 { format!(", {shapes} shapes") } else { String::new() },
        if bored > 0 { format!(", {bored} m in tunnels") } else { String::new() },
        if t.pads.is_empty() { String::new() } else { format!(", {} pads", t.pads.len()) },
        if dropped.is_empty() { String::new() } else { format!(", {} structures taken out of the corridor", dropped.len()) },
    )
}

struct Road<'a> {
    line: &'a [Vec2],
    y: &'a [f32],
    bank: &'a [f32],
    half: &'a [f32],
    wall: &'a [f32],
    runoff: &'a [f32],
    road: &'a [u8],
    kerb: u8,
    verge: &'a [Option<u8>],
    deck: &'a [bool],
    covered: &'a [bool],
    tunnel: &'a [bool],
    along: &'a [f32],
    length: f32,
    jumps: &'a [Jump],
}

/// Lay the road, kerbs and run-off, and ease the ground past them back to
/// the world's. Each cell goes by its nearest point of the whole line; a cell
/// nearest a deck or tunnel point is left to it unless another stretch of
/// road over or under it has the cell (`geom::claims`), and the ground under
/// a deck is held `UNDER_DECK` below its surface, so the world's land never
/// comes up through a deck.
/// Returns the corridor: each cell of road, kerb, wall and run-off, with the
/// road's height there.
fn grade_road(cx: &mut EventCtx, r: &Road) -> Vec<(usize, f32)> {
    let cause = cx.cause;
    let f = &mut cx.f;
    let outer = |k: usize| r.half[k] + 1.0 + if r.wall[k] > 0.0 { WALL_THICK } else { 0.0 } + r.runoff[k];
    let reach = |k: usize| outer(k) + if r.covered[k] { 0.0 } else { MARGIN };
    let cells = claims(f, r.line, true, reach, |k| r.deck[k] || r.tunnel[k], |k| r.y[k]);
    // The land under a deck that no other stretch of road has.
    for c in cells.iter().filter(|c| c.owner.is_none() && r.deck[c.near.k] && c.near.d < r.half[c.near.k] + 1.5) {
        f.h[c.cell] = f.h[c.cell].min(r.y[c.near.k] - UNDER_DECK);
    }
    let mut corridor = Vec::new();
    for c in cells {
        let (i, Some(Nearest { d, k, side })) = (c.cell, c.owner) else { continue };
        let half = r.half[k];
        if d >= reach(k) {
            continue;
        }
        // The road's surface here, banked: the plane through the centreline
        // tilted by the bank, held level past the edge of the road.
        let across = side.clamp(-half, half);
        let tilt = r.bank[k] * across;
        let target = r.y[k] + tilt + skewed_lip(r, k, across);
        if d < half + 1.0 {
            f.h[i] = target;
            f.surface[i] = if d < half { r.road[k] } else { r.kerb };
            f.occupied[i] = true;
            f.water[i] = 3;
            f.cliff[i] = 0.0;
            f.cause[i] = cause;
            // A river the world ran across the road goes under it.
            f.inland.remove(&(i as u32));
            corridor.push((i, r.y[k]));
        } else if d < outer(k) {
            // Wall (built after this) or run-off: level with the road's edge.
            f.h[i] = target;
            if let Some(v) = r.verge[k] {
                f.surface[i] = v;
            }
            f.occupied[i] = true;
            f.cliff[i] = 0.0;
            f.cause[i] = cause;
            if f.water[i] == 1 {
                f.water[i] = 0;
                f.inland.remove(&(i as u32));
            }
            corridor.push((i, r.y[k]));
        } else {
            let t = smoothstep(reach(k), outer(k), d);
            f.h[i] += (target - f.h[i]) * t;
            f.cliff[i] *= 1.0 - t;
            if t > 0.3 {
                f.occupied[i] = true;
            }
        }
    }
    corridor
}

/// How far past a jump's lip the race's drivers count it as the jump, metres:
/// past where a kart at full speed comes down (about 35 m; `--check` says).
const JUMP_FLIGHT: f32 = 45.0;

/// A jump's height over the road `d` m past the start of its ramp, as
/// tools/course/track_file.py `jump_profile` shapes it (the centreline's
/// heights in track.json already have it): a parabola up to the lip over
/// `len`, then a slip face falling 1 in 2 (at least 3 m long) back to the road.
fn jump_profile(d: f32, height: f32, len: f32) -> f32 {
    let back = (height / 0.5).max(3.0);
    if (0.0..len).contains(&d) {
        height * (d / len).powi(2)
    } else if d >= len && d < len + back {
        height * (1.0 - (d - len) / back)
    } else {
        0.0
    }
}

/// How much higher a skewed jump's ramp is `across` m left of the line at
/// point k than on the line itself: the ramp is the same shape, slid back
/// along the road by `across * tan(skew)` (the right side's lip further on).
fn skewed_lip(r: &Road, k: usize, across: f32) -> f32 {
    let mut dy = 0.0;
    for j in r.jumps.iter().filter(|j| j.skew != 0.0 && j.len > 0.0) {
        let ramp = |s: f32| jump_profile((s - (j.s - j.len)).rem_euclid(r.length), j.height, j.len);
        let s = r.along[k];
        // A point `across` m left sees the lip `across * tan` sooner: the
        // right side's lip is further along.
        dy += ramp(s + across * j.skew.to_radians().tan()) - ramp(s);
    }
    dy
}

/// Paint the boost pads into the road in `mat` (`boost_pad` unless the track
/// names another), the length of each pad and 3 m across.
fn paint_pads(cx: &mut EventCtx, line: &[Vec2], along: &[f32], length: f32, half: &[f32], pads: &[Pad], mat: &str) {
    if pads.is_empty() {
        return;
    }
    let Some(m) = cx.lib.material(mat) else {
        cx.warnings.push(format!("track: the library has no material {mat}, so the pads are unmarked"));
        return;
    };
    let n = line.len();
    for pad in pads {
        let s0 = pad.s.rem_euclid(length);
        let k0 = along.partition_point(|&a| a <= s0).saturating_sub(1);
        let steps = pad.len.ceil() as usize + 1;
        for j in 0..steps {
            let k = (k0 + j) % n;
            let d = (line[(k + 1) % n] - line[(k + n - 1) % n]).normalize_or(Vec2::X);
            let right = Vec2::new(-d.y, d.x);
            let mid = line[k] + right * pad.side.clamp(-1.0, 1.0) * (half[k] - 2.0).max(0.0);
            each_near(&mut cx.f, mid, 2.2, |f, i, _, q| {
                let (a, x) = ((q - line[k]).dot(d), (q - mid).dot(right));
                if a.abs() <= 0.5 && x.abs() < 1.5 {
                    f.surface[i] = m;
                }
            });
        }
    }
}

/// A light panel in a tunnel's roof every this many metres, each
/// `LIGHT_LONG` along the road and `LIGHT_WIDE` across, with a lamp under it.
const BORE_LAMPS: f32 = 8.0;
const LIGHT_LONG: usize = 2;
const LIGHT_WIDE: f32 = 1.5;
/// A tunnel's hollow reaches this far past the edge of the road: the kerb
/// and half a metre more. Its lining is a metre outside that.
const BORE_CLEAR: f32 = 1.5;
/// The hollow runs on this many metres out past each end of a tunnel, so
/// the hill it is bored into is cut back from the portal.
const BORE_OUT: usize = 10;
/// Each portal's frame: how far it stands out beside the hollow and over
/// its roof, metres.
const FRAME_SIDE: f32 = 2.0;
const FRAME_TOP: f32 = 2.0;

/// Bore the tunnels: for each run of `tunnel` points, in pieces, a hollow
/// over the road and its kerbs from the road's height up `height` m, carved
/// out of whatever land is there, the road and kerbs laid as voxels on its
/// floor, and a metre of lining round it (walls, roof, and a slab under the
/// road). The hollow and the road run on `BORE_OUT` m past each end, so the
/// hill is cut back from the portal, and each portal is framed in `portal`.
/// The heightfield is the world's: the land over a tunnel stays.
/// Adds each road cell to `corridor`, and returns how many metres it bored.
#[allow(clippy::too_many_arguments)]
fn bore(cx: &mut EventCtx, line: &[Vec2], along: &[f32], length: f32, y: &[f32], half: &[f32], tunnel: &[bool], defs: &[Tunnel], mats: [&str; 2], corridor: &mut Vec<(usize, f32)>) -> usize {
    if !tunnel.iter().any(|&b| b) {
        return 0;
    }
    let Some(def) = cx.lib.structure(TUNNEL_STRUCTURE) else {
        cx.warnings.push("track: the library has no structure called race_tunnel, so no tunnels".into());
        return 0;
    };
    const V: f32 = crate::world::gen::VOXEL;
    let n = line.len();
    let [road, kerb] = mats;
    // What each point of the line is to the tunnels: 1 bored, 2 the hollow
    // run on out past an end, 3 a portal (the last metre or two of the bore
    // and the first outside it, framed); and which tunnel it belongs to.
    let mut kind = vec![0u8; n];
    let mut owner = vec![0usize; n];
    for k in 0..n {
        if tunnel[k] {
            kind[k] = 1;
            owner[k] = defs.iter().position(|d| in_run(along[k], d.from.rem_euclid(length), d.to.rem_euclid(length))).unwrap_or(0);
        }
    }
    for k in 0..n {
        if !tunnel[k] {
            continue;
        }
        for (side, step) in [(-1i64, (k + n - 1) % n), (1, (k + 1) % n)] {
            if tunnel[step] {
                continue;
            }
            // `k` is an end of the bore; outward is `side`.
            kind[k] = 3;
            for j in 1..=BORE_OUT as i64 {
                let q = (k as i64 + side * j).rem_euclid(n as i64) as usize;
                if tunnel[q] {
                    break;
                }
                kind[q] = if j == 1 { 3 } else { kind[q].max(2) };
                owner[q] = owner[k];
            }
        }
    }
    // A point marked as tunnel that no entry of `tunnels` covers is bored
    // with the type's defaults and lined in the road's material.
    let plain = Tunnel { from: 0.0, to: 0.0, height: six(), mat: road.to_string(), portal: concrete(), light: works_lamp() };
    let of = |k: usize| defs.get(owner[k]).unwrap_or(&plain);
    let dir = |k: usize| (line[(k + 1) % n] - line[(k + n - 1) % n]).normalize_or(Vec2::X);
    // Where the light panels are: `LIGHT_LONG` points every `BORE_LAMPS`
    // metres along each bore, starting half a spacing in.
    let mut light = vec![false; n];
    let mut since = BORE_LAMPS * 0.5;
    let mut left = 0;
    let first = first_free(&kind);
    for step in 0..n {
        let k = (first + step) % n;
        if kind[k] != 1 {
            since = BORE_LAMPS * 0.5;
            left = 0;
            continue;
        }
        if left > 0 {
            light[k] = true;
            left -= 1;
        }
        since += line[k].distance(line[(k + 1) % n]);
        if since >= BORE_LAMPS {
            since = 0.0;
            light[k] = true;
            left = LIGHT_LONG - 1;
        }
    }
    let all_panels: Vec<usize> = (0..n).filter(|&k| light[k] && !light[(k + n - 1) % n]).collect();
    let lib = cx.lib;
    let mut metres = 0;
    for piece in pieces(n, |k| kind[k] != 0) {
        metres += piece.iter().filter(|&&k| tunnel[k]).count();
        let near: Vec<usize> = with_seams(&piece, n).filter(|&k| kind[k] != 0).collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        let (mut ylo, mut yhi) = (f32::MAX, f32::MIN);
        for &k in &near {
            let h = of(k).height;
            let reach = half[k] + BORE_CLEAR + FRAME_SIDE + 1.0;
            lo = lo.min(line[k] - Vec2::splat(reach));
            hi = hi.max(line[k] + Vec2::splat(reach));
            ylo = ylo.min(y[k] - 1.5);
            yhi = yhi.max(y[k] + h + FRAME_TOP + 0.5);
        }
        let mut cols = Columns::new(lo, hi, ylo, yhi, 0);
        // The light panels near this piece, each drawn square about its own
        // point rather than cell by nearest point, so its edges are straight;
        // every piece that reaches one draws it, so none carves it away.
        let panels: Vec<usize> = all_panels.iter().copied().filter(|&c| near.iter().any(|&k| line[k].distance(line[c]) < 4.0)).collect();
        for lz in 0..cols.size[2] {
            for lx in 0..cols.size[0] {
                let q = cols.centre(lx, lz);
                let Some(k) = nearest_of(&near, line, q) else { continue };
                let x = (q - line[k]).dot(Vec2::new(-dir(k).y, dir(k).x)).abs();
                let a = (q - line[k]).dot(dir(k)).abs();
                if a > 0.75 {
                    continue;
                }
                let (ry, hw) = (y[k], half[k]);
                let Tunnel { height: h, mat: lining, portal, light: lamp, .. } = of(k);
                let h = *h;
                let surface = if x < hw || x >= hw + 1.0 { road } else { kerb };
                // The road, its kerbs, and half a metre of road either side
                // so a kart sliding along the kerb clears the wall's voxel
                // steps round a bend.
                let inside = x < hw + BORE_CLEAR;
                match kind[k] {
                    1 if inside => {
                        cols.column(lx, lz, ry - 1.0, ry - V, lining);
                        cols.column(lx, lz, ry - V, ry, surface);
                        let panel = panels.iter().any(|&c| {
                            let d = q - line[c];
                            let (along_c, across_c) = (d.dot(dir(c)), d.dot(Vec2::new(-dir(c).y, dir(c).x)));
                            along_c.abs() < LIGHT_LONG as f32 * 0.5 && across_c.abs() < LIGHT_WIDE * 0.5
                        });
                        if panel {
                            // A light panel hung a voxel below the roof.
                            cols.column(lx, lz, ry, ry + h - V, CARVE);
                            cols.column(lx, lz, ry + h - V, ry + h, lamp);
                        } else {
                            cols.column(lx, lz, ry, ry + h, CARVE);
                        }
                        cols.column(lx, lz, ry + h, ry + h + 1.0, lining);
                    }
                    1 if x < hw + BORE_CLEAR + 1.0 => cols.column(lx, lz, ry - 1.0, ry + h + 1.0, lining),
                    3 if inside => {
                        cols.column(lx, lz, ry - 1.0, ry - V, portal);
                        cols.column(lx, lz, ry - V, ry, surface);
                        cols.column(lx, lz, ry, ry + h, CARVE);
                        cols.column(lx, lz, ry + h, ry + h + FRAME_TOP, portal);
                    }
                    3 if x < hw + BORE_CLEAR + FRAME_SIDE => cols.column(lx, lz, ry - 1.0, ry + h + FRAME_TOP, portal),
                    2 if inside => {
                        cols.column(lx, lz, ry - V, ry, surface);
                        cols.column(lx, lz, ry, ry + h, CARVE);
                    }
                    _ => {}
                }
            }
        }
        cols.place(cx, def, &|m| or_carve(lib, m));
    }
    // The road's cells, for the corridor; nothing grows or settles where
    // the land over the tunnel is lower than its roof (in a gallery, at a
    // portal); and a lamp under each light panel.
    for k in 0..n {
        if !tunnel[k] {
            continue;
        }
        let h = of(k).height;
        let (hw, ry) = (half[k], y[k]);
        each_near(&mut cx.f, line[k], hw + BORE_CLEAR + 1.5, |f, i, d, _| {
            if d < hw + 1.0 {
                corridor.push((i, ry));
            }
            if f.h[i] < ry + h + 2.0 {
                f.occupied[i] = true;
            }
        });
        if light[k] && !light[(k + n - 1) % n] {
            cx.lamps.push(crate::world::fields::WorldLamp {
                pos: glam::Vec3::new(line[k].x, y[k] + h - 0.6, line[k].y),
                reach: 12.0,
                colour: [1.0, 0.86, 0.66],
                strength: 1.0,
                after_dark: false,
            });
        }
    }
    metres
}

/// The first point that is no part of any tunnel, to walk the loop from.
fn first_free(kind: &[u8]) -> usize {
    (0..kind.len()).find(|&k| kind[k] == 0).unwrap_or(0)
}

/// Take out every structure of the world's own (the first `world_placed`)
/// whose footprint covers a corridor cell with its foot less than
/// `HEADROOM` over the road there: the track's corridor is the track's.
/// Returns what was taken out, for the build's warnings.
fn clear_corridor(cx: &mut EventCtx, corridor: &[(usize, f32)], world_placed: usize) -> Vec<String> {
    let n = cx.f.n;
    let cells: HashMap<usize, f32> = corridor.iter().copied().collect();
    // A footprint is in voxels; the corridor's cells are a metre.
    const V: f32 = crate::world::gen::VOXEL;
    const PER_M: i32 = (1.0 / V) as i32;
    let mut gone = vec![false; cx.placed.len()];
    let mut out = Vec::new();
    for (k, p) in cx.placed.iter().enumerate().take(world_placed) {
        let (x0, z0, x1, z1) = p.footprint(&cx.shapes);
        let foot = p.origin[1] as f32 * V;
        let (cx0, cz0) = ((x0.max(0) / PER_M) as usize, (z0.max(0) / PER_M) as usize);
        let (cx1, cz1) = (((x1.max(0) + PER_M - 1) / PER_M) as usize, ((z1.max(0) + PER_M - 1) / PER_M) as usize);
        let hit = (cz0..cz1.min(n)).any(|z| (cx0..cx1.min(n)).any(|x| cells.get(&(z * n + x)).is_some_and(|&y| foot < y + HEADROOM)));
        if hit {
            gone[k] = true;
            let name = cx.lib.structures.get(p.def).map_or("a structure", |d| d.name.as_str());
            out.push(format!("{name} at {:.0},{:.0}", x0 as f32 * V, z0 as f32 * V));
        }
    }
    if out.is_empty() {
        return out;
    }
    // Renumber what stays, and what stands only some of the time with it.
    let mut new_index = vec![usize::MAX; gone.len()];
    let mut next = 0;
    for (k, g) in gone.iter().enumerate() {
        if !g {
            new_index[k] = next;
            next += 1;
        }
    }
    let mut k = 0;
    cx.placed.retain(|_| {
        let keep = !gone[k];
        k += 1;
        keep
    });
    cx.when.retain_mut(|(i, _)| {
        let to = new_index[*i as usize];
        *i = to as u32;
        to != usize::MAX
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::World;

    /// A hash of what a kart touches: each cell of road, kerb, wall and
    /// run-off whose nearest point of the line is not on a deck or in a
    /// tunnel (height to
    /// the centimetre, material by name); the top of every deck and of the
    /// gate, column by column (their pillars and posts reach down to the
    /// world's ground, which is the world's); and every voxel of the track's
    /// shapes and tunnels.
    fn corridor_hash(w: &World, t: &TrackFile) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        let names: HashMap<u8, String> = w.lib.materials.iter().filter_map(|m| w.lib.material(&m.name).map(|id| (id, m.name.clone()))).collect();
        let name = |id: u8| if id == crate::world::shape::CARVE { "carve".to_string() } else { names.get(&id).cloned().unwrap_or_else(|| format!("#{id}")) };
        let f = &w.f;
        let reach = |r: &[f32; 8]| r[4] * 0.5 + 1.0 + if r[5] > 0.0 { WALL_THICK } else { 0.0 } + r[6];
        let mut best: HashMap<usize, (f32, usize)> = HashMap::default();
        for (k, r) in t.line.iter().enumerate() {
            let (p, out) = (Vec2::new(r[0], r[1]), reach(r));
            let (x0, x1) = (((p.x - out) as usize).min(f.n - 1), ((p.x + out) as usize + 1).min(f.n - 1));
            let (z0, z1) = (((p.y - out) as usize).min(f.n - 1), ((p.y + out) as usize + 1).min(f.n - 1));
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let d = Vec2::new(x as f32 + 0.5, z as f32 + 0.5).distance(p);
                    let e = best.entry(z * f.n + x).or_insert((f32::MAX, 0));
                    if d < e.0 {
                        *e = (d, k);
                    }
                }
            }
        }
        let mut cells: Vec<usize> = best.into_iter().filter(|(_, (d, k))| *d < reach(&t.line[*k]) && t.line[*k][7] as u32 & 5 == 0).map(|(i, _)| i).collect();
        cells.sort_unstable();
        for i in cells {
            eat(&(i as u32).to_le_bytes());
            eat(&((f.h[i] * 100.0).round() as i32).to_le_bytes());
            eat(name(f.surface[i]).as_bytes());
        }
        for p in &w.placed {
            let def = &w.lib.structures[p.def].name;
            let whole = def == "track_shape" || def == "race_tunnel";
            if !whole && def != "race_deck" && def != "race_gate" {
                continue;
            }
            eat(def.as_bytes());
            let s = &w.shapes[p.shape];
            let [sx, sy, sz] = s.size;
            for z in 0..sz {
                for x in 0..sx {
                    for y in (0..sy).rev() {
                        let m = s.mat[((y * sz + z) * sx + x) as usize];
                        if m == 0 {
                            continue;
                        }
                        for v in [p.origin[0] + x, p.origin[1] + y, p.origin[2] + z] {
                            eat(&v.to_le_bytes());
                        }
                        eat(name(m).as_bytes());
                        if !whole {
                            break;
                        }
                    }
                }
            }
        }
        h
    }

    /// Peregrine's track as built: what a kart touches is the same however the
    /// engine changes round it. A change that moves the road on purpose
    /// updates this hash in the same commit.
    #[test]
    #[ignore = "builds Peregrine: cargo test --release peregrine_track -- --ignored"]
    fn peregrine_track_is_unchanged() {
        let loaded = crate::worldload::load("peregrine").expect("peregrine builds");
        let t = load(&loaded.world.dir, "track.json").expect("its track");
        let h = corridor_hash(&loaded.world, &t);
        assert_eq!(h, PEREGRINE, "Peregrine's corridor hash is now {h:#018x}");
    }

    /// Ribstrand's lamp posts are built of a passable copy of their bone,
    /// so a kart drives through them; the gantry's posts stay solid.
    #[test]
    #[ignore = "builds Ribstrand: cargo test --release lamp_posts_are_passable -- --ignored"]
    fn lamp_posts_are_passable() {
        let loaded = crate::worldload::load("ribstrand").expect("ribstrand builds");
        let w = &loaded.world;
        let ghost = w.lib.material(&crate::library::ghost("titan_bone")).expect("no passable copy of the post bone");
        assert!(crate::world::store::passable(ghost), "the copy is not passable");
        let bone = w.lib.material("titan_bone").unwrap();
        assert!(!crate::world::store::passable(bone), "the gantry's own bone became passable");
        let used = w.placed.iter().any(|p| w.shapes[p.shape].mat.contains(&ghost));
        assert!(used, "no lamp post is built of it");
    }

    const PEREGRINE: u64 =0x793d_1bf8_b94a_c277;

    /// Whatever the world does round the track (rougher land, a structure
    /// stood on the road), the track comes out the same, and what stood on
    /// the road is taken out.
    #[test]
    #[ignore = "builds Peregrine twice: cargo test --release world_cannot_move_the_track -- --ignored"]
    fn world_cannot_move_the_track() {
        let src = crate::worldload::world_dir("peregrine");
        let dir = std::env::temp_dir().join(format!("lurelia_track_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["world.json", "track.json", "ground.bin", "look.json"] {
            if src.join(f).exists() {
                std::fs::copy(src.join(f), dir.join(f)).unwrap();
            }
        }
        let mut chron: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src.join("chronicle.json")).unwrap()).unwrap();
        let events = chron["events"].as_array_mut().unwrap();
        events.push(serde_json::json!({"year": 10, "event": "terrain", "mode": "add", "base": 0, "hills": 12, "ridges": 6, "rough": 3, "cause": "rougher land"}));
        events.push(serde_json::json!({"year": 20, "event": "place", "structure": "race_deck", "at": {"along": 300}, "cause": "something on the road"}));
        std::fs::write(dir.join("chronicle.json"), serde_json::to_string(&chron).unwrap()).unwrap();

        let (lib, _) = crate::worldload::library_for("peregrine");
        let plain = crate::world::chronicle::build_world(&src, lib).expect("peregrine builds");
        let (lib, _) = crate::worldload::library_for("peregrine");
        let rough = crate::world::chronicle::build_world(&dir, lib).expect("the rough copy builds");
        let _ = std::fs::remove_dir_all(&dir);
        let t = load(&src, "track.json").unwrap();
        assert_eq!(corridor_hash(&plain, &t), corridor_hash(&rough, &t), "the world moved the track");
        assert!(rough.warnings.iter().any(|w| w.contains("taken out")), "nothing taken out of the corridor: {:?}", rough.warnings);
        // The land off the track did change.
        let far = rough.f.idx(30.0, 30.0);
        assert!((rough.f.h[far] - plain.f.h[far]).abs() > 0.01, "the rough land is the same as the plain");
    }
}
