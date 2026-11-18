//! The ways of playing a world, and which of them a world allows.
//!
//! A world lists its modes in world.json (`"modes": ["walk", "bike", ...]`);
//! a world that lists none allows them all. Walking is always allowed, since
//! every other mode starts and ends on foot. Movement modes (bike, glide,
//! climb, rope) live in ride.rs; screenshot mode is an activity layered over
//! walking (screenshot.rs). The camera (src/camera) is
//! the game's own mode, kept by Game rather than as an activity, since its film,
//! tasks and scrapbook outlive it.

/// One way of playing. `NAMES` gives each its world.json name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModeId {
    Walk,
    Bike,
    Glide,
    Climb,
    Rope,
    Kayak,
    Screenshot,
    Camera,
    /// Guns in hand: the shooter's rules (src/shooter.rs, src/combat).
    Shooter,
}

pub const ALL: [ModeId; 9] = [
    ModeId::Walk,
    ModeId::Bike,
    ModeId::Glide,
    ModeId::Climb,
    ModeId::Rope,
    ModeId::Kayak,
    ModeId::Screenshot,
    ModeId::Camera,
    ModeId::Shooter,
];

impl ModeId {
    pub fn name(self) -> &'static str {
        match self {
            ModeId::Walk => "walk",
            ModeId::Bike => "bike",
            ModeId::Glide => "glide",
            ModeId::Climb => "climb",
            ModeId::Rope => "rope",
            ModeId::Kayak => "kayak",
            ModeId::Screenshot => "screenshot",
            ModeId::Camera => "camera",
            ModeId::Shooter => "shooter",
        }
    }

    pub fn parse(s: &str) -> Option<ModeId> {
        ALL.into_iter().find(|m| m.name() == s)
    }
}

/// The set of modes a world allows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Allowed(u32);

impl Default for Allowed {
    fn default() -> Self {
        Allowed::all()
    }
}

impl Allowed {
    pub fn all() -> Self {
        Allowed(u32::MAX)
    }

    /// From a world's list; unknown names go to `unknown`. Walking is added.
    pub fn from_names(names: &[String], mut unknown: impl FnMut(&str)) -> Self {
        let mut bits = 1 << ModeId::Walk as u32;
        for n in names {
            match ModeId::parse(n) {
                Some(m) => bits |= 1 << m as u32,
                None => unknown(n),
            }
        }
        Allowed(bits)
    }

    /// The same set less one mode: what the launch screen's switches do.
    pub fn without(self, m: ModeId) -> Self {
        Allowed(self.0 & !(1 << m as u32))
    }

    pub fn has(self, m: ModeId) -> bool {
        self.0 & (1 << m as u32) != 0
    }
}

/// What an activity may read and set during its tick. The player is handed
/// over whole, because an activity may move them (a respawn) and push them
/// (a rocket jump); the creatures likewise, because a shot may hurt one.
pub struct ActivityCtx<'a> {
    pub player: &'a mut crate::player::Player,
    pub store: &'a crate::world::store::Store,
    pub ents: &'a mut crate::entities::Entities,
    pub models: &'a [crate::models::Model],
    /// The sea level, the world's water and whether the ground is drawn
    /// sloped: what `Player::update` needs, so an activity can move a body
    /// of its own through the same code the player goes through (src/brain).
    pub sea: f32,
    pub land: Option<&'a crate::world::fields::World>,
    pub slopes: bool,
    /// Set to save a screenshot of this frame (the window or headless code takes it).
    pub screenshot_request: &'a mut bool,
    /// Voxels an activity wants changed, applied by `Game` after the tick.
    ///
    /// An activity is handed the store to read and not to write, because the
    /// store, the streamer and the chunks the mesher is rebuilding are one
    /// thing and `Game` owns all of it. A weapon that digs a tunnel or lays
    /// a glob of sludge pushes its changes here instead, and they land
    /// together at the end of the tick. Nothing an activity writes this tick
    /// can be read back this tick, which is why anything that places voxels
    /// keeps its own account of what it put where.
    pub edits: &'a mut Vec<([i32; 3], u8)>,
}

impl ActivityCtx<'_> {
    /// The water surface over a point (a river's, else the sea) and the
    /// current there, as `Player::update` wants it. The same answer
    /// `Game::water_at` gives.
    pub fn water_at(&self, p: glam::Vec3) -> (f32, glam::Vec2) {
        crate::game::water_at(self.land, self.sea, self.store, p)
    }
}

