//! Herd records (notes/ANIMAL_SIM_PLAN.md C1): every prey herd in the world,
//! kept whether or not anyone is near it.
//!
//! Before this, herds were made up round the player and dropped behind
//! them, so a herd watched at the water was gone when the player turned
//! back. Now the whole map is seeded with herds when the world is set,
//! in the numbers its biomes list, and the spawner leaves those species
//! alone (spawn.rs `world_creatures` skips them). A herd near the player is
//! drawn: its animals are in the world, tagged `Recorded` with the herd's
//! number, and steered as any herd is (herd.rs, roam.rs). Once the player
//! is far from it, the animals are written back into the record (each one's
//! species, age, sex, build and coat, where it stood in the herd) and taken
//! out. While it is not drawn the herd still has a trip (roam.rs `unseen`)
//! and its middle moves along it. Come back and the same animals are put
//! down round wherever the herd has got to.
//!
//! Which herds: walkers with a `group` block of two or more that hunt
//! nothing (deer, bison, horses, the grazing dinosaurs), and every walking
//! predator. A predator whose `group` block allows two or more (wolves,
//! hyenas, lions, raptors) is kept as a pack, one record for the lot; one
//! that goes alone (cougar, tiger, fox) is a record of one. roam.rs sends
//! them hunting the herds. Lone prey, burrowers and waders keep the old
//! spawning.
//!
//! Out of sight, a pack shadowing its quarry (roam.rs) tries for a kill
//! every few seconds (`encounters`, ANIMAL_SIM C3): the herd loses one
//! animal, the weakest it has, and runs; the pack is fed and lies up where
//! it killed; and the spot is kept as a `KillSite`, put down as a carcass
//! (then bones) if the player comes by before it is gone.
//!
//! Numbers hold over days (`grow`, ANIMAL_SIM C5). Each species' target is
//! what the biome lists seed (`target`). A herd not drawn has young at
//! `BIRTHS_PER_DAY` per animal, slowed as its species nears its target and
//! by poor grazing where it stands, and splits once it is half again its
//! group's largest. A pack has young only when it has eaten in the last day,
//! and loses one each `STARVE_H` without a meal. A species fallen under
//! `FLOOR` of its target has a herd walk in from the map edge nearest one
//! of its homes, out of the player's sight, at most once in `WALK_IN_H`.
//!
//! The roster is saved to `herds.json` in the world's save folder when the
//! player saves (F5, the pause menu) and when the world is closed (leaving
//! for the menu or another world, quitting), not on the half-minute autosave
//! (game.rs). The file names species by name and carries the world's survey
//! key: a world that has changed since is seeded afresh. Trips are not
//! kept; a loaded herd plans a new one.

use super::age::{Age, Stage};
use super::herd::Kin;
use super::locomotion::Mover;
use super::walk::Critter;
use super::{Coat, Creature, Look, Species};
use crate::models::Model;
use crate::physics::Body;
use crate::world::fields::World as Land;
use crate::world::noise::{hash32, unit, Rng};
use crate::world::store::Store;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The roster's file in a world's save folder.
pub const FILE: &str = "herds.json";
/// The file's layout; a file of another is seeded over.
const FILE_VERSION: u32 = 2;

/// The bit every recorded herd's number carries, so roam.rs can tell a
/// recorded herd from one the spawner made (whose numbers are odd hashes).
const RECORD_BIT: u32 = 0x8000_0000;

/// Whether herd number `g` is a record.
pub fn is_record(g: u32) -> bool {
    g & RECORD_BIT != 0 && g & 1 == 0
}

/// On every drawn member of a recorded herd: which record it belongs to.
pub struct Recorded(pub u32);

/// The seeding grid, metres a cell.
const SEED_CELL_M: f32 = 64.0;
/// How often herds that are not drawn move on, seconds.
const UNSEEN_S: f32 = 3.0;
/// The most animals of recorded herds drawn at once. Herds are drawn out to
/// 250 m in view (350 m for big ones, spawn.rs `HERD_SEEN_M`), which on
/// Kitehold's plain is about 170 animals; past this the nearest are drawn
/// and the furthest written back. Each drawn animal is stepped and drawn in
/// full: 170 against 140 cost under 0.1 ms of tick and render (2026-11-24).
const MAX_DRAWN: u32 = 220;
/// `MAX_DRAWN`, or what `VG_HERD_CAP` says (a dev hook for measuring).
fn max_drawn() -> u32 {
    static C: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *C.get_or_init(|| std::env::var("VG_HERD_CAP").ok().and_then(|v| v.parse().ok()).unwrap_or(MAX_DRAWN))
}
/// The most herds drawn or written back in one go (each is a handful of
/// entities), so walking into a valley full of herds does not cost a frame.
const PER_CALL: usize = 3;

/// One animal of a herd as it was written back.
#[derive(Clone, Debug)]
pub struct Member {
    /// Where it stood from the herd's middle, metres.
    pub off: Vec3,
    pub yaw: f32,
    pub heading: f32,
    pub species: usize,
    pub spawned_for: usize,
    pub model: usize,
    pub half_w: f32,
    pub height: f32,
    pub stage: Stage,
    pub seed: u32,
    pub side: f32,
    pub morph: Option<crate::variety::Morph>,
    pub coat: Option<Coat>,
    pub sex: Option<super::sex::Sex>,
}

/// One herd.
#[derive(Clone, Debug)]
pub struct Herd {
    pub id: u32,
    /// The species the biomes list (its quota species).
    pub species: usize,
    /// How many it has; set by its members once it has been drawn.
    pub size: u32,
    /// Its middle, on the ground plane.
    pub centre: Vec3,
    /// Its animals are in the world now.
    pub drawn: bool,
    /// Its animals as last written back; empty until it is first drawn.
    pub members: Vec<Member>,
    /// Its home ground: where it was seeded, or the lost herd's it came to
    /// replace.
    pub home: Vec3,
    /// Wariness read from a save, handed to roam.rs on the next update.
    pub wary: f32,
    /// A pack's game hours since it last ate, and when that meal was (the
    /// game clock, roam.rs `ate_at`).
    pub hungry_h: f32,
    pub last_meal: f32,
}

