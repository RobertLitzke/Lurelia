//! The world survey (notes/WORLD_SURVEY_PLAN.md): what the ground offers,
//! measured once from the built world and read by anything that needs to
//! know where water, food, cover or a way through is. It records physical
//! facts only (how steep, how open, what grows); what a game makes of them
//! is that game's business.
//!
//! The static grids are 4 m a cell, one byte a cell per channel. A world
//! ships its survey as `survey.bin` beside its world.json. The file carries
//! a key: a hash of everything the survey reads from the built world (the
//! fields, structures, plants, lamps, roads, the material table) and of this
//! module's own source, less its comment lines and line endings. On load the
//! key is worked out again from the world
//! as it now builds; a file whose key differs is never used, but rebuilt and
//! rewritten, and `--validate` reports it, so a survey cannot fall out of
//! step with its world without someone being told.
//!
//! The game draws every channel on the ecology map (game/eco.rs, `[` and `]`
//! step through them), and `--eco-map NAME` draws one in a headless shot.

mod build;
pub mod graph;
pub mod terrain;

pub use graph::Graph;

use super::fields::World;
use std::path::Path;

/// Edge of one survey cell, metres.
pub const CELL_M: f32 = 4.0;

/// The file a world's survey ships in, beside its world.json.
pub const FILE: &str = "survey.bin";

const MAGIC: &[u8; 8] = b"VGSURV01";

/// How a channel's byte reads, for the map's legend and for `value`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    /// 0..255 is a share, 0 to 1.
    Share,
    /// Yes (255) or no (0).
    Flag,
    /// Metres away, 4 a step; 255 is that far or further (1 km).
    Distance,
    /// Metres above (over 128) or below the ground round about.
    Rise,
    /// A small number naming a class (`Chan::classes`).
    Class,
}

macro_rules! chans {
    ($($v:ident = $name:literal, $scale:ident, $about:literal;)*) => {
        /// One channel of the static grid.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Chan { $($v),* }

        impl Chan {
            pub const ALL: &'static [Chan] = &[$(Chan::$v),*];

            pub fn name(self) -> &'static str {
                match self { $(Chan::$v => $name),* }
            }

            pub fn scale(self) -> Scale {
                match self { $(Chan::$v => Scale::$scale),* }
            }

            /// One line for the map's legend.
            pub fn about(self) -> &'static str {
                match self { $(Chan::$v => $about),* }
            }
        }
    };
}

chans! {
    PassSmall = "pass_small", Flag, "A SMALL WALKER (HARE, FOX) CAN CROSS IT";
    PassPerson = "pass_person", Flag, "A PERSON ON FOOT CAN CROSS IT";
    PassLarge = "pass_large", Flag, "A LARGE WALKER (DEER, BISON) CAN CROSS IT";
    PassVehicle = "pass_vehicle", Flag, "A WHEELED VEHICLE CAN CROSS IT";
    Swim = "swim", Flag, "WATER DEEP ENOUGH TO SWIM";
    Water = "water", Class, "WATER: FRESH OR SALT, AND HOW DEEP";
    Drink = "drink", Distance, "HOW FAR TO A BANK BESIDE FRESH WATER";
    Browse = "browse", Share, "SHARE UNDER PLANTS THAT AFFORD BROWSE";
    Graze = "graze", Share, "SHARE OF DRY GRASS GROUND";
    Nectar = "nectar", Share, "SHARE UNDER PLANTS THAT AFFORD NECTAR";
    Cover = "cover", Share, "CANOPY, UNDERGROWTH AND LOW WALLS TO HIDE IN";
    Roof = "roof", Share, "SHARE WITH A ROOF OVERHEAD";
    Open = "open", Share, "HOW OPEN THE HORIZON IS FROM EYE HEIGHT, TO 100 M";
    Rise50 = "rise50", Rise, "HEIGHT OVER THE GROUND WITHIN 50 M";
    Rise200 = "rise200", Rise, "HEIGHT OVER THE GROUND WITHIN 200 M";
    Built = "built", Share, "SHARE BUILT UP (2 M AND TALLER) WITHIN 20 M";
    Loud = "loud", Share, "HOW LOUD A STEP IS (0.5 ORDINARY GROUND)";
    Print = "print", Class, "WHAT A FOOT LEAVES IN THE GROUND";
    Light = "light", Share, "LAMPLIGHT";
    Road = "road", Distance, "HOW FAR TO A ROAD OR PATH";
}

