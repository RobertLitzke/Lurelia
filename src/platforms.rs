//! Pieces of the world that move: lifts, drawbridges, pistons, a ferry.
//!
//! Nothing else in the engine moves world geometry. Terrain is meshed in
//! chunks and a chunk that changed has to be remeshed, which is fine for
//! digging a voxel and hopeless for a lift that climbs ten metres in three
//! seconds. So a platform is not terrain: it is a rigid box of voxels with a
//! position of its own, drawn as a model instance (the kayak and the bike are
//! drawn the same way) and made solid by handing the store a slab (see
//! `world::store::Slab`). Every body in the game asks the store what is
//! solid, so the player, the bots, the animals and anything shot all collide
//! with a platform without knowing platforms exist.
//!
//! A platform glides: it is drawn and collided where it really is, not where
//! the voxel grid would put it. A lift that moved in quarter-metre steps read
//! as a staircase however fast it went, which is not what a lift is.
//!
//! It collides where it is drawn, too. The slab handed to the store carries
//! two boxes: the cells wholly inside the plate, for anything that reads
//! voxels (a shot, the nav graph, a creature's feet), and the plate's exact
//! box in metres, which `physics::hits` tests every body against. Bodies
//! once had only the voxel copy, which in flight sits up to a quarter metre
//! below the deck: a rider fell onto the copy every step and was set back on
//! the deck the next, and rode the lift as a judder, and a bot was drawn
//! sunk into the plate (`physics::drawn_ground_lift` read the copy too).
//!
//! Riders are not left to the collision. Whoever is standing on one is moved
//! by the plate's own movement and set down on its surface, so feet stay on
//! the deck they can see all the way up. And a plate eases in and out of
//! each stop (`ACCEL`) rather than going from rest to full speed in a step.
//!
//! A world states its platforms in `world.json`:
//!
//! ```json
//! "platforms": [{
//!   "name": "lift_left",
//!   "at": [120.5, 88.0], "up": 0.0, "size": [3.0, 0.5, 3.0],
//!   "path": [[0, 10.5, 0]], "speed": 3.5, "wait": 2.0, "trigger": "stand"
//! }]
//! ```
//!
//! `at` and `up` place the top of it at home, in metres, `up` measured from
//! the floor the world's own spawn point stands on -- the same datum nav
//! nodes and pickup pads use. `path` is where it goes from there, as offsets
//! in metres; it travels the list in order and comes back the same way.
//! `trigger` says what starts it: `stand` (a body on it: a lift), `near` (a
//! body within `near_m`: a door) or `loop` (never stops).
//!
//! A door is not a kind of platform in here. It is a plate whose `path` runs
//! sideways on a `near` trigger, and two things in the stepping are what make
//! that read as a door rather than as a lift lying on its side:
//!
//! - `near` measures from where the plate sits at **home**, not from where
//!   the leaf now is. Measured from the leaf, the body that called the door
//!   falls out of range the moment it starts moving, so the door shuts on
//!   them, which puts them back in range: it chatters in the doorway instead
//!   of opening.
//! - At the far end of its path a `near` plate **holds** while it is still
//!   called. A lift does not wait for its rider to get off, because a lift
//!   that never comes back is a lift out of service; a door that shuts while
//!   you are standing in it is not a door.
//!
//! A world need not write its doors down. `doors_in` finds every box of
//! `door_*` material in the shapes the chronicle actually placed and hands
//! each one back as a plate, and `gen::draw_structures` leaves those voxels
//! out, so the doorway is an opening and the plate is the leaf standing in
//! it. That is what makes a door on a cottage open instead of eroding when
//! you point a pick at it. A door states its height in world metres
//! (`absolute`) rather than against the spawn floor, because a cottage on a
//! hillside knows its own ground and nothing else; and `settle` leaves such
//! a plate alone, since the rule that lifts a buried plate assumes a plate
//! is thin, and a door is as thick as it is tall. Only on a structure being
//! kept up: a ruin's door does not open. Lowmouth has 39 and Hemlow 14,
//! which is why plates that would come out identical share one model, and
//! why a leaf is painted as a leaf: the tread-and-rim scheme that makes a
//! lift read as a plate from above paints a dark band across the head of a
//! door, so a plate taller than it is thick gets a frame round its face
//! instead.

use crate::library::Library;
use crate::models::{self, Instance, Model};
use crate::world::material::STONE;
use crate::world::store::{Slab, Store};
use glam::Vec3;
use serde::Deserialize;

/// Voxels to the metre, and the other way round.
const VPM: f32 = 4.0;
const VOXEL_M: f32 = 0.25;
/// The widest a platform can be, in voxels: a model is a 32-voxel grid.
const MAX_VOX: i32 = 32;
/// A door: how fast the leaf slides, how long it holds after the last body
/// leaves, and how close you have to come for it to notice.
const DOOR_SPEED: f32 = 2.4;
const DOOR_WAIT: f32 = 0.6;
const DOOR_NEAR_M: f32 = 2.6;
/// Metres a second squared a platform speeds up and slows down at. A lift
/// that hits full speed in one step and stops dead in one more is a jolt at
/// each end that every rider feels through the camera; easing in and out is
/// what a mover in the maps these lifts come from did anyway. Fast enough
/// that an eight-metre-a-second lift still reaches its speed in half a
/// second.
const ACCEL: f32 = 16.0;
/// How far above a platform's surface a body still counts as standing on it.
/// A rider's feet sit exactly on it; the slack is for one carried a frame
/// late, and for the step a body takes as it walks on.
const RIDE_UP: f32 = 0.45;
/// And how far below, so a body being lifted by a floor arriving under it is
/// carried rather than left to be pushed out of it.
const RIDE_DOWN: f32 = 0.3;
/// How far above a plate's stated top to start looking for the ground it
/// rests on. Small on purpose: the question is only whether the plate is
/// inside the floor, and a wide search finds the storey above.
const FLOOR_SLACK: f32 = 0.6;
/// How many calls to `settle` a platform gets to find its ground before it
/// is placed at the height the world stated and left there.
const SETTLE_TRIES: u32 = 240;

/// One platform as a world states it.
#[derive(Deserialize, Clone, Debug)]
pub struct PlatformDef {
    /// What to call it in warnings, and what a bot route will ask for by
    /// name once the brain can wait for a lift (src/brain/nav.rs).
    #[serde(default)]
    pub name: String,
    /// The middle of it, metres, `[x, z]`.
    pub at: [f32; 2],
    /// Its top surface at home, metres above the floor the world's own spawn
    /// point stands on (as `NavNode::up` and `PickupDef::up` are measured).
    #[serde(default)]
    pub up: f32,
    /// Width, thickness and depth in metres. Thickness is what a rider
    /// stands on top of; half a metre reads as a plate.
    pub size: [f32; 3],
    /// Where it goes from home, as offsets in metres, in order. One entry is
    /// the usual lift: up and back down again.
    #[serde(default)]
    pub path: Vec<[f32; 3]>,
    /// Metres a second.
    #[serde(default = "default_speed")]
    pub speed: f32,
    /// Seconds it rests at each end of the path.
    #[serde(default = "default_wait")]
    pub wait: f32,
    /// `stand` (the default), `near` or `loop`.
    #[serde(default)]
    pub trigger: String,
    /// How close a body has to be for the `near` trigger, metres.
    #[serde(default = "default_near")]
    pub near_m: f32,
    /// `up` is world metres, not metres above the world's spawn floor.
    ///
    /// A world states a lift's height against the spawn floor because that
    /// is the datum everything hand-written shares. A door the chronicle
    /// put on a hillside knows its own absolute height and nothing else, so
    /// it says so. Not read from a world file: generated doors set it
    /// (`doors_in`).
    #[serde(skip)]
    pub absolute: bool,
    /// The library material it is made of, which decides what standing on it
    /// costs: a pan of slag carries its `hurt` wherever it goes. Rock when
    /// left out.
    #[serde(default)]
    pub material: Option<String>,
    /// Its colour, sRGB bytes. Platforms are drawn as models, which are flat
    /// colours rather than textures, so the colour is stated rather than
    /// taken from the material. Plate grey when left out.
    #[serde(default)]
    pub colour: Option<[f32; 3]>,
    /// Gives its own light, 0 to about 1.5, as a material's `glow` does.
    #[serde(default)]
    pub glow: f32,
}

