//! The zoo: every library creature (or the ones a world's `zoo` block names)
//! in a pen of its own on a level lawn, for looking at animals up close
//! without waiting to meet them. A world opts in with `"zoo": {}` in its
//! world.json (`worlds/zoo`), and the biome spawner stands down there.
//!
//! A walker's pen is a grid. Each column is one age and sex (juvenile,
//! adult and aged, female and male, leaving out the ages the species does
//! not have), and each row holds every individual in it at one activity
//! (`ROWS`): free (its own choices, kept near home), stand, look, graze,
//! drink, sit, lie, walk (pacing a line) and run (pacing a longer one, as
//! fleeing). Below the grid each of the species' displays has a stage with a
//! cast made to fit it, and the display is started again a few seconds after
//! it ends. Every other way of moving gets a group put down by the spawner's
//! own `place` at the pen, left to itself; swimmers and shore animals have
//! their pens along the sea at the south edge.
//!
//! Pens fill when the player comes within `FILL_M` and are emptied past
//! `EMPTY_M`, so only the few in view cost anything. Kept animals are tame:
//! they don't mind the player, hunt or run from each other (walk.rs).
//!
//! `VG_ZOO=deer,heron` pens only those and their relatives, and starts the
//! player at the first pen. `VG_ZOO_LOG=1` prints where every pen is.

use super::age::Stage;
use super::locomotion::Mover;
use super::sex::Sex;
use super::walk::Critter;
use super::{Creature, Look, Species};
use crate::anim::life::Act;
use crate::library::Library;
use crate::models::Model;
use crate::physics::Body;
use crate::world::fields::ZooDef;
use crate::world::noise::{hash32, Rng};
use crate::world::store::Store;
use glam::{Vec2, Vec3};

/// Pens whose nearest corner is this close to the player are filled.
pub const FILL_M: f32 = 70.0;
/// And emptied past this.
pub const EMPTY_M: f32 = 110.0;
/// Gap between pens, metres: the paths the player walks.
const PATH_M: f32 = 10.0;
/// Where the land pens start and how far east they run before a new row.
const LAND_X0: f32 = 30.0;
const LAND_Z0: f32 = 40.0;
/// The sea pens' line, metres in from the south edge's shore. The sea
/// starts about `SEA_REACH_M` in from the south edge (worlds/zoo).
const SEA_REACH_M: f32 = 300.0;
/// Seconds after a display ends before it is started again.
const RESTAGE_S: f32 = 3.0;

/// What a kept animal is held to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lock {
    /// Its own choices, kept within `leash` of home.
    Free,
    /// This activity and nothing else, at home. `Act::Walk` paces a line.
    Act(Act),
    /// Running: pacing a long line as if fleeing.
    Run,
    /// Waiting at home for the display its stage plays (an index into the
    /// species' `behaviour.displays`).
    Display(usize),
}

impl Lock {
    pub fn name(self) -> &'static str {
        match self {
            Lock::Free => "free",
            Lock::Act(a) => a.name(),
            Lock::Run => "run",
            Lock::Display(_) => "display",
        }
    }
}

/// On every animal the zoo put out.
#[derive(Clone, Copy, Debug)]
pub struct Kept {
    /// Which pen (`Zoo::pens`) it belongs to.
    #[allow(dead_code)] // for debugging and the census
    pub pen: u32,
    /// Where it stands, and the far end of the line it paces (walk, run).
    pub home: Vec3,
    pub to: Vec3,
    /// The way it faces at home (a `Look::yaw`).
    pub yaw: f32,
    pub lock: Lock,
    /// How far a free one may wander from home, metres.
    pub leash: f32,
    pub sex: Sex,
    /// Pacing toward `to` (else back toward `home`).
    pub outward: bool,
    /// Its display stage, shared by the cast (0: none). Doubles as the cast's
    /// `Kin`, so a family display finds its family.
    pub group: u32,
}

