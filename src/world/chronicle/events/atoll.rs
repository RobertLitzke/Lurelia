use super::coral_reef::{fore_ground, named, paint_as, undercuts, Fore, Zone};
use super::lava_tube::Tubes;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos, Tube};
use crate::world::noise::{hash32, perlin2, smoothstep, unit};
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::f32::consts::TAU;

/// An atoll (docs/UNDERWATER.md): a ring of reef round a lagoon, out in open
/// water. The ring is `ring` metres wide round a circle of `radius` (the
/// outline wanders), with its crest on the outer edge and the same fore reef
/// and wall outside it as a `coral_reef`. On the ring stand `motu`, low
/// islets of sand; `passes` channels cut through it where the tide runs; the
/// lagoon inside is sand `lagoon_depth` deep, with patch reefs coming up to
/// a metre or two of the surface, and maybe a `blue_hole`: a round shaft
/// with undercut walls, down to the floor of the world.
#[derive(Deserialize, Debug)]
pub struct Atoll {
    at: Pos,
    radius: f32,
    #[serde(default = "default_ring")]
    ring: f32,
    /// Depth of the reef flat on the ring.
    #[serde(default = "default_flat_depth")]
    flat_depth: f32,
    #[serde(default = "default_lagoon")]
    lagoon_depth: f32,
    /// How many patch reefs in the lagoon, about.
    #[serde(default = "default_patches")]
    patches: u32,
    #[serde(default = "default_motu")]
    motu: u32,
    /// How high the motu stand over the sea, metres.
    #[serde(default = "default_motu_height")]
    motu_height: f32,
    #[serde(default = "default_passes")]
    passes: u32,
    #[serde(default = "default_pass_width")]
    pass_width: f32,
    /// Radius of a blue hole in the lagoon (0: none).
    #[serde(default)]
    blue_hole: f32,
    #[serde(default)]
    fore: Fore,
    #[serde(default = "default_rock")]
    rock: String,
    #[serde(default = "default_sand")]
    sand: String,
    #[serde(default)]
    poi: Option<String>,
    /// A prefix for the regions it paints: `"fw"` names them `fw_reef_flat`,
    /// `fw_lagoon`, `fw_motu` and so on, so a world's own biomes claim them
    /// and the tropical reef biomes (named after the plain regions) are not
    /// pulled in with their corals. Without it the regions are the plain
    /// names the reef biomes match.
    #[serde(default)]
    regions: Option<String>,
}

fn default_ring() -> f32 {
    24.0
}
fn default_flat_depth() -> f32 {
    0.8
}
fn default_lagoon() -> f32 {
    7.0
}
fn default_patches() -> u32 {
    8
}
fn default_motu() -> u32 {
    4
}
fn default_motu_height() -> f32 {
    1.6
}
fn default_passes() -> u32 {
    1
}
fn default_pass_width() -> f32 {
    14.0
}
/// The reef's rock where the event names none (`events::defaults_used`).
pub const ROCK: &str = "reef_rock";
fn default_rock() -> String {
    ROCK.into()
}
fn default_sand() -> String {
    "sand".into()
}

/// The angular distance between two bearings, radians.
fn ang(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}

