//! Keeping creatures around the player. In a chronicle world each biome
//! within reach lists creatures and how many to keep; missing ones turn up
//! 20 to 90 metres away, handed to their verb's mover, and ones that wander
//! far off are dropped. The legacy island keeps its own three animals and a
//! few gulls.

use super::age::{Age, Stage};
use super::circle::Gull;
use super::herd::Kin;
use super::locomotion::Mover;
use super::terrain::{voxel_ground, water_voxel};
use super::walk::Critter;
use super::{census, flit, flock, swim, Coat, Creature, Look, Species};
use crate::models::{self, Model};
use crate::physics::Body;
use crate::util::FxHashMap as HashMap;
use crate::world::fields::World as Land;
use crate::world::gen::Generator;
use crate::world::material::GRASS;
use crate::world::noise::{hash32, Rng};
use crate::world::store::Store;
use glam::Vec3;

/// Each new individual rolls its own size, coat and habits once
/// (src/variety.rs), then has its age laid over that roll (age.rs).
pub fn roll_morphs(world: &mut hecs::World, species: &[Species], models: &[Model], rng: &Rng) {
    let unrolled: Vec<(hecs::Entity, usize, usize, Stage, Option<Coat>, Option<super::sex::Sex>)> = world
        .query::<(hecs::Entity, &Creature, Option<&crate::variety::Morph>, Option<&Age>, Option<&Coat>, Option<&super::zoo::Kept>)>()
        .iter()
        .filter(|(_, _, m, _, _, _)| m.is_none())
        .map(|(e, c, _, a, coat, k)| (e, c.species, c.spawned_for, a.map_or(Stage::Adult, |a| a.stage), coat.copied(), k.map(|k| k.sex)))
        .collect();
    for (e, s, parent, stage, coat, kept_sex) in unrolled {
        let bits = e.to_bits().get();
        let seed = hash32(bits as u32 ^ (bits >> 32) as u32 ^ rng.state);
        // A rare morph stands in for one of its parent's spawns and keeps its
        // place in the biome's quota (`Creature::spawned_for`): the pale mare
        // among the horses, the white bear, the great skate among the small
        // ones. Walkers draw theirs as a group is placed, so each member of a
        // herd rolls on its own and `species` already differs from
        // `spawned_for`; every other way of moving draws here, so circlers,
        // flutterers, flocks, swimmers and the rest are not left out.
        // The zoo (zoo.rs) pens each rare morph on its own, so one of its
        // animals is never swapped for another.
        let s = if s == parent && kept_sex.is_none() {
            crate::variety::pick(&species[s].morphs, crate::world::noise::unit(seed ^ 0x51ed_9a3b)).unwrap_or(s)
        } else {
            s
        };
        if s != parent {
            if let Ok(mut c) = world.get::<&mut Creature>(e) {
                c.species = s;
            }
            if let Ok(mut l) = world.get::<&mut Look>(e) {
                l.model = species[s].model;
            }
            let md = &models[species[s].model];
            if let Ok(mut b) = world.get::<&mut crate::physics::Body>(e) {
                b.half_w = md.half_w;
                b.height = md.height;
            }
        }
        let sp = &species[s];
        let mut m = crate::variety::Morph::roll(&sp.vary, seed);
        // The coat its group was dressed in (`dress`). A rare morph that
        // stood in above is a different entry with coats of its own, so the
        // group's choice may not reach: it then wears its own plain colours.
        if let Some(w) = coat.and_then(|c| sp.coats.get(c.which).map(|w| w.at(c.at))) {
            m.wear(&w);
        }
        m.age(stage, &sp.ages, sp.aged_tint);
        // A juvenile is smaller all through, its collision box included.
        if stage == Stage::Juvenile {
            if let Ok(mut b) = world.get::<&mut crate::physics::Body>(e) {
                b.half_w *= m.scale;
                b.height *= m.scale;
            }
        }
        let _ = world.insert_one(e, m);
        let sex = kept_sex.unwrap_or_else(|| super::sex::Sex::roll(seed));
        let _ = world.insert_one(e, sex);
        // Its sex and age may draw it with another model (library::Forms):
        // a hind without antlers, a lioness, a cub.
        let model = sp.model_for(Some(sex), stage);
        if let Ok(mut l) = world.get::<&mut Look>(e) {
            l.model = model;
        }
    }
}

/// Clear away what has had its time: bones that have lain half a day, and
/// carcasses nothing came back for. The spawner refills the slot.
pub fn drop_expired(world: &mut hecs::World, now: f32) {
    let done: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Age)>().iter().filter(|(_, a)| a.until > 0.0 && now >= a.until).map(|(e, _)| e).collect();
    for e in done {
        let _ = world.despawn(e);
    }
}

/// One of the random island's animals. They have no library entry, which is
/// why everything about them is this table rather than a species: the model
/// they wear tells them apart (`--world random` only).
pub struct Legacy {
    pub model: usize,
    pub name: &'static str,
    /// How many to keep near the player.
    pub want: usize,
    /// What a species' behaviour would say: how far off it minds the player,
    /// how fast it runs, how fast it walks, and whether it hops.
    pub wary: f32,
    pub fast: f32,
    pub slow: f32,
    pub hops: bool,
}

pub const LEGACY: [Legacy; 3] = [
    Legacy {
        model: models::DEER_M,
        name: "deer",
        want: 7,
        wary: 10.0,
        fast: 7.5,
        slow: 1.3,
        hops: false,
    },
    Legacy {
        model: models::RABBIT_M,
        name: "rabbit",
        want: 12,
        wary: 5.0,
        fast: 5.5,
        slow: 1.0,
        hops: true,
    },
    Legacy {
        model: models::FOX_M,
        name: "fox",
        want: 5,
        wary: 7.0,
        fast: 6.5,
        slow: 1.6,
        hops: false,
    },
];

/// The random island's animal drawn with `model`, if it is one.
pub fn legacy(model: usize) -> Option<&'static Legacy> {
    LEGACY.iter().find(|l| l.model == model)
}

/// The random island: deer, rabbits and foxes on grass near the player.
pub fn legacy_animals(world: &mut hecs::World, rng: &mut Rng, store: &Store, sea: f32, player: Vec3, models: &[Model]) {
    // Remove creatures that wandered far away.
    let far: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Critter, &Look)>()
        .iter()
        .filter(|(_, _, l)| (l.pos - player).length() > 130.0 || l.pos.y < 0.0)
        .map(|(e, _, _)| e)
        .collect();
    for e in far {
        let _ = world.despawn(e);
    }
    // Counted by the model standing there, since the island's three animals
    // are told apart by nothing else.
    let mut counts = [0usize; LEGACY.len()];
    for (_, look) in world.query::<(&Critter, &Look)>().iter() {
        if let Some(i) = LEGACY.iter().position(|l| l.model == look.model) {
            counts[i] += 1;
        }
    }
    for (i, want) in LEGACY.iter().enumerate() {
        if counts[i] >= want.want {
            continue;
        }
        for _ in 0..4 {
            let a = rng.unit() * std::f32::consts::TAU;
            let r = 20.0 + rng.unit() * 70.0;
            let (x, z) = (player.x + a.cos() * r, player.z + a.sin() * r);
            let Some((gy, m)) = voxel_ground(store, x, z, store.top_m()) else {
                continue;
            };
            if m != GRASS || gy < sea + 1.0 {
                continue;
            }
            let model = want.model;
            let md = &models[model];
            let pos = Vec3::new(x, gy + 0.01, z);
            let yaw = rng.unit() * std::f32::consts::TAU;
            let seed = hash32(rng.state ^ 0xabcdef);
            let timer = rng.unit() * 2.0;
            world.spawn((Critter::new(yaw, timer, seed, 1.0), Body::new(md.half_w, md.height), Look::new(model, pos, yaw)));
            break;
        }
    }
}

