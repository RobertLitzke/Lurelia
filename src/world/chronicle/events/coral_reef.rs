//! Coral reefs (docs/UNDERWATER.md): the shared profile of a reef's seaward
//! side, and the `coral_reef` event, a fringing reef along a coast. `atoll`
//! and `cenote` (their own files) use the same pieces.
//!
//! Across a reef, from the shore out: a reef flat a little under the surface,
//! the crest where the swell breaks, a fore reef of spurs and grooves (ridges
//! of coral running seaward with sand channels between) sloping down to the
//! drop-off, a wall falling near sheer to the foot, and a rubble apron that
//! eases back into the old seabed. The wall is cut under in places
//! (`undercut`): caves and overhangs carved by the same capsules as passages.

use super::lava_tube::Tubes;
use crate::world::chronicle::geom::spline;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos, Tube};
use crate::world::noise::{perlin2, smoothstep};
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// The seaward side of a reef, from its crest out: shared by `coral_reef`
/// and `atoll`. Depths are metres under the sea surface.
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(default)]
pub struct Fore {
    /// Depth at the crest.
    pub crest: f32,
    /// Width of the fore reef from the crest to the drop-off, metres.
    pub width: f32,
    /// Depth at the top of the wall.
    pub wall_top: f32,
    /// Depth at the foot of the wall (kept above the floor of the world).
    pub wall_foot: f32,
    /// Horizontal run of the wall, metres: 2-3 is near sheer.
    pub wall_run: f32,
    /// Spurs and grooves: spacing along the reef, and how high a spur
    /// stands over its groove.
    pub spur_spacing: f32,
    pub spur_height: f32,
    /// Rubble apron past the foot, metres, over which the old seabed
    /// comes back.
    pub apron: f32,
    /// Share of the wall cut under (0 none, 1 all of it).
    pub undercut: f32,
}

impl Default for Fore {
    fn default() -> Self {
        Fore { crest: 0.5, width: 32.0, wall_top: 5.0, wall_foot: 22.0, wall_run: 2.5, spur_spacing: 9.0, spur_height: 1.6, apron: 50.0, undercut: 0.35 }
    }
}

/// Which part of a reef a cell is in, for the biomes (`region` rules).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Zone {
    Flat,
    Crest,
    Fore,
    Groove,
    Wall,
    Apron,
}

impl Zone {
    pub fn region(self) -> &'static str {
        match self {
            Zone::Flat => "reef_flat",
            Zone::Crest => "reef_crest",
            Zone::Fore => "fore_reef",
            Zone::Groove => "reef_groove",
            Zone::Wall => "reef_wall",
            Zone::Apron => "reef_apron",
        }
    }
}

/// Where the drop-off is, metres seaward of the crest, at `along` metres
/// along the reef: the line wanders, so the wall is not a ruled edge.
pub fn drop_at(fore: &Fore, along: f32, seed: u32) -> f32 {
    fore.width + perlin2(along * 0.035, 3.1, seed ^ 0xd209) * fore.width * 0.22
}

/// The ground `s` metres seaward of the crest (0 at the crest), `along`
/// metres along the reef, over old ground `old`: its height and zone. None
/// past the apron (left alone).
pub fn fore_ground(fore: &Fore, sea: f32, s: f32, along: f32, old: f32, seed: u32) -> Option<(f32, Zone)> {
    let crest_y = sea - fore.crest;
    let top_y = sea - fore.wall_top;
    // A metre above the floor of the world at least.
    let foot_y = (sea - fore.wall_foot).max(1.0);
    let drop = drop_at(fore, along, seed);
    let rough = perlin2(s * 0.3, along * 0.3, seed ^ 0x51) * 0.25;
    if s < 2.0 {
        return Some((crest_y + rough * 0.6, Zone::Crest));
    }
    if s < drop {
        let t = ((s - 2.0) / (drop - 2.0)).clamp(0.0, 1.0);
        let base = crest_y + (top_y - crest_y) * t.powf(0.85);
        // Spurs run seaward: a ridge every `spur_spacing` along the reef,
        // wandering a little, highest mid-slope and dying out at both ends.
        let ph = along / fore.spur_spacing * std::f32::consts::TAU + perlin2(along * 0.05, s * 0.04, seed ^ 0x77) * 2.2;
        let ridge = smoothstep(-0.25, 0.65, ph.cos());
        let swell = (t * std::f32::consts::PI).sin().max(0.0).powf(0.6);
        let y = base + fore.spur_height * (ridge - 0.45) * swell + rough;
        let zone = if ridge < 0.3 && swell > 0.25 { Zone::Groove } else { Zone::Fore };
        return Some((y.min(crest_y), zone));
    }
    let run = fore.wall_run.max(0.5);
    if s < drop + run {
        // The wall: a steep S from the top to the foot.
        let t = smoothstep(0.0, 1.0, (s - drop) / run);
        return Some((top_y + (foot_y - top_y) * t, Zone::Wall));
    }
    let past = s - drop - run;
    if past < fore.apron {
        let k = smoothstep(0.0, fore.apron, past);
        let rubble = perlin2(s * 0.2, along * 0.2, seed ^ 0x99) * 0.6 * (1.0 - k);
        // Back to the old seabed, but never up above the foot near the wall.
        let y = foot_y + rubble + (old.min(top_y) - foot_y) * k * k;
        return Some((y, if past < 10.0 { Zone::Wall } else { Zone::Apron }));
    }
    None
}

