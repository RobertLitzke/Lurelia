//! The studio: where the thing being edited stands, the grey grid platform it
//! stands on, and the camera that goes round it.
//!
//! The platform is part of the generated ground rather than a set of voxel
//! edits (`Generator::studio_floor`): nothing of it can end up in somebody's
//! save, and the plant being previewed is drawn after it, so the floor cannot
//! cut the tree in half. It is lit, shadowed and meshed like any other
//! ground, and taking it away is one flag and a regenerate -- which is also
//! what makes the two views one key apart. With the platform off, the same
//! animal stands on the world's own ground wherever the page was opened,
//! which is the biome view.
//!
//! The studio is anchored to where the player stood when the page opened
//! (`home`), not to where the player is now, because while the page is up the
//! player's body is frozen and `player.pos` is the turntable's eye
//! (editor/cam.rs). Everything the studio places works off `home` and the way
//! they were facing, so orbiting the camera moves nothing but the view.

use super::cam::Cam;
use super::pose::Pose;
use super::subject::Built;
use crate::creatures::terrain::voxel_ground;
use crate::game::Game;
use crate::models::Model;
use crate::world::gen::Tree;
use glam::Vec3;

/// Half the platform's side, metres.
const HALF_M: f32 = 12.0;
/// The block of material the page stands on the platform when a material is
/// being edited: half its side and how tall it is, metres. Two metres across
/// shows a whole tile of the texture on three faces at once, and standing
/// taller than a person gives the eye something to judge it against.
const BLOCK_M: f32 = 1.5;
const BLOCK_H: f32 = 2.5;

/// How far the subject may wander from the middle of the studio before it
/// is put back, metres. Only LIVE wanders at all.
const TETHER_M: f32 = 11.0;

/// How far from `home` the subject stands, metres. It no longer depends on
/// how big the subject is: the camera pulls back for a tall tree instead of
/// the tree being stood further off, which is what keeps the platform and
/// the floor still while entries are stepped through.
const AHEAD: f32 = 7.0;

/// What the player was doing when the page opened, to put back when it
/// closes. The body is frozen while the page is up (`Game::editing`), so none
/// of this changes under us; it is saved because the camera writes over the
/// eye every frame and the ride is taken out of the way.
struct Was {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    ride: crate::ride::Ride,
}

/// Marks the one entity the studio stands up for something that is not a
/// creature -- a gun, a found thing. It has no species, no body and nothing
/// driving it; it is a model standing on the platform.
pub struct Prop;

pub struct Studio {
    /// Where the subject stands, in metres.
    pub at: Vec3,
    /// Where the player stood when the page opened: the middle of the
    /// platform, and what everything the studio places is measured from. The
    /// platform is centred here rather than on the subject so the room it
    /// clears is a room the camera starts inside -- in a built-up world a
    /// platform laid only around the subject leaves the view in a wall,
    /// looking at the back of it.
    pub home: Vec3,
    /// The turntable.
    pub cam: Cam,
    /// How many models, species and palette colours the world itself had, so
    /// dropping the subject is a truncate. The subject's palette block
    /// starts at `base_colours`.
    base_models: usize,
    base_species: usize,
    base_colours: usize,
    /// The species the subject was loaded as, once it has been.
    pub species: Option<usize>,
    /// The prop standing in the studio, for a kind that has no species, and
    /// every entity it is drawn with.
    pub prop: Option<hecs::Entity>,
    props: Vec<hecs::Entity>,
    /// Which palette slot each of its colours took when it was built, so a
    /// colour edit can tell whether repainting the palette is enough.
    pub paint: Vec<(String, u8)>,
    /// A creature is wanted but the ground under the studio has not loaded
    /// yet, so the spawn is retried each step.
    pub waiting: bool,
    /// The platform is laid.
    laid: bool,
    /// The material standing on the platform as a block, 0 for none.
    block: u8,
    /// How many times the platform has been tried and come to nothing.
    pub floor_tries: u32,
    /// The subject is posed rather than living in the world.
    pub standing: bool,
    /// How tall the subject is, metres: what the camera aims at.
    pub tall: f32,
    /// How far it reaches across, metres: what the camera stands back for. A
    /// crow is a hand high and a metre of wing wide.
    pub span: f32,
    /// The pose the subject is actually in, which is the one the page asked
    /// for unless the entry cannot take it (a gait asked for on a bird).
    pub showing: Pose,
    /// How many times the spawn has been tried and come to nothing.
    pub tries: u32,
    /// Whether the game was saving its world before the page opened, to put
    /// back when it closes.
    pub persist_was: bool,
    was: Was,
}