/// The legacy island's gulls over the nearest stretch of sea, placed once.
pub fn legacy_gulls(world: &mut hecs::World, rng: &mut Rng, gen: &Generator, player: Vec3) {
    let sea = gen.spec.sea_level_m;
    let mut placed = 0;
    for i in 0..64 {
        let a = i as f32 * 0.7;
        let r = 30.0 + (i as f32) * 3.0;
        let (x, z) = (player.x + a.cos() * r, player.z + a.sin() * r);
        if gen.height(x, z) < sea - 1.0 {
            let n = 2 + (rng.unit() * 2.0) as usize;
            for _ in 0..n {
                let centre = Vec3::new(x, sea + 9.0 + rng.unit() * 10.0, z);
                let radius = 8.0 + rng.unit() * 14.0;
                let speed = (6.0 + rng.unit() * 3.0) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
                let angle = rng.unit() * std::f32::consts::TAU;
                world.spawn((Gull::new(centre, radius, angle, speed), Look::new(models::GULL_M, Vec3::ZERO, 0.0)));
            }
            placed += 1;
            if placed >= 3 {
                break;
            }
        }
    }
}

/// Past this an animal is dropped once it is out of view, metres.
pub const DROP_M: f32 = 130.0;
/// Past this it is dropped even in view: further than the fog lets anything
/// be made out on most days, metres.
const DROP_SEEN_M: f32 = 360.0;
/// How far a herd or pack kept as a record (roster.rs) is drawn in view,
/// metres, and a large one (bison, mammoths); it dissolves out over the last
/// `EDGE_SHARE` of it.
pub const HERD_SEEN_M: f32 = 250.0;
pub const HERD_SEEN_LARGE_M: f32 = 350.0;
/// `HERD_SEEN_M` and `HERD_SEEN_LARGE_M`, or what `VG_HERD_REACH=seen,large`
/// says (a dev hook, for measuring what a longer reach costs).
fn herd_seen() -> (f32, f32) {
    static R: std::sync::OnceLock<(f32, f32)> = std::sync::OnceLock::new();
    *R.get_or_init(|| {
        let v: Vec<f32> = std::env::var("VG_HERD_REACH").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
        match v[..] {
            [a, b] => (a, b),
            _ => (HERD_SEEN_M, HERD_SEEN_LARGE_M),
        }
    })
}

/// Whales are watched from far off, so they go further first.
const DROP_WHALE_M: f32 = 260.0;
const DROP_WHALE_SEEN_M: f32 = 600.0;
/// The furthest from the player an ordinary animal is set down, metres.
pub const SPAWN_M: f32 = 90.0;
/// Where a small animal (not `Species::seen_far`) may be set down in view,
/// metres: further out than behind the player, a few pixels across, where
/// it dissolves in unremarked (`APPEAR_S`).
const SPAWN_SEEN_M: (f32, f32) = (60.0, 110.0);
/// The nearest a walker big enough to make out from far off
/// (`Species::seen_far`) is set down in view, by its size
/// (`Species::span`), metres: a badger at 175, a deer or a horse at 225, a
/// mammoth and up at 300, and in between in step with the size. Nearer
/// than this it only ever turns up out of view, so the far band is where
/// the plain ahead gets its animals (`far_band_m`).
pub const FAR_START: [(f32, f32); 3] = [(0.4, 175.0), (1.5, 225.0), (4.1, 300.0)];
/// How deep the far band runs past its nearest, metres.
const FAR_DEPTH_M: f32 = 200.0;
/// How far such a walker is kept in view, metres; it dissolves out over the
/// last `EDGE_SHARE` of it.
pub const DROP_FAR_SEEN_M: f32 = 500.0;
/// The far band's numbers, as a share of what the biomes out there list:
/// its ground is about seven times the 90 m round the player, but each
/// animal is a speck and every one is stepped like any other.
const DISTANT_QUOTA: f32 = 2.0;
/// The most animals in the far band at once.
const DISTANT_MAX: usize = 60;
/// A distant animal this near the player joins the ones about them and
/// counts toward its biome's quota, metres: nearer than any of the far
/// band is set down.
pub const JOIN_M: f32 = 150.0;

/// Where in view walker `sp` is set down in the far band, metres: from
/// its size's place in `FAR_START`, `FAR_DEPTH_M` deep.
pub fn far_band_m(sp: &Species, models: &[Model]) -> (f32, f32) {
    let size = sp.span(models);
    let near = match FAR_START.iter().position(|&(s, _)| size < s) {
        Some(0) => FAR_START[0].1,
        Some(k) => {
            let ((s0, d0), (s1, d1)) = (FAR_START[k - 1], FAR_START[k]);
            d0 + (d1 - d0) * (size - s0) / (s1 - s0)
        }
        None => FAR_START[FAR_START.len() - 1].1,
    };
    (near, near + FAR_DEPTH_M)
}
/// How long a newcomer takes to dissolve into view, seconds.
pub const APPEAR_S: f32 = 0.6;
/// The share of its in-view reach (`reach`) over which an animal walking
/// away dissolves out before it is dropped, so it does not wink out.
pub const EDGE_SHARE: f32 = 0.1;

/// When a creature was set down (`Entities::clock`), for dissolving it into
/// view (entities.rs `instances`).
pub struct Appear(pub f32);

/// On an animal of the far band (`far_band_m`): counted apart from the
/// biomes' quotas (census.rs `quota`) until it comes within `JOIN_M`.
pub struct Distant;
/// How much further off a large animal (`Species::large`: elephants,
/// bison, the big dinosaurs) is found, kept and set down than the rest, so
/// a herd can be watched from a ridge across a valley. Past full detail
/// (stream.rs `FULL_DETAIL_M`) it walks the generator's ground instead of
/// the voxels (walk.rs `far`).
pub const LARGE_REACH: f32 = 2.0;
/// The share more of a large species the biomes want, since its numbers
/// are spread over four times the ground. Half as many again rather than
/// four times, because each is a rigged model drawn in full: a Thunderfield
/// view holds about seventy large animals.
const LARGE_QUOTA: f32 = 1.5;
/// How long after the world opens a large animal may be set down in view:
/// the first fill, so the plain below a lookout has its herds on arrival.
/// After that newcomers turn up behind the player like everything else.
const FIRST_FILL_S: f32 = 3.0;

/// How far from the player an animal of `sp` is kept, metres: out of view,
/// and in view (past that it is dropped even while watched).
pub fn reach(sp: &Species, models: &[Model]) -> (f32, f32) {
    if sp.verb().mover == Mover::Breach {
        (DROP_WHALE_M, DROP_WHALE_SEEN_M)
    } else if super::roster::recorded(sp) {
        // Herds and packs kept as records (roster.rs) are drawn nearer than
        // the old far band: a herd at 250 m reads as a herd, and past that
        // only the big animals are worth drawing.
        let (seen, seen_large) = herd_seen();
        if sp.large(models) {
            (DROP_M * LARGE_REACH, seen_large)
        } else {
            (DROP_M, seen)
        }
    } else if sp.large(models) {
        (DROP_M * LARGE_REACH, DROP_SEEN_M * LARGE_REACH)
    } else if sp.seen_far(models) {
        (DROP_M, DROP_FAR_SEEN_M)
    } else {
        (DROP_M, DROP_SEEN_M)
    }
}