/// One place in a pen.
struct Slot {
    at: Vec2,
    /// The far end of a pacing line.
    to: Vec2,
    stage: Stage,
    sex: Sex,
    lock: Lock,
    leash: f32,
    group: u32,
}

pub struct Pen {
    pub species: usize,
    /// North-west corner and size, metres.
    pub min: Vec2,
    pub size: Vec2,
    /// What the HUD calls each row, top to bottom, with its z.
    pub rows: Vec<(String, f32)>,
    slots: Vec<Slot>,
    /// A walker pen is laid out slot by slot; anything else is one group
    /// put down by the spawner at the pen's middle.
    walkers: bool,
    ents: Vec<Option<hecs::Entity>>,
    live: bool,
    /// Seconds before an empty non-walker pen is tried again.
    retry: f32,
}

impl Pen {
    pub fn contains(&self, p: Vec2) -> bool {
        p.cmpge(self.min - Vec2::splat(PATH_M * 0.5)).all() && p.cmple(self.min + self.size + Vec2::splat(PATH_M * 0.5)).all()
    }

    fn dist(&self, p: Vec2) -> f32 {
        let q = p.clamp(self.min, self.min + self.size);
        (q - p).length()
    }

    /// The row the point is in, for the HUD.
    pub fn row_at(&self, z: f32) -> Option<&str> {
        self.rows.iter().rev().find(|(_, rz)| z >= *rz - 0.5).map(|(n, _)| n.as_str())
    }
}

#[derive(Default)]
pub struct Zoo {
    pub pens: Vec<Pen>,
    /// Display stages waiting to start again: (lead, display, seconds left).
    restage: Vec<(hecs::Entity, usize, f32)>,
    next_group: u32,
    /// Set when `VG_ZOO` picked the pens: the game takes the player to the
    /// first one (game.rs `zoo_jump`).
    pub jump: bool,
}

/// Rows of a walker's pen, top to bottom, and how many cells deep each is.
const ROWS: [(Lock, f32); 9] = [
    (Lock::Free, 2.0),
    (Lock::Act(Act::Stand), 1.0),
    (Lock::Act(Act::Look), 1.0),
    (Lock::Act(Act::Graze), 1.0),
    (Lock::Act(Act::Drink), 1.0),
    (Lock::Act(Act::Sit), 1.0),
    (Lock::Act(Act::Lie), 1.0),
    (Lock::Act(Act::Walk), 3.0),
    (Lock::Run, 5.0),
];

/// Is creature `name` one the zoo pens? `VG_ZOO` wins over the world's
/// `only`; a name matches itself or any of its ancestors.
pub fn wanted(lib: &Library, z: &ZooDef, name: &str) -> bool {
    let env: Vec<String> = std::env::var("VG_ZOO").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let only = if env.is_empty() { &z.only } else { &env };
    // A form is penned as the sex or age of its species, not on its own.
    if lib.is_form(name) {
        return false;
    }
    if only.is_empty() {
        return true;
    }
    let mut at = Some(name.to_string());
    for _ in 0..8 {
        let Some(n) = at else { break };
        if only.contains(&n) {
            return true;
        }
        at = lib.creatures.iter().find(|c| c.name == n).and_then(|c| c.like.clone());
    }
    false
}

/// The root of a species' `like` chain, to keep families together.
fn family(species: &[Species], s: usize) -> String {
    let mut name = species[s].name.clone();
    let mut like = species[s].like.clone();
    for _ in 0..8 {
        let Some(l) = like else { break };
        match species.iter().find(|x| x.name == l) {
            Some(p) => {
                name = p.name.clone();
                like = p.like.clone();
            }
            None => {
                name = l;
                break;
            }
        }
    }
    name
}

/// Where a species lives in the zoo.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Ground {
    Land,
    Sea,
}

