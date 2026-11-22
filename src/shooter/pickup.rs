//! Things lying on the floor waiting to be walked over.
//!
//! An arena is a map rather than a room because of these. `give_all` hands
//! out everything at every spawn, which is fine for finding out whether bots
//! can fight and useless afterwards: if every fighter is always whole and
//! always carrying every gun, the floor has nothing on it worth crossing for
//! and the fight has no geography. A health pack in a dead end is a decision.
//!
//! Four kinds, and they are verbs rather than nouns -- Rust rather than a
//! library file -- because each one is a thing the rules do to a `Fighter`
//! and there are only four of them. What *is* data is where they stand, how
//! much they give and how long they take to come back, which lives in the
//! world (`fields::PickupDef`).
//!
//! Two rules make them feel right and both are older than this engine:
//!
//! - A pad over a fighter who has no use for it stays standing. Walking over
//!   a health pack at full health must not waste it, or the player learns to
//!   edge around their own pickups.
//! - A pad that has been taken comes back on its own clock, and the clock is
//!   per pad rather than per kind, so the rocket launcher can be the slow one.

use crate::brain::{Fighter, Lure};
use crate::world::fields::PickupDef;
use glam::Vec3;

/// What a pad does to whoever walks over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Health,
    Armour,
    Ammo,
    Weapon,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "health" => Some(Kind::Health),
            "armour" | "armor" => Some(Kind::Armour),
            "ammo" => Some(Kind::Ammo),
            "weapon" => Some(Kind::Weapon),
            _ => None,
        }
    }

    pub const NAMES: [&'static str; 4] = ["health", "armour", "ammo", "weapon"];

    /// How much it gives when the world does not say, and how long it takes
    /// to come back. A weapon's amount is rounds handed over with the gun.
    fn default_amount(self) -> f32 {
        match self {
            // The middle of the three (`Size::AMOUNTS`).
            Kind::Health => Size::AMOUNTS[1],
            Kind::Armour => 50.0,
            Kind::Ammo => 0.0,
            Kind::Weapon => 0.0,
        }
    }

    fn default_respawn(self) -> f32 {
        match self {
            Kind::Health => 20.0,
            Kind::Armour => 30.0,
            Kind::Ammo => 15.0,
            Kind::Weapon => 25.0,
        }
    }

    /// The colour of the lit blob, which is the only thing telling you what
    /// it is from across a hall. Green mends, blue shields, amber is rounds,
    /// white is a gun -- and the gun's own model is drawn over it, so the
    /// white is a glow under it rather than the whole of it.
    pub fn tint(self) -> [f32; 3] {
        match self {
            Kind::Health => [0.30, 1.00, 0.44],
            Kind::Armour => [0.36, 0.62, 1.00],
            Kind::Ammo => [1.00, 0.82, 0.26],
            Kind::Weapon => [0.86, 0.90, 1.00],
        }
    }
}

/// How big a health pad is.
///
/// Three of them, because one size makes every health pad on the map the
/// same decision. A vial is worth a step sideways and nothing more; a crate
/// is worth crossing the deck for, and worth being seen crossing it. Which
/// one a pad is follows from the amount the world gives it, so a world says
/// only how much it mends and the size, the look, the mark over it and the
/// clock all come out of that.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Vial,
    Pack,
    Crate,
}

impl Size {
    /// What the three of them mend, which is also how a world asks for one.
    pub const AMOUNTS: [f32; 3] = [10.0, 25.0, 50.0];

    /// Which size an amount is. The bands are wide on purpose: a world that
    /// says 40 wants the big one, not a pack drawn small and lying about it.
    pub fn of(amount: f32) -> Size {
        match amount {
            a if a <= 15.0 => Size::Vial,
            a if a <= 35.0 => Size::Pack,
            _ => Size::Crate,
        }
    }

    /// The big one is worth holding ground for, so it is the slow one.
    fn respawn(self) -> f32 {
        match self {
            Size::Vial => 12.0,
            Size::Pack => 20.0,
            Size::Crate => 40.0,
        }
    }
}

/// Has this fighter anything it can fire at all? A bot with an empty gun
/// and nothing to swap to is not fighting, whatever it is doing, and should
/// be crossing the map for the nearest thing that changes that.
fn armed(f: &Fighter) -> bool {
    f.arsenal.guns.iter().any(|g| !g.empty())
}

/// One pad on the floor.
/// What rounds for a gun not carried are worth, when they are taken at all.
const STASH_WORTH: f32 = 0.12;

/// What topping health up from full to the cap is worth, all of it.
const OVERHEAL_WORTH: f32 = 0.4;

