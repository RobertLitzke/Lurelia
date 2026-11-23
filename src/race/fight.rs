//! The race's side of the weapons (`arms.rs`): who carries what, who fires
//! it and where, and what the hits do to the karts. Kart 0 is the player's,
//! kart `i + 1` is rival `i`.

use super::arms::{assist, Harm, Item, Pose, Slot, BLOCK_DAMAGE, BLOCK_HOP, BLOCK_KEEP, KART_SIZE, OVER_SECS, REPAIR, SHIELD_SECS, SPIN_SECS, TURNED_SECS};
use super::{boost_kick, Bot, Race, NAMES};
use crate::player::Player;
use crate::ride::Ride;
use crate::vehicle::{Controls, Vehicle};
use glam::{Quat, Vec3};

/// A spun kart keeps this share of its speed.
const SPIN_KEEP: f32 = 0.35;
/// Overdrive: top speed raised by this share, pushed toward it this hard
/// (m/s²).
const OVER_TOP: f32 = 1.15;
const OVER_PUSH: f32 = 8.0;
/// The small boost with nothing carried: seconds of it when full, and
/// seconds to refill.
const SMALL_BOOST_SECS: f32 = 1.2;
const SMALL_BOOST_REFILL: f32 = 8.0;
/// Seconds the BOOST item holds a kart at its boosted top speed.
const BOOST_SECS: f32 = 2.0;
/// The pickups: how big each is drawn, and how far over the road it turns.
const PICKUP_SIZE: f32 = 2.2;
/// The shield's bubble round a kart (avatar `BUBBLE_M`): the scale it is
/// drawn at, about 2.8 m across and 2.4 m high, and how far its pivot sits
/// under the kart's middle, so it is a dome over the kart and its driver.
const SHIELD_SIZE: f32 = 1.7;
const SHIELD_DROP: f32 = 0.95;
/// How far the gun reaches for a kart to bend onto (how far ahead a rival
/// fires it is its brain's), and how far along the road it takes the
/// road's rise or fall to aim along, metres.
const GUN_ASSIST: f32 = 90.0;
const GUN_AIM_AHEAD: f32 = 40.0;

/// Which way an item goes: the usual way (weapons ahead, goo and blocks
/// dropped behind), behind, or (goo and blocks) thrown ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Aim {
    Usual,
    Back,
    Ahead,
}

/// How far back the gun and rocket look for a kart to fire behind at,
/// metres.
const BACK_GUN: f32 = 45.0;
const BACK_ROCKET: f32 = 50.0;
/// Seconds a rival holds a weapon with nobody ahead before it fires back.
const REAR_PATIENCE: f32 = 3.0;

/// Kart `k`'s car.
pub(super) fn kart<'a>(bots: &'a mut [Bot], ride: &'a mut Ride, k: usize) -> Option<&'a mut Vehicle> {
    if k == 0 {
        ride.car_mut()
    } else {
        bots.get_mut(k - 1).map(|b| &mut b.car)
    }
}

/// Kart `k`'s name on the HUD.
pub(super) fn name(k: usize) -> &'static str {
    if k == 0 {
        "YOU"
    } else {
        NAMES[(k - 1) % NAMES.len()]
    }
}

impl Race {
    /// Where every kart is, for the weapons.
    pub(super) fn poses(&mut self, ride: &Ride) {
        let mut p = Vec::with_capacity(self.bots.len() + 1);
        p.push(ride.car().map_or(Pose::default(), |c| Pose { pos: c.pos(), fwd: c.forward(), on: self.me.ghost <= 0.0 && self.me.finish.is_none() }));
        for b in &self.bots {
            p.push(Pose { pos: b.car.pos(), fwd: b.car.forward(), on: !b.ghost && !b.home() && b.laps.ghost <= 0.0 });
        }
        self.arms.poses = p;
        let n = self.bots.len() + 1;
        self.harm.resize(n, Harm::default());
        self.slots.resize(n, Slot::Empty);
        self.reload.resize(n, 0.0);
        self.targets.resize(n, None);
        self.held_t.resize(n, 0.0);
        self.arms.aims.resize(n, None);
    }

    /// Distance round the track from kart `a` to kart `b`, metres: positive
    /// when `b` is ahead.
    fn gap(&self, a: usize, b: usize) -> f32 {
        self.laps_of(b).progress - self.laps_of(a).progress
    }

