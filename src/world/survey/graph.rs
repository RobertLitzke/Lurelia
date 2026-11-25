//! The survey's graph (notes/WORLD_SURVEY_PLAN.md, S3-S5): zones, anchors and
//! the links between them, found on the static grids after they are
//! measured, and kept in survey.bin with them.
//!
//! A zone is a stretch of ground a game can talk about: a named area where
//! the world names one (world/places.rs), else a patch of one biome cut to
//! about 300 m. An anchor is a point a detector found, with a kind that says
//! why (a bank to drink from, a knoll with a view, a house) and the same
//! measured record as every other: games filter on the numbers, so the kind
//! is only a hint. A link is a way between two anchors for one kind of body,
//! found on that body's passable grid, with how long, how narrow and how
//! covered it is.
//!
//! The anchors are found in two rounds. The first (drink, feed, vantage,
//! refuge, focal, the world's edges) is linked; the second reads those links
//! (pinches are the narrowest points on them, entries where they cross from
//! one zone to the next, nooks the covered dead ends away from them) and is
//! linked onto the first.

use super::build::{box_mean, distance, inside, Ground, EYE};
use super::{water, Chan, CELL_M};
use crate::world::fields::{World, CELL};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Why a detector picked a point.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A zone's open middle: a plaza, a clearing.
    Focal,
    /// A rise with a wide view.
    Vantage,
    /// The narrowest point on a way between places: a bridge, a pass, a gate.
    Pinch,
    /// Where a way crosses into a zone, or comes in from the world's edge.
    Entry,
    /// Somewhere enclosed: a building's rooms, a thicket.
    Refuge,
    /// A covered dead end away from the ways.
    Nook,
    /// A bank beside fresh water.
    Drink,
    /// The best grazing or browsing in a patch.
    Feed,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Focal, Kind::Vantage, Kind::Pinch, Kind::Entry, Kind::Refuge, Kind::Nook, Kind::Drink, Kind::Feed];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Focal => "focal",
            Kind::Vantage => "vantage",
            Kind::Pinch => "pinch",
            Kind::Entry => "entry",
            Kind::Refuge => "refuge",
            Kind::Nook => "nook",
            Kind::Drink => "drink",
            Kind::Feed => "feed",
        }
    }
}

/// A kind of body a link is for, by its passable channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Person,
    Large,
    Vehicle,
}

impl Class {
    pub const ALL: [Class; 3] = [Class::Person, Class::Large, Class::Vehicle];

    pub fn name(self) -> &'static str {
        match self {
            Class::Person => "person",
            Class::Large => "large",
            Class::Vehicle => "vehicle",
        }
    }

    pub fn chan(self) -> Chan {
        match self {
            Class::Person => Chan::PassPerson,
            Class::Large => Chan::PassLarge,
            Class::Vehicle => Chan::PassVehicle,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    /// The world names it (an area); otherwise a biome patch.
    pub named: bool,
    pub centre: [f32; 2],
    /// Survey cells in it.
    pub cells: u32,
    /// The ground most of it stands above: its 10th percentile height.
    pub floor: f32,
    /// The biomes, the area's kind and the structures' library tags.
    pub tags: Vec<String>,
    /// Shares and means over its cells, 0 to 1.
    pub enclosure: f32,
    pub verticality: f32,
    pub open: f32,
    pub cover: f32,
    pub forage: f32,
    pub water: f32,
    pub passable: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub kind: Kind,
    pub pos: [f32; 3],
    /// The way with the longest view, on the ground plane.
    pub facing: [f32; 2],
    /// Index into `Graph::zones`, or u16::MAX for none.
    pub zone: u16,
    /// Height over its zone's floor, metres.
    pub above_floor: f32,
    /// Openness (survey `open`), 0 to 1.
    pub open: f32,
    /// How far a person standing there can be seen along `facing`, metres.
    pub sightline: f32,
    /// Ways in: a building's doors and gaps, or the separate passable
    /// directions round the spot (8 means open all round).
    pub ways_in: u8,
    /// The flat passable radius round it, metres (up to 30).
    pub flat: f32,
    pub cover: f32,
    pub roofed: bool,
    /// Metres to a drinkable bank.
    pub drink: f32,
    /// Food within about 25 m, 0 to 1, by what eats it: dry grass for
    /// grazers, plants that afford `browse` for browsers. Which animal wants
    /// which is the animal's business (its diet), not the survey's.
    pub graze: f32,
    pub browse: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    pub a: u32,
    pub b: u32,
    pub class: Class,
    /// Along the way, metres.
    pub length: f32,
    /// The narrowest the way gets, metres.
    pub narrowest: f32,
    /// Mean cover along it.
    pub cover: f32,
    /// It goes through a gap of `PINCH_M` or less.
    pub pinch: bool,
    /// The way, simplified, metres (x, z).
    pub path: Vec<[f32; 2]>,
    /// Where the narrowest point is, while the graph is built.
    #[serde(skip)]
    pub narrow_at: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    pub zones: Vec<Zone>,
    /// Zone index + 1 per survey cell, 0 for none (sea, deep water).
    pub zone_of: Vec<u16>,
    pub anchors: Vec<Anchor>,
    pub links: Vec<Link>,
}

/// A gap this narrow or less makes a pinch, metres.
const PINCH_M: f32 = 12.0;
/// How far a link may run, metres of path.
const LINK_REACH_M: f32 = 400.0;
/// Links from each anchor, per class: to the nearest this many.
const LINKS_EACH: usize = 4;
/// Unnamed ground is cut into patches about this wide, metres.
const PATCH_M: f32 = 300.0;
/// Zones smaller than this many survey cells (4,000 m²) are merged into a
/// neighbour.
const MIN_ZONE: usize = 250;
/// Named areas smaller than this, square metres, are left to the patches.
const MIN_AREA_M2: f32 = 2500.0;

struct Ctx<'a> {
    w: &'a World,
    g: &'a [Vec<u8>],
    gr: &'a Ground,
    n: usize,
}