#[derive(Default)]
pub struct Roster {
    pub herds: Vec<Herd>,
    /// The world the herds were seeded for (its size), so a new world seeds
    /// afresh.
    seeded_for: Option<(String, u32)>,
    since: f32,
    /// Herds drawn and written back since the world was set, for the stats.
    pub drawn: u32,
    pub stored: u32,
    /// Herds gone: every member dead or carried off.
    pub lost: u32,
    /// Kills made out of sight, still lying where they were made.
    pub kills: Vec<KillSite>,
    /// Kills made out of sight since the world was set, for the stats.
    pub killed: u32,
    /// Each species' target: the animals the biome lists seed (`seed`).
    pub target: Vec<u32>,
    /// Where each species' herds were seeded: where a walk-in heads from.
    homes: Vec<Vec<Vec3>>,
    /// When each species last had a herd walk in, game hours since the world
    /// was set.
    walked_at: Vec<f32>,
    /// Game hours since the world was set, and the hour last seen.
    pub hours: f32,
    last_hour: Option<f32>,
    /// The next record number to give out.
    next_id: u32,
    /// Herds lost and not yet replaced: (species, home, game hour due).
    pending: Vec<(usize, Vec3, f32)>,
    /// Herds come in from the edge to replace one lost.
    pub replaced: u32,
    /// Since the world was set: young born, hunters starved, herds split,
    /// herds walked in.
    pub born: u32,
    pub starved: u32,
    pub split: u32,
    pub walked_in: u32,
}

/// A kill made out of sight: what was killed, by what, where and when.
/// Put down as a carcass while the player is near (`ent`), until its bones
/// are gone (age.rs `CARCASS_S` + `BONES_LAST_S` after `when`).
#[derive(Clone, Debug)]
pub struct KillSite {
    pub at: Vec3,
    pub victim: Member,
    /// The hunter's species.
    pub by: usize,
    pub when: f32,
    pub ent: Option<hecs::Entity>,
}

/// A pack this near its quarry's middle, metres, may try for a kill.
const STRIKE_M: f32 = 32.0;
/// The chance a pack of one makes a kill in each `UNSEEN_S` it is in
/// reach; each more hunter in the pack adds half as much again, up to four.
/// About a minute and a half of shadowing for a pride of two.
const STRIKE_CHANCE: f32 = 0.025;
/// A kill site is put down as a carcass within this of the player, metres.
const KILL_SEEN_M: f32 = 140.0;

/// Whether species `sp` lives in recorded herds (or packs).
pub fn recorded(sp: &Species) -> bool {
    let walker = matches!(sp.verb().mover, Mover::Walk { wades: false, burrows: false, .. });
    walker && !sp.is_water_form && (packs(sp) || !sp.hunts.is_empty())
}

/// What a record of species `sp` is called in the logs and the debug view:
/// a herd, a pack, or a lone hunter.
pub fn kind(sp: &Species) -> &'static str {
    match (sp.hunts.is_empty(), packs(sp)) {
        (true, _) => "herd",
        (false, true) => "pack",
        (false, false) => "lone",
    }
}

/// Whether species `sp` goes about in groups of two or more: a herd for
/// prey, a pack for a predator.
pub fn packs(sp: &Species) -> bool {
    sp.beh.group.as_ref().is_some_and(|g| g.size[1] >= 2)
}

impl Roster {
    /// The world `land` is being played: seed its herds, once.
    pub fn seed(&mut self, land: &Land, species: &[Species], models: &[Model]) {
        let key = (land.def.name.clone(), land.f.size as u32);
        if self.seeded_for.as_ref() == Some(&key) {
            return;
        }
        self.seeded_for = Some(key);
        self.herds = seed(land, species, models);
        self.set_targets(species, &self.herds.clone());
        self.drawn = 0;
        self.stored = 0;
        self.lost = 0;
        self.kills.clear();
        self.killed = 0;
        self.born = 0;
        self.starved = 0;
        self.split = 0;
        self.walked_in = 0;
        self.replaced = 0;
        self.pending.clear();
        self.hours = 0.0;
        self.last_hour = None;
    }

    /// Each species' target and homes, from a seeding of the map.
    fn set_targets(&mut self, species: &[Species], seeded: &[Herd]) {
        self.target = vec![0; species.len()];
        self.homes = vec![Vec::new(); species.len()];
        self.walked_at = vec![f32::NEG_INFINITY; species.len()];
        for h in seeded {
            self.target[h.species] += h.size;
            self.homes[h.species].push(h.centre);
        }
        self.next_id = self.herds.iter().map(|h| h.id).max().unwrap_or(RECORD_BIT) + 2;
    }