impl Chan {
    pub fn parse(s: &str) -> Option<Chan> {
        Chan::ALL.iter().copied().find(|c| c.name() == s)
    }

    /// The names of a `Class` channel's values, by byte.
    pub fn classes(self) -> &'static [&'static str] {
        match self {
            Chan::Water => &["DRY", "FRESH, ANKLE DEEP", "FRESH, WADING", "FRESH, DEEP", "SALT, SHALLOW", "SALT, DEEP"],
            Chan::Print => &["NOTHING", "BENDS (GRASS)", "WHEN SOFT (EARTH)", "A PRINT (SNOW, MUD, SAND)"],
            _ => &[],
        }
    }
}

/// Water classes (`Chan::Water`).
pub mod water {
    pub const DRY: u8 = 0;
    pub const FRESH_SHALLOW: u8 = 1;
    pub const FRESH_WADE: u8 = 2;
    pub const FRESH_DEEP: u8 = 3;
    pub const SALT_SHALLOW: u8 = 4;
    pub const SALT_DEEP: u8 = 5;
}

/// A world's static survey grids.
pub struct Survey {
    /// Cells along each side.
    pub n: usize,
    /// The world's edge, metres.
    pub size: f32,
    /// The hash of what the survey was measured from (see the module doc).
    pub key: u64,
    /// One byte a cell, per channel in `Chan::ALL` order.
    chans: Vec<Vec<u8>>,
    /// How it got here, for the map's legend: "read" or "built in N ms".
    pub origin: String,
    /// Zones, anchors and the links between them (graph.rs).
    pub graph: Graph,
}

impl Survey {
    /// Cell index of a world point, clamped to the grid.
    pub fn idx(&self, x: f32, z: f32) -> usize {
        cell_index(self.n, x, z)
    }

    pub fn grid(&self, c: Chan) -> &[u8] {
        &self.chans[c as usize]
    }

    pub fn byte(&self, c: Chan, x: f32, z: f32) -> u8 {
        self.chans[c as usize][self.idx(x, z)]
    }

    /// The channel's value at a world point in its own units: a share or
    /// flag 0 to 1, metres for a distance or a rise, the class number.
    pub fn value(&self, c: Chan, x: f32, z: f32) -> f32 {
        decode(c, self.byte(c, x, z))
    }

    /// A survey of `n` cells a side with every channel zero, for tests that
    /// set the grids and the graph they need.
    #[cfg(test)]
    pub(crate) fn blank(n: usize) -> Survey {
        Survey { n, size: n as f32 * CELL_M, key: 0, chans: vec![vec![0; n * n]; Chan::ALL.len()], origin: "test".into(), graph: Graph::default() }
    }

    #[cfg(test)]
    pub(crate) fn grid_mut(&mut self, c: Chan) -> &mut [u8] {
        &mut self.chans[c as usize]
    }

    /// Measure `w` now.
    pub fn build(w: &World) -> Survey {
        let t = std::time::Instant::now();
        let key = key(w);
        let (n, chans, ground) = build::grids(w);
        let grids_ms = t.elapsed().as_secs_f64() * 1000.0;
        let graph = graph::build(w, n, &chans, &ground);
        let origin = format!("built in {:.0} ms (graph {:.0})", t.elapsed().as_secs_f64() * 1000.0, t.elapsed().as_secs_f64() * 1000.0 - grids_ms);
        Survey { n, size: w.f.size, key, chans, origin, graph }
    }

    /// The survey shipped in `dir` if it was measured from this world as it
    /// builds now; otherwise one measured afresh, written back to `dir`
    /// (problems writing are reported, not fatal).
    pub fn load_or_build(w: &World, dir: &Path) -> Survey {
        let path = dir.join(FILE);
        let key = key(w);
        let why = match read(&path) {
            Ok(s) if s.key == key && s.n == cells(w) => return Survey { origin: "read from survey.bin".into(), ..s },
            Ok(_) => "stale",
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "missing",
            Err(_) => "unreadable",
        };
        let s = Survey::build(w);
        match write(&s, &path) {
            Ok(()) => eprintln!("world {}: survey {why}, {} and written to {}", w.def.name, s.origin, path.display()),
            Err(e) => eprintln!("world {}: survey {why}, {}; could not write {}: {e}", w.def.name, s.origin, path.display()),
        }
        s
    }