impl Ctx<'_> {
    fn ch(&self, c: Chan, k: usize) -> u8 {
        self.g[c as usize][k]
    }
    fn centre(&self, k: usize) -> (f32, f32) {
        (((k % self.n) as f32 + 0.5) * CELL_M, ((k / self.n) as f32 + 0.5) * CELL_M)
    }
    fn cell(&self, x: f32, z: f32) -> usize {
        super::cell_index(self.n, x, z)
    }
    fn i1(&self, x: f32, z: f32) -> usize {
        self.w.f.idx(x, z)
    }
    fn land(&self, k: usize) -> bool {
        !matches!(self.ch(Chan::Water, k), water::FRESH_DEEP | water::SALT_SHALLOW | water::SALT_DEEP)
    }
}

/// Zones, anchors and links for a surveyed world.
pub(super) fn build(w: &World, n: usize, g: &[Vec<u8>], gr: &Ground) -> Graph {
    let cx = Ctx { w, g, gr, n };
    let (zones, zone_of) = zones(&cx);
    let mut graph = Graph { zones, zone_of, anchors: Vec::new(), links: Vec::new() };
    let mut found = first_round(&cx, &graph);
    let first = found.len();
    for (kind, x, z, ways) in found.drain(..) {
        graph.anchors.push(measure(&cx, &graph, kind, x, z, ways));
    }
    let clear: Vec<Vec<f32>> = Class::ALL.iter().map(|&c| clearance(&cx, c)).collect();
    graph.links = link(&cx, &graph, &clear, 0..first);
    let second = second_round(&cx, &graph);
    for (kind, x, z, ways) in second {
        graph.anchors.push(measure(&cx, &graph, kind, x, z, ways));
    }
    let more = link(&cx, &graph, &clear, first..graph.anchors.len());
    let mut seen: std::collections::HashSet<(u32, u32, Class)> = graph.links.iter().map(|l| (l.a.min(l.b), l.a.max(l.b), l.class)).collect();
    for l in more {
        if seen.insert((l.a.min(l.b), l.a.max(l.b), l.class)) {
            graph.links.push(l);
        }
    }
    graph
}

// ---------------------------------------------------------------- zones

