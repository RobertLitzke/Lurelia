use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos, Tube};
use crate::world::noise::{perlin2, smoothstep};
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A lava tube: a tunnel `depth` m under the ground along `path`, opening at
/// its `mouths` and through `skylights` collapsed holes in its roof. `wall`
/// turns the rock all round it to a material (the dark basalt the tube
/// drained out of), as a `passage`'s does.
///
/// A network is several tubes: a branch with a closed end stops at its
/// depth there, so ending it on another tube's line (at that tube's depth)
/// joins the two underground.
#[derive(Deserialize, Debug)]
pub struct LavaTube {
    path: Vec<Pos>,
    radius: f32,
    depth: f32,
    /// Depth at each path point instead of `depth` everywhere: a lower level
    /// dropping under an upper one, a branch rising to meet another.
    #[serde(default)]
    depths: Vec<f32>,
    /// Which ends rise to the surface as mouths.
    #[serde(default)]
    mouths: Mouths,
    #[serde(default)]
    skylights: u32,
    #[serde(default)]
    wall: Option<String>,
    /// How far the line wanders off the points, metres.
    #[serde(default = "d_meander")]
    meander: f32,
    /// How much the bore swells and pinches along its length (0.25: a
    /// quarter either way).
    #[serde(default)]
    vary: f32,
    /// Chambers: places evenly along it where the bore opens out to
    /// `chamber` times its radius over some 25 m, where lava pooled.
    #[serde(default)]
    chambers: u32,
    #[serde(default = "d_chamber")]
    chamber: f32,
    /// A material over the roof (glow-worm colonies, in patches when there
    /// is a `wall`), between these fractions of its length.
    #[serde(default)]
    lining: Option<String>,
    #[serde(default = "d_lined")]
    lined: [f32; 2],
    #[serde(default)]
    poi: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Mouths {
    #[default]
    Both,
    Start,
    End,
    None,
}

fn d_meander() -> f32 {
    4.0
}
fn d_chamber() -> f32 {
    1.8
}
fn d_lined() -> [f32; 2] {
    [0.0, 1.0]
}

/// Every tube segment so far; the world keeps them to carve the voxels.
#[derive(Default)]
pub struct Tubes(pub Vec<Tube>);

impl Process for LavaTube {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let wall = self.wall.as_ref().map_or(0, |m| cx.mat(m));
        let lining = self.lining.as_ref().map_or(0, |m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        if pts.len() < 2 {
            return "skipped: a lava tube needs two points".into();
        }
        if !self.depths.is_empty() && self.depths.len() != pts.len() {
            cx.warnings.push(format!("lava_tube: {} depths for {} points; using `depth`", self.depths.len(), pts.len()));
        }
        let depth_at = |i: usize| if self.depths.len() == pts.len() { self.depths[i] } else { self.depth };
        let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let (open_a, open_b) = match self.mouths {
            Mouths::Both => (true, true),
            Mouths::Start => (true, false),
            Mouths::End => (false, true),
            Mouths::None => (false, false),
        };
        // Points every 2 m, wandering off the line but not at its ends, so a
        // closed end lands where it was put.
        let mut line = Vec::new();
        let mut depth = Vec::new();
        let mut s = 0.0;
        for (i, w) in pts.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let len = a.distance(b);
            let d = (b - a) / len.max(1e-3);
            let perp = Vec2::new(-d.y, d.x);
            let steps = (len / 2.0).ceil().max(1.0) as usize;
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                let at = s + t * len;
                // As `geom::densify` wanders, but still at a closed end.
                let mut fade = (t * (1.0 - t) * 4.0).clamp(0.35, 1.0);
                if !open_a {
                    fade *= (at / 20.0).min(1.0);
                }
                if !open_b {
                    fade *= ((total - at) / 20.0).min(1.0);
                }
                let off = perlin2(at * 0.012, 3.7, seed) * self.meander * 2.0 * fade;
                line.push(a + d * len * t + perp * off);
                depth.push(depth_at(i) + (depth_at(i + 1) - depth_at(i)) * t);
            }
            s += len;
        }
        line.push(pts[pts.len() - 1]);
        depth.push(depth_at(pts.len() - 1));

        let f = &cx.f;
        let n = line.len();
        let mut along = 0.0;
        let mut centres = Vec::with_capacity(n);
        let mut radii = Vec::with_capacity(n);
        for (k, p) in line.iter().enumerate() {
            if k > 0 {
                along += p.distance(line[k - 1]);
            }
            let mut r = self.radius * (1.0 + self.vary * perlin2(along * 0.045, 11.3, seed ^ 0x7b));
            for c in 0..self.chambers {
                let at = (c as f32 + 0.5) / self.chambers as f32 * total;
                let bell = smoothstep(14.0, 0.0, (along - at).abs());
                r *= 1.0 + (self.chamber - 1.0) * bell;
            }
            // Rise to the surface at a mouth.
            let ends = if open_a { smoothstep(0.0, 18.0, along) } else { 1.0 } * if open_b { smoothstep(total, total - 18.0, along) } else { 1.0 };
            let g = f.height(p.x, p.y);
            let mut y = g - (depth[k] * ends).max(r * 0.35);
            // Away from the mouths the roof stays under the ground: a
            // chamber does not break the surface.
            if ends > 0.99 {
                y = y.min(g - r * 1.1 - 1.0);
            }
            centres.push(Vec3::new(p.x, y, p.y));
            radii.push(r);
        }
        let tubes = &mut cx.state.get::<Tubes>().0;
        for k in 0..n - 1 {
            let frac = k as f32 / (n - 1) as f32;
            let lined = lining != 0 && frac >= self.lined[0] && frac <= self.lined[1];
            tubes.push(Tube { a: centres[k], b: centres[k + 1], r: (radii[k] + radii[k + 1]) * 0.5, lining: if lined { lining } else { 0 }, wall });
        }
        // Skylights: shafts where the roof fell in.
        for s in 0..self.skylights {
            let k = (((s as f32 + 1.0) / (self.skylights as f32 + 1.0) * n as f32) as usize).min(n - 1);
            let c = centres[k];
            let top = f.height(c.x, c.z) + 1.0;
            tubes.push(Tube { a: c, b: Vec3::new(c.x, top, c.z), r: radii[k].min(self.radius) * 0.75, lining: 0, wall });
            cx.pois.push(Poi { name: "skylight into a lava tube".into(), pos: Vec2::new(c.x, c.z), cause });
        }
        if open_a {
            cx.pois.push(Poi { name: "lava tube mouth".into(), pos: line[0], cause });
        }
        if open_b {
            cx.pois.push(Poi { name: "lava tube mouth".into(), pos: line[n - 1], cause });
        }
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: line[n / 2], cause });
        }
        format!("lava tube {total:.0} m long, {} skylights, {} chambers", self.skylights, self.chambers)
    }
}
