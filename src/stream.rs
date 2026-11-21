//! Chunk streaming: decides which columns should exist at which level of detail,
//! generates and meshes them on worker threads nearest-first, and swaps them in
//! without holes or overlap.
//!
//! The level-of-detail layout is a quadtree over the world footprint. A node at
//! level L > 1 splits while the player is closer than `view_m * 2^(L-1)` to it, so
//! level 1 reaches out to twice the view distance, level 2 to four times, and so on.
//! Level 1 splits into full detail only within `full_m`, which is the view
//! distance up to `FULL_DETAIL_M`: past that a 25 cm voxel is under a pixel at
//! 1080p, so a longer view keeps the far rings where it puts them and spends
//! nothing more up close.
//!
//! While the layout changes, a column that is no longer wanted ("stale") keeps
//! drawing until every wanted column overlapping it has loaded; wanted columns that
//! overlap a stale one stay hidden until then. Coarse and fine terrain never draw
//! over each other and the ground never has holes.

use crate::mesh::{mesh_chunk_t, sprites, Mesh};
use crate::util::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::world::gen::*;
use crate::world::store::{Store, Voxels};
use glam::Vec3;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The farthest full detail (level 0) reaches, metres, however long the view.
/// At 256 m it drew three times the quads of 128 m facing a forest and looked
/// the same at 1080p (docs/PERFORMANCE.md). `--full-detail` overrides it.
pub const FULL_DETAIL_M: f32 = 128.0;

#[derive(Clone, Copy, Debug)]
pub struct GpuChunk {
    pub cy: i32,
    pub slot: u32,
    pub offset: u32,
    pub dir_counts: [u32; 6],
    pub tails: [[u32; 6]; crate::mesh::LEANS.len() + 1],
    pub rtails: [[u32; 6]; crate::mesh::LEANS.len() + 1],
    /// Sprite records after the face groups (see mesh/sprites.rs): fill tufts
    /// first, then the other grass, then the rest (Mesh::sprite_fill).
    pub sprites: u32,
    pub sprite_fill: u32,
    pub sprite_grass: u32,
    /// The far grass patches, last, and the flower stalks before them.
    pub sprite_patch: u32,
    pub sprite_stalk: u32,
    pub min: Vec3,
    pub max: Vec3,
}

impl GpuChunk {
    pub fn quads(&self) -> u32 {
        self.dir_counts.iter().sum::<u32>() + self.sprites
    }
}

/// Where meshes go: the renderer's GPU buffers, or nothing in tests.
pub trait ChunkSink {
    fn upload(&mut self, origin: Vec3, scale: f32, mesh: &Mesh) -> Option<GpuChunk>;
    fn free(&mut self, c: &GpuChunk);
}

enum Job {
    /// Generate and mesh a column. `true` regenerates one already loaded (after
    /// the slopes setting changed); those survive rescheduling.
    Column(ColKey, bool),
    Remesh {
        chunk: (i32, i32, i32),
        padded: Box<[u8; PAD3]>,
        version: u32,
    },
}

enum JobResult {
    Column {
        key: ColKey,
        chunks: Vec<(i32, Mesh, Option<Voxels>)>,
        solid_below: i32,
        count: i32,
        micros: u64,
        epoch: u64,
        smooth: bool,
    },
    Remesh {
        chunk: (i32, i32, i32),
        mesh: Mesh,
        version: u32,
        smooth: bool,
    },
}

struct QueueState {
    jobs: Vec<(f32, Job)>,
    shutdown: bool,
    busy: usize,
}

struct Shared {
    queue: Mutex<QueueState>,
    cv: Condvar,
    edits: RwLock<(EditMap, u64)>,
    gen: Generator,
    smooth: std::sync::atomic::AtomicBool,
}