pub struct Pad {
    pub kind: Kind,
    /// Where it stands, feet on the floor.
    pub at: Vec3,
    pub amount: f32,
    /// The library weapon this one is for, as an index into the arsenal's
    /// slot order; `None` on health and armour.
    pub weapon: Option<String>,
    pub respawn_s: f32,
    /// Seconds until it comes back; 0 while it is standing there.
    pub gone: f32,
    /// The height the world stated for it, kept so the pad can be asked
    /// again once the ground under it has streamed in.
    up: Option<f32>,
    /// Whether `at` is the real floor, or the height the world stated
    /// because the column was not loaded yet (`settle`).
    pub placed: bool,
    /// Ammunition for a gun not carried is taken and kept
    /// (`ShooterDef::ammo_without_gun`).
    pub stash: bool,
    /// The slab material of the platform under it (`PickupDef::pad_material`).
    platform: Option<String>,
    /// Whether the platform has been laid, or there is none to lay.
    stamped: bool,
}

impl Pad {
    pub fn ready(&self) -> bool {
        self.gone <= 0.0
    }

    /// How big the thing over this pad is drawn, and how much taller its
    /// beacon stands, by how much it gives: 1 for the middle health pack
    /// and the default armour, and for every pad of rounds or a gun. Not
    /// in steps, so a world that gives 40 armour shows a plate between the
    /// 30 and the 50; and under the square root, so a hundred-point pad is
    /// big enough to cross the map for without filling the room.
    pub fn scale(&self) -> f32 {
        let norm = match self.kind {
            Kind::Health => Size::AMOUNTS[1],
            Kind::Armour => Kind::Armour.default_amount(),
            _ => return 1.0,
        };
        (self.amount / norm).max(0.0).powf(0.55).clamp(0.6, 2.2)
    }

    /// Which of the three sizes this is, `None` on any kind but health.
    #[cfg(test)]
    pub fn size(&self) -> Option<Size> {
        (self.kind == Kind::Health).then(|| Size::of(self.amount))
    }

    /// How much good this pad would do the fighter standing on it, 0 for one
    /// it has no use for. This is also the whole of what a bot is told about
    /// it (`brain::Lure`): the brain never learns what a health pack is, it
    /// is handed a point in the world and a number.
    pub fn good(&self, f: &Fighter) -> f32 {
        if !self.ready() {
            return 0.0;
        }
        match self.kind {
            // Worth most to a fighter who is nearly out, and nothing to one
            // who is whole. Squared, so a bot on its last legs crosses a map
            // for one and a scratched bot does not break off a fight.
            // Worth most to a fighter who is nearly out, and nothing to one
            // who is whole. Squared, so a bot on its last legs crosses a map
            // for one and a scratched bot does not break off a fight.
            //
            // Then by how much of the gap this pad actually closes, or the
            // three sizes are three looks and one decision: a bot at ten
            // health wanted a vial exactly as much as the crate across the
            // hall, and took whichever was nearer. A vial still beats
            // nothing, which is why the term starts at 0.4 rather than 0.
            Kind::Health => {
                if f.health >= f.max_health {
                    return 0.0;
                }
                // Whole but not at the cap: a pad is worth a little, enough
                // to take one in passing and never enough to cross the map.
                if f.health >= f.full_health {
                    let room = (f.max_health - f.health) / (f.max_health - f.full_health).max(1.0);
                    // Enough to go for, not only to take in passing: a bot
                    // with nothing to do builds its stock, as a player does.
                    return OVERHEAL_WORTH * room.clamp(0.0, 1.0);
                }
                //
                // To the power one and a half and scaled so that a bot at a
                // third of its health wants a pack about as much as a gun it
                // has not got. Squared and scaled to 1.4 it never did: a new
                // gun is worth 1.0 and up, and the league measured the cost
                // -- of forty-odd times a bot fell under half health in a
                // match, two or three ended at a health pad. The rest walked
                // past the packs to the guns and died with them.
                let short = 1.0 - (f.health / f.full_health.max(1.0)).clamp(0.0, 1.0);
                let gap = f.full_health - f.health;
                let closes = (self.amount / gap.max(1.0)).clamp(0.0, 1.0);
                short.powf(1.5) * 2.6 * (0.4 + 0.6 * closes)
            }
            Kind::Armour => {
                // The same test `give` makes, and it has to be the same
                // test. A world that carries armour pads but gives nobody
                // any armour capacity -- which Slagdeck did -- has
                // `full_armour` of zero, so "how much armour is missing"
                // came out as all of it, for ever: every bot wanted every
                // armour pad at all times, walking over it did nothing
                // because `give` refused, so the pad stayed standing and
                // stayed wanted. Two bots spent whole matches pacing
                // between two pads they could not use. A pad that cannot do
                // anything is worth nothing, and the only safe way to say
                // that is to ask the question `give` asks.
                if f.armour >= f.full_armour {
                    return 0.0;
                }
                let missing = 1.0 - (f.armour / f.full_armour.max(1.0)).clamp(0.0, 1.0);
                missing * 0.8
            }
            Kind::Ammo => self.arms_worth(f) * self.alive_to_use(f),
            Kind::Weapon => self.arms_worth(f) * self.alive_to_use(f),
        }
    }

