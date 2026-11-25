//! Weapons and items in a race.
//!
//! The weapons are the shooter's, flown by the shooter's own code: the race
//! owns a `Combat`, hands it a `Hitbox` per kart each step, fires into it,
//! and turns the `Hit`s it gives back into what happens to a kart (`Harm`):
//! a hit of 20 or more spins the kart, a lighter one slows it and builds
//! toward a spin, and a blast's push knocks it sideways. The fire modes are
//! taken from the shooter's weapons and retuned for karts (`fires`): a kart
//! does 30 m/s and the shooter's rocket flies at 30.
//!
//! - Machine gun: the minigun's left button, along the kart's nose, bending
//!   onto a kart near its line (aiming from a kart is hard).
//! - Rocket: the launcher's guided right button, after where you look (held
//!   level, so it flies on over the road), or after a kart you hold near the
//!   middle of the view. It doesn't go off in the air over the course, only
//!   on what it hits or once it is past the course's edge.
//! - Goo: the goo gun's grown glob, dropped behind; it goes off under the
//!   next kart and spins it.
//! - Block: the bore's block of spoil, dropped behind; a kart that runs into
//!   it breaks it, takes damage, loses 15% of its speed and is held to 60%
//!   of its top speed for a second and a half.
//! - Disc: the skipjack's disc, up the road; it bounces at the edge of the
//!   track, wall or none, and is held over the road on the hills. A disc
//!   takes 1% off a kart's speed (`ItemRules::disc_damage`) and never spins it.
//!
//! Items stand on the road in rows across it (`ItemBox`), each a pickup you
//! can see before you reach it, drawn as the shooter's ammunition for it:
//! steer for the one you want. Which item stands where is fixed by the row
//! and the place in it, nothing random, the same whatever your place (no
//! rubber banding). A kart already carrying something drives through an
//! item without taking it; a wrench mends a damaged kart at once.
//!
//! Hits do damage (`Harm::damage`): a share of the kart's speed it loses
//! until the race ends, up to `ItemRules::damage_max`, about 2% for a rocket. The numbers
//! are the race game's (`ItemRules`, `library/games/race.json`).
//! `KART_ARMS=all` hands every kart every weapon, refilled, for tuning, and
//! `KART_ARMS=off` races with no boxes, for timing the karts.

use crate::combat::{Combat, Hit, Hitbox, Scene};
use crate::library::{FireDef, Library};
use crate::world::fields::ItemRow;
use crate::vehicle::bot::Track;
use glam::{Vec2, Vec3};

/// What a kart can hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Item {
    Gun,
    Rocket,
    Goo,
    Block,
    Disc,
    Boost,
    Shield,
    Overdrive,
    /// Not carried: taken, it mends `ItemRules::repair` of a kart's damage at once.
    Repair,
}

impl Item {
    #[cfg(test)]
    pub const ALL: [Item; 9] = [Item::Gun, Item::Rocket, Item::Goo, Item::Block, Item::Disc, Item::Boost, Item::Shield, Item::Overdrive, Item::Repair];
    /// The weapons, in the order the digits pick them with `KART_ARMS=all`.
    pub const WEAPONS: [Item; 5] = [Item::Gun, Item::Rocket, Item::Goo, Item::Block, Item::Disc];

    /// The item a track's pickup row names (`gun`, `rocket`, ...).
    pub fn named(name: &str) -> Option<Item> {
        Some(match name {
            "gun" => Item::Gun,
            "rocket" => Item::Rocket,
            "goo" => Item::Goo,
            "block" => Item::Block,
            "disc" => Item::Disc,
            "boost" => Item::Boost,
            "shield" => Item::Shield,
            "overdrive" => Item::Overdrive,
            "repair" => Item::Repair,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Item::Gun => "GUN",
            Item::Rocket => "ROCKET",
            Item::Goo => "GOO",
            Item::Block => "BLOCK",
            Item::Disc => "DISC",
            Item::Boost => "BOOST",
            Item::Shield => "SHIELD",
            Item::Overdrive => "OVERDRIVE",
            Item::Repair => "REPAIR",
        }
    }


    /// Its colour on the HUD and its box.
    pub fn colour(self) -> [f32; 4] {
        match self {
            Item::Gun => [1.0, 0.8, 0.35, 1.0],
            Item::Rocket => [1.0, 0.36, 0.3, 1.0],
            Item::Goo => [0.6, 0.92, 0.25, 1.0],
            Item::Block => [0.75, 0.75, 0.78, 1.0],
            Item::Disc => [0.6, 0.85, 1.0, 1.0],
            Item::Boost => [1.0, 0.6, 0.15, 1.0],
            Item::Shield => [0.45, 0.65, 1.0, 1.0],
            Item::Overdrive => [1.0, 0.95, 0.4, 1.0],
            Item::Repair => [0.4, 1.0, 0.5, 1.0],
        }
    }

    /// Which of `Arms::fires` it fires, for a weapon.
    fn fire(self) -> Option<usize> {
        Item::WEAPONS.iter().position(|&w| w == self)
    }

    /// Fired again while the button is held.
    pub fn held(self) -> bool {
        self == Item::Gun
    }
}

/// What a kart is carrying: an item and its uses left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Empty,
    Have(Item, u32),
}