    /// Every half second (Entities::populate): write back the drawn herds
    /// the player has left, draw the ones they have come to, and every few
    /// seconds move the rest along their trips.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut hecs::World,
        species: &[Species],
        models: &[Model],
        land: &Land,
        store: &Store,
        plants: &super::plants::Flora,
        rng: &mut Rng,
        roam: &mut super::roam::Roam,
        live: &crate::world::live::Live,
        player: Vec3,
        facing: Vec3,
        hour: f32,
        now: f32,
        dt: f32,
    ) {
        // Wariness read from a save goes to the herds' minds.
        for h in self.herds.iter_mut().filter(|h| h.wary != 0.0) {
            roam.set_wary(h.id, h.wary);
            h.wary = 0.0;
        }
        // The drawn herds as they stand now.
        let mut at: crate::util::FxHashMap<u32, (Vec3, u32)> = Default::default();
        for (r, look, _) in world.query::<(&Recorded, &Look, &Critter)>().iter() {
            let e = at.entry(r.0).or_insert((Vec3::ZERO, 0));
            e.0 += look.pos;
            e.1 += 1;
        }
        let flat = |p: Vec3| Vec3::new(p.x - player.x, 0.0, p.z - player.z).length();
        let mut done = 0;
        let mut gone: Vec<u32> = Vec::new();
        for h in self.herds.iter_mut().filter(|h| h.drawn) {
            let Some(&(sum, n)) = at.get(&h.id) else {
                // Every member dead or taken: the herd is no more.
                gone.push(h.id);
                continue;
            };
            h.centre = Vec3::new(sum.x / n as f32, 0.0, sum.z / n as f32);
            // One of it gone while drawn (shot, taken): the herd remembers.
            if n < h.size {
                roam.trouble(h.id, h.centre, super::roam::WARY_LOSS, now);
            }
            h.size = n;
            if done < PER_CALL && leave(&species[h.species], models, h.centre, player, facing) {
                store_herd(world, h);
                done += 1;
                self.stored += 1;
            }
        }
        // Over the cap: the furthest drawn herds out of the near reach go
        // back into their records.
        let mut total: u32 = self.herds.iter().filter(|h| h.drawn).map(|h| h.size).sum();
        if total > max_drawn() {
            let mut far: Vec<usize> = (0..self.herds.len())
                .filter(|&i| self.herds[i].drawn && flat(self.herds[i].centre) > super::spawn::reach(&species[self.herds[i].species], models).0 * 0.9)
                .collect();
            far.sort_by(|&a, &b| flat(self.herds[b].centre).total_cmp(&flat(self.herds[a].centre)));
            for i in far {
                if total <= max_drawn() || done >= PER_CALL {
                    break;
                }
                total -= self.herds[i].size;
                store_herd(world, &mut self.herds[i]);
                done += 1;
                self.stored += 1;
            }
        }
        for g in gone {
            self.lose(g, roam);
        }
        // Draw what the player has come to, nearest first, while there is
        // room under the cap.
        let mut near: Vec<usize> = (0..self.herds.len()).filter(|&i| !self.herds[i].drawn && arrive(&species[self.herds[i].species], models, self.herds[i].centre, player, facing)).collect();
        near.sort_by(|&a, &b| flat(self.herds[a].centre).total_cmp(&flat(self.herds[b].centre)));
        let mut drew = 0;
        for i in near {
            if drew >= PER_CALL || total + self.herds[i].size > max_drawn() {
                break;
            }
            if draw_herd(world, species, models, land, store, plants, rng, &mut self.herds[i], now) {
                // A pack that ate while not drawn is still full, and a herd
                // met at night is in bed: its animals lie down on their first
                // choice (walk.rs `Sleep`) rather than graze first.
                let fed = roam.fed_left(self.herds[i].id, now);
                let night = roam.sleep_of(self.herds[i].id);
                for (r, crit) in world.query_mut::<(&Recorded, &mut Critter)>() {
                    if r.0 == self.herds[i].id {
                        crit.fed = crit.fed.max(fed);
                        crit.sleep = night;
                        if night == super::walk::Sleep::Bedded {
                            crit.timer = crit.timer.min(0.2);
                        }
                    }
                }
                drew += 1;
                self.drawn += 1;
                total += self.herds[i].size;
            }
        }
        // The rest move on.
        self.since += dt;
        if self.since >= UNSEEN_S {
            let step = self.since;
            self.since = 0.0;
            if let Some(graph) = land.survey.as_ref().map(|s| &s.graph) {
                for h in self.herds.iter_mut().filter(|h| !h.drawn) {
                    roam.unseen(h.id, h.species, species, h.size as usize, &mut h.centre, graph, live, hour, now, step);
                }
            }
            self.encounters(species, models, roam, rng, now);
            self.grow(species, models, land, roam, rng, hour, now, player, facing);
        }
        // Dev hook: VG_KILL_SITE=1 lays one kill site 20 m east of the player
        // a few seconds in (a made-up adult of the first herd's kind), to see
        // a kill out of sight put down.
        if now > 3.0 && self.killed == 0 && std::env::var_os("VG_KILL_SITE").is_some() {
            if let Some(i) = self.herds.iter().position(|h| species[h.species].hunts.is_empty()) {
                let mut h = self.herds[i].clone();
                h.members.clear();
                let victim = take_weakest(&mut h, species, models, rng);
                let by = self.herds.iter().find(|h| !species[h.species].hunts.is_empty()).map_or(h.species, |h| h.species);
                self.kills.push(KillSite { at: Vec3::new(player.x + 20.0, 0.0, player.z), victim, by, when: now, ent: None });
                self.killed += 1;
            }
        }
        self.kill_sites(world, models, land, store, player, now);
    }

    /// Out of sight, every pack shadowing its quarry within `STRIKE_M` rolls
    /// for a kill. Both must be records not drawn: near the player the chase
    /// is walk.rs's.
    fn encounters(&mut self, species: &[Species], models: &[Model], roam: &mut super::roam::Roam, rng: &mut Rng, now: f32) {
        let tries: Vec<(usize, usize)> = (0..self.herds.len())
            .filter(|&i| !self.herds[i].drawn)
            .filter_map(|i| {
                let (q, d) = roam.striking(self.herds[i].id)?;
                let j = self.herds.iter().position(|h| h.id == q && !h.drawn)?;
                (d < STRIKE_M).then_some((i, j))
            })
            .collect();
        let mut gone: Vec<u32> = Vec::new();
        for (i, j) in tries {
            let hunters = self.herds[i].size.clamp(1, 4) as f32;
            if gone.contains(&self.herds[j].id) || rng.unit() >= STRIKE_CHANCE * (1.0 + 0.5 * (hunters - 1.0)) {
                continue;
            }
            let (pack, by) = (self.herds[i].id, self.herds[i].species);
            let q = &mut self.herds[j];
            let victim = take_weakest(q, species, models, rng);
            let at = q.centre + victim.off;
            q.size = q.size.saturating_sub(1);
            let (quarry, left) = (q.id, q.size);
            roam.killed(pack, quarry, at, now);
            // The pack lies up at the kill.
            self.herds[i].centre = Vec3::new(at.x, 0.0, at.z);
            if std::env::var_os("VG_ROAM_LOG").is_some() {
                eprintln!("roam {now:.0} {pack:x} {}: killed a {} from herd {quarry:x} out of sight at {:.0}, {:.0}; {left} left", kind(&species[by]), species[victim.species].name, at.x, at.z);
            }
            self.kills.push(KillSite { at: Vec3::new(at.x, 0.0, at.z), victim, by, when: now, ent: None });
            self.killed += 1;
            if left == 0 {
                gone.push(quarry);
            }
        }
        for g in gone {
            self.lose(g, roam);
        }
    }

    /// Herd `g` is gone, every animal of it killed or starved: forget it, and
    /// send another of its kind in from the edge in `REPLACE_H` game hours to
    /// take its ground (`grow`), so each territory keeps its herd.
    fn lose(&mut self, g: u32, roam: &mut super::roam::Roam) {
        if let Some(h) = self.herds.iter().find(|h| h.id == g) {
            self.pending.push((h.species, h.home, self.hours + REPLACE_H));
        }
        self.herds.retain(|h| h.id != g);
        roam.forget(g);
        self.lost += 1;
    }

    /// Put down as a carcass every kill site the player has come near, and
    /// forget the ones whose bones are gone. A site whose carcass the
    /// spawner has dropped far behind the player is put down again on the
    /// way back.
    #[allow(clippy::too_many_arguments)]
    fn kill_sites(&mut self, world: &mut hecs::World, models: &[Model], land: &Land, store: &Store, player: Vec3, now: f32) {
        let last = super::age::CARCASS_S + super::age::BONES_LAST_S;
        self.kills.retain(|k| now - k.when < last);
        for k in self.kills.iter_mut() {
            if k.ent.is_some_and(|e| !world.contains(e)) {
                k.ent = None;
            }
            let d = Vec3::new(k.at.x - player.x, 0.0, k.at.z - player.z).length();
            if k.ent.is_some() || d > KILL_SEEN_M {
                continue;
            }
            let ground = super::carcass::ground_at(store, Some(land), Vec3::new(k.at.x, land.f.height(k.at.x, k.at.z) + 20.0, k.at.z));
            let m = &k.victim;
            let pos = Vec3::new(k.at.x, ground + 0.05, k.at.z);
            let e = world.spawn((
                Creature { species: m.species, spawned_for: m.spawned_for },
                Look::new(m.model, pos, m.yaw),
                Age { stage: Stage::Dead, until: k.when + last, bones_at: k.when + super::age::CARCASS_S },
            ));
            if let Some(mo) = m.morph.clone() {
                let _ = world.insert_one(e, mo);
            }
            if let Some(c) = m.coat {
                let _ = world.insert_one(e, c);
            }
            if !super::carcass::go_limp(world, e, models) {
                let _ = world.despawn(e);
                continue;
            }
            // Picked over by the pack: what is left of the meat.
            let mut c = super::carcass::Carcass::fresh(m.height, pos);
            c.meat *= 0.3;
            let _ = world.insert_one(e, c);
            k.ent = Some(e);
        }
    }
}

