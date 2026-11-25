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
    /// A race on autopilot (`--bench-route race`, windowed): the camera is
    /// the race's own, a bot drives the player's kart, and the run ends when
    /// everyone is home. For the hitches that only show in a race.
    Race,
}

impl Route {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fly" => Some(Route::Fly),
            "field" => Some(Route::Field),
            "race" => Some(Route::Race),
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
    /// Whether the race's autopilot has been switched on (`Route::Race`).
    pub piloted: bool,
    /// Each windowed frame's work and wait for the display, ms.
    pub work: Vec<(f32, f32)>,
    /// The slowest frames' work, wait and parts (`trace::line`), kept as they
    /// come so a long run doesn't hold a line for every frame.
    pub worst: Vec<(f32, f32, String)>,
}

/// Slow frames kept with their parts.
const WORST: usize = 16;

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
            piloted: false,
            work: Vec::new(),
            worst: Vec::new(),
        }
    }

    /// A windowed frame's work, its wait for the display and its parts.
    pub fn frame(&mut self, work: f32, wait: f32, parts: impl FnOnce() -> String) {
        self.work.push((work, wait));
        let total = work + wait;
        if self.worst.len() < WORST || total > self.worst.last().map_or(0.0, |w| w.0 + w.1) {
            self.worst.push((work, wait, parts()));
            self.worst.sort_by(|a, b| (b.0 + b.1).total_cmp(&(a.0 + a.1)));
            self.worst.truncate(WORST);
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
        if !self.work.is_empty() {
            let mut w: Vec<f32> = self.work.iter().map(|f| f.0).collect();
            w.sort_by(|a, b| a.total_cmp(b));
            let m = w.len();
            let med = w[m / 2];
            out["work_p50_ms"] = med.into();
            out["work_p99_ms"] = w[(m * 99 / 100).min(m - 1)].into();
            // A frame with twice the usual work shows as a hitch at any frame
            // rate, though at 140 fps it never reaches the 16 ms of `trace`.
            out["work_over_2x_median"] = w.iter().filter(|&&x| x > 2.0 * med).count().into();
            // Frames held up by the display or the GPU rather than by work.
            out["wait_over_8ms"] = self.work.iter().filter(|f| f.1 > 8.0).count().into();
            out["frames_over_12ms"] = self.work.iter().filter(|f| f.0 + f.1 > 12.0).count().into();
            out["worst"] = self
                .worst
                .iter()
                .map(|(work, wait, l)| format!("{work:.1} work {wait:.1} wait: {l}"))
                .collect::<Vec<_>>()
                .into();
        }
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
