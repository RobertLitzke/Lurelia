//! What a course adds to a plain race (tools/course/FORMAT.md): props a kart
//! smashes through, and stretches of road where the wind blows, gravity is
//! light, or a flood rises lap by lap.
//!
//! The props are stamped in a passable copy of their material
//! (`race_track.rs`), so no kart ever hits one: as a kart reaches a prop the
//! race takes its voxels away, throws debris and takes a little of the
//! kart's speed. A new race wipes the world back to how it was built
//! (`Race::place_all` asks for it), so every race starts with them standing.

use glam::{Vec2, Vec3};

use crate::fx::Cue;
use crate::vehicle::Vehicle;
use crate::world::fields::{Smash, Zone};
use crate::world::store::{Cell, Store};

/// Voxels a metre.
const V: f32 = 1.0 / crate::world::gen::VOXEL;
/// How far round a prop a kart's middle breaks it: half a kart's width and
/// a little, metres.
const REACH: f32 = 1.0;
/// Seconds ahead along its path a kart is looked for, so the prop is gone
/// before the kart is in it.
const AHEAD: f32 = 0.12;
/// A gust is shown coming this many seconds before it blows.
pub const GUST_WARN: f32 = 1.2;

#[derive(Clone)]
pub struct Course {
    zones: Vec<Zone>,
    smash: Vec<Smash>,
    /// Each prop's box in metres, and whether it is gone this race.
    boxes: Vec<(Vec3, Vec3)>,
    broken: Vec<bool>,
    /// Voxel changes and bursts waiting for the next step (`Race::step`
    /// hands them to the game).
    edits: Vec<([i32; 3], u8)>,
    cues: Vec<Cue>,
    /// The lap each flood stands at, and the water it is made of.
    flood_lap: Vec<Option<usize>>,
    /// Animals to put down (herd zone, species, x, z, seed), for the next step.
    spawns: Vec<(usize, String, f32, f32, u32)>,
    /// The animals each herd zone has put down (`Race::step` says which),
    /// and those to take away at the next step: a zone's last herd when it
    /// puts down the next lap's, and every herd when the race starts over.
    herd: Vec<(usize, hecs::Entity)>,
    gone: Vec<hecs::Entity>,
    /// Water cells each flood laid last, for the status line.
    flood_cells: Vec<usize>,
    water: Option<u8>,
    /// Seconds since the race began, for the gusts.
    clock: f32,
    /// Which karts (0 the player) are gliding: left the ground on a glide
    /// stretch and not down yet.
    gliding: Vec<bool>,
    /// Seconds to the next puff of thermal dust and gust streak.
    puff: f32,
    /// The wall's height beside the road a metre apart round the lap
    /// (empty: no walls), and the lap's length.
    walls: Vec<f32>,
    length: f32,
}

impl Course {
    pub fn new(zones: Vec<Zone>, smash: Vec<Smash>, water: Option<u8>, walls: Vec<f32>, length: f32) -> Course {
        let boxes = smash
            .iter()
            .map(|s| {
                let lo = s.boxes.iter().map(|b| Vec3::new(b[0] as f32, b[1] as f32, b[2] as f32)).fold(Vec3::splat(f32::MAX), Vec3::min);
                let hi = s.boxes.iter().map(|b| Vec3::new(b[3] as f32, b[4] as f32, b[5] as f32)).fold(Vec3::splat(f32::MIN), Vec3::max);
                (lo / V, hi / V)
            })
            .collect();
        let n = smash.len();
        let floods = zones.len();
        Course { zones, smash, boxes, broken: vec![false; n], edits: Vec::new(), cues: Vec::new(), flood_lap: vec![None; floods], flood_cells: vec![0; floods], spawns: Vec::new(), herd: Vec::new(), gone: Vec::new(), gliding: Vec::new(), puff: 0.0, water, clock: 0.0, walls, length }
    }

    /// The same course as a new race finds it: every prop standing, no
    /// flood laid, and the herds the last race put down on their way out.
    pub fn fresh(&self) -> Course {
        let mut c = Course::new(self.zones.clone(), self.smash.clone(), self.water, self.walls.clone(), self.length);
        c.gone = self.gone.iter().copied().chain(self.herd.iter().map(|h| h.1)).collect();
        c
    }

    /// The wall's height beside the road `s` metres round (0: none).
    pub fn wall_at(&self, s: f32) -> f32 {
        if self.walls.is_empty() || self.length <= 0.0 {
            return 0.0;
        }
        let n = self.walls.len();
        self.walls[((s.rem_euclid(self.length) / self.length * n as f32) as usize).min(n - 1)]
    }