/// Young a herd has a day, per animal, with room to grow and good grazing.
const BIRTHS_PER_DAY: f32 = 0.35;
/// A pack that has eaten in the last day has this many young a day, per
/// animal (prides and packs breed slowly).
const PACK_BIRTHS_PER_DAY: f32 = 0.12;
/// A pack loses one to hunger each this many game hours without a meal.
const STARVE_H: f32 = 60.0;
/// Under this share of its target a species has a herd walk in.
const FLOOR: f32 = 0.5;
/// A lost herd is replaced from the edge this many game hours after.
const REPLACE_H: f32 = 6.0;
/// At most one walk-in per species in this many game hours.
const WALK_IN_H: f32 = 12.0;
/// A herd half again its group's largest splits in two.
const SPLIT_AT: f32 = 1.5;
/// A walk-in sets off this far inside the map edge, and only this far or
/// more from the player, metres.
const EDGE_IN_M: f32 = 24.0;
const WALK_IN_AWAY_M: f32 = 400.0;

impl Roster {
    /// Births, splits, hunger and walk-ins for the game hours since the last
    /// call (the clock read from `hour`). Only records not drawn change:
    /// nothing is born or starves in front of the player.
    #[allow(clippy::too_many_arguments)]
    pub fn grow(&mut self, species: &[Species], models: &[Model], land: &Land, roam: &mut super::roam::Roam, rng: &mut Rng, hour: f32, now: f32, player: Vec3, facing: Vec3) {
        if self.target.len() != species.len() {
            // A roster read from a file: the targets a fresh seeding would set.
            let fresh = seed(land, species, models);
            self.set_targets(species, &fresh);
        }
        let dh = self.last_hour.map_or(0.0, |l| (hour - l).rem_euclid(24.0));
        self.last_hour = Some(hour);
        if dh <= 0.0 {
            return;
        }
        self.hours += dh;
        let days = dh / 24.0;
        let mut have = vec![0u32; species.len()];
        for h in &self.herds {
            have[h.species] += h.size;
        }
        let survey = land.survey.as_ref();
        let mut new_herds: Vec<Herd> = Vec::new();
        let mut gone: Vec<u32> = Vec::new();
        for h in self.herds.iter_mut().filter(|h| !h.drawn) {
            let sp = &species[h.species];
            let hunter = !sp.hunts.is_empty();
            let room = (1.0 - have[h.species] as f32 / self.target[h.species].max(1) as f32).max(0.0);
            // A hunter of herds lives by its kills; one after lizards or
            // rabbits (not records) eats what is not kept here, and grows as
            // a herd does.
            let hunts_herds = hunter && sp.hunts.iter().any(|&q| recorded(&species[q]));
            let rate = if hunts_herds {
                // Fed in the last day: young. A long hunger: one dies.
                match roam.ate_at(h.id) {
                    Some(t) if t > h.last_meal => {
                        h.last_meal = t;
                        h.hungry_h = 0.0;
                    }
                    _ => h.hungry_h += dh,
                }
                if h.hungry_h > STARVE_H {
                    h.hungry_h = STARVE_H * 0.5;
                    if h.size > 0 {
                        h.size -= 1;
                        h.members.pop();
                        self.starved += 1;
                        have[h.species] = have[h.species].saturating_sub(1);
                        if h.size == 0 {
                            gone.push(h.id);
                        }
                    }
                    continue;
                }
                if h.hungry_h < 24.0 {
                    PACK_BIRTHS_PER_DAY
                } else {
                    0.0
                }
            } else {
                let graze = survey.map_or(1.0, |sv| sv.byte(crate::world::survey::Chan::Graze, h.centre.x, h.centre.z) as f32 / 255.0);
                if hunter {
                    PACK_BIRTHS_PER_DAY
                } else {
                    BIRTHS_PER_DAY * (0.4 + 0.6 * graze)
                }
            };
            let expect = rate * room * h.size as f32 * days;
            let n = expect.floor() as u32 + (rng.unit() < expect.fract()) as u32;
            for _ in 0..n {
                born_into(h, models, rng);
                self.born += 1;
                have[h.species] += 1;
            }
            // Too big: half of it goes its own way.
            let most = sp.beh.group.as_ref().map_or(1, |g| g.size[1]).max(2);
            if h.size as f32 > most as f32 * SPLIT_AT && h.size >= 4 {
                let half = h.size / 2;
                let mut other = Herd { id: 0, species: h.species, size: half, centre: h.centre + Vec3::new(rng.unit() - 0.5, 0.0, rng.unit() - 0.5) * 60.0, drawn: false, members: Vec::new(), home: h.home, wary: 0.0, hungry_h: h.hungry_h, last_meal: h.last_meal };
                if !h.members.is_empty() {
                    let at = h.members.len().saturating_sub(half as usize);
                    other.members = h.members.split_off(at);
                    other.size = other.members.len() as u32;
                }
                h.size -= other.size;
                new_herds.push(other);
            }
        }
        for g in gone {
            self.lose(g, roam);
        }
        for mut h in new_herds {
            h.id = self.next_id;
            self.next_id += 2;
            self.split += 1;
            self.herds.push(h);
        }
        // Herds lost a while ago: one of the kind comes in from the edge
        // nearest its ground and makes for it (roam.rs `Need::Home`),
        // crossing whatever lies between.
        let due: Vec<(usize, Vec3, f32)> = self.pending.iter().copied().filter(|p| p.2 <= self.hours).collect();
        for (s, home, when) in due {
            let Some(at) = edge_in(land, home, player, facing) else { continue };
            self.pending.retain(|p| !(p.0 == s && p.1 == home && p.2 == when));
            let g = species[s].beh.group.clone().unwrap_or_default();
            let least = if species[s].hunts.is_empty() { 2 } else { 1 };
            let size = g.size[0].max(least) + (rng.unit() * (g.size[1].max(least) - g.size[0].max(least) + 1) as f32) as u32;
            let id = self.next_id;
            self.next_id += 2;
            self.herds.push(Herd { id, species: s, size, centre: at, drawn: false, members: Vec::new(), home, wary: 0.0, hungry_h: 0.0, last_meal: f32::NEG_INFINITY });
            roam.adopt(id, home);
            have[s] += size;
            self.replaced += 1;
            if std::env::var_os("VG_ROAM_LOG").is_some() {
                eprintln!("roam {now:.0} {id:x} {}: {size} {} come in at {:.0}, {:.0} for the ground at {:.0}, {:.0}", kind(&species[s]), species[s].name, at.x, at.z, home.x, home.z);
            }
        }
        // Too few of a species: a herd walks in from an edge.
        for s in 0..species.len() {
            let target = self.target[s];
            if target == 0 || (have[s] as f32) >= target as f32 * FLOOR || self.hours - self.walked_at[s] < WALK_IN_H || self.homes[s].is_empty() {
                continue;
            }
            let home = self.homes[s][((rng.unit() * self.homes[s].len() as f32) as usize).min(self.homes[s].len() - 1)];
            let Some(at) = edge_in(land, home, player, facing) else { continue };
            let g = species[s].beh.group.clone().unwrap_or_default();
            let least = if species[s].hunts.is_empty() { 2 } else { 1 };
            let size = g.size[0].max(least) + (rng.unit() * (g.size[1].max(least) - g.size[0].max(least) + 1) as f32) as u32;
            self.herds.push(Herd { id: self.next_id, species: s, size, centre: at, drawn: false, members: Vec::new(), home, wary: 0.0, hungry_h: 0.0, last_meal: f32::NEG_INFINITY });
            roam.adopt(self.next_id, home);
            if std::env::var_os("VG_ROAM_LOG").is_some() {
                eprintln!("roam {now:.0} {:x} {}: {} walked in from the edge at {:.0}, {:.0} ({} of {target})", self.next_id, kind(&species[s]), species[s].name, at.x, at.z, have[s]);
            }
            self.next_id += 2;
            self.walked_at[s] = self.hours;
            self.walked_in += 1;
        }
    }
}