fn zones(cx: &Ctx) -> (Vec<Zone>, Vec<u16>) {
    let (w, n) = (cx.w, cx.n);
    let f = &w.f;
    // Label per cell: an area (its index), or a patch key.
    let mut area_of = vec![u32::MAX; n * n];
    let mut areas: Vec<usize> = (0..w.areas.len()).filter(|&a| w.areas[a].area_m2 >= MIN_AREA_M2).collect();
    // Biggest first, so a smaller area inside it takes its cells.
    areas.sort_by(|&a, &b| w.areas[b].area_m2.total_cmp(&w.areas[a].area_m2));
    for &a in &areas {
        let ar = &w.areas[a];
        let c = |v: f32| ((v / CELL_M).floor().max(0.0) as usize).min(n);
        for z in c(ar.lo.y)..(c(ar.hi.y) + 1).min(n) {
            for x in c(ar.lo.x)..(c(ar.hi.x) + 1).min(n) {
                let k = z * n + x;
                let (px, pz) = cx.centre(k);
                if cx.land(k) && inside(&ar.poly, glam::Vec2::new(px, pz)) {
                    area_of[k] = a as u32;
                }
            }
        }
    }
    // Patches: same biome, nearest of a jittered grid of seeds `PATCH_M`
    // apart (so patches are irregular cells, not squares).
    let per = (PATCH_M / CELL_M).round().max(1.0) as usize;
    let seeds = n.div_ceil(per);
    let seed_at = |sx: usize, sz: usize| -> (f32, f32) {
        let h = crate::world::noise::hash32((sx as u32).wrapping_mul(73_856_093) ^ (sz as u32).wrapping_mul(19_349_663) ^ 0x5eed);
        let j = |v: u32| (v & 0xffff) as f32 / 65535.0 * 0.8 + 0.1;
        (((sx as f32) + j(h)) * per as f32, ((sz as f32) + j(h >> 16)) * per as f32)
    };
    let nearest_seed = |k: usize| -> (usize, usize) {
        let (x, z) = ((k % n) as f32 + 0.5, (k / n) as f32 + 0.5);
        let (cx0, cz0) = ((k % n) / per, (k / n) / per);
        let mut best = (f32::MAX, (cx0, cz0));
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let (sx, sz) = (cx0 as i32 + dx, cz0 as i32 + dz);
                if sx < 0 || sz < 0 || sx as usize >= seeds || sz as usize >= seeds {
                    continue;
                }
                let (px, pz) = seed_at(sx as usize, sz as usize);
                let d = (px - x).powi(2) + (pz - z).powi(2);
                if d < best.0 {
                    best = (d, (sx as usize, sz as usize));
                }
            }
        }
        best.1
    };
    let key = |k: usize| -> u64 {
        if !cx.land(k) {
            return u64::MAX;
        }
        if area_of[k] != u32::MAX {
            return 1 << 40 | area_of[k] as u64;
        }
        let (px, pz) = cx.centre(k);
        let b = f.biome[f.idx(px, pz)] as u64;
        let (sx, sz) = nearest_seed(k);
        b << 32 | (sz as u64) << 16 | sx as u64
    };
    // Connected pieces of one key.
    let mut comp = vec![u32::MAX; n * n];
    let mut pieces: Vec<(u64, Vec<usize>)> = Vec::new();
    for s in 0..n * n {
        if comp[s] != u32::MAX || key(s) == u64::MAX {
            continue;
        }
        let ks = key(s);
        let id = pieces.len() as u32;
        let mut stack = vec![s];
        comp[s] = id;
        let mut cells = Vec::new();
        while let Some(k) = stack.pop() {
            cells.push(k);
            let (x, z) = (k % n, k / n);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                if nx < 0 || nz < 0 || nx as usize >= n || nz as usize >= n {
                    continue;
                }
                let j = nz as usize * n + nx as usize;
                if comp[j] == u32::MAX && key(j) == ks {
                    comp[j] = id;
                    stack.push(j);
                }
            }
        }
        pieces.push((ks, cells));
    }
    // Small pieces go to the neighbour they share most border with; an area
    // is never merged away (it is a name the world gave).
    let mut owner: Vec<u32> = (0..pieces.len() as u32).collect();
    fn root(o: &mut [u32], mut i: u32) -> u32 {
        while o[i as usize] != i {
            o[i as usize] = o[o[i as usize] as usize];
            i = o[i as usize];
        }
        i
    }
    let mut size: Vec<usize> = pieces.iter().map(|p| p.1.len()).collect();
    // An area cut in two by water is still one place.
    let mut first_of_area: HashMap<u64, u32> = HashMap::new();
    for (p, (ks, _)) in pieces.iter().enumerate() {
        if ks >> 40 == 1 {
            let f0 = *first_of_area.entry(*ks).or_insert(p as u32);
            let r = root(&mut owner, p as u32);
            if r != f0 {
                owner[r as usize] = f0;
                size[f0 as usize] += size[r as usize];
            }
        }
    }
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by_key(|&p| size[p]);
    for p in order {
        let r = root(&mut owner, p as u32);
        if size[r as usize] >= MIN_ZONE || pieces[p].0 >> 40 == 1 {
            continue;
        }
        let mut border: HashMap<u32, usize> = HashMap::new();
        for &k in &pieces[p].1 {
            let (x, z) = (k % n, k / n);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                if nx < 0 || nz < 0 || nx as usize >= n || nz as usize >= n {
                    continue;
                }
                let j = nz as usize * n + nx as usize;
                if comp[j] != u32::MAX {
                    let o = root(&mut owner, comp[j]);
                    if o != r {
                        *border.entry(o).or_default() += 1;
                    }
                }
            }
        }
        if let Some((&o, _)) = border.iter().max_by_key(|(&o, &c)| (c, Reverse(o))) {
            owner[r as usize] = o;
            size[o as usize] += size[r as usize];
        }
    }
    // Number the zones that are left.
    let mut index: HashMap<u32, u16> = HashMap::new();
    let mut zone_of = vec![0u16; n * n];
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut first_key: Vec<u64> = Vec::new();
    for (p, (ks, cells)) in pieces.iter().enumerate() {
        let r = root(&mut owner, p as u32);
        let z = *index.entry(r).or_insert_with(|| {
            members.push(Vec::new());
            first_key.push(pieces[r as usize].0);
            (members.len() - 1) as u16
        });
        let _ = ks;
        members[z as usize].extend(cells);
        for &k in cells {
            zone_of[k] = z + 1;
        }
    }
    // What each zone is like.
    let mut tags_of: Vec<HashMap<String, usize>> = vec![HashMap::new(); members.len()];
    for p in &w.placed {
        let (x, z) = ((p.origin[0] as f32 + 0.5) * crate::world::gen::VOXEL, (p.origin[2] as f32 + 0.5) * crate::world::gen::VOXEL);
        let zi = zone_of[cx.cell(x, z)];
        if zi > 0 {
            for t in &w.lib.structures.get(p.def).map(|d| d.tags.clone()).unwrap_or_default() {
                *tags_of[zi as usize - 1].entry(t.clone()).or_default() += 1;
            }
        }
    }
    let mut out = Vec::with_capacity(members.len());
    let mut patch_count: HashMap<String, usize> = HashMap::new();
    for (zi, cells) in members.iter().enumerate() {
        let cnt = cells.len().max(1) as f32;
        let mean = |c: Chan| cells.iter().map(|&k| cx.ch(c, k) as f32 / 255.0).sum::<f32>() / cnt;
        let mut hs: Vec<f32> = cells.iter().map(|&k| cx.gr.ground[k]).collect();
        hs.sort_by(f32::total_cmp);
        let floor = hs[hs.len() / 10];
        let (sx, sz) = cells.iter().fold((0.0, 0.0), |a, &k| {
            let c = cx.centre(k);
            (a.0 + c.0, a.1 + c.1)
        });
        let mut biomes: HashMap<u8, usize> = HashMap::new();
        for &k in cells {
            let (px, pz) = cx.centre(k);
            *biomes.entry(f.biome[f.idx(px, pz)]).or_default() += 1;
        }
        let mut biomes: Vec<(u8, usize)> = biomes.into_iter().collect();
        biomes.sort_by_key(|&(b, c)| (Reverse(c), b));
        let biome_name = |b: u8| w.lib.biomes.get(b as usize).map_or("ground".to_string(), |d| d.name.clone());
        let key = first_key[zi];
        let (name, named, mut tags) = if key >> 40 == 1 {
            let a = &w.areas[(key & 0xffff_ffff) as usize];
            (a.name.clone(), true, vec![a.kind.clone()])
        } else {
            let b = biome_name(biomes.first().map_or(0, |x| x.0));
            let k = patch_count.entry(b.clone()).or_default();
            *k += 1;
            (format!("{b} {k}"), false, Vec::new())
        };
        tags.extend(biomes.iter().take(2).map(|&(b, _)| biome_name(b)));
        let mut st: Vec<(String, usize)> = tags_of[zi].drain().collect();
        st.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        tags.extend(st.into_iter().take(4).map(|(t, _)| t));
        tags.retain(|t| !t.is_empty());
        tags.dedup();
        let tops: f32 = cells.iter().map(|&k| cx.ch(Chan::Built, k) as f32 / 255.0).sum::<f32>() / cnt;
        let rise: f32 = cells.iter().map(|&k| (cx.ch(Chan::Rise50, k) as f32 - 128.0).abs()).sum::<f32>() / cnt;
        out.push(Zone {
            name,
            named,
            centre: [sx / cnt, sz / cnt],
            cells: cells.len() as u32,
            floor,
            tags,
            enclosure: tops,
            verticality: ((hs[hs.len() * 9 / 10] - floor) / 30.0 + rise / 20.0).clamp(0.0, 1.0),
            open: mean(Chan::Open),
            cover: mean(Chan::Cover),
            forage: cells.iter().map(|&k| cx.ch(Chan::Graze, k).max(cx.ch(Chan::Browse, k)) as f32 / 255.0).sum::<f32>() / cnt,
            water: cells.iter().filter(|&&k| cx.ch(Chan::Water, k) != water::DRY).count() as f32 / cnt,
            passable: cells.iter().filter(|&&k| cx.ch(Chan::PassPerson, k) > 0).count() as f32 / cnt,
        });
    }
    (out, zone_of)
}