impl Studio {
    pub fn new(game: &mut Game) -> Studio {
        let home = game.player.pos;
        let f = game.player.forward();
        let face = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::NEG_Z);
        let at = spot_clear(game, home, face, AHEAD);
        let was = Was {
            pos: home,
            yaw: game.player.yaw,
            pitch: game.player.pitch,
            // Out of the way while the page is up: a player who opened it from
            // a bike would otherwise have the ride's own head for an eye.
            ride: std::mem::take(&mut game.ride),
        };
        Studio {
            at,
            home,
            cam: Cam::new(at, home, 1.8),
            base_models: game.models.len(),
            base_species: game.ents.species.len(),
            base_colours: crate::models::FIXED_SLOTS + game.library_colours,
            species: None,
            prop: None,
            props: Vec::new(),
            paint: Vec::new(),
            waiting: false,
            laid: false,
            block: 0,
            floor_tries: 0,
            standing: false,
            tall: 1.0,
            span: 1.0,
            showing: Pose::Rest,
            tries: 0,
            persist_was: true,
            was,
        }
    }

    /// Say how big the subject is. The camera frames it again only when the
    /// size has really changed, so stepping a behaviour knob leaves the view
    /// you had set up alone.
    pub fn size(&mut self, height_m: f32, span_m: f32, aspect: f32) {
        self.tall = height_m.max(0.2);
        self.span = span_m.max(self.tall);
        self.cam.refit(self.at, self.tall, self.span, aspect);
    }

    /// Frame the subject from scratch, whatever the view is now (the F key,
    /// and every time another entry is opened).
    pub fn reframe(&mut self, aspect: f32) {
        self.cam.frame(self.at, self.tall, self.span, aspect);
    }

    /// Put the camera where the turntable says. Called every frame the page
    /// is open, after the world has ticked.
    pub fn drive_camera(&mut self, game: &mut Game, aspect: f32, dt: f32) {
        self.cam.spin(dt);
        self.cam.apply(game, aspect);
    }

    /// The ground the platform is laid at: under where the page was opened,
    /// or under the subject where that is all there is.
    pub fn ground(&self, game: &Game) -> Option<f32> {
        let under = |p: Vec3| voxel_ground(&game.store, p.x, p.z, p.y + 1.0).or_else(|| voxel_ground(&game.store, p.x, p.z, game.store.top_m()));
        under(self.home).or_else(|| under(self.at)).map(|(y, _)| y)
    }

    /// Whether the platform is in.
    pub fn floored(&self) -> bool {
        self.laid
    }

    /// How far the subject stands from the middle of the platform.
    fn reach(&self) -> f32 {
        (self.at.x - self.home.x).hypot(self.at.z - self.home.z)
    }

    /// Lay the platform at the subject's spot. False while the ground there
    /// has not loaded, so the caller can try again next step.
    pub fn floor_on(&mut self, game: &mut Game) -> bool {
        if self.floored() {
            return true;
        }
        let Some((pale, dark)) = greys(game) else { return false };
        let Some(gy) = self.ground(game) else { return false };
        // Wide enough to hold both where the page was opened and whatever is
        // standing in front of it, with room to walk round.
        let floor = crate::world::gen::StudioFloor {
            x: self.home.x,
            z: self.home.z,
            half_m: HALF_M.max(self.reach() + 6.0),
            top_m: gy,
            pale,
            dark,
            block: self.block,
            block_x: self.at.x,
            block_z: self.at.z,
            block_m: BLOCK_M,
            block_h: BLOCK_H,
        };
        *game.streamer.generator().studio_floor.write().unwrap() = Some(floor);
        self.laid = true;
        self.at.y = gy;
        self.regen(game);
        true
    }

    /// Take the platform away, putting the world's own ground back.
    pub fn floor_off(&mut self, game: &mut Game) {
        if !self.floored() {
            return;
        }
        *game.streamer.generator().studio_floor.write().unwrap() = None;
        self.laid = false;
        self.regen(game);
    }

    /// Generate the ground around the studio again, platform, plant and all.
    fn regen(&self, game: &mut Game) {
        let p = game.player.pos;
        let r = HALF_M.max(self.reach() + 6.0) + 8.0;
        game.streamer.regen_area(self.home.x - r, self.home.z - r, self.home.x + r, self.home.z + r, p);
    }

    /// Where the subject's palette block starts: straight after the world's
    /// own blocks. What its models' `block` is set to.
    pub fn block(&self) -> u32 {
        self.base_colours as u32
    }

    /// Make `colours` the subject's palette block, in place of whatever
    /// block it had, and have the palette uploaded again.
    pub fn paint_block(&self, game: &mut Game, colours: &[[f32; 4]]) {
        game.palette.resize(self.base_colours, [1.0, 0.0, 1.0, 0.0]);
        game.palette.extend_from_slice(colours);
        game.palette_dirty = true;
    }

    /// Hand the game an edited creature as a species of its own, replacing
    /// whatever subject was loaded before. Returns what the build complained
    /// about, if anything.
    pub fn load_creature(&mut self, game: &mut Game, built: Built) -> Vec<String> {
        self.drop_subject(game);
        let index = game.ents.species.len();
        let mut built = built;
        self.paint_block(game, &built.colours);
        game.models.append(&mut built.models);
        game.ents.species.push(built.species);
        game.models_dirty = true;
        self.species = Some(index);
        self.paint = std::mem::take(&mut built.paint);
        self.waiting = true;
        self.tries = 0;
        built.problems
    }

    /// Stand a model that is not a creature on the platform: a gun, a found
    /// thing. `colours` are appended to the palette the same way a creature's
    /// are, and the first model is the one stood up.
    /// `turn` is added to the way it faces: a creature is shown three-quarters
    /// on, but a gun reads broadside, which is a quarter turn from that.
    pub fn load_prop(&mut self, game: &mut Game, models: Vec<Model>, colours: &[[f32; 4]], turn: f32) {
        self.drop_subject(game);
        self.paint_block(game, colours);
        let first = game.models.len();
        let n = models.len();
        let mut models = models;
        game.models.append(&mut models);
        game.models_dirty = true;
        let ground = self.ground(game).unwrap_or(self.at.y);
        let at = Vec3::new(self.at.x, ground, self.at.z);
        let yaw = self.facing() + turn;
        // One entity to a model, all at the same place: the minigun's barrels
        // are a model of their own with their own pivot, and the two compose
        // into the gun exactly as the shooter draws them.
        let mut first_e = None;
        for k in 0..n {
            let e = game.ents.world.spawn((crate::entities::Look::new(first + k, at, yaw),));
            let _ = game.ents.world.insert_one(e, Prop);
            self.props.push(e);
            first_e.get_or_insert(e);
        }
        self.prop = first_e;
        self.standing = true;
        self.waiting = false;
    }

    /// Everything the subject brought with it: its individuals or its prop,
    /// its species row and its models.
    pub fn drop_subject(&mut self, game: &mut Game) {
        if self.prop.take().is_some() {
            for e in std::mem::take(&mut self.props) {
                let _ = game.ents.world.despawn(e);
            }
            game.models.truncate(self.base_models);
            game.models_dirty = true;
        }
        let Some(s) = self.species.take() else {
            self.waiting = false;
            return;
        };
        self.despawn_mine(game, s);
        game.ents.species.truncate(self.base_species);
        game.models.truncate(self.base_models);
        game.models_dirty = true;
        self.waiting = false;
    }

    /// The subject's own individuals and nobody else's.
    fn mine(&self, game: &Game, s: usize) -> Vec<hecs::Entity> {
        game.ents
            .world
            .query::<(hecs::Entity, &crate::creatures::Creature)>()
            .iter()
            .filter(|(_, c)| c.species >= s)
            .map(|(e, _)| e)
            .collect()
    }

    fn despawn_mine(&self, game: &mut Game, s: usize) {
        for e in self.mine(game, s) {
            let _ = game.ents.world.despawn(e);
        }
    }

    /// Put one individual of the loaded species at the studio spot, at the
    /// age, roll and pose the page is showing. False while the ground has not
    /// loaded, so the caller tries again.
    pub fn spawn(&mut self, game: &mut Game, stage: crate::creatures::age::Stage, seed: u32, pose: Pose) -> bool {
        let Some(s) = self.species else { return false };
        self.showing = pose;
        self.despawn_mine(game, s);
        if pose != Pose::Live {
            // Posed: no mover, no mind, nothing to walk it out of frame.
            if self.ground(game).is_none() {
                self.tries += 1;
                self.waiting = self.tries < 120;
                return false;
            }
            self.stand(game, s);
            self.dress(game, s, stage, seed);
            self.repose(game, pose, seed);
            self.waiting = false;
            self.standing = true;
            self.tries = 0;
            return true;
        }
        let models = std::mem::take(&mut game.models);
        let placed = game.ents.place_one(s, (self.at.x, self.at.z), &game.store, &models, seed).is_some();
        game.models = models;
        if !placed {
            // The spawner refuses a spot that does not suit the animal: an
            // alligator wants water, a woodpecker a trunk, a deer ground
            // that is not a cliff. The studio shows it anyway, standing
            // still, because a page that shows nothing teaches nothing.
            self.tries += 1;
            if self.tries < 30 {
                self.waiting = true;
                return false;
            }
            self.stand(game, s);
            self.dress(game, s, stage, seed);
            self.repose(game, Pose::Rest, seed);
            self.showing = Pose::Rest;
            self.waiting = false;
            self.standing = true;
            return true;
        }
        self.dress(game, s, stage, seed);
        self.waiting = false;
        self.standing = false;
        self.tries = 0;
        true
    }

    /// Which way the subject faces: back toward where the page was opened, so
    /// it is three-quarters on in the view the camera starts in and stays put
    /// as the camera goes round it.
    fn facing(&self) -> f32 {
        let to = self.home - self.at;
        if to.x.hypot(to.z) < 0.01 {
            return 0.0;
        }
        (-to.x).atan2(-to.z)
    }

    /// Stand the subject at the studio spot with nothing driving it: the
    /// model, posed, at the studio spot. What the spawner would have given it
    /// -- a gait, a mind, a place in a herd -- it does without.
    fn stand(&mut self, game: &mut Game, s: usize) {
        let model = game.ents.species[s].model;
        let ground = self.ground(game).unwrap_or(self.at.y);
        let at = Vec3::new(self.at.x, ground, self.at.z);
        let yaw = self.facing();
        let e = game.ents.world.spawn((crate::entities::Look::new(model, at, yaw), crate::creatures::Creature::of(s)));
        if let Some(rig) = game.models[model].rig.as_ref() {
            let _ = game.ents.world.insert_one(e, crate::anim::Anim::hold(crate::anim::Pose::rest(rig.bones.len())));
        }
    }

    /// Give the posed subject the animation its pose wants, in place of the
    /// held rest pose it was stood up with.
    fn repose(&mut self, game: &mut Game, pose: Pose, seed: u32) {
        let Some(s) = self.species else { return };
        if pose == Pose::Live {
            return;
        }
        let model = game.ents.species[s].model;
        let Some(e) = self.mine(game, s).into_iter().next() else { return };
        let built = super::pose::build(pose, &game.ents.species[s], &game.models[model], seed);
        if let Some(a) = built {
            let _ = game.ents.world.insert_one(e, a);
        }
    }

    /// Dress and pose the individual that is already standing there, without
    /// despawning it or touching a model: what a behaviour, variety, age,
    /// pose or roll change needs and no more. A LIVE subject is left to the
    /// world, which reads the patched species row by itself.
    pub fn restyle(&mut self, game: &mut Game, stage: crate::creatures::age::Stage, seed: u32, pose: Pose) {
        let Some(s) = self.species else { return };
        if self.waiting || self.mine(game, s).is_empty() {
            // Nothing standing yet: the spawn that is already being retried
            // will pick all of this up.
            self.showing = pose;
            return;
        }
        if (pose == Pose::Live) != (self.showing == Pose::Live) {
            // Between a pose and the living world is a different animal
            // altogether -- one has a body and a mind, the other has not.
            self.spawn(game, stage, seed, pose);
            return;
        }
        self.showing = pose;
        self.dress(game, s, stage, seed);
        if pose != Pose::Live {
            self.repose(game, pose, seed);
        }
    }

    /// Advance the posed subject's animation. Nothing else moves it, so this
    /// is the whole of what it does.
    pub fn pose_tick(&mut self, game: &mut Game, pose: Pose, dt: f32) {
        let Some(s) = self.species else { return };
        if pose == Pose::Live || !self.standing {
            return;
        }
        let model = game.ents.species[s].model;
        let yaw = self.facing();
        let at = self.at;
        let Some(e) = self.mine(game, s).into_iter().next() else { return };
        let Ok(mut anim) = game.ents.world.get::<&mut crate::anim::Anim>(e) else { return };
        super::pose::drive(&mut anim, &game.models[model], &game.ents.species[s], pose, at, yaw, dt);
    }

    /// Give every individual of the subject the page's age and roll, in place
    /// of the one the spawner would have rolled from its entity id
    /// (creatures/spawn.rs `roll_morphs`).
    fn dress(&self, game: &mut Game, s: usize, stage: crate::creatures::age::Stage, seed: u32) {
        let sp = &game.ents.species[s];
        let mut morph = crate::variety::Morph::roll(&sp.vary, crate::world::noise::hash32(seed | 1));
        let shares: Vec<f32> = sp.coats.iter().map(|c| c.share).collect();
        if let Some((which, at)) = crate::variety::pick_coat(
            &shares,
            crate::world::noise::unit(crate::world::noise::hash32(seed ^ 0xc0a7)),
            crate::world::noise::unit(crate::world::noise::hash32(seed ^ 0x5bad_e5)),
            |i| sp.coats[i].shades,
        ) {
            let wear = sp.coats[which].at(at);
            morph.wear(&wear);
        }
        morph.age(stage, &sp.ages, sp.aged_tint);
        let yaw = self.facing();
        let juvenile = stage == crate::creatures::age::Stage::Juvenile;
        let scale = morph.scale;
        for e in self.mine(game, s) {
            let _ = game.ents.world.insert_one(e, crate::creatures::age::Age::new(stage));
            let _ = game.ents.world.insert_one(e, morph.clone());
            if let Ok(mut l) = game.ents.world.get::<&mut crate::entities::Look>(e) {
                l.yaw = yaw;
                l.prev_yaw = yaw;
            }
            if juvenile {
                if let Ok(mut b) = game.ents.world.get::<&mut crate::physics::Body>(e) {
                    b.half_w *= scale;
                    b.height *= scale;
                }
            }
        }
    }

    /// Stand a block of one material on the platform, in place of any block
    /// before it. The floor is laid again, since the block is part of it.
    pub fn show_material(&mut self, game: &mut Game, id: u8) {
        if self.block == id && self.floored() {
            // The material's own look may have changed under the same id, so
            // the ground is drawn again even when nothing here moved.
            self.regen(game);
            return;
        }
        self.block = id;
        let was = self.floored();
        if was {
            self.floor_off(game);
        }
        self.floor_on(game);
        if !was {
            // A page showing a material always has the platform, since the
            // block has to stand on something.
            self.laid = true;
        }
    }

    /// Take the block away again.
    pub fn clear_material(&mut self, game: &mut Game) {
        if self.block == 0 {
            return;
        }
        self.block = 0;
        if self.floored() {
            self.floor_off(game);
            self.floor_on(game);
        }
    }

    /// Show one grown plant at the studio spot, in place of any before it.
    pub fn load_plant(&mut self, game: &mut Game, tree: Tree) {
        let r = tree.radius.max(2.0) + 4.0;
        let (x, z) = (tree.pos.x, tree.pos.z);
        {
            let mut list = game.streamer.generator().studio.write().unwrap();
            *list = vec![tree];
        }
        let p = game.player.pos;
        game.streamer.regen_area(x - r, z - r, x + r, z + r, p);
    }

    /// Take the plant away again.
    pub fn clear_plant(&mut self, game: &mut Game) {
        let gone: Vec<Tree> = {
            let mut list = game.streamer.generator().studio.write().unwrap();
            std::mem::take(&mut *list)
        };
        let p = game.player.pos;
        for t in gone {
            let r = t.radius.max(2.0) + 4.0;
            game.streamer.regen_area(t.pos.x - r, t.pos.z - r, t.pos.x + r, t.pos.z + r, p);
        }
    }

    /// Which individual is standing in the studio, if any. The headless
    /// checks watch this across an edit: a behaviour edit that despawns and
    /// spawns again is the blink this page was meant to lose.
    pub fn subject_entity(&self, game: &Game) -> Option<hecs::Entity> {
        let s = self.species?;
        self.mine(game, s).into_iter().next()
    }

    /// Where the subject has got to, if it is in the world at all.
    pub fn subject_at(&self, game: &Game) -> Option<Vec3> {
        let s = self.species?;
        let mut out = None;
        for (look, c) in game.ents.world.query::<(&crate::entities::Look, &crate::creatures::Creature)>().iter() {
            if c.species >= s {
                out = Some(look.pos);
                break;
            }
        }
        out
    }

    /// LIVE only: bring the subject back when it wanders out of the studio.
    /// Any other pose does not move, so there is nothing to hold.
    pub fn hold(&mut self, game: &mut Game, stage: crate::creatures::age::Stage, seed: u32, pose: Pose) -> bool {
        if pose != Pose::Live {
            return false;
        }
        let Some(at) = self.subject_at(game) else { return false };
        if (at.x - self.at.x).hypot(at.z - self.at.z) <= TETHER_M {
            return false;
        }
        self.spawn(game, stage, seed, pose);
        true
    }

    /// Put the world back as it was: no platform, no subject, no plant, and
    /// the player standing where they were with the view they had.
    pub fn close(&mut self, game: &mut Game) {
        game.set_editing(false);
        self.clear_material(game);
        self.clear_plant(game);
        self.drop_subject(game);
        self.floor_off(game);
        game.ride = std::mem::take(&mut self.was.ride);
        game.player.pos = self.was.pos;
        game.player.prev_pos = self.was.pos;
        game.player.yaw = self.was.yaw;
        game.player.pitch = self.was.pitch;
    }
}