fn default_speed() -> f32 {
    3.0
}

fn default_wait() -> f32 {
    2.0
}

fn default_near() -> f32 {
    3.0
}

/// What sets a platform going.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Trigger {
    /// A body standing on it: a lift. It waits at the far end and comes
    /// back, then stays home until it is called again.
    Stand,
    /// A body within `near_m` of it: a door, a bridge that lays itself.
    Near,
    /// Never stops.
    Loop,
}

impl Trigger {
    fn parse(s: &str) -> Option<Trigger> {
        match s {
            "" | "stand" => Some(Trigger::Stand),
            "near" => Some(Trigger::Near),
            "loop" => Some(Trigger::Loop),
            _ => None,
        }
    }
}

/// A platform compiled for a world: its box in voxels, its model, and the
/// state it is in.
pub struct Platform {
    pub name: String,
    /// Size in voxels.
    dims: [i32; 3],
    /// The lowest corner of the box at home, in voxels. `None` until the
    /// floor the world's heights are measured from is known (`settle`).
    home: Option<[i32; 3]>,
    /// Where it stands now: the lowest corner, in metres. Continuous, so it
    /// glides rather than stepping from voxel to voxel.
    at: Vec3,
    /// Home, and then every waypoint, as offsets from home in metres.
    way: Vec<Vec3>,
    /// How far along the way it is, metres from home along the path so far.
    gone: f32,
    /// The waypoint it is travelling toward.
    going_to: usize,
    /// Seconds left resting.
    resting: f32,
    /// How fast it is going along its path right now, metres a second:
    /// up to `speed`, eased in and out by `ACCEL`.
    pace: f32,
    /// Something is standing on it (or near it) right now.
    called: bool,
    /// How far it moved on the last step, metres. What a rider is carried by.
    pub moved: Vec3,
    /// Every metre it has travelled, and every time it has been called away
    /// from home: what the headless report reads, since a lift that never
    /// moved and a lift that moved perfectly look the same in a screenshot.
    pub travelled: f32,
    pub rides: u32,
    speed: f32,
    wait: f32,
    trigger: Trigger,
    near_m: f32,
    /// The material it is solid as, so its `hurt` and its footsteps are the
    /// material's and not a special case.
    mat: u8,
    /// Its model, as an index into the game's models.
    model: usize,
    /// Metres from the lowest corner of the box to where the model is drawn
    /// from (its pivot is the bottom centre of a 32-voxel grid).
    draw_off: Vec3,
}

impl Platform {
    /// Where its top surface is now, in metres.
    pub fn top_m(&self) -> f32 {
        self.at.y + self.dims[1] as f32 * VOXEL_M
    }

    /// The box it stands in now, metres: lowest corner and highest.
    fn box_m(&self) -> (Vec3, Vec3) {
        let hi = self.at + Vec3::new(self.dims[0] as f32, self.dims[1] as f32, self.dims[2] as f32) * VOXEL_M;
        (self.at, hi)
    }

    /// The middle of it, metres.
    pub fn centre_m(&self) -> Vec3 {
        let (lo, hi) = self.box_m();
        (lo + hi) * 0.5
    }

    /// The middle of where it sits when it is home, metres.
    ///
    /// This and not `centre_m` is what the `near` trigger asks about, and
    /// the difference is the whole of whether a door works. A door is a
    /// plate that slides out of its own doorway: measured from where the
    /// leaf now is, the body that called it falls out of range the moment
    /// it starts moving, so the door shuts on them, which puts them back in
    /// range, and it chatters in the frame instead of opening. Measured
    /// from the doorway, somebody standing in it holds it open, which is
    /// what a door is.
    fn home_centre_m(&self) -> Vec3 {
        let half = Vec3::new(self.dims[0] as f32, self.dims[1] as f32, self.dims[2] as f32) * VOXEL_M * 0.5;
        // `at` less how far along the way it has travelled is where it
        // started; before `settle` the two are the same.
        self.at - self.offset_m() + half
    }

    /// How far it has moved from home, metres.
    fn offset_m(&self) -> Vec3 {
        let mut left = self.gone;
        let mut off = Vec3::ZERO;
        for leg in self.way.windows(2) {
            let step = leg[1] - leg[0];
            let d = step.length();
            if left <= 0.0 || d <= 1e-6 {
                break;
            }
            let take = left.min(d);
            off += step / d * take;
            left -= take;
        }
        off
    }

    /// Is a body with its feet at `p` and half-width `hw` standing on it?
    fn carries(&self, p: Vec3, hw: f32) -> bool {
        let (lo, hi) = self.box_m();
        if p.y > hi.y + RIDE_UP || p.y < hi.y - RIDE_DOWN {
            return false;
        }
        p.x + hw > lo.x && p.x - hw < hi.x && p.z + hw > lo.z && p.z - hw < hi.z
    }

    /// The slab the store collides with: the cells wholly inside the plate,
    /// so it is never solid where it is not drawn. A plate in flight can lose
    /// a cell off each edge that way, and keeps at least its top row, because
    /// a floor you cannot stand on is not a floor.
    fn slab(&self) -> Slab {
        let (lo, hi) = self.box_m();
        let mut s = Slab {
            lo: [(lo.x * VPM).ceil() as i32, (lo.y * VPM).ceil() as i32, (lo.z * VPM).ceil() as i32],
            hi: [(hi.x * VPM).floor() as i32, (hi.y * VPM).floor() as i32, (hi.z * VPM).floor() as i32],
            mat: self.mat,
            lo_m: lo,
            hi_m: hi,
        };
        for a in 0..3 {
            if s.hi[a] <= s.lo[a] {
                s.lo[a] = s.hi[a] - 1;
            }
        }
        s
    }

    /// The offset `gone` metres along the path.
    fn along(&self) -> Vec3 {
        let mut left = self.gone;
        for w in self.way.windows(2) {
            let leg = w[0].distance(w[1]);
            if left <= leg || leg <= 0.0 {
                let f = if leg > 0.0 { left / leg } else { 0.0 };
                return w[0].lerp(w[1], f.clamp(0.0, 1.0));
            }
            left -= leg;
        }
        *self.way.last().unwrap_or(&Vec3::ZERO)
    }

    /// Metres along the path at waypoint `i`.
    fn mark(&self, i: usize) -> f32 {
        self.way.windows(2).take(i).map(|w| w[0].distance(w[1])).sum()
    }

