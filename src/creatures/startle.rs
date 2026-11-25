//! When an animal runs from the player.
//!
//! Two numbers describe a species' feeling about people. Its library entry's
//! `wary` is the **flee distance**: how far out it minds the player at all.
//! Its `flightiness` is how readily it actually bolts once the player is
//! inside that distance: 0 for a manta ray, which carries on as if nobody
//! were there, about 0.2 for a crocodile, 1 for an ordinary wild animal and
//! close to 2 for a hare.
//!
//! Inside the flee distance the player presses on the animal at some rate of
//! `pressure` per second, and the animal bolts once a second's worth has
//! built up (`bolts`). That gives the behaviour wanted here without a
//! per-step coin flip: walk slowly toward a deer and it turns and watches
//! you (anim/life.rs does the watching) and only breaks when you are close,
//! while the same deer bolts the moment a bike comes over the rise, because
//! the bike's pressure is ten times higher. Pressure rises with
//!
//! - **proximity**, steeply, and doubled within two metres. Height counts,
//!   so a glider twenty metres up is twenty metres away even right overhead;
//! - **crouching** (`Threat::crouched`): down on your heels halves the menace.
//! - **the way the player travels** (`Threat::menace`): a bike alarms, a
//!   wingsuit overhead barely registers;
//! - **approach speed** (`Threat::haste`): standing still is a third of the
//!   pressure of running straight at it;
//! - **which way the animal is facing**: a fifth less from behind, where it
//!   has not seen you;
//!
//! and falls where something stands between the two of you (`cover`), which
//! is only asked about within `COVER_M` so it costs one short raycast for
//! the few animals at close range.
//!
//! Those are spread over what the animal can sense, each a term of its own
//! (`pressure_on`):
//!
//! - **sight** is movement it can see. Nothing at all while you hold still,
//!   least from behind, and little while its head is down in the grass
//!   (`head_down`): move while it grazes, freeze when it looks up.
//! - **noise** is your footsteps (`Threat::noise`): loud on dry leaves and
//!   at a run, quiet on wet moss, and masked by rain (game.rs works it out
//!   from the ground underfoot and the weather).
//! - **closeness**: someone standing a few metres off is alarming however
//!   still they keep.
//! - **scent** carries downwind of you (`Threat::scent`), past the flee
//!   distance, and neither crouching nor cover nor keeping still helps. On
//!   a still day it pools round you instead, over a shorter reach.
//!
//! A pressure too small to matter lets the alarm settle rather than slowly
//! build (`bolts`): an animal that has seen you and then sees nothing more
//! goes back to grazing.
//!
//! The same model runs a prey animal's flight from the predators its
//! `flees_from` lists, through `Nerve::hunted` and `Threat::animal`: a wolf
//! trotting past at a distance is watched, one coming in at speed is run from,
//! and everything runs from a predator in the end, however little it minds
//! people. Cover is not asked about there, since a deer under a tree still
//! hears the wolf.
//!
//! Whether a walker minds the player at all is still `flees_from: ["player"]`
//! (`Species::flees_player`, the gate in walk.rs). Flightiness is the graded
//! knob inside that, and the only one the flocks, schools and insects have.
//!
//! Age shifts flightiness, not distance: `juvenile_flightiness` is below 1
//! for the species whose young stand and stare (fawns, bear cubs) and above
//! 1 for the ones whose young are jumpier than their parents, and the aged
//! are a little warier than their prime.

use super::age::Stage;
use super::Species;
use crate::ride::Mode;
use crate::variety::Morph;
use glam::Vec3;