/// The numbers the items and weapons go by, from the race game's `items`
/// block (`library/games/race.json`, src/games.rs). Anything the block
/// leaves out is the value here, which the file restates.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemRules {
    /// The pickups along the rows a track doesn't fill itself, in order:
    /// each row takes the next ones, so the rows differ and every item
    /// comes round.
    pub cycle: Vec<Item>,
    /// Seconds a taken pickup takes to come back.
    pub pickup_back_s: f32,
    /// What one pickup gives: rounds of the gun, blocks, discs. The rest
    /// are one use each.
    pub gun_rounds: u32,
    pub blocks: u32,
    pub discs: u32,
    /// A hit of this much of the shooter's damage spins a kart, and so do
    /// light hits adding up to `heat_spin`, which cool at `heat_cool` a
    /// second. A light hit takes `slow_per_hit` m/s off.
    pub spin_at: f32,
    pub heat_spin: f32,
    pub heat_cool: f32,
    pub slow_per_hit: f32,
    /// Seconds a spin lasts, the seconds after it before another can, and
    /// the share of its speed a spun kart keeps.
    pub spin_s: f32,
    pub spin_grace_s: f32,
    pub spin_keep: f32,
    /// The most a blast's push knocks a kart, m/s.
    pub knock_max: f32,
    /// A kart that runs over a block keeps this share of its speed, and is
    /// thrown up at this many m/s: a rattle and a bounce, not a stop. For
    /// `block_slow_s` after, it is held to `block_slow_top` of its top speed.
    pub block_keep: f32,
    pub block_hop: f32,
    pub block_slow_s: f32,
    pub block_slow_top: f32,
    /// Seconds of shield (it turns every hit), of overdrive, of the BOOST
    /// item and of a boost pad. Overdrive raises top speed by
    /// `overdrive_top` and pushes toward it at `overdrive_push` m/s².
    pub shield_s: f32,
    pub overdrive_s: f32,
    pub overdrive_top: f32,
    pub overdrive_push: f32,
    pub boost_s: f32,
    pub pad_s: f32,
    /// A pad's kick as a multiple of the BOOST item's (twice as strong).
    pub pad_kick: f32,
    /// The small boost on Shift with nothing carried: seconds of it, and
    /// seconds to refill.
    pub small_boost_s: f32,
    pub small_boost_refill_s: f32,
    /// Damage, as a share of a kart's top speed, power and drive force:
    /// the most it can lose, what a rocket's hit and blast cost together
    /// (other hits by the shooter's damage, against the rocket's 96), what
    /// running into a block and a disc's blow cost, and what a wrench mends.
    pub damage_max: f32,
    pub rocket_damage: f32,
    pub block_damage: f32,
    pub disc_damage: f32,
    pub repair: f32,
    /// The shooter's weapons retuned for karts (`fires`): the gun's reach
    /// and spread, the rocket's speed, fuse and blast push, the disc's speed.
    pub gun_range: f32,
    pub gun_spread_deg: f32,
    pub rocket_speed: f32,
    pub rocket_fuse_s: f32,
    pub rocket_push: f32,
    pub disc_speed: f32,
}

impl Default for ItemRules {
    fn default() -> ItemRules {
        use Item::*;
        ItemRules {
            cycle: vec![Gun, Rocket, Boost, Repair, Goo, Disc, Shield, Gun, Block, Rocket, Boost, Repair, Disc, Overdrive, Goo, Shield],
            pickup_back_s: 3.0,
            gun_rounds: 45,
            blocks: 2,
            discs: 3,
            spin_at: 20.0,
            heat_spin: 50.0,
            heat_cool: 12.0,
            slow_per_hit: 0.4,
            spin_s: 0.9,
            spin_grace_s: 1.5,
            spin_keep: 0.35,
            knock_max: 6.0,
            block_keep: 0.85,
            block_hop: 1.2,
            block_slow_s: 1.5,
            block_slow_top: 0.6,
            shield_s: 10.0,
            overdrive_s: 6.0,
            overdrive_top: 1.15,
            overdrive_push: 8.0,
            boost_s: 2.0,
            pad_s: 2.4,
            pad_kick: 2.0,
            small_boost_s: 1.2,
            small_boost_refill_s: 8.0,
            damage_max: 0.15,
            rocket_damage: 0.02,
            block_damage: 0.03,
            disc_damage: 0.01,
            repair: 0.05,
            gun_range: 250.0,
            gun_spread_deg: 2.0,
            rocket_speed: 55.0,
            rocket_fuse_s: 6.0,
            rocket_push: 6.0,
            disc_speed: 55.0,
        }
    }
}

impl ItemRules {
    /// Uses a pickup gives: rounds of the gun, or how many of the rest.
    pub fn uses(&self, item: Item) -> u32 {
        match item {
            Item::Gun => self.gun_rounds,
            Item::Block => self.blocks,
            Item::Disc => self.discs,
            _ => 1,
        }
        .max(1)
    }

    /// What a point of the shooter's damage costs: a rocket's hit and blast
    /// together come to about 96.
    fn per_point(&self) -> f32 {
        self.rocket_damage / 96.0
    }
}

/// What has been done to one kart, and what it has on.
#[derive(Clone, Copy, Debug, Default)]
pub struct Harm {
    /// Seconds left spun out, and seconds before it can be spun again.
    pub spin: f32,
    pub immune: f32,
    /// Light hits lately: enough of them spin the kart.
    pub heat: f32,
    /// Seconds left of the shield, and of overdrive.
    pub shield: f32,
    pub over: f32,
    /// Seconds left held slow after running into a block.
    pub slowed: f32,
    /// The share of its speed the kart has lost to hits, 0 to `ItemRules::damage_max`.
    pub damage: f32,
    /// Seconds left of the kart blinking after it took damage, and of its
    /// shield flaring after the shield turned a hit.
    pub blink: f32,
    pub turned: f32,
}