    /// What a gun or rounds are worth to somebody this hurt: less the nearer
    /// dead they are, down to a fifth at the brink. A player at ten health
    /// goes for the pack, not the rocket launcher, and a bot did the
    /// opposite -- the league caught one at 7 health crossing the map four times
    /// in a minute for guns, walking past the packs on the way.
    fn alive_to_use(&self, f: &Fighter) -> f32 {
        let whole = (f.health / f.full_health.max(1.0)).clamp(0.0, 1.0);
        0.2 + 0.8 * whole
    }

    fn arms_worth(&self, f: &Fighter) -> f32 {
        match self.kind {
            Kind::Ammo => match self.weapon.as_deref().and_then(|n| f.arsenal.guns.iter().find(|g| g.def.name == n)) {
                Some(g) if g.wants_ammo() => {
                    let share = g.ammo as f32 / g.def.ammo_max.max(1) as f32;
                    // A gun with nothing in it is a different thing from a
                    // gun that is merely not full, and if it is the only one
                    // being carried then rounds for it are the only thing on
                    // the map that matters. Measured before this was here: a
                    // bot fired its 220 starting rounds in the first twelve
                    // seconds of a match and then walked about unarmed for
                    // the remaining hundred, because the walk to the ammo pad
                    // cost more than it scored.
                    let dry = if g.ammo < g.def.fire.per_shot { 0.8 } else { 0.0 };
                    let last = if armed(f) { 0.0 } else { 0.8 };
                    0.2 + 0.4 * (1.0 - share) + dry + last
                }
                // Rounds put by for a gun it may find: worth a little.
                Some(g) if self.stash && !g.have && g.ammo < g.def.ammo_max => STASH_WORTH,
                _ => 0.0,
            },
            Kind::Weapon => match self.weapon.as_deref().and_then(|n| f.arsenal.guns.iter().find(|g| g.def.name == n)) {
                // A gun it has not got is the best thing on the floor, and
                // the better gun is the one with the higher slot, which is
                // how the library already orders them.
                Some(g) if !g.have => 1.0 + 0.1 * g.def.slot as f32 + if armed(f) { 0.0 } else { 0.8 },
                Some(g) if g.wants_ammo() => 0.35,
                _ => 0.0,
            },
            _ => 0.0,
        }
    }

    /// Hand it over. Returns what to put on screen, or `None` if this
    /// fighter had no use for it -- in which case the pad is left standing.
    /// For tests: the same hand-over, under a name a test may call.
    #[cfg(test)]
    fn clone_give(&self, f: &mut Fighter) -> Option<String> {
        self.give(f)
    }

    fn give(&self, f: &mut Fighter) -> Option<String> {
        match self.kind {
            Kind::Health => {
                if f.health >= f.max_health {
                    return None;
                }
                f.health = (f.health + self.amount).min(f.max_health);
                Some(format!("HEALTH +{}", self.amount.round() as i32))
            }
            Kind::Armour => {
                if f.armour >= f.full_armour {
                    return None;
                }
                f.armour = (f.armour + self.amount).min(f.full_armour);
                Some(format!("ARMOUR +{}", self.amount.round() as i32))
            }
            Kind::Ammo => {
                let name = self.weapon.clone()?;
                let g = f.arsenal.named_mut(&name)?;
                if self.stash && !g.have {
                    if g.ammo >= g.def.ammo_max {
                        return None;
                    }
                    let rounds = if self.amount > 0.0 { self.amount as u32 } else { g.def.ammo_max / 2 };
                    g.ammo = (g.ammo + rounds).min(g.def.ammo_max);
                    return Some(format!("{} ROUNDS +{rounds}", g.def.display));
                }
                if !g.wants_ammo() {
                    return None;
                }
                let rounds = if self.amount > 0.0 { self.amount as u32 } else { g.def.ammo_max / 2 };
                let display = g.def.display.clone();
                g.give(rounds);
                Some(format!("{display} +{rounds}"))
            }
            Kind::Weapon => {
                let name = self.weapon.clone()?;
                let g = f.arsenal.named_mut(&name)?;
                if g.have && !g.wants_ammo() {
                    return None;
                }
                let display = g.def.display.clone();
                let first = g.found(self.amount as u32);
                // Take it out at once if it is new and better than what is
                // in hand: a gun picked up and not drawn is a gun you forget
                // you have.
                if first {
                    f.arsenal.draw_something();
                }
                Some(if first { display } else { format!("{display} AMMO") })
            }
        }
    }
}

/// Every pad in the match.
#[derive(Default)]
pub struct Pickups {
    pub pads: Vec<Pad>,
}

/// How far each way from the middle the platform under a pad reaches, in
/// voxels: a square a metre across, wider under a pickup drawn bigger than
/// the middle size (`Pad::scale`). Square, because a disc this few voxels
/// across comes out as a plus, which on every pad reads as a health cross.
const PLATFORM_R: f32 = 2.0;
/// How high the platform stands: a slab is half a voxel.
pub const PLATFORM_M: f32 = 0.125;
/// How far above the floor `floor_at` stands a pad, metres.
const PAD_LIFT: f32 = 0.1;
/// Voxels of air a platform needs over it: a standing body (1.75 m, seven
/// voxels) on top of the half voxel the platform raises it by. Less, and
/// the step onto it puts the head in the ceiling, so in a tunnel built to
/// body height the platform is a wall.
const PLATFORM_HEADROOM: i32 = 8;
/// How far round the platform, in voxels, the same headroom is wanted: a
/// body steps up while still standing beside it, so the ceiling over the
/// approach counts as much as the ceiling over the platform.
const PLATFORM_APPROACH: i32 = 2;

