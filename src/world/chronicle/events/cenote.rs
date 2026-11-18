use super::lava_tube::Tubes;
use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos, Tube, Wet};
use crate::world::noise::perlin2;
use glam::Vec3;
use serde::Deserialize;
use std::f32::consts::TAU;

/// A deep cenote (docs/UNDERWATER.md): a karst hill `hill` metres over the
/// ground round it, `hill_radius` across its foot, with a round shaft in its
/// crown, sheer-walled, `radius` across and flooded to `below_rim` under the
/// rim with `depth` metres of clear water. Below `bell_from` of its depth
/// the shaft opens out into a bell `bell` metres wider than its mouth, the
/// way a cave roof that fell in leaves one.
///
/// Its water is inland water at its own level, so its bed can lie well above
/// the sea: the sea is drawn as one plane, and a bed below it would show a
/// second surface. The bed is kept a metre above the sea for that reason.
#[derive(Deserialize, Debug)]
pub struct Cenote {
    name: String,
    at: Pos,
    #[serde(default = "default_radius")]
    radius: f32,
    #[serde(default = "default_hill")]
    hill: f32,
    #[serde(default = "default_hill_radius")]
    hill_radius: f32,
    #[serde(default = "default_below_rim")]
    below_rim: f32,
    #[serde(default = "default_depth")]
    depth: f32,
    #[serde(default = "default_bell")]
    bell: f32,
    #[serde(default = "default_bell_from")]
    bell_from: f32,
    /// The hill's own rock (left out: the world's strata).
    #[serde(default)]
    rock: Option<String>,
    /// The shaft's floor.
    #[serde(default = "default_bed")]
    bed: String,
    #[serde(default)]
    poi: Option<String>,
}

fn default_radius() -> f32 {
    15.0
}
fn default_hill() -> f32 {
    28.0
}
fn default_hill_radius() -> f32 {
    75.0
}
fn default_below_rim() -> f32 {
    2.5
}
fn default_depth() -> f32 {
    30.0
}
fn default_bell() -> f32 {
    6.0
}
fn default_bell_from() -> f32 {
    0.35
}
fn default_bed() -> String {
    "sand".into()
}

/// One cenote's water, laid in after the rivers and lakes settle.
pub struct Pool {
    pub cells: Vec<u32>,
    pub level: f32,
    /// Shaft cells: open water at the surface (`Fields::water` 1).
    pub open: Vec<u32>,
}

/// Every cenote so far.
#[derive(Default)]
pub struct Cenotes(pub Vec<Pool>);

/// After `settle_water` (which rebuilds the inland water from the lakes and
/// rivers): the cenotes' own water.
pub fn settle(f: &mut Fields, c: &Cenotes) {
    for pool in &c.0 {
        for &i in &pool.cells {
            f.inland.insert(i, Wet { level: pool.level, flow: [0.0, 0.0] });
        }
        for &i in &pool.open {
            f.water[i as usize] = 1;
        }
    }
}

impl Process for Cenote {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(&self.at);
        let bed = cx.mat(&self.bed);
        let rock = self.rock.as_ref().map_or(0, |m| cx.mat(m));
        let sea = cx.f.sea;
        let ground = cx.f.height(c.x, c.y);
        let rim = ground + self.hill;
        let level = rim - self.below_rim;
        let floor = (level - self.depth).max(sea + 1.0);
        let depth = level - floor;
        let mouth = |a: f32| self.radius * (1.0 + 0.1 * perlin2(a.cos() * 1.7, a.sin() * 1.7, seed ^ 0xce));
        let reach = self.hill_radius;
        let wide = self.radius + self.bell + 2.0;
        let mut cells = Vec::new();
        let mut open = Vec::new();
        each_near(&mut cx.f, c, reach, |f: &mut Fields, i, d, p| {
            let v = p - c;
            let a = v.y.atan2(v.x);
            let m = mouth(a);
            if d < m {
                // The shaft: a sandy floor, a little lower in the middle.
                f.h[i] = floor - 0.8 * (1.0 - (d / m).powi(2)) + perlin2(p.x * 0.2, p.y * 0.2, seed ^ 0x11) * 0.3;
                f.surface[i] = bed;
                f.water[i] = 1;
                open.push(i as u32);
            } else {
                // The hill: flat-topped round the shaft, falling away on a
                // slope a walker can take, its foot lost in the ground.
                // The foot wanders a quarter of the way in and out, so the
                // hill is not a turned dome.
                let foot = reach * (1.0 + 0.22 * perlin2(a.cos() * 1.3 + 5.0, a.sin() * 1.3, seed ^ 0x1f) + 0.1 * perlin2(a.cos() * 3.7, a.sin() * 3.7, seed ^ 0x2f));
                let t = ((foot - d) / (foot - m - 5.0)).clamp(0.0, 1.0);
                let lump = (perlin2(p.x * 0.05, p.y * 0.05, seed ^ 0x4a) * 3.5 + perlin2(p.x * 0.13, p.y * 0.13, seed ^ 0x5a) * 1.4) * t;
                let y = ground + self.hill * t * t * (3.0 - 2.0 * t) + lump;
                if y > f.h[i] {
                    f.h[i] = y;
                }
                if rock != 0 && t > 0.6 {
                    f.surface[i] = rock;
                }
            }
            if d < wide {
                cells.push(i as u32);
                f.region[i] = f.region_id("cenote_shaft");
            }
            f.cause[i] = cause;
        });
        // The bell: rings of capsules round the shaft below `bell_from`,
        // wider at the bottom, which is what leaves the mouth an overhang.
        let mut tubes = Vec::new();
        let top = level - depth * self.bell_from;
        for (k, (at, grow)) in [(0.0f32, 0.55f32), (0.5, 0.85), (0.85, 1.0)].iter().enumerate() {
            let y = top - (top - floor - 2.5) * at;
            let tr = (self.bell * 0.6).clamp(2.0, 4.0);
            let rr = self.radius + self.bell * grow - tr * 0.6;
            let n = 20;
            for s in 0..n {
                let (a0, a1) = (s as f32 / n as f32 * TAU, (s + 1) as f32 / n as f32 * TAU);
                let wob = |a: f32| rr * (1.0 + 0.12 * perlin2(a.cos() * 2.0 + k as f32, a.sin() * 2.0, seed ^ 0xbe));
                tubes.push(Tube {
                    a: Vec3::new(c.x + a0.cos() * wob(a0), y, c.y + a0.sin() * wob(a0)),
                    b: Vec3::new(c.x + a1.cos() * wob(a1), y, c.y + a1.sin() * wob(a1)),
                    r: tr,
                    lining: 0,
                    wall: 0,
                });
            }
        }
        cx.state.get::<Tubes>().0.extend(tubes);
        cx.state.get::<Cenotes>().0.push(Pool { cells, level, open });
        cx.pois.push(Poi { name: self.poi.clone().unwrap_or_else(|| self.name.clone()), pos: c, cause });
        format!("{}: hill {:.0} m over {:.0} m ground, water at {level:.1} m, {depth:.0} m deep, bell to {:.0} m across", self.name, self.hill, ground, (self.radius + self.bell) * 2.0)
    }
}
