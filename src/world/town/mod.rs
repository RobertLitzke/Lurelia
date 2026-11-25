//! Towns and cities from a handful of inputs: where, how big, which roads
//! come in from the country, and what kind of place each part of it is.
//!
//! `generate` knows nothing of voxels, kits or the library. It reads the
//! ground through `Ground` and returns a `Plan`: streets as polylines with a
//! rank, bridges where a main road crosses water, building footprints on
//! their lots with the edge that faces the street, the plaza, the parks, the
//! street trees, and the walls and fences of the fields round about. The
//! `town` chronicle event turns a plan into a world; a test or a tool can
//! read one directly.
//!
//! The order is the order a town grows in. The roads from the country are
//! found first, across the land and over the narrowest water, and they meet
//! at the centre. A ring goes round the old core (the line of its wall). The
//! sectors between the main roads are districts, each laid out in its own
//! style: a surveyed grid, rings and spokes, the maze of lanes an old town
//! grows, or a suburb's curving roads and closes. Plots come off every
//! frontage back to the line halfway to the next street, so a block fills
//! from all its sides; the fields go round the outside.

pub mod country;
pub mod extent;
pub mod graph;
pub mod grow;
pub mod lots;
pub mod sketch;

use crate::world::noise::{hash32, perlin2, unit};
use glam::Vec2;
use serde::Deserialize;

/// What a plan needs to know of the land.
pub trait Ground {
    fn height(&self, p: Vec2) -> f32;
    /// Open water: sea, river or lake.
    fn wet(&self, p: Vec2) -> bool;
    /// The world's width, metres (square, from 0).
    fn size(&self) -> f32;
    /// Ground something earlier already holds (a placed structure, a road
    /// laid before the town): streets don't run through it and nothing is
    /// built on it.
    fn taken(&self, _p: Vec2) -> bool {
        false
    }
}

/// How a district's streets are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Surveyed blocks, long and narrow, square to a main road.
    Grid,
    /// Rings round the centre crossed by spokes.
    Radial,
    /// Lanes that grew: short bent streets, small odd blocks, few crossings.
    Maze,
    /// Curving roads with closes and loops off them, houses in gardens.
    Suburb,
    /// A few lanes off the through road, houses spaced along them.
    Village,
    /// No streets of its own: cottages strung along the roads already there,
    /// the way a town grows out of its gates.
    Ribbon,
}

/// Street ranks, most important first.
pub mod rank {
    /// A road in from the country, inside the town: the high street.
    pub const MAIN: u8 = 0;
    /// The ring round the old core.
    pub const RING: u8 = 1;
    pub const STREET: u8 = 2;
    pub const LANE: u8 = 3;
    /// A suburban road or close.
    pub const CLOSE: u8 = 4;
    pub const ALLEY: u8 = 5;
    /// A road in from the country, outside the town.
    pub const COUNTRY: u8 = 6;
    /// A street along the water.
    pub const QUAY: u8 = 7;
}

/// Carriageway half-width and pavement width by rank and district style.
pub fn widths(r: u8, style: Style) -> (f32, f32) {
    match (r, style) {
        (rank::MAIN, Style::Maze | Style::Village | Style::Ribbon) => (3.0, 0.9),
        (rank::MAIN, _) => (4.0, 2.0),
        (rank::RING, _) => (3.2, 1.4),
        (rank::STREET, Style::Grid | Style::Radial) => (2.7, 1.2),
        (rank::STREET, Style::Maze) => (2.5, 0.6),
        (rank::STREET, _) => (3.2, 1.2),
        (rank::LANE, _) => (1.8, 0.0),
        (rank::CLOSE, _) => (2.8, 0.0),
        (rank::ALLEY, _) => (1.2, 0.0),
        (rank::QUAY, _) => (3.0, 1.5),
        _ => (2.4, 0.0),
    }
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub centre: Vec2,
    pub radius: f32,
    pub seed: u32,
    /// The old core inside the ring.
    pub core: Style,
    /// Fraction of the radius the core takes (0 for no ring and no core).
    pub core_frac: f32,
    /// The districts round the core, one per sector between main roads,
    /// cycled when there are more sectors than styles.
    pub outer: Vec<Style>,
    /// Where roads arrive from the country: points beyond the edge (or on
    /// it), each joined to the centre by a main road.
    pub gates: Vec<Vec2>,
    /// A wall round the core, along the ring.
    pub wall: bool,
    /// How far the farmland reaches past the edge, metres (0 for none).
    pub fields: f32,
    /// Metres between field boundaries, roughly.
    pub field_size: f32,
    /// Corner buildings fill their corner, whatever its shape; otherwise
    /// they are cut back to rectangles square to their street.
    pub irregular: bool,
    /// What the caller wants in it beyond streets and houses.
    pub features: Features,
    /// How hard the town is drawn to nearby water (0 not at all, 1 usual).
    pub waterfront: f32,
    /// How far it strings out along the roads in (0 not at all, 1 usual).
    pub ribbon: f32,
    /// Metres between houses in the closed blocks (0: terraced wall to wall).
    pub gaps: f32,
    /// Plots thin out toward the edge, except along the roads in.
    pub gradient: bool,
    /// An outline the town stays inside (empty: none).
    pub bounds: Vec<Vec2>,
    /// An outline the farmland fills (empty: `fields` m round the town).
    pub farmland: Vec<Vec2>,
    /// Roads already in the world (a line and its half-width): the fields
    /// hang off them too, and no wall crosses them.
    pub roads: Vec<(Vec<Vec2>, f32)>,
    /// Jetties off the quays (off where the world has its own wharves).
    pub jetties: bool,
}