/// How close you have to be to take one, metres from the middle of the pad.
/// Generous on purpose: a pad you have to stand exactly on is a pad you walk
/// past at speed, and speed is the whole point of an arena.
const REACH: f32 = 1.3;
/// And how far above and below it, so a pad is taken by a body running over
/// it and not by one on the floor above.
const REACH_UP: f32 = 2.2;

impl Pickups {
    /// The pads a world asks for. `floor` places each one the way a spawn is
    /// placed, and a kind nobody recognises is reported and dropped.
    pub fn new(defs: &[PickupDef], floor: impl Fn(f32, f32, Option<f32>) -> (Vec3, bool), mut warn: impl FnMut(String)) -> Pickups {
        let mut pads = Vec::new();
        for d in defs {
            let Some(kind) = Kind::parse(&d.kind) else {
                warn(format!("no pickup kind called {} (one of {:?})", d.kind, Kind::NAMES));
                continue;
            };
            if matches!(kind, Kind::Ammo | Kind::Weapon) && d.weapon.is_none() {
                warn(format!("a {} pad has to say which weapon", d.kind));
                continue;
            }
            let (at, placed) = floor(d.at[0], d.at[1], d.up);
            let amount = d.amount.unwrap_or_else(|| kind.default_amount());
            pads.push(Pad {
                kind,
                at,
                amount,
                weapon: d.weapon.clone(),
                respawn_s: d.respawn_s.unwrap_or_else(|| match kind {
                    // A health pad's clock is its size's, not its kind's.
                    Kind::Health => Size::of(amount).respawn(),
                    _ => kind.default_respawn(),
                }),
                gone: 0.0,
                up: d.up,
                placed,
                stash: false,
                platform: d.pad_material(),
                stamped: false,
            });
        }
        Pickups { pads }
    }