#[derive(Default, Clone, Debug)]
pub struct StreamStats {
    pub columns: usize,
    pub chunks: usize,
    pub quads: u64,
    pub queued: usize,
    pub busy: usize,
    pub gen_jobs: u64,
    pub gen_micros: u64,
    pub chunks_generated: u64,
    pub per_lod: [usize; 8],
    pub accept_ms: f32,
    pub plan_ms: f32,
    pub vis_ms: f32,
    pub accepted: u32,
    pub unload_ms: f32,
    pub unloaded: u32,
}

/// Free voxel chunks on a background thread. Unloading a few hundred columns
/// at once (turning, or moving fast) freed thousands of 32 KB boxes on the
/// main thread, 25 to 55 ms in one frame.
/// The `full_box` for a match: the box round its walk graph, padded 12 m, if
/// it is no bigger than an arena. Past `full_m` from the player there is
/// otherwise no floor in the store, and a bot or a respawn at the far base
/// of a map wider than that (Starfall) stands on nothing.
pub fn arena_box(nav: &crate::world::fields::NavDef) -> Option<[f32; 4]> {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for n in &nav.nodes {
        b = [b[0].min(n.at[0]), b[1].min(n.at[1]), b[2].max(n.at[0]), b[3].max(n.at[1])];
    }
    const PAD_M: f32 = 12.0;
    (b[0] <= b[2] && b[2] - b[0] < 400.0 && b[3] - b[1] < 400.0).then(|| [b[0] - PAD_M, b[1] - PAD_M, b[2] + PAD_M, b[3] + PAD_M])
}

fn free_later(dead: Vec<Voxels>) {
    static TX: std::sync::OnceLock<Sender<Vec<Voxels>>> = std::sync::OnceLock::new();
    if dead.is_empty() {
        return;
    }
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel::<Vec<Voxels>>();
        std::thread::Builder::new().name("voxel free".into()).spawn(move || while rx.recv().is_ok() {}).expect("free thread");
        tx
    });
    let _ = tx.send(dead);
}

pub struct Streamer {
    shared: Arc<Shared>,
    rx: Receiver<JobResult>,
    threads: Vec<JoinHandle<()>>,
    pub loaded: HashMap<ColKey, Vec<GpuChunk>>,
    pub desired: HashSet<ColKey>,
    pending: HashSet<ColKey>,
    hidden: HashSet<ColKey>,
    last_center: Option<Vec3>,
    /// The view distance: where the rings of level 1 and coarser end.
    pub view_m: f32,
    /// The radius of full detail (level 0), at most the view distance.
    pub full_m: f32,
    /// A box (min x, min z, max x, max z, metres) kept at full detail however
    /// far the player is from it: a match's arena, so a bot or a respawn at
    /// the far end of a map wider than `full_m` has a floor to stand on.
    pub full_box: Option<[f32; 4]>,
    roots: Vec<ColKey>,
    max_lod: u8,
    chunk_versions: HashMap<(i32, i32, i32), u32>,
    pub stats: StreamStats,
    pub dirty_edits: bool,
}

impl Streamer {
    pub fn new(gen: Generator, edits: EditMap, view_m: f32, threads: usize) -> Self {
        let world_vox = gen.world_vox;
        let mut max_lod = 0u8;
        while (CHUNK << max_lod) < world_vox {
            max_lod += 1;
        }
        let root_size = CHUNK << max_lod;
        let n = (world_vox + root_size - 1) / root_size;
        let mut roots = Vec::new();
        for z in 0..n {
            for x in 0..n {
                roots.push(ColKey { lod: max_lod, x, z });
            }
        }
        let shared = Arc::new(Shared {
            queue: Mutex::new(QueueState {
                jobs: Vec::new(),
                shutdown: false,
                busy: 0,
            }),
            cv: Condvar::new(),
            edits: RwLock::new((edits, 0)),
            gen,
            smooth: std::sync::atomic::AtomicBool::new(false),
        });
        let (tx, rx) = channel();
        let threads = (0..threads.max(1))
            .map(|i| {
                let s = shared.clone();
                let tx = tx.clone();
                std::thread::Builder::new().name(format!("chunk-worker-{i}")).spawn(move || worker(s, tx)).unwrap()
            })
            .collect();
        Self {
            shared,
            rx,
            threads,
            loaded: HashMap::default(),
            desired: HashSet::default(),
            pending: HashSet::default(),
            hidden: HashSet::default(),
            last_center: None,
            view_m,
            full_m: view_m.min(FULL_DETAIL_M),
            full_box: None,
            roots,
            max_lod,
            chunk_versions: HashMap::default(),
            stats: StreamStats::default(),
            dirty_edits: false,
        }
    }

