use crate::world::chronicle::hydro::{self, Cut, Grid, Inflow};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use serde::Deserialize;

/// Running water, given time: every stream on the land cuts down toward the
/// sea, the big ones fastest, and the valleys that result branch and wind
/// the way real ones do because the water chose them. Nothing here is a
/// drawn line. To steer it, shape the land first (`terrain`: a `course` is a
/// sag the main river will find) and say what arrives from beyond the map
/// (`inflow`).
///
/// It cuts back from the sea, so few `iters` leave gorges near the coast and
/// an untouched plateau behind them; many reach the whole country.
///
/// With `name` it names the biggest river it made, for later events to
/// build on: `<name>_source`, `<name>_1` to `<name>_9` down its length, and
/// `<name>_mouth`, which faces the way the river was running when it reached
/// the water. A `watercourse` given those as `via` runs down the valley this
/// cut for it.
#[derive(Deserialize, Debug)]
pub struct Drainage {
    #[serde(default)]
    name: Option<String>,
    /// How many points down the named river to name (9: `_1` to `_9`). A
    /// long winding river wants more, or a `river` drawn through them cuts
    /// its corners.
    #[serde(default = "default_points")]
    points: usize,
    #[serde(default = "default_iters")]
    iters: u32,
    /// How hard it cuts: 0.1 notches, 0.4 gorges.
    #[serde(default = "default_strength")]
    strength: f32,
    /// How much harder a big river cuts than a small one.
    #[serde(default = "default_exponent")]
    exponent: f32,
    /// How fast steep valley sides slump back (0 to 0.25).
    #[serde(default = "default_diffuse")]
    diffuse: f32,
    /// Rivers arriving from beyond the map.
    #[serde(default)]
    inflow: Vec<Source>,
    /// Places water ends at above the sea: a lake with no outlet, a swallow
    /// hole. The river that reaches one stops there.
    #[serde(default)]
    sinks: Vec<Pos>,
    /// Water may leave over the world's edge as well as into the sea: for a
    /// country with no coast.
    #[serde(default)]
    edges: bool,
    /// How far streams stray from the straight way down, m: 0 gives ruled
    /// lines on even ground, 2 a wandering stream.
    #[serde(default = "default_wander")]
    wander: f32,
    /// The catchment a stream needs before it cuts, square metres.
    #[serde(default = "default_source")]
    source: f32,
    /// Metres to a side of the cells it works in.
    #[serde(default = "default_step")]
    step: usize,
}

#[derive(Deserialize, Debug)]
pub struct Source {
    pub at: Pos,
    /// The catchment behind it, square metres: 1e6 is a square kilometre.
    pub area: f32,
}

fn default_points() -> usize {
    9
}
fn default_iters() -> u32 {
    24
}
fn default_strength() -> f32 {
    0.25
}
fn default_exponent() -> f32 {
    0.45
}
fn default_diffuse() -> f32 {
    0.12
}
fn default_wander() -> f32 {
    1.6
}
fn default_source() -> f32 {
    2500.0
}
fn default_step() -> usize {
    2
}

impl Process for Drainage {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let inflow: Vec<Inflow> = self.inflow.iter().map(|s| Inflow { at: cx.pos(&s.at), area: s.area }).collect();
        let sinks: Vec<glam::Vec2> = self.sinks.iter().map(|p| cx.pos(p)).collect();
        let (seed, cause) = (cx.seed, cx.cause);
        let f = &mut cx.f;
        let mut g = Grid::from(f, self.step);
        let before = g.h.clone();
        let cut = Cut { iters: self.iters, strength: self.strength, exponent: self.exponent, diffuse: self.diffuse.clamp(0.0, 0.25), edges: self.edges, wander: self.wander, source: self.source, sinks };
        let flow = hydro::cut(&mut g, f.sea, &cut, seed, &inflow);
        let moved = g.apply_delta(&before, f, cause);
        let deepest = g.h.iter().zip(&before).map(|(a, b)| b - a).fold(0.0, f32::max);
        let streams = hydro::reaches(&g, &flow, 20_000.0);
        let longest = streams.iter().map(|r| r.length()).fold(0.0, f32::max);
        let mut named = String::new();
        if let (Some(name), Some(r)) = (&self.name, streams.iter().max_by(|a, b| a.area[a.area.len().saturating_sub(2)].partial_cmp(&b.area[b.area.len().saturating_sub(2)]).unwrap())) {
            let n = r.pts.len();
            let way = (r.pts[n - 1] - r.pts[n.saturating_sub(16)]).normalize_or_zero();
            let faces = crate::util::bearing(way).to_degrees().rem_euclid(360.0);
            cx.seat(&format!("{name}_source"), r.pts[0], None);
            cx.seat(&format!("{name}_mouth"), r.pts[n - 1], Some(faces));
            for q in 1..=self.points.max(1) {
                cx.seat(&format!("{name}_{q}"), r.pts[(n - 1) * q / (self.points.max(1) + 1)], None);
            }
            named = format!("; {name} runs {:.0},{:.0} -> {:.0},{:.0}, arriving on bearing {faces:.0}", r.pts[0].x, r.pts[0].y, r.pts[n - 1].x, r.pts[n - 1].y);
        }
        format!("{} passes: {moved} m2 cut, up to {deepest:.0} m deep; {} streams, the longest {longest:.0} m{named}", self.iters, streams.len())
    }
}