/// How much of a creature is dissolved away, 0 to 1: coming in for
/// `APPEAR_S` after it was set down (`since`, seconds; none for one that
/// was always there), and going out over the last `EDGE_SHARE` of its
/// in-view reach `seen` at distance `d`.
pub fn dissolved(since: Option<f32>, d: f32, seen: f32) -> f32 {
    let edge = ((seen - d) / (seen * EDGE_SHARE)).clamp(0.0, 1.0);
    let t = since.map_or(1.0, |t| (t / APPEAR_S).clamp(0.0, 1.0));
    let born = t * t * (3.0 - 2.0 * t);
    1.0 - edge * born
}

/// Whether the player may be looking at `p`: within 75 degrees either side
/// of where they face (a wide screen shows about 50), across the ground.
/// With no facing known everything counts as in view.
pub fn in_view(p: Vec3, player: Vec3, facing: Vec3) -> bool {
    let f = Vec3::new(facing.x, 0.0, facing.z);
    let d = Vec3::new(p.x - player.x, 0.0, p.z - player.z);
    if f.length_squared() < 1e-4 {
        return true;
    }
    d.length_squared() < 1.0 || d.normalize().dot(f.normalize()) > 0.26
}

/// Drop creatures that wandered far from the player, once they are out of
/// view: a bird or an elephant does not wink out on the skyline while it is
/// watched, only behind the player's back (or very far off).
pub fn drop_far(world: &mut hecs::World, species: &[Species], models: &[Model], player: Vec3, facing: Vec3, tracked: &mut super::tracked::Tracked) {
    // A recorded herd's animals are written back by its record (roster.rs).
    let far: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Look, &Creature)>()
        .without::<(&super::roster::Recorded, &Critter)>()
        .iter()
        .filter(|(_, l, c)| {
            if l.pos.y < 0.0 {
                return true;
            }
            let (near, seen) = reach(&species[c.species], models);
            let d = Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length();
            d > seen || d > near && !in_view(l.pos, player, facing)
        })
        .map(|(e, _, _)| e)
        .collect();
    for e in far {
        // A large animal is remembered, to be put back if the player
        // returns (tracked.rs); the rest are made up again.
        if world.get::<&Creature>(e).is_ok_and(|c| species[c.species].large(models)) && world.get::<&Distant>(e).is_err() {
            tracked.remember(world, e);
        }
        let _ = world.despawn(e);
    }
}

/// Bring in what the biomes around the player want and don't have yet.
#[allow(clippy::too_many_arguments)]
pub fn world_creatures(
    world: &mut hecs::World,
    species: &[Species],
    rng: &mut Rng,
    land: &Land,
    store: &Store,
    player: Vec3,
    facing: Vec3,
    models: &[Model],
    plants: &super::plants::Flora,
    now: f32,
    hour: f32,
    sky: &crate::world::weather::Sky,
    phase: &crate::world::season::Phase,
    tracked: &mut super::tracked::Tracked,
) {
    let f = &land.f;
    drop_far(world, species, models, player, facing, tracked);
    // A distant animal the player has come up to is one of theirs now.
    let joined: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Look)>()
        .with::<&Distant>()
        .iter()
        .filter(|(_, l)| Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length() < JOIN_M)
        .map(|(e, _)| e)
        .collect();
    for e in joined {
        let _ = world.remove_one::<Distant>(e);
    }
    // What is here already: anything else at the end was set down now, and
    // dissolves in (`Appear`).
    let before: std::collections::HashSet<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).collect();
    // The large animals the player is coming back to, where they were left,
    // before anything new is asked for: they fill their biomes' numbers.
    tracked.restore(world, store, player);
    // Under a roof the player is in a room the land was laid over -- a
    // buried arena, a cave -- and newcomers are looked for on its floor,
    // not on the land overhead (#377).
    let indoors = super::terrain::roof_over(store, player).map(|_| player);
    // What the biomes around the player want.
    let mut want: HashMap<usize, f32> = HashMap::default();
    let mut in_cave: std::collections::HashSet<usize> = Default::default();
    for k in 0..9 {
        let p = if k == 0 {
            player
        } else {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            player + Vec3::new(a.cos(), 0.0, a.sin()) * 45.0
        };
        let b = &land.lib.biomes[f.biome[f.idx(p.x, p.z)] as usize];
        let list = if indoors.is_some() && !b.cave_creatures.is_empty() { &b.cave_creatures } else { &b.creatures };
        for cu in list {
            // Herds that are kept as records (roster.rs) are not made up here.
            if let Some(s) = species.iter().position(|x| x.name == cu.creature && !super::roster::recorded(x)) {
                let e = want.entry(s).or_insert(0.0);
                *e = e.max(cu.count);
                if indoors.is_some() && !b.cave_creatures.is_empty() {
                    in_cave.insert(s);
                }
            }
        }
    }
    // Large animals are wanted by the biomes out to their own reach too:
    // a herd on the plain below a lookout belongs to the plain's biome, not
    // the cliff top's. Two rings: half way out to where a large animal is
    // set down at the furthest, and all the way.
    for share in [0.5, 1.0] {
        let ring = SPAWN_M * LARGE_REACH * share;
        for k in 0..12 {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            let p = player + Vec3::new(a.cos(), 0.0, a.sin()) * ring;
            let b = &land.lib.biomes[f.biome[f.idx(p.x, p.z)] as usize];
            for cu in &b.creatures {
                if let Some(s) = species.iter().position(|x| x.name == cu.creature && x.large(models) && !super::roster::recorded(x)) {
                    let e = want.entry(s).or_insert(0.0);
                    *e = e.max(cu.count);
                }
            }
        }
    }
    for (s, n) in want.iter_mut() {
        if species[*s].large(models) {
            *n *= LARGE_QUOTA;
        }
    }
    // Dev hook: VG_SPAWN=name,name puts one of each named creature (a group
    // for flockers and schools) about VG_SPAWN_DIST m (default 4) from the
    // player, or the nearest spot further out that suits it, retrying each
    // half second until the ground there has loaded, for testing behaviour
    // headless.
    static PENDING: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);
    if let Ok(mut pending) = PENDING.lock() {
        let list = pending.get_or_insert_with(|| {
            std::env::var("VG_SPAWN")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default()
        });
        let dist: f32 = std::env::var("VG_SPAWN_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
        let mut k = 0;
        list.retain(|name| {
            k += 1;
            let Some(s) = species.iter().position(|x| &x.name == name) else {
                return false;
            };
            let size = sp_group(&species[s]);
            for ring in 0..8 {
                let r = dist + ring as f32 * 6.0;
                for t in 0..12 {
                    let a = (k * 5 + t) as f32 * 0.9 + k as f32 * 0.37;
                    let (x, z) = (player.x + a.cos() * r, player.z + a.sin() * r);
                    let land_ok = store.top_m() > 0.0 && (voxel_ground(store, x, z, store.top_m()).is_some() || species[s].large(models) && super::terrain::far_ground(store, x, z).is_some());
                    if !land_ok {
                        continue;
                    }
                    if place(world, species, s, rng, land, store, models, plants, (x, z), size, now, indoors, false) {
                        dress(world, species, s, biome_pick(land, x, z, &species[s].name), rng);
                        let at: Vec<String> = world
                            .query::<(&Look, &Creature)>()
                            .iter()
                            .filter(|(_, c)| c.species == s)
                            .map(|(l, _)| format!("{:.1},{:.1},{:.1}", l.pos.x, l.pos.y, l.pos.z))
                            .collect();
                        eprintln!("VG_SPAWN: {name} x{size} near {x:.0},{z:.0}; now at {}", at.join(" "));
                        return false;
                    }
                }
            }
            true
        });
    }
    // The clock thins each species down to what is abroad at this hour
    // (diel.rs): the ones that sleep where they stand mostly stay, bedded
    // down, and the rest -- birds, insects, fish -- are simply not out.
    let square = (player.x / 128.0).floor() as i32 as u32 ^ ((player.z / 128.0).floor() as i32 as u32).wrapping_mul(0x9e37_79b9);
    // In a cave it is always night: its creatures are out whatever the hour.
    let mut order: Vec<(usize, u32)> = want
        .into_iter()
        .map(|(s, n)| (s, if in_cave.contains(&s) { whole(n, s, square) } else { super::diel::quota(whole(n, s, square), abroad(&species[s], hour, sky, phase)) }))
        .collect();
    order.sort();
    cull_off_hours(world, species, player, facing, &order, hour, sky, phase);
    far_band(world, species, rng, land, store, models, plants, player, facing, now, square, hour, sky, phase, indoors.is_some());
    let have = census::quota(world);
    for (s, n) in order {
        if have.get(&s).copied().unwrap_or(0) >= n {
            continue;
        }
        let sp = &species[s];
        if sp.beh.spawn_chance < 1.0 && rng.unit() >= sp.beh.spawn_chance {
            continue;
        }
        let mover = sp.verb().mover;
        let whale = mover == Mover::Breach;
        let large = sp.large(models);
        let seen_far = sp.seen_far(models);
        // A group never brings the count past what the biomes want.
        let room = n - have.get(&s).copied().unwrap_or(0);
        // Newcomers turn up all round, but in view only small ones, further
        // out, where they dissolve in over `APPEAR_S` (entities.rs
        // `instances`); the bigger walkers ahead come from the far band
        // (`far_band`). On the first fill (`FIRST_FILL_S`) anything goes
        // anywhere, so a view opens with its animals in it.
        let looking = facing.length_squared() > 0.01 && now >= FIRST_FILL_S;
        for _ in 0..4 {
            let a = rng.unit() * std::f32::consts::TAU;
            let dir = Vec3::new(a.cos(), 0.0, a.sin());
            let seen = looking && in_view(player + dir, player, facing);
            if seen && seen_far {
                continue;
            }
            // Fish turn up nearer: under water the fog hides anything past
            // a few visibilities, and a reef's clouds stay where they are
            // born, so 20-90 m out most of them would never be met.
            let fish = mover == Mover::School;
            let (lo, hi) = if whale {
                (if seen { 100.0 } else { 50.0 }, 140.0)
            } else if large {
                // Out to twice as far, and seldom right on top of the
                // player: a herd is better met across open ground.
                (30.0, SPAWN_M * LARGE_REACH)
            } else if fish {
                (if seen { 30.0 } else { 12.0 }, 50.0)
            } else if seen {
                SPAWN_SEEN_M
            } else {
                (20.0, SPAWN_M)
            };
            let r = lo + rng.unit() * (hi - lo);
            let (x, z) = (player.x + dir.x * r, player.z + dir.z * r);
            let i = f.idx(x, z);
            let lists = land.lib.biomes[f.biome[i] as usize].creatures.iter().any(|c| c.creature == sp.name);
            if !lists {
                continue;
            }
            let group = sp.beh.group.clone().unwrap_or_default();
            let size = (group.size[0] + (rng.unit() * (group.size[1].saturating_sub(group.size[0]) + 1) as f32) as u32).min(room);
            let placed = place(world, species, s, rng, land, store, models, plants, (x, z), size, now, indoors, false);
            if placed {
                dress(world, species, s, biome_pick(land, x, z, &sp.name), rng);
                break;
            }
        }
    }
    let new: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).filter(|e| !before.contains(e)).collect();
    for e in new {
        let _ = world.insert_one(e, Appear(now));
    }
}