    pub fn smooth(&self) -> bool {
        self.shared.smooth.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Turn sloped ground on or off. Loaded columns are regenerated nearest first
    /// and swapped in place as they finish, so nothing disappears meanwhile.
    pub fn set_smooth(&mut self, on: bool, p: Vec3) {
        if on == self.smooth() {
            return;
        }
        self.shared.smooth.store(on, std::sync::atomic::Ordering::Relaxed);
        let mut keys: Vec<ColKey> = self.loaded.keys().copied().collect();
        keys.sort_by(|a, b| b.dist_m(p.x, p.z).total_cmp(&a.dist_m(p.x, p.z)));
        self.queue_regen(&keys);
    }

    /// Put the world in another part of the year (world/season.rs). Every
    /// loaded column is built again, nearest first and swapped in place, so
    /// the leaves turn across the view over a few seconds rather than all at
    /// once. A turn that changes nothing baked -- a world of one season, or a
    /// step the trees cannot tell apart -- costs nothing.
    pub fn set_turn(&mut self, t: crate::world::season::Turn, p: Vec3) {
        let was = self.shared.gen.turn();
        if was == t {
            return;
        }
        self.shared.gen.set_turn(t);
        // Nothing to do where the trees cannot tell the two apart, or where
        // the turn has only just begun and none of them has changed yet.
        if was.snow == t.snow && (!t.moves() || (t.step == 0 && was.to == t.from)) {
            return;
        }
        let mut keys: Vec<ColKey> = self.loaded.keys().copied().collect();
        keys.sort_by(|a, b| b.dist_m(p.x, p.z).total_cmp(&a.dist_m(p.x, p.z)));
        self.queue_regen(&keys);
    }

    /// Queue regeneration of loaded columns; the last key runs first.
    fn queue_regen(&mut self, keys: &[ColKey]) {
        let mut q = self.shared.queue.lock().unwrap();
        for k in keys {
            q.jobs.push((0.0, Job::Column(*k, true)));
        }
        self.shared.cv.notify_all();
    }

    /// Regenerate the loaded columns over a rectangle of the world (metres),
    /// nearest to `p` first: something drawn there changed (a structure that
    /// stands only at night).
    pub fn regen_area(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, p: Vec3) {
        let mut keys: Vec<ColKey> = self
            .loaded
            .keys()
            .copied()
            .filter(|k| {
                let s = k.size_m();
                let (cx0, cz0) = (k.x as f32 * s, k.z as f32 * s);
                cx0 < x1 && cx0 + s > x0 && cz0 < z1 && cz0 + s > z0
            })
            .collect();
        keys.sort_by(|a, b| b.dist_m(p.x, p.z).total_cmp(&a.dist_m(p.x, p.z)));
        self.queue_regen(&keys);
    }

    pub fn generator(&self) -> &Generator {
        &self.shared.gen
    }

    /// Nothing queued, running or waiting to be accepted.
    pub fn idle(&self) -> bool {
        let q = self.shared.queue.lock().unwrap();
        q.jobs.is_empty() && q.busy == 0 && self.pending.is_empty()
    }

    /// How tall the world is, metres: the top of any column's box.
    pub fn world_height_m(&self) -> f32 {
        self.shared.gen.height_vox as f32 * VOXEL
    }

    pub fn is_drawn(&self, k: &ColKey) -> bool {
        self.loaded.contains_key(k) && (!self.desired.contains(k) || !self.hidden.contains(k))
    }

    /// Accept finished work for up to `budget`, then re-plan if the player moved.
    pub fn update(&mut self, p: Vec3, fwd: Vec3, sink: &mut dyn ChunkSink, store: &mut Store, budget: Duration) {
        if store.ground.is_none() && std::env::var_os("VG_ROUND_GROUND").is_none() {
            let sh = self.shared.clone();
            store.ground = Some(std::sync::Arc::new(move |x, z| sh.gen.ground_vox(x, z)));
        }
        let start = Instant::now();
        let mut changed = false;
        let mut accepted = 0;
        while start.elapsed() < budget {
            match self.rx.try_recv() {
                Ok(r) => {
                    self.accept(r, sink, store);
                    changed = true;
                    accepted += 1;
                }
                Err(_) => break,
            }
        }
        let t_accept = start.elapsed();
        let moved = self.last_center.map_or(true, |c| (c - p).length() > 4.0);
        if moved {
            self.last_center = Some(p);
            let mut desired = HashSet::default();
            for r in self.roots.clone() {
                self.visit(r, p, &mut desired);
            }
            self.desired = desired;
            self.reschedule(p, fwd);
            changed = true;
        }
        let t_plan = start.elapsed();
        if changed {
            self.refresh_visibility(sink, store);
        }
        let t_vis = start.elapsed();
        self.stats.accept_ms = t_accept.as_secs_f32() * 1000.0;
        self.stats.plan_ms = (t_plan - t_accept).as_secs_f32() * 1000.0;
        self.stats.vis_ms = (t_vis - t_plan).as_secs_f32() * 1000.0;
        self.stats.accepted = accepted;
        let q = self.shared.queue.lock().unwrap();
        self.stats.queued = q.jobs.len();
        self.stats.busy = q.busy;
    }

    fn visit(&self, k: ColKey, p: Vec3, out: &mut HashSet<ColKey>) {
        let reach = if k.lod == 1 { self.full_m } else { self.view_m * (1u32 << k.lod.saturating_sub(1)) as f32 };
        let held = self.full_box.is_some_and(|[x0, z0, x1, z1]| {
            let (lo, hi) = k.footprint_m();
            lo.x < x1 && hi.x > x0 && lo.y < z1 && hi.y > z0
        });
        if k.lod > 0 && (k.dist_m(p.x, p.z) < reach || held) {
            for c in k.children() {
                self.visit(c, p, out);
            }
        } else {
            out.insert(k);
        }
    }

    fn reschedule(&mut self, p: Vec3, fwd: Vec3) {
        let mut q = self.shared.queue.lock().unwrap();
        let old = std::mem::take(&mut q.jobs);
        let mut not_started = HashSet::default();
        let mut keep = Vec::new();
        for (prio, j) in old {
            match j {
                Job::Column(k, false) => {
                    not_started.insert(k);
                }
                other => keep.push((prio, other)),
            }
        }
        // Whatever was pending but not in the queue is running right now.
        self.pending.retain(|k| !not_started.contains(k));
        let f = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
        let mut jobs: Vec<(f32, Job)> = Vec::new();
        for k in &self.desired {
            if self.loaded.contains_key(k) || self.pending.contains(k) {
                continue;
            }
            let s = k.size_m();
            let c = Vec3::new((k.x as f32 + 0.5) * s - p.x, 0.0, (k.z as f32 + 0.5) * s - p.z);
            let d = k.dist_m(p.x, p.z);
            let ahead = d < 16.0 || c.normalize_or_zero().dot(f) > -0.3;
            jobs.push((if ahead { d } else { d * 2.5 + 20.0 }, Job::Column(*k, false)));
        }
        for (_, j) in &jobs {
            if let Job::Column(k, _) = j {
                self.pending.insert(*k);
            }
        }
        jobs.sort_by(|a, b| b.0.total_cmp(&a.0));
        jobs.extend(keep);
        q.jobs = jobs;
        self.shared.cv.notify_all();
    }

    fn accept(&mut self, r: JobResult, sink: &mut dyn ChunkSink, store: &mut Store) {
        match r {
            JobResult::Column {
                key,
                chunks,
                solid_below,
                count,
                micros,
                epoch,
                smooth,
            } => {
                self.pending.remove(&key);
                if smooth != self.smooth() && self.desired.contains(&key) {
                    // Meshed before the setting changed: show it, but redo it.
                    self.queue_regen(&[key]);
                }
                self.stats.gen_jobs += 1;
                self.stats.gen_micros += micros;
                self.stats.chunks_generated += chunks.len() as u64;
                if !self.desired.contains(&key) {
                    return;
                }
                if let Some(old) = self.loaded.remove(&key) {
                    for c in &old {
                        sink.free(c);
                    }
                }
                let scale = VOXEL * (1u32 << key.lod) as f32;
                let size = key.size_m();
                let chunk_m = 32.0 * scale;
                let mut list = Vec::new();
                let mut vox = Vec::new();
                for (cy, mesh, v) in chunks {
                    // Herb markers mesh as air, so a chunk can hold sprites and no quads.
                    if !mesh.quads.is_empty() || !mesh.sprites.is_empty() {
                        let origin = Vec3::new(key.x as f32 * size, cy as f32 * chunk_m, key.z as f32 * size);
                        if let Some(mut g) = sink.upload(origin, scale, &mesh) {
                            g.cy = cy;
                            list.push(g);
                        }
                    }
                    if let Some(v) = v {
                        vox.push((cy, v));
                    }
                }
                self.loaded.insert(key, list);
                if key.lod == 0 {
                    store.insert_column(key.x, key.z, count, solid_below, vox);
                    // Edits made while this column was generating are missing from it.
                    let edits = self.shared.edits.read().unwrap();
                    if edits.1 != epoch {
                        let mut touched = Vec::new();
                        for (&(cx, cy, cz), list) in edits.0.iter() {
                            if (cx - key.x).abs() <= 1 && (cz - key.z).abs() <= 1 && !list.is_empty() {
                                if cx == key.x && cz == key.z {
                                    for (&i, &m) in list {
                                        let i = i as i32;
                                        store.set(cx * 32 + i % 32, cy * 32 + i / 1024, cz * 32 + (i / 32) % 32, m);
                                    }
                                }
                                for dy in -1..=1 {
                                    touched.push((key.x, cy + dy, key.z));
                                }
                            }
                        }
                        drop(edits);
                        touched.sort();
                        touched.dedup();
                        for c in touched {
                            if c.1 >= 0 && c.1 < count {
                                self.remesh_chunk(c, store);
                            }
                        }
                    }
                }
            }
            JobResult::Remesh { chunk, mesh, version, smooth } => {
                if self.chunk_versions.get(&chunk) != Some(&version) {
                    return;
                }
                if smooth != self.smooth() {
                    self.remesh_chunk(chunk, store);
                }
                let key = ColKey { lod: 0, x: chunk.0, z: chunk.2 };
                let Some(list) = self.loaded.get_mut(&key) else {
                    return;
                };
                if let Some(i) = list.iter().position(|c| c.cy == chunk.1) {
                    let old = list.swap_remove(i);
                    sink.free(&old);
                }
                if !mesh.quads.is_empty() || !mesh.sprites.is_empty() {
                    let origin = Vec3::new(chunk.0 as f32 * 8.0, chunk.1 as f32 * 8.0, chunk.2 as f32 * 8.0);
                    if let Some(mut g) = sink.upload(origin, VOXEL, &mesh) {
                        g.cy = chunk.1;
                        list.push(g);
                    }
                }
            }
        }
    }

    fn refresh_visibility(&mut self, sink: &mut dyn ChunkSink, store: &mut Store) {
        self.hidden.clear();
        let stale: Vec<ColKey> = self.loaded.keys().filter(|k| !self.desired.contains(k)).copied().collect();
        let mut unload_t = Duration::ZERO;
        let mut unloaded = 0;
        let mut dead = Vec::new();
        for k in stale {
            let mut overlapping = Vec::new();
            let covered = self.coverage(k, &mut overlapping);
            if covered {
                let t = Instant::now();
                if let Some(list) = self.loaded.remove(&k) {
                    for c in &list {
                        sink.free(c);
                    }
                }
                if k.lod == 0 {
                    store.take_column(k.x, k.z, (self.shared.gen.height_vox + 31) / 32, &mut dead);
                }
                unload_t += t.elapsed();
                unloaded += 1;
            } else {
                self.hidden.extend(overlapping);
            }
        }
        free_later(dead);
        self.stats.unload_ms = unload_t.as_secs_f32() * 1000.0;
        self.stats.unloaded = unloaded;
        let mut s = StreamStats {
            queued: self.stats.queued,
            busy: self.stats.busy,
            ..self.stats.clone()
        };
        s.columns = 0;
        s.chunks = 0;
        s.quads = 0;
        s.per_lod = [0; 8];
        for (k, list) in &self.loaded {
            if self.is_drawn(k) {
                s.columns += 1;
                s.chunks += list.len();
                s.quads += list.iter().map(|c| c.quads() as u64).sum::<u64>();
                s.per_lod[k.lod as usize] += 1;
            }
        }
        self.stats = s;
    }

    /// Whether every wanted column overlapping `k` has loaded; collects those columns.
    fn coverage(&self, k: ColKey, out: &mut Vec<ColKey>) -> bool {
        let mut a = k;
        while a.lod < self.max_lod {
            a = a.parent();
            if self.desired.contains(&a) {
                out.push(a);
                return self.loaded.contains_key(&a);
            }
        }
        self.covered_below(k, out)
    }

    fn covered_below(&self, k: ColKey, out: &mut Vec<ColKey>) -> bool {
        if self.desired.contains(&k) {
            out.push(k);
            return self.loaded.contains_key(&k);
        }
        if k.lod == 0 {
            return true;
        }
        let mut all = true;
        for c in k.children() {
            all &= self.covered_below(c, out);
        }
        all
    }

    /// Apply voxel edits (base-level voxel coordinates) and remesh what they touch.
    pub fn edit(&mut self, store: &mut Store, changes: &[([i32; 3], u8)]) -> usize {
        let mut dirty = HashSet::default();
        let mut applied = 0;
        {
            let mut e = self.shared.edits.write().unwrap();
            for &([x, y, z], m) in changes {
                if store.get(x, y, z) == Some(m) || !store.set(x, y, z, m) {
                    continue;
                }
                applied += 1;
                let (cx, cy, cz) = (x >> 5, y >> 5, z >> 5);
                let i = ((y & 31) * 1024 + (z & 31) * 32 + (x & 31)) as u16;
                e.0.entry((cx, cy, cz)).or_default().insert(i, m);
                // A neighbour's padded copy holds our voxels within M across
                // and MY vertically of the shared face, so an edit that close
                // changes its mesh too.
                let r = |l: i32, pad: usize| -> &'static [i32] {
                    if l < pad as i32 {
                        &[0, -1]
                    } else if l >= 32 - pad as i32 {
                        &[0, 1]
                    } else {
                        &[0]
                    }
                };
                for &dx in r(x & 31, M) {
                    for &dy in r(y & 31, MY) {
                        for &dz in r(z & 31, M) {
                            dirty.insert((cx + dx, cy + dy, cz + dz));
                        }
                    }
                }
            }
            if applied > 0 {
                e.1 += 1;
            }
        }
        for c in dirty {
            if self.loaded.contains_key(&ColKey { lod: 0, x: c.0, z: c.2 }) && c.1 >= 0 {
                self.remesh_chunk(c, store);
            }
        }
        if applied > 0 {
            self.dirty_edits = true;
        }
        applied
    }