fn ground_of(sp: &Species) -> Ground {
    match sp.verb().mover {
        Mover::School | Mover::Breach | Mover::Paddle | Mover::Creep | Mover::Run { .. } => Ground::Sea,
        Mover::Walk { wades: true, .. } | Mover::Walk { dives: true, .. } => Ground::Sea,
        Mover::Buzz { water: true } => Ground::Sea,
        _ => Ground::Land,
    }
}

/// Is it a walker with a rig, whose activities can be held?
fn holdable(sp: &Species, models: &[Model]) -> bool {
    matches!(sp.verb().mover, Mover::Walk { still: false, .. }) && models.get(sp.model).is_some_and(|m| m.rig.is_some())
}

/// The ages and sexes a species' columns stand for.
fn columns(sp: &Species) -> Vec<(Stage, Sex)> {
    let mut out = Vec::new();
    for stage in [Stage::Juvenile, Stage::Adult, Stage::Aged] {
        let has = match stage {
            Stage::Juvenile => sp.ages.has_juvenile(),
            Stage::Aged => sp.ages.aged,
            _ => true,
        };
        if has {
            out.push((stage, Sex::Female));
            out.push((stage, Sex::Male));
        }
    }
    out
}

/// A model's footprint across, metres: its length or its breadth.
fn span(m: &Model) -> f32 {
    (m.reach[1] - m.reach[0]).max(2.0 * m.half_w).max(m.height * 0.6)
}

/// Facing east, side-on to someone walking the path along the pen's top.
const EAST_YAW: f32 = -std::f32::consts::FRAC_PI_2;