/// One young born into herd `h`: a juvenile of an adult's kind and coat,
/// beside it, its build and size rolled when it is drawn (spawn.rs
/// `roll_morphs`). A herd never drawn just counts one more.
fn born_into(h: &mut Herd, models: &[Model], rng: &mut Rng) {
    h.size += 1;
    if h.members.is_empty() {
        return;
    }
    let adults: Vec<usize> = (0..h.members.len()).filter(|&k| h.members[k].stage == Stage::Adult).collect();
    let pool: Vec<usize> = if adults.is_empty() { (0..h.members.len()).collect() } else { adults };
    let mum = &h.members[pool[((rng.unit() * pool.len() as f32) as usize).min(pool.len() - 1)]];
    let md = &models[mum.model];
    let child = Member {
        off: mum.off + Vec3::new(rng.unit() - 0.5, 0.0, rng.unit() - 0.5) * 2.0,
        half_w: md.half_w,
        height: md.height,
        stage: Stage::Juvenile,
        seed: hash32(mum.seed ^ h.size.wrapping_mul(0x9e37_79b9)),
        morph: None,
        sex: None,
        ..mum.clone()
    };
    h.members.push(child);
}

/// Where a herd walking in sets off: the map edge nearest `home`, a little
/// inside, on ground a large walker can stand on, far from the player and
/// out of their view. None if that spot won't do.
fn edge_in(land: &Land, home: Vec3, player: Vec3, facing: Vec3) -> Option<Vec3> {
    let size = land.f.size;
    let edges = [(home.x, Vec3::new(EDGE_IN_M, 0.0, home.z)), (size - home.x, Vec3::new(size - EDGE_IN_M, 0.0, home.z)), (home.z, Vec3::new(home.x, 0.0, EDGE_IN_M)), (size - home.z, Vec3::new(home.x, 0.0, size - EDGE_IN_M))];
    let (_, at) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    let ok_ground = land.survey.as_ref().map_or(true, |sv| sv.byte(crate::world::survey::Chan::PassLarge, at.x, at.z) > 0 && sv.byte(crate::world::survey::Chan::Water, at.x, at.z) == 0);
    let far = Vec3::new(at.x - player.x, 0.0, at.z - player.z).length() > WALK_IN_AWAY_M;
    // Nearer the edge failed: walk in from home itself, if it is out of the way.
    if ok_ground && far && !super::spawn::in_view(at, player, facing) {
        Some(at)
    } else if Vec3::new(home.x - player.x, 0.0, home.z - player.z).length() > WALK_IN_AWAY_M && !super::spawn::in_view(home, player, facing) {
        Some(Vec3::new(home.x, 0.0, home.z))
    } else {
        None
    }
}