/// Set down walkers big enough to make out from far off
/// (`Species::seen_far`) out ahead of the player, `far_band_m` away, where
/// they dissolve in (`Appear`) as specks on the plain. They are counted
/// apart (`Distant`) up to what the biomes out there list, by
/// `DISTANT_QUOTA` and the hour, until the player comes up to one.
#[allow(clippy::too_many_arguments)]
fn far_band(
    world: &mut hecs::World,
    species: &[Species],
    rng: &mut Rng,
    land: &Land,
    store: &Store,
    models: &[Model],
    plants: &super::plants::Flora,
    player: Vec3,
    facing: Vec3,
    now: f32,
    square: u32,
    hour: f32,
    sky: &crate::world::weather::Sky,
    phase: &crate::world::season::Phase,
    indoors: bool,
) {
    if indoors || facing.length_squared() < 0.01 {
        return;
    }
    let f = &land.f;
    let ahead = facing.z.atan2(facing.x);
    // What the biomes out ahead list, across the view and the band.
    let mut want: HashMap<usize, f32> = HashMap::default();
    for k in 0..7 {
        let a = ahead + (k as f32 - 3.0) * 0.35;
        for r in [FAR_START[0].1 + 50.0, FAR_START[2].1 + 50.0, FAR_START[2].1 + FAR_DEPTH_M - 50.0] {
            let p = player + Vec3::new(a.cos(), 0.0, a.sin()) * r;
            let b = &land.lib.biomes[f.biome[f.idx(p.x, p.z)] as usize];
            for cu in &b.creatures {
                if let Some(s) = species.iter().position(|x| x.name == cu.creature && x.seen_far(models) && !super::roster::recorded(x)) {
                    let e = want.entry(s).or_insert(0.0);
                    *e = e.max(cu.count);
                }
            }
        }
    }
    let mut have: HashMap<usize, u32> = HashMap::default();
    let mut total = 0;
    for c in world.query::<&Creature>().with::<&Distant>().iter() {
        *have.entry(c.spawned_for).or_insert(0) += 1;
        total += 1;
    }
    let mut order: Vec<(usize, u32)> = want.into_iter().map(|(s, n)| (s, super::diel::quota(whole(n * DISTANT_QUOTA, s, square), abroad(&species[s], hour, sky, phase)))).collect();
    order.sort();
    let before: std::collections::HashSet<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).collect();
    for (s, n) in order {
        let got = have.get(&s).copied().unwrap_or(0);
        if got >= n || total >= DISTANT_MAX {
            continue;
        }
        let sp = &species[s];
        if sp.beh.spawn_chance < 1.0 && rng.unit() >= sp.beh.spawn_chance {
            continue;
        }
        for _ in 0..4 {
            // Across the view (`in_view`'s 75 degrees either side).
            let a = ahead + (rng.unit() - 0.5) * 2.6;
            let (lo, hi) = far_band_m(sp, models);
            let r = lo + rng.unit() * (hi - lo);
            let (x, z) = (player.x + a.cos() * r, player.z + a.sin() * r);
            if !land.lib.biomes[f.biome[f.idx(x, z)] as usize].creatures.iter().any(|c| c.creature == sp.name) {
                continue;
            }
            let group = sp.beh.group.clone().unwrap_or_default();
            let size = (group.size[0] + (rng.unit() * (group.size[1].saturating_sub(group.size[0]) + 1) as f32) as u32).min(n - got);
            if place(world, species, s, rng, land, store, models, plants, (x, z), size, now, None, true) {
                dress(world, species, s, biome_pick(land, x, z, &sp.name), rng);
                total += size as usize;
                break;
            }
        }
    }
    let new: Vec<hecs::Entity> = world.query::<(hecs::Entity, &Creature)>().iter().map(|(e, _)| e).filter(|e| !before.contains(e)).collect();
    for e in new {
        let _ = world.insert_one(e, Distant);
    }
}

