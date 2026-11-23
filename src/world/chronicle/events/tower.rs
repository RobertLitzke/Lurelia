use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Arch, Growth, Poi, Pos, Tower, TOWER_ANGLES, TOWER_DY};
use crate::world::noise::{hash32, perlin2, perlin3, smoothstep};
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::f32::consts::TAU;

/// A free-standing rock tower drawn as a volume, so it can pinch in, bulge
/// out and overhang like a giant hoodoo or a karst tower: a radius that
/// changes with height in lumps (`bulge`), vertical fluting, a lean, and a
/// top that is a dome, knobs, a slope, a point or flat. Plants named in
/// `ledge_plants` grow on the ledges where a bulge sticks out from under a
/// waist, and `ledge_vine` hangs from them. With `joints`, `notch`, `arch`,
/// `tideline` and `skerries` it is a sea stack: split along its joints,
/// cut in at the waterline, holed through, and standing among the low rocks
/// it has shed.
#[derive(Deserialize, Debug, Clone)]
pub struct TowerEvent {
    at: Pos,
    radius: f32,
    /// Metres over the ground round its foot...
    #[serde(default)]
    height: f32,
    /// ...or the top's height above sea level.
    #[serde(default)]
    top: Option<f32>,
    /// How lumpy the profile is, 0 (a column) to 1 (waists and bulges).
    #[serde(default = "half")]
    bulge: f32,
    /// Depth of the vertical grooves, as a share of the radius.
    #[serde(default = "flute")]
    flute: f32,
    /// How much narrower the top is than the foot, 0 to 1 (default 0.25 to
    /// 0.55 at random): towers taper, so their sides aren't plumb.
    #[serde(default)]
    taper: Option<f32>,
    /// Lean over its height, metres (default up to a tenth of the height).
    #[serde(default)]
    lean: Option<f32>,
    /// The top: "dome", "knobs", "slope", "point", "flat" or "any" (the default: one
    /// of the first four).
    #[serde(default)]
    crown: Option<String>,
    /// [bearing, factor]: an oval tower.
    #[serde(default)]
    stretch: Option<[f32; 2]>,
    /// A rock of its own instead of the world's strata (a darker or paler
    /// tower among the others).
    #[serde(default)]
    rock: Option<String>,
    /// A material covering its skin (vines, moss), and over how much of it.
    #[serde(default)]
    mantle: Option<String>,
    #[serde(default = "most")]
    mantle_share: f32,
    /// Moss streaking down its walls (a material), over `moss_share` of
    /// them (default 0.3), thickest just under its ledges.
    #[serde(default)]
    moss: Option<String>,
    #[serde(default = "third")]
    moss_share: f32,
    /// Soil (a material) on top of every ledge and bulge, where the skin faces up.
    #[serde(default)]
    soil: Option<String>,
    /// A fan of sand (this material) round its foot where it stands in
    /// shallow water: a beach on some sides, a bar running out on others.
    #[serde(default)]
    apron: Option<String>,
    /// Fallen blocks scattered round its foot: structures picked from the
    /// list, `rubble_count` of them, in the tower's own rock (slot "stone").
    #[serde(default)]
    rubble: Vec<String>,
    #[serde(default)]
    rubble_count: u32,
    #[serde(default)]
    ledge_plants: Vec<String>,
    #[serde(default)]
    ledge_vine: Option<String>,
    /// Ledge plants per 100 m of height (2).
    #[serde(default = "two")]
    ledges: f32,
    /// Jointed rock, 0 (smooth lumps, the default) to 1: the outline breaks
    /// into flat faces meeting at corners, and every few metres up (a bed)
    /// the faces step in or out, as a sea stack splits along its joints.
    #[serde(default)]
    joints: f32,
    /// A wave-cut notch round it at the waterline, metres deep.
    #[serde(default)]
    notch: f32,
    /// [bearing, height, width]: a sea arch through it along the bearing,
    /// its opening `height` m over the foot and `width` m across.
    #[serde(default)]
    arch: Option<[f32; 3]>,
    /// A band of this material on its skin from the foot up to
    /// `tideline_height` m over the sea (barnacles, black tar lichen).
    #[serde(default)]
    tideline: Option<String>,
    #[serde(default = "tide_band")]
    tideline_height: f32,
    /// Surface material on its top, where turf would otherwise grow.
    #[serde(default)]
    cap: Option<String>,
    /// Plants on its top: `crown_count` of them picked from the list
    /// (windswept trees on a stack).
    #[serde(default)]
    crown_plants: Vec<String>,
    #[serde(default)]
    crown_count: u32,
    /// A biome for its top, from the world's list (`gh_spruce`): the top
    /// grows that biome's ground, plants and features as if it were a patch
    /// of the mainland, whatever the biome's rules say.
    #[serde(default)]
    biome: Option<String>,
    /// Low plants scattered over its top round the trees (ferns, salal):
    /// `undergrowth_count` of them, by default about one for every 4 m2 of
    /// level top.
    #[serde(default)]
    undergrowth: Vec<String>,
    #[serde(default)]
    undergrowth_count: Option<u32>,
    /// Low wave-cut rocks round its foot, in its own rock and joints: how
    /// many, how high they stand over the sea at most (m), and how far out
    /// they go (m from the middle; default 2.5 radii + 8).
    #[serde(default)]
    skerries: u32,
    #[serde(default = "skerry_height")]
    skerry_height: f32,
    #[serde(default)]
    skerry_reach: Option<f32>,
    #[serde(default)]
    poi: Option<String>,
}

