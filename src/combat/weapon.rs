//! A gun in hand: which one is out, how many rounds are left, and when the
//! next one may go. The definition it works from is `library::WeaponDef`;
//! what happens to whoever it hits is `super::shot`.

use crate::library::{FireDef, FireKind, WeaponDef};
use std::sync::Arc;

/// Which button is down. `None` is a finger off the trigger, which is what
/// lets the minigun's barrels run down again.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    #[default]
    Off,
    Fire,
    Alt,
}

/// What one step on the trigger produced.
#[derive(Default, Clone, Copy, Debug)]
pub struct Pulled {
    /// Rounds that left the gun this step.
    pub shots: u32,
    /// What those rounds are worth, 1 at a tap and `charge_mult` at a full
    /// charge. 1 for everything that does not charge.
    pub strength: f32,
    /// A beam is live this step: no rounds, a line held on the target and
    /// damage by the second.
    pub beam: bool,
    /// The spade took a bite this step: whoever is holding the gun should
    /// cut the wall in front of it (`WeaponDef::dig`). Off every weapon but
    /// the Digger, and off that one unless the left button is down.
    pub dig: bool,
    /// Which button a charged shot was built on, when it leaves after that
    /// button has come up. Look the mode up by this (`Held::fired_mode`),
    /// not by the button down now, or a glob let go on release is spent
    /// and never thrown.
    pub from: Option<Trigger>,
}

impl Pulled {
    /// Nothing happened.
    fn none() -> Self {
        Pulled {
            shots: 0,
            strength: 1.0,
            beam: false,
            dig: false,
            from: None,
        }
    }
}

/// The lowest share of the full rate a minigun fires at the moment the
/// trigger goes down.
const SPIN_FLOOR: f32 = 0.3;
/// How fast the barrels run down again once the trigger is let go, as a
/// multiple of how fast they spun up.
const SPIN_DOWN: f32 = 1.6;

/// One weapon as carried: the definition, the rounds in it, and the state the
/// trigger has left behind.
pub struct Held {
    pub def: Arc<WeaponDef>,
    /// Rounds in the player's store for this weapon, the ones in the gun
    /// included.
    pub ammo: u32,
    /// Rounds in the gun. A weapon with no `mag` is fed straight from the
    /// store and this is not read.
    pub loaded: u32,
    /// Seconds until the next round may go.
    cool: f32,
    /// Seconds left of a reload, 0 when it is not reloading.
    pub reload: f32,
    /// 0 at rest, 1 at the full rate (the minigun's barrels).
    pub spin: f32,
    /// 0 to 1 of a charging mode's `charge_s` built up so far. A charge is
    /// held until the trigger is let go, so this is the only state in here
    /// that survives the finger coming off the button.
    pub charge: f32,
    /// Which trigger the charge belongs to: letting go of the other button
    /// must not fire it.
    charging: Trigger,
    /// Rounds owed by a beam, which spends ammunition by the second rather
    /// than a round at a time.
    burn: f32,
    /// Seconds until the spade may take another bite (`WeaponDef::dig`).
    spade: f32,
    /// Rounds fired so far of the burst under way (`FireDef::burst`), 0
    /// between bursts, and which button started it. A burst under way
    /// finishes on that button's mode even after the finger comes up.
    volley: u32,
    volley_t: Trigger,
    /// Extra spread, degrees, for the rounds that left this step: how far
    /// into its burst the gun was (`FireDef::burst_spread_deg`).
    bloom: f32,
    /// Whether this body is carrying the gun at all.
    ///
    /// The arsenal holds every weapon the world has, because a slot has to
    /// be a slot whether or not you have found what goes in it, and because
    /// a gun you pick up has to remember the rounds you had for it. A gun
    /// you are not carrying is skipped by everything that chooses one, and
    /// a pad hands it over (`shooter::pickup`).
    pub have: bool,
    /// This gun's rounds are a store it shares with other guns
    /// (`WeaponDef::ammo_type`), and what that store held when the arsenal
    /// last squared them up (`Arsenal::share`).
    pub pooled: bool,
    pool_last: u32,
}

impl Held {
    pub fn new(def: Arc<WeaponDef>, full: bool) -> Self {
        let ammo = if full { def.ammo_max } else { def.ammo_start.min(def.ammo_max) };
        let loaded = def.mag.min(ammo);
        Held {
            def,
            ammo,
            loaded,
            cool: 0.0,
            reload: 0.0,
            spin: 0.0,
            charge: 0.0,
            charging: Trigger::Off,
            burn: 0.0,
            spade: 0.0,
            volley: 0,
            volley_t: Trigger::Off,
            bloom: 0.0,
            have: true,
            pooled: false,
            pool_last: 0,
        }
    }

    /// Pick it up: the gun, and the rounds that came with it. True if this
    /// was the first time, which is what the note on screen says.
    pub fn found(&mut self, rounds: u32) -> bool {
        let first = !self.have;
        self.have = true;
        self.give(rounds.max(self.def.ammo_start.min(self.def.ammo_max)));
        first
    }