    /// Distance off the centreline of kart `k`, metres.
    fn lane(&self, k: usize) -> f32 {
        self.laps_of(k).place.map_or(0.0, |p| p.off)
    }

    /// How far round the lap kart `k` is, metres.
    fn lap_s(&self, k: usize) -> f32 {
        self.laps_of(k).place.map_or(0.0, |p| p.s)
    }

    /// The boxes, everyone's use of what they carry, and the
    /// guided shells' aims, once a step while racing.
    pub(super) fn fight(&mut self, ride: &mut Ride, player: &Player, dt: f32) {
        let n = self.bots.len() + 1;
        // The pickups: an item into an empty slot, a wrench onto a damaged
        // kart at once. One a kart has no use for stays where it is.
        let (slots, harm, all) = (&self.slots, &self.harm, self.arms.all);
        let wants = |k: usize, item: Item| match item {
            Item::Repair => harm.get(k).is_some_and(|h| h.damage > 0.0),
            _ => !all && slots.get(k) == Some(&Slot::Empty),
        };
        for (k, item) in self.arms.boxes_taken(dt, wants) {
            if item == Item::Repair {
                self.harm[k].hurt(-REPAIR);
                if k == 0 {
                    self.note = Some("REPAIRED".into());
                }
            } else {
                self.slots[k] = Slot::Have(item, item.uses());
            }
            if k == 0 {
                let pick = if item == Item::Repair { crate::sound::Pick::Health } else { crate::sound::Pick::Weapon };
                crate::sound::emit(crate::sound::Noise::ear(crate::sound::Kind::Pickup(pick), 0.6));
            }
        }
        for k in 0..n {
            if self.arms.all && !matches!(self.slots[k], Slot::Have(..)) {
                let item = if k == 0 { Item::WEAPONS[self.pick % Item::WEAPONS.len()] } else { self.arms.roll_weapon() };
                self.slots[k] = Slot::Have(item, item.uses());
            }
            self.reload[k] = (self.reload[k] - dt).max(0.0);
        }
        if self.arms.all {
            if let Slot::Have(i, _) = self.slots[0] {
                let want = Item::WEAPONS[self.pick % Item::WEAPONS.len()];
                if i != want {
                    self.slots[0] = Slot::Have(want, want.uses());
                }
            }
        }

        // The small boost on Shift: short and slow to come back, and only
        // with nothing carried (ride.rs `race_boost`).
        ride.race_boost = Some((SMALL_BOOST_SECS, SMALL_BOOST_REFILL, self.slots[0] == Slot::Empty && self.me.finish.is_none()));

        // The player: the button, pressed or held.
        let pressed = self.trigger && !self.trigger_was;
        self.trigger_was = self.trigger;
        if self.me.finish.is_none() && self.harm[0].spin <= 0.0 {
            if let Slot::Have(item, _) = self.slots[0] {
                let go = if item.held() { self.trigger && self.reload[0] <= 0.0 } else { pressed };
                if go {
                    // The stick or S held back, or looking behind: behind.
                    // The stick held forward: goo or a block thrown ahead.
                    let aim = if self.stick_back || self.look_back {
                        Aim::Back
                    } else if self.stick_ahead {
                        Aim::Ahead
                    } else {
                        Aim::Usual
                    };
                    let look = if aim == Aim::Back { -self.arms.poses[0].fwd } else { player.forward() };
                    self.use_item(0, item, Some(look), aim, ride);
                }
            }
        }
        // The rivals, by what is round them, and the autopilot as a rival.
        // With KART_ARMS=all a weapon held a while unused is swapped, so
        // every weapon gets tried.
        for k in 0..n {
            if k == 0 && self.autopilot.is_none() {
                continue;
            }
            if self.arms.all && self.held_t[k] > 6.0 {
                let item = self.arms.roll_weapon();
                self.slots[k] = Slot::Have(item, item.uses());
                self.held_t[k] = 0.0;
            }
            let out = self.laps_of(k).finish.is_some() || (k > 0 && self.bots[k - 1].ghost);
            if out || self.harm[k].spin > 0.0 {
                continue;
            }
            let Slot::Have(item, _) = self.slots[k] else {
                self.held_t[k] = 0.0;
                continue;
            };
            self.held_t[k] += dt;
            if self.reload[k] <= 0.0 {
                if let Some(aim) = self.bot_wants(k, item) {
                    self.use_item(k, item, None, aim, ride);
                }
            }
        }

        // Each guided shell steers after its kart's target, or the player's
        // look.
        for k in 0..n {
            let pose = self.arms.poses[k];
            let eye = pose.pos + Vec3::Y * 0.6;
            self.arms.aims[k] = match self.targets[k] {
                Some(t) if self.arms.poses.get(t).is_some_and(|p| p.on) => {
                    let to = self.arms.poses[t].pos + Vec3::Y * 0.3 - eye;
                    Some((eye, to.normalize_or(pose.fwd)))
                }
                _ if k == 0 => Some((eye, player.forward())),
                _ => Some((eye, pose.fwd)),
            };
        }
    }