    /// One step. `dt` seconds.
    fn step(&mut self, dt: f32) {
        let Some(home) = self.home else { return };
        let was = self.at;
        let home = Vec3::new(home[0] as f32, home[1] as f32, home[2] as f32) * VOXEL_M;
        if self.resting > 0.0 {
            self.resting -= dt;
        } else {
            let want = self.mark(self.going_to);
            // Eased: faster by `ACCEL` each second up to its speed, and no
            // faster than it can still stop in before the waypoint.
            let left = (want - self.gone).abs();
            self.pace = (self.pace + ACCEL * dt).min(self.speed).min((2.0 * ACCEL * left).sqrt());
            // Never so slow that the last centimetre takes forever.
            let reach = (self.pace * dt).max(0.05 * dt);
            let done = if want > self.gone {
                self.gone = (self.gone + reach).min(want);
                self.gone >= want
            } else if want < self.gone {
                self.gone = (self.gone - reach).max(want);
                self.gone <= want
            } else {
                true
            };
            if done {
                self.pace = 0.0;
                self.arrived();
            }
        }
        self.at = home + self.along();
        self.moved = self.at - was;
        self.travelled += self.moved.length();
    }

    /// It reached the waypoint it was going to: rest, then pick the next.
    fn arrived(&mut self) {
        let last = self.way.len() - 1;
        let home = self.going_to == 0;
        if home && self.trigger != Trigger::Loop {
            // Back where it started: wait to be called again, and don't
            // count the rest, so a lift answers the moment you stand on it.
            self.going_to = if self.called { last.max(1) } else { 0 };
            if self.going_to != 0 {
                self.rides += 1;
            }
            return;
        }
        // At the far end of the path. A door holds open while somebody is
        // still at it; a lift does not wait for its rider to get off,
        // because a lift that never comes back is a lift out of service.
        // `near` is the door trigger, so it is the one that holds.
        if self.trigger == Trigger::Near && self.called && self.going_to == last {
            return;
        }
        self.resting = self.wait;
        self.going_to = if self.going_to == last { 0 } else { last };
    }
}

/// Every platform in a world, and the floor its heights are measured from.
#[derive(Default)]
pub struct Platforms {
    pub list: Vec<Platform>,
    /// The floor the world's spawn point stands on, metres, once the game
    /// has found it (`WorldParts::floor`).
    base: Option<f32>,
    /// The defs, kept to place the platforms once the base is known.
    defs: Vec<PlatformDef>,
    /// Attempts left at finding the ground under a plate that has not been
    /// placed yet, while its chunks stream in.
    tries: u32,
}

