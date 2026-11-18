//! Where a bot points the gun, and how wrong it is.
//!
//! Error is a product of multipliers over a base, and the multiplier that
//! matters is the first one:
//!
//! ```text
//! error *= 11 - 10 * dot(dir_to_target, dir_to_where_it_will_be)
//! ```
//!
//! Error grows with how fast the target crosses the shooter's view. That one
//! line is why strafing works, why standing still in the open is fatal, and
//! why a bot that is wrong about your movement misses in a way that reads as
//! being outplayed rather than as a dice roll.
//!
//! The rest: a freshly acquired target is aimed at about twice as badly for
//! the first second or so (reaction time as settling, not as a timer), and a
//! bot that has just been hit, or that is in the air, or whose target is in
//! the air, aims worse again.

use super::person::Personality;
use glam::Vec3;

/// The base error, degrees, before any of the multipliers. A ray needs a
/// smaller one than a shell: a rocket that misses by two degrees still lands
/// near enough to hurt, a bullet does not.
pub fn base_deg(hitscan: bool) -> f32 {
    if hitscan {
        2.2
    } else {
        4.5
    }
}

/// Where the target will be shortly, allowing for both bodies' motion: the
/// vector the crossing factor is measured against.
///
/// The target's position a second and a quarter along its current course,
/// seen from where the shooter is heading. It is deliberately not the shooter's lead solution -- it is a
/// statement about how hard the target is to track, not about where the shot
/// is going, and the two are different the moment a ray is fired, because a
/// ray is never led.
pub fn ahead(from: Vec3, from_vel: Vec3, at: Vec3, at_vel: Vec3) -> Vec3 {
    ((at + at_vel * 1.25) - (from + from_vel)).normalize_or(Vec3::NEG_Z)
}

/// How wrong this shot is, in radians of half-angle.
///
/// `to_target` and `ahead` are both unit: where the target is, and where it
/// is going (`ahead` above). `held` is how long this run of seeing it has
/// lasted.
#[allow(clippy::too_many_arguments)]
pub fn error_rad(p: &Personality, hitscan: bool, to_target: Vec3, ahead: Vec3, held: f32, hurt_recently: bool, airborne: bool, roll: f32) -> f32 {
    let mut err = base_deg(hitscan);
    // Crossing the view is what makes a target hard. One coming straight at
    // you is dot 1 and costs nothing; one crossing fast pushes the dot down
    // and the error up sharply, and because the angle it crosses through
    // grows as it gets nearer, this is also what makes a close fight hard.
    //
    // Measured against where the target is going, never against the
    // shooter's lead solution: a ray is not led, so `to_lead` there is
    // literally `to_target`, the dot is exactly 1, and the whole term
    // silently does nothing for every bullet in the game. That was the bug
    // that made every difficulty an aimbot inside ten metres.
    err *= 11.0 - 10.0 * to_target.dot(ahead).clamp(-1.0, 1.0);
    // Skill, with a roll in it so two shots at the same target differ.
    //
    // The span matters more than the shape. At 2.2 degrees of base, a cone
    // of six degrees at ten metres is about a fifth of the shots on a body
    // and one of three at four; a cone of two and a half is nearly all of
    // them. So this runs from about 3.4x at the bottom to 0.25x at the top,
    // and `easy` lands near six degrees deliberately: a bot that hits a
    // walking man one time in five with a minigun is a bot a person can
    // walk away from.
    err *= (3.6 - 3.2 * (p.aim + roll * 0.15)).max(0.25);
    // It has not had time to find the target yet.
    if held < p.settle {
        err *= 1.0 + (1.0 - held / p.settle.max(0.01));
    }
    if hurt_recently {
        err *= 1.25;
    }
    if airborne {
        err *= 1.4;
    }
    err.to_radians()
}