/// The notable places of a town, each the caller's choice.
#[derive(Clone, Debug)]
pub struct Features {
    /// A market square at the centre.
    pub market: bool,
    /// Smaller squares at junctions in the dense districts.
    pub squares: u32,
    /// Churches: the first on the market, the rest on the squares.
    pub churches: u32,
    pub parks: u32,
    /// Back lanes through deep grid blocks and passages through the old
    /// lanes' blocks.
    pub alleys: bool,
    /// Fences between the back yards of the closed blocks and behind the
    /// gardens of the houses.
    pub yards: bool,
    /// Library structures to stand somewhere particular.
    pub landmarks: Vec<Landmark>,
    /// Library structures for the detached houses (village, ribbon and
    /// suburb plots), picked in turn where they fit: a world's own
    /// cottages instead of footprint houses. `near` is ignored.
    pub houses: Vec<Landmark>,
    /// Up to this many more library houses on the ground the frontages
    /// leave free (back plots, odd corners), each turned to face its
    /// nearest street and kept reachable by a footpath.
    pub infill: u32,
    /// A footpath from every library house's door to the nearest street,
    /// square or path, round the buildings (`Plan::paths`).
    pub paths: bool,
    /// Footprint houses are drawn on the plots no library house takes;
    /// false leaves those plots for `infill`.
    pub footprints: bool,
}

impl Default for Features {
    fn default() -> Self {
        Features { market: true, squares: 0, churches: 1, parks: 0, alleys: true, yards: true, landmarks: Vec::new(), houses: Vec::new(), infill: 0, paths: false, footprints: true }
    }
}