// ---------------------------------------------------------------- anchors

type Found = (Kind, f32, f32, Option<u8>);

/// The best candidates, `spacing` metres apart at least, at most `per_zone`
/// in any zone. `cands` is (score, x, z).
fn spread(graph: &Graph, n: usize, mut cands: Vec<(f32, f32, f32)>, spacing: f32, per_zone: usize, near: &[Found]) -> Vec<(f32, f32)> {
    cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)));
    let mut out: Vec<(f32, f32)> = Vec::new();
    let mut in_zone: HashMap<u16, usize> = HashMap::new();
    let s2 = spacing * spacing;
    for (_, x, z) in cands {
        let zi = graph.zone_of[super::cell_index(n, x, z)];
        // A big zone (a wide named plain) gets as many as the patches it would
        // have been cut into.
        let patch = (PATCH_M / CELL_M).powi(2);
        let cap = per_zone * if zi > 0 { (graph.zones[zi as usize - 1].cells as f32 / patch).ceil().max(1.0) as usize } else { 1 };
        if in_zone.get(&zi).copied().unwrap_or(0) >= cap {
            continue;
        }
        let far = |&(ax, az): &(f32, f32)| (ax - x).powi(2) + (az - z).powi(2) >= s2;
        if out.iter().all(far) && near.iter().all(|f| far(&(f.1, f.2))) {
            out.push((x, z));
            *in_zone.entry(zi).or_default() += 1;
        }
    }
    out
}

fn first_round(cx: &Ctx, graph: &Graph) -> Vec<Found> {
    let n = cx.n;
    let all = 0..n * n;
    let mut out: Vec<Found> = Vec::new();
    let on = |c: Chan, k: usize| cx.ch(c, k) > 0;

    // Drink: banks a large walker can stand on first.
    let cands = all.clone().filter(|&k| cx.gr.bank[k]).map(|k| {
        let (x, z) = cx.centre(k);
        (1.0 + on(Chan::PassLarge, k) as u8 as f32, x, z)
    });
    out.extend(spread(graph, n, cands.collect(), 60.0, 4, &[]).into_iter().map(|(x, z)| (Kind::Drink, x, z, None)));

    // Feed: the best grazing or browsing, smoothed over about 25 m, every
    // 100 m wherever there is enough of it: a plain that is all grass gets
    // feed all across it, with no cap per zone.
    let forage: Vec<f32> = all.clone().map(|k| cx.ch(Chan::Graze, k).max(cx.ch(Chan::Browse, k)) as f32 / 255.0).collect();
    let smooth = box_mean(&forage, n, 3);
    let cands = all.clone().filter(|&k| smooth[k] >= 0.45 && on(Chan::PassLarge, k)).map(|k| {
        let (x, z) = cx.centre(k);
        (smooth[k], x, z)
    });
    out.extend(spread(graph, n, cands.collect(), 100.0, usize::MAX / 64, &[]).into_iter().map(|(x, z)| (Kind::Feed, x, z, None)));

    // Vantage: risen and open.
    let cands = all.clone().filter(|&k| on(Chan::PassPerson, k) && cx.ch(Chan::Rise50, k) >= 132 && cx.ch(Chan::Open, k) >= 140).map(|k| {
        let (x, z) = cx.centre(k);
        let rise = (cx.ch(Chan::Rise50, k) as f32 - 128.0).min(30.0) + 0.3 * (cx.ch(Chan::Rise200, k) as f32 - 128.0).clamp(0.0, 60.0);
        (rise * cx.ch(Chan::Open, k) as f32 / 255.0, x, z)
    });
    out.extend(spread(graph, n, cands.collect(), 120.0, 3, &[]).into_iter().map(|(x, z)| (Kind::Vantage, x, z, None)));

    // Refuges: each building's rooms (roofed floor a person can stand on),
    // with the gaps out of them counted as its ways in.
    out.extend(buildings(cx));
    // And thickets: thick cover, little open, a large walker can bed there.
    let cands = all.clone().filter(|&k| on(Chan::PassLarge, k) && cx.ch(Chan::Roof, k) == 0).filter_map(|k| {
        let s = cx.ch(Chan::Cover, k) as f32 / 255.0 * (1.0 - cx.ch(Chan::Open, k) as f32 / 255.0);
        (s >= 0.45).then(|| {
            let (x, z) = cx.centre(k);
            (s, x, z)
        })
    });
    let near = out.clone();
    out.extend(spread(graph, n, cands.collect(), 80.0, 2, &near).into_iter().map(|(x, z)| (Kind::Refuge, x, z, None)));

    // Focal: per zone, the flattest open passable ground near its middle.
    for (zi, zone) in graph.zones.iter().enumerate() {
        if zone.cells < 150 {
            continue;
        }
        let mut best: Option<(f32, usize)> = None;
        for k in 0..n * n {
            if graph.zone_of[k] as usize != zi + 1 || !on(Chan::PassPerson, k) {
                continue;
            }
            let (x, z) = cx.centre(k);
            let d = ((x - zone.centre[0]).powi(2) + (z - zone.centre[1]).powi(2)).sqrt();
            let flat = flat4(cx, k);
            let s = flat + cx.ch(Chan::Open, k) as f32 / 255.0 - d / 60.0;
            if best.is_none_or(|b| s > b.0) {
                best = Some((s, k));
            }
        }
        if let Some((_, k)) = best {
            let (x, z) = cx.centre(k);
            out.push((Kind::Focal, x, z, None));
        }
    }

    // Entries from the world's edge: passable land on the border.
    let mut edge = Vec::new();
    for i in 0..n {
        for k in [i, (n - 1) * n + i, i * n, i * n + n - 1] {
            if on(Chan::PassPerson, k) && cx.land(k) {
                let (x, z) = cx.centre(k);
                edge.push((1.0, x, z));
            }
        }
    }
    out.extend(spread(graph, n, edge, 150.0, 2, &[]).into_iter().map(|(x, z)| (Kind::Entry, x, z, None)));
    out
}

