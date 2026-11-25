use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::city::{self, town_default, BuildingDef, CityFile, DeckDef, StreetDef};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::hash32;
use crate::world::route;
use crate::world::town::{self, rank, Edge, Kind, Roof, Style};
use glam::Vec2;
use serde::Deserialize;

/// A town or city grown from its inputs (`world/town/`): the roads in from
/// the country, a core and the districts round it each in a style, plots on
/// every frontage, and fields round the outside. Buildings are drawn from
/// their footprints by the city code, the way an imported city is.
#[derive(Deserialize, Debug)]
pub struct Town {
    name: String,
    at: Pos,
    radius: f32,
    /// The core inside the ring road: maze, radial, grid, village.
    #[serde(default = "d_core")]
    core: Style,
    /// Fraction of the radius the core takes; 0 for no ring.
    #[serde(default = "d_core_frac")]
    core_frac: f32,
    /// One style per sector between main roads, cycled.
    #[serde(default)]
    districts: Vec<Style>,
    /// Where roads come in from the country.
    #[serde(default)]
    gates: Vec<Pos>,
    /// A wall round the core.
    #[serde(default)]
    wall: bool,
    /// How far the fields reach past the edge, metres.
    #[serde(default)]
    fields: f32,
    #[serde(default = "d_field_size")]
    field_size: f32,
    /// Corner buildings fill their corner whatever its shape (true), or
    /// are cut back to rectangles (false).
    #[serde(default = "yes")]
    irregular: bool,
    /// How hard the town is drawn to nearby water (0 not at all, 1 usual).
    #[serde(default = "one")]
    waterfront: f32,
    /// How far it strings out along the roads in (0 not at all, 1 usual).
    #[serde(default = "one")]
    ribbon: f32,
    /// Metres between houses in the closed blocks, and at least this
    /// between library houses; 1.6 for a town under 170 m radius when left
    /// out, else terraced wall to wall.
    #[serde(default)]
    gaps: Option<f32>,
    /// Plots thin out toward the edge, except along the roads in.
    #[serde(default = "yes")]
    gradient: bool,
    /// An outline the town stays inside: the ground an older settlement
    /// held, when a town replaces it.
    #[serde(default)]
    bounds: Vec<Pos>,
    /// An outline the fields fill, instead of `fields` m all round.
    #[serde(default)]
    farmland: Vec<Pos>,
    /// Jetties off the quays; false where the world has wharves of its own.
    #[serde(default = "yes")]
    jetties: bool,
    /// A market square at the centre.
    #[serde(default = "yes")]
    market: bool,
    /// More squares, at the busiest junctions of the dense districts.
    #[serde(default)]
    squares: u32,
    /// Churches: the first on the market, the rest on the other squares.
    /// One when left out.
    #[serde(default)]
    churches: Option<u32>,
    /// Parks: none under 180 m radius, one to 280 m, two beyond, when left
    /// out.
    #[serde(default)]
    parks: Option<u32>,
    /// Back lanes and passages through the blocks.
    #[serde(default = "yes")]
    alleys: bool,
    /// Fences between back yards and behind gardens.
    #[serde(default = "yes")]
    yards: bool,
    /// Library structures to stand on the market, a square, the edge, the
    /// waterfront or anywhere: `{"structure": "market_cross", "near": "market"}`.
    #[serde(default)]
    landmarks: Vec<LandmarkDef>,
    /// Library structures for the detached houses, instead of footprint
    /// houses: a world's own cottages. They turn only by quarter turns, so
    /// they stand where a street runs near north-south or east-west, and
    /// footprint houses take the rest.
    #[serde(default)]
    houses: Vec<String>,
    /// Up to this many more of `houses` on the ground the street plots
    /// leave free (back plots, corners), each facing its nearest street
    /// and reached by a footpath; for a small top where few streets run
    /// square to the compass.
    #[serde(default)]
    infill: u32,
    /// A footpath 1.4 m wide (the kit's `path` street, else its `lane`)
    /// from every library structure's door to the streets, routed round
    /// the buildings and joining the paths already there.
    #[serde(default)]
    paths: bool,
    /// The market is a village green: left as grass on the lie of the
    /// land, the lanes running on across it, not a levelled, paved square.
    #[serde(default)]
    green: bool,
    /// A town on stilts over shallow water: the route kit every street is
    /// laid with instead (a boardwalk, `bayou_walk`). Water up to
    /// `stilt_depth` m deep counts as ground for streets and plots, deeper
    /// channels stay open and the walks span them; nothing is graded or
    /// paved, and only library structures (`houses`, `landmarks`) are
    /// built, each joined to its walk by a short gangway.
    #[serde(default)]
    walkways: Option<String>,
    #[serde(default = "d_stilt_depth")]
    stilt_depth: f32,
    /// Footprint houses on the plots a library house can't take (false:
    /// those plots stay empty, and the town is all library structures,
    /// which an `abandon` event can age).
    #[serde(default = "yes")]
    footprints: bool,
    /// Region names for the field parcels, picked in turn (for biomes).
    #[serde(default)]
    crops: Vec<String>,
    /// The chance a field has a shed or shack in its corner, as on a
    /// `fields` event.
    #[serde(default)]
    shacks: super::fields::Shacks,
    /// The city kit (`library/cities/`) whose `town` section says what the
    /// town is made of.
    kit: String,
    /// Wall and roof materials by district style, over the kit's.
    #[serde(default)]
    walls: HashMap<String, Vec<String>>,
    #[serde(default)]
    roofs: HashMap<String, Vec<String>>,
    /// The region painted over the town (for its biome).
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    seed: u32,
}

