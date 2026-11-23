//! Everything in the world that isn't terrain, stored in one hecs world:
//! creatures (src/creatures) and pickups (src/items.rs). `Entities` owns the
//! hecs world and the per-world tables, and runs each step in order:
//! spawning, individual variation, walkers, herds, flocks, flitting birds,
//! swimmers, pickups, circlers, then bird wing animation.

use crate::creatures::terrain::{voxel_ground, Terrain};
use crate::creatures::{birds, carry, census, circle, flit, flock, fowl, herd, hummer, insects, octopus, runner, snake, spawn, swim, walk, woodpecker};
use crate::models::{self, Instance, Model};
use crate::util::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::world::fields::World as Land;
use crate::world::gen::Generator;
use crate::world::noise::{hash32, Rng};
use crate::world::store::Store;
use glam::Vec3;
use hecs::World;
use std::sync::Arc;

pub use crate::creatures::{Look, Pathing, Species};

pub struct Entities {
    pub world: World,
    pub gems_collected: HashSet<(i32, i32)>,
    gems_spawned: HashSet<(i32, i32)>,
    spawn_timer: f32,
    rng: Rng,
    gulls_placed: bool,
    /// Spray the animals threw up, waiting for `Game` to hand to the
    /// particle pool (src/fx).
    pub cues: Vec<crate::fx::Cue>,
    /// Footfalls the walkers made this step (creatures/sign.rs), for the
    /// game to lay as marks.
    pub steps: Vec<crate::creatures::sign::Step>,
    /// Large animals remembered where the player left them
    /// (creatures/tracked.rs).
    pub tracked: crate::creatures::tracked::Tracked,
    /// The bases of shrubs big walkers are walking into this step, for the
    /// game to flatten (creatures/trample.rs).
    pub crush: Vec<Vec3>,
    pub just_collected: u32,
    /// A chronicle-built world: creatures come from its biomes and pickups from
    /// its items.
    land: Option<Arc<Land>>,
    pub species: Vec<Species>,
    items: HashMap<String, crate::worldload::ItemModel>,
    /// The display name of the last thing collected, for the HUD.
    pub last_found: Option<String>,
    pub pathing: Pathing,
    /// Fish and whales, for the headless stats.
    pub swim: swim::Stats,
    /// The world's placed plants, for creatures to find (creatures/plants.rs).
    pub plants: crate::creatures::plants::Flora,
    /// The time of day (world/clock.rs), set by the game each step: animals
    /// drink more at dawn and dusk.
    pub hour: f32,
    /// What kind of day it is (world/weather.rs), set by the game each step:
    /// birds keep out of the rain, and grazers lie down in it.
    pub sky: crate::world::weather::Sky,
    /// What part of the year it is (world/season.rs), set by the game each
    /// step: less is abroad in winter, as in the small hours.
    pub phase: crate::world::season::Phase,
    /// The season's name, for which plants are in flower (`set_season`).
    pub season: String,
    /// Where the player is looking (horizontal), set by the game each step:
    /// newcomers turn up out of view, not in front of you.
    pub facing: Vec3,
    /// Seconds the creatures have run, for blinking glows.
    pub clock: f32,
    /// Every creature that has lived near the player, by species, when a
    /// headless tour asks (`--tour`): how often each kind turns up.
    pub met: Option<HashMap<String, HashSet<u64>>>,
}

impl Entities {
    pub fn new(collected: HashSet<(i32, i32)>, seed: u32) -> Self {
        Self {
            world: World::new(),
            gems_collected: collected,
            gems_spawned: HashSet::default(),
            spawn_timer: 0.0,
            rng: Rng::new(hash32(seed ^ 0xe7037ed1)),
            gulls_placed: false,
            cues: Vec::new(),
            steps: Vec::new(),
            tracked: Default::default(),
            crush: Vec::new(),
            just_collected: 0,
            land: None,
            species: Vec::new(),
            items: HashMap::default(),
            last_found: None,
            pathing: Pathing::default(),
            swim: swim::Stats::default(),
            plants: Default::default(),
            hour: crate::world::clock::START,
            sky: Default::default(),
            phase: Default::default(),
            season: String::new(),
            facing: Vec3::ZERO,
            clock: 0.0,
            met: None,
        }
    }

    /// Tell the creatures the season, so hummingbirds go only to what is in
    /// flower (an ocotillo blooms in spring).
    pub fn set_season(&mut self, season: &str) {
        if self.season != season {
            self.season = season.to_string();
            if let Some(land) = &self.land {
                self.plants.set_season(&land.lib, season);
            }
        }
    }