    /// Mesh again every loaded full-detail chunk that touches the box
    /// `lo`..`hi`, metres: for a change that is not a voxel edit, such as
    /// grass being kept out from under a platform.
    pub fn remesh_box(&mut self, store: &Store, lo: Vec3, hi: Vec3) {
        let c = |v: f32| (v * 4.0).floor() as i32 >> 5;
        for cx in c(lo.x)..=c(hi.x) {
            for cz in c(lo.z)..=c(hi.z) {
                if !self.loaded.contains_key(&ColKey { lod: 0, x: cx, z: cz }) {
                    continue;
                }
                for cy in c(lo.y).max(0)..=c(hi.y) {
                    self.remesh_chunk((cx, cy, cz), store);
                }
            }
        }
    }

    fn remesh_chunk(&mut self, c: (i32, i32, i32), store: &Store) {
        let v = self.chunk_versions.entry(c).or_insert(0);
        *v += 1;
        let version = *v;
        let padded = store.padded(c.0, c.1, c.2);
        let mut q = self.shared.queue.lock().unwrap();
        // Remeshes go to the end, which is popped first.
        q.jobs.push((-1.0, Job::Remesh { chunk: c, padded, version }));
        self.shared.cv.notify_one();
    }

    /// Throw away every voxel edit and build the loaded world back the way
    /// the generator draws it, nearest first (the same machinery a turn of
    /// the season uses, `set_turn`). A column is regenerated from scratch
    /// and the edit map is reapplied to it, so emptying the map and asking
    /// for a regen is all it takes; the store is refilled by the same job,
    /// which is what puts the collision back as well as the mesh.
    ///
    /// Returns how many edited voxels were undone.
    pub fn clear_edits(&mut self, p: Vec3) -> usize {
        let undone = {
            let mut e = self.shared.edits.write().unwrap();
            let n: usize = e.0.values().map(|l| l.len()).sum();
            if n == 0 {
                return 0;
            }
            e.0.clear();
            e.1 += 1;
            n
        };
        let mut keys: Vec<ColKey> = self.loaded.keys().copied().collect();
        keys.sort_by(|a, b| b.dist_m(p.x, p.z).total_cmp(&a.dist_m(p.x, p.z)));
        self.queue_regen(&keys);
        self.dirty_edits = true;
        undone
    }