impl Zoo {
    /// Lay out a pen for every species the world loaded.
    pub fn build(lib: &Library, def: &ZooDef, species: &[Species], models: &[Model], size: f32) -> Zoo {
        let mut order: Vec<usize> = (0..species.len())
            .filter(|&s| !species[s].is_water_form && models.get(species[s].model).is_some_and(|m| m.height > 0.0) && wanted(lib, def, &species[s].name))
            .collect();
        order.sort_by_key(|&s| (ground_of(&species[s]), family(species, s), species[s].like.is_some(), species[s].name.clone()));
        let mut zoo = Zoo {
            next_group: 1,
            jump: std::env::var("VG_ZOO").is_ok_and(|v| !v.is_empty()),
            ..Default::default()
        };
        let land_w = size - 2.0 * LAND_X0;
        let sea_z = size - SEA_REACH_M + 25.0;
        let (mut x, mut z, mut row_d) = (LAND_X0, LAND_Z0, 0.0f32);
        let mut sea_x = LAND_X0;
        for s in order {
            let sp = &species[s];
            let m = &models[sp.model];
            let cell = (span(m) * 1.4 + 0.8).clamp(1.4, 30.0);
            let mut pen = Pen {
                species: s,
                min: Vec2::ZERO,
                size: Vec2::ZERO,
                rows: Vec::new(),
                slots: Vec::new(),
                walkers: false,
                ents: Vec::new(),
                live: false,
                retry: 0.0,
            };
            if ground_of(sp) == Ground::Sea {
                // A strip along the shore: the spawner sets each down on the
                // water or the bank by its own rules.
                let w = (cell * 4.0).max(24.0);
                pen.min = Vec2::new(sea_x, sea_z - 20.0);
                pen.size = Vec2::new(w, 40.0);
                pen.rows.push(("free".into(), pen.min.y));
                sea_x += w + PATH_M;
                zoo.pens.push(pen);
                continue;
            }
            if holdable(sp, models) {
                pen.walkers = true;
                let cols = columns(sp);
                let mut dz = 0.0;
                for (lock, deep) in ROWS {
                    pen.rows.push((lock.name().to_string(), dz));
                    for (k, &(stage, sex)) in cols.iter().enumerate() {
                        let cx = (k as f32 + 0.5) * cell;
                        let at = Vec2::new(cx, dz + cell * 0.5);
                        let to = Vec2::new(cx, dz + cell * (deep - 0.5));
                        let leash = if lock == Lock::Free { cell * 0.9 } else { 0.0 };
                        pen.slots.push(Slot { at, to, stage, sex, lock, leash, group: 0 });
                    }
                    dz += deep * cell;
                }
                // A stage for each display, as wide as the pen, with a cast
                // to fit the display's own filters.
                for (i, d) in sp.beh.displays.iter().enumerate() {
                    let Some(members) = super::display::zoo_cast(d) else { continue };
                    pen.rows.push((format!("display {}", d.kind), dz));
                    let deep = 4.0;
                    let group = zoo.next_group;
                    zoo.next_group += 1;
                    let mid = Vec2::new(cols.len() as f32 * cell * 0.5, dz + deep * cell * 0.5);
                    let n = members.len();
                    for (k, (age, sex)) in members.into_iter().enumerate() {
                        let stage = match age.as_deref() {
                            Some("juvenile") if sp.ages.has_juvenile() => Stage::Juvenile,
                            Some("aged") if sp.ages.aged => Stage::Aged,
                            _ => Stage::Adult,
                        };
                        let sex = match sex.as_deref() {
                            Some("male") => Sex::Male,
                            Some("female") => Sex::Female,
                            _ => [Sex::Male, Sex::Female][k % 2],
                        };
                        // In a ring round the stage's middle; the lead first.
                        let a = k as f32 / n.max(1) as f32 * std::f32::consts::TAU;
                        let r = if n > 1 { cell * 0.8 } else { 0.0 };
                        let at = mid + Vec2::new(a.cos(), a.sin()) * r;
                        pen.slots.push(Slot { at, to: at, stage, sex, lock: Lock::Display(i), leash: 0.0, group });
                    }
                    dz += deep * cell;
                }
                pen.size = Vec2::new(cols.len() as f32 * cell, dz);
            } else {
                // Anything else: a patch to put a group down on.
                pen.size = Vec2::splat((cell * 4.0).max(16.0));
                pen.rows.push(("free".into(), 0.0));
            }
            if x + pen.size.x > LAND_X0 + land_w && x > LAND_X0 {
                x = LAND_X0;
                z += row_d + PATH_M;
                row_d = 0.0;
            }
            pen.min = Vec2::new(x, z);
            for r in pen.rows.iter_mut() {
                r.1 += z;
            }
            x += pen.size.x + PATH_M;
            row_d = row_d.max(pen.size.y);
            zoo.pens.push(pen);
        }
        // VG_ZOO_LOG=1: where each pen is, to aim a headless camera.
        if std::env::var_os("VG_ZOO_LOG").is_some() {
            for (i, p) in zoo.pens.iter().enumerate() {
                eprintln!("zoo pen {i}: {} at {:.0},{:.0} size {:.0}x{:.0}, {} slots", species[p.species].name, p.min.x, p.min.y, p.size.x, p.size.y, p.slots.len());
            }
        }
        if z + row_d > sea_z - 30.0 {
            eprintln!("zoo: the land pens run to z {:.0}, into the shore at {:.0}; make the world bigger", z + row_d, sea_z - 30.0);
        }
        zoo
    }

    /// The pen the player stands in or beside.
    pub fn pen_at(&self, p: Vec3) -> Option<&Pen> {
        let q = Vec2::new(p.x, p.z);
        self.pens.iter().find(|pen| pen.contains(q))
    }

    /// The pen the player is in, or the nearest.
    pub fn nearest(&self, p: Vec3) -> Option<usize> {
        let q = Vec2::new(p.x, p.z);
        (0..self.pens.len()).min_by(|&a, &b| self.pens[a].dist(q).total_cmp(&self.pens[b].dist(q)))
    }