/// How flat and passable the ground is round survey cell `k`: the share of
/// the 7 x 7 cells round it a person can cross within a metre of its height.
fn flat4(cx: &Ctx, k: usize) -> f32 {
    let n = cx.n as i32;
    let (x, z) = ((k % cx.n) as i32, (k / cx.n) as i32);
    let h = cx.gr.ground[k];
    let mut good = 0;
    for dz in -3..=3 {
        for dx in -3..=3 {
            let (nx, nz) = (x + dx, z + dz);
            if nx >= 0 && nz >= 0 && nx < n && nz < n {
                let j = (nz * n + nx) as usize;
                good += (cx.ch(Chan::PassPerson, j) > 0 && (cx.gr.ground[j] - h).abs() <= 1.0) as i32;
            }
        }
    }
    good as f32 / 49.0
}

/// Every building's rooms as a refuge: connected roofed 1 m cells a person
/// can stand on, 12 m² and more, at the cell nearest their middle, with the
/// number of separate gaps out of them (doors, open sides).
fn buildings(cx: &Ctx) -> Vec<Found> {
    let f = &cx.w.f;
    let n1 = f.n;
    let gr = cx.gr;
    let room = |i: usize| gr.pass1[i] && gr.cells[i].roof;
    let mut seen = vec![false; n1 * n1];
    let mut out = Vec::new();
    let nb4 = |i: usize| {
        let (x, z) = (i % n1, i / n1);
        [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)].into_iter().filter_map(move |(dx, dz)| {
            let (nx, nz) = (x as i32 + dx, z as i32 + dz);
            (nx >= 0 && nz >= 0 && (nx as usize) < n1 && (nz as usize) < n1).then(|| nz as usize * n1 + nx as usize)
        })
    };
    for s in 0..n1 * n1 {
        if seen[s] || !room(s) {
            continue;
        }
        let mut stack = vec![s];
        seen[s] = true;
        let mut cells = Vec::new();
        let mut exits: Vec<usize> = Vec::new();
        while let Some(i) = stack.pop() {
            cells.push(i);
            for j in nb4(i) {
                if room(j) {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                } else if gr.pass1[j] {
                    exits.push(j);
                }
            }
        }
        if cells.len() < 12 {
            continue;
        }
        // Gaps: the exit cells in touching groups.
        exits.sort_unstable();
        exits.dedup();
        let mut gap_seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut gaps = 0u8;
        for &e in &exits {
            if !gap_seen.insert(e) {
                continue;
            }
            gaps = gaps.saturating_add(1);
            let mut st = vec![e];
            while let Some(i) = st.pop() {
                let (x, z) = ((i % n1) as i32, (i / n1) as i32);
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, nz) = (x + dx, z + dz);
                        if nx < 0 || nz < 0 || nx as usize >= n1 || nz as usize >= n1 {
                            continue;
                        }
                        let j = nz as usize * n1 + nx as usize;
                        if exits.binary_search(&j).is_ok() && gap_seen.insert(j) {
                            st.push(j);
                        }
                    }
                }
            }
        }
        let (sx, sz) = cells.iter().fold((0.0, 0.0), |a, &i| (a.0 + (i % n1) as f32 + 0.5, a.1 + (i / n1) as f32 + 0.5));
        let (mx, mz) = (sx / cells.len() as f32, sz / cells.len() as f32);
        let mid = cells.iter().copied().min_by(|&a, &b| {
            let d = |i: usize| ((i % n1) as f32 + 0.5 - mx).powi(2) + ((i / n1) as f32 + 0.5 - mz).powi(2);
            d(a).total_cmp(&d(b))
        });
        if let Some(i) = mid {
            out.push((Kind::Refuge, (i % n1) as f32 * CELL + 0.5, (i / n1) as f32 * CELL + 0.5, Some(gaps)));
        }
    }
    out
}