    pub fn edits_snapshot(&self) -> EditMap {
        self.shared.edits.read().unwrap().0.clone()
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        {
            let mut q = self.shared.queue.lock().unwrap();
            q.shutdown = true;
            q.jobs.clear();
        }
        self.shared.cv.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Workers run below normal priority so the main thread is never preempted by
/// them: with every core busy, an equal-priority worker can hold a core for a
/// whole scheduler quantum (about 15 ms on Windows), which shows up as a hitch.
fn lower_priority() {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

fn worker(s: Arc<Shared>, tx: Sender<JobResult>) {
    lower_priority();
    let mut scratch = GenScratch::default();
    let mut mesh = Mesh::default();
    // A mesh handed to the main thread is one exact-size allocation.
    let take = |m: &Mesh| Mesh {
        quads: m.quads.clone(),
        sprites: m.sprites.clone(),
        ..*m
    };
    loop {
        let job = {
            let mut q = s.queue.lock().unwrap();
            loop {
                if q.shutdown {
                    return;
                }
                if let Some((_, j)) = q.jobs.pop() {
                    q.busy += 1;
                    break j;
                }
                q = s.cv.wait(q).unwrap();
            }
        };
        let smooth = s.smooth.load(std::sync::atomic::Ordering::Relaxed);
        let result = match job {
            Job::Column(key, _) => {
                let t = Instant::now();
                // Copy just the edits near this column so the lock is held briefly.
                let (local, epoch) = if key.lod == 0 {
                    let e = s.edits.read().unwrap();
                    let mut local = EditMap::default();
                    for (k, v) in e.0.iter() {
                        if (k.0 - key.x).abs() <= 1 && (k.2 - key.z).abs() <= 1 {
                            local.insert(*k, v.clone());
                        }
                    }
                    (local, e.1)
                } else {
                    (EditMap::default(), 0)
                };
                let mut chunks = Vec::with_capacity(8);
                let edits = if local.is_empty() { None } else { Some(&local) };
                // Thin leaning stems are drawn along their own lines (mesh/lean.rs).
                let tube_trees = s.gen.tube_trees(key);
                let (solid_below, count) = s.gen.gen_column_with(key, edits, &mut scratch, |cy, padded, colh| {
                    let threads = s.gen.chunk_threads(&tube_trees, key, cy);
                    // Leaf blocks on the two finest levels (mesh.rs module notes).
                    mesh_chunk_t(padded, smooth, Some(colh), key.lod <= 1, &threads, &mut mesh);
                    // Every level scatters: the coarse ones carry only the far
                    // cover (sprites.rs), which is what keeps the distance
                    // from being bare ground. Its cells and its cards are in
                    // the column's own voxels, so both double with each level
                    // and the count per ring stays about the same.
                    (mesh.sprite_fill, mesh.sprite_grass, mesh.sprite_patch, mesh.sprite_stalk) = sprites::scatter(
                        padded,
                        smooth,
                        Some(colh),
                        [key.x * 32, cy * 32, key.z * 32],
                        VOXEL * (1u32 << key.lod) as f32,
                        key.lod > 0,
                        &|x, z| s.gen.water_m(x, z),
                        &mut mesh.sprites,
                    );
                    let v = if key.lod == 0 { Some(Voxels::from_padded(padded)) } else { None };
                    chunks.push((cy, take(&mesh), v));
                });
                JobResult::Column {
                    key,
                    chunks,
                    solid_below,
                    count,
                    micros: t.elapsed().as_micros() as u64,
                    epoch,
                    smooth,
                }
            }
            Job::Remesh { chunk, padded, version } => {
                // True ground heights of the padded columns, in this chunk's voxels.
                let mut colh = vec![0f32; PAD * PAD];
                for pz in 0..PAD {
                    for px in 0..PAD {
                        let (x, z) = (chunk.0 * 32 - M as i32 + px as i32, chunk.2 * 32 - M as i32 + pz as i32);
                        colh[pz * PAD + px] = s.gen.ground_vox(x, z) - (chunk.1 * 32 - MY as i32) as f32;
                    }
                }
                let key = crate::world::gen::ColKey { lod: 0, x: chunk.0, z: chunk.2 };
                let threads = s.gen.chunk_threads(&s.gen.tube_trees(key), key, chunk.1);
                mesh_chunk_t(&padded, smooth, Some(&colh), true, &threads, &mut mesh);
                (mesh.sprite_fill, mesh.sprite_grass, mesh.sprite_patch, mesh.sprite_stalk) = sprites::scatter(
                    &padded,
                    smooth,
                    Some(&colh),
                    [chunk.0 * 32, chunk.1 * 32, chunk.2 * 32],
                    VOXEL,
                    false,
                    &|x, z| s.gen.water_m(x, z),
                    &mut mesh.sprites,
                );
                JobResult::Remesh {
                    chunk,
                    mesh: take(&mesh),
                    version,
                    smooth,
                }
            }
        };
        s.queue.lock().unwrap().busy -= 1;
        if tx.send(result).is_err() {
            return;
        }
    }
}
