//! The benchmark routes (--bench, --bench-route), windowed or headless.

use crate::game::Game;
use glam::Vec3;

/// Which way the camera goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// From the spawn point inland, a climb, and a pass along the coast: fast,
    /// high and streaming all the way.
    Fly,
    /// Once round a loop 50 m across at eye height and a jog's pace, starting
    /// at the spawn (or `--ground`), looking about as it goes. On Hemlow's
    /// spawn that is the West Field, with sheep, horses and cattle round it:
    /// the creatures, grass and sound that a flight high over them misses.
    Field,
}

impl Route {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fly" => Some(Route::Fly),
            "field" => Some(Route::Field),
            _ => None,
        }
    }
}

/// A fixed camera route and the frame times along it.
pub struct Bench {
    pub route: Route,
    pub t: f32,
    pub start: Vec3,
    pub times: Vec<f32>,
    /// The step the world moved by each frame, ms (windowed only; headless
    /// moves by a fixed step). Under vsync this is what the motion follows, so
    /// how much it changes frame to frame is how much the motion judders.
    pub steps: Vec<f32>,
    pub duration: f32,
    pub loading_frames: u32,
    pub max_queue: usize,
}

impl Bench {
    pub fn new(start: Vec3, route: Route) -> Self {
        Self {
            route,
            t: 0.0,
            start,
            times: Vec::new(),
            steps: Vec::new(),
            duration: 30.0,
            loading_frames: 0,
            max_queue: 0,
        }
    }
    pub fn camera(&self, game: &Game) -> (Vec3, f32, f32) {
        let t = self.t / self.duration;
        if self.route == Route::Field {
            return self.field(game, t);
        }
        let w = game.generator().spec.size_m;
        let a = self.start;
        let centre = Vec3::new(w * 0.45, a.y + 25.0, w * 0.5);
        // Out toward the middle of the island, then circle back along the coast.
        let p = if t < 0.4 {
            a.lerp(centre, t / 0.4) + Vec3::Y * 3.0
        } else {
            let u = (t - 0.4) / 0.6;
            let ang = u * std::f32::consts::TAU * 0.6;
            let r = w * 0.32;
            centre + Vec3::new(ang.cos() * r * 0.9, -10.0 + 8.0 * (ang * 2.0).sin(), ang.sin() * r)
        };
        let ahead = if t < 0.4 { centre - a } else { Vec3::new(-(t * 10.0).sin(), -0.2, (t * 10.0).cos()) };
        let yaw = (-ahead.x).atan2(-ahead.z);
        (p, yaw, -0.18)
    }
    /// The field loop at `t` (0 to 1 over the run).
    fn field(&self, game: &Game, t: f32) -> (Vec3, f32, f32) {
        const R: f32 = 25.0;
        let ang = t * std::f32::consts::TAU;
        let a = self.start;
        // Round a centre R east of the start.
        let (x, z) = (a.x + R - R * ang.cos(), a.z - R * ang.sin());
        let y = game.generator().ground_under(x, z) + 1.7;
        let along = Vec3::new(ang.sin(), 0.0, -ang.cos());
        // Look from side to side as it goes, about 35 degrees either way.
        let sway = 0.6 * (ang * 5.0).sin();
        let yaw = (-along.x).atan2(-along.z) + sway;
        (Vec3::new(x, y, z), yaw, -0.06)
    }

    pub fn report(&self) -> serde_json::Value {
        let mut v = self.times.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len().max(1);
        let pct = |p: f32| v.get(((n as f32 * p) as usize).min(n.saturating_sub(1))).copied().unwrap_or(0.0);
        let avg = v.iter().sum::<f32>() / n as f32;
        let mut out = serde_json::json!({
            "frames": v.len(),
            "avg_ms": avg,
            "p50_ms": pct(0.5),
            "p95_ms": pct(0.95),
            "p99_ms": pct(0.99),
            "max_ms": v.last().copied().unwrap_or(0.0),
            "over_16ms": v.iter().filter(|&&t| t > 16.0).count(),
            "frames_still_loading": self.loading_frames,
            "max_queue": self.max_queue,
        });
        if self.steps.len() > 1 {
            // The mean change in step from one frame to the next: 0 is
            // perfectly even motion.
            let jumps: Vec<f32> = self.steps.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            let mut s = self.steps.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let m = s.len();
            out["step_p05_ms"] = s[m / 20].into();
            out["step_p95_ms"] = s[(m * 19 / 20).min(m - 1)].into();
            out["step_jump_ms"] = (jumps.iter().sum::<f32>() / jumps.len() as f32).into();
        }
        out
    }
}