fn second_round(cx: &Ctx, graph: &Graph) -> Vec<Found> {
    let n = cx.n;
    let mut out: Vec<Found> = Vec::new();
    let existing: Vec<Found> = graph.anchors.iter().map(|a| (a.kind, a.pos[0], a.pos[2], None)).collect();
    // Pinches: the narrowest point of each narrow way.
    let mut cands = Vec::new();
    for l in graph.links.iter().filter(|l| l.pinch) {
        if let Some(&(x, z)) = l.narrow_at.as_ref() {
            cands.push((PINCH_M - l.narrowest, x, z));
        }
    }
    out.extend(spread(graph, n, cands, 40.0, 6, &existing).into_iter().map(|(x, z)| (Kind::Pinch, x, z, None)));
    // Entries: where a person's way crosses from one zone into the next.
    let mut cands = Vec::new();
    let mut walked = vec![false; n * n];
    for l in graph.links.iter().filter(|l| l.class == Class::Person) {
        let mut last = None;
        for p in densify(&l.path) {
            let k = cx.cell(p[0], p[1]);
            walked[k] = true;
            let zi = graph.zone_of[k];
            if let Some(prev) = last {
                if prev != zi && prev > 0 && zi > 0 {
                    cands.push((1.0, p[0], p[1]));
                }
            }
            last = Some(zi);
        }
    }
    out.extend(spread(graph, n, cands, 60.0, 6, &existing).into_iter().map(|(x, z)| (Kind::Entry, x, z, None)));
    // Nooks: covered and enclosed, 25 m and more from any way.
    let away = distance(&walked, n);
    let cands: Vec<(f32, f32, f32)> = (0..n * n)
        .filter(|&k| away[k] as f32 * CELL_M >= 25.0 && away[k] < 255 && cx.ch(Chan::PassPerson, k) > 0 && cx.ch(Chan::Cover, k) >= 128)
        .map(|k| {
            let (x, z) = cx.centre(k);
            (cx.ch(Chan::Cover, k) as f32 / 255.0 * (1.0 - cx.ch(Chan::Open, k) as f32 / 255.0), x, z)
        })
        .collect();
    let mut nooks = Vec::new();
    let mut tried = cands;
    tried.sort_by(|a, b| b.0.total_cmp(&a.0));
    tried.truncate(4000);
    for (s, x, z) in tried {
        if ways_round(cx, x, z) <= 2 {
            nooks.push((s, x, z));
        }
    }
    let mut all_near = existing;
    all_near.extend(out.iter().cloned());
    out.extend(spread(graph, n, nooks, 80.0, 2, &all_near).into_iter().map(|(x, z)| (Kind::Nook, x, z, None)));
    out
}

/// A path's points about a survey cell apart.
fn densify(path: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let steps = (d / (CELL_M * 0.5)).ceil().max(1.0) as usize;
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    if let Some(&l) = path.last() {
        out.push(l);
    }
    out
}

/// Separate passable directions round a point: runs of passable 1 m cells on
/// a ring 6 m out, 48 samples. 8 when nothing round it is blocked.
fn ways_round(cx: &Ctx, x: f32, z: f32) -> u8 {
    const N: usize = 48;
    let ok: Vec<bool> = (0..N)
        .map(|i| {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            let (px, pz) = (x + a.cos() * 6.0, z + a.sin() * 6.0);
            px >= 0.0 && pz >= 0.0 && px < cx.w.f.size && pz < cx.w.f.size && cx.gr.pass1[cx.i1(px, pz)]
        })
        .collect();
    if ok.iter().all(|&o| o) {
        return 8;
    }
    let runs = (0..N).filter(|&i| ok[i] && !ok[(i + N - 1) % N]).count();
    runs.min(8) as u8
}

/// An anchor's record (`Anchor`) at (x, z).
fn measure(cx: &Ctx, graph: &Graph, kind: Kind, x: f32, z: f32, ways: Option<u8>) -> Anchor {
    let f = &cx.w.f;
    let k = cx.cell(x, z);
    let i = cx.i1(x, z);
    let c = &cx.gr.cells[i];
    let floor_here = f.h[i] + if c.built <= 0.6 { c.built.max(0.0) } else { 0.0 };
    let zi = graph.zone_of[k];
    let zone_floor = if zi > 0 { graph.zones[zi as usize - 1].floor } else { f.sea };
    let (facing, sightline) = sightline(cx, x, z, floor_here.max(f.sea) + EYE);
    Anchor {
        kind,
        pos: [x, floor_here, z],
        facing,
        zone: if zi > 0 { zi - 1 } else { u16::MAX },
        above_floor: floor_here - zone_floor,
        open: cx.ch(Chan::Open, k) as f32 / 255.0,
        sightline,
        ways_in: ways.unwrap_or_else(|| ways_round(cx, x, z)),
        flat: flat_radius(cx, x, z),
        cover: cx.ch(Chan::Cover, k) as f32 / 255.0,
        roofed: c.roof,
        drink: cx.ch(Chan::Drink, k) as f32 * CELL_M,
        graze: near_mean(cx, Chan::Graze, k),
        browse: near_mean(cx, Chan::Browse, k),
    }
}

/// A channel's mean over the 7 x 7 survey cells round `k` (about 25 m).
fn near_mean(cx: &Ctx, c: Chan, k: usize) -> f32 {
    let n = cx.n as i32;
    let (x, z) = ((k % cx.n) as i32, (k / cx.n) as i32);
    let (mut sum, mut cnt) = (0.0, 0.0);
    for dz in -3..=3 {
        for dx in -3..=3 {
            let (nx, nz) = (x + dx, z + dz);
            if nx >= 0 && nz >= 0 && nx < n && nz < n {
                sum += cx.ch(c, (nz * n + nx) as usize) as f32 / 255.0;
                cnt += 1.0;
            }
        }
    }
    sum / cnt
}

/// The bearing (of 16) along which a person standing far off can be seen
/// furthest from `eye`, and how far, to 600 m. Crowns block the view.
fn sightline(cx: &Ctx, x: f32, z: f32, eye: f32) -> ([f32; 2], f32) {
    let o = &cx.gr.obstr;
    let mut best = ([1.0, 0.0], 0.0f32);
    for b in 0..16 {
        let a = b as f32 / 16.0 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        let mut steep = f32::MIN;
        let mut seen = 0.0;
        let mut d = 2.0;
        while d <= 600.0 {
            let Some(g) = o.at(x + c * d, z + s * d) else { break };
            let ground = cx.w.f.height(x + c * d, z + s * d);
            // A person standing there: their head against what came before.
            if (ground + EYE - eye) / d >= steep {
                seen = d;
            }
            steep = steep.max((o.solid[g].max(o.crown[g]) - eye) / d);
            d += if d < 100.0 { 2.0 } else { 4.0 };
        }
        if seen > best.1 {
            best = ([c, s], seen);
        }
    }
    best
}