/// Nerve per second at the very edge of patience, before any modifier.
const BASE: f32 = 3.0;
/// How quickly built-up alarm drains once the player backs off.
const CALM: f32 = 0.5;
/// An old animal's flightiness, relative to its prime.
const AGED: f32 = 1.12;
/// Cover is only looked for this close, to keep the raycast rare.
pub const COVER_M: f32 = 6.0;
/// How much more a predator alarms than a person on foot.
const PREDATOR: f32 = 1.6;
/// Below this much pressure (nerve per second) the alarm settles rather than
/// builds, faster the less is pressing: keeping still lets it calm down.
const QUIET: f32 = 0.3;
/// The weights of the senses (`pressure_on`). Sight is scaled by how much the
/// animal is watching and how much there is to see; these hold a person
/// walking in plain view at about what the single-term model gave.
const SIGHT: f32 = 1.0;
const NOISE: f32 = 0.25;
const CLOSE: f32 = 0.3;
/// Scent at full carry, and how far past the flee distance it reaches.
const SCENT: f32 = 0.45;
const SCENT_REACH: f32 = 1.5;
/// Footsteps on ordinary ground at a walk.
pub const STEP_NOISE: f32 = 0.5;

/// Something an animal might run from: the player, or a predator.
#[derive(Clone, Copy, Debug)]
pub struct Threat {
    pub pos: Vec3,
    /// Horizontal velocity, m/s.
    pub vel: Vec3,
    /// How alarming this one is in itself, apart from where it is and how
    /// fast it is coming: 1 is a person on foot.
    pub menace: f32,
    /// How loud its footsteps are: 0 silent, `STEP_NOISE` walking on
    /// ordinary ground, 2 and more running through dry leaves.
    pub noise: f32,
    /// Which way its scent drifts (horizontal, unit) times how far it
    /// carries (0 none, about 1 on a steady breeze): the wind.
    pub scent: Vec3,
    /// Scent lying round it on a still day, 0 to 1: no direction, a short
    /// reach.
    pub pool: f32,
}

impl Default for Threat {
    fn default() -> Self {
        Threat {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            menace: 1.0,
            noise: 0.0,
            scent: Vec3::ZERO,
            pool: 0.0,
        }
    }
}

/// Footsteps from speed alone, on ordinary ground: silent standing still,
/// `STEP_NOISE` at a walk, more at a run.
fn steps_at(speed: f32) -> f32 {
    STEP_NOISE * (speed / 1.4).min(3.0)
}

impl Threat {
    /// The player, travelling by `mode`. A bike is loud and fast at eye
    /// level; a glider passes overhead in silence; a kayak and a climber are
    /// quiet and slow.
    pub fn new(pos: Vec3, vel: Vec3, mode: Mode) -> Self {
        let menace = match mode {
            Mode::Foot => 1.0,
            Mode::Bike => 1.4,
            Mode::Wing => 0.35,
            Mode::Climb => 0.55,
            Mode::Swing => 0.8,
            Mode::Kayak => 0.65,
            // An engine: loud, fast and big. Most things are gone before it
            // arrives.
            Mode::Drive => 2.0,
        };
        let flat = Vec3::new(vel.x, 0.0, vel.z);
        // A bike rattles whatever the ground; a glider and a kayak are
        // nearly silent.
        let noise = match mode {
            Mode::Foot => steps_at(flat.length()),
            Mode::Bike => 1.2 + flat.length() / 10.0,
            Mode::Wing => 0.0,
            Mode::Climb | Mode::Swing => 0.2,
            Mode::Kayak => 0.15,
            Mode::Drive => 2.5 + flat.length() / 6.0,
        };
        Threat {
            pos,
            vel: flat,
            menace,
            noise,
            ..Threat::default()
        }
    }

    /// Crouched (0 standing, 1 right down): low and slow is half as alarming,
    /// which is what lets you near anything worth photographing. Crouching
    /// also softens the step.
    pub fn crouched(mut self, crouch: f32) -> Self {
        let c = crouch.clamp(0.0, 1.0);
        self.menace *= 1.0 - 0.5 * c;
        self.noise *= 1.0 - 0.4 * c;
        self
    }

    /// The footsteps scaled by what they fall on and what is masking them:
    /// `ground` 1 for ordinary ground (dry leaves about 2, wet moss under
    /// half), `masked` 0 to 1 for the share drowned by rain or a river.
    pub fn heard(mut self, ground: f32, masked: f32) -> Self {
        self.noise *= ground.max(0.0) * (1.0 - masked.clamp(0.0, 0.9));
        self
    }