    /// Whether the course has anything for a race to do.
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty() && self.smash.is_empty()
    }

    /// The zones of `kind` that lap distance `s` is on.
    fn at<'a>(&'a self, kind: &'a str, s: f32) -> impl Iterator<Item = &'a Zone> + 'a {
        self.zones.iter().filter(move |z| z.kind == kind && z.on(s))
    }

    /// How hard a wind zone blows now, 0 to 1: steady with no `gust`; with
    /// one, a gust `gust` seconds long every three times that, rising and
    /// falling over a fifth of it. `ahead` seconds from now.
    pub fn blow(&self, z: &Zone, ahead: f32) -> f32 {
        if z.gust <= 0.0 {
            return 1.0;
        }
        let period = z.gust * 3.0;
        // Each zone on its own phase, so two windy stretches don't gust together.
        let t = (self.clock + ahead + z.from * 0.37).rem_euclid(period);
        let edge = (z.gust * 0.2).max(0.2);
        if t < z.gust {
            (t / edge).min(1.0).min((z.gust - t) / edge).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The wind on lap distance `s` now (`ahead` = 0) or soon: the push, m/s²,
    /// across the ground.
    pub fn wind(&self, s: f32, ahead: f32) -> Vec3 {
        self.at("wind", s)
            .map(|z| {
                let b = z.bearing.to_radians();
                // Bearing clockwise from north; x east, z south.
                Vec3::new(b.sin(), 0.0, -b.cos()) * z.value * self.blow(z, ahead)
            })
            .fold(Vec3::ZERO, |a, b| a + b)
    }

    /// Whether kart `k` (0 the player) is gliding.
    pub fn gliding(&self, k: usize) -> bool {
        self.gliding.get(k).copied().unwrap_or(false)
    }

    /// One step for kart `k` (0 the player) at lap distance `s`: the wind
    /// on it, the gravity it feels, and a glide off a glide stretch: a kart
    /// that leaves the ground there floats down at a fraction of its
    /// gravity, its path swinging round to where its nose points and turned
    /// by its bank, as a glider's is, lifted where a thermal rises.
    pub fn drive(&mut self, k: usize, car: &mut Vehicle, s: f32, dt: f32) {
        if self.zones.is_empty() {
            return;
        }
        if self.gliding.len() <= k {
            self.gliding.resize(k + 1, false);
        }
        if car.grounded > 0 {
            self.gliding[k] = false;
        } else if !self.gliding[k] && car.air_time < 0.3 && self.at("glide", s).next().is_some() {
            self.gliding[k] = true;
        }
        if self.gliding[k] {
            let g = self.zones.iter().find(|z| z.kind == "glide").map_or(0.3, |z| z.value.max(0.05));
            let flat = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
            let fwd = car.forward();
            let nose = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
            let up = car.up();
            let bank = up.dot(Vec3::new(-nose.z, 0.0, nose.x));
            let turn = (-bank * 1.6 * dt).clamp(-0.05, 0.05);
            let (st, ct) = turn.sin_cos();
            let mut v = Vec3::new(flat.x * ct - flat.z * st, 0.0, flat.x * st + flat.z * ct);
            if nose != Vec3::ZERO && v.length() > 1.0 {
                v = v.lerp(nose * v.length(), (dt * 1.2).min(1.0));
            }
            car.b.vel = Vec3::new(v.x, car.b.vel.y, v.z);
            car.gravity_k = g;
            if car.b.vel.y < -4.0 {
                car.b.vel.y += (-4.0 - car.b.vel.y) * (dt * 1.5).min(1.0);
            }
            for z in self.zones.iter().filter(|z| z.kind == "thermal" && z.at.len() >= 3) {
                let d = Vec2::new(car.pos().x - z.at[0], car.pos().z - z.at[1]).length();
                if d < z.at[2] {
                    car.b.vel.y += z.value * (1.0 - d / z.at[2]).max(0.3) * dt;
                }
            }
        }
        let w = self.wind(s, 0.0);
        if w != Vec3::ZERO {
            // Less on a kart on the ground, whose tyres hold it.
            let k = if car.grounded > 0 { 0.6 } else { 1.0 };
            car.b.vel += w * k * dt;
        }
        if !self.gliding[k] {
            car.gravity_k = self.at("gravity", s).map(|z| z.value).next().unwrap_or(1.0);
        }
    }

    /// Break any prop kart `car` is about to drive into: its
    /// voxels go, debris flies, and the kart keeps `slow` of its speed.
    pub fn smash(&mut self, car: &mut Vehicle, store: &Store) {
        if self.smash.is_empty() {
            return;
        }
        let p = car.pos();
        let q = p + car.b.vel * AHEAD;
        for k in 0..self.smash.len() {
            if self.broken[k] {
                continue;
            }
            let (lo, hi) = self.boxes[k];
            let (lo, hi) = (lo - Vec3::splat(REACH), hi + Vec3::splat(REACH));
            let inside = |x: Vec3| x.cmpge(lo).all() && x.cmple(hi).all();
            if !inside(p) && !inside(q) {
                continue;
            }
            // Its voxels can only be taken away where they are loaded.
            if !self.smash[k].boxes.iter().all(|b| store.loaded(b[0], b[1], b[2])) {
                continue;
            }
            self.broken[k] = true;
            car.b.vel *= self.smash[k].slow;
            let mid = (self.boxes[k].0 + self.boxes[k].1) * 0.5;
            for b in &self.smash[k].boxes {
                for x in b[0]..b[3] {
                    for y in b[1]..b[4] {
                        for z in b[2]..b[5] {
                            self.edits.push(([x, y, z], crate::world::material::AIR));
                        }
                    }
                }
            }
            let dir = (car.b.vel.normalize_or_zero() + Vec3::Y).normalize_or(Vec3::Y);
            let size = (self.boxes[k].1 - self.boxes[k].0).length();
            crate::fx::cue(&mut self.cues, Cue { name: "burst_debris", at: mid, radius: size * 0.3, dir, amount: (size * 0.6).clamp(0.6, 3.0) });
            crate::fx::cue(&mut self.cues, Cue { name: "impact_chips", at: mid, radius: size * 0.2, dir, amount: 1.0 });
            crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Clank, mid, 0.5));
        }
    }

    /// The floods, as the race leader starts lap `lap` (0 the first): each
    /// whose depth has changed is laid again.
    pub fn flood(&mut self, lap: usize, store: &Store) {
        let Some(water) = self.water else { return };
        for (i, z) in self.zones.iter().enumerate() {
            if z.kind != "flood" || z.laps.is_empty() {
                continue;
            }
            let k = lap.min(z.laps.len() - 1);
            if self.flood_lap[i] == Some(k) {
                continue;
            }
            // An edit to ground not loaded is lost (`Streamer::edit`), so a
            // flood waits until all of its ground is in, at each end and
            // the middle.
            let n = z.area.len();
            if n == 0 || ![0, n / 2, n - 1].iter().all(|&j| {
                let b = z.area[j];
                store.loaded(b[0], b[1], b[2]) && store.loaded(b[3] - 1, b[4] - 1, b[5] - 1)
            }) {
                continue;
            }
            self.flood_lap[i] = Some(k);
            let mut cells = 0;
            let top = ((z.level + z.laps[k]) * V).round() as i32;
            for b in &z.area {
                for x in b[0]..b[3] {
                    for zz in b[2]..b[5] {
                        for y in b[1]..b[4] {
                            // Only open air and the flood's own water: never
                            // the ground or the road.
                            if !matches!(store.cell(x, y, zz), Cell::Open | Cell::Water) {
                                continue;
                            }
                            let m = if y < top { water } else { crate::world::material::AIR };
                            cells += usize::from(y < top);
                            self.edits.push(([x, y, zz], m));
                        }
                    }
                }
            }
            self.flood_cells[i] = cells;
        }
    }

    /// What the course has done so far, for the headless status line:
    /// props smashed, and the lap each flood stands at.
    pub fn status(&self) -> String {
        let mut s = String::new();
        if !self.smash.is_empty() {
            s += &format!("  smashed {}/{}", self.broken.iter().filter(|&&b| b).count(), self.smash.len());
        }
        let floods: Vec<String> = self.flood_lap.iter().zip(&self.zones).enumerate().filter(|(_, (_, z))| z.kind == "flood").map(|(i, (l, _))| l.map_or("-".into(), |l| format!("{}:{}", l + 1, self.flood_cells[i]))).collect();
        if !floods.is_empty() {
            s += &format!("  floods {}", floods.join("/"));
        }
        s
    }

    /// The herds, as the player comes within reach of each on the leader's
    /// lap `lap`: put down along their stretch once a lap, on the road or
    /// out on the verges as the lap says (`Zone::species`).
    pub fn herds(&mut self, lap: usize, player_s: f32, track: &crate::vehicle::bot::Track) {
        for (i, z) in self.zones.iter().enumerate() {
            if z.kind != "herd" || z.laps.is_empty() || z.species.is_empty() {
                continue;
            }
            let ahead = (z.from - player_s).rem_euclid(self.length.max(1.0));
            if ahead > 220.0 || self.flood_lap[i] == Some(lap) {
                continue;
            }
            self.flood_lap[i] = Some(lap);
            // The herd the zone put down on an earlier lap makes way for this
            // lap's, or a race of five laps ends with five herds on it.
            self.herd.retain(|&(zone, e)| {
                let keep = zone != i;
                if !keep {
                    self.gone.push(e);
                }
                keep
            });
            let off = z.laps[lap.min(z.laps.len() - 1)];
            if off < 0.0 {
                continue;
            }
            let span = (z.to - z.from).rem_euclid(self.length.max(1.0));
            let n = z.value.max(1.0) as usize;
            for k in 0..n {
                let seed = (i * 977 + lap * 131 + k * 17) as u32;
                let jitter = |m: u32| ((crate::world::noise::hash32(seed.wrapping_mul(m) | 1) % 1000) as f32 / 1000.0) - 0.5;
                let s = z.from + span * (k as f32 + 0.5 + 0.6 * jitter(3)) / n as f32;
                let (p, d) = track.at(s);
                let right = glam::Vec2::new(-d.y, d.x);
                let across = if off == 0.0 {
                    jitter(5) * (track.width_at(s) - 2.0)
                } else {
                    (if k % 2 == 0 { 1.0 } else { -1.0 }) * (off + 4.0 * jitter(7))
                };
                let q = p + right * across;
                self.spawns.push((i, z.species.clone(), q.x, q.y, seed));
            }
        }
    }

    /// The animals waiting to be put down: (herd zone, species, x, z, seed).
    pub fn take_spawns(&mut self) -> Vec<(usize, String, f32, f32, u32)> {
        std::mem::take(&mut self.spawns)
    }

    /// What herd zone `zone` put down, to be taken away when it is replaced.
    pub fn placed(&mut self, zone: usize, ents: impl IntoIterator<Item = hecs::Entity>) {
        self.herd.extend(ents.into_iter().map(|e| (zone, e)));
    }

    /// The animals to take off the course now.
    pub fn take_gone(&mut self) -> Vec<hecs::Entity> {
        std::mem::take(&mut self.gone)
    }

    /// Where a kart that drowned at lap distance `s` is put back: on the
    /// far bank of the flood it went into, a few metres past it, so a missed
    /// jump costs the time and not the same jump again. Only a flood with
    /// water down this lap: on a dry lap (Thornpan's first) a kart stopped
    /// there is stuck on plain road and is put back where it is.
    pub fn past_flood(&self, s: f32) -> Option<f32> {
        self.zones
            .iter()
            .enumerate()
            .find(|(i, z)| z.kind == "flood" && z.on(s) && self.flood_cells.get(*i).is_some_and(|&c| c > 0))
            .map(|(_, z)| (z.to + 4.0).rem_euclid(self.length.max(1.0)))
    }

    /// The race clock moves on, and the course shows what it is about to
    /// do near the player at `p`: dust rising in the thermals, and dust
    /// streaming across a windy stretch a beat before each gust.
    pub fn tick(&mut self, dt: f32, p: Option<Vec3>, track: &crate::vehicle::bot::Track, player_s: f32) {
        self.clock += dt;
        self.puff -= dt;
        let Some(p) = p else { return };
        if self.puff > 0.0 || self.zones.is_empty() {
            return;
        }
        self.puff = 0.12;
        let seed = (self.clock * 1000.0) as u32;
        let r = |m: u32| ((crate::world::noise::hash32(seed ^ m.wrapping_mul(0x9e37)) % 1000) as f32 / 1000.0) - 0.5;
        let len = self.length.max(1.0);
        let mut cues = Vec::new();
        for z in self.zones.iter().filter(|z| z.kind == "thermal" && z.at.len() >= 3) {
            if Vec2::new(z.at[0] - p.x, z.at[1] - p.z).length() > 250.0 {
                continue;
            }
            let base = z.at.get(3).copied().unwrap_or(p.y - 20.0);
            let q = Vec2::new(z.at[0] + r(1) * z.at[2], z.at[1] + r(2) * z.at[2]);
            cues.push(Cue { name: "seed_drift", at: Vec3::new(q.x, base + (r(3) + 0.5) * 25.0, q.y), radius: 3.0, dir: Vec3::Y * 6.0, amount: 1.0 });
            cues.push(Cue { name: "leaf_fall", at: Vec3::new(q.x, base + (r(4) + 0.5) * 25.0, q.y), radius: 3.0, dir: Vec3::Y * 4.0, amount: 0.6 });
        }
        for z in self.zones.iter().filter(|z| z.kind == "wind" && z.gust > 0.0) {
            // Coming and not yet at full strength: the streaks are the warning.
            let soon = self.blow(z, GUST_WARN);
            if soon <= 0.05 || self.blow(z, 0.0) > 0.9 {
                continue;
            }
            let b = z.bearing.to_radians();
            let dir = Vec3::new(b.sin(), 0.0, -b.cos());
            for m in 0..3 {
                let s = (player_s + 10.0 + (r(10 + m) + 0.5) * 80.0).rem_euclid(len);
                if !z.on(s) {
                    continue;
                }
                let (q, _) = track.at(s);
                let at = Vec3::new(q.x, p.y + 0.5, q.y) - dir * 14.0;
                cues.push(Cue { name: "wheel_dust", at, radius: 4.0, dir: dir * (12.0 * soon) + Vec3::Y, amount: 1.4 });
            }
        }
        for c in cues {
            crate::fx::cue(&mut self.cues, c);
        }
    }

    /// The voxel changes and bursts waiting, for the game to make.
    pub fn take(&mut self, edits: &mut Vec<([i32; 3], u8)>, cues: &mut Vec<Cue>) {
        edits.append(&mut self.edits);
        for c in self.cues.drain(..) {
            crate::fx::cue(cues, c);
        }
    }
}