/// The radius of passable ground within a metre of the height at (x, z),
/// to 30 m.
fn flat_radius(cx: &Ctx, x: f32, z: f32) -> f32 {
    let f = &cx.w.f;
    let h0 = f.h[cx.i1(x, z)];
    for r in 1..=30 {
        let steps = (r * 8).max(8);
        for s in 0..steps {
            let a = s as f32 / steps as f32 * std::f32::consts::TAU;
            let (px, pz) = (x + a.cos() * r as f32, z + a.sin() * r as f32);
            if px < 0.0 || pz < 0.0 || px >= f.size || pz >= f.size {
                return r as f32 - 1.0;
            }
            let i = cx.i1(px, pz);
            if !cx.gr.pass1[i] || (f.h[i] - h0).abs() > 1.0 {
                return r as f32 - 1.0;
            }
        }
    }
    30.0
}

// ---------------------------------------------------------------- links

/// How wide the passable ground is at each survey cell for a class, metres.
fn clearance(cx: &Ctx, class: Class) -> Vec<f32> {
    let blocked: Vec<bool> = (0..cx.n * cx.n).map(|k| cx.ch(class.chan(), k) == 0).collect();
    distance(&blocked, cx.n).iter().map(|&d| if d == 0 { 0.0 } else { (2.0 * d as f32 - 1.0) * CELL_M }).collect()
}

/// Links for the anchors in `from`, to the nearest `LINKS_EACH` anchors
/// each class can reach, by bounded Dijkstra on that class's grid.
fn link(cx: &Ctx, graph: &Graph, clear: &[Vec<f32>], from: std::ops::Range<usize>) -> Vec<Link> {
    let n = cx.n;
    let mut at_cell: HashMap<usize, Vec<u32>> = HashMap::new();
    for (a, an) in graph.anchors.iter().enumerate() {
        at_cell.entry(cx.cell(an.pos[0], an.pos[2])).or_default().push(a as u32);
    }
    let jobs: Vec<(usize, Class)> = from.flat_map(|a| Class::ALL.into_iter().map(move |c| (a, c))).collect();
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).clamp(1, 16);
    let per = jobs.len().div_ceil(threads).max(1);
    let mut out: Vec<Link> = Vec::new();
    std::thread::scope(|sc| {
        let handles: Vec<_> = jobs
            .chunks(per)
            .map(|chunk| {
                let at_cell = &at_cell;
                sc.spawn(move || {
                    let mut dist = vec![f32::INFINITY; n * n];
                    let mut prev = vec![u32::MAX; n * n];
                    let mut touched = Vec::new();
                    let mut links = Vec::new();
                    for &(a, class) in chunk {
                        let an = &graph.anchors[a];
                        let pass = &cx.g[class.chan() as usize];
                        let Some(start) = snap(n, pass, cx.cell(an.pos[0], an.pos[2])) else { continue };
                        for &t in &touched {
                            dist[t] = f32::INFINITY;
                            prev[t] = u32::MAX;
                        }
                        touched.clear();
                        let mut heap = BinaryHeap::new();
                        dist[start] = 0.0;
                        touched.push(start);
                        heap.push(Reverse((Ord32(0.0), start)));
                        let mut got: Vec<(u32, usize)> = Vec::new();
                        let reach = LINK_REACH_M / CELL_M;
                        while let Some(Reverse((Ord32(d), k))) = heap.pop() {
                            if d > dist[k] {
                                continue;
                            }
                            if let Some(list) = at_cell.get(&k) {
                                for &b in list {
                                    if b as usize != a && !got.iter().any(|g| g.0 == b) {
                                        got.push((b, k));
                                    }
                                }
                                if got.len() >= LINKS_EACH {
                                    break;
                                }
                            }
                            let (x, z) = ((k % n) as i32, (k / n) as i32);
                            for (dx, dz, step) in [(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0), (1, 1, 1.4142), (1, -1, 1.4142), (-1, 1, 1.4142), (-1, -1, 1.4142)] {
                                let (nx, nz) = (x + dx, z + dz);
                                if nx < 0 || nz < 0 || nx as usize >= n || nz as usize >= n {
                                    continue;
                                }
                                let j = nz as usize * n + nx as usize;
                                if pass[j] == 0 {
                                    continue;
                                }
                                // Wading costs three times walking.
                                let wet = if cx.ch(Chan::Water, j) != water::DRY { 3.0 } else { 1.0 };
                                let nd = d + step * wet;
                                if nd < dist[j] && nd <= reach {
                                    if dist[j].is_infinite() {
                                        touched.push(j);
                                    }
                                    dist[j] = nd;
                                    prev[j] = k as u32;
                                    heap.push(Reverse((Ord32(nd), j)));
                                }
                            }
                        }
                        for (b, end) in got {
                            let mut cells = vec![end];
                            let mut k = end;
                            while prev[k] != u32::MAX {
                                k = prev[k] as usize;
                                cells.push(k);
                            }
                            cells.reverse();
                            links.push(make_link(cx, clear, class, a as u32, b, &cells, dist[end] * CELL_M));
                        }
                    }
                    links
                })
            })
            .collect();
        for h in handles {
            out.extend(h.join().unwrap_or_default());
        }
    });
    // Two anchors that found each other give one link.
    let mut seen = std::collections::HashSet::new();
    out.sort_by(|x, y| (x.a.min(x.b), x.a.max(x.b), x.class as u8).cmp(&(y.a.min(y.b), y.a.max(y.b), y.class as u8)).then(x.length.total_cmp(&y.length)));
    out.retain(|l| seen.insert((l.a.min(l.b), l.a.max(l.b), l.class)));
    out
}

/// The nearest passable cell to `k` within one cell, if any.
fn snap(n: usize, pass: &[u8], k: usize) -> Option<usize> {
    if pass[k] > 0 {
        return Some(k);
    }
    let (x, z) = ((k % n) as i32, (k / n) as i32);
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
        let (nx, nz) = (x + dx, z + dz);
        if nx >= 0 && nz >= 0 && (nx as usize) < n && (nz as usize) < n {
            let j = nz as usize * n + nx as usize;
            if pass[j] > 0 {
                return Some(j);
            }
        }
    }
    None
}

