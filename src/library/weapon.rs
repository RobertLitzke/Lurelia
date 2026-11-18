//! Weapons: guns, their fire modes, splashes, clusters and spades.

#[allow(unused_imports)]
use super::*;

/// How a fire mode throws its damage.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FireKind {
    /// A ray, resolved the instant the trigger goes.
    Hitscan,
    /// A body that flies, drops and bursts on what it touches.
    Shell,
    /// A ray held on a target rather than fired: damage is per second and
    /// the beam is resolved afresh every step the trigger is down. The
    /// pulse gun's secondary.
    Beam,
}

/// The spade on the end of a weapon: what it takes out of the wall it is
/// swung at, and how fast. Only the Digger has one.
///
/// A gun with a spade cuts the world while its trigger is held, on a clock
/// of its own rather than the fire mode's: the Digger's blast goes off at 84
/// rounds a minute and the spade bites at 430, which is the rate a shovel
/// works at out in the world. Whether what it bites is allowed to go is
/// `Game`'s business, not the gun's, the same as a bore.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct DigDef {
    /// Radius of one bite, metres.
    pub radius: f32,
    /// Bites a minute while the trigger is held.
    pub rpm: f32,
    /// How far the spade reaches, metres.
    pub range: f32,
}

/// A burst: what it does to everything round where it went off.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct SplashDef {
    /// Damage at the centre, falling to nothing at the rim.
    pub damage: f32,
    pub radius: f32,
    /// Metres per second given to whoever is caught, at the centre.
    #[serde(default)]
    pub push: f32,
    /// How much of its own burst the shooter takes: 0 none, 1 all of it.
    /// Under 1 on the damage but not the push is what makes a rocket jump
    /// worth doing.
    #[serde(default = "default_one")]
    pub self_damage: f32,
    #[serde(default = "default_one")]
    pub self_push: f32,
    /// Radius in metres of the ball of world the burst takes out where it
    /// goes off. 0, the default, leaves the world alone. 0.33 takes about
    /// four quarter-metre voxels out of a flat floor.
    #[serde(default)]
    pub crater: f32,
    /// How the damage falls from the centre to the rim: the share left at a
    /// fraction x of the way out is (1 - x) to this power. 1, the default,
    /// is a straight line; above 1 the centre hits as hard and the rim
    /// still reaches as far, but the damage dies away sooner in between.
    #[serde(default = "default_one")]
    pub curve: f32,
}

/// What a shell breaks into where it lands: a handful of little ones thrown
/// out of it, each of which flies, bounces and bursts on its own.
///
/// The parent still does whatever its own `damage` and `splash` say; this is
/// what happens next. A shell with a `cluster` is the one shot in the arena
/// you lob rather than aim, because what hurts is where the children end up
/// and not where the shell landed.
#[derive(Deserialize, Clone, Debug)]
pub struct ClusterDef {
    /// How many come out.
    pub count: u32,
    /// What one of them does on a direct hit.
    pub damage: f32,
    /// Metres a second they are thrown at, and how hard they drop.
    pub speed: f32,
    #[serde(default = "default_gravity")]
    pub gravity: f32,
    /// Half-angle of the spray, degrees, measured off the way the parent
    /// came: a wide one scatters them round the room, a narrow one keeps
    /// them together.
    #[serde(default = "default_cluster_cone")]
    pub cone_deg: f32,
    pub bounces: u32,
    pub fuse_s: f32,
    #[serde(default)]
    pub splash: Option<SplashDef>,
    /// Which little body each is drawn as (`combat::viewmodel::SHELLS`).
    #[serde(default)]
    pub look: String,
}

pub(super) fn default_gravity() -> f32 {
    9.0
}

pub(super) fn default_cluster_cone() -> f32 {
    75.0
}