/// A block breaks when a kart's nose comes within this many metres of it,
/// before the kart's body can meet its voxels.
const BLOCK_REACH: f32 = 1.6;
const BLOCK_AHEAD: f32 = 2.0;
/// Seconds a kart blinks after taking damage, and flashes a second while it
/// does; seconds a shield flares after turning a hit.
const BLINK_SECS: f32 = 0.5;
const BLINK_HZ: f32 = 12.0;
pub const TURNED_SECS: f32 = 0.35;

impl Harm {
    /// Seconds pass.
    pub fn cool(&mut self, r: &ItemRules, dt: f32) {
        self.spin = (self.spin - dt).max(0.0);
        self.immune = (self.immune - dt).max(0.0);
        self.heat = (self.heat - r.heat_cool * dt).max(0.0);
        self.shield = (self.shield - dt).max(0.0);
        self.over = (self.over - dt).max(0.0);
        self.slowed = (self.slowed - dt).max(0.0);
        self.blink = (self.blink - dt).max(0.0);
        self.turned = (self.turned - dt).max(0.0);
    }

    /// Spin out, unless just spun or protected. True if it did.
    pub fn spin_out(&mut self, r: &ItemRules) -> bool {
        if self.immune > 0.0 || self.over > 0.0 {
            return false;
        }
        if self.shield > 0.0 {
            self.turned = TURNED_SECS;
            return false;
        }
        self.spin = r.spin_s;
        self.immune = r.spin_s + r.spin_grace_s;
        self.heat = 0.0;
        true
    }

    /// A disc's blow: `disc_damage` of the kart's speed and a light hit's
    /// slowing, but never a spin, and nothing toward one.
    pub fn graze(&mut self, r: &ItemRules) -> (bool, f32, Vec3) {
        if self.shield > 0.0 || self.over > 0.0 {
            self.shielded();
            return (false, 0.0, Vec3::ZERO);
        }
        self.hurt(r, r.disc_damage);
        (false, r.slow_per_hit, Vec3::ZERO)
    }

    /// Damage done to the kart, up to the most it can take.
    pub fn hurt(&mut self, r: &ItemRules, share: f32) {
        self.damage = (self.damage + share).clamp(0.0, r.damage_max);
        if share > 0.0 {
            self.blink = BLINK_SECS;
        }
    }

    /// A hit the shield turned: it flares.
    fn shielded(&mut self) {
        if self.shield > 0.0 {
            self.turned = TURNED_SECS;
        }
    }

    /// Whether the kart is drawn lit up this frame: it flashes while it
    /// blinks.
    pub fn flashing(&self) -> bool {
        self.blink > 0.0 && (self.blink * BLINK_HZ).fract() >= 0.5
    }

    /// What one hit does: (spun, speed to take off m/s, knock m/s). The
    /// shield and overdrive turn all of it.
    pub fn take(&mut self, r: &ItemRules, h: &Hit) -> (bool, f32, Vec3) {
        if self.shield > 0.0 || self.over > 0.0 {
            self.shielded();
            return (false, 0.0, Vec3::ZERO);
        }
        let flat = Vec3::new(h.push.x, 0.0, h.push.z);
        let knock = flat.clamp_length_max(r.knock_max);
        self.hurt(r, h.damage * r.per_point());
        if h.damage >= r.spin_at {
            return (self.spin_out(r), 0.0, knock);
        }
        self.heat += h.damage;
        if self.heat >= r.heat_spin {
            return (self.spin_out(r), 0.0, knock);
        }
        (false, r.slow_per_hit, knock)
    }
}

/// One pickup of a row across the road: where it stands, what it is, and
/// seconds until a taken one is back.
#[derive(Clone, Debug)]
pub struct ItemBox {
    pub at: Vec3,
    pub item: Item,
    pub back: f32,
}

/// A kart within this many metres of a box takes it.
const BOX_REACH: f32 = 1.6 * KART_SIZE;
/// The karts are drawn this much bigger than their physics (vehicle/draw.rs
/// `KART_SIZE`), and are as much bigger to hit and to reach a box with.
pub const KART_SIZE: f32 = crate::vehicle::draw::KART_SIZE;

/// Where a kart is, for the weapons: its middle, its nose, and whether it
/// is in the race to be hit (not carried round out of sight).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub on: bool,
}

struct Shot {
    by: usize,
    fire: usize,
    eye: Vec3,
    muzzle: Vec3,
    dir: Vec3,
    /// Added to what it is thrown with: goo or a block thrown ahead goes
    /// with the kart's speed and more, or the kart runs into it at once.
    carry: Vec3,
}

/// A block of spoil lying on the road: its middle, its voxels, and how long
/// it has lain.
struct Block {
    at: Vec3,
    voxels: Vec<[i32; 3]>,
    age: f32,
}

/// Seconds a block lies before it goes on its own.
const BLOCK_LIFE: f32 = 30.0;
/// Metres past the road's outermost points that still count as over the
/// course, for a rocket's fuse (`Arms::step`).
const COURSE_MARGIN: f32 = 20.0;
/// Seconds a rocket flies on after it leaves the course's box.
const ROCKET_OUT_S: f32 = 1.0;
/// Metres over the road a rocket or gun round flies while it is over it
/// (`ride_road`): the middle of a kart's hitbox. A disc flies lower.
const RIDE_H: f32 = 0.8;
const DISC_H: f32 = 0.5;
/// How far off its riding height a shell can be and still be held to it,
/// metres: further is a shell on a deck over or under this stretch.
const RIDE_REACH: f32 = 3.0;