/// A way of playing layered over the walking game: screenshot mode now, a race or
/// a shooter later. It may freeze the world, take the camera, use some
/// actions itself and draw its own HUD. The game runs at most one; to add
/// one, implement this, add a ModeId, and start it from Game::tick.
pub trait Activity: Send {
    fn id(&self) -> ModeId;
    /// Once a frame, before the frame's fixed steps: take the actions it
    /// uses out of `actions`, and do whatever is only for the view. A key
    /// press arrives once a frame and a frame may hold no fixed step or
    /// several, so a press taken in `step` would be lost or doubled; a held
    /// key is read here and kept for the steps.
    fn frame(&mut self, _cx: &mut ActivityCtx, _input: &crate::player::MoveInput, _actions: &mut crate::game::Actions, _dt: f32) {}
    /// One fixed step of `game::STEP`, on the same clock as the player's
    /// body and the world's moving parts, and run while the activity pauses
    /// the world too.
    fn step(&mut self, _cx: ActivityCtx, _input: &crate::player::MoveInput) {}
    /// The world stops while it runs, and actions it didn't take are dropped.
    fn pauses_world(&self) -> bool {
        false
    }
    /// The camera, when it takes the view over.
    fn camera(&self, _player: &crate::player::Player) -> Option<crate::rig::Camera> {
        None
    }
    /// Field of view as a multiple of the normal one.
    fn zoom(&self) -> f32 {
        1.0
    }
    /// Its HUD; `fov` is the vertical field of view in radians.
    fn hud(&self, _hud: &mut crate::render::hud::Hud, _w: f32, _h: f32, _s: f32, _fov: f32) {}
    /// Lines to draw into the world this frame (tracers, gates, sight lines).
    fn overlay(&self, _player: &crate::player::Player, _out: &mut crate::render::overlay::Overlay) {}
    /// Models to draw this frame, as `(index, instance)`. `base` is where
    /// this activity's own models start in `models` (Game::add_ride_models
    /// appends them). A gun held in the camera's frame goes here rather than
    /// on the HUD, so it takes the light of the room it is in.
    fn instances(&self, _player: &crate::player::Player, _alpha: f32, _base: usize, _models: &[crate::models::Model], _out: &mut Vec<(usize, crate::models::Instance)>) {}
    /// A line for the corner of the screen, taken once (a weapon's name, a
    /// death).
    fn note(&mut self) -> Option<String> {
        None
    }
    /// A screenshot of the view was just saved.
    fn shutter(&mut self) {}
    /// The shooter's rules, when this is one: how the headless tools read a
    /// match from outside without `Game` knowing what is running.
    fn shooter(&self) -> Option<&crate::shooter::Shooter> {
        None
    }
    /// The same, to change: `--no-player` has to take the player out of a
    /// headless match before it starts, and nothing else should.
    fn shooter_mut(&mut self) -> Option<&mut crate::shooter::Shooter> {
        None
    }
    /// Every body this activity moves itself (the bots), handed over whole
    /// so the world can carry one that is riding a platform. Whole, because
    /// a body is drawn interpolated from where it was: move it without its
    /// previous position and it is drawn trailing the lift it is standing
    /// on, and anything reading its speed thinks it sprinted.
    fn bodies(&mut self, _each: &mut dyn FnMut(&mut crate::player::Player)) {}
    /// The activity has started over and wants the damage done to the world
    /// during the last go undone (a new match in an arena). Taken once.
    fn wants_world_clean(&mut self) -> bool {
        false
    }

    /// Leaving: put back what it changed.
    fn exit(&mut self, _player: &mut crate::player::Player) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_list_allows_walking_and_what_it_names() {
        let mut bad = Vec::new();
        let a = Allowed::from_names(&["bike".into(), "jetpack".into()], |n| bad.push(n.to_string()));
        assert!(a.has(ModeId::Walk) && a.has(ModeId::Bike));
        assert!(!a.has(ModeId::Glide) && !a.has(ModeId::Shooter));
        assert_eq!(bad, ["jetpack"]);
        assert!(Allowed::default().has(ModeId::Rope));
    }
}