    /// Where to stand to look at pen `i`: on the path at its top-left
    /// corner, and the point to look at (the middle of its top rows).
    pub fn viewpoint(&self, i: usize) -> Option<(Vec2, Vec2)> {
        let p = self.pens.get(i)?;
        let look = p.min + Vec2::new(p.size.x * 0.5, p.size.y.min(p.size.x).min(30.0) * 0.5);
        Some((p.min + Vec2::new(-PATH_M * 0.4, -PATH_M * 0.4), look))
    }

    /// Fill the pens near the player, empty the far ones, and put back
    /// anything that has gone (a kill, a fall into the sea).
    #[allow(clippy::too_many_arguments)]
    pub fn populate(
        &mut self,
        world: &mut hecs::World,
        species: &[Species],
        models: &[Model],
        rng: &mut Rng,
        land: &crate::world::fields::World,
        store: &Store,
        plants: &super::plants::Flora,
        player: Vec3,
        now: f32,
        dt: f32,
    ) {
        let me = Vec2::new(player.x, player.z);
        for (pi, pen) in self.pens.iter_mut().enumerate() {
            let d = pen.dist(me);
            if d > EMPTY_M {
                if pen.live {
                    for e in pen.ents.drain(..).flatten() {
                        let _ = world.despawn(e);
                    }
                    pen.live = false;
                }
                continue;
            }
            if d > FILL_M && !pen.live {
                continue;
            }
            pen.live = true;
            if pen.walkers {
                pen.ents.resize(pen.slots.len(), None);
                for (k, slot) in pen.slots.iter().enumerate() {
                    if pen.ents[k].is_some_and(|e| world.contains(e)) {
                        continue;
                    }
                    pen.ents[k] = walker(world, species, models, store, pen.species, pi as u32, pen.min, slot, rng);
                }
                continue;
            }
            // Any other mover: one group, put down again when all of it has
            // gone.
            pen.ents.retain(|e| e.is_some_and(|e| world.contains(e)));
            pen.retry -= dt;
            if !pen.ents.is_empty() || pen.retry > 0.0 {
                continue;
            }
            pen.retry = 3.0;
            let mid = pen.min + pen.size * 0.5;
            let s = pen.species;
            let size = species[s].beh.group.as_ref().map_or(1, |g| g.size[1].max(1)).min(12);
            // On land, the pen's middle. At the sea, the shore under the
            // strip, then out from it: a seal or a wader on the bank, fish
            // and ducks a little way out, whales further.
            let spots: Vec<Vec2> = if ground_of(&species[s]) == Ground::Land {
                vec![mid]
            } else {
                let f = &land.f;
                let shore = (0..400).map(|k| pen.min.y + k as f32).find(|&z| f.height(mid.x, z) < f.sea).unwrap_or(mid.y);
                let walks = matches!(species[s].verb().mover, Mover::Walk { .. } | Mover::Run { .. });
                let out: &[f32] = if walks { &[-3.0, -6.0, -1.5] } else { &[6.0, 14.0, 30.0, 60.0, 110.0, 170.0] };
                out.iter().map(|o| Vec2::new(mid.x, shore + o)).collect()
            };
            if !spots.into_iter().any(|p| super::spawn::place(world, species, s, rng, land, store, models, plants, (p.x, p.y), size, now, None, false)) {
                continue;
            }
            super::spawn::dress(world, species, s, None, rng);
            let fresh: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().without::<&Kept>().iter().map(|(e, _)| e).collect();
            for e in fresh {
                let pos = world.get::<&Look>(e).map_or(Vec3::new(mid.x, 0.0, mid.y), |l| l.pos);
                let sex = Sex::roll(e.to_bits().get() as u32);
                let _ = world.insert_one(
                    e,
                    Kept {
                        pen: pi as u32,
                        home: pos,
                        to: pos,
                        yaw: EAST_YAW,
                        lock: Lock::Free,
                        leash: 0.0,
                        sex,
                        outward: true,
                        group: 0,
                    },
                );
                pen.ents.push(Some(e));
            }
        }
    }

