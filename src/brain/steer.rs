//! A want turned into keys.
//!
//! This is the layer that keeps a bot honest. It takes a direction in the
//! world and works out which of forward, back, left and right a person would
//! be holding to go that way, which is a quantisation: four booleans cannot
//! express every heading, so a bot moves on the eight headings a player can,
//! and has to turn its body to go anywhere else.
//!
//! Wants add before they become keys, which is where flocking plugs in: a
//! herd member's `go` is the sum of its path and its neighbours
//! (`creatures::flock::steer` returns exactly such a vector). Boids is a
//! steering behaviour, not a brain.

use crate::player::MoveInput;
use glam::Vec3;

/// What the body is being asked to do, before it is keys.
#[derive(Default, Clone, Copy)]
pub struct Want {
    /// Where to travel, in the world, level. Zero to stand still.
    pub go: Vec3,
    pub sprint: bool,
    pub jump: bool,
    /// Sidestep while keeping the current facing: -1 left, +1 right. Added
    /// to `go` in the body's own frame, which is how a bot circles you while
    /// still looking at you.
    pub strafe: f32,
    /// Break the step with a dodge that way (`Tapper`). `None` is every
    /// want that is only walking, which is most of them.
    pub dodge: Option<Dodge>,
}

/// Which key a dodge taps, in the body's own frame.
///
/// A dodge is the one move a brain cannot ask for. The body reads one off
/// the keys -- a direction pressed twice in a quarter of a second
/// (`Player::double_tap`) -- and it reads a bot's keys by exactly the same
/// rule it reads a player's, because there is one piece of movement code.
/// So a want for a dodge goes to a `Tapper`, which does with the key what a
/// player's hand does with it.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dodge {
    #[default]
    Forward,
    Back,
    Left,
    Right,
}

/// Body steps one dodge takes to tap: let go, press, let go, press. Four
/// steps is 0.13 s, which is well inside the window the body allows, and
/// short enough that the keys are the want's own again almost at once.
const TAP: u8 = 4;

/// One key being tapped twice, which is all a dodge is from outside the
/// body.
///
/// It is also the hand that keeps a bot from dodging by accident. The body
/// reads two presses of a key inside `Player::double_tap`'s window as a
/// dodge whoever made them, and a brain whose wanted heading wobbles across
/// a key's threshold presses, lets go and presses again several times a
/// second. Measured in the league: the brain asked for five dodges a minute
/// and the body took thirty-eight, which is the "they dodge sideways faster
/// than I can" of play. A person does not re-press a key that fast without
/// meaning to, so this hand will not either: outside a tap it refuses a
/// press of a key pressed less than `GUARD` steps ago.
pub struct Tapper {
    /// Steps of the tap left; 0 is nothing in progress.
    left: u8,
    key: Dodge,
    /// Per key, forward, back, left, right: down last step, and steps since
    /// it last went down.
    was: [bool; 4],
    since: [u16; 4],
}

impl Default for Tapper {
    fn default() -> Self {
        // Every key as if last pressed long ago, or a body's first press of
        // each key would be held back.
        Tapper { left: 0, key: Dodge::Forward, was: [false; 4], since: [u16::MAX; 4] }
    }
}

/// Body steps a key must wait after going down before it may go down again
/// unbidden: 0.3 s, past the body's 0.26 s double-tap window.
const GUARD: u16 = 18;

impl Tapper {
    /// One **body** step, not one frame. The bodies run on the fixed step
    /// (`Shooter::step_bodies`) and a frame may hold none or several, so a
    /// pattern counted in frames is a pattern the body never sees.
    ///
    /// `want` is a fresh request, taken only when nothing is already being
    /// tapped. What comes back is the want's own keys with that one key
    /// driven by the tap instead.
    pub fn step(&mut self, want: Option<Dodge>, mv: MoveInput) -> MoveInput {
        let tapping = self.left > 0 || want.is_some();
        let mut out = self.tap(want, mv);
        let keys = [&mut out.forward, &mut out.back, &mut out.left, &mut out.right];
        let tapped = self.key as usize;
        for (i, k) in keys.into_iter().enumerate() {
            let meant = tapping && i == tapped;
            if *k && !self.was[i] && !meant && self.since[i] < GUARD {
                *k = false;
            }
            // Nor reverse a sidestep it has only just begun: a side key may
            // not go down within `GUARD` of the other one going down. The
            // league found 38% of reversals came after under 0.3 s.
            let other = match i {
                2 => Some(3),
                3 => Some(2),
                _ => None,
            };
            if let Some(o) = other {
                if *k && !self.was[i] && !meant && self.since[o] < GUARD {
                    *k = false;
                }
            }
            if *k && !self.was[i] {
                self.since[i] = 0;
            } else {
                self.since[i] = self.since[i].saturating_add(1);
            }
            self.was[i] = *k;
        }
        out
    }

