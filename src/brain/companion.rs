//! A survivor on the player's side in the zombie round: keeps up, and shoots
//! what comes at the party.
//!
//! Two orders for now, from the rules (`shooter::zombies`): follow the player
//! (`Goal::Follow`), or hold a spot (`Goal::Hold`). It has rounds to spare
//! (the rules top it up), so what keeps it from emptying a rifle at every
//! zombie on the hill is its own restraint: it only shoots what is close or
//! coming at the party, in short bursts with a breath between them, and it
//! leaves the far ones that have not noticed anybody.

use super::{aim, senses, steer, Brain, Ctx, Foe, Goal, Me, Pilot};
use crate::combat::weapon::Trigger;
use glam::Vec3;

/// Metres inside which anything gets shot, whatever it is doing.
const CLOSE_M: f32 = 14.0;
/// Metres inside which something walking toward the party gets shot.
const COMING_M: f32 = 32.0;
/// Seconds of trigger in a burst, and of rest after it.
const BURST_S: f32 = 0.16;
const REST_S: f32 = 0.42;
/// How near the aim has to be before it fires, radians; at range, also
/// within `ON_TARGET_M` of the target, so it does not spend rounds on a
/// zombie it is likely to miss.
const ON_TARGET: f32 = 0.07;
const ON_TARGET_M: f32 = 0.5;
/// Past `CLOSE_M` it fires shorter bursts with longer rests between them:
/// about 30% fewer rounds than up close.
const FAR_BURST_S: f32 = 0.12;
const FAR_REST: f32 = 1.15;
/// How often it looks for something to shoot, seconds.
const LOOK_S: f32 = 0.25;
/// Metres it keeps from anybody else in the party, pushing off like a boid.
const SPACE_M: f32 = 2.6;
/// Metres either side of its line of fire nobody in the party may stand.
const CLEAR_M: f32 = 1.0;
/// Which way each of the party watches, radians off the way the party is
/// going: the first behind, the others out to the left and right. The
/// player looks ahead.
const WATCH: [f32; 3] = [std::f32::consts::PI, -1.75, 1.75];

pub struct Companion {
    seed: u32,
    /// Its arc to watch (`WATCH`).
    watch: f32,
    /// The way the player was last going.
    heading: Vec3,
    /// Where in the party it walks: an angle behind the player and a reach.
    slot: (f32, f32),
    target: Option<usize>,
    look_due: f32,
    /// Seconds into the current burst or rest; positive firing, negative resting.
    burst: f32,
    /// The error on this burst, radians, re-rolled each time.
    wobble: (f32, f32),
    keep: f32,
    last: Vec3,
    still: f32,
    doing: &'static str,
}

impl Companion {
    /// `place` says which of the party it is (0, 1, 2): each walks at its own
    /// spot behind the player so they do not all queue down one line.
    pub fn new(seed: u32, place: usize) -> Companion {
        let angle = [0.0, 0.7, -0.7, 1.3, -1.3][place % 5];
        Companion {
            seed,
            watch: WATCH[place % WATCH.len()],
            heading: Vec3::Z,
            slot: (angle, 3.5 + (place % 3) as f32 * 0.8),
            target: None,
            look_due: 0.0,
            burst: -REST_S,
            wobble: (0.0, 0.0),
            keep: 0.0,
            last: Vec3::ZERO,
            still: 0.0,
            doing: "follow",
        }
    }