/// Hold the shells over the road, before they fly a step of `dt`: rockets
/// and gun rounds `RIDE_H` over it while they are over it, discs `DISC_H`
/// always (`Arms::discs` keeps them on it). The height is the road's own
/// surface, read from the voxels under the shell and along the way it will
/// fly this step, so banking, a skewed lip and a jump's ramp ahead are all
/// cleared, not only the centreline's height. A shell keeps its speed and
/// flies level; past the road's edge it carries on as it was going. Goo
/// and blocks are thrown and dropped to land, so they are left alone.
fn ride_road(shells: &mut [crate::combat::shot::Shell], store: &crate::world::store::Store, track: &Track, disc: Option<usize>, dt: f32) {
    for s in shells.iter_mut().filter(|s| !s.stuck && s.goo_r == 0.0 && s.spoil == 0.0) {
        let is_disc = Some(s.look) == disc;
        let lift = if is_disc { DISC_H } else { RIDE_H };
        let pl = track.place(s.pos, None);
        let on = |p: &crate::vehicle::bot::Place| p.off.abs() <= track.width_at(p.s) * 0.5 + 0.5;
        if !is_disc && !on(&pl) {
            continue;
        }
        let Some(centre) = track.height_at(pl.s) else { continue };
        // The road's top here and along this step's flight, from a little
        // over the centreline's height, so a wall or lamp head is not read
        // as road and a deck overhead is out of reach.
        let flat = Vec3::new(s.vel.x, 0.0, s.vel.z);
        let mut top: Option<f32> = None;
        for k in 0..4 {
            let q = s.pos + flat * dt * (k as f32 * 0.5);
            let pq = track.place(q, Some(pl));
            if !on(&pq) {
                continue;
            }
            let c = track.height_at(pq.s).unwrap_or(centre);
            if let Some(t) = ground_top(store, q.x, q.z, c) {
                top = Some(top.map_or(t, |m: f32| m.max(t)));
            }
        }
        let want = top.unwrap_or(centre) + lift;
        if !is_disc && (s.pos.y - want).abs() > RIDE_REACH {
            continue;
        }
        let speed = s.vel.length();
        s.pos.y = want;
        s.vel = flat.normalize_or_zero() * speed;
    }
}

/// The top of the solid ground at (x, z) near the road height `road`,
/// metres: the first solid voxel going down from `RIDE_REACH` over it (a
/// jump's lip is within that) to as far under it. Passable voxels are not
/// ground.
fn ground_top(store: &crate::world::store::Store, x: f32, z: f32, road: f32) -> Option<f32> {
    let v = crate::world::gen::VOXEL;
    let (vx, vz) = ((x / v).floor() as i32, (z / v).floor() as i32);
    let (hi, lo) = (((road + RIDE_REACH) / v).floor() as i32, ((road - RIDE_REACH) / v).floor() as i32);
    (lo..=hi).rev().find(|&vy| store.solid(vx, vy, vz)).map(|vy| (vy + 1) as f32 * v)
}

pub struct Arms {
    pub combat: Combat,
    fires: Vec<FireDef>,
    pending: Vec<Shot>,
    pub hits: Vec<Hit>,
    pub poses: Vec<Pose>,
    /// Where each kart's guided shells steer: (eye, way), by kart.
    pub aims: Vec<Option<(Vec3, Vec3)>>,
    blocks: Vec<Block>,
    /// Voxels of broken and old blocks, to clear at the next step.
    clear: Vec<([i32; 3], u8)>,
    pub boxes: Vec<ItemBox>,
    disc_look: Option<usize>,
    seed: u32,
    /// `KART_ARMS=all`: every kart has every weapon, refilled.
    pub all: bool,
    pub rules: ItemRules,
}

impl Arms {
    pub fn new(lib: &Library, track: &Track, rows: &[ItemRow], rules: ItemRules) -> Arms {
        let all = std::env::var("KART_ARMS").is_ok_and(|v| v == "all");
        // KART_ARMS=off races with no boxes: a clean race, for timing.
        let boxes = if std::env::var("KART_ARMS").is_ok_and(|v| v == "off") { Vec::new() } else { place_boxes(track, rows, &rules.cycle) };
        Arms::with(fires(lib, &rules), boxes, all, rules)
    }

    /// These weapons and boxes as a new race has them: nothing fired or
    /// lying on the road, and every box standing.
    pub fn fresh(&self) -> Arms {
        let boxes = self.boxes.iter().map(|b| ItemBox { back: 0.0, ..b.clone() }).collect();
        Arms::with(self.fires.clone(), boxes, self.all, self.rules.clone())
    }

    fn with(fires: Vec<FireDef>, boxes: Vec<ItemBox>, all: bool, rules: ItemRules) -> Arms {
        let mut combat = Combat::new(0x6b61_7274);
        // Spoil lands as plain stone: easy to tell from the road and the goo.
        combat.spoil_mat = crate::world::material::STONE;
        Arms {
            combat,
            fires,
            pending: Vec::new(),
            hits: Vec::new(),
            poses: Vec::new(),
            aims: Vec::new(),
            blocks: Vec::new(),
            clear: Vec::new(),
            boxes,
            disc_look: crate::combat::viewmodel::shell_of("disc"),
            seed: 0x51ed,
            all,
            rules,
        }
    }

    /// Whether a hit came from a disc (its shell's look).
    pub fn is_disc(&self, h: &Hit) -> bool {
        h.look.is_some() && h.look == self.disc_look
    }

    /// The fire mode `item` shoots, if it is a weapon.
    pub fn fire_of(&self, item: Item) -> Option<&FireDef> {
        item.fire().and_then(|i| self.fires.get(i))
    }

    /// Seconds between shots of a held weapon.
    pub fn gap(&self, item: Item) -> f32 {
        self.fire_of(item).map_or(0.5, |f| 60.0 / f.rpm.max(1.0))
    }