/// Caves and overhangs under the wall: short level capsules a little inside
/// its face, in the stretches the noise picks, between the top and the
/// foot. `face(a)` is a point on the wall's face `a` metres along and `inward`
/// the unit direction into the reef there.
pub fn undercuts(fore: &Fore, sea: f32, length: f32, seed: u32, face: impl Fn(f32) -> (Vec2, Vec2)) -> Vec<Tube> {
    let mut out = Vec::new();
    if fore.undercut <= 0.0 {
        return out;
    }
    let top_y = sea - fore.wall_top;
    let foot_y = (sea - fore.wall_foot).max(1.0);
    let height = top_y - foot_y;
    if height < 5.0 {
        return out;
    }
    let r = (height * 0.22).clamp(1.6, 3.4);
    let step = 3.0;
    let mut a = 0.0;
    while a + step <= length {
        let keep = perlin2(a * 0.03, 7.7, seed ^ 0x0c) * 0.5 + 0.5;
        if keep < fore.undercut {
            let (p0, n0) = face(a);
            let (p1, n1) = face(a + step);
            let y = foot_y + r + 0.6 + (height - 2.0 * r - 1.2).max(0.0) * (0.35 + 0.3 * perlin2(a * 0.05, 1.3, seed ^ 0x3d));
            // Centred a little inside the face, so the rock over it stands
            // out as a lip.
            let a3 = Vec3::new(p0.x + n0.x * r * 0.55, y, p0.y + n0.y * r * 0.55);
            let b3 = Vec3::new(p1.x + n1.x * r * 0.55, y, p1.y + n1.y * r * 0.55);
            out.push(Tube { a: a3, b: b3, r, lining: 0, wall: 0 });
        }
        a += step;
    }
    out
}

/// Paint a reef cell: its height, surface and zone (as a region).
pub fn paint(f: &mut Fields, i: usize, y: f32, zone: Zone, rock: u8, sand: u8, cause: u16) {
    f.h[i] = y;
    f.surface[i] = if matches!(zone, Zone::Groove | Zone::Apron) { sand } else { rock };
    f.region[i] = f.region_id(zone.region());
    f.cause[i] = cause;
}

/// A fringing reef along a coast: `path` is its crest, a little off the
/// shore; the side where the ground is lower is the sea.
#[derive(Deserialize, Debug)]
pub struct CoralReef {
    path: Vec<Pos>,
    /// The reef flat between the shore and the crest: width and depth.
    #[serde(default = "default_flat")]
    flat: f32,
    #[serde(default = "default_flat_depth")]
    flat_depth: f32,
    #[serde(default)]
    fore: Fore,
    /// Over how many metres the reef dies out at each end.
    #[serde(default = "default_taper")]
    taper: f32,
    #[serde(default = "default_rock")]
    rock: String,
    #[serde(default = "default_sand")]
    sand: String,
    #[serde(default)]
    poi: Option<String>,
}

fn default_flat() -> f32 {
    22.0
}
fn default_flat_depth() -> f32 {
    1.2
}
fn default_taper() -> f32 {
    40.0
}
/// The reef's rock where the event names none (`events::defaults_used`).
pub const ROCK: &str = "reef_rock";
fn default_rock() -> String {
    ROCK.into()
}
fn default_sand() -> String {
    "sand".into()
}

/// The nearest point of polyline `line` to `p`: (signed distance, positive
/// to the left of travel; metres along; the unit left normal there).
pub fn nearest(line: &[Vec2], cum: &[f32], p: Vec2) -> (f32, f32, Vec2) {
    let mut best = (f32::MAX, 0.0, 0.0, Vec2::X);
    for k in 0..line.len() - 1 {
        let (a, b) = (line[k], line[k + 1]);
        let ab = b - a;
        let len2 = ab.length_squared().max(1e-6);
        let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
        let q = a + ab * t;
        let d = p.distance(q);
        if d < best.0 {
            let left = Vec2::new(-ab.y, ab.x).normalize_or_zero();
            let side = if (p - q).dot(left) >= 0.0 { 1.0 } else { -1.0 };
            best = (d, side, cum[k] + ab.length() * t, left);
        }
    }
    (best.0 * best.1, best.2, best.3)
}