    /// For `--validate`: what is wrong with the survey shipped in `dir`.
    pub fn check(w: &World, dir: &Path) -> Option<String> {
        let path = dir.join(FILE);
        match read(&path) {
            Ok(s) if s.key == key(w) && s.n == cells(w) => None,
            Ok(_) => Some(format!("{FILE} is stale: the world or the survey code changed since it was measured; run the world once, or --survey, to rewrite it")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(format!("no {FILE}: run the world once, or --survey, to write it")),
            Err(e) => Some(format!("{FILE} unreadable ({e}); run --survey to rewrite it")),
        }
    }

    /// Lines for `--survey`: each channel's spread over the land.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![format!("survey: {} x {} cells of {CELL_M} m, key {:016x}, {}", self.n, self.n, self.key, self.origin)];
        for &c in Chan::ALL {
            let g = self.grid(c);
            let mut hist = [0usize; 256];
            for &b in g {
                hist[b as usize] += 1;
            }
            let pct = |lo: usize| (hist[lo..].iter().sum::<usize>() as f64 / g.len() as f64 * 100.0).round();
            let mean = g.iter().map(|&b| decode(c, b) as f64).sum::<f64>() / g.len() as f64;
            let line = match c.scale() {
                Scale::Class => {
                    let names = c.classes();
                    let parts: Vec<String> =
                        (0..names.len()).filter(|&k| hist[k] > 0).map(|k| format!("{} {:.0}%", names[k].to_lowercase(), hist[k] as f64 / g.len() as f64 * 100.0)).collect();
                    parts.join(", ")
                }
                Scale::Flag => format!("{:.0}% yes", pct(128)),
                Scale::Rise => {
                    let abs = g.iter().map(|&b| decode(c, b).abs() as f64).sum::<f64>() / g.len() as f64;
                    format!("mean {abs:.1} m above or below, highest {:+.0} m", g.iter().map(|&b| decode(c, b)).fold(f32::MIN, f32::max))
                }
                _ => format!("mean {mean:.2}, nonzero {:.0}%", pct(1)),
            };
            out.push(format!("  {:<12} {line}", c.name()));
        }
        let g = &self.graph;
        out.push(format!("graph: {} zones ({} named), {} anchors, {} links", g.zones.len(), g.zones.iter().filter(|z| z.named).count(), g.anchors.len(), g.links.len()));
        out.push(format!("  anchors: {}", g.counts()));
        for l in g.audit() {
            out.push(format!("  {l}"));
        }
        out
    }
}

/// `--survey`: build world `name`, measure its survey afresh, write it
/// beside the world and print each channel. The exit code.
pub fn command(name: &str) -> i32 {
    let dir = crate::worldload::world_dir(name);
    let (edges, _) = crate::worldload::edges_for(name);
    // The world's own material table, installed last (ground materials are
    // part of the key).
    let (lib, _) = crate::worldload::library_for(name);
    let w = match super::chronicle::build_world_with(&dir, lib, &edges) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("world {name}: {e}");
            return 1;
        }
    };
    let s = Survey::build(&w);
    let path = dir.join(FILE);
    if let Err(e) = write(&s, &path) {
        eprintln!("could not write {}: {e}", path.display());
        return 1;
    }
    println!("world {name}: wrote {}", path.display());
    for l in s.report() {
        println!("{l}");
    }
    0
}

/// A channel's byte in its own units.
pub fn decode(c: Chan, b: u8) -> f32 {
    match c.scale() {
        Scale::Share | Scale::Flag => b as f32 / 255.0,
        Scale::Distance => b as f32 * CELL_M,
        Scale::Rise => b as f32 - 128.0,
        Scale::Class => b as f32,
    }
}

fn cells(w: &World) -> usize {
    side(w.f.size)
}

/// Cells along each side of a survey grid for a world `size` metres across.
/// Other 4 m grids (world/site.rs) use this and `cell_index` so a cell is
/// the same ground in all of them.
pub fn side(size: f32) -> usize {
    (size / CELL_M).ceil() as usize
}

/// Index of the cell holding world point (x, z) on a grid `n` cells a side,
/// clamped to the grid.
pub fn cell_index(n: usize, x: f32, z: f32) -> usize {
    let c = |v: f32| ((v / CELL_M) as i32).clamp(0, n as i32 - 1) as usize;
    c(z) * n + c(x)
}

/// A 64-bit hash that is the same on every run and machine (std's is not
/// promised to be).
struct Hash(u64);

impl Hash {
    fn new() -> Hash {
        Hash(0x9e37_79b9_7f4a_7c15)
    }

