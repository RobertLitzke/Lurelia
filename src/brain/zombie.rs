//! A zombie: walks where it is sent and turns to face whoever it is after.
//!
//! It decides almost nothing. The zombie round's director (`shooter::zombies`)
//! works out where every zombie should be heading from a flow field over the
//! world survey and hands it over as an order: a point to make for, how hard
//! to go (`urgency`), and the survivor it is after (`mark`). This only turns
//! that into keys, with the gait of a body that is not quite working: it
//! keeps going at the pace the rules give its body, slides along walls, and
//! hops what is knee high.
//!
//! The blow itself is the rules' (`zombies::strikes`), as a pad is the rules'
//! and not the bot's: a zombie that is close enough and facing its mark hits.

use super::{steer, Brain, Ctx, Goal, Me, Pilot};
use glam::Vec3;

/// Below this urgency a zombie mills about; above `RUN` it runs.
pub const IDLE: f32 = 0.3;
pub const RUN: f32 = 0.75;

pub struct Zombie {
    seed: u32,
    /// Seconds into its gait, which sets when its legs are going.
    phase: f32,
    /// Which side of a wall it slid along last (`steer::around_keeping`).
    keep: f32,
    /// Where it was a moment ago, and how long it has been there: a zombie
    /// that has not got anywhere in a couple of seconds jumps and tries the
    /// other side.
    last: Vec3,
    still: f32,
    unstick: f32,
    /// Where an idle one is drifting, and for how long more.
    drift: Vec3,
    drift_t: f32,
    doing: &'static str,
}

impl Zombie {
    pub fn new(seed: u32) -> Zombie {
        Zombie {
            seed,
            phase: (seed % 977) as f32 * 0.013,
            keep: 0.0,
            last: Vec3::ZERO,
            still: 0.0,
            unstick: 0.0,
            drift: Vec3::ZERO,
            drift_t: 0.0,
            doing: "idle",
        }
    }

    fn roll(&mut self) -> f32 {
        self.seed = crate::world::noise::hash32(self.seed) | 1;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

impl Brain for Zombie {
    fn think(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32) {
        self.phase += dt;
        let order = me.order;
        let urgency = order.map_or(0.0, |o| o.urgency);
        let target = match order.map(|o| o.goal) {
            Some(Goal::Point(p)) => Some(p),
            _ => None,
        };
        // The survivor it is after, to face when it is close.
        let mark = order.and_then(|o| o.mark).and_then(|w| cx.fighters.get(w)).filter(|f| f.alive);

        let mut go = match target {
            Some(p) if urgency >= IDLE => {
                self.doing = if urgency >= RUN { "run" } else { "chase" };
                p - me.foot
            }
            _ => {
                // Milling about: a few metres one way, a pause, another way.
                self.doing = "idle";
                self.drift_t -= dt;
                if self.drift_t <= 0.0 {
                    let a = self.roll() * std::f32::consts::TAU;
                    let stop = self.roll() < 0.4;
                    self.drift = if stop { Vec3::ZERO } else { Vec3::new(a.cos(), 0.0, a.sin()) };
                    self.drift_t = 2.0 + self.roll() * 4.0;
                }
                self.drift
            }
        };
        go.y = 0.0;
        let far = go.length();
        if far < 0.6 && self.doing != "idle" {
            go = Vec3::ZERO;
        }

        // Stuck against something: jump, and for a moment go the other way
        // along it.
        let moved = (me.foot - self.last).length();
        if go != Vec3::ZERO && moved < 0.05 * dt * 60.0 {
            self.still += dt;
        } else {
            self.still = 0.0;
        }
        self.last = me.foot;
        let mut jump = false;
        if self.still > 1.5 {
            self.still = 0.0;
            self.unstick = 1.0;
            self.keep = if self.keep >= 0.0 { -1.0 } else { 1.0 };
            jump = true;
        }
        self.unstick = (self.unstick - dt).max(0.0);

        if go != Vec3::ZERO {
            let (way, side) = steer::around_keeping(cx.store, me.foot, go, 0.9, me.half_w, me.height, self.keep);
            if side != 0.0 {
                self.keep = side;
            }
            go = way;
            // A kerb, a step, a fallen log: knee high and clear above it.
            let level = Vec3::new(go.x, 0.0, go.z).normalize_or_zero();
            let ahead = me.foot + level * 0.7;
            let low = crate::physics::hits(cx.store, ahead + Vec3::Y * 0.45, me.half_w, 0.05);
            let high = crate::physics::hits(cx.store, ahead + Vec3::Y * 1.3, me.half_w, 0.05);
            if low && !high && me.on_ground {
                jump = true;
            }
        }

        // Facing: its mark when that is near, else where it is going.
        let face = match mark {
            Some(f) if (f.foot - me.foot).length() < 6.0 => Some(f.chest() - me.eye),
            _ => (go != Vec3::ZERO).then_some(go),
        };
        if let Some(d) = face {
            let (yaw, _) = super::aim::look_at(d);
            let mut pitch = out.pitch;
            let mut y = out.yaw;
            super::aim::turn_head(&mut y, &mut pitch, (yaw, 0.0), 4.0, dt);
            out.yaw = y;
            out.pitch = pitch;
        }

        // The gait. After somebody, its legs keep going: how fast it goes is
        // its body's pace, which the rules set (`zombies::CHASE_MPS`), and
        // the shamble is the figure's. Milling about, it stops and starts.
        let duty = if urgency >= IDLE { 1.0 } else { 0.5 };
        let stride = 1.1 + (self.seed % 7) as f32 * 0.07;
        let legs = (self.phase / stride).fract() < duty;
        let want = steer::Want {
            go: if legs || jump { go } else { Vec3::ZERO },
            jump,
            ..Default::default()
        };
        out.mv = steer::drive(out.yaw, &want);
        out.trigger = Default::default();
    }

    fn doing(&self) -> &'static str {
        self.doing
    }
}