/// A structure the caller places: its footprint (metres, unturned), and
/// near what.
#[derive(Clone, Debug)]
pub struct Landmark {
    pub id: usize,
    pub size: Vec2,
    pub near: Near,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Near {
    /// On the market square.
    Market,
    /// On one of the other squares (the market when there are none).
    Square,
    /// Out at the edge of the town.
    Edge,
    /// On the waterfront.
    Water,
    /// On the highest ground in the town: a castle, a church on its hill.
    High,
    Anywhere,
}

/// Where a landmark went: its centre and quarter turns (clockwise from
/// unturned), so that its front faces what it was put beside.
#[derive(Clone, Debug)]
pub struct Site {
    pub id: usize,
    /// The centre of the turned footprint.
    pub at: Vec2,
    pub turns: u8,
    /// The turned footprint, metres east and south.
    pub size: Vec2,
}

/// One street of the plan.
#[derive(Clone, Debug)]
pub struct Way {
    pub pts: Vec<Vec2>,
    pub rank: u8,
    pub style: Style,
    pub half: f32,
    pub walk: f32,
    /// Per point: over water (the stretch a bridge carries).
    pub wet: Vec<bool>,
    /// False for the edges of a square, which plots front onto but which
    /// are not laid as streets (the square is paved already).
    pub drawn: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Terraced, a shop on the ground floor.
    Shop,
    /// Terraced, homes all the way up.
    Row,
    /// A house in its own garden.
    House,
    Church,
    /// A tower on the town wall.
    Tower,
    /// A warehouse on the quay, or a hall.
    Hall,
    Barn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Roof {
    Gable,
    Hip,
    Flat,
}

/// A building's footprint and what stands on it. `poly` runs round the
/// outline; edge `front` (from point `front` to the next) faces the street.
#[derive(Clone, Debug)]
pub struct Building {
    pub poly: Vec<Vec2>,
    pub front: usize,
    pub kind: Kind,
    pub roof: Roof,
    pub floors: u8,
    pub style: Style,
    pub seed: u32,
    /// The ridge's direction when it should run along the street rather
    /// than along the longest wall.
    pub ridge: Option<Vec2>,
    /// Picks the building's materials. Neighbours along a street mostly
    /// share one, so a row reads as a row and not as confetti.
    pub tone: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// A field wall of heaped stone.
    FieldWall,
    /// A post-and-rail fence.
    Rail,
    /// A garden's back or side fence.
    Garden,
    /// The town wall round the core.
    TownWall,
    /// A hedge between fields.
    Hedge,
}

/// A paved open place: a rectangle `r` m either way along `u` and `w` m
/// either way across it.
#[derive(Clone, Copy, Debug)]
pub struct Square {
    pub c: Vec2,
    pub u: Vec2,
    pub r: f32,
    pub w: f32,
}

impl Square {
    pub fn contains(&self, p: Vec2, grow: f32) -> bool {
        let d = p - self.c;
        d.dot(self.u).abs() <= self.r + grow && d.perp_dot(self.u).abs() <= self.w + grow
    }
}

#[derive(Clone, Debug)]
pub struct Boundary {
    pub pts: Vec<Vec2>,
    pub edge: Edge,
}

#[derive(Default)]
pub struct Plan {
    pub ways: Vec<Way>,
    pub buildings: Vec<Building>,
    pub boundaries: Vec<Boundary>,
    /// Paved open places, the market first.
    pub plazas: Vec<Square>,
    /// Jetties off the quays: from the quay's edge out over the water.
    pub piers: Vec<(Vec2, Vec2)>,
    /// Driveways: from the kerb to a garage door.
    pub drives: Vec<(Vec2, Vec2)>,
    /// Footpaths from library houses' doors to the streets, door first.
    pub paths: Vec<Vec<Vec2>>,
    /// Turning circles at the ends of closes.
    pub turnings: Vec<Vec2>,
    /// Parks, likewise.
    pub parks: Vec<(Vec2, f32)>,
    /// Landmarks placed.
    pub sites: Vec<Site>,
    /// Trees: where, and which kind.
    pub trees: Vec<(Vec2, Tree)>,
    /// Which crop each patch of farmland grows (the event names crops).
    pub crops: country::Crops,
    /// A corner of each field, for its shed (`country::Corner`).
    pub corners: Vec<country::Corner>,
    /// How far the farmland reaches from the centre, metres.
    pub farm: f32,
    pub districts: Districts,
    pub notes: Vec<String>,
    /// Things worth saying in the summary that are not problems.
    pub notes_quiet: Vec<String>,
    pub stats: PlotStats,
}

/// Which of a kit's trees stands at a spot (`TownKit::trees`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tree {
    /// Along a street: the kit's first tree.
    Street,
    /// In a park or a garden: any of the kit's trees.
    Park,
    /// In a field hedge: the kit's last tree.
    Hedge,
}

/// How many plots were built, and how many refused for each reason.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlotStats {
    pub built: usize,
    /// Outside every district.
    pub outside: usize,
    /// Too little depth behind the frontage.
    pub shallow: usize,
    /// Off the edge of the town's raster.
    pub edge: usize,
    /// On ground something else holds.
    pub taken: usize,
    /// On ground that falls too far across the plot.
    pub steep: usize,
}

impl std::fmt::Display for PlotStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} plots built, refused {} outside, {} shallow, {} off the edge, {} taken, {} steep",
            self.built, self.outside, self.shallow, self.edge, self.taken, self.steep
        )
    }
}