    /// The wind carrying its scent: `toward` is where the wind blows to
    /// (horizontal), `carry` how far it takes it, `pool` how much lies round
    /// it on a still day.
    pub fn scented(mut self, toward: Vec3, carry: f32, pool: f32) -> Self {
        self.scent = Vec3::new(toward.x, 0.0, toward.z).normalize_or_zero() * carry.max(0.0);
        self.pool = pool.clamp(0.0, 1.0);
        self
    }

    /// A predator, which alarms more than a person does.
    pub fn animal(pos: Vec3, vel: Vec3) -> Self {
        let flat = Vec3::new(vel.x, 0.0, vel.z);
        Threat {
            pos,
            vel: flat,
            menace: PREDATOR,
            noise: steps_at(flat.length()),
            ..Threat::default()
        }
    }

    /// Another animal bolting nearby: the alarm spreads from it. Weaker than
    /// the thing it ran from; it is the crash of the bolt that carries.
    pub fn alarm_of(pos: Vec3, vel: Vec3) -> Self {
        let flat = Vec3::new(vel.x, 0.0, vel.z);
        Threat {
            pos,
            vel: flat,
            menace: 0.7,
            noise: 1.0,
            ..Threat::default()
        }
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Closing speed toward `at`, m/s; negative moving away.
    pub fn closing(&self, at: Vec3) -> f32 {
        let d = Vec3::new(at.x - self.pos.x, 0.0, at.z - self.pos.z);
        self.vel.dot(d.normalize_or_zero())
    }

    /// Approach as the animal feels it: 0.3 for someone standing still, 1 and
    /// over for someone running straight at it.
    pub fn haste(&self, at: Vec3) -> f32 {
        0.3 + self.motion(at)
    }

    /// How much there is to see: 0 standing still, about 0.2 walking in,
    /// 0.8 running straight at it.
    pub fn motion(&self, at: Vec3) -> f32 {
        0.5 * (self.closing(at).max(0.0) / 5.0).min(1.0) + 0.3 * (self.speed() / 8.0).min(1.0)
    }

    /// How strongly this one's scent reaches `at`, 0 to 1.5: downwind of it
    /// in a cone, or all round on a still day over a shorter reach.
    pub fn scent_at(&self, at: Vec3) -> f32 {
        let carry = self.scent.length();
        let d = Vec3::new(at.x - self.pos.x, 0.0, at.z - self.pos.z).normalize_or_zero();
        let down = if carry > 0.0 { d.dot(self.scent / carry) } else { 0.0 };
        let cone = ((down - 0.7) / 0.25).clamp(0.0, 1.0);
        (cone * carry).max(self.pool * 0.5).min(1.5)
    }
}

/// One individual's feeling about people: how far out it minds them, and how
/// readily it runs.
#[derive(Clone, Copy, Debug)]
pub struct Nerve {
    /// Flee distance, metres.
    pub dist: f32,
    /// 0 never runs, 1 an ordinary wild animal, 2 a hare.
    pub flightiness: f32,
}

impl Nerve {
    /// A library creature's, with this individual's variation and age.
    pub fn of(sp: &Species, morph: Option<&Morph>, stage: Stage) -> Nerve {
        if stage == Stage::Dead {
            return Nerve { dist: 0.0, flightiness: 0.0 };
        }
        let age = match stage {
            Stage::Juvenile => sp.beh.juvenile_flightiness,
            Stage::Aged => AGED,
            _ => 1.0,
        };
        Nerve {
            dist: sp.beh.wary * morph.map_or(1.0, |m| m.wary),
            flightiness: (sp.beh.flightiness * age).max(0.0),
        }
    }

    /// The random island's animals (creatures/spawn.rs `LEGACY`), which have no library
    /// entry: an ordinary wild animal at `dist` metres.
    pub fn plain(dist: f32) -> Nerve {
        Nerve { dist, flightiness: 1.0 }
    }