    /// Whether rival `k` uses `item` now, and which way: the gun at a kart
    /// close ahead in line, a rocket or disc at a kart ahead, goo or a block
    /// for one close behind, a boost on a straight, a shield after holding
    /// it a while, overdrive at once. Its brain may also fire back at a kart
    /// close behind, throw goo or a block ahead onto a kart's line, put the
    /// shield up when a kart behind is armed, and keep overdrive for a
    /// straight.
    fn bot_wants(&self, k: usize, item: Item) -> Option<Aim> {
        let n = self.bots.len() + 1;
        let me = self.arms.poses[k];
        let brain = self.brains.get(k).copied().unwrap_or_default();
        let others = || (0..n).filter(move |&j| j != k && self.arms.poses[j].on);
        let usual = |yes: bool| yes.then_some(Aim::Usual);
        // Forward fire first; back only once the item has been held a while
        // with nobody to shoot ahead, so karts in front take most of it.
        let patient = self.held_t[k] > REAR_PATIENCE;
        let or_back = |ahead: bool, back: bool| {
            if ahead {
                Some(Aim::Usual)
            } else if brain.rear_fire && patient && back {
                Some(Aim::Back)
            } else {
                None
            }
        };
        let s = self.lap_s(k);
        let straight = || (0..8).all(|i| self.track.curve_at(s + i as f32 * 6.0).abs() < 1.0 / 70.0);
        match item {
            Item::Gun => or_back(
                assist(&self.arms.poses, k, me.pos, me.fwd, brain.gun_cone.to_radians(), brain.gun_range).is_some_and(|(t, _)| self.gap(k, t) > 0.0),
                brain.rear_fire && assist(&self.arms.poses, k, me.pos, -me.fwd, 8f32.to_radians(), BACK_GUN).is_some(),
            ),
            Item::Rocket => or_back(
                assist(&self.arms.poses, k, me.pos, me.fwd, brain.rocket_cone.to_radians(), brain.rocket_range).is_some_and(|(t, _)| self.gap(k, t) > 0.0),
                brain.rear_fire && assist(&self.arms.poses, k, me.pos, -me.fwd, 25f32.to_radians(), BACK_ROCKET).is_some(),
            ),
            Item::Disc => or_back(others().any(|j| (5.0..40.0).contains(&self.gap(k, j))), others().any(|j| (-30.0..-5.0).contains(&self.gap(k, j)))),
            Item::Goo | Item::Block => {
                let in_line = |j: usize| (self.lane(j) - self.lane(k)).abs() < 2.5;
                if others().any(|j| (-15.0..-2.0).contains(&self.gap(k, j)) && in_line(j)) {
                    Some(Aim::Usual)
                } else if brain.throw_ahead && others().any(|j| (8.0..30.0).contains(&self.gap(k, j)) && in_line(j)) && !others().any(|j| (-25.0..0.0).contains(&self.gap(k, j))) {
                    Some(Aim::Ahead)
                } else {
                    None
                }
            }
            Item::Boost => usual(straight()),
            Item::Shield => {
                let armed = |j: usize| matches!(self.slots.get(j), Some(Slot::Have(Item::Gun | Item::Rocket | Item::Disc, _)));
                let threat = brain.shield_react && others().any(|j| (-40.0..-3.0).contains(&self.gap(k, j)) && armed(j));
                usual(self.held_t[k] > brain.shield_hold || threat)
            }
            Item::Overdrive => usual(!brain.over_straight || straight()),
            Item::Repair => Some(Aim::Usual),
        }
    }

