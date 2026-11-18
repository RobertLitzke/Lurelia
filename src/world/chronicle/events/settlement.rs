use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::{each_near, stamp_path};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{hash32, perlin2, unit};
use glam::Vec2;
use serde::Deserialize;

/// A village or town: `houses` buildings (cycling through `buildings`) on flat
/// dry sites within `radius` m of `at`, each facing the centre, plus `extras`.
/// Paints region `name` over it.
#[derive(Deserialize, Debug)]
pub struct Settlement {
    name: String,
    at: Pos,
    radius: f32,
    houses: u32,
    buildings: Vec<String>,
    #[serde(default)]
    extras: Vec<Extra>,
    #[serde(default)]
    slots: HashMap<String, String>,
    /// Material for paths from each house to the centre.
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Extra {
    pub structure: String,
    #[serde(default = "one")]
    pub count: u32,
    /// centre, river, edge.
    #[serde(default = "centre")]
    pub near: String,
}

fn one() -> u32 {
    1
}
fn centre() -> String {
    "centre".into()
}

/// Each settlement's buildings (indices into the placed list), by name, for
/// `abandon`.
#[derive(Default)]
pub struct Settlements(pub HashMap<String, Vec<usize>>);

impl Process for Settlement {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Settlement { name, at, radius, houses, buildings, extras, slots, path } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let c = cx.pos(at);
        let path_m = path.as_ref().map(|p| cx.mat(p));
        let mut members = Vec::new();
        let mut sites: Vec<Vec2> = Vec::new();
        let defs: Vec<usize> = buildings
            .iter()
            .filter_map(|b| {
                let d = cx.lib.structure(b);
                if d.is_none() {
                    cx.warnings.push(format!("settlement {name}: unknown structure {b}"));
                }
                d
            })
            .collect();
        // Candidate sites: flat dry ground, nearest the centre first.
        let mut cands: Vec<(f32, Vec2)> = Vec::new();
        let mut k = 0u32;
        while cands.len() < 600 && k < 6000 {
            let h0 = hash32(seed ^ k.wrapping_mul(0x2545_f491));
            k += 1;
            let a = unit(h0) * std::f32::consts::TAU;
            let rr = unit(hash32(h0 ^ 3)).sqrt() * radius;
            let p = c + Vec2::new(a.cos(), a.sin()) * rr;
            if site_ok(&cx.f, p, 7.0) {
                cands.push((rr + unit(hash32(h0 ^ 7)) * 12.0, p));
            }
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut want: Vec<usize> = (0..*houses as usize).map(|i| defs.get(i % defs.len().max(1)).copied().unwrap_or(usize::MAX)).collect();
        want.retain(|&d| d != usize::MAX);
        for d in want {
            let Some(&(_, p)) = cands.iter().find(|(_, p)| sites.iter().all(|s| s.distance(*p) > 16.0)) else { break };
            sites.push(p);
            let bearing = crate::util::bearing(c - p).to_degrees();
            if let Some(ix) = cx.place(d, p, bearing, slots, 0.0, seed ^ sites.len() as u32) {
                members.push(ix);
                if let Some(pm) = path_m {
                    stamp_path(&mut cx.f, &[p, c], 1.4, pm);
                }
            }
        }
        for ex in extras {
            let Some(d) = cx.lib.structure(&ex.structure) else {
                cx.warnings.push(format!("settlement {name}: unknown structure {}", ex.structure));
                continue;
            };
            for j in 0..ex.count {
                let p = match ex.near.as_str() {
                    "river" => {
                        // The dry bank nearest the centre beside flowing water.
                        let mut best: Option<(f32, Vec2)> = None;
                        each_near(&mut cx.f, c, radius * 1.5, |f: &mut Fields, i, d, p| {
                            if f.water[i] == 1 {
                                for off in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                                    let q = p + off * 9.0;
                                    if site_ok(f, q, 5.0) && sites.iter().all(|s| s.distance(q) > 12.0) && best.is_none_or(|b| d < b.0) {
                                        best = Some((d, q));
                                    }
                                }
                            }
                        });
                        best.map(|b| b.1)
                    }
                    "edge" => cands.iter().rev().map(|x| x.1).find(|p| sites.iter().all(|s| s.distance(*p) > 12.0)),
                    _ => cands.iter().map(|x| x.1).find(|p| sites.iter().all(|s| s.distance(*p) > 9.0) && p.distance(c) < radius * 0.5),
                };
                let Some(p) = p else {
                    cx.warnings.push(format!("settlement {name}: no site for {} #{j}", ex.structure));
                    continue;
                };
                sites.push(p);
                let bearing = crate::util::bearing(c - p).to_degrees();
                if let Some(ix) = cx.place(d, p, bearing, slots, 0.0, seed ^ (100 + sites.len()) as u32) {
                    members.push(ix);
                }
            }
        }
        let rid = cx.f.region_id(name);
        each_near(&mut cx.f, c, radius * 1.6, |f: &mut Fields, i, d, p| {
            if d < radius * (1.15 + 0.3 * perlin2(p.x * 0.03, p.y * 0.03, seed)) {
                f.region[i] = rid;
            }
        });
        let count = members.len();
        cx.state.get::<Settlements>().0.insert(name.clone(), members);
        cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        format!("{name}: {count} buildings within {radius} m of {:.0},{:.0}", c.x, c.y)
    }
}

/// Flat, dry, free ground in a square `half` m either side of `p`.
fn site_ok(f: &Fields, p: Vec2, half: f32) -> bool {
    if p.x < half || p.y < half || p.x > f.size - half || p.y > f.size - half {
        return false;
    }
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for dz in [-half, 0.0, half] {
        for dx in [-half, 0.0, half] {
            let i = f.idx(p.x + dx, p.y + dz);
            if f.water[i] != 0 || f.occupied[i] {
                return false;
            }
            lo = lo.min(f.h[i]);
            hi = hi.max(f.h[i]);
        }
    }
    lo > f.sea + 1.2 && hi - lo < half * 0.45
}