    /// The same animal about something that eats it. Everything runs from a
    /// predator, a crocodile and a manta ray included, but a hare is still
    /// quicker about it than they are.
    pub fn hunted(&self) -> Nerve {
        Nerve {
            dist: self.dist,
            flightiness: 0.7 + 0.6 * self.flightiness.min(2.0),
        }
    }

    /// The same animal asleep (`drowsy` 0 to 1, from creatures/diel.rs): it
    /// notices you later and takes longer to make up its mind, which is why
    /// you can walk up on bedded deer at night. It still bolts once woken --
    /// `anim::life` scrambles it to its feet as the alarm rises.
    pub fn dozing(&self, drowsy: f32) -> Nerve {
        let d = drowsy.clamp(0.0, 1.0);
        Nerve {
            dist: self.dist * (1.0 - 0.4 * d),
            flightiness: self.flightiness * (1.0 - 0.35 * d),
        }
    }

    pub fn minds_people(&self) -> bool {
        self.flightiness > 0.0 && self.dist > 0.0
    }
}

/// How hard the player presses on an animal at `at` facing `facing` (a unit
/// horizontal vector), in nerve per second: at 1 it bolts after a second of
/// this, at 10 all but at once. 0 beyond its flee distance, and for anything
/// that doesn't mind people.
///
/// `cover` is only called within `COVER_M`, and says whether something stands
/// between the animal and the player.
pub fn pressure(n: &Nerve, at: Vec3, facing: Vec3, threat: &Threat, cover: impl FnOnce() -> bool) -> f32 {
    pressure_on(n, at, facing, 0.0, threat, cover)
}

/// `pressure` for an animal whose head is `head_down` (0 up and looking, 1
/// down in the grass or at the water): it sees much less of what moves.
pub fn pressure_on(n: &Nerve, at: Vec3, facing: Vec3, head_down: f32, threat: &Threat, cover: impl FnOnce() -> bool) -> f32 {
    senses_on(n, at, facing, head_down, threat, cover).total()
}

/// `pressure_on` split by sense, nerve per second each: what the inspector
/// (game/eco.rs) shows of why an animal is getting jumpy.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Senses {
    pub sight: f32,
    pub noise: f32,
    pub close: f32,
    pub scent: f32,
}

impl Senses {
    pub fn total(&self) -> f32 {
        self.sight + self.noise + self.close + self.scent
    }

    /// The sense pressing hardest, and how hard.
    pub fn strongest(&self) -> (&'static str, f32) {
        [("sight", self.sight), ("noise", self.noise), ("closeness", self.close), ("scent", self.scent)]
            .into_iter()
            .fold(("nothing", 0.0), |a, b| if b.1 > a.1 { b } else { a })
    }
}

pub fn senses_on(n: &Nerve, at: Vec3, facing: Vec3, head_down: f32, threat: &Threat, cover: impl FnOnce() -> bool) -> Senses {
    if !n.minds_people() {
        return Senses::default();
    }
    // How far off the player is, height included: a glider twenty metres up
    // is twenty metres away even when it passes right overhead, and so is
    // someone looking down from a cliff.
    let to_player = threat.pos - at;
    let dist = to_player.length();
    // Scent first: it reaches past the flee distance, downwind.
    let smell = threat.scent_at(at);
    let reach = n.dist * (1.0 + SCENT_REACH * smell.min(1.0));
    let scent = if smell > 0.0 && dist < reach {
        BASE * n.flightiness * SCENT * smell * (1.0 - dist / reach).powf(1.3)
    } else {
        0.0
    };
    if dist >= n.dist {
        return Senses { scent, ..Senses::default() };
    }
    // Steeply toward the animal, and doubled for someone standing over it.
    let prox = (1.0 - dist / n.dist).clamp(0.0, 1.0).powf(1.3);
    let near = if dist < 2.0 { 2.0 } else { 1.0 };
    // Sight: a good deal less from behind, where it has not seen you, and
    // little with its head down.
    let ahead = facing.dot(Vec3::new(to_player.x, 0.0, to_player.z).normalize_or_zero());
    let eyes = (0.4 + 0.6 * ((ahead + 0.6) / 0.8).clamp(0.0, 1.0)) * (1.0 - 0.75 * head_down.clamp(0.0, 1.0));
    let hidden = if dist < COVER_M && cover() { 0.45 } else { 1.0 };
    let sight = SIGHT * eyes * threat.motion(at) * hidden;
    let noise = NOISE * threat.noise;
    let close = CLOSE * prox;
    let k = BASE * n.flightiness * prox * near * threat.menace;
    Senses {
        sight: k * sight,
        noise: k * noise,
        close: k * close,
        scent,
    }
}