    /// Fire `item` from kart `by` at the next step.
    pub fn shoot(&mut self, by: usize, item: Item, eye: Vec3, muzzle: Vec3, dir: Vec3) {
        self.throw(by, item, eye, muzzle, dir, Vec3::ZERO);
    }

    /// `shoot`, the shell given `carry` on top of its own speed.
    pub fn throw(&mut self, by: usize, item: Item, eye: Vec3, muzzle: Vec3, dir: Vec3, carry: Vec3) {
        if let Some(fire) = item.fire() {
            self.pending.push(Shot { by, fire, eye, muzzle, dir, carry });
        }
    }

    /// A weapon at random, evenly (`KART_ARMS=all`).
    pub fn roll_weapon(&mut self) -> Item {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        Item::WEAPONS[(self.seed >> 16) as usize % Item::WEAPONS.len()]
    }

    /// Seconds pass for the pickups; each kart (by pose) that takes one this
    /// step, and what. `wants` says whether a kart has a use for an item: a
    /// pickup a kart can't use stays standing for the next.
    pub fn boxes_taken(&mut self, dt: f32, wants: impl Fn(usize, Item) -> bool) -> Vec<(usize, Item)> {
        let mut took: Vec<(usize, Item)> = Vec::new();
        let back = self.rules.pickup_back_s;
        for b in &mut self.boxes {
            b.back = (b.back - dt).max(0.0);
            if b.back > 0.0 {
                continue;
            }
            let near = |p: &Pose| p.on && Vec2::new(p.pos.x - b.at.x, p.pos.z - b.at.z).length() < BOX_REACH && (p.pos.y - b.at.y).abs() < 2.0;
            if let Some(k) = self.poses.iter().enumerate().position(|(k, p)| near(p) && !took.iter().any(|(j, _)| *j == k) && wants(k, b.item)) {
                b.back = back;
                took.push((k, b.item));
            }
        }
        took
    }

    /// Whether a kart at `pos` heading `fwd` has run onto a block, which then
    /// breaks: checked a little ahead of the kart too, so the block is gone
    /// before the kart's body meets its voxels and stops dead.
    pub fn run_into_block(&mut self, pos: Vec3, fwd: Vec3) -> bool {
        let ahead = pos + Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero() * BLOCK_AHEAD;
        let near = |b: &Block, p: Vec3| Vec2::new(b.at.x - p.x, b.at.z - p.z).length() < BLOCK_REACH && (b.at.y - p.y).abs() < 1.2;
        let Some(k) = self.blocks.iter().position(|b| near(b, pos) || near(b, ahead)) else {
            return false;
        };
        let b = self.blocks.swap_remove(k);
        self.clear.extend(b.voxels.into_iter().map(|v| (v, crate::world::material::AIR)));
        true
    }