/// The water's surface over `p` for a kart that isn't the player's: the
/// sea, or water voxels (a ford, a flood) under the kart.
pub fn water_level(store: &Store, sea: f32, p: Vec3) -> f32 {
    // Looked for from just under the kart up past its seat: a shallow ford
    // under it, or deep water it is sitting in.
    [-0.25f32, 0.0, 0.25, 0.5, 0.75, 1.0]
        .iter()
        .find_map(|&dy| crate::physics::water_voxel_top(store, p + Vec3::Y * dy))
        .map_or(sea, |t| t.max(sea))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::bot::Track;

    fn ring() -> Track {
        let pts = (0..200).map(|i| {
            let a = i as f32 / 200.0 * std::f32::consts::TAU;
            Vec2::new(100.0 * a.cos(), -100.0 * a.sin())
        });
        Track::new(pts.collect(), 10.0)
    }

    /// A kart stopped on a flood's stretch is carried past it only while the
    /// flood has water down.
    #[test]
    fn a_dry_flood_puts_nobody_past_it() {
        let zone = Zone { from: 100.0, to: 140.0, kind: "flood".into(), laps: vec![0.0, 0.6], ..Default::default() };
        let mut c = Course::new(vec![zone], Vec::new(), Some(1), Vec::new(), 600.0);
        assert_eq!(c.past_flood(120.0), None, "not laid yet");
        c.flood_cells[0] = 0;
        assert_eq!(c.past_flood(120.0), None, "laid dry");
        c.flood_cells[0] = 900;
        assert_eq!(c.past_flood(120.0), Some(144.0));
        assert_eq!(c.past_flood(200.0), None, "off its stretch");
    }

    /// Each lap's herd takes the place of the last lap's, and a race started
    /// over takes every herd the last one put down off the course.
    #[test]
    fn a_herd_is_put_down_once_a_lap_and_taken_away_after() {
        let track = ring();
        let zone = Zone { from: 100.0, to: 160.0, kind: "herd".into(), value: 3.0, laps: vec![0.0, 6.0, -1.0], species: "cow".into(), ..Default::default() };
        let mut c = Course::new(vec![zone], Vec::new(), None, Vec::new(), track.length);
        let mut w = hecs::World::new();
        let mut put = |c: &mut Course| {
            let s = c.take_spawns();
            assert!(s.iter().all(|x| x.0 == 0 && x.1 == "cow"));
            let e: Vec<hecs::Entity> = s.iter().map(|_| w.spawn(())).collect();
            c.placed(0, e.clone());
            e
        };
        c.herds(0, 0.0, &track);
        let first = put(&mut c);
        assert_eq!(first.len(), 3);
        // Later on the same lap nothing new comes and nothing goes.
        c.herds(0, 40.0, &track);
        assert!(c.take_spawns().is_empty() && c.take_gone().is_empty());
        c.herds(1, 0.0, &track);
        assert_eq!(c.take_gone(), first, "lap one's herd makes way for lap two's");
        let second = put(&mut c);
        assert_eq!(second.len(), 3);
        // A lap that puts none down still clears the last.
        c.herds(2, 0.0, &track);
        assert!(c.take_spawns().is_empty());
        assert_eq!(c.take_gone(), second);
        c.herds(0, 0.0, &track);
        let again = put(&mut c);
        let mut fresh = c.fresh();
        assert_eq!(fresh.take_gone(), again, "a restart takes the herd away");
        assert!(fresh.take_gone().is_empty());
    }
}