impl Process for CoralReef {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed, cx.cause);
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        if pts.len() < 2 {
            return "coral_reef: a path needs two points".into();
        }
        let line = spline(&pts, false, 4.0);
        let mut cum = vec![0.0f32];
        for w in line.windows(2) {
            cum.push(cum.last().unwrap() + w[0].distance(w[1]));
        }
        let length = *cum.last().unwrap();
        let rock = cx.mat(&self.rock);
        let sand = cx.mat(&self.sand);
        let sea = cx.f.sea;
        let fore = self.fore;
        // The sea is on the side where the ground 40 m off the crest is
        // lower, along most of the reef.
        let mut vote = 0.0;
        for k in 0..line.len() - 1 {
            let mid = (line[k] + line[k + 1]) * 0.5;
            let d = line[k + 1] - line[k];
            let left = Vec2::new(-d.y, d.x).normalize_or_zero();
            let (l, r) = (cx.f.height(mid.x + left.x * 40.0, mid.y + left.y * 40.0), cx.f.height(mid.x - left.x * 40.0, mid.y - left.y * 40.0));
            vote += (r - l).signum();
        }
        let seaward = if vote >= 0.0 { 1.0 } else { -1.0 };
        let reach_sea = fore.width * 1.3 + fore.wall_run + fore.apron;
        let reach = reach_sea.max(self.flat + 10.0);
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for p in &line {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let cell = cx.f.size / cx.f.n as f32;
        let n = cx.f.n as i32;
        let (x0, z0) = (((lo.x - reach) / cell).floor().max(0.0) as i32, ((lo.y - reach) / cell).floor().max(0.0) as i32);
        let (x1, z1) = ((((hi.x + reach) / cell).ceil() as i32).min(n - 1), (((hi.y + reach) / cell).ceil() as i32).min(n - 1));
        let mut cells = 0;
        for z in z0..=z1 {
            for x in x0..=x1 {
                let i = z as usize * cx.f.n + x as usize;
                if cx.f.occupied[i] {
                    continue;
                }
                let p = cx.f.centre(i);
                let (sd, along, _) = nearest(&line, &cum, p);
                let s = sd * seaward;
                if s < -self.flat - 8.0 || s > reach_sea {
                    continue;
                }
                let k = smoothstep(0.0, self.taper, along) * smoothstep(0.0, self.taper, length - along);
                if k <= 0.0 {
                    continue;
                }
                let old = cx.f.h[i];
                let (y, zone) = if s < 0.0 {
                    // Behind the crest the bed falls away from the shore to
                    // the flat's depth at the crest's back, built up where it
                    // was lower and cut down where it was higher. Dry land,
                    // the shore itself, is left alone.
                    if old >= sea + 0.2 {
                        continue;
                    }
                    let flat_y = sea - self.flat_depth + perlin2(p.x * 0.15, p.y * 0.15, seed ^ 0x21) * 0.3;
                    let edge = smoothstep(-self.flat - 8.0, -2.0, s);
                    (old + (flat_y - old) * edge, Zone::Flat)
                } else {
                    match fore_ground(&fore, sea, s, along, old, seed) {
                        Some(v) => v,
                        None => continue,
                    }
                };
                let y = old + (y - old) * k;
                if k > 0.5 {
                    paint(&mut cx.f, i, y, zone, rock, sand, cause);
                } else {
                    cx.f.h[i] = y;
                }
                cells += 1;
            }
        }
        // Caves under the wall, away from the tapered ends.
        let face = |a: f32| {
            let a = a.clamp(0.0, length);
            let k = cum.partition_point(|c| *c < a).clamp(1, line.len() - 1);
            let (p, q) = (line[k - 1], line[k]);
            let t = ((a - cum[k - 1]) / (cum[k] - cum[k - 1]).max(1e-3)).clamp(0.0, 1.0);
            let at = p.lerp(q, t);
            let d = q - p;
            let sea_dir = Vec2::new(-d.y, d.x).normalize_or_zero() * seaward;
            let out = drop_at(&fore, a, seed) + fore.wall_run * 0.5;
            (at + sea_dir * out, -sea_dir)
        };
        let inner = (length - 2.0 * self.taper).max(0.0);
        let mut caves = undercuts(&fore, sea, inner, seed, |a| face(a + self.taper));
        let ncaves = caves.len();
        cx.state.get::<Tubes>().0.append(&mut caves);
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: line[line.len() / 2], cause });
        }
        format!("coral reef {length:.0} m long, {cells} cells, wall {:.0}-{:.0} m deep, {ncaves} cave sections", fore.wall_top, fore.wall_foot.min(sea - 1.0))
    }
}