fn d_core() -> Style {
    Style::Maze
}
fn d_core_frac() -> f32 {
    0.35
}
#[derive(Deserialize, Debug)]
struct LandmarkDef {
    structure: String,
    #[serde(default = "anywhere")]
    near: town::Near,
    /// A name for the map and the compass.
    #[serde(default)]
    poi: Option<String>,
}

fn anywhere() -> town::Near {
    town::Near::Anywhere
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}
fn d_stilt_depth() -> f32 {
    2.0
}
fn d_field_size() -> f32 {
    70.0
}

fn key(s: Style) -> &'static str {
    match s {
        Style::Grid => "grid",
        Style::Radial => "radial",
        Style::Maze => "maze",
        Style::Suburb => "suburb",
        Style::Village => "village",
        Style::Ribbon => "ribbon",
    }
}

/// The ground under a plan, read from the fields. For a town on stilts
/// (the second field: how deep it may stand), shallow water is ground at
/// the water's level.
pub(super) struct FieldGround<'a>(pub &'a Fields, pub Option<f32>);

/// Whether cell `i` is water or within 0.3 m of the sea's level.
fn wet_cell(f: &Fields, i: usize) -> bool {
    f.water[i] == 1 || f.h[i] < f.sea + 0.3
}

impl town::Ground for FieldGround<'_> {
    fn height(&self, p: Vec2) -> f32 {
        let h = self.0.height(p.x, p.y);
        if self.1.is_some() {
            h.max(self.0.sea)
        } else {
            h
        }
    }
    fn wet(&self, p: Vec2) -> bool {
        if p.x < 0.0 || p.y < 0.0 || p.x >= self.0.size || p.y >= self.0.size {
            return true;
        }
        let i = self.0.idx(p.x, p.y);
        match self.1 {
            Some(depth) => self.0.water[i] == 1 && self.0.sea - self.0.h[i] > depth,
            None => wet_cell(self.0, i),
        }
    }
    fn size(&self) -> f32 {
        self.0.size
    }
    fn taken(&self, p: Vec2) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && p.x < self.0.size && p.y < self.0.size && self.0.occupied[self.0.idx(p.x, p.y)]
    }
}

