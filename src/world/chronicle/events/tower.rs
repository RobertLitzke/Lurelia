use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Growth, Poi, Pos, Tower, TOWER_ANGLES, TOWER_DY};
use crate::world::noise::{hash32, perlin2, perlin3, smoothstep};
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::f32::consts::TAU;

/// A free-standing rock tower drawn as a volume, so it can pinch in, bulge
/// out and overhang like a giant hoodoo or a karst tower: a radius that
/// changes with height in lumps (`bulge`), vertical fluting, a lean, and a
/// top that is a dome, knobs, a slope, a point or flat. Plants named in
/// `ledge_plants` grow on the ledges where a bulge sticks out from under a
/// waist, and `ledge_vine` hangs from them.
#[derive(Deserialize, Debug)]
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
    #[serde(default)]
    poi: Option<String>,
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
        let (seed, cause) = (cx.seed, cx.cause);
        let rock = self.rock.as_ref().map_or(0, |m| cx.mat(m));
        let mantle = self.mantle.as_ref().map_or(0, |m| cx.mat(m));
        let moss = self.moss.as_ref().map_or(0, |m| cx.mat(m));
        let soil = self.soil.as_ref().map_or(0, |m| cx.mat(m));
        let mut rng = seed ^ 0x7077_6572;
        let mut rnd = || {
            rng = hash32(rng.wrapping_add(0x6d2b_79f5));
            (rng >> 8) as f32 / 16_777_216.0
        };
        let r0 = self.radius.max(2.0);
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
            Some(t) => (cx.f.sea + t - foot).max(4.0),
            None => self.height.max(4.0),
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
                *v = (r * oval * (1.0 + lobes + grooves) * knob).max(0.0);
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
                                    *slot = y + 0.25;
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
        // Ledges: where the outline steps in going up, the bulge below has a
        // flat top sticking out. Pick a few for plants.
        let mut growths = Vec::new();
        if !self.ledge_plants.is_empty() || self.ledge_vine.is_some() {
            let want = (height / 100.0 * self.ledges).round().max(1.0) as usize;
            let n = tower.rows.len();
            let mut tries = 0;
            while growths.len() < want * 2 && tries < 200 {
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
                if growths.len() % 2 == 0 && !self.ledge_plants.is_empty() {
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
        let nled = growths.len();
        cx.state.get::<Growths>().0.extend(growths);
        all.push(tower);
        cx.state.get::<Towers>().0 = all;
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        }
        format!("{cap} tower {height:.0} m from {foot:.0} m, r {r0} m ({cells} m2, {nled} ledge plants, {apron_cells} m2 apron, {nrub} rubble)")
    }
}
