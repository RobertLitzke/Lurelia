use crate::world::chronicle::{EventCtx, Process};
use crate::world::cloth::{linear, Cloth, FlagLine};
use crate::world::fields::Pos;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// Lines of flags in the wind (`world/cloth.rs`): drawn as plates that
/// flutter, not as voxels. Three ways to string them, all in one event:
/// `lines` from point to point; `mast`, every structure of that name with
/// `fan` lines from its top down to stakes round it, the way a prayer-flag
/// pole is guyed; and `link`, each structure of those names joined at its
/// top to its nearest neighbours, so a town's roofs are strung together.
/// `near` and `radius` keep `mast` and `link` to one place.
#[derive(Deserialize, Debug)]
pub struct Flags {
    #[serde(default)]
    lines: Vec<LineDef>,
    /// A structure name: fan lines from the top of each one.
    #[serde(default)]
    mast: Option<String>,
    /// How many lines fan out from each mast.
    #[serde(default = "d_fan")]
    fan: u32,
    /// How far from the mast the fan lines come down, metres.
    #[serde(default = "d_fan_radius")]
    fan_radius: f32,
    /// How high over the ground the fan lines are staked, metres.
    #[serde(default = "d_fan_up")]
    fan_up: f32,
    /// Structure names whose tops are strung together.
    #[serde(default)]
    link: Vec<String>,
    /// The longest a linking line may run, metres.
    #[serde(default = "d_within")]
    within: f32,
    /// How many neighbours each linked structure is strung to.
    #[serde(default = "d_per")]
    per: usize,
    /// Metres under the top of a structure the line is tied on.
    #[serde(default = "d_below")]
    below: f32,
    #[serde(default)]
    near: Option<Pos>,
    #[serde(default = "d_radius")]
    radius: f32,
    /// sRGB bytes, taken in turn along each line. The five colours of a
    /// prayer-flag string when left out.
    #[serde(default)]
    colours: Vec<[u8; 3]>,
    /// One flag's width and height, metres.
    #[serde(default = "d_size")]
    size: [f32; 2],
    #[serde(default = "d_spacing")]
    spacing: f32,
    /// How far the middle of a line hangs, as a share of its length.
    #[serde(default = "d_sag")]
    sag: f32,
}

#[derive(Deserialize, Debug)]
struct LineDef {
    from: Pos,
    /// Metres over the ground at `from`.
    #[serde(default)]
    from_up: f32,
    to: Pos,
    #[serde(default)]
    to_up: f32,
}

fn d_fan() -> u32 {
    6
}
fn d_fan_radius() -> f32 {
    12.0
}
fn d_fan_up() -> f32 {
    1.0
}
fn d_within() -> f32 {
    24.0
}
fn d_per() -> usize {
    2
}
fn d_below() -> f32 {
    0.3
}
fn d_radius() -> f32 {
    1.0e9
}
fn d_size() -> [f32; 2] {
    [0.3, 0.4]
}
fn d_spacing() -> f32 {
    0.55
}
fn d_sag() -> f32 {
    0.05
}

/// Blue, white, red, green, yellow: sky, wind, fire, water, earth.
const PRAYER: [[u8; 3]; 5] = [[52, 84, 150], [226, 222, 210], [178, 54, 44], [58, 128, 82], [214, 170, 52]];

impl Process for Flags {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let colours: Vec<[f32; 4]> = if self.colours.is_empty() { PRAYER.iter().map(|c| linear(*c)).collect() } else { self.colours.iter().map(|c| linear(*c)).collect() };
        let centre = self.near.as_ref().map(|p| cx.pos(p));
        let within = |p: Vec2| centre.is_none_or(|c| c.distance(p) <= self.radius);
        // The tops of placed structures by name: the middle of the footprint,
        // at the highest voxel.
        let tops = |cx: &EventCtx, name: &str| -> Vec<Vec3> {
            cx.placed
                .iter()
                .filter(|p| cx.lib.structures[p.def].name == name)
                .filter_map(|p| {
                    let (x0, z0, x1, z1) = p.footprint(&cx.shapes);
                    let c = Vec2::new((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125);
                    within(c).then(|| Vec3::new(c.x, (p.origin[1] + cx.shapes[p.shape].top) as f32 * 0.25, c.y))
                })
                .collect()
        };
        let mut lines: Vec<(Vec3, Vec3)> = Vec::new();
        for l in &self.lines {
            let a = cx.pos(&l.from);
            let b = cx.pos(&l.to);
            lines.push((Vec3::new(a.x, cx.f.height(a.x, a.y) + l.from_up, a.y), Vec3::new(b.x, cx.f.height(b.x, b.y) + l.to_up, b.y)));
        }
        if let Some(m) = &self.mast {
            let masts = tops(cx, m);
            if masts.is_empty() {
                cx.warnings.push(format!("flags: no structure called {m} to hang them from"));
            }
            for (k, top) in masts.iter().enumerate() {
                let a = *top - Vec3::Y * self.below;
                for i in 0..self.fan {
                    let ang = (i as f32 + 0.5 * (k % 2) as f32) / self.fan as f32 * std::f32::consts::TAU;
                    let g = Vec2::new(a.x + ang.sin() * self.fan_radius, a.z - ang.cos() * self.fan_radius);
                    lines.push((a, Vec3::new(g.x, cx.f.height(g.x, g.y) + self.fan_up, g.y)));
                }
            }
        }
        if !self.link.is_empty() {
            let mut pts: Vec<Vec3> = Vec::new();
            for n in &self.link {
                pts.extend(tops(cx, n).into_iter().map(|p| p - Vec3::Y * self.below));
            }
            let mut seen = std::collections::HashSet::new();
            for (i, p) in pts.iter().enumerate() {
                let mut near: Vec<(f32, usize)> = pts.iter().enumerate().filter(|(j, q)| *j != i && Vec2::new(q.x - p.x, q.z - p.z).length() <= self.within).map(|(j, q)| (Vec2::new(q.x - p.x, q.z - p.z).length(), j)).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                for &(_, j) in near.iter().take(self.per) {
                    if seen.insert((i.min(j), i.max(j))) {
                        lines.push((*p, pts[j]));
                    }
                }
            }
        }
        let cloth = cx.state.get::<Cloth>();
        let n0 = cloth.flags.len();
        for (k, (a, b)) in lines.iter().enumerate() {
            cloth.flags.push(FlagLine { a: *a, b: *b, sag: self.sag, size: self.size, spacing: self.spacing, colours: colours.clone(), seed: cx.seed ^ (k as u32).wrapping_mul(2654435761) });
        }
        let metres: f32 = lines.iter().map(|(a, b)| a.distance(*b)).sum();
        format!("{} flag lines, {metres:.0} m, {} lines in the world", lines.len(), n0 + lines.len())
    }
}