impl Process for Town {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed ^ self.seed, cx.cause);
        let lib = cx.lib;
        let Some(kit) = lib.city_kit(&self.kit) else {
            cx.warnings.push(format!("town {}: no city kit called {}", self.name, self.kit));
            return "skipped".into();
        };
        let tk = &kit.town;
        let c = cx.pos(&self.at);
        let gates: Vec<Vec2> = self.gates.iter().map(|g| cx.pos(g)).collect();
        let bounds: Vec<Vec2> = self.bounds.iter().map(|g| cx.pos(g)).collect();
        let farmland: Vec<Vec2> = self.farmland.iter().map(|g| cx.pos(g)).collect();
        // Each library structure's size in voxels, compiled once.
        let mut sizes: HashMap<usize, [i32; 3]> = HashMap::default();
        let mut size_of = |def: usize| *sizes.entry(def).or_insert_with(|| crate::world::chronicle::compile_structure(lib, def, &[]).size);
        let mut footprint = |def: usize| {
            let size = size_of(def);
            Vec2::new(size[0] as f32, size[2] as f32) * crate::world::gen::VOXEL
        };
        let mut landmarks = Vec::new();
        for l in &self.landmarks {
            let Some(def) = lib.structure(&l.structure) else {
                cx.warnings.push(format!("town {}: unknown structure {}", self.name, l.structure));
                continue;
            };
            landmarks.push(town::Landmark { id: def, size: footprint(def), near: l.near });
        }
        let mut houses = Vec::new();
        for name in &self.houses {
            let Some(def) = lib.structure(name) else {
                cx.warnings.push(format!("town {}: unknown structure {}", self.name, name));
                continue;
            };
            houses.push(town::Landmark { id: def, size: footprint(def), near: town::Near::Anywhere });
        }
        let features = town::Features {
            market: self.market,
            squares: self.squares,
            churches: self.churches.unwrap_or(1),
            parks: self.parks.unwrap_or(if self.radius > 280.0 {
                2
            } else if self.radius > 180.0 {
                1
            } else {
                0
            }),
            alleys: self.alleys,
            yards: self.yards,
            landmarks,
            houses,
            infill: self.infill,
            paths: self.paths,
            footprints: self.footprints && self.walkways.is_none(),
        };
        let spec = town::Spec {
            centre: c,
            radius: self.radius,
            seed,
            core: self.core,
            core_frac: self.core_frac,
            outer: self.districts.clone(),
            gates,
            wall: self.wall,
            fields: self.fields,
            field_size: self.field_size,
            irregular: self.irregular,
            features,
            waterfront: self.waterfront,
            ribbon: self.ribbon,
            gaps: self.gaps.unwrap_or(if self.radius < 170.0 { 1.6 } else { 0.0 }),
            gradient: self.gradient,
            bounds,
            farmland,
            roads: cx.state.get::<route::RoadLines>().0.clone(),
            jetties: self.jetties,
        };
        let stilts = self.walkways.as_ref().map(|_| self.stilt_depth);
        let plan = town::generate(&spec, &FieldGround(&cx.f, stilts));
        if let Ok(dir) = std::env::var("VG_TOWN_SKETCH") {
            let path = std::path::Path::new(&dir).join(format!("{}_plan.png", self.name.to_lowercase()));
            if let Err(e) = town::sketch::write(&plan, &FieldGround(&cx.f, stilts), plan.farm.max(plan.districts.ext.reach + 40.0), &path) {
                cx.warnings.push(format!("town {}: sketch: {e}", self.name));
            }
        }
        let c = plan.districts.c;
        for n in &plan.notes {
            cx.warnings.push(format!("town {}: {n}", self.name));
        }

        // Regions first, so the fields and the town take their biomes.
        let town_region = cx.f.region_id(self.region.as_deref().unwrap_or("town"));
        let crop_ids: Vec<u8> = self.crops.iter().map(|n| cx.f.region_id(n)).collect();
        let park_id = cx.f.region_id(town_default::PARK_REGION);
        let dist = &plan.districts;
        let reach = plan.farm.max(self.radius * 1.3);
        let f = &mut cx.f;
        let (lo, hi) = ((c - Vec2::splat(reach)).max(Vec2::ZERO), (c + Vec2::splat(reach)).min(Vec2::splat(f.size - 1.0)));
        for z in lo.y as usize..hi.y as usize {
            for x in lo.x as usize..hi.x as usize {
                let i = z * f.n + x;
                let p = f.centre(i);
                if dist.at(p).is_some() {
                    f.region[i] = town_region;
                } else if !crop_ids.is_empty() {
                    let k = plan.crops.at(p);
                    if k != 255 {
                        f.region[i] = crop_ids[k as usize % crop_ids.len()];
                    }
                }
            }
        }
        for &(p, r) in &plan.parks {
            crate::world::chronicle::geom::each_near(&mut cx.f, p, r, |f: &mut Fields, i, _, _| f.region[i] = park_id);
        }

        // What is laid on the ground: all of the plan, or for a town on
        // stilts none of it (its streets are boardwalks, further down).
        let nothing = town::Plan::default();
        let laid = if stilts.is_none() { &plan } else { &nothing };

        // Grade the streets: each one's ground smoothed along it, and the
        // land each side eased down to meet it.
        for way in laid.ways.iter().filter(|w| w.drawn) {
            grade(&mut cx.f, way, cause);
        }
        // Streets stop at the edge of a paved square; a green they cross.
        let paved: &[town::Square] = if self.green { &[] } else { &plan.plazas };
        // The plaza, paved and level.
        let plaza_mat = cx.mat(tk.plaza_mat());
        for sq in laid.plazas.iter().filter(|_| !self.green) {
            let base = cx.f.height(sq.c.x, sq.c.y);
            let reach = sq.r.max(sq.w) + 6.0;
            crate::world::chronicle::geom::each_near(&mut cx.f, sq.c, reach * 1.5, |f: &mut Fields, i, _, p| {
                if f.water[i] == 1 {
                    return;
                }
                if sq.contains(p, 0.0) {
                    f.h[i] = base;
                    f.surface[i] = plaza_mat;
                    f.occupied[i] = true;
                } else {
                    for k in 1..=4 {
                        if sq.contains(p, k as f32) {
                            let t = 1.0 - k as f32 / 5.0;
                            f.h[i] += (base - f.h[i]) * t;
                            break;
                        }
                    }
                }
            });
        }

        // The city file the footprint code reads.
        let mut cf = CityFile::default();
        let pick = |list: &[String], h: u32| -> String { list[(h as usize) % list.len()].clone() };
        // Wall and roof materials by district style, in `STYLE_KEYS` order:
        // the event's own list for a style, else the kit's.
        let walls: Vec<Vec<String>> = city::STYLE_KEYS.iter().map(|k| city::TownKit::walls_for(&[&self.walls, &tk.walls], k)).collect();
        let roofs: Vec<Vec<String>> = city::STYLE_KEYS.iter().map(|k| city::TownKit::roofs_for(&[&self.roofs, &tk.roofs], k)).collect();
        let style_at = |s: Style| city::STYLE_KEYS.iter().position(|k| *k == key(s)).unwrap_or(0);
        let trims = tk.trim_list();
        let trees = tk.tree_list();
        let footprints = stilts.is_none() && self.footprints;
        for (k, b) in plan.buildings.iter().enumerate().filter(|_| footprints) {
            let h = b.seed;
            let (walls, roofs) = (&walls[style_at(b.style)], &roofs[style_at(b.style)]);
            let floor = match b.kind {
                Kind::Church => 4.0,
                Kind::Shop => 3.2,
                _ => 3.0,
            };
            let style = match b.kind {
                Kind::Shop => "shop",
                Kind::Tower => "block",
                Kind::Row => "block",
                Kind::Hall => "industrial",
                Kind::House => "house",
                Kind::Church => "church",
                Kind::Barn => "industrial",
            };
            let wall = if matches!(b.kind, Kind::Church | Kind::Tower) { tk.church.clone().unwrap_or_else(|| pick(walls, hash32(h ^ 1))) } else { pick(walls, hash32(h ^ 1)) };
            let roof_mat = if b.roof == Roof::Flat { tk.flat_roof.clone().unwrap_or_else(|| pick(roofs, hash32(b.tone ^ 2))) } else { pick(roofs, hash32(b.tone ^ 2)) };
            cf.buildings.push(BuildingDef {
                id: ((seed as u64) << 32) | k as u64,
                style: style.into(),
                poly: b.poly.iter().map(|p| [p.x, p.y]).collect(),
                holes: Vec::new(),
                height: b.floors as f32 * floor + if b.kind == Kind::Church { 2.0 } else { 0.0 },
                // A terrace's roofs are pitched low, across the plot's depth:
                // steep little roofs at every angle read as stripes from above.
                roof_rise: match (b.kind, b.ridge) {
                    (Kind::Row | Kind::Shop, Some(u)) => {
                        let v = Vec2::new(-u.y, u.x);
                        let (lo, hi) = b.poly.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.dot(v)), h.max(p.dot(v))));
                        ((hi - lo) * 0.5 * 0.45).clamp(1.2, 3.4)
                    }
                    _ => 0.0,
                },
                floor,
                roof: match b.roof {
                    Roof::Gable => "gable",
                    Roof::Hip => "hip",
                    Roof::Flat => "flat",
                }
                .into(),
                wall,
                roof_mat,
                trim: pick(&trims, hash32(h ^ 3)),
                door: b.front,
                ridge: b.ridge.map(|v| [v.x, v.y]),
                // Terraced roofs barely overhang, or each eave laps its
                // neighbour's roof.
                eave: matches!(b.kind, Kind::Row | Kind::Shop).then_some(0.15),
            });
        }
        let mut bridges = 0;
        for way in laid.ways.iter().filter(|w| w.drawn) {
            let mat = street_mat(tk, way);
            // Dry runs are streets; wet runs become a deck on piers.
            let mut run: Vec<[f32; 2]> = Vec::new();
            let mut wet_run: Vec<Vec2> = Vec::new();
            let n = way.pts.len();
            for k in 0..n {
                let p = way.pts[k];
                // A square is paved all over: the street runs to its edge
                // and a metre in, and starts again where it leaves.
                if paved.iter().any(|sq| sq.contains(p, -1.0)) {
                    if let Some(&[x, z]) = run.last() {
                        let a = Vec2::new(x, z);
                        let e = edge_point(paved, a, p);
                        run.push([e.x, e.y]);
                    }
                    if run.len() >= 2 {
                        cf.streets.push(StreetDef { pts: std::mem::take(&mut run), width: way.half * 2.0, mat: mat.clone(), walk: way.walk, strip: 0.0 });
                    }
                    run.clear();
                    continue;
                }
                if way.wet[k] {
                    if wet_run.is_empty() && k > 0 {
                        wet_run.push(way.pts[k - 1]);
                    }
                    wet_run.push(p);
                    if run.len() >= 2 {
                        cf.streets.push(StreetDef { pts: std::mem::take(&mut run), width: way.half * 2.0, mat: mat.clone(), walk: way.walk, strip: 0.0 });
                    }
                    run.clear();
                } else {
                    if !wet_run.is_empty() {
                        wet_run.push(p);
                        bridge(&mut cf, &cx.f, &wet_run, way.half);
                        bridges += 1;
                        wet_run.clear();
                    }
                    if run.is_empty() && k > 0 && paved.iter().any(|sq| sq.contains(way.pts[k - 1], -1.0)) {
                        let e = edge_point(paved, p, way.pts[k - 1]);
                        run.push([e.x, e.y]);
                    }
                    run.push([p.x, p.y]);
                }
            }
            if run.len() >= 2 {
                cf.streets.push(StreetDef { pts: run, width: way.half * 2.0, mat: mat.clone(), walk: way.walk, strip: 0.0 });
            }
            // A street that ends out on the water (it met another there, mid
            // river) still carries its half of the bridge.
            if wet_run.len() >= 2 {
                bridge(&mut cf, &cx.f, &wet_run, way.half);
                bridges += 1;
            }
        }
        // Turning circles: a disc of the close's surface.
        let close = tk.streets.get("close").cloned().unwrap_or_else(|| town_default::STREET.to_string());
        for &p in &laid.turnings {
            cf.streets.push(StreetDef { pts: vec![[p.x - 0.1, p.y], [p.x + 0.1, p.y]], width: 14.0, mat: close.clone(), walk: 0.0, strip: 0.0 });
        }
        // Driveways.
        let drive = tk.streets.get("drive").cloned().unwrap_or_else(|| town_default::DRIVE.to_string());
        for &(a, b) in &laid.drives {
            cf.streets.push(StreetDef { pts: vec![[a.x, a.y], [b.x, b.y]], width: 3.0, mat: drive.clone(), walk: 0.0, strip: 0.0 });
        }
        // Footpaths to the doors of the library houses.
        let path_mat = tk.streets.get("path").or_else(|| tk.streets.get("lane")).cloned().unwrap_or_else(|| town_default::STREET.to_string());
        for pts in &laid.paths {
            cf.streets.push(StreetDef { pts: pts.iter().map(|p| [p.x, p.y]).collect(), width: 1.4, mat: path_mat.clone(), walk: 0.0, strip: 0.0 });
        }
        for (p, kind) in &laid.trees {
            let k = hash32((p.x as u32).wrapping_mul(73) ^ (p.y as u32).wrapping_mul(9301) ^ seed);
            let name = match kind {
                town::Tree::Park => pick(&trees, k),
                town::Tree::Hedge => trees.last().cloned().unwrap_or_default(),
                town::Tree::Street => trees[0].clone(),
            };
            cf.trees.push((p.x, p.y, name));
        }
        let mut missing = std::collections::BTreeSet::new();
        let (built, msg) = city::build(&mut cx.f, &cf, kit, cause, seed, |name| {
            lib.material(name).unwrap_or_else(|| {
                missing.insert(name.to_string());
                crate::world::material::STONE
            })
        });
        for m in missing {
            cx.warnings.push(format!("town {}: unknown material {m}", self.name));
        }
        cx.state.get::<city::City>().absorb(built);
        let (mut cf_decks, mut cf_piers): (Vec<DeckDef>, Vec<[f32; 4]>) = (Vec::new(), Vec::new());

        // Jetties: decks on piles out over the water.
        for &(a, b) in &plan.piers {
            let top = cx.f.sea + 1.3;
            cf_decks.push(DeckDef { pts: vec![[a.x, top, a.y], [b.x, top, b.y]], width: 5.0 });
            let n = (a.distance(b) / 6.0) as usize;
            for k in 1..=n {
                let p = a.lerp(b, k as f32 / n as f32);
                for side in [-1.4f32, 1.4] {
                    let q = p + (b - a).perp().normalize_or_zero() * side;
                    cf_piers.push([q.x, q.y, 0.35, top - 0.4]);
                }
            }
        }
        if !cf_decks.is_empty() {
            let jetties = CityFile { decks: cf_decks, piers: cf_piers, ..Default::default() };
            let (built, _) = city::build(&mut cx.f, &jetties, kit, cause, seed ^ 0x7e77, |name| lib.material(name).unwrap_or(crate::world::material::STONE));
            cx.state.get::<city::City>().absorb(built);
        }

        // The town's roads out join the world's road list, for any fields
        // laid later.
        for w in plan.ways.iter().filter(|w| w.rank == rank::COUNTRY) {
            cx.state.get::<route::RoadLines>().0.push((w.pts.clone(), w.half));
        }

        // Landmarks and library houses. A site is the centre of the turned
        // footprint; `place` wants the structure's anchor.
        let mut members = Vec::new();
        for (k, s) in plan.sites.iter().enumerate() {
            let none = HashMap::default();
            let sd = &lib.structures[s.id];
            let size = *sizes.entry(s.id).or_insert_with(|| crate::world::chronicle::compile_structure(lib, s.id, &[]).size);
            let (w, d) = crate::world::shape::rotated_size(size, s.turns);
            let (ax, az) = crate::world::shape::rotate(sd.anchor[0], sd.anchor[2], size, s.turns);
            let at = s.at + Vec2::new(ax as f32 - w as f32 * 0.5, az as f32 - d as f32 * 0.5) * crate::world::gen::VOXEL;
            match cx.place(s.id, at, s.turns as f32 * 90.0, &none, 0.0, hash32(seed ^ (900 + k as u32))) {
                Some(i) => members.push(i),
                None => cx.warnings.push(format!("town {}: a landmark would not stand at {:.0},{:.0}", self.name, s.at.x, s.at.y)),
            }
        }
        // Its structures, by the town's name, for an `abandon` event.
        cx.state.get::<super::settlement::Settlements>().0.entry(self.name.clone()).or_default().extend(members);
        // Named landmarks: the first site of each structure, in the order
        // they were asked for.
        let mut named = std::collections::HashSet::new();
        for l in &self.landmarks {
            let (Some(poi), Some(def)) = (&l.poi, lib.structure(&l.structure)) else { continue };
            if let Some(s) = plan.sites.iter().enumerate().find(|(k, s)| s.id == def && !named.contains(k)) {
                named.insert(s.0);
                cx.pois.push(Poi { name: poi.clone(), pos: s.1.at, cause });
            }
        }

        // A town on stilts: every street a boardwalk, and a gangway from
        // each building's front to the nearest walk.
        let mut walked = 0.0;
        if let Some(name) = &self.walkways {
            match route::kit_index(cx, name) {
                None => cx.warnings.push(format!("town {}: unknown kit {name}", self.name)),
                Some(k) => {
                    for way in plan.ways.iter().filter(|w| w.drawn) {
                        let pts = town::grow::resample(&way.pts, 2.0);
                        if pts.len() >= 2 {
                            walked += route::lay(cx, &pts, k, 2.0).length;
                        }
                    }
                    for s in &plan.sites {
                        let face = town::quarter_axis(s.turns);
                        let reach = (s.size.x * face.x).abs() * 0.5 + (s.size.y * face.y).abs() * 0.5;
                        let door = s.at + face * reach;
                        let mut best = (f32::MAX, door);
                        for w in plan.ways.iter().filter(|w| w.drawn) {
                            for seg in w.pts.windows(2) {
                                let (d, q, _) = town::graph::seg_foot(seg[0], seg[1], door);
                                if d < best.0 {
                                    best = (d, q);
                                }
                            }
                        }
                        if best.0 > 0.8 && best.0 < 14.0 {
                            let pts = town::grow::resample(&[door, best.1], 0.75);
                            if pts.len() >= 3 {
                                walked += route::lay(cx, &pts, k, 0.0).length;
                            }
                        }
                    }
                }
            }
        }

        // Walls and fences.
        let mut fences = 0.0;
        for b in &laid.boundaries {
            let kit = match b.edge {
                Edge::FieldWall => tk.field_wall_kit(),
                Edge::Rail => tk.rail_kit(),
                Edge::Garden => tk.garden_kit(),
                Edge::TownWall => tk.town_wall_kit(),
                Edge::Hedge => tk.hedge_kit(),
            };
            let Some(k) = route::kit_index(cx, kit) else {
                cx.warnings.push(format!("town {}: unknown kit {kit}", self.name));
                continue;
            };
            // Every line at the spacing the field walls use: a ribbon laid
            // from two or three far-apart points comes out empty.
            let pts = town::grow::resample(&b.pts, 2.0);
            if pts.len() < 3 {
                continue;
            }
            let l = route::lay(cx, &pts, k, 4.0);
            fences += l.length;
        }
        // Sheds in the corners of the fields, once their walls stand.
        let (sheds, why) = super::fields::place_shacks(cx, &plan.corners, &self.crops, &self.shacks, &tk.shack_list(), seed);

        let mut extra = if self.walkways.is_some() { format!("; {walked:.0} m of boardwalk") } else { String::new() };
        if sheds > 0 {
            extra += &format!("; {sheds} field sheds ({why})");
        }
        let by_rank = |r: u8| plan.ways.iter().filter(|w| w.rank == r).map(|w| w.pts.windows(2).map(|s| s[0].distance(s[1])).sum::<f32>()).sum::<f32>();
        cx.pois.push(Poi { name: self.name.clone(), pos: c, cause });
        format!(
            "{}: {:.0} m main road, {:.0} m streets, {:.0} m lanes and closes, {} bridges; {} buildings; {:.0} m of field walls and fences; {} districts {:?}; {}; {}; {msg}{extra}",
            self.name,
            by_rank(rank::MAIN) + by_rank(rank::COUNTRY),
            by_rank(rank::STREET) + by_rank(rank::RING),
            by_rank(rank::LANE) + by_rank(rank::CLOSE),
            bridges,
            plan.buildings.len(),
            fences,
            dist.styles.len(),
            dist.styles,
            plan.notes_quiet.join(", "),
            plan.stats,
        )
    }
}