    fn roll(&mut self) -> f32 {
        self.seed = crate::world::noise::hash32(self.seed) | 1;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    /// The way its arc faces, level.
    fn arc(&self) -> Vec3 {
        let (s, c) = self.watch.sin_cos();
        let h = self.heading;
        Vec3::new(h.x * c - h.z * s, 0.0, h.x * s + h.z * c)
    }

    /// The zombie most worth shooting now: the nearest one it can see that
    /// is close, or coming at somebody in the party, with the ones in its
    /// own arc counted nearer than they are.
    fn pick(&self, me: &Me, cx: &Ctx, party: &[Foe]) -> Option<usize> {
        let arc = self.arc();
        let mut best: Option<(f32, usize)> = None;
        for f in cx.fighters.iter().filter(|f| f.alive && !f.friend_of(me.team)) {
            let d = (f.foot - me.foot).length();
            if d > COMING_M.max(me.person.sight.min(60.0)) {
                continue;
            }
            // Close to anybody in the party, or walking toward the nearest of us.
            let near = party.iter().map(|p| (p.foot - f.foot).length()).fold(f32::INFINITY, f32::min);
            let coming = party.iter().any(|p| {
                let to = p.foot - f.foot;
                to.length() < COMING_M && f.vel.dot(to.normalize_or_zero()) > 0.8
            });
            if near > CLOSE_M && !coming {
                continue;
            }
            if !senses::can_see(cx.store, me.eye, Vec3::new(0.0, 0.0, -1.0), &super::person::Personality { peripheral: -1.0, ..*me.person }, f.chest()) {
                continue;
            }
            let toward = Vec3::new(f.foot.x - me.foot.x, 0.0, f.foot.z - me.foot.z).normalize_or_zero();
            let score = near.min(d) * if toward.dot(arc) > 0.3 { 0.5 } else { 1.0 };
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, f.who));
            }
        }
        best.map(|(_, w)| w)
    }
}