    fn tap(&mut self, want: Option<Dodge>, mut mv: MoveInput) -> MoveInput {
        if self.left == 0 {
            match want {
                Some(d) => {
                    self.left = TAP;
                    self.key = d;
                }
                None => return mv,
            }
        }
        // Down on the odd counts: 4 up, 3 down, 2 up, 1 down. It starts on
        // an up so the press that follows is an edge whether or not the
        // body was already walking that way.
        let down = self.left % 2 == 1;
        let k = match self.key {
            Dodge::Forward => &mut mv.forward,
            Dodge::Back => &mut mv.back,
            Dodge::Left => &mut mv.left,
            Dodge::Right => &mut mv.right,
        };
        *k = down;
        self.left -= 1;
        mv
    }

    /// Mid-tap: a second dodge asked for now would be dropped.
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.left > 0
    }
}

impl Dodge {
    /// The key that best matches going `go` for a body facing `yaw`: the
    /// same quantisation `drive` does, since four keys cannot express every
    /// heading. `None` for a want of nowhere.
    ///
    /// A brain that wants to dodge *along its way out* has to ask for this
    /// rather than for `Forward`, because a bot breaking off is still
    /// looking at whoever it is breaking off from, and forward for that body
    /// is straight back into them.
    pub fn toward(yaw: f32, go: Vec3) -> Option<Dodge> {
        let level = Vec3::new(go.x, 0.0, go.z).normalize_or_zero();
        if level == Vec3::ZERO {
            return None;
        }
        let (sy, cy) = yaw.sin_cos();
        let f = level.dot(Vec3::new(-sy, 0.0, -cy));
        let r = level.dot(Vec3::new(cy, 0.0, -sy));
        Some(if f.abs() >= r.abs() {
            if f > 0.0 {
                Dodge::Forward
            } else {
                Dodge::Back
            }
        } else if r > 0.0 {
            Dodge::Right
        } else {
            Dodge::Left
        })
    }

    /// Where this dodge goes in the world, for a body facing `yaw`.
    pub fn heading(self, yaw: f32) -> Vec3 {
        let (sy, cy) = yaw.sin_cos();
        let fwd = Vec3::new(-sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, -sy);
        match self {
            Dodge::Forward => fwd,
            Dodge::Back => -fwd,
            Dodge::Left => -right,
            Dodge::Right => right,
        }
    }
}

/// How much of a direction has to be on an axis before the key goes down.
/// Lower and a bot mashes all four; higher and it refuses diagonals.
const PRESS: f32 = 0.35;

/// The keys a body facing `yaw` would hold to travel `go` and sidestep.
pub fn drive(yaw: f32, w: &Want) -> MoveInput {
    let (sy, cy) = yaw.sin_cos();
    let fwd = Vec3::new(-sy, 0.0, -cy);
    let right = Vec3::new(cy, 0.0, -sy);
    let level = Vec3::new(w.go.x, 0.0, w.go.z);
    let mut f = level.dot(fwd);
    let mut r = level.dot(right) + w.strafe;
    // Normalise so a want of any length presses the same keys: what matters
    // is the heading, not how badly it wants to go there.
    let m = (f * f + r * r).sqrt();
    if m > 1e-4 {
        f /= m;
        r /= m;
    }
    MoveInput {
        forward: f > PRESS,
        back: f < -PRESS,
        right: r > PRESS,
        left: r < -PRESS,
        jump: w.jump,
        sprint: w.sprint,
        ..Default::default()
    }
}

/// The keys for a body that is not aiming at anybody: forward, steering with
/// the head, as a person runs. A side key goes down only when the way is more
/// than `side` (a sine) off the facing, so a bot following a path while its
/// head comes round holds W rather than flicking A and D. `drive` presses a
/// side key at about twenty degrees, which is right for a strafe and wrong
/// for a run.
pub fn drive_running(yaw: f32, w: &Want, side: f32) -> MoveInput {
    let mut mv = drive(yaw, w);
    let (sy, cy) = yaw.sin_cos();
    let fwd = Vec3::new(-sy, 0.0, -cy);
    let right = Vec3::new(cy, 0.0, -sy);
    let level = Vec3::new(w.go.x, 0.0, w.go.z).normalize_or_zero();
    let f = level.dot(fwd);
    let r = level.dot(right) + w.strafe;
    if f > 0.0 && r.abs() < side {
        mv.left = false;
        mv.right = false;
        mv.forward = level != Vec3::ZERO;
    }
    mv
}