impl Process for Atoll {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(&self.at);
        let rock = cx.mat(&self.rock);
        let sand = cx.mat(&self.sand);
        let pre = self.regions.clone().unwrap_or_default();
        let sea = cx.f.sea;
        let fore = self.fore;
        let r0 = self.radius;
        // The outline: the ring's outer edge wanders by a fifth of its radius.
        let edge = |a: f32| r0 * (1.0 + 0.12 * perlin2(a.cos() * 1.4 + 3.0, a.sin() * 1.4, seed) + 0.06 * perlin2(a.cos() * 3.3, a.sin() * 3.3 + 1.0, seed ^ 0x5a));
        let pick = |k: u32, salt: u32| unit(hash32(seed ^ salt ^ k.wrapping_mul(0x9e37_79b9)));
        // Motu and passes at spread-out bearings, passes first so no islet
        // sits in one.
        let passes: Vec<f32> = (0..self.passes).map(|k| (k as f32 + pick(k, 0x9a55) * 0.6) / self.passes.max(1) as f32 * TAU).collect();
        let mut motu: Vec<(f32, f32)> = Vec::new();
        for k in 0..self.motu * 3 {
            if motu.len() as u32 >= self.motu {
                break;
            }
            let a = pick(k, 0x307) * TAU;
            let span = (0.18 + 0.3 * pick(k, 0x3a)) * 40.0 / r0.max(20.0);
            if passes.iter().all(|p| ang(*p, a) > span + self.pass_width / r0 + 0.1) && motu.iter().all(|m| ang(m.0, a) > m.1 + span + 0.1) {
                motu.push((a, span));
            }
        }
        // Patch reefs and the blue hole in the lagoon.
        let inner = r0 - self.ring;
        let patches: Vec<(Vec2, f32)> = (0..self.patches)
            .map(|k| {
                let a = pick(k, 0x7a7) * TAU;
                let d = inner * 0.75 * pick(k, 0x7b1).sqrt();
                (c + Vec2::new(a.cos(), a.sin()) * d, 3.0 + 5.0 * pick(k, 0x7c3))
            })
            .collect();
        let hole = (self.blue_hole > 0.0).then(|| {
            let a = pick(0, 0xb1e) * TAU;
            (c + Vec2::new(a.cos(), a.sin()) * inner * 0.35, self.blue_hole)
        });
        let reach = r0 * 1.25 + fore.width * 1.3 + fore.wall_run + fore.apron;
        let mut motu_cells = 0;
        each_cell(&mut cx.f, c, reach, |f: &mut Fields, i, p| {
            if f.occupied[i] {
                return;
            }
            let v = p - c;
            let d = v.length();
            let a = v.y.atan2(v.x);
            let outer = edge(a);
            let s = d - outer;
            let old = f.h[i];
            let along = a.rem_euclid(TAU) * outer;
            if s >= 0.0 {
                if let Some((mut y, mut zone)) = fore_ground(&fore, sea, s, along, old, seed) {
                    // A pass runs on out through the fore reef to the drop-off.
                    for pa in &passes {
                        let w = ang(a, *pa) * d / (self.pass_width * 0.5);
                        let open = smoothstep(super::coral_reef::drop_at(&fore, along, seed) + 4.0, 0.0, s);
                        if w < 1.3 && open > 0.0 {
                            let k = smoothstep(1.3, 0.5, w) * open;
                            let floor = sea - self.lagoon_depth - 1.0;
                            if floor < y {
                                y = y + (floor - y) * k;
                                if k > 0.4 {
                                    zone = Zone::Groove;
                                }
                            }
                        }
                    }
                    paint_as(f, i, y, zone, rock, sand, cause, &pre);
                }
                return;
            }
            let into = -s;
            let flat_y = sea - self.flat_depth + perlin2(p.x * 0.17, p.y * 0.17, seed ^ 0x21) * 0.25;
            if into < self.ring {
                // On the ring: the crest at its outer edge, the flat behind.
                let (mut y, mut zone) = if into < 3.0 { (sea - fore.crest, Zone::Crest) } else { (flat_y, Zone::Flat) };
                // A motu: a low sand dome on the ring.
                for (ma, span) in &motu {
                    let da = ang(a, *ma) / span;
                    let mid = (into - self.ring * 0.5).abs() / (self.ring * 0.42);
                    let t = 1.0 - (da * da + mid * mid);
                    if t > 0.0 {
                        let top = sea + self.motu_height * t.sqrt().min(1.0) + perlin2(p.x * 0.2, p.y * 0.2, seed ^ 0x40) * 0.2 * t;
                        if top > y {
                            y = top;
                            zone = Zone::Flat;
                        }
                    }
                }
                // A pass: a channel through the ring to the lagoon floor.
                for pa in &passes {
                    let w = ang(a, *pa) * d / (self.pass_width * 0.5);
                    if w < 1.3 {
                        let k = smoothstep(1.3, 0.5, w);
                        y = y + (sea - self.lagoon_depth - 1.0 - y) * k;
                        zone = Zone::Groove;
                    }
                }
                if y > sea + 0.2 {
                    f.h[i] = y;
                    f.surface[i] = sand;
                    f.region[i] = f.region_id(&named(&pre, "motu"));
                    f.cause[i] = cause;
                    motu_cells += 1;
                } else {
                    paint_as(f, i, y, zone, rock, sand, cause, &pre);
                }
                return;
            }
            // The lagoon: shelving from the ring to its sand floor.
            let t = smoothstep(0.0, 14.0, into - self.ring);
            let floor = sea - self.lagoon_depth + perlin2(p.x * 0.05, p.y * 0.05, seed ^ 0x6c) * 0.8;
            let mut y = flat_y + (floor - flat_y) * t;
            let mut region = "lagoon";
            let mut surface = sand;
            for (pc, pr) in &patches {
                let q = p.distance(*pc) / (pr * (1.0 + 0.25 * perlin2(p.x * 0.3, p.y * 0.3, seed ^ 0x2b)));
                if q < 1.0 {
                    let top = sea - 1.4 - perlin2(pc.x, pc.y, seed) * 0.6;
                    let k = smoothstep(1.0, 0.45, q);
                    y = y + (top - y) * k;
                    if k > 0.3 {
                        region = "patch_reef";
                        surface = rock;
                    }
                }
            }
            if let Some((hc, hr)) = hole {
                let q = p.distance(hc) / hr;
                if q < 1.25 {
                    // A lip of reef round it, then straight down.
                    if q < 1.0 {
                        y = 0.6 + (1.0 - q) * 0.4;
                        region = "blue_hole";
                        surface = sand;
                    } else {
                        y = y.max(sea - 2.5);
                        region = "reef_wall";
                        surface = rock;
                    }
                }
            }
            f.h[i] = y;
            f.surface[i] = surface;
            f.region[i] = f.region_id(&named(&pre, region));
            f.cause[i] = cause;
        });
        // Caves under the outer wall, all the way round.
        let length = TAU * r0;
        let face = |s: f32| {
            let a = s / r0;
            let dir = Vec2::new(a.cos(), a.sin());
            let out = edge(a) + super::coral_reef::drop_at(&fore, a.rem_euclid(TAU) * edge(a), seed) + fore.wall_run * 0.5;
            (c + dir * out, -dir)
        };
        let mut caves = undercuts(&fore, sea, length, seed, face);
        // The blue hole is wider below than at its mouth.
        if let Some((hc, hr)) = hole {
            let depth = sea - 0.6;
            for (ring, at) in [(0.45f32, 0.55f32), (0.7, 0.3)] {
                let y = sea - depth * at;
                let rr = hr * (1.0 + ring);
                let tr = (hr * ring).clamp(1.5, 3.5);
                for k in 0..16 {
                    let (a0, a1) = (k as f32 / 16.0 * TAU, (k + 1) as f32 / 16.0 * TAU);
                    caves.push(Tube {
                        a: Vec3::new(hc.x + a0.cos() * rr * 0.8, y, hc.y + a0.sin() * rr * 0.8),
                        b: Vec3::new(hc.x + a1.cos() * rr * 0.8, y, hc.y + a1.sin() * rr * 0.8),
                        r: tr,
                        lining: 0,
                        wall: 0,
                    });
                }
            }
        }
        let ncaves = caves.len();
        cx.state.get::<Tubes>().0.append(&mut caves);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        }
        if let Some((hc, _)) = hole {
            cx.pois.push(Poi { name: "the blue hole".into(), pos: hc, cause });
        }
        format!(
            "atoll {:.0} m across at {:.0},{:.0}: {} motu ({motu_cells} cells), {} passes, {} patch reefs, blue hole {}, {ncaves} cave sections",
            r0 * 2.0,
            c.x,
            c.y,
            motu.len(),
            passes.len(),
            patches.len(),
            if hole.is_some() { "yes" } else { "no" }
        )
    }
}

/// Every cell within `r` of `c`, with its centre.
fn each_cell(f: &mut Fields, c: Vec2, r: f32, mut g: impl FnMut(&mut Fields, usize, Vec2)) {
    crate::world::chronicle::geom::each_near(f, c, r, |f, i, _, p| g(f, i, p));
}