    pub fn set_world(&mut self, land: Arc<Land>, species: Vec<Species>, items: Vec<crate::worldload::ItemModel>) {
        self.plants = crate::creatures::plants::Flora::build(&land);
        if !self.season.is_empty() {
            self.plants.set_season(&land.lib, &self.season);
        }
        crate::items::upgrade_found(&mut self.gems_collected, &land);
        self.land = Some(land);
        self.species = species;
        crate::creatures::link_families(&mut self.species);
        self.items = items.into_iter().map(|i| (i.name.clone(), i)).collect();
    }

    /// Put one individual of species `s` on the ground at (x, z), the way the
    /// spawner would, for the EDIT page (src/editor): no biome quota, no
    /// group, and the roll comes from `seed` rather than from the clock.
    /// False when the spot won't do (the ground there has not loaded, or the
    /// species wants water and there is none).
    pub fn place_one(&mut self, s: usize, (x, z): (f32, f32), store: &Store, models: &[Model], seed: u32) -> bool {
        let Some(land) = self.land.clone() else { return false };
        let mut rng = Rng::new(hash32(seed | 1));
        if !spawn::place(&mut self.world, &self.species, s, &mut rng, &land, store, models, &self.plants, (x, z), 1, self.clock, None) {
            return false;
        }
        spawn::dress(&mut self.world, &self.species, s, None, &mut rng);
        true
    }

    pub fn has_world(&self) -> bool {
        self.land.is_some()
    }

    /// How many things of this display name have been collected.
    pub fn found_count(&self, display: &str) -> usize {
        let Some(land) = &self.land else { return 0 };
        crate::items::found_count(&self.gems_collected, land, &self.items, display)
    }

    /// Creatures alive now by species, for the headless stats.
    pub fn census(&self) -> std::collections::BTreeMap<String, usize> {
        census::census(&self.world, &self.species)
    }

    /// Rigged creatures by current activity, for the headless stats.
    pub fn activities(&self) -> std::collections::BTreeMap<String, usize> {
        census::activities(&self.world, self.land.as_deref())
    }

    pub fn critter_count(&self) -> usize {
        census::critter_count(&self.world)
    }

    pub fn update(&mut self, dt: f32, store: &Store, gen: &Generator, threat: &crate::creatures::startle::Threat, slopes: bool, models: &[Model]) {
        let player = threat.pos;
        let sea = gen.spec.sea_level_m;
        self.clock += dt;
        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = 0.5;
            self.populate(store, gen, player, models);
            if let Some(met) = self.met.as_mut() {
                for (e, c) in self.world.query::<(hecs::Entity, &crate::creatures::Creature)>().iter() {
                    met.entry(crate::creatures::display_name(&self.species, c.species).to_string()).or_default().insert(e.to_bits().get());
                }
            }
        }
        let land = self.land.clone();
        let terrain = Terrain { store, land: land.as_deref(), sea };
        let world = &mut self.world;
        let species = &self.species;

        spawn::roll_morphs(world, species, models, &self.rng);
        walk::attach_life(world, species, models);
        let mut stats = self.pathing;
        walk::update(
            world,
            species,
            &self.plants,
            &terrain,
            dt,
            threat,
            slopes,
            models,
            self.hour,
            &self.sky,
            self.clock,
            &mut stats,
            &mut self.cues,
            &mut self.steps,
            &mut self.crush,
        );
        herd::update(world, species, models, dt);
        stats.jostled += 2 * crate::creatures::crowd::update(world, models, store, dt);
        let groups: Vec<flock::Group> = species.iter().map(|s| s.beh.group.clone().unwrap_or_default()).collect();
        let kills = flock::update(world, species, &groups, &self.plants, dt, threat, &terrain);
        carry::take(world, species, models, &kills, self.clock);
        flit::update(world, species, dt, threat, &terrain);
        fowl::update(world, species, models, dt, threat, &terrain);
        woodpecker::update(world, species, &self.plants, dt, threat, &terrain);
        snake::update(world, species, &self.plants, models, dt, threat, &terrain, &mut stats);
        octopus::update(world, species, models, dt, threat, &terrain);
        insects::update(world, species, &self.plants, dt, threat, &terrain);
        let mut caught = Vec::new();
        runner::update(world, species, dt, threat, &terrain, &mut caught);
        carry::take(world, species, models, &caught, self.clock);
        for &(hunter, _) in &caught {
            let _ = world.insert_one(hunter, carry::Beak { until: self.clock + carry::CARRY_S });
        }
        hummer::update(world, species, &self.plants, &self.plants.blooming, dt, threat, &terrain);
        census::count_sitting(world, &terrain, &mut stats);
        let mut swim = swim::Stats {
            leaps: self.swim.leaps,
            spouts: self.swim.spouts,
            breaches: self.swim.breaches,
            shallow: self.swim.shallow,
            ..Default::default()
        };
        swim::update(world, species, &groups, models, dt, threat, &terrain, &mut swim, &mut self.cues);
        self.swim = swim;