    /// Kart `k` uses `item`; `look` is where the player is looking (None for
    /// a rival). Weapons are aimed for a kart, not for a crosshair: along
    /// the nose or the road unless the player looks well away, and onto a
    /// kart near the line.
    fn use_item(&mut self, k: usize, item: Item, look: Option<Vec3>, aim: Aim, ride: &mut Ride) {
        let pose = self.arms.poses[k];
        let nose = Vec3::new(pose.fwd.x, 0.0, pose.fwd.z).normalize_or(pose.fwd);
        let flat_look = look.map(|l| Vec3::new(l.x, 0.0, l.z).normalize_or(nose));
        // Looking well away from the nose (the mouse in the chase camera):
        // that way, and looking back fires back.
        let away = flat_look.filter(|l| l.dot(nose) < 20f32.to_radians().cos());
        let back = aim == Aim::Back || flat_look.is_some_and(|l| l.dot(nose) < -0.3);
        let up = Vec3::Y;
        match item {
            Item::Gun => {
                // Behind: out of the back of the kart, along the road behind.
                let (way, reach) = if back { (-nose, -GUN_AIM_AHEAD) } else { (nose, GUN_AIM_AHEAD) };
                let from = pose.pos + way * 1.1 * KART_SIZE + up * 0.5;
                // Along the road's own rise and fall ahead, not the kart's
                // pitch, which on a crest or in a dip puts the rounds into
                // the road a few metres off.
                let s = self.lap_s(k);
                let along = match (self.track.height_at(s), self.track.height_at(s + reach)) {
                    (Some(a), Some(b)) => (way + Vec3::Y * ((b - a) / GUN_AIM_AHEAD)).normalize_or(way),
                    _ => way,
                };
                let dir = assist(&self.arms.poses, k, from, along, 8f32.to_radians(), GUN_ASSIST).map_or(along, |(_, w)| w);
                self.arms.shoot(k, item, from, from, dir);
                self.reload[k] = self.arms.gap(item);
            }
            Item::Rocket => {
                let way = if aim == Aim::Back { -nose } else { away.unwrap_or(nose) };
                let from = pose.pos + up * 0.6;
                let lock = assist(&self.arms.poses, k, from, way, if k == 0 { 6f32.to_radians() } else { 25f32.to_radians() }, 90.0);
                self.targets[k] = lock.map(|(t, _)| t);
                let dir = lock.map_or(way, |(_, w)| w);
                self.arms.shoot(k, item, from, from + dir * 1.4, dir);
            }
            Item::Disc => {
                let (_, d) = self.track.at(self.lap_s(k));
                let road = Vec3::new(d.x, 0.0, d.y) * if back { -1.0 } else { 1.0 };
                let dir = away.filter(|_| !back).unwrap_or(road);
                let from = pose.pos + up * 0.5;
                self.arms.shoot(k, item, from, from + dir * 1.5, dir);
            }
            Item::Goo | Item::Block if aim == Aim::Ahead => {
                // Lobbed high over the nose and well ahead of the kart's own
                // speed, so it lands on the road 15 to 30 m in front.
                let vel = kart(&mut self.bots, ride, k).map_or(Vec3::ZERO, |c| Vec3::new(c.b.vel.x, 0.0, c.b.vel.z));
                let from = pose.pos + nose * 1.9 * KART_SIZE + up * 0.9;
                let dir = (nose * 0.6 + up * 0.8).normalize();
                self.arms.throw(k, item, from, from, dir, vel + nose * 16.0 + up * 5.0);
                self.reload[k] = 0.4;
            }
            Item::Goo | Item::Block => {
                let from = pose.pos - nose * 1.7 * KART_SIZE + up * 0.6;
                let dir = (-nose * 0.6 - up * 0.4).normalize();
                self.arms.shoot(k, item, from, from, dir);
                self.reload[k] = 0.4;
            }
            Item::Boost => {
                if let Some(car) = kart(&mut self.bots, ride, k) {
                    boost_kick(car, BOOST_SECS);
                }
            }
            Item::Shield => {
                self.harm[k].shield = SHIELD_SECS;
                crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Pickup(crate::sound::Pick::Armour), pose.pos, 0.8));
                if k == 0 {
                    self.note = Some("SHIELD UP".into());
                }
            }
            Item::Overdrive => self.harm[k].over = OVER_SECS,
            Item::Repair => self.harm[k].hurt(-REPAIR),
        }
        if let Slot::Have(i, left) = self.slots[k] {
            self.slots[k] = if left > 1 { Slot::Have(i, left - 1) } else { Slot::Empty };
        }
        self.held_t[k] = 0.0;
    }

    /// What the hits since last step did, and what being hit, slowed or in
    /// overdrive does to each kart this step.
    pub(super) fn harmed(&mut self, ride: &mut Ride, dt: f32) {
        let n = self.bots.len() + 1;
        let hits = std::mem::take(&mut self.arms.hits);
        for h in hits {
            if h.who >= n {
                continue;
            }
            let (spun, slow, knock) = if self.arms.is_disc(&h) { self.harm[h.who].graze() } else { self.harm[h.who].take(&h) };
            if let Some(car) = kart(&mut self.bots, ride, h.who) {
                if spun {
                    let v = car.b.vel;
                    car.b.vel = Vec3::new(v.x * SPIN_KEEP, v.y, v.z * SPIN_KEEP);
                    car.drift = None;
                    car.kick_t = 0.0;
                }
                if slow > 0.0 {
                    let flat = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
                    let v = flat.length();
                    if v > 0.1 {
                        car.b.vel -= flat / v * slow.min(v);
                    }
                }
                car.b.vel += knock;
            }
            if spun {
                crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Clank, self.arms.poses[h.who].pos, 0.9));
                if h.who == 0 {
                    self.note = Some(format!("HIT BY {}", name(h.by)));
                    ride.jolt(10.0);
                } else if h.by == 0 {
                    self.note = Some(format!("HIT {}", name(h.who)));
                }
            }
        }
        for k in 0..n {
            // Running over a block: it breaks, and the kart rattles over it,
            // thrown up a little and 5% slower.
            let pose = self.arms.poses[k];
            if pose.on && self.arms.run_into_block(pose.pos, pose.fwd) && self.harm[k].over <= 0.0 && self.harm[k].shield <= 0.0 {
                self.harm[k].hurt(BLOCK_DAMAGE);
                if let Some(car) = kart(&mut self.bots, ride, k) {
                    let v = car.b.vel;
                    car.b.vel = Vec3::new(v.x * BLOCK_KEEP, v.y.max(0.0) + BLOCK_HOP, v.z * BLOCK_KEEP);
                }
                crate::sound::emit(crate::sound::Noise::at(crate::sound::Kind::Clank, pose.pos, 0.7));
                if k == 0 {
                    self.note = Some("BLOCK".into());
                    ride.jolt(6.0);
                    ride.rumble = ride.rumble.max(0.4);
                }
            }
            let h = self.harm[k];
            let Some(car) = kart(&mut self.bots, ride, k) else { continue };
            if h.spin > 0.0 {
                // A full turn over the spin, about the kart's own up.
                let turn = std::f32::consts::TAU / SPIN_SECS * dt;
                car.b.rot = (Quat::from_rotation_y(turn) * car.b.rot).normalize();
            }
            let top = car.spec.top_speed;
            let flat = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
            let v = flat.length();
            if h.over > 0.0 && v < top * OVER_TOP && car.grounded > 0 {
                let fwd = car.forward();
                car.b.vel += Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero() * OVER_PUSH * dt;
            }
        }
        for h in &mut self.harm {
            h.cool(dt);
        }
        ride.flash = self.harm.first().is_some_and(|h| h.flashing());
        // The player's controls while spun; none at all once home
        // (`ride.hold`).
        if self.me.finish.is_some() {
            ride.auto = None;
        } else if self.harm.first().is_some_and(|h| h.spin > 0.0) {
            ride.auto = Some(Controls::default());
        } else if self.autopilot.is_none() {
            ride.auto = None;
        }
    }
}