/// Take the weakest of herd `h` for a kill out of sight: a young one if it
/// has one, else an old one, else any. A herd never drawn has no animals
/// kept yet, and one of its kind is made up.
fn take_weakest(h: &mut Herd, species: &[Species], models: &[Model], rng: &mut Rng) -> Member {
    if !h.members.is_empty() {
        let rank = |m: &Member| match m.stage {
            Stage::Juvenile => 0,
            Stage::Aged => 1,
            _ => 2,
        };
        let best = h.members.iter().map(rank).min().unwrap_or(2);
        let pick: Vec<usize> = (0..h.members.len()).filter(|&k| rank(&h.members[k]) == best).collect();
        let k = pick[((rng.unit() * pick.len() as f32) as usize).min(pick.len() - 1)];
        return h.members.remove(k);
    }
    let sp = &species[h.species];
    let md = &models[sp.model];
    let a = rng.unit() * std::f32::consts::TAU;
    Member {
        off: Vec3::new(a.cos(), 0.0, a.sin()) * 4.0,
        yaw: rng.unit() * std::f32::consts::TAU,
        heading: 0.0,
        species: h.species,
        spawned_for: h.species,
        model: sp.model,
        half_w: md.half_w,
        height: md.height,
        stage: Stage::Adult,
        seed: hash32(h.id ^ h.size),
        side: 1.0,
        morph: None,
        coat: None,
        sex: None,
    }
}

/// Whether a drawn herd of `sp` with its middle at `c` is far enough from
/// the player to be written back: the spawner's own rule for dropping
/// (spawn.rs `reach`, `in_view`), a little further, so a herd is not drawn
/// and stored back and forth at the line.
fn leave(sp: &Species, models: &[Model], c: Vec3, player: Vec3, facing: Vec3) -> bool {
    let (near, seen) = super::spawn::reach(sp, models);
    let d = Vec3::new(c.x - player.x, 0.0, c.z - player.z).length();
    d > seen * 1.05 || d > near * 1.1 && !super::spawn::in_view(c, player, facing)
}

/// Whether a herd not drawn should be: within its in-view reach while the
/// player may be looking (it dissolves in, spawn.rs `Appear`), or within its
/// out-of-view reach anyway.
fn arrive(sp: &Species, models: &[Model], c: Vec3, player: Vec3, facing: Vec3) -> bool {
    let (near, seen) = super::spawn::reach(sp, models);
    let d = Vec3::new(c.x - player.x, 0.0, c.z - player.z).length();
    d < near * 0.9 || d < seen * 0.9 && super::spawn::in_view(c, player, facing)
}

/// Write a drawn herd back and take its animals out.
fn store_herd(world: &mut hecs::World, h: &mut Herd) {
    let members = members_of(world, h.id, h.centre);
    let ents: Vec<(hecs::Entity, bool)> = world.query::<(hecs::Entity, &Recorded, Option<&Critter>)>().iter().filter(|(_, r, _)| r.0 == h.id).map(|(e, _, c)| (e, c.is_some())).collect();
    for (e, alive) in ents {
        // A carcass of the herd is left to the carcass's own clock.
        if alive {
            let _ = world.despawn(e);
        } else {
            let _ = world.remove_one::<Recorded>(e);
        }
    }
    h.size = members.len() as u32;
    h.members = members;
    h.drawn = false;
}

/// Put a herd's animals in the world round its middle: the ones written back
/// if it has been drawn before, else a new family group made the spawner's
/// way. False when the ground is not there yet (try again later).
#[allow(clippy::too_many_arguments)]
fn draw_herd(world: &mut hecs::World, species: &[Species], models: &[Model], land: &Land, store: &Store, plants: &super::plants::Flora, rng: &mut Rng, h: &mut Herd, now: f32) -> bool {
    let ground = |x: f32, z: f32| super::terrain::voxel_ground(store, x, z, store.top_m()).map(|g| g.0).or_else(|| super::terrain::far_ground(store, x, z));
    let mut made: Vec<hecs::Entity> = Vec::new();
    if h.members.is_empty() {
        let before: std::collections::HashSet<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).collect();
        if !super::spawn::place(world, species, h.species, rng, land, store, models, plants, (h.centre.x, h.centre.z), h.size, now, None, true) {
            return false;
        }
        super::spawn::dress(world, species, h.species, super::spawn::biome_pick(land, h.centre.x, h.centre.z, &species[h.species].name), rng);
        made = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).filter(|e| !before.contains(e)).collect();
    } else {
        // Every member's ground must be there, or none is put down yet.
        let spots: Option<Vec<f32>> = h.members.iter().map(|m| ground(h.centre.x + m.off.x, h.centre.z + m.off.z)).collect();
        let Some(spots) = spots else { return false };
        for (m, gy) in h.members.iter().zip(spots) {
            let pos = Vec3::new(h.centre.x + m.off.x, gy + 0.01, h.centre.z + m.off.z);
            let e = world.spawn((
                Critter::new(m.heading, 1.0, m.seed, m.side),
                Creature { species: m.species, spawned_for: m.spawned_for },
                Body::new(m.half_w, m.height),
                Look::new(m.model, pos, m.yaw),
                Age::new(m.stage),
            ));
            // Its own build, coat and sex, so the same animal comes back
            // (spawn.rs `roll_morphs` leaves one that has a Morph alone).
            if let Some(mo) = m.morph.clone() {
                let _ = world.insert_one(e, mo);
            }
            if let Some(c) = m.coat {
                let _ = world.insert_one(e, c);
            }
            if let Some(s) = m.sex {
                let _ = world.insert_one(e, s);
            }
            made.push(e);
        }
    }
    if made.is_empty() {
        return false;
    }
    for &e in &made {
        let _ = world.insert_one(e, Kin { group: h.id });
        let _ = world.insert_one(e, Recorded(h.id));
        let _ = world.insert_one(e, super::spawn::Appear(now));
    }
    // Bones the spawner laid with a new family group are not members.
    h.size = made.iter().filter(|e| world.get::<&Critter>(**e).is_ok()).count() as u32;
    h.drawn = true;
    true
}