/// The two greys the platform is drawn in. Most worlds reach `concrete` and
/// `road_asphalt` (worldload::ENGINE_MATERIALS), but a world that has run out
/// of voxel ids may have neither, and a studio with no floor is how the page
/// came to show nothing at all: so any two solid materials it does have will
/// do, and the grid lines read whatever they are drawn in.
fn greys(game: &Game) -> Option<(u8, u8)> {
    let want = |names: &[&str]| names.iter().find_map(|n| game.material(n));
    let pale = want(&["concrete", "pavement", "fieldstone", "limestone", "chalk"]);
    let dark = want(&["road_asphalt", "asphalt", "basalt", "slate", "bridge_steel"]);
    // Failing both, anything this world draws that a body cannot walk
    // through: an arena's deck plate is a better floor than no floor.
    let any = |skip: Option<u8>| -> Option<u8> {
        let lib = game.library()?;
        lib.materials
            .iter()
            .find(|m| m.id != 0 && Some(m.id) != skip && !m.water && !m.passable && m.foliage != Some(true) && m.coat_of.is_none())
            .map(|m| m.id)
    };
    let pale = pale.or_else(|| any(None))?;
    Some((pale, dark.or_else(|| any(Some(pale))).unwrap_or(pale)))
}

/// The first spot with ground under it: straight ahead if that will do,
/// else swinging either way and drawing in, which is what gets the studio out
/// of a wall or off a ledge in a built-up world.
pub fn spot_clear(game: &Game, from: Vec3, face: Vec3, away: f32) -> Vec3 {
    let here = from + face * away;
    let ground = |p: Vec3| voxel_ground(&game.store, p.x, p.z, game.store.top_m()).is_some();
    if ground(here) {
        return here;
    }
    for step in 1..6 {
        let d = away * (1.0 - step as f32 * 0.15);
        for turn in [0.4f32, -0.4, 0.9, -0.9, 1.6, -1.6] {
            let a = turn * step as f32 * 0.5;
            let dir = Vec3::new(face.x * a.cos() - face.z * a.sin(), 0.0, face.x * a.sin() + face.z * a.cos());
            let p = from + dir * d.max(2.5);
            if ground(p) {
                return p;
            }
        }
    }
    here
}