fn street_mat(tk: &city::TownKit, way: &town::Way) -> String {
    let r = match way.rank {
        rank::MAIN => "main",
        rank::RING => "ring",
        rank::STREET => "street",
        rank::LANE => "lane",
        rank::CLOSE => "close",
        rank::ALLEY => "alley",
        rank::QUAY => "quay",
        _ => "country",
    };
    tk.streets
        .get(&format!("{}_{r}", key(way.style)))
        .or_else(|| tk.streets.get(r))
        .cloned()
        .unwrap_or_else(|| town_default::STREET.to_string())
}

/// Where the line from `out` (outside every square) to `inn` (inside one)
/// crosses the square's edge, carried a metre on in.
fn edge_point(sqs: &[town::Square], out: Vec2, inn: Vec2) -> Vec2 {
    let (mut a, mut b) = (0.0f32, 1.0f32);
    for _ in 0..20 {
        let m = (a + b) * 0.5;
        if sqs.iter().any(|sq| sq.contains(out.lerp(inn, m), 0.0)) {
            b = m;
        } else {
            a = m;
        }
    }
    let e = out.lerp(inn, b);
    e + (inn - out).normalize_or_zero() * 1.0
}

/// Smooth the ground along a street and ease the land each side to it.
fn grade(f: &mut Fields, way: &town::Way, cause: u16) {
    let n = way.pts.len();
    if n < 2 {
        return;
    }
    let raw: Vec<f32> = way.pts.iter().map(|p| f.height(p.x, p.y)).collect();
    // A moving average over about 20 m.
    let mut prof = raw.clone();
    for k in 0..n {
        let (mut s, mut w) = (0.0, 0.0);
        let mut acc = 0.0;
        let mut j = k;
        while j > 0 && acc < 10.0 {
            acc += way.pts[j].distance(way.pts[j - 1]);
            j -= 1;
        }
        let from = j;
        acc = 0.0;
        j = k;
        while j + 1 < n && acc < 10.0 {
            acc += way.pts[j].distance(way.pts[j + 1]);
            j += 1;
        }
        for (q, &h) in raw.iter().enumerate().take(j + 1).skip(from) {
            if !way.wet[q] {
                s += h;
                w += 1.0;
            }
        }
        if w > 0.0 {
            prof[k] = s / w;
        }
    }
    let core = way.half + way.walk + 0.5;
    let feather = 4.0;
    for (k, s) in way.pts.windows(2).enumerate() {
        let (a, b) = (s[0], s[1]);
        let lo = a.min(b) - Vec2::splat(core + feather);
        let hi = a.max(b) + Vec2::splat(core + feather);
        for z in (lo.y.max(0.0) as usize)..(hi.y.min(f.size - 1.0) as usize) {
            for x in (lo.x.max(0.0) as usize)..(hi.x.min(f.size - 1.0) as usize) {
                let i = z * f.n + x;
                if f.water[i] == 1 || f.h[i] < f.sea {
                    continue;
                }
                let p = f.centre(i);
                let (d, _, t) = town::graph::seg_foot(a, b, p);
                let y = prof[k] + (prof[k + 1] - prof[k]) * t;
                if d <= core {
                    f.h[i] = y;
                    f.occupied[i] = true;
                    f.cliff[i] = 0.0;
                    f.cause[i] = cause;
                } else if d <= core + feather {
                    let w = 1.0 - (d - core) / feather;
                    f.h[i] += (y - f.h[i]) * w * 0.8;
                }
            }
        }
    }
}

