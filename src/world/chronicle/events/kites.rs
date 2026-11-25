use crate::world::chronicle::{EventCtx, Process};
use crate::world::cloth::{linear, Cloth, Kite};
use crate::world::fields::Pos;
use crate::world::noise::hash32;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// Kites flown from the ground (`world/cloth.rs`): each one a diamond on a
/// string with a tail, drawn as plates that ride and rock in the wind, high
/// on a windy day, lower on a light one, and down on a still day or at night.
/// `at` gives each anchor; `around` and `count` scatter them on a ring.
#[derive(Deserialize, Debug)]
pub struct Kites {
    #[serde(default)]
    at: Vec<Pos>,
    #[serde(default)]
    around: Option<Pos>,
    /// The ring's radius, metres.
    #[serde(default = "d_ring")]
    radius: f32,
    #[serde(default)]
    count: u32,
    /// How high they fly over their anchors on a fair day, metres.
    #[serde(default = "d_height")]
    height: [f32; 2],
    /// Half the diamond's diagonal, metres.
    #[serde(default = "d_size")]
    size: [f32; 2],
    /// sRGB bytes for the bodies; the stripe and tail take another of them.
    #[serde(default)]
    colours: Vec<[u8; 3]>,
    /// Metres over the ground each `at` anchor stands, in order: a kite
    /// flown from a tower's roof (Vila Azul) rather than the foot of it.
    #[serde(default)]
    up: Vec<f32>,
}

fn d_ring() -> f32 {
    40.0
}
fn d_height() -> [f32; 2] {
    [28.0, 70.0]
}
fn d_size() -> [f32; 2] {
    [0.7, 1.3]
}

const DYES: [[u8; 3]; 5] = [[178, 56, 44], [214, 160, 56], [58, 70, 128], [70, 126, 90], [230, 224, 208]];

impl Process for Kites {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let dyes: Vec<[u8; 3]> = if self.colours.is_empty() { DYES.to_vec() } else { self.colours.clone() };
        let mut anchors: Vec<Vec2> = self.at.iter().map(|p| cx.pos(p)).collect();
        if let Some(c) = &self.around {
            let c = cx.pos(c);
            for i in 0..self.count {
                let r = |k: u32| hash32(cx.seed ^ i.wrapping_mul(0x85eb_ca6b) ^ k) as f32 / u32::MAX as f32;
                let ang = (i as f32 + r(1) * 0.6) / self.count.max(1) as f32 * std::f32::consts::TAU;
                let rr = self.radius * (0.75 + 0.3 * r(2));
                anchors.push(Vec2::new(c.x + ang.sin() * rr, c.y - ang.cos() * rr));
            }
        }
        let seed = cx.seed;
        let mut made = Vec::new();
        for (i, a) in anchors.iter().enumerate() {
            let r = |k: u32| hash32(seed ^ (i as u32).wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x27d4_eb2f)) as f32 / u32::MAX as f32;
            let pick = |k: u32| dyes[(hash32(seed ^ i as u32 ^ k) as usize) % dyes.len()];
            let body = pick(11);
            let mut stripe = pick(12);
            if stripe == body {
                stripe = dyes[(dyes.iter().position(|d| *d == body).unwrap_or(0) + 1) % dyes.len()];
            }
            made.push(Kite {
                anchor: Vec3::new(a.x, cx.f.height(a.x, a.y) + 1.4 + self.up.get(i).copied().unwrap_or(0.0), a.y),
                height: self.height[0] + (self.height[1] - self.height[0]) * r(3),
                size: self.size[0] + (self.size[1] - self.size[0]) * r(4),
                body: linear(body),
                stripe: linear(stripe),
                tail: linear(pick(13)),
                seed: hash32(seed ^ i as u32 ^ 0x51ed),
            });
        }
        let n = made.len();
        cx.state.get::<Cloth>().kites.extend(made);
        format!("{n} kites")
    }
}