impl Platforms {
    /// The platforms of a world. `built` is what `build` compiled, in the
    /// same order as the defs; `first_model` is where those models were put
    /// in the game's model list.
    pub fn new(defs: &[PlatformDef], built: &[Built], first_model: usize) -> Platforms {
        let list = defs
            .iter()
            .zip(built)
            .map(|(d, b)| {
                let dims = b.dims;
                // Waypoints sit on the voxel grid: a platform at rest is then
                // drawn and collided in exactly the same place, and only one
                // in flight gives up a quarter metre of its edge.
                let mut way = vec![Vec3::ZERO];
                way.extend(d.path.iter().map(|p| (Vec3::from(*p) * VPM).round() / VPM));
                Platform {
                    name: d.name.clone(),
                    dims,
                    home: None,
                    at: Vec3::ZERO,
                    way,
                    gone: 0.0,
                    going_to: 0,
                    resting: 0.0,
                    pace: 0.0,
                    called: false,
                    moved: Vec3::ZERO,
                    travelled: 0.0,
                    rides: 0,
                    speed: d.speed.max(0.01),
                    wait: d.wait.max(0.0),
                    trigger: Trigger::parse(&d.trigger).unwrap_or(Trigger::Stand),
                    near_m: d.near_m,
                    mat: b.mat,
                    model: first_model + b.model,
                    // The model's cells start at this corner of its grid, and
                    // it is drawn from the bottom centre (16, 0, 16).
                    draw_off: Vec3::new((MAX_VOX / 2 - dims[0] / 2) as f32 - 16.0, 0.0, (MAX_VOX / 2 - dims[2] / 2) as f32 - 16.0) * -VOXEL_M,
                }
            })
            .collect();
        Platforms {
            list,
            base: None,
            defs: defs.to_vec(),
            tries: SETTLE_TRIES,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Place every platform at home, once the floor the world measures its
    /// heights from is known (`WorldParts::floor`). Until then there is no
    /// slab and nothing is drawn, because at the moment a world starts only
    /// the chunks round the spawn have streamed in and a lift ten metres
    /// away is over space the store has never heard of.
    ///
    /// `floor` finds the solid surface below a point, as the nav graph's
    /// does, and is what keeps a plate out of the floor it rests on. A world
    /// states where the plate's *top* sits, and a lift that joins two storeys
    /// states the lower floor: the box then hangs below that line, inside the
    /// floor, and what you see is a plate buried to its lid, z-fighting with
    /// the deck over its whole face. That is what one imported map's two lifts looked
    /// like. So a plate whose underside would be below the ground under it is
    /// lifted to stand on that ground instead, by at most its own thickness,
    /// and its path is shortened by as much so the top of the ride still
    /// arrives at the floor it was built to reach.
    ///
    /// A platform settles on its own, the frame its column has streamed in,
    /// and takes the height the world stated if it is still waiting after
    /// `SETTLE_TRIES` -- the same bargain `Shooter::settle_nav` strikes.
    ///
    /// True on the call that placed something, so the caller can clear the
    /// grass out from under it (`keep_out`).
    pub fn settle(&mut self, base: f32, floor: impl Fn(f32, f32, f32) -> Option<f32>) -> bool {
        if self.list.is_empty() || self.list.iter().all(|p| p.home.is_some()) {
            return false;
        }
        let mut placed = false;
        self.base = Some(base);
        self.tries = self.tries.saturating_sub(1);
        let last = self.tries == 0;
        for (p, d) in self.list.iter_mut().zip(&self.defs) {
            if p.home.is_some() {
                continue;
            }
            // `up` places the top of it; the box hangs below that.
            let top_m = if d.absolute { d.up } else { base + d.up };
            let thick = p.dims[1] as f32 * VOXEL_M;
            let ground = floor(d.at[0], d.at[1], top_m + FLOOR_SLACK);
            if ground.is_none() && !last {
                continue;
            }
            // Only ever a lift, and never by more than the plate is thick:
            // this is here to take a plate out of the floor, not to second
            // guess where the world put it.
            //
            // And not at all for a plate that stated an absolute height.
            // That rule assumes a plate is thin, so the most it can be
            // wrong by is small; a door is as thick as it is tall, so the
            // same arithmetic would hang one two metres up in its own
            // doorway. A height measured from the world needs no rescuing.
            let rise = if d.absolute { 0.0 } else { ground.map_or(0.0, |g| (g - (top_m - thick)).clamp(0.0, thick)) };
            let top = ((top_m + rise) * VPM).round() as i32;
            let home = [((d.at[0] * VPM).round() as i32) - p.dims[0] / 2, top - p.dims[1], ((d.at[1] * VPM).round() as i32) - p.dims[2] / 2];
            // The ride ends where it ended before: a plate that starts half a
            // metre higher climbs half a metre less.
            if rise > 0.0 {
                for w in p.way.iter_mut().skip(1) {
                    w.y = (w.y - rise).max(0.0);
                }
            }
            p.home = Some(home);
            p.at = Vec3::new(home[0] as f32, home[1] as f32, home[2] as f32) * VOXEL_M;
            placed = true;
        }
        placed
    }

    /// The room each placed platform sweeps over its whole path, metres
    /// `[lo, hi]`: where no grass may grow (`sprites::set_keep_out`).
    pub fn keep_out(&self) -> Vec<[f32; 6]> {
        self.list
            .iter()
            .filter_map(|p| {
                let home = p.home?;
                let lo = Vec3::new(home[0] as f32, home[1] as f32, home[2] as f32) * VOXEL_M;
                let size = Vec3::new(p.dims[0] as f32, p.dims[1] as f32, p.dims[2] as f32) * VOXEL_M;
                let (mut a, mut b) = (lo, lo + size);
                for w in &p.way {
                    a = a.min(lo + *w);
                    b = b.max(lo + *w + size);
                }
                Some([a.x, a.y, a.z, b.x, b.y, b.z])
            })
            .collect()
    }

    /// Note a body, so the platforms it is on or near know they are wanted.
    /// Called for every body that can call a platform, before `step`.
    pub fn sense(&mut self, p: Vec3, hw: f32) {
        for plat in &mut self.list {
            if plat.called || plat.home.is_none() {
                continue;
            }
            plat.called = match plat.trigger {
                Trigger::Stand => plat.carries(p, hw),
                Trigger::Near => plat.home_centre_m().distance(p) <= plat.near_m,
                Trigger::Loop => true,
            };
        }
    }

    /// Move every platform and hand the store the slabs it should collide
    /// with. Call it before anything that moves a body, so a rider is
    /// carried by a floor that has already moved.
    pub fn step(&mut self, dt: f32, store: &mut Store) {
        if self.list.is_empty() {
            return;
        }
        for p in &mut self.list {
            if p.trigger == Trigger::Loop {
                p.called = true;
            }
            p.step(dt);
            // Being called lasts one step; a body still standing on it says
            // so again next step.
            p.called = false;
        }
        store.platforms.clear();
        store.platforms.extend(self.list.iter().filter(|p| p.home.is_some()).map(|p| p.slab()));
    }

    /// How far a body with its feet at `p` should be moved this step, which
    /// is zero unless it is standing on a platform that moved.
    ///
    /// It is the platform's own movement, plus however far the body had sunk
    /// into the deck: collision rounds the plate inward, so a body that
    /// walked onto a moving one can be a few centimetres low, and a rider
    /// belongs on the surface it can see. `falling` says the body is not on
    /// its way up, so that jumping off a lift is not undone.
    ///
    /// Bodies are drawn interpolated between steps, so a caller must move a
    /// body's previous position by this as well as its current one, or it is
    /// drawn trailing behind the lift it is riding.
    pub fn carry(&self, p: Vec3, hw: f32, falling: bool) -> Vec3 {
        for plat in &self.list {
            if plat.moved == Vec3::ZERO || plat.home.is_none() || !plat.carries(p, hw) {
                continue;
            }
            let mut by = plat.moved;
            let top = plat.top_m();
            let feet = p.y + by.y;
            if falling && feet < top && feet > top - RIDE_DOWN {
                by.y += top - feet;
            }
            return by;
        }
        Vec3::ZERO
    }

    /// Is a body at `p` being carried right now: on a platform that moved
    /// this step? A passenger presses no keys and is still going somewhere.
    pub fn riding(&self, p: Vec3, hw: f32) -> bool {
        self.list.iter().any(|plat| plat.moved != Vec3::ZERO && plat.home.is_some() && plat.carries(p, hw))
    }

    /// Where a rider being carried up by `up` metres should actually end up.
    ///
    /// A plate must not post its rider into a ceiling. A body inside solid
    /// voxels cannot move at all, and `physics::free_body` then shoves it up
    /// through whatever it is in rather than back down onto the plate, which
    /// is a bot standing waist-deep in a walkway for the rest of the match.
    /// Measured on an imported map: two of three bots spent steps buried at the top
    /// of the right lift's travel, at the lip of the landing it carries them
    /// to.
    ///
    /// So: the whole ride if it fits; else the whole ride a little to one
    /// side, which is how a body pressed against the lip of a landing ends
    /// up standing on it; else as much of the ride as fits; else none of it.
    pub fn clear_lift(&self, store: &crate::world::store::Store, pos: Vec3, up: f32, hw: f32, h: f32) -> Vec3 {
        let solid = |p: Vec3| crate::physics::hits(store, p, hw, h);
        let want = pos + Vec3::Y * up;
        if up <= 0.0 || !solid(want) {
            return want;
        }
        // Inward first. A shaft is barely wider than its plate, so a body
        // riding the corner of one clips the wall the whole way up, and the
        // place it belongs is the middle of the floor it is standing on --
        // not whichever side happens to be free, which on the way past a
        // doorway is off the plate altogether.
        let middle = self.list.iter().find(|q| q.carries(pos, hw)).map(|q| q.centre_m());
        const Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
        let mut out = [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (Q, Q), (Q, -Q), (-Q, Q), (-Q, -Q)];
        if let Some(m) = middle {
            let inward = |&(dx, dz): &(f32, f32)| {
                let to = Vec3::new(m.x - pos.x, 0.0, m.z - pos.z).normalize_or_zero();
                -(dx * to.x + dz * to.z)
            };
            out.sort_by(|a, b| inward(a).total_cmp(&inward(b)));
        }
        for k in 1..=4 {
            let d = k as f32 * 0.25;
            for (dx, dz) in out {
                let q = want + Vec3::new(dx * d, 0.0, dz * d);
                if !solid(q) {
                    return q;
                }
            }
        }
        // Nowhere to put it level with the landing: take as much of the lift
        // as there is headroom for, so the body rides up to the ceiling and
        // stops rather than entering it.
        let mut fits = 0.0;
        let mut d = up;
        while d > 0.05 {
            d -= 0.05;
            if !solid(pos + Vec3::Y * d) {
                fits = d;
                break;
            }
        }
        pos + Vec3::Y * fits
    }

    /// Every platform, where it stands and how far it has been: what the
    /// headless report prints, since a lift that never moved and one that
    /// worked perfectly look the same in a screenshot.
    pub fn report(&self) -> serde_json::Value {
        let rows: Vec<serde_json::Value> = self
            .list
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "top_m": (p.top_m() * 100.0).round() / 100.0,
                    "at": [(p.at.x * 100.0).round() / 100.0, (p.at.z * 100.0).round() / 100.0],
                    "travelled_m": (p.travelled * 10.0).round() / 10.0,
                    "rides": p.rides,
                    "moving": p.moved != Vec3::ZERO,
                })
            })
            .collect();
        serde_json::Value::Array(rows)
    }

    /// The platforms as drawn copies of their models.
    ///
    /// Drawn between the last step and this one, like every body is
    /// (`alpha`). The world steps thirty times a second and a lift climbs at
    /// six metres a second, so a plate drawn where the step left it jumps a
    /// fifth of a metre at a time: the ride read as bumping up a staircase
    /// even though the lift itself glides. `moved` is this step's movement,
    /// so the position it came from is `at - moved`.
    pub fn instances(&self, alpha: f32, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        for p in &self.list {
            if p.home.is_none() || models.get(p.model).is_none() {
                continue;
            }
            let at = p.at - p.moved * (1.0 - alpha.clamp(0.0, 1.0)) + p.draw_off;
            out.push((
                p.model,
                Instance {
                    pos_yaw: [at.x, at.y, at.z, 0.0],
                    info: [VOXEL_M, 16.0, 0.0, 16.0],
                    tint: [1.0, 1.0, 1.0, 0.0],
                    rot: [0.0; 4],
                    shift: [0.0; 4],
                },
            ));
        }
    }
}

/// A platform's model and material, compiled from its def.
pub struct Built {
    pub dims: [i32; 3],
    /// Index into the models `build` returned.
    pub model: usize,
    pub mat: u8,
}