impl Race {
    /// What the weapons did this tick, for `Game`'s sounds and effects.
    pub fn combat_cues(&mut self) -> Vec<crate::combat::Cue> {
        self.arms.combat.take_cues()
    }

    /// The bursts lighting the road for a moment, for `Game`'s lamps.
    pub fn flashes(&self) -> Vec<(Vec3, f32, [f32; 3], f32)> {
        self.arms.combat.flashes().collect()
    }

    /// The shells in the air, drawn as the shooter draws them, and the item
    /// boxes, turning and bobbing in a colour that cycles through the items'.
    pub(super) fn arms_instances(&self, player: &Player, alpha: f32, weapons: usize, models: &[crate::models::Model], out: &mut Vec<(usize, crate::models::Instance)>) {
        use crate::models::Instance;
        let eye = player.eye(alpha);
        for s in &self.arms.combat.shells {
            let Some(md) = models.get(weapons + s.look) else { continue };
            if (s.pos - eye).length() < 0.22 {
                continue;
            }
            let d = s.vel.normalize_or(Vec3::NEG_Z);
            let yaw = (-d.x).atan2(-d.z);
            let rot = Quat::from_rotation_x(d.y.asin()) * Quat::from_rotation_z(s.spin);
            out.push((
                weapons + s.look,
                Instance {
                    pos_yaw: [s.pos.x, s.pos.y, s.pos.z, yaw],
                    info: [md.voxel, md.pivot.x, md.pivot.y, md.pivot.z],
                    tint: [1.0, 1.0, 1.0, 0.0],
                    rot: [rot.x, rot.y, rot.z, rot.w],
                    shift: [0.0; 4],
                    flesh: [[0.0; 4]; 3],
                    glow: [0.0; 4],
                },
            ));
        }
        // The pickups: each the shooter's ammunition for its weapon, turning
        // and bobbing over the road, see-through; the shield the shooter's
        // breastplate, a wrench its health cross, a boost or overdrive the
        // lit blob in its colour.
        use crate::combat::viewmodel::{AMMO, AMMOS, COUNT};
        use crate::shooter::hold;
        let shells = weapons + COUNT;
        let ammo = |name: &str| AMMOS.iter().position(|a| *a == name).map(|i| weapons + AMMO + i);
        for (i, b) in self.arms.boxes.iter().enumerate() {
            if b.back > 0.0 {
                continue;
            }
            let t = self.clock + i as f32 * 0.37;
            let at = b.at + Vec3::Y * (t * 2.2).sin() * 0.12 - Vec3::Y * 0.4;
            let spin = t * 1.8;
            let c = b.item.colour();
            let tint = [c[0], c[1], c[2]];
            let (model, size, glow) = match b.item {
                Item::Gun => (ammo("bullets"), PICKUP_SIZE, false),
                Item::Rocket => (ammo("rockets"), PICKUP_SIZE, false),
                Item::Goo => (ammo("goo"), PICKUP_SIZE, false),
                Item::Block => (ammo("bore"), PICKUP_SIZE, false),
                Item::Disc => (ammo("discs"), PICKUP_SIZE, false),
                Item::Shield => (Some(shells + crate::brain::avatar::ARMOUR_M), 0.9, true),
                Item::Repair => (Some(shells + crate::brain::avatar::CROSS_M), 0.9, true),
                Item::Boost | Item::Overdrive => (Some(shells + crate::brain::avatar::PAD_M), if b.item == Item::Overdrive { 1.6 } else { 1.2 }, true),
            };
            let Some(model) = model else { continue };
            let tint = if glow { tint } else { [1.0; 3] };
            if let Some((m, inst)) = hold::sized(model, models, at, spin, Quat::IDENTITY, tint, size) {
                out.push((m, inst.glass(0.9)));
            }
            // A ring of the item's colour under it, so a row reads from afar.
            if !glow {
                if let Some((m, inst)) = hold::sized(shells + crate::brain::avatar::PAD_M, models, at - Vec3::Y * 0.1, spin, Quat::IDENTITY, tint, 0.8) {
                    out.push((m, inst.glass(0.35)));
                }
            }
        }
        // The shield: a dome of blue light over the kart, breathing, flaring
        // when it turns a hit, and flickering as its last two seconds run
        // out.
        for (k, h) in self.harm.iter().enumerate() {
            if h.shield <= 0.0 {
                continue;
            }
            let Some(p) = self.arms.poses.get(k).filter(|p| p.on) else { continue };
            if h.shield < 2.0 && (self.clock * 10.0).fract() < 0.4 {
                continue;
            }
            let flare = (h.turned / TURNED_SECS).clamp(0.0, 1.0);
            let pulse = 1.0 + (self.clock * 5.0).sin() * 0.03 + flare * 0.08;
            let tint = [0.15 + flare * 0.6, 0.4 + flare * 0.45, 1.0];
            let at = p.pos + Vec3::Y * (0.35 - SHIELD_DROP * pulse);
            if let Some((m, inst)) = hold::sized(shells + crate::brain::avatar::BUBBLE_M, models, at, self.clock * 0.6, Quat::IDENTITY, tint, SHIELD_SIZE * pulse) {
                out.push((m, inst.glass(0.22 + flare * 0.4)));
            }
        }
    }
}