/// Which district a point is in: the core inside the ring, else the sector
/// between two main roads, out to a ragged edge.
#[derive(Clone, Debug, Default)]
pub struct Districts {
    pub c: Vec2,
    pub r_core: f32,
    /// The market square's radius (0 without one).
    pub plaza: f32,
    /// How far past the ring the core reaches: to the town wall, a plot's
    /// depth out, when there is one.
    pub wall: f32,
    /// The ring's radius by bearing, when it follows the ground (72 steps
    /// from +x); empty for a ragged circle.
    pub ring: Vec<f32>,
    pub radius: f32,
    pub seed: u32,
    /// Sector boundaries, radians (atan2 of z, x), ascending.
    pub cuts: Vec<f32>,
    /// Index 0 is the core, then one per sector.
    pub styles: Vec<Style>,
    /// Where the town reaches (`extent.rs`).
    pub ext: extent::Extent,
    /// The roads in from the country, centre first.
    pub mains: Vec<Vec<Vec2>>,
}

impl Districts {
    /// The bearings (radians) sector `k` runs between, the second the
    /// greater: the last sector's wraps past a full turn.
    pub fn span(&self, k: usize) -> (f32, f32) {
        let a1 = if k + 1 < self.cuts.len() { self.cuts[k + 1] } else { self.cuts[0] + std::f32::consts::TAU };
        (self.cuts[k], a1)
    }

    /// How far inside (negative) or outside the town's edge a point is.
    pub fn sd(&self, p: Vec2) -> f32 {
        self.ext.sd(p)
    }

    /// 0 deep in the town to 1 at its edge, for loosening plots outward.
    pub fn far(&self, p: Vec2) -> f32 {
        (1.0 + self.sd(p) / (self.radius * 0.6)).clamp(0.0, 1.2)
    }

    pub fn ring_r(&self, d: Vec2) -> f32 {
        if !self.ring.is_empty() {
            let n = self.ring.len();
            let t = (d.y.atan2(d.x).rem_euclid(std::f32::consts::TAU)) / std::f32::consts::TAU * n as f32;
            let (k, f) = (t.floor() as usize % n, t.fract());
            return self.ring[k] * (1.0 - f) + self.ring[(k + 1) % n] * f;
        }
        self.r_core * (1.0 + 0.09 * perlin2(d.x * 1.7, d.y * 1.7 + 3.0, self.seed ^ 0x53))
    }

    pub fn at(&self, p: Vec2) -> Option<u8> {
        if self.sd(p) > 0.0 {
            return None;
        }
        let v = p - self.c;
        let r = v.length();
        let d = v / r.max(1e-3);
        if self.r_core > 0.0 && r < self.ring_r(d) + self.wall {
            return Some(0);
        }
        Some(self.sector(v.y.atan2(v.x)) as u8 + 1)
    }

    pub fn sector(&self, a: f32) -> usize {
        if self.cuts.is_empty() {
            return 0;
        }
        // Sector k runs from cuts[k] to cuts[k+1], the last wrapping round.
        for k in 0..self.cuts.len() {
            if a < self.cuts[k] {
                return (k + self.cuts.len() - 1) % self.cuts.len();
            }
        }
        self.cuts.len() - 1
    }

    pub fn style(&self, d: u8) -> Style {
        self.styles[d as usize]
    }
}