impl Brain for Companion {
    fn think(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32) {
        let party: Vec<Foe> = cx.fighters.iter().filter(|f| f.alive && f.friend_of(me.team)).copied().collect();
        let leader = party.iter().find(|f| f.who == 0).copied();
        let pace = leader.map_or(0.0, |l| l.vel.x.hypot(l.vel.z));
        if let Some(l) = leader.filter(|_| pace > 0.6) {
            self.heading = Vec3::new(l.vel.x, 0.0, l.vel.z).normalize();
        }

        // Something to shoot.
        self.look_due -= dt;
        if self.look_due <= 0.0 {
            self.look_due = LOOK_S;
            self.target = self.pick(me, cx, &party);
        }
        let target = self.target.and_then(|w| cx.fighters.get(w)).filter(|f| f.alive);

        // Where to stand.
        let (spot, hold) = match me.order.map(|o| o.goal) {
            Some(Goal::Hold { post, .. }) => (Some(post), true),
            // A point on the player's trail (the rules pick it).
            Some(Goal::Point(p)) => (Some(p), false),
            Some(Goal::Follow(w)) => (
                cx.fighters.get(w).filter(|f| f.alive).map(|f| {
                    // Behind them, at its own angle: behind is the way they
                    // came, which is where their velocity points from, or
                    // straight back down their facing when they stand still.
                    let back = if f.vel.length() > 0.5 { -f.vel.normalize() } else { (me.foot - f.foot).normalize_or(Vec3::Z) };
                    let (s, c) = self.slot.0.sin_cos();
                    let dir = Vec3::new(back.x * c - back.z * s, 0.0, back.x * s + back.z * c);
                    f.foot + dir * self.slot.1
                }),
                false,
            ),
            _ => (None, false),
        };
        let mut go = spot.map_or(Vec3::ZERO, |p| Vec3::new(p.x - me.foot.x, 0.0, p.z - me.foot.z));
        let far = go.length();
        let near_enough = if hold { 1.5 } else { 1.2 };
        if far < near_enough {
            go = Vec3::ZERO;
        }
        // Boids: push off anybody in the party who is too close.
        let mut apart = Vec3::ZERO;
        for f in party.iter().filter(|f| f.who != me.who) {
            let d = Vec3::new(me.foot.x - f.foot.x, 0.0, me.foot.z - f.foot.z);
            let n = d.length();
            if n < SPACE_M {
                let away = if n > 0.05 { d / n } else { Vec3::new((me.who as f32).cos(), 0.0, (me.who as f32).sin()) };
                apart += away * (SPACE_M - n) / SPACE_M;
            }
        }
        if apart.length() > 0.15 {
            go = go.normalize_or_zero() * go.length().min(4.0) + apart * 3.0;
        }
        // Keeping pace: run when it is falling behind or the player runs.
        let sprint = far > 4.5 || (pace > 3.2 && far > 1.5);

        // Stuck: hop and try the other side of whatever it is.
        let moved = (me.foot - self.last).length();
        self.still = if go != Vec3::ZERO && moved < 0.04 { self.still + dt } else { 0.0 };
        self.last = me.foot;
        let mut jump = false;
        if self.still > 1.0 {
            self.still = 0.0;
            self.keep = if self.keep >= 0.0 { -1.0 } else { 1.0 };
            jump = true;
        }
        if go != Vec3::ZERO {
            let (way, side) = steer::around_keeping(cx.store, me.foot, go, 0.9, me.half_w, me.height, self.keep);
            if side != 0.0 {
                self.keep = side;
            }
            go = way;
            let level = go.normalize_or_zero();
            let ahead = me.foot + level * 0.7;
            if me.on_ground && crate::physics::hits(cx.store, ahead + Vec3::Y * 0.45, me.half_w, 0.05) && !crate::physics::hits(cx.store, ahead + Vec3::Y * 1.3, me.half_w, 0.05) {
                jump = true;
            }
        }

        // Where to look: the target, else the way it is going, else along
        // the player's view.
        let rate = aim::turn_rate(me.person);
        let mut trigger = Trigger::Off;
        if let Some(t) = target {
            self.doing = "shoot";
            let want = aim::look_at(t.chest() - me.eye);
            let want = (want.0 + self.wobble.0, want.1 + self.wobble.1);
            aim::turn_head(&mut out.yaw, &mut out.pitch, want, rate, dt);
            let off = crate::util::wrap_angle(want.0 - out.yaw).abs() + (want.1 - out.pitch).abs();
            // Bursts with a breath between them; shorter and further apart
            // at range.
            let range = (t.foot - me.foot).length();
            let far = range > CLOSE_M;
            if self.burst >= 0.0 {
                self.burst += dt;
                if self.burst > if far { FAR_BURST_S } else { BURST_S } {
                    self.burst = -REST_S * (0.8 + self.roll().abs()) * if far { FAR_REST } else { 1.0 };
                }
            } else {
                self.burst += dt;
                if self.burst >= 0.0 {
                    let spread = (3.6 - 3.2 * me.person.aim).max(0.3) * 0.012;
                    self.wobble = (self.roll() * spread, self.roll() * spread * 0.6);
                }
            }
            // Never past one of the party: anybody on our side nearer than
            // the target with any of them, foot to head, within a metre of
            // the line. Blocked, it steps sideways for a clear one.
            let line = t.chest() - me.eye;
            let len = line.length().max(0.01);
            let dir = line / len;
            let mut blocked = None;
            for f in party.iter().filter(|f| f.who != me.who) {
                for h in [0.3, 0.9, 1.5, 1.8] {
                    let v = f.foot + Vec3::Y * h - me.eye;
                    let along = v.dot(dir);
                    if along > -0.3 && along < len && (v - dir * along).length() < CLEAR_M {
                        blocked = Some(f.foot);
                    }
                }
            }
            if let Some(p) = blocked {
                let side = Vec3::new(-dir.z, 0.0, dir.x);
                let s = if side.dot(me.foot - p) >= 0.0 { 1.0 } else { -1.0 };
                go += side * s * 2.0;
            }
            let tolerance = ON_TARGET.min(ON_TARGET_M / range.max(1.0));
            if self.burst >= 0.0 && off < tolerance && blocked.is_none() {
                trigger = Trigger::Fire;
            }
        } else {
            // Nothing to shoot: it watches its own arc, the rear guard
            // behind the party and the others out to the sides.
            self.doing = if hold { "hold" } else { "follow" };
            // Far behind, it looks where it runs, to catch up.
            let want = aim::look_at(if far > 4.5 && go != Vec3::ZERO { go } else { self.arc() });
            aim::turn_head(&mut out.yaw, &mut out.pitch, (want.0, 0.0), rate, dt);
        }

        let w = steer::Want {
            go,
            sprint: sprint && target.is_none(),
            jump,
            ..Default::default()
        };
        out.mv = if target.is_some() { steer::drive(out.yaw, &w) } else { steer::drive_running(out.yaw, &w, 0.5) };
        out.trigger = trigger;
    }

    fn doing(&self) -> &'static str {
        self.doing
    }
}