/// The map's herds: on a 64 m grid, each recorded species a biome lists
/// adds the share of its count that the spawner would keep on that much
/// ground (its count is for the circle the spawner fills round the player);
/// whenever a species' running total makes up a herd, one is put down at a
/// passable dry spot in that cell. The cells are taken in a hashed order so
/// herds spread rather than lining up from one corner.
fn seed(land: &Land, species: &[Species], models: &[Model]) -> Vec<Herd> {
    let f = &land.f;
    let n = (f.size / SEED_CELL_M).ceil() as usize;
    let mut cells: Vec<usize> = (0..n * n).collect();
    cells.sort_by_key(|&k| hash32(k as u32 ^ 0x5eed_4e7d));
    let survey = land.survey.as_ref();
    let mut acc = vec![0f32; species.len()];
    let mut want = vec![0u32; species.len()];
    let mut out = Vec::new();
    // A herd is two at the least; a predator may go alone (a pride of one
    // to three lions is sometimes one).
    let size_of = |s: usize, roll: f32| {
        let g = species[s].beh.group.clone().unwrap_or_default();
        let least = if species[s].hunts.is_empty() { 2 } else { 1 };
        let lo = g.size[0].max(least);
        lo + (roll * (g.size[1].max(lo) - lo + 1) as f32) as u32
    };
    for (s, w) in want.iter_mut().enumerate() {
        *w = size_of(s, unit(hash32(s as u32 ^ 0x4e7d)));
    }
    for (order, &k) in cells.iter().enumerate() {
        let (cx, cz) = (((k % n) as f32 + 0.5) * SEED_CELL_M, ((k / n) as f32 + 0.5) * SEED_CELL_M);
        let b = &land.lib.biomes[f.biome[f.idx(cx, cz)] as usize];
        for cu in &b.creatures {
            let Some(s) = species.iter().position(|x| x.name == cu.creature) else { continue };
            let sp = &species[s];
            if !recorded(sp) {
                continue;
            }
            // The spawner keeps `count` within its reach round the player:
            // 90 m, or 180 m and half as many again for a large animal.
            let (r, more) = if sp.large(models) { (super::spawn::SPAWN_M * super::spawn::LARGE_REACH, 1.5) } else { (super::spawn::SPAWN_M, 1.0) };
            acc[s] += cu.count * more * SEED_CELL_M * SEED_CELL_M / (std::f32::consts::PI * r * r);
            while acc[s] >= want[s] as f32 {
                let seedv = hash32(k as u32 ^ (s as u32).wrapping_mul(0x9e37_79b9) ^ out.len() as u32);
                // A dry spot a large walker can stand on, in this cell, in
                // ground this biome covers.
                let mut spot = None;
                for t in 0..8u32 {
                    let h = hash32(seedv ^ t.wrapping_mul(0x85eb_ca6b));
                    let (x, z) = (cx + (unit(h) - 0.5) * SEED_CELL_M, cz + (unit(h >> 7) - 0.5) * SEED_CELL_M);
                    if x < 16.0 || z < 16.0 || x > f.size - 16.0 || z > f.size - 16.0 || f.height(x, z) < f.sea + 0.5 {
                        continue;
                    }
                    let ok = survey.map_or(true, |sv| sv.byte(crate::world::survey::Chan::PassLarge, x, z) > 0 && sv.byte(crate::world::survey::Chan::Water, x, z) == 0);
                    if ok && land.lib.biomes[f.biome[f.idx(x, z)] as usize].creatures.iter().any(|c| c.creature == sp.name) {
                        spot = Some(Vec3::new(x, 0.0, z));
                        break;
                    }
                }
                acc[s] -= want[s] as f32;
                if let Some(centre) = spot {
                    out.push(Herd { id: RECORD_BIT | ((out.len() as u32 + 1) << 1), species: s, size: want[s], centre, drawn: false, members: Vec::new(), home: centre, wary: 0.0, hungry_h: 0.0, last_meal: f32::NEG_INFINITY });
                }
                want[s] = size_of(s, unit(hash32(seedv ^ order as u32)));
            }
        }
    }
    out
}

/// One animal as the file keeps it.
#[derive(Serialize, Deserialize)]
struct SavedMember {
    off: [f32; 2],
    yaw: f32,
    heading: f32,
    species: String,
    spawned_for: String,
    half_w: f32,
    height: f32,
    stage: u8,
    seed: u32,
    side: f32,
    sex: Option<u8>,
    coat: Option<(usize, f32)>,
    /// scale, tint, shift, speed, wary, head, supple, glow, then the habits.
    morph: Option<Vec<f32>>,
}

#[derive(Serialize, Deserialize)]
struct SavedHerd {
    /// How wary it was (roam.rs C4); missing in older files.
    #[serde(default)]
    wary: f32,
    species: String,
    size: u32,
    at: [f32; 2],
    members: Vec<SavedMember>,
}

#[derive(Serialize, Deserialize)]
struct SavedRoster {
    version: u32,
    world: String,
    /// The survey's key (world/survey), 0 for a world without one.
    key: u64,
    herds: Vec<SavedHerd>,
    lost: u32,
}

fn stage_code(s: Stage) -> u8 {
    match s {
        Stage::Juvenile => 0,
        Stage::Adult => 1,
        Stage::Aged => 2,
        Stage::Dead => 3,
    }
}

fn stage_of(c: u8) -> Stage {
    match c {
        0 => Stage::Juvenile,
        2 => Stage::Aged,
        3 => Stage::Dead,
        _ => Stage::Adult,
    }
}

fn morph_vec(m: &crate::variety::Morph) -> Vec<f32> {
    let mut v = vec![m.scale];
    v.extend(m.tint);
    v.extend(m.shift);
    v.extend([m.speed, m.wary, m.head, m.supple, m.glow]);
    v.extend(m.habits);
    v
}

fn morph_of(v: &[f32]) -> Option<crate::variety::Morph> {
    let habits: crate::anim::life::Habits = v.get(12..)?.try_into().ok()?;
    Some(crate::variety::Morph {
        scale: v[0],
        tint: [v[1], v[2], v[3]],
        shift: [v[4], v[5], v[6]],
        speed: v[7],
        wary: v[8],
        head: v[9],
        supple: v[10],
        glow: v[11],
        habits,
    })
}

fn survey_key(land: &Land) -> u64 {
    land.survey.as_ref().map_or(0, |s| s.key)
}