    /// Start each display stage's display again a little after it ends.
    pub fn restage(&mut self, world: &mut hecs::World, shows: &mut super::display::Shows, w: &super::display::World, dt: f32) {
        // Leads of stages that are idle and not yet waiting.
        let leads: Vec<(hecs::Entity, usize)> = world
            .query::<(hecs::Entity, &Kept, Option<&super::display::Show>)>()
            .iter()
            .filter(|(_, k, s)| s.is_none() && matches!(k.lock, Lock::Display(_)))
            .filter_map(|(e, k, _)| match k.lock {
                Lock::Display(i) => Some((e, i, k.group)),
                _ => None,
            })
            // One per stage: the first found.
            .fold(Vec::<(hecs::Entity, usize, u32)>::new(), |mut v, x| {
                if !v.iter().any(|y| y.2 == x.2) {
                    v.push(x);
                }
                v
            })
            .into_iter()
            // Not while any of its cast is still in a show.
            .filter(|&(_, _, g)| !world.query::<(&Kept, &super::display::Show)>().iter().any(|(k, _)| k.group == g))
            .map(|(e, i, _)| (e, i))
            .collect();
        for (e, i) in leads {
            if !self.restage.iter().any(|r| r.0 == e) {
                self.restage.push((e, i, RESTAGE_S));
            }
        }
        let mut i = 0;
        while i < self.restage.len() {
            self.restage[i].2 -= dt;
            let (e, d, t) = self.restage[i];
            if !world.contains(e) {
                self.restage.swap_remove(i);
                continue;
            }
            if t <= 0.0 {
                self.restage.swap_remove(i);
                // Only once the cast has walked home.
                let home = world.get::<&Kept>(e).ok().map(|k| k.home);
                let at = world.get::<&Look>(e).ok().map(|l| l.pos);
                if let (Some(h), Some(p)) = (home, at) {
                    if Vec2::new(h.x - p.x, h.z - p.z).length() < 1.5 {
                        shows.stage(world, w, e, d);
                        continue;
                    }
                }
                self.restage.push((e, d, 1.0));
                continue;
            }
            i += 1;
        }
    }
}

/// Put one walker in its slot. None while the ground there has not loaded.
#[allow(clippy::too_many_arguments)]
fn walker(world: &mut hecs::World, species: &[Species], models: &[Model], store: &Store, s: usize, pen: u32, min: Vec2, slot: &Slot, rng: &mut Rng) -> Option<hecs::Entity> {
    let md = &models[species[s].model];
    let at = min + slot.at;
    let to = min + slot.to;
    let (gy, _) = super::terrain::voxel_ground(store, at.x, at.y, store.top_m())?;
    let gt = super::terrain::voxel_ground(store, to.x, to.y, store.top_m()).map_or(gy, |g| g.0);
    let pos = Vec3::new(at.x, gy + 0.01, at.y);
    let seed = hash32(rng.state ^ s as u32 ^ (at.x.to_bits() ^ at.y.to_bits().rotate_left(7)));
    rng.unit();
    let side = if seed & 1 == 0 { 1.0 } else { -1.0 };
    let e = world.spawn((
        Critter::new(EAST_YAW + std::f32::consts::PI, 1.0, seed, side),
        Creature::of(s),
        Body::new(md.half_w, md.height),
        Look::new(species[s].model, pos, EAST_YAW),
        super::age::Age::new(slot.stage),
        Kept {
            pen,
            home: pos,
            to: Vec3::new(to.x, gt + 0.01, to.y),
            yaw: EAST_YAW,
            lock: slot.lock,
            leash: slot.leash,
            sex: slot.sex,
            outward: true,
            group: slot.group,
        },
    ));
    if slot.group != 0 {
        let _ = world.insert_one(e, super::herd::Kin { group: slot.group });
    }
    Some(e)
}