/// One fire mode of a weapon: the left button (`fire`) or the right (`alt`).
#[derive(Deserialize, Clone, Debug)]
pub struct FireDef {
    pub kind: FireKind,
    /// Rounds a minute: the cadence while the trigger is held.
    pub rpm: f32,
    /// Damage per pellet, or on a direct hit for a shell.
    pub damage: f32,
    /// Rays per shot (buckshot); ignored by shells.
    #[serde(default = "default_one_u")]
    pub pellets: u32,
    /// Half-angle of the cone the pellets go into, degrees.
    #[serde(default)]
    pub spread_deg: f32,
    /// How far a ray reaches, metres.
    #[serde(default = "default_range")]
    pub range: f32,
    /// What share of the damage is left at `range` (1 keeps it all).
    #[serde(default = "default_one")]
    pub falloff: f32,
    /// Seconds of held trigger before the full rate is reached (the minigun).
    #[serde(default)]
    pub spin_up: f32,
    /// Shells: how fast it flies (m/s), how hard it drops (m/s^2), and how
    /// many times it may bounce before it goes off.
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub gravity: f32,
    #[serde(default)]
    pub bounces: u32,
    /// Of its speed, what a shell keeps through each bounce. A grenade off
    /// a wall keeps under half; a skipjack disc is made to ricochet, and one
    /// that kept only that much crawled after its second bounce and ran its
    /// fuse out hanging in the air.
    #[serde(default = "default_bounce_keep")]
    pub bounce_keep: f32,
    /// Shells: the share of its damage a shell does to whoever fired it,
    /// once it has come off a wall. 0, the default, is a shell that passes
    /// through its owner. The skipjack's discs come back at you at half.
    #[serde(default)]
    pub self_hit: f32,
    /// Seconds after which a shell goes off wherever it is.
    #[serde(default)]
    pub fuse_s: f32,
    #[serde(default)]
    pub splash: Option<SplashDef>,
    /// Rounds spent per shot.
    #[serde(default = "default_one_u")]
    pub per_shot: u32,
    /// Degrees the view climbs per shot.
    #[serde(default)]
    pub kick: f32,
    /// Tracer and shell colour, 0..1 linear; white when left out.
    #[serde(default = "default_tracer")]
    pub tracer: [f32; 3],
    /// Seconds a tracer stays up.
    #[serde(default = "default_tracer_s")]
    pub tracer_s: f32,
    /// Seconds of held trigger that build a full charge. 0 is a mode that
    /// fires the instant the trigger goes down; over 0 holds the shot back
    /// and lets it go when the trigger is released or the charge is full,
    /// which is the goo gun's secondary.
    #[serde(default)]
    pub charge_s: f32,
    /// What a full charge is worth: damage, burst and speed are all scaled
    /// between 1 at a tap and this at full. Ignored without `charge_s`.
    #[serde(default = "default_one")]
    pub charge_mult: f32,
    /// Shells: seconds a shell clings to whatever it lands on instead of
    /// going off there. A stuck shell goes off when somebody comes within
    /// reach of it or when the time runs out -- the goo gun's sludge.
    #[serde(default)]
    pub stick_s: f32,
    /// Rays: what the damage is multiplied by for a hit in the top of the
    /// body. 1 is a weapon that does not care where it lands.
    #[serde(default = "default_one")]
    pub head_mult: f32,
    /// Rays: metres of tunnel this one cuts through the world as it goes.
    /// 0 is a ray that leaves the walls where they are, which is every other
    /// weapon. A mode with a `bore_r` does not stop on a wall -- it eats it
    /// and carries on to its full `range`, which is the whole point of the
    /// thing: a map with a bore in it has no permanent sightlines.
    #[serde(default)]
    pub bore_r: f32,
    /// Beams: turns a second of the thread of light drawn round the beam
    /// (`Combat::twist`). 0 is a plain rope, which is every other beam; over
    /// 0 is something turning, which is what a drill and a plasma stream
    /// both are.
    #[serde(default)]
    pub twist: f32,
    /// Beams: how many times the beam jumps on from the body it is held on
    /// to the nearest other body, within `arc_m` metres and in sight of the
    /// last, and the share of its damage it keeps at each jump. The pulse
    /// gun's right button: held on one of a crowd, it is in all of them.
    /// 0 jumps is a beam that stops where it lands.
    #[serde(default)]
    pub arc_jumps: u32,
    #[serde(default)]
    pub arc_m: f32,
    #[serde(default = "default_arc_keep")]
    pub arc_keep: f32,
    /// Shells: what this one breaks into where it lands. None is every
    /// other shell, which simply goes off.
    #[serde(default)]
    pub cluster: Option<ClusterDef>,
    /// Shells: metres of real sludge voxels the glob lays down where it
    /// lands, instead of hanging there as a model. 0 is every other shell.
    ///
    /// A glob that finds a body still bursts on it. One that finds a wall or
    /// the floor becomes matter: it clings where it touches, falls when
    /// nothing holds it up, stacks on other globs, and sets them off when it
    /// goes (`combat::sludge`). `stick_s` is its fuse, and its `splash` is
    /// what it does when the fuse runs out or somebody walks into it.
    #[serde(default)]
    pub goo_r: f32,
    /// Shells: a block of the map's spoil (`ShooterDef::spoil`, its bedrock
    /// unless it says otherwise), this many metres across, laid down as real
    /// voxels where the shell lands. The shell flies as a block of stone and
    /// becomes one: the bore's right button, which throws back out of the
    /// gun what the left button cut out of the walls. One that finds a body
    /// shatters on it rather than burying it. 0 is every other shell.
    #[serde(default)]
    pub spoil_r: f32,
    /// Rounds per burst. 0 or 1 is a mode that fires on its `rpm` for as
    /// long as the trigger is held; over 1 fires that many at `rpm` and then
    /// waits `burst_gap_s` before the next burst. A burst that has started
    /// finishes even if the button comes up mid-way, so a tap is a whole
    /// burst (`combat::weapon::Held::volley`).
    #[serde(default)]
    pub burst: u32,
    /// Seconds between the last round of one burst and the first of the
    /// next, on top of the ordinary `rpm` gap.
    #[serde(default)]
    pub burst_gap_s: f32,
    /// Which little voxel body is drawn for a shell in flight
    /// (`combat::viewmodel::SHELLS`): "rocket", "grenade" or "shard". Left
    /// out, it is worked out from the mode: a bouncing shell is a grenade, a
    /// shell with a burst a rocket, one without a shard.
    #[serde(default)]
    pub look: String,
}