/// Compile a world's platforms: a model each, the palette colours they need
/// (a block per model, appended to `colours`, which is the library models'
/// list of blocks and goes in the palette after models::FIXED_SLOTS), and the
/// material each is solid as. One model per platform, since each has its own
/// size.
pub fn build(defs: &[PlatformDef], lib: &Library, colours: &mut Vec<[f32; 4]>, problems: &mut Vec<String>) -> (Vec<Built>, Vec<Model>) {
    let mut built = Vec::new();
    let mut out = Vec::new();
    // A model is a 32-voxel grid, and a world with doors in it has one
    // platform per door: a village of forty cottages would be forty models
    // of the same plate. Plates that would come out identical share one.
    /// A plate's shape and paint, and the model built for it.
    type Made = ([i32; 3], u8, [f32; 3], f32, usize);
    let mut made: Vec<Made> = Vec::new();
    for d in defs {
        let who = if d.name.is_empty() { "a platform".to_string() } else { format!("platform {}", d.name) };
        if Trigger::parse(&d.trigger).is_none() {
            problems.push(format!("{who}: no trigger called {} (stand, near or loop)", d.trigger));
        }
        let mut dims = [0i32; 3];
        for (dim, size) in dims.iter_mut().zip(d.size) {
            let v = (size * VPM).round() as i32;
            if v > MAX_VOX {
                problems.push(format!("{who} is {size} m across, and a platform can be {} m at most", MAX_VOX as f32 * VOXEL_M));
            }
            *dim = v.clamp(1, MAX_VOX);
        }
        let mat = match &d.material {
            None => STONE,
            Some(name) => match lib.material(name) {
                Some(id) if id > 0 => id,
                _ => {
                    problems.push(format!("{who} asks for material {name}, which this world has not got"));
                    STONE
                }
            },
        };
        let rgb = d.colour.unwrap_or([122.0, 128.0, 136.0]);
        if let Some((.., model)) = made.iter().find(|(dm, m, c, g, _)| *dm == dims && *m == mat && *c == rgb && *g == d.glow) {
            built.push(Built { dims, model: *model, mat });
            continue;
        }
        // A plate reads as a plate because its tread is lighter than its
        // sides and its rim is darker: three colours, in a palette block of
        // the model's own.
        let start = colours.len();
        let slot = |colours: &mut Vec<[f32; 4]>, f: f32| -> u8 {
            let lin = models::linear_colour(&[rgb[0] * f, rgb[1] * f, rgb[2] * f, d.glow]);
            models::block_slot(colours, start, lin)
        };
        let (body, tread, rim) = (slot(colours, 1.0), slot(colours, 1.14), slot(colours, 0.72));
        // A plate you stand on and a leaf you walk through want different
        // paint. The tread-and-rim scheme is about a top face: the top row
        // is lighter in the middle and darker at the lip, which is what
        // makes a lift read as a plate from above. A door is taller than it
        // is thick and is only ever seen edge-on, where that same rule
        // paints a dark band across the head of it -- on a leaf one voxel
        // thick every cell is a rim cell, so the whole top row goes dark
        // and the door wears a cap. A leaf gets a frame round its face
        // instead, which is what a door has anyway.
        let leaf = dims[1] > dims[0].min(dims[2]);
        let mut cells = vec![0u8; 32 * 32 * 32];
        let (ox, oz) = (MAX_VOX / 2 - dims[0] / 2, MAX_VOX / 2 - dims[2] / 2);
        for y in 0..dims[1] {
            for z in 0..dims[2] {
                for x in 0..dims[0] {
                    let edge = x == 0 || z == 0 || x == dims[0] - 1 || z == dims[2] - 1;
                    // The border of the face it is seen from: along
                    // whichever of x and z it is wide in, and up its height.
                    let wide = if dims[0] >= dims[2] { x } else { z };
                    let across = if dims[0] >= dims[2] { dims[0] } else { dims[2] };
                    let framed = wide == 0 || wide == across - 1 || y == 0 || y == dims[1] - 1;
                    let c = if leaf {
                        if framed && across > 2 && dims[1] > 2 {
                            rim
                        } else {
                            body
                        }
                    } else if y < dims[1] - 1 {
                        body
                    } else if edge {
                        rim
                    } else {
                        tread
                    };
                    cells[(((y) * 32 + (z + oz)) * 32 + (x + ox)) as usize] = c;
                }
            }
        }
        let half_w = dims[0].max(dims[2]) as f32 * 0.5 * VOXEL_M;
        made.push((dims, mat, rgb, d.glow, out.len()));
        built.push(Built { dims, model: out.len(), mat });
        let mut model = models::from_cells(&cells, VOXEL_M, half_w, dims[1] as f32 * VOXEL_M);
        model.block = (models::FIXED_SLOTS + start) as u32;
        out.push(model);
    }
    (built, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lift(path: Vec<[f32; 3]>, trigger: &str) -> Platforms {
        let d = PlatformDef {
            name: "lift".into(),
            at: [4.0, 4.0],
            up: 0.0,
            size: [3.0, 0.5, 3.0],
            path,
            speed: 4.0,
            wait: 1.0,
            trigger: trigger.into(),
            near_m: 3.0,
            material: None,
            colour: None,
            glow: 0.0,
            absolute: false,
        };
        let built = [Built {
            dims: [12, 2, 12],
            model: 0,
            mat: STONE,
        }];
        let mut p = Platforms::new(&[d], &built, 0);
        // The floor is right under the plate's underside, so it settles
        // where it was asked to and nothing is lifted.
        p.settle(8.0, |_, _, _| Some(7.5));
        p
    }

    /// A world states where a plate's top sits, and on a lift joining two
    /// storeys that is the lower floor -- which would bury the box in it.
    /// The plate is stood on the floor instead, and the ride is shortened
    /// by as much so the top of it still arrives where it did.
    #[test]
    fn a_plate_is_not_left_inside_the_floor() {
        let d = PlatformDef {
            name: "lift".into(),
            at: [4.0, 4.0],
            up: 0.0,
            size: [3.0, 0.5, 3.0],
            path: vec![[0.0, 10.0, 0.0]],
            speed: 4.0,
            wait: 1.0,
            trigger: "stand".into(),
            near_m: 3.0,
            material: None,
            colour: None,
            glow: 0.0,
            absolute: false,
        };
        let built = [Built {
            dims: [12, 2, 12],
            model: 0,
            mat: STONE,
        }];
        let mut ps = Platforms::new(&[d], &built, 0);
        // The floor is level with the stated top: the plate's half metre
        // hangs below it, inside the deck.
        ps.settle(8.0, |_, _, _| Some(8.0));
        let top = ps.list[0].top_m();
        assert!((top - 8.5).abs() < 1e-4, "plate rests at {top}, not on the floor at 8");
        // And it still stops at 18, where the floor it serves is.
        let mut store = Store::default();
        let rider = Vec3::new(4.0, top, 4.0);
        let mut high = top;
        for _ in 0..600 {
            ps.sense(rider, 0.3);
            ps.step(1.0 / 60.0, &mut store);
            high = high.max(ps.list[0].top_m());
        }
        assert!((high - 18.0).abs() < 1e-3, "the ride ended at {high}");
    }

    /// A door: a plate whose path runs sideways, opened by a body coming
    /// near rather than standing on it. It holds open while somebody is
    /// there and shuts behind them.
    ///
    /// Doors are not a kind of platform in the code -- they are this
    /// arrangement of one, and the only reason the test exists is that
    /// nothing else in the engine says so.
    #[test]
    fn a_door_slides_aside_for_whoever_comes_to_it() {
        // Two metres to the left, and a body is "near" within three.
        let mut ps = lift(vec![[-2.0, 0.0, 0.0]], "near");
        let mut store = Store::default();
        let shut = ps.list[0].centre_m();
        // Nobody about: it stays shut, however long you wait.
        for _ in 0..600 {
            ps.sense(Vec3::new(40.0, 8.0, 40.0), 0.3);
            ps.step(1.0 / 60.0, &mut store);
        }
        assert!(ps.list[0].centre_m().distance(shut) < 1e-3, "it opened with nobody there");
        // Somebody walks up to it: it slides aside and holds.
        let caller = shut + Vec3::new(0.0, 0.0, 2.0);
        for _ in 0..600 {
            ps.sense(caller, 0.3);
            ps.step(1.0 / 60.0, &mut store);
        }
        let open = ps.list[0].centre_m();
        assert!((open.x - (shut.x - 2.0)).abs() < 1e-2, "it opened to {}, not two metres aside", open.x);
        assert!((open.y - shut.y).abs() < 1e-3, "a door should not have climbed: {open:?}");
        // And it shuts again once they have gone.
        for _ in 0..900 {
            ps.sense(Vec3::new(40.0, 8.0, 40.0), 0.3);
            ps.step(1.0 / 60.0, &mut store);
        }
        assert!(ps.list[0].centre_m().distance(shut) < 1e-2, "it stayed open after they left");
    }

    /// A door box found in a shape becomes a leaf standing in its own
    /// doorway: the right size, the right way round, and at the height the
    /// structure put it, not the height the spawn floor is at.
    #[test]
    fn a_door_in_a_shape_becomes_a_leaf_in_its_doorway() {
        use crate::world::shape::Shape;
        // A 6 x 4 x 6 block of wall with a 2-wide, 3-high, 1-deep doorway
        // in the z = 2 face, filled with door material (id 7).
        let size = [6, 4, 6];
        let n = (size[0] * size[1] * size[2]) as usize;
        let mut sh = Shape {
            size,
            mat: vec![1u8; n],
            part: vec![0u8; n],
            solid: n,
            top: 4,
            roof: vec![0; 36],
            col_top: vec![4; 36],
            threads: Vec::new(),
        };
        for y in 0..3 {
            for x in 2..4 {
                let i = sh.idx(x, y, 2).unwrap();
                sh.mat[i] = 7;
            }
        }
        let boxes = door_boxes(&sh, &[7]);
        assert_eq!(boxes.len(), 1, "found {} doors in one doorway", boxes.len());
        let (lo, hi) = boxes[0];
        assert_eq!((lo, hi), ([2, 0, 2], [4, 3, 3]), "the box is {lo:?}..{hi:?}");

        // Placed with its minimum corner at world voxel (40, 20, 80), unturned.
        let placed = crate::world::fields::Placed {
            shape: 0,
            def: 0,
            origin: [40, 20, 80],
            rot: 0,
            age: 0.0,
            cause: 0,
            seed: 1,
        };
        let d = door_def(&placed, &sh, lo, hi, "door_wood", [112.0, 58.0, 44.0]).expect("a door this size fits a platform");
        // Half a metre wide, three quarters tall, a quarter thick.
        assert_eq!(d.size, [0.5, 0.75, 0.25], "the leaf is {:?} m", d.size);
        // Centred on the doorway: voxels 42..44 in x is 10.5 to 11.0 m.
        assert!((d.at[0] - 10.75).abs() < 1e-4, "x {}", d.at[0]);
        assert!((d.at[1] - 20.625).abs() < 1e-4, "z {}", d.at[1]);
        // Its top is the world height of the opening, stated absolutely:
        // voxel 20 + 3 is 5.75 m, and nothing adds a spawn-floor datum.
        assert!(d.absolute && (d.up - 5.75).abs() < 1e-4, "up {}", d.up);
        // And it slides along the way it is wide, not through the wall.
        assert_eq!(d.path, vec![[0.5, 0.0, 0.0]], "it opens {:?}", d.path);
        assert_eq!(d.trigger, "near");
    }

    /// A lift stays home until somebody stands on it, climbs to the top of
    /// its path, waits, and comes back -- carrying its rider all the way,
    /// whose feet stay on the surface they can see.
    #[test]
    fn a_lift_answers_a_rider_and_returns() {
        let mut ps = lift(vec![[0.0, 10.0, 0.0]], "stand");
        let mut store = Store::default();
        let home = ps.list[0].top_m();
        assert!((home - 8.0).abs() < 1e-4, "top at {home}");
        // Nobody on it: it stays.
        for _ in 0..60 {
            ps.step(1.0 / 60.0, &mut store);
        }
        assert_eq!(ps.list[0].top_m(), home);
        assert_eq!(store.platforms.len(), 1);
        // Stand on it and it climbs. Ten metres at four a second: 2.5 s,
        // and a quarter second more for easing in and out.
        let mut rider = Vec3::new(4.0, home, 4.0);
        for _ in 0..175 {
            ps.sense(rider, 0.3);
            ps.step(1.0 / 60.0, &mut store);
            rider += ps.carry(rider, 0.3, true);
            assert!((rider.y - ps.list[0].top_m()).abs() < 1e-3, "rider at {} floor at {}", rider.y, ps.list[0].top_m());
        }
        assert!((ps.list[0].top_m() - (home + 10.0)).abs() < 1e-3, "climbed to {}", ps.list[0].top_m());
        // Let go: it waits a second and comes back down.
        for _ in 0..400 {
            ps.step(1.0 / 60.0, &mut store);
        }
        assert!((ps.list[0].top_m() - home).abs() < 1e-4, "came back to {}", ps.list[0].top_m());
        for _ in 0..120 {
            ps.step(1.0 / 60.0, &mut store);
        }
        assert!((ps.list[0].top_m() - home).abs() < 1e-4);
    }

    /// A rider under gravity and the real collision stays on the deck it can
    /// see every step of the way up and back down, and stays grounded, so it
    /// can jump off at any point. This is the ride that juddered: bodies
    /// collided with the voxel copy of the plate, which is up to a quarter
    /// metre below the drawn deck while it is between voxels, so each step
    /// the rider fell onto the copy and was set back on the deck.
    #[test]
    fn a_rider_stays_on_the_deck_all_the_way() {
        use crate::physics::{move_body, Body};
        let mut ps = lift(vec![[0.0, 6.0, 0.0]], "stand");
        let mut store = Store::default();
        for cz in -1..=1 {
            for cy in 0..=3 {
                for cx in -1..=1 {
                    store.chunks.insert((cx, cy, cz), crate::world::store::Voxels::Uniform(0));
                }
            }
        }
        let dt = 1.0 / 30.0;
        let home = ps.list[0].top_m();
        let mut pos = Vec3::new(4.0, home, 4.0);
        let mut body = Body::new(0.3, 1.75);
        let (mut worst, mut airborne, mut high, mut low_after) = (0.0f32, 0, 0.0f32, f32::MAX);
        for _ in 0..300 {
            ps.sense(pos, body.half_w);
            ps.step(dt, &mut store);
            let by = ps.carry(pos, body.half_w, body.vel.y <= 0.0);
            pos = ps.clear_lift(&store, pos, by.y, body.half_w, body.height);
            body.vel.y -= 24.0 * dt;
            let delta = body.vel * dt;
            move_body(&store, &mut pos, &mut body, delta, 0.5);
            let top = ps.list[0].top_m();
            high = high.max(top);
            if high > home + 5.9 {
                low_after = low_after.min(top);
            }
            worst = worst.max((pos.y - top).abs());
            airborne += (!body.on_ground) as u32;
        }
        assert!(high > home + 5.9, "the lift never went up: {high}");
        assert!((low_after - home).abs() < 1e-4, "it never came back down: {low_after}");
        assert!(worst < 1e-3, "the rider was {worst} m off the deck at worst");
        assert_eq!(airborne, 0, "the rider was off the ground for {airborne} steps");
    }

    /// It eases in and out of each stop rather than starting and stopping
    /// at full speed in a single step.
    #[test]
    fn it_eases_in_and_out() {
        let mut ps = lift(vec![[0.0, 10.0, 0.0]], "loop");
        let mut store = Store::default();
        let dt = 1.0 / 30.0;
        let mut speeds = Vec::new();
        for _ in 0..200 {
            ps.step(dt, &mut store);
            speeds.push(ps.list[0].moved.y / dt);
        }
        // Between any two steps it changes speed by no more than ACCEL allows.
        for w in speeds.windows(2) {
            // A little over on the step it starts braking, and the last
            // few centimetres into a stop are taken as they come.
            let slow = w[0].abs().max(w[1].abs()) < 1.5;
            assert!(slow || (w[1] - w[0]).abs() <= ACCEL * dt * 1.5, "it jumped from {} to {} m/s in a step", w[0], w[1]);
        }
        assert!(speeds.iter().any(|&v| (v - 4.0).abs() < 1e-3), "it never reached its speed");
    }

    /// It glides rather than stepping from voxel to voxel: a tenth of a
    /// second into a four-metre-a-second climb it has moved about 40 cm, not
    /// a round number of quarter metres.
    #[test]
    fn it_glides() {
        let mut ps = lift(vec![[0.0, 10.0, 0.0]], "loop");
        let mut store = Store::default();
        let home = ps.list[0].top_m();
        // It rests a second at each end of its path, this one included.
        for _ in 0..66 {
            ps.step(1.0 / 60.0, &mut store);
        }
        let risen = ps.list[0].top_m() - home;
        assert!(risen > 0.02 && risen < 0.5, "risen {risen} m a tenth of a second in, which is not a glide");
        assert!((risen * VPM).fract() > 1e-3, "it landed on the grid, so this proves nothing");
        // And every step moves it, rather than one step in four.
        for _ in 0..30 {
            ps.step(1.0 / 60.0, &mut store);
            assert!(ps.list[0].moved.y > 0.0, "it stepped instead of gliding");
        }
    }

    /// It is solid where it stands and nowhere else, and it carries its
    /// material with it. In flight it is never solid where it is not drawn.
    #[test]
    fn it_is_solid_where_it_stands() {
        let mut ps = lift(vec![[0.0, 4.0, 0.0]], "loop");
        let mut store = Store::default();
        store.chunks.insert((0, 0, 0), crate::world::store::Voxels::Uniform(0));
        store.chunks.insert((0, 1, 0), crate::world::store::Voxels::Uniform(0));
        ps.step(1.0 / 60.0, &mut store);
        // Its top is at 8 m (voxel 32), half a metre thick: voxels 30 and 31.
        assert!(store.solid(16, 31, 16), "not solid in the plate");
        assert!(store.solid(16, 30, 16));
        assert!(!store.solid(16, 32, 16), "solid above the plate");
        assert!(!store.solid(16, 29, 16), "solid below the plate");
        assert!(!store.solid(2, 31, 16), "solid outside the plate");
        assert_eq!(store.get(16, 31, 16), Some(STONE));
        // On the way up it is never solid above the surface it is drawn at.
        for _ in 0..90 {
            ps.step(1.0 / 60.0, &mut store);
            let top = ps.list[0].top_m();
            let over = (top * VPM).floor() as i32;
            assert!(!store.solid(16, over, 16), "solid at {over} with the top drawn at {top}");
        }
        assert!(!store.solid(16, 31, 16), "it left its old self behind");
        // And it took its solidity with it.
        let top = ps.list[0].top_m();
        assert!(store.solid(16, (top * VPM) as i32 - 1, 16), "nothing solid under its surface");
    }

    /// Only a body on top rides: one under it, beside it, or on another
    /// floor is left alone, and one on its way up is not pulled back down.
    #[test]
    fn it_carries_only_what_stands_on_it() {
        let mut ps = lift(vec![[0.0, 10.0, 0.0]], "loop");
        let mut store = Store::default();
        for _ in 0..70 {
            ps.step(1.0 / 60.0, &mut store);
        }
        let top = ps.list[0].top_m();
        assert_ne!(ps.list[0].moved, Vec3::ZERO, "it never set off");
        assert_ne!(ps.carry(Vec3::new(4.0, top, 4.0), 0.3, true), Vec3::ZERO, "the rider was not carried");
        assert_eq!(ps.carry(Vec3::new(4.0, top - 3.0, 4.0), 0.3, true), Vec3::ZERO, "something underneath was carried");
        assert_eq!(ps.carry(Vec3::new(4.0, top + 3.0, 4.0), 0.3, true), Vec3::ZERO, "something on the floor above was carried");
        assert_eq!(ps.carry(Vec3::new(12.0, top, 4.0), 0.3, true), Vec3::ZERO, "something beside it was carried");
        // A body that has jumped is moved with the lift but not set down on
        // it, which would be the lift undoing the jump.
        let jumped = ps.carry(Vec3::new(4.0, top + 0.2, 4.0), 0.3, false);
        assert_eq!(jumped, ps.list[0].moved, "a jump was pulled back onto the deck");
    }

    /// A lift must not post its rider into a ceiling. The plate here rises
    /// into a slab with a hole in it off to one side: the ride carries the
    /// body up through the hole rather than into the underside, which is
    /// what left bots standing inside an imported map's landing.
    #[test]
    fn a_lift_does_not_carry_a_body_into_the_ceiling() {
        let ps = lift(vec![[0.0, 10.0, 0.0]], "loop");
        let mut store = Store::default();
        for cz in -1..=1 {
            for cy in 0..=2 {
                for cx in -1..=1 {
                    store.chunks.insert((cx, cy, cz), crate::world::store::Voxels::Uniform(0));
                }
            }
        }
        // A ceiling at 12 m with a hole in it over the middle of the plate.
        for x in 0..40 {
            for z in 0..40 {
                for y in 48..52 {
                    store.set(x, y, z, STONE);
                }
            }
        }
        for x in 10..20 {
            for z in 10..24 {
                for y in 48..52 {
                    store.set(x, y, z, 0);
                }
            }
        }
        let (hw, h) = (0.3, 1.75);
        // A body riding the far edge of the plate, where its head goes into
        // the solid part: the lift has to put it under the hole instead,
        // which is inward, because the middle of the plate is the one place
        // on it that is certainly floor.
        let edge = Vec3::new(5.2, 10.2, 4.0);
        assert!(!crate::physics::hits(&store, edge, hw, h), "the body did not start clear");
        assert!(crate::physics::hits(&store, edge + Vec3::Y * 0.25, hw, h), "the ceiling was not in the way");
        let out = ps.clear_lift(&store, edge, 0.25, hw, h);
        assert!(!crate::physics::hits(&store, out, hw, h), "the lift posted the body into the ceiling: {out:?}");
        assert!(out.y > edge.y, "it did not ride at all: {out:?}");
        assert!(out.x < edge.x, "it was moved away from the middle of the plate: {out:?}");
        // And where the way is clear the whole ride is taken, with nothing
        // moved sideways.
        let clear = Vec3::new(4.0, 8.0, 4.0);
        assert_eq!(ps.clear_lift(&store, clear, 0.25, hw, h), clear + Vec3::Y * 0.25);
    }

    /// A platform with no path is still a platform: it stands there.
    #[test]
    fn a_platform_with_nowhere_to_go_stands_still() {
        let mut ps = lift(vec![], "stand");
        let mut store = Store::default();
        let home = ps.list[0].top_m();
        for _ in 0..120 {
            ps.sense(Vec3::new(4.0, home, 4.0), 0.3);
            ps.step(1.0 / 60.0, &mut store);
        }
        assert_eq!(ps.list[0].top_m(), home);
        assert_eq!(store.platforms.len(), 1);
    }

    /// Fifty plates of fifty colours want 150 palette colours, more than
    /// the 128 a whole world used to get. Each plate's model has a block of
    /// its own, so the first and the last both read back their own paint,
    /// looked up the way `vs_entity` does it.
    #[test]
    fn every_plate_reads_its_own_colours_past_128() {
        let defs: Vec<PlatformDef> = (0..50)
            .map(|k| PlatformDef {
                name: format!("plate{k}"),
                at: [4.0, 4.0],
                up: 0.0,
                size: [1.0, 0.5, 1.0],
                path: vec![],
                speed: 4.0,
                wait: 1.0,
                trigger: "stand".into(),
                near_m: 3.0,
                material: None,
                colour: Some([40.0 + k as f32 * 4.0, 90.0, 200.0 - k as f32 * 3.0]),
                glow: 0.0,
                absolute: false,
            })
            .collect();
        let mut colours = Vec::new();
        let mut problems = Vec::new();
        let (_, out) = build(&defs, &Library::default(), &mut colours, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(out.len(), 50);
        assert!(colours.len() > 128, "only {} colours", colours.len());
        // The palette as the game lays it out: the fixed block, then these.
        let mut palette = models::palette();
        palette.extend_from_slice(&colours);
        for (k, d) in defs.iter().enumerate() {
            let rgb = d.colour.unwrap();
            let want: Vec<[f32; 4]> = [1.0, 1.14, 0.72].iter().map(|f| models::linear_colour(&[rgb[0] * f, rgb[1] * f, rgb[2] * f, 0.0])).collect();
            let m = &out[k];
            assert!(!m.mesh.quads.is_empty());
            for q in &m.mesh.quads {
                let (mat, base) = (q[1] & 255, crate::world::material::PALETTE_BASE as u32);
                let row = m.block as usize + (mat.max(base) - base) as usize;
                assert!(want.contains(&palette[row]), "plate {k} reads {:?}", palette[row]);
            }
        }
    }
}

/// Every door the chronicle put on the land, as platforms.
///
/// A door in a structure is a box of `door_*` material in its shape, which
/// until now was terrain: you could not open it, and digging eroded it. This
/// finds those boxes in the shapes the world actually placed and hands each
/// one back as a plate that slides aside for whoever comes to it. The
/// generator leaves the voxels out (`gen::draw_structures`), so the doorway
/// is an opening and the plate is the leaf standing in it.
///
/// Only on a structure that is being kept up. A door hanging off a ruin is
/// scenery, and a ruin whose door slides smoothly aside is a worse lie than
/// one whose door is stuck.
pub fn doors_in(world: &crate::world::fields::World) -> Vec<PlatformDef> {
    let ids = door_ids(&world.lib);
    let mut out = Vec::new();
    if ids.is_empty() {
        return out;
    }
    for p in &world.placed {
        if p.age > 0.0 {
            continue;
        }
        let s = &world.shapes[p.shape];
        for (lo, hi) in door_boxes(s, &ids) {
            let Some(leaf) = world.lib.materials.iter().find(|m| world.lib.material(&m.name) == Some(s.get(lo[0], lo[1], lo[2]).0)) else {
                continue;
            };
            if let Some(d) = door_def(p, s, lo, hi, &leaf.name, leaf.texture.base) {
                out.push(d);
            }
        }
    }
    out
}

/// The material ids that are a door leaf. The library names them `door_*`,
/// and a structure file says `"mat": "door"`, which the slot resolves to one
/// of them, so this needs no new field in the library.
fn door_ids(lib: &Library) -> Vec<u8> {
    lib.materials.iter().filter(|m| m.name.starts_with("door_")).filter_map(|m| lib.material(&m.name)).collect()
}

/// One door box, placed in the world.
///
/// It slides along its own wider horizontal axis -- a leaf is wide and thin,
/// and the way it opens is the way it is wide -- by its own width, so it
/// ends up clear of the opening it stood in.
fn door_def(p: &crate::world::fields::Placed, s: &crate::world::shape::Shape, lo: [i32; 3], hi: [i32; 3], leaf: &str, paint: [f32; 3]) -> Option<PlatformDef> {
    use crate::world::shape::rotate;
    // The box's two x/z corners, turned the way the structure was placed.
    let (ax, az) = rotate(lo[0], lo[2], s.size, p.rot);
    let (bx, bz) = rotate(hi[0] - 1, hi[2] - 1, s.size, p.rot);
    let (x0, x1) = (ax.min(bx), ax.max(bx) + 1);
    let (z0, z1) = (az.min(bz), az.max(bz) + 1);
    let dims = [x1 - x0, hi[1] - lo[1], z1 - z0];
    if dims.iter().any(|&d| d <= 0 || d > MAX_VOX) {
        return None;
    }
    let world = |v: i32| v as f32 * VOXEL_M;
    let at = [world(2 * p.origin[0] + x0 + x1) * 0.5, world(2 * p.origin[2] + z0 + z1) * 0.5];
    let up = world(p.origin[1] + hi[1]);
    // Sideways by its own width, along whichever of x and z it is wider in.
    let path = if dims[0] >= dims[2] { [world(dims[0]), 0.0, 0.0] } else { [0.0, 0.0, world(dims[2])] };
    Some(PlatformDef {
        name: "door".into(),
        at,
        up,
        size: [world(dims[0]), world(dims[1]), world(dims[2])],
        path: vec![path],
        speed: DOOR_SPEED,
        wait: DOOR_WAIT,
        trigger: "near".into(),
        near_m: DOOR_NEAR_M,
        // Its own material and its own paint: a door drawn as a grey stone
        // plate is a slab in a doorway, not a door.
        material: Some(leaf.to_string()),
        colour: Some(paint),
        glow: 0.0,
        absolute: true,
    })
}

/// The boxes of door material in a shape, in local voxels: lowest corner and
/// one past the highest. Each run of door voxels that touches is one door;
/// a cottage has one and a hall may have several.
fn door_boxes(s: &crate::world::shape::Shape, ids: &[u8]) -> Vec<([i32; 3], [i32; 3])> {
    let is_door = |m: u8| ids.contains(&m);
    let [sx, sy, sz] = s.size;
    let mut seen = vec![false; (sx * sy * sz) as usize];
    let mut out = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let Some(i) = s.idx(x, y, z) else { continue };
                if seen[i] || !is_door(s.get(x, y, z).0) {
                    continue;
                }
                // Flood the run of touching door voxels and take its box.
                let (mut lo, mut hi) = ([x, y, z], [x + 1, y + 1, z + 1]);
                let mut stack = vec![[x, y, z]];
                seen[i] = true;
                while let Some([cx, cy, cz]) = stack.pop() {
                    for k in 0..3 {
                        lo[k] = lo[k].min([cx, cy, cz][k]);
                        hi[k] = hi[k].max([cx, cy, cz][k] + 1);
                    }
                    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                        let n = [cx + dx, cy + dy, cz + dz];
                        let Some(j) = s.idx(n[0], n[1], n[2]) else { continue };
                        if !seen[j] && is_door(s.get(n[0], n[1], n[2]).0) {
                            seen[j] = true;
                            stack.push(n);
                        }
                    }
                }
                out.push((lo, hi));
            }
        }
    }
    out
}