/// Where to point so a shell of `speed` arrives where the target is going.
/// A single step of iteration is plenty: the flight time barely changes.
/// `gravity` is metres per second squared of drop, 0 for a rocket.
pub fn lead(from: Vec3, at: Vec3, vel: Vec3, speed: f32, gravity: f32) -> Vec3 {
    if speed <= 0.0 {
        return (at - from).normalize_or(Vec3::NEG_Z);
    }
    let mut aim = at;
    for _ in 0..2 {
        let t = (aim - from).length() / speed;
        aim = at + vel * t + Vec3::Y * 0.5 * gravity * t * t;
    }
    (aim - from).normalize_or(Vec3::NEG_Z)
}

/// Yaw and pitch that point along `dir`, in the player's own convention
/// (`Player::forward`).
pub fn look_at(dir: Vec3) -> (f32, f32) {
    let d = dir.normalize_or(Vec3::NEG_Z);
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

/// Turn the head toward `want` at `rate` radians a second, the long way
/// round never taken. A bot that snaps its view is the tell that gives the
/// whole thing away, so even the best one turns at a speed.
pub fn turn_head(yaw: &mut f32, pitch: &mut f32, want: (f32, f32), rate: f32, dt: f32) {
    let step = rate * dt;
    let dy = crate::util::wrap_angle(want.0 - *yaw);
    *yaw += dy.clamp(-step, step);
    let dp = want.1 - *pitch;
    *pitch = (*pitch + dp.clamp(-step, step)).clamp(-1.5, 1.5);
}

/// How fast a bot of this skill turns its head, radians a second.
///
/// The old floor of 3 rad/s was 172 deg/s, and a man strafing at 6 m/s only
/// crosses 49 deg/s at seven metres, so every bot could hold its view on
/// anything and only the cone of error mattered. These still beat a strafing
/// target at ordinary ranges and give way at close quarters, which is where
/// a head that cannot keep up should show.
///
/// It is coupled to the rate of fire: `duel::shoot` only pulls the trigger
/// within about ten degrees of the target, so a bot that cannot turn fast
/// enough stops shooting rather than shooting wide. That is the mechanism
/// this is meant to use, and also the one it could overshoot into.
pub fn turn_rate(p: &Personality) -> f32 {
    1.8 + 5.2 * p.aim
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::person::level;

    /// A target crossing the view is harder to hit than one standing still,
    /// and a better bot is better at both.
    #[test]
    fn crossing_the_view_is_what_makes_it_hard() {
        let easy = level("easy").unwrap();
        let good = level("masterful").unwrap();
        let ahead = Vec3::NEG_Z;
        // Thirty-seven degrees off: a man sprinting across at ten metres.
        let across = Vec3::new(0.6, 0.0, -0.8).normalize();
        let still = error_rad(&easy, true, ahead, ahead, 9.0, false, false, 0.5);
        let moving = error_rad(&easy, true, ahead, across, 9.0, false, false, 0.5);
        assert!(moving > still * 2.5, "crossing cost almost nothing: {still} -> {moving}");
        let better = error_rad(&good, true, ahead, across, 9.0, false, false, 0.5);
        assert!(better < moving * 0.6, "skill bought nothing: {moving} -> {better}");
    }

    /// The bug that made every difficulty an aimbot: a ray is never led, so
    /// the crossing factor used to be measured against a vector identical to
    /// `to_target`, came out as exactly 1, and cost nothing. Measured
    /// against where the target is going, a strafing man at seven metres is
    /// several times harder than a standing one, for every skill.
    #[test]
    fn a_strafing_target_is_hard_at_close_range() {
        let eye = Vec3::ZERO;
        for name in ["novice", "easy", "average", "skilled", "godlike"] {
            let p = level(name).unwrap();
            let at = Vec3::new(0.0, 0.0, -7.0);
            let to_target = at.normalize();
            let still = error_rad(&p, true, to_target, ahead(eye, Vec3::ZERO, at, Vec3::ZERO), 9.0, false, false, 0.5);
            let strafing = error_rad(&p, true, to_target, ahead(eye, Vec3::ZERO, at, Vec3::X * 6.0), 9.0, false, false, 0.5);
            assert!(strafing > still * 3.0, "{name}: strafing cost almost nothing at 7 m, {still} -> {strafing}");
        }
    }

    /// And the same movement at thirty metres barely matters, because the
    /// angle it crosses through is small. This is the half that stops the
    /// fix from making bots useless at range.
    #[test]
    fn the_same_strafe_is_cheap_far_away() {
        let p = level("average").unwrap();
        let eye = Vec3::ZERO;
        let at = Vec3::new(0.0, 0.0, -30.0);
        let to_target = at.normalize();
        let still = error_rad(&p, true, to_target, ahead(eye, Vec3::ZERO, at, Vec3::ZERO), 9.0, false, false, 0.5);
        let strafing = error_rad(&p, true, to_target, ahead(eye, Vec3::ZERO, at, Vec3::X * 6.0), 9.0, false, false, 0.5);
        assert!(strafing < still * 1.6, "a far strafe should be cheap: {still} -> {strafing}");
    }

    /// A bot shooting on the move aims worse than one that has stopped,
    /// because its own motion is in the vector too.
    #[test]
    fn a_bot_on_the_move_aims_worse() {
        let p = level("skilled").unwrap();
        let eye = Vec3::ZERO;
        let at = Vec3::new(0.0, 0.0, -12.0);
        let to_target = at.normalize();
        let planted = error_rad(&p, true, to_target, ahead(eye, Vec3::ZERO, at, Vec3::ZERO), 9.0, false, false, 0.5);
        let running = error_rad(&p, true, to_target, ahead(eye, Vec3::X * 6.0, at, Vec3::ZERO), 9.0, false, false, 0.5);
        assert!(running > planted * 1.5, "its own motion cost nothing: {planted} -> {running}");
    }

    /// A turning head must still beat an ordinary strafing target at
    /// ordinary range, or a bot stops firing altogether: the trigger is
    /// gated on facing within about ten degrees.
    #[test]
    fn even_a_novice_can_track_at_ordinary_range() {
        // A man at 6 m/s across the view at ten metres: 34 deg/s.
        let need = (6.0f32 / 10.0).atan();
        for name in crate::brain::person::NAMES {
            let p = level(name).unwrap();
            assert!(turn_rate(&p) > need * 1.5, "{name} cannot keep up with a strafing man at ten metres");
        }
    }

    /// A freshly seen target is aimed at worse, and settling helps.
    #[test]
    fn a_fresh_target_is_aimed_at_badly() {
        let p = level("average").unwrap();
        let ahead = Vec3::NEG_Z;
        let fresh = error_rad(&p, true, ahead, ahead, 0.0, false, false, 0.5);
        let settled = error_rad(&p, true, ahead, ahead, 9.0, false, false, 0.5);
        assert!(fresh > settled * 1.9, "settling did nothing: {fresh} -> {settled}");
    }

    /// A lead is ahead of a crossing target, and the solution converges on
    /// the point the shell will actually meet.
    #[test]
    fn the_lead_is_ahead_of_a_crossing_target() {
        let from = Vec3::ZERO;
        let at = Vec3::new(0.0, 0.0, -30.0);
        let vel = Vec3::new(8.0, 0.0, 0.0);
        let dir = lead(from, at, vel, 30.0, 0.0);
        assert!(dir.x > 0.2, "it aimed at where he was: {dir:?}");
        // The shell and the man should arrive together, within a body width.
        let t = 30.0 / 30.0;
        let shell = from + dir * 30.0 * t;
        let man = at + vel * t;
        assert!((shell - man).length() < 1.5, "missed by {} m", (shell - man).length());
    }

    /// Pointing at something and reading the angles back agree.
    #[test]
    fn look_at_round_trips() {
        for d in [Vec3::NEG_Z, Vec3::X, Vec3::new(0.4, 0.3, -0.8).normalize()] {
            let (yaw, pitch) = look_at(d);
            let (sy, cy) = yaw.sin_cos();
            let (sp, cp) = pitch.sin_cos();
            let back = Vec3::new(-sy * cp, sp, -cy * cp);
            assert!((back - d).length() < 1e-4, "{d:?} -> {back:?}");
        }
    }
}