/// A deck across the water from bank to bank, on piers.
fn bridge(cf: &mut CityFile, f: &Fields, pts: &[Vec2], half: f32) {
    // Level with the higher bank, measured only at ends that are on land:
    // half a bridge that ends mid river (where it meets the other half)
    // takes its height from its own bank.
    let wet = |p: Vec2| wet_cell(f, f.idx(p.x, p.y));
    let mut top = f.sea + 1.5;
    for p in [pts[0], pts[pts.len() - 1]] {
        if !wet(p) {
            top = top.max(f.height(p.x, p.y));
        }
    }
    let top = top + 0.6;
    let len: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    // An end on land comes down to the street there, so the deck carries on
    // from the street without a step: it climbs to `top` over a ramp of
    // 1 in 5, or less where the bridge is short.
    let ends: Vec<Option<f32>> = [pts[0], pts[pts.len() - 1]].iter().map(|p| (!wet(*p)).then(|| f.height(p.x, p.y))).collect();
    let n = len.ceil().max(1.0) as usize;
    let deck: Vec<[f32; 3]> = (0..=n)
        .map(|j| {
            let s = len * j as f32 / n as f32;
            let (p, _) = town::lots::point_at(pts, s);
            let mut y = top;
            for (g, d) in [(ends[0], s), (ends[1], len - s)] {
                if let Some(g) = g {
                    let ramp = ((top - g) * 5.0).clamp(2.0, len * 0.45);
                    y = y.min(g + (top - g) * crate::world::noise::smoothstep(0.0, ramp, d));
                }
            }
            [p.x, y, p.y]
        })
        .collect();
    cf.decks.push(DeckDef { pts: deck, width: half * 2.0 + 1.0 });
    let spans = (len / 14.0).ceil().max(1.0) as usize;
    for s in 1..spans {
        let (p, _) = town::lots::point_at(pts, len * s as f32 / spans as f32);
        cf.piers.push([p.x, p.y, 1.1, top - 0.6]);
    }
}

