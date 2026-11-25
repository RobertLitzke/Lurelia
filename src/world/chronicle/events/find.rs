use super::lava_tube::Tubes;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Item, Pos};
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// Collectibles left in one place on purpose: offerings on a cave floor, a
/// dropped purse, a cache. Each point is `[east, south]` metres from `at`,
/// one `item` on each. They lie on the floor under `height` (metres above the
/// sea), or, without it, on the floor of the nearest lava tube passing within
/// 12 m (an earlier `lava_tube` event), or else on the ground. The report
/// gives the floor height it found, which is the `height` to give a `place`
/// standing beside them.
#[derive(Deserialize, Debug)]
pub struct Find {
    item: String,
    at: Pos,
    #[serde(default = "one")]
    points: Vec<[f32; 2]>,
    #[serde(default)]
    height: Option<f32>,
}

fn one() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0]]
}

/// The floor of the nearest level stretch of lava tube within 12 m of `p`
/// across the ground, if any: the tube's centre line there less its radius.
fn tube_floor(tubes: &[crate::world::fields::Tube], p: Vec2) -> Option<f32> {
    let mut best: Option<(f32, f32)> = None;
    for t in tubes {
        let (a, b) = (Vec2::new(t.a.x, t.a.z), Vec2::new(t.b.x, t.b.z));
        let ab = b - a;
        let len2 = ab.length_squared();
        // Skylight shafts run straight up: skip them.
        if len2 < 0.25 {
            continue;
        }
        let s = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
        let d = p.distance(a + ab * s);
        if d < 12.0 && best.is_none_or(|(bd, _)| d < bd) {
            let y = t.a.y + (t.b.y - t.a.y) * s;
            best = Some((d, y - t.r * 0.9));
        }
    }
    best.map(|(_, y)| y)
}

impl Process for Find {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let cause = cx.cause;
        let sea = cx.f.sea;
        let floor = self.height.map(|h| h + sea).or_else(|| tube_floor(&cx.state.get::<Tubes>().0, c));
        for d in &self.points {
            let t = cx.turned(&self.at, *d);
            let (x, z) = (c.x + t[0], c.y + t[1]);
            let y = floor.unwrap_or_else(|| cx.f.height(x, z));
            // Laid a little over the floor: the game drops a surface find onto
            // the first solid voxel under it.
            cx.items.push(Item { kind: self.item.clone(), pos: Vec3::new(x, y + 1.2, z), size: 1.0, cause, surface: true });
        }
        match floor {
            Some(y) => format!("{} {} round {:.0},{:.0}, floor {:.1} m over the sea", self.points.len(), self.item, c.x, c.y, y - sea),
            None => format!("{} {} round {:.0},{:.0} on the ground", self.points.len(), self.item, c.x, c.y),
        }
    }
}