    fn u64(&mut self, v: u64) {
        let x = (self.0 ^ v).wrapping_mul(0xff51_afd7_ed55_8ccd);
        self.0 = (x ^ (x >> 32)).wrapping_mul(0xc4ce_b9fe_1a85_ec53) ^ (x >> 29);
    }

    fn f32(&mut self, v: f32) {
        self.u64(v.to_bits() as u64);
    }

    fn bytes(&mut self, b: &[u8]) {
        let mut it = b.chunks_exact(8);
        for c in &mut it {
            self.u64(u64::from_le_bytes(c.try_into().unwrap()));
        }
        let mut last = [0u8; 8];
        last[..it.remainder().len()].copy_from_slice(it.remainder());
        self.u64(u64::from_le_bytes(last) ^ (b.len() as u64) << 56);
    }

    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

/// Everything the survey reads from a world, hashed, with this module's own
/// source, so a change to either makes a shipped survey stale.
pub fn key(w: &World) -> u64 {
    let f = &w.f;
    let mut h = Hash::new();
    // The code that measures it, less what cannot change what it measures:
    // carriage returns (a checkout's line endings) and comment lines.
    for src in [include_str!("mod.rs"), include_str!("build.rs"), include_str!("graph.rs")] {
        let code: String = src.lines().map(str::trim_end).filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        h.str(&code);
    }
    h.f32(f.size);
    h.f32(f.sea);
    for v in [&f.h, &f.cliff] {
        let mut b = Vec::with_capacity(v.len() * 4);
        for x in v.iter() {
            b.extend_from_slice(&x.to_bits().to_le_bytes());
        }
        h.bytes(&b);
    }
    h.bytes(&f.water);
    h.bytes(&f.surface);
    h.bytes(&f.biome);
    h.bytes(&f.occupied.iter().map(|&o| o as u8).collect::<Vec<_>>());
    // The river water is a map, so its order is not fixed: sort it.
    let mut wet: Vec<(u32, u32)> = f.inland.iter().map(|(k, v)| (*k, v.level.to_bits())).collect();
    wet.sort_unstable();
    for (k, l) in wet {
        h.u64((k as u64) << 32 | l as u64);
    }
    for (k, p) in w.placed.iter().enumerate() {
        let s = &w.shapes[p.shape];
        h.u64(p.shape as u64 ^ (p.rot as u64) << 40 ^ (k as u64) << 48);
        h.u64((p.origin[0] as u64) << 42 ^ (p.origin[1] as u32 as u64) << 21 ^ p.origin[2] as u32 as u64);
        h.u64(s.size[0] as u64 ^ (s.size[1] as u64) << 20 ^ (s.size[2] as u64) << 40 ^ (s.solid as u64) << 8);
    }
    for (k, _) in &w.when {
        h.u64(*k as u64 | 1 << 63);
    }
    // Plants are placed from many threads: hash them in any order.
    let mut trees = 0u64;
    for t in &w.trees {
        let mut th = Hash::new();
        for v in [t.pos.x, t.pos.y, t.pos.z, t.top, t.radius, t.trunk_r, t.canopy_lo] {
            th.f32(v);
        }
        th.u64(t.plant as u64);
        trees = trees.wrapping_add(th.0);
    }
    h.u64(trees);
    h.u64(w.trees.len() as u64);
    for l in &w.lamps {
        for v in [l.pos.x, l.pos.y, l.pos.z, l.reach, l.strength] {
            h.f32(v);
        }
    }
    for s in &w.ribbons.segs {
        for v in [s.a.x, s.a.z, s.b.x, s.b.z] {
            h.f32(v);
        }
    }
    for c in &w.circuits {
        h.u64(c.points.len() as u64);
        for p in &c.points {
            h.f32(p.x);
            h.f32(p.y);
        }
    }
    for b in &w.city.buildings {
        for v in [b.lo.x, b.lo.y, b.hi.x, b.hi.y, b.wall_top] {
            h.f32(v);
        }
    }
    // What each plant affords, and each ground material's name, since the
    // grids read them.
    for p in &w.lib.plants {
        h.str(&p.name);
        for a in &p.affords {
            h.str(a);
        }
    }
    for m in 0..=255u8 {
        h.str(super::matlib::key(m).or(super::matlib::name(m)).unwrap_or(""));
        h.u64(super::matlib::has(m, super::matlib::flag::UNDERSTORY) as u64 | (super::matlib::grass(m).is_some() as u64) << 1);
    }
    for b in &w.biome_mats {
        h.u64(b.surface as u64);
    }
    // What the graph reads: the named areas, the door leaves, the biome
    // names and the placed structures' tags.
    for a in &w.areas {
        h.str(&a.name);
        h.str(&a.kind);
        h.f32(a.area_m2);
        for p in &a.poly {
            h.f32(p.x);
            h.f32(p.y);
        }
    }
    h.bytes(&w.doors);
    for b in &w.lib.biomes {
        h.str(&b.name);
    }
    for p in &w.placed {
        for t in w.lib.structures.get(p.def).map(|d| d.tags.as_slice()).unwrap_or(&[]) {
            h.str(t);
        }
    }
    h.0
}

fn write(s: &Survey, path: &Path) -> std::io::Result<()> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&(s.n as u32).to_le_bytes());
    raw.extend_from_slice(&s.size.to_le_bytes());
    raw.extend_from_slice(&(Chan::ALL.len() as u32).to_le_bytes());
    for &c in Chan::ALL {
        raw.push(c.name().len() as u8);
        raw.extend_from_slice(c.name().as_bytes());
        raw.extend_from_slice(s.grid(c));
    }
    let graph = serde_json::to_vec(&s.graph).map_err(std::io::Error::other)?;
    raw.extend_from_slice(&(graph.len() as u32).to_le_bytes());
    raw.extend_from_slice(&graph);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&s.key.to_le_bytes());
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(&raw, 8));
    // Written aside and moved into place, so a reader never meets half a file
    // and two games writing at once leave one whole one.
    let tmp = path.with_extension(format!("bin.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &out)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn read(path: &Path) -> std::io::Result<Survey> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let data = std::fs::read(path)?;
    if data.len() < 16 || &data[..8] != MAGIC {
        return Err(bad("not a survey file"));
    }
    let key = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let raw = miniz_oxide::inflate::decompress_to_vec(&data[16..]).map_err(|_| bad("corrupt"))?;
    let mut at = 0usize;
    let mut take = |k: usize| -> std::io::Result<&[u8]> {
        let s = raw.get(at..at + k).ok_or_else(|| bad("short"))?;
        at += k;
        Ok(s)
    };
    let n = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
    let size = f32::from_le_bytes(take(4)?.try_into().unwrap());
    let count = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
    let mut chans = vec![Vec::new(); Chan::ALL.len()];
    for _ in 0..count {
        let len = take(1)?[0] as usize;
        let name = String::from_utf8_lossy(take(len)?).into_owned();
        let g = take(n * n)?.to_vec();
        // A channel this build does not know is skipped; one it misses
        // leaves its grid empty, and the length check below turns that
        // into a rebuild.
        if let Some(c) = Chan::parse(&name) {
            chans[c as usize] = g;
        }
    }
    if chans.iter().any(|g| g.len() != n * n) {
        return Err(bad("missing channels"));
    }
    let len = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
    let graph: Graph = serde_json::from_slice(take(len)?).map_err(|_| bad("graph unreadable"))?;
    if graph.zone_of.len() != n * n {
        return Err(bad("graph does not fit the grid"));
    }
    Ok(Survey { n, size, key, chans, origin: String::new(), graph })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_survey_file_reads_back_as_written() {
        let n = 5;
        let chans: Vec<Vec<u8>> = Chan::ALL.iter().enumerate().map(|(k, _)| (0..n * n).map(|i| (i * 7 + k) as u8).collect()).collect();
        let graph = Graph { zone_of: vec![1; n * n], zones: vec![graph::Zone { name: "a".into(), ..Default::default() }], ..Default::default() };
        let s = Survey { n, size: 20.0, key: 0xdead_beef, chans, origin: String::new(), graph };
        let dir = std::env::temp_dir().join(format!("survey_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        write(&s, &path).unwrap();
        let r = read(&path).unwrap();
        assert_eq!((r.n, r.key, r.size), (n, s.key, s.size));
        for &c in Chan::ALL {
            assert_eq!(r.grid(c), s.grid(c), "{}", c.name());
        }
        assert_eq!(r.graph.zones[0].name, "a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decoding_reads_each_scale() {
        assert_eq!(decode(Chan::Drink, 10), 40.0);
        assert_eq!(decode(Chan::Rise50, 130), 2.0);
        assert_eq!(decode(Chan::Water, 3), 3.0);
        assert!((decode(Chan::Cover, 255) - 1.0).abs() < 1e-6);
    }
}