/// A seeded stream of numbers in [0, 1).
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(hash32(seed ^ 0x2545_f491))
    }
    pub fn f(&mut self) -> f32 {
        self.0 = hash32(self.0.wrapping_add(0x9e37_79b9));
        unit(self.0)
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

/// Grow a town.
/// The market square's radius for a town of `radius`, metres.
fn market_radius(radius: f32) -> f32 {
    (radius * 0.1).clamp(15.0, 34.0)
}

/// How far the ground falls across `poly`: its highest corner less its lowest.
pub fn fall(ground: &dyn Ground, poly: &[Vec2]) -> f32 {
    let (lo, hi) = poly.iter().map(|&q| ground.height(q)).fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
    hi - lo
}

/// The quarter turns (clockwise from north) nearest the way `face` points.
pub fn quarter_turns(face: Vec2) -> u8 {
    ((crate::util::bearing(face).to_degrees() + 45.0).rem_euclid(360.0) / 90.0) as u8 % 4
}

/// The unit vector `turns` quarter turns clockwise from north.
pub fn quarter_axis(turns: u8) -> Vec2 {
    let b = turns as f32 * std::f32::consts::FRAC_PI_2;
    Vec2::new(b.sin(), -b.cos())
}

pub fn generate(spec: &Spec, ground: &dyn Ground) -> Plan {
    let mut rng = Rng::new(spec.seed);
    let mut c = dry_centre(spec, ground);
    // A town by wide water has its market on the waterfront.
    if spec.waterfront >= 0.75 && spec.features.market {
        let plaza = market_radius(spec.radius);
        if let Some(w) = extent::waterfront_centre(c, spec.radius, plaza, ground) {
            c = w;
        }
    }
    let mut plan = Plan::default();
    let mut g = graph::Graph::default();

    // Roads from the country. With none given, three or four come in from
    // wherever the land allows.
    let mut gates = spec.gates.clone();
    if gates.is_empty() {
        let n = 3 + (hash32(spec.seed ^ 9) % 2) as usize;
        let a0 = rng.f() * std::f32::consts::TAU;
        for k in 0..n {
            let a = a0 + k as f32 / n as f32 * std::f32::consts::TAU + rng.range(-0.35, 0.35);
            gates.push(c + Vec2::new(a.cos(), a.sin()) * (spec.radius + spec.fields + 40.0));
        }
    }
    let mains = grow::main_roads(c, &gates, ground, spec.seed);
    let ext = extent::build(c, spec.radius, &mains, ground, spec.seed, spec.waterfront, spec.ribbon, &spec.bounds);

    // Districts: the sectors between the main roads where they cross the
    // middle of the town.
    let mid = spec.radius * (0.5 + spec.core_frac * 0.5);
    let mut cuts: Vec<f32> = mains
        .iter()
        .filter_map(|line| {
            let p = line.iter().find(|p| p.distance(c) > mid)?;
            let v = *p - c;
            Some(v.y.atan2(v.x))
        })
        .collect();
    cuts.sort_by(|a, b| a.total_cmp(b));
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.35);
    // Too few roads leave a sector half the town; split the widest.
    while cuts.len() < 3 {
        let mut widest = (0usize, 0.0f32);
        for k in 0..cuts.len().max(1) {
            let a = cuts.get(k).copied().unwrap_or(0.0);
            let b = if cuts.len() <= 1 { a + std::f32::consts::TAU } else if k + 1 < cuts.len() { cuts[k + 1] } else { cuts[0] + std::f32::consts::TAU };
            if b - a > widest.1 {
                widest = (k, b - a);
            }
        }
        let a = cuts.get(widest.0).copied().unwrap_or(0.0) + widest.1 * 0.5;
        cuts.push(if a > std::f32::consts::PI { a - std::f32::consts::TAU } else { a });
        cuts.sort_by(|a, b| a.total_cmp(b));
    }
    let mut styles = vec![spec.core];
    for k in 0..cuts.len() {
        styles.push(if spec.outer.is_empty() { Style::Grid } else { spec.outer[k % spec.outer.len()] });
    }
    let dist = Districts {
        c,
        r_core: spec.radius * spec.core_frac,
        plaza: if spec.features.market { market_radius(spec.radius) } else { 0.0 },
        wall: if spec.wall { 15.0 } else { 0.0 },
        ring: if spec.wall {
            contour_ring(c, spec.radius * spec.core_frac, ground)
        } else if spec.core_frac > 0.0 {
            score_ring(c, spec.core_frac, &ext, market_radius(spec.radius) + 30.0)
        } else {
            Vec::new()
        },
        radius: spec.radius,
        seed: spec.seed,
        cuts,
        styles,
        ext,
        mains: mains.iter().map(|l| l.iter().rev().copied().collect()).collect(),
    };

    // The main roads go in first: inside the edge they are the high
    // streets, outside it the roads to the country.
    for line in &mains {
        // Centre first, so the country road starts where the high street ends.
        let line: Vec<Vec2> = line.iter().rev().copied().collect();
        let mut inside: Vec<Vec2> = Vec::new();
        let mut outside: Vec<Vec2> = Vec::new();
        for &p in &line {
            if dist.sd(p) <= 0.0 {
                inside.push(p);
            } else {
                if outside.is_empty() {
                    if let Some(&l) = inside.last() {
                        outside.push(l);
                    }
                }
                outside.push(p);
            }
        }
        for run in grow::uncrowded(&g, &inside, 7.0) {
            let d = dist.at(run[run.len() / 2]).unwrap_or(0);
            g.insert_line(&run, rank::MAIN, d, 12.0);
        }
        // The country road starts where the high street ended, even though
        // its first metres run on along the same line (which `uncrowded`
        // would otherwise take for a second road beside the first).
        for (k, mut run) in grow::uncrowded(&g, &outside, 7.0).into_iter().enumerate() {
            if k == 0 && !outside.is_empty() && run[0].distance(outside[0]) < 30.0 {
                run[0] = outside[0];
            }
            g.insert_line(&run, rank::COUNTRY, 255, 12.0);
        }
        // Where it crosses water is a bridge the streets keep clear of: a
        // short wet stretch with dry land at both ends, not a road running
        // through the low wet ground along a bank.
        let mut k = 0;
        while k < line.len() {
            if !ground.wet(line[k]) {
                k += 1;
                continue;
            }
            let j0 = k;
            while k < line.len() && ground.wet(line[k]) {
                k += 1;
            }
            let crossing = j0 > 0 && k < line.len() && line[j0 - 1].distance(line[k]) < 90.0;
            let mid = line[(j0 + k - 1) / 2];
            if crossing && g.bridges.iter().all(|b| b.distance(mid) > 40.0) {
                g.bridges.push(mid);
            }
        }
    }

    // The ring round the core: a big town's line of its old wall. A small
    // town's edge is its back lanes, not a ring road.
    if dist.r_core > 0.0 && (spec.radius > 200.0 || spec.wall) {
        let n = 72;
        let ring: Vec<Vec2> = (0..=n)
            .map(|k| {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                let d = Vec2::new(a.cos(), a.sin());
                c + d * dist.ring_r(d)
            })
            .collect();
        for run in grow::bridged_runs(&mut g.bridges, &ring, ground, |_| true, 45.0) {
            g.insert_line(&run, rank::RING, 0, 10.0);
        }
    }

    // The market place at the centre.
    if spec.features.market {
        // Long along the main road through the centre: a market street
        // widened, not a roundabout.
        let u = mains
            .iter()
            .filter_map(|l| l.iter().rev().find(|p| p.distance(c) > dist.plaza * 1.5).map(|p| (*p - c).normalize_or_zero()))
            .next()
            .unwrap_or(Vec2::X);
        plan.plazas.push(Square { c, u, r: dist.plaza * 1.25, w: dist.plaza * 0.7 });
    }

    // A town on a river crosses it near the market: Bridge Street, from the
    // centre over the shortest crossing close by, unless a main road
    // already crosses near the middle.
    if g.bridges.iter().all(|b| b.distance(c) > spec.radius * 0.45) {
        if let Some(line) = grow::bridge_street(c, spec.radius, ground) {
            g.insert_line(&line, rank::MAIN, 0, 10.0);
            g.bridges.push(line[line.len() / 2]);
            plan.notes_quiet.push("bridge street".into());
        }
    }

    // Quays along the water, before the districts so their streets meet them.
    // A quay faces wide water (a harbour, an estuary); a small river's
    // banks stay green.
    let wide = |p: Vec2| (0..24).filter(|&j| {
        let b = j as f32 / 24.0 * std::f32::consts::TAU;
        ground.wet(p + Vec2::new(b.cos(), b.sin()) * 40.0)
    }).count() >= 10;
    let quay_lines: Vec<Vec<Vec2>> = grow::quays(&dist, ground, 6.0)
        .into_iter()
        .filter(|q| spec.radius > 250.0 || q.iter().step_by(4).any(|&p| wide(p)))
        .collect();
    if spec.jetties {
        plan.piers = grow::piers(&quay_lines, ground, &mut rng, c, spec.radius * 0.45);
    }
    for q in quay_lines {
        for run in grow::uncrowded(&g, &q, 16.0) {
            if run.len() >= 4 {
                let d = dist.at(run[run.len() / 2]).unwrap_or(0);
                g.insert_line(&run, rank::QUAY, d, 14.0);
            }
        }
    }

    // Each district in its own style, core first.
    for d in 0..dist.styles.len() as u8 {
        let style = dist.style(d);
        grow::fill(&mut g, &dist, d, style, ground, spec, &mut rng);
    }
    g.prune(26.0);
    grow::reach_on(&mut g, ground);
    let canal_bridges = grow::bridge_canals(&mut g, ground);
    if canal_bridges > 0 {
        plan.notes_quiet.push(format!("{canal_bridges} canal bridges"));
    }
    grow::link_edge_ends(&mut g, &dist, ground);
    g.prune(40.0);
    // Streets left hanging out at the edge go; the country roads stay.
    g.prune_outer(|p| dist.sd(p) > -0.15 * dist.radius, rank::COUNTRY);
    g.hand_over(rank::MAIN, rank::COUNTRY);
    // A close ends in a turning circle.
    {
        let deg = g.degrees();
        let close_end = |n: u32| g.segs.iter().any(|s| s.alive && s.rank == rank::CLOSE && (s.a == n || s.b == n));
        plan.turnings = g.ends.iter().filter(|&&n| deg[n as usize] == 1 && close_end(n)).map(|&n| g.nodes[n as usize]).collect();
    }

    // Streets out of the graph.
    for (pts, r, d) in g.ways() {
        let style = if d == 255 { Style::Village } else { dist.styles.get(d as usize).copied().unwrap_or(Style::Village) };
        // Inside a wall the ring is the lane behind it, not a boulevard.
        let (half, walk) = if spec.wall && r == rank::RING { (2.4, 0.0) } else { widths(r, style) };
        let wet = pts.iter().map(|&p| ground.wet(p)).collect();
        plan.ways.push(Way { pts, rank: r, style, half, walk, wet, drawn: true });
    }

    // A town wall just outside the ring, broken where streets go through.
    if spec.wall && dist.r_core > 0.0 {
        plan.boundaries.extend(lots::town_wall(&dist, &plan.ways, ground));
    }

    lots::build(&mut plan, &dist, spec, ground, &mut rng);
    if spec.fields > 0.0 {
        country::fields(&mut plan, &dist, spec, ground);
    }
    plan.districts = dist;
    plan
}

