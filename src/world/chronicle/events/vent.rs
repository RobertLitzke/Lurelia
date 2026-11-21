use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Pos, WorldVent};
use glam::Vec3;
use serde::Deserialize;

/// Places that breathe particles all the time: a chimney's smoke, sea smoke
/// over open water, steam from a crack. Each point is `[east, south, up]`
/// metres from `at`, up being above the ground there, or above the sea with
/// `on_sea`. The game trickles `fx` from each at `rate` a second, from
/// somewhere within `spread` m of it, while the camera is near.
#[derive(Deserialize, Debug)]
pub struct Vent {
    at: Pos,
    points: Vec<[f32; 3]>,
    fx: String,
    #[serde(default = "default_rate")]
    rate: f32,
    #[serde(default)]
    spread: f32,
    #[serde(default)]
    on_sea: bool,
}

fn default_rate() -> f32 {
    2.0
}

impl Process for Vent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        for d in &self.points {
            let t = cx.turned(&self.at, [d[0], d[1]]);
            let (x, z) = (c.x + t[0], c.y + t[1]);
            let ground = if self.on_sea { cx.f.sea } else { cx.f.height(x, z) };
            cx.vents.push(WorldVent { pos: Vec3::new(x, ground + d[2], z), fx: self.fx.clone(), rate: self.rate, spread: self.spread });
        }
        format!("{} vents of {} round {:.0},{:.0}", self.points.len(), self.fx, c.x, c.y)
    }
}