/// A biome's count as a whole number of animals. The fraction of a count like
/// 2.2 is one more in that share of the country, rolled once per species for
/// each 128 m square the player stands in (`square`), so the extra one comes
/// and goes as they travel rather than flickering in place.
fn whole(count: f32, s: usize, square: u32) -> u32 {
    let n = count.max(0.0);
    let roll = crate::world::noise::unit(hash32(square ^ (s as u32).wrapping_mul(0x85eb_ca6b)));
    (n + roll).floor() as u32
}

/// Whether a species sleeps where it stands rather than leaving when its
/// hours are over. A walker beds down in the grass and a snake lies up where
/// it is, so most of them are still there to be walked up on; a bird, an
/// insect or a fish that keeps hours is simply not out (diel.rs).
fn sleeps_put(sp: &Species) -> bool {
    matches!(sp.verb().mover, Mover::Walk { .. } | Mover::Slither { .. })
}

/// The share of a species' quota that is abroad at `hour`, in this weather.
/// The clock and the sky multiply: a swift in the rain at midnight is as
/// scarce as both make it.
fn abroad(sp: &Species, hour: f32, sky: &crate::world::weather::Sky, phase: &crate::world::season::Phase) -> f32 {
    let (p, off) = (sp.beh.period(), sp.beh.off_hours());
    let clock = if sleeps_put(sp) {
        super::diel::abroad_sleeping(p, off, hour)
    } else {
        super::diel::abroad(p, off, hour)
    };
    // What sleeps where it stands is still lying there in the rain, so its
    // quota thins no further than the share the hour leaves put. The year
    // thins it the same way the hour does: an insect is gone in the winter,
    // and so is anything that migrates or sleeps through it.
    let w = sky.abroad(taste(sp)) * phase.abroad(build(sp), sp.beh.winter);
    if sleeps_put(sp) {
        (clock * w).max(clock * super::diel::stays())
    } else {
        clock * w
    }
}

/// What a species minds about the weather (world/weather.rs `Taste`): the
/// default for the way it moves, with whatever its library entry sets over
/// the top. A bird's living is made in the air and a fish's is not, and that
/// is what the way it moves says.
fn taste(sp: &Species) -> crate::world::weather::Taste {
    use crate::world::weather::Taste;
    let b = &sp.beh;
    Taste::of(build(sp)).with(b.rain, b.snow, b.heat, b.wind)
}

/// What broad kind of animal this is, which is what the way it moves says: a
/// bird's living is made in the air and a fish's is not.
fn build(sp: &Species) -> crate::world::weather::Build {
    use crate::world::weather::Build;
    match sp.verb().mover {
        // A shorebird or a roadrunner makes its living on its feet.
        Mover::Walk { .. } | Mover::Slither { .. } | Mover::Run { .. } => Build::Ground,
        Mover::Buzz { .. } => Build::Insect,
        Mover::School | Mover::Breach | Mover::Creep => Build::Swimmer,
        Mover::Circle { .. } | Mover::Flock { .. } | Mover::Flit | Mover::Paddle | Mover::Cling | Mover::Hum => Build::Flier,
    }
}

/// Take away what the hour has put to bed: anything over the species' share
/// of its quota, furthest from the player first and never within `KEEP_M`, so
/// the fireflies go out over the next field rather than in your face. Only
/// species a biome round the player still lists are thinned, so an animal
/// left behind by walking out of its biome wanders off as it always has.
fn cull_off_hours(world: &mut hecs::World, species: &[Species], player: Vec3, facing: Vec3, want: &[(usize, u32)], hour: f32, sky: &crate::world::weather::Sky, phase: &crate::world::season::Phase) {
    /// Nothing is taken away nearer than this.
    const KEEP_M: f32 = 45.0;
    let have = census::quota(world);
    for &(s, n) in want {
        if abroad(&species[s], hour, sky, phase) >= 1.0 || have.get(&s).copied().unwrap_or(0) <= n {
            continue;
        }
        let mut out: Vec<(f32, hecs::Entity)> = world
            .query::<(hecs::Entity, &Look, &Creature)>()
            .iter()
            .filter(|(e, _, c)| c.spawned_for == s && world.get::<&Distant>(*e).is_err())
            .filter(|(_, l, _)| !in_view(l.pos, player, facing))
            .map(|(e, l, _)| ((l.pos - player).length(), e))
            .filter(|(d, _)| *d > KEEP_M)
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        let surplus = (have.get(&s).copied().unwrap_or(0) - n) as usize;
        for (_, e) in out.into_iter().take(surplus) {
            let _ = world.despawn(e);
        }
    }
}

/// Put one coat on everything just placed: every library animal that has not
/// been dressed yet. One draw for the whole group, so a herd of elk is of a
/// piece rather than a scatter of colours, and the biome has its say, since
/// this runs where the group was placed and the biome is known there.
///
/// Called after each `place`, so anything still undressed came from it.
pub(crate) fn dress(world: &mut hecs::World, species: &[Species], s: usize, pick: Option<&crate::library::MorphPick>, rng: &mut Rng) {
    let coats = &species[s].coats;
    if coats.is_empty() {
        return;
    }
    // A biome's pick stands in for the entry's own weights: one coat by name,
    // or weights of its own over the coats it names.
    let shares: Vec<f32> = match pick {
        None => coats.iter().map(|c| c.share).collect(),
        Some(crate::library::MorphPick::One(n)) => coats.iter().map(|c| if &c.name == n { 1.0 } else { 0.0 }).collect(),
        Some(crate::library::MorphPick::Weights(w)) => coats.iter().map(|c| w.get(&c.name).copied().unwrap_or(0.0).max(0.0)).collect(),
    };
    let Some((which, at)) = crate::variety::pick_coat(&shares, rng.unit(), rng.unit(), |i| coats[i].shades) else {
        return;
    };
    // Dev hook: VG_COATS=1 says what each group was dressed in.
    if std::env::var_os("VG_COATS").is_some() {
        let w = &coats[which];
        let r = w.at(at);
        eprintln!(
            "VG_COATS: {} in {} at {at:.2} (mul {:?} add {:?})",
            species[s].name,
            w.name,
            r.mul.map(|v| (v * 100.0).round() / 100.0),
            r.add.map(|v| (v * 100.0).round() / 100.0)
        );
    }
    let bare: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Creature, Option<&Coat>)>()
        .iter()
        .filter(|(_, _, c)| c.is_none())
        .map(|(e, _, _)| e)
        .collect();
    for e in bare {
        let _ = world.insert_one(e, Coat { which, at });
    }
}

/// Which coats the biome at (x, z) wants of the creature it is listing.
pub(crate) fn biome_pick<'a>(land: &'a Land, x: f32, z: f32, name: &str) -> Option<&'a crate::library::MorphPick> {
    let f = &land.f;
    let b = &land.lib.biomes[f.biome[f.idx(x, z)] as usize];
    b.creatures.iter().find(|c| c.creature == name)?.morph.as_ref()
}

/// A group's size for the dev hook: the smallest group the species keeps.
fn sp_group(sp: &Species) -> u32 {
    sp.beh.group.as_ref().map_or(1, |g| g.size[0].max(1))
}