    pub fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }

    /// How many pads are still standing at the height the world stated
    /// rather than on ground anybody has looked at.
    pub fn unplaced(&self) -> usize {
        self.pads.iter().filter(|p| !p.placed).count()
    }

    /// Ask again for the floor under the pads that had none. A chunk that
    /// has not streamed in yet leaves a pad at the height the world file
    /// gives, which is right to a voxel or two on a flat map and a storey
    /// out on one with floors eleven metres apart -- a gun sunk in the
    /// floor of the room above. Nothing else about the pad changes, so a
    /// pad that has been taken keeps its clock.
    pub fn settle(&mut self, floor: impl Fn(f32, f32, Option<f32>) -> (Vec3, bool)) {
        for p in &mut self.pads {
            if p.placed {
                continue;
            }
            let (at, placed) = floor(p.at.x, p.at.z, p.up);
            p.at = at;
            p.placed = placed;
        }
    }

    /// The voxels of the platforms still to be laid: a square of slab under
    /// every pad that has found its floor. A platform goes down only where
    /// the floor is level with the pad's, so one at the foot of a wall or
    /// on the lip of a stair does not climb it. The pad stands on its
    /// platform from then on.
    ///
    /// It is laid by the rules rather than built into the world, so every
    /// world with pickups gets them whatever made its floor, and a world
    /// file says nothing more than where a pad is and what it gives.
    pub fn platforms(&mut self, store: &crate::world::store::Store, out: &mut Vec<([i32; 3], u8)>) {
        for p in &mut self.pads {
            if p.stamped || !p.placed {
                continue;
            }
            p.stamped = true;
            let Some(mat) = p.platform.as_deref().and_then(crate::world::matlib::id_of) else {
                continue;
            };
            let foot = (p.at.y - PAD_LIFT) * 4.0;
            // Standing on one already: a match started again on the same
            // ground, which kept the last one's.
            if (foot - foot.floor() - 0.5).abs() < 0.1 {
                continue;
            }
            let row = foot.round() as i32;
            let (cx, cz) = (p.at.x * 4.0, p.at.z * 4.0);
            let reach = PLATFORM_R * p.scale().max(1.0);
            let r = reach.ceil() as i32;
            // Under a low ceiling, or with one over the way in: no
            // platform, rather than one nobody can get onto. A column that
            // is wall at the floor, or has no floor, is not a ceiling.
            let (fx, fz) = (cx.floor() as i32, cz.floor() as i32);
            let a = r + PLATFORM_APPROACH;
            let low = (fz - a..=fz + a).any(|z| {
                (fx - a..=fx + a).any(|x| store.get(x, row, z) == Some(crate::world::material::AIR) && store.solid(x, row - 1, z) && (1..PLATFORM_HEADROOM).any(|k| store.solid(x, row + k, z)))
            });
            if low {
                continue;
            }
            let mut under = false;
            for z in cz.floor() as i32 - r..=cz.floor() as i32 + r {
                for x in cx.floor() as i32 - r..=cx.floor() as i32 + r {
                    let (dx, dz) = (x as f32 + 0.5 - cx, z as f32 + 0.5 - cz);
                    if dx.abs() > reach || dz.abs() > reach {
                        continue;
                    }
                    let level = store.get(x, row, z) == Some(crate::world::material::AIR) && store.solid(x, row - 1, z) && !store.slab(x, row - 1, z);
                    if level {
                        out.push(([x, row, z], mat));
                        under |= x == cx.floor() as i32 && z == cz.floor() as i32;
                    }
                }
            }
            if under {
                p.at.y += PLATFORM_M;
            }
        }
    }

    pub fn step(&mut self, dt: f32) {
        for p in &mut self.pads {
            if p.gone > 0.0 {
                p.gone = (p.gone - dt).max(0.0);
            }
        }
    }

    /// Hand over anything the fighter at `foot` is standing on. Returns what
    /// to say about the last one taken, for whoever cares (the player's HUD;
    /// a bot takes its pickups in silence).
    pub fn walk_over(&mut self, foot: Vec3, f: &mut Fighter) -> Option<String> {
        let mut said = None;
        for p in &mut self.pads {
            if !p.ready() {
                continue;
            }
            let d = Vec3::new(p.at.x - foot.x, 0.0, p.at.z - foot.z).length();
            if d > REACH || (foot.y - p.at.y).abs() > REACH_UP {
                continue;
            }
            if let Some(note) = p.give(f) {
                p.gone = p.respawn_s;
                f.picked += 1;
                said = Some(note);
            }
        }
        said
    }

    /// The pads worth something to this fighter, as the brain sees them.
    pub fn lures(&self, f: &Fighter, out: &mut Vec<Lure>) {
        out.clear();
        for p in &self.pads {
            let good = p.good(f);
            if good > 0.0 {
                let gun = match p.kind {
                    Kind::Weapon => p.weapon.as_deref().and_then(|n| f.arsenal.guns.iter().position(|g| g.def.name == n && !g.have)),
                    _ => None,
                };
                out.push(Lure {
                    at: p.at,
                    good,
                    gun,
                    mends: p.kind == Kind::Health,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::weapon::Arsenal;
    use crate::library::Library;

    fn fighter(lib: &Library, give_all: bool) -> Fighter {
        let names = ["minigun".to_string(), "rocket_launcher".to_string(), "scrap_gun".to_string()];
        let mut f = Fighter::new(Arsenal::new(lib, &names, give_all, |n| panic!("{n}")), 100.0, 50.0);
        if !give_all {
            f.arsenal.strip();
        }
        f
    }

    /// A health pad's definition, as a world states it.
    fn p0() -> PickupDef {
        PickupDef {
            kind: "health".into(),
            at: [0.0, 0.0],
            up: None,
            amount: None,
            weapon: None,
            respawn_s: None,
        }
    }

    fn pads(defs: &[(&str, Option<&str>)]) -> Pickups {
        let defs: Vec<PickupDef> = defs
            .iter()
            .map(|(k, w)| PickupDef {
                kind: k.to_string(),
                at: [0.0, 0.0],
                up: None,
                amount: None,
                weapon: w.map(|s| s.to_string()),
                respawn_s: None,
            })
            .collect();
        Pickups::new(&defs, |x, z, _| (Vec3::new(x, 0.0, z), true), |w| panic!("{w}"))
    }

    /// A pad that has found its floor gets a platform under it: a square of
    /// slab on the level floor round it and nowhere else, laid once, with
    /// the pad lifted onto it.
    #[test]
    fn a_placed_pad_gets_a_platform() {
        use crate::world::material::{AIR, STONE};
        use crate::world::store::{Store, Voxels};
        let (_, _table) = crate::physics::tests::slab_material();
        // By the name the pad asks for, which `worldload::reach` hands
        // every world with pads rather than every world there is.
        let name = p0().pad_material().unwrap();
        let mat = crate::world::matlib::id_of(&name).expect("the health platform has an id");
        let mut store = Store::default();
        for cz in -1..=1 {
            for cx in -1..=1 {
                store.chunks.insert((cx, 0, cz), Voxels::Uniform(STONE));
                store.chunks.insert((cx, 1, cz), Voxels::Uniform(AIR));
            }
        }
        // A wall a metre to the east: the disc must not climb it.
        for z in -8..8 {
            for y in 32..40 {
                store.set(6, y, z, STONE);
            }
        }
        let mut p = pads(&[("health", None)]);
        p.pads[0].at = Vec3::new(1.0, 8.1, 0.0);
        let mut out = Vec::new();
        p.platforms(&store, &mut out);
        assert!(out.len() >= 12, "only {} voxels of platform", out.len());
        assert!(out.iter().all(|&([x, y, _], m)| y == 32 && m == mat && x < 6), "{out:?}");
        assert!(out.iter().any(|&(at, _)| at == [4, 32, 0]), "nothing under the pad's middle");
        assert!((p.pads[0].at.y - 8.225).abs() < 1e-4, "the pad was not lifted onto it: {}", p.pads[0].at.y);
        out.clear();
        p.platforms(&store, &mut out);
        assert!(out.is_empty(), "laid twice");

        // In a tunnel a body only just fits, there is no room to step up
        // onto one: none is laid and the pad stays on the floor.
        for z in -8..8 {
            for x in -4..6 {
                store.set(x, 32, z, AIR);
                store.set(x, 39, z, STONE);
            }
        }
        let mut p = pads(&[("health", None)]);
        p.pads[0].at = Vec3::new(1.0, 8.1, 0.0);
        p.platforms(&store, &mut out);
        assert!(out.is_empty(), "a platform in a 1.75 m tunnel: {out:?}");
        assert!((p.pads[0].at.y - 8.1).abs() < 1e-4, "the pad moved: {}", p.pads[0].at.y);
    }

    /// A pad over somebody who has no use for it is left standing, and one
    /// that is taken comes back on its own clock.
    #[test]
    fn a_pad_you_cannot_use_is_not_wasted() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let mut p = pads(&[("health", None)]);
        let mut f = fighter(&lib, true);
        assert_eq!(p.walk_over(Vec3::ZERO, &mut f), None, "it took a health pack at full health");
        assert!(p.pads[0].ready(), "and used it up doing so");
        f.take(60.0);
        let before = f.health;
        assert!(p.walk_over(Vec3::ZERO, &mut f).is_some());
        assert!(f.health > before);
        assert!(!p.pads[0].ready(), "it is still standing there");
        // Back after its own time, not before.
        p.step(p.pads[0].respawn_s - 0.5);
        assert!(!p.pads[0].ready());
        p.step(1.0);
        assert!(p.pads[0].ready());
    }

    /// With a cap over the spawn health, a pad heals past it and stops at the
    /// cap; a bot at full health wants one a little, never as much as a hurt
    /// one does.
    #[test]
    fn health_pads_heal_past_full_up_to_the_cap() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let mut f = fighter(&lib, true);
        f.health_cap(200.0);
        let mut p = pads(&[("health", None)]);
        let want_whole = p.pads[0].good(&f);
        assert!(want_whole > 0.0, "a pad was worth nothing to a fighter under the cap");
        for _ in 0..40 {
            p.walk_over(Vec3::ZERO, &mut f);
            p.step(p.pads[0].respawn_s + 0.1);
        }
        assert_eq!(f.health, 200.0, "pads healed to {}", f.health);
        assert_eq!(p.walk_over(Vec3::ZERO, &mut f), None, "it took a pad at the cap");
        f.health = 30.0;
        assert!(p.pads[0].good(&f) > want_whole * 2.0, "a hurt fighter wants a pad no more than a whole one");
    }

    /// Three sizes of health pad, told apart by what they mend: the amount
    /// is the only thing a world states and the size, the mark drawn over
    /// it and the clock all follow from it.
    #[test]
    fn health_comes_in_three_sizes() {
        let defs: Vec<PickupDef> = Size::AMOUNTS
            .iter()
            .map(|&a| PickupDef {
                kind: "health".into(),
                at: [0.0, 0.0],
                up: None,
                amount: Some(a),
                weapon: None,
                respawn_s: None,
            })
            .collect();
        let p = Pickups::new(&defs, |x, z, _| (Vec3::new(x, 0.0, z), true), |w| panic!("{w}"));
        let sizes: Vec<Option<Size>> = p.pads.iter().map(|d| d.size()).collect();
        assert_eq!(sizes, [Some(Size::Vial), Some(Size::Pack), Some(Size::Crate)]);
        // Bigger means drawn bigger and back slower, in that order.
        for w in p.pads.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            assert!(b.scale() > a.scale(), "the bigger pad is not drawn bigger");
            assert!(b.respawn_s > a.respawn_s, "the bigger pad comes back as fast: {} vs {}", a.respawn_s, b.respawn_s);
        }
        // Nothing else has a size, and a pad with no stated amount is the
        // middle one, so every world written before this reads the same.
        let plain = pads(&[("health", None), ("armour", None)]);
        assert_eq!(plain.pads[0].size(), Some(Size::Pack));
        assert_eq!(plain.pads[1].size(), None);
        assert_eq!(plain.pads[0].respawn_s, p.pads[1].respawn_s);
    }

    /// A badly hurt bot wants the big one, and would still take the vial if
    /// that were all there was.
    #[test]
    fn a_dying_bot_prefers_the_crate() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let defs: Vec<PickupDef> = [Size::AMOUNTS[0], Size::AMOUNTS[2]]
            .iter()
            .map(|&a| PickupDef {
                kind: "health".into(),
                at: [0.0, 0.0],
                up: None,
                amount: Some(a),
                weapon: None,
                respawn_s: None,
            })
            .collect();
        let p = Pickups::new(&defs, |x, z, _| (Vec3::new(x, 0.0, z), true), |w| panic!("{w}"));
        let mut f = fighter(&lib, true);
        f.take(85.0);
        let (vial, crate_) = (p.pads[0].good(&f), p.pads[1].good(&f));
        assert!(crate_ > vial * 1.5, "a dying bot rates a vial as highly as a crate: {vial} vs {crate_}");
        assert!(vial > 0.0, "it would not cross the room for a vial at fifteen health");
        // Grazed, both close the whole gap, so neither is worth much and
        // neither is worth more: there is nothing to choose between them.
        let mut f = fighter(&lib, true);
        f.take(5.0);
        let (vial, crate_) = (p.pads[0].good(&f), p.pads[1].good(&f));
        assert!((vial - crate_).abs() < 1e-6, "a scratch is worth a crate more than a vial: {vial} vs {crate_}");
        assert!(crate_ < 0.05, "a scratched bot breaks off a fight for health: {crate_}");
    }

    /// A weapon pad hands over a gun you are not carrying, and the gun comes
    /// out: the arsenal holds every slot the world has whether or not you
    /// have found what goes in it.
    #[test]
    fn a_weapon_pad_arms_you() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let mut p = pads(&[("weapon", Some("rocket_launcher"))]);
        let mut f = fighter(&lib, false);
        let carried = |f: &Fighter| f.arsenal.guns.iter().filter(|g| g.have).count();
        assert_eq!(carried(&f), 1, "a stripped fighter is carrying more than its sidearm");
        // The slot off the gun's own entry, not a number written here: the
        // library renumbered the slots this afternoon and a test that knows
        // the launcher is on digit two is a test that breaks for no reason.
        let slot = f.arsenal.guns.iter().find(|g| g.def.name == "rocket_launcher").expect("a launcher").def.slot;
        assert!(!f.arsenal.pick(slot), "it could draw a gun it has not found");
        let note = p.walk_over(Vec3::ZERO, &mut f).expect("it refused a gun it had not got");
        assert!(note.contains("ROCKET"), "the note does not name the gun: {note}");
        assert_eq!(carried(&f), 2);
        assert!(f.arsenal.pick(slot), "it still cannot draw the gun it just picked up");
        assert!(f.arsenal.current().is_some_and(|g| g.ammo > 0), "it came with no rounds");
    }

    /// What a bot is told: a point and a number, with the number saying how
    /// badly it wants the thing rather than what the thing is.
    #[test]
    fn a_pad_rates_itself_for_whoever_is_asking() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let p = pads(&[("health", None), ("weapon", Some("rocket_launcher"))]);
        let mut whole = fighter(&lib, true);
        let mut out = Vec::new();
        p.lures(&whole, &mut out);
        assert!(out.is_empty(), "a fighter with everything was still tempted: {}", out.len());
        // Nearly dead, and the health pack is now worth more than it was.
        whole.take(90.0);
        p.lures(&whole, &mut out);
        assert_eq!(out.len(), 1);
        let dying = out[0].good;
        let mut scratched = fighter(&lib, true);
        scratched.take(10.0);
        p.lures(&scratched, &mut out);
        let grazed = out.first().map_or(0.0, |l| l.good);
        assert!(dying > grazed * 4.0, "a dying bot wants health no more than a grazed one: {dying} vs {grazed}");
        // And a gun it has not got beats anything else on the floor.
        let unarmed = fighter(&lib, false);
        p.lures(&unarmed, &mut out);
        let best = out.iter().map(|l| l.good).fold(0.0, f32::max);
        assert!(best > 1.0, "an unarmed bot does not want the rocket launcher: {best}");
    }

    /// A kind nobody recognises is reported, not a crash, and an ammo pad
    /// that does not say which weapon is refused.
    #[test]
    fn a_world_that_asks_for_nonsense_is_told() {
        let mut said = Vec::new();
        let defs = vec![
            PickupDef {
                kind: "sandwich".into(),
                at: [0.0, 0.0],
                up: None,
                amount: None,
                weapon: None,
                respawn_s: None,
            },
            PickupDef {
                kind: "ammo".into(),
                at: [0.0, 0.0],
                up: None,
                amount: None,
                weapon: None,
                respawn_s: None,
            },
        ];
        let p = Pickups::new(&defs, |x, z, _| (Vec3::new(x, 0.0, z), true), |w| said.push(w));
        assert!(p.is_empty());
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].contains("sandwich") && said[1].contains("weapon"), "{said:?}");
    }

    /// Slagdeck's own pads, as the world file has them: every one standing
    /// on a nav node, so a bot can always route to the thing it wants, and
    /// every kind spelled a way the engine knows.
    ///
    /// The layout is generated by `worlds/slagdeck/make_arena.py` off node
    /// names and asserted there too, but the generator is not run on a build
    /// and the file it writes is what ships, so this reads the file.
    #[test]
    fn the_arena_pads_stand_on_nav_nodes() {
        let text = std::fs::read_to_string("worlds/slagdeck/world.json").expect("slagdeck world.json");
        let v: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        let sh = &v["shooter"];
        let pads = sh["pickups"].as_array().expect("a pickups array");
        let nodes = sh["nav"]["nodes"].as_array().expect("a nav graph");
        assert!(pads.len() >= 10, "{} pads is not an arena", pads.len());
        let mut kinds = std::collections::BTreeSet::new();
        for p in pads {
            let kind = p["kind"].as_str().expect("a kind");
            assert!(Kind::parse(kind).is_some(), "no pickup kind called {kind}");
            kinds.insert(kind.to_string());
            let (x, z) = (p["at"][0].as_f64().unwrap(), p["at"][1].as_f64().unwrap());
            let on = nodes.iter().any(|n| {
                let (nx, nz) = (n["at"][0].as_f64().unwrap(), n["at"][1].as_f64().unwrap());
                (nx - x).abs() < 0.01 && (nz - z).abs() < 0.01
            });
            assert!(on, "the {kind} pad at {x},{z} is not on a nav node, so no bot can go and get it");
        }
        assert_eq!(kinds.len(), 4, "the arena is missing a kind of pickup: {kinds:?}");
        // `give_all` is deliberately not asserted either way. With it on, a
        // respawn hands over every gun and a weapon pad is only an ammo pad;
        // with it off the pads are the whole economy. Slagdeck runs with it
        // on, because off made bots that burned their starting rounds stand
        // about unarmed (docs/SHOOTER.md). What has to
        // hold in both worlds is below -- that a pad names a gun this world
        // actually issues, or it can never do anything at all.
        // Every weapon the pads name is one the world actually carries.
        let guns: Vec<&str> = sh["weapons"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
        for p in pads {
            if let Some(w) = p["weapon"].as_str() {
                assert!(guns.contains(&w), "a pad holds {w}, which this world does not issue");
            }
        }
    }

    /// A world may issue no armour and still have armour on its floor: the
    /// capacity is what makes a pad worth taking, and a respawn hands back
    /// what the world issues rather than what was picked up. This is the
    /// arrangement the imported maps use -- nothing worn at spawn, a
    /// hundred and fifty to fill from the floor.
    #[test]
    fn armour_off_the_floor_is_not_handed_back_at_a_respawn() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let names = ["minigun".to_string()];
        let mut f = Fighter::new(Arsenal::new(&lib, &names, true, |n| panic!("{n}")), 100.0, 0.0);
        f.armour_cap(150.0);
        let mut p = pads(&[("armour", None)]);
        assert!(p.pads[0].good(&f) > 0.0, "a fighter with room for armour did not want any");
        assert!(p.walk_over(Vec3::ZERO, &mut f).is_some(), "the armour pad was refused");
        assert_eq!(f.armour, 50.0);
        f.die(Vec3::ZERO, 1.0, Vec3::ZERO);
        f.revive(true);
        assert_eq!(f.armour, 0.0, "a respawn handed back armour that was picked up");
        assert_eq!(f.full_armour, 150.0, "the capacity did not survive the respawn");
    }

    /// A pad that cannot do anything is worth nothing to a bot. This is the
    /// invariant the armour magnet broke: `good` and `give` have to agree,
    /// or a bot crosses the map for something that does not happen and then
    /// crosses it again.
    #[test]
    fn nothing_wants_a_pad_that_would_do_nothing() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let mut p = pads(&[("health", None), ("armour", None), ("ammo", Some("minigun"))]);
        // A world that issues no armour at all: the capacity is zero, so an
        // armour pad can never do anything for anybody.
        let names = ["minigun".to_string()];
        let mut none = Fighter::new(Arsenal::new(&lib, &names, true, |n| panic!("{n}")), 100.0, 0.0);
        let mut out = Vec::new();
        p.lures(&none, &mut out);
        assert!(out.is_empty(), "a fighter who can wear no armour still wanted some: {}", out.len());
        assert_eq!(p.walk_over(Vec3::ZERO, &mut none), None);
        assert!(p.pads.iter().all(|x| x.ready()), "something was taken that could not be used");
        // Hurt, and the health pad is wanted and works.
        none.take(50.0);
        p.lures(&none, &mut out);
        assert_eq!(out.len(), 1, "{out:?}", out = out.len());
        assert!(p.walk_over(Vec3::ZERO, &mut none).is_some());
        // Every kind agrees both ways: if it is wanted it does something,
        // and if it does something it was wanted.
        for f in [&none] {
            for pad in &p.pads {
                let wanted = pad.good(f) > 0.0;
                let mut copy = Fighter::new(Arsenal::new(&lib, &names, true, |n| panic!("{n}")), 100.0, 0.0);
                copy.health = f.health;
                copy.armour = f.armour;
                let did = pad.clone_give(&mut copy).is_some();
                assert_eq!(wanted && pad.ready(), did && pad.ready(), "{:?} is wanted {wanted} and does {did}", pad.kind);
            }
        }
    }
}
