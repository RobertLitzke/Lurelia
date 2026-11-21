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
    let unrolled: Vec<(hecs::Entity, usize, usize, Stage, Option<Coat>)> = world
        .query::<(hecs::Entity, &Creature, Option<&crate::variety::Morph>, Option<&Age>, Option<&Coat>)>()
        .iter()
        .filter(|(_, _, m, _, _)| m.is_none())
        .map(|(e, c, _, a, coat)| (e, c.species, c.spawned_for, a.map_or(Stage::Adult, |a| a.stage), coat.copied()))
        .collect();
    for (e, s, parent, stage, coat) in unrolled {
        let bits = e.to_bits().get();
        let seed = hash32(bits as u32 ^ (bits >> 32) as u32 ^ rng.state);
        // A rare morph stands in for one of its parent's spawns and keeps its
        // place in the biome's quota (`Creature::spawned_for`): the pale mare
        // among the horses, the white bear, the great skate among the small
        // ones. Walkers draw theirs as a group is placed, so each member of a
        // herd rolls on its own and `species` already differs from
        // `spawned_for`; every other way of moving draws here, so circlers,
        // flutterers, flocks, swimmers and the rest are not left out.
        let s = if s == parent {
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
const DROP_M: f32 = 130.0;
/// Past this it is dropped even in view: further than the fog lets anything
/// be made out on most days, metres.
const DROP_SEEN_M: f32 = 360.0;
/// Whales are watched from far off, so they go further first.
const DROP_WHALE_M: f32 = 260.0;
const DROP_WHALE_SEEN_M: f32 = 600.0;

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
pub fn drop_far(world: &mut hecs::World, species: &[Species], player: Vec3, facing: Vec3) {
    let far: Vec<hecs::Entity> = world
        .query::<(hecs::Entity, &Look, &Creature)>()
        .iter()
        .filter(|(_, l, c)| {
            if l.pos.y < 0.0 {
                return true;
            }
            let (near, seen) = if species[c.species].verb().mover == Mover::Breach { (DROP_WHALE_M, DROP_WHALE_SEEN_M) } else { (DROP_M, DROP_SEEN_M) };
            let d = Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length();
            d > seen || d > near && !in_view(l.pos, player, facing)
        })
        .map(|(e, _, _)| e)
        .collect();
    for e in far {
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
) {
    let f = &land.f;
    drop_far(world, species, player, facing);
    // Under a roof the player is in a room the land was laid over -- a
    // buried arena, a cave -- and newcomers are looked for on its floor,
    // not on the land overhead (#377).
    let indoors = super::terrain::roof_over(store, player).map(|_| player);
    // What the biomes around the player want.
    let mut want: HashMap<usize, u32> = HashMap::default();
    for k in 0..9 {
        let p = if k == 0 {
            player
        } else {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            player + Vec3::new(a.cos(), 0.0, a.sin()) * 45.0
        };
        let b = &land.lib.biomes[f.biome[f.idx(p.x, p.z)] as usize];
        for cu in &b.creatures {
            if let Some(s) = species.iter().position(|x| x.name == cu.creature) {
                let e = want.entry(s).or_insert(0);
                *e = (*e).max(cu.count);
            }
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
                    let land_ok = store.top_m() > 0.0 && voxel_ground(store, x, z, store.top_m()).is_some();
                    if !land_ok {
                        continue;
                    }
                    if place(world, species, s, rng, land, store, models, plants, (x, z), size, now, indoors) {
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
    let mut order: Vec<(usize, u32)> = want.into_iter().map(|(s, n)| (s, super::diel::quota(n, abroad(&species[s], hour, sky, phase)))).collect();
    order.sort();
    cull_off_hours(world, species, player, facing, &order, hour, sky, phase);
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
        // A group never brings the count past what the biomes want.
        let room = n - have.get(&s).copied().unwrap_or(0);
        // Newcomers turn up out of view: behind and beside the player, never in
        // the 140 degrees ahead (birds popping into an open sky give it away).
        let ahead = facing.z.atan2(facing.x);
        let looking = facing.length_squared() > 0.01;
        for _ in 0..4 {
            let a = if looking {
                ahead + std::f32::consts::PI + (rng.unit() - 0.5) * 3.8
            } else {
                rng.unit() * std::f32::consts::TAU
            };
            // Fish turn up nearer: under water the fog hides anything past
            // a few visibilities, and a reef's clouds stay where they are
            // born, so 20-90 m out most of them would never be met.
            let fish = mover == Mover::School;
            let r = if whale {
                50.0 + rng.unit() * 90.0
            } else if fish {
                12.0 + rng.unit() * 38.0
            } else {
                20.0 + rng.unit() * 70.0
            };
            let (x, z) = (player.x + a.cos() * r, player.z + a.sin() * r);
            let i = f.idx(x, z);
            let lists = land.lib.biomes[f.biome[i] as usize].creatures.iter().any(|c| c.creature == sp.name);
            if !lists {
                continue;
            }
            let group = sp.beh.group.clone().unwrap_or_default();
            let size = (group.size[0] + (rng.unit() * (group.size[1].saturating_sub(group.size[0]) + 1) as f32) as u32).min(room);
            let placed = place(world, species, s, rng, land, store, models, plants, (x, z), size, now, indoors);
            if placed {
                dress(world, species, s, biome_pick(land, x, z, &sp.name), rng);
                break;
            }
        }
    }
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
        Mover::Walk { .. } | Mover::Slither { .. } => Build::Ground,
        Mover::Buzz { .. } => Build::Insect,
        Mover::School | Mover::Breach | Mover::Creep => Build::Swimmer,
        Mover::Circle { .. } | Mover::Flock { .. } | Mover::Flit | Mover::Paddle | Mover::Cling => Build::Flier,
    }
}

/// Take away what the hour has put to bed: anything over the species' share
/// of its quota, furthest from the player first and never within `KEEP_M`, so
/// the fireflies go out over the next field rather than in your face. Only
/// species a biome round the player still lists are thinned, so an animal
/// left behind by walking out of its biome wanders off as it always has.
fn cull_off_hours(
    world: &mut hecs::World,
    species: &[Species],
    player: Vec3,
    facing: Vec3,
    want: &[(usize, u32)],
    hour: f32,
    sky: &crate::world::weather::Sky,
    phase: &crate::world::season::Phase,
) {
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
            .filter(|(_, _, c)| c.spawned_for == s)
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
fn biome_pick<'a>(land: &'a Land, x: f32, z: f32, name: &str) -> Option<&'a crate::library::MorphPick> {
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
    // will do: a walker, and a small bird that hops and perches (`flit`).
    // The rest steer by the land's height map or by plants growing on it,
    // and would be up through the roof at once.
    if let Some(near) = room {
        return match mover {
            Mover::Walk { wades: false, .. } => walkers(world, species, s, rng, land, store, models, (x, z), size, false, now, Some(near)),
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
        Mover::Walk { wades, .. } => walkers(world, species, s, rng, land, store, models, (x, z), size, wades, now, None),
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

/// A walker, or a family group standing together a few spacings apart, at
/// (x, z). False when the spot won't do.
#[allow(clippy::too_many_arguments)]
fn walkers(world: &mut hecs::World, species: &[Species], s: usize, rng: &mut Rng, land: &Land, store: &Store, models: &[Model], (x, z): (f32, f32), size: u32, wades: bool, now: f32, room: Option<Vec3>) -> bool {
    let f = &land.f;
    let sea = f.sea;
    let sp = &species[s];
    let group = sp.beh.group.clone().unwrap_or_default();
    let yaw = rng.unit() * std::f32::consts::TAU;
    // In a room under the land, its own floor: the sea level and the height
    // map both describe the land on top, and a buried room's floor can lie
    // below either.
    let ground = |x: f32, z: f32| match room {
        Some(near) => super::terrain::ground_near(store, x, z, near),
        None => voxel_ground(store, x, z, store.top_m()),
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
    for k in 0..size.max(1) {
        let (mx, mz) = if k == 0 {
            (x, z)
        } else {
            let a = rng.unit() * std::f32::consts::TAU;
            let r = group.spacing * (1.0 + rng.unit());
            (x + a.cos() * r, z + a.sin() * r)
        };
        let Some((my, mm)) = ground(mx, mz) else {
            continue;
        };
        // Never set down in a river: a wader in the shallows at most.
        // (VG_SPAWN_WET=1 lets the dev hook put walkers in the water to test
        // swimming.)
        let river = (water_voxel(mm) || f.river_depth(mx, mz) > if wades { 0.5 } else { 0.1 }) && std::env::var_os("VG_SPAWN_WET").is_none();
        let off_land = room.is_none() && (my < sea + 0.4 || (my - f.height(mx, mz)).abs() > 1.0);
        if off_land || crate::mesh::sprites::leafy(mm) || river {
            continue;
        }
        let pos = Vec3::new(mx, my + 0.01, mz);
        let mmd = &models[sp.model];
        if crate::physics::hits(store, pos, mmd.half_w, mmd.height) {
            continue;
        }
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
            let pose = crate::anim::ragdoll::settled(rig, models[mmodel].voxel, seed);
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
        let e = world.spawn((
            Critter::new(yaw, timer, seed, side),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Coat as LibCoat, MorphPick, Paint};
    use crate::variety::Morph;

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