/// Is there floor ahead? A body about to walk off a ledge it did not mean to
/// is the commonest way a bot looks stupid, so steering checks for itself.
/// `ahead` metres in front of `foot`, and `drop` metres is how far down
/// counts as a fall.
pub fn footing(store: &crate::world::store::Store, foot: Vec3, dir: Vec3, ahead: f32, drop: f32) -> bool {
    let level = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if level == Vec3::ZERO {
        return true;
    }
    let p = foot + level * ahead + Vec3::Y * 0.3;
    crate::creatures::terrain::voxel_ground(store, p.x, p.z, p.y).is_some_and(|(y, _)| foot.y - y < drop)
}

/// Is the way ahead open, or is there a wall in it?
///
/// `footing` asks about the floor; this asks about everything above it. A
/// bot closing on a target steers straight at it, and in a real map that
/// line goes through walls -- which is the commonest way a bot ends up
/// pressed against one, shuffling, for the rest of the match. Three probes
/// up the body rather than one, because a knee-high step is not a wall and a
/// lintel is.
pub fn clear(store: &crate::world::store::Store, foot: Vec3, dir: Vec3, ahead: f32, half_w: f32, height: f32) -> bool {
    let level = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if level == Vec3::ZERO {
        return true;
    }
    // From a step's height up, so a kerb does not read as a wall.
    [0.45, 0.5 * height, 0.92 * height]
        .iter()
        .all(|&up| !crate::physics::hits(store, foot + level * ahead + Vec3::Y * up, half_w, 0.05))
}

/// A want with the wall taken out of it: the same direction if the way is
/// open, else the open one of the two along the wall, else nothing.
///
/// Sliding rather than stopping is the point. A bot that stops when its line
/// is blocked has given up on the fight; one that slides finds the doorway,
/// because a doorway is the direction that stops being blocked.
///
/// It keeps to one side of the wall. The fan used to take the first clear
/// heading and always tried the same side first, so a body at the corner of
/// a doorframe slid one
/// way, found that side blocked a step later, slid back, and did that several
/// times a second. The league counted it: over half of a roaming bot's
/// hundred sidestep reversals a minute were this. Given the side it slid to
/// last (`keep`, +1 or -1, 0 for no preference), this tries every heading on
/// that side before any on the other, and says which side it took (0 when
/// the way was clear and no slide was needed).
pub fn around_keeping(store: &crate::world::store::Store, foot: Vec3, go: Vec3, ahead: f32, half_w: f32, height: f32, keep: f32) -> (Vec3, f32) {
    let level = Vec3::new(go.x, 0.0, go.z);
    let d = level.length();
    if d < 1e-4 || clear(store, foot, level, ahead, half_w, height) {
        return (go, 0.0);
    }
    let side = Vec3::new(-level.z, 0.0, level.x) / d;
    let fwd = level / d;
    // A widening fan, both ways, nearest the wanted direction first. Two
    // tries -- half along the wall, half toward the goal -- is a single
    // heading about twenty degrees off the wall, and in an arena of pillars
    // and doorframes it finds a lot of walls and few doorways: measured at
    // 8-20% of pressed ticks going nowhere. The last pair leans backward
    // along the wall, which is how a body gets out of a corner it has
    // walked into rather than grinding the inside of it.
    let fan: [(f32, f32); 8] = if keep != 0.0 {
        let (a, b) = (keep.signum(), -keep.signum());
        [(a, 0.35), (a, 0.0), (a, -0.35), (b, 0.35), (b, 0.0), (b, -0.35), (a, -0.9), (b, -0.9)]
    } else {
        [(1.0, 0.35), (-1.0, 0.35), (1.0, 0.0), (-1.0, 0.0), (1.0, -0.35), (-1.0, -0.35), (1.0, -0.9), (-1.0, -0.9)]
    };
    for (along, back) in fan {
        let slide = (side * along + fwd * back).normalize_or_zero();
        if clear(store, foot, slide, ahead, half_w, height) {
            return (slide * d, along);
        }
    }
    // Boxed in on every side: keep the want it came with rather than
    // answering "nowhere".
    //
    // Returning zero here looks tidy and is a trap. A want of zero is a bot
    // pressing no keys, which reads to everything downstream as a bot that
    // has chosen to stand still -- including `Duel::unjam`, whose whole job
    // is to notice a bot going nowhere and which only looks at bots that are
    // asking to move. So the one case where a bot most needs shoving out of
    // something was the one case nothing was watching. Measured: the bot
    // that reconsiders most often stood motionless for 72% of a round.
    (go, 0.0)
}