fn tide_band() -> f32 {
    2.5
}
fn skerry_height() -> f32 {
    2.5
}

fn half() -> f32 {
    0.5
}
fn flute() -> f32 {
    0.1
}
fn two() -> f32 {
    2.0
}
fn third() -> f32 {
    0.3
}
fn most() -> f32 {
    0.8
}

/// Every tower so far; the world keeps them to fill the voxels.
#[derive(Default)]
pub struct Towers(pub Vec<Tower>);
/// Plants the chronicle asked for at exact spots.
#[derive(Default)]
pub struct Growths(pub Vec<Growth>);

impl Process for TowerEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let seed = cx.seed;
        let mut msg = self.raise(cx, c, seed);
        if self.skerries > 0 {
            msg += &self.skerry_field(cx, c, seed);
        }
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause: cx.cause });
        }
        msg
    }
}

impl TowerEvent {
    /// Low rocks round the foot: each a small tower of the same rock,
    /// standing up to `skerry_height` m out of the water (or a boulder on
    /// the sand), domed or flat or sloping, broken by the same joints.
    fn skerry_field(&self, cx: &mut EventCtx, c: Vec2, seed: u32) -> String {
        let mut rng = seed ^ 0x736b_6572;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            (rng >> 8) as f32 / 16_777_216.0
        };
        let r0 = self.radius.max(2.0);
        let reach = self.skerry_reach.unwrap_or(r0 * 2.5 + 8.0);
        let sea = cx.f.sea;
        let mut done: Vec<(Vec2, f32)> = Vec::new();
        let mut tries = 0;
        while (done.len() as u32) < self.skerries && tries < self.skerries * 20 + 20 {
            tries += 1;
            // More of them close in, where the stack shed them.
            let a = rnd() * TAU;
            let d = r0 * 1.05 + (reach - r0 * 1.05).max(1.0) * rnd().powf(1.4);
            let p = c + Vec2::new(a.cos(), a.sin()) * d;
            let f = &cx.f;
            if p.x < 4.0 || p.y < 4.0 || p.x > f.size - 4.0 || p.y > f.size - 4.0 {
                continue;
            }
            let ground = f.height(p.x, p.y);
            // In the water or on the beach just above it, not up on the land.
            if ground > sea + 3.0 || ground < sea - 12.0 {
                continue;
            }
            let r = 0.9 + (r0 * 0.3).clamp(0.5, 3.5) * rnd() * (0.4 + 0.6 * rnd());
            if done.iter().any(|(q, rq)| q.distance(p) < (r + rq) * 0.9) {
                continue;
            }
            // Some barely break the surface (reefs the swell foams over),
            // a few stand well out.
            let over = 0.3 + self.skerry_height * rnd().powf(1.7);
            let height = (sea.max(ground) + over - ground).max(0.6);
            let crown = ["dome", "dome", "flat", "slope", "knobs"][(rnd() * 5.0) as usize % 5];
            let stretch = if rnd() < 0.6 { Some([rnd() * 180.0, 1.2 + rnd() * 1.4]) } else { None };
            let small = TowerEvent {
                radius: r,
                height,
                top: None,
                bulge: 0.2,
                flute: 0.05,
                taper: Some(0.15 + 0.3 * rnd()),
                lean: Some(0.0),
                crown: Some(crown.to_string()),
                stretch,
                apron: None,
                rubble: Vec::new(),
                rubble_count: 0,
                ledge_plants: Vec::new(),
                ledge_vine: None,
                arch: None,
                notch: 0.0,
                crown_plants: Vec::new(),
                crown_count: 0,
                undergrowth: Vec::new(),
                biome: None,
                skerries: 0,
                poi: None,
                ..self.clone()
            };
            small.raise(cx, p, hash32(seed ^ 0x5eed ^ (done.len() as u32).wrapping_mul(0x9e37_79b9)));
            done.push((p, r));
        }
        format!(", {} skerries", done.len())
    }

    fn raise(&self, cx: &mut EventCtx, c: Vec2, seed: u32) -> String {
        let cause = cx.cause;
        let rock = self.rock.as_ref().map_or(0, |m| cx.mat(m));
        let mantle = self.mantle.as_ref().map_or(0, |m| cx.mat(m));
        let moss = self.moss.as_ref().map_or(0, |m| cx.mat(m));
        let soil = self.soil.as_ref().map_or(0, |m| cx.mat(m));
        let mut rng = seed ^ 0x7077_6572;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            (rng >> 8) as f32 / 16_777_216.0
        };
        let r0 = self.radius.max(0.8);
        // The foot: the low fifth of the ground in a ring just outside it.
        let mut ring: Vec<f32> = (0..24)
            .map(|k| {
                let a = k as f32 * TAU / 24.0;
                cx.f.height(c.x + a.cos() * r0 * 1.2, c.y + a.sin() * r0 * 1.2)
            })
            .collect();
        ring.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let foot = ring[ring.len() / 5].min(cx.f.height(c.x, c.y));
        // The sand fan at the foot, before the tower rises over it.
        let mut apron_cells = 0;
        if let (Some(m), true) = (self.apron.as_ref(), foot < cx.f.sea + 0.5) {
            let mat = cx.mat(m);
            let sea = cx.f.sea;
            let reach = r0 * (2.0 + 0.7 * rnd()) + 10.0;
            let phase = rnd() * 50.0;
            crate::world::chronicle::geom::each_near(&mut cx.f, c, reach * 1.4, |f, i, d, p| {
                let a = (p.y - c.y).atan2(p.x - c.x);
                // Some sides get a wide beach, some next to none.
                let side = (perlin2(a.cos() * 1.3 + phase, a.sin() * 1.3, seed + 21) * 0.5 + 0.5).clamp(0.0, 1.0);
                let rr = reach * (0.5 + 0.8 * side);
                if d >= rr {
                    return;
                }
                let q = 1.0 - d / rr;
                // A shelf of beach a little above the water, then a slope off
                // into the lagoon.
                let want = sea - 2.6 + 3.7 * smoothstep(0.0, 0.5, q) + perlin2(p.x * 0.15, p.y * 0.15, seed + 22) * 0.35;
                if want > f.h[i] {
                    f.h[i] = want;
                    f.cause[i] = cause;
                    f.surface[i] = mat;
                    apron_cells += 1;
                }
            });
        }
        let base = foot - 2.0;
        let height = match self.top {
            Some(t) => (cx.f.sea + t - foot).max(0.6),
            None => self.height.max(0.6),
        };
        let top = foot + height;
        let cap = match self.crown.as_deref().unwrap_or("any") {
            "any" => ["dome", "dome", "knobs", "knobs", "point", "point", "slope"][(rnd() * 7.0) as usize % 7].to_string(),
            s => s.to_string(),
        };
        let lean = self.lean.unwrap_or(rnd() * height * 0.12);
        let taper = self.taper.unwrap_or(0.25 + 0.3 * rnd()).clamp(0.0, 0.9);
        let lean_dir = {
            let a = rnd() * TAU;
            Vec2::new(a.cos(), a.sin())
        };
        let (sdir, sfac) = match self.stretch {
            Some([b, k]) => {
                let a = b.to_radians();
                (Vec2::new(a.sin(), -a.cos()), k.max(0.2))
            }
            None => (Vec2::X, 1.0),
        };
        // Lumps up the height: knots every so often, eased between.
        let spacing = (r0 * 0.9).clamp(4.0, 12.0);
        let nk = (height / spacing).ceil() as usize + 2;
        let knots: Vec<f32> = (0..nk).map(|_| 1.0 + (0.5 + 0.8 * rnd() - 0.9) * self.bulge * 1.7).collect();
        let cap_h = match cap.as_str() {
            "point" => height * (0.35 + 0.25 * rnd()),
            "dome" => (r0 * (1.3 + 0.9 * rnd())).min(height * 0.6),
            "knobs" => (r0 * (1.0 + 0.6 * rnd())).min(height * 0.5),
            "slope" => (r0 * 0.9).min(height * 0.4),
            _ => 0.0,
        };
        let tilt = if cap == "slope" {
            let a = rnd() * TAU;
            Vec2::new(a.cos(), a.sin()) * (0.35 + 0.5 * rnd())
        } else {
            Vec2::ZERO
        };
        // Joints and beds, from a stream of their own so towers without them
        // come out as they always did. A handful of faces round the outline;
        // the rock is split into beds a few metres thick, and in each bed
        // every face stands a little in or out of the one below.
        let mut jrng = seed ^ 0x6a6f_696e;
        let mut jrnd = || {
            jrng = hash32(jrng.wrapping_add(0x6d2b_79f5));
            (jrng >> 8) as f32 / 16_777_216.0
        };
        let joints = self.joints.clamp(0.0, 1.0);
        let nf = 5 + (jrnd() * 4.0) as usize;
        let phase = jrnd() * TAU;
        let faces: Vec<f32> = (0..nf).map(|k| phase + (k as f32 + (jrnd() - 0.5) * 0.7) * TAU / nf as f32).collect();
        let mut beds: Vec<(f32, Vec<f32>, f32)> = Vec::new();
        if joints > 0.0 {
            let mut yb = base;
            while yb < top + 2.0 {
                yb += 2.5 + 3.5 * jrnd();
                beds.push((yb, (0..nf).map(|_| jrnd() * 2.0 - 1.0).collect(), jrnd() * 2.0 - 1.0));
            }
        }
        // How far a face steps in or out from one bed to the next, as a share
        // of the radius: about a metre whatever the size, so a big stack
        // doesn't grow eaves.
        let bed_step = (1.0 / (r0 * sfac.max(1.0))).min(0.1);
        let sea = cx.f.sea;
        let nrows = ((top + 1.0 - base) / TOWER_DY).ceil() as usize + 2;
        let s3 = seed.wrapping_add(17);
        let mut rows = Vec::with_capacity(nrows);
        let mut off = Vec::with_capacity(nrows);
        let mut reach: f32 = 0.0;
        for i in 0..nrows {
            let y = base + i as f32 * TOWER_DY;
            let h = (y - foot).max(0.0);
            let t = h / height;
            let kf = h / spacing;
            let k0 = (kf as usize).min(nk - 2);
            let u = kf - k0 as f32;
            let u = u * u * (3.0 - 2.0 * u);
            let mut r = r0 * (knots[k0] + (knots[k0 + 1] - knots[k0]) * u);
            // Narrower toward the top, a flared foot, and the cap.
            r *= 1.0 - taper * t.min(1.0).powf(1.3);
            r *= 1.0 + 0.3 * (1.0 - h / (height * 0.15)).max(0.0).powi(2);
            let into_cap = h - (height - cap_h);
            if cap_h > 0.0 && into_cap > 0.0 {
                let q = (into_cap / cap_h).min(1.0);
                r *= match cap.as_str() {
                    "point" => (1.0 - q).powf(0.9),
                    _ => (1.0 - q * q).max(0.0).sqrt(),
                };
            }
            if y > top + 0.01 + tilt.length() * r0 * 1.5 {
                r = 0.0;
            }
            // The notch the waves cut, deepest a little over the still water:
            // the same depth all round, however the outline is stretched.
            let notch = if self.notch > 0.0 { self.notch * (-((y - sea - 0.6) / 1.4).powi(2)).exp() } else { 0.0 };
            let bed = beds.iter().find(|b| y < b.0).or(beds.last());
            off.push(lean_dir * lean * t + Vec2::new(perlin2(y * 0.05, 1.3, seed), perlin2(y * 0.05, 7.9, seed)) * r0 * 0.25);
            let mut row = [0.0f32; TOWER_ANGLES];
            for (k, v) in row.iter_mut().enumerate() {
                let a = k as f32 * TAU / TOWER_ANGLES as f32;
                let (ca, sa) = (a.cos(), a.sin());
                let dir = Vec2::new(ca, sa);
                // Oval: radius along the stretch scaled by `sfac`.
                let along = dir.dot(sdir);
                let across = dir.perp_dot(sdir);
                let oval = 1.0 / ((along / sfac).powi(2) + across * across).sqrt();
                let lobes = perlin3(ca * 1.1, sa * 1.1, y * 0.045, seed) * 0.42 + perlin3(ca * 2.4, sa * 2.4, y * 0.09, seed + 5) * 0.14;
                let grooves = perlin3(ca * r0 * 0.4, sa * r0 * 0.4, y * 0.025, s3) * self.flute;
                // Knobs: near the top the outline breaks into separate lumps.
                let knob = if cap == "knobs" && into_cap > 0.0 {
                    1.0 + (perlin3(ca * 1.6, sa * 1.6, 3.7, s3 + 1) * 1.8).clamp(-0.9, 0.7) * (into_cap / cap_h).min(1.0)
                } else {
                    1.0
                };
                let mut shape = 1.0 + lobes + grooves;
                if let Some((_, jit, whole)) = bed {
                    // The outline as a polygon: the nearest face's plane.
                    let mut poly = f32::MAX;
                    for (fk, fa) in faces.iter().enumerate() {
                        let cs = (a - fa).cos();
                        if cs > 0.05 {
                            poly = poly.min((1.0 + bed_step * jit[fk]) / cs);
                        }
                    }
                    // Clefts where the sea has opened a joint: sharp-sided,
                    // running most of the way up.
                    let cl = perlin3(ca * 2.6, sa * 2.6, y * 0.03, seed + 41);
                    let cleft = 0.2 * smoothstep(0.22, 0.3, cl);
                    let jointed = poly.min(1.6) * 0.92 * (1.0 + lobes * 0.25 + 0.5 * bed_step * whole - cleft);
                    shape += (jointed - shape) * joints;
                }
                let full = r * oval * shape * knob;
                *v = (full - notch).max(full * 0.45).max(0.0);
                reach = reach.max(*v + off[i].length());
            }
            rows.push(row);
        }
        reach += 1.0;
        let x0 = (c.x - reach).floor() as i32;
        let z0 = (c.y - reach).floor() as i32;
        let w = (reach * 2.0).ceil() as i32 + 1;
        let mut tower = Tower {
            c,
            base,
            rows,
            off,
            top,
            tilt,
            reach,
            x0,
            z0,
            w,
            under: vec![f32::NAN; (w * w) as usize],
            rock,
            mantle,
            mantle_share: self.mantle_share.clamp(0.0, 1.0),
            moss,
            moss_share: self.moss_share.clamp(0.0, 1.0),
            soil,
            arch: self.arch.map(|[b, h, wd]| {
                let a = b.to_radians();
                let hw = (wd * 0.5).max(0.5);
                Arch { dir: Vec2::new(a.sin(), -a.cos()), hw, spring: foot + h.max(hw) - hw }
            }),
            tide: self.tideline.as_ref().map_or(0, |m| cx.mat(m)),
            tide_top: sea + self.tideline_height,
        };
        let cap_mat = self.cap.as_ref().map(|m| cx.mat(m));
        let pin = match &self.biome {
            Some(b) => match cx.lib.biome(b) {
                Some(k) => Some(k as u8),
                None => {
                    cx.warnings.push(format!("tower: unknown biome {b}"));
                    None
                }
            },
            None => None,
        };

        // Its top into the heightfield, cell by cell, scanning down.
        let mut all = std::mem::take(&mut cx.state.get::<Towers>().0);
        let prior = &all;
        let prior_under = |x: f32, z: f32| -> Option<f32> { prior.iter().filter_map(|t| t.under_at(x, z)).fold(None, |m: Option<f32>, u| Some(m.map_or(u, |m| m.min(u)))) };
        let mut tops = vec![f32::NAN; (w * w) as usize];
        let f = &mut cx.f;
        let mut cells = 0;
        // Finding where the tower's skin is over each cell -- scanning down
        // half a metre at a time until the shape is hit -- is most of what a
        // tower costs, and each cell's answer stands on its own, so it is
        // done in bands across the machine's cores. The heightfield is
        // written afterwards in one pass, cell by cell in the old order, so
        // the world comes out the same whatever the band split.
        {
            let (size, shape) = (f.size, &tower);
            let threads = crate::world::fields::build_threads();
            let wide = w as usize;
            let rows_per = wide.div_ceil(threads).max(1);
            let start = top + tilt.length() * reach + 1.0;
            std::thread::scope(|sc| {
                for (band, out) in tops.chunks_mut(rows_per * wide).enumerate() {
                    sc.spawn(move || {
                        for (k, slot) in out.iter_mut().enumerate() {
                            let (iz, ix) = ((band * rows_per + k / wide) as i32, (k % wide) as i32);
                            let (x, z) = ((x0 + ix) as f32 + 0.5, (z0 + iz) as f32 + 0.5);
                            if x < 0.0 || z < 0.0 || x >= size || z >= size {
                                continue;
                            }
                            let mut y = start;
                            while y > base {
                                if shape.inside(Vec3::new(x, y, z)) {
                                    // The skin is somewhere in the half metre
                                    // above: find it, so the ground height is
                                    // the rock's own and the voxels over it
                                    // aren't drawn as bare rock above ground.
                                    let (mut lo, mut hi) = (y, y + 0.5);
                                    for _ in 0..5 {
                                        let mid = (lo + hi) * 0.5;
                                        if shape.inside(Vec3::new(x, mid, z)) {
                                            lo = mid;
                                        } else {
                                            hi = mid;
                                        }
                                    }
                                    *slot = lo;
                                    break;
                                }
                                y -= 0.5;
                            }
                        }
                    });
                }
            });
        }
        for iz in 0..w {
            for ix in 0..w {
                let ht = tops[(iz * w + ix) as usize];
                if ht.is_nan() {
                    continue;
                }
                let (x, z) = ((x0 + ix) as f32 + 0.5, (z0 + iz) as f32 + 0.5);
                let i = f.idx(x, z);
                if ht > f.h[i] {
                    let ground = prior_under(x, z).unwrap_or(f.h[i]);
                    tower.under[(iz * w + ix) as usize] = ground.min(f.h[i]);
                    f.h[i] = ht;
                    f.cause[i] = cause;
                    if let Some(m) = cap_mat {
                        f.surface[i] = m;
                    }
                    // The top only: not the walls, nor the shoulders of a
                    // dome running down into the sea.
                    if let Some(b) = pin {
                        if ht > f.sea + 2.0 {
                            f.biome_pin.insert(i as u32, b);
                        }
                    }
                    cells += 1;
                }
            }
        }
        // Cliffs where the top falls away to a neighbour.
        for iz in 1..w - 1 {
            for ix in 1..w - 1 {
                let t0 = tops[(iz * w + ix) as usize];
                if t0.is_nan() {
                    continue;
                }
                let drop = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| {
                        let t = tops[((iz + dz) * w + ix + dx) as usize];
                        if t.is_nan() {
                            f32::MAX
                        } else {
                            t0 - t
                        }
                    })
                    .fold(0.0, f32::max);
                if drop > 2.0 {
                    let (x, z) = ((x0 + ix) as f32 + 0.5, (z0 + iz) as f32 + 0.5);
                    let i = f.idx(x, z);
                    f.cliff[i] = 1.0;
                }
            }
        }
        let mut growths = Vec::new();
        // Plants on the top: cells near the highest, level with their
        // neighbours, a couple of metres apart.
        if (!self.crown_plants.is_empty() && self.crown_count > 0) || !self.undergrowth.is_empty() {
            let tmax = tops.iter().filter(|t| !t.is_nan()).fold(f32::MIN, |m, t| m.max(*t));
            let band = (cap_h * 0.5).max(1.5);
            let mut spots = Vec::new();
            for iz in 1..w - 1 {
                for ix in 1..w - 1 {
                    let k = (iz * w + ix) as usize;
                    let t0 = tops[k];
                    if t0.is_nan() || t0 < tmax - band || tower.under[k].is_nan() || t0 < sea + 2.0 {
                        continue;
                    }
                    let level = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dz)| {
                        let t = tops[((iz + dz) * w + ix + dx) as usize];
                        !t.is_nan() && (t - t0).abs() < 1.0
                    });
                    if level {
                        spots.push(Vec3::new((x0 + ix) as f32 + 0.5, t0, (z0 + iz) as f32 + 0.5));
                    }
                }
            }
            let mut put: Vec<Vec3> = Vec::new();
            let mut tries = 0;
            while put.len() < self.crown_count as usize && !self.crown_plants.is_empty() && !spots.is_empty() && tries < 200 {
                tries += 1;
                let at = spots[(jrnd() * spots.len() as f32) as usize % spots.len()];
                if put.iter().any(|q| Vec2::new(q.x - at.x, q.z - at.z).length() < 2.5) {
                    continue;
                }
                let name = &self.crown_plants[(jrnd() * self.crown_plants.len() as f32) as usize % self.crown_plants.len()];
                let out = (Vec2::new(at.x, at.z) - c).normalize_or_zero();
                growths.push(Growth { plant: name.clone(), at, out, seed: None });
                put.push(at);
            }
            // Undergrowth round the trunks, clear of them.
            if !self.undergrowth.is_empty() {
                let want = self.undergrowth_count.unwrap_or(spots.len() as u32 / 4) as usize;
                let mut low: Vec<Vec3> = Vec::new();
                let mut tries = 0;
                while low.len() < want && !spots.is_empty() && tries < want * 20 + 20 {
                    tries += 1;
                    let at = spots[(jrnd() * spots.len() as f32) as usize % spots.len()];
                    let near = |q: &Vec3, d: f32| Vec2::new(q.x - at.x, q.z - at.z).length() < d;
                    if put.iter().any(|q| near(q, 1.2)) || low.iter().any(|q| near(q, 0.9)) {
                        continue;
                    }
                    let name = &self.undergrowth[(jrnd() * self.undergrowth.len() as f32) as usize % self.undergrowth.len()];
                    let out = (Vec2::new(at.x, at.z) - c).normalize_or_zero();
                    growths.push(Growth { plant: name.clone(), at, out, seed: None });
                    low.push(at);
                }
            }
        }
        let ncrown = growths.len();
        // Ledges: where the outline steps in going up, the bulge below has a
        // flat top sticking out. Pick a few for plants.
        if !self.ledge_plants.is_empty() || self.ledge_vine.is_some() {
            let want = (height / 100.0 * self.ledges).round().max(1.0) as usize;
            let n = tower.rows.len();
            let mut tries = 0;
            while growths.len() - ncrown < want * 2 && tries < 200 {
                tries += 1;
                let i = 4 + (rnd() * (n as f32 - 12.0).max(1.0)) as usize;
                if i + 4 >= n {
                    continue;
                }
                let k = (rnd() * TOWER_ANGLES as f32) as usize % TOWER_ANGLES;
                let (r_here, r_up) = (tower.rows[i][k], tower.rows[i + 4][k]);
                if r_here - r_up < 1.2 || r_up < 0.5 {
                    continue;
                }
                let a = k as f32 * TAU / TOWER_ANGLES as f32;
                let dir = Vec2::new(a.cos(), a.sin());
                let y = tower.base + i as f32 * TOWER_DY;
                if y < cx.f.sea + 3.0 {
                    continue;
                }
                let rr = (r_here + r_up) * 0.5;
                let p = c + tower.off[i] + dir * rr;
                let at = Vec3::new(p.x, y + TOWER_DY, p.y);
                if (growths.len() - ncrown) % 2 == 0 && !self.ledge_plants.is_empty() {
                    let name = &self.ledge_plants[(rnd() * self.ledge_plants.len() as f32) as usize % self.ledge_plants.len()];
                    growths.push(Growth { plant: name.clone(), at, out: dir, seed: None });
                }
                if let Some(v) = &self.ledge_vine {
                    let edge = c + tower.off[i] + dir * (r_here - 0.3);
                    growths.push(Growth {
                        plant: v.clone(),
                        at: Vec3::new(edge.x, y + TOWER_DY, edge.y),
                        out: dir,
                        seed: None,
                    });
                }
            }
        }
        // Rubble round the foot: blocks that fell from the tower.
        let mut nrub = 0;
        if !self.rubble.is_empty() && self.rubble_count > 0 {
            let mut slots = crate::util::FxHashMap::default();
            if let Some(r) = &self.rock {
                slots.insert("stone".to_string(), r.clone());
            }
            let foot_r = tower
                .rows
                .get(((foot - tower.base) / TOWER_DY) as usize + 2)
                .map_or(r0, |row| row.iter().fold(0.0f32, |m, v| m.max(*v)));
            for k in 0..self.rubble_count {
                let name = &self.rubble[(rnd() * self.rubble.len() as f32) as usize % self.rubble.len()];
                let Some(def) = cx.lib.structure(name) else { continue };
                let a = rnd() * TAU;
                let d = foot_r * (1.0 + 0.6 * rnd()) + 1.5 + 4.0 * rnd() * rnd();
                let p = c + Vec2::new(a.cos(), a.sin()) * d;
                if cx.f.height(p.x, p.y) < cx.f.sea - 2.0 {
                    continue;
                }
                cx.place(def, p, rnd() * 360.0, &slots, 0.0, hash32(seed ^ 0x7275_6262 ^ k));
                nrub += 1;
            }
        }
        let nled = growths.len() - ncrown;
        cx.state.get::<Growths>().0.extend(growths);
        all.push(tower);
        cx.state.get::<Towers>().0 = all;
        let crowned = if ncrown > 0 { format!(", {ncrown} on top") } else { String::new() };
        format!("{cap} tower {height:.0} m from {foot:.0} m, r {r0} m ({cells} m2, {nled} ledge plants{crowned}, {apron_cells} m2 apron, {nrub} rubble)")
    }
}