/// Put `size` of species `s` at or near (x, z) by its way of moving. False
/// when the spot won't do (no water for a swimmer, no tree for a woodpecker).
#[allow(clippy::too_many_arguments)]
pub(crate) fn place(
    world: &mut hecs::World,
    species: &[Species],
    s: usize,
    rng: &mut Rng,
    land: &Land,
    store: &Store,
    models: &[Model],
    plants: &super::plants::Flora,
    (x, z): (f32, f32),
    size: u32,
    now: f32,
    room: Option<Vec3>,
    far: bool,
) -> bool {
    let f = &land.f;
    let sea = f.sea;
    // A species that crosses between land and water takes the form the spot
    // calls for: a biome lists the animal once and where it lands decides.
    // The water form wants the depth its behaviour keeps to; shallower than
    // that, or dry, and the land form stands on the bank instead.
    let s = match species[s].water {
        Some(w) if !species[s].is_water_form => {
            let want = species[w].beh.shallows.unwrap_or(2.0);
            // The water form carries its own `spawn_chance`, checked here
            // rather than with the land form's: seals are common on the rocks
            // and scarce out on the shelf.
            if sea - f.height(x, z) >= want && rng.unit() < species[w].beh.spawn_chance {
                w
            } else {
                s
            }
        }
        _ => s,
    };
    let sp = &species[s];
    let mover = sp.verb().mover;
    let whale = mover == Mover::Breach;
    let i = f.idx(x, z);
    // In a room under the land only what keeps to the floor it was put on
    // will do: a walker, a small bird that hops and perches (`flit`), and
    // fish, which read the floor under them from the voxels. The rest steer
    // by the land's height map or by plants growing on it, and would be up
    // through the roof at once.
    if let Some(near) = room {
        return match mover {
            Mover::Walk { wades: false, .. } => walkers(world, species, s, rng, land, store, models, (x, z), size, false, now, Some(near), false),
            Mover::Flit => {
                let Some((gy, m)) = super::terrain::ground_near(store, x, z, near) else {
                    return false;
                };
                if water_voxel(m) {
                    return false;
                }
                flit::spawn(world, s, sp, Vec3::new(x, gy, z), size.max(1), hash32(rng.state ^ s as u32 ^ 0xf117));
                true
            }
            // Fish in a sea cave's pool: open water at the surface here, deep
            // enough down to the floor (swim.rs reads the depth the same way).
            Mover::School => {
                let ter = super::terrain::Terrain { store, land: Some(land), sea };
                let top = f.sea_now();
                let p = Vec3::new(x, top - 0.3, z);
                if !store.loaded((x * 4.0).floor() as i32, (p.y * 4.0).floor() as i32, (z * 4.0).floor() as i32) || ter.in_rock(p) {
                    return false;
                }
                let depth = top - ter.bed_under(p);
                if depth < sp.beh.shallows.unwrap_or(2.0) {
                    return false;
                }
                swim::spawn_school(world, s, species, models, Vec3::new(x, top, z), depth, size.max(1), hash32(rng.state ^ s as u32 ^ 0xf15));
                true
            }
            _ => false,
        };
    }
    match mover {
        Mover::Flock { soar, .. } => {
            let gy = f.height(x, z).max(sea);
            let seed = hash32(rng.state ^ s as u32 ^ 0x51ed);
            let n = if soar { 1 } else { size.max(1) };
            flock::spawn(world, s, sp, Vec3::new(x, gy, z), n, seed);
            true
        }
        Mover::School | Mover::Breach => {
            let seed = hash32(rng.state ^ s as u32 ^ 0xf15);
            let depth = sea - f.height(x, z);
            if depth >= if whale { 14.0 } else { sp.beh.shallows.unwrap_or(2.0) } {
                if whale {
                    swim::spawn_whale(world, s, species, models, Vec3::new(x, sea, z), seed);
                } else {
                    swim::spawn_school(world, s, species, models, Vec3::new(x, sea, z), depth, size.max(1), seed);
                }
                return true;
            }
            // Fish in a river or lake: the deepest water within 30 m.
            if whale {
                return false;
            }
            let Some((at, d)) = inland_pool(f, x, z, 30.0) else {
                return false;
            };
            swim::spawn_school(world, s, species, models, at, d, size.max(1), seed);
            true
        }
        Mover::Flit => {
            let Some((gy, _)) = voxel_ground(store, x, z, store.top_m()) else {
                return false;
            };
            if gy < sea + 0.4 || f.water[i] == 1 {
                return false;
            }
            let seed = hash32(rng.state ^ s as u32 ^ 0xf117);
            flit::spawn(world, s, sp, Vec3::new(x, gy, z), size.max(1), seed);
            true
        }
        Mover::Circle { radius, speed } => {
            let _yaw = rng.unit();
            let gy = f.height(x, z).max(sea);
            let centre = Vec3::new(x, gy + sp.beh.fly_height.max(1.0) + rng.unit() * 3.0, z);
            let radius = radius.0 + rng.unit() * radius.1;
            let speed = (speed.0 + rng.unit() * speed.1) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
            let angle = rng.unit() * std::f32::consts::TAU;
            world.spawn((Gull::new(centre, radius, angle, speed), Creature::of(s), Look::new(sp.model, Vec3::ZERO, 0.0)));
            true
        }
        Mover::Walk { wades, .. } => walkers(world, species, s, rng, land, store, models, (x, z), size, wades, now, None, far),
        Mover::Paddle => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            let at = Vec3::new(x, sea, z);
            let Some(w) = super::fowl::water_near(&ter, at, 0.0, 12.0, None, rng).or_else(|| super::fowl::water_near(&ter, at, 0.0, 60.0, None, rng)) else {
                return false;
            };
            super::fowl::spawn(world, s, sp, models, &ter, w, size.max(1), hash32(rng.state ^ s as u32 ^ 0xd0c));
            true
        }
        Mover::Cling => super::woodpecker::spawn(world, s, sp, plants, Vec3::new(x, f.height(x, z), z), size.max(1), hash32(rng.state ^ s as u32 ^ 0x90d)),
        Mover::Buzz { water } => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            let gy = voxel_ground(store, x, z, store.top_m()).map_or(f.height(x, z), |g| g.0).max(sea);
            super::insects::spawn(world, s, sp, plants, &ter, Vec3::new(x, gy + 0.8, z), size.max(1), water, hash32(rng.state ^ s as u32 ^ 0xbee))
        }
        Mover::Slither { habit } => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            let gy = voxel_ground(store, x, z, store.top_m()).map_or(f.height(x, z), |g| g.0);
            super::snake::spawn(world, s, sp, habit, models, &ter, plants, Vec3::new(x, gy, z), size.max(1), hash32(rng.state ^ s as u32 ^ 0x5bad))
        }
        Mover::Creep => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            super::octopus::spawn(world, s, sp, models, &ter, (x, z), size.max(1), hash32(rng.state ^ s as u32 ^ 0x0c70))
        }
        Mover::Run { style } => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            let gy = voxel_ground(store, x, z, store.top_m()).map_or(f.height(x, z), |g| g.0);
            super::runner::spawn(world, s, sp, style, &ter, Vec3::new(x, gy, z), size.max(1), hash32(rng.state ^ s as u32 ^ 0x5a4d))
        }
        Mover::Hum => {
            let ter = super::terrain::Terrain { store, land: Some(land), sea };
            let gy = voxel_ground(store, x, z, store.top_m()).map_or(f.height(x, z), |g| g.0).max(sea);
            super::hummer::spawn(world, s, sp, plants, &plants.blooming, &ter, Vec3::new(x, gy, z), size.max(1), hash32(rng.state ^ s as u32 ^ 0x4a11))
        }
    }
}

/// The deepest river or lake water at least 1.2 m deep within `r` m of
/// (x, z), sampled every 2 m: its surface point and depth.
fn inland_pool(f: &crate::world::fields::Fields, x: f32, z: f32, r: f32) -> Option<(Vec3, f32)> {
    if f.inland.is_empty() {
        return None;
    }
    let mut best: Option<(Vec3, f32)> = None;
    let steps = (r / 2.0) as i32;
    for i in -steps..=steps {
        for j in -steps..=steps {
            let (px, pz) = (x + i as f32 * 2.0, z + j as f32 * 2.0);
            let Some(w) = f.wet(px, pz) else { continue };
            let d = w.level - f.height(px, pz);
            if w.level > f.sea && d >= 1.2 && best.is_none_or(|b| d > b.1) {
                best = Some((Vec3::new(px, w.level, pz), d));
            }
        }
    }
    best
}