impl Roster {
    /// Write the roster to `path`: the herds not drawn as they are kept, the
    /// drawn ones from their animals as they stand now (left in the world).
    pub fn save(&self, world: &hecs::World, species: &[Species], roam: &super::roam::Roam, land: &Land, path: &std::path::Path) -> std::io::Result<()> {
        let name = |s: usize| species.get(s).map_or(String::new(), |x| x.name.clone());
        let saved_member = |m: &Member| SavedMember {
            off: [m.off.x, m.off.z],
            yaw: m.yaw,
            heading: m.heading,
            species: name(m.species),
            spawned_for: name(m.spawned_for),
            half_w: m.half_w,
            height: m.height,
            stage: stage_code(m.stage),
            seed: m.seed,
            side: m.side,
            sex: m.sex.map(|s| (s == super::sex::Sex::Male) as u8),
            coat: m.coat.map(|c| (c.which, c.at)),
            morph: m.morph.as_ref().map(morph_vec),
        };
        let mut herds = Vec::with_capacity(self.herds.len());
        for h in &self.herds {
            let members: Vec<SavedMember> = if h.drawn { members_of(world, h.id, h.centre).iter().map(saved_member).collect() } else { h.members.iter().map(saved_member).collect() };
            let size = if h.drawn { members.len() as u32 } else { h.size };
            herds.push(SavedHerd { wary: roam.wary_of(h.id), species: name(h.species), size, at: [h.centre.x, h.centre.z], members });
        }
        let file = SavedRoster { version: FILE_VERSION, world: land.def.name.clone(), key: survey_key(land), herds, lost: self.lost };
        let text = serde_json::to_string(&file).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Take the herds from `path` for world `land`, if the file is there and
    /// was written for this world as it is now. True when it was; the map
    /// is then not seeded. A herd whose species is gone from the library is
    /// left out, and so is an animal of a species that is gone.
    pub fn load(&mut self, species: &[Species], land: &Land, path: &std::path::Path) -> bool {
        let Ok(text) = std::fs::read_to_string(path) else { return false };
        let Ok(file) = serde_json::from_str::<SavedRoster>(&text) else {
            eprintln!("herds: {} will not read; seeding the map afresh", path.display());
            return false;
        };
        if file.version != FILE_VERSION || file.world != land.def.name || file.key != survey_key(land) {
            eprintln!("herds: {} was written for another build of this world; seeding the map afresh", path.display());
            return false;
        }
        let find = |n: &str| species.iter().position(|s| s.name == n);
        let mut herds = Vec::with_capacity(file.herds.len());
        for (k, sh) in file.herds.into_iter().enumerate() {
            let Some(s) = find(&sh.species) else { continue };
            let members: Vec<Member> = sh
                .members
                .into_iter()
                .filter_map(|m| {
                    let sp = find(&m.species)?;
                    let sex = m.sex.map(|x| if x == 1 { super::sex::Sex::Male } else { super::sex::Sex::Female });
                    let stage = stage_of(m.stage);
                    Some(Member {
                        off: Vec3::new(m.off[0], 0.0, m.off[1]),
                        yaw: m.yaw,
                        heading: m.heading,
                        species: sp,
                        spawned_for: find(&m.spawned_for).unwrap_or(sp),
                        model: species[sp].model_for(sex, stage),
                        half_w: m.half_w,
                        height: m.height,
                        stage,
                        seed: m.seed,
                        side: m.side,
                        morph: m.morph.as_deref().and_then(morph_of),
                        coat: m.coat.map(|(which, at)| Coat { which, at }),
                        sex,
                    })
                })
                .collect();
            let size = if members.is_empty() { sh.size } else { members.len() as u32 };
            herds.push(Herd { id: RECORD_BIT | ((k as u32 + 1) << 1), species: s, size, centre: Vec3::new(sh.at[0], 0.0, sh.at[1]), drawn: false, members, home: Vec3::new(sh.at[0], 0.0, sh.at[1]), wary: sh.wary, hungry_h: 0.0, last_meal: f32::NEG_INFINITY });
        }
        self.herds = herds;
        // The targets are worked out on the first `grow`, which has the models.
        self.target.clear();
        self.lost = file.lost;
        self.seeded_for = Some((land.def.name.clone(), land.f.size as u32));
        true
    }
}

/// A drawn herd's living animals as members, from where they stand.
fn members_of(world: &hecs::World, id: u32, centre: Vec3) -> Vec<Member> {
    let mut out = Vec::new();
    for (r, crit, c, look, body, age, morph, coat, sex) in world
        .query::<(&Recorded, &Critter, &Creature, &Look, &Body, Option<&Age>, Option<&crate::variety::Morph>, Option<&Coat>, Option<&super::sex::Sex>)>()
        .iter()
    {
        let stage = age.map_or(Stage::Adult, |a| a.stage);
        if r.0 != id || stage == Stage::Dead {
            continue;
        }
        out.push(Member {
            off: Vec3::new(look.pos.x - centre.x, 0.0, look.pos.z - centre.z),
            yaw: look.yaw,
            heading: crit.heading,
            species: c.species,
            spawned_for: c.spawned_for,
            model: look.model,
            half_w: body.half_w,
            height: body.height,
            stage,
            seed: crit.rng.state,
            side: crit.side,
            morph: morph.cloned(),
            coat: coat.copied(),
            sex: sex.copied(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_morph_reads_back_as_written() {
        let m = crate::variety::Morph {
            scale: 0.9,
            tint: [1.0, 0.8, 0.7],
            shift: [0.0, 0.1, 0.2],
            speed: 1.1,
            wary: 0.5,
            head: 1.2,
            supple: 0.8,
            glow: 0.0,
            habits: [1.0; crate::anim::life::ACTS.len()],
        };
        assert_eq!(morph_of(&morph_vec(&m)), Some(m));
        assert!(morph_of(&[1.0; 5]).is_none());
        for s in [Stage::Juvenile, Stage::Adult, Stage::Aged, Stage::Dead] {
            assert_eq!(stage_of(stage_code(s)), s);
        }
    }

    #[test]
    fn record_numbers_are_told_from_the_spawners() {
        assert!(is_record(RECORD_BIT | 2));
        assert!(is_record(RECORD_BIT | (5000 << 1)));
        // The spawner's group numbers are hashes with the low bit set.
        assert!(!is_record(0x8123_4567));
        assert!(!is_record(0x0000_0042));
    }
}