    /// Rounds the gun can fire before it has to be reloaded. A weapon with
    /// no magazine answers with its whole store.
    pub fn in_gun(&self) -> u32 {
        if self.def.mag > 0 {
            self.loaded
        } else {
            self.ammo
        }
    }

    /// A weapon with nothing in it to run out of: the Digger, whose
    /// `ammo_max` is 0. Every ammunition rule skips it.
    pub fn unlimited(&self) -> bool {
        self.def.ammo_max == 0
    }

    /// True while it cannot be fired because it is being reloaded.
    /// Extra spread in degrees for what this gun fired this step. The match
    /// adds it to the mode's own before the rounds are thrown.
    pub fn bloom(&self) -> f32 {
        self.bloom
    }

    pub fn reloading(&self) -> bool {
        self.reload > 0.0
    }

    /// How far through the reload it is, 0 to 1, for the HUD's bar.
    pub fn reload_frac(&self) -> f32 {
        if self.def.reload_s > 0.0 {
            1.0 - (self.reload / self.def.reload_s).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// Start a reload. False if there is no point: no magazine, already
    /// reloading, already full, or nothing left in the store to put in.
    pub fn start_reload(&mut self) -> bool {
        let mag = self.def.mag;
        if mag == 0 || self.reloading() || self.loaded >= mag.min(self.ammo) {
            return false;
        }
        self.reload = self.def.reload_s.max(0.05);
        true
    }

    /// Run the reload clock down and fill the gun when it reaches the end.
    fn tick_reload(&mut self, dt: f32) {
        if !self.reloading() {
            return;
        }
        self.reload -= dt;
        if self.reload <= 0.0 {
            self.reload = 0.0;
            self.loaded = self.def.mag.min(self.ammo);
            self.cool = 0.0;
        }
    }

    /// The mode `shot` was fired in: the button it was built on for a
    /// charge let go on release, otherwise the button `t` down now.
    pub fn fired_mode(&self, shot: &Pulled, t: Trigger) -> Option<&FireDef> {
        self.mode(shot.from.unwrap_or(t))
    }

    pub fn mode(&self, t: Trigger) -> Option<&FireDef> {
        match t {
            Trigger::Off => None,
            Trigger::Fire => Some(&self.def.fire),
            // A gun whose right button is a sight has no second mode: the
            // zoom is the whole of what that button does, and a sniper who
            // squeezes it must not loose a round into the floor.
            Trigger::Alt if self.def.zoom > 0.0 && self.def.alt.is_none() => None,
            Trigger::Alt => self.def.alt.as_ref().or(Some(&self.def.fire)),
        }
    }

    /// Rounds a shot of `f` costs, which is nothing at all on a weapon that
    /// needs no ammunition.
    fn cost(&self, f: &FireDef) -> u32 {
        if self.unlimited() {
            0
        } else {
            f.per_shot
        }
    }

    /// What the trigger produces this step, spending the ammunition for it:
    /// rounds, a beam, or a charge still being built.
    ///
    /// Three kinds of mode go through here. An ordinary one fires on the
    /// `rpm` clock, at most a handful of rounds a step so a long frame
    /// cannot empty a magazine at once. A charging one holds its shot back
    /// while the trigger is down and lets it go, worth up to `charge_mult`,
    /// when the trigger is released or the charge fills. A beam fires
    /// nothing: it reports itself live and burns rounds by the second.
    pub fn pull(&mut self, t: Trigger, dt: f32) -> Pulled {
        let dig = self.bite(t, dt);
        let mut out = self.trigger(t, dt);
        out.dig = dig;
        out
    }

    /// The spade's own clock, which runs beside the fire mode's rather than
    /// off it: a gun with a `dig` bites while the left button is held, at
    /// its own `rpm`, whatever the gun it is bolted to is doing. One bite a
    /// step at most, so a long frame does not hollow out a room.
    fn bite(&mut self, t: Trigger, dt: f32) -> bool {
        let Some(d) = self.def.dig.filter(|_| t == Trigger::Fire && !self.reloading()) else {
            self.spade = 0.0;
            return false;
        };
        self.spade -= dt;
        if self.spade > 0.0 {
            return false;
        }
        self.spade = 60.0 / d.rpm.max(1.0);
        true
    }

    /// What the trigger alone produces: rounds, a beam, or a charge.
    fn trigger(&mut self, t: Trigger, dt: f32) -> Pulled {
        self.tick_reload(dt);
        if self.reloading() {
            self.volley = 0;
        }
        // A burst under way finishes on the button that started it, whether
        // that button is still down or not. Pressing the other one mid-burst
        // gives up the rest of it.
        if self.volley > 0 && t != self.volley_t {
            match (t, self.mode(self.volley_t).cloned()) {
                (Trigger::Off, Some(f)) => {
                    return Pulled {
                        shots: self.cadence(&f, dt),
                        strength: 1.0,
                        beam: false,
                        dig: false,
                        from: None,
                    };
                }
                _ => self.volley = 0,
            }
        }
        let Some(f) = self.mode(t).cloned() else {
            self.run_down(dt);
            self.cool = (self.cool - dt).max(0.0);
            // The finger is off the button: a charge that was being built
            // goes now, which is how a charging mode is fired at all.
            return self.release(t);
        };
        if self.reloading() {
            self.run_down(dt);
            return Pulled::none();
        }
        if f.kind == FireKind::Beam {
            self.spin = 1.0;
            return self.hold_beam(&f, dt);
        }
        if f.charge_s > 0.0 {
            return self.build_charge(t, &f, dt);
        }
        if f.burst > 1 {
            self.volley_t = t;
        }
        let shots = self.cadence(&f, dt);
        Pulled {
            shots,
            strength: 1.0,
            beam: false,
            dig: false,
            from: None,
        }
    }

    /// Rounds off the `rpm` clock, ammunition spent for each.
    fn cadence(&mut self, f: &FireDef, dt: f32) -> u32 {
        // Spin-up: the rate climbs while the trigger is held.
        let rate = if f.spin_up > 0.0 {
            self.spin = (self.spin + dt / f.spin_up).min(1.0);
            f.rpm / 60.0 * (SPIN_FLOOR + (1.0 - SPIN_FLOOR) * self.spin)
        } else {
            self.spin = 1.0;
            f.rpm / 60.0
        };
        let gap = 1.0 / rate.max(0.01);
        self.cool -= dt;
        let mut shots = 0;
        // The round this step's first shot is in its burst, for the spread.
        self.bloom = f.burst_spread_deg * self.volley as f32;
        while self.cool <= 0.0 && shots < 4 {
            if !self.spend(f) {
                self.cool = 0.0;
                break;
            }
            self.cool += gap;
            shots += 1;
            if f.burst > 1 {
                self.volley += 1;
                if self.volley >= f.burst {
                    self.volley = 0;
                    self.cool += f.burst_gap_s;
                }
            }
        }
        self.cool = self.cool.max(0.0);
        // A gun that has run its magazine dry reloads itself, which is what
        // a player expects of anything with a magazine at all.
        if self.def.mag > 0 && self.loaded < self.cost(f) {
            self.start_reload();
        }
        shots
    }

    /// Take the rounds one shot costs; false if they are not there.
    fn spend(&mut self, f: &FireDef) -> bool {
        let cost = self.cost(f);
        if self.ammo < cost || self.in_gun() < cost {
            return false;
        }
        self.ammo -= cost;
        if self.def.mag > 0 {
            self.loaded -= cost.min(self.loaded);
        }
        true
    }

    /// A beam held on a target: no rounds leave, and the store runs down at
    /// `rpm` rounds a minute while it is live.
    fn hold_beam(&mut self, f: &FireDef, dt: f32) -> Pulled {
        if self.unlimited() {
            return Pulled {
                shots: 0,
                strength: 1.0,
                beam: true,
                dig: false,
                from: None,
            };
        }
        self.burn += dt * f.rpm / 60.0 * f.per_shot as f32;
        while self.burn >= 1.0 {
            if self.ammo == 0 || self.in_gun() == 0 {
                self.burn = 0.0;
                break;
            }
            self.burn -= 1.0;
            self.ammo -= 1;
            if self.def.mag > 0 {
                self.loaded -= 1;
            }
        }
        if self.in_gun() == 0 {
            if self.def.mag > 0 {
                self.start_reload();
            }
            return Pulled::none();
        }
        Pulled {
            shots: 0,
            strength: 1.0,
            beam: true,
            dig: false,
            from: None,
        }
    }

    /// Build a charge while the button is down, and fire it the moment it is
    /// full. The rest of it goes off in `release`.
    fn build_charge(&mut self, t: Trigger, f: &FireDef, dt: f32) -> Pulled {
        // Nothing to charge with: a gun with an empty magazine should be
        // reloading, not winding up a shot it cannot fire.
        if !self.unlimited() && self.in_gun() < self.cost(f) {
            if self.def.mag > 0 {
                self.start_reload();
            }
            return Pulled::none();
        }
        self.charging = t;
        self.cool = (self.cool - dt).max(0.0);
        // Full, it waits: the shot is kept loaded for as long as the button
        // is held and goes when it comes up (`release`).
        self.charge = (self.charge + dt / f.charge_s).min(1.0);
        Pulled::none()
    }

    /// The button to pull for a player holding `fire` (left) and `alt`
    /// (right). On a gun whose right button is a sight and a charge, the
    /// charge goes only on the left button: letting the sight down without
    /// it throws the charge away rather than loosing a shot nobody aimed.
    /// Bots press their buttons one at a time and keep the plain rule, that
    /// a charge goes when its button comes up.
    pub fn hands(&mut self, fire: bool, alt: bool) -> Trigger {
        let sight = self.def.zoom > 0.0 && self.def.alt.as_ref().is_some_and(|a| a.charge_s > 0.0);
        let charged = self.charge > 0.0 && self.charging == Trigger::Alt;
        if sight && charged {
            match (fire, alt) {
                // Off lets the charge go (`release`).
                (true, true) => return Trigger::Off,
                (_, false) => {
                    self.charge = 0.0;
                    self.charging = Trigger::Off;
                }
                _ => {}
            }
        }
        match (fire, alt) {
            (true, _) => Trigger::Fire,
            (_, true) => Trigger::Alt,
            _ => Trigger::Off,
        }
    }

    /// Fire the charge that was being built, if there is one.
    fn release(&mut self, t: Trigger) -> Pulled {
        if self.charge <= 0.0 {
            return Pulled::none();
        }
        // Still holding the other button: the charge waits rather than
        // being thrown away.
        if t != Trigger::Off && t != self.charging {
            return Pulled::none();
        }
        let Some(f) = self.mode(self.charging).cloned() else {
            self.charge = 0.0;
            return Pulled::none();
        };
        if self.reloading() {
            return Pulled::none();
        }
        self.loose(&f)
    }

    /// Let a charged shot go: one round, worth 1 at a tap and `charge_mult`
    /// at a full charge.
    fn loose(&mut self, f: &FireDef) -> Pulled {
        let k = self.charge.clamp(0.0, 1.0);
        let from = self.charging;
        self.charge = 0.0;
        self.charging = Trigger::Off;
        if !self.spend(f) {
            return Pulled::none();
        }
        self.cool = 60.0 / f.rpm.max(1.0);
        if self.def.mag > 0 && self.loaded < self.cost(f) {
            self.start_reload();
        }
        Pulled {
            shots: 1,
            strength: 1.0 + (f.charge_mult - 1.0) * k,
            beam: false,
            dig: false,
            from: Some(from),
        }
    }

    fn run_down(&mut self, dt: f32) {
        let up = self.def.fire.spin_up.max(self.def.alt.as_ref().map_or(0.0, |a| a.spin_up));
        if up > 0.0 {
            self.spin = (self.spin - dt * SPIN_DOWN / up).max(0.0);
        } else {
            self.spin = 0.0;
        }
    }

    #[allow(dead_code)] // for ammunition pickups, which are not built yet
    pub fn give(&mut self, rounds: u32) {
        // A shared store is capped on what is outside the magazines, since
        // each gun of the kind has a magazine of its own to fill.
        let cap = if self.pooled { self.def.ammo_max + (self.ammo - self.spare()) } else { self.def.ammo_max };
        self.ammo = (self.ammo + rounds).min(cap.max(self.ammo));
        if self.def.mag > 0 && self.loaded == 0 {
            self.start_reload();
        }
    }

    pub fn empty(&self) -> bool {
        !self.have || (!self.unlimited() && self.ammo < self.def.fire.per_shot)
    }

    /// Is there room for more rounds? A pad over a full gun stays standing.
    pub fn wants_ammo(&self) -> bool {
        let store = if self.pooled { self.spare() } else { self.ammo };
        self.have && !self.unlimited() && store < self.def.ammo_max
    }

    /// Rounds in the store and not in the gun: for a gun with no magazine,
    /// all of them.
    pub fn spare(&self) -> u32 {
        if self.def.mag > 0 {
            self.ammo.saturating_sub(self.loaded)
        } else {
            self.ammo
        }
    }

    /// Which store its rounds come out of.
    fn kind(&self) -> &str {
        self.def.ammo_type.as_deref().unwrap_or(&self.def.name)
    }
}

/// The guns carried, in slot order, and which one is out.
pub struct Arsenal {
    pub guns: Vec<Held>,
    pub held: usize,
}

impl Arsenal {
    /// The named weapons out of the library, in their own slot order. Names
    /// the library does not know are reported and skipped.
    pub fn new(lib: &crate::library::Library, names: &[String], full: bool, mut unknown: impl FnMut(&str)) -> Arsenal {
        let mut guns: Vec<Held> = Vec::new();
        for n in names {
            match lib.weapon(n) {
                Some(i) => guns.push(Held::new(Arc::new(lib.weapons[i].clone()), full)),
                None => unknown(n),
            }
        }
        guns.sort_by_key(|g| g.def.slot);
        for i in 0..guns.len() {
            guns[i].pooled = (0..guns.len()).any(|j| j != i && guns[j].kind() == guns[i].kind());
        }
        let mut a = Arsenal { guns, held: 0 };
        a.settle();
        a
    }

    pub fn current(&self) -> Option<&Held> {
        self.guns.get(self.held).filter(|g| g.have)
    }

    pub fn current_mut(&mut self) -> Option<&mut Held> {
        self.guns.get_mut(self.held).filter(|g| g.have)
    }

    /// Take out the weapon on digit `slot`; false if nothing is on it, or
    /// nothing you are carrying.
    pub fn pick(&mut self, slot: u32) -> bool {
        match self.guns.iter().position(|g| g.def.slot == slot && g.have) {
            Some(i) => {
                self.held = i;
                true
            }
            None => false,
        }
    }

    /// The next or previous weapon round the ring (the mouse wheel), past
    /// the slots that are empty-handed.
    pub fn cycle(&mut self, by: i32) {
        if self.guns.is_empty() || !self.guns.iter().any(|g| g.have) {
            return;
        }
        let n = self.guns.len() as i32;
        let step = if by >= 0 { 1 } else { -1 };
        let mut at = self.held as i32;
        for _ in 0..n {
            at = (at + step) % n;
            if at < 0 {
                at += n;
            }
            if self.guns[at as usize].have {
                self.held = at as usize;
                return;
            }
        }
    }

    /// Fill everything up and hand over every gun: what `give_all` does at
    /// a respawn, in a world with no pads to stand on.
    pub fn refill(&mut self) {
        for g in &mut self.guns {
            g.have = true;
            g.ammo = g.def.ammo_max;
            g.loaded = g.def.mag.min(g.ammo);
            g.spin = 0.0;
            g.cool = 0.0;
            g.reload = 0.0;
            g.charge = 0.0;
            g.burn = 0.0;
        }
        self.settle();
    }

    /// Give back everything but what a fighter respawns holding, for a world
    /// with pads: the lowest gun that takes ammunition, plus anything that
    /// takes none at all. The Digger is the second of those -- there
    /// is nothing in it to run out of, so there is nothing for a pad to
    /// hand over, and taking it away at a death would only leave a slot
    /// that could never be filled again.
    pub fn strip(&mut self) {
        let first = self.guns.iter().position(|g| !g.unlimited()).unwrap_or(0);
        for (i, g) in self.guns.iter_mut().enumerate() {
            g.have = i == first || g.unlimited();
            g.ammo = if i == first { g.def.ammo_start.min(g.def.ammo_max) } else { 0 };
            g.loaded = g.def.mag.min(g.ammo);
            g.spin = 0.0;
            g.cool = 0.0;
            g.reload = 0.0;
            g.charge = 0.0;
            g.burn = 0.0;
        }
        // The gun with rounds in it is the one in hand, not whatever happens
        // to sit in the lowest slot: you come back holding something you can
        // shoot, with the hammer a digit away.
        self.held = first.min(self.guns.len().saturating_sub(1));
        self.settle();
    }

    /// The gun of this library name, carried or not.
    pub fn named_mut(&mut self, name: &str) -> Option<&mut Held> {
        self.guns.iter_mut().find(|g| g.def.name == name)
    }

    /// Reload the weapon in hand, if there is anything to reload.
    pub fn reload(&mut self) -> bool {
        self.share();
        self.current_mut().is_some_and(|g| g.start_reload())
    }

    /// Square up the guns that share a store (`WeaponDef::ammo_type`): what
    /// each one spent or was given since the last time is taken from or put
    /// into the one store, and every gun of the kind then shows it. Each gun
    /// keeps its own magazine. Run every step before a gun is pulled, and
    /// after anything hands rounds over.
    pub fn share(&mut self) {
        for i in 0..self.guns.len() {
            if !self.guns[i].pooled || (0..i).any(|j| self.guns[j].kind() == self.guns[i].kind()) {
                continue;
            }
            let of: Vec<usize> = (i..self.guns.len()).filter(|&j| self.guns[j].kind() == self.guns[i].kind()).collect();
            let was = self.guns[i].pool_last;
            if of.iter().any(|&j| self.guns[j].pool_last != was) {
                self.settle();
                return;
            }
            let cap = of.iter().map(|&j| self.guns[j].def.ammo_max).max().unwrap_or(0) as i64;
            let moved: i64 = of.iter().map(|&j| self.guns[j].spare() as i64 - was as i64).sum();
            let store = (was as i64 + moved).clamp(0, cap) as u32;
            for &j in &of {
                let g = &mut self.guns[j];
                g.ammo = g.ammo - g.spare() + store;
                g.pool_last = store;
            }
        }
    }

    /// Make a shared store whole again after a respawn or a refill set each
    /// gun's rounds on its own: the store is the most any gun of the kind
    /// was left with.
    fn settle(&mut self) {
        for i in 0..self.guns.len() {
            if !self.guns[i].pooled {
                continue;
            }
            let kind = self.guns[i].kind().to_string();
            let of: Vec<usize> = (0..self.guns.len()).filter(|&j| self.guns[j].kind() == kind).collect();
            let cap = of.iter().map(|&j| self.guns[j].def.ammo_max).max().unwrap_or(0);
            let store = of.iter().map(|&j| self.guns[j].spare()).max().unwrap_or(0).min(cap);
            for &j in &of {
                let g = &mut self.guns[j];
                g.ammo = g.ammo - g.spare() + store;
                g.pool_last = store;
            }
        }
    }

    /// The best weapon with anything in it, for a respawn: the highest slot
    /// that is not empty.
    pub fn best_loaded(&self) -> Option<usize> {
        (0..self.guns.len()).rfind(|&i| !self.guns[i].empty())
    }

    /// Take out the best thing carried, if what is in hand has gone.
    pub fn draw_something(&mut self) {
        if self.current().is_some_and(|g| !g.empty()) {
            return;
        }
        if let Some(i) = self.best_loaded() {
            self.held = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{FireKind, WeaponDef};

    /// The sidearm and the minigun take one kind of round out of one store:
    /// what either fires is gone from both, a pad for either fills both, and
    /// each keeps its own magazine.
    #[test]
    fn guns_of_one_ammo_type_share_a_store() {
        let mut side = (*gun(600.0, 0.0, 400, 1)).clone();
        side.name = "side".into();
        side.mag = 40;
        side.reload_s = 1.0;
        side.ammo_start = 100;
        side.ammo_type = Some("bullets".into());
        let mut mini = (*gun(600.0, 0.0, 400, 1)).clone();
        mini.name = "mini".into();
        mini.slot = 7;
        mini.ammo_start = 0;
        mini.ammo_type = Some("bullets".into());
        let mut a = Arsenal { guns: vec![Held::new(Arc::new(side), false), Held::new(Arc::new(mini), false)], held: 0 };
        for i in 0..2 {
            a.guns[i].pooled = true;
        }
        a.settle();
        // 100 rounds, 40 of them in the sidearm: 60 in the store, which is
        // all the minigun has.
        assert_eq!((a.guns[0].ammo, a.guns[0].loaded, a.guns[1].ammo), (100, 40, 60));
        // The minigun fires ten: the sidearm's store is ten short too, and
        // its magazine is untouched.
        a.guns[1].ammo -= 10;
        a.share();
        assert_eq!((a.guns[0].ammo, a.guns[0].loaded, a.guns[1].ammo), (90, 40, 50));
        // The sidearm fires five out of its magazine: the store is the same.
        a.guns[0].ammo -= 5;
        a.guns[0].loaded -= 5;
        a.share();
        assert_eq!(a.guns[1].ammo, 50, "a round from the magazine came out of the store");
        // A pad for the sidearm fills the minigun as well.
        a.guns[0].give(30);
        a.share();
        assert_eq!(a.guns[1].ammo, 80);
        // A reload moves rounds from the store into the sidearm.
        assert!(a.guns[0].start_reload());
        a.guns[0].reload = 0.001;
        let _ = a.guns[0].pull(Trigger::Off, 0.01);
        a.share();
        assert_eq!((a.guns[0].loaded, a.guns[1].ammo), (40, 75));
    }

    fn gun(rpm: f32, spin_up: f32, ammo: u32, per_shot: u32) -> Arc<WeaponDef> {
        Arc::new(WeaponDef {
            name: "test".into(),
            version: 1,
            introduced_by: String::new(),
            display: "TEST".into(),
            slot: 1,
            ammo_max: ammo,
            ammo_start: ammo,
            mag: 0,
            reload_s: 0.0,
            ammo_type: None,
            dig: None,
            fire: FireDef {
                bore_r: 0.0,
                goo_r: 0.0,
                spoil_r: 0.0,
                burst: 0,
                burst_gap_s: 0.0,
                burst_spread_deg: 0.0,
                pierce_m: 0.0,
                guide: 0.0,
                ring: false,
                twist: 0.0,
                arc_jumps: 0,
                arc_m: 0.0,
                arc_keep: 0.5,
                bend_m: 0.0,
                cluster: None,
                kind: FireKind::Hitscan,
                rpm,
                damage: 1.0,
                pellets: 1,
                spread_deg: 0.0,
                range: 100.0,
                falloff: 1.0,
                spin_up,
                speed: 0.0,
                gravity: 0.0,
                bounces: 0,
                bounce_keep: 0.45,
                self_hit: 0.0,
                fuse_s: 0.0,
                splash: None,
                per_shot,
                kick: 0.0,
                tracer: [1.0; 3],
                tracer_s: 0.05,
                charge_s: 0.0,
                charge_mult: 1.0,
                stick_s: 0.0,
                head_mult: 1.0,
                look: String::new(),
            },
            alt: None,
            view: String::new(),
            grip: Vec::new(),
            zoom: 0.0,
            tint: [1.0; 3],
            notes: String::new(),
        })
    }

    /// A burst mode fires its rounds and then waits; a tap shorter than the
    /// burst still gets all of it.
    #[test]
    fn a_burst_is_three_rounds_and_a_pause() {
        let mut def = (*gun(900.0, 0.0, 1000, 1)).clone();
        def.fire.burst = 3;
        def.fire.burst_gap_s = 0.3;
        let def = Arc::new(def);
        let dt = 1.0 / 120.0;
        // Held for a second: 3 rounds take 0.2 s of gaps, plus 0.3 s of
        // pause, so two whole bursts.
        let mut h = Held::new(def.clone(), true);
        let mut times = Vec::new();
        for i in 0..120 {
            for _ in 0..h.pull(Trigger::Fire, dt).shots {
                times.push(i as f32 * dt);
            }
        }
        assert_eq!(times.len(), 6, "rounds in a held second: {times:?}");
        assert!(times[3] - times[2] > 0.3, "no pause between bursts: {times:?}");
        assert!(times[2] - times[0] < 0.2, "the burst was not rapid: {times:?}");
        // One frame on the button and then off: the burst still finishes.
        let mut h = Held::new(def, true);
        let mut shots = h.pull(Trigger::Fire, dt).shots;
        for _ in 0..60 {
            shots += h.pull(Trigger::Off, dt).shots;
        }
        assert_eq!(shots, 3, "a tap fired {shots}");
    }

    /// A second on the trigger at 600 rpm is ten rounds, however the frames
    /// fall.
    #[test]
    fn the_cadence_is_the_rpm() {
        for dt in [1.0f32 / 60.0, 1.0 / 30.0, 1.0 / 144.0] {
            let mut h = Held::new(gun(600.0, 0.0, 1000, 1), true);
            let mut n = 0;
            let steps = (1.0 / dt).round() as i32;
            for _ in 0..steps {
                n += h.pull(Trigger::Fire, dt).shots;
            }
            assert!((n as i32 - 10).abs() <= 1, "{n} rounds in a second at dt {dt}");
        }
    }

    /// A spinning gun starts slow and reaches its rate, and lets go again.
    #[test]
    fn the_barrels_spin_up_and_down() {
        let mut h = Held::new(gun(1200.0, 0.5, 4000, 1), true);
        let mut first = 0;
        for _ in 0..30 {
            first += h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        let mut then = 0;
        for _ in 0..30 {
            then += h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        assert!(then > first, "the second half second ({then}) was no faster than the first ({first})");
        assert!(h.spin > 0.99);
        for _ in 0..60 {
            h.pull(Trigger::Off, 1.0 / 60.0).shots;
        }
        assert_eq!(h.spin, 0.0, "the barrels never ran down");
    }

    /// A magazine weapon fires what it holds, reloads itself, and comes
    /// back: the reload is the real cadence, not the rpm.
    #[test]
    fn a_magazine_reloads_itself() {
        let mut def = (*gun(600.0, 0.0, 100, 1)).clone();
        def.mag = 4;
        def.reload_s = 0.5;
        let mut h = Held::new(Arc::new(def), true);
        assert_eq!(h.in_gun(), 4, "it did not start loaded");
        let mut n = 0;
        for _ in 0..30 {
            n += h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        assert_eq!(n, 4, "half a second at 600 rpm emptied more than the magazine: {n}");
        assert!(h.reloading(), "an empty magazine did not start a reload");
        assert_eq!(h.pull(Trigger::Fire, 1.0 / 60.0).shots, 0, "it fired while reloading");
        // Finger off the trigger while it reloads, or it empties the fresh
        // magazine as fast as it is filled and is reloading again by the
        // time we look.
        for _ in 0..40 {
            h.pull(Trigger::Off, 1.0 / 60.0).shots;
        }
        assert!(!h.reloading() && h.in_gun() > 0, "it never came back from the reload");
        // The store counts the rounds in the gun, so a reload moves nothing
        // out of it: four fired is four gone, and no more.
        assert_eq!(h.ammo, 96, "the store lost rounds that were never fired");
        assert_eq!(h.in_gun(), 4, "it did not come back with a full magazine");
    }

    /// A belt-fed gun has no magazine and never reloads: the minigun, whose
    /// spin-up is the wait it has instead.
    #[test]
    fn a_belt_never_reloads() {
        let mut h = Held::new(gun(600.0, 0.0, 100, 1), true);
        assert!(!h.start_reload(), "a gun with no magazine started a reload");
        for _ in 0..60 {
            h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        assert!(!h.reloading());
    }

    /// A charging mode holds its shot back while the button is down and
    /// lets it go when the button comes up, worth more for the wait.
    #[test]
    fn a_charge_is_bought_with_the_wait() {
        let mut def = (*gun(120.0, 0.0, 100, 1)).clone();
        def.fire.charge_s = 1.0;
        def.fire.charge_mult = 3.0;
        let mut h = Held::new(Arc::new(def), true);
        // Half a second down: nothing has left the gun.
        let mut fired = Pulled::none();
        for _ in 0..30 {
            let p = h.pull(Trigger::Fire, 1.0 / 60.0);
            assert_eq!(p.shots, 0, "it fired while the charge was building");
            fired = p;
        }
        assert!(h.charge > 0.4 && h.charge < 0.6, "half a second built {}", h.charge);
        assert_eq!(fired.shots, 0);
        // Let go: one shot, worth half the full multiplier.
        let out = h.pull(Trigger::Off, 1.0 / 60.0);
        assert_eq!(out.shots, 1, "letting go fired nothing");
        assert!((out.strength - 2.0).abs() < 0.15, "a half charge was worth {}", out.strength);
        assert_eq!(h.charge, 0.0, "the charge was not spent");
        assert_eq!(h.ammo, 99, "a charged shot cost {} rounds", 100 - h.ammo);
    }

    /// Held past full, it waits for the finger: a full charge stays loaded
    /// until the button comes up, then goes at full strength, in the mode
    /// it was built in although the button is no longer down.
    #[test]
    fn a_full_charge_waits_for_the_finger() {
        let mut def = (*gun(120.0, 0.0, 100, 1)).clone();
        def.fire.charge_s = 0.5;
        def.fire.charge_mult = 2.0;
        let mut h = Held::new(Arc::new(def), true);
        for _ in 0..180 {
            assert_eq!(h.pull(Trigger::Fire, 1.0 / 60.0).shots, 0, "a held full charge went by itself");
        }
        let out = h.pull(Trigger::Off, 1.0 / 60.0);
        assert_eq!(out.shots, 1, "letting go of a full charge fired nothing");
        assert!((out.strength - 2.0).abs() < 0.01, "a full charge was worth {}", out.strength);
        assert!(h.fired_mode(&out, Trigger::Off).is_some(), "the released shot has no mode to be fired in");
    }

    /// A beam fires no rounds at all: it reports itself live and drains the
    /// store by the second.
    #[test]
    fn a_beam_burns_by_the_second() {
        let mut def = (*gun(600.0, 0.0, 100, 1)).clone();
        def.fire.kind = FireKind::Beam;
        let mut h = Held::new(Arc::new(def), true);
        let mut lit = 0;
        for _ in 0..60 {
            let p = h.pull(Trigger::Fire, 1.0 / 60.0);
            assert_eq!(p.shots, 0, "a beam fired a round");
            lit += p.beam as u32;
        }
        assert_eq!(lit, 60, "the beam flickered: live for {lit} of 60 steps");
        // 600 rpm is ten rounds a second.
        assert!((h.ammo as i32 - 90).abs() <= 1, "a second of beam cost {} rounds", 100 - h.ammo);
    }

    /// A weapon with no ammunition at all never runs out and never wants a
    /// pad: the Digger.
    #[test]
    fn a_weapon_with_no_rounds_never_empties() {
        let mut def = (*gun(120.0, 0.0, 0, 1)).clone();
        def.ammo_max = 0;
        def.ammo_start = 0;
        let mut h = Held::new(Arc::new(def), true);
        assert!(h.unlimited() && !h.empty());
        let mut n = 0;
        for _ in 0..600 {
            n += h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        assert!(n > 15, "ten seconds at 120 rpm fired {n}");
        assert!(!h.empty(), "it ran out of what it does not have");
        assert!(!h.wants_ammo(), "it asked for rounds it cannot hold");
    }

    /// A gun whose right button is a sight does not fire on it.
    #[test]
    fn a_sight_is_not_a_second_trigger() {
        let mut def = (*gun(60.0, 0.0, 10, 1)).clone();
        def.zoom = 0.3;
        let mut h = Held::new(Arc::new(def), true);
        assert!(h.mode(Trigger::Alt).is_none(), "the sight is a second fire mode");
        for _ in 0..120 {
            assert_eq!(h.pull(Trigger::Alt, 1.0 / 60.0).shots, 0, "the sight fired a round");
        }
        assert_eq!(h.ammo, 10);
    }

    /// A sight that charges: letting the sight down throws the charge away,
    /// and the left button with the sight up lets it go.
    #[test]
    fn letting_the_sight_down_does_not_fire() {
        let mut def = (*gun(60.0, 0.0, 10, 1)).clone();
        def.zoom = 0.3;
        let mut alt = def.fire.clone();
        alt.charge_s = 1.0;
        alt.charge_mult = 1.5;
        def.alt = Some(alt);
        let mut h = Held::new(Arc::new(def), true);
        let dt = 1.0 / 60.0;
        let pull = |h: &mut Held, fire, alt| {
            let t = h.hands(fire, alt);
            h.pull(t, dt)
        };
        for _ in 0..90 {
            assert_eq!(pull(&mut h, false, true).shots, 0);
        }
        assert_eq!(pull(&mut h, false, false).shots, 0, "letting the sight down fired");
        assert_eq!(h.ammo, 10);
        for _ in 0..90 {
            pull(&mut h, false, true);
        }
        let p = pull(&mut h, true, true);
        assert_eq!(p.shots, 1, "the left button did not loose the charge");
        assert!(p.strength > 1.4, "a full charge went at {}", p.strength);
        assert_eq!(h.ammo, 9);
    }

    /// It stops when the magazine is out, and a shell costs what it says.
    #[test]
    fn it_runs_out() {
        let mut h = Held::new(gun(6000.0, 0.0, 6, 2), true);
        let mut n = 0;
        for _ in 0..60 {
            n += h.pull(Trigger::Fire, 1.0 / 60.0).shots;
        }
        assert_eq!(n, 3, "six rounds at two apiece is three shots");
        assert!(h.empty());
    }
}