fn make_link(cx: &Ctx, clear: &[Vec<f32>], class: Class, a: u32, b: u32, cells: &[usize], length: f32) -> Link {
    let cl = &clear[Class::ALL.iter().position(|&c| c == class).unwrap_or(0)];
    let pass = &cx.g[class.chan() as usize];
    // The narrowest point, leaving out the ends (an anchor in a doorway is
    // not a pinch on the way to it). Width is measured across the way, both
    // sides, since a shortest way hugs whatever it passes and the distance
    // to the nearest obstacle says nothing of the room on the other side.
    // Only cells whose nearest obstacle is close need the casting.
    let inner = if cells.len() > 4 { &cells[2..cells.len() - 2] } else { cells };
    let n = cx.n;
    let across = |i: usize| -> f32 {
        let k = inner[i];
        if cl[k] > PINCH_M * 2.0 {
            return cl[k];
        }
        let (a, b) = (inner[i.saturating_sub(2)], inner[(i + 2).min(inner.len() - 1)]);
        let (dx, dz) = ((b % n) as f32 - (a % n) as f32, (b / n) as f32 - (a / n) as f32);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 0.5 {
            return cl[k];
        }
        let (px, pz) = (-dz / len, dx / len);
        let side = |sgn: f32| {
            let mut d = 0.5f32;
            while d < 20.0 {
                let (x, z) = ((k % n) as f32 + 0.5 + px * d * sgn, (k / n) as f32 + 0.5 + pz * d * sgn);
                if x < 0.0 || z < 0.0 || x >= n as f32 || z >= n as f32 || pass[z as usize * n + x as usize] == 0 {
                    break;
                }
                d += 0.5;
            }
            d
        };
        (side(1.0) + side(-1.0)) * CELL_M
    };
    let (narrowest, narrow_k) = (0..inner.len()).map(|i| (across(i), inner[i])).min_by(|x, y| x.0.total_cmp(&y.0)).unwrap_or((f32::INFINITY, cells[0]));
    let cover = cells.iter().map(|&k| cx.ch(Chan::Cover, k) as f32 / 255.0).sum::<f32>() / cells.len().max(1) as f32;
    let pts: Vec<[f32; 2]> = cells.iter().map(|&k| {
        let (x, z) = cx.centre(k);
        [x, z]
    }).collect();
    let narrowest = if narrowest.is_finite() { narrowest } else { 999.0 };
    let (nx, nz) = cx.centre(narrow_k);
    Link {
        a,
        b,
        class,
        length,
        narrowest,
        cover,
        pinch: narrowest <= PINCH_M,
        path: simplify(&pts, 3.0),
        narrow_at: Some((nx, nz)),
    }
}

/// Douglas-Peucker: drop points within `tol` metres of the line through
/// their neighbours.
fn simplify(p: &[[f32; 2]], tol: f32) -> Vec<[f32; 2]> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let mut keep = vec![false; p.len()];
    keep[0] = true;
    keep[p.len() - 1] = true;
    let mut stack = vec![(0usize, p.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (ax, az, bx, bz) = (p[a][0], p[a][1], p[b][0], p[b][1]);
        let len = ((bx - ax).powi(2) + (bz - az).powi(2)).sqrt().max(1e-3);
        let mut far = (0.0, a);
        for i in a + 1..b {
            let d = ((bx - ax) * (az - p[i][1]) - (ax - p[i][0]) * (bz - az)).abs() / len;
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > tol {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    p.iter().zip(keep).filter(|(_, k)| *k).map(|(q, _)| *q).collect()
}

/// An f32 the heap can order (no NaN reaches it).
#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Ord32(f32);
impl Eq for Ord32 {}
impl Ord for Ord32 {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

// ---------------------------------------------------------------- audit

impl Graph {
    /// Per class: how many separate networks the links make, and how many
    /// anchors have no link at all.
    pub fn audit(&self) -> Vec<String> {
        let mut out = Vec::new();
        for class in Class::ALL {
            let mut o: Vec<u32> = (0..self.anchors.len() as u32).collect();
            fn root(o: &mut [u32], mut i: u32) -> u32 {
                while o[i as usize] != i {
                    o[i as usize] = o[o[i as usize] as usize];
                    i = o[i as usize];
                }
                i
            }
            let mut linked = vec![false; self.anchors.len()];
            let mut count = 0;
            for l in self.links.iter().filter(|l| l.class == class) {
                count += 1;
                linked[l.a as usize] = true;
                linked[l.b as usize] = true;
                let (ra, rb) = (root(&mut o, l.a), root(&mut o, l.b));
                if ra != rb {
                    o[ra as usize] = rb;
                }
            }
            let mut nets = std::collections::HashSet::new();
            for a in 0..self.anchors.len() as u32 {
                if linked[a as usize] {
                    nets.insert(root(&mut o, a));
                }
            }
            let alone = linked.iter().filter(|&&l| !l).count();
            out.push(format!("{} links: {count}, in {} networks; {alone} anchors with none", class.name(), nets.len()));
        }
        out
    }

    /// One line per kind: how many.
    pub fn counts(&self) -> String {
        Kind::ALL
            .iter()
            .map(|&k| format!("{} {}", k.name(), self.anchors.iter().filter(|a| a.kind == k).count()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_keeps_corners_and_drops_straight_runs() {
        let p: Vec<[f32; 2]> = (0..10).map(|i| [i as f32 * 4.0, 0.0]).chain((1..10).map(|i| [36.0, i as f32 * 4.0])).collect();
        let s = simplify(&p, 1.0);
        assert_eq!(s, vec![[0.0, 0.0], [36.0, 0.0], [36.0, 36.0]]);
    }

    #[test]
    fn densify_puts_points_half_a_cell_apart() {
        let d = densify(&[[0.0, 0.0], [8.0, 0.0]]);
        assert_eq!(d.len(), 5);
        assert_eq!(d[4], [8.0, 0.0]);
    }
}