        for (cell, what) in crate::items::collect(world, dt, player) {
            self.gems_collected.insert(cell);
            self.just_collected += 1;
            self.last_found = what;
        }

        carry::update(world, models, dt, self.clock, &terrain);
        // The carcasses still in the air or still sliding, which is every
        // body a shot made until it has come to rest.
        crate::creatures::flung::update(world, models, store, dt);
        circle::update(world, species, &terrain, dt, threat, &mut stats, &mut self.cues);
        self.pathing = stats;
        birds::animate(world, species, dt, models);
    }

    fn populate(&mut self, store: &Store, gen: &Generator, player: Vec3, models: &[Model]) {
        if let Some(land) = self.land.clone() {
            spawn::drop_expired(&mut self.world, self.clock);
            spawn::world_creatures(
                &mut self.world,
                &self.species,
                &mut self.rng,
                &land,
                store,
                player,
                self.facing,
                models,
                &self.plants,
                self.clock,
                self.hour,
                &self.sky,
                &self.phase,
                &mut self.tracked,
            );
            self.pathing.large_restored = self.tracked.restored;
            self.pathing.large_remembered = self.tracked.len() as u32;
            let (mut about, mut far) = (0, 0);
            for (c, l) in self.world.query::<(&crate::creatures::Creature, &crate::creatures::Look)>().iter() {
                if self.species[c.species].large(models) {
                    about += 1;
                    far += (Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length() > crate::stream::FULL_DETAIL_M) as u32;
                }
            }
            (self.pathing.large_about, self.pathing.large_far) = (about, far);
            crate::items::world_items(&mut self.world, &mut self.gems_spawned, &self.gems_collected, &land, &self.items, store, player);
            return;
        }
        let sea = gen.spec.sea_level_m;
        spawn::legacy_animals(&mut self.world, &mut self.rng, store, sea, player, models);
        crate::items::legacy_gems(&mut self.world, &mut self.gems_spawned, &self.gems_collected, store, gen.spec.seed, sea, player);
        if !self.gulls_placed {
            spawn::legacy_gulls(&mut self.world, &mut self.rng, gen, player);
            self.gulls_placed = true;
        }
    }

    /// One of each model standing in a row in front of `eye`, for screenshots.
    pub fn showcase(&mut self, store: &Store, eye: Vec3, yaw: f32) {
        let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let models = [models::DEER_M, models::FOX_M, models::RABBIT_M, models::GEM_M, models::GULL_M];
        for (i, &model) in models.iter().enumerate() {
            let p = eye + fwd * 4.0 + right * ((i as f32 - 2.0) * 1.3);
            let gy = voxel_ground(store, p.x, p.z, eye.y + 2.0).map(|g| g.0).unwrap_or(eye.y - 1.6);
            let y = if model == models::GULL_M {
                gy + 1.2
            } else if model == models::GEM_M {
                gy + 0.35
            } else {
                gy
            };
            // Face the camera, turned a little so the side shows.
            let face = yaw + std::f32::consts::PI + 0.6;
            self.world.spawn((Look::new(model, Vec3::new(p.x, y, p.z), face),));
        }
    }

    /// The model a collectible kind is drawn with, and its display name.
    pub fn item_model(&self, kind: &str) -> Option<(usize, String)> {
        self.items.get(kind).map(|m| (m.model, m.display.clone()))
    }

    /// The creature under a ray, nearest first: (distance, display name).
    pub fn identify(&self, models: &[Model], origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, String)> {
        census::identify(&self.world, &self.species, models, origin, dir, max)
    }

    /// `ground` is the height of the surface under (x, z), land or sea, when
    /// it is known: a creature's shadow fades with its height above it.
    pub fn instances(&self, alpha: f32, models: &[Model], ground: &dyn Fn(f32, f32) -> Option<f32>, out: &mut Vec<(usize, Instance)>) {
        out.clear();
        let mut fk = Vec::new();
        for (e, look, anim, morph, cr, age) in self
            .world
            .query::<(
                hecs::Entity,
                &Look,
                Option<&crate::anim::Anim>,
                Option<&crate::variety::Morph>,
                Option<&crate::creatures::Creature>,
                Option<&crate::creatures::age::Age>,
            )>()
            .iter()
        {
            let (scale, tint, shift) = morph.map_or((1.0, [1.0; 3], [0.0; 3]), |m| (m.scale, m.tint, m.shift));
            // A coat's gain on the glow, less one (models.rs `Instance::glow`).
            let gain = morph.map_or(0.0, |m| m.glow - 1.0);
            let head = morph.map_or(1.0, |m| m.head);
            // The deceased are drawn with the bones inside them, in the bones'
            // own colours, once the body has gone.
            let bones = age.is_some_and(|a| a.boned(self.clock));
            // tint.w: above 0 a white flash (startled); below 0 how far its
            // glowing colours are dimmed (scene.wgsl fs_entity), for blinkers.
            let blink = cr.map_or(0.0, |c| self.species[c.species].beh.blink);
            let flash = if look.flash > 0.0 || blink <= 0.0 {
                look.flash
            } else {
                let phase = (hash32(e.id()) & 0xffff) as f32 / 65536.0;
                let rate = blink * (0.8 + 0.4 * phase);
                let t = (self.clock * rate + phase * 7.0).fract();
                // A short soft flash, then dark for the rest of the cycle.
                let on = if t < 0.22 { (t / 0.22 * std::f32::consts::PI).sin() } else { 0.0 };
                -(1.0 - on).max(0.02)
            };
            let m = &models[look.model];
            let p = look.prev_pos.lerp(look.pos, alpha);
            let lift = look.prev_lift + (look.lift - look.prev_lift) * alpha;
            let yaw = look.prev_yaw + crate::util::wrap_angle(look.yaw - look.prev_yaw) * alpha;
            let skeleton = bones.then(|| m.skeleton.as_ref()).flatten();
            let fade = ground(p.x, p.z).map_or(1.0, |g| shadow_fade(p.y + look.bob + lift - g, m.voxel * scale * 32.0));
            let first = out.len();
            if let Some(rig) = skeleton.or(m.rig.as_ref()) {
                // Rigged creatures draw one instance per part (src/anim).
                let pose = match anim {
                    Some(a) => a.prev.lerp(&a.pose, alpha),
                    None => crate::anim::Pose::rest(rig.bones.len()),
                };
                let at = Vec3::new(p.x, p.y + look.bob + lift, p.z);
                // Bones are their own colours: no coat, no tint.
                let (tint, shift) = if skeleton.is_some() { ([1.0; 3], [0.0; 3]) } else { (tint, shift) };
                crate::anim::push_parts(rig, &pose, models, m.voxel, scale, head, m.pivot, at, yaw, [tint[0], tint[1], tint[2], flash], shift, &mut fk, out);
                if fade < 1.0 {
                    out[first..].iter_mut().for_each(|(_, i)| i.shift[3] = -fade);
                }
                out[first..].iter_mut().for_each(|(_, i)| i.glow[0] = gain);
                continue;
            }
            out.push((
                look.model,
                Instance {
                    pos_yaw: [p.x, p.y + look.bob + lift, p.z, yaw],
                    info: [m.voxel * scale, m.pivot.x, m.pivot.y, m.pivot.z],
                    tint: [tint[0], tint[1], tint[2], flash],
                    rot: [0.0; 4],
                    shift: [shift[0], shift[1], shift[2], if fade < 1.0 { -fade } else { 0.0 }],
                    flesh: [[0.0; 4]; 3],
                    glow: [gain, 0.0, 0.0, 0.0],
                },
            ));
        }
        out.sort_by_key(|(m, _)| *m);
    }
}

/// How much of its shadow a creature `height` m above the ground casts, for a
/// model `span` m across (its 32-voxel grid). The sun is half a degree wide,
/// so a shadow blurs by about a hundredth of the height it falls; a small
/// bird's is gone a few times its size up, and a low sun stretches what is
/// left into a long dark smear far from the bird. Full up to three spans
/// (and four metres), gone by twelve.
pub fn shadow_fade(height: f32, span: f32) -> f32 {
    let near = (span * 3.0).max(4.0);
    let far = (span * 12.0).max(near + 8.0);
    (1.0 - (height - near) / (far - near)).clamp(0.0, 1.0)
}