/// How often a member of a family group other than the one that chose the
/// spot is a juvenile. Group species therefore have more young than the flat
/// share gives a lone animal, which is the point: fawns come with does.
const FAMILY_JUVENILE: f32 = 0.3;

/// Dev hook VG_SPAWN_WET, read once: walkers may be set down in the water,
/// to test swimming.
fn spawn_wet() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VG_SPAWN_WET").is_some())
}

/// A walker, or a family group standing together a few spacings apart, at
/// (x, z). False when the spot won't do.
#[allow(clippy::too_many_arguments)]
fn walkers(
    world: &mut hecs::World,
    species: &[Species],
    s: usize,
    rng: &mut Rng,
    land: &Land,
    store: &Store,
    models: &[Model],
    (x, z): (f32, f32),
    size: u32,
    wades: bool,
    now: f32,
    room: Option<Vec3>,
    far: bool,
) -> bool {
    let f = &land.f;
    let sea = f.sea;
    let sp = &species[s];
    let group = sp.beh.group.clone().unwrap_or_default();
    let yaw = rng.unit() * std::f32::consts::TAU;
    // In a room under the land, its own floor: the sea level and the height
    // map both describe the land on top, and a buried room's floor can lie
    // below either.
    // A large animal, or one of the far band (`far`), may be set down past
    // full detail, on the generator's ground (terrain.rs `far_ground`), with
    // nothing there to bump into.
    let large = sp.large(models) || far;
    // The ground's height and the material of its top voxel; no material
    // on the generator's ground.
    let ground = |x: f32, z: f32| -> Option<(f32, Option<u8>)> {
        match room {
            Some(near) => super::terrain::ground_near(store, x, z, near).map(|(g, m)| (g, Some(m))),
            None => voxel_ground(store, x, z, store.top_m()).map(|(g, m)| (g, Some(m))).or_else(|| {
                // Never off the edge of the map, where the generator still
                // answers with a height.
                let inside = x > 8.0 && z > 8.0 && x < f.size - 8.0 && z < f.size - 8.0;
                (large && inside).then(|| super::terrain::far_ground(store, x, z)).flatten().map(|g| (g, None))
            }),
        }
    };
    let Some((gy, _)) = ground(x, z) else {
        return false;
    };
    if room.is_none() && gy < sea + 0.4 {
        return false;
    }
    // Waders only turn up by a river or the shore.
    if wades && super::terrain::shore_dist(land, x, z) > 6.0 {
        return false;
    }
    let kin = if size > 1 { Some(hash32(rng.state ^ 0x4b1e ^ s as u32) | 1) } else { None };
    let mut placed = 0;
    // A burrower is set down on a burrow of its own (a structure tagged
    // `burrow` near where its group was asked for), which it then keeps to
    // and bolts back down (walk.rs); with none near, where it stands.
    let mut burrows = if sp.verb().burrows() && room.is_none() { burrows_near(land, x, z, BURROW_REACH_M) } else { Vec::new() };
    burrows.reverse();
    // Where the members already stand, so the next is set down beside them
    // and not inside one (creatures/crowd.rs would push them apart, but a
    // herd that spends its first seconds unpicking itself looks it).
    let mut taken: Vec<super::crowd::Shape> = Vec::new();
    let lead = &models[sp.model];
    let length = lead.reach[1] - lead.reach[0];
    let shape_at = |mx: f32, mz: f32| super::crowd::Shape::of(Vec3::new(mx, 0.0, mz), yaw, lead.half_w, lead.height, lead.reach, 1.0);
    let clear_of = |sh: &super::crowd::Shape, taken: &[super::crowd::Shape]| {
        taken.iter().all(|t| {
            let (p, q) = sh.nearest(t);
            (p - q).length() >= sh.r + t.r + group.spacing * 0.3
        })
    };
    for k in 0..size.max(1) {
        // The first member stands where the group was asked for; each of
        // the others tries a few spots round it, so one boulder or a patch
        // of brush costs a herd a moment's search rather than a member.
        let mut found = None;
        let hole = burrows.pop();
        for _ in 0..if k == 0 || hole.is_some() { 1 } else { 10 } {
            let (mx, mz) = if let Some(h) = hole {
                (h.x, h.y)
            } else if k == 0 {
                (x, z)
            } else {
                // A long animal needs a body length more room than its
                // spacing says before anyone stands nose to its tail.
                let a = rng.unit() * std::f32::consts::TAU;
                let r = group.spacing * (1.0 + rng.unit()) + length * 0.5 * rng.unit();
                let c = (x + a.cos() * r, z + a.sin() * r);
                if !clear_of(&shape_at(c.0, c.1), &taken) {
                    continue;
                }
                c
            };
            let Some((my, mm)) = ground(mx, mz) else {
                continue;
            };
            // Never set down in a river: a wader in the shallows at most
            // (unless `spawn_wet`).
            let river = (mm.is_some_and(water_voxel) || f.river_depth(mx, mz) > if wades { 0.5 } else { 0.1 }) && !spawn_wet();
            let off_land = room.is_none() && (my < sea + 0.4 || (my - f.height(mx, mz)).abs() > 1.0);
            if off_land || mm.is_some_and(crate::mesh::sprites::leafy) || river {
                continue;
            }
            let pos = Vec3::new(mx, my + 0.01, mz);
            // On the generator's ground there are no voxels to stand in.
            let far = mm.is_none();
            if !far && crate::physics::hits(store, pos, lead.half_w, lead.height) {
                continue;
            }
            found = Some((mx, mz, pos));
            break;
        }
        let Some((mx, mz, pos)) = found else {
            continue;
        };
        taken.push(shape_at(mx, mz));
        let seed = hash32(rng.state ^ 0xabcdef ^ s as u32 ^ k.wrapping_mul(0x9e37));
        let timer = rng.unit() * 2.0;
        let yaw = yaw + (rng.unit() - 0.5) * 0.8;
        // Now and then a rare morph stands in for one of the group.
        let ms = crate::variety::pick(&sp.morphs, rng.unit()).unwrap_or(s);
        let mmodel = species[ms].model;
        let mmd = &models[mmodel];
        let side = if seed & 1 == 0 { 1.0 } else { -1.0 };
        // Which age this one is. In a family group the one that set the spot
        // is the adult and the others are often its young, so juveniles turn
        // up in clusters rather than alone.
        let ages = &species[ms].ages;
        let stage = if kin.is_some() && k > 0 && ages.has_juvenile() && rng.unit() < FAMILY_JUVENILE {
            Stage::Juvenile
        } else {
            super::age::roll(ages, rng.unit())
        };
        if stage == Stage::Dead {
            // Bones already lying here, with no mover and no collision box.
            // Nothing to draw them with means nothing to find.
            let Some(rig) = models[mmodel].skeleton.as_ref() else {
                continue;
            };
            let pose = crate::anim::limp::settled(rig, models[mmodel].voxel, models[mmodel].pivot, seed);
            world.spawn((
                Creature {
                    species: ms,
                    spawned_for: species[s].quota,
                },
                Look::new(mmodel, pos, yaw),
                Age::old_bones(now, rng.unit()),
                crate::anim::Anim::hold(pose),
            ));
            placed += 1;
            continue;
        }
        let mut critter = Critter::new(yaw, timer, seed, side);
        if hole.is_some() {
            critter.home = Some(pos);
        }
        let e = world.spawn((
            critter,
            Creature {
                species: ms,
                spawned_for: species[s].quota,
            },
            Body::new(mmd.half_w, mmd.height),
            Look::new(mmodel, pos, yaw),
            Age::new(stage),
        ));
        if let Some(g) = kin {
            let _ = world.insert_one(e, Kin { group: g });
        }
        placed += 1;
    }
    placed > 0
}