/// Add this step's `pressure` to an animal's built-up alarm and say whether
/// it bolts now. Alarm drains while nothing is pressing it, and settles while
/// little is (below `QUIET`), so an animal you froze in front of calms down.
pub fn bolts(alarm: &mut f32, pressure: f32, dt: f32) -> bool {
    if pressure <= 0.0 {
        *alarm = (*alarm - CALM * dt).max(0.0);
        return false;
    }
    let settle = CALM * (1.0 - pressure / QUIET).max(0.0);
    *alarm = (*alarm + (pressure - settle) * dt).max(0.0);
    if *alarm >= 1.0 {
        *alarm = 0.0;
        true
    } else {
        false
    }
}

/// The radius at which a group that flees as one — a flock bursting from a
/// tree, a school turning away, insects lifting off — goes. The groups have
/// no per-individual alarm to build up, so the species' flightiness and the
/// player's approach move the radius instead: a shy flock at a walking pace
/// lets you to two thirds of its flee distance, a bike at speed flushes it
/// half as far out again. 0 for a species that doesn't mind people.
///
/// `floor` is the least flee distance that movement makes sense with (a bird
/// needs a few metres to get off the branch).
pub fn reach(n: &Nerve, threat: &Threat, at: Vec3, floor: f32) -> f32 {
    if !n.minds_people() {
        return 0.0;
    }
    let base = n.dist.max(floor);
    let how = (threat.menace * threat.haste(at) / 0.75).clamp(0.45, 1.5);
    base * (0.5 + 0.5 * n.flightiness.min(2.0)) * how
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walking_at(at: Vec3) -> Threat {
        let d = Vec3::new(at.x, 0.0, at.z).normalize_or_zero() * 1.4;
        Threat::new(Vec3::ZERO, d, Mode::Foot)
    }

    fn secs_to_bolt(n: &Nerve, at: Vec3, threat: &Threat) -> Option<f32> {
        let mut alarm = 0.0;
        let dt = 1.0 / 30.0;
        for step in 0..600 {
            let p = pressure(n, at, Vec3::Z, threat, || false);
            if bolts(&mut alarm, p, dt) {
                return Some(step as f32 * dt);
            }
        }
        None
    }

    #[test]
    fn indifferent_species_never_bolt() {
        let n = Nerve { dist: 10.0, flightiness: 0.0 };
        let at = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(pressure(&n, at, Vec3::Z, &walking_at(at), || false), 0.0);
        assert_eq!(reach(&n, &walking_at(at), at, 6.0), 0.0);
    }

    #[test]
    fn nothing_beyond_the_flee_distance() {
        let n = Nerve::plain(8.0);
        let at = Vec3::new(0.0, 0.0, 9.0);
        assert_eq!(pressure(&n, at, Vec3::Z, &walking_at(at), || false), 0.0);
    }

    #[test]
    fn a_hare_bolts_sooner_than_a_crocodile() {
        let at = Vec3::new(0.0, 0.0, 3.0);
        let t = walking_at(at);
        let hare = secs_to_bolt(&Nerve { dist: 6.0, flightiness: 1.8 }, at, &t).unwrap();
        // A crocodile walked past at three metres barely minds: the pressure
        // is under what lets an alarm settle, so it may never go at all.
        let croc = secs_to_bolt(&Nerve { dist: 6.0, flightiness: 0.2 }, at, &t);
        assert!(hare < 1.5, "hare took {hare}s");
        assert!(croc.is_none_or(|c| c > hare * 4.0), "croc took {croc:?}s, hare {hare}s");
    }

    #[test]
    fn a_bike_presses_harder_than_a_wingsuit() {
        let at = Vec3::new(0.0, 0.0, 5.0);
        let n = Nerve::plain(10.0);
        let fast = Vec3::new(0.0, 0.0, 9.0);
        let bike = pressure(&n, at, Vec3::Z, &Threat::new(Vec3::ZERO, fast, Mode::Bike), || false);
        let wing = pressure(&n, at, Vec3::Z, &Threat::new(Vec3::ZERO, fast, Mode::Wing), || false);
        assert!(bike > wing * 3.0, "bike {bike}, wing {wing}");
    }

    #[test]
    fn standing_still_presses_least() {
        let at = Vec3::new(0.0, 0.0, 4.0);
        let n = Nerve::plain(10.0);
        let still = pressure(&n, at, Vec3::Z, &Threat::new(Vec3::ZERO, Vec3::ZERO, Mode::Foot), || false);
        let walk = pressure(&n, at, Vec3::Z, &walking_at(at), || false);
        let run = pressure(&n, at, Vec3::Z, &Threat::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 7.0), Mode::Foot), || false);
        assert!(still < walk && walk < run, "{still} {walk} {run}");
        assert!(run > still * 2.5);
    }

    #[test]
    fn less_from_behind_and_less_under_cover() {
        let at = Vec3::new(0.0, 0.0, 4.0);
        let n = Nerve::plain(10.0);
        let t = walking_at(at);
        // Facing the player (-Z, back toward the origin) against facing away.
        let seen = pressure(&n, at, -Vec3::Z, &t, || false);
        let behind = pressure(&n, at, Vec3::Z, &t, || false);
        assert!(behind < seen * 0.9, "{behind} vs {seen}");
        let hidden = pressure(&n, at, -Vec3::Z, &t, || true);
        assert!(hidden < seen * 0.9, "{hidden} vs {seen}");
    }

    #[test]
    fn a_fawn_stares_where_a_leveret_bolts() {
        let at = Vec3::new(0.0, 0.0, 4.0);
        let t = walking_at(at);
        let adult = Nerve { dist: 10.0, flightiness: 1.2 };
        let fawn = Nerve {
            flightiness: adult.flightiness * 0.55,
            ..adult
        };
        let a = secs_to_bolt(&adult, at, &t).unwrap();
        let f = secs_to_bolt(&fawn, at, &t).unwrap();
        assert!(f > a * 1.5, "fawn {f}s, adult {a}s");
    }

    #[test]
    fn everything_runs_from_a_predator_and_a_hare_still_fastest() {
        let at = Vec3::new(0.0, 0.0, 4.0);
        let wolf = Threat::animal(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0));
        // A manta ray ignores people and still runs from what eats it.
        let calm = Nerve { dist: 10.0, flightiness: 0.0 };
        assert_eq!(pressure(&calm, at, Vec3::Z, &wolf, || false), 0.0);
        let hunted = calm.hunted();
        assert!(pressure(&hunted, at, Vec3::Z, &wolf, || false) > 0.0);
        assert!(secs_to_bolt(&hunted, at, &wolf).is_some());
        // A hare is still quicker about it than a crocodile.
        let hare = Nerve { dist: 10.0, flightiness: 1.8 }.hunted();
        let croc = Nerve { dist: 10.0, flightiness: 0.2 }.hunted();
        assert!(secs_to_bolt(&hare, at, &wolf).unwrap() < secs_to_bolt(&croc, at, &wolf).unwrap());
    }

    #[test]
    fn a_predator_presses_harder_than_a_person_at_the_same_pace() {
        let at = Vec3::new(0.0, 0.0, 5.0);
        let n = Nerve::plain(10.0);
        let vel = Vec3::new(0.0, 0.0, 3.0);
        let wolf = pressure(&n.hunted(), at, Vec3::Z, &Threat::animal(Vec3::ZERO, vel), || false);
        let person = pressure(&n, at, Vec3::Z, &Threat::new(Vec3::ZERO, vel, Mode::Foot), || false);
        assert!(wolf > person * 1.5, "wolf {wolf}, person {person}");
    }

    #[test]
    fn an_entry_that_says_nothing_is_an_ordinary_wild_animal() {
        let beh: crate::library::Behaviour = serde_json::from_value(serde_json::json!({ "movement": "graze" })).unwrap();
        assert_eq!(beh.flightiness, 1.0);
        assert_eq!(beh.juvenile_flightiness, 1.0);
    }

    #[test]
    fn alarm_drains_when_the_player_leaves() {
        let mut alarm = 0.0;
        assert!(!bolts(&mut alarm, 0.9, 0.5));
        assert!(alarm > 0.4);
        for _ in 0..40 {
            bolts(&mut alarm, 0.0, 1.0 / 30.0);
        }
        assert_eq!(alarm, 0.0);
    }
    #[test]
    fn freezing_in_front_of_it_lets_it_settle() {
        // Seen at 6 m of a 10 m flee distance: walking in, it bolts; stop
        // and hold still, and the alarm built so far drains away.
        let n = Nerve::plain(10.0);
        let at = Vec3::new(0.0, 0.0, 6.0);
        let still = Threat::new(Vec3::ZERO, Vec3::ZERO, Mode::Foot);
        let mut alarm = 0.6;
        for _ in 0..90 {
            let p = pressure(&n, at, -Vec3::Z, &still, || false);
            assert!(!bolts(&mut alarm, p, 1.0 / 30.0));
        }
        assert!(alarm < 0.6, "alarm grew to {alarm} while the player stood still");
        assert!(secs_to_bolt(&n, at, &walking_at(at)).is_some());
    }

    #[test]
    fn a_grazing_head_sees_less() {
        let n = Nerve::plain(10.0);
        let at = Vec3::new(0.0, 0.0, 5.0);
        let t = walking_at(at).heard(0.0, 0.0);
        let up = pressure_on(&n, at, -Vec3::Z, 0.0, &t, || false);
        let down = pressure_on(&n, at, -Vec3::Z, 1.0, &t, || false);
        assert!(down < up * 0.75, "head down {down}, up {up}");
    }

    #[test]
    fn dry_leaves_carry_and_rain_masks() {
        let n = Nerve::plain(10.0);
        let at = Vec3::new(0.0, 0.0, 5.0);
        let base = walking_at(at);
        let leaves = pressure(&n, at, Vec3::Z, &base.heard(2.0, 0.0), || false);
        let moss = pressure(&n, at, Vec3::Z, &base.heard(0.4, 0.0), || false);
        let rain = pressure(&n, at, Vec3::Z, &base.heard(2.0, 0.8), || false);
        assert!(leaves > moss && rain < leaves, "leaves {leaves}, moss {moss}, rain {rain}");
    }

    #[test]
    fn scent_reaches_downwind_past_the_flee_distance_and_not_upwind() {
        let n = Nerve::plain(10.0);
        let still = Threat::new(Vec3::ZERO, Vec3::ZERO, Mode::Foot).scented(Vec3::Z, 1.0, 0.0);
        let downwind = Vec3::new(0.0, 0.0, 18.0);
        let upwind = Vec3::new(0.0, 0.0, -18.0);
        assert!(pressure(&n, downwind, -Vec3::Z, &still, || true) > 0.0);
        assert_eq!(pressure(&n, upwind, Vec3::Z, &still, || false), 0.0);
        // Crouching does not hide a scent.
        let low = still.crouched(1.0);
        assert_eq!(pressure(&n, downwind, -Vec3::Z, &low, || true), pressure(&n, downwind, -Vec3::Z, &still, || true));
        // Upwind and still, close in, it settles; downwind it does not.
        let near_up = Vec3::new(0.0, 0.0, -7.0);
        let near_down = Vec3::new(0.0, 0.0, 7.0);
        assert!(secs_to_bolt(&n, near_up, &still).is_none());
        assert!(secs_to_bolt(&n, near_down, &still).is_some());
    }
}