/// Would a step `ahead` metres this way put a foot in something that burns?
///
/// `footing` asks whether there is floor and `clear` whether there is a wall;
/// this asks whether the floor is a floor you want. Slag and lava are a
/// property of the material (`matlib::hurt`), so nothing here is per-world
/// and no world keeps a list of hazards.
///
/// Only ever used to refuse *entering* it. A bot already standing in the
/// stuff has to be able to walk out, and every direction out of a pool
/// crosses more of the pool, so a check that refused those too would stand
/// a bot in the slag until it died -- which, measured, is exactly what was
/// happening before this existed: an easy bot killed itself twice in a
/// two-minute match on Slagdeck with nobody shooting at it.
pub fn scalding(store: &crate::world::store::Store, foot: Vec3, dir: Vec3, ahead: f32, half_w: f32) -> bool {
    let level = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if level == Vec3::ZERO {
        return false;
    }
    crate::creatures::terrain::hurt_at(store, foot + level * ahead, half_w) > 0.0
}

/// Of the headings round the compass, the one nearest `heading` that a body
/// can take: floor under it, and no burning ground a step on.
///
/// `scalding` answers yes or no about one direction, which is enough to
/// refuse a step and not enough to decide where to go instead. Turning a
/// flat quarter turn off a refused heading, which is what the duel brain did
/// on its own, walks into the slag whenever the pool is on the inside of the
/// turn -- and when the pit is a ring, it usually is. This tries the
/// turns in order of how far they are from where the bot wanted to go, so
/// the smallest turn that is actually safe wins.
///
/// `burning` says whether a direction is refused for being on fire rather
/// than for having no floor: when a bot is already standing in the stuff
/// every way out crosses more of it, so the fire test moves to the far end
/// of the step and asks whether it *leaves* the pool instead.
pub fn aside(store: &crate::world::store::Store, foot: Vec3, heading: Vec3, ahead: f32, half_w: f32, drop: f32, standing_in_it: bool) -> Option<Vec3> {
    let level = Vec3::new(heading.x, 0.0, heading.z).normalize_or_zero();
    let h = if level == Vec3::ZERO { Vec3::NEG_Z } else { level };
    // Eighths of a turn each way, nearest first, and the way back last.
    for eighth in [1i32, -1, 2, -2, 3, -3, 4] {
        let a = eighth as f32 * std::f32::consts::FRAC_PI_4;
        let (sa, ca) = a.sin_cos();
        let d = Vec3::new(h.x * ca + h.z * sa, 0.0, -h.x * sa + h.z * ca);
        if !footing(store, foot, d, ahead, drop) {
            continue;
        }
        let out = match standing_in_it {
            // Out of the pool: the ground a stride on is what matters.
            true => crate::creatures::terrain::hurt_at(store, foot + d * ahead, half_w) <= 0.0,
            false => !scalding(store, foot, d, ahead, half_w),
        };
        if out {
            return Some(d);
        }
    }
    None
}

