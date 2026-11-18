//! The benchmark flight (--bench), windowed or headless.

use crate::game::Game;
use glam::Vec3;

/// A fixed fly-through: from the spawn point inland, a climb, and a pass along the coast.
pub struct Bench {
    pub t: f32,
    pub start: Vec3,
    pub times: Vec<f32>,
    pub duration: f32,
    pub loading_frames: u32,
    pub max_queue: usize,
}

impl Bench {
    pub fn new(start: Vec3) -> Self {
        Self {
            t: 0.0,
            start,
            times: Vec::new(),
            duration: 30.0,
            loading_frames: 0,
            max_queue: 0,
        }
    }
    pub fn camera(&self, game: &Game) -> (Vec3, f32, f32) {
        let w = game.generator().spec.size_m;
        let t = self.t / self.duration;
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
    pub fn report(&self) -> serde_json::Value {
        let mut v = self.times.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len().max(1);
        let pct = |p: f32| v.get(((n as f32 * p) as usize).min(n.saturating_sub(1))).copied().unwrap_or(0.0);
        let avg = v.iter().sum::<f32>() / n as f32;
        serde_json::json!({
            "frames": v.len(),
            "avg_ms": avg,
            "p50_ms": pct(0.5),
            "p95_ms": pct(0.95),
            "p99_ms": pct(0.99),
            "max_ms": v.last().copied().unwrap_or(0.0),
            "over_16ms": v.iter().filter(|&&t| t > 16.0).count(),
            "frames_still_loading": self.loading_frames,
            "max_queue": self.max_queue,
        })
    }
}