/// One gun. A world hands some of them out (`"shooter"` in world.json); the
/// rules that decide what happens when you die are not in here (src/shooter.rs).
#[derive(Deserialize, Clone, Debug)]
pub struct WeaponDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// What the HUD calls it.
    pub display: String,
    /// Which digit picks it, 1 to 9.
    pub slot: u32,
    /// Rounds the player may carry. 0 is a weapon that needs no ammunition
    /// at all and can never be emptied, which is what the Digger is:
    /// there is nothing in it to run out of.
    pub ammo_max: u32,
    /// Rounds handed out with the weapon.
    #[serde(default)]
    pub ammo_start: u32,
    /// Rounds the gun holds before it has to be reloaded. 0 is a weapon fed
    /// straight from its store, which is what the minigun's belt is: its
    /// spin-up is the wait it has instead of a reload.
    #[serde(default)]
    pub mag: u32,
    /// Seconds a reload takes. Ignored without a `mag`.
    #[serde(default)]
    pub reload_s: f32,
    pub fire: FireDef,
    #[serde(default)]
    pub alt: Option<FireDef>,
    /// How far the right button narrows the view, as a share of the normal
    /// field of view: 0.33 is three times magnification. Over 0 the right
    /// button is a sight rather than a second fire mode, which is what the
    /// sniper rifle's is.
    #[serde(default)]
    pub zoom: f32,
    /// A spade on the end of it, which cuts the world while the left button
    /// is held (`DigDef`). None is every weapon but the Digger.
    #[serde(default)]
    pub dig: Option<DigDef>,
    /// Which built-in shape is drawn in the player's hands
    /// (`combat::viewmodel::SHAPES`). The shapes are Rust, not data, so a
    /// weapon file picks one rather than describing one.
    #[serde(default)]
    pub view: String,
    /// The flat shape the HUD draws for the gun in the corner: boxes
    /// `[x, y, w, h]`, each a share of the panel, x right from its left edge
    /// and y up from the bottom. Drawn only when the view-model is off.
    #[serde(default)]
    pub grip: Vec<[f32; 4]>,
    /// The grip's colour, 0..1 linear.
    #[serde(default = "default_tracer")]
    pub tint: [f32; 3],
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

pub(super) fn default_one_u() -> u32 {
    1
}

pub(super) fn default_range() -> f32 {
    240.0
}

pub(super) fn default_tracer() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

pub(super) fn default_tracer_s() -> f32 {
    0.06
}