/// The same, with legs of uneven length so it does not read as a metronome.
///
/// A fixed period is the difference between a bot circling you and a bot
/// dancing: equal legs put it back where it started every two beats, which
/// over a whole fight is two to five metres of pacing, forever, and is the
/// single most machine-like thing a bot does. `spread` is how much the legs
/// vary, 0 for none and 1 for anything from half to one and a half of the
/// period; `seed` makes two bots differ.
///
/// Deterministic in `phase`, so there is no state to keep and two calls in
/// one step agree.
pub fn lurch(phase: f32, period: f32, spread: f32, seed: u32) -> f32 {
    let p = period.max(0.1);
    let mut t = phase.max(0.0);
    let mut leg = 0u32;
    // Walk the legs out rather than dividing, because each one is a
    // different length. A fight is seconds long, so this is a handful of
    // times round at worst.
    for _ in 0..64 {
        let r = crate::world::noise::unit(crate::world::noise::hash32(seed ^ leg.wrapping_mul(0x9e3779b9) | 1));
        let len = p * (1.0 + spread.clamp(0.0, 1.0) * (r - 0.5));
        if t < len {
            break;
        }
        t -= len;
        leg += 1;
    }
    if leg % 2 == 0 {
        1.0
    } else {
        -1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facing north, asked to go north: forward alone. Asked to go east:
    /// right alone. Asked to go back and left: both, and nothing else.
    #[test]
    fn a_direction_becomes_the_keys_a_person_would_hold() {
        // Yaw 0 faces -Z, which is the engine's north.
        let k = drive(
            0.0,
            &Want {
                go: Vec3::NEG_Z,
                ..Default::default()
            },
        );
        assert!(k.forward && !k.back && !k.left && !k.right);
        let k = drive(0.0, &Want { go: Vec3::X, ..Default::default() });
        assert!(k.right && !k.left && !k.forward && !k.back);
        let k = drive(
            0.0,
            &Want {
                go: Vec3::new(-1.0, 0.0, 1.0),
                ..Default::default()
            },
        );
        assert!(k.back && k.left && !k.forward && !k.right);
        // Turned a quarter: going east is now straight ahead.
        let k = drive(-std::f32::consts::FRAC_PI_2, &Want { go: Vec3::X, ..Default::default() });
        assert!(k.forward && !k.right, "turned east, east should be forward");
    }

    /// A want of any length presses the same keys, and a pure sidestep
    /// presses only the side.
    #[test]
    fn length_does_not_matter_and_a_sidestep_is_a_sidestep() {
        let a = drive(
            0.0,
            &Want {
                go: Vec3::NEG_Z * 0.01,
                ..Default::default()
            },
        );
        let b = drive(
            0.0,
            &Want {
                go: Vec3::NEG_Z * 90.0,
                ..Default::default()
            },
        );
        assert_eq!((a.forward, a.right), (b.forward, b.right));
        let s = drive(0.0, &Want { strafe: 1.0, ..Default::default() });
        assert!(s.right && !s.forward && !s.back && !s.left);
    }

    /// The tap is what a player's hand does: let go, press, let go, press,
    /// inside the window the body reads a dodge in. It takes a want only
    /// when it is not already mid-tap, and gives the key back afterwards.
    #[test]
    fn the_tap_presses_a_key_twice_and_gives_it_back() {
        let mut t = Tapper::default();
        let held = MoveInput {
            forward: true,
            right: true,
            ..Default::default()
        };
        // Nothing asked for: the keys pass straight through.
        assert!(t.step(None, held).right && !t.busy());
        // Asked for: four steps of tapping, up and down alternately.
        let mut rights = Vec::new();
        for i in 0..4 {
            let want = if i == 0 { Some(Dodge::Right) } else { Some(Dodge::Left) };
            // A second want mid-tap is dropped rather than starting again.
            rights.push(t.step(want, held).right);
        }
        assert_eq!(rights, vec![false, true, false, true], "the tap was {rights:?}");
        assert!(!t.busy(), "the tap outlasted its four steps");
        // The want's own keys are its own again.
        assert!(t.step(None, held).right);
        // And the keys the tap does not own are never touched.
        let mut t = Tapper::default();
        for _ in 0..4 {
            assert!(t.step(Some(Dodge::Left), held).forward, "the tap let go of a key it does not own");
        }
    }

    /// The lurch also changes legs, but not on the beat: over a long fight
    /// it spends a different amount of time each way, which is what stops it
    /// pacing back and forth across the same five metres forever.
    #[test]
    fn the_lurch_is_not_a_metronome() {
        let steps: Vec<f32> = (0..1200).map(|i| lurch(i as f32 * 0.05, 1.1, 0.8, 7)).collect();
        assert!(steps.iter().any(|&s| s > 0.0) && steps.iter().any(|&s| s < 0.0), "it never turned round");
        // Leg lengths differ: count how many steps each run lasts and check
        // they are not all the same.
        let mut runs = Vec::new();
        let mut n = 1;
        for w in steps.windows(2) {
            if w[0] == w[1] {
                n += 1;
            } else {
                runs.push(n);
                n = 1;
            }
        }
        assert!(runs.len() > 8, "only {} legs in a minute", runs.len());
        let first = runs[0];
        assert!(runs.iter().any(|&r| r != first), "every leg is the same length: {runs:?}");
        // Two bots do not dance together.
        let other: Vec<f32> = (0..1200).map(|i| lurch(i as f32 * 0.05, 1.1, 0.8, 19)).collect();
        let same = steps.iter().zip(&other).filter(|(a, b)| a == b).count();
        assert!(same < 1100, "two bots with different seeds weaved in lockstep");
    }
}