/// How far from where a burrowing group was asked for its members look for
/// burrows, metres.
const BURROW_REACH_M: f32 = 18.0;

/// The mouths of the burrows (structures tagged `burrow`) within `reach` of
/// a point, nearest first.
fn burrows_near(land: &Land, x: f32, z: f32, reach: f32) -> Vec<glam::Vec2> {
    let at = glam::Vec2::new(x, z);
    let mut out: Vec<(f32, glam::Vec2)> = land
        .placed_near(x - reach, z - reach, x + reach, z + reach)
        .into_iter()
        .map(|k| &land.placed[k as usize])
        .filter(|p| land.lib.structures[p.def].tags.iter().any(|t| t == "burrow"))
        .map(|p| {
            let (x0, z0, x1, z1) = p.footprint(&land.shapes);
            glam::Vec2::new((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125)
        })
        .map(|c| (c.distance(at), c))
        .filter(|(d, _)| *d <= reach)
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Coat as LibCoat, MorphPick, Paint};
    use crate::variety::Morph;

    #[test]
    fn newcomers_dissolve_in_and_far_ones_out() {
        let seen = 360.0;
        assert_eq!(dissolved(None, 50.0, seen), 0.0, "always there and near: whole");
        assert_eq!(dissolved(Some(0.0), 50.0, seen), 1.0, "just set down: not yet seen");
        assert!((dissolved(Some(APPEAR_S / 2.0), 50.0, seen) - 0.5).abs() < 1e-6);
        assert_eq!(dissolved(Some(APPEAR_S), 50.0, seen), 0.0);
        assert_eq!(dissolved(None, seen * (1.0 - EDGE_SHARE) - 1.0, seen), 0.0);
        assert_eq!(dissolved(None, seen, seen), 1.0, "gone by the time it is dropped");
    }

    #[test]
    fn far_animals_stay_while_watched() {
        let (player, east) = (Vec3::ZERO, Vec3::X);
        assert!(in_view(Vec3::new(200.0, 30.0, 40.0), player, east));
        assert!(!in_view(Vec3::new(-200.0, 0.0, 0.0), player, east));
        assert!(!in_view(Vec3::new(10.0, 0.0, 200.0), player, east), "square to one side is out of view");
        assert!(in_view(Vec3::new(-50.0, 0.0, 0.0), player, Vec3::ZERO), "no facing yet: everything counts as seen");
    }

    /// A fox with a red coat and a rare silver one, built the way worldload
    /// does it: library coats fitted against the entry's own colours.
    fn fox() -> Species {
        let colors: std::collections::HashMap<String, Vec<f32>> = [("fox".to_string(), vec![214.0, 104.0, 34.0]), ("white".to_string(), vec![240.0, 236.0, 228.0])].into_iter().collect();
        let coats = vec![
            LibCoat {
                name: "red".into(),
                share: 0.9,
                shades: 1,
                paint: vec![],
            },
            LibCoat {
                name: "silver".into(),
                share: 0.1,
                shades: 1,
                // Two colours named, as the library entry does it: the fur
                // goes black and the tail tip stays white, which no multiply
                // could manage.
                paint: vec![("fox".into(), Paint::One(vec![36.0, 28.0, 24.0])), ("white".into(), Paint::One(vec![240.0, 236.0, 228.0]))],
            },
        ];
        let beh = serde_json::from_value(serde_json::json!({ "movement": "skulk" })).unwrap();
        Species {
            name: "fox".into(),
            model: 0,
            beh,
            flees: vec![],
            hunts: vec![],
            flees_player: false,
            like: None,
            quota: 0,
            water: None,
            is_water_form: false,
            vary: Default::default(),
            coats: crate::variety::wears(&coats, &colors),
            morphs: vec![],
            ages: Default::default(),
            aged_tint: [1.0; 3],
            forms: Default::default(),
        }
    }

    fn pack(n: usize) -> hecs::World {
        let mut w = hecs::World::new();
        for _ in 0..n {
            w.spawn((Creature::of(0), Look::new(0, Vec3::ZERO, 0.0)));
        }
        w
    }

    fn worn(world: &hecs::World) -> Vec<Coat> {
        world.query::<&Coat>().iter().map(|c| *c).collect()
    }

    #[test]
    fn a_group_is_dressed_in_one_coat() {
        let species = [fox()];
        let mut rng = Rng::new(7);
        let mut world = pack(5);
        dress(&mut world, &species, 0, None, &mut rng);
        let got = worn(&world);
        assert_eq!(got.len(), 5, "everything placed is dressed");
        assert!(got.iter().all(|c| *c == got[0]), "a group wears one coat: {got:?}");
    }

    #[test]
    fn a_second_group_is_dressed_on_its_own_and_the_first_is_left_alone() {
        let species = [fox()];
        let mut rng = Rng::new(7);
        let mut world = pack(2);
        dress(&mut world, &species, 0, Some(&MorphPick::One("silver".into())), &mut rng);
        let first = worn(&world);
        assert!(first.iter().all(|c| c.which == 1), "the biome asked for silver: {first:?}");
        // A fresh group turns up and is dressed in red; the silver ones keep
        // the coat they came in.
        for _ in 0..2 {
            world.spawn((Creature::of(0), Look::new(0, Vec3::ZERO, 0.0)));
        }
        dress(&mut world, &species, 0, Some(&MorphPick::One("red".into())), &mut rng);
        let mut counts = [0, 0];
        for c in worn(&world) {
            counts[c.which] += 1;
        }
        assert_eq!(counts, [2, 2], "each group kept its own coat");
    }

    #[test]
    fn the_coat_reaches_what_is_drawn() {
        let species = [fox()];
        let mut rng = Rng::new(3);
        let mut world = pack(3);
        dress(&mut world, &species, 0, Some(&MorphPick::One("silver".into())), &mut rng);
        roll_morphs(&mut world, &species, &[crate::models::from_cells(&[0u8; 32 * 32 * 32], 0.05, 0.18, 0.75)], &Rng::new(11));
        let fur = crate::models::linear_colour(&[214.0, 104.0, 34.0]);
        let mut seen = 0;
        for m in world.query::<&Morph>().iter() {
            // Where the fur ends up, before the light: the coat's map with
            // this individual's own draw over it.
            let drawn = [0, 1, 2].map(|i| fur[i] * m.tint[i] + m.shift[i]);
            let want = crate::models::linear_colour(&[36.0, 28.0, 24.0]);
            for i in 0..3 {
                // Within the individual's own shade and hue spread.
                assert!((drawn[i] - want[i]).abs() < 0.03, "{drawn:?} wanted {want:?}");
            }
            assert!(m.shift.iter().any(|v| v.abs() > 1e-3), "a silver fox is repainted, not only darkened");
            seen += 1;
        }
        assert_eq!(seen, 3);
    }

    #[test]
    fn a_species_in_one_colouring_is_not_dressed() {
        let mut species = [fox()];
        species[0].coats.clear();
        let mut rng = Rng::new(7);
        let mut world = pack(3);
        dress(&mut world, &species, 0, None, &mut rng);
        assert!(worn(&world).is_empty());
    }
}