    /// One step of everything in the air (`Activity::step`): hitboxes from
    /// the karts, what was fired since, the flight, the discs kept on the
    /// road and bouncing at its edges, and what landed recorded.
    pub fn step(&mut self, store: &crate::world::store::Store, ents: &mut crate::entities::Entities, models: &[crate::models::Model], track: &Track, edits: &mut Vec<([i32; 3], u8)>, dt: f32) {
        let boxes: Vec<Hitbox> = self.poses.iter().map(|p| Hitbox { foot: p.pos - Vec3::Y * 0.25, height: 1.1 * KART_SIZE, half_w: KART_SIZE, alive: p.on, team: None }).collect();
        self.combat.aims.clear();
        for (k, a) in self.aims.iter().enumerate() {
            if let Some((eye, way)) = a {
                self.combat.aims.push((k, *eye, *way));
            }
        }
        let shots = std::mem::take(&mut self.pending);
        // A rocket (the race's one guided shell) doesn't go off in the air
        // over the course: its fuse and range are held while it is inside
        // the course's box, and once out of it it goes off within
        // `ROCKET_OUT_S`. It still bursts on anything it hits.
        let (lo, hi) = track.points.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let (lo, hi) = (lo - Vec2::splat(COURSE_MARGIN), hi + Vec2::splat(COURSE_MARGIN));
        for s in self.combat.shells.iter_mut().filter(|s| s.guide > 0.0 && !s.stuck) {
            let p = Vec2::new(s.pos.x, s.pos.z);
            if p.cmpge(lo).all() && p.cmple(hi).all() {
                s.fuse = s.fuse.max(ROCKET_OUT_S);
                s.flown = 0.0;
            } else {
                s.fuse = s.fuse.min(ROCKET_OUT_S);
            }
        }
        {
            let mut scene = Scene { store, ents, models, fighters: &boxes };
            for s in shots {
                let f = &self.fires[s.fire];
                let before = self.combat.shells.len();
                self.combat.fire(&mut scene, s.eye, s.muzzle, s.dir, f, s.by, 1.0);
                for sh in &mut self.combat.shells[before..] {
                    sh.vel += s.carry;
                }
            }
            ride_road(&mut self.combat.shells, store, track, self.disc_look, dt);
            self.combat.step(&mut scene, dt);
        }
        self.discs(track);
        self.hits.extend(self.combat.take_hits());
        edits.append(&mut self.clear);
        // What landed, blocks of spoil kept track of. A new race wipes it all
        // with the rest of what the race did to the world (`Race::clean_world`).
        for (v, m) in self.combat.take_edits() {
            if m == crate::world::material::STONE {
                let at = (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * crate::world::gen::VOXEL;
                match self.blocks.iter_mut().find(|b| b.at.distance(at) < 1.0) {
                    Some(b) => b.voxels.push(v),
                    None => self.blocks.push(Block { at, voxels: vec![v], age: 0.0 }),
                }
            }
            edits.push((v, m));
        }
        for b in &mut self.blocks {
            b.age += dt;
        }
        let mut gone = Vec::new();
        self.blocks.retain(|b| {
            let old = b.age > BLOCK_LIFE;
            if old {
                gone.extend(b.voxels.iter().map(|&v| (v, crate::world::material::AIR)));
            }
            !old
        });
        // Broken and old blocks go at the next step.
        self.clear.extend(gone);
    }

    /// The discs: bounced off the edge of the track (past the kerb) whether
    /// or not a wall stands there, each bounce counted as the shooter counts
    /// one off a wall. `ride_road` holds them over the road.
    fn discs(&mut self, track: &Track) {
        let Some(look) = self.disc_look else { return };
        for s in self.combat.shells.iter_mut().filter(|s| s.look == look && !s.stuck) {
            let pl = track.place(s.pos, None);
            let half = track.width_at(pl.s) * 0.5 + 1.0;
            if pl.off.abs() > half {
                let (_, d) = track.at(pl.s);
                // Right of the way round in (x, z), z south.
                let out = Vec3::new(-d.y, 0.0, d.x) * pl.off.signum();
                let v_out = s.vel.dot(out);
                if v_out > 0.0 {
                    s.vel = (s.vel - out * 2.0 * v_out) * s.keep;
                    s.bounced = true;
                    if s.bounces == 0 {
                        s.fuse = 0.0;
                    } else {
                        s.bounces -= 1;
                    }
                }
                s.pos -= out * (pl.off.abs() - half);
            }
        }
    }

    /// What lies on the road for a driver to steer round: blocks, and goo
    /// waiting to go off.
    pub fn hazards(&self) -> Vec<Vec3> {
        self.blocks.iter().map(|b| b.at).chain(self.combat.mines().map(|(p, _)| p)).collect()
    }
}

/// The race's fire modes, from the shooter's weapons, retuned for karts:
/// gun, rocket, goo, block, disc (the order of `Item::WEAPONS`).
fn fires(lib: &Library, r: &ItemRules) -> Vec<FireDef> {
    let get = |name: &str, alt: bool| -> FireDef {
        let w = lib.weapon(name).map(|i| &lib.weapons[i]);
        match (w, alt) {
            (Some(w), false) => w.fire.clone(),
            (Some(w), true) => w.alt.clone().unwrap_or_else(|| w.fire.clone()),
            (None, _) => panic!("the race's weapons come from the library's {name}, which is missing"),
        }
    };
    let mut gun = get("minigun", false);
    gun.spread_deg = r.gun_spread_deg;
    gun.range = r.gun_range;
    gun.falloff = 1.0;
    gun.spin_up = 0.0;
    let mut rocket = get("rocket_launcher", true);
    rocket.speed = r.rocket_speed;
    rocket.fuse_s = r.rocket_fuse_s;
    if let Some(sp) = &mut rocket.splash {
        sp.crater = 0.0;
        sp.self_damage = 0.0;
        sp.self_push = 0.0;
        sp.push = r.rocket_push;
    }
    let mut goo = get("goo_gun", true);
    goo.speed = 4.0;
    goo.charge_s = 0.0;
    // One glob: a kart that touches it sets all of it off, and it goes in a
    // puff, as in the shooter. Small enough that a kart's touch (its hitbox,
    // `Arms::step`) reaches it before its nose meets the sludge.
    goo.per_shot = 1;
    goo.goo_r = 0.8;
    goo.stick_s = 20.0;
    goo.fuse_s = 22.0;
    if let Some(sp) = &mut goo.splash {
        sp.self_damage = 0.0;
        sp.self_push = 0.0;
    }
    let mut block = get("bore", true);
    block.speed = 4.0;
    block.per_shot = 1;
    block.damage = 0.0;
    let mut disc = get("skipjack", false);
    disc.speed = r.disc_speed;
    disc.self_hit = 0.0;
    vec![gun, rocket, goo, block, disc]
}

/// The rows of boxes: where the track puts them (`rows`), each of the kinds
/// it names from the left and `cycle` for the rest; or, for a track that
/// names none, on the straights, 80 m or more past the line, at least 200 m
/// apart, up to three a lap, each four across the road.
fn place_boxes(track: &Track, rows: &[ItemRow], cycle: &[Item]) -> Vec<ItemBox> {
    let len = track.length;
    let rows: Vec<(f32, u32, Vec<Option<Item>>)> = if rows.is_empty() {
        let mut at: Vec<f32> = Vec::new();
        let mut s = 80.0;
        while s < len - 60.0 && at.len() < 3 {
            // A straight: the road barely bends for 30 m either side.
            let straight = (-6..=6).all(|k| track.curve_at(s + k as f32 * 5.0).abs() < 1.0 / 90.0);
            if straight && at.last().is_none_or(|&r| s - r >= 200.0) {
                at.push(s);
                s += 200.0;
            } else {
                s += 10.0;
            }
        }
        at.into_iter().map(|s| (s, 4, Vec::new())).collect()
    } else {
        rows.iter().map(|r| (r.s, r.across.clamp(1, 8), r.kinds.iter().map(|k| Item::named(k)).collect())).collect()
    };
    let mut out = Vec::new();
    let mut next = 0;
    for (s, across, kinds) in rows {
        let (p, d) = track.at(s);
        let right = Vec2::new(-d.y, d.x);
        let y = track.height_at(s).unwrap_or(0.0) + 0.9;
        let width = track.width_at(s);
        for k in 0..across {
            // Spread over the middle of the road, clear of the kerbs.
            let off = (k as f32 - (across - 1) as f32 * 0.5) * (width * 0.8 / across as f32);
            let q = p + right * off;
            let item = match kinds.get(k as usize).copied().flatten() {
                Some(item) => item,
                None if cycle.is_empty() => continue,
                None => {
                    next += 1;
                    cycle[(next - 1) % cycle.len()]
                }
            };
            out.push(ItemBox { at: Vec3::new(q.x, y, q.y), item, back: 0.0 });
        }
    }
    out
}

/// Kart `me` aims at the kart nearest the line `dir` from `from`, within
/// `cone` radians and `range` metres: the way to it, or None.
pub fn assist(poses: &[Pose], me: usize, from: Vec3, dir: Vec3, cone: f32, range: f32) -> Option<(usize, Vec3)> {
    poses
        .iter()
        .enumerate()
        .filter(|&(k, p)| k != me && p.on)
        .filter_map(|(k, p)| {
            let to = p.pos + Vec3::Y * 0.3 - from;
            let d = to.length();
            let way = to / d.max(1e-3);
            (d < range && way.dot(dir) > cone.cos()).then_some((k, way, way.dot(dir)))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(k, w, _)| (k, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Ribstrand (banked, with jumps), a disc and a rocket fired along
    /// the road from all round the lap, on the line and off to each side,
    /// fly on over it for a second and a half without striking it: they
    /// ride over the road's own surface, ramps and banking included.
    #[test]
    #[ignore = "streams Ribstrand in: cargo test --release shells_clear_the_road -- --ignored --nocapture"]
    fn shells_clear_the_road() {
        let (store, track) = crate::vehicle::lab::open("ribstrand");
        let disc = crate::combat::viewmodel::shell_of("disc");
        let shell = |pos: Vec3, vel: Vec3, look: usize| crate::combat::shot::Shell {
            pos,
            prev: pos,
            vel,
            gravity: 0.0,
            bounces: 0,
            keep: 0.0,
            self_hit: 0.0,
            bounced: false,
            fuse: f32::MAX,
            flown: 0.0,
            damage: 0.0,
            splash: None,
            tint: [1.0; 3],
            look,
            spin: 0.0,
            stick_s: 0.0,
            guide: 0.0,
            stuck: false,
            goo_r: 0.0,
            spoil: 0.0,
            cluster: None,
            by: 0,
        };
        let dt = 1.0 / 60.0;
        let (mut flights, mut struck) = (0, Vec::new());
        let mut s0 = 0.0;
        while s0 < track.length {
            let (c, d) = track.at(s0);
            let right = Vec2::new(-d.y, d.x);
            let Some(h) = track.height_at(s0) else {
                s0 += 10.0;
                continue;
            };
            for off in [-3.0, 0.0, 3.0] {
                for (look, speed) in [(disc.unwrap_or(usize::MAX), 40.0), (usize::MAX - 1, 55.0)] {
                    let at = c + right * off;
                    let mut sh = [shell(Vec3::new(at.x, h + 0.8, at.y), Vec3::new(d.x, 0.0, d.y) * speed, look)];
                    flights += 1;
                    for _ in 0..90 {
                        ride_road(&mut sh, &store, &track, disc, dt);
                        let s = &mut sh[0];
                        let step = s.vel * dt;
                        let len = step.length();
                        let pl = track.place(s.pos, None);
                        if pl.off.abs() > track.width_at(pl.s) * 0.5 {
                            // Run off the road on a bend: what it meets
                            // there is not the road.
                            break;
                        }
                        if let Some(hit) = store.ray(s.pos, step / len, len, |m| m != crate::world::material::AIR && !crate::world::store::passable(m)) {
                            struck.push((s0, off, Some(look) == disc, pl.s, pl.off, s.pos.y - track.height_at(pl.s).unwrap_or(0.0), hit.normal));
                            break;
                        }
                        s.pos += step;
                    }
                }
            }
            s0 += 10.0;
        }
        println!("{flights} flights, {} struck", struck.len());
        for (s0, off, is_disc, at, across, up, n) in struck.iter().take(20) {
            println!("  {} from {s0:.0} m, {off:+} m across: struck at {at:.0} m, {across:+.1} m across, {up:.2} m over the line, face {n:?}", if *is_disc { "disc" } else { "rocket" });
        }
        assert!(struck.is_empty(), "{} of {flights} flights struck the road", struck.len());
    }

    fn r() -> ItemRules {
        ItemRules::default()
    }

    fn hit(damage: f32, push: Vec3) -> Hit {
        Hit { who: 1, by: 2, damage, push, fling: push, head: false, blast: false, risky: false, look: None }
    }

    /// A heavy hit spins a kart; another straight after doesn't; a shield
    /// turns every hit for all of its time, then is gone.
    #[test]
    fn heavy_hits_spin_once_and_shields_last() {
        let mut h = Harm::default();
        assert!(h.take(&r(), &hit(42.0, Vec3::ZERO)).0);
        assert!(!h.take(&r(), &hit(42.0, Vec3::ZERO)).0, "spun again inside the grace");
        let mut s = Harm { shield: r().shield_s, ..Default::default() };
        for _ in 0..3 {
            assert!(!s.take(&r(), &hit(42.0, Vec3::ZERO)).0);
        }
        assert_eq!(s.damage, 0.0, "the shield let damage through");
        s.cool(&r(), r().shield_s + 0.1);
        assert!(s.take(&r(), &hit(42.0, Vec3::ZERO)).0, "the shield outlasted its time");
    }

    /// Damage makes a kart blink for a moment, on and off; a hit the shield
    /// turns flares it and does not blink the kart.
    #[test]
    fn damage_blinks_and_a_turned_hit_flares() {
        let mut h = Harm::default();
        h.take(&r(), &hit(5.0, Vec3::ZERO));
        let lit: Vec<bool> = (0..12).map(|_| {
            h.cool(&r(), 1.0 / 60.0);
            h.flashing()
        }).collect();
        assert!(lit.contains(&true) && lit.contains(&false), "no blink: {lit:?}");
        h.cool(&r(), BLINK_SECS);
        assert!(!h.flashing(), "still blinking");
        let mut s = Harm { shield: r().shield_s, ..Default::default() };
        s.take(&r(), &hit(5.0, Vec3::ZERO));
        assert!(s.turned > 0.0 && s.blink == 0.0);
    }

    /// A block breaks while the kart's nose is still a couple of metres
    /// short of it, so the kart never meets its voxels and stops dead.
    #[test]
    fn a_block_breaks_before_the_kart_reaches_it() {
        let (lib, _) = crate::library::Library::load(&crate::library::data_root());
        let pts: Vec<Vec2> = (0..400).map(|i| Vec2::from_angle(i as f32 * std::f32::consts::TAU / 400.0) * 60.0).collect();
        let mut arms = Arms::new(&lib, &Track::new(pts, 12.0), &[], r());
        let at = Vec3::new(10.0, 0.5, 0.0);
        let fwd = Vec3::X;
        arms.blocks.push(Block { at, voxels: Vec::new(), age: 0.0 });
        assert!(!arms.run_into_block(at - fwd * 5.0, fwd), "broke from 5 m back");
        assert!(arms.run_into_block(at - fwd * 3.0, fwd), "still there 3 m short");
        assert!(arms.blocks.is_empty());
    }

    #[test]
    fn a_disc_never_spins_a_kart() {
        let mut h = Harm::default();
        for _ in 0..20 {
            assert!(!h.graze(&r()).0);
        }
        assert!((h.damage - r().damage_max).abs() < 1e-6, "{}", h.damage);
        let mut one = Harm::default();
        one.graze(&r());
        assert!((one.damage - 0.01).abs() < 1e-6);
        assert_eq!(one.heat, 0.0, "a disc builds nothing toward a spin");
    }

    /// A rocket's hit and blast do about 2% damage; it adds up to no more
    /// than r().damage_max, and a wrench takes 5% off.
    #[test]
    fn damage_adds_up_to_its_cap() {
        let mut h = Harm::default();
        h.take(&r(), &hit(30.4, Vec3::ZERO));
        h.take(&r(), &hit(65.6, Vec3::ZERO));
        assert!((h.damage - 0.02).abs() < 1e-4, "{}", h.damage);
        for _ in 0..20 {
            h.immune = 0.0;
            h.take(&r(), &hit(96.0, Vec3::ZERO));
        }
        assert_eq!(h.damage, r().damage_max);
        h.hurt(&r(), -r().repair);
        assert!((h.damage - (r().damage_max - r().repair)).abs() < 1e-6);
    }

    /// The pickups along the rows are fixed and every item comes round, in
    /// the race game's file as in the default.
    #[test]
    fn every_item_stands_somewhere_in_the_rows() {
        let file = crate::games::get("race").and_then(|g| g.items.clone()).expect("race.json has an items block");
        for rules in [r(), file] {
            for i in Item::ALL {
                assert!(rules.cycle.contains(&i), "{i:?} never stands in a row");
            }
        }
    }

    /// Light hits slow a kart, and enough of them close together spin it.
    #[test]
    fn a_burst_of_light_hits_spins() {
        let mut h = Harm::default();
        let mut spun = false;
        for _ in 0..20 {
            let (s, slow, _) = h.take(&r(), &hit(3.85, Vec3::ZERO));
            spun |= s;
            if !s {
                assert!(slow > 0.0);
            }
            h.cool(&r(), 1.0 / 18.0);
        }
        assert!(spun, "twenty rounds in a second didn't spin it");
        // Spread out, they never do.
        let mut h = Harm::default();
        for _ in 0..40 {
            assert!(!h.take(&r(), &hit(3.85, Vec3::ZERO)).0);
            h.cool(&r(), 0.5);
        }
    }

    /// A blast's push is flattened and capped.
    #[test]
    fn a_knock_is_flat_and_capped() {
        let mut h = Harm::default();
        let (_, _, k) = h.take(&r(), &hit(5.0, Vec3::new(30.0, 20.0, 0.0)));
        assert_eq!(k.y, 0.0);
        assert!((k.length() - r().knock_max).abs() < 1e-4);
    }

    /// A kart off to one side is not aimed at; one near the line is.
    #[test]
    fn assist_picks_the_kart_near_the_line() {
        let poses = [
            Pose { pos: Vec3::ZERO, on: true, ..Default::default() },
            Pose { pos: Vec3::new(1.0, 0.0, -20.0), on: true, ..Default::default() },
            Pose { pos: Vec3::new(15.0, 0.0, -20.0), on: true, ..Default::default() },
        ];
        let got = assist(&poses, 0, Vec3::ZERO, Vec3::NEG_Z, 8f32.to_radians(), 40.0);
        assert_eq!(got.map(|g| g.0), Some(1));
    }

    /// The shooter's weapons are all there to take the race's from, and the
    /// rocket outruns a boosting kart.
    #[test]
    fn the_race_fires_come_from_the_library() {
        let (lib, _) = Library::load(&crate::library::data_root());
        let f = fires(&lib, &r());
        assert_eq!(f.len(), Item::WEAPONS.len());
        assert!(f[1].guide > 0.0, "the rocket isn't guided");
        assert!(f[1].speed > 40.0);
        assert!(f[2].goo_r > 0.0, "the goo lays no sludge");
        assert!(f[3].spoil_r > 0.0, "the block lays no spoil");
        assert!(f[4].bounces > 0, "the disc doesn't bounce");
    }
}