/// The centre the author asked for, or the nearest point to it with dry,
/// level ground round it: a market place stands beside a river, not in it.
fn dry_centre(spec: &Spec, ground: &dyn Ground) -> Vec2 {
    let ok = |p: Vec2| {
        let h0 = ground.height(p);
        (0..12).all(|k| {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let q = p + Vec2::new(a.cos(), a.sin()) * 28.0;
            !ground.wet(q) && (ground.height(q) - h0).abs() < 3.5
        }) && !ground.wet(p)
    };
    if ok(spec.centre) {
        return spec.centre;
    }
    for ring in 1..40 {
        let r = ring as f32 * 6.0;
        let n = 8 + ring * 4;
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            let p = spec.centre + Vec2::new(a.cos(), a.sin()) * r;
            if ok(p) {
                return p;
            }
        }
    }
    spec.centre
}

/// A ring round `c` near radius `r0` that keeps to one height: on a hill
/// the wall goes round the hill, not round a compass circle. 72 radii.
fn contour_ring(c: Vec2, r0: f32, ground: &dyn Ground) -> Vec<f32> {
    let n = 72;
    let dirs: Vec<Vec2> = (0..n).map(|k| {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        Vec2::new(a.cos(), a.sin())
    }).collect();
    let target = dirs.iter().map(|&d| ground.height(c + d * r0)).sum::<f32>() / n as f32;
    let raw: Vec<f32> = dirs
        .iter()
        .map(|&d| {
            let mut best = (f32::MAX, r0);
            let mut r = r0 * 0.75;
            while r <= r0 * 1.3 {
                let e = (ground.height(c + d * r) - target).abs() + (r - r0).abs() * 0.02;
                if e < best.0 && !ground.wet(c + d * r) {
                    best = (e, r);
                }
                r += 2.0;
            }
            best.1
        })
        .collect();
    // Smoothed round the ring, so it bends rather than zigzags.
    (0..n)
        .map(|k| (-3i32..=3).map(|j| raw[(k as i32 + j).rem_euclid(n as i32) as usize]).sum::<f32>() / 7.0)
        .collect()
}

/// The core's ring where the town's score crosses `frac`: like the town
/// itself it runs out along the water and the roads, and stops short of the
/// water. At least `least` m from the centre. 72 radii.
fn score_ring(c: Vec2, frac: f32, ext: &extent::Extent, least: f32) -> Vec<f32> {
    let n = 72;
    let raw: Vec<f32> = (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            let d = Vec2::new(a.cos(), a.sin());
            let mut r = least;
            while r < ext.reach {
                let s = ext.score(c + d * r);
                if !s.is_finite() {
                    // Water: the ring keeps a plot's depth back from it.
                    r = (r - 14.0).max(least);
                    break;
                }
                if s >= frac {
                    break;
                }
                r += 2.0;
            }
            r
        })
        .collect();
    (0..n)
        .map(|k| (-3i32..=3).map(|j| raw[(k as i32 + j).rem_euclid(n as i32) as usize]).sum::<f32>() / 7.0)
        .collect()
}
